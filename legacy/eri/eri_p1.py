#!/usr/bin/env python3
"""ERI P1：元数据补全 / 许可与维护状态 / D5 疑似重复 / 质量评分 / 隔离 / Skills 映射。

在 P0（eri.py）之上只增不改：新表见 P1_SCHEMA；resources 表仅更新既有列
（quality_score / trust_level / status），不动 P0 的解析、入库、同步逻辑。

CLI：
  python3 -I eri_p1.py enrich [--all|--pending|--failed|--ids a,b] [--transport prod]
  python3 -I eri_p1.py d5
  python3 -I eri_p1.py quality
  python3 -I eri_p1.py map-skills
  python3 -I eri_p1.py quarantine <resource_id> --reason "..." | unquarantine <id>
  python3 -I eri_p1.py query [--category C] [--type T] [--grade G] [--license L]
                             [--maintenance M] [--keyword K] [--skill S] [--dup]
                             [--review] [--limit N]
  python3 -I eri_p1.py stats
  python3 -I eri_p1.py backup | restore --file <db>
  python3 -I eri_p1.py reports            # 生成 metadata/quality/duplicate/cost 四份 md
"""
from __future__ import annotations

import argparse
import difflib
import json
import os
import re
import shutil
import sqlite3
import sys
import time
import urllib.error
import urllib.request
from datetime import datetime, timezone

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import eri  # noqa: E402

SKILLS_DIR = os.path.expanduser("~/.dsh/redteam/skills")
TOOLS_JSON = os.path.expanduser("~/.dsh/redteam/rtis/data/registry/tools.json")
REPORTS_DIR = os.path.join(HERE, "reports")            # 测试可覆盖
BACKUP_DIR = os.path.join(HERE, "db", "backups")       # 测试可覆盖
ENGINE_VERSION = "p1-quality-1"

# ─────────────────────────── P1 库表（只增不改） ───────────────────────────

P1_SCHEMA = """
CREATE TABLE IF NOT EXISTS github_metadata (
  resource_id     TEXT PRIMARY KEY REFERENCES resources(resource_id),
  repository_id   INTEGER, full_name TEXT, html_url TEXT,
  description     TEXT, homepage TEXT, visibility TEXT,
  archived        INTEGER, disabled INTEGER, fork INTEGER,
  parent_repository TEXT, default_branch TEXT,
  created_at      TEXT, updated_at TEXT, pushed_at TEXT,
  stargazers_count INTEGER, watchers_count INTEGER, forks_count INTEGER,
  open_issues_count INTEGER, language TEXT, topics TEXT,
  license_spdx_id TEXT, license_name TEXT, license_key TEXT,
  license_status  TEXT,      -- IDENTIFIED | FILE_UNIDENTIFIED | NO_LICENSE_FILE | AMBIGUOUS | UNKNOWN
  maintenance_status TEXT,   -- ACTIVE | LOW_ACTIVITY | STALE | ARCHIVED | DISABLED | UNAVAILABLE | UNKNOWN
  api_response_at TEXT, etag TEXT,
  fetch_status    TEXT,      -- ok | not_modified | gone | error
  fetch_error     TEXT,
  meta_updated_at TEXT
);

CREATE TABLE IF NOT EXISTS enrichment_state (
  resource_id TEXT PRIMARY KEY,
  last_attempt TEXT, attempts INTEGER DEFAULT 0,
  status TEXT NOT NULL DEFAULT 'pending',  -- pending | done | failed | rate_limited
  note TEXT
);

CREATE TABLE IF NOT EXISTS d5_candidates (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  resource_a TEXT NOT NULL, resource_b TEXT NOT NULL,
  name_similarity REAL, description_similarity REAL, keyword_overlap REAL,
  category_overlap REAL, repository_relationship TEXT,
  decision TEXT NOT NULL,  -- POSSIBLE_DUPLICATE | RELATED_RESOURCE | FORK | MIRROR | DISTINCT | NEEDS_REVIEW
  score REAL, evidence TEXT, computed_at TEXT,
  UNIQUE(resource_a, resource_b)
);

CREATE TABLE IF NOT EXISTS quality_scores (
  resource_id TEXT PRIMARY KEY REFERENCES resources(resource_id),
  relevance REAL, reliability REAL, maintenance REAL, maturity REAL,
  complementarity REAL, license_clarity REAL, docs_testing REAL,
  final_score REAL, grade TEXT,           -- A | B | C | D
  adoption_blocked INTEGER DEFAULT 0,     -- 硬门槛：许可证不明等
  reasons TEXT,                           -- JSON 分项理由
  computed_at TEXT, engine_version TEXT
);

CREATE TABLE IF NOT EXISTS skill_mappings (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  resource_id TEXT NOT NULL REFERENCES resources(resource_id),
  target TEXT NOT NULL,                   -- skill:<name> 或 tool:<name> 或 workflow:<id>
  mapping_kind TEXT NOT NULL DEFAULT 'skill',
  mapping_status TEXT NOT NULL DEFAULT 'SUGGESTED',  -- SUGGESTED | REVIEWED | ADOPTED | REJECTED
  confidence REAL, match_basis TEXT,      -- 命中词证据
  capability_overlap TEXT, complementary TEXT,
  evidence TEXT, created_at TEXT, updated_at TEXT,
  UNIQUE(resource_id, target)
);

CREATE TABLE IF NOT EXISTS ai_usage_log (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  task_id TEXT, resource_ids TEXT, model TEXT,
  input_tokens INTEGER, output_tokens INTEGER,
  prompt_hash TEXT, content_hash TEXT, cached INTEGER,
  result TEXT, created_at TEXT
);

CREATE TABLE IF NOT EXISTS api_call_log (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  ts TEXT NOT NULL, resource_id TEXT, url TEXT,
  status INTEGER, etag_sent TEXT, etag_recv TEXT,
  rate_remaining INTEGER, note TEXT
);
"""

PERMISSIVE = {"MIT", "Apache-2.0", "BSD-2-Clause", "BSD-3-Clause", "ISC",
              "CC0-1.0", "CC-BY-4.0", "CC-BY-SA-4.0", "Unlicense", "MPL-2.0", "Zlib"}
COPYLEFT = {"GPL-2.0", "GPL-2.0-only", "GPL-3.0", "GPL-3.0-only", "AGPL-3.0",
            "LGPL-2.1", "LGPL-3.0"}


def p1_connect() -> sqlite3.Connection:
    con = eri.connect()
    con.executescript(P1_SCHEMA)
    return con


def now_iso() -> str:
    return datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


# ─────────────────────────── 传输层（可注入，供测试） ───────────────────────────

