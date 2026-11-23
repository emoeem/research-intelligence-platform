//! V0008 reports 表集成测试（PHASE-6B-TASK.md §0 D4④；document_domain.rs:21
//! skip_unless_pg 词形同构）。
//!
//! 门禁口径：RP_TEST_PG_DSN 未设或不可达时跳过（V-9，容器端到端保持本地
//! 执行）；生产 PG 实例零连接（V0008 只在开发库副本与测试库执行——V0007
//! 头注词形）。
//!
//! 判据（D4④）：
//! 1. V0008 幂等：refinery 二跑零变化；
//! 2. rep- 前缀 roundtrip：store → load 对账 report_ir 字节面 + rep- 前缀；
//! 3. format_exports additive 写面：按 format 键 upsert 不改他键；
//! 4. list_reports_by_task：同 task 多条台账可列（created_at 升序）。

use intel_db::report_repo::{self, FormatExportEntry, StoreReportInput};
use intel_db::{open_pool, run_migrations, PgConfig};

fn skip_unless_pg() -> Option<intel_db::PgPool> {
    let dsn = std::env::var("RP_TEST_PG_DSN").ok()?;
    let pool = match open_pool(&PgConfig {
        dsn,
        max_connections: 4,
    }) {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: RP_TEST_PG_DSN 不可达（V-9，容器端到端保持本地执行）");
            return None;
        }
    };
    // V0008 表面就绪（refinery 幂等：已应用版本跳过——测试名字母序无关，
    // 每测试独立就绪，不依赖 migrations 测试先跑）。
    let mut conn = pool.get().unwrap();
    run_migrations(&mut conn).unwrap();
    Some(pool)
}

/// 最小 Report IR v1 形状 fixture（repo 层只断言 JSON 对象面——词表由
/// IR schema 单源承载，本层禁第二事实源）。
fn fixture_ir(tag: &str) -> serde_json::Value {
    serde_json::json!({
        "schema": "report-ir-v1",
        "title": format!("P6b 落库验收 {tag}"),
        "findings": [],
        "tag": tag
    })
}

fn unique_task_id() -> String {
    format!(
        "task-p6b-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    )
}

#[test]
fn v0008_reports_idempotent() {
    let Some(pool) = skip_unless_pg() else { return };
    let mut conn = pool.get().unwrap();
    let _first = run_migrations(&mut conn).unwrap();
    let second = run_migrations(&mut conn).unwrap();
    assert!(second.is_empty(), "V0008 二跑必须 no-op: {second:?}");
}

#[test]
fn rep_prefix_roundtrip_ir_bytes() {
    let Some(pool) = skip_unless_pg() else { return };
    let task_id = unique_task_id();
    let ir = fixture_ir("roundtrip");
    let stored = report_repo::store_report(
        &pool,
        &StoreReportInput {
            task_id: task_id.clone(),
            report_ir: ir.clone(),
        },
    )
    .unwrap();
    assert!(
        stored.report_id.starts_with("rep-"),
        "rep- 前缀冻结面: {}",
        stored.report_id
    );
    let loaded = report_repo::load_report(&pool, &stored.report_id)
        .unwrap()
        .expect("store 后必须可读回");
    assert_eq!(loaded.report_id, stored.report_id);
    assert_eq!(loaded.task_id, task_id);
    // report_ir 字节面对账（serde_json 规范化序列化逐字节相等）。
    assert_eq!(
        serde_json::to_string(&loaded.report_ir).unwrap(),
        serde_json::to_string(&ir).unwrap(),
        "report_ir roundtrip 字节面不一致"
    );
    assert_eq!(
        loaded.format_exports,
        serde_json::json!({}),
        "缺省台账为空对象",
    );
    // 不存在报告读面 = None。
    assert!(report_repo::load_report(&pool, "rep-does-not-exist")
        .unwrap()
        .is_none());
}

#[test]
fn format_exports_additive_upsert() {
    let Some(pool) = skip_unless_pg() else { return };
    let task_id = unique_task_id();
    let stored = report_repo::store_report(
        &pool,
        &StoreReportInput {
            task_id,
            report_ir: fixture_ir("exports"),
        },
    )
    .unwrap();
    let pdf = FormatExportEntry {
        sha256: "a".repeat(64),
        bytes: 1024,
        exported_at: "2026-11-23T00:00:00Z".to_string(),
    };
    let after_pdf =
        report_repo::upsert_format_export(&pool, &stored.report_id, "pdf", &pdf).unwrap();
    assert_eq!(after_pdf["pdf"]["bytes"], 1024);
    // 第二格式 upsert 不改他键（additive-only）。
    let docx = FormatExportEntry {
        sha256: "b".repeat(64),
        bytes: 2048,
        exported_at: "2026-11-23T00:00:01Z".to_string(),
    };
    let after_docx =
        report_repo::upsert_format_export(&pool, &stored.report_id, "docx", &docx).unwrap();
    assert_eq!(after_docx["pdf"]["bytes"], 1024, "他键不得被改写");
    assert_eq!(after_docx["docx"]["sha256"], "b".repeat(64));
    // 同 format 重导出 = 原位覆盖（re-export 语义）。
    let pdf2 = FormatExportEntry {
        sha256: "c".repeat(64),
        bytes: 4096,
        exported_at: "2026-11-23T00:00:02Z".to_string(),
    };
    let after_re =
        report_repo::upsert_format_export(&pool, &stored.report_id, "pdf", &pdf2).unwrap();
    assert_eq!(after_re["pdf"]["bytes"], 4096);
    assert_eq!(after_re["docx"]["bytes"], 2048, "re-export 不改他键");
    // 不存在报告 upsert = Invalid 报错（保留错误信息）。
    let err = report_repo::upsert_format_export(&pool, "rep-missing", "pdf", &pdf2).unwrap_err();
    assert!(
        err.to_string().contains("report not found"),
        "错误信息保留: {err}"
    );
}

#[test]
fn list_reports_by_task_asc() {
    let Some(pool) = skip_unless_pg() else { return };
    let task_id = unique_task_id();
    let first = report_repo::store_report(
        &pool,
        &StoreReportInput {
            task_id: task_id.clone(),
            report_ir: fixture_ir("list-1"),
        },
    )
    .unwrap();
    let second = report_repo::store_report(
        &pool,
        &StoreReportInput {
            task_id: task_id.clone(),
            report_ir: fixture_ir("list-2"),
        },
    )
    .unwrap();
    let rows = report_repo::list_reports_by_task(&pool, &task_id).unwrap();
    assert_eq!(rows.len(), 2, "同 task 台账两条");
    assert_eq!(
        rows[0].report_id, first.report_id,
        "created_at 升序（store 顺序）"
    );
    assert_eq!(rows[1].report_id, second.report_id);
}
