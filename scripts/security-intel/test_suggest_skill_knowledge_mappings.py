import importlib.util
import json
import sqlite3
import tempfile
import unittest
from pathlib import Path

SCRIPT = Path(__file__).with_name("suggest_skill_knowledge_mappings.py")
SPEC = importlib.util.spec_from_file_location("skill_knowledge_suggester", SCRIPT)
MODULE = importlib.util.module_from_spec(SPEC)
assert SPEC and SPEC.loader
SPEC.loader.exec_module(MODULE)


class SkillKnowledgeMappingTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        (self.root / "skills").mkdir()
        (self.root / "skills" / "log4j.md").write_text(
            "# Log4j\nCVE-2021-44228 is relevant here.\n",
            encoding="utf-8",
        )
        self.platform = self.root / "platform.db"
        conn = sqlite3.connect(self.platform)
        conn.execute("""CREATE TABLE skill (
            skill_id TEXT, version TEXT, body_ref TEXT, enabled INTEGER,
            governance TEXT, source_system TEXT, source_pk TEXT
        )""")
        conn.execute(
            "INSERT INTO skill VALUES (?,?,?,?,?,?,?)",
            ("framework-log4j2", "0.2.0", "skills/log4j.md", 1, "ACTIVE", "test-skills", "log4j.md"),
        )
        conn.commit()
        conn.close()

        self.plugin = self.root / "plugin.db"
        conn = sqlite3.connect(self.plugin)
        conn.execute("""CREATE TABLE knowledge_source_entry (
            plugin_id TEXT, plugin_version TEXT, item_key TEXT, title TEXT,
            content_sha256 TEXT, source_url TEXT, source_commit TEXT,
            review_status TEXT, license_status TEXT, meta_json TEXT, body_bytes INTEGER
        )""")
        conn.execute(
            "INSERT INTO knowledge_source_entry VALUES (?,?,?,?,?,?,?,?,?,?,?)",
            (
                "knowledge.nuclei-templates", "0.1.1", "http/cves/2021/CVE-2021-44228.yaml",
                "Apache Log4j RCE", "a" * 64, "https://github.com/projectdiscovery/nuclei-templates",
                "commit-a", "APPROVED", "MIT",
                json.dumps({"nuclei_template": {"classification": {"cve-id": "CVE-2021-44228"}}}), 1200,
            ),
        )
        conn.execute(
            "INSERT INTO knowledge_source_entry VALUES (?,?,?,?,?,?,?,?,?,?,?)",
            (
                "knowledge.cisa-kev", "0.1.2", "CVE-2021-44228", "Log4Shell",
                "b" * 64, "https://github.com/cisagov/kev-data", "commit-b",
                "APPROVED", "CC0-1.0", json.dumps({"cve_record": {"cve_id": "CVE-2021-44228"}}), 400,
            ),
        )
        conn.commit()
        conn.close()

    def tearDown(self):
        self.temp.cleanup()

    def test_exact_cve_match_emits_vulnerability_and_template_candidates_read_only(self):
        before_platform = self.platform.read_bytes()
        before_plugin = self.plugin.read_bytes()
        report = MODULE.build_report(self.platform, self.plugin, self.root)
        self.assertEqual(self.platform.read_bytes(), before_platform)
        self.assertEqual(self.plugin.read_bytes(), before_plugin)
        self.assertEqual(report["summary"]["skill_cve_pairs"], 1)
        self.assertEqual(report["summary"]["skill_vulnerability_candidates"], 1)
        self.assertEqual(report["summary"]["skill_template_candidates"], 1)
        self.assertEqual(len(report["candidates"]), 2)
        vuln = next(x for x in report["candidates"] if x["target_type"] == "vulnerability")
        template = next(x for x in report["candidates"] if x["target_type"] == "detection_template")
        self.assertEqual(vuln["target_source"]["source_system"], "knowledge.cisa-kev")
        self.assertEqual(template["target_source"]["content_sha256"], "a" * 64)
        self.assertEqual(template["evidence"][0]["line"], 2)
        self.assertEqual(vuln["review_status"], "CANDIDATE")
        self.assertFalse(report["governance"]["auto_approved"])


if __name__ == "__main__":
    unittest.main()
