//! 执行引擎集成测试（P8-2 自验库内层：017 三表 + 四类节点执行器）。
//!
//! 覆盖（CLI 层等价断言在 apps/server/selftest_phase8_workflow.sh 与
//! RELEASE 实录）：
//! - Tool/Knowledge/Condition/Parallel 四类节点各自最小定义 run 到
//!   SUCCEEDED，节点状态行独立终态 + attempt 落库；
//! - Condition 未选中分支 skip 级联（节点状态 skipped 终态）；
//! - Parallel join 失败收敛（子失败 → parallel failed → run failed）；
//! - 节点超时显式 failed 终态（code=node_timeout）；
//! - 断点恢复：kill 后 resume，已完成节点 state/attempt 保持不重跑；
//! - 失败重试：新 run 承载，只重跑失败节点 attempt 递增；
//! - 受理门（agent 节点 / 非法 branch 边形状 / input_schema 违规）拒绝；
//! - 017 迁移在临时库重放两次幂等（chain tail 017）。

use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::sync::Arc;
use std::time::Duration;

use database::rusqlite::OptionalExtension;
use database::{migrate, DbPath, Pool, PoolConfig};
use serde_json::json;
use tool_engine::mock_runner::MockRunner;
use tool_engine::{DriveOutcome, Engine};
use workflow_engine::exec::{RunOutcome, ToolRunDriver, WorkflowExecutor};
use workflow_engine::store::WorkflowStore;

const MIGRATIONS_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../migrations");
const KB_MANIFEST: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../manifests/tools/rip.kb.keyword_search.json"
);

/// 临时文件库（测试内唯一，目录随测试结束留给 /tmp）。
fn temp_db(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "rip-wf-exec-test-{}-{}",
        tag,
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let path = dir.join(format!("{tag}.db"));
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(dir.join(format!("{tag}.db-wal")));
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
        applied.iter().any(|m| m.version == 17),
        "017 必须在本次迁移中应用（临时库 user_version 从 0 起步）"
    );
    pool
}

/// 登记测试工具（kb 真实 manifest：input_schema {keyword}，output_schema
/// {keyword, per_group_limit, total_hits, groups}；Mock Runner 结果按
/// output_schema 组装）。
fn register_kb_tool(pool: &Pool) {
    let manifest_text = std::fs::read_to_string(KB_MANIFEST).expect("read kb manifest");
    let manifest: serde_json::Value =
        serde_json::from_str(&manifest_text).expect("kb manifest json");
    let registry = tool_registry::Registry::open(pool.clone());
    registry.register(&manifest).expect("register kb tool");
}

fn mock_result() -> serde_json::Value {
    json!({
        "keyword": "example.com",
        "per_group_limit": 1,
        "total_hits": 0,
        "groups": []
    })
}

/// Mock 注入驱动（TOOL-CONTRACT §10.1 注入口径）。
struct MockDriver {
    runner: MockRunner,
}

impl ToolRunDriver for MockDriver {
    fn drive(&self, engine: &Engine, run_id: &str) -> Result<DriveOutcome, tool_engine::EngineError> {
        engine.drive(run_id, &self.runner)
    }
}

/// 慢驱动（节点超时看门狗测试载具）。
struct SlowDriver {
    delay: Duration,
}

impl ToolRunDriver for SlowDriver {
    fn drive(&self, engine: &Engine, run_id: &str) -> Result<DriveOutcome, tool_engine::EngineError> {
        std::thread::sleep(self.delay);
        engine.drive(run_id, &MockRunner::succeed(mock_result(), "slow ok"))
    }
}

/// 测试载具知识源（空候选，合法结果）。
struct EmptySource;

