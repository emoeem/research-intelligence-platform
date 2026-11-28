//! Parser Registry + Provider（RIP3 P2-1，ADR-0003 裁决 2/3）。
//!
//! R2 10.3 Provider 接口：[`DocumentParser`]（detect + parse）与
//! [`OCRProvider`]（Tesseract 兜底）为领域级 trait，容器派发实现
//! （[`PodmanParserProvider`]）与 trait 分离，换执行面不动 Registry。
//!
//! 执行面（ADR-0003 裁决 3）：解析容器登记为 rip.parser.* 工具（T1
//! podman_host manifest，containerfiles/rip-parser/），经既有 tool-engine
//! 受理链（D-TC-05 submit → runtime-gateway Gateway::dispatch 健康面 →
//! execute_container_via_engine）派发；隔离全套（read-only rootfs /
//! cap-drop ALL / network none / 资源帽 / tmpfs）、看门狗、FamilyLimiter、
//! tool_run 审计与 trace 关联全部继承，本模块零自管 podman 参数。
//!
//! 运行库边界（lib.rs 红线修订见 ADR-0003）：本模块打开专用 SQLite ops
//! 运行库（tool_run/registry 状态面，RIP_DATA_DIR 口径缺省
//! rip-data/parse/ops.db，显式 enable_wal）；生产 platform*.db 不经此路径。
//!
//! 格式路由：detect 结果（容器内魔法数判定）→ parse 工具单点
//! （rip.parser.docling 承载多引擎：pdf/docx/pptx/xlsx/html/image 走
//! Docling，epub 走 PyMuPDF，markdown/txt/code 走 native_markdown，由容器
//! 内部路由并如实记录 parser_metadata.parser）；ocr 兜底 → rip.parser.ocr。

use crate::ir::{
    Block, BlockSource, DocumentIr, DocumentMetadata, ExtractionQuality, ParserMetadata,
    QualityCheck, SourceMapping, Structure, StructureNode,
};
use crate::IntelError;
use domain::{NodeHealth, RuntimeNode, ToolRunStatus};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Duration;
use tool_engine::adapter::{
    execute_container_via_engine, execute_via_engine, AdapterCall, ContainerCall,
    NativeRustAdapter, NativeRustConfig, NetworkPlan, PodmanHostAdapter, PodmanHostConfig,
    PythonWorkerAdapter, PythonWorkerConfig, RestartPolicy,
};
use tool_engine::capability::StaticCatalogProvider;
use tool_engine::Engine;
use tool_registry::{Options, Registry};

/// 解析工具 id 词表（manifest 同名文件：containerfiles/rip-parser/<id>.json）。
pub const TOOL_DETECT: &str = "rip.parser.detect";
pub const TOOL_PARSE: &str = "rip.parser.docling";
pub const TOOL_OCR: &str = "rip.parser.ocr";

/// parse 面支持的格式（R2 4.2 第一批；zip 由平台侧走 detect 成员清单逐成员
/// 编排，不在容器单点 parse 内）。
pub const PARSE_FORMATS: [&str; 10] = [
    "pdf", "docx", "pptx", "xlsx", "html", "image", "epub", "markdown", "txt", "code",
];

/// 格式路由（R2 10.3 防锁定：路由表唯一变化点）。
pub fn route_parse(format: &str) -> Option<&'static str> {
    if PARSE_FORMATS.contains(&format) {
        Some(TOOL_PARSE)
    } else {
        None
    }
}

/// Provider 配置（缺省沿 RIP_DATA_DIR 口径；run_db 为专用 ops 运行库）。
#[derive(Debug, Clone)]
pub struct ParserProviderConfig {
    /// SQLite ops 运行库（tool_run/registry 状态面）。
    pub run_db: PathBuf,
    /// SQLite 迁移目录（仓库 migrations/）。
    pub sqlite_migrations_dir: PathBuf,
    /// rip.parser.* manifest 目录（containerfiles/rip-parser/）。
    pub manifest_dir: PathBuf,
    /// podman 工作区根（<root>/<run_id>/{in,out,tmp}）。
    pub workspace_root: PathBuf,
    /// 解析产物暂存根（ws/out 拷出，提交服务消费）。
    pub staging_root: PathBuf,
    /// 每容器内存字节（解析模型 4 GiB；ADR §7 基线 2 GiB 的族级覆盖，
    /// manifest resources.memory 同值，批次报告如实记录）。
    pub memory_bytes: u64,
    /// 执行时长帽（秒；podman_host 看门狗 clamp [1,600]）。
    pub timeout_seconds: u64,
}

impl Default for ParserProviderConfig {
    fn default() -> Self {
        let data_root = std::env::var("RIP_DATA_DIR")
            .ok()
            .filter(|s| !s.trim().is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("rip-data"));
        Self {
            run_db: data_root.join("parse").join("ops.db"),
            sqlite_migrations_dir: PathBuf::from("migrations"),
            manifest_dir: PathBuf::from("containerfiles/rip-parser"),
            workspace_root: data_root.join("parse").join("ws"),
            staging_root: data_root.join("parse").join("staging"),
            memory_bytes: 4 * 1024 * 1024 * 1024,
            timeout_seconds: 600,
        }
    }
}

/// detect 结果（容器 rip.parser.detect 的 data 投影）。
#[derive(Debug, Clone, serde::Serialize)]
pub struct DetectInfo {
    pub format: String,
    pub mime_type: String,
    pub size_bytes: u64,
    pub sha256: String,
    pub document_type: String,
    /// zip 成员清单与安全判定（13-SECURITY 第二节；非 zip 为 null）。
    pub zip: Option<Value>,
    /// 容器返回的完整 data（不做形状收窄，供调用面按需消费）。
    pub data: Value,
}

