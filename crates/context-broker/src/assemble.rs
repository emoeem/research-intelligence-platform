//! 装配全流程与 AssembledContext 输出契约（设计稿 CONTEXT-BROKER-DESIGN
//! §5.3 选择策略、§6 清单落库、§9 输出契约、§10 Broker 签名）。
//!
//! 流程（一次 `assemble`）：
//! 1. 意图分类（[`crate::intent::classify`]，纯函数）+ 意图审计事件旁路发射；
//! 2. 预算分配（[`crate::policy::BudgetPolicy::allocate`]，七分项）；
//! 3. 全源候选收集（源错误降级为 warning，不炸装配，设计稿 §9.2 第 4 条）；
//! 4. 五权重打分 + 一票否决（[`crate::score`]）；
//! 5. 分桶选择：L0 规则封顶内全量（§5.3 第 1 条）→ L1 目录摘要单项
//!    （kind=doc/included=catalog-card，词表内最近项，设计稿 §9.1 l1 计数
//!    口径）→ L2 知识摘要与工具 brief 按 score 降序装入（同 content_hash
//!    只装一次；装不下先降级 ref-only，仍不够才驱逐，§5.3 第 3/5/6 条）→
//!    Artifact 引用挂 l3_refs/citations（ref-only，正文永不预装，§5.3 第 7 条）；
//! 6. citations[] 与 l2 摘要分离（§9.2 第 2 条：下游只允许引用 citations）；
//! 7. selection_hash = 所选集合指纹（不含 id/时间戳，同输入一致可对账）；
//! 8. 契约校验（§9.2 第 5 条）：序列化后估算 tokens 超总量即装配失败
//!    （fail fast，不允许静默超预算）；
//! 9. 三表落库（只追加：清单头 + 条目 + ASSEMBLED 事件）。
//!
//! `expand`（§9.2 第 1 条 + §5.3 第 4 条）：L3 按需展开记账，写 EXPANDED
//! 事件并从 l3_reserved 扣账返回增量 tokens；额度不足时拒绝并返回剩余额度
//! 与替代建议（读摘要）。已用额度由 EXPANDED 事件推导（只追加语义，无
//! totals_json 回写）。

use crate::cache::{BrokerCache, CacheValidation, TTL_SCHEMA};
use crate::error::ContextBrokerError;
use crate::estimate::est_tokens;
use crate::intent::{classify, emit_intent_event, IntentConfig};
use crate::manifest::{ItemRow, ManifestRow, ManifestStore, RefEventRow};
use crate::policy::BudgetPolicy;
use crate::score::{candidate_est_tokens, score_candidate, ScoreWeights};
use crate::source::{Candidate, ContextSource};
use crate::{ContentRef, ContextItemKind, ContextLayer, InclusionForm, RefEvent};
use audit_engine::AuditEngine;
use serde::Serialize;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

/// 装配逻辑版本（失效触发器之一，设计稿 §7.2）。
pub const BROKER_VERSION: &str = "0.1.0";

/// AssembledContext 结构版本（设计稿 §9.1）。
pub const CONTEXT_SCHEMA_VERSION: &str = "1";

/// ref-only 降级形态与 Artifact 引用的预算成本（tokens；设计稿 §5.3
/// 「引用同样占极小额度的 ref 预算」的落面常数）。
pub const REF_ONLY_COST: u64 = 8;

/// 单源候选收集上限（预算闸前的源级分页上限）。
const SOURCE_CANDIDATE_CAP: usize = 64;

/// l3 引用（Artifact）装载上限（极小额度的数量面）。
const L3_REF_CAP: usize = 10;

/// 装配请求（设计稿 §10 AssembleRequest 落面）。
#[derive(Debug, Clone)]
pub struct AssembleRequest {
    /// 关联任务 id。
    pub task_id: String,
    /// 任务文本（意图分类输入，「kingdee HTTP 检查」类）。
    pub text: String,
    /// 审计关联 id（任务-意图-上下文-工具-证据链，设计稿 §6.2）。
    pub correlation_id: String,
    /// 执行者（审计 actor）。
    pub actor: String,
}

/// 装配引擎（设计稿 §10 Broker：sources + policy + intent config + manifest
/// store + audit；cache 面 P4-5 加入）。
pub struct Broker {
    sources: Vec<Box<dyn ContextSource>>,
    policy: BudgetPolicy,
    intent_config: IntentConfig,
    store: ManifestStore,
    audit: Option<AuditEngine>,
    cache: Option<Arc<BrokerCache>>,
}

impl Broker {
    /// 构造（sources 顺序即 L0/L1 计数与候选收集顺序；无缓存位）。
    pub fn new(
        sources: Vec<Box<dyn ContextSource>>,
        policy: BudgetPolicy,
        intent_config: IntentConfig,
        store: ManifestStore,
        audit: Option<AuditEngine>,
    ) -> Self {
        Self {
            sources,
            policy,
            intent_config,
            store,
            audit,
            cache: None,
        }
    }

