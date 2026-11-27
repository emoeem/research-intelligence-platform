"""Acceptance checks for the real derived index across approved and candidate lanes."""
import json
import os
import sqlite3
import subprocess
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
INDEX = Path(os.environ.get("RIP_TEST_SEARCH_INDEX", ROOT / "rip-data/dev/asset-governance-search.db"))
BINARY = Path(os.environ.get("RP_SEARCH_INDEX_BIN", ROOT / "target/debug/rp-search-index"))


def query(*args):
    result = subprocess.run(
        [str(BINARY), "query", "--index-db", str(INDEX), *args],
        capture_output=True, text=True, check=True, timeout=20,
    )
    return json.loads(result.stdout)


class AssetSearchCoverageTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        if not INDEX.is_file() or not BINARY.is_file():
            raise unittest.SkipTest("build the disposable asset-governance search index first")

    def test_index_has_expected_approved_and_candidate_lanes(self):
        conn = sqlite3.connect(f"file:{INDEX}?mode=ro", uri=True)
        try:
            docs = conn.execute("SELECT COUNT(*) FROM search_index_doc").fetchone()[0]
            states = dict(conn.execute("SELECT governance,COUNT(*) FROM search_index_doc GROUP BY governance"))
            types = dict(conn.execute("SELECT resource_type,COUNT(*) FROM search_index_doc GROUP BY resource_type"))
        finally:
            conn.close()
        self.assertEqual(docs, 18147)
        self.assertEqual(states, {"ACTIVE": 3789, "CANDIDATE": 14358})
        self.assertEqual(types["nuclei_template"], 14111)
        self.assertEqual(types["rtis_poc"], 49)
        self.assertEqual(types["rtis_weapon_candidate"], 561)

    def test_approved_cisa_pocindex_and_nuclei_are_searchable(self):
        cisa = query("--keywords", "CVE-2021-44228", "--resource-type", "catalog_entry", "--limit", "10")
        ids = {hit["resource_id"] for hit in cisa["hits"]}
        self.assertTrue(any("knowledge.cisa-kev" in rid for rid in ids))
        self.assertTrue(any("knowledge.pocindex-kev" in rid for rid in ids))
        nuclei = query("--keywords", "CVE-2021-44228", "--resource-type", "nuclei_template", "--limit", "10")
        self.assertGreater(nuclei["total_matched"], 0)
        self.assertTrue(all(hit["governance_status"] == "ACTIVE" for hit in nuclei["hits"]))

    def test_exact_vulnerability_identity_facet_returns_typed_cross_domain_entities(self):
        result = query("--vulnerability-id", "CVE-2021-44228", "--limit", "100")
        hits = result["hits"]
        self.assertGreaterEqual(result["total_matched"], 45)
        self.assertTrue(all("CVE-2021-44228" in hit["identity"]["vulnerability_ids"] for hit in hits))
        self.assertTrue(all(hit["governance_status"] == "ACTIVE" for hit in hits))
        types = {hit["resource_type"] for hit in hits}
        self.assertIn("nuclei_template", types)
        self.assertIn("catalog_entry", types)
        source_systems = {hit["identity"]["source_system"] for hit in hits}
        self.assertIn("knowledge.cisa-kev", source_systems)
        self.assertIn("knowledge.pocindex-kev", source_systems)

    def test_rtis_candidate_is_hidden_by_default_and_visible_on_opt_in(self):
        args = ("--keywords", "sourcemap", "--resource-type", "rtis_weapon_candidate", "--limit", "10")
        default = query(*args)
        self.assertEqual(default["total_matched"], 0)
        candidates = query(*args, "--include-candidates")
        self.assertGreater(candidates["total_matched"], 0)
        self.assertTrue(all(hit["governance_status"] == "CANDIDATE" for hit in candidates["hits"]))
        self.assertTrue(all(hit["lifecycle_status"] == "DISCOVERED" for hit in candidates["hits"]))
        self.assertTrue(all(hit["description"] is None for hit in candidates["hits"]))

    def test_nuclei_candidate_is_hidden_by_default_and_visible_on_opt_in(self):
        conn = sqlite3.connect(f"file:{INDEX}?mode=ro", uri=True)
        try:
            row = conn.execute(
                "SELECT name FROM search_index_doc WHERE resource_type='nuclei_template' "
                "AND governance='CANDIDATE' AND json_array_length(nuclei_cve_ids_json)>0 "
                "ORDER BY resource_id LIMIT 1"
            ).fetchone()
        finally:
            conn.close()
        self.assertIsNotNone(row)
        args = ("--keywords", row[0][:80], "--resource-type", "nuclei_template", "--limit", "20")
        default = query(*args)
        self.assertEqual(default["total_matched"], 0)
        candidates = query(*args, "--include-candidates")
        self.assertEqual(candidates["total_matched"], 1)
        self.assertEqual(candidates["hits"][0]["governance_status"], "CANDIDATE")


if __name__ == "__main__":
    unittest.main()
