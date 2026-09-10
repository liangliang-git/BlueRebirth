#!/usr/bin/env python3
"""Simplified-Chinese localization for the common language table."""

from __future__ import annotations

import importlib.util
import json
import re
import shutil
import sqlite3
from pathlib import Path
from typing import Any


FILE_NAME = "config_language.db"
FIELD = "content"
JP_KANA_RE = re.compile(r"[ぁ-ゟァ-ヿ]")
TEXT_TOKEN_RE = re.compile(r"[0-9０-９]+|%[sd]|\{[^}]+\}")


def has_kana(value: str) -> bool:
    return bool(JP_KANA_RE.search(value))


def load_helpers(repo_root: Path):
    path = repo_root / "tools" / "localize-ship-names.py"
    spec = importlib.util.spec_from_file_location("blueoath_language_helpers", path)
    if spec is None or spec.loader is None:
        raise RuntimeError(path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def load_cn_reference(reference_root: Path) -> dict[str, Any]:
    path = reference_root / "config_language.json"
    document = json.loads(path.read_text(encoding="utf-8"))
    rows = document.get("Rows", document.get("rows"))
    if not isinstance(rows, list):
        return {}
    result: dict[str, Any] = {}
    for row in rows:
        if not isinstance(row, dict):
            continue
        row_id = row.get("Id", row.get("id"))
        if row_id is not None:
            result[str(row_id)] = row.get("Data", row.get("value"))
    return result


def usable_cn_reference(reference: Any, original: str) -> bool:
    return (
        isinstance(reference, str)
        and bool(reference.strip())
        and reference != original
        and not has_kana(reference)
        and TEXT_TOKEN_RE.findall(reference) == TEXT_TOKEN_RE.findall(original)
    )


def translate_internal(value: str, translator: Any) -> str:
    translated = getattr(translator, "INTERNAL_DIRECT", {}).get(value)
    if translated is None:
        translated = translator.direct_pattern_translation(value)
    if translated is None:
        translated = translator.heuristic_local_translation(value)
    if translated is None:
        translated = value
    if has_kana(translated):
        cleaned = translator.heuristic_local_translation(translated)
        if cleaned is not None:
            translated = cleaned
    return translated.replace("ー", "").replace("・", "·")


def main() -> int:
    repo_root = Path(__file__).resolve().parents[1]
    client_root = Path(r"E:\BlueOath Rebirth")
    current_db = client_root / "blueoath" / "blueoath_Data" / "StreamingAssets" / "config" / FILE_NAME
    cn_reference_root = client_root / "国服config"
    work_root = client_root / "config_language汉化"
    source_dir = work_root / "source-db"
    decoded_dir = work_root / "decoded-json"
    translated_dir = work_root / "translated-json"
    compiled_dir = work_root / "compiled-db"
    backup_dir = work_root / "backup" / "client-config"
    for directory in (source_dir, decoded_dir, translated_dir, compiled_dir, backup_dir):
        directory.mkdir(parents=True, exist_ok=True)

    helpers = load_helpers(repo_root)
    translator = helpers.load_local_translator(repo_root)
    references = load_cn_reference(cn_reference_root)
    source_db = source_dir / FILE_NAME
    if not source_db.exists():
        shutil.copy2(current_db, source_db)
    shutil.copy2(current_db, backup_dir / FILE_NAME)
    shutil.copy2(source_db, compiled_dir / FILE_NAME)

    con = sqlite3.connect(source_db)
    rows = con.execute("select id,indexid,jsonbytes from DBObject order by id").fetchall()
    con.close()
    decoded: list[dict[str, Any]] = []
    translated_rows: list[dict[str, Any]] = []
    changes: list[tuple[str, bytes]] = []
    mappings: list[dict[str, Any]] = []
    stats = {"rows": 0, "valid_json": 0, "text_fields": 0, "jp_before": 0, "changed": 0, "cn_reference": 0, "jp_after": 0}

    for row_id, index_id, raw in rows:
        row_id = str(row_id)
        index_id = "" if index_id is None else str(index_id)
        stats["rows"] += 1
        payload = helpers.decode_payload(bytes(raw))
        if payload is None:
            decoded.append({"id": row_id, "indexid": index_id, "json": None})
            translated_rows.append({"id": row_id, "indexid": index_id, "json": None})
            continue
        stats["valid_json"] += 1
        new_payload = payload
        mapped = payload
        if isinstance(payload, dict) and isinstance(payload.get(FIELD), str):
            original = payload[FIELD]
            stats["text_fields"] += 1
            stats["jp_before"] += int(has_kana(original))
            reference_row = references.get(row_id)
            reference = reference_row.get(FIELD) if isinstance(reference_row, dict) else None
            if usable_cn_reference(reference, original):
                mapped = reference
                source = "cn-reference"
                stats["cn_reference"] += 1
            else:
                mapped = translate_internal(original, translator)
                source = "internal"
            stats["jp_after"] += int(has_kana(mapped))
            if mapped != original:
                new_payload = dict(payload)
                new_payload[FIELD] = mapped
                stats["changed"] += 1
                mappings.append({"id": row_id, "source": source, "original": original, "translated": mapped})
        decoded.append({"id": row_id, "indexid": index_id, "json": payload})
        translated_rows.append({"id": row_id, "indexid": index_id, "json": new_payload})
        if new_payload is not payload:
            changes.append((row_id, helpers.encode_payload(new_payload)))

    helpers.dump_json(decoded_dir / f"{FILE_NAME}.json", decoded)
    helpers.dump_json(translated_dir / f"{FILE_NAME}.json", translated_rows)
    compiled_con = sqlite3.connect(compiled_dir / FILE_NAME)
    for row_id, encoded in changes:
        compiled_con.execute("update DBObject set jsonbytes=? where id=?", (sqlite3.Binary(encoded), row_id))
    compiled_con.commit()
    integrity = compiled_con.execute("pragma integrity_check").fetchone()[0]
    compiled_con.close()
    if integrity != "ok":
        raise RuntimeError(f"{FILE_NAME}: integrity_check={integrity}")
    stats["integrity_ok"] = 1
    helpers.dump_json(work_root / "translation-map.json", mappings)
    helpers.dump_json(work_root / "run-summary.json", {"file": FILE_NAME, "files": {FILE_NAME: stats}, "field": FIELD, "cn_reference_root": str(cn_reference_root), "network_translation": False, "xor_key": 0x55})
    print(json.dumps({"work_root": str(work_root), "files": {FILE_NAME: stats}}, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
