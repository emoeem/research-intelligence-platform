//! Context Broker 域类型（计划 §12:703 ContextManifest 实体行 + 设计稿
//! CONTEXT-BROKER-DESIGN §4.1 意图表示与 §6.1 词表列，P4-1 收编）。
//!
//! 边界约定（MODULE_BOUNDARIES §8.2.1，同 crate 头注）：纯 std，零依赖。
//! 本模块只承载身份与词表（008 迁移五类 CHECK 域的一一对应面 + intent_json
//! 快照内的槽位/工具族词表），装配、预算与校验逻辑属 crates/context-broker
//! （P4-1 起骨架，PHASE4-DESIGN §6 批次表）。
//!
//! 词表口径：layer/kind/included/intent_status/event 五类即
//! migrations/008_context_broker.sql 的 CHECK 域（设计稿 §6.1 列注释）；
//! target_slot 取设计稿 §4.1 槽位清单八值；tool_family 取计划 §4.2 工具族
//! 表十三族原文名（存储值即计划原文，解析精确匹配；触发词映射属 P4-3
//! intent 面，版本化配置而非硬编码，设计稿 §4.2 第 3 条）。

use crate::error::DomainError;
use std::fmt;

/// 四级上下文层级（计划 §8:437-443 层表；设计稿 §3）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ContextLayer {
    /// L0：RIP 全局能力说明与核心规则（会话启动装配，永不驱逐）。
    L0,
    /// L1：插件、工具、知识包目录元数据（发现能力时，只给目录不给正文）。
    L1,
    /// L2：匹配插件摘要、工具说明、知识目录（任务规划时，预算大头）。
    L2,
    /// L3：完整文档、代码、证据、工具 Schema（实际需要时按需展开记账）。
    L3,
}

impl ContextLayer {
    /// 全部变体（按 L0 到 L3 顺序）。
    pub const ALL: [ContextLayer; 4] = [
        ContextLayer::L0,
        ContextLayer::L1,
        ContextLayer::L2,
        ContextLayer::L3,
    ];

    /// 存储值（008 迁移 layer 列 CHECK 域）。
    pub fn as_str(&self) -> &'static str {
        match self {
            ContextLayer::L0 => "L0",
            ContextLayer::L1 => "L1",
            ContextLayer::L2 => "L2",
            ContextLayer::L3 => "L3",
        }
    }

    /// 从存储值解析（精确匹配 008 CHECK 域）。
    pub fn parse(value: &str) -> Result<Self, DomainError> {
        Self::ALL
            .into_iter()
            .find(|v| v.as_str() == value)
            .ok_or_else(|| DomainError::UnknownContextVariant {
                field: "layer".to_string(),
                value: value.to_string(),
            })
    }
}

/// 清单条目种类（设计稿 §6.1 kind 列注释七值）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ContextItemKind {
    /// 规则包条目（L0 规则包内的单条规则）。
    Rule,
    /// 工具（Registry 登记工具）。
    Tool,
    /// 知识条目（platform 知识面 / RTIS / ERI）。
    Knowledge,
    /// 插件（Phase 6 起有实体，此前空集）。
    Plugin,
    /// Artifact 引用（工具输出，只引用不内联，计划 §14:794 第三条）。
    Artifact,
    /// 工具 Schema（L3 按需展开面）。
    Schema,
    /// 文档正文（工具 context.docs 或知识正文）。
    Doc,
}

impl ContextItemKind {
    /// 全部变体（按设计稿 §6.1 注释顺序）。
    pub const ALL: [ContextItemKind; 7] = [
        ContextItemKind::Rule,
        ContextItemKind::Tool,
        ContextItemKind::Knowledge,
        ContextItemKind::Plugin,
        ContextItemKind::Artifact,
        ContextItemKind::Schema,
        ContextItemKind::Doc,
    ];

    /// 存储值（008 迁移 kind 列 CHECK 域）。
    pub fn as_str(&self) -> &'static str {
        match self {
            ContextItemKind::Rule => "rule",
            ContextItemKind::Tool => "tool",
            ContextItemKind::Knowledge => "knowledge",
            ContextItemKind::Plugin => "plugin",
            ContextItemKind::Artifact => "artifact",
            ContextItemKind::Schema => "schema",
            ContextItemKind::Doc => "doc",
        }
    }

    /// 从存储值解析（精确匹配 008 CHECK 域）。
    pub fn parse(value: &str) -> Result<Self, DomainError> {
        Self::ALL
            .into_iter()
            .find(|v| v.as_str() == value)
            .ok_or_else(|| DomainError::UnknownContextVariant {
                field: "kind".to_string(),
                value: value.to_string(),
            })
    }
}

