//! MA-2 并发派发引擎三硬验收（docs/rip3/MA-2-TASK.md §4 判据 2/3/4 + 判据
//! 6 caller 落库与继承 + 判据 5 legacy 守卫）：
//!
//! 1. 判据 2 硬验收①：单 caller N=3 并行提速实测——三步各 ~300ms 模拟驱
//!    动的同 caller run，并行帽=3 配置 vs 串行基线：并行总耗时 < 串行 ×
//!    0.7，且驱动侧并发观测峰值 ≥ 2（真实计时，禁模拟冒充）。
//! 2. 判据 3 硬验收②：超预算硬拒三点——速率帽窗口超限（kind=
//!    budget_caller_rate）/ 总预算步数到位（kind=budget_caller_total）/
//!    受理面 max_concurrent_runs 超限（BudgetDenied，run 不落行）。
//! 3. 判据 4 硬验收③：双 caller 互不抢占——a 满载时 b 受理/执行不受影响，
//!    a 第三 run BudgetDenied 而 b 同刻受理成功。
//! 4. 判据 6：agent_run.caller_id / tool_run.caller_id 落列 + retry 继承。
//! 5. 判据 5 守卫：allow_parallel_subtasks=true 而 caller 面不齐仍串行
//!    （红线 1 放行三与条件）。
//!
//! 全程内存库 + 真实 migrations（001..027）+ 仓库登记 Manifest + 注入驱动
//! （RunDriver 端口，与 run_control_p7_4.rs 同款 harness 形态）。同测试内
//! 多 run 共享同一 Pool（内存库跨线程克隆共享，受理记账同库可观测）。

#![deny(missing_docs)]

use agent_engine::agent_loop::{
    AgentLoop, Decision, LoopConfig, PlanContext, PlanStep, Planner, RunRequest,
};
use agent_engine::broker_link::{BrokerLink, FirstFamilyConfirmer};
use agent_engine::control::{CallerBudget, RunControlConfig};
use agent_engine::evidence::EvidenceRegistrar;
use agent_engine::store::AgentStore;
use agent_engine::tool_link::{RunDriver, ToolLink};
use context_broker::{ArtifactSource, Broker, ManifestStore, RegistrySource, RulepackSource};
use database::{DbPath, Pool, PoolConfig};
use domain::{AgentDefinition, AgentId, AgentRunState, CapabilityRef};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tool_engine::mock_runner::MockRunner;
use tool_engine::Engine;
use tool_registry::Registry;

const AGENT_ID: &str = "agent.ma2.budget";
const TOOL_ID: &str = "rip.kb.keyword_search";

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

/// 登记仓库真实 Manifest 并置 ENABLED + MA-2 测试 Agent 定义（一次性载具）。
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
        .register_definition(&definition, "MA-2 预算面测试 Agent")
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

/// 计时驱动（判据 2 并发观测面）：每步真实 sleep `delay_ms` 再驱动状态机，
/// 记录并发在飞峰值（peak ≥ 2 即并行支路真实并发）。可 Clone，测试侧留
/// 句柄读计数。
#[derive(Clone)]
struct TimedDriver {
    delay_ms: u64,
    runner: MockRunner,
    active: Arc<AtomicUsize>,
    peak: Arc<AtomicUsize>,
    calls: Arc<AtomicUsize>,
}

impl TimedDriver {
    fn timed(delay_ms: u64) -> Self {
        Self {
            delay_ms,
            runner: MockRunner::succeed(
                json!({"keyword": "mock", "per_group_limit": 5, "total_hits": 0, "groups": []}),
                "mock 检索完成",
            ),
            active: Arc::new(AtomicUsize::new(0)),
            peak: Arc::new(AtomicUsize::new(0)),
            calls: Arc::new(AtomicUsize::new(0)),
        }
    }
    fn peak(&self) -> usize {
        self.peak.load(Ordering::SeqCst)
    }
}

impl RunDriver for TimedDriver {
    fn drive(
        &self,
        engine: &Engine,
        run_id: &str,
    ) -> Result<tool_engine::DriveOutcome, tool_engine::EngineError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let now_active = self.active.fetch_add(1, Ordering::SeqCst) + 1;
        self.peak.fetch_max(now_active, Ordering::SeqCst);
        std::thread::sleep(Duration::from_millis(self.delay_ms));
        let out = engine.drive(run_id, &self.runner);
        self.active.fetch_sub(1, Ordering::SeqCst);
        out
    }
}

