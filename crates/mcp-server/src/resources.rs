//! P9-4 rip:// 资源面（计划 §11:662-668 七 resource，外部 AI 可读）：
//!
//! - 静态目录四件（resources/list）：`rip://workspace/current`、
//!   `rip://knowledge/catalog`、`rip://plugins/catalog`、`rip://tools/catalog`；
//! - 模板三件（resources/templates/list，MCP 2024-11-05 同一方法面）：
//!   `rip://tasks/{task_id}`、`rip://runs/{run_id}`、`rip://evidence/{evidence_id}`；
//!   合计七 resource，resources/list 只列具体资源、模板段列模板（协议口径，
//!   不把模板伪装成具体资源）；
//! - `resources/read`（[`McpServer::read_resource`]）按 URI 形状分发：
//!   * 目录四件为读时投影（真实空态文档语义：数据面不可读时返回
//!     `available:false + reason` 的诚实空态载荷，不报错不造假数据，
//!     rp_server 读面同款口径）；
//!   * `rip://runs/<run_id>` 与 `rip.get_run_status` 同一投影（dispatch
//!     [`dispatch::RunChannel::run_status`] 唯一实现，不建第二套读面）；
//!   * `rip://evidence/<evidence_id>` 证据行 + storage_ref 指向的 Artifact
//!     快照文本（64 KiB 上限，超限截断并标注）；
//!   * `rip://tasks/<task_id>` research_task 行（主库读面）；
//!   * 实体不存在回 -32602（同 rip.get_tool_schema 未知 tool 口径）；运行库
//!     缺席 / 表缺失回 -32603（reason 词表同 catalog.rs）。
//!
//! 只读纪律：全部数据面以 SQLITE_OPEN_READ_ONLY 打开；本模块零 SQL 写；
//! 令牌等凭据永不入资源载荷（workspace/current 只含路径与版本面信息）。

#![forbid(unsafe_code)]

use std::path::Path;

use database::{rusqlite, Pool, PoolConfig};
use serde_json::{json, Value};

use crate::catalog::RegistryFilters;
use crate::McpServer;

/// 静态资源 URI（计划 §11:662-665）。
pub const WORKSPACE_URI: &str = "rip://workspace/current";
/// 静态资源 URI。
pub const KNOWLEDGE_CATALOG_URI: &str = "rip://knowledge/catalog";
/// 静态资源 URI。
pub const PLUGINS_CATALOG_URI: &str = "rip://plugins/catalog";
/// 静态资源 URI。
pub const TOOLS_CATALOG_URI: &str = "rip://tools/catalog";

/// 模板资源 URI（计划 §11:666-668）。
pub const TASK_TEMPLATE_URI: &str = "rip://tasks/{task_id}";
/// 模板资源 URI。
pub const RUN_TEMPLATE_URI: &str = "rip://runs/{run_id}";
/// 模板资源 URI。
pub const EVIDENCE_TEMPLATE_URI: &str = "rip://evidence/{evidence_id}";

/// 目录资源读取上限（lib.rs MAX_PAGE_LIMIT 同款，读时投影不做分页）。
const CATALOG_READ_LIMIT: usize = 200;

/// Artifact 快照文本内联上限（64 KiB，超出截断并标注）。
const EVIDENCE_INLINE_LIMIT: u64 = 64 * 1024;

/// resources/list 条目（MCP 2024-11-05 Resource 对象子集；线上字段名
/// mimeType 为协议形状，serde rename，Rust 侧保持 snake_case）。
#[derive(Debug, Clone, serde::Serialize)]
pub struct ResourceDef {
    /// 资源 URI。
    pub uri: &'static str,
    /// 资源名。
    pub name: &'static str,
    /// 一行说明。
    pub description: &'static str,
    /// MIME 类型（线上字段 mimeType）。
    #[serde(rename = "mimeType")]
    pub mime_type: &'static str,
}

/// resources/templates/list 条目（MCP 2024-11-05 ResourceTemplate 子集）。
#[derive(Debug, Clone, serde::Serialize)]
pub struct ResourceTemplateDef {
    /// URI 模板（RFC 6570 单占位符子集；线上字段 uriTemplate）。
    #[serde(rename = "uriTemplate")]
    pub uri_template: &'static str,
    /// 资源名。
    pub name: &'static str,
    /// 一行说明。
    pub description: &'static str,
    /// MIME 类型（线上字段 mimeType）。
    #[serde(rename = "mimeType")]
    pub mime_type: &'static str,
}

