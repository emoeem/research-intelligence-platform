//! ToolCallRequest 受理与 D-TC-05 提交校验（TOOL-CONTRACT §4.1、§2.4、§3.2 第 1 行）。
//!
//! 提交校验在落 run 行之前同步执行（§3.2 第 1 行备注：「校验失败同步拒绝，
//! 不产生 run」），覆盖：结构合法性（协议版本、run_id、workspace_id）、
//! 工具存在性与版本解析（Registry）、预算显式值不超过 Manifest 上限
//! （§4.1，超出按 VALIDATION 拒绝）、T2 授权引用缺失按 POLICY_DENIED 拒绝
//! （§2.4）、input 须合 Manifest input_schema（§4.1）。
//! 注意：调用对象没有 secrets 字段（D-TC-06）与宿主机路径字段（§5.5:331），
//! 本模块的结构检查同样拒绝这两类键。

use serde_json::{json, Map, Value};

/// 调用请求（§4.1 字段表；input_refs 为已解析的 artifact 引用数组）。
#[derive(Debug, Clone)]
pub struct SubmitRequest {
    /// 协议主版本（恒 "1"）。
    pub protocol_version: String,
    /// 目标工具 id。
    pub tool_id: String,
    /// 指定版本（缺省解析 Registry current）。
    pub tool_version: Option<String>,
    /// 服务端生成全局唯一 run id。
    pub run_id: String,
    /// 幂等提交键。
    pub submit_key: Option<String>,
    /// 已通过 input_schema 校验的参数。
    pub input: Value,
    /// 工作区隔离边界。
    pub workspace_id: String,
    /// 关联研究任务。
    pub task_id: Option<String>,
    /// 预算覆盖（字段级缺省取 Manifest resources）。
    pub budget: Budget,
    /// 授权引用（T2 必填）。
    pub authorization_ref: Option<String>,
    /// Artifact 引用（artifact_id + mount_as）。
    pub input_refs: Vec<(String, String)>,
}

/// 预算覆盖（§4.1）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Budget {
    /// 显式超时秒。
    pub timeout_seconds: Option<u64>,
    /// 显式输出字节上限。
    pub max_output_bytes: Option<u64>,
}

/// 提交校验拒绝类别（§5.3 同源词表：提交期只产生 VALIDATION 与
/// POLICY_DENIED 两类，§3.2 第 1 行备注同步拒绝）。
#[derive(Debug, Clone, PartialEq)]
pub enum Rejection {
    /// 结构不合法（协议版本/缺字段/禁用字段）。
    Structural(String),
    /// 工具不存在。
    UnknownTool(String),
    /// 版本不可解析（显式版本未登记）。
    UnknownVersion {
        /// 工具 id。
        tool_id: String,
        /// 请求的版本。
        version: String,
    },
    /// run_id 非法。
    InvalidRunId(String),
    /// 预算超 Manifest 上限（VALIDATION）。
    BudgetExceeds(String),
    /// T2 缺授权引用（POLICY_DENIED）。
    MissingAuthorization(String),
    /// input 不合 input_schema（VALIDATION）。
    InputSchemaViolation(String),
}

impl Rejection {
    /// 对应 §5.3 错误类别。
    pub fn category(&self) -> &'static str {
        match self {
            Rejection::MissingAuthorization(_) => "POLICY_DENIED",
            _ => "VALIDATION",
        }
    }

    /// 人类可读原因。
    pub fn reason(&self) -> String {
        match self {
            Rejection::Structural(r) => format!("structural: {r}"),
            Rejection::UnknownTool(t) => format!("unknown tool {t:?}"),
            Rejection::UnknownVersion { tool_id, version } => {
                format!("tool {tool_id} version {version} not registered")
            }
            Rejection::InvalidRunId(r) => format!("invalid run id: {r}"),
            Rejection::BudgetExceeds(r) => format!("budget exceeds manifest cap: {r}"),
            Rejection::MissingAuthorization(t) => {
                format!("permission_level=T2 requires authorization_ref (D-TC-05): {t}")
            }
            Rejection::InputSchemaViolation(r) => format!("input fails input_schema: {r}"),
        }
    }

    /// Worker Protocol 风格错误码（提交期平台侧码）。
    pub fn code(&self) -> Value {
        match self {
            Rejection::MissingAuthorization(_) => json!("POLICY_DENIED"),
            _ => json!(-32602),
        }
    }
}

const REQUEST_KEYS: [&str; 11] = [
    "protocol_version",
    "tool_id",
    "tool_version",
    "run_id",
    "submit_key",
    "input",
    "workspace_id",
    "task_id",
    "budget",
    "result_schema_version",
    "authorization_ref",
];

