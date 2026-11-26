//! CB-1 B1/B3 e2e 硬验收（docs/rip3/CB-1-TASK.md §4 判据 1-5）：
//!
//! 1. 判据 1 长跑折叠实测：N 轮模拟驱动 run（--transcript-fold 等价配置，
//!    阈值/间隔/热窗收紧），触发折叠 ≥2 次；折叠后规划器可见 history 长度
//!    < 全量步数（合成折叠行替代）；RunReport.steps 全量不缩水；
//!    agent_run_step 行数 = 全量步数（权威面零重写实测）。折叠前/后
//!    planner 可见 est_tokens 数字 println 供批次报告（禁模拟冒充）。
//! 2. 判据 2 折叠确定性：fresh 折叠 digest vs resume 重算 digest 逐字节
//!    相等；持久化折叠行被污染时 resume 漂移检测 warning 留痕（负例）。
//! 3. 判据 4 两级触发：阈值未到不折叠（负例）；阈值到而轮距未到不折叠
//!    （防抖负例）；热窗覆盖全转录不折叠（负例）；折叠开启下既有硬护栏
//!    （max_rounds / max_tool_calls）行为不变——预算数真实步数不因折叠放宽。
//! 4. 判据 3 回查面：TaskMemoryStore::list_by_run 可读折叠行；compress_run
//!    后 summary 词形留存（013 生命周期语义兼容实测）。
//! 5. 判据 5 默认关守卫：FoldConfig 默认关 e2e 路径 fold_events 空 + 无
//!    折叠行落库 + steps 全量。
//!
//! 全程内存库 + 真实 migrations（001..027）+ 仓库登记 Manifest + 注入驱动
//! （RunDriver 端口，ma2_parallel_budget.rs / run_control_p7_4.rs 同款
//! harness 形态）。

#![deny(missing_docs)]

use agent_engine::agent_loop::{
    AgentLoop, Decision, FoldEvent, LoopConfig, PlanContext, PlanStep, Planner, ResumeRequest,
    RunRequest, FOLD_SUMMARY_PREFIX, FOLD_TOOL_ID,
};
use agent_engine::broker_link::{BrokerLink, FirstFamilyConfirmer};
use agent_engine::control::{FoldConfig, RunControlConfig};
use agent_engine::evidence::EvidenceRegistrar;
use agent_engine::memory::TaskMemoryStore;
use agent_engine::state::RunStateStore;
use agent_engine::store::AgentStore;
use agent_engine::tool_link::{RunDriver, ToolLink};
use context_broker::{ArtifactSource, Broker, ManifestStore, RegistrySource, RulepackSource};
use database::{DbPath, Pool, PoolConfig};
use domain::{AgentDefinition, AgentId, CapabilityRef};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tool_engine::mock_runner::MockRunner;
use tool_engine::Engine;
use tool_registry::Registry;

const AGENT_ID: &str = "agent.cb1.fold";
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

/// 登记仓库真实 Manifest 并置 ENABLED + CB-1 测试 Agent 定义（一次性载具）。
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
        .register_definition(&definition, "CB-1 折叠验收 Agent")
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

/// 计数驱动（逐次唯一载荷成功信封——每步新 content_hash，避免停滞护栏
/// 截停长跑；实例间全局 salt 隔离，resume 段载荷不与 fresh 段撞哈希；
/// 统计真实执行步数——resume 不重跑判据面）。
#[derive(Clone)]
struct CountingDriver {
    salt: u64,
    calls: Arc<AtomicUsize>,
}

