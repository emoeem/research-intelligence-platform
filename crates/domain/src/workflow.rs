//! Workflow 域模型（Phase 8 P8-1 新实体，纯类型零依赖）。
//!
//! 口径（计划 §12:701-702 实体行 + PHASE-ROADMAP §9.3 P8-1）：
//! - [`WorkflowDefinition`]：workflow_id、version、graph、input_schema
//!   （graph 以结构化节点/边向量承载，input_schema 以原样 JSON 串承载）；
//! - [`WorkflowRun`]：run_id、workflow_id、state、node_states（计划 §12:702），
//!   另加 workflow_version 钉定字段：run 必须钉死在定义的某一版本上执行，
//!   版本管理（P8-3 长任务语义）按 (workflow_id, workflow_version) 回放，
//!   不漂移到最新版。
//!
//! 边界（如实声明，与 domain::agent 同款分工）：
//! - graph 静态校验（环 / 孤立节点 / 必填参数 / 非法边端点）属
//!   workflow-engine 的 validate 模块纯函数（P8-1 判据），domain 只承载
//!   结构与身份校验（id 形状、节点 id 唯一性、边端点非空、状态词表）；
//! - WorkflowRun 持久化面（run 表与节点状态表）属 P8-2/P8-3 批次
//!   （迁移 17 号段待占，016 只有定义表）；本批先冻结 run 状态词表与
//!   跃迁表（[`WorkflowRunState::can_transition`]），词值即未来迁移
//!   CHECK 域，届时按此实现，不回改本批已登记行；
//! - 003 workflow 表（RTIS 方法层定义，实查 13 行 WF-00..WF-12）不是
//!   可执行节点图：旧行以 [`WorkflowCompat::LegacyMethodReference`] 标记
//!   只读参考导入（workflow-engine store::import_legacy），不为兼容旧
//!   形态扭曲 DSL（ROADMAP §9.5 风险 2）。

use crate::error::DomainError;
use crate::ids::RunId;
use std::fmt;

/// Workflow 定义的 workflow_id（非空且不超过 128 字符；不做字符集强约束，
/// 与 AgentId 同风格：兼容存量 WF-00 形式的参考行与 `wf-` 前缀新行）。
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct WorkflowId(String);

impl WorkflowId {
    /// 长度上限（与冻结契约 run_id maxLength 同口径）。
    pub const MAX_LEN: usize = 128;

    /// 校验并构造。
    pub fn try_new(value: impl Into<String>) -> Result<Self, DomainError> {
        let value = value.into();
        let reason = if value.is_empty() {
            "empty workflow id".to_string()
        } else if value.chars().count() > Self::MAX_LEN {
            format!("workflow id exceeds {} chars", Self::MAX_LEN)
        } else {
            return Ok(Self(value));
        };
        Err(DomainError::InvalidWorkflowId { value, reason })
    }

    /// 内部值视图。
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// 消费自身，返回内部 String。
    pub fn into_inner(self) -> String {
        self.0
    }
}

impl fmt::Display for WorkflowId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl AsRef<str> for WorkflowId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for WorkflowId {
    type Error = DomainError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        WorkflowId::try_new(value)
    }
}

impl TryFrom<&str> for WorkflowId {
    type Error = DomainError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        WorkflowId::try_new(value.to_string())
    }
}

/// 节点 id 校验（node 与 edge 端点共用）：非空且不超过 64 字符，字符集
/// `[A-Za-z0-9][A-Za-z0-9_.-]*`（与 schema pattern 同口径，禁止空白与
/// 路径分隔符，防止画布 JSON-Pointer 式注入歧义）。
pub(crate) fn check_node_id(value: &str) -> Result<(), String> {
    if value.is_empty() {
        return Err("empty node id".to_string());
    }
    if value.chars().count() > 64 {
        return Err("node id exceeds 64 chars".to_string());
    }
    let mut chars = value.chars();
    let first = chars.next().expect("non-empty checked above");
    let ok_first = first.is_ascii_alphanumeric();
    let ok_rest = chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.' || c == '-');
    if !(ok_first && ok_rest) {
        return Err(format!("node id {value:?} outside [A-Za-z0-9][A-Za-z0-9_.-]*"));
    }
    Ok(())
}

