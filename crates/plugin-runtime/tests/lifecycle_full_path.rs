//! P6-2 生命周期契约测试（ROADMAP §7.3 P6-2 完成判据）：
//! - 主链 REGISTERED → ENABLED → LOADED → ACTIVE 全路径；
//! - disable 不删数据；re-enable 重过检查；uninstall 保留已入主库知识并
//!   标记来源已卸载；upgrade 失败自动回滚（§5.3/§5.4）；
//! - migrations/009_plugin.sql 测试副本幂等执行两次无差异；
//! - 五类生命周期操作各 Emit 一条 audit-engine 事件且对核心表零直写；
//! - INVALID 处置经 quality-engine PluginGovernor（复用双轴状态机 + Review
//!   队列，PLUGIN-CONTRACT §11.5 既有裁定；dev-dep 环接线）。
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
use plugin_runtime::{PluginRuntime, RetentionAction};
use serde_json::Value;

const KNOWLEDGE_EXAMPLE: &str = "schemas/plugin-manifest/examples/knowledge.rtis-redteam.json";
const TOOL_EXAMPLE: &str = "schemas/plugin-manifest/examples/tool.rip.python-workers.json";

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
    let root = std::env::temp_dir().join(format!("p6rt-{tag}-{}-{n}", std::process::id()));
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

/// 在 data_root 下构建包目录（entry 文件在位，内容为占位正文）。
fn build_package(data_root: &Path, tag: &str, entries: &[&str]) -> PathBuf {
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let pkg = data_root.join(format!("pkg-{tag}-{n}"));
    fs::create_dir_all(&pkg).unwrap();
    // P6-5 对齐：knowledge 示例 capability entry = README.md（真实 RTIS 树口径）。
    fs::write(pkg.join("README.md"), "# RTIS 红队知识包\n").unwrap();
    for entry in entries {
        let target = pkg.join(entry);
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::write(&target, format!("entry stub: {entry}\n")).unwrap();
    }
    pkg
}

fn audit_counts(pool: &Pool) -> Vec<(String, i64)> {
    pool.with_conn(|c| {
        let mut stmt = c
            .prepare("SELECT action, COUNT(*) FROM audit_event WHERE action LIKE 'plugin.%' GROUP BY action ORDER BY action")
            .unwrap();
        let rows = stmt
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))
            .unwrap();
        let mut out = Vec::new();
        for r in rows {
            out.push(r.unwrap());
        }
        Ok::<Vec<(String, i64)>, database::rusqlite::Error>(out)
    })
    .unwrap()
}

fn core_table_counts(pool: &Pool) -> Vec<(String, i64)> {
    // 核心表集合：插件数据面之外的代表性实体表（001..008 全部建表的核心行）。
    const CORE: [&str; 10] = [
        "resource",
        "resource_source",
        "tool",
        "tool_version",
        "tool_run",
        "artifact",
        "agent_definition",
        "agent_run",
        "context_manifest",
        "review",
    ];
    CORE.iter()
        .map(|t| {
            let n: i64 = pool
                .with_conn(|c| {
                    c.query_row(&format!("SELECT COUNT(*) FROM {t}"), [], |r| r.get(0))
                        .map_err(database::DbError::from)
                })
                .unwrap_or(-1);
            (t.to_string(), n)
        })
        .collect()
}

// ---------------------------------------------------------------------------
// 主链四态 + disable（不删数据）+ re-enable（重过检查）+ uninstall（保留 +
// 来源已卸载标记）
// ---------------------------------------------------------------------------

