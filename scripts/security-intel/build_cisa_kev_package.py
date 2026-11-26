#!/usr/bin/env python3
"""Build a minimal CISA KEV package from an already checked-out, pinned upstream commit."""
from __future__ import annotations

import argparse
import json
import shutil
import subprocess
from pathlib import Path

UPSTREAM = "https://github.com/cisagov/kev-data"
JSON_NAME = "known_exploited_vulnerabilities.json"


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source-dir", type=Path, required=True, help="Git checkout of cisagov/kev-data")
    parser.add_argument("--output-dir", type=Path, default=Path("target/cisa-kev-package"))
    parser.add_argument("--expected-commit", default="51e0a777743fa06957afbaeb280f8735d292c1be")
    parser.add_argument("--force", action="store_true")
    args = parser.parse_args()

    source = args.source_dir.expanduser().resolve(strict=True)
    output = args.output_dir.expanduser().resolve()
    commit = subprocess.run(
        ["git", "-C", str(source), "rev-parse", "HEAD"],
        check=True, capture_output=True, text=True,
    ).stdout.strip()
    if commit != args.expected_commit:
        raise SystemExit(f"source checkout commit mismatch: expected {args.expected_commit}, got {commit}")
    dirty = subprocess.run(
        ["git", "-C", str(source), "status", "--porcelain"],
        check=True, capture_output=True, text=True,
    ).stdout.strip()
    if dirty:
        raise SystemExit("source checkout is dirty; refusing to package an unpinned working tree")

    source_json = source / JSON_NAME
    feed = json.loads(source_json.read_text(encoding="utf-8"))
    records = feed.get("vulnerabilities")
    if not isinstance(records, list) or not records or feed.get("count") != len(records):
        raise SystemExit("CISA KEV feed count does not match its vulnerabilities array")
    if output.exists() and not args.force:
        raise SystemExit(f"output directory already exists (pass --force to replace): {output}")
    if output.exists():
        shutil.rmtree(output)
    output.mkdir(parents=True)
    shutil.copy2(source_json, output / JSON_NAME)
    readme = (
        "# CISA KEV snapshot package\n\n"
        f"Upstream: {UPSTREAM}\n"
        f"Pinned commit: `{commit}`\n"
        f"Catalog version: `{feed.get('catalogVersion')}`\n"
        f"Date released: `{feed.get('dateReleased')}`\n"
        f"Records: `{len(records)}`\n"
        "License: CC0-1.0\n\n"
        "Only the official KEV JSON snapshot is packaged. No PoC code is included or executed.\n"
    )
    (output / "README.md").write_text(readme, encoding="utf-8")
    print(json.dumps({
        "source_url": UPSTREAM,
        "source_commit": commit,
        "catalog_version": feed.get("catalogVersion"),
        "date_released": feed.get("dateReleased"),
        "records": len(records),
        "json_bytes": (output / JSON_NAME).stat().st_size,
        "output_dir": str(output),
        "execution_performed": False,
    }, ensure_ascii=False, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