    /// 挂内容寻址缓存（expand 的源读载荷缓存；设计稿 §7.1「只加速读源与
    /// 摘要生成」，装配结果本身不缓存，§7.3）。
    pub fn with_cache(mut self, cache: Arc<BrokerCache>) -> Self {
        self.cache = Some(cache);
        self
    }

    /// 预算策略只读访问。
    pub fn policy(&self) -> &BudgetPolicy {
        &self.policy
    }

    /// 清单 store 只读访问（对账/测试用）。
    pub fn store(&self) -> &ManifestStore {
        &self.store
    }

    /// 装配（设计稿 §10 `assemble(req) -> AssembledContext`）。
    pub fn assemble(&self, req: &AssembleRequest) -> Result<AssembledContext, ContextBrokerError> {
        if req.task_id.trim().is_empty() {
            return Err(ContextBrokerError::InvalidInput {
                field: "task_id".to_string(),
                reason: "must be a non-empty string".to_string(),
            });
        }
        let intent = classify(&req.text, &self.intent_config);
        if let Some(audit) = &self.audit {
            emit_intent_event(audit, &intent, &req.correlation_id, &req.actor);
        }
        let alloc = self.policy.allocate()?;

        // ---- 候选收集与打分（源错误降级 warning）----
        let mut warnings: Vec<String> = Vec::new();
        let mut scored: Vec<ScoredCandidate> = Vec::new();
        for source in &self.sources {
            match source.candidates(&intent.query, SOURCE_CANDIDATE_CAP) {
                Ok(candidates) => {
                    for candidate in candidates {
                        let metrics = crate::score::CandidateMetrics {
                            quality_grade: candidate.quality_grade.clone(),
                            health: None,
                            recency: None,
                            past_success: None,
                            governance: candidate.governance.clone(),
                        };
                        let result = score_candidate(
                            &candidate,
                            &intent,
                            &metrics,
                            &ScoreWeights::default(),
                        );
                        if result.veto_reason.is_some() {
                            warnings.push(format!(
                                "veto:{}:{}",
                                candidate.item_id,
                                result.veto_reason.unwrap_or_default()
                            ));
                            continue;
                        }
                        scored.push(ScoredCandidate {
                            candidate,
                            score: result.score,
                            signals: result.matched_signals,
                        });
                    }
                }
                Err(e) => warnings.push(format!("source:{} unavailable: {e}", source.id())),
            }
        }
        // 分桶排序：分数降序，同分按 item_id 稳定序。
        scored.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.candidate.item_id.cmp(&b.candidate.item_id))
        });

        let mut installed: Vec<InstalledItem> = Vec::new();
        let mut evicted: Vec<EvictionRecord> = Vec::new();
        let mut seen_content_hash: std::collections::HashSet<String> =
            std::collections::HashSet::new();
        let mut used = ComponentUsage::zero();

        // ---- L0：规则包封顶内全量（永不驱逐；装到封顶为止，超出记 warning）----
        for candidate in scored
            .iter()
            .filter(|s| s.candidate.layer == ContextLayer::L0)
        {
            let est = candidate_est_tokens(&candidate.candidate);
            if used.l0_rules + est > alloc.l0_rules {
                warnings.push(format!(
                    "l0 truncated at budget {}: {} remains uninstalled",
                    alloc.l0_rules, candidate.candidate.item_id
                ));
                break;
            }
            used.l0_rules += est;
            let mut item = InstalledItem::summary(candidate, est);
            item.signals
                .push(format!("rule:{}", item.candidate.item_id));
            installed.push(item);
        }

        // ---- L1：目录摘要单项（counts 供 §9.1 l1 counts 口径）----
        let mut counts_tools = 0usize;
        let mut counts_knowledge = 0usize;
        let mut counts_plugins = 0usize;
        let mut counts_rules = 0usize;
        for s in &scored {
            match s.candidate.kind {
                ContextItemKind::Tool => counts_tools += 1,
                ContextItemKind::Knowledge => counts_knowledge += 1,
                ContextItemKind::Plugin => counts_plugins += 1,
                ContextItemKind::Rule => counts_rules += 1,
                _ => {}
            }
        }
        let source_ids: Vec<&str> = self.sources.iter().map(|s| s.id()).collect();
        let digest = format!(
            "catalog@sources:{};tools={counts_tools};knowledge={counts_knowledge};plugins={counts_plugins};rules={counts_rules}",
            source_ids.join(",")
        );
        let l1_est = est_tokens(&digest).min(alloc.l1_catalog.max(1));
        used.l1_catalog = l1_est;
        installed.push(InstalledItem {
            item_id: "catalog:digest".to_string(),
            layer: ContextLayer::L1,
            candidate: Candidate {
                item_id: "catalog:digest".to_string(),
                kind: ContextItemKind::Doc,
                layer: ContextLayer::L1,
                title: "能力目录摘要".to_string(),
                summary: digest.clone(),
                family: None,
                governance: None,
                quality_grade: None,
                reference: ContentRef {
                    source: "catalog".to_string(),
                    ref_id: digest.clone(),
                    ref_version: None,
                    content_hash: None,
                    uri: None,
                },
                est_tokens: l1_est,
            },
            included: InclusionForm::CatalogCard,
            summary: digest.clone(),
            est: l1_est,
            signals: vec!["l1:catalog-digest".to_string()],
            score: 0.0,
            selection_seq: SELECT_SEQ.fetch_add(1, Ordering::Relaxed),
        });

        // ---- L2 分桶 + L3 引用（Artifact）----
        let mut knowledge_bucket: Vec<ScoredCandidate> = Vec::new();
        let mut tools_bucket: Vec<ScoredCandidate> = Vec::new();
        let mut l3_bucket: Vec<ScoredCandidate> = Vec::new();
        for s in scored {
            match (s.candidate.kind, s.candidate.layer) {
                (ContextItemKind::Knowledge, _) => knowledge_bucket.push(s),
                (ContextItemKind::Tool, _) => tools_bucket.push(s),
                (ContextItemKind::Artifact, _) | (_, ContextLayer::L3) => l3_bucket.push(s),
                (ContextItemKind::Rule, _) => {
                    // L0 段已消费（见上）；此分支防重复入桶。
                }
                (ContextItemKind::Plugin, _) => {
                    warnings.push(format!(
                        "plugin:{} deferred to Phase 6",
                        s.candidate.item_id
                    ));
                }
                (ContextItemKind::Doc, _) | (ContextItemKind::Schema, _) => {
                    // 内容类候选随知识桶取舍（设计稿 §5.3 第 3 条摘要形态）。
                    knowledge_bucket.push(s);
                }
            }
        }

        // 知识桶：score 降序装入，同 content_hash 只装一次，装不下降级
        // ref-only，仍不够才驱逐（设计稿 §5.3 第 3/5/6 条）。
        for s in &knowledge_bucket {
            if let Some(hash) = &s.candidate.reference.content_hash {
                if !seen_content_hash.insert(hash.clone()) {
                    warnings.push(format!("dedup:content_hash={}", &hash[..8.min(hash.len())]));
                    continue;
                }
            }
            install_into_bucket(
                s,
                alloc.l2_knowledge,
                &mut used.l2_knowledge,
                &mut installed,
                &mut evicted,
                "l2_knowledge",
            );
        }
        // 工具桶：装入形态为 brief（源提供时），否则摘要。
        for s in &tools_bucket {
            if let Some(hash) = &s.candidate.reference.content_hash {
                if !seen_content_hash.insert(hash.clone()) {
                    warnings.push(format!("dedup:content_hash={}", &hash[..8.min(hash.len())]));
                    continue;
                }
            }
            install_into_bucket(
                s,
                alloc.l2_tools,
                &mut used.l2_tools,
                &mut installed,
                &mut evicted,
                "l2_tools",
            );
        }

        // Artifact 引用：正文永不预装，ref-only 挂 l3_refs/citations，
        // 计入 l3_reserved 极小额度（设计稿 §5.3 第 7 条）。
        let mut l3_refs: Vec<String> = Vec::new();
        for s in l3_bucket.iter().take(L3_REF_CAP) {
            if used.l3_reserved + REF_ONLY_COST > alloc.l3_reserved {
                warnings.push(format!(
                    "l3 refs capped at budget {}: {} not cited",
                    alloc.l3_reserved, s.candidate.item_id
                ));
                break;
            }
            used.l3_reserved += REF_ONLY_COST;
            let mut item = InstalledItem::ref_only(s, REF_ONLY_COST);
            item.summary = String::new();
            l3_refs.push(s.candidate.reference.ref_id.clone());
            installed.push(item);
        }

        // ---- citations[]（§9.2 第 2 条：权威引用集，与摘要分离）----
        let citations: Vec<Citation> = installed
            .iter()
            .map(|item| Citation {
                item_id: item.item_id.clone(),
                source: item.candidate.reference.source.clone(),
                ref_id: item.candidate.reference.ref_id.clone(),
                version: item.candidate.reference.ref_version.clone(),
                content_hash: item.candidate.reference.content_hash.clone(),
                uri: item.candidate.reference.uri.clone(),
            })
            .collect();

        // ---- selection_hash（同输入一致；不含 id/时间戳）----
        let mut fingerprint: Vec<String> = installed
            .iter()
            .map(|item| {
                format!(
                    "{}|{}|{}|{}|{}|{}",
                    item.candidate.reference.source,
                    item.candidate.reference.ref_id,
                    item.candidate
                        .reference
                        .ref_version
                        .clone()
                        .unwrap_or_default(),
                    item.candidate
                        .reference
                        .content_hash
                        .clone()
                        .unwrap_or_default(),
                    item.included.as_str(),
                    item.est,
                )
            })
            .collect();
        fingerprint.sort();
        let selection_hash = tool_registry::hash::sha256_hex(fingerprint.join("\n").as_bytes());

        // ---- AssembledContext 与契约校验（fail fast，§9.2 第 5 条）----
        let context = AssembledContext {
            schema_version: CONTEXT_SCHEMA_VERSION.to_string(),
            context_id: new_id("ctx", &req.task_id),
            manifest_id: new_id("ctxm", &req.task_id),
            task_id: req.task_id.clone(),
            created_at: domain::time::now_iso(),
            broker_version: BROKER_VERSION.to_string(),
            policy_version: self.policy.policy_version().to_string(),
            intent: IntentSummary {
                status: intent.status.as_str().to_string(),
                families: intent
                    .query
                    .families
                    .iter()
                    .map(|f| f.as_str().to_string())
                    .collect(),
                target_slots: intent
                    .query
                    .target_slots
                    .iter()
                    .map(|s| s.as_str().to_string())
                    .collect(),
                confidence: intent.query.confidence,
                alternatives: intent
                    .alternatives
                    .iter()
                    .map(|f| f.as_str().to_string())
                    .collect(),
            },
            layers: Layers {
                l0: L0Layer {
                    rule_pack: "rip-rules".to_string(),
                    rules_ref: format!("rp-rules:{}#installed", "1.0.0"),
                },
                l1: L1Layer {
                    catalog_digest_ref: digest,
                    counts: CatalogCounts {
                        tools: counts_tools,
                        knowledge: counts_knowledge,
                        plugins: counts_plugins,
                        rules: counts_rules,
                    },
                },
                l2: L2Layer {
                    knowledge_summaries: installed
                        .iter()
                        .filter(|i| {
                            i.layer == ContextLayer::L2
                                && matches!(
                                    i.candidate.kind,
                                    ContextItemKind::Knowledge
                                        | ContextItemKind::Doc
                                        | ContextItemKind::Schema
                                )
                        })
                        .map(|i| KnowledgeSummary {
                            item_id: i.item_id.clone(),
                            ref_id: i.candidate.reference.ref_id.clone(),
                            source: i.candidate.reference.source.clone(),
                            title: i.candidate.title.clone(),
                            grade: i.candidate.quality_grade.clone(),
                            summary: if i.included == InclusionForm::Summary {
                                i.summary.clone()
                            } else {
                                String::new()
                            },
                            content_hash: i.candidate.reference.content_hash.clone(),
                            reason: i.reason(),
                        })
                        .collect(),
                    tool_briefs: installed
                        .iter()
                        .filter(|i| {
                            i.layer == ContextLayer::L2 && i.candidate.kind == ContextItemKind::Tool
                        })
                        .map(|i| ToolBriefRef {
                            item_id: i.item_id.clone(),
                            tool_id: i.candidate.reference.ref_id.clone(),
                            version: i
                                .candidate
                                .reference
                                .ref_version
                                .clone()
                                .unwrap_or_else(|| "current".to_string()),
                            summary_ref: format!(
                                "{}:brief:{}@{}",
                                i.candidate.reference.source,
                                i.candidate.reference.ref_id,
                                i.candidate
                                    .reference
                                    .ref_version
                                    .clone()
                                    .unwrap_or_else(|| "current".to_string())
                            ),
                            summary: if i.included == InclusionForm::Summary {
                                i.summary.clone()
                            } else {
                                String::new()
                            },
                            reason: i.reason(),
                        })
                        .collect(),
                },
                l3_refs: l3_refs.clone(),
            },
            citations,
            budget_report: BudgetReport {
                total_tokens: self.policy.total_tokens(),
                allocated: ComponentUsage::from_alloc(&alloc),
                used,
                evicted: evicted.clone(),
            },
            warnings,
            audit: AuditInfo {
                correlation_id: req.correlation_id.clone(),
            },
        };
        let serialized =
            serde_json::to_string(&context).map_err(|e| ContextBrokerError::Source {
                source: "assemble".to_string(),
                reason: format!("serialize AssembledContext: {e}"),
            })?;
        let inline_est = est_tokens(&serialized);
        if inline_est > self.policy.total_tokens() {
            return Err(ContextBrokerError::InvalidInput {
                field: "budget_report.used".to_string(),
                reason: format!(
                    "serialized AssembledContext needs ~{inline_est} tokens over total budget {}; fail fast (设计稿 §9.2 第 5 条)",
                    self.policy.total_tokens()
                ),
            });
        }

        // ---- 三表落库（只追加）----
        let manifest_row = ManifestRow {
            manifest_id: context.manifest_id.clone(),
            task_id: req.task_id.clone(),
            intent_json: serde_json::json!({
                "status": intent.status.as_str(),
                "confidence": intent.query.confidence,
                "families": intent.query.families.iter().map(|f| f.as_str()).collect::<Vec<_>>(),
                "target_slots": intent.query.target_slots.iter().map(|s| s.as_str()).collect::<Vec<_>>(),
                "task_verbs": intent.query.task_verbs,
                "knowledge_intents": intent.query.knowledge_intents,
                "matched_signals": intent.matched_signals,
                "alternatives": intent
                    .alternatives
                    .iter()
                    .map(|f| f.as_str())
                    .collect::<Vec<_>>(),
            })
            .to_string(),
            policy_version: self.policy.policy_version().to_string(),
            broker_version: BROKER_VERSION.to_string(),
            budget_json: serde_json::to_string(&context.budget_report.allocated)
                .unwrap_or_default(),
            totals_json: serde_json::to_string(&context.budget_report.used).unwrap_or_default(),
            intent_status: intent.status,
            selection_hash: selection_hash.clone(),
            created_at: context.created_at.clone(),
            schema_version: CONTEXT_SCHEMA_VERSION.to_string(),
        };
        self.store.insert_manifest(&manifest_row)?;
        for item in &installed {
            self.store.insert_item(&ItemRow {
                manifest_id: manifest_row.manifest_id.clone(),
                item_id: item.item_id.clone(),
                layer: item.layer,
                kind: item.candidate.kind,
                ref_id: item.candidate.reference.ref_id.clone(),
                ref_version: item.candidate.reference.ref_version.clone(),
                source: item.candidate.reference.source.clone(),
                title: item.candidate.title.clone(),
                reason: item.reason(),
                matched_signals_json: serde_json::to_string(&item.signals).unwrap_or_default(),
                est_tokens: item.est as i64,
                included: item.included,
                content_hash: item.candidate.reference.content_hash.clone(),
            })?;
            self.store.append_ref_event(
                &manifest_row.manifest_id,
                &item.item_id,
                RefEvent::Assembled,
                &serde_json::json!({
                    "included": item.included.as_str(),
                    "est_tokens": item.est,
                })
                .to_string(),
            )?;
        }
        // 装配期溢出候选不写 EVICTED 事件：ref_log 事件只指向已装条目
        // （复合外键），溢出记录在 budget_report.evicted 与 totals_json
        // 快照中；EVICTED 事件留给运行期对已装 L3 展开的收回（P4-5 起使用）。
        Ok(context)
    }

    /// L3 按需展开记账（设计稿 §10 `expand(manifest_id, item_id)`）。
    pub fn expand(&self, manifest_id: &str, item_id: &str) -> Result<Expanded, ContextBrokerError> {
        let row =
            self.store
                .manifest(manifest_id)?
                .ok_or_else(|| ContextBrokerError::InvalidInput {
                    field: "manifest_id".to_string(),
                    reason: format!("manifest {manifest_id:?} not found"),
                })?;
        let items = self.store.items(manifest_id)?;
        let item = items.iter().find(|i| i.item_id == item_id).ok_or_else(|| {
            ContextBrokerError::InvalidInput {
                field: "item_id".to_string(),
                reason: format!("item {item_id:?} not found in {manifest_id:?}"),
            }
        })?;
        let alloc = self.policy.allocate()?;
        let used_l3 = self.store.expanded_tokens(manifest_id)?;
        let remaining = alloc.l3_reserved.saturating_sub(used_l3);

        let source = self
            .sources
            .iter()
            .find(|s| s.id() == item.source)
            .ok_or_else(|| ContextBrokerError::Source {
                source: item.source.clone(),
                reason: format!("no live source for manifest item {item_id:?}"),
            })?;
        let reference = ContentRef {
            source: item.source.clone(),
            ref_id: item.ref_id.clone(),
            ref_version: item.ref_version.clone(),
            content_hash: item.content_hash.clone(),
            uri: None,
        };
        let content = match &self.cache {
            Some(cache) => {
                // 源读载荷缓存（设计稿 §7.1 读时校验 + TTL_SCHEMA 兜底；
                // 触发器 1/7 载体：version + content_hash + broker 分量）。
                let version = item
                    .ref_version
                    .clone()
                    .unwrap_or_else(|| "current".to_string());
                let fingerprint = format!(
                    "broker={};source={}",
                    crate::assemble::BROKER_VERSION,
                    item.source
                );
                let key =
                    BrokerCache::cache_key("load", &item.ref_id, &version, "expand", &fingerprint);
                let validation = CacheValidation {
                    version: version.clone(),
                    content_hash: item.content_hash.clone().unwrap_or_default(),
                    source_fingerprint: fingerprint,
                };
                match cache.get(&key, &validation, self.policy.policy_version(), TTL_SCHEMA) {
                    Ok(Some(payload)) => crate::source::Content {
                        reference: reference.clone(),
                        est_tokens: est_tokens(&payload),
                        body: payload,
                    },
                    _ => {
                        let content = source.load(&reference)?;
                        let _ = cache.put(
                            "load",
                            &item.ref_id,
                            &validation,
                            &content.body,
                            self.policy.policy_version(),
                        );
                        content
                    }
                }
            }
            None => source.load(&reference)?,
        };
        let cost = content.est_tokens;
        if cost > remaining {
            // 设计稿 §5.3 第 4 条：拒绝时返回剩余额度与替代建议（读摘要）。
            return Err(ContextBrokerError::InvalidInput {
                field: "l3_reserved".to_string(),
                reason: format!(
                    "expand needs {cost} tokens, only {remaining} remaining in l3_reserved; fallback: read summary of item {item_id:?}"
                ),
            });
        }
        let granted = cost;
        let remaining_after = remaining - granted;
        self.store.append_ref_event(
            manifest_id,
            item_id,
            RefEvent::Expanded,
            &serde_json::json!({
                "granted_tokens": granted,
                "remaining_after": remaining_after,
                "policy_version": row.policy_version,
            })
            .to_string(),
        )?;
        Ok(Expanded {
            manifest_id: manifest_id.to_string(),
            item_id: item_id.to_string(),
            granted_tokens: granted,
            remaining_l3: remaining_after,
            content,
        })
    }

    /// 清单视图（设计稿 §10 `manifest(manifest_id) -> ManifestView`）。
    pub fn manifest(&self, manifest_id: &str) -> Result<ManifestView, ContextBrokerError> {
        let manifest =
            self.store
                .manifest(manifest_id)?
                .ok_or_else(|| ContextBrokerError::InvalidInput {
                    field: "manifest_id".to_string(),
                    reason: format!("manifest {manifest_id:?} not found"),
                })?;
        let items = self.store.items(manifest_id)?;
        let ref_events = self.store.ref_events(manifest_id)?;
        Ok(ManifestView {
            manifest,
            items,
            ref_events,
        })
    }
}