/// 结构检查（不含 Registry 依赖的部分）：协议版本、必填、禁用键
/// （secrets / 宿主机路径类字段）、run_id 域。合法时返回规整后的请求。
pub fn structural_check(value: &Value) -> Result<SubmitRequest, Rejection> {
    let obj: &Map<String, Value> = value
        .as_object()
        .ok_or_else(|| Rejection::Structural("request must be a JSON object".to_string()))?;
    for key in obj.keys() {
        if !REQUEST_KEYS.contains(&key.as_str()) {
            return Err(Rejection::Structural(format!(
                "unknown request field {key:?} (no secrets/host paths in ToolCallRequest, D-TC-06 and plan 5.5:331)"
            )));
        }
    }
    let protocol_version = obj
        .get("protocol_version")
        .and_then(Value::as_str)
        .ok_or_else(|| Rejection::Structural("protocol_version must be a string".to_string()))?;
    if protocol_version != "1" {
        return Err(Rejection::Structural(format!(
            "protocol_version {protocol_version:?} must be \"1\""
        )));
    }
    let tool_id = obj
        .get("tool_id")
        .and_then(Value::as_str)
        .ok_or_else(|| Rejection::Structural("tool_id must be a string".to_string()))?
        .to_string();
    let run_id = obj
        .get("run_id")
        .and_then(Value::as_str)
        .ok_or_else(|| Rejection::Structural("run_id must be a string".to_string()))?
        .to_string();
    domain::RunId::try_new(run_id.clone()).map_err(|e| Rejection::InvalidRunId(e.to_string()))?;
    let workspace_id = obj
        .get("workspace_id")
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| Rejection::Structural("workspace_id must be a string".to_string()))?;
    if workspace_id.is_empty() {
        return Err(Rejection::Structural(
            "workspace_id must be non-empty".to_string(),
        ));
    }
    let input = obj
        .get("input")
        .and_then(Value::as_object)
        .map(|o| Value::Object(o.clone()))
        .ok_or_else(|| Rejection::Structural("input must be an object".to_string()))?;
    let budget = match obj.get("budget") {
        Some(b) => {
            let bo = b
                .as_object()
                .ok_or_else(|| Rejection::Structural("budget must be an object".to_string()))?;
            Budget {
                timeout_seconds: bo.get("timeout_seconds").and_then(Value::as_u64),
                max_output_bytes: bo.get("max_output_bytes").and_then(Value::as_u64),
            }
        }
        None => Budget {
            timeout_seconds: None,
            max_output_bytes: None,
        },
    };
    let submit_key = obj
        .get("submit_key")
        .and_then(Value::as_str)
        .map(str::to_string);
    if let Some(k) = &submit_key {
        if k.is_empty() || k.chars().count() > 512 {
            return Err(Rejection::Structural(
                "submit_key length must be 1..512 (worker_protocol v1 idempotency_key)".to_string(),
            ));
        }
    }
    let tool_version = obj
        .get("tool_version")
        .and_then(Value::as_str)
        .map(str::to_string);
    let task_id = obj
        .get("task_id")
        .and_then(Value::as_str)
        .map(str::to_string);
    let authorization_ref = obj
        .get("authorization_ref")
        .and_then(Value::as_str)
        .map(str::to_string);
    let input_refs = Vec::new(); // input_refs 数组经 registry Artifact 暂存后入 run 行，mock 提交面暂不内联
    Ok(SubmitRequest {
        protocol_version: protocol_version.to_string(),
        tool_id,
        tool_version,
        run_id,
        submit_key,
        input,
        workspace_id,
        task_id,
        budget,
        authorization_ref,
        input_refs,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn base() -> Value {
        json!({
            "protocol_version": "1",
            "tool_id": "rip.doc.parse",
            "run_id": "rn-01HTXYZEXAMPLE",
            "input": {"text": "x"},
            "workspace_id": "ws-default"
        })
    }

    #[test]
    fn structural_ok() {
        let req = structural_check(&base()).unwrap();
        assert_eq!(req.tool_id, "rip.doc.parse");
        assert_eq!(req.run_id, "rn-01HTXYZEXAMPLE");
        assert_eq!(req.workspace_id, "ws-default");
        assert!(req.budget.timeout_seconds.is_none());
    }

    #[test]
    fn rejects_unknown_fields_and_bad_protocol() {
        let mut v = base();
        v["secrets"] = json!([]);
        let err = structural_check(&v).unwrap_err();
        assert_eq!(err.category(), "VALIDATION");
        assert!(
            err.reason().contains("no secrets"),
            "reason: {}",
            err.reason()
        );

        let mut v2 = base();
        v2["protocol_version"] = json!("2");
        assert!(matches!(
            structural_check(&v2),
            Err(Rejection::Structural(_))
        ));

        let mut v3 = base();
        v3["run_id"] = json!("");
        assert!(matches!(
            structural_check(&v3),
            Err(Rejection::InvalidRunId(_))
        ));
    }
}