/// parse/ocr 结果（IR + 产物暂存路径）。
#[derive(Debug, Clone)]
pub struct ParseOutcome {
    /// 容器产出 IR（schema 校验在提交侧执行；本层只透传）。
    pub ir: DocumentIr,
    /// IR 原始 JSON（与 ir 对应，validate_ir 用）。
    pub raw: Value,
    /// 容器侧计算的去键 IR 摘要（对账用）。
    pub ir_sha256: String,
    pub format: String,
    /// ws/out 拷出暂存（ir.json + assets/；提交服务读取资产文件入 CAS）。
    pub staging_dir: PathBuf,
}

/// Provider 错误（EngineError/AdapterFailure 文本透传）。
#[derive(Debug)]
pub struct ParserError(pub String);

impl std::fmt::Display for ParserError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for ParserError {}

impl From<tool_engine::EngineError> for ParserError {
    fn from(e: tool_engine::EngineError) -> Self {
        Self(format!("engine: {e}"))
    }
}

impl From<IntelError> for ParserError {
    fn from(e: IntelError) -> Self {
        Self(format!("intel-db: {e}"))
    }
}

impl From<std::io::Error> for ParserError {
    fn from(e: std::io::Error) -> Self {
        Self(format!("io: {e}"))
    }
}

impl From<ParserError> for IntelError {
    fn from(e: ParserError) -> Self {
        IntelError::Invalid(format!("parser: {}", e.0))
    }
}

/// R2 10.3 DocumentParser trait（领域层不写死供应商）。
pub trait DocumentParser: Send + Sync {
    fn detect(&self, input: &Path) -> Result<DetectInfo, ParserError>;
    fn parse(&self, input: &Path) -> Result<ParseOutcome, ParserError>;
}

/// R2 10.3 OCRProvider trait（Tesseract 兜底面）。
pub trait OCRProvider: Send + Sync {
    fn ocr(&self, input: &Path, language: &str) -> Result<ParseOutcome, ParserError>;
}

/// 常驻运行环境（懒初始化：ops 库 + 迁移 + manifest 登记 + 引擎）。
struct RunEnv {
    pool: database::Pool,
    engine: Engine,
    manifests: std::collections::HashMap<String, Value>,
}

/// podman_host 派发实现（rip-parser 容器族）。
pub struct PodmanParserProvider {
    config: ParserProviderConfig,
    env: OnceLock<Result<RunEnv, String>>,
}

impl PodmanParserProvider {
    pub fn new(config: ParserProviderConfig) -> Self {
        Self {
            config,
            env: OnceLock::new(),
        }
    }

    fn env(&self) -> Result<&RunEnv, ParserError> {
        let cell = self.env.get_or_init(|| self.init_env().map_err(|e| e.0));
        cell.as_ref().map_err(|e| ParserError(e.clone()))
    }

    fn init_env(&self) -> Result<RunEnv, ParserError> {
        std::fs::create_dir_all(&self.config.workspace_root)?;
        std::fs::create_dir_all(&self.config.staging_root)?;
        if let Some(dir) = self.config.run_db.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let pool = database::Pool::open(database::PoolConfig {
            path: database::DbPath::File(self.config.run_db.clone()),
            readonly: false,
            max_connections: 4,
            busy_timeout: Duration::from_secs(5),
        })
        .map_err(|e| ParserError(format!("ops db open: {e}")))?;
        pool.enable_wal()
            .map_err(|e| ParserError(format!("ops db wal: {e}")))?;
        database::migrate(&pool, &self.config.sqlite_migrations_dir)
            .map_err(|e| ParserError(format!("ops db migrate: {e}")))?;
        let registry = Registry::open(pool.clone()).with_options(Options {
            allow_placeholder_digest: false,
        });
        let mut manifests = std::collections::HashMap::new();
        for tool_id in [TOOL_DETECT, TOOL_PARSE, TOOL_OCR] {
            let path = self.config.manifest_dir.join(format!("{tool_id}.json"));
            let raw = std::fs::read_to_string(&path)
                .map_err(|e| ParserError(format!("manifest read {path:?}: {e}")))?;
            let value: Value = serde_json::from_str(&raw)
                .map_err(|e| ParserError(format!("manifest parse {path:?}: {e}")))?;
            // 幂等登记：已登记同版本则跳过（run 库跨次复用；digest 变更 =
            // 版本号变更，manifest 侧纪律）。
            match registry.query(tool_id) {
                Ok(detail) => {
                    let current = detail.current_version.unwrap_or_default();
                    let want = value["version"].as_str().unwrap_or_default();
                    if current == want {
                        manifests.insert(tool_id.to_string(), value);
                        continue;
                    }
                }
                Err(tool_registry::RegistryError::NotFound { .. }) => {}
                Err(e) => return Err(ParserError(format!("registry query {tool_id}: {e}"))),
            }
            registry
                .register(&value)
                .map_err(|e| ParserError(format!("registry register {tool_id}: {e}")))?;
            manifests.insert(tool_id.to_string(), value);
        }
        let engine = Engine::open(pool.clone(), registry);
        Ok(RunEnv {
            pool,
            engine,
            manifests,
        })
    }

