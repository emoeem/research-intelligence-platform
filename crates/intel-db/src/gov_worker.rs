//! 治理 worker 编排面（Phase 3 P3-2，D3 近似重复）：rip.gov.minhash 容器经
//! 既有 T1 podman_host 通道派发（submit → Gateway::dispatch →
//! execute_container_via_engine，与 parser.rs 同链不复刻隔离逻辑），批量
//! MinHash+LSH 候选对 → NEAR_DUPLICATE 候选边（不自动合并，R2 5.2）。
//!
//! 与 parser.rs 的分工：parser 面携带输入文件与 staging 资产；本面为纯内联
//! 计算（卡片文本进 items，候选对回内存），无输入文件与产物拷出。

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

pub const TOOL_MINHASH: &str = "rip.gov.minhash";

/// 单对候选（jaccard 估计值随行进 relation_evidence）。
#[derive(Debug, Clone, serde::Serialize)]
pub struct MinhashPair {
    pub a: String,
    pub b: String,
    pub jaccard_est: f64,
}

#[derive(Debug, serde::Serialize)]
pub struct MinhashScanOutcome {
    /// 参与扫描的卡片数（REJECTED 排除，空正文排除）。
    pub scanned: usize,
    /// 容器产出的候选对数。
    pub pairs: usize,
    /// 新建的 NEAR_DUPLICATE 候选边数（幂等去重后）。
    pub relations_created: usize,
    /// 容器召回但库内已不存在的 id 数（如实计）。
    pub skipped_missing: usize,
}

#[derive(Debug, Clone)]
pub struct GovWorkerConfig {
    pub run_db: PathBuf,
    pub sqlite_migrations_dir: PathBuf,
    pub manifest_dir: PathBuf,
    pub workspace_root: PathBuf,
    pub timeout_seconds: u64,
}

impl Default for GovWorkerConfig {
    fn default() -> Self {
        let data_root = std::env::var("RIP_DATA_DIR")
            .ok()
            .filter(|s| !s.trim().is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("rip-data"));
        Self {
            run_db: data_root.join("parse").join("ops.db"),
            sqlite_migrations_dir: PathBuf::from("migrations"),
            manifest_dir: PathBuf::from("containerfiles/rip-gov"),
            workspace_root: data_root.join("parse").join("ws"),
            timeout_seconds: 600,
        }
    }
}

struct RunEnv {
    pool: database::Pool,
    engine: Engine,
    manifest: Value,
}

pub struct GovWorkerProvider {
    config: GovWorkerConfig,
    env: OnceLock<Result<RunEnv, String>>,
}

impl GovWorkerProvider {
    pub fn new(config: GovWorkerConfig) -> Self {
        Self { config, env: OnceLock::new() }
    }

    fn env(&self) -> Result<&RunEnv, ParserErr> {
        let cell = self
            .env
            .get_or_init(|| self.init_env().map_err(|e| e.0));
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
        let path = self.config.manifest_dir.join(format!("{TOOL_MINHASH}.json"));
        let value: Value = serde_json::from_str(
            &std::fs::read_to_string(&path)
                .map_err(|e| ParserErr(format!("manifest read {path:?}: {e}")))?,
        )
        .map_err(|e| ParserErr(format!("manifest parse: {e}")))?;
        match registry.query(TOOL_MINHASH) {
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
            "tool_id": TOOL_MINHASH,
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

struct ParserErr(pub String);

impl From<std::io::Error> for ParserErr {
    fn from(e: std::io::Error) -> Self {
        Self(format!("io: {e}"))
    }
}

impl From<ParserErr> for IntelError {
    fn from(e: ParserErr) -> Self {
        IntelError::Invalid(format!("gov worker: {}", e.0))
    }
}

/// D3 近似重复扫描：库内非 REJECTED 卡片 → 容器 MinHash+LSH → NEAR_DUPLICATE
/// 候选边（幂等；合并判定走审核，R2 5.2 硬性）。
pub fn minhash_scan(
    pool: &PgPool,
    workspace_id: &str,
    threshold: f64,
    num_perm: u32,
    config: &GovWorkerConfig,
) -> IntelResult<MinhashScanOutcome> {
    let provider = GovWorkerProvider::new(config.clone());
    let mut conn = pool.get()?;
    let rows = conn.query(
        "SELECT knowledge_id, content FROM knowledge_cards
         WHERE workspace_id = $1 AND review_status <> 'REJECTED' AND content <> ''
         ORDER BY knowledge_id",
        &[&workspace_id],
    )?;
    drop(conn);
    let items: Vec<Value> = rows
        .iter()
        .map(|r| {
            let id: String = r.get(0);
            let text: String = r.get(1);
            json!({ "id": id, "text": text })
        })
        .collect();
    if items.is_empty() {
        return Ok(MinhashScanOutcome { scanned: 0, pairs: 0, relations_created: 0, skipped_missing: 0 });
    }
    let scanned = items.len();
    let data = provider
        .dispatch(json!({
            "items": items,
            "num_perm": num_perm,
            "threshold": threshold,
        }))
        .map_err(IntelError::from)?;
    let pairs_json = data
        .get("pairs")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut skipped_missing = 0usize;
    let mut relations_created = 0usize;
    for pair in pairs_json {
        let a = pair["a"].as_str().unwrap_or_default().to_string();
        let b = pair["b"].as_str().unwrap_or_default().to_string();
        let jaccard = pair["jaccard_est"].as_f64().unwrap_or(0.0);
        let (Ok(src), Ok(dst)) = (
            KnowledgeCardId::try_new(a.clone()),
            KnowledgeCardId::try_new(b.clone()),
        ) else {
            skipped_missing += 1;
            continue;
        };
        // 容器返回的 id 来自本扫描集；对已删除卡 add_relation 会因 FK 失败，
        // 如实计 skipped（候选面不静默）。
        let outcome = card_repo::add_relation(
            pool,
            &src,
            &dst,
            RelationType::NearDuplicate,
            Some(&json!({
                "rule": "D3 minhash+lsh",
                "jaccard_est": jaccard,
                "threshold": threshold,
                "num_perm": num_perm,
            })),
            "gov_worker.minhash:0.1.0",
            jaccard as f32,
        );
        match outcome {
            Ok((_, true)) => relations_created += 1,
            Ok((_, false)) => {}
            Err(_) => skipped_missing += 1,
        }
    }
    let pairs_count = data["pairs"].as_array().map(|a| a.len()).unwrap_or(0);
    Ok(MinhashScanOutcome {
        scanned,
        pairs: pairs_count,
        relations_created,
        skipped_missing,
    })
}
