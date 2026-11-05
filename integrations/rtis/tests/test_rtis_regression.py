#!/usr/bin/env python3
"""rtis.py 回归测试（真实调用，离线）：

1. 自测全量：python3 -I rtis.py test 结果必须与 M5 基线完全一致
   （17 例，13 过 4 败：T2/T4/T11/T13，均为改动前既有失败）。
2. gate 真实调用（default legacy / rp / off 三种模式）：
   校验 external_resource_review 存在性与 9 字段形状。

运行：python3 -I <本文件> -v
"""

import json
import os
import subprocess
import sys
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))
RTIS = os.path.join(os.path.expanduser("~"), ".dsh", "redteam", "rtis", "rtis.py")
TASKS_DIR = os.path.join(os.path.expanduser("~"), ".dsh", "redteam", "rtis", "data", "tasks")

REVIEW_FIELDS = {"status", "query_id", "candidate_count", "recommended_resources",
                 "rejected_resources", "already_covered_by_internal", "capability_gaps",
                 "detailed_research_required", "reason"}
GATE_SUMMARY_FIELDS = {"status", "query_id", "candidate_count", "detailed_research_required"}
BASELINE_FAILED = {"T2 知识优先检索（7 组特征查询）", "T4 知识调用审计（search/returned 落事件）",
                   "T11 角色稿 RTIS 段覆盖", "T13 全链路（画像→门禁→路由→交接→复盘）"}


def rtis_cli(args, extra_env=None, timeout=180):
    env = dict(os.environ)
    if extra_env:
        env.update(extra_env)
    return subprocess.run([sys.executable, "-I", RTIS] + args,
                          capture_output=True, text=True, timeout=timeout, env=env)


class RtisSelftestRegression(unittest.TestCase):
    def test_selftest_matches_pre_m5_baseline(self):
        r = rtis_cli(["test"])
        self.assertEqual(r.returncode, 0, r.stderr[:300])
        d = json.loads(r.stdout)
        self.assertEqual(d["total"], 17)
        failed = {x["name"] for x in d["results"] if not x["ok"]}
        self.assertEqual(d["failed"], 4, f"failing={failed}")
        self.assertEqual(failed, BASELINE_FAILED,
                         "自测结果偏离 M5 基线：失败集必须仍为既有 4 例且不得新增")


class RtisGateRealCall(unittest.TestCase):
    """gate 一次真实调用：确认 external_resource_review 存在且形状不变。"""

    def _gate_and_check(self, env, expect_review):
        r = rtis_cli(["task", "new", "--objective", "M5 regression gate call",
                      "--target", "https://example-landing.test/", "--type", "recon"])
        tid = json.loads(r.stdout)["task_id"]
        task_file = os.path.join(TASKS_DIR, f"{tid}.json")
        try:
            g = rtis_cli(["gate", tid, "--extra", "hex2bin 参数 diff", "--limit", "5"],
                         extra_env=env)
            self.assertEqual(g.returncode, 0, g.stderr[:300])
            gate_out = json.loads(g.stdout)
            self.assertTrue(gate_out["ok"])
            summary = gate_out["gate"].get("external_resource_review")
            with open(task_file, encoding="utf-8") as fh:
                t = json.load(fh)
            review = t.get("external_resource_review")
            if expect_review:
                self.assertIsNotNone(summary, f"mode={env} 缺 gate 摘要")
                self.assertEqual(set(summary.keys()), GATE_SUMMARY_FIELDS)
                self.assertIsNotNone(review, f"mode={env} 缺完整 review")
                self.assertEqual(set(review.keys()), REVIEW_FIELDS)
                self.assertEqual(review["status"], summary["status"])
            else:
                self.assertIsNone(summary)
                self.assertIsNone(review)
        finally:
            if os.path.exists(task_file):
                os.unlink(task_file)

    def test_gate_default_mode_legacy_review_present(self):
        # 默认（不设 RTIS_RESOLVER_MODE）= legacy 现行为，review 来自 eri_p2
        self._gate_and_check({}, expect_review=True)

    def test_gate_rp_mode_review_present(self):
        self._gate_and_check({"RTIS_RESOLVER_MODE": "rp"}, expect_review=True)

    def test_gate_off_mode_review_absent(self):
        self._gate_and_check({"RTIS_RESOLVER_MODE": "off"}, expect_review=False)

    def test_gate_eri_off_review_absent(self):
        self._gate_and_check({"RTIS_ERI": "off"}, expect_review=False)


if __name__ == "__main__":
    unittest.main(verbosity=2)