/// 生成源内稳定 id 前缀的新 id（ctx- / ctxm-；时间戳 + 进程内序号 +
/// 任务熵，sha256 截断，tool_registry::hash 复用）。
fn new_id(prefix: &str, entropy: &str) -> String {
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let seq = SEQ.fetch_add(1, Ordering::Relaxed);
    // 熵源（2026-11-10 修复轮：秒级 now_iso + 进内序号在「同 task 同秒跨
    // 进程」场景 100% 碰撞，第二次 INSERT 撞 UNIQUE 硬失败）。改为：
    // 纳秒时戳（跨秒）+ 进程熵（RandomState 每 RandomState::new 独立种子，
    // 覆盖跨进程与进程重启；std 无 RNG 的标准熵通道）+ pid + 进内序号。
    static PROCESS_SALT: OnceLock<(u64, u64)> = OnceLock::new();
    let (salt_a, salt_b) =
        *PROCESS_SALT.get_or_init(|| (process_entropy_u64(), process_entropy_u64()));
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let material = format!(
        "{nanos}|{salt_a:x}{salt_b:x}|{seq}|{}|{entropy}",
        std::process::id()
    );
    format!(
        "{prefix}-{}",
        &tool_registry::hash::sha256_hex(material.as_bytes())[..24]
    )
}

