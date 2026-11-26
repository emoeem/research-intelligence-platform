//! 验证计划持久化（SQLite 020 运营面：research_plan / research_plan_item /
//! research_plan_event 三表读写与五态守卫）。
//!
//! 纪律：
//! - 只在传入 Pool 上读写（迁移应用由调用方负责：rp-verify dev 库路径 /
//!   测试内存库 `database::migrate(&pool, migrations)`）；
//! - 五态守卫（[`ResultGuard`]）在 store 写面强制：CONFIRMED/FALSE_POSITIVE
//!   必须带 result_reason；error_category 在场时禁止 NOT_REPRODUCED
//!   （执行失败/超时 ≠ 未复现，补充计划 §5.7 纪律硬约束）；
//! - 计划项五态流转写 research_plan_event（append-only，词表封闭）；
//! - 事件 JSON 出站均过契约 Schema 校验（P5-2 完成标准：Schema 有版本号
//!   和契约测试）。

use crate::contract::{
    id_prefix_matches, validate_event_doc, validate_item_doc, validate_plan_doc, SCHEMA_VERSION,
};
use database::rusqlite::OptionalExtension;
use database::Pool;
use domain::time::now_iso;
use domain::verification::{ErrorCategory, PlanStatus, ResultStatus};
use serde::Serialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::fmt;

/// Store 层错误（字符串承载，CLI/HTTP 层映射各自错误形态）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlanStoreError {
    /// 引用的计划/项不存在。
    NotFound(String),
    /// 守卫拒绝（原因/状态/词表违规）。
    GuardRejected(String),
    /// 底层 DB 错误。
    Db(String),
}

impl fmt::Display for PlanStoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PlanStoreError::NotFound(v) => write!(f, "not found: {v}"),
            PlanStoreError::GuardRejected(v) => write!(f, "guard rejected: {v}"),
            PlanStoreError::Db(v) => write!(f, "db error: {v}"),
        }
    }
}

/// 生成 `vp-/vi-/ve-` 形状 ID（时间戳 + 微秒 + 随机尾，符合 Schema pattern）。
fn make_id(prefix: &str) -> String {
    // now_compact_micros = YYYYMMDDHHMMSSffffff（20 字符无分隔，见 domain::time）；
    // date=[0..8] time=[8..14] 微秒前 3 位做毫秒段=[14..17]，微秒后 3 位 hex 化做随机尾。
    let compact = domain::time::now_compact_micros();
    let date = &compact[0..8];
    let time = &compact[8..14];
    let millis = &compact[14..17];
    let tail = &compact[17..20];
    let tail_value: u16 = tail.parse().unwrap_or(0);
    let tail_hex = format!("{:04x}", tail_value % 0xFFFF);
    format!("{prefix}-{date}-{time}-{millis}-{tail_hex}")
}

/// 验证计划记录（verification-plan.v1 形状）。
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct PlanRecord {
    pub schema_version: i64,
    pub plan_id: String,
    pub task_ref: Option<String>,
    pub workspace_id: String,
    pub title: Option<String>,
    pub target_summary: Option<String>,
    pub status: String,
    pub budget: Option<Value>,
    pub created_at: String,
    pub updated_at: String,
}

