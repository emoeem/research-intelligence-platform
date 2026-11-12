//! Task Memory 数据面与读写 API（013 agent_task_memory 表；run 内轮次工作
//! 记忆，易失面）。
//!
//! 压缩/驱逐语义（013 头注原文）：
//! - 压缩（[`TaskMemoryStore::compress_run`]）：ACTIVE → COMPRESSED，
//!   content_json 置空 + summary 留确定性摘要（细节丢弃，计数留存）；
//! - 驱逐（[`TaskMemoryStore::evict_run`]）：ACTIVE/COMPRESSED → EVICTED，
//!   content_json 与 summary 全置空，行留墓碑（审计可查，不物理删行）。
//!
//! 载荷不变式由 domain::agent::TaskMemoryEntry::try_new 承载（回读即复验）；
//! 压缩/驱逐不动 agent_evidence_memory/agent_research_memory（契约面）。

use crate::error::AgentError;
use database::rusqlite::OptionalExtension;
use database::{rusqlite, Pool};
use domain::{RunId, TaskMemoryEntry};
use std::time::Duration;

/// Task Memory 存储等待窗（与 Pool busy_timeout 同向的显式常量，便于测试内联）。
pub const STORE_BUSY_TIMEOUT: Duration = Duration::from_secs(5);

/// 库内 Task Memory 行投影（域对象 + 时间列）。
#[derive(Debug, Clone)]
pub struct StoredTaskMemory {
    /// 域对象。
    pub entry: TaskMemoryEntry,
    /// 创建时刻。
    pub created_at: String,
    /// 更新时刻。
    pub updated_at: String,
}

/// Task Memory 读写存储。
#[derive(Clone)]
pub struct TaskMemoryStore {
    pool: Pool,
}

impl TaskMemoryStore {
    /// 在连接池之上打开。
    pub fn new(pool: Pool) -> Self {
        Self { pool }
    }

