//! 语义嵌入编排面（Phase 3 P3-3，D4 语义候选）：rip.embed.text 容器（bge-
//! small-zh-v1.5 ONNX，D7 本地推理，模型构建期烘焙）经既有 T1 podman_host
//! 通道派发；卡片向量落 knowledge_card_vectors（V0005，pgvector HNSW）；
//! cosine 相似候选 → NEAR_DUPLICATE 候选边（**禁自动合并**，R2 5.2 D4 硬性；
//! relation_source 与 D3 区分）。
//!
//! 与 gov_worker.rs 同链同模式（submit → Gateway dispatch →
//! execute_container_via_engine），容器族拆分：rip-gov（datasketch 词法面）
//! vs rip-embed（ONNX 语义面），异构依赖分族。

use crate::card_repo;
use crate::{IntelError, IntelResult, PgPool};
use domain::kcard::{KnowledgeCardId, RelationType};
use domain::{NodeHealth, RuntimeNode, ToolRunStatus};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::Duration;
use tool_engine::adapter::{
    execute_container_via_engine, ContainerCall, PodmanHostAdapter, PodmanHostConfig,
};
use tool_engine::Engine;
use tool_registry::{Options, Registry};

pub const TOOL_EMBED: &str = "rip.embed.text";
/// D4 候选阈值（cosine 相似；0.92 对 bge-small-zh 近重复中文文本为保守召回线，
/// 可调参数留在调用面）。
pub const DEFAULT_SEMANTIC_THRESHOLD: f32 = 0.92;
const EMBED_MODEL_ID: &str = "bge-small-zh-v1.5";
const EMBED_VERSION: &str = "0.2.0";
const EMBED_BATCH: usize = 64;

#[derive(Debug, serde::Serialize)]
pub struct EmbedOutcome {
    /// 参与嵌入的卡片数。
    pub embedded: usize,
    /// 已有向量跳过数（幂等面）。
    pub skipped_existing: usize,
}

#[derive(Debug, serde::Serialize)]
pub struct SemanticOutcome {
    /// 参与近邻搜索的向量数。
    pub searched: usize,
    /// 阈值上的候选对数（容器面产出）。
    pub pairs: usize,
    /// 新建 NEAR_DUPLICATE 候选边数（幂等去重后）。
    pub relations_created: usize,
}

#[derive(Debug, Clone)]
pub struct EmbedWorkerConfig {
    pub run_db: PathBuf,
    pub sqlite_migrations_dir: PathBuf,
    pub manifest_dir: PathBuf,
    pub workspace_root: PathBuf,
    pub timeout_seconds: u64,
}

impl Default for EmbedWorkerConfig {
    fn default() -> Self {
        let data_root = std::env::var("RIP_DATA_DIR")
            .ok()
            .filter(|s| !s.trim().is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("rip-data"));
        Self {
            run_db: data_root.join("parse").join("ops.db"),
            sqlite_migrations_dir: PathBuf::from("migrations"),
            manifest_dir: PathBuf::from("containerfiles/rip-embed"),
            workspace_root: data_root.join("parse").join("ws"),
            timeout_seconds: 600,
        }
    }
}

struct ParserErr(pub String);

impl From<std::io::Error> for ParserErr {
    fn from(e: std::io::Error) -> Self {
        Self(format!("io: {e}"))
    }
}

impl From<ParserErr> for IntelError {
    fn from(e: ParserErr) -> Self {
        IntelError::Invalid(format!("embed worker: {}", e.0))
    }
}

struct RunEnv {
    pool: database::Pool,
    engine: Engine,
    manifest: Value,
}

pub struct EmbedWorkerProvider {
    config: EmbedWorkerConfig,
    env: OnceLock<Result<RunEnv, String>>,
}

impl EmbedWorkerProvider {
    pub fn new(config: EmbedWorkerConfig) -> Self {
        Self { config, env: OnceLock::new() }
    }

