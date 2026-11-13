//! workflow_definition（016）与 workflow 运行持久化（017）存储面集成
//! 测试（临时文件库）。
//!
//! 覆盖 P8-1 自验第 2 条与第 4 条、P8-2 运行持久化自验的库内层
//! （CLI 层等价断言在 RELEASE 实录）：
//! - put 版本追加语义：首版 version=1，重复 put version=2（历史不可变）；
//! - get --version 1 回读与首版逐字节一致（版本管理与历史可查）；
//! - 017/018 迁移在临时库重放两次：user_version 与表清单两次一致（幂等）；
//!   018 六 ID 关联列与控制表在位（P8-3）。

use database::rusqlite;
use database::{migrate, Pool};
use std::path::{Path, PathBuf};
use workflow_engine::dsl::parse_definition;
use workflow_engine::store::{writable_pool, WorkflowStore};

const FOUR_NODE: &str = include_str!("../../../docs/phase8/samples/four-node.json");
const CYCLE: &str = include_str!("../../../docs/phase8/samples/cycle.json");

/// 仓库根 migrations 目录（cargo test CWD 为 crate 目录，经 CARGO_MANIFEST_DIR 锚定）。
const MIGRATIONS_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../migrations");

/// 临时文件库路径（测试内唯一，测试目录由调用方清理）。
fn temp_db(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "rip-workflow-engine-test-{}-{}",
        tag,
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).expect("temp dir");
    dir.join("test.db")
}

fn open_migrated(path: &Path) -> Pool {
    let pool = writable_pool(path).expect("open writable pool");
    let applied = migrate(&pool, Path::new(MIGRATIONS_DIR)).expect("migrate");
    assert!(
        applied.iter().any(|m| m.version == 16),
        "016 必须在本次迁移中应用（临时库 user_version 从 0 起步）"
    );
    pool
}

#[test]
fn put_appends_versions_and_get_is_byte_identical() {
    let db = temp_db("put-get");
    let pool = open_migrated(&db);
    let store = WorkflowStore::new(pool);

    let doc = parse_definition(FOUR_NODE).expect("parse four node sample");
    let first = store.put(&doc, "test-put").expect("put v1");
    assert_eq!(first.workflow_id, "wf-four-node");
    assert_eq!(first.version, 1);
    assert_eq!(first.compat, "graph-v1");
    assert_eq!((first.node_count, first.edge_count), (4, 3));

    // 首版回读快照（字节一致性基准）
    let v1_before = store.get("wf-four-node", Some(1)).expect("get v1").definition_json;
    // 规范形状：version 字段与存储行一致
    assert!(v1_before.contains("\"version\":1"), "{v1_before}");

    // 重复 put：同内容也追加一版（版本管理语义：每 put 一版，历史可查）
    let second = store.put(&doc, "test-put").expect("put v2");
    assert_eq!(second.version, 2);

    // get --version 1 回读与首版逐字节一致（历史不可变）
    let stored = store.get("wf-four-node", Some(1)).expect("get v1");
    assert_eq!(stored.definition_json, v1_before);
    // 规范形状自证：规范化后与原始样例同语义（四节点三边），字节形状确定
    let doc_round = parse_definition(&stored.definition_json).expect("canonical text reparse");
    assert_eq!(doc_round.graph.nodes.len(), 4);
    assert_eq!(doc_round.graph.edges.len(), 3);
    // 最新版（None）落在 v2，version 字段 = 2
    let latest = store.get("wf-four-node", None).expect("get latest");
    assert_eq!(latest.definition.version(), 2);
    let latest_json = latest.definition_json.clone();
    assert!(latest_json.contains("\"version\":2"), "{latest_json}");
    // 两次 get 同版本逐字节一致
    let again = store.get("wf-four-node", Some(1)).expect("get v1 again");
    assert_eq!(again.definition_json, v1_before);
    // 不存在的版本 / workflow
    assert!(matches!(
        store.get("wf-four-node", Some(3)),
        Err(workflow_engine::WorkflowError::NotFound { .. })
    ));
    assert!(matches!(
        store.get("wf-nope", None),
        Err(workflow_engine::WorkflowError::NotFound { .. })
    ));

    // list：两版归并一行，version_count=2
    let rows = store.list().expect("list");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].latest_version, 2);
    assert_eq!(rows[0].version_count, 2);

    std::fs::remove_dir_all(db.parent().expect("parent")).ok();
}

