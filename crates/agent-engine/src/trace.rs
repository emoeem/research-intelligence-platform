//! P7-5 统一只读关联查询（SESSION-HANDOFF §十追加项 1：按任一 ID 关联出
//! 任务、意图、上下文、工具、证据全链）。
//!
//! 定位（边界如实声明）：
//! - 纯只读 SELECT，不写任何行；调用方（rp-agent `trace` 子命令、测试、
//!   后续 /api 面）拿 [`TraceChain`] 自行序列化。
//! - 关联拓扑 = 迁移 015 列 + 既有 FK：trace_id 五面列（agent_run/tool_run/
//!   artifact/evidence/audit_event）为主锚；context_manifest（意图/上下文面）
//!   经 task_id 关联（agent_run.task_id 或无任务时 run_id 兜底，见 agent_loop
//!   drive() 装配面）；research_task（任务面）经 agent_run.task_id 关联。
//! - 探测顺序（按 ID 解析 trace 集合）：run 族 id → 产物/证据 id（列直查
//!   加 run 行 join 兜底）→ trace_id/correlation_id 直查 → 任务 id（一次任务
//!   多 run，可解析出多个 trace_id）→ context manifest id 反查。
//! - 015 之前落库的行 trace_id 为 NULL：关联查询按 NULL 如实呈现（空段），
//!   不回填伪造。

use database::{rusqlite, DbError, DbResult, Pool};
use serde_json::{json, Map, Value};

/// 每段行数上限（缺省；task 方向可能展开多 run，防全表倾倒）。
pub const DEFAULT_SECTION_LIMIT: usize = 200;

/// 关联链结果（五类事件链：任务/意图/上下文/工具/证据 + 审计旁路面）。
#[derive(Debug, Clone)]
pub struct TraceChain {
    /// 查询入参原值（六 ID 之一）。
    pub query_id: String,
    /// 解析出的 trace_id 集合（单 run 方向恰 1；task/manifest 方向可多）。
    pub trace_ids: Vec<String>,
    /// 解析命中面（六 ID 词形：agent_run_id/tool_run_id/artifact_id/
    /// evidence_id/trace_id/task_id/context_manifest_id；供调用方说明）。
    pub matched_via: Vec<String>,
    /// 任务行（research_task，task_id = 链内 agent_run.task_id）。
    pub tasks: Vec<Value>,
    /// agent run 行（agent_run，trace_id 直查；含 goal_text）。
    pub agent_runs: Vec<Value>,
    /// 意图/上下文清单头（context_manifest，intent_json 快照为意图面）。
    pub manifests: Vec<Value>,
    /// 上下文清单条目（context_manifest_item，manifest 归并）。
    pub context_items: Vec<Value>,
    /// 工具运行行（tool_run，trace_id 直查）。
    pub tool_runs: Vec<Value>,
    /// 产物行（artifact，trace_id 直查）。
    pub artifacts: Vec<Value>,
    /// 证据行（evidence，trace_id 直查）。
    pub evidence: Vec<Value>,
    /// 审计事件（audit_event，trace_id 直查，event_id 升序）。
    pub audit_events: Vec<Value>,
}

impl TraceChain {
    /// 非空链判定：六面行数合计 > 0。
    pub fn is_empty(&self) -> bool {
        self.tasks.is_empty()
            && self.agent_runs.is_empty()
            && self.manifests.is_empty()
            && self.context_items.is_empty()
            && self.tool_runs.is_empty()
            && self.artifacts.is_empty()
            && self.evidence.is_empty()
            && self.audit_events.is_empty()
    }

    /// 摘要计数（供 CLI 单行 JSON 与测试断言）。
    pub fn counts(&self) -> Value {
        json!({
            "tasks": self.tasks.len(),
            "agent_runs": self.agent_runs.len(),
            "manifests": self.manifests.len(),
            "context_items": self.context_items.len(),
            "tool_runs": self.tool_runs.len(),
            "artifacts": self.artifacts.len(),
            "evidence": self.evidence.len(),
            "audit_events": self.audit_events.len(),
        })
    }
}