/// 条目纳入形态（设计稿 §6.1 included 列注释四值；降级链只改形态不删行，
/// 设计稿 §5.3 第 6 条「L3 正文 > L2 摘要 > ref-only」）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InclusionForm {
    /// full：正文完整内联（L3 展开后）。
    Full,
    /// summary：摘要形态（L2 主形态）。
    Summary,
    /// ref-only：只留引用与哈希（降级终点，证据引用不丢）。
    RefOnly,
    /// catalog-card：目录卡片（L1 形态）。
    CatalogCard,
}

impl InclusionForm {
    /// 全部变体（按设计稿 §6.1 注释顺序）。
    pub const ALL: [InclusionForm; 4] = [
        InclusionForm::Full,
        InclusionForm::Summary,
        InclusionForm::RefOnly,
        InclusionForm::CatalogCard,
    ];

    /// 存储值（008 迁移 included 列 CHECK 域）。
    pub fn as_str(&self) -> &'static str {
        match self {
            InclusionForm::Full => "full",
            InclusionForm::Summary => "summary",
            InclusionForm::RefOnly => "ref-only",
            InclusionForm::CatalogCard => "catalog-card",
        }
    }

    /// 从存储值解析（精确匹配 008 CHECK 域）。
    pub fn parse(value: &str) -> Result<Self, DomainError> {
        Self::ALL
            .into_iter()
            .find(|v| v.as_str() == value)
            .ok_or_else(|| DomainError::UnknownContextVariant {
                field: "included".to_string(),
                value: value.to_string(),
            })
    }
}

/// 意图分类状态（设计稿 §4.2 第 4 条：低置信回退标记 ambiguous 附 top3
/// 候选，Broker 不静默猜；对应 008 intent_status 列 CHECK 域）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IntentStatus {
    /// 意图明确（confidence 达阈值，默认 0.5）。
    Ok,
    /// 意图含混（附 alternatives 等确认，重新装配后出新 manifest）。
    Ambiguous,
}

impl IntentStatus {
    /// 全部变体。
    pub const ALL: [IntentStatus; 2] = [IntentStatus::Ok, IntentStatus::Ambiguous];

    /// 存储值（008 迁移 intent_status 列 CHECK 域）。
    pub fn as_str(&self) -> &'static str {
        match self {
            IntentStatus::Ok => "ok",
            IntentStatus::Ambiguous => "ambiguous",
        }
    }

    /// 从存储值解析（精确匹配 008 CHECK 域）。
    pub fn parse(value: &str) -> Result<Self, DomainError> {
        Self::ALL
            .into_iter()
            .find(|v| v.as_str() == value)
            .ok_or_else(|| DomainError::UnknownContextVariant {
                field: "intent_status".to_string(),
                value: value.to_string(),
            })
    }
}

/// 引用事件类型（设计稿 §6.1 event 列注释五值；008 context_ref_log CHECK 域）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RefEvent {
    /// 装配签发（item 进入 Manifest 即记）。
    Assembled,
    /// L3 按需展开（expand 记账，从 l3_reserved 扣账）。
    Expanded,
    /// 超预算驱逐（正文副本被逐，引用与哈希保留）。
    Evicted,
    /// 被工具调用引用（调用对象 context_refs，计划:219）。
    CitedInToolCall,
    /// 被 Evidence 引用（provenance 闭环，设计稿 §6.2）。
    CitedInEvidence,
}

impl RefEvent {
    /// 全部变体（按设计稿 §6.1 注释顺序）。
    pub const ALL: [RefEvent; 5] = [
        RefEvent::Assembled,
        RefEvent::Expanded,
        RefEvent::Evicted,
        RefEvent::CitedInToolCall,
        RefEvent::CitedInEvidence,
    ];

    /// 存储值（008 迁移 event 列 CHECK 域，大写下划线风格）。
    pub fn as_str(&self) -> &'static str {
        match self {
            RefEvent::Assembled => "ASSEMBLED",
            RefEvent::Expanded => "EXPANDED",
            RefEvent::Evicted => "EVICTED",
            RefEvent::CitedInToolCall => "CITED_IN_TOOL_CALL",
            RefEvent::CitedInEvidence => "CITED_IN_EVIDENCE",
        }
    }

    /// 从存储值解析（精确匹配 008 CHECK 域）。
    pub fn parse(value: &str) -> Result<Self, DomainError> {
        Self::ALL
            .into_iter()
            .find(|v| v.as_str() == value)
            .ok_or_else(|| DomainError::UnknownContextVariant {
                field: "event".to_string(),
                value: value.to_string(),
            })
    }
}

/// 任务目标槽位（设计稿 §4.1 target_slots 取值八值）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TargetSlot {
    /// 域名。
    Domain,
    /// IP 地址。
    Ip,
    /// URL。
    Url,
    /// Android 安装包。
    Apk,
    /// 二进制样本。
    Binary,
    /// 文档样本。
    Document,
    /// 代码仓库。
    CodeRepo,
    /// 无目标（纯检索/评估类任务）。
    None,
}

