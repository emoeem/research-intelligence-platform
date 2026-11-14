//! 域错误类型（纯 std，无 thiserror 依赖，锁定最少依赖集）。

use std::fmt;

/// 域层错误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DomainError {
    /// Resource ID 不符合 `gh:owner/repo` / `url:hash16` 稳定 ID 口径。
    InvalidResourceId {
        /// 被拒绝的原始值。
        value: String,
        /// 拒绝原因。
        reason: String,
    },
    /// 生命周期轴出现未知变体（schema CHECK 约束之外的值）。
    UnknownLifecycleVariant(String),
    /// 治理轴出现未知变体。
    UnknownGovernanceVariant(String),
    /// 质量等级出现未知变体（合法：A/B/C/D）。
    UnknownQualityGrade(String),
    /// 审计事件缺必填字段（actor / action）。
    InvalidAuditEvent(String),
    /// ToolRun 状态出现未知变体（TOOL-CONTRACT §3.1 八态之外的值）。
    UnknownToolRunStatus(String),
    /// RuntimeNode kind 出现未知变体（计划 §6 Runtime 类型表本阶段落面
    /// 三值 python_worker/podman_host/podman_vm 之外的值；remote_runner 不入词表）。
    UnknownRuntimeNodeKind(String),
    /// 节点健康出现未知变体（D-TC-07 四值 unknown/ok/degraded/broken 之外的值）。
    UnknownNodeHealth(String),
    /// RuntimeNode 字段非法（node_id 空/超长、可空字段带空串等）。
    InvalidRuntimeNode {
        /// 被拒绝的字段名。
        field: String,
        /// 拒绝原因。
        reason: String,
    },
    /// RunId 非法（TOOL-CONTRACT §4.1：非空且不超过 128 字符）。
    InvalidRunId {
        /// 被拒绝的原始值。
        value: String,
        /// 拒绝原因。
        reason: String,
    },
    /// ArtifactId 非法（TOOL-CONTRACT §5.2：非空）。
    InvalidArtifactId {
        /// 被拒绝的原始值。
        value: String,
        /// 拒绝原因。
        reason: String,
    },
    /// TraceId 非法（P7-5 关联面：非空且不超过 128 字符）。
    InvalidTraceId {
        /// 被拒绝的原始值。
        value: String,
        /// 拒绝原因。
        reason: String,
    },
    /// TaskId 非法（P7-5 关联面：非空且不超过 128 字符）。
    InvalidTaskId {
        /// 被拒绝的原始值。
        value: String,
        /// 拒绝原因。
        reason: String,
    },
    /// EvidenceId 非法（P7-5 关联面：非空且不超过 128 字符）。
    InvalidEvidenceId {
        /// 被拒绝的原始值。
        value: String,
        /// 拒绝原因。
        reason: String,
    },
    /// AgentId 非法（P7-1：非空且不超过 128 字符）。
    InvalidAgentId {
        /// 被拒绝的原始值。
        value: String,
        /// 拒绝原因。
        reason: String,
    },
    /// capability_refs 绑定指向不存在的工具（ROADMAP §8.3 P7-1：指向不存在
    /// 工具即拒绝；谓词由调用方注入，domain 保持零依赖）。
    UnknownCapabilityTarget {
        /// 绑定引用的工具 id。
        tool_id: String,
    },
    /// AgentDefinition 字段非法（版本/上下文策略等）。
    InvalidAgentDefinition {
        /// 被拒绝的字段名。
        field: String,
        /// 拒绝原因。
        reason: String,
    },
    /// AgentRun 状态迁移非法（P7-2 状态机补充：跃迁表之外的状态对一律拒绝，
    /// 含终态迁出与自迁移）。
    InvalidAgentTransition {
        /// 起始状态（存储值词形）。
        from: String,
        /// 目标状态（存储值词形）。
        to: String,
    },
    /// Agent Memory 字段非法（P7-3：id/hash 快照/载荷不变式/回链等校验拒绝）。
    InvalidAgentMemory {
        /// 被拒绝的字段名。
        field: String,
        /// 拒绝原因。
        reason: String,
    },
    /// Memory 生命周期出现未知变体（013 CHECK 词值之外的存储值）。
    UnknownMemoryLifecycle(
        /// 被拒绝的原始值。
        String,
    ),
    /// Memory 生命周期迁移非法（P7-3：按类别跃迁表拒绝；Evidence/Research
    /// 记忆的驱逐与归档出词值即在此拒绝）。
    InvalidMemoryTransition {
        /// Memory 类别（task/evidence/research）。
        kind: String,
        /// 起始状态（存储值词形）。
        from: String,
        /// 目标状态（存储值词形）。
        to: String,
    },
    /// Context Broker 词表出现未知变体（008 迁移 CHECK 域与 domain::context
    /// 枚举之外的值；覆盖 layer/kind/included/intent_status/event/target_slot/
    /// tool_family 七类词表，P4-1 收编）。
    UnknownContextVariant {
        /// 词表字段名（layer/kind/included/intent_status/event/target_slot/tool_family）。
        field: String,
        /// 被拒绝的原始值。
        value: String,
    },
    /// PluginId 非法（P6-2：小写点分 namespace.name，草案 D1，上限 128 字节）。
    InvalidPluginId(
        /// 被拒绝的原始值。
        String,
    ),
    /// PluginState 出现未知变体（PLUGIN-CONTRACT §5.1 十二态之外的值）。
    UnknownPluginState(
        /// 被拒绝的原始值。
        String,
    ),
    /// PluginKind 出现未知变体（§4.1 六类之外的值）。
    InvalidPluginKind(
        /// 被拒绝的原始值。
        String,
    ),
    /// CapabilityType 出现未知变体（§3.1 七种之外的值）。
    InvalidCapabilityType(
        /// 被拒绝的原始值。
        String,
    ),
    /// PluginTrust 出现未知变体（§6 四档之外的值）。
    InvalidPluginTrust(
        /// 被拒绝的原始值。
        String,
    ),
    /// WorkflowId 非法（P8-1：非空且不超过 128 字符）。
    InvalidWorkflowId {
        /// 被拒绝的原始值。
        value: String,
        /// 拒绝原因。
        reason: String,
    },
    /// Workflow 节点字段非法（节点 id 形状等）。
    InvalidWorkflowNode {
        /// 被拒绝的字段名。
        field: String,
        /// 拒绝原因。
        reason: String,
    },
    /// Workflow 边字段非法（端点 id 形状等）。
    InvalidWorkflowEdge {
        /// 被拒绝的字段名。
        field: String,
        /// 拒绝原因。
        reason: String,
    },
    /// WorkflowDefinition 字段非法（name 空、version 0、节点 id 重复等）。
    InvalidWorkflowDefinition {
        /// 拒绝原因。
        reason: String,
    },
    /// WorkflowRun 字段非法（workflow_version 0、节点状态重复等）。
    InvalidWorkflowRun(
        /// 拒绝原因。
        String,
    ),
    /// Workflow 节点类型出现未知变体（P8-1 七值词表之外）。
    UnknownWorkflowNodeKind(
        /// 被拒绝的原始值。
        String,
    ),
    /// Workflow 边类型出现未知变体（seq/branch 之外）。
    UnknownWorkflowEdgeKind(
        /// 被拒绝的原始值。
        String,
    ),
    /// Workflow 兼容面出现未知变体（016 CHECK 域两值之外）。
    UnknownWorkflowCompat(
        /// 被拒绝的原始值。
        String,
    ),
    /// WorkflowRun 状态出现未知变体（P8-1 六值词表之外）。
    UnknownWorkflowRunState(
        /// 被拒绝的原始值。
        String,
    ),
    /// Workflow 节点运行状态出现未知变体（五值词表之外）。
    UnknownWorkflowNodeRunState(
        /// 被拒绝的原始值。
        String,
    ),
    /// DocumentId 非法（P1-2，03-DOMAIN-MODEL：非空且不超过 128 字符）。
    InvalidDocumentId {
        /// 被拒绝的原始值。
        value: String,
        /// 拒绝原因。
        reason: String,
    },
    /// DocumentRevisionId 非法（同上口径）。
    InvalidDocumentRevisionId {
        /// 被拒绝的原始值。
        value: String,
        /// 拒绝原因。
        reason: String,
    },
    /// DocumentBlock 的 block id 非法（同上口径）。
    InvalidBlockId {
        /// 被拒绝的原始值。
        value: String,
        /// 拒绝原因。
        reason: String,
    },
    /// SourceRecord 的 source id 非法（同上口径）。
    InvalidSourceRecordId {
        /// 被拒绝的原始值。
        value: String,
        /// 拒绝原因。
        reason: String,
    },
    /// 文档可见性词出现未知变体（private/shared/public 之外）。
    InvalidDocumentVisibility {
        /// 被拒绝的原始值。
        value: String,
    },
}

