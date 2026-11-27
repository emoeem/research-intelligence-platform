import sqlite3
import tempfile
import unittest
from pathlib import Path

import audit_ai_knowledge_arsenal as audit


class ReadOnlyAuditTests(unittest.TestCase):
    def test_open_readonly_prevents_mutation(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "sample.db"
            conn = sqlite3.connect(path)
            conn.execute("CREATE TABLE search_index_doc (resource_type TEXT NOT NULL)")
            conn.execute("INSERT INTO search_index_doc VALUES ('nuclei_template')")
            conn.commit()
            conn.close()

            with audit.open_readonly(path) as readonly:
                self.assertEqual(readonly.execute("SELECT COUNT(*) FROM search_index_doc").fetchone()[0], 1)
                with self.assertRaises(sqlite3.OperationalError):
                    readonly.execute("INSERT INTO search_index_doc VALUES ('poc')")

    def test_search_projection_report_counts_docs_and_types(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "search.db"
            conn = sqlite3.connect(path)
            conn.executescript(
                "CREATE TABLE search_index_doc (resource_type TEXT NOT NULL);"
                "CREATE VIRTUAL TABLE search_index_fts USING fts5(resource_type);"
                "INSERT INTO search_index_doc VALUES ('nuclei_template'),('rtis_poc');"
                "INSERT INTO search_index_fts VALUES ('nuclei_template'),('rtis_poc');"
            )
            conn.commit()
            conn.close()

            report = audit.audit_search(path)
            self.assertEqual(report["integrity"], "ok")
            self.assertEqual(report["documents"], 2)
            self.assertEqual(report["fts_rows"], 2)
            self.assertEqual(sum(row["n"] for row in report["by_resource_type"]), 2)


if __name__ == "__main__":
    unittest.main()
