/**
 * RTIS Bridge —— 让红队插件（工具面 / 角色 / 面板）真正用上 RTIS 知识层
 *
 * 背景（为什么需要这一层）：
 *   RTIS 在 knowledge.db 里建了 6 张知识表（k_meta / k_negative / k_relation /
 *   k_event / k_proposal / k_version）并回填了真实元数据，但生产插件对它**一无所知**：
 *   grep 全部插件源码，k_* 表零命中。表现为三处断点：
 *     ① redteam_poc_search 仍走「空白切词 → 逐词加双引号 → AND → FTS」，中文多词查询
 *        整串当一个 token，命中率 0（基线实测 Recall@5 = 3/8）；
 *     ② 排序仍是 verified DESC, hit_count DESC, updated_at DESC，**没有相关性打分**，
 *        K2/K3 证据等级与 lifecycle 不进排序；
 *     ③ 检索过程不落任何事件，opened/selected/applied 全无观测。
 *   本模块就是这三个断点的桥：**只做加法，不改既有行为**——
 *   数据库缺 k_* 表时全部方法优雅退化（available() === false），插件行为与升级前一致。
 *
 * 设计约束（与 rtis/design/Knowledge_Intelligence_Design.md §2 一致）：
 *   · 分词：ASCII 词元（保 hex2bin / _tpl_ / x-amz-cf-pop 整体，同时拆子片段）+ CJK 2-gram；
 *   · 排序：text(bm25 饱和 × 组覆盖率) 主 + verified 弱偏置 + 证据等级 + 时效；
 *   · 组覆盖率门禁：词组 token 命中 ≥ ceil(0.6·n) 才算命中，防「长中文概念只蹭到一个通用 2-gram」；
 *   · 全程只读 + 只往 k_event 追加事件，不改 poc / 不删条目 / 不动 skills。
 *
 * 这个文件不依赖 Cordis，可被 store-core.js、bin/cli.mjs 与离线脚本直接复用。
 */
import { DatabaseSync } from 'node:sqlite'
import { existsSync, readFileSync, readdirSync } from 'node:fs'
import { join, basename } from 'node:path'

/* ── 权重（与 rtis/tools/kb_search_v2.py 同源；改这里必须同步跑 bench 回归）──── */
const SAT = 4.0            /* bm25 饱和归一化分母: x/(x+SAT) */
const GROUP_MATCH_RATIO = 0.6
/* 低于此相关性直接丢弃（负样本门禁）；可用 RTIS_MIN_TEXT 覆盖以便调参回归 */
const MIN_TEXT = Number(process.env.RTIS_MIN_TEXT || 0.48)
const W = {
  text: 0.55,              /* 相关性主体（离线脚本 0.35 + 原 context 权重合并） */
  verified: 0.10,
  evidence: 0.05,
  recency: 0.10,
  stale: -0.05,
}
const FIELD_W = {
  title: 3.0, code: 2.0, cve: 3.0, component: 2.0, tags: 2.0,
  versions: 1.0, description: 2.0, usage: 1.0, content: 1.0,
}
const K_SCORE = { K0: 0.0, K1: 0.25, K2: 0.5, K3: 0.8, K4: 1.0 }

/* ── 分词 ──────────────────────────────────────────────────────────────── */
const WORD_RE = /[a-z0-9_-]*[a-z0-9][a-z0-9_-]*/g
const CJK_RE = /[\u4e00-\u9fff]+/g

export function hasCjk(text) {
  return /[\u4e00-\u9fff]/.test(String(text || ''))
}

/** 词干：去掉标识符两端的 - / 单侧 _（`x-amz-cf-id` ≈ `amz`；但 `_tpl_` 这种双下划线包裹的
 *  cookie 名是**精确标识符**，不参与归并——否则会退化成搜 `tpl`，召回一堆模板描述）。 */
function stem(tok) {
  const t = String(tok)
  if (/^_.*_$/.test(t) && t.length > 2) return t
  return t.replace(/^[-]+/, '').replace(/[-]+$/, '').replace(/^_(?=[a-z0-9])/, '').replace(/(?<=[a-z0-9])_$/, '')
}

/** 把 token 归并成「概念组」：同 stem 的不同写法只算一个组，防覆盖率被同义词稀释。 */
function groupTokens(tokens) {
  const groups = new Map()
  for (const t of tokens) {
    const k = stem(t) || t
    const g = groups.get(k)
    if (g === undefined) groups.set(k, [t])
    else if (!g.includes(t)) g.push(t)
  }
  return groups
}