#[test]
fn put_rejects_invalid_definitions_without_writing_rows() {
    let db = temp_db("put-reject");
    let pool = open_migrated(&db);
    let store = WorkflowStore::new(pool);

    // 环图：静态校验拒绝（错误文本含 cycle），且整行不入库
    let doc = parse_definition(CYCLE).expect("parse cycle sample");
    let err = store.put(&doc, "test-put").expect_err("cycle rejected");
    assert!(err.to_string().contains("cycle"), "{err}");
    let rows = store.list().expect("list");
    assert!(rows.is_empty(), "拒绝后不留半登记: {rows:?}");
    std::fs::remove_dir_all(db.parent().expect("parent")).ok();
}

#[test]
fn migration_017_replay_twice_is_idempotent() {
    let db = temp_db("replay");
    let pool = writable_pool(&db).expect("open");

    // 第一次迁移：001..017 全量
    let first = migrate(&pool, Path::new(MIGRATIONS_DIR)).expect("first migrate");
    let version_1: i64 = pool
        .with_conn(|c| c.query_row("PRAGMA user_version", [], |r| r.get(0)))
        .expect("user_version");
    let tables_1 = table_list(&pool);

    // 第二次重放：0 行新增，user_version 与表清单不变
    let second = migrate(&pool, Path::new(MIGRATIONS_DIR)).expect("second migrate");
    let version_2: i64 = pool
        .with_conn(|c| c.query_row("PRAGMA user_version", [], |r| r.get(0)))
        .expect("user_version");
    let tables_2 = table_list(&pool);

    assert!(first.iter().any(|m| m.version == 17), "first: {first:?}");
    assert!(first.iter().any(|m| m.version == 18), "first: {first:?}");
    assert!(!second.iter().any(|m| m.version == 17), "重放不应重复应用: {second:?}");
    assert!(!second.iter().any(|m| m.version == 18), "重放不应重复应用: {second:?}");
    assert_eq!(version_1, 18);
    assert_eq!(version_2, 18);
    assert_eq!(tables_1, tables_2, "两次重放表清单一致（幂等）");
    assert!(tables_1.contains(&"workflow_definition".to_string()));
    assert!(tables_1.contains(&"workflow_run".to_string()));
    assert!(tables_1.contains(&"workflow_run_node".to_string()));
    assert!(tables_1.contains(&"workflow_checkpoint".to_string()));
    assert!(tables_1.contains(&"workflow_run_control".to_string()));
    // 018 追加列在位（六 ID 关联列 + 节点实体链接列）。
    for (table, column) in [
        ("workflow_run", "trace_id"),
        ("workflow_run", "task_id"),
        ("workflow_run", "agent_run_id"),
        ("workflow_run", "tool_run_id"),
        ("workflow_run", "artifact_id"),
        ("workflow_run", "evidence_id"),
        ("workflow_run_node", "tool_run_id"),
        ("workflow_run_node", "agent_run_id"),
    ] {
        let present: i64 = pool
            .with_conn(|c| {
                c.query_row(
                    "SELECT COUNT(*) FROM pragma_table_info(?1) WHERE name = ?2",
                    rusqlite::params![table, column],
                    |r| r.get(0),
                )
            })
            .expect("pragma_table_info");
        assert_eq!(present, 1, "{table}.{column} 必须在位");
    }

    // 另开第二个临时库直跑两次 017 SQL 原文（raw file replay 口径，同 002 头注守卫）：
    // 017 无 ALTER 语句，全部 IF NOT EXISTS / INSERT OR IGNORE，重放安全。
    let sql = std::fs::read_to_string(format!("{MIGRATIONS_DIR}/017_workflow_run.sql")).expect("read 017");
    let second_pool = writable_pool(&db).expect("reopen");
    second_pool
        .with_conn(|c| c.execute_batch(&sql).map_err(database::DbError::Sqlite))
        .expect("raw replay 017 once");
    second_pool
        .with_conn(|c| c.execute_batch(&sql).map_err(database::DbError::Sqlite))
        .expect("raw replay 017 twice");
    let version_raw: i64 = second_pool
        .with_conn(|c| c.query_row("PRAGMA user_version", [], |r| r.get(0)))
        .expect("user_version");
    assert_eq!(version_raw, 17, "raw 重放后 user_version 仍为 17");
    std::fs::remove_dir_all(db.parent().expect("parent")).ok();
}

fn table_list(pool: &Pool) -> Vec<String> {
    pool.with_conn(|c| {
        let mut stmt = c
            .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
            .expect("prepare");
        let rows = stmt
            .query_map([], |r| r.get::<_, String>(0))
            .expect("query_map")
            .collect::<Result<Vec<_>, _>>()
            .expect("rows");
        Ok::<_, rusqlite::Error>(rows)
    })
    .expect("table list")
}
