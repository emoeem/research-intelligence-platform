//! Phase 9 平台 MCP server crate（bin rp-mcp，rmcp 迁移整改批次重构）。
//!
//! 职责（ROADMAP §10.3 P9-1 + P9-2 + P9-4；审计 §5 十项整改，选型记录见
//! docs/phase9/RMCP-MIGRATION.md）：
//! - [`catalog`]：工具目录读面，与 Tool Center 同源（同一 tool 表读面，
//!   aux 增量合并同 rp_server.py handle_tools 口径，详见模块文档）；
//! - [`dispatch`]：P9-2 rip.run_tool 端到端执行通道（经 tool-engine Engine
//!   受理与派发，结构化结果 + Artifact + Evidence 登记）与
//!   rip.get_run_status / rip.cancel_run 运行查询与协作式取消面；run_tool
//!   受理走 submit_correlated 自锚定（trace_id = run_id，015/018
//!   「run 即链锚」同款惯例，REST /api/traces 可反查 MCP 通道运行）；
//! - [`resources`]：P9-4 rip:// 七 resource（计划 §11:662-668，静态四件 +
//!   模板三件），rip://runs/<run_id> 与 rip.get_run_status 同一投影；
//! - [`prompts`]：P9-4 prompt 面三条（工具执行剧本 / 审计链复核 / 知识简报）；
//! - [`service`]：rmcp（MCP 官方 Rust SDK）ServerHandler 业务接入层：
//!   JSON-RPC 2.0 解析、initialize 生命周期、协议版本协商、协议错误信封、
//!   通知静默与 stdio I/O 全部交给 rmcp 承载（审计 P1 整改：手写协议层
//!   退役），本 crate 只提供目录 / 执行 / 资源 / prompt 四个业务面：
//!   * `initialize`：协议版本协商由 rmcp 承载（支持客户端版本则按规范
//!     协商回显，不支持则回落服务端支持版本），能力面 capabilities.tools
//!     （listChanged=false）+ resources（subscribe=false）+ prompts，一行
//!     导航 instructions，不携带任何工具全文说明（计划 §11:658）；
//!   * `tools/list`：分页目录（能力面 rip.* 六条固定前段 + 注册面 tool 表
//!     逐行投影 `rip.<tool_id>`）；P0 整改：每个 Tool 对象含标准必需
//!     `name` 与 `inputSchema`（注册面 input_schema 列直读，缺省宽松
//!     object Schema），分页只控量不省必需 Schema，token 纪律改为注册面
//!     条目不携带 description（完整元数据经 rip.get_tool_schema 按需获取）；
//!   * `tools/call`：接通 rip.list_tools / rip.get_tool_schema（发现面）与
//!     rip.run_tool / rip.get_run_status / rip.cancel_run（执行与运行面，
//!     经 [`dispatch::RunChannel`]，spawn_blocking 隔离 sqlite 与工具执行）；
//!     rip.search_knowledge 仍为目录条目（调用返回结构化 isError，不伪造
//!     成功）；注册面直呼 `rip.<tool_id>` 仍指向 rip.run_tool 通道提示；
//!   * `resources/list` / `resources/templates/list` / `resources/read` 与
//!     `prompts/list` / `prompts/get`：见上；词表外资源 / prompt 回
//!     -32602，数据面不可读回 -32603；
//!   * 握手鉴权（审计 §4.3 安全边界重定义）：`RP_MCP_TOKEN` 为宿主进程
//!     环境变量注入即授权边界（bin 层缺失 / 空值 fail-closed 拒启 exit 2），
//!     不实现也不发明 MCP 标准外的客户端协议认证字段（`_meta.rp.auth`
//!     自定义出示校验随手写协议层一并退役）。
//! - MCP 工具面不出现 Tool Center 没有的能力：注册面严格 1:1 投影 tool 表
//!   （不发明条目），能力面只收录计划 §11:640-656 中由既有平台能力面
//!   （tool 表 / tool_run 表 / knowledge 表）承载的子集，agents /
//!   workflows 等其余 rip.* 条目按批次归属后续，不在本批出现。
//!
//! 与 rtis_mcp.mjs 的边界（ROADMAP §10.2 / 盘点 461 行）：本 crate 是平台
//! MCP 面（rip.* 前缀），kit 的 rtis_mcp.mjs（redteam_* 前缀，9 工具）是
//! project_endpoint 接入面，两者命名空间隔离，互不引用。
//!
//! 纪律：stdout 只允许协议消息（rmcp transport 承担）；日志一律走 stderr。

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod catalog;
pub mod dispatch;
pub mod prompts;
pub mod resources;
pub mod service;

use std::sync::Arc;

use rmcp::model::Tool;
use serde_json::{Value, json};

use crate::catalog::{Catalog, CatalogError, RegistryFilters};

