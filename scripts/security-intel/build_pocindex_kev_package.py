#!/usr/bin/env python3
"""Build a metadata-only PoC Index enrichment snapshot for the official CISA KEV set."""
from __future__ import annotations

import argparse
import hashlib
import json
import re
from datetime import datetime, timezone
from pathlib import Path

REPO_URL = "https://github.com/0xMarcio/pocindex"
CDN_URL = "https://pocindex.io"
SOURCE_COMMIT = "e9076fea96095e014fc107c02dd23ee8dc21476d"
FEEDS = ("kev.json", "epss.json", "cve_metadata.json", "nuclei.json")
CVE_RE = re.compile(r"^CVE-[0-9]{4}-[0-9]{4,19}$", re.IGNORECASE)


def headers(path: Path) -> dict[str, str]:
    if not path.exists():
        return {}
    blocks = re.split(r"\r?\n\r?\n", path.read_text(encoding="latin-1"))
    result: dict[str, str] = {}
    for line in blocks[-2].splitlines() if len(blocks) > 1 else blocks[-1].splitlines():
        if ":" not in line:
            continue
        key, value = line.split(":", 1)
        result[key.strip().lower()] = value.strip()
    return result


def number_between(value, low: float, high: float, field: str) -> float:
    if isinstance(value, bool) or not isinstance(value, (int, float)):
        raise SystemExit(f"{field} must be numeric")
    result = float(value)
    if not low <= result <= high:
        raise SystemExit(f"{field} outside {low}..{high}: {result}")
    return result


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source-dir", type=Path, default=Path("target/pocindex-source"))
    parser.add_argument("--cisa-json", type=Path, default=Path("target/cisa-kev-package/known_exploited_vulnerabilities.json"))
    parser.add_argument("--output-dir", type=Path, default=Path("target/pocindex-kev-package"))
    parser.add_argument("--source-commit", default=SOURCE_COMMIT)
    parser.add_argument("--force", action="store_true")
    args = parser.parse_args()

    source_dir = args.source_dir.expanduser().resolve(strict=True)
    cisa_path = args.cisa_json.expanduser().resolve(strict=True)
    output_dir = args.output_dir.expanduser().resolve()
    if not re.fullmatch(r"[0-9a-fA-F]{40}", args.source_commit):
        raise SystemExit("source commit must be a full 40-character Git SHA")

    feeds = {}
    file_meta = {}
    for name in FEEDS:
        path = source_dir / name
        if not path.is_file() or path.stat().st_size == 0:
            raise SystemExit(f"missing or empty PoC Index feed: {path}")
        data = json.loads(path.read_text(encoding="utf-8"))
        if not isinstance(data, dict):
            raise SystemExit(f"PoC Index feed must be a JSON object: {name}")
        feeds[name] = data
        response_headers = headers(source_dir / f"{name}.headers")
        file_meta[name] = {
            "url": f"{CDN_URL}/{name}",
            "etag": response_headers.get("etag"),
            "last_modified": response_headers.get("last-modified"),
            "sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
            "bytes": path.stat().st_size,
        }

    cisa = json.loads(cisa_path.read_text(encoding="utf-8"))
    cisa_records = cisa.get("vulnerabilities")
    if not isinstance(cisa_records, list) or cisa.get("count") != len(cisa_records):
        raise SystemExit("CISA KEV source count does not match its records")
    cisa_by_id = {}
    for record in cisa_records:
        cve = record.get("cveID") if isinstance(record, dict) else None
        if not isinstance(cve, str) or not CVE_RE.fullmatch(cve):
            raise SystemExit(f"invalid CISA CVE ID: {cve!r}")
        if cve in cisa_by_id:
            raise SystemExit(f"duplicate CISA CVE ID: {cve}")
        cisa_by_id[cve] = record

    kev, epss, metadata, nuclei = (feeds[name] for name in FEEDS)
    outside_kev = sorted(set(kev) - set(cisa_by_id))
    if outside_kev:
        raise SystemExit(f"PoC Index KEV keys not present in official CISA feed: {outside_kev[:10]}")
    union = (set(kev) | set(epss) | set(metadata) | set(nuclei)) & set(cisa_by_id)
    records = []
    for cve in sorted(union):
        epss_value = epss.get(cve)
        epss_score = epss_percentile = None
        if epss_value is not None:
            if not isinstance(epss_value, list) or len(epss_value) != 2:
                raise SystemExit(f"invalid EPSS tuple for {cve}")
            epss_score = number_between(epss_value[0], 0, 1, f"{cve} EPSS score")
            epss_percentile = number_between(epss_value[1], 0, 1, f"{cve} EPSS percentile")

        cvss_assessments = []
        metadata_value = metadata.get(cve, {})
        if not isinstance(metadata_value, dict):
            raise SystemExit(f"invalid CVE metadata object for {cve}")
        for assessment in metadata_value.get("cvss", []):
            if not isinstance(assessment, list) or len(assessment) != 6:
                raise SystemExit(f"invalid CVSS assessment for {cve}: {assessment!r}")
            version, score, severity, vector, source, assessment_type = assessment
            if version not in {"2.0", "3.0", "3.1", "4.0"}:
                raise SystemExit(f"unsupported CVSS version for {cve}: {version!r}")
            cvss_assessments.append({
                "version": version,
                "score": number_between(score, 0, 10, f"{cve} CVSS score"),
                "severity": str(severity).upper(),
                "vector": str(vector),
                "source": str(source),
                "assessment_type": str(assessment_type),
            })

        advisories = []
        for advisory in metadata_value.get("advisories", []):
            if not isinstance(advisory, list) or len(advisory) != 2:
                raise SystemExit(f"invalid advisory record for {cve}: {advisory!r}")
            url, tags = advisory
            if not isinstance(url, str) or not url.startswith(("http://", "https://")) or not isinstance(tags, list):
                raise SystemExit(f"invalid advisory URL/tags for {cve}")
            advisories.append({"url": url, "tags": [str(tag) for tag in tags]})

        nuclei_value = nuclei.get(cve)
        if nuclei_value is not None:
            if not isinstance(nuclei_value, dict):
                raise SystemExit(f"invalid Nuclei metadata object for {cve}")
            severity = str(nuclei_value.get("severity", "unknown")).lower()
            if severity not in {"info", "low", "medium", "high", "critical", "unknown"}:
                raise SystemExit(f"invalid Nuclei severity for {cve}: {severity}")
            nuclei_value = {
                "severity": severity,
                "cvss": number_between(nuclei_value["cvss"], 0, 10, f"{cve} Nuclei CVSS") if nuclei_value.get("cvss") is not None else None,
                "cvss_vector": nuclei_value.get("cvss_vector"),
                "cwe": nuclei_value.get("cwe"),
            }

        cisa_record = cisa_by_id[cve]
        records.append({
            "cve_id": cve,
            "cisa_date_added": cisa_record.get("dateAdded"),
            "pocindex_kev_present": cve in kev,
            "pocindex_kev_value": kev.get(cve),
            "epss_score": epss_score,
            "epss_percentile": epss_percentile,
            "cvss_assessments": cvss_assessments,
            "advisories": advisories,
            "nuclei_metadata": nuclei_value,
        })

    if output_dir.exists() and not args.force:
        raise SystemExit(f"output directory already exists (pass --force to replace): {output_dir}")
    if output_dir.exists():
        import shutil
        shutil.rmtree(output_dir)
    output_dir.mkdir(parents=True)
    snapshot = {
        "repository": REPO_URL,
        "repository_commit": args.source_commit,
        "cdn": CDN_URL,
        "retrieved_at": datetime.now(timezone.utc).isoformat(),
        "cisa_catalog_version": cisa.get("catalogVersion"),
        "files": file_meta,
    }
    payload = {
        "schema": "pocindex-kev-enrichment-v1",
        "count": len(records),
        "snapshot": snapshot,
        "records": records,
    }
    output_json = output_dir / "pocindex-kev.json"
    output_json.write_text(json.dumps(payload, ensure_ascii=False, separators=(",", ":")) + "\n", encoding="utf-8")
    readme = (
        "# PoC Index KEV enrichment snapshot\n\n"
        f"Repository: {REPO_URL}\n"
        f"Repository commit: `{args.source_commit}`\n"
        f"CDN snapshot retrieved: `{snapshot['retrieved_at']}`\n"
        f"CISA catalog version: `{cisa.get('catalogVersion')}`\n"
        f"CVE records: `{len(records)}`\n"
        "License: MIT (metadata projection only; third-party advisory links retain their own terms).\n\n"
        "No exploit code or PoC bodies are included. EPSS/CVSS values remain attributed to their source feeds.\n"
    )
    (output_dir / "README.md").write_text(readme, encoding="utf-8")
    print(json.dumps({
        "records": len(records),
        "pocindex_kev": sum(r["pocindex_kev_present"] for r in records),
        "epss": sum(r["epss_score"] is not None for r in records),
        "cvss": sum(bool(r["cvss_assessments"]) for r in records),
        "nuclei": sum(r["nuclei_metadata"] is not None for r in records),
        "advisories": sum(len(r["advisories"]) for r in records),
        "output_bytes": output_json.stat().st_size,
        "output_dir": str(output_dir),
        "snapshot": snapshot,
        "execution_performed": False,
    }, ensure_ascii=False, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
