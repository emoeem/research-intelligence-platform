//! WorkflowDefinition 持久化读写面（016 迁移 workflow_definition 表）。
//!
//! 版本管理语义（P8-1 冻结，ROADMAP §9.4 验收基线）：
//! - [`WorkflowStore::put`]：装载门（parse + Schema 门）与静态校验门全过
//!   才入库；版本号由存储侧分配（同 workflow_id 现有最大版本 + 1），每次
//!   put 追加一版，历史行不可变（无 UPDATE 路径），`get <id> --version N`
//!   回读历史版逐字节一致（definition_json 列存 put 原文）；
//! - [`WorkflowStore::import_legacy`]：003 workflow 表（RTIS 方法层定义，
//!   实查 13 行 WF-00..WF-12，trigger/phases/stop_conditions JSON，非可
//!   执行节点图）以 compat=legacy-method-reference 只读参考导入：空图 +
//!   legacy 原文快照逐字段落 legacy_json 列，DSL 不扭曲（ROADMAP §9.5
//!   风险 2）；幂等（同 workflow_id 已有 v1 即跳过）；
//! - WorkflowRun 持久化面不在本批：run 表属 P8-2/P8-3（17 号段待占），
//!   词表已冻结在 domain::workflow。

use crate::dsl::{self, DefinitionV1};
use crate::error::WorkflowError;
use crate::validate;
use database::rusqlite::OptionalExtension;
use database::{rusqlite, Pool, PoolConfig};
use domain::{WorkflowCompat, WorkflowDefinition};
use serde_json::json;

/// Workflow 定义存储（016 表读写面）。
#[derive(Clone)]
pub struct WorkflowStore {
    pool: Pool,
}

/// put 结果（新版本行）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PutOutcome {
    /// workflow id。
    pub workflow_id: String,
    /// 存储侧分配的版本号（现有最大版本 + 1）。
    pub version: u32,
    /// 装载时钉定的 compat 词形。
    pub compat: String,
    /// 节点数。
    pub node_count: usize,
    /// 边数。
    pub edge_count: usize,
}

/// get 返回的存储行投影（域向量 + 回读原文）。
#[derive(Debug, Clone, PartialEq)]
pub struct StoredDefinition {
    /// 域向量。
    pub definition: WorkflowDefinition,
    /// definition_json 列原文（put 时不规整，回读逐字节一致）。
    pub definition_json: String,
    /// input_schema_json 列（展开列，与 definition_json 内一致）。
    pub input_schema_json: Option<String>,
    /// 描述。
    pub description: Option<String>,
    /// 兼容面。
    pub compat: WorkflowCompat,
    /// 来源标记（rp-workflow-put / rp-workflow-import-legacy）。
    pub source: String,
    /// legacy 快照原文（仅 legacy-method-reference 行有值）。
    pub legacy_json: Option<String>,
    /// 登记时刻。
    pub created_at: String,
}

/// list 行投影（每 workflow_id 一行，最新版本）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListRow {
    /// workflow id。
    pub workflow_id: String,
    /// 最新版本号。
    pub latest_version: u32,
    /// 版本总数。
    pub version_count: u32,
    /// 最新版名。
    pub name: String,
    /// 最新版 compat。
    pub compat: String,
    /// 最新版节点数。
    pub node_count: u32,
    /// 最新版边数。
    pub edge_count: u32,
    /// 最新版登记时刻。
    pub created_at: String,
}

/// import-legacy 结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegacyImportReport {
    /// 实际导入行数（写入 workflow_definition v1）。
    pub imported: u32,
    /// 跳过行数（该 workflow_id v1 已存在，幂等跳过）。
    pub skipped: u32,
    /// 源表实查行数。
    pub source_rows: u32,
    /// 源表形态结论（导入依据，随报告透出）。
    pub conclusion: String,
}

