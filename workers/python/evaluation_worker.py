#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""Evaluation Worker：知识条目评估，输出 candidate/reject/review 建议（M3）。

输入 execute.task：
  {"entries": [entry, ...], "rules": {...}}          内联规则集
  {"entries": [...], "rules_path": "/abs/rules.json"} 外置规则集文件
  {"entries": [...]}                                  缺省用 rules/default_ruleset.json
entry 形状（宽松）：
  {"id": "...", "title": "...", "content": "...",
   "source": "..." | "sources": [...], "evidence_refs": [...] | "evidence_ref_count": n}
输出 data：
  {ruleset_version, results: [{index, entry_id, suggestion, score, signals,
    matched_rules, reasons}], summary: {candidate, reject, review}}

判定顺序：reject_if 命中 -> reject；否则 candidate_if 命中 -> candidate；
否则按 score_thresholds 兜底（>= candidate 阈值 -> candidate，<= reject 阈值 -> reject），
否则 review。建议仅供参考，不直接改写知识状态（M3 约束：AI 输出默认是 Candidate/Suggestion）。
"""

from __future__ import annotations

import json
import re
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent / "runtime"))

from worker_runtime import (WorkerRuntime, WorkerContext, InvalidParams,
                            InputError)

DEFAULT_RULESET_PATH = Path(__file__).resolve().parent / "rules" / "default_ruleset.json"
MAX_ENTRIES = 500

_CVE_RE = re.compile(r"CVE-\d{4}-\d{4,7}", re.IGNORECASE)

FIELDS = ("source_count", "has_source", "cve_count", "has_cve",
          "evidence_ref_count", "content_chars")
OPS = {
    "eq": lambda a, b: a == b,
    "ne": lambda a, b: a != b,
    "gt": lambda a, b: a > b,
    "ge": lambda a, b: a >= b,
    "lt": lambda a, b: a < b,
    "le": lambda a, b: a <= b,
    "contains": lambda a, b: b in a,
}


def _require(cond: bool, msg: str) -> None:
    if not cond:
        raise InputError(f"invalid ruleset: {msg}")


def validate_ruleset(rules: dict) -> None:
    _require(isinstance(rules, dict), "ruleset must be an object")
    _require(isinstance(rules.get("version"), str), "version (string) required")
    for key in ("reject_if", "candidate_if"):
        lst = rules.get(key, [])
        _require(isinstance(lst, list), f"{key} must be a list")
        for rule in lst:
            _require(isinstance(rule, dict), f"{key} entries must be objects")
            _require(isinstance(rule.get("rule_id"), str) and rule["rule_id"],
                     f"{key} rule_id required")
            _require(isinstance(rule.get("reason"), str), "rule reason required")
            when = rule.get("when")
            _require(isinstance(when, dict), "rule.when must be an object")
            _require(when.get("field") in FIELDS,
                     f"when.field must be one of {FIELDS}, got {when.get('field')!r}")
            _require(when.get("op") in OPS,
                     f"when.op must be one of {sorted(OPS)}, got {when.get('op')!r}")
            _require("value" in when, "when.value required")
    weights = rules.get("score_weights", {})
    _require(isinstance(weights, dict), "score_weights must be an object")
    for k, v in weights.items():
        _require(k in ("source", "cve", "evidence", "content"),
                 f"score_weights unknown key {k!r}")
        _require(isinstance(v, (int, float)) and not isinstance(v, bool)
                 and 0 <= v <= 1, f"score_weights.{k} must be number in [0,1]")
    thresholds = rules.get("score_thresholds", {})
    _require(isinstance(thresholds, dict), "score_thresholds must be an object")
    for k in ("candidate", "reject"):
        if k in thresholds:
            v = thresholds[k]
            _require(isinstance(v, (int, float)) and not isinstance(v, bool)
                     and 0 <= v <= 1, f"score_thresholds.{k} must be number in [0,1]")


def normalize_entry(entry: dict, index: int) -> dict:
    if not isinstance(entry, dict):
        raise InvalidParams(f"entries[{index}] must be an object")
    sources = entry.get("sources")
    if not isinstance(sources, list):
        s = entry.get("source")
        sources = [s] if isinstance(s, str) and s.strip() else []
    sources = [s for s in sources if isinstance(s, str) and s.strip()]
    evidence = entry.get("evidence_refs")
    if isinstance(evidence, list):
        ref_count = len(evidence)
    elif isinstance(entry.get("evidence_ref_count"), int):
        ref_count = entry["evidence_ref_count"]
    else:
        ref_count = 0
    title = entry.get("title") or ""
    content = entry.get("content") or ""
    haystack = f"{title}\n{content}"
    cves = sorted({m.upper() for m in _CVE_RE.findall(haystack)})
    return {
        "entry_id": entry.get("id"),
        "source_count": len(sources),
        "has_source": len(sources) > 0,
        "cve_count": len(cves),
        "has_cve": len(cves) > 0,
        "cves": cves,
        "evidence_ref_count": ref_count,
        "content_chars": len(haystack),
    }


def evaluate_entry(signals: dict, rules: dict) -> dict:
    matched_reject: list[dict] = []
    matched_candidate: list[dict] = []
    for key, bucket in (("reject_if", matched_reject), ("candidate_if", matched_candidate)):
        for rule in rules.get(key, []):
            when = rule["when"]
            actual = signals[when["field"]]
            if OPS[when["op"]](actual, when["value"]):
                bucket.append({
                    "rule_id": rule["rule_id"],
                    "reason": rule["reason"],
                    "field": when["field"],
                    "op": when["op"],
                    "expected": when["value"],
                    "actual": actual,
                })

    w = rules.get("score_weights", {})
    tw = rules.get("score_thresholds", {})
    score = (
        w.get("source", 0.3) * min(1.0, signals["source_count"])
        + w.get("cve", 0.2) * min(1.0, signals["cve_count"])
        + w.get("evidence", 0.3) * min(1.0, signals["evidence_ref_count"] / 3.0)
        + w.get("content", 0.2) * min(1.0, signals["content_chars"] / 2000.0)
    )
    score = round(min(1.0, score), 4)

    if matched_reject:
        suggestion = "reject"
    elif matched_candidate:
        suggestion = "candidate"
    elif score >= tw.get("candidate", 0.6):
        suggestion = "candidate"
    elif score <= tw.get("reject", 0.2):
        suggestion = "reject"
    else:
        suggestion = "review"

    matched = matched_reject + matched_candidate
    return {
        "suggestion": suggestion,
        "score": score,
        "matched_rules": [r["rule_id"] for r in matched],
        "reasons": matched,
    }


class EvaluationWorker(WorkerRuntime):
    NAME = "evaluation-worker"
    VERSION = "0.1.0"
    CAPABILITIES = ["evaluate.knowledge", "rules.external-json", "suggest.candidate-reject"]

    def execute(self, ctx: WorkerContext, task: dict) -> dict:
        if "op" in task:  # op 前缀保留给运行时（debug sleep/boom 等）
            return super().execute(ctx, task)
        entries = task.get("entries")
        if not isinstance(entries, list):
            raise InvalidParams("task.entries (list) is required")
        if len(entries) > MAX_ENTRIES:
            raise InputError(f"entries exceeds {MAX_ENTRIES} items: {len(entries)}")

        rules = self._load_rules(task)
        ctx.check_cancelled()

        results = []
        summary = {"candidate": 0, "reject": 0, "review": 0}
        for i, raw in enumerate(entries):
            ctx.check_cancelled()
            signals = normalize_entry(raw, i)
            verdict = evaluate_entry(signals, rules)
            entry_id = raw.get("id") if isinstance(raw, dict) else None
            results.append({
                "index": i,
                "entry_id": entry_id,
                **signals_minus(signals),
                **verdict,
            })
            summary[verdict["suggestion"]] += 1

        return {
            "ruleset_version": rules["version"],
            "results": results,
            "summary": summary,
        }

    def _load_rules(self, task: dict) -> dict:
        if "rules" in task and "rules_path" in task:
            raise InvalidParams("use either rules or rules_path, not both")
        if "rules" in task:
            rules = task["rules"]
            if not isinstance(rules, dict):
                raise InvalidParams("task.rules must be an object")
        elif "rules_path" in task:
            p = Path(str(task["rules_path"]))
            try:
                rules = json.loads(p.read_text(encoding="utf-8"))
            except OSError as exc:
                raise InputError(f"cannot read rules_path: {exc}")
            except ValueError as exc:
                raise InputError(f"rules_path is not valid JSON: {exc}")
        else:
            try:
                rules = json.loads(DEFAULT_RULESET_PATH.read_text(encoding="utf-8"))
            except (OSError, ValueError) as exc:
                raise InputError(f"default ruleset unreadable: {exc}")
        validate_ruleset(rules)
        return rules


def signals_minus(signals: dict) -> dict:
    """signals 中派生字段（cves 列表等）单独透出，布尔字段保留给消费方。"""
    out = {k: v for k, v in signals.items() if k != "entry_id"}
    return out


if __name__ == "__main__":
    EvaluationWorker().serve()
