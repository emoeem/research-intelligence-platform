//! rmcp ServerHandler 业务接入层（rmcp 迁移整改批次，审计 §5 任务 4/5/6）。
//!
//! 协议职责全部交给 rmcp（MCP 官方 Rust SDK 3.10.0，选型记录
//! docs/phase9/RMCP-MIGRATION.md）：JSON-RPC 2.0 解析、initialize 生命周期、
//! 协议版本协商（[`ServerHandler::supported_protocol_versions`] 上限 +
//! SDK 协商实现：客户端版本在支持列表内则按规范协商回显，不在则回落
//! 服务端声明版本，绝不假装支持任意客户端版本）、协议错误信封、通知静默
//! （notifications/initialized 等）与 stdio I/O（transport 承担，stdout 只出
//! 协议消息）。本模块只实现 RIP 业务面：
//! - tools/list：目录分页（能力面前段 + 注册面投影），P0 整改：每个 Tool
//!   对象含标准必需 name 与 inputSchema（[`CatalogEntry::to_mcp_tool`]）；
//! - tools/call：rip.* 能力面派发（发现 / 执行 / 运行面）+ 注册面定向提示，
//!   sqlite 与工具执行经 spawn_blocking 隔离 async 运行时（run 执行期
//!   cancel 仍可并发受理，协作式取消 D-TC-04 不被阻塞面卡死）；
//! - resources 与 prompts：P9-4 面透传（[`crate::resources`] /
//!   [`crate::prompts`]）。
//!
//! 错误码映射（与手写协议层口径一致）：BadFilter / 参数缺失 / 未知工具 /
//! 未知资源 = -32602 invalid_params；目录数据面不可读 = -32603 internal，
//! data 携带 reason/message（真实空态说明）；词表外方法由 rmcp 兜底
//! -32601 method_not_found。

use std::borrow::Cow;

use rmcp::ErrorData as McpError;
use rmcp::handler::server::ServerHandler;
use rmcp::model::{
    CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock, Cursor,
    ErrorCode, GetPromptRequestParams, GetPromptResponse, GetPromptResult,
    ListPromptsResult, ListResourcesResult, ListResourceTemplatesResult, ListToolsResult,
    PaginatedRequestParams, Prompt, PromptArgument, PromptMessage, ProtocolVersion,
    ReadResourceRequestParams, ReadResourceResponse, ReadResourceResult, Resource,
    ResourceContents, ResourceTemplate, Role, ServerConfig, ToolsCapability,
};
use rmcp::service::{RequestContext, RoleServer};
use serde_json::{Value, json};

use crate::catalog::{CatalogError, RegistryFilters};
use crate::dispatch::RunChannel;
use crate::resources::ResourceError;
use crate::{
    CAPABILITY_TOOLS, CatalogEntry, DEFAULT_PAGE_LIMIT, McpServer, SERVER_NAME, SERVER_VERSION,
    capability_input_schema, paginate, parse_filters, parse_limit_param, parse_offset,
};

/// rmcp 实际支持的协议版本上限（initialize 握手面；2026-07-28 起生命周期
/// 改为 per-request _meta（SEP-2567），无握手可答，不在本服务支持列表）。
const SERVER_SUPPORTED_PROTOCOL_VERSIONS: [ProtocolVersion; 4] = [
    ProtocolVersion::V_2024_11_05,
    ProtocolVersion::V_2025_03_26,
    ProtocolVersion::V_2025_06_18,
    ProtocolVersion::V_2025_11_25,
];

/// 服务端支持的最高协议版本（stderr 日志与文档口径；协商语义见
/// [`ServerHandler::supported_protocol_versions`]）。
pub const SERVER_SUPPORTED_VERSIONS_LAST: &str = "2025-11-25";

/// -32602 invalid_params。
fn invalid_params(message: impl Into<Cow<'static, str>>) -> McpError {
    McpError::new(ErrorCode::INVALID_PARAMS, message, None)
}

/// -32603 internal（可选 data：真实空态 reason/message）。
fn internal_error(message: impl Into<Cow<'static, str>>, data: Option<Value>) -> McpError {
    McpError::new(ErrorCode::INTERNAL_ERROR, message, data)
}

