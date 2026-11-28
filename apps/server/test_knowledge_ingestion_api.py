#!/usr/bin/env python3
"""Contract tests for the authenticated Knowledge Ingestion API helpers."""
from __future__ import annotations

import importlib.util
import json
import subprocess
import unittest
from pathlib import Path
from unittest.mock import patch

SERVER = Path(__file__).with_name("rp_server.py")
spec = importlib.util.spec_from_file_location("rp_server_knowledge_ingestion_test", SERVER)
assert spec and spec.loader
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


class KnowledgeIngestionApiTests(unittest.TestCase):
    def test_report_submission_passes_validated_source_and_platform_dsn(self) -> None:
        body = {
            "workspace_id": "default",
            "idempotency_key": "client-request-123",
            "source": {"kind": "research_report", "report_id": "rep-example-1"},
        }
        payload = {"ingestion_id": "ing-example-1", "status": "REVIEW_PENDING"}
        with (
            patch.object(module, "BATCH_REVIEW_DSN", "postgres://isolated/test"),
            patch.object(module, "KNOWLEDGE_INGESTION_BIN", "/tmp/rp-ingest"),
            patch.object(module, "REPO_ROOT", "/tmp"),
            patch.object(
                module.subprocess,
                "run",
                return_value=subprocess.CompletedProcess([], 0, json.dumps(payload), ""),
            ) as run,
        ):
            result = module.knowledge_ingestion_post(body, "test-caller")

        self.assertEqual(result, payload)
        command = run.call_args.args[0]
        self.assertEqual(command[2:4], ["--report-id", "rep-example-1"])
        self.assertIn("client-request-123", command)
        self.assertIn("test-caller", command)
        self.assertEqual(run.call_args.kwargs["env"]["RP_PG_DSN"], "postgres://isolated/test")

    def test_unsupported_source_is_rejected_before_subprocess(self) -> None:
        body = {
            "workspace_id": "default",
            "idempotency_key": "key",
            "source": {"kind": "url", "url": "https://example.invalid"},
        }
        with patch.object(module.subprocess, "run") as run:
            with self.assertRaises(module.ApiError) as caught:
                module.knowledge_ingestion_post(body, "test-caller")
        self.assertEqual(caught.exception.status, 400)
        run.assert_not_called()

    def test_document_revision_submission_passes_document_and_revision_ids(self) -> None:
        body = {
            "workspace_id": "default",
            "idempotency_key": "document-request-123",
            "source": {
                "kind": "document_revision",
                "document_id": "doc-example-1",
                "revision_id": "drev-example-1",
            },
        }
        payload = {"ingestion_id": "ing-example-1", "status": "REVIEW_PENDING"}
        with (
            patch.object(module, "BATCH_REVIEW_DSN", "postgres://isolated/test"),
            patch.object(module, "KNOWLEDGE_INGESTION_BIN", "/tmp/rp-ingest"),
            patch.object(module, "REPO_ROOT", "/tmp"),
            patch.object(
                module.subprocess,
                "run",
                return_value=subprocess.CompletedProcess([], 0, json.dumps(payload), ""),
            ) as run,
        ):
            result = module.knowledge_ingestion_post(body, "test-caller")

        self.assertEqual(result, payload)
        command = run.call_args.args[0]
        self.assertEqual(command[1:8], [
            "document", "--document-id", "doc-example-1",
            "--revision-id", "drev-example-1", "--workspace", "default",
        ])
        self.assertIn("document-request-123", command)

    def test_document_revision_requires_both_valid_source_ids(self) -> None:
        body = {
            "workspace_id": "default",
            "idempotency_key": "key",
            "source": {
                "kind": "document_revision",
                "document_id": "doc-valid",
                "revision_id": "wrong-prefix",
            },
        }
        with patch.object(module.subprocess, "run") as run:
            with self.assertRaises(module.ApiError) as caught:
                module.knowledge_ingestion_post(body, "test-caller")
        self.assertEqual(caught.exception.status, 400)
        self.assertEqual(caught.exception.code, "bad_revision_id")
        run.assert_not_called()

    def test_unparsed_document_revision_maps_to_conflict(self) -> None:
        body = {
            "workspace_id": "default",
            "idempotency_key": "unparsed-document",
            "source": {
                "kind": "document_revision",
                "document_id": "doc-valid",
                "revision_id": "drev-valid",
            },
        }
        failed = subprocess.CompletedProcess(
            [], 1, "",
            '{"error":"document revision must be parsed before ingestion (parse_state=DISCOVERED)"}',
        )
        with (
            patch.object(module, "BATCH_REVIEW_DSN", "postgres://isolated/test"),
            patch.object(module, "KNOWLEDGE_INGESTION_BIN", "/tmp/rp-ingest"),
            patch.object(module, "REPO_ROOT", "/tmp"),
            patch.object(module.subprocess, "run", return_value=failed),
        ):
            with self.assertRaises(module.ApiError) as caught:
                module.knowledge_ingestion_post(body, "test-caller")
        self.assertEqual(caught.exception.status, 409)
        self.assertEqual(caught.exception.code, "ingestion_source_not_parsed")

    def test_idempotency_conflict_maps_to_http_409(self) -> None:
        body = {
            "workspace_id": "default",
            "idempotency_key": "key",
            "source": {"kind": "research_report", "report_id": "rep-example-1"},
        }
        failed = subprocess.CompletedProcess(
            [], 1, "", '{"error":"ingestion failed: invalid: idempotency_key was already used"}\nusage'
        )
        with (
            patch.object(module, "BATCH_REVIEW_DSN", "postgres://isolated/test"),
            patch.object(module, "KNOWLEDGE_INGESTION_BIN", "/tmp/rp-ingest"),
            patch.object(module, "REPO_ROOT", "/tmp"),
            patch.object(module.subprocess, "run", return_value=failed),
        ):
            with self.assertRaises(module.ApiError) as caught:
                module.knowledge_ingestion_post(body, "test-caller")
        self.assertEqual(caught.exception.status, 409)

    def test_existing_task_workspace_conflict_maps_to_http_409(self) -> None:
        body = {
            "workspace_id": "default",
            "idempotency_key": "key",
            "source": {"kind": "research_report", "report_id": "rep-example-1"},
        }
        failed = subprocess.CompletedProcess(
            [], 1, "", '{"error":"ingestion failed: invalid: task already belongs to workspace private"}\nusage'
        )
        with (
            patch.object(module, "BATCH_REVIEW_DSN", "postgres://isolated/test"),
            patch.object(module, "KNOWLEDGE_INGESTION_BIN", "/tmp/rp-ingest"),
            patch.object(module, "REPO_ROOT", "/tmp"),
            patch.object(module.subprocess, "run", return_value=failed),
        ):
            with self.assertRaises(module.ApiError) as caught:
                module.knowledge_ingestion_post(body, "test-caller")
        self.assertEqual(caught.exception.status, 409)

    def test_ingestion_status_requires_workspace_scoped_identifier(self) -> None:
        with self.assertRaises(module.ApiError) as caught:
            module.knowledge_ingestion_status("invalid", "default")
        self.assertEqual(caught.exception.status, 400)

    def test_candidate_evidence_is_workspace_scoped_and_uses_cli(self) -> None:
        payload = {
            "candidate_id": "ingc-example",
            "evidence": [{"source_text_untrusted": True}],
        }
        with (
            patch.object(module, "BATCH_REVIEW_DSN", "postgres://isolated/test"),
            patch.object(module, "KNOWLEDGE_INGESTION_BIN", "/tmp/rp-ingest"),
            patch.object(module, "REPO_ROOT", "/tmp"),
            patch.object(
                module.subprocess,
                "run",
                return_value=subprocess.CompletedProcess([], 0, json.dumps(payload), ""),
            ) as run,
        ):
            result = module.knowledge_ingestion_candidate(
                "ing-example", "ingc-example", "default"
            )

        self.assertEqual(result, payload)
        self.assertEqual(run.call_args.args[0][1:], [
            "candidate", "--ingestion-id", "ing-example",
            "--candidate-id", "ingc-example", "--workspace", "default",
        ])

    def test_candidate_evidence_rejects_bad_ids_and_non_default_workspace(self) -> None:
        for ingestion_id, candidate_id, workspace, expected_status in [
            ("bad", "ingc-valid", "default", 400),
            ("ing-valid", "bad", "default", 400),
            ("ing-valid", "ingc-valid", "private", 403),
        ]:
            with self.subTest(ingestion_id=ingestion_id, candidate_id=candidate_id):
                with patch.object(module.subprocess, "run") as run:
                    with self.assertRaises(module.ApiError) as caught:
                        module.knowledge_ingestion_candidate(
                            ingestion_id, candidate_id, workspace
                        )
                self.assertEqual(caught.exception.status, expected_status)
                run.assert_not_called()

    def test_api_fails_closed_for_non_default_workspace_until_acl_exists(self) -> None:
        body = {
            "workspace_id": "private-workspace",
            "idempotency_key": "key",
            "source": {"kind": "research_report", "report_id": "rep-example-1"},
        }
        with patch.object(module.subprocess, "run") as run:
            with self.assertRaises(module.ApiError) as caught:
                module.knowledge_ingestion_post(body, "test-caller")
        self.assertEqual(caught.exception.status, 403)
        run.assert_not_called()


if __name__ == "__main__":
    unittest.main(verbosity=2)
