//! SK-3 集成测试（P6-11 第 2/3 步，安全知识源计划 §6.2D/§8 P6-11）：
//! rip.knowledge.source.sync 受控同步 + 恶意 fixture 拒绝矩阵的 Rust 侧：
//! - dry-run 零落盘、实跑幂等二跑 skipped=全量、失败重试与部分失败报告；
//! - 浮动分支 pin 前置闸拒绝（source_sync floating_ref_rejected）+ 复验
//!   P6-4 plugin-registry floating_branch_rejected 语义（validate rule 5）；
//! - idempotency_key 幂等跳过；
//! - skill_candidate 到 Resource 候选映射（resource-engine 受控写面，
//!   APPROVED 审核闸）。

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use database::{DbPath, Pool, PoolConfig};
use domain::knowledge::ReviewStatus;
use plugin_registry::parse_document;
use plugin_runtime::source_sync::{
    package_stats, read_sync_keys, sync_knowledge_source, SyncOptions,
};
use resource_engine::SqliteResourceRepository;

fn unique(tag: &str) -> PathBuf {
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("sk3-{tag}-{}-{n}", std::process::id()));
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

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn migrations_dir() -> PathBuf {
    repo_root().join("migrations")
}

/// fixture 拷贝进临时目录（摄取只读，但避免任何运行期写回仓库 fixture）。
fn fixture_package(fixture: &str) -> PathBuf {
    fn copy_tree(src: &Path, dst: &Path) {
        fs::create_dir_all(dst).unwrap();
        for entry in fs::read_dir(src).unwrap() {
            let entry = entry.unwrap();
            let target = dst.join(entry.file_name());
            if entry.path().is_dir() {
                copy_tree(&entry.path(), &target);
            } else {
                fs::copy(entry.path(), &target).unwrap();
            }
        }
    }
    let src = repo_root().join("tests/fixtures/sk2").join(fixture);
    let dir = unique(fixture);
    copy_tree(&src, &dir);
    dir
}

fn sk3_package(fixture: &str) -> PathBuf {
    let src = repo_root().join("tests/fixtures/sk3").join(fixture);
    let dir = unique(fixture);
    fn copy_tree(src: &Path, dst: &Path) {
        fs::create_dir_all(dst).unwrap();
        for entry in fs::read_dir(src).unwrap() {
            let entry = entry.unwrap();
            let target = dst.join(entry.file_name());
            if entry.path().is_dir() {
                copy_tree(&entry.path(), &target);
            } else {
                fs::copy(entry.path(), &target).unwrap();
            }
        }
    }
    copy_tree(&src, &dir);
    dir
}

fn ah_manifest() -> serde_json::Value {
    let text =
        fs::read_to_string(repo_root().join("manifests/plugins/knowledge.awesome-hacking.yaml"))
            .unwrap();
    parse_document(&text).unwrap()
}

fn skill_pack_manifest() -> serde_json::Value {
    let text = fs::read_to_string(repo_root().join("tests/fixtures/sk3/skill-pack/manifest.yaml"))
        .unwrap();
    parse_document(&text).unwrap()
}

fn kse_count(pool: &Pool, where_clause: &str) -> i64 {
    let sql = format!("SELECT COUNT(*) FROM knowledge_source_entry {where_clause}");
    pool.with_conn(|c| c.query_row(&sql, [], |r| r.get(0)))
        .unwrap()
}

fn base_opts() -> SyncOptions {
    SyncOptions {
        retries: 0,
        ..SyncOptions::default()
    }
}

// ---------------------------------------------------------------------------
// dry-run 零落盘 → 实跑 → 幂等二跑 skipped=全量
// ---------------------------------------------------------------------------

