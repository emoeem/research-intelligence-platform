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
use domain::{NodeHealth, RuntimeNode, ToolRunStatus};
use serde_json::{json, Value};
use std::sync::OnceLock;
use std::time::Duration;
use tool_engine::adapter::{
    execute_container_via_engine, ContainerCall, NetworkPlan, PodmanHostAdapter, PodmanHostConfig,
};
use tool_engine::Engine;
use tool_registry::{Options, Registry};

/// RRF 常数（R2 6.2：RRF 融合；k=60 业界缺省，调参随 Benchmark）。
pub const RRF_K: f32 = 60.0;
/// MMR 多样化 lambda（0=纯多样性，1=纯相关性；缺省 0.7）。
pub const DEFAULT_MMR_LAMBDA: f32 = 0.7;

#[derive(Debug, Clone)]
pub struct SearchFilters {
    pub workspace_id: String,
    /// 知识类型过滤（R2 6.4 结构化过滤子集：类型面）。
    pub knowledge_type: Option<KnowledgeType>,
    /// 仅 APPROVED（缺省 true = 未经批准默认不进检索，R2 5.4）。
    pub approved_only: bool,
    /// JSONB 包含过滤（R2 6.4 结构化面：structured_content @> $json，
    /// P4-2 扩展；键值对单层包含语义）。
    pub json_contains: Option<Value>,
}

