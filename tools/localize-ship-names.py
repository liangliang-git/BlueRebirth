#!/usr/bin/env python3
"""Offline ship-name localization for BlueOath Rebirth.

Workflow:
  1. Copy source DBs into work/source-db.
  2. Decode XOR-0x55 JSON into work/decoded-json.
  3. Translate only JSON field ship_name with local project dictionaries.
  4. Write translated JSON into work/translated-json.
  5. Rebuild SQLite DBs into work/compiled-db.

No network code is used.  The existing translator module is loaded only for
its local dictionaries/functions; its remote translation entry points are not
called.
"""

from __future__ import annotations

import argparse
import base64
import importlib.util
import json
import re
import shutil
import sqlite3
from pathlib import Path
from typing import Any


XOR_KEY = 0x55
TARGET_FILES = [
    "config_interaction_figurte.db",
    "config_plot_episode_ship.db",
    "config_ship_fleet.db",
    "config_ship_info.db",
    "config_ship_model.db",
    "config_ship_position.db",
    "config_ship_show.db",
]
JP_KANA_RE = re.compile(r"[ぁ-ゟァ-ヿ]")

# Japanese glyph variants which can remain after local phonetic conversion.
# These are used only in ship_name values.
JP_TO_CN = {
    "敵": "敌",
    "黒": "黑",
    "駆": "驱",
    "軽": "轻",
    "戦": "战",
    "艦": "舰",
    "砲": "炮",
    "魚": "鱼",
    "雷": "雷",
    "擁": "拥",
    "抱擁": "拥抱",
    "戦鬼": "战鬼",
    "駆逐艦": "驱逐舰",
    "軽巡洋艦": "轻巡洋舰",
    "戦艦": "战舰",
}

# Local corrections for names where generic phonetic conversion is ambiguous.
# These are game-name corrections, not calls to an external translator.
LOCAL_NAME_OVERRIDES = {
    "カッシン": "卡辛",
    "ダウンズ": "唐斯",
    "オバノン": "奥班农",
    "フレッチャー": "弗莱彻",
    "コロラド": "科罗拉多",
    "ブラックキャット": "黑猫",
    "ヘレナイン・ザビーチ": "海伦娜·海滩",
    "聖夜の探求": "圣夜的探索",
    "漆黒の雫": "漆黑之滴",
    "漆黒の花": "漆黑之花",
    "ディヴェルの抱擁": "迪维尔的拥抱",
    "ライザリン·シュタウト": "莱莎琳·斯托特",
    "ライザリン・シュタウト": "莱莎琳·斯托特",
    "ライザ": "莱莎",
    "敵ペネロピ": "敌佩内洛普",
    "敵ブリュッヒャー": "敌布吕歇尔",
    "ムーバー": "移动者",
    "エリート": "精英",
    "アンノウン": "未知",
    "ラジウム": "镭",
    "ウラン": "铀",
    "ポロニウム": "钋",
    "トリウム": "钍",
    "アメリシウム": "镅",
    "プルトニウム": "钚",
}


def xor_bytes(data: bytes) -> bytes:
    return bytes(value ^ XOR_KEY for value in data)


def load_local_translator(repo_root: Path):
    path = repo_root / "tools" / "translate-client.py"
    spec = importlib.util.spec_from_file_location("blueoath_local_translator", path)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"cannot load local translator: {path}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def load_catalog_samples(repo_root: Path) -> dict[tuple[str, str], str]:
    """Read existing local CN catalog samples as an additional offline source."""
    result: dict[tuple[str, str], str] = {}
    for catalog_path in (
        repo_root / "docs" / "config-catalog" / "catalog.json",
        repo_root / "docs" / "config-catalog-deployed" / "catalog.json",
    ):
        if not catalog_path.exists():
            continue
        try:
            catalog = json.loads(catalog_path.read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError):
            continue
        for client in catalog.get("Clients", []):
            if not str(client.get("Id", "")).startswith("cn-"):
                continue
            for table in client.get("Tables", []):
                table_name = table.get("Name")
                for sample in table.get("Samples", []):
                    preview = sample.get("JsonPreview", "")
                    try:
                        payload = json.loads(preview)
                    except (TypeError, json.JSONDecodeError):
                        continue
                    name = payload.get("ship_name")
                    if isinstance(name, str) and name and not JP_KANA_RE.search(name):
                        result.setdefault((str(table_name), str(sample.get("Id", ""))), name)
    return result


def translate_name(name: str, translator: Any) -> str:
    """Translate through local direct rules, then local heuristic rules."""
    translated = name
    for source, target in sorted(LOCAL_NAME_OVERRIDES.items(), key=lambda item: -len(item[0])):
        translated = translated.replace(source, target)
    if translated == name:
        translated = translator.direct_pattern_translation(name)
        if translated is None:
            translated = translator.heuristic_local_translation(name)
        if translated is None:
            translated = name

    # A direct rule can leave Japanese variants; run local cleanup afterward.
    for source, target in sorted(JP_TO_CN.items(), key=lambda item: -len(item[0])):
        translated = translated.replace(source, target)
    translated = translated.replace("ー", "").replace("・", "·")
    return translated


def decode_payload(raw: bytes) -> Any | None:
    try:
        return json.loads(xor_bytes(raw).decode("utf-8"))
    except (UnicodeDecodeError, json.JSONDecodeError):
        return None


def encode_payload(payload: Any) -> bytes:
    text = json.dumps(payload, ensure_ascii=False, separators=(",", ":"))
    return xor_bytes(text.encode("utf-8"))


