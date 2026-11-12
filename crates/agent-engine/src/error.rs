//! Agent Engine 错误（各依赖面错误转文本携带，保持单一错误类型；
//! 引用校验拒绝保留 domain 原语义词形）。

/// Agent Engine 错误。
#[derive(Debug, Clone, PartialEq)]
pub enum AgentError {
    /// 定义缺失或字段非法（含引用校验拒绝原因原文）。
    Definition(String),
    /// (agent_id, version) 重复登记（agent_definition 主键冲突）。
    DefinitionExists {
        /// agent id。
        agent_id: String,
        /// 定义版本。
        version: String,
    },
    /// run 行不存在。
    RunNotFound(String),
    /// AgentRun 状态 CAS 迁移失败（非法对或并发漂移，from 为库内当前态）。
    InvalidTransition {
        /// run id。
        run_id: String,
        /// 库内当前状态。
        from: String,
        /// 请求的目标状态。
        to: String,
    },
    /// 规划/护栏拒绝（能力绑定外的工具、无步骤等）。
    Plan(String),
    /// Broker 装配/展开失败（ContextBrokerError 原文）。
    Broker(String),
    /// Tool Engine 受理/驱动失败（EngineError 原文）。
    Tool(String),
    /// 底层存储错误。
    Storage(String),
}

impl std::fmt::Display for AgentError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AgentError::Definition(r) => write!(f, "agent definition error: {r}"),
            AgentError::DefinitionExists { agent_id, version } => {
                write!(f, "agent definition {agent_id}@{version} already exists")
            }
            AgentError::RunNotFound(id) => write!(f, "agent run {id:?} not found"),
            AgentError::InvalidTransition { run_id, from, to } => {
                write!(f, "agent run {run_id} transition {from} -> {to} rejected")
            }
            AgentError::Plan(r) => write!(f, "plan rejected: {r}"),
            AgentError::Broker(r) => write!(f, "context broker error: {r}"),
            AgentError::Tool(r) => write!(f, "tool engine error: {r}"),
            AgentError::Storage(r) => write!(f, "storage error: {r}"),
        }
    }
}

impl std::error::Error for AgentError {}

impl From<domain::DomainError> for AgentError {
    fn from(e: domain::DomainError) -> Self {
        AgentError::Definition(e.to_string())
    }
}

impl From<context_broker::ContextBrokerError> for AgentError {
    fn from(e: context_broker::ContextBrokerError) -> Self {
        AgentError::Broker(e.to_string())
    }
}

impl From<tool_engine::EngineError> for AgentError {
    fn from(e: tool_engine::EngineError) -> Self {
        AgentError::Tool(e.to_string())
    }
}

impl From<database::DbError> for AgentError {
    fn from(e: database::DbError) -> Self {
        AgentError::Storage(e.to_string())
    }
}

impl From<tool_registry::RegistryError> for AgentError {
    fn from(e: tool_registry::RegistryError) -> Self {
        AgentError::Definition(format!("registry lookup failed: {e}"))
    }
}
