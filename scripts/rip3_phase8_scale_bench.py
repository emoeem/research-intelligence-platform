#!/usr/bin/env python3
"""RIP3 Phase 8 reproducible PostgreSQL scale benchmark.
Creates an isolated disposable schema; never writes production tables.
Default: 1k/10k/100k documents and 1M chunks (10 per document).
Requires psycopg3 and pg_dump/pg_restore/createdb/dropdb on PATH.
"""
from __future__ import annotations
import argparse, concurrent.futures, json, os, platform, shutil, statistics, subprocess, sys, time, uuid
from datetime import datetime, timezone
from pathlib import Path
import psycopg
from psycopg.conninfo import conninfo_to_dict, make_conninfo

def percentile(values: list[float], p: float) -> float:
    if not values: return 0.0
    ordered = sorted(values)
    return round(ordered[min(len(ordered)-1, int((len(ordered)-1)*p))], 3)

def timed(fn):
    start = time.perf_counter()
    value = fn()
    return value, round(time.perf_counter()-start, 3)

def copy_docs(conn, schema, start, end):
    with conn.cursor() as cur:
        with cur.copy(f'COPY "{schema}".docs (doc_id,title,body,checksum) FROM STDIN') as cp:
            for doc_id in range(start, end):
                body = f"research document {doc_id} parameter validation alpha beta needle group-{doc_id % 100}"
                cp.write_row((doc_id, f"Synthetic research document {doc_id}", body, f"{doc_id:064x}"))

def copy_chunks(conn, schema, start_doc, end_doc, per_doc):
    with conn.cursor() as cur:
        with cur.copy(f'COPY "{schema}".chunks (chunk_id,doc_id,ordinal,body) FROM STDIN') as cp:
            for doc_id in range(start_doc, end_doc):
                for ordinal in range(per_doc):
                    chunk_id = doc_id * per_doc + ordinal
                    body = (f"document-{doc_id} chunk-{ordinal} research security parameter validation "
                            f"needle payload alpha beta group-{doc_id % 100} finding-{doc_id % 997} markerx{doc_id}")
                    cp.write_row((chunk_id, doc_id, ordinal, body))

def retrieval_once(dsn, schema, term):
    started = time.perf_counter()
    with psycopg.connect(dsn) as conn:
        conn.execute(f"""SELECT doc_id FROM "{schema}".chunks
            WHERE search_vector @@ plainto_tsquery('simple', %s) LIMIT 20""", (term,)).fetchall()
    return (time.perf_counter()-started)*1000

def write_ops_events(dsn, schema, worker):
    with psycopg.connect(dsn) as conn:
        for batch in range(10):
            rows = [
                (f"caller-{worker % 4}", "tool_run",
                 json.dumps({"worker": worker, "batch": batch, "i": i}),
                 datetime.now(timezone.utc))
                for i in range(50)
            ]
            with conn.cursor() as cur:
                cur.executemany(
                    f'INSERT INTO "{schema}".ops_events(caller_id,event_type,payload,created_at) VALUES(%s,%s,%s,%s)',
                    rows,
                )
            conn.commit()

