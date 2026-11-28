#!/usr/bin/env python3
"""Automated real-corpus ingestion through the existing rp-doc host parser."""
from __future__ import annotations

import argparse
import hashlib
import json
import os
import subprocess
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
RP_DOC = ROOT / "target/debug/rp-doc"
SUPPORTED = {".pdf", ".docx", ".pptx", ".xlsx", ".html", ".htm",
             ".md", ".markdown", ".txt", ".png", ".jpg", ".jpeg"}


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--corpus", required=True, type=Path)
    parser.add_argument("--execute", action="store_true")
    parser.add_argument("--report", type=Path)
    parser.add_argument("--workspace", default="default")
    args = parser.parse_args()

    if not args.corpus.is_dir():
        parser.error("--corpus must be a directory")
    files = sorted(
        p for p in args.corpus.rglob("*")
        if p.is_file() and p.suffix.lower() in SUPPORTED
    )
    plan = [{
        "path": str(p),
        "sha256": sha256(p),
        "bytes": p.stat().st_size,
        "format": p.suffix.lower().lstrip("."),
        "workspace": args.workspace,
    } for p in files]

    report = {
        "status": "planned",
        "execute": args.execute,
        "sample_count": len(plan),
        "minimum_samples": 40,
        "gate_passed": len(plan) >= 40,
        "samples": plan,
        "results": [],
    }

    if not args.execute:
        print(json.dumps(report, ensure_ascii=False, indent=2))
        return 0

    dsn = os.environ.get("RP_PG_DSN") or os.environ.get("RP_TEST_PG_DSN")
    if not dsn:
        report["status"] = "blocked"
        report["blocker"] = "RP_PG_DSN or RP_TEST_PG_DSN is required"
        print(json.dumps(report, ensure_ascii=False, indent=2))
        return 2
    if not RP_DOC.is_file():
        report["status"] = "blocked"
        report["blocker"] = "target/debug/rp-doc is missing; build intel-db rp-doc first"
        print(json.dumps(report, ensure_ascii=False, indent=2))
        return 2
    if len(plan) < 40:
        report["status"] = "blocked"
        report["blocker"] = f"real corpus has {len(plan)} supported files; 40 required"
        print(json.dumps(report, ensure_ascii=False, indent=2))
        return 2

    report["status"] = "running"
    for item in plan:
        path = item["path"]
        title = Path(path).name
        imp = subprocess.run(
            [str(RP_DOC), "import", "--file", path, "--title", title,
             "--workspace", args.workspace],
            cwd=ROOT,
            capture_output=True,
            text=True,
            timeout=60,
            env=os.environ.copy(),
        )
        if imp.returncode != 0:
            report["results"].append({"path": path, "stage": "import",
                                      "ok": False, "output": imp.stdout[-2000:]})
            continue
        try:
            imported = json.loads(imp.stdout)
        except json.JSONDecodeError:
            report["results"].append({"path": path, "stage": "import",
                                      "ok": False, "output": imp.stdout[-2000:]})
            continue
        document_id = imported.get("document_id")
        if not document_id:
            report["results"].append({"path": path, "stage": "import",
                                      "ok": False, "output": imp.stdout[-2000:]})
            continue
        parsed = subprocess.run(
            [str(RP_DOC), "parse", "--document", document_id],
            cwd=ROOT,
            capture_output=True,
            text=True,
            timeout=600,
            env=os.environ.copy(),
        )
        report["results"].append({
            "path": path,
            "sha256": item["sha256"],
            "document_id": document_id,
            "stage": "parse",
            "ok": parsed.returncode == 0,
            "output": parsed.stdout[-4000:],
        })

    successes = sum(1 for r in report["results"] if r["ok"])
    report["status"] = "pass" if successes == len(plan) else "partial"
    report["success_count"] = successes
    report["candidate_publication"] = False

    if args.report:
        args.report.parent.mkdir(parents=True, exist_ok=True)
        args.report.write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n",
                               encoding="utf-8")
    print(json.dumps(report, ensure_ascii=False, indent=2))
    return 0 if report["status"] == "pass" else 1


if __name__ == "__main__":
    raise SystemExit(main())
