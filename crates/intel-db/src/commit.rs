//! 解析结果提交服务（P2-1，ADR-0003 裁决 4）：workflow run 终态后由驱动侧
//! 调用；幂等键 = revision_id + IR 摘要（未变化即跳过），变化则全量重放
//! （DELETE+INSERT 单事务），不重解析。
//!
//! 引用计数口径（12-STORAGE 第二节扩展，批次报告如实记录）：
//! content_objects.ref_count = source_records 行数 + document_revisions 行数
//! + document_assets 行数 + document_revisions.ir_content_hash 引用。
//!
//! 重放时先按旧集合减账再按新集合加账（同事务内完成）。
//!
//! parse_state 判定：POSSIBLE_PROMPT_INJECTION 命中或 overall=poor →
//! REVIEW_PENDING（QUARANTINED 候选，13 号冻结物第三节；人工 approve 走
//! [`review_approve`]），否则 APPROVED。

use crate::ir::{validate_ir, remap_block_ids, DocumentIr, QUARANTINE_FLAGS};
use crate::object_store::{sha256_hex, ObjectStore};
use crate::{IntelError, IntelResult, PgPool};
use domain::{DocumentId, DocumentRevisionId};
use serde_json::Value;
use std::collections::HashMap;


/// 提交结果。
#[derive(Debug, serde::Serialize)]
pub struct CommitOutcome {
    pub revision_id: String,
    pub parse_state: String,
    pub blocks_written: usize,
    pub assets_written: usize,
    /// IR 文本 CAS 键（document_revisions.ir_content_hash）。
    pub ir_content_hash: String,
    /// true = IR 摘要与库内一致，跳过重放（幂等命中）。
    pub replayed: bool,
}

/// 解析九态词（R2 4.6；V0003 CHECK 同词值）。
pub const PARSE_STATES: [&str; 12] = [
    "DISCOVERED",
    "VALIDATING",
    "QUEUED",
    "PARSING",
    "NORMALIZING",
    "EXTRACTING",
    "QUALITY_CHECK",
    "REVIEW_PENDING",
    "APPROVED",
    "FAILED",
    "CANCELLED",
    "QUARANTINED",
];

fn check_parse_state(state: &str) -> IntelResult<()> {
    if PARSE_STATES.contains(&state) {
        Ok(())
    } else {
        Err(IntelError::Invalid(format!("invalid parse_state {state:?} (V0003 CHECK domain)")))
    }
}

/// 驱动侧状态置位（PARSING/NORMALIZING/FAILED/CANCELLED 等；APPROVED 仅经
/// 提交服务或人工 approve 面）。
pub fn set_parse_state(
    pool: &PgPool,
    revision_id: &DocumentRevisionId,
    state: &str,
) -> IntelResult<()> {
    check_parse_state(state)?;
    let mut conn = pool.get()?;
    let updated = conn.execute(
        "UPDATE document_revisions SET parse_state = $2 WHERE revision_id = $1",
        &[&revision_id.as_str(), &state],
    )?;
    if updated == 0 {
        return Err(IntelError::Invalid(format!("revision {revision_id} not found")));
    }
    Ok(())
}

/// 人工审核通过（REVIEW_PENDING → APPROVED；13 号冻结物第三节审核面）。
pub fn review_approve(pool: &PgPool, revision_id: &DocumentRevisionId) -> IntelResult<()> {
    let mut conn = pool.get()?;
    let updated = conn.execute(
        "UPDATE document_revisions SET parse_state = 'APPROVED'
         WHERE revision_id = $1 AND parse_state = 'REVIEW_PENDING'",
        &[&revision_id.as_str()],
    )?;
    if updated == 0 {
        return Err(IntelError::Invalid(format!(
            "revision {revision_id} not in REVIEW_PENDING"
        )));
    }
    Ok(())
}

