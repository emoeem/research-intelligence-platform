#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
rt.py — 红队知识库引擎（CLI 子命令 + stdio MCP server），双模运行。
让 zcode / DSH / 任何能起子进程的宿主复用同一份本地知识库。零第三方依赖。

═══════════════════════════════════════════════════════════════════════════
数据在哪（2026-11-02 实测核对：sqlite3 .tables + 读 store-core.js/schema.js）
═══════════════════════════════════════════════════════════════════════════

【全局知识库】~/.dsh/redteam/knowledge.db   （单一真源，28MB）
  · poc        14,649 条 POC/EXP/script/template/payload，UNIQUE(code)
               字段：code title kind category cve component versions severity
                     language source source_url description usage content path
                     verified verified_note hit_count used_on
                     engagement_id engagement_name asset_target found_by_agent
                     tags created_by created_at updated_at
  · poc_fts    FTS5 全文索引（普通 fts5 表，不是 external content）
               列：poc_id(UNINDEXED) title cve component versions tags description content
               ⚠️ 陷阱：fts5 自带 rowid ≠ poc.id（14,554 / 14,649 行不相等）。
               检索必须 `WHERE id IN (SELECT poc_id FROM poc_fts WHERE poc_fts MATCH ?)`，
               绝不能拿 rowid 当 poc.id 用。

【每靶标事实库】~/.dsh/redteam/engagements/<slug>/assets.db   ← 关键：资产/漏洞/链路/得分全在这里
  这些数据 **不在** knowledge.db 里（knowledge.db 只有 poc 相关 7 张表）。
  一个靶标一份 SQLite，DDL 见 store 的 schema.js：DDL 常量（25+ 表）：
  · 资产面   segment(网段) asset(资产, UNIQUE(segment_cidr,ip)) asset_name port
             service fingerprint observation edge tag
             视图 v_asset_summary / v_asset_service；FTS 表 asset_fts
  · 漏洞面   vuln(标题/严重级/状态/证据/gained) http_evidence(原始请求响应)
             attack_file(攻击文件, UNIQUE(target,name))
  · 链路     attack_step(seq 排序的步骤, tool/result/agent/stage_code) stage(五阶段定义)
  · 得分     score_point(得分点, code/cap/dedup_scope/builtin) score_hit(命中,
             points/multiplier/self_created/stage_code)
  · 入口     credential access_session webshell tunnel shot(证据截图)
  靶标元数据 engagement.yaml（target_name / scope_cidrs / status / created_at）
  角色提示词 agents/{plan,recon,assess,vuln-scan,exploit,internal}.md
  运行产物   runs/ 与 reports/
  当前靶标   ~/.dsh/redteam/current（纯文本 slug，单行）

【POC 落盘】~/.dsh/redteam/pocs/<code>/<filename>
  poc.path 存**绝对路径**（例：/home/emo/.dsh/redteam/pocs/<code>/poc.sh）。
  458 / 14,649 条的 path 为空。写操作不修改既有落盘文件内容。

【skill 库】~/.dsh/redteam/skills/<name>.md   125 个
  YAML front-matter：name / description / whenToUse / role / enabled
  role 分布（实测）：vuln-scan 72、recon 20、internal 14、plan 7、exploit 6，
  另有 6 个文件没有 role 字段（av-evasion / confluence-atlassian / electron-client /
  iot-snmp-pivot / opsec-anti-trace / phishing-delivery）。