/// 静态目录资源四件（固定次序 = 计划 §11 行序）。
pub fn static_resources() -> Vec<ResourceDef> {
    vec![
        ResourceDef {
            uri: WORKSPACE_URI,
            name: "当前工作区",
            description: "平台 MCP 面当前工作区上下文（服务器信息、数据面路径、目录与执行面配置；不含凭据）",
            mime_type: "application/json",
        },
        ResourceDef {
            uri: KNOWLEDGE_CATALOG_URI,
            name: "知识目录",
            description: "knowledge 表目录投影（主库 ETL 注册面，检索背景用）",
            mime_type: "application/json",
        },
        ResourceDef {
            uri: PLUGINS_CATALOG_URI,
            name: "插件目录",
            description: "plugin 表目录投影（RP_PLUGIN_DB 读面，与 Plugin Center 同源）",
            mime_type: "application/json",
        },
        ResourceDef {
            uri: TOOLS_CATALOG_URI,
            name: "工具目录",
            description: "tool 表注册面投影（与 rip.list_tools 注册面 / Tool Center 同一读面）",
            mime_type: "application/json",
        },
    ]
}

/// 模板资源三件（固定次序 = 计划 §11 行序）。
pub fn resource_templates() -> Vec<ResourceTemplateDef> {
    vec![
        ResourceTemplateDef {
            uri_template: TASK_TEMPLATE_URI,
            name: "研究任务",
            description: "按 task_id 读 research_task 行（主库任务面）",
            mime_type: "application/json",
        },
        ResourceTemplateDef {
            uri_template: RUN_TEMPLATE_URI,
            name: "工具运行",
            description: "按 run_id 读运行面全链投影（run 行 + artifact + evidence 引用 + trace 锚，与 rip.get_run_status 同一投影）",
            mime_type: "application/json",
        },
        ResourceTemplateDef {
            uri_template: EVIDENCE_TEMPLATE_URI,
            name: "证据",
            description: "按 evidence_id 读证据行与 storage_ref 指向的 Artifact 快照文本",
            mime_type: "application/json",
        },
    ]
}

/// 资源读错误（JSON-RPC 映射在 lib.rs：Unknown/Invalid/NotFound = -32602，
/// 其余 -32603）。
#[derive(Debug)]
pub enum ResourceError {
    /// URI 形状不认识（七 resource 词表外）。
    UnknownUri(String),
    /// 模板资源缺 id 段（如 rip://runs/ 无 run_id）。
    InvalidUri(String),
    /// 实体不存在（按 id 查询未命中）。
    NotFound(String),
    /// 数据面不可读（真实空态说明，reason 词表同 catalog.rs）。
    Unavailable {
        /// 缺什么（main_db_missing / table_missing / run_face_not_configured 等）。
        reason: String,
        /// 给宿主的完整说明（真实空态，非空数据）。
        message: String,
    },
    /// 底层 sqlite 错误。
    Db(database::DbError),
}

impl std::fmt::Display for ResourceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ResourceError::UnknownUri(uri) => write!(f, "未知资源 URI：{uri}"),
            ResourceError::InvalidUri(msg) => write!(f, "资源 URI 不完整：{msg}"),
            ResourceError::NotFound(id) => write!(f, "资源实体不存在：{id}"),
            ResourceError::Unavailable { reason, message } => {
                write!(f, "unavailable({reason}): {message}")
            }
            ResourceError::Db(e) => write!(f, "db error: {e}"),
        }
    }
}

impl From<database::DbError> for ResourceError {
    fn from(e: database::DbError) -> Self {
        ResourceError::Db(e)
    }
}

/// 资源读结果（uri + MIME + JSON 载荷；lib.rs 包成 contents[0].text）。
pub struct ResourceDoc {
    /// 资源 URI（回填响应 contents[0].uri）。
    pub uri: String,
    /// MIME 类型。
    pub mime_type: &'static str,
    /// JSON 载荷（pretty 序列化后进 text）。
    pub payload: Value,
}

