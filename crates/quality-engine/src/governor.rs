//! 状态机落库执行器 + VERIFIED 人工守卫。
//!
//! 守卫语义（任务口径"任何更新路径对 VERIFIED 行返回 NeedReview 错误"）：
//! - [`Governor::update_resource`]（属性/内容更新，AI 富化与自动管道的唯一
//!   更新入口）对 lifecycle=VERIFIED 的行一律拒绝，返回 `NeedReview`。
//!   人工放行路径：review 决议后由人工通道执行（decide 落 review 表）。
//! - 生命周期跃迁走状态机表：`VERIFIED → INDEXED` 合法（索引流水线推进，
//!   不改动已验证内容）；`VERIFIED → 其他` 均为表外非法路径。
//! - 治理轴与生命周期正交（K-08）：隔离/拒绝不改动已验证内容，不受该守卫
//!   约束（隔离一个 VERIFIED 资源是合法治理动作）。

use crate::error::GovernanceError;
use crate::state::{governance_transition_allowed, lifecycle_transition_allowed};
use database::{rusqlite, Pool};
use domain::{GovernanceStatus, LifecycleStatus};
use rusqlite::OptionalExtension;

/// 资源属性更新集（None = 不改）。这些列构成"内容面"，受 VERIFIED 守卫保护。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ResourceUpdate {
    /// 名称。
    pub name: Option<String>,
    /// 描述。
    pub description: Option<String>,
    /// 规范化 URL。
    pub canonical_url: Option<String>,
    /// 质量终分。
    pub quality_final_score: Option<f64>,
    /// 质量等级（A/B/C/D）。
    pub quality_grade: Option<String>,
    /// 质量采用门槛。
    pub quality_adoption_blocked: Option<bool>,
    /// 质量理由（JSON 文本）。
    pub quality_reasons: Option<String>,
}

impl ResourceUpdate {
    fn is_empty(&self) -> bool {
        *self == ResourceUpdate::default()
    }

    /// 展开为 (列名, SQL 值) 对。
    fn assignments(&self) -> Vec<(&'static str, rusqlite::types::Value)> {
        use rusqlite::types::Value;
        let mut out = Vec::new();
        if let Some(v) = &self.name {
            out.push(("name", Value::Text(v.clone())));
        }
        if let Some(v) = &self.description {
            out.push(("description", Value::Text(v.clone())));
        }
        if let Some(v) = &self.canonical_url {
            out.push(("canonical_url", Value::Text(v.clone())));
        }
        if let Some(v) = &self.quality_final_score {
            out.push(("quality_final_score", Value::Real(*v)));
        }
        if let Some(v) = &self.quality_grade {
            out.push(("quality_grade", Value::Text(v.clone())));
        }
        if let Some(v) = &self.quality_adoption_blocked {
            out.push(("quality_adoption_blocked", Value::Integer(i64::from(*v))));
        }
        if let Some(v) = &self.quality_reasons {
            out.push(("quality_reasons", Value::Text(v.clone())));
        }
        out
    }
}

/// 治理执行器（面向含 `resource` 表的可写库）。
#[derive(Clone)]
pub struct Governor {
    pool: Pool,
}

impl Governor {
    /// 用可写连接池构造。
    pub fn new(pool: Pool) -> Self {
        Self { pool }
    }

    /// 读取当前生命周期（不存在则 NotFound）。
    pub fn current_lifecycle(&self, resource_id: &str) -> Result<LifecycleStatus, GovernanceError> {
        let raw = self
            .pool
            .with_conn(|c| {
                c.query_row(
                    "SELECT lifecycle FROM resource WHERE resource_id = ?1",
                    [resource_id],
                    |r| r.get::<_, String>(0),
                )
                .optional()
            })?
            .ok_or_else(|| GovernanceError::NotFound(resource_id.to_string()))?;
        Ok(LifecycleStatus::parse(&raw)?)
    }

    /// 读取当前治理状态（不存在则 NotFound）。
    pub fn current_governance(
        &self,
        resource_id: &str,
    ) -> Result<GovernanceStatus, GovernanceError> {
        let raw = self
            .pool
            .with_conn(|c| {
                c.query_row(
                    "SELECT governance FROM resource WHERE resource_id = ?1",
                    [resource_id],
                    |r| r.get::<_, String>(0),
                )
                .optional()
            })?
            .ok_or_else(|| GovernanceError::NotFound(resource_id.to_string()))?;
        Ok(GovernanceStatus::parse(&raw)?)
    }

    /// 应用生命周期跃迁：先校验状态机表，非法路径返回 `IllegalTransition`。
    /// 合法则更新 lifecycle 与 updated_at（单事务）。
    pub fn apply_lifecycle(
        &self,
        resource_id: &str,
        to: LifecycleStatus,
    ) -> Result<(LifecycleStatus, LifecycleStatus), GovernanceError> {
        let from = self.current_lifecycle(resource_id)?;
        if !lifecycle_transition_allowed(from, to) {
            return Err(GovernanceError::IllegalTransition {
                axis: "lifecycle",
                from: from.to_string(),
                to: to.to_string(),
            });
        }
        self.pool.with_tx(|c| -> rusqlite::Result<()> {
            c.execute(
                "UPDATE resource SET lifecycle = ?1, updated_at = ?2 WHERE resource_id = ?3",
                rusqlite::params![to.as_str(), domain::time::now_iso(), resource_id],
            )?;
            Ok(())
        })?;
        Ok((from, to))
    }

