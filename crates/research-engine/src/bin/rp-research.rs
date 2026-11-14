//! rp-research CLI（P7-6 契约验收入口）：
//!
//! ~~~text
//! cargo run --offline -p research-engine --bin rp-research -- list
//! cargo run --offline -p research-engine --bin rp-research -- run <capability_id> \
//!   --input '<json>' [--run-id <id>] [--trace-id <id>] [--task-id <id>] \
//!   [--capability-version <v>] [--out <path>]
//! ~~~
//!
//! run 流程（契约验收入口的四道门）：装配请求信封 → request-v1 Schema 校验
//! （违规 exit 2）→ [`research_engine::contract::execute_request`] 派发 →
//! result-v1 Schema 符合性自检（内部不变量，违规 exit 3）→ 信封单行 JSON 进
//! stdout，SUCCEEDED 退出 0，FAILED 退出 1。
//!
//! correlation 缺省面：--run-id / --trace-id 缺省时按输入确定性派生（同输入
//! 恒同 ID，mock 可重放；真实运行由调用方传 agent_run/tool_run 的实际 ID）。
//! list 先做 14 份内嵌 Schema 的 meta-schema 全量门再列词表。

use research_engine::contract::{
    check_all_schema_documents, digest16, execute_request, validate_against, CapabilityId,
    SchemaKind, CAPABILITY_VERSION, PROTOCOL_VERSION, SCHEMA_DIR,
};
use std::path::PathBuf;
use std::process::exit;

fn main() {
    exit(run());
}

fn usage_text() -> &'static str {
    "usage: rp-research <sub> [args]\n\
     \n\
     subcommands:\n\
     \x20 list\n\
     \x20   列七能力词表（先过 14 份内嵌 Schema 的 meta-schema 全量门）\n\
     \x20 run <capability_id> --input '<json>' [--run-id <id>] [--trace-id <id>]\n\
     \x20   [--task-id <id>] [--capability-version <v>] [--out <path>]\n\
     \x20   装配请求信封，request-v1 校验后派发，result-v1 自检，单行 JSON 进 stdout\n\
     \x20   exit: SUCCEEDED=0, FAILED=1, 用法/契约违规=2, 内部不变量违规=3\n\
     \n\
     capability_id 词表: research.planner | research.source-discovery |\n\
     \x20 research.evidence-extraction | research.verification | research.synthesis |\n\
     \x20 research.citation | research.evaluator\n\
     \n\
     help: rp-research --help（stdout, exit 0）"
}

fn usage() -> i32 {
    // P9-3 帮助面统一口径：误用 = usage 到 stderr + exit 2；
    // 显式 --help/-h = usage 到 stdout + exit 0（见 run()）。
    eprintln!("{}", usage_text());
    2
}

fn run() -> i32 {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!("{}", usage_text());
        return 0;
    }
    match args.first().map(String::as_str) {
        Some("list") => cmd_list(),
        Some("run") => {
            if args.len() < 2 {
                usage()
            } else {
                cmd_run(&args[1..])
            }
        }
        _ => usage(),
    }
}

/// list：14 份内嵌 Schema 全量门 + 七能力词表输出（逐行 JSON）。
fn cmd_list() -> i32 {
    if let Err(reason) = check_all_schema_documents() {
        eprintln!("schema gate failed: {reason}");
        return 3;
    }
    for capability in CapabilityId::ALL {
        let line = serde_json::json!({
            "capability_id": capability.as_str(),
            "capability_version": CAPABILITY_VERSION,
            "purpose": capability.purpose(),
            "request_schema": format!("{SCHEMA_DIR}/{}", capability.request_schema_file()),
            "result_schema": format!("{SCHEMA_DIR}/{}", capability.result_schema_file()),
        });
        println!("{line}");
    }
    println!(
        "{}",
        serde_json::json!({
            "total": CapabilityId::ALL.len(),
            "capability_version": CAPABILITY_VERSION,
            "schemas_checked": CapabilityId::ALL.len() * 2,
            "schema_check": "ok",
        })
    );
    0
}

