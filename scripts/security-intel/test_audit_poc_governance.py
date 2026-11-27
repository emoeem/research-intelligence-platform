import json
import sqlite3
import tempfile
import unittest
from pathlib import Path

from audit_poc_governance import build_report


class PocGovernanceAuditTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        (self.root / "pocs").mkdir()
        self.source = self.root / "pocs" / "safe.sh"
        self.source.write_text("#!/bin/sh\necho static-check\n", encoding="utf-8")
        self.db = self.root / "rtis.db"
        conn = sqlite3.connect(self.db)
        conn.executescript("""
            CREATE TABLE poc (
                id INTEGER PRIMARY KEY, code TEXT, title TEXT, kind TEXT, category TEXT,
                cve TEXT, component TEXT, versions TEXT, severity TEXT, language TEXT,
                source TEXT, source_url TEXT, description TEXT, usage TEXT, content TEXT,
                path TEXT, verified INTEGER, verified_note TEXT, tags TEXT
            );
            CREATE TABLE k_meta (
                knowledge_id INTEGER PRIMARY KEY, normalized_type TEXT, archetypes TEXT,
                technology_tags TEXT, method_tags TEXT, evidence_level TEXT, lifecycle TEXT,
                verification_basis TEXT, dedup_key TEXT, version INTEGER
            );
        """)
        conn.execute(
            "INSERT INTO poc VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
            (1, "safe", "Safe fixture", "poc", "test", "CVE-2021-44228", "fixture", None,
             "high", "bash", "self", None, "description", "usage with secret-placeholder", "body-secret-placeholder",
             str(self.source), 1, "password=example-secret-value", "test"),
        )
        conn.execute(
            "INSERT INTO poc VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
            (2, "external", "External path fixture", "script", "test", "N/A", None, None,
             "low", "python", "web", "https://example.invalid/poc", "description", None, None,
             "/etc/passwd", 0, None, None),
        )
        conn.execute(
            "INSERT INTO poc VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
            (3, "nuclei", "Nuclei fixture", "template", "nuclei", None, None, None,
             "high", "yaml", "nuclei", None, "description", None, None, "/usr/share/template.yaml", 0, None, None),
        )
        conn.execute("INSERT INTO k_meta VALUES (1,'PoC','[]','[]','[]','K2','Researched','legacy_claimed','dup',1)")
        conn.commit()
        conn.close()

    def tearDown(self):
        self.temp.cleanup()

    def test_read_only_redacted_and_non_nuclei_only(self):
        before = self.db.read_bytes()
        report = build_report(self.db, self.root)
        self.assertEqual(self.db.read_bytes(), before)
        self.assertEqual(report["summary"]["total_records"], 2)
        self.assertEqual(report["summary"]["actual_poc_kind_records"], 1)
        candidate = report["candidates"][0]
        self.assertEqual(candidate["source_file_state"], "file")
        self.assertIn("credential_assignment", candidate["secret_indicator_types"])
        self.assertEqual(candidate["review_status"], "CANDIDATE")
        rendered = json.dumps(report)
        self.assertNotIn("example-secret-value", rendered)
        self.assertNotIn("body-secret-placeholder", rendered)
        self.assertNotIn("/etc/passwd", rendered)
        self.assertEqual(report["governance_policy"]["content_was_executed"], False)
        self.assertEqual(report["summary"]["auto_approved"], 0)


if __name__ == "__main__":
    unittest.main()
