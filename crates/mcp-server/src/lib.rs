//! Phase 9 平台 MCP server crate（bin rp-mcp）。
//!
//! 职责（ROADMAP §10.3 P9-1 + P9-2 + P9-4）：
//! - [`catalog`]：工具目录读面，与 Tool Center 同源（同一 tool 表读面，
//!   aux 增量合并同 rp_server.py handle_tools 口径，详见模块文档）；
//! - [`dispatch`]：P9-2 rip.run_tool 端到端执行通道（经 tool-engine Engine
//!   受理与派发，结构化结果 + Artifact + Evidence 登记）与
//!   rip.get_run_status / rip.cancel_run 运行查询与协作式取消面；P9-4 起
//!   run_tool 受理走 submit_correlated 自锚定（trace_id = run_id，015/018
//!   「run 即链锚」同款惯例，REST /api/traces 可反查 MCP 通道运行）；
//! - [`resources`]：P9-4 rip:// 七 resource（计划 §11:662-668，静态四件 +
//!   模板三件），rip://runs/<run_id> 与 rip.get_run_status 同一投影；
//! - [`prompts`]：P9-4 prompt 面三条（工具执行剧本 / 审计链复核 / 知识简报）；
//! - 本文件：stdio NDJSON JSON-RPC 2.0 薄协议层（MCP 2024-11-05 子集，
//!   形态参考 integrations/rtis/rtis_mcp.mjs 但代码独立）：
//!   * `initialize` 只暴露目录与分页发现（capabilities.tools.listChanged=false
//!     + 一行导航 instructions，不携带任何工具全文说明，计划 §11:658 原文
//!       「不要在 MCP 初始化时把所有工具的完整说明暴露给模型」）；P9-4 起
//!       capabilities 增 resources（subscribe=false）与 prompts 面；
//!   * `tools/list` 分页目录：能力面（rip.* 六条，计划 §11:640-656 子集，
//!     P9-2 起含 rip.cancel_run）+ 注册面（tool 表逐行，MCP 名 =
//!     `rip.<tool_id>`）；条目为摘要投影，无 description 无 inputSchema，
//!     Schema 经 `rip.get_tool_schema` 按需获取；
//!   * `tools/call`：接通 rip.list_tools / rip.get_tool_schema（发现面）与
//!     rip.run_tool / rip.get_run_status / rip.cancel_run（P9-2 执行与运行
//!     面，经 [`dispatch::RunChannel`]）；rip.search_knowledge 仍为目录条目
//!     （调用返回结构化 isError，后续批次接线，不伪造成功）；注册面直呼
//!     `rip.<tool_id>` 仍指向 rip.run_tool 通道提示（不在目录面重复派发）；
//!   * `resources/list` / `resources/templates/list` / `resources/read` 与
//!     `prompts/list` / `prompts/get`：P9-4 落地（见上）；词表外资源 /
//!     prompt 回 -32602，数据面不可读回 -32603；
//!   * 握手鉴权：RP_MCP_TOKEN 环境变量必须配置（禁止匿名），未配置时所有
//!     请求以 -32001 auth_required 拒绝（无令牌握手被拒）；initialize 携带
//!     `params._meta.rp.auth` 时以常时比较校验（可选出示，出示必校验）；
//! - MCP 工具面不出现 Tool Center 没有的能力：注册面严格 1:1 投影 tool 表
//!   （不发明条目），能力面只收录计划 §11:640-656 中由既有平台能力面
//!   （tool 表 / tool_run 表 / knowledge 表）承载的子集，agents /
//!   workflows 等其余 rip.* 条目按批次归属后续，不在本批出现。
//!
//! 与 rtis_mcp.mjs 的边界（ROADMAP §10.2 / 盘点 461 行）：本 crate 是平台
//! MCP 面（rip.* 前缀），kit 的 rtis_mcp.mjs（redteam_* 前缀，9 工具）是
//! project_endpoint 接入面，两者命名空间隔离，互不引用。
//!
//! 纪律：stdout 只允许 JSON 行；日志一律走 stderr（rtis_mcp.mjs 同款）。

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod catalog;
pub mod dispatch;
pub mod prompts;
pub mod resources;

use std::io::Write;

