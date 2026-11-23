//! Reports 域存取面（03-DOMAIN-MODEL §1.9 :124/:132-133 落库，Phase 6b D4）。
//!
//! 形状冻结：report_id `rep-` 前缀 / task_id 稳定 ID 引用（非跨库 FK）/
//! report_ir JSONB（Report IR v1 = `schemas/report-ir/report-ir-v1.schema.json`，
//! 词表由 IR schema 单源承载——本层禁 CHECK 词表，禁第二事实源）/
//! format_exports JSONB additive-only 导出台账 / created_at。
//!
//! 分层纪律：report-engine 纯函数零 I/O（只出 IR 与渲染字节），持久化全部在本层；
//! 本层只收 `serde_json::Value`（不引 report-engine 类型——避免 crate 反向依赖）。
//! RTIS 零接触：report_ir 内 `rtis://` 引用只读透传零解析（6A 红线 4 词形）。

use crate::{new_entity_id, IntelError, IntelResult, PgPool};
use postgres::Row;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// 单条格式导出台账项（format_exports 的 value 形状）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FormatExportEntry {
    /// 导出产物 sha256（hex 小写 64）。
    pub sha256: String,
    /// 产物字节数。
    pub bytes: u64,
    /// 导出时间（RFC3339 / ISO8601 文本）。
    pub exported_at: String,
}

/// 报告写入输入（report_ir 为 Report IR v1 JSON 值）。
#[derive(Debug, Clone)]
pub struct StoreReportInput {
    /// 任务稳定 ID 引用（非跨库 FK）。
    pub task_id: String,
    /// Report IR v1（调用面以 schema 校验后传入）。
    pub report_ir: Value,
}

/// 报告读回结果。
#[derive(Debug, Clone)]
pub struct StoredReport {
    /// `rep-` 前缀报告 ID。
    pub report_id: String,
    /// 任务稳定 ID。
    pub task_id: String,
    /// Report IR v1。
    pub report_ir: Value,
    /// 导出台账（format → 条目）。
    pub format_exports: Value,
    /// 创建时间（库侧文本化）。
    pub created_at: String,
}

/// 写报告（生成 `rep-` ID；format_exports 缺省 `{}`）。
pub fn store_report(pool: &PgPool, input: &StoreReportInput) -> IntelResult<StoredReport> {
    if input.task_id.trim().is_empty() {
        return Err(IntelError::Invalid("task_id must be non-empty".to_string()));
    }
    if !input.report_ir.is_object() {
        return Err(IntelError::Invalid(
            "report_ir must be a JSON object (Report IR v1)".to_string(),
        ));
    }
    let report_id = new_entity_id("rep-");
    let mut conn = pool.get()?;
    conn.execute(
        "INSERT INTO reports (report_id, task_id, report_ir) VALUES ($1, $2, $3)",
        &[&report_id, &input.task_id, &input.report_ir],
    )?;
    let row = conn.query_one(
        "SELECT created_at::text FROM reports WHERE report_id = $1",
        &[&report_id],
    )?;
    Ok(StoredReport {
        report_id,
        task_id: input.task_id.clone(),
        report_ir: input.report_ir.clone(),
        format_exports: Value::Object(Default::default()),
        created_at: row.get(0),
    })
}

/// 读单条报告（不存在返回 None）。
pub fn load_report(pool: &PgPool, report_id: &str) -> IntelResult<Option<StoredReport>> {
    let mut conn = pool.get()?;
    let row = conn.query_opt(
        "SELECT report_id, task_id, report_ir, format_exports, created_at::text \
         FROM reports WHERE report_id = $1",
        &[&report_id],
    )?;
    Ok(row.map(|r| row_to_report(&r)))
}

/// 按 task 列台账（created_at 升序，同 task 多版本可追溯）。
pub fn list_reports_by_task(pool: &PgPool, task_id: &str) -> IntelResult<Vec<StoredReport>> {
    let mut conn = pool.get()?;
    let rows = conn.query(
        "SELECT report_id, task_id, report_ir, format_exports, created_at::text \
         FROM reports WHERE task_id = $1 ORDER BY created_at, report_id",
        &[&task_id],
    )?;
    Ok(rows.iter().map(row_to_report).collect())
}

/// 导出台账 additive 写面：按 format 键 upsert，不改动他键
/// （`format_exports || jsonb_build_object($2, $3::jsonb)`）。
pub fn upsert_format_export(
    pool: &PgPool,
    report_id: &str,
    format: &str,
    entry: &FormatExportEntry,
) -> IntelResult<Value> {
    if format.trim().is_empty() {
        return Err(IntelError::Invalid("format must be non-empty".to_string()));
    }
    let entry_value: Value = serde_json::to_value(entry)
        .map_err(|e| IntelError::Invalid(format!("format export entry serialize: {e}")))?;
    let mut conn = pool.get()?;
    let row = conn.query_opt(
        "UPDATE reports SET format_exports = format_exports || jsonb_build_object($2::text, $3::jsonb) \
         WHERE report_id = $1 RETURNING format_exports",
        &[&report_id, &format, &entry_value],
    )?;
    match row {
        Some(r) => Ok(r.get(0)),
        None => Err(IntelError::Invalid(format!(
            "report not found: {report_id}"
        ))),
    }
}

fn row_to_report(row: &Row) -> StoredReport {
    StoredReport {
        report_id: row.get(0),
        task_id: row.get(1),
        report_ir: row.get(2),
        format_exports: row.get(3),
        created_at: row.get(4),
    }
}
