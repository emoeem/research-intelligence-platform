//! 知识条目审核状态受控写面（SK-1，ADR 第七节：Search/Context 只收
//! APPROVED，翻转必须走受控 API 并留审计事件；计划 §7 第 4 条）。
//!
//! 写面纪律：只 UPDATE knowledge_source_entry.review_status 一列（011 表，
//! 本模块为该列唯一写面）；任何状态翻转 Emit 一条
//! [`plugin_actions::KNOWLEDGE_REVIEW`] 审计事件（audit-engine 唯一出口，
//! 旁路发射不反压）。RTIS 三类（knowledge_ingest）不经此面（P6-5 词表无
//! 审核轴，010 表结构不动）。

use database::Pool;
use domain::knowledge::ReviewStatus;
use serde_json::json;

use crate::RuntimeError;

/// 审核状态翻转（DISCOVERED/UNREVIEWED/REJECTED/APPROVED/DEPRECATED 任意
/// 组合允许，全部留审计；驳回后复批是合法治理路径）。
pub fn set_review_status(
    pool: &Pool,
    audit: &audit_engine::AuditEngine,
    actor: &str,
    plugin_id: &str,
    item_keys: &[&str],
    to: ReviewStatus,
) -> Result<u64, RuntimeError> {
    if item_keys.is_empty() {
        return Err(RuntimeError::new(
            "review_keys_empty",
            "item_keys must be non-empty (refuse-to-flip-everything guard)",
        ));
    }
    let mut updated = 0u64;
    pool.with_tx(|c| -> Result<(), database::rusqlite::Error> {
        for key in item_keys {
            let n = c.execute(
                "UPDATE knowledge_source_entry SET review_status = ?1
                 WHERE plugin_id = ?2 AND item_key = ?3",
                database::rusqlite::params![to.as_str(), plugin_id, key],
            )?;
            updated += n as u64;
        }
        Ok(())
    })
    .map_err(|e: database::DbError| RuntimeError::new("db", e.to_string()))?;
    audit.emit_best_effort(
        domain::audit::AuditEvent::new(actor, audit_engine::plugin_actions::KNOWLEDGE_REVIEW)
            .with_entity("plugin", plugin_id)
            .with_detail(
                json!({
                    "to": to.as_str(),
                    "item_keys": item_keys,
                    "updated": updated,
                })
                .to_string(),
            ),
    );
    Ok(updated)
}

/// 某插件当前各审核状态行数（CLI 展示与对账）。
pub fn review_counts(pool: &Pool, plugin_id: &str) -> Result<Vec<(String, i64)>, RuntimeError> {
    pool.with_conn(|c| {
        let mut stmt = c.prepare(
            "SELECT review_status, COUNT(*) FROM knowledge_source_entry
             WHERE plugin_id = ?1 GROUP BY review_status ORDER BY review_status",
        )?;
        let rows = stmt
            .query_map([plugin_id], |r| {
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
