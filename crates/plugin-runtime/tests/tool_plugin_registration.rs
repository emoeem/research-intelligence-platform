//! P6-6 Tool 插件注册契约测试：tool.rip.python-workers 三工具 capability
//! 落 Tool Registry（ROADMAP §7.3 P6-6 完成判据）。
//!
//! 判据：
//! - 三工具（tool.rip.text.parse / rip.text.embed / rip.knowledge.evaluate）
//!   经 install 自动注册，tool-registry catalog::query 可查（tool +
//!   tool_version 行带 provenance）；
//! - runtime 七种之内校验：podman_host（schema 词表内有、计划七种外）拒绝；
//! - remote_runner 注册拒绝（TOOL-CONTRACT D-TC-01 Phase 1）；
//! - 幂等：同版本重注册 skipped。

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use database::{DbPath, Pool, PoolConfig};
use domain::plugin::PluginTrust;
use plugin_registry::parse_document;
use plugin_runtime::capability::tool_register::RUNTIME_SEVEN;
use serde_json::{json, Value};
use tool_registry::catalog;

fn unique(tag: &str) -> PathBuf {
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("p6tool-{tag}-{}-{n}", std::process::id()));
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

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

const TOOL_YAML: &str = "schemas/plugin-manifest/examples/tool.rip.python-workers.yaml";

fn open_runtime(pool: &Pool) -> plugin_runtime::PluginRuntime {
    let audit = audit_engine::AuditEngine::new(pool.clone());
    plugin_runtime::PluginRuntime::open(pool.clone(), audit)
}

fn tool_example() -> Value {
    let doc = fs::read_to_string(repo_root().join(TOOL_YAML)).unwrap();
    parse_document(&doc).unwrap()
}

/// tool 示例的包体：三个 worker 入口 stub（entry 相对路径）。
fn build_worker_package(root: &Path) -> PathBuf {
    let pkg = root.join("pkg");
    fs::create_dir_all(&pkg).unwrap();
    for f in [
        "parsing_worker.py",
        "embedding_worker.py",
        "evaluation_worker.py",
    ] {
        fs::write(pkg.join(f), format!("# {f} stub\n")).unwrap();
    }
    pkg
}

#[test]
fn seven_runtime_vocabulary_excludes_podman_host() {
    assert_eq!(RUNTIME_SEVEN.len(), 7);
    assert!(!RUNTIME_SEVEN.contains(&"podman_host"));
}

// ---------------------------------------------------------------------------
// 主链：install → 三工具自动注册 → catalog::query 可查
// ---------------------------------------------------------------------------

