"""Split legacy GM shop data into one server-owned JSON file per shop page."""

import argparse
import json
import sqlite3
from collections import defaultdict
from pathlib import Path


def config_rows(path: Path):
    json_path = path.with_suffix(".json")
    if json_path.is_file():
        document = json.loads(json_path.read_text(encoding="utf-8"))
        for row in document.get("rows", []):
            if row.get("value") is not None:
                yield int(row["id"]), row["value"]
        return
    with sqlite3.connect(path) as connection:
        for row_id, encoded in connection.execute("SELECT id, jsonbytes FROM DBObject"):
            decoded = bytes(byte ^ 0x55 for byte in encoded)
            yield int(row_id), json.loads(decoded)


def costs_by_good_id(path: Path):
    costs = {}
    for row_id, value in config_rows(path):
        if not isinstance(value, dict):
            continue
        currencies = value.get("currency", [])
        prices = value.get("price", [])
        rows = []
        for currency, price in zip(currencies, prices):
            if len(currency) < 2 or not price:
                continue
            goods_type, item_id = currency[:2]
            amount = price[0]
            if goods_type > 0 and item_id > 0 and amount > 0:
                rows.append(
                    {"type": goods_type, "itemId": item_id, "amount": amount}
                )
        if rows:
            costs[row_id] = rows
    return costs


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--goods", type=Path, default=Path("rust-server/catalog/data/gm-goods.json"))
    parser.add_argument(
        "--shop-config",
        type=Path,
        default=Path("rust-server/catalog/config/config_shop_goods.db"),
    )
    parser.add_argument("--output", type=Path, default=Path("rust-server/catalog/data/shops"))
    args = parser.parse_args()

    source = json.loads(args.goods.read_text(encoding="utf-8"))
    costs = costs_by_good_id(args.shop_config)
    pages = defaultdict(list)
    for good in source.get("goods", []):
        good_id = int(good["goodId"])
        row = {
            "goodId": good_id,
            "type": int(good["type"]),
            "itemId": int(good["itemId"]),
            "num": max(1, int(good.get("num", 1))),
        }
        if good_id in costs:
            row["costs"] = costs[good_id]
        pages[int(good["shopId"])].append(row)

    args.output.mkdir(parents=True, exist_ok=True)
    for shop_id, goods in sorted(pages.items()):
        path = args.output / f"shop-{shop_id:04d}.json"
        document = {
            "shopId": shop_id,
            "label": f"shop-{shop_id:04d}",
            "goods": sorted(goods, key=lambda row: row["goodId"]),
        }
        path.write_text(json.dumps(document, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")


if __name__ == "__main__":
    main()
