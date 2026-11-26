from __future__ import annotations

import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

SCRIPT = Path(__file__).with_name("build_pocindex_kev_package.py")
COMMIT = "e9076fea96095e014fc107c02dd23ee8dc21476d"


class PocIndexKevPackageTests(unittest.TestCase):
    def test_build_filters_to_cisa_kev_and_preserves_score_provenance(self) -> None:
        with tempfile.TemporaryDirectory(prefix="pocindex-kev-test-") as tmp:
            root = Path(tmp)
            source = root / "source"
            source.mkdir()
            (source / "kev.json").write_text(json.dumps({"CVE-2099-12345": ["2099-01-01", 0]}), encoding="utf-8")
            (source / "epss.json").write_text(json.dumps({
                "CVE-2099-12345": [0.9, 0.99], "CVE-2099-12346": [0.1, 0.2],
            }), encoding="utf-8")
            (source / "cve_metadata.json").write_text(json.dumps({
                "CVE-2099-12345": {
                    "cvss": [["3.1", 9.8, "CRITICAL", "CVSS:3.1/AV:N", "nvd@nist.gov", "Primary"]],
                    "advisories": [["https://example.com/advisory", ["Vendor Advisory"]]],
                },
            }), encoding="utf-8")
            (source / "nuclei.json").write_text(json.dumps({
                "CVE-2099-12345": {"severity": "critical", "cvss": 9.8, "cvss_vector": "CVSS:3.1/AV:N", "cwe": "CWE-78"},
            }), encoding="utf-8")
            cisa = root / "cisa.json"
            cisa.write_text(json.dumps({
                "catalogVersion": "2099.01.01",
                "count": 2,
                "vulnerabilities": [
                    {"cveID": "CVE-2099-12345", "dateAdded": "2099-01-01"},
                    {"cveID": "CVE-2099-12346", "dateAdded": "2099-01-02"},
                ],
            }), encoding="utf-8")
            output = root / "package"
            result = subprocess.run(
                [sys.executable, str(SCRIPT), "--source-dir", str(source), "--cisa-json", str(cisa),
                 "--output-dir", str(output), "--source-commit", COMMIT],
                check=True, capture_output=True, text=True,
            )
            report = json.loads(result.stdout)
            self.assertEqual(report["records"], 2)
            self.assertEqual(report["pocindex_kev"], 1)
            self.assertEqual(report["epss"], 2)
            self.assertEqual(report["cvss"], 1)
            self.assertEqual(report["nuclei"], 1)
            payload = json.loads((output / "pocindex-kev.json").read_text(encoding="utf-8"))
            first = next(r for r in payload["records"] if r["cve_id"] == "CVE-2099-12345")
            self.assertEqual(first["epss_score"], 0.9)
            self.assertEqual(first["cvss_assessments"][0]["source"], "nvd@nist.gov")
            self.assertEqual(first["nuclei_metadata"]["cwe"], "CWE-78")
            self.assertEqual(payload["snapshot"]["repository_commit"], COMMIT)

    def test_rejects_invalid_epss_range(self) -> None:
        with tempfile.TemporaryDirectory(prefix="pocindex-kev-invalid-") as tmp:
            root = Path(tmp); source = root / "source"; source.mkdir()
            for name, data in {
                "kev.json": {},
                "epss.json": {"CVE-2099-12345": [1.1, 0.9]},
                "cve_metadata.json": {},
                "nuclei.json": {},
            }.items():
                (source / name).write_text(json.dumps(data), encoding="utf-8")
            cisa = root / "cisa.json"
            cisa.write_text(json.dumps({"catalogVersion":"2099.01.01","count":1,"vulnerabilities":[{"cveID":"CVE-2099-12345","dateAdded":"2099-01-01"}]}), encoding="utf-8")
            result = subprocess.run(
                [sys.executable, str(SCRIPT), "--source-dir", str(source), "--cisa-json", str(cisa),
                 "--output-dir", str(root / "out"), "--source-commit", COMMIT],
                capture_output=True, text=True,
            )
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("EPSS score", result.stderr)


if __name__ == "__main__":
    unittest.main(verbosity=2)