impl TargetSlot {
    /// 全部变体（按设计稿 §4.1 清单顺序）。
    pub const ALL: [TargetSlot; 8] = [
        TargetSlot::Domain,
        TargetSlot::Ip,
        TargetSlot::Url,
        TargetSlot::Apk,
        TargetSlot::Binary,
        TargetSlot::Document,
        TargetSlot::CodeRepo,
        TargetSlot::None,
    ];

    /// 存储值（intent_json 快照内小写下划线风格，设计稿 §4.1 清单原文名）。
    pub fn as_str(&self) -> &'static str {
        match self {
            TargetSlot::Domain => "domain",
            TargetSlot::Ip => "ip",
            TargetSlot::Url => "url",
            TargetSlot::Apk => "apk",
            TargetSlot::Binary => "binary",
            TargetSlot::Document => "document",
            TargetSlot::CodeRepo => "code_repo",
            TargetSlot::None => "none",
        }
    }

    /// 从存储值解析（精确匹配清单原文名）。
    pub fn parse(value: &str) -> Result<Self, DomainError> {
        Self::ALL
            .into_iter()
            .find(|v| v.as_str() == value)
            .ok_or_else(|| DomainError::UnknownContextVariant {
                field: "target_slot".to_string(),
                value: value.to_string(),
            })
    }
}

/// 工具族（计划 §4.2 工具分类表十三族，计划:149-162；设计稿 §4.1 families
/// 取值即此表）。存储值 = 计划原文名（含空格与斜杠），解析精确匹配。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ToolFamily {
    /// Web / HTTP：HTTP 请求、响应头、重定向、Cookie、TLS、页面抓取。
    WebHttp,
    /// DNS / Domain：DNS 记录、子域信息、证书透明度数据整理、域名关系。
    DnsDomain,
    /// Network：端口与服务识别、协议探测、连通性诊断。
    Network,
    /// Web Analysis：页面结构、JS 资源、路由/API 线索、静态资源提取。
    WebAnalysis,
    /// Source Code：Git、代码搜索、依赖清单、调用关系、静态检查。
    SourceCode,
    /// Mobile / APK：APK 元数据、Manifest、权限、组件、资源、DEX 分析。
    MobileApk,
    /// Binary / Reverse：文件类型、哈希、字符串、符号、反汇编/反编译适配。
    BinaryReverse,
    /// Document：PDF、Office、Markdown、HTML、OCR、文档转换。
    Document,
    /// Data Processing：JSON/CSV/YAML/XML 解析、去重、规范化、格式转换。
    DataProcessing,
    /// Knowledge：索引、检索、实体关联、质量检查、来源核验。
    Knowledge,
    /// Evidence：截图、请求/响应归档、文件哈希、证据包导出。
    Evidence,
    /// Utility：文件操作、压缩解压、差异比较、编码、哈希。
    Utility,
    /// Browser Automation：页面导航、交互、截图、下载、DOM 提取。
    BrowserAutomation,
}

impl ToolFamily {
    /// 全部变体（按计划 §4.2 表行序，恰 13 族）。
    pub const ALL: [ToolFamily; 13] = [
        ToolFamily::WebHttp,
        ToolFamily::DnsDomain,
        ToolFamily::Network,
        ToolFamily::WebAnalysis,
        ToolFamily::SourceCode,
        ToolFamily::MobileApk,
        ToolFamily::BinaryReverse,
        ToolFamily::Document,
        ToolFamily::DataProcessing,
        ToolFamily::Knowledge,
        ToolFamily::Evidence,
        ToolFamily::Utility,
        ToolFamily::BrowserAutomation,
    ];

    /// 存储值（计划 §4.2 工具族原文名）。
    pub fn as_str(&self) -> &'static str {
        match self {
            ToolFamily::WebHttp => "Web / HTTP",
            ToolFamily::DnsDomain => "DNS / Domain",
            ToolFamily::Network => "Network",
            ToolFamily::WebAnalysis => "Web Analysis",
            ToolFamily::SourceCode => "Source Code",
            ToolFamily::MobileApk => "Mobile / APK",
            ToolFamily::BinaryReverse => "Binary / Reverse",
            ToolFamily::Document => "Document",
            ToolFamily::DataProcessing => "Data Processing",
            ToolFamily::Knowledge => "Knowledge",
            ToolFamily::Evidence => "Evidence",
            ToolFamily::Utility => "Utility",
            ToolFamily::BrowserAutomation => "Browser Automation",
        }
    }

    /// 从存储值解析（精确匹配计划原文名；触发词到族的映射属 P4-3 intent 面）。
    pub fn parse(value: &str) -> Result<Self, DomainError> {
        Self::ALL
            .into_iter()
            .find(|v| v.as_str() == value)
            .ok_or_else(|| DomainError::UnknownContextVariant {
                field: "tool_family".to_string(),
                value: value.to_string(),
            })
    }
}

