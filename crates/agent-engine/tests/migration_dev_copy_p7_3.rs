//! 013 起的追加迁移在开发库副本上幂等执行两次无差异（P7-3 自验项顺延，
//! 链尾随批次演进至 014；开发库副本取 migrations/dev-p7.db，P7-2 验收留下
//! 的真实开发数据，user_version=12 起点）。
//!
//! 流程：把 dev-p7.db 拷贝到临时目录（原件零写入），连续执行两次
//! database::migrate：第一次应只落 013..014（12 个既有迁移按 user_version
//! 跳过；P7-4 起 014_agent_run_control，P8-1 起 016_workflow_definition 为链尾），第二次必须 no-op，且两次
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
        vec![
            13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33
        ],
        "开发副本上第一次迁移应只落 013..033（链尾演进至 033（KSE source review history；032 Nuclei 模板知识类型；Projects/Research Assets 031，编号冻结规则顺延；P7-5 trace 关联 015 同链落列，P8-1 workflow 定义版本化 016 同链落表，P8-2 workflow 运行持久化 017 同链落表，P8-3 workflow 控制面与六 ID 关联 018 同链落列，P9-3 REST 对外访问审计 019 同链落表，RIP3 P5-2 研究编排域三表 020 同链落表，RIP3 P5-7 验证证据档 research_evidence 021 同链落表，RIP3 P5-8 覆盖矩阵 dimension 列与 research_hypothesis 022 同链落列落表，WSW-1 CAS 正文表 023，WSW-2 wsw_intercept 拦截队列三表 024，WSW-3 wsw_exchange 四表 025，MA-1 caller/api_key 双表 026，MA-2 caller 归属四列与 caller_budget 027，MA-3 协作协议 task_claim/ma_message 029，WSW-5/6 WS 帧与被动候选两表 030，Projects/Research Assets 031）"
    );
    let snapshot_after_first = schema_snapshot(&pool);

    let second = database::migrate(&pool, &migrations_dir).unwrap();
    assert!(second.is_empty(), "第二次必须 no-op: {second:?}");
    let snapshot_after_second = schema_snapshot(&pool);
    assert_eq!(
        snapshot_after_first, snapshot_after_second,
        "013..033 幂等执行两次 schema 必须零差异"
    );

    let after_version: i64 = pool
        .with_conn(|c| c.query_row("PRAGMA user_version", [], |r| r.get(0)))
        .unwrap();
    assert_eq!(
        after_version, 33,
        "链尾 033（KSE source review history；前序 032 Nuclei template kind；031 Projects/Research Assets；前序 030 WSW-5/6 WS 帧与被动候选两表；前序 029 MA-3 协作协议 task_claim/ma_message 两表，028 WSW-4 wsw_fuzz 两表，027 MA-2 caller 归属四列与 caller_budget，026 MA-1 caller/api_key 双表，025 WSW-3 wsw_exchange 四表）"
    );

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
