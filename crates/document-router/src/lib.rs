//! Document Router（DI-1 ADR D1）：文档域编排层——手写魔数嗅探（D6）→ 通道表
//! 编排（D3）→ 能力/帽前置门（D7）→ 规范化（D3 统一输出 Schema）。RT-0.5-E
//! §3 路由图最小形态，档位判定前置承接 RT-3（接入点 DI-3 登记）。
//!
//! 层次纪律（D1/D2）：
//! - Router 只消费 tool-engine 公开 API
//!   [`tool_engine::adapter::execute_native_rust_via_engine`] 与
//!   [`tool_engine::capability::StaticCatalogProvider`]，每一步工具执行都是
//!   真实 ToolRun（状态机/信封/Artifact/Evidence/Trace 既有面）；
//! - Router 自身零 `std::process::Command`（D2 否决绕过 adapter 直接 spawn）；
//! - 不建第二事实源（E §5:122：ToolRun/Artifact/Evidence/Trace/预算/取消
//!   全部复用引擎既有面）；
//! - 错误信封只用九类词表 + RT-1 NATIVE_* 码表 + D8 表新增 code（category
//!   不新增）。
//!
//! 二进制输入暂存（D5）：子 run 输入由 Router 在驱动该 run **之前**预写至
//! `<workspace_root>/<run_id>/in/<staged 名>`；adapter `input_content` 仅承载
//! UTF-8 文本（不扩），级联产物重写进下一 run 自己的 in/（不绕过 in/ 前缀
//! 校验）。

pub mod channel;
pub mod error;
pub mod normalize;
pub mod router;
pub mod sniff;

// Router 主面 crate 根 re-export（dispatch/测试消费面；D1 落位面）。
pub use router::{RouteOutcome, RouteStatus, Router, RouterConfig, RouterManifests, RunRecord};
