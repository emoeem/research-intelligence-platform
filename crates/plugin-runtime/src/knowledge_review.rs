//! 知识条目审核状态受控写面（SK-1，ADR 第七节：Search/Context 只收
//! APPROVED，翻转必须走受控 API 并留审计事件；计划 §7 第 4 条）。
//!
//! 写面纪律：只更新指定 plugin/version/entry 的 review_status；状态变更与
//! entry-linked append-only review log 同事务写入，再补发全局审计事件。
//! RTIS 三类（knowledge_ingest）不经此面（P6-5 词表无审核轴，010 表结构不动）。

use database::Pool;
use domain::knowledge::ReviewStatus;
use domain::time::now_iso;
use serde_json::json;
use std::collections::BTreeSet;

use crate::RuntimeError;

/// One durable source review decision.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct KnowledgeReviewEvent {
    /// State before the explicit decision.
    pub from_status: String,
    /// State selected by the reviewer.
    pub to_status: String,
    /// Reviewer identity supplied by the caller.
    pub reviewer: String,
    /// Human rationale recorded with the decision.
    pub reason: String,
    /// UTC timestamp for the decision.
    pub created_at: String,
}

/// Reviewer identity and rationale for one explicit review decision.
#[derive(Debug, Clone, Copy)]
pub struct KnowledgeReviewContext<'a> {
    /// Reviewer identity supplied by the caller.
    pub reviewer: &'a str,
    /// Human rationale recorded with the decision.
    pub reason: &'a str,
}

/// 审核状态翻转（DISCOVERED/UNREVIEWED/REJECTED/APPROVED/DEPRECATED 任意
/// 组合允许，全部留审计；驳回后复批是合法治理路径）。
pub fn set_review_status(
    pool: &Pool,
    audit: &audit_engine::AuditEngine,
    plugin_id: &str,
    plugin_version: &str,
    item_keys: &[&str],
    to: ReviewStatus,
    context: KnowledgeReviewContext<'_>,
) -> Result<u64, RuntimeError> {
    if item_keys.is_empty() {
        return Err(RuntimeError::new(
            "review_keys_empty",
            "item_keys must be non-empty (refuse-to-flip-everything guard)",
        ));
    }
    if context.reviewer.trim().is_empty()
        || plugin_version.trim().is_empty()
        || context.reason.trim().is_empty()
    {
        return Err(RuntimeError::new(
            "review_context_required",
            "source review requires a plugin version, reviewer and non-empty reason",
        ));
    }
    let unique_keys: BTreeSet<&str> = item_keys.iter().copied().collect();
    if unique_keys.len() != item_keys.len() || unique_keys.iter().any(|key| key.trim().is_empty()) {
        return Err(RuntimeError::new(
            "review_keys_invalid",
            "source review keys must be non-empty and unique",
        ));
    }
    let changes = pool
        .with_tx(
            |c| -> Result<Vec<serde_json::Value>, database::rusqlite::Error> {
                let mut changes = Vec::with_capacity(item_keys.len());
                for key in item_keys {
                    let (entry_id, from_status): (i64, String) = c.query_row(
                        "SELECT entry_id, review_status FROM knowledge_source_entry
                 WHERE plugin_id = ?1 AND plugin_version = ?2 AND item_key = ?3",
                        database::rusqlite::params![plugin_id, plugin_version, key],
                        |row| Ok((row.get(0)?, row.get(1)?)),
                    )?;
                    c.execute(
                        "UPDATE knowledge_source_entry SET review_status = ?1
                 WHERE entry_id = ?2",
                        database::rusqlite::params![to.as_str(), entry_id],
                    )?;
                    let created_at = now_iso();
                    c.execute(
                        "INSERT INTO knowledge_source_review_log (
                     entry_id, from_status, to_status, reviewer, reason, created_at
                 ) VALUES (?1,?2,?3,?4,?5,?6)",
                        database::rusqlite::params![
                            entry_id,
                            from_status,
                            to.as_str(),
                            context.reviewer,
                            context.reason,
                            created_at
                        ],
                    )?;
                    changes.push(json!({
                        "item_key": key,
                        "plugin_version": plugin_version,
                        "from_status": from_status,
                        "to_status": to.as_str(),
                        "entry_id": entry_id,
                    }));
                }
                Ok(changes)
            },
        )
        .map_err(|e: database::DbError| match e {
            database::DbError::Sqlite(database::rusqlite::Error::QueryReturnedNoRows) => {
                RuntimeError::new(
                    "review_target_not_found",
                    "one or more source entries were not found; no changes were committed",
                )
            }
            other => RuntimeError::new("db", other.to_string()),
        })?;
    audit.emit_best_effort(
        domain::audit::AuditEvent::new(
            context.reviewer,
            audit_engine::plugin_actions::KNOWLEDGE_REVIEW,
        )
        .with_entity("plugin", plugin_id)
        .with_detail(
            json!({
                "to": to.as_str(),
                "item_keys": item_keys,
                "reason": context.reason,
                "changes": changes,
            })
            .to_string(),
        ),
    );
    Ok(changes.len() as u64)
}

/// Read durable review decisions for one plugin source entry.
pub fn review_history(
    pool: &Pool,
    plugin_id: &str,
    plugin_version: &str,
    item_key: &str,
) -> Result<Vec<KnowledgeReviewEvent>, RuntimeError> {
    pool.with_conn(|c| {
        let mut stmt = c.prepare(
            "SELECT l.from_status, l.to_status, l.reviewer, l.reason, l.created_at
             FROM knowledge_source_review_log l
             JOIN knowledge_source_entry e ON e.entry_id=l.entry_id
             WHERE e.plugin_id=?1 AND e.plugin_version=?2 AND e.item_key=?3
             ORDER BY l.created_at, l.review_id",
        )?;
        let rows = stmt
            .query_map(
                database::rusqlite::params![plugin_id, plugin_version, item_key],
                |row| {
                    Ok(KnowledgeReviewEvent {
                        from_status: row.get(0)?,
                        to_status: row.get(1)?,
                        reviewer: row.get(2)?,
                        reason: row.get(3)?,
                        created_at: row.get(4)?,
                    })
                },
            )?
            .collect::<Result<Vec<_>, database::rusqlite::Error>>()?;
        Ok::<_, database::rusqlite::Error>(rows)
    })
    .map_err(|e: database::DbError| RuntimeError::new("db", e.to_string()))
}

/// 某插件当前各审核状态行数（CLI 展示与对账）。
pub fn review_counts(
    pool: &Pool,
    plugin_id: &str,
    plugin_version: &str,
) -> Result<Vec<(String, i64)>, RuntimeError> {
    pool.with_conn(|c| {
        let mut stmt = c.prepare(
            "SELECT review_status, COUNT(*) FROM knowledge_source_entry
             WHERE plugin_id = ?1 AND plugin_version = ?2
             GROUP BY review_status ORDER BY review_status",
        )?;
        let rows = stmt
            .query_map([plugin_id, plugin_version], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
            })?
            .collect::<Result<Vec<_>, database::rusqlite::Error>>()?;
        Ok::<_, database::rusqlite::Error>(rows)
    })
    .map_err(|e: database::DbError| RuntimeError::new("db", e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn review_status_wordlist_is_uppercase() {
        assert_eq!(ReviewStatus::Approved.as_str(), "APPROVED");
        assert_eq!(ReviewStatus::Discovered.as_str(), "DISCOVERED");
    }
}
