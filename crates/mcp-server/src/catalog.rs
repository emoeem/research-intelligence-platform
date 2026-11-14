//! 工具目录读面（P9-1）：与 Tool Center 同源的唯一 tool 表读面。
//!
//! 同源口径（对齐 apps/server/rp_server.py 的 handle_tools，以 2026-11-14
//! 读到的实现为准）：
//! - 数据面守卫同 `phase14_source`（rp_server.py:231-241）：主库已建 tool 表
//!   一律主库优先（主库是 ETL 全量注册面）；主库缺表回退 aux；都无则返回
//!   真实空态说明（Unavailable，按缺什么说什么，不造假数据）。
//! - 列投影同 `TOOL_LIST_COLUMNS`（rp_server.py:507）：tool_id, name, kind,
//!   lifecycle, governance, install_state, health_state, permission_level,
//!   timeout_s, source_system, created_at, updated_at。
//! - 过滤同款：keyword 按 name/tool_id LIKE 字面匹配（like_escape 转义
//!   `% _ \`），lifecycle / governance 枚举白名单校验（rp_server.py:496-497
//!   词表，check_enum 口径）。
//! - 排序同款：`ORDER BY updated_at DESC, name COLLATE NOCASE ASC`。
//! - aux 增量合并同 handle_tools 注释口径「aux 只补主库没有的行，不重复
//!   主库已有条目」（rp_server.py:669-670）：aux 行 tool_id 全局不在主库
//!   id 集合内才追加，追加段保持 aux 库同款排序。与 rp_server.py:737-747
//!   实现有两处已登记偏差（deviation 登记在 docs/phase9/PHASE9-DESIGN.md）：
//!   其一，那边与「本页已返回的 id 集合」去重（limit 截断页下存在跨页重复
//!   的理论缝隙），本读面与「主库全量 id 集合」去重（全局口径，分页游标下
//!   无跨页重复）；无过滤且页覆盖全量时两口径结果一致。其二，那边 aux 段
//!   不过滤（keyword/lifecycle/governance 只作用于主库段），本读面对 aux 段
//!   同样生效（主库回退态下无差异）。
//! - 按需 Schema 同 handle_tools ?tool_id= 详情顺序：主库精确查找，未命中
//!   且 aux 在场时回退 aux；input/output Schema 列 NULL 或坏 JSON 记 null
//!   （parse_json_or_none 口径）。
//!
//! 只读纪律：两个库都以 SQLITE_OPEN_READ_ONLY 打开（database::Pool
//! readonly_file），整个 crate 无任何 SQL 写，生产 platform*.db 零写红线。

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use database::{rusqlite, Pool, PoolConfig};
use serde_json::Value;

/// lifecycle 五态词表（rp_server.py:496）。
pub const TOOL_LIFECYCLE: [&str; 5] =
    ["REGISTERED", "VALIDATED", "ENABLED", "DISABLED", "DEPRECATED"];

/// governance 四态词表（rp_server.py:497）。
pub const TOOL_GOVERNANCE: [&str; 4] = ["ACTIVE", "QUARANTINED", "REJECTED", "ARCHIVED"];

/// 列投影，逐列对应 rp_server.py TOOL_LIST_COLUMNS（507 行）。
const TOOL_LIST_COLUMNS: &str = "tool_id, name, kind, lifecycle, governance, install_state, \
health_state, permission_level, timeout_s, source_system, created_at, updated_at";

/// 目录读面错误：Unavailable 是真实空态说明（数据面不可读），不是空数据。
#[derive(Debug)]
pub enum CatalogError {
    /// 数据面不可读（按缺什么说什么，reason 词表同 rp_server.py:242-259）。
    Unavailable {
        /// 缺什么（aux_db_not_configured / aux_db_missing / table_missing 等）。
        reason: String,
        /// 给宿主的完整说明（真实空态，非空数据）。
        message: String,
    },
    /// 工具不存在（按需 Schema 查询未命中）。
    NotFound(String),
    /// 过滤参数非法（词表外 lifecycle/governance 等）。
    BadFilter(String),
    /// 底层 sqlite 错误。
    Db(database::DbError),
}

