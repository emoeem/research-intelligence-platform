//! `wsw_runtime::fuzz::spec`：FuzzSpec 声明式配置（WSW-4 D3/D4/D5）。
//!
//! 三面（D3 裁决）：
//! - **标记面** = 请求模板（[`OutboundRequest`] JSON）内 `§…§`（U+00A7）
//!   成对字节区间标记；未闭合/空标记/嵌套交叉 → 预检拒绝 fail-closed。
//!   标记符选 `§` 非 `{{}}`：SSTI payload 含 `{{7*7}}`，`{{}}` 必然冲突
//!   （D3 裁决原文；Burp Intruder `§…§` 成对分隔符先例）。
//! - **选择器面** = 结构化命名位置七类：`query`/`header`/`cookie`/`form`/
//!   `json`(RFC 6901 pointer)/`path_segment`/`multipart`；引擎直接定位
//!   OutboundRequest 对应字段值字节（不经标记改写）。
//! - **组合估算** = 单位置逐个（sum）/ 笛卡尔积（checked 乘）/ 同步迭代
//!   zip（等长校验）三模式（D10；PLAN :248-250），u64 溢出饱和拒。
//!
//! fail-closed 预检（判据 3/4）：标记词法 + 选择器目标在场 + payloads 与
//! 位置数一致 + budget 在场且 max_requests ≥ 1 + 字典文件可读非空 +
//! serialize_request 模板可序列化（CRLF 注入守卫既有词形复用）。

use crate::persist::FuzzJobRow;
use serde::{Deserialize, Serialize};
use wsw_message::serialize::OutboundRequest;
use wsw_message::types::HeaderRecord;

/// 标记分隔符（U+00A7，D3）。
pub const MARKER: char = '§';
/// UTF-8 编码下的标记字节序列（0xC2 0xA7；body 字节面扫描锚）。
const MARKER_BYTES: [u8; 2] = [0xC2, 0xA7];

/// spec 错误词表（fail-closed 预检面；`fuzz_spec_*` 自别前缀——红线 6）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpecError {
    /// spec JSON 解析失败。
    Json(String),
    /// 模板不可序列化（serialize_request 既有守卫词形直通）。
    TemplateInvalid(String),
    /// 标记未闭合（奇数个 §）。
    MarkerUnclosed,
    /// 空标记（`§§`）。
    MarkerEmpty,
    /// payload 源解析失败（字典读文件/范围非法）。
    PayloadSource(String),
    /// payloads 数量 ≠ 位置数。
    PayloadCountMismatch { positions: usize, payloads: usize },
    /// 选择器目标字段不在模板（fail-closed：不静默造默认值）。
    SelectorTargetMissing(String),
    /// 选择器目标字段与标记面同字段冲突（歧义拒）。
    SelectorMarkerConflict(String),
    /// 组合数估算溢出（u64 饱和拒）。
    EstimateOverflow,
    /// zip 模式列表不等长。
    ZipLengthMismatch,
    /// budget 缺失或非法（无显式 budget = spec 拒绝，D5）。
    BudgetInvalid(String),
    /// 单 case 构建失败（payload 注入/processor 面守卫）。
    CaseBuild(String),
    /// mutator 参数非法（regex 编译失败等）。
    MutatorInvalid(String),
}

impl std::fmt::Display for SpecError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SpecError::Json(m) => write!(f, "fuzz_spec_invalid: {m}"),
            SpecError::TemplateInvalid(m) => write!(f, "fuzz_spec_template_invalid: {m}"),
            SpecError::MarkerUnclosed => write!(f, "fuzz_spec_marker_unclosed"),
            SpecError::MarkerEmpty => write!(f, "fuzz_spec_marker_empty"),
            SpecError::PayloadSource(m) => write!(f, "fuzz_spec_payload_source: {m}"),
            SpecError::PayloadCountMismatch {
                positions,
                payloads,
            } => {
                write!(
                    f,
                    "fuzz_spec_payload_count_mismatch: positions={positions} payloads={payloads}"
                )
            }
            SpecError::SelectorTargetMissing(m) => {
                write!(f, "fuzz_spec_selector_target_missing: {m}")
            }
            SpecError::SelectorMarkerConflict(m) => {
                write!(f, "fuzz_spec_selector_marker_conflict: {m}")
            }
            SpecError::EstimateOverflow => write!(f, "fuzz_spec_estimate_overflow"),
            SpecError::ZipLengthMismatch => write!(f, "fuzz_spec_zip_length_mismatch"),
            SpecError::BudgetInvalid(m) => write!(f, "fuzz_spec_budget_invalid: {m}"),
            SpecError::CaseBuild(m) => write!(f, "fuzz_spec_case_build: {m}"),
            SpecError::MutatorInvalid(m) => write!(f, "fuzz_spec_mutator_invalid: {m}"),
        }
    }
}

/// 运行模式（D10 三模式；PLAN :248-250）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FuzzMode {
    /// 单位置逐个（其余位置保模板原值；Burp Sniper 语义）。
    Single,
    /// 多位置笛卡尔积（Burp Cluster bomb 语义）。
    Cartesian,
    /// 多位置同步迭代（等长 zip）。
    Zip,
}

/// 预算帽（D5；Rust 强制面。无显式 budget = spec 拒绝——fail-closed）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FuzzBudget {
    /// 总请求帽（必填 ≥ 1）。
    pub max_requests: u64,
    /// 并发帽（缺省 2 = 默认低速低并发，PLAN :271）。
    #[serde(default = "default_max_concurrency")]
    pub max_concurrency: usize,
    /// 滑动窗口毫秒（0 = 不限；与 rate_max_requests 成对出现）。
    #[serde(default)]
    pub rate_window_ms: u64,
    /// 窗口内请求帽（0 = 不限）。
    #[serde(default)]
    pub rate_max_requests: u64,
    /// 相邻请求最小间隔毫秒（0 = 不限）。
    #[serde(default)]
    pub min_request_interval_ms: u64,
    /// job 最长运行毫秒（0 = 不限）。
    #[serde(default)]
    pub max_duration_ms: u64,
}

