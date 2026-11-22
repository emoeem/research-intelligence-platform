//! Report IR v1 类型面与校验（PHASE-6A D1/D2/D4）。
//!
//! 三通道结构（intel-db/src/ir.rs 先例同构）：
//! 1. schema 单源落盘 `schemas/report-ir/report-ir-v1.schema.json` 经
//!    [`IR_V1_SCHEMA`] include_str! 编译期内嵌（draft 2020-12；五处词表枚举
//!    + ID 前缀/uri 词形 pattern，fail-closed）；
//! 2. [`tool_registry::jsonschema_lite::validate_instance`] 全语义校验
//!    （jsonschema 0.56，tool-registry 同校验器——零新校验器零新校验依赖）；
//! 3. Rust 侧兜底（[`validate_ir`]）：词表/前缀/词形显式复检，错误信息含
//!    字段名（判据 4 fail-closed 回显）；schema 无法表达的一致性约束
//!    （start_line ≤ end_line）在此兜底（intel-db ir.rs:10-11 先例）。
//!
//! IR 是内存模型零落库（D2：SQLite 035 不立项；PG reports 落库属 6b
//! V0008 候选）。rtis_ref 只承载 `rtis://poc/<id>` 引用词形（020:36
//! RTIS 红线：POC 正文/利用细节零复制）。

use domain::{ArtifactId, EvidenceId, TaskId, TraceId};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use tool_registry::jsonschema_lite;

/// IR v1 schema 文本（编译期内嵌；落盘单点 schemas/report-ir/report-ir-v1.schema.json）。
pub const IR_V1_SCHEMA: &str = include_str!("../../../schemas/report-ir/report-ir-v1.schema.json");

/// severity 词表（D1——RTIS 权威值镜像只读，03-DOMAIN-MODEL :140）。
pub const SEVERITIES: &[&str] = &["critical", "high", "medium", "low", "info"];

/// 五态研究结论词表（020 ResultGuard 同源，snake_case 归一）。
pub const RESULT_STATES: &[&str] = &[
    "confirmed",
    "false_positive",
    "not_reproduced",
    "not_tested",
    "inconclusive",
];

/// 报告类型词表（PLAN :517 §8.2 A/B/C 三类）。
pub const REPORT_KINDS: &[&str] = &["src_vuln_report", "pentest_report", "management_report"];

/// Evidence 类型词表（021 CHECK 同源）。
pub const EVIDENCE_TYPES: &[&str] = &["provider_result", "normalized_result", "log_excerpt"];

/// 修复优先级词表（D1 remediation.priority CHECK 面；词形四值为本批实施
/// 裁决——任务书未钉具体值，snake_case 与 severity 词形区分，登记批次报告）。
pub const REMEDIATION_PRIORITIES: &[&str] = &["immediate", "high", "medium", "low"];

/// 七维覆盖词表（022 dimension CHECK 直投影）。
pub const COVERAGE_DIMENSIONS: &[&str] = &[
    "assets",
    "api_endpoints",
    "identity_permissions",
    "input_points",
    "business_flows",
    "file_processing",
    "data_flows",
];

/// rtis_ref 词形前缀（A9 先例：rtis://poc/<id>——禁 provider.rs:219 `rtis-poc:` 旧词形）。
pub const RTIS_REF_PREFIX: &str = "rtis://poc/";

/// evidence uri 词形前缀（A9 先例：ripartifact:// 与 rtis:// 双形态）。
pub const EVIDENCE_URI_PREFIXES: &[&str] = &["ripartifact://", "rtis://"];

fn default_language() -> String {
    "zh".to_string()
}

