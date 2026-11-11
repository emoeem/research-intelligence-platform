//! project_endpoint capability：MCP server 接入点（PLUGIN-CONTRACT §3.4；
//! ROADMAP P6-6）。
//!
//! 两段语义：
//! - **握手与快照（stage，install 的 checksums 之前执行）**：对
//!   `provides.endpoint`（`[...]` JSON 数组启动命令，stdio MCP 2024-11-05）
//!   走 initialize → notifications/initialized → tools/list 握手（复用
//!   plugin-registry mcp_server 握手实现），把协议版本、serverInfo、工具
//!   列表快照 sha256 与明细并入协商快照（写包内 negotiation.json，安装链
//!   读它写 plugin.lock.negotiation（§3.4：工具列表快照哈希进 lock）并生成
//!   capabilities/<id>.yaml 描述文件（rule 7 entry 指向）；
//! - **能力映射（register_tools，REGISTERED 半边）**：MCP 工具逐个映射为
//!   mcp_client runtime 的 Tool 行落 Tool Registry（§3.1 表：MCP 工具映射
//!   为 mcp_client runtime 的 Tool），id = `{prefix}.{tool_name}`（prefix
//!   缺省取插件 id），provenance origin=internal-kit（RTIS 为 kit 资产，
//!   TOOL-INVENTORY T09/T10 口径），permission_level=T1。
//!
//! 健康检查声明（§3.4）：capability provides.health_check 原样随
//! plugin_capability.provides_json 落库（结构化探测归 runtime-gateway 轴）。

use std::path::Path;

use serde::Serialize;
use serde_json::{json, Value};

use crate::RuntimeError;
use database::Pool;
use plugin_registry::hash::sha256_hex;
use tool_registry::{catalog, Registry};

/// 注册报告（机器可读）。
#[derive(Debug, Clone, Serialize)]
pub struct EndpointRegistrationReport {
    /// capability id。
    pub capability_id: String,
    /// 握手协商的协议版本。
    pub protocol_version: String,
    /// serverInfo（快照）。
    pub server_info: Value,
    /// 工具列表快照 sha256（进 plugin.lock.negotiation）。
    pub tools_sha256: String,
    /// 工具数。
    pub tool_count: usize,
    /// 新注册的 mcp_client 工具 id。
    pub registered: Vec<String>,
    /// 同版本已存在跳过的工具 id。
    pub skipped: Vec<String>,
}

impl EndpointRegistrationReport {
    /// 单行 JSON。
    pub fn to_json_string(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{\"registered\":[]}".to_string())
    }
}

/// 握手阶段（install 的 checksums 之前）：写协商快照与 capability 描述文件。
/// 返回协商快照（含 tools_sha256）。
pub fn stage(manifest: &Value, package_dir: &Path) -> Result<(), RuntimeError> {
    for (capability_id, endpoint, descriptor_entry) in endpoint_capabilities(manifest)? {
        let command = parse_command(&endpoint)?;
        let handshake =
            plugin_registry::source::mcp_server::handshake_with_cwd(&command, Some(package_dir))
                .map_err(RuntimeError::from)?;
        let tools_sha256 = handshake["tools_sha256"]
            .as_str()
            .unwrap_or_default()
            .to_string();

        // 协商快照合并：既有快照（local_dir/git/mcp_server 来源写入）之上
        // 追加 endpoints 数组与顶层 tools_sha256。
        let mut negotiation = match std::fs::read_to_string(package_dir.join("negotiation.json")) {
            Ok(body) => serde_json::from_str::<Value>(&body).unwrap_or_else(|_| json!({})),
            Err(_) => json!({}),
        };
        if !negotiation.is_object() {
            negotiation = json!({});
        }
        let endpoints = negotiation
            .as_object_mut()
            .unwrap()
            .entry("endpoints".to_string())
            .or_insert_with(|| json!([]));
        endpoints.as_array_mut().unwrap().push(json!({
            "capability_id": capability_id,
            "protocol_version": handshake["protocol_version"],
            "server_info": handshake["server_info"],
            "tools_sha256": tools_sha256,
            "tools": handshake["tools"],
        }));
        negotiation["tools_sha256"] = json!(tools_sha256);
        std::fs::write(
            package_dir.join("negotiation.json"),
            negotiation.to_string(),
        )
        .map_err(|e| RuntimeError::new("io", format!("write negotiation.json: {e}")))?;

        // capability 描述文件（rule 7 entry 指向；已存在则覆盖）。
        let desc_dir = package_dir.join("capabilities");
        std::fs::create_dir_all(&desc_dir).map_err(|e| {
            RuntimeError::new("io", format!("create_dir {:?}: {e}", desc_dir.display()))
        })?;
        let descriptor = format!(
            "# MCP endpoint snapshot (capability: {capability_id}, tools_sha256: {tools_sha256})\n{}\n",
            serde_json::to_string_pretty(&handshake["tools"]).unwrap_or_default()
        );
        std::fs::write(desc_dir.join(&descriptor_entry), descriptor)
            .map_err(|e| RuntimeError::new("io", format!("write descriptor: {e}")))?;
    }
    Ok(())
}

