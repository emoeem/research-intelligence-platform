//! P9-4 协议一致性载具 crate（ROADMAP §10.4：`cargo test --offline -p rp-mcp-parity`；
//! §10.5 风险 1：MCP 面与 Web 逻辑漂移是最容易复发的债，一致性测试载具必须是
//! 常驻 CI 门禁而不是一次性验收）。
//!
//! crate 本体无业务实现（`#[deny(missing_docs)]` 占位 lib），全部断言在
//! tests/parity.rs：五个业务操作（列工具、查 Schema、跑工具、查 Run、查
//! trace）对 REST 与 MCP 两通道断言同一输出形状与同一审计链。

#![forbid(unsafe_code)]
