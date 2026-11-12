//! Research Memory 数据面与读写 API（013 agent_research_memory 表；Context
//! Memory 雏形：可追溯研究成果）。
//!
//! 口径（SESSION-HANDOFF §十追加项原文）：
//! - 每条记忆引用 manifest item/artifact/evidence ID 可解析回链：写入时
//!   （[`ResearchMemoryStore::record`]）逐条当场解析，任一不可解析即整行
//!   拒绝（不留断链成果）；任意时刻可重放 [`ResearchMemoryStore::resolve_refs`]。
//! - 与聊天记忆分离：独立表、独立生命周期，不复用任何会话/上下文存储
//!   （context_manifest 等表只作回链目标，不作存储面）。
//! - 压缩（[`ResearchMemoryStore::compress`]）与归档
//!   （[`ResearchMemoryStore::archive`]）只置空详情载荷，title/summary/
//!   refs_json 原样保留（可追溯面）；无驱逐 API（domain 跃迁表 + 013 CHECK
//!   词值双兜底）。

use crate::error::AgentError;
use database::rusqlite::OptionalExtension;
use database::{rusqlite, Pool};
use domain::{
    AgentId, MemoryLifecycle, MemoryObjectRef, MemoryRefType, ResearchMemoryEntry, RunId,
};

/// refs_json 单条形状（013 注释原文：manifest_item 带 manifest_id + item_id，
/// artifact/evidence 带 ref_id）。
#[derive(serde::Serialize, serde::Deserialize)]
struct RefJson {
    ref_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    manifest_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    item_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    ref_id: Option<String>,
}

/// 库内 Research Memory 行投影。
#[derive(Debug, Clone)]
pub struct StoredResearchMemory {
    /// 域对象。
    pub entry: ResearchMemoryEntry,
    /// 创建时刻。
    pub created_at: String,
    /// 更新时刻。
    pub updated_at: String,
}

/// 单条回链的解析结论。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedRef {
    /// 回链类型。
    pub ref_type: MemoryRefType,
    /// 对象 id（ManifestItem 时为 item_id）。
    pub ref_id: String,
    /// 装配清单 id（仅 ManifestItem）。
    pub manifest_id: Option<String>,
    /// 是否解析成功（回链行存在）。
    pub resolved: bool,
    /// 结论明细。
    pub detail: String,
}

/// Research Memory 读写存储（Context Memory 雏形；只压缩/归档，无驱逐）。
#[derive(Clone)]
pub struct ResearchMemoryStore {
    pool: Pool,
}

impl ResearchMemoryStore {
    /// 在连接池之上打开。
    pub fn new(pool: Pool) -> Self {
        Self { pool }
    }

    fn serialize_refs(refs: &[MemoryObjectRef]) -> String {
        let rows: Vec<RefJson> = refs
            .iter()
            .map(|r| match r.ref_type() {
                MemoryRefType::ManifestItem => RefJson {
                    ref_type: r.ref_type().as_str().to_string(),
                    manifest_id: r.manifest_id().map(str::to_string),
                    item_id: Some(r.ref_id().to_string()),
                    ref_id: None,
                },
                MemoryRefType::Artifact | MemoryRefType::Evidence => RefJson {
                    ref_type: r.ref_type().as_str().to_string(),
                    manifest_id: None,
                    item_id: None,
                    ref_id: Some(r.ref_id().to_string()),
                },
            })
            .collect();
        serde_json::to_string(&rows).unwrap_or_else(|_| "[]".to_string())
    }

