//! mcp_server 来源（PLUGIN-CONTRACT §6 第四行；ROADMAP P6-4）。
//!
//! 语义：source.endpoint（URL 或启动命令）+ protocol_version。校验 = 走
//! stdio JSON-RPC 2.0 握手（initialize → notifications/initialized →
//! tools/list），协议版本协商一致后取工具列表快照哈希；能力协商结果
//! （协议版本、serverInfo、tools 快照 sha256 与明细）记录进协商快照，由
//! 安装链写入 plugin.lock.negotiation（§6 能力协商记录进 lock）。
//!
//! 形态边界（如实声明）：endpoint 若为 `[...]` 起始的 JSON 数组按启动命令
//! 解析（spawn stdio 子进程，fixture 用 python3 -I 脚本闭环）；URL 形态
//! （远程 MCP）归后续批次（协议待验证项同 PLUGIN-CONTRACT §12.4 口径）。
//! 握手超时强控归 P6-7 运行层（本批 fixture 快速退出，读至 EOF）。

use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Command, Stdio};

use serde_json::{json, Value};

use super::{FetchOutcome, Source, SourceError, TrustLevel};
use tool_registry::hash::sha256_hex;

/// mcp_server 来源实现。
pub struct McpServerSource;

/// endpoint 解析：`[` 起始按 JSON 数组启动命令，其余按 URL（本批仅命令）。
fn command_of(endpoint: &str) -> Result<Vec<String>, SourceError> {
    if endpoint.starts_with('[') {
        serde_json::from_str::<Vec<String>>(endpoint).map_err(|e| {
            SourceError::new("endpoint_invalid", format!("endpoint command array: {e}"))
        })
    } else {
        Err(SourceError::new(
            "endpoint_form_unsupported",
            format!(
                "mcp_server endpoint URL form is not open in this batch (launch command `[...]` form only); got {endpoint:?}"
            ),
        ))
    }
}

/// 一次 stdio JSON-RPC 握手（initialize + tools/list），返回协商快照。
/// 子进程 CWD 缺省继承调用方（P6-6 project_endpoint 以包目录为 CWD）。
pub fn handshake(command: &[String]) -> Result<Value, SourceError> {
    handshake_with_cwd(command, None)
}

/// 带 CWD 的握手（project_endpoint 的 endpoint 命令按包内相对路径解析）。
pub fn handshake_with_cwd(command: &[String], cwd: Option<&Path>) -> Result<Value, SourceError> {
    let (program, args) = command
        .split_first()
        .ok_or_else(|| SourceError::new("endpoint_invalid", "launch command is empty"))?;
    let mut cmd = Command::new(program);
    cmd.args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    if let Some(dir) = cwd {
        cmd.current_dir(dir);
    }
    let mut child = cmd
        .spawn()
        .map_err(|e| SourceError::new("spawn_failed", format!("spawn {program:?}: {e}")))?;
    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| SourceError::new("spawn_failed", "child stdin unavailable"))?;
    let mut stdout = BufReader::new(
        child
            .stdout
            .take()
            .ok_or_else(|| SourceError::new("spawn_failed", "child stdout unavailable"))?,
    );

    let initialize = json!({
        "jsonrpc": "2.0", "id": 1, "method": "initialize",
        "params": {
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {"name": "rip-plugin-registry", "version": "0.2.0"}
        }
    });
    writeln!(stdin, "{initialize}")
        .map_err(|e| SourceError::new("handshake_io", format!("write initialize: {e}")))?;
    // MCP 语义：initialized 通知（无 id）。
    let initialized = json!({"jsonrpc": "2.0", "method": "notifications/initialized"});
    writeln!(stdin, "{initialized}")
        .map_err(|e| SourceError::new("handshake_io", format!("write initialized: {e}")))?;
    let tools_req = json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list", "params": {}});
    writeln!(stdin, "{tools_req}")
        .map_err(|e| SourceError::new("handshake_io", format!("write tools/list: {e}")))?;
    drop(stdin); // 关闭写端，fixture 读到 EOF 后退出。

    let mut init_resp: Option<Value> = None;
    let mut tools_resp: Option<Value> = None;
    let mut line = String::new();
    loop {
        line.clear();
        let n = stdout
            .read_line(&mut line)
            .map_err(|e| SourceError::new("handshake_io", format!("read response: {e}")))?;
        if n == 0 {
            break; // EOF
        }
        let resp: Value = match serde_json::from_str(line.trim()) {
            Ok(v) => v,
            Err(_) => continue, // 非 JSON 行（日志等）跳过
        };
        match resp.get("id").and_then(Value::as_i64) {
            Some(1) => init_resp = Some(resp),
            Some(2) => tools_resp = Some(resp),
            _ => {}
        }
        if init_resp.is_some() && tools_resp.is_some() {
            break;
        }
    }
    // 统一收割子进程（fixture 响应完即退出；异常路径由 EOF/内核回收兜底）。
    let _ = child.wait();
    let init = init_resp.ok_or_else(|| {
        SourceError::new(
            "handshake_failed",
            "initialize response not received before EOF",
        )
    })?;
    let tools = tools_resp.ok_or_else(|| {
        SourceError::new(
            "handshake_failed",
            "tools/list response not received before EOF",
        )
    })?;

    let server_version = init
        .pointer("/result/protocolVersion")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let tools = tools
        .pointer("/result/tools")
        .cloned()
        .unwrap_or_else(|| Value::Array(Vec::new()));
    let tools_sha256 = sha256_hex(tools.to_string().as_bytes());
    Ok(json!({
        "protocol_version": server_version,
        "server_info": init.pointer("/result/serverInfo").cloned().unwrap_or(Value::Null),
        "tools_sha256": tools_sha256,
        "tools": tools,
    }))
}

