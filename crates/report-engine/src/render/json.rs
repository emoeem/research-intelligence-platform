//! JSON 渲染器（PHASE-6A D4：`render_json`——native IR JSON，serde
//! Serialize 直出面，零 I/O）。
//!
//! 确定性：serde_json 默认 Map = BTreeMap（键序稳定）+ Vec 保序——同 IR
//! 两次渲染字节一致（判据 10）。

use crate::ir::ReportIr;

/// IR → native IR JSON（Value 形态——导出面与 SARIF 双形态并列，D4）。
pub fn render_json(ir: &ReportIr) -> serde_json::Value {
    serde_json::to_value(ir).expect("report-ir serialize is infallible: string-keyed maps only")
}
