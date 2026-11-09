//! 五权重打分、一票否决过滤与 matched_signals（设计稿 CONTEXT-BROKER-DESIGN
//! §4.3；纯函数，零 IO）。
//!
//! 打分口径（设计稿 §4.3 原文）：
//! `score = w1*family_match + w2*quality + w3*health + w4*recency + w5*past_success`
//! 默认 w1=0.40、w2=0.25、w3=0.20、w4=0.10、w5=0.05（权重进策略文件可配的
//! 接线随 P4-4 预算策略落位，本批先落常量与结构）。
//!
//! 一票否决（设计稿 §4.3，不可被分数翻盘）：
//! - 治理轴 QUARANTINED / REJECTED / ARCHIVED 不可见（[`vetoes`]；知识源侧
//!   由 search-engine 查询面同款域约束兜底，search-engine/src/query.rs:268）；
//! - past_success：tool_execution 历史 0 行，Phase 4 恒为 0 只留通路
//!   （设计稿 §4.3；ROADMAP §5.5 风险 3）。

use crate::intent::ClassifyResult;
use crate::source::Candidate;
use crate::{est_tokens, ToolFamily};
use resource_engine::tokenizer::{expand_keywords, keyword_hits, tokenize};

/// 默认族匹配权重（设计稿 §4.3 w1）。
pub const W_FAMILY_MATCH: f64 = 0.40;
/// 默认质量权重（w2）。
pub const W_QUALITY: f64 = 0.25;
/// 默认健康权重（w3）。
pub const W_HEALTH: f64 = 0.20;
/// 默认时近权重（w4）。
pub const W_RECENCY: f64 = 0.10;
/// 默认历史成功权重（w5；Phase 4 恒 0 通路）。
pub const W_PAST_SUCCESS: f64 = 0.05;

/// 打分权重（可配；默认值即设计稿 §4.3 常量）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScoreWeights {
    /// 族匹配 w1。
    pub family_match: f64,
    /// 质量 w2。
    pub quality: f64,
    /// 健康 w3。
    pub health: f64,
    /// 时近 w4。
    pub recency: f64,
    /// 历史成功 w5。
    pub past_success: f64,
}

impl Default for ScoreWeights {
    fn default() -> Self {
        Self {
            family_match: W_FAMILY_MATCH,
            quality: W_QUALITY,
            health: W_HEALTH,
            recency: W_RECENCY,
            past_success: W_PAST_SUCCESS,
        }
    }
}

/// 候选质量属性（源可知则填，缺证按文档化中性值计分）。
#[derive(Debug, Clone, PartialEq)]
pub struct CandidateMetrics {
    /// 质量等级（A/B/C/D；None 缺证按 0 计，不给偏高假设）。
    pub quality_grade: Option<String>,
    /// 健康分 0..1（Registry health_state / RTIS verified 等归一；None 缺证
    /// 按中性 0.5 计）。
    pub health: Option<f64>,
    /// 时近分 0..1（越新越高；源无时间轴时 None 按中性 0.5 计）。
    pub recency: Option<f64>,
    /// 历史成功率 0..1（tool_execution 聚合；Phase 4 恒 None → 0 计）。
    pub past_success: Option<f64>,
    /// 治理状态（Some 且越出 ACTIVE 词表段即一票否决）。
    pub governance: Option<String>,
}

impl CandidateMetrics {
    /// 全缺证中性输入（governance 未知不算否决，可见性由源面过滤）。
    pub fn unknown() -> Self {
        Self {
            quality_grade: None,
            health: None,
            recency: None,
            past_success: None,
            governance: None,
        }
    }
}

/// 一票否决判定（设计稿 §4.3 过滤清单；返回拒绝原因）。
pub fn vetoes(governance: Option<&str>) -> Option<&'static str> {
    match governance {
        Some("QUARANTINED") => Some("governance QUARANTINED is invisible (设计稿 §4.3)"),
        Some("REJECTED") => Some("governance REJECTED is invisible (设计稿 §4.3)"),
        Some("ARCHIVED") => Some("governance ARCHIVED is invisible (设计稿 §4.3)"),
        _ => None,
    }
}

/// 打分结果（分数 + 一票否决原因 + matched_signals）。
#[derive(Debug, Clone, PartialEq)]
pub struct Scored {
    /// 综合分 0..1（被否决候选仍给出分数但不进装配，否决原因在 veto_reason）。
    pub score: f64,
    /// 五分项明细（family_match/quality/health/recency/past_success）。
    pub components: ScoreComponents,
    /// 一票否决原因（None = 未否决）。
    pub veto_reason: Option<String>,
    /// 命中信号（family:/kw:/slot:/verb:/kint:，来自 intent 信号 + 词元级
    /// 关键词命中 keyword_hits，设计稿 §4.3 matched_signals[]）。
    pub matched_signals: Vec<String>,
}

