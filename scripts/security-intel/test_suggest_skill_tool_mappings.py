import importlib.util
import sqlite3
import tempfile
import unittest
from pathlib import Path

SCRIPT = Path(__file__).with_name("suggest_skill_tool_mappings.py")
SPEC = importlib.util.spec_from_file_location("skill_mapping_suggester", SCRIPT)
MODULE = importlib.util.module_from_spec(SPEC)
assert SPEC and SPEC.loader
SPEC.loader.exec_module(MODULE)


class SkillToolMappingTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        (self.root / "skills").mkdir()
        (self.root / "skills" / "alpha.md").write_text(
            "# 工具命令\n使用 `nmap -sV <target>` 检查服务。\n不要使用 `ffuf` 进行此步骤。\n",
            encoding="utf-8",
        )
        self.db = self.root / "platform.db"
        conn = sqlite3.connect(self.db)
        conn.executescript("""
            CREATE TABLE skill (
                skill_id TEXT, name TEXT, version TEXT, description TEXT,
                when_to_use TEXT, required_tools TEXT, knowledge_refs TEXT,
                body_ref TEXT, enabled INTEGER, governance TEXT,
                source_system TEXT, source_pk TEXT
            );
            CREATE TABLE tool (
                tool_id TEXT, name TEXT, kind TEXT, lifecycle TEXT,
                governance TEXT, install_state TEXT, health_state TEXT,
                permission_level TEXT, purpose TEXT, provenance_ref TEXT,
                source_system TEXT, source_pk TEXT, schema_version INTEGER, updated_at TEXT
            );
            INSERT INTO skill VALUES
                ('alpha','alpha','1.0','scan services','when needed',NULL,NULL,
                 'skills/alpha.md',1,'ACTIVE','test','alpha.md'),
                ('outside','outside','1.0','mentions nmap',NULL,NULL,NULL,
                 '/etc/passwd',1,'ACTIVE','test','outside.md');
            INSERT INTO tool VALUES
                ('nmap','nmap','cli','REGISTERED','ACTIVE','installed','ok','T0','scan','registry#nmap','test','nmap',1,'2026-11-27T00:00:00Z'),
                ('ffuf','ffuf','cli','REGISTERED','ACTIVE','installed','degraded','T0','fuzz','registry#ffuf','test','ffuf',1,'2026-11-27T00:00:00Z');
        """)
        conn.commit()
        conn.close()

    def tearDown(self):
        self.temp.cleanup()

    def test_candidates_keep_evidence_and_do_not_promote_or_read_outside_root(self):
        before = self.db.read_bytes()
        report = MODULE.build_report(self.db, self.root)
        self.assertEqual(self.db.read_bytes(), before)
        self.assertEqual(report["summary"]["skills_total"], 2)
        self.assertEqual(report["summary"]["skills_with_readable_body"], 1)
        self.assertEqual(report["missing_bodies"], ["outside"])
        self.assertEqual(len(report["candidates"]), 2)
        candidate = next(x for x in report["candidates"] if x["skill_id"] == "alpha")
        metadata_only = next(x for x in report["candidates"] if x["skill_id"] == "outside")
        self.assertEqual(candidate["tool_id"], "nmap")
        self.assertFalse(metadata_only["activation_eligible_after_review"])
        self.assertEqual(metadata_only["match_kind"], "metadata_mention")
        self.assertEqual(candidate["review_status"], "CANDIDATE")
        self.assertTrue(candidate["activation_eligible_after_review"])
        self.assertEqual(candidate["evidence"][0]["line"], 2)
        self.assertEqual(candidate["source"]["body_ref"], "skills/alpha.md")
        self.assertEqual(len(candidate["source"]["body_sha256"]), 64)
        self.assertEqual(len(report["negative_mentions"]), 1)
        self.assertEqual(report["negative_mentions"][0]["tool_id"], "ffuf")
        self.assertTrue(report["negative_mentions"][0]["excluded_from_candidates"])
        self.assertFalse(report["governance"]["auto_approved"])


if __name__ == "__main__":
    unittest.main()
