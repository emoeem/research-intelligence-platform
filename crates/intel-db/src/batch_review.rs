//! batch_review：M10 批次审核台账（rip_platform PG）。
//!
//! 定位：`review_batch` / `review_batch_item` / `review_batch_decision_log`
//! 三表（V0014）承载审核台账。`decide` / `undo` 追加决策日志；`apply` 按域
//! 路由（skill_relation_* 更新 knowledge_relations；knowledge_ingestion 仅
//! 更新提及候选状态；其他未接线域明确保持 ledger_only）。dry-run 是默认，
//! `--apply` 显式提交。
//! 红线：本模块不自动批准任何候选；不修改历史日志行（undo 是追加）。

use crate::{IntelError, IntelResult, PgPool};
use postgres::GenericClient;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::path::Path;

const DOMAINS: [&str; 7] = [
    "vulnerability",
    "nuclei",
    "weapon",
    "poc",
    "poc_sensitive",
    "skill_relation_tool",
    "skill_relation_ref",
];

/// 由 batch_id / 文件名推导审核域。
pub fn domain_from_id(id: &str) -> Option<&'static str> {
    let lower = id.to_lowercase();
    if lower.starts_with("vulnerability") {
        Some("vulnerability")
    } else if lower.starts_with("nuclei") {
        Some("nuclei")
    } else if lower.starts_with("weapon") {
        Some("weapon")
    } else if lower.starts_with("poc-sensitive") {
        Some("poc_sensitive")
    } else if lower.starts_with("poc") {
        Some("poc")
    } else if lower.contains("skill-tool") {
        Some("skill_relation_tool")
    } else if lower.contains("skill-knowledge") || lower.contains("skill-ref") {
        Some("skill_relation_ref")
    } else {
        None
    }
}

pub fn is_known_domain(domain: &str) -> bool {
    DOMAINS.contains(&domain)
}

fn file_sha256(path: &Path) -> IntelResult<String> {
    let bytes = std::fs::read(path).map_err(IntelError::Io)?;
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    Ok(format!("{:x}", hasher.finalize()))
}

/// 导入批次文件（7 域统一形状：`tasks[]` 批次 / `candidates[]` skill 候选）。
/// 幂等：`UNIQUE(batch_id, candidate_id)` 冲突跳过；重复导入刷新批次元数据。
pub fn import_batch_file(
    pool: &PgPool,
    path: &Path,
    domain_override: Option<&str>,
) -> IntelResult<Value> {
    let raw = std::fs::read_to_string(path).map_err(IntelError::Io)?;
    let value: Value =
        serde_json::from_str(&raw).map_err(|e| IntelError::Invalid(format!("bad json: {e}")))?;
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or_else(|| IntelError::Invalid("path has no file stem".into()))?
        .to_string();

    let (batch_id, domain, items) = if let Some(tasks) =
        value.get("tasks").and_then(|t| t.as_array())
    {
        let batch_id = value
            .get("batch_id")
            .and_then(|b| b.as_str())
            .ok_or_else(|| IntelError::Invalid("batch file missing batch_id".into()))?
            .to_string();
        let domain = domain_override
            .map(str::to_string)
            .or_else(|| domain_from_id(&batch_id).map(str::to_string))
            .ok_or_else(|| {
                IntelError::Invalid(format!("cannot infer domain for batch {batch_id}"))
            })?;
        let mut items = Vec::with_capacity(tasks.len());
        for task in tasks {
            let candidate_id = task
                .get("candidate_id")
                .and_then(|c| c.as_str())
                .ok_or_else(|| IntelError::Invalid("task missing candidate_id".into()))?;
            items.push((
                candidate_id.to_string(),
                task.get("priority_rank")
                    .and_then(|v| v.as_i64())
                    .map(|v| v as i32),
                task.get("priority_band")
                    .and_then(|v| v.as_str())
                    .map(str::to_string),
                task.clone(),
            ));
        }
        (batch_id, domain, items)
    } else if let Some(candidates) = value.get("candidates").and_then(|c| c.as_array()) {
        let batch_id = stem.clone();
        let domain = domain_override
            .map(str::to_string)
            .or_else(|| domain_from_id(&stem).map(str::to_string))
            .ok_or_else(|| IntelError::Invalid(format!("cannot infer domain for file {stem}")))?;
        let mut items = Vec::with_capacity(candidates.len());
        for candidate in candidates {
            let skill_id = candidate
                .get("skill_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| IntelError::Invalid("skill candidate missing skill_id".into()))?;
            let relation_type = candidate
                .get("relation_type")
                .and_then(|v| v.as_str())
                .unwrap_or("REFERENCES");
            let candidate_id = if domain == "skill_relation_tool" {
                let tool_id = candidate
                    .get("tool_id")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| {
                        IntelError::Invalid("skill tool candidate missing tool_id".into())
                    })?;
                format!("USES_TOOL:skill:{skill_id}:tool:{tool_id}")
            } else {
                let target_type = candidate
                    .get("target_type")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");
                let target_key = candidate
                    .get("target_key")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| {
                        IntelError::Invalid("skill ref candidate missing target_key".into())
                    })?;
                format!("REFERENCES:skill:{skill_id}:{target_type}:{target_key}")
            };
            let mut payload = candidate.clone();
            if let Some(obj) = payload.as_object_mut() {
                obj.insert("relation_type".into(), json!(relation_type));
            }
            items.push((candidate_id, None, None, payload));
        }
        (batch_id, domain, items)
    } else {
        return Err(IntelError::Invalid(
            "unsupported batch shape: neither tasks[] nor candidates[]".into(),
        ));
    };

    if !is_known_domain(&domain) {
        return Err(IntelError::Invalid(format!("unknown domain {domain}")));
    }

    let sha = file_sha256(path)?;
    let mut conn = pool.get()?;
    let mut tx = conn.transaction()?;
    tx.execute(
        "INSERT INTO review_batch (batch_id, domain, source_file, source_sha256, item_count)
         VALUES ($1,$2,$3,$4,$5)
         ON CONFLICT (batch_id) DO UPDATE
         SET domain=EXCLUDED.domain, source_file=EXCLUDED.source_file,
             source_sha256=EXCLUDED.source_sha256, item_count=EXCLUDED.item_count",
        &[
            &batch_id,
            &domain,
            &path.display().to_string(),
            &sha,
            &(items.len() as i32),
        ],
    )?;
    let mut created = 0usize;
    let mut skipped = 0usize;
    for (candidate_id, rank, band, payload) in &items {
        let n = tx.execute(
            "INSERT INTO review_batch_item
             (batch_id, candidate_id, priority_rank, priority_band, payload)
             VALUES ($1,$2,$3,$4,$5::jsonb)
             ON CONFLICT (batch_id, candidate_id) DO NOTHING",
            &[&batch_id, candidate_id, rank, band, payload],
        )?;
        if n == 1 {
            created += 1;
        } else {
            skipped += 1;
        }
    }
    tx.commit()?;
    Ok(json!({
        "command": "import",
        "batch_id": batch_id,
        "domain": domain,
        "item_count": items.len(),
        "items_created": created,
        "items_skipped_existing": skipped,
        "source_sha256": sha,
    }))
}