/// 提交解析结果。
///
/// `asset_files`：asset_id（IR 原值）→ 容器产物文件路径（staging/assets/ 下）。
/// 本函数内部完成 block_id/asset_id 的 revision 域重映射（ir.rs remap），
/// 调用侧传容器原 IR 即可。
pub fn commit_parse_result(
    pool: &PgPool,
    store: &dyn ObjectStore,
    document_id: &DocumentId,
    revision_id: &DocumentRevisionId,
    ir_in: &DocumentIr,
    asset_files: &HashMap<String, std::path::PathBuf>,
) -> IntelResult<CommitOutcome> {
    // revision 域重映射 + 校验（schema 门 + 树门）。
    let ir = remap_block_ids(ir_in, revision_id.as_str())?;
    let raw = serde_json::to_value(&ir)
        .map_err(|e| IntelError::Invalid(format!("ir reserialize: {e}")))?;
    validate_ir(&ir, &raw)?;

    // 资产入 CAS：内容 sha256 必须与 IR 声明一致（fail-closed）。
    let mut asset_stored: Vec<(String, String, String)> = Vec::new(); // (asset_id, hash, uri)
    for asset in &ir.assets {
        let path = asset_files
            .get(&asset.asset_id)
            .ok_or_else(|| IntelError::Invalid(format!(
                "asset file missing for {}",
                asset.asset_id
            )))?;
        let bytes = std::fs::read(path)?;
        let actual = sha256_hex(&bytes);
        if actual != asset.content_sha256 {
            return Err(IntelError::Invalid(format!(
                "asset {} content hash mismatch: ir={} actual={}",
                asset.asset_id, asset.content_sha256, actual
            )));
        }
        if asset.storage_uri != format!("cas://{actual}") {
            return Err(IntelError::Invalid(format!(
                "asset {} storage_uri must be cas://<content sha256>",
                asset.asset_id
            )));
        }
        let stored = store.put(&bytes)?;
        asset_stored.push((asset.asset_id.clone(), stored.content_hash, stored.storage_uri));
    }

    // IR 文本进 CAS（解析产物原始凭证，可重放；04 号冻结物第四节）。
    let ir_text = serde_json::to_string(&ir)
        .map_err(|e| IntelError::Invalid(format!("ir serialize: {e}")))?;
    let ir_stored = store.put(ir_text.as_bytes())?;

    // parse_state 判定（质量面 + 风险标记面）。
    let quarantine = ir
        .extraction_quality
        .flags
        .iter()
        .any(|f| QUARANTINE_FLAGS.contains(&f.as_str()));
    let state = if quarantine || ir.extraction_quality.overall == "poor" {
        "REVIEW_PENDING"
    } else {
        "APPROVED"
    };

    let mut conn = pool.get()?;
    let mut tx = conn.transaction()?;

    // 归属校验：revision 必须属于该 document 且活跃。
    let owned: Option<String> = tx
        .query_opt(
            "SELECT content_hash FROM document_revisions r
             JOIN documents d ON d.document_id = r.document_id
             WHERE r.revision_id = $1 AND r.document_id = $2 AND d.lifecycle_status = 'ACTIVE'",
            &[&revision_id.as_str(), &document_id.as_str()],
        )?
        .map(|r| r.get(0));
    if owned.is_none() {
        return Err(IntelError::Invalid(format!(
            "revision {revision_id} not found under document {document_id} (or document deleted)"
        )));
    }

    // 幂等命中：ir_content_hash 一致则跳过重放。
    let existing: Option<(Option<String>,)> = tx
        .query_opt(
            "SELECT ir_content_hash FROM document_revisions WHERE revision_id = $1",
            &[&revision_id.as_str()],
        )?
        .map(|r| (r.get(0),));
    if let Some((Some(old_hash),)) = existing.as_ref() {
        if *old_hash == ir_stored.content_hash {
            tx.commit()?;
            return Ok(CommitOutcome {
                revision_id: revision_id.as_str().to_string(),
                parse_state: state.to_string(),
                blocks_written: ir.blocks.len(),
                assets_written: ir.assets.len(),
                ir_content_hash: ir_stored.content_hash,
                replayed: true,
            });
        }
    }

    // 旧集合减账（重放面）：旧 blocks/assets 行删除 + 旧 ir/assets 引用减账。
    let old_asset_hashes: Vec<String> = tx
        .query(
            "SELECT content_hash FROM document_assets WHERE revision_id = $1",
            &[&revision_id.as_str()],
        )?
        .iter()
        .map(|r| r.get(0))
        .collect();
    let old_ir_hash: Option<Option<String>> = existing.as_ref().map(|(h,)| h.clone());
    tx.execute(
        "DELETE FROM document_assets WHERE revision_id = $1",
        &[&revision_id.as_str()],
    )?;
    tx.execute(
        "DELETE FROM document_blocks WHERE revision_id = $1",
        &[&revision_id.as_str()],
    )?;
    for hash in &old_asset_hashes {
        tx.execute(
            "UPDATE content_objects SET ref_count = ref_count - 1 WHERE content_hash = $1",
            &[&hash],
        )?;
    }
    if let Some(Some(hash)) = &old_ir_hash {
        tx.execute(
            "UPDATE content_objects SET ref_count = ref_count - 1 WHERE content_hash = $1",
            &[&hash],
        )?;
    }

    // 新集合加账 + 落库（blocks → assets → revision 回填）。
    for b in &ir.blocks {
        let location = b.location.clone().unwrap_or(Value::Null);
        let structured = if b.table.is_some() || b.code.is_some() {
            let mut payload = serde_json::Map::new();
            if let Some(t) = &b.table {
                payload.insert("table".to_string(), t.clone());
            }
            if let Some(c) = &b.code {
                payload.insert("code".to_string(), c.clone());
            }
            Value::Object(payload)
        } else {
            Value::Null
        };
        tx.execute(
            "INSERT INTO document_blocks (block_id, document_id, revision_id, parent_block_id,
                 block_type, order_index, content, page_number, location_metadata,
                 structured_payload, extraction_method, extraction_confidence)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)",
            &[
                &b.block_id,
                &document_id.as_str(),
                &revision_id.as_str(),
                &b.parent_id,
                &b.block_type,
                &b.order_index,
                &b.content,
                &b.page.map(|p| p as i32),
                &location,
                &structured,
                &b.extraction_method,
                &b.extraction_confidence.map(|f| f as f32),
            ],
        )?;
    }
    for (asset, (aid, hash, uri)) in ir.assets.iter().zip(asset_stored.iter()) {
        if asset.asset_id != *aid {
            return Err(IntelError::Invalid("asset zip ordering diverged".to_string()));
        }
        tx.execute(
            "INSERT INTO content_objects (content_hash, size_bytes, mime_type, storage_uri, ref_count)
             VALUES ($1, 0, $2, $3, 1)
             ON CONFLICT (content_hash) DO UPDATE SET ref_count = content_objects.ref_count + 1",
            &[&hash, &asset.mime_type, uri],
        )?;
        tx.execute(
            "INSERT INTO document_assets (asset_id, document_id, revision_id, kind,
                 content_hash, storage_uri, source_block_id, mime_type)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8)",
            &[
                aid,
                &document_id.as_str(),
                &revision_id.as_str(),
                &asset.kind,
                hash,
                uri,
                &asset.source_block_id,
                &asset.mime_type,
            ],
        )?;
    }
    // IR 文本对象加账 + revision 回填。
    tx.execute(
        "INSERT INTO content_objects (content_hash, size_bytes, mime_type, storage_uri, ref_count)
         VALUES ($1, $2, 'application/json', $3, 1)
         ON CONFLICT (content_hash) DO UPDATE SET ref_count = content_objects.ref_count + 1",
        &[
            &ir_stored.content_hash,
            &(ir_text.len() as i64),
            &ir_stored.storage_uri,
        ],
    )?;
    let updated = tx.execute(
        "UPDATE document_revisions SET parser_version = $2, parse_state = $3,
             extraction_quality = $4, ir_content_hash = $5
         WHERE revision_id = $1",
        &[
            &revision_id.as_str(),
            &format!(
                "{} {}",
                ir.parser_metadata.parser, ir.parser_metadata.parser_version
            ),
            &state,
            &serde_json::to_value(&ir.extraction_quality)
                .map_err(|e| IntelError::Invalid(format!("quality serialize: {e}")))?,
            &ir_stored.content_hash,
        ],
    )?;
    if updated == 0 {
        return Err(IntelError::Invalid(format!("revision {revision_id} vanished mid-commit")));
    }
    tx.commit()?;
    Ok(CommitOutcome {
        revision_id: revision_id.as_str().to_string(),
        parse_state: state.to_string(),
        blocks_written: ir.blocks.len(),
        assets_written: ir.assets.len(),
        ir_content_hash: ir_stored.content_hash,
        replayed: false,
    })
}