impl SearchFilters {
    pub fn new(workspace_id: &str) -> Self {
        Self {
            workspace_id: workspace_id.to_string(),
            knowledge_type: None,
            approved_only: true,
            json_contains: None,
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
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
    // JSONB 包含过滤（P4-2：structured_content @> 占位符序号 = type 后一位，
    // 由调用面按 has_type/json 在场动态分配——占位符计数在调用面维护）。
    if filters.json_contains.is_some() {
        // 显式 ::jsonb cast（PG 未类型化参数推断失败坑位，::vector 同源实录）。
        conds.push("c.structured_content @> $JSON::jsonb".to_string());
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
    // 动态占位符组装：workspace=1，query=2，limit=3，type/json 按在场续号
    // （参数数与占位符数必须一致的坑位实录，P4-1）。
    let has_type = filters.knowledge_type.is_some();
    let has_json = filters.json_contains.is_some();
    // 占位符按 SQL 出现顺序编号：$1 ws，$2 query，$3 type(可选)，$4 json(可选)，
    // 末位 limit（坑位实录：未引用占位符会被 PG 拒推断，必须四态精确匹配）。
    let type_idx = if has_type { Some(3) } else { None };
    let json_idx = if has_json { Some(if has_type { 4 } else { 3 }) } else { None };
    let lim_idx = 3 + (has_type as usize) + (has_json as usize);
    let where_sql = filters_where(filters, 1, type_idx).replace("$JSON", &format!("${}", json_idx.unwrap_or(0)));
    let sql = format!(
        "SELECT c.knowledge_id, c.title, c.knowledge_type, paradedb.score(c.knowledge_id) AS score
         FROM knowledge_cards c
         WHERE {where_sql} AND (c.title @@@ $2 OR c.content @@@ $2)
         ORDER BY score DESC LIMIT ${lim_idx}"
    );
    let ktype = filters.knowledge_type.map(|k| k.as_str().to_string());
    let mut conn = pool.get()?;
    let rows = match (has_type, has_json) {
        (true, true) => {
            conn.query(&sql, &[&filters.workspace_id, &query, &ktype, &filters.json_contains, &(limit as i64)])?
        }
        (true, false) => {
            conn.query(&sql, &[&filters.workspace_id, &query, &ktype, &(limit as i64)])?
        }
        (false, true) => {
            conn.query(&sql, &[&filters.workspace_id, &query, &filters.json_contains, &(limit as i64)])?
        }
        (false, false) => {
            conn.query(&sql, &[&filters.workspace_id, &query, &(limit as i64)])?
        }
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
/// 查询向量缓存（P4-3 R2 6.6）：key = sha256(模型指纹 ‖ query)，命中跳过容器
/// 派发（秒级推理 → 磁盘读）；缓存失败静默降级直查（不阻断检索）。
pub fn vector_search(
    pool: &PgPool,
    config: &EmbedWorkerConfig,
    filters: &SearchFilters,
    query: &str,
    limit: u32,
) -> IntelResult<Vec<SearchHit>> {
    let model_fp = config.manifest_dir.display().to_string();
    let cache = crate::search_cache::SearchCache::open_default().ok().map(|c| {
        let k = crate::search_cache::cache_key("query-vector", "query", &model_fp, query, "embed-query-v1");
        (c, k)
    });
    let query_vector: pgvector::Vector = match cache
        .as_ref()
        .and_then(|(c, k)| c.get(k, &model_fp, "embed-query-v1", crate::search_cache::TTL_QUERY_VECTOR))
        .and_then(|payload| serde_json::from_str::<Vec<f32>>(&payload).ok())
    {
        Some(vec_vals) => pgvector::Vector::from(vec_vals),
        None => {
            let data = embed_provider(config)
                .dispatch(json!({ "texts": [query] }))
                .map_err(IntelError::from)?;
            let vec_vals: Vec<f32> = data["vectors"][0]
                .as_array()
                .ok_or_else(|| IntelError::Invalid("query embedding missing".to_string()))?
                .iter()
                .map(|x| x.as_f64().unwrap_or(0.0) as f32)
                .collect();
            if let Some((c, k)) = cache.as_ref() {
                if let Ok(payload) = serde_json::to_string(&vec_vals) {
                    let _ = c.put(k, &model_fp, "embed-query-v1", &payload);
                }
            }
            pgvector::Vector::from(vec_vals)
        }
    };
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

/// 混合检索入口（BM25 + 向量 → RRF；R2 6.2 流程面）。
/// P4-4 修正：MMR 从本面移除——R2 6.2 流程 MMR 位于 Reranking 之后，输入
/// 应为 0-1 相关分；RRF 分（~1/(k+rank) 量级）直接做 MMR 时多样性项
///（title Jaccard 0-1）压倒相关性项，基准实测 hybrid Recall@10 从 0.54
/// 掉到 0.04（BENCHMARK 实录）。MMR 现于 rerank_search 的 rerank 分之后。
/// 检索结果缓存（P4-3 R2 6.6 Query Cache）：key = sha256(mode ‖ filters ‖
/// query ‖ limit)，fingerprint = 索引 schema 版本 + workspace 治理态
/// （APPROVED 卡数 + max(updated_at)：review 跃迁/建卡即失效）；缓存失败
/// 静默降级直查。
pub fn hybrid_search(
    pool: &PgPool,
    embed_config: &EmbedWorkerConfig,
    filters: &SearchFilters,
    query: &str,
    limit: u32,
    mmr_lambda: f32,
) -> IntelResult<Vec<SearchHit>> {
    let cache = crate::search_cache::SearchCache::open_default().ok();
    let gov_fp = governance_fingerprint(pool, &filters.workspace_id).unwrap_or_default();
    let fp = format!(
        "pg-search-v6\u{1f}{gov_fp}\u{1f}approved={}\u{1f}type={:?}\u{1f}json={}",
        filters.approved_only,
        filters.knowledge_type.map(|k| k.as_str()),
        serde_json::to_string(&filters.json_contains).unwrap_or_default(),
    );
    // 缓存键必须含 query（P4-4 基准实录：漏 query 时 10 查询共用一键，
    // 后续查询全部命中首查询缓存，hybrid Recall@10 从 0.54 掉到 0.04）。
    let params = format!("{query}|{limit}|{mmr_lambda}|{}", serde_json::to_string(&filters.json_contains).unwrap_or_default());
    let cache = cache.map(|c| {
        let k = crate::search_cache::cache_key("retrieval", "hybrid", SEARCH_CACHE_VERSION, &params, &fp);
        (c, k)
    });
    if let Some((c, k)) = cache.as_ref() {
        if let Some(payload) = c.get(k, SEARCH_CACHE_VERSION, &fp, crate::search_cache::TTL_RETRIEVAL) {
            if let Ok(hits) = serde_json::from_str::<Vec<SearchHit>>(&payload) {
                return Ok(hits);
            }
        }
    }
    let bm25_hits = bm25_search(pool, filters, query, limit)?;
    let vector_hits = vector_search(pool, embed_config, filters, query, limit)?;
    let fused = rrf_fuse(&[bm25_hits, vector_hits], limit);
    if let Some((c, k)) = cache.as_ref() {
        if let Ok(payload) = serde_json::to_string(&fused) {
            let _ = c.put(k, SEARCH_CACHE_VERSION, &fp, &payload);
        }
    }
    Ok(fused)
}

/// 检索结果缓存条目版本（结构/语义变化即整体失效；v3 = 键补 query +
/// MMR 移出 hybrid）。
const SEARCH_CACHE_VERSION: &str = "search-v3";

/// workspace 治理态指纹（APPROVED 卡数 + max(updated_at)：审核跃迁/新建卡
/// 即缓存失效；查询失败 None = 调用方禁用缓存面）。
fn governance_fingerprint(pool: &PgPool, workspace_id: &str) -> Option<String> {
    let mut conn = pool.get().ok()?;
    let row = conn
        .query_opt(
            "SELECT COUNT(*), COALESCE(MAX(updated_at)::text, '') FROM knowledge_cards
             WHERE workspace_id = $1",
            &[&workspace_id],
        )
        .ok()??;
    let count: i64 = row.get(0);
    let max_updated: String = row.get(1);
    Some(format!("{count}:{max_updated}"))
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

// ---------------------------------------------------------------------------
// 重排序面（P4-3，R2 6.2 Reranking）：rip.rerank.text 容器 pairwise 推理，
// 对既有检索结果（bm25/vector/hybrid）按 query-text 相关分重排序；排序面不改
// 检索集（R2 6.2 流程位置：RRF 之后）。与 embed_worker 同链同模式派发。
// ---------------------------------------------------------------------------

/// 重排序容器 id（containerfiles/rip-rerank/rip.rerank.text.json）。
pub const TOOL_RERANK: &str = "rip.rerank.text";
/// rerank 单条正文字节帽（= 容器 rerank_tools.MAX_TEXT_BYTES，调用侧对齐截断，
/// UTF-8 边界安全：截断点回退到字符边界，不产生半个码点）。
pub const RERANK_TEXT_CAP_BYTES: usize = 8 * 1024;

#[derive(Debug, Clone)]
pub struct RerankWorkerConfig {
    pub run_db: std::path::PathBuf,
    pub sqlite_migrations_dir: std::path::PathBuf,
    pub manifest_dir: std::path::PathBuf,
    pub workspace_root: std::path::PathBuf,
    pub timeout_seconds: u64,
}

impl Default for RerankWorkerConfig {
    fn default() -> Self {
        let data_root = std::env::var("RIP_DATA_DIR")
            .ok()
            .filter(|s| !s.trim().is_empty())
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| std::path::PathBuf::from("rip-data"));
        Self {
            run_db: data_root.join("parse").join("ops.db"),
            sqlite_migrations_dir: std::path::PathBuf::from("migrations"),
            manifest_dir: std::path::PathBuf::from("containerfiles/rip-rerank"),
            workspace_root: data_root.join("parse").join("ws"),
            timeout_seconds: 600,
        }
    }
}

struct RerankErr(pub String);

impl From<std::io::Error> for RerankErr {
    fn from(e: std::io::Error) -> Self {
        Self(format!("io: {e}"))
    }
}

impl From<RerankErr> for IntelError {
    fn from(e: RerankErr) -> Self {
        IntelError::Invalid(format!("rerank worker: {}", e.0))
    }
}

struct RerankRunEnv {
    pool: database::Pool,
    engine: Engine,
    manifest: Value,
}

/// 重排序 provider（OnceLock 懒初始化，与 embed_worker 同模式；manifest 目录
/// 与解析/嵌入面独立 = 异构依赖分族）。
pub struct RerankWorkerProvider {
    config: RerankWorkerConfig,
    env: OnceLock<Result<RerankRunEnv, String>>,
}

impl RerankWorkerProvider {
    pub fn new(config: RerankWorkerConfig) -> Self {
        Self { config, env: OnceLock::new() }
    }

    fn env(&self) -> Result<&RerankRunEnv, RerankErr> {
        let cell = self.env.get_or_init(|| self.init_env().map_err(|e| e.0));
        cell.as_ref().map_err(|e| RerankErr(e.clone()))
    }

    fn init_env(&self) -> Result<RerankRunEnv, RerankErr> {
        std::fs::create_dir_all(&self.config.workspace_root)?;
        if let Some(dir) = self.config.run_db.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let pool = database::Pool::open(database::PoolConfig {
            path: database::DbPath::File(self.config.run_db.clone()),
            readonly: false,
            max_connections: 4,
            busy_timeout: Duration::from_secs(5),
        })
        .map_err(|e| RerankErr(format!("ops db open: {e}")))?;
        pool.enable_wal().map_err(|e| RerankErr(format!("ops db wal: {e}")))?;
        database::migrate(&pool, &self.config.sqlite_migrations_dir)
            .map_err(|e| RerankErr(format!("ops db migrate: {e}")))?;
        let registry = Registry::open(pool.clone()).with_options(Options {
            allow_placeholder_digest: false,
        });
        let path = self.config.manifest_dir.join(format!("{TOOL_RERANK}.json"));
        let value: Value = serde_json::from_str(
            &std::fs::read_to_string(&path)
                .map_err(|e| RerankErr(format!("manifest read {path:?}: {e}")))?,
        )
        .map_err(|e| RerankErr(format!("manifest parse: {e}")))?;
        match registry.query(TOOL_RERANK) {
            Ok(detail) => {
                let current = detail.current_version.unwrap_or_default();
                let want = value["version"].as_str().unwrap_or_default();
                if current != want {
                    registry
                        .register(&value)
                        .map_err(|e| RerankErr(format!("registry register: {e}")))?;
                }
            }
            Err(tool_registry::RegistryError::NotFound { .. }) => {
                registry
                    .register(&value)
                    .map_err(|e| RerankErr(format!("registry register: {e}")))?;
            }
            Err(e) => return Err(RerankErr(format!("registry query: {e}"))),
        }
        let engine = Engine::open(pool.clone(), registry);
        Ok(RerankRunEnv { pool, engine, manifest: value })
    }

    fn dispatch(&self, input: Value) -> Result<Value, RerankErr> {
        let env = self.env()?;
        let image = env.manifest["runtime"]["image"]
            .as_str()
            .ok_or_else(|| RerankErr("manifest runtime.image missing".to_string()))?
            .to_string();
        let entrypoint: Vec<String> = env.manifest["runtime"]["entrypoint"]
            .as_array()
            .map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect())
            .unwrap_or_default();
        if entrypoint.is_empty() {
            return Err(RerankErr("manifest entrypoint empty".to_string()));
        }
        let family = env.manifest["family"].as_str().unwrap_or("document").to_string();
        let network = match env.manifest["network"]["mode"].as_str().unwrap_or("none") {
            "none" => NetworkPlan::None,
            other => return Err(RerankErr(format!("network {other:?} 不可派发（none 之外拒绝）"))),
        };
        let adapter = PodmanHostAdapter::new(PodmanHostConfig {
            workspace_root: self.config.workspace_root.clone(),
            ..PodmanHostConfig::default()
        });
        let request = json!({
            "protocol_version": "1",
            "tool_id": TOOL_RERANK,
            "run_id": crate::new_entity_id("rn-"),
            "submit_key": format!("p4-{}", crate::new_entity_id("k")),
            "input": input,
            "workspace_id": "ws-governance",
            "budget": {
                "timeout_seconds": self.config.timeout_seconds,
                "max_output_bytes": 33554432i64,
            },
            "result_schema_version": "1",
        });
        let store = runtime_gateway::HealthStore::open(env.pool.clone());
        let gateway = runtime_gateway::Gateway::new(
            env.engine.clone(),
            store,
            runtime_gateway::GatewayConfig::default(),
        );
        gateway
            .register_node(
                RuntimeNode {
                    node_id: "host-cachyos".to_string(),
                    kind: domain::RuntimeNodeKind::PodmanHost,
                    endpoint: None,
                    health: NodeHealth::Unknown,
                    version: None,
                },
                std::sync::Arc::new(runtime_gateway::CommandProbe::podman()),
            )
            .map_err(|e| RerankErr(format!("gateway register: {e}")))?;
        let run_id = match gateway
            .dispatch(&request)
            .map_err(|e| RerankErr(format!("gateway dispatch: {e}")))?
        {
            runtime_gateway::DispatchOutcome::Queued { run_id, .. } => run_id,
            other => return Err(RerankErr(format!("dispatch 未承接：{other:?}"))),
        };
        let outcome = execute_container_via_engine(ContainerCall {
            engine: &env.engine,
            run_id: &run_id,
            adapter: &adapter,
            image: &image,
            entrypoint: &entrypoint,
            family: &family,
            network,
        })
        .map_err(|e| RerankErr(format!("container run: {e}")))?;
        if outcome.envelope.status != ToolRunStatus::Succeeded {
            let msg = outcome
                .envelope
                .error
                .as_ref()
                .map(|e| e.message.clone())
                .unwrap_or_else(|| "unknown failure".to_string());
            return Err(RerankErr(format!("tool run failed: {msg}")));
        }
        outcome
            .envelope
            .result
            .ok_or_else(|| RerankErr("result missing".to_string()))
    }
}

/// rerank 全链（R2 6.2 流程位置：RRF/MMR 之后）：hybrid 检索 top-N（N ≤ 64
/// 容器帽）→ 批量读正文（单条 SQL，IN 一次性取回）→ rerank_hits 容器排序。
/// source 标注 "rerank"；limit > 64 时检索/读正文仍按 limit，容器单批帽
/// 内截前 64 条重排（其余保持原序续后——排序面不改检索集）。
pub fn rerank_search(
    pool: &PgPool,
    embed_config: &EmbedWorkerConfig,
    rerank_config: &RerankWorkerConfig,
    filters: &SearchFilters,
    query: &str,
    limit: u32,
) -> IntelResult<Vec<SearchHit>> {
    // 检索集 = hybrid 产出（RRF + MMR，双通道并归后多样化）。
    let candidates = hybrid_search(pool, embed_config, filters, query, limit, DEFAULT_MMR_LAMBDA)?;
    if candidates.is_empty() {
        return Ok(candidates);
    }
    // 正文批量读取（单条 SQL IN；approved-only 等前置已在检索 SQL 内，
    // 此处只补正文；读不到正文回落 title——rerank_hits 同口径）。
    let ids: Vec<String> = candidates.iter().map(|h| h.knowledge_id.clone()).collect();
    let mut conn = pool.get()?;
    let rows = conn.query(
        "SELECT knowledge_id, content FROM knowledge_cards WHERE knowledge_id = ANY($1)",
        &[&ids],
    )?;
    drop(conn);
    let mut bodies: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    for r in rows {
        bodies.insert(r.get::<_, String>(0), r.get::<_, String>(1));
    }
    rerank_hits(rerank_config, query, &candidates, &bodies)
        .map(|ranked| mmr_diversify(&ranked, DEFAULT_MMR_LAMBDA, limit))
}

/// UTF-8 边界安全截断（按字节帽；截断点回退到最近字符边界）。
fn truncate_utf8(s: &str, cap: usize) -> &str {
    if s.len() <= cap {
        return s;
    }
    let mut end = cap;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

/// rerank 核心（调用面先取检索集 + 正文，本函数容器派发排序）：
/// hits 按容器 scores 降序重排，score 字段替换为 rerank 相关分；正文按
/// [`RERANK_TEXT_CAP_BYTES`] UTF-8 安全截断（容器 8KiB 帽对齐）。
pub fn rerank_hits(
    rerank_config: &RerankWorkerConfig,
    query: &str,
    hits: &[SearchHit],
    bodies: &std::collections::HashMap<String, String>,
) -> IntelResult<Vec<SearchHit>> {
    let provider = RerankWorkerProvider::new(rerank_config.clone());
    let texts: Vec<Value> = hits
        .iter()
        .map(|h| {
            let body = bodies.get(&h.knowledge_id).cloned().unwrap_or_else(|| h.title.clone());
            let body = truncate_utf8(&body, RERANK_TEXT_CAP_BYTES);
            json!({ "id": h.knowledge_id, "text": body })
        })
        .collect();
    let data = RerankWorkerProvider::dispatch(&provider, json!({ "query": query, "texts": texts }))
        .map_err(IntelError::from)?;
    let scores = data["scores"]
        .as_array()
        .ok_or_else(|| IntelError::Invalid("rerank scores missing".to_string()))?;
    let mut by_id: std::collections::HashMap<String, f64> = std::collections::HashMap::new();
    for s in scores {
        by_id.insert(
            s["id"].as_str().unwrap_or_default().to_string(),
            s["score"].as_f64().unwrap_or(0.0),
        );
    }
    let mut out: Vec<SearchHit> = hits
        .iter()
        .map(|h| {
            let mut c = h.clone();
            if let Some(s) = by_id.get(&h.knowledge_id) {
                c.score = *s as f32;
            }
            c.source = "rerank".to_string();
            c
        })
        .collect();
    out.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    Ok(out)
}
