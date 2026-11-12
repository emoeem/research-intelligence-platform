//! 统一审计事件流（MODULE_BOUNDARIES §8.1 audit-engine 行 + §8.2.4 旁路纪律）。
//!
//! 接口（§8.4）：
//! - [`AuditEngine::emit`]：`Emit(AuditEvent)`，各模块唯一审计出口
//! - [`AuditEngine::trace`]：`Trace(correlation_id)`，按关联 ID 回放事件链
//!
//! 旁路纪律（§8.2.4）：任何模块故障不得因审计写入失败而失败。
//! [`AuditEngine::emit_best_effort`] 写入失败时进进程内缓冲（有界，丢最旧），
//! 下次调用时自动重试；跨进程持久队列留后续里程碑。
#![forbid(unsafe_code)]
#![deny(missing_docs)]

use database::{rusqlite, DbError, DbResult, Pool};
use domain::audit::AuditEvent;
use std::sync::Mutex;

/// 缓冲上限（超过即丢最旧，旁路不反压业务）。
pub const BUFFER_CAP: usize = 1024;

/// 插件生命周期审计动作词形（P6-2 接线，PLUGIN-CONTRACT §11.4：install/
/// enable/disable/upgrade/uninstall 全事件走本 crate 唯一 Emit 出口，不新增
/// JSONL 分叉）。调用方只引用常量，防词形漂移。
pub mod plugin_actions {
    /// 安装（含静态校验、checksums、lock、capability 登记）。
    pub const INSTALL: &str = "plugin.install";
    /// 启用 / 重启用（重过静态检查，§5.3）。
    pub const ENABLE: &str = "plugin.enable";
    /// 禁用（数据不动，capability 撤出匹配面）。
    pub const DISABLE: &str = "plugin.disable";
    /// 升级（result=switched / rolled_back，§5.3）。
    pub const UPGRADE: &str = "plugin.upgrade";
    /// 卸载（来源已卸载标记 + §5.4 保留处置报告）。
    pub const UNINSTALL: &str = "plugin.uninstall";
    /// capability 登记（INSTALLED → REGISTERED，系统步；install 内完成）。
    pub const REGISTER: &str = "plugin.register";
    /// 知识条目审核状态翻转（SK-1：APPROVED 才进 Search/Context 面，
    /// 翻转走 plugin-runtime knowledge_review 受控写面）。
    pub const KNOWLEDGE_REVIEW: &str = "plugin.knowledge.review";
    /// 知识摄取（SK-1：ingest 动作审计，dry-run 亦留痕标注 dry_run=true）。
    pub const KNOWLEDGE_INGEST: &str = "plugin.knowledge.ingest";
    /// 知识来源同步（SK-3：rip.knowledge.source.sync，含 dry-run/幂等跳过/
    /// 部分失败全留痕，detail 机器可读）。
    pub const KNOWLEDGE_SYNC: &str = "plugin.knowledge.sync";
    /// 候选资源映射（SK-3：APPROVED skill_candidate 到 Resource 候选，
    /// resource-engine 受控写面）。
    pub const KNOWLEDGE_MAP_CANDIDATES: &str = "plugin.knowledge.map_candidates";
}

/// 审计引擎。
pub struct AuditEngine {
    pool: Pool,
    buffer: Mutex<Vec<AuditEvent>>,
}

/// 落库后的事件（含行 ID）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredAuditEvent {
    /// audit_event 表自增 event_id。
    pub event_id: i64,
    /// 事件内容。
    pub event: AuditEvent,
}

impl AuditEngine {
    /// 用连接池构造引擎。
    pub fn new(pool: Pool) -> Self {
        Self {
            pool,
            buffer: Mutex::new(Vec::new()),
        }
    }

    /// `Emit(AuditEvent)`：写入 audit_event 表，返回行 ID。
    pub fn emit(&self, event: AuditEvent) -> DbResult<i64> {
        event
            .validate()
            .map_err(|e| DbError::Validation(e.to_string()))?;
        self.insert(&event)
    }

    /// 旁路发射：先重试缓冲，再写当前事件；任何失败只缓冲、不向调用方报错。
    ///
    /// 返回 true 表示当前事件已落库，false 表示已入缓冲待重试。
    pub fn emit_best_effort(&self, event: AuditEvent) -> bool {
        if let Err(err) = event.validate() {
            // 非法事件属于编程错误：记日志并丢弃，不缓冲（重试也不会变合法）。
            eprintln!("audit-engine: dropped invalid event: {err}");
            return false;
        }
        self.flush();
        match self.insert(&event) {
            Ok(_) => true,
            Err(err) => {
                eprintln!("audit-engine: buffering event after write failure: {err}");
                self.push_to_buffer(event);
                false
            }
        }
    }

