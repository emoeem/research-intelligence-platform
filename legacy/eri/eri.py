#!/usr/bin/env python3
"""ERI —— External Resource Intelligence（RTIS 外部资源子系统，P0）

职责：把 Awesome-Hacking 这类上游资源列表解析为结构化资源目录（sidecar SQLite），
供 RTIS Orchestrator 后续检索；不触碰 ~/.dsh/redteam/knowledge.db。

设计约束（源自 ERI 实施计划）：
  - 外部资源与内部知识分离：本目录独立建库，不导入知识库。
  - 确定性工作（解析/规范化/去重/哈希/入库）全部普通代码，无 LLM 参与。
  - 所有资源带来源（source_id/section/commit/position），无来源不入库。
  - 增量：上游 commit 与快照哈希均未变化时，不重解析、不重写库。
  - 软删除：资源从上游消失只标记 UNLISTED，不删除记录。

用法：
  python3 eri.py parse   <readme.md> [--json]        # 只解析不落库（干跑）
  python3 eri.py ingest  --readme <path> --meta <repo-meta.json> \
                         --commit <sha> [--source-id awesome-hacking] [--force]
  python3 eri.py sync    [--force]                    # 走 GitHub API 增量同步
  python3 eri.py search  <关键词...> [--category X] [--limit N]
  python3 eri.py get     <resource_id>
  python3 eri.py stats
  python3 eri.py report                              # 输出 Markdown 资源清单
  python3 eri.py dedup-report                        # 目录内疑似重复候选

数据库位置：$ERI_DB 或 <本文件所在目录>/db/catalog.db
上游快照：  <本文件所在目录>/data/upstream/<source-id>/<短sha>/README.md
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import sqlite3
import sys
import tarfile
import tempfile
import time
import urllib.error
import urllib.parse
import urllib.request
from datetime import datetime, timezone
from io import BytesIO

ERI_ROOT = os.path.dirname(os.path.abspath(__file__))
DB_PATH = os.environ.get("ERI_DB") or os.path.join(ERI_ROOT, "db", "catalog.db")
UPSTREAM_DIR = os.path.join(ERI_ROOT, "data", "upstream")

DEFAULT_SOURCE_ID = "awesome-hacking"
DEFAULT_SOURCE_REPO = "Hack-with-Github/Awesome-Hacking"
SOURCE_LICENSE = "CC0-1.0"  # 上游“列表”许可证；子项目许可证各自独立核查（P1）

# 检索预算（计划 §9.2：首轮 5 个候选，需要时扩到 10，全部可配）
SEARCH_DEFAULT_LIMIT = 5
SEARCH_MAX_LIMIT = 20

# 归一化时剔除的追踪参数（保守清单：只删公认的广告/归因参数）
TRACKING_PARAMS = {
    "fbclid", "gclid", "yclid", "twclid", "msclkid", "igshid",
    "mc_cid", "mc_eid", "_hsenc", "_hsmi", "vero_id",
    "ref", "ref_src", "ref_url", "spm", "from",
    "utm_source", "utm_medium", "utm_campaign", "utm_term", "utm_content",
    "utm_id", "utm_source_platform", "utm_creative_format",
}
EXCLUDED_HOSTS = {"twitter.com", "x.com", "www.facebook.com", "facebook.com",
                  "shields.io", "img.shields.io", "badgen.net"}

ROW_PIPE = re.compile(r"^\s*\|?\s*\[([^\]]+)\]\(([^)\s]+)\)\s*\|\s*(.+?)\s*\|?\s*$")
ROW_BULLET = re.compile(r"^\s*[-*]\s+\[([^\]]+)\]\(([^)\s]+)\)\s*(?:[—–-]+|\|)\s*(.+?)\s*$")
HEADING = re.compile(r"^(#{2,3})\s+(.+?)\s*$")
GITHUB_HOSTS = {"github.com", "www.github.com"}


def now_iso() -> str:
    return datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


def sha256_hex(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


# ─────────────────────────── URL 规范化（D2/D3） ───────────────────────────

def normalize_url(raw: str):
    """返回 (canonical_url, gh_identity|None, skip_reason|None)。

    canonical：小写主机、去默认端口、去追踪参数、去锚点、去 .git、去尾斜杠、
    GitHub 收敛到 owner/repo 两段。原始 URL（含锚点）保留在来源表 source_url。
    """
    s = raw.strip().strip("<>").strip()
    if not s:
        return None, None, "empty"
    if s.startswith("//"):
        s = "https:" + s
    p = urllib.parse.urlsplit(s)
    if p.scheme not in ("http", "https"):
        return None, None, f"scheme:{p.scheme or 'none'}"
    if not p.hostname:
        return None, None, "no-host"
    host = p.hostname.lower()
    if host in EXCLUDED_HOSTS:
        return None, None, f"excluded-host:{host}"
    port = p.port
    netloc = host
    if port and not ((p.scheme == "http" and port == 80) or (p.scheme == "https" and port == 443)):
        netloc = f"{host}:{port}"
    path = re.sub(r"/{2,}", "/", p.path)

    gh = None
    if host in GITHUB_HOSTS:
        host = "github.com"
        path = path.rstrip("/")
        segs = [seg for seg in path.split("/") if seg]
        if segs and segs[-1].endswith(".git"):
            segs[-1] = segs[-1][: -len(".git")]
        if len(segs) >= 2:
            owner, repo = segs[0], segs[1]
            gh = (owner, repo)
            path = f"/{owner}/{repo}"
        else:
            path = "/" + "/".join(segs) if segs else ""
        canonical = urllib.parse.urlunsplit(("https", host, path, "", ""))
        return canonical, gh, None

    if path == "/":
        path = ""
    elif path.endswith("/"):
        path = path[:-1]
    pairs = urllib.parse.parse_qsl(p.query, keep_blank_values=True)
    kept = [(k, v) for k, v in pairs if k.lower() not in TRACKING_PARAMS]
    query = urllib.parse.urlencode(kept)
    canonical = urllib.parse.urlunsplit((p.scheme, netloc, path, query, ""))
    return canonical, None, None


def resource_type_for(host_path_canonical: str, gh) -> str:
    if gh:
        return "github_repo"
    host = urllib.parse.urlsplit(host_path_canonical).hostname or ""
    if host.endswith(".github.io"):
        return "github_page"
    return "website"


def resource_id_for(gh, canonical: str) -> str:
    """资源唯一 ID：GitHub 用仓库身份（大小写不敏感），其余用规范 URL 哈希。"""
    if gh:
        return f"gh:{gh[0].lower()}/{gh[1].lower()}"
    return "url:" + sha256_hex(canonical.encode("utf-8"))[:16]


# ─────────────────────────── Markdown 解析（第 4.2 节） ───────────────────────────

def parse_readme(text: str, source_meta: dict) -> dict:
    """从 README 提取资源记录。分类来自文档真实章节结构，不依赖手写清单。"""
    records, skipped = [], []
    section, sec_index = None, 0
    for lineno, line in enumerate(text.splitlines(), start=1):
        m = HEADING.match(line)
        if m:
            section, sec_index = m.group(2).strip(), 0
            continue
        m = ROW_PIPE.match(line) or ROW_BULLET.match(line)
        if not m:
            continue
        sec_index += 1
        name, url_raw, desc = m.group(1).strip(), m.group(2).strip(), m.group(3).strip()
        desc = re.sub(r"\s+", " ", desc)
        canonical, gh, reason = normalize_url(url_raw)
        if reason or not canonical:
            skipped.append({"line": lineno, "section": section, "name": name,
                            "url": url_raw, "reason": reason or "normalize-failed"})
            continue
        if gh and f"{gh[0].lower()}/{gh[1].lower()}" == DEFAULT_SOURCE_REPO.lower():
            skipped.append({"line": lineno, "section": section, "name": name,
                            "url": url_raw, "reason": "self-repo"})
            continue
        records.append({
            "source_id": source_meta["source_id"],
            "source_repository": source_meta["source_repository"],
            "source_commit": source_meta["source_commit"],
            "source_file": source_meta.get("source_file", "README.md"),
            "source_section": section or "(root)",
            "source_position": lineno,
            "resource_name": name,
            "resource_description": desc,
            "resource_url_raw": url_raw,
            "resource_url": canonical,
            "identity": gh,
            "resource_type": resource_type_for(canonical, gh),
            "category": section or "(root)",
        })
    return {"records": records, "skipped": skipped}


# ─────────────────────────── 批内去重（D1–D4） ───────────────────────────

def dedup_batch(records: list) -> dict:
    """同一身份的多条记录合并为一个资源，保留全部来源/分类。D5 相似去重属 P1。"""
    merged, order = {}, []
    for rec in records:
        rid = resource_id_for(rec["identity"], rec["resource_url"])
        if rid not in merged:
            merged[rid] = {"resource_id": rid, "primary": rec, "occurrences": [rec]}
            order.append(rid)
        else:
            merged[rid]["occurrences"].append(rec)
    cross_section = sum(1 for rid in order
                        if len({o["source_section"] for o in merged[rid]["occurrences"]}) > 1)
    name_variants = {rid: sorted({o["resource_name"] for o in m["occurrences"]})
                     for rid, m in merged.items()
                     if len({o["resource_name"] for o in m["occurrences"]}) > 1}
    return {"merged": merged, "order": order, "cross_section": cross_section,
            "name_variants": name_variants}


# ─────────────────────────── 数据库层 ───────────────────────────

SCHEMA = """
PRAGMA journal_mode=WAL;
PRAGMA foreign_keys=ON;

