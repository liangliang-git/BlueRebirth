#!/usr/bin/env python3
"""Prepare a server-only catalog directory from an exported JSON catalog.

The source directory may contain the complete client snapshot. The prepared
directory receives only config tables referenced by the Rust catalog loader.
Run the exporter first when the source still contains encoded client DB files.
"""

from __future__ import annotations

import argparse
import json
import re
import shutil
import subprocess
import sys
from pathlib import Path


CONFIG_NAME_RE = re.compile(r'config_[A-Za-z0-9_]+\.db')


def loader_tables(repo_root: Path) -> list[str]:
    loader = repo_root / "rust-server" / "crates" / "server" / "src" / "catalog_loader.rs"
    names = sorted(set(CONFIG_NAME_RE.findall(loader.read_text(encoding="utf-8"))))
    if not names:
        raise RuntimeError(f"no config table references found in {loader}")
    return [name.removesuffix(".db") + ".json" for name in names]


def read_catalog(path: Path) -> int:
    document = json.loads(path.read_text(encoding="utf-8"))
    rows = document.get("rows")
    if not isinstance(rows, list):
        raise RuntimeError(f"{path}: rows is not an array")
    return len(rows)


def main() -> int:
    repo_root = Path(__file__).resolve().parents[1]
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--source",
        type=Path,
        default=repo_root / "rust-server" / "catalog" / "config",
        help="directory containing exported config_*.json files",
    )
    parser.add_argument(
        "--output",
        type=Path,
        default=repo_root / "rust-server" / "catalog" / "server-config",
        help="directory receiving the server-only snapshot",
    )
    args = parser.parse_args()
    source = args.source.resolve()
    output = args.output.resolve()
    if not source.is_dir():
        raise RuntimeError(f"source directory does not exist: {source}")
    output.mkdir(parents=True, exist_ok=True)

    copied: list[str] = []
    total_rows = 0
    for name in loader_tables(repo_root):
        source_path = source / name
        if not source_path.is_file():
            continue
        total_rows += read_catalog(source_path)
        shutil.copy2(source_path, output / name)
        copied.append(name)

    if not copied:
        raise RuntimeError(f"no loader tables found under {source}")
    manifest = {
        "format": "blueoath-server-catalog-manifest",
        "version": 1,
        "source": str(source),
        "tables": copied,
        "rows": total_rows,
    }
    (output / "manifest.json").write_text(
        json.dumps(manifest, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
    )
    builder = repo_root / "tools" / "build-catalog-db.py"
    catalog_root = output.parent
    server_db = (
        catalog_root.parent / "server_config.db"
        if catalog_root.name == "catalog"
        else catalog_root / "server_config.db"
    )
    subprocess.run(
        [
            sys.executable,
            str(builder),
            "--catalog-root",
            str(catalog_root),
            "--output",
            str(server_db),
        ],
        check=True,
    )
    print(f"Prepared {len(copied)} tables, {total_rows} rows to {output}")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, ValueError, RuntimeError) as error:
        raise SystemExit(f"error: {error}")
