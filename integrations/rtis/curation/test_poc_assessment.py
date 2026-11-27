from __future__ import annotations

import hashlib
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))

from poc_assessment import (  # noqa: E402
    ASSESSMENT_DEFAULTS,
    validate_assessment_defaults,
    validate_local_artifact_hash,
    validate_poc_assessment,
)


class PocAssessmentTests(unittest.TestCase):
    def test_defaults_are_explicitly_unassessed(self) -> None:
        validate_assessment_defaults(dict(ASSESSMENT_DEFAULTS))
        invalid = dict(ASSESSMENT_DEFAULTS, artifact_validity="valid")
        with self.assertRaisesRegex(ValueError, "explicit unassessed states"):
            validate_assessment_defaults(invalid)

    def test_complete_evidence_requires_a_gap_free_hashed_checklist(self) -> None:
        assessment = {
            "evidence_completeness": "complete",
            "evidence_refs": ["evidence/result.json"],
            "evidence_artifacts": [
                {
                    "ref": "evidence/result.json",
                    "sha256": "a" * 64,
                    "kind": "raw_request_response",
                    "captured_at": "2026-11-27T10:00:00Z",
                }
            ],
            "evidence_checklist": [
                {
                    "requirement": "raw_response",
                    "status": "present",
                    "evidence_refs": ["evidence/result.json"],
                }
            ],
            "assessed_at": "2026-11-27T10:10:00Z",
            "reviewer": "reviewer-1",
        }
        validate_poc_assessment(assessment, has_missing_evidence=False)
        with self.assertRaisesRegex(ValueError, "no declared gaps"):
            validate_poc_assessment(assessment, has_missing_evidence=True)
        assessment["evidence_checklist"][0]["status"] = "missing"
        with self.assertRaisesRegex(ValueError, "no declared gaps"):
            validate_poc_assessment(assessment, has_missing_evidence=False)

    def test_valid_artifact_requires_reproduction_and_evidence(self) -> None:
        assessment = {
            "artifact_validity": "valid",
            "evidence_refs": ["evidence/result.json"],
            "evidence_artifacts": [
                {
                    "ref": "evidence/result.json",
                    "sha256": "b" * 64,
                    "kind": "independent_reproduction",
                    "captured_at": "2026-11-27T10:00:00Z",
                }
            ],
            "reproduction_conditions": ["isolated test fixture"],
            "reproduction_steps": ["run documented check"],
            "expected_result": "expected observation",
            "actual_result": "observed result",
            "assessed_at": "2026-11-27T10:10:00Z",
            "reviewer": "reviewer-1",
        }
        validate_poc_assessment(assessment, has_missing_evidence=False)
        del assessment["reproduction_steps"]
        with self.assertRaisesRegex(ValueError, "conditions, steps"):
            validate_poc_assessment(assessment, has_missing_evidence=False)

    def test_approved_assessment_requires_auditable_review_metadata(self) -> None:
        assessment = {
            "review_status": "approved",
            "artifact_validity": "not_assessed",
        }
        with self.assertRaisesRegex(ValueError, "reviewer and review_reason"):
            validate_poc_assessment(assessment, has_missing_evidence=False)
        assessment.update(
            reviewer="reviewer-1",
            review_reason="reviewed evidence package",
            reviewed_at="2026-11-27T10:10:00Z",
        )
        with self.assertRaisesRegex(ValueError, "without an assessed claim"):
            validate_poc_assessment(assessment, has_missing_evidence=False)



    def test_malformed_state_types_fail_as_validation_errors(self) -> None:
        for field in ("evidence_completeness", "artifact_validity", "remediation_status", "license_status"):
            with self.subTest(field=field):
                with self.assertRaisesRegex(ValueError, f"invalid {field}"):
                    validate_poc_assessment({field: {}}, has_missing_evidence=False)
        with self.assertRaisesRegex(ValueError, "review_status"):
            validate_poc_assessment({"review_status": []}, has_missing_evidence=False)

    def test_hashed_artifact_must_be_referenced_by_evidence_refs(self) -> None:
        assessment = {
            "evidence_refs": ["evidence/other.json"],
            "evidence_artifacts": [
                {
                    "ref": "evidence/result.json",
                    "sha256": "a" * 64,
                    "kind": "raw_request_response",
                    "captured_at": "2026-11-27T10:00:00Z",
                }
            ],
        }
        with self.assertRaisesRegex(ValueError, "must also appear in evidence_refs"):
            validate_poc_assessment(assessment, has_missing_evidence=False)

    def test_remote_evidence_url_cannot_masquerade_as_a_locally_hashed_artifact(self) -> None:
        assessment = {
            "evidence_refs": ["https://evidence.example/report"],
            "evidence_artifacts": [
                {
                    "ref": "https://evidence.example/report",
                    "sha256": "d" * 64,
                    "kind": "advisory",
                    "captured_at": "2026-11-27T10:00:00Z",
                }
            ],
        }
        with self.assertRaisesRegex(ValueError, "local bytes"):
            validate_poc_assessment(assessment, has_missing_evidence=False)

    def test_rejected_assessment_requires_auditable_review_metadata(self) -> None:
        assessment = {"review_status": "rejected"}
        with self.assertRaisesRegex(ValueError, "rejected POC assessment requires reviewer"):
            validate_poc_assessment(assessment, has_missing_evidence=False)
        assessment.update(
            reviewer="reviewer-1",
            review_reason="source integrity could not be established",
            reviewed_at="2026-11-27T10:10:00Z",
        )
        validate_poc_assessment(assessment, has_missing_evidence=False)

    def test_fixed_remediation_claim_requires_product_versions_and_hashed_evidence(self) -> None:
        assessment = {
            "remediation_status": "fixed",
            "affected_product": "Example Product",
            "affected_versions": ["1.2.3"],
            "evidence_refs": ["evidence/advisory.json"],
            "evidence_artifacts": [
                {
                    "ref": "evidence/advisory.json",
                    "sha256": "e" * 64,
                    "kind": "advisory",
                    "captured_at": "2026-11-27T10:00:00Z",
                }
            ],
            "assessed_at": "2026-11-27T10:10:00Z",
            "reviewer": "reviewer-1",
        }
        validate_poc_assessment(assessment, has_missing_evidence=False)
        del assessment["affected_versions"]
        with self.assertRaisesRegex(ValueError, "product, versions and hashed evidence"):
            validate_poc_assessment(assessment, has_missing_evidence=False)

    def test_license_claim_requires_expression_and_hashed_evidence(self) -> None:
        assessment = {
            "license_status": "verified",
            "license_expression": "MIT",
            "evidence_refs": ["evidence/license.txt"],
            "evidence_artifacts": [
                {
                    "ref": "evidence/license.txt",
                    "sha256": "f" * 64,
                    "kind": "license",
                    "captured_at": "2026-11-27T10:00:00Z",
                }
            ],
            "assessed_at": "2026-11-27T10:10:00Z",
            "reviewer": "reviewer-1",
        }
        validate_poc_assessment(assessment, has_missing_evidence=False)
        del assessment["license_expression"]
        with self.assertRaisesRegex(ValueError, "expression and hashed evidence"):
            validate_poc_assessment(assessment, has_missing_evidence=False)

    def test_local_evidence_hash_is_verified_without_parsing_content(self) -> None:
        with tempfile.TemporaryDirectory(prefix="poc-assessment-hash-") as tmp:
            path = Path(tmp) / "evidence.bin"
            payload = b"fixture evidence bytes"
            path.write_bytes(payload)
            digest = hashlib.sha256(payload).hexdigest()
            validate_local_artifact_hash(path, digest)
            with self.assertRaisesRegex(ValueError, "SHA-256 mismatch"):
                validate_local_artifact_hash(path, "c" * 64)


if __name__ == "__main__":
    unittest.main()
