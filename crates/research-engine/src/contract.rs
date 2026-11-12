//! 研究能力契约面（P7-6，SESSION-HANDOFF §十追加项 2）。
//!
//! 职责：
//! - [`CapabilityId`]：七能力词表（planner / source-discovery / evidence-extraction /
//!   verification / synthesis / citation / evaluator）与 14 份契约 Schema 的装载
//!   （`schemas/research-capability/` 双份惯例，编译期 `include_str!` 内嵌，运行期
//!   无路径依赖）；
//! - 信封类型：[`CapabilityRequest`] / [`CapabilityResult`]（沿用
//!   schemas/tool-contract request-v1/result-v1 的信封惯例：protocol_version 恒 1、
//!   条件必填由 status 驱动、provenance 六 ID 锚数组恒在）；
//! - [`Correlation`]：六 ID 关联锚（域层 [`domain::ids`] 类型校验后入信封）；
//! - [`execute_request`]：统一派发入口（typed 入参解析 → capabilities 模块纯函数
//!   执行 → 信封回包）；入参载荷非法映射为 FAILED 信封（category=VALIDATION，对齐
//!   TOOL-CONTRACT 5.3「参数校验失败落 FAILED 结果信封」），信封头（协议版本/关联锚）
//!   非法直接返回 [`ContractError`]（调用侧契约违规，不产信封）；
//! - 确定性摘要 [`digest16`] / [`digest_hex`]（sha256，ID 派生共用）。
//!
//! 依赖纪律：本 crate 不依赖 agent-engine（单向弱耦合：Phase 8 的 agent_loop 接线
//! 是 agent-engine 侧引用本 crate，反方向禁止）；零迁移（契约面纯 Schema 加代码）。

use crate::capabilities::{
    citation::CitationCapability, discovery::DiscoveryCapability, extraction::ExtractionCapability,
    evaluator::EvaluatorCapability, planner::PlannerCapability, synthesis::SynthesisCapability,
    verification::VerificationCapability, CapabilityExec,
};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;

/// 契约协议版本（信封 protocol_version，恒 1）。
pub const PROTOCOL_VERSION: &str = "1";

/// 七能力契约版本（P7-6 冻结为 0.1.0，升级需同步 14 份 Schema 与本常量）。
pub const CAPABILITY_VERSION: &str = "0.1.0";

/// 内嵌 Schema 相对仓库根的目录（编译期路径，仅文档说明用）。
pub const SCHEMA_DIR: &str = "schemas/research-capability";

// ---------------------------------------------------------------------------
// 14 份契约 Schema 内嵌（编译期快照；磁盘文件为源，本表随文件同步）
// ---------------------------------------------------------------------------

const PLANNER_REQUEST: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../schemas/research-capability/planner.request-v1.json"
));
const PLANNER_RESULT: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../schemas/research-capability/planner.result-v1.json"
));
const DISCOVERY_REQUEST: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../schemas/research-capability/discovery.request-v1.json"
));
const DISCOVERY_RESULT: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../schemas/research-capability/discovery.result-v1.json"
));
const EXTRACTION_REQUEST: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../schemas/research-capability/evidence-extraction.request-v1.json"
));
const EXTRACTION_RESULT: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../schemas/research-capability/evidence-extraction.result-v1.json"
));
const VERIFICATION_REQUEST: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../schemas/research-capability/verification.request-v1.json"
));
const VERIFICATION_RESULT: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../schemas/research-capability/verification.result-v1.json"
));
const SYNTHESIS_REQUEST: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../schemas/research-capability/synthesis.request-v1.json"
));
const SYNTHESIS_RESULT: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../schemas/research-capability/synthesis.result-v1.json"
));
const CITATION_REQUEST: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../schemas/research-capability/citation.request-v1.json"
));
const CITATION_RESULT: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../schemas/research-capability/citation.result-v1.json"
));
const EVALUATOR_REQUEST: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../schemas/research-capability/evaluator.request-v1.json"
));
const EVALUATOR_RESULT: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../schemas/research-capability/evaluator.result-v1.json"
));

