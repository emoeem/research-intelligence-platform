//! Agent 节点执行器（P8-3）：Phase 7 agent-engine 为执行体，运行面经
//! [`AgentNodeDriver`] 端口注入（CLI 装 AgentLoop 真实闭环，测试装 Mock），
//! 控制面复用 agent-engine RunControl pause/cancel/retry 与 014
//! agent_run_control 协作暂停语义，本 crate 不重复实现控制面。
//!
//! 职责切分（「不重复实现控制面」的落点）：
//! - 本执行器只做三件事：解析 params 并装配 [`AgentNodeRequest`]、调用
//!   端口、把端口结果映射为 [`AgentNodeResult`]（Output / Paused / Failed）；
//!   派发前预生成 agent_run_id 与节点/run 链接列落库在 exec::dispatch_node
//!   （018 实体链接列，跨进程 pause/cancel 传播据此发现运行中实体）；
//! - fresh run 与 PAUSED 续接（AgentLoop::run / AgentLoop::resume 的选择）
//!   由端口实现侧经 agent-engine 既有 API 判定（agent_run_id 钉定在请求里，
//!   实现按 agent_run 行现态分派），引擎零 agent 状态机知识；
//! - workflow 级 pause 传播：WorkflowControl::propagate_pause_to_agents 经
//!   agent-engine RunControl::pause 写 014 请求位，agent 循环步间消费后转
//!   PAUSED，端口据此返回 Paused。
//!
//! 节点 params（required_params(Agent) = ["agent_id"] 冻结基线的消费）：
//! - `agent_id`（必填）：目标 agent 定义 id（登记存在性由 agent-engine
//!   受理面校验，未知定义 → 节点 failed code=agent_failed）；
//! - `goal`（可选，支持 `$/` 引用）：run 目标文本；缺省 = 整个 run state
//!   的紧凑 JSON（确定性，不引入隐藏输入）；
//! - `version`（可选）：agent 定义版本钉定；
//! - `task_id`（可选）：关联研究任务（透传 agent run task_id）；
//! - `workspace_id`（可选）：透传 agent 装配面。
//!
//! 输出：agent run 终态报告投影 object（见 [`report_projection`]，浅合并
//! 进 run state）；agent run PAUSED → [`AgentNodeResult::Paused`]（run
//! paused，节点回 pending 等 resume）；FAILED/CANCELLED → 节点 failed
//! code=agent_failed（message 带 agent 现态）。

use super::{resolve_refs, NodeFailure};
use serde_json::{json, Map, Value};

/// Agent 节点执行请求（端口入参；字段全部为引擎已钉定值，实现侧零回查）。
#[derive(Debug, Clone)]
pub struct AgentNodeRequest {
    /// workflow run id（015 链路锚：agent_run.trace_id = 该值，audit
    /// correlation 同链）。
    pub run_id: String,
    /// workflow 节点 id。
    pub node_id: String,
    /// 节点 attempt（attempt 语义与 017 节点行一致）。
    pub attempt: u32,
    /// 预生成的 agent run id（引擎在派发前已落节点链接列；实现侧必须以此
    /// id 落 agent_run 行，保证跨进程传播可见）。
    pub agent_run_id: String,
    /// agent 定义 id。
    pub agent_id: String,
    /// agent 定义版本钉定（None = 最新登记版本）。
    pub agent_version: Option<String>,
    /// run 目标文本。
    pub goal: String,
    /// 关联研究任务（透传）。
    pub task_id: Option<String>,
    /// workspace id（透传 agent 装配面；None = 实现侧缺省）。
    pub workspace_id: Option<String>,
}

impl AgentNodeRequest {
    /// audit/日志用定位串（wf:<run>:<node>:a<attempt>:<agent_run>）。
    pub fn locator(&self) -> String {
        format!(
            "wf:{}:{}:a{}:{}",
            self.run_id, self.node_id, self.attempt, self.agent_run_id
        )
    }
}

