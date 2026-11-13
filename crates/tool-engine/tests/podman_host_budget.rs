//! P2-7 验证载具（ROADMAP P2-7 的 podman_host 面）：镜像缓存命中、
//! 资源限制基线（ADR §7 表逐项）、超 budget 强制取消产 partial Artifact。
//!
//! 判据对应：
//! 1. 同一镜像第二次派发不再拉取或重建：podman 调用经 wrapper 脚本记账，
//!    两次派发的子命令日志含两次 run 与 image exists，零 pull/build
//!    （Adapter 缓存策略：只探本地 store，从不 pull/build）；
//! 2. 超 budget 用例：run 强制取消（TIMED_OUT）、已产出按 partial 登记
//!    （provenance 语义，ADR §5 第 3 条）、tmp/ 清理；
//! 3. 资源基线与 ADR §7 表逐项一致并被 adapter 注入 podman 参数
//!    （注入断言见 podman_host_e2e inspect 用例；本文件单测基线常量 +
//!    提交门帽 600000 ms / 64 MiB）；
//! 4. 每工具族并发默认 1（FamilyLimiter 单测）。
//!
//! 门禁：容器用例 podman 不可用时跳过（V-9，ROADMAP §3.5 风险 5）；
//! run 前缀 rip-tool-rn-p27- 与 e2e（rn-p25-）互不越界。

use database::{migrate, DbPath, Pool, PoolConfig};
use domain::ToolRunStatus;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tool_engine::adapter::{
    execute_container_via_engine, ContainerCall, FamilyLimiter, NetworkPlan, PodmanHostAdapter,
    PodmanHostConfig,
};
use tool_engine::Engine;
use tool_registry::{Options, Registry};

static IMAGE_LOCK: Mutex<()> = Mutex::new(());
static E2E_LOCK: Mutex<()> = Mutex::new(());

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

/// 镜像引用（digest 取自 manifest json，避免测试侧再维护一份常量）。
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

/// 镜像就绪（并发测试共用一把锁）。引用取自 manifest digest 钉死，镜像属
/// 运行时数据（ADR §3.2），不在仓库内也无法现建命中同一 digest：只在已
/// 准备镜像的受控环境执行，否则按 V-9 跳过。
fn ensure_image() -> Option<()> {
    let _guard = IMAGE_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let image = image_ref();
    let exists = Command::new("podman")
        .args(["image", "exists", &image])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    if !exists {
        eprintln!("SKIP: 验收镜像 {image} 不在本地镜像库，digest 钉死无法现建命中（V-9，受控环境端到端）");
        return None;
    }
    Some(())
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
    let ws_root = std::env::temp_dir().join(format!("rip-p27-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&ws_root);
    std::fs::create_dir_all(&ws_root).unwrap();
    (pool, engine, ws_root)
}

fn submit(engine: &Engine, run_id: &str, input: Value, budget: Value) {
    engine
        .submit(&json!({
            "protocol_version": "1",
            "tool_id": "rip.smoke.native",
            "run_id": run_id,
            "submit_key": format!("p27-{run_id}"),
            "input": input,
            "workspace_id": "ws-p27",
            "budget": budget,
            "result_schema_version": "1"
        }))
        .expect("submit accepted");
}

/// 本文件前缀的 rip-tool 容器（跨测试二进制并行互不越界）。
fn p27_containers(running_only: bool) -> Vec<String> {
    let mut args = vec!["ps"];
    if !running_only {
        args.push("-a");
    }
    args.extend([
        "--filter",
        "label=rip.runtime=rip-tool",
        "--format",
        "{{.Names}}",
    ]);
    let out = Command::new("podman")
        .args(&args)
        .output()
        .expect("podman ps");
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| l.starts_with("rip-tool-rn-p27-"))
        .collect()
}

/// podman 调用记账 wrapper：每个子命令首词落日志后转发真身。
fn make_podman_wrapper(dir: &Path) -> PathBuf {
    let real = String::from_utf8_lossy(
        &Command::new("sh")
            .args(["-c", "command -v podman"])
            .output()
            .expect("command -v podman")
            .stdout,
    )
    .trim()
    .to_string();
    assert!(
        !real.is_empty(),
        "podman must be on PATH for wrapper target"
    );
    let wrapper = dir.join("podman-wrapper.sh");
    std::fs::write(
        &wrapper,
        format!(
            "#!/bin/sh\nprintf '%s\\n' \"$1\" >> {}/invocations.log\nexec {} \"$@\"\n",
            dir.display(),
            real
        ),
    )
    .unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&wrapper, std::fs::Permissions::from_mode(0o755)).unwrap();
    wrapper
}

