//! P2-5 验证载具（ROADMAP P2-5 判据）：podman_host Runtime Adapter 经
//! tool-engine 派发链端到端。契约测试载具经 tool-engine 派发链发起（完整
//! HTTP API 面属 P5-4 不在本阶段）。
//!
//! 判据（批次自验清单）：
//! 1. 冒烟工具容器 run 到 SUCCEEDED（D-TC-03 帧）；
//! 2. podman inspect 复核 §5.4 清单逐条生效（ReadOnlyRootfs、User 非 root、
//!    挂载仅 workspaces/<run_id>/in(ro)+out(rw)、无 Podman/Docker socket、
//!    非 privileged、capabilities 收敛、PidsLimit/Cpu/Memory 上限、镜像
//!    @sha256:64hex digest 固定）；
//! 3. out/ 产物 sha256 与 artifact 表对账一致且属主为宿主用户
//!    （--userns=keep-id，消解 ADR 待验证项 5）；
//! 4. 超时与协作式取消（cancel_requested_at 语义，D-TC-04）各一用例；
//! 5. run 结束后临时容器清零（label 过滤）与 tmp/ 删除。
//!
//! 门禁：本测试属容器端到端，按 ROADMAP §3.5 风险 5（CI podman 可用性
//! V-9 待验证）保持本地执行：podman 不可用时打印跳过说明直接返回，
//! CI（无 podman）保持全绿。镜像缺失时经 Containerfile 构建（镜像属
//! 运行时数据，ADR §3.2 口径，宿主机零安装）。

use database::{migrate, DbPath, Pool, PoolConfig};
use domain::ToolRunStatus;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tool_engine::adapter::{
    execute_container_via_engine, ContainerCall, PodmanHostAdapter, PodmanHostConfig,
};
use tool_engine::Engine;
use tool_registry::{Options, Registry};

const IMAGE_TAG: &str = "localhost/rip-tool-base:smoke";
/// tool-manifest.yaml 的 json 孪生（仓库 json/yaml 双份惯例）。
const MANIFEST_DIGEST: &str = "84877bf60e8a768c08f5b7a9b334eac48231b70f8b1c248008723590f79dc81a";

static IMAGE_LOCK: Mutex<()> = Mutex::new(());
/// 容器生成类用例串行执行（run 结束清零断言与按名等待都依赖独占容器面）。
static E2E_LOCK: Mutex<()> = Mutex::new(());

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root")
}