impl Source for McpServerSource {
    fn source_type(&self) -> &'static str {
        "mcp_server"
    }

    fn verify(&self, manifest: &Value) -> Result<(), SourceError> {
        let source = super::source_of(manifest)?;
        let endpoint = source
            .get("endpoint")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                SourceError::new("endpoint_missing", "mcp_server source requires `endpoint`")
            })?;
        command_of(endpoint)?;
        if source
            .get("protocol_version")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .is_empty()
        {
            return Err(SourceError::new(
                "protocol_version_missing",
                "mcp_server source requires `protocol_version`",
            ));
        }
        Ok(())
    }

    fn fetch(&self, manifest: &Value, target: &Path) -> Result<FetchOutcome, SourceError> {
        let source = super::source_of(manifest)?;
        let endpoint = source
            .get("endpoint")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                SourceError::new("endpoint_missing", "mcp_server source requires `endpoint`")
            })?;
        let command = command_of(endpoint)?;
        let negotiation = handshake(&command)?;
        // 协议版本一致性（§6 协商语义：不一致即拒）。
        let declared = source
            .get("protocol_version")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if !declared.is_empty() && negotiation["protocol_version"] != declared {
            return Err(SourceError::new(
                "protocol_version_mismatch",
                format!(
                    "handshake protocol version {:?} does not match declared {declared:?}",
                    negotiation["protocol_version"]
                ),
            )
            .with_details(negotiation.clone()));
        }
        // 安装目录内容：协商快照 + capability 描述文件（rule 7 entry 指向）。
        std::fs::create_dir_all(target.join("capabilities"))
            .map_err(|e| SourceError::new("io", format!("create_dir {target:?}: {e}")))?;
        std::fs::write(target.join("negotiation.json"), negotiation.to_string())
            .map_err(|e| SourceError::new("io", format!("write negotiation.json: {e}")))?;
        let descriptor = format!(
            "# MCP tools snapshot (tools_sha256: {})\n{}\n",
            negotiation["tools_sha256"],
            serde_json::to_string_pretty(&negotiation["tools"]).unwrap_or_default()
        );
        std::fs::write(target.join("capabilities/mcp-tools.yaml"), descriptor)
            .map_err(|e| SourceError::new("io", format!("write capability descriptor: {e}")))?;
        Ok(FetchOutcome {
            target: target.to_path_buf(),
            negotiation: Some(negotiation),
        })
    }

    fn default_trust(&self) -> TrustLevel {
        TrustLevel::Remote
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_form_unsupported_this_batch() {
        let manifest = serde_json::json!({"source": {"type": "mcp_server", "endpoint": "http://127.0.0.1:9/mcp", "protocol_version": "2024-11-05"}});
        let err = McpServerSource.verify(&manifest).unwrap_err();
        assert_eq!(err.code, "endpoint_form_unsupported");
    }

    #[test]
    fn command_form_accepted() {
        let manifest = serde_json::json!({"source": {"type": "mcp_server", "endpoint": "[\"true\"]", "protocol_version": "2024-11-05"}});
        assert!(McpServerSource.verify(&manifest).is_ok());
    }
}
