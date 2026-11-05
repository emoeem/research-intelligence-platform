//! 双轴状态机（纯表驱动，无 IO）。
//!
//! 生命周期轴（M0 矛盾修正后的 K-16 口径，与 001_init.sql CHECK 一致）：
//! `DISCOVERED → FETCHED → PARSED → CANDIDATE → VERIFIED → INDEXED / STALE`。
//! 非法跃迁（表外路径、同态自跃迁）一律拒绝。
//!
//! 治理轴（K-08：与生命周期正交）：
//! `ACTIVE ⇄ QUARANTINED`、`ACTIVE → REJECTED / ARCHIVED`、
//! `QUARANTINED → REJECTED`，`REJECTED / ARCHIVED → ACTIVE`（人工复议恢复）。
//!
//! VERIFIED 守卫与状态机的关系：VERIFIED 行的属性覆盖走 [`crate::governor`]
//! 的 `NeedReview` 错误；生命周期推进 `VERIFIED → INDEXED` 是索引流水线的合法
//! 跃迁（不改动已验证内容），`VERIFIED → 其他任何状态` 均为表外非法路径。

use domain::{GovernanceStatus, LifecycleStatus};

/// 生命周期合法跃迁表。
pub const LIFECYCLE_TRANSITIONS: &[(LifecycleStatus, LifecycleStatus)] = &[
    (LifecycleStatus::Discovered, LifecycleStatus::Fetched),
    (LifecycleStatus::Fetched, LifecycleStatus::Parsed),
    (LifecycleStatus::Parsed, LifecycleStatus::Candidate),
    (LifecycleStatus::Candidate, LifecycleStatus::Verified),
    (LifecycleStatus::Verified, LifecycleStatus::Indexed),
    (LifecycleStatus::Indexed, LifecycleStatus::Stale),
    (LifecycleStatus::Stale, LifecycleStatus::Fetched), // 重扫循环
];

/// 治理轴合法跃迁表（K-08 正交轴的治理流转）。
pub const GOVERNANCE_TRANSITIONS: &[(GovernanceStatus, GovernanceStatus)] = &[
    (GovernanceStatus::Active, GovernanceStatus::Quarantined),
    (GovernanceStatus::Active, GovernanceStatus::Rejected),
    (GovernanceStatus::Active, GovernanceStatus::Archived),
    (GovernanceStatus::Quarantined, GovernanceStatus::Active),
    (GovernanceStatus::Quarantined, GovernanceStatus::Rejected),
    (GovernanceStatus::Rejected, GovernanceStatus::Active),
    (GovernanceStatus::Archived, GovernanceStatus::Active),
];

/// 生命周期跃迁是否合法。
pub fn lifecycle_transition_allowed(from: LifecycleStatus, to: LifecycleStatus) -> bool {
    LIFECYCLE_TRANSITIONS
        .iter()
        .any(|(f, t)| *f == from && *t == to)
}

/// 生命周期当前状态的合法目标集（诊断/展示用）。
pub fn lifecycle_allowed_targets(from: LifecycleStatus) -> Vec<LifecycleStatus> {
    LIFECYCLE_TRANSITIONS
        .iter()
        .filter(|(f, _)| *f == from)
        .map(|(_, t)| *t)
        .collect()
}

/// 治理轴跃迁是否合法。
pub fn governance_transition_allowed(from: GovernanceStatus, to: GovernanceStatus) -> bool {
    GOVERNANCE_TRANSITIONS
        .iter()
        .any(|(f, t)| *f == from && *t == to)
}

/// 治理轴当前状态的合法目标集（诊断/展示用）。
pub fn governance_allowed_targets(from: GovernanceStatus) -> Vec<GovernanceStatus> {
    GOVERNANCE_TRANSITIONS
        .iter()
        .filter(|(f, _)| *f == from)
        .map(|(_, t)| *t)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lifecycle_chain_is_exact() {
        // 主链逐段合法
        assert!(lifecycle_transition_allowed(
            LifecycleStatus::Discovered,
            LifecycleStatus::Fetched
        ));
        assert!(lifecycle_transition_allowed(
            LifecycleStatus::Fetched,
            LifecycleStatus::Parsed
        ));
        assert!(lifecycle_transition_allowed(
            LifecycleStatus::Parsed,
            LifecycleStatus::Candidate
        ));
        assert!(lifecycle_transition_allowed(
            LifecycleStatus::Candidate,
            LifecycleStatus::Verified
        ));
        assert!(lifecycle_transition_allowed(
            LifecycleStatus::Verified,
            LifecycleStatus::Indexed
        ));
        assert!(lifecycle_transition_allowed(
            LifecycleStatus::Indexed,
            LifecycleStatus::Stale
        ));
        assert!(lifecycle_transition_allowed(
            LifecycleStatus::Stale,
            LifecycleStatus::Fetched
        ));
        // 跳段非法
        assert!(!lifecycle_transition_allowed(
            LifecycleStatus::Discovered,
            LifecycleStatus::Indexed
        ));
        assert!(!lifecycle_transition_allowed(
            LifecycleStatus::Candidate,
            LifecycleStatus::Fetched
        ));
        // 回退非法（VERIFIED 不可回 CANDIDATE）
        assert!(!lifecycle_transition_allowed(
            LifecycleStatus::Verified,
            LifecycleStatus::Candidate
        ));
        // 同态自跃迁非法
        for s in LifecycleStatus::ALL {
            assert!(!lifecycle_transition_allowed(s, s), "self transition {s}");
        }
        // STALE 不能直达终态（必须重走 fetch 链）
        assert!(!lifecycle_transition_allowed(
            LifecycleStatus::Stale,
            LifecycleStatus::Indexed
        ));
    }

    #[test]
    fn lifecycle_targets_complete() {
        assert_eq!(
            lifecycle_allowed_targets(LifecycleStatus::Verified),
            vec![LifecycleStatus::Indexed]
        );
        assert_eq!(
            lifecycle_allowed_targets(LifecycleStatus::Indexed),
            vec![LifecycleStatus::Stale]
        );
        assert_eq!(
            lifecycle_allowed_targets(LifecycleStatus::Stale),
            vec![LifecycleStatus::Fetched]
        );
    }

    #[test]
    fn governance_axis_transitions() {
        assert!(governance_transition_allowed(
            GovernanceStatus::Active,
            GovernanceStatus::Quarantined
        ));
        assert!(governance_transition_allowed(
            GovernanceStatus::Quarantined,
            GovernanceStatus::Active
        ));
        assert!(governance_transition_allowed(
            GovernanceStatus::Quarantined,
            GovernanceStatus::Rejected
        ));
        assert!(governance_transition_allowed(
            GovernanceStatus::Rejected,
            GovernanceStatus::Active
        ));
        assert!(governance_transition_allowed(
            GovernanceStatus::Archived,
            GovernanceStatus::Active
        ));
        // REJECTED/ARCHIVED 之间与自环非法
        assert!(!governance_transition_allowed(
            GovernanceStatus::Rejected,
            GovernanceStatus::Archived
        ));
        assert!(!governance_transition_allowed(
            GovernanceStatus::Archived,
            GovernanceStatus::Rejected
        ));
        for s in GovernanceStatus::ALL {
            assert!(!governance_transition_allowed(s, s), "self transition {s}");
        }
    }
}