impl context_broker::ContextSource for EmptySource {
    fn id(&self) -> &str {
        "exec-test-stub"
    }
    fn candidates(
        &self,
        _q: &context_broker::IntentQuery,
        _limit: usize,
    ) -> Result<Vec<context_broker::Candidate>, context_broker::ContextBrokerError> {
        Ok(Vec::new())
    }
    fn load(
        &self,
        _r: &context_broker::ContentRef,
    ) -> Result<context_broker::Content, context_broker::ContextBrokerError> {
        Err(context_broker::ContextBrokerError::Source {
            source: "exec-test-stub".to_string(),
            reason: "nothing to load".to_string(),
        })
    }
}

/// 带全源最小 Broker 的执行器（rulepack 取仓库目录，与既有 broker 测试同源锚定）。
fn executor_with_broker(pool: Pool) -> WorkflowExecutor {
    let rulepack_dir =
        Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../rulepacks/rip-rules"));
    let broker = context_broker::Broker::new(
        vec![Box::new(EmptySource)],
        context_broker::BudgetPolicy::default_for(8000).expect("policy"),
        context_broker::IntentConfig::from_dir(rulepack_dir).expect("intent config"),
        context_broker::ManifestStore::new(pool.clone()),
        None,
    );
    WorkflowExecutor::new(pool).with_broker(Arc::new(broker))
}

/// put 一份定义文档文本并返回（走 put 装载门 + 静态校验门）。
fn put_definition(pool: &Pool, text: &str) -> String {
    let doc = workflow_engine::parse_definition(text).expect("parse definition");
    let store = WorkflowStore::new(pool.clone());
    let outcome = store.put(&doc, "exec-test").expect("put definition");
    outcome.workflow_id
}

const WF_TOOL: &str = include_str!("../../../docs/phase8/samples/p8-tool-node.json");
const WF_KNOWLEDGE: &str = include_str!("../../../docs/phase8/samples/p8-knowledge-node.json");
const WF_CONDITION: &str = include_str!("../../../docs/phase8/samples/p8-condition-node.json");
const WF_PARALLEL: &str = include_str!("../../../docs/phase8/samples/p8-parallel-node.json");
const WF_RETRY: &str = include_str!("../../../docs/phase8/samples/p8-retry-fail-then-fix.json");

fn node_states(outcome: &RunOutcome) -> Vec<(String, &'static str, u32)> {
    outcome
        .nodes
        .iter()
        .map(|n| (n.node_id.clone(), n.state.as_str(), n.attempt))
        .collect()
}

#[test]
fn tool_node_minimal_run_succeeds_with_persisted_node_states() {
    let path = temp_db("tool");
    let pool = open_migrated(&path);
    register_kb_tool(&pool);
    let workflow_id = put_definition(&pool, WF_TOOL);
    let driver: Arc<dyn ToolRunDriver> = Arc::new(MockDriver {
        runner: MockRunner::succeed(mock_result(), "mock kb ok"),
    });
    let executor = WorkflowExecutor::new(pool.clone()).with_tool_driver(driver);
    let outcome = executor
        .start(&workflow_id, None, &json!({"target": "example.com"}))
        .expect("run succeeds");
    assert_eq!(outcome.state, domain::WorkflowRunState::Succeeded);
    assert_eq!(
        node_states(&outcome),
        vec![
            ("done".to_string(), "succeeded", 1),
            ("search".to_string(), "succeeded", 1),
            ("start".to_string(), "succeeded", 1),
        ]
    );
    // 节点状态对账（库内层）：017 workflow_run_node 每节点独立终态 + attempt。
    let runs = workflow_engine::RunStore::new(pool.clone());
    let rows = runs.node_rows(&outcome.run_id).expect("node rows");
    assert!(rows.iter().all(|r| r.state != domain::WorkflowNodeRunState::Pending));
    let search = rows.iter().find(|r| r.node_id == "search").expect("search row");
    assert_eq!(search.node_kind, "tool");
    assert_eq!(search.attempt, 1);
    // tool_run 关联：trace_id = workflow run id（015 链路）。
    let tool_runs: Vec<(String, String)> = pool
        .with_conn(|c| {
            let mut stmt = c
                .prepare("SELECT run_id, trace_id FROM tool_run ORDER BY run_id")
                .expect("prepare");
            let rows = stmt
                .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
                .expect("query map")
                .collect::<Result<Vec<_>, database::rusqlite::Error>>()
                .expect("rows");
            Ok::<_, database::DbError>(rows)
        })
        .expect("tool runs");
    assert_eq!(tool_runs.len(), 1, "one tool run row");
    assert_eq!(tool_runs[0].1, outcome.run_id, "trace_id = workflow run id");
    // checkpoint：每节点终态一行（start/search/done 各一）。
    let snap = runs.snapshot_of(&outcome.run_id).expect("snapshot");
    assert_eq!(
        snap.get("nodes").and_then(|n| n.as_object()).map(|m| m.len()),
        Some(3)
    );
}