function subtokens(word) {
  const out = new Set([word])
  for (const part of word.split(/[-_]+/)) if (part.length >= 2) out.add(part)
  return out
}

/** 查询与文档共用同一套分词，保证「文档怎么切，查询就怎么切」。 */
export function tokenize(text) {
  const low = String(text || '').toLowerCase()
  if (low === '') return []
  const toks = []
  for (const w of low.match(WORD_RE) || []) toks.push(...subtokens(w))
  for (const run of low.match(CJK_RE) || []) {
    if (run.length === 1) toks.push(run)
    else for (let i = 0; i < run.length - 1; i += 1) toks.push(run.slice(i, i + 2))
  }
  return toks
}

/* ── 小工具 ────────────────────────────────────────────────────────────── */
function nowIso() { return new Date().toISOString().replace(/\.\d{3}Z$/, 'Z') }
function daysSince(iso) {
  if (!iso) return 9999
  const t = Date.parse(iso)
  if (!Number.isFinite(t)) return 9999
  return (Date.now() - t) / 86400000
}
function recencyScore(iso) {
  const d = daysSince(iso)
  if (d <= 180) return 1
  if (d >= 730) return 0
  return 1 - (d - 180) / 550
}

/* ── 表探测 ────────────────────────────────────────────────────────────── */
const RTIS_TABLES = ['k_meta', 'k_negative', 'k_relation', 'k_event', 'k_version']

/** 一个 knowledge.db 对应一个 bridge 实例（进程内复用，避免重复探表与重复准备语句）。 */
const INSTANCES = new Map()

export function rtisBridgeFor(knowledgePath) {
  const key = String(knowledgePath || '')
  let b = INSTANCES.get(key)
  if (b === undefined) { b = new RtisBridge(key); INSTANCES.set(key, b) }
  return b
}

export class RtisBridge {
  constructor(knowledgePath) {
    this.path = knowledgePath
    this._db = null
    this._schema = null
    this._schemaAt = 0
  }

  db() {
    if (this._db !== null) return this._db
    if (!this.path || !existsSync(this.path)) return null
    try { this._db = new DatabaseSync(this.path) } catch { this._db = null }
    return this._db
  }

  /** k_* 表是否齐（30s 缓存：插件进程长活，建表可能发生在运行期）。 */
  available() {
    const now = Date.now()
    if (this._schema !== null && now - this._schemaAt < 30000) return this._schema
    const db = this.db()
    let ok = false
    if (db !== null) {
      try {
        const rows = db.prepare("SELECT name FROM sqlite_master WHERE type='table' AND name LIKE 'k\\_%' ESCAPE '\\'").all()
        const have = new Set(rows.map((r) => r.name))
        ok = RTIS_TABLES.every((t) => have.has(t))
      } catch { ok = false }
    }
    this._schema = ok
    this._schemaAt = now
    return ok
  }

  metaMap(ids) {
    const out = new Map()
    if (!this.available() || !ids || ids.length === 0) return out
    const db = this.db()
    for (let i = 0; i < ids.length; i += 500) {
      const chunk = ids.slice(i, i + 500).filter((x) => Number.isFinite(Number(x)))
      if (chunk.length === 0) continue
      try {
        const rows = db.prepare(`SELECT * FROM k_meta WHERE knowledge_id IN (${chunk.map(() => '?').join(',')})`).all(...chunk)
        for (const r of rows) out.set(Number(r.knowledge_id), r)
      } catch { /* 表结构异常时静默退化 */ }
    }
    return out
  }

  negativeIds() {
    const set = new Set()
    if (!this.available()) return set
    try {
      for (const r of this.db().prepare('SELECT DISTINCT knowledge_id FROM k_negative').all()) set.add(Number(r.knowledge_id))
    } catch { /* noop */ }
    return set
  }

  /**
   * 把 poc 行附上 RTIS 元数据；不改原行字段（只在 `rtis` 下加一层），
   * 这样既有调用方（面板、报告、CLI）读到的字段一个不少。
   */
  annotate(rows) {
    if (!Array.isArray(rows) || rows.length === 0) return rows
    const metas = this.metaMap(rows.map((r) => r.id))
    const negs = this.negativeIds()
    for (const r of rows) {
      const m = metas.get(Number(r.id))
      r.rtis = m === undefined
        ? { known: false }
        : {
            known: true,
            type: m.normalized_type,
            evidence_level: m.evidence_level,
            lifecycle: m.lifecycle,
            archetypes: safeJson(m.archetypes, []),
            technology_tags: safeJson(m.technology_tags, []),
            method_tags: safeJson(m.method_tags, []),
            basis: m.verification_basis,
            has_negative: negs.has(Number(r.id)),
          }
    }
    return rows
  }