/// 平台基线协议版本（2024-11-05 子集口径；协商面见 [`service`]，
/// 支持列表 2024-11-05 至 2025-11-25 由 rmcp 承载协商）。
pub const PROTOCOL_VERSION: &str = "2024-11-05";
/// serverInfo.name。
pub const SERVER_NAME: &str = "rp-mcp";
/// serverInfo.version（workspace 版本）。
pub const SERVER_VERSION: &str = env!("CARGO_PKG_VERSION");

/// tools/list 页大小（服务端固定；MCP 标准分页只含 cursor，客户端不可指定
/// limit，控量由服务端页大小承担，不用省略必需 Schema 控 Token）。
pub const DEFAULT_PAGE_LIMIT: usize = 20;
/// 目录读面上限（rip.list_tools / resources 目录投影共用；
/// rp_server.py MAX_LIMIT=200 同款上限）。
pub const MAX_PAGE_LIMIT: usize = 200;

/// JSON-RPC 错误码：方法不存在（rmcp 兜底面，未知方法由 SDK 派发
/// method_not_found）。
pub const CODE_METHOD_NOT_FOUND: i64 = -32601;
/// JSON-RPC 错误码：参数非法（含游标 / 工具不存在）。
pub const CODE_INVALID_PARAMS: i64 = -32602;
/// JSON-RPC 错误码：内部错误（目录数据面不可读等）。
pub const CODE_INTERNAL: i64 = -32603;

/// 能力面工具（计划 §11:640-656 子集，固定次序 = 目录前段次序）。
/// rip.run_tool / rip.get_run_status / rip.cancel_run 接通执行与运行面
/// （[`dispatch::RunChannel`]）；`wired` 为 false 的条目（当前仅
/// rip.search_knowledge）在 tools/call 返回结构化 isError，不伪造执行结果。
pub const CAPABILITY_TOOLS: [(&str, bool); 6] = [
    ("rip.list_tools", true),
    ("rip.get_tool_schema", true),
    ("rip.run_tool", true),
    ("rip.search_knowledge", false),
    ("rip.get_run_status", true),
    ("rip.cancel_run", true),
];

/// 能力面工具的一行 description（标准 Tool 对象可选字段；只描述平台交互面，
/// 不携带工具全文，计划 §11:658 纪律）。
pub fn capability_description(tool: &str) -> &'static str {
    match tool {
        "rip.list_tools" => "工具目录分页（注册面摘要，total/limit/items/nextCursor）",
        "rip.get_tool_schema" => "按需取工具入参 / 出参 Schema（注册面与能力面同一入口）",
        "rip.run_tool" => "执行工具（经 tool-engine 派发；T2 须带 authorization_ref）",
        "rip.search_knowledge" => "知识检索（尚未接线，调用返回结构化 isError）",
        "rip.get_run_status" => "查询 tool_run 终态与 Artifact / Evidence 引用",
        "rip.cancel_run" => "协作式取消（RUNNING / WAITING_FOR_RUNNER 期受理）",
        _ => "",
    }
}

/// 能力面工具的入参 Schema（tools/list 标准 inputSchema 与
/// rip.get_tool_schema 同源同形状）。
pub fn capability_input_schema(tool: &str) -> Value {
    let s = |props: Value, required: &[&str]| {
        let mut schema = json!({ "type": "object", "properties": props });
        if !required.is_empty() {
            schema["required"] = json!(required);
        }
        schema
    };
    match tool {
        "rip.list_tools" => s(
            json!({
                "cursor": { "type": "string", "description": "分页游标（返回的 nextCursor 原样回传）" },
                "limit": { "type": "integer", "description": "页大小，1 到 200，默认 20" },
                "keyword": { "type": "string", "description": "按 name/tool_id 字面匹配（作用于注册面）" },
                "lifecycle": { "type": "string", "description": "REGISTERED|VALIDATED|ENABLED|DISABLED|DEPRECATED（作用于注册面）" },
                "governance": { "type": "string", "description": "ACTIVE|QUARANTINED|REJECTED|ARCHIVED（作用于注册面）" }
            }),
            &[],
        ),
        "rip.get_tool_schema" => s(
            json!({
                "tool_id": { "type": "string",
                    "description": "注册面用 tool 表 tool_id（如 chisel / rip.kb.rtis）；能力面用完整名（如 rip.run_tool）" }
            }),
            &["tool_id"],
        ),
        "rip.run_tool" => s(
            json!({
                "tool_id": { "type": "string", "description": "tool 表 tool_id（须已在运行库经 rp-tool register 登记）" },
                "input": { "type": "object", "description": "工具入参（按 Manifest input_schema 校验，D-TC-05）" },
                "task_id": { "type": "string", "description": "关联 research_task（可选）" },
                "authorization_ref": { "type": "string", "description": "T2 工具必填的授权引用（缺省按 POLICY_DENIED 拒绝，D-TC-05）" },
                "tool_version": { "type": "string", "description": "版本钉定（可选，缺省 Registry current）" },
                "workspace_id": { "type": "string", "description": "工作区隔离边界（可选，缺省 ws-mcp）" },
                "budget": { "type": "object", "description": "预算覆盖（可选：timeout_seconds / max_output_bytes，超过 Manifest 上限按 VALIDATION 拒绝）" }
            }),
            &["tool_id", "input"],
        ),
        "rip.search_knowledge" => s(
            json!({
                "q": { "type": "string", "description": "检索词" },
                "limit": { "type": "integer", "description": "条数上限" }
            }),
            &["q"],
        ),
        "rip.get_run_status" => s(
            json!({
                "run_id": { "type": "string", "description": "tool_run.run_id" }
            }),
            &["run_id"],
        ),
        "rip.cancel_run" => s(
            json!({
                "run_id": { "type": "string", "description": "tool_run.run_id（RUNNING / WAITING_FOR_RUNNER 可发起协作式取消，D-TC-04）" }
            }),
            &["run_id"],
        ),
        _ => permissive_object_schema(),
    }
}

