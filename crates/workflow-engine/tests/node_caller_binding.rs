//! MA-3 节点 caller 角色绑定与预算协同 e2e（docs/rip3/MA-3-TASK.md §0 D1/D8
//! + §4 判据 1/2 Rust 面）：
//!
//! 判据 2 解析序（Rust 面）：解析序 = 「节点 params caller_id 显式
//! → run 级 caller → None」（三层各一正例；None = legacy 零回归：
//! agent_run.caller_id NULL）+ params caller_id 非字符串值 →
//! params_invalid 拒（负例）。
//! 节点级解析落点 = node_agent::execute（params 开放面新键）；run 级
//! 权威 = workflow_run.caller_id 持久化列；完整解析序经本文件
//! LoopBindingDriver（与 rp-workflow AgentLoopDriver 同一解析序实现，
//! 真进程级硬验收由 apps/server/selftest_ma3.sh 判据 1 承载）。
//!
//! 判据 1 硬验收① Rust 面：双 AI 协作同一 workflow（节点分属两
//! caller）——agent_run 两行 caller_id 分别 = caller-a / caller-b（库
//! 直查断言）+ workflow_run.caller_id = caller-a + GUARDRAIL 审计行按
//! caller 分账（caller-a 总预算帽 budget_caller_total 落行实测）。
//!
//! 判据 1 预算协同：caller_budget 帽对 caller-b 生效时节点 failed
//! code=agent_failed message 带 `budget denied` 词形（受理面跨 run 并发
//! 帽 BudgetDenied 硬拒，run 不落行）→ run fail-fast 收敛。
//!
//! 全程内存库 + 真实 migrations（001..029）+ 仓库登记 Manifest + 真实
//! AgentLoop 装配（mock 工具驱动 RunDriver 端口，ma2_parallel_budget.rs
//! harness 同款形态）；caller_budget 预算行落 027 真表、经
//! agent_engine::control::load_caller_budgets 真实读入（D8 帽值解析点 =
//! 装配层纪律）。agent-engine crate 零 diff（D1 通道全复用）。

#![deny(missing_docs)]

use agent_engine::agent_loop::{
    AgentLoop, Decision, LoopConfig, PlanContext, PlanStep, Planner, RunRequest,
};
use agent_engine::broker_link::{BrokerLink, FirstFamilyConfirmer};
use agent_engine::control::RunControlConfig;
use agent_engine::evidence::EvidenceRegistrar;
use agent_engine::store::AgentStore;
use agent_engine::tool_link::{RunDriver, ToolLink};
use context_broker::{ArtifactSource, Broker, ManifestStore, RegistrySource, RulepackSource};
use database::rusqlite::OptionalExtension;
use database::{rusqlite, DbPath, Pool, PoolConfig};
use domain::{AgentDefinition, AgentId, AgentRun, CapabilityRef};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tool_engine::mock_runner::MockRunner;
use tool_engine::Engine;
use tool_registry::Registry;
use workflow_engine::collab::{ClaimCreateInput, ClaimKind, ClaimStore};
use workflow_engine::exec::node_agent::{AgentNodeDriver, AgentNodeOutcome, AgentNodeRequest};
use workflow_engine::exec::WorkflowExecutor;
use workflow_engine::store::WorkflowStore;

const TOOL_ID: &str = "rip.kb.keyword_search";
const AGENT_ID: &str = "agent.ma3.binding";
const RUN_MIGRATIONS_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../migrations");

/// 双 agent 节点 workflow：node1 无绑定（继承 run 级 caller-a）、node2
/// params 显式 caller_id=caller-b（D1 硬验收①载具）。
const TWO_AGENT_DEF: &str = r#"{
  "schema": "workflow/definition-v1",
  "workflow_id": "wf-ma3-caller-binding",
  "version": 1,
  "name": "MA-3 双 caller 节点绑定样例",
  "description": "start -> node1(agent, 无 caller 绑定) -> node2(agent, params caller_id=caller-b) -> done",
  "graph": {
    "nodes": [
      { "id": "start", "type": "start" },
      { "id": "node1", "type": "agent", "params": { "agent_id": "agent.ma3.binding", "goal": "MA-3 节点绑定 node1" } },
      { "id": "node2", "type": "agent", "params": { "agent_id": "agent.ma3.binding", "goal": "MA-3 节点绑定 node2", "caller_id": "caller-b" } },
      { "id": "done", "type": "end" }
    ],
    "edges": [
      { "from": "start", "to": "node1" },
      { "from": "node1", "to": "node2" },
      { "from": "node2", "to": "done" }
    ]
  }
}"#;