#[test]
fn sk3_sync_dry_run_zero_writes_then_real_then_idempotent() {
    let pool = memory_pool();
    database::migrate(&pool, &migrations_dir()).unwrap();
    let manifest = ah_manifest();
    let pkg = fixture_package("awesome-hacking");

    // dry-run：报告计数与实跑同构，但 knowledge_source_entry 零行。
    let report = sync_knowledge_source(
        &pool,
        &manifest,
        &pkg,
        &SyncOptions {
            dry_run: true,
            ..base_opts()
        },
    )
    .unwrap();
    assert!(report.ok);
    assert!(report.dry_run);
    assert!(report.final_report.as_ref().unwrap().dry_run);
    assert_eq!(
        report
            .final_report
            .as_ref()
            .unwrap()
            .counts
            .get("catalog_entry"),
        Some(&85)
    );
    assert_eq!(kse_count(&pool, ""), 0, "dry-run 必须零落盘");

    // 实跑：85 条 catalog_entry。
    let report = sync_knowledge_source(&pool, &manifest, &pkg, &base_opts()).unwrap();
    assert!(report.ok);
    assert!(!report.dry_run);
    assert_eq!(report.pin_kind.as_deref(), Some("commit"));
    assert_eq!(
        report.pinned_commit.as_deref(),
        Some("440486fd08b0a0d9ad9a041a0bb234dcdf65dee2")
    );
    assert_eq!(kse_count(&pool, "WHERE entry_kind='catalog_entry'"), 85);

    // 幂等二跑：skipped=全量、changed=0。
    let report2 = sync_knowledge_source(&pool, &manifest, &pkg, &base_opts()).unwrap();
    assert!(report2.ok);
    let fr = report2.final_report.as_ref().unwrap();
    assert_eq!(fr.skipped, 85, "二跑 skipped=全量");
    assert_eq!(fr.changed, 0);
    assert!(fr.counts.values().all(|v| *v == 0));

    // 包统计面（限额核对面）如实携带。
    assert!(report.package_files.unwrap() >= 1);
    assert!(report.package_bytes.unwrap() > 0);
}

// ---------------------------------------------------------------------------
// 浮动分支：sync pin 前置闸 + P6-4 registry 语义复验
// ---------------------------------------------------------------------------

#[test]
fn sk3_floating_branch_refused_by_pin_gate_and_registry_rule() {
    let mut manifest = ah_manifest();
    manifest["source"]["pinned"]["kind"] = serde_json::json!("branch");

    // registry 侧（P6-4 既有语义复验）：validate rule 5 floating_branch_rejected
    let report = plugin_registry::validate::validate(
        &manifest,
        None,
        &std::collections::HashSet::new(),
        &plugin_registry::validate::Options::default(),
    );
    assert_eq!(report.errors[0].rule, 5);
    assert_eq!(report.errors[0].code, "floating_branch_rejected");

    // sync 侧 pin 前置闸：fail-fast 拒绝且不落任何行。
    let pool = memory_pool();
    database::migrate(&pool, &migrations_dir()).unwrap();
    let pkg = fixture_package("awesome-hacking");
    let err = sync_knowledge_source(&pool, &manifest, &pkg, &base_opts()).unwrap_err();
    assert_eq!(err.code, "floating_ref_rejected");
    assert_eq!(kse_count(&pool, ""), 0, "浮动分支不得落任何行");

    // pin 缺失（git source 无 pinned）同样拒绝。
    let mut manifest2 = ah_manifest();
    manifest2["source"]["pinned"] = serde_json::Value::Null;
    let err2 = sync_knowledge_source(&pool, &manifest2, &pkg, &base_opts()).unwrap_err();
    assert_eq!(err2.code, "pin_missing");

    // 完整 Commit 语义：kind=commit 短哈希拒绝（sync 比 registry 7-64 更严）。
    let mut manifest3 = ah_manifest();
    manifest3["source"]["pinned"]["value"] = serde_json::json!("440486f");
    let err3 = sync_knowledge_source(&pool, &manifest3, &pkg, &base_opts()).unwrap_err();
    assert_eq!(err3.code, "pin_value_invalid");
}

// ---------------------------------------------------------------------------
// 限额与失败报告（机器可读）
// ---------------------------------------------------------------------------

