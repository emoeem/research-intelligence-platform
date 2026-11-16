//! RIP3 Phase 5 知识驱动漏洞验证引擎（RIP-VRVE）——契约与持久化面（P5-2）。
//!
//! 职责与批次路线（Phase 5 补充计划 §5/§7）：
//! - P5-2（本批）：契约 Schema（[`contract`]，schemas/verification/ 四份 v1）
//!   加五态结果词表（domain::verification，与任务状态分离）加三表持久化
//!   （[`store`]，SQLite 020 运营面迁移）加去重指纹（[`fingerprint`]）加
//!   rp-verify CLI；
//! - P5-3：Knowledge Matcher（复用 intel-db 统一检索 + context-broker，
//!   禁止绕过统一检索）；
//! - P5-4：Verification Planner（适用性过滤/去重/缓存决策/预算后端生效）；
//! - P5-5：Provider Adapter（RTIS POC 先行，Nuclei 次之；执行一律经
//!   tool-engine / runtime-gateway）；
//! - P5-6：执行调度与 Activity（暂停/恢复/取消真实传播）；
//! - P5-7：结果归一化五态 + Evidence + Trace 六 ID + RTIS 闭环。
//!
//! 架构纪律（R2 原则 G / 补充计划 §3）：
//! - 不建第二套任务队列/Workflow Runtime/工具注册表；执行派发复用
//!   tool-engine（P5-5 起单向依赖，反方向禁止）；
//! - 不建与 RTIS 重叠的漏洞事实库：`research_plan_item.rtis_poc_id` 只存
//!   RTIS POC 数字 ID 引用，POC 正文/利用细节零复制进 3.0 库与公开仓库；
//!   Finding 事实只经 RTIS 既有接口（rt.py CLI / rtis MCP）读写；
//! - 编排执行态按 P5-0-B3 判定落 SQLite 020 运营面迁移（与 workflow_run/
//!   tool_run 同族），情报核（migrations_pg）零新表；
//! - 五态纪律：CONFIRMED/FALSE_POSITIVE 必须带 result_reason；error_category
//!   在场（执行失败/超时）时禁止写 NOT_REPRODUCED（store 层守卫强制）。

pub mod contract;
pub mod fingerprint;
pub mod matcher;
pub mod planner;
pub mod provider;
pub mod store;

pub use contract::{SCHEMA_DIR, SCHEMA_VERSION};
pub use fingerprint::dedup_fingerprint;
pub use matcher::{
    run_match, KnowledgeChannel, MatchExcluded, MatchHit, MatchQuery, MatchResult, RtisPocChannel,
    UnifiedSearchChannel,
};
pub use planner::{draft_from_match, persist_draft, CacheDecision, PlanBudget, PlanDraft};
pub use provider::{
    execute_nuclei, NucleiProvider, ProviderOutcome, RtisPocProvider, VerificationProvider,
};
pub use store::{PlanRecord, PlanStore, PlanStoreError, PlanSummary, ResultGuard, StoreEvent};
