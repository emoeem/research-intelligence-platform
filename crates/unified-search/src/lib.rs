//! 统一检索归并面（RIP3 P4-2，R2 6.1/6.2 + 12-STORAGE C 层「双库各自索引，
//! 统一 Search API 归并」）：SQLite 运营核知识面（search-engine FTS5/BM25，
//! platform 资源 + Overlay 审核 POC）+ PG 情报核面（intel-db search：BM25 + 向量 + RRF/MMR）跨库
//! 归并，输出统一 UnifiedHit（store 标注来源库）。
//!
//! 薄 crate 边界（ADR-0001 双库边界）：本 crate 只做归并编排，不持有任何
//! 存储实现；PG 面复用 intel-db::search，SQLite 面复用 search-engine（2.0
//! 既有 FTS 面，RTIS 对账零回归红线不触碰）。RRF 常数与 intel-db::search
//! 一致（k=60）。

use intel_db::{
    search::{self as pg_search, SearchFilters as PgFilters, SearchHit as PgHit},
    IntelResult,
};
use search_engine::model::{SearchFilters as SqliteFilters, SearchQuery as SqliteQuery};
use serde::Serialize;
use std::collections::HashMap;

/// 跨库统一命中（store 标注来源库；PG 面带知识类型，SQLite 面带资源 URL）。
#[derive(Debug, Clone, Serialize)]
pub struct UnifiedSource {
    /// 原始来源库。
    pub store: String,
    /// 来源库内稳定记录 ID。
    pub id: String,
    /// 该来源自己的元数据与证据引用。
    pub detail: serde_json::Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct UnifiedHit {
    /// 来源库标注："pg-intel" | "sqlite-knowledge"。
    pub store: String,
    /// 各库内稳定 ID。
    pub id: String,
    /// 名称/标题。
    pub title: String,
    /// 归并分数（RRF）。
    pub score: f32,
    /// 补充字段（包含可用的 typed identity）。
    pub detail: serde_json::Value,
    /// 被合并到同一 typed entity 下的所有来源记录；不会丢失原始来源 ID。
    pub sources: Vec<UnifiedSource>,
}

#[derive(Debug, Clone, Serialize)]
pub struct UnifiedOutcome {
    pub hits: Vec<UnifiedHit>,
    /// 各库通道命中数（归并前）。
    pub channels: serde_json::Value,
}

/// 跨库检索输入（workspace 双库同语义；query 同文本）。
pub struct UnifiedSearch<'a> {
    /// PG 连接池（intel-db）。
    pub pg_pool: &'a intel_db::PgPool,
    /// SQLite 索引库池（search-engine FTS 面；可重建派生物）。
    pub sqlite_pool: &'a database::Pool,
    /// PG 嵌入容器配置（向量面查询向量推理）。
    pub embed_config: &'a intel_db::embed_worker::EmbedWorkerConfig,
    pub workspace_id: String,
    pub knowledge_type: Option<domain::kcard::KnowledgeType>,
    pub approved_only: bool,
    pub limit: u32,
    /// SQLite 面过滤（可空 = 无等值过滤）。
    pub sqlite_filters: SqliteFilters,
}

fn sqlite_query(
    pool: &database::Pool,
    filters: &SqliteFilters,
    query: &str,
    limit: u32,
) -> Result<Vec<UnifiedHit>, search_engine::SearchError> {
    let engine = search_engine::SearchEngine::new(pool.clone());
    // 关键词拆分（rp-search-index 同口径：空白/逗号分隔多词，FTS OR 语义）。
    let keywords: Vec<String> = query
        .split(|c: char| c.is_whitespace() || c == ',')
        .filter(|w| !w.is_empty())
        .map(|w| w.to_string())
        .collect();
    let hits = engine.search(&SqliteQuery {
        keywords,
        filters: filters.clone(),
        limit: limit as usize,
        offset: 0,
    })?;
    Ok(hits.hits.into_iter().map(sqlite_hit_to_unified).collect())
}