    fn deserialize_refs(text: &str) -> Result<Vec<MemoryObjectRef>, AgentError> {
        let rows: Vec<RefJson> = serde_json::from_str(text)
            .map_err(|e| AgentError::Storage(format!("refs_json is not the 013 row shape: {e}")))?;
        let refs = rows
            .into_iter()
            .map(|r| -> Result<MemoryObjectRef, domain::DomainError> {
                match MemoryRefType::parse(&r.ref_type)? {
                    MemoryRefType::ManifestItem => {
                        let manifest_id = r.manifest_id.ok_or_else(|| {
                            domain::DomainError::InvalidAgentMemory {
                                field: "refs_json.manifest_id".to_string(),
                                reason: "manifest_item ref missing manifest_id".to_string(),
                            }
                        })?;
                        let item_id =
                            r.item_id
                                .ok_or_else(|| domain::DomainError::InvalidAgentMemory {
                                    field: "refs_json.item_id".to_string(),
                                    reason: "manifest_item ref missing item_id".to_string(),
                                })?;
                        MemoryObjectRef::manifest_item(manifest_id, item_id)
                    }
                    MemoryRefType::Artifact => {
                        MemoryObjectRef::artifact(r.ref_id.ok_or_else(|| {
                            domain::DomainError::InvalidAgentMemory {
                                field: "refs_json.ref_id".to_string(),
                                reason: "artifact ref missing ref_id".to_string(),
                            }
                        })?)
                    }
                    MemoryRefType::Evidence => {
                        MemoryObjectRef::evidence(r.ref_id.ok_or_else(|| {
                            domain::DomainError::InvalidAgentMemory {
                                field: "refs_json.ref_id".to_string(),
                                reason: "evidence ref missing ref_id".to_string(),
                            }
                        })?)
                    }
                }
            })
            .collect::<Result<Vec<_>, domain::DomainError>>()?;
        Ok(refs)
    }

    /// 写时回链解析（013 契约：任一不可解析即整行拒绝，不留断链成果）。
    fn check_refs_resolvable(&self, refs: &[MemoryObjectRef]) -> Result<(), AgentError> {
        for r in refs {
            let resolved = self.resolve_one(r)?;
            if !resolved.resolved {
                return Err(AgentError::Storage(format!(
                    "unresolvable back-link {} ({}): {}",
                    r.ref_type(),
                    r.ref_id(),
                    resolved.detail
                )));
            }
        }
        Ok(())
    }

    /// 解析单条回链（回链行存在性；evidence 回链附同哈希对账结论）。
    fn resolve_one(&self, r: &MemoryObjectRef) -> Result<ResolvedRef, AgentError> {
        let resolved = match r.ref_type() {
            MemoryRefType::ManifestItem => {
                let manifest_id = r.manifest_id().unwrap_or_default();
                let found: bool = self.pool.with_conn(|c| {
                    let mut stmt = c.prepare(
                        "SELECT 1 FROM context_manifest_item
                         WHERE manifest_id = ?1 AND item_id = ?2",
                    )?;
                    stmt.exists(rusqlite::params![manifest_id, r.ref_id()])
                })?;
                ResolvedRef {
                    ref_type: r.ref_type(),
                    ref_id: r.ref_id().to_string(),
                    manifest_id: Some(manifest_id.to_string()),
                    resolved: found,
                    detail: if found {
                        "context_manifest_item row found".to_string()
                    } else {
                        format!(
                            "no context_manifest_item row for {manifest_id}/{}",
                            r.ref_id()
                        )
                    },
                }
            }
            MemoryRefType::Artifact => {
                let found: bool = self.pool.with_conn(|c| {
                    let mut stmt = c.prepare("SELECT 1 FROM artifact WHERE artifact_id = ?1")?;
                    stmt.exists([r.ref_id()])
                })?;
                ResolvedRef {
                    ref_type: r.ref_type(),
                    ref_id: r.ref_id().to_string(),
                    manifest_id: None,
                    resolved: found,
                    detail: if found {
                        "artifact row found".to_string()
                    } else {
                        format!("no artifact row for {:?}", r.ref_id())
                    },
                }
            }
            MemoryRefType::Evidence => {
                let found: bool = self.pool.with_conn(|c| {
                    let mut stmt = c.prepare("SELECT 1 FROM evidence WHERE evidence_id = ?1")?;
                    stmt.exists([r.ref_id()])
                })?;
                ResolvedRef {
                    ref_type: r.ref_type(),
                    ref_id: r.ref_id().to_string(),
                    manifest_id: None,
                    resolved: found,
                    detail: if found {
                        "evidence row found".to_string()
                    } else {
                        format!("no evidence row for {:?}", r.ref_id())
                    },
                }
            }
        };
        Ok(resolved)
    }

