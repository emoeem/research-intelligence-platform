//! P6-6 外部 Project 插件握手契约测试（ROADMAP §7.3 P6-6）：
//! project_endpoint capability 经 stdio MCP 2024-11-05 握手，工具列表快照
//! sha256 进 plugin.lock.negotiation，MCP 工具映射为 mcp_client runtime 的
//! Tool 行落 Tool Registry（§3.1：MCP 工具映射为 mcp_client runtime 的
//! Tool；§3.4：能力协商记录进 lock）。
//!
//! 测试纪律：stdio 握手用 python3 -I fixture 脚本闭环（真实 rtis_mcp.mjs
//! 握手在批次冒烟中实跑，见 BatchResult check）。

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use database::{DbPath, Pool, PoolConfig};
use domain::plugin::PluginTrust;
use plugin_registry::read_lock;
use plugin_runtime::PluginRuntime;
use serde_json::{json, Value};
use tool_registry::catalog;

fn unique(tag: &str) -> PathBuf {
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("p6ep-{tag}-{}-{n}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn memory_pool() -> Pool {
    Pool::open(PoolConfig {
        path: DbPath::Memory,
        readonly: false,
        max_connections: 1,
        busy_timeout: Duration::from_secs(5),
    })
    .unwrap()
}

fn migrations_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../migrations")
}

fn fixture_script() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/mcp_endpoint_fixture.py")
}

/// project 插件 Manifest（endpoint 指向 fixture 脚本）。
fn endpoint_manifest(fixture: &Path) -> Value {
    json!({
        "schema_version": "1",
        "id": "project.fixture-endpoint",
        "name": "Fixture Endpoint",
        "version": "0.1.0",
        "kind": "project",
        "description": "P6-6 握手契约测试",
        "license": "MIT",
        "source": {"type": "local_dir", "path": ".", "pinned": {"kind": "commit"}},
        "compatibility": {"rip_core": ">=0.2.0, <1.0.0", "manifest_schema": ["1"], "runtimes": ["mcp_client"]},
        "capabilities": [{"id": "project-endpoint.fixture.mcp", "type": "project_endpoint",
            "entry": "capabilities/project-endpoint.fixture.mcp.yaml", "summary": "s",
            "runtime": {"type": "mcp_client", "protocol": "mcp/2024-11-05"},
            "provides": {
                          "endpoint": format!("[\"python3\", \"-I\", \"{}\"]", fixture.display()),
                          "tool_id_prefix": "fixture.mcp"}}],
        "dependencies": [],
        "permissions": {"data_access": "none"},
        "context": {"summary": "s", "docs": "docs/usage.md"}
    })
}

fn build_project_package(root: &Path) -> PathBuf {
    let pkg = root.join("pkg");
    fs::create_dir_all(pkg.join("docs")).unwrap();
    fs::write(pkg.join("docs/usage.md"), "# usage\n").unwrap();
    pkg
}

#[test]
fn handshake_snapshot_into_lock_and_mcp_client_mapping() {
    let root = unique("handshake");
    let pkg = build_project_package(&root);
    let manifest = endpoint_manifest(&fixture_script());
    let pool = memory_pool();
    database::migrate(&pool, &migrations_dir()).unwrap();
    let audit = audit_engine::AuditEngine::new(pool.clone());
    let rt = PluginRuntime::open(pool.clone(), audit);

    // install：stage（握手 + 快照落盘）→ validate → checksums → lock →
    // REGISTERED → mcp_client 工具映射。
    let outcome = rt
        .install("user:test", &manifest, &pkg, PluginTrust::Remote)
        .unwrap();
    assert_eq!(outcome.state_after, "REGISTERED");
    assert_eq!(outcome.endpoint_registration.len(), 1);
    let report = &outcome.endpoint_registration[0];
    assert_eq!(report.capability_id, "project-endpoint.fixture.mcp");
    assert_eq!(report.protocol_version, "2024-11-05");
    assert_eq!(report.server_info["name"], "fixture-endpoint");
    assert_eq!(report.tool_count, 2);
    assert_eq!(
        report.registered,
        vec![
            "fixture.mcp.fixture_echo".to_string(),
            "fixture.mcp.fixture_stats".to_string()
        ]
    );
    assert_eq!(report.tools_sha256.len(), 64);

    // 工具列表快照哈希写进 plugin.lock（§3.4 能力协商记录进 lock）。
    let lock = read_lock(&pkg).unwrap();
    let negotiation = lock.negotiation.as_ref().expect("negotiation in lock");
    assert_eq!(negotiation["tools_sha256"], json!(report.tools_sha256));
    let endpoints = negotiation["endpoints"].as_array().unwrap();
    assert_eq!(endpoints.len(), 1);
    assert_eq!(
        endpoints[0]["capability_id"],
        "project-endpoint.fixture.mcp"
    );
    // 快照哈希与 tools 明细重算对账（与握手实现同口径）。
    assert_eq!(
        report.tools_sha256,
        plugin_registry::hash::sha256_hex(endpoints[0]["tools"].to_string().as_bytes())
    );
    // 快照描述文件与 negotiation.json 在包内（checksums 覆盖）。
    assert!(pkg.join("negotiation.json").is_file());
    assert!(pkg
        .join("capabilities/project-endpoint.fixture.mcp.yaml")
        .is_file());

    // mcp_client 映射行：Tool Registry 可查，runtime = mcp_client。
    let detail =
        catalog::query(&pool, "fixture.mcp.fixture_echo").unwrap_or_else(|e| panic!("query: {e}"));
    assert_eq!(detail.tool.tool_id, "fixture.mcp.fixture_echo");
    let provenance = detail
        .versions
        .first()
        .and_then(|v| v.provenance_json.clone())
        .unwrap_or_default();
    assert!(
        provenance.contains("internal-kit"),
        "kit provenance: {provenance}"
    );
}

#[test]
fn endpoint_missing_rejected_before_handshake() {
    let root = unique("noep");
    let pkg = build_project_package(&root);
    let mut manifest = endpoint_manifest(&fixture_script());
    manifest.as_object_mut().unwrap()["capabilities"][0]["provides"]
        .as_object_mut()
        .unwrap()
        .remove("endpoint");
    let pool = memory_pool();
    database::migrate(&pool, &migrations_dir()).unwrap();
    let audit = audit_engine::AuditEngine::new(pool.clone());
    let rt = PluginRuntime::open(pool.clone(), audit);
    let err = rt
        .install("user:test", &manifest, &pkg, PluginTrust::Remote)
        .unwrap_err();
    assert_eq!(err.code, "endpoint_missing");
}

#[test]
fn url_form_endpoint_rejected_this_batch() {
    let root = unique("urlep");
    let pkg = build_project_package(&root);
    let mut manifest = endpoint_manifest(&fixture_script());
    manifest.as_object_mut().unwrap()["capabilities"][0]["provides"]["endpoint"] =
        json!("http://127.0.0.1:9/mcp");
    let pool = memory_pool();
    database::migrate(&pool, &migrations_dir()).unwrap();
    let audit = audit_engine::AuditEngine::new(pool.clone());
    let rt = PluginRuntime::open(pool.clone(), audit);
    let err = rt
        .install("user:test", &manifest, &pkg, PluginTrust::Remote)
        .unwrap_err();
    assert_eq!(err.code, "endpoint_form_unsupported");
}
