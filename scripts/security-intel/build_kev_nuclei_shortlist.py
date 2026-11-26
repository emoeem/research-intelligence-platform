#!/usr/bin/env python3
"""Generate a read-only Markdown shortlist of CISA KEV CVEs with approved Nuclei coverage."""
from __future__ import annotations

import argparse
import json
import sqlite3
from pathlib import Path

CISA_PLUGIN = "knowledge.cisa-kev"
NUCLEI_PLUGIN = "knowledge.nuclei-templates"
CISA_COMMIT = "51e0a777743fa06957afbaeb280f8735d292c1be"


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--platform-db", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--limit", type=int, default=30)
    args = parser.parse_args()
    if args.limit < 1 or args.limit > 500:
        raise SystemExit("--limit must be between 1 and 500")

    db = args.platform_db.expanduser().resolve(strict=True)
    conn = sqlite3.connect(f"file:{db}?mode=ro", uri=True)
    conn.row_factory = sqlite3.Row
    try:
        kev_rows = conn.execute(
            "SELECT item_key, title, canonical_url, meta_json, source_commit "
            "FROM knowledge_source_entry WHERE plugin_id=? AND review_status='APPROVED'",
            (CISA_PLUGIN,),
        ).fetchall()
        nuclei_rows = conn.execute(
            "SELECT item_key, title, canonical_url, meta_json, source_commit, license_status "
            "FROM knowledge_source_entry WHERE plugin_id=? AND review_status='APPROVED'",
            (NUCLEI_PLUGIN,),
        ).fetchall()
    finally:
        conn.close()

    kev: dict[str, tuple[sqlite3.Row, dict]] = {}
    for row in kev_rows:
        meta = json.loads(row["meta_json"] or "{}")
        record = meta.get("cve_record")
        if isinstance(record, dict) and record.get("known_exploited") is True:
            kev[row["item_key"]] = (row, record)

    by_cve: dict[str, list[tuple[sqlite3.Row, dict, dict]]] = {}
    for row in nuclei_rows:
        meta = json.loads(row["meta_json"] or "{}")
        template = meta.get("nuclei_template")
        if not isinstance(template, dict) or template.get("parse_status") != "parsed":
            continue
        classification = template.get("classification") or {}
        cves = classification.get("cve-id") or []
        if isinstance(cves, str):
            cves = [cves]
        if not isinstance(cves, list):
            continue
        for cve in cves:
            if isinstance(cve, str) and cve in kev:
                by_cve.setdefault(cve, []).append((row, template, classification))

    rank = {"critical": 5, "high": 4, "medium": 3, "low": 2, "info": 1}
    records = []
    for cve, (kev_row, cve_meta) in kev.items():
        templates = by_cve.get(cve, [])
        if not templates:
            continue
        templates.sort(
            key=lambda item: rank.get(str(item[1].get("severity", "unknown")).lower(), 0),
            reverse=True,
        )
        severity = max(
            (str(item[1].get("severity", "unknown")).lower() for item in templates),
            key=lambda value: rank.get(value, 0),
        )
        cvss_values = [item[2].get("cvss-score") for item in templates if item[2].get("cvss-score") is not None]
        epss_values = [item[2].get("epss-score") for item in templates if item[2].get("epss-score") is not None]
        percentile_values = [item[2].get("epss-percentile") for item in templates if item[2].get("epss-percentile") is not None]
        def max_numeric(values):
            numeric = []
            for value in values:
                try:
                    numeric.append(float(value))
                except (TypeError, ValueError):
                    pass
            return max(numeric) if numeric else None
        records.append({
            "date": str(cve_meta.get("date_added", "")),
            "cve": cve,
            "kev_url": kev_row["canonical_url"] or f"https://www.cisa.gov/known-exploited-vulnerabilities-catalog?field_cve={cve}",
            "vendor": str(cve_meta.get("vendor_project", "")).replace("|", "\\|"),
            "product": str(cve_meta.get("product", "")).replace("|", "\\|"),
            "ransomware": cve_meta.get("known_ransomware_campaign_use", "Unknown"),
            "forensic": cve_meta.get("forensic_triage", "Unknown"),
            "cwes": ", ".join(cve_meta.get("cwes") or []) or "—",
            "severity": severity,
            "cvss": max_numeric(cvss_values),
            "epss": max_numeric(epss_values),
            "percentile": max_numeric(percentile_values),
            "templates": templates,
        })

    records.sort(key=lambda item: (item["date"], rank.get(item["severity"], 0), item["cve"]), reverse=True)
    output = [
        "# CISA KEV × Nuclei 高风险漏洞清单（2026-11-26 快照）",
        "",
        "本表按 CISA 收录日期排序，列出有已审核 Nuclei 模板覆盖的近期 KEV CVE。CISA KEV 表示已知在野利用，不代表某个目标当前仍受影响。模板只完成静态解析与来源审核，没有执行。",
        "",
        f"- CISA KEV：commit `{CISA_COMMIT}`（CC0-1.0）；审核通过 {len(kev)} 条。",
        f"- Nuclei：commit `a346d8f00c61bf95bbd30365f12cf923be51ba3d`（MIT）；审核通过 {len(nuclei_rows)} 条匹配模板。",
        f"- 交叉覆盖：{len(by_cve)} 个 KEV CVE；共 {sum(len(v) for v in by_cve.values())} 条模板；当前展示 {min(args.limit, len(records))} 条。",
        "- CVSS/EPSS 列取自 Nuclei 模板 classification，不是 CISA KEV 字段，也不是本工具独立请求 NVD/FIRST 后得到的最新评分。",
        "",
        "| CVE | 厂商 / 产品 | CISA 收录 | Nuclei severity | Nuclei CVSS | Nuclei EPSS | 勒索软件使用 | 取证分诊 | CWE |",
        "|---|---|---:|---|---:|---:|---|---|---|",
    ]
    commit = "a346d8f00c61bf95bbd30365f12cf923be51ba3d"
    for item in records[: args.limit]:
        cvss = f"{item['cvss']:.1f}" if item["cvss"] is not None else "—"
        epss = f"{item['epss']:.5f}" if item["epss"] is not None else "—"
        percentile = f" ({item['percentile']:.1%})" if item["percentile"] is not None else ""
        output.append(
            f"| [{item['cve']}]({item['kev_url']}) | {item['vendor']} / {item['product']} | {item['date']} | {item['severity']} | {cvss} | {epss}{percentile} | {item['ransomware']} | {item['forensic']} | {item['cwes']} |"
        )
        links = []
        for row, template, _classification in item["templates"][:2]:
            path = row["item_key"]
            if path.startswith("/") or ".." in Path(path).parts:
                continue
            title = str(template.get("template_id") or row["title"]).replace("|", "\\|")
            links.append(f"[{title}](https://github.com/projectdiscovery/nuclei-templates/blob/{commit}/{path})")
        output.append(f"| ↳ 检测模板 | {'; '.join(links) or '—'} | | | | | | | |")

    output.extend([
        "",
        "## 使用说明",
        "",
        "- 优先查看 CISA `requiredAction` 与厂商修复公告；KEV 收录是修复优先级信号。",
        "- Nuclei CVSS/EPSS 是模板自带分类字段，可能滞后；后续应独立接入 NVD 与 FIRST EPSS 并保留来源和更新时间。",
        "- Nuclei 模板只用于已授权的检测范围；执行前仍需单独确认目标授权、版本、参数和速率限制。",
        "- CISA CVE 条目的 `cvss_score` / `epss_score` 仍保持 null；不会把模板分数冒充 CISA 字段。",
        "",
    ])
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text("\n".join(output), encoding="utf-8")
    print(json.dumps({"kev_records": len(kev), "nuclei_approved": len(nuclei_rows), "overlap_cves": len(by_cve), "overlap_templates": sum(len(v) for v in by_cve.values()), "rows_written": min(args.limit, len(records)), "output": str(args.output)}, ensure_ascii=False, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