/// Report IR v1 顶层（D1 全字段：12 内容字段 + findings/attack_paths/
/// appendix/properties 四容器位 = 16 顶层键）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportIr {
    pub schema_version: String,
    /// 报告稳定 ID（rep- 前缀，03-DOMAIN-MODEL :141 词形）。
    pub report_id: String,
    /// 报告类型（PLAN :517 A/B/C 词表）。
    pub report_kind: String,
    pub title: String,
    /// ISO8601 生成时间。
    pub generated_at: String,
    /// 报告语言，缺省 zh（:670 中文正确验收）。
    #[serde(default = "default_language")]
    pub language: String,
    /// 研究任务引用（ID 引用非跨库 FK——03-DOMAIN-MODEL :144）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_ref: Option<TaskRef>,
    /// 执行摘要（markdown 全文段）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub executive_summary: Option<String>,
    /// 授权范围与资产位（B 类 pentest_report 骨架投影）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<Scope>,
    /// 方法论（markdown 全文段）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub methodology: Option<String>,
    /// 关键时间线。
    #[serde(default)]
    pub timeline: Vec<TimelineItem>,
    /// 七维覆盖投影（022 词表同源）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub coverage: Option<Coverage>,
    /// Finding 集合（可空——空集边界判据 9）。
    pub findings: Vec<FindingIr>,
    /// 攻击路径（只承载引用——:531 图表事实以 RTIS 攻击链为准，Diagram Engine 属 6b）。
    #[serde(default)]
    pub attack_paths: Vec<AttackPath>,
    /// 附录（markdown 全文段）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub appendix: Option<String>,
    /// 自由扩展位（SARIF property bag 对齐位）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub properties: Option<Map<String, Value>>,
}

/// 研究任务引用（D1 task_ref{task_id?, workflow_run_id?}）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskRef {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workflow_run_id: Option<String>,
}

/// 授权范围与资产位（D1 scope{authorization?, assets[]}）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Scope {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub authorization: Option<String>,
    #[serde(default)]
    pub assets: Vec<String>,
}

/// 时间线条目（D1 timeline[{date, event}]）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineItem {
    /// ISO 日期（YYYY-MM-DD）。
    pub date: String,
    pub event: String,
}

/// 七维覆盖投影容器（D1 coverage{dimensions[]}）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Coverage {
    pub dimensions: Vec<CoverageDimension>,
}

/// 单维覆盖（D1 {dimension, total, tested, confirmed}——022 词表同源）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoverageDimension {
    pub dimension: String,
    pub total: u64,
    pub tested: u64,
    pub confirmed: u64,
}

/// 攻击路径（D1 {path_id, title, steps[], evidence_refs[]}——只承载引用）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttackPath {
    pub path_id: String,
    pub title: String,
    pub steps: Vec<String>,
    pub evidence_refs: Vec<String>,
}

/// Finding 投影（D1 FindingIr 18 字段——Finding 事实源在 RTIS，IR 只承载
/// 报告投影；severity 五态词表镜像只读）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FindingIr {
    /// Finding 稳定 ID（find- 前缀）。
    pub finding_id: String,
    /// RTIS POC 引用（rtis://poc/<id> 词形；可选——管理报告类可无 POC 对应；
    /// 细节不落 IR——03-DOMAIN-MODEL :140 + 020:36 RTIS 红线）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rtis_ref: Option<String>,
    pub title: String,
    /// 严重级（SEVERITIES 词表；SARIF level 三值映射见 render::sarif）。
    pub severity: String,
    /// 五态研究结论（RESULT_STATES 词表）。
    pub result_state: String,
    /// SARIF physicalLocation 对齐位。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location: Option<Location>,
    pub vuln_type: String,
    #[serde(default)]
    pub affected_scope: Vec<String>,
    /// 前置条件（markdown 片段——HTML 面经 comrak 渲染）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preconditions: Option<String>,
    /// 原理（markdown 片段）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub principle: Option<String>,
    /// 复现步骤（markdown 片段）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repro_steps: Option<String>,
    /// 请求/响应对（direction = request|response）。
    #[serde(default)]
    pub request_response: Vec<RequestResponse>,
    /// Evidence 引用集。
    #[serde(default)]
    pub evidence: Vec<EvidenceRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub impact: Option<String>,
    /// 修复建议（priority 词表 + suggestions）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remediation: Option<Remediation>,
    /// 技术解释字段位（8.3 双解释——本批只承载不生成，6b 生成器）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub technical_explanation: Option<String>,
    /// 通俗解释字段位（8.3 通俗七问——本批只承载不生成，6b 生成器）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plain_explanation: Option<String>,
    /// 自由扩展位。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub properties: Option<Map<String, Value>>,
}

/// SARIF physicalLocation 对齐位（D1 location{artifact_uri?, start_line?,
/// end_line?, snippet?}）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Location {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact_uri: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_line: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_line: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snippet: Option<String>,
}

