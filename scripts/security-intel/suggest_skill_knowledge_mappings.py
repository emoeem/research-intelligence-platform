#!/usr/bin/env python3
"""Generate exact-CVE Skill -> Vulnerability/Nuclei-template candidates.

Reads Platform Skills, the active KSE plugin projection and RTIS-derived source
metadata in SQLite read-only mode. Skill bodies are constrained to the repo.
Only CVE identifiers physically present in a Skill body are used; fuzzy title
matching is intentionally not performed. No source database is modified.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import re
import sqlite3
import sys
from pathlib import Path
from typing import Any

CVE_RE = re.compile(r"CVE-\d{4}-\d{4,7}", re.I)


def connect(path: Path) -> sqlite3.Connection:
    conn = sqlite3.connect(f"file:{path.resolve().as_posix()}?mode=ro", uri=True)
    conn.row_factory = sqlite3.Row
    conn.execute("PRAGMA query_only=ON")
    return conn


def safe_body(raw: str | None, root: Path) -> Path | None:
    if not raw:
        return None
    candidate = Path(raw)
    if not candidate.is_absolute():
        candidate = root / candidate
    try:
        resolved = candidate.resolve(strict=True)
        resolved.relative_to(root.resolve())
    except (OSError, ValueError):
        return None
    return resolved if resolved.is_file() else None


def sha256_json(value: Any) -> str:
    data = json.dumps(value, ensure_ascii=False, sort_keys=True, separators=(",", ":")).encode("utf-8")
    return hashlib.sha256(data).hexdigest()


def body_cve_mentions(body: str) -> dict[str, list[dict[str, Any]]]:
    found: dict[str, list[dict[str, Any]]] = {}
    for line_no, line in enumerate(body.splitlines(), 1):
        for match in CVE_RE.finditer(line):
            cve = match.group(0).upper()
            found.setdefault(cve, [])
            item = {"line": line_no, "excerpt": line.strip()[:280]}
            if item not in found[cve] and len(found[cve]) < 5:
                found[cve].append(item)
    return found


def build_report(platform_db: Path, plugin_db: Path, repo_root: Path) -> dict[str, Any]:
    platform = connect(platform_db)
    plugin = connect(plugin_db)
    try:
        skills = platform.execute(
            "SELECT skill_id,version,body_ref,enabled,governance,source_system,source_pk "
            "FROM skill ORDER BY skill_id"
        ).fetchall()
        templates: dict[str, list[dict[str, Any]]] = {}
        for row in plugin.execute(
            "SELECT plugin_id,plugin_version,item_key,title,content_sha256,source_url,source_commit,"
            "review_status,license_status,meta_json,body_bytes FROM knowledge_source_entry "
            "WHERE plugin_id='knowledge.nuclei-templates' AND plugin_version='0.1.1' "
            "AND review_status IN ('APPROVED','DISCOVERED') ORDER BY item_key"
        ):
            meta = json.loads(row["meta_json"] or "{}")
            template = meta.get("nuclei_template") or {}
            classification = template.get("classification") or {}
            cve_value = classification.get("cve-id")
            if not isinstance(cve_value, str):
                continue
            for cve in set(m.upper() for m in CVE_RE.findall(cve_value)):
                templates.setdefault(cve, []).append(dict(row))

        kev: dict[str, dict[str, Any]] = {}
        for row in plugin.execute(
            "SELECT plugin_id,plugin_version,item_key,title,content_sha256,source_url,source_commit,"
            "review_status,license_status,meta_json FROM knowledge_source_entry "
            "WHERE plugin_id='knowledge.cisa-kev' AND plugin_version='0.1.2' AND review_status='APPROVED'"
        ):
            kev[row["item_key"].upper()] = dict(row)

        candidates: list[dict[str, Any]] = []
        missing_bodies: list[str] = []
        skill_body_count = 0
        skill_cve_pairs: set[tuple[str, str]] = set()
        matched_template_keys: set[str] = set()
        kev_pair_count = 0
        template_review_counts: dict[str, int] = {}

        for skill in skills:
            body_path = safe_body(skill["body_ref"], repo_root)
            if not body_path:
                missing_bodies.append(skill["skill_id"])
                continue
            body_bytes = body_path.read_bytes()
            body = body_bytes.decode("utf-8", errors="replace")
            body_hash = hashlib.sha256(body_bytes).hexdigest()
            body_ref = body_path.relative_to(repo_root.resolve()).as_posix()
            skill_body_count += 1
            mentions = body_cve_mentions(body)
            version = skill["version"] or f"unversioned-{body_hash[:12]}"
            skill_source = {
                "source_system": skill["source_system"] or "rip.skill-registry",
                "source_pk": skill["source_pk"] or skill["skill_id"],
                "body_ref": body_ref,
                "body_sha256": body_hash,
            }
            for cve, excerpts in sorted(mentions.items()):
                matching_templates = templates.get(cve, [])
                if not matching_templates:
                    continue
                skill_cve_pairs.add((skill["skill_id"], cve))
                kev_record = kev.get(cve)
                if kev_record:
                    kev_pair_count += 1
                    vuln_source = {
                        "source_system": kev_record["plugin_id"],
                        "source_entity_id": kev_record["item_key"],
                        "source_version": f"{kev_record['plugin_version']}@{kev_record['source_commit'] or 'unknown'}",
                        "canonical_uri": kev_record["source_url"] or f"cve://{cve}",
                        "content_sha256": kev_record["content_sha256"],
                        "source_review_status": kev_record["review_status"],
                        "license_status": kev_record["license_status"],
                    }
                else:
                    projection = [
                        {"item_key": row["item_key"], "content_sha256": row["content_sha256"]}
                        for row in matching_templates
                    ]
                    vuln_source = {
                        "source_system": "knowledge.nuclei-templates.cve-index",
                        "source_entity_id": cve,
                        "source_version": "0.1.1-cve-index",
                        "canonical_uri": f"cve://{cve}",
                        "content_sha256": sha256_json(projection),
                        "source_review_status": "DERIVED_FROM_TEMPLATE_CLASSIFICATION",
                        "license_status": "SEE_LINKED_TEMPLATE_SNAPSHOTS",
                    }
                candidates.append({
                    "skill_id": skill["skill_id"],
                    "skill_version": skill["version"],
                    "skill_enabled": bool(skill["enabled"]) if skill["enabled"] is not None else None,
                    "skill_governance": skill["governance"],
                    "target_type": "vulnerability",
                    "target_key": cve,
                    "target_source": vuln_source,
                    "relation_type": "REFERENCES",
                    "review_status": "CANDIDATE",
                    "activation_eligible_after_review": bool(skill["enabled"] == 1 and skill["governance"] == "ACTIVE"),
                    "match_basis": "exact_cve_identifier_in_skill_body_and_active_nuclei_classification",
                    "cve": cve,
                    "evidence": excerpts,
                    "source": skill_source,
                    "decision_note": "Exact identifier reference only; this does not prove the Skill applies to every affected product/version or that a target is vulnerable.",
                })
                for template in matching_templates:
                    matched_template_keys.add(template["item_key"])
                    template_review_counts[template["review_status"]] = template_review_counts.get(template["review_status"], 0) + 1
                    target_version = f"{template['plugin_version']}@{template['source_commit'] or 'unknown'}"
                    candidates.append({
                        "skill_id": skill["skill_id"],
                        "skill_version": skill["version"],
                        "skill_enabled": bool(skill["enabled"]) if skill["enabled"] is not None else None,
                        "skill_governance": skill["governance"],
                        "target_type": "detection_template",
                        "target_key": f"nuclei-template:{template['item_key']}",
                        "target_source": {
                            "source_system": template["plugin_id"],
                            "source_entity_id": template["item_key"],
                            "source_version": target_version,
                            "canonical_uri": f"nuclei-template://{template['plugin_version']}/{template['item_key']}",
                            "content_sha256": template["content_sha256"],
                            "source_commit": template["source_commit"],
                            "source_url": template["source_url"],
                            "source_review_status": template["review_status"],
                            "license_status": template["license_status"],
                            "body_bytes": template["body_bytes"],
                        },
                        "relation_type": "REFERENCES",
                        "review_status": "CANDIDATE",
                        "activation_eligible_after_review": bool(
                            skill["enabled"] == 1 and skill["governance"] == "ACTIVE"
                            and template["review_status"] == "APPROVED"
                            and template["license_status"] in ("MIT", "Apache-2.0", "BSD-3-Clause", "BSD-2-Clause", "ISC", "CC0-1.0")
                        ),
                        "match_basis": "exact_cve_identifier_in_skill_body_and_nuclei_template_classification",
                        "cve": cve,
                        "template_title": template["title"],
                        "evidence": excerpts,
                        "source": skill_source,
                        "decision_note": "Exact CVE join only. KSE review/license metadata is copied as provenance, but the new graph source assessment remains UNREVIEWED until independently reviewed.",
                    })

        candidates.sort(key=lambda row: (row["skill_id"], row["target_type"], row["target_key"]))
        return {
            "schema_version": "1.0.0",
            "mode": "read_only_exact_identifier_candidates",
            "source_databases": {
                "platform": {"path": str(platform_db), "sha256": hashlib.sha256(platform_db.read_bytes()).hexdigest()},
                "plugin": {"path": str(plugin_db), "sha256": hashlib.sha256(plugin_db.read_bytes()).hexdigest()},
            },
            "summary": {
                "skills_total": len(skills),
                "skills_with_readable_body": skill_body_count,
                "skills_missing_or_outside_repo_body": len(missing_bodies),
                "skill_cve_pairs": len(skill_cve_pairs),
                "unique_skills_with_cve_matches": len({skill for skill, _ in skill_cve_pairs}),
                "unique_cves": len({cve for _, cve in skill_cve_pairs}),
                "skill_vulnerability_candidates": sum(row["target_type"] == "vulnerability" for row in candidates),
                "skill_template_candidates": sum(row["target_type"] == "detection_template" for row in candidates),
                "unique_nuclei_templates": len(matched_template_keys),
                "skill_cve_pairs_with_approved_cisa_kev": kev_pair_count,
                "template_source_review_distribution": dict(sorted(template_review_counts.items())),
                "total_candidates": len(candidates),
            },
            "governance": {
                "auto_approved": False,
                "source_databases_mutated": False,
                "relation_type": "REFERENCES",
                "source_assessment_on_import": "UNREVIEWED",
                "heuristic_score_is_probability": False,
                "matching_policy": "CVE identifier must occur in the Skill body and match the active KSE Nuclei classification; no fuzzy matching.",
            },
            "missing_bodies": missing_bodies,
            "candidates": candidates,
        }
    finally:
        platform.close()
        plugin.close()


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--platform-db", type=Path, default=Path("migrations/platform.db"))
    parser.add_argument("--plugin-db", type=Path, default=Path("migrations/dev-p6.db"))
    parser.add_argument("--repo-root", type=Path, default=Path.cwd())
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    report = build_report(args.platform_db, args.plugin_db, args.repo_root)
    rendered = json.dumps(report, ensure_ascii=False, indent=2) + "\n"
    if args.output:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(rendered, encoding="utf-8")
    else:
        sys.stdout.write(rendered)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