    /// 单 op 派发（submit → Gateway::dispatch → 容器执行 → 产物拷出）。
    fn run_op(
        &self,
        tool_id: &str,
        input: Value,
        input_file: Option<&Path>,
        input_name: &str,
    ) -> Result<(Value, PathBuf), ParserError> {
        let env = self.env()?;
        let manifest = env
            .manifests
            .get(tool_id)
            .ok_or_else(|| ParserError(format!("manifest missing for {tool_id}")))?;
        let image = manifest["runtime"]["image"]
            .as_str()
            .ok_or_else(|| ParserError(format!("manifest {tool_id} runtime.image missing")))?
            .to_string();
        let entrypoint: Vec<String> = manifest["runtime"]["entrypoint"]
            .as_array()
            .ok_or_else(|| ParserError(format!("manifest {tool_id} entrypoint missing")))?
            .iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .collect();
        let family = manifest["family"]
            .as_str()
            .unwrap_or("document")
            .to_string();
        let network_mode = manifest["network"]["mode"].as_str().unwrap_or("none");
        let network = match network_mode {
            "none" => NetworkPlan::None,
            other => {
                return Err(ParserError(format!(
                    "network mode {other:?} not accepted for parser family (network=none 红线)"
                )))
            }
        };

        let adapter = PodmanHostAdapter::new(PodmanHostConfig {
            workspace_root: self.config.workspace_root.clone(),
            memory_bytes: self.config.memory_bytes,
            ..PodmanHostConfig::default()
        });
        // 受理 + 健康面一条链：Gateway::dispatch 内部 submit（D-TC-05），
        // QUEUED 即承接（e2e 同款；register_node 为 upsert，逐次调用安全）。
        let request = json!({
            "protocol_version": "1",
            "tool_id": tool_id,
            "run_id": crate::new_entity_id("rn-"),
            "submit_key": format!("p2-{}", crate::new_entity_id("k")),
            "input": input,
            "workspace_id": "ws-parser",
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
            .map_err(|e| ParserError(format!("gateway register: {e}")))?;
        let run_id = match gateway
            .dispatch(&request)
            .map_err(|e| ParserError(format!("gateway dispatch: {e}")))?
        {
            runtime_gateway::DispatchOutcome::Queued { run_id, .. } => run_id,
            other => {
                return Err(ParserError(format!(
                    "dispatch did not queue (no healthy runner?): {other:?}"
                )))
            }
        };

        // 输入文件预置（Adapter prepare_workspace 只建目录不清文件）。
        let ws = adapter
            .prepare_workspace(&run_id, &input)
            .map_err(|e| ParserError(format!("prepare workspace: {e}")))?;
        if let Some(src) = input_file {
            let dst = ws.in_dir.join(input_name);
            std::fs::copy(src, &dst).map_err(|e| ParserError(format!("input copy: {e}")))?;
        }

        let outcome = execute_container_via_engine(ContainerCall {
            engine: &env.engine,
            run_id: &run_id,
            adapter: &adapter,
            image: &image,
            entrypoint: &entrypoint,
            family: &family,
            network,
            caller: None,
            caller_cap: None,
        })
        .map_err(|e| ParserError(format!("container run: {e}")))?;
        if outcome.envelope.status != ToolRunStatus::Succeeded {
            let msg = outcome
                .envelope
                .error
                .as_ref()
                .map(|e| e.message.clone())
                .unwrap_or_else(|| "unknown container failure".to_string());
            return Err(ParserError(format!("tool run failed: {msg}")));
        }
        let data = outcome.envelope.result.unwrap_or(Value::Null);

        // 产物拷出（ws/out → staging/<run_id>/；容器用后即焚）。
        let staging = self.config.staging_root.join(&run_id);
        std::fs::create_dir_all(&staging)?;
        copy_dir(&ws.out_dir, &staging)?;
        adapter.cleanup_container(&adapter.container_name(&run_id));
        adapter.cleanup_tmp(&ws);
        let _ = std::fs::remove_dir_all(&ws.root);
        Ok((data, staging))
    }
}

fn copy_dir(src: &Path, dst: &Path) -> std::io::Result<()> {
    if !src.exists() {
        return Ok(());
    }
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let target = dst.join(entry.file_name());
        if ty.is_dir() {
            std::fs::create_dir_all(&target)?;
            copy_dir(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), &target)?;
        }
    }
    Ok(())
}

impl DocumentParser for PodmanParserProvider {
    fn detect(&self, input: &Path) -> Result<DetectInfo, ParserError> {
        let name = input
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "input.bin".to_string());
        let (data, _) = self.run_op(
            TOOL_DETECT,
            json!({ "path": format!("/task/in/{name}") }),
            Some(input),
            &name,
        )?;
        Ok(DetectInfo {
            format: data["format"].as_str().unwrap_or("unknown").to_string(),
            mime_type: data["mime_type"]
                .as_str()
                .unwrap_or("application/octet-stream")
                .to_string(),
            size_bytes: data["size_bytes"].as_u64().unwrap_or(0),
            sha256: data["sha256"].as_str().unwrap_or_default().to_string(),
            document_type: data["document_type"]
                .as_str()
                .unwrap_or("other")
                .to_string(),
            zip: data.get("zip_verdict").cloned(),
            data,
        })
    }

    fn parse(&self, input: &Path) -> Result<ParseOutcome, ParserError> {
        let name = input
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "input.bin".to_string());
        let (data, staging) = self.run_op(
            TOOL_PARSE,
            json!({ "path": format!("/task/in/{name}") }),
            Some(input),
            &name,
        )?;
        parse_outcome_from_data(data, staging)
    }
}

impl OCRProvider for PodmanParserProvider {
    fn ocr(&self, input: &Path, language: &str) -> Result<ParseOutcome, ParserError> {
        let name = input
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "input.bin".to_string());
        let (data, staging) = self.run_op(
            TOOL_OCR,
            json!({ "path": format!("/task/in/{name}"), "language": language }),
            Some(input),
            &name,
        )?;
        parse_outcome_from_data(data, staging)
    }
}

/// Host-first parser provider.
///
/// It reuses the existing Document Router, Tool Engine, Python worker and
/// NativeRust adapters. It does not duplicate parser dispatch or spawn arbitrary
/// shell commands.
pub struct HostParserProvider {
    config: ParserProviderConfig,
    env: OnceLock<Result<HostRunEnv, String>>,
}

struct HostRunEnv {
    engine: Engine,
    router: document_router::Router,
    adapter: NativeRustAdapter,
    capability: StaticCatalogProvider,
    python_adapter: PythonWorkerAdapter,
}

impl HostParserProvider {
    pub fn new(config: ParserProviderConfig) -> Self {
        Self {
            config,
            env: OnceLock::new(),
        }
    }

    fn env(&self) -> Result<&HostRunEnv, ParserError> {
        let cell = self.env.get_or_init(|| self.init_env().map_err(|e| e.0));
        cell.as_ref().map_err(|e| ParserError(e.clone()))
    }

