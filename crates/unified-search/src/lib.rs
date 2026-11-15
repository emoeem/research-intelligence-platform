//! 统一检索归并面（RIP3 P4-2，R2 6.1/6.2 + 12-STORAGE C 层「双库各自索引，
//! 统一 Search API 归并」）：SQLite 运营核知识面（search-engine FTS5/BM25，
//! resource 语料）+ PG 情报核面（intel-db search：BM25 + 向量 + RRF/MMR）跨库
//! 归并，输出统一 UnifiedHit（store 标注来源库）。
//!
//! 薄 crate 边界（ADR-0001 双库边界）：本 crate 只做归并编排，不持有任何
//! 存储实现；PG 面复用 intel-db::search，SQLite 面复用 search-engine（2.0
//! 既有 FTS 面，RTIS 对账零回归红线不触碰）。RRF 常数与 intel-db::search
//! 一致（k=60）。

use intel_db::{search::{self as pg_search, SearchFilters as PgFilters, SearchHit as PgHit}, IntelResult};
use search_engine::model::{SearchFilters as SqliteFilters, SearchQuery as SqliteQuery};
use serde::Serialize;

/// 跨库统一命中（store 标注来源库；PG 面带知识类型，SQLite 面带资源 URL）。
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
    /// 补充字段（PG：knowledge_type；SQLite：canonical_url）。
    pub detail: serde_json::Value,
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
    Ok(hits
        .hits
        .into_iter()
        .map(|h| UnifiedHit {
            store: "sqlite-knowledge".to_string(),
            id: h.resource_id.clone(),
            title: h.name.clone(),
            score: 0.0,
            detail: serde_json::json!({
                "canonical_url": h.canonical_url,
                "resource_type": h.resource_type,
                "quality_grade": h.quality_grade,
            }),
        })
        .collect())
}

fn rrf_merge(pg_hits: Vec<UnifiedHit>, sqlite_hits: Vec<UnifiedHit>, limit: u32) -> Vec<UnifiedHit> {
    let k = 60.0_f32;
    let mut scores: std::collections::HashMap<(String, String), (f32, UnifiedHit)> =
        std::collections::HashMap::new();
    for channel in [pg_hits, sqlite_hits] {
        for (rank, hit) in channel.into_iter().enumerate() {
            let contribution = 1.0 / (k + rank as f32 + 1.0);
            let key = (hit.store.clone(), hit.id.clone());
            let entry = scores
                .entry(key)
                .or_insert_with(|| (0.0, hit.clone()));
            entry.0 += contribution;
        }
    }
    let mut merged: Vec<UnifiedHit> = scores
        .into_values()
        .map(|(score, mut hit)| {
            hit.score = score;
            hit
        })
        .collect();
    merged.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    merged.truncate(limit as usize);
    merged
}

/// 跨库统一检索（PG 面复用 intel-db hybrid，SQLite 面复用 search-engine FTS，
/// RRF 归并；PG 面 workspace 前置谓词在 SQL 内——R2 11.4 权限前置口径）。
pub fn unified_search(input: &UnifiedSearch<'_>, query: &str) -> IntelResult<UnifiedOutcome> {
    // PG 面：hybrid（BM25 + 向量 → RRF，intel-db::search 内部完成）。
    let mut pg_filters = PgFilters::new(&input.workspace_id);
    pg_filters.knowledge_type = input.knowledge_type;
    pg_filters.approved_only = input.approved_only;
    let pg_hits: Vec<UnifiedHit> = pg_search::hybrid_search(
        input.pg_pool,
        input.embed_config,
        &pg_filters,
        query,
        input.limit,
        intel_db::search::DEFAULT_MMR_LAMBDA,
    )?
    .into_iter()
    .map(|h: PgHit| UnifiedHit {
        store: "pg-intel".to_string(),
        id: h.knowledge_id.clone(),
        title: h.title.clone(),
        score: h.score,
        detail: serde_json::json!({ "knowledge_type": h.knowledge_type }),
    })
    .collect();
    let pg_count = pg_hits.len();

    // SQLite 面：FTS5/BM25（2.0 既有面；失败降级为空通道——检索是只读派生
    // 面，单库缺席不阻断跨库归并，如实标注 channels）。
    let (sqlite_hits, sqlite_err) = match sqlite_query(
        input.sqlite_pool,
        &input.sqlite_filters,
        query,
        input.limit,
    ) {
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
    fn rrf_merge_cross_store_and_truncate() {
        let pg = vec![
            UnifiedHit { store: "pg-intel".into(), id: "kc-1".into(), title: "a".into(), score: 0.5, detail: serde_json::json!({}) },
            UnifiedHit { store: "pg-intel".into(), id: "kc-2".into(), title: "b".into(), score: 0.4, detail: serde_json::json!({}) },
        ];
        let sq = vec![
            UnifiedHit { store: "sqlite-knowledge".into(), id: "res-1".into(), title: "a 主题".into(), score: 0.0, detail: serde_json::json!({}) },
            UnifiedHit { store: "sqlite-knowledge".into(), id: "res-9".into(), title: "c".into(), score: 0.0, detail: serde_json::json!({}) },
        ];
        let merged = rrf_merge(pg, sq, 3);
        assert_eq!(merged.len(), 3);
        // 双库各自首条并列最高（RRF 同 rank 同分），稳定性按 map 序不保证——
        // 只断言分数与 store 标注面。
        assert!(merged.iter().all(|h| h.score > 0.0));
        assert!(merged.iter().any(|h| h.store == "pg-intel"));
        assert!(merged.iter().any(|h| h.store == "sqlite-knowledge"));
    }
}