/// 进程熵抽取（RandomState 种子来自 OS 熵；每次调用独立，std 标准通道）。
fn process_entropy_u64() -> u64 {
    use std::hash::{BuildHasher, Hasher};
    let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
    hasher.write_u64(0x9E37_79B9_7F4A_7C15);
    hasher.finish()
}

/// 打分后的候选（装配内部载具）。
struct ScoredCandidate {
    candidate: Candidate,
    score: f64,
    signals: Vec<String>,
}

/// 待装条目（装配内部载具）。
struct InstalledItem {
    item_id: String,
    layer: ContextLayer,
    candidate: Candidate,
    included: InclusionForm,
    summary: String,
    est: u64,
    signals: Vec<String>,
    score: f64,
    selection_seq: u64,
}

/// 装配期条目序号（零信号退化 reason 的可追溯序号；不进 selection 指纹）。
static SELECT_SEQ: AtomicU64 = AtomicU64::new(1);

impl InstalledItem {
    fn summary(scored: &ScoredCandidate, est: u64) -> Self {
        Self::build(
            scored,
            InclusionForm::Summary,
            scored.candidate.summary.clone(),
            est,
        )
    }

    fn ref_only(scored: &ScoredCandidate, est: u64) -> Self {
        Self::build(scored, InclusionForm::RefOnly, String::new(), est)
    }

