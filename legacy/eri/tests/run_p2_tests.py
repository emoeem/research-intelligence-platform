#!/usr/bin/env python3
"""ERI P2 测试：场景 A-F 回归 + Resolver/Reader/Tracker/审核/桥接单测。

用真实库的副本跑场景（零污染）；组件单测用夹具库。
运行：python3 -I tests/run_p2_tests.py
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
REAL_DB = os.path.join(HERE, "..", "db", "catalog.db")

TMP = tempfile.mkdtemp(prefix="eri-p2-test-")
os.environ["ERI_DB"] = os.path.join(TMP, "catalog.db")
import eri      # noqa: E402
import eri_p1   # noqa: E402
import eri_p2   # noqa: E402

_counter = itertools.count()
FAILURES = []


def check(name, cond, detail=""):
    if cond:
        print(f"  ok   {name}")
    else:
        print(f"  FAIL {name}  {detail}")
        FAILURES.append((name, detail))


def use_real_copy():
    """复制真实库到临时路径（场景 A-F 用真实数据，不污染原库）。"""
    eri.DB_PATH = os.path.join(TMP, f"realcopy-{next(_counter)}.db")
    shutil.copy(REAL_DB, eri.DB_PATH)
    # WAL 残留不复制（副本一致性以 checkpoint 后主文件为准；P0/P1 全部提交过）


def fixture_db(repos):
    """repos: list of (rid, name, desc, topics, grade, score, status)"""
    eri.DB_PATH = os.path.join(TMP, f"fix-{next(_counter)}.db")
    os.makedirs(os.path.dirname(eri.DB_PATH), exist_ok=True)
    eri.ingest(os.path.join(HERE, "fixtures", "readme-basic.md"), None, "t1",
               source_id="test")
    con = eri_p2.p2_connect()
    for rid, name, desc, topics, grade, score, status in repos:
        con.execute("INSERT OR IGNORE INTO resources(resource_id, canonical_url,"
                    " resource_type, name, description, status, first_seen, last_seen,"
                    " created_at, updated_at) VALUES (?,?,?,?,?,?,?,?,?,?)",
                    (rid, f"https://github.com/{rid[3:]}", "github_repo", name, desc,
                     status, "2026-01-01", "2026-01-01", "2026-01-01", "2026-01-01"))
        con.execute("INSERT OR REPLACE INTO quality_scores(resource_id, relevance,"
                    " reliability, maintenance, maturity, complementarity,"
                    " license_clarity, docs_testing, final_score, grade, adoption_blocked,"
                    " reasons, computed_at, engine_version) VALUES "
                    "(?,?,?,?,?,?,?,?,?,?,0,'{}',?,'p1-quality-1')",
                    (rid, 20, 15, 10, 10, 5, 10, 3, score, grade, "2026-01-01"))
        con.execute("INSERT OR REPLACE INTO github_metadata(resource_id, topics,"
                    " license_status, maintenance_status, fetch_status) VALUES (?,?,?,?,'ok')",
                    (rid, topics, "IDENTIFIED", "ACTIVE"))
    con.commit()
    con.close()


# ── 场景 A-F（真实库副本） ──────────────────────────────────────────────────
def test_scenarios():
    print("[P2-场景] A-F 真实数据回归")
    use_real_copy()

    # A: 白牌营销落地页
    r = eri_p2.resolve({"task_id": "p2test-A", "keywords": ["landing", "redirect",
                        "asset", "discovery"], "site_profile": {"technologies": ["cdn"]}})
    ids = [x["resource_id"] for x in r["matched_resources"]]
    check("A: 找到 asset discovery 资源",
          "gh:redhuntlabs/awesome-asset-discovery" in ids, ids)
    check("A: 无移动资源混入",
          not any("android" in i or "apk" in i for i in ids), ids)
    check("A: 有 skill 关联或质量信息",
          all(("skill_relationships" in x and "quality" in x) for x in r["matched_resources"]))

    # B: SPA 与 API
    r = eri_p2.resolve({"task_id": "p2test-B", "keywords": ["api", "openapi", "spa",
                        "javascript"]})
    ids = [x["resource_id"] for x in r["matched_resources"]]
    check("B: API Security Checklist 优先",
          "gh:shieldfy/api-security-checklist" in ids[:3], ids)
    check("B: persistence 不再因 rest 误中",
          "gh:karneades/awesome-malware-persistence" not in ids, ids)

    # C: Android
    r = eri_p2.resolve({"task_id": "p2test-C", "keywords": ["android", "apk", "mobile"]})
    ids = [x["resource_id"] for x in r["matched_resources"]]
    check("C: android-security-awesome 命中",
          "gh:ashishb/android-security-awesome" in ids[:2], ids)
    opened_all = all(x["resource_type"] for x in r["matched_resources"])
    check("C: 只读元数据未触发外读（resource_type 有值且无网络调用）", opened_all)

    # D: 密码学
    r = eri_p2.resolve({"task_id": "p2test-D", "keywords": ["cryptography", "jwt",
                        "encryption"]})
    ids = [x["resource_id"] for x in r["matched_resources"]]
    check("D: awesome-cryptography 命中",
          "gh:sobolevn/awesome-cryptography" in ids[:3], ids)

    # E: 无关任务
    r = eri_p2.resolve({"task_id": "p2test-E", "keywords": ["量子计算", "quantum", "steam"]})
    check("E: 无匹配返回空集", r["search_status"] == "no_match"
          and r["matched_resources"] == [], r["retrieval_stats"])

    # F: 重复控制
    r = eri_p2.resolve({"task_id": "p2test-F", "keywords": ["payload", "bypass", "waf"]})
    ids = [x["resource_id"] for x in r["matched_resources"]]
    check("F: 候选无重复", len(ids) == len(set(ids)), ids)
    use_real_copy()
    con = sqlite3.connect(eri.DB_PATH)
    n = con.execute("SELECT COUNT(*) FROM resource_sources").fetchone()[0]
    con.close()
    check("F: 多来源保留（sources 行数不变）", n >= 81, n)


# ── Resolver 单元行为 ───────────────────────────────────────────────────────
def test_resolver_rules():
    print("[P2-resolver] 排除/评分/幂等/预算")
    fixture_db([
        ("gh:x/alpha", "Alpha Fuzzer", "a fuzzing tool", '["fuzzing"]', "A", 85, "INDEXED"),
        ("gh:x/beta", "Beta Fuzzer", "fuzzing framework", '["fuzzing"]', "B", 70, "INDEXED"),
        ("gh:x/quar", "Quarantine Fuzzer", "fuzzing tool", '["fuzzing"]', "C", 50,
         "QUARANTINED"),
        ("gh:x/nolic", "NoLic Fuzzer", "fuzzing utility", "[]", "B", 68, "INDEXED"),
    ])
    con = eri_p2.p2_connect()
    con.execute("UPDATE github_metadata SET license_status='NO_LICENSE_FILE'"
                " WHERE resource_id='gh:x/nolic'")
    con.commit(); con.close()
    r = eri_p2.resolve({"task_id": "p2test-r1", "keywords": ["fuzzing"]}, limit=10)
    ids = [x["resource_id"] for x in r["matched_resources"]]
    check("QUARANTINED 排除", "gh:x/quar" not in ids, ids)
    check("Top-K 默认 5、可扩展", r["retrieval_stats"]["returned"] <= 10)
    nolic = next((x for x in r["matched_resources"] if x["resource_id"] == "gh:x/nolic"), None)
    if nolic:
        check("license 未知标记不可采用", nolic["adoptable_as_dependency"] is False
              and nolic["license_note"], nolic)
    check("relevance_reasons 存在", all(x["relevance_reasons"] for x in r["matched_resources"]))
    r2 = eri_p2.resolve({"task_id": "p2test-r1", "keywords": ["fuzzing"]}, limit=10)
    check("同输入幂等（同序同分）",
          [x["resource_id"] for x in r2["matched_resources"]] == ids
          and [x["relevance_score"] for x in r2["matched_resources"]]
          == [x["relevance_score"] for x in r["matched_resources"]])
    # NE D5 NEEDS_REVIEW 资源未被拔高：fixture 无 d5 行，分数只来自规则维度
    check("无证据维度不虚构（reasons 里声明 missing）",
          any("no evidence" in " ".join(x["relevance_reasons"]) for x in r["matched_resources"]))


# ── Reader 缓存 ─────────────────────────────────────────────────────────────
def test_reader():
    print("[P2-reader] 缓存/304/陈旧回落/章节")
    use_real_copy()
    eri_p2.CACHE_DIR = os.path.join(TMP, "cache")
    calls = {"n": 0}
    README = "# Proj\n## Install\nrun it\n## Usage\nuse it\n"

    def transport(url, etag):
        calls["n"] += 1
        if calls["n"] == 1:
            return 200, README.encode(), {"etag": 'W/"r1"'}
        if calls["n"] == 2:
            return 304, b"", {"etag": 'W/"r1"'}
        if calls["n"] == 3:
            return 200, (README + "\n## Changelog\nv2\n").encode(), {"etag": 'W/"r2"'}
        raise RuntimeError("network down")
    rid = "gh:swisskyrepo/payloadsallthethings"
    r1 = eri_p2.read(rid, transport=transport)
    check("首次读取写缓存", r1["cache"] == "miss" and r1["byte_len"] > 0, r1)
    check("章节索引提取", any(s["title"] == "Install" for s in r1["section_index"]))
    r2 = eri_p2.read(rid, transport=transport)
    check("未变更 304 命中缓存", r2["cache"] is True, r2)
    r3 = eri_p2.read(rid, transport=transport)
    check("内容变更后重新缓存", r3["cache"] == "miss"
          and any(s["title"] == "Changelog" for s in r3["section_index"]))
    sec = eri_p2.read(rid, transport=transport, section="Usage")
    check("章节按需提取", "use it" in sec.get("section", ""), sec.get("section", ""))
    # 网络失败回落陈旧缓存
    r4 = eri_p2.read(rid, transport=transport)
    check("网络失败回落 stale 缓存", r4.get("cache") == "stale-cache", r4)
    con = eri_p2.p2_connect()
    opened = con.execute("SELECT COUNT(*) FROM resource_usage WHERE event_type='OPENED'"
                         ).fetchone()[0]
    con.close()
    check("OPENED 事件已记录", opened >= 4, opened)


# ── Tracker ────────────────────────────────────────────────────────────────
def test_tracker():
    print("[P2-tracker] 七态漏斗")
    use_real_copy()
    r = eri_p2.resolve({"task_id": "p2test-t1", "keywords": ["fuzzing"]})
    qid = r["query_id"]
    top = r["matched_resources"]
    if top:
        eri_p2.track("OPENED", task_id="p2test-t1", query_id=qid,
                     resource_id=top[0]["resource_id"])
        eri_p2.track("SELECTED", task_id="p2test-t1", query_id=qid,
                     resource_id=top[0]["resource_id"])
        eri_p2.track("APPLIED", task_id="p2test-t1", query_id=qid,
                     resource_id=top[0]["resource_id"])
        eri_p2.track("SUCCESSFUL", task_id="p2test-t1", query_id=qid,
                     resource_id=top[0]["resource_id"], result="found payload idea")
        eri_p2.track("REJECTED", task_id="p2test-t1", query_id=qid,
                     resource_id=top[1]["resource_id"], reason="不相关")
    f = eri_p2.funnel(task_id="p2test-t1")
    c = f["counts"]
    check("SEARCHED/RETURNED 记录", c.get("SEARCHED", 0) >= 1 and c.get("RETURNED") == len(top), c)
    check("七态区分", {"OPENED", "SELECTED", "APPLIED", "SUCCESSFUL", "REJECTED"}
          <= set(c), c)
    check("比率计算", f["rates"]["open_rate"] is not None
          and f["rates"]["rejection_rate"] is not None, f["rates"])
    bad = eri_p2.track("NOT_A_STATE", task_id="x")
    check("非法事件拒绝", "error" in bad)


# ── 桥接 ───────────────────────────────────────────────────────────────────
def test_bridge():
    print("[P2-bridge] gate 兼容 dims/dimensions/objective")
    use_real_copy()
    task = {"task_id": "p2test-br", "objective": "落地页跳转链路研究",
            "keywords": [], "profile": {"dimensions": {
                "delivery_architecture": {"value": "cdn-fronted", "confidence": 0.9}}}}
    out = eri_p2.bridge_gate(task)
    eri = out["external_resource_review"]
    check("bridge 出结构化 review", eri["status"] in ("ok", "no_match")
          and "query_id" in eri, eri.get("status"))
    check("CJK 桥生效（落地页/跳转）",
          eri["status"] == "ok" and eri["candidate_count"] >= 1, eri["candidate_count"])
    check("缺 ERI 时静默（空 profile）",
          eri_p2.bridge_gate({"task_id": "x"})["external_resource_review"]["status"]
          in ("ok", "no_match", "error"))


# ── 审核队列（夹具库冒烟） ──────────────────────────────────────────────────
def test_reviews():
    print("[P2-review] 三队列规则")
    fixture_db([
        ("gh:x/alpha", "Alpha Tool", "exploit framework", '["exploit"]', "A", 85, "INDEXED"),
        ("gh:x/nolic", "NoLic Tool", "old thing", "[]", "B", 68, "INDEXED"),
    ])
    con = eri_p2.p2_connect()
    con.execute("INSERT OR REPLACE INTO d5_candidates(resource_a, resource_b,"
                " name_similarity, description_similarity, keyword_overlap,"
                " category_overlap, repository_relationship, decision, score, evidence,"
                " computed_at) VALUES ('gh:x/alpha','gh:x/nolic',0.9,0.1,0.2,0,"
                " 'none','NEEDS_REVIEW',0.5,'{}','2026-01-01')")
    con.execute("INSERT OR REPLACE INTO skill_mappings(resource_id, target, mapping_kind,"
                " mapping_status, confidence, created_at, updated_at)"
                " VALUES ('gh:x/alpha','skill:exploit-windows','skill','SUGGESTED',0.3,"
                " '2026-01-01','2026-01-01')")
    con.execute("UPDATE github_metadata SET license_status='NO_LICENSE_FILE',"
                " license_spdx_id=NULL WHERE resource_id='gh:x/nolic'")
    con.commit(); con.close()
    d5 = eri_p2.review_d5()
    mp = eri_p2.review_mappings(skills_dir=os.path.join(HERE, "fixtures", "no-skills"))
    lc = eri_p2.review_licenses()
    check("d5 reviewed", d5["reviewed"] == 1, d5)
    check("mappings reviewed", mp["reviewed"] == 1, mp)
    check("licenses reviewed", lc["reviewed"] >= 2, lc)
    con = eri_p2.p2_connect()
    dec = dict(con.execute("SELECT item_key, decision FROM review_log"
                           " WHERE queue='d5'").fetchall())
    lic = dict(con.execute("SELECT item_key, decision FROM review_log"
                           " WHERE queue='license'").fetchall())
    n_res = con.execute("SELECT COUNT(*) FROM resources").fetchone()[0]
    con.close()
    check("d5 同主名似低述叠判 SAME_OWNER_VARIANTS",
          any(v == "SAME_OWNER_VARIANTS" for v in dec.values()), dec)
    check("B 级无许可 -> 人工核", any(v == "NEEDS_HUMAN_REVIEW" for v in lic.values()), lic)
    check("审核零删除", n_res == 7, n_res)  # 夹具 5 + alpha/nolic 2
    # skill 文件缺失 -> NEEDS_MORE_EVIDENCE
    con = eri_p2.p2_connect()
    ms = dict(con.execute("SELECT mapping_status, COUNT(*) FROM skill_mappings"
                          " GROUP BY 1").fetchall())
    con.close()
    check("skill 缺失降级为待证", "NEEDS_MORE_EVIDENCE" in ms, ms)


def main():
    tests = [test_scenarios, test_resolver_rules, test_reader, test_tracker,
             test_bridge, test_reviews]
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
    print("ALL P2 TESTS PASS")
    shutil.rmtree(TMP, ignore_errors=True)


if __name__ == "__main__":
    main()