  /* ── 候选召回 ─────────────────────────────────────────────────────────── */
  _candidates(tokens, cap = 400) {
    const db = this.db()
    const ids = new Set()
    if (db === null || tokens.length === 0) return ids
    const like = (t) => '%' + t.replace(/[%_]/g, (c) => '\\' + c) + '%'
    const fields = ['title', 'code', 'cve', 'component', 'versions', 'tags', 'description', 'usage']
    const sqlOne = `SELECT id FROM poc WHERE ${fields.map((f) => `${f} LIKE ? ESCAPE '\\'`).join(' OR ')} LIMIT ${cap}`
    let stmt
    try { stmt = db.prepare(sqlOne) } catch { return ids }
    for (const t of tokens) {
      if (t.length < 2 && !hasCjk(t)) continue
      try {
        for (const r of stmt.all(...fields.map(() => like(t)))) ids.add(Number(r.id))
      } catch { /* 单 token 失败不影响其它 */ }
      if (ids.size >= cap * 2) break
    }
    /* content 只在上面没召回时才补扫（内容列 588KB，单独走一次） */
    if (ids.size === 0 && tokens.length > 0) {
      try {
        const s2 = db.prepare(`SELECT id FROM poc WHERE content LIKE ? ESCAPE '\\' LIMIT ${cap}`)
        for (const t of tokens) for (const r of s2.all(like(t))) ids.add(Number(r.id))
      } catch { /* noop */ }
    }
    return ids
  }

  _rows(ids) {
    const db = this.db()
    const out = []
    const list = [...ids]
    for (let i = 0; i < list.length; i += 400) {
      const chunk = list.slice(i, i + 400)
      if (chunk.length === 0) continue
      try {
        const rows = db.prepare(`SELECT id, code, title, kind, category, cve, component, versions, severity,
            language, source, source_url, description, usage, path, verified, verified_note, hit_count, used_on, tags,
            engagement_id, engagement_name, asset_target, found_by_agent, created_by, created_at, updated_at,
            LENGTH(COALESCE(content, '')) AS content_bytes,
            CASE WHEN COALESCE(content, '') = '' THEN 0 ELSE 1 END AS has_content
          FROM poc WHERE id IN (${chunk.map(() => '?').join(',')})`).all(...chunk)
        out.push(...rows)
      } catch { /* noop */ }
    }
    return out
  }

  _contentOf(id) {
    try {
      const r = this.db().prepare('SELECT COALESCE(content, \'\') AS c, path FROM poc WHERE id = ?').get(Number(id))
      if (r === undefined) return ''
      if (r.c !== '') return r.c
      if (r.path && existsSync(r.path)) return readFileSync(r.path, 'utf8')
    } catch { /* noop */ }
    return ''
  }

  /* ── 打分 ─────────────────────────────────────────────────────────────── */
  scoreRow(row, tokens, metas) {
    const fields = {
      title: row.title, code: row.code, cve: row.cve, component: row.component,
      versions: row.versions, tags: row.tags, description: row.description, usage: row.usage,
    }
    const groups = groupTokens(tokens)
    let matchedGroups = 0
    let weighted = 0
    let content = null
    for (const variants of groups.values()) {
      let best = 0
      for (const t of variants) {
        let hit = 0
        for (const [f, w] of Object.entries(FIELD_W)) {
          if (f === 'content') continue
          const v = fields[f]
          if (!v) continue
          if (String(v).toLowerCase().includes(t)) hit += w
        }
        if (hit === 0 && row.has_content === 1) {
          if (content === null) content = this._contentOf(row.id).toLowerCase()
          if (content.includes(t)) hit += FIELD_W.content * 3   /* 正文命中一次抵三次，鼓励「完整方法」 */
        }
        if (hit > best) best = hit
      }
      if (best > 0) matchedGroups += 1
      weighted += Math.min(best, 9)
    }
    const coverage = groups.size === 0 ? 0 : matchedGroups / groups.size
    if (matchedGroups < Math.max(1, Math.ceil(groups.size * GROUP_MATCH_RATIO))) return null
    const textScore = (weighted / (weighted + SAT)) * coverage
    if (textScore < MIN_TEXT) return null
    const m = metas.get(Number(row.id))
    const evidence = m ? (K_SCORE[m.evidence_level] ?? 0) : (row.verified === 1 ? 0.8 : 0)
    const total = W.text * textScore
      + W.verified * (row.verified === 1 ? 1 : 0)
      + W.evidence * evidence
      + W.recency * recencyScore(row.updated_at || row.created_at)
      + (row.verified !== 1 && daysSince(row.updated_at) > 365 ? W.stale : 0)
    return { total: Number(total.toFixed(4)), text: Number(textScore.toFixed(4)), coverage: Number(coverage.toFixed(2)) }
  }

