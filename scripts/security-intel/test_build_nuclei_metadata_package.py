from __future__ import annotations

import hashlib
import json
import sqlite3
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

SCRIPT = Path(__file__).with_name("build_nuclei_metadata_package.py")
SOURCE_URL = "https://github.com/projectdiscovery/nuclei-templates"
PIN = "a346d8f00c61bf95bbd30365f12cf923be51ba3d"


class NucleiMetadataPackageTests(unittest.TestCase):
    def test_invalid_source_commit_is_rejected_before_force_replaces_output(self) -> None:
        with tempfile.TemporaryDirectory(prefix="nuclei-metadata-invalid-pin-") as tmp:
            root = Path(tmp)
            output = root / "existing-package" / "knowledge.db"
            output.parent.mkdir()
            output.write_text("preserve this output", encoding="utf-8")

            result = subprocess.run(
                [
                    sys.executable,
                    str(SCRIPT),
                    "--source-db",
                    str(root / "missing-source.db"),
                    "--output",
                    str(output),
                    "--template-root",
                    str(root / "missing-templates"),
                    "--source-commit",
                    "a346d8f",
                    "--force",
                ],
                capture_output=True,
                text=True,
            )

            self.assertNotEqual(result.returncode, 0)
            self.assertIn("full 40-character hexadecimal commit", result.stderr)
            self.assertEqual(output.read_text(encoding="utf-8"), "preserve this output")

    def test_build_is_read_only_and_excludes_unrelated_poc_fields(self) -> None:
        with tempfile.TemporaryDirectory(prefix="nuclei-metadata-test-") as tmp:
            root = Path(tmp)
            template_root = root / "templates"
            template_root.mkdir()
            template = template_root / "http" / "cves" / "CVE-2099-12345.yaml"
            template.parent.mkdir(parents=True)
            template.write_text("id: synthetic\ninfo:\n  name: synthetic\n", encoding="utf-8")

            source = root / "source.db"
            conn = sqlite3.connect(source)
            conn.execute("""CREATE TABLE poc (
                id INTEGER PRIMARY KEY, code TEXT, title TEXT, description TEXT,
                usage TEXT, severity TEXT, cve TEXT, tags TEXT, path TEXT,
                category TEXT, source TEXT, source_url TEXT, content TEXT,
                verified INTEGER, verified_note TEXT, engagement_id INTEGER,
                engagement_name TEXT, asset_target TEXT, used_on TEXT,
                found_by_agent TEXT
            )""")
            conn.execute(
                """INSERT INTO poc VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)""",
                (1, "nuclei", "Synthetic template", "metadata only", "run only in lab",
                 "critical", "CVE-2099-12345", "test", str(template), "nuclei", "nuclei",
                 SOURCE_URL, "DO-NOT-COPY-POC-BODY", 1, "DO-NOT-COPY-VERIFIED-NOTE",
                 42, "DO-NOT-COPY-ENGAGEMENT", "DO-NOT-COPY-TARGET", "DO-NOT-COPY-USED-ON",
                 "DO-NOT-COPY-AGENT"),
            )
            conn.execute(
                """INSERT INTO poc VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)""",
                (2, "legacy", "Unrelated", "", "", "high", "CVE-2000-0001", "", "/tmp/legacy.yaml",
                 "poc", "rtis", "https://example.invalid", "legacy", 1, "legacy", 1, "legacy", "legacy", "legacy", "legacy"),
            )
            conn.commit()
            conn.close()
            before = hashlib.sha256(source.read_bytes()).hexdigest()
            output = root / "package" / "knowledge.db"

            result = subprocess.run(
                [sys.executable, str(SCRIPT), "--source-db", str(source), "--output", str(output),
                 "--template-root", str(template_root), "--source-commit", PIN],
                check=True, capture_output=True, text=True,
            )
            report = json.loads(result.stdout)
            self.assertEqual(report["rows"], 1)
            self.assertEqual(report["missing_template_files"], 0)
            self.assertEqual(
                report["source_revision_verification"],
                "unverified_non_git_tree",
            )
            self.assertFalse(report["execution_performed"])
            self.assertEqual(hashlib.sha256(source.read_bytes()).hexdigest(), before)

            projected = sqlite3.connect(f"file:{output}?mode=ro", uri=True)
            columns = {row[1] for row in projected.execute("PRAGMA table_info(poc)")}
            self.assertEqual(projected.execute("SELECT count(*) FROM poc").fetchone()[0], 1)
            self.assertTrue({"id", "title", "severity", "cve", "path", "category", "source", "source_url"} <= columns)
            self.assertFalse({"content", "verified", "verified_note", "engagement_id", "engagement_name", "asset_target", "used_on", "found_by_agent"} & columns)
            self.assertEqual(projected.execute("PRAGMA integrity_check").fetchone()[0], "ok")
            projected.close()
            self.assertIn(PIN, (output.parent / "README.md").read_text(encoding="utf-8"))

            refused = subprocess.run(
                [sys.executable, str(SCRIPT), "--source-db", str(source), "--output", str(output),
                 "--template-root", str(template_root), "--source-commit", PIN],
                capture_output=True, text=True,
            )
            self.assertNotEqual(refused.returncode, 0)
            self.assertIn("output already exists", refused.stderr)

    def test_git_template_checkout_must_match_pin_before_force_replaces_output(self) -> None:
        with tempfile.TemporaryDirectory(prefix="nuclei-metadata-git-pin-") as tmp:
            root = Path(tmp)
            template_root = root / "templates"
            template_root.mkdir()
            (template_root / "template.yaml").write_text("id: sample\n", encoding="utf-8")
            subprocess.run(["git", "init", "--quiet", str(template_root)], check=True)
            subprocess.run(
                ["git", "-C", str(template_root), "add", "template.yaml"],
                check=True,
            )
            subprocess.run(
                [
                    "git",
                    "-C",
                    str(template_root),
                    "-c",
                    "user.name=Test",
                    "-c",
                    "user.email=test@example.invalid",
                    "commit",
                    "--quiet",
                    "-m",
                    "fixture",
                ],
                check=True,
            )
            source = root / "source.db"
            source.touch()
            output = root / "existing-package" / "knowledge.db"
            output.parent.mkdir()
            output.write_text("preserve this output", encoding="utf-8")

            result = subprocess.run(
                [
                    sys.executable,
                    str(SCRIPT),
                    "--source-db",
                    str(source),
                    "--output",
                    str(output),
                    "--template-root",
                    str(template_root),
                    "--source-commit",
                    "f" * 40,
                    "--force",
                ],
                capture_output=True,
                text=True,
            )

            self.assertNotEqual(result.returncode, 0)
            self.assertIn("does not match --source-commit", result.stderr)
            self.assertEqual(output.read_text(encoding="utf-8"), "preserve this output")

    def test_clean_git_checkout_is_verified_and_dirty_tree_is_rejected(self) -> None:
        with tempfile.TemporaryDirectory(prefix="nuclei-metadata-git-verified-") as tmp:
            root = Path(tmp)
            template_root = root / "templates"
            template_root.mkdir()
            template = template_root / "http" / "sample.yaml"
            template.parent.mkdir(parents=True)
            template.write_text("id: sample\ninfo:\n  name: Sample\n", encoding="utf-8")
            subprocess.run(["git", "init", "--quiet", str(template_root)], check=True)
            subprocess.run(
                ["git", "-C", str(template_root), "add", "http/sample.yaml"],
                check=True,
            )
            subprocess.run(
                [
                    "git",
                    "-C",
                    str(template_root),
                    "-c",
                    "user.name=Test",
                    "-c",
                    "user.email=test@example.invalid",
                    "commit",
                    "--quiet",
                    "-m",
                    "fixture",
                ],
                check=True,
            )
            pin = subprocess.run(
                ["git", "-C", str(template_root), "rev-parse", "HEAD"],
                check=True,
                capture_output=True,
                text=True,
            ).stdout.strip()

            source = root / "source.db"
            conn = sqlite3.connect(source)
            conn.execute("""CREATE TABLE poc (
                id INTEGER PRIMARY KEY, code TEXT, title TEXT, description TEXT,
                usage TEXT, severity TEXT, cve TEXT, tags TEXT, path TEXT,
                category TEXT, source TEXT, source_url TEXT
            )""")
            conn.execute(
                "INSERT INTO poc VALUES (1,'sample','Sample','','','low','','',?,'nuclei','nuclei',?)",
                (str(template), SOURCE_URL),
            )
            conn.commit()
            conn.close()
            output = root / "package" / "knowledge.db"

            result = subprocess.run(
                [
                    sys.executable,
                    str(SCRIPT),
                    "--source-db",
                    str(source),
                    "--output",
                    str(output),
                    "--template-root",
                    str(template_root),
                    "--source-commit",
                    pin,
                ],
                check=True,
                capture_output=True,
                text=True,
            )
            report = json.loads(result.stdout)
            self.assertEqual(report["source_revision_verification"], "verified_git_commit")

            output_before = output.read_bytes()
            template.write_text("id: modified\n", encoding="utf-8")
            dirty = subprocess.run(
                [
                    sys.executable,
                    str(SCRIPT),
                    "--source-db",
                    str(source),
                    "--output",
                    str(output),
                    "--template-root",
                    str(template_root),
                    "--source-commit",
                    pin,
                    "--force",
                ],
                capture_output=True,
                text=True,
            )
            self.assertNotEqual(dirty.returncode, 0)
            self.assertIn("modified or untracked files", dirty.stderr)
            self.assertEqual(output.read_bytes(), output_before)


if __name__ == "__main__":
    unittest.main(verbosity=2)