    fn build(scored: &ScoredCandidate, included: InclusionForm, summary: String, est: u64) -> Self {
        Self {
            item_id: scored.candidate.item_id.clone(),
            layer: scored.candidate.layer,
            candidate: scored.candidate.clone(),
            included,
            summary,
            est,
            signals: scored.signals.clone(),
            score: scored.score,
            selection_seq: SELECT_SEQ.fetch_add(1, Ordering::Relaxed),
        }
    }

    /// 可解释理由（设计稿 §4.3 matched_signals 原样；零信号退化形态
    /// source-order:<序号>+score:<分>;source:<源>，可追溯选择顺序、打分
    /// 与来源）。
    fn reason(&self) -> String {
        if self.signals.is_empty() {
            format!(
                "source-order:{}+score:{:.3};source:{}",
                self.selection_seq, self.score, self.candidate.reference.source
            )
        } else {
            self.signals.join("; ")
        }
    }
}

/// 桶内安装（score 降序调用方保证）：fits 装 Summary；装不下降级 RefOnly；
/// 仍不够驱逐（设计稿 §5.3 第 3/5/6 条）。
fn install_into_bucket(
    scored: &ScoredCandidate,
    budget: u64,
    used: &mut u64,
    installed: &mut Vec<InstalledItem>,
    evicted: &mut Vec<EvictionRecord>,
    bucket: &str,
) {
    let est = candidate_est_tokens(&scored.candidate);
    if *used + est <= budget {
        *used += est;
        installed.push(InstalledItem::summary(scored, est));
        return;
    }
    // 降级链：ref-only 不删 item，只改 included 形态（设计稿 §5.3 第 6 条）。
    if *used + REF_ONLY_COST <= budget {
        *used += REF_ONLY_COST;
        let mut item = InstalledItem::ref_only(scored, REF_ONLY_COST);
        item.summary = String::new();
        installed.push(item);
        return;
    }
    evicted.push(EvictionRecord {
        item_id: scored.candidate.item_id.clone(),
        source: scored.candidate.reference.source.clone(),
        kind: scored.candidate.kind.as_str().to_string(),
        score: scored.score,
        reason: format!("budget:{bucket} exhausted at {used}/{budget}"),
    });
}

