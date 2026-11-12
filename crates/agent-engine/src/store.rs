//! AgentDefinition / AgentRun 持久化读写面（006 迁移两表，P7-1 提前通道
//! 已建表，本批补数据访问，不新建迁移不重复建模）。
//!
//! capability_refs 绑定引用校验接线（ROADMAP §8.3 P7-1 判据「指向不存在
//! 工具即拒绝」的 P7-2 落位）：谓词由 [`tool_registry::Registry`] 查询承载
//! （tool/tool_version 语义面归 Registry，不直读其表），版本钉定比对同为
//! 接线层职责（domain 纯函数只校验存在性，见 domain::agent 模块头注）。
//! 拒绝即整行不入库（INSERT 前置校验，不留半登记）。
//!
//! AgentRun 状态推进：[`AgentStore::transition_run`] 按 domain::agent
//! P7-2 跃迁表做域校验 + 库内 CAS（UPDATE ... WHERE run_id = ? AND state = ?），
//! 并发漂移或非法对都拒绝；RUNNING 进入记 started_at，终态进入记 finished_at。

use crate::error::AgentError;
use crate::new_id;
use database::rusqlite::OptionalExtension;
use database::{rusqlite, Pool};
use domain::{AgentDefinition, AgentId, AgentRun, AgentRunState, CapabilityRef, RunId};
use tool_registry::Registry;

/// capability_refs_json 行形状（006 迁移注释原文：`[{"tool_id": ...,
/// "version": "0.1.0"|null}, ...]`，CapabilityRef 序列化对齐）。
#[derive(serde::Serialize, serde::Deserialize)]
struct RefJson {
    tool_id: String,
    version: Option<String>,
}

/// 库内定义投影（域对象 + 人类可读名 + 登记时间）。
#[derive(Debug, Clone)]
pub struct StoredDefinition {
    /// 域对象。
    pub definition: AgentDefinition,
    /// 人类可读名。
    pub name: String,
    /// 创建时刻。
    pub created_at: String,
    /// 更新时刻。
    pub updated_at: String,
}

/// 库内 run 投影（域对象 + 时间列）。
#[derive(Debug, Clone)]
pub struct StoredRun {
    /// 域对象。
    pub run: AgentRun,
    /// 链路追踪 ID（P7-5 迁移 015 trace_id 列；015 之前落库的行与显式
    /// None 受理为 None，只读查询面按 NULL 如实呈现）。
    pub trace_id: Option<String>,
    /// 创建时刻。
    pub created_at: String,
    /// RUNNING 进入时刻。
    pub started_at: Option<String>,
    /// 终态进入时刻。
    pub finished_at: Option<String>,
}

/// Agent 数据面存储（pool + Registry 只读视图）。
#[derive(Clone)]
pub struct AgentStore {
    pool: Pool,
    registry: Registry,
}

impl AgentStore {
    /// 在连接池与工具注册表之上打开存储。
    pub fn open(pool: Pool, registry: Registry) -> Self {
        Self { pool, registry }
    }

    /// 连接池视图。
    pub fn pool(&self) -> &Pool {
        &self.pool
    }

    /// 绑定引用校验接线（登记与运行前均调用）：任一 ref 指向 Registry 不
    /// 存在的工具即拒绝（domain 判据原样）；版本钉定与 Registry 已登记版本
    /// 比对失败同样拒绝（接线层职责，domain::agent 头注口径）。
    pub fn validate_capability_refs(&self, definition: &AgentDefinition) -> Result<(), AgentError> {
        definition.validate_capability_refs(&|tool_id| self.registry.query(tool_id).is_ok())?;
        for r in definition.capability_refs() {
            if let Some(pinned) = r.version() {
                let detail = self.registry.query(r.tool_id())?;
                if !detail.versions.iter().any(|vr| vr.version == pinned) {
                    return Err(AgentError::Definition(format!(
                        "capability ref version pin {pinned:?} is not registered for tool {}",
                        r.tool_id()
                    )));
                }
            }
        }
        Ok(())
    }

    fn serialize_refs(refs: &[CapabilityRef]) -> String {
        let rows: Vec<RefJson> = refs
            .iter()
            .map(|r| RefJson {
                tool_id: r.tool_id().to_string(),
                version: r.version().map(str::to_string),
            })
            .collect();
        serde_json::to_string(&rows).unwrap_or_else(|_| "[]".to_string())
    }

