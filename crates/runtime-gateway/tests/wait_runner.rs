//! P2-4b 验证载具（ROADMAP P2-4 判据）：注入心跳失联用例，Runner 探测
//! 失败时新派发 run 的 tool_run.status 落 WAITING_FOR_RUNNER 而非 FAILED
//! （计划 §5.6:336-337 原文语义，复用 tool-engine lib.rs:316 既有注入点
//! 同一跃迁对），runtime_node.health 行同步更新；心跳恢复后 run 回
//! PREPARING；过渡期心跳即本机 podman 命令可用性检查（ADR §6 原文口径），
//! 不 tight-poll（间隔按 Manifest probe.interval_seconds 基线 600s）。
//!
//! 测试库：内存库迁移全链 001..007；工具经 tool-registry 登记示例
//! Manifest（占位 digest 显式放行，仅测试库）；platform*.db 零改动。

use database::{migrate, DbPath, Pool, PoolConfig};
use domain::{NodeHealth, RuntimeNode, RuntimeNodeKind, ToolRunStatus};
use runtime_gateway::{Gateway, GatewayConfig, HealthStore, SwitchProbe};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tool_engine::mock_runner::MockRunner;
use tool_engine::Engine;
use tool_registry::{Options, Registry};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root")
}

fn fresh_pool() -> Pool {
    let pool = Pool::open(PoolConfig {
        path: DbPath::Memory,
        readonly: false,
        max_connections: 1,
        busy_timeout: Duration::from_secs(1),
    })
    .unwrap();
    migrate(&pool, &repo_root().join("migrations")).unwrap();
    pool
}

fn request(run_id: &str) -> Value {
    json!({
        "protocol_version": "1",
        "tool_id": "rip.doc.parse",
        "run_id": run_id,
        "submit_key": format!("p2-4b-{run_id}"),
        "input": {"text": "# 标题\n\n正文一段。", "format": "md"},
        "workspace_id": "ws-p24b",
        "budget": {"timeout_seconds": 60, "max_output_bytes": 1048576},
        "result_schema_version": "1"
    })
}

fn result_ok() -> Value {
    json!({
        "format": "md",
        "source": {"kind": "inline", "path": null, "bytes": 30, "sha256": "abc"},
        "sections": [],
        "heading_tree": [],
        "stats": {"chars": 30, "sections": 1, "headings": 1}
    })
}

fn node_health_raw(pool: &Pool, node_id: &str, column: &str) -> Option<String> {
    pool.with_conn(|c| {
        c.query_row(
            &format!("SELECT {column} FROM runtime_node WHERE node_id = ?1"),
            [node_id],
            |r| r.get::<_, Option<String>>(0),
        )
    })
    .unwrap()
}

