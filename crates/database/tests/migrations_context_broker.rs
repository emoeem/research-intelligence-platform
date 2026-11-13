//! P4-1 验证载具：008_context_broker.sql 在开发库副本上经 crates/database
//! 迁移执行器（migrate.rs:32 discover_migrations / :67 migrate 幂等机制）
//! 连续执行两次无差异（migrations_tool_contract.rs 的 dev-p13 先例同款形态）。
//!
//! 判据（PHASE4-DESIGN §6 P4-1 原句）：迁移在开发库副本幂等执行两次无差异；
//! DDL 字段覆盖设计稿 CONTEXT-BROKER-DESIGN §6.1 三表草案列 + 计划 §12:706
//! 评审要求（唯一键 PK(manifest_id,item_id)、外键 CASCADE、索引、删除语义）；
//! 五类 CHECK 词表与 domain::context 枚举一一对应。
//!
//! 开发库副本路径：`migrations/dev-p4.db`（项目内，*.db 已 gitignore；
//! 每次运行先删旧副本重建，platform.db 与 platform-dryrun.db 不写）。

use database::{discover_migrations, migrate, DbPath, Pool, PoolConfig};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

/// 两个用例共用 migrations/dev-p4.db（reset 会互删对方打开中的文件，
/// 并行时触发 SQLITE_READONLY / disk I/O，第 1 轮门禁实测），进程内
/// 串行化共享副本的全部访问；路径契约不变（头注释口径）。
static DEV_COPY_LOCK: Mutex<()> = Mutex::new(());

fn repo_migrations_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../migrations")
        .canonicalize()
        .expect("migrations dir must exist")
}

fn dev_copy_path() -> PathBuf {
    repo_migrations_dir().join("dev-p4.db")
}

fn open_dev_pool() -> Pool {
    Pool::open(PoolConfig {
        path: DbPath::File(dev_copy_path()),
        readonly: false,
        max_connections: 1,
        busy_timeout: Duration::from_secs(5),
    })
    .expect("open dev copy pool")
}

/// sqlite_master 全量结构转储（幂等比对基准：type/name/sql 按序拼接）。
fn schema_dump(pool: &Pool) -> String {
    pool.with_conn(|c| {
        let mut stmt = c.prepare(
            "SELECT type, name, sql FROM sqlite_master
             WHERE name NOT LIKE 'sqlite_%'
             ORDER BY type, name",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, Option<String>>(2)?,
            ))
        })?;
        let mut out = String::new();
        for row in rows {
            let (t, n, sql) = row?;
            out.push_str(&format!("{t}|{n}|{}\n", sql.unwrap_or_default()));
        }
        Ok::<String, database::rusqlite::Error>(out)
    })
    .unwrap()
}

fn user_version(pool: &Pool) -> i64 {
    pool.with_conn(|c| c.query_row("PRAGMA user_version", [], |r| r.get(0)))
        .unwrap()
}

fn schema_version_rows(pool: &Pool) -> Vec<(i64, String)> {
    pool.with_conn(|c| {
        let mut stmt = c
            .prepare("SELECT version, applied_by FROM schema_version ORDER BY version")
            .unwrap();
        let rows = stmt
            .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))
            .unwrap();
        rows.collect::<Result<Vec<_>, _>>()
    })
    .unwrap()
}

fn reset_dev_copy() {
    let base = dev_copy_path();
    for suffix in ["", "-wal", "-shm"] {
        let mut p = base.clone().into_os_string();
        p.push(suffix);
        let p = PathBuf::from(p);
        if p.exists() {
            std::fs::remove_file(&p).unwrap_or_else(|e| panic!("remove {}: {e}", p.display()));
        }
    }
}