impl McpServer {
    /// resources/read 分发（计划 §11:662-668 七 resource 唯一入口）。
    pub fn read_resource(&self, uri: &str) -> Result<ResourceDoc, ResourceError> {
        match uri {
            WORKSPACE_URI => self.read_workspace(uri),
            KNOWLEDGE_CATALOG_URI => self.read_knowledge(uri),
            PLUGINS_CATALOG_URI => self.read_plugins(uri),
            TOOLS_CATALOG_URI => self.read_tools(uri),
            _ => {
                if let Some(id) = uri.strip_prefix("rip://tasks/") {
                    return self.read_task(uri, id);
                }
                if let Some(id) = uri.strip_prefix("rip://runs/") {
                    return self.read_run(uri, id);
                }
                if let Some(id) = uri.strip_prefix("rip://evidence/") {
                    return self.read_evidence(uri, id);
                }
                Err(ResourceError::UnknownUri(uri.to_string()))
            }
        }
    }

    /// rip://workspace/current：当前工作区上下文（无凭据；路径为宿主注入
    /// 配置，与 stderr ready 行同信息面）。
    fn read_workspace(&self, uri: &str) -> Result<ResourceDoc, ResourceError> {
        Ok(ResourceDoc {
            uri: uri.to_string(),
            mime_type: "application/json",
            payload: json!({
                "workspace_id": "ws-mcp",
                "server": { "name": crate::SERVER_NAME, "version": crate::SERVER_VERSION,
                            "protocol_version": crate::PROTOCOL_VERSION },
                "faces": {
                    "tools_catalog": { "uri": TOOLS_CATALOG_URI },
                    "knowledge_catalog": { "uri": KNOWLEDGE_CATALOG_URI },
                    "plugins_catalog": { "uri": PLUGINS_CATALOG_URI },
                    "runs": { "uri_template": RUN_TEMPLATE_URI },
                    "tasks": { "uri_template": TASK_TEMPLATE_URI },
                    "evidence": { "uri_template": EVIDENCE_TEMPLATE_URI }
                },
                "data_plane": {
                    "main_db": self.main_db.display().to_string(),
                    "main_db_present": self.main_db.is_file(),
                    "aux_db": self.aux_db.as_ref().map(|p| p.display().to_string()),
                    "run_db": self.run_channel.as_ref().map(|c| c.run_db.display().to_string()),
                    "run_db_present": self.run_channel.as_ref().map(|c| c.run_db.is_file()),
                    "plugin_db": self.plugin_db.as_ref().map(|p| p.display().to_string()),
                    "plugin_db_present": self.plugin_db.as_ref().map(|p| p.is_file()),
                    "execution": if self.run_channel.is_some() {
                        "rip.run_tool（python_worker / podman 经 tool-engine 既有派发链）"
                    } else {
                        "run face 未配置（RP_RUN_DB=none）"
                    }
                },
                "note": "令牌等凭据经 RP_MCP_TOKEN 环境变量注入，不出现在任何资源载荷"
            }),
        })
    }

    /// rip://knowledge/catalog：knowledge 表目录投影（主库读面，rp_server
    /// /api/knowledge 同源；数据面不可读返回诚实空态载荷）。
    fn read_knowledge(&self, uri: &str) -> Result<ResourceDoc, ResourceError> {
        let pool = self.main_ro_pool()?;
        let payload = pool.with_conn(|conn| -> rusqlite::Result<Value> {
            if !table_exists(conn, "knowledge") {
                return Ok(honest_unavailable(
                    "table_missing",
                    "主库缺 knowledge 表（迁移水位不足），知识目录暂不可读",
                ));
            }
            let total: i64 =
                conn.query_row("SELECT COUNT(*) FROM knowledge", [], |r| r.get(0))?;
            let items = rows_as_json(
                conn,
                "SELECT knowledge_id, record_kind, lifecycle, governance, title, \
                 content_kind, category, severity, language, updated_at \
                 FROM knowledge ORDER BY updated_at DESC, knowledge_id ASC LIMIT ?",
                CATALOG_READ_LIMIT,
            )?;
            Ok(json!({
                "available": true,
                "resource": uri,
                "source": "main",
                "total": total,
                "returned": items.len(),
                "limit": CATALOG_READ_LIMIT,
                "items": items
            }))
        })?;
        Ok(ResourceDoc { uri: uri.to_string(), mime_type: "application/json", payload })
    }