/// Schema 面（请求或结果）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SchemaKind {
    /// 请求契约（request-v1）。
    Request,
    /// 结果契约（result-v1）。
    Result,
}

/// 七能力词表（capability_id 段内字符集与 tool-contract tool_id 同口径）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum CapabilityId {
    /// 研究计划（Research Planner）。
    Planner,
    /// 源发现（Source Discovery）。
    SourceDiscovery,
    /// 证据抽取（Evidence Extraction）。
    EvidenceExtraction,
    /// 核验（Verification）。
    Verification,
    /// 综合（Synthesis）。
    Synthesis,
    /// 引文（Citation）。
    Citation,
    /// 评估（Evaluator）。
    Evaluator,
}

impl CapabilityId {
    /// 七能力全集（词表顺序即 list 输出与测试遍历顺序）。
    pub const ALL: [CapabilityId; 7] = [
        CapabilityId::Planner,
        CapabilityId::SourceDiscovery,
        CapabilityId::EvidenceExtraction,
        CapabilityId::Verification,
        CapabilityId::Synthesis,
        CapabilityId::Citation,
        CapabilityId::Evaluator,
    ];

    /// 能力标识字符串（Schema capability_id const 同源）。
    pub fn as_str(&self) -> &'static str {
        match self {
            CapabilityId::Planner => "research.planner",
            CapabilityId::SourceDiscovery => "research.source-discovery",
            CapabilityId::EvidenceExtraction => "research.evidence-extraction",
            CapabilityId::Verification => "research.verification",
            CapabilityId::Synthesis => "research.synthesis",
            CapabilityId::Citation => "research.citation",
            CapabilityId::Evaluator => "research.evaluator",
        }
    }

    /// 一句话用途（rp-research list 输出与文档登记共用）。
    pub fn purpose(&self) -> &'static str {
        match self {
            CapabilityId::Planner => "把研究目标分解为固定流水线计划（确定性 mock）",
            CapabilityId::SourceDiscovery => "定位可用知识源与资源锚（确定性 mock）",
            CapabilityId::EvidenceExtraction => "把证据引用固化为可核验抽取条目（确定性 mock）",
            CapabilityId::Verification => "按策略对断言给出核验判定（确定性 mock）",
            CapabilityId::Synthesis => "把证据素材组装为主题化段落（确定性 mock）",
            CapabilityId::Citation => "为综合成果生成可回链引文（确定性 mock）",
            CapabilityId::Evaluator => "对成果跑检查项评分（确定性 mock）",
        }
    }

    /// 请求 Schema 文件名（仓库根相对，list 输出用）。
    pub fn request_schema_file(&self) -> &'static str {
        match self {
            CapabilityId::Planner => "planner.request-v1.json",
            CapabilityId::SourceDiscovery => "discovery.request-v1.json",
            CapabilityId::EvidenceExtraction => "evidence-extraction.request-v1.json",
            CapabilityId::Verification => "verification.request-v1.json",
            CapabilityId::Synthesis => "synthesis.request-v1.json",
            CapabilityId::Citation => "citation.request-v1.json",
            CapabilityId::Evaluator => "evaluator.request-v1.json",
        }
    }

    /// 结果 Schema 文件名（仓库根相对，list 输出用）。
    pub fn result_schema_file(&self) -> &'static str {
        match self {
            CapabilityId::Planner => "planner.result-v1.json",
            CapabilityId::SourceDiscovery => "discovery.result-v1.json",
            CapabilityId::EvidenceExtraction => "evidence-extraction.result-v1.json",
            CapabilityId::Verification => "verification.result-v1.json",
            CapabilityId::Synthesis => "synthesis.result-v1.json",
            CapabilityId::Citation => "citation.result-v1.json",
            CapabilityId::Evaluator => "evaluator.result-v1.json",
        }
    }

    /// 内嵌请求 Schema 原文。
    pub fn request_schema_str(&self) -> &'static str {
        match self {
            CapabilityId::Planner => PLANNER_REQUEST,
            CapabilityId::SourceDiscovery => DISCOVERY_REQUEST,
            CapabilityId::EvidenceExtraction => EXTRACTION_REQUEST,
            CapabilityId::Verification => VERIFICATION_REQUEST,
            CapabilityId::Synthesis => SYNTHESIS_REQUEST,
            CapabilityId::Citation => CITATION_REQUEST,
            CapabilityId::Evaluator => EVALUATOR_REQUEST,
        }
    }

    /// 内嵌结果 Schema 原文。
    pub fn result_schema_str(&self) -> &'static str {
        match self {
            CapabilityId::Planner => PLANNER_RESULT,
            CapabilityId::SourceDiscovery => DISCOVERY_RESULT,
            CapabilityId::EvidenceExtraction => EXTRACTION_RESULT,
            CapabilityId::Verification => VERIFICATION_RESULT,
            CapabilityId::Synthesis => SYNTHESIS_RESULT,
            CapabilityId::Citation => CITATION_RESULT,
            CapabilityId::Evaluator => EVALUATOR_RESULT,
        }
    }

    /// 内嵌 Schema 原文（按面取）。
    pub fn schema_str(&self, kind: SchemaKind) -> &'static str {
        match kind {
            SchemaKind::Request => self.request_schema_str(),
            SchemaKind::Result => self.result_schema_str(),
        }
    }
}