def _token() -> str | None:
    for v in ("GITHUB_TOKEN", "GH_TOKEN", "GITHUB_API_TOKEN"):
        if os.environ.get(v):
            return os.environ[v]
    return None


def prod_transport(url: str, headers: dict) -> tuple[int, bytes, dict]:
    """GET with retries. 3xx/4xx/5xx 一律返回状态码（304/403/404/429 也走这里）。"""
    tok = _token()
    h = {"User-Agent": "eri-p1 (+RTIS external resource intelligence)",
         "Accept": "application/vnd.github+json",
         "X-GitHub-Api-Version": "2022-11-28"}
    if tok:
        h["Authorization"] = "Bearer " + tok
    h.update(headers)
    last = None
    for attempt in range(3):
        try:
            req = urllib.request.Request(url, headers=h)
            with urllib.request.urlopen(req, timeout=60) as r:
                return r.status, r.read(), dict(r.headers)
        except urllib.error.HTTPError as e:
            return e.code, e.read() or b"", dict(e.headers)
        except (urllib.error.URLError, TimeoutError, ConnectionError) as e:
            last = e
            time.sleep(2 * (attempt + 1))
    raise RuntimeError(f"transport failed after 3 attempts: {url} ({last})")


# ─────────────────────────── 分类器 ───────────────────────────

def classify_license(api_license) -> tuple[str, str | None, str | None, str | None]:
    """返回 (status, spdx_id, name, key)。null 不等于无许可，仅表示 API 未检测到。"""
    if api_license is None:
        return "NO_LICENSE_FILE", None, None, None
    spdx = api_license.get("spdx_id")
    key = api_license.get("key")
    name = api_license.get("name")
    if spdx and spdx != "NOASSERTION":
        return "IDENTIFIED", spdx, name, key
    # license 对象存在但 spdx 为 NOASSERTION 或 key=other：文件在，识别失败
    return "FILE_UNIDENTIFIED", None, name, key


def classify_maintenance(archived, disabled, pushed_at, fetch_status) -> str:
    if fetch_status == "gone":
        return "UNAVAILABLE"
    if disabled:
        return "DISABLED"
    if archived:
        return "ARCHIVED"
    if not pushed_at:
        return "UNKNOWN"
    days = (datetime.now(timezone.utc)
            - datetime.strptime(pushed_at, "%Y-%m-%dT%H:%M:%SZ").replace(tzinfo=timezone.utc)).days
    if days <= 180:
        return "ACTIVE"
    if days <= 365 * 2:
        return "LOW_ACTIVITY"
    return "STALE"


def parse_repo_payload(payload: dict) -> dict:
    lic_status, spdx, lic_name, lic_key = classify_license(payload.get("license"))
    fetch_status = "ok"
    row = {
        "repository_id": payload.get("id"),
        "full_name": payload.get("full_name"),
        "html_url": payload.get("html_url"),
        "description": payload.get("description"),
        "homepage": payload.get("homepage") or None,
        "visibility": payload.get("visibility") or ("public" if payload else None),
        "archived": 1 if payload.get("archived") else 0,
        "disabled": 1 if payload.get("disabled") else 0,
        "fork": 1 if payload.get("fork") else 0,
        "parent_repository": (payload.get("parent") or {}).get("full_name")
        if isinstance(payload.get("parent"), dict) else None,
        "default_branch": payload.get("default_branch"),
        "created_at": payload.get("created_at"),
        "updated_at": payload.get("updated_at"),
        "pushed_at": payload.get("pushed_at"),
        "stargazers_count": payload.get("stargazers_count"),
        "watchers_count": payload.get("watchers_count"),
        "forks_count": payload.get("forks_count"),
        "open_issues_count": payload.get("open_issues_count"),
        "language": payload.get("language"),
        "topics": json.dumps(payload.get("topics") or [], ensure_ascii=False),
        "license_spdx_id": spdx, "license_name": lic_name, "license_key": lic_key,
        "license_status": lic_status,
        "fetch_status": fetch_status, "fetch_error": None,
    }
    row["maintenance_status"] = classify_maintenance(
        row["archived"], row["disabled"], row["pushed_at"], fetch_status)
    return row


# ─────────────────────────── 元数据补全（断点/限流/ETag） ───────────────────────────

def github_repos(con) -> list[sqlite3.Row]:
    return con.execute(
        "SELECT r.resource_id, r.owner, r.repo_name FROM resources r"
        " WHERE r.resource_type='github_repo' ORDER BY r.resource_id").fetchall()


