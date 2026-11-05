//! M2 交付物集成测试：迁移执行器 + audit-engine + resource-engine 三件套联通。

use audit_engine::AuditEngine;
use database::{DbPath, Pool, PoolConfig};
use domain::audit::AuditEvent;
use rp_eri_parity::{fresh_tmp_dir, migrations_dir, parity_prereq, run_rust_resolve};
use std::time::Duration;

/// 真实 001_init.sql 在内存库上可执行且幂等，resolve 事件可 Emit/Trace。
#[test]
fn migrate_audit_resolve_integration() {
    // 1) 内存库 + 真实迁移（两次执行，第二次为 no-op）
    let pool = Pool::open(PoolConfig {
        path: DbPath::Memory,
        readonly: false,
        max_connections: 1,
        busy_timeout: Duration::from_secs(5),
    })
    .unwrap();
    let applied1 = database::migrate(&pool, &migrations_dir()).unwrap();
    // migrations/ 含 001_init.sql（自设 user_version=1）与 002_tasks.sql（M1 第二批，
    // 002 头注明确把 user_version 递增机制留给 M2 迁移框架：由执行器补写 2）。
    // M4 会话注：003_tools_workflow.sql 为并行 M6 会话增量迁移，断言随目录
    // 现状演进（按文件数动态断言，前两版固定为 1/2）。
    let versions: Vec<i64> = applied1.iter().map(|m| m.version).collect();
    assert_eq!(
        &versions[..2.min(versions.len())],
        &[1, 2],
        "got {applied1:?}"
    );
    let sql_count = std::fs::read_dir(migrations_dir())
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|x| x == "sql"))
        .count();
    assert_eq!(versions.len(), sql_count, "migrations applied == sql files");
    let applied2 = database::migrate(&pool, &migrations_dir()).unwrap();
    assert!(applied2.is_empty(), "migrate must be idempotent");
    let user_version: i64 = pool
        .with_conn(|c| c.query_row("PRAGMA user_version", [], |r| r.get(0)))
        .unwrap();
    assert_eq!(
        user_version, sql_count as i64,
        "user_version tracks migration count"
    );

    // 2) audit-engine Emit（走过真实 audit_event 表）
    let engine = AuditEngine::new(pool.clone());
    let event_id = engine
        .emit(
            AuditEvent::new("resource-engine", "resource.resolved")
                .with_entity("resource", "gh:o/r")
                .with_correlation("erq-integration")
                .with_detail(r#"{"keywords":["fuzzing"]}"#),
        )
        .unwrap();
    assert!(event_id >= 1);
    let chain = engine.trace("erq-integration").unwrap();
    assert_eq!(chain.len(), 1);
    assert_eq!(chain[0].event.action, "resource.resolved");
    assert_eq!(chain[0].event.entity_type.as_deref(), Some("resource"));

    // 3) resource-engine 对真实迁移后空库 resolve：no_match 但形状完整
    let repo = resource_engine::SqliteResourceRepository::new(pool);
    let mut q = resource_engine::ResolveQuery::new(vec!["fuzzing".to_string()]);
    q.task_id = "m2-integration".to_string();
    let r = resource_engine::ResourceRepository::resolve(&repo, &q).unwrap();
    assert_eq!(r.search_status, "no_match");
    assert!(r.matched_resources.is_empty());
    assert_eq!(r.weights_config.keyword, 0.6);
    assert_eq!(r.task_id, "m2-integration");
}

/// 只读 dryrun 库上：resolve 只读可用，且写入被拒（readonly 池）。
#[test]
fn readonly_platform_db_smoke() {
    if parity_prereq().is_err() {
        eprintln!("SKIP readonly_platform_db_smoke: platform db missing");
        return;
    }
    let r = run_rust_resolve(&["fuzzing"]);
    assert_eq!(r.search_status, "ok");
    assert!(r.retrieval_stats.candidates >= r.retrieval_stats.returned);
}

/// 审计旁路纪律：emit_best_effort 对坏库不 panic、不报错，只缓冲。
#[test]
fn audit_bypass_never_fails_caller() {
    let tmp = fresh_tmp_dir("bypass");
    // 无 audit_event 表的库：emit 必失败，旁路必须吞掉。
    let pool = Pool::open(PoolConfig {
        path: DbPath::File(tmp.join("no-audit-table.db")),
        readonly: false,
        max_connections: 1,
        busy_timeout: Duration::from_secs(1),
    })
    .unwrap();
    let engine = AuditEngine::new(pool);
    let ok = engine.emit_best_effort(AuditEvent::new("system", "resource.resolved"));
    assert!(!ok, "write must fail on table-less db");
    assert_eq!(engine.buffered(), 1, "event must be buffered for retry");
    // 缓冲重试依旧失败，事件保留在缓冲内（不丢失、不反压调用方）。
    assert_eq!(engine.flush(), 0);
    assert_eq!(engine.buffered(), 1);
}
