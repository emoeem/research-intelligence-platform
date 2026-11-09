//! L1 目录源：直连已交付的 tool-registry + 三级工具说明生成器
//! （设计稿 §8 三级定义与 §10 sources/registry.rs；P4-0 决议 2 与决议 3）。
//!
//! 取数路径（P4-0 决议 3 的批内定稿）：
//!
//! - 库面（Registry 公开接口 discover/query/schemas，P4-0 决议 2 直连）：
//!   tool_id、name、lifecycle、governance、permission_level、input_schema、
//!   output_schema、current_version 与 L3 full 层的 Schema 全文；
//! - 文件面（manifests/tools/ 与 containerfiles/*/ 双份登记数据的 JSON 本体）：
//!   description、family、context.summary、network.mode。Manifest 全文不入库
//!   （tool-registry/src/catalog.rs:1-5 头注），此二类字段属文件面登记数据，
//!   由 [`ManifestIndex`] 进程内索引，不改 Registry 登记面（决议 3：登记面
//!   零改动）。
//!
//! 三级说明（设计稿 §8 表）：tier1 card 约 30 tokens（L1 用）；tier2 brief
//! 约 120 tokens 软上限，生成时超限即压缩（L2 用，本 crate 生成，决议 3）；
//! tier3 full = input/output JSON Schema 全文，经 `load`/`tool_schema` 按需
//! 展开（L3 用，expand 记账在 P4-4）。

use crate::cache::{BrokerCache, CacheValidation, TTL_SCHEMA};
use crate::error::ContextBrokerError;
use crate::estimate::est_tokens;
use crate::source::{Candidate, Content, ContextSource};
use crate::{ContentRef, ContextItemKind, ContextLayer, IntentQuery};
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tool_registry::catalog::{ToolDetail, ToolSummary};
use tool_registry::{Registry, RegistryError};

/// 源 id（Manifest item source 词表成员）。
pub const REGISTRY_SOURCE_ID: &str = "registry";

/// tier2 brief 软上限（tokens；设计稿 §8「每条约 120 tokens，生成时超限即压缩」）。
pub const BRIEF_TOKEN_SOFT_CAP: u64 = 120;

/// tier1 card 软上限（tokens；设计稿 §8「每条约 30 tokens」加标点与状态段余量）。
pub const CARD_TOKEN_SOFT_CAP: u64 = 45;

/// 目录可见性（设计稿 §4.3 一票否决过滤的目录面：治理轴 QUARANTINED/
/// REJECTED/ARCHIVED 不可见；lifecycle 不在 ENABLED 活跃段的不进候选）。
fn catalog_visible(lifecycle: &str, governance: &str) -> bool {
    lifecycle == "ENABLED" && governance == "ACTIVE"
}

/// 文件面登记元数据（description/family/context.summary/network.mode）。
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ManifestMeta {
    /// 工具族标签（manifest family 原值，如 knowledge/document）。
    pub family: String,
    /// 一句话描述（manifest description）。
    pub description: String,
    /// context.summary（设计稿 §8 tier2 首选摘要源；仅 8 份 Manifest 携带）。
    pub summary: Option<String>,
    /// network.mode（限制行的数据源）。
    pub network_mode: Option<String>,
}

/// 文件面登记数据索引（tool_id 到元数据；from_dirs 扫描 JSON 双份之一）。
#[derive(Debug, Clone, Default)]
pub struct ManifestIndex {
    entries: HashMap<String, ManifestMeta>,
}