/// 非字符串 caller_id 负例载具（params caller_id = 42）。
const BAD_PARAMS_DEF: &str = r#"{
  "schema": "workflow/definition-v1",
  "workflow_id": "wf-ma3-bad-caller-params",
  "version": 1,
  "name": "MA-3 非法 caller params 负例",
  "description": "start -> node1(agent, caller_id=42) -> done",
  "graph": {
    "nodes": [
      { "id": "start", "type": "start" },
      { "id": "node1", "type": "agent", "params": { "agent_id": "agent.ma3.binding", "goal": "负例", "caller_id": 42 } },
      { "id": "done", "type": "end" }
    ],
    "edges": [
      { "from": "start", "to": "node1" },
      { "from": "node1", "to": "done" }
    ]
  }
}"#;

fn migrated_pool() -> Pool {
    let pool = Pool::open(PoolConfig {
        path: DbPath::Memory,
        readonly: false,
        max_connections: 1,
        busy_timeout: Duration::from_secs(5),
    })
    .unwrap();
    database::migrate(&pool, PathBuf::from(RUN_MIGRATIONS_DIR).as_path()).unwrap();
    pool
}

/// 登记 Manifest + 测试 Agent 定义（ma2 harness 同款一次性载具）。
fn register_fixture(pool: &Pool) {
    let manifest_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../manifests/tools")
        .join(format!("{TOOL_ID}.json"));
    let manifest: Value =
        serde_json::from_str(&std::fs::read_to_string(manifest_path).unwrap()).unwrap();
    Registry::open(pool.clone()).register(&manifest).unwrap();
    pool.with_conn(|c| {
        c.execute(
            "UPDATE tool SET lifecycle = 'ENABLED' WHERE tool_id = ?1",
            [TOOL_ID],
        )
    })
    .unwrap();
    let definition = AgentDefinition::try_new(
        AgentId::try_new(AGENT_ID).unwrap(),
        "0.1.0".to_string(),
        "{}".to_string(),
        vec![CapabilityRef::try_new(TOOL_ID, None).unwrap()],
    )
    .unwrap();
    AgentStore::open(pool.clone(), Registry::open(pool.clone()))
        .register_definition(&definition, "MA-3 节点绑定测试 Agent")
        .unwrap();
}

fn build_broker_link(pool: &Pool) -> BrokerLink {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let rulepack = RulepackSource::from_dir(&root.join("rulepacks/rip-rules")).unwrap();
    let intent_config =
        context_broker::IntentConfig::from_dir(&root.join("rulepacks/rip-rules")).unwrap();
    let index = context_broker::ManifestIndex::from_dirs(&[root.join("manifests/tools")]).unwrap();
    let registry_source = RegistrySource::new(Registry::open(pool.clone()), index);
    let sources: Vec<Box<dyn context_broker::ContextSource>> = vec![
        Box::new(rulepack),
        Box::new(registry_source),
        Box::new(ArtifactSource::new(pool.clone())),
    ];
    let broker = Broker::new(
        sources,
        context_broker::BudgetPolicy::default_for(8000).unwrap(),
        intent_config,
        ManifestStore::new(pool.clone()),
        Some(audit_engine::AuditEngine::new(pool.clone())),
    );
    BrokerLink::new(broker)
}

/// mock 工具驱动（每步固定成功；真实状态机信封走 tool-engine Engine）。
#[derive(Clone)]
struct StepDriver {
    runner: MockRunner,
}

impl RunDriver for StepDriver {
    fn drive(
        &self,
        engine: &Engine,
        run_id: &str,
    ) -> Result<tool_engine::DriveOutcome, tool_engine::EngineError> {
        engine.drive(run_id, &self.runner)
    }
}