/// 请求/响应对（D1 request_response[{direction, content}]）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestResponse {
    /// 方向（request|response 词表）。
    pub direction: String,
    pub content: String,
}

/// Evidence 引用（D1 EvidenceRef 7 字段——六 ID 投影 015/021 同款列名）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceRef {
    /// Evidence 稳定 ID（re- 前缀，021 词形）。
    pub evidence_id: String,
    pub artifact_id: String,
    pub tool_run_id: String,
    pub trace_id: String,
    /// uri 词形 ripartifact:// 或 rtis://（agent-engine/src/evidence.rs:109 先例）。
    pub uri: String,
    /// sha256 小写 hex（021 content_hash 口径）。
    pub content_hash: String,
    /// 021 CHECK 词表（可选——IR 引用面可只挂 ID）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evidence_type: Option<String>,
}

/// 修复建议（D1 remediation{priority, suggestions[]}）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Remediation {
    /// 优先级（REMEDIATION_PRIORITIES 词表）。
    pub priority: String,
    pub suggestions: Vec<String>,
}

/// 校验 IR 实例（Value 形态）：Rust 侧兜底 + schema 全语义校验。
///
/// Err 携带首个失败的说明（含字段名——判据 4 fail-closed 回显）。通道顺序
/// 反转于 intel-db 先例：Rust 兜底先跑（词表/前缀/词形/一致性——错误信息
/// 显式含字段名；jsonschema 0.56 消息不带 instance path，判据 4 要求字段名
/// 可回显），schema 作最后防线（未知字段/结构/pattern 全语义 fail-closed）。
pub fn validate_ir(instance: &Value) -> Result<(), String> {
    // 通道 1：Rust 侧兜底（错误信息显式含字段名 + schema 无法表达的一致性
    // 约束；serde 对未知字段宽容——未知字段面由通道 2 schema 拒）。
    let ir: ReportIr = serde_json::from_value(instance.clone())
        .map_err(|e| format!("report-ir deserialize: {e}"))?;
    validate_beyond_schema(&ir)?;

    // 通道 2：schema 全语义校验（draft 2020-12；未知字段/结构/pattern 全拒
    // ——fail-closed 最后防线）。
    let schema: Value = serde_json::from_str(IR_V1_SCHEMA)
        .map_err(|e| format!("report-ir schema is not valid JSON: {e}"))?;
    jsonschema_lite::validate_instance(&schema, instance)
}

/// 解析并校验 IR 实例（D4 入口：schema 校验 fail-closed 后反序列化）。
pub fn from_json(instance: &Value) -> Result<ReportIr, String> {
    validate_ir(instance)?;
    serde_json::from_value(instance.clone())
        .map_err(|e| format!("report-ir deserialize after validation: {e}"))
}

/// Rust 侧兜底校验（schema 之后的显式复检——错误信息含字段名）。
fn validate_beyond_schema(ir: &ReportIr) -> Result<(), String> {
    if ir.schema_version != "1" {
        return Err(format!(
            "schema_version: expected \"1\", got {:?}",
            ir.schema_version
        ));
    }
    if !ir.report_id.starts_with("rep-") {
        return Err(format!(
            "report_id: must start with \"rep-\", got {:?}",
            ir.report_id
        ));
    }
    if !REPORT_KINDS.contains(&ir.report_kind.as_str()) {
        return Err(format!(
            "report_kind: {:?} not in wordlist {REPORT_KINDS:?}",
            ir.report_kind
        ));
    }
    if let Some(task_ref) = &ir.task_ref {
        if let Some(task_id) = &task_ref.task_id {
            TaskId::try_new(task_id.as_str())
                .map_err(|_| format!("task_ref.task_id: empty value {:?}", task_id))?;
        }
    }
    for (idx, dim) in ir
        .coverage
        .iter()
        .flat_map(|c| c.dimensions.iter())
        .enumerate()
    {
        if !COVERAGE_DIMENSIONS.contains(&dim.dimension.as_str()) {
            return Err(format!(
                "coverage.dimensions[{idx}].dimension: {:?} not in seven-dimension wordlist {COVERAGE_DIMENSIONS:?}",
                dim.dimension
            ));
        }
    }
    for (idx, finding) in ir.findings.iter().enumerate() {
        validate_finding_beyond_schema(idx, finding)?;
    }
    Ok(())
}