/// Workflow 节点类型词表（计划 §14:828 五类执行体 + 图边界两值 + P8-3
/// 人工介入一值）：start/end 为图边界（P8-1 起静态校验要求恰好一个 start、
/// 至少一个 end），tool/knowledge/agent/condition/parallel 为计划 Phase 8
/// 节点五类（P8-2 落非智能执行器，P8-3 落 Agent 节点），human 为 P8-3
/// 人工介入节点（停在 awaiting_approval，批准/注入参数后恢复续跑；P8-1
/// 冻结注的「扩展须新增变体并升迁移」落点：017 尚未入库提交，CHECK 词值
/// 随本变体批内补入，dev 副本可重建）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum WorkflowNodeKind {
    /// 图入口（恰好一个）。
    Start,
    /// 图出口（至少一个）。
    End,
    /// Tool 节点（P8-2 执行）。
    Tool,
    /// Knowledge 节点（P8-2 执行，经 Broker 检索）。
    Knowledge,
    /// Agent 节点（P8-3 执行，Phase 7 agent-engine 为执行体）。
    Agent,
    /// 人工介入节点（P8-3 执行：停在 awaiting_approval，批准参数注入后
    /// 恢复续跑）。
    Human,
    /// Condition 节点（P8-2 执行，branch 边走向运行期判定）。
    Condition,
    /// Parallel 节点（P8-2 执行，branch 边扇出并发）。
    Parallel,
}

impl WorkflowNodeKind {
    /// 存储词形（与 016 之外未来 run 表 CHECK 域、definition-v1 Schema
    /// enum 同词值）。
    pub fn as_str(&self) -> &'static str {
        match self {
            WorkflowNodeKind::Start => "start",
            WorkflowNodeKind::End => "end",
            WorkflowNodeKind::Tool => "tool",
            WorkflowNodeKind::Knowledge => "knowledge",
            WorkflowNodeKind::Agent => "agent",
            WorkflowNodeKind::Human => "human",
            WorkflowNodeKind::Condition => "condition",
            WorkflowNodeKind::Parallel => "parallel",
        }
    }

    /// 从存储词形解析（domain 词表一一对应）。
    pub fn parse(value: &str) -> Result<Self, DomainError> {
        match value {
            "start" => Ok(WorkflowNodeKind::Start),
            "end" => Ok(WorkflowNodeKind::End),
            "tool" => Ok(WorkflowNodeKind::Tool),
            "knowledge" => Ok(WorkflowNodeKind::Knowledge),
            "agent" => Ok(WorkflowNodeKind::Agent),
            "human" => Ok(WorkflowNodeKind::Human),
            "condition" => Ok(WorkflowNodeKind::Condition),
            "parallel" => Ok(WorkflowNodeKind::Parallel),
            other => Err(DomainError::UnknownWorkflowNodeKind(other.to_string())),
        }
    }
}

impl fmt::Display for WorkflowNodeKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// 边类型词表：seq 为顺序流（唯一前驱语义由 P8-2 执行器解释），branch 为
/// 条件/并行分支（Condition 与 Parallel 节点的出边词形，运行期判定走向）。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum WorkflowEdgeKind {
    /// 顺序流边（默认值）。
    #[default]
    Seq,
    /// 分支边（condition/parallel 出边）。
    Branch,
}

impl WorkflowEdgeKind {
    /// 存储词形。
    pub fn as_str(&self) -> &'static str {
        match self {
            WorkflowEdgeKind::Seq => "seq",
            WorkflowEdgeKind::Branch => "branch",
        }
    }

    /// 从存储词形解析。
    pub fn parse(value: &str) -> Result<Self, DomainError> {
        match value {
            "seq" => Ok(WorkflowEdgeKind::Seq),
            "branch" => Ok(WorkflowEdgeKind::Branch),
            other => Err(DomainError::UnknownWorkflowEdgeKind(other.to_string())),
        }
    }
}

