//! Plugin 与 PluginCapability 域实体（计划 §12 PluginCapability 实体行 692 行、
//! Plugin 实体行随 PLUGIN-CONTRACT §3/§5 收编；2.0 新实体唯一合法落点为本
//! crate，盘点 48 行）。
//!
//! 口径（PLUGIN-CONTRACT，docs/phase0/PLUGIN-CONTRACT-DRAFT.md）：
//! - [`PluginState`]：§5.1 十二态词表（主链七态 + 辅助五态，草案 D2），
//!   SCREAMING_SNAKE 词形与表原文一致；跃迁合法性校验归 plugin-runtime
//!   （P6-2，PLUGIN-CONTRACT §5.2 跃迁表），本 crate 只承载状态身份
//!   （对齐 lib.rs 状态机口径注记：tool_run 跃迁校验属 tool-engine 同构）。
//! - [`PluginKind`]：§4.1 六选一词表（计划 380 到 385 行）。
//! - [`CapabilityType`]：§3.1 七种能力类型（计划 692 行 PluginCapability）。
//! - [`PluginTrust`]：§6 信任级四档（计划 584 行）。
//! - UPDATE_PENDING 是叠加标记而非独占态（§5.1 原文），在 [`Plugin`] 上以
//!   `update_pending` 布尔承载，不占状态位。

use crate::error::DomainError;

/// 插件身份新类型：小写点分 namespace.name（草案 D1），字符集与
/// tool-manifest D-TC-02 修订口径一致（段内 [a-z0-9_-]），上限 128 字节。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PluginId(String);

impl PluginId {
    /// 校验并构造。
    pub fn try_new(value: impl Into<String>) -> Result<Self, DomainError> {
        let value = value.into();
        if value.is_empty() || value.len() > 128 {
            return Err(DomainError::InvalidPluginId(value));
        }
        let segs: Vec<&str> = value.split('.').collect();
        let ok = segs.len() >= 2
            && segs.iter().all(|s| {
                !s.is_empty()
                    && s.starts_with(|c: char| c.is_ascii_lowercase())
                    && s.chars().all(|c| {
                        c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_'
                    })
            });
        if ok {
            Ok(Self(value))
        } else {
            Err(DomainError::InvalidPluginId(value))
        }
    }

    /// 内部值。
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// 消费出内部字符串。
    pub fn into_inner(self) -> String {
        self.0
    }
}

impl std::fmt::Display for PluginId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl AsRef<str> for PluginId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

/// 插件生命周期十二态（PLUGIN-CONTRACT §5.1；词形与表原文一致）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PluginState {
    /// 来源扫描发现，Manifest 元数据已解析进 registry 索引。
    Discovered,
    /// 静态校验（§4.2 七条）全部通过。
    Validated,
    /// 内容落盘 installed/，校验和与 lock 写入。
    Installed,
    /// capabilities 登记进平台数据面，可被搜索与发现，不可被任务匹配。
    Registered,
    /// 允许被 Context Broker 匹配与任务调用；运行资源未加载。
    Enabled,
    /// 插件运行资源已加载（worker 已连、adapter 已就绪）。
    Loaded,
    /// 至少一个 capability 正在服务调用。
    Active,
    /// 静态校验失败。
    Invalid,
    /// 用户禁用；安装内容与数据全保留；撤出匹配面。
    Disabled,
    /// 远端存在兼容新版本（叠加标记，非独占态，见 [`Plugin::update_pending`]）。
    UpdatePending,
    /// 健康检查失败或运行期故障。
    Error,
    /// 终态：内容删除，数据按 §5.4 保留规则处置。
    Uninstalled,
}

impl PluginState {
    /// 十二态词形全表（SCREAMING_SNAKE）。
    pub const ALL: [PluginState; 12] = [
        PluginState::Discovered,
        PluginState::Validated,
        PluginState::Installed,
        PluginState::Registered,
        PluginState::Enabled,
        PluginState::Loaded,
        PluginState::Active,
        PluginState::Invalid,
        PluginState::Disabled,
        PluginState::UpdatePending,
        PluginState::Error,
        PluginState::Uninstalled,
    ];

