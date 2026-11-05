# -*- coding: utf-8 -*-
"""evaluation-worker 功能测试：reject/candidate 规则、评分、外置规则集、参数校验。"""

import json
import tempfile
import unittest
from pathlib import Path

import _harness
from _harness import WorkerProc, assert_execute_result, assert_protocol_response

WORKERS_PY = Path(_harness.__file__).resolve().parents[1]
DEFAULT_RULESET = WORKERS_PY / "rules" / "default_ruleset.json"


class EvaluationFunctionalTest(unittest.TestCase):
    def setUp(self):
        self.w = WorkerProc("evaluation", allow_debug=False)
        self.w.send({"jsonrpc": "2.0", "id": "init", "method": "initialize",
                     "params": {"protocol_version": "1"}})
        self.w.recv_for("init")

    def tearDown(self):
        self.w.close()

    def evaluate(self, task: dict, request_id="x") -> dict:
        self.w.send({"jsonrpc": "2.0", "id": request_id, "method": "execute",
                     "params": {"task": task}})
        msg = self.w.recv_for(request_id)
        assert_protocol_response(msg, request_id)
        return assert_execute_result(msg["result"])

    def test_default_ruleset_file_valid(self):
        rules = json.loads(DEFAULT_RULESET.read_text(encoding="utf-8"))
        assert rules["version"] == "1"
        assert any(r["rule_id"] == "missing_source" for r in rules["reject_if"])
        assert any(r["rule_id"] == "has_cve" for r in rules["candidate_if"])

    def test_missing_source_rejected(self):
        result = self.evaluate({"entries": [
            {"id": "n1", "title": "t", "content": "no source here"}]})
        r = result["data"]["results"][0]
        assert r["suggestion"] == "reject"
        assert r["matched_rules"] == ["missing_source"]
        assert r["reasons"][0]["field"] == "source_count"

    def test_cve_entry_is_candidate(self):
        result = self.evaluate({"entries": [
            {"id": "c1", "title": "exploit CVE-2023-1337 works",
             "content": "details", "source": "https://nvd"}]})
        r = result["data"]["results"][0]
        assert r["suggestion"] == "candidate"
        assert "has_cve" in r["matched_rules"]
        assert r["cve_count"] == 1

    def test_evidence_refs_candidate(self):
        result = self.evaluate({"entries": [
            {"id": "e1", "title": "t", "content": "body",
             "source": "https://a",
             "evidence_refs": ["ev1", "ev2"]}]})
        r = result["data"]["results"][0]
        assert r["suggestion"] == "candidate"
        assert "evidence_refs_enough" in r["matched_rules"]
        assert r["evidence_ref_count"] == 2

    def test_source_only_is_review(self):
        result = self.evaluate({"entries": [
            {"id": "m1", "title": "t", "content": "short",
             "source": "https://a"}]})
        r = result["data"]["results"][0]
        assert r["suggestion"] == "review"
        assert r["matched_rules"] == []
        assert 0.2 < r["score"] < 0.6

    def test_summary_counts(self):
        result = self.evaluate({"entries": [
            {"id": "a", "title": "t", "content": "c"},
            {"id": "b", "title": "CVE-2020-1234 found", "content": "c", "source": "s"},
            {"id": "d", "title": "t", "content": "c", "source": "s"}]})
        summary = result["data"]["summary"]
        assert summary == {"candidate": 1, "reject": 1, "review": 1}
        assert result["data"]["ruleset_version"] == "1"

    def test_inline_rules_override(self):
        rules = {
            "version": "test-1",
            "reject_if": [{"rule_id": "no_cve_reject",
                           "when": {"field": "cve_count", "op": "eq", "value": 0},
                           "reason": "must cite CVE"}],
            "candidate_if": [],
            "score_weights": {"source": 0.5, "cve": 0.5,
                              "evidence": 0.0, "content": 0.0},
            "score_thresholds": {"candidate": 0.9, "reject": 0.2},
        }
        result = self.evaluate({"entries": [
            {"id": "x", "title": "t", "content": "c", "source": "https://a"}],
            "rules": rules})
        r = result["data"]["results"][0]
        assert r["suggestion"] == "reject"
        assert r["matched_rules"] == ["no_cve_reject"]
        assert result["data"]["ruleset_version"] == "test-1"

    def test_rules_path_override(self):
        with tempfile.TemporaryDirectory() as tmp:
            p = Path(tmp) / "rules.json"
            p.write_text(json.dumps({
                "version": "file-1",
                "reject_if": [],
                "candidate_if": [{"rule_id": "everything",
                                  "when": {"field": "has_source", "op": "eq",
                                           "value": True},
                                  "reason": "all sourced entries pass"}],
            }), encoding="utf-8")
            result = self.evaluate({"entries": [
                {"id": "y", "title": "t", "content": "c", "source": "s"}],
                "rules_path": str(p)})
            r = result["data"]["results"][0]
            assert r["suggestion"] == "candidate"
            assert result["data"]["ruleset_version"] == "file-1"

    def test_rules_path_missing_is_input_error(self):
        result = self.evaluate({"entries": [{"id": "z", "title": "t"}],
                                "rules_path": "/nonexistent/rules.json"})
        assert result["status"] == "error"
        assert result["error"]["code"] == -32008

    def test_invalid_ruleset_rejected(self):
        result = self.evaluate({"entries": [{"id": "z", "title": "t"}],
                                "rules": {"version": "1",
                                          "reject_if": [{"rule_id": "bad",
                                                         "when": {"field": "nope",
                                                                  "op": "eq",
                                                                  "value": 1},
                                                         "reason": "?"}]}})
        assert result["status"] == "error"
        assert result["error"]["code"] == -32008

    def test_rules_and_rules_path_mutually_exclusive(self):
        result = self.evaluate({"entries": [{"id": "z", "title": "t"}],
                                "rules": {"version": "1"},
                                "rules_path": "/tmp/also.json"})
        assert result["status"] == "error"
        assert result["error"]["code"] == -32602

    def test_entries_must_be_list(self):
        result = self.evaluate({"entries": "not-a-list"})
        assert result["status"] == "error"
        assert result["error"]["code"] == -32602

    def test_signals_exposed(self):
        result = self.evaluate({"entries": [
            {"id": "sig", "title": "About CVE-2024-99999",
             "content": "long body " * 100,
             "source": "https://a", "evidence_refs": ["e1"]}]})
        r = result["data"]["results"][0]
        assert r["source_count"] == 1
        assert r["has_source"] is True
        assert r["cves"] == ["CVE-2024-99999"]
        assert r["evidence_ref_count"] == 1
        assert r["content_chars"] > 400
        assert 0 <= r["score"] <= 1


if __name__ == "__main__":
    unittest.main()