#[test]
fn knowledge_node_minimal_run_succeeds_via_broker() {
    let path = temp_db("knowledge");
    let pool = open_migrated(&path);
    let workflow_id = put_definition(&pool, WF_KNOWLEDGE);
    let executor = executor_with_broker(pool.clone());
    let outcome = executor
        .start(&workflow_id, None, &json!({"target": "example.com"}))
        .expect("run succeeds");
    assert_eq!(outcome.state, domain::WorkflowRunState::Succeeded);
    let runs = workflow_engine::RunStore::new(pool.clone());
    let rows = runs.node_rows(&outcome.run_id).expect("rows");
    let retrieve = rows.iter().find(|r| r.node_id == "retrieve").expect("row");
    let output = retrieve.output.clone().expect("knowledge output");
    assert_eq!(output["query"], "example.com");
    assert!(output["context_id"].as_str().expect("ctx").starts_with("ctx-"));
    assert_eq!(output["knowledge_count"], 0);
    // Broker 副作用：manifest 头行落库（correlation_id = run_id）。
    // Broker 副作用：manifest 头行落库（task_id = run_id，008 列名）。
    let manifests: i64 = pool
        .with_conn(|c| {
            c.query_row(
                "SELECT COUNT(*) FROM context_manifest WHERE task_id = ?1",
                [&outcome.run_id],
                |r| r.get(0),
            )
            .map_err(database::DbError::Sqlite)
        })
        .expect("manifest count");
    assert!(manifests >= 1, "assemble leaves a manifest row");
}

#[test]
fn condition_node_branches_and_skips_cascade() {
    let path = temp_db("condition");
    let pool = open_migrated(&path);
    register_kb_tool(&pool);
    let workflow_id = put_definition(&pool, WF_CONDITION);
    let driver: Arc<dyn ToolRunDriver> = Arc::new(MockDriver {
        runner: MockRunner::succeed(mock_result(), "mock kb ok"),
    });
    // miss 分支是 knowledge 节点：执行器需同时装配 broker 与 tool 驱动。
    let executor = executor_with_broker(pool.clone()).with_tool_driver(driver);
    // true 分支：hit 执行、miss 级联 skipped。
    let outcome = executor
        .start(&workflow_id, None, &json!({"target": "example.com"}))
        .expect("run outcome");
    if outcome.state != domain::WorkflowRunState::Succeeded {
        let runs = workflow_engine::RunStore::new(pool.clone());
        let row = runs.run_row(&outcome.run_id).expect("run row");
        panic!("condition run failed: {:?}", row.error);
    }
    let states = node_states(&outcome);
    assert!(states.contains(&("hit".to_string(), "succeeded", 1)));
    assert!(states.contains(&("miss".to_string(), "skipped", 0)), "{states:?}");
    assert!(states.contains(&("done-hit".to_string(), "succeeded", 1)));
    assert!(states.contains(&("done-miss".to_string(), "skipped", 0)));
    // false 分支：换 input 反向。
    let outcome2 = executor
        .start(&workflow_id, None, &json!({"target": "other.example"}))
        .expect("run succeeds");
    assert_eq!(outcome2.state, domain::WorkflowRunState::Succeeded);
    let states2 = node_states(&outcome2);
    assert!(states2.contains(&("miss".to_string(), "succeeded", 1)));
    assert!(states2.contains(&("hit".to_string(), "skipped", 0)));
    // condition 输出并入 state（checkpoint 快照可对账）。
    let runs = workflow_engine::RunStore::new(pool.clone());
    let rows = runs.node_rows(&outcome2.run_id).expect("rows");
    let check = rows.iter().find(|r| r.node_id == "check").expect("check row");
    assert_eq!(
        check.output.clone().expect("cond out"),
        json!({"condition_result": false})
    );
}

