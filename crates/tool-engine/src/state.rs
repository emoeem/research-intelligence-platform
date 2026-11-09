//! ToolRun 状态机跃迁表（TOOL-CONTRACT §3.2，纯函数，无存储依赖）。
//!
//! 跃迁 13 行与 §3.2 表一一对应：初始落库（无 → QUEUED）1 行 + 12 行
//! 状态对（其中 RUNNING → SUCCEEDED 出现两次：第 9 行常规交付与第 13 行
//! 协作式取消后任务自行完成，D-TC-04，去重后 11 个合法状态对）。
//! 终态不可迁出（§3.2 末行）；QUEUED → FAILED / QUEUED → CANCELLED 等
//! 未列入表的跃迁一律非法。

use domain::ToolRunStatus;

/// 全部合法状态对（§3.2 表第 2 到 12 行；初始落库行由提交受理承载）。
pub const LEGAL_TRANSITIONS: [(ToolRunStatus, ToolRunStatus); 11] = [
    // 第 2 行：调度器选出可用 runner
    (ToolRunStatus::Queued, ToolRunStatus::Preparing),
    // 第 3 行：无可用 runner（VM 离线），submit_key 保留
    (ToolRunStatus::Queued, ToolRunStatus::WaitingForRunner),
    // 第 4 行：心跳恢复，runner 可用
    (ToolRunStatus::WaitingForRunner, ToolRunStatus::Preparing),
    // 第 5 行：等待期内被取消
    (ToolRunStatus::WaitingForRunner, ToolRunStatus::Cancelled),
    // 第 6 行：执行体启动成功
    (ToolRunStatus::Preparing, ToolRunStatus::Running),
    // 第 7 行：镜像缺失/digest 不符/POLICY_DENIED/INPUT 解析失败
    (ToolRunStatus::Preparing, ToolRunStatus::Failed),
    // 第 8 行：拉取镜像时 node 失联
    (ToolRunStatus::Preparing, ToolRunStatus::WaitingForRunner),
    // 第 9 行：status=result 且 output_schema 校验通过
    (ToolRunStatus::Running, ToolRunStatus::Succeeded),
    // 第 10 行：status=error / 容器非零退出 / 结构化结果校验失败
    (ToolRunStatus::Running, ToolRunStatus::Failed),
    // 第 11 行：status=timeout / 平台侧超时兜底
    (ToolRunStatus::Running, ToolRunStatus::TimedOut),
    // 第 12 行：cancel 生效且任务终止（-32004）
    (ToolRunStatus::Running, ToolRunStatus::Cancelled),
];

/// 跃迁是否合法（含终态不可迁出；自迁移非法）。
pub fn is_legal(from: ToolRunStatus, to: ToolRunStatus) -> bool {
    if from.is_terminal() || from == to {
        return false;
    }
    LEGAL_TRANSITIONS.iter().any(|&(f, t)| f == from && t == to)
}

/// 协作式取消（D-TC-04）可发起请求的状态：RUNNING（第 12/13 行）与
/// WAITING_FOR_RUNNER（第 5 行）。QUEUED/PREPARING 的取消未列入 §3.2 表，
/// 一律不支持（同步拒绝）。
pub fn cancel_requestable(status: ToolRunStatus) -> bool {
    matches!(
        status,
        ToolRunStatus::Running | ToolRunStatus::WaitingForRunner
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use ToolRunStatus::*;

    #[test]
    fn legal_pairs_match_contract_table() {
        assert_eq!(LEGAL_TRANSITIONS.len(), 11);
        for (from, to) in LEGAL_TRANSITIONS {
            assert!(is_legal(from, to), "{from:?} -> {to:?} must be legal");
        }
        // 第 13 行（取消后自行完成）与第 9 行同状态对，仅语义不同。
        assert!(is_legal(Running, Succeeded));
    }

    #[test]
    fn terminal_states_cannot_leave() {
        for t in [Succeeded, Failed, TimedOut, Cancelled] {
            for to in [
                Queued,
                WaitingForRunner,
                Preparing,
                Running,
                Succeeded,
                Failed,
                TimedOut,
                Cancelled,
            ] {
                assert!(!is_legal(t, to), "terminal {t:?} -> {to:?} must be illegal");
            }
        }
    }

    #[test]
    fn unlisted_pairs_are_illegal() {
        assert!(!is_legal(Queued, Failed));
        assert!(!is_legal(Queued, Cancelled));
        assert!(!is_legal(Queued, Succeeded));
        assert!(!is_legal(Queued, Running));
        assert!(!is_legal(Preparing, Succeeded));
        assert!(!is_legal(Preparing, TimedOut));
        assert!(!is_legal(Preparing, Cancelled));
        assert!(!is_legal(WaitingForRunner, Failed));
        assert!(!is_legal(WaitingForRunner, Running));
        assert!(!is_legal(WaitingForRunner, Succeeded));
        assert!(!is_legal(Running, Preparing));
        assert!(!is_legal(Running, Queued));
        assert!(!is_legal(Running, WaitingForRunner));
    }

    #[test]
    fn cancel_requestable_only_in_two_states() {
        assert!(cancel_requestable(Running));
        assert!(cancel_requestable(WaitingForRunner));
        assert!(!cancel_requestable(Queued));
        assert!(!cancel_requestable(Preparing));
        for t in [Succeeded, Failed, TimedOut, Cancelled] {
            assert!(!cancel_requestable(t));
        }
    }
}