/// Agent 节点执行端口结果。
#[derive(Debug, Clone)]
pub enum AgentNodeOutcome {
    /// agent run 终态 SUCCEEDED，载输出投影（object 或 null）。
    Succeeded {
        /// 输出投影（object 或 null；浅合并进 run state）。
        output: Value,
    },
    /// agent run 协作暂停（PAUSED，非终态）：run paused，节点回 pending。
    Paused {
        /// 暂停中的 agent run id（留痕/续接锚）。
        agent_run_id: String,
    },
    /// agent run 失败（FAILED/CANCELLED/受理拒绝/端口内部错误）。
    Failed(NodeFailure),
}

/// Agent 节点执行端口（agent-engine AgentLoop 注入点；与 ToolRunDriver
/// 同款分工：引擎受理/状态机/轨迹/控制面全走 agent-engine 既有公开接口，
/// 本 crate 不绕过不直调宿主 Shell）。
pub trait AgentNodeDriver: Send + Sync {
    /// 执行到终态或协作暂停（实现侧按 agent_run 现态选择 fresh run 或
    /// PAUSED 续接）。
    fn execute(&self, request: &AgentNodeRequest) -> AgentNodeOutcome;
}

/// Agent 节点执行（在派发线程内运行；driver 为 Arc 克隆面）。
pub fn execute(
    driver: Option<&dyn AgentNodeDriver>,
    request: &AgentNodeRequest,
    params_json: Option<&str>,
    state: &Value,
) -> AgentNodeOutcome {
    let Some(driver) = driver else {
        return AgentNodeOutcome::Failed(NodeFailure::new(
            "node_driver_unavailable",
            "no AgentNodeDriver configured; install one via WorkflowExecutor::with_agent_driver",
        ));
    };
    let params: Map<String, Value> = match params_json {
        Some(raw) => match serde_json::from_str(raw) {
            Ok(map) => map,
            Err(e) => {
                return AgentNodeOutcome::Failed(NodeFailure::new(
                    "params_invalid",
                    format!("agent node params parse failed: {e}"),
                ))
            }
        },
        None => Map::new(),
    };
    let Some(agent_id) = params.get("agent_id").and_then(Value::as_str) else {
        return AgentNodeOutcome::Failed(NodeFailure::new(
            "params_invalid",
            "agent node params missing agent_id",
        ));
    };
    let goal = match params.get("goal") {
        Some(v) if !v.is_null() => {
            let resolved = match resolve_refs(v, state) {
                Ok(resolved) => resolved,
                Err(failure) => return AgentNodeOutcome::Failed(failure),
            };
            match resolved {
                Value::String(text) => text,
                other => other.to_string(),
            }
        }
        _ => match serde_json::to_string(state) {
            Ok(text) => text,
            Err(e) => {
                return AgentNodeOutcome::Failed(NodeFailure::new(
                    "params_invalid",
                    format!("goal fallback serialize failed: {e}"),
                ))
            }
        },
    };
    let req = AgentNodeRequest {
        run_id: request.run_id.clone(),
        node_id: request.node_id.clone(),
        attempt: request.attempt,
        agent_run_id: request.agent_run_id.clone(),
        agent_id: agent_id.to_string(),
        agent_version: params
            .get("version")
            .and_then(Value::as_str)
            .map(str::to_string),
        goal,
        task_id: params
            .get("task_id")
            .and_then(Value::as_str)
            .map(str::to_string),
        workspace_id: params
            .get("workspace_id")
            .and_then(Value::as_str)
            .map(str::to_string),
    };
    match driver.execute(&req) {
        AgentNodeOutcome::Succeeded { output } => match output {
            Value::Object(_) | Value::Null => AgentNodeOutcome::Succeeded { output },
            other => AgentNodeOutcome::Failed(NodeFailure::new(
                "output_not_object",
                format!(
                    "agent node output must be a JSON object or null to merge into run state, got {}",
                    super::type_word(&other)
                ),
            )),
        },
        other => other,
    }
}