/// 源库 workflow 表行（003 形状，只读消费）。
struct LegacyRow {
    workflow_id: String,
    name: String,
    description: Option<String>,
    legacy_json: String,
}

impl WorkflowStore {
    /// 构造（调用方先行 database::migrate 保证 016 表存在）。
    pub fn new(pool: Pool) -> Self {
        Self { pool }
    }

    /// 追加一版定义（装载门 + 静态校验门前置，拒绝即整行不入库）。
    ///
    /// 版本号由存储侧分配（现有最大版本 + 1）：文档内的 version 字段不作为
    /// 分配依据，落库文本（definition_json）以存储侧规范化形状重写
    /// （[`dsl::canonical_document`]，version 字段恒等于存储行版本），同输入
    /// 恒同字节；get 回读该规范形状，同版本任意两次 get 逐字节一致（版本
    /// 管理与历史可查的判据口径），与 003 etl json.dumps(sort_keys=True)
    /// 的入库规范化先例同款。
    pub fn put(&self, doc: &DefinitionV1, source: &str) -> Result<PutOutcome, WorkflowError> {
        let def = dsl::to_domain(doc)?;
        let issues = validate::validate_definition(&def);
        if !issues.is_empty() {
            let joined = issues
                .iter()
                .map(|i| i.message.clone())
                .collect::<Vec<_>>()
                .join("; ");
            return Err(WorkflowError::Validation(joined));
        }
        let compat = def.compat().as_str().to_string();
        let node_count = def.nodes().len();
        let edge_count = def.edges().len();
        let workflow_id = def.workflow_id().as_str().to_string();
        let input_schema_json = def.input_schema_json().map(str::to_string);
        let name = def.name().to_string();
        let description = def.description().map(str::to_string);
        let source = source.to_string();

        // 落库文本：存储侧规范化形状（版本字段由分配结果回填，同输入恒同字节）。
        // 版本分配与插入两步非原子，竞争面是同 workflow 并发 put；CLI 单写者
        // 场景足够，主键 (workflow_id, version) 兜底重复插入必失败。
        let version: u32 = self
            .pool
            .with_conn(|c| {
                c.query_row(
                    "SELECT COALESCE(MAX(version), 0) + 1 FROM workflow_definition
                     WHERE workflow_id = ?1",
                    rusqlite::params![workflow_id],
                    |r| r.get(0),
                )
                .map_err(database::DbError::Sqlite)
            })
            .map_err(WorkflowError::from)?;
        // 以存储行版本重建域对象（try_new 与首次构造同校验面），再序列化为
        // 规范文本（version 字段与存储行一致）。
        let canonical_def = WorkflowDefinition::try_new(
            def.workflow_id().clone(),
            version,
            def.name().to_string(),
            def.description().map(str::to_string),
            def.compat(),
            def.nodes().to_vec(),
            def.edges().to_vec(),
            def.input_schema_json().map(str::to_string),
        )?;
        let canonical_text = dsl::canonical_document(&canonical_def)?;
        // 自检：规范文本可回读且版本字段与存储行一致。
        let reparsed = dsl::parse_definition(&canonical_text)?;
        debug_assert_eq!(reparsed.version, version);
        let now = domain::time::now_iso();
        self.pool
            .with_conn(|c| {
                c.execute(
                    "INSERT INTO workflow_definition (workflow_id, version, name, description,
                     definition_json, input_schema_json, node_count, edge_count, compat,
                     source, legacy_json, created_at, schema_version)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, NULL, ?11, 1)",
                    rusqlite::params![
                        workflow_id,
                        version,
                        name,
                        description,
                        canonical_text,
                        input_schema_json,
                        node_count,
                        edge_count,
                        compat,
                        source,
                        now
                    ],
                )
                .map_err(database::DbError::Sqlite)?;
                Ok::<_, database::DbError>(())
            })
            .map_err(WorkflowError::from)?;
        Ok(PutOutcome {
            workflow_id,
            version,
            compat,
            node_count,
            edge_count,
        })
    }

    /// 取定义：`version = Some(n)` 精确取历史版；None 取最新版。
    pub fn get(
        &self,
        workflow_id: &str,
        version: Option<u32>,
    ) -> Result<StoredDefinition, WorkflowError> {
        let row = match version {
            Some(v) => self
                .pool
                .with_conn(|c| {
                    c.query_row(
                        "SELECT definition_json, input_schema_json, compat, source,
                                legacy_json, created_at, node_count, edge_count
                         FROM workflow_definition WHERE workflow_id = ?1 AND version = ?2",
                        rusqlite::params![workflow_id, v],
                        |r| {
                            Ok((
                                r.get::<_, String>(0)?,
                                r.get::<_, Option<String>>(1)?,
                                r.get::<_, String>(2)?,
                                r.get::<_, String>(3)?,
                                r.get::<_, Option<String>>(4)?,
                                r.get::<_, String>(5)?,
                                r.get::<_, u32>(6)?,
                                r.get::<_, u32>(7)?,
                            ))
                        },
                    )
                    .optional()
                })
                .map_err(WorkflowError::from)?,
            None => self
                .pool
                .with_conn(|c| {
                    c.query_row(
                        "SELECT definition_json, input_schema_json, compat, source,
                                legacy_json, created_at, node_count, edge_count
                         FROM workflow_definition
                         WHERE workflow_id = ?1 AND version =
                               (SELECT MAX(version) FROM workflow_definition WHERE workflow_id = ?1)",
                        rusqlite::params![workflow_id],
                        |r| {
                            Ok((
                                r.get::<_, String>(0)?,
                                r.get::<_, Option<String>>(1)?,
                                r.get::<_, String>(2)?,
                                r.get::<_, String>(3)?,
                                r.get::<_, Option<String>>(4)?,
                                r.get::<_, String>(5)?,
                                r.get::<_, u32>(6)?,
                                r.get::<_, u32>(7)?,
                            ))
                        },
                    )
                    .optional()
                })
                .map_err(WorkflowError::from)?,
        };
        let Some((definition_json, input_schema_json, compat, source, legacy_json, created_at, node_count, edge_count)) = row
        else {
            return Err(WorkflowError::NotFound {
                workflow_id: workflow_id.to_string(),
                version,
            });
        };
        let doc = dsl::parse_definition(&definition_json)?;
        let definition = dsl::to_domain(&doc)?;
        // 存储行不变式：节点/边计数与文档一致（防手工改库后静默漂移）。
        if definition.nodes().len() as u32 != node_count
            || definition.edges().len() as u32 != edge_count
        {
            return Err(WorkflowError::Storage(format!(
                "workflow_definition {workflow_id} row invariant broken: node/edge count mismatch"
            )));
        }
        let description = definition.description().map(str::to_string);
        let compat = WorkflowCompat::parse(&compat)
            .map_err(|e| WorkflowError::Storage(e.to_string()))?;
        Ok(StoredDefinition {
            definition,
            definition_json,
            description,
            compat,
            source,
            legacy_json,
            created_at,
            input_schema_json,
        })
    }
}