/// 目录数据面错误转 MCP 协议错误：BadFilter（词表外参数值）= -32602；
/// Unavailable（真实空态）/ Db = -32603，data 携带 reason/message
/// （按缺什么说什么）。
fn catalog_error(e: CatalogError) -> McpError {
    match e {
        CatalogError::BadFilter(msg) => invalid_params(msg),
        CatalogError::Unavailable { reason, message } => internal_error(
            message.clone(),
            Some(json!({ "reason": reason, "message": message })),
        ),
        other => internal_error(other.to_string(), None),
    }
}

/// 资源数据面错误转 MCP 协议错误（词表外 / 缺 id / 实体不存在 = -32602，
/// 数据面不可读 = -32603 + reason/message data）。
fn resource_error(e: ResourceError) -> McpError {
    match e {
        ResourceError::UnknownUri(uri) => invalid_params(format!(
            "{uri}（七 resource 词表：rip://workspace/current、rip://knowledge/catalog、rip://plugins/catalog、rip://tools/catalog、rip://tasks/<task_id>、rip://runs/<run_id>、rip://evidence/<evidence_id>）"
        )),
        ResourceError::InvalidUri(msg) | ResourceError::NotFound(msg) => invalid_params(msg),
        ResourceError::Unavailable { reason, message } => internal_error(
            message.clone(),
            Some(json!({ "reason": reason, "message": message })),
        ),
        ResourceError::Db(err) => internal_error(err.to_string(), None),
    }
}

/// tools/call 载荷转标准结果信封（content[0].text = pretty JSON + isError，
/// 与 REST / CLI 面同一 JSON 载荷形状；手写协议层 call_result 同口径）。
fn call_text_result(payload: Value, is_error: bool) -> CallToolResult {
    let text = serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".to_string());
    if is_error {
        CallToolResult::error(vec![ContentBlock::text(text)])
    } else {
        CallToolResult::success(vec![ContentBlock::text(text)])
    }
}

/// RunChannel 缺席时的结构化说明（不伪造执行）。
fn run_channel_missing_result(name: &str) -> CallToolResult {
    call_text_result(
        json!({
            "ok": false,
            "tool": name,
            "error": "run face 未配置（RP_RUN_DB 未装配；bin 层缺省 rip-data/tool-runs.db）",
            "wired_in": "P9-2"
        }),
        true,
    )
}

/// tools/call 的未接线结果（isError=true，不伪造执行成功）。
/// 当前仅 rip.search_knowledge 走此面（后续批次接线）。
fn unwired_result(name: &str) -> CallToolResult {
    call_text_result(
        json!({
            "ok": false,
            "tool": name,
            "error": "该能力面尚未接线（P9-2 已接通 rip.run_tool / rip.get_run_status / rip.cancel_run；search_knowledge 属后续批次）",
            "wired_in": "后续批次"
        }),
        true,
    )
}

/// 注册面工具直呼（rip.<tool_id>）的定向结果：执行面统一走 rip.run_tool
/// 通道（P9-2），目录名直呼不派发、不伪造执行。
fn registry_direct_call_result(name: &str) -> CallToolResult {
    call_text_result(
        json!({
            "ok": false,
            "tool": name,
            "error": "注册面工具经 rip.run_tool 通道执行（arguments: {\"tool_id\": \"<tool_id>\", \"input\": {...}}）；目录名直呼不派发",
            "run_via": "rip.run_tool"
        }),
        true,
    )
}

/// 运行面阻塞调用隔离：sqlite 打开 / 迁移 / 工具执行都是阻塞面，经
/// spawn_blocking 隔离 async 运行时；RunChannel 缺席 = 结构化未配置说明。
async fn run_face_blocking<F>(
    channel: Option<RunChannel>,
    name: &str,
    f: F,
) -> Result<CallToolResult, McpError>
where
    F: FnOnce(&RunChannel) -> Result<Value, Value> + Send + 'static,
{
    let Some(channel) = channel else {
        return Ok(run_channel_missing_result(name));
    };
    let joined = tokio::task::spawn_blocking(move || f(&channel))
        .await
        .map_err(|e| internal_error(format!("run face join failed: {e}"), None))?;
    match joined {
        Ok(payload) => Ok(call_text_result(payload, false)),
        Err(payload) => Ok(call_text_result(payload, true)),
    }
}

