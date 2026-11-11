//! RIP 统一域模型（纯类型库）。
//!
//! 边界约定（MODULE_BOUNDARIES §8.1 / §8.2.1）：domain 被所有模块依赖，
//! 自身不依赖任何 workspace crate 或外部 crate（纯 std）。本批收编：
//!
//! - [`resource`]：Resource 实体 + LifecycleStatus / GovernanceStatus 双轴状态枚举
//! - [`audit`]：AuditEvent 纯数据类型（audit-engine 的 Emit 载荷）
//! - [`ids`]：Resource 稳定 ID 新类型（`gh:owner/repo` / `url:hash16`）；
//!   2.0 追加 RunId / ArtifactId（TOOL-CONTRACT §4.1 / §5.2）
//! - [`error`]：域错误类型
//! - [`time`]：UTC ISO8601 时间戳（纯 std 实现，供各 engine 复用）
//! - [`tool`]：ToolRunStatus 八态枚举（TOOL-CONTRACT §3.1，P1-2 收编，
//!   2.0 新实体唯一合法落点为本 crate，盘点 §2.1）
//! - [`node`]：RuntimeNode 数据面类型（计划 §12:696 + D-TC-07 健康词表，
//!   P2-4a 收编；节点级健康判定属 runtime-gateway，本 crate 只承载身份）
//! - [`context`]：Context Broker 域类型（计划 §12:703 ContextManifest 词表
//!   与设计稿 §4.1 IntentQuery/ContentRef，P4-1 收编；五类词表与 008 迁移
//!   CHECK 域一一对应）
//!
//! 状态机口径：lifecycle 为处理阶段链，governance 为治理状态，两轴正交
//! （M1 评审修正 K-08，见 migrations/001_init.sql 头注）；tool_run 状态机
//! 跃迁合法性校验属 tool-engine（P1-5），domain 只承载状态身份。
#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod agent;
pub mod audit;
pub mod context;
pub mod error;
pub mod ids;
pub mod knowledge;
pub mod node;
pub mod plugin;
pub mod resource;
pub mod time;
pub mod tool;

pub use agent::{AgentDefinition, AgentId, AgentRun, AgentRunState, CapabilityRef};
pub use context::{
    ContentRef, ContextItemKind, ContextLayer, InclusionForm, IntentQuery, IntentStatus, RefEvent,
    TargetSlot, ToolFamily,
};
pub use error::DomainError;
pub use ids::{ArtifactId, ResourceId, RunId};
pub use node::{NodeHealth, RuntimeNode, RuntimeNodeKind};
pub use plugin::{
    CapabilityType, Plugin, PluginCapability, PluginId, PluginKind, PluginState, PluginTrust,
};
pub use resource::{GovernanceStatus, LifecycleStatus, QualityGrade, Resource};
pub use tool::ToolRunStatus;