def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--dsn", default=os.environ.get("RP_BENCH_DSN", os.environ.get("RP_PG_DSN", "")))
    ap.add_argument("--docs", type=int, default=100_000)
    ap.add_argument("--chunks-per-doc", type=int, default=10)
    ap.add_argument("--workers", type=int, default=16)
    ap.add_argument("--queries", type=int, default=100)
    ap.add_argument("--output-dir", default="docs/rip3/benchmarks")
    ap.add_argument("--skip-backup-restore", action="store_true")
    a = ap.parse_args()
    if not a.dsn: ap.error("set RP_BENCH_DSN or RP_PG_DSN, or pass --dsn")
    if a.docs < 1000 or a.chunks_per_doc < 1 or a.workers < 1 or a.queries < 1: ap.error("invalid scale parameters")
    required = ["pg_dump","pg_restore","createdb","dropdb"]
    missing = [x for x in required if shutil.which(x) is None]
    if missing and not a.skip_backup_restore: ap.error("missing tools: "+", ".join(missing))
    stamp = datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%SZ")
    schema = f"rip3_bench_{uuid.uuid4().hex[:12]}"
    out = Path(a.output_dir); out.mkdir(parents=True, exist_ok=True)
    json_path = out / f"phase8-scale-{stamp}.json"
    md_path = out / f"phase8-scale-{stamp}.md"
    report = {
        "schema_version":"rip3-phase8-benchmark-v1", "started_at":datetime.now(timezone.utc).isoformat(),
        "environment":{"python":sys.version.split()[0],"platform":platform.platform(),"cpu_count":os.cpu_count(),
                       "docs_target":a.docs,"chunks_per_doc":a.chunks_per_doc,"workers":a.workers,"queries":a.queries},
        "tiers":[],"operations":{},"limitations":[]
    }
    dbname = conninfo_to_dict(a.dsn).get("dbname","postgres")
    restore_db = f"rip3_restore_{uuid.uuid4().hex[:10]}"
    dump_path = Path(f"/tmp/{schema}.dump")
    backup_created = False
    try:
        with psycopg.connect(a.dsn, autocommit=True) as conn:
            report["environment"]["postgres_version"] = conn.execute("SHOW server_version").fetchone()[0]
            report["environment"]["shared_buffers"] = conn.execute("SHOW shared_buffers").fetchone()[0]
            conn.execute(f'CREATE SCHEMA "{schema}"')
            conn.execute(f'''CREATE TABLE "{schema}".docs (
                doc_id BIGINT PRIMARY KEY,title TEXT NOT NULL,body TEXT NOT NULL,checksum CHAR(64) NOT NULL)''')
            conn.execute(f'''CREATE TABLE "{schema}".chunks (
                chunk_id BIGINT PRIMARY KEY,doc_id BIGINT NOT NULL,ordinal SMALLINT NOT NULL,body TEXT NOT NULL,
                search_vector TSVECTOR GENERATED ALWAYS AS (to_tsvector('simple',body)) STORED)''')
            conn.execute(f'''CREATE TABLE "{schema}".ops_events (
                event_id BIGSERIAL PRIMARY KEY,caller_id TEXT NOT NULL,event_type TEXT NOT NULL,
                payload JSONB NOT NULL,created_at TIMESTAMPTZ NOT NULL)''')
            conn.execute(f'CREATE INDEX ops_events_caller_created ON "{schema}".ops_events(caller_id,created_at)')
            tiers = sorted(set(x for x in (1000,10000,a.docs) if x <= a.docs))
            previous = 0
            for tier in tiers:
                (_, doc_s) = timed(lambda: copy_docs(conn,schema,previous,tier))
                (_, chunk_s) = timed(lambda: copy_chunks(conn,schema,previous,tier,a.chunks_per_doc))
                doc_n = tier-previous; chunk_n = doc_n*a.chunks_per_doc
                report["tiers"].append({"documents_total":tier,"chunks_total":tier*a.chunks_per_doc,
                    "documents_added":doc_n,"chunks_added":chunk_n,"document_import_seconds":doc_s,
                    "chunk_import_seconds":chunk_s,"documents_per_second":round(doc_n/max(doc_s,.0001),1),
                    "chunks_per_second":round(chunk_n/max(chunk_s,.0001),1)})
                previous = tier
            (_, index_s) = timed(lambda: (
                conn.execute(f'CREATE INDEX chunks_search_gin ON "{schema}".chunks USING GIN(search_vector)'),
                conn.execute(f'CREATE INDEX chunks_doc_idx ON "{schema}".chunks(doc_id)')))
            report["operations"]["index_build_seconds"] = index_s
            conn.execute(f'ANALYZE "{schema}".docs'); conn.execute(f'ANALYZE "{schema}".chunks')
            search_term = f"markerx{a.docs // 2}"
            for _ in range(10):
                conn.execute(f"""SELECT doc_id FROM "{schema}".chunks WHERE search_vector @@
                    plainto_tsquery('simple',%s) LIMIT 20""", (search_term,)).fetchall()
            lat = [retrieval_once(a.dsn,schema,search_term) for _ in range(a.queries)]
            report["operations"]["retrieval"] = {"samples":len(lat),"p50_ms":percentile(lat,.5),
                "p95_ms":percentile(lat,.95),"max_ms":round(max(lat),3),"mean_ms":round(statistics.mean(lat),3)}
            started = time.perf_counter()
            with concurrent.futures.ThreadPoolExecutor(max_workers=a.workers) as pool:
                clat = list(pool.map(lambda _:retrieval_once(a.dsn,schema,search_term),range(a.queries)))
            elapsed = time.perf_counter()-started
            report["operations"]["concurrency"] = {"workers":a.workers,"queries":len(clat),
                "elapsed_seconds":round(elapsed,3),"throughput_queries_per_second":round(len(clat)/max(elapsed,.0001),2),
                "p50_ms":percentile(clat,.5),"p95_ms":percentile(clat,.95)}
            write_start = time.perf_counter()
            with concurrent.futures.ThreadPoolExecutor(max_workers=8) as executor:
                list(executor.map(lambda worker: write_ops_events(a.dsn,schema,worker),range(8)))
            write_elapsed = time.perf_counter()-write_start
            report["operations"]["concurrent_writes"] = {
                "workers":8,"rows":4000,"elapsed_seconds":round(write_elapsed,3),
                "rows_per_second":round(4000/max(write_elapsed,.0001),2),
                "transaction_batch_rows":50,
            }
            (_, reindex_s) = timed(lambda: conn.execute(f'REINDEX TABLE "{schema}".chunks'))
            report["operations"]["reindex_seconds"] = reindex_s
            def migration_probe():
                conn.execute(f'ALTER TABLE "{schema}".docs ADD COLUMN migration_probe INTEGER')
                conn.execute(f'CREATE INDEX docs_probe_idx ON "{schema}".docs(migration_probe)')
            (_, migration_s) = timed(migration_probe)
            report["operations"]["migration_probe_seconds"] = migration_s
            sizes = conn.execute(f"""SELECT pg_total_relation_size('"{schema}".docs')+pg_total_relation_size('"{schema}".chunks')+pg_total_relation_size('"{schema}".ops_events'),
                pg_relation_size('"{schema}".docs')+pg_relation_size('"{schema}".chunks')+pg_relation_size('"{schema}".ops_events'),
                pg_indexes_size('"{schema}".docs')+pg_indexes_size('"{schema}".chunks')+pg_indexes_size('"{schema}".ops_events')""").fetchone()
            stats = conn.execute("SELECT blks_hit,blks_read FROM pg_stat_database WHERE datname=current_database()").fetchone()
            report["operations"]["storage"] = {"total_bytes":sizes[0],"heap_bytes":sizes[1],"index_bytes":sizes[2],
                "database_cache_hits":stats[0],"database_reads":stats[1]}
            backend_memory = conn.execute(
                "SELECT COALESCE(SUM(total_bytes),0),COUNT(*) FROM pg_backend_memory_contexts"
            ).fetchone()
            host_memory = {}
            try:
                for line in Path("/proc/meminfo").read_text().splitlines():
                    key, raw = line.split(":", 1)
                    if key in ("MemTotal", "MemAvailable"):
                        host_memory[key.lower() + "_bytes"] = int(raw.strip().split()[0]) * 1024
            except (OSError, ValueError):
                pass
            active_backends = conn.execute(
                "SELECT COUNT(*) FROM pg_stat_activity WHERE datname=current_database()"
            ).fetchone()[0]
            report["operations"]["memory"] = {
                "shared_buffers": report["environment"]["shared_buffers"],
                "current_backend_context_bytes": int(backend_memory[0]),
                "current_backend_context_count": backend_memory[1],
                "active_database_backends": active_backends,
                "host_memory": host_memory,
                "note": "backend context is sampled after benchmark queries; host memory is system-level, not PostgreSQL process RSS",
            }
            report["operations"]["retrieval_plan"] = conn.execute(f"""EXPLAIN (ANALYZE, BUFFERS, FORMAT JSON)
                SELECT doc_id FROM "{schema}".chunks WHERE search_vector @@
                plainto_tsquery('simple',%s) LIMIT 20""", (search_term,)).fetchone()[0]
            report["operations"]["counts"] = {"documents":conn.execute(f'SELECT COUNT(*) FROM "{schema}".docs').fetchone()[0],
                "chunks":conn.execute(f'SELECT COUNT(*) FROM "{schema}".chunks').fetchone()[0],
                "ops_events":conn.execute(f'SELECT COUNT(*) FROM "{schema}".ops_events').fetchone()[0]}
            report["operations"]["schema"] = schema
        if not a.skip_backup_restore:
            (_, dump_s) = timed(lambda: subprocess.run(["pg_dump","--format=custom","--no-owner","--no-acl",
                "--schema",schema,"--dbname",a.dsn,"--file",str(dump_path)],check=True,capture_output=True,text=True))
            backup_created = True
            (_, create_s) = timed(lambda: subprocess.run(["createdb","--maintenance-db",a.dsn,restore_db],
                check=True,capture_output=True,text=True))
            restore_dsn = make_conninfo(a.dsn,dbname=restore_db)
            (_, restore_s) = timed(lambda: subprocess.run(["pg_restore","--no-owner","--no-acl","--dbname",
                restore_dsn,str(dump_path)],check=True,capture_output=True,text=True))
            with psycopg.connect(restore_dsn) as restored:
                restored_docs = restored.execute(f'SELECT COUNT(*) FROM "{schema}".docs').fetchone()[0]
                restored_chunks = restored.execute(f'SELECT COUNT(*) FROM "{schema}".chunks').fetchone()[0]
                restored_events = restored.execute(f'SELECT COUNT(*) FROM "{schema}".ops_events').fetchone()[0]
            expected_docs=a.docs; expected_chunks=a.docs*a.chunks_per_doc; expected_events=4000
            if restored_docs != expected_docs or restored_chunks != expected_chunks or restored_events != expected_events:
                raise RuntimeError(f"restore mismatch docs={restored_docs}/{expected_docs} chunks={restored_chunks}/{expected_chunks} events={restored_events}/{expected_events}")
            report["operations"]["backup_restore"]={"dump_seconds":dump_s,"create_database_seconds":create_s,
                "restore_seconds":restore_s,"restored_documents":restored_docs,"restored_chunks":restored_chunks,
                "restored_ops_events":restored_events,"verified":True,"dump_bytes":dump_path.stat().st_size}
    finally:
        try:
            with psycopg.connect(a.dsn,autocommit=True) as conn: conn.execute(f'DROP SCHEMA IF EXISTS "{schema}" CASCADE')
        except Exception as exc: report["limitations"].append(f"cleanup failed: {exc}")
        if backup_created:
            subprocess.run(["dropdb","--maintenance-db",a.dsn,"--if-exists",restore_db],check=False)
            dump_path.unlink(missing_ok=True)
    report["finished_at"]=datetime.now(timezone.utc).isoformat(); report["status"]="completed"
    json_path.write_text(json.dumps(report,ensure_ascii=False,indent=2)+"\n",encoding="utf-8")
    rows="\n".join(f"| {x['documents_total']:,} | {x['chunks_total']:,} | {x['document_import_seconds']} | {x['chunk_import_seconds']} | {x['documents_per_second']} | {x['chunks_per_second']} |" for x in report["tiers"])
    ops=report["operations"]
    md=f"""# Phase 8 Scale Benchmark

- Started: {report['started_at']}
- Finished: {report['finished_at']}
- PostgreSQL: {report['environment'].get('postgres_version')}
- Platform: {report['environment']['platform']}
- CPU count: {report['environment']['cpu_count']}
- Shared buffers: {report['environment'].get('shared_buffers')}
- Isolated schema: {schema} (dropped after test)

## Import

| Documents | Chunks | Document import (s) | Chunk import (s) | Documents/s | Chunks/s |
|---:|---:|---:|---:|---:|---:|
{rows}

## Index / retrieval / concurrency

- Index build: {ops.get('index_build_seconds')} s
- Retrieval p50/p95/max: {ops.get('retrieval',{}).get('p50_ms')} / {ops.get('retrieval',{}).get('p95_ms')} / {ops.get('retrieval',{}).get('max_ms')} ms
- Concurrent throughput: {ops.get('concurrency',{}).get('throughput_queries_per_second')} q/s; p95 {ops.get('concurrency',{}).get('p95_ms')} ms
- Concurrent operational writes: {ops.get('concurrent_writes',{}).get('rows_per_second')} rows/s (8 writers, 4,000 events)
- Reindex: {ops.get('reindex_seconds')} s
- Additive migration probe: {ops.get('migration_probe_seconds')} s
- Memory sample: {json.dumps(ops.get('memory',{}),ensure_ascii=False)}

## Storage / backup / restore

- Total relation bytes: {ops.get('storage',{}).get('total_bytes')}
- Heap bytes: {ops.get('storage',{}).get('heap_bytes')}
- Index bytes: {ops.get('storage',{}).get('index_bytes')}
- Backup/restore: {json.dumps(ops.get('backup_restore',{'skipped':True}),ensure_ascii=False)}

## Query plan

{json.dumps(ops.get('retrieval_plan'),ensure_ascii=False,indent=2)}

## Interpretation

Synthetic PostgreSQL workload, not a claim about real corpus quality. Use these
measurements to identify bottlenecks. No SQLite-to-PostgreSQL migration decision
is made without a comparable production-shaped workload and restore drill.
"""
    md_path.write_text(md,encoding="utf-8")
    print(json.dumps({"status":"completed","json_report":str(json_path),"markdown_report":str(md_path),
        "counts":ops.get("counts"),"retrieval":ops.get("retrieval"),"concurrency":ops.get("concurrency"),
        "storage":ops.get("storage"),"backup_restore":ops.get("backup_restore")},ensure_ascii=False,indent=2))
    return 0

if __name__ == "__main__":
    raise SystemExit(main())