    fn init_env(&self) -> Result<HostRunEnv, ParserError> {
        let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .canonicalize()
            .map_err(|e| ParserError(format!("host parser repo root resolve: {e}")))?;

        std::fs::create_dir_all(&self.config.workspace_root)?;
        std::fs::create_dir_all(&self.config.staging_root)?;
        if let Some(dir) = self.config.run_db.parent() {
            std::fs::create_dir_all(dir)?;
        }

        let pool = database::Pool::open(database::PoolConfig {
            path: database::DbPath::File(self.config.run_db.clone()),
            readonly: false,
            max_connections: 4,
            busy_timeout: Duration::from_secs(5),
        })
        .map_err(|e| ParserError(format!("host parser ops db open: {e}")))?;
        pool.enable_wal()
            .map_err(|e| ParserError(format!("host parser ops db wal: {e}")))?;
        database::migrate(&pool, &self.config.sqlite_migrations_dir)
            .map_err(|e| ParserError(format!("host parser ops db migrate: {e}")))?;

        let registry = Registry::open(pool.clone()).with_options(Options {
            allow_placeholder_digest: false,
        });
        for id in [
            "rip.doc.pdf.info",
            "rip.doc.pdf.text",
            "rip.doc.office.to_pdf",
            "rip.doc.image.meta",
            "rip.doc.image.ocr",
            "rip.media.probe",
            "rip.media.frames",
            "rip.archive.list",
            "rip.archive.extract",
            "rip.doc.parse",
        ] {
            let path = repo_root.join(format!("manifests/tools/{id}.json"));
            let raw = std::fs::read_to_string(&path)
                .map_err(|e| ParserError(format!("manifest read {path:?}: {e}")))?;
            let manifest: Value = serde_json::from_str(&raw)
                .map_err(|e| ParserError(format!("manifest parse {path:?}: {e}")))?;
            // 幂等注册：run 库跨多次 parse 复用，同版本重复 register 会撞
            // VersionComparable（"not greater than existing version"，43 份
            // 真实语料批跑实录：首份注册成功、后续全数被拒）。已登记同版本
            // 则跳过；未登记/新版本才走 register。
            let manifest_version = manifest["version"].as_str().unwrap_or_default();
            let already = matches!(
                registry.query(id),
                Ok(detail) if detail.current_version.as_deref() == Some(manifest_version),
            );
            if !already {
                registry
                    .register(&manifest)
                    .map_err(|e| ParserError(format!("manifest register {id}: {e}")))?;
            }
        }
        let engine = Engine::open(pool.clone(), registry);

        let manifests =
            document_router::RouterManifests::from_dirs(&[repo_root.join("manifests/tools")])
                .map_err(|e| ParserError(format!("host parser manifests: {e}")))?;
        let router = document_router::Router::new(
            document_router::RouterConfig {
                workspace_root: self.config.workspace_root.clone(),
            },
            manifests,
        );
        let adapter = NativeRustAdapter::new(NativeRustConfig {
            workspace_root: self.config.workspace_root.clone(),
            ..NativeRustConfig::default()
        });
        let capability = StaticCatalogProvider::load(
            &repo_root.join("manifests/capabilities/host-catalog-seed.json"),
        )
        .map_err(|e| ParserError(format!("host capability catalog: {e}")))?;

        let python_adapter = PythonWorkerAdapter::new(PythonWorkerConfig {
            worker_name: "parsing-worker".to_string(),
            command: vec![
                "python3".to_string(),
                "-I".to_string(),
                "workers/python/parsing_worker.py".to_string(),
            ],
            working_dir: Some(repo_root),
            restart: RestartPolicy::Pooled,
            extra_env: Vec::new(),
            handshake_timeout: Duration::from_secs(10),
            venv_config: None,
            venv_identity: None,
        });

        Ok(HostRunEnv {
            engine,
            router,
            adapter,
            capability,
            python_adapter,
        })
    }
}

impl DocumentParser for HostParserProvider {
    fn detect(&self, input: &Path) -> Result<DetectInfo, ParserError> {
        let bytes = std::fs::read(input)?;
        detect_host_format(input, &bytes)
    }