/// 宽松 object Schema（注册面 input_schema 列缺省 / 非 object 形态时的
/// 标准必需字段回退：语义 = 接受任意 object 入参；真实校验在执行期由
/// tool-engine 按 Manifest input_schema 承担（D-TC-05），不造假 Schema）。
pub fn permissive_object_schema() -> Value {
    json!({ "type": "object", "properties": {} })
}

/// MCP server 状态：目录路径 + 执行通道（rmcp ServerHandler 的业务承载面；
/// RP_MCP_TOKEN 授权边界在 bin 层 fail-closed 处理，不进协议面）。
#[derive(Debug, Clone)]
pub struct McpServer {
    /// 主库路径（默认 migrations/platform.db，bin 层从 RP_DB_PATH 读取）。
    pub main_db: std::path::PathBuf,
    /// aux 只读库路径（bin 层从 RP_AUX_DB 读取，缺省 None）。
    pub aux_db: Option<std::path::PathBuf>,
    /// 插件库路径（bin 层从 RP_PLUGIN_DB 读取，缺省 migrations/dev-p6.db；
    /// P9-4 resources 插件目录读面用，None = 诚实空态）。
    pub plugin_db: Option<std::path::PathBuf>,
    /// P9-2 执行通道（bin 层由 RP_RUN_DB 等装配；None = run_tool /
    /// get_run_status / cancel_run 返回结构化 isError，不伪造执行）。
    pub run_channel: Option<dispatch::RunChannel>,
}

/// 目录条目（tools/list 的 tool 对象装配面；Serialize 仅供 rip.list_tools
/// 摘要信封 items 复用（Tool Center 十二列同源投影），input_schema 列
/// 不进信封（skip），标准 Tool 对象经 [`CatalogEntry::to_mcp_tool`] 装配）。
#[derive(Debug, Clone, serde::Serialize)]
pub struct CatalogEntry {
    /// MCP 工具名（能力面 = 计划 §11 名；注册面 = `rip.<tool_id>`）。
    pub name: String,
    /// 目录层（capability=平台能力面 / registry=tool 表投影）。
    pub tier: &'static str,
    /// 注册面：tool 表 tool_id（与 Tool Center 同值）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_id: Option<String>,
    /// 注册面摘要投影（Tool Center 列表列同源）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    /// lifecycle 五态。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lifecycle: Option<String>,
    /// governance 四态。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub governance: Option<String>,
    /// T0/T1/T2。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permission_level: Option<String>,
    /// 来源登记面。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_system: Option<String>,
    /// 更新时刻。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
    /// 注册面 input_schema 列直读（P0 标准必需 inputSchema 来源；None =
    /// 列缺省 / 坏 JSON / 非 object，转标准 Tool 时回退宽松 object Schema；
    /// 摘要信封不携带本列）。
    #[serde(skip_serializing)]
    pub input_schema: Option<Value>,
}

impl CatalogEntry {
    /// 能力面固定条目。
    fn capability(name: &str) -> Self {
        Self {
            name: name.to_string(),
            tier: "capability",
            tool_id: None,
            kind: None,
            lifecycle: None,
            governance: None,
            permission_level: None,
            source_system: None,
            updated_at: None,
            input_schema: Some(capability_input_schema(name)),
        }
    }