impl fmt::Display for WorkflowEdgeKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// 定义兼容面（016 workflow_definition.compat CHECK 域，两值一一对应）：
/// graph-v1 为可执行节点图；legacy-method-reference 为 003 存量行的只读
/// 参考导入（空图 + legacy 快照落 legacy_json 列），永不进入执行器。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum WorkflowCompat {
    /// 可执行节点图（默认）。
    #[default]
    GraphV1,
    /// 003 方法层定义只读参考（非可执行）。
    LegacyMethodReference,
}

impl WorkflowCompat {
    /// 存储词形（016 CHECK 域词值）。
    pub fn as_str(&self) -> &'static str {
        match self {
            WorkflowCompat::GraphV1 => "graph-v1",
            WorkflowCompat::LegacyMethodReference => "legacy-method-reference",
        }
    }

    /// 从存储词形解析。
    pub fn parse(value: &str) -> Result<Self, DomainError> {
        match value {
            "graph-v1" => Ok(WorkflowCompat::GraphV1),
            "legacy-method-reference" => Ok(WorkflowCompat::LegacyMethodReference),
            other => Err(DomainError::UnknownWorkflowCompat(other.to_string())),
        }
    }
}

impl fmt::Display for WorkflowCompat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Workflow 图节点：id + 类型 + params 原样 JSON 串。
///
/// params_json 为 DSL 节点 params 子对象的原样 JSON 串（None = 无 params）；
/// 必填参数校验（tool 节点必填 tool_id 等）在 workflow-engine validate 按
/// 类型词表执行，domain 不解释 params 内容。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkflowNode {
    id: String,
    kind: WorkflowNodeKind,
    params_json: Option<String>,
}

impl WorkflowNode {
    /// 校验并构造（id 形状 + 词表 + params_json 必须是 JSON object 串或 None；
    /// JSON 合法性由调用方以 serde_json 解析验证，domain 零依赖不解析）。
    pub fn try_new(
        id: impl Into<String>,
        kind: WorkflowNodeKind,
        params_json: Option<String>,
    ) -> Result<Self, DomainError> {
        let id = id.into();
        check_node_id(&id).map_err(|reason| DomainError::InvalidWorkflowNode {
            field: "id".to_string(),
            reason,
        })?;
        Ok(Self {
            id,
            kind,
            params_json,
        })
    }

    /// 节点 id。
    pub fn id(&self) -> &str {
        &self.id
    }

    /// 节点类型。
    pub fn kind(&self) -> WorkflowNodeKind {
        self.kind
    }

    /// params 原样 JSON 串（None = 无 params）。
    pub fn params_json(&self) -> Option<&str> {
        self.params_json.as_deref()
    }
}

/// Workflow 图边：from/to 端点 + 边类型 + 可选条件载荷原样 JSON 串。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkflowEdge {
    from: String,
    to: String,
    kind: WorkflowEdgeKind,
    when_json: Option<String>,
}

impl WorkflowEdge {
    /// 校验并构造（端点 id 形状校验；端点是否指向图中存在的节点属
    /// workflow-engine validate 静态校验，domain 不持图引用）。
    pub fn try_new(
        from: impl Into<String>,
        to: impl Into<String>,
        kind: WorkflowEdgeKind,
        when_json: Option<String>,
    ) -> Result<Self, DomainError> {
        let from = from.into();
        let to = to.into();
        for (field, value) in [("from", &from), ("to", &to)] {
            check_node_id(value).map_err(|reason| DomainError::InvalidWorkflowEdge {
                field: field.to_string(),
                reason,
            })?;
        }
        Ok(Self {
            from,
            to,
            kind,
            when_json,
        })
    }

    /// 起点节点 id。
    pub fn from(&self) -> &str {
        &self.from
    }