fn default_max_concurrency() -> usize {
    2
}

impl FuzzBudget {
    /// budget 校验（max_requests ≥ 1 / max_concurrency ≥ 1 / 速率对一致）。
    pub fn validate(&self) -> Result<(), SpecError> {
        if self.max_requests == 0 {
            return Err(SpecError::BudgetInvalid("max_requests must be >= 1".into()));
        }
        if self.max_concurrency == 0 {
            return Err(SpecError::BudgetInvalid(
                "max_concurrency must be >= 1".into(),
            ));
        }
        if (self.rate_window_ms == 0) != (self.rate_max_requests == 0) {
            return Err(SpecError::BudgetInvalid(
                "rate_window_ms and rate_max_requests must both be zero or both non-zero".into(),
            ));
        }
        Ok(())
    }

    /// 行承载投影（028 预算六列：max_requests/max_concurrency/rate_window_ms/
    /// rate_max_requests/min_request_interval_ms/max_duration_ms）。
    pub fn to_row_fields(&self) -> (i64, i64, i64, i64, i64, i64) {
        (
            self.max_requests.min(i64::MAX as u64) as i64,
            self.max_concurrency.min(i64::MAX as usize) as i64,
            self.rate_window_ms.min(i64::MAX as u64) as i64,
            self.rate_max_requests.min(i64::MAX as u64) as i64,
            self.min_request_interval_ms.min(i64::MAX as u64) as i64,
            self.max_duration_ms.min(i64::MAX as u64) as i64,
        )
    }
}

/// 选择器七类（D3；serde 内部 tag `selector`）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "selector", rename_all = "snake_case")]
pub enum PositionSelector {
    /// URL query 参数值（target origin/absolute form 均可）。
    Query { name: String },
    /// 请求头值（名大小写不敏感）。
    Header { name: String },
    /// Cookie 头内单个 cookie 值。
    Cookie { name: String },
    /// 表单体字段值（application/x-www-form-urlencoded）。
    Form { name: String },
    /// JSON 体 RFC 6901 pointer 指向的标量值。
    Json { pointer: String },
    /// URL path 第 n 段（0 起，跳过前导空段）。
    PathSegment { index: usize },
    /// multipart/form-data part 字段值。
    Multipart { field: String },
}

impl PositionSelector {
    /// 选择器展示名（落库/日志面）。
    pub fn describe(&self) -> String {
        match self {
            PositionSelector::Query { name } => format!("query={name}"),
            PositionSelector::Header { name } => format!("header={name}"),
            PositionSelector::Cookie { name } => format!("cookie={name}"),
            PositionSelector::Form { name } => format!("form={name}"),
            PositionSelector::Json { pointer } => format!("json={pointer}"),
            PositionSelector::PathSegment { index } => format!("path_segment={index}"),
            PositionSelector::Multipart { field } => format!("multipart={field}"),
        }
    }
}

/// 结果分析配置（关键字/正则命中；PLAN :258/:261）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct AnalyzeConfig {
    /// 关键字面（体文本子串命中计数）。
    #[serde(default)]
    pub keywords: Vec<String>,
    /// 正则面（regex crate，编译失败 = spec 拒）。
    #[serde(default)]
    pub regexes: Vec<String>,
}

/// Payload 源（D4 三源；声明式 `type` 词表）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PayloadSource {
    /// 手工列表。
    List { values: Vec<String> },
    /// 本地字典文件（逐行；BOM 剥离 / 空行 / `#` 注释跳过）。
    Wordlist { path: String },
    /// 数字范围（含端点）。
    RangeNumber { from: i64, to: i64, step: i64 },
    /// 字母序列（长度 from_len..=to_len 的小写字母全排列）。
    RangeAlpha { from_len: u32, to_len: u32 },
    /// 日期范围（YYYY-MM-DD 含端点，步进天）。
    RangeDate {
        from: String,
        to: String,
        step_days: u32,
    },
}

/// 字典/生成列表行数帽（fail-closed 防失控生成）。
pub const MAX_PAYLOAD_ITEMS: usize = 100_000;

