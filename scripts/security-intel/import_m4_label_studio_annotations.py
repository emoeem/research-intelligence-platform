#!/usr/bin/env python3
"""Import Label Studio JSON annotations into the M4 CSV without trusting predictions."""
from __future__ import annotations

import argparse
import csv
import json
from collections import defaultdict
from datetime import datetime, timezone
from pathlib import Path

LABEL_TO_GRADE = {
    "0 — irrelevant": 0,
    "1 — weakly related": 1,
    "2 — relevant": 2,
    "3 — directly relevant": 3,
}


def annotation_grade(annotation: dict) -> tuple[int | None, str]:
    grades = []
    notes = []
    for item in annotation.get("result", []):
        if item.get("from_name") == "relevance" and item.get("type") == "choices":
            for value in item.get("value", {}).get("choices", []):
                if value in LABEL_TO_GRADE:
                    grades.append(LABEL_TO_GRADE[value])
                elif isinstance(value, str) and value[:1] in "0123":
                    grades.append(int(value[0]))
        elif item.get("from_name") == "human_notes" and item.get("type") == "textarea":
            notes.extend(item.get("value", {}).get("text", []))
    if len(set(grades)) > 1:
        raise ValueError(f"one annotation contains conflicting relevance labels: {grades}")
    return (grades[0] if grades else None, " | ".join(note.strip() for note in notes if note.strip()))


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--input",
        type=Path,
        required=True,
        help="Label Studio JSON export (all tasks, including unannotated tasks)",
    )
    parser.add_argument(
        "--batch",
        type=Path,
        default=Path("test-results/m4-cross-source-qrel-review-batch-001.csv"),
    )
    parser.add_argument("--output", type=Path, help="output CSV; defaults to in-place update")
    args = parser.parse_args()

    exported = json.loads(args.input.read_text(encoding="utf-8"))
    if not isinstance(exported, list):
        parser.error("expected Label Studio JSON export to be a list of tasks")

    with args.batch.open(encoding="utf-8-sig", newline="") as handle:
        reader = csv.DictReader(handle)
        original_fields = list(reader.fieldnames or [])
        rows = list(reader)
    by_id = {row["candidate_id"]: row for row in rows}
    judgments: dict[str, list[tuple[int, str, str, str]]] = defaultdict(list)
    unknown_ids = []

    for task in exported:
        data = task.get("data") or {}
        candidate_id = str(data.get("candidate_id") or task.get("meta", {}).get("candidate_id") or "")
        if candidate_id not in by_id:
            if candidate_id:
                unknown_ids.append(candidate_id)
            continue
        for annotation in task.get("annotations", []):
            if annotation.get("was_cancelled") or not annotation.get("result"):
                continue
            grade, notes = annotation_grade(annotation)
            if grade is None:
                continue
            reviewer = str((annotation.get("completed_by") or {}).get("email") or (annotation.get("completed_by") or {}).get("username") or "")
            reviewed_at = str(annotation.get("updated_at") or annotation.get("created_at") or "")
            judgments[candidate_id].append((grade, notes, reviewer, reviewed_at))

    if unknown_ids:
        parser.error(f"export contains unknown candidate IDs: {sorted(set(unknown_ids))[:10]}")

    conflicts = {
        candidate_id: sorted({grade for grade, _, _, _ in entries})
        for candidate_id, entries in judgments.items()
        if len({grade for grade, _, _, _ in entries}) > 1
    }
    if conflicts:
        parser.error(f"conflicting annotator judgments require manual resolution: {conflicts}")

    new_fields = original_fields[:]
    for field in ("human_reviewer", "human_reviewed_at"):
        if field not in new_fields:
            new_fields.append(field)

    for row in rows:
        candidate_id = row["candidate_id"]
        entries = judgments.get(candidate_id, [])
        if not entries:
            continue
        grades = {grade for grade, _, _, _ in entries}
        if len(grades) != 1:
            continue
        row["human_grade"] = str(next(iter(grades)))
        notes = [note for _, note, _, _ in entries if note]
        row["human_notes"] = " | ".join(dict.fromkeys(notes))
        reviewers = [reviewer for _, _, reviewer, _ in entries if reviewer]
        times = [reviewed_at for _, _, _, reviewed_at in entries if reviewed_at]
        row["human_reviewer"] = "; ".join(dict.fromkeys(reviewers))
        row["human_reviewed_at"] = "; ".join(dict.fromkeys(times))
        row["review_status"] = "human-reviewed"

    destination = args.output or args.batch
    destination.parent.mkdir(parents=True, exist_ok=True)
    with destination.open("w", encoding="utf-8-sig", newline="") as handle:
        writer = csv.DictWriter(handle, fieldnames=new_fields)
        writer.writeheader()
        writer.writerows(rows)

    judged = sum(bool(row["human_grade"].strip()) for row in rows)
    result = {
        "status": "human_review_partial" if judged < len(rows) else "human_review_complete",
        "candidate_pair_count": len(rows),
        "judged_pair_count": judged,
        "unjudged_pair_count": len(rows) - judged,
        "newly_judged_in_this_import": len(judgments),
        "label_conflicts": 0,
        "output": str(destination),
        "imported_at_utc": datetime.now(timezone.utc).isoformat(),
    }
    print(json.dumps(result, ensure_ascii=False, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
