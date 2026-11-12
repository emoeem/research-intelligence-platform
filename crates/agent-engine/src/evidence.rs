//! 结果解析与 Artifact/Evidence 登记（计划 §9:454-477 第 6 步；计划 §9:478
//! 知识工具联动八步的 6/7 两步落面）。
//!
//! - Artifact（005 artifact 表）：工具结构化输出登记为 role=primary 产物，
//!   sha256 与 size_bytes 实算；artifacts_dir 给出时正文落文件（`<dir>/
//!   <tool_run_id>/<artifact_id>.json`），缺省登记引用 uri（ripartifact://
//!   形态，正文持久化归档属 Artifact Manager 后续批次，此处不做伪装）。
//! - Evidence（001 evidence 表）：evidence_type=log（结构化结果日志面），
//!   content_hash 与 Artifact 同哈希（可复核），storage_ref 指向 Artifact
//!   uri，source_pk 指回 tool_run id（审计链 correlation 同 run）。

use crate::error::AgentError;
use crate::new_id;
use database::{rusqlite, Pool};
use serde_json::Value;
use tool_registry::hash::sha256_hex;

/// 登记载荷（一次工具调用的落账束）。
#[derive(Debug, Clone)]
pub struct Registration {
    /// 登记者（agent_id，进 evidence.captured_by）。
    pub agent_id: String,
    /// 关联研究任务（evidence.task_ref）。
    pub task_id: Option<String>,
    /// tool_run id（artifact FK + evidence.source_pk）。
    pub tool_run_id: String,
    /// 工具 id（provenance 摘要用）。
    pub tool_id: String,
    /// 执行版本。
    pub tool_version: String,
    /// 结构化载荷（SUCCEEDED 取信封 result；失败取 error 对象）。
    pub payload: Value,
    /// 链路追踪 ID（P7-5 迁移 015：artifact/evidence 行 trace_id 列同链
    /// 落账；None 不落列，与 015 之前登记面语义一致）。
    pub trace_id: Option<String>,
}

/// 登记结果（Artifact/Evidence 双 id + uri + 同哈希快照）。
#[derive(Debug, Clone, PartialEq)]
pub struct RegisteredOutcome {
    /// artifact id。
    pub artifact_id: String,
    /// artifact uri（文件路径或 ripartifact:// 引用）。
    pub artifact_uri: String,
    /// evidence id。
    pub evidence_id: String,
    /// 双方共同 sha256（P7-3 Evidence Memory 台账快照来源）。
    pub content_hash: String,
}

/// Artifact/Evidence 登记器。
#[derive(Clone)]
pub struct EvidenceRegistrar {
    pool: Pool,
    artifacts_dir: Option<std::path::PathBuf>,
}

impl EvidenceRegistrar {
    /// 构造（缺省登记引用 uri，不落文件）。
    pub fn new(pool: Pool) -> Self {
        Self {
            pool,
            artifacts_dir: None,
        }
    }

    /// 挂产物落盘目录（存在即写文件）。
    pub fn with_artifacts_dir(mut self, dir: std::path::PathBuf) -> Self {
        self.artifacts_dir = Some(dir);
        self
    }

