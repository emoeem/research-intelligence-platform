//! 意图识别纯函数（设计稿 CONTEXT-BROKER-DESIGN §4.1/§4.2；五步全确定性，
//! 零 IO，文末仅保留一个旁路审计发射出口）。
//!
//! 流程（设计稿 §4.2 五步的本批落面）：
//! 1. 归一化：文本小写化，词元化复用 resource-engine `tokenize`（eri 正则
//!    口径，tokenizer.rs:25），本模块不重写分词；
//! 2. 槽位抽取：词典与确定性行内检测（域名/IP/URL/APK/哈希样串/代码仓/
//!    文档），证据串进 matched_signals；
//! 3. 意图分类：触发词表来自版本化配置 rulepacks/rip-rules/intent-families.toml
//!    （设计稿 §4.2 第 3 条「映射表是版本化配置而非硬编码」）；ASCII 触发词
//!    走词元级匹配（含复数容忍），非 ASCII 触发词走子串匹配（CJK 不入
//!    tokenizer 词元集）；
//! 4. 低置信回退：confidence 低于 [`AMBIGUOUS_CONFIDENCE_THRESHOLD`] 标记
//!    ambiguous 并附 top3 候选族，不静默猜（设计稿 §4.2 第 4 条）；
//! 5. 审计：意图事件带 correlation_id 走 audit-engine（§4.2 第 5 条；
//!    [`emit_intent_event`] 用 emit_best_effort 旁路，写失败不影响装配，
//!    设计稿 §2 第 7 条）。

use crate::error::ContextBrokerError;
use crate::{IntentQuery, IntentStatus, TargetSlot, ToolFamily};
use audit_engine::AuditEngine;
use domain::audit::AuditEvent;
use resource_engine::tokenizer::{keyword_hits, tokenize};

/// 低置信阈值（设计稿 §4.2 第 4 条「默认 0.5」；低于即 ambiguous）。
pub const AMBIGUOUS_CONFIDENCE_THRESHOLD: f32 = 0.5;

/// 源 id 之外的意图审计动作名（audit_event.action）。
pub const INTENT_ACTION: &str = "context.intent.classify";

/// 族分类规则（intent-families.toml [[family]]）。
#[derive(Debug, Clone, PartialEq)]
pub struct FamilyRule {
    /// 族 id（计划 §4.2 原文名，与 ToolFamily::parse 兼容）。
    pub id: String,
    /// 触发词（ASCII 走词元匹配，非 ASCII 走子串匹配）。
    pub triggers: Vec<String>,
    /// 示例（文档面，不参与匹配）。
    pub examples: Vec<String>,
}

/// 知识意图规则（intent-families.toml [[knowledge_intent]]；设计稿 §4.1
/// knowledge_intents 词表：检索/评估/来源核验/反知识核查）。
#[derive(Debug, Clone, PartialEq)]
pub struct KnowledgeIntentRule {
    /// 意图 id（kb-search/kb-evaluate/source-verify/negative-check）。
    pub id: String,
    /// 触发词。
    pub triggers: Vec<String>,
}

/// 版本化意图配置（intent-families.toml 解析结果）。
#[derive(Debug, Clone, PartialEq)]
pub struct IntentConfig {
    /// 配置 id（intent-families）。
    pub config_id: String,
    /// 配置版本（版本化配置的失效信号）。
    pub config_version: String,
    /// 族分类规则（恰为计划 §4.2 十三族，解析时逐条 ToolFamily::parse 校验）。
    pub families: Vec<FamilyRule>,
    /// 知识意图规则。
    pub knowledge_intents: Vec<KnowledgeIntentRule>,
    /// 任务动词表（槽位与置信度输入）。
    pub task_verbs: Vec<String>,
}

