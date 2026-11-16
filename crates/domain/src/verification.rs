//! 验证编排域状态词表（RIP3 Phase 5，P5-2 契约面）。
//!
//! 口径（Phase 5 补充计划 §5.7 五态纪律 + P5-0-B3 存储判定冻结）：
//! - **结果五态与研究任务状态是两个维度**：[`ResultStatus`] 是单个验证项的
//!   研究结论状态（持久列 research_plan_item.result_status），任务/计划的
//!   运行状态（[`PlanStatus`]）是编排状态，二者互不混用。
//! - **纪律硬约束**（应用层守卫承载，见 crates/verification-engine）：工具
//!   退出码 0 ≠ CONFIRMED；执行失败/超时（error_category 在场）≠
//!   NOT_REPRODUCED；未测试不得计为通过；FALSE_POSITIVE 必须有判定依据。
//! - RTIS 红线：本域不承载 RTIS Finding 事实；rtis_poc_id 只存引用。
//! - 本模块保持 domain 零依赖纪律（MODULE_BOUNDARIES §8.1）：JSON 词值由
//!   as_str/parse 手工承载，信封序列化在 verification-engine 侧做。
//!
//! 跃迁合法性校验属 crates/verification-engine（对齐 tool-engine 分工先例），
//! 本模块只承载状态身份、词表与词值转换。

use crate::error::DomainError;
use std::fmt;

/// 验证项研究结论五态（补充计划 §5.7 表格原文词值）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ResultStatus {
    /// 有足够证据确认（关联或引用 RTIS Finding）。
    Confirmed,
    /// 在本次条件下未复现（保存条件、失败原因和限制）。
    NotReproduced,
    /// 未执行（记录未测试原因和覆盖缺口）。
    NotTested,
    /// 结果不确定、证据不足或存在冲突（补充证据或进入研究分析）。
    Inconclusive,
    /// 经证据判断为误报（保存误报依据和规则质量反馈）。
    FalsePositive,
}

impl ResultStatus {
    /// 全部变体（§5.7 表格顺序）。
    pub const ALL: [ResultStatus; 5] = [
        ResultStatus::Confirmed,
        ResultStatus::NotReproduced,
        ResultStatus::NotTested,
        ResultStatus::Inconclusive,
        ResultStatus::FalsePositive,
    ];

    /// 存储词值（与迁移 020 CHECK 词表一致；JSON 信封同词值）。
    pub fn as_str(&self) -> &'static str {
        match self {
            ResultStatus::Confirmed => "CONFIRMED",
            ResultStatus::NotReproduced => "NOT_REPRODUCED",
            ResultStatus::NotTested => "NOT_TESTED",
            ResultStatus::Inconclusive => "INCONCLUSIVE",
            ResultStatus::FalsePositive => "FALSE_POSITIVE",
        }
    }

    /// 从存储词值解析（大小写敏感，词表封闭）。
    pub fn parse(value: &str) -> Result<Self, DomainError> {
        match value {
            "CONFIRMED" => Ok(ResultStatus::Confirmed),
            "NOT_REPRODUCED" => Ok(ResultStatus::NotReproduced),
            "NOT_TESTED" => Ok(ResultStatus::NotTested),
            "INCONCLUSIVE" => Ok(ResultStatus::Inconclusive),
            "FALSE_POSITIVE" => Ok(ResultStatus::FalsePositive),
            other => Err(DomainError::InvalidVerificationStatus(other.to_string())),
        }
    }
}

impl fmt::Display for ResultStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// 适用性判定三态（Knowledge Matcher 输出，P5-3 承载判定逻辑）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ApplicabilityStatus {
    /// 结构化条件命中，可进入计划。
    Applicable,
    /// 版本/协议/认证/前置条件不适用（排除必须带原因）。
    Excluded,
    /// 信息不足（不强行执行，标记需要补充上下文）。
    NeedsContext,
}

impl ApplicabilityStatus {
    /// 存储词值（迁移 020 CHECK 词表）。
    pub fn as_str(&self) -> &'static str {
        match self {
            ApplicabilityStatus::Applicable => "applicable",
            ApplicabilityStatus::Excluded => "excluded",
            ApplicabilityStatus::NeedsContext => "needs_context",
        }
    }

    /// 从存储词值解析。
    pub fn parse(value: &str) -> Result<Self, DomainError> {
        match value {
            "applicable" => Ok(ApplicabilityStatus::Applicable),
            "excluded" => Ok(ApplicabilityStatus::Excluded),
            "needs_context" => Ok(ApplicabilityStatus::NeedsContext),
            other => Err(DomainError::InvalidVerificationStatus(format!(
                "applicability: {other}"
            ))),
        }
    }
}

/// 验证计划编排状态（与结果五态分离；CHECK 词表同源）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PlanStatus {
    /// 草稿（计划项可增删）。
    Draft,
    /// 定稿待执行（计划项冻结，五态预置 NOT_TESTED）。
    Ready,
    /// 执行中（P5-6 派发）。
    Executing,
    /// 暂停（workflow control 面传播）。
    Paused,
    /// 完成（全部计划项到达终局五态或预算耗尽）。
    Completed,
    /// 取消（cancel 真实传播）。
    Cancelled,
}