/// run：单能力契约执行四道门。
fn cmd_run(args: &[String]) -> i32 {
    let capability_raw = match args.first() {
        Some(value) if !value.starts_with("--") => value.clone(),
        _ => return usage(),
    };
    let capability = match CapabilityId::try_from(capability_raw.as_str()) {
        Ok(capability) => capability,
        Err(reason) => {
            eprintln!("{reason}");
            return usage();
        }
    };
    let mut input_raw: Option<String> = None;
    let mut run_id: Option<String> = None;
    let mut trace_id: Option<String> = None;
    let mut task_id: Option<String> = None;
    let mut capability_version = CAPABILITY_VERSION.to_string();
    let mut out: Option<PathBuf> = None;
    let mut iter = args.iter().skip(1);
    while let Some(flag) = iter.next() {
        let mut value = || iter.next().cloned().unwrap_or_default();
        match flag.as_str() {
            "--input" => input_raw = Some(value()),
            "--run-id" => run_id = Some(value()),
            "--trace-id" => trace_id = Some(value()),
            "--task-id" => task_id = Some(value()),
            "--capability-version" => capability_version = value(),
            "--out" => out = Some(PathBuf::from(value())),
            _ => {
                eprintln!("unknown flag: {flag}");
                return usage();
            }
        }
    }
    let input_raw = match input_raw {
        Some(raw) => raw,
        None => {
            eprintln!("run requires --input '<json>'");
            return usage();
        }
    };
    let input: serde_json::Value = match serde_json::from_str(&input_raw) {
        Ok(value) => value,
        Err(reason) => {
            eprintln!("--input is not valid JSON: {reason}");
            return 2;
        }
    };

    // correlation 缺省面：同输入恒同 ID（mock 可重放）。
    let id_seed = [capability.as_str(), &input_raw];
    let run_id = run_id.unwrap_or_else(|| format!("rn-{}", digest16(&["rp-research-run", &id_seed.join("\u{1f}")])));
    let trace_id = trace_id
        .unwrap_or_else(|| format!("tr-{}", digest16(&["rp-research-trace", &id_seed.join("\u{1f}")])));
    let mut correlation = serde_json::json!({
        "run_id": run_id,
        "trace_id": trace_id,
    });
    if let Some(task) = task_id {
        correlation["task_id"] = serde_json::Value::String(task);
    }
    let request = serde_json::json!({
        "protocol_version": PROTOCOL_VERSION,
        "capability_id": capability.as_str(),
        "capability_version": capability_version,
        "correlation": correlation,
        "input": input,
    });

    // 门 1：request-v1 Schema 校验。
    if let Err(reason) = validate_against(capability, SchemaKind::Request, &request) {
        eprintln!("request contract violation: {reason}");
        return 2;
    }
    // 门 2：统一派发（typed 入参解析 + 纯函数执行）。
    let envelope = match execute_request(capability, &request) {
        Ok(envelope) => envelope,
        Err(reason) => {
            eprintln!("request rejected: {reason}");
            return 2;
        }
    };
    // 门 3：result-v1 符合性自检（内部不变量：本 crate 产出的信封必须过自己的契约）。
    if let Err(reason) = validate_against(capability, SchemaKind::Result, &envelope) {
        eprintln!("internal invariant violated (envelope fails result schema): {reason}");
        return 3;
    }
    let line = serde_json::to_string(&envelope).expect("envelope serialization cannot fail");
    if let Some(path) = out {
        if let Err(reason) = std::fs::write(&path, format!("{line}\n")) {
            eprintln!("failed to write --out {}: {reason}", path.display());
            return 2;
        }
    }
    println!("{line}");
    // 门 4：退出码按终态（SUCCEEDED=0 / FAILED=1）。
    match envelope.get("status").and_then(|v| v.as_str()) {
        Some("SUCCEEDED") => 0,
        _ => 1,
    }
}
