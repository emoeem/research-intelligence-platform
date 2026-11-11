//! 插件治理面（P6-2：治理对象从 Resource 扩到 Plugin，PLUGIN-CONTRACT
//! §11.5 既有裁定：盘点 83、203 行）。
//!
//! 职责：插件行的治理轴（K-08 正交轴，词表与 resource.governance 同源：
//! ACTIVE/QUARANTINED/REJECTED/ARCHIVED）跃迁校验复用 [`crate::state`]
//! 双轴状态机既有表（不复制词表），并给出 INVALID 处置入口：治理轴
//! QUARANTINED + Review 队列 PENDING 提案（复用 [`crate::review::ReviewStore`]
//! 队列骨架，幂等）。
//!
//! 边界：生命周期 state 列（INVALID 属 lifecycle 侧）由 plugin-runtime
//! 驱动；本模块只操作治理轴列，两轴互不越界（§11.5 与 K-08 口径）。

use database::{rusqlite, Pool};
use domain::{GovernanceStatus, PluginState};
use serde::Serialize;

use crate::error::GovernanceError;
use crate::review::{Proposal, ProposeOutcome, ReviewStore};
use crate::state::governance_transition_allowed;

/// INVALID 处置结果（机器可读）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct InvalidDisposition {
    /// 本调用是否实际执行了治理轴跃迁（已隔离时幂等跳过）。
    pub quarantined: bool,
    /// 处置前治理状态。
    pub from_governance: String,
    /// Review 提案 ID（quality-engine:{source_pk} 形）。
    pub review_id: String,
    /// 提案入队结果。
    pub review_outcome: String,
}

/// 插件治理器（操作 plugin.governance 列；只读写 plugin 表治理轴列）。
#[derive(Debug, Clone)]
pub struct PluginGovernor {
    pool: Pool,
}

impl PluginGovernor {
    /// 打开治理器。
    pub fn open(pool: Pool) -> Self {
        Self { pool }
    }

    /// 当前治理状态（active 版本行）。
    pub fn current_governance(&self, plugin_id: &str) -> Result<GovernanceStatus, GovernanceError> {
        let raw: String = self
            .pool
            .with_conn(|c| {
                c.query_row(
                    "SELECT governance FROM plugin WHERE plugin_id = ?1 AND is_active = 1",
                    [plugin_id],
                    |r| r.get::<_, String>(0),
                )
                .map_err(database::DbError::from)
            })
            .map_err(GovernanceError::Db)?;
        GovernanceStatus::parse(&raw).map_err(GovernanceError::Domain)
    }

    /// 应用治理轴跃迁：复用双轴状态机既有表校验，合法则更新 governance 列。
    pub fn apply_governance(
        &self,
        plugin_id: &str,
        to: GovernanceStatus,
    ) -> Result<(GovernanceStatus, GovernanceStatus), GovernanceError> {
        let from = self.current_governance(plugin_id)?;
        if !governance_transition_allowed(from, to) {
            return Err(GovernanceError::IllegalTransition {
                axis: "governance",
                from: from.to_string(),
                to: to.to_string(),
            });
        }
        self.pool
            .with_tx(|c| -> rusqlite::Result<()> {
                c.execute(
                    "UPDATE plugin SET governance = ?1, updated_at = ?2 WHERE plugin_id = ?3 AND is_active = 1",
                    rusqlite::params![to.as_str(), domain::time::now_iso(), plugin_id],
                )?;
                Ok(())
            })
            .map_err(GovernanceError::Db)?;
        Ok((from, to))
    }

