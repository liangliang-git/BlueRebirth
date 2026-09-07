#!/usr/bin/env python3
"""Scale combat ship attributes in XOR-encoded config_ship_main.db."""

from __future__ import annotations

import argparse
import datetime as dt
import json
import math
import shutil
import sqlite3
from pathlib import Path


STAT_FIELDS = (
    "hp",
    "hp_levelup",
    "attack",
    "attack_levelup",
    "defense",
    "defense_levelup",
    "torpedo_attack",
    "torpedo_attack_levelup",
    "torpedo_defense",
    "torpedo_defense_levelup",
    "to_air_attack",
    "to_air_attack_levelup",
    "to_torpedo_attack",
    "to_torpedo_attack_levelup",
    "antisubmarine",
    "antisubmarine_levelup",
    "hit",
    "dodge",
    "crit",
    "anti_crit",
    "ship_air_control",
    "ship_air_control_levelup",
    "ship_bomb_attack",
    "ship_bomb_attack_levelup",
    "ship_torpedo_attack",
    "ship_torpedo_attack_levelup",
)


def decode(blob: bytes) -> dict:
    raw = bytes(byte ^ 0x55 for byte in blob)
    value = json.loads(raw)
    if not isinstance(value, dict):
        raise ValueError("config row is not a JSON object")
    return value


def encode(value: dict) -> bytes:
    raw = json.dumps(value, ensure_ascii=False, separators=(",", ":")).encode()
    return bytes(byte ^ 0x55 for byte in raw)


def scaled(value: int | float, multiplier: float) -> int | float:
    result = value * multiplier
    if not math.isfinite(result):
        raise ValueError(f"non-finite scaled value: {value} * {multiplier}")
    if isinstance(value, int) and not isinstance(value, bool):
        return int(round(result))
    return result


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("path", type=Path)
    parser.add_argument("--multiplier", type=float, default=2.0)
    args = parser.parse_args()
    path = args.path.resolve()
    if not path.is_file():
        raise SystemExit(f"missing config: {path}")
    if not math.isfinite(args.multiplier) or args.multiplier <= 0:
        raise SystemExit("multiplier must be a positive finite number")

    stamp = dt.datetime.now().strftime("%Y%m%d-%H%M%S")
    backup = path.with_name(f"{path.name}.bak-{stamp}")
    shutil.copy2(path, backup)

    changed_rows = 0
    changed_values = 0
    skipped_rows = 0
    with sqlite3.connect(path) as connection:
        connection.execute("BEGIN IMMEDIATE")
        rows = connection.execute("SELECT id, jsonbytes FROM DBObject").fetchall()
        for row_id, blob in rows:
            try:
                value = decode(blob)
            except (UnicodeDecodeError, json.JSONDecodeError, ValueError):
                skipped_rows += 1
                continue
            row_changed = False
            for field in STAT_FIELDS:
                current = value.get(field)
                if isinstance(current, (int, float)) and not isinstance(current, bool):
                    value[field] = scaled(current, args.multiplier)
                    row_changed = True
                    changed_values += 1
            if row_changed:
                connection.execute(
                    "UPDATE DBObject SET jsonbytes = ? WHERE id = ?",
                    (encode(value), row_id),
                )
                changed_rows += 1
        connection.commit()

    print(f"updated: {path}")
    print(f"backup:  {backup}")
    print(f"rows:    {changed_rows}")
    print(f"values:  {changed_values}")
    print(f"skipped: {skipped_rows}")
    print(f"factor:  {args.multiplier:g}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
