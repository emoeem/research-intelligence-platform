#!/usr/bin/env python3
"""Repository secret, runtime file, and large artifact guard."""
from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]

SECRET_PATTERNS = (
    ("private_key", re.compile(rb"-----BEGIN (?:RSA |EC |OPENSSH )?PRIVATE KEY-----")),
    ("github_token", re.compile(rb"\bgh[pousr]_[A-Za-z0-9]{30,}\b")),
    ("aws_access_key", re.compile(rb"\b(?:AKIA|ASIA)[A-Z0-9]{16}\b")),
    ("generic_bearer", re.compile(rb"(?i)\bbearer\s+[A-Za-z0-9._~+/=-]{24,}")),
)

RUNTIME_NAMES = (
    ".sock.lock",
    ".pid",
    ".log",
)

DB_EXTENSIONS = (
    ".db",
    ".sqlite",
    ".sqlite3",
)


def tracked_files() -> list[Path]:
    result = subprocess.run(
        ["git", "-C", str(ROOT), "ls-files", "-z"],
        check=True,
        stdout=subprocess.PIPE,
    )
    return [ROOT / name.decode("utf-8", "surrogateescape") for name in result.stdout.split(b"\0") if name]


def main() -> int:
    failures: list[str] = []
    for path in tracked_files():
        rel = path.relative_to(ROOT).as_posix()
        if rel.startswith(("vendor/", "target/", "node_modules/")):
            continue
        if rel.startswith("migrations/sources-snapshot/") and path.stat().st_size > 25 * 1024 * 1024:
            continue
        if any(rel.endswith(suffix) or Path(rel).name.endswith(suffix) for suffix in RUNTIME_NAMES):
            failures.append(f"runtime file tracked: {rel}")
        if Path(rel).suffix in DB_EXTENSIONS:
            if not (
                rel.startswith(("migrations/sources-snapshot/", "migrations/dev-", "migrations/platform-"))
                or rel.startswith("tests/")
            ):
                failures.append(f"database file tracked outside classified snapshot: {rel}")
        try:
            size = path.stat().st_size
        except FileNotFoundError:
            continue
        if size > 25 * 1024 * 1024:
            failures.append(f"large tracked file: {rel} ({size} bytes)")
        if size <= 5 * 1024 * 1024:
            data = path.read_bytes()
            is_fixture = "/tests/" in f"/{rel}" or rel.startswith("tests/")
            if not is_fixture:
                for label, pattern in SECRET_PATTERNS:
                    if pattern.search(data):
                        failures.append(f"secret pattern {label}: {rel}")
    if failures:
        print("QUALITY_REPO_SCAN=FAIL")
        for failure in failures:
            print(f"- {failure}")
        return 1
    print("QUALITY_REPO_SCAN=PASS")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