#[test]
fn three_worker_tools_registered_and_queryable() {
    let root = unique("reg");
    let pkg = build_worker_package(&root);
    let manifest = tool_example();
    let pool = memory_pool();
    database::migrate(&pool, &migrations_dir()).unwrap();
    let rt = open_runtime(&pool);

    let outcome = rt
        .install("user:test", &manifest, &pkg, PluginTrust::Local)
        .unwrap();
    assert_eq!(outcome.state_after, "REGISTERED");
    assert_eq!(
        outcome.tool_registration.registered,
        vec![
            "tool.rip.text.parse".to_string(),
            "tool.rip.text.embed".to_string(),
            "tool.rip.knowledge.evaluate".to_string()
        ],
        "three tool capabilities registered"
    );
    assert!(outcome.tool_registration.skipped.is_empty());

    // tool-registry query 可查（tool 行 + tool_version 行带 provenance）。
    for tool_id in [
        "tool.rip.text.parse",
        "tool.rip.text.embed",
        "tool.rip.knowledge.evaluate",
    ] {
        let detail =
            catalog::query(&pool, tool_id).unwrap_or_else(|e| panic!("query {tool_id}: {e}"));
        assert_eq!(detail.tool.tool_id, tool_id);
        assert!(
            detail.versions.iter().any(|v| v.version == "0.1.0"),
            "{tool_id} version 0.1.0 registered"
        );
        let version = detail
            .versions
            .iter()
            .find(|v| v.version == "0.1.0")
            .unwrap();
        let provenance = version.provenance_json.clone().unwrap_or_default();
        assert!(
            provenance.contains("P6-6"),
            "provenance recorded: {provenance}"
        );
    }
    // tool 行落库对账（lifecycle VALIDATED + input_schema 列承载 Schema 原文；
    // binary_ref 列语义 = /runtime/binary，python_worker 行为 NULL 属列契约）。
    let (binary_ref, lifecycle, input_schema): (Option<String>, String, Option<String>) = pool
        .with_conn(|c| {
            c.query_row(
                "SELECT binary_ref, lifecycle, input_schema FROM tool WHERE tool_id = 'tool.rip.text.parse'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .map_err(database::DbError::from)
        })
        .unwrap();
    assert_eq!(lifecycle, "VALIDATED", "tool row lifecycle");
    assert!(
        binary_ref.is_none(),
        "python_worker rows leave binary_ref NULL"
    );
    let schema = input_schema.unwrap_or_default();
    assert!(
        schema.contains("content"),
        "input_schema column carries the capability schema: {schema}"
    );
    // discover 全量可见。
    let all = catalog::discover(&pool).unwrap();
    let ids: Vec<String> = all.iter().map(|t| t.tool_id.clone()).collect();
    for tool_id in [
        "tool.rip.text.parse",
        "tool.rip.text.embed",
        "tool.rip.knowledge.evaluate",
    ] {
        assert!(
            ids.contains(&tool_id.to_string()),
            "discover sees {tool_id}: {ids:?}"
        );
    }
}

// ---------------------------------------------------------------------------
// runtime 七种之外拒绝（podman_host：schema 词表内、计划七种外）
// ---------------------------------------------------------------------------

#[test]
fn capability_runtime_outside_plan_seven_rejected() {
    let root = unique("out");
    let pkg = build_worker_package(&root);
    let mut manifest = tool_example();
    manifest.as_object_mut().unwrap()["capabilities"][0]["runtime"]["type"] = json!("podman_host");
    let pool = memory_pool();
    database::migrate(&pool, &migrations_dir()).unwrap();
    let rt = open_runtime(&pool);
    // schema rule 2 词表（8 种含 podman_host）通过，tool_register 七种校验拒绝。
    let err = rt
        .install("user:test", &manifest, &pkg, PluginTrust::Local)
        .unwrap_err();
    assert_eq!(err.code, "capability_runtime_unknown");
    assert!(err.to_json_string().contains("plan-seven"));
}

// ---------------------------------------------------------------------------
// remote_runner 注册拒绝（TOOL-CONTRACT D-TC-01 Phase 1 口径）
// ---------------------------------------------------------------------------

#[test]
fn remote_runner_registration_refused() {
    let root = unique("rr");
    let pkg = build_worker_package(&root);
    let mut manifest = tool_example();
    manifest.as_object_mut().unwrap()["capabilities"][0]["runtime"] =
        json!({"type": "remote_runner"});
    let pool = memory_pool();
    database::migrate(&pool, &migrations_dir()).unwrap();
    let rt = open_runtime(&pool);
    let err = rt
        .install("user:test", &manifest, &pkg, PluginTrust::Local)
        .unwrap_err();
    assert_eq!(err.code, "runtime_registration_refused");
    assert!(err.to_json_string().contains("D-TC-01"));
}

// ---------------------------------------------------------------------------
// 幂等：同版本重注册 skipped（tool-registry rule 2 语义）
// ---------------------------------------------------------------------------

#[test]
fn reregister_same_version_skipped() {
    let root = unique("idem");
    let pkg = build_worker_package(&root);
    let manifest = tool_example();
    let pool = memory_pool();
    database::migrate(&pool, &migrations_dir()).unwrap();
    let rt = open_runtime(&pool);
    rt.install("user:test", &manifest, &pkg, PluginTrust::Local)
        .unwrap();

    let report = plugin_runtime::capability::tool_register::register_tool_capabilities(
        &pool, &manifest, &pkg,
    )
    .unwrap();
    assert!(report.registered.is_empty(), "no re-registration");
    assert_eq!(report.skipped.len(), 3, "all three skipped as same-version");
    assert!(report.deferred.is_empty());
}