    /// 追加一条工作记忆（run 不存在先拒绝，FK 前置显式报因）。
    pub fn append(&self, entry: &TaskMemoryEntry) -> Result<(), AgentError> {
        let run_id = entry.run_id().as_str();
        let exists: bool = self.pool.with_conn(|c| {
            let mut stmt = c.prepare("SELECT 1 FROM agent_run WHERE run_id = ?1")?;
            stmt.exists([run_id])
        })?;
        if !exists {
            return Err(AgentError::RunNotFound(run_id.to_string()));
        }
        let now = domain::time::now_iso();
        self.pool
            .with_conn(|c| {
                c.execute(
                    "INSERT INTO agent_task_memory (memory_id, run_id, task_id, round,
                     content_json, summary, lifecycle, created_at, updated_at, schema_version)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8, 1)",
                    rusqlite::params![
                        entry.memory_id(),
                        run_id,
                        entry.task_id(),
                        entry.round(),
                        entry.content_json(),
                        entry.summary(),
                        entry.lifecycle().as_str(),
                        now
                    ],
                )?;
                Ok::<(), rusqlite::Error>(())
            })
            .map_err(AgentError::from)?;
        Ok(())
    }

    /// 按 memory_id 取单条。
    pub fn get(&self, memory_id: &str) -> Result<Option<StoredTaskMemory>, AgentError> {
        type Row = (
            String,
            Option<String>,
            i64,
            String,
            String,
            String,
            String,
            String,
        );
        let row: Option<Row> = self.pool.with_conn(|c| {
            let mut stmt = c.prepare(
                "SELECT run_id, task_id, round, content_json, summary, lifecycle,
                        created_at, updated_at
                 FROM agent_task_memory WHERE memory_id = ?1",
            )?;
            stmt.query_row([memory_id], |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                    r.get(6)?,
                    r.get(7)?,
                ))
            })
            .optional()
        })?;
        row.map(
            |(run_id, task_id, round, content_json, summary, lifecycle, created_at, updated_at)| {
                let lifecycle = domain::MemoryLifecycle::parse(&lifecycle)?;
                let entry = TaskMemoryEntry::try_new(
                    memory_id,
                    RunId::try_new(run_id)?,
                    task_id,
                    u32::try_from(round).unwrap_or(0),
                    content_json,
                    summary,
                    lifecycle,
                )?;
                Ok::<StoredTaskMemory, AgentError>(StoredTaskMemory {
                    entry,
                    created_at,
                    updated_at,
                })
            },
        )
        .transpose()
    }

    /// 按 run 列出（登记顺序，rowid 升序）。
    pub fn list_by_run(&self, run_id: &str) -> Result<Vec<StoredTaskMemory>, AgentError> {
        let ids: Vec<String> = self.pool.with_conn(|c| {
            let mut stmt = c.prepare(
                "SELECT memory_id FROM agent_task_memory WHERE run_id = ?1 ORDER BY rowid",
            )?;
            let rows = stmt
                .query_map([run_id], |r| r.get::<_, String>(0))?
                .collect::<Result<Vec<_>, rusqlite::Error>>()?;
            Ok::<_, rusqlite::Error>(rows)
        })?;
        let mut out = Vec::new();
        for id in ids {
            if let Some(row) = self.get(&id)? {
                out.push(row);
            }
        }
        Ok(out)
    }

    /// 压缩 run 内全部 ACTIVE 工作记忆：content_json 置空，summary 写确定性
    /// 摘要（细节丢弃，计数留存）。返回压缩条数。
    pub fn compress_run(&self, run_id: &str) -> Result<usize, AgentError> {
        let rows: Vec<(String, i64, i64)> = self.pool.with_conn(|c| {
            let mut stmt = c.prepare(
                "SELECT memory_id, round, length(content_json)
                 FROM agent_task_memory WHERE run_id = ?1 AND lifecycle = 'ACTIVE'",
            )?;
            let mapped = stmt
                .query_map([run_id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
                .collect::<Result<Vec<_>, rusqlite::Error>>()?;
            Ok::<_, rusqlite::Error>(mapped)
        })?;
        let now = domain::time::now_iso();
        let mut count = 0usize;
        for (memory_id, round, content_len) in rows {
            let summary = format!("compressed {content_len} chars of round-{round} task memory");
            let updated = self
                .pool
                .with_conn(|c| {
                    c.execute(
                        "UPDATE agent_task_memory SET lifecycle = 'COMPRESSED', content_json = '',
                         summary = ?2, updated_at = ?3
                         WHERE memory_id = ?1 AND lifecycle = 'ACTIVE'",
                        rusqlite::params![memory_id, summary, now],
                    )
                })
                .map_err(AgentError::from)?;
            count += updated;
        }
        Ok(count)
    }

    /// 驱逐 run 内全部 ACTIVE/COMPRESSED 工作记忆：详情与摘要全置空，行留
    /// 墓碑（EVICTED）。返回驱逐条数。
    pub fn evict_run(&self, run_id: &str) -> Result<usize, AgentError> {
        let now = domain::time::now_iso();
        let updated = self
            .pool
            .with_conn(|c| {
                c.execute(
                    "UPDATE agent_task_memory SET lifecycle = 'EVICTED', content_json = '',
                     summary = '', updated_at = ?2
                     WHERE run_id = ?1 AND lifecycle IN ('ACTIVE','COMPRESSED')",
                    rusqlite::params![run_id, now],
                )
            })
            .map_err(AgentError::from)?;
        Ok(updated)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use database::{DbPath, PoolConfig};
    use domain::MemoryLifecycle;

    fn migrated_pool() -> Pool {
        let pool = Pool::open(PoolConfig {
            path: DbPath::Memory,
            readonly: false,
            max_connections: 1,
            busy_timeout: STORE_BUSY_TIMEOUT,
        })
        .unwrap();
        let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../migrations");
        database::migrate(&pool, &dir).unwrap();
        pool
    }

    /// agent_run 行（FK 父）。
    fn seed_run(pool: &Pool, run_id: &str) {
        pool.with_conn(|c| -> rusqlite::Result<()> {
            c.execute(
                "INSERT INTO agent_definition (agent_id, version, name, context_policy,
                 capability_refs_json, created_at, updated_at, schema_version)
                 VALUES ('agent.mem.test', '0.1.0', 'm', '{}', '[]',
                 '2026-11-13T00:00:00Z', '2026-11-13T00:00:00Z', 1)",
                [],
            )
            .unwrap();
            c.execute(
                "INSERT INTO agent_run (run_id, task_id, agent_id, agent_version, state,
                 memory_refs_json, created_at, updated_at, schema_version)
                 VALUES (?1, 'task-m', 'agent.mem.test', '0.1.0', 'SUCCEEDED', '[]',
                 '2026-11-13T00:00:00Z', '2026-11-13T00:00:00Z', 1)",
                [run_id],
            )
            .unwrap();
            Ok(())
        })
        .unwrap();
    }

    fn entry(run_id: &str, round: u32, content: &str) -> TaskMemoryEntry {
        TaskMemoryEntry::try_new(
            format!("mem-t-{round:02}"),
            RunId::try_new(run_id).unwrap(),
            Some("task-m".to_string()),
            round,
            content.to_string(),
            "",
            MemoryLifecycle::Active,
        )
        .unwrap()
    }

    #[test]
    fn append_list_roundtrip_and_orphan_rejected() {
        let pool = migrated_pool();
        seed_run(&pool, "arn-tm1");
        let store = TaskMemoryStore::new(pool.clone());

        store.append(&entry("arn-tm1", 0, r#"{"step":1}"#)).unwrap();
        store.append(&entry("arn-tm1", 1, r#"{"step":2}"#)).unwrap();

        // 孤儿 run 拒绝（FK 前置显式报因，零行残留）。
        let orphan = entry("arn-ghost", 0, r#"{}"#);
        assert!(matches!(
            store.append(&orphan),
            Err(AgentError::RunNotFound(_))
        ));

        let rows = store.list_by_run("arn-tm1").unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].entry.round(), 0);
        assert_eq!(rows[1].entry.content_json(), r#"{"step":2}"#);
        assert_eq!(rows[0].entry.lifecycle(), MemoryLifecycle::Active);

        let one = store.get("mem-t-01").unwrap().unwrap();
        assert_eq!(one.entry.run_id().as_str(), "arn-tm1");
        assert!(store.get("mem-t-99").unwrap().is_none());
    }

    #[test]
    fn compress_then_evict_leaves_tombstones_and_payload_cleared() {
        let pool = migrated_pool();
        seed_run(&pool, "arn-tm2");
        let store = TaskMemoryStore::new(pool.clone());
        store.append(&entry("arn-tm2", 0, r#"{"a":1}"#)).unwrap();
        store.append(&entry("arn-tm2", 1, r#"{"b":2}"#)).unwrap();

        // 压缩：ACTIVE → COMPRESSED，详情置空，确定性摘要在场。
        let compressed = store.compress_run("arn-tm2").unwrap();
        assert_eq!(compressed, 2);
        for row in store.list_by_run("arn-tm2").unwrap() {
            assert_eq!(row.entry.lifecycle(), MemoryLifecycle::Compressed);
            assert!(row.entry.content_json().is_empty());
            assert!(row.entry.summary().contains("compressed"));
        }

        // 压缩幂等：无 ACTIVE 行再压零条。
        assert_eq!(store.compress_run("arn-tm2").unwrap(), 0);

        // 驱逐：COMPRESSED → EVICTED，摘要也置空（墓碑行仍在）。
        let evicted = store.evict_run("arn-tm2").unwrap();
        assert_eq!(evicted, 2);
        let rows = store.list_by_run("arn-tm2").unwrap();
        assert_eq!(rows.len(), 2, "墓碑行保留，不物理删行");
        for row in rows {
            assert_eq!(row.entry.lifecycle(), MemoryLifecycle::Evicted);
            assert!(row.entry.content_json().is_empty());
            assert!(row.entry.summary().is_empty());
        }
        assert_eq!(store.evict_run("arn-tm2").unwrap(), 0, "驱逐幂等");
    }
}
