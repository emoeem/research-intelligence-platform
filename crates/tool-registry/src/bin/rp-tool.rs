//! rp-tool CLI：Tool Registry 校验/注册/查询最小命令面（ROADMAP P6-9，
//! SESSION-HANDOFF §八「添加新工具 = 一条可复制路径」的登记侧载体）。
//!
//! 子命令：
//! - `validate --manifest <path>`：七条静态校验（复用 validate_static +
//!   jsonschema_lite + schemas/tool-manifest/v1.json 冻结契约），不触库；
//! - `register --manifest <path> --db <path>`：migrate + 七条 + rule 2
//!   版本可比 + tool/tool_version 落库（tool-registry 既有 Registry 门面）；
//! - `list --db <path>`：catalog::discover（tool 表全量）；
//! - `get --db <path> <tool_id>`：catalog::query（tool 行 + 版本面）。
//!
//! Manifest 支持 YAML 与 JSON（serde_yaml_ng 解析，JSON 是 YAML 1.2 子集）。
//! 输出统一为单行 JSON（机器可读）；exit 0 成功、1 失败、2 用法错误。

use std::path::PathBuf;

use serde_json::{json, Value};
use tool_registry::{catalog, Options, Registry};

fn die_json(code: &str, message: &str) -> ! {
    eprintln!("{}", json!({"error": {"code": code, "message": message}}));
    std::process::exit(1);
}

fn usage() -> ! {
    eprintln!(
        "usage: rp-tool validate --manifest <path>\n       \
         rp-tool register --manifest <path> --db <path>\n       \
         rp-tool list --db <path>\n       \
         rp-tool get --db <path> <tool_id>"
    );
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
        other => {
            eprintln!("unknown command: {other:?}");
            usage();
        }
    }
}