    /// 应用治理轴跃迁（K-08 正交轴，独立于生命周期校验）。
    pub fn apply_governance(
        &self,
        resource_id: &str,
        to: GovernanceStatus,
    ) -> Result<(GovernanceStatus, GovernanceStatus), GovernanceError> {
        let from = self.current_governance(resource_id)?;
        if !governance_transition_allowed(from, to) {
            return Err(GovernanceError::IllegalTransition {
                axis: "governance",
                from: from.to_string(),
                to: to.to_string(),
            });
        }
        self.pool.with_tx(|c| -> rusqlite::Result<()> {
            c.execute(
                "UPDATE resource SET governance = ?1, updated_at = ?2 WHERE resource_id = ?3",
                rusqlite::params![to.as_str(), domain::time::now_iso(), resource_id],
            )?;
            Ok(())
        })?;
        Ok((from, to))
    }

    /// **VERIFIED 守卫入口**：属性/内容更新。
    ///
    /// 对 lifecycle=VERIFIED 的行返回 `NeedReview`（人工验证结果不可被 AI 或
    /// 自动管道覆盖）；其余行按更新集执行（空更新集拒绝）。
    pub fn update_resource(
        &self,
        resource_id: &str,
        update: &ResourceUpdate,
    ) -> Result<(), GovernanceError> {
        if update.is_empty() {
            return Err(GovernanceError::InvalidDecision(
                "empty resource update".to_string(),
            ));
        }
        let lifecycle = self.current_lifecycle(resource_id)?;
        if lifecycle == LifecycleStatus::Verified {
            return Err(GovernanceError::NeedReview {
                resource_id: resource_id.to_string(),
                lifecycle: lifecycle.to_string(),
                note: "VERIFIED 行受人工守卫保护：任何属性更新路径被拒绝，\
                       需经 review 决议后由人工通道变更"
                    .to_string(),
            });
        }
        let assignments = update.assignments();
        let set_clause = assignments
            .iter()
            .enumerate()
            .map(|(i, (col, _))| format!("{col} = ?{}", i + 1))
            .collect::<Vec<_>>()
            .join(", ");
        let sql = format!(
            "UPDATE resource SET {set_clause}, updated_at = ?{n} WHERE resource_id = ?{last}",
            n = assignments.len() + 1,
            last = assignments.len() + 2
        );
        self.pool.with_tx(|c| -> rusqlite::Result<()> {
            let values: Vec<rusqlite::types::Value> = assignments
                .iter()
                .map(|(_, v)| v.clone())
                .chain(std::iter::once(rusqlite::types::Value::Text(
                    domain::time::now_iso(),
                )))
                .chain(std::iter::once(rusqlite::types::Value::Text(
                    resource_id.to_string(),
                )))
                .collect();
            c.execute(&sql, rusqlite::params_from_iter(values.iter()))?;
            Ok(())
        })?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use database::{DbPath, PoolConfig};
    use std::time::Duration;

    /// 内存库 + 最小 resource 表（001_init.sql 的治理相关列子集）。
    fn fixture_governor() -> Governor {
        let pool = Pool::open(PoolConfig {
            path: DbPath::Memory,
            readonly: false,
            max_connections: 1,
            busy_timeout: Duration::from_secs(1),
        })
        .unwrap();
        pool.with_conn(|c| {
            c.execute_batch(
                "CREATE TABLE resource (
                   resource_id TEXT PRIMARY KEY, canonical_url TEXT NOT NULL,
                   name TEXT NOT NULL, description TEXT,
                   lifecycle TEXT NOT NULL, governance TEXT NOT NULL,
                   quality_final_score REAL, quality_grade TEXT,
                   quality_adoption_blocked INTEGER, quality_reasons TEXT,
                   updated_at TEXT NOT NULL);
                 INSERT INTO resource (resource_id, canonical_url, name, lifecycle,
                   governance, updated_at) VALUES
                   ('gh:a/verified', 'https://github.com/a/verified', 'Verified Thing',
                     'VERIFIED', 'ACTIVE', '2026-01-01T00:00:00Z'),
                   ('gh:b/indexed', 'https://github.com/b/indexed', 'Indexed Thing',
                     'INDEXED', 'ACTIVE', '2026-01-01T00:00:00Z'),
                   ('gh:c/candidate', 'https://github.com/c/candidate', 'Candidate Thing',
                     'CANDIDATE', 'ACTIVE', '2026-01-01T00:00:00Z'),
                   ('gh:d/stale', 'https://github.com/d/stale', 'Stale Thing',
                     'STALE', 'ACTIVE', '2026-01-01T00:00:00Z');",
            )
        })
        .unwrap();
        Governor::new(pool)
    }

