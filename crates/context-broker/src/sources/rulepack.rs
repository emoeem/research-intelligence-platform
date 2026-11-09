//! L0 规则包源（设计稿 §3 L0 行 + §10 sources/rulepack.rs）。
//!
//! 来源为随仓库分发的规则包目录 `rulepacks/rip-rules/`：rules.toml 结构化
//! 规则（版本化，policy_version 与预算策略同源）+ RULEPACK.md 说明文档。
//! 规则包不计入候选检索（设计稿 §3 L0 行原文）；L0 会话启动装配一次、
//! 封顶内全量、永不驱逐（设计稿 §5.3 第 1 条）。
//!
//! 版本信号（设计稿 §7.2 失效触发器「策略变化」行）：policy_version 变化
//! 使 L0 与预算相关缓存整体失效；pack_version 变化使规则条目缓存失效。

use crate::error::ContextBrokerError;
use crate::estimate::est_tokens;
use crate::source::{Candidate, Content, ContextSource};
use crate::{ContentRef, ContextItemKind, ContextLayer, IntentQuery};
use std::path::Path;

/// 源 id（Manifest item source 词表成员，设计稿 §6.1）。
pub const RULEPACK_SOURCE_ID: &str = "rulepack";

/// 说明文档在引用空间的保留 ref_id（load 返回 RULEPACK.md 全文）。
pub const PACK_DOC_REF_ID: &str = "rp-rules:pack";

/// 规则条目（rules.toml `[[rules]]` 数组表）。
#[derive(Debug, Clone, PartialEq)]
pub struct RuleEntry {
    /// 规则 id（包内唯一，引用空间为 rp-rules:<id>）。
    pub id: String,
    /// 规则标题。
    pub title: String,
    /// 权限分级标记（T0/T1/T2，架构盘点:152 分级口径）。
    pub tier: String,
    /// 规则正文（L0 装配内容与 est_tokens 计算对象）。
    pub body: String,
}

/// 解析后的规则包（rules.toml 头部字段 + 规则数组）。
#[derive(Debug, Clone, PartialEq)]
pub struct RulePack {
    /// 包 id（rip-rules）。
    pub pack_id: String,
    /// 结构版本（rules.toml 自描述）。
    pub schema_version: String,
    /// 包版本（ContentRef.ref_version；条目级失效信号）。
    pub pack_version: String,
    /// 预算策略版本（与 BudgetPolicy 同源；L0 与预算缓存整体失效信号）。
    pub policy_version: String,
    /// 规则条目（L0 装配的候选集）。
    pub rules: Vec<RuleEntry>,
}

/// L0 规则包源（进程内持有解析后的包与说明文档全文）。
#[derive(Debug, Clone)]
pub struct RulepackSource {
    pack: RulePack,
    pack_doc: String,
}

impl RulePack {
    /// 从 rules.toml 文本解析（toml 1.x 成熟实现，不自写解析器）。
    pub fn from_toml(text: &str) -> Result<Self, ContextBrokerError> {
        let value: toml::Value = toml::from_str(text).map_err(|e| ContextBrokerError::Source {
            source: RULEPACK_SOURCE_ID.to_string(),
            reason: format!("rules.toml is not valid TOML: {e}"),
        })?;
        let str_field = |name: &str| -> Result<String, ContextBrokerError> {
            value
                .get(name)
                .and_then(toml::Value::as_str)
                .map(str::to_string)
                .ok_or_else(|| ContextBrokerError::Source {
                    source: RULEPACK_SOURCE_ID.to_string(),
                    reason: format!("rules.toml missing string field {name:?}"),
                })
        };
        let pack_id = str_field("pack_id")?;
        let schema_version = str_field("schema_version")?;
        let pack_version = str_field("pack_version")?;
        let policy_version = str_field("policy_version")?;
        if pack_id.trim().is_empty() || pack_version.trim().is_empty() {
            return Err(ContextBrokerError::Source {
                source: RULEPACK_SOURCE_ID.to_string(),
                reason: "pack_id and pack_version must be non-empty".to_string(),
            });
        }
        let rules_val = value
            .get("rules")
            .ok_or_else(|| ContextBrokerError::Source {
                source: RULEPACK_SOURCE_ID.to_string(),
                reason: "rules.toml missing [[rules]] array".to_string(),
            })?;
        let rules_arr = rules_val
            .as_array()
            .ok_or_else(|| ContextBrokerError::Source {
                source: RULEPACK_SOURCE_ID.to_string(),
                reason: "rules must be an array of tables".to_string(),
            })?;
        let mut rules = Vec::with_capacity(rules_arr.len());
        for (idx, entry) in rules_arr.iter().enumerate() {
            let field = |name: &str| -> Result<String, ContextBrokerError> {
                entry
                    .get(name)
                    .and_then(toml::Value::as_str)
                    .map(str::to_string)
                    .ok_or_else(|| ContextBrokerError::Source {
                        source: RULEPACK_SOURCE_ID.to_string(),
                        reason: format!("rules[{idx}] missing string field {name:?}"),
                    })
            };
            let rule = RuleEntry {
                id: field("id")?,
                title: field("title")?,
                tier: field("tier")?,
                body: field("body")?,
            };
            if rule.id.trim().is_empty() || rule.body.trim().is_empty() {
                return Err(ContextBrokerError::Source {
                    source: RULEPACK_SOURCE_ID.to_string(),
                    reason: format!("rules[{idx}] id and body must be non-empty"),
                });
            }
            rules.push(rule);
        }
        let unique = rules.iter().map(|r| r.id.as_str()).collect::<Vec<_>>();
        {
            let mut sorted = unique.clone();
            sorted.sort();
            sorted.dedup();
            if sorted.len() != unique.len() {
                return Err(ContextBrokerError::Source {
                    source: RULEPACK_SOURCE_ID.to_string(),
                    reason: "duplicate rule id in rules.toml".to_string(),
                });
            }
        }
        Ok(Self {
            pack_id,
            schema_version,
            pack_version,
            policy_version,
            rules,
        })
    }
}

