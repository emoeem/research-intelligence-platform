//! P6-3 布局契约测试（ROADMAP §7.3 P6-3 完成判据）：
//! - §8 目录树（registry/、installed/<id>/<version>/ 含 manifest.yaml、
//!   capabilities/、docs/、data/、checksums.sha256、disabled/、cache/、
//!   state/、logs/）在临时目录建立；
//! - RIP_DATA_DIR 重定位与 rip.plugins.dir 单独配置实测生效且互不干扰；
//! - state/plugin-state.json 与 transitions.jsonl 双写随操作同步；
//! - 拉取落位（stage_package）路径穿越拒绝。
//!
//! 测试纪律：全部临时目录 + 内存库，不触 platform*.db。

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use database::{DbPath, Pool, PoolConfig};
use domain::plugin::PluginTrust;
use plugin_runtime::{
    PluginPaths, PluginRuntime, ResolveOptions, StateStore, TRANSITIONS_FILE_NAME,
};
use serde_json::Value;

fn unique(tag: &str) -> PathBuf {
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("p6layout-{tag}-{}-{n}", std::process::id()));
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

fn env_mock(value: Option<String>) -> impl Fn(&str) -> Option<String> {
    move |key: &str| {
        if key == plugin_runtime::ENV_DATA_DIR {
            value.clone()
        } else {
            None
        }
    }
}

const KNOWLEDGE_YAML: &str = "schemas/plugin-manifest/examples/knowledge.rtis-redteam.yaml";

/// 构建来源包：manifest.yaml（真实契约示例）+ README.md（P6-5 对齐后的
/// capability entry）+ capabilities/ + docs/ + data/。
fn build_source_package(root: &Path) -> (PathBuf, PathBuf) {
    let src = root.join("src-pkg");
    let manifest_src = root.join("manifest.yaml");
    fs::create_dir_all(src.join("capabilities")).unwrap();
    fs::create_dir_all(src.join("docs")).unwrap();
    fs::create_dir_all(src.join("data")).unwrap();
    fs::copy(repo_root().join(KNOWLEDGE_YAML), &manifest_src).unwrap();
    fs::write(src.join("README.md"), "# RTIS 红队知识包\n").unwrap();
    fs::write(
        src.join("capabilities/knowledge_source.yaml"),
        "corpus_id: rtis-redteam\n",
    )
    .unwrap();
    fs::write(src.join("docs/usage.md"), "# usage\n").unwrap();
    fs::write(src.join("data/index.bin"), vec![1u8, 2, 3]).unwrap();
    (manifest_src, src)
}

// ---------------------------------------------------------------------------
// 定位解析：缺省、RIP_DATA_DIR 重定位、显式优先级
// ---------------------------------------------------------------------------

#[test]
fn resolve_precedence_default_env_explicit() {
    let dep = unique("dep");
    // 缺省：随部署根（deployment_root 模拟）取 rip-data 相对名。
    let paths = PluginPaths::resolve(ResolveOptions {
        deployment_root: Some(&dep),
        ..Default::default()
    })
    .unwrap();
    assert_eq!(
        paths.data_dir,
        dep.join("rip-data"),
        "default data dir is <deployment_root>/rip-data"
    );
    assert_eq!(paths.plugins_dir, dep.join("rip-data").join("plugins"));

    // RIP_DATA_DIR 覆盖缺省。
    let relocated = unique("env");
    let paths = PluginPaths::resolve(ResolveOptions {
        deployment_root: Some(&dep),
        env: Some(&env_mock(Some(relocated.to_string_lossy().to_string()))),
        ..Default::default()
    })
    .unwrap();
    assert_eq!(paths.data_dir, relocated, "RIP_DATA_DIR wins over default");
    assert_eq!(paths.plugins_dir, relocated.join("plugins"));

    // 显式值压过 env（rip.data_dir 优先级最高）。
    let explicit = unique("explicit");
    let paths = PluginPaths::resolve(ResolveOptions {
        deployment_root: Some(&dep),
        data_dir: Some(&explicit),
        env: Some(&env_mock(Some(relocated.to_string_lossy().to_string()))),
        ..Default::default()
    })
    .unwrap();
    assert_eq!(
        paths.data_dir, explicit,
        "explicit rip.data_dir wins over env"
    );
}

// ---------------------------------------------------------------------------
// rip.plugins.dir 单独配置与 data_dir 互不干扰
// ---------------------------------------------------------------------------