  /** 主检索入口：返回带 rtis 注解与 `score` 的 poc 行（按相关性排序）。 */
  search(text, opts = {}) {
    const limit = Math.min(Number(opts.limit) || 20, 200)
    const q = String(text || '').trim()
    if (q === '') return { ok: false, error: 'q 为空', items: [] }
    if (!this.available()) return { ok: false, error: 'rtis 未启用（k_* 表不存在）', items: [] }
    const tokens = [...new Set(tokenize(q))].filter((t) => t.length >= 2 || hasCjk(t))
    if (tokens.length === 0) return { ok: true, items: [], tokens: [] }
    const wanted = String(opts.archetype || '').trim().toLowerCase()
    const typeWanted = String(opts.type || '').trim().toLowerCase()
    const cand = this._candidates(tokens)
    if (cand.size === 0) return { ok: true, items: [], tokens }
    const rows = this._rows(cand)
    const metas = this.metaMap(rows.map((r) => r.id))
    const scored = []
    for (const r of rows) {
      if (wanted !== '') {
        const m = metas.get(Number(r.id))
        const arch = m ? safeJson(m.archetypes, []).join(' ').toLowerCase() : ''
        if (!arch.includes(wanted)) continue
      }
      if (typeWanted !== '') {
        const m = metas.get(Number(r.id))
        if (!m || String(m.normalized_type || '').toLowerCase() !== typeWanted) continue
      }
      const s = this.scoreRow(r, tokens, metas)
      if (s === null) continue
      scored.push({ ...r, score: s.total, rtis: metaOf(metas.get(Number(r.id)), this.negativeIds()) })
    }
    scored.sort((a, b) => b.score - a.score || (b.verified === 1) - (a.verified === 1) || (b.hit_count || 0) - (a.hit_count || 0))
    return { ok: true, tokens, total_candidates: cand.size, items: scored.slice(0, limit) }
  }

  /* ── 关系 / 反知识 / 站型视图 ─────────────────────────────────────────── */
  relations(key) {
    if (!this.available()) return { ok: false, error: 'rtis 未启用', edges: [] }
    const code = String(key)
    const edges = []
    try {
      const rows = this.db().prepare('SELECT src, dst, type, confidence, note FROM k_relation WHERE src = ? OR dst = ? ORDER BY confidence DESC').all(code, code)
      edges.push(...rows)
    } catch { /* noop */ }
    return { ok: true, key: code, edges }
  }

  negatives(key) {
    if (!this.available()) return { ok: false, error: 'rtis 未启用', items: [] }
    const db = this.db()
    const out = []
    try {
      const row = db.prepare('SELECT id FROM poc WHERE code = ?').get(String(key))
      const id = row !== undefined ? Number(row.id) : Number(key)
      if (Number.isFinite(id)) {
        out.push(...db.prepare('SELECT * FROM k_negative WHERE knowledge_id = ?').all(id))
      }
      if (out.length === 0) {
        out.push(...db.prepare('SELECT * FROM k_negative WHERE knowledge_id IN (SELECT knowledge_id FROM k_meta WHERE method_tags LIKE ? OR technology_tags LIKE ?)').all('%' + key + '%', '%' + key + '%'))
      }
    } catch { /* noop */ }
    return { ok: true, key: String(key), items: out }
  }