CREATE TABLE IF NOT EXISTS resources (
  resource_id     TEXT PRIMARY KEY,
  identity        TEXT UNIQUE,
  canonical_url   TEXT NOT NULL UNIQUE,
  resource_type   TEXT NOT NULL,
  name            TEXT NOT NULL,
  description     TEXT,
  owner           TEXT,
  repo_name       TEXT,
  status          TEXT NOT NULL DEFAULT 'INDEXED',
  first_seen      TEXT NOT NULL,
  last_seen       TEXT NOT NULL,
  last_checked    TEXT,
  last_source_commit TEXT,
  quality_score   REAL,            -- P1：ResourceQualityEngine
  trust_level     TEXT,            -- P1：A/B/C/D/Quarantined/Rejected
  created_at      TEXT NOT NULL,
  updated_at      TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS resource_sources (
  id               INTEGER PRIMARY KEY AUTOINCREMENT,
  resource_id      TEXT NOT NULL REFERENCES resources(resource_id),
  source_id        TEXT NOT NULL,
  source_repository TEXT NOT NULL,
  source_file      TEXT NOT NULL,
  source_section   TEXT NOT NULL,
  source_position  INTEGER,
  source_commit    TEXT NOT NULL,
  source_url       TEXT NOT NULL,           -- 原始 URL（含锚点/参数）
  first_seen       TEXT NOT NULL,
  last_seen        TEXT NOT NULL,
  UNIQUE(resource_id, source_section, source_url)
);
CREATE INDEX IF NOT EXISTS ix_sources_rid ON resource_sources(resource_id);
CREATE INDEX IF NOT EXISTS ix_sources_sid ON resource_sources(source_id, source_commit);

CREATE TABLE IF NOT EXISTS resource_categories (
  resource_id     TEXT NOT NULL REFERENCES resources(resource_id),
  category        TEXT NOT NULL,
  category_source TEXT NOT NULL,
  confidence      REAL NOT NULL DEFAULT 1.0,
  PRIMARY KEY (resource_id, category, category_source)
);

CREATE TABLE IF NOT EXISTS resource_versions (    -- P1 起逐资源版本；P0 记同步级版本
  id            INTEGER PRIMARY KEY AUTOINCREMENT,
  source_id     TEXT NOT NULL,
  commit_sha    TEXT NOT NULL,
  content_hash  TEXT NOT NULL,
  fetched_at    TEXT NOT NULL,
  resources_added   INTEGER,
  resources_updated INTEGER,
  resources_removed INTEGER,
  UNIQUE(source_id, commit_sha)
);

CREATE TABLE IF NOT EXISTS resource_relationships (  -- P2：与 Skills/Workflow/知识关联
  source_resource_id TEXT NOT NULL,
  target_entity_id   TEXT NOT NULL,
  relationship_type  TEXT NOT NULL,
  confidence         REAL DEFAULT 0.5,
  evidence           TEXT,
  created_at         TEXT,
  UNIQUE(source_resource_id, target_entity_id, relationship_type)
);

CREATE TABLE IF NOT EXISTS resource_reviews (        -- P1：审核记录
  resource_id   TEXT PRIMARY KEY REFERENCES resources(resource_id),
  review_status TEXT, reviewer TEXT, reviewed_at TEXT,
  technical_quality TEXT, maintenance_quality TEXT, license_status TEXT,
  security_status TEXT, decision TEXT, decision_reason TEXT
);

CREATE TABLE IF NOT EXISTS sync_state (
  source_id        TEXT PRIMARY KEY,
  upstream_repo    TEXT,
  default_branch   TEXT,
  last_commit_sha  TEXT,
  last_etag        TEXT,
  snapshot_hash    TEXT,
  source_license   TEXT,
  last_successful_sync TEXT,
  last_sync_status TEXT,
  last_error       TEXT
);

CREATE TABLE IF NOT EXISTS events (
  id     INTEGER PRIMARY KEY AUTOINCREMENT,
  ts     TEXT NOT NULL,
  actor  TEXT NOT NULL DEFAULT 'eri-p0',
  action TEXT NOT NULL,
  detail TEXT
);
"""


def connect() -> sqlite3.Connection:
    os.makedirs(os.path.dirname(DB_PATH), exist_ok=True)
    con = sqlite3.connect(DB_PATH, timeout=30)
    con.row_factory = sqlite3.Row
    con.executescript(SCHEMA)
    return con


def log_event(con, action: str, detail: dict):
    con.execute("INSERT INTO events(ts, actor, action, detail) VALUES (?,?,?,?)",
                (now_iso(), "eri-p0", action, json.dumps(detail, ensure_ascii=False)))


# ─────────────────────────── 入库管线 ───────────────────────────

def ingest(readme_path: str, meta_path: str | None, commit_sha: str,
           source_id: str = DEFAULT_SOURCE_ID, force: bool = False) -> dict:
    text = open(readme_path, "r", encoding="utf-8").read()
    content_hash = sha256_hex(text.encode("utf-8"))
    con = connect()
    try:
        row = con.execute("SELECT * FROM sync_state WHERE source_id=?", (source_id,)).fetchone()
        if (row and not force and row["last_commit_sha"] == commit_sha
                and row["snapshot_hash"] == content_hash):
            log_event(con, "sync_noop", {"source_id": source_id, "commit": commit_sha,
                                         "reason": "unchanged-commit-and-hash"})
            con.execute("UPDATE sync_state SET last_successful_sync=?, last_sync_status='noop' "
                        "WHERE source_id=?", (now_iso(), source_id))
            con.commit()
            return {"status": "noop", "reason": "commit-and-hash-unchanged",
                    "commit": commit_sha, "content_hash": content_hash}

        meta = {}
        if meta_path and os.path.exists(meta_path):
            meta = json.load(open(meta_path, encoding="utf-8"))
        parsed = parse_readme(text, {"source_id": source_id,
                                     "source_repository": meta.get("full_name", DEFAULT_SOURCE_REPO),
                                     "source_commit": commit_sha})
        dd = dedup_batch(parsed["records"])

        ts = now_iso()
        added = updated = unchanged = 0
        seen_now = set()
        for rid in dd["order"]:
            item = dd["merged"][rid]
            rec = item["primary"]
            seen_now.add(rid)
            old = con.execute("SELECT * FROM resources WHERE resource_id=?", (rid,)).fetchone()
            if old is None:
                gh = rec["identity"]
                con.execute(
                    "INSERT INTO resources(resource_id, identity, canonical_url, resource_type,"
                    " name, description, owner, repo_name, status, first_seen, last_seen,"
                    " last_source_commit, created_at, updated_at)"
                    " VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
                    (rid, f"gh:{gh[0].lower()}/{gh[1].lower()}" if gh else None,
                     rec["resource_url"], rec["resource_type"], rec["resource_name"],
                     rec["resource_description"] or None,
                     rec["identity"][0] if gh else urllib.parse.urlsplit(rec["resource_url"]).hostname,
                     rec["identity"][1] if gh else None, "INDEXED", ts, ts, commit_sha, ts, ts))
                added += 1
                log_event(con, "resource_added", {"resource_id": rid, "name": rec["resource_name"]})
            else:
                fields, vals = [], []
                if old["name"] != rec["resource_name"]:
                    fields.append("name"); vals.append(rec["resource_name"])
                if (old["description"] or "") != rec["resource_description"]:
                    fields.append("description"); vals.append(rec["resource_description"] or None)
                if fields:
                    fields += ["updated_at", "last_source_commit", "status"]
                    vals += [ts, commit_sha, "INDEXED"]
                    con.execute(f"UPDATE resources SET {', '.join(f + '=?' for f in fields)} "
                                f"WHERE resource_id=?", (*vals, rid))
                    updated += 1
                    log_event(con, "resource_updated",
                              {"resource_id": rid, "fields": fields[:-3]})
                else:
                    unchanged += 1
                    if old["status"] != "INDEXED":  # 曾被标记 UNLISTED 的资源重新出现
                        con.execute("UPDATE resources SET status='INDEXED', last_seen=?,"
                                    " last_source_commit=?, updated_at=? WHERE resource_id=?",
                                    (ts, commit_sha, ts, rid))
                    else:
                        con.execute("UPDATE resources SET last_seen=?, last_source_commit=?"
                                    " WHERE resource_id=?", (ts, commit_sha, rid))

            for occ in item["occurrences"]:
                con.execute(
                    "INSERT INTO resource_sources(resource_id, source_id, source_repository,"
                    " source_file, source_section, source_position, source_commit, source_url,"
                    " first_seen, last_seen) VALUES (?,?,?,?,?,?,?,?,?,?)"
                    " ON CONFLICT(resource_id, source_section, source_url) DO UPDATE SET"
                    " source_commit=excluded.source_commit, last_seen=excluded.last_seen,"
                    " source_position=excluded.source_position",
                    (rid, source_id, rec["source_repository"], occ["source_file"],
                     occ["source_section"], occ["source_position"], commit_sha,
                     occ["resource_url_raw"], ts, ts))
                con.execute(
                    "INSERT OR IGNORE INTO resource_categories(resource_id, category,"
                    " category_source, confidence) VALUES (?,?,?,1.0)",
                    (rid, occ["category"], f"{source_id}:{occ['source_file']}"))

        # 来源消失：软删除（保留历史，标记 UNLISTED）
        stale = con.execute(
            "SELECT resource_id, COUNT(*) n FROM resource_sources WHERE source_id=?"
            " GROUP BY resource_id", (source_id,)).fetchall()
        removed = []
        for r in stale:
            if r["resource_id"] in seen_now:
                continue
            cur = con.execute("SELECT status FROM resources WHERE resource_id=?",
                              (r["resource_id"],)).fetchone()
            if cur and cur["status"] not in ("UNLISTED",):
                con.execute("UPDATE resources SET status='UNLISTED', updated_at=? "
                            "WHERE resource_id=?", (ts, r["resource_id"]))
                removed.append(r["resource_id"])
                log_event(con, "source_removed",
                          {"resource_id": r["resource_id"], "reason": "absent-in-latest-snapshot"})

        con.execute(
            "INSERT INTO sync_state(source_id, upstream_repo, default_branch, last_commit_sha,"
            " snapshot_hash, source_license, last_successful_sync, last_sync_status, last_error)"
            " VALUES (?,?,?,?,?,?,?,?,?)"
            " ON CONFLICT(source_id) DO UPDATE SET last_commit_sha=excluded.last_commit_sha,"
            " snapshot_hash=excluded.snapshot_hash, default_branch=excluded.default_branch,"
            " source_license=excluded.source_license,"
            " last_successful_sync=excluded.last_successful_sync,"
            " last_sync_status='ok', last_error=NULL",
            (source_id, meta.get("full_name", DEFAULT_SOURCE_REPO),
             meta.get("default_branch", "master"), commit_sha, content_hash, SOURCE_LICENSE,
             ts, "ok", None))
        con.execute(
            "INSERT INTO resource_versions(source_id, commit_sha, content_hash, fetched_at,"
            " resources_added, resources_updated, resources_removed) VALUES (?,?,?,?,?,?,?)"
            " ON CONFLICT(source_id, commit_sha) DO UPDATE SET fetched_at=excluded.fetched_at,"
            " resources_added=excluded.resources_added,"
            " resources_updated=excluded.resources_updated,"
            " resources_removed=excluded.resources_removed",
            (source_id, commit_sha, content_hash, ts, added, updated, len(removed)))
        log_event(con, "ingest_done", {
            "source_id": source_id, "commit": commit_sha, "content_hash": content_hash,
            "parsed": len(parsed["records"]), "skipped": len(parsed["skipped"]),
            "added": added, "updated": updated, "unchanged": unchanged,
            "removed": len(removed), "cross_section_dupes": dd["cross_section"],
            "name_variants": dd["name_variants"]})
        con.commit()
        return {"status": "ok", "commit": commit_sha, "content_hash": content_hash,
                "parsed": len(parsed["records"]), "skipped": len(parsed["skipped"]),
                "skipped_detail": parsed["skipped"], "added": added, "updated": updated,
                "unchanged": unchanged, "removed": removed,
                "cross_section_dupes": dd["cross_section"],
                "name_variants": dd["name_variants"]}
    finally:
        con.close()


# ─────────────────────────── 上游同步（第 8 节） ───────────────────────────

def _http_get(url: str, accept: str = "application/json", retries: int = 3,
              timeout: int = 60) -> tuple[bytes, dict]:
    last_err = None
    for attempt in range(retries):
        try:
            req = urllib.request.Request(url, headers={
                "User-Agent": "eri-p0 (+RTIS external resource intelligence)",
                "Accept": accept})
            with urllib.request.urlopen(req, timeout=timeout) as resp:
                return resp.read(), dict(resp.headers)
        except urllib.error.HTTPError as e:
            if e.code in (403, 429) and e.headers.get("x-ratelimit-remaining") == "0":
                raise RuntimeError("GitHub API rate limit exhausted; retry later") from e
            last_err = e
            if e.code < 500:
                raise
        except (urllib.error.URLError, TimeoutError, ConnectionError) as e:
            last_err = e
        time.sleep(2 * (attempt + 1))
    raise RuntimeError(f"GET failed after {retries} attempts: {url} ({last_err})")


def sync(source_id: str = DEFAULT_SOURCE_ID, force: bool = False) -> dict:
    repo = DEFAULT_SOURCE_REPO
    meta_bytes, _ = _http_get(f"https://api.github.com/repos/{repo}")
    meta = json.loads(meta_bytes)
    branch = meta.get("default_branch") or "master"

    commit_bytes, _ = _http_get(
        f"https://api.github.com/repos/{repo}/commits?path=README.md&per_page=1")
    commits = json.loads(commit_bytes)
    if not commits:
        raise RuntimeError("upstream returned no README commit")
    sha = commits[0]["sha"]

    state = connect()
    row = state.execute("SELECT last_commit_sha FROM sync_state WHERE source_id=?",
                        (source_id,)).fetchone()
    state.close()
    if row and row["last_commit_sha"] == sha and not force:
        return {"status": "noop", "reason": "commit-unchanged", "commit": sha,
                "branch": branch, "api_calls": 2}

    tgz, _ = _http_get(f"https://codeload.github.com/{repo}/tar.gz/refs/heads/{branch}",
                       accept="application/x-gzip", timeout=120)
    tf = tarfile.open(fileobj=BytesIO(tgz), mode="r:gz")
    readme_data = license_data = None
    for member in tf.getmembers():
        name = member.name.rsplit("/", 1)[-1]
        if member.isfile() and name == "README.md" and readme_data is None:
            readme_data = tf.extractfile(member).read()
        elif member.isfile() and name == "LICENSE" and license_data is None:
            license_data = tf.extractfile(member).read()
    tf.close()
    if readme_data is None:
        raise RuntimeError("README.md not found in tarball")

    short = sha[:12]
    snap_dir = os.path.join(UPSTREAM_DIR, source_id, short)
    os.makedirs(snap_dir, exist_ok=True)
    readme_path = os.path.join(snap_dir, "README.md")
    with open(readme_path, "wb") as f:
        f.write(readme_data)
    if license_data:
        with open(os.path.join(snap_dir, "LICENSE"), "wb") as f:
            f.write(license_data)
    with open(os.path.join(snap_dir, "repo-meta.json"), "w", encoding="utf-8") as f:
        json.dump(meta, f, ensure_ascii=False, indent=2)
    with open(os.path.join(snap_dir, "snapshot.json"), "w", encoding="utf-8") as f:
        json.dump({"source_id": source_id, "commit_sha": sha, "branch": branch,
                   "content_hash": sha256_hex(readme_data), "fetched_at": now_iso(),
                   "via": "codeload-tarball"}, f, ensure_ascii=False, indent=2)

    result = ingest(readme_path, os.path.join(snap_dir, "repo-meta.json"), sha,
                    source_id=source_id, force=force)
    result["branch"] = branch
    result["snapshot_dir"] = snap_dir
    result["api_calls"] = 3
    return result


# ─────────────────────────── 检索（第 9.2 节，元数据层） ───────────────────────────

def search(terms: list, category: str | None = None, limit: int = SEARCH_DEFAULT_LIMIT) -> list:
    limit = max(1, min(limit, SEARCH_MAX_LIMIT))
    con = connect()
    try:
        where, params = [], []
        if terms:
            likes = []
            for t in terms:
                pat = f"%{t}%"
                likes.append("(r.name LIKE ? OR r.description LIKE ? OR r.owner LIKE ?"
                             " OR r.repo_name LIKE ? OR r.canonical_url LIKE ?)")
                params += [pat] * 5
            where.append("(" + " AND ".join(likes) + ")")
        if category:
            where.append("r.resource_id IN (SELECT resource_id FROM resource_categories"
                         " WHERE category LIKE ?)")
            params.append(f"%{category}%")
        sql = ("SELECT r.resource_id, r.name, r.resource_type, r.status, r.canonical_url,"
               " r.description, r.owner, r.repo_name,"
               " GROUP_CONCAT(DISTINCT c.category) AS categories"
               " FROM resources r LEFT JOIN resource_categories c USING(resource_id)"
               + (" WHERE " + " AND ".join(where) if where else "")
               + " GROUP BY r.resource_id"
               " ORDER BY CASE WHEN r.name LIKE ? THEN 0 ELSE 1 END, r.name"
               f" LIMIT {limit}")
        if terms:
            params.append(f"%{terms[0]}%")
        return [dict(r) for r in con.execute(sql, params).fetchall()]
    finally:
        con.close()


def get(resource_id: str) -> dict | None:
    con = connect()
    try:
        r = con.execute("SELECT * FROM resources WHERE resource_id=? OR identity=?",
                        (resource_id, resource_id)).fetchone()
        if not r:
            return None
        out = dict(r)
        out["sources"] = [dict(x) for x in con.execute(
            "SELECT * FROM resource_sources WHERE resource_id=? ORDER BY source_section",
            (r["resource_id"],)).fetchall()]
        out["categories"] = [dict(x) for x in con.execute(
            "SELECT * FROM resource_categories WHERE resource_id=?",
            (r["resource_id"],)).fetchall()]
        return out
    finally:
        con.close()


def stats() -> dict:
    con = connect()
    try:
        q = lambda sql, p=(): [dict(x) for x in con.execute(sql, p).fetchall()]  # noqa: E731
        return {
            "db": DB_PATH,
            "totals": q("SELECT resource_type, COUNT(*) n FROM resources GROUP BY 1"),
            "by_status": q("SELECT status, COUNT(*) n FROM resources GROUP BY 1"),
            "by_category": q("SELECT c.category, COUNT(DISTINCT c.resource_id) n"
                             " FROM resource_categories c GROUP BY 1 ORDER BY n DESC"),
            "sources_rows": con.execute("SELECT COUNT(*) FROM resource_sources").fetchone()[0],
            "sync_state": [dict(x) for x in con.execute(
                "SELECT source_id, default_branch, last_commit_sha, source_license,"
                " last_successful_sync, last_sync_status FROM sync_state").fetchall()],
        }
    finally:
        con.close()


def dedup_report() -> dict:
    con = connect()
    try:
        same_name = [dict(x) for x in con.execute(
            "SELECT name, GROUP_CONCAT(resource_id) ids, COUNT(*) n FROM resources"
            " GROUP BY LOWER(name) HAVING n > 1")]
        gh_variants = [dict(x) for x in con.execute(
            "SELECT identity, canonical_url FROM resources WHERE identity IS NOT NULL"
            " AND identity NOT LIKE 'gh:%'")]
        url_dupes = [dict(x) for x in con.execute(
            "SELECT canonical_url, COUNT(*) n FROM resources GROUP BY canonical_url HAVING n>1")]
        return {"same_name_different_repo": same_name, "identity_anomalies": gh_variants,
                "canonical_url_dupes": url_dupes}
    finally:
        con.close()


def report() -> str:
    con = connect()
    try:
        lines = ["# ERI 资源清单（resource-inventory）", "",
                 f"生成时间：{now_iso()}　数据库：{DB_PATH}", ""]
        cats = con.execute(
            "SELECT category, COUNT(DISTINCT resource_id) n FROM resource_categories"
            " GROUP BY 1 ORDER BY n DESC").fetchall()
        for c in cats:
            lines += [f"## {c['category']}（{c['n']}）", "",
                      "| 资源 ID | 名称 | 类型 | 状态 | URL |", "|---|---|---|---|---|"]
            rows = con.execute(
                "SELECT r.resource_id, r.name, r.resource_type, r.status, r.canonical_url"
                " FROM resources r JOIN resource_categories c USING(resource_id)"
                " WHERE c.category=? ORDER BY r.name", (c["category"],)).fetchall()
            for r in rows:
                lines.append(f"| `{r['resource_id']}` | {r['name']} | {r['resource_type']}"
                             f" | {r['status']} | {r['canonical_url']} |")
            lines.append("")
        return "\n".join(lines)
    finally:
        con.close()


# ─────────────────────────── CLI ───────────────────────────

def main(argv=None):
    ap = argparse.ArgumentParser(description="ERI external resource intelligence (P0)")
    sub = ap.add_subparsers(dest="cmd", required=True)

    p = sub.add_parser("parse", help="解析 README，不落库")
    p.add_argument("readme")
    p.add_argument("--json", action="store_true")

    p = sub.add_parser("ingest", help="解析并入库")
    p.add_argument("--readme", required=True)
    p.add_argument("--meta")
    p.add_argument("--commit", required=True)
    p.add_argument("--source-id", default=DEFAULT_SOURCE_ID)
    p.add_argument("--force", action="store_true")

    p = sub.add_parser("sync", help="GitHub API 增量同步")
    p.add_argument("--source-id", default=DEFAULT_SOURCE_ID)
    p.add_argument("--force", action="store_true")

    p = sub.add_parser("search", help="元数据层检索（默认 5 条）")
    p.add_argument("terms", nargs="+")
    p.add_argument("--category")
    p.add_argument("--limit", type=int, default=SEARCH_DEFAULT_LIMIT)

    p = sub.add_parser("get", help="查看单个资源全记录")
    p.add_argument("resource_id")

    sub.add_parser("stats", help="目录统计")
    sub.add_parser("report", help="输出 Markdown 资源清单")
    sub.add_parser("dedup-report", help="目录内疑似重复候选")

    args = ap.parse_args(argv)
    if args.cmd == "parse":
        meta = {"source_id": DEFAULT_SOURCE_ID, "source_repository": DEFAULT_SOURCE_REPO,
                "source_commit": "dry-run"}
        out = parse_readme(open(args.readme, encoding="utf-8").read(), meta)
        if args.json:
            print(json.dumps(out, ensure_ascii=False, indent=2))
        else:
            dd = dedup_batch(out["records"])
            print(f"records={len(out['records'])} unique={len(dd['order'])} "
                  f"skipped={len(out['skipped'])} cross_section_dupes={dd['cross_section']}")
            for r in out["records"]:
                print(f"  L{r['source_position']:>4} [{r['source_section']}] "
                      f"{resource_id_for(r['identity'], r['resource_url'])}  {r['resource_name']}")
            if out["skipped"]:
                print("skipped:")
                for s in out["skipped"]:
                    print(f"  L{s['line']:>4} {s['reason']}: {s['url']}")
    elif args.cmd == "ingest":
        print(json.dumps(ingest(args.readme, args.meta, args.commit,
                                args.source_id, args.force), ensure_ascii=False, indent=2))
    elif args.cmd == "sync":
        print(json.dumps(sync(args.source_id, args.force), ensure_ascii=False, indent=2))
    elif args.cmd == "search":
        for r in search(args.terms, args.category, args.limit):
            desc = (r["description"] or "")[:100]
            print(f"{r['resource_id']}  {r['name']}  [{r['resource_type']}/{r['status']}]"
                  f"  cat={r['categories']}")
            print(f"    {r['canonical_url']}")
            if desc:
                print(f"    {desc}")
    elif args.cmd == "get":
        out = get(args.resource_id)
        print(json.dumps(out, ensure_ascii=False, indent=2) if out else "not found")
    elif args.cmd == "stats":
        print(json.dumps(stats(), ensure_ascii=False, indent=2))
    elif args.cmd == "report":
        print(report())
    elif args.cmd == "dedup-report":
        print(json.dumps(dedup_report(), ensure_ascii=False, indent=2))


if __name__ == "__main__":
    main()