fn invocation_log(dir: &Path) -> Vec<String> {
    std::fs::read_to_string(dir.join("invocations.log"))
        .unwrap_or_default()
        .lines()
        .map(|l| l.trim().to_string())
        .collect()
}

#[test]
fn adapter_config_defaults_match_adr7_baseline() {
    let cfg = PodmanHostConfig::default();
    assert_eq!(cfg.cpus, 2.0, "ADR 7: CPU 每容器 2 核");
    assert_eq!(
        cfg.memory_bytes,
        2 * 1024 * 1024 * 1024,
        "ADR 7: 内存 2 GiB"
    );
    assert_eq!(cfg.pids_limit, 256, "ADR 7: PIDs 256");
    assert_eq!(cfg.tmpfs_size, "1g", "ADR 7: 任务 tmp tmpfs 封顶 1 GiB");
    assert_eq!(cfg.family_concurrency_default, 1, "ADR 7: 每工具族默认 1");
    assert_eq!(
        cfg.workspace_root,
        Path::new(&std::env::var("HOME").unwrap()).join("rip-runtime/workspaces"),
        "ADR 5: 工作区根 ~/rip-runtime/workspaces"
    );
}

#[test]
fn submit_gates_cap_timeout_600s_and_output_64mib() {
    // 时长上限 600000 ms（ADR 7 执行时长行；Manifest cap 600s 由 Registry
    // timeout_s 解析，提交超帽 D-TC-05 拒绝）。
    let (_pool, engine, ws_root) = fresh_env("gates");
    let over_timeout = engine.submit(&json!({
        "protocol_version": "1",
        "tool_id": "rip.smoke.native",
        "run_id": "rn-p27-cap-t",
        "submit_key": "p27-cap-t",
        "input": {"echo": "x"},
        "workspace_id": "ws-p27",
        "budget": {"timeout_seconds": 601, "max_output_bytes": 1048576},
        "result_schema_version": "1"
    }));
    let err = over_timeout.unwrap_err();
    assert!(
        err.to_string().contains("601 exceeds manifest cap 600"),
        "timeout above 600s cap must be rejected: {err}"
    );
    // 输出 64 MiB 截断信封（ADR 7 输出大小行；worker protocol 域帽 67108864）。
    let over_output = engine.submit(&json!({
        "protocol_version": "1",
        "tool_id": "rip.smoke.native",
        "run_id": "rn-p27-cap-o",
        "submit_key": "p27-cap-o",
        "input": {"echo": "x"},
        "workspace_id": "ws-p27",
        "budget": {"timeout_seconds": 60, "max_output_bytes": 67108865},
        "result_schema_version": "1"
    }));
    let err = over_output.unwrap_err();
    assert!(
        err.to_string().contains("67108865 outside [256, 67108864]"),
        "output above 64 MiB cap must be rejected: {err}"
    );
    let _ = std::fs::remove_dir_all(&ws_root);
}

#[test]
fn family_limiter_defaults_to_one_slot() {
    let limiter = FamilyLimiter::new(1);
    let g1 = limiter.try_acquire("utility").expect("first slot");
    assert!(
        limiter.try_acquire("utility").is_none(),
        "second acquire on same family must be blocked (default 1)"
    );
    assert!(
        limiter.try_acquire("web_http").is_some(),
        "different family is a separate slot"
    );
    drop(g1);
    assert!(
        limiter.try_acquire("utility").is_some(),
        "slot must be free after guard drop"
    );
    // 收紧到 2 的族可并行两个。
    let limiter2 = FamilyLimiter::new(2);
    let a = limiter2.try_acquire("utility");
    let b = limiter2.try_acquire("utility");
    assert!(
        a.is_some() && b.is_some(),
        "limit 2 admits two concurrent holds"
    );
    assert!(limiter2.try_acquire("utility").is_none());
    // 阻塞 acquire 在槽位释放后可进入（子线程持有 300ms 后释放）。
    let limiter3 = FamilyLimiter::new(1);
    let holder = limiter3.try_acquire("utility").unwrap();
    let waiter = limiter3.clone();
    let t = std::thread::spawn(move || waiter.acquire("utility"));
    std::thread::sleep(Duration::from_millis(300));
    drop(holder);
    assert!(
        t.join().is_ok(),
        "blocking acquire must proceed after release"
    );
    let _ = limiter2.try_acquire("utility");
    let _ = a;
    let _ = b;
}

