//! rp-collab CLI（MA-3 协作协议面：TaskClaim 认领队列 + ma_message 邮箱，
//! ROADMAP §9.4 命令形家族惯例）：
//!
//! ~~~text
//! rp-collab claim-create   --db PATH --caller-id CALLER --kind agent_subtask|review
//!                          --title T --payload '<json>' [--run-id WFR] [--node-id N]
//!                          [--agent-id A] [--depends-on TCL]
//! rp-collab claim-list     --db PATH [--limit N]        # ready_list（依赖门过滤读面）
//! rp-collab claim-claim    --db PATH --caller-id CALLER --claim-id TCL   # D2 CAS 原子领取
//! rp-collab claim-complete --db PATH --caller-id CALLER --claim-id TCL --result '<json>'
//! rp-collab claim-fail     --db PATH --caller-id CALLER --claim-id TCL --error '<json>'
//! rp-collab claim-transfer --db PATH --caller-id CALLER --claim-id TCL   # R3 转派（新 attempt）
//! rp-collab claim-show     --db PATH --claim-id TCL
//! rp-collab msg-send       --db PATH --from CALLER --to CALLER --payload '<json>'
//!                          [--claim-id TCL] [--run-id WFR]
//! rp-collab msg-fetch      --db PATH --caller-id CALLER [--limit N]   # 拉模型（投递分记）
//! rp-collab msg-read       --db PATH --caller-id CALLER --message-id MAM
//! rp-collab msg-sweep      --db PATH [--limit N]        # 未投递清单/计数（MA-4 SSE 输入源）
//! ~~~
//!
//! - 写权面：--caller-id 必填署名（claim-create/claim-claim/complete/fail/
//!   transfer 与 msg-send 的 from/to——caller 是路由与认领主语，缺署名
//!   fail-closed exit 2）。
//! - exit 约定：ok = 0；语义失败（CAS 败方 / NotClaimer / 依赖门 /
//!   NotFound / 环检测 / 校验拒）= 1（错误 JSON 照实输出）；用法错误
//!   （缺旗标 / 词表外 kind / 库守卫）= 2。
//! - 库守卫：--db 拒绝 platform*.db（rp-workflow guard_db_path 同款，
//!   ROADMAP §13 生产库零触碰）；写路径自动 database::migrate（幂等
//!   no-op，rp-admin 写路径 CLI 同款惯例）。
//!
//! D7：本 CLI 是 claim 队列的创建面（编排方显式登记），workflow 执行器
//! dispatch_node 零改动（零引擎自动建 claim）。

use std::path::{Path, PathBuf};
use std::process::exit;

use database::{migrate, DbPath, Pool, PoolConfig};
use workflow_engine::collab::{
    ClaimCreateInput, ClaimKind, ClaimStore, CollabError, MessageSendInput, MessageStore,
};

fn main() {
    exit(run());
}

fn usage_text() -> &'static str {
    "usage: rp-collab <sub> [args]\n\
     \n\
     subcommands:\n\
     \x20 claim-create   --db PATH --caller-id CALLER --kind agent_subtask|review --title T\n\
     \x20                --payload '<json>' [--run-id WFR] [--node-id N] [--agent-id A]\n\
     \x20                [--depends-on TCL]       建 claim（claimable，attempt=1，环检测 fail-closed）\n\
     \x20 claim-list     --db PATH [--limit N]    ready_list（依赖门过滤读面）\n\
     \x20 claim-claim    --db PATH --caller-id CALLER --claim-id TCL    D2 单语句 CAS 原子领取\n\
     \x20 claim-complete --db PATH --caller-id CALLER --claim-id TCL --result '<json>'\n\
     \x20 claim-fail     --db PATH --caller-id CALLER --claim-id TCL --error '<json>'\n\
     \x20 claim-transfer --db PATH --caller-id CALLER --claim-id TCL    转派（新 attempt 行）\n\
     \x20 claim-show     --db PATH --claim-id TCL\n\
     \x20 msg-send       --db PATH --from CALLER --to CALLER --payload '<json>'\n\
     \x20                [--claim-id TCL] [--run-id WFR]\n\
     \x20 msg-fetch      --db PATH --caller-id CALLER [--limit N]   拉模型收件（投递分记）\n\
     \x20 msg-read       --db PATH --caller-id CALLER --message-id MAM\n\
     \x20 msg-sweep      --db PATH [--limit N]    未投递清单/计数（MA-4 SSE 输入源）\n\
     \n\
     exit: ok=0, 语义失败（CAS 败方/NotClaimer/依赖门/NotFound/环检测/校验拒）=1, 用法错误=2\n\
     默认无 --db（必填）；platform*.db 一律拒绝\n\
     \n\
     help: rp-collab --help（stdout, exit 0）"
}

