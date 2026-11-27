#!/usr/bin/env python3
"""Export M4 Label Studio tasks/annotations locally without the legacy-token export endpoint."""
from __future__ import annotations

import argparse
import json
import os
import sys
from pathlib import Path


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--project-id", type=int, default=1)
    parser.add_argument(
        "--output",
        type=Path,
        default=Path("test-results/m4-label-studio-batch-001/export.json"),
    )
    parser.add_argument("--expected-tasks", type=int, default=140)
    args = parser.parse_args()

    home = Path.home()
    venv_site = home / ".cache/rip-m4-label-studio-venv/lib/python3.13/site-packages"
    package_root = venv_site / "label_studio"
    if not package_root.is_dir():
        parser.error(f"Label Studio package not found: {package_root}")

    os.environ.setdefault(
        "LABEL_STUDIO_BASE_DATA_DIR",
        str(home / ".local/share/rip-m4-label-studio/data"),
    )
    os.environ.setdefault("LABEL_STUDIO_HOST", "http://127.0.0.1:8062")
    os.environ.setdefault("HOST", "http://127.0.0.1:8062")
    os.environ.setdefault("LABEL_STUDIO_ENABLE_LEGACY_API_TOKEN", "false")
    os.environ.setdefault("DJANGO_SETTINGS_MODULE", "label_studio.core.settings.label_studio")
    sys.path.insert(0, str(venv_site))
    sys.path.insert(0, str(package_root))

    import django  # type: ignore[import-not-found]

    django.setup()
    from data_export.serializers import ExportDataSerializer  # type: ignore[import-not-found]
    from tasks.models import Task  # type: ignore[import-not-found]

    queryset = (
        Task.objects.filter(project_id=args.project_id)
        .select_related("project")
        .prefetch_related("annotations", "predictions")
        .order_by("id")
    )
    task_count = queryset.count()
    if args.expected_tasks and task_count != args.expected_tasks:
        parser.error(f"expected {args.expected_tasks} tasks, found {task_count}")

    payload = list(
        ExportDataSerializer(
            queryset,
            many=True,
            expand=["drafts"],
            context={"interpolate_key_frames": False},
        ).data
    )
    candidate_ids = [str((task.get("data") or {}).get("candidate_id") or "") for task in payload]
    if any(not candidate_id for candidate_id in candidate_ids):
        parser.error("one or more tasks are missing data.candidate_id")
    if len(set(candidate_ids)) != len(candidate_ids):
        parser.error("duplicate candidate_id values found in Label Studio tasks")

    output = args.output.resolve()
    output.parent.mkdir(parents=True, exist_ok=True)
    temporary = output.with_suffix(output.suffix + ".tmp")
    temporary.write_text(json.dumps(payload, ensure_ascii=False, indent=2), encoding="utf-8")
    temporary.replace(output)

    annotation_count = sum(len(task.get("annotations", [])) for task in payload)
    prediction_count = sum(len(task.get("predictions", [])) for task in payload)
    completed_count = sum(
        any(annotation.get("result") and not annotation.get("was_cancelled") for annotation in task.get("annotations", []))
        for task in payload
    )
    print(
        json.dumps(
            {
                "status": "exported",
                "project_id": args.project_id,
                "task_count": len(payload),
                "candidate_id_count": len(set(candidate_ids)),
                "annotation_count": annotation_count,
                "tasks_with_completed_annotations": completed_count,
                "prediction_count": prediction_count,
                "legacy_api_token_used": False,
                "output": str(output),
            },
            ensure_ascii=False,
            indent=2,
        )
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
