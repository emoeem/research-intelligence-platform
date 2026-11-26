#!/usr/bin/env python3
"""Generate a CVSS/EPSS-prioritized CISA KEV shortlist from approved metadata-only sources."""
from __future__ import annotations

import argparse
import json
import sqlite3
from pathlib import Path

CISA = "knowledge.cisa-kev"
POCINDEX = "knowledge.pocindex-kev"
NUCLEI = "knowledge.nuclei-templates"
CVSS_VERSION_RANK = {"2.0": 1, "3.0": 2, "3.1": 3, "4.0": 4}
SEVERITY_RANK = {"critical": 5, "high": 4, "medium": 3, "low": 2, "info": 1, "unknown": 0}


def load_rows(conn: sqlite3.Connection, plugin_id: str):
    return conn.execute(
        "SELECT item_key,title,canonical_url,meta_json,source_commit FROM knowledge_source_entry "
        "WHERE plugin_id=? AND review_status='APPROVED'",
        (plugin_id,),
    ).fetchall()


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--platform-db", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--limit", type=int, default=30)
    args = parser.parse_args()
    if not 1 <= args.limit <= 500:
        raise SystemExit("--limit must be between 1 and 500")

    db = args.platform_db.expanduser().resolve(strict=True)
    conn = sqlite3.connect(f"file:{db}?mode=ro", uri=True)
    conn.row_factory = sqlite3.Row
    try:
        cisa_rows = load_rows(conn, CISA)
        poc_rows = load_rows(conn, POCINDEX)
        nuclei_rows = load_rows(conn, NUCLEI)
    finally:
        conn.close()

    cisa = {}
    for row in cisa_rows:
        record = json.loads(row["meta_json"] or "{}").get("cve_record")
        if isinstance(record, dict):
            cisa[row["item_key"]] = (row, record)

    pocindex = {}
    for row in poc_rows:
        record = json.loads(row["meta_json"] or "{}").get("pocindex_record")
        if isinstance(record, dict):
            pocindex[row["item_key"]] = (row, record)

    nuclei_by_cve = {}
    for row in nuclei_rows:
        template = json.loads(row["meta_json"] or "{}").get("nuclei_template")
        if not isinstance(template, dict) or template.get("parse_status") != "parsed":
            continue
        classification = template.get("classification") or {}
        cves = classification.get("cve-id") or []
        if isinstance(cves, str):
            cves = [cves]
        for cve in cves:
            if isinstance(cve, str):
                nuclei_by_cve.setdefault(cve, []).append((row, template))

    records = []
    for cve, (poc_row, intel) in pocindex.items():
        if cve not in cisa:
            continue
        _cisa_row, cve_meta = cisa[cve]
        assessments = intel.get("cvss_assessments") or []
        primary = [a for a in assessments if a.get("assessment_type") == "Primary"]
        selected = max(
            primary or assessments,
            key=lambda a: (CVSS_VERSION_RANK.get(str(a.get("version")), 0), float(a.get("score", 0))),
            default=None,
        )
        templates = nuclei_by_cve.get(cve, [])
        templates.sort(
            key=lambda item: SEVERITY_RANK.get(str(item[1].get("severity", "unknown")).lower(), 0),
            reverse=True,
        )
        epss = intel.get("epss_score")
        percentile = intel.get("epss_percentile")
        advisories = intel.get("advisories") or []
        records.append({
            "cve": cve,
            "date": str(cve_meta.get("date_added", "")),
            "vendor": str(cve_meta.get("vendor_project", "")).replace("|", "\\|"),
            "product": str(cve_meta.get("product", "")).replace("|", "\\|"),
            "ransomware": cve_meta.get("known_ransomware_campaign_use", "Unknown"),
            "forensic": cve_meta.get("forensic_triage", "Unknown"),
            "required_action": str(cve_meta.get("required_action", "Not specified")).replace("|", "\\|").replace("\n", " ")[:240],
            "cwes": ", ".join(cve_meta.get("cwes") or []) or "—",
            "cisa_url": _cisa_row["canonical_url"] or f"https://www.cisa.gov/known-exploited-vulnerabilities-catalog?field_cve={cve}",
            "pocindex_kev": bool(intel.get("pocindex_kev_present")),
            "epss": epss,
            "percentile": percentile,
            "cvss": selected,
            "templates": templates,
            "pocindex_nuclei": intel.get("nuclei_metadata"),
            "advisories": advisories,
        })

    records.sort(
        key=lambda r: (
            float(r["epss"]) if r["epss"] is not None else -1,
            float(r["cvss"]["score"]) if r["cvss"] else -1,
            r["date"],
        ),
        reverse=True,
    )

    output = [
        "# CISA KEV 高风险优先级清单（PoC Index CVSS/EPSS 快照）",
        "",
        "按 EPSS 降序、CVSS 降序、CISA 收录日期降序排列。该列表用于漏洞修复优先级，不代表任何具体目标当前易受攻击。",
        "",
        f"- CISA KEV：{len(cisa_rows)} 条审核通过记录；上游 commit `51e0a777743fa06957afbaeb280f8735d292c1be`（CC0-1.0）。",
        f"- PoC Index：{len(poc_rows)} 条审核通过的 CVE 风险元数据；上游 commit `e9076fea96095e014fc107c02dd23ee8dc21476d`；EPSS 来自 FIRST，CVSS assessment 保留 NVD/CVE Program 来源。",
        f"- Nuclei：{len(nuclei_rows)} 条审核通过模板元数据；上游 commit `a346d8f00c61bf95bbd30365f12cf923be51ba3d`（MIT）。",
        f"- 覆盖情况：PoC Index KEV membership {sum(bool(v[1].get('pocindex_kev_present')) for v in pocindex.values())}；EPSS {sum(v[1].get('epss_score') is not None for v in pocindex.values())}；CVSS {sum(bool(v[1].get('cvss_assessments')) for v in pocindex.values())}；PoC Index Nuclei metadata {sum(v[1].get('nuclei_metadata') is not None for v in pocindex.values())}；本地 Nuclei 模板覆盖 {len(nuclei_by_cve)} 个 CVE。",
        f"- 当前展示 {min(args.limit, len(records))} 条；CISA KEV 原始全集仍为 {len(cisa_rows)} 条。缺少 PoC Index enrichment 不代表不存在 PoC。",
        "- 本表的 EPSS/CVSS 来自 PoC Index 聚合快照，不是 CISA 字段；保留 assessment source/type，不将多版本 CVSS 合并成一个虚构分数。",
        "",
        "| CVE | 厂商 / 产品 | CISA 收录 | EPSS | CVSS 主评估 | 本地 Nuclei | PoC Index Nuclei | KEV 索引 | 勒索软件使用 | 取证分诊 |",
        "|---|---|---:|---:|---|---|---|---|---|---|",
    ]
    nuclei_commit = "a346d8f00c61bf95bbd30365f12cf923be51ba3d"
    for item in records[: args.limit]:
        epss = f"{item['epss']:.5f}" if item["epss"] is not None else "—"
        if item["percentile"] is not None:
            epss += f" ({item['percentile']:.1%})"
        cvss = "—"
        if item["cvss"]:
            a = item["cvss"]
            cvss = f"v{a['version']} {float(a['score']):.1f} {a['severity']} ({a['source']})"
        local_templates = item["templates"]
        local_label = "—"
        if local_templates:
            severity = str(local_templates[0][1].get("severity", "unknown")).lower()
            local_label = f"{len(local_templates)} · {severity}"
        pn = item["pocindex_nuclei"] or {}
        pn_label = "—"
        if pn:
            pn_label = f"{pn.get('severity', 'unknown')} / {pn.get('cvss') if pn.get('cvss') is not None else '—'} / {pn.get('cwe') or '—'}"
        output.append(
            f"| [{item['cve']}]({item['cisa_url']}) | {item['vendor']} / {item['product']} | {item['date']} | {epss} | {cvss} | {local_label} | {pn_label} | {'是' if item['pocindex_kev'] else '否'} | {item['ransomware']} | {item['forensic']} |"
        )
        output.append(f"| ↳ CISA 修复动作 | {item['required_action']} | | | | | | | | |")
        links = []
        for row, template in local_templates[:2]:
            path = row["item_key"]
            if path.startswith("/") or ".." in Path(path).parts:
                continue
            title = str(template.get("template_id") or row["title"]).replace("|", "\\|")
            links.append(f"[{title}](https://github.com/projectdiscovery/nuclei-templates/blob/{nuclei_commit}/{path})")
        advisories = []
        for advisory in item["advisories"][:2]:
            url = advisory.get("url", "")
            tags = advisory.get("tags") or []
            if url.startswith(("https://", "http://")):
                advisories.append(f"[{tags[0] if tags else 'Advisory'}]({url})")
        if links or advisories:
            output.append(f"| ↳ 参考 | Nuclei: {'; '.join(links) or '—'} · Advisories: {'; '.join(advisories) or '—'} | | | | | | | | |")

    output.extend([
        "",
        "## 解释与限制",
        "",
        "- EPSS 是 FIRST 对未来 30 天内被利用概率的估计，不代表某个目标是否存在该漏洞。",
        "- CVSS 保留 NVD/CVE Program 的多版本 assessment；表格选择 Primary assessment 中版本优先级最高的一项展示，完整数组保留在知识库记录中。",
        "- PoC Index KEV membership 仅表示该 CVE 出现在 PoC Index 的 KEV 索引，不代表索引中一定存在可执行 PoC。",
        "- Nuclei 模板只做 YAML/元数据校验，未执行模板、未对真实目标发送请求；模板存在不等于目标受影响。",
        "- PoC Index 的 `CVE_list.json` PoC 链接清单未纳入本批；本批只保留 CVSS/EPSS、advisory URL 与 Nuclei 元数据，不复制 PoC 正文。",
        "- CISA KEV 中没有 PoC Index enrichment 的记录继续保留在 CISA 数据集中，不按缺失推断为无漏洞或无 PoC。",
        "",
    ])
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text("\n".join(output), encoding="utf-8")
    print(json.dumps({"cisa": len(cisa_rows), "pocindex": len(poc_rows), "nuclei": len(nuclei_rows), "risk_rows": len(records), "written": min(args.limit, len(records)), "output": str(args.output)}, ensure_ascii=False, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