/// 意图摘要（设计稿 §9.1 intent 块）。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct IntentSummary {
    /// ok | ambiguous。
    pub status: String,
    /// 候选族。
    pub families: Vec<String>,
    /// 目标槽位。
    pub target_slots: Vec<String>,
    /// 置信度。
    pub confidence: f32,
    /// ambiguous 时的候选族。
    pub alternatives: Vec<String>,
}

/// 层结构（设计稿 §9.1 layers 块）。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Layers {
    /// L0：规则包引用（正文不内联，rules_ref 指向包）。
    pub l0: L0Layer,
    /// L1：目录摘要与计数。
    pub l1: L1Layer,
    /// L2：知识摘要与工具 brief。
    pub l2: L2Layer,
    /// L3：按需展开引用（初始为 Artifact 引用集，expand 后增长）。
    pub l3_refs: Vec<String>,
}

/// L0 层。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct L0Layer {
    /// 规则包 id。
    pub rule_pack: String,
    /// 规则引用。
    pub rules_ref: String,
}

/// L1 层。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct L1Layer {
    /// 目录摘要引用。
    pub catalog_digest_ref: String,
    /// 目录计数（目录全量，非所选）。
    pub counts: CatalogCounts,
}

/// 目录计数。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct CatalogCounts {
    /// 工具候选数。
    pub tools: usize,
    /// 知识候选数。
    pub knowledge: usize,
    /// 插件候选数（Phase 6 前恒 0）。
    pub plugins: usize,
    /// 规则候选数。
    pub rules: usize,
}

