//! capability 数据面注册（PLUGIN-CONTRACT §3.1；ROADMAP P6-6）。
//!
//! 分发纪律：install 的 REGISTERED 半边按 capability type 分派：
//! - tool / tool_pack → [`tool_register`]（落 Tool Registry，runtime 七种
//!   之内校验 + remote_runner 注册拒绝，TOOL-CONTRACT D-TC-01 口径）；
//! - project_endpoint → [`project_endpoint`]（MCP initialize 握手 + 工具
//!   列表快照哈希进 plugin.lock.negotiation + mcp_client runtime 工具映射）；
//! - knowledge_source → [`crate::ingest`]（P6-5 摄取管道，独立命令面）；
//! - agent_definition / workflow_definition / integration_connector →
//!   数据面行已随 plugin_capability 落库，深注册归 P7-1/P8-1（本批仅登记）。

pub mod project_endpoint;
pub mod tool_register;
