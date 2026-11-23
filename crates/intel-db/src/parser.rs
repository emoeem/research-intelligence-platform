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

use crate::ir::DocumentIr;
use crate::IntelError;
use domain::{NodeHealth, RuntimeNode, ToolRunStatus};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Duration;
use tool_engine::adapter::{
    execute_container_via_engine, ContainerCall, NetworkPlan, PodmanHostAdapter, PodmanHostConfig,
};
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
