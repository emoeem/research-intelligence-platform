//! P2-4a 验证载具：007_runtime_node.sql 在 DbPath::Memory 测试库上经
//! crates/database 迁移执行器（migrate.rs:67 幂等机制）连续执行两次 schema 无差异。
//!
//! 判据（P2-4a 完成判据原文）：007 经 crates/database::migrate 在
//! DbPath::Memory 测试库连续执行两次 schema 无差异；第二次零变更；
//! user_version 与 schema_version 登记行一致；runtime_node 列覆盖计划
//! §12:696 字段清单（id/kind/endpoint/health/version）+ 心跳时间戳；
//! CHECK 域拒绝词表外值（D-TC-07 健康四值 + kind 三值）。
//!
//! 红线：全程内存库（DbPath::Memory），不触任何 *.db 文件；
//! platform.db 与 platform-dryrun.db 零改动。

use database::{discover_migrations, migrate, DbPath, Pool, PoolConfig};
use std::path::{Path, PathBuf};
use std::time::Duration;

fn repo_migrations_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../migrations")
        .canonicalize()
        .expect("migrations dir must exist")
}

fn open_memory_pool() -> Pool {
    Pool::open(PoolConfig {
        path: DbPath::Memory,
        readonly: false,
        max_connections: 1,
        busy_timeout: Duration::from_secs(5),
    })
    .expect("open memory pool")
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

#[test]
fn p2_4a_migrate_twice_no_diff_on_memory() {
    let dir = repo_migrations_dir();
    let discovered = discover_migrations(&dir).unwrap();
    let versions: Vec<i64> = discovered.iter().map(|m| m.version).collect();
    assert_eq!(
        versions,
        vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15],
        "discover must see 001..014 in order (007 = P2-4a 按合入顺序先到先得，005/006 已占用；008 = P4-1 Context Broker 三表, 009 = P6-2 Plugin 数据面, 010 = P6-5 Knowledge 摄取数据面, 011 = SK-1 通用知识来源数据面, 013 = P7-3 Agent Memory 三表, 014 = P7-4 运行控制与步骤轨迹, 015 = P7-5 trace 统一关联)"
    );

    let pool = open_memory_pool();

    // 第一次：全链 001..009 落地，链尾 009_plugin（007/008 落于链中段）。
    let first = migrate(&pool, &dir).unwrap();
    let last = first.last().expect("at least one migration applied");
    assert_eq!(
        (last.version, last.name.as_str()),
        (15, "_trace_correlation"),
        "first run must land 015 last"
    );

    let dump1 = schema_dump(&pool);
    let uv1 = user_version(&pool);
    assert_eq!(uv1, 15, "user_version must be 15 after first run");

    // 第二次：执行器按 user_version 全跳过，零变更。
    let second = migrate(&pool, &dir).unwrap();
    assert!(second.is_empty(), "second run must be a no-op");
    assert_eq!(
        schema_dump(&pool),
        dump1,
        "schema must not differ after second run"
    );
    assert_eq!(user_version(&pool), uv1, "user_version must be identical");

    // schema_version 登记行（version=7）。
    let sv: String = pool
        .with_conn(|c| {
            c.query_row(
                "SELECT applied_by FROM schema_version WHERE version = 7",
                [],
                |r| r.get(0),
            )
        })
        .unwrap();
    assert_eq!(sv, "007_runtime_node.sql");
}

#[test]
fn p2_4a_runtime_node_columns_and_check_domains() {
    let dir = repo_migrations_dir();
    let pool = open_memory_pool();
    migrate(&pool, &dir).unwrap();

    let columns: Vec<String> = pool
        .with_conn(|c| {
            let mut stmt = c.prepare("PRAGMA table_info(runtime_node)")?;
            let rows = stmt.query_map([], |r| r.get::<_, String>(1))?;
            rows.collect::<Result<Vec<_>, _>>()
        })
        .unwrap();

    // 计划 §12:696 五字段 + 心跳时间戳 + 审计时间戳。
    for required in [
        "node_id",
        "kind",
        "endpoint",
        "health",
        "version",
        "last_heartbeat_at",
        "created_at",
        "updated_at",
    ] {
        assert!(
            columns.iter().any(|c| c == required),
            "runtime_node must cover {required} (计划 §12:696 + 心跳落点); got {columns:?}"
        );
    }

    pool.with_conn(|c| {
        let now = "2026-01-01T00:00:00Z";
        // 合法：三种 kind 均可落行；health 缺省取 DEFAULT 'unknown'。
        for kind in ["python_worker", "podman_host", "podman_vm"] {
            let id = format!("node-ok-{kind}");
            c.execute(
                "INSERT INTO runtime_node (node_id, kind, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?3)",
                database::rusqlite::params![id, kind, now],
            )?;
            let health: String = c
                .query_row(
                    "SELECT health FROM runtime_node WHERE node_id = ?1",
                    [&id],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(
                health, "unknown",
                "health must default to unknown for kind {kind}"
            );
        }
        // 词表外 kind 必须被 CHECK 拒绝（remote_runner 按 D-TC-01 不入词表）。
        let bad_kind = c.execute(
            "INSERT INTO runtime_node (node_id, kind, created_at, updated_at)
             VALUES ('node-bad-kind', 'remote_runner', ?1, ?1)",
            [now],
        );
        assert!(
            bad_kind.is_err(),
            "non-vocabulary kind must be rejected by CHECK: {:?}",
            bad_kind.err()
        );
        // 词表外 health 必须被 CHECK 拒绝（D-TC-07 四值之外）。
        let bad_health = c.execute(
            "INSERT INTO runtime_node (node_id, kind, health, created_at, updated_at)
             VALUES ('node-bad-health', 'podman_host', 'UP', ?1, ?1)",
            [now],
        );
        assert!(
            bad_health.is_err(),
            "non-vocabulary health must be rejected by CHECK: {:?}",
            bad_health.err()
        );
        // 健康更新路径：unknown → ok（Gateway 心跳写面预演）。
        c.execute(
            "UPDATE runtime_node SET health = 'ok', last_heartbeat_at = ?2
             WHERE node_id = 'node-ok-podman_host'",
            database::rusqlite::params!["node-ok-podman_host", now],
        )?;
        let health: String = c
            .query_row(
                "SELECT health FROM runtime_node WHERE node_id = 'node-ok-podman_host'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(health, "ok", "heartbeat update must persist ok health");
        Ok::<(), database::rusqlite::Error>(())
    })
    .unwrap();
}
