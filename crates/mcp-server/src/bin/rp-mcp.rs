//! rp-mcp CLI（rmcp 迁移整改批次的 stdio 服务入口，ROADMAP §10.4 命令形）：
//!
//! ~~~text
//! RP_MCP_TOKEN=<t> cargo run --offline -p mcp-server --bin rp-mcp <<'EOF'
//! {"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"t","version":"0"}}}
//! {"jsonrpc":"2.0","method":"notifications/initialized"}
//! {"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}
//! {"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"rip.run_tool","arguments":{"tool_id":"rip.http.inspect","input":{"url":"https://example.com/"}}}}
//! EOF
//! ~~~
//!
//! 协议面（JSON-RPC 2.0 解析、initialize 生命周期、协议版本协商、错误信封、
//! 通知静默、stdio I/O）由 rmcp（MCP 官方 Rust SDK）承载，本 bin 只做：
//! 1. RP_MCP_TOKEN 授权边界 fail-closed 门（审计 §4.3 安全边界重定义）：
//!    stdio 子进程的环境变量注入即授权边界，宿主 spawn 时注入非空令牌 =
//!    授权本实例对外服务；缺失或空值一律 exit 2 拒启（stderr 显著告警），
//!    不实现也不发明 MCP 标准外的客户端协议认证字段；
//! 2. 数据面与执行通道环境变量装配（同 P9-1/P9-2 原口径）：
//!    - RP_DB_PATH        主库路径（默认 migrations/platform.db，按仓库根 CWD 相对）
//!    - RP_AUX_DB         aux 只读库（缺省不挂载；handle_tools 同款 aux 增量合并）
//!    - RP_PLUGIN_DB      插件库（P9-4 rip://plugins/catalog 读面；缺省
//!      migrations/dev-p6.db，rp_server RIP_PLUGIN_DB 同默认；缺席 = 诚实空态）
//!    - RP_RUN_DB         P9-2 运行库（可写数据面：tool_run / artifact /
//!      evidence / runtime_node 行；默认 rip-data/tool-runs.db，gitignore
//!      运行时数据；按仓库 migrations 全量迁移，生产 platform.db 零写）
//!    - RP_MIGRATIONS_DIR 迁移目录（默认 migrations）
//!    - RP_MANIFEST_DIRS  Manifest 目录 csv（runtime 块解析来源；默认
//!      manifests/tools,containerfiles/rip-tool-webresearch,containerfiles/rip-tool-coreutils）
//!    - RP_ARTIFACTS_DIR  Artifact 落盘根（默认 rip-data/artifacts）
//!    - RP_RUNTIME_NODE_ID 容器 Runner 节点 id（默认 host-local）
//! 3. rmcp serve（transport-io stdio transport）拉起服务并等连接收尾
//!    （EOF 即退出 0）。
//!
//! 纪律：stdout 只允许协议消息（rmcp transport 承担）；日志一律走 stderr；
//! 目录两库只读打开（SQLITE_OPEN_READ_ONLY）零 SQL 写；运行库为可写数据面
//! （gitignore 运行时数据，生产 platform.db 不在其内）。本 bin 不含业务
//! 逻辑：rmcp 业务 handler 在 service.rs（tests/mcp_protocol.rs 直测），
//! 目录读面在 catalog.rs（Tool Center 同源），执行通道在 dispatch.rs
//! （tool-engine Engine 派发，tests/run_tool_e2e.rs 直测）。

use std::io::IsTerminal;
use std::path::PathBuf;
use std::process::exit;

use rmcp::service::serve_server;

use mcp_server::dispatch::RunChannel;
use mcp_server::McpServer;