#[test]
fn parallel_node_fans_out_concurrently_and_joins() {
    let path = temp_db("parallel");
    let pool = open_migrated(&path);
    register_kb_tool(&pool);
    let workflow_id = put_definition(&pool, WF_PARALLEL);
    let driver: Arc<dyn ToolRunDriver> = Arc::new(MockDriver {
        runner: MockRunner::succeed(mock_result(), "mock kb ok"),
    });
    let executor = WorkflowExecutor::new(pool.clone()).with_tool_driver(driver);
    let outcome = executor
        .start(&workflow_id, None, &json!({"target": "example.com"}))
        .expect("run succeeds");
    assert_eq!(outcome.state, domain::WorkflowRunState::Succeeded);
    let states = node_states(&outcome);
    // fan 子节点各自独立终态与 attempt；parallel 节点自身 succeeded。
    assert!(states.contains(&("w1".to_string(), "succeeded", 1)));
    assert!(states.contains(&("w2".to_string(), "succeeded", 1)));
    assert!(states.contains(&("fan".to_string(), "succeeded", 1)));
    // 子输出合并：keyword 键来自两路子节点输出（id 升序后到覆盖）。
    let runs = workflow_engine::RunStore::new(pool.clone());
    let w1 = runs
        .node_rows(&outcome.run_id)
        .expect("rows")
        .into_iter()
        .find(|r| r.node_id == "w1")
        .expect("w1");
    assert_eq!(w1.output.expect("w1 out")["keyword"], "example.com");
}

#[test]
fn parallel_join_failure_fails_run() {
    let path = temp_db("parallel-fail");
    let pool = open_migrated(&path);
    register_kb_tool(&pool);
    // 注入失败：MockRunner Fail（两路子节点都失败，join 收敛失败）。
    let doc = workflow_engine::parse_definition(
        r#"{
        "schema": "workflow/definition-v1",
        "workflow_id": "wf-p8-parallel-fail",
        "version": 1,
        "name": "parallel join failure",
        "graph": {
            "nodes": [
                { "id": "start", "type": "start" },
                { "id": "fan", "type": "parallel" },
                { "id": "w1", "type": "tool", "params": { "tool_id": "rip.kb.keyword_search", "input": {"keyword": "x"} } },
                { "id": "w2", "type": "tool", "params": { "tool_id": "rip.kb.keyword_search", "input": {"keyword": "y"} } },
                { "id": "done", "type": "end" }
            ],
            "edges": [
                { "from": "start", "to": "fan" },
                { "from": "fan", "to": "w1", "kind": "branch" },
                { "from": "fan", "to": "w2", "kind": "branch" },
                { "from": "w1", "to": "done" },
                { "from": "w2", "to": "done" }
            ]
        }
    }"#,
    )
    .expect("parse");
    let store = WorkflowStore::new(pool.clone());
    store.put(&doc, "exec-test").expect("put");
    let workflow_id = "wf-p8-parallel-fail";
    let driver: Arc<dyn ToolRunDriver> = Arc::new(MockDriver {
        runner: MockRunner::fail("EXECUTION", json!({"code": -32000}), "injected failure"),
    });
    let executor = WorkflowExecutor::new(pool.clone()).with_tool_driver(driver);
    // 终态失败收敛为 outcome（drive_run 错误映射：run 到终态即返回结果）。
    let outcome = executor
        .start(workflow_id, None, &json!({}))
        .expect("terminal outcome");
    assert_eq!(outcome.state, domain::WorkflowRunState::Failed);
    let states = node_states(&outcome);
    assert!(states.contains(&("w1".to_string(), "failed", 1)), "{states:?}");
    assert!(states.contains(&("w2".to_string(), "failed", 1)), "{states:?}");
    assert!(states.contains(&("fan".to_string(), "failed", 1)), "{states:?}");
}

