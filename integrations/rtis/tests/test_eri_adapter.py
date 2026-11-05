#!/usr/bin/env python3
"""eri_adapter.py 三路 fallback 单测（mock 子进程，全部离线）。

运行：python3 -I -m unittest discover -s <本目录> -t <本目录> -v
"""

import importlib.util
import json
import os
import subprocess
import sys
import tempfile
import unittest
from unittest import mock

HERE = os.path.dirname(os.path.abspath(__file__))
ADAPTER_PATH = os.path.join(HERE, "..", "eri_adapter.py")

_spec = importlib.util.spec_from_file_location("eri_adapter", ADAPTER_PATH)
adapter = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(adapter)

RP_JSON = {
    "query_id": "erq-test-rp-0001", "task_id": "t1",
    "matched_resources": [
        {"resource_id": "gh:a/tool", "name": "Tool", "canonical_url": "https://github.com/a/tool",
         "description": "d", "resource_type": "github_repo", "category_matches": [],
         "keyword_matches": ["android"], "skill_relationships": [], "workflow_relationships": [],
         "quality": 90.0, "grade": "A", "license_status": "MIT", "maintenance_status": "ACTIVE",
         "adoptable_as_dependency": True, "license_note": None,
         "relevance_score": 0.8, "relevance_reasons": ["keyword hits: [android]"]},
        {"resource_id": "gh:b/list", "name": "List", "canonical_url": "https://github.com/b/list",
         "description": "d", "resource_type": "github_repo", "category_matches": [],
         "keyword_matches": ["security"], "skill_relationships": [], "workflow_relationships": [],
         "quality": 70.0, "grade": "B", "license_status": "UNKNOWN", "maintenance_status": "UNKNOWN",
         "adoptable_as_dependency": False,
         "license_note": "license=UNKNOWN: 仅供参考，不可直接采用为依赖",
         "relevance_score": 0.5, "relevance_reasons": ["keyword hits: [security]"]},
    ],
    "retrieval_stats": {"candidates": 10, "filtered": 8, "returned": 2, "cache_hits": 0},
    "already_covered_by_internal": [], "capability_gaps": [],
    "search_status": "ok", "weights_config": {"keyword": 0.6, "quality": 0.4},
}

LEGACY_REVIEW = {
    "status": "ok", "query_id": "erq-test-legacy-0001", "candidate_count": 3,
    "recommended_resources": ["gh:x/one"],
    "rejected_resources": [{"resource_id": "gh:y/two", "reason": "grade=C"}],
    "already_covered_by_internal": [{"resource_id": "gh:x/one", "covered_by": "skill:recon"}],
    "capability_gaps": [{"resource_id": "gh:y/two", "reason": "未映射"}],
    "detailed_research_required": True, "reason": "存在未映射的任务相关资源，需按需深读",
}

TASK = {"task_id": "t1", "objective": "android app pentest", "keywords": ["android", "security"],
        "profile": {"dims": {}}, "skills": [], "workflows": []}


def is_rp_cmd(cmd) -> bool:
    return any(a == "--db" for a in cmd) and not any(a == "bridge-gate" for a in cmd)


class RerunnableFake:
    """按脚本顺序返回结果：每项为 ('rp'|'legacy', behavior, payload)。"""

    def __init__(self, script):
        self.script = list(script)
        self.calls = []

    def __call__(self, cmd, capture_output=True, text=True, timeout=None):
        kind = "rp" if is_rp_cmd(cmd) else "legacy"
        self.calls.append(kind)
        if not self.script:
            raise AssertionError(f"unexpected subprocess call: {kind}")
        k, behavior, payload = self.script.pop(0)
        assert k == kind, f"expected {k} call, got {kind}"
        resp = mock.Mock(returncode=0, stderr="")
        if behavior == "ok":
            resp.stdout = json.dumps(payload)
            return resp
        if behavior == "rc":
            resp.returncode = 2
            resp.stdout = ""
            return resp
        if behavior == "badjson":
            resp.stdout = "not-json"
            return resp
        if behavior == "timeout":
            raise subprocess.TimeoutExpired(cmd, timeout or 5)
        raise AssertionError(f"unknown behavior {behavior}")