/// 批次列表（含各决策状态计数）。
pub fn list_batches(pool: &PgPool) -> IntelResult<Value> {
    let mut conn = pool.get()?;
    let rows = conn.query(
        "SELECT b.batch_id, b.domain, b.item_count, b.source_file,
                count(i.item_id) FILTER (WHERE i.decision='PENDING')  AS pending,
                count(i.item_id) FILTER (WHERE i.decision='APPROVED') AS approved,
                count(i.item_id) FILTER (WHERE i.decision='REJECTED') AS rejected,
                count(i.item_id) FILTER (WHERE i.decision='DEFERRED') AS deferred,
                count(i.item_id) FILTER (WHERE i.applied_at IS NOT NULL) AS applied
         FROM review_batch b
         LEFT JOIN review_batch_item i ON i.batch_id = b.batch_id
         GROUP BY b.batch_id, b.domain, b.item_count, b.source_file
         ORDER BY b.batch_id",
        &[],
    )?;
    let batches: Vec<Value> = rows
        .iter()
        .map(|r| {
            json!({
                "batch_id": r.get::<_, String>("batch_id"),
                "domain": r.get::<_, String>("domain"),
                "item_count": i64::from(r.get::<_, i32>("item_count")),
                "source_file": r.get::<_, String>("source_file"),
                "pending": r.get::<_, i64>("pending"),
                "approved": r.get::<_, i64>("approved"),
                "rejected": r.get::<_, i64>("rejected"),
                "deferred": r.get::<_, i64>("deferred"),
                "applied": r.get::<_, i64>("applied"),
            })
        })
        .collect();
    Ok(json!({"command": "list", "batches": batches}))
}

/// 批次详情（条目分页；payload 随行供证据展示）。
pub fn batch_detail(
    pool: &PgPool,
    batch_id: &str,
    decision: Option<&str>,
    limit: i64,
    offset: i64,
) -> IntelResult<Value> {
    let mut conn = pool.get()?;
    let batch = conn
        .query_opt(
            "SELECT batch_id, domain, source_file, source_sha256, item_count,
                    created_at::text AS created_at
             FROM review_batch WHERE batch_id=$1",
            &[&batch_id],
        )?
        .ok_or_else(|| IntelError::Invalid(format!("batch not found: {batch_id}")))?;
    let rows = if let Some(d) = decision {
        conn.query(
            "SELECT item_id, candidate_id, priority_rank, priority_band, decision,
                    reviewer, reason, decided_at::text AS decided_at,
                    applied_at::text AS applied_at, payload
             FROM review_batch_item
             WHERE batch_id=$1 AND decision=$2
             ORDER BY priority_rank NULLS LAST, item_id LIMIT $3 OFFSET $4",
            &[&batch_id, &d, &limit, &offset],
        )?
    } else {
        conn.query(
            "SELECT item_id, candidate_id, priority_rank, priority_band, decision,
                    reviewer, reason, decided_at::text AS decided_at,
                    applied_at::text AS applied_at, payload
             FROM review_batch_item
             WHERE batch_id=$1
             ORDER BY priority_rank NULLS LAST, item_id LIMIT $2 OFFSET $3",
            &[&batch_id, &limit, &offset],
        )?
    };
    let items: Vec<Value> = rows
        .iter()
        .map(|r| {
            json!({
                "item_id": r.get::<_, i64>("item_id"),
                "candidate_id": r.get::<_, String>("candidate_id"),
                "priority_rank": r.get::<_, Option<i32>>("priority_rank").map(i64::from),
                "priority_band": r.get::<_, Option<String>>("priority_band"),
                "decision": r.get::<_, String>("decision"),
                "reviewer": r.get::<_, Option<String>>("reviewer"),
                "reason": r.get::<_, Option<String>>("reason"),
                "decided_at": r.get::<_, Option<String>>("decided_at"),
                "applied_at": r.get::<_, Option<String>>("applied_at"),
                "payload": r.get::<_, Value>("payload"),
            })
        })
        .collect();
    Ok(json!({
        "command": "show",
        "batch": {
            "batch_id": batch.get::<_, String>("batch_id"),
            "domain": batch.get::<_, String>("domain"),
            "item_count": i64::from(batch.get::<_, i32>("item_count")),
            "source_file": batch.get::<_, String>("source_file"),
            "source_sha256": batch.get::<_, String>("source_sha256"),
            "created_at": batch.get::<_, Option<String>>("created_at"),
        },
        "decision_filter": decision,
        "limit": limit,
        "offset": offset,
        "items": items,
    }))
}