    /// rip://plugins/catalog：plugin 表目录投影（RP_PLUGIN_DB 读面，
    /// rp_server /api/plugins L1 目录同源；库缺席返回诚实空态载荷）。
    fn read_plugins(&self, uri: &str) -> Result<ResourceDoc, ResourceError> {
        let Some(path) = self.plugin_db.clone() else {
            return Ok(ResourceDoc {
                uri: uri.to_string(),
                mime_type: "application/json",
                payload: honest_unavailable(
                    "plugin_db_not_configured",
                    "未配置 RP_PLUGIN_DB，插件目录暂不可读（rp_server /api/plugins 同守卫口径）",
                ),
            });
        };
        if !path.is_file() {
            return Ok(ResourceDoc {
                uri: uri.to_string(),
                mime_type: "application/json",
                payload: honest_unavailable(
                    "plugin_db_missing",
                    &format!(
                        "RP_PLUGIN_DB 指向的文件不存在：{}，插件目录暂不可读",
                        path.display()
                    ),
                ),
            });
        }
        let pool = open_ro(&path)?;
        let db_display = path.display().to_string();
        let payload = pool.with_conn(|conn| -> rusqlite::Result<Value> {
            if !table_exists(conn, "plugin") {
                return Ok(honest_unavailable(
                    "table_missing",
                    "插件库缺 plugin 表（迁移水位不足），插件目录暂不可读",
                ));
            }
            let total: i64 = conn.query_row("SELECT COUNT(*) FROM plugin", [], |r| r.get(0))?;
            let items = rows_as_json(
                conn,
                "SELECT plugin_id, version, name, kind, state, trust_level, is_active, \
                 update_pending, governance, updated_at FROM plugin \
                 ORDER BY plugin_id ASC LIMIT ?",
                CATALOG_READ_LIMIT,
            )?;
            let caps = rows_as_json(
                conn,
                "SELECT plugin_id, plugin_version, capability_id, capability_type, summary \
                 FROM plugin_capability ORDER BY plugin_id, capability_id LIMIT ?",
                CATALOG_READ_LIMIT,
            )?;
            Ok(json!({
                "available": true,
                "resource": uri,
                "db": db_display,
                "total": total,
                "returned": items.len(),
                "limit": CATALOG_READ_LIMIT,
                "items": items,
                "capabilities": caps
            }))
        })?;
        Ok(ResourceDoc { uri: uri.to_string(), mime_type: "application/json", payload })
    }

    /// rip://tools/catalog：tool 表注册面投影（目录读面 = rip.list_tools
    /// 注册面同一实现 list_registry，无过滤全量一页，十二列同 TOOL_LIST_COLUMNS）。
    fn read_tools(&self, uri: &str) -> Result<ResourceDoc, ResourceError> {
        let rows = self
            .open_catalog()
            .and_then(|c| c.list_registry(&RegistryFilters::default()))
            .map_err(|e| match e {
                crate::catalog::CatalogError::Unavailable { reason, message } => {
                    ResourceError::Unavailable { reason, message }
                }
                crate::catalog::CatalogError::Db(db) => ResourceError::Db(db),
                // list_registry 无过滤调用不会产生 BadFilter / NotFound。
                other => ResourceError::Unavailable {
                    reason: "catalog_read_failed".to_string(),
                    message: other.to_string(),
                },
            })?;
        let total = rows.len();
        Ok(ResourceDoc {
            uri: uri.to_string(),
            mime_type: "application/json",
            payload: json!({
                "available": true,
                "resource": uri,
                "total": total,
                "returned": total.min(CATALOG_READ_LIMIT),
                "limit": CATALOG_READ_LIMIT,
                "items": &rows[..total.min(CATALOG_READ_LIMIT)],
                "mcp_prefix": "rip."
            }),
        })
    }