def dump_json(path: Path, value: Any) -> None:
    path.write_text(json.dumps(value, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")


def process_db(
    source_db: Path,
    source_copy: Path,
    decoded_path: Path,
    translated_path: Path,
    compiled_db: Path,
    translator: Any,
    catalog_samples: dict[tuple[str, str], str],
) -> dict[str, int]:
    shutil.copy2(source_db, source_copy)
    shutil.copy2(source_db, compiled_db)

    decoded_records: list[dict[str, Any]] = []
    translated_records: list[dict[str, Any]] = []
    updates: list[tuple[str, str, str, str]] = []
    stats = {"rows": 0, "valid_json": 0, "ship_names": 0, "jp_before": 0, "translated": 0, "jp_after": 0}

    source_con = sqlite3.connect(source_db)
    try:
        rows = source_con.execute("SELECT id, indexid, jsonbytes FROM DBObject ORDER BY id").fetchall()
    finally:
        source_con.close()

    for row_id, index_id, raw in rows:
        row_id = str(row_id)
        index_id = "" if index_id is None else str(index_id)
        stats["rows"] += 1
        payload = decode_payload(bytes(raw))
        record: dict[str, Any] = {"id": row_id, "indexid": index_id, "json": payload}
        translated_payload = payload
        if payload is None:
            record["raw_base64"] = base64.b64encode(bytes(raw)).decode("ascii")
        else:
            stats["valid_json"] += 1
            if isinstance(payload, dict) and isinstance(payload.get("ship_name"), str):
                stats["ship_names"] += 1
                name = payload["ship_name"]
                if JP_KANA_RE.search(name):
                    stats["jp_before"] += 1
                mapped = translate_name(name, translator)
                # Catalog sample is fallback only; local name rules keep all
                # duplicated ship tables consistent.
                if mapped == name:
                    mapped = catalog_samples.get((source_db.stem, row_id), mapped)
                if mapped != name:
                    translated_payload = dict(payload)
                    translated_payload["ship_name"] = mapped
                    updates.append((mapped, row_id, name, source_db.name))
                    stats["translated"] += 1
                if isinstance(translated_payload, dict) and JP_KANA_RE.search(str(translated_payload.get("ship_name", ""))):
                    stats["jp_after"] += 1
        decoded_records.append(record)
        translated_records.append({"id": row_id, "indexid": index_id, "json": translated_payload})

    dump_json(decoded_path, decoded_records)
    dump_json(translated_path, translated_records)

    compiled_con = sqlite3.connect(compiled_db)
    try:
        for mapped, row_id, _original, _file_name in updates:
            payload = next(record["json"] for record in translated_records if record["id"] == row_id)
            compiled_con.execute(
                "UPDATE DBObject SET jsonbytes = ? WHERE id = ?",
                (sqlite3.Binary(encode_payload(payload)), row_id),
            )
        compiled_con.commit()
        integrity = compiled_con.execute("PRAGMA integrity_check").fetchone()[0]
        if integrity != "ok":
            raise RuntimeError(f"{compiled_db.name}: integrity_check={integrity}")
    finally:
        compiled_con.close()

    return stats


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--client-root", type=Path, default=Path(r"E:\BlueOath Rebirth"))
    parser.add_argument("--repo-root", type=Path, default=Path(__file__).resolve().parents[1])
    parser.add_argument("--work-root", type=Path, default=None)
    args = parser.parse_args()

    config_root = args.client_root / "blueoath" / "blueoath_Data" / "StreamingAssets" / "config"
    work_root = args.work_root or (args.client_root / "舰船名称汉化")
    source_dir = work_root / "source-db"
    decoded_dir = work_root / "decoded-json"
    translated_dir = work_root / "translated-json"
    compiled_dir = work_root / "compiled-db"
    for directory in (source_dir, decoded_dir, translated_dir, compiled_dir):
        directory.mkdir(parents=True, exist_ok=True)

    translator = load_local_translator(args.repo_root)
    catalog_samples = load_catalog_samples(args.repo_root)
    all_stats: dict[str, dict[str, int]] = {}
    for file_name in TARGET_FILES:
        source_db = config_root / file_name
        if not source_db.exists():
            raise FileNotFoundError(source_db)
        all_stats[file_name] = process_db(
            source_db,
            source_dir / file_name,
            decoded_dir / f"{file_name}.json",
            translated_dir / f"{file_name}.json",
            compiled_dir / file_name,
            translator,
            catalog_samples,
        )

    map_rows = []
    for file_name, stats in all_stats.items():
        translated_path = translated_dir / f"{file_name}.json"
        records = json.loads(translated_path.read_text(encoding="utf-8"))
        source_records = json.loads((decoded_dir / f"{file_name}.json").read_text(encoding="utf-8"))
        source_by_id = {record["id"]: record.get("json") for record in source_records}
        for record in records:
            payload = record.get("json")
            if not isinstance(payload, dict) or not isinstance(payload.get("ship_name"), str):
                continue
            original = source_by_id.get(record["id"])
            original_name = original.get("ship_name") if isinstance(original, dict) else None
            map_rows.append({
                "file": file_name,
                "id": record["id"],
                "original_name": original_name,
                "translated_name": payload["ship_name"],
                "changed": original_name != payload["ship_name"],
            })

    dump_json(work_root / "translation-map.json", map_rows)
    dump_json(work_root / "run-summary.json", {"files": all_stats, "catalog_sample_count": len(catalog_samples), "xor_key": XOR_KEY})
    print(json.dumps({"work_root": str(work_root), "files": all_stats, "catalog_sample_count": len(catalog_samples)}, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