fn usage() -> i32 {
    eprintln!("{}", usage_text());
    2
}

fn run() -> i32 {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!("{}", usage_text());
        return 0;
    }
    let Some(sub) = args.first().cloned() else {
        return usage();
    };
    match sub.as_str() {
        "claim-create" | "claim-list" | "claim-claim" | "claim-complete" | "claim-fail"
        | "claim-transfer" | "claim-show" | "msg-send" | "msg-fetch" | "msg-read"
        | "msg-sweep" => cmd_dispatch(&sub, &args[1..]),
        _ => usage(),
    }
}

/// 生产库守卫（rp-workflow guard_db_path 同款）。
fn guard_db_path(db: &Path) {
    let name = db
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    if name.starts_with("platform") {
        eprintln!(
            "{}",
            serde_json::json!({"code": "production_db_refused",
                   "message": "--db must be a dev copy or temp file; platform*.db is forbidden (ROADMAP §13)"})
        );
        exit(1);
    }
}

/// 旗标解析（--key value 对；未知旗标拒绝 = fail-closed）。
fn parse_flags(args: &[String], allowed: &[&str]) -> Result<std::collections::HashMap<String, String>, String> {
    let mut flags = std::collections::HashMap::new();
    let mut iter = args.iter();
    while let Some(flag) = iter.next() {
        let Some(name) = flag.strip_prefix("--") else {
            return Err(format!("unexpected positional argument {flag:?}"));
        };
        if !allowed.contains(&name) {
            return Err(format!("unknown flag --{name}"));
        }
        let Some(value) = iter.next() else {
            return Err(format!("flag --{name} requires a value"));
        };
        flags.insert(name.to_string(), value.clone());
    }
    Ok(flags)
}

fn require<'a>(flags: &'a std::collections::HashMap<String, String>, name: &str) -> Result<&'a str, String> {
    flags
        .get(name)
        .map(String::as_str)
        .filter(|v| !v.trim().is_empty())
        .ok_or_else(|| format!("missing required flag --{name}"))
}

fn opt<'a>(flags: &'a std::collections::HashMap<String, String>, name: &str) -> Option<&'a str> {
    flags.get(name).map(String::as_str).filter(|v| !v.trim().is_empty())
}

fn opt_usize(flags: &std::collections::HashMap<String, String>, name: &str) -> Result<Option<usize>, String> {
    match opt(flags, name) {
        None => Ok(None),
        Some(raw) => raw
            .parse::<usize>()
            .map(Some)
            .map_err(|_| format!("--{name} must be a positive integer, got {raw:?}")),
    }
}

/// 打开可写库并跑迁移（001..029 幂等）；生产库守卫前置。
fn open_pool(db: &str) -> Pool {
    let path = PathBuf::from(db);
    guard_db_path(&path);
    let pool = Pool::open(PoolConfig {
        path: DbPath::File(path.clone()),
        readonly: false,
        max_connections: 2,
        busy_timeout: std::time::Duration::from_secs(5),
    })
    .unwrap_or_else(|e| {
        eprintln!("open {db}: {e}");
        exit(1);
    });
    let applied = migrate(&pool, Path::new("migrations")).unwrap_or_else(|e| {
        eprintln!("migrate {db}: {e}");
        exit(1);
    });
    if !applied.is_empty() {
        eprintln!(
            "{}",
            serde_json::json!({"code": "migrations_applied",
                   "count": applied.len(),
                   "latest": applied.last().map(|m| m.version)})
        );
    }
    pool
}

/// CollabError → exit 码（0 ok / 1 语义失败 / 2 用法错误；本映射只出 1）。
fn fail(err: CollabError) -> i32 {
    println!(
        "{}",
        serde_json::json!({"error": {"code": collab_code(&err), "message": err.to_string()}})
    );
    1
}

fn collab_code(err: &CollabError) -> &'static str {
    match err {
        CollabError::NotFound(_) => "not_found",
        CollabError::NotClaimable { .. } => "not_claimable",
        CollabError::DependencyNotCompleted(_) => "dependency_not_completed",
        CollabError::NotClaimer(_) => "not_claimer",
        CollabError::CycleDetected(_) => "cycle_detected",
        CollabError::InvalidInput(_) => "invalid_input",
        CollabError::Storage(_) => "storage",
    }
}

