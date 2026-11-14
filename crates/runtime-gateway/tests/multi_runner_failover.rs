//! P10-3 验证载具（ROADMAP §11.3 P10-3 判据）：双 Runner 下调度与心跳正确；
//! 单节点离线不丢任务。覆盖：
//! 1. 派发优先序：主节点可用时不再探测后续候选（优先承接）；
//! 2. 故障转移：主节点离线时任务转投备用节点（QUEUED，不落 WAITING）；
//! 3. 全候选离线：run 落 WAITING_FOR_RUNNER（挂主节点，§5.6:336 不 FAILED）；
//! 4. 恢复续跑：任一候选恢复即回 PREPARING（probe_and_resume），批量恢复
//!    （recover_waiting_runs）不丢任务；状态机轨迹含 WAITING → PREPARING；
//! 5. set_primary 重排候选优先序；未知节点拒绝。
//!
//! 测试库：内存库迁移全链；工具经 tool-registry 登记示例 Manifest（占位
//! digest 显式放行，仅测试库）；platform*.db 零改动。探测间隔取 ZERO：
//! 每次派发都真实重探（生产基线 600s 间隔由 wait_runner.rs 的
//! probe_interval_guards_against_tight_poll 固化，本文件不重复）。

use database::{DbPath, Pool, PoolConfig, migrate};
use domain::{NodeHealth, RuntimeNode, RuntimeNodeKind, ToolRunStatus};
use runtime_gateway::{DispatchOutcome, Gateway, GatewayConfig, HealthStore, SwitchProbe};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tool_engine::Engine;
use tool_engine::mock_runner::MockRunner;
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
        "submit_key": format!("p10-3-{run_id}"),
        "input": {"text": "# 标题\n\n正文一段。", "format": "md"},
        "workspace_id": "ws-p103",
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

/// 双 Runner 沙盘：node-vm（VM 面）与 node-host（宿主面），各自独立探针。
/// 返回（gateway, engine, pool, vm_probe, host_probe）。
fn two_runner_env() -> (Gateway, Engine, Pool, Arc<SwitchProbe>, Arc<SwitchProbe>) {
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
    let gateway = Gateway::new(
        engine.clone(),
        store,
        GatewayConfig {
            probe_interval: Duration::ZERO,
        },
    );
    let vm_probe = SwitchProbe::new(true);
    let host_probe = SwitchProbe::new(true);
    gateway
        .register_node(
            RuntimeNode {
                node_id: "node-vm-p103".to_string(),
                kind: RuntimeNodeKind::PodmanVm,
                endpoint: None,
                health: NodeHealth::Unknown,
                version: None,
            },
            vm_probe.clone(),
        )
        .unwrap();
    gateway
        .register_node(
            RuntimeNode {
                node_id: "node-host-p103".to_string(),
                kind: RuntimeNodeKind::PodmanHost,
                endpoint: None,
                health: NodeHealth::Unknown,
                version: None,
            },
            host_probe.clone(),
        )
        .unwrap();
    (gateway, engine, pool, vm_probe, host_probe)
}

#[test]
fn dispatch_prefers_primary_then_fails_over_to_backup() {
    let (gateway, engine, pool, vm_probe, host_probe) = two_runner_env();
    assert_eq!(
        gateway.candidate_nodes(),
        vec!["node-vm-p103", "node-host-p103"],
        "candidates are primary first, then registration order"
    );

    // 主节点可用：QUEUED 落主节点，不触发转移。
    let outcome = gateway.dispatch(&request("rn-p103-primary")).unwrap();
    match &outcome {
        DispatchOutcome::Queued {
            node_id, health, ..
        } => {
            assert_eq!(node_id, "node-vm-p103", "healthy primary takes the run");
            assert_eq!(*health, NodeHealth::Ok);
        }
        other => panic!("expected Queued on primary, got {other:?}"),
    }
    assert_eq!(
        engine.run_status(outcome.run_id()).unwrap(),
        Some(ToolRunStatus::Queued)
    );

    // 主节点离线：任务转投备用节点（QUEUED，不丢、不 WAITING）。
    vm_probe.set_healthy(false);
    let outcome = gateway.dispatch(&request("rn-p103-failover")).unwrap();
    match &outcome {
        DispatchOutcome::Queued {
            node_id, health, ..
        } => {
            assert_eq!(node_id, "node-host-p103", "run fails over to the backup");
            assert_eq!(*health, NodeHealth::Ok);
        }
        other => panic!("expected Queued on backup, got {other:?}"),
    }
    assert_eq!(
        engine.run_status(outcome.run_id()).unwrap(),
        Some(ToolRunStatus::Queued),
        "failed-over run must be QUEUED on the backup, not WAITING_FOR_RUNNER"
    );
    assert_eq!(
        node_health_raw(&pool, "node-vm-p103", "health").as_deref(),
        Some("broken"),
        "primary heartbeat must land broken"
    );
    assert_eq!(
        node_health_raw(&pool, "node-host-p103", "health").as_deref(),
        Some("ok"),
        "backup heartbeat must land ok"
    );

    // 全候选离线：run 落 WAITING_FOR_RUNNER，挂主节点（§5.6:336 不 FAILED）。
    host_probe.set_healthy(false);
    let outcome = gateway.dispatch(&request("rn-p103-alldown")).unwrap();
    match &outcome {
        DispatchOutcome::WaitingForRunner {
            node_id, health, ..
        } => {
            assert_eq!(node_id, "node-vm-p103", "waiting run pins the primary");
            assert_eq!(*health, NodeHealth::Broken);
        }
        other => panic!("expected WaitingForRunner, got {other:?}"),
    }
    assert_eq!(
        engine.run_status(outcome.run_id()).unwrap(),
        Some(ToolRunStatus::WaitingForRunner),
        "all candidates offline must land WAITING_FOR_RUNNER, not FAILED"
    );

    // 全候选仍离线：恢复判定拒绝，run 保持 WAITING_FOR_RUNNER。
    let still_down = gateway.probe_and_resume(outcome.run_id());
    assert!(
        matches!(
            still_down,
            Err(runtime_gateway::GatewayError::RunnerUnavailable { ref node_id, .. })
                if node_id == "node-vm-p103"
        ),
        "resume with all candidates offline must be rejected: {still_down:?}"
    );
    assert_eq!(
        engine.run_status(outcome.run_id()).unwrap(),
        Some(ToolRunStatus::WaitingForRunner)
    );
}