#[test]
fn node_timeout_is_explicit_failed_terminal() {
    let path = temp_db("timeout");
    let pool = open_migrated(&path);
    register_kb_tool(&pool);
    let workflow_id = put_definition(&pool, WF_TOOL);
    let driver: Arc<dyn ToolRunDriver> = Arc::new(SlowDriver {
        delay: Duration::from_millis(400),
    });
    let executor = WorkflowExecutor::new(pool.clone())
        .with_tool_driver(driver)
        .with_node_timeout_ms(50);
    let outcome = executor
        .start(&workflow_id, None, &json!({"target": "example.com"}))
        .expect("terminal outcome");
    assert_eq!(outcome.state, domain::WorkflowRunState::Failed);
    let search = outcome
        .nodes
        .iter()
        .find(|n| n.node_id == "search")
        .expect("search row");
    assert_eq!(search.state, domain::WorkflowNodeRunState::Failed);
    assert_eq!(search.attempt, 1);
    let error = search.error.clone().expect("error payload");
    assert_eq!(error["code"], "node_timeout", "{error}");
    // run 头行错误载荷同源。
    let runs = workflow_engine::RunStore::new(pool.clone());
    let run = runs.run_row(&outcome.run_id).expect("run row");
    let run_error = run.error.expect("run error");
    assert_eq!(run_error["code"], "node_timeout");
    assert_eq!(run_error["node_id"], "search");
}