fn sqlite_hit_to_unified(h: search_engine::model::SearchHit) -> UnifiedHit {
    let store = "sqlite-knowledge".to_string();
    let id = h.resource_id;
    let detail = serde_json::json!({
        "canonical_url": h.canonical_url,
        "resource_type": h.resource_type,
        "quality_grade": h.quality_grade,
        "evidence_level": h.evidence_level,
        "knowledge_lifecycle": h.knowledge_lifecycle,
        "poc_category": h.poc_category,
        "poc_severity": h.poc_severity,
        "poc_cve_id": h.poc_cve_id,
        "source_path": h.source_path,
        "source_commit": h.source_commit,
        "related_poc_ids": h.related_poc_ids,
        "related_entities": h.related_entities,
        "poc_source_integrity": h.poc_source_integrity,
        "poc_verification_result": h.poc_verification_result,
        "poc_dedup_status": h.poc_dedup_status,
        "poc_independent_knowledge_status": h.poc_independent_knowledge_status,
        "poc_missing_evidence": h.poc_missing_evidence,
        "poc_evidence_refs": h.poc_evidence_refs,
        "poc_review_gate": h.poc_review_gate,
        "poc_source_urls": h.poc_source_urls,
        "source_annotations": h.source_annotations,
        "identity": h.identity,
    });
    UnifiedHit {
        store: store.clone(),
        id: id.clone(),
        title: h.name,
        score: 0.0,
        detail: detail.clone(),
        sources: vec![UnifiedSource { store, id, detail }],
    }
}

fn source_ref_merge_key(source: &serde_json::Value, require_approved: bool) -> Option<String> {
    let source_system = source.get("source_system")?.as_str()?;
    let source_entity_id = source.get("source_entity_id")?.as_str()?;
    let source_version = source
        .get("source_version")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    let source_state = source
        .get("source_state")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("CURRENT");
    let review_status = source
        .get("review_status")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("UNREVIEWED");
    if source_system.is_empty()
        || source_entity_id.is_empty()
        || source_system == "rip.intel-db"
        || source_state != "CURRENT"
        || (require_approved && review_status != "APPROVED")
    {
        return None;
    }
    Some(format!(
        "source-ref::{source_system}::{source_entity_id}::{source_version}"
    ))
}

/// A hit may have a typed canonical identity and several immutable source snapshots.
/// Source snapshot keys let a PG KnowledgeCard join a SQLite document only when the
/// explicit source system/entity/version tuple matches; title similarity is never used.
fn canonical_merge_keys(hit: &UnifiedHit) -> Vec<String> {
    let identity = hit.detail.get("identity");
    let mut keys = Vec::new();
    let entity_type = identity
        .and_then(|value| value.get("entity_type"))
        .and_then(serde_json::Value::as_str);
    let canonical_key = identity
        .and_then(|value| value.get("canonical_key"))
        .and_then(serde_json::Value::as_str);
    if let (Some(entity_type), Some(canonical_key)) = (entity_type, canonical_key) {
        if !entity_type.is_empty() && !canonical_key.is_empty() {
            keys.push(format!("entity::{entity_type}::{canonical_key}"));
        }
    }

    if let Some(source_refs) = identity
        .and_then(|value| value.get("source_refs"))
        .and_then(serde_json::Value::as_array)
    {
        for source in source_refs {
            if let Some(key) = source_ref_merge_key(source, true) {
                keys.push(key);
            }
        }
    }

    // SQLite SearchIdentity has a single source snapshot at the top level. The PG
    // primary source is its own KnowledgeCard row and must not be confused with an
    // external source snapshot; external PG refs are handled above.
    if let Some(identity) = identity {
        let source_system = identity
            .get("source_system")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        if source_system != "postgres.knowledge_cards" {
            if let Some(key) = source_ref_merge_key(identity, false) {
                keys.push(key);
            }
        }
    }

    if keys.is_empty() {
        keys.push(format!("source::{}::{}", hit.store, hit.id));
    }
    keys.sort();
    keys.dedup();
    keys
}

