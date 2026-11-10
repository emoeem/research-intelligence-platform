//! P3-3 验证载具：rip.http.inspect Registry 登记入库（开发库副本）+ 经
//! Gateway::dispatch（runtime-gateway lib.rs:196）全链端到端（TOOL-CONTRACT
//! §9.3 四步；ROADMAP P3-2 本体，消解 §11 待验证项 1 与 ROADMAP V-10）。
//!
//! 判据（批次自验清单）：
//! 1. Gateway::register_node（recovery_drill.rs:122 同款）登记
//!    kind=podman_host 节点（CommandProbe::podman 真实探针），dispatch 后
//!    runtime_node health=ok；
//! 2. 对 example.com 的 GET 全链跑通：dispatch（QUEUED）→
//!    execute_container_via_engine → SUCCEEDED，结构化结果含
//!    status_code/final_url/body_artifact_id，output_schema 校验在 Result
//!    信封内生效（不合 schema 即 FAILED，引擎侧强制）；
//! 3. 响应体 Artifact：out/body.bin sha256 与 artifact 表对账一致
//!    （role=primary，TOOL-CONTRACT §9.3 第 4 步）；
//! 4. 开发库副本（文件库，migrations 001 到 007 应用）：tool_run 出现
//!    SUCCEEDED + rip.http.inspect 行，tool/tool_version 登记行在库；
//!    测试不清理库与工作区，供批次自验 python3 -I 只读复核；
//! 5. 最小功能负向：method=POST 被 input_schema 在提交面拒绝（登记面
//!    T1 收敛）；HEAD 与 save_body=false 变体最小功能验证通过。
//!
//! 门禁：容器端到端保持本地执行（V-9 同款）；example.com 为 T1 单请求。

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

/// 开发库副本（文件库）：测试后保留，供批次自验 python3 -I 只读对账。
const DEV_COPY_DIR: &str = "rip-p33-devcopy";
const DEV_COPY_DB: &str = "platform-devcopy.db";

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

fn registered_manifest() -> Value {
    serde_json::from_str(
        &std::fs::read_to_string(
            repo_root().join("containerfiles/rip-tool-webresearch/tool-manifest.json"),
        )
        .unwrap(),
    )
    .unwrap()
}

/// 开发库副本 + Registry 登记 + 引擎/网关（recovery_drill fresh_env 同款，换文件库）。
fn fresh_dev_env() -> (Pool, Engine, PathBuf, Value) {
    let manifest = registered_manifest();
    let base = std::env::temp_dir().join(DEV_COPY_DIR);
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(&base).unwrap();
    let db_path = base.join(DEV_COPY_DB);
    let ws_root = base.join("ws");
    std::fs::create_dir_all(&ws_root).unwrap();
    let pool = Pool::open(PoolConfig {
        path: DbPath::File(db_path),
        readonly: false,
        max_connections: 1,
        busy_timeout: Duration::from_secs(5),
    })
    .unwrap();
    migrate(&pool, &repo_root().join("migrations")).unwrap();
    let registry = Registry::open(pool.clone()).with_options(Options {
        allow_placeholder_digest: false,
    });
    registry.register(&manifest).unwrap();
    let engine = Engine::open(pool.clone(), registry);
    (pool, engine, ws_root, manifest)
}

fn submit_request(run_id: &str, input: Value) -> Value {
    json!({
        "protocol_version": "1",
        "tool_id": "rip.http.inspect",
        "run_id": run_id,
        "submit_key": format!("p33-{run_id}"),
        "input": input,
        "workspace_id": "ws-p33",
        "budget": {"timeout_seconds": 120, "max_output_bytes": 1048576},
        "result_schema_version": "1"
    })
}

