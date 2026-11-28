#!/usr/bin/env python3
"""Independent PostgreSQL table dump/restore drill against template0."""
from __future__ import annotations

import argparse
import os
import subprocess
import tempfile
import uuid
from pathlib import Path

import psycopg
from psycopg import sql
from psycopg.conninfo import conninfo_to_dict, make_conninfo


def restore_dsn(dsn: str, dbname: str) -> str:
    params = conninfo_to_dict(dsn)
    params["dbname"] = dbname
    return make_conninfo(**params)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--dsn", default=os.environ.get("RP_TEST_PG_DSN", ""))
    args = parser.parse_args()
    if not args.dsn:
        parser.error("RP_TEST_PG_DSN is required")

    table = f"quality_recovery_{uuid.uuid4().hex[:12]}"
    restore_db = f"quality_restore_{uuid.uuid4().hex[:12]}"
    try:
        with psycopg.connect(args.dsn, autocommit=True) as conn:
            conn.execute(
                f'CREATE TABLE "{table}" '
                '(id bigint PRIMARY KEY, payload text NOT NULL)'
            )
            conn.execute(
                f'INSERT INTO "{table}" VALUES (1, %s), (2, %s)',
                ("before", "recovery"),
            )
            conn.execute(
                sql.SQL("CREATE DATABASE {} TEMPLATE template0").format(
                    sql.Identifier(restore_db)
                )
            )
        with tempfile.TemporaryDirectory(prefix="rip-pg-independent-") as tmp:
            dump = Path(tmp) / "quality.dump"
            subprocess.run(
                ["pg_dump", "-Fc", "-t", table, "-d", args.dsn],
                check=True,
                stdout=dump.open("wb"),
            )
            target_dsn = restore_dsn(args.dsn, restore_db)
            subprocess.run(
                [
                    "pg_restore",
                    "--exit-on-error",
                    "--no-owner",
                    "--no-privileges",
                    "-d",
                    target_dsn,
                    str(dump),
                ],
                check=True,
                stdout=subprocess.DEVNULL,
            )
            with psycopg.connect(target_dsn) as conn:
                rows = conn.execute(
                    f'SELECT id,payload FROM "{table}" ORDER BY id'
                ).fetchall()
            if rows != [(1, "before"), (2, "recovery")]:
                raise SystemExit(f"PostgreSQL recovery mismatch: {rows!r}")
    finally:
        with psycopg.connect(args.dsn, autocommit=True) as conn:
            conn.execute(
                sql.SQL("DROP TABLE IF EXISTS {}").format(sql.Identifier(table))
            )
            conn.execute(
                sql.SQL("DROP DATABASE IF EXISTS {} WITH (FORCE)").format(
                    sql.Identifier(restore_db)
                )
            )
    print("PG_INDEPENDENT_RESTORE=PASS")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