#[test]
fn probe_failure_lands_waiting_for_runner_not_failed_and_recovers() {
    let pool = fresh_pool();
    let registry = Registry::open(pool.clone()).with_options(Options {
        allow_placeholder_digest: true,
    });
    let manifest: Value = serde_json::from_str(
        &std::fs::read_to_string(
            repo_root().join("schemas/tool-manifest/examples/rip.doc.parse.json"),
        )
        .unwrap(),
    )
    .unwrap();
    registry.register(&manifest).unwrap();
    let engine = Engine::open(pool.clone(), registry);
    let store = HealthStore::open(pool.clone());
    // 默认 600s 间隔：dispatch 首探（无 last 记录 → due），恢复判定走 forced。
    let gateway = Gateway::new(engine.clone(), store.clone(), GatewayConfig::default());
    let probe = SwitchProbe::new(false); // 注入：Runner 探测失败
    gateway
        .register_node(
            RuntimeNode {
                node_id: "node-host-p24b".to_string(),
                kind: RuntimeNodeKind::PodmanHost,
                endpoint: None,
                health: NodeHealth::Unknown,
                version: None,
            },
            probe.clone(),
        )
        .unwrap();

    // Runner 探测失败时派发：落 WAITING_FOR_RUNNER，不 FAILED（§5.6:336）。
    let outcome = gateway.dispatch(&request("rn-p24b-wait")).unwrap();
    let run_id = outcome.run_id().to_string();
    match &outcome {
        runtime_gateway::DispatchOutcome::WaitingForRunner {
            health, node_id, ..
        } => {
            assert_eq!(*health, NodeHealth::Broken);
            assert_eq!(node_id, "node-host-p24b");
        }
        other => panic!("expected WaitingForRunner, got {other:?}"),
    }
    assert_eq!(
        engine.run_status(&run_id).unwrap(),
        Some(ToolRunStatus::WaitingForRunner),
        "run must land WAITING_FOR_RUNNER, not FAILED (计划 §5.6:336)"
    );
    // runtime_node.health 行同步更新 + 心跳时刻落列。
    assert_eq!(
        node_health_raw(&pool, "node-host-p24b", "health").as_deref(),
        Some("broken"),
        "runtime_node.health must record broken on probe failure"
    );
    assert!(
        node_health_raw(&pool, "node-host-p24b", "last_heartbeat_at").is_some(),
        "last_heartbeat_at must be stamped by the failed probe"
    );

    // 仍离线时的恢复判定：拒绝且 run 保持 WAITING_FOR_RUNNER（不误转终态）。
    let still_offline = gateway.probe_and_resume(&run_id);
    assert!(
        matches!(
            still_offline,
            Err(runtime_gateway::GatewayError::RunnerUnavailable { .. })
        ),
        "resume while offline must be rejected: {still_offline:?}"
    );
    assert_eq!(
        engine.run_status(&run_id).unwrap(),
        Some(ToolRunStatus::WaitingForRunner),
        "run must stay WAITING_FOR_RUNNER while probe keeps failing"
    );

    // 心跳恢复：run 回 PREPARING（§3.2 第 4 行），runtime_node.health 同步 ok。
    probe.set_healthy(true);
    let resumed = gateway.probe_and_resume(&run_id).unwrap();
    assert_eq!(
        resumed,
        ToolRunStatus::Preparing,
        "recovered run returns to PREPARING"
    );
    assert_eq!(
        engine.run_status(&run_id).unwrap(),
        Some(ToolRunStatus::Preparing)
    );
    assert_eq!(
        node_health_raw(&pool, "node-host-p24b", "health").as_deref(),
        Some("ok"),
        "runtime_node.health must record ok after recovery"
    );

    // 续跑到终态：drive_resumed 复用 drive 尾段，轨迹含 WAITING → PREPARING。
    let outcome = engine
        .drive_resumed(&run_id, &MockRunner::succeed(result_ok(), "p2-4b ok"))
        .unwrap();
    assert_eq!(
        outcome.transcript,
        vec![
            ToolRunStatus::WaitingForRunner,
            ToolRunStatus::Preparing,
            ToolRunStatus::Running,
            ToolRunStatus::Succeeded,
        ],
        "transcript must show the full heartbeat recovery trajectory"
    );
    assert_eq!(
        engine.run_status(&run_id).unwrap(),
        Some(ToolRunStatus::Succeeded)
    );
}

#[test]
fn dispatch_online_runner_stays_queued() {
    let pool = fresh_pool();
    let registry = Registry::open(pool.clone()).with_options(Options {
        allow_placeholder_digest: true,
    });
    let manifest: Value = serde_json::from_str(
        &std::fs::read_to_string(
            repo_root().join("schemas/tool-manifest/examples/rip.doc.parse.json"),
        )
        .unwrap(),
    )
    .unwrap();
    registry.register(&manifest).unwrap();
    let engine = Engine::open(pool.clone(), registry);
    let store = HealthStore::open(pool.clone());
    let gateway = Gateway::new(engine.clone(), store, GatewayConfig::default());
    let probe = SwitchProbe::new(true); // Runner 在位
    gateway
        .register_node(
            RuntimeNode {
                node_id: "node-host-up".to_string(),
                kind: RuntimeNodeKind::PythonWorker,
                endpoint: None,
                health: NodeHealth::Unknown,
                version: None,
            },
            probe,
        )
        .unwrap();

    let outcome = gateway.dispatch(&request("rn-p24b-queued")).unwrap();
    assert!(
        matches!(outcome, runtime_gateway::DispatchOutcome::Queued { .. }),
        "healthy runner must keep run QUEUED: {outcome:?}"
    );
    assert_eq!(
        engine.run_status(outcome.run_id()).unwrap(),
        Some(ToolRunStatus::Queued)
    );
    assert_eq!(
        node_health_raw(&pool, "node-host-up", "health").as_deref(),
        Some("ok")
    );
}

