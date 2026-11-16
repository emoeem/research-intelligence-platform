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
use database::Pool;
use domain::time::now_iso;
use domain::verification::{ErrorCategory, PlanStatus, ResultStatus};
use serde::Serialize;
use serde_json::{json, Value};
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

/// 计划项新增入参（去重指纹与预算由 Planner/调用方给出）。
#[derive(Debug, Clone)]
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
                Ok((
                    parsed.as_str().to_owned(),
                    reason.map(str::to_owned),
                    None,
                ))
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
            let doc = serde_json::to_value(record)
                .map_err(|e| PlanStoreError::Db(e.to_string()))?;
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
        let parsed = PlanStatus::parse(status)
            .map_err(|e| PlanStoreError::GuardRejected(e.to_string()))?;
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
        let applicability = domain::verification::ApplicabilityStatus::parse(&item.applicability_status)
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
        if matches!(applicability, domain::verification::ApplicabilityStatus::Excluded)
            && item.applicability_reasons.is_empty()
        {
            return Err(PlanStoreError::GuardRejected(
                "excluded item requires at least one applicability reason".to_owned(),
            ));
        }
        self.get_plan(plan_id)?
            .ok_or_else(|| PlanStoreError::NotFound(plan_id.to_owned()))?;
        let item_id = make_id("vi");
        let now = now_iso();
        let reasons_json =
            serde_json::to_string(&item.applicability_reasons).map_err(|e| PlanStoreError::Db(e.to_string()))?;
        self.pool
            .with_conn(|c| {
                c.execute(
                    "INSERT INTO research_plan_item (item_id, plan_id, knowledge_ref, rtis_poc_id,
                        provider_id, provider_version, target_ref, dedup_fingerprint,
                        applicability_status, applicability_reasons_json, risk_class,
                        request_budget, timeout_budget_s, execution_order, expected_observation,
                        schema_version, created_at, updated_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?17)",
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
                            error_category, tool_run_id, trace_id, created_at, updated_at
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
            let doc = serde_json::to_value(record)
                .map_err(|e| PlanStoreError::Db(e.to_string()))?;
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
                let mut stmt = c
                    .prepare(
                        "SELECT item_id FROM research_plan_item WHERE plan_id = ?1
                         ORDER BY execution_order ASC, item_id ASC",
                    )?;
                let rows = stmt
                    .query_map([plan_id], |row| row.get::<_, String>(0))?;
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
        validate_event_doc(&doc)
            .map_err(|errs| PlanStoreError::GuardRejected(errs.join("; ")))?;
        let payload_json = event
            .payload
            .as_ref()
            .map(|p| p.to_string());
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
                let mut stmt = c
                    .prepare(
                        "SELECT event_id, plan_id, item_id, tool_run_id, trace_id, event_type,
                                payload_json, created_at
                         FROM research_plan_event WHERE plan_id = ?1
                         ORDER BY created_at ASC, event_id ASC",
                    )?;
                let rows = stmt
                    .query_map([plan_id], |row| {
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
        for (event_id, plan_id, item_id, tool_run_id, trace_id, event_type, payload_json, created_at) in rows {
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
            let doc = serde_json::to_value(&event).map_err(|e| PlanStoreError::Db(e.to_string()))?;
            validate_event_doc(&doc)
                .map_err(|errs| PlanStoreError::GuardRejected(errs.join("; ")))?;
            events.push(event);
        }
        Ok(events)
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
