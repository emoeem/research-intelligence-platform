//! 013 起的追加迁移在开发库副本上幂等执行两次无差异（P7-3 自验项顺延，
//! 链尾随批次演进至 014；开发库副本取 migrations/dev-p7.db，P7-2 验收留下
//! 的真实开发数据，user_version=12 起点）。
//!
//! 流程：把 dev-p7.db 拷贝到临时目录（原件零写入），连续执行两次
//! database::migrate：第一次应只落 013..014（12 个既有迁移按 user_version
//! 跳过；P7-4 起 014_agent_run_control 为链尾），第二次必须 no-op，且两次
//! 之间 sqlite_master 快照零差异。
//!
//! dev-p7.db 不入 git（gitignore），缺席时本测试跳过并打印说明（不伪造
//! 通过；正式幂等判据另见 migration_idempotent.rs 的全量新库路径）。

#![deny(missing_docs)]

use database::{DbPath, Pool, PoolConfig};
use std::path::PathBuf;
use std::time::Duration;

/// schema 快照（sqlite_master 全量：type/name/sql 排序拼接）。
fn schema_snapshot(pool: &Pool) -> String {
    pool.with_conn(|c| -> database::rusqlite::Result<String> {
        let mut stmt = c
            .prepare(
                "SELECT type, name, sql FROM sqlite_master
                 WHERE name NOT LIKE 'sqlite_%' ORDER BY type, name",
            )
            .unwrap();
        let rows: Vec<(String, String, Option<String>)> = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        Ok(rows
            .into_iter()
            .map(|(t, n, s)| format!("{t}|{n}|{}", s.unwrap_or_default()))
            .collect::<Vec<_>>()
            .join("\n"))
    })
    .unwrap()
}

#[test]
fn migration_013_is_idempotent_on_dev_copy() {
    let src = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../migrations/dev-p7.db");
    if !src.is_file() {
        eprintln!("SKIP: migrations/dev-p7.db 不存在（gitignore 开发副本，可删文件重跑）");
        return;
    }
    let tmp = std::env::temp_dir().join(format!("rip-p73-dev-copy-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();
    let db_path = tmp.join("dev-p7-copy.db");
    std::fs::copy(&src, &db_path).unwrap();

    let pool = Pool::open(PoolConfig {
        path: DbPath::File(db_path),
        readonly: false,
        max_connections: 1,
        busy_timeout: Duration::from_secs(5),
    })
    .unwrap();

    let before_version: i64 = pool
        .with_conn(|c| c.query_row("PRAGMA user_version", [], |r| r.get(0)))
        .unwrap();
    assert_eq!(
        before_version, 12,
        "dev-p7.db 起点必须是 P7-2 后的 user_version=12"
    );

    let migrations_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../migrations");
    let first = database::migrate(&pool, &migrations_dir).unwrap();
    assert_eq!(
        first.iter().map(|m| m.version).collect::<Vec<_>>(),
        vec![13, 14, 15],
        "开发副本上第一次迁移应只落 013..015（链尾演进至 015，编号冻结规则顺延；P7-5 trace 关联 015 同链落列）"
    );
    let snapshot_after_first = schema_snapshot(&pool);

    let second = database::migrate(&pool, &migrations_dir).unwrap();
    assert!(second.is_empty(), "第二次必须 no-op: {second:?}");
    let snapshot_after_second = schema_snapshot(&pool);
    assert_eq!(
        snapshot_after_first, snapshot_after_second,
        "013..015 幂等执行两次 schema 必须零差异"
    );

    let after_version: i64 = pool
        .with_conn(|c| c.query_row("PRAGMA user_version", [], |r| r.get(0)))
        .unwrap();
    assert_eq!(after_version, 15, "链尾 015（P7-5 trace 统一关联）");

    // 三表在场 + 生产库未触核对（原件 user_version 仍为 12）。
    for table in [
        "agent_task_memory",
        "agent_evidence_memory",
        "agent_research_memory",
    ] {
        let present: i64 = pool
            .with_conn(|c| {
                c.query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
                    [table],
                    |r| r.get(0),
                )
            })
            .unwrap();
        assert_eq!(present, 1, "table {table} must exist after migration");
    }
    let readonly = Pool::open(PoolConfig {
        path: DbPath::File(src.clone()),
        readonly: true,
        max_connections: 1,
        busy_timeout: Duration::from_secs(5),
    })
    .unwrap();
    let src_version: i64 = readonly
        .with_conn(|c| c.query_row("PRAGMA user_version", [], |r| r.get(0)))
        .unwrap();
    assert_eq!(src_version, 12, "原件必须保持未写（只拷贝不落迁移）");

    let _ = std::fs::remove_dir_all(&tmp);
}