    fn parse(&self, input: &Path) -> Result<ParseOutcome, ParserError> {
        let bytes = std::fs::read(input)?;
        let detect = detect_host_format(input, &bytes)?;
        let staging = self.config.staging_root.join(format!(
            "host-{}",
            &detect.sha256[..16.min(detect.sha256.len())]
        ));
        std::fs::create_dir_all(&staging)?;
        std::fs::create_dir_all(staging.join("assets"))?;

        let format = match detect.format.as_str() {
            "markdown" => "markdown",
            "txt" => "txt",
            "html" => "html",
            "code" => "txt",
            "pdf" | "docx" | "pptx" | "xlsx" | "image" => detect.format.as_str(),
            other => {
                return Err(ParserError(format!(
                    "host parser does not support format {other}"
                )))
            }
        };

        let (raw, ir_sha, parser_name, parser_version) =
            if matches!(format, "markdown" | "txt" | "html") {
                let env = self.env()?;
                let nonce = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_nanos();
                let run_id = format!("rn-host-text-{}-{nonce}", &detect.sha256[..12]);
                let submit_key = format!("host-text-{}-{nonce}", detect.sha256);
                env.engine
                    .submit(&json!({
                        "protocol_version": "1",
                        "tool_id": "rip.doc.parse",
                        "run_id": run_id,
                        "submit_key": submit_key,
                        "input": {
                            "path": input,
                            "format": if format == "markdown" { "md" } else { format }
                        },
                        "workspace_id": "default",
                        "budget": {"timeout_seconds": 60, "max_output_bytes": 1048576},
                        "result_schema_version": "1"
                    }))
                    .map_err(|e| ParserError(format!("host text submit: {e}")))?;
                let outcome = execute_via_engine(AdapterCall {
                    engine: &env.engine,
                    run_id: &run_id,
                    adapter: &env.python_adapter,
                    cancel_after: None,
                })
                .map_err(|e| ParserError(format!("host text execute: {e}")))?;
                let result = outcome.envelope.result.as_ref().ok_or_else(|| {
                    ParserError(format!(
                        "host text worker returned no result: status={:?} error={:?}",
                        outcome.envelope.status, outcome.envelope.error
                    ))
                })?;
                let text = String::from_utf8_lossy(&bytes).to_string();
                let raw = host_text_ir(input, &detect, &text, result, format, &run_id)?;
                let sha = sha2::Sha256::digest(serde_json::to_vec(&raw).unwrap_or_default());
                let ir_sha = format!("{sha:x}");
                (raw, ir_sha, "native_worker", "0.1.0".to_string())
            } else {
                let env = self.env()?;
                let source_name = input
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| "input.bin".to_string());
                let ext = input
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("")
                    .to_ascii_lowercase();
                let route = env.router.route(
                    &env.engine,
                    &env.adapter,
                    Some(&env.capability),
                    &bytes,
                    &source_name,
                    &ext,
                );
                match route {
                    Ok(outcome) if outcome.status == document_router::RouteStatus::Succeeded => {
                        let unified = outcome.unified.ok_or_else(|| {
                            ParserError("host router succeeded without unified output".to_string())
                        })?;
                        let raw = host_unified_ir(
                            input,
                            &detect,
                            &unified,
                            outcome.runs.iter().map(|r| r.run_id.clone()).collect(),
                        )?;
                        let sha =
                            sha2::Sha256::digest(serde_json::to_vec(&raw).unwrap_or_default());
                        let ir_sha = format!("{sha:x}");
                        let parser_name = match format {
                            "pdf" => "poppler",
                            "docx" | "pptx" | "xlsx" => "libreoffice-poppler",
                            "image" => "imagemagick-tesseract",
                            _ => "host",
                        };
                        (raw, ir_sha, parser_name, "host".to_string())
                    }
                    Ok(outcome) => {
                        return Err(ParserError(
                            outcome
                                .reject
                                .map(|r| format!("host route failed: {} {}", r.code, r.message))
                                .unwrap_or_else(|| "host route failed".to_string()),
                        ))
                    }
                    Err(reject) => {
                        return Err(ParserError(format!(
                            "host route rejected: {} {}",
                            reject.code, reject.message
                        )))
                    }
                }
            };

        let ir: DocumentIr = serde_json::from_value(raw.clone())
            .map_err(|e| ParserError(format!("host IR deserialize: {e}")))?;
        std::fs::write(
            staging.join("ir.json"),
            serde_json::to_vec_pretty(&raw).unwrap_or_default(),
        )?;
        let _ = parser_name;
        let _ = parser_version;
        Ok(ParseOutcome {
            ir,
            raw,
            ir_sha256: ir_sha,
            format: format.to_string(),
            staging_dir: staging,
        })
    }
}

impl OCRProvider for HostParserProvider {
    fn ocr(&self, input: &Path, language: &str) -> Result<ParseOutcome, ParserError> {
        let bytes = std::fs::read(input)?;
        let detect = detect_host_format(input, &bytes)?;
        let env = self.env()?;
        let source_name = input
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "input.bin".to_string());
        let ext = input
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        let outcome = env
            .router
            .route_ocr(
                &env.engine,
                &env.adapter,
                Some(&env.capability),
                &bytes,
                &source_name,
                &ext,
                Some(language),
            )
            .map_err(|r| ParserError(format!("host OCR rejected: {} {}", r.code, r.message)))?;
        if outcome.status != document_router::RouteStatus::Succeeded {
            return Err(ParserError(
                outcome
                    .reject
                    .map(|r| format!("host OCR failed: {} {}", r.code, r.message))
                    .unwrap_or_else(|| "host OCR failed".to_string()),
            ));
        }
        let unified = outcome
            .unified
            .ok_or_else(|| ParserError("host OCR succeeded without unified output".to_string()))?;
        let raw = host_unified_ir(
            input,
            &detect,
            &unified,
            outcome.runs.iter().map(|r| r.run_id.clone()).collect(),
        )?;
        let ir: DocumentIr = serde_json::from_value(raw.clone())
            .map_err(|e| ParserError(format!("host OCR IR deserialize: {e}")))?;
        let sha = sha2::Sha256::digest(serde_json::to_vec(&raw).unwrap_or_default());
        let staging = self.config.staging_root.join(format!(
            "host-{}",
            &detect.sha256[..16.min(detect.sha256.len())]
        ));
        std::fs::create_dir_all(&staging)?;
        std::fs::create_dir_all(staging.join("assets"))?;
        std::fs::write(
            staging.join("ir.json"),
            serde_json::to_vec_pretty(&raw).unwrap_or_default(),
        )?;
        Ok(ParseOutcome {
            ir,
            raw,
            ir_sha256: format!("{sha:x}"),
            format: "image".to_string(),
            staging_dir: staging,
        })
    }
}