#[test]
fn main_chain_and_disable_reenable_uninstall() {
    let (pool, rt, data_root) = fresh_runtime("chain");
    let manifest = read_example(TOOL_EXAMPLE);
    let pkg = build_package(
        &data_root,
        "tool",
        &[
            "parsing_worker.py",
            "embedding_worker.py",
            "evaluation_worker.py",
        ],
    );

    // install：INSTALLED → REGISTERED（capabilities 同步登记）。
    let installed = rt
        .install("user:test", &manifest, &pkg, PluginTrust::Local)
        .unwrap();
    assert_eq!(installed.state_after, "REGISTERED");
    assert_eq!(installed.plugin_id, "tool.rip.python-workers");
    let row = rt
        .store()
        .get_active("tool.rip.python-workers")
        .unwrap()
        .unwrap();
    assert_eq!(row.state, domain::plugin::PluginState::Registered);
    assert!(row.is_active);
    assert!(row
        .lock_sha256
        .as_deref()
        .map(|s| s.len() == 64)
        .unwrap_or(false));
    let caps = rt
        .store()
        .capabilities("tool.rip.python-workers", "0.1.0")
        .unwrap();
    assert_eq!(caps.len(), 3, "tool example declares 3 capabilities");
    assert!(pkg.join("checksums.sha256").is_file());
    assert!(pkg.join("plugin.lock").is_file());

    // 主链：REGISTERED → ENABLED → LOADED → ACTIVE。
    rt.enable("user:test", "tool.rip.python-workers").unwrap();
    assert_eq!(
        rt.load("tool.rip.python-workers").unwrap(),
        domain::plugin::PluginState::Loaded
    );
    assert_eq!(
        rt.activate("tool.rip.python-workers").unwrap(),
        domain::plugin::PluginState::Active
    );

    // disable：LOADED 复合步（先 deactivate），数据全保留。
    let disabled = rt.disable("user:test", "tool.rip.python-workers").unwrap();
    assert!(disabled.deactivated_first);
    assert_eq!(disabled.from_state, "ACTIVE");
    let row = rt
        .store()
        .get_active("tool.rip.python-workers")
        .unwrap()
        .unwrap();
    assert_eq!(row.state, domain::plugin::PluginState::Disabled);
    // 禁用不删数据：installed 目录、Manifest、capabilities 行全部在。
    assert!(pkg.join("parsing_worker.py").is_file());
    assert!(pkg.join("checksums.sha256").is_file());
    assert!(pkg.join("plugin.lock").is_file());
    assert!(row.manifest_json.contains("tool.rip.python-workers"));
    assert_eq!(
        rt.store()
            .capabilities("tool.rip.python-workers", "0.1.0")
            .unwrap()
            .len(),
        3
    );

    // re-enable：重过静态检查（DISABLED → ENABLED）。
    let enabled = rt.enable("user:test", "tool.rip.python-workers").unwrap();
    assert_eq!(enabled.from_state, "DISABLED");
    let row = rt
        .store()
        .get_active("tool.rip.python-workers")
        .unwrap()
        .unwrap();
    assert_eq!(row.state, domain::plugin::PluginState::Enabled);

    // uninstall：前置自动 disable，来源已卸载标记，§5.4 处置报告。
    let report = rt
        .uninstall("user:test", "tool.rip.python-workers")
        .unwrap();
    assert_eq!(report.op, "uninstall");
    let body = report
        .items
        .iter()
        .find(|i| i.category == "plugin_body")
        .expect("plugin_body item");
    assert_eq!(body.action, RetentionAction::Delete);
    assert!(
        body.executed,
        "package under data_root is physically removed"
    );
    let knowledge = report
        .items
        .iter()
        .find(|i| i.category == "ingested_knowledge")
        .expect("ingested_knowledge item");
    assert_eq!(knowledge.action, RetentionAction::Retain);
    let row = rt
        .store()
        .get_active("tool.rip.python-workers")
        .unwrap()
        .unwrap();
    assert_eq!(row.state, domain::plugin::PluginState::Uninstalled);
    assert!(row.source_uninstalled, "orphan source marker set (§5.3)");
    // 版本行保留（已入主库知识溯源锚点），包体已删。
    assert!(rt
        .store()
        .get("tool.rip.python-workers", "0.1.0")
        .unwrap()
        .is_some());
    assert!(!pkg.join("parsing_worker.py").exists());

    // 五类操作中已发生的四类各 Emit 一条（upgrade 在 test 4 单独覆盖）。
    let counts = audit_counts(&pool);
    let find = |action: &str| {
        counts
            .iter()
            .find(|(a, _)| a == action)
            .map(|(_, n)| *n)
            .unwrap_or(0)
    };
    assert_eq!(find(plugin_actions::INSTALL), 1);
    assert_eq!(
        find(plugin_actions::ENABLE),
        2,
        "首启一条 + disable 后 re-enable 一条"
    );
    assert_eq!(find(plugin_actions::DISABLE), 1);
    assert_eq!(find(plugin_actions::UNINSTALL), 1);
    assert_eq!(find(plugin_actions::UPGRADE), 0);
}