impl fmt::Display for CapabilityId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl Serialize for CapabilityId {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for CapabilityId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        CapabilityId::try_from(raw.as_str()).map_err(serde::de::Error::custom)
    }
}

impl TryFrom<&str> for CapabilityId {
    type Error = String;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "research.planner" => Ok(CapabilityId::Planner),
            "research.source-discovery" => Ok(CapabilityId::SourceDiscovery),
            "research.evidence-extraction" => Ok(CapabilityId::EvidenceExtraction),
            "research.verification" => Ok(CapabilityId::Verification),
            "research.synthesis" => Ok(CapabilityId::Synthesis),
            "research.citation" => Ok(CapabilityId::Citation),
            "research.evaluator" => Ok(CapabilityId::Evaluator),
            other => Err(format!("unknown capability id: {other}")),
        }
    }
}

impl TryFrom<String> for CapabilityId {
    type Error = String;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        CapabilityId::try_from(value.as_str())
    }
}

// ---------------------------------------------------------------------------
// Schema 装载与校验（jsonschema 0.56，与 tool-registry jsonschema_lite 同用法）
// ---------------------------------------------------------------------------

/// 解析内嵌 Schema 文档为 JSON Value。
pub fn schema_document(
    capability: CapabilityId,
    kind: SchemaKind,
) -> Result<serde_json::Value, String> {
    serde_json::from_str(capability.schema_str(kind))
        .map_err(|e| format!("embedded schema parse failed: {e}"))
}

/// 用实例校验对应契约 Schema。Err 携带首个失败的说明。
pub fn validate_against(
    capability: CapabilityId,
    kind: SchemaKind,
    instance: &serde_json::Value,
) -> Result<(), String> {
    let schema = schema_document(capability, kind)?;
    let validator =
        jsonschema::validator_for(&schema).map_err(|e| format!("schema compile failed: {e}"))?;
    validator
        .validate(instance)
        .map_err(|e| format!("contract violation: {e}"))
}