    #[test]
    fn verified_guard_rejects_any_attribute_update() {
        let gov = fixture_governor();
        let upd = ResourceUpdate {
            name: Some("AI-renamed".to_string()),
            quality_final_score: Some(99.0),
            ..ResourceUpdate::default()
        };
        let err = gov.update_resource("gh:a/verified", &upd).unwrap_err();
        assert!(
            matches!(err, GovernanceError::NeedReview { ref resource_id, ref lifecycle, .. }
                if resource_id == "gh:a/verified" && lifecycle == "VERIFIED"),
            "got {err:?}"
        );
        // 守卫是拒绝而非部分写入：内容不变
        let name: String = gov
            .pool
            .with_conn(|c| {
                c.query_row(
                    "SELECT name FROM resource WHERE resource_id='gh:a/verified'",
                    [],
                    |r| r.get(0),
                )
            })
            .unwrap();
        assert_eq!(name, "Verified Thing");
    }

    #[test]
    fn non_verified_rows_updatable() {
        let gov = fixture_governor();
        let upd = ResourceUpdate {
            description: Some("enriched".to_string()),
            quality_final_score: Some(75.0),
            quality_grade: Some("C".to_string()),
            ..ResourceUpdate::default()
        };
        gov.update_resource("gh:b/indexed", &upd).unwrap();
        let (desc, score): (String, f64) = gov
            .pool
            .with_conn(|c| {
                c.query_row(
                    "SELECT description, quality_final_score FROM resource
                     WHERE resource_id='gh:b/indexed'",
                    [],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
            })
            .unwrap();
        assert_eq!(desc, "enriched");
        assert_eq!(score, 75.0);
    }

    #[test]
    fn lifecycle_machine_transitions() {
        let gov = fixture_governor();
        // 合法：CANDIDATE -> VERIFIED
        let (from, to) = gov
            .apply_lifecycle("gh:c/candidate", LifecycleStatus::Verified)
            .unwrap();
        assert_eq!(
            (from, to),
            (LifecycleStatus::Candidate, LifecycleStatus::Verified)
        );
        // 非法：VERIFIED -> CANDIDATE（回退）
        let err = gov
            .apply_lifecycle("gh:c/candidate", LifecycleStatus::Candidate)
            .unwrap_err();
        assert!(matches!(
            err,
            GovernanceError::IllegalTransition {
                axis: "lifecycle",
                ..
            }
        ));
        // 非法：STALE 跳段直达 INDEXED（必须重走 fetch 链）
        let err2 = gov
            .apply_lifecycle("gh:d/stale", LifecycleStatus::Indexed)
            .unwrap_err();
        assert!(matches!(err2, GovernanceError::IllegalTransition { .. }));
        // 合法：VERIFIED -> INDEXED（索引流水线推进，守卫不拦生命周期推进）
        gov.apply_lifecycle("gh:c/candidate", LifecycleStatus::Indexed)
            .unwrap();
        // 合法：INDEXED -> STALE -> FETCHED（重扫循环）
        gov.apply_lifecycle("gh:c/candidate", LifecycleStatus::Stale)
            .unwrap();
        gov.apply_lifecycle("gh:c/candidate", LifecycleStatus::Fetched)
            .unwrap();
        // 不存在资源
        assert!(matches!(
            gov.apply_lifecycle("gh:missing/x", LifecycleStatus::Fetched),
            Err(GovernanceError::NotFound(_))
        ));
    }

    #[test]
    fn governance_axis_orthogonal_and_quarantine_works_on_verified() {
        let gov = fixture_governor();
        // K-08 正交：VERIFIED 行可被隔离（治理动作不改内容）
        gov.apply_governance("gh:a/verified", GovernanceStatus::Quarantined)
            .unwrap();
        assert_eq!(
            gov.current_governance("gh:a/verified").unwrap(),
            GovernanceStatus::Quarantined
        );
        // 守卫仍拦属性更新
        let err = gov
            .update_resource(
                "gh:a/verified",
                &ResourceUpdate {
                    name: Some("x".to_string()),
                    ..ResourceUpdate::default()
                },
            )
            .unwrap_err();
        assert!(matches!(err, GovernanceError::NeedReview { .. }));
        // 非法治理跃迁：QUARANTINED -> ARCHIVED
        let err2 = gov
            .apply_governance("gh:a/verified", GovernanceStatus::Archived)
            .unwrap_err();
        assert!(matches!(
            err2,
            GovernanceError::IllegalTransition {
                axis: "governance",
                ..
            }
        ));
        // 释放回 ACTIVE 合法
        gov.apply_governance("gh:a/verified", GovernanceStatus::Active)
            .unwrap();
    }

    #[test]
    fn empty_update_rejected() {
        let gov = fixture_governor();
        let err = gov
            .update_resource("gh:b/indexed", &ResourceUpdate::default())
            .unwrap_err();
        assert!(matches!(err, GovernanceError::InvalidDecision(_)));
    }
}