impl std::fmt::Display for CatalogError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CatalogError::Unavailable { reason, message } => {
                write!(f, "unavailable({reason}): {message}")
            }
            CatalogError::NotFound(tool_id) => write!(f, "tool 不存在：{tool_id}"),
            CatalogError::BadFilter(msg) => write!(f, "{msg}"),
            CatalogError::Db(e) => write!(f, "db error: {e}"),
        }
    }
}

impl From<database::DbError> for CatalogError {
    fn from(e: database::DbError) -> Self {
        CatalogError::Db(e)
    }
}

/// 过滤条件（keyword / lifecycle / governance，语义同 handle_tools）。
#[derive(Debug, Default, Clone)]
pub struct RegistryFilters {
    /// 按 name / tool_id LIKE 字面匹配（like_escape 转义）。
    pub keyword: Option<String>,
    /// lifecycle 五态之一（词表外报 BadFilter）。
    pub lifecycle: Option<String>,
    /// governance 四态之一（词表外报 BadFilter）。
    pub governance: Option<String>,
}

/// tool 行投影（TOOL_LIST_COLUMNS 十二列）。
#[derive(Debug, Clone, serde::Serialize)]
pub struct RegistryRow {
    /// 工具 id（主键，Tool Center 同名同值）。
    pub tool_id: String,
    /// 名称。
    pub name: String,
    /// kind（cli / datastore / mcp / tool 等）。
    pub kind: Option<String>,
    /// lifecycle 五态。
    pub lifecycle: Option<String>,
    /// governance 四态。
    pub governance: Option<String>,
    /// 安装状态。
    pub install_state: Option<String>,
    /// 健康状态。
    pub health_state: Option<String>,
    /// T0/T1/T2。
    pub permission_level: Option<String>,
    /// 超时秒数。
    pub timeout_s: Option<i64>,
    /// 来源登记面（rtis-registry / tool-registry 等）。
    pub source_system: Option<String>,
    /// 创建时刻。
    pub created_at: Option<String>,
    /// 更新时刻。
    pub updated_at: Option<String>,
}

/// 注册面行 + input_schema 列直读（tools/list 标准 Tool 对象的必需
/// inputSchema 来源；rmcp 迁移批次新增，摘要十二列投影不变，schema 列只在
/// 本读面取，rip.list_tools 信封仍走 TOOL_LIST_COLUMNS 十二列原口径）。
#[derive(Debug, Clone)]
pub struct RegistryRowWithSchema {
    /// 十二列摘要投影（与 [`RegistryRow`] 同源同序）。
    pub row: RegistryRow,
    /// tool 表 input_schema 列（NULL / 坏 JSON / 非 object 形态一律 None，
    /// 上层回退宽松 object Schema；parse_json_or_none 口径加 object 形态守卫）。
    pub input_schema: Option<Value>,
}

/// 按需 Schema 查询结果（tool 表 input_schema / output_schema 列直读）。
#[derive(Debug, Clone, serde::Serialize)]
pub struct ToolSchema {
    /// 工具 id。
    pub tool_id: String,
    /// 命中的读面（main / aux，同 handle_tools detail 的 source 口径）。
    pub source: String,
    /// input Schema JSON 文档（列 NULL 或非 JSON 时为 null，parse_json_or_none 口径）。
    pub input_schema: Value,
    /// output Schema JSON 文档（同上）。
    pub output_schema: Value,
}

/// 工具目录：主库 + 可选 aux 只读库。
#[derive(Debug, Clone)]
pub struct Catalog {
    main: Pool,
    aux: Option<Pool>,
    /// aux 配置了但文件缺失 / 打不开时的说明（aux_db_missing 等，真实空态）。
    aux_unavailable_reason: Option<String>,
    /// 主库文件路径（Unavailable 说明用）。
    main_path: PathBuf,
    /// aux 文件路径。
    aux_path: Option<PathBuf>,
}