fn merge_root(parent: &mut [usize], index: usize) -> usize {
    if parent[index] != index {
        let root = merge_root(parent, parent[index]);
        parent[index] = root;
    }
    parent[index]
}

fn merge_union(parent: &mut [usize], left: usize, right: usize) {
    let left_root = merge_root(parent, left);
    let right_root = merge_root(parent, right);
    if left_root != right_root {
        let (root, child) = if left_root < right_root {
            (left_root, right_root)
        } else {
            (right_root, left_root)
        };
        parent[child] = root;
    }
}

fn rrf_merge(
    pg_hits: Vec<UnifiedHit>,
    sqlite_hits: Vec<UnifiedHit>,
    limit: u32,
) -> Vec<UnifiedHit> {
    let k = 60.0_f32;
    let mut ranked = Vec::new();
    for channel in [pg_hits, sqlite_hits] {
        for (rank, mut hit) in channel.into_iter().enumerate() {
            if hit.sources.is_empty() {
                hit.sources.push(UnifiedSource {
                    store: hit.store.clone(),
                    id: hit.id.clone(),
                    detail: hit.detail.clone(),
                });
            }
            ranked.push((hit, 1.0 / (k + rank as f32 + 1.0)));
        }
    }

    // Union only across different stores. Multiple explicit source snapshots may
    // connect a KnowledgeCard to its SQLite projection without title-based guessing.
    let mut parent: Vec<usize> = (0..ranked.len()).collect();
    let mut key_hits: HashMap<String, Vec<(usize, String)>> = HashMap::new();
    for (index, (hit, _)) in ranked.iter().enumerate() {
        for key in canonical_merge_keys(hit) {
            if let Some(previous) = key_hits.get(&key) {
                for (other_index, other_store) in previous {
                    if other_store != &hit.store {
                        merge_union(&mut parent, index, *other_index);
                    }
                }
            }
            key_hits
                .entry(key)
                .or_default()
                .push((index, hit.store.clone()));
        }
    }

    let roots: Vec<usize> = (0..ranked.len())
        .map(|index| merge_root(&mut parent, index))
        .collect();
    let mut groups: HashMap<usize, (f32, UnifiedHit, Vec<String>)> = HashMap::new();
    for (index, (hit, contribution)) in ranked.into_iter().enumerate() {
        let root = roots[index];
        let keys = canonical_merge_keys(&hit);
        let entry = groups
            .entry(root)
            .or_insert_with(|| (0.0, hit.clone(), Vec::new()));
        entry.0 += contribution;
        entry.2.extend(keys);
        for source in hit.sources {
            if !entry
                .1
                .sources
                .iter()
                .any(|existing| existing.store == source.store && existing.id == source.id)
            {
                entry.1.sources.push(source);
            }
        }
        if entry.1.detail.get("identity").is_none() {
            if let Some(identity) = hit.detail.get("identity") {
                entry.1.detail["identity"] = identity.clone();
            }
        }
    }

    let mut merged: Vec<(f32, String, UnifiedHit)> = groups
        .into_values()
        .map(|(score, mut hit, mut keys)| {
            keys.sort();
            keys.dedup();
            let stable_key = keys
                .into_iter()
                .next()
                .unwrap_or_else(|| format!("source::{}::{}", hit.store, hit.id));
            hit.score = score;
            (score, stable_key, hit)
        })
        .collect();
    merged.sort_by(|a, b| {
        b.0.total_cmp(&a.0)
            .then_with(|| a.1.cmp(&b.1))
            .then_with(|| a.2.store.cmp(&b.2.store))
            .then_with(|| a.2.id.cmp(&b.2.id))
    });
    merged
        .into_iter()
        .take(limit as usize)
        .map(|(_, _, hit)| hit)
        .collect()
}

