//! ContextManifest 三表持久化与引用事件流水（设计稿 §6；008 迁移三表）。
//!
//! 纪律（设计稿 §2 第 5 条 + §6.1）：只追加不更新，本模块不含任何改写
//! 语句；重装配产生新 manifest_id；删除语义由外键 ON DELETE CASCADE 承担
//! （应用层不逐条删）。三表：
//!
//! - `context_manifest`：装配清单头（selection_hash/budget/totals 快照）；
//! - `context_manifest_item`：清单条目（layer/kind/included 词表 =
//!   domain::context 枚举）；
//! - `context_ref_log`：引用事件（ASSEMBLED/EXPANDED/EVICTED/CITED_IN_
//!   TOOL_CALL/CITED_IN_EVIDENCE，设计稿 §6.1 event 列）。

use crate::error::ContextBrokerError;
use crate::{ContextItemKind, ContextLayer, InclusionForm, IntentStatus, RefEvent};
use database::Pool;

/// domain 词表解析错误到 rusqlite 行映射错误（query_row 闭包内使用）。
fn row_err(e: domain::DomainError) -> database::rusqlite::Error {
    database::rusqlite::Error::ToSqlConversionFailure(Box::new(e))
}

/// 清单头行（context_manifest 投影）。
#[derive(Debug, Clone, PartialEq)]
pub struct ManifestRow {
    /// 清单 id（ctxm-<hex>，重装配出新 id）。
    pub manifest_id: String,
    /// 关联任务。
    pub task_id: String,
    /// IntentQuery 快照（JSON）。
    pub intent_json: String,
    /// 预算策略版本。
    pub policy_version: String,
    /// 装配逻辑版本。
    pub broker_version: String,
    /// 分项分配快照（JSON）。
    pub budget_json: String,
    /// 分项实际占用与驱逐计数（JSON）。
    pub totals_json: String,
    /// 意图状态。
    pub intent_status: IntentStatus,
    /// 所选集合指纹（同输入一致，可对账）。
    pub selection_hash: String,
    /// 签发时刻（ISO8601 UTC）。
    pub created_at: String,
    /// 结构版本。
    pub schema_version: String,
}

/// 清单条目行（context_manifest_item 投影）。
#[derive(Debug, Clone, PartialEq)]
pub struct ItemRow {
    /// 所属清单。
    pub manifest_id: String,
    /// 条目 id（清单内唯一）。
    pub item_id: String,
    /// 层级。
    pub layer: ContextLayer,
    /// 种类。
    pub kind: ContextItemKind,
    /// 来源对象 id。
    pub ref_id: String,
    /// 来源版本（可空指 current）。
    pub ref_version: Option<String>,
    /// 来源源 id。
    pub source: String,
    /// 标题。
    pub title: String,
    /// 「为什么是它」（可解释验收落点）。
    pub reason: String,
    /// 命中信号数组（JSON）。
    pub matched_signals_json: String,
    /// 预算预计算 token 数。
    pub est_tokens: i64,
    /// 纳入形态（full/summary/ref-only/catalog-card）。
    pub included: InclusionForm,
    /// 内容 sha256 hex（可空）。
    pub content_hash: Option<String>,
}

/// 引用事件行（context_ref_log 投影）。
#[derive(Debug, Clone, PartialEq)]
pub struct RefEventRow {
    /// 事件流水 id。
    pub log_id: i64,
    /// 所属清单。
    pub manifest_id: String,
    /// 条目 id。
    pub item_id: String,
    /// 事件类型。
    pub event: RefEvent,
    /// 事件明细（JSON）。
    pub detail_json: String,
    /// 事件时刻。
    pub created_at: String,
}

/// 三表持久化门面（只追加；读回供对账与 expand 记账）。
pub struct ManifestStore {
    pool: Pool,
}

impl ManifestStore {
    /// 用连接池构造（开发库副本或测试内存库；生产库不经本 store）。
    pub fn new(pool: Pool) -> Self {
        Self { pool }
    }

    /// 连接池只读访问（对账用）。
    pub fn pool(&self) -> &Pool {
        &self.pool
    }

