#!/usr/bin/env python3
"""Generate a source-review manifest shell for a real 40+ document corpus.

This script does not claim that existing test fixtures are a real corpus.
It records one user-reviewed JSON manifest entry per real source file.
"""
from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path


def sha256(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            h.update(chunk)
    return h.hexdigest()


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--corpus", required=True, type=Path)
    parser.add_argument("--out", required=True, type=Path)
    args = parser.parse_args()
    if not args.corpus.is_dir():
        parser.error("--corpus must be a directory")

    rows = []
    for path in sorted(p for p in args.corpus.rglob("*") if p.is_file()):
        rows.append({
            "sample_id": f"gc-{len(rows) + 1:03d}",
            "path": str(path),
            "sha256": sha256(path),
            "bytes": path.stat().st_size,
            "source": "",
            "license": "",
            "format": path.suffix.lower().lstrip("."),
            "category": "",
            "expected_structure": "",
            "expected_text": "",
            "page_or_block_annotations": [],
            "human_reviewed": False,
        })

    result = {
        "status": "pending_source_review",
        "minimum_samples": 40,
        "sample_count": len(rows),
        "gate_passed": len(rows) >= 40,
        "samples": rows,
    }
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(result, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({"status": "pending_source_review", "sample_count": len(rows),
                      "gate_passed": len(rows) >= 40, "manifest": str(args.out)},
                     ensure_ascii=False))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
