#!/usr/bin/env node
/**
 * RTIS 知识桥（stdin JSON → stdout JSON）
 *
 * 为什么要有这一层：
 *   知识层唯一引擎是 dsh-purge-community/lib/redteam/rtis-bridge.js（分词/打分/门禁/事件）。
 *   rtis.py（编排层）是 Python，不能 import 它。**但绝不能复制一份算法**——复制即分叉，
 *   一旦两边权重漂移，同一句查询在编排层和工具面给出的排序会不一致，回归测试也就失去意义。
 *   所以这里只做一件事：按 mtime 动态 import 那一份 rtis-bridge.js，把调用转过去。
 *
 * 调用：
 *   echo '{"cmd":"search","q":"加密响应 明文旁路","limit":10}' | node rtis_kb.mjs
 *   echo '{"cmd":"event","run":"task-xxx","event":"selected","id":900003}' | node rtis_kb.mjs
 *
 * 只读保证：
 *   search/relations/negatives/archetype/stats/content 全部只读；
 *   唯一的写是 event → k_event（append-only 遥测表），绝不碰 poc / hit_count / k_meta。
 */
import { DatabaseSync } from 'node:sqlite'
import { existsSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import { dirname, join } from 'node:path'

/* 2026-11-09 解耦：默认取本目录同级的 rtis-bridge.js / knowledge.db（RIP kit 自包含）。 */
const HERE = dirname(fileURLToPath(import.meta.url))
const BRIDGE = process.env.RTIS_BRIDGE || join(HERE, 'rtis-bridge.js')
const KB = process.env.RTIS_KB || join(HERE, 'knowledge.db')

function readStdin() {
  return new Promise((resolve, reject) => {
    let buf = ''
    process.stdin.setEncoding('utf8')
    process.stdin.on('data', (c) => { buf += c })
    process.stdin.on('end', () => resolve(buf))
    process.stdin.on('error', reject)
    setTimeout(() => resolve(buf), 15000).unref?.()
  })
}

function fail(code, message) {
  process.stdout.write(JSON.stringify({ ok: false, code, error: message }))
  process.exit(0)
}

const raw = await readStdin()
let input
try { input = JSON.parse(raw || '{}') } catch (e) { fail('bad_json', String(e && e.message)) }

if (!existsSync(KB)) fail('no_kb', `知识库不存在: ${KB}`)
if (!existsSync(BRIDGE)) fail('no_bridge', `引擎不存在: ${BRIDGE}`)

const { rtisBridgeFor } = await import(BRIDGE)
const bridge = rtisBridgeFor(KB)
if (!bridge.available()) fail('rtis_unavailable', 'k_* 表不存在')

const cmd = String(input.cmd || 'search')
const out = { ok: true, cmd, engine: BRIDGE }

try {
  if (cmd === 'search') {
    const r = bridge.search(String(input.q || ''), {
      limit: Number(input.limit) || 20,
      archetype: input.archetype || '',
      type: input.type || '',
    })
    out.result = r
  } else if (cmd === 'relations') {
    out.result = bridge.relations(String(input.key || ''))
  } else if (cmd === 'negatives') {
    out.result = bridge.negatives(String(input.key || ''))
  } else if (cmd === 'archetype') {
    out.result = bridge.byArchetype(String(input.archetype || ''), Number(input.limit) || 30)
  } else if (cmd === 'stats') {
    out.result = bridge.stats()
  } else if (cmd === 'trace') {
    out.result = bridge.trace(String(input.run || ''), Number(input.limit) || 200)
  } else if (cmd === 'event') {
    const run = String(input.run || '').trim()
    const ev = String(input.event || '').trim()
    if (!run || !ev) fail('bad_args', 'event 需要 run 与 event')
    bridge.logEvent(run, ev, input.id ?? null, input.payload || {})
    out.result = { logged: true, run, event: ev, id: input.id ?? null }
  } else if (cmd === 'content') {
    const ids = Array.isArray(input.ids) ? input.ids.map(Number).filter(Number.isFinite) : []
    if (ids.length === 0) fail('bad_args', 'content 需要 ids 数组')
    const db = new DatabaseSync(KB, { readOnly: true })
    const rows = []
    const stmt = db.prepare('SELECT id, code, title, kind, category, cve, component, severity, verified, verified_note, description, usage, content, tags FROM poc WHERE id = ?')
    for (const id of ids) {
      const r = stmt.get(id)
      if (r) rows.push(r)
    }
    db.close()
    out.result = rows
  } else if (cmd === 'meta') {
    const meta = bridge.metaMap(Array.isArray(input.ids) ? input.ids.map(Number) : [])
    const arr = []
    for (const [k, v] of meta) arr.push({ id: Number(k), ...v })
    out.result = arr
  } else {
    fail('unknown_cmd', cmd)
  }
} catch (e) {
  fail('engine_error', String((e && e.stack) || e))
}

process.stdout.write(JSON.stringify(out))