#[test]
fn p4_1_migrate_twice_no_diff_on_dev_copy() {
    let _guard = DEV_COPY_LOCK.lock().unwrap();
    let dir = repo_migrations_dir();
    let discovered = discover_migrations(&dir).unwrap();
    let versions: Vec<i64> = discovered.iter().map(|m| m.version).collect();
    assert_eq!(
        versions,
        vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18],
        "discover must see 001..017 in order (008 = P4-1 Context Broker 三表，009 = P6-2 Plugin 数据面；编号按 ROADMAP §1.4 先到先得，设计稿 §6.1 草案号 005 让位，P4-0 决议 1; 012 = SK-3 文档抓取审计数据面, 013 = P7-3 Agent Memory 三表, 014 = P7-4 运行控制与步骤轨迹, 015 = P7-5 trace 统一关联, 016 = P8-1 workflow 定义版本化, 017 = P8-2 workflow 运行持久化)"
    );

    reset_dev_copy();
    let pool = open_dev_pool();

    // 第一次：全链 001..011 落地，链尾 011_knowledge_sources。
    let first = migrate(&pool, &dir).unwrap();
    let first_summary: Vec<(i64, String)> =
        first.iter().map(|m| (m.version, m.name.clone())).collect();
    assert_eq!(
        first_summary,
        vec![
            (1, "_init".to_string()),
            (2, "_tasks".to_string()),
            (3, "_tools_workflow".to_string()),
            (4, "_fullbatch".to_string()),
            (5, "_tool_contract".to_string()),
            (6, "_agent_data_plane".to_string()),
            (7, "_runtime_node".to_string()),
            (8, "_context_broker".to_string()),
            (9, "_plugin".to_string()),
            (10, "_knowledge_ingest".to_string()),
            (11, "_knowledge_sources".to_string()),
            (12, "_knowledge_document_fetch".to_string()),
            (13, "_agent_memory".to_string()),
            (14, "_agent_run_control".to_string()),
            (15, "_trace_correlation".to_string()),
            (16, "_workflow_definition".to_string()),
            (17, "_workflow_run".to_string()),
            (18, "_workflow_control_correlation".to_string()),
        ],
        "first run must apply exactly 001..018"
    );

    let dump1 = schema_dump(&pool);
    let uv1 = user_version(&pool);
    let sv1 = schema_version_rows(&pool);
    assert_eq!(
        uv1, 18,
        "user_version must be 18 after first run (chain tail 018)"
    );
    assert!(
        sv1.iter()
            .any(|(v, by)| *v == 8 && by == "008_context_broker.sql"),
        "schema_version must carry the version 8 registration row: {sv1:?}"
    );

    // 第二次：执行器按 user_version 全跳过，零变更。
    let second = migrate(&pool, &dir).unwrap();
    assert!(second.is_empty(), "second run must be a no-op");

    let dump2 = schema_dump(&pool);
    let uv2 = user_version(&pool);
    let sv2 = schema_version_rows(&pool);
    assert_eq!(dump2, dump1, "schema must not differ after second run");
    assert_eq!(uv2, uv1, "user_version must be identical after second run");
    assert_eq!(
        sv2, sv1,
        "schema_version rows must be identical after second run"
    );
}