/// 单步规划器工厂（每 agent run 一份独立计数器；一步后 Finish 收尾）。
struct OneStepPlanner {
    counter: AtomicUsize,
}

impl Planner for OneStepPlanner {
    fn plan(&self, _ctx: &PlanContext<'_>) -> Decision {
        if self.counter.fetch_add(1, Ordering::SeqCst) == 0 {
            Decision::Steps(vec![PlanStep {
                tool_id: TOOL_ID.to_string(),
                version: None,
                input: json!({"keyword": "ma3-binding", "per_group_limit": 5}),
                rationale: "MA-3 节点绑定单步规划".to_string(),
            }])
        } else {
            Decision::Finish {
                reason: "MA-3 规划器收尾".to_string(),
            }
        }
    }
}

/// caller 预算行登记（027 真表；caller 行先在——load_caller_budgets JOIN
/// caller revoked 判定同口径）。
fn add_caller(pool: &Pool, caller_id: &str) {
    pool.with_conn(|c| {
        c.execute(
            "INSERT INTO caller (caller_id, display_name, permission_level, created_at) \
             VALUES (?1, ?2, 'T1', '2026-11-22T00:00:00.000Z')",
            rusqlite::params![caller_id, format!("name-{caller_id}")],
        )
    })
    .unwrap();
}

/// caller_budget 行登记（七帽列直填；D8 预算面协同零新机制——写 027 真表，
/// 引擎经 load_caller_budgets 读入）。
fn set_budget(
    pool: &Pool,
    caller: &str,
    parallel: i64,
    concurrent: i64,
    window_ms: i64,
    rate: i64,
    total: i64,
) {
    pool.with_conn(|c| {
        c.execute(
            "INSERT OR REPLACE INTO caller_budget (caller_id, max_parallel_subtasks, \
             max_concurrent_runs, rate_window_ms, rate_max_steps, budget_max_steps_per_run, \
             updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, '2026-11-22T00:00:00.000Z')",
            rusqlite::params![caller, parallel, concurrent, window_ms, rate, total],
        )
    })
    .unwrap();
}

/// agent_run.caller_id 读回（按 agent_run_id 点查）。
fn agent_run_caller(pool: &Pool, agent_run_id: &str) -> Option<String> {
    pool.with_conn(|c| {
        c.query_row(
            "SELECT caller_id FROM agent_run WHERE run_id = ?1",
            [agent_run_id],
            |r| r.get::<_, Option<String>>(0),
        )
        .optional()
    })
    .unwrap()
    .flatten()
}

/// workflow_run_node.agent_run_id 链接列读回（节点 → 预生成 agent run 锚）。
fn node_agent_run_id(pool: &Pool, run_id: &str, node_id: &str) -> Option<String> {
    pool.with_conn(|c| {
        c.query_row(
            "SELECT agent_run_id FROM workflow_run_node WHERE run_id = ?1 AND node_id = ?2",
            rusqlite::params![run_id, node_id],
            |r| r.get::<_, Option<String>>(0),
        )
        .optional()
    })
    .unwrap()
    .flatten()
}