/// 五分项明细。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScoreComponents {
    /// 族匹配分（0 或 1）。
    pub family_match: f64,
    /// 质量分（0..1）。
    pub quality: f64,
    /// 健康分（0..1）。
    pub health: f64,
    /// 时近分（0..1）。
    pub recency: f64,
    /// 历史成功分（0..1）。
    pub past_success: f64,
}

/// 族名到 domain 十三族的容错解析（manifest family 原值为小写，如 knowledge）。
pub fn family_of(name: &str) -> Option<ToolFamily> {
    let lowered = name.to_lowercase();
    ToolFamily::ALL
        .into_iter()
        .find(|f| f.as_str().to_lowercase() == lowered)
}

/// 质量等级到 0..1 分（A=1.0 / B=0.75 / C=0.5 / D=0.25 / 缺证=0）。
fn quality_component(grade: Option<&str>) -> f64 {
    match grade.map(str::to_ascii_uppercase).as_deref() {
        Some("A") => 1.0,
        Some("B") => 0.75,
        Some("C") => 0.5,
        Some("D") => 0.25,
        _ => 0.0,
    }
}

fn clamp01(value: Option<f64>, neutral: f64) -> f64 {
    match value {
        Some(v) => v.clamp(0.0, 1.0),
        None => neutral,
    }
}

/// 对单候选打分并生成 matched_signals（设计稿 §4.3；纯函数）。
///
/// 信号两路合并：意图侧信号（intent.matched_signals 中与该候选相关的
/// family 项）+ 候选侧词元级关键词命中（resource-engine keyword_hits：
/// 意图词元经 expand_keywords 扩展后对候选 title+summary 词元求交，
/// 含复数容忍）。
pub fn score_candidate(
    candidate: &Candidate,
    intent: &ClassifyResult,
    metrics: &CandidateMetrics,
    weights: &ScoreWeights,
) -> Scored {
    let veto_reason = vetoes(metrics.governance.as_deref()).map(str::to_string);

    // family_match：候选族解析后落进意图 families 集。
    let family_match = candidate
        .family
        .as_deref()
        .and_then(family_of)
        .map(|f| intent.query.families.contains(&f))
        .map(|hit| if hit { 1.0 } else { 0.0 })
        .unwrap_or(0.0);

    let quality = quality_component(
        metrics
            .quality_grade
            .as_deref()
            .or(candidate.quality_grade.as_deref()),
    );
    let health = clamp01(metrics.health, 0.5);
    let recency = clamp01(metrics.recency, 0.5);
    // past_success：Phase 4 恒 0（None → 0），通路保留（设计稿 §4.3）。
    let past_success = metrics.past_success.unwrap_or(0.0).clamp(0.0, 1.0);

    let components = ScoreComponents {
        family_match,
        quality,
        health,
        recency,
        past_success,
    };
    let score = weights.family_match * components.family_match
        + weights.quality * components.quality
        + weights.health * components.health
        + weights.recency * components.recency
        + weights.past_success * components.past_success;

    // 词元级关键词命中信号（kw: 前缀）。
    let intent_tokens = tokenize(&intent.query.text_normalized);
    let keywords = expand_keywords(&intent_tokens);
    let candidate_tokens = tokenize(&format!("{} {}", candidate.title, candidate.summary));
    let hits = keyword_hits(&keywords, &candidate_tokens);

    let mut matched_signals: Vec<String> = intent
        .matched_signals
        .iter()
        .filter(|s| {
            s.starts_with("family:")
                && candidate
                    .family
                    .as_deref()
                    .map(|f| {
                        s.to_lowercase()
                            .contains(&format!("family:{}", f.to_lowercase()))
                    })
                    .unwrap_or(false)
        })
        .cloned()
        .collect();
    for slot in &intent.query.target_slots {
        matched_signals.push(format!("slot:{}", slot.as_str()));
    }
    for hit in hits {
        matched_signals.push(format!("kw:{hit}"));
    }
    if family_match > 0.0 {
        if let Some(f) = candidate.family.as_deref().and_then(family_of) {
            matched_signals.push(format!("family-match:{}", f.as_str()));
        }
    }

    Scored {
        score,
        components,
        veto_reason,
        matched_signals,
    }
}

/// 候选摘要形态 token 数（装配预算口径的换算入口，P4-4 使用）。
pub fn candidate_est_tokens(candidate: &Candidate) -> u64 {
    if candidate.est_tokens > 0 {
        candidate.est_tokens
    } else {
        est_tokens(&candidate.summary)
    }
}