    /// 重试缓冲中的事件，返回成功落库条数。
    pub fn flush(&self) -> usize {
        let drained: Vec<AuditEvent> = match self.buffer.lock() {
            Ok(mut buf) => std::mem::take(&mut *buf),
            Err(_) => return 0,
        };
        let mut flushed = 0;
        let mut retry = Vec::new();
        for ev in drained {
            match self.insert(&ev) {
                Ok(_) => flushed += 1,
                Err(_) => retry.push(ev),
            }
        }
        if !retry.is_empty() {
            if let Ok(mut buf) = self.buffer.lock() {
                // 未清空的新事件排在前，重试失败的保持在后，顺序近似保留。
                for ev in retry {
                    push_capped(&mut buf, ev, BUFFER_CAP);
                }
            }
        }
        flushed
    }

    /// 当前缓冲条数。
    pub fn buffered(&self) -> usize {
        self.buffer.lock().map(|b| b.len()).unwrap_or(0)
    }

    /// `Trace(correlation_id)`：按关联 ID 升序回放事件链。
    pub fn trace(&self, correlation_id: &str) -> DbResult<Vec<StoredAuditEvent>> {
        let conn = self.pool.get()?;
        let mut stmt = conn.prepare(
            "SELECT event_id, ts, actor, action, entity_type, entity_id, correlation_id,
                    trace_id, detail, permission_level, source_system, source_pk
             FROM audit_event WHERE correlation_id = ?1 ORDER BY event_id ASC",
        )?;
        let rows = stmt.query_map([correlation_id], row_to_event)?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    /// `Trace` 的 trace_id 方向（P7-5 迁移 015）：按链路追踪 ID 升序回放
    /// 跨实体族事件链（同一 trace_id 下的全部事件，不限单一 run）。
    pub fn trace_by_trace_id(&self, trace_id: &str) -> DbResult<Vec<StoredAuditEvent>> {
        let conn = self.pool.get()?;
        let mut stmt = conn.prepare(
            "SELECT event_id, ts, actor, action, entity_type, entity_id, correlation_id,
                    trace_id, detail, permission_level, source_system, source_pk
             FROM audit_event WHERE trace_id = ?1 ORDER BY event_id ASC",
        )?;
        let rows = stmt.query_map([trace_id], row_to_event)?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    /// 入缓冲（有界，丢最旧）。锁中毒时丢弃事件（旁路不反压）。
    fn push_to_buffer(&self, event: AuditEvent) {
        if let Ok(mut buf) = self.buffer.lock() {
            push_capped(&mut buf, event, BUFFER_CAP);
        }
    }

    fn insert(&self, event: &AuditEvent) -> DbResult<i64> {
        let conn = self.pool.get()?;
        conn.execute(
            "INSERT INTO audit_event (ts, actor, action, entity_type, entity_id,
                                      correlation_id, trace_id, detail, permission_level,
                                      source_system, source_pk, created_at, updated_at,
                                      schema_version)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11,
                     strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
                     strftime('%Y-%m-%dT%H:%M:%SZ', 'now'), 1)",
            rusqlite_params(event),
        )
        .map_err(DbError::Sqlite)?;
        Ok(conn.last_insert_rowid())
    }
}

fn rusqlite_params(e: &AuditEvent) -> [&dyn rusqlite::ToSql; 11] {
    [
        &e.ts,
        &e.actor,
        &e.action,
        &e.entity_type,
        &e.entity_id,
        &e.correlation_id,
        &e.trace_id,
        &e.detail,
        &e.permission_level,
        &e.source_system,
        &e.source_pk,
    ]
}

fn row_to_event(row: &rusqlite::Row<'_>) -> rusqlite::Result<StoredAuditEvent> {
    Ok(StoredAuditEvent {
        event_id: row.get(0)?,
        event: AuditEvent {
            ts: row.get(1)?,
            actor: row.get(2)?,
            action: row.get(3)?,
            entity_type: row.get(4)?,
            entity_id: row.get(5)?,
            correlation_id: row.get(6)?,
            trace_id: row.get(7)?,
            detail: row.get(8)?,
            permission_level: row.get(9)?,
            source_system: row.get(10)?,
            source_pk: row.get(11)?,
        },
    })
}

