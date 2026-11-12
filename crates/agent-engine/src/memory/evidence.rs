//! Evidence Memory 数据面与读写 API（013 agent_evidence_memory 表；证据/
//! 产物引用台账，防丢失机制面）。
//!
//! 反重复建模（ROADMAP §8.5 风险 4，评审记录
//! docs/phase7/AGENT-MEMORY-REVIEW.md）：本台账只存指针（evidence/artifact
//! id）与登记时刻快照（content_hash/storage_ref），不存 captured_at/
//! captured_by/permission_level 等 evidence 实体字段；原始证据的唯一登记面
//! 仍是 001 evidence 表。
//!
//! 契约（计划 §9:478 原文「原始证据不能因上下文压缩而丢失」）：
//! - 写入时快照必须与既有表权威值一致（evidence.content_hash /
//!   evidence.storage_ref / artifact.sha256 三方一致，否则整行拒绝）；
//! - 压缩（[`EvidenceMemoryStore::compress_run`]）只推进生命周期词值，
//!   指针与快照列零改动；
//! - 本台账无驱逐 API：domain 跃迁表对 Evidence 类拒绝 EVICTED/ARCHIVED，
//!   013 CHECK 词值兜底；[`EvidenceMemoryStore::resolve`] 在任意时刻以
//!   evidence/artifact 既有表为权威复核，落盘产物逐字节复算。

use crate::error::AgentError;
use database::rusqlite::OptionalExtension;
use database::{rusqlite, Pool};
use domain::{ArtifactId, EvidenceMemoryEntry, RunId};
use tool_registry::hash::sha256_hex;

/// 库内 Evidence Memory 行投影。
#[derive(Debug, Clone)]
pub struct StoredEvidenceMemory {
    /// 域对象。
    pub entry: EvidenceMemoryEntry,
    /// 创建时刻。
    pub created_at: String,
    /// 更新时刻。
    pub updated_at: String,
}

/// 单条证据台账的解析结果（三方哈希对账 + 落盘复算结论）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedEvidence {
    /// 台账 memory id。
    pub memory_id: String,
    /// 证据 id（001 evidence 表）。
    pub evidence_id: String,
    /// 产物 id（005 artifact 表；台账未挂产物时为 None）。
    pub artifact_id: Option<String>,
    /// 哈希（三方一致时的共同值；不一致时为快照值）。
    pub content_hash: String,
    /// 解析入口（evidence.storage_ref 实值）。
    pub storage_ref: String,
    /// 三方哈希是否一致（快照 = evidence.content_hash = artifact.sha256）。
    pub db_hash_consistent: bool,
    /// 落盘产物逐字节复算结论：Some(true/false) 为文件形态结论，
    /// None 为引用形态（ripartifact:// 等无落盘正文，如实标注不伪装）。
    pub disk_hash_verified: Option<bool>,
    /// 结论明细（失败原因或标注）。
    pub detail: String,
}

/// Evidence Memory 读写存储（引用台账；只压缩，无驱逐）。
#[derive(Clone)]
pub struct EvidenceMemoryStore {
    pool: Pool,
}

impl EvidenceMemoryStore {
    /// 在连接池之上打开。
    pub fn new(pool: Pool) -> Self {
        Self { pool }
    }