use serde_json::{json, Value};

use crate::catalog::{Catalog, CatalogError, RegistryFilters};

/// MCP 协议版本（2024-11-05 子集）。
pub const PROTOCOL_VERSION: &str = "2024-11-05";
/// serverInfo.name。
pub const SERVER_NAME: &str = "rp-mcp";
/// serverInfo.version（workspace 版本）。
pub const SERVER_VERSION: &str = env!("CARGO_PKG_VERSION");

/// tools/list 默认页大小（注册面页；能力面恒在前段）。
pub const DEFAULT_PAGE_LIMIT: usize = 20;
/// tools/list 最大页大小（rp_server.py MAX_LIMIT=200 同款上限）。
pub const MAX_PAGE_LIMIT: usize = 200;

/// JSON-RPC 错误码：方法不存在。
pub const CODE_METHOD_NOT_FOUND: i64 = -32601;
/// JSON-RPC 错误码：参数非法（含游标 / 工具不存在）。
pub const CODE_INVALID_PARAMS: i64 = -32602;
/// JSON-RPC 错误码：内部错误（目录数据面不可读等）。
pub const CODE_INTERNAL: i64 = -32603;
/// JSON-RPC 错误码（服务端自定义段）：鉴权被拒（无令牌 / 令牌不匹配）。
pub const CODE_AUTH_REQUIRED: i64 = -32001;

/// 能力面工具（计划 §11:640-656 子集，固定次序 = 目录前段次序）。
/// P9-2 起 rip.run_tool / rip.get_run_status / rip.cancel_run 接通执行与
/// 运行面（[`dispatch::RunChannel`]）；`wired` 为 false 的条目（当前仅
/// rip.search_knowledge）在 tools/call 返回结构化 isError，不伪造执行结果。
pub const CAPABILITY_TOOLS: [(&str, bool); 6] = [
    ("rip.list_tools", true),
    ("rip.get_tool_schema", true),
    ("rip.run_tool", true),
    ("rip.search_knowledge", false),
    ("rip.get_run_status", true),
    ("rip.cancel_run", true),
];

/// 能力面工具的入参 Schema（按需经 rip.get_tool_schema 获取）。
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
                "cursor": { "type": "string", "description": "分页游标（tools/list 返回的 nextCursor 原样回传）" },
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
        other => json!({ "type": "object", "properties": {}, "description": format!("未知能力面工具：{other}") }),
    }
}

/// MCP server 状态：目录路径 + 握手令牌 + 执行通道。
#[derive(Debug, Clone)]
pub struct McpServer {
    /// 主库路径（默认 migrations/platform.db，bin 层从 RP_DB_PATH 读取）。
    pub main_db: std::path::PathBuf,
    /// aux 只读库路径（bin 层从 RP_AUX_DB 读取，缺省 None）。
    pub aux_db: Option<std::path::PathBuf>,
    /// 插件库路径（bin 层从 RP_PLUGIN_DB 读取，缺省 migrations/dev-p6.db；
    /// P9-4 resources 插件目录读面用，None = 诚实空态）。
    pub plugin_db: Option<std::path::PathBuf>,
    /// 握手令牌（bin 层从 RP_MCP_TOKEN 读取；None = 禁止匿名，全部请求被拒）。
    pub token: Option<String>,
    /// P9-2 执行通道（bin 层由 RP_RUN_DB 等装配；None = run_tool /
    /// get_run_status / cancel_run 返回结构化 isError，不伪造执行）。
    pub run_channel: Option<dispatch::RunChannel>,
}

/// 目录条目（tools/list 的 tool 对象）。
#[derive(Debug, Clone, serde::Serialize)]
pub struct CatalogEntry {
    /// MCP 工具名（能力面 = 计划 §11 名；注册面 = `rip.<tool_id>`）。
    pub name: String,
    /// 目录层（capability=平台能力面 / registry=tool 表投影）。
    pub tier: &'static str,
    /// 注册面：tool 表 tool_id（与 Tool Center 同值）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_id: Option<String>,
    /// 注册面摘要投影（Tool Center 列表列同源；无 description 无 inputSchema）。
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
}