def enrich(mode: str = "pending", ids: list | None = None, transport=None,
           max_calls: int | None = None, backup_first: bool = True) -> dict:
    transport = transport or prod_transport
    con = p1_connect()
    try:
        if backup_first and con.execute(
                "SELECT COUNT(*) c FROM github_metadata").fetchone()["c"] == 0:
            backup()

        all_repos = {r["resource_id"]: r for r in github_repos(con)}
        states = {r["resource_id"]: r["status"] for r in con.execute(
            "SELECT resource_id, status FROM enrichment_state").fetchall()}
        if mode == "all":
            targets = list(all_repos)
        elif mode == "failed":
            targets = [i for i in all_repos if states.get(i) == "failed"]
        elif mode == "ids":
            targets = [i for i in (ids or []) if i in all_repos]
        else:  # pending
            targets = [i for i in all_repos if states.get(i) != "done"]

        counters = {"ok": 0, "not_modified": 0, "gone": 0, "error": 0}
        stopped_reason = None
        calls = 0
        for rid in targets:
            if stopped_reason:
                con.execute("INSERT INTO enrichment_state(resource_id, last_attempt,"
                            " attempts, status, note) VALUES (?,?,1,'rate_limited',?)"
                            " ON CONFLICT(resource_id) DO UPDATE SET last_attempt=excluded.last_attempt,"
                            " attempts=enrichment_state.attempts+1, status='rate_limited',"
                            " note=excluded.note", (rid, now_iso(), stopped_reason))
                continue
            if max_calls is not None and calls >= max_calls:
                stopped_reason = f"max_calls={max_calls} reached"
                con.execute("UPDATE enrichment_state SET status='pending' WHERE resource_id=?",
                            (rid,))
                continue
            r = all_repos[rid]
            url = f"https://api.github.com/repos/{r['owner']}/{r['repo_name']}"
            etag_row = con.execute("SELECT etag FROM github_metadata WHERE resource_id=?",
                                   (rid,)).fetchone()
            headers = {"If-None-Match": etag_row["etag"]} if etag_row and etag_row["etag"] else {}
            try:
                status, body, resp_headers = transport(url, headers)
            except Exception as e:  # noqa: BLE001
                con.execute("INSERT INTO enrichment_state(resource_id, last_attempt, attempts,"
                            " status, note) VALUES (?,?,1,'failed',?)"
                            " ON CONFLICT(resource_id) DO UPDATE SET last_attempt=excluded.last_attempt,"
                            " attempts=enrichment_state.attempts+1, status='failed', note=excluded.note",
                            (rid, now_iso(), repr(e)[:200]))
                counters["error"] += 1
                calls += 1
                continue
            calls += 1
            remaining = resp_headers.get("x-ratelimit-remaining") \
                or resp_headers.get("X-RateLimit-Remaining")
            con.execute("INSERT INTO api_call_log(ts, resource_id, url, status, etag_sent,"
                        " etag_recv, rate_remaining, note) VALUES (?,?,?,?,?,?,?,?)",
                        (now_iso(), rid, url, status, headers.get("If-None-Match"),
                         resp_headers.get("etag") or resp_headers.get("ETag"),
                         int(remaining) if remaining and remaining.isdigit() else None, None))

            if status == 304:
                con.execute("UPDATE github_metadata SET api_response_at=?, etag=?,"
                            " fetch_status='not_modified' WHERE resource_id=?",
                            (now_iso(), resp_headers.get("etag") or resp_headers.get("ETag"), rid))
                con.execute("INSERT INTO enrichment_state(resource_id, last_attempt, attempts,"
                            " status, note) VALUES (?,?,1,'done','etag-304')"
                            " ON CONFLICT(resource_id) DO UPDATE SET last_attempt=excluded.last_attempt,"
                            " attempts=enrichment_state.attempts+1, status='done', note='etag-304'",
                            (rid, now_iso()))
                counters["not_modified"] += 1
            elif status == 200:
                row = parse_repo_payload(json.loads(body))
                _upsert_metadata(con, rid, row, resp_headers)
                _mark_state(con, rid, "done", None)
                counters["ok"] += 1
            elif status in (404, 451, 410):
                con.execute(
                    "INSERT INTO github_metadata(resource_id, license_status,"
                    " maintenance_status, api_response_at, etag, fetch_status, fetch_error,"
                    " meta_updated_at) VALUES (?,?,?,?,?,?,?,?)"
                    " ON CONFLICT(resource_id) DO UPDATE SET license_status='UNKNOWN',"
                    " maintenance_status='UNAVAILABLE', api_response_at=excluded.api_response_at,"
                    " etag=excluded.etag, fetch_status='gone', fetch_error=excluded.fetch_error,"
                    " meta_updated_at=excluded.meta_updated_at",
                    (rid, "UNKNOWN", "UNAVAILABLE", now_iso(),
                     resp_headers.get("etag") or resp_headers.get("ETag"),
                     "gone", f"http-{status}", now_iso()))
                con.execute("UPDATE resources SET status='UNAVAILABLE', updated_at=?"
                            " WHERE resource_id=?", (now_iso(), rid))
                _mark_state(con, rid, "done", f"http-{status} unavailable")
                counters["gone"] += 1
            elif status in (403, 429):
                stopped_reason = (f"http-{status} rate-limit (remaining={remaining})")
                _mark_state(con, rid, "rate_limited", stopped_reason)
            else:
                _mark_state(con, rid, "failed", f"http-{status}")
                counters["error"] += 1

            # 响应头显示配额耗尽：停（进度已逐条落盘）
            if not stopped_reason and remaining is not None and remaining.isdigit() \
                    and int(remaining) <= 0 and status != 304:
                stopped_reason = f"x-ratelimit-remaining=0 after {rid}"

        con.commit()
        done = con.execute("SELECT COUNT(*) c FROM github_metadata WHERE fetch_status"
                           " IN ('ok','not_modified','gone')").fetchone()["c"]
        return {"targets": len(targets), "calls": calls, **counters,
                "stopped_reason": stopped_reason, "metadata_rows": done,
                "token_used": _token() is not None}
    finally:
        con.close()


def _upsert_metadata(con, rid: str, row: dict, resp_headers: dict):
    cols = list(row.keys()) + ["api_response_at", "etag", "meta_updated_at"]
    vals = [row[c] for c in row] + [now_iso(),
                                    resp_headers.get("etag") or resp_headers.get("ETag"),
                                    now_iso()]
    con.execute(
        f"INSERT INTO github_metadata(resource_id, {', '.join(cols)})"
        f" VALUES (?, {', '.join('?' * len(cols))})"
        f" ON CONFLICT(resource_id) DO UPDATE SET "
        + ", ".join(f"{c}=excluded.{c}" for c in cols), [rid, *vals])


def _mark_state(con, rid: str, status: str, note):
    con.execute("INSERT INTO enrichment_state(resource_id, last_attempt, attempts, status, note)"
                " VALUES (?,?,1,?,?) ON CONFLICT(resource_id) DO UPDATE SET"
                " last_attempt=excluded.last_attempt, attempts=enrichment_state.attempts+1,"
                " status=excluded.status, note=excluded.note",
                (rid, now_iso(), status, note))


# ─────────────────────────── D5 相似候选 ───────────────────────────

STOPWORDS = {"the", "and", "for", "with", "list", "of", "awesome", "collection",
             "resources", "resource", "tools", "tool", "a", "an", "in", "to",
             "from", "about", "your", "you", "web", "other", "using", "related",
             "security", "cyber", "curated"}


def tokens(text: str) -> set:
    return {t for t in re.findall(r"[a-z0-9+#.]{3,}", (text or "").lower())
            if t not in STOPWORDS}


def name_similarity(a: str, b: str) -> float:
    return difflib.SequenceMatcher(None, (a or "").lower(), (b or "").lower()).ratio()


def description_similarity(da: str, db: str) -> float:
    ta, tb = tokens(da), tokens(db)
    jac = (len(ta & tb) / len(ta | tb)) if ta | tb else 0.0
    seq = difflib.SequenceMatcher(None, (da or "").lower()[:200],
                                  (db or "").lower()[:200]).quick_ratio()
    return round(0.6 * jac + 0.4 * seq, 4)


