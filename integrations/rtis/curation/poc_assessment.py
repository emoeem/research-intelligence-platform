"""Validation rules for additive POC assessment metadata."""
from __future__ import annotations

from datetime import datetime
import hashlib
from pathlib import Path
from typing import Any


ASSESSMENT_DEFAULTS = {
    "schema_version": 1,
    "evidence_completeness": "not_assessed",
    "artifact_validity": "not_assessed",
    "remediation_status": "unknown",
    "license_status": "unknown",
}

_EVIDENCE_COMPLETENESS = {"not_assessed", "incomplete", "complete"}
_ARTIFACT_VALIDITY = {
    "not_assessed",
    "candidate",
    "valid",
    "invalid",
    "superseded",
    "disputed",
}
_REMEDIATION_STATUS = {
    "unknown",
    "affected",
    "fixed",
    "partially_fixed",
    "not_applicable",
    "disputed",
}
_LICENSE_STATUS = {
    "unknown",
    "identified",
    "verified",
    "restricted",
    "prohibited",
    "disputed",
}
_REVIEW_STATUS = {"candidate", "approved", "rejected"}
_ARTIFACT_KINDS = {
    "source",
    "reproduction",
    "raw_request_response",
    "negative_control",
    "independent_reproduction",
    "advisory",
    "license",
    "other",
}
_HEX = frozenset("0123456789abcdef")


def _iso_timestamp(value: Any, field: str) -> None:
    if not isinstance(value, str) or not value.strip():
        raise ValueError(f"{field} must be a non-empty ISO-8601 timestamp")
    try:
        parsed = datetime.fromisoformat(value.replace("Z", "+00:00"))
    except ValueError as exc:
        raise ValueError(f"{field} must be an ISO-8601 timestamp") from exc
    if parsed.tzinfo is None or parsed.utcoffset() is None:
        raise ValueError(f"{field} must include a timezone")


def validate_assessment_defaults(value: Any) -> None:
    if value != ASSESSMENT_DEFAULTS:
        raise ValueError("poc_assessment_defaults must declare the explicit unassessed states")


def validate_local_artifact_hash(path: Path, expected_sha256: str) -> None:
    try:
        actual_sha256 = hashlib.sha256(path.read_bytes()).hexdigest()
    except OSError as exc:
        raise ValueError(f"cannot hash evidence artifact {path}: {exc}") from exc
    if actual_sha256 != expected_sha256:
        raise ValueError(f"evidence artifact SHA-256 mismatch: {path}")