  byArchetype(name, limit = 30) {
    if (!this.available()) return { ok: false, error: 'rtis 未启用', items: [] }
    const want = String(name || '').trim().toLowerCase()
    if (want === '') return { ok: false, error: 'archetype 为空', items: [] }
    const db = this.db()
    let metas = []
    try { metas = db.prepare('SELECT * FROM k_meta').all() } catch { metas = [] }
    const ids = metas.filter((m) => safeJson(m.archetypes, []).join(' ').toLowerCase().includes(want)).map((m) => Number(m.knowledge_id))
    const out = []
    const negs = this.negativeIds()
    for (const id of ids) {
      try {
        const r = db.prepare(`SELECT id, code, title, kind, category, component, verified, verified_note, hit_count, tags FROM poc WHERE id = ?`).get(id)
        if (r === undefined) continue
        out.push({ ...r, rtis: metaOf(metas.find((m) => Number(m.knowledge_id) === id), negs) })
      } catch { /* noop */ }
    }
    out.sort((a, b) => (b.rtis.evidence_level || '').localeCompare(a.rtis.evidence_level || '') || (b.hit_count || 0) - (a.hit_count || 0))
    return { ok: true, archetype: want, count: out.length, items: out.slice(0, limit) }
  }

  stats() {
    const out = { available: this.available(), path: this.path }
    if (!out.available) return out
    const db = this.db()
    const q = (sql) => { try { return db.prepare(sql).all() } catch { return [] } }
    out.by_type = q('SELECT normalized_type AS k, COUNT(*) AS n FROM k_meta GROUP BY 1 ORDER BY n DESC')
    out.by_level = q('SELECT evidence_level AS k, COUNT(*) AS n FROM k_meta GROUP BY 1 ORDER BY 1')
    out.by_lifecycle = q('SELECT lifecycle AS k, COUNT(*) AS n FROM k_meta GROUP BY 1 ORDER BY n DESC')
    out.negatives = q('SELECT COUNT(*) AS n FROM k_negative')[0] ?? { n: 0 }
    out.relations = q('SELECT type AS k, COUNT(*) AS n FROM k_relation GROUP BY 1 ORDER BY n DESC')
    out.events = q('SELECT event AS k, COUNT(*) AS n FROM k_event GROUP BY 1 ORDER BY n DESC')
    out.proposals = q("SELECT status AS k, COUNT(*) AS n FROM k_proposal GROUP BY 1")
    out.orphans = q(`SELECT COUNT(*) AS n FROM k_meta m LEFT JOIN poc p ON p.id = m.knowledge_id WHERE p.id IS NULL`)[0] ?? { n: 0 }
    return out
  }

  /* ── 事件链（补上原本完全没有的观测）───────────────────────────────────── */
  logEvent(runId, event, knowledgeId, payload) {
    if (!this.available()) return { ok: false }
    const allowed = ['search', 'returned', 'opened', 'selected', 'applied', 'outcome', 'rejected']
    if (!allowed.includes(event)) return { ok: false, error: 'event 非法: ' + event }
    try {
      const db = this.db()
      let json = null
      if (payload !== undefined && payload !== null) {
        try { json = JSON.stringify(payload) } catch { json = null }
        if (json !== null && json.length > 4000) json = json.slice(0, 4000)
      }
      db.prepare('INSERT INTO k_event (run_id, event, knowledge_id, at, payload_json) VALUES (?,?,?,?,?)')
        .run(String(runId || 'auto'), event, knowledgeId === undefined ? null : Number(knowledgeId), nowIso(), json)
      return { ok: true }
    } catch (e) { return { ok: false, error: String(e && e.message ? e.message : e) } }
  }

  trace(runId, limit = 200) {
    if (!this.available()) return { ok: false, error: 'rtis 未启用', items: [] }
    try {
      const items = this.db().prepare('SELECT id, run_id, event, knowledge_id, at, payload_json FROM k_event WHERE run_id = ? ORDER BY id ASC LIMIT ?').all(String(runId), Math.min(Number(limit) || 200, 1000))
      const counts = {}
      for (const it of items) counts[it.event] = (counts[it.event] || 0) + 1
      return { ok: true, run_id: String(runId), counts, items }
    } catch (e) { return { ok: false, error: String(e && e.message ? e.message : e), items: [] } }
  }