    /// 终点节点 id。
    pub fn to(&self) -> &str {
        &self.to
    }

    /// 边类型。
    pub fn kind(&self) -> WorkflowEdgeKind {
        self.kind
    }

    /// 条件载荷原样 JSON 串（None = 无条件）。
    pub fn when_json(&self) -> Option<&str> {
        self.when_json.as_deref()
    }
}

/// WorkflowDefinition（计划 §12:701）：workflow_id、version、graph、
/// input_schema，另加 name/description（人类可读面）与 compat（兼容面标记）。
///
/// 本类型不持 JSON 文档：definition-v1 文档与 domain 向量的互转属
/// workflow-engine dsl 模块（serde 面）。domain 构造器只做身份校验
/// （workflow_id/name/version/节点 id/边端点形状），图拓扑（环/孤立/
/// 必填参数/边端点存在性）一律以 workflow-engine validate 为准（P8-1
/// 判据，避免两处规则漂移）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkflowDefinition {
    workflow_id: WorkflowId,
    version: u32,
    name: String,
    description: Option<String>,
    compat: WorkflowCompat,
    nodes: Vec<WorkflowNode>,
    edges: Vec<WorkflowEdge>,
    input_schema_json: Option<String>,
}

impl WorkflowDefinition {
    /// 校验并构造。
    // 参数面 = 计划 §12:701 实体行 + name/description/compat 三个辅助字段
    // （P8-2/P8-3 执行器按此签名消费；dsl::to_domain 为唯一调用面）。
    #[allow(clippy::too_many_arguments)]
    pub fn try_new(
        workflow_id: WorkflowId,
        version: u32,
        name: impl Into<String>,
        description: Option<String>,
        compat: WorkflowCompat,
        nodes: Vec<WorkflowNode>,
        edges: Vec<WorkflowEdge>,
        input_schema_json: Option<String>,
    ) -> Result<Self, DomainError> {
        let name = name.into();
        let reason = if name.trim().is_empty() {
            Some("empty name".to_string())
        } else if version == 0 {
            Some("version must be >= 1".to_string())
        } else {
            None
        };
        if let Some(reason) = reason {
            return Err(DomainError::InvalidWorkflowDefinition { reason });
        }
        // 节点 id 唯一性：同图内重复 id 会使边端点语义歧义，属身份面而非
        // 拓扑面，在 domain 构造期即拒绝。
        let mut seen = std::collections::BTreeSet::new();
        for node in &nodes {
            if !seen.insert(node.id().to_string()) {
                return Err(DomainError::InvalidWorkflowDefinition {
                    reason: format!("duplicate node id {:?}", node.id()),
                });
            }
        }
        Ok(Self {
            workflow_id,
            version,
            name,
            description,
            compat,
            nodes,
            edges,
            input_schema_json,
        })
    }

    /// workflow_id。
    pub fn workflow_id(&self) -> &WorkflowId {
        &self.workflow_id
    }

    /// 定义版本（put 语义：每版追加，历史不可变）。
    pub fn version(&self) -> u32 {
        self.version
    }

    /// 人类可读名。
    pub fn name(&self) -> &str {
        &self.name
    }

    /// 描述。
    pub fn description(&self) -> Option<&str> {
        self.description.as_deref()
    }

    /// 兼容面。
    pub fn compat(&self) -> WorkflowCompat {
        self.compat
    }

    /// 图节点向量。
    pub fn nodes(&self) -> &[WorkflowNode] {
        &self.nodes
    }

    /// 图边向量。
    pub fn edges(&self) -> &[WorkflowEdge] {
        &self.edges
    }

    /// input_schema 原样 JSON 串（None = 未声明输入 Schema）。
    pub fn input_schema_json(&self) -> Option<&str> {
        self.input_schema_json.as_deref()
    }
}

