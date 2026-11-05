#!/usr/bin/env python3
"""ERI P2：ResourceResolver / RTIS 集成 / ResourceReader / UsageTracker / 审核队列。

在 P0/P1 之上只增不改。检索全程确定性（SQL + 规则评分），零 LLM。
AI 深度阅读属后续阶段；本阶段 ai_usage_log 仍为 0。

CLI：
  python3 -I eri_p2.py resolve --task-id T --keywords "k1,k2" [--profile-json '{...}']
                               [--skills s1,s2] [--workflows w1] [--limit 5] [--json]
  python3 -I eri_p2.py read <resource_id> [--kind readme|license] [--section "标题"]
  python3 -I eri_p2.py track <event_type> --task-id T [--query-id Q] [--resource-id R]
                             [--result ok] [--reason "..."]
  python3 -I eri_p2.py funnel [--task-id T]
  python3 -I eri_p2.py review-d5 | review-mappings | review-licenses
  python3 -I eri_p2.py bridge-gate --task-file <task.json>   # RTIS gate 后处理（stdout=JSON）
  python3 -I eri_p2.py perf --rows 100000                    # 合成性能测试（隔离临时库）
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import sqlite3
import sys
import tempfile
import time
import urllib.error
import urllib.request
from datetime import datetime, timezone

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import eri        # noqa: E402
import eri_p1     # noqa: E402

CACHE_DIR = os.path.join(HERE, "cache")     # 测试可覆盖
RESOLVER_WEIGHTS = {                        # 初始权重，可调（计划 §2.2）
    "keyword": 0.25, "category": 0.20, "tech": 0.15,
    "profile": 0.15, "relation": 0.15, "quality": 0.10,
}
ALIASES = {                                 # 确定性别名扩展
    "js": ["javascript"], "spa": ["javascript", "bundle", "frontend"],
    "api": ["openapi", "swagger", "rest"], "android": ["apk", "mobile"],
    "crypto": ["cryptography", "jwt", "encryption"], "cdn": ["edge", "cloudfront"],
    "web": ["webapp", "webapplication"], "landing": ["landing", "redirect"],
    "miniapp": ["wechat", "wxapp"], "ios": ["osx", "iphone"],
}
CJK_BRIDGE = {                              # profile dims 中文特征 -> 检索词（确定性）
    "落地页": ["landing", "redirect"], "跳转": ["redirect"], "资产": ["asset", "discovery"],
    "加密": ["crypto", "encryption"], "密文": ["crypto", "encryption"],
    "小程序": ["miniapp", "wechat"], "边缘": ["cdn", "edge"], "代理": ["proxy"],
    "移动": ["mobile", "android"], "安卓": ["android", "apk"], "接口": ["api"],
    "渗透": ["pentest"], "溯源": ["osint"], "口令": ["password"], "密码": ["password"],
    "端口": ["scan", "recon"], "跳板": ["tunnel", "pivot"], "隧道": ["tunnel"],
}
EXCLUDED_STATUSES = {"QUARANTINED", "REJECTED", "UNAVAILABLE"}
DEFAULT_TOP_K = 5


def now_iso() -> str:
    return datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


def p2_connect() -> sqlite3.Connection:
    con = eri_p1.p1_connect()
    con.executescript("""