def d5() -> dict:
    con = p1_connect()
    try:
        res = {r["resource_id"]: dict(r) for r in con.execute(
            "SELECT r.resource_id, r.name, r.description, r.owner, m.fork,"
            " m.parent_repository, m.full_name FROM resources r"
            " LEFT JOIN github_metadata m USING(resource_id)").fetchall()}
        cats = {}
        for row in con.execute("SELECT resource_id, category FROM resource_categories"):
            cats.setdefault(row["resource_id"], set()).add(row["category"])
        ids = sorted(res)
        recorded = distinct = 0
        for i in range(len(ids)):
            for j in range(i + 1, len(ids)):
                a, b = ids[i], ids[j]
                ra, rb = res[a], res[b]
                nsim = name_similarity(ra["name"], rb["name"])
                dsim = description_similarity(ra["description"] or "", rb["description"] or "")
                ca, cb = cats.get(a, set()), cats.get(b, set())
                cat_over = len(ca & cb)
                ta, tb = tokens((ra["name"] or "") + " " + (ra["description"] or "")), \
                         tokens((rb["name"] or "") + " " + (rb["description"] or ""))
                kw = round(len(ta & tb) / min(len(ta), len(tb)), 4) if ta and tb else 0.0

                rel = "none"
                if ra.get("fork") and ra.get("parent_repository") == rb.get("full_name"):
                    rel = "fork-of-b"
                elif rb.get("fork") and rb.get("parent_repository") == ra.get("full_name"):
                    rel = "fork-of-a"
                elif ra.get("owner") == rb.get("owner"):
                    rel = "same-owner"

                if rel.startswith("fork"):
                    decision = "FORK"
                elif nsim >= 0.85 and dsim >= 0.55:
                    decision = "POSSIBLE_DUPLICATE"
                elif nsim >= 0.90 and (not ra["description"] or not rb["description"]):
                    decision = "POSSIBLE_DUPLICATE"
                elif (ra["name"] or "").strip().lower() == (rb["name"] or "").strip().lower() \
                        and dsim < 0.30:
                    decision = "NEEDS_REVIEW"   # 同名不同项目
                elif dsim >= 0.45 and cat_over > 0:
                    decision = "RELATED_RESOURCE"
                elif nsim >= 0.60 or dsim >= 0.50:
                    decision = "NEEDS_REVIEW"
                else:
                    distinct += 1
                    continue

                evidence = {"shared_keywords": sorted(ta & tb)[:12],
                            "categories_a": sorted(ca), "categories_b": sorted(cb),
                            "note": "确定性初筛，不代表结论；不自动合并或删除"}
                con.execute(
                    "INSERT INTO d5_candidates(resource_a, resource_b, name_similarity,"
                    " description_similarity, keyword_overlap, category_overlap,"
                    " repository_relationship, decision, score, evidence, computed_at)"
                    " VALUES (?,?,?,?,?,?,?,?,?,?,?)"
                    " ON CONFLICT(resource_a, resource_b) DO UPDATE SET"
                    " name_similarity=excluded.name_similarity,"
                    " description_similarity=excluded.description_similarity,"
                    " keyword_overlap=excluded.keyword_overlap,"
                    " category_overlap=excluded.category_overlap,"
                    " repository_relationship=excluded.repository_relationship,"
                    " decision=excluded.decision, score=excluded.score,"
                    " evidence=excluded.evidence, computed_at=excluded.computed_at",
                    (a, b, round(nsim, 4), dsim, kw, cat_over, rel, decision,
                     round(nsim * 0.5 + dsim * 0.5, 4),
                     json.dumps(evidence, ensure_ascii=False), now_iso()))
                recorded += 1
        con.commit()
        return {"pairs_total": len(ids) * (len(ids) - 1) // 2, "recorded": recorded,
                "distinct_not_recorded": distinct}
    finally:
        con.close()


# ─────────────────────────── 质量引擎 ───────────────────────────

RED_TEAM_TERMS = ["pentest", "penetration", "exploit", "red team", "redteam", "ctf",
                  "bug bounty", "vulnerab", "payload", "privilege", "reversing",
                  "malware", "forensic", "threat", "osint", "fuzzing", "hacking",
                  "attack", "honeypot", "incident response", "yara", "cve"]
GAP_TERMS = {"cdn": "边缘识别", "edge": "边缘识别", "javascript": "JS bundle",
             "bundle": "JS bundle", "sourcemap": "JS bundle", "openapi": "OpenAPI",
             "swagger": "OpenAPI", "api": "API 安全", "proxy": "代理服务面"}


def _days_since(dt: str | None) -> int | None:
    if not dt:
        return None
    try:
        return (datetime.now(timezone.utc)
                - datetime.strptime(dt[:19], "%Y-%m-%dT%H:%M:%S").replace(
                    tzinfo=timezone.utc)).days
    except ValueError:
        return None


