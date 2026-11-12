//! Tool Engine 调用面（计划 §9:454-477 第 4 到 5 步「选择工具/调用 Tool
//! Engine」）：D-TC-05 受理校验 + ToolRun 状态机 + 信封交付全走
//! tool-engine 既有公开接口，Agent 层不绕过、不直调宿主 Shell（计划
//! §6:370 禁令同等生效，运行面由 [`RunDriver`] 端口注入）。
//!
//! 驱动实现：
//! - [`MockDriver`]：Mock Runner 注入路径（TOOL-CONTRACT §10.1 注入口径，
//!   P7-2 验收缩小版与契约测试用）；
//! - [`PythonWorkerDriver`]：真实 python_worker 派发（argv 固定清单经
//!   Adapter，不经 shell；runtime 块来自 Manifest 文件面，cli 侧组装）。

use crate::error::AgentError;
use crate::new_id;
use serde_json::{json, Value};
use tool_engine::adapter::python_worker::{execute_via_engine, AdapterCall, PythonWorkerAdapter};
use tool_engine::mock_runner::MockRunner;
use tool_engine::{DriveOutcome, Engine};

/// 驱动端口：把一个 QUEUED 的 tool_run 推到终态并交付信封（运行面注入点）。
pub trait RunDriver {
    /// 驱动到终态。
    fn drive(
        &self,
        engine: &Engine,
        run_id: &str,
    ) -> Result<DriveOutcome, tool_engine::EngineError>;
}

/// Mock Runner 驱动（注入配置透传；P7-2 闭环验收与契约测试）。
pub struct MockDriver {
    /// 注入的 runner 配置。
    pub runner: MockRunner,
}

impl MockDriver {
    /// 常规成功驱动（result + summary）。
    pub fn succeed(result: Value, summary: impl Into<String>) -> Self {
        Self {
            runner: MockRunner::succeed(result, summary),
        }
    }
}

impl RunDriver for MockDriver {
    fn drive(
        &self,
        engine: &Engine,
        run_id: &str,
    ) -> Result<DriveOutcome, tool_engine::EngineError> {
        engine.drive(run_id, &self.runner)
    }
}

/// python_worker 真实驱动（Adapter 配置由调用方按 Manifest runtime 块组装）。
pub struct PythonWorkerDriver {
    /// 派发器。
    pub adapter: PythonWorkerAdapter,
}

impl RunDriver for PythonWorkerDriver {
    fn drive(
        &self,
        engine: &Engine,
        run_id: &str,
    ) -> Result<DriveOutcome, tool_engine::EngineError> {
        execute_via_engine(AdapterCall {
            engine,
            run_id,
            adapter: &self.adapter,
            cancel_after: None,
        })
    }
}

/// 工具调用规格（Agent 计划步骤到 ToolCallRequest 的投影）。
#[derive(Debug, Clone)]
pub struct ToolCallSpec {
    /// 目标工具 id（必须在 capability_refs 内，规划面闸在 loop）。
    pub tool_id: String,
    /// 版本钉定（None = Registry current）。
    pub version: Option<String>,
    /// 工具入参（合 Manifest input_schema，受理校验兜底）。
    pub input: Value,
    /// 关联研究任务。
    pub task_id: Option<String>,
    /// 工作区隔离边界。
    pub workspace_id: String,
    /// 幂等提交键（缺省由引擎生成 agent:{run 序号} 键）。
    pub submit_key: Option<String>,
    /// 链路追踪 ID（P7-5：经 [`ToolLink::call`] 走 tool-engine
    /// `submit_correlated` 落 tool_run.trace_id 列，迁移 015；None = 无关联）。
    pub trace_id: Option<String>,
}

/// 调用结果。
#[derive(Debug, Clone)]
pub struct ToolCallOutcome {
    /// tool_run id（审计链与 Artifact FK 载体）。
    pub run_id: String,
    /// 解析后的执行版本。
    pub tool_version: String,
    /// 终态信封（已过 envelope::validate）。
    pub envelope: tool_engine::envelope::ToolRunResult,
}

/// Tool Engine 链接层。
pub struct ToolLink {
    engine: Engine,
    driver: Box<dyn RunDriver>,
}

impl ToolLink {
    /// 在引擎与驱动之上构造。
    pub fn new(engine: Engine, driver: Box<dyn RunDriver>) -> Self {
        Self { engine, driver }
    }

