//! P8-3 控制面与智能节点集成测试（018 控制表 + Agent/Human 节点执行器 +
//! workflow 级协作暂停/恢复 + 取消记状态 + 六 ID 关联列）。
//!
//! 覆盖（ask P8-3 判据的库内层；CLI/API 层等价断言在
//! apps/server/selftest_phase8_workflow.sh）：
//! - Agent 节点经 AgentNodeDriver 端口执行：fresh run → Paused（agent 循环
//!   步间消费暂停请求后返回），节点回 pending attempt 保留，节点/run 行
//!   agent_run 链接列落库；resume 续接至 SUCCEEDED，已完成节点 attempt
//!   不变；
//! - Human 人工介入节点：run 停在 awaiting_approval（018 控制行
//!   approval_node_id 非空且 approval_json 空），无注入 resume 重写请求
//!   再次停等（幂等），注入批准参数 resume 后终态 succeeded；
//! - 协作暂停「等完成再 checkpoint」定稿：暂停请求落在运行中 tool 节点
//!   期间，节点完成并落 checkpoint 后下一波转 paused（运行中节点不抢占）；
//! - 取消「记状态」定稿：cancel 直接跃迁之外向运行中实体传播留痕
//!   （tool_run.cancel_requested_at + 018 控制行 cancel_requested_at）；
//! - 六 ID 关联列：trace_id = run_id 创建即写，tool_run_id 节点/run 行落
//!   （submit_key 精确回查），artifact/evidence 经 015 trace 面刷新。

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use database::rusqlite::OptionalExtension;
use database::{migrate, DbPath, Pool, PoolConfig};
use serde_json::{json, Value};
use tool_engine::mock_runner::MockRunner;
use tool_engine::{DriveOutcome, Engine};
use workflow_engine::control::WorkflowControl;
use workflow_engine::exec::{node_agent, RunOutcome, ToolRunDriver, WorkflowExecutor};
use workflow_engine::state::RunStore;
use workflow_engine::store::WorkflowStore;

const MIGRATIONS_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../migrations");

fn temp_db(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "rip-wf-p83-test-{}-{}",
        tag,
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let path = dir.join(format!("{tag}.db"));
    let _ = std::fs::remove_file(&path);
    path
}

fn open_migrated(path: &Path) -> Pool {
    let pool = Pool::open(PoolConfig {
        path: DbPath::File(path.to_path_buf()),
        readonly: false,
        max_connections: 1,
        busy_timeout: Duration::from_secs(5),
    })
    .expect("open pool");
    let applied = migrate(&pool, Path::new(MIGRATIONS_DIR)).expect("migrate");
    assert!(
        applied.iter().any(|m| m.version == 18),
        "018 必须在本次迁移中应用（临时库 user_version 从 0 起步）"
    );
    pool
}

fn put_definition(pool: &Pool, text: &str) -> String {
    let doc = workflow_engine::parse_definition(text).expect("parse definition");
    let store = WorkflowStore::new(pool.clone());
    let outcome = store.put(&doc, "p83-test").expect("put definition");
    outcome.workflow_id
}

fn node_states(outcome: &RunOutcome) -> Vec<(String, &'static str, u32)> {
    outcome
        .nodes
        .iter()
        .map(|n| (n.node_id.clone(), n.state.as_str(), n.attempt))
        .collect()
}

/// 单列快查（python -I 对账的库内层等价）。
fn scalar(pool: &Pool, sql: &str) -> Option<String> {
    pool.with_conn(|c| {
        c.query_row(sql, [], |r| r.get::<_, Option<String>>(0))
            .optional()
            .map_err(database::DbError::Sqlite)
    })
    .expect("scalar query")
    .flatten()
}

/// Mock Agent 驱动：首次调用返回 Paused（协作暂停语义），此后返回 Succeeded。
struct PauseThenSucceedAgentDriver {
    calls: AtomicUsize,
}

