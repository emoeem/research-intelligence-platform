//! rp-tool CLI：Tool Registry 校验/注册/查询最小命令面（ROADMAP P6-9，
//! SESSION-HANDOFF §八「添加新工具 = 一条可复制路径」的登记侧载体）+
//! P9-2 run/status/cancel 对外执行子命令（ROADMAP §10.3 P9-2 判据面）。
//!
//! 子命令：
//! - `validate --manifest <path>`：七条静态校验（复用 validate_static +
//!   jsonschema_lite + schemas/tool-manifest/v1.json 冻结契约），不触库；
//! - `register --manifest <path> --db <path>`：migrate + 七条 + rule 2
//!   版本可比 + tool/tool_version 落库（tool-registry 既有 Registry 门面）；
//! - `list --db <path>`：catalog::discover（tool 表全量）；
//! - `get --db <path> <tool_id>`：catalog::query（tool 行 + 版本面）；
//! - `run --db <run_db> --tool <tool_id> --input <json>`（P9-2）：经
//!   rp-mcp 的 rip.run_tool 通道执行（本子命令是最小 MCP 客户端：spawn
//!   rp-mcp 子进程、一次性 stdio NDJSON JSON-RPC 交换、不另建第二套派发
//!   逻辑；执行面 = mcp-server dispatch.rs 同一实现，与 MCP 外部 AI 面同
//!   源）。运行库默认 rip-data/tool-runs.db（可写数据面，gitignore 运行
//!   时数据）；工具须先 `register --db <run_db>` 登记；
//! - `status --db <run_db> --run <run_id>`（P9-2）：经 rip.get_run_status
//!   查询运行状态与 Artifact/Evidence 引用；
//! - `cancel --db <run_db> --run <run_id>`（P9-2）：经 rip.cancel_run 发起
//!   协作式取消（D-TC-04，RUNNING / WAITING_FOR_RUNNER 受理）。
//!
//! run/status/cancel 的 rp-mcp 定位：RP_MCP_BIN 环境变量优先，缺省
//! target/debug/rp-mcp（先例 rip_new_tool.py 的 target/debug 二进制直调）；
//! 令牌为一次性自生成（经环境变量注入子进程，禁止匿名纪律由 MCP 握手
//! 鉴权承载，token 不落盘不回显）。
//!
//! Manifest 支持 YAML 与 JSON（serde_yaml_ng 解析，JSON 是 YAML 1.2 子集）。
//! 输出统一为单行 JSON（机器可读）；exit 0 成功、1 失败、2 用法错误。

use std::io::{BufRead, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};

use serde_json::{json, Value};
use tool_registry::{catalog, Options, Registry};

fn die_json(code: &str, message: &str) -> ! {
    eprintln!("{}", json!({"error": {"code": code, "message": message}}));
    std::process::exit(1);
}

fn usage_text() -> &'static str {
    "usage: rp-tool validate --manifest <path>\n       \
     rp-tool register --manifest <path> --db <path>\n       \
     rp-tool list --db <path>\n       \
     rp-tool get --db <path> <tool_id>\n       \
     rp-tool run --db <run_db> --tool <tool_id> --input '<json>' [--task <id>]\n       \
       [--authorization-ref <ref>] [--version <v>] [--workspace <id>] [--budget '<json>']\n       \
       [--db-main <main_db>] [--manifest-dirs <csv>] [--migrations-dir <dir>]\n       \
     rp-tool status --db <run_db> --run <run_id>\n       \
     rp-tool cancel --db <run_db> --run <run_id>\n\n\
     help: rp-tool --help（stdout, exit 0；run/status/cancel 经 rp-mcp 执行通道，\n\
     RP_MCP_BIN 可指路，缺省 target/debug/rp-mcp）"
}

fn usage() -> ! {
    // P9-3 帮助面统一口径：误用（无子命令/缺参/未知子命令）= usage 到
    // stderr + exit 2；显式 --help/-h = usage 到 stdout + exit 0（见 main）。
    eprintln!("{}", usage_text());
    std::process::exit(2);
}

fn parse_manifest(path: &str) -> Value {
    let text = std::fs::read_to_string(path)
        .unwrap_or_else(|e| die_json("manifest_read_failed", &format!("read {path:?}: {e}")));
    if text.trim_start().starts_with('{') {
        // strict JSON（收尾批）：重复键拒绝，与 YAML 通道（serde_yaml_ng 原生拒绝）对齐。
        tool_registry::strict_json::from_str(&text)
            .unwrap_or_else(|e| die_json("json_syntax", &format!("parse: {e}")))
    } else {
        serde_yaml_ng::from_str(&text)
            .unwrap_or_else(|e| die_json("yaml_syntax", &format!("parse: {e}")))
    }
}