// ---------------------------------------------------------------------------
// re-enable 重过检查：包体坏掉即拒绝且状态不变
// ---------------------------------------------------------------------------

#[test]
fn reenable_rechecks_static_rules() {
    let (_pool, rt, data_root) = fresh_runtime("recheck");
    let manifest = read_example(KNOWLEDGE_EXAMPLE);
    let pkg = build_package(
        &data_root,
        "knowledge",
        &["capabilities/knowledge_source.yaml"],
    );
    rt.install("user:test", &manifest, &pkg, PluginTrust::Local)
        .unwrap();
    rt.enable("user:test", "knowledge.rtis-redteam").unwrap();
    rt.disable("user:test", "knowledge.rtis-redteam").unwrap();

    // 破坏包体：entry 文件（README.md，P6-5 对齐后口径）删除 → rule 7 拒绝，
    // 状态仍 DISABLED。
    fs::remove_file(pkg.join("README.md")).unwrap();
    let err = rt
        .enable("user:test", "knowledge.rtis-redteam")
        .unwrap_err();
    assert_eq!(err.code, "validation_failed");
    assert!(
        err.to_json_string().contains("entry_missing"),
        "{}",
        err.to_json_string()
    );
    let row = rt
        .store()
        .get_active("knowledge.rtis-redteam")
        .unwrap()
        .unwrap();
    assert_eq!(
        row.state,
        domain::plugin::PluginState::Disabled,
        "enable failure keeps state"
    );

    // 修复后 re-enable 通过。
    fs::write(pkg.join("README.md"), "# RTIS 红队知识包\n").unwrap();
    let enabled = rt.enable("user:test", "knowledge.rtis-redteam").unwrap();
    assert_eq!(enabled.from_state, "DISABLED");
}

// ---------------------------------------------------------------------------
// upgrade：UPDATE_PENDING 叠加、新旧并存、失败自动回滚（§5.3）
// ---------------------------------------------------------------------------

