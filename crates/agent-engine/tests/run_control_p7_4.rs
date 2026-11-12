//! P7-4 运行控制契约验收（编排批次 P7-4 第 4/7 批，ROADMAP §8.3 P7-5 判据
//! 「每项控制各一个契约测试用例，取消走协作式语义与 ToolRun D-TC-04 一致」，
//! 端到端用例为「一次 run 暂停后续跑，已完成步骤不重跑且 audit_event 事件链
//! 连续」）。
//!
//! 九类控制（计划 §9:474 能力清单，全项与 P7-4 同批落地，ROADMAP §8.5
//! 风险 1 不留「先上循环后补护栏」缺口），用例对照：
//! 1. 暂停（协作式请求位，步间消费）：pause_contract_request_is_signal...
//! 2. 恢复（轨迹续接，已完成步骤不重跑）：pause_resume_e2e_...（兼端到端）
//! 3. 取消（RUNNING 协作式 + QUEUED 直接跃迁）：cancel_cooperative_and_queued_direct
//! 4. 重试（FAILED 新 run 重放同定义版本）：retry_replays_failed_run_...
//! 5. 停滞检测（连续无新证据哈希）：stall_detection_...
//! 6. 重复调用检测（规范化键判重跳过）：duplicate_call_detection_...
//! 7. 目标偏移检测（初始计划面外跳转）：goal_drift_detection_...
//! 8. 证据不足提示（可用证据低于阈值留痕）：insufficient_evidence_hint_...
//! 9. 预算与资源限制（轮次预算 + 并行子任务默认关闭串行化）：
//!    budget_rounds_and_parallel_serialization
//!
//! 全程内存库 + 真实 migrations（001 到 014）+ 仓库登记 Manifest，控制回路
//! 规划器经 PlanContext.run_id 对控制面发起协作式请求（D-TC-04 语义）。

#![deny(missing_docs)]

use agent_engine::agent_loop::{
    AgentLoop, Decision, KeywordSearchPlanner, LoopConfig, PlanContext, PlanStep, Planner,
    ResumeRequest, RunRequest,
};
use agent_engine::broker_link::{BrokerLink, FirstFamilyConfirmer};
use agent_engine::control::{RunControl, RunControlConfig};
use agent_engine::evidence::EvidenceRegistrar;
use agent_engine::state::RunStateStore;
use agent_engine::store::AgentStore;
use agent_engine::tool_link::{RunDriver, ToolLink};
use context_broker::{ArtifactSource, Broker, ManifestStore, RegistrySource, RulepackSource};
use database::rusqlite::OptionalExtension;
use database::{DbPath, Pool, PoolConfig};
use domain::{AgentDefinition, AgentId, AgentRun, AgentRunState, CapabilityRef, RunId};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tool_engine::mock_runner::MockRunner;
use tool_engine::Engine;
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