/// Finding 侧兜底校验（词表/前缀/词形/一致性）。
fn validate_finding_beyond_schema(idx: usize, finding: &FindingIr) -> Result<(), String> {
    let at = format!("findings[{idx}]");
    if !finding.finding_id.starts_with("find-") {
        return Err(format!(
            "{at}.finding_id: must start with \"find-\", got {:?}",
            finding.finding_id
        ));
    }
    if !SEVERITIES.contains(&finding.severity.as_str()) {
        return Err(format!(
            "{at}.severity: {:?} not in wordlist {SEVERITIES:?}",
            finding.severity
        ));
    }
    if !RESULT_STATES.contains(&finding.result_state.as_str()) {
        return Err(format!(
            "{at}.result_state: {:?} not in five-state wordlist {RESULT_STATES:?}",
            finding.result_state
        ));
    }
    if let Some(rtis_ref) = &finding.rtis_ref {
        if !rtis_ref.starts_with(RTIS_REF_PREFIX)
            || !rtis_ref[RTIS_REF_PREFIX.len()..]
                .chars()
                .all(|c| c.is_ascii_digit())
            || rtis_ref.len() <= RTIS_REF_PREFIX.len()
        {
            return Err(format!(
                "{at}.rtis_ref: must match \"{RTIS_REF_PREFIX}<numeric id>\", got {rtis_ref:?}"
            ));
        }
    }
    if let Some(location) = &finding.location {
        if let (Some(start), Some(end)) = (location.start_line, location.end_line) {
            if start > end {
                return Err(format!(
                    "{at}.location: start_line {start} > end_line {end} (schema cannot express this)"
                ));
            }
        }
    }
    for rr in &finding.request_response {
        if rr.direction != "request" && rr.direction != "response" {
            return Err(format!(
                "{at}.request_response[].direction: {:?} not in [\"request\", \"response\"]",
                rr.direction
            ));
        }
    }
    if let Some(remediation) = &finding.remediation {
        if !REMEDIATION_PRIORITIES.contains(&remediation.priority.as_str()) {
            return Err(format!(
                "{at}.remediation.priority: {:?} not in wordlist {REMEDIATION_PRIORITIES:?}",
                remediation.priority
            ));
        }
    }
    for (eidx, evidence) in finding.evidence.iter().enumerate() {
        validate_evidence_beyond_schema(&format!("{at}.evidence[{eidx}]"), evidence)?;
    }
    Ok(())
}

/// EvidenceRef 侧兜底校验（前缀/词形/非空——domain ID 类型词形根）。
fn validate_evidence_beyond_schema(at: &str, evidence: &EvidenceRef) -> Result<(), String> {
    if !evidence.evidence_id.starts_with("re-") {
        return Err(format!(
            "{at}.evidence_id: must start with \"re-\", got {:?}",
            evidence.evidence_id
        ));
    }
    EvidenceId::try_new(evidence.evidence_id.as_str())
        .map_err(|_| format!("{at}.evidence_id: empty value"))?;
    ArtifactId::try_new(evidence.artifact_id.as_str())
        .map_err(|_| format!("{at}.artifact_id: empty value {:?}", evidence.artifact_id))?;
    TraceId::try_new(evidence.trace_id.as_str())
        .map_err(|_| format!("{at}.trace_id: empty value {:?}", evidence.trace_id))?;
    if evidence.tool_run_id.is_empty() {
        return Err(format!("{at}.tool_run_id: empty value"));
    }
    if !EVIDENCE_URI_PREFIXES
        .iter()
        .any(|prefix| evidence.uri.starts_with(prefix))
    {
        return Err(format!(
            "{at}.uri: must start with ripartifact:// or rtis://, got {:?}",
            evidence.uri
        ));
    }
    let hash_ok = evidence.content_hash.len() == 64
        && evidence
            .content_hash
            .chars()
            .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c));
    if !hash_ok {
        return Err(format!(
            "{at}.content_hash: must be 64 lowercase hex chars, got {:?}",
            evidence.content_hash
        ));
    }
    if let Some(evidence_type) = &evidence.evidence_type {
        if !EVIDENCE_TYPES.contains(&evidence_type.as_str()) {
            return Err(format!(
                "{at}.evidence_type: {:?} not in wordlist {EVIDENCE_TYPES:?}",
                evidence_type
            ));
        }
    }
    Ok(())
}
