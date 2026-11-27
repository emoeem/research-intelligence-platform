#!/usr/bin/env python3
"""Read-only exact-identity and title-collision audit for a v7 search index."""
from __future__ import annotations

import argparse
import json
import re
import sqlite3
from collections import defaultdict
from pathlib import Path


def norm_title(value: str) -> str:
    return re.sub(r"\s+", " ", value.casefold()).strip()


def groups(rows: list[dict], key: str) -> list[dict]:
    by_key: dict[str, list[dict]] = defaultdict(list)
    for row in rows:
        value = row.get(key)
        if value:
            by_key[value].append(row)
    return [
        {"key": value, "count": len(items), "records": items}
        for value, items in sorted(by_key.items(), key=lambda item: (-len(item[1]), item[0]))
        if len(items) > 1
    ]


def main() -> int:
    root = Path(__file__).resolve().parents[2]
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--index", type=Path, required=True)
    parser.add_argument("--output", type=Path, default=root / "test-results/m4-dedup-audit-2026-11-26.json")
    args = parser.parse_args()
    index = args.index.resolve()
    if not index.is_file():
        raise SystemExit(f"index not found: {index}")

    conn = sqlite3.connect(f"file:{index}?mode=ro", uri=True)
    try:
        meta = dict(conn.execute("SELECT key,value FROM search_index_meta"))
        if meta.get("schema_version") != "7":
            raise SystemExit(f"expected index schema v7, got {meta.get('schema_version')!r}")
        raw_rows = conn.execute(
            "SELECT resource_id,name,resource_type,canonical_url,identity_json "
            "FROM search_index_doc ORDER BY resource_id"
        ).fetchall()
    finally:
        conn.close()

    rows = []
    for resource_id, name, resource_type, canonical_url, identity_raw in raw_rows:
        identity = json.loads(identity_raw or "null") or {}
        rows.append(
            {
                "resource_id": resource_id,
                "name": name,
                "resource_type": resource_type,
                "canonical_url": canonical_url,
                "entity_type": identity.get("entity_type"),
                "canonical_key": identity.get("canonical_key"),
                "source_system": identity.get("source_system"),
                "source_entity_id": identity.get("source_entity_id"),
                "source_version": identity.get("source_version"),
                "content_sha256": identity.get("content_sha256"),
                "metadata_sha256": identity.get("metadata_sha256"),
                "vulnerability_ids": identity.get("vulnerability_ids") or [],
            }
        )

    content_dupes = groups(rows, "content_sha256")
    metadata_dupes = groups(rows, "metadata_sha256")
    canonical_rows = []
    for row in rows:
        if row["entity_type"] and row["canonical_key"]:
            canonical_rows.append({
                **row,
                "typed_canonical_key": f"{row['entity_type']}::{row['canonical_key']}",
            })
    canonical_dupes = groups(canonical_rows, "typed_canonical_key")
    title_map: dict[str, list[dict]] = defaultdict(list)
    for row in rows:
        title = norm_title(row["name"])
        if title:
            title_map[title].append(row)
    title_collisions = [
        {
            "normalized_title": title,
            "count": len(items),
            "records": [
                {
                    "resource_id": item["resource_id"],
                    "resource_type": item["resource_type"],
                    "entity_type": item["entity_type"],
                    "canonical_key": item["canonical_key"],
                    "source_system": item["source_system"],
                    "source_entity_id": item["source_entity_id"],
                    "vulnerability_ids": item["vulnerability_ids"],
                }
                for item in items
            ],
            "classification": "title_collision_requires_review_not_a_duplicate",
        }
        for title, items in sorted(title_map.items())
        if len(items) > 1
    ]

    url_map: dict[str, list[dict]] = defaultdict(list)
    for row in rows:
        if row["canonical_url"]:
            url_map[row["canonical_url"]].append(row)
    corpus_url_groups = [
        {
            "canonical_url": url,
            "count": len(items),
            "resource_types": sorted({item["resource_type"] for item in items}),
            "classification": "source_or_corpus_locator_not_a_resource_duplicate_key",
        }
        for url, items in sorted(url_map.items(), key=lambda item: (-len(item[1]), item[0]))
        if len(items) > 1
    ]

    summary = {
        "index_schema_version": 7,
        "document_count": len(rows),
        "identity_source_coverage": sum(bool(row["source_system"] and row["source_entity_id"]) for row in rows),
        "canonical_entity_coverage": sum(bool(row["entity_type"] and row["canonical_key"]) for row in rows),
        "vulnerability_id_coverage": sum(bool(row["vulnerability_ids"]) for row in rows),
        "content_hash_count": sum(bool(row["content_sha256"]) for row in rows),
        "metadata_hash_count": sum(bool(row["metadata_sha256"]) for row in rows),
        "exact_content_duplicate_groups": len(content_dupes),
        "exact_content_duplicate_extra_rows": sum(group["count"] - 1 for group in content_dupes),
        "exact_metadata_duplicate_groups": len(metadata_dupes),
        "exact_metadata_duplicate_extra_rows": sum(group["count"] - 1 for group in metadata_dupes),
        "canonical_entity_duplicate_groups": len(canonical_dupes),
        "title_collision_groups": len(title_collisions),
        "corpus_url_collision_groups": len(corpus_url_groups),
        "automatic_merge_count": 0,
        "content_duplicate_candidates": content_dupes,
        "metadata_duplicate_candidates": metadata_dupes,
        "canonical_entity_groups": canonical_dupes,
        "title_collisions": title_collisions,
        "corpus_url_collisions": corpus_url_groups,
        "policy": "No records or relations are mutated. Titles and corpus URLs are not duplicate evidence.",
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(summary, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({key: value for key, value in summary.items() if key not in {
        "content_duplicate_candidates", "metadata_duplicate_candidates",
        "canonical_entity_groups", "title_collisions", "corpus_url_collisions",
    }}, ensure_ascii=False, indent=2))
    print(f"report: {args.output}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
