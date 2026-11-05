#!/usr/bin/env python3
"""ERI P1 测试套件（对应实施计划第十一节 14 项，全部离线）。

传输层注入：enrich(transport=...) 接收 (url, headers) -> (status, body, headers)。
运行：python3 -I tests/run_p1_tests.py
"""
import itertools
import json
import os
import shutil
import sqlite3
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, ".."))

TMP = tempfile.mkdtemp(prefix="eri-p1-test-")
os.environ["ERI_DB"] = os.path.join(TMP, "catalog.db")
import eri  # noqa: E402
import eri_p1 as p1  # noqa: E402

_db_counter = itertools.count()
FAILURES = []


def check(name, cond, detail=""):
    if cond:
        print(f"  ok   {name}")
    else:
        print(f"  FAIL {name}  {detail}")
        FAILURES.append((name, detail))


def fresh_db_with_catalog():
    """夹具 readme-basic.md：4 个 GitHub 仓库（alpha/anchor/beta/delta，按 ID 排序）+ 1 个网页。"""
    eri.DB_PATH = os.path.join(TMP, f"p1db-{next(_db_counter)}", "catalog.db")
    os.makedirs(os.path.dirname(eri.DB_PATH), exist_ok=True)
    eri.ingest(os.path.join(HERE, "fixtures", "readme-basic.md"), None, "t111",
               source_id="test")
    con = p1.p1_connect()
    for t in ("github_metadata", "enrichment_state", "d5_candidates", "quality_scores",
              "skill_mappings", "api_call_log"):
        con.execute(f"DELETE FROM {t}")
    con.commit()
    con.close()


def sample_payload(full_name, **over):
    p = {"id": 1234, "full_name": full_name,
         "html_url": f"https://github.com/{full_name}",
         "description": "A test repo", "homepage": None, "visibility": "public",
         "archived": False, "disabled": False, "fork": False, "parent": None,
         "default_branch": "main", "created_at": "2018-01-01T00:00:00Z",
         "updated_at": "2026-10-01T00:00:00Z", "pushed_at": "2026-09-01T00:00:00Z",
         "stargazers_count": 1200, "watchers_count": 60, "forks_count": 90,
         "open_issues_count": 12, "language": "Python", "topics": ["security"],
         "license": {"key": "mit", "name": "MIT License", "spdx_id": "MIT"}}
    p.update(over)
    return json.dumps(p).encode()


def canned_transport(responses):
    """responses: list of (status, body_bytes, headers_dict)；耗尽后抛错。"""
    calls = {"n": 0, "log": []}

    def transport(url, headers):
        calls["n"] += 1
        calls["log"].append({"url": url, "headers": dict(headers)})
        if calls["n"] > len(responses):
            raise RuntimeError("canned transport exhausted")
        status, body, hd = responses[calls["n"] - 1]
        return status, body, hd
    return transport, calls


# ── 1/2/3. 元数据解析、License 区分、维护判断 ───────────────────────────────
def test_parse_and_classify():
    print("[P1-1/2/3] metadata parse / license / maintenance")
    row = p1.parse_repo_payload(json.loads(sample_payload("a/b")))
    check("fields parsed", row["stargazers_count"] == 1200 and row["language"] == "Python"
          and row["repository_id"] == 1234)
    check("license IDENTIFIED/MIT", row["license_status"] == "IDENTIFIED"
          and row["license_spdx_id"] == "MIT")
    check("maintenance ACTIVE", row["maintenance_status"] == "ACTIVE")

    lic_cases = [
        ({"key": "mit", "name": "MIT", "spdx_id": "MIT"}, "IDENTIFIED"),
        ({"key": "other", "name": "Other", "spdx_id": "NOASSERTION"}, "FILE_UNIDENTIFIED"),
        (None, "NO_LICENSE_FILE"),
    ]
    for api_license, want in lic_cases:
        st, _, _, _ = p1.classify_license(api_license)
        check(f"license {want}", st == want, st)

    row = p1.parse_repo_payload(json.loads(sample_payload("a/b", archived=True)))
    check("archived -> ARCHIVED", row["maintenance_status"] == "ARCHIVED")
    row = p1.parse_repo_payload(json.loads(sample_payload("a/b", disabled=True)))
    check("disabled -> DISABLED", row["maintenance_status"] == "DISABLED")
    row = p1.parse_repo_payload(json.loads(sample_payload(
        "a/b", pushed_at="2020-01-01T00:00:00Z")))
    check("old push -> STALE", row["maintenance_status"] == "STALE")
    row = p1.parse_repo_payload(json.loads(sample_payload("a/b", license=None)))
    check("license null -> NO_LICENSE_FILE (不等于确证无许可)",
          row["license_status"] == "NO_LICENSE_FILE")
    check("gone -> UNAVAILABLE",
          p1.classify_maintenance(0, 0, None, "gone") == "UNAVAILABLE")