impl Catalog {
    /// 打开只读目录。main 库文件必须存在；aux 缺省 None，配置了但缺失时
    /// 记录原因（沿用 rp_server.py:242-259 phase14_unavailable 的 reason 词表），
    /// 不在打开期失败（handle_tools 对 aux 缺失按守卫降级，不炸端点）。
    pub fn open(main: &Path, aux: Option<&Path>) -> Result<Self, CatalogError> {
        if !main.is_file() {
            return Err(CatalogError::Unavailable {
                reason: "main_db_missing".to_string(),
                message: format!("主库文件不存在：{}", main.display()),
            });
        }
        let (aux_pool, aux_reason) = match aux {
            None => (None, None),
            Some(p) => {
                if !p.is_file() {
                    (
                        None,
                        Some(format!(
                            "RP_AUX_DB 指向的文件不存在：{}，aux 段不可读",
                            p.display()
                        )),
                    )
                } else {
                    match Pool::open(PoolConfig::readonly_file(p)) {
                        Ok(pool) => (Some(pool), None),
                        Err(e) => (None, Some(format!("aux 库打开失败（{}）：{e}", p.display()))),
                    }
                }
            }
        };
        Ok(Self {
            main: Pool::open(PoolConfig::readonly_file(main))?,
            aux: aux_pool,
            aux_unavailable_reason: aux_reason,
            main_path: main.to_path_buf(),
            aux_path: aux.map(Path::to_path_buf),
        })
    }

    /// 列出合并后的注册面（主库优先 + aux 增量补行，排序同 handle_tools）。
    /// 每次调用经只读连接直查（stdio 单请求进程，无缓存副本，与 Tool Center
    /// 同源实时读面）。
    pub fn list_registry(
        &self,
        filters: &RegistryFilters,
    ) -> Result<Vec<RegistryRow>, CatalogError> {
        Ok(self
            .list_registry_with_schema(filters)?
            .into_iter()
            .map(|entry| entry.row)
            .collect())
    }

    /// 列出合并后的注册面并直读 input_schema 列（[`RegistryRowWithSchema`]；
    /// tools/list 标准 Tool 对象必需 Schema 用，合并与排序语义与
    /// [`Catalog::list_registry`] 完全同一实现）。
    pub fn list_registry_with_schema(
        &self,
        filters: &RegistryFilters,
    ) -> Result<Vec<RegistryRowWithSchema>, CatalogError> {
        validate_filters(filters)?;
        let like = filters.keyword.as_deref().map(like_escape);
        let (where_sql, where_args) = build_where(&like, filters);

        // 数据面守卫（phase14_source 口径）：主库优先，aux 回退，都无则真实空态。
        let main_has =
            self.main.with_conn(|c| Ok::<bool, database::rusqlite::Error>(table_exists(c, "tool")))?;
        let aux_has = self
            .aux
            .as_ref()
            .map(|p| p.with_conn(|c| Ok::<bool, database::rusqlite::Error>(table_exists(c, "tool"))))
            .transpose()?
            .unwrap_or(false);
        if !main_has && !aux_has {
            return Err(self.unavailable_error());
        }

        let mut rows: Vec<RegistryRowWithSchema> = Vec::new();
        if main_has {
            // 主库段（过滤 + 排序）与主库全量 id 集合一次取出。
            let (main_rows, main_ids) = self.main.with_conn(|c| {
                let rows = query_rows_with_schema(c, &where_sql, &where_args)?;
                let ids: HashSet<String> = c
                    .prepare("SELECT tool_id FROM tool")?
                    .query_map([], |r| r.get::<_, String>(0))?
                    .collect::<Result<_, rusqlite::Error>>()?;
                Ok::<_, rusqlite::Error>((rows, ids))
            })?;
            rows.extend(main_rows);
            // aux 增量合并：只补主库没有的行（全局 id 集合口径，见模块注释）。
            if aux_has {
                if let Some(aux_pool) = self.aux.as_ref() {
                    let aux_rows = aux_pool
                        .with_conn(|c| query_rows_with_schema(c, &where_sql, &where_args))?;
                    rows.extend(
                        aux_rows
                            .into_iter()
                            .filter(|entry| !main_ids.contains(&entry.row.tool_id)),
                    );
                }
            }
        } else if let Some(aux_pool) = self.aux.as_ref() {
            // 主库缺表回退 aux（handle_tools 数据面回退口径）。
            rows.extend(aux_pool.with_conn(|c| query_rows_with_schema(c, &where_sql, &where_args))?);
        }
        Ok(rows)
    }