#[test]
fn same_image_second_dispatch_hits_cache() {
    if !podman_available() {
        eprintln!("SKIP: podman unavailable (V-9，容器端到端保持本地执行)");
        return;
    }
    let _e2e = E2E_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    if ensure_image().is_none() {
        return;
    }
    let (_pool, engine, ws_root) = fresh_env("cache");
    let wrapper = make_podman_wrapper(&ws_root);
    let config = PodmanHostConfig {
        workspace_root: ws_root.clone(),
        podman_bin: wrapper.to_string_lossy().to_string(),
        ..PodmanHostConfig::default()
    };
    let adapter = PodmanHostAdapter::new(config);
    let image = image_ref();

    for run_id in ["rn-p27-cache-1", "rn-p27-cache-2"] {
        submit(
            &engine,
            run_id,
            json!({"echo": "cache-probe"}),
            json!({"timeout_seconds": 60, "max_output_bytes": 1048576}),
        );
        let started = Instant::now();
        let outcome = execute_container_via_engine(ContainerCall {
            engine: &engine,
            run_id,
            adapter: &adapter,
            image: &image,
            entrypoint: &["python3".to_string(), "/app/smoke_tool.py".to_string()],
            family: "utility",
            network: NetworkPlan::None,
        })
        .unwrap_or_else(|e| panic!("{run_id} container run failed: {e}"));
        assert_eq!(outcome.envelope.status, ToolRunStatus::Succeeded);
        assert!(
            started.elapsed() < Duration::from_secs(30),
            "cached dispatch must be quick"
        );
    }

    // 缓存命中实测：两次派发的 podman 子命令日志零 pull 零 build。
    let log = invocation_log(&ws_root);
    let mut counts: HashMap<&str, usize> = HashMap::new();
    for cmd in &log {
        *counts.entry(cmd.as_str()).or_default() += 1;
    }
    assert_eq!(
        counts.get("run").copied().unwrap_or(0),
        2,
        "two dispatches must issue exactly two podman run: {log:?}"
    );
    assert!(
        counts.get("image").copied().unwrap_or(0) >= 2,
        "PREPARING must probe local store with image exists: {log:?}"
    );
    assert!(
        !log.iter().any(|c| c == "pull" || c == "build"),
        "adapter must never pull or build (cache policy): {log:?}"
    );
    assert!(
        p27_containers(false).is_empty(),
        "no rip-tool-rn-p27 containers may remain"
    );
    let _ = std::fs::remove_dir_all(&ws_root);
}