impl node_agent::AgentNodeDriver for PauseThenSucceedAgentDriver {
    fn execute(&self, request: &node_agent::AgentNodeRequest) -> node_agent::AgentNodeOutcome {
        let n = self.calls.fetch_add(1, Ordering::SeqCst);
        assert!(!request.agent_run_id.is_empty(), "agent_run_id 必须预生成");
        assert_eq!(request.run_id, "wfr-p83-agent", "workflow run id 透传");
        assert_eq!(request.node_id, "think");
        assert_eq!(request.agent_id, "ag-wf-demo");
        if n == 0 {
            node_agent::AgentNodeOutcome::Paused {
                agent_run_id: request.agent_run_id.clone(),
            }
        } else {
            node_agent::AgentNodeOutcome::Succeeded {
                output: node_agent::report_projection(
                    &request.agent_run_id,
                    "SUCCEEDED",
                    "completed: all steps done",
                    1,
                    &[],
                    "mf-p83",
                    "ok",
                ),
            }
        }
    }
}

/// AGENT/HUMAN 样例定义（与 docs/phase8/samples 同形状，测试内联）。
const WF_AGENT: &str = r#"{
  "schema": "workflow/definition-v1",
  "workflow_id": "wf-p83-agent",
  "version": 1,
  "name": "P8-3 agent node (pause/resume)",
  "graph": {
    "nodes": [
      { "id": "start", "type": "start" },
      { "id": "think", "type": "agent", "params": { "agent_id": "ag-wf-demo", "goal": "调查 $/target" } },
      { "id": "done", "type": "end" }
    ],
    "edges": [
      { "from": "start", "to": "think" },
      { "from": "think", "to": "done" }
    ]
  }
}"#;

const WF_HUMAN: &str = r#"{
  "schema": "workflow/definition-v1",
  "workflow_id": "wf-p83-human",
  "version": 1,
  "name": "P8-3 human approval gate",
  "graph": {
    "nodes": [
      { "id": "start", "type": "start" },
      { "id": "review", "type": "human", "params": { "prompt": "批准外部访问?", "key": "approval" } },
      { "id": "done", "type": "end" }
    ],
    "edges": [
      { "from": "start", "to": "review" },
      { "from": "review", "to": "done" }
    ]
  }
}"#;

const WF_TOOL: &str = r#"{
  "schema": "workflow/definition-v1",
  "workflow_id": "wf-p83-gate-tool",
  "version": 1,
  "name": "P8-3 gated tool node (pause/cancel determinism)",
  "graph": {
    "nodes": [
      { "id": "start", "type": "start" },
      { "id": "search", "type": "tool", "params": { "tool_id": "rip.kb.keyword_search", "input": { "keyword": "nmap" } } },
      { "id": "done", "type": "end" }
    ],
    "edges": [
      { "from": "start", "to": "search" },
      { "from": "search", "to": "done" }
    ]
  }
}"#;

/// 登记测试工具（kb manifest，与 P8-2 测试同源）。
fn register_kb_tool(pool: &Pool) {
    let manifest_text =
        std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../manifests/tools/rip.kb.keyword_search.json"))
            .expect("read kb manifest");
    let manifest: Value = serde_json::from_str(&manifest_text).expect("kb manifest json");
    tool_registry::Registry::open(pool.clone())
        .register(&manifest)
        .expect("register kb tool");
}

fn mock_result() -> Value {
    json!({
        "keyword": "example.com",
        "per_group_limit": 1,
        "total_hits": 0,
        "groups": []
    })
}

/// 门控驱动：进入 WAITING_FOR_RUNNER（cancel_requestable 状态）后阻塞在
/// 门上，测试主线程放行后经心跳恢复路径跑到 SUCCEEDED。
struct GatedDriver {
    gate: Arc<(Mutex<bool>, std::sync::Condvar)>,
}

impl ToolRunDriver for GatedDriver {
    fn drive(&self, engine: &Engine, run_id: &str) -> Result<DriveOutcome, tool_engine::EngineError> {
        engine.mark_waiting_for_runner(run_id)?;
        {
            let (lock, cv) = &*self.gate;
            let mut open = lock.lock().expect("gate lock");
            while !*open {
                open = cv.wait(open).expect("gate cv");
            }
        }
        engine.resume_to_preparing(run_id)?;
        engine.drive_resumed(run_id, &MockRunner::succeed(mock_result(), "gated ok"))
    }
}