/// 单条 JSON-RPC 请求的处理结果。
pub enum Outcome {
    /// 有响应（result 或 error 信封）。
    Reply(Value),
    /// 通知（无 id），不回任何行。
    Silent,
}

impl McpServer {
    /// 处理一行 stdio 输入：合法 JSON 行返回 [`Outcome`]；坏行记 stderr 后
    /// 由调用方跳过（rtis_mcp.mjs bad json line 同款，不回 -32700）。
    pub fn handle_line(&self, line: &str) -> LineOutcome {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return LineOutcome::Skip;
        }
        let msg: Value = match serde_json::from_str(trimmed) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("[rp-mcp] bad json line: {e}");
                return LineOutcome::Skip;
            }
        };
        match self.handle_message(&msg) {
            Outcome::Silent => LineOutcome::Silent,
            Outcome::Reply(v) => match serde_json::to_string(&v) {
                Ok(s) => LineOutcome::Reply(s),
                Err(e) => {
                    eprintln!("[rp-mcp] serialize error: {e}");
                    LineOutcome::Skip
                }
            },
        }
    }

    /// 消息派发（JSON-RPC 2.0 + MCP 2024-11-05 子集）。
    pub fn handle_message(&self, msg: &Value) -> Outcome {
        let method = msg.get("method").and_then(Value::as_str);
        let has_id = msg.get("id").is_some();
        let id = msg.get("id").cloned().unwrap_or(Value::Null);
        let params = msg.get("params").cloned().unwrap_or(Value::Null);
        let Some(method) = method else {
            return if has_id {
                Outcome::Reply(error_envelope(&id, CODE_INVALID_PARAMS, "缺少 method 字段"))
            } else {
                Outcome::Silent
            };
        };
        if method == "notifications/initialized" || method == "initialized" {
            return Outcome::Silent;
        }
        if !has_id {
            // 其余通知一律静默（本子集无服务器主动通知面）。
            return Outcome::Silent;
        }
        // 握手鉴权门（禁止匿名）：RP_MCP_TOKEN 未配置时所有请求一律被拒。
        if self.token.is_none() {
            return Outcome::Reply(error_envelope(
                &id,
                CODE_AUTH_REQUIRED,
                "auth_required: RP_MCP_TOKEN 未配置，禁止匿名握手（由宿主经环境变量注入令牌）",
            ));
        }
        // initialize 出示令牌时必须匹配（可选出示，出示必校验，常时比较）。
        if method == "initialize" {
            if let Some(presented) = params
                .get("_meta")
                .and_then(|m| m.get("rp.auth"))
                .and_then(Value::as_str)
            {
                let expected = format!("Bearer {}", self.token.as_deref().unwrap_or(""));
                if !constant_time_eq(presented.as_bytes(), expected.as_bytes()) {
                    return Outcome::Reply(error_envelope(
                        &id,
                        CODE_AUTH_REQUIRED,
                        "auth_mismatch: _meta.rp.auth 与 RP_MCP_TOKEN 不匹配",
                    ));
                }
            }
        }
        match method {
            "initialize" => Outcome::Reply(self.initialize_reply(&id, &params)),
            "ping" => Outcome::Reply(result_envelope(&id, json!({}))),
            "tools/list" => self.tools_list(&id, &params),
            "tools/call" => self.tools_call(&id, &params),
            "resources/list" => Outcome::Reply(result_envelope(
                &id,
                json!({ "resources": resources::static_resources() }),
            )),
            "resources/templates/list" => Outcome::Reply(result_envelope(
                &id,
                json!({ "resourceTemplates": resources::resource_templates() }),
            )),
            "resources/read" => self.resources_read(&id, &params),
            "prompts/list" => Outcome::Reply(result_envelope(
                &id,
                json!({ "prompts": prompts::list() }),
            )),
            "prompts/get" => self.prompts_get(&id, &params),
            other => Outcome::Reply(error_envelope(
                &id,
                CODE_METHOD_NOT_FOUND,
                &format!("method not found: {other}（MCP 2024-11-05 子集：initialize/ping/tools/list/tools/call/resources/list/resources/templates/list/resources/read/prompts/list/prompts/get）"),
            )),
        }
    }

    /// initialize 响应：只暴露目录与分页发现，一行导航 instructions，
    /// 不携带任何工具全文（计划 §11:658）；P9-4 起 capabilities 增
    /// resources（subscribe=false）与 prompts 面（资源与 prompt 是导航面，
    /// 不携带工具全文说明）。
    fn initialize_reply(&self, id: &Value, params: &Value) -> Value {
        let protocol = params
            .get("protocolVersion")
            .and_then(Value::as_str)
            .unwrap_or(PROTOCOL_VERSION)
            .to_string();
        result_envelope(
            id,
            json!({
                "protocolVersion": protocol,
                "capabilities": {
                    "tools": { "listChanged": false },
                    "resources": { "subscribe": false },
                    "prompts": { "listChanged": false }
                },
                "serverInfo": { "name": SERVER_NAME, "version": SERVER_VERSION },
                "instructions": "工具目录经 tools/list 分页发现（摘要条目，无完整说明）；Schema 用 rip.get_tool_schema 按需获取；执行用 rip.run_tool（T2 须带 authorization_ref），状态与取消用 rip.get_run_status / rip.cancel_run；目录与审计背景读 resources，操作剧本读 prompts。"
            }),
        )
    }

    /// tools/list：分页目录 = 能力面前段（固定 5 条）+ 注册面（tool 表投影，
    /// handle_tools 排序）。条目为摘要投影：无 description 无 inputSchema。
    fn tools_list(&self, id: &Value, params: &Value) -> Outcome {
        let filters = match parse_filters(params) {
            Ok(f) => f,
            Err(e) => return Outcome::Reply(error_envelope(id, CODE_INVALID_PARAMS, &e)),
        };
        let offset = match parse_offset(params.get("cursor").and_then(Value::as_str)) {
            Ok(o) => o,
            Err(e) => return Outcome::Reply(error_envelope(id, CODE_INVALID_PARAMS, &e)),
        };
        let limit = match parse_limit_param(params.get("limit")) {
            Ok(l) => l,
            Err(e) => return Outcome::Reply(error_envelope(id, CODE_INVALID_PARAMS, &e)),
        };
        match self.catalog_entries(&filters) {
            Ok(entries) => {
                let (page, next) = paginate(&entries, offset, limit);
                let mut result = json!({ "tools": page });
                if let Some(cursor) = next {
                    result["nextCursor"] = json!(cursor);
                }
                Outcome::Reply(result_envelope(id, result))
            }
            Err(e) => Outcome::Reply(catalog_error_envelope(id, e)),
        }
    }

    /// tools/call：P9-1 接通 rip.list_tools 与 rip.get_tool_schema；P9-2 接通
    /// rip.run_tool / rip.get_run_status / rip.cancel_run（经 RunChannel）；
    /// rip.search_knowledge 仍未接线（结构化 isError，不伪造成功）。
    fn tools_call(&self, id: &Value, params: &Value) -> Outcome {
        let name = params.get("name").and_then(Value::as_str).unwrap_or("");
        let args = params.get("arguments").cloned().unwrap_or(json!({}));
        if name == "rip.list_tools" {
            return self.call_list_tools(id, &args);
        }
        if name == "rip.get_tool_schema" {
            return self.call_get_tool_schema(id, &args);
        }
        if name == "rip.run_tool" {
            return self.call_run_tool(id, &args, "rip.run_tool");
        }
        if name == "rip.get_run_status" {
            return self.call_run_channel(id, &args, "rip.get_run_status");
        }
        if name == "rip.cancel_run" {
            return self.call_run_channel(id, &args, "rip.cancel_run");
        }
        if CAPABILITY_TOOLS.iter().any(|(n, _)| *n == name) {
            return Outcome::Reply(unwired_result(id, name));
        }
        // 注册面调用尝试：rip.<tool_id> 在目录内 = 已通告（执行走
        // rip.run_tool 通道，目录名直呼不派发）；目录外 = -32602 unknown tool。
        let tool_id = name.strip_prefix("rip.").unwrap_or(name).to_string();
        let filters = RegistryFilters {
            keyword: Some(tool_id.clone()),
            ..RegistryFilters::default()
        };
        match self.catalog_entries(&filters) {
            Ok(entries) => {
                let known = entries
                    .iter()
                    .any(|e| e.tier == "registry" && e.tool_id.as_deref() == Some(tool_id.as_str()));
                if known {
                    Outcome::Reply(registry_direct_call_result(id, name))
                } else {
                    Outcome::Reply(error_envelope(
                        id,
                        CODE_INVALID_PARAMS,
                        &format!("unknown tool: {name}（目录外工具；目录见 tools/list）"),
                    ))
                }
            }
            Err(e) => Outcome::Reply(catalog_error_envelope(id, e)),
        }
    }

    /// rip.run_tool 调用（RunChannel 缺席时如实返回未配置，不伪造执行）。
    fn call_run_tool(&self, id: &Value, args: &Value, name: &str) -> Outcome {
        match self.run_channel.as_ref() {
            None => Outcome::Reply(run_channel_missing_result(id, name)),
            Some(channel) => match channel.run_tool(args) {
                Ok(payload) => Outcome::Reply(call_result(id, payload, false)),
                Err(payload) => Outcome::Reply(call_result(id, payload, true)),
            },
        }
    }

    /// rip.get_run_status / rip.cancel_run 调用（run_id 必填）。
    fn call_run_channel(&self, id: &Value, args: &Value, name: &str) -> Outcome {
        let Some(run_id) = args.get("run_id").and_then(Value::as_str) else {
            return Outcome::Reply(error_envelope(
                id,
                CODE_INVALID_PARAMS,
                &format!("{name} 需要 run_id"),
            ));
        };
        let Some(channel) = self.run_channel.as_ref() else {
            return Outcome::Reply(run_channel_missing_result(id, name));
        };
        let outcome = match name {
            "rip.get_run_status" => channel.run_status(run_id),
            _ => channel.cancel_run(run_id),
        };
        match outcome {
            Ok(payload) => Outcome::Reply(call_result(id, payload, false)),
            Err(payload) => Outcome::Reply(call_result(id, payload, true)),
        }
    }

    /// resources/read：七 resource 唯一入口（resources.rs 分发），实体不
    /// 存在 -32602、数据面不可读 -32603（reason 词表同目录读面）。
    fn resources_read(&self, id: &Value, params: &Value) -> Outcome {
        let Some(uri) = params.get("uri").and_then(Value::as_str) else {
            return Outcome::Reply(error_envelope(
                id,
                CODE_INVALID_PARAMS,
                "resources/read 需要 uri",
            ));
        };
        match self.read_resource(uri) {
            Ok(doc) => {
                let text = serde_json::to_string_pretty(&doc.payload)
                    .unwrap_or_else(|_| "{}".to_string());
                Outcome::Reply(result_envelope(
                    id,
                    json!({
                        "contents": [ {
                            "uri": doc.uri,
                            "mimeType": doc.mime_type,
                            "text": text
                        } ]
                    }),
                ))
            }
            Err(resources::ResourceError::UnknownUri(uri)) => Outcome::Reply(error_envelope(
                id,
                CODE_INVALID_PARAMS,
                &format!("{uri}（七 resource 词表：rip://workspace/current、rip://knowledge/catalog、rip://plugins/catalog、rip://tools/catalog、rip://tasks/<task_id>、rip://runs/<run_id>、rip://evidence/<evidence_id>）"),
            )),
            Err(resources::ResourceError::InvalidUri(msg))
            | Err(resources::ResourceError::NotFound(msg)) => {
                Outcome::Reply(error_envelope(id, CODE_INVALID_PARAMS, &msg))
            }
            Err(e) => {
                let mut envelope =
                    error_envelope(id, CODE_INTERNAL, &e.to_string());
                if let resources::ResourceError::Unavailable { reason, message } = &e {
                    envelope["error"]["data"] = json!({ "reason": reason, "message": message });
                }
                Outcome::Reply(envelope)
            }
        }
    }

    /// prompts/get：name + arguments → messages（未知 prompt / 缺必填 -32602）。
    fn prompts_get(&self, id: &Value, params: &Value) -> Outcome {
        let Some(name) = params.get("name").and_then(Value::as_str) else {
            return Outcome::Reply(error_envelope(id, CODE_INVALID_PARAMS, "prompts/get 需要 name"));
        };
        let args = params
            .get("arguments")
            .cloned()
            .unwrap_or_else(|| json!({}));
        match prompts::get(name, &args) {
            Ok((description, messages)) => Outcome::Reply(result_envelope(
                id,
                json!({ "description": description, "messages": messages }),
            )),
            Err(msg) => Outcome::Reply(error_envelope(id, CODE_INVALID_PARAMS, &msg)),
        }
    }

    /// rip.list_tools 调用：与 tools/list 方法同读面同分页，返回
    /// handle_tools 口径的摘要信封（total/returned/limit/filters/items/nextCursor）。
    fn call_list_tools(&self, id: &Value, args: &Value) -> Outcome {
        let filters = match parse_filters(args) {
            Ok(f) => f,
            Err(e) => return Outcome::Reply(error_envelope(id, CODE_INVALID_PARAMS, &e)),
        };
        let offset = match parse_offset(args.get("cursor").and_then(Value::as_str)) {
            Ok(o) => o,
            Err(e) => return Outcome::Reply(error_envelope(id, CODE_INVALID_PARAMS, &e)),
        };
        let limit = match parse_limit_param(args.get("limit")) {
            Ok(l) => l,
            Err(e) => return Outcome::Reply(error_envelope(id, CODE_INVALID_PARAMS, &e)),
        };
        match self.catalog_entries(&filters) {
            Ok(entries) => {
                // 注册面段（能力面不计入 total，handle_tools 口径的工具数）。
                let registry: Vec<&CatalogEntry> =
                    entries.iter().filter(|e| e.tier == "registry").collect();
                let total = registry.len();
                let (page_rows, next) = paginate_range(total, offset, limit);
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
                Outcome::Reply(call_result(id, result, false))
            }
            Err(e) => Outcome::Reply(catalog_error_envelope(id, e)),
        }
    }

    /// rip.get_tool_schema 调用：能力面名返回内置入参 Schema；tool_id 返回
    /// tool 表 input/output Schema（主库优先，aux 回退，Tool Center 同源）；
    /// 目录两库都没有但运行库已登记的行回退运行库（source=run_db，执行面
    /// 同一 tool 表形状，只读不发明条目）。
    fn call_get_tool_schema(&self, id: &Value, args: &Value) -> Outcome {
        let Some(tool_id) = args.get("tool_id").and_then(Value::as_str) else {
            return Outcome::Reply(error_envelope(
                id,
                CODE_INVALID_PARAMS,
                "rip.get_tool_schema 需要 tool_id",
            ));
        };
        if let Some((cap, _)) = CAPABILITY_TOOLS.iter().find(|(n, _)| *n == tool_id) {
            return Outcome::Reply(call_result(
                id,
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
            Ok(schema) => Outcome::Reply(call_result(id, json!(schema), false)),
            Err(CatalogError::NotFound(t)) => {
                match self.run_schema_fallback(tool_id) {
                    Some(payload) => Outcome::Reply(call_result(id, payload, false)),
                    None => Outcome::Reply(error_envelope(
                        id,
                        CODE_INVALID_PARAMS,
                        &format!("tool 不存在：{t}"),
                    )),
                }
            }
            Err(e) => Outcome::Reply(catalog_error_envelope(id, e)),
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

    /// 组合目录：能力面前段 + 注册面投影（MCP 名 = rip.<tool_id>）。
    fn catalog_entries(&self, filters: &RegistryFilters) -> Result<Vec<CatalogEntry>, CatalogError> {
        let mut entries: Vec<CatalogEntry> = CAPABILITY_TOOLS
            .iter()
            .map(|(name, _)| CatalogEntry {
                name: (*name).to_string(),
                tier: "capability",
                tool_id: None,
                kind: None,
                lifecycle: None,
                governance: None,
                permission_level: None,
                source_system: None,
                updated_at: None,
            })
            .collect();
        for row in self.registry_rows(filters)? {
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
            });
        }
        Ok(entries)
    }

    fn registry_rows(&self, filters: &RegistryFilters) -> Result<Vec<catalog::RegistryRow>, CatalogError> {
        self.open_catalog()?.list_registry(filters)
    }

    pub(crate) fn open_catalog(&self) -> Result<Catalog, CatalogError> {
        Catalog::open(&self.main_db, self.aux_db.as_deref())
    }
}

/// 行处理出口。
#[derive(Debug)]
pub enum LineOutcome {
    /// 回一行 JSON（stdout）。
    Reply(String),
    /// 通知 / 通知类消息，无输出。
    Silent,
    /// 坏行（stderr 已记录），无输出。
    Skip,
}

/// 把请求行写回 stdout（只允许 JSON 行；日志走 stderr）。
pub fn write_line<W: Write>(out: &mut W, line: &str) -> std::io::Result<()> {
    writeln!(out, "{line}")?;
    out.flush()
}

/// result 信封。
fn result_envelope(id: &Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

/// error 信封。
fn error_envelope(id: &Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

/// 目录数据面错误转 JSON-RPC：BadFilter（词表外参数值）= -32602 invalid
/// params；Unavailable（真实空态）/Db = -32603，data 携带 reason/message
/// （按缺什么说什么）。
fn catalog_error_envelope(id: &Value, e: CatalogError) -> Value {
    match e {
        CatalogError::BadFilter(msg) => error_envelope(id, CODE_INVALID_PARAMS, &msg),
        other => {
            let mut envelope = error_envelope(id, CODE_INTERNAL, &other.to_string());
            if let CatalogError::Unavailable { reason, message } = &other {
                envelope["error"]["data"] = json!({ "reason": reason, "message": message });
            }
            envelope
        }
    }
}

/// tools/call 的未接线结果（isError=true，不伪造执行成功）。
/// P9-2 后仅 rip.search_knowledge 走此面（后续批次接线）。
fn unwired_result(id: &Value, name: &str) -> Value {
    call_result(
        id,
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
fn registry_direct_call_result(id: &Value, name: &str) -> Value {
    call_result(
        id,
        json!({
            "ok": false,
            "tool": name,
            "error": "注册面工具经 rip.run_tool 通道执行（arguments: {\"tool_id\": \"<tool_id>\", \"input\": {...}}）；目录名直呼不派发",
            "run_via": "rip.run_tool"
        }),
        true,
    )
}

/// RunChannel 缺席时的结构化说明（不伪造执行）。
fn run_channel_missing_result(id: &Value, name: &str) -> Value {
    call_result(
        id,
        json!({
            "ok": false,
            "tool": name,
            "error": "run face 未配置（RP_RUN_DB 未装配；bin 层缺省 rip-data/tool-runs.db）",
            "wired_in": "P9-2"
        }),
        true,
    )
}

/// tools/call 结果信封（content 文本 + isError，rtis_mcp.mjs 形态）。
fn call_result(id: &Value, payload: Value, is_error: bool) -> Value {
    let text = serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".to_string());
    result_envelope(
        id,
        json!({
            "content": [ { "type": "text", "text": text } ],
            "isError": is_error
        }),
    )
}

/// 过滤参数解析（tools/list / rip.list_tools 共用；词表校验在 catalog 层）。
fn parse_filters(params: &Value) -> Result<RegistryFilters, String> {
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

/// 游标解析：`v1-<offset>`，缺省 / 空串 = 0。
fn parse_offset(cursor: Option<&str>) -> Result<usize, String> {
    match cursor {
        None | Some("") => Ok(0),
        Some(c) => c
            .strip_prefix("v1-")
            .and_then(|rest| rest.parse::<usize>().ok())
            .ok_or_else(|| format!("非法游标：{c:?}（期望 v1-<offset>）")),
    }
}

/// limit 参数解析：1 到 200（rp_server.py parse_limit 同款边界），缺省 20。
fn parse_limit_param(raw: Option<&Value>) -> Result<usize, String> {
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
fn paginate(entries: &[CatalogEntry], offset: usize, limit: usize) -> (Vec<CatalogEntry>, Option<String>) {
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
fn paginate_range(total: usize, offset: usize, limit: usize) -> (usize, Option<String>) {
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

/// 常时比较（长度差早退，内容按定长 XOR 折叠比较，不随匹配进度分支）。
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut fold: u8 = 0;
    for (x, y) in a.iter().zip(b.iter()) {
        fold |= x ^ y;
    }
    fold == 0
}
