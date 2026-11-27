#!/usr/bin/env python3
"""Read-only, evidence-backed Skill -> registered Tool mapping candidates.

This deliberately does not write platform.db or PostgreSQL. Exact mentions are
review candidates, not proof that a Skill requires or is authorized to execute
that tool. All paths are constrained to the repository tree.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import re
import sqlite3
import sys
from pathlib import Path
from typing import Any

NEGATIVE = re.compile(r"(?:不要|不使用|不能|禁止|避免|无需|无须|不应|切勿|do not|don't|avoid|must not|cannot|not use)", re.I)
TOOL_SECTION = re.compile(r"(?:工具|命令|执行|扫描|本地已备|tool|command|execution)", re.I)


def connect_readonly(path: Path) -> sqlite3.Connection:
    uri = f"file:{path.resolve().as_posix()}?mode=ro"
    conn = sqlite3.connect(uri, uri=True)
    conn.row_factory = sqlite3.Row
    conn.execute("PRAGMA query_only=ON")
    return conn


def safe_body_path(raw: str | None, repo_root: Path) -> Path | None:
    if not raw:
        return None
    candidate = Path(raw)
    if not candidate.is_absolute():
        candidate = repo_root / candidate
    try:
        resolved = candidate.resolve(strict=True)
        resolved.relative_to(repo_root.resolve())
    except (OSError, ValueError):
        return None
    return resolved if resolved.is_file() else None


def snippets(body: str, tool_id: str) -> list[dict[str, Any]]:
    token = re.compile(r"(?<![\w.-])" + re.escape(tool_id) + r"(?![\w.-])", re.I)
    lines = body.splitlines()
    current_section = ""
    matches: list[dict[str, Any]] = []
    for number, line in enumerate(lines, 1):
        heading = re.match(r"^\s*#{1,6}\s+(.+?)\s*$", line)
        if heading:
            current_section = heading.group(1)
        if not token.search(line):
            continue
        negative = bool(NEGATIVE.search(line))
        command_pattern = re.compile(
            r"(?:`\s*|^\s*(?:[-*+]\s*)?)(?:sudo\s+)?(?:\./)?(?:~?/[^\s`]+/)?"
            + re.escape(tool_id)
            + r"(?=\s|$|[`|,，;；])",
            re.I,
        )
        command = bool(command_pattern.search(line))
        section_match = bool(TOOL_SECTION.search(current_section))
        polarity = "negative" if negative else "positive"
        match_kind = "command_invocation" if command else "tool_section_mention" if section_match else "body_mention"
        score = 0.72 if command else 0.60 if section_match else 0.45
        if negative:
            score = 0.0
        matches.append({
            "line": number,
            "section": current_section or None,
            "excerpt": line.strip()[:280],
            "polarity": polarity,
            "match_kind": match_kind,
            "heuristic_score": score,
        })
        if len(matches) >= 6:
            break
    return matches


def build_report(db_path: Path, repo_root: Path) -> dict[str, Any]:
    conn = connect_readonly(db_path)
    try:
        skills = conn.execute(
            "SELECT skill_id,name,version,description,when_to_use,required_tools,knowledge_refs,body_ref,enabled,governance,source_system,source_pk "
            "FROM skill ORDER BY skill_id"
        ).fetchall()
        tools = conn.execute(
            "SELECT tool_id,name,kind,lifecycle,governance,install_state,health_state,permission_level,purpose,provenance_ref,source_system,source_pk,schema_version,updated_at "
            "FROM tool ORDER BY tool_id"
        ).fetchall()
        candidates: list[dict[str, Any]] = []
        negative_mentions: list[dict[str, Any]] = []
        missing_bodies: list[str] = []
        skill_body_count = 0
        for skill in skills:
            path = safe_body_path(skill["body_ref"], repo_root)
            body = ""
            body_hash = None
            relative_path = None
            if path:
                body_bytes = path.read_bytes()
                body = body_bytes.decode("utf-8", errors="replace")
                body_hash = hashlib.sha256(body_bytes).hexdigest()
                relative_path = path.relative_to(repo_root.resolve()).as_posix()
                skill_body_count += 1
            else:
                missing_bodies.append(skill["skill_id"])
            metadata = "\n".join((skill["description"] or "", skill["when_to_use"] or ""))
            for tool in tools:
                matches = snippets(body, tool["tool_id"]) if body else []
                if not matches:
                    # Metadata-only matches are weaker and never treated as command use.
                    token = re.compile(r"(?<![\w.-])" + re.escape(tool["tool_id"]) + r"(?![\w.-])", re.I)
                    if token.search(metadata):
                        matches = [{
                            "line": None, "section": "skill metadata",
                            "excerpt": token.sub(lambda m: m.group(0), metadata)[:280],
                            "polarity": "positive", "match_kind": "metadata_mention",
                            "heuristic_score": 0.30,
                        }]
                if not matches:
                    continue
                positive = [m for m in matches if m["polarity"] == "positive"]
                negative = [m for m in matches if m["polarity"] == "negative"]
                tool_snapshot = {
                    "tool_id": tool["tool_id"], "name": tool["name"], "kind": tool["kind"],
                    "lifecycle": tool["lifecycle"], "governance": tool["governance"],
                    "install_state": tool["install_state"], "health_state": tool["health_state"],
                    "permission_level": tool["permission_level"], "purpose": tool["purpose"],
                    "provenance_ref": tool["provenance_ref"],
                }
                tool_snapshot_bytes = json.dumps(tool_snapshot, ensure_ascii=False, sort_keys=True, separators=(",", ":")).encode("utf-8")
                shared = {
                    "skill_id": skill["skill_id"],
                    "skill_version": skill["version"],
                    "skill_enabled": bool(skill["enabled"]) if skill["enabled"] is not None else None,
                    "skill_governance": skill["governance"],
                    "tool_id": tool["tool_id"],
                    "tool_kind": tool["kind"],
                    "tool_governance": tool["governance"],
                    "tool_install_state": tool["install_state"],
                    "tool_health_state": tool["health_state"],
                    "tool_permission_level": tool["permission_level"],
                    "tool_provenance_ref": tool["provenance_ref"],
                    "tool_source": {
                        "source_system": tool["source_system"] or "rip.platform.tool-registry",
                        "source_pk": tool["source_pk"] or tool["tool_id"],
                        "source_version": f"{tool['schema_version']}:{tool['updated_at'] or ''}",
                        "canonical_uri": f"riptool://{tool['tool_id']}",
                        "content_sha256": hashlib.sha256(tool_snapshot_bytes).hexdigest(),
                    },
                    "source": {
                        "body_ref": relative_path,
                        "body_sha256": body_hash,
                        "source_system": skill["source_system"],
                        "source_pk": skill["source_pk"],
                    },
                }
                if positive:
                    best = max(positive, key=lambda m: (m["heuristic_score"], -int(m["line"] or 0)))
                    eligible = (
                        body_hash is not None
                        and best["match_kind"] != "metadata_mention"
                        and skill["enabled"] == 1
                        and skill["governance"] == "ACTIVE"
                        and tool["governance"] == "ACTIVE"
                        and tool["install_state"] == "installed"
                        and tool["health_state"] == "ok"
                    )
                    candidates.append({
                        **shared,
                        "relation_type": "USES_TOOL",
                        "review_status": "CANDIDATE",
                        "activation_eligible_after_review": eligible,
                        "heuristic_score": best["heuristic_score"],
                        "match_kind": best["match_kind"],
                        "occurrence_count": len(positive),
                        "evidence": positive[:3],
                        "decision_note": "Exact lexical mention only; human review must decide whether this tool is required, optional, or merely illustrative.",
                    })
                if negative:
                    negative_mentions.append({**shared, "excluded_from_candidates": True, "evidence": negative[:3]})
        candidates.sort(key=lambda x: (x["skill_id"], x["tool_id"]))
        negative_mentions.sort(key=lambda x: (x["skill_id"], x["tool_id"]))
        by_tool: dict[str, int] = {}
        for item in candidates:
            by_tool[item["tool_id"]] = by_tool.get(item["tool_id"], 0) + 1
        return {
            "schema_version": "1.0.0",
            "mode": "read_only_candidate_generation",
            "source_db": str(db_path),
            "source_db_sha256": hashlib.sha256(db_path.read_bytes()).hexdigest(),
            "summary": {
                "skills_total": len(skills),
                "skills_with_readable_body": skill_body_count,
                "skills_missing_or_outside_repo_body": len(missing_bodies),
                "registered_tools": len(tools),
                "candidate_skill_tool_pairs": len(candidates),
                "skills_with_candidates": len({x["skill_id"] for x in candidates}),
                "negative_mentions_excluded": len(negative_mentions),
                "activation_eligible_after_review": sum(bool(x["activation_eligible_after_review"]) for x in candidates),
                "candidate_pairs_by_tool": dict(sorted(by_tool.items())),
            },
            "governance": {
                "auto_approved": False,
                "source_db_mutated": False,
                "relation_type": "USES_TOOL",
                "required_review": "Confirm required-vs-optional semantics, tool health, scope and permission before creating/approving graph relations.",
                "heuristic_score_is_probability": False,
            },
            "missing_bodies": missing_bodies,
            "candidates": candidates,
            "negative_mentions": negative_mentions,
        }
    finally:
        conn.close()


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--db", type=Path, default=Path("migrations/platform.db"))
    parser.add_argument("--repo-root", type=Path, default=Path.cwd())
    parser.add_argument("--output", type=Path, help="Write the report file; default stdout")
    args = parser.parse_args()
    report = build_report(args.db, args.repo_root)
    rendered = json.dumps(report, ensure_ascii=False, indent=2) + "\n"
    if args.output:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(rendered, encoding="utf-8")
    else:
        sys.stdout.write(rendered)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
