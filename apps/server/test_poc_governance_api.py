#!/usr/bin/env python3
"""Read-only M3 POC governance API contract tests."""
from __future__ import annotations

import importlib.util
import json
import subprocess
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

SERVER = Path(__file__).with_name("rp_server.py")
spec = importlib.util.spec_from_file_location("rp_server_poc_governance_test", SERVER)
assert spec and spec.loader
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


class PocGovernanceApiTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temp = tempfile.TemporaryDirectory()
        self.overlay = Path(self.temp.name) / "overlay.json"
        self.overlay.write_text(json.dumps({
            "poc_assessment_defaults": {
                "schema_version": 1,
                "evidence_completeness": "not_assessed",
                "artifact_validity": "not_assessed",
                "remediation_status": "unknown",
                "license_status": "unknown",
            },
            "external_entities": [{"code": "fixture-case", "title": "Fixture case group"}],
            "relations": [{"src": "Fixture POC", "dst": "fixture-case", "type": "supports", "confidence": 0.8, "note": "test relation"}],
            "entries": [{
                "knowledge_id": 42,
                "code": "Fixture POC",
                "evidence_level": "K0",
                "lifecycle": "Candidate",
                "missing_evidence": ["raw_request_response"],
                "evidence_refs": [],
            }],
        }), encoding="utf-8")

    def tearDown(self) -> None:
        self.temp.cleanup()

    def test_list_projects_explicit_unassessed_defaults_without_source_bodies(self) -> None:
        with patch.object(module, "CURATED_SEARCH_OVERLAY", str(self.overlay)):
            result = module.handle_curated_poc_governance(None, {})
        self.assertIs(module.API_ROUTES.get("/api/curated-pocs/governance"), module.handle_curated_poc_governance)
        self.assertEqual(result["summary"]["total"], 1)
        self.assertEqual(result["summary"]["relation_count"], 1)
        item = result["items"][0]
        self.assertEqual(item["related_relations"][0]["target_title"], "Fixture case group")
        self.assertEqual(item["assessment"]["artifact_validity"], "not_assessed")
        self.assertEqual(item["assessment"]["remediation_status"], "unknown")
        self.assertNotIn("body", item)
        self.assertNotIn("description", item)

    def test_invalid_poc_id_is_rejected(self) -> None:
        with patch.object(module, "CURATED_SEARCH_OVERLAY", str(self.overlay)):
            with self.assertRaises(module.ApiError):
                module.handle_curated_poc_governance(None, {"poc_id": ["0"]})

    def test_retest_history_is_read_only_metadata_projection(self) -> None:
        binary = Path(self.temp.name) / "rp-verify"
        binary.write_text("#!/bin/sh\nexit 0\n", encoding="utf-8")
        binary.chmod(0o755)
        payload = {"rtis_poc_id": 42, "attempts": [{"result_status": "NOT_TESTED", "evidence": [{"content_hash": "a" * 64}]}]}
        with (
            patch.object(module, "CURATED_SEARCH_OVERLAY", str(self.overlay)),
            patch.object(module, "VERIFY_BIN", str(binary)),
            patch.object(module, "DB_PATH", "/tmp/disposable-platform.db"),
            patch.object(module, "REPO_ROOT", self.temp.name),
            patch.object(module.subprocess, "run", return_value=subprocess.CompletedProcess([], 0, json.dumps(payload), "")) as run,
        ):
            result = module.handle_curated_poc_governance(None, {"poc_id": ["42"]})
        self.assertEqual(result["retest_history"]["rtis_poc_id"], 42)
        self.assertEqual(result["item"]["related_relations"][0]["type"], "supports")
        self.assertEqual(run.call_args.kwargs["env"]["RP_DB_PATH"], "/tmp/disposable-platform.db")
        self.assertEqual(run.call_args.args[0][-2:], ["--poc-id", "42"])


if __name__ == "__main__":
    unittest.main(verbosity=2)
