#!/usr/bin/env python3
"""M8 检索相关性标注任务单生成器（read-only）。

对硬编码查询集（8 类，中/英/日）逐条调用统一检索 API（默认 ACTIVE 通道；
候选类查询显式带 include_candidates），取 Top10 命中输出 JSONL 标注任务单：
docs/knowledge-governance/retrieval-qrel-task-<date>.jsonl
每行一个 (query, rank, doc) 待评项，relevance 字段留空待人工填 0-3。

不修改任何审核状态、不写库；仅读 .rp-workbench-token 与派发 HTTP 请求。
配套：evaluate_retrieval.py 读已标注 JSONL 算 Recall@K / Precision@K / MRR /
nDCG@10 / No-result Rate / Source Coverage / Candidate Leakage Rate。
"""
from __future__ import annotations

import json
import sys
import time
import urllib.parse
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
TOKEN_FILE = ROOT / ".rp-workbench-token"
OUT_DIR = ROOT / "docs/knowledge-governance"
API = "http://127.0.0.1:7800/api/search"
TOP_K = 10

# 8 类查询集：CVE 精确 / 自然语言漏洞（中英日）/ 技术关键词 / 工具 / POC /
# Nuclei / Skill / 多来源综合 + 无结果与弃权探针。
QUERY_SET = [
    # 1) CVE 精确（结构化路由）
    ("cve_exact", "CVE-2021-44228", False),
    ("cve_exact", "CVE-2023-34362", False),
    ("cve_exact", "CVE-2017-0144", False),
    ("cve_exact", "CVE-2019-0708", False),
    ("cve_exact", "CVE-2020-1472", False),
    ("cve_exact", "CVE-2014-6271", False),
    # 2) 自然语言漏洞查询（中/英/日）
    ("nl_vuln_en", "Java deserialization remote code execution", False),
    ("nl_vuln_en", "authentication bypass in router web panel", False),
    ("nl_vuln_en", "arbitrary file read via path traversal in Java apps", False),
    ("nl_vuln_zh", "Java 反序列化远程代码执行漏洞", False),
    ("nl_vuln_zh", "路由器管理面板身份验证绕过", False),
    ("nl_vuln_zh", "Spring 框架远程代码执行", False),
    ("nl_vuln_ja", "Java 逆シリアル化によるリモートコード実行", False),
    ("nl_vuln_ja", "認証バイパスの脆弱性", False),
    # 3) 技术关键词
    ("keyword", "Apache Struts", False),
    ("keyword", "SQL Injection", False),
    ("keyword", "XSS", False),
    ("keyword", "Privilege Escalation", False),
    ("keyword", "Linux Privilege Escalation", False),
    ("keyword", "Windows Privilege Escalation", False),
    ("keyword", "sourcemap", False),
    ("keyword", "Nuclei", False),
    # 4) 工具查询
    ("tool", "nmap", False),
    ("tool", "sqlmap", False),
    ("tool", "frp", False),
    ("tool", "chisel", False),
    ("tool", "fscan", False),
    # 5) POC 查询
    ("poc", "exchange proxyshell poc", False),
    ("poc", "zabbix remote code execution exploit", False),
    ("poc", "weblogic deserialization poc", False),
    # 6) Nuclei 模板查询
    ("nuclei", "nuclei template log4j", False),
    ("nuclei", "spring actuator exposure template", False),
    ("nuclei", "wordpress plugin detection", False),
    # 7) Skill 查询
    ("skill", "active scan", False),
    ("skill", "ad attacks", False),
    ("skill", "lateral movement", False),
    ("skill", "内网横向移动", False),
    # 8) 多来源综合
    ("multi_source", "log4shell", False),
    ("multi_source", "Confluence Atlassian vulnerability", False),
    ("multi_source", "VMware vCenter vulnerability", False),
    # 9) 无结果 / 弃权探针（标注：期望空或低相关，测 No-result Rate）
    ("no_result", "zzqqxx-完全无关词-9x7", False),
    ("no_result", "quantum cryptocurrency jellyfish unicorn", False),
    # 10) 候选通道探针（include_candidates=true；候选会带 CANDIDATE 标记）
    ("candidates", "sourcemap", True),
    ("candidates", "webpack source map disclosure", True),
]


def search(query: str, include_candidates: bool) -> list:
    token = TOKEN_FILE.read_text().strip()
    params = {"q": query, "limit": str(TOP_K)}
    if include_candidates:
        params["include_candidates"] = "true"
    url = API + "?" + urllib.parse.urlencode(params)
    req = urllib.request.Request(url, headers={"Authorization": f"Bearer {token}"})
    with urllib.request.urlopen(req, timeout=30) as resp:
        payload = json.load(resp)
    return payload.get("hits") or []


def main() -> int:
    date = time.strftime("%Y-%m-%d")
    out_path = OUT_DIR / f"retrieval-qrel-task-{date}.jsonl"
    OUT_DIR.mkdir(parents=True, exist_ok=True)
    lines = []
    queries_seen = set()
    failures = []
    for category, query, candidates in QUERY_SET:
        qid = f"{category}:{queries_seen.__len__()}:{query[:48]}"
        queries_seen.add(qid)
        try:
            hits = search(query, candidates)
        except Exception as exc:  # noqa: BLE001
            failures.append({"query": query, "error": str(exc)})
            continue
        for rank, hit in enumerate(hits, start=1):
            detail = hit.get("detail") or {}
            identity = (detail.get("identity") or {})
            lines.append(json.dumps({
                "query_id": qid,
                "category": category,
                "query": query,
                "include_candidates": candidates,
                "rank": rank,
                "doc_id": hit.get("item_id") or identity.get("source_entity_id") or "",
                "title": (detail.get("title") or hit.get("title") or "")[:160],
                "source_system": identity.get("source_system") or detail.get("source_system") or "",
                "governance_status": detail.get("governance_status") or "",
                "deep_link": detail.get("canonical_url") or "",
                "relevance": None,          # 人工填：0-3（见标注指南）
                "notes": "",
            }, ensure_ascii=False))
        # 查询无命中也要留一行空标记，评测器才能算 No-result Rate。
        if not hits:
            lines.append(json.dumps({
                "query_id": qid, "category": category, "query": query,
                "include_candidates": candidates, "rank": 0, "doc_id": "",
                "title": "", "source_system": "", "governance_status": "",
                "deep_link": "", "relevance": None, "notes": "NO_RESULT",
            }, ensure_ascii=False))
        time.sleep(0.3)
    out_path.write_text("\n".join(lines) + "\n")
    print(json.dumps({
        "command": "build_retrieval_qrel_task",
        "out": str(out_path),
        "queries": len(QUERY_SET),
        "annotation_items": len(lines),
        "request_failures": failures,
    }, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    sys.exit(main())
