#!/usr/bin/env python3
"""SK-1 review API must supply the CLI's version and auditable reason fields."""
from __future__ import annotations

import importlib.util
import subprocess
import unittest
from pathlib import Path
from unittest.mock import patch

SERVER = Path(__file__).with_name("rp_server.py")
spec = importlib.util.spec_from_file_location("rp_server_plugin_review_test", SERVER)
assert spec and spec.loader
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


class PluginReviewApiTests(unittest.TestCase):
    def setUp(self) -> None:
        self.patches = [
            patch.object(module, "PLUGIN_BIN", "/tmp/rp-plugin"),
            patch.object(module, "PLUGIN_BIN_ARGS", []),
            patch.object(module, "PLUGIN_DB_PATH", "/tmp/review.db"),
            patch.object(module.subprocess, "run", return_value=subprocess.CompletedProcess([], 0, "{}", "")),
        ]
        for item in self.patches:
            item.start()

    def tearDown(self) -> None:
        for item in reversed(self.patches):
            item.stop()

    def test_review_cli_receives_exact_version_keys_status_and_reason(self) -> None:
        with patch.object(module.subprocess, "run", wraps=module.subprocess.run) as run:
            module.plugin_write("review", "fixture-plugin", {
                "item_keys": ["templates/a.yaml"], "status": "APPROVED",
                "version": "1.2.3", "reason": "Reviewed upstream source and license evidence",
            })
        argv = run.call_args.args[0]
        self.assertEqual(argv[-10:], [
            "review", "fixture-plugin", "--version", "1.2.3", "--keys", "templates/a.yaml",
            "--status", "APPROVED", "--reason", "Reviewed upstream source and license evidence",
        ])

    def test_missing_or_invalid_version_is_rejected_before_subprocess(self) -> None:
        with patch.object(module.subprocess, "run") as run:
            with self.assertRaises(module.ApiError):
                module.plugin_write("review", "fixture-plugin", {
                    "item_keys": ["templates/a.yaml"], "status": "APPROVED",
                    "reason": "reviewed source evidence",
                })
        run.assert_not_called()

    def test_empty_reason_is_rejected_before_subprocess(self) -> None:
        with patch.object(module.subprocess, "run") as run:
            with self.assertRaises(module.ApiError):
                module.plugin_write("review", "fixture-plugin", {
                    "item_keys": ["templates/a.yaml"], "status": "APPROVED",
                    "version": "1.2.3", "reason": "  ",
                })
        run.assert_not_called()


if __name__ == "__main__":
    unittest.main(verbosity=2)