/// 判据 1 库内层：Agent 节点 fresh run 返回 Paused，节点回 pending attempt
/// 保留，agent_run 链接列落库；resume 续接至 SUCCEEDED 且已完成节点
/// attempt 不变。
#[test]
fn agent_node_pauses_then_resumes_with_completed_nodes_untouched() {
    let path = temp_db("agent");
    let pool = open_migrated(&path);
    let workflow_id = put_definition(&pool, WF_AGENT);
    let driver = Arc::new(PauseThenSucceedAgentDriver {
        calls: AtomicUsize::new(0),
    });
    let executor = WorkflowExecutor::new(pool.clone()).with_agent_driver(driver.clone());

    let outcome = executor
        .start_with_id(&workflow_id, None, &json!({"target": "example.com"}), Some("wfr-p83-agent"))
        .expect("paused outcome");
    assert_eq!(outcome.state, domain::WorkflowRunState::Paused, "{outcome:?}");
    let states = node_states(&outcome);
    // 节点回 pending（attempt 保留 1），start 已完成。
    assert!(states.contains(&("think".to_string(), "pending", 1)), "{states:?}");
    assert!(states.contains(&("start".to_string(), "succeeded", 1)), "{states:?}");

    // 六 ID 关联：trace_id = run_id 创建即写；agent_run 链接列节点/run 双落。
    assert_eq!(
        scalar(&pool, "SELECT trace_id FROM workflow_run WHERE run_id = 'wfr-p83-agent'"),
        Some("wfr-p83-agent".to_string())
    );
    let agent_run_id = scalar(
        &pool,
        "SELECT agent_run_id FROM workflow_run WHERE run_id = 'wfr-p83-agent'",
    )
    .expect("run agent_run_id");
    assert!(agent_run_id.starts_with("arn-"), "{agent_run_id}");
    assert_eq!(
        scalar(
            &pool,
            "SELECT agent_run_id FROM workflow_run_node WHERE run_id = 'wfr-p83-agent' AND node_id = 'think'"
        ),
        Some(agent_run_id.clone())
    );
    // 控制行在位（start_with_id 即建）。
    assert!(scalar(
        &pool,
        "SELECT run_id FROM workflow_run_control WHERE run_id = 'wfr-p83-agent'"
    )
    .is_some());

    // resume：PAUSED agent run 续接（驱动第二次调用）→ succeeded；
    // 已完成节点 attempt 不变，agent 节点重派发 attempt 1 → 2。
    let resumed = executor.resume("wfr-p83-agent").expect("resumed outcome");
    assert_eq!(resumed.state, domain::WorkflowRunState::Succeeded, "{resumed:?}");
    let states = node_states(&resumed);
    assert!(states.contains(&("think".to_string(), "succeeded", 2)), "{states:?}");
    assert!(states.contains(&("start".to_string(), "succeeded", 1)), "{states:?}");
    assert!(states.contains(&("done".to_string(), "succeeded", 1)), "{states:?}");
    assert_eq!(driver.calls.load(Ordering::SeqCst), 2);
}

/// 判据 2 库内层：人工介入节点 run 停在 awaiting_approval，无注入 resume
/// 重写请求再次停等（幂等），注入批准参数 resume 后终态 succeeded。
#[test]
fn human_node_awaits_approval_and_resume_injects_params() {
    let path = temp_db("human");
    let pool = open_migrated(&path);
    let workflow_id = put_definition(&pool, WF_HUMAN);
    let executor = WorkflowExecutor::new(pool.clone());
    let control = WorkflowControl::new(pool.clone());

    let outcome = executor
        .start_with_id(&workflow_id, None, &json!({}), Some("wfr-p83-human"))
        .expect("awaiting outcome");
    assert_eq!(outcome.state, domain::WorkflowRunState::Paused, "{outcome:?}");
    // 停在 awaiting_approval：控制行 approval_node_id 非空且 approval_json 空。
    let awaiting = control
        .awaiting_approval("wfr-p83-human")
        .expect("control read")
        .expect("awaiting row");
    assert_eq!(awaiting.approval_node_id.as_deref(), Some("review"));
    let request = awaiting.approval_request().expect("request payload");
    assert_eq!(request["prompt"], json!("批准外部访问?"));
    assert_eq!(request["node_key"], json!("approval"));
    // 节点保持 running（人的步进未完）。
    assert!(node_states(&outcome).contains(&("review".to_string(), "running", 1)));

    // 无注入 resume：恢复态无批准键，节点重写请求再次停等（幂等语义）。
    let again = executor.resume("wfr-p83-human").expect("re-await");
    assert_eq!(again.state, domain::WorkflowRunState::Paused);
    let awaiting2 = control.awaiting_approval("wfr-p83-human").expect("row").expect("still awaiting");
    assert_eq!(awaiting2.approval_node_id.as_deref(), Some("review"));
    assert!(awaiting2.approval_json.is_none());

    // 注入批准参数 resume：终态 succeeded，approval_json 落值留痕。
    let approved = executor
        .resume_with_injection("wfr-p83-human", Some(&json!({"approval": {"ok": true, "by": "reviewer"}})))
        .expect("approved outcome");
    assert_eq!(approved.state, domain::WorkflowRunState::Succeeded, "{approved:?}");
    let states = node_states(&approved);
    // attempt = 3：首次派发 + 无注入重派发（重写请求）+ 注入后重派发。
    assert!(states.contains(&("review".to_string(), "succeeded", 3)), "{states:?}");
    assert!(states.contains(&("done".to_string(), "succeeded", 1)), "{states:?}");
    let after = control.control_row("wfr-p83-human").expect("row").expect("some");
    assert!(!after.is_awaiting_approval());
    // serde_json Value 默认 BTreeMap 键序（规范键序，canonical_call_key 同源）。
    assert_eq!(
        after.approval_json.as_deref(),
        Some("{\"approval\":{\"by\":\"reviewer\",\"ok\":true}}")
    );
    // run 终态输出含批准参数（浅合并进 state）。
    let output = executor.status("wfr-p83-human").expect("status").0.output.expect("output");
    assert_eq!(output["approval"]["ok"], json!(true));
}