impl IntentConfig {
    /// 从 intent-families.toml 文本解析（toml 1.x 成熟实现；族 id 不可解析
    /// 即拒绝，防拼写漂移）。
    pub fn from_toml(text: &str) -> Result<Self, ContextBrokerError> {
        let value: toml::Value = toml::from_str(text).map_err(|e| ContextBrokerError::Source {
            source: "intent-config".to_string(),
            reason: format!("intent-families.toml is not valid TOML: {e}"),
        })?;
        let str_field = |v: &toml::Value, name: &str| -> Option<String> {
            v.get(name)
                .and_then(toml::Value::as_str)
                .map(str::to_string)
        };
        let config_id =
            str_field(&value, "config_id").ok_or_else(|| ContextBrokerError::Source {
                source: "intent-config".to_string(),
                reason: "missing config_id".to_string(),
            })?;
        let config_version =
            str_field(&value, "config_version").ok_or_else(|| ContextBrokerError::Source {
                source: "intent-config".to_string(),
                reason: "missing config_version".to_string(),
            })?;

        let mut families = Vec::new();
        for (idx, entry) in value
            .get("family")
            .and_then(toml::Value::as_array)
            .ok_or_else(|| ContextBrokerError::Source {
                source: "intent-config".to_string(),
                reason: "missing [[family]] array".to_string(),
            })?
            .iter()
            .enumerate()
        {
            let id = str_field(entry, "id").ok_or_else(|| ContextBrokerError::Source {
                source: "intent-config".to_string(),
                reason: format!("family[{idx}] missing id"),
            })?;
            // 族 id 必须可解析为 domain 十三族（拼写漂移即拒绝）。
            ToolFamily::parse(&id).map_err(|_| ContextBrokerError::Source {
                source: "intent-config".to_string(),
                reason: format!("family[{idx}] id {id:?} is not a plan §4.2 family"),
            })?;
            let triggers = string_list(entry, "triggers", idx, "family")?;
            let examples = entry
                .get("examples")
                .and_then(toml::Value::as_array)
                .map(|arr| {
                    arr.iter()
                        .filter_map(toml::Value::as_str)
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default();
            families.push(FamilyRule {
                id,
                triggers,
                examples,
            });
        }

        let mut knowledge_intents = Vec::new();
        if let Some(arr) = value
            .get("knowledge_intent")
            .and_then(toml::Value::as_array)
        {
            for (idx, entry) in arr.iter().enumerate() {
                knowledge_intents.push(KnowledgeIntentRule {
                    id: str_field(entry, "id").ok_or_else(|| ContextBrokerError::Source {
                        source: "intent-config".to_string(),
                        reason: format!("knowledge_intent[{idx}] missing id"),
                    })?,
                    triggers: string_list(entry, "triggers", idx, "knowledge_intent")?,
                });
            }
        }

        let task_verbs = value
            .get("task_verbs")
            .and_then(|v| v.get("verbs"))
            .and_then(toml::Value::as_array)
            .map(|arr| {
                arr.iter()
                    .filter_map(toml::Value::as_str)
                    .map(str::to_string)
                    .collect()
            })
            .ok_or_else(|| ContextBrokerError::Source {
                source: "intent-config".to_string(),
                reason: "missing [task_verbs] verbs".to_string(),
            })?;

        Ok(Self {
            config_id,
            config_version,
            families,
            knowledge_intents,
            task_verbs,
        })
    }

    /// 从规则包目录加载（rulepacks/rip-rules/intent-families.toml）。
    pub fn from_dir(dir: &std::path::Path) -> Result<Self, ContextBrokerError> {
        let path = dir.join("intent-families.toml");
        let text = std::fs::read_to_string(&path).map_err(|e| ContextBrokerError::Source {
            source: "intent-config".to_string(),
            reason: format!("read {}: {e}", path.display()),
        })?;
        Self::from_toml(&text)
    }
}

fn string_list(
    entry: &toml::Value,
    name: &str,
    idx: usize,
    section: &str,
) -> Result<Vec<String>, ContextBrokerError> {
    entry
        .get(name)
        .and_then(toml::Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(toml::Value::as_str)
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .filter(|list| !list.is_empty())
        .ok_or_else(|| ContextBrokerError::Source {
            source: "intent-config".to_string(),
            reason: format!("{section}[{idx}] missing non-empty {name}"),
        })
}

/// 槽位检测证据（槽位 + 命中子串）。
#[derive(Debug, Clone, PartialEq)]
pub struct SlotEvidence {
    /// 槽位。
    pub slot: TargetSlot,
    /// 命中证据（原文片段，进 matched_signals）。
    pub evidence: String,
}

/// 分类结果（IntentQuery 快照 + 状态 + 候选族 + 信号；设计稿 §4.1/§4.2）。
#[derive(Debug, Clone, PartialEq)]
pub struct ClassifyResult {
    /// 结构化意图（confidence 与 families 已落）。
    pub query: IntentQuery,
    /// 意图状态（ok / ambiguous）。
    pub status: IntentStatus,
    /// ambiguous 时的 top3 候选族（ok 时为空）。
    pub alternatives: Vec<ToolFamily>,
    /// 命中信号（family:/slot:/verb:/kint: 前缀，装配后原样入 Manifest
    /// item matched_signals，设计稿 §4.3）。
    pub matched_signals: Vec<String>,
}

/// ASCII 触发词词元匹配（含复数容忍，对齐 keyword_hits 口径）。
fn trigger_matches(trigger: &str, text_lower: &str, tokens: &[String]) -> bool {
    if trigger.is_ascii() {
        keyword_hits(&[trigger.to_string()], tokens).len() == 1
            || tokens.iter().any(|t| t.contains(trigger))
    } else {
        text_lower.contains(trigger)
    }
}

/// 已知顶级域集合（域名词元判定用；冻结小集合，防词元误报，如
/// rip.kb.keyword 被误判为域名）。
const TLD_SET: &[&str] = &[
    "com", "net", "org", "io", "cn", "dev", "app", "xyz", "me", "info", "co", "ai", "cloud", "edu",
    "gov", "top", "site", "online", "cc", "tv", "biz", "pro", "ru", "de", "uk", "fr", "jp", "kr",
    "qq",
];

/// 文档扩展名集合（Document 槽位判定）。
const DOC_EXTS: &[&str] = &[
    "pdf", "doc", "docx", "xls", "xlsx", "ppt", "pptx", "md", "txt",
];

fn is_ip_token(token: &str) -> bool {
    let parts: Vec<&str> = token.split('.').collect();
    parts.len() == 4
        && parts.iter().all(|p| {
            !p.is_empty()
                && p.len() <= 3
                && p.chars().all(|c| c.is_ascii_digit())
                && p.parse::<u16>().map(|v| v <= 255).unwrap_or(false)
        })
}

fn is_domain_token(token: &str) -> bool {
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() < 2 || is_ip_token(token) {
        return false;
    }
    let tld = parts[parts.len() - 1];
    TLD_SET.contains(&tld)
        && parts[..parts.len() - 1]
            .iter()
            .all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'))
}

fn is_hex_hash(token: &str) -> bool {
    (token.len() == 32 || token.len() == 40 || token.len() == 64)
        && token.chars().all(|c| c.is_ascii_hexdigit())
}

/// 槽位抽取（设计稿 §4.2 第 2 条；确定性检测，证据串可回放）。
pub fn detect_slots(text_lower: &str) -> Vec<SlotEvidence> {
    let mut slots: Vec<SlotEvidence> = Vec::new();
    let push = |slot: TargetSlot, evidence: String, slots: &mut Vec<SlotEvidence>| {
        if !slots.iter().any(|s| s.slot == slot) {
            slots.push(SlotEvidence { slot, evidence });
        }
    };
    if text_lower.contains("http://") || text_lower.contains("https://") {
        let marker = text_lower
            .split_whitespace()
            .find(|w| w.starts_with("http://") || w.starts_with("https://"))
            .unwrap_or("http(s)://");
        push(TargetSlot::Url, marker.to_string(), &mut slots);
    }
    for token in tokenize(text_lower) {
        if token.ends_with(".apk") {
            push(TargetSlot::Apk, token.clone(), &mut slots);
        } else if is_ip_token(&token) {
            push(TargetSlot::Ip, token.clone(), &mut slots);
        } else if is_domain_token(&token) {
            push(TargetSlot::Domain, token.clone(), &mut slots);
        } else if is_hex_hash(&token) {
            push(TargetSlot::Binary, token.clone(), &mut slots);
        } else if DOC_EXTS
            .iter()
            .any(|ext| token.ends_with(&format!(".{ext}")))
        {
            push(TargetSlot::Document, token.clone(), &mut slots);
        }
    }
    if text_lower.contains("gh:")
        || text_lower.contains("github.com/")
        || text_lower.contains("gitee.com/")
    {
        push(TargetSlot::CodeRepo, "repo-ref".to_string(), &mut slots);
    }
    slots
}

/// 意图分类（设计稿 §4.2 第 1 到 4 步；纯函数，零 IO）。
pub fn classify(text: &str, config: &IntentConfig) -> ClassifyResult {
    let text_normalized: String = text
        .to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let tokens = tokenize(&text_normalized);

    // 槽位。
    let slots = detect_slots(&text_normalized);

    // 任务动词（词表子串/词元命中）。
    let verbs: Vec<String> = config
        .task_verbs
        .iter()
        .filter(|v| trigger_matches(v, &text_normalized, &tokens))
        .cloned()
        .collect();

    // 族分类（命中数降序，同数按 id 升序）。
    let mut family_hits: Vec<(FamilyRule, Vec<String>)> = Vec::new();
    for rule in &config.families {
        let matched: Vec<String> = rule
            .triggers
            .iter()
            .filter(|t| trigger_matches(t, &text_normalized, &tokens))
            .cloned()
            .collect();
        if !matched.is_empty() {
            family_hits.push((rule.clone(), matched));
        }
    }
    family_hits.sort_by(|a, b| b.1.len().cmp(&a.1.len()).then_with(|| a.0.id.cmp(&b.0.id)));

    // 知识意图。
    let mut kint_hits: Vec<(KnowledgeIntentRule, Vec<String>)> = Vec::new();
    for rule in &config.knowledge_intents {
        let matched: Vec<String> = rule
            .triggers
            .iter()
            .filter(|t| trigger_matches(t, &text_normalized, &tokens))
            .cloned()
            .collect();
        if !matched.is_empty() {
            kint_hits.push((rule.clone(), matched));
        }
    }

    // 信号留痕（顺序确定：family -> slot -> verb -> kint）。
    let mut matched_signals = Vec::new();
    let mut families: Vec<ToolFamily> = Vec::new();
    for (rule, hits) in &family_hits {
        if let Ok(family) = ToolFamily::parse(&rule.id) {
            families.push(family);
        }
        for hit in hits {
            matched_signals.push(format!("family:{}:{}", rule.id, hit));
        }
    }
    for slot in &slots {
        matched_signals.push(format!("slot:{}:{}", slot.slot.as_str(), slot.evidence));
    }
    for verb in &verbs {
        matched_signals.push(format!("verb:{verb}"));
    }
    for (rule, hits) in &kint_hits {
        for hit in hits {
            matched_signals.push(format!("kint:{}:{}", rule.id, hit));
        }
    }

    // 置信度（规则打分落面：族 0.4/槽 0.2/动词 0.2/知识意图 0.2，各封顶 2 次
    // 命中；校准随 P4-6 token 校准同批复核，ROADMAP §5.5 风险 1 同款纪律）。
    let conf_family = 0.4 * family_hits.len().min(2) as f32;
    let conf_slot = 0.2 * slots.len().min(2) as f32;
    let conf_verb = 0.2 * verbs.len().min(2) as f32;
    let conf_kint = 0.2 * kint_hits.len().min(2) as f32;
    let confidence = (conf_family + conf_slot + conf_verb + conf_kint).min(1.0);

    let status = if confidence < AMBIGUOUS_CONFIDENCE_THRESHOLD {
        IntentStatus::Ambiguous
    } else {
        IntentStatus::Ok
    };
    let alternatives = if status == IntentStatus::Ambiguous {
        family_hits
            .iter()
            .take(3)
            .filter_map(|(rule, _)| ToolFamily::parse(&rule.id).ok())
            .collect()
    } else {
        Vec::new()
    };

    ClassifyResult {
        query: IntentQuery {
            text_normalized,
            families,
            knowledge_intents: kint_hits.iter().map(|(r, _)| r.id.clone()).collect(),
            target_slots: slots.iter().map(|s| s.slot).collect(),
            task_verbs: verbs,
            constraints: Vec::new(),
            confidence,
        },
        status,
        alternatives,
        matched_signals,
    }
}

/// 意图审计事件（设计稿 §4.2 第 5 条；emit_best_effort 旁路，写失败不
/// 影响装配并返回 false，设计稿 §2 第 7 条）。返回是否落库。
pub fn emit_intent_event(
    engine: &AuditEngine,
    result: &ClassifyResult,
    correlation_id: &str,
    trace_id: Option<&str>,
    actor: &str,
) -> bool {
    let detail = serde_json::json!({
        "status": result.status.as_str(),
        "confidence": result.query.confidence,
        "families": result
            .query
            .families
            .iter()
            .map(|f| f.as_str())
            .collect::<Vec<_>>(),
        "target_slots": result
            .query
            .target_slots
            .iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>(),
        "task_verbs": result.query.task_verbs,
        "knowledge_intents": result.query.knowledge_intents,
        "alternatives": result
            .alternatives
            .iter()
            .map(|f| f.as_str())
            .collect::<Vec<_>>(),
        "matched_signals": result.matched_signals,
    })
    .to_string();
    engine.emit_best_effort(AuditEvent {
        ts: domain::time::now_iso(),
        actor: actor.to_string(),
        action: INTENT_ACTION.to_string(),
        entity_type: Some("intent".to_string()),
        entity_id: None,
        correlation_id: Some(correlation_id.to_string()),
        trace_id: trace_id.map(str::to_string),
        detail: Some(detail),
        permission_level: Some("T0".to_string()),
        source_system: Some("context-broker".to_string()),
        source_pk: None,
    })
}