fn podman_available() -> bool {
    Command::new("podman")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// 镜像就绪（并发测试共用一把锁；缺失时按 Containerfile 构建）。
fn ensure_image() -> String {
    let _guard = IMAGE_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let exists = Command::new("podman")
        .args(["image", "exists", IMAGE_TAG])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    if !exists {
        let dir = repo_root().join("containerfiles/rip-tool-base");
        let out = Command::new("podman")
            .args([
                "build",
                "-t",
                IMAGE_TAG,
                "-f",
                &dir.join("Containerfile").to_string_lossy(),
                &dir.to_string_lossy(),
            ])
            .output()
            .expect("podman build spawn");
        assert!(
            out.status.success(),
            "podman build failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    let digest = Command::new("podman")
        .args(["inspect", "--format", "{{.Digest}}", IMAGE_TAG])
        .output()
        .expect("podman inspect spawn");
    let text = String::from_utf8_lossy(&digest.stdout).trim().to_string();
    assert_eq!(
        text,
        format!("sha256:{MANIFEST_DIGEST}"),
        "local image digest must match tool-manifest digest pinning"
    );
    format!("{IMAGE_TAG}@sha256:{MANIFEST_DIGEST}")
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
    let manifest: Value = serde_json::from_str(
        &std::fs::read_to_string(
            repo_root().join("containerfiles/rip-tool-base/tool-manifest.json"),
        )
        .unwrap(),
    )
    .unwrap();
    registry.register(&manifest).unwrap();
    let engine = Engine::open(pool.clone(), registry);
    let ws_root = std::env::temp_dir().join(format!("rip-p25-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&ws_root);
    std::fs::create_dir_all(&ws_root).unwrap();
    (pool, engine, ws_root)
}

fn submit(engine: &Engine, run_id: &str, input: Value, timeout_seconds: u64) {
    engine
        .submit(&json!({
            "protocol_version": "1",
            "tool_id": "rip.smoke.native",
            "run_id": run_id,
            "submit_key": format!("p25-{run_id}"),
            "input": input,
            "workspace_id": "ws-p25",
            "budget": {"timeout_seconds": timeout_seconds, "max_output_bytes": 1048576},
            "result_schema_version": "1"
        }))
        .expect("submit accepted");
}

fn rip_containers() -> Vec<String> {
    let out = Command::new("podman")
        .args([
            "ps",
            "-a",
            "--filter",
            "label=rip.runtime=rip-tool",
            "--format",
            "{{.Names}}",
        ])
        .output()
        .expect("podman ps spawn");
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| l.starts_with("rip-tool-rn-p25-"))
        .collect()
}

/// 启动清理：强清上轮失败运行遗留的 rip-tool 容器（同名冲突与陈旧
/// inspect 的污染源；只动 label=rip.runtime=rip-tool 自有容器）。
fn force_cleanup_rip_containers() {
    let _ = Command::new("podman")
        .args([
            "ps",
            "-aq",
            "--filter",
            "label=rip.runtime=rip-tool",
            "--format",
            "{{.Names}}",
        ])
        .output()
        .map(|o| {
            let names = String::from_utf8_lossy(&o.stdout);
            for name in names
                .lines()
                .map(|l| l.trim())
                .filter(|l| l.starts_with("rip-tool-rn-p25-"))
            {
                let _ = Command::new("podman").args(["rm", "-f", name]).status();
            }
        });
}

fn wait_container_visible(expected: &str, timeout: Duration) -> Option<String> {
    let deadline = Instant::now() + timeout;
    loop {
        if rip_containers_running().iter().any(|n| n == expected) {
            return Some(expected.to_string());
        }
        if Instant::now() >= deadline {
            return None;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}

fn rip_containers_running() -> Vec<String> {
    let out = Command::new("podman")
        .args([
            "ps",
            "--filter",
            "label=rip.runtime=rip-tool",
            "--format",
            "{{.Names}}",
        ])
        .output()
        .expect("podman ps spawn");
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| l.starts_with("rip-tool-rn-p25-"))
        .collect()
}

fn file_uid(path: &Path) -> u32 {
    use std::os::unix::fs::MetadataExt;
    std::fs::metadata(path).unwrap().uid()
}

#[test]
fn smoke_run_succeeds_with_keep_id_and_artifacts() {
    let _e2e = E2E_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    force_cleanup_rip_containers();
    if !podman_available() {
        eprintln!("SKIP: podman unavailable (V-9，容器端到端保持本地执行)");
        return;
    }
    let image = ensure_image();
    let (_pool, engine, ws_root) = fresh_env("ok");
    let config = PodmanHostConfig {
        workspace_root: ws_root.clone(),
        ..PodmanHostConfig::default()
    };
    let adapter = PodmanHostAdapter::new(config);
    submit(&engine, "rn-p25-ok", json!({"echo": "hello-rip"}), 60);

    let outcome = execute_container_via_engine(ContainerCall {
        engine: &engine,
        run_id: "rn-p25-ok",
        adapter: &adapter,
        image: &image,
        entrypoint: &["python3".to_string(), "/app/smoke_tool.py".to_string()],
        family: "utility",
    })
    .expect("container e2e run");
    let env = &outcome.envelope;
    assert_eq!(
        env.status,
        ToolRunStatus::Succeeded,
        "smoke run must reach SUCCEEDED: {:?}",
        env.error
    );
    assert_eq!(
        outcome.transcript,
        vec![
            ToolRunStatus::Queued,
            ToolRunStatus::Preparing,
            ToolRunStatus::Running,
            ToolRunStatus::Succeeded,
        ]
    );
    assert_eq!(env.environment.runtime_type, "podman_host");

    // 容器内自证（§5.4 的容器视角）：非 root、capabilities 收敛、只读根、tmpfs。
    let data = env.result.as_ref().expect("result data");
    assert!(
        data["uid"].as_u64().expect("uid") != 0,
        "container uid must be non-root"
    );
    assert_eq!(
        data["cap_eff"].as_str().expect("cap_eff"),
        "0000000000000000",
        "--cap-drop ALL must zero CapEff"
    );
    assert_eq!(data["tmpfs_writable"].as_bool(), Some(true));
    assert_eq!(data["rootfs_readonly"].as_bool(), Some(true));

    // out/ 产物 sha256 与 artifact 表对账一致 + keep-id 属主（宿主用户）。
    let artifacts = &env.artifacts;
    assert!(!artifacts.is_empty(), "out/ artifacts must be collected");
    let result_path = ws_root.join("rn-p25-ok/out/result.json");
    assert!(result_path.exists(), "out/result.json must exist");
    let bytes = std::fs::read(&result_path).unwrap();
    let primary = artifacts
        .iter()
        .find(|a| a.uri.ends_with("result.json"))
        .expect("result.json artifact");
    assert_eq!(primary.sha256, tool_registry::hash::sha256_hex(&bytes));
    assert_eq!(primary.size_bytes as usize, bytes.len());
    assert_eq!(primary.role, "primary");
    let rows: Vec<(String, String, i64)> = engine
        .pool()
        .with_conn(|c| {
            let mut stmt = c.prepare(
                "SELECT role, sha256, size_bytes FROM artifact WHERE run_id = 'rn-p25-ok'",
            )?;
            let rows = stmt
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
                .filter_map(|r| r.ok())
                .collect();
            Ok::<Vec<_>, database::rusqlite::Error>(rows)
        })
        .unwrap();
    assert!(
        rows.iter().any(|(role, sha, size)| role == "primary"
            && sha == &tool_registry::hash::sha256_hex(&bytes)
            && *size == bytes.len() as i64),
        "artifact table must reconcile with out/ file sha256: {rows:?}"
    );
    // keep-id：容器内 uid == 产物属主 == 宿主目录属主（同一宿主用户）。
    assert_eq!(
        file_uid(&result_path),
        file_uid(&ws_root),
        "artifact file must be owned by the host user (--userns=keep-id, ADR 待验证项 5)"
    );
    assert_eq!(
        data["uid"].as_u64().unwrap() as u32,
        file_uid(&ws_root),
        "container uid must equal host owner uid"
    );

    // run 结束后临时容器清零 + tmp/ 删除。
    assert!(
        rip_containers().is_empty(),
        "no rip-tool containers may remain: {riptools:?}",
        riptools = rip_containers()
    );
    assert!(
        !ws_root.join("rn-p25-ok/tmp").exists(),
        "tmp/ must be removed"
    );
    let _ = std::fs::remove_dir_all(&ws_root);
}

#[test]
fn inspect_constraints_and_cooperative_cancel() {
    let _e2e = E2E_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    force_cleanup_rip_containers();
    if !podman_available() {
        eprintln!("SKIP: podman unavailable (V-9，容器端到端保持本地执行)");
        return;
    }
    let image = ensure_image();
    let (_pool, engine, ws_root) = fresh_env("cancel");
    let config = PodmanHostConfig {
        workspace_root: ws_root.clone(),
        ..PodmanHostConfig::default()
    };
    let adapter = PodmanHostAdapter::new(config);
    submit(
        &engine,
        "rn-p25-cancel",
        json!({"echo": "cancel-me", "sleep_ms": 60000}),
        600,
    );

    std::thread::scope(|s| {
        let runner = s.spawn(|| {
            execute_container_via_engine(ContainerCall {
                engine: &engine,
                run_id: "rn-p25-cancel",
                adapter: &adapter,
                image: &image,
                entrypoint: &["python3".to_string(), "/app/smoke_tool.py".to_string()],
                family: "utility",
            })
        });

        // 容器可见后做 §5.4 inspect 逐条复核（podman 6.1.4 字段路径实测）。
        let name = wait_container_visible("rip-tool-rn-p25-cancel", Duration::from_secs(30))
            .expect("container must become visible");
        let inspect = adapter.inspect(&name).expect("inspect");
        let hc = &inspect["HostConfig"];
        assert_eq!(
            hc["ReadonlyRootfs"].as_bool(),
            Some(true),
            "rootfs read-only"
        );
        assert_eq!(hc["Privileged"].as_bool(), Some(false), "not privileged");
        assert_eq!(
            hc["CapAdd"].as_array().map(Vec::len),
            Some(0),
            "no added capabilities"
        );
        assert_eq!(hc["PidsLimit"].as_i64(), Some(256), "pids limit");
        assert_eq!(
            hc["Memory"].as_u64(),
            Some(2 * 1024 * 1024 * 1024),
            "memory cap"
        );
        assert_eq!(
            hc["NanoCpus"].as_u64(),
            Some(2_000_000_000),
            "cpu cap (--cpus 2)"
        );
        assert_eq!(
            hc["NetworkMode"].as_str(),
            Some("none"),
            "default no network"
        );
        assert_eq!(
            hc["PortBindings"].as_object().map(|m| m.len()),
            Some(0),
            "no published ports"
        );
        assert_eq!(
            hc["Annotations"]["io.podman.annotations.userns"].as_str(),
            Some("keep-id"),
            "userns keep-id annotation"
        );
        let user = inspect["Config"]["User"].as_str().expect("Config.User");
        assert!(
            user != "0" && user != "root" && !user.starts_with("0:"),
            "container user must be non-root: {user}"
        );
        let binds = hc["Binds"].as_array().expect("binds");
        assert_eq!(binds.len(), 2, "only in/out task mounts allowed");
        let in_bind = binds
            .iter()
            .find(|b| b.as_str().unwrap_or("").contains(":/task/in:"))
            .expect("in mount");
        let out_bind = binds
            .iter()
            .find(|b| b.as_str().unwrap_or("").contains(":/task/out:"))
            .expect("out mount");
        assert!(
            in_bind.as_str().unwrap().starts_with(
                &ws_root
                    .join("rn-p25-cancel/in")
                    .to_string_lossy()
                    .to_string()
            ) && in_bind.as_str().unwrap().contains(":ro"),
            "in mount must be workspace in/ read-only: {in_bind}"
        );
        assert!(
            out_bind.as_str().unwrap().starts_with(
                &ws_root
                    .join("rn-p25-cancel/out")
                    .to_string_lossy()
                    .to_string()
            ) && out_bind.as_str().unwrap().contains(":rw"),
            "out mount must be workspace out/ read-write: {out_bind}"
        );
        for b in binds {
            let text = b.as_str().unwrap();
            assert!(
                !text.contains("sock")
                    && !text.contains("/var/run")
                    && !text.contains("/run/podman"),
                "no podman/docker socket mounts allowed: {text}"
            );
        }
        let tmpfs = hc["Tmpfs"].as_object().expect("tmpfs map");
        assert!(
            tmpfs
                .get("/tmp")
                .map(|v| v.as_str().unwrap_or("").contains("size=1g"))
                == Some(true),
            "tmpfs /tmp must be capped at 1g: {tmpfs:?}"
        );

        // 协作式取消（D-TC-04）：cancel_requested_at 写入 → 适配器观测 → 终止。
        assert!(engine.request_cancel("rn-p25-cancel").unwrap());
        let outcome = runner
            .join()
            .expect("runner thread")
            .expect("cancelled run");
        let env = &outcome.envelope;
        assert_eq!(
            env.status,
            ToolRunStatus::Cancelled,
            "cooperative cancel lands CANCELLED"
        );
        assert_eq!(
            env.error.as_ref().map(|e| e.category.as_str()),
            Some("CANCELLED")
        );
        assert_eq!(
            env.error.as_ref().and_then(|e| e.code.as_i64()),
            Some(-32004)
        );
    });

    let cancel_at: Option<String> = engine
        .pool()
        .with_conn(|c| {
            c.query_row(
                "SELECT cancel_requested_at FROM tool_run WHERE run_id = 'rn-p25-cancel'",
                [],
                |r| r.get(0),
            )
        })
        .unwrap();
    assert!(
        cancel_at.is_some(),
        "cancel_requested_at must be recorded (D-TC-04)"
    );
    assert_eq!(
        engine.run_status("rn-p25-cancel").unwrap(),
        Some(ToolRunStatus::Cancelled)
    );
    assert!(
        rip_containers().is_empty(),
        "container must be reclaimed after cancel"
    );
    assert!(
        !ws_root.join("rn-p25-cancel/tmp").exists(),
        "tmp/ must be removed"
    );
    let _ = std::fs::remove_dir_all(&ws_root);
}

#[test]
fn budget_timeout_terminates_container() {
    let _e2e = E2E_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    force_cleanup_rip_containers();
    if !podman_available() {
        eprintln!("SKIP: podman unavailable (V-9，容器端到端保持本地执行)");
        return;
    }
    let image = ensure_image();
    let (_pool, engine, ws_root) = fresh_env("timeout");
    let config = PodmanHostConfig {
        workspace_root: ws_root.clone(),
        ..PodmanHostConfig::default()
    };
    let adapter = PodmanHostAdapter::new(config);
    submit(
        &engine,
        "rn-p25-timeout",
        json!({"echo": "timeout-me", "sleep_ms": 60000}),
        1,
    );
    let started = Instant::now();
    let outcome = execute_container_via_engine(ContainerCall {
        engine: &engine,
        run_id: "rn-p25-timeout",
        adapter: &adapter,
        image: &image,
        entrypoint: &["python3".to_string(), "/app/smoke_tool.py".to_string()],
        family: "utility",
    })
    .expect("timeout run");
    assert!(
        started.elapsed() < Duration::from_secs(15),
        "watchdog must fire promptly at 1s budget"
    );
    assert_eq!(outcome.envelope.status, ToolRunStatus::TimedOut);
    assert_eq!(
        outcome.envelope.error.as_ref().map(|e| e.category.as_str()),
        Some("TIMEOUT")
    );
    assert_eq!(
        engine.run_status("rn-p25-timeout").unwrap(),
        Some(ToolRunStatus::TimedOut)
    );
    assert!(
        rip_containers().is_empty(),
        "container must be reclaimed after timeout"
    );
    assert!(
        !ws_root.join("rn-p25-timeout/tmp").exists(),
        "tmp/ must be removed"
    );
    let _ = std::fs::remove_dir_all(&ws_root);
}

#[test]
fn tag_ref_image_rejected_in_preparing() {
    // 无需 podman（引用校验先于镜像探测）；验 Adapter 侧 digest 二道校验。
    let (_pool, engine, ws_root) = fresh_env("tagref");
    let config = PodmanHostConfig {
        workspace_root: ws_root.clone(),
        ..PodmanHostConfig::default()
    };
    let adapter = PodmanHostAdapter::new(config);
    submit(&engine, "rn-p25-tagref", json!({"echo": "x"}), 60);
    let outcome = execute_container_via_engine(ContainerCall {
        engine: &engine,
        run_id: "rn-p25-tagref",
        adapter: &adapter,
        image: "localhost/rip-tool-base:smoke",
        entrypoint: &["python3".to_string(), "/app/smoke_tool.py".to_string()],
        family: "utility",
    })
    .expect("tag-ref run must be handled, not panic");
    assert_eq!(outcome.envelope.status, ToolRunStatus::Failed);
    assert_eq!(
        outcome.envelope.error.as_ref().map(|e| e.category.as_str()),
        Some("DEPENDENCY"),
        "tag reference rejected in PREPARING (rule 4 double check)"
    );
    let _ = std::fs::remove_dir_all(&ws_root);
}

#[test]
fn adapter_unit_helpers() {
    assert!(PodmanHostAdapter::image_ref_ok(
        "localhost/rip-tool-base:smoke@sha256:84877bf60e8a768c08f5b7a9b334eac48231b70f8b1c248008723590f79dc81a"
    ));
    assert!(!PodmanHostAdapter::image_ref_ok(
        "localhost/rip-tool-base:smoke"
    ));
    assert!(!PodmanHostAdapter::image_ref_ok("img@sha256:<digest>"));
    assert!(!PodmanHostAdapter::image_ref_ok("img@sha256:ABC"));
    let cfg = PodmanHostConfig::default();
    let adapter = PodmanHostAdapter::new(cfg);
    assert_eq!(
        adapter.container_name("rn-p25/ok:1"),
        "rip-tool-rn-p25-ok-1"
    );
    assert_eq!(
        adapter.container_name("run-with-very-long-identifier-abcdefghijklmnop"),
        "rip-tool-run-with-very-long-identifier-abcdefghij"
    );
}