/// L2 层。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct L2Layer {
    /// 知识摘要（摘要与 citations 分离，§9.2 第 2 条）。
    pub knowledge_summaries: Vec<KnowledgeSummary>,
    /// 工具 brief。
    pub tool_briefs: Vec<ToolBriefRef>,
}

/// 知识摘要条目。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct KnowledgeSummary {
    /// 清单条目 id。
    pub item_id: String,
    /// 来源对象 id。
    pub ref_id: String,
    /// 源 id。
    pub source: String,
    /// 标题。
    pub title: String,
    /// 质量等级（缺证 None）。
    pub grade: Option<String>,
    /// 摘要（ref-only 形态为空串）。
    pub summary: String,
    /// 内容哈希。
    pub content_hash: Option<String>,
    /// 可解释理由（matched_signals 原样）。
    pub reason: String,
}

/// 工具 brief 条目。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ToolBriefRef {
    /// 清单条目 id。
    pub item_id: String,
    /// 工具 id。
    pub tool_id: String,
    /// 版本。
    pub version: String,
    /// brief 引用（{source}:brief:{tool_id}@{version}）。
    pub summary_ref: String,
    /// brief 文本（ref-only 形态为空串）。
    pub summary: String,
    /// 可解释理由。
    pub reason: String,
}