    /// 登记一条研究成果记忆（run 存在性 + 回链写时全量解析，任一失败整行
    /// 拒绝）。
    pub fn record(&self, entry: &ResearchMemoryEntry) -> Result<(), AgentError> {
        let run_id = entry.run_id().as_str();
        let run_exists: bool = self.pool.with_conn(|c| {
            let mut stmt = c.prepare("SELECT 1 FROM agent_run WHERE run_id = ?1")?;
            stmt.exists([run_id])
        })?;
        if !run_exists {
            return Err(AgentError::RunNotFound(run_id.to_string()));
        }
        self.check_refs_resolvable(entry.refs())?;
        let now = domain::time::now_iso();
        self.pool
            .with_conn(|c| {
                c.execute(
                    "INSERT INTO agent_research_memory (memory_id, run_id, task_id, agent_id,
                     title, summary, content_json, refs_json, lifecycle, created_at, updated_at,
                     schema_version)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?10, 1)",
                    rusqlite::params![
                        entry.memory_id(),
                        run_id,
                        entry.task_id(),
                        entry.agent_id().as_str(),
                        entry.title(),
                        entry.summary(),
                        entry.content_json(),
                        Self::serialize_refs(entry.refs()),
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
    pub fn get(&self, memory_id: &str) -> Result<Option<StoredResearchMemory>, AgentError> {
        type Row = (
            String,
            Option<String>,
            String,
            String,
            String,
            String,
            String,
            String,
            String,
            String,
        );
        let row: Option<Row> = self.pool.with_conn(|c| {
            let mut stmt = c.prepare(
                "SELECT run_id, task_id, agent_id, title, summary, content_json, refs_json,
                        lifecycle, created_at, updated_at
                 FROM agent_research_memory WHERE memory_id = ?1",
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
                    r.get(8)?,
                    r.get(9)?,
                ))
            })
            .optional()
        })?;
        row.map(
            |(
                run_id,
                task_id,
                agent_id,
                title,
                summary,
                content_json,
                refs_json,
                lifecycle,
                created_at,
                updated_at,
            )| {
                let lifecycle = MemoryLifecycle::parse(&lifecycle)?;
                let refs = Self::deserialize_refs(&refs_json)?;
                let entry = ResearchMemoryEntry::try_new(
                    memory_id,
                    RunId::try_new(run_id)?,
                    task_id,
                    AgentId::try_new(agent_id)?,
                    title,
                    summary,
                    content_json,
                    refs,
                    lifecycle,
                )?;
                Ok::<StoredResearchMemory, AgentError>(StoredResearchMemory {
                    entry,
                    created_at,
                    updated_at,
                })
            },
        )
        .transpose()
    }