impl ManifestIndex {
    /// 扫描目录集合内的 *.json Manifest 建索引（重复 id 即拒绝，登记面
    /// 唯一性口径与 tool 表 PK 一致）。
    pub fn from_dirs(dirs: &[PathBuf]) -> Result<Self, ContextBrokerError> {
        let mut entries = HashMap::new();
        for dir in dirs {
            let reader = std::fs::read_dir(dir).map_err(|e| ContextBrokerError::Source {
                source: REGISTRY_SOURCE_ID.to_string(),
                reason: format!("read dir {}: {e}", dir.display()),
            })?;
            for entry in reader {
                let entry = entry.map_err(|e| ContextBrokerError::Source {
                    source: REGISTRY_SOURCE_ID.to_string(),
                    reason: format!("dir entry {}: {e}", dir.display()),
                })?;
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) != Some("json") {
                    continue;
                }
                let text =
                    std::fs::read_to_string(&path).map_err(|e| ContextBrokerError::Source {
                        source: REGISTRY_SOURCE_ID.to_string(),
                        reason: format!("read {}: {e}", path.display()),
                    })?;
                let doc: Value =
                    serde_json::from_str(&text).map_err(|e| ContextBrokerError::Source {
                        source: REGISTRY_SOURCE_ID.to_string(),
                        reason: format!("parse {}: {e}", path.display()),
                    })?;
                let tool_id = doc
                    .get("id")
                    .and_then(Value::as_str)
                    .ok_or_else(|| ContextBrokerError::Source {
                        source: REGISTRY_SOURCE_ID.to_string(),
                        reason: format!("{} missing id field", path.display()),
                    })?
                    .to_string();
                let meta = ManifestMeta {
                    family: doc
                        .get("family")
                        .and_then(Value::as_str)
                        .unwrap_or("unknown")
                        .to_string(),
                    description: doc
                        .get("description")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string(),
                    summary: doc
                        .get("context")
                        .and_then(|c| c.get("summary"))
                        .and_then(Value::as_str)
                        .map(str::to_string),
                    network_mode: doc
                        .get("network")
                        .and_then(|n| n.get("mode"))
                        .and_then(Value::as_str)
                        .map(str::to_string),
                };
                if entries.insert(tool_id.clone(), meta).is_some() {
                    return Err(ContextBrokerError::Source {
                        source: REGISTRY_SOURCE_ID.to_string(),
                        reason: format!("duplicate manifest id {tool_id:?}"),
                    });
                }
            }
        }
        Ok(Self { entries })
    }

    /// 取某工具的文件面元数据。
    pub fn get(&self, tool_id: &str) -> Option<&ManifestMeta> {
        self.entries.get(tool_id)
    }

    /// 已索引工具数。
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// 是否为空。
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// tier1 目录卡片（设计稿 §8 tier1 行：id、名称、族标签、一句话状态）。
#[derive(Debug, Clone, PartialEq)]
pub struct ToolCard {
    /// 工具 id。
    pub tool_id: String,
    /// 名称。
    pub name: String,
    /// 族标签（manifest family 原值；缺索引为 unknown）。
    pub family: String,
    /// 状态行（lifecycle/governance/permission/version）。
    pub status_line: String,
    /// 单行卡片全文（L1 装配形态）。
    pub line: String,
    /// 卡片 token 估算。
    pub est_tokens: u64,
}

/// tier2 工具 brief（设计稿 §8 tier2 行；生成落位本 crate，P4-0 决议 3）。
#[derive(Debug, Clone, PartialEq)]
pub struct ToolBrief {
    /// 工具 id。
    pub tool_id: String,
    /// 版本（current 或显式请求版本）。
    pub version: String,
    /// 摘要（context.summary 优先，缺省 description）。
    pub summary: String,
    /// 输入必填键名列表（input_schema.required）。
    pub required_input: Vec<String>,
    /// 输出顶层键名列表（output_schema.properties 键，字典序）。
    pub output_keys: Vec<String>,
    /// 已知限制一行（网络模式 + 权限分级）。
    pub limitation: String,
    /// 组装后的 brief 全文（压缩后形态，L2 装配直接取用）。
    pub text: String,
    /// brief token 估算（不超过 BRIEF_TOKEN_SOFT_CAP）。
    pub est_tokens: u64,
    /// 生成时是否触发压缩（超限即压缩的留痕）。
    pub compressed: bool,
}