def score_resource(res: dict, meta: dict | None, mapping_conf: float | None,
                   gap_hits: list) -> dict:
    reasons = {}
    text = " ".join([str(res.get("name") or ""), str(res.get("description") or ""),
                     str((meta or {}).get("description") or ""),
                     str((meta or {}).get("topics") or "[]")]).lower()

    # 相关性 25
    hits = [t for t in RED_TEAM_TERMS if t in text]
    rel = min(25, 4 + 3 * len(hits))
    if (res.get("categories") or "") and "Awesome Repositories" in res["categories"]:
        rel = min(25, rel + 2)
    reasons["relevance"] = {"score": rel, "term_hits": hits[:6]}

    # 可靠性 20（星数只占其中 8 分，且按对数段位）
    if meta and meta.get("created_at"):
        age = _days_since(meta["created_at"]) or 0
        rel_score = 6 if age > 365 * 5 else 5 if age > 365 * 3 else 3 if age > 365 else 1
        stars = meta.get("stargazers_count") or 0
        star_score = 8 if stars >= 10000 else 7 if stars >= 3000 else 5 \
            if stars >= 1000 else 4 if stars >= 300 else 2 if stars >= 50 else 0
        extra = (2 if meta.get("homepage") else 0) + \
                (2 if meta.get("topics") not in (None, "[]") else 0)
        relia = min(20, 2 + rel_score + star_score + extra)
        reasons["reliability"] = {"score": relia, "age_days": age,
                                  "stargazers": stars,
                                  "note": "星数仅占 8/20，禁止仅凭星数定级"}
    else:
        relia = 8
        reasons["reliability"] = {"score": 8, "note": "无元数据，中性给分"}

    # 维护 15
    maint_map = {"ACTIVE": 15, "LOW_ACTIVITY": 11, "STALE": 6, "ARCHIVED": 2,
                 "DISABLED": 0, "UNAVAILABLE": 0, "UNKNOWN": 8}
    maint_status = (meta or {}).get("maintenance_status") or "UNKNOWN"
    maint = maint_map[maint_status]
    reasons["maintenance"] = {"score": maint, "status": maint_status,
                              "note": "长期稳定规范类项目可能无需频繁更新；此项只计推送新近度"}

    # 成熟度 15
    if meta and meta.get("forks_count") is not None:
        forks = meta.get("forks_count") or 0
        f = 5 if forks >= 1000 else 4 if forks >= 300 else 3 if forks >= 50 \
            else 2 if forks >= 10 else 1
        issues = meta.get("open_issues_count") or 0
        iss = 3 if issues < 50 else 2 if issues < 200 else 1
        docs = (2 if meta.get("language") else 0) + \
               (2 if (meta.get("description") or "").strip() else 0)
        maturity = min(15, f + iss + docs)
        reasons["maturity"] = {"score": maturity, "forks": forks, "open_issues": issues}
    else:
        maturity = 7
        reasons["maturity"] = {"score": 7, "note": "无元数据，中性给分"}

    # 互补性 10
    comp = 4
    if mapping_conf is not None:
        comp = 8 if mapping_conf >= 0.3 else 6
    if gap_hits:
        comp = min(10, comp + 2)
    reasons["complementarity"] = {"score": comp, "skill_mapped": mapping_conf is not None,
                                  "gap_coverage": gap_hits,
                                  "note": "gap 覆盖对照 security-research-upgrade 七主题缺口"}

    # License 清晰度 10
    lic = (meta or {}).get("license_status") or "UNKNOWN"
    spdx = (meta or {}).get("license_spdx_id")
    if lic == "IDENTIFIED" and spdx in PERMISSIVE:
        lc = 10
    elif lic == "IDENTIFIED" and spdx in COPYLEFT:
        lc = 7
    elif lic == "IDENTIFIED":
        lc = 8
    elif lic == "FILE_UNIDENTIFIED":
        lc = 4
    elif lic == "NO_LICENSE_FILE":
        lc = 3
    else:
        lc = 3
    reasons["license_clarity"] = {"score": lc, "status": lic, "spdx": spdx,
                                  "note": "API null 仅表示未检测到，非确证无许可"}

    # 文档与测试代理信号 5（不克隆仓库，只用元数据代理）
    doc = ((meta or {}).get("homepage") and 2 or 0) \
        + ((meta or {}).get("topics") not in (None, "[]") and 1 or 0) \
        + (len((meta or {}).get("description") or "") > 80 and 1 or 0) \
        + ((meta or {}).get("language") and 1 or 0)
    reasons["docs_testing"] = {"score": doc, "note": "元数据代理信号，未读源码"}

    final = round(rel + relia + maint + maturity + comp + lc + doc, 1)
    adoption_blocked = 1 if lc < 4 else 0
    if adoption_blocked:
        reasons["adoption_gate"] = "许可证状态不明（非禁止收录，禁止自动采用）"
    if maint_status == "DISABLED":
        reasons["adoption_gate"] = "仓库 disabled，转隔离审核"

    grade = "A" if final >= 80 else "B" if final >= 65 else "C" if final >= 45 else "D"
    if lc < 4 and grade == "A":
        grade = "B"   # 许可不清晰时封顶 B（仍可人工采用）
    return {"relevance": rel, "reliability": relia, "maintenance": maint,
            "maturity": maturity, "complementarity": comp, "license_clarity": lc,
            "docs_testing": doc, "final_score": final, "grade": grade,
            "adoption_blocked": adoption_blocked, "reasons": reasons}


def quality() -> dict:
    con = p1_connect()
    try:
        mapping_conf = {}
        for row in con.execute(
                "SELECT resource_id, MAX(confidence) c FROM skill_mappings"
                " WHERE mapping_kind='skill' AND mapping_status IN ('SUGGESTED','REVIEWED','ADOPTED')"
                " GROUP BY resource_id"):
            mapping_conf[row["resource_id"]] = row["c"]
        n = 0
        for res_row in con.execute(
                "SELECT r.resource_id, r.name, r.description,"
                " GROUP_CONCAT(DISTINCT c.category) categories FROM resources r"
                " LEFT JOIN resource_categories c USING(resource_id) GROUP BY r.resource_id"):
            res = dict(res_row)
            meta_row = con.execute("SELECT * FROM github_metadata WHERE resource_id=?",
                                   (res["resource_id"],)).fetchone()
            meta = dict(meta_row) if meta_row else None
            text = ((res["name"] or "") + " " + (res["description"] or "")).lower()
            gap_hits = sorted({v for k, v in GAP_TERMS.items() if k in text})
            s = score_resource(res, meta, mapping_conf.get(res["resource_id"]), gap_hits)
            con.execute(
                "INSERT INTO quality_scores(resource_id, relevance, reliability, maintenance,"
                " maturity, complementarity, license_clarity, docs_testing, final_score,"
                " grade, adoption_blocked, reasons, computed_at, engine_version)"
                " VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?)"
                " ON CONFLICT(resource_id) DO UPDATE SET relevance=excluded.relevance,"
                " reliability=excluded.reliability, maintenance=excluded.maintenance,"
                " maturity=excluded.maturity, complementarity=excluded.complementarity,"
                " license_clarity=excluded.license_clarity, docs_testing=excluded.docs_testing,"
                " final_score=excluded.final_score, grade=excluded.grade,"
                " adoption_blocked=excluded.adoption_blocked, reasons=excluded.reasons,"
                " computed_at=excluded.computed_at, engine_version=excluded.engine_version",
                (res["resource_id"], s["relevance"], s["reliability"], s["maintenance"],
                 s["maturity"], s["complementarity"], s["license_clarity"],
                 s["docs_testing"], s["final_score"], s["grade"], s["adoption_blocked"],
                 json.dumps(s["reasons"], ensure_ascii=False), now_iso(), ENGINE_VERSION))
            # disabled 仓库自动隔离
            if meta and meta["disabled"]:
                con.execute("UPDATE resources SET status='QUARANTINED', updated_at=?"
                            " WHERE resource_id=?", (now_iso(), res["resource_id"]))
            else:
                con.execute("UPDATE resources SET quality_score=?, trust_level=?, updated_at=?"
                            " WHERE resource_id=? AND status NOT IN ('QUARANTINED','UNAVAILABLE')",
                            (s["final_score"], s["grade"], now_iso(), res["resource_id"]))
            n += 1
        con.commit()
        return {"scored": n}
    finally:
        con.close()


# ─────────────────────────── 隔离 ───────────────────────────

