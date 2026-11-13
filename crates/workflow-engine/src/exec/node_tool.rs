//! Tool 节点执行器（P8-2）：tool-engine 派发链（submit D-TC-05 受理 →
//! ToolRun 状态机 → 信封交付），运行面经 [`ToolRunDriver`] 端口注入
//! （CLI 装 python_worker 真实派发，测试装 Mock 注入，TOOL-CONTRACT
//! §10.1 注入口径；本 crate 不绕过引擎不直调宿主 Shell，计划 §6:370
//! 禁令同等生效）。
//!
//! 节点 params（受理面词表，validate.rs required_params 冻结基线的消费）：
//! - `tool_id`（必填）：目标工具 id（Registry 存在性由引擎 submit 校验，
//!   未知工具 D-TC-05 拒绝 → 节点 failed code=tool_submit_rejected）；
//! - `input`（可选）：工具入参 object，字符串叶值 `$/` 前缀按 run state
//!   JSON Pointer 解析；缺省 = 整个 run state；
//! - `version`（可选）：版本钉定（缺省 Registry current）；
//! - `workspace_id`（可选，缺省 `ws-workflow`）；
//! - `task_id` / `authorization_ref` / `budget`（可选透传）。
//!
//! 输出：SUCCEEDED 信封的结构化 result（null 或 JSON object；非 object
//! fail-closed code=output_not_object）；非 SUCCEEDED 终态 → 节点 failed
//! code=tool_<status 词形小写>。tool_run.trace_id 落 run_id（015 链路
//! 关联，audit 回放按 correlation_id 串起 workflow run 的全部 tool run）。

use super::{new_id, NodeFailure, ToolRunDriver};
use crate::exec::resolve_refs;
use crate::state::type_word;
use serde_json::{json, Map, Value};
use tool_engine::Engine;

/// Tool 节点执行（在派发线程内运行；engine 为 Clone、driver 为 Arc 克隆面，
/// Parallel 并发子分支复用同一执行器实例）。
#[allow(clippy::too_many_arguments)]
pub fn execute(
    engine: &Engine,
    driver: Option<&dyn ToolRunDriver>,
    run_id: &str,
    node_id: &str,
    attempt: u32,
    params_json: Option<&str>,
    state: &Value,
) -> Result<Value, NodeFailure> {
    let params: Map<String, Value> = match params_json {
        Some(raw) => serde_json::from_str(raw)
            .map_err(|e| NodeFailure::new("params_invalid", format!("tool node params parse failed: {e}")))?,
        None => Map::new(),
    };
    let tool_id = params
        .get("tool_id")
        .and_then(Value::as_str)
        .ok_or_else(|| NodeFailure::new("params_invalid", "tool node params missing tool_id"))?;
    let input = match params.get("input") {
        Some(v) if !v.is_null() => resolve_refs(v, state)?,
        _ => state.clone(),
    };
    if input.as_object().is_none() {
        return Err(NodeFailure::new(
            "params_invalid",
            format!("tool input must be a JSON object, got {}", type_word(&input)),
        ));
    }
    let mut request = json!({
        "protocol_version": "1",
        "run_id": new_id("rn"),
        "tool_id": tool_id,
        "input": input,
        "workspace_id": params.get("workspace_id").and_then(Value::as_str).unwrap_or("ws-workflow"),
        "submit_key": format!("wf:{run_id}:{node_id}:a{attempt}"),
    });
    if let Some(obj) = request.as_object_mut() {
        if let Some(v) = params.get("version").and_then(Value::as_str) {
            obj.insert("tool_version".to_string(), json!(v));
        }
        if let Some(v) = params.get("task_id").and_then(Value::as_str) {
            obj.insert("task_id".to_string(), json!(v));
        }
        if let Some(v) = params.get("authorization_ref").and_then(Value::as_str) {
            obj.insert("authorization_ref".to_string(), json!(v));
        }
        if let Some(budget) = params.get("budget").filter(|b| b.is_object()) {
            obj.insert("budget".to_string(), budget.clone());
        }
    }
    // D-TC-05 受理（拒绝不落 run 行）+ 015 trace 关联（trace_id = workflow run id）。
    let submitted = engine
        .submit_correlated(&request, Some(run_id))
        .map_err(|e| {
            NodeFailure::new(
                "tool_submit_rejected",
                format!("tool engine submit rejected: {e}"),
            )
        })?;
    let Some(driver) = driver else {
        return Err(NodeFailure::new(
            "node_driver_unavailable",
            "no ToolRunDriver configured; install one via WorkflowExecutor::with_tool_driver",
        ));
    };
    let outcome = driver
        .drive(engine, &submitted.run_id)
        .map_err(|e| NodeFailure::new("tool_drive_failed", format!("tool drive failed: {e}")))?;
    let envelope = outcome.envelope;
    match envelope.status {
        domain::ToolRunStatus::Succeeded => {
            let result = envelope.result.unwrap_or(Value::Null);
            if !result.is_null() && result.as_object().is_none() {
                return Err(NodeFailure::new(
                    "output_not_object",
                    format!(
                        "tool {} result must be a JSON object or null to merge into run state, got {}",
                        submitted.tool_id,
                        type_word(&result)
                    ),
                ));
            }
            Ok(result)
        }
        other => {
            let message = envelope
                .error
                .as_ref()
                .map(|e| format!("{}: {}", e.category, e.message))
                .or_else(|| envelope.summary.clone())
                .unwrap_or_else(|| format!("tool run {} ended {}", submitted.run_id, other.as_str()));
            Err(NodeFailure::new(
                format!("tool_{}", other.as_str().to_lowercase()),
                message,
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn params_missing_tool_id_is_fail_closed() {
        let engine = test_engine("missing-tool-id");
        let err = execute(&engine, None, "wfr-x", "t1", 1, Some("{}"), &json!({}))
            .expect_err("missing tool_id");
        assert_eq!(err.code, "params_invalid");
    }

    #[test]
    fn params_bad_json_is_fail_closed() {
        let engine = test_engine("bad-json");
        let err = execute(&engine, None, "wfr-x", "t1", 1, Some("{bad"), &json!({}))
            .expect_err("bad params json");
        assert_eq!(err.code, "params_invalid");
    }

    #[test]
    fn unresolvable_state_ref_is_fail_closed() {
        let engine = test_engine("unresolvable-ref");
        let params = json!({"tool_id": "any", "input": {"k": "$/missing"}});
        let err = execute(&engine, None, "wfr-x", "t1", 1, Some(&params.to_string()), &json!({}))
            .expect_err("unresolvable ref");
        assert_eq!(err.code, "state_ref_unresolvable");
    }

    /// 独立临时库（同名路径并行测试互删文件会触发 migrate 版本错配竞态，
    /// 每用例独立 tag）。
    fn test_engine(tag: &str) -> Engine {
        let dir = std::env::temp_dir().join(format!(
            "rip-wf-node-tool-{}-{}",
            tag,
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).expect("dir");
        let path = dir.join("t.db");
        let _ = std::fs::remove_file(&path);
        let pool = database::Pool::open(database::PoolConfig {
            path: database::DbPath::File(path),
            readonly: false,
            max_connections: 1,
            busy_timeout: std::time::Duration::from_secs(5),
        })
        .expect("pool");
        database::migrate(
            &pool,
            std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../migrations")),
        )
        .expect("migrate");
        Engine::open(pool.clone(), tool_registry::Registry::open(pool))
    }
}
