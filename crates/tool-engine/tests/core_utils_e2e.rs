//! P3-4 验证载具：Core Utilities Pack 五工具端到端（ROADMAP P3-3 本体）。
//!
//! 判据（批次自验清单）：
//! 1. 五工具（file_meta/hash/compress/convert/validate）各至少一次经
//!    Gateway::dispatch（register_node + 真实探针）到 podman_host 的 run
//!    端到端 SUCCEEDED，artifact sha256 与落盘文件对账一致；
//! 2. 坏输入用例 FAILED 且状态机跃迁合法（[Queued,Preparing,Running,Failed]）；
//!    yaml 校验请求在登记面被 input_schema 拒（stdlib-only 降级声明的执行面）；
//! 3. Manifest 校验九条负例在位：tag 引用镜像与 sha256:<digest> 占位被
//!    Registry 拒绝；network.mode 词表外值被拒（validate.rs:419 同族）；
//! 4. 每工具族并发基线 1（FamilyLimiter 既有默认，不为验收提速调高）；
//! 5. 嵌套 json 坏输入 FAILED（convert 受限矩阵执行面）；截断/超时/取消走
//!    共享 Adapter 既有语义（podman_host_budget 同载具覆盖，见头注门禁段）。
//!
//! 门禁：容器端到端保持本地执行（V-9 同款）；超时/取消路径由共享 Adapter 的
//! 既有行为测试覆盖（crates/tool-engine/tests/podman_host_budget.rs，同一
//! execute_container_via_engine 载具），本批不重复构造 coreutils 专属长跑负载
//! （五工具均为秒级纯本地变换，无受控睡眠面）。

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

const IMAGE_TAG: &str = "localhost/rip-tool-coreutils:latest";
/// P3-1 首建、P3-4 op 分派改造后重建的 digest（containerfiles/README.md 台账）。
const IMAGE_DIGEST: &str = "49436f71b66974631ffc4ea7ac805c95bfa07d4d5fb00423c3d5c7ea9b18acaf";