#[test]
fn sk3_sync_limits_exceeded_fail_fast_machine_readable() {
    let pool = memory_pool();
    database::migrate(&pool, &migrations_dir()).unwrap();
    let manifest = ah_manifest();
    let pkg = fixture_package("awesome-hacking");

    let report = sync_knowledge_source(
        &pool,
        &manifest,
        &pkg,
        &SyncOptions {
            max_bytes: Some(1), // 必然超限
            ..base_opts()
        },
    )
    .unwrap();
    assert!(!report.ok);
    assert!(report.limits_exceeded.contains(&"max_bytes".to_string()));
    assert!(report
        .partial_failures
        .iter()
        .any(|f| f.contains("max_bytes")));
    assert!(report.final_report.is_none(), "超限不摄取（fail-fast）");
    assert_eq!(kse_count(&pool, ""), 0);

    // max_files 同形。
    let report2 = sync_knowledge_source(
        &pool,
        &manifest,
        &pkg,
        &SyncOptions {
            max_files: Some(0),
            ..base_opts()
        },
    )
    .unwrap();
    assert!(report2.limits_exceeded.contains(&"max_files".to_string()));
}

#[test]
fn sk3_sync_retry_attempts_recorded_machine_readable() {
    let pool = memory_pool();
    database::migrate(&pool, &migrations_dir()).unwrap();
    // 破坏权限闸（data_access 改为 read_only）：摄取受控 API 每次尝试都
    // deterministic 拒绝（ingest_permission_denied），重试一次共两 attempts。
    let mut manifest = ah_manifest();
    manifest["permissions"]["data_access"] = serde_json::json!("read_only");
    let pkg = fixture_package("awesome-hacking");
    let report = sync_knowledge_source(
        &pool,
        &manifest,
        &pkg,
        &SyncOptions {
            retries: 1,
            ..base_opts()
        },
    )
    .unwrap();
    assert!(!report.ok);
    assert_eq!(report.attempts.len(), 2, "retries=1 → 共两次尝试");
    assert!(report.attempts.iter().all(|a| !a.ok));
    assert_eq!(
        report.attempts[0].error_code.as_deref(),
        Some("ingest_permission_denied")
    );
    assert!(report
        .partial_failures
        .iter()
        .any(|f| f.contains("ingest failed after 2 attempt")));
    assert_eq!(kse_count(&pool, ""), 0);
}

// ---------------------------------------------------------------------------
// 幂等键：命中既有成功键整体跳过（零落盘）
// ---------------------------------------------------------------------------

#[test]
fn sk3_sync_idempotency_key_skips_second_run() {
    let pool = memory_pool();
    database::migrate(&pool, &migrations_dir()).unwrap();
    let manifest = ah_manifest();
    let pkg = fixture_package("awesome-hacking");

    // 首跑（带键，prior 为空）→ 实跑。
    let report = sync_knowledge_source(
        &pool,
        &manifest,
        &pkg,
        &SyncOptions {
            idempotency_key: Some("sk3-key-001".to_string()),
            ..base_opts()
        },
    )
    .unwrap();
    assert!(report.ok);
    assert!(!report.idempotent_skipped);
    assert_eq!(kse_count(&pool, ""), 85);

    // 二跑同键（prior 含该键）→ 整体跳过，final_report=None，零新增。
    let report2 = sync_knowledge_source(
        &pool,
        &manifest,
        &pkg,
        &SyncOptions {
            idempotency_key: Some("sk3-key-001".to_string()),
            prior_sync_keys: vec!["sk3-key-001".to_string()],
            ..base_opts()
        },
    )
    .unwrap();
    assert!(report2.ok);
    assert!(report2.idempotent_skipped);
    assert!(report2.final_report.is_none());
    assert_eq!(kse_count(&pool, ""), 85, "幂等跳过零新增");

    // 键台账侧车：append/read 往返（非数据库写面）。
    let state_dir = unique("sync-keys");
    plugin_runtime::source_sync::append_sync_key(
        &state_dir,
        "sk3-key-001",
        "knowledge.awesome-hacking",
    )
    .unwrap();
    assert_eq!(read_sync_keys(&state_dir), vec!["sk3-key-001".to_string()]);
    assert_eq!(
        plugin_runtime::source_sync::SYNC_KEYS_FILE_NAME,
        "sync-keys.jsonl"
    );

    // 包统计独立可用（CLI 限额面同一实现）。
    let (files, bytes) = package_stats(&pkg).unwrap();
    assert!(files >= 2 && bytes > 100);
}

