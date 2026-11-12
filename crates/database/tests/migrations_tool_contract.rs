//! P1-3 验证载具：005_tool_contract.sql 在开发库副本上经 crates/database
//! 迁移执行器（migrate.rs:32-80 幂等机制）连续执行两次无差异。
//!
//! 判据（PHASE-ROADMAP §2.3 P1-3 原句）：开发库副本连续执行两次无差异，
//! 第二次零变更，user_version 与 schema_version 一致；
//! DDL 字段覆盖 TOOL-CONTRACT §3.3（run_id/task_id/tool_id/input_ref/status/
//! timestamps）与 §5.2（artifact_id/run_id/uri/hash/mime/size/provenance）。
//!
//! 开发库副本路径：`migrations/dev-p13.db`（项目内，*.db 已 gitignore；
//! 每次运行先删旧副本重建，platform.db 与 platform-dryrun.db 不写）。

use database::{discover_migrations, migrate, DbPath, Pool, PoolConfig};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

fn repo_migrations_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../migrations")
        .canonicalize()
        .expect("migrations dir must exist")
}

fn dev_copy_path() -> PathBuf {
    repo_migrations_dir().join("dev-p13.db")
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

/// 本文件各用例共用同一开发库副本（reset 会互删对方打开中的文件，
/// 并行时触发 SQLITE_READONLY / disk I/O，第 1 轮门禁在
/// migrations_context_broker.rs 实测同款竞态），进程内串行化访问。
static DEV_COPY_LOCK: Mutex<()> = Mutex::new(());

#[test]
fn p1_3_migrate_twice_no_diff_on_dev_copy() {
    let _guard = DEV_COPY_LOCK.lock().unwrap();
    let dir = repo_migrations_dir();
    let discovered = discover_migrations(&dir).unwrap();
    let versions: Vec<i64> = discovered.iter().map(|m| m.version).collect();
    assert_eq!(
        versions,
        vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15],
        "discover must see 001..014 in order (006 = P7-1 提前通道，007 = P2-4a RuntimeNode 数据面，008 = P4-1 Context Broker 三表，均按合入顺序顺延；013 = P7-3 Agent Memory 三表, 014 = P7-4 运行控制与步骤轨迹, 015 = P7-5 trace 统一关联)"
    );

    reset_dev_copy();
    let pool = open_dev_pool();

    // 第一次：全链 001..011 落地，含本批 005_tool_contract、P7-1 的 006、
    // P2-4a 的 007、P4-1 的 008 与 P6-2 的 009。
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
        ],
        "first run must apply exactly 001..015"
    );

    let dump1 = schema_dump(&pool);
    let uv1 = user_version(&pool);
    let sv1 = schema_version_rows(&pool);
    assert_eq!(
        uv1, 15,
        "user_version must be 15 after first run (chain tail 015)"
    );
    assert!(
        sv1.iter()
            .any(|(v, by)| *v == 5 && by == "005_tool_contract.sql"),
        "schema_version must carry the version 5 registration row: {sv1:?}"
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
fn p1_3_ddl_covers_contract_3_3_and_5_2() {
    let _guard = DEV_COPY_LOCK.lock().unwrap();
    reset_dev_copy();
    let pool = open_dev_pool();
    migrate(&pool, &repo_migrations_dir()).unwrap();

    let columns = |table: &str| -> Vec<String> {
        pool.with_conn(|c| {
            let mut stmt = c.prepare(&format!("PRAGMA table_info({table})")).unwrap();
            let rows = stmt.query_map([], |r| r.get::<_, String>(1)).unwrap();
            rows.collect::<Result<Vec<_>, _>>()
        })
        .unwrap()
    };

    // §3.3 最小字段：run_id/task_id/tool_id/input_ref/status/timestamps。
    let run_cols = columns("tool_run");
    for required in [
        "run_id",
        "task_id",
        "tool_id",
        "input_ref",
        "status",
        "created_at",
        "started_at",
        "finished_at",
        "updated_at",
    ] {
        assert!(
            run_cols.iter().any(|c| c == required),
            "tool_run must cover {required} (TOOL-CONTRACT 3.3); got {run_cols:?}"
        );
    }

    // §5.2 / 计划 §12:698：artifact_id/run_id/uri/hash/mime/size/provenance。
    let art_cols = columns("artifact");
    for required in [
        "artifact_id",
        "run_id",
        "uri",
        "sha256",
        "size_bytes",
        "mime_type",
        "provenance",
    ] {
        assert!(
            art_cols.iter().any(|c| c == required),
            "artifact must cover {required} (TOOL-CONTRACT 5.2); got {art_cols:?}"
        );
    }

    // 状态域：八态之外的值必须被 CHECK 拒绝（先落一行合法 tool 满足 FK）。
    pool.with_conn(|c| {
        c.execute(
            "INSERT INTO tool (tool_id, name, governance, created_at, updated_at)
             VALUES ('tool:p13probe', 'p13-probe', 'ACTIVE', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            [],
        )?;
        let ok = c.execute(
            "INSERT INTO tool_run (run_id, tool_id, workspace_id, status, created_at, updated_at)
             VALUES ('rn-p13-ok', 'tool:p13probe', 'ws-p13', 'QUEUED', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            [],
        );
        assert!(ok.is_ok(), "legal QUEUED insert must succeed: {:?}", ok.err());
        let bad = c.execute(
            "INSERT INTO tool_run (run_id, tool_id, workspace_id, status, created_at, updated_at)
             VALUES ('rn-p13-bad', 'tool:p13probe', 'ws-p13', 'FINISHED', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            [],
        );
        assert!(bad.is_err(), "non-contract status must be rejected by CHECK");
        let bad_role = c.execute(
            "INSERT INTO artifact (artifact_id, run_id, role, uri, sha256, size_bytes, mime_type, provenance, created_at)
             VALUES ('art-p13-bad', 'rn-p13-ok', 'screenshot', 'mem://x', ?, 1, 'text/plain', 'probe', '2026-01-01T00:00:00Z')",
            ["0".repeat(64)],
        );
        assert!(bad_role.is_err(), "non-contract artifact role must be rejected by CHECK");
        let bad_hash = c.execute(
            "INSERT INTO artifact (artifact_id, run_id, role, uri, sha256, size_bytes, mime_type, provenance, created_at)
             VALUES ('art-p13-short', 'rn-p13-ok', 'log', 'mem://x', 'abc', 1, 'text/plain', 'probe', '2026-01-01T00:00:00Z')",
            [],
        );
        assert!(bad_hash.is_err(), "sha256 shorter than 64 must be rejected by CHECK");
        Ok::<(), database::rusqlite::Error>(())
    })
    .unwrap();
}
