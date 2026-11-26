from __future__ import annotations

import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

SCRIPT = Path(__file__).with_name("build_cisa_kev_package.py")


class CisaKevPackageTests(unittest.TestCase):
    def test_package_requires_clean_pinned_checkout_and_copies_only_feed(self) -> None:
        with tempfile.TemporaryDirectory(prefix="cisa-kev-package-test-") as tmp:
            root = Path(tmp)
            source = root / "source"
            source.mkdir()
            subprocess.run(["git", "init", "-q", str(source)], check=True)
            subprocess.run(["git", "-C", str(source), "config", "user.name", "Test"], check=True)
            subprocess.run(["git", "-C", str(source), "config", "user.email", "test@example.invalid"], check=True)
            (source / "README.md").write_text("synthetic source\n", encoding="utf-8")
            (source / "known_exploited_vulnerabilities.json").write_text(json.dumps({
                "catalogVersion": "2099.01.01",
                "dateReleased": "2099-01-01T00:00:00Z",
                "count": 1,
                "vulnerabilities": [{"cveID": "CVE-2099-12345"}],
            }), encoding="utf-8")
            subprocess.run(["git", "-C", str(source), "add", "README.md", "known_exploited_vulnerabilities.json"], check=True)
            subprocess.run(["git", "-C", str(source), "commit", "-q", "-m", "fixture"], check=True)
            commit = subprocess.run(["git", "-C", str(source), "rev-parse", "HEAD"], check=True, capture_output=True, text=True).stdout.strip()
            output = root / "package"

            result = subprocess.run(
                [sys.executable, str(SCRIPT), "--source-dir", str(source), "--output-dir", str(output), "--expected-commit", commit],
                check=True, capture_output=True, text=True,
            )
            report = json.loads(result.stdout)
            self.assertEqual(report["records"], 1)
            self.assertFalse(report["execution_performed"])
            self.assertEqual(sorted(p.name for p in output.iterdir()), ["README.md", "known_exploited_vulnerabilities.json"])
            self.assertIn(commit, (output / "README.md").read_text(encoding="utf-8"))

            exists = subprocess.run(
                [sys.executable, str(SCRIPT), "--source-dir", str(source), "--output-dir", str(output), "--expected-commit", commit],
                capture_output=True, text=True,
            )
            self.assertNotEqual(exists.returncode, 0)
            self.assertIn("output directory already exists", exists.stderr)

            (source / "README.md").write_text("modified\n", encoding="utf-8")
            dirty = subprocess.run(
                [sys.executable, str(SCRIPT), "--source-dir", str(source), "--output-dir", str(root / "dirty-package"), "--expected-commit", commit],
                capture_output=True, text=True,
            )
            self.assertNotEqual(dirty.returncode, 0)
            self.assertIn("source checkout is dirty", dirty.stderr)


if __name__ == "__main__":
    unittest.main(verbosity=2)