def quarantine(rid: str, reason: str, undo: bool = False) -> str:
    con = p1_connect()
    try:
        row = con.execute("SELECT status FROM resources WHERE resource_id=?", (rid,)).fetchone()
        if not row:
            return f"not found: {rid}"
        if undo:
            con.execute("UPDATE resources SET status='INDEXED', updated_at=? WHERE resource_id=?",
                        (now_iso(), rid))
            con.execute("INSERT INTO resource_reviews(resource_id, review_status, decision,"
                        " decision_reason, reviewer, reviewed_at) VALUES (?,?,?,?,?,?)"
                        " ON CONFLICT(resource_id) DO UPDATE SET review_status='UNQUARANTINED',"
                        " decision='release', decision_reason=excluded.decision_reason,"
                        " reviewer=excluded.reviewer, reviewed_at=excluded.reviewed_at",
                        (rid, "UNQUARANTINED", "release", reason, "eri-p1", now_iso()))
            con.commit()
            return f"released {rid} -> INDEXED"
        con.execute("UPDATE resources SET status='QUARANTINED', updated_at=? WHERE resource_id=?",
                    (now_iso(), rid))
        con.execute("INSERT INTO resource_reviews(resource_id, review_status, decision,"
                    " decision_reason, reviewer, reviewed_at) VALUES (?,?,?,?,?,?)"
                    " ON CONFLICT(resource_id) DO UPDATE SET review_status='QUARANTINED',"
                    " decision='quarantine', decision_reason=excluded.decision_reason,"
                    " reviewer=excluded.reviewer, reviewed_at=excluded.reviewed_at",
                    (rid, "QUARANTINED", "quarantine", reason, "eri-p1", now_iso()))
        con.commit()
        return f"quarantined {rid}: {reason}"
    finally:
        con.close()


# ─────────────────────────── Skills / Tools 映射 ───────────────────────────

def _front_matter(text: str) -> dict:
    fm = {}
    if text.startswith("---"):
        end = text.find("\n---", 3)
        if end > 0:
            for line in text[3:end].strip().splitlines():
                if ":" in line:
                    k, _, v = line.partition(":")
                    fm[k.strip()] = v.strip()
    return fm


def load_skills(skills_dir: str = SKILLS_DIR) -> list[dict]:
    skills = []
    if not os.path.isdir(skills_dir):
        return skills
    for fn in sorted(os.listdir(skills_dir)):
        if not fn.endswith(".md"):
            continue
        path = os.path.join(skills_dir, fn)
        try:
            fm = _front_matter(open(path, encoding="utf-8").read())
        except OSError:
            continue
        name = fm.get("name") or fn[:-3]
        blob = " ".join([name, fm.get("description", ""), fm.get("whenToUse", ""),
                         fn[:-3].replace("-", " ")])
        skills.append({"name": name, "role": fm.get("role", ""), "tokens": tokens(blob),
                       "raw_len": len(blob)})
    return skills


def load_tools(path: str = TOOLS_JSON) -> list[str]:
    try:
        d = json.load(open(path, encoding="utf-8"))
        items = d.get("items", d if isinstance(d, list) else [])
        names = []
        for it in items:
            n = it.get("name") or it.get("id") or it.get("tool")
            if n:
                names.append(str(n))
        return names
    except (OSError, ValueError):
        return []


def map_skills(skills_dir: str = SKILLS_DIR, tools_path: str = TOOLS_JSON) -> dict:
    con = p1_connect()
    try:
        skills = load_skills(skills_dir)
        tools = load_tools(tools_path)
        made = 0
        for res in con.execute(
                "SELECT r.resource_id, r.name, r.description, r.repo_name,"
                " m.topics, m.language FROM resources r"
                " LEFT JOIN github_metadata m USING(resource_id)"):
            rtokens = tokens(" ".join([res["name"] or "", res["description"] or "",
                                       res["repo_name"] or "",
                                       (res["topics"] or "[]").strip("[]") if res["topics"] else "",
                                       res["language"] or ""]))
            best_conf, best_skill = 0.0, None
            for sk in skills:
                inter = rtokens & sk["tokens"]
                if not inter:
                    continue
                cov = len(inter) / min(len(rtokens), len(sk["tokens"]))
                jac = len(inter) / len(rtokens | sk["tokens"])
                conf = round(0.5 * cov + 0.5 * jac, 4)
                if conf > best_conf:
                    best_conf, best_skill = conf, (sk, inter)
            if best_skill and best_conf >= 0.18:
                sk, inter = best_skill
                con.execute(
                    "INSERT INTO skill_mappings(resource_id, target, mapping_kind,"
                    " mapping_status, confidence, match_basis, evidence, created_at, updated_at)"
                    " VALUES (?,?,?,'SUGGESTED',?,?,?,(SELECT datetime('now')),?)"
                    " ON CONFLICT(resource_id, target) DO UPDATE SET"
                    " confidence=excluded.confidence, match_basis=excluded.match_basis,"
                    " evidence=excluded.evidence, updated_at=excluded.updated_at",
                    (res["resource_id"], f"skill:{sk['name']}", "skill", best_conf,
                     json.dumps(sorted(inter)[:10], ensure_ascii=False),
                     json.dumps({"shared_tokens": sorted(inter)[:10],
                                 "role": sk["role"]}, ensure_ascii=False), now_iso()))
                made += 1
            # 工具名命中：全名相等或连字符分段精确相等（避免 node 误中 nodejs-security）
            for t in tools:
                tl = t.lower().replace("_", "-")
                rn = (res["repo_name"] or "").lower()
                segs = set(rn.split("-")) | {rn}
                if tl and rn and tl in segs:
                    con.execute(
                        "INSERT INTO skill_mappings(resource_id, target, mapping_kind,"
                        " mapping_status, confidence, match_basis, evidence, created_at, updated_at)"
                        " VALUES (?,?,?,'SUGGESTED',0.8,'repo-name-exact',"
                        "'仓库名与本地工具名一致',?,?)"
                        " ON CONFLICT(resource_id, target) DO UPDATE SET"
                        " confidence=excluded.confidence, updated_at=excluded.updated_at",
                        (res["resource_id"], f"tool:{t}", "tool", now_iso(), now_iso()))
                    made += 1
        con.commit()
        return {"skills_loaded": len(skills), "tools_loaded": len(tools),
                "resources_with_mapping": made}
    finally:
        con.close()


# ─────────────────────────── 查询 / 统计 ───────────────────────────

