//! rp-verify CLI（RIP3 Phase 5 验证编排入口，P5-2 契约面验收 + P5-3 起逐批扩展）：
//!
//! ~~~text
//! cargo run --offline -p verification-engine --bin rp-verify -- plan-create \
//!   --task <rtis-task-id> --title '<t>' [--target '<summary>'] [--budget '<json>']
//! cargo run --offline -p verification-engine --bin rp-verify -- plan-get --plan <id>
//! cargo run --offline -p verification-engine --bin rp-verify -- plan-list [--limit N]
//! cargo run --offline -p verification-engine --bin rp-verify -- plan-status --plan <id> --status ready
//! cargo run --offline -p verification-engine --bin rp-verify -- item-add --plan <id> \
//!   --provider rtis-poc --target '<normalized>' --fingerprint <sha256> \
//!   [--poc-id 900008] [--knowledge <knowledge_id>] [--reasons '<json array>'] \
//!   [--applicability applicable|excluded|needs_context] [--risk low_impact] \
//!   [--requests N] [--timeout-s N] [--order N] [--expected '<text>']
//! cargo run --offline -p verification-engine --bin rp-verify -- item-get --item <id>
//! cargo run --offline -p verification-engine --bin rp-verify -- item-list --plan <id>
//! cargo run --offline -p verification-engine --bin rp-verify -- result-set --item <id> \
//!   --status CONFIRMED --reason '<依据>' [--error-category timeout] [--run-id <id>] [--trace-id <id>]
//! cargo run --offline -p verification-engine --bin rp-verify -- event-list --plan <id>
//! cargo run --offline -p verification-engine --bin rp-verify -- fingerprint \
//!   --target h --endpoint e --poc p --provider pv [--params '<json>'] [--authctx <fingerprint>]
//! ~~~
//!
//! 输出契约：单行 JSON 进 stdout（rp-doc 惯例），错误帧 {"error":{code,message}}
//! 也走 stdout，退出码 0 = 成功 / 2 = 用法违规 / 1 = 守卫或 DB 拒绝。
//! RP_DB_PATH 指定 dev 库副本（缺省 memory 仅自检）；迁移由本 CLI 幂等应用
//! （database::migrate，020 已在链）。

use database::{migrate, DbPath, Pool, PoolConfig};
use std::path::PathBuf;
use std::process::exit;
use std::time::Duration;
use verification_engine::fingerprint::dedup_fingerprint;
use verification_engine::store::{NewItem, PlanStore};

fn main() {
    exit(run());
}

fn usage_text() -> &'static str {
    "usage: rp-verify <sub> [args]\n\
     \n\
     subcommands:\n\
     \x20 plan-create --task <id> --title '<t>' [--workspace w] [--target '<s>'] [--budget '<json>']\n\
     \x20 plan-get --plan <id>\n\
     \x20 plan-list [--limit N]\n\
     \x20 plan-status --plan <id> --status draft|ready|executing|paused|completed|cancelled\n\
     \x20 item-add --plan <id> --provider <id> --target '<ref>' --fingerprint <sha256>\n\
     \x20   [--poc-id N] [--knowledge <id>] [--applicability ...] [--reasons '<json>']\n\
     \x20   [--risk ...] [--requests N] [--timeout-s N] [--order N] [--expected '<t>'] [--version v]\n\
     \x20 item-get --item <id>\n\
     \x20 item-list --plan <id>\n\
     \x20 result-set --item <id> --status <五态> --reason '<r>' [--error-category <c>]\n\
     \x20   [--run-id <id>] [--trace-id <id>]\n\
     \x20 event-list --plan <id>\n\
     \x20 fingerprint --target h --endpoint e --poc p --provider pv [--params '<json>'] [--authctx <fp>]\n\
     \n\
     env: RP_DB_PATH（dev 库副本路径；缺省内存库）\n\
     exit: 0 成功 / 1 守卫或 DB 拒绝 / 2 用法违规\n\
     \n\
     help: rp-verify --help（stdout, exit 0）"
}

fn usage() -> i32 {
    eprintln!("{}", usage_text());
    2
}

/// 参数表（--key value 形态；布尔/缺省由调用侧处理）。
struct Args {
    map: std::collections::HashMap<String, String>,
}