/// 全量门：14 份内嵌 Schema 各按 draft 2020-12 官方 meta-schema 校验文档结构。
/// Err 携带首份不合格文档的定位与说明。
pub fn check_all_schema_documents() -> Result<(), String> {
    for capability in CapabilityId::ALL {
        for kind in [SchemaKind::Request, SchemaKind::Result] {
            let doc = schema_document(capability, kind)?;
            let meta = jsonschema::meta::validator_for(&doc)
                .map_err(|e| format!("not a valid JSON Schema document: {e}"))?;
            meta.validate(&doc).map_err(|e| {
                format!(
                    "{} {} not a valid JSON Schema document: {e}",
                    capability.as_str(),
                    match kind {
                        SchemaKind::Request => "request-v1",
                        SchemaKind::Result => "result-v1",
                    }
                )
            })?;
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// 六 ID 关联锚与 provenance 锚
// ---------------------------------------------------------------------------

/// 六 ID 关联锚（请求信封 correlation 字段）。
///
/// 域层校验在构造时完成（[`domain::ids::RunId`] / [`domain::ids::TraceId`] /
/// [`domain::ids::TaskId`]），非法 ID 在反序列化期即拒绝。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Correlation {
    /// 关联运行 ID（agent_run/tool_run 共享 RunId 冻结形状）。
    pub run_id: domain::ids::RunId,
    /// 关联链路追踪 ID（迁移 015 trace_id 列口径）。
    pub trace_id: domain::ids::TraceId,
    /// 关联研究任务 ID（可缺省）。
    pub task_id: Option<domain::ids::TaskId>,
}

impl Correlation {
    /// 域层校验并构造。
    pub fn try_new(
        run_id: impl Into<String>,
        trace_id: impl Into<String>,
        task_id: Option<String>,
    ) -> Result<Self, String> {
        let run_id = domain::ids::RunId::try_new(run_id).map_err(|e| e.to_string())?;
        let trace_id = domain::ids::TraceId::try_new(trace_id).map_err(|e| e.to_string())?;
        let task_id = match task_id {
            Some(raw) if raw.is_empty() => None,
            Some(raw) => Some(domain::ids::TaskId::try_new(raw).map_err(|e| e.to_string())?),
            None => None,
        };
        Ok(Self {
            run_id,
            trace_id,
            task_id,
        })
    }

    /// correlation 派生的 provenance 锚（run 必有，task 缺省则无）。
    pub fn anchors(&self) -> Vec<ProvenanceAnchor> {
        let mut anchors = vec![
            ProvenanceAnchor::run(&self.run_id),
            ProvenanceAnchor::trace(&self.trace_id),
        ];
        if let Some(task) = &self.task_id {
            anchors.push(ProvenanceAnchor::task(task));
        }
        anchors
    }
}

impl Serialize for Correlation {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let len = if self.task_id.is_some() { 3 } else { 2 };
        let mut state = serializer.serialize_struct("Correlation", len)?;
        state.serialize_field("run_id", self.run_id.as_str())?;
        state.serialize_field("trace_id", self.trace_id.as_str())?;
        if let Some(task) = &self.task_id {
            state.serialize_field("task_id", task.as_str())?;
        }
        state.end()
    }
}

impl<'de> Deserialize<'de> for Correlation {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Raw {
            run_id: String,
            trace_id: String,
            #[serde(default)]
            task_id: Option<String>,
        }
        let raw = Raw::deserialize(deserializer)?;
        Correlation::try_new(raw.run_id, raw.trace_id, raw.task_id)
            .map_err(serde::de::Error::custom)
    }
}

/// 锚的 ID 族（六 ID 词表）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AnchorKind {
    /// 稳定资源 ID（gh:/url: 前缀）。
    Resource,
    /// 运行 ID（agent_run/tool_run 别名族）。
    Run,
    /// 链路追踪 ID。
    Trace,
    /// 研究任务 ID。
    Task,
    /// 产物 ID。
    Artifact,
    /// 证据 ID。
    Evidence,
}

/// provenance/citation 锚：六 ID 之一的字符串形式加族标注。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProvenanceAnchor {
    /// ID 族。
    pub kind: AnchorKind,
    /// ID 字符串形式（域层各类型的 Display 输出）。
    pub id: String,
}