/// 目标条目选择器。
pub enum Selector {
    /// 显式 candidate_id 列表。
    Ids(Vec<String>),
    /// 按优先级带（P0/P1/P2/P3）。
    Band(String),
    /// 全部 PENDING 条目。
    AllPending,
}

fn select_items<C: GenericClient>(
    conn: &mut C,
    batch_id: &str,
    selector: &Selector,
) -> IntelResult<Vec<(i64, String, String)>> {
    let rows = match selector {
        Selector::Ids(ids) => conn.query(
            "SELECT item_id, candidate_id, decision FROM review_batch_item
             WHERE batch_id=$1 AND candidate_id = ANY($2) ORDER BY item_id",
            &[&batch_id, ids],
        )?,
        Selector::Band(band) => conn.query(
            "SELECT item_id, candidate_id, decision FROM review_batch_item
             WHERE batch_id=$1 AND priority_band=$2
             ORDER BY priority_rank NULLS LAST, item_id",
            &[&batch_id, band],
        )?,
        Selector::AllPending => conn.query(
            "SELECT item_id, candidate_id, decision FROM review_batch_item
             WHERE batch_id=$1 AND decision='PENDING'
             ORDER BY priority_rank NULLS LAST, item_id",
            &[&batch_id],
        )?,
    };
    Ok(rows
        .iter()
        .map(|r| {
            (
                r.get::<_, i64>("item_id"),
                r.get::<_, String>("candidate_id"),
                r.get::<_, String>("decision"),
            )
        })
        .collect())
}

/// 决策（dry-run 默认；`apply=true` 提交台账）。只写台账，不触权威存储。
pub fn decide(
    pool: &PgPool,
    batch_id: &str,
    selector: &Selector,
    decision: &str,
    reviewer: &str,
    reason: &str,
    apply: bool,
) -> IntelResult<Value> {
    if !matches!(decision, "APPROVED" | "REJECTED" | "DEFERRED") {
        return Err(IntelError::Invalid(format!(
            "decision must be APPROVED|REJECTED|DEFERRED, got {decision}"
        )));
    }
    if reviewer.trim().is_empty() {
        return Err(IntelError::Invalid("reviewer is required".into()));
    }
    if reason.trim().is_empty() {
        return Err(IntelError::Invalid(
            "reason is required for every decision (audit discipline)".into(),
        ));
    }
    let mut conn = pool.get()?;
    let mut tx = conn.transaction()?;
    let items = select_items(&mut tx, batch_id, selector)?;
    if items.is_empty() {
        tx.rollback()?;
        return Ok(json!({
            "command": "decide",
            "batch_id": batch_id,
            "matched": 0,
            "applied": apply,
            "note": "no matching items",
        }));
    }
    let samples: Vec<String> = items
        .iter()
        .take(20)
        .map(|(_, cid, _)| cid.clone())
        .collect();
    if !apply {
        tx.rollback()?;
        return Ok(json!({
            "command": "decide",
            "batch_id": batch_id,
            "mode": "dry_run",
            "matched": items.len(),
            "decision": decision,
            "sample_candidate_ids": samples,
            "next_step": "rerun with --apply to commit the ledger decision",
        }));
    }
    let mut changed = 0usize;
    for (item_id, _cid, from) in &items {
        let n = tx.execute(
            "UPDATE review_batch_item
             SET decision=$2, reviewer=$3, reason=$4, decided_at=now()
             WHERE item_id=$1 AND decision<>$2",
            &[item_id, &decision, &reviewer, &reason],
        )?;
        if n == 1 {
            tx.execute(
                "INSERT INTO review_batch_decision_log
                 (item_id, action, from_decision, to_decision, reviewer, reason)
                 VALUES ($1,'decide',$2,$3,$4,$5)",
                &[item_id, from, &decision, &reviewer, &reason],
            )?;
            changed += 1;
        }
    }
    tx.commit()?;
    Ok(json!({
        "command": "decide",
        "batch_id": batch_id,
        "mode": "applied",
        "matched": items.len(),
        "changed": changed,
        "decision": decision,
        "reviewer": reviewer,
        "next_step": "run rp-batch-review apply --batch <id> to route APPROVED/REJECTED to authoritative stores",
    }))
}