impl Args {
    fn parse(all: &[String]) -> Args {
        let mut map = std::collections::HashMap::new();
        let mut index = 0;
        while index < all.len() {
            let arg = &all[index];
            if let Some(key) = arg.strip_prefix("--") {
                if let Some(value) = all.get(index + 1) {
                    map.insert(key.to_owned(), value.clone());
                    index += 2;
                    continue;
                }
            }
            index += 1;
        }
        Args { map }
    }

    fn get(&self, key: &str) -> Option<&str> {
        self.map.get(key).map(String::as_str)
    }

    fn required(&self, key: &str) -> Result<String, i32> {
        match self.get(key) {
            Some(value) => Ok(value.to_owned()),
            None => Err(usage()),
        }
    }
}

fn open_store() -> Result<PlanStore, String> {
    let path = std::env::var("RP_DB_PATH").unwrap_or_default();
    let db_path = if path.trim().is_empty() {
        DbPath::Memory
    } else {
        DbPath::File(PathBuf::from(path))
    };
    let pool = Pool::open(PoolConfig {
        path: db_path,
        readonly: false,
        max_connections: 1,
        busy_timeout: Duration::from_secs(5),
    })
    .map_err(|e| e.to_string())?;
    let migrations_dir = repo_root().join("migrations");
    migrate(&pool, &migrations_dir).map_err(|e| e.to_string())?;
    Ok(PlanStore::new(pool))
}