【纪律】除 poc add/use/update 三处显式写操作外，全部只读；不发起任何网络请求。
═══════════════════════════════════════════════════════════════════════════
"""

from __future__ import annotations

import argparse
import json
import os
import re
import sqlite3
import sys
import unicodedata
from datetime import datetime, timezone
from pathlib import Path

VERSION = "1.0.0"
SERVER_NAME = "rt-knowledge"
PROTOCOL_VERSION = "2024-11-05"

HOME = Path(
    os.environ.get("RT_HOME")
    or os.environ.get("REDTEAM_HOME")
    or Path(__file__).resolve().parent  # 2026-11-09 解耦：默认 kit 自身目录（RIP integrations/rtis）
)
KB = HOME / "knowledge.db"
POCS = HOME / "pocs"
SKILLS = HOME / "skills"
ENGAGEMENTS = HOME / "engagements"
CURRENT_FILE = HOME / "current"

DEFAULT_LIMIT = 20
MAX_LIMIT = 200

POC_COLS = [
    "code", "title", "kind", "category", "cve", "component", "versions", "severity",
    "language", "source", "source_url", "description", "usage", "content", "path",
    "verified", "verified_note", "hit_count", "used_on", "engagement_id",
    "engagement_name", "asset_target", "found_by_agent", "tags", "created_by",
]

# 搜索返回的紧凑字段（content 太重，只在 get 里出）
SEARCH_COLS = [
    "id", "code", "title", "kind", "category", "cve", "component", "versions",
    "severity", "language", "source", "verified", "verified_note", "hit_count",
    "used_on", "engagement_id", "engagement_name", "asset_target", "found_by_agent",
    "tags", "path", "description", "created_at", "updated_at",
]

LIKE_COLS = [
    "code", "title", "cve", "component", "versions", "tags", "description",
    "content", "usage", "source_url", "asset_target", "verified_note",
]

FILENAME_BY_LANG = {
    "python": "poc.py", "go": "poc.go", "java": "Poc.java", "bash": "poc.sh",
    "shell": "poc.sh", "http": "request.http", "nuclei": "poc.yaml",
    "js": "poc.js", "javascript": "poc.js", "php": "poc.php",
}


# ────────────────────────────────────────────────────────────── 基础工具
def log(*args) -> None:
    """调试日志一律走 stderr —— stdout 在 MCP 模式下只能是协议帧。"""
    print(*args, file=sys.stderr, flush=True)


def die(msg: str, code: int = 2):
    log(f"rt: {msg}")
    raise SystemExit(code)


def now_iso() -> str:
    return datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%S.") + \
        f"{datetime.now(timezone.utc).microsecond // 1000:03d}Z"


def truthy(v) -> int:
    if isinstance(v, bool):
        return 1 if v else 0
    if isinstance(v, (int, float)):
        return 1 if v else 0
    if isinstance(v, str):
        return 1 if v.strip().lower() in ("1", "true", "yes", "y", "on") else 0
    return 0


def clamp_limit(v, default: int = DEFAULT_LIMIT) -> int:
    try:
        n = int(v)
    except (TypeError, ValueError):
        n = default
    return max(1, min(n, MAX_LIMIT))


def connect() -> sqlite3.Connection:
    if not KB.exists():
        die(f"知识库不存在: {KB}")
    con = sqlite3.connect(str(KB), timeout=15)
    con.row_factory = sqlite3.Row
    return con


_SLUG_DROP = re.compile(r"[^0-9A-Za-z_\u4e00-\u9fff]+")


def slugify(text: str, maxlen: int = 80) -> str:
    """和 store 的 slug 风格一致：保留 CJK + 字母数字，其余折成单个 '-'。"""
    s = _SLUG_DROP.sub("-", (text or "").strip())
    s = re.sub(r"-{2,}", "-", s).strip("-")
    return (s[:maxlen].strip("-") or "poc")


def _cwidth(ch: str) -> int:
    return 2 if unicodedata.east_asian_width(ch) in ("W", "F") else 1


def dw(s) -> int:
    return sum(_cwidth(c) for c in str(s))


def pad(s, width: int) -> str:
    s = str(s)
    if dw(s) > width:
        out, cur = [], 0
        for ch in s:
            w = _cwidth(ch)
            if cur + w > width - 1:
                out.append("…")
                break
            out.append(ch)
            cur += w
        s = "".join(out)
    return s + " " * max(0, width - dw(s))


def table(headers, rows, caps=None) -> str:
    """等宽（含 CJK 双宽）表格；caps 给每列最大宽度。"""
    caps = caps or [40] * len(headers)
    cells = [[str(c) if c is not None else "" for c in r] for r in rows]
    widths = []
    for i, h in enumerate(headers):
        w = dw(h)
        for r in cells:
            if i < len(r):
                w = max(w, min(dw(r[i]), caps[i]))
        widths.append(min(max(w, dw(h)), caps[i]))
    line = "  ".join(pad(h, widths[i]) for i, h in enumerate(headers))
    out = [line, "  ".join("-" * widths[i] for i in range(len(headers)))]
    for r in cells:
        out.append("  ".join(pad(r[i] if i < len(r) else "", widths[i]) for i in range(len(headers))))
    return "\n".join(out)


def emit_json(obj) -> None:
    """--json 一律纯 JSON 到 stdout（ensure_ascii=False 便于人工核对中文）。"""
    print(json.dumps(obj, ensure_ascii=False, indent=2, default=str))


# ────────────────────────────────────────────────────────────── POC 检索
def _fts_ranked(con, q: str, limit: int = 450):
    """按 bm25 返回命中的 poc_id（越靠前越相关）。

    FTS5 的查询串里 '-' ':' '(' '"' 都是语法成分：`rt-selftest` 会被当成
    列过滤直接抛 `no such column: selftest`。所以先用原串试，失败再把整串
    包成短语重试，两次都失败返回 None（调用方降级 LIKE）。
    """
    candidates = [q, '"' + q.replace('"', '""') + '"']
    last = None
    for cand in candidates:
        try:
            return [r[0] for r in con.execute(
                "SELECT poc_id FROM poc_fts WHERE poc_fts MATCH ?"
                " ORDER BY bm25(poc_fts) LIMIT ?", (cand, limit))]
        except sqlite3.Error as exc:
            last = exc
    log(f"[fts] 检索失败，降级 LIKE: {last}")
    return None


def _like_clause(q: str):
    clause = " OR ".join(f"{c} LIKE ?" for c in LIKE_COLS)
    return f"({clause})", [f"%{q}%"] * len(LIKE_COLS)


def search_pocs(q=None, cve=None, component=None, category=None, kind=None,
                verified=False, engagement=None, asset=None, limit=DEFAULT_LIMIT,
                with_content=False) -> list:
    limit = clamp_limit(limit)
    con = connect()
    try:
        where, params = [], []
        rank_map = None

        if q:
            ranked = _fts_ranked(con, q) or []
            # LIKE 并集：FTS5 的 unicode61 把连续 CJK 当一个 token，
            # 「反序列化」搜不到「反序列化漏洞」——中文必须补 LIKE，否则召回丢一半。
            clause, ps = _like_clause(q)
            like_ids = [r[0] for r in con.execute(
                f"SELECT id FROM poc WHERE {clause}"
                " ORDER BY verified DESC, COALESCE(hit_count,0) DESC, id DESC LIMIT 450", ps)]

            cand, seen = [], set()
            for pid in ranked:
                if pid not in seen:
                    seen.add(pid)
                    cand.append(pid)
            rank_map = {pid: i for i, pid in enumerate(ranked)}
            for i, pid in enumerate(like_ids):
                if pid not in seen:
                    seen.add(pid)
                    cand.append(pid)
                    rank_map[pid] = 10 ** 6 + i   # LIKE-only 命中排在 bm25 之后

            if cand:
                where.append("id IN (%s)" % ",".join("?" * len(cand)))
                params.extend(cand)
            else:
                where.append("0 = 1")   # FTS 与 LIKE 皆无命中 → 空结果

        if cve:
            where.append("cve LIKE ?")
            params.append(f"%{cve}%")
        if component:
            where.append("component LIKE ?")
            params.append(f"%{component}%")
        if category:
            where.append("lower(COALESCE(category,'')) = lower(?)")
            params.append(category)
        if kind:
            where.append("lower(COALESCE(kind,'')) = lower(?)")
            params.append(kind)
        if verified:
            where.append("verified = 1")
        if engagement:
            where.append("(engagement_id LIKE ? OR engagement_name LIKE ?)")
            params.extend([f"%{engagement}%", f"%{engagement}%"])
        if asset:
            where.append("asset_target LIKE ?")
            params.append(f"%{asset}%")

        cols = ", ".join(SEARCH_COLS)
        if with_content:
            cols += ", content"
        sql = f"SELECT {cols}, length(content) AS content_chars FROM poc"
        if where:
            sql += " WHERE " + " AND ".join(where)

        rows = [dict(r) for r in con.execute(sql, params)]

        # 排序：已验证优先 → 手工条目优先于 nuclei 模板 → 相关度 → 复用次数 → 新条目
        # 为什么把 nuclei 降一档：库里有 14,111 条自动导入的 nuclei 模板，纯按关键词
        # 它们会淹没手工编写的高信息量条目；而 nuclei 模板本身就是 `nuclei -t <path>`
        # 一句话可跑，不需要排在前面给人读。
        ql = (q or "").strip().lower()

        def _tier(r):
            return 0 if (r.get("source") or "") != "nuclei" else 1

        def _rel(r):
            if rank_map is not None:
                return rank_map.get(r["id"], 10 ** 6)
            if not ql:
                return 0
            title = (r.get("title") or "").lower()
            code = (r.get("code") or "").lower()
            comp = (r.get("component") or "").lower()
            tags = (r.get("tags") or "").lower()
            if ql in title:
                return 0
            if ql in code or ql in comp:
                return 1
            if ql in tags:
                return 2
            return 3

        rows.sort(key=lambda r: (0 if r.get("verified") else 1, _tier(r), _rel(r),
                                 -(r.get("hit_count") or 0), -r["id"]))
        return rows[:limit]
    finally:
        con.close()


def get_poc(poc_id=None, code=None) -> dict:
    if poc_id is None and not code:
        die("poc get 需要 --id 或 --code")
    con = connect()
    try:
        if poc_id is not None:
            row = con.execute("SELECT * FROM poc WHERE id = ?", (int(poc_id),)).fetchone()
        else:
            code = str(code).strip()
            if code.endswith(".md"):
                code = code[:-3]
            row = con.execute("SELECT * FROM poc WHERE code = ?", (code,)).fetchone()
        if row is None:
            return {}
        d = dict(row)
        d["content_chars"] = len(d.get("content") or "")
        return d
    finally:
        con.close()


def _fts_refresh(con, poc_id: int, row) -> None:
    con.execute("DELETE FROM poc_fts WHERE poc_id = ?", (poc_id,))
    con.execute(
        "INSERT INTO poc_fts (poc_id,title,cve,component,versions,tags,description,content)"
        " VALUES (?,?,?,?,?,?,?,?)",
        (poc_id, row.get("title"), row.get("cve"), row.get("component"),
         row.get("versions"), row.get("tags"), row.get("description"), row.get("content")),
    )


def _materialize_content(code: str, merged: dict):
    """content 有、path 没有 → 写 pocs/<code>/<filename> 并回填 path。

    已有文件不覆盖（写操作不修改既有落盘内容），只在 stderr 提示。
    """
    content = merged.get("content")
    if not content or merged.get("path"):
        return merged.get("path") or None
    lang = (merged.get("language") or "").strip().lower()
    fname = FILENAME_BY_LANG.get(lang, "poc.txt")
    d = POCS / code
    f = d / fname
    try:
        d.mkdir(parents=True, exist_ok=True)
        if f.exists():
            log(f"[add] 落盘文件已存在，保留原内容: {f}")
        else:
            f.write_text(content if content.endswith("\n") else content + "\n", encoding="utf-8")
            try:
                f.chmod(0o755 if fname.endswith((".sh", ".py")) else 0o644)
            except OSError:
                pass
            log(f"[add] 正文已落盘: {f}")
    except OSError as exc:
        log(f"[add] 落盘失败（path 不回填）: {exc}")
        return None
    return str(f)


def add_poc(data: dict) -> dict:
    title = (data.get("title") or "").strip()
    if not title:
        die("poc add 需要 title")

    code = (data.get("code") or "").strip() or slugify(title)
    con = connect()
    try:
        existing = con.execute("SELECT * FROM poc WHERE code = ?", (code,)).fetchone()
        merged = dict(existing) if existing else {}
        for k in POC_COLS:
            if k in data and data[k] is not None:
                merged[k] = data[k]
        merged["code"] = code
        merged["title"] = title
        merged["verified"] = truthy(merged.get("verified"))
        merged["hit_count"] = int(merged.get("hit_count") or 0)

        path = _materialize_content(code, merged)
        if path:
            merged["path"] = path

        now = now_iso()
        if existing:
            sets = ", ".join(f"{c} = ?" for c in POC_COLS if c != "code")
            vals = [merged.get(c) for c in POC_COLS if c != "code"]
            con.execute(f"UPDATE poc SET {sets}, updated_at = ? WHERE id = ?",
                        vals + [now, existing["id"]])
            poc_id = existing["id"]
            created = False
        else:
            cols = POC_COLS + ["created_at", "updated_at"]
            vals = [merged.get(c) for c in POC_COLS] + [now, now]
            cur = con.execute(
                f"INSERT INTO poc ({', '.join(cols)}) VALUES ({', '.join('?' * len(cols))})", vals)
            poc_id = cur.lastrowid
            created = True

        row = con.execute("SELECT * FROM poc WHERE id = ?", (poc_id,)).fetchone()
        _fts_refresh(con, poc_id, dict(row))
        con.commit()
        return {"id": poc_id, "code": code, "path": merged.get("path"),
                "created": created, "verified": merged["verified"]}
    finally:
        con.close()


def update_poc(poc_id=None, code=None, fields=None) -> dict:
    fields = {k: v for k, v in (fields or {}).items() if v is not None}
    if not fields:
        die("poc update 没有给出任何要改的字段")
    con = connect()
    try:
        if poc_id is not None:
            row = con.execute("SELECT * FROM poc WHERE id = ?", (int(poc_id),)).fetchone()
        else:
            row = con.execute("SELECT * FROM poc WHERE code = ?", (str(code).strip(),)).fetchone()
        if row is None:
            return {}
        pid = row["id"]

        sets, vals = [], []
        for k, v in fields.items():
            if k not in POC_COLS:
                log(f"[update] 忽略未知字段: {k}")
                continue
            if k == "verified":
                v = truthy(v)
            sets.append(f"{k} = ?")
            vals.append(v)
        if not sets:
            die("poc update 没有可写字段")
        con.execute(f"UPDATE poc SET {', '.join(sets)}, updated_at = ? WHERE id = ?",
                    vals + [now_iso(), pid])

        row = con.execute("SELECT * FROM poc WHERE id = ?", (pid,)).fetchone()
        _fts_refresh(con, pid, dict(row))
        con.commit()
        return {"id": pid, "code": row["code"], "updated": [s.split(" = ")[0] for s in sets]}
    finally:
        con.close()


def use_poc(poc_id=None, code=None, used_on: str = "") -> dict:
    if not used_on:
        die("poc use 需要 --on <目标/靶标>")
    con = connect()
    try:
        if poc_id is not None:
            row = con.execute("SELECT * FROM poc WHERE id = ?", (int(poc_id),)).fetchone()
        else:
            row = con.execute("SELECT * FROM poc WHERE code = ?", (str(code).strip(),)).fetchone()
        if row is None:
            return {}
        con.execute(
            "UPDATE poc SET hit_count = COALESCE(hit_count,0) + 1, used_on = ?, updated_at = ?"
            " WHERE id = ?", (used_on, now_iso(), row["id"]))
        con.commit()
        return {"id": row["id"], "code": row["code"],
                "hit_count": int(row["hit_count"] or 0) + 1, "used_on": used_on}
    finally:
        con.close()


def kb_stats() -> dict:
    con = connect()
    try:
        total = con.execute("SELECT COUNT(*) FROM poc").fetchone()[0]
        verified = con.execute("SELECT COUNT(*) FROM poc WHERE verified = 1").fetchone()[0]

        def dist(col):
            rows = con.execute(
                f"SELECT COALESCE(NULLIF({col},''),'(空)') k, COUNT(*) c FROM poc"
                f" GROUP BY k ORDER BY c DESC").fetchall()
            return {r["k"]: r["c"] for r in rows}

        last = con.execute("SELECT MAX(updated_at) FROM poc").fetchone()[0]
        out = {
            "db": str(KB),
            "db_bytes": KB.stat().st_size if KB.exists() else 0,
            "total": total,
            "verified": verified,
            "unverified": total - verified,
            "content_chars": con.execute(
                "SELECT COALESCE(SUM(length(content)),0) FROM poc").fetchone()[0],
            "with_file": con.execute(
                "SELECT COUNT(*) FROM poc WHERE COALESCE(path,'') <> ''").fetchone()[0],
            "last_updated": last,
            "by_category": dist("category"),
            "by_kind": dist("kind"),
            "by_source": dist("source"),
            "by_severity": dist("severity"),
            "engagements": _engagement_overview(),
            "skills": {"dir": str(SKILLS), "count": len(list_skills())},
        }
        return out
    finally:
        con.close()


def _engagement_overview() -> list:
    """每个靶标 assets.db 的事实量（只读；资产/漏洞/链路/得分都在这里）。"""
    out = []
    if not ENGAGEMENTS.is_dir():
        return out
    for d in sorted(ENGAGEMENTS.iterdir()):
        if not d.is_dir():
            continue
        item = {"slug": d.name, "assets_db": (d / "assets.db").exists()}
        yml = d / "engagement.yaml"
        if yml.exists():
            try:
                for line in yml.read_text(encoding="utf-8", errors="replace").splitlines():
                    if line.startswith("target_name:"):
                        item["target_name"] = line.split(":", 1)[1].strip()
                    elif line.startswith("status:"):
                        item["status"] = line.split(":", 1)[1].strip()
            except OSError:
                pass
        db = d / "assets.db"
        if db.exists():
            try:
                c = sqlite3.connect(f"file:{db}?mode=ro", uri=True, timeout=5)
                for key, sql in (
                    ("assets", "SELECT COUNT(*) FROM asset"),
                    ("vulns", "SELECT COUNT(*) FROM vuln"),
                    ("steps", "SELECT COUNT(*) FROM attack_step"),
                    ("hits", "SELECT COUNT(*) FROM score_hit"),
                    ("webshells", "SELECT COUNT(*) FROM webshell"),
                    ("tunnels", "SELECT COUNT(*) FROM tunnel"),
                ):
                    try:
                        item[key] = c.execute(sql).fetchone()[0]
                    except sqlite3.Error:
                        item[key] = None
                try:
                    item["score"] = c.execute(
                        "SELECT COALESCE(SUM(COALESCE(h.points, p.points) * COALESCE(h.multiplier,1)),0)"
                        " FROM score_hit h JOIN score_point p ON p.id = h.point_id"
                        " WHERE COALESCE(h.self_created,0) = 0").fetchone()[0]
                except sqlite3.Error:
                    item["score"] = None
                c.close()
            except sqlite3.Error as exc:
                item["error"] = str(exc)
        out.append(item)
    return out


# ────────────────────────────────────────────────────────────── skill 库
def _parse_front_matter(text: str):
    meta, body = {}, text
    if text.startswith("---"):
        lines = text.split("\n")
        end = None
        for i in range(1, len(lines)):
            if lines[i].strip() == "---":
                end = i
                break
        if end is not None:
            for line in lines[1:end]:
                if line[:1] in (" ", "\t", "") or ":" not in line:
                    continue
                k, v = line.split(":", 1)
                v = v.strip()
                if len(v) >= 2 and v[0] == v[-1] and v[0] in "\"'":
                    v = v[1:-1]
                meta[k.strip()] = v
            body = "\n".join(lines[end + 1:]).lstrip("\n")
    return meta, body


def list_skills(role=None) -> list:
    out = []
    if not SKILLS.is_dir():
        return out
    for f in sorted(SKILLS.glob("*.md")):
        try:
            text = f.read_text(encoding="utf-8", errors="replace")
        except OSError as exc:
            log(f"[skill] 读不了 {f.name}: {exc}")
            continue
        meta, _ = _parse_front_matter(text)
        item = {
            "name": meta.get("name") or f.stem,
            "role": meta.get("role", ""),
            "description": meta.get("description", ""),
            "whenToUse": meta.get("whenToUse", ""),
            "enabled": meta.get("enabled", "true").strip().lower() != "false",
            "file": f.name,
            "path": str(f),
            "bytes": f.stat().st_size,
        }
        if role and item["role"].lower() != role.strip().lower():
            continue
        out.append(item)
    out.sort(key=lambda x: (x["role"] or "zzz", x["name"]))
    return out


def get_skill(name: str) -> dict:
    name = str(name or "").strip()
    if name.endswith(".md"):
        name = name[:-3]
    f = SKILLS / f"{name}.md"
    if not f.exists():
        cand = [p for p in SKILLS.glob("*.md") if p.stem.lower() == name.lower()]
        if not cand:
            return {}
        f = cand[0]
    text = f.read_text(encoding="utf-8", errors="replace")
    meta, body = _parse_front_matter(text)
    return {
        "name": meta.get("name") or f.stem,
        "role": meta.get("role", ""),
        "description": meta.get("description", ""),
        "whenToUse": meta.get("whenToUse", ""),
        "enabled": meta.get("enabled", "true").strip().lower() != "false",
        "path": str(f),
        "bytes": f.stat().st_size,
        "body": body,
    }


# ────────────────────────────────────────────────────────────── CLI 输出
def cmd_poc_search(args):
    rows = search_pocs(
        q=args.q, cve=args.cve, component=args.component, category=args.category,
        kind=args.kind, verified=args.verified, engagement=args.engagement,
        asset=args.asset, limit=args.limit,
    )
    if args.json:
        emit_json({"count": len(rows), "limit": clamp_limit(args.limit), "results": rows})
        return
    if not rows:
        print("（无命中）")
        return
    data = [[r["id"], r["code"], r["title"], r.get("kind") or "", r.get("severity") or "",
             "★" if r.get("verified") else "", r.get("hit_count") or 0] for r in rows]
    print(table(["id", "code", "title", "kind", "sev", "V", "hit"], data,
                caps=[6, 44, 46, 10, 9, 1, 4]))
    print(f"\n共 {len(rows)} 条（limit={clamp_limit(args.limit)}，上限 {MAX_LIMIT}）")


def cmd_poc_get(args):
    d = get_poc(poc_id=args.id, code=args.code)
    if not d:
        die("未找到该 POC")
    if args.json:
        emit_json(d)
        return
    print(f"# {d.get('title')}")
    print(f"id={d['id']}  code={d['code']}")
    print(f"kind={d.get('kind') or '-'}  category={d.get('category') or '-'}  "
          f"severity={d.get('severity') or '-'}  language={d.get('language') or '-'}  "
          f"verified={'YES' if d.get('verified') else 'no'}  hit_count={d.get('hit_count') or 0}")
    if d.get("component") or d.get("versions") or d.get("cve"):
        print(f"component={d.get('component') or '-'}  versions={d.get('versions') or '-'}  "
              f"cve={d.get('cve') or '-'}")
    if d.get("source") or d.get("source_url"):
        print(f"source={d.get('source') or '-'}  url={d.get('source_url') or '-'}")
    if d.get("engagement_name") or d.get("asset_target"):
        print(f"engagement={d.get('engagement_name') or d.get('engagement_id') or '-'}  "
              f"asset={d.get('asset_target') or '-'}")
    if d.get("tags"):
        print(f"tags={d['tags']}")
    if d.get("path"):
        print(f"path={d['path']}")
    if d.get("verified_note"):
        print(f"verified_note={d['verified_note']}")
    if d.get("description"):
        print(f"\n--- description ---\n{d['description']}")
    if d.get("usage"):
        print(f"\n--- usage ---\n{d['usage']}")
    print(f"\n--- content ({d.get('content_chars', 0)} chars) ---")
    print(d.get("content") or "（空）")


def cmd_poc_add(args):
    if args.stdin:
        raw = sys.stdin.read()
    else:
        raw = Path(args.json_file).read_text(encoding="utf-8")
    try:
        data = json.loads(raw)
    except json.JSONDecodeError as exc:
        die(f"--json-file/--stdin 不是合法 JSON: {exc}")
    if not isinstance(data, dict):
        die("JSON 顶层必须是对象")
    if args.engagement:
        data.setdefault("engagement", args.engagement)
    res = add_poc(data)
    if args.json:
        emit_json(res)
    else:
        print(f"{'已新增' if res['created'] else '已更新'} id={res['id']} code={res['code']}")
        if res.get("path"):
            print(f"path={res['path']}")


def cmd_poc_use(args):
    res = use_poc(poc_id=args.id, code=args.code, used_on=args.on)
    if not res:
        die("未找到该 POC")
    if args.json:
        emit_json(res)
    else:
        print(f"复用已记录：{res['code']} → {res['used_on']}（hit_count={res['hit_count']}）")


def cmd_poc_update(args):
    fields = {}
    if args.verified:
        fields["verified"] = 1
    if args.unverified:
        fields["verified"] = 0
    if args.note is not None:
        fields["verified_note"] = args.note
    if args.severity:
        fields["severity"] = args.severity
    if args.tags is not None:
        fields["tags"] = args.tags
    if args.description is not None:
        fields["description"] = args.description
    if args.usage is not None:
        fields["usage"] = args.usage
    if args.content_file:
        fields["content"] = Path(args.content_file).read_text(encoding="utf-8")
    res = update_poc(poc_id=args.id, code=args.code, fields=fields)
    if not res:
        die("未找到该 POC")
    if args.json:
        emit_json(res)
    else:
        print(f"已更新 id={res['id']} code={res['code']}：{', '.join(res['updated'])}")


def cmd_kb_stats(args):
    st = kb_stats()
    if args.json:
        emit_json(st)
        return
    print(f"知识库: {st['db']}  ({st['db_bytes'] / 1048576:.1f} MB)")
    print(f"总数 {st['total']}  POC —— 已验证 {st['verified']}，未验证 {st['unverified']}，"
          f"正文 {st['content_chars'] / 1048576:.1f} M 字符，有落盘文件 {st['with_file']}")
    print(f"最近更新: {st['last_updated']}")
    for title, key in (("category", "by_category"), ("kind", "by_kind"),
                       ("source", "by_source"), ("severity", "by_severity")):
        items = list(st[key].items())
        print(f"\n[{title}] " + "  ".join(f"{k}={v}" for k, v in items))
    if st["engagements"]:
        print(f"\n[靶标] {len(st['engagements'])} 个（资产/漏洞/链路/得分在各自 assets.db）")
        rows = [[e["slug"], e.get("assets", "-"), e.get("vulns", "-"),
                 e.get("steps", "-"), e.get("hits", "-"), e.get("score", "-")]
                for e in st["engagements"]]
        print(table(["slug", "资产", "漏洞", "步骤", "得分命中", "得分"], rows,
                    caps=[58, 6, 6, 6, 8, 8]))
    print(f"\n[skill] {st['skills']['count']} 个  {st['skills']['dir']}")


def cmd_skill_list(args):
    items = list_skills(role=args.role)
    if args.json:
        emit_json({"count": len(items), "role": args.role or "*", "skills": items})
        return
    rows = [[s["name"], s["role"] or "-", "on" if s["enabled"] else "off", s["description"]]
            for s in items]
    print(table(["name", "role", "en", "description"], rows, caps=[34, 12, 3, 76]))
    print(f"\n共 {len(items)} 个 skill" + (f"（role={args.role}）" if args.role else ""))


def cmd_skill_get(args):
    d = get_skill(args.name)
    if not d:
        die(f"未找到 skill: {args.name}")
    if args.json:
        emit_json(d)
        return
    print(f"# {d['name']}  (role={d['role'] or '-'}, {d['bytes']}B)")
    print(f"path={d['path']}")
    print(f"\n{d['body']}")


# ────────────────────────────────────────────────────────────── MCP server
def _schema(props, required=()):
    return {"type": "object", "properties": props, "required": list(required),
            "additionalProperties": False}


_P_STR = {"type": "string"}
_P_INT = {"type": "integer"}
_P_BOOL = {"type": "boolean"}

TOOLS = [
    {
        "name": "redteam_poc_search",
        "description": "【渗透任务的第一步：动手前必须调用】检索本地 POC 知识库 —— "
                       "14,649 条 POC/EXP + 14,133 个 nuclei 模板的全文索引，其中 84 条已在真实目标上验证通过。"
                       "凡涉及：Nday/1day 利用、组件漏洞（Shiro/Weblogic/Spring/泛微/致远/Nacos/Struts2…）、"
                       "Web 打点、SQLi、未授权访问、文件上传、反序列化、提权、内网横向、隧道、免杀 —— "
                       "先按组件名、CVE 号或漏洞类别搜一次。命中就用 redteam_poc_get 取全文照着做；"
                       "禁止不看库就手写 POC，禁止凭印象说某版本存在某个 CVE。返回空才转去互联网搜索。",
        "inputSchema": _schema({
            "q": {"type": "string", "description": "关键词：组件+版本、漏洞名、正文特征串"},
            "cve": {"type": "string", "description": "CVE / CNVD 编号"},
            "component": {"type": "string", "description": "组件/产品名，如 Shiro、Weblogic"},
            "category": {"type": "string", "description": "rce|deserialization|file-upload|sqli|unauthorized|auth-bypass|weak-password|ssrf|xxe|path-traversal|info-leak|privesc|tunnel|other"},
            "kind": {"type": "string", "description": "poc|exp|script|template|payload"},
            "verified": {"type": "boolean", "description": "只看实测验证过的"},
            "engagement": {"type": "string", "description": "按来源靶标过滤"},
            "asset": {"type": "string", "description": "按发现资产过滤（IP/域名/URL 片段）"},
            "limit": {"type": "integer", "description": "返回条数，默认 20，上限 200"},
        }),
    },
    {
        "name": "redteam_poc_get",
        "description": "【search 命中后必须紧接着调用】取一条 POC/EXP 的完整正文、用法命令、验证记录与落盘路径。"
                       "只看到 search 结果里的标题就去手写脚本，等于没用上知识库 —— 正文里通常写着该目标需要的参数、"
                       "绕过细节和坑。",
        "inputSchema": _schema({
            "id": {"type": "integer", "description": "知识库条目 id"},
            "code": {"type": "string", "description": "稳定标识 code（与 id 二选一）"},
        }),
    },
    {
        "name": "redteam_poc_add",
        "description": "【在目标上打出效果后回填】把通用可复用的 POC/EXP 写入知识库（同名 code 合并刷新）。"
                       "必须填 engagement（哪个靶标）+ asset_target（哪台资产）+ verified/verified_note（什么回显证明了它有效）。"
                       "没打通的、只是试过的脚本不要写进来。正文落盘到 pocs/<code>/<filename> 并自动回填 path。",
        "inputSchema": _schema({
            "title": {"type": "string", "description": "标题（必填）"},
            "code": {"type": "string", "description": "稳定标识，缺省由 title 生成"},
            "kind": {"type": "string", "description": "poc|exp|script|template|payload"},
            "category": {"type": "string", "description": "归类"},
            "cve": {"type": "string"}, "component": {"type": "string"},
            "versions": {"type": "string"}, "severity": {"type": "string"},
            "language": {"type": "string", "description": "python|go|java|bash|http|nuclei|js|php"},
            "source": {"type": "string", "description": "web|self|manual|nuclei-template"},
            "source_url": {"type": "string"}, "description": {"type": "string"},
            "usage": {"type": "string"}, "content": {"type": "string"},
            "path": {"type": "string"}, "verified": {"type": "boolean"},
            "verified_note": {"type": "string"},
            "engagement": {"type": "string", "description": "来源靶标（id 或名称）"},
            "asset_target": {"type": "string"}, "tags": {"type": "string"},
            "found_by_agent": {"type": "string"}, "created_by": {"type": "string"},
        }, ["title"]),
    },
    {
        "name": "redteam_poc_use",
        "description": "【用库里的 POC 打了目标就记一次】hit_count+1 并记录用在哪个目标，复用次数高的条目会被排到前面，"
                       "供后续任务优先选用。",
        "inputSchema": _schema({
            "id": _P_INT, "code": _P_STR,
            "used_on": {"type": "string", "description": "用在哪：靶标名 / 目标 IP / URL"},
        }, ["used_on"]),
    },
    {
        "name": "redteam_poc_update",
        "description": "【验证通过后必须回来置 verified=true】更新知识库条目：补验证结论与证据、修正影响版本/严重级/标签、补正文。"
                       "没置 verified 的条目后续不会被优先复用。",
        "inputSchema": _schema({
            "id": _P_INT, "code": _P_STR,
            "verified": _P_BOOL,
            "verified_note": {"type": "string", "description": "验证证据：哪台目标、什么回显"},
            "severity": _P_STR, "tags": _P_STR, "description": _P_STR,
            "usage": _P_STR, "content": _P_STR,
            "versions": _P_STR, "component": _P_STR, "cve": _P_STR,
        }),
    },
    {
        "name": "redteam_kb_stats",
        "description": "知识库概览：总数、已验证数、按 category/kind/source 分布、各靶标事实库规模。"
                       "不确定库里有什么可先看一眼，再决定搜什么。",
        "inputSchema": _schema({}),
    },
    {
        "name": "redteam_skill_list",
        "description": "列出本机 skill 库（125 个），可按角色过滤。",
        "inputSchema": _schema({
            "role": {"type": "string", "description": "plan|recon|assess|vuln-scan|exploit|internal"},
        }),
    },
]


def _split_engagement(val: str):
    """engagement 参数：纯数字 → engagement_id，否则 → engagement_name。"""
    if val is None:
        return None, None
    v = str(val).strip()
    if not v:
        return None, None
    return (v, None) if v.isdigit() else (None, v)


def mcp_tool_call(name: str, argv: dict) -> dict:
    argv = argv or {}

    if name == "redteam_poc_search":
        rows = search_pocs(
            q=argv.get("q"), cve=argv.get("cve"), component=argv.get("component"),
            category=argv.get("category"), kind=argv.get("kind"),
            verified=bool(argv.get("verified")), engagement=argv.get("engagement"),
            asset=argv.get("asset"), limit=argv.get("limit", DEFAULT_LIMIT),
        )
        return {"count": len(rows), "results": rows}

    if name == "redteam_poc_get":
        d = get_poc(poc_id=argv.get("id"), code=argv.get("code"))
        if not d:
            raise RuntimeError("未找到该 POC")
        return d

    if name == "redteam_poc_add":
        data = dict(argv)
        if data.get("engagement"):
            eid, ename = _split_engagement(data.pop("engagement"))
            if eid:
                data["engagement_id"] = eid
            if ename:
                data["engagement_name"] = ename
        return add_poc(data)

    if name == "redteam_poc_use":
        res = use_poc(poc_id=argv.get("id"), code=argv.get("code"),
                      used_on=argv.get("used_on", ""))
        if not res:
            raise RuntimeError("未找到该 POC")
        return res

    if name == "redteam_poc_update":
        fields = {k: argv.get(k) for k in
                  ("verified", "verified_note", "severity", "tags", "description",
                   "usage", "content", "versions", "component", "cve") if k in argv}
        res = update_poc(poc_id=argv.get("id"), code=argv.get("code"), fields=fields)
        if not res:
            raise RuntimeError("未找到该 POC")
        return res

    if name == "redteam_kb_stats":
        return kb_stats()

    if name == "redteam_skill_list":
        items = list_skills(role=argv.get("role"))
        return {"count": len(items), "role": argv.get("role") or "*", "skills": items}

    raise RuntimeError(f"未知工具: {name}")


def _send(obj) -> None:
    sys.stdout.write(json.dumps(obj, ensure_ascii=False) + "\n")
    sys.stdout.flush()


def _ok(mid, result):
    return {"jsonrpc": "2.0", "id": mid, "result": result}


def _err(mid, code, message):
    return {"jsonrpc": "2.0", "id": mid, "error": {"code": code, "message": message}}


def cmd_mcp(args):
    """stdio MCP：NDJSON，一行一帧；stdin EOF 退出；stdout 只有协议帧。"""
    log(f"rt mcp: {SERVER_NAME} v{VERSION} ready (kb={KB})")
    for raw in sys.stdin:
        line = raw.strip()
        if not line:
            continue
        try:
            msg = json.loads(line)
        except json.JSONDecodeError as exc:
            _send(_err(None, -32700, f"Parse error: {exc}"))
            continue
        if not isinstance(msg, dict):
            _send(_err(None, -32600, "Invalid Request"))
            continue

        mid = msg.get("id")
        method = msg.get("method")
        params = msg.get("params") or {}

        # 通知（无 id）一律不回帧
        if mid is None and isinstance(method, str) and method.startswith("notifications/"):
            continue
        if method == "notifications/initialized":
            continue

        try:
            if method == "initialize":
                _send(_ok(mid, {
                    "protocolVersion": PROTOCOL_VERSION,
                    "capabilities": {"tools": {"listChanged": False}},
                    "serverInfo": {"name": SERVER_NAME, "version": VERSION},
                }))
            elif method == "tools/list":
                _send(_ok(mid, {"tools": TOOLS}))
            elif method == "tools/call":
                tname = params.get("name")
                targs = params.get("arguments") or {}
                try:
                    payload = mcp_tool_call(str(tname), targs)
                    _send(_ok(mid, {
                        "content": [{"type": "text",
                                     "text": json.dumps(payload, ensure_ascii=False, default=str)}],
                        "isError": False,
                    }))
                except Exception as exc:  # 工具内部失败回 isError，不炸服务
                    _send(_ok(mid, {
                        "content": [{"type": "text", "text": f"ERROR: {exc}"}],
                        "isError": True,
                    }))
            elif method == "ping":
                _send(_ok(mid, {}))
            elif method in ("resources/list",):
                _send(_ok(mid, {"resources": []}))
            elif method in ("prompts/list",):
                _send(_ok(mid, {"prompts": []}))
            else:
                if mid is not None:
                    _send(_err(mid, -32601, f"Method not found: {method}"))
        except Exception as exc:  # 兜底：任何异常都要回一帧，不能静默
            if mid is not None:
                _send(_err(mid, -32603, f"Internal error: {exc}"))
    log("rt mcp: stdin closed, bye")


# ────────────────────────────────────────────────────────────── 入口
def build_parser() -> argparse.ArgumentParser:
    p = argparse.ArgumentParser(
        prog="rt.py",
        description="红队知识库引擎：POC 库 + skill 库检索，CLI 与 stdio MCP 双模。",
        epilog="数据真源：~/.dsh/redteam/knowledge.db（POC）与 engagements/<slug>/assets.db（资产/漏洞/链路/得分）",
    )
    p.add_argument("-V", "--version", action="version", version=f"rt.py {VERSION}")
    sub = p.add_subparsers(dest="cmd", metavar="<command>")

    # poc
    poc = sub.add_parser("poc", help="POC/EXP 知识库操作")
    poc_sub = poc.add_subparsers(dest="poc_cmd", metavar="<sub>")

    s = poc_sub.add_parser("search", help="检索 POC（FTS5 + 过滤，失败降级 LIKE）")
    s.add_argument("--q", help="关键词（组件/漏洞名/正文特征）")
    s.add_argument("--cve", help="CVE / CNVD 编号")
    s.add_argument("--component", help="组件/产品名")
    s.add_argument("--category", help="rce|sqli|unauthorized|auth-bypass|...")
    s.add_argument("--kind", help="poc|exp|script|template|payload")
    s.add_argument("--verified", action="store_true", help="只看已验证")
    s.add_argument("--engagement", help="按来源靶标过滤")
    s.add_argument("--asset", help="按发现资产过滤")
    s.add_argument("--limit", type=int, default=DEFAULT_LIMIT,
                   help=f"条数，默认 {DEFAULT_LIMIT}，上限 {MAX_LIMIT}")
    s.add_argument("--json", action="store_true", help="纯 JSON 输出")
    s.set_defaults(func=cmd_poc_search)

    g = poc_sub.add_parser("get", help="取完整内容")
    g.add_argument("--id", type=int)
    g.add_argument("--code")
    g.add_argument("--json", action="store_true")
    g.set_defaults(func=cmd_poc_get)

    a = poc_sub.add_parser("add", help="新增/合并写入（同名 code 刷新）")
    src = a.add_mutually_exclusive_group(required=True)
    src.add_argument("--json-file", help="JSON 文件路径")
    src.add_argument("--stdin", action="store_true", help="从 stdin 读 JSON")
    a.add_argument("--engagement", help="来源靶标（覆盖 JSON 内同名字段）")
    a.add_argument("--json", action="store_true")
    a.set_defaults(func=cmd_poc_add)

    u = poc_sub.add_parser("use", help="记一次复用")
    u.add_argument("--id", type=int)
    u.add_argument("--code")
    u.add_argument("--on", required=True, metavar="TARGET", help="用在哪：靶标/目标 IP/URL")
    u.add_argument("--json", action="store_true")
    u.set_defaults(func=cmd_poc_use)

    up = poc_sub.add_parser("update", help="补验证结论/严重级/标签")
    up.add_argument("--id", type=int)
    up.add_argument("--code")
    up.add_argument("--verified", action="store_true", help="置为已验证")
    up.add_argument("--unverified", action="store_true", help="置为未验证")
    up.add_argument("--note", metavar="TEXT", help="验证证据 → verified_note")
    up.add_argument("--severity")
    up.add_argument("--tags")
    up.add_argument("--description")
    up.add_argument("--usage")
    up.add_argument("--content-file", metavar="F", help="用文件内容覆盖正文")
    up.add_argument("--json", action="store_true")
    up.set_defaults(func=cmd_poc_update)

    # kb
    kb = sub.add_parser("kb", help="知识库概览")
    kb_sub = kb.add_subparsers(dest="kb_cmd", metavar="<sub>")
    ks = kb_sub.add_parser("stats", help="总数 / verified / 分布 / 各靶标事实量")
    ks.add_argument("--json", action="store_true")
    ks.set_defaults(func=cmd_kb_stats)

    # skill
    sk = sub.add_parser("skill", help="skill 库")
    sk_sub = sk.add_subparsers(dest="skill_cmd", metavar="<sub>")
    sl = sk_sub.add_parser("list", help="列出 skill（可按 role 过滤）")
    sl.add_argument("--role")
    sl.add_argument("--json", action="store_true")
    sl.set_defaults(func=cmd_skill_list)
    sg = sk_sub.add_parser("get", help="打印 skill 正文")
    sg.add_argument("name", metavar="NAME")
    sg.add_argument("--json", action="store_true")
    sg.set_defaults(func=cmd_skill_get)

    # mcp
    m = sub.add_parser("mcp", help="stdio MCP server（JSON-RPC 2.0 over NDJSON）")
    m.set_defaults(func=cmd_mcp)

    return p


def main() -> None:
    parser = build_parser()
    args = parser.parse_args()
    if not getattr(args, "func", None):
        parser.print_help()
        return
    try:
        args.func(args)
    except KeyboardInterrupt:
        log("rt: interrupted")
        raise SystemExit(130)
    except BrokenPipeError:
        try:
            sys.stdout.close()
        except Exception:
            pass
        raise SystemExit(0)


if __name__ == "__main__":
    main()