/// 单元格 → JSON（rusqlite::types::Value 直转；BLOB 不入 JSON，只给长度
/// 说明，RIP 行面无 BLOB 列，防御性兜底）。
fn cell_to_json(v: rusqlite::types::Value) -> Value {
    use rusqlite::types::Value as RV;
    match v {
        RV::Null => Value::Null,
        RV::Integer(i) => json!(i),
        RV::Real(f) => json!(f),
        RV::Text(t) => Value::String(t),
        RV::Blob(b) => json!(format!("<blob {} bytes>", b.len())),
    }
}

/// 只读行查询：按列名出 JSON 对象数组（表存在性由调用方守卫）。
fn query_rows(
    pool: &Pool,
    sql: &str,
    params: &[&dyn rusqlite::ToSql],
    cols: &[&str],
) -> DbResult<Vec<Value>> {
    let conn = pool.get()?;
    let mut stmt = conn.prepare(sql)?;
    let mut rows = stmt.query(params)?;
    let mut out = Vec::new();
    while let Some(row) = rows.next()? {
        let mut map = Map::new();
        for (i, col) in cols.iter().enumerate() {
            let v: rusqlite::types::Value = row.get(i)?;
            map.insert((*col).to_string(), cell_to_json(v));
        }
        out.push(Value::Object(map));
    }
    Ok(out)
}

/// 表是否存在（内存库/开发副本均经迁移建表，守卫防误接未迁移库）。
fn table_exists(pool: &Pool, table: &str) -> DbResult<bool> {
    let conn = pool.get()?;
    let found: bool = conn
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1",
            [table],
            |_| Ok(true),
        )
        .optional()?
        .unwrap_or(false);
    Ok(found)
}

use database::rusqlite::OptionalExtension;

/// IN 占位符串（n 个 `?`，逗号连接）。
fn placeholders(n: usize) -> String {
    vec!["?"; n].join(",")
}