/// WorkflowRun 状态词表（保守核心六态：覆盖计划 §14:830「运行、暂停、
/// 恢复、失败重试和版本管理」的最小状态面；词值即未来 17 号段迁移
/// CHECK 域，扩展须新增变体并升迁移，不回改本批已登记行）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum WorkflowRunState {
    /// 已受理未启动。
    Queued,
    /// 执行中。
    Running,
    /// 暂停（协作式，P8-3 续接语义）。
    Paused,
    /// 成功终态。
    Succeeded,
    /// 失败终态（可重试：P8-3 经 retry 起新 run，本行不再跃迁）。
    Failed,
    /// 取消终态。
    Cancelled,
}

impl WorkflowRunState {
    /// 存储词形（未来 run 表 CHECK 域词值）。
    pub fn as_str(&self) -> &'static str {
        match self {
            WorkflowRunState::Queued => "queued",
            WorkflowRunState::Running => "running",
            WorkflowRunState::Paused => "paused",
            WorkflowRunState::Succeeded => "succeeded",
            WorkflowRunState::Failed => "failed",
            WorkflowRunState::Cancelled => "cancelled",
        }
    }

    /// 从存储词形解析。
    pub fn parse(value: &str) -> Result<Self, DomainError> {
        match value {
            "queued" => Ok(WorkflowRunState::Queued),
            "running" => Ok(WorkflowRunState::Running),
            "paused" => Ok(WorkflowRunState::Paused),
            "succeeded" => Ok(WorkflowRunState::Succeeded),
            "failed" => Ok(WorkflowRunState::Failed),
            "cancelled" => Ok(WorkflowRunState::Cancelled),
            other => Err(DomainError::UnknownWorkflowRunState(other.to_string())),
        }
    }

    /// 是否终态。
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            WorkflowRunState::Succeeded | WorkflowRunState::Failed | WorkflowRunState::Cancelled
        )
    }

    /// 跃迁表（P8-1 冻结基线，P8-3 执行器按此实现）：
    /// - queued → running | cancelled
    /// - running → paused | succeeded | failed | cancelled
    /// - paused → running（恢复）| cancelled
    /// - succeeded/failed/cancelled 为终态，不迁出（失败重试以新 run
    ///   承载并继承 trace/retry 链，先例 agent_engine RunControl::retry）。
    pub fn can_transition(&self, to: WorkflowRunState) -> bool {
        use WorkflowRunState::*;
        matches!(
            (self, to),
            (Queued, Running)
                | (Queued, Cancelled)
                | (Running, Paused)
                | (Running, Succeeded)
                | (Running, Failed)
                | (Running, Cancelled)
                | (Paused, Running)
                | (Paused, Cancelled)
        )
    }
}

impl fmt::Display for WorkflowRunState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// WorkflowRun 单节点状态（run 内 per-node 轨迹；持久化面 P8-2 落表，
/// 本批只冻结词表与载荷形状）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkflowNodeState {
    node_id: String,
    state: WorkflowNodeRunState,
    attempts: u32,
}

impl WorkflowNodeState {
    /// 构造（node_id 形状校验，attempts 从 0 起）。
    pub fn try_new(
        node_id: impl Into<String>,
        state: WorkflowNodeRunState,
        attempts: u32,
    ) -> Result<Self, DomainError> {
        let node_id = node_id.into();
        check_node_id(&node_id).map_err(|reason| DomainError::InvalidWorkflowNode {
            field: "node_id".to_string(),
            reason,
        })?;
        Ok(Self {
            node_id,
            state,
            attempts,
        })
    }

    /// 节点 id。
    pub fn node_id(&self) -> &str {
        &self.node_id
    }

    /// 节点运行状态。
    pub fn state(&self) -> WorkflowNodeRunState {
        self.state
    }

    /// 尝试次数（失败重试计数，0 = 未执行）。
    pub fn attempts(&self) -> u32 {
        self.attempts
    }
}

