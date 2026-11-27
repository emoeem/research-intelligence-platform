#!/usr/bin/env python3
"""Regression tests for the authenticated unified search API and v7 index lifecycle."""
from __future__ import annotations

import importlib.util
import json
import os
import sqlite3
import subprocess
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

SERVER = Path(__file__).with_name("rp_server.py")
spec = importlib.util.spec_from_file_location("rp_server_unified_search_test", SERVER)
assert spec and spec.loader
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


class UnifiedSearchApiTests(unittest.TestCase):
    def test_unified_search_invokes_cli_with_default_workspace_and_safe_filters(self) -> None:
        with tempfile.TemporaryDirectory() as temp:
            binary = Path(temp) / "rp-search-unified"
            binary.write_text("#!/bin/sh\nexit 0\n", encoding="utf-8")
            binary.chmod(0o755)
            payload = {
                "hits": [{"store": "sqlite", "id": "kse:example", "title": "Example"}],
                "channels": {"pg": 2, "sqlite": 3},
            }
            with (
                patch.object(module, "_ensure_curated_search_index"),
                patch.object(module, "CURATED_SEARCH_UNIFIED_BIN", str(binary)),
                patch.object(module, "CURATED_SEARCH_INDEX_DB", str(Path(temp) / "search-v7.db")),
                patch.object(module, "REPO_ROOT", temp),
                patch.object(
                    module.subprocess,
                    "run",
                    return_value=subprocess.CompletedProcess([], 0, json.dumps(payload), ""),
                ) as run,
            ):
                result = module.handle_unified_search(
                    None,
                    {
                        "q": ["CVE-2025-12345"],
                        "limit": ["25"],
                        "resource_type": ["nuclei_template"],
                        "vulnerability_id": ["cve-2025-12345"],
                        "canonical_key": ["nuclei:CVE-2025-12345"],
                    },
                )

            self.assertIs(module.API_ROUTES.get("/api/search"), module.handle_unified_search)
            self.assertEqual(result["workspace"], "default")
            self.assertEqual(result["returned"], 1)
            self.assertEqual(result["channel_counts"], {"pg": 2, "sqlite": 3})
            self.assertEqual(result["sqlite_filters"]["vulnerability_id"], "CVE-2025-12345")
            self.assertNotIn("total", result)
            command = run.call_args.args[0]
            self.assertIn("--workspace", command)
            self.assertEqual(command[command.index("--workspace") + 1], "default")
            self.assertIn("--index-db", command)
            self.assertIn("--sqlite-filters", command)
            filters = command[command.index("--sqlite-filters") + 1]
            self.assertIn("resource-type=nuclei_template", filters)
            self.assertIn("vulnerability-id=CVE-2025-12345", filters)
            self.assertIn("canonical-key=nuclei:CVE-2025-12345", filters)

    def test_unified_search_rejects_non_default_workspace_until_acl_mapping_exists(self) -> None:
        with self.assertRaises(module.ApiError):
            module.handle_unified_search(None, {"q": ["test"], "workspace_id": ["ws-private"]})

    def test_unified_search_rejects_invalid_filters_before_subprocess(self) -> None:
        with patch.object(module, "_ensure_curated_search_index"), patch.object(
            module.subprocess, "run"
        ) as run:
            with self.assertRaises(module.ApiError):
                module.handle_unified_search(
                    None, {"q": ["test"], "canonical_key": ["bad,key=inject"]}
                )
        run.assert_not_called()

    def test_unified_search_requires_query_and_limits_query_length(self) -> None:
        with self.assertRaises(module.ApiError):
            module.handle_unified_search(None, {"q": [""]})
        with self.assertRaises(module.ApiError):
            module.handle_unified_search(None, {"q": ["x" * 201]})

    def test_v6_index_is_rebuilt_even_when_source_mtimes_are_older(self) -> None:
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            sources = [root / name for name in ("platform.db", "plugin.db", "catalog.db", "rtis.db", "overlay.json")]
            for source in sources:
                source.write_text("source", encoding="utf-8")
            index = root / "derived" / "search.db"
            index.parent.mkdir()
            conn = sqlite3.connect(index)
            conn.execute("CREATE TABLE search_index_meta (key TEXT PRIMARY KEY, value TEXT NOT NULL)")
            conn.execute("INSERT INTO search_index_meta VALUES ('schema_version', '6')")
            conn.commit()
            conn.close()
            binary = root / "rp-search-index"
            binary.write_text("#!/bin/sh\nexit 0\n", encoding="utf-8")
            binary.chmod(0o755)
            future = max(path.stat().st_mtime for path in sources) + 10
            os.utime(index, (future, future))

            with (
                patch.object(module, "DB_PATH", str(sources[0])),
                patch.object(module, "PLUGIN_DB_PATH", str(sources[1])),
                patch.object(module, "CURATED_SEARCH_CATALOG_DB", str(sources[2])),
                patch.object(module, "CURATED_SEARCH_RTIS_DB", str(sources[3])),
                patch.object(module, "CURATED_SEARCH_OVERLAY", str(sources[4])),
                patch.object(module, "CURATED_SEARCH_INDEX_DB", str(index)),
                patch.object(module, "CURATED_SEARCH_BIN", str(binary)),
                patch.object(module, "REPO_ROOT", temp),
                patch.object(
                    module.subprocess,
                    "run",
                    return_value=subprocess.CompletedProcess([], 0, "{}", ""),
                ) as run,
            ):
                module._ensure_curated_search_index()

            command = run.call_args.args[0]
            self.assertIn("--plugin-db", command)
            self.assertEqual(command[command.index("--plugin-db") + 1], str(sources[1]))
            self.assertIn("--rebuild", command)

    def test_fresh_v7_index_is_reused(self) -> None:
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            sources = [root / name for name in ("platform.db", "plugin.db", "catalog.db", "rtis.db", "overlay.json")]
            for source in sources:
                source.write_text("source", encoding="utf-8")
            index = root / "search.db"
            conn = sqlite3.connect(index)
            conn.execute("CREATE TABLE search_index_meta (key TEXT PRIMARY KEY, value TEXT NOT NULL)")
            conn.execute("INSERT INTO search_index_meta VALUES ('schema_version', '7')")
            conn.commit()
            conn.close()
            future = max(path.stat().st_mtime for path in sources) + 10
            os.utime(index, (future, future))

            with (
                patch.object(module, "DB_PATH", str(sources[0])),
                patch.object(module, "PLUGIN_DB_PATH", str(sources[1])),
                patch.object(module, "CURATED_SEARCH_CATALOG_DB", str(sources[2])),
                patch.object(module, "CURATED_SEARCH_RTIS_DB", str(sources[3])),
                patch.object(module, "CURATED_SEARCH_OVERLAY", str(sources[4])),
                patch.object(module, "CURATED_SEARCH_INDEX_DB", str(index)),
                patch.object(module.subprocess, "run") as run,
            ):
                module._ensure_curated_search_index()
            run.assert_not_called()


if __name__ == "__main__":
    unittest.main(verbosity=2)