impl McpServer {
    /// rip.list_tools 调用：与 tools/list 同一目录读面，limit/keyword 等
    /// 过滤参数在本工具承载（MCP 标准分页无过滤参数），返回 handle_tools
    /// 口径的摘要信封（total/returned/limit/filters/items/nextCursor，
    /// 十二列摘要投影与 REST /api/tools 同源）。
    fn call_list_tools(&self, args: &Value) -> Result<CallToolResult, McpError> {
        let filters = parse_filters(args).map_err(invalid_params)?;
        let offset =
            parse_offset(args.get("cursor").and_then(Value::as_str)).map_err(invalid_params)?;
        let limit = parse_limit_param(args.get("limit")).map_err(invalid_params)?;
        let entries = self.catalog_entries(&filters).map_err(catalog_error)?;
        // 注册面段（能力面不计入 total，handle_tools 口径的工具数）。
        let registry: Vec<&CatalogEntry> = entries.iter().filter(|e| e.tier == "registry").collect();
        let total = registry.len();
        let (page_rows, next) = crate::paginate_range(total, offset, limit);
        let mut result = json!({
            "total": total,
            "returned": page_rows,
            "limit": limit,
            "offset": offset,
            "filters": {
                "keyword": filters.keyword,
                "lifecycle": filters.lifecycle,
                "governance": filters.governance
            },
            "items": registry.iter().skip(offset).take(limit).collect::<Vec<_>>()
        });
        if let Some(cursor) = next {
            result["nextCursor"] = json!(cursor);
        }
        Ok(call_text_result(result, false))
    }

    /// rip.get_tool_schema 调用：能力面名返回内置入参 Schema；tool_id 返回
    /// tool 表 input/output Schema（主库优先，aux 回退，Tool Center 同源）；
    /// 目录两库都没有但运行库已登记的行回退运行库（source=run_db，执行面
    /// 同一 tool 表形状，只读不发明条目）。
    fn call_get_tool_schema(&self, args: &Value) -> Result<CallToolResult, McpError> {
        let Some(tool_id) = args.get("tool_id").and_then(Value::as_str) else {
            return Err(invalid_params("rip.get_tool_schema 需要 tool_id"));
        };
        if let Some((cap, _)) = CAPABILITY_TOOLS.iter().find(|(n, _)| *n == tool_id) {
            return Ok(call_text_result(
                json!({
                    "tool_id": cap,
                    "source": "capability",
                    "input_schema": capability_input_schema(cap),
                    "output_schema": Value::Null
                }),
                false,
            ));
        }
        let catalog_result = self.open_catalog().and_then(|c| c.tool_schema(tool_id));
        match catalog_result {
            Ok(schema) => Ok(call_text_result(json!(schema), false)),
            Err(CatalogError::NotFound(t)) => match self.run_schema_fallback(tool_id) {
                Some(payload) => Ok(call_text_result(payload, false)),
                None => Err(invalid_params(format!("tool 不存在：{t}"))),
            },
            Err(e) => Err(catalog_error(e)),
        }
    }

    /// 运行库 Schema 回退（目录两库 NotFound 时；RunChannel 缺席、运行库
    /// 无此行或读失败返回 None，维持原 -32602 语义）。
    fn run_schema_fallback(&self, tool_id: &str) -> Option<Value> {
        let channel = self.run_channel.as_ref()?;
        match channel.tool_schema(tool_id) {
            Ok(Some(schemas)) => Some(json!({
                "tool_id": tool_id,
                "source": "run_db",
                "input_schema": schemas["input_schema"],
                "output_schema": schemas["output_schema"]
            })),
            _ => None,
        }
    }
}

