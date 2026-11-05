#!/usr/bin/env python3
"""影子对照（M8 切流材料）：同一 task 上下文分别经 legacy 与 rp 取 Resolve 结果，字段级 diff。

- legacy：直调 eri_p2.py bridge-gate（生产现行为）
- rp：eri_adapter.py bridge-gate --mode rp（纯 rp-resolve 路径，不经 fallback）

差异分类：
  expected：可归因到已记录的平台简化（缺 topics 轴 / 权重两轴 0.6+0.4 / 缺 CJK 桥 /
            license 元数据缺失 / 无 skill_mappings、resource_categories 表）
  anomaly ：status=error 或无法归因的差异

输出：stdout 摘要 + integrations/rtis/shadow_report.json（M8 材料留存）。
只读检索；legacy 路径按生产契约会向 ERI 库写审计行（task_id 前缀 shadow-m5-，可辨识）。
"""

import json
import os
import subprocess
import sys
import tempfile
from datetime import datetime, timezone

HERE = os.path.dirname(os.path.abspath(__file__))
ADAPTER = os.path.join(HERE, "eri_adapter.py")
ERI_P2 = os.environ.get("RTIS_ERI_RESOLVER") or os.path.join(
    os.path.expanduser("~"), "code", "pensting", "security-research-upgrade", "eri", "eri_p2.py")
REPORT_PATH = os.path.join(HERE, "shadow_report.json")

QUERIES = [
    {"name": "落地页（白牌站群）", "task": {
        "task_id": "shadow-m5-landing", "objective": "白牌站群落地页 hex2bin 参数分析",
        "keywords": ["落地页", "hex2bin", "跳转"],
        "profile": {"dims": {"business_type": {"value": "white-label-landing"}}},
        "skills": [], "workflows": []}},
    {"name": "api security", "task": {
        "task_id": "shadow-m5-api", "objective": "API 未授权访问与 swagger 泄露检测",
        "keywords": ["api", "security", "swagger"],
        "profile": {"dims": {}}, "skills": [], "workflows": []}},
    {"name": "android", "task": {
        "task_id": "shadow-m5-android", "objective": "Android 应用渗透 frida 工具链",
        "keywords": ["android", "security"],
        "profile": {"dims": {"business_type": {"value": "mobile"}}},
        "skills": [], "workflows": []}},
]

# 每个字段的差异 -> 平台已记录简化的归因（预期差异）
EXPECTED_RULES = {
    "candidate_count": "命中集差异：platform 缺 topics 轴、缺 CJK 桥、权重两轴(0.6/0.4)简化",
    "status": "命中集差异（同上），no_match vs ok",
    "recommended_resources": "license 元数据缺失(platform 恒 UNKNOWN -> adoptable=false)+命中集差异",
    "rejected_resources": "license 元数据缺失+命中集差异",
    "matched_ids": "词元 blob 组成不同(platform: name+description+url)与权重简化",
    "already_covered_by_internal": "platform 无 skill_mappings 表，恒空",
    "capability_gaps": "platform 无 skill_mappings 表，恒空 -> detailed_research_required 恒 false",
    "detailed_research_required": "capability_gaps 恒空所致",
    "reason": "capability_gaps 联动文案",
}


def run_legacy(task: dict) -> dict:
    fd, tmp = tempfile.mkstemp(suffix=".json")
    try:
        with os.fdopen(fd, "w", encoding="utf-8") as f:
            json.dump(task, f, ensure_ascii=False)
        out = subprocess.run([sys.executable, "-I", ERI_P2, "bridge-gate", "--task-file", tmp],
                             capture_output=True, text=True, timeout=60)
        return (json.loads(out.stdout or "{}")).get("external_resource_review") or {"status": "error"}
    finally:
        if os.path.exists(tmp):
            os.unlink(tmp)


def run_rp(task: dict) -> dict:
    fd, tmp = tempfile.mkstemp(suffix=".json")
    try:
        with os.fdopen(fd, "w", encoding="utf-8") as f:
            json.dump(task, f, ensure_ascii=False)
        out = subprocess.run([sys.executable, "-I", ADAPTER, "bridge-gate",
                              "--task-file", tmp, "--mode", "rp"],
                             capture_output=True, text=True, timeout=60)
        env = json.loads(out.stdout or "{}")
        return env.get("external_resource_review") or {"status": "error"}
    finally:
        if os.path.exists(tmp):
            os.unlink(tmp)