impl ProvenanceAnchor {
    /// 从 [`domain::ids::ResourceId`] 构造资源锚。
    pub fn resource(id: &domain::ids::ResourceId) -> Self {
        Self {
            kind: AnchorKind::Resource,
            id: id.to_string(),
        }
    }

    /// 从 [`domain::ids::RunId`] 构造运行锚。
    pub fn run(id: &domain::ids::RunId) -> Self {
        Self {
            kind: AnchorKind::Run,
            id: id.to_string(),
        }
    }

    /// 从 [`domain::ids::TraceId`] 构造链路锚。
    pub fn trace(id: &domain::ids::TraceId) -> Self {
        Self {
            kind: AnchorKind::Trace,
            id: id.to_string(),
        }
    }

    /// 从 [`domain::ids::TaskId`] 构造任务锚。
    pub fn task(id: &domain::ids::TaskId) -> Self {
        Self {
            kind: AnchorKind::Task,
            id: id.to_string(),
        }
    }

    /// 从 [`domain::ids::ArtifactId`] 构造产物锚。
    pub fn artifact(id: &domain::ids::ArtifactId) -> Self {
        Self {
            kind: AnchorKind::Artifact,
            id: id.to_string(),
        }
    }

    /// 从 [`domain::ids::EvidenceId`] 构造证据锚。
    pub fn evidence(id: &domain::ids::EvidenceId) -> Self {
        Self {
            kind: AnchorKind::Evidence,
            id: id.to_string(),
        }
    }
}

// ---------------------------------------------------------------------------
// 信封类型
// ---------------------------------------------------------------------------

/// 能力终态（mock 纯函数只会产出 SUCCEEDED 或 FAILED）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum CapabilityStatus {
    /// 执行成功（result 必填）。
    Succeeded,
    /// 执行失败（error 必填；mock 只会因入参被拒而失败）。
    Failed,
}

/// 能力错误类别（TOOL-CONTRACT 5.3 词表子集）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum ErrorCategory {
    /// 入参在派发前被拒（mock 唯一产出类别）。
    Validation,
    /// 执行期失败（Phase 8 真实实现预留，词表先行冻结）。
    Execution,
}

/// 结果信封错误对象（result-v1 $defs/error 同形）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CapabilityError {
    /// 错误类别。
    pub category: ErrorCategory,
    /// 稳定错误码（CAP- 前缀）。
    pub code: String,
    /// 人类可读说明。
    pub message: String,
    /// 可重试性（mock 的 VALIDATION 恒 false）。
    pub retryable: bool,
}

/// 能力执行失败（内部分析态，转信封前用）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapabilityFailure {
    /// 错误类别。
    pub category: ErrorCategory,
    /// 稳定错误码（CAP- 前缀）。
    pub code: String,
    /// 人类可读说明。
    pub message: String,
    /// 可重试性。
    pub retryable: bool,
}

impl CapabilityFailure {
    /// 入参校验失败（不可重试）。
    pub fn validation(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            category: ErrorCategory::Validation,
            code: code.into(),
            message: message.into(),
            retryable: false,
        }
    }
}

/// 能力请求信封（request-v1，I 为能力专属输入载荷）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CapabilityRequest<I> {
    /// 协议版本（恒 "1"）。
    pub protocol_version: String,
    /// 能力标识。
    pub capability_id: CapabilityId,
    /// 能力契约版本。
    pub capability_version: String,
    /// 六 ID 关联锚。
    pub correlation: Correlation,
    /// 能力专属载荷。
    pub input: I,
}