    /// rip://tasks/<task_id>：research_task 行（主库任务面）。
    fn read_task(&self, uri: &str, task_id: &str) -> Result<ResourceDoc, ResourceError> {
        if task_id.is_empty() || task_id.contains('/') {
            return Err(ResourceError::InvalidUri(format!(
                "rip://tasks/<task_id> 需要 task_id（收到 {task_id:?}）"
            )));
        }
        let pool = self.main_ro_pool()?;
        let present = pool.with_conn(|conn| -> rusqlite::Result<bool> {
            Ok(table_exists(conn, "research_task"))
        })?;
        if !present {
            return Err(ResourceError::Unavailable {
                reason: "table_missing".to_string(),
                message: "主库缺 research_task 表（迁移水位不足），任务面暂不可读".to_string(),
            });
        }
        let row = pool.with_conn(|conn| -> rusqlite::Result<Option<Value>> {
            let mut stmt = conn.prepare(
                "SELECT task_id, title, objective, target, task_type, state, \
                 status_history, source_system, created_at, updated_at \
                 FROM research_task WHERE task_id = ?1",
            )?;
            let rows = stmt
                .query_map([task_id], row_to_json)?
                .filter_map(|x| x.ok())
                .collect::<Vec<_>>();
            Ok(rows.into_iter().next())
        })?;
        let Some(row) = row else {
            return Err(ResourceError::NotFound(format!("task {task_id:?}")));
        };
        Ok(ResourceDoc { uri: uri.to_string(), mime_type: "application/json", payload: row })
    }

    /// rip://runs/<run_id>：运行面全链投影（dispatch RunChannel::run_status
    /// 唯一实现：run 行 + artifact + evidence 引用 + trace 锚；与
    /// rip.get_run_status 同一形状，通道一致性对账同源）。
    fn read_run(&self, uri: &str, run_id: &str) -> Result<ResourceDoc, ResourceError> {
        if run_id.is_empty() || run_id.contains('/') {
            return Err(ResourceError::InvalidUri(format!(
                "rip://runs/<run_id> 需要 run_id（收到 {run_id:?}）"
            )));
        }
        let Some(channel) = self.run_channel.as_ref() else {
            return Err(ResourceError::Unavailable {
                reason: "run_face_not_configured".to_string(),
                message: "run face 未配置（RP_RUN_DB 未装配；bin 层缺省 rip-data/tool-runs.db)"
                    .to_string(),
            });
        };
        let mut payload = channel.run_status(run_id).map_err(|p| {
            // run_status 的 Err 载荷带 category（VALIDATION=不存在 → NotFound）
            let category = p
                .get("error")
                .and_then(|e| e.get("category"))
                .and_then(Value::as_str)
                .unwrap_or("");
            if category == "VALIDATION" {
                ResourceError::NotFound(format!("run {run_id:?}"))
            } else {
                ResourceError::Unavailable {
                    reason: "run_face_unavailable".to_string(),
                    message: p.to_string(),
                }
            }
        })?;
        payload["uri"] = json!(uri);
        Ok(ResourceDoc { uri: uri.to_string(), mime_type: "application/json", payload })
    }

    /// rip://evidence/<evidence_id>：证据行（运行库优先，主库回退）+
    /// storage_ref 指向的 Artifact 快照文本（64 KiB 截断）。
    fn read_evidence(&self, uri: &str, evidence_id: &str) -> Result<ResourceDoc, ResourceError> {
        if evidence_id.is_empty() || evidence_id.contains('/') {
            return Err(ResourceError::InvalidUri(format!(
                "rip://evidence/<evidence_id> 需要 evidence_id（收到 {evidence_id:?}）"
            )));
        }
        let mut row: Option<Value> = None;
        if let Some(channel) = self.run_channel.as_ref() {
            if channel.run_db.is_file() {
                let pool = open_ro(&channel.run_db)?;
                row = pool.with_conn(|conn| evidence_row(conn, evidence_id))?;
            }
        }
        if row.is_none() && self.main_db.is_file() {
            let pool = open_ro(&self.main_db)?;
            row = pool.with_conn(|conn| evidence_row(conn, evidence_id))?;
        }
        let Some(mut row) = row else {
            return Err(ResourceError::NotFound(format!("evidence {evidence_id:?}")));
        };
        // storage_ref 指向的 Artifact 快照内联（文件在位且 ≤ 上限时给全文）。
        if let Some(storage_ref) = row["storage_ref"].as_str() {
            let path = Path::new(storage_ref);
            if path.is_file() {
                match std::fs::metadata(path) {
                    Ok(meta) if meta.len() <= EVIDENCE_INLINE_LIMIT => {
                        if let Ok(text) = std::fs::read_to_string(path) {
                            row["artifact_text"] = json!(text);
                            row["artifact_text_truncated"] = json!(false);
                        }
                    }
                    Ok(meta) => {
                        row["artifact_text_truncated"] = json!(true);
                        row["artifact_text_bytes"] = json!(meta.len());
                        row["note"] =
                            json!("storage_ref 快照超过内联上限（64 KiB），按 storage_ref 路径取件");
                    }
                    Err(_) => {}
                }
            }
        }
        row["uri"] = json!(uri);
        Ok(ResourceDoc { uri: uri.to_string(), mime_type: "application/json", payload: row })
    }