    fn env(&self) -> Result<&RunEnv, ParserErr> {
        let cell = self.env.get_or_init(|| self.init_env().map_err(|e| e.0));
        cell.as_ref().map_err(|e| ParserErr(e.clone()))
    }

    fn init_env(&self) -> Result<RunEnv, ParserErr> {
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
        .map_err(|e| ParserErr(format!("ops db open: {e}")))?;
        pool.enable_wal().map_err(|e| ParserErr(format!("ops db wal: {e}")))?;
        database::migrate(&pool, &self.config.sqlite_migrations_dir)
            .map_err(|e| ParserErr(format!("ops db migrate: {e}")))?;
        let registry = Registry::open(pool.clone()).with_options(Options {
            allow_placeholder_digest: false,
        });
        let path = self.config.manifest_dir.join(format!("{TOOL_EMBED}.json"));
        let value: Value = serde_json::from_str(
            &std::fs::read_to_string(&path)
                .map_err(|e| ParserErr(format!("manifest read {path:?}: {e}")))?,
        )
        .map_err(|e| ParserErr(format!("manifest parse: {e}")))?;
        match registry.query(TOOL_EMBED) {
            Ok(detail) => {
                let current = detail.current_version.unwrap_or_default();
                let want = value["version"].as_str().unwrap_or_default();
                if current != want {
                    registry
                        .register(&value)
                        .map_err(|e| ParserErr(format!("registry register: {e}")))?;
                }
            }
            Err(tool_registry::RegistryError::NotFound { .. }) => {
                registry
                    .register(&value)
                    .map_err(|e| ParserErr(format!("registry register: {e}")))?;
            }
            Err(e) => return Err(ParserErr(format!("registry query: {e}"))),
        }
        let engine = Engine::open(pool.clone(), registry);
        Ok(RunEnv { pool, engine, manifest: value })
    }