impl PlanStatus {
    /// 存储词值（迁移 020 CHECK 词表）。
    pub fn as_str(&self) -> &'static str {
        match self {
            PlanStatus::Draft => "draft",
            PlanStatus::Ready => "ready",
            PlanStatus::Executing => "executing",
            PlanStatus::Paused => "paused",
            PlanStatus::Completed => "completed",
            PlanStatus::Cancelled => "cancelled",
        }
    }

    /// 从存储词值解析。
    pub fn parse(value: &str) -> Result<Self, DomainError> {
        match value {
            "draft" => Ok(PlanStatus::Draft),
            "ready" => Ok(PlanStatus::Ready),
            "executing" => Ok(PlanStatus::Executing),
            "paused" => Ok(PlanStatus::Paused),
            "completed" => Ok(PlanStatus::Completed),
            "cancelled" => Ok(PlanStatus::Cancelled),
            other => Err(DomainError::InvalidVerificationStatus(format!(
                "plan status: {other}"
            ))),
        }
    }

    /// 是否终态（completed/cancelled 不可迁出）。
    pub fn is_terminal(self) -> bool {
        matches!(self, PlanStatus::Completed | PlanStatus::Cancelled)
    }
}

/// 风险分级（默认路径只含低影响、可观测、可中止的检测；补充计划 §3.4）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RiskClass {
    /// 被动观测（不改变目标状态）。
    Passive,
    /// 低影响主动检测（默认路径；请求预算内）。
    LowImpact,
    /// 受控主动检测（显式选择，默认路径不含）。
    ActiveControlled,
}

impl RiskClass {
    /// 存储词值（迁移 020 CHECK 词表）。
    pub fn as_str(&self) -> &'static str {
        match self {
            RiskClass::Passive => "passive",
            RiskClass::LowImpact => "low_impact",
            RiskClass::ActiveControlled => "active_controlled",
        }
    }

    /// 从存储词值解析。
    pub fn parse(value: &str) -> Result<Self, DomainError> {
        match value {
            "passive" => Ok(RiskClass::Passive),
            "low_impact" => Ok(RiskClass::LowImpact),
            "active_controlled" => Ok(RiskClass::ActiveControlled),
            other => Err(DomainError::InvalidVerificationStatus(format!(
                "risk class: {other}"
            ))),
        }
    }
}

/// 执行失败分类（§5.9：网络失败、认证失败、超时、工具不可用分别分类）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ErrorCategory {
    /// 单项/总时长超时。
    Timeout,
    /// 网络不可达/DNS/连接重置。
    Network,
    /// 认证/会话失效。
    Auth,
    /// 工具/Provider 不可用（镜像缺失、runner 离线）。
    ToolUnavailable,
    /// 断言失败（工具正常执行但判据未命中；不等于 NOT_REPRODUCED）。
    Assertion,
    /// 请求/时间预算耗尽。
    BudgetExhausted,
    /// 未分类（兜底，必须带原因文本）。
    Unknown,
}

impl ErrorCategory {
    /// 存储词值（迁移 020 CHECK 词表）。
    pub fn as_str(&self) -> &'static str {
        match self {
            ErrorCategory::Timeout => "timeout",
            ErrorCategory::Network => "network",
            ErrorCategory::Auth => "auth",
            ErrorCategory::ToolUnavailable => "tool_unavailable",
            ErrorCategory::Assertion => "assertion",
            ErrorCategory::BudgetExhausted => "budget_exhausted",
            ErrorCategory::Unknown => "unknown",
        }
    }

    /// 从存储词值解析。
    pub fn parse(value: &str) -> Result<Self, DomainError> {
        match value {
            "timeout" => Ok(ErrorCategory::Timeout),
            "network" => Ok(ErrorCategory::Network),
            "auth" => Ok(ErrorCategory::Auth),
            "tool_unavailable" => Ok(ErrorCategory::ToolUnavailable),
            "assertion" => Ok(ErrorCategory::Assertion),
            "budget_exhausted" => Ok(ErrorCategory::BudgetExhausted),
            "unknown" => Ok(ErrorCategory::Unknown),
            other => Err(DomainError::InvalidVerificationStatus(format!(
                "error category: {other}"
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn result_status_word_table_round_trip() {
        for status in ResultStatus::ALL {
            assert_eq!(ResultStatus::parse(status.as_str()).unwrap(), status);
        }
        assert!(ResultStatus::parse("confirmed").is_err());
        assert!(ResultStatus::parse("").is_err());
        // 词值与迁移 020 CHECK 词表逐字一致。
        assert_eq!(
            ResultStatus::ALL
                .map(|s| s.as_str().to_owned())
                .join(","),
            "CONFIRMED,NOT_REPRODUCED,NOT_TESTED,INCONCLUSIVE,FALSE_POSITIVE"
        );
    }

    #[test]
    fn plan_status_terminal_only_two() {
        assert!(PlanStatus::Completed.is_terminal());
        assert!(PlanStatus::Cancelled.is_terminal());
        assert!(!PlanStatus::Paused.is_terminal());
        assert!(!PlanStatus::Ready.is_terminal());
        assert!(PlanStatus::parse("executing").is_ok());
        assert!(PlanStatus::parse("EXECUTING").is_err());
    }

    #[test]
    fn applicability_and_risk_and_error_round_trip() {
        for (value, expect) in [
            ("applicable", ApplicabilityStatus::Applicable),
            ("excluded", ApplicabilityStatus::Excluded),
            ("needs_context", ApplicabilityStatus::NeedsContext),
        ] {
            assert_eq!(ApplicabilityStatus::parse(value).unwrap(), expect);
        }
        assert_eq!(RiskClass::parse("low_impact").unwrap(), RiskClass::LowImpact);
        assert!(RiskClass::parse("destructive").is_err());
        for value in [
            "timeout",
            "network",
            "auth",
            "tool_unavailable",
            "assertion",
            "budget_exhausted",
            "unknown",
        ] {
            assert!(ErrorCategory::parse(value).is_ok());
        }
        assert!(ErrorCategory::parse("exit_code").is_err());
    }
}
