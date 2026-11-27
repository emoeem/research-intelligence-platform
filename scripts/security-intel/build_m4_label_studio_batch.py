#!/usr/bin/env python3
"""Convert the reviewed M4 CSV pool into Label Studio tasks with advisory predictions."""
from __future__ import annotations

import argparse
import csv
import json
from pathlib import Path

LABELS = {
    0: "0 — irrelevant",
    1: "1 — weakly related",
    2: "2 — relevant",
    3: "3 — directly relevant",
}
CONFIG = """<View>
  <Header value="Query: $query" />
  <Header value="Candidate $candidate_id · Rank $rank" />
  <View style="border: 1px solid #ddd; padding: 12px; border-radius: 8px;">
    <Header value="$title" />
    <Text name="description" value="$description" />
    <Text name="source_system" value="Source: $source_system" />
    <Text name="resource_type" value="Type: $resource_type" />
    <Text name="canonical_url" value="$canonical_url" />
    <Text name="source_path" value="Path: $source_path" />
    <Text name="source_commit" value="Commit: $source_commit" />
  </View>
  <Header value="Human relevance judgment" />
  <Choices name="relevance" toName="description" choice="single" required="true" showInLine="true">
    <Choice value="0 — irrelevant" />
    <Choice value="1 — weakly related" />
    <Choice value="2 — relevant" />
    <Choice value="3 — directly relevant" />
  </Choices>
  <Header value="Optional rationale" />
  <TextArea name="human_notes" toName="description" rows="3" maxSubmissions="1" />
</View>
"""


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--input",
        type=Path,
        default=Path("test-results/m4-cross-source-qrel-review-batch-001.csv"),
    )
    parser.add_argument(
        "--output-dir",
        type=Path,
        default=Path("test-results/m4-label-studio-batch-001"),
    )
    args = parser.parse_args()

    with args.input.open(encoding="utf-8-sig", newline="") as handle:
        rows = list(csv.DictReader(handle))
    if len(rows) != 140:
        parser.error(f"expected the fixed 140-pair batch, got {len(rows)}")
    if len({row["candidate_id"] for row in rows}) != len(rows):
        parser.error("candidate_id values must be unique")
    if any(row["human_grade"].strip() for row in rows):
        parser.error("refusing to export a batch that already contains human grades")

    tasks = []
    for row in rows:
        grade = int(row["ai_suggested_grade"])
        confidence = float(row["ai_suggestion_confidence"])
        data = {
            "batch_id": row["batch_id"],
            "candidate_id": row["candidate_id"],
            "query_id": row["query_id"],
            "query": row["query"],
            "rank": int(row["rank"]),
            "resource_id": row["resource_id"],
            "resource_type": row["resource_type"],
            "title": row["title"],
            "source_system": row["source_system"],
            "source_entity_id": row["source_entity_id"],
            "source_version": row["source_version"],
            "canonical_url": row["canonical_url"],
            "description": row["description"],
            "source_path": row["source_path"],
            "source_commit": row["source_commit"],
            "ai_suggested_grade": grade,
            "ai_suggestion_reason": row["ai_suggestion_reason"],
            "ai_suggestion_confidence": confidence,
            "ai_suggestion_status": "advisory_unconfirmed",
        }
        tasks.append(
            {
                "data": data,
                "predictions": [
                    {
                        "model_version": "RIP M4 AI triage — advisory only",
                        "score": confidence,
                        "result": [
                            {
                                "from_name": "relevance",
                                "to_name": "description",
                                "type": "choices",
                                "value": {"choices": [LABELS[grade]]},
                            }
                        ],
                    }
                ],
            }
        )

    args.output_dir.mkdir(parents=True, exist_ok=True)
    tasks_path = args.output_dir / "tasks.json"
    config_path = args.output_dir / "labeling-config.xml"
    manifest_path = args.output_dir / "manifest.json"
    tasks_path.write_text(json.dumps(tasks, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    config_path.write_text(CONFIG, encoding="utf-8")
    manifest = {
        "batch_id": rows[0]["batch_id"],
        "status": "ready_for_human_annotation",
        "task_count": len(tasks),
        "query_count": len({row["query_id"] for row in rows}),
        "source_counts": dict(
            sorted(
                (source, sum(row["source_system"] == source for row in rows))
                for source in {row["source_system"] for row in rows}
            )
        ),
        "human_labels_included": False,
        "ai_predictions_are_advisory": True,
        "tasks_file": tasks_path.name,
        "labeling_config_file": config_path.name,
    }
    manifest_path.write_text(json.dumps(manifest, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(manifest, ensure_ascii=False, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