/// GUARDRAIL 审计行按 caller 分账读回（audit_event.detail 含 caller_id）。
fn guardrail_rows(pool: &Pool, caller: &str) -> Vec<(String, String)> {
    pool.with_conn(|c| -> Result<Vec<(String, String)>, database::DbError> {
        let mut stmt = c.prepare(
            "SELECT action, detail FROM audit_event \
             WHERE action = 'agent.run.guardrail' AND detail LIKE ?1 ORDER BY event_id",
        )?;
        let pattern = format!("%\"caller_id\":\"{caller}\"%");
        let rows = stmt
            .query_map([pattern], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    })
    .unwrap()
}

/// MA-3 解析序真实装配驱动（与 rp-workflow AgentLoopDriver 同一解析序：
/// 节点 params 显式 > workflow_run.caller_id 持久化列 > None；caller 在位
/// 时 caller_budget 表经 load_caller_budgets 真实读入挂 with_control——
/// D8 帽值解析点 = 装配层，引擎不直查配置表）。
struct LoopBindingDriver {
    pool: Pool,
}

impl LoopBindingDriver {
    fn lookup_run_caller(&self, run_id: &str) -> Option<String> {
        self.pool
            .with_conn(|c| {
                c.query_row(
                    "SELECT caller_id FROM workflow_run WHERE run_id = ?1",
                    [run_id],
                    |r| r.get::<_, Option<String>>(0),
                )
                .optional()
            })
            .unwrap()
            .flatten()
    }
}

impl AgentNodeDriver for LoopBindingDriver {
    fn execute(&self, request: &AgentNodeRequest) -> AgentNodeOutcome {
        let resolved = request
            .caller_id
            .clone()
            .or_else(|| self.lookup_run_caller(&request.run_id));
        let store = AgentStore::open(self.pool.clone(), Registry::open(self.pool.clone()));
        let loop_ = AgentLoop::new(
            store,
            build_broker_link(&self.pool),
            ToolLink::new(
                Engine::open(self.pool.clone(), Registry::open(self.pool.clone())),
                Box::new(StepDriver {
                    runner: MockRunner::succeed(
                        json!({"keyword": "mock", "per_group_limit": 5, "total_hits": 0, "groups": []}),
                        "mock 检索完成",
                    ),
                }),
            ),
            EvidenceRegistrar::new(self.pool.clone()),
            Some(audit_engine::AuditEngine::new(self.pool.clone())),
            LoopConfig {
                workspace_id: "ws-ma3-binding".to_string(),
                max_tool_calls: 8,
            },
        );
        let loop_ = match &resolved {
            Some(_) => {
                let caller_budgets =
                    agent_engine::control::load_caller_budgets(&self.pool).unwrap_or_default();
                loop_.with_control(RunControlConfig {
                    caller_budgets,
                    ..RunControlConfig::default()
                })
            }
            None => loop_,
        };
        let mut req = RunRequest {
            agent_id: request.agent_id.clone(),
            agent_version: request.agent_version.clone(),
            goal: request.goal.clone(),
            task_id: None,
            trace_id: Some(request.run_id.clone()),
            run_id: Some(request.agent_run_id.clone()),
            input: None,
            caller_id: resolved,
            planner: Box::new(OneStepPlanner {
                counter: AtomicUsize::new(0),
            }),
            confirmer: Box::new(FirstFamilyConfirmer::new(
                context_broker::IntentConfig::from_dir(
                    &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../rulepacks/rip-rules"),
                )
                .unwrap(),
            )),
        };
        match loop_.run(&mut req) {
            Ok(report) => AgentNodeOutcome::Succeeded {
                output: workflow_engine::exec::node_agent::report_projection(
                    &report.run_id,
                    &report.state,
                    &report.finish_reason,
                    report.steps.len(),
                    &report.warnings,
                    &report.manifest_id,
                    &report.intent_status,
                ),
            },
            Err(e) => AgentNodeOutcome::Failed(workflow_engine::exec::NodeFailure::new(
                "agent_failed",
                e.to_string(),
            )),
        }
    }
}

/// 记录型 Mock 驱动（判据 2 节点 params 绑定传播面：逐节点捕获
/// AgentNodeRequest.caller_id 原值）。
#[derive(Default)]
struct RecordingDriver {
    seen: std::sync::Mutex<Vec<(String, Option<String>)>>,
}

impl AgentNodeDriver for RecordingDriver {
    fn execute(&self, request: &AgentNodeRequest) -> AgentNodeOutcome {
        self.seen
            .lock()
            .unwrap()
            .push((request.node_id.clone(), request.caller_id.clone()));
        AgentNodeOutcome::Succeeded {
            output: json!({"done": true}),
        }
    }
}

fn put_definition(pool: &Pool, raw: &str) {
    let doc = workflow_engine::dsl::parse_definition(raw).unwrap();
    WorkflowStore::new(pool.clone())
        .put(&doc, "ma3-test")
        .unwrap();
}

#[test]
fn node_params_binding_flows_to_driver_and_run_row() {
    let pool = migrated_pool();
    register_fixture(&pool);
    add_caller(&pool, "caller-a");
    add_caller(&pool, "caller-b");
    put_definition(&pool, TWO_AGENT_DEF);
    let recorder = Arc::new(RecordingDriver::default());
    let executor = WorkflowExecutor::new(pool.clone()).with_agent_driver(recorder.clone());
    let outcome = executor
        .start_with_caller(
            "wf-ma3-caller-binding",
            None,
            &json!({}),
            None,
            Some("caller-a"),
        )
        .expect("run succeeds");
    assert_eq!(
        domain::WorkflowRunState::Succeeded,
        outcome.state,
        "双节点 run 到 SUCCEEDED"
    );
    // run 级 caller 落 workflow_run.caller_id（D1① 写面接线）。
    let run_caller: Option<String> = pool
        .with_conn(|c| {
            c.query_row(
                "SELECT caller_id FROM workflow_run WHERE run_id = ?1",
                [outcome.run_id.as_str()],
                |r| r.get(0),
            )
            .optional()
        })
        .unwrap()
        .flatten();
    assert_eq!(run_caller.as_deref(), Some("caller-a"));
    // 节点 params 绑定逐节点捕获：node1 无绑定（None，实现侧兜 run 级），
    // node2 显式 caller-b（D1② 节点级解析）。
    let seen = recorder.seen.lock().unwrap();
    assert_eq!(seen.len(), 2, "两 agent 节点各记录一次");
    assert_eq!(seen[0].0, "node1");
    assert_eq!(seen[0].1, None, "node1 无 params 绑定 → None（节点级）");
    assert_eq!(seen[1].0, "node2");
    assert_eq!(
        seen[1].1.as_deref(),
        Some("caller-b"),
        "node2 params 显式绑定"
    );
}

#[test]
fn legacy_none_caller_keeps_null_columns() {
    let pool = migrated_pool();
    register_fixture(&pool);
    put_definition(&pool, TWO_AGENT_DEF);
    let recorder = Arc::new(RecordingDriver::default());
    let executor = WorkflowExecutor::new(pool.clone()).with_agent_driver(recorder.clone());
    // legacy：start_with_id（caller None）→ workflow_run.caller_id NULL。
    let outcome = executor
        .start_with_id("wf-ma3-caller-binding", None, &json!({}), None)
        .expect("legacy run succeeds");
    let run_caller: Option<String> = pool
        .with_conn(|c| {
            c.query_row(
                "SELECT caller_id FROM workflow_run WHERE run_id = ?1",
                [outcome.run_id.as_str()],
                |r| r.get(0),
            )
            .optional()
        })
        .unwrap()
        .flatten();
    assert_eq!(run_caller, None, "legacy run caller 列 NULL 零回归");
}

#[test]
fn params_caller_id_non_string_is_params_invalid() {
    let pool = migrated_pool();
    register_fixture(&pool);
    put_definition(&pool, BAD_PARAMS_DEF);
    let recorder = Arc::new(RecordingDriver::default());
    let executor = WorkflowExecutor::new(pool.clone()).with_agent_driver(recorder.clone());
    let outcome = executor
        .start_with_caller(
            "wf-ma3-bad-caller-params",
            None,
            &json!({}),
            None,
            Some("caller-a"),
        )
        .expect("run executes (fail-fast node)");
    assert_eq!(domain::WorkflowRunState::Failed, outcome.state);
    let node = outcome
        .nodes
        .iter()
        .find(|n| n.node_id == "node1")
        .expect("node1 row");
    assert_eq!(domain::WorkflowNodeRunState::Failed, node.state);
    let error = node.error.clone().expect("node error payload");
    assert_eq!(error["code"], json!("params_invalid"));
    assert!(
        error["message"]
            .as_str()
            .unwrap()
            .contains("caller_id must be a string"),
        "params_invalid message carries the type guard word form: {}",
        error["message"]
    );
    assert!(
        recorder.seen.lock().unwrap().is_empty(),
        "驱动不得被非法 params 调用"
    );
}

#[test]
fn dual_caller_agent_rows_and_guardrail_attribution() {
    let pool = migrated_pool();
    register_fixture(&pool);
    add_caller(&pool, "caller-a");
    add_caller(&pool, "caller-b");
    put_definition(&pool, TWO_AGENT_DEF);
    // caller-a 总预算帽 1 步（首步执行后整轮硬停，GUARDRAIL
    // budget_caller_total 落行——审计分账实测载体）；caller-b 不设帽。
    set_budget(&pool, "caller-a", 0, 0, 60000, 0, 1);
    let executor = WorkflowExecutor::new(pool.clone())
        .with_agent_driver(Arc::new(LoopBindingDriver { pool: pool.clone() }));
    let outcome = executor
        .start_with_caller(
            "wf-ma3-caller-binding",
            None,
            &json!({}),
            None,
            Some("caller-a"),
        )
        .expect("run to terminal");
    assert_eq!(domain::WorkflowRunState::Succeeded, outcome.state);
    // workflow_run.caller_id = caller-a。
    let run_caller: Option<String> = pool
        .with_conn(|c| {
            c.query_row(
                "SELECT caller_id FROM workflow_run WHERE run_id = ?1",
                [outcome.run_id.as_str()],
                |r| r.get(0),
            )
            .optional()
        })
        .unwrap()
        .flatten();
    assert_eq!(run_caller.as_deref(), Some("caller-a"));
    // agent_run 两行 caller_id 分属两 caller（库直查，节点链接列锚定）。
    let node1_run =
        node_agent_run_id(&pool, outcome.run_id.as_str(), "node1").expect("node1 agent_run link");
    let node2_run =
        node_agent_run_id(&pool, outcome.run_id.as_str(), "node2").expect("node2 agent_run link");
    assert_eq!(
        agent_run_caller(&pool, &node1_run).as_deref(),
        Some("caller-a"),
        "node1（无绑定）继承 run 级 caller-a"
    );
    assert_eq!(
        agent_run_caller(&pool, &node2_run).as_deref(),
        Some("caller-b"),
        "node2（params caller_id=caller-b）落 caller-b"
    );
    // 两 agent_run 行 trace_id 同链（015 单锚传播）。
    let traces: Vec<String> = pool
        .with_conn(|c| -> Result<Vec<String>, database::DbError> {
            let mut stmt = c.prepare("SELECT trace_id FROM agent_run WHERE run_id IN (?1, ?2)")?;
            let rows = stmt
                .query_map(rusqlite::params![node1_run, node2_run], |r| r.get(0))?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(rows)
        })
        .unwrap();
    assert!(
        traces.iter().all(|t| t == outcome.run_id.as_str()),
        "两节点 agent_run trace_id = workflow run_id"
    );
    // GUARDRAIL 审计行按 caller 分账：caller-a 总预算帽 budget_caller_total
    // 落行；caller-b 无帽零预算行。
    let caller_a_guardrails = guardrail_rows(&pool, "caller-a");
    assert!(
        caller_a_guardrails
            .iter()
            .any(|(a, d)| a == "agent.run.guardrail" && d.contains("budget_caller_total")),
        "caller-a budget_caller_total GUARDRAIL 行在位: {caller_a_guardrails:?}"
    );
    assert!(
        guardrail_rows(&pool, "caller-b").is_empty(),
        "caller-b 无预算帽 → 零 GUARDRAIL 预算行"
    );
}

#[test]
fn caller_budget_cap_denies_node_with_budget_denied_word_form() {
    let pool = migrated_pool();
    register_fixture(&pool);
    add_caller(&pool, "caller-a");
    add_caller(&pool, "caller-b");
    put_definition(&pool, TWO_AGENT_DEF);
    // caller-b 跨 run 并发帽 = 1，且已有一个在飞 caller-b run（QUEUED 记账）
    // ——node2 受理面 BudgetDenied 硬拒，run 不落行。
    set_budget(&pool, "caller-b", 0, 1, 60000, 0, 0);
    let store = AgentStore::open(pool.clone(), Registry::open(pool.clone()));
    let preexisting = AgentRun::try_new(
        domain::RunId::try_new(format!("arn-ma3-preexisting-{}", std::process::id())).unwrap(),
        None,
        AgentId::try_new(AGENT_ID).unwrap(),
        "0.1.0".to_string(),
        domain::AgentRunState::Queued,
        Vec::new(),
    )
    .unwrap();
    store
        .create_run(&preexisting, Some("tr-ma3-cap"), Some("caller-b"))
        .expect("pre-existing caller-b run row");
    let executor = WorkflowExecutor::new(pool.clone())
        .with_agent_driver(Arc::new(LoopBindingDriver { pool: pool.clone() }));
    let outcome = executor
        .start_with_caller(
            "wf-ma3-caller-binding",
            None,
            &json!({}),
            None,
            Some("caller-a"),
        )
        .expect("run executes (fail-fast at node2)");
    // run fail-fast 收敛 failed；node1 成功（caller-a 无帽）、node2 帽拒。
    assert_eq!(domain::WorkflowRunState::Failed, outcome.state);
    let node1 = outcome.nodes.iter().find(|n| n.node_id == "node1").unwrap();
    assert_eq!(domain::WorkflowNodeRunState::Succeeded, node1.state);
    let node2 = outcome.nodes.iter().find(|n| n.node_id == "node2").unwrap();
    assert_eq!(domain::WorkflowNodeRunState::Failed, node2.state);
    let error = node2.error.clone().expect("node2 error payload");
    assert_eq!(error["code"], json!("agent_failed"));
    let message = error["message"].as_str().unwrap();
    assert!(
        message.contains("budget denied"),
        "budget denied 词形实测: {message}"
    );
    assert!(
        message.contains("max_concurrent_runs"),
        "帽语义词形实测: {message}"
    );
    // node2 BudgetDenied run 不落行（受理前置）。
    let node2_agent_run = node_agent_run_id(&pool, outcome.run_id.as_str(), "node2").unwrap();
    assert!(
        agent_run_caller(&pool, &node2_agent_run).is_none(),
        "BudgetDenied run 不落行（agent_run 缺失）"
    );
    // caller-a 分账照常（node1 一步 tool_run.caller_id = caller-a）。
    let caller_a_tool_runs: i64 = pool
        .with_conn(|c| {
            c.query_row(
                "SELECT COUNT(*) FROM tool_run WHERE caller_id = 'caller-a'",
                [],
                |r| r.get(0),
            )
        })
        .unwrap();
    assert!(caller_a_tool_runs >= 1, "caller-a 步记账在位");
}

/// D7 收尾对照：claim 队列与 workflow 执行器解耦——executor.run 全程零
/// task_claim 行自动创建（claim 创建面 = 编排方显式登记）。
#[test]
fn executor_run_creates_zero_claims() {
    let pool = migrated_pool();
    register_fixture(&pool);
    add_caller(&pool, "caller-a");
    put_definition(&pool, TWO_AGENT_DEF);
    let recorder = Arc::new(RecordingDriver::default());
    let executor = WorkflowExecutor::new(pool.clone()).with_agent_driver(recorder.clone());
    executor
        .start_with_caller(
            "wf-ma3-caller-binding",
            None,
            &json!({}),
            None,
            Some("caller-a"),
        )
        .expect("run succeeds");
    let claims: i64 = pool
        .with_conn(|c| c.query_row("SELECT COUNT(*) FROM task_claim", [], |r| r.get(0)))
        .unwrap();
    assert_eq!(claims, 0, "workflow 执行器零自动建 claim（D7）");
    // 编排方显式登记面在位（rp-collab / MCP 同一 ClaimStore 入口）。
    let store = ClaimStore::new(pool);
    let row = store
        .create(&ClaimCreateInput {
            kind: ClaimKind::AgentSubtask,
            title: "显式登记对照".to_string(),
            payload_json: "{}".to_string(),
            run_id: None,
            node_id: None,
            agent_id: None,
            depends_on_claim_id: None,
            created_by: "caller-a".to_string(),
        })
        .expect("explicit claim create");
    assert!(row.claim_id.starts_with("tcl-"));
}