/// Attach immutable source snapshots to the PG hits in one read-only query. The
/// entity/source tables are the authoritative identity map; no content bodies are read.
fn attach_pg_source_refs(pool: &intel_db::PgPool, hits: &mut [UnifiedHit]) -> IntelResult<()> {
    let ids: Vec<String> = hits
        .iter()
        .filter(|hit| hit.store == "pg-intel")
        .map(|hit| hit.id.clone())
        .collect();
    if ids.is_empty() {
        return Ok(());
    }
    let mut conn = pool.get()?;
    let rows = conn.query(
        "SELECT s.entity_ref_id, e.entity_type, e.canonical_key,
                s.source_ref_id, s.source_system, s.source_entity_id,
                s.source_version, s.canonical_uri, s.source_commit,
                s.content_sha256, s.source_state, a.review_status
         FROM knowledge_entity_sources s
         JOIN knowledge_entity_refs e ON e.entity_ref_id = s.entity_ref_id
         JOIN knowledge_entity_source_assessments a ON a.source_ref_id = s.source_ref_id
         WHERE s.entity_ref_id = ANY($1)
         ORDER BY s.entity_ref_id, s.source_system, s.source_entity_id, s.source_version",
        &[&ids],
    )?;
    let mut by_entity: HashMap<String, Vec<serde_json::Value>> = HashMap::new();
    for row in rows {
        let entity_ref_id: String = row.get(0);
        by_entity
            .entry(entity_ref_id)
            .or_default()
            .push(serde_json::json!({
                "entity_type": row.get::<_, String>(1),
                "canonical_key": row.get::<_, String>(2),
                "source_ref_id": row.get::<_, String>(3),
                "source_system": row.get::<_, String>(4),
                "source_entity_id": row.get::<_, String>(5),
                "source_version": row.get::<_, String>(6),
                "canonical_uri": row.get::<_, Option<String>>(7),
                "source_commit": row.get::<_, Option<String>>(8),
                "content_sha256": row.get::<_, Option<String>>(9),
                "source_state": row.get::<_, String>(10),
                "review_status": row.get::<_, String>(11),
            }));
    }
    for hit in hits.iter_mut().filter(|hit| hit.store == "pg-intel") {
        hit.detail["identity"]["source_refs"] =
            serde_json::json!(by_entity.remove(&hit.id).unwrap_or_default());
    }
    Ok(())
}