#[test]
fn upgrade_switch_pointer_and_rollback_on_failure() {
    let (pool, rt, data_root) = fresh_runtime("upgrade");
    let manifest_v1 = read_example(KNOWLEDGE_EXAMPLE);
    let pkg_v1 = build_package(&data_root, "v1", &["capabilities/knowledge_source.yaml"]);
    rt.install("user:test", &manifest_v1, &pkg_v1, PluginTrust::Local)
        .unwrap();

    // update_check：叠加 UPDATE_PENDING 标记，状态不变（§5.1）。
    assert!(rt.update_check("knowledge.rtis-redteam", "0.2.0").unwrap());
    let row = rt
        .store()
        .get_active("knowledge.rtis-redteam")
        .unwrap()
        .unwrap();
    assert!(row.update_pending);
    assert_eq!(row.state, domain::plugin::PluginState::Registered);
    // 无新版（相同版本）清除标记。
    assert!(!rt.update_check("knowledge.rtis-redteam", "0.1.0").unwrap());
    let row = rt
        .store()
        .get_active("knowledge.rtis-redteam")
        .unwrap()
        .unwrap();
    assert!(!row.update_pending);

    // upgrade 0.2.0：新包目录并存，指针切换，旧版本保留（回滚窗口）。
    let mut manifest_v2 = manifest_v1.clone();
    manifest_v2
        .as_object_mut()
        .unwrap()
        .insert("version".into(), Value::from("0.2.0"));
    let pkg_v2 = build_package(&data_root, "v2", &["capabilities/knowledge_source.yaml"]);
    let outcome = rt
        .upgrade(
            "user:test",
            "knowledge.rtis-redteam",
            &manifest_v2,
            &pkg_v2,
            PluginTrust::Local,
        )
        .unwrap();
    assert_eq!(outcome.from_version, "0.1.0");
    assert_eq!(outcome.to_version, "0.2.0");
    assert!(outcome.old_dir_retained);
    let active = rt
        .store()
        .get_active("knowledge.rtis-redteam")
        .unwrap()
        .unwrap();
    assert_eq!(active.version, "0.2.0");
    assert!(active.is_active);
    assert_eq!(active.state, domain::plugin::PluginState::Registered);
    let old = rt
        .store()
        .get("knowledge.rtis-redteam", "0.1.0")
        .unwrap()
        .unwrap();
    assert!(
        !old.is_active,
        "old version retained in rollback window (D6)"
    );
    assert!(
        rt.store()
            .capabilities("knowledge.rtis-redteam", "0.2.0")
            .unwrap()
            .len()
            == 2
    );
    assert!(
        pkg_v1.join("checksums.sha256").is_file(),
        "old dir retained on disk"
    );

    // upgrade 失败自动回滚：v0.3.0 包体校验和被破坏（预写清单后篡改内容）。
    let mut manifest_v3 = manifest_v1.clone();
    manifest_v3
        .as_object_mut()
        .unwrap()
        .insert("version".into(), Value::from("0.3.0"));
    let pkg_v3 = build_package(&data_root, "v3", &["capabilities/knowledge_source.yaml"]);
    plugin_registry::write_checksums(&pkg_v3).unwrap();
    fs::write(
        pkg_v3.join("capabilities/knowledge_source.yaml"),
        "tampered\n",
    )
    .unwrap();
    let err = rt
        .upgrade(
            "user:test",
            "knowledge.rtis-redteam",
            &manifest_v3,
            &pkg_v3,
            PluginTrust::Local,
        )
        .unwrap_err();
    assert_eq!(err.code, "checksum_verification_failed");
    // 指针不切换：active 仍 0.2.0；新版本行不存在（事务回滚）。
    let active = rt
        .store()
        .get_active("knowledge.rtis-redteam")
        .unwrap()
        .unwrap();
    assert_eq!(active.version, "0.2.0");
    assert!(rt
        .store()
        .get("knowledge.rtis-redteam", "0.3.0")
        .unwrap()
        .is_none());

    // 审计：upgrade 成功 switched 一条 + 失败 rolled_back 一条。
    let upgrades: Vec<Value> = pool
        .with_conn(|c| {
            let mut stmt = c
                .prepare("SELECT detail FROM audit_event WHERE action = 'plugin.upgrade' ORDER BY event_id")
                .unwrap();
            let rows = stmt
                .query_map([], |r| r.get::<_, String>(0))
                .unwrap()
                .collect::<Result<Vec<_>, _>>()
                .unwrap();
            Ok::<Vec<String>, database::rusqlite::Error>(rows)
        })
        .unwrap()
        .into_iter()
        .map(|d| serde_json::from_str(&d).unwrap())
        .collect();
    assert_eq!(upgrades.len(), 2);
    assert_eq!(upgrades[0]["result"], "switched");
    assert_eq!(upgrades[1]["result"], "rolled_back");
    assert_eq!(upgrades[1]["active_version_unchanged"], Value::Bool(true));
}

// ---------------------------------------------------------------------------
// 五类操作各 Emit 一条 + 核心表零直写
// ---------------------------------------------------------------------------