#[test]
fn plugins_dir_override_is_independent() {
    let data_dir = unique("data");
    let plugins_dir = unique("plugins");
    let paths = PluginPaths::resolve(ResolveOptions {
        data_dir: Some(&data_dir),
        plugins_dir: Some(&plugins_dir),
        ..Default::default()
    })
    .unwrap();
    assert_eq!(
        paths.plugins_dir, plugins_dir,
        "rip.plugins.dir 单独配置生效"
    );
    assert_eq!(paths.data_dir, data_dir);
    let report = paths.ensure_layout().unwrap();
    // 六子目录全部在插件根之下。
    for name in [
        "registry",
        "installed",
        "disabled",
        "cache",
        "state",
        "logs",
    ] {
        let dir = plugins_dir.join(name);
        assert!(dir.is_dir(), "{name} under plugins root: {report:?}");
    }
    // data_dir 一侧不产生 plugins/（互不干扰）。
    assert!(
        !data_dir.join("plugins").exists(),
        "no plugins dir under data_dir"
    );
    assert!(!plugins_dir.join("plugins").exists());
    // state 文件定位在插件根的 state/ 下。
    assert_eq!(
        paths.state_file(),
        plugins_dir.join("state/plugin-state.json")
    );
    assert_eq!(
        paths.transitions_file(),
        plugins_dir.join("state/transitions.jsonl")
    );
    // ensure 幂等：第二次全量 existed。
    let again = paths.ensure_layout().unwrap();
    assert!(again.created.is_empty(), "second ensure creates nothing");
    assert_eq!(again.existed.len(), 7, "plugins root + 6 subdirs existed");
}

// ---------------------------------------------------------------------------
// §8 全树：installed/<id>/<version>/ 含 manifest.yaml、capabilities/、
// docs/、data/、checksums.sha256（+ plugin.lock），install 经拉取落位
// ---------------------------------------------------------------------------

#[test]
fn full_contract8_tree_via_stage_and_install() {
    let root = unique("tree");
    let paths = PluginPaths::resolve(ResolveOptions {
        data_dir: Some(&root.join("data")),
        ..Default::default()
    })
    .unwrap();
    paths.ensure_layout().unwrap();

    let pool = memory_pool();
    database::migrate(&pool, &migrations_dir()).unwrap();
    let audit = audit_engine::AuditEngine::new(pool.clone());
    let rt = PluginRuntime::open(pool.clone(), audit).with_data_root(root.join("data"));

    let (manifest_src, src_pkg) = build_source_package(&root);
    let manifest_doc = fs::read_to_string(&manifest_src).unwrap();
    let manifest = plugin_registry::parse_document(&manifest_doc).unwrap();

    // 拉取落位 + 安装（校验和与 lock 由安装链生成）。
    let staged = paths
        .stage_package(
            "knowledge.rtis-redteam",
            "0.1.0",
            &manifest_src,
            Some(&src_pkg),
        )
        .unwrap();
    assert_eq!(
        staged,
        paths
            .installed_root()
            .join("knowledge.rtis-redteam")
            .join("0.1.0"),
        "stage target is installed/<id>/<version>"
    );
    rt.install("user:test", &manifest, &staged, PluginTrust::Local)
        .unwrap();

    // §8 版本目录树逐项断言。
    assert!(
        staged.join("manifest.yaml").is_file(),
        "manifest.yaml at version root"
    );
    assert!(staged.join("capabilities/knowledge_source.yaml").is_file());
    assert!(staged.join("docs/usage.md").is_file());
    assert!(staged.join("data/index.bin").is_file());
    assert!(
        staged.join("checksums.sha256").is_file(),
        "install writes checksums"
    );
    assert!(
        staged.join("plugin.lock").is_file(),
        "install writes plugin.lock"
    );

    // §8 顶层骨架（installed/disabled/cache/state/logs/registry 全在位）。
    for name in ["registry", "disabled", "cache", "state", "logs"] {
        assert!(paths.plugins_dir.join(name).is_dir(), "{name} exists");
    }
    // 安装目录记录进 DB 行（uninstall 处置的输入）。
    let row = rt
        .store()
        .get_active("knowledge.rtis-redteam")
        .unwrap()
        .unwrap();
    assert_eq!(
        row.installed_dir.as_deref(),
        Some(staged.to_string_lossy().as_ref())
    );
}

// ---------------------------------------------------------------------------
// state/ 文件承载：快照随操作同步 + transitions.jsonl 追加可读回
// ---------------------------------------------------------------------------