def validate_poc_assessment(
    value: Any,
    *,
    has_missing_evidence: bool,
) -> None:
    if not isinstance(value, dict):
        raise ValueError("poc_assessment must be an object")

    states = {
        "evidence_completeness": (value.get("evidence_completeness", "not_assessed"), _EVIDENCE_COMPLETENESS),
        "artifact_validity": (value.get("artifact_validity", "not_assessed"), _ARTIFACT_VALIDITY),
        "remediation_status": (value.get("remediation_status", "unknown"), _REMEDIATION_STATUS),
        "license_status": (value.get("license_status", "unknown"), _LICENSE_STATUS),
    }
    for field, (state, allowed) in states.items():
        if not isinstance(state, str) or state not in allowed:
            raise ValueError(f"invalid {field}: {state!r}")

    refs = value.get("evidence_refs", [])
    if not isinstance(refs, list) or any(not isinstance(ref, str) or not ref.strip() for ref in refs):
        raise ValueError("poc_assessment.evidence_refs must be an array of non-empty strings")
    if len(refs) != len(set(refs)):
        raise ValueError("poc_assessment.evidence_refs must be unique")

    artifacts = value.get("evidence_artifacts", [])
    if not isinstance(artifacts, list):
        raise ValueError("poc_assessment.evidence_artifacts must be an array")
    artifact_refs: set[str] = set()
    for artifact in artifacts:
        if not isinstance(artifact, dict):
            raise ValueError("poc_assessment evidence artifact must be an object")
        ref = artifact.get("ref")
        digest = artifact.get("sha256")
        kind = artifact.get("kind")
        if not isinstance(ref, str) or not ref.strip():
            raise ValueError("poc_assessment evidence artifact requires ref")
        if ref.startswith(("http://", "https://")):
            raise ValueError(
                "evidence_artifacts must reference local bytes so SHA-256 can be verified; "
                "use evidence_refs for remote provenance URLs"
            )
        if ref in artifact_refs:
            raise ValueError(f"duplicate poc_assessment evidence artifact ref: {ref}")
        artifact_refs.add(ref)
        if (
            not isinstance(digest, str)
            or len(digest) != 64
            or any(char not in _HEX for char in digest)
        ):
            raise ValueError(f"evidence artifact sha256 must be 64 lowercase hex characters: {ref}")
        if not isinstance(kind, str) or kind not in _ARTIFACT_KINDS:
            raise ValueError(f"invalid evidence artifact kind for {ref}: {kind!r}")
        _iso_timestamp(artifact.get("captured_at"), f"captured_at for {ref}")

    if not artifact_refs.issubset(set(refs)):
        raise ValueError("every hashed evidence artifact must also appear in evidence_refs")

    completeness = states["evidence_completeness"][0]
    validity = states["artifact_validity"][0]
    remediation = states["remediation_status"][0]
    license_status = states["license_status"][0]
    checklist = value.get("evidence_checklist", [])
    if not isinstance(checklist, list):
        raise ValueError("poc_assessment.evidence_checklist must be an array")
    checklist_items: set[str] = set()
    checklist_missing = False
    for item in checklist:
        if not isinstance(item, dict):
            raise ValueError("poc_assessment evidence checklist item must be an object")
        requirement = item.get("requirement")
        state = item.get("status")
        if not isinstance(requirement, str) or not requirement.strip() or requirement in checklist_items:
            raise ValueError("evidence checklist requirements must be non-empty and unique")
        checklist_items.add(requirement)
        if not isinstance(state, str) or state not in {"present", "missing", "not_applicable"}:
            raise ValueError(f"invalid evidence checklist status for {requirement}: {state!r}")
        if state == "missing":
            checklist_missing = True
        if state == "present":
            item_refs = item.get("evidence_refs")
            if (
                not isinstance(item_refs, list)
                or not item_refs
                or any(ref not in refs or ref not in artifact_refs for ref in item_refs)
            ):
                raise ValueError(
                    f"present evidence checklist item requires hashed evidence_refs: {requirement}"
                )

    if completeness == "complete" and (
        has_missing_evidence
        or checklist_missing
        or not checklist
        or not any(item.get("status") == "present" for item in checklist)
        or not refs
        or not artifacts
    ):
        raise ValueError(
            "complete evidence assessment requires a full checklist, no declared gaps, evidence_refs and hashed artifacts"
        )
    if completeness == "incomplete" and not (has_missing_evidence or checklist_missing):
        raise ValueError(
            "incomplete evidence assessment requires an explicit missing-evidence item"
        )
    if validity in {"valid", "invalid", "superseded", "disputed"} and (
        not refs or not artifacts or not value.get("actual_result")
    ):
        raise ValueError(
            f"{validity} artifact validity requires evidence_refs, hashed artifacts and actual_result"
        )
    if validity == "valid":
        conditions = value.get("reproduction_conditions")
        steps = value.get("reproduction_steps")
        if (
            not isinstance(conditions, list)
            or not conditions
            or any(not isinstance(item, str) or not item.strip() for item in conditions)
            or not isinstance(steps, list)
            or not steps
            or any(not isinstance(item, str) or not item.strip() for item in steps)
            or not isinstance(value.get("expected_result"), str)
            or not value["expected_result"].strip()
        ):
            raise ValueError(
                "valid artifact assessment requires reproduction conditions, steps and expected_result"
            )

    remediation_claim = remediation != "unknown"
    if remediation_claim:
        product = value.get("affected_product")
        versions = value.get("affected_versions")
        if (
            not isinstance(product, str)
            or not product.strip()
            or not isinstance(versions, list)
            or not versions
            or any(not isinstance(version, str) or not version.strip() for version in versions)
            or len(versions) != len(set(versions))
            or not refs
            or not artifacts
        ):
            raise ValueError(
                f"{remediation} remediation assessment requires product, versions and hashed evidence"
            )

    if license_status != "unknown" and (
        not isinstance(value.get("license_expression"), str)
        or not value["license_expression"].strip()
        or not refs
        or not artifacts
    ):
        raise ValueError(
            f"{license_status} license assessment requires an expression and hashed evidence"
        )

    claimed = (
        completeness != "not_assessed"
        or validity in {"valid", "invalid", "superseded", "disputed"}
        or remediation_claim
        or license_status != "unknown"
    )
    if claimed:
        _iso_timestamp(value.get("assessed_at"), "poc_assessment.assessed_at")
        reviewer = value.get("reviewer")
        if not isinstance(reviewer, str) or not reviewer.strip():
            raise ValueError("assessed POC claims require a reviewer")

    review_status = value.get("review_status", "candidate")
    if not isinstance(review_status, str) or review_status not in _REVIEW_STATUS:
        raise ValueError(f"invalid poc_assessment review_status: {review_status!r}")
    if review_status in {"approved", "rejected"}:
        reviewer = value.get("reviewer")
        reason = value.get("review_reason")
        if not isinstance(reviewer, str) or not reviewer.strip() or not isinstance(reason, str) or not reason.strip():
            raise ValueError(f"{review_status} POC assessment requires reviewer and review_reason")
        _iso_timestamp(value.get("reviewed_at"), "poc_assessment.reviewed_at")
        if review_status == "approved" and not claimed:
            raise ValueError("cannot approve a POC assessment without an assessed claim")