impl PayloadSource {
    /// 展开为 payload 列表（字典文件在此读取）。
    pub fn resolve(&self) -> Result<Vec<String>, SpecError> {
        let out = match self {
            PayloadSource::List { values } => values.clone(),
            PayloadSource::Wordlist { path } => {
                let text = std::fs::read_to_string(path)
                    .map_err(|e| SpecError::PayloadSource(format!("wordlist read {path}: {e}")))?;
                let mut lines = Vec::new();
                for (i, raw) in text.lines().enumerate() {
                    // 首行 BOM 剥离；空行 / `#` 注释跳过（判据 4）。
                    let line = if i == 0 {
                        raw.strip_prefix('\u{feff}').unwrap_or(raw)
                    } else {
                        raw
                    };
                    let line = line.trim_end_matches('\r');
                    let t = line.trim();
                    if t.is_empty() || t.starts_with('#') {
                        continue;
                    }
                    lines.push(line.to_string());
                    if lines.len() > MAX_PAYLOAD_ITEMS {
                        return Err(SpecError::PayloadSource(format!(
                            "wordlist exceeds {MAX_PAYLOAD_ITEMS} items"
                        )));
                    }
                }
                lines
            }
            PayloadSource::RangeNumber { from, to, step } => {
                if *step == 0 {
                    return Err(SpecError::PayloadSource("range step must be != 0".into()));
                }
                let mut out = Vec::new();
                if *step > 0 {
                    let mut v = *from;
                    while v <= *to {
                        out.push(v.to_string());
                        v = v.saturating_add(*step);
                        if out.len() > MAX_PAYLOAD_ITEMS {
                            return Err(SpecError::PayloadSource(format!(
                                "range exceeds {MAX_PAYLOAD_ITEMS} items"
                            )));
                        }
                    }
                } else {
                    let mut v = *from;
                    while v >= *to {
                        out.push(v.to_string());
                        match v.checked_sub(step.unsigned_abs() as i64) {
                            Some(nv) => v = nv,
                            None => break,
                        }
                        if out.len() > MAX_PAYLOAD_ITEMS {
                            return Err(SpecError::PayloadSource(format!(
                                "range exceeds {MAX_PAYLOAD_ITEMS} items"
                            )));
                        }
                    }
                }
                out
            }
            PayloadSource::RangeAlpha { from_len, to_len } => {
                if from_len == &0 || to_len < from_len || to_len > &4 {
                    return Err(SpecError::PayloadSource(
                        "alpha range requires 1 <= from_len <= to_len <= 4".into(),
                    ));
                }
                let mut out = Vec::new();
                for len in *from_len..=*to_len {
                    let total = 26usize.pow(len);
                    if out.len() + total > MAX_PAYLOAD_ITEMS {
                        return Err(SpecError::PayloadSource(format!(
                            "alpha range exceeds {MAX_PAYLOAD_ITEMS} items"
                        )));
                    }
                    // 计数进制遍历（确定性；不用 itertools——零新依赖）。
                    for i in 0..total {
                        let mut s = String::with_capacity(len as usize);
                        let mut v = i;
                        for _ in 0..len {
                            s.push((b'a' + (v % 26) as u8) as char);
                            v /= 26;
                        }
                        out.push(s);
                    }
                }
                out
            }
            PayloadSource::RangeDate {
                from,
                to,
                step_days,
            } => {
                if step_days == &0 {
                    return Err(SpecError::PayloadSource("date step must be != 0".into()));
                }
                let parse = |s: &str| -> Result<i64, SpecError> {
                    // YYYY-MM-DD → epoch day（纯整数历法换算，零依赖）。
                    let parts: Vec<&str> = s.split('-').collect();
                    if parts.len() != 3 {
                        return Err(SpecError::PayloadSource(format!(
                            "date must be YYYY-MM-DD: {s}"
                        )));
                    }
                    let y: i64 = parts[0]
                        .parse()
                        .map_err(|_| SpecError::PayloadSource(format!("bad date year: {s}")))?;
                    let m: i64 = parts[1]
                        .parse()
                        .map_err(|_| SpecError::PayloadSource(format!("bad date month: {s}")))?;
                    let d: i64 = parts[2]
                        .parse()
                        .map_err(|_| SpecError::PayloadSource(format!("bad date day: {s}")))?;
                    if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
                        return Err(SpecError::PayloadSource(format!("bad date: {s}")));
                    }
                    // days-from-civil（Howard Hinnant 算法）。
                    let y2 = if m <= 2 { y - 1 } else { y };
                    let era = if y2 >= 0 { y2 } else { y2 - 399 } / 400;
                    let yoe = y2 - era * 400;
                    let mp = (m + 9) % 12;
                    let doy = (153 * mp + 2) / 5 + d - 1;
                    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
                    Ok(era * 146_097 + doe - 719_468)
                };
                let from_d = parse(from)?;
                let to_d = parse(to)?;
                let fmt = |days: i64| -> String {
                    let z = days + 719_468;
                    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
                    let doe = z - era * 146_097;
                    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
                    let y = yoe + era * 400;
                    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
                    let mp = (5 * doy + 2) / 153;
                    let d = doy - (153 * mp + 2) / 5 + 1;
                    let m = if mp < 10 { mp + 3 } else { mp - 9 };
                    let y = if m <= 2 { y + 1 } else { y };
                    format!("{y:04}-{m:02}-{d:02}")
                };
                let mut out = Vec::new();
                let mut cur = from_d;
                while cur <= to_d {
                    out.push(fmt(cur));
                    cur += *step_days as i64;
                    if out.len() > MAX_PAYLOAD_ITEMS {
                        return Err(SpecError::PayloadSource(format!(
                            "date range exceeds {MAX_PAYLOAD_ITEMS} items"
                        )));
                    }
                }
                out
            }
        };
        if out.is_empty() {
            return Err(SpecError::PayloadSource(
                "payload source resolved to zero items".into(),
            ));
        }
        Ok(out)
    }
}

/// FuzzSpec 全量（spec_json；028 行承载）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FuzzSpec {
    /// 请求模板（可含 §…§ 标记）。
    pub request: OutboundRequest,
    /// 结构化选择器位置（顺序 = 位置序）。
    #[serde(default)]
    pub positions: Vec<PositionSelector>,
    /// payload 源（数量必须 = 位置总数 = positions + 标记数）。
    pub payloads: Vec<PayloadSource>,
    /// Mutator 有序链（D4）。
    #[serde(default)]
    pub mutators: Vec<crate::fuzz::mutator::Mutator>,
    /// Processor 有序链（请求侧后处理，D4）。
    #[serde(default)]
    pub processors: Vec<crate::fuzz::mutator::Processor>,
    /// 结果分析配置。
    #[serde(default)]
    pub analyze: AnalyzeConfig,
    /// 运行模式。
    pub mode: FuzzMode,
    /// 预算帽（必填；无显式 budget = spec 拒绝，D5）。
    pub budget: FuzzBudget,
}

/// 标记位置（模板字段 + 字节区间）：
/// [start, end) = 标记内容（payload 替换目标），[full_start, full_end) =
/// 含 `§` 标记字节的完整区间（构建面 splice 目标——标记分隔符不进最终
/// 请求，D3）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MarkerSpan {
    /// target 串内区间。
    Target {
        full_start: usize,
        full_end: usize,
        start: usize,
        end: usize,
    },
    /// 第 idx 个头（headers 下标）的 value 内区间。
    Header {
        index: usize,
        full_start: usize,
        full_end: usize,
        start: usize,
        end: usize,
    },
    /// body 字节区间。
    Body {
        full_start: usize,
        full_end: usize,
        start: usize,
        end: usize,
    },
}

