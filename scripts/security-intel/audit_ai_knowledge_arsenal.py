#!/usr/bin/env python3
"""Read-only inventory of RIP's knowledge, weapon, skill and agent data planes."""
from __future__ import annotations

import argparse
import json
import sqlite3
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[2]
DEFAULTS = {
    "platform": ROOT / "migrations/platform.db",
    "plugin": ROOT / "migrations/dev-p6.db",
    "rtis": ROOT / "integrations/rtis/knowledge.db",
    "search": ROOT / "rip-data/derived/curated-poc-search.db",
}


def open_readonly(path: Path) -> sqlite3.Connection:
    if not path.is_file():
        raise FileNotFoundError(path)
    conn = sqlite3.connect(f"file:{path.resolve()}?mode=ro", uri=True)
    conn.row_factory = sqlite3.Row
    conn.execute("PRAGMA query_only = ON")
    return conn


def tables(conn: sqlite3.Connection) -> set[str]:
    return {
        str(row[0])
        for row in conn.execute("SELECT name FROM sqlite_master WHERE type='table'")
    }


def count(conn: sqlite3.Connection, table: str) -> int | None:
    if table not in tables(conn):
        return None
    return int(conn.execute(f'SELECT COUNT(*) FROM "{table}"').fetchone()[0])


def grouped(conn: sqlite3.Connection, sql: str) -> list[dict[str, Any]]:
    return [dict(row) for row in conn.execute(sql)]


def audit_platform(path: Path) -> dict[str, Any]:
    with open_readonly(path) as conn:
        t = tables(conn)
        out: dict[str, Any] = {
            "integrity": conn.execute("PRAGMA integrity_check").fetchone()[0],
            "counts": {
                name: count(conn, name)
                for name in (
                    "knowledge", "resource", "tool", "skill", "workflow",
                    "agent_definition", "agent_run", "workflow_definition",
                )
            },
        }
        if "knowledge" in t:
            out["knowledge_lifecycle"] = grouped(
                conn,
                "SELECT lifecycle, record_kind, COUNT(*) AS n FROM knowledge "
                "GROUP BY lifecycle, record_kind ORDER BY n DESC",
            )
        if "resource" in t:
            out["resource_quality"] = grouped(
                conn,
                "SELECT quality_grade, COUNT(*) AS n FROM resource "
                "GROUP BY quality_grade ORDER BY quality_grade",
            )
        if "tool" in t:
            out["tool_health"] = grouped(
                conn,
                "SELECT install_state, health_state, COUNT(*) AS n FROM tool "
                "GROUP BY install_state, health_state ORDER BY n DESC",
            )
        if "skill" in t:
            out["skill_mapping_coverage"] = dict(
                conn.execute(
                    "SELECT COUNT(*) AS total, "
                    "SUM(CASE WHEN enabled=1 THEN 1 ELSE 0 END) AS enabled, "
                    "SUM(CASE WHEN length(trim(COALESCE(required_tools,'')))>2 THEN 1 ELSE 0 END) AS with_required_tools, "
                    "SUM(CASE WHEN length(trim(COALESCE(knowledge_refs,'')))>2 THEN 1 ELSE 0 END) AS with_knowledge_refs, "
                    "SUM(CASE WHEN length(trim(COALESCE(body_ref,'')))>0 THEN 1 ELSE 0 END) AS with_body_ref "
                    "FROM skill"
                ).fetchone()
            )
        if "workflow" in t:
            out["workflow_coverage"] = dict(
                conn.execute(
                    "SELECT COUNT(*) AS total, "
                    "SUM(CASE WHEN length(trim(COALESCE(phases,'')))>2 THEN 1 ELSE 0 END) AS with_phases, "
                    "SUM(CASE WHEN length(trim(COALESCE(related_skills,'')))>2 THEN 1 ELSE 0 END) AS with_skills, "
                    "SUM(CASE WHEN length(trim(COALESCE(knowledge_queries,'')))>2 THEN 1 ELSE 0 END) AS with_knowledge_queries, "
                    "SUM(CASE WHEN length(trim(COALESCE(stop_conditions,'')))>2 THEN 1 ELSE 0 END) AS with_stop_conditions, "
                    "SUM(CASE WHEN length(trim(COALESCE(requires_json,'')))>2 AND trim(requires_json)<>'[]' THEN 1 ELSE 0 END) AS with_requirements "
                    "FROM workflow"
                ).fetchone()
            )
        return out


