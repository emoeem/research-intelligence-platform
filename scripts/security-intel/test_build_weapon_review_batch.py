import importlib.util
import unittest
from pathlib import Path

SCRIPT = Path(__file__).with_name("build_weapon_review_batch.py")
spec = importlib.util.spec_from_file_location("build_weapon_review_batch", SCRIPT)
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


class WeaponReviewBatchTests(unittest.TestCase):
    def test_sensitive_and_poc_assets_are_prioritized_without_body_export(self):
        report = {"candidates": [
            {"entity_id": "rtis:payload:1", "kind": "payload", "severity": "low", "secret_indicator_types": [], "static_behavior_flags": [], "content_sha256": "a"},
            {"entity_id": "rtis:poc:2", "kind": "poc", "severity": "high", "secret_indicator_types": [], "static_behavior_flags": ["network_client"], "content_sha256": "b"},
            {"entity_id": "rtis:script:3", "kind": "script", "severity": "medium", "secret_indicator_types": ["token"], "static_behavior_flags": ["network_client"], "content_sha256": "c"},
        ]}
        batch = module.build(report, 0, 10)
        self.assertEqual(batch["tasks"][0]["candidate_id"], "rtis:script:3")
        self.assertEqual(batch["tasks"][1]["candidate_id"], "rtis:poc:2")
        self.assertFalse(batch["tasks"][0]["guardrails"]["raw_code_or_payload_included"])
        self.assertFalse(batch["auto_approval"])
        self.assertNotIn("actual-secret-value", str(batch))

    def test_pagination_is_stable(self):
        rows = [{"entity_id": f"rtis:payload:{i}", "kind": "payload", "severity": "low", "secret_indicator_types": [], "static_behavior_flags": []} for i in range(5)]
        first = module.build({"candidates": rows}, 0, 2)
        second = module.build({"candidates": rows}, 2, 2)
        self.assertEqual(first["next_offset"], 2)
        self.assertEqual(second["offset"], 2)


if __name__ == "__main__":
    unittest.main()