# ── 4/5/6. 断点续跑、限流模拟、ETag/304 ─────────────────────────────────────
def test_enrich_engine():
    print("[P1-4/5/6] resume / rate-limit / etag-304")
    fresh_db_with_catalog()
    # 排序后目标：alpha-tool, anchor, beta-tool, delta（网页 gamma 不补全）
    # 前 2 个 200，第 3 个（beta-tool）403 限流 -> beta/delta 标记 rate_limited
    ok_hdr = {"etag": "W/\"abc\"", "x-ratelimit-remaining": "50"}
    responses = [
        (200, sample_payload("example/alpha-tool"), ok_hdr),
        (200, sample_payload("example/anchor"), ok_hdr),
        (403, b"{}", {"x-ratelimit-remaining": "0"}),
    ]
    transport, calls = canned_transport(responses)
    out = p1.enrich(mode="all", transport=transport, backup_first=False)
    check("stopped on rate limit", out["stopped_reason"] is not None, out)
    check("2 ok before stop", out["ok"] == 2, out)
    check("website not targeted", all("example.com" not in c["url"] for c in calls["log"]),
          calls["log"])
    con = p1.p1_connect()
    st = {r["resource_id"]: r["status"] for r in con.execute(
        "SELECT resource_id, status FROM enrichment_state")}
    con.close()
    check("done x2 (alpha, anchor)", st.get("gh:example/alpha-tool") == "done"
          and st.get("gh:example/anchor") == "done", st)
    check("rate_limited marked (beta hit 403)", st.get("gh:example/beta-tool")
          == "rate_limited", st)
    check("blocked tail marked", st.get("gh:example/delta") == "rate_limited", st)

    # 续跑 pending：只补 beta/delta，不重打已完成的 alpha/anchor
    transport2, calls2 = canned_transport([
        (200, sample_payload("example/beta-tool"), ok_hdr),
        (200, sample_payload("example/delta"), ok_hdr),
        (200, sample_payload("example/unused"), ok_hdr),
    ])
    out2 = p1.enrich(mode="pending", transport=transport2, backup_first=False)
    check("resume completes rest", out2["ok"] == 2 and out2["stopped_reason"] is None, out2)
    check("resume skips done (2 calls only)", calls2["n"] == 2, calls2["n"])
    check("no alpha re-fetch", all("alpha" not in c["url"] for c in calls2["log"]))
    con = p1.p1_connect()
    done_n = con.execute("SELECT COUNT(*) FROM enrichment_state"
                         " WHERE status='done'").fetchone()[0]
    meta_n = con.execute("SELECT COUNT(*) FROM github_metadata"
                         " WHERE fetch_status='ok'").fetchone()[0]
    con.close()
    check("all 4 done after resume", done_n == 4, done_n)
    check("metadata rows written", meta_n == 4, meta_n)

    # ETag/304：再跑 all，全部带 If-None-Match -> 304，旧数据保留
    transport3, calls3 = canned_transport([
        (304, b"", {"etag": "W/\"abc\"", "x-ratelimit-remaining": "55"}) for _ in range(4)
    ])
    out3 = p1.enrich(mode="all", transport=transport3, backup_first=False)
    check("all 304", out3["not_modified"] == 4, out3)
    sent = [c["headers"].get("If-None-Match") for c in calls3["log"]]
    check("etag sent on re-run", all(s == 'W/"abc"' for s in sent), sent)
    con = p1.p1_connect()
    kept = con.execute("SELECT stargazers_count FROM github_metadata"
                       " WHERE resource_id='gh:example/alpha-tool'").fetchone()
    con.close()
    check("304 keeps old data", kept and kept["stargazers_count"] == 1200)

    # 404：记录 UNAVAILABLE 并继续（delta 复活场景：先 gone）
    transport4, _ = canned_transport([(404, b"{}", {})])
    out4 = p1.enrich(mode="ids", ids=["gh:example/delta"], transport=transport4,
                     backup_first=False)
    check("404 gone recorded", out4["gone"] == 1, out4)
    con = p1.p1_connect()
    row = con.execute("SELECT fetch_status, maintenance_status FROM github_metadata"
                      " WHERE resource_id='gh:example/delta'").fetchone()
    res_status = con.execute("SELECT status FROM resources WHERE"
                             " resource_id='gh:example/delta'").fetchone()
    con.close()
    check("404 -> UNAVAILABLE", row["fetch_status"] == "gone"
          and row["maintenance_status"] == "UNAVAILABLE")
    check("resource status UNAVAILABLE", res_status["status"] == "UNAVAILABLE")


