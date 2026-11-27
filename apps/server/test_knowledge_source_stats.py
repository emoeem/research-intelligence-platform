#!/usr/bin/env python3
"""Regression: plugin knowledge statistics must be scoped to a plugin version."""
import importlib.util
import sqlite3
from pathlib import Path

SERVER = Path(__file__).with_name("rp_server.py")
spec = importlib.util.spec_from_file_location("rp_server_stats_test", SERVER)
assert spec and spec.loader
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)

conn = sqlite3.connect(":memory:")
conn.row_factory = sqlite3.Row
conn.execute("""CREATE TABLE knowledge_source_entry (
    plugin_id TEXT NOT NULL,
    plugin_version TEXT NOT NULL,
    review_status TEXT NOT NULL,
    entry_kind TEXT NOT NULL,
    last_seen_at TEXT NOT NULL
)""")
conn.executemany(
    "INSERT INTO knowledge_source_entry VALUES (?, ?, ?, ?, ?)",
    [
        ("knowledge.example", "0.1.0", "APPROVED", "catalog_entry", "2026-11-24T00:00:00Z"),
        ("knowledge.example", "0.1.0", "DISCOVERED", "catalog_entry", "2026-11-24T00:00:00Z"),
        ("knowledge.example", "0.2.0", "APPROVED", "catalog_entry", "2026-11-26T00:00:00Z"),
    ],
)

result = module.knowledge_source_stats(conn)
assert result["available"] is True
versions = result["stats"]["knowledge.example"]
assert versions["0.1.0"]["review"] == {"APPROVED": 1, "DISCOVERED": 1}
assert versions["0.1.0"]["entry_kinds"] == {"catalog_entry": 2}
assert versions["0.1.0"]["last_seen_at"] == "2026-11-24T00:00:00Z"
assert versions["0.2.0"]["review"] == {"APPROVED": 1}
assert versions["0.2.0"]["entry_kinds"] == {"catalog_entry": 1}
assert versions["0.2.0"]["last_seen_at"] == "2026-11-26T00:00:00Z"
print("PASS: knowledge source statistics are isolated by plugin version")
