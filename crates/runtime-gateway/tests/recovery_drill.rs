//! P2-8 验证载具（ROADMAP P2-8 的 podman_host 面）：故障恢复演练与阶段收口。
//!
//! 五项演练（判据见 docs/phase2/PHASE2-ACCEPTANCE.md，本文件为可重复执行的
//! 自动化载具；演练记录含命令原文与输出摘要落盘验收文档）：
//! 1. runtime 不可达（停心跳目标）后派发 run 停 WAITING_FOR_RUNNER，不卡死
//!    不误报 FAILED（计划 §5.6:336 原文语义）；
//! 2. 恢复后按 submit_key 幂等重放：单 run 行、无重复副作用（计划 §5.6:337-338）；
//! 3. 容器越界负向检查：挂载白名单外宿主路径不可达，platform*.db 与
//!    .rp-workbench-token 不出现在任何挂载面（计划 §14:772 验收第 3 条）；
//! 4. 磁盘自愈演练：悬空镜像/孤儿容器清理流程走通 + df 阈值告警判定（ADR §6）；
//! 5. 演练后临时容器与临时镜像清零。
//!
//! 门禁：容器相关演练 podman 不可用时跳过（V-9）；run 前缀 rip-tool-rn-rec-
//! 与既有批次（rn-p25-/rn-p27-）互不越界。

use database::{migrate, DbPath, Pool, PoolConfig};
use domain::{NodeHealth, RuntimeNode, RuntimeNodeKind, ToolRunStatus};
use runtime_gateway::{DispatchOutcome, Gateway, GatewayConfig, HealthStore, SwitchProbe};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tool_engine::adapter::{
    execute_container_via_engine, ContainerCall, NetworkPlan, PodmanHostAdapter, PodmanHostConfig,
};
use tool_engine::mock_runner::MockRunner;
use tool_engine::Engine;
use tool_registry::{Options, Registry};

/// 容器生成类演练串行执行（drill4 的清零断言与孤儿清理依赖独占容器面，
/// 并行会撞上 drill3 的活探针容器）。
static DRILL_LOCK: Mutex<()> = Mutex::new(());

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root")
}

fn manifest() -> Value {
    serde_json::from_str(
        &std::fs::read_to_string(
            repo_root().join("containerfiles/rip-tool-base/tool-manifest.json"),
        )
        .unwrap(),
    )
    .unwrap()
}

fn image_ref() -> String {
    manifest()["runtime"]["image"]
        .as_str()
        .expect("runtime.image")
        .to_string()
}

