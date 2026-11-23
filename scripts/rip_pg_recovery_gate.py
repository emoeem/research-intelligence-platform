#!/usr/bin/env python3
"""PostgreSQL migration/backup/recovery smoke gate."""
from __future__ import annotations

import argparse
import os
import subprocess
import tempfile
import uuid
from pathlib import Path

import psycopg


def dsn_args(dsn: str) -> list[str]:
    return ["-d", dsn]


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--dsn", default=os.environ.get("RP_TEST_PG_DSN", ""))
    args = parser.parse_args()
    if not args.dsn:
        parser.error("RP_TEST_PG_DSN is required")

    table = f"quality_recovery_{uuid.uuid4().hex[:12]}"
    with psycopg.connect(args.dsn, autocommit=True) as conn:
        conn.execute(f'CREATE TABLE "{table}" (id bigint PRIMARY KEY, payload text NOT NULL)')
        conn.execute(f'INSERT INTO "{table}" VALUES (1, %s), (2, %s)', ("before", "recovery"))

    with tempfile.TemporaryDirectory(prefix="rip-pg-recovery-") as tmp:
        dump = Path(tmp) / "quality.dump"
        subprocess.run(
            ["pg_dump", "-Fc", *dsn_args(args.dsn)],
            check=True,
            stdout=dump.open("wb"),
        )
        with psycopg.connect(args.dsn, autocommit=True) as conn:
            conn.execute(f'DROP TABLE "{table}"')
        subprocess.run(
            [
                "pg_restore",
                "--clean",
                "--if-exists",
                "--exit-on-error",
                "--no-owner",
                "--no-privileges",
                "-d",
                args.dsn,
                str(dump),
            ],
            check=True,
            stdout=subprocess.DEVNULL,
        )
        with psycopg.connect(args.dsn) as conn:
            rows = conn.execute(f'SELECT id,payload FROM "{table}" ORDER BY id').fetchall()
        if rows != [(1, "before"), (2, "recovery")]:
            raise SystemExit(f"PostgreSQL recovery mismatch: {rows!r}")
        with psycopg.connect(args.dsn, autocommit=True) as conn:
            conn.execute(f'DROP TABLE "{table}"')
    print("PG_RECOVERY_GATE=PASS")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