/// spec 预备产物（JobRunner 消费面）。
pub struct PreparedSpec {
    pub spec: FuzzSpec,
    /// 每位置 mutator 展开后的有效 payload 列表（估算口径）。
    pub effective_payloads: Vec<Vec<String>>,
    /// 模板标记区间（scan 序：target → headers → body）。
    pub marker_spans: Vec<MarkerSpan>,
    /// 位置总数（= marker_spans.len() + positions.len()）。
    pub position_count: usize,
    /// 组合估算（有效列表口径；三模式）。
    pub estimated_requests: u64,
}

/// 从 spec JSON 文本解析 + 全量预检（CLI 与 JobRunner 共用；fail-closed）。
pub fn prepare_spec(json: &str) -> Result<PreparedSpec, SpecError> {
    let spec: FuzzSpec = serde_json::from_str(json).map_err(|e| SpecError::Json(e.to_string()))?;
    spec.budget.validate()?;

    // 模板可序列化（serialize_request 既有守卫直通；CRLF 注入拒既有词形）。
    wsw_message::serialize::serialize_request(&spec.request)
        .map_err(|e| SpecError::TemplateInvalid(e.to_string()))?;

    // 标记词法（target → headers → body scan 序）。
    let marker_spans = parse_marker_spans(&spec.request)?;
    let position_count = marker_spans.len() + spec.positions.len();

    // payload 源解析 + mutator 展开（有效列表口径）。
    if spec.payloads.len() != position_count {
        return Err(SpecError::PayloadCountMismatch {
            positions: position_count,
            payloads: spec.payloads.len(),
        });
    }
    let mut effective_payloads = Vec::with_capacity(spec.payloads.len());
    for src in &spec.payloads {
        let base = src.resolve()?;
        let expanded = crate::fuzz::mutator::apply_mutators(&base, &spec.mutators)?;
        effective_payloads.push(expanded);
    }

    // 选择器目标在场 + 与标记面同字段冲突拒（fail-closed）。
    for sel in &spec.positions {
        check_selector_target(&spec.request, sel, &marker_spans)?;
    }

    let estimated_requests = estimate(&effective_payloads, spec.mode)?;
    Ok(PreparedSpec {
        spec,
        effective_payloads,
        marker_spans,
        position_count,
        estimated_requests,
    })
}

/// 行投影（estimate 落 028 estimated_requests 列；i64 饱和）。
pub fn estimate_to_row(estimated: u64) -> i64 {
    estimated.min(i64::MAX as u64) as i64
}

/// job 行组装 helper（CLI 面复用；预算六列 + estimate）。
pub fn job_row_from_prepared(
    prepared: &PreparedSpec,
    job_id: &str,
    session_id: &str,
    spec_json: &str,
    now: String,
) -> FuzzJobRow {
    let (
        max_requests,
        max_concurrency,
        rate_window_ms,
        rate_max_requests,
        min_request_interval_ms,
        max_duration_ms,
    ) = prepared.spec.budget.to_row_fields();
    FuzzJobRow {
        job_id: job_id.to_string(),
        session_id: session_id.to_string(),
        listener_id: crate::fuzz::persist::FUZZ_LISTENER_ID.to_string(),
        state: "CREATED".to_string(),
        spec_json: spec_json.to_string(),
        estimated_requests: estimate_to_row(prepared.estimated_requests),
        max_requests,
        max_concurrency,
        rate_window_ms,
        rate_max_requests,
        min_request_interval_ms,
        max_duration_ms,
        baseline_case_id: None,
        trace_id: None,
        task_id: None,
        evidence_id: None,
        started_at: None,
        completed_at: None,
        duration_ms: None,
        error_code: None,
        created_at: now,
    }
}

/// 标记词法解析（scan 序 target → headers → body；奇数 = 未闭合拒、
/// 空内容 = 空标记拒——D3 fail-closed）。
pub fn parse_marker_spans(req: &OutboundRequest) -> Result<Vec<MarkerSpan>, SpecError> {
    let mut spans = Vec::new();
    // target（UTF-8 字节偏移）。
    scan_str_markers(&req.target, |fs, fe, s, e| {
        spans.push(MarkerSpan::Target {
            full_start: fs,
            full_end: fe,
            start: s,
            end: e,
        })
    })?;
    // headers（value 串内区间；头名不扫）。
    for (i, h) in req.headers.iter().enumerate() {
        scan_str_markers(&h.value, |fs, fe, s, e| {
            spans.push(MarkerSpan::Header {
                index: i,
                full_start: fs,
                full_end: fe,
                start: s,
                end: e,
            })
        })?;
    }
    // body（字节面 0xC2 0xA7 对扫）。
    let b = &req.body;
    let mut i = 0usize;
    let mut open: Option<usize> = None;
    while i + 1 < b.len() {
        if b[i] == MARKER_BYTES[0] && b[i + 1] == MARKER_BYTES[1] {
            match open {
                None => open = Some(i),
                Some(o) => {
                    if i == o + 2 {
                        return Err(SpecError::MarkerEmpty);
                    }
                    spans.push(MarkerSpan::Body {
                        full_start: o,
                        full_end: i + 2,
                        start: o + 2,
                        end: i,
                    });
                    open = None;
                }
            }
            i += 2;
        } else {
            i += 1;
        }
    }
    if open.is_some() {
        return Err(SpecError::MarkerUnclosed);
    }
    Ok(spans)
}

