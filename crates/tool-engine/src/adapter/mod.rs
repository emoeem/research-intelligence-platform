//! Runtime Adapter 层（TOOL-CONTRACT §10.1 落地物第 5 行，ROADMAP P1-6）。
//!
//! 本模块承接 P1-5 引擎骨架与真实 runtime 之间的接缝：
//! - [`python_worker`]：宿主派发器（进程池 + initialize/get_capabilities
//!   握手 + execute/cancel 转发 + stderr 日志采集），修补架构盘点差距第 4 条
//!   「Worker 宿主派发器缺失」（CURRENT-ARCHITECTURE-INVENTORY.md:219）；
//! - §5.3 错误码映射表（[`map_worker_code`]）为 Worker Protocol 码表到
//!   信封 category 的一一对应，全量码均有断言（tests/python_worker_e2e.rs）。

pub mod python_worker;
pub mod structured_output;

pub use python_worker::{
    execute_via_engine, AdapterCall, PythonWorkerAdapter, PythonWorkerConfig, RestartPolicy,
};

use serde_json::Value;

/// 适配器终局（信封组装前的中性形状，错误码保持 worker 协议原码）。
#[derive(Debug, Clone, PartialEq)]
pub enum AdapterOutcome {
    /// status=result（cached 透传进 replay）。
    Succeeded {
        /// 工具结构化输出（data）。
        data: Value,
        /// 幂等命中标记。
        cached: bool,
        /// worker 侧 usage（elapsed_ms/output_bytes 等）。
        usage: Option<Value>,
    },
    /// status=error 或 JSON-RPC error（码映射见 §5.3 表）。
    Failed {
        /// §5.3 类别。
        category: String,
        /// worker 协议原码。
        code: Value,
        /// 消息。
        message: String,
        /// 类别默认可重试性。
        retryable: bool,
    },
    /// status=timeout（BUDGET_TIMEOUT，worker 内强制）或平台侧看门狗到期。
    TimedOut {
        /// 消息。
        message: String,
    },
    /// 协作式取消生效（CANCELLED，-32004）。
    Cancelled {
        /// 消息。
        message: String,
    },
}

/// 传输/握手层失败（INFRA 面：信封以 category=INFRA、retryable=true 交付）。
#[derive(Debug, Clone, PartialEq)]
pub struct AdapterFailure {
    /// 失败说明。
    pub message: String,
}

impl AdapterFailure {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl std::fmt::Display for AdapterFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "adapter failure: {}", self.message)
    }
}

/// Worker Protocol 错误码到 §5.3 类别的一一映射（README.md:88-110 码表）。
/// 返回 (category, retryable 默认值)。INFRA 面默认可重试，其余类别默认不可
/// 重试；TIMEOUT 与 EXECUTION 的「视工具」语义由 Manifest 层覆写（P1-7）。
pub fn map_worker_code(code: i64) -> (&'static str, bool) {
    match code {
        -32602 | -32003 => ("VALIDATION", false),
        -32008 | -32009 => ("INPUT", false),
        -32002 => ("TIMEOUT", false),
        -32004 => ("CANCELLED", false),
        -32006 | -32603 => ("EXECUTION", false),
        // -32700 PARSE_ERROR / -32600 INVALID_REQUEST / -32601 METHOD_NOT_FOUND /
        // -32000 PROTOCOL_VERSION_MISMATCH / -32001 NOT_INITIALIZED /
        // -32005 IDEMPOTENCY_CONFLICT / -32007 UNKNOWN_REQUEST 及未知码
        _ => ("INFRA", true),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_every_code_in_section_5_3_table() {
        // §5.3 表逐项断言（码表来源 README.md:88-110 / worker_runtime ErrorCodes）
        let table: [(i64, &str, bool); 15] = [
            (-32602, "VALIDATION", false), // INVALID_PARAMS
            (-32003, "VALIDATION", false), // BUDGET_INVALID
            (-32008, "INPUT", false),      // INPUT_ERROR
            (-32009, "INPUT", false),      // UNSUPPORTED_FORMAT
            (-32002, "TIMEOUT", false),    // BUDGET_TIMEOUT（视工具，默认否）
            (-32004, "CANCELLED", false),  // CANCELLED
            (-32006, "EXECUTION", false),  // EXECUTION_ERROR（视工具，默认否）
            (-32603, "EXECUTION", false),  // 任务级 INTERNAL_ERROR
            (-32700, "INFRA", true),       // PARSE_ERROR
            (-32600, "INFRA", true),       // INVALID_REQUEST
            (-32601, "INFRA", true),       // METHOD_NOT_FOUND
            (-32000, "INFRA", true),       // PROTOCOL_VERSION_MISMATCH
            (-32001, "INFRA", true),       // NOT_INITIALIZED
            (-32005, "INFRA", true),       // IDEMPOTENCY_CONFLICT
            (-32007, "INFRA", true),       // UNKNOWN_REQUEST
        ];
        for (code, category, retryable) in table {
            let got = map_worker_code(code);
            assert_eq!(got, (category, retryable), "code {code} mapping");
        }
        // 未知码兜底 INFRA
        assert_eq!(map_worker_code(-9999), ("INFRA", true));
    }
}