    /// 登记一次工具调用的 Artifact + Evidence（同哈希可互相对账）。
    pub fn register_tool_result(
        &self,
        reg: &Registration,
    ) -> Result<RegisteredOutcome, AgentError> {
        let artifact_id = new_id("art");
        let payload_text = serde_json::to_string(&reg.payload)
            .map_err(|e| AgentError::Storage(format!("serialize tool result: {e}")))?;
        let bytes = payload_text.as_bytes();
        let sha = sha256_hex(bytes);
        let size_bytes = bytes.len() as i64;
        let now = domain::time::now_iso();

        let artifact_uri = match &self.artifacts_dir {
            Some(dir) => {
                let run_dir = dir.join(&reg.tool_run_id);
                std::fs::create_dir_all(&run_dir).map_err(|e| {
                    AgentError::Storage(format!("create artifacts dir {}: {e}", run_dir.display()))
                })?;
                let path = run_dir.join(format!("{artifact_id}.json"));
                std::fs::write(&path, bytes).map_err(|e| {
                    AgentError::Storage(format!("write artifact {}: {e}", path.display()))
                })?;
                path.to_string_lossy().to_string()
            }
            None => format!("ripartifact://{}/{artifact_id}", reg.tool_run_id),
        };

        let provenance = format!(
            "agent-engine 登记工具结构化输出（P7-2 最小闭环）：{}@{}",
            reg.tool_id, reg.tool_version
        );
        self.pool
            .with_conn(|c| {
                c.execute(
                    "INSERT INTO artifact (artifact_id, run_id, role, uri, sha256, size_bytes,
                     mime_type, provenance, trace_id, created_at, schema_version)
                     VALUES (?1, ?2, 'primary', ?3, ?4, ?5, 'application/json', ?6, ?7, ?8, 1)",
                    rusqlite::params![
                        artifact_id,
                        reg.tool_run_id,
                        artifact_uri,
                        sha,
                        size_bytes,
                        provenance,
                        reg.trace_id,
                        now
                    ],
                )?;
                Ok::<(), rusqlite::Error>(())
            })
            .map_err(AgentError::from)?;

