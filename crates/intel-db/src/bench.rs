//! 检索质量评估（RIP3 P4-4，R2 6.8 Retrieval Benchmark）：
//! 指标 = Recall@K / Precision@K / MRR / NDCG@K（二值相关判定，qrel 显式
//! 相关集合），纯函数零 IO，单测钉值；执行器对四模式（bm25/vector/hybrid/
//! rerank）逐查询跑检索聚合指标与耗时（R2 6.8 全量指标面；Token Cost 记录
//! 口径沿 R2 6.7，本批以查询向量缓存命中与否侧面呈现）。
//!
//! 评测纪律（R2 6.8）：测试集 = RTIS 真实语料播种（元数据方法论面，POC 正文
//! 不进 RIP3 库）+ 固定合成集；qrel 显式声明，不用检索结果自判相关性。

use crate::search::{self, SearchFilters, RerankWorkerConfig};
use crate::{IntelResult, PgPool};
use crate::embed_worker::EmbedWorkerConfig;
use serde::Serialize;
use std::collections::HashSet;
use std::time::Instant;

/// 单查询检索结果评测判据。
pub struct BenchQuery {
    pub query: String,
    /// 相关文档（knowledge_id）显式集合。
    pub relevant: HashSet<String>,
}

/// 单模式单查询的评测行。
#[derive(Debug, Clone, Serialize)]
pub struct BenchRow {
    pub mode: String,
    pub query: String,
    pub k: u32,
    pub recall_at_k: f64,
    pub precision_at_k: f64,
    pub mrr: f64,
    pub ndcg_at_k: f64,
    pub latency_ms: f64,
    pub hits: usize,
}

/// Recall@K：|相关 ∩ 前 K| / |相关|（相关集空时返回 0——无判据查询不进平均）。
pub fn recall_at_k(ranked: &[String], relevant: &HashSet<String>, k: usize) -> f64 {
    if relevant.is_empty() {
        return 0.0;
    }
    let hits = ranked
        .iter()
        .take(k)
        .filter(|id| relevant.contains(*id))
        .count();
    hits as f64 / relevant.len() as f64
}

/// Precision@K：|相关 ∩ 前 K| / K。
pub fn precision_at_k(ranked: &[String], relevant: &HashSet<String>, k: usize) -> f64 {
    if k == 0 {
        return 0.0;
    }
    let hits = ranked
        .iter()
        .take(k)
        .filter(|id| relevant.contains(*id))
        .count();
    hits as f64 / k as f64
}

/// MRR：首个相关文档排名倒数的均值分量（无相关命中为 0）。
pub fn mrr(ranked: &[String], relevant: &HashSet<String>) -> f64 {
    ranked
        .iter()
        .position(|id| relevant.contains(id))
        .map(|p| 1.0 / (p + 1) as f64)
        .unwrap_or(0.0)
}

/// NDCG@K（二值相关）：DCG = Σ rel_i / log2(i+2)；IDCG = 完整 qrel 集（min
/// (|relevant|, k) 项）的理想序列同式——未检出的相关文档在分母扣分，NDCG
/// 同时反映排序质量与召回不足。
pub fn ndcg_at_k(ranked: &[String], relevant: &HashSet<String>, k: usize) -> f64 {
    let dcg: f64 = ranked
        .iter()
        .take(k)
        .enumerate()
        .map(|(i, id)| {
            let rel = if relevant.contains(id) { 1.0 } else { 0.0 };
            rel / (i as f64 + 2.0).log2()
        })
        .sum();
    let ideal_hits = relevant.len().min(k);
    let idcg: f64 = (0..ideal_hits)
        .map(|i| 1.0 / (i as f64 + 2.0).log2())
        .sum();
    if idcg == 0.0 {
        return 0.0;
    }
    dcg / idcg
}

fn evaluate(mode: &str, q: &BenchQuery, ranked: &[String], k: usize, latency: f64) -> BenchRow {
    BenchRow {
        mode: mode.to_string(),
        query: q.query.clone(),
        k: k as u32,
        recall_at_k: recall_at_k(ranked, &q.relevant, k),
        precision_at_k: precision_at_k(ranked, &q.relevant, k),
        mrr: mrr(ranked, &q.relevant),
        ndcg_at_k: ndcg_at_k(ranked, &q.relevant, k),
        latency_ms: latency,
        hits: ranked.len(),
    }
}