fn open_pool(db: &str) -> database::Pool {
    let pool = database::Pool::open(database::PoolConfig {
        path: database::DbPath::File(PathBuf::from(db)),
        readonly: false,
        max_connections: 2,
        busy_timeout: std::time::Duration::from_secs(5),
    })
    .unwrap_or_else(|e| die_json("db_open_failed", &format!("open {db:?}: {e}")));
    database::migrate(&pool, std::path::Path::new("migrations"))
        .unwrap_or_else(|e| die_json("migration_failed", &format!("migrate: {e}")));
    pool
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!("{}", usage_text());
        std::process::exit(0);
    }
    let Some(command) = args.first() else { usage() };
    let rest = &args[1..];

    let flag_value = |flag: &str| -> Option<String> {
        rest.iter()
            .position(|a| a == flag)
            .and_then(|i| rest.get(i + 1))
            .cloned()
    };

    match command.as_str() {
        "validate" => {
            let Some(manifest_path) = flag_value("--manifest") else {
                usage()
            };
            let manifest = parse_manifest(&manifest_path);
            let vm = tool_registry::validate::validate_static(&manifest, &Options::default())
                .unwrap_or_else(|e| die_json("validation_failed", &format!("{e}")));
            println!(
                "{}",
                json!({
                    "ok": true,
                    "command": "validate",
                    "tool_id": vm.id,
                    "version": vm.version,
                    "content_hash": vm.content_hash,
                })
            );
        }
        "register" => {
            let Some(manifest_path) = flag_value("--manifest") else {
                usage()
            };
            let Some(db) = flag_value("--db") else {
                usage()
            };
            let manifest = parse_manifest(&manifest_path);
            let pool = open_pool(&db);
            let registry = Registry::open(pool).with_options(Options::default());
            let registered = registry
                .register(&manifest)
                .unwrap_or_else(|e| die_json("registration_failed", &format!("{e}")));
            println!(
                "{}",
                json!({
                    "ok": true,
                    "command": "register",
                    "tool_id": registered.tool_id,
                    "version": registered.version,
                    "content_hash": registered.content_hash,
                    "superseded_previous": registered.superseded_previous,
                })
            );
        }
        "list" => {
            let Some(db) = flag_value("--db") else {
                usage()
            };
            let pool = open_pool(&db);
            let tools = catalog::discover(&pool)
                .unwrap_or_else(|e| die_json("discover_failed", &format!("{e}")));
            println!(
                "{}",
                json!({
                    "ok": true,
                    "command": "list",
                    "total": tools.len(),
                    "items": tools,
                })
            );
        }
        "get" => {
            let Some(db) = flag_value("--db") else {
                usage()
            };
            let Some(tool_id) = rest.iter().find(|a| !a.starts_with("--")) else {
                usage()
            };
            let pool = open_pool(&db);
            let detail = catalog::query(&pool, tool_id)
                .unwrap_or_else(|e| die_json("query_failed", &format!("{e}")));
            println!(
                "{}",
                json!({
                    "ok": true,
                    "command": "get",
                    "detail": detail,
                })
            );
        }
        "run" | "status" | "cancel" => run_via_mcp(command.as_str(), &flag_value),
        other => {
            eprintln!("unknown command: {other:?}");
            usage();
        }
    }
}

// ---------------------------------------------------------------------------
// P9-2 run/status/cancel：最小 MCP 客户端（spawn rp-mcp 一次性 stdio 交换）
// ---------------------------------------------------------------------------