        let evidence_id = new_id("ev");
        let captured_by = format!("agent-engine:{}", reg.agent_id);
        self.pool
            .with_conn(|c| {
                c.execute(
                    "INSERT INTO evidence (evidence_id, evidence_type, storage_ref, content_hash,
                     captured_at, captured_by, task_ref, lifecycle, governance, source_system,
                     source_pk, trace_id, created_at, updated_at, schema_version)
                     VALUES (?1, 'log', ?2, ?3, ?4, ?5, ?6, 'RAW', 'ACTIVE', 'tool-engine',
                     ?7, ?8, ?4, ?4, 1)",
                    rusqlite::params![
                        evidence_id,
                        artifact_uri,
                        sha,
                        now,
                        captured_by,
                        reg.task_id,
                        reg.tool_run_id,
                        reg.trace_id
                    ],
                )?;
                Ok::<(), rusqlite::Error>(())
            })
            .map_err(AgentError::from)?;

        Ok(RegisteredOutcome {
            artifact_id,
            artifact_uri,
            evidence_id,
            content_hash: sha,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use database::{DbPath, PoolConfig};
    use serde_json::json;
    use std::time::Duration;

    fn migrated_pool() -> Pool {
        let pool = Pool::open(PoolConfig {
            path: DbPath::Memory,
            readonly: false,
            max_connections: 1,
            busy_timeout: Duration::from_secs(5),
        })
        .unwrap();
        let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../migrations");
        database::migrate(&pool, &dir).unwrap();
        pool
    }

    fn registration() -> Registration {
        Registration {
            agent_id: "agent.research.minimal".to_string(),
            task_id: Some("task-42".to_string()),
            tool_run_id: "rn-tool-1".to_string(),
            tool_id: "rip.kb.keyword_search".to_string(),
            tool_version: "0.1.0".to_string(),
            payload: json!({"keyword": "nmap", "total_hits": 2}),
            trace_id: None,
        }
    }

    /// artifact FK（run_id REFERENCES tool_run）要求先有 tool_run 行。
    fn seed_tool_run(pool: &Pool, run_id: &str) {
        pool.with_conn(|c| -> rusqlite::Result<()> {
            c.execute(
                "INSERT INTO tool (tool_id, name, kind, lifecycle, governance, permission_level,
                 source_system, source_pk, created_at, updated_at, schema_version)
                 VALUES ('rip.kb.keyword_search', 'KB Keyword Search', 'tool', 'ENABLED', 'ACTIVE',
                 'T0', 'tool-registry', 'rip.kb.keyword_search', '2026-11-12T00:00:00Z',
                 '2026-11-12T00:00:00Z', 1)",
                [],
            )
            .unwrap();
            c.execute(
                "INSERT INTO tool_run (run_id, tool_id, tool_version, task_id, workspace_id,
                 status, created_at, updated_at, schema_version)
                 VALUES (?1, 'rip.kb.keyword_search', '0.1.0', 'task-42', 'ws-default',
                 'SUCCEEDED', '2026-11-12T00:00:00Z', '2026-11-12T00:00:00Z', 1)",
                [run_id],
            )
            .unwrap();
            Ok(())
        })
        .unwrap();
    }

    /// 引用形态（无 artifacts_dir）：artifact + evidence 落行且同哈希。
    #[test]
    fn reference_mode_registers_both_rows_with_same_hash() {
        let pool = migrated_pool();
        seed_tool_run(&pool, "rn-tool-1");
        let registrar = EvidenceRegistrar::new(pool.clone());
        let mut reg = registration();
        reg.trace_id = Some("tr-ev-1".to_string());
        let out = registrar.register_tool_result(&reg).unwrap();
        assert!(out.artifact_uri.starts_with("ripartifact://rn-tool-1/"));

        // P7-5：trace_id 同链落两行（迁移 015 列）。
        let (artifact_trace, evidence_trace): (Option<String>, Option<String>) = pool
            .with_conn(|c| {
                let a: Option<String> = c
                    .query_row(
                        "SELECT trace_id FROM artifact WHERE artifact_id = ?1",
                        [&out.artifact_id],
                        |r| r.get(0),
                    )
                    .unwrap();
                let e: Option<String> = c
                    .query_row(
                        "SELECT trace_id FROM evidence WHERE evidence_id = ?1",
                        [&out.evidence_id],
                        |r| r.get(0),
                    )
                    .unwrap();
                Ok::<(Option<String>, Option<String>), rusqlite::Error>((a, e))
            })
            .unwrap();
        assert_eq!(artifact_trace.as_deref(), Some("tr-ev-1"));
        assert_eq!(evidence_trace.as_deref(), Some("tr-ev-1"));

        let (uri, sha, role): (String, String, String) = pool
            .with_conn(|c| {
                c.query_row(
                    "SELECT uri, sha256, role FROM artifact WHERE artifact_id = ?1",
                    [&out.artifact_id],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                )
            })
            .unwrap();
        assert_eq!(uri, out.artifact_uri);
        assert_eq!(role, "primary");
        assert_eq!(sha.len(), 64);

        let (etype, storage_ref, content_hash, source_pk, task_ref): (
            String,
            String,
            String,
            String,
            Option<String>,
        ) = pool
            .with_conn(|c| {
                c.query_row(
                    "SELECT evidence_type, storage_ref, content_hash, source_pk, task_ref
                     FROM evidence WHERE evidence_id = ?1",
                    [&out.evidence_id],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
                )
            })
            .unwrap();
        assert_eq!(etype, "log");
        assert_eq!(storage_ref, out.artifact_uri);
        assert_eq!(content_hash, sha, "artifact 与 evidence 必须同哈希");
        assert_eq!(source_pk, "rn-tool-1");
        assert_eq!(task_ref.as_deref(), Some("task-42"));
    }

    /// 落盘形态：正文写入 artifacts_dir，uri 指向文件且内容一致。
    #[test]
    fn file_mode_writes_payload_file() {
        let pool = migrated_pool();
        seed_tool_run(&pool, "rn-tool-1");
        let dir = std::env::temp_dir().join(format!(
            "rip-agent-artifacts-{}-{}",
            std::process::id(),
            crate::new_id("seq")
        ));
        let registrar = EvidenceRegistrar::new(pool.clone()).with_artifacts_dir(dir.clone());
        let out = registrar.register_tool_result(&registration()).unwrap();
        let path = std::path::PathBuf::from(&out.artifact_uri);
        assert!(
            path.is_file(),
            "artifact file must exist: {}",
            path.display()
        );
        let body = std::fs::read_to_string(&path).unwrap();
        assert_eq!(tool_registry::hash::sha256_hex(body.as_bytes()).len(), 64);
        let parsed: Value = serde_json::from_str(&body).unwrap();
        assert_eq!(parsed["keyword"], "nmap");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