/// 登记仓库真实 Manifest 并置 ENABLED（Broker 目录面可见口径）。
fn register_tool(pool: &Pool, tool_id: &str) {
    let manifest_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../manifests/tools")
        .join(format!("{tool_id}.json"));
    let manifest: Value =
        serde_json::from_str(&std::fs::read_to_string(manifest_path).unwrap()).unwrap();
    Registry::open(pool.clone()).register(&manifest).unwrap();
    pool.with_conn(|c| {
        c.execute(
            "UPDATE tool SET lifecycle = 'ENABLED' WHERE tool_id = ?1",
            [tool_id],
        )
    })
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

/// 计数驱动（统计工具真实执行次数；resume 不重跑的判据面）。可 Clone，
/// 测试侧留句柄在 build_loop 消费驱动后继续读计数。
///
/// 按工具派发信封载荷（output_schema 校验要求每个被执行工具的结果合其
/// Manifest output_schema）：`by_tool` 命中用专用 runner，否则用缺省 runner。
#[derive(Clone)]
struct CountingDriver {
    default_runner: MockRunner,
    by_tool: Vec<(&'static str, MockRunner)>,
    calls: Arc<AtomicUsize>,
}

impl CountingDriver {
    fn succeed(result: Value, summary: &str) -> Self {
        Self {
            default_runner: MockRunner::succeed(result, summary),
            by_tool: Vec::new(),
            calls: Arc::new(AtomicUsize::new(0)),
        }
    }
    fn fail(category: &str, code: Value, message: &str) -> Self {
        Self {
            default_runner: MockRunner::fail(category, code, message),
            by_tool: Vec::new(),
            calls: Arc::new(AtomicUsize::new(0)),
        }
    }
    /// 挂按工具派发的成功 runner（信封载荷合该工具 output_schema）。
    fn with_tool_payload(mut self, tool_id: &'static str, result: Value, summary: &str) -> Self {
        self.by_tool
            .push((tool_id, MockRunner::succeed(result, summary)));
        self
    }
    fn count(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
    /// 按 run 行的 tool_id 选 runner（Engine::pool 公开视图，只读）。
    fn runner_for(&self, engine: &Engine, run_id: &str) -> &MockRunner {
        let tool_id: Option<String> = engine
            .pool()
            .with_conn(|c| {
                let mut stmt = c.prepare("SELECT tool_id FROM tool_run WHERE run_id = ?1")?;
                stmt.query_row([run_id], |r| r.get::<_, String>(0))
                    .optional()
            })
            .unwrap();
        match tool_id.as_deref() {
            Some(t) => self
                .by_tool
                .iter()
                .find(|(id, _)| *id == t)
                .map(|(_, r)| r)
                .unwrap_or(&self.default_runner),
            None => &self.default_runner,
        }
    }
}

impl RunDriver for CountingDriver {
    fn drive(
        &self,
        engine: &Engine,
        run_id: &str,
    ) -> Result<tool_engine::DriveOutcome, tool_engine::EngineError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let runner = self.runner_for(engine, run_id);
        engine.drive(run_id, runner)
    }
}

struct DeclineConfirmer;
impl agent_engine::broker_link::IntentConfirmer for DeclineConfirmer {
    fn confirm(&mut self, _alternatives: &[String], _goal: &str) -> Option<String> {
        None
    }
}

/// 组装闭环（驱动注入；control 参数缺省 = 低并发纪律最严面）。
fn build_loop(
    pool: &Pool,
    driver: CountingDriver,
    control: RunControlConfig,
    max_tool_calls: usize,
) -> (AgentLoop, AgentStore, RunStateStore) {
    let store = AgentStore::open(pool.clone(), Registry::open(pool.clone()));
    let state = RunStateStore::new(pool.clone());
    let engine = Engine::open(pool.clone(), Registry::open(pool.clone()));
    let loop_ = AgentLoop::new(
        store.clone(),
        build_broker_link(pool),
        ToolLink::new(engine, Box::new(driver)),
        EvidenceRegistrar::new(pool.clone()),
        Some(audit_engine::AuditEngine::new(pool.clone())),
        LoopConfig {
            workspace_id: "ws-default".to_string(),
            max_tool_calls,
        },
    )
    .with_control(control);
    (loop_, store, state)
}

/// 顺序关键词规划器：按已执行步数推进关键词；用尽即收尾。
struct KeywordSeqPlanner {
    keywords: Vec<&'static str>,
}

impl Planner for KeywordSeqPlanner {
    fn plan(&self, ctx: &PlanContext<'_>) -> Decision {
        if ctx.history.len() >= self.keywords.len() {
            return Decision::Finish {
                reason: "关键词用尽（顺序规划器收尾）".to_string(),
            };
        }
        Decision::Steps(vec![PlanStep {
            tool_id: "rip.kb.keyword_search".to_string(),
            version: None,
            input: json!({"keyword": self.keywords[ctx.history.len()]}),
            rationale: "顺序关键词规划（P7-4 契约测试）".to_string(),
        }])
    }
}

/// 控制回路规划器：到达指定轮次即对控制面发起协作式请求（D-TC-04），随后
/// 按顺序关键词推进（请求消费点决定步骤是否真的执行）。
struct ControlArmedPlanner {
    keywords: Vec<&'static str>,
    state: RunStateStore,
    arm_round: usize,
    pause_at_arm: bool,
}

impl Planner for ControlArmedPlanner {
    fn plan(&self, ctx: &PlanContext<'_>) -> Decision {
        if ctx.round == self.arm_round {
            if self.pause_at_arm {
                assert!(
                    self.state.request_pause(ctx.run_id).unwrap(),
                    "协作式暂停请求必须受理（RUNNING 期）"
                );
            } else {
                assert!(
                    self.state.request_cancel(ctx.run_id).unwrap(),
                    "协作式取消请求必须受理（RUNNING 期）"
                );
            }
        }
        if ctx.history.len() >= self.keywords.len() {
            return Decision::Finish {
                reason: "关键词用尽（控制回路规划器收尾）".to_string(),
            };
        }
        Decision::Steps(vec![PlanStep {
            tool_id: "rip.kb.keyword_search".to_string(),
            version: None,
            input: json!({"keyword": self.keywords[ctx.history.len()]}),
            rationale: "控制回路规划（P7-4 契约测试）".to_string(),
        }])
    }
}

/// 重复规划器：第二轮重复第一轮的 (tool, input) 规范键。
struct DuplicatePlanner;

impl Planner for DuplicatePlanner {
    fn plan(&self, ctx: &PlanContext<'_>) -> Decision {
        if ctx.history.is_empty() {
            Decision::Steps(vec![PlanStep {
                tool_id: "rip.kb.keyword_search".to_string(),
                version: None,
                input: json!({"keyword": "same"}),
                rationale: "首轮（重复判重基准）".to_string(),
            }])
        } else {
            Decision::Steps(vec![PlanStep {
                tool_id: "rip.kb.keyword_search".to_string(),
                version: None,
                input: json!({"keyword": "same"}),
                rationale: "同键重复轮（判重跳过面）".to_string(),
            }])
        }
    }
}

/// 恒定规划器：每轮都产出下一步（预算/停滞护栏的驱动力）。
struct AlwaysPlanPlanner {
    counter: AtomicUsize,
}

impl Planner for AlwaysPlanPlanner {
    fn plan(&self, _ctx: &PlanContext<'_>) -> Decision {
        let n = self.counter.fetch_add(1, Ordering::SeqCst);
        Decision::Steps(vec![PlanStep {
            tool_id: "rip.kb.keyword_search".to_string(),
            version: None,
            input: json!({"keyword": format!("kw{n}")}),
            rationale: "恒定规划（护栏驱动）".to_string(),
        }])
    }
}

/// 偏移规划器：轮 0 在初始计划面内（A），轮 1 跳面外（B，容限 1 放行并
/// 留痕），轮 2 再跳另一面外（C，连续偏移超容限即停，C 不执行）。
struct DriftPlanner;

impl Planner for DriftPlanner {
    fn plan(&self, ctx: &PlanContext<'_>) -> Decision {
        let (tool, input): (&str, Value) = match ctx.round {
            0 => ("rip.kb.keyword_search", json!({"keyword": "alpha"})),
            1 => ("rip.doc.parse", json!({"text": "偏移步（容限内放行）"})),
            _ => ("rip.text.embed", json!({"text": "连续偏移步（应被停）"})),
        };
        Decision::Steps(vec![PlanStep {
            tool_id: tool.to_string(),
            version: None,
            input,
            rationale: format!("偏移面规划 round {}", ctx.round),
        }])
    }
}

/// 默认成功信封载荷（合工具 output_schema；不同 keyword 也返回同载荷：
/// 停滞判据用哈希不重）。
fn mock_payload() -> Value {
    json!({"keyword": "mock", "per_group_limit": 5, "total_hits": 0, "groups": []})
}

fn run_request(agent_id: &str, planner: Box<dyn Planner>, goal: &str) -> RunRequest {
    RunRequest {
        agent_id: agent_id.to_string(),
        agent_version: None,
        goal: goal.to_string(),
        task_id: Some("task-p74".to_string()),
        input: None,
        planner,
        confirmer: Box::new(DeclineConfirmer),
    }
}

fn audit_actions(pool: &Pool, run_id: &str) -> Vec<(i64, String)> {
    let audit = audit_engine::AuditEngine::new(pool.clone());
    audit
        .trace(run_id)
        .unwrap()
        .into_iter()
        .map(|s| (s.event_id, s.event.action))
        .collect()
}

fn assert_action_present(actions: &[(i64, String)], action: &str) -> usize {
    let pos = actions
        .iter()
        .position(|(_, a)| a == action)
        .unwrap_or_else(|| panic!("audit chain missing {action}: {actions:?}"));
    pos
}

// ---------------------------------------------------------------------------
// 1 + 2 + 端到端：暂停（协作式）→ 恢复（轨迹续接，已完成步骤不重跑，
// audit_event 事件链连续）
// ---------------------------------------------------------------------------

#[test]
fn pause_resume_e2e_state_continuity_no_rerun_and_audit_chain() {
    let pool = migrated_pool();
    register_tool(&pool, "rip.kb.keyword_search");
    let driver = CountingDriver::succeed(mock_payload(), "关键词检索完成（mock 注入）");
    let driver_handle = driver.clone();
    let (loop_, store, state) = build_loop(&pool, driver, RunControlConfig::default(), 4);
    let definition = AgentDefinition::try_new(
        AgentId::try_new("agent.p74.pause").unwrap(),
        "0.1.0".to_string(),
        "{}".to_string(),
        vec![CapabilityRef::try_new("rip.kb.keyword_search", None).unwrap()],
    )
    .unwrap();
    store
        .register_definition(&definition, "暂停恢复测试 Agent")
        .unwrap();

    let state_store = state.clone();
    let planner = ControlArmedPlanner {
        keywords: vec!["alpha", "beta"],
        state: state_store,
        arm_round: 1,
        pause_at_arm: true,
    };
    let mut req = run_request(
        "agent.p74.pause",
        Box::new(planner),
        "检索 nmap 端口扫描相关知识并调用登记工具",
    );
    let paused_report = loop_.run(&mut req).unwrap();
    assert_eq!(paused_report.state, "PAUSED", "report: {paused_report:?}");
    assert!(
        paused_report.finish_reason.contains("paused"),
        "finish_reason: {}",
        paused_report.finish_reason
    );
    assert_eq!(paused_report.steps.len(), 1, "暂停前只完成一步");
    assert!(
        !paused_report.warnings.iter().any(|w| w.contains("resumed")),
        "暂停报告不得带 resumed 留痕: {:?}",
        paused_report.warnings
    );

    // 暂停态落库：run 行 PAUSED、控制行带 goal、轨迹 1 步。
    let stored = store.run(&paused_report.run_id).unwrap().unwrap();
    assert_eq!(stored.run.state(), AgentRunState::Paused);
    assert!(stored.started_at.is_some() && stored.finished_at.is_none());
    assert_eq!(
        state.goal_text(&paused_report.run_id).unwrap().as_deref(),
        Some("检索 nmap 端口扫描相关知识并调用登记工具")
    );
    let trajectory = state.steps(&paused_report.run_id).unwrap();
    assert_eq!(trajectory.len(), 1);
    let step1_tool_run = trajectory[0].record.tool_run_id.clone();
    assert_eq!(driver_handle.count(), 1, "暂停前工具真实执行一次");

    // 暂停事件链已落 audit_event（correlation_id = run_id）。
    let paused_actions = audit_actions(&pool, &paused_report.run_id);
    let accepted_pos = assert_action_present(&paused_actions, "agent.run.accepted");
    let paused_pos = assert_action_present(&paused_actions, "agent.run.paused");
    assert!(accepted_pos < paused_pos, "accepted 必须先于 paused");
    assert!(
        !paused_actions
            .iter()
            .any(|(_, a)| a == "agent.run.finished"),
        "暂停收尾不是 finished"
    );

    // ---- 恢复：轨迹续接，已完成步骤不重跑 ----
    let mut resume = ResumeRequest {
        run_id: paused_report.run_id.clone(),
        planner: Box::new(KeywordSeqPlanner {
            keywords: vec!["alpha", "beta"],
        }),
        confirmer: Box::new(FirstFamilyConfirmer::new(
            context_broker::IntentConfig::from_dir(
                &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../rulepacks/rip-rules"),
            )
            .unwrap(),
        )),
    };
    let final_report = loop_.resume(&mut resume).unwrap();
    assert_eq!(final_report.state, "SUCCEEDED", "report: {final_report:?}");
    assert_eq!(final_report.steps.len(), 2, "恢复后补齐第二步");
    assert_eq!(final_report.resumed_steps, 1, "轨迹续接 1 步");
    assert_eq!(
        driver_handle.count(),
        2,
        "已完成步骤不得重跑（总执行次数不变增量）"
    );
    assert_eq!(
        final_report.steps[0].tool_run_id, step1_tool_run,
        "第一步 tool_run_id 必须与暂停前一致（不重跑）"
    );
    assert_eq!(final_report.steps[1].status, "SUCCEEDED");
    assert!(
        final_report
            .warnings
            .iter()
            .any(|w| w.contains("resumed: 1 persisted steps replayed")),
        "恢复留痕: {:?}",
        final_report.warnings
    );

    // run 行终态 + 轨迹 2 步 + memory_refs 记账连续。
    let done = store.run(&paused_report.run_id).unwrap().unwrap();
    assert_eq!(done.run.state(), AgentRunState::Succeeded);
    assert!(done.finished_at.is_some());
    let trajectory_after = state.steps(&paused_report.run_id).unwrap();
    assert_eq!(trajectory_after.len(), 2, "轨迹逐步骤追加");
    assert_eq!(trajectory_after[0].record.tool_run_id, step1_tool_run);

    // ---- audit_event 事件链连续（correlation_id 全程 = run_id）----
    let final_actions = audit_actions(&pool, &paused_report.run_id);
    let accepted_pos = assert_action_present(&final_actions, "agent.run.accepted");
    let paused_pos = assert_action_present(&final_actions, "agent.run.paused");
    let resumed_pos = assert_action_present(&final_actions, "agent.run.resumed");
    let finished_pos = assert_action_present(&final_actions, "agent.run.finished");
    let step_positions: Vec<usize> = final_actions
        .iter()
        .enumerate()
        .filter(|(_, (_, a))| a == "agent.step.completed")
        .map(|(i, _)| i)
        .collect();
    assert_eq!(step_positions.len(), 2, "两步各一条 completed 事件");
    assert!(
        accepted_pos < step_positions[0] && step_positions[0] < paused_pos,
        "accepted < step1 < paused（事件链连续）: {final_actions:?}"
    );
    assert!(
        paused_pos < resumed_pos
            && resumed_pos < step_positions[1]
            && step_positions[1] < finished_pos,
        "paused < resumed < step2 < finished（事件链连续）: {final_actions:?}"
    );

    // 非暂停态拒绝恢复（SUCCEEDED run 不可 resume）。
    let mut again = ResumeRequest {
        run_id: paused_report.run_id.clone(),
        planner: Box::new(KeywordSearchPlanner),
        confirmer: Box::new(DeclineConfirmer),
    };
    assert!(loop_.resume(&mut again).is_err(), "终态 run 恢复必须拒绝");
}

/// 暂停契约用例（单独面）：请求只写信号位不立即跃迁；消费点才 PAUSED。
#[test]
fn pause_contract_request_is_signal_and_loop_consumes_it() {
    let pool = migrated_pool();
    register_tool(&pool, "rip.kb.keyword_search");
    let driver = CountingDriver::succeed(mock_payload(), "ok");
    let (loop_, store, state) = build_loop(&pool, driver, RunControlConfig::default(), 4);
    let definition = AgentDefinition::try_new(
        AgentId::try_new("agent.p74.pause2").unwrap(),
        "0.1.0".to_string(),
        "{}".to_string(),
        vec![CapabilityRef::try_new("rip.kb.keyword_search", None).unwrap()],
    )
    .unwrap();
    store
        .register_definition(&definition, "暂停契约 Agent")
        .unwrap();

    // 直接面：对不存在 run 的请求报 RunNotFound；对终态 run 拒绝。
    let control = RunControl::new(store.clone(), state.clone());
    assert!(matches!(
        control.pause("arn-no-such-run"),
        Err(agent_engine::AgentError::RunNotFound(_))
    ));

    // 造一个 FAILED run：pause 谓词拒绝（Refused(FAILED)）。
    let run = AgentRun::try_new(
        RunId::try_new("arn-p74-failed-src").unwrap(),
        None,
        AgentId::try_new("agent.p74.pause2").unwrap(),
        "0.1.0".to_string(),
        AgentRunState::Queued,
        vec![],
    )
    .unwrap();
    store.create_run(&run).unwrap();
    store
        .transition_run("arn-p74-failed-src", AgentRunState::Running)
        .unwrap();
    store
        .transition_run("arn-p74-failed-src", AgentRunState::Failed)
        .unwrap();
    state.ensure_control_row("arn-p74-failed-src", "").unwrap();
    assert_eq!(
        control.pause("arn-p74-failed-src").unwrap(),
        agent_engine::PauseOutcome::Refused("FAILED".to_string())
    );

    // 运行中协作暂停：消费点 PAUSED，工具步骤停在请求轮。
    let planner = ControlArmedPlanner {
        keywords: vec!["alpha", "beta"],
        state: state.clone(),
        arm_round: 1,
        pause_at_arm: true,
    };
    let mut req = run_request("agent.p74.pause2", Box::new(planner), "zzzq 暂停契约词串");
    let report = loop_.run(&mut req).unwrap();
    assert_eq!(report.state, "PAUSED");
    assert_eq!(report.steps.len(), 1);
    assert!(
        !state.consume_pause_request(&report.run_id).unwrap(),
        "请求位消费即清空，不得残留"
    );
}

// ---------------------------------------------------------------------------
// 3. 取消（协作式语义，D-TC-04）+ QUEUED 直接取消
// ---------------------------------------------------------------------------

#[test]
fn cancel_cooperative_and_queued_direct() {
    let pool = migrated_pool();
    register_tool(&pool, "rip.kb.keyword_search");
    let driver = CountingDriver::succeed(mock_payload(), "ok");
    let (loop_, store, state) = build_loop(&pool, driver, RunControlConfig::default(), 4);
    let definition = AgentDefinition::try_new(
        AgentId::try_new("agent.p74.cancel").unwrap(),
        "0.1.0".to_string(),
        "{}".to_string(),
        vec![CapabilityRef::try_new("rip.kb.keyword_search", None).unwrap()],
    )
    .unwrap();
    store
        .register_definition(&definition, "取消测试 Agent")
        .unwrap();
    let control = RunControl::new(store.clone(), state.clone());

    // 协作式：RUNNING 期写请求位，执行循环步间消费后 CANCELLED。
    let planner = ControlArmedPlanner {
        keywords: vec!["alpha", "beta"],
        state: state.clone(),
        arm_round: 1,
        pause_at_arm: false,
    };
    let mut req = run_request("agent.p74.cancel", Box::new(planner), "zzzq 取消协作词串");
    let report = loop_.run(&mut req).unwrap();
    assert_eq!(report.state, "CANCELLED", "report: {report:?}");
    assert!(
        report
            .finish_reason
            .contains("cancelled (cooperative, D-TC-04)"),
        "finish_reason: {}",
        report.finish_reason
    );
    assert_eq!(report.steps.len(), 1, "取消前完成的一步保留在轨迹");
    let stored = store.run(&report.run_id).unwrap().unwrap();
    assert_eq!(stored.run.state(), AgentRunState::Cancelled);
    assert!(stored.finished_at.is_some());
    let actions = audit_actions(&pool, &report.run_id);
    assert_action_present(&actions, "agent.run.cancelled");
    assert!(
        !actions.iter().any(|(_, a)| a == "agent.run.finished"),
        "协作取消收尾不是 finished"
    );

    // QUEUED 直接取消（不在执行循环内，直接跃迁 CANCELLED）。
    let queued = AgentRun::try_new(
        RunId::try_new("arn-p74-queued-cancel").unwrap(),
        None,
        AgentId::try_new("agent.p74.cancel").unwrap(),
        "0.1.0".to_string(),
        AgentRunState::Queued,
        vec![],
    )
    .unwrap();
    store.create_run(&queued).unwrap();
    state
        .ensure_control_row("arn-p74-queued-cancel", "")
        .unwrap();
    let outcome = control.cancel("arn-p74-queued-cancel").unwrap();
    assert_eq!(outcome, agent_engine::CancelOutcome::CancelledNow);
    let stored = store.run("arn-p74-queued-cancel").unwrap().unwrap();
    assert_eq!(stored.run.state(), AgentRunState::Cancelled);
    assert!(stored.finished_at.is_some());
    let direct_actions = audit_actions(&pool, "arn-p74-queued-cancel");
    // 直接取消不产生执行循环事件（accepted 缺席，run 未经过 loop）。
    assert!(
        !direct_actions
            .iter()
            .any(|(_, a)| a == "agent.run.accepted"),
        "QUEUED 直接取消不经执行循环"
    );

    // 终态取消拒绝（SUCCEEDED 不可取消）。
    let done = AgentRun::try_new(
        RunId::try_new("arn-p74-done-cancel").unwrap(),
        None,
        AgentId::try_new("agent.p74.cancel").unwrap(),
        "0.1.0".to_string(),
        AgentRunState::Queued,
        vec![],
    )
    .unwrap();
    store.create_run(&done).unwrap();
    state.ensure_control_row("arn-p74-done-cancel", "").unwrap();
    store
        .transition_run("arn-p74-done-cancel", AgentRunState::Running)
        .unwrap();
    store
        .transition_run("arn-p74-done-cancel", AgentRunState::Succeeded)
        .unwrap();
    assert_eq!(
        control.cancel("arn-p74-done-cancel").unwrap(),
        agent_engine::CancelOutcome::Refused("SUCCEEDED".to_string())
    );
    // 不存在的 run 报 RunNotFound。
    assert!(matches!(
        control.cancel("arn-no-such-run"),
        Err(agent_engine::AgentError::RunNotFound(_))
    ));
}

// ---------------------------------------------------------------------------
// 4. 重试：FAILED 新 run 重放同定义版本（源 run 终态不复活）
// ---------------------------------------------------------------------------

#[test]
fn retry_replays_failed_run_as_new_run_with_link() {
    let pool = migrated_pool();
    register_tool(&pool, "rip.kb.keyword_search");
    let driver = CountingDriver::succeed(mock_payload(), "ok");
    let (_loop_, store, state) = build_loop(&pool, driver, RunControlConfig::default(), 4);
    let definition = AgentDefinition::try_new(
        AgentId::try_new("agent.p74.retry").unwrap(),
        "0.1.0".to_string(),
        "{}".to_string(),
        vec![CapabilityRef::try_new("rip.kb.keyword_search", None).unwrap()],
    )
    .unwrap();
    store
        .register_definition(&definition, "重试测试 Agent")
        .unwrap();
    let control = RunControl::new(store.clone(), state.clone());

    // 源 run：FAILED 终态（带 task_id 与钉定版本）。
    let source = AgentRun::try_new(
        RunId::try_new("arn-p74-retry-src").unwrap(),
        Some("task-retry-42".to_string()),
        AgentId::try_new("agent.p74.retry").unwrap(),
        "0.1.0".to_string(),
        AgentRunState::Queued,
        vec![],
    )
    .unwrap();
    store.create_run(&source).unwrap();
    state
        .ensure_control_row("arn-p74-retry-src", "源目标")
        .unwrap();
    store
        .transition_run("arn-p74-retry-src", AgentRunState::Running)
        .unwrap();
    store
        .transition_run("arn-p74-retry-src", AgentRunState::Failed)
        .unwrap();

    let outcome = control.retry("arn-p74-retry-src").unwrap();
    assert_eq!(outcome.source_run_id, "arn-p74-retry-src");
    assert!(
        outcome.new_run_id.starts_with("arn-"),
        "新 run id: {outcome:?}"
    );

    // 新 run：QUEUED、同 agent/版本/task 重放、retry_of_run_id 链回源。
    let new_run = store.run(&outcome.new_run_id).unwrap().unwrap();
    assert_eq!(new_run.run.state(), AgentRunState::Queued);
    assert_eq!(new_run.run.agent_version(), "0.1.0", "重放钉定同定义版本");
    assert_eq!(new_run.run.agent_id().as_str(), "agent.p74.retry");
    assert_eq!(new_run.run.task_id(), Some("task-retry-42"));
    assert_eq!(
        state.retry_of(&outcome.new_run_id).unwrap().as_deref(),
        Some("arn-p74-retry-src")
    );
    // 源 run 保持 FAILED 终态不复活（域跃迁表：终态不可迁出）。
    assert_eq!(
        store.run("arn-p74-retry-src").unwrap().unwrap().run.state(),
        AgentRunState::Failed
    );

    // 非 FAILED 源拒绝重试（QUEUED/成功/取消都拒绝）。
    assert!(
        control.retry(&outcome.new_run_id).is_err(),
        "QUEUED 不可重试"
    );
    let err_text = control.retry("arn-p74-no-such").unwrap_err().to_string();
    assert!(err_text.contains("not found"), "err: {err_text}");
}

// ---------------------------------------------------------------------------
// 5. 停滞检测：连续无新证据哈希步数达窗口即停（mock 恒载荷 → 哈希不重）
// ---------------------------------------------------------------------------

#[test]
fn stall_detection_stops_after_window_without_new_evidence() {
    let pool = migrated_pool();
    register_tool(&pool, "rip.kb.keyword_search");
    let driver = CountingDriver::succeed(mock_payload(), "ok");
    let (loop_, store, _state) = build_loop(&pool, driver, RunControlConfig::default(), 6);
    let definition = AgentDefinition::try_new(
        AgentId::try_new("agent.p74.stall").unwrap(),
        "0.1.0".to_string(),
        "{}".to_string(),
        vec![CapabilityRef::try_new("rip.kb.keyword_search", None).unwrap()],
    )
    .unwrap();
    store
        .register_definition(&definition, "停滞测试 Agent")
        .unwrap();

    // 不同关键词（不触发重复判重）、同载荷（哈希不重）：三步触发窗口 2。
    let planner = AlwaysPlanPlanner {
        counter: AtomicUsize::new(0),
    };
    let mut req = run_request("agent.p74.stall", Box::new(planner), "zzzq 停滞检测词串");
    let report = loop_.run(&mut req).unwrap();
    assert_eq!(report.state, "SUCCEEDED", "report: {report:?}");
    assert_eq!(
        report.steps.len(),
        3,
        "第 3 步触发停滞窗口 2（第 1 步有新哈希）"
    );
    assert!(
        report.finish_reason.contains("guardrail: stall detected"),
        "finish_reason: {}",
        report.finish_reason
    );
    assert!(
        report
            .finish_reason
            .contains("2 steps without new evidence hash"),
        "finish_reason: {}",
        report.finish_reason
    );
    let actions = audit_actions(&pool, &report.run_id);
    assert_action_present(&actions, "agent.run.guardrail");
    // 轨迹 3 步全落库（停滞前的执行都留痕）。
    assert_eq!(
        report
            .steps
            .iter()
            .filter(|s| s.status == "SUCCEEDED")
            .count(),
        3
    );
}

// ---------------------------------------------------------------------------
// 6. 重复调用检测：同 (tool, 规范化 input) 已执行即跳过；整轮全是重复
//    即停（防规划器死循环）
// ---------------------------------------------------------------------------

#[test]
fn duplicate_call_detection_skips_and_stops_when_exhausted() {
    let pool = migrated_pool();
    register_tool(&pool, "rip.kb.keyword_search");
    let driver = CountingDriver::succeed(mock_payload(), "ok");
    let (loop_, store, _state) = build_loop(&pool, driver, RunControlConfig::default(), 6);
    let definition = AgentDefinition::try_new(
        AgentId::try_new("agent.p74.dup").unwrap(),
        "0.1.0".to_string(),
        "{}".to_string(),
        vec![CapabilityRef::try_new("rip.kb.keyword_search", None).unwrap()],
    )
    .unwrap();
    store
        .register_definition(&definition, "判重测试 Agent")
        .unwrap();

    let mut req = run_request("agent.p74.dup", Box::new(DuplicatePlanner), "zzzq 判重词串");
    let report = loop_.run(&mut req).unwrap();
    assert_eq!(report.state, "SUCCEEDED", "report: {report:?}");
    assert_eq!(report.steps.len(), 1, "重复步不产生新 StepRecord");
    assert!(
        report
            .warnings
            .iter()
            .any(|w| w.contains("duplicate call detected")),
        "warnings: {:?}",
        report.warnings
    );
    assert!(
        report
            .finish_reason
            .contains("guardrail: duplicate call detected"),
        "finish_reason: {}",
        report.finish_reason
    );
    let actions = audit_actions(&pool, &report.run_id);
    assert_action_present(&actions, "agent.step.duplicate");
    assert_action_present(&actions, "agent.run.guardrail");
    // 轨迹只有 1 步（重复跳步不入轨迹；轨迹读写一致性在 e2e 用例覆盖）。
    let traj = RunStateStore::new(pool.clone())
        .steps(&report.run_id)
        .unwrap();
    assert_eq!(traj.len(), 1, "重复跳步不得入轨迹");
}

// ---------------------------------------------------------------------------
// 7. 目标偏移检测：初始计划面外工具跳转计偏移事件，连续超过容限即停
// ---------------------------------------------------------------------------

#[test]
fn goal_drift_detection_stops_after_tolerance() {
    let pool = migrated_pool();
    register_tool(&pool, "rip.kb.keyword_search");
    register_tool(&pool, "rip.doc.parse");
    register_tool(&pool, "rip.text.embed");
    // doc.parse 有独立 output_schema（required: format/source/sections/
    // heading_tree/stats），按工具派发合规载荷（容限内放行步要真能成功）。
    let driver = CountingDriver::succeed(mock_payload(), "ok").with_tool_payload(
        "rip.doc.parse",
        json!({"format": "md", "source": {}, "sections": [], "heading_tree": [], "stats": {}}),
        "doc parse ok",
    );
    let (loop_, store, _state) = build_loop(&pool, driver, RunControlConfig::default(), 6);
    let definition = AgentDefinition::try_new(
        AgentId::try_new("agent.p74.drift").unwrap(),
        "0.1.0".to_string(),
        "{}".to_string(),
        vec![
            CapabilityRef::try_new("rip.kb.keyword_search", None).unwrap(),
            CapabilityRef::try_new("rip.doc.parse", None).unwrap(),
            CapabilityRef::try_new("rip.text.embed", None).unwrap(),
        ],
    )
    .unwrap();
    store
        .register_definition(&definition, "偏移测试 Agent")
        .unwrap();

    let mut req = run_request("agent.p74.drift", Box::new(DriftPlanner), "zzzq 偏移词串");
    let report = loop_.run(&mut req).unwrap();
    // 轮 0 A 成功；轮 1 B 偏移事件（容限 1 放行执行）；轮 2 C 连续偏移 2
    // 超容限 → 停（C 不执行）。
    assert_eq!(report.state, "SUCCEEDED", "report: {report:?}");
    assert_eq!(report.steps.len(), 2, "偏移停机步（C）不得执行");
    assert_eq!(report.steps[0].tool_id, "rip.kb.keyword_search");
    assert_eq!(report.steps[1].tool_id, "rip.doc.parse");
    assert!(
        report
            .warnings
            .iter()
            .any(|w| w.contains("goal drift event") && w.contains("rip.doc.parse")),
        "warnings: {:?}",
        report.warnings
    );
    assert!(
        report
            .finish_reason
            .contains("guardrail: goal drift detected"),
        "finish_reason: {}",
        report.finish_reason
    );
    assert!(
        report.finish_reason.contains("rip.text.embed"),
        "finish_reason 需点名偏移工具: {}",
        report.finish_reason
    );
    let actions = audit_actions(&pool, &report.run_id);
    let guardrail_pos = assert_action_present(&actions, "agent.run.guardrail");
    assert!(
        guardrail_pos > assert_action_present(&actions, "agent.step.completed"),
        "护栏事件在步骤事件之后落"
    );
}

// ---------------------------------------------------------------------------
// 8. 证据不足提示：可用证据（SUCCEEDED 且登记 evidence）低于阈值留提示
// ---------------------------------------------------------------------------

#[test]
fn insufficient_evidence_hint_appears_on_zero_usable_evidence() {
    let pool = migrated_pool();
    register_tool(&pool, "rip.kb.keyword_search");
    let driver = CountingDriver::fail("EXECUTION", json!(-32006), "worker exploded");
    let (loop_, store, _state) = build_loop(&pool, driver, RunControlConfig::default(), 4);
    let definition = AgentDefinition::try_new(
        AgentId::try_new("agent.p74.hint").unwrap(),
        "0.1.0".to_string(),
        "{}".to_string(),
        vec![CapabilityRef::try_new("rip.kb.keyword_search", None).unwrap()],
    )
    .unwrap();
    store
        .register_definition(&definition, "证据提示 Agent")
        .unwrap();

    let mut req = run_request(
        "agent.p74.hint",
        Box::new(KeywordSearchPlanner),
        "zzzq 证据提示词串",
    );
    let report = loop_.run(&mut req).unwrap();
    assert_eq!(report.state, "FAILED", "report: {report:?}");
    assert!(
        report
            .warnings
            .iter()
            .any(|w| w.contains("insufficient evidence") && w.contains("0 usable evidence")),
        "warnings: {:?}",
        report.warnings
    );
    // 对照组：成功 run 不触发提示（可用证据 >= 阈值 1）。
    let driver2 = CountingDriver::succeed(mock_payload(), "ok");
    let (loop2, store2, _s2) = build_loop(&pool, driver2, RunControlConfig::default(), 4);
    let definition2 = AgentDefinition::try_new(
        AgentId::try_new("agent.p74.hint2").unwrap(),
        "0.1.0".to_string(),
        "{}".to_string(),
        vec![CapabilityRef::try_new("rip.kb.keyword_search", None).unwrap()],
    )
    .unwrap();
    store2
        .register_definition(&definition2, "证据提示对照 Agent")
        .unwrap();
    let mut req2 = run_request(
        "agent.p74.hint2",
        Box::new(KeywordSearchPlanner),
        "zzzq 证据提示对照词串",
    );
    let ok_report = loop2.run(&mut req2).unwrap();
    assert_eq!(ok_report.state, "SUCCEEDED");
    assert!(
        !ok_report
            .warnings
            .iter()
            .any(|w| w.contains("insufficient evidence")),
        "成功 run 不得触发证据不足提示: {:?}",
        ok_report.warnings
    );
}

// ---------------------------------------------------------------------------
// 9. 预算与资源限制：轮次预算到限即停 + 并行子任务默认关闭（多步骤串行化）
// ---------------------------------------------------------------------------

#[test]
fn budget_rounds_and_parallel_serialization() {
    let pool = migrated_pool();
    register_tool(&pool, "rip.kb.keyword_search");
    let driver = CountingDriver::succeed(mock_payload(), "ok");
    let (loop_, store, _state) = build_loop(
        &pool,
        driver,
        RunControlConfig {
            max_rounds: 1,
            ..RunControlConfig::default()
        },
        8,
    );
    let definition = AgentDefinition::try_new(
        AgentId::try_new("agent.p74.budget").unwrap(),
        "0.1.0".to_string(),
        "{}".to_string(),
        vec![CapabilityRef::try_new("rip.kb.keyword_search", None).unwrap()],
    )
    .unwrap();
    store
        .register_definition(&definition, "预算测试 Agent")
        .unwrap();

    // 轮次预算：max_rounds=1，恒定规划器在第 2 轮前被截停。
    let planner = AlwaysPlanPlanner {
        counter: AtomicUsize::new(0),
    };
    let mut req = run_request("agent.p74.budget", Box::new(planner), "zzzq 预算词串");
    let report = loop_.run(&mut req).unwrap();
    assert_eq!(report.state, "SUCCEEDED", "report: {report:?}");
    assert_eq!(report.steps.len(), 1, "轮次预算 1：只有首轮的 1 步执行");
    assert!(
        report
            .finish_reason
            .contains("guardrail: budget max_rounds=1 reached"),
        "finish_reason: {}",
        report.finish_reason
    );
    let actions = audit_actions(&pool, &report.run_id);
    assert_action_present(&actions, "agent.run.guardrail");

    // 并行子任务默认关闭：单轮多步骤串行执行并留痕（低并发纪律）。
    struct TwoStepPlanner;
    impl Planner for TwoStepPlanner {
        fn plan(&self, ctx: &PlanContext<'_>) -> Decision {
            if ctx.history.is_empty() {
                Decision::Steps(vec![
                    PlanStep {
                        tool_id: "rip.kb.keyword_search".to_string(),
                        version: None,
                        input: json!({"keyword": "first"}),
                        rationale: "串行第一步".to_string(),
                    },
                    PlanStep {
                        tool_id: "rip.kb.keyword_search".to_string(),
                        version: None,
                        input: json!({"keyword": "second"}),
                        rationale: "串行第二步".to_string(),
                    },
                ])
            } else {
                Decision::Finish {
                    reason: "两步完成".to_string(),
                }
            }
        }
    }
    let driver3 = CountingDriver::succeed(mock_payload(), "ok");
    let (loop3, store3, _s3) = build_loop(&pool, driver3, RunControlConfig::default(), 8);
    let definition3 = AgentDefinition::try_new(
        AgentId::try_new("agent.p74.parallel").unwrap(),
        "0.1.0".to_string(),
        "{}".to_string(),
        vec![CapabilityRef::try_new("rip.kb.keyword_search", None).unwrap()],
    )
    .unwrap();
    store3
        .register_definition(&definition3, "并行纪律测试 Agent")
        .unwrap();
    let mut req3 = run_request(
        "agent.p74.parallel",
        Box::new(TwoStepPlanner),
        "zzzq 并行纪律词串",
    );
    let report3 = loop3.run(&mut req3).unwrap();
    assert_eq!(report3.state, "SUCCEEDED", "report3: {report3:?}");
    assert_eq!(report3.steps.len(), 2, "两步都执行（串行）");
    assert!(
        report3
            .warnings
            .iter()
            .any(|w| w.contains("parallel subtasks disabled") && w.contains("serialized")),
        "warnings: {:?}",
        report3.warnings
    );
    // 步序保持规划顺序（串行面，轨迹 seq 断言）。
    let traj = _s3.steps(&report3.run_id).unwrap();
    assert_eq!(traj[0].seq, 0);
    assert_eq!(traj[1].seq, 1);
    assert_eq!(traj[0].round, traj[1].round, "同轮两步串行");
}

// ---------------------------------------------------------------------------
// 对照：KeywordSearchPlanner 常规路径在 P7-4 控制面下行为不变（P7-2 回归）
// ---------------------------------------------------------------------------

#[test]
fn default_planner_single_tool_loop_unchanged_under_control_plane() {
    let pool = migrated_pool();
    register_tool(&pool, "rip.kb.keyword_search");
    let driver = CountingDriver::succeed(mock_payload(), "ok");
    let (loop_, store, _state) = build_loop(&pool, driver, RunControlConfig::default(), 4);
    let definition = AgentDefinition::try_new(
        AgentId::try_new("agent.p74.regress").unwrap(),
        "0.1.0".to_string(),
        "{}".to_string(),
        vec![CapabilityRef::try_new("rip.kb.keyword_search", None).unwrap()],
    )
    .unwrap();
    store
        .register_definition(&definition, "回归对照 Agent")
        .unwrap();
    let mut req = run_request(
        "agent.p74.regress",
        Box::new(KeywordSearchPlanner),
        "检索 nmap 端口扫描相关知识并调用登记工具",
    );
    let report = loop_.run(&mut req).unwrap();
    assert_eq!(report.state, "SUCCEEDED", "report: {report:?}");
    assert_eq!(report.steps.len(), 1);
    assert_eq!(report.resumed_steps, 0);
    assert_eq!(
        store.run(&report.run_id).unwrap().unwrap().run.state(),
        AgentRunState::Succeeded
    );
}
