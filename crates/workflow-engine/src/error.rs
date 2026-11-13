//! Workflow Engine 错误（单一错误类型，依赖面错误转文本携带；
//! agent-engine error.rs 同款惯例）。

/// Workflow Engine 错误。
#[derive(Debug, Clone, PartialEq)]
pub enum WorkflowError {
    /// definition-v1 文档解析失败（JSON 语法 / serde 形状 / schema 面常量）。
    Parse(String),
    /// definition-v1 Schema 校验拒绝（JSON Schema 门违规原因原文）。
    Schema(String),
    /// 图静态校验拒绝（环 / 孤立节点 / 必填参数 / 非法边端点等；
    /// 文本含 kind 词，CLI 逐条透传）。
    Validation(String),
    /// workflow_id + version 组合无定义行。
    NotFound {
        /// workflow id。
        workflow_id: String,
        /// 请求版本（None = latest 不存在，即该 workflow 无任何版本）。
        version: Option<u32>,
    },
    /// 底层存储错误（DbError 原文）。
    Storage(String),
    /// 旧行导入失败（源库 workflow 表缺失 / 行形状非法）。
    LegacyImport(String),
    /// run 行不存在。
    RunNotFound {
        /// run id。
        run_id: String,
    },
    /// run 状态跃迁非法（现态读回随错误携带；乐观守卫触发即报）。
    IllegalTransition {
        /// run id。
        run_id: String,
        /// 现态词形。
        from: String,
        /// 目标态词形。
        to: String,
    },
    /// 节点不可启动（乐观守卫只放行 pending；并发/残留行竞态即报）。
    NodeNotStartable {
        /// run id。
        run_id: String,
        /// 节点 id。
        node_id: String,
    },
    /// 受理面输入非法（run 输入非 object / 非法节点目标 / 非法 branch 边
    /// 载荷等；执行器受理校验失败原文）。
    InvalidInput(String),
    /// 执行面失败（节点派发链失败后的 run 级包装；载荷原文含 node_id 与
    /// code，文本含 kind 词 machine 可 grep）。
    Exec(String),
}

impl std::fmt::Display for WorkflowError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WorkflowError::Parse(r) => write!(f, "workflow definition parse error: {r}"),
            WorkflowError::Schema(r) => write!(f, "workflow definition schema violation: {r}"),
            WorkflowError::Validation(r) => write!(f, "workflow graph validation failed: {r}"),
            WorkflowError::NotFound { workflow_id, version } => match version {
                Some(v) => write!(f, "workflow definition {workflow_id}@v{v} not found"),
                None => write!(f, "workflow definition {workflow_id} not found"),
            },
            WorkflowError::Storage(r) => write!(f, "workflow storage error: {r}"),
            WorkflowError::LegacyImport(r) => write!(f, "legacy workflow import error: {r}"),
            WorkflowError::RunNotFound { run_id } => write!(f, "workflow run {run_id} not found"),
            WorkflowError::IllegalTransition { run_id, from, to } => {
                write!(
                    f,
                    "workflow run {run_id} illegal transition {from} -> {to} (domain frozen table)"
                )
            }
            WorkflowError::NodeNotStartable { run_id, node_id } => {
                write!(f, "workflow node {run_id}/{node_id} not startable in pending")
            }
            WorkflowError::InvalidInput(r) => write!(f, "workflow run input invalid: {r}"),
            WorkflowError::Exec(r) => write!(f, "workflow run execution error: {r}"),
        }
    }
}

impl std::error::Error for WorkflowError {}

impl From<database::DbError> for WorkflowError {
    fn from(value: database::DbError) -> Self {
        WorkflowError::Storage(value.to_string())
    }
}

impl From<domain::DomainError> for WorkflowError {
    fn from(value: domain::DomainError) -> Self {
        WorkflowError::Parse(value.to_string())
    }
}

impl From<serde_json::Error> for WorkflowError {
    fn from(value: serde_json::Error) -> Self {
        WorkflowError::Parse(value.to_string())
    }
}