#[test]
fn waiting_run_resumes_when_any_candidate_recovers() {
    let (gateway, engine, _pool, vm_probe, host_probe) = two_runner_env();
    vm_probe.set_healthy(false);
    host_probe.set_healthy(false);
    let outcome = gateway.dispatch(&request("rn-p103-resume")).unwrap();
    let run_id = outcome.run_id().to_string();
    assert!(matches!(outcome, DispatchOutcome::WaitingForRunner { .. }));

    // 备用节点恢复（主节点仍离线）：恢复判定经备用候选通过（第 4 行跃迁）。
    host_probe.set_healthy(true);
    let resumed = gateway.probe_and_resume(&run_id).unwrap();
    assert_eq!(
        resumed,
        ToolRunStatus::Preparing,
        "recovery goes through the recovered backup candidate"
    );
    assert_eq!(
        engine.run_status(&run_id).unwrap(),
        Some(ToolRunStatus::Preparing)
    );

    // 续跑到终态：轨迹含 WAITING_FOR_RUNNER → PREPARING（恢复续跑不丢任务）。
    let outcome = engine
        .drive_resumed(&run_id, &MockRunner::succeed(result_ok(), "p10-3 ok"))
        .unwrap();
    assert_eq!(
        outcome.transcript,
        vec![
            ToolRunStatus::WaitingForRunner,
            ToolRunStatus::Preparing,
            ToolRunStatus::Running,
            ToolRunStatus::Succeeded,
        ],
        "transcript must show the failover recovery trajectory"
    );
    assert_eq!(
        engine.run_status(&run_id).unwrap(),
        Some(ToolRunStatus::Succeeded)
    );
}