    fn dispatch(&self, input: Value) -> Result<Value, ParserErr> {
        let env = self.env()?;
        let image = env.manifest["runtime"]["image"]
            .as_str()
            .ok_or_else(|| ParserErr("manifest runtime.image missing".to_string()))?
            .to_string();
        let entrypoint: Vec<String> = env.manifest["runtime"]["entrypoint"]
            .as_array()
            .map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect())
            .unwrap_or_default();
        if entrypoint.is_empty() {
            return Err(ParserErr("manifest entrypoint empty".to_string()));
        }
        let family = env.manifest["family"].as_str().unwrap_or("document").to_string();
        let network = match env.manifest["network"]["mode"].as_str().unwrap_or("none") {
            "none" => tool_engine::adapter::NetworkPlan::None,
            other => return Err(ParserErr(format!("network {other:?} 不可派发（none 之外拒绝）"))),
        };
        let memory_bytes = env.manifest["resources"]["memory"]
            .as_str()
            .and_then(|s| {
                let upper = s.trim().to_ascii_uppercase();
                let (n, m) = if let Some(n) = upper.strip_suffix("GI") {
                    (n, 1024u64 * 1024 * 1024)
                } else if let Some(n) = upper.strip_suffix("G") {
                    (n, 1024u64 * 1024 * 1024)
                } else if let Some(n) = upper.strip_suffix("MI") {
                    (n, 1024u64 * 1024)
                } else if let Some(n) = upper.strip_suffix("M") {
                    (n, 1024u64 * 1024)
                } else {
                    (upper.as_str(), 1)
                };
                n.trim().parse::<u64>().ok().map(|v| v * m)
            })
            .unwrap_or(2 * 1024 * 1024 * 1024);
        let adapter = PodmanHostAdapter::new(PodmanHostConfig {
            workspace_root: self.config.workspace_root.clone(),
            memory_bytes,
            ..PodmanHostConfig::default()
        });
        let request = json!({
            "protocol_version": "1",
            "tool_id": TOOL_EMBED,
            "run_id": crate::new_entity_id("rn-"),
            "submit_key": format!("p3-{}", crate::new_entity_id("k")),
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
            .map_err(|e| ParserErr(format!("gateway register: {e}")))?;
        let run_id = match gateway
            .dispatch(&request)
            .map_err(|e| ParserErr(format!("gateway dispatch: {e}")))?
        {
            runtime_gateway::DispatchOutcome::Queued { run_id, .. } => run_id,
            other => return Err(ParserErr(format!("dispatch 未承接：{other:?}"))),
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
        .map_err(|e| ParserErr(format!("container run: {e}")))?;
        if outcome.envelope.status != ToolRunStatus::Succeeded {
            let msg = outcome
                .envelope
                .error
                .as_ref()
                .map(|e| e.message.clone())
                .unwrap_or_else(|| "unknown failure".to_string());
            return Err(ParserErr(format!("tool run failed: {msg}")));
        }
        outcome
            .envelope
            .result
            .ok_or_else(|| ParserErr("result missing".to_string()))
    }
}

/// 批量嵌入卡片（缺向量者；64 条/批，容器内逐条推理 L2 归一化）→
/// knowledge_card_vectors upsert（幂等）。
pub fn embed_cards(
    pool: &PgPool,
    workspace_id: &str,
    config: &EmbedWorkerConfig,
) -> IntelResult<EmbedOutcome> {
    let provider = EmbedWorkerProvider::new(config.clone());
    // 已有向量数即跳过数（先查，clippy late-init 口径）。
    let skipped_existing = {
        let mut count_conn = pool.get()?;
        count_conn
            .query_one(
                "SELECT COUNT(*) FROM knowledge_card_vectors v
                 JOIN knowledge_cards c ON c.knowledge_id = v.card_id
                 WHERE c.workspace_id = $1",
                &[&workspace_id],
            )?
            .get::<_, i64>(0) as usize
    };
    let mut conn = pool.get()?;
    let rows = conn.query(
        "SELECT c.knowledge_id, c.content FROM knowledge_cards c
         LEFT JOIN knowledge_card_vectors v ON v.card_id = c.knowledge_id
         WHERE c.workspace_id = $1 AND c.review_status <> 'REJECTED'
           AND c.content <> '' AND v.card_id IS NULL
         ORDER BY c.knowledge_id",
        &[&workspace_id],
    )?;
    drop(conn);
    let mut embedded = 0usize;
    let mut pending: Vec<(String, String)> = rows
        .iter()
        .map(|r| (r.get::<_, String>(0), r.get::<_, String>(1)))
        .collect();
    while !pending.is_empty() {
        let batch: Vec<(String, String)> = pending.drain(..EMBED_BATCH.min(pending.len())).collect();
        let texts: Vec<Value> = batch.iter().map(|(_, t)| json!(t)).collect();
        let data = provider
            .dispatch(json!({ "texts": texts }))
            .map_err(IntelError::from)?;
        let dim = data["dim"].as_i64().unwrap_or(0);
        if dim != 512 {
            return Err(IntelError::Invalid(format!(
                "embedding dim mismatch: {dim} != 512 (V0005 schema)"
            )));
        }
        let vectors = data["vectors"]
            .as_array()
            .ok_or_else(|| IntelError::Invalid("vectors missing".to_string()))?;
        if vectors.len() != batch.len() {
            return Err(IntelError::Invalid(format!(
                "vectors length {} != texts length {}",
                vectors.len(),
                batch.len()
            )));
        }
        let mut conn = pool.get()?;
        let mut tx = conn.transaction()?;
        for ((card_id, _), v) in batch.iter().zip(vectors.iter()) {
            let vals: Vec<f32> = v
                .as_array()
                .expect("vector array")
                .iter()
                .map(|x| x.as_f64().unwrap_or(0.0) as f32)
                .collect();
            let vector = pgvector::Vector::from(vals);
            tx.execute(
                "INSERT INTO knowledge_card_vectors (card_id, embedding, embedding_model_id,
                     embedding_version, dimension)
                 VALUES ($1,$2,$3,$4,512)
                 ON CONFLICT (card_id) DO UPDATE SET embedding = EXCLUDED.embedding,
                     embedding_version = EXCLUDED.embedding_version, updated_at = now()",
                &[&card_id, &vector, &EMBED_MODEL_ID, &EMBED_VERSION],
            )?;
            embedded += 1;
        }
        tx.commit()?;
    }
    Ok(EmbedOutcome { embedded, skipped_existing })
}

/// D4 语义候选扫描：每个有向量卡片取 top-K 余弦近邻（阈值上）→
/// NEAR_DUPLICATE 候选边（relation_source = gov_worker.semantic:0.1.0，与
/// D3 词法面区分；**禁自动合并**，R2 5.2 D4 硬性）。
pub fn semantic_scan(
    pool: &PgPool,
    workspace_id: &str,
    threshold: f32,
    top_k: u32,
    config: &EmbedWorkerConfig,
) -> IntelResult<SemanticOutcome> {
    // 先补齐向量（幂等）。
    embed_cards(pool, workspace_id, config)?;
    let mut conn = pool.get()?;
    let cards: Vec<String> = conn
        .query(
            "SELECT v.card_id FROM knowledge_card_vectors v
             JOIN knowledge_cards c ON c.knowledge_id = v.card_id
             WHERE c.workspace_id = $1 AND c.review_status <> 'REJECTED'
             ORDER BY v.card_id",
            &[&workspace_id],
        )?
        .iter()
        .map(|r| r.get(0))
        .collect();
    drop(conn);
    let searched = cards.len();
    let mut relations_created = 0usize;
    let mut pairs = 0usize;
    let mut stmt_pool = pool.get()?;
    for card_id in &cards {
        let card = KnowledgeCardId::try_new(card_id.clone()).map_err(IntelError::from)?;

        // cosine distance（pgvector <=> = 1 - cosine similarity）；CTE 按 card_id
        // 取锚点向量（$1 文本参数复用，无向量参数绑定——坑位实录：CTE 内未
        // 类型化参数被推断 text，向量参数绑定必须显式 ::vector 或按列取）。
        let rows = stmt_pool.query(
            "WITH q AS (SELECT embedding FROM knowledge_card_vectors WHERE card_id = $1)
             SELECT v.card_id, v.embedding <=> q.embedding AS dist
             FROM knowledge_card_vectors v
             JOIN knowledge_cards c ON c.knowledge_id = v.card_id
             CROSS JOIN q
             WHERE v.card_id <> $1 AND c.workspace_id = $2 AND c.review_status <> 'REJECTED'
             ORDER BY dist LIMIT $3",
            &[&card.as_str(), &workspace_id, &(top_k as i64)],
        )?;
        for r in rows {
            let other_id: String = r.get(0);
            let dist: f64 = r.get(1);
            let sim = (1.0 - dist) as f32;
            if sim < threshold {
                continue;
            }
            pairs += 1;
            let src = KnowledgeCardId::try_new(card_id.clone()).map_err(IntelError::from)?;
            let dst = KnowledgeCardId::try_new(other_id).map_err(IntelError::from)?;
            // 无向候选统一 (min,max) 有向化（字典序），幂等语义稳定。
            let (src, dst) = if src.as_str() < dst.as_str() { (src, dst) } else { (dst, src) };
            let outcome = card_repo::add_relation(
                pool,
                &src,
                &dst,
                RelationType::NearDuplicate,
                Some(&json!({
                    "rule": "D4 semantic cosine",
                    "cosine_sim": sim,
                    "threshold": threshold,
                    "model": EMBED_MODEL_ID,
                })),
                "gov_worker.semantic:0.1.0",
                sim,
            );
            if matches!(outcome, Ok((_, true))) {
                relations_created += 1;
            }
        }
    }
    Ok(SemanticOutcome { searched, pairs, relations_created })
}