/// caller 预算面装配元组：(caller, 并行帽, 跨 run 帽, 窗口 ms, 速率帽, 总步帽)。
type BudgetTuple = (&'static str, usize, usize, u64, usize, usize);

fn budgets_map(budgets: &[BudgetTuple]) -> HashMap<String, CallerBudget> {
    budgets
        .iter()
        .map(|(caller, parallel, concurrent, window, rate, total)| {
            (
                caller.to_string(),
                CallerBudget {
                    caller_id: caller.to_string(),
                    max_parallel_subtasks: *parallel,
                    max_concurrent_runs: *concurrent,
                    rate_window_ms: *window,
                    rate_max_steps: *rate,
                    budget_max_steps_per_run: *total,
                },
            )
        })
        .collect()
}

/// 在共享 pool 上装配 AgentLoop（D7 分层纪律：budget 内存映射由测试构造，
/// 与 rp-agent 启动时 load_caller_budgets 读表同口径）。返回驱动句柄供
/// 并发峰值观测。AgentLoop 不跨线程移动——线程内各自装配（Pool 共享）。
fn build_loop_on(
    pool: &Pool,
    allow_parallel: bool,
    budgets: &[BudgetTuple],
    delay_ms: u64,
) -> (AgentLoop, TimedDriver) {
    let driver = TimedDriver::timed(delay_ms);
    let loop_ = AgentLoop::new(
        AgentStore::open(pool.clone(), Registry::open(pool.clone())),
        build_broker_link(pool),
        ToolLink::new(
            Engine::open(pool.clone(), Registry::open(pool.clone())),
            Box::new(driver.clone()),
        ),
        EvidenceRegistrar::new(pool.clone()),
        Some(audit_engine::AuditEngine::new(pool.clone())),
        LoopConfig {
            workspace_id: "ws-default".to_string(),
            max_tool_calls: 8,
        },
    )
    .with_control(RunControlConfig {
        allow_parallel_subtasks: allow_parallel,
        caller_budgets: budgets_map(budgets),
        ..RunControlConfig::default()
    });
    (loop_, driver)
}

fn make_request(caller_id: Option<&str>, planner: Box<dyn Planner>) -> RunRequest {
    RunRequest {
        agent_id: AGENT_ID.to_string(),
        agent_version: None,
        goal: "MA-2 预算面验收词串".to_string(),
        task_id: Some("task-ma2".to_string()),
        trace_id: None,
        run_id: None,
        input: None,
        caller_id: caller_id.map(str::to_string),
        planner,
        confirmer: Box::new(FirstFamilyConfirmer::new(
            context_broker::IntentConfig::from_dir(
                &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../rulepacks/rip-rules"),
            )
            .unwrap(),
        )),
    }
}

/// 顺序规划器：逐轮吐出预置计划（每步独立 keyword 防重复检测跳步）。
struct SeqPlanPlanner {
    plans: Vec<Vec<String>>,
    counter: AtomicUsize,
}

impl SeqPlanPlanner {
    fn keywords_per_round(rounds: usize, steps: usize) -> Self {
        let plans = (0..rounds)
            .map(|r| {
                (0..steps)
                    .map(|s| format!("kw-{r}-{s}"))
                    .collect::<Vec<_>>()
            })
            .collect();
        Self {
            plans,
            counter: AtomicUsize::new(0),
        }
    }

    /// 次轮步数覆盖（判据 3 用：首轮帽内执行、次轮新规划步触发硬停）。
    fn with_second_round(mut self, steps: usize) -> Self {
        self.plans[1] = (0..steps).map(|s| format!("kw-1-over-{s}")).collect();
        self
    }
}

impl Planner for SeqPlanPlanner {
    fn plan(&self, _ctx: &PlanContext<'_>) -> Decision {
        let idx = self.counter.fetch_add(1, Ordering::SeqCst);
        match self.plans.get(idx) {
            Some(steps) => Decision::Steps(
                steps
                    .iter()
                    .map(|kw| PlanStep {
                        tool_id: TOOL_ID.to_string(),
                        version: None,
                        input: json!({"keyword": kw, "per_group_limit": 5}),
                        rationale: "MA-2 并行支路规划".to_string(),
                    })
                    .collect(),
            ),
            None => Decision::Finish {
                reason: "计划用尽（MA-2 规划器收尾）".to_string(),
            },
        }
    }
}

/// run 完成后 caller 在飞行数（agent_run WHERE caller_id AND state IN
/// (QUEUED,RUNNING)，D1 记账公式）。
fn active_runs(pool: &Pool, caller: &str) -> usize {
    pool.with_conn(|c| {
        c.query_row(
            "SELECT COUNT(*) FROM agent_run WHERE caller_id = ?1 AND state IN ('QUEUED','RUNNING')",
            [caller],
            |r| r.get::<_, i64>(0),
        )
    })
    .unwrap() as usize
}

/// 该 caller 名下 tool_run 行数（速率帽窗口计数数据源，D5.2）。
fn tool_run_count(pool: &Pool, caller: &str) -> usize {
    pool.with_conn(|c| {
        c.query_row(
            "SELECT COUNT(*) FROM tool_run WHERE caller_id = ?1",
            [caller],
            |r| r.get::<_, i64>(0),
        )
    })
    .unwrap() as usize
}

/// run 审计事件 (action, detail) 列表。
fn audit_events(pool: &Pool, run_id: &str) -> Vec<(String, Option<String>)> {
    let audit = audit_engine::AuditEngine::new(pool.clone());
    audit
        .trace(run_id)
        .unwrap()
        .into_iter()
        .map(|s| (s.event.action, s.event.detail))
        .collect()
}

fn detail_kinds(events: &[(String, Option<String>)], action: &str) -> Vec<Value> {
    events
        .iter()
        .filter(|(a, _)| a == action)
        .filter_map(|(_, d)| d.as_ref().and_then(|d| serde_json::from_str::<Value>(d).ok()))
        .collect()
}

/// 轮询等待 caller 在飞 run 数达预期（受理面/执行态观测，判据 4 时延面）。
fn wait_active_runs(pool: &Pool, caller: &str, want: usize, timeout: Duration) -> bool {
    let started = Instant::now();
    while started.elapsed() < timeout {
        if active_runs(pool, caller) >= want {
            return true;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    active_runs(pool, caller) >= want
}

// ---------------------------------------------------------------------------
// 判据 2 硬验收①：单 caller N=3 并行提速实测
// ---------------------------------------------------------------------------

#[test]
fn hard_acceptance_parallel_speedup_n3_peak_ge2_under_serial_x070() {
    let pool = migrated_pool();
    register_fixture(&pool);
    // 串行基线：三步各 ~300ms（allow=true 但 caller 无 budget 行 = 缺行
    // 口径并行关），串行执行。
    let (loop_serial, driver_serial) = build_loop_on(&pool, true, &[], 300);
    let req = make_request(
        Some("caller-speed"),
        Box::new(SeqPlanPlanner::keywords_per_round(1, 3)),
    );
    let mut req = req;
    let serial_started = Instant::now();
    let serial_report = loop_serial.run(&mut req).unwrap();
    let serial_ms = serial_started.elapsed().as_millis() as u64;
    assert_eq!(serial_report.state, "SUCCEEDED");
    assert_eq!(serial_report.steps.len(), 3, "串行基线三步全执行");
    assert_eq!(driver_serial.peak(), 1, "serial execution must never exceed 1 in flight");
    // 串行基线留痕：并行关（缺 budget 行）→ 既有 warning。
    assert!(
        serial_report
            .warnings
            .iter()
            .any(|w| w.contains("parallel subtasks disabled")),
        "serial warnings: {:?}",
        serial_report.warnings
    );

    // 并行配置：caller 预算面齐备（max_parallel_subtasks=3）。
    let (loop_parallel, driver_parallel) =
        build_loop_on(&pool, true, &[("caller-speed", 3, 0, 60_000, 0, 0)], 300);
    let mut req2 = make_request(
        Some("caller-speed"),
        Box::new(SeqPlanPlanner::keywords_per_round(1, 3)),
    );
    let parallel_started = Instant::now();
    let parallel_report = loop_parallel.run(&mut req2).unwrap();
    let parallel_ms = parallel_started.elapsed().as_millis() as u64;
    assert_eq!(parallel_report.state, "SUCCEEDED");
    assert_eq!(parallel_report.steps.len(), 3, "并行三步全执行");

    eprintln!(
        "MA-2 判据2 计时：串行={}ms 并行={}ms 比值={:.3}（并行帽=3，步延时 300ms）",
        serial_ms,
        parallel_ms,
        parallel_ms as f64 / serial_ms as f64
    );
    assert!(
        parallel_ms * 10 < serial_ms * 7,
        "hard acceptance: parallel {parallel_ms}ms must be < serial*0.7 ({}ms)",
        serial_ms
    );
    assert!(
        driver_parallel.peak() >= 2,
        "hard acceptance: driver-side concurrency peak {} must be >= 2",
        driver_parallel.peak()
    );
    // 步记录按 plan 顺序落（seq 确定性）。
    for step in &parallel_report.steps {
        assert_eq!(step.tool_id, TOOL_ID);
    }
    // 并行 run 的 R5 预估留痕（D8）。
    assert!(
        parallel_report
            .warnings
            .iter()
            .any(|w| w.contains("parallel fan-out plan")),
        "parallel warnings: {:?}",
        parallel_report.warnings
    );

    // 并行支路 caller 落库（判据 6 部分）：agent_run.caller_id +
    // tool_run.caller_id 全部落列（027）。
    let stored_caller: Option<String> = pool
        .with_conn(|c| {
            c.query_row(
                "SELECT caller_id FROM agent_run WHERE run_id = ?1",
                [&parallel_report.run_id],
                |r| r.get(0),
            )
        })
        .unwrap();
    assert_eq!(stored_caller.as_deref(), Some("caller-speed"));
    let attributed: i64 = pool
        .with_conn(|c| {
            c.query_row(
                "SELECT COUNT(*) FROM tool_run WHERE task_id = 'task-ma2'
                 AND caller_id = 'caller-speed'",
                [],
                |r| r.get(0),
            )
        })
        .unwrap();
    assert!(
        attributed >= 6,
        "both runs' steps must carry tool_run.caller_id: {attributed}"
    );
}

// ---------------------------------------------------------------------------
// 判据 3 硬验收②：超预算硬拒三点
// ---------------------------------------------------------------------------

/// 速率帽窗口超限 → 整轮硬停 + GUARDRAIL kind=budget_caller_rate，
/// 超帽步骤不执行（真实窗口计数 = tool_run.caller_id 行 COUNT）。
#[test]
fn hard_acceptance_rate_cap_hard_stop_budget_caller_rate() {
    let pool = migrated_pool();
    register_fixture(&pool);
    let (loop_, _driver) =
        build_loop_on(&pool, true, &[("caller-rate", 3, 0, 60_000, 2, 0)], 0);
    // 首轮 2 步（帽内执行），次轮再规划 1 步即被窗口计数硬停。
    let mut req = make_request(
        Some("caller-rate"),
        Box::new(SeqPlanPlanner::keywords_per_round(2, 2).with_second_round(1)),
    );
    let report = loop_.run(&mut req).unwrap();
    eprintln!("MA-2 判据3-速率 finish: {}", report.finish_reason);
    assert!(
        report.finish_reason.contains("guardrail: caller rate budget"),
        "finish_reason: {}",
        report.finish_reason
    );
    assert_eq!(report.steps.len(), 2, "超帽步骤不执行：只有首轮 2 步落库");
    let events = audit_events(&pool, &report.run_id);
    let guardrails = detail_kinds(&events, "agent.run.guardrail");
    assert!(
        guardrails
            .iter()
            .any(|d| d.get("kind").and_then(Value::as_str) == Some("budget_caller_rate")),
        "guardrail events: {guardrails:?}"
    );
    assert_eq!(tool_run_count(&pool, "caller-rate"), 2);
}

/// 总预算步数到位 → GUARDRAIL kind=budget_caller_total。
#[test]
fn hard_acceptance_total_budget_hard_stop_budget_caller_total() {
    let pool = migrated_pool();
    register_fixture(&pool);
    let (loop_, _driver) =
        build_loop_on(&pool, true, &[("caller-total", 3, 0, 60_000, 0, 2)], 0);
    let mut req = make_request(
        Some("caller-total"),
        Box::new(SeqPlanPlanner::keywords_per_round(2, 2).with_second_round(1)),
    );
    let report = loop_.run(&mut req).unwrap();
    eprintln!("MA-2 判据3-总量 finish: {}", report.finish_reason);
    assert!(
        report
            .finish_reason
            .contains("guardrail: caller total budget"),
        "finish_reason: {}",
        report.finish_reason
    );
    assert_eq!(report.steps.len(), 2, "帽到位即停：2 步执行后新规划步不执行");
    let events = audit_events(&pool, &report.run_id);
    let guardrails = detail_kinds(&events, "agent.run.guardrail");
    assert!(
        guardrails
            .iter()
            .any(|d| d.get("kind").and_then(Value::as_str) == Some("budget_caller_total")),
        "guardrail events: {guardrails:?}"
    );
}

/// 受理面 max_concurrent_runs 超限 → BudgetDenied 硬拒（run 不落行）。
#[test]
fn hard_acceptance_admission_cap_budget_denied_no_row() {
    let pool = migrated_pool();
    register_fixture(&pool);
    // run A 在后台线程占满并发帽（3 步 × 300ms 真实驱动，共享同库）。
    let pool_a = pool.clone();
    let runner_handle = std::thread::spawn(move || {
        let (loop_a, _d) = build_loop_on(&pool_a, true, &[("caller-adm", 2, 1, 60_000, 0, 0)], 300);
        let mut req = make_request(
            Some("caller-adm"),
            Box::new(SeqPlanPlanner::keywords_per_round(1, 3)),
        );
        loop_a.run(&mut req).unwrap().run_id
    });
    // 等待 run A 真正 RUNNING（在飞记账 = 1）。
    assert!(
        wait_active_runs(&pool, "caller-adm", 1, Duration::from_secs(10)),
        "run A must be RUNNING before admission attempt"
    );
    // 同 caller 第二 run：BudgetDenied 硬拒（create_run 前，run 不落行）。
    let (loop_b, _d2) = build_loop_on(&pool, true, &[("caller-adm", 2, 1, 60_000, 0, 0)], 0);
    let mut req_b = make_request(
        Some("caller-adm"),
        Box::new(SeqPlanPlanner::keywords_per_round(1, 1)),
    );
    let err = loop_b.run(&mut req_b).unwrap_err();
    eprintln!("MA-2 判据3-受理 rejection: {err}");
    assert!(
        err.to_string().contains("budget denied"),
        "err: {err}"
    );
    // run 不落行：在飞仍只有 run A 一个；B 未产生任何 agent_run/tool_run 行。
    assert_eq!(active_runs(&pool, "caller-adm"), 1);
    let agent_run_rows: i64 = pool
        .with_conn(|c| {
            c.query_row(
                "SELECT COUNT(*) FROM agent_run WHERE caller_id = 'caller-adm'",
                [],
                |r| r.get(0),
            )
        })
        .unwrap();
    assert_eq!(agent_run_rows, 1, "denied run must not persist any row");
    let _ = runner_handle.join().unwrap();
}

// ---------------------------------------------------------------------------
// 判据 4 硬验收③：双 caller 互不抢占
// ---------------------------------------------------------------------------

#[test]
fn hard_acceptance_dual_caller_isolation_admission_latency() {
    let pool = migrated_pool();
    register_fixture(&pool);
    let budgets: [BudgetTuple; 2] = [
        ("caller-iso-a", 2, 2, 60_000, 0, 0),
        ("caller-iso-b", 2, 2, 60_000, 0, 0),
    ];
    // a 双 run 后台并发占满并发帽（各 3 步 × 300ms，共享同库）。
    let handles: Vec<_> = (0..2)
        .map(|_| {
            let pool = pool.clone();
            std::thread::spawn(move || {
                let (loop_a, _d) = build_loop_on(&pool, true, &budgets, 300);
                let mut req = make_request(
                    Some("caller-iso-a"),
                    Box::new(SeqPlanPlanner::keywords_per_round(1, 3)),
                );
                loop_a.run(&mut req).unwrap().run_id
            })
        })
        .collect();
    assert!(
        wait_active_runs(&pool, "caller-iso-a", 2, Duration::from_secs(10)),
        "caller-a must be at cap (2 RUNNING runs)"
    );
    // b 受理/执行不受影响（时延粗断言：b run 全程远快于 a 剩余执行时长，
    // 且 b 受理即刻成功——在 a 满载窗口内完成受理与执行）。
    let b_started = Instant::now();
    {
        let (loop_b, _d) = build_loop_on(&pool, true, &budgets, 0);
        let mut req = make_request(
            Some("caller-iso-b"),
            Box::new(SeqPlanPlanner::keywords_per_round(1, 1)),
        );
        let report = loop_b.run(&mut req).unwrap();
        let b_ms = b_started.elapsed().as_millis();
        eprintln!(
            "MA-2 判据4 b-run 全程（a 满载窗口内）：{b_ms}ms state={}",
            report.state
        );
        assert_eq!(
            report.state, "SUCCEEDED",
            "b run must complete while a saturated"
        );
        assert!(
            active_runs(&pool, "caller-iso-a") >= 1,
            "caller-a runs must still be in flight (isolation observed live)"
        );
    }
    // a 第三 run BudgetDenied 而 b 第二 run 同刻受理成功。若 a 双 run 已
    // 收敛，先补位一轮保证满载窗口。
    if active_runs(&pool, "caller-iso-a") < 2 {
        let pool2 = pool.clone();
        let _refill = std::thread::spawn(move || {
            let (loop_a, _d) = build_loop_on(&pool2, true, &budgets, 300);
            let mut req = make_request(
                Some("caller-iso-a"),
                Box::new(SeqPlanPlanner::keywords_per_round(1, 3)),
            );
            loop_a.run(&mut req).unwrap().run_id
        });
        assert!(
            wait_active_runs(&pool, "caller-iso-a", 1, Duration::from_secs(10)),
            "caller-a refill run must be RUNNING"
        );
    }
    let (loop_a3, _d3) = build_loop_on(&pool, true, &budgets, 0);
    let mut req_a3 = make_request(
        Some("caller-iso-a"),
        Box::new(SeqPlanPlanner::keywords_per_round(1, 1)),
    );
    let err = loop_a3.run(&mut req_a3).unwrap_err();
    eprintln!("MA-2 判据4 a 第三 run rejection: {err}");
    assert!(
        err.to_string().contains("budget denied"),
        "a third run must be BudgetDenied: {err}"
    );
    // b 同刻第二 run 受理成功（a 满载不影响 b 记账）。
    let (loop_b2, _d4) = build_loop_on(&pool, true, &budgets, 0);
    let mut req_b2 = make_request(
        Some("caller-iso-b"),
        Box::new(SeqPlanPlanner::keywords_per_round(1, 1)),
    );
    let report_b2 = loop_b2.run(&mut req_b2).unwrap();
    assert_eq!(report_b2.state, "SUCCEEDED");
    for h in handles {
        let _ = h.join().unwrap();
    }
}

/// D5.①并发帽排队（帽语义非拒绝）：帽以上步骤排队不并行，GUARDRAIL
/// kind=budget_caller_parallel 留痕，全部步骤仍执行完毕（不硬拒）。
#[test]
fn parallel_cap_queues_tasks_without_rejection_and_audits_budget_caller_parallel() {
    let pool = migrated_pool();
    register_fixture(&pool);
    let (loop_, _driver) =
        build_loop_on(&pool, true, &[("caller-queue", 2, 0, 60_000, 0, 0)], 0);
    let mut req = make_request(
        Some("caller-queue"),
        Box::new(SeqPlanPlanner::keywords_per_round(1, 3)),
    );
    let report = loop_.run(&mut req).unwrap();
    assert_eq!(report.state, "SUCCEEDED");
    assert_eq!(report.steps.len(), 3, "帽以上排队不拒绝：三步全部执行");
    let events = audit_events(&pool, &report.run_id);
    let guardrails = detail_kinds(&events, "agent.run.guardrail");
    let parallel_audit = guardrails
        .iter()
        .find(|d| d.get("kind").and_then(Value::as_str) == Some("budget_caller_parallel"))
        .expect("queueing must emit budget_caller_parallel guardrail");
    assert_eq!(
        parallel_audit.get("tasks").and_then(Value::as_i64),
        Some(3),
        "audit payload carries real task count: {parallel_audit}"
    );
    assert_eq!(parallel_audit.get("cap").and_then(Value::as_i64), Some(2));
    assert_eq!(parallel_audit.get("queued").and_then(Value::as_i64), Some(1));
}

// ---------------------------------------------------------------------------
// 判据 6：retry 继承 caller + 判据 5 守卫：caller 面不齐仍串行
// ---------------------------------------------------------------------------

/// 失败驱动（fail-fast 后重试链继承面）。
#[derive(Clone)]
struct FailingDriver;

impl RunDriver for FailingDriver {
    fn drive(
        &self,
        engine: &Engine,
        run_id: &str,
    ) -> Result<tool_engine::DriveOutcome, tool_engine::EngineError> {
        engine.drive(
            run_id,
            &MockRunner::fail(
                "EXECUTION",
                json!({"code": -32000, "message": "mock failure"}),
                "mock 注入失败",
            ),
        )
    }
}

#[test]
fn retry_chain_inherits_caller_id_from_source_run() {
    let pool = migrated_pool();
    register_fixture(&pool);
    let store = AgentStore::open(pool.clone(), Registry::open(pool.clone()));
    let loop_ = AgentLoop::new(
        store.clone(),
        build_broker_link(&pool),
        ToolLink::new(
            Engine::open(pool.clone(), Registry::open(pool.clone())),
            Box::new(FailingDriver),
        ),
        EvidenceRegistrar::new(pool.clone()),
        Some(audit_engine::AuditEngine::new(pool.clone())),
        LoopConfig::default(),
    );
    let mut req = RunRequest {
        agent_id: AGENT_ID.to_string(),
        agent_version: None,
        goal: "重试继承词串".to_string(),
        task_id: None,
        trace_id: Some("tr-ma2-retry".to_string()),
        run_id: None,
        input: None,
        caller_id: Some("caller-retry".to_string()),
        planner: Box::new(SeqPlanPlanner::keywords_per_round(1, 1)),
        confirmer: Box::new(FirstFamilyConfirmer::new(
            context_broker::IntentConfig::from_dir(
                &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../rulepacks/rip-rules"),
            )
            .unwrap(),
        )),
    };
    let report = loop_.run(&mut req).unwrap();
    assert_eq!(report.state, "FAILED");
    let stored = store.run(&report.run_id).unwrap().unwrap();
    assert_eq!(stored.caller_id.as_deref(), Some("caller-retry"));
    assert_eq!(stored.trace_id.as_deref(), Some("tr-ma2-retry"));
    // tool_run.caller_id 亦落列（027）。
    let step_caller: Option<String> = pool
        .with_conn(|c| {
            c.query_row(
                "SELECT caller_id FROM tool_run WHERE run_id = ?1",
                [&report.steps[0].tool_run_id],
                |r| r.get(0),
            )
        })
        .unwrap();
    assert_eq!(step_caller.as_deref(), Some("caller-retry"));
    // retry：新 run 继承源 run caller（D4③）+ trace 继承不回归。
    let control = agent_engine::control::RunControl::new(
        store.clone(),
        agent_engine::state::RunStateStore::new(pool.clone()),
    );
    let retry = control.retry(&report.run_id).unwrap();
    let replayed = store.run(&retry.new_run_id).unwrap().unwrap();
    assert_eq!(
        replayed.caller_id.as_deref(),
        Some("caller-retry"),
        "retry must inherit caller"
    );
    assert_eq!(
        replayed.trace_id.as_deref(),
        Some("tr-ma2-retry"),
        "trace inheritance unchanged"
    );
    assert_eq!(replayed.run.state(), AgentRunState::Queued);
}

/// 红线 1：allow_parallel_subtasks=true 而 caller_id=None → 仍串行 +
/// 既有 warning（放行三与条件缺一即串行）。
#[test]
fn parallel_flag_without_caller_face_stays_serial() {
    let pool = migrated_pool();
    register_fixture(&pool);
    let (loop_, driver) = build_loop_on(&pool, true, &[], 0);
    let mut req = make_request(None, Box::new(SeqPlanPlanner::keywords_per_round(1, 2)));
    let report = loop_.run(&mut req).unwrap();
    assert_eq!(report.state, "SUCCEEDED");
    assert!(
        report
            .warnings
            .iter()
            .any(|w| w.contains("parallel subtasks disabled")),
        "legacy caller_id=None must stay serial with the existing warning: {:?}",
        report.warnings
    );
    assert_eq!(driver.peak(), 1, "legacy path must never run steps concurrently");
    // 无 caller：tool_run.caller_id 全 NULL（零回归）。
    let null_callers: i64 = pool
        .with_conn(|c| {
            c.query_row(
                "SELECT COUNT(*) FROM tool_run WHERE task_id = 'task-ma2' AND caller_id IS NULL",
                [],
                |r| r.get(0),
            )
        })
        .unwrap();
    assert!(null_callers >= 2, "legacy steps must keep caller_id NULL");
}