    /// 稳定词形（存储列值）。
    pub fn as_str(&self) -> &'static str {
        match self {
            PluginState::Discovered => "DISCOVERED",
            PluginState::Validated => "VALIDATED",
            PluginState::Installed => "INSTALLED",
            PluginState::Registered => "REGISTERED",
            PluginState::Enabled => "ENABLED",
            PluginState::Loaded => "LOADED",
            PluginState::Active => "ACTIVE",
            PluginState::Invalid => "INVALID",
            PluginState::Disabled => "DISABLED",
            PluginState::UpdatePending => "UPDATE_PENDING",
            PluginState::Error => "ERROR",
            PluginState::Uninstalled => "UNINSTALLED",
        }
    }

    /// 解析存储列值。
    pub fn parse(value: &str) -> Result<Self, DomainError> {
        PluginState::ALL
            .iter()
            .find(|s| s.as_str() == value)
            .copied()
            .ok_or_else(|| DomainError::UnknownPluginState(value.to_string()))
    }

    /// 是否终态（§5.1：UNINSTALLED 为唯一终态）。
    pub fn is_terminal(&self) -> bool {
        matches!(self, PluginState::Uninstalled)
    }
}

impl std::fmt::Display for PluginState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// 六类插件（PLUGIN-CONTRACT §3；计划 380 到 385 行）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PluginKind {
    /// Knowledge Plugin（知识源）。
    Knowledge,
    /// Tool Plugin（工具/工具包）。
    Tool,
    /// Project Plugin（外部完整项目接入点）。
    Project,
    /// Agent Plugin。
    Agent,
    /// Workflow Plugin。
    Workflow,
    /// Integration Plugin（连接器）。
    Integration,
}

impl PluginKind {
    /// 词形全表（小写，与 §4.1 kind 词表一致）。
    pub const ALL: [PluginKind; 6] = [
        PluginKind::Knowledge,
        PluginKind::Tool,
        PluginKind::Project,
        PluginKind::Agent,
        PluginKind::Workflow,
        PluginKind::Integration,
    ];

    /// 稳定词形。
    pub fn as_str(&self) -> &'static str {
        match self {
            PluginKind::Knowledge => "knowledge",
            PluginKind::Tool => "tool",
            PluginKind::Project => "project",
            PluginKind::Agent => "agent",
            PluginKind::Workflow => "workflow",
            PluginKind::Integration => "integration",
        }
    }

    /// 解析。
    pub fn parse(value: &str) -> Result<Self, DomainError> {
        PluginKind::ALL
            .iter()
            .find(|k| k.as_str() == value)
            .copied()
            .ok_or_else(|| DomainError::InvalidPluginKind(value.to_string()))
    }
}

/// 七种能力类型（PLUGIN-CONTRACT §3.1；计划 692 行 PluginCapability）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CapabilityType {
    /// 语料声明（Knowledge 插件）。
    KnowledgeSource,
    /// 单个 ToolDefinition。
    Tool,
    /// 一组 tool 的打包声明。
    ToolPack,
    /// 外部完整项目接入点。
    ProjectEndpoint,
    /// Agent 定义。
    AgentDefinition,
    /// Workflow 定义。
    WorkflowDefinition,
    /// 连接器描述。
    IntegrationConnector,
}

impl CapabilityType {
    /// 词形全表。
    pub const ALL: [CapabilityType; 7] = [
        CapabilityType::KnowledgeSource,
        CapabilityType::Tool,
        CapabilityType::ToolPack,
        CapabilityType::ProjectEndpoint,
        CapabilityType::AgentDefinition,
        CapabilityType::WorkflowDefinition,
        CapabilityType::IntegrationConnector,
    ];

    /// 稳定词形（snake_case，与 v1.json capability type 词表一致）。
    pub fn as_str(&self) -> &'static str {
        match self {
            CapabilityType::KnowledgeSource => "knowledge_source",
            CapabilityType::Tool => "tool",
            CapabilityType::ToolPack => "tool_pack",
            CapabilityType::ProjectEndpoint => "project_endpoint",
            CapabilityType::AgentDefinition => "agent_definition",
            CapabilityType::WorkflowDefinition => "workflow_definition",
            CapabilityType::IntegrationConnector => "integration_connector",
        }
    }

    /// 解析。
    pub fn parse(value: &str) -> Result<Self, DomainError> {
        CapabilityType::ALL
            .iter()
            .find(|t| t.as_str() == value)
            .copied()
            .ok_or_else(|| DomainError::InvalidCapabilityType(value.to_string()))
    }
}