def query(**kw) -> list[dict]:
    con = p1_connect()
    try:
        where, params = ["1=1"], []
        if kw.get("category"):
            where.append("r.resource_id IN (SELECT resource_id FROM resource_categories"
                         " WHERE category LIKE ?)"); params.append(f"%{kw['category']}%")
        if kw.get("type"):
            where.append("r.resource_type=?"); params.append(kw["type"])
        if kw.get("grade"):
            where.append("q.grade=?"); params.append(kw["grade"])
        if kw.get("license"):
            where.append("m.license_status=?"); params.append(kw["license"])
        if kw.get("maintenance"):
            where.append("m.maintenance_status=?"); params.append(kw["maintenance"])
        if kw.get("keyword"):
            where.append("(r.name LIKE ? OR r.description LIKE ?"
                         " OR m.description LIKE ?)")
            params += [f"%{kw['keyword']}%"] * 3
        if kw.get("skill"):
            where.append("r.resource_id IN (SELECT resource_id FROM skill_mappings"
                         " WHERE target LIKE ?)"); params.append(f"%{kw['skill']}%")
        if kw.get("dup"):
            where.append("r.resource_id IN (SELECT resource_a FROM d5_candidates"
                         " WHERE decision IN ('POSSIBLE_DUPLICATE','NEEDS_REVIEW','FORK','MIRROR')"
                         " UNION SELECT resource_b FROM d5_candidates"
                         " WHERE decision IN ('POSSIBLE_DUPLICATE','NEEDS_REVIEW','FORK','MIRROR'))")
        if kw.get("review"):
            where.append("(q.adoption_blocked=1 OR r.status='QUARANTINED')")
        sql = ("SELECT r.resource_id, r.name, r.resource_type, r.status, r.canonical_url,"
               " q.grade, q.final_score, m.license_status, m.maintenance_status,"
               " m.stargazers_count, m.pushed_at,"
               " (SELECT GROUP_CONCAT(DISTINCT sm.target) FROM skill_mappings sm"
               "  WHERE sm.resource_id=r.resource_id) mappings,"
               " (SELECT GROUP_CONCAT(DISTINCT dc.decision) FROM d5_candidates dc"
               "  WHERE dc.resource_a=r.resource_id OR dc.resource_b=r.resource_id) dup_decisions"
               " FROM resources r LEFT JOIN github_metadata m USING(resource_id)"
               " LEFT JOIN quality_scores q USING(resource_id)"
               f" WHERE {' AND '.join(where)} ORDER BY q.final_score DESC LIMIT ?")
        params.append(kw.get("limit", 20))
        return [dict(x) for x in con.execute(sql, params).fetchall()]
    finally:
        con.close()


def stats() -> dict:
    con = p1_connect()
    try:
        g = lambda sql: [dict(x) for x in con.execute(sql).fetchall()]  # noqa: E731
        repos = con.execute("SELECT COUNT(*) FROM resources WHERE"
                            " resource_type='github_repo'").fetchone()[0]
        meta_ok = con.execute("SELECT COUNT(*) FROM github_metadata WHERE"
                              " fetch_status IN ('ok','not_modified')").fetchone()[0]
        gone = con.execute("SELECT COUNT(*) FROM github_metadata WHERE"
                           " fetch_status='gone'").fetchone()[0]
        return {
            "resources_total": con.execute("SELECT COUNT(*) FROM resources").fetchone()[0],
            "github_repos": repos, "web_resources": con.execute(
                "SELECT COUNT(*) FROM resources WHERE resource_type!='github_repo'").fetchone()[0],
            "metadata": {"enriched_ok": meta_ok, "gone": gone,
                         "not_enriched": repos - meta_ok - gone,
                         "completion_pct": round(100.0 * (meta_ok + gone) / max(repos, 1), 1)},
            "license_status": g("SELECT license_status, COUNT(*) n FROM github_metadata"
                                " GROUP BY 1 ORDER BY n DESC"),
            "maintenance": g("SELECT maintenance_status, COUNT(*) n FROM github_metadata"
                             " GROUP BY 1 ORDER BY n DESC"),
            "grades": g("SELECT grade, COUNT(*) n FROM quality_scores GROUP BY 1"
                        " ORDER BY n DESC"),
            "status": g("SELECT status, COUNT(*) n FROM resources GROUP BY 1"),
            "d5": g("SELECT decision, COUNT(*) n FROM d5_candidates GROUP BY 1"
                    " ORDER BY n DESC"),
            "quarantined": con.execute("SELECT COUNT(*) FROM resources WHERE"
                                       " status='QUARANTINED'").fetchone()[0],
            "skill_mappings": {"resources_mapped": con.execute(
                "SELECT COUNT(DISTINCT resource_id) FROM skill_mappings"
                " WHERE mapping_kind='skill'").fetchone()[0],
                "total": con.execute("SELECT COUNT(*) FROM skill_mappings").fetchone()[0]},
            "api_calls": g("SELECT status, COUNT(*) n FROM api_call_log GROUP BY 1"),
            "ai_calls": con.execute("SELECT COUNT(*) FROM ai_usage_log").fetchone()[0],
        }
    finally:
        con.close()


# ─────────────────────────── 备份 / 恢复 ───────────────────────────

def backup() -> str:
    os.makedirs(BACKUP_DIR, exist_ok=True)
    ts = datetime.now().strftime("%Y%m%d-%H%M%S")
    dest = os.path.join(BACKUP_DIR, f"catalog-{ts}.db")
    src = sqlite3.connect(eri.DB_PATH)
    dst = sqlite3.connect(dest)
    with dst:
        src.backup(dst)
    src.close()
    dst.close()
    return dest


def restore(file: str, confirm: bool = False) -> str:
    if not os.path.exists(file):
        return f"backup not found: {file}"
    if not confirm:
        return "refusing to overwrite without --confirm"
    src = sqlite3.connect(file)
    dst = sqlite3.connect(eri.DB_PATH)
    with dst:
        src.backup(dst)
    src.close()
    dst.close()
    return f"restored {file} -> {eri.DB_PATH}"


# ─────────────────────────── 报告 ───────────────────────────

def _md_table(headers: list, rows: list) -> list[str]:
    out = ["| " + " | ".join(headers) + " |", "|" + "---|" * len(headers)]
    for r in rows:
        out.append("| " + " | ".join(str(x) if x is not None else "" for x in r) + " |")
    return out