# ── 7/8. D5 与 Fork 识别 ────────────────────────────────────────────────────
def test_d5():
    print("[P1-7/8] D5 candidates / fork")
    fresh_db_with_catalog()
    con = p1.p1_connect()
    # 插入元数据：delta 是 alpha 的 fork；beta 维护者另名同库
    con.execute(
        "INSERT INTO github_metadata(resource_id, full_name, fork, parent_repository)"
        " VALUES ('gh:example/alpha-tool','example/alpha-tool',0,NULL),"
        " ('gh:example/delta','example/delta',1,'example/alpha-tool'),"
        " ('gh:example/beta-tool','example/beta-tool',0,NULL)")
    con.commit()
    con.close()
    out = p1.d5()
    check("d5 runs", out["recorded"] >= 1, out)
    con = p1.p1_connect()
    fork = con.execute("SELECT * FROM d5_candidates WHERE decision='FORK'").fetchall()
    review = con.execute("SELECT * FROM d5_candidates WHERE decision='NEEDS_REVIEW'"
                         " AND ((resource_a='gh:example/alpha-tool' AND"
                         " resource_b='gh:example/beta-tool') OR (resource_a="
                         "'gh:example/beta-tool' AND resource_b='gh:example/alpha-tool'))"
                         ).fetchall()
    n = con.execute("SELECT COUNT(*) FROM resources").fetchone()[0]
    con.close()
    check("FORK detected (alpha <- delta)", len(fork) == 1
          and {fork[0]["resource_a"], fork[0]["resource_b"]}
          == {"gh:example/alpha-tool", "gh:example/delta"},
          [dict(x) for x in fork])
    check("similar-name pair flagged for review", len(review) >= 1,
          [dict(x) for x in review])
    check("no deletion after d5", n == 5, n)