    /// 转标准 MCP Tool 对象（P0：name + inputSchema 标准必需；description
    /// 只给能力面一行导航，注册面条目不携带说明，完整元数据经
    /// rip.get_tool_schema 按需获取，计划 §11:658 纪律）。
    pub fn to_mcp_tool(&self) -> Tool {
        let input_schema = match &self.input_schema {
            Some(v) if v.is_object() => v.clone(),
            _ => permissive_object_schema(),
        };
        let props = input_schema
            .as_object()
            .cloned()
            .unwrap_or_default();
        // rmcp Tool 为 #[non_exhaustive]：Default + 字段赋值构造。
        let mut tool = Tool::default();
        tool.name = self.name.clone().into();
        tool.description = if self.tier == "capability" {
            Some(capability_description(&self.name).to_string().into())
        } else {
            None
        };
        tool.input_schema = Arc::new(props);
        tool
    }
}

impl McpServer {
    /// 组合目录：能力面前段 + 注册面投影（MCP 名 = rip.<tool_id>），
    /// 注册面带 input_schema 列直读（P0 标准必需 inputSchema 来源）。
    pub(crate) fn catalog_entries(
        &self,
        filters: &RegistryFilters,
    ) -> Result<Vec<CatalogEntry>, CatalogError> {
        let mut entries: Vec<CatalogEntry> = CAPABILITY_TOOLS
            .iter()
            .map(|(name, _)| CatalogEntry::capability(name))
            .collect();
        for entry in self.registry_rows(filters)? {
            let row = entry.row;
            entries.push(CatalogEntry {
                name: format!("rip.{}", row.tool_id),
                tier: "registry",
                tool_id: Some(row.tool_id),
                kind: row.kind,
                lifecycle: row.lifecycle,
                governance: row.governance,
                permission_level: row.permission_level,
                source_system: row.source_system,
                updated_at: row.updated_at,
                input_schema: entry.input_schema,
            });
        }
        Ok(entries)
    }

    fn registry_rows(
        &self,
        filters: &RegistryFilters,
    ) -> Result<Vec<catalog::RegistryRowWithSchema>, CatalogError> {
        self.open_catalog()?.list_registry_with_schema(filters)
    }

    /// 只读打开目录（主库优先 aux 回退，零 SQL 写）。
    pub(crate) fn open_catalog(&self) -> Result<Catalog, CatalogError> {
        Catalog::open(&self.main_db, self.aux_db.as_deref())
    }
}

/// 过滤参数解析（rip.list_tools 共用；词表校验在 catalog 层）。
pub(crate) fn parse_filters(params: &Value) -> Result<RegistryFilters, String> {
    let get = |key: &str| -> Option<String> {
        params
            .get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
    };
    Ok(RegistryFilters {
        keyword: get("keyword"),
        lifecycle: get("lifecycle"),
        governance: get("governance"),
    })
}

/// 游标解析：`v1-<offset>`，缺省 / 空串 = 0（tools/list 与 rip.list_tools
/// 共用不透明游标口径）。
pub(crate) fn parse_offset(cursor: Option<&str>) -> Result<usize, String> {
    match cursor {
        None | Some("") => Ok(0),
        Some(c) => c
            .strip_prefix("v1-")
            .and_then(|rest| rest.parse::<usize>().ok())
            .ok_or_else(|| format!("非法游标：{c:?}（期望 v1-<offset>）")),
    }
}

/// limit 参数解析（rip.list_tools 面）：1 到 200（rp_server.py parse_limit
/// 同款边界），缺省 20。tools/list 标准分页无 limit 参数，页大小由服务端
/// [`DEFAULT_PAGE_LIMIT`] 固定承担。
pub(crate) fn parse_limit_param(raw: Option<&Value>) -> Result<usize, String> {
    match raw {
        None | Some(Value::Null) => Ok(DEFAULT_PAGE_LIMIT),
        Some(v) => {
            let n = v
                .as_u64()
                .ok_or_else(|| format!("limit 必须是正整数，收到：{v}"))?;
            if n < 1 || n as usize > MAX_PAGE_LIMIT {
                return Err(format!(
                    "limit 需在 1..{MAX_PAGE_LIMIT} 之间，收到：{n}"
                ));
            }
            Ok(n as usize)
        }
    }
}

/// 组合目录分页（tools/list 用）：offset..offset+limit 切片 + nextCursor。
pub(crate) fn paginate(
    entries: &[CatalogEntry],
    offset: usize,
    limit: usize,
) -> (Vec<CatalogEntry>, Option<String>) {
    let total = entries.len();
    let (count, next) = paginate_range(total, offset, limit);
    let page = entries
        .iter()
        .skip(offset)
        .take(count)
        .cloned()
        .collect::<Vec<_>>();
    (page, next)
}

/// 通用偏移分页算术：返回 (本页条数, 下一页游标)。
pub(crate) fn paginate_range(total: usize, offset: usize, limit: usize) -> (usize, Option<String>) {
    if offset >= total {
        return (0, None);
    }
    let count = (total - offset).min(limit);
    let next = offset + count;
    let cursor = if next < total {
        Some(format!("v1-{next}"))
    } else {
        None
    };
    (count, cursor)
}