/// 信任级四档（PLUGIN-CONTRACT §6；计划 584 行；草案 D10 仅 official 允许
/// update_check 自动提示）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PluginTrust {
    /// 官方来源。
    Official,
    /// 社区来源。
    Community,
    /// 本地来源。
    Local,
    /// 远程来源。
    Remote,
}

impl PluginTrust {
    /// 词形全表（小写）。
    pub const ALL: [PluginTrust; 4] = [
        PluginTrust::Official,
        PluginTrust::Community,
        PluginTrust::Local,
        PluginTrust::Remote,
    ];

    /// 稳定词形。
    pub fn as_str(&self) -> &'static str {
        match self {
            PluginTrust::Official => "official",
            PluginTrust::Community => "community",
            PluginTrust::Local => "local",
            PluginTrust::Remote => "remote",
        }
    }

    /// 解析。
    pub fn parse(value: &str) -> Result<Self, DomainError> {
        PluginTrust::ALL
            .iter()
            .find(|t| t.as_str() == value)
            .copied()
            .ok_or_else(|| DomainError::InvalidPluginTrust(value.to_string()))
    }
}

/// Plugin 实体（plugin 表行映射，纯数据；写路径由 plugin-runtime 唯一承担）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plugin {
    /// 插件身份。
    pub plugin_id: PluginId,
    /// 插件版本（X.Y.Z，版本化行，主键 (plugin_id, version)）。
    pub version: String,
    /// 展示名。
    pub name: String,
    /// 六类之一。
    pub kind: PluginKind,
    /// 一句话摘要。
    pub description: String,
    /// 许可证（SPDX 或 UNLICENSED）。
    pub license: String,
    /// 信任级。
    pub trust_level: PluginTrust,
    /// 生命周期状态。
    pub state: PluginState,
    /// UPDATE_PENDING 叠加标记（§5.1：非独占态，状态本身不变）。
    pub update_pending: bool,
    /// active 版本指针（§5.3 升级语义：新旧版本目录并存，指针切换）。
    pub is_active: bool,
    /// 来源类型（五类之一，v1.json source.type）。
    pub source_type: String,
    /// 来源定位（url / path / endpoint / base_url）。
    pub source_ref: String,
    /// 版本固定记录 JSON（{kind, value} 或 null）。
    pub pinned_json: Option<String>,
    /// plugin.lock 包内容指纹（checksums.sha256 文件 sha256）。
    pub lock_sha256: Option<String>,
    /// Manifest 原文（规范化 JSON）。
    pub manifest_json: String,
    /// 安装内容目录（installed/<id>/<version>/ 形态；P6-3 布局承载）。
    pub installed_dir: Option<String>,
    /// 已入主库知识处置策略（data_retention.ingested_policy，§5.4）。
    pub ingested_policy: Option<String>,
    /// 卸载后标记：已入主库知识的来源插件已卸载（§5.3 孤儿来源清单）。
    pub source_uninstalled: bool,
    /// 建行时间（ISO8601 UTC）。
    pub created_at: String,
    /// 更新时间（ISO8601 UTC）。
    pub updated_at: String,
}

/// PluginCapability 实体（plugin_capability 表行映射，纯数据）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginCapability {
    /// capability 身份（点分小写）。
    pub capability_id: String,
    /// 所属插件。
    pub plugin_id: PluginId,
    /// 所属插件版本。
    pub plugin_version: String,
    /// 七种能力类型之一。
    pub capability_type: CapabilityType,
    /// 包内相对入口路径。
    pub entry: String,
    /// 一句话摘要（L2 匹配级）。
    pub summary: String,
    /// runtime 类型声明（tool/tool_pack capability 才有）。
    pub runtime: Option<String>,
    /// provides 声明 JSON。
    pub provides_json: Option<String>,
    /// requires 引用 JSON 数组。
    pub requires_json: Option<String>,
}
