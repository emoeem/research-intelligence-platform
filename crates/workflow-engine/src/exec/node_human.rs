//! Human 人工介入节点执行器（P8-3）：run 停在 awaiting_approval，批准/
//! 注入参数后恢复续跑。
//!
//! 语义（ROADMAP §9.3 P8-3「人工介入节点」+ 计划 §9:474 暂停恢复能力）：
//! - 首次派发：节点进入 running 后，本执行器把介入请求写 018
//!   workflow_run_control（approval_node_id + approval_request_json，
//!   approval_json 置空），向 run 循环返回 AwaitingApproval 信号；run 随即
//!   paused（「停在 awaiting_approval」 = 控制行 approval_node_id 非空且
//!   approval_json 为空且 run 现态 paused）；
//! - 恢复：`resume --input '<批准参数 JSON>'` 把参数并入 run state（键 =
//!   params.key，缺省 `approval`）并落 approval_json 留痕；节点被重派发时
//!   本执行器在 run state 中看到该键即成功，输出 `{key: 批准参数}` 浅合并
//!   进 state，下游节点续跑；
//! - 无批准参数 resume：恢复态无该键，节点重写介入请求再次停等（幂等语义，
//!   同一请求载荷重放，控制行 updated_at 推进）。
//!
//! params（required_params(Human) = []，全部可选）：
//! - `prompt`（可选）：给审批人的说明文本，原样进介入请求载荷；
//! - `request`（可选）：介入请求附加载荷 object（`$/` 引用先解析）；
//! - `key`（可选）：批准参数在 run state 中的键（缺省 `approval`；
//!   形状约束 [A-Za-z0-9_.-]+，防 JSON Pointer 注入歧义）。

use super::{resolve_refs, NodeFailure};
use serde_json::{json, Value};

/// 缺省批准参数键（run state 中的落键）。
pub const DEFAULT_APPROVAL_KEY: &str = "approval";

/// 人工介入节点执行（在派发线程内运行）。
///
/// 返回 `(output, awaiting)`：
/// - `awaiting = true`：run state 无批准键，请求载荷已交回（由调用方落
///   workflow_run_control），节点未终态；
/// - `awaiting = false`：批准参数在 state 中，output = `{key: 参数}`
///   （object 或 null 以外形状 fail-closed，与 Tool/Knowledge 合并口径一致）。
pub fn execute(
    params_json: Option<&str>,
    state: &Value,
) -> Result<(Value, bool), NodeFailure> {
    let params: serde_json::Map<String, Value> = match params_json {
        Some(raw) => serde_json::from_str(raw)
            .map_err(|e| NodeFailure::new("params_invalid", format!("human node params parse failed: {e}")))?,
        None => serde_json::Map::new(),
    };
    let key = match params.get("key").and_then(Value::as_str) {
        None => DEFAULT_APPROVAL_KEY.to_string(),
        Some(k) => {
            let ok = !k.is_empty()
                && k.chars().count() <= 64
                && k.chars().next().expect("non-empty checked")
                    .is_ascii_alphanumeric()
                && k.chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.' || c == '-');
            if !ok {
                return Err(NodeFailure::new(
                    "params_invalid",
                    format!("human node params.key {k:?} outside [A-Za-z0-9][A-Za-z0-9_.-]*"),
                ));
            }
            k.to_string()
        }
    };
    // 批准参数已在 state（resume 注入）→ 成功交付。
    if let Some(approval) = state.get(&key) {
        if approval.is_null() {
            // null 视为未注入（resume 未带该键与显式 null 同形，fail-closed
            // 不允许 null 当批准值，避免与「未批准」歧义）。
            return Err(NodeFailure::new(
                "approval_not_injected",
                format!(
                    "human node awaiting approval: state key {key:?} is null \
                     (inject approval params via resume --input)"
                ),
            ));
        }
        let output = json!({ key.clone(): approval.clone() });
        return Ok((output, false));
    }
    // 未批准：构造介入请求载荷（awaiting 信号，调用方落控制行）。
    let mut request = serde_json::Map::new();
    request.insert("node_key".to_string(), json!(key));
    if let Some(prompt) = params.get("prompt").and_then(Value::as_str) {
        request.insert("prompt".to_string(), json!(prompt));
    }
    match params.get("request") {
        Some(v) if !v.is_null() => {
            let resolved = resolve_refs(v, state)?;
            match resolved {
                Value::Object(map) => {
                    for (k, val) in map {
                        request.insert(k, val);
                    }
                }
                other => {
                    return Err(NodeFailure::new(
                        "params_invalid",
                        format!(
                            "human node params.request must be a JSON object, got {}",
                            super::type_word(&other)
                        ),
                    ))
                }
            }
        }
        _ => {}
    }
    Ok((Value::Object(request), true))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn awaiting_request_carries_prompt_and_request_payload() {
        let params = json!({"prompt": "批准外部访问?", "request": {"target": "$/target"}});
        let (request, awaiting) = execute(Some(&params.to_string()), &json!({"target": "example.com"}))
            .expect("execute");
        assert!(awaiting);
        assert_eq!(request["node_key"], json!("approval"));
        assert_eq!(request["prompt"], json!("批准外部访问?"));
        assert_eq!(request["target"], json!("example.com"));
    }

    #[test]
    fn approval_in_state_completes_node() {
        let (output, awaiting) = execute(None, &json!({"approval": {"ok": true}})).expect("execute");
        assert!(!awaiting);
        assert_eq!(output, json!({"approval": {"ok": true}}));
        // 标量批准值同样交付。
        let (output, awaiting) = execute(None, &json!({"approval": "go"})).expect("execute");
        assert!(!awaiting);
        assert_eq!(output, json!({"approval": "go"}));
    }

    #[test]
    fn custom_key_and_bad_shapes_fail_closed() {
        let params = json!({"key": "human_decision"});
        let (output, awaiting) = execute(
            Some(&params.to_string()),
            &json!({"human_decision": {"approved_by": "reviewer"}}),
        )
        .expect("execute");
        assert!(!awaiting);
        assert_eq!(output["human_decision"], json!({"approved_by": "reviewer"}));
        // 坏 key 形状拒绝。
        let bad = json!({"key": "bad key"});
        let err = execute(Some(&bad.to_string()), &json!({})).expect_err("bad key");
        assert_eq!(err.code, "params_invalid");
        // state 键为 null 视为未批准（不可当批准值）。
        let err = execute(None, &json!({"approval": null})).expect_err("null approval");
        assert_eq!(err.code, "approval_not_injected");
        // 非 object request 拒绝。
        let bad_req = json!({"request": [1, 2]});
        let err = execute(Some(&bad_req.to_string()), &json!({})).expect_err("bad request");
        assert_eq!(err.code, "params_invalid");
        // 坏 params JSON 拒绝。
        let err = execute(Some("{bad"), &json!({})).expect_err("bad json");
        assert_eq!(err.code, "params_invalid");
    }
}