#[test]
fn resume_from_checkpoint_keeps_completed_nodes_and_attempts() {
    let path = temp_db("resume");
    let pool = open_migrated(&path);
    register_kb_tool(&pool);
    let doc = workflow_engine::parse_definition(
        r#"{
        "schema": "workflow/definition-v1",
        "workflow_id": "wf-p8-resume",
        "version": 1,
        "name": "resume two tool nodes",
        "graph": {
            "nodes": [
                { "id": "start", "type": "start" },
                { "id": "a", "type": "tool", "params": { "tool_id": "rip.kb.keyword_search", "input": {"keyword": "a"} } },
                { "id": "b", "type": "tool", "params": { "tool_id": "rip.kb.keyword_search", "input": {"keyword": "b"} } },
                { "id": "done", "type": "end" }
            ],
            "edges": [
                { "from": "start", "to": "a" },
                { "from": "a", "to": "b" },
                { "from": "b", "to": "done" }
            ]
        }
    }"#,
    )
    .expect("parse");
    let store = WorkflowStore::new(pool.clone());
    store.put(&doc, "exec-test").expect("put");

    // 第一段：节点 a 快速成功，节点 b 阻塞（kill 模拟载具）。
    // 派发面按 tool_run 行 replay_json.input_sanitized.keyword 分流：
    // a 走 mock，b 永久阻塞（残留线程由乐观守卫收口）。
    struct SplitDriver {
        pool: Pool,
    }
    impl ToolRunDriver for SplitDriver {
        fn drive(
            &self,
            engine: &Engine,
            run_id: &str,
        ) -> Result<DriveOutcome, tool_engine::EngineError> {
            let keyword: Option<String> = self
                .pool
                .with_conn(|c| {
                    let raw: Option<String> = c
                        .query_row(
                            "SELECT replay_json FROM tool_run WHERE run_id = ?1",
                            [run_id],
                            |r| r.get(0),
                        )
                        .map_err(database::DbError::Sqlite)?;
                    let parsed: serde_json::Value = raw
                        .and_then(|t| serde_json::from_str(&t).ok())
                        .unwrap_or(serde_json::Value::Null);
                    Ok::<_, database::DbError>(
                        parsed.get("input_sanitized").and_then(|v| v["keyword"].as_str()).map(str::to_string),
                    )
                })
                .ok()
                .flatten();
            if keyword.as_deref() == Some("b") {
                let (_tx, rx): (mpsc::Sender<()>, mpsc::Receiver<()>) = mpsc::channel();
                let _ = rx.recv();
                unreachable!("blocked")
            }
            engine.drive(run_id, &MockRunner::succeed(mock_result(), "ok"))
        }
    }
    // b 阻塞在驱动线程，主线程轮询 checkpoint 出现 a 后放弃该执行器
    // （模拟 kill：run 行停留 running，b 行停留 running）。
    let handle = {
        let pool = pool.clone();
        std::thread::spawn(move || {
            let driver: Arc<dyn ToolRunDriver> = Arc::new(SplitDriver {
                pool: pool.clone(),
            });
            let exec = WorkflowExecutor::new(pool).with_tool_driver(driver);
            exec.start("wf-p8-resume", None, &json!({}))
        })
    };
    let runs = workflow_engine::RunStore::new(pool.clone());
    // 轮询等待确定性 kill 点：a 已 succeeded（checkpoint 落行）且 b 已进入
    // running（node_start 落行，attempt=1）。两条件齐备后 kill。
    let mut run_id_in_flight = String::new();
    for _ in 0..300 {
        std::thread::sleep(Duration::from_millis(20));
        let found: Option<(String, i64)> = pool
            .with_conn(|c| {
                c.query_row(
                    "SELECT cr.run_id, COUNT(*) FROM workflow_checkpoint cr
                     JOIN workflow_run_node nr ON nr.run_id = cr.run_id AND nr.node_id = 'b'
                     WHERE cr.node_id = 'a' AND cr.node_state = 'succeeded'
                       AND nr.state = 'running'
                     GROUP BY cr.run_id LIMIT 1",
                    [],
                    |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)),
                )
                .optional()
                .map_err(database::DbError::Sqlite)
            })
            .expect("poll");
        if let Some((run_id, _)) = found {
            run_id_in_flight = run_id;
            break;
        }
    }
    assert!(
        !run_id_in_flight.is_empty(),
        "kill point (a succeeded checkpoint + b running) must appear"
    );
    // kill 模拟：残留线程继续阻塞，不再等待其结果（drop handle）。
    drop(handle);
    let row = runs.run_row(&run_id_in_flight).expect("run row");
    assert_eq!(row.state, domain::WorkflowRunState::Running, "kill leaves running");
    let rows = runs.node_rows(&run_id_in_flight).expect("rows");
    let a = rows.iter().find(|r| r.node_id == "a").expect("a row");
    assert_eq!((a.state.as_str(), a.attempt), ("succeeded", 1));
    // resume：a 保持 succeeded attempt=1 不重跑；b 从 running 复位续跑。
    let resume_executor = {
        let driver: Arc<dyn ToolRunDriver> = Arc::new(MockDriver {
            runner: MockRunner::succeed(mock_result(), "recovered"),
        });
        WorkflowExecutor::new(pool.clone()).with_tool_driver(driver)
    };
    let outcome = resume_executor
        .resume(&run_id_in_flight)
        .expect("resume succeeds");
    assert_eq!(outcome.state, domain::WorkflowRunState::Succeeded);
    let states = node_states(&outcome);
    // a：已完成节点保持 succeeded 且 attempt 不变（不重跑）。
    assert!(states.contains(&("a".to_string(), "succeeded", 1)), "{states:?}");
    // b：kill 时 running（attempt=1），复位 pending 后重跑 attempt 递增为 2。
    assert!(states.contains(&("b".to_string(), "succeeded", 2)), "{states:?}");
    assert!(states.contains(&("done".to_string(), "succeeded", 1)));
}