/// 字符串内成对标记扫描（奇数拒 / 空拒；字节偏移。sink = (full_start,
/// full_end, content_start, content_end)）。
fn scan_str_markers(
    text: &str,
    mut sink: impl FnMut(usize, usize, usize, usize),
) -> Result<(), SpecError> {
    let bytes = text.as_bytes();
    let mut i = 0usize;
    let mut open: Option<usize> = None;
    while i < bytes.len() {
        if bytes[i] == MARKER_BYTES[0] && i + 1 < bytes.len() && bytes[i + 1] == MARKER_BYTES[1] {
            match open {
                None => open = Some(i),
                Some(o) => {
                    if i == o + 2 {
                        return Err(SpecError::MarkerEmpty);
                    }
                    sink(o, i + 2, o + 2, i);
                    open = None;
                }
            }
            i += 2;
        } else {
            i += 1;
        }
    }
    if open.is_some() {
        return Err(SpecError::MarkerUnclosed);
    }
    Ok(())
}

/// 单标记内容替换（同字段多标记按区间降序 splice——调用方保证顺序）。
pub fn splice_str(text: &str, start: usize, end: usize, payload: &str) -> String {
    let mut out = String::with_capacity(text.len() + payload.len());
    out.push_str(&text[..start]);
    out.push_str(payload);
    out.push_str(&text[end..]);
    out
}

/// body 字节区间 splice。
pub fn splice_bytes(body: &[u8], start: usize, end: usize, payload: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(body.len() + payload.len());
    out.extend_from_slice(&body[..start]);
    out.extend_from_slice(payload.as_bytes());
    out.extend_from_slice(&body[end..]);
    out
}

fn full_end_of(s: &MarkerSpan) -> usize {
    match s {
        MarkerSpan::Target { full_end, .. }
        | MarkerSpan::Header { full_end, .. }
        | MarkerSpan::Body { full_end, .. } => *full_end,
    }
}

/// 单 case 构建：位置 payload 向量（None = 保模板原值）+ processors →
/// OutboundRequest 副本（serialize 守卫在发送面复用，不重复实现）。
pub fn build_case_request(
    spec: &FuzzSpec,
    marker_spans: &[MarkerSpan],
    assignments: &[Option<&str>],
) -> Result<OutboundRequest, SpecError> {
    let total = marker_spans.len() + spec.positions.len();
    if assignments.len() != total {
        return Err(SpecError::CaseBuild(format!(
            "assignments {} != positions {total}",
            assignments.len()
        )));
    }
    let mut req = spec.request.clone();

    // 1) 标记面：同字段区间降序 splice（偏移不漂移）。
    let marker_assignments: Vec<(usize, &str)> = marker_spans
        .iter()
        .enumerate()
        .filter_map(|(i, _)| assignments[i].map(|p| (i, p)))
        .collect();
    // 同字段多标记按声明序逐个 splice（区间以模板字节偏移计——每个标记
    // 独立区间，前序 splice 不影响后序区间吗？——影响：splice 使字符串
    // 变长，后序 full_start 偏移漂移。同字段多标记场景按 full_end 降序
    // splice（从右往左，偏移稳定）。
    let mut marker_assignments: Vec<(usize, &str)> = marker_assignments;
    let field_of = |s: &MarkerSpan| match s {
        MarkerSpan::Target { .. } => 0usize,
        MarkerSpan::Header { index, .. } => 1 + *index,
        MarkerSpan::Body { .. } => usize::MAX,
    };
    marker_assignments.sort_by(|a, b| {
        let (sa, sb) = (&marker_spans[a.0], &marker_spans[b.0]);
        (field_of(sa), full_end_of(sa)).cmp(&(field_of(sb), full_end_of(sb)))
    });
    for (pos_i, payload) in marker_assignments.iter().rev() {
        match &marker_spans[*pos_i] {
            MarkerSpan::Target {
                full_start,
                full_end,
                ..
            } => {
                req.target = splice_str(&req.target, *full_start, *full_end, payload);
            }
            MarkerSpan::Header {
                index,
                full_start,
                full_end,
                ..
            } => {
                let h = &mut req.headers[*index];
                h.value = splice_str(&h.value, *full_start, *full_end, payload);
            }
            MarkerSpan::Body {
                full_start,
                full_end,
                ..
            } => {
                req.body = splice_bytes(&req.body, *full_start, *full_end, payload);
            }
        }
    }

    // 2) 选择器面：结构化定位替换（不经标记）。
    for (k, sel) in spec.positions.iter().enumerate() {
        let idx = marker_spans.len() + k;
        if let Some(payload) = assignments[idx] {
            apply_selector(&mut req, sel, payload)?;
        }
    }

    // 3) Processor 面（请求侧后处理；头值合法性由发送面 serialize 守卫）。
    crate::fuzz::mutator::apply_processors(&mut req, &spec.processors)?;
    Ok(req)
}