impl ServerHandler for McpServer {
    /// 服务端声明信息：能力面 tools/resources/prompts（listChanged=false、
    /// resources subscribe=false，手写协议层同口径）、一行导航 instructions
    /// （不携带任何工具全文，计划 §11:658）与服务端声明协议版本。
    fn get_info(&self) -> ServerConfig {
        // rmcp 能力面与 ServerConfig 均为 #[non_exhaustive]：builder + 字段
        // 赋值构造，显式声明 listChanged=false / subscribe=false（手写协议层
        // 同口径）。
        let mut capabilities = rmcp::model::ServerCapabilities::builder()
            .enable_prompts()
            .enable_resources()
            .enable_tools()
            .build();
        let mut tools = ToolsCapability::default();
        tools.list_changed = Some(false);
        capabilities.tools = Some(tools);
        let mut resources = rmcp::model::ResourcesCapability::default();
        resources.subscribe = Some(false);
        resources.list_changed = Some(false);
        capabilities.resources = Some(resources);
        let mut prompts = rmcp::model::PromptsCapability::default();
        prompts.list_changed = Some(false);
        capabilities.prompts = Some(prompts);
        let mut info = ServerConfig::new(capabilities);
        info.protocol_version = ProtocolVersion::V_2025_11_25;
        info.server_info = rmcp::model::Implementation::new(SERVER_NAME, SERVER_VERSION);
        info.instructions = Some(
            "工具目录经 tools/list 分页发现（每条含标准 name 与 inputSchema）；详情用 rip.get_tool_schema 按需获取；执行用 rip.run_tool（T2 须带 authorization_ref），状态与取消用 rip.get_run_status / rip.cancel_run；背景读 resources，剧本读 prompts。"
                .to_string(),
        );
        info
    }

