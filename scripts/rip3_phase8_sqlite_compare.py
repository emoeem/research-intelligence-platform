#!/usr/bin/env python3
"""RIP3 Phase 8 SQLite WAL comparison for the same synthetic docs/chunks workload."""
from __future__ import annotations
import argparse, concurrent.futures, json, os, resource, sqlite3, statistics, tempfile, threading, time, uuid
from datetime import datetime, timezone
from pathlib import Path

def pct(values, p):
    ordered=sorted(values)
    return round(ordered[min(len(ordered)-1,int((len(ordered)-1)*p))],3) if ordered else 0.0

def timed(fn):
    start=time.perf_counter(); value=fn(); return value,round(time.perf_counter()-start,3)

def main():
    ap=argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--docs",type=int,default=100_000)
    ap.add_argument("--chunks-per-doc",type=int,default=10)
    ap.add_argument("--workers",type=int,default=16)
    ap.add_argument("--queries",type=int,default=100)
    ap.add_argument("--output-dir",default="docs/rip3/benchmarks")
    a=ap.parse_args()
    if a.docs<1000 or a.chunks_per_doc<1: ap.error("invalid scale")
    out=Path(a.output_dir); out.mkdir(parents=True,exist_ok=True)
    stamp=datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%SZ")
    with tempfile.TemporaryDirectory(prefix="rip3-sqlite-bench-") as tmp:
        db=Path(tmp)/"bench.sqlite"
        def connect():
            c=sqlite3.connect(db,timeout=60)
            c.execute("PRAGMA busy_timeout=60000")
            return c
        conn=connect()
        conn.execute("PRAGMA journal_mode=WAL")
        conn.execute("PRAGMA synchronous=NORMAL")
        conn.execute("PRAGMA temp_store=MEMORY")
        conn.execute("CREATE TABLE docs(doc_id INTEGER PRIMARY KEY,title TEXT NOT NULL,body TEXT NOT NULL,checksum TEXT NOT NULL)")
        conn.execute("CREATE TABLE chunks(chunk_id INTEGER PRIMARY KEY,doc_id INTEGER NOT NULL,ordinal INTEGER NOT NULL,body TEXT NOT NULL)")
        conn.execute("CREATE VIRTUAL TABLE chunks_fts USING fts5(body,content='chunks',content_rowid='chunk_id')")
        conn.execute("CREATE TABLE ops_events(event_id INTEGER PRIMARY KEY AUTOINCREMENT,caller_id TEXT NOT NULL,event_type TEXT NOT NULL,payload TEXT NOT NULL,created_at TEXT NOT NULL)")
        conn.execute("CREATE INDEX ops_events_caller_created ON ops_events(caller_id,created_at)")
        tiers=sorted(set(x for x in (1000,10000,a.docs) if x<=a.docs))
        report={"schema_version":"rip3-phase8-sqlite-compare-v1","started_at":datetime.now(timezone.utc).isoformat(),
            "environment":{"python":__import__("sys").version.split()[0],"sqlite_version":sqlite3.sqlite_version,
            "docs_target":a.docs,"chunks_per_doc":a.chunks_per_doc,"workers":a.workers,"queries":a.queries},
            "tiers":[],"operations":{}}
        previous=0
        for tier in tiers:
            doc_rows=((i,f"Synthetic research document {i}",f"research document {i} parameter validation alpha beta needle markerx{i}",f"{i:064x}") for i in range(previous,tier))
            _,doc_s=timed(lambda: conn.executemany("INSERT INTO docs VALUES(?,?,?,?)",doc_rows))
            chunk_rows=((i*a.chunks_per_doc+j,i,j,
                f"document-{i} chunk-{j} research security parameter validation needle payload alpha beta markerx{i}")
                for i in range(previous,tier) for j in range(a.chunks_per_doc))
            _,chunk_s=timed(lambda: conn.executemany("INSERT INTO chunks VALUES(?,?,?,?)",chunk_rows))
            conn.commit()
            dn=tier-previous; cn=dn*a.chunks_per_doc
            report["tiers"].append({"documents_total":tier,"chunks_total":tier*a.chunks_per_doc,
                "documents_added":dn,"chunks_added":cn,"document_import_seconds":doc_s,"chunk_import_seconds":chunk_s,
                "documents_per_second":round(dn/max(doc_s,.0001),1),"chunks_per_second":round(cn/max(chunk_s,.0001),1)})
            previous=tier
        _,index_s=timed(lambda: conn.execute("INSERT INTO chunks_fts(chunks_fts) VALUES('rebuild')"))
        conn.commit()
        report["operations"]["index_build_seconds"]=index_s
        term=f"markerx{a.docs//2}"
        def query_once():
            c=connect()
            try: c.execute("SELECT rowid FROM chunks_fts WHERE chunks_fts MATCH ? LIMIT 20",(term,)).fetchall()
            finally: c.close()
        for _ in range(10): query_once()
        lat=[]
        for _ in range(a.queries):
            t=time.perf_counter(); query_once(); lat.append((time.perf_counter()-t)*1000)
        report["operations"]["retrieval"]={"samples":len(lat),"p50_ms":pct(lat,.5),"p95_ms":pct(lat,.95),
            "max_ms":round(max(lat),3),"mean_ms":round(statistics.mean(lat),3)}
        start=time.perf_counter()
        with concurrent.futures.ThreadPoolExecutor(max_workers=a.workers) as pool:
            clat=list(pool.map(lambda _: (lambda t: (query_once(),(time.perf_counter()-t)*1000)[1])(time.perf_counter()),range(a.queries)))
        elapsed=time.perf_counter()-start
        report["operations"]["concurrency"]={"workers":a.workers,"queries":len(clat),"elapsed_seconds":round(elapsed,3),
            "throughput_queries_per_second":round(len(clat)/max(elapsed,.0001),2),"p50_ms":pct(clat,.5),"p95_ms":pct(clat,.95)}
        # Concurrent operational-event writers model append-heavy audit/run telemetry.
        def writer(worker):
            c=connect()
            try:
                for batch in range(10):
                    rows=[(f"caller-{worker%4}","tool_run",f'{{"worker":{worker},"batch":{batch},"i":{i}}}',datetime.now(timezone.utc).isoformat()) for i in range(50)]
                    c.executemany("INSERT INTO ops_events(caller_id,event_type,payload,created_at) VALUES(?,?,?,?)",rows)
                    c.commit()
            finally: c.close()
        start=time.perf_counter()
        with concurrent.futures.ThreadPoolExecutor(max_workers=8) as pool:
            list(pool.map(writer,range(8)))
        write_s=round(time.perf_counter()-start,3)
        report["operations"]["concurrent_writes"]={"workers":8,"rows":4000,"elapsed_seconds":write_s,
            "rows_per_second":round(4000/max(write_s,.0001),2),"sqlite_wal_single_writer":True}
        _,migration_s=timed(lambda: conn.execute("ALTER TABLE docs ADD COLUMN migration_probe INTEGER"))
        _,rebuild_s=timed(lambda: conn.execute("INSERT INTO chunks_fts(chunks_fts) VALUES('rebuild')"))
        conn.commit()
        report["operations"]["migration_probe_seconds"]=migration_s
        report["operations"]["index_rebuild_seconds"]=rebuild_s
        conn.execute("PRAGMA wal_checkpoint(TRUNCATE)")
        page_count=conn.execute("PRAGMA page_count").fetchone()[0]
        page_size=conn.execute("PRAGMA page_size").fetchone()[0]
        freelist=conn.execute("PRAGMA freelist_count").fetchone()[0]
        report["operations"]["storage"]={"database_bytes":db.stat().st_size,"page_count":page_count,
            "page_size":page_size,"allocated_bytes":page_count*page_size,"freelist_pages":freelist,
            "wal_bytes":Path(str(db)+"-wal").stat().st_size if Path(str(db)+"-wal").exists() else 0}
        report["operations"]["memory"]={"process_max_rss_bytes":resource.getrusage(resource.RUSAGE_SELF).ru_maxrss*1024,
            "cache_size_pages":conn.execute("PRAGMA cache_size").fetchone()[0],
            "note":"process peak RSS includes Python harness and SQLite; not an isolated SQLite engine measurement"}
        report["operations"]["counts"]={"documents":conn.execute("SELECT COUNT(*) FROM docs").fetchone()[0],
            "chunks":conn.execute("SELECT COUNT(*) FROM chunks").fetchone()[0],
            "ops_events":conn.execute("SELECT COUNT(*) FROM ops_events").fetchone()[0]}
        # SQLite online backup API and restore count verification.
        backup=Path(tmp)/"backup.sqlite"; restored=Path(tmp)/"restored.sqlite"
        _,backup_s=timed(lambda: (lambda dst: (conn.backup(dst),dst.close()))(sqlite3.connect(backup)))
        _,restore_s=timed(lambda: (lambda src,dst: (src.backup(dst),src.close(),dst.close()))(sqlite3.connect(backup),sqlite3.connect(restored)))
        with sqlite3.connect(restored) as verify:
            docs_n=verify.execute("SELECT COUNT(*) FROM docs").fetchone()[0]
            chunks_n=verify.execute("SELECT COUNT(*) FROM chunks").fetchone()[0]
        if docs_n!=a.docs or chunks_n!=a.docs*a.chunks_per_doc: raise RuntimeError("SQLite backup restore verification failed")
        report["operations"]["backup_restore"]={"backup_seconds":backup_s,"restore_seconds":restore_s,
            "restored_documents":docs_n,"restored_chunks":chunks_n,"verified":True,"backup_bytes":backup.stat().st_size}
        conn.close()
    report["finished_at"]=datetime.now(timezone.utc).isoformat()
    path=out/f"phase8-sqlite-{stamp}.json"
    path.write_text(json.dumps(report,ensure_ascii=False,indent=2)+"\n",encoding="utf-8")
    print(json.dumps({"status":"completed","report":str(path),"counts":report["operations"]["counts"],
        "retrieval":report["operations"]["retrieval"],"concurrency":report["operations"]["concurrency"],
        "concurrent_writes":report["operations"]["concurrent_writes"],"storage":report["operations"]["storage"],
        "memory":report["operations"]["memory"],"backup_restore":report["operations"]["backup_restore"]},indent=2))
    return 0

if __name__=="__main__": raise SystemExit(main())
