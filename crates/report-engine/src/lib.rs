//! Report Engine：Report IR v1 单源 + 三格式渲染器（PHASE-6A）。
//!
//! 公开面 = 纯函数零 I/O（D4）：`ReportIr`（serde 内存模型）+ `from_json`
//! （schema 校验 fail-closed）+ 四渲染函数（Markdown/HTML/JSON/SARIF）+
//! `validate_ir`。四渲染全部确定性纯函数——同 IR 两次渲染字节一致（判据
//! 10；RIP3 PLAN :541 禁每格式重跑 AI）。渲染函数内禁读库/读盘/起子进程
//! （红线 1；装配面属 6b）。

pub mod escape;
pub mod ir;
pub mod render;

pub use ir::{from_json, validate_ir, ReportIr};
pub use render::html::render_html;
pub use render::json::render_json;
pub use render::markdown::render_markdown;
pub use render::sarif::render_sarif;