CREATE TABLE IF NOT EXISTS resolver_queries (
  query_id TEXT PRIMARY KEY,
  task_id TEXT, created_at TEXT,
  input_json TEXT, result_ids TEXT,
  candidates INTEGER, filtered INTEGER, returned INTEGER, cache_hits INTEGER,
  elapsed_ms REAL, search_status TEXT
);
CREATE TABLE IF NOT EXISTS resource_usage (
  event_id INTEGER PRIMARY KEY AUTOINCREMENT,
  task_id TEXT, query_id TEXT, resource_id TEXT,
  event_type TEXT NOT NULL CHECK (event_type IN
    ('SEARCHED','RETURNED','OPENED','SELECTED','APPLIED','SUCCESSFUL','REJECTED')),
  workflow_id TEXT, skill_id TEXT,
  ts TEXT NOT NULL, result TEXT, reason TEXT
);
CREATE INDEX IF NOT EXISTS ix_usage_task ON resource_usage(task_id);
CREATE INDEX IF NOT EXISTS ix_usage_rid ON resource_usage(resource_id);
CREATE TABLE IF NOT EXISTS reader_cache (
  resource_id TEXT NOT NULL,
  kind TEXT NOT NULL,                    -- readme | license
  content_hash TEXT, byte_len INTEGER,
  commit_sha TEXT, etag TEXT,
  summary TEXT, summary_content_hash TEXT, summary_commit TEXT,
  path TEXT, fetched_at TEXT,
  PRIMARY KEY (resource_id, kind)
);
CREATE TABLE IF NOT EXISTS review_log (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  queue TEXT NOT NULL,                   -- d5 | mapping | license
  item_key TEXT NOT NULL,                -- 'a|b' / 'resource->target' / resource_id
  decision TEXT NOT NULL, confidence REAL,
  reason TEXT, reviewer TEXT DEFAULT 'eri-p2-rules',
  reviewed_at TEXT,
  UNIQUE(queue, item_key)
);
""")
    return con


# ─────────────────────────── ResourceResolver ───────────────────────────

def _expand(keywords: list) -> list:
    out = []
    for k in keywords:
        k = (k or "").lower().strip()
        if not k:
            continue
        out.append(k)
        out.extend(ALIASES.get(k, []))
    return sorted(set(out))


def _norm_profile(profile: dict) -> list:
    """site_profile -> 特征词（只取有证据的字段，不虚构）。"""
    words = []
    for v in (profile.get("technologies") or []):
        words.append(str(v).lower())
    for v in (profile.get("archetypes") or []):
        words.extend(str(v).lower().replace("_", "-").split("-"))
    for v in (profile.get("features") or []):
        words.append(str(v).lower())
    return [w for w in _expand(words) if len(w) >= 3]


def _cjk_bridge(texts: list) -> list:
    """中文画像特征 -> 英文检索词。只做查表，不做翻译。"""
    out = []
    joined = " ".join(str(t) for t in texts)
    for cjk, en in CJK_BRIDGE.items():
        if cjk in joined:
            out.extend(en)
    return sorted(set(out))


def resolve(task_context: dict, limit: int = DEFAULT_TOP_K) -> dict:
    con = p2_connect()
    t0 = time.perf_counter()
    try:
        task_id = task_context.get("task_id") or "adhoc"
        query_id = "erq-" + datetime.now().strftime("%Y%m%d%H%M%S%f") + \
            "-" + hashlib.sha1(json.dumps(task_context, sort_keys=True).encode()).hexdigest()[:6]
        kw = _expand(list(task_context.get("keywords") or []) +
                     list(task_context.get("technologies") or []))
        kw += [w for w in _cjk_bridge(task_context.get("keywords") or []) if w not in kw]
        profile_words = _norm_profile(task_context.get("site_profile") or {})
        task_skills = set(task_context.get("skill_ids") or [])
        task_workflows = set(task_context.get("workflow_ids") or [])

        # 1) SQL 预筛候选（排除隔离/拒绝/不可用；排除 D5 高风险未审重复对中的低分侧）
        like_clause = " OR ".join(
            "(r.name LIKE ? OR r.description LIKE ? OR m.description LIKE ?"
            " OR m.topics LIKE ? OR r.canonical_url LIKE ?)" for _ in kw) or "1=1"
        params: list = []
        for k in kw:
            params += [f"%{k}%"] * 5
        cand = con.execute(
            f"SELECT r.resource_id, r.name, r.description, r.canonical_url, r.resource_type,"
            f" r.status, r.owner, m.topics, m.language, m.license_status, m.maintenance_status,"
            f" q.final_score, q.grade"
            f" FROM resources r LEFT JOIN github_metadata m USING(resource_id)"
            f" LEFT JOIN quality_scores q USING(resource_id)"
            f" WHERE r.status NOT IN ({','.join('?' * len(EXCLUDED_STATUSES))})"
            f" AND ({like_clause})",
            [*EXCLUDED_STATUSES, *params]).fetchall()

        # 2) 关系映射一次性载入
        maps: dict[str, list] = {}
        for row in con.execute(
                "SELECT resource_id, target, mapping_kind, confidence FROM skill_mappings"
                " WHERE mapping_status IN ('SUGGESTED','REVIEWED','ADOPTED')"):
            maps.setdefault(row["resource_id"], []).append(dict(row))
        cats: dict[str, list] = {}
        for row in con.execute("SELECT resource_id, category FROM resource_categories"):
            cats.setdefault(row["resource_id"], []).append(row["category"])

        scored = []
        filtered = 0
        for c in cand:
            rid = c["resource_id"]
            blob = " ".join([c["name"] or "", c["description"] or "",
                             c["topics"] or "[]", c["language"] or ""]).lower()
            blob_toks = {seg for tok in re.findall(r"[a-z0-9+#.\-]{3,}", blob)
                         for seg in tok.split("-")}

            # 词元级匹配：整词或复数；合成词靠 ALIASES 显式扩展，不用前缀防误中
            kw_hits = [k for k in kw if k in blob_toks
                       or (k + "s") in blob_toks]
            if not kw_hits and not profile_words and not task_skills:
                filtered += 1
                continue
            reasons, weights_used = [], {}

            # keyword 25%
            if kw_hits:
                cov = len(kw_hits) / max(len(kw), 1)
                weights_used["keyword"] = (RESOLVER_WEIGHTS["keyword"], min(1.0, cov * 1.5))
                reasons.append(f"keyword hits: {kw_hits[:5]}")
            # category 20%（资源分类/章节与任务词交集）
            cat_hits = [x for x in cats.get(rid, []) if any(
                t in x.lower() for t in (kw + profile_words))] if kw or profile_words else []
            if cat_hits:
                weights_used["category"] = (RESOLVER_WEIGHTS["category"], 1.0)
                reasons.append(f"category: {cat_hits}")
            # tech 15%（topics/language 与任务技术词；无证据不计分）
            if c["topics"] and c["topics"] != "[]" and any(t in blob for t in (kw + profile_words)):
                weights_used["tech"] = (RESOLVER_WEIGHTS["tech"], 0.8)
                reasons.append("tech topics match")
            # profile 15%
            prof_hits = [p for p in profile_words if p in blob]
            if prof_hits:
                weights_used["profile"] = (RESOLVER_WEIGHTS["profile"],
                                           min(1.0, len(prof_hits) / max(len(profile_words), 1) * 2))
                reasons.append(f"profile features: {prof_hits[:5]}")
            # relation 15%（该资源映射到的 skill 与任务 skill 交集）
            rel_hits = [m for m in maps.get(rid, [])
                        if m["target"].split(":", 1)[-1] in task_skills
                        or m["target"] in task_workflows]
            if rel_hits:
                weights_used["relation"] = (RESOLVER_WEIGHTS["relation"],
                                            min(1.0, max(m["confidence"] for m in rel_hits) * 2))
                reasons.append(f"skill/workflow links: {[m['target'] for m in rel_hits][:3]}")
            # quality 10%
            if c["final_score"] is not None:
                weights_used["quality"] = (RESOLVER_WEIGHTS["quality"],
                                           min(1.0, c["final_score"] / 100.0))
                reasons.append(f"quality {c['final_score']} ({c['grade']})")

            if not weights_used:
                filtered += 1
                continue
            wsum = sum(w for w, _ in weights_used.values())
            score = round(sum(w * v for w, v in weights_used.values()) / wsum, 4) if wsum else 0.0
            missing = [k for k in RESOLVER_WEIGHTS if k not in weights_used]
            if missing:
                reasons.append("no evidence for: " + ",".join(missing) + " (权重已按证据归一)")
            rel_maps = maps.get(rid, [])
            license_note = None
            if c["license_status"] in (None, "UNKNOWN", "NO_LICENSE_FILE", "FILE_UNIDENTIFIED"):
                license_note = f"license={c['license_status'] or 'UNKNOWN'}: 仅供参考，不可直接采用为依赖"
            scored.append({
                "resource_id": rid, "name": c["name"], "canonical_url": c["canonical_url"],
                "description": c["description"], "resource_type": c["resource_type"],
                "category_matches": cats.get(rid, []),
                "keyword_matches": kw_hits,
                "skill_relationships": [m["target"] for m in rel_maps],
                "workflow_relationships": [],
                "quality": c["final_score"], "grade": c["grade"],
                "license_status": c["license_status"] or "UNKNOWN",
                "maintenance_status": c["maintenance_status"] or "UNKNOWN",
                "adoptable_as_dependency": license_note is None,
                "license_note": license_note,
                "relevance_score": score, "relevance_reasons": reasons,
            })
        scored.sort(key=lambda x: (-x["relevance_score"], -(x["quality"] or 0), x["resource_id"]))
        top = scored[:limit]

        # already_covered_by_internal：任务 skill 已映射覆盖的资源
        covered = []
        for rid, ms in maps.items():
            for m in ms:
                if m["target"].split(":", 1)[-1] in task_skills:
                    covered.append({"resource_id": rid, "covered_by": m["target"]})
        gaps = [{"resource_id": s["resource_id"],
                 "reason": "任务相关度高但未映射到任何现有 skill，可能为能力缺口候选"}
                for s in top if not maps.get(s["resource_id"])]

        elapsed = (time.perf_counter() - t0) * 1000
        result = {
            "query_id": query_id, "task_id": task_id,
            "matched_resources": top,
            "retrieval_stats": {"candidates": len(cand), "filtered": filtered,
                                "returned": len(top), "cache_hits": 0},
            "already_covered_by_internal": covered,
            "capability_gaps": gaps[:5],
            "search_status": "ok" if scored else "no_match",
            "weights_config": RESOLVER_WEIGHTS,
        }
        con.execute("INSERT INTO resolver_queries(query_id, task_id, created_at, input_json,"
                    " result_ids, candidates, filtered, returned, cache_hits, elapsed_ms,"
                    " search_status) VALUES (?,?,?,?,?,?,?,?,?,?,?)",
                    (query_id, task_id, now_iso(),
                     json.dumps(task_context, ensure_ascii=False),
                     json.dumps([r["resource_id"] for r in top]),
                     len(cand), filtered, len(top), 0, round(elapsed, 2),
                     result["search_status"]))
        con.execute("INSERT INTO resource_usage(task_id, query_id, resource_id, event_type,"
                    " ts, result) VALUES (?,?,?,?,?,?)",
                    (task_id, query_id, None, "SEARCHED", now_iso(), result["search_status"]))
        for r in top:
            con.execute("INSERT INTO resource_usage(task_id, query_id, resource_id, event_type,"
                        " ts, result) VALUES (?,?,?,?,?,?)",
                        (task_id, query_id, r["resource_id"], "RETURNED", now_iso(),
                         f"score={r['relevance_score']}"))
        con.commit()
        return result
    finally:
        con.close()


# ─────────────────────────── ResourceReader ───────────────────────────

def _fetch(url: str, etag: str | None = None, accept: str = "application/vnd.github.raw"):
    h = {"User-Agent": "eri-p2-reader", "Accept": accept}
    if etag:
        h["If-None-Match"] = etag
    req = urllib.request.Request(url, headers=h)
    try:
        with urllib.request.urlopen(req, timeout=60) as r:
            return r.status, r.read(), dict(r.headers)
    except urllib.error.HTTPError as e:
        return e.code, b"", dict(e.headers)


def _tarball_read(full_name: str, filename: str, branch: str = "master") -> bytes | None:
    """api 端点不可用时的回落通道：codeload tarball 直接提取单文件。"""
    import tarfile
    from io import BytesIO
    last = None
    for attempt in range(3):
        try:
            req = urllib.request.Request(
                f"https://codeload.github.com/{full_name}/tar.gz/refs/heads/{branch}",
                headers={"User-Agent": "eri-p2-reader"})
            with urllib.request.urlopen(req, timeout=120) as r:
                tgz = r.read()
            tf = tarfile.open(fileobj=BytesIO(tgz), mode="r:gz")
            try:
                for m in tf.getmembers():
                    if m.isfile() and m.name.rsplit("/", 1)[-1] == filename:
                        return tf.extractfile(m).read()
            finally:
                tf.close()
            return None
        except Exception as e:  # noqa: BLE001
            last = e
            time.sleep(2 * (attempt + 1))
    raise RuntimeError(f"tarball fetch failed: {full_name} ({last})")


def read(resource_id: str, kind: str = "readme", section: str | None = None,
         transport=None) -> dict:
    """按需读取 + 缓存。原文缓存于 cache/<rid>/，哈希与 commit 入 reader_cache。
    summary 仅在内容哈希变化时重算（本阶段规则摘要，零 LLM）。
    默认传输层带 3 次重试退避（api.github.com 在本机偶发 TLS EOF）。"""
    transport = transport or eri_p1.prod_transport
    con = p2_connect()
    try:
        row = con.execute("SELECT r.owner, r.repo_name FROM resources r"
                          " WHERE r.resource_id=?", (resource_id,)).fetchone()
        if not row or not row["owner"] or not row["repo_name"]:
            return {"error": f"not a github resource: {resource_id}"}
        cached = con.execute("SELECT * FROM reader_cache WHERE resource_id=? AND kind=?",
                             (resource_id, kind)).fetchone()
        cache_hit = False
        url = (f"https://api.github.com/repos/{row['owner']}/{row['repo_name']}/readme"
               if kind == "readme" else
               f"https://api.github.com/repos/{row['owner']}/{row['repo_name']}/license")
        try:
            status, body, headers = transport(url, cached["etag"] if cached else None)
        except Exception:  # noqa: BLE001  网络异常：回落陈旧缓存
            status, body, headers = None, b"", {}

        if not cache_hit:
            have_cache_file = bool(cached and cached["path"]
                                   and os.path.exists(cached["path"]))
            if status not in (200, 304) and not have_cache_file:
                # api 端点不可用且无本地缓存：codeload tarball 回落
                try:
                    fname = "README.md" if kind == "readme" else "LICENSE"
                    tb = _tarball_read(f"{row['owner']}/{row['repo_name']}", fname)
                    if tb:
                        body, status, headers = tb, 200, {}
                except Exception:  # noqa: BLE001
                    pass
            if status == 304 and cached and cached["path"] \
                    and os.path.exists(cached["path"]):
                cache_hit = True
                text = open(cached["path"], encoding="utf-8", errors="replace").read()
                c_hash = cached["content_hash"]
            elif status == 200:
                text = body.decode("utf-8", errors="replace")
                c_hash = hashlib.sha256(text.encode()).hexdigest()
                if cached and cached["content_hash"] == c_hash:
                    # tarball 通道无 ETag：内容哈希相同即视为命中，不重写缓存
                    cache_hit = "tarball-unchanged"
                    text = open(cached["path"], encoding="utf-8",
                                errors="replace").read() if cached["path"] \
                        and os.path.exists(cached["path"]) else text
                else:
                    commit = None
                    d = os.path.join(CACHE_DIR, resource_id.replace("/", "_").replace(":", "_"))
                    os.makedirs(d, exist_ok=True)
                    path = os.path.join(d, kind + ".md")
                    open(path, "w", encoding="utf-8").write(text)
                    # 摘要仅依赖内容哈希：内容不变不重算（本阶段为规则摘要：章节目录）
                    if not cached or cached["summary_content_hash"] != c_hash:
                        summary = json.dumps(_section_index(text), ensure_ascii=False)
                    else:
                        summary = cached["summary"]
                    con.execute(
                        "INSERT INTO reader_cache(resource_id, kind, content_hash, byte_len,"
                        " commit_sha, etag, summary, summary_content_hash, path, fetched_at)"
                        " VALUES (?,?,?,?,?,?,?,?,?,?)"
                        " ON CONFLICT(resource_id, kind) DO UPDATE SET"
                        " content_hash=excluded.content_hash, byte_len=excluded.byte_len,"
                        " etag=excluded.etag, summary=excluded.summary,"
                        " summary_content_hash=excluded.summary_content_hash,"
                        " path=excluded.path, fetched_at=excluded.fetched_at",
                        (resource_id, kind, c_hash, len(body), commit,
                         headers.get("etag") or headers.get("ETag"), summary, c_hash,
                         path, now_iso()))
                    con.commit()
            elif cached:
                # 网络失败回落缓存（标注陈旧）
                text = open(cached["path"], encoding="utf-8", errors="replace").read()
                c_hash, cache_hit = cached["content_hash"], "stale-cache"
            else:
                return {"error": f"fetch http-{status}", "resource_id": resource_id}

        out = {"resource_id": resource_id, "kind": kind, "cache": cache_hit or "miss",
               "content_hash": c_hash, "byte_len": len(text or "")}
        idx = json.loads(cached["summary"]) if (cache_hit and cached and cached["summary"]) \
            else _section_index(text or "")
        if section:
            out["section"] = _extract_section(text or "", section)
        else:
            out["section_index"] = idx
        # OPENED 事件
        con.execute("INSERT INTO resource_usage(resource_id, event_type, ts, result)"
                    " VALUES (?,?,?,?)", (resource_id, "OPENED", now_iso(),
                                          f"kind={kind} cache={out['cache']}"))
        con.commit()
        return out
    finally:
        con.close()


def _section_index(text: str) -> list:
    idx = []
    for m in re.finditer(r"^(#{2,3})\s+(.+?)\s*$", text, re.M):
        idx.append({"level": len(m.group(1)), "title": m.group(2)})
    return idx[:50]


def _extract_section(text: str, title: str) -> str:
    pat = re.compile(r"^(#{2,3})\s+(.+?)\s*$", re.M)
    matches = list(pat.finditer(text))
    for i, m in enumerate(matches):
        if title.lower() in m.group(2).lower():
            start = m.end()
            end = matches[i + 1].start() if i + 1 < len(matches) else len(text)
            return text[start:end].strip()
    return ""


# ─────────────────────────── UsageTracker ───────────────────────────

def track(event_type: str, task_id=None, query_id=None, resource_id=None,
          workflow_id=None, skill_id=None, result=None, reason=None) -> dict:
    if event_type not in ("SEARCHED", "RETURNED", "OPENED", "SELECTED",
                          "APPLIED", "SUCCESSFUL", "REJECTED"):
        return {"error": f"bad event_type {event_type}"}
    con = p2_connect()
    try:
        con.execute("INSERT INTO resource_usage(task_id, query_id, resource_id, event_type,"
                    " workflow_id, skill_id, ts, result, reason) VALUES (?,?,?,?,?,?,?,?,?)",
                    (task_id, query_id, resource_id, event_type, workflow_id, skill_id,
                     now_iso(), result, reason))
        con.commit()
        return {"ok": True}
    finally:
        con.close()


def funnel(task_id: str | None = None) -> dict:
    con = p2_connect()
    try:
        where, params = "", []
        if task_id:
            where = "WHERE task_id=?"
            params = [task_id]
        counts = {r["event_type"]: r["n"] for r in con.execute(
            f"SELECT event_type, COUNT(*) n FROM resource_usage {where} GROUP BY 1", params)}
        searched = counts.get("SEARCHED", 0)
        returned = counts.get("RETURNED", 0)
        opened = counts.get("OPENED", 0)
        applied = counts.get("APPLIED", 0)
        successful = counts.get("SUCCESSFUL", 0)
        rejected = counts.get("REJECTED", 0)
        pct = lambda a, b: round(100.0 * a / b, 1) if b else None  # noqa: E731
        return {"counts": counts,
                "rates": {
                    "retrieval_rate": pct(returned, searched),
                    "open_rate": pct(opened, returned),
                    "application_rate": pct(applied, returned),
                    "success_rate": pct(successful, applied),
                    "rejection_rate": pct(rejected, returned)},
                "by_resource": [dict(r) for r in con.execute(
                    f"SELECT resource_id, event_type, COUNT(*) n FROM resource_usage {where}"
                    + (" AND" if where else " WHERE") +
                    " resource_id IS NOT NULL GROUP BY resource_id, event_type", params)]}
    finally:
        con.close()


# ─────────────────────────── RTIS gate 桥 ───────────────────────────

def bridge_gate(task: dict) -> dict:
    """RTIS gate 之后调用：把 task 里已有的画像/技能上下文转成 resolver 查询。
    输出 external_resource_review（计划 §3.2 结构）。绝不抛异常。"""
    try:
        profile = (task.get("profile") or {})
        dims = profile.get("dims") or profile.get("dimensions") or {}
        dim_words = []
        for v in dims.values():
            val = v.get("value") if isinstance(v, dict) else v
            if isinstance(val, str) and val:
                dim_words.append(val)
            elif isinstance(val, list):
                dim_words.extend(str(x) for x in val if x)
        objective = str(task.get("objective") or "")
        ctx = {
            "task_id": task.get("task_id") or task.get("id") or "rtis-task",
            "keywords": list(task.get("keywords") or []) + [objective] + dim_words,
            "site_profile": {"technologies": dim_words,
                             "archetypes": profile.get("archetypes") or [],
                             "features": profile.get("features") or []},
            "skill_ids": list(task.get("skills") or []),
            "workflow_ids": list(task.get("workflows") or []),
        }
        ctx["keywords"] = [k for k in ctx["keywords"] if k][:12]
        res = resolve(ctx)
        recommended, rejected = [], []
        # 排除规则已在 resolve 内执行；这里按 license/质量二分展示
        for r in res["matched_resources"]:
            if r["grade"] in ("A", "B") and r["adoptable_as_dependency"]:
                recommended.append(r["resource_id"])
            else:
                rejected.append({"resource_id": r["resource_id"],
                                 "reason": r["license_note"] or f"grade={r['grade']}"})
        return {"external_resource_review": {
            "status": res["search_status"], "query_id": res["query_id"],
            "candidate_count": res["retrieval_stats"]["returned"],
            "recommended_resources": recommended,
            "rejected_resources": rejected,
            "already_covered_by_internal": res["already_covered_by_internal"],
            "capability_gaps": res["capability_gaps"],
            "detailed_research_required": bool(res["capability_gaps"]),
            "reason": "存在未映射的任务相关资源，需按需深读" if res["capability_gaps"]
                      else "内部能力已覆盖或无显著相关资源",
        }}
    except Exception as e:  # noqa: BLE001
        return {"external_resource_review": {"status": "error", "error": repr(e)[:200]}}


# ─────────────────────────── 审核队列 ───────────────────────────

def review_d5() -> dict:
    """D5 候选规则复核：FORK/MIRROR/同名同主/高描述重叠/独立。不删除任何资源。"""
    con = p2_connect()
    try:
        n = 0
        for c in con.execute("SELECT * FROM d5_candidates").fetchall():
            a = con.execute("SELECT r.*, m.full_name, m.fork, m.parent_repository,"
                            " m.stargazers_count FROM resources r"
                            " LEFT JOIN github_metadata m USING(resource_id)"
                            " WHERE r.resource_id=?", (c["resource_a"],)).fetchone()
            b = con.execute("SELECT r.*, m.full_name, m.fork, m.parent_repository,"
                            " m.stargazers_count FROM resources r"
                            " LEFT JOIN github_metadata m USING(resource_id)"
                            " WHERE r.resource_id=?", (c["resource_b"],)).fetchone()
            if not a or not b:
                continue
            decision, conf, why = "NEEDS_MORE_EVIDENCE", 0.3, []
            if c["repository_relationship"].startswith("fork"):
                decision, conf = "FORK_KEEP_BOTH", 0.9
                why.append(f"元数据证实 fork：{a['full_name']} fork of {b['full_name']}；"
                           "保留双方，fork 可能在原项目停滞时独立演进")
            elif (a["owner"] or "").lower() == (b["owner"] or "").lower() \
                    and c["name_similarity"] >= 0.8:
                decision, conf = "SAME_OWNER_VARIANTS", 0.7
                why.append("同一 owner 的不同仓库，通常为不同项目代际或姊妹项目，保留")
            elif c["name_similarity"] >= 0.85 and c["description_similarity"] < 0.2:
                decision, conf = "DISTINCT", 0.6
                why.append("名字近似但描述无重叠，判独立项目")
            elif c["description_similarity"] >= 0.5:
                decision, conf = "RELATED_KEEP_BOTH", 0.6
                why.append("描述显著重叠但分属不同 owner，判相关而非重复")
            else:
                why.append("证据不足，维持待审，不自动处理")
            con.execute("INSERT INTO review_log(queue, item_key, decision, confidence,"
                        " reason, reviewed_at) VALUES ('d5',?,?,?,?,?)"
                        " ON CONFLICT(queue, item_key) DO UPDATE SET decision=excluded.decision,"
                        " confidence=excluded.confidence, reason=excluded.reason,"
                        " reviewed_at=excluded.reviewed_at",
                        (f"{c['resource_a']}|{c['resource_b']}", decision, conf,
                         "; ".join(why), now_iso()))
            n += 1
        con.commit()
        return {"reviewed": n}
    finally:
        con.close()


def review_mappings(skills_dir: str | None = None) -> dict:
    """18 条 SUGGESTED 逐项审：读 skill 真实 front matter + 正文词，比对资源描述。
    状态：APPROVED_REFERENCE / COMPLEMENTARY / DUPLICATE_CAPABILITY / NEEDS_MORE_EVIDENCE / REJECTED。"""
    skills_dir = skills_dir or eri_p1.SKILLS_DIR
    con = p2_connect()
    try:
        skills_cache = {}
        n = 0
        for m in con.execute("SELECT * FROM skill_mappings WHERE mapping_kind='skill'"
                             " AND mapping_status='SUGGESTED'").fetchall():
            sname = m["target"].split(":", 1)[-1]
            path = os.path.join(skills_dir, sname + ".md")
            if sname not in skills_cache:
                try:
                    skills_cache[sname] = open(path, encoding="utf-8").read()
                except OSError:
                    skills_cache[sname] = ""
            sk_text = skills_cache[sname]
            r = con.execute("SELECT r.name, r.description, m.description AS gh_desc,"
                            " m.topics FROM resources r"
                            " LEFT JOIN github_metadata m USING(resource_id)"
                            " WHERE r.resource_id=?", (m["resource_id"],)).fetchone()
            res_text = " ".join([r["name"] or "", r["description"] or "",
                                 r["gh_desc"] or "", r["topics"] or ""]).lower()
            sk_toks = eri_p1.tokens(sk_text[:2000])
            res_toks = set(re.findall(r"[a-z0-9+#.\-]{3,}", res_text))
            shared = sk_toks & res_toks - eri_p1.STOPWORDS
            # 判定规则（确定性，可解释）
            is_list_resource = bool(re.search(r"awesome|list|reference|collection",
                                              res_text)) or "awesome" in (m["resource_id"] or "")
            if not sk_text:
                decision, conf = "NEEDS_MORE_EVIDENCE", 0.2
                reason = "skill 文件缺失或不可读"
            elif is_list_resource:
                decision, conf = "APPROVED_REFERENCE", 0.7
                reason = ("资源为索引/列表类，与 skill 属参考关系而非能力重复；"
                          f"共享词 {sorted(shared)[:8]}")
            elif len(shared) >= 5:
                decision, conf = "DUPLICATE_CAPABILITY", 0.6
                reason = (f"skill 与项目功能词高度重叠（{sorted(shared)[:8]}），"
                          "引入前需对比成熟度")
            elif len(shared) >= 2:
                decision, conf = "COMPLEMENTARY", 0.5
                reason = f"部分重叠，判断为互补（共享词 {sorted(shared)[:8]}）"
            else:
                decision, conf = "NEEDS_MORE_EVIDENCE", 0.3
                reason = f"正文比对共享词过少（{sorted(shared)[:8]}），维持候选"
            con.execute("UPDATE skill_mappings SET mapping_status=?, confidence=?,"
                        " match_basis=?, updated_at=? WHERE id=?",
                        (decision, conf, reason[:300], now_iso(), m["id"]))
            con.execute("INSERT INTO review_log(queue, item_key, decision, confidence,"
                        " reason, reviewed_at) VALUES ('mapping',?,?,?,?,?)"
                        " ON CONFLICT(queue, item_key) DO UPDATE SET decision=excluded.decision,"
                        " confidence=excluded.confidence, reason=excluded.reason,"
                        " reviewed_at=excluded.reviewed_at",
                        (f"{m['resource_id']}->{m['target']}", decision, conf, reason, now_iso()))
            n += 1
        con.commit()
        return {"reviewed": n}
    finally:
        con.close()


def review_licenses() -> dict:
    """License 分层审核：只对 A/B 级深入标记，不无差别调用外部资源。"""
    con = p2_connect()
    try:
        n = 0
        for row in con.execute(
                "SELECT q.resource_id, q.grade, m.license_status, m.license_spdx_id"
                " FROM quality_scores q LEFT JOIN github_metadata m USING(resource_id)"
                " WHERE m.license_status IS NOT NULL").fetchall():
            st, spdx, grade = row["license_status"], row["license_spdx_id"], row["grade"]
            if st == "IDENTIFIED" and spdx in eri_p1.PERMISSIVE:
                decision, reason = "OK_PERMISSIVE", f"SPDX {spdx}，宽松许可"
            elif st == "IDENTIFIED" and spdx in eri_p1.COPYLEFT:
                decision, reason = "OK_COPYLEFT_NOTE", f"SPDX {spdx}，传染性许可，采用前评估"
            elif st == "IDENTIFIED":
                decision, reason = "OK_REVIEW_SPDX", f"SPDX {spdx}，非标准分类，采用前读 LICENSE"
            elif st == "FILE_UNIDENTIFIED":
                decision = "NEEDS_HUMAN_REVIEW"
                reason = "License 文件存在但 API 未识别；仅高价值项人工核读"
            elif st == "NO_LICENSE_FILE":
                decision = ("NEEDS_HUMAN_REVIEW" if grade in ("A", "B")
                            else "RISK_NO_LICENSE")
                reason = ("未检测到 License 文件（不等于确证无许可）；"
                          + ("A/B 级高价值，建议人工核读 LICENSE/COPYING/README 声明"
                             if grade in ("A", "B") else "低优先级，默认不可作为依赖采用"))
            else:
                decision, reason = "UNKNOWN", "API 状态未知"
            con.execute("INSERT INTO review_log(queue, item_key, decision, confidence,"
                        " reason, reviewed_at) VALUES ('license',?,?,?,?,?)"
                        " ON CONFLICT(queue, item_key) DO UPDATE SET decision=excluded.decision,"
                        " reason=excluded.reason, reviewed_at=excluded.reviewed_at",
                        (row["resource_id"], decision, 0.8, reason, now_iso()))
            n += 1
        con.commit()
        return {"reviewed": n}
    finally:
        con.close()


# ─────────────────────────── 性能验证（合成库，隔离） ───────────────────────────

_WORDS = ("scan exploit proxy fuzz inject bypass crawl decode monitor asset recon "
          "payload auth token cipher packet trace honeypot tunnel pivot enum brute "
          "javascript android api web cloud edge cdn crypto jwt saml oauth sso wasm "
          "grpc graphql rest soap xss sqli ssrf rce lfi rdf deserial").split()


def perf(rows: int = 100000) -> dict:
    tmpdir = tempfile.mkdtemp(prefix="eri-perf-")
    old_db = eri.DB_PATH
    try:
        eri.DB_PATH = os.path.join(tmpdir, "perf.db")
        con = p2_connect()
        # 合成资源：确定性伪随机（同参数可复现）
        rng_state = 12345

        def rnd(n):
            nonlocal rng_state
            rng_state = (rng_state * 1103515245 + 12345) % (2 ** 31)
            return rng_state % n

        t0 = time.perf_counter()
        batch = []
        for i in range(rows):
            owner = f"org{rnd(500)}"
            repo = f"repo{i}"
            desc = " ".join(_WORDS[rnd(len(_WORDS))] for _ in range(8))
            batch.append((f"gh:{owner}/{repo}", f"gh:{owner}/{repo}",
                          f"https://github.com/{owner}/{repo}", "github_repo",
                          f"Synthetic {repo}", desc, owner, repo, "INDEXED",
                          "2026-01-01T00:00:00Z", "2026-01-01T00:00:00Z",
                          "2026-01-01T00:00:00Z", "2026-01-01T00:00:00Z", None, None))
            if len(batch) >= 5000:
                con.executemany(
                    "INSERT OR IGNORE INTO resources(resource_id, identity, canonical_url,"
                    " resource_type, name, description, owner, repo_name, status,"
                    " first_seen, last_seen, created_at, updated_at, quality_score,"
                    " trust_level) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)", batch)
                batch = []
        if batch:
            con.executemany(
                "INSERT OR IGNORE INTO resources(resource_id, identity, canonical_url,"
                " resource_type, name, description, owner, repo_name, status,"
                " first_seen, last_seen, created_at, updated_at, quality_score,"
                " trust_level) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)", batch)
        con.execute("UPDATE resources SET quality_score=ABS(RANDOM()%100)")
        con.commit()
        seed_s = round((time.perf_counter() - t0) * 1000, 1)

        # 查询基准（真实 resolver 路径）
        ctx = {"task_id": "perf", "keywords": ["fuzz", "bypass"],
               "site_profile": {"technologies": ["javascript"], "archetypes": ["spa-api"]},
               "skill_ids": []}
        t0 = time.perf_counter()
        r1 = resolve(ctx, limit=5)
        q1 = round((time.perf_counter() - t0) * 1000, 1)
        ctx2 = {"task_id": "perf2", "keywords": ["android", "apk", "fuzz", "proxy"],
                "site_profile": {"technologies": ["android"]}, "skill_ids": ["recon"]}
        t0 = time.perf_counter()
        r2 = resolve(ctx2, limit=10)
        q2 = round((time.perf_counter() - t0) * 1000, 1)
        # 多条件过滤（SQL 直查）
        t0 = time.perf_counter()
        con.execute("SELECT r.resource_id FROM resources r JOIN quality_scores q"
                    " USING(resource_id) WHERE r.status='INDEXED' AND q.grade='A'"
                    " AND q.final_score>60 ORDER BY q.final_score DESC LIMIT 20").fetchall()
        q3 = round((time.perf_counter() - t0) * 1000, 1)
        # 并发读（4 线程同时 resolver）
        import threading
        errs = []
        def worker():
            try:
                resolve({"task_id": "perf-parallel", "keywords": ["scan", "crawl"]}, limit=5)
            except Exception as e:  # noqa: BLE001
                errs.append(repr(e))
        t0 = time.perf_counter()
        ts = [threading.Thread(target=worker) for _ in range(4)]
        [t.start() for t in ts]
        [t.join() for t in ts]
        q4 = round((time.perf_counter() - t0) * 1000, 1)
        n_rows = con.execute("SELECT COUNT(*) FROM resources").fetchone()[0]
        ok_integrity = con.execute("PRAGMA integrity_check").fetchone()[0]
        # 备份/恢复基准
        t0 = time.perf_counter()
        eri_p1.BACKUP_DIR = os.path.join(tmpdir, "bak")
        bak = eri_p1.backup()
        q5 = round((time.perf_counter() - t0) * 1000, 1)
        con.close()
        return {"rows": n_rows, "seed_seconds": seed_s,
                "resolve_top5_ms": q1, "resolve_top10_multi_ms": q2,
                "filter_topk_ms": q3, "parallel4_ms": q4, "parallel_errors": errs,
                "backup_100k_ms": q5, "integrity": ok_integrity, "tmpdir": tmpdir}
    finally:
        eri.DB_PATH = old_db


# ─────────────────────────── CLI ───────────────────────────

def main(argv=None):
    ap = argparse.ArgumentParser(description="ERI P2")
    sub = ap.add_subparsers(dest="cmd", required=True)

    p = sub.add_parser("resolve")
    p.add_argument("--task-id", default="adhoc")
    p.add_argument("--keywords", default="")
    p.add_argument("--profile-json", default="{}")
    p.add_argument("--skills", default="")
    p.add_argument("--workflows", default="")
    p.add_argument("--limit", type=int, default=DEFAULT_TOP_K)
    p.add_argument("--json", action="store_true")

    p = sub.add_parser("read")
    p.add_argument("resource_id")
    p.add_argument("--kind", default="readme", choices=["readme", "license"])
    p.add_argument("--section")

    p = sub.add_parser("track")
    p.add_argument("event_type")
    p.add_argument("--task-id"); p.add_argument("--query-id")
    p.add_argument("--resource-id"); p.add_argument("--workflow-id")
    p.add_argument("--skill-id"); p.add_argument("--result"); p.add_argument("--reason")

    p = sub.add_parser("funnel"); p.add_argument("--task-id")

    sub.add_parser("review-d5"); sub.add_parser("review-mappings")
    sub.add_parser("review-licenses")

    p = sub.add_parser("bridge-gate"); p.add_argument("--task-file", required=True)
    p = sub.add_parser("perf"); p.add_argument("--rows", type=int, default=100000)

    args = ap.parse_args(argv)
    if args.cmd == "resolve":
        ctx = {"task_id": args.task_id,
               "keywords": [k for k in args.keywords.split(",") if k],
               "site_profile": json.loads(args.profile_json),
               "skill_ids": [s for s in args.skills.split(",") if s],
               "workflow_ids": [w for w in args.workflows.split(",") if w]}
        out = resolve(ctx, args.limit)
        if args.json:
            print(json.dumps(out, ensure_ascii=False, indent=2))
        else:
            for r in out["matched_resources"]:
                print(f"{r['relevance_score']:.3f} [{r['grade'] or '-'}]"
                      f" {r['resource_id']}  {r['name']}")
                print(f"    {r['canonical_url']}")
                for why in r["relevance_reasons"]:
                    print(f"    - {why}")
                if r["license_note"]:
                    print(f"    ! {r['license_note']}")
            print(f"stats: {out['retrieval_stats']}  status: {out['search_status']}")
    elif args.cmd == "read":
        print(json.dumps(read(args.resource_id, args.kind, args.section),
                         ensure_ascii=False, indent=2))
    elif args.cmd == "track":
        print(json.dumps(track(args.event_type, args.task_id, args.query_id,
                               args.resource_id, args.workflow_id, args.skill_id,
                               args.result, args.reason), ensure_ascii=False))
    elif args.cmd == "funnel":
        print(json.dumps(funnel(args.task_id), ensure_ascii=False, indent=2))
    elif args.cmd == "review-d5":
        print(json.dumps(review_d5(), ensure_ascii=False))
    elif args.cmd == "review-mappings":
        print(json.dumps(review_mappings(), ensure_ascii=False))
    elif args.cmd == "review-licenses":
        print(json.dumps(review_licenses(), ensure_ascii=False))
    elif args.cmd == "bridge-gate":
        task = json.load(open(args.task_file, encoding="utf-8"))
        print(json.dumps(bridge_gate(task), ensure_ascii=False, indent=2))
    elif args.cmd == "perf":
        print(json.dumps(perf(args.rows), ensure_ascii=False, indent=2))


if __name__ == "__main__":
    main()