/// 组装 brief 全文（纯函数内聚，压缩步进的基底；限制行为空时整段省略）。
fn compose_brief(
    tool_id: &str,
    version: &str,
    summary: &str,
    required: &[String],
    out_keys: &[String],
    limitation: &str,
) -> String {
    let required_text = if required.is_empty() {
        "(无必填)".to_string()
    } else {
        required.join(", ")
    };
    let output_text = if out_keys.is_empty() {
        "(未声明)".to_string()
    } else {
        out_keys.join(", ")
    };
    let limitation_text = if limitation.is_empty() {
        String::new()
    } else {
        format!("｜限制: {limitation}")
    };
    format!("{tool_id} v{version}｜{summary}｜输入必填: {required_text}｜输出: {output_text}{limitation_text}")
}

/// tier2 brief 生成器（纯函数；设计稿 §8：约 120 tokens 软上限，生成时
/// 超限即压缩。压缩步进确定序：先键名列表收敛到 8 键（附省略计数，摘要
/// 是信息主位不动），仍超则截摘要（截断补省略号，找最长可容纳前缀），仍超
/// 则列表收敛到 4 键，仍超则去限制行，最终兜底为全文按字符前缀截断）。
pub fn generate_brief(
    tool_id: &str,
    version: &str,
    summary: &str,
    required_input: &[String],
    output_keys: &[String],
    limitation: &str,
) -> ToolBrief {
    let compose = |sum: &str, req: &[String], keys: &[String], limit: &str| {
        compose_brief(tool_id, version, sum, req, keys, limit)
    };
    let mut compressed = false;
    let mut text = compose(summary, required_input, output_keys, limitation);
    if est_tokens(&text) <= BRIEF_TOKEN_SOFT_CAP {
        return finish_brief(
            tool_id,
            version,
            BriefParts {
                summary,
                required: required_input,
                keys: output_keys,
                limitation,
                compressed,
            },
            text,
        );
    }
    compressed = true;

    // 步进 1：键名列表收敛到 8 键（附省略计数），摘要保持全长。
    let req8 = truncate_keys(required_input, 8);
    let keys8 = truncate_keys(output_keys, 8);
    text = compose(summary, &req8, &keys8, limitation);
    if est_tokens(&text) <= BRIEF_TOKEN_SOFT_CAP {
        return finish_brief(
            tool_id,
            version,
            BriefParts {
                summary,
                required: &req8,
                keys: &keys8,
                limitation,
                compressed,
            },
            text,
        );
    }

    // 步进 2：截摘要（截断必带省略号；找最长可容纳前缀，从全长向下）。
    let chars: Vec<char> = summary.chars().collect();
    let mut fitted_summary = String::new();
    for take in (1..chars.len()).rev() {
        let prefix: String = chars[..take].iter().collect();
        let with_ellipsis = format!("{prefix}\u{2026}");
        let ellipsis_text = compose(&with_ellipsis, &req8, &keys8, limitation);
        if est_tokens(&ellipsis_text) <= BRIEF_TOKEN_SOFT_CAP {
            fitted_summary = with_ellipsis;
            text = ellipsis_text;
            break;
        }
    }
    if fitted_summary.is_empty() {
        // 摘要连一个字符都放不下（键与 id 吃满预算）：置空摘要再继续收敛。
        text = compose("", &req8, &keys8, limitation);
    }
    if est_tokens(&text) <= BRIEF_TOKEN_SOFT_CAP && !fitted_summary.is_empty() {
        return finish_brief(
            tool_id,
            version,
            BriefParts {
                summary: &fitted_summary,
                required: &req8,
                keys: &keys8,
                limitation,
                compressed,
            },
            text,
        );
    }

    // 步进 3：键名列表收敛到 4 键。
    let req4 = truncate_keys(required_input, 4);
    let keys4 = truncate_keys(output_keys, 4);
    text = compose(&fitted_summary, &req4, &keys4, limitation);
    if est_tokens(&text) <= BRIEF_TOKEN_SOFT_CAP {
        return finish_brief(
            tool_id,
            version,
            BriefParts {
                summary: &fitted_summary,
                required: &req4,
                keys: &keys4,
                limitation,
                compressed,
            },
            text,
        );
    }

    // 步进 4：去限制行。
    text = compose(&fitted_summary, &req8, &keys8, "");
    if est_tokens(&text) <= BRIEF_TOKEN_SOFT_CAP {
        return finish_brief(
            tool_id,
            version,
            BriefParts {
                summary: &fitted_summary,
                required: &req8,
                keys: &keys8,
                limitation: "",
                compressed,
            },
            text,
        );
    }

    // 兜底：全文按字符前缀截断（病态超长 id 场景）。
    let all: Vec<char> = text.chars().collect();
    for take in (0..=all.len()).rev() {
        let candidate: String = all[..take].iter().collect();
        if est_tokens(&candidate) <= BRIEF_TOKEN_SOFT_CAP {
            text = candidate;
            break;
        }
    }
    finish_brief(
        tool_id,
        version,
        BriefParts {
            summary: &fitted_summary,
            required: &req8,
            keys: &keys8,
            limitation: "",
            compressed,
        },
        text,
    )
}