impl fmt::Display for DomainError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DomainError::InvalidResourceId { value, reason } => {
                write!(f, "invalid resource id {value:?}: {reason}")
            }
            DomainError::UnknownLifecycleVariant(v) => {
                write!(f, "unknown lifecycle variant: {v:?}")
            }
            DomainError::UnknownGovernanceVariant(v) => {
                write!(f, "unknown governance variant: {v:?}")
            }
            DomainError::UnknownQualityGrade(v) => {
                write!(f, "unknown quality grade: {v:?}")
            }
            DomainError::InvalidAuditEvent(reason) => {
                write!(f, "invalid audit event: {reason}")
            }
            DomainError::UnknownToolRunStatus(v) => {
                write!(f, "unknown tool run status: {v:?}")
            }
            DomainError::UnknownRuntimeNodeKind(v) => {
                write!(f, "unknown runtime node kind: {v:?}")
            }
            DomainError::UnknownNodeHealth(v) => {
                write!(f, "unknown node health: {v:?}")
            }
            DomainError::InvalidRuntimeNode { field, reason } => {
                write!(f, "invalid runtime node field {field:?}: {reason}")
            }
            DomainError::InvalidRunId { value, reason } => {
                write!(f, "invalid run id {value:?}: {reason}")
            }
            DomainError::InvalidArtifactId { value, reason } => {
                write!(f, "invalid artifact id {value:?}: {reason}")
            }
            DomainError::InvalidTraceId { value, reason } => {
                write!(f, "invalid trace id {value:?}: {reason}")
            }
            DomainError::InvalidTaskId { value, reason } => {
                write!(f, "invalid task id {value:?}: {reason}")
            }
            DomainError::InvalidEvidenceId { value, reason } => {
                write!(f, "invalid evidence id {value:?}: {reason}")
            }
            DomainError::InvalidAgentId { value, reason } => {
                write!(f, "invalid agent id {value:?}: {reason}")
            }
            DomainError::UnknownCapabilityTarget { tool_id } => {
                write!(f, "capability ref targets unknown tool {tool_id:?}")
            }
            DomainError::InvalidAgentDefinition { field, reason } => {
                write!(f, "invalid agent definition field {field:?}: {reason}")
            }
            DomainError::InvalidAgentTransition { from, to } => {
                write!(
                    f,
                    "illegal agent run transition {from} -> {to} (P7-2 transition table)"
                )
            }
            DomainError::InvalidAgentMemory { field, reason } => {
                write!(f, "invalid agent memory field {field:?}: {reason}")
            }
            DomainError::UnknownMemoryLifecycle(value) => {
                write!(
                    f,
                    "unknown memory lifecycle value {value:?} (013 CHECK domain)"
                )
            }
            DomainError::InvalidMemoryTransition { kind, from, to } => {
                write!(
                    f,
                    "illegal {kind} memory transition {from} -> {to} (P7-3 transition table)"
                )
            }
            DomainError::UnknownContextVariant { field, value } => {
                write!(f, "unknown context variant in {field:?}: {value:?}")
            }
            DomainError::InvalidPluginId(value) => {
                write!(
                    f,
                    "invalid plugin id {value:?} (dot-separated lowercase namespace.name, D1)"
                )
            }
            DomainError::UnknownPluginState(v) => {
                write!(f, "unknown plugin state: {v:?}")
            }
            DomainError::InvalidPluginKind(v) => {
                write!(f, "invalid plugin kind: {v:?}")
            }
            DomainError::InvalidCapabilityType(v) => {
                write!(f, "invalid capability type: {v:?}")
            }
            DomainError::InvalidPluginTrust(v) => {
                write!(f, "invalid plugin trust level: {v:?}")
            }
            DomainError::InvalidWorkflowId { value, reason } => {
                write!(f, "invalid workflow id {value:?}: {reason}")
            }
            DomainError::InvalidWorkflowNode { field, reason } => {
                write!(f, "invalid workflow node field {field:?}: {reason}")
            }
            DomainError::InvalidWorkflowEdge { field, reason } => {
                write!(f, "invalid workflow edge field {field:?}: {reason}")
            }
            DomainError::InvalidWorkflowDefinition { reason } => {
                write!(f, "invalid workflow definition: {reason}")
            }
            DomainError::InvalidWorkflowRun(reason) => {
                write!(f, "invalid workflow run: {reason}")
            }
            DomainError::UnknownWorkflowNodeKind(v) => {
                write!(f, "unknown workflow node kind: {v:?}")
            }
            DomainError::UnknownWorkflowEdgeKind(v) => {
                write!(f, "unknown workflow edge kind: {v:?}")
            }
            DomainError::UnknownWorkflowCompat(v) => {
                write!(f, "unknown workflow compat marker: {v:?}")
            }
            DomainError::UnknownWorkflowRunState(v) => {
                write!(f, "unknown workflow run state: {v:?}")
            }
            DomainError::UnknownWorkflowNodeRunState(v) => {
                write!(f, "unknown workflow node run state: {v:?}")
            }
            DomainError::InvalidDocumentId { value, reason } => {
                write!(f, "invalid document id {value:?}: {reason}")
            }
            DomainError::InvalidDocumentRevisionId { value, reason } => {
                write!(f, "invalid document revision id {value:?}: {reason}")
            }
            DomainError::InvalidBlockId { value, reason } => {
                write!(f, "invalid document block id {value:?}: {reason}")
            }
            DomainError::InvalidSourceRecordId { value, reason } => {
                write!(f, "invalid source record id {value:?}: {reason}")
            }
            DomainError::InvalidDocumentVisibility { value } => {
                write!(f, "unknown document visibility: {value:?}")
            }
        }
    }
}

impl std::error::Error for DomainError {}