    /// 按 run 列出（登记顺序）。
    pub fn list_by_run(&self, run_id: &str) -> Result<Vec<StoredResearchMemory>, AgentError> {
        let ids: Vec<String> = self.pool.with_conn(|c| {
            let mut stmt = c.prepare(
                "SELECT memory_id FROM agent_research_memory WHERE run_id = ?1 ORDER BY rowid",
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

    /// 压缩：ACTIVE → COMPRESSED，详情载荷置空，title/summary/refs 原样
    /// （域校验 + CAS 词值推进）。
    pub fn compress(&self, memory_id: &str) -> Result<(), AgentError> {
        self.transition(memory_id, MemoryLifecycle::Compressed)
    }

    /// 归档：ACTIVE/COMPRESSED → ARCHIVED，回链原样（成果永不驱逐）。
    pub fn archive(&self, memory_id: &str) -> Result<(), AgentError> {
        self.transition(memory_id, MemoryLifecycle::Archived)
    }

    /// 生命周期推进（域校验 [`MemoryLifecycle::can_transition_kind`] +
    /// CAS UPDATE ... WHERE lifecycle = ?；并发漂移拒绝）。
    fn transition(&self, memory_id: &str, to: MemoryLifecycle) -> Result<(), AgentError> {
        let stored = self.get(memory_id)?.ok_or_else(|| {
            AgentError::Storage(format!("research memory {memory_id:?} not found"))
        })?;
        let from = stored.entry.lifecycle();
        if !from.can_transition_kind(domain::MemoryKind::Research, to) {
            return Err(AgentError::Definition(format!(
                "illegal research memory transition {} -> {} (P7-3 transition table)",
                from.as_str(),
                to.as_str()
            )));
        }
        let now = domain::time::now_iso();
        let updated = self
            .pool
            .with_conn(|c| {
                c.execute(
                    "UPDATE agent_research_memory SET lifecycle = ?2, content_json = '',
                     updated_at = ?3 WHERE memory_id = ?1 AND lifecycle = ?4",
                    rusqlite::params![memory_id, to.as_str(), now, from.as_str()],
                )
            })
            .map_err(AgentError::from)?;
        if updated == 0 {
            return Err(AgentError::Storage(format!(
                "research memory {memory_id:?} lifecycle drifted concurrently"
            )));
        }
        Ok(())
    }

    /// 全量回链解析（压缩/归档后仍可调用：refs_json 原样保留）。逐条返回
    /// 解析结论，调用方据 resolved 判定契约成败。
    pub fn resolve_refs(&self, memory_id: &str) -> Result<Vec<ResolvedRef>, AgentError> {
        let stored = self.get(memory_id)?.ok_or_else(|| {
            AgentError::Storage(format!("research memory {memory_id:?} not found"))
        })?;
        let mut out = Vec::new();
        for r in stored.entry.refs() {
            out.push(self.resolve_one(r)?);
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use database::{DbPath, PoolConfig};
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

    /// 种子：agent_run + 回链目标三行（manifest item/artifact/evidence）。
    fn seed_targets(pool: &Pool, run_id: &str) {
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
                 VALUES (?1, 'task-r', 'agent.mem.test', '0.1.0', 'SUCCEEDED', '[]',
                 '2026-11-13T00:00:00Z', '2026-11-13T00:00:00Z', 1)",
                [run_id],
            )
            .unwrap();
            c.execute(
                "INSERT INTO context_manifest (manifest_id, task_id, intent_json, policy_version,
                 broker_version, budget_json, totals_json, intent_status, selection_hash,
                 created_at, schema_version)
                 VALUES ('ctxm-mem-1', 'task-r', '{}', 'v1', 'v1', '{}', '{}', 'ok', '',
                 '2026-11-13T00:00:00Z', '1')",
                [],
            )
            .unwrap();
            c.execute(
                "INSERT INTO context_manifest_item (manifest_id, item_id, layer, kind, ref_id,
                 ref_version, source, title, reason, matched_signals_json, est_tokens, included,
                 content_hash)
                 VALUES ('ctxm-mem-1', 'ki-mem-1', 'L2', 'knowledge', 'kb:1', NULL,
                 'platform-kb', 't', 'r', '[]', 10, 'summary', NULL)",
                [],
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
                 VALUES ('rn-mem-1', 'rip.mem.tool', '0.1.0', 'task-r', 'ws', 'SUCCEEDED',
                 '2026-11-13T00:00:00Z', '2026-11-13T00:00:00Z', 1)",
                [],
            )
            .unwrap();
            c.execute(
                "INSERT INTO artifact (artifact_id, run_id, role, uri, sha256, size_bytes,
                 mime_type, provenance, created_at, schema_version)
                 VALUES ('art-mem-1', 'rn-mem-1', 'primary', 'ripartifact://rn-mem-1/art-mem-1',
                 ?, 2, 'application/json', 'p', '2026-11-13T00:00:00Z', 1)",
                ["e".repeat(64)],
            )
            .unwrap();
            c.execute(
                "INSERT INTO evidence (evidence_id, evidence_type, storage_ref, content_hash,
                 captured_at, captured_by, task_ref, lifecycle, governance, source_system,
                 source_pk, created_at, updated_at, schema_version)
                 VALUES ('ev-mem-1', 'log', 'ripartifact://rn-mem-1/art-mem-1', ?,
                 '2026-11-13T00:00:00Z', 'by', 'task-r', 'RAW', 'ACTIVE', 'tool-engine',
                 'rn-mem-1', '2026-11-13T00:00:00Z', '2026-11-13T00:00:00Z', 1)",
                ["e".repeat(64)],
            )
            .unwrap();
            Ok(())
        })
        .unwrap();
    }

    fn entry(run_id: &str, refs: Vec<MemoryObjectRef>) -> ResearchMemoryEntry {
        ResearchMemoryEntry::try_new(
            "mem-r-1",
            RunId::try_new(run_id).unwrap(),
            Some("task-r".to_string()),
            AgentId::try_new("agent.mem.test").unwrap(),
            "nmap 检索成果",
            "命中工具与知识各一组",
            json!({"steps": 1, "tools": ["rip.kb.keyword_search"]}).to_string(),
            refs,
            MemoryLifecycle::Active,
        )
        .unwrap()
    }

    fn full_refs() -> Vec<MemoryObjectRef> {
        vec![
            MemoryObjectRef::manifest_item("ctxm-mem-1", "ki-mem-1").unwrap(),
            MemoryObjectRef::artifact("art-mem-1").unwrap(),
            MemoryObjectRef::evidence("ev-mem-1").unwrap(),
        ]
    }

    #[test]
    fn record_rejects_unresolvable_refs_then_roundtrips() {
        let pool = migrated_pool();
        seed_targets(&pool, "arn-rm1");
        let store = ResearchMemoryStore::new(pool.clone());

        // 断链成果拒绝：回链行缺席即整行不入库。
        let broken = vec![
            MemoryObjectRef::manifest_item("ctxm-ghost", "ki-x").unwrap(),
            MemoryObjectRef::artifact("art-mem-1").unwrap(),
            MemoryObjectRef::evidence("ev-mem-1").unwrap(),
        ];
        assert!(store.record(&entry("arn-rm1", broken)).is_err());
        let count: i64 = pool
            .with_conn(|c| {
                c.query_row("SELECT COUNT(*) FROM agent_research_memory", [], |r| {
                    r.get(0)
                })
            })
            .unwrap();
        assert_eq!(count, 0, "unresolvable refs must not leave rows");

        // 全链可解析：登记成功，refs_json 序列化回读一致。
        store.record(&entry("arn-rm1", full_refs())).unwrap();
        let stored = store.get("mem-r-1").unwrap().unwrap();
        assert_eq!(stored.entry.refs(), full_refs());
        assert_eq!(stored.entry.title(), "nmap 检索成果");
    }

    #[test]
    fn compress_and_archive_keep_refs_and_resolve_afterwards() {
        let pool = migrated_pool();
        seed_targets(&pool, "arn-rm2");
        let store = ResearchMemoryStore::new(pool.clone());
        store.record(&entry("arn-rm2", full_refs())).unwrap();

        // 压缩：详情置空，title/summary/refs 原样。
        store.compress("mem-r-1").unwrap();
        let compressed = store.get("mem-r-1").unwrap().unwrap();
        assert_eq!(compressed.entry.lifecycle(), MemoryLifecycle::Compressed);
        assert!(compressed.entry.content_json().is_empty());
        assert_eq!(compressed.entry.refs().len(), 3);
        assert_eq!(compressed.entry.summary(), "命中工具与知识各一组");

        // 归档：回链原样，仍全量可解析。
        store.archive("mem-r-1").unwrap();
        let refs = store.resolve_refs("mem-r-1").unwrap();
        assert_eq!(refs.len(), 3);
        assert!(refs.iter().all(|r| r.resolved), "refs: {refs:?}");

        // 终态不可再推进（域跃迁表拒绝）。
        assert!(store.compress("mem-r-1").is_err());
        assert!(store.archive("mem-r-1").is_err());
    }

    #[test]
    fn resolve_refs_reports_broken_backlinks_after_target_removal() {
        let pool = migrated_pool();
        seed_targets(&pool, "arn-rm3");
        let store = ResearchMemoryStore::new(pool.clone());
        store.record(&entry("arn-rm3", full_refs())).unwrap();

        // 目标行被删（模拟漂移）：resolved=false 如实报告，不伪装可解析。
        pool.with_conn(|c| c.execute("DELETE FROM artifact WHERE artifact_id = 'art-mem-1'", []))
            .unwrap();
        let refs = store.resolve_refs("mem-r-1").unwrap();
        let broken: Vec<_> = refs.iter().filter(|r| !r.resolved).collect();
        assert_eq!(broken.len(), 1);
        assert_eq!(broken[0].ref_type, MemoryRefType::Artifact);
        assert!(broken[0].detail.contains("no artifact row"));
    }
}