/// 选择器定位替换（七类；目标缺失 = fail-closed 拒）。
pub fn apply_selector(
    req: &mut OutboundRequest,
    sel: &PositionSelector,
    payload: &str,
) -> Result<(), SpecError> {
    match sel {
        PositionSelector::Query { name } => {
            let (path_q, has_q) = match req.target.find('?') {
                Some(p) => (req.target[p + 1..].to_string(), true),
                None => (String::new(), false),
            };
            if !has_q {
                return Err(SpecError::SelectorTargetMissing(sel.describe()));
            }
            let mut parts: Vec<String> = path_q.split('&').map(String::from).collect();
            let mut hit = false;
            for part in parts.iter_mut() {
                let key = part.split('=').next().unwrap_or("");
                if key == name {
                    *part = format!("{name}={payload}");
                    hit = true;
                }
            }
            if !hit {
                return Err(SpecError::SelectorTargetMissing(sel.describe()));
            }
            let base = req.target.split('?').next().unwrap_or("").to_string();
            req.target = format!("{base}?{}", parts.join("&"));
            Ok(())
        }
        PositionSelector::Header { name } => {
            let h = req
                .headers
                .iter_mut()
                .find(|h| h.name.eq_ignore_ascii_case(name))
                .ok_or_else(|| SpecError::SelectorTargetMissing(sel.describe()))?;
            h.value = payload.to_string();
            Ok(())
        }
        PositionSelector::Cookie { name } => {
            let h = req
                .headers
                .iter_mut()
                .find(|h| h.name.eq_ignore_ascii_case("cookie"))
                .ok_or_else(|| SpecError::SelectorTargetMissing(sel.describe()))?;
            let mut parts: Vec<String> = h.value.split(';').map(|s| s.trim().to_string()).collect();
            let mut hit = false;
            for part in parts.iter_mut() {
                if let Some(eq) = part.find('=') {
                    if &part[..eq] == name {
                        *part = format!("{name}={payload}");
                        hit = true;
                    }
                }
            }
            if !hit {
                return Err(SpecError::SelectorTargetMissing(sel.describe()));
            }
            h.value = parts.join("; ");
            Ok(())
        }
        PositionSelector::Form { name } => {
            let text = String::from_utf8(req.body.clone())
                .map_err(|_| SpecError::SelectorTargetMissing("form body not utf-8".into()))?;
            let mut parts: Vec<String> = text.split('&').map(String::from).collect();
            let mut hit = false;
            for part in parts.iter_mut() {
                let key = part.split('=').next().unwrap_or("");
                if key == name {
                    *part = format!("{name}={payload}");
                    hit = true;
                }
            }
            if !hit {
                return Err(SpecError::SelectorTargetMissing(sel.describe()));
            }
            req.body = parts.join("&").into_bytes();
            Ok(())
        }
        PositionSelector::Json { pointer } => {
            let text = String::from_utf8(req.body.clone())
                .map_err(|_| SpecError::SelectorTargetMissing("json body not utf-8".into()))?;
            let mut value: serde_json::Value = serde_json::from_str(&text)
                .map_err(|e| SpecError::SelectorTargetMissing(format!("json parse: {e}")))?;
            replace_json_pointer(&mut value, pointer, payload)
                .map_err(|e| SpecError::SelectorTargetMissing(format!("json={pointer}: {e}")))?;
            req.body = serde_json::to_vec(&value)
                .map_err(|e| SpecError::CaseBuild(format!("json reserialize: {e}")))?;
            Ok(())
        }
        PositionSelector::PathSegment { index } => {
            let query = req
                .target
                .find('?')
                .map(|p| req.target[p..].to_string())
                .unwrap_or_default();
            let path = req.target.split('?').next().unwrap_or("");
            let mut segs: Vec<&str> = path.split('/').collect();
            // "/a/b" split → ["", "a", "b"]：跳过前导空段计数。
            let logical: Vec<usize> = segs
                .iter()
                .enumerate()
                .filter(|(i, s)| !(*i == 0 && s.is_empty()))
                .map(|(i, _)| i)
                .collect();
            let Some(si) = logical.get(*index) else {
                return Err(SpecError::SelectorTargetMissing(sel.describe()));
            };
            segs[*si] = payload;
            req.target = format!("{}{}", segs.join("/"), query);
            Ok(())
        }
        PositionSelector::Multipart { field } => {
            let ct = req
                .headers
                .iter()
                .find(|h| h.name.eq_ignore_ascii_case("content-type"))
                .map(|h| h.value.clone())
                .ok_or_else(|| SpecError::SelectorTargetMissing(sel.describe()))?;
            let boundary = ct
                .split(';')
                .map(str::trim)
                .find_map(|p| p.strip_prefix("boundary="))
                .map(|b| b.trim_matches('"').to_string())
                .ok_or_else(|| {
                    SpecError::SelectorTargetMissing(format!("{}: no boundary", sel.describe()))
                })?;
            req.body =
                replace_multipart_field(&req.body, &boundary, field, payload).map_err(|e| {
                    SpecError::SelectorTargetMissing(format!("{}: {e}", sel.describe()))
                })?;
            Ok(())
        }
    }
}

/// RFC 6901 pointer 定位标量替换（对象键/数组下标；非标量拒）。
fn replace_json_pointer(
    value: &mut serde_json::Value,
    pointer: &str,
    payload: &str,
) -> Result<(), String> {
    if !pointer.starts_with('/') {
        return Err("pointer must start with '/'".into());
    }
    let tokens: Vec<String> = pointer[1..]
        .split('/')
        .map(|t| t.replace("~1", "/").replace("~0", "~"))
        .collect();
    let mut cur = value;
    let last = tokens.len() - 1;
    for (i, token) in tokens.iter().enumerate() {
        if i == last {
            let slot = match cur {
                serde_json::Value::Object(map) => map
                    .get_mut(token.as_str())
                    .ok_or_else(|| format!("key missing: {token}"))?,
                serde_json::Value::Array(arr) => {
                    let idx: usize = token.parse().map_err(|_| format!("bad index: {token}"))?;
                    arr.get_mut(idx)
                        .ok_or_else(|| format!("index missing: {token}"))?
                }
                _ => return Err("parent not object/array".into()),
            };
            if slot.is_object() || slot.is_array() {
                return Err("target is not scalar".into());
            }
            *slot = serde_json::Value::String(payload.to_string());
            return Ok(());
        }
        cur = match cur {
            serde_json::Value::Object(map) => map
                .get_mut(token.as_str())
                .ok_or_else(|| format!("key missing: {token}"))?,
            serde_json::Value::Array(arr) => {
                let idx: usize = token.parse().map_err(|_| format!("bad index: {token}"))?;
                arr.get_mut(idx)
                    .ok_or_else(|| format!("index missing: {token}"))?
            }
            _ => return Err("parent not object/array".into()),
        };
    }
    Ok(())
}