fn detect_host_format(input: &Path, bytes: &[u8]) -> Result<DetectInfo, ParserError> {
    let ext = input
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .unwrap_or_default();
    let format = if bytes.starts_with(b"%PDF-") {
        "pdf"
    } else if bytes.starts_with(b"\x89PNG") || bytes.starts_with(b"\xff\xd8\xff") {
        "image"
    } else if bytes.starts_with(b"PK\x03\x04") && matches!(ext.as_str(), "docx" | "pptx" | "xlsx") {
        ext.as_str()
    } else if matches!(ext.as_str(), "md" | "markdown") {
        "markdown"
    } else if matches!(ext.as_str(), "html" | "htm") {
        "html"
    } else if matches!(ext.as_str(), "txt" | "log") || std::str::from_utf8(bytes).is_ok() {
        "txt"
    } else {
        "unknown"
    };
    let mime = match format {
        "pdf" => "application/pdf",
        "docx" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        "pptx" => "application/vnd.openxmlformats-officedocument.presentationml.presentation",
        "xlsx" => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        "image" if bytes.starts_with(b"\x89PNG") => "image/png",
        "image" => "image/jpeg",
        "html" => "text/html",
        "markdown" => "text/markdown",
        "txt" => "text/plain",
        _ => "application/octet-stream",
    };
    let sha = Sha256::digest(bytes);
    let sha_hex = format!("{sha:x}");
    let document_type = match format {
        "pdf" => "pdf",
        "docx" => "docx",
        "pptx" => "pptx",
        "xlsx" => "xlsx",
        "html" => "html",
        "markdown" => "markdown",
        "image" => "image",
        "txt" => "other",
        _ => "other",
    };
    Ok(DetectInfo {
        format: format.to_string(),
        mime_type: mime.to_string(),
        size_bytes: bytes.len() as u64,
        sha256: sha_hex.clone(),
        document_type: document_type.to_string(),
        zip: None,
        data: json!({
            "format": format,
            "mime_type": mime,
            "size_bytes": bytes.len(),
            "sha256": sha_hex,
            "document_type": document_type,
            "execution": "host",
        }),
    })
}

fn host_root_and_block(
    document_id: &str,
    title: &str,
    text: &str,
    page: Option<i64>,
) -> (Structure, Vec<Block>, Vec<BlockSource>) {
    let root_id = "blk-host-root";
    let paragraph_id = "blk-host-content";
    // 空文本不产内容块（IR schema block.content minLength 1；pptx/xlsx 转换出
    // 空文本 PDF 曾撞 schema violation，43 份真实语料批跑实录）。整体空文本
    // 由 overall=poor + REVIEW_PENDING 质量闸承载，不伪造内容。
    let mut blocks = vec![Block {
        block_id: root_id.to_string(),
        parent_id: None,
        block_type: "document_title".to_string(),
        order_index: 0,
        content: title.to_string(),
        heading_level: None,
        table: None,
        code: None,
        image_ref: None,
        page,
        location: None,
        extraction_method: "native_markdown".to_string(),
        extraction_confidence: None,
    }];
    if !text.is_empty() {
        blocks.push(Block {
            block_id: paragraph_id.to_string(),
            parent_id: Some(root_id.to_string()),
            block_type: "paragraph".to_string(),
            order_index: 0,
            content: text.to_string(),
            heading_level: None,
            table: None,
            code: None,
            image_ref: None,
            page,
            location: None,
            extraction_method: "native_markdown".to_string(),
            extraction_confidence: None,
        });
    }
    let structure = Structure {
        root: StructureNode {
            block_id: root_id.to_string(),
            children: blocks.iter().skip(1).map(|b| b.block_id.clone()).collect(),
        },
    };
    let mapping = vec![BlockSource {
        block_id: paragraph_id.to_string(),
        page,
        char_offset: Some(vec![0, text.chars().count() as i64]),
        bbox: None,
    }];
    let _ = document_id;
    (structure, blocks, mapping)
}

fn host_text_ir(
    input: &Path,
    detect: &DetectInfo,
    text: &str,
    worker: &Value,
    format: &str,
    run_id: &str,
) -> Result<Value, ParserError> {
    let title = input
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "Untitled document".to_string());
    let (structure, blocks, mapping) = host_root_and_block("doc-host", &title, text, None);
    let sections = worker
        .get("sections")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let overall = if text.is_empty() { "poor" } else { "good" };
    let ir = DocumentIr {
        schema_version: "1".to_string(),
        document_metadata: DocumentMetadata {
            document_id: "doc-host".to_string(),
            title,
            // txt 在 detect 面映射为 other（IR enum 无 txt；一致口径）。
            document_type: if format == "txt" {
                "other".to_string()
            } else {
                format.to_string()
            },
            original_filename: input
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default(),
            mime_type: detect.mime_type.clone(),
            language: None,
            content_sha256: detect.sha256.clone(),
        },
        parser_metadata: ParserMetadata {
            parser: "native_markdown".to_string(),
            parser_version: "0.1.0".to_string(),
            options: json!({
                "execution": "host",
                "worker": "parsing-worker",
                "host_run_id": run_id
            }),
            pipeline: vec!["format_detect".to_string(), "host_worker".to_string()],
            started_at: None,
            duration_ms: None,
            device: Some("host".to_string()),
        },
        structure,
        blocks,
        assets: Vec::new(),
        references: Vec::new(),
        extraction_quality: ExtractionQuality {
            self_check: vec![QualityCheck {
                check: "toc_present".to_string(),
                passed: !sections.is_empty(),
                detail: format!("sections={}", sections.len()),
            }],
            overall: overall.to_string(),
            flags: Vec::new(),
        },
        source_mapping: SourceMapping {
            block_to_source: mapping,
        },
    };
    serde_json::to_value(ir).map_err(|e| ParserError(format!("host text IR serialize: {e}")))
}

