//! rp-tool-health CLI：python_worker Worker Protocol v1 健康检查最小命令面
//! （ROADMAP P6-9；SESSION-HANDOFF §八「健康检查」载体）。
//!
//! 子命令：
//! - `ping --manifest <path>`：读 Tool Manifest → 提取 runtime 块 → spawn
//!   worker 子进程 → 发送 initialize（协议版本 "1"）→ 校验 worker_name 一致
//!   → 发送 shutdown → 退出。输出机器可读 JSON。
//!
//! 协议口径：Worker Protocol v1（schemas/worker-protocol/v1.json，
//! stdio NDJSON JSON-RPC 2.0）与 tool-engine adapter/python_worker.rs 的
//! Runtime Adapter 同一套 framing 与方法名（TOOL-CONTRACT §2.2）。
//! 输出统一为单行 JSON；exit 0 健康、1 不健康、2 用法错误。

use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};
use std::time::Duration;

fn die_json(code: &str, message: &str) -> ! {
    eprintln!(
        "{}",
        json!({"ok": false, "error": {"code": code, "message": message}})
    );
    std::process::exit(1);
}

fn usage_text() -> &'static str {
    "usage: rp-tool-health ping --manifest <path> [--timeout-ms <n>]\n\n\
     commands:\n\
     \x20 ping  Worker Protocol v1 握手探测（spawn worker 一次 initialize/ping 交换）\n\n\
     exit: ok=0, 探测失败=1, 用法错误=2\n\
     help: rp-tool-health --help（stdout, exit 0）"
}

fn usage() -> ! {
    // P9-3 帮助面统一口径：误用（无命令/未知命令/缺参）= usage 到 stderr +
    // exit 2；显式 --help/-h = usage 到 stdout + exit 0（见 main）。
    eprintln!("{}", usage_text());
    std::process::exit(2);
}

fn parse_manifest(path: &str) -> Value {
    let text = std::fs::read_to_string(path)
        .unwrap_or_else(|e| die_json("manifest_read_failed", &format!("read {path:?}: {e}")));
    if text.trim_start().starts_with('{') {
        serde_json::from_str(&text)
            .unwrap_or_else(|e| die_json("json_syntax", &format!("parse: {e}")))
    } else {
        serde_yaml_ng::from_str(&text)
            .unwrap_or_else(|e| die_json("yaml_syntax", &format!("parse: {e}")))
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!("{}", usage_text());
        std::process::exit(0);
    }
    let Some(command) = args.first() else { usage() };
    if command != "ping" {
        eprintln!("unknown command: {command:?}");
        usage();
    }
    let mut manifest_path = None;
    let mut timeout_ms = 15_000u64;
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--manifest" => {
                manifest_path = args.get(i + 1).cloned();
                i += 2;
            }
            "--timeout-ms" => {
                timeout_ms = args
                    .get(i + 1)
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(15_000);
                i += 2;
            }
            _ => i += 1,
        }
    }
    let Some(manifest_path) = manifest_path else {
        usage()
    };

    let manifest = parse_manifest(&manifest_path);
    let runtime_type = manifest
        .pointer("/runtime/type")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if runtime_type != "python_worker" {
        die_json(
            "runtime_type_unsupported",
            &format!("health ping only supports python_worker (got {runtime_type:?})"),
        );
    }
    let worker_name = manifest
        .pointer("/runtime/worker_name")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let command: Vec<String> = manifest
        .pointer("/runtime/command")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(Value::as_str)
                .map(String::from)
                .collect()
        })
        .unwrap_or_default();
    if command.is_empty() {
        die_json(
            "runtime_command_missing",
            "runtime.command is empty or missing",
        );
    }

    let (program, cargs) = command.split_first().unwrap();
    let mut child = Command::new(program)
        .args(cargs)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap_or_else(|e| {
            die_json(
                "spawn_failed",
                &format!("spawn {program:?}: {e} (worker binary/command not found?)"),
            )
        });

    let mut stdin = child
        .stdin
        .take()
        .unwrap_or_else(|| die_json("spawn_failed", "child stdin unavailable"));
    let mut stdout = BufReader::new(
        child
            .stdout
            .take()
            .unwrap_or_else(|| die_json("spawn_failed", "child stdout unavailable")),
    );

    // initialize
    let init_req = json!({
        "jsonrpc": "2.0", "id": 1, "method": "initialize",
        "params": {"protocol_version": "1"}
    });
    writeln!(stdin, "{init_req}")
        .unwrap_or_else(|e| die_json("protocol_io", &format!("write initialize: {e}")));

    let mut init_resp: Option<Value> = None;
    let mut line = String::new();
    let deadline = std::time::Instant::now() + Duration::from_millis(timeout_ms);
    loop {
        if std::time::Instant::now() > deadline {
            let _ = child.kill();
            let _ = child.wait();
            die_json(
                "health_timeout",
                &format!("initialize response not received within {timeout_ms}ms"),
            );
        }
        line.clear();
        let n = stdout
            .read_line(&mut line)
            .unwrap_or_else(|e| die_json("protocol_io", &format!("read: {e}")));
        if n == 0 {
            break; // EOF
        }
        let resp: Value = match serde_json::from_str(line.trim()) {
            Ok(v) => v,
            Err(_) => continue,
        };
        if resp.get("id").and_then(Value::as_i64) == Some(1) {
            init_resp = Some(resp);
            break;
        }
    }

    let init = init_resp.unwrap_or_else(|| {
        let _ = child.kill();
        let _ = child.wait();
        die_json(
            "handshake_failed",
            "initialize response not received before EOF",
        )
    });

    // 校验 worker_name（TOOL-CONTRACT §2.2：worker_name 须与响应一致）
    let result = &init["result"];
    let actual_name = result
        .get("worker_name")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let protocol_version = result
        .get("protocol_version")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let worker_version = result
        .get("worker_version")
        .and_then(Value::as_str)
        .unwrap_or_default();

    if actual_name != worker_name {
        let _ = child.kill();
        let _ = child.wait();
        die_json(
            "worker_name_mismatch",
            &format!(
                "manifest worker_name {worker_name:?} vs initialize response {actual_name:?} (TOOL-CONTRACT 2.2)"
            ),
        );
    }
    if protocol_version != "1" {
        let _ = child.kill();
        let _ = child.wait();
        die_json(
            "protocol_version_mismatch",
            &format!("worker protocol version {protocol_version:?} != \"1\""),
        );
    }

    // shutdown
    let shutdown_req = json!({"jsonrpc": "2.0", "id": 2, "method": "shutdown"});
    let _ = writeln!(stdin, "{shutdown_req}");
    drop(stdin);
    let _ = child.wait();

    println!(
        "{}",
        json!({
            "ok": true,
            "command": "ping",
            "worker_name": actual_name,
            "worker_version": worker_version,
            "protocol_version": protocol_version,
        })
    );
}