/// 按 ID 解析 trace_id 集合（探测顺序见模块头注；每面只读 SELECT）。
fn resolve_trace_ids(pool: &Pool, query_id: &str) -> DbResult<(Vec<String>, Vec<String>)> {
    let mut traces: Vec<String> = Vec::new();
    let mut via: Vec<String> = Vec::new();
    let probe = |pool: &Pool,
                 sql: String,
                 id: &str,
                 via_label: &str,
                 traces: &mut Vec<String>,
                 via: &mut Vec<String>|
     -> DbResult<()> {
        let conn = pool.get()?;
        let mut stmt = conn.prepare(&sql)?;
        let found: Vec<String> = stmt
            .query_map(rusqlite::params![id], |r| r.get::<_, String>(0))?
            .collect::<Result<Vec<_>, rusqlite::Error>>()
            .map_err(DbError::Sqlite)?;
        if !found.is_empty() && traces.is_empty() {
            *traces = found;
            via.push(via_label.to_string());
        }
        Ok(())
    };

    // 1. run 族 id（agent_run_id / tool_run_id 同名 run_id 列，按表探测）。
    probe(
        pool,
        "SELECT DISTINCT trace_id FROM agent_run WHERE run_id = ?1 AND trace_id IS NOT NULL"
            .to_string(),
        query_id,
        "agent_run_id",
        &mut traces,
        &mut via,
    )?;
    if !traces.is_empty() {
        return Ok((traces, via));
    }
    probe(
        pool,
        "SELECT DISTINCT trace_id FROM tool_run WHERE run_id = ?1 AND trace_id IS NOT NULL"
            .to_string(),
        query_id,
        "tool_run_id",
        &mut traces,
        &mut via,
    )?;
    if !traces.is_empty() {
        return Ok((traces, via));
    }

    // 2. artifact / evidence id（trace_id 列直查 + run 行 join 兜底）。
    probe(
        pool,
        "SELECT DISTINCT trace_id FROM artifact WHERE artifact_id = ?1 AND trace_id IS NOT NULL"
            .to_string(),
        query_id,
        "artifact_id",
        &mut traces,
        &mut via,
    )?;
    probe(
        pool,
        "SELECT DISTINCT tr.trace_id FROM artifact a JOIN tool_run tr ON tr.run_id = a.run_id \
         WHERE a.artifact_id = ?1 AND tr.trace_id IS NOT NULL"
            .to_string(),
        query_id,
        "artifact_id",
        &mut traces,
        &mut via,
    )?;
    if !traces.is_empty() {
        return Ok((traces, via));
    }
    probe(
        pool,
        "SELECT DISTINCT trace_id FROM evidence WHERE evidence_id = ?1 AND trace_id IS NOT NULL"
            .to_string(),
        query_id,
        "evidence_id",
        &mut traces,
        &mut via,
    )?;
    probe(
        pool,
        "SELECT DISTINCT tr.trace_id FROM evidence e JOIN tool_run tr ON tr.run_id = e.source_pk \
         WHERE e.evidence_id = ?1 AND tr.trace_id IS NOT NULL"
            .to_string(),
        query_id,
        "evidence_id",
        &mut traces,
        &mut via,
    )?;
    if !traces.is_empty() {
        return Ok((traces, via));
    }

    // 3. trace_id / correlation_id 直查（audit_event 双通道）。
    probe(
        pool,
        "SELECT DISTINCT trace_id FROM audit_event WHERE trace_id = ?1 AND trace_id IS NOT NULL"
            .to_string(),
        query_id,
        "trace_id",
        &mut traces,
        &mut via,
    )?;
    if !traces.is_empty() {
        return Ok((traces, via));
    }
    probe(
        pool,
        "SELECT DISTINCT trace_id FROM audit_event WHERE correlation_id = ?1 \
         AND trace_id IS NOT NULL"
            .to_string(),
        query_id,
        "trace_id",
        &mut traces,
        &mut via,
    )?;
    if !traces.is_empty() {
        return Ok((traces, via));
    }

    // 4. 任务 id（research_task / agent_run.task_id / tool_run.task_id；
    //    一次任务多 run，返回全部 trace）。
    probe(
        pool,
        "SELECT DISTINCT trace_id FROM agent_run WHERE task_id = ?1 AND trace_id IS NOT NULL"
            .to_string(),
        query_id,
        "task_id",
        &mut traces,
        &mut via,
    )?;
    if !traces.is_empty() {
        return Ok((traces, via));
    }
    probe(
        pool,
        "SELECT DISTINCT ar.trace_id FROM research_task rt \
         JOIN agent_run ar ON ar.task_id = rt.task_id WHERE rt.task_id = ?1 \
         AND ar.trace_id IS NOT NULL"
            .to_string(),
        query_id,
        "task_id",
        &mut traces,
        &mut via,
    )?;
    if !traces.is_empty() {
        return Ok((traces, via));
    }
    probe(
        pool,
        "SELECT DISTINCT trace_id FROM tool_run WHERE task_id = ?1 AND trace_id IS NOT NULL"
            .to_string(),
        query_id,
        "task_id",
        &mut traces,
        &mut via,
    )?;
    if !traces.is_empty() {
        return Ok((traces, via));
    }

    // 5. context manifest id 反查（manifest.task_id = run.task_id 或 run_id 兜底）。
    let manifest_task: Option<String> = {
        let conn = pool.get()?;
        conn.query_row(
            "SELECT task_id FROM context_manifest WHERE manifest_id = ?1",
            [query_id],
            |r| r.get::<_, Option<String>>(0),
        )
        .optional()?
        .flatten()
    };
    if let Some(task_id) = manifest_task {
        let conn = pool.get()?;
        let mut stmt = conn.prepare(
            "SELECT DISTINCT trace_id FROM agent_run \
             WHERE (task_id = ?1 OR run_id = ?1) AND trace_id IS NOT NULL",
        )?;
        let found: Vec<String> = stmt
            .query_map(rusqlite::params![task_id], |r| r.get::<_, String>(0))?
            .collect::<Result<Vec<_>, rusqlite::Error>>()
            .map_err(DbError::Sqlite)?;
        if !found.is_empty() {
            traces = found;
            via.push("context_manifest_id".to_string());
        }
    }
    Ok((traces, via))
}