#[test]
fn five_ops_emit_audit_and_core_tables_untouched() {
    let (pool, rt, data_root) = fresh_runtime("audit");
    let before = core_table_counts(&pool);

    let manifest = read_example(KNOWLEDGE_EXAMPLE);
    let pkg = build_package(&data_root, "op", &["capabilities/knowledge_source.yaml"]);
    rt.install("user:test", &manifest, &pkg, PluginTrust::Local)
        .unwrap();

    // upgrade（REGISTERED 态可发起：当前版本继续运行，§5.3 第 1 步）。
    let mut manifest_v2 = manifest.clone();
    manifest_v2
        .as_object_mut()
        .unwrap()
        .insert("version".into(), Value::from("0.2.0"));
    let pkg_v2 = build_package(&data_root, "opv2", &["capabilities/knowledge_source.yaml"]);
    rt.upgrade(
        "user:test",
        "knowledge.rtis-redteam",
        &manifest_v2,
        &pkg_v2,
        PluginTrust::Local,
    )
    .unwrap();

    // enable（重过静态检查）→ disable → uninstall。
    rt.enable("user:test", "knowledge.rtis-redteam").unwrap();
    rt.disable("user:test", "knowledge.rtis-redteam").unwrap();
    rt.uninstall("user:test", "knowledge.rtis-redteam").unwrap();

    // 五类各一条，无多余 plugin.* 词形。
    let counts = audit_counts(&pool);
    let find = |action: &str| {
        counts
            .iter()
            .find(|(a, _)| a == action)
            .map(|(_, n)| *n)
            .unwrap_or(0)
    };
    assert_eq!(find(plugin_actions::INSTALL), 1, "counts: {counts:?}");
    assert_eq!(find(plugin_actions::ENABLE), 1);
    assert_eq!(find(plugin_actions::DISABLE), 1);
    assert_eq!(find(plugin_actions::UPGRADE), 1);
    assert_eq!(find(plugin_actions::UNINSTALL), 1);
    let total_plugin_events: i64 = counts.iter().map(|(_, n)| n).sum();
    assert_eq!(
        total_plugin_events, 5,
        "no extra plugin.* event shapes: {counts:?}"
    );

    // 核心表零直写：插件生命周期全程行数不变。
    let after = core_table_counts(&pool);
    assert_eq!(
        before, after,
        "core tables must be untouched: before={before:?} after={after:?}"
    );
}

// ---------------------------------------------------------------------------
// 009 迁移：测试文件副本幂等执行两次无差异
// ---------------------------------------------------------------------------

#[test]
fn migration_009_idempotent_on_file_copy() {
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let db = std::env::temp_dir().join(format!("p6rt-mig-{}-{n}.db", std::process::id()));
    let _ = fs::remove_file(&db);
    let pool = Pool::open(PoolConfig {
        path: DbPath::File(db.clone()),
        readonly: false,
        max_connections: 2,
        busy_timeout: Duration::from_secs(5),
    })
    .unwrap();

    let snapshot = |pool: &Pool| -> (i64, Vec<(String, String, String)>, i64) {
        let uv: i64 = pool
            .with_conn(|c| c.query_row("PRAGMA user_version", [], |r| r.get(0)))
            .unwrap();
        let objects: Vec<(String, String, String)> = pool
            .with_conn(|c| {
                let mut stmt = c
                    .prepare("SELECT type, name, sql FROM sqlite_master ORDER BY type, name")
                    .unwrap();
                let rows = stmt
                    .query_map([], |r| {
                        Ok((
                            r.get::<_, String>(0)?,
                            r.get::<_, String>(1)?,
                            r.get::<_, Option<String>>(2)?.unwrap_or_default(),
                        ))
                    })
                    .unwrap()
                    .collect::<Result<Vec<_>, _>>()
                    .unwrap();
                Ok::<Vec<(String, String, String)>, database::rusqlite::Error>(rows)
            })
            .unwrap();
        let sv: i64 = pool
            .with_conn(|c| {
                c.query_row("SELECT COUNT(*) FROM schema_version", [], |r| r.get(0))
                    .map_err(database::DbError::from)
            })
            .unwrap();
        (uv, objects, sv)
    };

    let applied1 = database::migrate(&pool, &migrations_dir()).unwrap();
    assert!(
        applied1
            .iter()
            .any(|m| m.version == 9 && m.name.contains("plugin")),
        "009 applied: {applied1:?}"
    );
    let snap1 = snapshot(&pool);
    // 链尾随批次演进：P6-5 起 010_knowledge_ingest 为尾（本测仍验证 009 幂等）。
    assert!(
        snap1.0 >= 9,
        "user_version at least 9 after migrate (got {})",
        snap1.0
    );

    // 第二次全量 migrate：no-op 且快照无差异。
    let applied2 = database::migrate(&pool, &migrations_dir()).unwrap();
    assert!(applied2.is_empty(), "migrate must be idempotent");
    let snap2 = snapshot(&pool);
    assert_eq!(snap1, snap2, "second migrate leaves schema byte-identical");

    // 009 文件内容直接重放两次（绕过 user_version 跳过逻辑的最强幂等证据）：
    // 对象集合逐字节不变。009 自带 PRAGMA user_version = 9（文件自钉语义），
    // 链尾演进后（010 起）重放会把 user_version 回钉 9，属文件自身行为，
    // 断言对象集合并用 migrate() 复位链尾。
    let sql9 = fs::read_to_string(migrations_dir().join("009_plugin.sql")).unwrap();
    pool.with_conn(|c| c.execute_batch(&sql9)).unwrap();
    pool.with_conn(|c| c.execute_batch(&sql9)).unwrap();
    let snap3 = snapshot(&pool);
    assert_eq!(
        snap2.1, snap3.1,
        "raw 009 replay twice leaves schema objects identical"
    );
    assert_eq!(
        snap3.0, 9,
        "009 file self-pins user_version = 9 (own pragma)"
    );
    let reapply = database::migrate(&pool, &migrations_dir()).unwrap();
    // 链尾随批次演进：SK-3 起 012_knowledge_document_fetch 为尾，009 自钉回
    // 9 后 migrate 需重放 010..012 三枚（断言按链尾距离演进）。
    assert_eq!(
        reapply.len(),
        3,
        "migrate re-applies 010..012 after 009 self-pin reset"
    );
    let snap4 = snapshot(&pool);
    assert_eq!(snap4.0, snap2.0, "chain tail restored");
    assert_eq!(snap4.1, snap2.1, "schema objects identical after restore");

    let _ = fs::remove_file(&db);
}

