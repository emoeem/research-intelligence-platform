//! ToolRun 生命周期状态枚举（2.0 新实体在 domain 的第一批落地物之一）。
//!
//! 口径（TOOL-CONTRACT §3.1 状态清单与 §3.2 跃迁表，PHASE-ROADMAP §2.3 P1-2）：
//! QUEUED 为初始态，SUCCEEDED / FAILED / TIMED_OUT / CANCELLED 为终态且不可迁出，
//! WAITING_FOR_RUNNER 插入在 QUEUED 与 PREPARING 之间（计划 §5.6:336-337
//! 「VM 离线时任务进入 WAITING_FOR_RUNNER，不伪装成工具失败」）。
//! 跃迁合法性校验属 tool-engine（P1-5），本枚举只承载状态身份与终态判定。

use crate::error::DomainError;
use std::fmt;

/// ToolRun 生命周期状态（TOOL-CONTRACT §3.1 八态）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ToolRunStatus {
    /// 初始：调用请求通过提交校验（D-TC-05），ToolRun 行落库。
    Queued,
    /// 非终态：目标 runtime node 离线或不可达，等待 runner。
    WaitingForRunner,
    /// 非终态：Runner 已分配，解析 Artifact 引用、确认镜像 digest、注入 secrets。
    Preparing,
    /// 非终态：执行体已启动（python_worker 完成 initialize 且 execute 已送达；容器已启动）。
    Running,
    /// 终态：结果信封通过 output_schema 校验并交付。
    Succeeded,
    /// 终态：任务级错误（EXECUTION/INPUT/VALIDATION/POLICY_DENIED/DEPENDENCY 等，§5.3）。
    Failed,
    /// 终态：超过 resources.timeout_seconds（python_worker 侧对应 status=timeout）。
    TimedOut,
    /// 终态：协作式取消生效且任务终止（python_worker 侧 error.code=-32004）。
    Cancelled,
}

impl ToolRunStatus {
    /// 全部变体（按 §3.1 状态清单顺序）。
    pub const ALL: [ToolRunStatus; 8] = [
        ToolRunStatus::Queued,
        ToolRunStatus::WaitingForRunner,
        ToolRunStatus::Preparing,
        ToolRunStatus::Running,
        ToolRunStatus::Succeeded,
        ToolRunStatus::Failed,
        ToolRunStatus::TimedOut,
        ToolRunStatus::Cancelled,
    ];

    /// 存储值（大写下划线风格，与 §3.1 状态名一致，供 tool_run 表 status 列与审计事件使用）。
    pub fn as_str(&self) -> &'static str {
        match self {
            ToolRunStatus::Queued => "QUEUED",
            ToolRunStatus::WaitingForRunner => "WAITING_FOR_RUNNER",
            ToolRunStatus::Preparing => "PREPARING",
            ToolRunStatus::Running => "RUNNING",
            ToolRunStatus::Succeeded => "SUCCEEDED",
            ToolRunStatus::Failed => "FAILED",
            ToolRunStatus::TimedOut => "TIMED_OUT",
            ToolRunStatus::Cancelled => "CANCELLED",
        }
    }

    /// 从存储值解析（大小写不敏感，与 LifecycleStatus::parse 同风格）。
    pub fn parse(value: &str) -> Result<Self, DomainError> {
        let upper = value.to_ascii_uppercase();
        Self::ALL
            .into_iter()
            .find(|s| s.as_str() == upper)
            .ok_or_else(|| DomainError::UnknownToolRunStatus(value.to_string()))
    }

    /// 是否终态（§3.2：终态不可迁出，修正需新 run）。
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            ToolRunStatus::Succeeded
                | ToolRunStatus::Failed
                | ToolRunStatus::TimedOut
                | ToolRunStatus::Cancelled
        )
    }
}

impl fmt::Display for ToolRunStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for ToolRunStatus {
    type Err = DomainError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        ToolRunStatus::parse(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn covers_eight_states() {
        assert_eq!(ToolRunStatus::ALL.len(), 8);
        assert_eq!(ToolRunStatus::Queued.as_str(), "QUEUED");
        assert_eq!(
            ToolRunStatus::WaitingForRunner.as_str(),
            "WAITING_FOR_RUNNER"
        );
        assert_eq!(ToolRunStatus::Preparing.as_str(), "PREPARING");
        assert_eq!(ToolRunStatus::Running.as_str(), "RUNNING");
        assert_eq!(ToolRunStatus::Succeeded.as_str(), "SUCCEEDED");
        assert_eq!(ToolRunStatus::Failed.as_str(), "FAILED");
        assert_eq!(ToolRunStatus::TimedOut.as_str(), "TIMED_OUT");
        assert_eq!(ToolRunStatus::Cancelled.as_str(), "CANCELLED");
    }

    #[test]
    fn parse_roundtrip_case_insensitive() {
        for s in ToolRunStatus::ALL {
            assert_eq!(ToolRunStatus::parse(s.as_str()).unwrap(), s);
            let lower = s.as_str().to_ascii_lowercase();
            assert_eq!(ToolRunStatus::parse(&lower).unwrap(), s);
        }
        assert!(ToolRunStatus::parse("FINISHED").is_err());
        assert!(ToolRunStatus::parse("").is_err());
    }

    #[test]
    fn only_four_terminal_states() {
        assert!(!ToolRunStatus::Queued.is_terminal());
        assert!(!ToolRunStatus::WaitingForRunner.is_terminal());
        assert!(!ToolRunStatus::Preparing.is_terminal());
        assert!(!ToolRunStatus::Running.is_terminal());
        assert!(ToolRunStatus::Succeeded.is_terminal());
        assert!(ToolRunStatus::Failed.is_terminal());
        assert!(ToolRunStatus::TimedOut.is_terminal());
        assert!(ToolRunStatus::Cancelled.is_terminal());
    }
}