#[test]
fn p4_1_ddl_columns_check_domains_and_fk_cascade() {
    let _guard = DEV_COPY_LOCK.lock().unwrap();
    let dir = repo_migrations_dir();
    reset_dev_copy();
    let pool = open_dev_pool();
    migrate(&pool, &dir).unwrap();

    let columns = |table: &str| -> Vec<String> {
        pool.with_conn(|c| {
            let mut stmt = c.prepare(&format!("PRAGMA table_info({table})")).unwrap();
            let rows = stmt.query_map([], |r| r.get::<_, String>(1)).unwrap();
            rows.collect::<Result<Vec<_>, _>>()
        })
        .unwrap()
    };

    // 设计稿 §6.1 三表草案列 + selection_hash（§7.3）。
    let manifest_cols = columns("context_manifest");
    for required in [
        "manifest_id",
        "task_id",
        "intent_json",
        "policy_version",
        "broker_version",
        "budget_json",
        "totals_json",
        "intent_status",
        "selection_hash",
        "created_at",
        "schema_version",
    ] {
        assert!(
            manifest_cols.iter().any(|c| c == required),
            "context_manifest must cover {required} (设计稿 §6.1/§7.3); got {manifest_cols:?}"
        );
    }
    let item_cols = columns("context_manifest_item");
    for required in [
        "manifest_id",
        "item_id",
        "layer",
        "kind",
        "ref_id",
        "ref_version",
        "source",
        "title",
        "reason",
        "matched_signals_json",
        "est_tokens",
        "included",
        "content_hash",
    ] {
        assert!(
            item_cols.iter().any(|c| c == required),
            "context_manifest_item must cover {required} (设计稿 §6.1); got {item_cols:?}"
        );
    }
    let log_cols = columns("context_ref_log");
    for required in [
        "log_id",
        "manifest_id",
        "item_id",
        "event",
        "detail_json",
        "created_at",
    ] {
        assert!(
            log_cols.iter().any(|c| c == required),
            "context_ref_log must cover {required} (设计稿 §6.1); got {log_cols:?}"
        );
    }

    // 合法链路：manifest -> item -> ref_log 各一行。
    pool.with_conn(|c| {
        c.execute(
            "INSERT INTO context_manifest (manifest_id, policy_version, broker_version, intent_status, created_at)
             VALUES ('ctxm-p4-probe', 'policy-1', '0.1.0', 'ok', '2026-11-10T00:00:00Z')",
            [],
        )?;
        c.execute(
            "INSERT INTO context_manifest_item (manifest_id, item_id, layer, kind, ref_id, source, included, est_tokens)
             VALUES ('ctxm-p4-probe', 'ki-1', 'L2', 'knowledge', 'kb:900008', 'rtis', 'summary', 120)",
            [],
        )?;
        c.execute(
            "INSERT INTO context_ref_log (manifest_id, item_id, event, detail_json, created_at)
             VALUES ('ctxm-p4-probe', 'ki-1', 'ASSEMBLED', '{}', '2026-11-10T00:00:00Z')",
            [],
        )?;
        Ok::<(), database::rusqlite::Error>(())
    })
    .unwrap();

    let count = |table: &str| -> i64 {
        pool.with_conn(|c| {
            c.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| {
                r.get::<_, i64>(0)
            })
        })
        .unwrap()
    };
    assert_eq!(count("context_manifest"), 1);
    assert_eq!(count("context_manifest_item"), 1);
    assert_eq!(count("context_ref_log"), 1);

    // CHECK 词表拒绝（008 CHECK 域 = domain::context 枚举）。
    pool.with_conn(|c| {
        let bad_status = c.execute(
            "INSERT INTO context_manifest (manifest_id, policy_version, broker_version, intent_status, created_at)
             VALUES ('ctxm-bad-status', 'policy-1', '0.1.0', 'pending', '2026-11-10T00:00:00Z')",
            [],
        );
        assert!(bad_status.is_err(), "intent_status outside CHECK domain must be rejected");
        let bad_layer = c.execute(
            "INSERT INTO context_manifest_item (manifest_id, item_id, layer, kind, ref_id, source, included, est_tokens)
             VALUES ('ctxm-p4-probe', 'ki-bad-layer', 'L4', 'knowledge', 'kb:x', 'rtis', 'summary', 1)",
            [],
        );
        assert!(bad_layer.is_err(), "layer L4 must be rejected by CHECK");
        let bad_kind = c.execute(
            "INSERT INTO context_manifest_item (manifest_id, item_id, layer, kind, ref_id, source, included, est_tokens)
             VALUES ('ctxm-p4-probe', 'ki-bad-kind', 'L2', 'secret', 'kb:x', 'rtis', 'summary', 1)",
            [],
        );
        assert!(bad_kind.is_err(), "kind outside seven-value word list must be rejected");
        let bad_included = c.execute(
            "INSERT INTO context_manifest_item (manifest_id, item_id, layer, kind, ref_id, source, included, est_tokens)
             VALUES ('ctxm-p4-probe', 'ki-bad-included', 'L2', 'knowledge', 'kb:x', 'rtis', 'inline', 1)",
            [],
        );
        assert!(bad_included.is_err(), "included 'inline' must be rejected by CHECK");
        let negative_tokens = c.execute(
            "INSERT INTO context_manifest_item (manifest_id, item_id, layer, kind, ref_id, source, included, est_tokens)
             VALUES ('ctxm-p4-probe', 'ki-bad-tokens', 'L2', 'knowledge', 'kb:x', 'rtis', 'summary', -1)",
            [],
        );
        assert!(negative_tokens.is_err(), "negative est_tokens must be rejected by CHECK");
        let bad_event = c.execute(
            "INSERT INTO context_ref_log (manifest_id, item_id, event, detail_json, created_at)
             VALUES ('ctxm-p4-probe', 'ki-1', 'DELETED', '{}', '2026-11-10T00:00:00Z')",
            [],
        );
        assert!(bad_event.is_err(), "event outside five-value word list must be rejected");
        // 外键：item 指向不存在的 manifest 即拒绝（pool.rs:247 foreign_keys=ON）。
        let orphan_item = c.execute(
            "INSERT INTO context_manifest_item (manifest_id, item_id, layer, kind, ref_id, source, included, est_tokens)
             VALUES ('ctxm-missing', 'ki-orphan', 'L2', 'knowledge', 'kb:x', 'rtis', 'summary', 1)",
            [],
        );
        assert!(orphan_item.is_err(), "item without parent manifest must be rejected by FK");
        let orphan_log = c.execute(
            "INSERT INTO context_ref_log (manifest_id, item_id, event, detail_json, created_at)
             VALUES ('ctxm-p4-probe', 'ki-missing', 'EXPANDED', '{}', '2026-11-10T00:00:00Z')",
            [],
        );
        assert!(orphan_log.is_err(), "ref log without parent item must be rejected by composite FK");
        Ok::<(), database::rusqlite::Error>(())
    })
    .unwrap();

    // 删除语义（计划 §12:706）：删清单头连带条目与引用事件（ON DELETE CASCADE）。
    pool.with_conn(|c| {
        c.execute(
            "DELETE FROM context_manifest WHERE manifest_id = 'ctxm-p4-probe'",
            [],
        )
    })
    .unwrap();
    assert_eq!(count("context_manifest"), 0, "manifest deleted");
    assert_eq!(
        count("context_manifest_item"),
        0,
        "items cascade with manifest (no UPDATE path, append-only tables)"
    );
    assert_eq!(count("context_ref_log"), 0, "ref log cascade with manifest");
}