/// multipart part 字段值替换（浅层扫描：boundary 分段 + name= 匹配；
/// 深嵌套/命名空间限制登记批次报告——D10 XML 同源口径）。
fn replace_multipart_field(
    body: &[u8],
    boundary: &str,
    field: &str,
    payload: &str,
) -> Result<Vec<u8>, String> {
    let delim = format!("--{boundary}");
    let db = delim.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(body.len() + payload.len());
    let mut pos = 0usize;
    let mut replaced = false;
    while pos < body.len() {
        let Some(next) = find_sub(body, pos, db) else {
            out.extend_from_slice(&body[pos..]);
            break;
        };
        // 该段 = [pos, next)；段内找 name=field 的 part。
        let seg = &body[pos..next];
        if let Some(new_seg) = replace_part_value(seg, field, payload) {
            out.extend_from_slice(&new_seg);
            replaced = true;
        } else {
            out.extend_from_slice(seg);
        }
        out.extend_from_slice(db);
        pos = next + db.len();
    }
    if !replaced {
        return Err(format!("multipart field missing: {field}"));
    }
    Ok(out)
}

/// 单段内 part 值替换（headers 结束后到段尾 CRLF 前为值区）。
fn replace_part_value(seg: &[u8], field: &str, payload: &str) -> Option<Vec<u8>> {
    let text_start = std::str::from_utf8(&seg[..seg.len().min(4096)]).ok()?;
    if !text_start.contains(&format!("name=\"{field}\"")) {
        return None;
    }
    // 头块结束锚（CRLFCRLF 或 LFLF）。
    let (hdr_end, sep_len) = find_headers_end(seg)?;
    // 段尾 CRLF（boundary 前的 \r\n）为 part 分隔，非值内容。
    let mut value_end = seg.len();
    if seg.ends_with(b"\r\n") {
        value_end -= 2;
    } else if seg.ends_with(b"\n") {
        value_end -= 1;
    }
    if value_end < hdr_end + sep_len {
        return None;
    }
    let mut out = Vec::with_capacity(seg.len() + payload.len());
    out.extend_from_slice(&seg[..hdr_end + sep_len]);
    out.extend_from_slice(payload.as_bytes());
    out.extend_from_slice(&seg[value_end..]);
    Some(out)
}

/// 头块结束位置（CRLFCRLF → (end_of_crlfcrlf, 0)；LFLF 同型）。
fn find_headers_end(seg: &[u8]) -> Option<(usize, usize)> {
    for i in 0..seg.len().saturating_sub(3) {
        if &seg[i..i + 4] == b"\r\n\r\n" {
            return Some((i, 4));
        }
        if &seg[i..i + 2] == b"\n\n" {
            return Some((i, 2));
        }
    }
    None
}

/// 子串查找。
fn find_sub(hay: &[u8], from: usize, needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || from >= hay.len() {
        return None;
    }
    hay[from..]
        .windows(needle.len())
        .position(|w| w == needle)
        .map(|p| p + from)
}

/// 选择器目标在场 + 同字段标记冲突检查（fail-closed）。
fn check_selector_target(
    req: &OutboundRequest,
    sel: &PositionSelector,
    spans: &[MarkerSpan],
) -> Result<(), SpecError> {
    // 同字段冲突面：选择器目标字段内存在标记 → 歧义拒。
    let conflict = |field_has_marker: bool| -> Result<(), SpecError> {
        if field_has_marker {
            return Err(SpecError::SelectorMarkerConflict(sel.describe()));
        }
        Ok(())
    };
    match sel {
        PositionSelector::Query { name } => {
            conflict(spans.iter().any(|s| matches!(s, MarkerSpan::Target { .. })))?;
            if !req.target.contains('?') {
                return Err(SpecError::SelectorTargetMissing(sel.describe()));
            }
            let q = req.target.split('?').nth(1).unwrap_or("");
            let hit = q
                .split('&')
                .any(|p| p.split('=').next() == Some(name.as_str()));
            if !hit {
                return Err(SpecError::SelectorTargetMissing(sel.describe()));
            }
        }
        PositionSelector::Header { name } => {
            conflict(spans.iter().any(|s| matches!(s, MarkerSpan::Header { .. })))?;
            if !req
                .headers
                .iter()
                .any(|h| h.name.eq_ignore_ascii_case(name))
            {
                return Err(SpecError::SelectorTargetMissing(sel.describe()));
            }
        }
        PositionSelector::Cookie { name } => {
            conflict(spans.iter().any(|s| matches!(s, MarkerSpan::Header { .. })))?;
            let hit = req.headers.iter().any(|h| {
                h.name.eq_ignore_ascii_case("cookie")
                    && h.value
                        .split(';')
                        .any(|c| c.trim().split('=').next() == Some(name.as_str()))
            });
            if !hit {
                return Err(SpecError::SelectorTargetMissing(sel.describe()));
            }
        }
        PositionSelector::Form { name } => {
            conflict(spans.iter().any(|s| matches!(s, MarkerSpan::Body { .. })))?;
            let text = String::from_utf8_lossy(&req.body);
            let hit = text
                .split('&')
                .any(|p| p.split('=').next() == Some(name.as_str()));
            if !hit {
                return Err(SpecError::SelectorTargetMissing(sel.describe()));
            }
        }
        PositionSelector::Json { pointer } => {
            conflict(spans.iter().any(|s| matches!(s, MarkerSpan::Body { .. })))?;
            let text = String::from_utf8_lossy(&req.body);
            let value: serde_json::Value = serde_json::from_str(text.trim())
                .map_err(|e| SpecError::SelectorTargetMissing(format!("json parse: {e}")))?;
            let hit = json_pointer_exists(&value, pointer);
            if !hit {
                return Err(SpecError::SelectorTargetMissing(sel.describe()));
            }
        }
        PositionSelector::PathSegment { index } => {
            conflict(spans.iter().any(|s| matches!(s, MarkerSpan::Target { .. })))?;
            let path = req.target.split('?').next().unwrap_or("");
            let logical = path
                .split('/')
                .enumerate()
                .filter(|(i, s)| !(*i == 0 && s.is_empty()))
                .count();
            if *index >= logical {
                return Err(SpecError::SelectorTargetMissing(sel.describe()));
            }
        }
        PositionSelector::Multipart { field } => {
            conflict(spans.iter().any(|s| matches!(s, MarkerSpan::Body { .. })))?;
            let ct = req
                .headers
                .iter()
                .find(|h| h.name.eq_ignore_ascii_case("content-type"))
                .map(|h| h.value.clone())
                .ok_or_else(|| SpecError::SelectorTargetMissing(sel.describe()))?;
            let boundary = ct
                .split(';')
                .map(str::trim)
                .find_map(|p| p.strip_prefix("boundary="))
                .map(|b| b.trim_matches('"').to_string())
                .ok_or_else(|| {
                    SpecError::SelectorTargetMissing(format!("{}: no boundary", sel.describe()))
                })?;
            let body_text = String::from_utf8_lossy(&req.body);
            if !body_text.contains(&format!("name=\"{field}\"")) {
                return Err(SpecError::SelectorTargetMissing(sel.describe()));
            }
            if !body_text.contains(&format!("--{boundary}")) {
                return Err(SpecError::SelectorTargetMissing(format!(
                    "{}: boundary not found in body",
                    sel.describe()
                )));
            }
        }
    }
    Ok(())
}

