import importlib.util
import json
import os
import subprocess
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

SCRIPT = Path(__file__).resolve().parents[1] / "rip_unified_search.py"
SPEC = importlib.util.spec_from_file_location("rip_unified_search_adapter", SCRIPT)
MODULE = importlib.util.module_from_spec(SPEC)
assert SPEC and SPEC.loader
SPEC.loader.exec_module(MODULE)


class UnifiedSearchAdapterTests(unittest.TestCase):
    def test_include_candidates_is_explicitly_forwarded(self):
        root = Path(__file__).resolve().parents[2]
        binary = root / "target/debug/rp-search-unified"
        index = root / "rip-data/dev/asset-governance-search.db"
        if not binary.is_file() or not index.is_file():
            self.skipTest("unified-search binary and disposable index are required")
        fake = subprocess.CompletedProcess([], 0, json.dumps({"hits": [], "channels": {}}), "")
        with patch.object(MODULE.sys, "argv", [str(SCRIPT), "Log4j", "5", "true"]), \
             patch.dict(os.environ, {"RP_SEARCH_UNIFIED_BIN": str(binary), "RP_UNIFIED_SEARCH_INDEX_DB": str(index)}), \
             patch.object(MODULE.subprocess, "run", return_value=fake) as run:
            self.assertEqual(MODULE.main(), 0)
        argv = run.call_args.args[0]
        self.assertEqual(argv[-2:], ["--sqlite-filters", "include-candidates=true"])

    def test_default_does_not_forward_candidate_opt_in(self):
        root = Path(__file__).resolve().parents[2]
        binary = root / "target/debug/rp-search-unified"
        index = root / "rip-data/dev/asset-governance-search.db"
        if not binary.is_file() or not index.is_file():
            self.skipTest("unified-search binary and disposable index are required")
        fake = subprocess.CompletedProcess([], 0, json.dumps({"hits": [], "channels": {}}), "")
        with patch.object(MODULE.sys, "argv", [str(SCRIPT), "Log4j", "5"]), \
             patch.dict(os.environ, {"RP_SEARCH_UNIFIED_BIN": str(binary), "RP_UNIFIED_SEARCH_INDEX_DB": str(index)}), \
             patch.object(MODULE.subprocess, "run", return_value=fake) as run:
            self.assertEqual(MODULE.main(), 0)
        argv = run.call_args.args[0]
        self.assertNotIn("--sqlite-filters", argv)


if __name__ == "__main__":
    unittest.main()