/// 权威引用（设计稿 §9.2 第 2 条：source/version/content_hash/uri 齐全字段）。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Citation {
    /// 清单条目 id。
    pub item_id: String,
    /// 源 id。
    pub source: String,
    /// 来源对象 id。
    pub ref_id: String,
    /// 版本（缺省指 current）。
    pub version: Option<String>,
    /// 内容 sha256 hex。
    pub content_hash: Option<String>,
    /// 来源 URI。
    pub uri: Option<String>,
}

/// 分项占用（allocated 与 used 共用形状）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct ComponentUsage {
    /// 规则包。
    pub l0_rules: u64,
    /// 目录卡片。
    pub l1_catalog: u64,
    /// 知识摘要。
    pub l2_knowledge: u64,
    /// 工具 brief。
    pub l2_tools: u64,
    /// L3 动态池（used 含 Artifact 引用与 expand 已授额度）。
    pub l3_reserved: u64,
    /// 记忆（Phase 7 接入）。
    pub memory_history: u64,
    /// 输出保留（恒 0）。
    pub reserve_output: u64,
}

impl ComponentUsage {
    fn zero() -> Self {
        Self {
            l0_rules: 0,
            l1_catalog: 0,
            l2_knowledge: 0,
            l2_tools: 0,
            l3_reserved: 0,
            memory_history: 0,
            reserve_output: 0,
        }
    }

    fn from_alloc(alloc: &crate::policy::BudgetAllocation) -> Self {
        Self {
            l0_rules: alloc.l0_rules,
            l1_catalog: alloc.l1_catalog,
            l2_knowledge: alloc.l2_knowledge,
            l2_tools: alloc.l2_tools,
            l3_reserved: alloc.l3_reserved,
            memory_history: alloc.memory_history,
            reserve_output: alloc.reserve_output,
        }
    }
}

/// 驱逐记录（设计稿 §9.1 budget_report.evicted[]）。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EvictionRecord {
    /// 被逐条目 id。
    pub item_id: String,
    /// 源 id。
    pub source: String,
    /// 种类。
    pub kind: String,
    /// 打分。
    pub score: f64,
    /// 原因（budget:<bucket> exhausted）。
    pub reason: String,
}

/// 预算报告（设计稿 §9.1 budget_report 块）。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BudgetReport {
    /// 总预算。
    pub total_tokens: u64,
    /// 分项分配。
    pub allocated: ComponentUsage,
    /// 分项实际占用。
    pub used: ComponentUsage,
    /// 驱逐记录（score 降序处理的溢出尾部，低分先逐）。
    pub evicted: Vec<EvictionRecord>,
}

/// 审计信息。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AuditInfo {
    /// 关联 id。
    pub correlation_id: String,
}

/// 装配输出契约（设计稿 §9.1；序列化后整体过预算闸，§9.2 第 5 条）。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AssembledContext {
    /// 结构版本。
    pub schema_version: String,
    /// 装配 id（ctx-<hex>，重装配出新 id）。
    pub context_id: String,
    /// 清单 id。
    pub manifest_id: String,
    /// 任务 id。
    pub task_id: String,
    /// 签发时刻。
    pub created_at: String,
    /// 装配逻辑版本。
    pub broker_version: String,
    /// 策略版本。
    pub policy_version: String,
    /// 意图摘要。
    pub intent: IntentSummary,
    /// 层结构。
    pub layers: Layers,
    /// 权威引用集。
    pub citations: Vec<Citation>,
    /// 预算报告。
    pub budget_report: BudgetReport,
    /// 警告（源不可用/去重/截断等，空结果合法性的说明面）。
    pub warnings: Vec<String>,
    /// 审计信息。
    pub audit: AuditInfo,
}

/// expand 结果（设计稿 §9.2 第 1 条：返回增量 tokens）。
#[derive(Debug, Clone, PartialEq)]
pub struct Expanded {
    /// 清单 id。
    pub manifest_id: String,
    /// 条目 id。
    pub item_id: String,
    /// 本次授予的增量 tokens（从 l3_reserved 扣账）。
    pub granted_tokens: u64,
    /// 扣账后剩余额度。
    pub remaining_l3: u64,
    /// 展开内容。
    pub content: crate::source::Content,
}

/// 清单视图（设计稿 §10 manifest() 返回）。
#[derive(Debug, Clone, PartialEq)]
pub struct ManifestView {
    /// 清单头。
    pub manifest: ManifestRow,
    /// 条目集。
    pub items: Vec<ItemRow>,
    /// 引用事件集。
    pub ref_events: Vec<RefEventRow>,
}