/// 统一只读关联查询（模块头注拓扑；`limit` 为每段行数上限）。
pub fn trace_query(pool: &Pool, query_id: &str, limit: usize) -> DbResult<TraceChain> {
    let (trace_ids, matched_via) = resolve_trace_ids(pool, query_id)?;
    let mut chain = TraceChain {
        query_id: query_id.to_string(),
        trace_ids: trace_ids.clone(),
        matched_via,
        tasks: Vec::new(),
        agent_runs: Vec::new(),
        manifests: Vec::new(),
        context_items: Vec::new(),
        tool_runs: Vec::new(),
        artifacts: Vec::new(),
        evidence: Vec::new(),
        audit_events: Vec::new(),
    };
    if trace_ids.is_empty() {
        return Ok(chain);
    }

    // ---- agent_run（含 goal_text 续接上下文，014 控制行）----
    let ph = placeholders(trace_ids.len());
    let params: Vec<&dyn rusqlite::ToSql> = trace_ids
        .iter()
        .map(|t| t as &dyn rusqlite::ToSql)
        .collect();
    chain.agent_runs = query_rows(
        pool,
        &format!(
            "SELECT a.run_id, a.task_id, a.agent_id, a.agent_version, a.state, a.trace_id, \
                    a.created_at, a.started_at, a.finished_at, c.goal_text, c.retry_of_run_id \
             FROM agent_run a \
             LEFT JOIN agent_run_control c ON c.run_id = a.run_id \
             WHERE a.trace_id IN ({ph}) ORDER BY a.created_at ASC, a.run_id ASC LIMIT {limit}"
        ),
        &params,
        &[
            "run_id",
            "task_id",
            "agent_id",
            "agent_version",
            "state",
            "trace_id",
            "created_at",
            "started_at",
            "finished_at",
            "goal_text",
            "retry_of_run_id",
        ],
    )?;

    // ---- 任务面：research_task（链内 run 的 task_id）----
    let task_ids: Vec<String> = chain
        .agent_runs
        .iter()
        .filter_map(|r| r.get("task_id").and_then(Value::as_str).map(str::to_string))
        .collect();
    if table_exists(pool, "research_task")? && !task_ids.is_empty() {
        let tph = placeholders(task_ids.len());
        let tparams: Vec<&dyn rusqlite::ToSql> =
            task_ids.iter().map(|t| t as &dyn rusqlite::ToSql).collect();
        chain.tasks = query_rows(
            pool,
            &format!(
                "SELECT task_id, title, objective, target, task_type, state, created_at, updated_at \
                 FROM research_task WHERE task_id IN ({tph}) ORDER BY task_id ASC LIMIT {limit}"
            ),
            &tparams,
            &["task_id", "title", "objective", "target", "task_type", "state", "created_at", "updated_at"],
        )?;
    }

    // ---- 意图/上下文面：context_manifest（task_id = run.task_id 或 run_id 兜底）
    //      + 清单条目 ----
    if table_exists(pool, "context_manifest")? {
        let mut anchors: Vec<String> = task_ids.clone();
        for run in &chain.agent_runs {
            anchors.push(
                run.get("run_id")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
            );
        }
        anchors.retain(|a| !a.is_empty());
        if !anchors.is_empty() {
            let aph = placeholders(anchors.len());
            let aparams: Vec<&dyn rusqlite::ToSql> =
                anchors.iter().map(|t| t as &dyn rusqlite::ToSql).collect();
            chain.manifests = query_rows(
                pool,
                &format!(
                    "SELECT manifest_id, task_id, intent_json, intent_status, created_at \
                     FROM context_manifest WHERE task_id IN ({aph}) \
                     ORDER BY created_at ASC, manifest_id ASC LIMIT {limit}"
                ),
                &aparams,
                &[
                    "manifest_id",
                    "task_id",
                    "intent_json",
                    "intent_status",
                    "created_at",
                ],
            )?;
            let manifest_ids: Vec<String> = chain
                .manifests
                .iter()
                .filter_map(|m| {
                    m.get("manifest_id")
                        .and_then(Value::as_str)
                        .map(str::to_string)
                })
                .collect();
            if table_exists(pool, "context_manifest_item")? && !manifest_ids.is_empty() {
                let mph = placeholders(manifest_ids.len());
                let mparams: Vec<&dyn rusqlite::ToSql> = manifest_ids
                    .iter()
                    .map(|t| t as &dyn rusqlite::ToSql)
                    .collect();
                chain.context_items = query_rows(
                    pool,
                    &format!(
                        "SELECT manifest_id, item_id, layer, kind, included, ref_id, source, title \
                         FROM context_manifest_item WHERE manifest_id IN ({mph}) \
                         ORDER BY manifest_id ASC, item_id ASC LIMIT {limit}"
                    ),
                    &mparams,
                    &[
                        "manifest_id",
                        "item_id",
                        "layer",
                        "kind",
                        "included",
                        "ref_id",
                        "source",
                        "title",
                    ],
                )?;
            }
        }
    }

    // ---- 工具/产物/证据/审计四面（trace_id 直查）----
    chain.tool_runs = query_rows(
        pool,
        &format!(
            "SELECT run_id, tool_id, tool_version, task_id, workspace_id, submit_key, status, \
                    error_category, trace_id, created_at, started_at, finished_at \
             FROM tool_run WHERE trace_id IN ({ph}) \
             ORDER BY created_at ASC, run_id ASC LIMIT {limit}"
        ),
        &params,
        &[
            "run_id",
            "tool_id",
            "tool_version",
            "task_id",
            "workspace_id",
            "submit_key",
            "status",
            "error_category",
            "trace_id",
            "created_at",
            "started_at",
            "finished_at",
        ],
    )?;
    chain.artifacts = query_rows(
        pool,
        &format!(
            "SELECT artifact_id, run_id, role, uri, sha256, size_bytes, mime_type, trace_id, \
                    created_at \
             FROM artifact WHERE trace_id IN ({ph}) ORDER BY created_at ASC LIMIT {limit}"
        ),
        &params,
        &[
            "artifact_id",
            "run_id",
            "role",
            "uri",
            "sha256",
            "size_bytes",
            "mime_type",
            "trace_id",
            "created_at",
        ],
    )?;
    chain.evidence = query_rows(
        pool,
        &format!(
            "SELECT evidence_id, evidence_type, storage_ref, content_hash, captured_at, \
                    captured_by, task_ref, source_pk, trace_id, created_at \
             FROM evidence WHERE trace_id IN ({ph}) ORDER BY created_at ASC LIMIT {limit}"
        ),
        &params,
        &[
            "evidence_id",
            "evidence_type",
            "storage_ref",
            "content_hash",
            "captured_at",
            "captured_by",
            "task_ref",
            "source_pk",
            "trace_id",
            "created_at",
        ],
    )?;
    chain.audit_events = query_rows(
        pool,
        &format!(
            "SELECT event_id, ts, actor, action, entity_type, entity_id, correlation_id, \
                    trace_id, source_system \
             FROM audit_event WHERE trace_id IN ({ph}) ORDER BY event_id ASC LIMIT {limit}"
        ),
        &params,
        &[
            "event_id",
            "ts",
            "actor",
            "action",
            "entity_type",
            "entity_id",
            "correlation_id",
            "trace_id",
            "source_system",
        ],
    )?;

    Ok(chain)
}

