//! rp-mcp CLI（P9-1 验收入口，ROADMAP §10.4 命令形）：
//!
//! ~~~text
//! RP_MCP_TOKEN=<t> cargo run --offline -p mcp-server --bin rp-mcp <<'EOF'
//! {"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"t","version":"0"}}}
//! {"jsonrpc":"2.0","id":2,"method":"tools/list","params":{"cursor":""}}
//! EOF
//! ~~~
//!
//! 环境变量（stdio MCP 服务器的凭据通道 = 宿主 spawn 时注入，先例
//! github-mcp-server 的 GITHUB_TOKEN 同款形态）：
//! - RP_MCP_TOKEN  握手令牌（必填，禁止匿名；未配置时所有请求以 -32001
//!   auth_required 拒绝，进程保持读入直至 EOF，便于宿主与自验脚本观测）
//! - RP_DB_PATH    主库路径（默认 migrations/platform.db，按仓库根 CWD 相对）
//! - RP_AUX_DB     aux 只读库（缺省不挂载；handle_tools 同款 aux 增量合并）
//!
//! 纪律：stdout 只允许 JSON 行（NDJSON JSON-RPC 2.0）；日志一律走 stderr；
//! 两个库只读打开（SQLITE_OPEN_READ_ONLY），零 SQL 写；EOF 即退出 0。
//! 本 bin 不含业务逻辑：协议层在 mcp-server lib（tests/mcp_protocol.rs 直测），
//! 目录读面在 catalog.rs（Tool Center 同源）。

use std::io::{BufRead, IsTerminal};
use std::path::PathBuf;
use std::process::exit;

use mcp_server::{LineOutcome, McpServer};

fn main() {
    // 握手令牌：未配置不退出（保持 -32001 应答便于观测「无令牌握手被拒」），
    // 但 stderr 显著告警（rtis_mcp.mjs log 同款纪律：日志不进 stdout）。
    let token = std::env::var("RP_MCP_TOKEN")
        .ok()
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty());
    if token.is_none() {
        eprintln!(
            "[rp-mcp] RP_MCP_TOKEN 未配置：禁止匿名，所有请求将以 -32001 auth_required 拒绝"
        );
    }

    let main_db = std::env::var("RP_DB_PATH")
        .ok()
        .map(|p| PathBuf::from(p.trim()))
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| PathBuf::from("migrations/platform.db"));
    let aux_db = std::env::var("RP_AUX_DB")
        .ok()
        .map(|p| PathBuf::from(p.trim()))
        .filter(|p| !p.as_os_str().is_empty());

    let server = McpServer {
        main_db: main_db.clone(),
        aux_db: aux_db.clone(),
        token,
    };

    eprintln!(
        "[rp-mcp] ready name {} version {} db {} aux {} protocol 2024-11-05 subset",
        mcp_server::SERVER_NAME,
        mcp_server::SERVER_VERSION,
        main_db.display(),
        aux_db
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "-".to_string()),
    );

    // 交互式终端直接启动时给一行用法提示（stderr，不污染 stdout JSON 行）。
    if std::io::stdin().is_terminal() {
        eprintln!(
            "[rp-mcp] stdio NDJSON JSON-RPC 2.0：逐行喂请求，EOF 退出；交互用法见本文件头注释"
        );
    }

    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(e) => {
                eprintln!("[rp-mcp] stdin read error: {e}");
                break;
            }
        };
        match server.handle_line(&line) {
            LineOutcome::Reply(json_line) => {
                if mcp_server::write_line(&mut out, &json_line).is_err() {
                    // 宿主已关闭管道（正常收尾），安静退出。
                    exit(0);
                }
            }
            LineOutcome::Silent | LineOutcome::Skip => {}
        }
    }
    exit(0);
}