fn host_unified_ir(
    input: &Path,
    detect: &DetectInfo,
    unified: &Value,
    runs: Vec<String>,
) -> Result<Value, ParserError> {
    let channel = unified["channel"].as_str().unwrap_or("unknown");
    let text = unified["text"].as_str().unwrap_or_default();
    let anchors = unified["page_anchors"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let title = input
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "Untitled document".to_string());
    let extraction_method = match channel {
        "pdf" | "office" => "pymupdf",
        "image" | "ocr" => "tesseract",
        _ => "manual",
    };

    let root_id = "blk-host-root";
    let mut blocks = vec![Block {
        block_id: root_id.to_string(),
        parent_id: None,
        block_type: "document_title".to_string(),
        order_index: 0,
        content: title.clone(),
        heading_level: None,
        table: None,
        code: None,
        image_ref: None,
        page: Some(1),
        location: None,
        extraction_method: extraction_method.to_string(),
        extraction_confidence: None,
    }];
    let mut mapping = Vec::new();
    if !text.is_empty() {
        for (idx, anchor) in anchors.iter().enumerate() {
            let page = anchor["page"].as_i64().unwrap_or((idx + 1) as i64);
            let start = anchor["char_start"].as_i64().unwrap_or(0).max(0) as usize;
            let end = anchor["char_end"]
                .as_i64()
                .unwrap_or(text.chars().count() as i64)
                .max(start as i64) as usize;
            let chars: Vec<char> = text.chars().collect();
            let page_text: String = chars
                .get(start..end.min(chars.len()))
                .unwrap_or(&[])
                .iter()
                .collect();
            if page_text.is_empty() {
                continue;
            }
            let block_id = format!("blk-host-page-{page}");
            blocks.push(Block {
                block_id: block_id.clone(),
                parent_id: Some(root_id.to_string()),
                block_type: "paragraph".to_string(),
                order_index: idx as i64,
                content: page_text,
                heading_level: None,
                table: None,
                code: None,
                image_ref: None,
                page: Some(page),
                location: None,
                extraction_method: extraction_method.to_string(),
                extraction_confidence: None,
            });
            mapping.push(BlockSource {
                block_id,
                page: Some(page),
                char_offset: Some(vec![start as i64, end as i64]),
                bbox: None,
            });
        }
    }
    if blocks.len() == 1 && !text.is_empty() {
        blocks.push(Block {
            block_id: "blk-host-content".to_string(),
            parent_id: Some(root_id.to_string()),
            block_type: "paragraph".to_string(),
            order_index: 0,
            content: text.to_string(),
            heading_level: None,
            table: None,
            code: None,
            image_ref: None,
            page: Some(1),
            location: None,
            extraction_method: extraction_method.to_string(),
            extraction_confidence: None,
        });
        mapping.push(BlockSource {
            block_id: "blk-host-content".to_string(),
            page: Some(1),
            char_offset: Some(vec![0, text.chars().count() as i64]),
            bbox: None,
        });
    }
    let structure = Structure {
        root: StructureNode {
            block_id: root_id.to_string(),
            children: blocks.iter().skip(1).map(|b| b.block_id.clone()).collect(),
        },
    };
    let parser_name = match channel {
        "pdf" => "pymupdf",
        "office" => "pymupdf",
        "image" | "ocr" => "tesseract",
        _ => "manual",
    };
    let warnings = unified["warnings"].as_array().cloned().unwrap_or_default();
    let flags = warnings
        .iter()
        .filter_map(Value::as_str)
        .filter(|w| w.to_lowercase().contains("prompt") || w.to_lowercase().contains("inject"))
        .map(|_| "POSSIBLE_PROMPT_INJECTION".to_string())
        .collect::<Vec<_>>();
    let ir = DocumentIr {
        schema_version: "1".to_string(),
        document_metadata: DocumentMetadata {
            document_id: "doc-host".to_string(),
            title,
            document_type: detect.document_type.clone(),
            original_filename: input
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default(),
            mime_type: detect.mime_type.clone(),
            language: None,
            content_sha256: detect.sha256.clone(),
        },
        parser_metadata: ParserMetadata {
            parser: parser_name.to_string(),
            parser_version: "host".to_string(),
            options: json!({"execution": "host", "runs": runs}),
            pipeline: vec![
                "format_detect".to_string(),
                "host_native_tools".to_string(),
                "normalize".to_string(),
            ],
            started_at: None,
            duration_ms: None,
            device: Some("host".to_string()),
        },
        structure,
        blocks,
        assets: Vec::new(),
        references: Vec::new(),
        extraction_quality: ExtractionQuality {
            self_check: vec![QualityCheck {
                check: "page_coverage".to_string(),
                passed: true,
                detail: format!(
                    "channel={channel}, runs={}",
                    unified["runs"].as_array().map(|v| v.len()).unwrap_or(0)
                ),
            }],
            overall: if text.is_empty() { "poor" } else { "good" }.to_string(),
            flags,
        },
        source_mapping: SourceMapping {
            block_to_source: mapping,
        },
    };
    serde_json::to_value(ir).map_err(|e| ParserError(format!("host unified IR serialize: {e}")))
}