class AdapterFallbackTests(unittest.TestCase):
    def setUp(self):
        self._env = {k: os.environ.pop(k, None) for k in
                     ("RTIS_RP_RESOLVE", "RTIS_RP_DB", "RTIS_ERI_RESOLVER",
                      "RTIS_ERI_ADAPTER_TIMEOUT", "RTIS_ERI_ADAPTER_AUDIT")}
        os.environ["RTIS_ERI_ADAPTER_AUDIT"] = "off"
        os.environ["RTIS_RP_RESOLVE"] = "/usr/bin/true"
        os.environ["RTIS_RP_DB"] = ":memory:"
        os.environ["RTIS_ERI_RESOLVER"] = "/usr/bin/true"

    def tearDown(self):
        for k, v in self._env.items():
            if v is None:
                os.environ.pop(k, None)
            else:
                os.environ[k] = v

    def test_1_rp_success_in_auto_mode(self):
        fake = RerunnableFake([("rp", "ok", RP_JSON)])
        with mock.patch.object(adapter.subprocess, "run", fake):
            env = adapter.bridge_gate(TASK, mode="auto")
        self.assertEqual(env["adapter"]["resolver_used"], "rp")
        r = env["external_resource_review"]
        self.assertEqual(r["status"], "ok")
        self.assertEqual(r["candidate_count"], 2)
        self.assertEqual(r["recommended_resources"], ["gh:a/tool"])
        self.assertEqual([x["resource_id"] for x in r["rejected_resources"]], ["gh:b/list"])
        self.assertFalse(r["detailed_research_required"])
        self.assertEqual(fake.calls, ["rp"])

    def test_2_fallback_to_legacy_on_rp_failure(self):
        fake = RerunnableFake([("rp", "timeout", None), ("rp", "timeout", None),
                               ("legacy", "ok", {"external_resource_review": LEGACY_REVIEW})])
        with mock.patch.object(adapter.subprocess, "run", fake):
            env = adapter.bridge_gate(TASK, mode="auto")
        self.assertEqual(env["adapter"]["resolver_used"], "legacy")
        self.assertEqual(env["external_resource_review"], LEGACY_REVIEW)
        self.assertEqual(fake.calls, ["rp", "rp", "legacy"])

    def test_3_single_retry_then_rp_success(self):
        fake = RerunnableFake([("rp", "timeout", None), ("rp", "ok", RP_JSON)])
        with mock.patch.object(adapter.subprocess, "run", fake):
            env = adapter.bridge_gate(TASK, mode="auto")
        self.assertEqual(env["adapter"]["resolver_used"], "rp")
        self.assertEqual(fake.calls, ["rp", "rp"])

    def test_4_all_fail_returns_error_review(self):
        fake = RerunnableFake([("rp", "timeout", None), ("rp", "rc", None),
                               ("legacy", "badjson", None), ("legacy", "timeout", None)])
        with mock.patch.object(adapter.subprocess, "run", fake):
            env = adapter.bridge_gate(TASK, mode="auto")
        r = env["external_resource_review"]
        self.assertEqual(env["adapter"]["resolver_used"], "error")
        self.assertEqual(r["status"], "error")
        self.assertEqual(set(r.keys()), set(adapter.REVIEW_FIELDS))
        # attempts 按路径计（rp + legacy 各 1 条），子进程调用含重试共 4 次
        self.assertEqual(len(env["adapter"]["attempts"]), 2)
        self.assertEqual(fake.calls, ["rp", "rp", "legacy", "legacy"])

    def test_5_legacy_mode_skips_rp(self):
        fake = RerunnableFake([("legacy", "ok", {"external_resource_review": LEGACY_REVIEW})])
        with mock.patch.object(adapter.subprocess, "run", fake):
            env = adapter.bridge_gate(TASK, mode="legacy")
        self.assertEqual(env["adapter"]["resolver_used"], "legacy")
        self.assertEqual(fake.calls, ["legacy"])

    def test_6_rp_mode_has_no_legacy_fallback(self):
        fake = RerunnableFake([("rp", "timeout", None), ("rp", "rc", None)])
        with mock.patch.object(adapter.subprocess, "run", fake):
            env = adapter.bridge_gate(TASK, mode="rp")
        self.assertEqual(env["external_resource_review"]["status"], "error")
        self.assertEqual(fake.calls, ["rp", "rp"])

    def test_7_review_shape_field_compatible(self):
        r = adapter.review_from_resolve(RP_JSON)
        self.assertEqual(set(r.keys()), set(adapter.REVIEW_FIELDS))
        err = adapter.error_review("x")
        self.assertEqual(set(err.keys()), set(adapter.REVIEW_FIELDS))

    def test_8_derive_keywords_parity_with_legacy(self):
        # 与 eri_p2.bridge_gate 推导一致：keywords + objective + 画像维度值，截 12
        task = {"task_id": "t", "objective": "obj", "keywords": ["k1", "k2"],
                "profile": {"dims": {"d1": {"value": "white-label-landing"},
                                     "d2": {"value": ["v3", "v4"]}}}}
        self.assertEqual(adapter.derive_keywords(task),
                         ["k1", "k2", "obj", "white-label-landing", "v3", "v4"])
        self.assertEqual(len(adapter.derive_keywords({"keywords": list("abcdefghijklmn")})), 12)

    def test_9_missing_rp_binary_falls_back(self):
        os.environ["RTIS_RP_RESOLVE"] = "/nonexistent/rp-resolve"
        fake = RerunnableFake([("legacy", "ok", {"external_resource_review": LEGACY_REVIEW})])
        with mock.patch.object(adapter.subprocess, "run", fake):
            env = adapter.bridge_gate(TASK, mode="auto")
        self.assertEqual(env["adapter"]["resolver_used"], "legacy")

    def test_10_timeout_config_default_and_override(self):
        os.environ.pop("RTIS_ERI_ADAPTER_TIMEOUT", None)
        self.assertEqual(adapter._timeout(), 5.0)
        os.environ["RTIS_ERI_ADAPTER_TIMEOUT"] = "bogus"
        self.assertEqual(adapter._timeout(), 5.0)
        os.environ["RTIS_ERI_ADAPTER_TIMEOUT"] = "2"
        self.assertEqual(adapter._timeout(), 2.0)


if __name__ == "__main__":
    unittest.main(verbosity=2)