/// 能力结果信封（result-v1，O 为能力专属输出载荷）。
///
/// 条件必填由 status 驱动（Schema allOf 同口径）：SUCCEEDED 时 result 必填，
/// FAILED 时 error 必填；序列化侧以 skip_serializing_if 保证缺省面一致。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CapabilityResult<O> {
    /// 协议版本（恒 "1"）。
    pub protocol_version: String,
    /// 能力标识。
    pub capability_id: CapabilityId,
    /// 能力契约版本。
    pub capability_version: String,
    /// 终态。
    pub status: CapabilityStatus,
    /// 能力专属输出（仅 SUCCEEDED）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<O>,
    /// 人类可读一段话摘要。
    pub summary: String,
    /// 错误对象（仅 FAILED）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<CapabilityError>,
    /// 六 ID provenance 锚数组（至少含 correlation 的 run/trace 锚）。
    pub provenance: Vec<ProvenanceAnchor>,
}

/// 成功信封装配：correlation 锚加 payload 锚，result 恒 Some。
pub fn succeeded_envelope<O: Serialize>(
    capability: CapabilityId,
    correlation: &Correlation,
    payload: O,
    summary: String,
    payload_anchors: Vec<ProvenanceAnchor>,
) -> CapabilityResult<O> {
    let mut provenance = correlation.anchors();
    provenance.extend(payload_anchors);
    CapabilityResult {
        protocol_version: PROTOCOL_VERSION.to_string(),
        capability_id: capability,
        capability_version: CAPABILITY_VERSION.to_string(),
        status: CapabilityStatus::Succeeded,
        result: Some(payload),
        summary,
        error: None,
        provenance,
    }
}

/// 失败信封装配：correlation 锚保留（失败也可追溯），result 恒 None。
pub fn failed_envelope<O: Serialize>(
    capability: CapabilityId,
    correlation: &Correlation,
    failure: CapabilityFailure,
) -> CapabilityResult<O> {
    CapabilityResult {
        protocol_version: PROTOCOL_VERSION.to_string(),
        capability_id: capability,
        capability_version: CAPABILITY_VERSION.to_string(),
        status: CapabilityStatus::Failed,
        result: None,
        summary: format!(
            "执行被拒：{}（{}，mock 不重试）",
            failure.message, failure.code
        ),
        error: Some(CapabilityError {
            category: failure.category,
            code: failure.code,
            message: failure.message,
            retryable: failure.retryable,
        }),
        provenance: correlation.anchors(),
    }
}

// ---------------------------------------------------------------------------
// 统一派发入口
// ---------------------------------------------------------------------------

/// 信封头校验失败（调用侧契约违规，不产结果信封）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContractError {
    /// 请求缺信封必填字段。
    MissingField(&'static str),
    /// protocol_version 非 "1"。
    InvalidProtocolVersion(String),
    /// capability_version 非 semver 形状。
    InvalidCapabilityVersion(String),
    /// correlation 非法（域层 ID 校验未过或结构不符）。
    InvalidCorrelation(String),
    /// input 非对象。
    InputNotObject,
}

impl fmt::Display for ContractError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ContractError::MissingField(field) => write!(f, "missing request field: {field}"),
            ContractError::InvalidProtocolVersion(v) => {
                write!(f, "protocol_version must be \"1\", got: {v}")
            }
            ContractError::InvalidCapabilityVersion(v) => {
                write!(f, "capability_version must be semver, got: {v}")
            }
            ContractError::InvalidCorrelation(reason) => {
                write!(f, "invalid correlation: {reason}")
            }
            ContractError::InputNotObject => write!(f, "input must be an object"),
        }
    }
}

impl std::error::Error for ContractError {}

fn is_semverish(value: &str) -> bool {
    let parts: Vec<&str> = value.split('.').collect();
    parts.len() == 3
        && parts
            .iter()
            .all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()))
}