impl RulepackSource {
    /// 从规则包目录加载（rules.toml 必需；RULEPACK.md 必需，供
    /// `rp-rules:pack` 引用展开）。
    pub fn from_dir(dir: &Path) -> Result<Self, ContextBrokerError> {
        let toml_path = dir.join("rules.toml");
        let toml_text =
            std::fs::read_to_string(&toml_path).map_err(|e| ContextBrokerError::Source {
                source: RULEPACK_SOURCE_ID.to_string(),
                reason: format!("read {}: {e}", toml_path.display()),
            })?;
        let pack = RulePack::from_toml(&toml_text)?;
        let doc_path = dir.join("RULEPACK.md");
        let pack_doc =
            std::fs::read_to_string(&doc_path).map_err(|e| ContextBrokerError::Source {
                source: RULEPACK_SOURCE_ID.to_string(),
                reason: format!("read {}: {e}", doc_path.display()),
            })?;
        Ok(Self { pack, pack_doc })
    }

    /// 解析后的规则包（版本信号读取口）。
    pub fn pack(&self) -> &RulePack {
        &self.pack
    }

    fn rule_ref(&self, rule: &RuleEntry) -> ContentRef {
        ContentRef {
            source: RULEPACK_SOURCE_ID.to_string(),
            ref_id: format!("rp-rules:{}", rule.id),
            ref_version: Some(self.pack.pack_version.clone()),
            content_hash: None,
            uri: None,
        }
    }
}

impl ContextSource for RulepackSource {
    fn id(&self) -> &str {
        RULEPACK_SOURCE_ID
    }

    fn candidates(
        &self,
        _q: &IntentQuery,
        limit: usize,
    ) -> Result<Vec<Candidate>, ContextBrokerError> {
        // L0 封顶内全量装配（设计稿 §5.3 第 1 条），limit 为预算闸前的源级上限。
        Ok(self
            .pack
            .rules
            .iter()
            .take(limit)
            .map(|rule| Candidate {
                item_id: format!("rp-rules:{}", rule.id),
                kind: ContextItemKind::Rule,
                layer: ContextLayer::L0,
                title: rule.title.clone(),
                summary: rule.body.clone(),
                family: None,
                governance: None,
                quality_grade: None,
                reference: self.rule_ref(rule),
                est_tokens: est_tokens(&rule.body),
            })
            .collect())
    }

    fn load(&self, r: &ContentRef) -> Result<Content, ContextBrokerError> {
        if r.source != RULEPACK_SOURCE_ID {
            return Err(ContextBrokerError::Source {
                source: RULEPACK_SOURCE_ID.to_string(),
                reason: format!("ref source {:?} is not rulepack", r.source),
            });
        }
        if r.ref_id == PACK_DOC_REF_ID {
            return Ok(Content {
                reference: ContentRef {
                    ref_version: Some(self.pack.pack_version.clone()),
                    ..r.clone()
                },
                body: self.pack_doc.clone(),
                est_tokens: est_tokens(&self.pack_doc),
            });
        }
        let rule = self
            .pack
            .rules
            .iter()
            .find(|rule| format!("rp-rules:{}", rule.id) == r.ref_id)
            .ok_or_else(|| ContextBrokerError::Source {
                source: RULEPACK_SOURCE_ID.to_string(),
                reason: format!("unknown rule ref_id {:?}", r.ref_id),
            })?;
        Ok(Content {
            reference: self.rule_ref(rule),
            body: rule.body.clone(),
            est_tokens: est_tokens(&rule.body),
        })
    }
}