#[test]
fn recover_waiting_runs_resumes_all_and_reports_skips() {
    let (gateway, engine, _pool, vm_probe, host_probe) = two_runner_env();
    vm_probe.set_healthy(false);
    host_probe.set_healthy(false);
    let first = gateway.dispatch(&request("rn-p103-batch-1")).unwrap();
    let second = gateway.dispatch(&request("rn-p103-batch-2")).unwrap();
    assert!(matches!(first, DispatchOutcome::WaitingForRunner { .. }));
    assert!(matches!(second, DispatchOutcome::WaitingForRunner { .. }));

    // 全候选离线时批量恢复拒绝，不触碰任何 run。
    let rejected = gateway.recover_waiting_runs(10);
    assert!(
        matches!(
            rejected,
            Err(runtime_gateway::GatewayError::RunnerUnavailable { .. })
        ),
        "batch recovery with all candidates offline must be rejected: {rejected:?}"
    );
    assert_eq!(
        engine.run_status(first.run_id()).unwrap(),
        Some(ToolRunStatus::WaitingForRunner)
    );

    // 主节点恢复：批量恢复把两笔 run 全部带回 PREPARING（不丢任务）。
    vm_probe.set_healthy(true);
    let report = gateway.recover_waiting_runs(10).unwrap();
    assert_eq!(
        report.healthy_nodes,
        vec!["node-vm-p103"],
        "only the recovered primary is healthy in this batch"
    );
    assert_eq!(
        report.resumed_run_ids,
        vec![first.run_id().to_string(), second.run_id().to_string()],
        "both waiting runs resume in created_at order"
    );
    assert!(
        report.skipped_run_ids.is_empty(),
        "no concurrent drift in this scenario"
    );
    assert_eq!(
        engine.run_status(first.run_id()).unwrap(),
        Some(ToolRunStatus::Preparing)
    );
    assert_eq!(
        engine.run_status(second.run_id()).unwrap(),
        Some(ToolRunStatus::Preparing)
    );

    // 恢复后的 run 续跑（主节点承接）到终态。
    for run_id in [first.run_id(), second.run_id()] {
        let outcome = engine
            .drive_resumed(run_id, &MockRunner::succeed(result_ok(), "p10-3 batch ok"))
            .unwrap();
        assert_eq!(
            *outcome.transcript.last().unwrap(),
            ToolRunStatus::Succeeded
        );
    }

    // 再派发一笔落 WAITING 后由他方推进：批量恢复不再触碰已离开 WAITING
    // 的 run（skipped 面留给扫描与恢复之间的并发漂移，见 recover 文档）。
    vm_probe.set_healthy(false);
    host_probe.set_healthy(false);
    let drifting = gateway.dispatch(&request("rn-p103-batch-3")).unwrap();
    assert!(matches!(drifting, DispatchOutcome::WaitingForRunner { .. }));
    // 另一执行面直接把该 run 推回 PREPARING（模拟并发推进）。
    engine.resume_to_preparing(drifting.run_id()).unwrap();
    // 备用节点恢复后批量恢复：drifting 已不在 WAITING，resumed 不含它。
    host_probe.set_healthy(true);
    let report = gateway.recover_waiting_runs(10).unwrap();
    assert_eq!(report.healthy_nodes, vec!["node-host-p103"]);
    assert!(
        report.resumed_run_ids.is_empty(),
        "already-resumed run must not be double-moved: {:?}",
        report.resumed_run_ids
    );
    assert!(report.skipped_run_ids.is_empty());
    assert_eq!(
        engine.run_status(drifting.run_id()).unwrap(),
        Some(ToolRunStatus::Preparing),
        "recovery must not touch runs that already left WAITING_FOR_RUNNER"
    );
}

#[test]
fn set_primary_reorders_failover_priority() {
    let (gateway, _engine, _pool, vm_probe, _host_probe) = two_runner_env();
    // 主节点切到宿主面：候选优先序随之重排。
    gateway.set_primary("node-host-p103").unwrap();
    assert_eq!(
        gateway.candidate_nodes(),
        vec!["node-host-p103", "node-vm-p103"],
        "set_primary must move the node to the head of the candidate order"
    );
    vm_probe.set_healthy(false);
    let outcome = gateway.dispatch(&request("rn-p103-reprior")).unwrap();
    match &outcome {
        DispatchOutcome::Queued { node_id, .. } => {
            assert_eq!(
                node_id, "node-host-p103",
                "new primary takes the run; offline old primary is only a backup now"
            );
        }
        other => panic!("expected Queued on new primary, got {other:?}"),
    }
    // 未登记节点不可为主。
    let err = gateway.set_primary("ghost-node-p103").unwrap_err();
    assert!(
        matches!(err, runtime_gateway::GatewayError::UnknownNode(ref id) if id == "ghost-node-p103"),
        "unknown node cannot become primary: {err:?}"
    );
}

/// P10-3/P10-4 性能基准（本地执行，#[ignore] 不进常规测试流）：
/// Phase 2 派发延迟基准。measure.sh（.auto/measure.sh，autoresearch 冻结
/// 载具）以 `cargo test ... -- --ignored --nocapture bench_` 调用，本测试
/// 打印 `METRIC dispatch_ms=<中位数>`；基线与不回归门见
/// docs/phase10/PERF-BASELINE.md。内存库 + 内存迁移，无外部依赖。
#[test]
#[ignore]
fn bench_dispatch_latency() {
    let (gateway, _engine, _pool, _vm_probe, _host_probe) = two_runner_env();
    let iterations = 30;
    let mut samples_ms: Vec<f64> = Vec::with_capacity(iterations);
    for i in 0..iterations {
        let run_id = format!("rn-p103-bench-{i}");
        let start = std::time::Instant::now();
        let outcome = gateway.dispatch(&request(&run_id)).unwrap();
        let elapsed_ms = start.elapsed().as_secs_f64() * 1000.0;
        assert!(
            matches!(outcome, DispatchOutcome::Queued { .. }),
            "bench assumes healthy runners (run {run_id}): {outcome:?}"
        );
        samples_ms.push(elapsed_ms);
    }
    samples_ms.sort_by(|a, b| a.partial_cmp(b).expect("finite floats"));
    let median = samples_ms[samples_ms.len() / 2];
    println!("METRIC dispatch_ms={median:.3}");
    println!(
        "bench_dispatch_latency: n={} median_ms={median:.3} min_ms={:.3} max_ms={:.3}",
        iterations,
        samples_ms[0],
        samples_ms[iterations - 1]
    );
}