/// 键名列表收敛（保留前 keep 个键，超出附省略计数）。
fn truncate_keys(keys: &[String], keep: usize) -> Vec<String> {
    if keys.len() <= keep {
        return keys.to_vec();
    }
    let mut out: Vec<String> = keys[..keep].to_vec();
    out.push(format!("等 {} 键", keys.len()));
    out
}

/// brief 收尾载荷（拆出避免八参函数；clippy too_many_arguments 门禁口径）。
struct BriefParts<'a> {
    summary: &'a str,
    required: &'a [String],
    keys: &'a [String],
    limitation: &'a str,
    compressed: bool,
}

fn finish_brief(tool_id: &str, version: &str, parts: BriefParts<'_>, text: String) -> ToolBrief {
    debug_assert!(
        est_tokens(&text) <= BRIEF_TOKEN_SOFT_CAP || parts.compressed,
        "brief must respect soft cap after compression"
    );
    ToolBrief {
        tool_id: tool_id.to_string(),
        version: version.to_string(),
        summary: parts.summary.to_string(),
        required_input: parts.required.to_vec(),
        output_keys: parts.keys.to_vec(),
        limitation: parts.limitation.to_string(),
        est_tokens: est_tokens(&text),
        text,
        compressed: parts.compressed,
    }
}

/// tier1 card 生成器（纯函数；超上限先截名称再截 id，确定序）。
pub fn generate_card(
    tool_id: &str,
    name: &str,
    family: &str,
    lifecycle: &str,
    governance: &str,
    permission_level: Option<&str>,
    version: &str,
) -> ToolCard {
    let perm = permission_level.unwrap_or("-");
    let line =
        format!("{tool_id} | {name} | {family} | {lifecycle}/{governance} {perm} v{version}");
    let mut line = line;
    if est_tokens(&line) > CARD_TOKEN_SOFT_CAP {
        // 先收敛名称。
        let name_chars: Vec<char> = name.chars().collect();
        for take in (0..=name_chars.len()).rev() {
            let candidate_name: String = name_chars[..take].iter().collect();
            let candidate = format!(
                "{tool_id} | {candidate_name} | {family} | {lifecycle}/{governance} {perm} v{version}"
            );
            if est_tokens(&candidate) <= CARD_TOKEN_SOFT_CAP {
                line = candidate;
                break;
            }
        }
    }
    ToolCard {
        tool_id: tool_id.to_string(),
        name: name.to_string(),
        family: family.to_string(),
        status_line: format!("{lifecycle}/{governance} {perm} v{version}"),
        est_tokens: est_tokens(&line),
        line,
    }
}

/// L1 目录源（直连 Registry 公开接口 + 文件面元数据索引；brief 缓存位
/// 可选，P4-0 决议 3「装配期生成 + broker-cache 缓存」落位）。
pub struct RegistrySource {
    registry: Registry,
    index: ManifestIndex,
    cache: Option<Arc<BrokerCache>>,
    policy_version: String,
}

impl RegistrySource {
    /// 构造（Registry 实例 + 文件面索引；两者皆登记数据面，登记链路零改动）。
    pub fn new(registry: Registry, index: ManifestIndex) -> Self {
        Self {
            registry,
            index,
            cache: None,
            policy_version: "policy-1".to_string(),
        }
    }