/// 终态报告投影（端口实现侧交付 Succeeded output 的建议形状；引擎不强制
/// object 内键集，只保证浅合并安全）。
pub fn report_projection(
    agent_run_id: &str,
    state: &str,
    finish_reason: &str,
    steps: usize,
    warnings: &[String],
    manifest_id: &str,
    intent_status: &str,
) -> Value {
    json!({
        "agent_run_id": agent_run_id,
        "agent_state": state,
        "agent_finish_reason": finish_reason,
        "agent_steps": steps,
        "agent_warnings": warnings,
        "manifest_id": manifest_id,
        "intent_status": intent_status,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_request() -> AgentNodeRequest {
        AgentNodeRequest {
            run_id: "wfr-x".to_string(),
            node_id: "a1".to_string(),
            attempt: 1,
            agent_run_id: "arn-fixed".to_string(),
            agent_id: String::new(),
            agent_version: None,
            goal: String::new(),
            task_id: None,
            workspace_id: None,
        }
    }

    /// 缺 driver fail-closed。
    #[test]
    fn missing_driver_is_fail_closed() {
        let outcome = execute(None, &base_request(), Some("{}"), &json!({}));
        match outcome {
            AgentNodeOutcome::Failed(failure) => assert_eq!(failure.code, "node_driver_unavailable"),
            other => panic!("expected Failed, got {other:?}"),
        }
    }

    /// params 缺 agent_id 拒绝；坏 params JSON 拒绝。
    #[test]
    fn params_validation_is_fail_closed() {
        struct NoCall;
        impl AgentNodeDriver for NoCall {
            fn execute(&self, _request: &AgentNodeRequest) -> AgentNodeOutcome {
                panic!("driver must not be called for invalid params")
            }
        }
        for bad in [Some("{}"), Some("{bad"), Some(r#"{"agent_id": 3}"#)] {
            let outcome = execute(Some(&NoCall), &base_request(), bad, &json!({}));
            match outcome {
                AgentNodeOutcome::Failed(failure) => assert_eq!(failure.code, "params_invalid"),
                other => panic!("expected Failed for {bad:?}, got {other:?}"),
            }
        }
    }

    /// Paused 信号原样透传（引擎特判为暂停处理）；Succeeded object 透传，
    /// 非 object 输出 fail-closed。
    #[test]
    fn outcome_pass_through_and_output_guard() {
        struct PauseDriver;
        impl AgentNodeDriver for PauseDriver {
            fn execute(&self, request: &AgentNodeRequest) -> AgentNodeOutcome {
                assert_eq!(request.agent_run_id, "arn-fixed");
                assert_eq!(request.agent_id, "ag");
                AgentNodeOutcome::Paused {
                    agent_run_id: request.agent_run_id.clone(),
                }
            }
        }
        let outcome = execute(
            Some(&PauseDriver),
            &base_request(),
            Some(r#"{"agent_id": "ag"}"#),
            &json!({}),
        );
        match outcome {
            AgentNodeOutcome::Paused { agent_run_id } => assert_eq!(agent_run_id, "arn-fixed"),
            other => panic!("expected Paused, got {other:?}"),
        }
        struct ScalarDriver;
        impl AgentNodeDriver for ScalarDriver {
            fn execute(&self, _request: &AgentNodeRequest) -> AgentNodeOutcome {
                AgentNodeOutcome::Succeeded {
                    output: json!("scalar"),
                }
            }
        }
        let outcome = execute(
            Some(&ScalarDriver),
            &base_request(),
            Some(r#"{"agent_id": "ag"}"#),
            &json!({}),
        );
        match outcome {
            AgentNodeOutcome::Failed(failure) => assert_eq!(failure.code, "output_not_object"),
            other => panic!("expected Failed, got {other:?}"),
        }
    }

    /// 报告投影形状（Succeeded output 的建议形状）。
    #[test]
    fn report_projection_shape() {
        let projection = report_projection(
            "arn-1",
            "SUCCEEDED",
            "completed: all steps done",
            2,
            &["w1".to_string()],
            "mf-1",
            "ok",
        );
        assert_eq!(projection["agent_run_id"], json!("arn-1"));
        assert_eq!(projection["agent_state"], json!("SUCCEEDED"));
        assert_eq!(projection["agent_steps"], json!(2));
        assert_eq!(projection["intent_status"], json!("ok"));
    }
}