fn claim_row_json(row: &workflow_engine::collab::ClaimRow) -> serde_json::Value {
    serde_json::json!({
        "claim_id": row.claim_id,
        "kind": row.kind.as_str(),
        "title": row.title,
        "payload_json": row.payload_json,
        "run_id": row.run_id,
        "node_id": row.node_id,
        "agent_id": row.agent_id,
        "depends_on_claim_id": row.depends_on_claim_id,
        "state": row.state.as_str(),
        "claimed_by": row.claimed_by,
        "attempt": row.attempt,
        "result_json": row.result_json,
        "error_json": row.error_json,
        "created_by": row.created_by,
        "created_at": row.created_at,
        "claimed_at": row.claimed_at,
        "finished_at": row.finished_at,
        "updated_at": row.updated_at,
    })
}

fn message_row_json(row: &workflow_engine::collab::MessageRow) -> serde_json::Value {
    serde_json::json!({
        "message_id": row.message_id,
        "from_caller": row.from_caller,
        "to_caller": row.to_caller,
        "claim_id": row.claim_id,
        "run_id": row.run_id,
        "payload_json": row.payload_json,
        "delivered_at": row.delivered_at,
        "read_at": row.read_at,
        "created_at": row.created_at,
        "updated_at": row.updated_at,
    })
}