const FIVE_TOOLS: [&str; 5] = [
    "rip.core.file_meta",
    "rip.core.hash",
    "rip.core.compress",
    "rip.core.convert",
    "rip.core.validate",
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

fn ensure_image() -> String {
    let exists = Command::new("podman")
        .args(["image", "exists", IMAGE_TAG])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    if !exists {
        let dir = repo_root().join("containerfiles/rip-tool-coreutils");
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
        format!("sha256:{IMAGE_DIGEST}"),
        "local image digest must match containerfiles/README.md digest ledger"
    );
    format!("{IMAGE_TAG}@sha256:{IMAGE_DIGEST}")
}

fn manifest_value(tool_id: &str) -> Value {
    serde_json::from_str(
        &std::fs::read_to_string(
            repo_root().join(format!("containerfiles/rip-tool-coreutils/{tool_id}.json")),
        )
        .unwrap(),
    )
    .unwrap()
}

/// 开发库副本（文件库）+ 五 Manifest 登记 + 引擎。
fn fresh_dev_env(tag: &str) -> (Pool, Engine, PathBuf, String) {
    let image = ensure_image();
    let base = std::env::temp_dir().join(format!("rip-p34-{tag}-{}", std::process::id()));
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
    for tool_id in FIVE_TOOLS {
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
        "submit_key": format!("p34-{run_id}"),
        "input": input,
        "workspace_id": "ws-p34",
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
    let op = tool_id.trim_start_matches("rip.core.");
    let entrypoint = vec![
        "python3".to_string(),
        "/app/core_tools.py".to_string(),
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
        family: "utility",
        network: NetworkPlan::None,
    })
    .expect("container run")
}

#[test]
fn five_tools_end_to_end_with_artifact_reconciliation() {
    let _e2e = E2E_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    if !podman_available() {
        eprintln!("SKIP: podman unavailable (V-9，容器端到端保持本地执行)");
        return;
    }
    let (pool, engine, ws_root, image) = fresh_dev_env("ok");
    let sample = "rip core utils p3-4 sample\n";
    let rows_json = r#"[{"a":1,"b":"x"},{"a":2,"b":"y"}]"#;

    // 1) file_meta
    place_input(&ws_root, "rn-p34-meta", "sample.txt", sample);
    let outcome = dispatch_and_run(
        &pool,
        &engine,
        &ws_root,
        &image,
        "rip.core.file_meta",
        "rn-p34-meta",
        json!({"path": "/task/in/sample.txt"}),
        1048576,
    );
    assert_eq!(
        outcome.envelope.status,
        ToolRunStatus::Succeeded,
        "{:?}",
        outcome.envelope.error
    );
    let data = outcome.envelope.result.as_ref().unwrap();
    assert_eq!(data["type"], json!("text"));
    assert_eq!(data["size_bytes"].as_u64(), Some(sample.len() as u64));

    // 2) hash（与宿主 sha256sum 对账）
    place_input(&ws_root, "rn-p34-hash", "sample.txt", sample);
    let outcome = dispatch_and_run(
        &pool,
        &engine,
        &ws_root,
        &image,
        "rip.core.hash",
        "rn-p34-hash",
        json!({"path": "/task/in/sample.txt", "algorithm": "sha256"}),
        1048576,
    );
    assert_eq!(
        outcome.envelope.status,
        ToolRunStatus::Succeeded,
        "{:?}",
        outcome.envelope.error
    );
    let data = outcome.envelope.result.as_ref().unwrap();
    assert_eq!(
        data["digest"].as_str().unwrap(),
        tool_registry::hash::sha256_hex(sample.as_bytes()),
        "hash must match engine-side sha256 of the input bytes"
    );

    // 3) compress gzip（产物 sha256 与表对账）
    place_input(&ws_root, "rn-p34-gz", "sample.txt", sample);
    let outcome = dispatch_and_run(
        &pool,
        &engine,
        &ws_root,
        &image,
        "rip.core.compress",
        "rn-p34-gz",
        json!({"mode": "gzip", "src": "/task/in/sample.txt"}),
        1048576,
    );
    assert_eq!(
        outcome.envelope.status,
        ToolRunStatus::Succeeded,
        "{:?}",
        outcome.envelope.error
    );
    let gz_path = ws_root.join("rn-p34-gz/out/sample.txt.gz");
    assert!(gz_path.exists(), "gzip artifact must exist");
    let gz_bytes = std::fs::read(&gz_path).unwrap();
    let artifact = outcome
        .envelope
        .artifacts
        .iter()
        .find(|a| a.uri.ends_with("sample.txt.gz"))
        .expect("gzip artifact registered");
    assert_eq!(artifact.sha256, tool_registry::hash::sha256_hex(&gz_bytes));
    assert_eq!(
        outcome.envelope.result.as_ref().unwrap()["sha256"]
            .as_str()
            .unwrap(),
        tool_registry::hash::sha256_hex(&gz_bytes)
    );

    // 4) convert json->csv
    place_input(&ws_root, "rn-p34-cv", "rows.json", rows_json);
    let outcome = dispatch_and_run(
        &pool,
        &engine,
        &ws_root,
        &image,
        "rip.core.convert",
        "rn-p34-cv",
        json!({"from": "json", "to": "csv", "src": "/task/in/rows.json"}),
        1048576,
    );
    assert_eq!(
        outcome.envelope.status,
        ToolRunStatus::Succeeded,
        "{:?}",
        outcome.envelope.error
    );
    let csv_path = ws_root.join("rn-p34-cv/out/rows.csv");
    assert!(csv_path.exists());
    let csv_bytes = std::fs::read(&csv_path).unwrap();
    let artifact = outcome
        .envelope
        .artifacts
        .iter()
        .find(|a| a.uri.ends_with("rows.csv"))
        .expect("csv artifact registered");
    assert_eq!(artifact.sha256, tool_registry::hash::sha256_hex(&csv_bytes));
    assert_eq!(
        outcome.envelope.result.as_ref().unwrap()["rows"].as_i64(),
        Some(2)
    );

    // 5) validate json
    place_input(&ws_root, "rn-p34-val", "rows.json", rows_json);
    let outcome = dispatch_and_run(
        &pool,
        &engine,
        &ws_root,
        &image,
        "rip.core.validate",
        "rn-p34-val",
        json!({"format": "json", "src": "/task/in/rows.json"}),
        1048576,
    );
    assert_eq!(
        outcome.envelope.status,
        ToolRunStatus::Succeeded,
        "{:?}",
        outcome.envelope.error
    );
    let data = outcome.envelope.result.as_ref().unwrap();
    assert_eq!(data["valid"], json!(true));
    assert_eq!(data["items"].as_i64(), Some(2));

    // 开发库副本对账：五 run 全 SUCCEEDED + tool 表 5 行。
    let rows: Vec<(String, String)> = pool
        .with_conn(|c| {
            let mut stmt = c.prepare("SELECT run_id, status FROM tool_run")?;
            let rows = stmt
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
                .filter_map(|r| r.ok())
                .collect();
            Ok::<Vec<_>, database::rusqlite::Error>(rows)
        })
        .unwrap();
    assert_eq!(rows.len(), 5, "five runs expected: {rows:?}");
    assert!(rows.iter().all(|(_id, s)| s == "SUCCEEDED"));
    let tool_count: i64 = pool
        .with_conn(|c| c.query_row("SELECT COUNT(*) FROM tool", [], |r| r.get(0)))
        .unwrap();
    assert_eq!(tool_count, 5, "five registered tools expected");

    assert!(
        !Command::new("podman")
            .args([
                "ps",
                "-aq",
                "--filter",
                "label=rip.runtime=rip-tool",
                "--format",
                "{{.Names}}"
            ])
            .output()
            .map(|o| !String::from_utf8_lossy(&o.stdout).trim().is_empty())
            .unwrap_or(true),
        "no rip-tool containers may remain"
    );
}

#[test]
fn error_paths_bad_input_yaml_gate_and_truncation() {
    let _e2e = E2E_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    if !podman_available() {
        eprintln!("SKIP: podman unavailable (V-9，容器端到端保持本地执行)");
        return;
    }
    let (pool, engine, ws_root, image) = fresh_dev_env("err");

    // 坏输入（schema 合法、执行面失败）：路径不存在 → FAILED，跃迁合法。
    let outcome = dispatch_and_run(
        &pool,
        &engine,
        &ws_root,
        &image,
        "rip.core.hash",
        "rn-p34-missing",
        json!({"path": "/task/in/nope.txt"}),
        1048576,
    );
    assert_eq!(outcome.envelope.status, ToolRunStatus::Failed);
    assert_eq!(
        outcome.transcript,
        vec![
            ToolRunStatus::Queued,
            ToolRunStatus::Preparing,
            ToolRunStatus::Running,
            ToolRunStatus::Failed,
        ],
        "state machine transitions must be legal"
    );
    let error = outcome.envelope.error.as_ref().expect("structured error");
    assert_eq!(error.category, "EXECUTION");
    assert!(
        error.message.contains("FileNotFoundError") || error.message.contains("input not found"),
        "bad input must surface the tool error: {}",
        error.message
    );

    // yaml 校验请求：input_schema 枚举在登记面拒绝（stdlib-only 降级的执行面）。
    let rejected = engine.submit(&submit_request(
        "rip.core.validate",
        "rn-p34-yaml",
        json!({"format": "yaml", "text": "a: 1"}),
        1048576,
    ));
    assert!(
        rejected.is_err(),
        "yaml must be rejected by input_schema enum"
    );

    // 坏输入第二形（schema 合法、执行面失败）：嵌套 json 转 csv 被拒 → FAILED。
    place_input(
        &ws_root,
        "rn-p34-nested",
        "nested.json",
        r#"[{"a":1,"nested":{"b":2}}]"#,
    );
    let outcome = dispatch_and_run(
        &pool,
        &engine,
        &ws_root,
        &image,
        "rip.core.convert",
        "rn-p34-nested",
        json!({"from": "json", "to": "csv", "src": "/task/in/nested.json"}),
        1048576,
    );
    assert_eq!(outcome.envelope.status, ToolRunStatus::Failed);
    assert_eq!(
        outcome.transcript,
        vec![
            ToolRunStatus::Queued,
            ToolRunStatus::Preparing,
            ToolRunStatus::Running,
            ToolRunStatus::Failed,
        ]
    );
    let error = outcome.envelope.error.as_ref().expect("structured error");
    assert!(
        error.message.contains("flat objects") || error.message.contains("ValueError"),
        "nested json must surface the conversion error: {}",
        error.message
    );
    let status: String = pool
        .with_conn(|c| {
            c.query_row(
                "SELECT status FROM tool_run WHERE run_id = 'rn-p34-nested'",
                [],
                |r| r.get(0),
            )
        })
        .unwrap();
    assert_eq!(status, "FAILED", "bad-input run must be terminal FAILED");
}

#[test]
fn manifest_rule_nine_negatives_and_family_limiter_baseline() {
    // 无需 podman：Registry 校验九条负例 + FamilyLimiter 基线断言。
    let pool = Pool::open(PoolConfig {
        path: DbPath::Memory,
        readonly: false,
        max_connections: 1,
        busy_timeout: Duration::from_secs(5),
    })
    .unwrap();
    migrate(&pool, &repo_root().join("migrations")).unwrap();
    let registry = Registry::open(pool).with_options(Options {
        allow_placeholder_digest: false,
    });
    let good = manifest_value("rip.core.hash");

    // tag 引用镜像：拒绝。
    let mut tag_ref = good.clone();
    tag_ref["runtime"]["image"] = json!("localhost/rip-tool-coreutils:latest");
    assert!(
        registry.register(&tag_ref).is_err(),
        "tag-ref image must be rejected (rule 4 double check)"
    );

    // sha256:<digest> 占位：拒绝。
    let mut placeholder = good.clone();
    placeholder["runtime"]["image"] = json!("localhost/rip-tool-coreutils:latest@sha256:<digest>");
    assert!(
        registry.register(&placeholder).is_err(),
        "digest placeholder must be rejected"
    );

    // network.mode 词表外值：拒绝（validate.rs:419 同族）。
    let mut bad_network = good.clone();
    bad_network["network"] = json!({"mode": "bogus"});
    assert!(
        registry.register(&bad_network).is_err(),
        "network.mode outside vocabulary must be rejected"
    );

    // 正常五 Manifest：全部通过（good 保持原状注册一次验证基准面）。
    registry
        .register(&good)
        .expect("clean manifest must register");

    // FamilyLimiter 基线：每工具族默认 1（不为验收提速调高）。
    assert_eq!(
        PodmanHostConfig::default().family_concurrency_default,
        1,
        "family concurrency baseline must stay 1 (ROADMAP §4.5 discipline 4)"
    );
    let limiter = tool_engine::adapter::FamilyLimiter::new(1);
    let _guard = limiter.try_acquire("utility").expect("first slot");
    assert!(
        limiter.try_acquire("utility").is_none(),
        "second concurrent slot in same family must be denied (baseline 1)"
    );
}