    /// 按需 Schema：主库精确查找，未命中且 aux 在场时回退 aux
    /// （handle_tools ?tool_id= 详情同款顺序）。
    pub fn tool_schema(&self, tool_id: &str) -> Result<ToolSchema, CatalogError> {
        let main_has =
            self.main.with_conn(|c| Ok::<bool, database::rusqlite::Error>(table_exists(c, "tool")))?;
        if main_has {
            if let Some(row) = self.main.with_conn(|c| query_schema_row(c, "main", tool_id))? {
                return Ok(row);
            }
        }
        let aux_has = self
            .aux
            .as_ref()
            .map(|p| p.with_conn(|c| Ok::<bool, database::rusqlite::Error>(table_exists(c, "tool"))))
            .transpose()?
            .unwrap_or(false);
        if aux_has {
            if let Some(aux_pool) = self.aux.as_ref() {
                if let Some(row) =
                    aux_pool.with_conn(|c| query_schema_row(c, "aux", tool_id))?
                {
                    return Ok(row);
                }
            }
        }
        Err(CatalogError::NotFound(tool_id.to_string()))
    }

    /// 数据面不可读时的真实空态说明（Unavailable，reason 词表同
    /// rp_server.py:242-259）。仅在主库与 aux 均无 tool 表时到达。
    fn unavailable_error(&self) -> CatalogError {
        if self.aux.is_none() {
            return CatalogError::Unavailable {
                reason: "aux_db_not_configured".to_string(),
                message: format!(
                    "未配置 aux 库，且主库（{}）无 tool 表：工具目录数据面暂不可读（真实空态说明，非空数据）",
                    self.main_path.display()
                ),
            };
        }
        if self.aux_unavailable_reason.is_some() {
            return CatalogError::Unavailable {
                reason: "aux_db_missing".to_string(),
                message: self
                    .aux_unavailable_reason
                    .clone()
                    .unwrap_or_else(|| "aux 库不可读".to_string()),
            };
        }
        CatalogError::Unavailable {
            reason: "table_missing".to_string(),
            message: match self.aux_path.as_deref() {
                Some(p) => format!(
                    "主库（{}）与 aux 库（{}）均无 tool 表：对应迁移未应用，工具目录数据面暂不可读",
                    self.main_path.display(),
                    p.display()
                ),
                None => format!(
                    "主库（{}）无 tool 表：对应迁移未应用，工具目录数据面暂不可读",
                    self.main_path.display()
                ),
            },
        }
    }
}

/// sqlite_master 在场守卫（rp_server.py table_exists 同款）。
fn table_exists(conn: &rusqlite::Connection, table: &str) -> bool {
    conn.query_row(
        "SELECT 1 FROM sqlite_master WHERE type='table' AND name = ?1",
        [table],
        |_| Ok(()),
    )
    .is_ok()
}

/// like_escape 转义（rp_server.py:181-183 同款）：关键词按字面匹配。
fn like_escape(keyword: &str) -> String {
    keyword
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

/// 词表校验（rp_server.py check_enum 口径）。
fn validate_filters(filters: &RegistryFilters) -> Result<(), CatalogError> {
    if let Some(l) = filters.lifecycle.as_deref() {
        if !TOOL_LIFECYCLE.contains(&l) {
            return Err(CatalogError::BadFilter(format!(
                "lifecycle 仅支持 {}，收到：{l}",
                TOOL_LIFECYCLE.join("/")
            )));
        }
    }
    if let Some(g) = filters.governance.as_deref() {
        if !TOOL_GOVERNANCE.contains(&g) {
            return Err(CatalogError::BadFilter(format!(
                "governance 仅支持 {}，收到：{g}",
                TOOL_GOVERNANCE.join("/")
            )));
        }
    }
    Ok(())
}

/// WHERE 子句装配（keyword / lifecycle / governance，语义同 handle_tools）。
fn build_where(like: &Option<String>, filters: &RegistryFilters) -> (String, Vec<String>) {
    let mut clauses: Vec<String> = Vec::new();
    let mut args: Vec<String> = Vec::new();
    if let Some(needle) = like {
        clauses.push("(name LIKE ? ESCAPE '\\' OR tool_id LIKE ? ESCAPE '\\')".to_string());
        // rp_server.py:704 同款：%needle% 包裹（转义后的字面关键词）。
        let wildcard = format!("%{needle}%");
        args.push(wildcard.clone());
        args.push(wildcard);
    }
    if let Some(l) = filters.lifecycle.as_deref() {
        clauses.push("lifecycle = ?".to_string());
        args.push(l.to_string());
    }
    if let Some(g) = filters.governance.as_deref() {
        clauses.push("governance = ?".to_string());
        args.push(g.to_string());
    }
    let sql = if clauses.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", clauses.join(" AND "))
    };
    (sql, args)
}