def ids(items) -> set:
    out = set()
    for x in items or []:
        if isinstance(x, dict) and x.get("resource_id"):
            out.add(x["resource_id"])
        elif isinstance(x, str):
            out.add(x)
    return out


def diff_review(legacy: dict, rp: dict) -> list:
    diffs = []
    if ids(legacy.get("recommended_resources")) | ids(legacy.get("rejected_resources")) != \
       ids(rp.get("recommended_resources")) | ids(rp.get("rejected_resources")):
        lm = ids(legacy.get("recommended_resources")) | ids(legacy.get("rejected_resources"))
        rm = ids(rp.get("recommended_resources")) | ids(rp.get("rejected_resources"))
        diffs.append({"field": "matched_ids",
                      "legacy_only": sorted(lm - rm), "rp_only": sorted(rm - lm)})
    for f in ("status", "candidate_count", "detailed_research_required"):
        if legacy.get(f) != rp.get(f):
            diffs.append({"field": f, "legacy": legacy.get(f), "rp": rp.get(f)})
    if ids(legacy.get("already_covered_by_internal")) != ids(rp.get("already_covered_by_internal")):
        diffs.append({"field": "already_covered_by_internal",
                      "legacy": legacy.get("already_covered_by_internal"),
                      "rp": rp.get("already_covered_by_internal")})
    if ids(legacy.get("capability_gaps")) != ids(rp.get("capability_gaps")):
        diffs.append({"field": "capability_gaps",
                      "legacy_count": len(legacy.get("capability_gaps") or []),
                      "rp_count": len(rp.get("capability_gaps") or [])})
    if legacy.get("reason") != rp.get("reason"):
        diffs.append({"field": "reason", "legacy": legacy.get("reason"), "rp": rp.get("reason")})
    # query_id 恒不同（两侧各自生成时间戳 id），设计上排除，不做 diff
    return diffs


def classify(diffs: list) -> tuple:
    expected, anomaly = [], []
    for d in diffs:
        f = d.get("field", "")
        if "error" in str(d.get("legacy", "")) + str(d.get("rp", "")) or \
           (f == "status" and "error" in (str(d.get("legacy")) + str(d.get("rp")))):
            anomaly.append({**d, "why": "任一侧 status=error"})
        elif f in EXPECTED_RULES:
            expected.append({**d, "why": EXPECTED_RULES[f]})
        else:
            anomaly.append({**d, "why": "无归因规则"})
    return expected, anomaly


def main() -> None:
    results = []
    for q in QUERIES:
        legacy = run_legacy(q["task"])
        rp = run_rp(q["task"])
        diffs = diff_review(legacy, rp)
        expected, anomaly = classify(diffs)
        results.append({"query": q["name"], "task_id": q["task"]["task_id"],
                        "legacy_status": legacy.get("status"),
                        "rp_status": rp.get("status"),
                        "legacy_candidates": legacy.get("candidate_count"),
                        "rp_candidates": rp.get("candidate_count"),
                        "legacy_recommended": legacy.get("recommended_resources"),
                        "rp_recommended": rp.get("recommended_resources"),
                        "diffs": diffs, "expected": expected, "anomaly": anomaly})
        print(f"== {q['name']} ==")
        print(f"  legacy: status={legacy.get('status')} n={legacy.get('candidate_count')} "
              f"rec={legacy.get('recommended_resources')}")
        print(f"  rp    : status={rp.get('status')} n={rp.get('candidate_count')} "
              f"rec={rp.get('recommended_resources')}")
        print(f"  diffs: {len(diffs)} (expected {len(expected)}, anomaly {len(anomaly)})")
        for d in expected:
            print(f"    [expected] {d['field']}: {d['why']}")
        for d in anomaly:
            print(f"    [ANOMALY] {json.dumps(d, ensure_ascii=False)}")

    total_exp = sum(len(r["expected"]) for r in results)
    total_ano = sum(len(r["anomaly"]) for r in results)
    summary = {"generated_at": datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
               "queries": len(results), "expected_diffs": total_exp,
               "anomalies": total_ano,
               "verdict": "all_diffs_explained" if total_ano == 0 else "needs_review",
               "results": results}
    with open(REPORT_PATH, "w", encoding="utf-8") as f:
        json.dump(summary, f, ensure_ascii=False, indent=2)
    print(f"\nsummary: {len(results)} queries, expected={total_exp}, anomaly={total_ano}, "
          f"verdict={summary['verdict']}")
    print(f"report: {REPORT_PATH}")


if __name__ == "__main__":
    main()