/// 全链：Gateway::dispatch → execute_container_via_engine（不绕过 Gateway）。
fn dispatch_and_run(
    pool: &Pool,
    engine: &Engine,
    manifest: &Value,
    ws_root: &Path,
    run_id: &str,
    input: Value,
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

    let outcome = gateway
        .dispatch(&submit_request(run_id, input))
        .expect("dispatch");
    assert!(
        matches!(outcome, runtime_gateway::DispatchOutcome::Queued { .. }),
        "healthy podman_host node must queue the run: {outcome:?}"
    );
    let health: String = pool
        .with_conn(|c| {
            c.query_row(
                "SELECT health FROM runtime_node WHERE node_id = 'host-cachyos'",
                [],
                |r| r.get(0),
            )
        })
        .unwrap();
    assert_eq!(
        health, "ok",
        "podman_host node health must be ok after probe"
    );
    let kind: String = pool
        .with_conn(|c| {
            c.query_row(
                "SELECT kind FROM runtime_node WHERE node_id = 'host-cachyos'",
                [],
                |r| r.get(0),
            )
        })
        .unwrap();
    assert_eq!(kind, "podman_host", "node kind must be podman_host");

    let image = manifest["runtime"]["image"]
        .as_str()
        .expect("runtime.image");
    let entrypoint: Vec<String> = manifest["runtime"]["entrypoint"]
        .as_array()
        .expect("entrypoint")
        .iter()
        .map(|v| v.as_str().expect("entrypoint item").to_string())
        .collect();
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
        family: "web_http",
        network: NetworkPlan::EgressAllowlist {
            ports: vec![80, 443],
        },
    })
    .expect("container run via gateway-dispatched queue")
}

