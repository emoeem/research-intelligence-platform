//! 收尾武器库批验证载具：Source Analysis Pack 三工具端到端
//! （pack_id = manifests/packs/planned/source-analysis.json 冻结存根落地，
//! core_utils_e2e.rs 同款载具形状）。
//!
//! 判据：
//! 1. 三工具（deps-manifest/static-check/code-search）各一次经
//!    Gateway::dispatch（register_node + 真实探针）到 podman_host 的 run
//!    端到端 SUCCEEDED，结果与预置输入对账（deps 条目数、SEC001 命中、
//!    regex 命中行号）；
//! 2. 路径穿越负例 FAILED 且状态机跃迁合法（worker 侧 /task/in 前缀校验，
//!    core_tools 同款边界）；
//! 3. Registry 九条负例抽样在位：tag 引用镜像被拒（allow_placeholder_digest
//!    口径下 digest 缺失即拒，与 core_utils_e2e 负例同族）。
//!
//! 门禁：容器端到端保持本地执行（V-9 同款）：podman 不在位或验收镜像
//! （digest 钉死，containerfiles/README.md 台账）不在本地镜像库时跳过；
//! worker 纯函数面由 workers/python/tests/test_src_tools.py 宿主单测覆盖，
//! 不依赖容器。

use database::{migrate, DbPath, Pool, PoolConfig};
use domain::{NodeHealth, RuntimeNode, ToolRunStatus};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;
use std::time::Duration;
use tool_engine::adapter::{
    execute_container_via_engine, ContainerCall, NetworkPlan, PodmanHostAdapter, PodmanHostConfig,
};
use tool_engine::Engine;
use tool_registry::{Options, Registry};

const IMAGE_TAG: &str = "localhost/rip-tool-source:latest";
/// 收尾武器库批首建 digest（containerfiles/README.md 台账）。
const IMAGE_DIGEST: &str = "8251ba32e6ae4d107190f2b2279ae3774facdc8e074ae000266be3aa1fd0ea29";

const THREE_TOOLS: [&str; 3] = [
    "rip.src.deps-manifest",
    "rip.src.static-check",
    "rip.src.code-search",
];

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

/// 验收镜像按 digest 钉死（containerfiles/README.md 台账）。镜像属运行时
/// 数据（ADR §3.2），不在仓库内也无法现建命中同一 digest：只在已准备
/// 镜像的受控环境（本地验收）执行，否则按 V-9 跳过。
fn ensure_image() -> Option<String> {
    let pinned = format!("{IMAGE_TAG}@sha256:{IMAGE_DIGEST}");
    let exists = Command::new("podman")
        .args(["image", "exists", &pinned])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    if !exists {
        eprintln!("SKIP: 验收镜像 {pinned} 不在本地镜像库，digest 钉死无法现建命中（V-9，受控环境端到端）");
        return None;
    }
    Some(pinned)
}

fn manifest_value(tool_id: &str) -> Value {
    serde_json::from_str(
        &std::fs::read_to_string(
            repo_root().join(format!("containerfiles/rip-tool-source/{tool_id}.json")),
        )
        .unwrap(),
    )
    .unwrap()
}