/// 节点运行状态词表（五态：Pending/Running/Succeeded/Failed/Skipped；
/// Skipped 为 condition 分支未选中节点的终态，P8-2 条件执行器落面）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum WorkflowNodeRunState {
    /// 未到达。
    Pending,
    /// 执行中。
    Running,
    /// 成功。
    Succeeded,
    /// 失败（可重试，attempts 计数）。
    Failed,
    /// 条件分支未选中跳过（终态）。
    Skipped,
}

impl WorkflowNodeRunState {
    /// 存储词形（未来节点状态表 CHECK 域词值）。
    pub fn as_str(&self) -> &'static str {
        match self {
            WorkflowNodeRunState::Pending => "pending",
            WorkflowNodeRunState::Running => "running",
            WorkflowNodeRunState::Succeeded => "succeeded",
            WorkflowNodeRunState::Failed => "failed",
            WorkflowNodeRunState::Skipped => "skipped",
        }
    }

    /// 从存储词形解析。
    pub fn parse(value: &str) -> Result<Self, DomainError> {
        match value {
            "pending" => Ok(WorkflowNodeRunState::Pending),
            "running" => Ok(WorkflowNodeRunState::Running),
            "succeeded" => Ok(WorkflowNodeRunState::Succeeded),
            "failed" => Ok(WorkflowNodeRunState::Failed),
            "skipped" => Ok(WorkflowNodeRunState::Skipped),
            other => Err(DomainError::UnknownWorkflowNodeRunState(other.to_string())),
        }
    }
}

impl fmt::Display for WorkflowNodeRunState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// WorkflowRun（计划 §12:702）：run_id、workflow_id、state、node_states，
/// 另加 workflow_version 钉定字段（run 钉死定义版本，版本管理按
/// (workflow_id, workflow_version) 回放，不漂移到最新版）。
///
/// 本批（P8-1）无持久化面：domain 类型 + 状态词表 + 跃迁表先行冻结，
/// store 落表属 P8-2/P8-3（迁移 17 号段待占）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkflowRun {
    run_id: RunId,
    workflow_id: WorkflowId,
    workflow_version: u32,
    state: WorkflowRunState,
    node_states: Vec<WorkflowNodeState>,
}

impl WorkflowRun {
    /// 构造（node_states 内 node_id 不重复；run_id 形状由 RunId 校验）。
    pub fn try_new(
        run_id: RunId,
        workflow_id: WorkflowId,
        workflow_version: u32,
        state: WorkflowRunState,
        node_states: Vec<WorkflowNodeState>,
    ) -> Result<Self, DomainError> {
        if workflow_version == 0 {
            return Err(DomainError::InvalidWorkflowRun(
                "workflow_version must be >= 1".to_string(),
            ));
        }
        let mut seen = std::collections::BTreeSet::new();
        for node_state in &node_states {
            if !seen.insert(node_state.node_id().to_string()) {
                return Err(DomainError::InvalidWorkflowRun(format!(
                    "duplicate node state for {:?}",
                    node_state.node_id()
                )));
            }
        }
        Ok(Self {
            run_id,
            workflow_id,
            workflow_version,
            state,
            node_states,
        })
    }

    /// run_id。
    pub fn run_id(&self) -> &RunId {
        &self.run_id
    }

    /// workflow_id。
    pub fn workflow_id(&self) -> &WorkflowId {
        &self.workflow_id
    }

    /// 钉定的定义版本。
    pub fn workflow_version(&self) -> u32 {
        self.workflow_version
    }

    /// run 状态。
    pub fn state(&self) -> WorkflowRunState {
        self.state
    }