    /// INVALID 处置（§11.5）：生命周期态为 INVALID 的插件，治理轴隔离进
    /// QUARANTINED 并向 Review 队列入队 PENDING 提案（幂等：同 source_pk
    /// 已在队即跳过）。生命周期列由调用方（plugin-runtime/CLI）先置 INVALID。
    pub fn handle_invalid(
        &self,
        plugin_id: &str,
        reason: &str,
        reviews: &ReviewStore,
    ) -> Result<InvalidDisposition, GovernanceError> {
        let lifecycle_raw: String = self
            .pool
            .with_conn(|c| {
                c.query_row(
                    "SELECT state FROM plugin WHERE plugin_id = ?1 AND is_active = 1",
                    [plugin_id],
                    |r| r.get::<_, String>(0),
                )
                .map_err(database::DbError::from)
            })
            .map_err(GovernanceError::Db)?;
        // 处置守卫：只对生命周期 INVALID 的行处置（防止误隔离健康插件）。
        let state = PluginState::parse(&lifecycle_raw).map_err(GovernanceError::Domain)?;
        if state != PluginState::Invalid {
            return Err(GovernanceError::InvalidDecision(format!(
                "plugin {plugin_id:?} lifecycle state is {state}, INVALID disposition requires INVALID"
            )));
        }
        let from = self.current_governance(plugin_id)?;
        let mut quarantined = false;
        if from == GovernanceStatus::Active {
            self.apply_governance(plugin_id, GovernanceStatus::Quarantined)?;
            quarantined = true;
        }
        let proposal = Proposal::pending(
            "plugin_invalid",
            plugin_id,
            "plugin-runtime",
            &format!("plugin-invalid:{plugin_id}"),
        );
        let proposal = Proposal {
            reason: Some(reason.to_string()),
            ..proposal
        };
        let outcome = reviews.propose(&proposal)?;
        Ok(InvalidDisposition {
            quarantined,
            from_governance: from.to_string(),
            review_id: proposal.review_id(),
            review_outcome: match outcome {
                ProposeOutcome::Inserted => "inserted".to_string(),
                ProposeOutcome::AlreadyPresent => "already_present".to_string(),
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use database::{DbPath, Pool, PoolConfig};
    use std::time::Duration;

    fn memory_pool() -> Pool {
        Pool::open(PoolConfig {
            path: DbPath::Memory,
            readonly: false,
            max_connections: 1,
            busy_timeout: Duration::from_secs(5),
        })
        .unwrap()
    }

    fn seeded_plugin(pool: &Pool, state: &str) {
        pool.with_conn(|c| {
            c.execute(
                "INSERT INTO plugin (plugin_id, version, name, kind, description, license,
                    trust_level, state, is_active, governance, source_type, source_ref,
                    manifest_json, created_at, updated_at)
                 VALUES ('k.test','0.1.0','T','knowledge','d','MIT','local',?1,1,'ACTIVE',
                         'local_dir','p','{}','2026-01-01','2026-01-01')",
                rusqlite::params![state],
            )
        })
        .unwrap();
    }

    #[test]
    fn governance_reuses_dual_axis_table() {
        let pool = memory_pool();
        database::migrate(&pool, std::path::Path::new("../../migrations")).unwrap();
        seeded_plugin(&pool, "INVALID");
        let governor = PluginGovernor::open(pool.clone());
        let reviews = ReviewStore::open(pool.clone()).unwrap();
        let disposition = governor
            .handle_invalid("k.test", "manifest broken", &reviews)
            .unwrap();
        assert!(disposition.quarantined);
        assert_eq!(disposition.from_governance, "ACTIVE");
        assert_eq!(disposition.review_outcome, "inserted");
        // 幂等重放：治理轴已隔离则不重复跃迁，review 幂等跳过。
        let again = governor
            .handle_invalid("k.test", "manifest broken", &reviews)
            .unwrap();
        assert!(!again.quarantined);
        assert_eq!(again.review_outcome, "already_present");
        // 治理轴状态已 QUARANTINED。
        assert_eq!(
            governor.current_governance("k.test").unwrap(),
            GovernanceStatus::Quarantined
        );
    }

    #[test]
    fn non_invalid_state_refused() {
        let pool = memory_pool();
        database::migrate(&pool, std::path::Path::new("../../migrations")).unwrap();
        seeded_plugin(&pool, "REGISTERED");
        let governor = PluginGovernor::open(pool.clone());
        let reviews = ReviewStore::open(pool).unwrap();
        let err = governor
            .handle_invalid("k.test", "should refuse", &reviews)
            .unwrap_err();
        assert!(err.to_string().contains("INVALID"));
    }
}