/// 验证计划项记录（verification-item.v1 形状）。
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct PlanItemRecord {
    pub schema_version: i64,
    pub item_id: String,
    pub plan_id: String,
    pub knowledge_ref: Option<String>,
    pub rtis_poc_id: Option<i64>,
    pub provider_id: String,
    pub provider_version: Option<String>,
    pub target_ref: String,
    pub dedup_fingerprint: String,
    pub applicability_status: String,
    pub applicability_reasons: Vec<String>,
    pub risk_class: String,
    pub request_budget: i64,
    pub timeout_budget_s: i64,
    pub execution_order: i64,
    pub expected_observation: Option<String>,
    /// 七维覆盖矩阵归属（P5-8，022 列；skip 序列化 = 契约 v1 冻结面不动，
    /// 存储面与 coverage 推导消费）。
    #[serde(skip)]
    pub dimension: String,
    pub result_status: Option<String>,
    pub result_reason: Option<String>,
    pub error_category: Option<String>,
    pub tool_run_id: Option<String>,
    pub trace_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// 验证事件记录（verification-event.v1 形状）。
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct StoreEvent {
    pub schema_version: i64,
    pub event_id: String,
    pub plan_id: Option<String>,
    pub item_id: Option<String>,
    pub tool_run_id: Option<String>,
    pub trace_id: Option<String>,
    pub event_type: String,
    pub payload: Option<Value>,
    pub created_at: String,
}

/// 研究假设记录（P5-8，022 research_hypothesis 形状）：假设是研究中间面，
/// 没有证据不能生成 Finding（§5.10）；CONFIRMED 只能经计划项五态判定。
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct HypothesisRecord {
    pub hypothesis_id: String, // hi-YYYYMMDD-HHMMSS-ms-rand
    pub plan_id: String,
    pub dimension: String,
    pub content: String,
    pub source_evidence: String,
    pub association_reason: String,
    pub preconditions: Option<String>,
    pub expected_observation: Option<String>,
    pub negative_control: Option<String>,
    pub suggested_verification: Option<String>,
    pub cost_risk: Option<String>,
    pub stop_condition: String,
    pub round_number: i64,
    pub status: String, // proposed | in_plan | validated | dropped
    pub schema_version: i64,
    pub created_at: String,
    pub updated_at: String,
}

/// 假设创建输入（必填守卫：content/source_evidence/association_reason/
/// stop_condition 非空；无证据不生成）。
#[derive(Debug, Clone)]
pub struct NewHypothesis {
    pub dimension: String,
    pub content: String,
    pub source_evidence: String,
    pub association_reason: String,
    pub preconditions: Option<String>,
    pub expected_observation: Option<String>,
    pub negative_control: Option<String>,
    pub suggested_verification: Option<String>,
    pub cost_risk: Option<String>,
    pub stop_condition: String,
    pub round_number: i64,
}

/// 验证证据档记录（021 research_evidence 形状；payload 为采集时点原始快照）。
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct EvidenceRecord {
    pub evidence_id: String, // re-YYYYMMDD-HHMMSS-ms-rand
    pub item_id: String,
    pub plan_id: String,
    pub tool_run_id: Option<String>,
    pub trace_id: Option<String>,
    pub evidence_type: String, // provider_result | normalized_result | log_excerpt
    /// payload 紧凑序列化 SHA256（64 hex；重复观察识别键）。
    pub content_hash: String,
    pub payload: Value,
    pub created_at: String,
}

/// 计划项新增入参（去重指纹与预算由 Planner/调用方给出；Serialize 供
/// planner 预览出站）。
#[derive(Debug, Clone, Serialize)]
pub struct NewItem {
    pub knowledge_ref: Option<String>,
    pub rtis_poc_id: Option<i64>,
    pub provider_id: String,
    pub provider_version: Option<String>,
    pub target_ref: String,
    pub dedup_fingerprint: String,
    pub applicability_status: String,
    pub applicability_reasons: Vec<String>,
    pub risk_class: String,
    pub request_budget: i64,
    pub timeout_budget_s: i64,
    pub execution_order: i64,
    pub expected_observation: Option<String>,
    /// 七维覆盖归属（缺省 assets；词表外 add_item 守卫拒绝）。
    pub dimension: String,
}

/// 计划列表行（摘要面，不带 items）。
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct PlanSummary {
    pub plan_id: String,
    pub task_ref: Option<String>,
    pub status: String,
    pub title: Option<String>,
    pub item_count: i64,
    pub result_counts: Value,
    pub created_at: String,
    pub updated_at: String,
}

/// 五态结果写面守卫（补充计划 §5.7 硬约束）。
pub struct ResultGuard;

impl ResultGuard {
    /// 校验一次结果写入：返回 (result_status, result_reason, error_category)
    /// 或拒绝原因。
    ///
    /// 规则：
    /// 1. CONFIRMED / FALSE_POSITIVE 必须带非空 reason（§5.7：FALSE_POSITIVE
    ///    必须有明确判断依据；CONFIRMED 必须可追溯判据）；
    /// 2. error_category 在场（执行失败/超时）时禁止 NOT_REPRODUCED——失败
    ///    分类进 INCONCLUSIVE（结果不确定）或保持 NOT_TESTED 等调用方显式
    ///    决定，守卫只负责阻断误判路径；
    /// 3. 词表封闭（ResultStatus/ErrorCategory parse 失败即拒绝）。
    pub fn check(
        status: &str,
        reason: Option<&str>,
        error_category: Option<&str>,
    ) -> Result<(String, Option<String>, Option<String>), PlanStoreError> {
        let parsed = ResultStatus::parse(status)
            .map_err(|e| PlanStoreError::GuardRejected(e.to_string()))?;
        let reason = reason.map(str::trim).filter(|r| !r.is_empty());
        match parsed {
            ResultStatus::Confirmed | ResultStatus::FalsePositive => {
                let Some(reason) = reason else {
                    return Err(PlanStoreError::GuardRejected(format!(
                        "{parsed} requires non-empty result_reason"
                    )));
                };
                Ok((parsed.as_str().to_owned(), Some(reason.to_owned()), None))
            }
            ResultStatus::NotReproduced => {
                if let Some(category) = error_category {
                    let parsed_category = ErrorCategory::parse(category)
                        .map_err(|e| PlanStoreError::GuardRejected(e.to_string()))?;
                    return Err(PlanStoreError::GuardRejected(format!(
                        "error_category={} present: execution failure/timeout must not be recorded as NOT_REPRODUCED (use INCONCLUSIVE or NOT_TESTED)",
                        parsed_category.as_str()
                    )));
                }
                Ok((parsed.as_str().to_owned(), reason.map(str::to_owned), None))
            }
            ResultStatus::NotTested | ResultStatus::Inconclusive => Ok((
                parsed.as_str().to_owned(),
                reason.map(str::to_owned),
                error_category
                    .map(|c| {
                        ErrorCategory::parse(c)
                            .map(|parsed| parsed.as_str().to_owned())
                            .map_err(|e| PlanStoreError::GuardRejected(e.to_string()))
                    })
                    .transpose()?,
            )),
        }
    }
}

/// 计划存储（020 三表读写面）。
#[derive(Clone)]
pub struct PlanStore {
    pool: Pool,
}

impl PlanStore {
    /// 在既有 Pool 上打开（迁移由调用方应用）。
    pub fn new(pool: Pool) -> Self {
        Self { pool }
    }

    /// 新建验证计划（status=draft）。
    pub fn create_plan(
        &self,
        task_ref: Option<&str>,
        workspace_id: &str,
        title: Option<&str>,
        target_summary: Option<&str>,
        budget: Option<&Value>,
    ) -> Result<PlanRecord, PlanStoreError> {
        let plan_id = make_id("vp");
        let now = now_iso();
        let budget_json = budget.map(|b| b.to_string());
        self.pool
            .with_conn(|c| {
                c.execute(
                    "INSERT INTO research_plan (plan_id, task_ref, workspace_id, title,
                        target_summary, status, budget_json, schema_version, created_at, updated_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, 'draft', ?6, ?7, ?8, ?8)",
                    database::rusqlite::params![
                        plan_id,
                        task_ref,
                        workspace_id,
                        title,
                        target_summary,
                        budget_json,
                        SCHEMA_VERSION,
                        now
                    ],
                )
            })
            .map_err(|e| PlanStoreError::Db(e.to_string()))?;
        let record = self
            .get_plan(&plan_id)?
            .ok_or_else(|| PlanStoreError::NotFound(plan_id.clone()))?;
        self.record_event(
            Some(&plan_id),
            None,
            None,
            None,
            "plan_created",
            Some(json!({ "task_ref": task_ref, "title": title })),
        )?;
        Ok(record)
    }

    /// 单条计划（含契约校验的 JSON 形状）。
    pub fn get_plan(&self, plan_id: &str) -> Result<Option<PlanRecord>, PlanStoreError> {
        if !id_prefix_matches(plan_id, "vp-") {
            return Err(PlanStoreError::NotFound(format!("bad plan id: {plan_id}")));
        }
        let row = self
            .pool
            .with_conn(|c| {
                c.query_row(
                    "SELECT plan_id, task_ref, workspace_id, title, target_summary, status,
                            budget_json, created_at, updated_at
                     FROM research_plan WHERE plan_id = ?1",
                    [plan_id],
                    |row| {
                        Ok(PlanRecord {
                            schema_version: SCHEMA_VERSION,
                            plan_id: row.get(0)?,
                            task_ref: row.get(1)?,
                            workspace_id: row.get(2)?,
                            title: row.get(3)?,
                            target_summary: row.get(4)?,
                            status: row.get(5)?,
                            budget: row
                                .get::<_, Option<String>>(6)?
                                .and_then(|s| serde_json::from_str(&s).ok()),
                            created_at: row.get(7)?,
                            updated_at: row.get(8)?,
                        })
                    },
                )
                .map(Some)
                .or_else(|e| match e {
                    database::rusqlite::Error::QueryReturnedNoRows => Ok(None),
                    other => Err(other),
                })
            })
            .map_err(|e| PlanStoreError::Db(e.to_string()))?;
        if let Some(record) = &row {
            let doc =
                serde_json::to_value(record).map_err(|e| PlanStoreError::Db(e.to_string()))?;
            validate_plan_doc(&doc)
                .map_err(|errs| PlanStoreError::GuardRejected(errs.join("; ")))?;
        }
        Ok(row)
    }

    /// 计划列表（摘要：项数 + 五态计数）。
    pub fn list_plans(&self, limit: u32) -> Result<Vec<PlanSummary>, PlanStoreError> {
        let limit = limit.clamp(1, 200);
        self.pool
            .with_conn(|c| {
                let mut stmt = c
                    .prepare(
                        "SELECT p.plan_id, p.task_ref, p.status, p.title, p.created_at, p.updated_at,
                                (SELECT COUNT(*) FROM research_plan_item i WHERE i.plan_id = p.plan_id),
                                (SELECT COUNT(*) FROM research_plan_item i WHERE i.plan_id = p.plan_id AND i.result_status = 'CONFIRMED'),
                                (SELECT COUNT(*) FROM research_plan_item i WHERE i.plan_id = p.plan_id AND i.result_status = 'NOT_REPRODUCED'),
                                (SELECT COUNT(*) FROM research_plan_item i WHERE i.plan_id = p.plan_id AND i.result_status = 'NOT_TESTED'),
                                (SELECT COUNT(*) FROM research_plan_item i WHERE i.plan_id = p.plan_id AND i.result_status = 'INCONCLUSIVE'),
                                (SELECT COUNT(*) FROM research_plan_item i WHERE i.plan_id = p.plan_id AND i.result_status = 'FALSE_POSITIVE')
                         FROM research_plan p ORDER BY p.updated_at DESC, p.plan_id DESC LIMIT ?1",
                    )?;
                let rows = stmt
                    .query_map([limit], |row| {
                        Ok(PlanSummary {
                            plan_id: row.get(0)?,
                            task_ref: row.get(1)?,
                            status: row.get(2)?,
                            title: row.get(3)?,
                            created_at: row.get(4)?,
                            updated_at: row.get(5)?,
                            item_count: row.get(6)?,
                            result_counts: json!({
                                "CONFIRMED": row.get::<_, i64>(7)?,
                                "NOT_REPRODUCED": row.get::<_, i64>(8)?,
                                "NOT_TESTED": row.get::<_, i64>(9)?,
                                "INCONCLUSIVE": row.get::<_, i64>(10)?,
                                "FALSE_POSITIVE": row.get::<_, i64>(11)?,
                            }),
                        })
                    })?;
                rows.collect::<Result<Vec<_>, _>>()
            })
            .map_err(|e| PlanStoreError::Db(e.to_string()))
    }

    /// 计划状态推进（终态不可迁出；事件留痕）。
    pub fn set_plan_status(&self, plan_id: &str, status: &str) -> Result<(), PlanStoreError> {
        let parsed =
            PlanStatus::parse(status).map_err(|e| PlanStoreError::GuardRejected(e.to_string()))?;
        let current = self
            .get_plan(plan_id)?
            .ok_or_else(|| PlanStoreError::NotFound(plan_id.to_owned()))?;
        let current_status = PlanStatus::parse(&current.status)
            .map_err(|e| PlanStoreError::GuardRejected(e.to_string()))?;
        if current_status.is_terminal() {
            return Err(PlanStoreError::GuardRejected(format!(
                "plan {} is terminal ({})",
                plan_id, current.status
            )));
        }
        let now = now_iso();
        let changed = self
            .pool
            .with_conn(|c| {
                c.execute(
                    "UPDATE research_plan SET status = ?2, updated_at = ?3 WHERE plan_id = ?1",
                    database::rusqlite::params![plan_id, parsed.as_str(), now],
                )
            })
            .map_err(|e| PlanStoreError::Db(e.to_string()))?;
        if changed == 0 {
            return Err(PlanStoreError::NotFound(plan_id.to_owned()));
        }
        self.record_event(
            Some(plan_id),
            None,
            None,
            None,
            "plan_status",
            Some(json!({ "from": current.status, "to": parsed.as_str() })),
        )?;
        Ok(())
    }

    /// 计划项新增（applicability 词表与指纹形状守卫 + 事件留痕）。
    pub fn add_item(&self, plan_id: &str, item: NewItem) -> Result<PlanItemRecord, PlanStoreError> {
        let applicability =
            domain::verification::ApplicabilityStatus::parse(&item.applicability_status)
                .map_err(|e| PlanStoreError::GuardRejected(e.to_string()))?;
        let risk = domain::verification::RiskClass::parse(&item.risk_class)
            .map_err(|e| PlanStoreError::GuardRejected(e.to_string()))?;
        if item.dedup_fingerprint.len() != 64
            || !item
                .dedup_fingerprint
                .chars()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
        {
            return Err(PlanStoreError::GuardRejected(
                "dedup_fingerprint must be 64 lowercase hex chars".to_owned(),
            ));
        }
        if matches!(
            applicability,
            domain::verification::ApplicabilityStatus::Excluded
        ) && item.applicability_reasons.is_empty()
        {
            return Err(PlanStoreError::GuardRejected(
                "excluded item requires at least one applicability reason".to_owned(),
            ));
        }
        domain::verification::CoverageDimension::parse(&item.dimension)
            .map_err(|e| PlanStoreError::GuardRejected(e.to_string()))?;
        self.get_plan(plan_id)?
            .ok_or_else(|| PlanStoreError::NotFound(plan_id.to_owned()))?;
        let item_id = make_id("vi");
        let now = now_iso();
        let reasons_json = serde_json::to_string(&item.applicability_reasons)
            .map_err(|e| PlanStoreError::Db(e.to_string()))?;
        self.pool
            .with_conn(|c| {
                c.execute(
                    "INSERT INTO research_plan_item (item_id, plan_id, knowledge_ref, rtis_poc_id,
                        provider_id, provider_version, target_ref, dedup_fingerprint,
                        applicability_status, applicability_reasons_json, risk_class,
                        request_budget, timeout_budget_s, execution_order, expected_observation,
                        dimension, schema_version, created_at, updated_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?18)",
                    database::rusqlite::params![
                        item_id,
                        plan_id,
                        item.knowledge_ref,
                        item.rtis_poc_id,
                        item.provider_id,
                        item.provider_version,
                        item.target_ref,
                        item.dedup_fingerprint,
                        applicability.as_str(),
                        reasons_json,
                        risk.as_str(),
                        item.request_budget,
                        item.timeout_budget_s,
                        item.execution_order,
                        item.expected_observation,
                        item.dimension,
                        SCHEMA_VERSION,
                        now
                    ],
                )
                })
            .map_err(|e| PlanStoreError::Db(e.to_string()))?;
        self.record_event(
            Some(plan_id),
            Some(item_id.as_str()),
            None,
            None,
            "item_planned",
            Some(json!({
                "provider_id": item.provider_id,
                "target_ref": item.target_ref,
                "dedup_fingerprint": item.dedup_fingerprint,
                "applicability_status": applicability.as_str(),
                "dimension": item.dimension,
            })),
        )?;
        self.get_item(&item_id)?
            .ok_or(PlanStoreError::NotFound(item_id))
    }

    /// 单条计划项。
    pub fn get_item(&self, item_id: &str) -> Result<Option<PlanItemRecord>, PlanStoreError> {
        if !id_prefix_matches(item_id, "vi-") {
            return Err(PlanStoreError::NotFound(format!("bad item id: {item_id}")));
        }
        let row = self
            .pool
            .with_conn(|c| {
                c.query_row(
                    "SELECT item_id, plan_id, knowledge_ref, rtis_poc_id, provider_id,
                            provider_version, target_ref, dedup_fingerprint, applicability_status,
                            applicability_reasons_json, risk_class, request_budget, timeout_budget_s,
                            execution_order, expected_observation, result_status, result_reason,
                            error_category, tool_run_id, trace_id, created_at, updated_at, dimension
                     FROM research_plan_item WHERE item_id = ?1",
                    [item_id],
                    |row| {
                        let reasons_json: String = row.get(9)?;
                        Ok(PlanItemRecord {
                            schema_version: SCHEMA_VERSION,
                            item_id: row.get(0)?,
                            plan_id: row.get(1)?,
                            knowledge_ref: row.get(2)?,
                            rtis_poc_id: row.get(3)?,
                            provider_id: row.get(4)?,
                            provider_version: row.get(5)?,
                            target_ref: row.get(6)?,
                            dedup_fingerprint: row.get(7)?,
                            applicability_status: row.get(8)?,
                            applicability_reasons: serde_json::from_str(&reasons_json)
                                .unwrap_or_default(),
                            risk_class: row.get(10)?,
                            request_budget: row.get(11)?,
                            timeout_budget_s: row.get(12)?,
                            execution_order: row.get(13)?,
                            expected_observation: row.get(14)?,
                            result_status: row.get(15)?,
                            result_reason: row.get(16)?,
                            error_category: row.get(17)?,
                            tool_run_id: row.get(18)?,
                            trace_id: row.get(19)?,
                            created_at: row.get(20)?,
                            updated_at: row.get(21)?,
                            dimension: row.get(22)?,
                        })
                    },
                )
                .map(Some)
                .or_else(|e| match e {
                    database::rusqlite::Error::QueryReturnedNoRows => Ok(None),
                    other => Err(other),
                })
            })
            .map_err(|e| PlanStoreError::Db(e.to_string()))?;
        if let Some(record) = &row {
            let doc =
                serde_json::to_value(record).map_err(|e| PlanStoreError::Db(e.to_string()))?;
            validate_item_doc(&doc)
                .map_err(|errs| PlanStoreError::GuardRejected(errs.join("; ")))?;
        }
        Ok(row)
    }

    /// 计划项列表（按 execution_order 稳定排序）。
    pub fn list_items(&self, plan_id: &str) -> Result<Vec<PlanItemRecord>, PlanStoreError> {
        let ids: Vec<String> = self
            .pool
            .with_conn(|c| {
                let mut stmt = c.prepare(
                    "SELECT item_id FROM research_plan_item WHERE plan_id = ?1
                         ORDER BY execution_order ASC, item_id ASC",
                )?;
                let rows = stmt.query_map([plan_id], |row| row.get::<_, String>(0))?;
                rows.collect::<Result<Vec<_>, _>>()
            })
            .map_err(|e| PlanStoreError::Db(e.to_string()))?;
        ids.iter()
            .map(|id| {
                self.get_item(id)?
                    .ok_or_else(|| PlanStoreError::NotFound(id.clone()))
            })
            .collect()
    }

    /// 五态结果写入（守卫强制 + result_assigned 事件留痕）。
    pub fn set_result(
        &self,
        item_id: &str,
        status: &str,
        reason: Option<&str>,
        error_category: Option<&str>,
        tool_run_id: Option<&str>,
        trace_id: Option<&str>,
    ) -> Result<PlanItemRecord, PlanStoreError> {
        let (status_word, reason_word, category_word) =
            ResultGuard::check(status, reason, error_category)?;
        let current = self
            .get_item(item_id)?
            .ok_or_else(|| PlanStoreError::NotFound(item_id.to_owned()))?;
        let now = now_iso();
        let changed = self
            .pool
            .with_conn(|c| {
                c.execute(
                    "UPDATE research_plan_item SET result_status = ?2, result_reason = ?3,
                        error_category = ?4, tool_run_id = COALESCE(?5, tool_run_id),
                        trace_id = COALESCE(?6, trace_id), updated_at = ?7
                     WHERE item_id = ?1",
                    database::rusqlite::params![
                        item_id,
                        status_word,
                        reason_word,
                        category_word,
                        tool_run_id,
                        trace_id,
                        now
                    ],
                )
            })
            .map_err(|e| PlanStoreError::Db(e.to_string()))?;
        if changed == 0 {
            return Err(PlanStoreError::NotFound(item_id.to_owned()));
        }
        self.record_event(
            Some(&current.plan_id),
            Some(item_id),
            tool_run_id,
            trace_id,
            "result_assigned",
            Some(json!({
                "from": current.result_status,
                "to": status_word,
                "reason": reason_word,
                "error_category": category_word,
            })),
        )?;
        self.get_item(item_id)?
            .ok_or_else(|| PlanStoreError::NotFound(item_id.to_owned()))
    }

    /// 事件追加（词表守卫 + 契约校验；append-only）。
    pub fn record_event(
        &self,
        plan_id: Option<&str>,
        item_id: Option<&str>,
        tool_run_id: Option<&str>,
        trace_id: Option<&str>,
        event_type: &str,
        payload: Option<Value>,
    ) -> Result<StoreEvent, PlanStoreError> {
        const EVENT_TYPES: [&str; 9] = [
            "plan_created",
            "plan_status",
            "item_planned",
            "item_skipped",
            "dispatched",
            "evidence_collected",
            "result_assigned",
            "budget_updated",
            "cancelled",
        ];
        if !EVENT_TYPES.contains(&event_type) {
            return Err(PlanStoreError::GuardRejected(format!(
                "unknown event type: {event_type}"
            )));
        }
        let event = StoreEvent {
            schema_version: SCHEMA_VERSION,
            event_id: make_id("ve"),
            plan_id: plan_id.map(str::to_owned),
            item_id: item_id.map(str::to_owned),
            tool_run_id: tool_run_id.map(str::to_owned),
            trace_id: trace_id.map(str::to_owned),
            event_type: event_type.to_owned(),
            payload,
            created_at: now_iso(),
        };
        let doc = serde_json::to_value(&event).map_err(|e| PlanStoreError::Db(e.to_string()))?;
        validate_event_doc(&doc).map_err(|errs| PlanStoreError::GuardRejected(errs.join("; ")))?;
        let payload_json = event.payload.as_ref().map(|p| p.to_string());
        self.pool
            .with_conn(|c| {
                c.execute(
                    "INSERT INTO research_plan_event (event_id, plan_id, item_id, tool_run_id,
                        trace_id, event_type, payload_json, created_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                    database::rusqlite::params![
                        event.event_id,
                        event.plan_id,
                        event.item_id,
                        event.tool_run_id,
                        event.trace_id,
                        event.event_type,
                        payload_json,
                        event.created_at
                    ],
                )
            })
            .map_err(|e| PlanStoreError::Db(e.to_string()))?;
        Ok(event)
    }

    /// 事件列表（计划维度，时间升序）。
    pub fn list_events(&self, plan_id: &str) -> Result<Vec<StoreEvent>, PlanStoreError> {
        let rows = self
            .pool
            .with_conn(|c| {
                let mut stmt = c.prepare(
                    "SELECT event_id, plan_id, item_id, tool_run_id, trace_id, event_type,
                                payload_json, created_at
                         FROM research_plan_event WHERE plan_id = ?1
                         ORDER BY created_at ASC, event_id ASC",
                )?;
                let rows = stmt.query_map([plan_id], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, Option<String>>(1)?,
                        row.get::<_, Option<String>>(2)?,
                        row.get::<_, Option<String>>(3)?,
                        row.get::<_, Option<String>>(4)?,
                        row.get::<_, String>(5)?,
                        row.get::<_, Option<String>>(6)?,
                        row.get::<_, String>(7)?,
                    ))
                })?;
                rows.collect::<Result<Vec<_>, _>>()
            })
            .map_err(|e| PlanStoreError::Db(e.to_string()))?;
        let mut events = Vec::with_capacity(rows.len());
        for (
            event_id,
            plan_id,
            item_id,
            tool_run_id,
            trace_id,
            event_type,
            payload_json,
            created_at,
        ) in rows
        {
            let event = StoreEvent {
                schema_version: SCHEMA_VERSION,
                event_id,
                plan_id,
                item_id,
                tool_run_id,
                trace_id,
                event_type,
                payload: payload_json.and_then(|s| serde_json::from_str(&s).ok()),
                created_at,
            };
            let doc =
                serde_json::to_value(&event).map_err(|e| PlanStoreError::Db(e.to_string()))?;
            validate_event_doc(&doc)
                .map_err(|errs| PlanStoreError::GuardRejected(errs.join("; ")))?;
            events.push(event);
        }
        Ok(events)
    }

    // -----------------------------------------------------------------------
    // 验证证据档（RIP3 P5-7，021 迁移 research_evidence）：Evidence Artifact
    // 采集与 SHA256（§3.1 职责表：采集/哈希/Trace 关联/检索入口在 RIP3，
    // 结论性证据状态以 RTIS 为准）。payload_json append-only 不可变（原始
    // 证据不会被知识更新覆盖）；content_hash = payload 紧凑序列化 SHA256
    // （重复观察识别，§5.8 第四层去重）。
    // -----------------------------------------------------------------------

    /// 证据采集（append-only）：词表守卫 + 哈希 + 落档 + evidence_collected
    /// 事件（事件词表 020 预留面本批启用）。返回 (记录, 是否重复采集)：
    /// 同 plan 内已有同 content_hash 的证据时跳过写入（重复观察留痕在
    /// evidence_collected 事件的 dedup 标记上）。
    pub fn record_evidence(
        &self,
        item_id: &str,
        plan_id: &str,
        tool_run_id: Option<&str>,
        trace_id: Option<&str>,
        evidence_type: &str,
        payload: &Value,
    ) -> Result<(EvidenceRecord, bool), PlanStoreError> {
        const EVIDENCE_TYPES: [&str; 3] = ["provider_result", "normalized_result", "log_excerpt"];
        if !EVIDENCE_TYPES.contains(&evidence_type) {
            return Err(PlanStoreError::GuardRejected(format!(
                "unknown evidence type: {evidence_type}"
            )));
        }
        if !id_prefix_matches(item_id, "vi-") {
            return Err(PlanStoreError::GuardRejected("item id shape".to_owned()));
        }
        if !id_prefix_matches(plan_id, "vp-") {
            return Err(PlanStoreError::GuardRejected("plan id shape".to_owned()));
        }
        // 紧凑序列化即规范化（serde_json Value 键序确定性），同语义载荷同哈希。
        let payload_json = payload.to_string();
        let mut hasher = Sha256::new();
        hasher.update(payload_json.as_bytes());
        let content_hash = format!("{:x}", hasher.finalize());
        let existing = self.find_evidence_by_hash(plan_id, &content_hash)?;
        if let Some(dup) = existing {
            // 重复观察：不重复落档（append-only 快照不可变语义），留痕事件。
            self.record_event(
                Some(plan_id),
                Some(item_id),
                tool_run_id,
                trace_id,
                "evidence_collected",
                Some(json!({
                    "dedup": true,
                    "duplicate_of": dup.evidence_id,
                    "content_hash": content_hash,
                })),
            )?;
            return Ok((dup, true));
        }
        let record = EvidenceRecord {
            evidence_id: make_id("re"),
            item_id: item_id.to_owned(),
            plan_id: plan_id.to_owned(),
            tool_run_id: tool_run_id.map(str::to_owned),
            trace_id: trace_id.map(str::to_owned),
            evidence_type: evidence_type.to_owned(),
            content_hash,
            payload: payload.clone(),
            created_at: now_iso(),
        };
        let payload_store = record.payload.to_string();
        self.pool
            .with_conn(|c| {
                c.execute(
                    "INSERT INTO research_evidence (evidence_id, item_id, plan_id, tool_run_id,
                        trace_id, evidence_type, content_hash, payload_json, created_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                    database::rusqlite::params![
                        record.evidence_id,
                        record.item_id,
                        record.plan_id,
                        record.tool_run_id,
                        record.trace_id,
                        record.evidence_type,
                        record.content_hash,
                        payload_store,
                        record.created_at
                    ],
                )
            })
            .map_err(|e| PlanStoreError::Db(e.to_string()))?;
        self.record_event(
            Some(plan_id),
            Some(item_id),
            tool_run_id,
            trace_id,
            "evidence_collected",
            Some(json!({
                "evidence_id": record.evidence_id,
                "evidence_type": record.evidence_type,
                "content_hash": record.content_hash,
            })),
        )?;
        Ok((record, false))
    }

    /// 同哈希证据查找（计划维度；重复观察识别的存储面）。
    pub fn find_evidence_by_hash(
        &self,
        plan_id: &str,
        content_hash: &str,
    ) -> Result<Option<EvidenceRecord>, PlanStoreError> {
        let row = self.pool.with_conn(|c| {
            c.query_row(
                "SELECT evidence_id, item_id, plan_id, tool_run_id, trace_id, evidence_type,
                        content_hash, payload_json, created_at
                 FROM research_evidence
                 WHERE plan_id = ?1 AND content_hash = ?2
                 ORDER BY created_at LIMIT 1",
                database::rusqlite::params![plan_id, content_hash],
                |r| {
                    Ok(EvidenceRecord {
                        evidence_id: r.get(0)?,
                        item_id: r.get(1)?,
                        plan_id: r.get(2)?,
                        tool_run_id: r.get(3)?,
                        trace_id: r.get(4)?,
                        evidence_type: r.get(5)?,
                        content_hash: r.get(6)?,
                        payload: serde_json::from_str(&r.get::<_, String>(7)?)
                            .unwrap_or(Value::Null),
                        created_at: r.get(8)?,
                    })
                },
            )
            .optional()
        });
        match row {
            Ok(found) => Ok(found),
            Err(e) => Err(PlanStoreError::Db(e.to_string())),
        }
    }

    /// 证据列表（计划项维度，时间升序；UI 跳转面与覆盖矩阵消费）。
    pub fn list_evidence(&self, item_id: &str) -> Result<Vec<EvidenceRecord>, PlanStoreError> {
        if !id_prefix_matches(item_id, "vi-") {
            return Err(PlanStoreError::GuardRejected("item id shape".to_owned()));
        }
        let rows = self
            .pool
            .with_conn(|c| {
                let mut stmt = c.prepare(
                    "SELECT evidence_id, item_id, plan_id, tool_run_id, trace_id,
                                evidence_type, content_hash, payload_json, created_at
                         FROM research_evidence
                         WHERE item_id = ?1
                         ORDER BY created_at ASC, evidence_id ASC",
                )?;
                let collected = stmt
                    .query_map([item_id], |r| {
                        Ok((
                            r.get::<_, String>(0)?,
                            r.get::<_, String>(1)?,
                            r.get::<_, String>(2)?,
                            r.get::<_, Option<String>>(3)?,
                            r.get::<_, Option<String>>(4)?,
                            r.get::<_, String>(5)?,
                            r.get::<_, String>(6)?,
                            r.get::<_, String>(7)?,
                            r.get::<_, String>(8)?,
                        ))
                    })?
                    .collect::<Result<Vec<_>, _>>()?;
                Ok::<Vec<_>, database::rusqlite::Error>(collected)
            })
            .map_err(|e| PlanStoreError::Db(e.to_string()))?;
        let mut records = Vec::with_capacity(rows.len());
        for (
            evidence_id,
            item,
            plan,
            tool_run,
            trace,
            evidence_type,
            content_hash,
            payload_json,
            created_at,
        ) in rows
        {
            records.push(EvidenceRecord {
                evidence_id,
                item_id: item,
                plan_id: plan,
                tool_run_id: tool_run,
                trace_id: trace,
                evidence_type,
                content_hash,
                payload: serde_json::from_str(&payload_json).unwrap_or(Value::Null),
                created_at,
            });
        }
        Ok(records)
    }

    /// 证据列表（计划维度，时间升序；Evidence UI 列表面消费，P5-9b）。
    pub fn list_evidence_by_plan(
        &self,
        plan_id: &str,
    ) -> Result<Vec<EvidenceRecord>, PlanStoreError> {
        if !id_prefix_matches(plan_id, "vp-") {
            return Err(PlanStoreError::GuardRejected("plan id shape".to_owned()));
        }
        let rows = self
            .pool
            .with_conn(|c| {
                let mut stmt = c.prepare(
                    "SELECT evidence_id, item_id, plan_id, tool_run_id, trace_id,
                                evidence_type, content_hash, payload_json, created_at
                         FROM research_evidence
                         WHERE plan_id = ?1
                         ORDER BY created_at ASC, evidence_id ASC",
                )?;
                let collected = stmt
                    .query_map([plan_id], |r| {
                        Ok((
                            r.get::<_, String>(0)?,
                            r.get::<_, String>(1)?,
                            r.get::<_, String>(2)?,
                            r.get::<_, Option<String>>(3)?,
                            r.get::<_, Option<String>>(4)?,
                            r.get::<_, String>(5)?,
                            r.get::<_, String>(6)?,
                            r.get::<_, String>(7)?,
                            r.get::<_, String>(8)?,
                        ))
                    })?
                    .collect::<Result<Vec<_>, _>>()?;
                Ok::<Vec<_>, database::rusqlite::Error>(collected)
            })
            .map_err(|e| PlanStoreError::Db(e.to_string()))?;
        let mut records = Vec::with_capacity(rows.len());
        for (
            evidence_id,
            item,
            plan,
            tool_run,
            trace,
            evidence_type,
            content_hash,
            payload_json,
            created_at,
        ) in rows
        {
            records.push(EvidenceRecord {
                evidence_id,
                item_id: item,
                plan_id: plan,
                tool_run_id: tool_run,
                trace_id: trace,
                evidence_type,
                content_hash,
                payload: serde_json::from_str(&payload_json).unwrap_or(Value::Null),
                created_at,
            });
        }
        Ok(records)
    }

    /// 单条证据回读（Evidence 详情面，P5-9b；evidence_id 形状守卫）。
    pub fn get_evidence(
        &self,
        evidence_id: &str,
    ) -> Result<Option<EvidenceRecord>, PlanStoreError> {
        if !id_prefix_matches(evidence_id, "re-") {
            return Err(PlanStoreError::GuardRejected(
                "evidence id shape".to_owned(),
            ));
        }
        let row = self.pool.with_conn(|c| {
            c.query_row(
                "SELECT evidence_id, item_id, plan_id, tool_run_id, trace_id, evidence_type,
                        content_hash, payload_json, created_at
                 FROM research_evidence
                 WHERE evidence_id = ?1",
                [evidence_id],
                |r| {
                    Ok(EvidenceRecord {
                        evidence_id: r.get(0)?,
                        item_id: r.get(1)?,
                        plan_id: r.get(2)?,
                        tool_run_id: r.get(3)?,
                        trace_id: r.get(4)?,
                        evidence_type: r.get(5)?,
                        content_hash: r.get(6)?,
                        payload: serde_json::from_str(&r.get::<_, String>(7)?)
                            .unwrap_or(Value::Null),
                        created_at: r.get(8)?,
                    })
                },
            )
            .optional()
        });
        match row {
            Ok(found) => Ok(found),
            Err(e) => Err(PlanStoreError::Db(e.to_string())),
        }
    }

    // -----------------------------------------------------------------------
    // 研究假设（RIP3 P5-8，022 research_hypothesis）：假设生成/轮次守卫/
    // 状态机。假设是研究中间面（没有证据不能生成 Finding，§5.10）；轮次
    // 上限由 PlanBudget.max_rounds 在调用面守卫（round_number 超限拒绝）。
    // -----------------------------------------------------------------------

    /// 假设登记（append 型创建；词表与必填守卫；round 超限拒绝）。
    pub fn add_hypothesis(
        &self,
        plan_id: &str,
        h: NewHypothesis,
        max_rounds: i64,
    ) -> Result<HypothesisRecord, PlanStoreError> {
        if !id_prefix_matches(plan_id, "vp-") {
            return Err(PlanStoreError::GuardRejected("plan id shape".to_owned()));
        }
        domain::verification::CoverageDimension::parse(&h.dimension)
            .map_err(|e| PlanStoreError::GuardRejected(e.to_string()))?;
        for (field, value) in [
            ("content", &h.content),
            ("source_evidence", &h.source_evidence),
            ("association_reason", &h.association_reason),
            ("stop_condition", &h.stop_condition),
        ] {
            if value.trim().is_empty() {
                return Err(PlanStoreError::GuardRejected(format!(
                    "hypothesis {field} must not be empty (no evidence, no hypothesis)"
                )));
            }
        }
        if h.round_number < 1 || h.round_number > max_rounds {
            return Err(PlanStoreError::GuardRejected(format!(
                "hypothesis round {} outside budget max_rounds {max_rounds} (stop condition: round budget exhausted)",
                h.round_number
            )));
        }
        let now = now_iso();
        let hypothesis_id = make_id("hi");
        self.pool
            .with_conn(|c| {
                c.execute(
                    "INSERT INTO research_hypothesis (hypothesis_id, plan_id, dimension, content,
                        source_evidence, association_reason, preconditions, expected_observation,
                        negative_control, suggested_verification, cost_risk, stop_condition,
                        round_number, status, schema_version, created_at, updated_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, 'proposed', ?14, ?15, ?15)",
                    database::rusqlite::params![
                        hypothesis_id,
                        plan_id,
                        h.dimension,
                        h.content,
                        h.source_evidence,
                        h.association_reason,
                        h.preconditions,
                        h.expected_observation,
                        h.negative_control,
                        h.suggested_verification,
                        h.cost_risk,
                        h.stop_condition,
                        h.round_number,
                        SCHEMA_VERSION,
                        now
                    ],
                )
            })
            .map_err(|e| PlanStoreError::Db(e.to_string()))?;
        self.get_hypothesis(&hypothesis_id)?
            .ok_or(PlanStoreError::NotFound(hypothesis_id))
    }

    /// 假设状态推进（proposed→in_plan→validated/dropped；validated 不迁出）。
    pub fn set_hypothesis_status(
        &self,
        hypothesis_id: &str,
        status: &str,
    ) -> Result<(), PlanStoreError> {
        const STATUSES: [&str; 4] = ["proposed", "in_plan", "validated", "dropped"];
        if !STATUSES.contains(&status) {
            return Err(PlanStoreError::GuardRejected(format!(
                "unknown hypothesis status: {status}"
            )));
        }
        if !id_prefix_matches(hypothesis_id, "hi-") {
            return Err(PlanStoreError::GuardRejected(
                "hypothesis id shape".to_owned(),
            ));
        }
        let current: Option<String> = self
            .pool
            .with_conn(|c| {
                c.query_row(
                    "SELECT status FROM research_hypothesis WHERE hypothesis_id = ?1",
                    [hypothesis_id],
                    |r| r.get(0),
                )
                .optional()
            })
            .map_err(|e| PlanStoreError::Db(e.to_string()))?;
        match current.as_deref() {
            None => return Err(PlanStoreError::NotFound(hypothesis_id.to_owned())),
            Some("validated") => {
                return Err(PlanStoreError::GuardRejected(format!(
                    "hypothesis {hypothesis_id} is validated (terminal; finding path goes through plan item five-state + finding-link)"
                )));
            }
            _ => {}
        }
        self.pool
            .with_conn(|c| {
                c.execute(
                    "UPDATE research_hypothesis SET status = ?2, updated_at = ?3 WHERE hypothesis_id = ?1",
                    database::rusqlite::params![hypothesis_id, status, now_iso()],
                )
            })
            .map_err(|e| PlanStoreError::Db(e.to_string()))?;
        Ok(())
    }

    /// 假设列表（计划维度，轮次升序；研究 Trace 停止原因可追溯）。
    pub fn list_hypotheses(&self, plan_id: &str) -> Result<Vec<HypothesisRecord>, PlanStoreError> {
        if !id_prefix_matches(plan_id, "vp-") {
            return Err(PlanStoreError::GuardRejected("plan id shape".to_owned()));
        }
        let rows = self
            .pool
            .with_conn(|c| {
                let mut stmt = c.prepare(
                    "SELECT hypothesis_id, plan_id, dimension, content, source_evidence,
                            association_reason, preconditions, expected_observation,
                            negative_control, suggested_verification, cost_risk, stop_condition,
                            round_number, status, schema_version, created_at, updated_at
                     FROM research_hypothesis WHERE plan_id = ?1
                     ORDER BY round_number ASC, hypothesis_id ASC",
                )?;
                let collected = stmt
                    .query_map([plan_id], |r| {
                        Ok((
                            r.get::<_, String>(0)?,
                            r.get::<_, String>(1)?,
                            r.get::<_, String>(2)?,
                            r.get::<_, String>(3)?,
                            r.get::<_, String>(4)?,
                            r.get::<_, String>(5)?,
                            r.get::<_, Option<String>>(6)?,
                            r.get::<_, Option<String>>(7)?,
                            r.get::<_, Option<String>>(8)?,
                            r.get::<_, Option<String>>(9)?,
                            r.get::<_, Option<String>>(10)?,
                            r.get::<_, String>(11)?,
                            r.get::<_, i64>(12)?,
                            r.get::<_, String>(13)?,
                            r.get::<_, i64>(14)?,
                            r.get::<_, String>(15)?,
                            r.get::<_, String>(16)?,
                        ))
                    })?
                    .collect::<Result<Vec<_>, _>>()?;
                Ok::<Vec<_>, database::rusqlite::Error>(collected)
            })
            .map_err(|e| PlanStoreError::Db(e.to_string()))?;
        let mut records = Vec::with_capacity(rows.len());
        for (
            hypothesis_id,
            plan_id,
            dimension,
            content,
            source_evidence,
            association_reason,
            preconditions,
            expected_observation,
            negative_control,
            suggested_verification,
            cost_risk,
            stop_condition,
            round_number,
            status,
            schema_version,
            created_at,
            updated_at,
        ) in rows
        {
            records.push(HypothesisRecord {
                hypothesis_id,
                plan_id,
                dimension,
                content,
                source_evidence,
                association_reason,
                preconditions,
                expected_observation,
                negative_control,
                suggested_verification,
                cost_risk,
                stop_condition,
                round_number,
                status,
                schema_version,
                created_at,
                updated_at,
            });
        }
        Ok(records)
    }

    /// 单条假设。
    pub fn get_hypothesis(
        &self,
        hypothesis_id: &str,
    ) -> Result<Option<HypothesisRecord>, PlanStoreError> {
        let mut all = self
            .pool
            .with_conn(|c| {
                let mut stmt = c.prepare(
                    "SELECT hypothesis_id, plan_id, dimension, content, source_evidence,
                        association_reason, preconditions, expected_observation,
                        negative_control, suggested_verification, cost_risk, stop_condition,
                        round_number, status, schema_version, created_at, updated_at
                 FROM research_hypothesis WHERE hypothesis_id = ?1",
                )?;
                let collected = stmt
                    .query_map([hypothesis_id], |r| {
                        Ok((
                            r.get::<_, String>(0)?,
                            r.get::<_, String>(1)?,
                            r.get::<_, String>(2)?,
                            r.get::<_, String>(3)?,
                            r.get::<_, String>(4)?,
                            r.get::<_, String>(5)?,
                            r.get::<_, Option<String>>(6)?,
                            r.get::<_, Option<String>>(7)?,
                            r.get::<_, Option<String>>(8)?,
                            r.get::<_, Option<String>>(9)?,
                            r.get::<_, Option<String>>(10)?,
                            r.get::<_, String>(11)?,
                            r.get::<_, i64>(12)?,
                            r.get::<_, String>(13)?,
                            r.get::<_, i64>(14)?,
                            r.get::<_, String>(15)?,
                            r.get::<_, String>(16)?,
                        ))
                    })?
                    .collect::<Result<Vec<_>, _>>()?;
                Ok::<Vec<_>, database::rusqlite::Error>(collected)
            })
            .map_err(|e| PlanStoreError::Db(e.to_string()))?;
        let _ = &mut all;
        all.pop()
            .map(
                |(
                    hypothesis_id,
                    plan_id,
                    dimension,
                    content,
                    source_evidence,
                    association_reason,
                    preconditions,
                    expected_observation,
                    negative_control,
                    suggested_verification,
                    cost_risk,
                    stop_condition,
                    round_number,
                    status,
                    schema_version,
                    created_at,
                    updated_at,
                )| {
                    HypothesisRecord {
                        hypothesis_id,
                        plan_id,
                        dimension,
                        content,
                        source_evidence,
                        association_reason,
                        preconditions,
                        expected_observation,
                        negative_control,
                        suggested_verification,
                        cost_risk,
                        stop_condition,
                        round_number,
                        status,
                        schema_version,
                        created_at,
                        updated_at,
                    }
                },
            )
            .map(Some)
            .ok_or_else(|| PlanStoreError::NotFound(hypothesis_id.to_owned()))
    }

    /// 去重指纹查找（P5-4 缓存决策消费：同指纹最近一条已执行项）。
    pub fn find_latest_by_fingerprint(
        &self,
        dedup_fingerprint: &str,
    ) -> Result<Option<PlanItemRecord>, PlanStoreError> {
        let item_id = self
            .pool
            .with_conn(|c| {
                c.query_row(
                    "SELECT item_id FROM research_plan_item WHERE dedup_fingerprint = ?1
                       AND result_status IS NOT NULL
                     ORDER BY updated_at DESC, item_id DESC LIMIT 1",
                    [dedup_fingerprint],
                    |row| row.get::<_, String>(0),
                )
                .map(Some)
                .or_else(|e| match e {
                    database::rusqlite::Error::QueryReturnedNoRows => Ok(None),
                    other => Err(other),
                })
            })
            .map_err(|e| PlanStoreError::Db(e.to_string()))?;
        match item_id {
            Some(id) => self.get_item(&id),
            None => Ok(None),
        }
    }
}