fn repo_root() -> PathBuf {
    // rp-verify 约定从仓库根或 web/ 外任意目录运行：优先 RP_REPO_ROOT，
    // 缺省按 CARGO_MANIFEST_DIR 编译期锚（target/debug 运行时同样成立）。
    if let Ok(root) = std::env::var("RP_REPO_ROOT") {
        return PathBuf::from(root);
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn emit(value: &serde_json::Value) {
    println!(
        "{}",
        serde_json::to_string(value).unwrap_or_else(|_| "{\"error\":{\"code\":\"serialize\",\"message\":\"serde_json\"}}".to_owned())
    );
}

fn emit_error(code: &str, message: &str) -> i32 {
    emit(&serde_json::json!({ "error": { "code": code, "message": message } }));
    1
}

fn run() -> i32 {
    let all: Vec<String> = std::env::args().skip(1).collect();
    if all.iter().any(|a| a == "--help" || a == "-h") {
        println!("{}", usage_text());
        return 0;
    }
    let Some(sub) = all.first() else {
        return usage();
    };
    let args = Args::parse(&all[1..]);
    let store = match open_store() {
        Ok(store) => store,
        Err(message) => return emit_error("db_open", &message),
    };
    match sub.as_str() {
        "plan-create" => {
            let task = args.get("task").map(str::to_owned);
            let title = args.get("title").map(str::to_owned);
            let workspace = args.get("workspace").unwrap_or("default").to_owned();
            let target = args.get("target").map(str::to_owned);
            let budget: Option<serde_json::Value> = args
                .get("budget")
                .and_then(|b| serde_json::from_str(b).ok());
            match store.create_plan(
                task.as_deref(),
                &workspace,
                title.as_deref(),
                target.as_deref(),
                budget.as_ref(),
            ) {
                Ok(record) => {
                    emit(&serde_json::to_value(&record).unwrap_or_default());
                    0
                }
                Err(e) => emit_error("plan_create", &e.to_string()),
            }
        }
        "plan-get" => match args.required("plan") {
            Ok(plan_id) => match store.get_plan(&plan_id) {
                Ok(Some(record)) => {
                    emit(&serde_json::to_value(&record).unwrap_or_default());
                    0
                }
                Ok(None) => emit_error("not_found", &plan_id),
                Err(e) => emit_error("plan_get", &e.to_string()),
            },
            Err(code) => code,
        },
        "plan-list" => {
            let limit: u32 = args
                .get("limit")
                .and_then(|v| v.parse().ok())
                .unwrap_or(50);
            match store.list_plans(limit) {
                Ok(rows) => {
                    emit(&serde_json::json!({ "items": rows, "limit": limit }));
                    0
                }
                Err(e) => emit_error("plan_list", &e.to_string()),
            }
        }
        "plan-status" => {
            let (plan, status) = match (args.required("plan"), args.required("status")) {
                (Ok(a), Ok(b)) => (a, b),
                _ => return usage(),
            };
            match store.set_plan_status(&plan, &status) {
                Ok(()) => {
                    emit(&serde_json::json!({ "plan_id": plan, "status": status }));
                    0
                }
                Err(e) => emit_error("plan_status", &e.to_string()),
            }
        }
        "item-add" => {
            let plan = match args.required("plan") {
                Ok(v) => v,
                Err(code) => return code,
            };
            let provider = match args.required("provider") {
                Ok(v) => v,
                Err(code) => return code,
            };
            let target = match args.required("target") {
                Ok(v) => v,
                Err(code) => return code,
            };
            let fingerprint = match args.required("fingerprint") {
                Ok(v) => v,
                Err(code) => return code,
            };
            let reasons: Vec<String> = args
                .get("reasons")
                .and_then(|r| serde_json::from_str(r).ok())
                .unwrap_or_default();
            let item = NewItem {
                knowledge_ref: args.get("knowledge").map(str::to_owned),
                rtis_poc_id: args.get("poc-id").and_then(|v| v.parse().ok()),
                provider_id: provider,
                provider_version: args.get("version").map(str::to_owned),
                target_ref: target,
                dedup_fingerprint: fingerprint,
                applicability_status: args
                    .get("applicability")
                    .unwrap_or("needs_context")
                    .to_owned(),
                applicability_reasons: reasons,
                risk_class: args.get("risk").unwrap_or("low_impact").to_owned(),
                request_budget: args.get("requests").and_then(|v| v.parse().ok()).unwrap_or(10),
                timeout_budget_s: args.get("timeout-s").and_then(|v| v.parse().ok()).unwrap_or(60),
                execution_order: args.get("order").and_then(|v| v.parse().ok()).unwrap_or(0),
                expected_observation: args.get("expected").map(str::to_owned),
            };
            match store.add_item(&plan, item) {
                Ok(record) => {
                    emit(&serde_json::to_value(&record).unwrap_or_default());
                    0
                }
                Err(e) => emit_error("item_add", &e.to_string()),
            }
        }
        "item-get" => match args.required("item") {
            Ok(item_id) => match store.get_item(&item_id) {
                Ok(Some(record)) => {
                    emit(&serde_json::to_value(&record).unwrap_or_default());
                    0
                }
                Ok(None) => emit_error("not_found", &item_id),
                Err(e) => emit_error("item_get", &e.to_string()),
            },
            Err(code) => code,
        },
        "item-list" => match args.required("plan") {
            Ok(plan_id) => match store.list_items(&plan_id) {
                Ok(rows) => {
                    emit(&serde_json::json!({ "items": rows, "plan_id": plan_id }));
                    0
                }
                Err(e) => emit_error("item_list", &e.to_string()),
            },
            Err(code) => code,
        },
        "result-set" => {
            let item = match args.required("item") {
                Ok(v) => v,
                Err(code) => return code,
            };
            let status = match args.required("status") {
                Ok(v) => v,
                Err(code) => return code,
            };
            match store.set_result(
                &item,
                &status,
                args.get("reason"),
                args.get("error-category"),
                args.get("run-id"),
                args.get("trace-id"),
            ) {
                Ok(record) => {
                    emit(&serde_json::to_value(&record).unwrap_or_default());
                    0
                }
                Err(e) => emit_error("result_set", &e.to_string()),
            }
        }
        "event-list" => match args.required("plan") {
            Ok(plan_id) => match store.list_events(&plan_id) {
                Ok(rows) => {
                    emit(&serde_json::json!({ "items": rows, "plan_id": plan_id }));
                    0
                }
                Err(e) => emit_error("event_list", &e.to_string()),
            },
            Err(code) => code,
        },
        "fingerprint" => {
            let (target, endpoint, poc, provider) = match (
                args.required("target"),
                args.required("endpoint"),
                args.required("poc"),
                args.required("provider"),
            ) {
                (Ok(a), Ok(b), Ok(c), Ok(d)) => (a, b, c, d),
                _ => return usage(),
            };
            let value = dedup_fingerprint(
                &target,
                &endpoint,
                &poc,
                &provider,
                args.get("params").unwrap_or("{}"),
                args.get("authctx").unwrap_or("-"),
            );
            emit(&serde_json::json!({ "dedup_fingerprint": value }));
            0
        }
        other => {
            eprintln!("unknown subcommand: {other}");
            usage()
        }
    }
}