/// run/status/cancel 共用：装配 tools/call arguments 后经 rp-mcp 的
/// rip.run_tool / rip.get_run_status / rip.cancel_run 通道执行（与外部 AI
/// MCP 面同一实现，不另建派发逻辑）。isError=true 时 exit 1。
fn run_via_mcp(
    command: &str,
    flag_value: &dyn Fn(&str) -> Option<String>,
) -> ! {
    let capability = match command {
        "run" => "rip.run_tool",
        "status" => "rip.get_run_status",
        _ => "rip.cancel_run",
    };
    let Some(db) = flag_value("--db") else { usage() };
    let arguments = match command {
        "run" => {
            let Some(tool_id) = flag_value("--tool") else { usage() };
            let input_raw = flag_value("--input").unwrap_or_else(|| "{}".to_string());
            let input: Value = serde_json::from_str(&input_raw)
                .unwrap_or_else(|e| die_json("input_json_invalid", &format!("--input parse: {e}")));
            if input.as_object().is_none() {
                die_json("input_json_invalid", "--input must be a JSON object");
            }
            let mut arguments = json!({
                "tool_id": tool_id,
                "input": input,
            });
            if let Some(obj) = arguments.as_object_mut() {
                if let Some(v) = flag_value("--task") {
                    obj.insert("task_id".to_string(), json!(v));
                }
                if let Some(v) = flag_value("--authorization-ref") {
                    obj.insert("authorization_ref".to_string(), json!(v));
                }
                if let Some(v) = flag_value("--version") {
                    obj.insert("tool_version".to_string(), json!(v));
                }
                if let Some(v) = flag_value("--workspace") {
                    obj.insert("workspace_id".to_string(), json!(v));
                }
                if let Some(v) = flag_value("--budget") {
                    let budget: Value = serde_json::from_str(&v).unwrap_or_else(|e| {
                        die_json("budget_json_invalid", &format!("--budget parse: {e}"))
                    });
                    obj.insert("budget".to_string(), budget);
                }
            }
            arguments
        }
        _ => {
            let Some(run_id) = flag_value("--run") else { usage() };
            json!({ "run_id": run_id })
        }
    };

    let request = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": { "name": capability, "arguments": arguments },
    });

    // rp-mcp 定位：RP_MCP_BIN 优先，缺省 target/debug/rp-mcp（rip_new_tool.py
    // 的 target/debug 直调先例）。
    let mcp_bin = std::env::var("RP_MCP_BIN")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "target/debug/rp-mcp".to_string());
    if !std::path::Path::new(&mcp_bin).exists() {
        die_json(
            "mcp_bin_missing",
            &format!(
                "rp-mcp 不在 {mcp_bin:?}；先 cargo build --offline -p mcp-server --bin rp-mcp（或用 RP_MCP_BIN 指路）"
            ),
        );
    }

    // 一次性握手令牌（禁止匿名纪律由 MCP 握手鉴权承载；token 只经环境变量
    // 进子进程，不落盘不回显）。
    let token = ephemeral_token();
    let mut cmd = Command::new(&mcp_bin);
    cmd.env("RP_MCP_TOKEN", token)
        .env("RP_RUN_DB", &db)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    if let Some(main_db) = flag_value("--db-main") {
        cmd.env("RP_DB_PATH", main_db);
    }
    if let Some(dirs) = flag_value("--manifest-dirs") {
        cmd.env("RP_MANIFEST_DIRS", dirs);
    }
    if let Some(dir) = flag_value("--migrations-dir") {
        cmd.env("RP_MIGRATIONS_DIR", dir);
    }
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => die_json("mcp_spawn_failed", &format!("spawn {mcp_bin:?}: {e}")),
    };
    let Some(mut stdin) = child.stdin.take() else {
        die_json("mcp_spawn_failed", "rp-mcp stdin unavailable");
    };
    // rmcp 迁移批次（2026-11-14）：rp-mcp 协议层改由 rmcp SDK 承载，其
    // initialize 握手状态机要求首条请求必须为 initialize（否则连接以
    // ExpectedInitializeRequest 断开），P9-2 原「无状态直呼 tools/call」形态
    // 随手写协议层退役。本客户端补齐标准握手：initialize（id=0，响应被下方
    // id=1 读取器自动跳过）→ notifications/initialized（通知无响应）→ 业务
    // 请求（id=1）。CLI 输出形状不变（仍取 id=1 的 tools/call / 状态响应）。
    let handshake = json!({
        "jsonrpc": "2.0",
        "id": 0,
        "method": "initialize",
        "params": {
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": { "name": "rp-tool", "version": env!("CARGO_PKG_VERSION") }
        }
    });
    let initialized = json!({ "jsonrpc": "2.0", "method": "notifications/initialized" });
    if let Err(e) = writeln!(stdin, "{handshake}") {
        die_json("mcp_write_failed", &format!("write initialize: {e}"));
    }
    if let Err(e) = writeln!(stdin, "{initialized}") {
        die_json("mcp_write_failed", &format!("write notifications/initialized: {e}"));
    }
    if let Err(e) = writeln!(stdin, "{request}") {
        die_json("mcp_write_failed", &format!("write request: {e}"));
    }
    // 关 stdin 触发 EOF 收尾（rmcp transport 读到 EOF 收尾，进程退出 0）。
    drop(stdin);

    let Some(stdout) = child.stdout.take() else {
        die_json("mcp_spawn_failed", "rp-mcp stdout unavailable");
    };
    let mut reply: Option<Value> = None;
    for line in std::io::BufReader::new(stdout).lines() {
        let Ok(line) = line else { break };
        let Ok(msg) = serde_json::from_str::<Value>(&line) else { continue };
        if msg.get("id").and_then(Value::as_i64) == Some(1) {
            reply = Some(msg);
            break;
        }
    }
    let _ = child.wait();
    let Some(reply) = reply else {
        die_json(
            "mcp_no_reply",
            "rp-mcp 未回 id=1 响应（stderr 已静默；可用 RP_MCP_BIN 直跑观测）",
        );
    };
    if let Some(err) = reply.get("error") {
        eprintln!("{}", json!({"error": err}));
        std::process::exit(1);
    }
    let result = reply.get("result").cloned().unwrap_or(json!({}));
    let is_error = result.get("isError").and_then(Value::as_bool).unwrap_or(false);
    let text = result
        .get("content")
        .and_then(|c| c.get(0))
        .and_then(|c| c.get("text"))
        .and_then(Value::as_str)
        .unwrap_or("{}")
        .to_string();
    println!("{text}");
    if is_error {
        std::process::exit(1);
    }
    std::process::exit(0);
}

/// 一次性令牌（nanos + pid 经 sha256 折叠；tool-registry 既有 hash 面）。
fn ephemeral_token() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let material = format!("rp-tool:{}:{}:{}", nanos, std::process::id(), "rp-mcp-ephemeral");
    tool_registry::hash::sha256_hex(material.as_bytes())
}