// ---------------------------------------------------------------------------
// skill_candidate → Resource 候选映射（resource-engine 受控写面）
// ---------------------------------------------------------------------------

#[test]
fn sk3_sync_candidate_mapping_review_gate_and_idempotence() {
    let pool = memory_pool();
    database::migrate(&pool, &migrations_dir()).unwrap();
    let audit = audit_engine::AuditEngine::new(pool.clone());
    let manifest = skill_pack_manifest();
    let pkg = sk3_package("skill-pack");

    // sync 实跑：2 个 skill_candidate（DISCOVERED 初始态）。
    let report = sync_knowledge_source(&pool, &manifest, &pkg, &base_opts()).unwrap();
    assert!(report.ok);
    assert_eq!(
        report
            .final_report
            .as_ref()
            .unwrap()
            .counts
            .get("skill_candidate"),
        Some(&2)
    );

    // 审核闸：未 APPROVED 时不映射。
    let repo = SqliteResourceRepository::new(pool.clone());
    let m0 = repo
        .promote_knowledge_candidates("knowledge.sk3-skill-pack", 500)
        .unwrap();
    assert_eq!(m0.scanned, 2);
    assert_eq!(m0.mapped, 0, "DISCOVERED 不映射（审核闸）");
    assert_eq!(m0.skipped_not_approved, 2);

    // 受控审核翻转 → 映射 2 条 CANDIDATE 资源。
    let keys = vec!["skills/alpha/SKILL.md", "skills/beta/SKILL.md"];
    plugin_runtime::knowledge_review::set_review_status(
        &pool,
        &audit,
        "user:test",
        "knowledge.sk3-skill-pack",
        &keys,
        ReviewStatus::Approved,
    )
    .unwrap();
    let m1 = repo
        .promote_knowledge_candidates("knowledge.sk3-skill-pack", 500)
        .unwrap();
    assert_eq!(m1.mapped, 2);
    assert_eq!(m1.resource_ids.len(), 2);
    let (lifecycle, governance, rtype): (String, String, String) = pool
        .with_conn(|c| {
            c.query_row(
                "SELECT lifecycle, governance, resource_type FROM resource
                 WHERE resource_id = 'kse:knowledge.sk3-skill-pack:skills/alpha/SKILL.md'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
        })
        .unwrap();
    assert_eq!(
        (lifecycle.as_str(), governance.as_str(), rtype.as_str()),
        ("CANDIDATE", "ACTIVE", "skill_candidate")
    );

    // 映射幂等：二跑 skipped_existing，不覆盖。
    let m2 = repo
        .promote_knowledge_candidates("knowledge.sk3-skill-pack", 500)
        .unwrap();
    assert_eq!(m2.mapped, 0);
    assert_eq!(m2.skipped_existing, 2);

    // REJECTED 永不映射：翻转 alpha 为 REJECTED 后只余 beta 的既有行。
    plugin_runtime::knowledge_review::set_review_status(
        &pool,
        &audit,
        "user:test",
        "knowledge.sk3-skill-pack",
        &["skills/alpha/SKILL.md"],
        ReviewStatus::Rejected,
    )
    .unwrap();
    let m3 = repo
        .promote_knowledge_candidates("knowledge.sk3-skill-pack", 500)
        .unwrap();
    assert_eq!(m3.mapped, 0, "REJECTED 永不映射");
    assert_eq!(m3.skipped_not_approved, 1);
    assert_eq!(m3.skipped_existing, 1);
}