/// 关联链序列化（CLI 与测试共用；单 JSON 对象含摘要计数与六面行）。
pub fn chain_to_json(chain: &TraceChain) -> Value {
    json!({
        "query_id": chain.query_id,
        "trace_ids": chain.trace_ids,
        "matched_via": chain.matched_via,
        "counts": chain.counts(),
        "tasks": chain.tasks,
        "agent_runs": chain.agent_runs,
        "manifests": chain.manifests,
        "context_items": chain.context_items,
        "tool_runs": chain.tool_runs,
        "artifacts": chain.artifacts,
        "evidence": chain.evidence,
        "audit_events": chain.audit_events,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use database::{DbPath, PoolConfig};
    use std::path::PathBuf;
    use std::time::Duration;

    fn migrated_pool() -> Pool {
        let pool = Pool::open(PoolConfig {
            path: DbPath::Memory,
            readonly: false,
            max_connections: 1,
            busy_timeout: Duration::from_secs(5),
        })
        .unwrap();
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../migrations");
        database::migrate(&pool, &dir).unwrap();
        pool
    }

    /// 空库守卫：任意 id 查询返回空链（不炸、不伪造）。
    #[test]
    fn unknown_id_returns_empty_chain() {
        let pool = migrated_pool();
        let chain = trace_query(&pool, "no-such-id", DEFAULT_SECTION_LIMIT).unwrap();
        assert!(chain.trace_ids.is_empty());
        assert!(chain.is_empty());
        assert!(chain.matched_via.is_empty());
    }

    /// 手工落行探针：六 ID 方向解析（trace 直查、run id、artifact join 兜底、
    /// task 多 run 展开、audit correlation 通道）与五面行汇出。
    #[test]
    fn resolves_by_each_direction_and_assembles_chain() {
        let pool = migrated_pool();
        pool.with_conn(|c| {
            // 定义 + 双 run（同 task）同 trace。
            c.execute_batch(
                "INSERT INTO agent_definition (agent_id, version, name, context_policy,
                   capability_refs_json, created_at, updated_at, schema_version)
                 VALUES ('agent.t', '0.1.0', 't', '{}', '[]', '2026-01-01T00:00:00Z',
                   '2026-01-01T00:00:00Z', 1);
                 INSERT INTO agent_run (run_id, task_id, agent_id, agent_version, state,
                   memory_refs_json, trace_id, created_at, updated_at, schema_version)
                 VALUES ('arn-1', 'task-9', 'agent.t', '0.1.0', 'SUCCEEDED', '[]', 'tr-9',
                   '2026-01-01T00:00:01Z', '2026-01-01T00:00:01Z', 1),
                        ('arn-2', 'task-9', 'agent.t', '0.1.0', 'FAILED', '[]', 'tr-9',
                   '2026-01-01T00:00:02Z', '2026-01-01T00:00:02Z', 1);
                 INSERT INTO agent_run_control (run_id, goal_text, created_at, updated_at,
                   schema_version) VALUES ('arn-1', '目标文本', '2026-01-01T00:00:01Z',
                   '2026-01-01T00:00:01Z', 1);
                 INSERT INTO tool (tool_id, name, kind, lifecycle, governance, permission_level,
                   source_system, source_pk, created_at, updated_at, schema_version)
                 VALUES ('rip.t', 't', 'tool', 'ENABLED', 'ACTIVE', 'T0', 'x', 'x',
                   '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z', 1);
                 INSERT INTO tool_run (run_id, tool_id, tool_version, task_id, workspace_id,
                   status, trace_id, created_at, updated_at, schema_version)
                 VALUES ('rn-1', 'rip.t', '0.1.0', 'task-9', 'ws', 'SUCCEEDED', 'tr-9',
                   '2026-01-01T00:00:03Z', '2026-01-01T00:00:03Z', 1);
                 INSERT INTO artifact (artifact_id, run_id, role, uri, sha256, size_bytes,
                   mime_type, provenance, trace_id, created_at, schema_version)
                 VALUES ('art-1', 'rn-1', 'primary', 'u', 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa', 1, 'application/json', 'p',
                   'tr-9', '2026-01-01T00:00:04Z', 1);
                 INSERT INTO evidence (evidence_id, evidence_type, storage_ref, content_hash,
                   captured_at, captured_by, task_ref, lifecycle, governance, source_system,
                   source_pk, trace_id, created_at, updated_at, schema_version)
                 VALUES ('ev-1', 'log', 'u', 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa', '2026-01-01T00:00:04Z', 'by', 'task-9',
                   'RAW', 'ACTIVE', 'tool-engine', 'rn-1', 'tr-9',
                   '2026-01-01T00:00:04Z', '2026-01-01T00:00:04Z', 1);
                 INSERT INTO audit_event (ts, actor, action, entity_type, entity_id,
                   correlation_id, trace_id, detail, permission_level, source_system, source_pk,
                   created_at, updated_at, schema_version)
                 VALUES ('2026-01-01T00:00:05Z', 'a', 'agent.run.accepted', 'agent_run', 'arn-1',
                   'arn-1', 'tr-9', '{}', 'T0', 'agent-engine', 'arn-1',
                   '2026-01-01T00:00:05Z', '2026-01-01T00:00:05Z', 1);",
            )
            .unwrap();
            Ok::<(), rusqlite::Error>(())
        })
        .unwrap();

        // trace_id 直查。
        let by_trace = trace_query(&pool, "tr-9", DEFAULT_SECTION_LIMIT).unwrap();
        assert_eq!(by_trace.trace_ids, vec!["tr-9".to_string()]);
        assert_eq!(by_trace.matched_via, vec!["trace_id".to_string()]);
        assert_eq!(by_trace.agent_runs.len(), 2);
        assert_eq!(
            by_trace.agent_runs[0]
                .get("goal_text")
                .and_then(Value::as_str),
            Some("目标文本")
        );
        assert_eq!(by_trace.tool_runs.len(), 1);
        assert_eq!(by_trace.artifacts.len(), 1);
        assert_eq!(by_trace.evidence.len(), 1);
        assert_eq!(by_trace.audit_events.len(), 1);
        assert!(
            by_trace.tasks.is_empty(),
            "research_task 无行时任务段为空（不伪造）"
        );
        assert_eq!(by_trace.counts()["agent_runs"], json!(2));

        // agent_run_id 方向。
        let by_run = trace_query(&pool, "arn-2", DEFAULT_SECTION_LIMIT).unwrap();
        assert_eq!(by_run.trace_ids, vec!["tr-9".to_string()]);
        assert_eq!(by_run.matched_via, vec!["agent_run_id".to_string()]);

        // tool_run_id 方向。
        let by_tool = trace_query(&pool, "rn-1", DEFAULT_SECTION_LIMIT).unwrap();
        assert_eq!(by_tool.matched_via, vec!["tool_run_id".to_string()]);

        // artifact_id / evidence_id 方向。
        assert_eq!(
            trace_query(&pool, "art-1", DEFAULT_SECTION_LIMIT)
                .unwrap()
                .matched_via,
            vec!["artifact_id".to_string()]
        );
        let by_ev = trace_query(&pool, "ev-1", DEFAULT_SECTION_LIMIT).unwrap();
        assert_eq!(by_ev.matched_via, vec!["evidence_id".to_string()]);
        assert_eq!(by_ev.trace_ids, vec!["tr-9".to_string()]);

        // task 方向：一次任务两 run → 全 trace。
        let by_task = trace_query(&pool, "task-9", DEFAULT_SECTION_LIMIT).unwrap();
        assert_eq!(by_task.trace_ids, vec!["tr-9".to_string()]);
        assert_eq!(by_task.matched_via, vec!["task_id".to_string()]);

        // audit correlation 通道：correlation_id 无同名 run 行时兜底命中
        // （arn-1 直查先命中 agent_run_id 面，探测顺序见模块头注）。
        let by_run = trace_query(&pool, "arn-1", DEFAULT_SECTION_LIMIT).unwrap();
        assert_eq!(by_run.matched_via, vec!["agent_run_id".to_string()]);
        pool.with_conn(|c| {
            c.execute(
                "INSERT INTO audit_event (ts, actor, action, entity_type, entity_id,
                   correlation_id, trace_id, detail, permission_level, source_system, source_pk,
                   created_at, updated_at, schema_version)
                 VALUES ('2026-01-01T00:00:06Z', 'a', 'external.step', NULL, NULL,
                   'rn-ghost', 'tr-9', '{}', 'T0', 'tool-engine', 'rn-ghost',
                   '2026-01-01T00:00:06Z', '2026-01-01T00:00:06Z', 1)",
                [],
            )
            .unwrap();
            Ok::<(), rusqlite::Error>(())
        })
        .unwrap();
        let by_corr = trace_query(&pool, "rn-ghost", DEFAULT_SECTION_LIMIT).unwrap();
        assert_eq!(by_corr.trace_ids, vec!["tr-9".to_string()]);
        assert_eq!(by_corr.matched_via, vec!["trace_id".to_string()]);

        // 序列化面。
        let text = chain_to_json(&by_trace).to_string();
        assert!(text.contains("\"tr-9\""));
        assert!(text.contains("goal_text"));
    }
}