/// 撤销最近一次决策（追加 undo 日志；已 apply 的关系和摄入审核同时恢复其
/// 权威侧状态，保证台账与权威侧一致）。
pub fn undo(
    pool: &PgPool,
    batch_id: &str,
    ids: &[String],
    reviewer: &str,
    apply: bool,
) -> IntelResult<Value> {
    if reviewer.trim().is_empty() {
        return Err(IntelError::Invalid("reviewer is required".into()));
    }
    let mut conn = pool.get()?;
    let domain: String = conn
        .query_opt(
            "SELECT domain FROM review_batch WHERE batch_id=$1",
            &[&batch_id],
        )?
        .ok_or_else(|| IntelError::Invalid(format!("batch not found: {batch_id}")))?
        .get(0);
    let mut tx = conn.transaction()?;
    let items = select_items(&mut tx, batch_id, &Selector::Ids(ids.to_vec()))?;
    if items.is_empty() {
        tx.rollback()?;
        return Ok(json!({"command":"undo","batch_id":batch_id,"matched":0,"applied":apply}));
    }
    if !apply {
        tx.rollback()?;
        return Ok(json!({
            "command": "undo",
            "batch_id": batch_id,
            "mode": "dry_run",
            "matched": items.len(),
            "sample_candidate_ids": items.iter().take(20).map(|(_, c, _)| c.clone()).collect::<Vec<_>>(),
        }));
    }
    let mut undone = 0usize;
    let mut authoritative_reverted = 0usize;
    let mut failures: Vec<Value> = Vec::new();
    for (item_id, candidate_id, current) in &items {
        // 目标 = 最近一次 decide 的来向（apply/undo 日志的 from=to 无意义）。
        let last = tx.query_opt(
            "SELECT from_decision FROM review_batch_decision_log
             WHERE item_id=$1 AND action='decide' ORDER BY log_id DESC LIMIT 1",
            &[item_id],
        )?;
        let from: String = last
            .map(|row| row.get::<_, String>("from_decision"))
            .unwrap_or_else(|| "PENDING".to_string());
        let applied: bool = tx
            .query_one(
                "SELECT (applied_at IS NOT NULL) AS applied FROM review_batch_item WHERE item_id=$1",
                &[item_id],
            )?
            .get("applied");
        if &from == current && !applied {
            // 决策已是该日志的来向且无权威侧应用：跳过。
            continue;
        }
        // 权威侧回退（仅 skill_relation_* 域，且确实 apply 过）。
        if applied && domain.starts_with("skill_relation") {
            let payload_text: String = tx
                .query_one(
                    "SELECT payload::text FROM review_batch_item WHERE item_id=$1",
                    &[item_id],
                )?
                .get(0);
            let payload: Value = serde_json::from_str(&payload_text).unwrap_or(json!({}));
            let skill_id = payload
                .get("skill_id")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            let relation_type = payload
                .get("relation_type")
                .and_then(|v| v.as_str())
                .unwrap_or("REFERENCES");
            let dst_key = if domain == "skill_relation_tool" {
                let tool_id = payload
                    .get("tool_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default();
                format!("tool:{tool_id}")
            } else {
                payload
                    .get("target_key")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string()
            };
            let src_key = format!("skill:{skill_id}");
            let relation = tx.query_opt(
                "SELECT r.relation_id, r.review_status FROM knowledge_relations r
                 JOIN knowledge_entity_refs a ON a.entity_ref_id = r.src_id
                 JOIN knowledge_entity_refs b ON b.entity_ref_id = r.dst_id
                 WHERE r.relation_type=$1 AND a.canonical_key=$2 AND b.canonical_key=$3
                 LIMIT 1",
                &[&relation_type, &src_key, &dst_key],
            );
            let Ok(Some(rrow)) = relation else {
                failures.push(json!({
                    "candidate_id": candidate_id,
                    "error": "applied item relation not resolved for revert"
                }));
                continue;
            };
            let relation_id: String = rrow.get("relation_id");
            let rel_status: String = rrow.get("review_status");
            // apply 写入的关系日志最后一行的 from_status 即 apply 前状态。
            let prev = tx.query_opt(
                "SELECT from_status FROM knowledge_relation_review_log
                 WHERE relation_id=$1 ORDER BY log_id DESC LIMIT 1",
                &[&relation_id],
            )?;
            let revert_to: String = prev
                .map(|p| p.get::<_, String>("from_status"))
                .unwrap_or_else(|| "CANDIDATE".to_string());
            tx.execute(
                "UPDATE knowledge_relations SET review_status=$2 WHERE relation_id=$1",
                &[&relation_id, &revert_to],
            )?;
            tx.execute(
                "INSERT INTO knowledge_relation_review_log
                 (relation_id, from_status, to_status, reason, reviewer)
                 VALUES ($1,$2,$3,'batch review undo',$4)",
                &[&relation_id, &rel_status, &revert_to, &reviewer],
            )?;
            authoritative_reverted += 1;
        }
        if applied && domain == "knowledge_ingestion" {
            let payload: Value = tx
                .query_one(
                    "SELECT payload FROM review_batch_item WHERE item_id=$1",
                    &[item_id],
                )?
                .get(0);
            let initial_status = payload
                .get("candidate_status")
                .and_then(|value| value.as_str())
                .unwrap_or("PENDING_REVIEW");
            let changed = tx.execute(
                "UPDATE knowledge_ingestion_candidates
                 SET status=$2,updated_at=now()
                 WHERE candidate_id=$1 AND review_batch_id=$3",
                &[candidate_id, &initial_status, &batch_id],
            )?;
            if changed != 1 {
                failures.push(json!({
                    "candidate_id": candidate_id,
                    "error": "ingestion candidate not found for review undo"
                }));
                continue;
            }
            authoritative_reverted += 1;
        }
        tx.execute(
            "UPDATE review_batch_item
             SET decision=$2, reviewer=$3,
                 reason=concat_ws(' | ', 'undo by '||$3, reason),
                 decided_at=now(), applied_at=NULL
             WHERE item_id=$1",
            &[item_id, &from, &reviewer],
        )?;
        tx.execute(
            "INSERT INTO review_batch_decision_log
             (item_id, action, from_decision, to_decision, reviewer, reason)
             VALUES ($1,'undo',$2,$3,$4,'undo of previous decision')",
            &[item_id, current, &from, &reviewer],
        )?;
        undone += 1;
    }
    tx.commit()?;
    Ok(json!({
        "command": "undo",
        "batch_id": batch_id,
        "mode": "applied",
        "matched": items.len(),
        "undone": undone,
        "authoritative_reverted": authoritative_reverted,
        "failed": failures.len(),
        "failures": failures,
    }))
}

/// 按域路由执行已决策条目。dry-run 默认（`--apply` 提交）。
/// skill_relation_* 域：更新 knowledge_relations.review_status + 关系审核日志。
/// 其余域：权威通道接线前明确 ledger_only，不做任何权威写入。
pub fn apply_batch(
    pool: &PgPool,
    batch_id: &str,
    ids: Option<Vec<String>>,
    apply: bool,
) -> IntelResult<Value> {
    let mut conn = pool.get()?;
    let domain: String = conn
        .query_opt(
            "SELECT domain FROM review_batch WHERE batch_id=$1",
            &[&batch_id],
        )?
        .ok_or_else(|| IntelError::Invalid(format!("batch not found: {batch_id}")))?
        .get(0);
    let mut tx = conn.transaction()?;
    let items: Vec<ApplyItem> = match &ids {
        Some(list) => {
            let rows = tx.query(
                "SELECT item_id, candidate_id, decision, reviewer, reason, payload
                 FROM review_batch_item
                 WHERE batch_id=$1 AND candidate_id = ANY($2)
                   AND decision<>'PENDING' AND applied_at IS NULL
                 ORDER BY item_id",
                &[&batch_id, list],
            )?;
            map_apply_rows(&rows)
        }
        None => {
            let rows = tx.query(
                "SELECT item_id, candidate_id, decision, reviewer, reason, payload
                 FROM review_batch_item
                 WHERE batch_id=$1 AND decision<>'PENDING' AND applied_at IS NULL
                 ORDER BY item_id",
                &[&batch_id],
            )?;
            map_apply_rows(&rows)
        }
    };
    if items.is_empty() {
        tx.rollback()?;
        return Ok(json!({
            "command": "apply", "batch_id": batch_id, "domain": domain,
            "matched": 0, "applied": apply,
        }));
    }
    if !apply {
        tx.rollback()?;
        return Ok(json!({
            "command": "apply",
            "batch_id": batch_id,
            "domain": domain.clone(),
            "mode": "dry_run",
            "matched": items.len(),
            "routed": if domain.starts_with("skill_relation") { domain.clone() } else { "ledger_only".to_string() },
            "sample_candidate_ids": items.iter().take(20).map(|(_, c, _, _, _, _)| c.clone()).collect::<Vec<_>>(),
        }));
    }

    if domain == "nuclei" {
        // KSE 权威通道：rp-plugin review（受控写面 + 审计）。
        // 子进程不能在 PG 读事务内长跑：先释放事务再派发。
        tx.rollback()?;
        return apply_nuclei_kse(pool, batch_id, &items, apply);
    }
    if domain == "knowledge_ingestion" {
        let mut succeeded = 0usize;
        let mut failures = Vec::new();
        for (item_id, candidate_id, decision, reviewer, reason, _) in &items {
            let next_status = match decision.as_str() {
                "APPROVED" => "REVIEWED",
                "REJECTED" => "REJECTED",
                "DEFERRED" => "DEFERRED",
                _ => continue,
            };
            let reviewer = reviewer.as_deref().unwrap_or("unknown");
            let reason_text = reason.as_deref().unwrap_or("-");
            let changed = tx.execute(
                "UPDATE knowledge_ingestion_candidates
                 SET status=$2,updated_at=now()
                 WHERE candidate_id=$1 AND review_batch_id=$3",
                &[candidate_id, &next_status, &batch_id],
            )?;
            if changed != 1 {
                failures.push(json!({
                    "candidate_id": candidate_id,
                    "error": "ingestion candidate not found in batch"
                }));
                continue;
            }
            tx.execute(
                "UPDATE review_batch_item SET applied_at=now() WHERE item_id=$1",
                &[item_id],
            )?;
            tx.execute(
                "INSERT INTO review_batch_decision_log
                 (item_id,action,from_decision,to_decision,reviewer,reason)
                 VALUES ($1,'apply',$2,$2,$3,$4)",
                &[item_id, decision, &reviewer, &reason_text],
            )?;
            succeeded += 1;
        }
        tx.commit()?;
        return Ok(json!({
            "command": "apply",
            "batch_id": batch_id,
            "domain": domain,
            "mode": "reviewed_candidate_only",
            "matched": items.len(),
            "succeeded": succeeded,
            "failed": failures.len(),
            "failures": failures,
            "note": "approval records human review of an identifier mention only; it does not assert or publish a verified knowledge claim"
        }));
    }
    if !domain.starts_with("skill_relation") {
        // 无权威通道的域：保持台账决策，不标 applied（诚实面）。
        tx.commit()?;
        return Ok(json!({
            "command": "apply",
            "batch_id": batch_id,
            "domain": domain,
            "mode": "ledger_only",
            "matched": items.len(),
            "note": "no authoritative write channel wired for this domain yet; ledger decisions unchanged",
        }));
    }

    let relation_type = if domain == "skill_relation_tool" {
        "USES_TOOL"
    } else {
        "REFERENCES"
    };
    let mut succeeded = 0usize;
    let mut failures: Vec<Value> = Vec::new();
    for (item_id, candidate_id, decision, reviewer, reason, payload) in &items {
        let reviewer = reviewer.clone().unwrap_or_else(|| "unknown".into());
        let reason_text = reason.clone().unwrap_or_else(|| "-".into());
        let skill_id = payload
            .get("skill_id")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let dst_key = if domain == "skill_relation_tool" {
            let tool_id = payload
                .get("tool_id")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            format!("tool:{tool_id}")
        } else {
            payload
                .get("target_key")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string()
        };
        let src_key = format!("skill:{skill_id}");
        let resolved = tx.query_opt(
            "SELECT r.relation_id, r.review_status FROM knowledge_relations r
             JOIN knowledge_entity_refs a ON a.entity_ref_id = r.src_id
             JOIN knowledge_entity_refs b ON b.entity_ref_id = r.dst_id
             WHERE r.relation_type=$1 AND a.canonical_key=$2 AND b.canonical_key=$3
             LIMIT 1",
            &[&relation_type, &src_key, &dst_key],
        );
        let Ok(Some(row)) = resolved else {
            failures.push(json!({
                "candidate_id": candidate_id,
                "error": "relation not resolved in knowledge_relations"
            }));
            continue;
        };
        let relation_id: String = row.get("relation_id");
        let from_status: String = row.get("review_status");
        let n = tx.execute(
            "UPDATE knowledge_relations SET review_status=$2 WHERE relation_id=$1",
            &[&relation_id, decision],
        )?;
        if n != 1 {
            failures.push(json!({
                "candidate_id": candidate_id,
                "error": "relation update affected 0 rows"
            }));
            continue;
        }
        tx.execute(
            "INSERT INTO knowledge_relation_review_log
             (relation_id, from_status, to_status, reason, reviewer)
             VALUES ($1,$2,$3,$4,$5)",
            &[
                &relation_id,
                &from_status,
                decision,
                &reason_text,
                &reviewer,
            ],
        )?;
        tx.execute(
            "UPDATE review_batch_item SET applied_at=now() WHERE item_id=$1",
            &[item_id],
        )?;
        tx.execute(
            "INSERT INTO review_batch_decision_log
             (item_id, action, from_decision, to_decision, reviewer, reason)
             VALUES ($1,'apply',$2,$3,$4,$5)",
            &[item_id, decision, decision, &reviewer, &reason_text],
        )?;
        succeeded += 1;
    }
    tx.commit()?;
    Ok(json!({
        "command": "apply",
        "batch_id": batch_id,
        "domain": domain,
        "mode": "applied",
        "matched": items.len(),
        "succeeded": succeeded,
        "failed": failures.len(),
        "failures": failures,
    }))
}

/// apply 面条目载荷：(item_id, candidate_id, decision, reviewer, reason, payload)。
type ApplyItem = (i64, String, String, Option<String>, Option<String>, Value);

/// nuclei 域权威路由：把台账 APPROVED/REJECTED 经 `rp-plugin review
/// knowledge.nuclei-templates` 写回 KSE（受控写面 + audit）。candidate_id 形状
/// `nuclei:<version>:<item_key>`。子进程成功才落 applied_at（apply 日志）。
fn apply_nuclei_kse(
    pool: &PgPool,
    batch_id: &str,
    items: &[ApplyItem],
    apply: bool,
) -> IntelResult<Value> {
    const PLUGIN_ID: &str = "knowledge.nuclei-templates";
    let mut groups: std::collections::BTreeMap<(String, String), Vec<&ApplyItem>> =
        std::collections::BTreeMap::new();
    let mut parse_failures: Vec<Value> = Vec::new();
    for item in items {
        let cid = &item.1;
        let rest = cid
            .strip_prefix("nuclei:")
            .ok_or_else(|| IntelError::Invalid(format!("nuclei candidate_id malformed: {cid}")))?;
        let Some((version, key)) = rest.split_once(':') else {
            parse_failures.push(json!({"candidate_id": cid, "error": "missing key segment"}));
            continue;
        };
        if version.trim().is_empty() || key.trim().is_empty() {
            parse_failures.push(json!({"candidate_id": cid, "error": "empty version/key"}));
            continue;
        }
        groups
            .entry((version.to_string(), item.2.clone()))
            .or_default()
            .push(item);
    }
    if !apply {
        let routed: Vec<Value> = groups
            .iter()
            .map(|((version, status), group)| {
                json!({
                    "plugin_id": PLUGIN_ID,
                    "version": version,
                    "status": status,
                    "keys": group.len(),
                    "sample_keys": group.iter().take(3).map(|i| {
                        i.1.splitn(3, ':').nth(2).unwrap_or("").to_string()
                    }).collect::<Vec<_>>(),
                })
            })
            .collect();
        return Ok(json!({
            "command": "apply",
            "batch_id": batch_id,
            "domain": "nuclei",
            "mode": "dry_run",
            "matched": items.len(),
            "parse_failures": parse_failures.len(),
            "routed_groups": routed,
            "next_step": "rerun with --apply (writes KSE via rp-plugin review; test with RP_PLUGIN_DB on a copy first)",
        }));
    }
    let plugin_bin = std::path::Path::new("target/debug/rp-plugin");
    if !plugin_bin.exists() {
        return Err(IntelError::Invalid(
            "target/debug/rp-plugin not found".into(),
        ));
    }
    let mut group_results: Vec<Value> = Vec::new();
    let mut applied_item_ids: Vec<(i64, String, String, String)> = Vec::new();
    for ((version, status), group) in &groups {
        let keys: Vec<String> = group
            .iter()
            .map(|i| i.1.splitn(3, ':').nth(2).unwrap_or("").to_string())
            .collect();
        let reason = format!(
            "batch-review {batch_id}: {} item(s) routed from rip_platform review_batch ledger; per-item reasons live in the ledger",
            group.len()
        );
        let plugin_db =
            std::env::var("RP_PLUGIN_DB").unwrap_or_else(|_| "migrations/dev-p6.db".to_string());
        let output = std::process::Command::new(plugin_bin)
            .args(["--db", &plugin_db])
            .args([
                "review",
                PLUGIN_ID,
                "--version",
                version,
                "--keys",
                &keys.join(","),
                "--status",
                status,
                "--reason",
                &reason,
            ])
            .current_dir(".")
            .output();
        let Ok(out) = output else {
            group_results.push(json!({
                "version": version, "status": status, "keys": keys.len(),
                "error": "rp-plugin spawn failed",
            }));
            continue;
        };
        if !out.status.success() {
            let tail = String::from_utf8_lossy(&out.stderr)
                .chars()
                .rev()
                .take(400)
                .collect::<String>();
            let tail: String = tail.chars().rev().collect();
            group_results.push(json!({
                "version": version, "status": status, "keys": keys.len(),
                "error": "rp-plugin exited non-zero", "stderr_tail": tail,
            }));
            continue;
        }
        let updated = serde_json::from_str::<Value>(&String::from_utf8_lossy(&out.stdout))
            .ok()
            .and_then(|v| v.get("updated").and_then(|u| u.as_u64()))
            .unwrap_or(0);
        group_results.push(json!({
            "version": version, "status": status, "keys": keys.len(),
            "kse_updated": updated, "ok": true,
        }));
        for item in group {
            applied_item_ids.push((item.0, item.2.clone(), reviewer_of(item), reason.clone()));
        }
    }
    // 台账落账：仅子进程成功的组。
    let mut conn = pool.get()?;
    let mut tx = conn.transaction()?;
    for (item_id, decision, reviewer, reason) in &applied_item_ids {
        tx.execute(
            "UPDATE review_batch_item SET applied_at=now() WHERE item_id=$1",
            &[item_id],
        )?;
        tx.execute(
            "INSERT INTO review_batch_decision_log
             (item_id, action, from_decision, to_decision, reviewer, reason)
             VALUES ($1,'apply',$2,$2,$3,$4)",
            &[item_id, decision, reviewer, reason],
        )?;
    }
    tx.commit()?;
    Ok(json!({
        "command": "apply",
        "batch_id": batch_id,
        "domain": "nuclei",
        "mode": "applied",
        "matched": items.len(),
        "parse_failures": parse_failures.len(),
        "groups": group_results,
        "ledger_marked": applied_item_ids.len(),
        "note": "KSE reviewer recorded as user:cli (rp-plugin fixed context); actual reviewer lives in the rip_platform ledger",
    }))
}

fn reviewer_of(item: &ApplyItem) -> String {
    item.3.clone().unwrap_or_else(|| "unknown".into())
}

fn map_apply_rows(rows: &[postgres::Row]) -> Vec<ApplyItem> {
    rows.iter()
        .map(|r| {
            let payload: Value = r.get("payload");
            (
                r.get::<_, i64>("item_id"),
                r.get::<_, String>("candidate_id"),
                r.get::<_, String>("decision"),
                r.get::<_, Option<String>>("reviewer"),
                r.get::<_, Option<String>>("reason"),
                payload,
            )
        })
        .collect()
}

/// M7 投影计划（只读）：从 knowledge_relations 的 APPROVED 关系计算每个 Skill
/// 的 required_tools / knowledge_refs 目标值。写回 platform.db 由
/// scripts/security-intel/apply_skill_projection.py 执行（intel-db 红线：
/// 生产 platform*.db 不经本 crate）。
pub fn projection_plan(pool: &PgPool) -> IntelResult<Value> {
    let mut conn = pool.get()?;
    // 全关系面取技能（含非 APPROVED）：空集也下发，applier 才能清除残留投影。
    let rows = conn.query(
        "SELECT r.relation_id, a.canonical_key AS src_key, r.relation_type,
                b.canonical_key AS dst_key, r.review_status
         FROM knowledge_relations r
         JOIN knowledge_entity_refs a ON a.entity_ref_id = r.src_id
         JOIN knowledge_entity_refs b ON b.entity_ref_id = r.dst_id
         WHERE a.canonical_key LIKE 'skill:%'
         ORDER BY a.canonical_key, r.relation_type, b.canonical_key",
        &[],
    )?;
    let mut skills: std::collections::BTreeMap<String, (Vec<String>, Vec<String>)> =
        std::collections::BTreeMap::new();
    let mut relation_ids: Vec<String> = Vec::new();
    for row in &rows {
        let relation_id: String = row.get("relation_id");
        let src_key: String = row.get("src_key");
        let relation_type: String = row.get("relation_type");
        let dst_key: String = row.get("dst_key");
        let Some(skill) = src_key.strip_prefix("skill:") else {
            continue;
        };
        let review_status: String = row.get("review_status");
        let entry = skills.entry(skill.to_string()).or_default();
        if review_status != "APPROVED" {
            continue;
        }
        match relation_type.as_str() {
            "USES_TOOL" => {
                if let Some(tool) = dst_key.strip_prefix("tool:") {
                    entry.0.push(tool.to_string());
                }
            }
            "REFERENCES" => entry.1.push(dst_key.clone()),
            _ => {}
        }
        relation_ids.push(relation_id);
    }
    let plan: Vec<Value> = skills
        .iter()
        .map(|(skill, (tools, refs))| {
            json!({
                "skill": skill,
                "required_tools": tools,
                "knowledge_refs": refs,
                "relation_ids": [],
            })
        })
        .collect();
    Ok(json!({
        "command": "project",
        "mode": "plan_only",
        "approved_relation_count": relation_ids.len(),
        "skill_count": plan.len(),
        "plan": plan,
        "note": "apply to platform.db skill.required_tools/knowledge_refs via scripts/security-intel/apply_skill_projection.py (dry-run default)",
    }))
}

/// 导出批次决策台账（含完整决策历史）。
pub fn export_batch(pool: &PgPool, batch_id: &str) -> IntelResult<Value> {
    let mut conn = pool.get()?;
    let batch = conn
        .query_opt(
            "SELECT batch_id, domain, source_file, source_sha256, item_count,
                    created_at::text AS created_at
             FROM review_batch WHERE batch_id=$1",
            &[&batch_id],
        )?
        .ok_or_else(|| IntelError::Invalid(format!("batch not found: {batch_id}")))?;
    let rows = conn.query(
        "SELECT i.item_id, i.candidate_id, i.priority_rank, i.priority_band, i.decision,
                i.reviewer, i.reason, i.decided_at::text AS decided_at,
                i.applied_at::text AS applied_at,
                coalesce(jsonb_agg(l.*) FILTER (WHERE l.log_id IS NOT NULL), '[]'::jsonb)::text AS history
         FROM review_batch_item i
         LEFT JOIN review_batch_decision_log l ON l.item_id = i.item_id
         WHERE i.batch_id=$1
         GROUP BY i.item_id, i.candidate_id, i.priority_rank, i.priority_band, i.decision,
                  i.reviewer, i.reason, i.decided_at, i.applied_at
         ORDER BY i.priority_rank NULLS LAST, i.item_id",
        &[&batch_id],
    )?;
    let items: Vec<Value> = rows
        .iter()
        .map(|r| {
            let history: String = r.get("history");
            json!({
                "item_id": r.get::<_, i64>("item_id"),
                "candidate_id": r.get::<_, String>("candidate_id"),
                "priority_rank": r.get::<_, Option<i32>>("priority_rank").map(i64::from),
                "priority_band": r.get::<_, Option<String>>("priority_band"),
                "decision": r.get::<_, String>("decision"),
                "reviewer": r.get::<_, Option<String>>("reviewer"),
                "reason": r.get::<_, Option<String>>("reason"),
                "decided_at": r.get::<_, Option<String>>("decided_at"),
                "applied_at": r.get::<_, Option<String>>("applied_at"),
                "history": serde_json::from_str::<Value>(&history).unwrap_or(json!([])),
            })
        })
        .collect();
    Ok(json!({
        "command": "export",
        "batch": {
            "batch_id": batch.get::<_, String>("batch_id"),
            "domain": batch.get::<_, String>("domain"),
            "item_count": i64::from(batch.get::<_, i32>("item_count")),
            "source_file": batch.get::<_, String>("source_file"),
            "source_sha256": batch.get::<_, String>("source_sha256"),
            "created_at": batch.get::<_, Option<String>>("created_at"),
        },
        "items": items,
    }))
}