/// 读投影列表（排序同 handle_tools：updated_at DESC, name COLLATE NOCASE ASC）。
/// 十二列摘要 + input_schema 列（第 13 列直读，[`RegistryRowWithSchema`]）。
fn query_rows_with_schema(
    conn: &rusqlite::Connection,
    where_sql: &str,
    where_args: &[String],
) -> Result<Vec<RegistryRowWithSchema>, rusqlite::Error> {
    let sql = format!(
        "SELECT {TOOL_LIST_COLUMNS}, input_schema FROM tool {where_sql} \
         ORDER BY updated_at DESC, name COLLATE NOCASE ASC"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt
        .query_map(rusqlite::params_from_iter(where_args.iter()), map_row_with_schema)?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// 行映射（rusqlite 列序与 TOOL_LIST_COLUMNS + input_schema 第 13 列一致）。
fn map_row_with_schema(
    r: &rusqlite::Row<'_>,
) -> Result<RegistryRowWithSchema, rusqlite::Error> {
    let row = RegistryRow {
        tool_id: r.get(0)?,
        name: r.get(1)?,
        kind: r.get(2)?,
        lifecycle: r.get(3)?,
        governance: r.get(4)?,
        install_state: r.get(5)?,
        health_state: r.get(6)?,
        permission_level: r.get(7)?,
        timeout_s: r.get(8)?,
        source_system: r.get(9)?,
        created_at: r.get(10)?,
        updated_at: r.get(11)?,
    };
    let input_schema = match r.get::<_, Option<String>>(12)? {
        None => None,
        Some(text) => match serde_json::from_str::<Value>(&text) {
            Ok(v) if v.is_object() => Some(v),
            // 坏 JSON 或非 object 形态一律回退宽松 Schema（parse_json_or_none
            // 口径 + object 形态守卫，不把非法 Schema 发上线）。
            _ => None,
        },
    };
    Ok(RegistryRowWithSchema { row, input_schema })
}

/// 读单行 schema（input_schema / output_schema 列直读，NULL 或坏 JSON 记 null，
/// parse_json_or_none 口径）。`source` 为命中的读面标签（main / aux）。
fn query_schema_row(
    conn: &rusqlite::Connection,
    source: &str,
    tool_id: &str,
) -> Result<Option<ToolSchema>, rusqlite::Error> {
    let row = conn
        .query_row(
            "SELECT input_schema, output_schema FROM tool WHERE tool_id = ?1",
            [tool_id],
            |r| {
                Ok((
                    r.get::<_, Option<String>>(0)?,
                    r.get::<_, Option<String>>(1)?,
                ))
            },
        )
        .ok();
    match row {
        None => Ok(None),
        Some((input, output)) => Ok(Some(ToolSchema {
            tool_id: tool_id.to_string(),
            source: source.to_string(),
            input_schema: parse_json_or_none(input.as_deref()),
            output_schema: parse_json_or_none(output.as_deref()),
        })),
    }
}

/// parse_json_or_none（rp_server.py:260 同款）：NULL 或解析失败一律 null。
fn parse_json_or_none(text: Option<&str>) -> Value {
    match text.and_then(|s| serde_json::from_str(s).ok()) {
        Some(v) => v,
        None => Value::Null,
    }
}