    /// 节点状态向量。
    pub fn node_states(&self) -> &[WorkflowNodeState] {
        &self.node_states
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workflow_id_accepts_legacy_and_prefixed_forms() {
        assert!(WorkflowId::try_new("WF-00").is_ok());
        assert!(WorkflowId::try_new("wf-four-node").is_ok());
        assert!(WorkflowId::try_new("").is_err());
        assert!(WorkflowId::try_new("x".repeat(129)).is_err());
    }

    #[test]
    fn node_id_charset_matches_schema_pattern() {
        assert!(check_node_id("start").is_ok());
        assert!(check_node_id("fetch-spec_1.2").is_ok());
        assert!(check_node_id("with space").is_err());
        assert!(check_node_id("-lead").is_err());
        assert!(check_node_id("").is_err());
        assert!(check_node_id(&"a".repeat(65)).is_err());
    }

    #[test]
    fn node_kind_wordtable_round_trip() {
        let all = [
            WorkflowNodeKind::Start,
            WorkflowNodeKind::End,
            WorkflowNodeKind::Tool,
            WorkflowNodeKind::Knowledge,
            WorkflowNodeKind::Agent,
            WorkflowNodeKind::Human,
            WorkflowNodeKind::Condition,
            WorkflowNodeKind::Parallel,
        ];
        assert_eq!(all.len(), 8);
        for kind in all {
            assert_eq!(WorkflowNodeKind::parse(kind.as_str()), Ok(kind));
        }
        assert!(WorkflowNodeKind::parse("subgraph").is_err());
    }

    #[test]
    fn run_state_transition_table_covers_pause_and_retry_baseline() {
        use WorkflowRunState::*;
        // 正向链
        assert!(Queued.can_transition(Running));
        assert!(Running.can_transition(Paused));
        assert!(Paused.can_transition(Running));
        assert!(Running.can_transition(Succeeded));
        assert!(Running.can_transition(Failed));
        // 终态不迁出（失败重试以新 run 承载）
        assert!(!Failed.can_transition(Running));
        assert!(!Succeeded.can_transition(Running));
        assert!(!Cancelled.can_transition(Running));
        assert!(!Running.can_transition(Queued));
        assert!(Failed.is_terminal());
        assert!(!Paused.is_terminal());
    }

    #[test]
    fn node_run_state_wordtable_round_trip() {
        let all = [
            WorkflowNodeRunState::Pending,
            WorkflowNodeRunState::Running,
            WorkflowNodeRunState::Succeeded,
            WorkflowNodeRunState::Failed,
            WorkflowNodeRunState::Skipped,
        ];
        for state in all {
            assert_eq!(WorkflowNodeRunState::parse(state.as_str()), Ok(state));
        }
    }

    #[test]
    fn definition_rejects_duplicate_node_ids_and_empty_name() {
        let node = WorkflowNode::try_new("a", WorkflowNodeKind::Tool, None).expect("node");
        assert!(WorkflowDefinition::try_new(
            WorkflowId::try_new("wf-x").expect("id"),
            1,
            "dup",
            None,
            WorkflowCompat::GraphV1,
            vec![node.clone(), node],
            vec![],
            None,
        )
        .is_err());
        assert!(WorkflowDefinition::try_new(
            WorkflowId::try_new("wf-x").expect("id"),
            1,
            "  ",
            None,
            WorkflowCompat::GraphV1,
            vec![],
            vec![],
            None,
        )
        .is_err());
    }

    #[test]
    fn run_rejects_zero_version_and_duplicate_node_states() {
        let wid = WorkflowId::try_new("wf-x").expect("id");
        let rid = RunId::try_new("wfr-1").expect("run id");
        let ns =
            WorkflowNodeState::try_new("a", WorkflowNodeRunState::Pending, 0).expect("node state");
        assert!(WorkflowRun::try_new(rid.clone(), wid.clone(), 0, WorkflowRunState::Queued, vec![])
            .is_err());
        assert!(WorkflowRun::try_new(
            rid.clone(),
            wid.clone(),
            1,
            WorkflowRunState::Queued,
            vec![ns.clone(), ns],
        )
        .is_err());
        assert!(WorkflowRun::try_new(
            rid,
            wid,
            1,
            WorkflowRunState::Queued,
            vec![WorkflowNodeState::try_new(
                "a",
                WorkflowNodeRunState::Pending,
                0
            )
            .expect("node state")],
        )
        .is_ok());
    }
}