def generate_reports() -> dict:
    os.makedirs(REPORTS_DIR, exist_ok=True)
    con = p1_connect()
    written = {}
    try:
        st = stats()
        # 1) metadata
        lines = ["# ERI P1 元数据补全报告", "", f"生成：{now_iso()}", "",
                 f"- GitHub 仓库 {st['github_repos']} 个，已补全 {st['metadata']['enriched_ok']}"
                 f"（{st['metadata']['completion_pct']}%），不可达 {st['metadata']['gone']}，"
                 f"未补全 {st['metadata']['not_enriched']}",
                 f"- 网页资源 {st['web_resources']} 个（未按仓库处理）", "",
                 "## License 状态分布", *_md_table(["状态", "数量"], st["license_status"]),
                 "", "## 维护状态分布", *_md_table(["状态", "数量"], st["maintenance"]), "",
                 "## 明细（按星数降序，前 40）", *_md_table(
                     ["资源", "星", "License", "维护", "最后 push"],
                     [(r["resource_id"], r["stargazers_count"], r["license_status"],
                       r["maintenance_status"], (r["pushed_at"] or "")[:10])
                      for r in con.execute(
                          "SELECT resource_id, stargazers_count, license_status,"
                          " maintenance_status, pushed_at FROM github_metadata"
                          " WHERE fetch_status IN ('ok','not_modified')"
                          " ORDER BY stargazers_count DESC LIMIT 40")])]
        path = os.path.join(REPORTS_DIR, "p1-metadata-report.md")
        open(path, "w", encoding="utf-8").write("\n".join(lines) + "\n")
        written["metadata"] = path

        # 2) quality
        lines = ["# ERI P1 质量评分报告", "", f"生成：{now_iso()}　引擎：{ENGINE_VERSION}", "",
                 "## 等级分布", *_md_table(["等级", "数量"], st["grades"]), "",
                 "## 资源状态", *_md_table(["状态", "数量"], st["status"]), "",
                 "## 分级明细", *_md_table(
                     ["资源", "总分", "等级", "维护", "License", "采用门槛"],
                     [(r["resource_id"], r["final_score"], r["grade"],
                       r["maintenance_status"], r["license_status"],
                       "blocked" if r["adoption_blocked"] else "")
                      for r in con.execute(
                          "SELECT q.resource_id, q.final_score, q.grade,"
                          " m.maintenance_status, m.license_status, q.adoption_blocked"
                          " FROM quality_scores q LEFT JOIN github_metadata m"
                          " USING(resource_id) ORDER BY q.final_score DESC")])]
        path = os.path.join(REPORTS_DIR, "p1-quality-report.md")
        open(path, "w", encoding="utf-8").write("\n".join(lines) + "\n")
        written["quality"] = path

        # 3) duplicate
        lines = ["# ERI P1 D5 疑似重复报告", "", f"生成：{now_iso()}", "",
                 "D5 只建立疑似关系，不自动合并或删除；决策待人工审核。", "",
                 *_md_table(
                     ["决策", "数量"], st["d5"]), "",
                 "## 候选明细", *_md_table(
                     ["A", "B", "名似", "述似", "关系", "决策"],
                     [(r["resource_a"], r["resource_b"], r["name_similarity"],
                       r["description_similarity"], r["repository_relationship"],
                       r["decision"])
                      for r in con.execute(
                          "SELECT * FROM d5_candidates ORDER BY score DESC LIMIT 60")])]
        path = os.path.join(REPORTS_DIR, "p1-duplicate-report.md")
        open(path, "w", encoding="utf-8").write("\n".join(lines) + "\n")
        written["duplicate"] = path

        # 4) cost
        calls = con.execute("SELECT COUNT(*) c, SUM(CASE WHEN status=304 THEN 1 ELSE 0 END)"
                            " nm FROM api_call_log").fetchone()
        lines = ["# ERI P1 成本报告", "", f"生成：{now_iso()}", "",
                 f"- GitHub API 调用：{calls['c']} 次（其中 304 缓存命中 {calls['nm'] or 0} 次）",
                 f"- AI/LLM 调用：{st['ai_calls']} 次，token 消耗 0（本阶段全部确定性代码）",
                 f"- Token 使用：{_token() is not None and '有（仅请求头，未落盘）' or '无，未认证限额'}",
                 "", "## API 调用分布", *_md_table(["状态", "次数"], st["api_calls"])]
        path = os.path.join(REPORTS_DIR, "p1-cost-report.md")
        open(path, "w", encoding="utf-8").write("\n".join(lines) + "\n")
        written["cost"] = path
        return written
    finally:
        con.close()


# ─────────────────────────── CLI ───────────────────────────

def main(argv=None):
    ap = argparse.ArgumentParser(description="ERI P1")
    sub = ap.add_subparsers(dest="cmd", required=True)

    p = sub.add_parser("enrich"); p.add_argument("--all", action="store_true")
    p.add_argument("--pending", action="store_true"); p.add_argument("--failed", action="store_true")
    p.add_argument("--ids"); p.add_argument("--max-calls", type=int)

    sub.add_parser("d5"); sub.add_parser("quality"); sub.add_parser("map-skills")
    p = sub.add_parser("quarantine"); p.add_argument("resource_id"); p.add_argument("--reason", required=True)
    p = sub.add_parser("unquarantine"); p.add_argument("resource_id")

    p = sub.add_parser("query")
    for f in ("category", "type", "grade", "license", "maintenance", "keyword", "skill"):
        p.add_argument("--" + f)
    p.add_argument("--dup", action="store_true"); p.add_argument("--review", action="store_true")
    p.add_argument("--limit", type=int, default=20)

    sub.add_parser("stats")
    sub.add_parser("backup")
    p = sub.add_parser("restore"); p.add_argument("--file", required=True)
    p.add_argument("--confirm", action="store_true")
    sub.add_parser("reports")

    args = ap.parse_args(argv)
    if args.cmd == "enrich":
        mode = "all" if args.all else "failed" if args.failed else "pending"
        ids = args.ids.split(",") if args.ids else None
        print(json.dumps(enrich(mode, ids, max_calls=args.max_calls),
                         ensure_ascii=False, indent=2))
    elif args.cmd == "d5":
        print(json.dumps(d5(), ensure_ascii=False, indent=2))
    elif args.cmd == "quality":
        print(json.dumps(quality(), ensure_ascii=False, indent=2))
    elif args.cmd == "map-skills":
        print(json.dumps(map_skills(), ensure_ascii=False, indent=2))
    elif args.cmd == "quarantine":
        print(quarantine(args.resource_id, args.reason))
    elif args.cmd == "unquarantine":
        print(quarantine(args.resource_id, "", undo=True))
    elif args.cmd == "query":
        for r in query(**vars(args)):
            print(f"{r['resource_id']}  [{r['grade'] or '-'}/{r['status']}]"
                  f" lic={r['license_status'] or '-'} maint={r['maintenance_status'] or '-'}"
                  f" {r['name']}")
            if r["mappings"]:
                print(f"    maps: {r['mappings']}")
            if r["dup_decisions"]:
                print(f"    d5: {r['dup_decisions']}")
    elif args.cmd == "stats":
        print(json.dumps(stats(), ensure_ascii=False, indent=2))
    elif args.cmd == "backup":
        print(backup())
    elif args.cmd == "restore":
        print(restore(args.file, args.confirm))
    elif args.cmd == "reports":
        print(json.dumps(generate_reports(), ensure_ascii=False, indent=2))


if __name__ == "__main__":
    main()