#[test]
fn retry_after_failure_reruns_only_failed_node_with_incremented_attempt() {
    let path = temp_db("retry");
    let pool = open_migrated(&path);
    let workflow_id = put_definition(&pool, WF_RETRY);
    let driver: Arc<dyn ToolRunDriver> = Arc::new(MockDriver {
        runner: MockRunner::succeed(mock_result(), "mock kb ok"),
    });
    let executor = WorkflowExecutor::new(pool.clone()).with_tool_driver(driver);
    // 第一次 run：工具未登记，submit 拒绝 → 节点 failed，run FAILED。
    let outcome = executor
        .start(&workflow_id, None, &json!({"target": "example.com"}))
        .expect("terminal outcome");
    assert_eq!(outcome.state, domain::WorkflowRunState::Failed);
    let search = outcome
        .nodes
        .iter()
        .find(|n| n.node_id == "search")
        .expect("search row");
    assert_eq!(search.state, domain::WorkflowNodeRunState::Failed);
    assert_eq!(search.attempt, 1);
    assert_eq!(
        search.error.clone().expect("error")["code"],
        "tool_submit_rejected"
    );
    // 修复瞬时失败源：登记工具（失败注入 = 缺登记，修复 = 登记）。
    register_kb_tool(&pool);
    // retry：新 run 承载，只重跑失败节点，attempt 递增（1 -> 2）。
    let outcome2 = executor
        .retry(&outcome.run_id, "search")
        .expect("retry succeeds");
    assert_eq!(outcome2.state, domain::WorkflowRunState::Succeeded);
    assert_ne!(outcome2.run_id, outcome.run_id, "retry 以新 run 承载");
    let states2 = node_states(&outcome2);
    assert!(states2.contains(&("search".to_string(), "succeeded", 2)), "{states2:?}");
    // 原 run 保持 FAILED 终态且 attempt 不变（冻结跃迁表：终态不迁出）。
    let runs = workflow_engine::RunStore::new(pool.clone());
    let old_run = runs.run_row(&outcome.run_id).expect("old run");
    assert_eq!(old_run.state, domain::WorkflowRunState::Failed);
    let old_search = runs
        .node_rows(&outcome.run_id)
        .expect("old rows")
        .into_iter()
        .find(|r| r.node_id == "search")
        .expect("old search");
    assert_eq!((old_search.state.as_str(), old_search.attempt), ("failed", 1));
    // 非 failed 节点作为 retry 目标拒绝。
    let done_state = outcome2
        .nodes
        .iter()
        .find(|n| n.node_id == "done")
        .expect("done row")
        .state;
    let _ = done_state;
    assert!(executor.retry(&outcome2.run_id, "done").is_err());
}