/// pointer 存在性检查（对象键/数组下标链）。
fn json_pointer_exists(value: &serde_json::Value, pointer: &str) -> bool {
    if !pointer.starts_with('/') {
        return false;
    }
    let mut cur = value;
    for token in pointer[1..].split('/') {
        let token = token.replace("~1", "/").replace("~0", "~");
        cur = match cur {
            serde_json::Value::Object(map) => match map.get(&token) {
                Some(v) => v,
                None => return false,
            },
            serde_json::Value::Array(arr) => {
                match token.parse::<usize>().ok().and_then(|i| arr.get(i)) {
                    Some(v) => v,
                    None => return false,
                }
            }
            _ => return false,
        };
    }
    true
}

/// 组合估算（D5 判据 2：估算 = 无截断时实际派发数；u64 溢出饱和拒）。
pub fn estimate(effective: &[Vec<String>], mode: FuzzMode) -> Result<u64, SpecError> {
    match mode {
        FuzzMode::Single => {
            let mut total = 0u64;
            for list in effective {
                total = total
                    .checked_add(list.len() as u64)
                    .ok_or(SpecError::EstimateOverflow)?;
            }
            Ok(total)
        }
        FuzzMode::Cartesian => {
            let mut total = 1u64;
            for list in effective {
                total = total
                    .checked_mul(list.len() as u64)
                    .ok_or(SpecError::EstimateOverflow)?;
            }
            Ok(total)
        }
        FuzzMode::Zip => {
            let first = effective
                .first()
                .map(|l| l.len())
                .ok_or(SpecError::ZipLengthMismatch)?;
            if effective.iter().any(|l| l.len() != first) {
                return Err(SpecError::ZipLengthMismatch);
            }
            Ok(first as u64)
        }
    }
}

/// 位置组合展开器（惰性；ordinal → 位置赋值向量；判据 2 的派发事实源）。
pub struct CombinationIter<'a> {
    effective: &'a [Vec<String>],
    mode: FuzzMode,
    total: u64,
    next: u64,
}

impl<'a> CombinationIter<'a> {
    pub fn new(effective: &'a [Vec<String>], mode: FuzzMode) -> Result<Self, SpecError> {
        Ok(Self {
            total: estimate(effective, mode)?,
            effective,
            mode,
            next: 0,
        })
    }

    pub fn total(&self) -> u64 {
        self.total
    }

    /// 位置赋值向量（None = 保模板原值）。
    pub fn assignment(&self, ordinal: u64) -> Vec<Option<String>> {
        let n = self.effective.len();
        let mut out: Vec<Option<String>> = vec![None; n];
        match self.mode {
            FuzzMode::Single => {
                // ordinal = Σ_{i<pos} len_i + idx：定位活跃位置。
                let mut k = ordinal;
                for (i, list) in self.effective.iter().enumerate() {
                    let l = list.len() as u64;
                    if k < l {
                        out[i] = Some(list[k as usize].clone());
                        return out;
                    }
                    k -= l;
                }
                out
            }
            FuzzMode::Cartesian => {
                // mixed-radix 计数（ordinal = 进制串）。
                let mut k = ordinal;
                for i in (0..n).rev() {
                    let l = self.effective[i].len() as u64;
                    out[i] = Some(self.effective[i][(k % l) as usize].clone());
                    k /= l;
                }
                out
            }
            FuzzMode::Zip => {
                for (i, list) in self.effective.iter().enumerate() {
                    out[i] = Some(list[ordinal as usize].clone());
                }
                out
            }
        }
    }
}

impl<'a> Iterator for CombinationIter<'a> {
    type Item = Vec<Option<String>>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.next >= self.total {
            return None;
        }
        let a = self.assignment(self.next);
        self.next += 1;
        Some(a)
    }
}

/// 位置组合 payload 值串（payload_sha256 对账锚输入：`[v1,v2,…]` 序列化）。
pub fn payload_vector_json(assignments: &[Option<String>]) -> String {
    let vals: Vec<&str> = assignments
        .iter()
        .map(|a| a.as_deref().unwrap_or(""))
        .collect();
    serde_json::to_string(&vals).unwrap_or_else(|_| "[]".into())
}

/// 便于头构造（模板组装面）。
pub fn header(name: &str, value: &str) -> HeaderRecord {
    HeaderRecord::new(name, value)
}