impl fmt::Display for ContextLayer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl fmt::Display for ContextItemKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl fmt::Display for InclusionForm {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl fmt::Display for IntentStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl fmt::Display for RefEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl fmt::Display for TargetSlot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl fmt::Display for ToolFamily {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// 结构化任务意图（设计稿 §4.1 IntentQuery；确定性槽位，不依赖自然语言
/// 理解）。归一化与槽位抽取属 P4-3 intent 面，本类型只承载结果快照，
/// 装配时整体序列化进 context_manifest.intent_json。
#[derive(Debug, Clone, PartialEq)]
pub struct IntentQuery {
    /// 归一化文本（复用 resource-engine tokenizer 口径，设计稿 §4.2 第 1 条）。
    pub text_normalized: String,
    /// 候选工具族（计划 §4.2 十三族的子集）。
    pub families: Vec<ToolFamily>,
    /// 知识意图标签（检索/评估/来源核验/反知识核查等，设计稿 §4.1；
    /// 词表在 P4-3 intent 批随版本化配置定稿）。
    pub knowledge_intents: Vec<String>,
    /// 目标槽位（零或多，域/IP/URL/APK/二进制/文档/代码库/无）。
    pub target_slots: Vec<TargetSlot>,
    /// 任务动词（收集/解析/检索/评估/比对/监控等，设计稿 §4.1）。
    pub task_verbs: Vec<String>,
    /// 约束（只读、离线、超时上限等，设计稿 §4.1）。
    pub constraints: Vec<String>,
    /// 意图置信度（0.0 到 1.0；低于阈值即 ambiguous，设计稿 §4.2 第 4 条）。
    pub confidence: f32,
}

/// 权威内容引用（设计稿 §6.1 条目引用列 + §9.1 citations 元素；citations
/// 是下游唯一合法引用集，设计稿 §9.2 第 2 条）。source 词表
/// registry|platform-kb|rtis|eri|artifact-store|plugin:<id>：plugin:<id>
/// 为前缀形态，故用 String 承载（与 008 source 列同口径）。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ContentRef {
    /// 来源标识（词表见模块注；plugin:<id> 为前缀形态）。
    pub source: String,
    /// 来源对象 id（如 kb:900008 / rip.http.inspect / artifact id）。
    pub ref_id: String,
    /// 来源版本（缺省指 current）。
    pub ref_version: Option<String>,
    /// 内容 sha256 hex（来源追踪与版本追踪，计划 §12:703；目录卡片可缺）。
    pub content_hash: Option<String>,
    /// 来源 URI（设计稿 §9.1 citations uri，如 rtis://poc/900008）。
    pub uri: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn word_lists_round_trip_and_match_008_check_domains() {
        // 五类 DDL 词表与 008 CHECK 域逐值往返。
        for layer in ContextLayer::ALL {
            assert_eq!(ContextLayer::parse(layer.as_str()).unwrap(), layer);
        }
        for kind in ContextItemKind::ALL {
            assert_eq!(ContextItemKind::parse(kind.as_str()).unwrap(), kind);
        }
        for form in InclusionForm::ALL {
            assert_eq!(InclusionForm::parse(form.as_str()).unwrap(), form);
        }
        for status in IntentStatus::ALL {
            assert_eq!(IntentStatus::parse(status.as_str()).unwrap(), status);
        }
        for event in RefEvent::ALL {
            assert_eq!(RefEvent::parse(event.as_str()).unwrap(), event);
        }
        for slot in TargetSlot::ALL {
            assert_eq!(TargetSlot::parse(slot.as_str()).unwrap(), slot);
        }
        for family in ToolFamily::ALL {
            assert_eq!(ToolFamily::parse(family.as_str()).unwrap(), family);
        }
    }

    #[test]
    fn family_table_has_exactly_thirteen_plan_names() {
        assert_eq!(ToolFamily::ALL.len(), 13, "计划 §4.2 恰十三族");
        assert_eq!(ToolFamily::WebHttp.as_str(), "Web / HTTP");
        assert_eq!(ToolFamily::Knowledge.as_str(), "Knowledge");
        assert_eq!(ToolFamily::BrowserAutomation.as_str(), "Browser Automation");
    }

    #[test]
    fn parse_rejects_unknown_values_with_field_context() {
        let err = ContextLayer::parse("L4").unwrap_err();
        assert!(err.to_string().contains("layer"), "display: {err}");
        assert_eq!(
            RefEvent::parse("DELETED").unwrap_err(),
            DomainError::UnknownContextVariant {
                field: "event".to_string(),
                value: "DELETED".to_string(),
            }
        );
    }
}