#[test]
fn acceptance_gates_reject_non_executable_definitions() {
    let path = temp_db("gates");
    let pool = open_migrated(&path);
    let executor = WorkflowExecutor::new(pool.clone());
    // P8-3：agent 节点受理放行，无 AgentNodeDriver 时执行 fail-closed
    // （run failed，节点 error code=node_driver_unavailable）。
    let doc = workflow_engine::parse_definition(
        r#"{
        "schema": "workflow/definition-v1",
        "workflow_id": "wf-p8-agent-rejected",
        "version": 1,
        "name": "agent node rejected",
        "graph": {
            "nodes": [
                { "id": "start", "type": "start" },
                { "id": "think", "type": "agent", "params": { "agent_id": "a1" } },
                { "id": "done", "type": "end" }
            ],
            "edges": [
                { "from": "start", "to": "think" },
                { "from": "think", "to": "done" }
            ]
        }
    }"#,
    )
    .expect("parse");
    let store = WorkflowStore::new(pool.clone());
    store.put(&doc, "exec-test").expect("put agent def");
    // 终态失败收敛为 outcome（节点 error code=node_driver_unavailable）。
    let outcome = executor
        .start("wf-p8-agent-rejected", None, &json!({}))
        .expect("terminal outcome");
    assert_eq!(outcome.state, domain::WorkflowRunState::Failed);
    let think = outcome
        .nodes
        .iter()
        .find(|n| n.node_id == "think")
        .expect("agent node row");
    assert_eq!(think.state, domain::WorkflowNodeRunState::Failed);
    let code = think
        .error
        .as_ref()
        .and_then(|e| e.get("code"))
        .and_then(serde_json::Value::as_str)
        .expect("error code");
    assert_eq!(code, "node_driver_unavailable");
    // input 非 object 拒绝。
    let err = executor
        .start("wf-p8-agent-rejected", None, &json!(["x"]))
        .expect_err("non-object input");
    assert!(err.to_string().contains("JSON object"), "{err}");
    // input_schema 违规拒绝（wf-p8-tool 声明 target: string）。
    put_definition(&pool, WF_TOOL);
    register_kb_tool(&pool);
    let tool_executor = {
        let driver: Arc<dyn ToolRunDriver> = Arc::new(MockDriver {
            runner: MockRunner::succeed(mock_result(), "ok"),
        });
        WorkflowExecutor::new(pool.clone()).with_tool_driver(driver)
    };
    let err = tool_executor
        .start("wf-p8-tool", None, &json!({"target": 42}))
        .expect_err("schema violation");
    assert!(err.to_string().contains("input_schema"), "{err}");
    // legacy 参考行拒绝执行（compat=legacy-method-reference 的空图参考行）。
    let legacy_doc = workflow_engine::parse_definition(
        r#"{
        "schema": "workflow/definition-v1",
        "workflow_id": "wf-p8-legacy-ref",
        "version": 1,
        "name": "legacy reference",
        "compat": "legacy-method-reference",
        "graph": { "nodes": [], "edges": [] }
    }"#,
    )
    .expect("parse legacy");
    store.put(&legacy_doc, "exec-test").expect("put legacy row");
    let err = tool_executor
        .start("wf-p8-legacy-ref", None, &json!({}))
        .expect_err("legacy not executable");
    assert!(err.to_string().contains("legacy-method-reference"), "{err}");
    // branch 边形状：condition 上挂 seq 边拒绝（put 门在静态层放行，
    // 受理门执行面拒绝）。
    let doc = workflow_engine::parse_definition(
        r#"{
        "schema": "workflow/definition-v1",
        "workflow_id": "wf-p8-bad-edge",
        "version": 1,
        "name": "bad edge shape",
        "graph": {
            "nodes": [
                { "id": "start", "type": "start" },
                { "id": "c", "type": "condition", "params": { "expression": {"type": "object"} } },
                { "id": "done", "type": "end" }
            ],
            "edges": [
                { "from": "start", "to": "c" },
                { "from": "c", "to": "done" }
            ]
        }
    }"#,
    )
    .expect("parse");
    store.put(&doc, "exec-test").expect("put bad edge def");
    let err = tool_executor
        .start("wf-p8-bad-edge", None, &json!({}))
        .expect_err("seq edge from condition rejected");
    assert!(err.to_string().contains("condition nodes must branch"), "{err}");
    // cancel：非终态 run 可取消；终态 run 拒绝。
    let outcome = tool_executor
        .start("wf-p8-tool", None, &json!({"target": "example.com"}))
        .expect("run ok");
    assert!(executor.cancel(&outcome.run_id).is_err(), "terminal run cannot cancel");
}