/// 四模式基准执行器（逐查询 × {bm25, vector, hybrid, rerank}；结果按行
/// 返回，聚合口径由调用面/报告计算——各查询 qrel 分母不同不混平均）。
/// filters 由调用面构造（workspace 前置谓词在检索 SQL 内）。
pub fn run_benchmark(
    pool: &PgPool,
    embed_config: &EmbedWorkerConfig,
    rerank_config: &RerankWorkerConfig,
    filters: &SearchFilters,
    queries: &[BenchQuery],
    k: u32,
) -> IntelResult<Vec<BenchRow>> {
    let mut rows = Vec::new();
    for q in queries {
        let t = Instant::now();
        let bm25 = search::bm25_search(pool, filters, &q.query, k)?;
        rows.push(evaluate("bm25", q, &ids(&bm25), k as usize, ms(t)));

        let t = Instant::now();
        let vec = search::vector_search(pool, embed_config, filters, &q.query, k)?;
        rows.push(evaluate("vector", q, &ids(&vec), k as usize, ms(t)));

        let t = Instant::now();
        let hyb = search::hybrid_search(pool, embed_config, filters, &q.query, k, search::DEFAULT_MMR_LAMBDA)?;
        rows.push(evaluate("hybrid", q, &ids(&hyb), k as usize, ms(t)));

        let t = Instant::now();
        let rr = search::rerank_search(pool, embed_config, rerank_config, filters, &q.query, k)?;
        rows.push(evaluate("rerank", q, &ids(&rr), k as usize, ms(t)));
    }
    Ok(rows)
}

fn ids(hits: &[search::SearchHit]) -> Vec<String> {
    hits.iter().map(|h| h.knowledge_id.clone()).collect()
}

fn ms(t: Instant) -> f64 {
    (t.elapsed().as_secs_f64() * 1000.0 * 100.0).round() / 100.0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set(items: &[&str]) -> HashSet<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn recall_precision_mrr_ndcg_pinned_values() {
        let rel = set(&["a", "b", "c"]);
        let ranked: Vec<String> = ["x", "a", "y", "b", "z"].iter().map(|s| s.to_string()).collect();
        // recall@5 = 2/3（a、b 命中）；precision@5 = 2/5；mrr = 1/2（a 在位 2）。
        assert_eq!(recall_at_k(&ranked, &rel, 5), 2.0 / 3.0);
        assert_eq!(precision_at_k(&ranked, &rel, 5), 2.0 / 5.0);
        assert_eq!(mrr(&ranked, &rel), 0.5);
        // NDCG@5：DCG = 1/log2(3) + 1/log2(5) = 0.6309 + 0.4307；IDCG = 1/log2(2)+1/log2(3)+1/log2(4)。
        let ndcg = ndcg_at_k(&ranked, &rel, 5);
        let dcg = 1.0 / 3.0_f64.log2() + 1.0 / 5.0_f64.log2();
        let idcg = 1.0 / 2.0_f64.log2() + 1.0 / 3.0_f64.log2() + 1.0 / 4.0_f64.log2();
        assert!((ndcg - dcg / idcg).abs() < 1e-9);
        // 完美序列 NDCG = 1。
        let perfect: Vec<String> = ["a", "b", "c"].iter().map(|s| s.to_string()).collect();
        assert!((ndcg_at_k(&perfect, &rel, 3) - 1.0).abs() < 1e-9);
        // 零相关命中：MRR/NDCG = 0。
        let none: Vec<String> = ["x", "y"].iter().map(|s| s.to_string()).collect();
        assert_eq!(mrr(&none, &rel), 0.0);
        assert_eq!(ndcg_at_k(&none, &rel, 5), 0.0);
        // 空相关集：全 0（无判据查询不进平均）。
        let empty = HashSet::new();
        assert_eq!(recall_at_k(&ranked, &empty, 5), 0.0);
        // K 截断：recall@1 只看首位。
        assert_eq!(recall_at_k(&ranked, &rel, 1), 0.0);
        assert_eq!(precision_at_k(&ranked, &rel, 0), 0.0);
    }
}