    /// 插入清单头（同 manifest_id 重复插入即 PK 冲突报错：只追加语义）。
    pub fn insert_manifest(&self, row: &ManifestRow) -> Result<(), ContextBrokerError> {
        self.pool
            .with_conn(|c| {
                c.execute(
                    "INSERT INTO context_manifest (manifest_id, task_id, intent_json,
                       policy_version, broker_version, budget_json, totals_json,
                       intent_status, selection_hash, created_at, schema_version)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                    database::rusqlite::params![
                        row.manifest_id,
                        row.task_id,
                        row.intent_json,
                        row.policy_version,
                        row.broker_version,
                        row.budget_json,
                        row.totals_json,
                        row.intent_status.as_str(),
                        row.selection_hash,
                        row.created_at,
                        row.schema_version,
                    ],
                )?;
                Ok::<(), database::rusqlite::Error>(())
            })
            .map_err(|e| ContextBrokerError::Source {
                source: "manifest-store".to_string(),
                reason: e.to_string(),
            })
    }

    /// 插入清单条目。
    pub fn insert_item(&self, row: &ItemRow) -> Result<(), ContextBrokerError> {
        self.pool
            .with_conn(|c| {
                c.execute(
                    "INSERT INTO context_manifest_item (manifest_id, item_id, layer, kind,
                       ref_id, ref_version, source, title, reason, matched_signals_json,
                       est_tokens, included, content_hash)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
                    database::rusqlite::params![
                        row.manifest_id,
                        row.item_id,
                        row.layer.as_str(),
                        row.kind.as_str(),
                        row.ref_id,
                        row.ref_version,
                        row.source,
                        row.title,
                        row.reason,
                        row.matched_signals_json,
                        row.est_tokens,
                        row.included.as_str(),
                        row.content_hash,
                    ],
                )?;
                Ok::<(), database::rusqlite::Error>(())
            })
            .map_err(|e| ContextBrokerError::Source {
                source: "manifest-store".to_string(),
                reason: e.to_string(),
            })
    }

    /// 追加引用事件（ASSEMBLED/EXPANDED/EVICTED/CITED_*；expand 记账落点）。
    pub fn append_ref_event(
        &self,
        manifest_id: &str,
        item_id: &str,
        event: RefEvent,
        detail_json: &str,
    ) -> Result<i64, ContextBrokerError> {
        self.pool
            .with_conn(|c| {
                c.execute(
                    "INSERT INTO context_ref_log (manifest_id, item_id, event, detail_json, created_at)
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                    database::rusqlite::params![
                        manifest_id,
                        item_id,
                        event.as_str(),
                        detail_json,
                        domain::time::now_iso(),
                    ],
                )?;
                Ok::<i64, database::rusqlite::Error>(c.last_insert_rowid())
            })
            .map_err(|e| ContextBrokerError::Source {
                source: "manifest-store".to_string(),
                reason: e.to_string(),
            })
    }

    /// 读回清单头（对账与 expand 校验）。
    pub fn manifest(&self, manifest_id: &str) -> Result<Option<ManifestRow>, ContextBrokerError> {
        self.pool
            .with_conn(|c| {
                let row = c
                    .query_row(
                        "SELECT manifest_id, task_id, intent_json, policy_version,
                            broker_version, budget_json, totals_json, intent_status,
                            selection_hash, created_at, schema_version
                         FROM context_manifest WHERE manifest_id = ?1",
                        [manifest_id],
                        |r| {
                            Ok(ManifestRow {
                                manifest_id: r.get(0)?,
                                task_id: r.get(1)?,
                                intent_json: r.get(2)?,
                                policy_version: r.get(3)?,
                                broker_version: r.get(4)?,
                                budget_json: r.get(5)?,
                                totals_json: r.get(6)?,
                                intent_status: IntentStatus::parse(&r.get::<_, String>(7)?)
                                    .map_err(row_err)?,
                                selection_hash: r.get(8)?,
                                created_at: r.get(9)?,
                                schema_version: r.get(10)?,
                            })
                        },
                    )
                    .map(Some)
                    .or_else(|e| match e {
                        database::rusqlite::Error::QueryReturnedNoRows => Ok(None),
                        other => Err(other),
                    })?;
                Ok::<Option<ManifestRow>, database::rusqlite::Error>(row)
            })
            .map_err(|e| ContextBrokerError::Source {
                source: "manifest-store".to_string(),
                reason: e.to_string(),
            })
    }

    /// 读回清单全部条目（按 item_id 序）。
    pub fn items(&self, manifest_id: &str) -> Result<Vec<ItemRow>, ContextBrokerError> {
        self.pool
            .with_conn(|c| {
                let mut stmt = c.prepare(
                    "SELECT manifest_id, item_id, layer, kind, ref_id, ref_version, source,
                            title, reason, matched_signals_json, est_tokens, included, content_hash
                     FROM context_manifest_item WHERE manifest_id = ?1 ORDER BY item_id",
                )?;
                let rows = stmt
                    .query_map([manifest_id], |r| {
                        Ok(ItemRow {
                            manifest_id: r.get(0)?,
                            item_id: r.get(1)?,
                            layer: ContextLayer::parse(&r.get::<_, String>(2)?).map_err(row_err)?,
                            kind: ContextItemKind::parse(&r.get::<_, String>(3)?)
                                .map_err(row_err)?,
                            ref_id: r.get(4)?,
                            ref_version: r.get(5)?,
                            source: r.get(6)?,
                            title: r.get(7)?,
                            reason: r.get(8)?,
                            matched_signals_json: r.get(9)?,
                            est_tokens: r.get(10)?,
                            included: InclusionForm::parse(&r.get::<_, String>(11)?)
                                .map_err(row_err)?,
                            content_hash: r.get(12)?,
                        })
                    })?
                    .collect::<Result<Vec<_>, database::rusqlite::Error>>()?;
                Ok::<Vec<ItemRow>, database::rusqlite::Error>(rows)
            })
            .map_err(|e| ContextBrokerError::Source {
                source: "manifest-store".to_string(),
                reason: e.to_string(),
            })
    }

    /// 读回清单全部引用事件（按 log_id 序；l3 已用额度由此推导，只追加
    /// 语义下无需更新 totals_json）。
    pub fn ref_events(&self, manifest_id: &str) -> Result<Vec<RefEventRow>, ContextBrokerError> {
        self.pool
            .with_conn(|c| {
                let mut stmt = c.prepare(
                    "SELECT log_id, manifest_id, item_id, event, detail_json, created_at
                     FROM context_ref_log WHERE manifest_id = ?1 ORDER BY log_id",
                )?;
                let rows = stmt
                    .query_map([manifest_id], |r| {
                        Ok(RefEventRow {
                            log_id: r.get(0)?,
                            manifest_id: r.get(1)?,
                            item_id: r.get(2)?,
                            event: RefEvent::parse(&r.get::<_, String>(3)?).map_err(row_err)?,
                            detail_json: r.get(4)?,
                            created_at: r.get(5)?,
                        })
                    })?
                    .collect::<Result<Vec<_>, database::rusqlite::Error>>()?;
                Ok::<Vec<RefEventRow>, database::rusqlite::Error>(rows)
            })
            .map_err(|e| ContextBrokerError::Source {
                source: "manifest-store".to_string(),
                reason: e.to_string(),
            })
    }

    /// 汇总某清单已展开的 l3 tokens（EXPANDED 事件 detail.granted_tokens 之和；
    /// 只追加语义下的已用额度推导）。
    pub fn expanded_tokens(&self, manifest_id: &str) -> Result<u64, ContextBrokerError> {
        let events = self.ref_events(manifest_id)?;
        let mut total = 0u64;
        for event in events {
            if event.event == RefEvent::Expanded {
                if let Ok(detail) = serde_json::from_str::<serde_json::Value>(&event.detail_json) {
                    total += detail
                        .get("granted_tokens")
                        .and_then(serde_json::Value::as_u64)
                        .unwrap_or(0);
                }
            }
        }
        Ok(total)
    }
}