fn cmd_dispatch(sub: &str, args: &[String]) -> i32 {
    let allowed: &[&str] = match sub {
        "claim-create" => &[
            "db", "caller-id", "kind", "title", "payload", "run-id", "node-id", "agent-id",
            "depends-on",
        ],
        "claim-list" | "msg-sweep" => &["db", "limit"],
        "claim-claim" | "claim-complete" | "claim-fail" | "claim-transfer" => {
            &["db", "caller-id", "claim-id", "result", "error"]
        }
        "claim-show" => &["db", "claim-id"],
        "msg-send" => &["db", "from", "to", "payload", "claim-id", "run-id"],
        "msg-fetch" => &["db", "caller-id", "limit"],
        "msg-read" => &["db", "caller-id", "message-id"],
        _ => &[],
    };
    let flags = match parse_flags(args, allowed) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("rp-collab {sub}: {e}");
            return usage();
        }
    };
    let Some(db) = opt(&flags, "db") else {
        eprintln!("rp-collab {sub}: missing required flag --db");
        return usage();
    };
    let pool = open_pool(db);
    match sub {
        "claim-create" => {
            let caller = match require(&flags, "caller-id") {
                Ok(v) => v.to_string(),
                Err(e) => return usage_error(sub, &e),
            };
            let kind = match require(&flags, "kind").map(ClaimKind::parse) {
                Ok(Ok(k)) => k,
                Ok(Err(e)) => return usage_error(sub, &e.to_string()),
                Err(e) => return usage_error(sub, &e),
            };
            let input = ClaimCreateInput {
                kind,
                title: match require(&flags, "title") {
                    Ok(v) => v.to_string(),
                    Err(e) => return usage_error(sub, &e),
                },
                payload_json: match require(&flags, "payload") {
                    Ok(v) => v.to_string(),
                    Err(e) => return usage_error(sub, &e),
                },
                run_id: opt(&flags, "run-id").map(str::to_string),
                node_id: opt(&flags, "node-id").map(str::to_string),
                agent_id: opt(&flags, "agent-id").map(str::to_string),
                depends_on_claim_id: opt(&flags, "depends-on").map(str::to_string),
                created_by: caller,
            };
            match ClaimStore::new(pool).create(&input) {
                Ok(row) => print_result(claim_row_json(&row)),
                Err(e) => fail(e),
            }
        }
        "claim-list" => {
            let limit = match opt_usize(&flags, "limit") {
                Ok(v) => v.unwrap_or(50),
                Err(e) => return usage_error(sub, &e),
            };
            match ClaimStore::new(pool).ready_list(limit) {
                Ok(rows) => print_result(serde_json::json!({
                    "total": rows.len(),
                    "items": rows.iter().map(claim_row_json).collect::<Vec<_>>(),
                })),
                Err(e) => fail(e),
            }
        }
        "claim-claim" => {
            let (caller, claim_id) = match (require(&flags, "caller-id"), require(&flags, "claim-id")) {
                (Ok(a), Ok(b)) => (a.to_string(), b.to_string()),
                (Err(e), _) | (_, Err(e)) => return usage_error(sub, &e),
            };
            match ClaimStore::new(pool).claim(&claim_id, &caller) {
                Ok(row) => print_result(claim_row_json(&row)),
                Err(e) => fail(e),
            }
        }
        "claim-complete" | "claim-fail" => {
            let caller = match require(&flags, "caller-id") {
                Ok(v) => v.to_string(),
                Err(e) => return usage_error(sub, &e),
            };
            let claim_id = match require(&flags, "claim-id") {
                Ok(v) => v.to_string(),
                Err(e) => return usage_error(sub, &e),
            };
            let store = ClaimStore::new(pool);
            let result = if sub == "claim-complete" {
                let payload = match require(&flags, "result") {
                    Ok(v) => v.to_string(),
                    Err(e) => return usage_error(sub, &e),
                };
                store.complete(&claim_id, &caller, &payload)
            } else {
                let payload = match require(&flags, "error") {
                    Ok(v) => v.to_string(),
                    Err(e) => return usage_error(sub, &e),
                };
                store.fail(&claim_id, &caller, &payload)
            };
            match result {
                Ok(row) => print_result(claim_row_json(&row)),
                Err(e) => fail(e),
            }
        }
        "claim-transfer" => {
            let (caller, claim_id) = match (require(&flags, "caller-id"), require(&flags, "claim-id")) {
                (Ok(a), Ok(b)) => (a.to_string(), b.to_string()),
                (Err(e), _) | (_, Err(e)) => return usage_error(sub, &e),
            };
            match ClaimStore::new(pool).transfer(&claim_id, &caller) {
                Ok(row) => print_result(serde_json::json!({
                    "new_claim": claim_row_json(&row),
                })),
                Err(e) => fail(e),
            }
        }
        "claim-show" => {
            let claim_id = match require(&flags, "claim-id") {
                Ok(v) => v.to_string(),
                Err(e) => return usage_error(sub, &e),
            };
            match ClaimStore::new(pool).show(&claim_id) {
                Ok(row) => print_result(claim_row_json(&row)),
                Err(e) => fail(e),
            }
        }
        "msg-send" => {
            let (from, to) = match (require(&flags, "from"), require(&flags, "to")) {
                (Ok(a), Ok(b)) => (a.to_string(), b.to_string()),
                (Err(e), _) | (_, Err(e)) => return usage_error(sub, &e),
            };
            let input = MessageSendInput {
                from_caller: from,
                to_caller: to,
                payload_json: match require(&flags, "payload") {
                    Ok(v) => v.to_string(),
                    Err(e) => return usage_error(sub, &e),
                },
                claim_id: opt(&flags, "claim-id").map(str::to_string),
                run_id: opt(&flags, "run-id").map(str::to_string),
            };
            match MessageStore::new(pool).send(&input) {
                Ok(row) => print_result(message_row_json(&row)),
                Err(e) => fail(e),
            }
        }
        "msg-fetch" => {
            let caller = match require(&flags, "caller-id") {
                Ok(v) => v.to_string(),
                Err(e) => return usage_error(sub, &e),
            };
            let limit = match opt_usize(&flags, "limit") {
                Ok(v) => v.unwrap_or(50),
                Err(e) => return usage_error(sub, &e),
            };
            match MessageStore::new(pool).fetch(&caller, limit) {
                Ok(rows) => print_result(serde_json::json!({
                    "total": rows.len(),
                    "items": rows.iter().map(message_row_json).collect::<Vec<_>>(),
                })),
                Err(e) => fail(e),
            }
        }
        "msg-read" => {
            let (caller, message_id) = match (require(&flags, "caller-id"), require(&flags, "message-id")) {
                (Ok(a), Ok(b)) => (a.to_string(), b.to_string()),
                (Err(e), _) | (_, Err(e)) => return usage_error(sub, &e),
            };
            match MessageStore::new(pool).read(&message_id, &caller) {
                Ok(row) => print_result(message_row_json(&row)),
                Err(e) => fail(e),
            }
        }
        "msg-sweep" => {
            let limit = match opt_usize(&flags, "limit") {
                Ok(v) => v.unwrap_or(50),
                Err(e) => return usage_error(sub, &e),
            };
            match MessageStore::new(pool).sweep(limit) {
                Ok((rows, total)) => print_result(serde_json::json!({
                    "undelivered_total": total,
                    "items": rows.iter().map(message_row_json).collect::<Vec<_>>(),
                })),
                Err(e) => fail(e),
            }
        }
        _ => usage(),
    }
}

fn usage_error(sub: &str, msg: &str) -> i32 {
    eprintln!("rp-collab {sub}: {msg}");
    usage()
}

fn print_result(value: serde_json::Value) -> i32 {
    println!("{}", value);
    0
}