def audit_plugin(path: Path) -> dict[str, Any]:
    with open_readonly(path) as conn:
        t = tables(conn)
        if not {"knowledge_source_entry", "plugin"}.issubset(t):
            raise RuntimeError("plugin DB lacks knowledge_source_entry/plugin")
        active = "JOIN plugin p ON p.plugin_id=e.plugin_id AND p.version=e.plugin_version AND p.is_active=1"
        rows = grouped(
            conn,
            "SELECT e.plugin_id, e.plugin_version, e.entry_kind, e.review_status, COUNT(*) AS n "
            f"FROM knowledge_source_entry e {active} "
            "GROUP BY e.plugin_id,e.plugin_version,e.entry_kind,e.review_status "
            "ORDER BY e.plugin_id,e.entry_kind,e.review_status",
        )
        completeness = grouped(
            conn,
            "SELECT e.plugin_id, COUNT(*) AS total, "
            "SUM(CASE WHEN e.body_bytes>0 THEN 1 ELSE 0 END) AS with_body, "
            "SUM(CASE WHEN length(trim(COALESCE(e.description,'')))>0 THEN 1 ELSE 0 END) AS with_description, "
            "SUM(CASE WHEN length(trim(COALESCE(e.canonical_url,'')))>0 THEN 1 ELSE 0 END) AS with_canonical_url, "
            "SUM(CASE WHEN length(trim(COALESCE(e.license_status,'')))>0 THEN 1 ELSE 0 END) AS with_license_status "
            f"FROM knowledge_source_entry e {active} GROUP BY e.plugin_id ORDER BY e.plugin_id",
        )
        return {
            "integrity": conn.execute("PRAGMA integrity_check").fetchone()[0],
            "all_version_rows": count(conn, "knowledge_source_entry"),
            "active_version_distribution": rows,
            "active_source_completeness": completeness,
            "ingest_rows": count(conn, "knowledge_ingest"),
        }


def audit_rtis(path: Path) -> dict[str, Any]:
    with open_readonly(path) as conn:
        if "poc" not in tables(conn):
            raise RuntimeError("RTIS DB lacks poc")
        row = conn.execute(
            "SELECT COUNT(*) AS total, "
            "SUM(CASE WHEN verified=1 THEN 1 ELSE 0 END) AS legacy_verified_flag, "
            "SUM(CASE WHEN cve IS NOT NULL AND trim(cve)<>'' THEN 1 ELSE 0 END) AS with_cve, "
            "SUM(CASE WHEN versions IS NOT NULL AND trim(versions)<>'' THEN 1 ELSE 0 END) AS with_versions, "
            "SUM(CASE WHEN description IS NOT NULL AND trim(description)<>'' THEN 1 ELSE 0 END) AS with_description, "
            "SUM(CASE WHEN source_url IS NOT NULL AND trim(source_url)<>'' THEN 1 ELSE 0 END) AS with_source_url, "
            "SUM(CASE WHEN content IS NOT NULL AND trim(content)<>'' THEN 1 ELSE 0 END) AS with_content "
            "FROM poc"
        ).fetchone()
        return {
            "integrity": conn.execute("PRAGMA integrity_check").fetchone()[0],
            "poc_fields": dict(row),
            "by_source": grouped(conn, "SELECT COALESCE(source,'(null)') AS source, COUNT(*) AS n FROM poc GROUP BY source ORDER BY n DESC"),
            "by_kind": grouped(conn, "SELECT COALESCE(kind,'(null)') AS kind, COUNT(*) AS n FROM poc GROUP BY kind ORDER BY n DESC"),
            "governance_counts": {name: count(conn, name) for name in ("k_meta", "k_negative", "k_relation")},
        }


def audit_search(path: Path) -> dict[str, Any]:
    with open_readonly(path) as conn:
        t = tables(conn)
        return {
            "integrity": conn.execute("PRAGMA integrity_check").fetchone()[0],
            "documents": count(conn, "search_index_doc"),
            "fts_rows": count(conn, "search_index_fts"),
            "by_resource_type": grouped(
                conn,
                "SELECT resource_type, COUNT(*) AS n FROM search_index_doc "
                "GROUP BY resource_type ORDER BY n DESC",
            ) if "search_index_doc" in t else [],
        }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    for name, path in DEFAULTS.items():
        parser.add_argument(f"--{name}-db", type=Path, default=path)
    args = parser.parse_args()
    report = {
        "mode": "read-only",
        "platform": audit_platform(args.platform_db),
        "plugin_sources": audit_plugin(args.plugin_db),
        "rtis_arsenal": audit_rtis(args.rtis_db),
        "search_projection": audit_search(args.search_db),
    }
    print(json.dumps(report, ensure_ascii=False, indent=2, default=str))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