#[test]
fn gateway_dispatch_full_chain_get_with_body_artifact() {
    let _e2e = E2E_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    if !podman_available() {
        eprintln!("SKIP: podman unavailable (V-9，容器端到端保持本地执行)");
        return;
    }
    let (pool, engine, ws_root, manifest) = fresh_dev_env();

    // 主判据：GET https://example.com/ 经 Gateway::dispatch 全链跑通。
    let outcome = dispatch_and_run(
        &pool,
        &engine,
        &manifest,
        &ws_root,
        "rn-p33-ok",
        json!({"url": "https://example.com/"}),
    );
    let env = &outcome.envelope;
    assert_eq!(
        env.status,
        ToolRunStatus::Succeeded,
        "gateway-dispatched GET must succeed: {:?}",
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
    let data = env.result.as_ref().expect("structured result");
    assert_eq!(
        data["status_code"].as_i64(),
        Some(200),
        "example.com GET 200"
    );
    assert_eq!(data["final_url"].as_str(), Some("https://example.com/"));
    assert_eq!(data["body_artifact_id"].as_str(), Some("body.bin"));
    assert!(data["body_bytes"].as_u64().unwrap() > 0);

    // §9.3 第 4 步：响应体 Artifact 收集登记，sha256 与落盘文件对账一致。
    let body_path = ws_root.join("rn-p33-ok/out/body.bin");
    assert!(body_path.exists(), "response body artifact must exist");
    let bytes = std::fs::read(&body_path).unwrap();
    let sha = tool_registry::hash::sha256_hex(&bytes);
    assert_eq!(data["body_sha256"].as_str().unwrap(), sha);
    let artifact = env
        .artifacts
        .iter()
        .find(|a| a.uri.ends_with("body.bin"))
        .expect("body.bin registered as artifact");
    assert_eq!(artifact.role, "primary");
    assert_eq!(artifact.sha256, sha);
    assert_eq!(artifact.size_bytes as usize, bytes.len());

    // 开发库副本：tool_run 终态 + tool/tool_version 登记行在库。
    let run_rows: Vec<(String, String, String)> = pool
        .with_conn(|c| {
            let mut stmt = c.prepare("SELECT run_id, status, tool_id FROM tool_run")?;
            let rows = stmt
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
                .filter_map(|r| r.ok())
                .collect();
            Ok::<Vec<_>, database::rusqlite::Error>(rows)
        })
        .unwrap();
    assert!(
        run_rows
            .iter()
            .any(|(run_id, status, tool_id)| run_id == "rn-p33-ok"
                && status == "SUCCEEDED"
                && tool_id == "rip.http.inspect"),
        "dev copy must hold the SUCCEEDED rip.http.inspect run: {run_rows:?}"
    );
    let tool_row: i64 = pool
        .with_conn(|c| {
            c.query_row(
                "SELECT COUNT(*) FROM tool WHERE tool_id = 'rip.http.inspect'",
                [],
                |r| r.get(0),
            )
        })
        .unwrap();
    assert_eq!(tool_row, 1, "registry registration must land in dev copy");
    let version_rows: i64 = pool
        .with_conn(|c| {
            c.query_row(
                "SELECT COUNT(*) FROM tool_version WHERE tool_id = 'rip.http.inspect'",
                [],
                |r| r.get(0),
            )
        })
        .unwrap();
    assert!(version_rows >= 1, "tool_version row must exist");

    // 最小功能验证：HEAD（无响应体、无 body_artifact_id）。
    let head = dispatch_and_run(
        &pool,
        &engine,
        &manifest,
        &ws_root,
        "rn-p33-head",
        json!({"url": "https://example.com/", "method": "HEAD"}),
    );
    assert_eq!(head.envelope.status, ToolRunStatus::Succeeded);
    let head_data = head.envelope.result.as_ref().expect("head result");
    assert_eq!(head_data["status_code"].as_i64(), Some(200));
    assert_eq!(head_data["body_bytes"].as_i64(), Some(0));
    assert!(head_data.get("body_artifact_id").is_none());

    // 最小功能验证：save_body=false（不落响应体，仅 result.json Artifact）。
    let nosave = dispatch_and_run(
        &pool,
        &engine,
        &manifest,
        &ws_root,
        "rn-p33-nosave",
        json!({"url": "https://example.com/", "save_body": false}),
    );
    assert_eq!(nosave.envelope.status, ToolRunStatus::Succeeded);
    let nosave_data = nosave.envelope.result.as_ref().expect("nosave result");
    assert!(nosave_data.get("body_artifact_id").is_none());
    assert!(
        !ws_root.join("rn-p33-nosave/out/body.bin").exists(),
        "save_body=false must not write the body artifact"
    );

    // 登记面负向：method=POST 被 input_schema 在提交面拒绝（T1 收敛）。
    let rejected = engine.submit(&submit_request(
        "rn-p33-post",
        json!({"url": "https://example.com/", "method": "POST"}),
    ));
    assert!(
        rejected.is_err(),
        "POST must be rejected by input_schema (T1 read-only face)"
    );

    // 容器面清零（验证载具用后即删；开发库副本与工作区按批次要求保留）。
    let leftover = Command::new("podman")
        .args([
            "ps",
            "-aq",
            "--filter",
            "label=rip.runtime=rip-tool",
            "--format",
            "{{.Names}}",
        ])
        .output()
        .expect("podman ps spawn");
    assert!(
        String::from_utf8_lossy(&leftover.stdout).trim().is_empty(),
        "no rip-tool containers may remain after the batch"
    );
}

#[test]
fn manifest_matches_contract_section_9_1_shape() {
    // 输出格式验证（计划 §4.3 第 3 项）的库面部分：双份 Manifest 字段与
    // §9.1 一致（method 枚举、save_body、body_artifact_id），且 registry
    // 校验九条接受真实 digest 引用（无需 podman）。
    let manifest = registered_manifest();
    assert_eq!(manifest["id"], json!("rip.http.inspect"));
    assert_eq!(manifest["runtime"]["type"], json!("podman_host"));
    let image = manifest["runtime"]["image"].as_str().unwrap();
    assert!(
        image.ends_with("@sha256:085b4ef304e990be18d8f6aa917ec0c04c33223486d2f971544493176e288c58"),
        "manifest image must be digest-pinned to the local build: {image}"
    );
    assert_eq!(
        manifest["input_schema"]["properties"]["method"]["enum"],
        json!(["GET", "HEAD"])
    );
    assert_eq!(
        manifest["input_schema"]["properties"]["save_body"]["default"],
        json!(true)
    );
    assert!(
        manifest["output_schema"]["properties"]["body_artifact_id"].is_object(),
        "output schema must declare body_artifact_id (§9.1)"
    );
    assert_eq!(
        manifest["output_schema"]["required"],
        json!(["status_code", "final_url"])
    );
    assert_eq!(manifest["network"]["mode"], json!("egress_allowlist"));
    assert_eq!(manifest["permission_level"], json!("T1"));

    // Registry 校验九条接受（占位 digest 与 tag 引用在本机已实测拒绝）。
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
    registry
        .register(&manifest)
        .expect("manifest must pass rule 9");
}
