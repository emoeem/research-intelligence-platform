//! P10-2 插件生态收口测试（ROADMAP P10-1/P10-2 判据）：
//! - D10 门控：update_check 自动提示仅 official 放行（community/local/remote
//!   拒绝且 Emit plugin.update_check result=blocked）；manual 检查全档可用；
//! - 安装确认强度分级面（plugin_registry::trust_policy）与 D10 表一致性；
//! - 回滚窗口清理（§5.3 第 5 步 + D6）：未确认拒绝；确认后旧版本跃迁
//!   UNINSTALLED、data_root 之下目录物理删除、active 版本与 data_root 之外
//!   目录不动；清理幂等。
//!
//! 红线：全部用内存库与临时目录，不触 migrations/ 下任何 platform*.db。

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use audit_engine::plugin_actions;
use database::{DbPath, Pool, PoolConfig};
use domain::plugin::PluginTrust;
use plugin_registry::parse_document;
use plugin_runtime::{PluginRuntime, UpgradeOutcome};
use serde_json::Value;

const KNOWLEDGE_EXAMPLE: &str = "schemas/plugin-manifest/examples/knowledge.rtis-redteam.json";

fn memory_pool() -> Pool {
    Pool::open(PoolConfig {
        path: DbPath::Memory,
        readonly: false,
        max_connections: 1,
        busy_timeout: Duration::from_secs(5),
    })
    .unwrap()
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn migrations_dir() -> PathBuf {
    repo_root().join("migrations")
}

fn fresh_runtime(tag: &str) -> (Pool, PluginRuntime, PathBuf) {
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!("p10trust-{tag}-{}-{n}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let data_root = root.join("data_root");
    fs::create_dir_all(&data_root).unwrap();
    let pool = memory_pool();
    database::migrate(&pool, &migrations_dir()).unwrap();
    let audit = audit_engine::AuditEngine::new(pool.clone());
    let rt = PluginRuntime::open(pool.clone(), audit).with_data_root(&data_root);
    (pool, rt, data_root)
}

fn read_example(path: &str) -> Value {
    let doc = fs::read_to_string(repo_root().join(path)).unwrap();
    parse_document(&doc).unwrap()
}

fn build_package(data_root: &Path, tag: &str) -> PathBuf {
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let pkg = data_root.join(format!("pkg-{tag}-{n}"));
    fs::create_dir_all(&pkg).unwrap();
    fs::write(pkg.join("README.md"), "# RTIS 红队知识包\n").unwrap();
    let entry = pkg.join("capabilities/knowledge_source.yaml");
    fs::create_dir_all(entry.parent().unwrap()).unwrap();
    fs::write(&entry, "entry stub: capabilities/knowledge_source.yaml\n").unwrap();
    pkg
}

fn audit_events(pool: &Pool, action: &str) -> Vec<Value> {
    pool.with_conn(|c| {
        let mut stmt = c
            .prepare("SELECT detail FROM audit_event WHERE action = ?1 ORDER BY event_id")
            .unwrap();
        let rows = stmt
            .query_map([action], |r| r.get::<_, String>(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        Ok::<Vec<String>, database::rusqlite::Error>(rows)
    })
    .unwrap()
    .into_iter()
    .map(|d| serde_json::from_str(&d).unwrap())
    .collect()
}

fn upgraded_manifest(v1: &Value, version: &str) -> Value {
    let mut m = v1.clone();
    m.as_object_mut()
        .unwrap()
        .insert("version".into(), Value::from(version));
    m
}

// ---------------------------------------------------------------------------
// D10：update_check 自动提示仅 official；manual 全档可用
// ---------------------------------------------------------------------------

#[test]
fn d10_auto_update_prompt_gate() {
    let (pool, rt, data_root) = fresh_runtime("d10");
    let manifest_v1 = read_example(KNOWLEDGE_EXAMPLE);
    let pkg_v1 = build_package(&data_root, "v1");
    rt.install("user:test", &manifest_v1, &pkg_v1, PluginTrust::Local)
        .unwrap();

    // local：auto 被拒（机器可读错误码），manual 不受影响。
    let err = rt
        .update_check_auto("user:test", "knowledge.rtis-redteam", "0.2.0")
        .unwrap_err();
    assert_eq!(err.code, "update_check_auto_not_allowed");
    assert!(rt.update_check("knowledge.rtis-redteam", "0.2.0").unwrap());
    // blocked 留痕（audit-engine Emit，D10 门控审计面）。
    let events = audit_events(&pool, plugin_actions::UPDATE_CHECK);
    assert_eq!(events.len(), 1);
    assert_eq!(events[0]["mode"], "auto");
    assert_eq!(events[0]["result"], "blocked");
    assert_eq!(events[0]["trust_level"], "local");

    // community / remote：同样拒绝（错误码一致）。
    for trust in [PluginTrust::Community, PluginTrust::Remote] {
        let (pool_t, rt_t, root_t) = fresh_runtime("d10x");
        let pkg = build_package(&root_t, "v1");
        rt_t.install("user:test", &manifest_v1, &pkg, trust)
            .unwrap();
        let err = rt_t
            .update_check_auto("user:test", "knowledge.rtis-redteam", "0.2.0")
            .unwrap_err();
        assert_eq!(err.code, "update_check_auto_not_allowed");
        // manual 仍可用（§6：其余须手动检查，检查本身不被禁）。
        assert!(rt_t
            .update_check("knowledge.rtis-redteam", "0.2.0")
            .unwrap());
        let events = audit_events(&pool_t, plugin_actions::UPDATE_CHECK);
        assert_eq!(events[0]["trust_level"], trust.as_str());
        let _ = pool_t;
    }

    // official：auto 放行，result=prompted（有新版本）。
    let (pool_o, rt_o, root_o) = fresh_runtime("d10official");
    let pkg_o = build_package(&root_o, "v1");
    rt_o.install("user:test", &manifest_v1, &pkg_o, PluginTrust::Official)
        .unwrap();
    assert!(rt_o
        .update_check_auto("user:test", "knowledge.rtis-redteam", "0.2.0")
        .unwrap());
    let events = audit_events(&pool_o, plugin_actions::UPDATE_CHECK);
    assert_eq!(events[0]["result"], "prompted");
    assert_eq!(events[0]["trust_level"], "official");
    // official 无新版：result=no_new_version。
    assert!(!rt_o
        .update_check_auto("user:test", "knowledge.rtis-redteam", "0.1.0")
        .unwrap());
    let events = audit_events(&pool_o, plugin_actions::UPDATE_CHECK);
    assert_eq!(events[1]["result"], "no_new_version");
}

// ---------------------------------------------------------------------------
// 确认强度分级面与 D10 表一致（plugin-registry trust_policy）
// ---------------------------------------------------------------------------

#[test]
fn install_confirmation_ladder_matches_policy_table() {
    let policy = plugin_registry::trust_policy_json();
    let tiers = policy["tiers"].as_array().unwrap();
    assert_eq!(tiers.len(), 4);
    let word_of = |level: &str| -> String {
        tiers
            .iter()
            .find(|t| t["trust_level"].as_str() == Some(level))
            .unwrap()["install_confirmation"]
            .as_str()
            .unwrap()
            .to_string()
    };
    assert_eq!(word_of("official"), "single");
    assert_eq!(word_of("community"), "confirm");
    assert_eq!(word_of("local"), "confirm_with_provenance");
    assert_eq!(word_of("remote"), "double_confirm_typed");
    // D10 列：仅 official true。
    for tier in tiers {
        let expect = tier["trust_level"].as_str() == Some("official");
        assert_eq!(tier["auto_update_prompt"].as_bool().unwrap(), expect);
    }
    // 服务端强制面：remote 需要 confirm + confirm_id，official 无额外字段。
    let fields_of = |level: &str| -> Vec<String> {
        tiers
            .iter()
            .find(|t| t["trust_level"].as_str() == Some(level))
            .unwrap()["required_confirm_fields"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_string())
            .collect()
    };
    assert!(fields_of("official").is_empty());
    assert_eq!(fields_of("local"), vec!["confirm"]);
    assert_eq!(fields_of("remote"), vec!["confirm", "confirm_id"]);
}

// ---------------------------------------------------------------------------
// 回滚窗口清理（§5.3 第 5 步 + D6）
// ---------------------------------------------------------------------------

#[test]
fn rollback_window_cleanup_requires_confirmation_and_respects_red_lines() {
    let (pool, rt, data_root) = fresh_runtime("cleanup");
    let manifest_v1 = read_example(KNOWLEDGE_EXAMPLE);
    let pkg_v1 = build_package(&data_root, "v1");
    rt.install("user:test", &manifest_v1, &pkg_v1, PluginTrust::Local)
        .unwrap();
    let manifest_v2 = upgraded_manifest(&manifest_v1, "0.2.0");
    let pkg_v2 = build_package(&data_root, "v2");
    let outcome: UpgradeOutcome = rt
        .upgrade(
            "user:test",
            "knowledge.rtis-redteam",
            &manifest_v2,
            &pkg_v2,
            PluginTrust::Local,
        )
        .unwrap();
    assert_eq!(outcome.to_version, "0.2.0");

    // 未确认直接拒绝（合同：用户确认后清理）。
    let err = rt
        .cleanup_rollback_window("user:test", "knowledge.rtis-redteam", false)
        .unwrap_err();
    assert_eq!(err.code, "cleanup_requires_confirmation");

    // 确认后：0.1.0 跃迁 UNINSTALLED，data_root 之下目录物理删除，
    // active 0.2.0 与其目录不动。
    let report = rt
        .cleanup_rollback_window("user:test", "knowledge.rtis-redteam", true)
        .unwrap();
    assert_eq!(report.plugin_id, "knowledge.rtis-redteam");
    assert_eq!(report.active_version, "0.2.0");
    assert_eq!(report.cleaned, vec!["0.1.0"]);
    assert_eq!(report.dirs_removed.len(), 1);
    assert!(report.dirs_removed[0].starts_with(data_root.to_string_lossy().as_ref()));
    assert!(
        !pkg_v1.is_dir(),
        "old dir physically removed under data_root"
    );
    assert!(pkg_v2.is_dir(), "active dir untouched");
    let old = rt
        .store()
        .get("knowledge.rtis-redteam", "0.1.0")
        .unwrap()
        .unwrap();
    assert_eq!(old.state, domain::plugin::PluginState::Uninstalled);
    let active = rt
        .store()
        .get_active("knowledge.rtis-redteam")
        .unwrap()
        .unwrap();
    assert_eq!(active.version, "0.2.0");
    // 审计：清理 Emit plugin.uninstall（result=rollback_window_cleanup）。
    let events = audit_events(&pool, plugin_actions::UNINSTALL);
    let cleanup_events: Vec<&Value> = events
        .iter()
        .filter(|e| e["result"] == "rollback_window_cleanup")
        .collect();
    assert_eq!(cleanup_events.len(), 1);
    assert_eq!(cleanup_events[0]["active_version"], "0.2.0");

    // 幂等：第二次清理 cleaned 空（历史 UNINSTALLED 行保留不重复处理）。
    let report2 = rt
        .cleanup_rollback_window("user:test", "knowledge.rtis-redteam", true)
        .unwrap();
    assert!(report2.cleaned.is_empty());
    assert_eq!(report2.retained, vec!["0.1.0"]);
}

#[test]
fn rollback_window_cleanup_never_deletes_outside_data_root() {
    let (pool, rt, data_root) = fresh_runtime("redline");
    let manifest_v1 = read_example(KNOWLEDGE_EXAMPLE);
    // 包目录放在 data_root 之外（模拟调用方源目录），红线：绝不删除。
    let outside_root =
        std::env::temp_dir().join(format!("p10trust-outside-{}", std::process::id()));
    let _ = fs::remove_dir_all(&outside_root);
    fs::create_dir_all(&outside_root).unwrap();
    let pkg_v1 = build_package(&outside_root, "v1");
    rt.install("user:test", &manifest_v1, &pkg_v1, PluginTrust::Local)
        .unwrap();
    let manifest_v2 = upgraded_manifest(&manifest_v1, "0.2.0");
    let pkg_v2 = build_package(&data_root, "v2");
    rt.upgrade(
        "user:test",
        "knowledge.rtis-redteam",
        &manifest_v2,
        &pkg_v2,
        PluginTrust::Local,
    )
    .unwrap();

    let report = rt
        .cleanup_rollback_window("user:test", "knowledge.rtis-redteam", true)
        .unwrap();
    assert_eq!(report.cleaned, vec!["0.1.0"]);
    assert!(report.dirs_removed.is_empty());
    assert_eq!(report.dirs_skipped.len(), 1);
    assert_eq!(report.dirs_skipped[0].reason, "outside_data_root");
    assert!(pkg_v1.is_dir(), "red line: caller source dir never deleted");
    // 行仍跃迁 UNINSTALLED（目录守卫不影响状态清理）。
    let old = rt
        .store()
        .get("knowledge.rtis-redteam", "0.1.0")
        .unwrap()
        .unwrap();
    assert_eq!(old.state, domain::plugin::PluginState::Uninstalled);
    let _ = pool;
    let _ = fs::remove_dir_all(&outside_root);
}

#[test]
fn cleanup_requires_installed_plugin() {
    let (_pool, rt, _root) = fresh_runtime("noop");
    let err = rt
        .cleanup_rollback_window("user:test", "knowledge.nonexistent", true)
        .unwrap_err();
    assert_eq!(err.code, "unknown_plugin");
}