    /// 服务端实际支持的协议版本（rmcp 协商上限；P1 整改：版本协商交给
    /// SDK，支持列表内协商回显，列表外回落服务端声明版本）。
    fn supported_protocol_versions(&self) -> Cow<'static, [ProtocolVersion]> {
        Cow::Borrowed(&SERVER_SUPPORTED_PROTOCOL_VERSIONS)
    }

    /// tools/list：标准分页目录（cursor 参数唯一入参，MCP 2024-11-05 标准
    /// 形状；过滤经 rip.list_tools 承载）。P0 整改：每个条目含标准必需
    /// name 与 inputSchema；页大小服务端固定 [`DEFAULT_PAGE_LIMIT`]，
    /// 分页只控量不省必需 Schema。
    async fn list_tools(
        &self,
        request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, McpError> {
        let cursor: Option<Cursor> = request.and_then(|params| params.cursor);
        let offset = parse_offset(cursor.as_deref()).map_err(invalid_params)?;
        let entries = self
            .catalog_entries(&RegistryFilters::default())
            .map_err(catalog_error)?;
        let (page, next) = paginate(&entries, offset, DEFAULT_PAGE_LIMIT);
        let tools = page.iter().map(CatalogEntry::to_mcp_tool).collect();
        let mut result = ListToolsResult::with_all_items(tools);
        result.next_cursor = next;
        Ok(result)
    }

    /// tools/call：rip.* 能力面派发 + 注册面定向提示；未知工具 -32602；
    /// 未接线能力面与执行面未配置返回结构化 isError（不伪造成功）。
    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, McpError> {
        let name = request.name.to_string();
        let args = Value::Object(request.arguments.unwrap_or_default());
        match name.as_str() {
            "rip.list_tools" => self.call_list_tools(&args).map(Into::into),
            "rip.get_tool_schema" => self.call_get_tool_schema(&args).map(Into::into),
            "rip.run_tool" => {
                run_face_blocking(self.run_channel.clone(), "rip.run_tool", move |channel| {
                    channel.run_tool(&args)
                })
                .await
                .map(Into::into)
            }
            "rip.get_run_status" | "rip.cancel_run" => {
                let kind = name.clone();
                let Some(run_id) = args.get("run_id").and_then(Value::as_str).map(str::to_string)
                else {
                    return Err(invalid_params(format!("{name} 需要 run_id")));
                };
                run_face_blocking(self.run_channel.clone(), &name, move |channel| {
                    if kind == "rip.get_run_status" {
                        channel.run_status(&run_id)
                    } else {
                        channel.cancel_run(&run_id)
                    }
                })
                .await
                .map(Into::into)
            }
            _ => {
                if CAPABILITY_TOOLS.iter().any(|(n, _)| *n == name.as_str()) {
                    return Ok(unwired_result(&name).into());
                }
                // 注册面调用尝试：rip.<tool_id> 在目录内 = 已通告（执行走
                // rip.run_tool 通道，目录名直呼不派发）；目录外 = -32602。
                let tool_id = name.strip_prefix("rip.").unwrap_or(&name).to_string();
                let filters = RegistryFilters {
                    keyword: Some(tool_id.clone()),
                    ..Default::default()
                };
                let entries = self.catalog_entries(&filters).map_err(catalog_error)?;
                let known = entries.iter().any(|e| {
                    e.tier == "registry" && e.tool_id.as_deref() == Some(tool_id.as_str())
                });
                if known {
                    Ok(registry_direct_call_result(&name).into())
                } else {
                    Err(invalid_params(format!(
                        "unknown tool: {name}（目录外工具；目录见 tools/list）"
                    )))
                }
            }
        }
    }

    /// resources/list：静态目录四件（P9-4 词表透传）。
    async fn list_resources(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, McpError> {
        let resources = crate::resources::static_resources()
            .into_iter()
            .map(|r| {
                // rmcp Resource 为 #[non_exhaustive]：new + 字段赋值构造。
                let mut resource = Resource::new(r.uri, r.name);
                resource.description = Some(r.description.to_string());
                resource.mime_type = Some(r.mime_type.to_string());
                resource
            })
            .collect();
        let result = ListResourcesResult::with_all_items(resources);
        Ok(result)
    }

    /// resources/templates/list：模板三件（P9-4 词表透传）。
    async fn list_resource_templates(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourceTemplatesResult, McpError> {
        let resource_templates = crate::resources::resource_templates()
            .into_iter()
            .map(|t| {
                // rmcp ResourceTemplate 为 #[non_exhaustive]：new + 字段赋值。
                let mut template = ResourceTemplate::new(t.uri_template, t.name);
                template.description = Some(t.description.to_string());
                template.mime_type = Some(t.mime_type.to_string());
                template
            })
            .collect();
        let result = ListResourceTemplatesResult::with_all_items(resource_templates);
        Ok(result)
    }

    /// resources/read：七 resource 唯一入口（[`crate::resources`] 分发），
    /// 实体不存在 -32602、数据面不可读 -32603（reason 词表同目录读面）。
    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResponse, McpError> {
        let uri = request.uri;
        match self.read_resource(&uri) {
            Ok(doc) => {
                let text = serde_json::to_string_pretty(&doc.payload)
                    .unwrap_or_else(|_| "{}".to_string());
                let contents = vec![ResourceContents::TextResourceContents {
                    uri: doc.uri,
                    mime_type: Some(doc.mime_type.to_string()),
                    text,
                    meta: None,
                }];
                Ok(ReadResourceResult::new(contents).into())
            }
            Err(e) => Err(resource_error(e)),
        }
    }

    /// prompts/list：prompt 三条摘要（P9-4 词表透传）。
    async fn list_prompts(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListPromptsResult, McpError> {
        let prompts = crate::prompts::list()
            .into_iter()
            .map(|p| {
                // rmcp Prompt / PromptArgument 为 #[non_exhaustive]：
                // new + 字段赋值构造。
                let arguments = Some(
                    p.arguments
                        .into_iter()
                        .map(|a| {
                            let mut arg = PromptArgument::new(a.name);
                            arg.description = Some(a.description.to_string());
                            arg.required = Some(a.required);
                            arg
                        })
                        .collect(),
                );
                let mut prompt = Prompt::new(
                    p.name,
                    Some(p.description.to_string()),
                    arguments,
                );
                prompt.title = Some(p.title.to_string());
                prompt
            })
            .collect();
        Ok(ListPromptsResult::with_all_items(prompts))
    }

    /// prompts/get：name + arguments → messages（未知 prompt / 缺必填 -32602）。
    async fn get_prompt(
        &self,
        request: GetPromptRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<GetPromptResponse, McpError> {
        let name = request.name;
        let args = Value::Object(request.arguments.unwrap_or_default());
        match crate::prompts::get(&name, &args) {
            Ok((description, messages)) => {
                let messages = messages
                    .into_iter()
                    .map(|m| {
                        let text = m
                            .get("content")
                            .and_then(|c| c.get("text"))
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_string();
                        PromptMessage::new_text(Role::User, text)
                    })
                    .collect();
                Ok(GetPromptResult::new(messages)
                    .with_description(description)
                    .into())
            }
            Err(msg) => Err(invalid_params(msg)),
        }
    }
}