fn podman_available() -> bool {
    Command::new("podman")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// drill3 引用的是 tool-manifest.json 里按 digest 钉死的验收镜像
/// （localhost/rip-tool-base:smoke@sha256:...），镜像属运行时数据
/// （ADR §3.2），不在仓库内也无法现建命中同一 digest：只在已准备
/// 镜像的受控环境（本地验收）执行，否则按 V-9 跳过。
fn image_ready(image: &str) -> bool {
    Command::new("podman")
        .args(["image", "exists", image])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn fresh_env(tag: &str) -> (Pool, Engine, PathBuf) {
    let pool = Pool::open(PoolConfig {
        path: DbPath::Memory,
        readonly: false,
        max_connections: 1,
        busy_timeout: Duration::from_secs(5),
    })
    .unwrap();
    migrate(&pool, &repo_root().join("migrations")).unwrap();
    let registry = Registry::open(pool.clone()).with_options(Options {
        allow_placeholder_digest: false,
    });
    registry.register(&manifest()).unwrap();
    let engine = Engine::open(pool.clone(), registry);
    let ws_root = std::env::temp_dir().join(format!("rip-rec-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&ws_root);
    std::fs::create_dir_all(&ws_root).unwrap();
    (pool, engine, ws_root)
}

fn submit_request(run_id: &str, input: Value) -> Value {
    json!({
        "protocol_version": "1",
        "tool_id": "rip.smoke.native",
        "run_id": run_id,
        "submit_key": format!("rec-{run_id}"),
        "input": input,
        "workspace_id": "ws-rec",
        "budget": {"timeout_seconds": 60, "max_output_bytes": 1048576},
        "result_schema_version": "1"
    })
}

/// rip.smoke.native output_schema 的合法最小结果（MockRunner 续跑载具）。
fn smoke_schema_result() -> Value {
    json!({
        "echo": "recovery",
        "uid": 1000,
        "cap_eff": "0000000000000000",
        "tmpfs_writable": true,
        "rootfs_readonly": true
    })
}

// ---------------------------------------------------------------------------
// 演练 1 + 2：不可达 → WAITING_FOR_RUNNER；恢复 → submit_key 幂等重放。
// ---------------------------------------------------------------------------

#[test]
fn drill1_unreachable_lands_waiting_and_drill2_recovery_replays_once() {
    let (pool, engine, _ws) = fresh_env("state");
    let store = HealthStore::open(pool.clone());
    let gateway = Gateway::new(engine.clone(), store, GatewayConfig::default());
    let probe = SwitchProbe::new(false); // 停心跳目标：Runner 不可达
    gateway
        .register_node(
            RuntimeNode {
                node_id: "node-rec-1".to_string(),
                kind: RuntimeNodeKind::PodmanHost,
                endpoint: None,
                health: NodeHealth::Unknown,
                version: None,
            },
            probe.clone() as Arc<dyn runtime_gateway::HeartbeatProbe>,
        )
        .unwrap();

    // 演练 1：不可达期派发 → WAITING_FOR_RUNNER，不误报 FAILED。
    let started = Instant::now();
    let outcome = gateway
        .dispatch(&submit_request("rn-rec-wait", json!({"echo": "recovery"})))
        .unwrap();
    assert!(matches!(outcome, DispatchOutcome::WaitingForRunner { .. }));
    assert_eq!(
        engine.run_status(outcome.run_id()).unwrap(),
        Some(ToolRunStatus::WaitingForRunner),
        "run must land WAITING_FOR_RUNNER, never FAILED"
    );
    let runtime_node_health: String = pool
        .with_conn(|c| {
            c.query_row(
                "SELECT health FROM runtime_node WHERE node_id = 'node-rec-1'",
                [],
                |r| r.get(0),
            )
        })
        .unwrap();
    assert_eq!(
        runtime_node_health, "broken",
        "node health must record broken"
    );
    // 不卡死：恢复判定立即失败返回（RunnerUnavailable），run 原位保持。
    for _ in 0..2 {
        let resumed = gateway.probe_and_resume(outcome.run_id());
        assert!(
            matches!(
                resumed,
                Err(runtime_gateway::GatewayError::RunnerUnavailable { .. })
            ),
            "offline resume attempt must fail fast: {resumed:?}"
        );
        assert_eq!(
            engine.run_status(outcome.run_id()).unwrap(),
            Some(ToolRunStatus::WaitingForRunner),
            "run must stay put while unreachable"
        );
    }
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "offline drills must not hang"
    );

    // 演练 2：恢复后按 submit_key 幂等重放。派发器先按 submit_key 查既有
    // run（不新派发），恢复后续跑同一 run 行。
    let submit_key = format!("rec-{}", outcome.run_id());
    let found: Vec<String> = pool
        .with_conn(|c| {
            let mut stmt = c.prepare("SELECT run_id FROM tool_run WHERE submit_key = ?1")?;
            let rows = stmt
                .query_map([&submit_key], |r| r.get::<_, String>(0))?
                .filter_map(|r| r.ok())
                .collect();
            Ok::<Vec<_>, database::rusqlite::Error>(rows)
        })
        .unwrap();
    assert_eq!(
        found,
        vec![outcome.run_id().to_string()],
        "exactly one run row for the submit_key"
    );

    probe.set_healthy(true);
    let resumed = gateway.probe_and_resume(outcome.run_id()).unwrap();
    assert_eq!(
        resumed,
        ToolRunStatus::Preparing,
        "recovered run returns to PREPARING"
    );
    let drive = engine
        .drive_resumed(
            outcome.run_id(),
            &MockRunner::succeed(smoke_schema_result(), "recovery replay"),
        )
        .unwrap();
    assert_eq!(
        drive.transcript,
        vec![
            ToolRunStatus::WaitingForRunner,
            ToolRunStatus::Preparing,
            ToolRunStatus::Running,
            ToolRunStatus::Succeeded,
        ]
    );

    // 幂等核对：submit_key 仍只有一行，终态 SUCCEEDED，无重复副作用行。
    let (rows, status): (i64, String) = pool
        .with_conn(|c| {
            c.query_row(
                "SELECT COUNT(*), MAX(status) FROM tool_run WHERE submit_key = ?1",
                [&submit_key],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
        })
        .unwrap();
    assert_eq!(
        rows, 1,
        "idempotent replay must not create a duplicate run row"
    );
    assert_eq!(status, "SUCCEEDED");
    let replay_key: Option<String> = pool
        .with_conn(|c| {
            c.query_row(
                "SELECT replay_json FROM tool_run WHERE run_id = ?1",
                [outcome.run_id()],
                |r| r.get(0),
            )
        })
        .unwrap();
    let replay: Value = serde_json::from_str(&replay_key.unwrap()).unwrap();
    assert_eq!(
        replay["submit_key"],
        json!(submit_key),
        "submit_key carried through replay"
    );
    // 恢复后的二次恢复判定：终态不可迁出（NotStartable），无状态错乱。
    let again = gateway.probe_and_resume(outcome.run_id());
    assert!(
        matches!(again, Err(runtime_gateway::GatewayError::Engine(_))),
        "resume on terminal run must be rejected: {again:?}"
    );
    assert_eq!(
        engine.run_status(outcome.run_id()).unwrap(),
        Some(ToolRunStatus::Succeeded),
        "terminal state must be stable after drills"
    );
}

// ---------------------------------------------------------------------------
// 演练 3：容器越界负向检查（挂载白名单 + 敏感面零出现 + 白名单外不可达）。
// ---------------------------------------------------------------------------

#[test]
fn drill3_boundary_negative_mount_whitelist_and_sensitive_faces() {
    if !podman_available() {
        eprintln!("SKIP: podman unavailable (V-9，容器端到端保持本地执行)");
        return;
    }
    let image = image_ref();
    if !image_ready(&image) {
        eprintln!("SKIP: 验收镜像不在本地镜像库（{image}），digest 钉死无法现建命中（V-9，受控环境演练）");
        return;
    }
    let _drill = DRILL_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let (_pool, engine, ws_root) = fresh_env("boundary");
    let config = PodmanHostConfig {
        workspace_root: ws_root.clone(),
        ..PodmanHostConfig::default()
    };
    let adapter = PodmanHostAdapter::new(config);
    engine
        .submit(&json!({
            "protocol_version": "1",
            "tool_id": "rip.smoke.native",
            "run_id": "rn-rec-boundary",
            "submit_key": "rec-rn-rec-boundary",
            "input": {"echo": "boundary"},
            "workspace_id": "ws-rec",
            "budget": {"timeout_seconds": 60, "max_output_bytes": 1048576},
            "result_schema_version": "1"
        }))
        .unwrap();
    let outcome = execute_container_via_engine(ContainerCall {
        engine: &engine,
        run_id: "rn-rec-boundary",
        adapter: &adapter,
        image: &image,
        entrypoint: &["python3".to_string(), "/app/smoke_tool.py".to_string()],
        family: "utility",
        network: NetworkPlan::None,
    })
    .unwrap();
    assert_eq!(outcome.envelope.status, ToolRunStatus::Succeeded);

    // 挂载面白名单：以 Adapter 等价 flags 起长驻探针容器（--rm 冒烟容器
    // 秒退无法事后 inspect），bind 断言打在活容器上。
    let _probe_alive = Command::new("podman")
        .args([
            "run",
            "-d",
            "--name",
            "rip-tool-rn-rec-boundary-probe",
            "--label",
            "rip.runtime=rip-tool",
            "--network",
            "none",
            "--read-only",
            "--userns=keep-id",
            "--cap-drop",
            "ALL",
            "-v",
            &format!(
                "{}:/task/in:ro",
                ws_root.join("rn-rec-boundary/in").display()
            ),
            "-v",
            &format!(
                "{}:/task/out:rw",
                ws_root.join("rn-rec-boundary/out").display()
            ),
            "--tmpfs",
            "/tmp:rw,size=1g,nosuid,nodev",
            &image,
            "sleep",
            "30",
        ])
        .output()
        .expect("probe container spawn");
    let inspect = adapter.inspect("rip-tool-rn-rec-boundary-probe").unwrap();
    let binds = inspect["HostConfig"]["Binds"].as_array().unwrap().clone();
    assert_eq!(binds.len(), 2, "only task in/out mounts may exist");
    for b in &binds {
        let text = b.as_str().unwrap();
        assert!(
            text.starts_with(&ws_root.to_string_lossy().to_string()),
            "every mount source must be under the workspace: {text}"
        );
        for sensitive in [
            "platform.db",
            ".rp-workbench-token",
            "sock",
            "/var/run",
            "/run/podman",
        ] {
            assert!(
                !text.contains(sensitive),
                "sensitive face {sensitive:?} must never appear in mounts: {text}"
            );
        }
    }

    // 白名单外宿主路径在容器内不可达（命令原文见验收文档）。
    let probe_read = Command::new("podman")
        .args([
            "run",
            "--rm",
            "--network",
            "none",
            "--read-only",
            "--userns=keep-id",
            "--cap-drop",
            "ALL",
            "-v",
            &format!("{}:/task/in:ro", ws_root.join("rn-rec-boundary/in").display()),
            "-v",
            &format!("{}:/task/out:rw", ws_root.join("rn-rec-boundary/out").display()),
            &image,
            "sh",
            "-c",
            "ls /home/emo/migrations/platform.db /home/emo/.rp-workbench-token /run/podman/podman.sock /var/run/docker.sock 2>&1; echo PROBE_DONE",
        ])
        .output()
        .expect("podman run probe");
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&probe_read.stdout),
        String::from_utf8_lossy(&probe_read.stderr)
    );
    assert!(
        combined.contains("No such file or directory"),
        "whitelist-external host paths must be unreachable in-container: {combined}"
    );
    assert!(!combined.contains("PRAGMA"), "no database content may leak");
    // 只读输入挂载不可写。
    let probe_write = Command::new("podman")
        .args([
            "run",
            "--rm",
            "--network",
            "none",
            "--read-only",
            "--userns=keep-id",
            "--cap-drop",
            "ALL",
            "-v",
            &format!(
                "{}:/task/in:ro",
                ws_root.join("rn-rec-boundary/in").display()
            ),
            "-v",
            &format!(
                "{}:/task/out:rw",
                ws_root.join("rn-rec-boundary/out").display()
            ),
            &image,
            "sh",
            "-c",
            "touch /task/in/escape 2>&1; echo WPROBE_DONE",
        ])
        .output()
        .expect("podman run write probe");
    let write_out = format!(
        "{}{}",
        String::from_utf8_lossy(&probe_write.stdout),
        String::from_utf8_lossy(&probe_write.stderr)
    );
    assert!(
        write_out.contains("Read-only file system"),
        "write to read-only in/ must fail: {write_out}"
    );
    assert_eq!(
        engine.run_status("rn-rec-boundary").unwrap(),
        Some(ToolRunStatus::Succeeded)
    );
    let _ = Command::new("podman")
        .args(["rm", "-f", "rip-tool-rn-rec-boundary-probe"])
        .output();
    let _ = std::fs::remove_dir_all(&ws_root);
}

// ---------------------------------------------------------------------------
// 演练 4 + 5：磁盘自愈流程 + 演练后清零。
// ---------------------------------------------------------------------------

fn dangling_image_ids() -> Vec<String> {
    let out = Command::new("podman")
        .args(["images", "--filter", "dangling=true", "-q"])
        .output()
        .expect("podman images spawn");
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect()
}

fn root_avail_gb() -> i64 {
    let out = Command::new("df")
        .args(["-BG", "--output=avail", "/"])
        .output()
        .expect("df spawn");
    let text = String::from_utf8_lossy(&out.stdout);
    text.lines()
        .last()
        .unwrap_or("0G")
        .trim()
        .trim_end_matches('G')
        .parse()
        .unwrap_or(0)
}

#[test]
fn drill4_disk_self_heal_and_drill5_no_leftovers() {
    if !podman_available() {
        eprintln!("SKIP: podman unavailable (V-9，容器端到端保持本地执行)");
        return;
    }
    let _drill = DRILL_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let baseline_dangling = dangling_image_ids();
    let avail = root_avail_gb();
    // ADR §6 阈值判定：低于 50 GiB 触发自愈；高于则记录性通过。
    let heal_required = avail < 50;
    eprintln!(
        "DRILL4 df avail={avail}G threshold=50G heal_required={heal_required} dangling_baseline={}",
        baseline_dangling.len()
    );

    // 造一个真实悬空镜像（import 无 tag → <none>），验证清理流程真实生效。
    // Drop 守卫保证断言失败也不泄漏演练镜像。
    struct DrillImage(String);
    impl Drop for DrillImage {
        fn drop(&mut self) {
            let _ = Command::new("podman").args(["rmi", "-f", &self.0]).output();
        }
    }
    let created = Command::new("sh")
        .arg("-c")
        .arg("d=$(mktemp -d) && tar -C \"$d\" -cf - . | podman import - && rm -rf \"$d\"")
        .output()
        .expect("podman import spawn");
    assert!(
        created.status.success(),
        "import dangling probe: {}",
        String::from_utf8_lossy(&created.stderr)
    );
    // import 输出 sha256:<64hex> 全引称；dangling -q 返回 12 位短 id，
    // 归一化到完整 hex 后按前缀互相比对。
    let raw = String::from_utf8_lossy(&created.stdout).trim().to_string();
    let new_full = raw.strip_prefix("sha256:").unwrap_or(&raw).to_string();
    assert_eq!(new_full.len(), 64, "import must yield a 64-hex image id");
    let new_short = &new_full[..12];
    let _drill_image = DrillImage(new_full.clone());
    let after_import = dangling_image_ids();
    assert!(
        after_import.len() == baseline_dangling.len() + 1
            && after_import
                .iter()
                .any(|id| new_full.starts_with(id.as_str()) || id.starts_with(new_short)),
        "drill dangling image must be visible to the heal sweep: new={new_full} list={after_import:?}"
    );

    // 自愈流程：列出悬空镜像与孤儿容器（rip-tool label）→ 清理 → 记录告警面。
    let orphans: Vec<String> = {
        let out = Command::new("podman")
            .args(["ps", "-aq", "--filter", "label=rip.runtime=rip-tool"])
            .output()
            .unwrap();
        String::from_utf8_lossy(&out.stdout)
            .lines()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .collect()
    };
    // 演练面限定：只移除本演练创建的悬空镜像（生产策略清全部悬空，
    // 见 PHASE2-ACCEPTANCE.md 演练 4 记录），孤儿容器按 label 清零。
    let _ = Command::new("podman")
        .args(["rmi", "-f", &new_full])
        .output()
        .unwrap();
    for o in &orphans {
        let _ = Command::new("podman")
            .args(["rm", "-f", o])
            .output()
            .unwrap();
    }
    assert_eq!(
        dangling_image_ids().len(),
        baseline_dangling.len(),
        "heal must restore the dangling baseline count"
    );
    eprintln!(
        "DRILL4 healed: dangling_cleaned=[{new_full}] orphans_cleaned={} df_after={}G",
        orphans.len(),
        root_avail_gb()
    );

    // 演练 5：演练后临时容器与临时镜像清零（本批前缀面）。
    let out = Command::new("podman")
        .args([
            "ps",
            "-aq",
            "--filter",
            "label=rip.runtime=rip-tool",
            "--format",
            "{{.Names}}",
        ])
        .output()
        .unwrap();
    let leftover: Vec<String> = String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| l.starts_with("rip-tool-rn-rec-"))
        .collect();
    assert!(
        leftover.is_empty(),
        "no drill containers may remain: {leftover:?}"
    );
}