/// 注册阶段（REGISTERED 半边）：MCP 工具 → mcp_client runtime Tool 行。
/// `package_dir` = 二次握手的子进程 CWD（endpoint 命令按包内相对路径解析，
/// 与 stage 同一口径）。
pub fn register_tools(
    pool: &Pool,
    manifest: &Value,
    package_dir: &Path,
) -> Result<Vec<EndpointRegistrationReport>, RuntimeError> {
    let mut reports = Vec::new();
    for (capability_id, endpoint, _descriptor_entry) in endpoint_capabilities(manifest)? {
        let command = parse_command(&endpoint)?;
        let handshake =
            plugin_registry::source::mcp_server::handshake_with_cwd(&command, Some(package_dir))
                .map_err(RuntimeError::from)?;
        let tools = handshake["tools"].clone();
        let tools_sha256 = handshake["tools_sha256"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        let server_info = handshake["server_info"].clone();
        let protocol_version = handshake["protocol_version"]
            .as_str()
            .unwrap_or_default()
            .to_string();

        let obj = manifest.as_object().expect("checked object");
        let plugin_id = obj
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let plugin_version = obj
            .get("version")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let license = obj
            .get("license")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        // prefix 按 capability 各自取（provides.tool_id_prefix 缺省插件 id）。
        let cap_prefix = manifest
            .get("capabilities")
            .and_then(Value::as_array)
            .and_then(|caps| {
                caps.iter()
                    .find(|c| c.get("id").and_then(Value::as_str) == Some(capability_id.as_str()))
                    .and_then(|c| c.pointer("/provides/tool_id_prefix"))
                    .and_then(Value::as_str)
                    .map(str::to_string)
            })
            .unwrap_or_else(|| plugin_id.clone());

        let registry = Registry::open(pool.clone());
        let mut report = EndpointRegistrationReport {
            capability_id: capability_id.clone(),
            protocol_version,
            server_info,
            tools_sha256: tools_sha256.clone(),
            tool_count: tools.as_array().map(|a| a.len()).unwrap_or(0),
            registered: Vec::new(),
            skipped: Vec::new(),
        };
        if let Some(tool_list) = tools.as_array() {
            for tool in tool_list {
                let tool_name = tool
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                if tool_name.is_empty() {
                    continue;
                }
                let tool_id = format!("{cap_prefix}.{tool_name}");
                let description = tool
                    .get("description")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                let manifest_row = json!({
                    "schema_version": "1",
                    "id": tool_id,
                    "name": tool_name.chars().take(80).collect::<String>(),
                    "version": plugin_version,
                    "kind": "tool",
                    "family": "utility",
                    "description": description,
                    "runtime": {
                        "type": "mcp_client",
                        "endpoint": endpoint,
                        "tool_name": tool_name,
                    },
                    "input_schema": tool.get("inputSchema").cloned().unwrap_or(json!({"type": "object"})),
                    "output_schema": {"type": "object"},
                    "resources": {"cpu": "1", "memory": "256Mi", "timeout_seconds": 60},
                    "network": {"mode": "none"},
                    "secrets": [],
                    "permission_level": "T1",
                    "idempotent": false,
                    "health": {"probe": {"type": "none"}},
                    "provenance": {
                        "origin": "internal-kit",
                        "license": license,
                        "dependencies": [],
                        "tested": "P6-6 project_endpoint handshake (tools snapshot sha256 recorded in plugin.lock)",
                    },
                });
                match catalog::query(pool, &tool_id) {
                    Ok(detail) => {
                        if detail.versions.iter().any(|v| v.version == plugin_version) {
                            report.skipped.push(tool_id);
                            continue;
                        }
                        registry.register(&manifest_row).map_err(|e| {
                            RuntimeError::new(
                                "tool_registration_failed",
                                format!("{tool_id:?}: {e}"),
                            )
                        })?;
                        report.registered.push(tool_id);
                    }
                    Err(tool_registry::RegistryError::NotFound { .. }) => {
                        registry.register(&manifest_row).map_err(|e| {
                            RuntimeError::new(
                                "tool_registration_failed",
                                format!("{tool_id:?}: {e}"),
                            )
                        })?;
                        report.registered.push(tool_id);
                    }
                    Err(e) => return Err(RuntimeError::new("db", e.to_string())),
                }
            }
        }
        reports.push(report);
    }
    Ok(reports)
}

/// 提取 project_endpoint capability 三元组：(id, endpoint, descriptor 文件名)。
fn endpoint_capabilities(manifest: &Value) -> Result<Vec<(String, String, String)>, RuntimeError> {
    let mut out = Vec::new();
    let Some(caps) = manifest.get("capabilities").and_then(Value::as_array) else {
        return Ok(out);
    };
    for cap in caps {
        if cap.get("type").and_then(Value::as_str) != Some("project_endpoint") {
            continue;
        }
        let capability_id = cap
            .get("id")
            .and_then(Value::as_str)
            .ok_or_else(|| RuntimeError::new("manifest_invalid", "capability has no `id`"))?
            .to_string();
        let endpoint = cap
            .pointer("/provides/endpoint")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                RuntimeError::new(
                    "endpoint_missing",
                    format!(
                        "capability {capability_id:?} project_endpoint requires provides.endpoint"
                    ),
                )
            })?
            .to_string();
        let descriptor_entry = format!("{capability_id}.yaml");
        out.push((capability_id, endpoint, descriptor_entry));
    }
    Ok(out)
}