    /// 挂 brief 缓存（键含工具 version 与 content_hash；TTL_SCHEMA 兜底，
    /// 设计稿 §7.1/§7.2 触发器 1）。
    pub fn with_cache(
        mut self,
        cache: Arc<BrokerCache>,
        policy_version: impl Into<String>,
    ) -> Self {
        self.cache = Some(cache);
        self.policy_version = policy_version.into();
        self
    }

    /// 登记面全量目录卡片（设计稿 §8 Agent 可见性：不可见工具连目录都不出现，
    /// catalog_visible 过滤；每条经 generate_card 生成）。
    pub fn tool_cards(&self) -> Result<Vec<ToolCard>, ContextBrokerError> {
        let summaries = self.discover_visible()?;
        summaries
            .iter()
            .map(|s| {
                let detail = self.query_detail(&s.tool_id)?;
                let meta = self.index.get(&s.tool_id);
                Ok(generate_card(
                    &s.tool_id,
                    &s.name,
                    meta.map(|m| m.family.as_str()).unwrap_or("unknown"),
                    &s.lifecycle,
                    &s.governance,
                    s.permission_level.as_deref(),
                    detail.current_version.as_deref().unwrap_or("-"),
                ))
            })
            .collect()
    }

    /// 登记面全量 brief（每条经 generate_brief，超限即压缩）。
    pub fn tool_briefs(&self) -> Result<Vec<ToolBrief>, ContextBrokerError> {
        let summaries = self.discover_visible()?;
        summaries
            .iter()
            .map(|s| self.tool_brief(&s.tool_id))
            .collect()
    }

    /// 单工具 brief（tier2；数据面见模块注）。
    pub fn tool_brief(&self, tool_id: &str) -> Result<ToolBrief, ContextBrokerError> {
        let detail = self.query_detail(tool_id)?;
        let row = &detail.tool;
        let meta = self.index.get(tool_id);
        let summary = meta
            .and_then(|m| m.summary.clone())
            .unwrap_or_else(|| meta.map(|m| m.description.clone()).unwrap_or_default());
        let required = schema_required_keys(row.input_schema.as_deref());
        let out_keys = schema_output_keys(row.output_schema.as_deref());
        let limitation = format!(
            "网络 {}，权限 {}",
            meta.and_then(|m| m.network_mode.clone())
                .unwrap_or_else(|| "未声明".to_string()),
            row.permission_level.as_deref().unwrap_or("-")
        );
        let version = detail.current_version.unwrap_or_else(|| "-".to_string());
        Ok(generate_brief(
            tool_id,
            &version,
            &summary,
            &required,
            &out_keys,
            &limitation,
        ))
    }

    /// tier3 full 层：input/output JSON Schema 全文（L3 按需展开，expand
    /// 记账在 P4-4；version None 指 current）。
    pub fn tool_schema(
        &self,
        tool_id: &str,
        version: Option<&str>,
    ) -> Result<(Value, Value), ContextBrokerError> {
        self.registry
            .schemas(tool_id, version)
            .map_err(registry_error)
    }

    fn discover_visible(&self) -> Result<Vec<ToolSummary>, ContextBrokerError> {
        let all = self.registry.discover().map_err(registry_error)?;
        Ok(all
            .into_iter()
            .filter(|s| catalog_visible(&s.lifecycle, &s.governance))
            .collect())
    }

    fn query_detail(&self, tool_id: &str) -> Result<ToolDetail, ContextBrokerError> {
        self.registry.query(tool_id).map_err(registry_error)
    }
}