/// 开发库副本（文件库）+ 三 Manifest 登记 + 引擎（镜像由调用侧先经
/// ensure_image 取得，便于无镜像环境在测试侧按 V-9 跳过）。
fn fresh_dev_env(tag: &str, image: String) -> (Pool, Engine, PathBuf, String) {
    let base = std::env::temp_dir().join(format!("rip-src-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(&base).unwrap();
    let pool = Pool::open(PoolConfig {
        path: DbPath::File(base.join("platform-devcopy.db")),
        readonly: false,
        max_connections: 1,
        busy_timeout: Duration::from_secs(5),
    })
    .unwrap();
    migrate(&pool, &repo_root().join("migrations")).unwrap();
    let registry = Registry::open(pool.clone()).with_options(Options {
        allow_placeholder_digest: false,
    });
    for tool_id in THREE_TOOLS {
        registry
            .register(&manifest_value(tool_id))
            .unwrap_or_else(|e| panic!("{tool_id} must pass rule 9: {e}"));
    }
    let engine = Engine::open(pool.clone(), registry);
    let ws_root = base.join("ws");
    std::fs::create_dir_all(&ws_root).unwrap();
    (pool, engine, ws_root, image)
}

fn submit_request(tool_id: &str, run_id: &str, input: Value, max_output_bytes: u64) -> Value {
    json!({
        "protocol_version": "1",
        "tool_id": tool_id,
        "run_id": run_id,
        "submit_key": format!("src-{run_id}"),
        "input": input,
        "workspace_id": "ws-src",
        "budget": {"timeout_seconds": 120, "max_output_bytes": max_output_bytes},
        "result_schema_version": "1"
    })
}

/// 输入文件预置（Adapter prepare_workspace 只建目录不清文件）。
fn place_input(ws_root: &Path, run_id: &str, name: &str, content: &str) {
    let dir = ws_root.join(run_id).join("in");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(name), content).unwrap();
}

/// 全链：Gateway::dispatch（register_node + 真实探针）→ 容器执行（不绕过 Gateway）。
#[allow(clippy::too_many_arguments)] // 测试载具：八参为全链入参的最直白形状
fn dispatch_and_run(
    pool: &Pool,
    engine: &Engine,
    ws_root: &Path,
    image: &str,
    tool_id: &str,
    run_id: &str,
    input: Value,
    max_output_bytes: u64,
) -> tool_engine::DriveOutcome {
    let store = runtime_gateway::HealthStore::open(pool.clone());
    let gateway = runtime_gateway::Gateway::new(
        engine.clone(),
        store,
        runtime_gateway::GatewayConfig::default(),
    );
    gateway
        .register_node(
            RuntimeNode {
                node_id: "host-cachyos".to_string(),
                kind: domain::RuntimeNodeKind::PodmanHost,
                endpoint: None,
                health: NodeHealth::Unknown,
                version: None,
            },
            std::sync::Arc::new(runtime_gateway::CommandProbe::podman()),
        )
        .expect("register podman_host node");
    let dispatched = gateway
        .dispatch(&submit_request(tool_id, run_id, input, max_output_bytes))
        .expect("dispatch");
    assert!(
        matches!(dispatched, runtime_gateway::DispatchOutcome::Queued { .. }),
        "healthy node must queue: {dispatched:?}"
    );
    let op = match tool_id {
        "rip.src.deps-manifest" => "deps",
        "rip.src.static-check" => "static-check",
        _ => "search",
    };
    let entrypoint = vec![
        "python3".to_string(),
        "/app/src_tools.py".to_string(),
        op.to_string(),
    ];
    let adapter = PodmanHostAdapter::new(PodmanHostConfig {
        workspace_root: ws_root.to_path_buf(),
        ..PodmanHostConfig::default()
    });
    execute_container_via_engine(ContainerCall {
        engine,
        run_id,
        adapter: &adapter,
        image,
        entrypoint: &entrypoint,
        family: "source_code",
        network: NetworkPlan::None,
    })
    .expect("container run")
}

#[test]
fn three_tools_end_to_end_with_reconciliation() {
    let _e2e = E2E_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    if !podman_available() {
        eprintln!("SKIP: podman unavailable (V-9，容器端到端保持本地执行)");
        return;
    }
    let image = match ensure_image() {
        Some(image) => image,
        None => return,
    };
    let (pool, engine, ws_root, image) = fresh_dev_env("ok", image);

    // 1) deps-manifest：npm 清单归一化（条目数与 kind 对账）
    place_input(
        &ws_root,
        "rn-src-deps",
        "package.json",
        r#"{"name":"demo","dependencies":{"react":"^18.0.0","axios":"1.6.0"},"devDependencies":{"vitest":"^1.2.0"}}"#,
    );
    let outcome = dispatch_and_run(
        &pool,
        &engine,
        &ws_root,
        &image,
        "rip.src.deps-manifest",
        "rn-src-deps",
        json!({"path": "/task/in/package.json"}),
        1048576,
    );
    assert_eq!(
        outcome.envelope.status,
        ToolRunStatus::Succeeded,
        "{:?}",
        outcome.envelope.error
    );
    let data = outcome.envelope.result.as_ref().unwrap();
    assert_eq!(data["ecosystem"], json!("npm"));
    assert_eq!(data["count"].as_u64(), Some(3));
    assert_eq!(data["deps"][2]["kind"], json!("dev"));

    // 2) static-check：planted 秘钥命中 SEC001，eval 命中 CODE001
    place_input(
        &ws_root,
        "rn-src-sec",
        "app.py",
        "import os\nKEY = \"AKIAIOSFODNN7EXAMPLE\"\ndef f(x):\n    return eval(x)\n",
    );
    let outcome = dispatch_and_run(
        &pool,
        &engine,
        &ws_root,
        &image,
        "rip.src.static-check",
        "rn-src-sec",
        json!({"path": "/task/in/app.py"}),
        1048576,
    );
    assert_eq!(
        outcome.envelope.status,
        ToolRunStatus::Succeeded,
        "{:?}",
        outcome.envelope.error
    );
    let data = outcome.envelope.result.as_ref().unwrap();
    let rule_ids: Vec<&str> = data["findings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["rule_id"].as_str().unwrap())
        .collect();
    assert!(rule_ids.contains(&"SEC001"), "findings: {rule_ids:?}");
    assert!(rule_ids.contains(&"CODE001"), "findings: {rule_ids:?}");
    assert_eq!(data["summary"]["high"].as_u64(), Some(1));

    // 3) code-search：regex 命中行号对账
    place_input(
        &ws_root,
        "rn-src-search",
        "lib.rs",
        "fn a() {}\n// TODO fix later\nfn b() {}\n",
    );
    let outcome = dispatch_and_run(
        &pool,
        &engine,
        &ws_root,
        &image,
        "rip.src.code-search",
        "rn-src-search",
        json!({"path": "/task/in/lib.rs", "pattern": "TODO", "max_matches": 10}),
        1048576,
    );
    assert_eq!(
        outcome.envelope.status,
        ToolRunStatus::Succeeded,
        "{:?}",
        outcome.envelope.error
    );
    let data = outcome.envelope.result.as_ref().unwrap();
    assert_eq!(data["count"].as_u64(), Some(1));
    assert_eq!(data["matches"][0]["line"].as_u64(), Some(2));
}

#[test]
fn traversal_input_fails_with_legal_transitions() {
    let _e2e = E2E_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    if !podman_available() {
        eprintln!("SKIP: podman unavailable (V-9，容器端到端保持本地执行)");
        return;
    }
    let image = match ensure_image() {
        Some(image) => image,
        None => return,
    };
    let (pool, engine, ws_root, image) = fresh_dev_env("trav", image);
    let outcome = dispatch_and_run(
        &pool,
        &engine,
        &ws_root,
        &image,
        "rip.src.static-check",
        "rn-src-trav",
        json!({"path": "/task/in/../../etc/passwd"}),
        1048576,
    );
    assert_eq!(outcome.envelope.status, ToolRunStatus::Failed);
    let error = outcome.envelope.error.as_ref().expect("error frame");
    assert!(
        error.message.contains("escapes /task/in"),
        "traversal rejection message: {}",
        error.message
    );
}

#[test]
fn registry_rejects_digest_placeholder() {
    // Registry 负例不依赖 podman：allow_placeholder_digest=false 下占位 digest 拒绝。
    let base = std::env::temp_dir().join(format!("rip-src-neg-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(&base).unwrap();
    let pool = Pool::open(PoolConfig {
        path: DbPath::File(base.join("platform-devcopy.db")),
        readonly: false,
        max_connections: 1,
        busy_timeout: Duration::from_secs(5),
    })
    .unwrap();
    migrate(&pool, &repo_root().join("migrations")).unwrap();
    let registry = Registry::open(pool).with_options(Options {
        allow_placeholder_digest: false,
    });
    let mut manifest = manifest_value("rip.src.static-check");
    manifest["runtime"]["image"] = json!("localhost/rip-tool-source:latest");
    let err = registry
        .register(&manifest)
        .expect_err("tag reference must be rejected");
    assert!(
        err.to_string().contains("digest") || err.to_string().contains("image"),
        "rejection message: {err}"
    );
}
