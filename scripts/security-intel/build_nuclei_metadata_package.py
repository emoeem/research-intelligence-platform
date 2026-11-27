#!/usr/bin/env python3
"""Build a minimal, metadata-only RTIS SQLite projection for the Nuclei KSE parser.

The source RTIS database is opened read-only. No POC content, usage, verified_note,
engagement, target, credential, or other unrelated columns are copied.
"""
from __future__ import annotations

import argparse
import json
import sqlite3
import subprocess
import sys
from pathlib import Path

OFFICIAL_SOURCE = "https://github.com/projectdiscovery/nuclei-templates"
COLUMNS = ("id", "code", "title", "description", "usage", "severity", "cve", "tags", "path")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source-db", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--template-root", type=Path, default=Path("/usr/share/nuclei-templates"))
    parser.add_argument("--source-commit", default="a346d8f00c61bf95bbd30365f12cf923be51ba3d")
    parser.add_argument("--force", action="store_true")
    args = parser.parse_args()

    if len(args.source_commit) != 40 or any(
        character not in "0123456789abcdefABCDEF" for character in args.source_commit
    ):
        raise SystemExit("--source-commit must be a full 40-character hexadecimal commit")

    source = args.source_db.expanduser().resolve(strict=True)
    output = args.output.expanduser().resolve()
    template_root = args.template_root.expanduser().resolve(strict=True)
    source_revision_verification = "unverified_non_git_tree"
    if (template_root / ".git").exists():
        revision = subprocess.run(
            ["git", "-C", str(template_root), "rev-parse", "HEAD"],
            capture_output=True,
            text=True,
            check=False,
        )
        if revision.returncode != 0:
            raise SystemExit(
                f"cannot resolve template checkout revision: {revision.stderr.strip()}"
            )
        actual_commit = revision.stdout.strip()
        if actual_commit.lower() != args.source_commit.lower():
            raise SystemExit(
                f"template checkout revision {actual_commit} does not match "
                f"--source-commit {args.source_commit}"
            )
        status = subprocess.run(
            ["git", "-C", str(template_root), "status", "--porcelain", "--untracked-files=all"],
            capture_output=True,
            text=True,
            check=False,
        )
        if status.returncode != 0:
            raise SystemExit(f"cannot inspect template checkout status: {status.stderr.strip()}")
        if status.stdout.strip():
            raise SystemExit("template checkout has modified or untracked files")
        source_revision_verification = "verified_git_commit"
    if source == output or source == output.resolve():
        raise SystemExit("refusing to use the source DB as output")
    if output.exists() and not args.force:
        raise SystemExit(f"output already exists (pass --force to replace): {output}")
    output.parent.mkdir(parents=True, exist_ok=True)
    if output.exists():
        output.unlink()

    src = sqlite3.connect(f"file:{source}?mode=ro", uri=True)
    src.row_factory = sqlite3.Row
    try:
        columns = {row[1] for row in src.execute("PRAGMA table_info(poc)")}
        required = set(COLUMNS) | {"category", "source", "source_url"}
        missing = sorted(required - columns)
        if missing:
            raise SystemExit(f"source poc table missing columns: {missing}")
        query = "SELECT " + ",".join(COLUMNS) + " FROM poc WHERE category=? AND source=? AND source_url=? ORDER BY id"
        rows = src.execute(query, ("nuclei", "nuclei", OFFICIAL_SOURCE)).fetchall()
    finally:
        src.close()
    if not rows:
        raise SystemExit("no official Nuclei rows found in the RTIS source DB")

    out = sqlite3.connect(output)
    try:
        out.executescript("""
            PRAGMA journal_mode=DELETE;
            CREATE TABLE poc (
                id INTEGER PRIMARY KEY,
                code TEXT,
                title TEXT NOT NULL,
                description TEXT,
                usage TEXT,
                severity TEXT,
                cve TEXT,
                tags TEXT,
                path TEXT,
                category TEXT NOT NULL,
                source TEXT NOT NULL,
                source_url TEXT NOT NULL
            );
        """)
        insert = "INSERT INTO poc (" + ",".join(COLUMNS + ("category", "source", "source_url")) + ") VALUES (" + ",".join("?" for _ in range(len(COLUMNS) + 3)) + ")"
        out.executemany(insert, [tuple(row) + ("nuclei", "nuclei", OFFICIAL_SOURCE) for row in rows])
        out.commit()
        integrity = out.execute("PRAGMA integrity_check").fetchone()[0]
        if integrity != "ok":
            raise SystemExit(f"output SQLite integrity check failed: {integrity}")
        out.execute("VACUUM")
    finally:
        out.close()

    missing_templates = 0
    for row in rows:
        raw_path = row[8]
        if not raw_path:
            missing_templates += 1
            continue
        candidate = Path(raw_path)
        try:
            resolved = candidate.resolve(strict=True)
            resolved.relative_to(template_root)
        except (OSError, ValueError):
            missing_templates += 1

    report = {
        "source_db": str(source),
        "source_url": OFFICIAL_SOURCE,
        "source_commit": args.source_commit,
        "source_revision_verification": source_revision_verification,
        "template_root": str(template_root),
        "rows": len(rows),
        "missing_template_files": missing_templates,
        "output": str(output),
        "output_bytes": output.stat().st_size,
        "copied_columns": list(COLUMNS) + ["category", "source", "source_url"],
        "excluded_columns": ["content", "verified", "verified_note", "engagement_id", "engagement_name", "asset_target", "used_on", "found_by_agent"],
        "execution_performed": False,
    }
    readme = (
        "# Nuclei template metadata package\n\n"
        "Minimal SQLite projection generated read-only from RTIS Nuclei metadata rows. "
        "Only parser-required metadata columns are copied. Template YAML is read from the supplied "
        "local root and parsed statically. Git checkouts are checked against the pinned commit; "
        "non-Git package trees are explicitly unverified against that commit. "
        "No template is executed.\n\n"
        f"Upstream: {OFFICIAL_SOURCE}\n"
        f"Pinned commit: `{args.source_commit}`\n"
        "License: MIT\n"
    )
    output.with_name("README.md").write_text(readme, encoding="utf-8")
    report["readme"] = str(output.with_name("README.md"))
    print(json.dumps(report, ensure_ascii=False, indent=2))
    return 0


if __name__ == "__main__":
    sys.exit(main())
