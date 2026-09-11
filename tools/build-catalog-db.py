#!/usr/bin/env python3
"""Build readable, immutable server catalog SQLite database from JSON exports."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import sqlite3
import tempfile
from pathlib import Path
from typing import Any


SCHEMA_VERSION = "2"
IDENTIFIER = re.compile(r"[A-Za-z_][A-Za-z0-9_]*\Z")

# Server shop IDs for equipment-quality pages.  Good IDs live outside the
# client-config ranges; the client patch injects matching config_shop_goods
# entries at runtime.
SSR_EQUIPMENT_SHOP_ID = 18
UR_EQUIPMENT_SHOP_ID = 940
SSR_EQUIPMENT_GOOD_BASE = 1_800_000
UR_EQUIPMENT_GOOD_BASE = 2_800_000
EQUIPMENT_SHOP_PRICE = 200


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    repo_root = Path(__file__).resolve().parents[1]
    parser.add_argument(
        "--catalog-root",
        type=Path,
        default=repo_root / "rust-server" / "catalog",
        help="catalog root containing server-config and data",
    )
    parser.add_argument(
        "--output",
        type=Path,
        default=None,
        help="output server_config.db; defaults to the server directory beside catalog",
    )
    return parser.parse_args()


def quote_identifier(value: str) -> str:
    if not IDENTIFIER.fullmatch(value):
        raise ValueError(f"invalid SQLite identifier: {value}")
    return f'"{value}"'


def json_int(value: Any, default: int = 0) -> int:
    if isinstance(value, bool):
        return int(value)
    if isinstance(value, int):
        return value
    if isinstance(value, str):
        try:
            return int(value)
        except ValueError:
            return default
    return default


def split_reward_key(key: str) -> tuple[int, int]:
    goods_type, item_id = key.split(":", 1)
    return int(goods_type), int(item_id)


def value_kind(value: Any) -> str:
    if value is None:
        return "json"
    if isinstance(value, bool):
        return "bool"
    if isinstance(value, int):
        return "int"
    if isinstance(value, float):
        return "real"
    if isinstance(value, str):
        return "text"
    if isinstance(value, (list, dict)):
        return "json"
    raise ValueError(f"unsupported JSON value type: {type(value).__name__}")


def merge_kind(current: str | None, value: Any) -> str:
    observed = value_kind(value)
    if current is None:
        return observed
    # A NULL must remain distinguishable from an absent key. JSON text preserves
    # that distinction while also handling mixed scalar types safely.
    if observed == "json" or current == "json":
        return "json"
    if observed == current:
        return current
    if {observed, current} == {"int", "real"}:
        return "real"
    return "json"


def sqlite_type(kind: str) -> str:
    if kind in {"int", "bool"}:
        return "INTEGER"
    if kind == "real":
        return "REAL"
    return "TEXT"


def encode_cell(value: Any, kind: str) -> Any:
    if value is None:
        return "null" if kind == "json" else None
    if kind == "json":
        return json.dumps(value, ensure_ascii=False, separators=(",", ":"))
    if kind == "bool":
        return int(value)
    if kind == "int":
        return int(value)
    if kind == "real":
        return float(value)
    if kind == "text":
        return str(value)
    raise ValueError(f"unsupported column kind: {kind}")


def mapped_columns(
    field_order: list[str], field_kinds: dict[str, str | None]
) -> list[tuple[str, str, str]]:
    """Map JSON keys to SQLite columns without losing case-sensitive keys."""
    columns: list[tuple[str, str, str]] = []
    # Reserve both schema id and compatibility alias. The exact JSON key
    # `id` must always map to `value_id`, regardless of row field order.
    used = {"id", "value_id"}
    for field_name in field_order:
        if field_name == "id":
            columns.append((field_name, "value_id", field_kinds[field_name] or "json"))
            continue
        elif IDENTIFIER.fullmatch(field_name):
            base = field_name
        else:
            base = "field_" + "_".join(f"{ord(character):x}" for character in field_name)
        column_name = base
        suffix = 2
        while column_name.casefold() in used:
            column_name = f"{base}__{suffix}"
            suffix += 1
        used.add(column_name.casefold())
        columns.append((field_name, column_name, field_kinds[field_name] or "json"))
    return columns


def create_schema(connection: sqlite3.Connection) -> None:
    connection.executescript(
        """
        PRAGMA foreign_keys = ON;
        PRAGMA journal_mode = DELETE;
        PRAGMA synchronous = FULL;

        CREATE TABLE catalog_meta (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );
        CREATE TABLE catalog_configs (
            config_name TEXT PRIMARY KEY,
            source_file TEXT NOT NULL,
            source_hash TEXT NOT NULL,
            table_name TEXT NOT NULL UNIQUE,
            row_count INTEGER NOT NULL CHECK (row_count >= 0)
        );
        CREATE TABLE catalog_columns (
            config_name TEXT NOT NULL,
            field_name TEXT NOT NULL,
            column_name TEXT NOT NULL,
            value_kind TEXT NOT NULL CHECK (
                value_kind IN ('int', 'real', 'text', 'bool', 'json')
            ),
            ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
            PRIMARY KEY (config_name, field_name),
            UNIQUE (config_name, column_name),
            FOREIGN KEY (config_name) REFERENCES catalog_configs(config_name)
                ON DELETE CASCADE
        );
        CREATE TABLE battle_drop_quantities (
            copy_id INTEGER,
            goods_type INTEGER NOT NULL,
            item_id INTEGER NOT NULL,
            min_num INTEGER NOT NULL,
            max_num INTEGER NOT NULL,
            PRIMARY KEY (copy_id, goods_type, item_id)
        );
        CREATE TABLE server_shop_goods (
            good_id INTEGER NOT NULL,
            shop_id INTEGER NOT NULL,
            goods_type INTEGER NOT NULL,
            item_id INTEGER NOT NULL,
            num INTEGER NOT NULL,
            source_priority INTEGER NOT NULL,
            PRIMARY KEY (good_id, source_priority)
        );
        CREATE TABLE server_shop_good_costs (
            good_id INTEGER NOT NULL,
            source_priority INTEGER NOT NULL,
            ordinal INTEGER NOT NULL,
            goods_type INTEGER NOT NULL,
            item_id INTEGER NOT NULL,
            amount INTEGER NOT NULL,
            PRIMARY KEY (good_id, source_priority, ordinal),
            FOREIGN KEY (good_id, source_priority)
                REFERENCES server_shop_goods(good_id, source_priority)
                ON DELETE CASCADE
        );
        CREATE TABLE server_mails (
            mid INTEGER PRIMARY KEY,
            mail_type TEXT NOT NULL,
            goods_type INTEGER NOT NULL,
            config_id INTEGER NOT NULL,
            num INTEGER NOT NULL,
            subject TEXT NOT NULL,
            content TEXT NOT NULL
        );
        CREATE INDEX idx_catalog_configs_source ON catalog_configs(source_file);
        CREATE INDEX idx_catalog_columns_ordinal
            ON catalog_columns(config_name, ordinal);
        CREATE INDEX idx_shop_goods_shop ON server_shop_goods(shop_id, good_id);
        """
    )
    connection.execute(
        "INSERT INTO catalog_meta(key, value) VALUES ('schema_version', ?)",
        (SCHEMA_VERSION,),
    )


def import_config_rows(connection: sqlite3.Connection, config_dir: Path) -> int:
    imported = 0
    for path in sorted(config_dir.glob("config_*.json")):
        document = json.loads(path.read_text(encoding="utf-8"))
        rows = document.get("rows")
        if not isinstance(rows, list):
            raise ValueError(f"{path} does not contain rows[]")

        config_name = path.stem
        table = quote_identifier(config_name)
        valid_rows: list[tuple[int, dict[str, Any]]] = []
        field_order: list[str] = []
        field_kinds: dict[str, str | None] = {}
        for row in rows:
            row_id = json_int(row.get("id")) if isinstance(row, dict) else 0
            value = row.get("value") if isinstance(row, dict) else None
            if row_id <= 0 or value is None:
                continue
            if not isinstance(value, dict):
                raise ValueError(f"{path} row {row_id} value must be an object")
            if config_name == "config_parameter" and row_id == 203:
                # Client parameter.lua is authoritative for tower entry routing.
                # Some exported snapshots contain stale 40001 here; 30001 is the
                # chapter id consumed by TowerData:GetCopyIdNow().
                value = dict(value)
                value["value"] = 30_001
            valid_rows.append((row_id, value))
            for field_name, field_value in value.items():
                if not isinstance(field_name, str):
                    raise ValueError(f"{path} row {row_id} has non-string field name")
                if field_name not in field_kinds:
                    field_order.append(field_name)
                    field_kinds[field_name] = None
                field_kinds[field_name] = merge_kind(field_kinds[field_name], field_value)

        columns = mapped_columns(field_order, field_kinds)
        column_sql = ", ".join(
            [f'"id" INTEGER PRIMARY KEY CHECK (id > 0)']
            + [f"{quote_identifier(column)} {sqlite_type(kind)}" for _, column, kind in columns]
        )
        connection.execute(f"CREATE TABLE {table} ({column_sql})")
        connection.execute(
            "INSERT INTO catalog_configs"
            "(config_name, source_file, source_hash, table_name, row_count)"
            " VALUES (?, ?, ?, ?, ?)",
            (
                config_name,
                path.name,
                hashlib.sha256(path.read_bytes()).hexdigest(),
                config_name,
                len(valid_rows),
            ),
        )
        for ordinal, (field_name, column_name, kind) in enumerate(columns):
            connection.execute(
                "INSERT INTO catalog_columns"
                "(config_name, field_name, column_name, value_kind, ordinal)"
                " VALUES (?, ?, ?, ?, ?)",
                (config_name, field_name, column_name, kind, ordinal),
            )

        insert_columns = ["id"] + [column for _, column, _ in columns]
        placeholders = ", ".join("?" for _ in insert_columns)
        insert_sql = (
            f"INSERT INTO {table} ({', '.join(quote_identifier(column) for column in insert_columns)})"
            f" VALUES ({placeholders})"
        )
        for row_id, value in valid_rows:
            cells = [row_id]
            for field_name, _, kind in columns:
                cells.append(encode_cell(value.get(field_name), kind))
            connection.execute(insert_sql, cells)
        imported += 1
    return imported


def import_drop_quantities(connection: sqlite3.Connection, catalog_root: Path) -> None:
    path = catalog_root / "battle-drop-quantities.json"
    if not path.is_file():
        return
    document = json.loads(path.read_text(encoding="utf-8"))
    for key, value in document.get("defaultRewards", {}).items():
        goods_type, item_id = split_reward_key(key)
        connection.execute(
            "INSERT INTO battle_drop_quantities"
            "(copy_id, goods_type, item_id, min_num, max_num) VALUES (NULL, ?, ?, ?, ?)",
            (goods_type, item_id, int(value[0]), int(value[1])),
        )
    for copy_id, rewards in document.get("copies", {}).items():
        for key, value in rewards.items():
            goods_type, item_id = split_reward_key(key)
            connection.execute(
                "INSERT INTO battle_drop_quantities"
                "(copy_id, goods_type, item_id, min_num, max_num) VALUES (?, ?, ?, ?, ?)",
                (int(copy_id), goods_type, item_id, int(value[0]), int(value[1])),
            )


def import_shop_goods(connection: sqlite3.Connection, catalog_root: Path) -> None:
    sources: list[tuple[int, Path, dict[str, Any]]] = []
    pages_dir = catalog_root / "data" / "shops"
    for path in sorted(pages_dir.glob("*.json")):
        sources.append((2, path, json.loads(path.read_text(encoding="utf-8"))))
    gm_goods_path = catalog_root / "data" / "gm-goods.json"
    if gm_goods_path.is_file():
        sources.append((1, gm_goods_path, json.loads(gm_goods_path.read_text(encoding="utf-8"))))

    for priority, _, document in sources:
        default_shop_id = json_int(document.get("shopId"))
        for good in document.get("goods", []):
            good_id = json_int(good.get("goodId"))
            shop_id = json_int(good.get("shopId"), default_shop_id)
            goods_type = json_int(good.get("type"))
            item_id = json_int(good.get("itemId"))
            num = max(1, json_int(good.get("num"), 1))
            if min(good_id, shop_id, goods_type, item_id) <= 0:
                continue
            connection.execute(
                "INSERT OR REPLACE INTO server_shop_goods"
                "(good_id, shop_id, goods_type, item_id, num, source_priority)"
                " VALUES (?, ?, ?, ?, ?, ?)",
                (good_id, shop_id, goods_type, item_id, num, priority),
            )
            for ordinal, cost in enumerate(good.get("costs", [])):
                cost_type = json_int(cost.get("type"), json_int(cost.get("goodsType")))
                cost_item = json_int(cost.get("itemId"))
                amount = json_int(cost.get("amount"))
                if min(cost_type, cost_item, amount) <= 0:
                    continue
                connection.execute(
                    "INSERT OR REPLACE INTO server_shop_good_costs"
                    "(good_id, source_priority, ordinal, goods_type, item_id, amount)"
                    " VALUES (?, ?, ?, ?, ?, ?)",
                    (good_id, priority, ordinal, cost_type, cost_item, amount),
                )

    # Make every static SSR/UR equipment purchasable.  Keep explicitly
    # configured goods (for example the existing 300-token featured item) and
    # add deterministic server-owned good IDs for the remaining equipment.
    existing_items: dict[int, set[int]] = {
        SSR_EQUIPMENT_SHOP_ID: set(),
        UR_EQUIPMENT_SHOP_ID: set(),
    }
    for shop_id, item_id in connection.execute(
        "SELECT shop_id, item_id FROM server_shop_goods "
        "WHERE source_priority = 2 AND shop_id IN (?, ?)",
        (SSR_EQUIPMENT_SHOP_ID, UR_EQUIPMENT_SHOP_ID),
    ):
        existing_items[shop_id].add(item_id)

    equip_path = catalog_root / "server-config" / "config_equip.json"
    if not equip_path.is_file():
        return
    equip_document = json.loads(equip_path.read_text(encoding="utf-8"))
    for row in equip_document.get("rows", []):
        equipment_id = json_int(row.get("id"))
        quality = json_int((row.get("value") or {}).get("quality"))
        if quality == 4:
            shop_id = SSR_EQUIPMENT_SHOP_ID
            good_id = SSR_EQUIPMENT_GOOD_BASE + equipment_id
            currency_id = 9
        elif quality == 5:
            shop_id = UR_EQUIPMENT_SHOP_ID
            good_id = UR_EQUIPMENT_GOOD_BASE + equipment_id
            currency_id = 32
        else:
            continue
        if equipment_id <= 0 or equipment_id in existing_items[shop_id]:
            continue

        connection.execute(
            "INSERT OR REPLACE INTO server_shop_goods "
            "(good_id, shop_id, goods_type, item_id, num, source_priority) "
            "VALUES (?, ?, 2, ?, 1, 2)",
            (good_id, shop_id, equipment_id),
        )
        connection.execute(
            "INSERT OR REPLACE INTO server_shop_good_costs "
            "(good_id, source_priority, ordinal, goods_type, item_id, amount) "
            "VALUES (?, 2, 0, 5, ?, ?)",
            (good_id, currency_id, EQUIPMENT_SHOP_PRICE),
        )
        existing_items[shop_id].add(equipment_id)


def import_mails(connection: sqlite3.Connection, catalog_root: Path) -> None:
    path = catalog_root / "data" / "gm-mails.json"
    if not path.is_file():
        return
    document = json.loads(path.read_text(encoding="utf-8"))
    for mail in document.get("mails", []):
        mid = json_int(mail.get("mid"))
        goods_type = json_int(mail.get("goodsType"))
        config_id = json_int(mail.get("configId"))
        if min(mid, goods_type, config_id) <= 0:
            continue
        connection.execute(
            "INSERT OR REPLACE INTO server_mails"
            "(mid, mail_type, goods_type, config_id, num, subject, content)"
            " VALUES (?, ?, ?, ?, ?, ?, ?)",
            (
                mid,
                str(mail.get("type", "")),
                goods_type,
                config_id,
                max(1, json_int(mail.get("num"), 1)),
                str(mail.get("subject", "")),
                str(mail.get("content", "")),
            ),
        )


def build(catalog_root: Path, output: Path) -> None:
    config_dir = catalog_root / "server-config"
    if not config_dir.is_dir():
        raise FileNotFoundError(f"missing server config directory: {config_dir}")
    output.parent.mkdir(parents=True, exist_ok=True)
    fd, temp_name = tempfile.mkstemp(prefix=f"{output.stem}-", suffix=".tmp", dir=output.parent)
    os.close(fd)
    temp_path = Path(temp_name)
    config_count = 0
    try:
        connection = sqlite3.connect(temp_path)
        try:
            create_schema(connection)
            config_count = import_config_rows(connection, config_dir)
            import_drop_quantities(connection, catalog_root)
            import_shop_goods(connection, catalog_root)
            import_mails(connection, catalog_root)
            connection.execute(
                "INSERT INTO catalog_meta(key, value) VALUES ('config_count', ?)",
                (str(config_count),),
            )
            connection.execute(
                "INSERT INTO catalog_meta(key, value) VALUES ('source_root', ?)",
                (str(config_dir),),
            )
            connection.execute("PRAGMA user_version = 2")
            connection.commit()
        finally:
            connection.close()
        os.replace(temp_path, output)
    finally:
        if temp_path.exists():
            temp_path.unlink()
    print(f"built {output} configs={config_count}")


def main() -> None:
    args = parse_args()
    catalog_root = args.catalog_root.resolve()
    default_output = (
        catalog_root.parent / "server_config.db"
        if catalog_root.name == "catalog"
        else catalog_root / "server_config.db"
    )
    output = (args.output or default_output).resolve()
    build(catalog_root, output)


if __name__ == "__main__":
    main()