  /* ── 回填：本地规则（只有 k_meta 没有 poc 正文的孤儿）────────────────────── */
  /**
   * 把 knowledge/<file>.md 的本地规则补成 poc 行（保留号段 900001+）。
   * 幂等：已存在的 code/id 直接跳过。默认 dryRun，只有显式 apply 才写。
   * 不设 verified=1（避免污染「已验证 92 条」口径），证据等级由 k_meta 承载。
   */
  backfillLocalRules(knowledgeDir, opts = {}) {
    const apply = opts.apply === true
    if (!this.available()) return { ok: false, error: 'rtis 未启用' }
    if (!knowledgeDir || !existsSync(knowledgeDir)) return { ok: false, error: 'knowledge 目录不存在: ' + knowledgeDir }
    const db = this.db()
    const files = readdirSync(knowledgeDir).filter((f) => f.endsWith('.md')).sort()
    const plan = []
    for (const f of files) {
      const full = join(knowledgeDir, f)
      const text = readFileSync(full, 'utf8')
      const fm = parseFrontmatter(text)
      const code = fm.code || basename(f, '.md')
      const meta = db.prepare('SELECT * FROM k_meta WHERE knowledge_id IN (SELECT knowledge_id FROM k_meta WHERE dedup_key = ? OR method_tags LIKE ?)').all(code, '%' + code + '%')
      plan.push({
        file: f, path: full, code,
        title: fm.title || basename(f, '.md'),
        kind: 'rule',
        category: categoryOf(code),
        description: fm.description || firstParagraph(text),
        tags: fm.tags || '',
        content: text,
        meta_hit: meta.length,
      })
    }
    if (!apply) return { ok: true, dry_run: true, count: plan.length, items: plan.map((p) => ({ code: p.code, title: p.title, category: p.category, file: p.file })) }
    let inserted = 0
    const skipped = []
    for (const p of plan) {
      const exists = db.prepare('SELECT id FROM poc WHERE code = ?').get(p.code)
      if (exists !== undefined) { skipped.push(p.code); continue }
      const idRow = db.prepare('SELECT MAX(id) AS m FROM poc WHERE id BETWEEN 900001 AND 900999').get()
      const nextId = Math.max(900000, Number(idRow && idRow.m) || 900000) + 1
      try {
        db.prepare(`INSERT INTO poc (id, code, title, kind, category, component, language, source,
            description, content, path, verified, hit_count, tags, created_by, created_at, updated_at)
          VALUES (?,?,?,?,?,?,?,?,?,?,?,0,0,?,?,?,?)`)
          .run(nextId, p.code, p.title, p.kind, p.category, 'RTIS 本地规则', 'markdown', 'kb',
            p.description, p.content, p.path, p.tags, 'rtis-bridge', nowIso(), nowIso())
        inserted += 1
      } catch (e) { skipped.push(p.code + ':' + String(e && e.message ? e.message : e)) }
    }
    return { ok: true, dry_run: false, inserted, skipped, planned: plan.length }
  }
}

/* ── 杂项 ──────────────────────────────────────────────────────────────── */
function safeJson(s, dflt) {
  if (s === null || s === undefined || s === '') return dflt
  try {
    const v = JSON.parse(s)
    return Array.isArray(v) ? v : dflt
  } catch { return dflt }
}

function metaOf(m, negs) {
  if (m === undefined || m === null) return { known: false }
  return {
    known: true,
    type: m.normalized_type,
    evidence_level: m.evidence_level,
    lifecycle: m.lifecycle,
    archetypes: safeJson(m.archetypes, []),
    technology_tags: safeJson(m.technology_tags, []),
    method_tags: safeJson(m.method_tags, []),
    basis: m.verification_basis,
    has_negative: negs ? negs.has(Number(m.knowledge_id)) : false,
  }
}

function parseFrontmatter(text) {
  const out = {}
  const m = /^---\r?\n([\s\S]*?)\r?\n---/.exec(String(text || ''))
  if (m === null) return out
  for (const line of m[1].split(/\r?\n/)) {
    const i = line.indexOf(':')
    if (i <= 0) continue
    out[line.slice(0, i).trim()] = line.slice(i + 1).trim().replace(/^["']|["']$/g, '').replace(/^\[|\]$/g, '')
  }
  return out
}

function firstParagraph(text) {
  const body = String(text || '').replace(/^---[\s\S]*?---/, '').split(/\r?\n\r?\n/)
    .map((s) => s.trim()).filter((s) => s !== '' && !s.startsWith('#') && !s.startsWith('|'))
  return (body[0] || '').slice(0, 300)
}

function categoryOf(code) {
  if (/openapi|proxy-service/.test(code)) return 'unauthorized'
  if (/encryption|file-stream|edge-vs-origin|js-config/.test(code)) return 'info-leak'
  return 'other'
}

export { tokenize as rtisTokenize }
