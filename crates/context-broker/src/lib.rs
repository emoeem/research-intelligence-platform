//! RIP Context Broker（Phase 4，计划 §14:786-794；实施蓝本
//! docs/phase0/CONTEXT-BROKER-DESIGN.md，下称「设计稿」）。
//!
//! Agent 与知识库、工具库之间的唯一上下文装配层：四级加载（L0 到 L3）
//! 加预算控制加清单留痕（设计稿结论先行段）。验收三条（计划 §14:794）：
//! 不默认把整个工具目录或知识库正文塞入上下文；可解释选择了哪些知识和
//! 工具、为什么选择；工具输出通过 Artifact 引用进入上下文。
//!
//! 本批（P4-4 = 设计稿 §11 M4.3）落装配全流程与输出契约：
//!
//! - [`assemble`]：Broker（sources + policy + intent config + manifest
//!   store + audit）与 assemble/expand/manifest 三公开方法；选择、同
//!   content_hash 去重、降级链与驱逐、budget_report、selection_hash、
//!   AssembledContext 契约校验（fail fast）
//! - [`manifest`]：三表只追加持久化（清单头/条目/引用事件）与 expand
//!   记账推导
//! - [`sources::artifact`]：Artifact 只读引用源（正文永不预装）
//!
//! P4-1（M4.0）：[`policy`] / [`estimate`] / [`error`]；P4-2（M4.1）：
//! [`source`] 端口与 rulepack/registry/plugin 源；P4-3（M4.2）：[`intent`]
//! / [`score`] / knowledge/rtis 源。cache 面按 PHASE4-DESIGN §6 批次表在
//! P4-5 落位。
#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod assemble;
pub mod cache;
pub mod error;
pub mod estimate;
pub mod intent;
pub mod manifest;
pub mod policy;
pub mod score;
pub mod source;
pub mod sources;

pub use assemble::{
    AssembleRequest, AssembledContext, AuditInfo, Broker, BudgetReport, CatalogCounts, Citation,
    ComponentUsage, EvictionRecord, Expanded, IntentSummary, KnowledgeSummary, L0Layer, L1Layer,
    L2Layer, Layers, ManifestView, ToolBriefRef, BROKER_VERSION, CONTEXT_SCHEMA_VERSION,
    REF_ONLY_COST,
};
pub use cache::{
    BrokerCache, CacheEntry, CacheStats, CacheValidation, TTL_CATALOG, TTL_KNOWLEDGE, TTL_RTIS,
    TTL_SCHEMA,
};
pub use domain::context::{
    ContentRef, ContextItemKind, ContextLayer, InclusionForm, IntentQuery, IntentStatus, RefEvent,
    TargetSlot, ToolFamily,
};
pub use error::ContextBrokerError;
pub use estimate::{count_chars, est_tokens, CharCounts};
pub use intent::{
    classify, detect_slots, emit_intent_event, ClassifyResult, FamilyRule, IntentConfig,
    KnowledgeIntentRule, AMBIGUOUS_CONFIDENCE_THRESHOLD, INTENT_ACTION,
};
pub use manifest::{ItemRow, ManifestRow, ManifestStore, RefEventRow};
pub use policy::{borrow_allowed, BudgetAllocation, BudgetComponent, BudgetPolicy};
pub use score::{
    candidate_est_tokens, family_of, score_candidate, vetoes, CandidateMetrics, ScoreComponents,
    ScoreWeights, W_FAMILY_MATCH, W_HEALTH, W_PAST_SUCCESS, W_QUALITY, W_RECENCY,
};
pub use source::{Candidate, Content, ContextSource};
pub use sources::artifact::{ArtifactSource, ARTIFACT_SOURCE_ID};
pub use sources::knowledge::{KnowledgeSource, KNOWLEDGE_SOURCE_ID};
pub use sources::plugin::PluginSource;
pub use sources::registry::{
    generate_brief, ManifestIndex, ManifestMeta, RegistrySource, ToolBrief, ToolCard,
    BRIEF_TOKEN_SOFT_CAP, CARD_TOKEN_SOFT_CAP,
};
pub use sources::rtis::{RtisSource, RtisStatus, RTIS_SOURCE_ID, RTIS_TIMEOUT};
pub use sources::rulepack::{RuleEntry, RulePack, RulepackSource};