#[test]
fn state_files_sync_with_lifecycle_ops() {
    let root = unique("state");
    let paths = PluginPaths::resolve(ResolveOptions {
        data_dir: Some(&root.join("data")),
        ..Default::default()
    })
    .unwrap();
    paths.ensure_layout().unwrap();
    let state_store = StateStore::open(&paths);

    let pool = memory_pool();
    database::migrate(&pool, &migrations_dir()).unwrap();
    let audit = audit_engine::AuditEngine::new(pool.clone());
    let rt = PluginRuntime::open(pool.clone(), audit).with_data_root(root.join("data"));

    let (manifest_src, src_pkg) = build_source_package(&root);
    let manifest_doc = fs::read_to_string(&manifest_src).unwrap();
    let manifest = plugin_registry::parse_document(&manifest_doc).unwrap();
    let staged = paths
        .stage_package(
            "knowledge.rtis-redteam",
            "0.1.0",
            &manifest_src,
            Some(&src_pkg),
        )
        .unwrap();
    rt.install("user:test", &manifest, &staged, PluginTrust::Local)
        .unwrap();

    // install 后：快照 REGISTERED，流水含 install + register 两行。
    state_store
        .append_transitions(&[
            serde_json::json!({"action": "install", "plugin_id": "knowledge.rtis-redteam",
                "version": "0.1.0", "from": null, "to": "INSTALLED"}),
            serde_json::json!({"action": "register", "plugin_id": "knowledge.rtis-redteam",
                "version": "0.1.0", "from": "INSTALLED", "to": "REGISTERED"}),
        ])
        .unwrap();
    let rows = rt.store().list().unwrap();
    state_store.write_snapshot(&rows).unwrap();
    let snap: Value = state_store.read_snapshot().unwrap();
    assert_eq!(snap["plugins"][0]["state"], "REGISTERED");
    assert_eq!(snap["plugins"][0]["plugin_id"], "knowledge.rtis-redteam");
    let transitions = state_store.read_transitions().unwrap();
    assert_eq!(
        transitions.len(),
        2,
        "install + register lines: {transitions:?}"
    );
    assert_eq!(transitions[0]["action"], "install");
    assert_eq!(transitions[1]["to"], "REGISTERED");

    // enable 再 disable（REGISTERED 直禁非法，§5.2）；快照翻转 DISABLED，流水追加两行。
    rt.enable("user:test", "knowledge.rtis-redteam").unwrap();
    rt.disable("user:test", "knowledge.rtis-redteam").unwrap();
    let rows = rt.store().list().unwrap();
    state_store.write_snapshot(&rows).unwrap();
    let snap: Value = state_store.read_snapshot().unwrap();
    assert_eq!(
        snap["plugins"][0]["state"], "DISABLED",
        "snapshot mirrors DB after disable"
    );
    state_store
        .append_transitions(&[serde_json::json!({"action": "enable",
            "plugin_id": "knowledge.rtis-redteam", "version": "0.1.0",
            "from": "REGISTERED", "to": "ENABLED"})])
        .unwrap();
    state_store
        .append_transitions(&[serde_json::json!({"action": "disable",
            "plugin_id": "knowledge.rtis-redteam", "version": "0.1.0",
            "from": "ENABLED", "to": "DISABLED"})])
        .unwrap();
    assert_eq!(state_store.read_transitions().unwrap().len(), 4);
    assert!(paths.transitions_file().file_name().unwrap() == TRANSITIONS_FILE_NAME);
}

// ---------------------------------------------------------------------------
// 拉取落位安全：路径穿越拒绝 + 重装清空重建
// ---------------------------------------------------------------------------

#[test]
fn stage_rejects_traversal_and_reinstall_resets() {
    let root = unique("stage");
    let paths = PluginPaths::resolve(ResolveOptions {
        data_dir: Some(&root.join("data")),
        ..Default::default()
    })
    .unwrap();
    paths.ensure_layout().unwrap();
    let (manifest_src, _src_pkg) = build_source_package(&root);

    // 穿越 id 拒绝。
    let err = paths
        .stage_package("../escape", "0.1.0", &manifest_src, None)
        .unwrap_err();
    assert_eq!(err.code, "id_unsafe");
    assert!(err.to_json_string().contains("id_unsafe"));
    assert!(!root.join("data").join("plugins").join("escape").exists());
    // 绝对 version 拒绝。
    let err2 = paths
        .stage_package("knowledge.rtis-redteam", "/abs", &manifest_src, None)
        .unwrap_err();
    assert_eq!(err2.code, "id_unsafe");

    // 重装清空重建：第一次放多余文件，第二次（同 id/version）后消失。
    let (_, src_pkg) = build_source_package(&root);
    let staged = paths
        .stage_package(
            "knowledge.rtis-redteam",
            "0.2.0",
            &manifest_src,
            Some(&src_pkg),
        )
        .unwrap();
    fs::write(staged.join("stale.bin"), b"old").unwrap();
    assert!(staged.join("stale.bin").exists());
    let staged2 = paths
        .stage_package(
            "knowledge.rtis-redteam",
            "0.2.0",
            &manifest_src,
            Some(&src_pkg),
        )
        .unwrap();
    assert_eq!(staged, staged2);
    assert!(
        !staged2.join("stale.bin").exists(),
        "reinstall resets the directory"
    );
    assert!(staged2.join("manifest.yaml").is_file());
}