/// ListRow 占位收口（上方结构体字段覆盖 list 查询面）。
impl WorkflowStore {
    /// 全量清单：每 workflow_id 一行（最新版本 + 版本总数），按 id 升序。
    pub fn list(&self) -> Result<Vec<ListRow>, WorkflowError> {
        let rows: Vec<(String, u32, String, String, u32, u32, String)> = self
            .pool
            .with_conn(|c| {
                // SQLite max() 聚合配套裸列取自命中行（官方文档语义）。
                let mut stmt = c.prepare(
                    "SELECT workflow_id, MAX(version), name, compat, node_count, edge_count,
                            created_at
                     FROM workflow_definition GROUP BY workflow_id ORDER BY workflow_id",
                )?;
                let rows = stmt
                    .query_map([], |r| {
                        Ok((
                            r.get::<_, String>(0)?,
                            r.get::<_, u32>(1)?,
                            r.get::<_, String>(2)?,
                            r.get::<_, String>(3)?,
                            r.get::<_, u32>(4)?,
                            r.get::<_, u32>(5)?,
                            r.get::<_, String>(6)?,
                        ))
                    })?
                    .collect::<Result<Vec<_>, rusqlite::Error>>()?;
                Ok::<_, rusqlite::Error>(rows)
            })
            .map_err(WorkflowError::from)?;
        let mut out = Vec::with_capacity(rows.len());
        for (workflow_id, latest_version, name, compat, node_count, edge_count, created_at) in rows {
            let version_count: u32 = self
                .pool
                .with_conn(|c| {
                    c.query_row(
                        "SELECT COUNT(*) FROM workflow_definition WHERE workflow_id = ?1",
                        rusqlite::params![workflow_id],
                        |r| r.get::<_, u32>(0),
                    )
                    .map_err(database::DbError::Sqlite)
                })
                .map_err(WorkflowError::from)?;
            out.push(ListRow {
                workflow_id,
                latest_version,
                version_count,
                name,
                compat,
                node_count,
                edge_count,
                created_at,
            });
        }
        Ok(out)
    }