/// 跨库统一检索（PG 面复用 intel-db hybrid，SQLite 面复用 search-engine FTS，
/// RRF 归并；PG 面 workspace 前置谓词在 SQL 内——R2 11.4 权限前置口径）。
pub fn unified_search(input: &UnifiedSearch<'_>, query: &str) -> IntelResult<UnifiedOutcome> {
    // PG 面：hybrid（BM25 + 向量 → RRF，intel-db::search 内部完成）。
    let mut pg_filters = PgFilters::new(&input.workspace_id);
    pg_filters.knowledge_type = input.knowledge_type;
    pg_filters.approved_only = input.approved_only;
    let mut pg_hits: Vec<UnifiedHit> = pg_search::hybrid_search(
        input.pg_pool,
        input.embed_config,
        &pg_filters,
        query,
        input.limit,
        intel_db::search::DEFAULT_MMR_LAMBDA,
    )?
    .into_iter()
    .map(|h: PgHit| {
        let store = "pg-intel".to_string();
        let id = h.knowledge_id.clone();
        let detail = serde_json::json!({
            "knowledge_type": h.knowledge_type,
            "identity": {
                "entity_type": "knowledge_card",
                "canonical_key": h.knowledge_id,
                "source_system": "postgres.knowledge_cards",
                "source_entity_id": h.knowledge_id,
            }
        });
        UnifiedHit {
            store: store.clone(),
            id: id.clone(),
            title: h.title,
            score: h.score,
            detail: detail.clone(),
            sources: vec![UnifiedSource { store, id, detail }],
        }
    })
    .collect();
    attach_pg_source_refs(input.pg_pool, &mut pg_hits)?;
    let pg_count = pg_hits.len();

    // SQLite 面：FTS5/BM25（2.0 既有面；失败降级为空通道——检索是只读派生
    // 面，单库缺席不阻断跨库归并，如实标注 channels）。
    let (sqlite_hits, sqlite_err) =
        match sqlite_query(input.sqlite_pool, &input.sqlite_filters, query, input.limit) {
            Ok(hits) => (hits, None),
            Err(e) => (Vec::new(), Some(e.to_string())),
        };
    let sqlite_count = sqlite_hits.len();

    let merged = rrf_merge(pg_hits, sqlite_hits, input.limit);
    Ok(UnifiedOutcome {
        channels: serde_json::json!({
            "pg_intel": pg_count,
            "sqlite_knowledge": sqlite_count,
            "sqlite_error": sqlite_err,
        }),
        hits: merged,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sqlite_projection_preserves_review_facets_and_source_fields() {
        let hit = search_engine::model::SearchHit {
            resource_id: "rtis:poc:19".into(),
            name: "sample".into(),
            canonical_url: "rtis://poc/19".into(),
            description: None,
            resource_type: "rtis_poc".into(),
            quality_final_score: None,
            quality_grade: None,
            license_status: None,
            maintenance_status: None,
            topics: vec![],
            nuclei_severity: None,
            nuclei_protocols: vec![],
            nuclei_cve_ids: vec![],
            nuclei_cwe_ids: vec![],
            source_sections: vec![],
            evidence_level: Some("K2".into()),
            knowledge_lifecycle: Some("Researched".into()),
            poc_category: Some("unauthorized".into()),
            poc_severity: Some("high".into()),
            poc_cve_id: None,
            source_path: Some("pocs/sample".into()),
            source_commit: Some("abc123".into()),
            related_poc_ids: vec!["supports:31".into()],
            related_entities: vec!["has_case:case-a".into()],
            poc_source_urls: vec!["https://example.org/source".into()],
            poc_source_integrity: Some("partial".into()),
            poc_verification_result: Some("historical_evidence_supported".into()),
            poc_dedup_status: Some("independent".into()),
            poc_independent_knowledge_status: Some("independent".into()),
            poc_missing_evidence: vec![],
            poc_evidence_refs: vec!["pocs/sample/evidence.md".into()],
            poc_review_gate: None,
            identity: Some(search_engine::model::SearchIdentity {
                entity_type: Some("poc".into()),
                canonical_key: Some("rtis:poc:19".into()),
                source_system: Some("rtis".into()),
                source_entity_id: Some("19".into()),
                source_version: Some("curation-overlay-v1".into()),
                vulnerability_ids: vec!["CVE-2025-9999".into()],
                ..search_engine::model::SearchIdentity::default()
            }),
            score: 0.0,
            fts_score: None,
            keyword_coverage: 0.0,
            keyword_matches: vec![],
            reasons: vec![],
            source_annotations: vec![],
        };
        let unified = sqlite_hit_to_unified(hit);
        assert_eq!(unified.id, "rtis:poc:19");
        assert_eq!(unified.detail["evidence_level"], "K2");
        assert_eq!(unified.detail["knowledge_lifecycle"], "Researched");
        assert_eq!(unified.detail["source_commit"], "abc123");
        assert_eq!(unified.detail["identity"]["entity_type"], "poc");
        assert_eq!(unified.detail["identity"]["canonical_key"], "rtis:poc:19");
        assert_eq!(unified.sources.len(), 1);
        assert_eq!(unified.detail["related_poc_ids"][0], "supports:31");
        assert_eq!(unified.detail["related_entities"][0], "has_case:case-a");
        assert_eq!(
            unified.detail["poc_source_urls"][0],
            "https://example.org/source"
        );
        assert_eq!(unified.detail["poc_source_integrity"], "partial");
        assert_eq!(
            unified.detail["poc_evidence_refs"][0],
            "pocs/sample/evidence.md"
        );
    }

    #[test]
    fn rrf_merge_cross_store_and_truncate() {
        let pg = vec![
            UnifiedHit {
                store: "pg-intel".into(),
                id: "kc-1".into(),
                title: "a".into(),
                score: 0.5,
                detail: serde_json::json!({}),
                sources: vec![],
            },
            UnifiedHit {
                store: "pg-intel".into(),
                id: "kc-2".into(),
                title: "b".into(),
                score: 0.4,
                detail: serde_json::json!({}),
                sources: vec![],
            },
        ];
        let sq = vec![
            UnifiedHit {
                store: "sqlite-knowledge".into(),
                id: "res-1".into(),
                title: "a".into(),
                score: 0.0,
                detail: serde_json::json!({}),
                sources: vec![],
            },
            UnifiedHit {
                store: "sqlite-knowledge".into(),
                id: "res-9".into(),
                title: "c".into(),
                score: 0.0,
                detail: serde_json::json!({}),
                sources: vec![],
            },
        ];
        let merged = rrf_merge(pg.clone(), sq.clone(), 3);
        assert_eq!(merged.len(), 3);
        assert!(merged.iter().all(|h| h.score > 0.0));
        assert!(merged.iter().any(|h| h.store == "pg-intel"));
        assert!(merged.iter().any(|h| h.store == "sqlite-knowledge"));
        assert_eq!(merged.iter().filter(|h| h.title == "a").count(), 2);
        let repeated = rrf_merge(pg, sq, 3);
        assert_eq!(
            merged.iter().map(|h| (&h.store, &h.id)).collect::<Vec<_>>(),
            repeated
                .iter()
                .map(|h| (&h.store, &h.id))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn explicit_source_reference_merges_pg_card_with_sqlite_record() {
        let pg = UnifiedHit {
            store: "pg-intel".into(),
            id: "card-1".into(),
            title: "CyberoamOS SQL injection notes".into(),
            score: 0.0,
            detail: serde_json::json!({
                "identity": {
                    "entity_type": "knowledge_card",
                    "canonical_key": "card-1",
                    "source_refs": [{
                        "source_system": "knowledge.cisa-kev",
                        "source_entity_id": "CVE-2020-29574",
                        "source_version": "0.1.2",
                        "source_state": "CURRENT",
                        "review_status": "APPROVED"
                    }]
                }
            }),
            sources: vec![],
        };
        let sqlite = UnifiedHit {
            store: "sqlite-knowledge".into(),
            id: "kse:cisa-kev:CVE-2020-29574".into(),
            title: "CVE-2020-29574 SQL injection".into(),
            score: 0.0,
            detail: serde_json::json!({
                "identity": {
                    "entity_type": null,
                    "canonical_key": null,
                    "source_system": "knowledge.cisa-kev",
                    "source_entity_id": "CVE-2020-29574",
                    "source_version": "0.1.2"
                }
            }),
            sources: vec![],
        };
        let merged = rrf_merge(vec![pg], vec![sqlite], 10);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].sources.len(), 2);
        assert_eq!(merged[0].score, 2.0 / 61.0);
    }

    #[test]
    fn source_reference_version_and_state_are_part_of_merge_identity() {
        let make_pg = |version: &str, state: &str| UnifiedHit {
            store: "pg-intel".into(),
            id: "card-1".into(),
            title: "same title".into(),
            score: 0.0,
            detail: serde_json::json!({
                "identity": {
                    "entity_type": "knowledge_card",
                    "canonical_key": "card-1",
                    "source_refs": [{
                        "source_system": "knowledge.cisa-kev",
                        "source_entity_id": "CVE-2020-29574",
                        "source_version": version,
                        "source_state": state,
                        "review_status": "APPROVED"
                    }]
                }
            }),
            sources: vec![],
        };
        let sqlite = UnifiedHit {
            store: "sqlite-knowledge".into(),
            id: "kse:cisa-kev:CVE-2020-29574".into(),
            title: "same title".into(),
            score: 0.0,
            detail: serde_json::json!({
                "identity": {
                    "source_system": "knowledge.cisa-kev",
                    "source_entity_id": "CVE-2020-29574",
                    "source_version": "0.1.2"
                }
            }),
            sources: vec![],
        };
        assert_eq!(
            rrf_merge(vec![make_pg("0.1.1", "CURRENT")], vec![sqlite.clone()], 10).len(),
            2
        );
        assert_eq!(
            rrf_merge(vec![make_pg("0.1.2", "DEPRECATED")], vec![sqlite], 10).len(),
            2
        );
    }

    #[test]
    fn unreviewed_external_source_refs_never_trigger_cross_store_merge() {
        let pg = UnifiedHit {
            store: "pg-intel".into(),
            id: "card-unreviewed".into(),
            title: "same CVE title".into(),
            score: 0.0,
            detail: serde_json::json!({
                "identity": {
                    "entity_type": "knowledge_card",
                    "canonical_key": "card-unreviewed",
                    "source_refs": [{
                        "source_system": "knowledge.cisa-kev",
                        "source_entity_id": "CVE-2020-29574",
                        "source_version": "0.1.2",
                        "source_state": "CURRENT",
                        "review_status": "UNREVIEWED"
                    }]
                }
            }),
            sources: vec![],
        };
        let sqlite = UnifiedHit {
            store: "sqlite-knowledge".into(),
            id: "kse:cisa-kev:CVE-2020-29574".into(),
            title: "same CVE title".into(),
            score: 0.0,
            detail: serde_json::json!({
                "identity": {
                    "source_system": "knowledge.cisa-kev",
                    "source_entity_id": "CVE-2020-29574",
                    "source_version": "0.1.2"
                }
            }),
            sources: vec![],
        };
        assert_eq!(rrf_merge(vec![pg], vec![sqlite], 10).len(), 2);
    }

    #[test]
    fn rrf_merge_collapses_only_identical_typed_entities_and_keeps_sources() {
        let identity = serde_json::json!({
            "entity_type": "vulnerability",
            "canonical_key": "cve:CVE-2025-12345"
        });
        let pg = UnifiedHit {
            store: "pg-intel".into(),
            id: "card-1".into(),
            title: "CVE-2025-12345 advisory".into(),
            score: 0.0,
            detail: serde_json::json!({"identity": identity}),
            sources: vec![],
        };
        let sqlite = UnifiedHit {
            store: "sqlite-knowledge".into(),
            id: "kse:pocindex:1".into(),
            title: "CVE-2025-12345 risk record".into(),
            score: 0.0,
            detail: serde_json::json!({"identity": identity}),
            sources: vec![],
        };
        let different_type = UnifiedHit {
            store: "sqlite-knowledge".into(),
            id: "nuclei:1".into(),
            title: "CVE-2025-12345 template".into(),
            score: 0.0,
            detail: serde_json::json!({
                "identity": {
                    "entity_type": "detection_template",
                    "canonical_key": "cve:CVE-2025-12345"
                }
            }),
            sources: vec![],
        };

        let merged = rrf_merge(vec![pg], vec![sqlite, different_type], 10);
        assert_eq!(merged.len(), 2);
        let vulnerability = merged
            .iter()
            .find(|hit| hit.detail["identity"]["entity_type"] == "vulnerability")
            .unwrap();
        assert_eq!(vulnerability.sources.len(), 2);
        assert!(vulnerability
            .sources
            .iter()
            .any(|source| source.store == "pg-intel"));
        assert!(vulnerability
            .sources
            .iter()
            .any(|source| source.store == "sqlite-knowledge"));
        assert_eq!(vulnerability.score, 2.0 / 61.0);
    }
}
