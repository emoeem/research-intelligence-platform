#!/usr/bin/env python3
"""Read-only non-Nuclei POC/weapon governance audit and review-queue generator.

Never executes source material. Raw content, usage, paths outside the repository,
verified notes, credentials, hostnames and IP addresses are never emitted.
Only metadata, hashes, structural flags and redacted review reasons are exported.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import re
import sqlite3
from collections import Counter, defaultdict
from pathlib import Path
from typing import Any
from urllib.parse import urlparse

ROOT = Path(__file__).resolve().parents[2]
DEFAULT_DB = ROOT / "integrations/rtis/knowledge.db"
DEFAULT_OUT = ROOT / "docs/knowledge-governance/poc-governance-candidates-2026-11-27.json"
CVE_RE = re.compile(r"^CVE-\d{4}-\d{4,7}$", re.I)
SECRET_PATTERNS = {
    "credential_assignment": re.compile(r"(?i)\b(?:password|passwd|secret|token|api[_-]?key|access[_-]?key|client[_-]?secret)\b\s*[:=]\s*['\"]?[^\s'\"]{8,}"),
    "bearer_token": re.compile(r"(?i)\bBearer\s+[A-Za-z0-9._~+/-]{16,}"),
    "jwt_like": re.compile(r"\beyJ[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{8,}\b"),
    "github_token": re.compile(r"\b(?:gh[pousr]_[A-Za-z0-9]{20,}|github_pat_[A-Za-z0-9_]{30,})\b"),
    "aws_access_key": re.compile(r"\bAKIA[0-9A-Z]{16}\b"),
}
ACTIVE_PATTERNS = {
    "network_client": re.compile(r"(?i)\b(?:curl|wget|requests\.|httpx\.|urllib\.|fetch\(|Invoke-WebRequest|Invoke-RestMethod)"),
    "process_execution": re.compile(r"(?i)\b(?:subprocess\.|os\.system|Popen\(|exec\(|eval\(|child_process|shell=True)"),
    "destructive_operation": re.compile(r"(?i)(?:rm\s+-rf|mkfs\.|dd\s+if=|format\s+[a-z]:|DROP\s+DATABASE|TRUNCATE\s+TABLE)"),
    "shell_or_reverse_channel": re.compile(r"(?i)(?:bash\s+-i|/dev/tcp/|nc\s+-[el]|ncat\s+-[el]|powershell\s+-enc)"),
    "credential_access": re.compile(r"(?i)(?:/etc/shadow|SAM\\|LSASS|Login Data|id_rsa|authorized_keys|\.aws/credentials)"),
}


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def connect(path: Path) -> sqlite3.Connection:
    conn = sqlite3.connect(f"file:{path.resolve().as_posix()}?mode=ro", uri=True)
    conn.row_factory = sqlite3.Row
    conn.execute("PRAGMA query_only=ON")
    return conn


def normalized_cves(raw: str | None) -> tuple[list[str], bool]:
    if not raw or not raw.strip():
        return [], False
    values = [x.strip().upper() for x in re.split(r"[,;\s]+", raw) if x.strip()]
    valid = sorted({x for x in values if CVE_RE.fullmatch(x)})
    invalid = any(x not in valid and x not in {"N/A", "NA", "NONE", "NULL"} for x in values)
    return valid, invalid


def secret_flags(value: str | None) -> list[str]:
    if not value:
        return []
    return [name for name, pattern in SECRET_PATTERNS.items() if pattern.search(value)]


def static_flags(value: str | None) -> list[str]:
    if not value:
        return []
    return [name for name, pattern in ACTIVE_PATTERNS.items() if pattern.search(value)]


def safe_local_path(raw: str | None, root: Path) -> tuple[str, Path | None]:
    if not raw:
        return "missing", None
    alias = "/home/emo/code/pensting/research-platform"
    normalized = raw.replace(alias, str(root))
    candidate = Path(normalized).expanduser()
    try:
        resolved = candidate.resolve(strict=True)
        resolved.relative_to(root.resolve())
    except FileNotFoundError:
        return "missing", None
    except (OSError, ValueError):
        return "outside_repository", None
    return ("file" if resolved.is_file() else "not_file"), resolved if resolved.is_file() else None


def source_url_state(raw: str | None) -> str:
    if not raw or not raw.strip():
        return "missing"
    parsed = urlparse(raw.strip())
    if parsed.scheme in {"http", "https"} and parsed.netloc:
        return "http_url"
    return "non_url_reference"


def build_report(db_path: Path, root: Path) -> dict[str, Any]:
    conn = connect(db_path)
    try:
        rows = conn.execute(
            "SELECT p.id,p.code,p.title,p.kind,p.category,p.cve,p.component,p.versions,"
            "p.severity,p.language,p.source,p.source_url,p.description,p.usage,p.content,p.path,"
            "p.verified,p.verified_note,p.tags,m.normalized_type,m.archetypes,m.technology_tags,"
            "m.method_tags,m.evidence_level,m.lifecycle,m.verification_basis,m.dedup_key,m.version "
            "FROM poc p LEFT JOIN k_meta m ON m.knowledge_id=p.id "
            "WHERE lower(COALESCE(p.source,'')) <> 'nuclei' ORDER BY p.id"
        ).fetchall()
        by_code: dict[str, list[int]] = defaultdict(list)
        by_content: dict[str, list[int]] = defaultdict(list)
        candidates: list[dict[str, Any]] = []
        summary = Counter()
        distributions: dict[str, Counter] = {k: Counter() for k in (
            "kind", "source", "severity", "path_status", "verification_basis", "lifecycle", "evidence_level"
        )}
        secret_record_count = 0
        secret_note_count = 0
        active_code_count = 0
        duplicate_code_ids: set[int] = set()
        duplicate_content_ids: set[int] = set()

        for row in rows:
            item = dict(row)
            record_id = int(item["id"])
            kind = (item["kind"] or "unknown").strip().lower()
            source = (item["source"] or "unknown").strip().lower()
            severity = (item["severity"] or "unknown").strip().lower()
            cves, malformed_cve = normalized_cves(item["cve"])
            path_status, file_path = safe_local_path(item["path"], root)
            file_hash = None
            file_bytes = None
            file_text = None
            if file_path:
                raw_file = file_path.read_bytes()
                file_hash = sha256(raw_file)
                file_bytes = len(raw_file)
                file_text = raw_file.decode("utf-8", errors="replace")
            content = item["content"] or ""
            content_hash = sha256(content.encode("utf-8")) if content else None
            note = item["verified_note"] or ""
            code = item["code"] or ""
            by_code[code].append(record_id)
            if content_hash:
                by_content[content_hash].append(record_id)
            note_secrets = secret_flags(note)
            content_secrets = secret_flags(content)
            file_secrets = secret_flags(file_text)
            secret_types = sorted(set(note_secrets + content_secrets + file_secrets))
            if secret_types:
                secret_record_count += 1
            if note_secrets:
                secret_note_count += 1
            active_types = sorted(set(static_flags(content) + static_flags(file_text)))
            if active_types:
                active_code_count += 1
            url_state = source_url_state(item["source_url"])
            meta_present = item["normalized_type"] is not None
            verification_basis = item["verification_basis"] or "missing"
            lifecycle = item["lifecycle"] or "missing"
            evidence_level = item["evidence_level"] or "missing"
            distributions["kind"][kind] += 1
            distributions["source"][source] += 1
            distributions["severity"][severity] += 1
            distributions["path_status"][path_status] += 1
            distributions["verification_basis"][verification_basis] += 1
            distributions["lifecycle"][lifecycle] += 1
            distributions["evidence_level"][evidence_level] += 1

            reasons = []
            if severity in {"critical", "high"}:
                reasons.append("high_severity_review_priority")
            if cves:
                reasons.append("cve_identity_review")
            if not item["versions"]:
                reasons.append("affected_versions_missing")
            if url_state == "missing":
                reasons.append("source_url_missing")
            if path_status in {"missing", "outside_repository"}:
                reasons.append("source_file_unavailable_or_unlinked")
            if item["verified"] == 1:
                reasons.append("legacy_verified_flag_requires_evidence_revalidation")
            if not note:
                reasons.append("verification_evidence_missing")
            if not meta_present:
                reasons.append("structured_governance_metadata_missing")
            if secret_types:
                reasons.append("potential_secret_or_credential_material_review_required")
            if active_types:
                reasons.append("executable_or_network_behavior_static_review_required")
            if malformed_cve:
                reasons.append("malformed_or_noncanonical_cve_value")
            if not item["description"]:
                reasons.append("description_missing")
            if not item["content"] and not file_path:
                reasons.append("source_body_missing")

            # The report deliberately contains no raw note, usage, source body or path.
            candidates.append({
                "entity_id": f"rtis:poc:{record_id}",
                "rtis_id": record_id,
                "code_sha256": sha256(code.encode("utf-8")) if code else None,
                "title": item["title"],
                "kind": kind,
                "category": item["category"],
                "severity": severity,
                "cve_ids": cves,
                "cve_value_malformed": malformed_cve,
                "component_present": bool(item["component"]),
                "affected_versions_present": bool(item["versions"]),
                "language": item["language"],
                "source": source,
                "source_url_state": url_state,
                "source_file_state": path_status,
                "source_file_sha256": file_hash,
                "source_file_bytes": file_bytes,
                "content_sha256": content_hash,
                "content_bytes": len(content.encode("utf-8")) if content else 0,
                "legacy_verified_flag": bool(item["verified"] == 1),
                "legacy_verification_note_present": bool(note),
                "legacy_verification_note_sha256": sha256(note.encode("utf-8")) if note else None,
                "verification_basis": verification_basis,
                "lifecycle": lifecycle,
                "evidence_level": evidence_level,
                "structured_metadata_present": meta_present,
                "secret_indicator_types": secret_types,
                "static_behavior_flags": active_types,
                "review_status": "CANDIDATE",
                "auto_approved": False,
                "review_reasons": reasons,
            })

        for group in by_code.values():
            if len(group) > 1:
                duplicate_code_ids.update(group)
        for group in by_content.values():
            if len(group) > 1:
                duplicate_content_ids.update(group)
        for candidate in candidates:
            candidate["duplicate_code_candidate"] = candidate["rtis_id"] in duplicate_code_ids
            candidate["duplicate_content_candidate"] = candidate["rtis_id"] in duplicate_content_ids
            if candidate["duplicate_code_candidate"] or candidate["duplicate_content_candidate"]:
                candidate["review_reasons"].append("possible_duplicate_requires_manual_equivalence_check")

        kinds = Counter(c["kind"] for c in candidates)
        summary.update({
            "total_records": len(candidates),
            "actual_poc_kind_records": kinds.get("poc", 0),
            "exp_records": kinds.get("exp", 0),
            "script_records": kinds.get("script", 0),
            "payload_records": kinds.get("payload", 0),
            "rule_records": kinds.get("rule", 0),
            "historical_verified_flag": sum(c["legacy_verified_flag"] for c in candidates),
            "with_structured_metadata": sum(c["structured_metadata_present"] for c in candidates),
            "with_affected_versions": sum(c["affected_versions_present"] for c in candidates),
            "with_cve_ids": sum(bool(c["cve_ids"]) for c in candidates),
            "with_source_url": sum(c["source_url_state"] == "http_url" for c in candidates),
            "with_local_source_file": sum(c["source_file_state"] == "file" for c in candidates),
            "missing_or_unlinked_source_file": sum(c["source_file_state"] in {"missing", "outside_repository"} for c in candidates),
            "potential_secret_or_credential_records": secret_record_count,
            "records_with_secret_indicators_in_verification_note": secret_note_count,
            "records_with_executable_or_network_behavior": active_code_count,
            "possible_duplicate_code_records": len(duplicate_code_ids),
            "possible_duplicate_content_records": len(duplicate_content_ids),
            "candidate_review_records": len(candidates),
            "auto_approved": 0,
        })
        return {
            "schema_version": "1.0.0",
            "mode": "read_only_static_governance_audit",
            "source": {"database": str(db_path), "database_sha256": sha256(db_path.read_bytes())},
            "summary": dict(summary),
            "distributions": {key: dict(sorted(value.items())) for key, value in distributions.items()},
            "governance_policy": {
                "legacy_verified_is_not_current_lab_validation": True,
                "content_was_executed": False,
                "network_requests_sent": False,
                "raw_source_content_exported": False,
                "raw_verification_notes_exported": False,
                "potential_secrets_are_redacted_to_indicator_types": True,
                "outside_repository_paths_were_not_read": True,
                "auto_approval": False,
                "all_records_remain_candidate": True,
            },
            "candidates": candidates,
        }
    finally:
        conn.close()


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--db", type=Path, default=DEFAULT_DB)
    parser.add_argument("--repo-root", type=Path, default=ROOT)
    parser.add_argument("--output", type=Path, default=DEFAULT_OUT)
    args = parser.parse_args()
    report = build_report(args.db, args.repo_root)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({"output": str(args.output), "summary": report["summary"], "distributions": report["distributions"], "raw_content_exported": False}, ensure_ascii=False, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