    /// 主库只读连接（资源读面无状态模型，P9-1 同款：开查即释）。
    fn main_ro_pool(&self) -> Result<Pool, ResourceError> {
        if !self.main_db.is_file() {
            return Err(ResourceError::Unavailable {
                reason: "main_db_missing".to_string(),
                message: format!("主库文件不存在：{}", self.main_db.display()),
            });
        }
        open_ro(&self.main_db)
    }
}

/// 只读打开库文件。
fn open_ro(path: &Path) -> Result<Pool, ResourceError> {
    Pool::open(PoolConfig::readonly_file(path)).map_err(ResourceError::from)
}

/// 表存在性（sqlite_master 直查）。
fn table_exists(conn: &rusqlite::Connection, table: &str) -> bool {
    conn.query_row(
        "SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1",
        [table],
        |_| Ok(()),
    )
    .is_ok()
}

/// 行转 JSON（列名按游标元数据；避免逐资源手写投影漂移）。
fn row_to_json(row: &rusqlite::Row<'_>) -> rusqlite::Result<Value> {
    let mut map = serde_json::Map::new();
    for (idx, col) in row.as_ref().column_names().iter().enumerate() {
        let value = match row.get_ref(idx)? {
            rusqlite::types::ValueRef::Null => Value::Null,
            rusqlite::types::ValueRef::Integer(i) => json!(i),
            rusqlite::types::ValueRef::Real(x) => json!(x),
            rusqlite::types::ValueRef::Text(t) => {
                json!(String::from_utf8_lossy(t).to_string())
            }
            rusqlite::types::ValueRef::Blob(_) => Value::Null,
        };
        map.insert((*col).to_string(), value);
    }
    Ok(Value::Object(map))
}

/// 多行查询（LIMIT 参数绑定；列名投影同 row_to_json）。
fn rows_as_json(
    conn: &rusqlite::Connection,
    sql: &str,
    limit: usize,
) -> rusqlite::Result<Vec<Value>> {
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt
        .query_map([limit as i64], row_to_json)?
        .filter_map(|x| x.ok())
        .collect();
    Ok(rows)
}

/// 证据行（trace_id 列按库水位条件读：缺列库零贡献不炸；evidence 表缺席
/// 返回 None，不作为数据面错误）。
fn evidence_row(conn: &rusqlite::Connection, evidence_id: &str) -> rusqlite::Result<Option<Value>> {
    if !table_exists(conn, "evidence") {
        return Ok(None);
    }
    let has_trace = conn
        .prepare("SELECT trace_id FROM evidence LIMIT 0")
        .is_ok();
    let sql = if has_trace {
        "SELECT evidence_id, evidence_type, storage_ref, content_hash, captured_at, \
         captured_by, task_ref, source_pk, created_at, updated_at, trace_id \
         FROM evidence WHERE evidence_id = ?1"
    } else {
        "SELECT evidence_id, evidence_type, storage_ref, content_hash, captured_at, \
         captured_by, task_ref, source_pk, created_at, updated_at, NULL AS trace_id \
         FROM evidence WHERE evidence_id = ?1"
    };
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt
        .query_map([evidence_id], row_to_json)?
        .filter_map(|x| x.ok())
        .collect::<Vec<_>>();
    Ok(rows.into_iter().next())
}

/// 诚实空态载荷（rp_server 读面 available:false 口径）。
fn honest_unavailable(reason: &str, message: &str) -> Value {
    json!({ "available": false, "reason": reason, "message": message })
}