    /// 引擎视图（Evidence 登记面读 run 行用）。
    pub fn engine(&self) -> &Engine {
        &self.engine
    }

    /// 受理 + 驱动到终态。受理拒绝（结构/Registry/schema 校验）与驱动失败
    /// 都按 [`AgentError::Tool`] 上抛；工具业务失败（信封 FAILED/TIMED_OUT）
    /// 属正常交付，由调用方按信封判定。trace_id 经 submit_correlated 落
    /// tool_run 行（P7-5；冻结契约 JSON 不动，关联列平台侧承载）。
    pub fn call(&self, spec: &ToolCallSpec) -> Result<ToolCallOutcome, AgentError> {
        let run_id = new_id("rn");
        let request = json!({
            "protocol_version": "1",
            "tool_id": spec.tool_id,
            "tool_version": spec.version,
            "run_id": run_id,
            "submit_key": spec.submit_key,
            "input": spec.input,
            "workspace_id": spec.workspace_id,
            "task_id": spec.task_id,
        });
        let submitted = self
            .engine
            .submit_correlated(&request, spec.trace_id.as_deref())?;
        let outcome = self.driver.drive(&self.engine, &submitted.run_id)?;
        Ok(ToolCallOutcome {
            run_id: submitted.run_id,
            tool_version: submitted.tool_version,
            envelope: outcome.envelope,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use database::{DbPath, Pool, PoolConfig};
    use std::path::PathBuf;
    use std::time::Duration;
    use tool_registry::Registry;

    fn migrated_pool() -> Pool {
        let pool = Pool::open(PoolConfig {
            path: DbPath::Memory,
            readonly: false,
            max_connections: 1,
            busy_timeout: Duration::from_secs(5),
        })
        .unwrap();
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../migrations");
        database::migrate(&pool, &dir).unwrap();
        pool
    }

    fn repo_manifest(tool_id: &str) -> Value {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../manifests/tools")
            .join(format!("{tool_id}.json"));
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
    }

    /// 未登记工具：受理期 VALIDATION 拒绝（不产生 run 行）。
    #[test]
    fn unknown_tool_rejected_at_submit() {
        let pool = migrated_pool();
        let registry = Registry::open(pool.clone());
        let engine = Engine::open(pool, registry);
        let link = ToolLink::new(engine, Box::new(MockDriver::succeed(json!({}), "ok")));
        let err = link
            .call(&ToolCallSpec {
                tool_id: "rip.ghost.tool".to_string(),
                version: None,
                input: json!({}),
                task_id: None,
                workspace_id: "ws-default".to_string(),
                submit_key: None,
                trace_id: None,
            })
            .unwrap_err();
        assert!(err.to_string().contains("unknown tool"), "err: {err}");
    }

    /// 登记工具 + Mock 驱动：受理 → 状态机 → SUCCEEDED 信封交付。
    #[test]
    fn registered_tool_call_delivers_envelope() {
        let pool = migrated_pool();
        let registry = Registry::open(pool.clone());
        registry
            .register(&repo_manifest("rip.kb.keyword_search"))
            .unwrap();
        let engine = Engine::open(pool.clone(), registry);
        let link = ToolLink::new(
            engine,
            Box::new(MockDriver::succeed(
                json!({"keyword": "nmap", "per_group_limit": 5, "total_hits": 0, "groups": []}),
                "mock 检索完成",
            )),
        );
        let outcome = link
            .call(&ToolCallSpec {
                tool_id: "rip.kb.keyword_search".to_string(),
                version: None,
                input: json!({"keyword": "nmap"}),
                task_id: Some("task-agent-1".to_string()),
                workspace_id: "ws-default".to_string(),
                submit_key: Some("agent:arn-x:0".to_string()),
                trace_id: Some("tr-link-1".to_string()),
            })
            .unwrap();
        // P7-5：trace_id 经 submit_correlated 落 tool_run 行。
        let stored_trace: Option<String> = pool
            .with_conn(|c| {
                c.query_row(
                    "SELECT trace_id FROM tool_run WHERE run_id = ?1",
                    [&outcome.run_id],
                    |r| r.get(0),
                )
            })
            .unwrap();
        assert_eq!(stored_trace.as_deref(), Some("tr-link-1"));
        assert_eq!(outcome.tool_version, "0.1.0");
        assert_eq!(outcome.envelope.status, domain::ToolRunStatus::Succeeded);
        assert_eq!(outcome.envelope.result.as_ref().unwrap()["keyword"], "nmap");
    }
}
