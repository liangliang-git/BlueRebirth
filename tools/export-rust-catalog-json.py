#!/usr/bin/env python3
"""Export Blue Oath DBObject catalogs to plain JSON rows.

The Rust server consumes only decoded DBObject rows. JSON keeps those rows
editable and removes SQLite/XOR decoding from deployment while preserving the
same table and row boundaries.
"""

from __future__ import annotations

import argparse
import json
import re
import sqlite3
import tempfile
from pathlib import Path

XOR_KEY = 0x55
CONFIG_NAME_RE = re.compile(r"[\"'](config_[A-Za-z0-9_]+)(?:\.db)?[\"']")


def parse_id(value: object) -> int:
    try:
        return int(value)
    except (TypeError, ValueError):
        return 0


def read_rows(database: Path) -> list[dict[str, object]]:
    with sqlite3.connect(database) as connection:
        try:
            rows = connection.execute(
                "SELECT id, jsonbytes FROM DBObject ORDER BY rowid"
            ).fetchall()
        except sqlite3.Error as error:
            raise RuntimeError(f"{database}: DBObject table is unavailable") from error

    exported: list[dict[str, object]] = []
    for row_id, encoded in rows:
        raw = bytes(encoded or b"")
        decoded = bytes(byte ^ XOR_KEY for byte in raw)
        try:
            value = json.loads(decoded.decode("utf-8"))
        except (UnicodeDecodeError, json.JSONDecodeError):
            value = None
        exported.append({"id": parse_id(row_id), "value": value})
    return exported


def atomic_write(path: Path, document: dict[str, object]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.NamedTemporaryFile(
        "w", encoding="utf-8", dir=path.parent, delete=False, suffix=".tmp"
    ) as handle:
        json.dump(document, handle, ensure_ascii=False, separators=(",", ":"))
        handle.write("\n")
        temporary = Path(handle.name)
    temporary.replace(path)


def used_database_names(repo_root: Path) -> set[str]:
    loader_candidates = (
        repo_root / "rust-server" / "crates" / "server" / "src" / "game_config" / "loader.rs",
        repo_root / "rust-server" / "crates" / "server" / "src" / "catalog_loader.rs",
    )
    loader = next((path for path in loader_candidates if path.is_file()), None)
    if loader is None:
        raise RuntimeError(
            "game config loader is unavailable; checked: "
            + ", ".join(str(path) for path in loader_candidates)
        )
    names = {match.group(1) for match in CONFIG_NAME_RE.finditer(loader.read_text(encoding="utf-8"))}
    if not names:
        raise RuntimeError(f"no config_*.db references found in {loader}")
    return names


def main() -> int:
    repo_root = Path(__file__).resolve().parents[1]
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--source",
        type=Path,
        default=repo_root / "rust-server" / "catalog" / "config",
        help="directory containing config_*.db files",
    )
    parser.add_argument(
        "--output",
        type=Path,
        default=repo_root / "rust-server" / "catalog" / "config",
        help="directory receiving config_*.json files",
    )
    parser.add_argument(
        "--all",
        action="store_true",
        help="export every config_*.db instead of tables referenced by loader",
    )
    args = parser.parse_args()
    source = args.source.resolve()
    output = args.output.resolve()
    if not source.is_dir():
        raise RuntimeError(f"source directory does not exist: {source}")

    databases = sorted(source.glob("config_*.db"))
    if not args.all:
        names = used_database_names(repo_root)
        databases = [database for database in databases if database.stem in names]
    if not databases:
        raise RuntimeError(f"no matching config_*.db files found under {source}")

    exported_names: list[str] = []
    total_rows = 0
    for database in databases:
        rows = read_rows(database)
        document = {
            "format": "blueoath-catalog-json",
            "version": 1,
            "source": database.name,
            "rows": rows,
        }
        target = output / f"{database.stem}.json"
        atomic_write(target, document)
        exported_names.append(target.name)
        total_rows += len(rows)

    atomic_write(
        output / "manifest.json",
        {
            "format": "blueoath-catalog-json-manifest",
            "version": 1,
            "tables": exported_names,
            "rows": total_rows,
        },
    )
    print(f"Exported {len(exported_names)} JSON catalogs, {total_rows} rows to {output}")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except RuntimeError as error:
        raise SystemExit(f"error: {error}")