fn push_capped(buf: &mut Vec<AuditEvent>, ev: AuditEvent, cap: usize) {
    if buf.len() >= cap {
        buf.remove(0);
    }
    buf.push(ev);
}

#[cfg(test)]
mod tests {
    use super::*;
    use database::{DbPath, PoolConfig};
    use std::time::Duration;

    fn engine_with_audit_table() -> AuditEngine {
        let pool = Pool::open(PoolConfig {
            path: DbPath::Memory,
            readonly: false,
            max_connections: 1,
            busy_timeout: Duration::from_secs(1),
        })
        .unwrap();
        pool.with_conn(|c| {
            c.execute_batch(
                "CREATE TABLE audit_event (
                   event_id INTEGER PRIMARY KEY AUTOINCREMENT,
                   ts TEXT NOT NULL, actor TEXT NOT NULL, action TEXT NOT NULL,
                   entity_type TEXT, entity_id TEXT, correlation_id TEXT, trace_id TEXT,
                   detail TEXT,
                   permission_level TEXT, source_system TEXT, source_pk TEXT,
                   created_at TEXT NOT NULL, updated_at TEXT NOT NULL,
                   schema_version INTEGER NOT NULL DEFAULT 1);",
            )
        })
        .unwrap();
        AuditEngine::new(pool)
    }

    #[test]
    fn emit_and_trace_roundtrip() {
        let engine = engine_with_audit_table();
        let id = engine
            .emit(
                AuditEvent::new("resource-engine", "resource.resolved")
                    .with_entity("resource", "gh:o/r")
                    .with_correlation("erq-1"),
            )
            .unwrap();
        assert!(id >= 1);
        let chain = engine.trace("erq-1").unwrap();
        assert_eq!(chain.len(), 1);
        assert_eq!(chain[0].event.action, "resource.resolved");
        assert!(engine.trace("no-such").unwrap().is_empty());
    }

    /// P7-5：trace_id 列写读回环 + trace_by_trace_id 跨 run 回放
    /// （correlation_id 各自独立，trace_id 归并两 run 的事件）。
    #[test]
    fn trace_id_roundtrip_and_cross_run_replay() {
        let engine = engine_with_audit_table();
        engine
            .emit(
                AuditEvent::new("agent-engine", "agent.run.accepted")
                    .with_correlation("rn-a")
                    .with_trace("tr-1"),
            )
            .unwrap();
        engine
            .emit(
                AuditEvent::new("agent-engine", "agent.run.finished")
                    .with_correlation("rn-b")
                    .with_trace("tr-1"),
            )
            .unwrap();
        engine
            .emit(AuditEvent::new("system", "unrelated").with_correlation("rn-c"))
            .unwrap();

        let by_corr = engine.trace("rn-a").unwrap();
        assert_eq!(by_corr.len(), 1);
        assert_eq!(by_corr[0].event.trace_id.as_deref(), Some("tr-1"));

        let by_trace = engine.trace_by_trace_id("tr-1").unwrap();
        assert_eq!(by_trace.len(), 2, "trace_id 归并两 run 事件");
        assert_eq!(by_trace[0].event.correlation_id.as_deref(), Some("rn-a"));
        assert_eq!(by_trace[1].event.correlation_id.as_deref(), Some("rn-b"));
        assert!(engine.trace_by_trace_id("tr-none").unwrap().is_empty());
    }

    #[test]
    fn emit_rejects_invalid_event() {
        let engine = engine_with_audit_table();
        assert!(engine.emit(AuditEvent::new("", "a")).is_err());
        assert!(engine.emit(AuditEvent::new("a", " ")).is_err());
    }

    #[test]
    fn best_effort_buffers_then_flushes() {
        let engine = engine_with_audit_table();
        assert!(engine.emit_best_effort(AuditEvent::new("system", "x").with_correlation("c1")));
        assert_eq!(engine.buffered(), 0);
        // 非法事件被丢弃，不进缓冲。
        assert!(!engine.emit_best_effort(AuditEvent::new("", "x")));
        assert_eq!(engine.buffered(), 0);
        assert_eq!(engine.flush(), 0);
    }

    #[test]
    fn buffer_cap_drops_oldest() {
        let mut buf: Vec<AuditEvent> = Vec::new();
        for i in 0..(BUFFER_CAP + 10) {
            push_capped(
                &mut buf,
                AuditEvent::new("a", format!("act-{i}")),
                BUFFER_CAP,
            );
        }
        assert_eq!(buf.len(), BUFFER_CAP);
        assert_eq!(buf[0].action, "act-10", "oldest entries must be dropped");
    }
}