# ── 9/10. 质量评分与隔离 ────────────────────────────────────────────────────
def test_quality_quarantine():
    print("[P1-9/10] quality / quarantine")
    fresh_db_with_catalog()
    con = p1.p1_connect()
    # alpha: 优质 MIT ACTIVE；beta: 无许可 STALE；delta: disabled
    con.execute("INSERT INTO github_metadata(resource_id, license_status, license_spdx_id,"
                " maintenance_status, stargazers_count, created_at, pushed_at, homepage,"
                " topics, forks_count, open_issues_count, language, description,"
                " fetch_status, archived, disabled) VALUES"
                " ('gh:example/alpha-tool','IDENTIFIED','MIT','ACTIVE',8000,"
                " '2015-01-01T00:00:00Z','2026-10-01T00:00:00Z','https://a.example',"
                " '[\"pentest\"]',300,10,'Go','exploit framework','ok',0,0),"
                " ('gh:example/beta-tool','NO_LICENSE_FILE',NULL,'STALE',100,"
                " '2016-01-01T00:00:00Z','2019-01-01T00:00:00Z',NULL,'[]',10,300,NULL,"
                " 'old thing','ok',0,0),"
                " ('gh:example/delta','IDENTIFIED','MIT','DISABLED',500,"
                " '2017-01-01T00:00:00Z','2026-08-01T00:00:00Z',NULL,'[]',50,5,'C',"
                " 'tool','ok',0,1)")
    con.commit(); con.close()
    out = p1.quality()
    check("quality scored all 5", out["scored"] == 5, out)
    con = p1.p1_connect()
    qa = con.execute("SELECT * FROM quality_scores WHERE"
                     " resource_id='gh:example/alpha-tool'").fetchone()
    qb = con.execute("SELECT * FROM quality_scores WHERE"
                     " resource_id='gh:example/beta-tool'").fetchone()
    delta_status = con.execute("SELECT status FROM resources WHERE"
                               " resource_id='gh:example/delta'").fetchone()
    con.close()
    check("alpha > beta", qa["final_score"] > qb["final_score"],
          f"{qa['final_score']} vs {qb['final_score']}")
    check("reasons breakdown saved", "reliability" in json.loads(qa["reasons"]))
    check("beta adoption blocked (license)", qb["adoption_blocked"] == 1)
    check("alpha not blocked", qa["adoption_blocked"] == 0)
    check("beta grade capped (<=B)", qb["grade"] in ("B", "C", "D"))
    check("disabled auto-quarantined", delta_status["status"] == "QUARANTINED")

    # 用真实的网页资源（夹具 Gamma Site -> url:<hash>）做手工隔离
    con = p1.p1_connect()
    web_id = con.execute("SELECT resource_id FROM resources WHERE"
                         " resource_type='website'").fetchone()[0]
    con.close()
    msg = p1.quarantine(web_id, "manual test")
    con = p1.p1_connect()
    s = con.execute("SELECT status FROM resources WHERE resource_id=?",
                    (web_id,)).fetchone()
    rv = con.execute("SELECT * FROM resource_reviews WHERE resource_id=?",
                     (web_id,)).fetchone()
    con.close()
    check("manual quarantine", s["status"] == "QUARANTINED"
          and rv["decision_reason"] == "manual test", (web_id, msg))
    p1.quarantine(web_id, "release", undo=True)
    con = p1.p1_connect()
    s2 = con.execute("SELECT status FROM resources WHERE resource_id=?",
                     (web_id,)).fetchone()
    con.close()
    check("unquarantine restores", s2["status"] == "INDEXED", s2)
    check("quarantine unknown id is safe",
          "not found" in p1.quarantine("gh:nope/nope", "x"))

    # 幂等：重跑 quality 结果一致
    qa_before = qa["final_score"]
    p1.quality()
    con = p1.p1_connect()
    qa2 = con.execute("SELECT final_score FROM quality_scores WHERE"
                      " resource_id='gh:example/alpha-tool'").fetchone()
    n_rows = con.execute("SELECT COUNT(*) FROM quality_scores").fetchone()[0]
    con.close()
    check("quality idempotent", qa2["final_score"] == qa_before and n_rows == out["scored"])


# ── 11. Skills 映射 ─────────────────────────────────────────────────────────
def test_skill_mapping():
    print("[P1-11] skill mapping")
    fresh_db_with_catalog()
    tmp_skills = os.path.join(TMP, "skills")
    os.makedirs(tmp_skills, exist_ok=True)
    open(os.path.join(tmp_skills, "web-sec.md"), "w", encoding="utf-8").write(
        "---\nname: web-sec\ndescription: alpha payload testing framework, sqli, api fuzzing\n"
        "whenToUse: web pentest\nrole: vuln-scan\n---\nbody")
    out = p1.map_skills(skills_dir=tmp_skills,
                        tools_path=os.path.join(TMP, "no-tools.json"))
    check("skills loaded", out["skills_loaded"] == 1, out)
    con = p1.p1_connect()
    rows = con.execute("SELECT * FROM skill_mappings WHERE mapping_kind='skill'").fetchall()
    con.close()
    check("mapping candidates created", len(rows) >= 1, [dict(r) for r in rows])
    if rows:
        check("status SUGGESTED only", all(r["mapping_status"] == "SUGGESTED" for r in rows))
        check("evidence tokens saved", "shared_tokens" in json.loads(rows[0]["evidence"]))
    # tools.json 命中
    tools_path = os.path.join(TMP, "tools.json")
    json.dump({"items": [{"name": "alpha-tool"}]}, open(tools_path, "w"))
    p1.map_skills(skills_dir=tmp_skills, tools_path=tools_path)
    con = p1.p1_connect()
    t = con.execute("SELECT * FROM skill_mappings WHERE target='tool:alpha-tool'").fetchone()
    con.close()
    check("tool name exact match", t is not None and t["confidence"] == 0.8)