/// endpoint 启动命令解析（`[...]` JSON 数组；复用 mcp_server 语义）。
fn parse_command(endpoint: &str) -> Result<Vec<String>, RuntimeError> {
    if !endpoint.starts_with('[') {
        return Err(RuntimeError::new(
            "endpoint_form_unsupported",
            format!(
                "project_endpoint endpoint must be a launch command `[...]` in this batch; got {endpoint:?}"
            ),
        ));
    }
    serde_json::from_str::<Vec<String>>(endpoint)
        .map_err(|e| RuntimeError::new("endpoint_invalid", format!("endpoint command array: {e}")))
}

/// 工具快照 sha256 复算（对账用；与握手实现同口径）。
pub fn tools_snapshot_sha256(tools: &Value) -> String {
    sha256_hex(tools.to_string().as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_form_required() {
        let err = parse_command("http://127.0.0.1:9/mcp").unwrap_err();
        assert_eq!(err.code, "endpoint_form_unsupported");
        assert!(parse_command("[\"true\"]").is_ok());
    }

    #[test]
    fn missing_endpoint_rejected() {
        let manifest = json!({"capabilities": [{"id": "p.e", "type": "project_endpoint",
            "entry": "x", "summary": "s"}]});
        let err = endpoint_capabilities(&manifest).unwrap_err();
        assert_eq!(err.code, "endpoint_missing");
    }

    #[test]
    fn snapshot_sha_matches_handshake_semantics() {
        let tools = json!([{"name": "a"}]);
        assert_eq!(
            tools_snapshot_sha256(&tools),
            sha256_hex(tools.to_string().as_bytes())
        );
    }
}