impl CountingDriver {
    fn succeed() -> Self {
        use std::sync::atomic::AtomicU64;
        static DRIVER_SEQ: AtomicU64 = AtomicU64::new(0);
        Self {
            salt: DRIVER_SEQ.fetch_add(1, Ordering::SeqCst),
            calls: Arc::new(AtomicUsize::new(0)),
        }
    }
    fn count(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}

impl RunDriver for CountingDriver {
    fn drive(
        &self,
        engine: &Engine,
        run_id: &str,
    ) -> Result<tool_engine::DriveOutcome, tool_engine::EngineError> {
        let n = self.calls.fetch_add(1, Ordering::SeqCst);
        let runner = MockRunner::succeed(
            json!({"keyword": format!("mock-cb1-{}-{n}", self.salt), "per_group_limit": 5,
                   "total_hits": n, "groups": []}),
            format!("mock 检索完成（salt {} 第 {n} 次驱动）", self.salt),
        );
        engine.drive(run_id, &runner)
    }
}

/// 规划器可见面观测记录（判据 1 数字面：轮次 / 可见 history 长度 / 可见
/// est_tokens——est 走 context_broker::est_tokens 单点，D6）。
type VisibleObs = Arc<Mutex<Vec<(usize, usize, u64)>>>;

/// 顺序规划器：内部计数推进轮次（不用 ctx.history.len()——折叠后可见长度
/// 不再等于全量步数，这正是判据 1 的观测面），每轮 N 步独立 keyword 防
/// 重复检测跳步；可选在指定轮次对控制面发起协作式暂停（判据 2 resume
/// 链路载体）。
struct FoldingSeqPlanner {
    rounds: usize,
    steps_per_round: usize,
    seq_offset: usize,
    counter: AtomicUsize,
    pause_at_round: Option<usize>,
    state: Option<RunStateStore>,
    recorder: Option<VisibleObs>,
}

impl FoldingSeqPlanner {
    fn plan(rounds: usize) -> Self {
        Self {
            rounds,
            steps_per_round: 1,
            seq_offset: 0,
            counter: AtomicUsize::new(0),
            pause_at_round: None,
            state: None,
            recorder: None,
        }
    }
    /// 关键词序号偏移（resume 段规划器与 fresh 段关键词空间隔离——防重复
    /// 调用检测把恢复段首步判重跳步）。
    fn with_seq_offset(mut self, offset: usize) -> Self {
        self.seq_offset = offset;
        self
    }
    /// 每轮多步（判据 1 观测面：折入段 ≥2 步时合成行 1:N 替代才收缩长度）。
    fn with_steps_per_round(mut self, n: usize) -> Self {
        self.steps_per_round = n;
        self
    }
    fn with_pause_at(mut self, round: usize, state: RunStateStore) -> Self {
        self.pause_at_round = Some(round);
        self.state = Some(state);
        self
    }
    fn with_recorder(mut self, recorder: VisibleObs) -> Self {
        self.recorder = Some(recorder);
        self
    }
}

impl Planner for FoldingSeqPlanner {
    fn plan(&self, ctx: &PlanContext<'_>) -> Decision {
        if let Some(rec) = &self.recorder {
            let canonical = serde_json::to_string(ctx.history).unwrap_or_default();
            rec.lock().expect("visible obs mutex").push((
                ctx.round,
                ctx.history.len(),
                context_broker::est_tokens(&canonical),
            ));
        }
        if self.counter.fetch_add(1, Ordering::SeqCst) >= self.rounds {
            return Decision::Finish {
                reason: "计划轮次用尽（CB-1 折叠验收规划器收尾）".to_string(),
            };
        }
        if let (Some(arm), Some(state)) = (self.pause_at_round, &self.state) {
            if ctx.round == arm {
                assert!(
                    state.request_pause(ctx.run_id).unwrap(),
                    "协作式暂停请求必须受理（RUNNING 期）"
                );
            }
        }
        let seq = self.counter.load(Ordering::SeqCst);
        let steps: Vec<PlanStep> = (0..self.steps_per_round)
            .map(|i| PlanStep {
                tool_id: TOOL_ID.to_string(),
                version: None,
                input: json!({"keyword": format!("kw-cb1-{}-{}", seq + self.seq_offset, i)}),
                rationale: "CB-1 折叠验收顺序规划（关键词独立防判重跳步）".to_string(),
            })
            .collect();
        Decision::Steps(steps)
    }
}

fn make_request(planner: Box<dyn Planner>) -> RunRequest {
    RunRequest {
        agent_id: AGENT_ID.to_string(),
        agent_version: None,
        goal: "CB-1 长跑折叠验收词串".to_string(),
        task_id: Some("task-cb1".to_string()),
        trace_id: None,
        run_id: None,
        input: None,
        caller_id: None,
        planner,
        confirmer: Box::new(FirstFamilyConfirmer::new(
            context_broker::IntentConfig::from_dir(
                &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../rulepacks/rip-rules"),
            )
            .unwrap(),
        )),
    }
}

/// 折叠配置快取（判据 1 正例面：阈值 1 / 间隔 0 / 热窗 0 = 每轮界即触发）。
fn fold_on() -> FoldConfig {
    FoldConfig {
        enabled: true,
        fold_growth_tokens: 1,
        fold_min_interval_rounds: 0,
        fold_hot_window_rounds: 0,
    }
}

/// 在 pool 上装配 AgentLoop（折叠落库面挂接与否按参数）。
fn build_loop(
    pool: &Pool,
    driver: CountingDriver,
    control: RunControlConfig,
    max_tool_calls: usize,
) -> AgentLoop {
    AgentLoop::new(
        AgentStore::open(pool.clone(), Registry::open(pool.clone())),
        build_broker_link(pool),
        ToolLink::new(
            Engine::open(pool.clone(), Registry::open(pool.clone())),
            Box::new(driver),
        ),
        EvidenceRegistrar::new(pool.clone()),
        Some(audit_engine::AuditEngine::new(pool.clone())),
        LoopConfig {
            workspace_id: "ws-default".to_string(),
            max_tool_calls,
        },
    )
    .with_control(control)
    .with_fold_memory(TaskMemoryStore::new(pool.clone()))
}

/// 判据 1：长跑折叠实测——折叠 ≥2 次、规划器可见面收缩、权威面零重写。
#[test]
fn cb1_e2e_long_run_folds_planner_view_shrinks_authority_untouched() {
    let pool = migrated_pool();
    register_fixture(&pool);
    let driver = CountingDriver::succeed();
    let recorder: VisibleObs = Arc::new(Mutex::new(Vec::new()));
    // 每轮 2 步 × 3 轮 = 6 步：折入段 ≥2 步时合成行 1:N 替代才收缩可见长度。
    let planner = FoldingSeqPlanner::plan(3)
        .with_steps_per_round(2)
        .with_recorder(Arc::clone(&recorder));
    let loop_ = build_loop(
        &pool,
        driver.clone(),
        RunControlConfig {
            fold: fold_on(),
            ..RunControlConfig::default()
        },
        8,
    );
    let mut req = make_request(Box::new(planner));
    let report = loop_.run(&mut req).expect("run");

    // 折叠 ≥2 次（6 轮 × 1 步：轮 1..5 边界各折一段，共 5 次）。
    assert!(
        report.fold_events.len() >= 2,
        "长跑折叠触发 ≥2 次（实际 {}）",
        report.fold_events.len()
    );
    // RunReport.steps 全量不缩水（红线 5）。
    assert_eq!(report.steps.len(), 6, "报告 steps 全量");
    assert_eq!(driver.count(), 6, "真实执行 6 步");
    // agent_run_step 行数 = 全量步数（014 权威面零重写零回填零截断）。
    let persisted: i64 = pool
        .with_conn(|c| {
            c.query_row(
                "SELECT COUNT(*) FROM agent_run_step WHERE run_id = ?1",
                [&report.run_id],
                |r| r.get(0),
            )
        })
        .unwrap();
    assert_eq!(persisted, 6, "agent_run_step 行数 = 全量步数");
    // 折叠行只出现在 agent_task_memory（判据 10）。
    let fold_rows: i64 = pool
        .with_conn(|c| {
            c.query_row(
                "SELECT COUNT(*) FROM agent_task_memory WHERE run_id = ?1 AND summary LIKE 'transcript-fold:%'",
                [&report.run_id],
                |r| r.get(0),
            )
        })
        .unwrap();
    assert_eq!(
        fold_rows as usize,
        report.fold_events.len(),
        "折叠行逐事件落 013 记忆面"
    );

    // 规划器可见面收缩：折叠生效后的轮界，可见 history 长度 < 已执行全量步数。
    let obs = recorder.lock().unwrap().clone();
    assert!(
        obs.iter().any(|(round, visible, _)| {
            // 每轮 2 步：round r 规划时已执行 2*r 步；可见长度 < 全量即折叠
            // 合成行 1:N 替代实测。
            *visible < 2 * round
        }),
        "折叠后规划器可见 history 长度 < 全量步数"
    );
    // 且可见面含合成折叠行（tool_id = transcript:fold）。
    assert!(
        report
            .fold_events
            .iter()
            .all(|e| e.summary.starts_with(FOLD_SUMMARY_PREFIX) && e.digest.len() == 64),
        "折叠事件词形/digest 形状"
    );
    assert!(
        report.fold_events.iter().all(|e| e.memory_id.is_some()),
        "折叠行全部落库（with_fold_memory 挂接）"
    );

    // 判据 1 数字（批次报告输入，cargo test -- --nocapture 可读）。
    let final_round_obs = obs.last().expect("planner observations");
    let est_full_final: u64 = {
        let canonical = serde_json::to_string(&report.steps).unwrap();
        context_broker::est_tokens(&canonical)
    };
    println!(
        "CB1-判据1 数字：folds={} 全量步数=6 末轮规划可见长度={} 可见est={} 全量est={} \
         首折 est {}->{} 末折 est {}->{}",
        report.fold_events.len(),
        final_round_obs.1,
        final_round_obs.2,
        est_full_final,
        report.fold_events[0].est_before,
        report.fold_events[0].est_after,
        report.fold_events.last().unwrap().est_before,
        report.fold_events.last().unwrap().est_after,
    );
    // est 记账方向断言（折叠确有收缩；est_tokens 单点计量 D6）。
    for e in &report.fold_events {
        assert!(e.est_before > e.est_after, "折叠前 est > 折叠后 est");
        assert!(e.est_after > 0);
    }
}

/// 判据 2：fresh 折叠 digest vs resume 重算 digest 相等；漂移检测负例可触发。
#[test]
fn cb1_e2e_resume_recompute_digest_matches_fresh_and_drift_warns() {
    let pool = migrated_pool();
    register_fixture(&pool);
    let state = RunStateStore::new(pool.clone());
    let driver = CountingDriver::succeed();

    // fresh 段：4 轮计划，第 2 轮规划时发起协作暂停（轮 0/1 已执行并折叠）。
    let loop_ = build_loop(
        &pool,
        driver.clone(),
        RunControlConfig {
            fold: fold_on(),
            ..RunControlConfig::default()
        },
        8,
    );
    let mut req = make_request(Box::new(
        FoldingSeqPlanner::plan(6).with_pause_at(2, state.clone()),
    ));
    let paused_report = loop_.run(&mut req).expect("fresh segment");
    assert_eq!(paused_report.state, "PAUSED", "协作暂停中间态");
    let fresh_folds: Vec<FoldEvent> = paused_report.fold_events.clone();
    assert!(
        fresh_folds.iter().any(|e| e.rounds == [0, 0]),
        "fresh 段已折 rounds [0,0]"
    );

    // 污染持久化折叠行 digest（漂移检测负例注入面）：rounds 保留、digest
    // 换 deadbeef——resume 重算 digest ≠ 持久化 digest。
    {
        let task_memory = TaskMemoryStore::new(pool.clone());
        for row in task_memory.list_by_run(&paused_report.run_id).unwrap() {
            if !row.entry.summary().starts_with(FOLD_SUMMARY_PREFIX) {
                continue;
            }
            let mut payload: Value = serde_json::from_str(row.entry.content_json()).unwrap();
            payload["digest"] = json!("deadbeef");
            pool.with_conn(|c| {
                c.execute(
                    "UPDATE agent_task_memory SET content_json = ?1 WHERE memory_id = ?2",
                    database::rusqlite::params![payload.to_string(), row.entry.memory_id()],
                )
            })
            .unwrap();
        }
    }

    // resume：重算兜底 + 漂移检测 warning 留痕。
    let resumed_driver = CountingDriver::succeed();
    let loop2 = build_loop(
        &pool,
        resumed_driver.clone(),
        RunControlConfig {
            fold: fold_on(),
            ..RunControlConfig::default()
        },
        8,
    );
    let mut resume_req = ResumeRequest {
        run_id: paused_report.run_id.clone(),
        planner: Box::new(FoldingSeqPlanner::plan(4).with_seq_offset(10)),
        confirmer: Box::new(FirstFamilyConfirmer::new(
            context_broker::IntentConfig::from_dir(
                &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../rulepacks/rip-rules"),
            )
            .unwrap(),
        )),
    };
    let resumed_report = loop2.resume(&mut resume_req).expect("resume segment");
    // 漂移 warning 留痕（持久化 digest ≠ 重算 digest）。
    assert!(
        resumed_report
            .warnings
            .iter()
            .any(|w| w.contains("transcript fold drift detected")),
        "漂移检测 warning 留痕"
    );
    // 重算兜底：resume 事件列含与 fresh 相同轮区间的折叠，digest 与 fresh
    // 逐字节相等（重算 digest = fresh digest——同转录前缀同产物）。
    for fresh in fresh_folds.iter() {
        let recomputed = resumed_report
            .fold_events
            .iter()
            .find(|e| e.rounds == fresh.rounds)
            .unwrap_or_else(|| panic!("resume 重算覆盖轮区间 {:?}", fresh.rounds));
        assert_eq!(
            recomputed.digest, fresh.digest,
            "fresh vs resume 重算 digest 相等（判据 2；rounds {:?}）",
            fresh.rounds
        );
        assert_eq!(recomputed.summary, fresh.summary, "重算摘要相等");
    }
    // resume 段继续折叠（恢复后新轮界照常触发）。
    assert!(
        resumed_report.fold_events.iter().any(|e| e.rounds[0] >= 2),
        "resume 段新轮界继续折叠"
    );
    // resume 后权威面全量续接：已执行步不重跑（驱动计数 4 = 6 - 2 已折前段外步）。
    assert_eq!(resumed_driver.count(), 4, "resume 只执行轨迹之外步骤");
    // 权威面零重写：agent_run_step 行数 = resume 前后全量步数之和。
    let persisted: i64 = pool
        .with_conn(|c| {
            c.query_row(
                "SELECT COUNT(*) FROM agent_run_step WHERE run_id = ?1",
                [&paused_report.run_id],
                |r| r.get(0),
            )
        })
        .unwrap();
    assert_eq!(persisted, 6, "resume 后 agent_run_step 全量 6 行");
}

/// 判据 3：折叠行回查面（TaskMemoryStore::list_by_run 可读）+ compress_run
/// 后 summary 词形留存（013 生命周期语义兼容实测）。
#[test]
fn cb1_e2e_fold_rows_queryable_and_compress_keeps_summary_wordform() {
    let pool = migrated_pool();
    register_fixture(&pool);
    let driver = CountingDriver::succeed();
    let loop_ = build_loop(
        &pool,
        driver,
        RunControlConfig {
            fold: fold_on(),
            ..RunControlConfig::default()
        },
        8,
    );
    let mut req = make_request(Box::new(FoldingSeqPlanner::plan(3)));
    let report = loop_.run(&mut req).expect("run");
    assert!(!report.fold_events.is_empty());

    let task_memory = TaskMemoryStore::new(pool.clone());
    let rows = task_memory.list_by_run(&report.run_id).unwrap();
    let fold_rows: Vec<_> = rows
        .iter()
        .filter(|r| r.entry.summary().starts_with(FOLD_SUMMARY_PREFIX))
        .collect();
    assert_eq!(
        fold_rows.len(),
        report.fold_events.len(),
        "list_by_run 读到全部折叠行"
    );
    for row in &fold_rows {
        assert_eq!(row.entry.lifecycle().as_str(), "ACTIVE");
        assert!(row.entry.round() <= 2, "折叠行 round = 覆盖末轮");
        let payload: Value = serde_json::from_str(row.entry.content_json()).unwrap();
        assert_eq!(payload["kind"], "transcript-fold");
        assert!(payload["digest"].as_str().unwrap().len() == 64);
        assert!(payload["est_before"].as_u64().unwrap() > payload["est_after"].as_u64().unwrap());
    }

    // 013 生命周期语义兼容：compress_run 后 content_json 置空、summary 留存
    // （折叠行摘要词形在压缩后仍可引用回查）。
    let compressed = task_memory.compress_run(&report.run_id).unwrap();
    assert!(compressed >= fold_rows.len(), "压缩覆盖全部 ACTIVE 行");
    let rows_after = task_memory.list_by_run(&report.run_id).unwrap();
    for row in rows_after
        .iter()
        .filter(|r| r.entry.summary().starts_with(FOLD_SUMMARY_PREFIX))
    {
        assert_eq!(row.entry.lifecycle().as_str(), "COMPRESSED");
        assert!(
            row.entry.summary().starts_with(FOLD_SUMMARY_PREFIX),
            "compress_run 后折叠行 summary 词形留存"
        );
        assert!(
            row.entry.content_json().trim().is_empty(),
            "压缩置空细节载荷"
        );
    }
}

/// 判据 4：两级触发负例三连 + 折叠开启下既有硬护栏行为不变（预算数真实
/// 步数不因折叠放宽）。
#[test]
fn cb1_e2e_two_level_trigger_negatives_and_guardrails_unchanged() {
    let pool = migrated_pool();
    register_fixture(&pool);

    // 负例一：增长阈值未到（u64::MAX）不折叠。
    let driver = CountingDriver::succeed();
    let loop_ = build_loop(
        &pool,
        driver,
        RunControlConfig {
            fold: FoldConfig {
                enabled: true,
                fold_growth_tokens: u64::MAX,
                ..fold_on()
            },
            ..RunControlConfig::default()
        },
        8,
    );
    let mut req = make_request(Box::new(FoldingSeqPlanner::plan(5)));
    let report = loop_.run(&mut req).expect("run negative threshold");
    assert_eq!(report.state, "SUCCEEDED");
    assert!(report.fold_events.is_empty(), "阈值未到不折叠（负例一）");

    // 负例二：阈值到而轮距未到（间隔 100）不折叠（防抖负例）。
    let driver = CountingDriver::succeed();
    let loop_ = build_loop(
        &pool,
        driver,
        RunControlConfig {
            fold: FoldConfig {
                enabled: true,
                fold_growth_tokens: 1,
                fold_min_interval_rounds: 100,
                fold_hot_window_rounds: 0,
            },
            ..RunControlConfig::default()
        },
        8,
    );
    let mut req = make_request(Box::new(FoldingSeqPlanner::plan(5)));
    let report = loop_.run(&mut req).expect("run negative interval");
    assert!(report.fold_events.is_empty(), "轮距未到不折叠（防抖负例）");

    // 负例三：热窗覆盖全转录（hot ≥ 全部轮次）不折叠。
    let driver = CountingDriver::succeed();
    let loop_ = build_loop(
        &pool,
        driver,
        RunControlConfig {
            fold: FoldConfig {
                enabled: true,
                fold_growth_tokens: 1,
                fold_min_interval_rounds: 0,
                fold_hot_window_rounds: 100,
            },
            ..RunControlConfig::default()
        },
        8,
    );
    let mut req = make_request(Box::new(FoldingSeqPlanner::plan(5)));
    let report = loop_.run(&mut req).expect("run negative hot window");
    assert!(report.fold_events.is_empty(), "热窗段永不折叠（负例三）");

    // 既有硬护栏回归：max_rounds=1 + 折叠开 → 轮次预算照常硬停（折叠不放宽）。
    let driver = CountingDriver::succeed();
    let loop_ = build_loop(
        &pool,
        driver,
        RunControlConfig {
            fold: fold_on(),
            max_rounds: 1,
            ..RunControlConfig::default()
        },
        8,
    );
    let mut req = make_request(Box::new(FoldingSeqPlanner::plan(5)));
    let report = loop_.run(&mut req).expect("run max_rounds");
    assert_eq!(
        report.finish_reason, "guardrail: budget max_rounds=1 reached",
        "max_rounds 词形零改动"
    );
    assert_eq!(report.steps.len(), 1, "预算数真实步数");

    // 既有硬护栏回归：max_tool_calls=2 + 折叠开（折叠照常触发）→ 步数预算
    // 照常按真实步数硬停（折叠不放宽步数帽）。
    let driver = CountingDriver::succeed();
    let loop_ = build_loop(
        &pool,
        driver,
        RunControlConfig {
            fold: fold_on(),
            ..RunControlConfig::default()
        },
        2,
    );
    let mut req = make_request(Box::new(FoldingSeqPlanner::plan(5)));
    let report = loop_.run(&mut req).expect("run max_tool_calls");
    assert_eq!(
        report.finish_reason, "guardrail: max_tool_calls=2 reached",
        "max_tool_calls 词形零改动"
    );
    assert_eq!(report.steps.len(), 2, "步数帽数真实步数（不因折叠放宽）");
    assert!(
        !report.fold_events.is_empty(),
        "折叠照常触发（护栏与折叠互不影响）"
    );
    assert!(
        report
            .fold_events
            .iter()
            .all(|e| e.summary.starts_with(FOLD_SUMMARY_PREFIX)),
        "折叠行词形"
    );
    assert!(
        !report.steps.iter().any(|s| s.tool_id == FOLD_TOOL_ID),
        "权威 steps 不含合成折叠行（合成行只在规划器投影）"
    );
}

/// 判据 5 守卫：默认关 e2e 路径——fold_events 空 + 无折叠行落库 + steps 全量。
#[test]
fn cb1_e2e_default_off_zero_footprint() {
    let pool = migrated_pool();
    register_fixture(&pool);
    let driver = CountingDriver::succeed();
    // RunControlConfig::default()：fold.enabled=false（红线 4）。
    let loop_ = build_loop(&pool, driver, RunControlConfig::default(), 8);
    let mut req = make_request(Box::new(FoldingSeqPlanner::plan(3)));
    let report = loop_.run(&mut req).expect("run default off");
    assert_eq!(report.state, "SUCCEEDED");
    assert!(report.fold_events.is_empty(), "默认关 fold_events 空");
    assert_eq!(report.steps.len(), 3, "steps 全量");
    let fold_rows: i64 = pool
        .with_conn(|c| {
            c.query_row(
                "SELECT COUNT(*) FROM agent_task_memory WHERE run_id = ?1 AND summary LIKE 'transcript-fold:%'",
                [&report.run_id],
                |r| r.get(0),
            )
        })
        .unwrap();
    assert_eq!(fold_rows, 0, "默认关零折叠行落库");
}