/// 协作暂停「等完成再 checkpoint」定稿：暂停请求落在运行中 tool 节点期间，
/// 节点完成并落 checkpoint 后下一波转 paused（运行中节点不抢占）。
#[test]
fn pause_waits_for_running_tool_node_then_checkpoints() {
    let path = temp_db("pause-wait");
    let pool = open_migrated(&path);
    register_kb_tool(&pool);
    let _workflow_id = put_definition(&pool, WF_TOOL);
    let gate: Arc<(Mutex<bool>, std::sync::Condvar)> = Arc::new((Mutex::new(false), std::sync::Condvar::new()));
    let executor = Arc::new(
        WorkflowExecutor::new(pool.clone())
            .with_tool_driver(Arc::new(GatedDriver { gate: gate.clone() })),
    );
    let runner_pool = pool.clone();
    let handle = std::thread::spawn(move || {
        executor.start_with_id("wf-p83-gate-tool", None, &json!({}), Some("wfr-p83-pause"))
    });

    // 等待 tool_run 进入 WAITING_FOR_RUNNER（门控窗口确定开）。
    let mut seen_waiting = false;
    for _ in 0..600 {
        let status: Option<String> = runner_pool
            .with_conn(|c| {
                c.query_row(
                    "SELECT status FROM tool_run WHERE trace_id = 'wfr-p83-pause' LIMIT 1",
                    [],
                    |r| r.get::<_, Option<String>>(0),
                )
                .optional()
                .map_err(database::DbError::Sqlite)
            })
            .expect("tool_run poll")
            .flatten();
        if status.as_deref() == Some("WAITING_FOR_RUNNER") {
            seen_waiting = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(seen_waiting, "tool run never reached WAITING_FOR_RUNNER");

    // 暂停请求落在节点运行中：请求位写 + agent 传播零命中（无 agent run）。
    let exec = WorkflowExecutor::new(runner_pool.clone());
    let state_at_request = exec.pause("wfr-p83-pause").expect("pause accepted");
    assert_eq!(state_at_request, domain::WorkflowRunState::Running);
    let control = WorkflowControl::new(runner_pool.clone());
    assert!(control
        .control_row("wfr-p83-pause")
        .expect("row")
        .expect("some")
        .pause_requested_at
        .is_some());

    // 放行门控：节点完成 + checkpoint 落库，下一波消费暂停 → paused。
    {
        let (lock, cv) = &*gate;
        let mut open = lock.lock().expect("gate lock");
        *open = true;
        cv.notify_all();
    }
    let outcome = handle.join().expect("start thread").expect("outcome");
    assert_eq!(outcome.state, domain::WorkflowRunState::Paused, "{outcome:?}");
    let states = node_states(&outcome);
    // 运行中节点等完成：search succeeded attempt 1 + checkpoint 在位；end 未派发。
    assert!(states.contains(&("search".to_string(), "succeeded", 1)), "{states:?}");
    assert!(states.contains(&("done".to_string(), "pending", 0)), "{states:?}");
    let runs = RunStore::new(runner_pool.clone());
    let cp = runs.latest_checkpoint("wfr-p83-pause").expect("cp").expect("checkpoint");
    assert_eq!(cp.node_id, "search");
    assert_eq!(cp.node_state, domain::WorkflowNodeRunState::Succeeded);

    // 六 ID 关联：tool_run_id 经 submit_key 回查落节点/run 行；trace_id = run_id。
    let tool_run_id = scalar(
        &runner_pool,
        "SELECT tool_run_id FROM workflow_run WHERE run_id = 'wfr-p83-pause'",
    )
    .expect("run tool_run_id");
    assert_eq!(
        scalar(
            &runner_pool,
            "SELECT tool_run_id FROM workflow_run_node WHERE run_id = 'wfr-p83-pause' AND node_id = 'search'"
        ),
        Some(tool_run_id.clone())
    );
    assert_eq!(
        scalar(
            &runner_pool,
            &format!("SELECT run_id FROM tool_run WHERE run_id = '{tool_run_id}'")
        ),
        Some(tool_run_id)
    );

    // resume：已完成节点 attempt 不变续跑成功（判据 1 后半）。
    let resumed = exec.resume("wfr-p83-pause").expect("resume");
    assert_eq!(resumed.state, domain::WorkflowRunState::Succeeded, "{resumed:?}");
    assert!(node_states(&resumed).contains(&("search".to_string(), "succeeded", 1)));
    assert!(node_states(&resumed).contains(&("done".to_string(), "succeeded", 1)));
}

/// 取消「记状态」定稿：cancel 直接跃迁之外，向运行中实体传播留痕
/// （tool_run.cancel_requested_at + 018 控制行 cancel_requested_at）。
#[test]
fn cancel_records_state_on_in_flight_tool_run() {
    let path = temp_db("cancel-mark");
    let pool = open_migrated(&path);
    register_kb_tool(&pool);
    let _workflow_id = put_definition(&pool, WF_TOOL);
    let gate: Arc<(Mutex<bool>, std::sync::Condvar)> = Arc::new((Mutex::new(false), std::sync::Condvar::new()));
    let executor = Arc::new(
        WorkflowExecutor::new(pool.clone())
            .with_tool_driver(Arc::new(GatedDriver { gate: gate.clone() })),
    );
    let runner_pool = pool.clone();
    let handle = std::thread::spawn(move || {
        executor.start_with_id("wf-p83-gate-tool", None, &json!({}), Some("wfr-p83-cancel"))
    });

    let mut seen_waiting = false;
    for _ in 0..600 {
        let status: Option<String> = runner_pool
            .with_conn(|c| {
                c.query_row(
                    "SELECT status FROM tool_run WHERE trace_id = 'wfr-p83-cancel' LIMIT 1",
                    [],
                    |r| r.get::<_, Option<String>>(0),
                )
                .optional()
                .map_err(database::DbError::Sqlite)
            })
            .expect("tool_run poll")
            .flatten();
        if status.as_deref() == Some("WAITING_FOR_RUNNER") {
            seen_waiting = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(seen_waiting, "tool run never reached WAITING_FOR_RUNNER");

    // 取消：run 直接跃迁 cancelled + 运行中 tool_run 记 cancel_requested_at。
    let exec = WorkflowExecutor::new(runner_pool.clone());
    let state = exec.cancel("wfr-p83-cancel").expect("cancel");
    assert_eq!(state, domain::WorkflowRunState::Cancelled);
    assert_eq!(
        scalar(
            &runner_pool,
            "SELECT cancel_requested_at FROM tool_run WHERE trace_id = 'wfr-p83-cancel'"
        )
        .expect("cancel_requested_at recorded"),
        scalar(
            &runner_pool,
            "SELECT cancel_requested_at FROM workflow_run_control WHERE run_id = 'wfr-p83-cancel'"
        )
        .expect("control row cancel mark")
    );
    // 放行后 run 保持 cancelled 终态（协作语义：节点完成不复活 run）。
    {
        let (lock, cv) = &*gate;
        let mut open = lock.lock().expect("gate lock");
        *open = true;
        cv.notify_all();
    }
    let outcome = handle.join().expect("start thread").expect("outcome");
    assert_eq!(outcome.state, domain::WorkflowRunState::Cancelled);
}
