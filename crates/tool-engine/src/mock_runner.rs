//! Mock Runner（可注入假 runtime，TOOL-CONTRACT §10.1 落地物第 4 行、
//! §10.2 契约测试最小集的驱动面）。
//!
//! 可注入维度：终局四路径（SUCCEEDED/FAILED/TIMED_OUT/CANCELLED）、
//! WAITING_FOR_RUNNER 注入与恢复、等待期取消、准备期失败、幂等命中
//! （cached=true 透传）、runtime 类型指纹。真实 runtime 适配器（P1-6 的
//! python_worker 派发器与 Phase 2 容器 Runner）应复用同一信封与截断路径。

use serde_json::{json, Value};

/// 可注入终局。
#[derive(Debug, Clone)]
pub enum Outcome {
    /// RUNNING → SUCCEEDED（第 9 行）：结构化输出 + 摘要。
    Succeed {
        /// 结构化输出（引擎按 output_schema 校验后交付）。
        result: Value,
        /// 人类可读摘要。
        summary: String,
    },
    /// RUNNING → FAILED（第 10 行）。
    Fail {
        /// §5.3 类别。
        category: String,
        /// 协议/平台错误码。
        code: Value,
        /// 消息。
        message: String,
        /// 可重试性。
        retryable: bool,
    },
    /// RUNNING → TIMED_OUT（第 11 行）。
    Timeout {
        /// 消息。
        message: String,
    },
    /// 取消请求后任务终止（第 12 行，-32004 语义）。
    CancelThenTerminate {
        /// 消息。
        message: String,
    },
    /// 取消请求后任务仍自行完成（第 13 行，D-TC-04 协作式取消语义）。
    CancelButCompletes {
        /// 结构化输出。
        result: Value,
        /// 摘要。
        summary: String,
    },
}

/// Mock Runner 配置（每次 drive 只消费一份）。
#[derive(Debug, Clone)]
pub struct MockRunner {
    /// 注入的终局。
    pub outcome: Outcome,
    /// 环境指纹 runtime_type（默认 python_worker）。
    pub runtime_type: String,
    /// QUEUED → WAITING_FOR_RUNNER 注入（第 3 行）。
    pub wait_for_runner: bool,
    /// WAITING_FOR_RUNNER → PREPARING 恢复（第 4 行）；false 且未注入等待期
    /// 取消时 drive 报 Stuck。
    pub recover_after_wait: bool,
    /// 等待期内取消（第 5 行）。
    pub cancel_while_waiting: bool,
    /// PREPARING → FAILED 注入（第 7 行：镜像缺失/digest 不符/授权缺失）。
    pub fail_in_preparing: bool,
    /// PREPARING → WAITING_FOR_RUNNER 注入（第 8 行：拉取镜像时 node 失联），
    /// 随后按 recover_after_wait 语义回 PREPARING（第 4 行）。
    pub preparing_node_loss: bool,
    /// 准备期失败的 §5.3 类别（默认 POLICY_DENIED）。
    pub preparing_error_category: String,
    /// 幂等命中标记：true 时信封 replay.cached=true（§5.1 cached 透传）。
    pub cached: bool,
}

impl MockRunner {
    /// 常规成功 runner（无注入）。
    pub fn succeed(result: Value, summary: impl Into<String>) -> Self {
        Self {
            outcome: Outcome::Succeed {
                result,
                summary: summary.into(),
            },
            ..Self::bare()
        }
    }

    /// 最小默认配置。
    pub fn bare() -> Self {
        Self {
            outcome: Outcome::Succeed {
                result: json!({}),
                summary: "mock ok".to_string(),
            },
            runtime_type: "python_worker".to_string(),
            wait_for_runner: false,
            recover_after_wait: true,
            cancel_while_waiting: false,
            fail_in_preparing: false,
            preparing_node_loss: false,
            preparing_error_category: "POLICY_DENIED".to_string(),
            cached: false,
        }
    }

    /// 失败 runner（RUNNING → FAILED）。
    pub fn fail(category: &str, code: Value, message: impl Into<String>) -> Self {
        Self {
            outcome: Outcome::Fail {
                category: category.to_string(),
                code,
                message: message.into(),
                retryable: false,
            },
            ..Self::bare()
        }
    }

    /// 超时 runner（RUNNING → TIMED_OUT）。
    pub fn timeout(message: impl Into<String>) -> Self {
        Self {
            outcome: Outcome::Timeout {
                message: message.into(),
            },
            ..Self::bare()
        }
    }
}