    fn deserialize_refs(text: &str) -> Result<Vec<CapabilityRef>, AgentError> {
        let rows: Vec<RefJson> = serde_json::from_str(text).map_err(|e| {
            AgentError::Definition(format!(
                "capability_refs_json is not the 006 row shape: {e}"
            ))
        })?;
        rows.into_iter()
            .map(|r| CapabilityRef::try_new(r.tool_id, r.version).map_err(AgentError::from))
            .collect()
    }

    /// 登记定义（前置引用校验；(agent_id, version) 重复即
    /// [`AgentError::DefinitionExists`]，UNIQUE 天然防重 + 显式预检）。
    pub fn register_definition(
        &self,
        definition: &AgentDefinition,
        name: &str,
    ) -> Result<(), AgentError> {
        if name.trim().is_empty() {
            return Err(AgentError::Definition("name must be non-empty".to_string()));
        }
        self.validate_capability_refs(definition)?;
        let agent_id = definition.agent_id().as_str();
        let version = definition.version();
        let exists: bool = self.pool.with_conn(|c| {
            let mut stmt =
                c.prepare("SELECT 1 FROM agent_definition WHERE agent_id = ?1 AND version = ?2")?;
            let found = stmt.exists(rusqlite::params![agent_id, version])?;
            Ok::<bool, rusqlite::Error>(found)
        })?;
        if exists {
            return Err(AgentError::DefinitionExists {
                agent_id: agent_id.to_string(),
                version: version.to_string(),
            });
        }
        let now = domain::time::now_iso();
        self.pool
            .with_conn(|c| {
                c.execute(
                    "INSERT INTO agent_definition (agent_id, version, name, context_policy,
                     capability_refs_json, created_at, updated_at, schema_version)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6, 1)",
                    rusqlite::params![
                        agent_id,
                        version,
                        name,
                        definition.context_policy(),
                        Self::serialize_refs(definition.capability_refs()),
                        now
                    ],
                )?;
                Ok::<(), rusqlite::Error>(())
            })
            .map_err(AgentError::from)?;
        Ok(())
    }

    /// 按 (agent_id, version) 取定义。
    pub fn definition(
        &self,
        agent_id: &str,
        version: &str,
    ) -> Result<Option<StoredDefinition>, AgentError> {
        type DefRow = (String, Option<String>, String, String, String);
        let row: Option<DefRow> = self.pool.with_conn(|c| {
            let mut stmt = c.prepare(
                "SELECT name, context_policy, capability_refs_json, created_at, updated_at
                     FROM agent_definition WHERE agent_id = ?1 AND version = ?2",
            )?;
            stmt.query_row(rusqlite::params![agent_id, version], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, Option<String>>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, String>(4)?,
                ))
            })
            .optional()
        })?;
        row.map(|(name, policy, refs_json, created_at, updated_at)| {
            self.build_definition(
                agent_id, version, name, policy, &refs_json, created_at, updated_at,
            )
        })
        .transpose()
    }

    /// 取定义的最新版本（created_at 降序；同刻按 version 字典序兜底）。
    pub fn latest_definition(
        &self,
        agent_id: &str,
    ) -> Result<Option<StoredDefinition>, AgentError> {
        let versions: Vec<String> = self.pool.with_conn(|c| {
            let mut stmt = c.prepare(
                "SELECT version FROM agent_definition WHERE agent_id = ?1
                     ORDER BY created_at DESC, version DESC",
            )?;
            let rows = stmt
                .query_map([agent_id], |r| r.get::<_, String>(0))?
                .collect::<Result<Vec<_>, rusqlite::Error>>()?;
            Ok::<_, rusqlite::Error>(rows)
        })?;
        match versions.first() {
            Some(v) => self.definition(agent_id, v),
            None => Ok(None),
        }
    }

    /// 全量定义（登记顺序）。
    pub fn list_definitions(&self) -> Result<Vec<StoredDefinition>, AgentError> {
        let pairs: Vec<(String, String)> = self.pool.with_conn(|c| {
            let mut stmt = c.prepare(
                "SELECT agent_id, version FROM agent_definition ORDER BY created_at, agent_id",
            )?;
            let rows = stmt
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
                .collect::<Result<Vec<_>, rusqlite::Error>>()?;
            Ok::<_, rusqlite::Error>(rows)
        })?;
        let mut out = Vec::new();
        for (agent_id, version) in pairs {
            if let Some(stored) = self.definition(&agent_id, &version)? {
                out.push(stored);
            }
        }
        Ok(out)
    }

    #[allow(clippy::too_many_arguments)]
    fn build_definition(
        &self,
        agent_id: &str,
        version: &str,
        name: String,
        policy: Option<String>,
        refs_json: &str,
        created_at: String,
        updated_at: String,
    ) -> Result<StoredDefinition, AgentError> {
        let policy = policy.ok_or_else(|| {
            AgentError::Definition(format!(
                "definition {agent_id}@{version} has NULL context_policy"
            ))
        })?;
        let refs = Self::deserialize_refs(refs_json)?;
        let definition = AgentDefinition::try_new(
            AgentId::try_new(agent_id)?,
            version.to_string(),
            policy,
            refs,
        )?;
        Ok(StoredDefinition {
            definition,
            name,
            created_at,
            updated_at,
        })
    }

    /// 创建 run（QUEUED 落行；定义不存在先拒绝，FK 前置显式报因）。
    /// trace_id（P7-5 迁移 015）：None 时不落关联列（015 之前的调用面
    /// 语义不变）；Some 时须过 domain::ids::TraceId 校验。重试链继承源
    /// run 的 trace_id（RunControl::retry 传入），跨 run 关联同链。
    pub fn create_run(&self, run: &AgentRun, trace_id: Option<&str>) -> Result<(), AgentError> {
        let agent_id = run.agent_id().as_str();
        let agent_version = run.agent_version();
        let exists: bool = self.pool.with_conn(|c| {
            let mut stmt =
                c.prepare("SELECT 1 FROM agent_definition WHERE agent_id = ?1 AND version = ?2")?;
            let found = stmt.exists(rusqlite::params![agent_id, agent_version])?;
            Ok::<bool, rusqlite::Error>(found)
        })?;
        if !exists {
            return Err(AgentError::Definition(format!(
                "definition {agent_id}@{agent_version} not found; agent_run FK requires it"
            )));
        }
        let trace = trace_id
            .map(domain::TraceId::try_new)
            .transpose()
            .map_err(|e| AgentError::Storage(e.to_string()))?;
        let now = domain::time::now_iso();
        let refs: Vec<String> = run.memory_refs().to_vec();
        self.pool
            .with_conn(|c| {
                c.execute(
                    "INSERT INTO agent_run (run_id, task_id, agent_id, agent_version, state,
                     memory_refs_json, trace_id, created_at, updated_at, schema_version)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8, 1)",
                    rusqlite::params![
                        run.run_id().as_str(),
                        run.task_id(),
                        agent_id,
                        agent_version,
                        run.state().as_str(),
                        serde_json::to_string(&refs).unwrap_or_else(|_| "[]".to_string()),
                        trace.as_ref().map(|t| t.as_str()),
                        now
                    ],
                )?;
                Ok::<(), rusqlite::Error>(())
            })
            .map_err(AgentError::from)?;
        Ok(())
    }

    /// 按 run_id 取 run。
    pub fn run(&self, run_id: &str) -> Result<Option<StoredRun>, AgentError> {
        type RunRow = (
            Option<String>,
            String,
            String,
            String,
            String,
            String,
            Option<String>,
            Option<String>,
            Option<String>,
        );
        let row: Option<RunRow> = self.pool.with_conn(|c| {
            let mut stmt = c.prepare(
                "SELECT task_id, agent_id, agent_version, state, memory_refs_json,
                            created_at, started_at, finished_at, trace_id
                     FROM agent_run WHERE run_id = ?1",
            )?;
            stmt.query_row([run_id], |r| {
                Ok((
                    r.get::<_, Option<String>>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, String>(4)?,
                    r.get::<_, String>(5)?,
                    r.get::<_, Option<String>>(6)?,
                    r.get::<_, Option<String>>(7)?,
                    r.get::<_, Option<String>>(8)?,
                ))
            })
            .optional()
        })?;
        row.map(
            |(
                task_id,
                agent_id,
                agent_version,
                state,
                refs_json,
                created_at,
                started_at,
                finished_at,
                trace_id,
            )| {
                let state = AgentRunState::ALL
                    .into_iter()
                    .find(|s| s.as_str() == state)
                    .ok_or_else(|| {
                        AgentError::Storage(format!("agent_run {run_id:?} has unknown state value"))
                    })?;
                let refs: Vec<String> = serde_json::from_str(&refs_json).unwrap_or_default();
                let run = AgentRun::try_new(
                    RunId::try_new(run_id)?,
                    task_id,
                    AgentId::try_new(agent_id)?,
                    agent_version,
                    state,
                    refs,
                )?;
                Ok::<StoredRun, AgentError>(StoredRun {
                    run,
                    trace_id,
                    created_at,
                    started_at,
                    finished_at,
                })
            },
        )
        .transpose()
    }

    /// 状态推进（域校验 + CAS）：目标态 RUNNING 记 started_at，终态记
    /// finished_at；rows affected 为 0（非法对或并发漂移）即拒绝。
    pub fn transition_run(&self, run_id: &str, to: AgentRunState) -> Result<AgentRun, AgentError> {
        let stored = self
            .run(run_id)?
            .ok_or_else(|| AgentError::RunNotFound(run_id.to_string()))?;
        let from = stored.run.state();
        if !from.can_transition(to) {
            return Err(AgentError::InvalidTransition {
                run_id: run_id.to_string(),
                from: from.as_str().to_string(),
                to: to.as_str().to_string(),
            });
        }
        let now = domain::time::now_iso();
        let updated = self
            .pool
            .with_conn(|c| {
                if to == AgentRunState::Running {
                    c.execute(
                        "UPDATE agent_run SET state = ?2, started_at = ?3, updated_at = ?3
                         WHERE run_id = ?1 AND state = ?4",
                        rusqlite::params![run_id, to.as_str(), now, from.as_str()],
                    )
                } else if to.is_terminal() {
                    c.execute(
                        "UPDATE agent_run SET state = ?2, finished_at = ?3, updated_at = ?3
                         WHERE run_id = ?1 AND state = ?4",
                        rusqlite::params![run_id, to.as_str(), now, from.as_str()],
                    )
                } else {
                    c.execute(
                        "UPDATE agent_run SET state = ?2, updated_at = ?3
                         WHERE run_id = ?1 AND state = ?4",
                        rusqlite::params![run_id, to.as_str(), now, from.as_str()],
                    )
                }
            })
            .map_err(AgentError::from)?;
        if updated == 0 {
            // CAS 零行：域校验已过但库内当前态已漂移（并发写）。
            return Err(AgentError::InvalidTransition {
                run_id: run_id.to_string(),
                from: from.as_str().to_string(),
                to: to.as_str().to_string(),
            });
        }
        self.run(run_id)?
            .map(|s| s.run)
            .ok_or_else(|| AgentError::RunNotFound(run_id.to_string()))
    }

    /// 追加记忆引用（去重写回；引用串须非空）。
    pub fn append_memory_refs(&self, run_id: &str, refs: &[String]) -> Result<(), AgentError> {
        if refs.iter().any(|r| r.trim().is_empty()) {
            return Err(AgentError::Definition(
                "memory_refs items must be non-empty".to_string(),
            ));
        }
        let stored = self
            .run(run_id)?
            .ok_or_else(|| AgentError::RunNotFound(run_id.to_string()))?;
        let mut current: Vec<String> = stored.run.memory_refs().to_vec();
        for r in refs {
            if !current.contains(r) {
                current.push(r.clone());
            }
        }
        self.pool
            .with_conn(|c| {
                c.execute(
                    "UPDATE agent_run SET memory_refs_json = ?2, updated_at = ?3 WHERE run_id = ?1",
                    rusqlite::params![
                        run_id,
                        serde_json::to_string(&current).unwrap_or_else(|_| "[]".to_string()),
                        domain::time::now_iso()
                    ],
                )?;
                Ok::<(), rusqlite::Error>(())
            })
            .map_err(AgentError::from)?;
        Ok(())
    }

    /// 新建 run 的 id（`arn-` 前缀：与 tool_run 的 `rn-` 区分命名空间，
    /// 审计 correlation_id 全链一致）。
    pub fn new_run_id() -> RunId {
        // 长度上限 128：arn- + 24 hex 远低于上限，unwrap 安全。
        RunId::try_new(new_id("arn")).expect("run id within 128 chars")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use database::{DbPath, PoolConfig};
    use std::path::PathBuf;
    use std::time::Duration;

    fn memory_pool() -> Pool {
        let pool = Pool::open(PoolConfig {
            path: DbPath::Memory,
            readonly: false,
            max_connections: 1,
            busy_timeout: Duration::from_secs(5),
        })
        .unwrap();
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../migrations");
        database::migrate(&pool, &dir).unwrap();
        pool
    }

    fn store_with_tool(pool: &Pool, tool_id: &str) -> AgentStore {
        let registry = Registry::open(pool.clone());
        if let Some(manifest) = repo_manifest(tool_id) {
            registry.register(&manifest).unwrap();
            // Broker 目录面只放行 ENABLED（catalog_visible 口径）。
            pool.with_conn(|c| {
                c.execute(
                    "UPDATE tool SET lifecycle = 'ENABLED' WHERE tool_id = ?1",
                    [tool_id],
                )
            })
            .unwrap();
        }
        AgentStore::open(pool.clone(), registry)
    }

    /// 从仓库 manifests 目录取登记 Manifest（真实登记面，不用手写假件）。
    fn repo_manifest(tool_id: &str) -> Option<serde_json::Value> {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../manifests/tools")
            .join(format!("{tool_id}.json"));
        let text = std::fs::read_to_string(path).ok()?;
        serde_json::from_str(&text).ok()
    }

    fn def(agent_id: &str, tool_id: &str) -> AgentDefinition {
        AgentDefinition::try_new(
            AgentId::try_new(agent_id).unwrap(),
            "0.1.0".to_string(),
            "{\"l0_rules\":true}".to_string(),
            vec![CapabilityRef::try_new(tool_id, None).unwrap()],
        )
        .unwrap()
    }

    #[test]
    fn definition_roundtrip_and_dup_rejected() {
        let pool = memory_pool();
        let store = store_with_tool(&pool, "rip.kb.keyword_search");
        store
            .register_definition(
                &def("agent.research.minimal", "rip.kb.keyword_search"),
                "最小研究 Agent",
            )
            .unwrap();

        let stored = store
            .definition("agent.research.minimal", "0.1.0")
            .unwrap()
            .unwrap();
        assert_eq!(stored.name, "最小研究 Agent");
        assert_eq!(stored.definition.capability_refs().len(), 1);
        assert_eq!(
            stored.definition.capability_refs()[0].tool_id(),
            "rip.kb.keyword_search"
        );
        assert_eq!(stored.definition.context_policy(), "{\"l0_rules\":true}");
        assert!(store
            .latest_definition("agent.research.minimal")
            .unwrap()
            .is_some());

        // (agent_id, version) 重复登记拒绝（UNIQUE 主键 + 显式预检）。
        assert!(matches!(
            store.register_definition(&def("agent.research.minimal", "rip.kb.keyword_search"), "x"),
            Err(AgentError::DefinitionExists { .. })
        ));

        // 最新版本面：登记 0.2.0 后 latest 指向新版本。
        let v2 = AgentDefinition::try_new(
            AgentId::try_new("agent.research.minimal").unwrap(),
            "0.2.0".to_string(),
            "{}".to_string(),
            vec![
                CapabilityRef::try_new("rip.kb.keyword_search", Some("0.1.0".to_string())).unwrap(),
            ],
        )
        .unwrap();
        store.register_definition(&v2, "最小研究 Agent").unwrap();
        assert_eq!(
            store
                .latest_definition("agent.research.minimal")
                .unwrap()
                .unwrap()
                .definition
                .version(),
            "0.2.0"
        );
    }

    #[test]
    fn capability_refs_unknown_tool_rejected_and_no_row_written() {
        let pool = memory_pool();
        let store = store_with_tool(&pool, "rip.kb.keyword_search");

        // P7-1 判据接线：指向不存在工具的 capability_refs 拒绝。
        let ghost = def("agent.ghost", "rip.ghost.tool");
        let err = store.register_definition(&ghost, "幽灵绑定").unwrap_err();
        assert!(
            err.to_string().contains("unknown tool") || err.to_string().contains("rip.ghost.tool"),
            "err: {err}"
        );

        // 版本钉定与 Registry 已登记版本比对：未登记版本拒绝。
        let bad_pin = AgentDefinition::try_new(
            AgentId::try_new("agent.badpin").unwrap(),
            "0.1.0".to_string(),
            "{}".to_string(),
            vec![
                CapabilityRef::try_new("rip.kb.keyword_search", Some("9.9.9".to_string())).unwrap(),
            ],
        )
        .unwrap();
        assert!(store.register_definition(&bad_pin, "坏钉定").is_err());

        // 拒绝即零行：agent_definition 表无幽灵/坏钉定残留。
        let count: i64 = pool
            .with_conn(|c| c.query_row("SELECT COUNT(*) FROM agent_definition", [], |r| r.get(0)))
            .unwrap();
        assert_eq!(count, 0, "rejected definitions must not leave rows");
    }

    #[test]
    fn run_lifecycle_state_machine_and_memory_refs() {
        let pool = memory_pool();
        let store = store_with_tool(&pool, "rip.kb.keyword_search");
        store
            .register_definition(
                &def("agent.research.minimal", "rip.kb.keyword_search"),
                "最小",
            )
            .unwrap();

        // 定义不存在时建 run 拒绝（FK 数据面完整性显式报因）。
        let orphan = AgentRun::try_new(
            RunId::try_new("arn-orphan").unwrap(),
            None,
            AgentId::try_new("agent.nobody").unwrap(),
            "0.1.0".to_string(),
            AgentRunState::Queued,
            vec![],
        )
        .unwrap();
        assert!(store.create_run(&orphan, None).is_err());

        let run = AgentRun::try_new(
            RunId::try_new("arn-lc1").unwrap(),
            Some("task-42".to_string()),
            AgentId::try_new("agent.research.minimal").unwrap(),
            "0.1.0".to_string(),
            AgentRunState::Queued,
            vec![],
        )
        .unwrap();
        store.create_run(&run, None).unwrap();

        // QUEUED → RUNNING（记 started_at）→ SUCCEEDED（记 finished_at）。
        let running = store
            .transition_run("arn-lc1", AgentRunState::Running)
            .unwrap();
        assert_eq!(running.state(), AgentRunState::Running);
        let started_at = store.run("arn-lc1").unwrap().unwrap().started_at;
        assert!(started_at.is_some(), "RUNNING entry must stamp started_at");

        store
            .append_memory_refs("arn-lc1", &["mem://manifest/m1".to_string()])
            .unwrap();
        store
            .append_memory_refs(
                "arn-lc1",
                &[
                    "mem://manifest/m1".to_string(),
                    "mem://evidence/e1".to_string(),
                ],
            )
            .unwrap();
        let with_refs = store.run("arn-lc1").unwrap().unwrap();
        assert_eq!(
            with_refs.run.memory_refs(),
            &[
                "mem://manifest/m1".to_string(),
                "mem://evidence/e1".to_string()
            ]
        );

        let done = store
            .transition_run("arn-lc1", AgentRunState::Succeeded)
            .unwrap();
        assert_eq!(done.state(), AgentRunState::Succeeded);
        assert!(store.run("arn-lc1").unwrap().unwrap().finished_at.is_some());

        // 终态不可迁出（CAS 前域校验拒绝）。
        assert!(matches!(
            store.transition_run("arn-lc1", AgentRunState::Failed),
            Err(AgentError::InvalidTransition { .. })
        ));

        // 非法对：QUEUED 直接 SUCCEEDED（fresh run）。
        let run2 = AgentRun::try_new(
            RunId::try_new("arn-lc2").unwrap(),
            None,
            AgentId::try_new("agent.research.minimal").unwrap(),
            "0.1.0".to_string(),
            AgentRunState::Queued,
            vec![],
        )
        .unwrap();
        store.create_run(&run2, None).unwrap();
        assert!(matches!(
            store.transition_run("arn-lc2", AgentRunState::Succeeded),
            Err(AgentError::InvalidTransition { .. })
        ));

        // 不存在的 run。
        assert!(matches!(
            store.transition_run("arn-nope", AgentRunState::Running),
            Err(AgentError::RunNotFound(_))
        ));
    }
}