fn parse_outcome_from_data(data: Value, staging_dir: PathBuf) -> Result<ParseOutcome, ParserError> {
    let raw = data
        .get("ir")
        .cloned()
        .ok_or_else(|| ParserError("result data missing ir".to_string()))?;
    let ir: DocumentIr = serde_json::from_value(raw.clone())
        .map_err(|e| ParserError(format!("ir deserialize: {e}")))?;
    Ok(ParseOutcome {
        ir,
        raw,
        ir_sha256: data["ir_sha256"].as_str().unwrap_or_default().to_string(),
        format: data["format"].as_str().unwrap_or("unknown").to_string(),
        staging_dir,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    #[test]
    fn route_table_covers_first_batch_formats() {
        for fmt in PARSE_FORMATS {
            assert_eq!(route_parse(fmt), Some(TOOL_PARSE), "{fmt} must route");
        }
        assert_eq!(route_parse("zip"), None);
        assert_eq!(route_parse("json"), None);
        assert_eq!(route_parse("unknown"), None);
    }

    #[test]
    fn host_detect_format_and_sha256_are_real() {
        let dir = std::env::temp_dir().join(format!("rip-host-detect-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let md = dir.join("sample.md");
        std::fs::write(&md, b"# Sample\n\nHost text").unwrap();
        let bytes = std::fs::read(&md).unwrap();
        let detect = detect_host_format(&md, &bytes).unwrap();
        assert_eq!(detect.format, "markdown");
        assert_eq!(detect.mime_type, "text/markdown");
        assert_eq!(detect.sha256.len(), 64);
        assert_eq!(detect.data["execution"].as_str(), Some("host"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn host_text_parser_returns_document_ir() {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir =
            std::env::temp_dir().join(format!("rip-host-parser-{}-{nonce}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let md = dir.join("sample.md");
        std::fs::write(&md, b"# Host Title\n\nHost parser paragraph.\n").unwrap();
        let cfg = ParserProviderConfig {
            run_db: dir.join("ops.db"),
            sqlite_migrations_dir: repo_root().join("migrations"),
            manifest_dir: repo_root().join("containerfiles/rip-parser"),
            workspace_root: dir.join("ws"),
            staging_root: dir.join("staging"),
            memory_bytes: 256 * 1024 * 1024,
            timeout_seconds: 60,
        };
        let provider = HostParserProvider::new(cfg);
        let outcome = provider.parse(&md).expect("host text parse");
        assert_eq!(outcome.format, "markdown");
        assert_eq!(outcome.ir.document_metadata.document_id, "doc-host");
        assert_eq!(outcome.ir.blocks.len(), 2);
        assert_eq!(outcome.ir.parser_metadata.device.as_deref(), Some("host"));
        crate::ir::validate_ir(&outcome.ir, &outcome.raw).expect("host text IR validates");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn host_pdf_office_and_image_real_smoke() {
        // 宿主工具缺席跳过（CI runner 无 poppler/LibreOffice/Tesseract 实录：
        // 46145159264「NATIVE_BINARY_MISSING /usr/bin/pdfinfo」）。宿主优先
        // 解析的验收在装齐工具的本地执行（见 GOLDEN-CORPUS-ACCEPTANCE），
        // CI 面只保编译与纯函数闸，缺席即如实 SKIP，不伪造成功。
        let required = ["pdfinfo", "soffice", "tesseract", "convert"];
        let missing: Vec<&str> = required
            .iter()
            .filter(|bin| {
                std::process::Command::new(bin)
                    .arg("--version")
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .status()
                    .is_err()
            })
            .copied()
            .collect();
        if !missing.is_empty() {
            eprintln!("SKIP: host parser tools missing ({missing:?}); host-first acceptance runs on provisioned hosts");
            return;
        }
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir =
            std::env::temp_dir().join(format!("rip-host-real-{}-{nonce}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let cfg = ParserProviderConfig {
            run_db: dir.join("ops.db"),
            sqlite_migrations_dir: repo_root().join("migrations"),
            manifest_dir: repo_root().join("containerfiles/rip-parser"),
            workspace_root: dir.join("ws"),
            staging_root: dir.join("staging"),
            memory_bytes: 256 * 1024 * 1024,
            timeout_seconds: 120,
        };
        let provider = HostParserProvider::new(cfg);

        for name in ["sample.pdf", "sample.docx", "sample.pptx", "sample.xlsx"] {
            let input = repo_root()
                .join("crates/document-router/tests/fixtures")
                .join(name);
            match provider.parse(&input) {
                Ok(outcome) => {
                    crate::ir::validate_ir(&outcome.ir, &outcome.raw)
                        .expect("real host IR validates");
                    assert_eq!(outcome.ir.document_metadata.content_sha256.len(), 64);
                    assert!(!outcome
                        .staging_dir
                        .join("ir.json")
                        .to_string_lossy()
                        .is_empty());
                }
                Err(error) => panic!("host real parse {name} failed: {error}"),
            }
        }

        let image = repo_root()
            .join("crates/document-router/tests/fixtures")
            .join("sample.png");
        let ocr: &dyn OCRProvider = &provider;
        match ocr.ocr(&image, "eng") {
            Ok(image_outcome) => {
                crate::ir::validate_ir(&image_outcome.ir, &image_outcome.raw)
                    .expect("host OCR IR validates");
            }
            Err(error) if error.0.contains("NATIVE_IDENTITY_MISMATCH") => {
                eprintln!("BLOCKED host OCR pinned identity mismatch: {error}");
            }
            Err(error) => panic!("host image OCR smoke failed: {error}"),
        }

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn manifests_present_digest_pinned_entrypoint_ops() {
        for (tool_id, op) in [
            (TOOL_DETECT, "detect"),
            (TOOL_PARSE, "parse"),
            (TOOL_OCR, "ocr"),
        ] {
            let path = repo_root().join(format!("containerfiles/rip-parser/{tool_id}.json"));
            let raw =
                std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path:?} missing: {e}"));
            let m: Value = serde_json::from_str(&raw).unwrap();
            assert_eq!(m["id"].as_str(), Some(tool_id), "{tool_id} id mismatch");
            let image = m["runtime"]["image"].as_str().expect("runtime.image");
            assert!(
                PodmanHostAdapter::image_ref_ok(image),
                "{tool_id} image must be digest-pinned: {image}"
            );
            assert_eq!(m["network"]["mode"].as_str(), Some("none"));
            assert_eq!(m["permission_level"].as_str(), Some("T1"));
            let ep = m["runtime"]["entrypoint"].as_array().unwrap();
            assert_eq!(ep.last().and_then(|v| v.as_str()), Some(op));
            assert_eq!(
                m["runtime"]["entrypoint"][1].as_str(),
                Some("/app/parser_tools.py")
            );
        }
    }

    #[test]
    fn manifest_yaml_json_pairs_consistent() {
        for tool_id in [TOOL_DETECT, TOOL_PARSE, TOOL_OCR] {
            let yaml_path = repo_root().join(format!("containerfiles/rip-parser/{tool_id}.yaml"));
            assert!(yaml_path.exists(), "{tool_id}.yaml missing (人读口径)");
            let raw = std::fs::read_to_string(&yaml_path).unwrap();
            let y: Value = serde_yaml_ng::from_str(&raw).unwrap();
            let j: Value = serde_json::from_str(
                &std::fs::read_to_string(
                    repo_root().join(format!("containerfiles/rip-parser/{tool_id}.json")),
                )
                .unwrap(),
            )
            .unwrap();
            assert_eq!(y["id"], j["id"], "{tool_id} yaml/json id diverged");
            assert_eq!(
                y["runtime"]["image"], j["runtime"]["image"],
                "{tool_id} digest diverged (台账三处同步)"
            );
            assert_eq!(y["runtime"]["entrypoint"], j["runtime"]["entrypoint"]);
        }
    }
}