    /// 003 workflow 表存量行只读参考导入（幂等：v1 已存在即跳过）。
    ///
    /// `src` 为只读池（CLI 侧以 readonly_file 打开源库；本函数不写源库）。
    /// 导入行形状：空图 + compat=legacy-method-reference + legacy_json 快照
    /// （trigger/requires/related_skills/knowledge_queries/phases/
    /// stop_conditions/excluded_activities 逐字段 JSON 保真）。
    pub fn import_legacy(&self, src: &Pool) -> Result<LegacyImportReport, WorkflowError> {
        // 源表实查（只读）：行数 + 形状。
        let source_rows: u32 = src
            .with_conn(|c| {
                c.query_row("SELECT COUNT(*) FROM workflow", [], |r| r.get::<_, u32>(0))
                    .map_err(database::DbError::Sqlite)
            })
            .map_err(|e| {
                WorkflowError::LegacyImport(format!(
                    "source workflow table unreadable (003 shape required): {e}"
                ))
            })?;
        let legacy_rows: Vec<LegacyRow> = src
            .with_conn(|c| {
                let mut stmt = c.prepare(
                    "SELECT workflow_id, name, description, trigger_json, requires_json,
                            related_skills, knowledge_queries, phases, stop_conditions,
                            excluded_activities, source_system, source_file, created_at
                     FROM workflow ORDER BY workflow_id",
                )?;
                let rows = stmt
                    .query_map([], |r| {
                        Ok(LegacyRow {
                            workflow_id: r.get(0)?,
                            name: r.get(1)?,
                            description: r.get(2)?,
                            legacy_json: serde_json::to_string(&json!({
                                "source_system": r.get::<_, String>(10)?,
                                "source_file": r.get::<_, String>(11)?,
                                "source_created_at": r.get::<_, String>(12)?,
                                "trigger_json": r.get::<_, Option<String>>(3)?,
                                "requires_json": r.get::<_, Option<String>>(4)?,
                                "related_skills": r.get::<_, Option<String>>(5)?,
                                "knowledge_queries": r.get::<_, Option<String>>(6)?,
                                "phases": r.get::<_, Option<String>>(7)?,
                                "stop_conditions": r.get::<_, Option<String>>(8)?,
                                "excluded_activities": r.get::<_, Option<String>>(9)?,
                            }))
                            .map_err(|e| rusqlite::Error::ToSqlConversionFailure(
                                format!("legacy snapshot serialize failed: {e}").into(),
                            ))?,
                        })
                    })?
                    .collect::<Result<Vec<_>, rusqlite::Error>>()?;
                Ok::<_, rusqlite::Error>(rows)
            })
            .map_err(|e| WorkflowError::LegacyImport(format!("source rows unreadable: {e}")))?;

        let conclusion = "003 workflow table is RTIS method-layer definitions (trigger/phases/stop_conditions JSON), \
             not executable node graphs; imported as read-only reference rows \
             (compat=legacy-method-reference, empty graph), DSL not distorted";

        let mut imported = 0u32;
        let mut skipped = 0u32;
        for row in &legacy_rows {
            let doc = DefinitionV1 {
                schema: dsl::DEFINITION_SCHEMA_ID.to_string(),
                workflow_id: row.workflow_id.clone(),
                version: 1,
                name: row.name.clone(),
                description: row.description.clone(),
                compat: dsl::CompatV1::LegacyMethodReference,
                input_schema: None,
                graph: dsl::GraphV1::default(),
            };
            // 参考行走 Schema 门（条件约束放行空图）但不走图规则：
            // parse_definition 只做形状 + Schema 门；静态图规则对 legacy 行
            // 本身豁免（validate 规则 0），这里不需要跑。
            let raw = serde_json::to_string(&doc)
                .map_err(|e| WorkflowError::LegacyImport(format!("serialize failed: {e}")))?;
            let _ = dsl::parse_definition(&raw)?;
            // 幂等：v1 已存在即跳过（导入面不产生 v2+）。
            let exists: bool = self
                .pool
                .with_conn(|c| {
                    c.query_row(
                        "SELECT 1 FROM workflow_definition WHERE workflow_id = ?1 AND version = 1",
                        rusqlite::params![row.workflow_id],
                        |_| Ok(()),
                    )
                    .optional()
                    .map(|opt| opt.is_some())
                    .map_err(database::DbError::Sqlite)
                })
                .map_err(WorkflowError::from)?;
            if exists {
                skipped += 1;
                continue;
            }
            let name = row.name.clone();
            let description = row.description.clone();
            let legacy_json = row.legacy_json.clone();
            let workflow_id = row.workflow_id.clone();
            let raw = raw.clone();
            let now = domain::time::now_iso();
            self.pool
                .with_conn(|c| {
                    c.execute(
                        "INSERT INTO workflow_definition (workflow_id, version, name, description,
                         definition_json, input_schema_json, node_count, edge_count, compat,
                         source, legacy_json, created_at, schema_version)
                         VALUES (?1, 1, ?2, ?3, ?4, NULL, 0, 0, 'legacy-method-reference',
                                 'rp-workflow-import-legacy', ?5, ?6, 1)",
                        rusqlite::params![workflow_id, name, description, raw, legacy_json, now],
                    )
                    .map_err(database::DbError::Sqlite)?;
                    Ok::<_, database::DbError>(())
                })
                .map_err(WorkflowError::from)?;
            imported += 1;
        }
        Ok(LegacyImportReport {
            imported,
            skipped,
            source_rows,
            conclusion: conclusion.to_string(),
        })
    }
}

/// 便捷构造：可写文件库池（CLI 用；生产库守卫在 CLI 层）。
pub fn writable_pool(path: &std::path::Path) -> Result<Pool, WorkflowError> {
    Pool::open(PoolConfig {
        path: database::DbPath::File(path.to_path_buf()),
        readonly: false,
        max_connections: 1,
        busy_timeout: std::time::Duration::from_secs(5),
    })
    .map_err(WorkflowError::from)
}

/// 便捷构造：只读文件库池（import-legacy 源库用）。
pub fn readonly_pool(path: &std::path::Path) -> Result<Pool, WorkflowError> {
    Pool::open(PoolConfig {
        path: database::DbPath::File(path.to_path_buf()),
        readonly: true,
        max_connections: 1,
        busy_timeout: std::time::Duration::from_secs(5),
    })
    .map_err(WorkflowError::from)
}