// ---------------------------------------------------------------------------
// INVALID 处置经 quality-engine（§11.5 既有裁定：双轴状态机 + Review 队列）
// ---------------------------------------------------------------------------

#[test]
fn invalid_disposition_via_quality_engine() {
    let (pool, rt, data_root) = fresh_runtime("invalid");
    let manifest = read_example(KNOWLEDGE_EXAMPLE);
    let pkg = build_package(&data_root, "inv", &["capabilities/knowledge_source.yaml"]);
    rt.install("user:test", &manifest, &pkg, PluginTrust::Local)
        .unwrap();

    // 模拟静态校验失败后落库的 INVALID 生命周期态（validate 失败路径在
    // plugin-registry P6-1 已覆盖，这里直接置态检验治理面接线）。
    rt.store()
        .set_state(
            "knowledge.rtis-redteam",
            "0.1.0",
            domain::plugin::PluginState::Invalid,
        )
        .unwrap();

    let governor = quality_engine::PluginGovernor::open(pool.clone());
    let reviews = quality_engine::ReviewStore::open(pool.clone()).unwrap();
    let disposition = governor
        .handle_invalid(
            "knowledge.rtis-redteam",
            "checksum mismatch at install",
            &reviews,
        )
        .unwrap();
    assert!(disposition.quarantined);
    assert_eq!(disposition.from_governance, "ACTIVE");
    assert_eq!(disposition.review_outcome, "inserted");
    // 治理轴 QUARANTINED（K-08 正交轴），生命周期列仍 INVALID。
    assert_eq!(
        governor
            .current_governance("knowledge.rtis-redteam")
            .unwrap(),
        domain::GovernanceStatus::Quarantined
    );
    let row = rt
        .store()
        .get_active("knowledge.rtis-redteam")
        .unwrap()
        .unwrap();
    assert_eq!(row.state, domain::plugin::PluginState::Invalid);
    // Review 队列有 PENDING 提案。
    let pending = reviews.pending().unwrap();
    assert!(
        pending
            .iter()
            .any(|r| r.target_type == "plugin_invalid" && r.target_id == "knowledge.rtis-redteam"),
        "review queued: {pending:?}"
    );
}
