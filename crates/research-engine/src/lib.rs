//! Phase 7 P7-6 研究能力契约化 crate（SESSION-HANDOFF §十追加项 2）。
//!
//! 职责：
//! - [`contract`]：七能力（planner / source-discovery / evidence-extraction /
//!   verification / synthesis / citation / evaluator）的输入输出契约面：
//!   14 份 request-v1/result-v1 JSON Schema（`schemas/research-capability/`，
//!   编译期内嵌）+ 信封类型 + 统一派发入口 [`contract::execute_request`]；
//! - [`capabilities`]：七能力确定性 mock 纯函数（typed 进 typed 出，六 ID
//!   锚经 domain::ids 校验，同输入恒同输出；不是模型调用体）；
//! - [`bin/rp-research`]：list / run 子命令，契约验收入口。
//!
//! 边界（本批冻结）：
//! - 不依赖 agent-engine（单向弱耦合：Phase 8 agent_loop 真实接线是
//!   agent-engine 侧引用本 crate）；
//! - 零迁移：契约面纯 Schema 加代码，不占 016 号段；
//! - 不推翻 P7-2 已验闭环：本 crate 无状态、无 IO，不动任何既有数据面。

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod capabilities;
pub mod contract;

pub use contract::{
    execute_request, CapabilityId, CapabilityRequest, CapabilityResult, ContractError,
    CAPABILITY_VERSION, PROTOCOL_VERSION,
};