/// 统一派发入口：信封头校验（协议版本/契约版本/关联锚）→ typed 入参解析 →
/// 能力纯函数执行 → 结果信封回包（serde_json::Value）。
///
/// 入参载荷非法映射为 FAILED 信封（VALIDATION，对齐 TOOL-CONTRACT 5.3）；
/// 信封头非法返回 [`ContractError`]。
pub fn execute_request(
    capability: CapabilityId,
    request: &serde_json::Value,
) -> Result<serde_json::Value, ContractError> {
    let protocol_version = request
        .get("protocol_version")
        .and_then(|v| v.as_str())
        .ok_or(ContractError::MissingField("protocol_version"))?;
    if protocol_version != PROTOCOL_VERSION {
        return Err(ContractError::InvalidProtocolVersion(
            protocol_version.to_string(),
        ));
    }
    let capability_version = request
        .get("capability_version")
        .and_then(|v| v.as_str())
        .ok_or(ContractError::MissingField("capability_version"))?;
    if !is_semverish(capability_version) {
        return Err(ContractError::InvalidCapabilityVersion(
            capability_version.to_string(),
        ));
    }
    let correlation: Correlation = serde_json::from_value(
        request
            .get("correlation")
            .cloned()
            .ok_or(ContractError::MissingField("correlation"))?,
    )
    .map_err(|e| ContractError::InvalidCorrelation(e.to_string()))?;
    let input = request
        .get("input")
        .cloned()
        .ok_or(ContractError::MissingField("input"))?;
    if !input.is_object() {
        return Err(ContractError::InputNotObject);
    }

    let envelope: serde_json::Value = match capability {
        CapabilityId::Planner => dispatch::<PlannerCapability>(capability, &correlation, &input),
        CapabilityId::SourceDiscovery => {
            dispatch::<DiscoveryCapability>(capability, &correlation, &input)
        }
        CapabilityId::EvidenceExtraction => {
            dispatch::<ExtractionCapability>(capability, &correlation, &input)
        }
        CapabilityId::Verification => {
            dispatch::<VerificationCapability>(capability, &correlation, &input)
        }
        CapabilityId::Synthesis => {
            dispatch::<SynthesisCapability>(capability, &correlation, &input)
        }
        CapabilityId::Citation => dispatch::<CitationCapability>(capability, &correlation, &input),
        CapabilityId::Evaluator => {
            dispatch::<EvaluatorCapability>(capability, &correlation, &input)
        }
    };
    Ok(envelope)
}

/// typed 分发胶水：入参解析失败即 FAILED 信封（VALIDATION），成功则装配成功信封。
fn dispatch<E: CapabilityExec>(
    capability: CapabilityId,
    correlation: &Correlation,
    input: &serde_json::Value,
) -> serde_json::Value {
    let outcome: Result<(E::Output, String, Vec<ProvenanceAnchor>), CapabilityFailure> =
        parse_input::<E::Input>(input).and_then(|typed| E::run(correlation, typed));
    let envelope = match outcome {
        Ok((payload, summary, anchors)) => {
            succeeded_envelope(capability, correlation, payload, summary, anchors)
        }
        Err(failure) => failed_envelope::<E::Output>(capability, correlation, failure),
    };
    serde_json::to_value(envelope).expect("envelope serialization cannot fail")
}

fn parse_input<I: DeserializeOwned>(input: &serde_json::Value) -> Result<I, CapabilityFailure> {
    serde_json::from_value(input.clone()).map_err(|e| {
        CapabilityFailure::validation(
            "CAP-INPUT-INVALID",
            format!("input 载荷不符合契约: {e}"),
        )
    })
}

// ---------------------------------------------------------------------------
// 确定性摘要（ID 派生共用）
// ---------------------------------------------------------------------------

/// sha256 全长 hex（64 字符）。parts 以 NUL 分隔拼接，规避拼接歧义。
pub fn digest_hex(parts: &[&str]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    for part in parts {
        hasher.update(part.as_bytes());
        hasher.update([0u8]);
    }
    hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// sha256 前 16 位 hex（8 字节，plan/src/syn/cite 等 mock ID 派生）。
pub fn digest16(parts: &[&str]) -> String {
    digest_hex(parts).chars().take(16).collect()
}
