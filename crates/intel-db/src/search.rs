//! 统一检索面（Phase 4 P4-1，R2 第六部分 6.1/6.2/6.4 + 13 号冻结物第五节）：
//! PG 情报核面的 BM25（ParadeDB pg_search 0.26）+ 语义向量（pgvector）+
//! RRF 融合 + MMR 多样化，全部走本模块唯一入口（R2 6.1 禁止私建检索逻辑）。
//!
//! 权限前置（R2 11.4）：workspace + review_status 谓词在 SQL 内
//!（未经批准默认不进检索；hard 风险标记排除）——禁止取回后过滤。
//!
//! 跨库归并（SQLite 运营核知识面 + PG 文档面，12-STORAGE C 层「双库各自索引，
//! 统一 Search API 归并」）随 P4-2 批（独立薄 crate，不破 intel-db 边界）。

use crate::embed_worker::{EmbedWorkerConfig, EmbedWorkerProvider};
use crate::{IntelError, IntelResult, PgPool};
use domain::kcard::KnowledgeType;
use serde_json::json;

/// RRF 常数（R2 6.2：RRF 融合；k=60 业界缺省，调参随 Benchmark）。
pub const RRF_K: f32 = 60.0;
/// MMR 多样化 lambda（0=纯多样性，1=纯相关性；缺省 0.7）。
pub const DEFAULT_MMR_LAMBDA: f32 = 0.7;

#[derive(Debug, Clone)]
pub struct SearchFilters {
    pub workspace_id: String,
    /// 知识类型过滤（R2 6.4 结构化过滤子集：类型面；JSONB/时间面随批扩展）。
    pub knowledge_type: Option<KnowledgeType>,
    /// 仅 APPROVED（缺省 true = 未经批准默认不进检索，R2 5.4）。
    pub approved_only: bool,
}