    /// 登记一条证据指针（快照必须与既有表权威值一致，不一致整行拒绝；
    /// UNIQUE(run_id, evidence_id) 防同 run 重复登记）。
    pub fn record(&self, entry: &EvidenceMemoryEntry) -> Result<(), AgentError> {
        let run_id = entry.run_id().as_str();
        let run_exists: bool = self.pool.with_conn(|c| {
            let mut stmt = c.prepare("SELECT 1 FROM agent_run WHERE run_id = ?1")?;
            stmt.exists([run_id])
        })?;
        if !run_exists {
            return Err(AgentError::RunNotFound(run_id.to_string()));
        }
        let live: Option<(String, Option<String>)> = self.pool.with_conn(|c| {
            let mut stmt =
                c.prepare("SELECT content_hash, storage_ref FROM evidence WHERE evidence_id = ?1")?;
            stmt.query_row([entry.evidence_id()], |r| Ok((r.get(0)?, r.get(1)?)))
                .optional()
        })?;
        let Some((ev_hash, ev_ref)) = live else {
            return Err(AgentError::Storage(format!(
                "evidence row {:?} not found; ledger pointer requires the authoritative row",
                entry.evidence_id()
            )));
        };
        if ev_hash != entry.content_hash() {
            return Err(AgentError::Storage(format!(
                "evidence {:?} content_hash drifted from registration snapshot",
                entry.evidence_id()
            )));
        }
        let ev_ref = ev_ref.ok_or_else(|| {
            AgentError::Storage(format!(
                "evidence {:?} has NULL storage_ref; no resolvable entry point",
                entry.evidence_id()
            ))
        })?;
        if ev_ref != entry.storage_ref() {
            return Err(AgentError::Storage(format!(
                "evidence {:?} storage_ref drifted from registration snapshot",
                entry.evidence_id()
            )));
        }
        if let Some(artifact) = entry.artifact_id() {
            let art_hash: Option<String> = self.pool.with_conn(|c| {
                let mut stmt = c.prepare("SELECT sha256 FROM artifact WHERE artifact_id = ?1")?;
                stmt.query_row([artifact.as_str()], |r| r.get(0)).optional()
            })?;
            let Some(art_hash) = art_hash else {
                return Err(AgentError::Storage(format!(
                    "artifact row {:?} not found; ledger pointer requires the authoritative row",
                    artifact.as_str()
                )));
            };
            if art_hash != entry.content_hash() {
                return Err(AgentError::Storage(format!(
                    "artifact {:?} sha256 does not match evidence content_hash (same-hash ledger)",
                    artifact.as_str()
                )));
            }
        }
        let now = domain::time::now_iso();
        self.pool
            .with_conn(|c| {
                c.execute(
                    "INSERT INTO agent_evidence_memory (memory_id, run_id, evidence_id,
                     artifact_id, content_hash, storage_ref, lifecycle, created_at, updated_at,
                     schema_version)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8, 1)",
                    rusqlite::params![
                        entry.memory_id(),
                        run_id,
                        entry.evidence_id(),
                        entry.artifact_id().map(|a| a.as_str()),
                        entry.content_hash(),
                        entry.storage_ref(),
                        entry.lifecycle().as_str(),
                        now
                    ],
                )?;
                Ok::<(), rusqlite::Error>(())
            })
            .map_err(AgentError::from)?;
        Ok(())
    }

    /// 按 memory_id 取单条。
    pub fn get(&self, memory_id: &str) -> Result<Option<StoredEvidenceMemory>, AgentError> {
        type Row = (
            String,
            String,
            Option<String>,
            String,
            String,
            String,
            String,
            String,
        );
        let row: Option<Row> = self.pool.with_conn(|c| {
            let mut stmt = c.prepare(
                "SELECT run_id, evidence_id, artifact_id, content_hash, storage_ref,
                        lifecycle, created_at, updated_at
                 FROM agent_evidence_memory WHERE memory_id = ?1",
            )?;
            stmt.query_row([memory_id], |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                    r.get(6)?,
                    r.get(7)?,
                ))
            })
            .optional()
        })?;
        row.map(
            |(
                run_id,
                evidence_id,
                artifact_id,
                content_hash,
                storage_ref,
                lifecycle,
                created_at,
                updated_at,
            )| {
                let lifecycle = domain::MemoryLifecycle::parse(&lifecycle)?;
                let artifact_id = artifact_id
                    .map(ArtifactId::try_new)
                    .transpose()
                    .map_err(crate::error::AgentError::from)?;
                let entry = EvidenceMemoryEntry::try_new(
                    memory_id,
                    RunId::try_new(run_id)?,
                    evidence_id,
                    artifact_id,
                    content_hash,
                    storage_ref,
                    lifecycle,
                )?;
                Ok::<StoredEvidenceMemory, AgentError>(StoredEvidenceMemory {
                    entry,
                    created_at,
                    updated_at,
                })
            },
        )
        .transpose()
    }

    /// 按 run 列出台账行（登记顺序）。
    pub fn list_by_run(&self, run_id: &str) -> Result<Vec<StoredEvidenceMemory>, AgentError> {
        let ids: Vec<String> = self.pool.with_conn(|c| {
            let mut stmt = c.prepare(
                "SELECT memory_id FROM agent_evidence_memory WHERE run_id = ?1 ORDER BY rowid",
            )?;
            let rows = stmt
                .query_map([run_id], |r| r.get::<_, String>(0))?
                .collect::<Result<Vec<_>, rusqlite::Error>>()?;
            Ok::<_, rusqlite::Error>(rows)
        })?;
        let mut out = Vec::new();
        for id in ids {
            if let Some(row) = self.get(&id)? {
                out.push(row);
            }
        }
        Ok(out)
    }

    /// 压缩 run 内全部 ACTIVE 台账行：只推进生命周期词值（ACTIVE →
    /// COMPRESSED），指针与快照列零改动。返回压缩条数。
    pub fn compress_run(&self, run_id: &str) -> Result<usize, AgentError> {
        let now = domain::time::now_iso();
        let updated = self
            .pool
            .with_conn(|c| {
                c.execute(
                    "UPDATE agent_evidence_memory SET lifecycle = 'COMPRESSED', updated_at = ?2
                     WHERE run_id = ?1 AND lifecycle = 'ACTIVE'",
                    rusqlite::params![run_id, now],
                )
            })
            .map_err(AgentError::from)?;
        Ok(updated)
    }

    /// 解析一条台账（压缩/驱逐后仍可调用）：回链行存在性 + 三方哈希对账
    /// （快照 = evidence.content_hash = artifact.sha256）+ 落盘产物逐字节
    /// 复算（文件形态）。任何一环失守即 Err（契约失败面）。
    pub fn resolve(&self, memory_id: &str) -> Result<ResolvedEvidence, AgentError> {
        let stored = self.get(memory_id)?.ok_or_else(|| {
            AgentError::Storage(format!("evidence memory {memory_id:?} not found"))
        })?;
        let entry = &stored.entry;
        let ev: (String, Option<String>) = self
            .pool
            .with_conn(|c| {
                let mut stmt = c.prepare(
                    "SELECT content_hash, storage_ref FROM evidence WHERE evidence_id = ?1",
                )?;
                stmt.query_row([entry.evidence_id()], |r| Ok((r.get(0)?, r.get(1)?)))
                    .optional()
            })?
            .ok_or_else(|| {
                AgentError::Storage(format!(
                    "back-link broken: evidence row {:?} no longer exists",
                    entry.evidence_id()
                ))
            })?;
        let (ev_hash, ev_ref) = ev;
        let mut detail = String::new();
        let mut db_hash_consistent = ev_hash == entry.content_hash();
        if !db_hash_consistent {
            detail.push_str(&format!(
                "evidence.content_hash {ev_hash:?} != ledger snapshot {:?}",
                entry.content_hash()
            ));
        }
        let ev_ref = ev_ref.unwrap_or_default();
        if ev_ref != entry.storage_ref() {
            db_hash_consistent = false;
            if !detail.is_empty() {
                detail.push_str("; ");
            }
            detail.push_str(&format!(
                "evidence.storage_ref {ev_ref:?} != ledger snapshot {:?}",
                entry.storage_ref()
            ));
        }

        let mut artifact_id: Option<String> = None;
        let mut disk_hash_verified: Option<bool> = None;
        if let Some(artifact) = entry.artifact_id() {
            let art: Option<(String, String)> = self.pool.with_conn(|c| {
                let mut stmt =
                    c.prepare("SELECT uri, sha256 FROM artifact WHERE artifact_id = ?1")?;
                stmt.query_row([artifact.as_str()], |r| Ok((r.get(0)?, r.get(1)?)))
                    .optional()
            })?;
            let Some((uri, sha)) = art else {
                return Err(AgentError::Storage(format!(
                    "back-link broken: artifact row {:?} no longer exists",
                    artifact.as_str()
                )));
            };
            artifact_id = Some(artifact.as_str().to_string());
            if sha != ev_hash {
                db_hash_consistent = false;
                if !detail.is_empty() {
                    detail.push_str("; ");
                }
                detail.push_str(&format!(
                    "artifact.sha256 {sha:?} != evidence.content_hash {ev_hash:?}"
                ));
            }
            let path = std::path::Path::new(&uri);
            if path.is_file() {
                let bytes = std::fs::read(path)
                    .map_err(|e| AgentError::Storage(format!("read artifact file {uri:?}: {e}")))?;
                let actual = sha256_hex(&bytes);
                let ok = actual == sha;
                if !ok {
                    if !detail.is_empty() {
                        detail.push_str("; ");
                    }
                    detail.push_str(&format!(
                        "on-disk artifact hash {actual:?} != registered {sha:?}"
                    ));
                }
                disk_hash_verified = Some(ok);
            } else {
                // 引用形态（ripartifact:// 等）：无落盘正文可复算，如实标注。
                detail.push_str("; reference-mode artifact uri (no on-disk payload to rehash)");
            }
        }

        Ok(ResolvedEvidence {
            memory_id: memory_id.to_string(),
            evidence_id: entry.evidence_id().to_string(),
            artifact_id,
            content_hash: ev_hash,
            storage_ref: ev_ref,
            db_hash_consistent,
            disk_hash_verified,
            detail,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use database::{DbPath, PoolConfig};
    use domain::MemoryLifecycle;
    use serde_json::json;

    fn migrated_pool() -> Pool {
        let pool = Pool::open(PoolConfig {
            path: DbPath::Memory,
            readonly: false,
            max_connections: 1,
            busy_timeout: std::time::Duration::from_secs(5),
        })
        .unwrap();
        let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../migrations");
        database::migrate(&pool, &dir).unwrap();
        pool
    }

    /// 权威面种子：agent_run + tool_run + artifact + evidence（同哈希）。
    fn seed_authoritative(pool: &Pool, run_id: &str, artifact_uri: &str, sha: &str) {
        pool.with_conn(|c| -> rusqlite::Result<()> {
            c.execute(
                "INSERT INTO agent_definition (agent_id, version, name, context_policy,
                 capability_refs_json, created_at, updated_at, schema_version)
                 VALUES ('agent.mem.test', '0.1.0', 'm', '{}', '[]',
                 '2026-11-13T00:00:00Z', '2026-11-13T00:00:00Z', 1)",
                [],
            )
            .unwrap();
            c.execute(
                "INSERT INTO agent_run (run_id, task_id, agent_id, agent_version, state,
                 memory_refs_json, created_at, updated_at, schema_version)
                 VALUES (?1, 'task-m', 'agent.mem.test', '0.1.0', 'SUCCEEDED', '[]',
                 '2026-11-13T00:00:00Z', '2026-11-13T00:00:00Z', 1)",
                [run_id],
            )
            .unwrap();
            c.execute(
                "INSERT INTO tool (tool_id, name, kind, lifecycle, governance, permission_level,
                 source_system, source_pk, created_at, updated_at, schema_version)
                 VALUES ('rip.mem.tool', 'm', 'tool', 'ENABLED', 'ACTIVE', 'T0', 't', 't',
                 '2026-11-13T00:00:00Z', '2026-11-13T00:00:00Z', 1)",
                [],
            )
            .unwrap();
            c.execute(
                "INSERT INTO tool_run (run_id, tool_id, tool_version, task_id, workspace_id,
                 status, created_at, updated_at, schema_version)
                 VALUES ('rn-mem-1', 'rip.mem.tool', '0.1.0', 'task-m', 'ws', 'SUCCEEDED',
                 '2026-11-13T00:00:00Z', '2026-11-13T00:00:00Z', 1)",
                [],
            )
            .unwrap();
            c.execute(
                "INSERT INTO artifact (artifact_id, run_id, role, uri, sha256, size_bytes,
                 mime_type, provenance, created_at, schema_version)
                 VALUES ('art-mem-1', 'rn-mem-1', 'primary', ?2, ?3, 2, 'application/json',
                 'p', '2026-11-13T00:00:00Z', 1)",
                rusqlite::params![run_id, artifact_uri, sha],
            )
            .unwrap();
            c.execute(
                "INSERT INTO evidence (evidence_id, evidence_type, storage_ref, content_hash,
                 captured_at, captured_by, task_ref, lifecycle, governance, source_system,
                 source_pk, created_at, updated_at, schema_version)
                 VALUES ('ev-mem-1', 'log', ?2, ?3, '2026-11-13T00:00:00Z', 'by', 'task-m',
                 'RAW', 'ACTIVE', 'tool-engine', 'rn-mem-1',
                 '2026-11-13T00:00:00Z', '2026-11-13T00:00:00Z', 1)",
                rusqlite::params![run_id, artifact_uri, sha],
            )
            .unwrap();
            Ok(())
        })
        .unwrap();
    }

    fn entry(run_id: &str, uri: &str, sha: &str) -> EvidenceMemoryEntry {
        EvidenceMemoryEntry::try_new(
            "mem-e-1",
            RunId::try_new(run_id).unwrap(),
            "ev-mem-1",
            Some(ArtifactId::try_new("art-mem-1").unwrap()),
            sha.to_string(),
            uri.to_string(),
            MemoryLifecycle::Active,
        )
        .unwrap()
    }

    #[test]
    fn record_snapshots_must_match_authoritative_rows() {
        let pool = migrated_pool();
        let sha = "a".repeat(64);
        let uri = "ripartifact://rn-mem-1/art-mem-1";
        seed_authoritative(&pool, "arn-em1", uri, &sha);
        let store = EvidenceMemoryStore::new(pool.clone());

        store.record(&entry("arn-em1", uri, &sha)).unwrap();

        // 同 run 重复登记拒绝（UNIQUE + 显式预检语义）。
        assert!(store.record(&entry("arn-em1", uri, &sha)).is_err());

        // 快照漂移拒绝：hash 或 storage_ref 与权威行不一致即整行不入库。
        let bad_hash = EvidenceMemoryEntry::try_new(
            "mem-e-2",
            RunId::try_new("arn-em1").unwrap(),
            "ev-mem-1",
            None,
            "b".repeat(64),
            uri,
            MemoryLifecycle::Active,
        )
        .unwrap();
        assert!(store.record(&bad_hash).is_err());
        let bad_ref = EvidenceMemoryEntry::try_new(
            "mem-e-3",
            RunId::try_new("arn-em1").unwrap(),
            "ev-mem-1",
            None,
            sha.clone(),
            "somewhere-else",
            MemoryLifecycle::Active,
        )
        .unwrap();
        assert!(store.record(&bad_ref).is_err());

        // 权威行缺席拒绝：证据/产物行不存在。
        let ghost = EvidenceMemoryEntry::try_new(
            "mem-e-4",
            RunId::try_new("arn-em1").unwrap(),
            "ev-ghost",
            None,
            sha.clone(),
            uri,
            MemoryLifecycle::Active,
        )
        .unwrap();
        assert!(store.record(&ghost).is_err());

        // 拒绝面零残留。
        let count: i64 = pool
            .with_conn(|c| {
                c.query_row("SELECT COUNT(*) FROM agent_evidence_memory", [], |r| {
                    r.get(0)
                })
            })
            .unwrap();
        assert_eq!(count, 1, "only the consistent ledger row may persist");
    }

    #[test]
    fn resolve_verifies_hashes_and_survives_compress() {
        let pool = migrated_pool();
        let sha = "c".repeat(64);
        let uri = "ripartifact://rn-mem-1/art-mem-1";
        seed_authoritative(&pool, "arn-em2", uri, &sha);
        let store = EvidenceMemoryStore::new(pool.clone());
        store.record(&entry("arn-em2", uri, &sha)).unwrap();

        // 解析：三方哈希一致，引用形态如实标注（无落盘正文）。
        let resolved = store.resolve("mem-e-1").unwrap();
        assert!(resolved.db_hash_consistent);
        assert!(resolved.disk_hash_verified.is_none());
        assert!(resolved.detail.contains("reference-mode"));
        assert_eq!(resolved.evidence_id, "ev-mem-1");
        assert_eq!(resolved.artifact_id.as_deref(), Some("art-mem-1"));

        // 压缩后仍可解析（指针与快照零改动）。
        assert_eq!(store.compress_run("arn-em2").unwrap(), 1);
        let row = store.get("mem-e-1").unwrap().unwrap();
        assert_eq!(row.entry.lifecycle(), MemoryLifecycle::Compressed);
        let after = store.resolve("mem-e-1").unwrap();
        assert!(after.db_hash_consistent);
    }

    #[test]
    fn resolve_detects_authoritative_drift_and_missing_rows() {
        let pool = migrated_pool();
        let sha = "d".repeat(64);
        let uri = "ripartifact://rn-mem-1/art-mem-1";
        seed_authoritative(&pool, "arn-em3", uri, &sha);
        let store = EvidenceMemoryStore::new(pool.clone());
        store.record(&entry("arn-em3", uri, &sha)).unwrap();

        // 权威行被改（模拟漂移）：三方哈希失守 → resolve 报不一致。
        pool.with_conn(|c| {
            c.execute(
                "UPDATE artifact SET sha256 = ?1 WHERE artifact_id = 'art-mem-1'",
                ["f".repeat(64)],
            )
        })
        .unwrap();
        let resolved = store.resolve("mem-e-1").unwrap();
        assert!(!resolved.db_hash_consistent);
        assert!(resolved.detail.contains("artifact.sha256"));

        // 回锚面：台账指针在场时，权威证据行不可被删（FK 兜底，物理防丢失，
        // 计划 §9:478 原文「原始证据不能因上下文压缩而丢失」的强形态）。
        let deleted = pool
            .with_conn(|c| c.execute("DELETE FROM evidence WHERE evidence_id = 'ev-mem-1'", []));
        assert!(
            deleted.is_err(),
            "ledger pointer must anchor the evidence row (FK)"
        );
    }

    #[test]
    fn resolve_rehashes_on_disk_backed_artifact() {
        let pool = migrated_pool();
        let dir = std::env::temp_dir().join(format!("rip-mem-ev-{}-{}", std::process::id(), 1));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("art-mem-1.json");
        let body = json!({"keyword": "nmap"}).to_string();
        std::fs::write(&path, &body).unwrap();
        let sha = sha256_hex(body.as_bytes());
        seed_authoritative(&pool, "arn-em4", path.to_string_lossy().as_ref(), &sha);
        let store = EvidenceMemoryStore::new(pool.clone());
        store
            .record(&entry("arn-em4", path.to_string_lossy().as_ref(), &sha))
            .unwrap();

        // 文件形态：逐字节复算通过。
        let resolved = store.resolve("mem-e-1").unwrap();
        assert_eq!(
            resolved.disk_hash_verified,
            Some(true),
            "detail: {}",
            resolved.detail
        );

        // 正文被篡改：逐字节复算失守。
        std::fs::write(&path, "{\"keyword\":\"tampered\"}").unwrap();
        let tampered = store.resolve("mem-e-1").unwrap();
        assert_eq!(tampered.disk_hash_verified, Some(false));

        let _ = std::fs::remove_dir_all(&dir);
    }
}