#[test]
fn budget_timeout_registers_partial_artifact() {
    if !podman_available() {
        eprintln!("SKIP: podman unavailable (V-9，容器端到端保持本地执行)");
        return;
    }
    let _e2e = E2E_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    if ensure_image().is_none() {
        return;
    }
    let (pool, engine, ws_root) = fresh_env("partial");
    let config = PodmanHostConfig {
        workspace_root: ws_root.clone(),
        ..PodmanHostConfig::default()
    };
    let adapter = PodmanHostAdapter::new(config);
    let image = image_ref();
    submit(
        &engine,
        "rn-p27-partial",
        json!({"echo": "partial-probe", "sleep_ms": 60000}),
        json!({"timeout_seconds": 1, "max_output_bytes": 1048576}),
    );
    let started = Instant::now();
    let outcome = execute_container_via_engine(ContainerCall {
        engine: &engine,
        run_id: "rn-p27-partial",
        adapter: &adapter,
        image: &image,
        entrypoint: &["python3".to_string(), "/app/smoke_tool.py".to_string()],
        family: "utility",
        network: NetworkPlan::None,
    })
    .expect("timeout run");
    assert!(
        started.elapsed() < Duration::from_secs(15),
        "watchdog must fire promptly at 1s budget"
    );
    assert_eq!(outcome.envelope.status, ToolRunStatus::TimedOut);

    // 已产出按 partial 登记（ADR 5 第 3 条）：early.txt 在睡眠前落盘，
    // 强制取消后照常收集，provenance 标注 partial，sha256 与文件一致。
    let early = ws_root.join("rn-p27-partial/out/early.txt");
    assert!(early.exists(), "early output must exist before the kill");
    let bytes = std::fs::read(&early).unwrap();
    let rows: Vec<(Option<String>, String, i64, String)> = pool
        .with_conn(|c| {
            let mut stmt = c.prepare(
                "SELECT role, sha256, size_bytes, provenance FROM artifact WHERE run_id = 'rn-p27-partial'",
            )?;
            let rows = stmt
                .query_map([], |r| {
                    Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
                })?
                .filter_map(|r| r.ok())
                .collect();
            Ok::<Vec<_>, database::rusqlite::Error>(rows)
        })
        .unwrap();
    assert!(
        rows.iter().any(
            |(_role, sha, size, prov)| *sha == tool_registry::hash::sha256_hex(&bytes)
                && *size == bytes.len() as i64
                && prov.contains("partial")
        ),
        "partial artifact must reconcile with early.txt: {rows:?}"
    );
    assert!(outcome
        .envelope
        .artifacts
        .iter()
        .any(|a| a.provenance.contains("partial")));

    // tmp/ 清理 + 容器回收。
    assert!(
        !ws_root.join("rn-p27-partial/tmp").exists(),
        "tmp/ must be removed"
    );
    assert!(
        p27_containers(false).is_empty(),
        "container must be reclaimed"
    );
    let _ = std::fs::remove_dir_all(&ws_root);
}

#[test]
fn oversized_output_is_truncated_with_artifact_carrier() {
    if !podman_available() {
        eprintln!("SKIP: podman unavailable (V-9，容器端到端保持本地执行)");
        return;
    }
    let _e2e = E2E_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    if ensure_image().is_none() {
        return;
    }
    let (_pool, engine, ws_root) = fresh_env("trunc");
    let config = PodmanHostConfig {
        workspace_root: ws_root.clone(),
        ..PodmanHostConfig::default()
    };
    let adapter = PodmanHostAdapter::new(config);
    let image = image_ref();
    let big = "x".repeat(2000);
    submit(
        &engine,
        "rn-p27-trunc",
        json!({"echo": big}),
        // 256 为 worker protocol 域下限：结果超帽触发 §5.4 截断信封。
        json!({"timeout_seconds": 60, "max_output_bytes": 256}),
    );
    let outcome = execute_container_via_engine(ContainerCall {
        engine: &engine,
        run_id: "rn-p27-trunc",
        adapter: &adapter,
        image: &image,
        entrypoint: &["python3".to_string(), "/app/smoke_tool.py".to_string()],
        family: "utility",
        network: NetworkPlan::None,
    })
    .expect("truncation run");
    let env = &outcome.envelope;
    assert_eq!(env.status, ToolRunStatus::Succeeded);
    assert!(env.usage.truncated, "oversized result must be truncated");
    let notice = env.result.as_ref().expect("result");
    assert_eq!(notice["truncated"], json!(true), "notice carrier shape");
    assert!(notice["original_bytes"].as_u64().expect("bytes") > 256);
    // 截断载物 artifact（原文 sha256）与 out/ 原始产物并存。
    assert!(
        env.artifacts
            .iter()
            .any(|a| a.provenance.contains("截断") && a.sha256.len() == 64),
        "truncation carrier artifact must carry original sha256: {:?}",
        env.artifacts
    );
    let result_file = ws_root.join("rn-p27-trunc/out/result.json");
    let bytes = std::fs::read(&result_file).unwrap();
    assert!(
        env.artifacts.iter().any(|a| a.uri.ends_with("result.json")
            && a.sha256 == tool_registry::hash::sha256_hex(&bytes)),
        "out/ raw artifact must be collected untruncated"
    );
    let _ = std::fs::remove_dir_all(&ws_root);
}