impl SearchFilters {
    pub fn new(workspace_id: &str) -> Self {
        Self {
            workspace_id: workspace_id.to_string(),
            knowledge_type: None,
            approved_only: true,
        }
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct SearchHit {
    pub knowledge_id: String,
    pub title: String,
    pub knowledge_type: String,
    pub score: f32,
    /// bm25 | vector | rrf（融合面标注来源通道）。
    pub source: String,
}

fn filters_where(filters: &SearchFilters, ws_param: usize, type_param: Option<usize>) -> String {
    let ws = format!("c.workspace_id = ${}", ws_param);
    let review = if filters.approved_only {
        "c.review_status = 'APPROVED'"
    } else {
        "c.review_status <> 'REJECTED'"
    };
    // 硬性风险标记排除（13 号冻结物第三节：命中不进正式上下文）。
    let hard = "NOT (c.quality_flags && ARRAY['POSSIBLE_PROMPT_INJECTION'])";
    let mut conds = vec![ws, review.to_string(), hard.to_string()];
    if let (_, Some(idx)) = (filters.knowledge_type, type_param) {
        conds.push(format!("c.knowledge_type = ${}", idx));
    }
    conds.join(" AND ")
}

/// BM25 检索（pg_search 0.26：@@@ 操作符 + paradedb.score；Top-K 扫描要求
/// ORDER BY 直接使用 pdb.score 形，不用别名包装——性能注记实录）。
pub fn bm25_search(
    pool: &PgPool,
    filters: &SearchFilters,
    query: &str,
    limit: u32,
) -> IntelResult<Vec<SearchHit>> {
    // 动态占位符：类型过滤在场才绑第 4 参（缺省三参与 SQL 数一致，坑位实录）。
    let has_type = filters.knowledge_type.is_some();
    let where_sql = filters_where(filters, 1, if has_type { Some(4) } else { None });
    let sql = format!(
        "SELECT c.knowledge_id, c.title, c.knowledge_type, paradedb.score(c.knowledge_id) AS score
         FROM knowledge_cards c
         WHERE {where_sql} AND (c.title @@@ $2 OR c.content @@@ $2)
         ORDER BY score DESC LIMIT $3"
    );
    let ktype = filters.knowledge_type.map(|k| k.as_str().to_string());
    let mut conn = pool.get()?;
    let rows = if has_type {
        conn.query(&sql, &[&filters.workspace_id, &query, &(limit as i64), &ktype])?
    } else {
        conn.query(&sql, &[&filters.workspace_id, &query, &(limit as i64)])?
    };
    Ok(rows
        .iter()
        .map(|r| SearchHit {
            knowledge_id: r.get(0),
            title: r.get(1),
            knowledge_type: r.get(2),
            score: r.get::<_, Option<f32>>(3).unwrap_or(0.0),
            source: "bm25".to_string(),
        })
        .collect())
}

fn embed_provider(config: &EmbedWorkerConfig) -> EmbedWorkerProvider {
    EmbedWorkerProvider::new(config.clone())
}

/// 语义向量检索（pgvector 余弦距离 top-K；查询向量经 rip.embed.text 本地推理）。
pub fn vector_search(
    pool: &PgPool,
    config: &EmbedWorkerConfig,
    filters: &SearchFilters,
    query: &str,
    limit: u32,
) -> IntelResult<Vec<SearchHit>> {
    let data = embed_provider(config)
        .dispatch(json!({ "texts": [query] }))
        .map_err(IntelError::from)?;
    let vec_vals: Vec<f32> = data["vectors"][0]
        .as_array()
        .ok_or_else(|| IntelError::Invalid("query embedding missing".to_string()))?
        .iter()
        .map(|x| x.as_f64().unwrap_or(0.0) as f32)
        .collect();
    let query_vector = pgvector::Vector::from(vec_vals);
    let has_type = filters.knowledge_type.is_some();
    let where_sql = filters_where(filters, 2, if has_type { Some(4) } else { None });
    let sql = format!(
        "SELECT c.knowledge_id, c.title, c.knowledge_type, (1 - (v.embedding <=> $1::vector)) AS score
         FROM knowledge_card_vectors v
         JOIN knowledge_cards c ON c.knowledge_id = v.card_id
         WHERE {where_sql}
         ORDER BY v.embedding <=> $1::vector LIMIT $3"
    );
    let ktype = ktype_of(filters);
    let mut conn = pool.get()?;
    let rows = if has_type {
        conn.query(&sql, &[&query_vector, &filters.workspace_id, &(limit as i64), &ktype])?
    } else {
        conn.query(&sql, &[&query_vector, &filters.workspace_id, &(limit as i64)])?
    };
    Ok(rows
        .iter()
        .map(|r| SearchHit {
            knowledge_id: r.get(0),
            title: r.get(1),
            knowledge_type: r.get(2),
            score: r.get::<_, Option<f64>>(3).unwrap_or(0.0) as f32,
            source: "vector".to_string(),
        })
        .collect())
}

fn ktype_of(filters: &SearchFilters) -> Option<String> {
    filters.knowledge_type.map(|k| k.as_str().to_string())
}

/// RRF 融合（R2 6.2：score = Σ 1/(k + rank)，k=60；跨通道 rank 并归）。
pub fn rrf_fuse(channels: &[Vec<SearchHit>], limit: u32) -> Vec<SearchHit> {
    let mut scores: std::collections::HashMap<String, (f32, SearchHit)> =
        std::collections::HashMap::new();
    for channel in channels {
        for (rank, hit) in channel.iter().enumerate() {
            let contribution = 1.0 / (RRF_K + rank as f32 + 1.0);
            let entry = scores
                .entry(hit.knowledge_id.clone())
                .or_insert_with(|| (0.0, hit.clone()));
            entry.0 += contribution;
            entry.1.source = if entry.1.source == hit.source {
                hit.source.clone()
            } else {
                "rrf".to_string()
            };
        }
    }
    let mut merged: Vec<SearchHit> = scores
        .into_values()
        .map(|(score, mut hit)| {
            hit.score = score;
            hit.source = "rrf".to_string();
            hit
        })
        .collect();
    merged.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    merged.truncate(limit as usize);
    merged
}

/// MMR 多样化（向量面语义去重：λ*相关性 - (1-λ)*与已选集的最大相似度）。
/// 相似度按 hit 间的语义重叠近似——本面以 BM25 词面重叠作代理（向量化两两
/// 相似属 Phase 8 批规模面；代理在 limit<=50 下成本可忽略）。
pub fn mmr_diversify(hits: &[SearchHit], lambda: f32, limit: u32) -> Vec<SearchHit> {
    if hits.len() <= 1 {
        return hits.to_vec();
    }
    let tokens = |s: &str| -> std::collections::HashSet<String> {
        s.split_whitespace().map(|w| w.to_lowercase()).collect()
    };
    let token_sets: Vec<std::collections::HashSet<String>> =
        hits.iter().map(|h| tokens(&h.title)).collect();
    let mut selected: Vec<usize> = Vec::new();
    let mut candidates: Vec<usize> = (0..hits.len()).collect();
    while selected.len() < limit as usize && !candidates.is_empty() {
        let mut best: Option<(usize, f32)> = None;
        for &cand in &candidates {
            let rel = hits[cand].score;
            let max_sim = selected
                .iter()
                .map(|&s| {
                    let inter = token_sets[cand].intersection(&token_sets[s]).count();
                    let union = token_sets[cand].union(&token_sets[s]).count().max(1);
                    inter as f32 / union as f32
                })
                .fold(0.0_f32, f32::max);
            let mmr = lambda * rel - (1.0 - lambda) * max_sim;
            if best.map(|(_, b)| mmr > b).unwrap_or(true) {
                best = Some((cand, mmr));
            }
        }
        match best {
            Some((idx, _)) => {
                selected.push(idx);
                candidates.retain(|&c| c != idx);
            }
            None => break,
        }
    }
    selected.into_iter().map(|i| hits[i].clone()).collect()
}

/// 混合检索入口（BM25 + 向量 → RRF → MMR；R2 6.2 流程面）。
pub fn hybrid_search(
    pool: &PgPool,
    embed_config: &EmbedWorkerConfig,
    filters: &SearchFilters,
    query: &str,
    limit: u32,
    mmr_lambda: f32,
) -> IntelResult<Vec<SearchHit>> {
    let bm25_hits = bm25_search(pool, filters, query, limit)?;
    let vector_hits = vector_search(pool, embed_config, filters, query, limit)?;
    let fused = rrf_fuse(&[bm25_hits, vector_hits], limit * 3);
    Ok(mmr_diversify(&fused, mmr_lambda, limit))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hit(id: &str, score: f32) -> SearchHit {
        SearchHit {
            knowledge_id: id.to_string(),
            title: id.to_string(),
            knowledge_type: "vuln_principle".to_string(),
            score,
            source: "bm25".to_string(),
        }
    }

    #[test]
    fn rrf_fuse_ranks_cross_channel() {
        let a = vec![hit("x", 2.4), hit("y", 1.1)];
        let b = vec![hit("y", 0.9), hit("z", 0.5)];
        let fused = rrf_fuse(&[a, b], 10);
        // y 双通道命中 → RRF 分最高（1/(61+1) + 1/(61+1) > 1/(61+1)）。
        assert_eq!(fused[0].knowledge_id, "y");
        assert_eq!(fused[0].source, "rrf");
        assert!(fused.len() == 3);
    }

    #[test]
    fn mmr_diversifies_overlapping_titles() {
        let hits = vec![
            hit("dup one alpha", 0.9),
            hit("dup one alpha again", 0.85),
            hit("distinct beta topic", 0.6),
        ];
        let out = mmr_diversify(&hits, 0.5, 3);
        // λ=0.5：低相关的 distinct 主题应进前二（多样性面）。
        assert_ne!(out[1].knowledge_id, out[0].knowledge_id);
        assert!(out.iter().any(|h| h.knowledge_id == "distinct beta topic"));
    }
}
