//! 迁移副本幂等验收（P7-1 判据延续，P7-3 自验项）：把仓库 migrations 目录
//! 原样拷贝到临时目录，对临时文件库连续执行两次 database::migrate，第二次
//! 必须 no-op，且两次之间 schema 对象无差异（sqlite_master 全量比对）。

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
fn migrations_copy_is_idempotent_with_no_schema_diff() {
    let src = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../migrations");
    let tmp = std::env::temp_dir().join(format!("rip-agent-migration-copy-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();
    for entry in std::fs::read_dir(&src).unwrap() {
        let entry = entry.unwrap();
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if name_str.ends_with(".sql") {
            std::fs::copy(entry.path(), tmp.join(&*name)).unwrap();
        }
    }
    let db_path = tmp.join("idempotency-check.db");
    let pool = Pool::open(PoolConfig {
        path: DbPath::File(db_path),
        readonly: false,
        max_connections: 1,
        busy_timeout: Duration::from_secs(5),
    })
    .unwrap();

    let first = database::migrate(&pool, &tmp).unwrap();
    let applied_first: Vec<i64> = first.iter().map(|m| m.version).collect();
    let snapshot_after_first = schema_snapshot(&pool);
    // 001 到 019 全量执行（013 为 P7-3 agent memory 三表，014 为 P7-4
    // 运行控制与步骤轨迹两表，015 为 P7-5 trace 关联，016 为 P8-1
    // workflow 定义版本化，017 为 P8-2 workflow 运行持久化，018 为 P8-3
    // workflow 控制面与六 ID 关联，019 为 P9-3 REST 对外访问审计表）。
    assert_eq!(
        applied_first,
        vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19],
        "all migrations must apply once"
    );

    let second = database::migrate(&pool, &tmp).unwrap();
    assert!(second.is_empty(), "second run must be a no-op: {second:?}");
    let snapshot_after_second = schema_snapshot(&pool);
    assert_eq!(
        snapshot_after_first, snapshot_after_second,
        "schema must not change across a repeated migration run"
    );

    let user_version: i64 = pool
        .with_conn(|c| c.query_row("PRAGMA user_version", [], |r| r.get(0)))
        .unwrap();
    assert_eq!(user_version, 19);

    let _ = std::fs::remove_dir_all(&tmp);
}