/// 读回 blocks 树（roundtrip 对账面：parent/order/页码一致）。
pub fn read_blocks(
    pool: &PgPool,
    revision_id: &DocumentRevisionId,
) -> IntelResult<Vec<Value>> {
    let mut conn = pool.get()?;
    let rows = conn.query(
        "SELECT block_id, parent_block_id, block_type, order_index, content, page_number,
                location_metadata, structured_payload, extraction_method, extraction_confidence
         FROM document_blocks WHERE revision_id = $1 ORDER BY order_index",
        &[&revision_id.as_str()],
    )?;
    Ok(rows
        .iter()
        .map(|r| {
            serde_json::json!({
                "block_id": r.get::<_, String>(0),
                "parent_id": r.get::<_, Option<String>>(1),
                "block_type": r.get::<_, String>(2),
                "order_index": r.get::<_, i64>(3),
                "content": r.get::<_, String>(4),
                "page_number": r.get::<_, Option<i32>>(5),
                "location_metadata": r.get::<_, Option<Value>>(6),
                "structured_payload": r.get::<_, Option<Value>>(7),
                "extraction_method": r.get::<_, String>(8),
                "extraction_confidence": r.get::<_, Option<f32>>(9),
            })
        })
        .collect())
}

/// 读回资产清单。
pub fn read_assets(pool: &PgPool, revision_id: &DocumentRevisionId) -> IntelResult<Vec<Value>> {
    let mut conn = pool.get()?;
    let rows = conn.query(
        "SELECT asset_id, kind, content_hash, storage_uri, source_block_id, mime_type
         FROM document_assets WHERE revision_id = $1 ORDER BY asset_id",
        &[&revision_id.as_str()],
    )?;
    Ok(rows
        .iter()
        .map(|r| {
            serde_json::json!({
                "asset_id": r.get::<_, String>(0),
                "kind": r.get::<_, String>(1),
                "content_hash": r.get::<_, String>(2),
                "storage_uri": r.get::<_, String>(3),
                "source_block_id": r.get::<_, String>(4),
                "mime_type": r.get::<_, String>(5),
            })
        })
        .collect())
}
