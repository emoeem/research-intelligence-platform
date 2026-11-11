//! P7-1 验证载具：006_agent_data_plane.sql 在开发库副本上经 crates/database
//! 迁移执行器连续执行两次无差异（PHASE-ROADMAP §8.3 P7-1 判据）。
//!
//! 开发库副本：migrations/dev-p71.db（项目内，*.db 已 gitignore；每次运行
//! 先删旧副本重建；platform.db 与 platform-dryrun.db 不写）。

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
    repo_migrations_dir().join("dev-p71.db")
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
fn p7_1_migrate_twice_no_diff_and_fk_rejects_missing_definition() {
    let _guard = DEV_COPY_LOCK.lock().unwrap();
    let dir = repo_migrations_dir();
    let discovered = discover_migrations(&dir).unwrap();
    let versions: Vec<i64> = discovered.iter().map(|m| m.version).collect();
    assert_eq!(
        versions,
        vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11],
        "discover must see 001..011 in order (006 = next available by merge order, 007 = P2-4a RuntimeNode 数据面, 008 = P4-1 Context Broker 三表, 009 = P6-2 Plugin 数据面, 010 = P6-5 Knowledge 摄取数据面, 011 = SK-1 通用知识来源数据面)"
    );

    reset_dev_copy();
    let pool = open_dev_pool();
    let first = migrate(&pool, &dir).unwrap();
    assert!(
        first
            .iter()
            .any(|m| (m.version, m.name.as_str()) == (6, "_agent_data_plane")),
        "first run must still apply 006_agent_data_plane"
    );
    let last = first.last().expect("at least one migration applied");
    assert_eq!(
        (last.version, last.name.as_str()),
        (11, "_knowledge_sources"),
        "chain tail is now 011 (SK-1)，006/007/008/009/010 落于链中段"
    );

    let dump1 = schema_dump(&pool);
    let uv: i64 = pool
        .with_conn(|c| c.query_row("PRAGMA user_version", [], |r| r.get(0)))
        .unwrap();
    assert_eq!(
        uv, 11,
        "user_version must be 11 after first run (chain tail 011)"
    );

    // 第二次：执行器按 user_version 全跳过，零变更。
    let second = migrate(&pool, &dir).unwrap();
    assert!(second.is_empty(), "second run must be a no-op");
    assert_eq!(
        schema_dump(&pool),
        dump1,
        "schema must not differ after second run"
    );

    // schema_version 登记行（version=6）。
    let sv: String = pool
        .with_conn(|c| {
            c.query_row(
                "SELECT applied_by FROM schema_version WHERE version = 6",
                [],
                |r| r.get(0),
            )
        })
        .unwrap();
    assert_eq!(sv, "006_agent_data_plane.sql");

    // FK：agent_run 必须指向存在的 (agent_id, version) 定义。
    pool.with_conn(|c| {
        c.execute(
            "INSERT INTO agent_definition (agent_id, version, name, context_policy,
             capability_refs_json, created_at, updated_at)
             VALUES ('agent.demo', '0.1.0', 'Demo',
             '{\"l0\":true}',
             '[{\"tool_id\":\"rip.doc.parse\",\"version\":null}]',
             '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            [],
        )?;
        let ok = c.execute(
            "INSERT INTO agent_run (run_id, task_id, agent_id, agent_version, state,
             memory_refs_json, created_at, updated_at)
             VALUES ('rn-agent-ok', 't-1', 'agent.demo', '0.1.0', 'QUEUED',
             '[]', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            [],
        );
        assert!(
            ok.is_ok(),
            "run on existing definition must insert: {:?}",
            ok.err()
        );
        let bad = c.execute(
            "INSERT INTO agent_run (run_id, task_id, agent_id, agent_version, state,
             memory_refs_json, created_at, updated_at)
             VALUES ('rn-agent-fk', 't-1', 'agent.ghost', '9.9.9', 'QUEUED',
             '[]', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            [],
        );
        assert!(
            bad.is_err(),
            "run on missing definition must be rejected by FK"
        );
        let bad_state = c.execute(
            "INSERT INTO agent_run (run_id, task_id, agent_id, agent_version, state,
             memory_refs_json, created_at, updated_at)
             VALUES ('rn-agent-state', 't-1', 'agent.demo', '0.1.0', 'THINKING',
             '[]', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            [],
        );
        assert!(
            bad_state.is_err(),
            "non-contract state must be rejected by CHECK"
        );
        Ok::<(), database::rusqlite::Error>(())
    })
    .unwrap();
}