#[test]
fn probe_interval_guards_against_tight_poll() {
    let pool = fresh_pool();
    let registry = Registry::open(pool.clone()).with_options(Options {
        allow_placeholder_digest: true,
    });
    registry
        .register(
            &serde_json::from_str(
                &std::fs::read_to_string(
                    repo_root().join("schemas/tool-manifest/examples/rip.doc.parse.json"),
                )
                .unwrap(),
            )
            .unwrap(),
        )
        .unwrap();
    let engine = Engine::open(pool.clone(), registry);
    let store = HealthStore::open(pool.clone());
    // 显式 600s 基线间隔：间隔内的重复探测沿用落库值，不重复跑探针。
    let gateway = Gateway::new(
        engine,
        store,
        GatewayConfig {
            probe_interval: Duration::from_secs(600),
        },
    );
    let probe = SwitchProbe::new(true);
    gateway
        .register_node(
            RuntimeNode {
                node_id: "node-host-cached".to_string(),
                kind: RuntimeNodeKind::PodmanHost,
                endpoint: None,
                health: NodeHealth::Unknown,
                version: None,
            },
            probe.clone(),
        )
        .unwrap();

    assert_eq!(
        gateway.probe_node("node-host-cached").unwrap(),
        NodeHealth::Ok
    );
    assert_eq!(probe.calls.load(std::sync::atomic::Ordering::SeqCst), 1);
    // 间隔内第二次：不 tight-poll，探针不执行，返回落库健康。
    assert_eq!(
        gateway.probe_node("node-host-cached").unwrap(),
        NodeHealth::Ok
    );
    assert_eq!(
        probe.calls.load(std::sync::atomic::Ordering::SeqCst),
        1,
        "probe within interval must not re-run (600s baseline)"
    );
    // 强制探测无视间隔（恢复判定路径）。
    assert_eq!(
        gateway.probe_node_forced("node-host-cached").unwrap(),
        NodeHealth::Ok
    );
    assert_eq!(probe.calls.load(std::sync::atomic::Ordering::SeqCst), 2);
}

#[test]
fn unknown_node_and_missing_runner_are_rejected() {
    let pool = fresh_pool();
    let registry = Registry::open(pool.clone()).with_options(Options {
        allow_placeholder_digest: true,
    });
    // 先登记工具（否则 submit 在 NoRunnerRegistered 之前被 D-TC-05 拒）。
    registry
        .register(
            &serde_json::from_str(
                &std::fs::read_to_string(
                    repo_root().join("schemas/tool-manifest/examples/rip.doc.parse.json"),
                )
                .unwrap(),
            )
            .unwrap(),
        )
        .unwrap();
    let engine = Engine::open(pool.clone(), registry);
    let store = HealthStore::open(pool.clone());
    let gateway = Gateway::new(engine, store.clone(), GatewayConfig::default());
    // 未登记节点即派发。
    let err = gateway.dispatch(&request("rn-p24b-norunner")).unwrap_err();
    assert!(matches!(
        err,
        runtime_gateway::GatewayError::NoRunnerRegistered
    ));
    // 未登记节点探测。
    let err = gateway.probe_node("ghost-node").unwrap_err();
    assert!(matches!(err, runtime_gateway::GatewayError::UnknownNode(id) if id == "ghost-node"));
    // 健康库对未登记节点的 record 拒绝。
    let err = store
        .record("ghost-node", NodeHealth::Ok, Some("1.0"))
        .unwrap_err();
    assert!(matches!(err, runtime_gateway::GatewayError::UnknownNode(id) if id == "ghost-node"));
}