# ── 13. 并发冲突 ────────────────────────────────────────────────────────────
def test_concurrency():
    print("[P1-13] concurrent write conflict")
    fresh_db_with_catalog()
    c1 = sqlite3.connect(eri.DB_PATH, timeout=30, isolation_level=None)
    c1.execute("BEGIN IMMEDIATE")
    c1.execute("INSERT INTO api_call_log(ts, url, status) VALUES ('x','y',200)")
    c2 = sqlite3.connect(eri.DB_PATH, timeout=0.3, isolation_level=None)
    collided = False
    try:
        c2.execute("BEGIN IMMEDIATE")
        c2.execute("INSERT INTO api_call_log(ts, url, status) VALUES ('z','w',200)")
        c2.execute("COMMIT")
    except sqlite3.OperationalError as e:
        collided = "locked" in str(e).lower()
        try:
            c2.execute("ROLLBACK")
        except sqlite3.OperationalError:
            pass
    c1.execute("COMMIT")
    c1.close(); c2.close()
    check("write conflict detected, no corruption", collided)
    con = p1.p1_connect()
    n = con.execute("SELECT COUNT(*) FROM api_call_log").fetchone()[0]
    ok = con.execute("PRAGMA integrity_check").fetchone()[0]
    con.close()
    check("db integrity ok after conflict", ok == "ok" and n == 1, (ok, n))


# ── 14. 备份与恢复 ─────────────────────────────────────────────────────────
def test_backup_restore():
    print("[P1-14] backup / restore")
    fresh_db_with_catalog()
    p1.BACKUP_DIR = os.path.join(TMP, "backups")    # 备份进测试目录
    p1.quality()
    bak = p1.backup()
    check("backup file exists", os.path.exists(bak) and os.path.getsize(bak) > 0, bak)
    # 破坏后恢复
    con = p1.p1_connect()
    con.execute("DELETE FROM quality_scores"); con.commit(); con.close()
    msg = p1.restore(bak, confirm=True)
    con = p1.p1_connect()
    n = con.execute("SELECT COUNT(*) FROM quality_scores").fetchone()[0]
    res_n = con.execute("SELECT COUNT(*) FROM resources").fetchone()[0]
    con.close()
    check("restore recovers data", n > 0 and res_n == 5, (n, res_n))
    check("restore refuses without confirm", "refusing" in p1.restore(bak))


# ── 查询/统计冒烟 ───────────────────────────────────────────────────────────
def test_query_stats():
    print("[P1-extra] query / stats / reports")
    rows = p1.query(grade="A", limit=5)
    check("query grade filter", isinstance(rows, list))
    rows = p1.query(keyword="alpha")
    check("query keyword", any("alpha" in r["resource_id"] for r in rows))
    st = p1.stats()
    check("stats keys", all(k in st for k in ("metadata", "grades", "d5", "skill_mappings")))
    p1.REPORTS_DIR = os.path.join(TMP, "reports")   # 报告写测试目录，不污染真实目录
    written = p1.generate_reports()
    check("reports written", all(os.path.exists(p) for p in written.values()), written)


def main():
    tests = [test_parse_and_classify, test_enrich_engine, test_d5,
             test_quality_quarantine, test_skill_mapping, test_concurrency,
             test_backup_restore, test_query_stats]
    for t in tests:
        try:
            t()
        except Exception as e:  # noqa: BLE001
            import traceback
            traceback.print_exc()
            FAILURES.append((t.__name__, repr(e)))
    print()
    if FAILURES:
        print(f"FAILED: {len(FAILURES)}")
        for name, d in FAILURES:
            print(f"  - {name}: {d}")
        sys.exit(1)
    print("ALL P1 TESTS PASS")
    shutil.rmtree(TMP, ignore_errors=True)


if __name__ == "__main__":
    main()
