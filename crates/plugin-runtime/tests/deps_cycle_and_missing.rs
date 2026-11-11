//! P6-7 三层依赖与兼容检查契约测试（ROADMAP §7.3 P6-7 完成判据）：
//! - dependencies 缺失或无满足版本拒绝 enable 且输出机器可读 JSON 清单；
//! - requires 指向被禁用或已卸载插件拒绝 enable 并提示原因；
//! - 依赖环用例拒绝（A→B→A 与 A→A）；
//! - compatibility.rip_core 区间外拒绝 enable（平台版本面注入模拟平台变化）；
//! - 健康检查失败进 ERROR 且不做密集轮询（间隔下限 1s，失败插件从后续
//!   周期剔除即停止重试）。

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use database::{DbPath, Pool, PoolConfig};
use domain::plugin::PluginTrust;
use plugin_runtime::{
    compat, probe_always_ok, probe_stdio_handshake, HealthAction, HealthMonitor, PluginRuntime,
    ProbeOutcome,
};
use serde_json::{json, Value};

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

fn fresh_runtime(tag: &str) -> (Pool, PluginRuntime, PathBuf) {
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!("p6dep-rt-{tag}-{}-{n}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let pool = memory_pool();
    database::migrate(&pool, &migrations_dir()).unwrap();
    let audit = audit_engine::AuditEngine::new(pool.clone());
    let rt = PluginRuntime::open(pool.clone(), audit).with_data_root(root.join("data_root"));
    (pool, rt, root.join("data_root"))
}

/// 最小 knowledge 插件 Manifest（可叠加 dependencies / requires / compat）。
fn base_manifest(id: &str, version: &str) -> Value {
    json!({
        "schema_version": "1",
        "id": id,
        "name": id,
        "version": version,
        "kind": "knowledge",
        "description": "P6-7 契约测试",
        "license": "MIT",
        "source": {"type": "local_dir", "path": ".", "pinned": {"kind": "commit"}},
        "compatibility": {"rip_core": ">=0.2.0, <1.0.0", "manifest_schema": ["1"]},
        "capabilities": [{"id": "k.cap", "type": "knowledge_source", "entry": "e.yaml", "summary": "s"}],
        "dependencies": [],
        "context": {"summary": "s", "docs": "d.md"}
    })
}

fn base_pkg(root: &Path, tag: &str) -> PathBuf {
    let pkg = root.join(format!("pkg-{tag}"));
    fs::create_dir_all(&pkg).unwrap();
    fs::write(pkg.join("e.yaml"), "kind: knowledge_source\n").unwrap();
    pkg
}

fn install(rt: &PluginRuntime, manifest: &Value, pkg: &Path) {
    rt.install("user:test", manifest, pkg, PluginTrust::Local)
        .unwrap_or_else(|e| panic!("install {:?}: {}", manifest["id"], e.to_json_string()));
}

fn dep_report_of(err: &plugin_runtime::RuntimeError) -> Value {
    assert_eq!(
        err.code,
        "dependency_check_failed",
        "{}",
        err.to_json_string()
    );
    err.details
        .clone()
        .expect("machine-readable dependency report")
}

// ---------------------------------------------------------------------------
// 判据 1：dependencies 缺失 / 无满足版本 → enable 拒绝 + 机器可读 JSON 清单
// ---------------------------------------------------------------------------

#[test]
fn missing_dependency_rejected_with_json_list() {
    let (pool, rt, data_root) = fresh_runtime("missing");
    let pkg = base_pkg(&data_root, "b");
    let mut manifest = base_manifest("dep.b", "0.1.0");
    manifest.as_object_mut().unwrap()["dependencies"] =
        json!([{"id": "ghost.plugin", "requirement": ">=0.1.0"}]);
    install(&rt, &manifest, &pkg);
    // install（INSTALLED→REGISTERED）不带依赖检查，enable 才是激活层。
    let err = rt.enable("user:test", "dep.b").unwrap_err();
    let report = dep_report_of(&err);
    assert_eq!(report["ok"], json!(false));
    let entry = &report["plugin_deps"][0];
    assert_eq!(entry["id"], "ghost.plugin");
    assert_eq!(entry["status"], "missing");
    assert!(entry["detail"].as_str().unwrap().contains("not installed"));
    // 状态不变（仍 REGISTERED，拒绝不落状态）。
    assert_eq!(
        rt.store().get_active("dep.b").unwrap().unwrap().state,
        domain::plugin::PluginState::Registered
    );
    let _ = pool;
}

#[test]
fn version_unsatisfied_rejected_with_installed_version() {
    let (pool, rt, data_root) = fresh_runtime("ver");
    let pkg_a = base_pkg(&data_root, "a");
    let manifest_a = base_manifest("dep.a", "0.1.0");
    install(&rt, &manifest_a, &pkg_a);
    rt.enable("user:test", "dep.a").unwrap();

    let pkg_b = base_pkg(&data_root, "b");
    let mut manifest_b = base_manifest("dep.b", "0.1.0");
    manifest_b.as_object_mut().unwrap()["dependencies"] =
        json!([{"id": "dep.a", "requirement": ">=0.2.0"}]);
    install(&rt, &manifest_b, &pkg_b);
    let err = rt.enable("user:test", "dep.b").unwrap_err();
    let report = dep_report_of(&err);
    let entry = &report["plugin_deps"][0];
    assert_eq!(entry["status"], "version_unsatisfied");
    assert!(entry["detail"].as_str().unwrap().contains("0.1.0"));
    // 满足区间后 enable 通过（^0.1 区间）：同版本重装走删除重建。
    manifest_b.as_object_mut().unwrap()["dependencies"] =
        json!([{"id": "dep.a", "requirement": "^0.1.0"}]);
    rt.store().delete_version("dep.b", "0.1.0").unwrap();
    let pkg_b2 = base_pkg(&data_root, "b2");
    install(&rt, &manifest_b, &pkg_b2);
    rt.enable("user:test", "dep.b").unwrap();
    let _ = pool;
}

// ---------------------------------------------------------------------------
// 判据 2：requires 指向被禁用 / 已卸载插件 → 拒绝并提示原因
// ---------------------------------------------------------------------------

#[test]
fn capability_requires_blocked_by_disabled_and_uninstalled() {
    let (_pool, rt, data_root) = fresh_runtime("req");
    // 提供方 A（capability k.cap）。
    let pkg_a = base_pkg(&data_root, "a");
    let manifest_a = base_manifest("req.a", "0.1.0");
    install(&rt, &manifest_a, &pkg_a);
    rt.enable("user:test", "req.a").unwrap();

    // 依赖方 B：requires "req.a/k.cap"。
    let pkg_b = base_pkg(&data_root, "b");
    let mut manifest_b = base_manifest("req.b", "0.1.0");
    manifest_b.as_object_mut().unwrap()["capabilities"][0]["requires"] = json!(["req.a/k.cap"]);
    install(&rt, &manifest_b, &pkg_b);

    // A 禁用 → B enable 拒绝（dep_disabled，§7 item 2）。
    rt.disable("user:test", "req.a").unwrap();
    let err = rt.enable("user:test", "req.b").unwrap_err();
    let report = dep_report_of(&err);
    let entry = &report["capability_deps"][0];
    assert_eq!(entry["require"], "req.a/k.cap");
    assert_eq!(entry["status"], "dep_disabled");
    assert!(entry["detail"].as_str().unwrap().contains("DISABLED"));

    // A 卸载 → 拒绝（dep_uninstalled；已入主库知识保留，溯源标记在位）。
    rt.uninstall("user:test", "req.a").unwrap();
    let err2 = rt.enable("user:test", "req.b").unwrap_err();
    let report2 = dep_report_of(&err2);
    assert_eq!(report2["capability_deps"][0]["status"], "dep_uninstalled");

    // A 重装启用 → B enable 通过：uninstall 保留版本行（UNINSTALLED，溯源），
    // 重装走 delete_version 重建 + 新包体（uninstall 已删旧安装目录）。
    rt.store().delete_version("req.a", "0.1.0").unwrap();
    let pkg_a2 = base_pkg(&data_root, "a2");
    install(&rt, &manifest_a, &pkg_a2);
    rt.enable("user:test", "req.a").unwrap();
    rt.enable("user:test", "req.b").unwrap();
}

// ---------------------------------------------------------------------------
// 判据 3：依赖环拒绝（A→B→A 与 A→A）
// ---------------------------------------------------------------------------

#[test]
fn dependency_cycles_rejected() {
    let (_pool, rt, data_root) = fresh_runtime("cycle");
    // A 依赖 B，B 依赖 A（双方都装到 REGISTERED）。
    let mut manifest_a = base_manifest("cyc.a", "0.1.0");
    manifest_a.as_object_mut().unwrap()["dependencies"] =
        json!([{"id": "cyc.b", "requirement": ">=0.1.0"}]);
    let pkg_a = base_pkg(&data_root, "a");
    install(&rt, &manifest_a, &pkg_a);

    let mut manifest_b = base_manifest("cyc.b", "0.1.0");
    manifest_b.as_object_mut().unwrap()["dependencies"] =
        json!([{"id": "cyc.a", "requirement": ">=0.1.0"}]);
    let pkg_b = base_pkg(&data_root, "b");
    install(&rt, &manifest_b, &pkg_b);

    // enable A：环 [cyc.a, cyc.b, cyc.a] → 拒绝（机器可读路径）。
    let err = rt.enable("user:test", "cyc.a").unwrap_err();
    let report = dep_report_of(&err);
    let cycles = report["cycles"].as_array().unwrap();
    assert!(!cycles.is_empty(), "cycle detected: {report}");
    let path: Vec<&str> = cycles[0]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert_eq!(path.first(), Some(&"cyc.a"));
    assert_eq!(path.last(), Some(&"cyc.a"));

    // 自依赖 A→A 同样是环。
    let mut manifest_self = base_manifest("cyc.self", "0.1.0");
    manifest_self.as_object_mut().unwrap()["dependencies"] =
        json!([{"id": "cyc.self", "requirement": ">=0.1.0"}]);
    let pkg_self = base_pkg(&data_root, "self");
    install(&rt, &manifest_self, &pkg_self);
    let err2 = rt.enable("user:test", "cyc.self").unwrap_err();
    assert_eq!(err2.code, "dependency_check_failed");
}

// ---------------------------------------------------------------------------
// 判据 4：compatibility.rip_core 区间外拒绝 enable（平台版本面注入）
// ---------------------------------------------------------------------------

#[test]
fn compat_rip_core_outside_range_rejected_at_enable() {
    let (pool, rt, data_root) = fresh_runtime("compat");
    let pkg = base_pkg(&data_root, "a");
    let manifest = base_manifest("compat.a", "0.1.0");
    install(&rt, &manifest, &pkg);
    rt.enable("user:test", "compat.a").unwrap();
    rt.disable("user:test", "compat.a").unwrap();

    // 平台回滚到 0.1.0（install 与 enable 之间平台变化，§5.3 场景）：
    // re-enable 重过兼容检查 → rip_core ">=0.2.0" 不再满足 → 拒绝。
    let rolled_back = compat::Platform {
        rip_core: "0.1.0".to_string(),
        ..compat::Platform::default()
    };
    let rt_old = PluginRuntime::open(pool.clone(), audit_engine::AuditEngine::new(pool.clone()))
        .with_data_root(data_root.clone())
        .with_platform(rolled_back);
    let err = rt_old.enable("user:test", "compat.a").unwrap_err();
    assert_eq!(err.code, "compat_check_failed");
    let report = err.details.expect("machine-readable compat report");
    assert_eq!(report["rip_core"]["satisfied"], json!(false));
    assert_eq!(report["rip_core"]["platform"], json!("0.1.0"));
    // worker_protocol 声明不一致同样拒绝（同 id 重装先删版本行）。
    let mut m2 = manifest.clone();
    m2.as_object_mut().unwrap()["compatibility"]["worker_protocol"] = json!("v2");
    rt.store().delete_version("compat.a", "0.1.0").unwrap();
    let pkg2 = base_pkg(&data_root, "wp");
    install(&rt, &m2, &pkg2);
    let err2 = rt.enable("user:test", "compat.a").unwrap_err();
    assert_eq!(err2.code, "compat_check_failed");
    let report2 = err2.details.expect("compat report");
    assert_eq!(report2["worker_protocol"]["satisfied"], json!(false));
}

// ---------------------------------------------------------------------------
// 判据 5：健康检查失败进 ERROR、不做密集轮询（间隔下限 1s + 停止重试）
// ---------------------------------------------------------------------------

#[test]
fn health_failure_marks_error_and_stops_retrying() {
    let (_pool, rt, data_root) = fresh_runtime("health");
    // 两个 LOADED 插件：A 健康、B 探测失败。
    let pkg_a = base_pkg(&data_root, "a");
    let manifest_a = base_manifest("health.a", "0.1.0");
    install(&rt, &manifest_a, &pkg_a);
    rt.enable("user:test", "health.a").unwrap();
    rt.load("health.a").unwrap();

    let pkg_b = base_pkg(&data_root, "b");
    let manifest_b = base_manifest("health.b", "0.1.0");
    install(&rt, &manifest_b, &pkg_b);
    rt.enable("user:test", "health.b").unwrap();
    rt.load("health.b").unwrap();

    // 探测注入：A Ok、B Failed。
    let monitor = HealthMonitor::new(
        Duration::from_secs(1),
        Box::new(|plugin_id| {
            if plugin_id == "health.b" {
                ProbeOutcome::Failed("worker ping timeout".to_string())
            } else {
                ProbeOutcome::Ok
            }
        }),
    )
    .unwrap();

    // 三个周期（间隔 1s，共 ~2s 睡眠）：B 在第 1 周期进 ERROR 并剔除。
    let reports = monitor.run_cycles(&rt, 3).unwrap();
    assert_eq!(reports.len(), 3);
    let b_action = reports[0]
        .actions
        .iter()
        .find(|(id, _)| id == "health.b")
        .map(|(_, a)| a);
    assert!(
        matches!(b_action, Some(HealthAction::MarkedError)),
        "cycle 1 marks B error: {:?}",
        b_action
    );
    // 停止重试：后续周期不再检查 B。
    for r in &reports[1..] {
        assert!(
            r.actions.iter().all(|(id, _)| id != "health.b"),
            "B excluded after ERROR (cycle {}): {:?}",
            r.cycle,
            r.actions
        );
    }
    // 状态对账：B ERROR、A 保持 LOADED（不伪装成 ACTIVE）。
    assert_eq!(
        rt.store().get_active("health.b").unwrap().unwrap().state,
        domain::plugin::PluginState::Error
    );
    assert_eq!(
        rt.store().get_active("health.a").unwrap().unwrap().state,
        domain::plugin::PluginState::Loaded
    );

    // 修复路径：ERROR → enable（重过静态+激活层）→ ENABLED → load 回 LOADED。
    rt.enable("user:test", "health.b").unwrap();
    rt.load("health.b").unwrap();
    assert_eq!(
        rt.store().get_active("health.b").unwrap().unwrap().state,
        domain::plugin::PluginState::Loaded
    );
}

#[test]
fn health_interval_below_one_second_rejected() {
    let err = match HealthMonitor::new(Duration::from_millis(500), Box::new(probe_always_ok())) {
        Err(e) => e,
        Ok(_) => panic!("interval below minimum must be rejected"),
    };
    assert_eq!(err.code, "interval_too_short");
    assert!(err.to_json_string().contains("tight-polling"));
}

// ---------------------------------------------------------------------------
// 内置 stdio 握手探测（project_endpoint health_check 声明，§3.4）
// ---------------------------------------------------------------------------

#[test]
fn builtin_stdio_probe_reports_real_endpoint_state() {
    let fixture =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/mcp_endpoint_fixture.py");
    let good = probe_stdio_handshake(vec![
        "python3".to_string(),
        "-I".to_string(),
        fixture.display().to_string(),
    ]);
    assert!(matches!(good("p"), ProbeOutcome::Ok));
    let bad = probe_stdio_handshake(vec![
        "python3".to_string(),
        "-I".to_string(),
        "/nonexistent.py".to_string(),
    ]);
    assert!(matches!(bad("p"), ProbeOutcome::Failed(_)));
}