fn main() {
    // 授权边界（fail-closed）：RP_MCP_TOKEN 缺失或空值一律拒启（exit 2），
    // 不进入服务循环（宿主环境变量注入即授权，见模块文档第 1 条）。
    let token = std::env::var("RP_MCP_TOKEN")
        .ok()
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty());
    if token.is_none() {
        eprintln!(
            "[rp-mcp] RP_MCP_TOKEN 未配置或为空：授权边界 fail-closed，拒绝启动（宿主须在 spawn 时注入非空令牌；exit 2）"
        );
        exit(2);
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
    // P9-4 插件库（rip://plugins/catalog 读面；RP_PLUGIN_DB=none 显式关闭）。
    let plugin_db = match std::env::var("RP_PLUGIN_DB") {
        Ok(v) if v.trim().eq_ignore_ascii_case("none") => None,
        _ => Some(
            std::env::var("RP_PLUGIN_DB")
                .ok()
                .map(|p| PathBuf::from(p.trim()))
                .filter(|p| !p.as_os_str().is_empty())
                .unwrap_or_else(|| PathBuf::from("migrations/dev-p6.db")),
        ),
    };

    // P9-2 执行通道（RP_RUN_DB 缺省即装配，rip-data 为 gitignore 运行时数据；
    // 显式 RP_RUN_DB=none 可关停执行面，run_tool 返回结构化未配置说明）。
    let run_channel = match std::env::var("RP_RUN_DB") {
        Ok(v) if v.trim().eq_ignore_ascii_case("none") => None,
        _ => {
            let run_db = std::env::var("RP_RUN_DB")
                .ok()
                .map(|p| PathBuf::from(p.trim()))
                .filter(|p| !p.as_os_str().is_empty())
                .unwrap_or_else(|| PathBuf::from("rip-data/tool-runs.db"));
            let migrations_dir = std::env::var("RP_MIGRATIONS_DIR")
                .ok()
                .map(|p| PathBuf::from(p.trim()))
                .filter(|p| !p.as_os_str().is_empty())
                .unwrap_or_else(|| PathBuf::from("migrations"));
            let manifest_dirs: Vec<PathBuf> = std::env::var("RP_MANIFEST_DIRS")
                .ok()
                .map(|v| {
                    v.split(',')
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .map(PathBuf::from)
                        .collect()
                })
                .unwrap_or_else(|| {
                    vec![
                        PathBuf::from("manifests/tools"),
                        PathBuf::from("containerfiles/rip-tool-webresearch"),
                        PathBuf::from("containerfiles/rip-tool-coreutils"),
                    ]
                });
            let artifacts_dir = std::env::var("RP_ARTIFACTS_DIR")
                .ok()
                .map(|p| PathBuf::from(p.trim()))
                .filter(|p| !p.as_os_str().is_empty())
                .unwrap_or_else(|| PathBuf::from("rip-data/artifacts"));
            Some(RunChannel {
                run_db,
                migrations_dir,
                manifest_dirs,
                artifacts_dir,
                workspaces_root: PathBuf::from("rip-data/workspaces"),
                runtime_node_id: std::env::var("RP_RUNTIME_NODE_ID")
                    .ok()
                    .filter(|s| !s.trim().is_empty())
                    .unwrap_or_else(|| "host-local".to_string()),
                working_dir: std::env::current_dir().ok(),
            })
        }
    };

    let server = McpServer {
        main_db: main_db.clone(),
        aux_db: aux_db.clone(),
        plugin_db: plugin_db.clone(),
        run_channel,
    };

    eprintln!(
        "[rp-mcp] ready name {} version {} db {} aux {} run_db {} protocol {}-{} (rmcp stdio)",
        mcp_server::SERVER_NAME,
        mcp_server::SERVER_VERSION,
        main_db.display(),
        aux_db
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "-".to_string()),
        server
            .run_channel
            .as_ref()
            .map(|c| c.run_db.display().to_string())
            .unwrap_or_else(|| "-".to_string()),
        mcp_server::PROTOCOL_VERSION,
        mcp_server::service::SERVER_SUPPORTED_VERSIONS_LAST,
    );

    // 交互式终端直接启动时给一行用法提示（stderr，不污染 stdout 协议消息）。
    if std::io::stdin().is_terminal() {
        eprintln!(
            "[rp-mcp] stdio MCP server（rmcp transport）：逐行喂 JSON-RPC 请求，EOF 退出；交互用法见本文件头注释"
        );
    }

    // rmcp serve：initialize 握手、版本协商与请求派发全部交给 SDK；
    // EOF（连接关闭）后 waiting() 收尾退出 0。
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap_or_else(|e| {
            eprintln!("[rp-mcp] tokio runtime build failed: {e}");
            exit(1);
        });
    runtime.block_on(async move {
        let transport = rmcp::transport::stdio();
        match serve_server(server, transport).await {
            Ok(running) => match running.waiting().await {
                Ok(reason) => {
                    eprintln!("[rp-mcp] service closed: {reason:?}");
                    exit(0);
                }
                Err(e) => {
                    eprintln!("[rp-mcp] service error: {e}");
                    exit(1);
                }
            },
            Err(e) => {
                eprintln!("[rp-mcp] serve failed: {e}");
                exit(1);
            }
        }
    });
}