/// input_schema JSON 文本中的 required 键名列表（缺列或非对象为空表）。
fn schema_required_keys(input_schema: Option<&str>) -> Vec<String> {
    let Some(text) = input_schema else {
        return Vec::new();
    };
    let Ok(value) = serde_json::from_str::<Value>(text) else {
        return Vec::new();
    };
    value
        .get("required")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

/// output_schema JSON 文本中的顶层键名列表（properties 键，字典序；无
/// properties 时回退 required）。
fn schema_output_keys(output_schema: Option<&str>) -> Vec<String> {
    let Some(text) = output_schema else {
        return Vec::new();
    };
    let Ok(value) = serde_json::from_str::<Value>(text) else {
        return Vec::new();
    };
    if let Some(props) = value.get("properties").and_then(Value::as_object) {
        let mut keys: Vec<String> = props.keys().cloned().collect();
        keys.sort();
        return keys;
    }
    value
        .get("required")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn registry_error(e: RegistryError) -> ContextBrokerError {
    ContextBrokerError::Source {
        source: REGISTRY_SOURCE_ID.to_string(),
        reason: e.to_string(),
    }
}

impl ContextSource for RegistrySource {
    fn id(&self) -> &str {
        REGISTRY_SOURCE_ID
    }

    fn brief_for(&self, item_id: &str) -> Option<String> {
        // tier2 brief（P4-0 决议 3：装配期生成 + 缓存，命中即零生成）。
        // 缓存键含 version + content_hash（设计稿 §7.2 触发器 1：工具清单
        // version/digest 变化即重建），TTL_SCHEMA 兜底。
        let detail = self.registry.query(item_id).ok()?;
        let version = detail.current_version.clone()?;
        let content_hash = detail
            .versions
            .last()
            .map(|v| v.content_hash.clone())
            .unwrap_or_default();
        let fingerprint = format!("broker={}", crate::assemble::BROKER_VERSION);
        if let Some(cache) = &self.cache {
            let key = BrokerCache::cache_key("brief", item_id, &version, "v1", &fingerprint);
            let validation = CacheValidation {
                version: version.clone(),
                content_hash: content_hash.clone(),
                source_fingerprint: fingerprint.clone(),
            };
            if let Ok(Some(payload)) =
                cache.get(&key, &validation, &self.policy_version, TTL_SCHEMA)
            {
                return Some(payload);
            }
            if let Ok(brief) = self.tool_brief(item_id) {
                let _ = cache.put(
                    "brief",
                    item_id,
                    &validation,
                    &brief.text,
                    &self.policy_version,
                );
                return Some(brief.text);
            }
            return None;
        }
        self.tool_brief(item_id).ok().map(|b| b.text)
    }

    fn candidates(
        &self,
        _q: &IntentQuery,
        limit: usize,
    ) -> Result<Vec<Candidate>, ContextBrokerError> {
        // 族匹配与打分属 P4-3（M4.2），源只回可见全量（limit 为预算闸前上限）。
        let visible = self.discover_visible()?;
        Ok(visible
            .into_iter()
            .take(limit)
            .map(|s| {
                let meta = self.index.get(&s.tool_id);
                Candidate {
                    item_id: s.tool_id.clone(),
                    kind: ContextItemKind::Tool,
                    layer: ContextLayer::L2,
                    title: s.name.clone(),
                    summary: meta.map(|m| m.description.clone()).unwrap_or_default(),
                    family: meta.map(|m| m.family.clone()),
                    governance: None,
                    quality_grade: None,
                    reference: ContentRef {
                        source: REGISTRY_SOURCE_ID.to_string(),
                        ref_id: s.tool_id.clone(),
                        ref_version: None, // 缺省指 current（设计稿 §6.1）
                        content_hash: None,
                        uri: None,
                    },
                    est_tokens: meta.map(|m| est_tokens(&m.description)).unwrap_or(0),
                }
            })
            .collect())
    }

    fn load(&self, r: &ContentRef) -> Result<Content, ContextBrokerError> {
        if r.source != REGISTRY_SOURCE_ID {
            return Err(ContextBrokerError::Source {
                source: REGISTRY_SOURCE_ID.to_string(),
                reason: format!("ref source {:?} is not registry", r.source),
            });
        }
        let (input, output) = self
            .registry
            .schemas(&r.ref_id, r.ref_version.as_deref())
            .map_err(registry_error)?;
        let detail = self.query_detail(&r.ref_id)?;
        let body = serde_json::json!({
            "tool_id": r.ref_id,
            "version": detail.current_version,
            "input_schema": input,
            "output_schema": output,
        })
        .to_string();
        Ok(Content {
            reference: ContentRef {
                ref_version: detail.current_version,
                ..r.clone()
            },
            est_tokens: est_tokens(&body),
            body,
        })
    }
}
