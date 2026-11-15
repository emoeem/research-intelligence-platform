//! rp-search-unified：跨库统一检索 CLI（RIP3 P4-2，R2 6.1 唯一检索面的对外
//! 投影之一）：SQLite 运营核知识面 + PG 情报核面跨库 RRF 归并，JSON stdout。
//!
//! exit: ok=0，运行失败=1，用法错误=2（rp-* 家族惯例）。

use unified_search::{UnifiedSearch, UnifiedOutcome};

fn usage() -> ! {
    eprintln!(
        "usage: rp-search-unified --query Q [--workspace W] [--limit N] [--type T]\n\
         [--index-db PATH] [--sqlite-filters grade=A,license=…]\n\
         [--pg-dsn DSN] [--run-db P] [--manifest-dir D] [--include-unapproved]\n\
\n\
env: RP_PG_DSN（PG 情报核，intel-db 缺省口径）\n\
     index-db 缺省 migrations/platform-search.db（2.0 FTS 索引面）\n\
help: rp-search-unified --help（stdout, exit 0）"
    );
    std::process::exit(2);
}

fn help_only() -> ! {
    println!("rp-search-unified：RIP3 跨库统一检索 CLI（子命令清单见 stderr usage；此行输出后 exit 0）");
    std::process::exit(0);
}

fn main() {
    let mut argv = std::env::args().peekable();
    let _bin = argv.next();
    if argv.peek().map(|s| s == "--help" || s == "-h").unwrap_or(false) {
        help_only();
    }
    let mut flags = std::collections::HashMap::new();
    let mut subs: Vec<String> = Vec::new();
    let mut i = argv.peekable();
    while let Some(arg) = i.next() {
        if let Some(name) = arg.strip_prefix("--") {
            if name == "include-unapproved" {
                flags.insert(name.to_string(), "true".to_string());
            } else {
                let value = i.next().unwrap_or_else(|| usage());
                flags.insert(name.to_string(), value);
            }
        } else {
            subs.push(arg);
        }
    }
    let opt = |k: &str| flags.get(k).cloned();
    let req = |k: &str| -> String { flags.get(k).cloned().unwrap_or_else(|| usage()) };
    if !subs.is_empty() {
        usage();
    }
    let query = req("query");
    let workspace = opt("workspace").unwrap_or_else(|| "default".to_string());
    let limit: u32 = opt("limit").and_then(|s| s.parse().ok()).unwrap_or(10);

    // PG 情报核面。
    let pg_cfg = intel_db::PgConfig {
        dsn: opt("pg-dsn").unwrap_or_else(|| {
            std::env::var("RP_PG_DSN").unwrap_or_else(|_| "postgres://postgres:postgres@127.0.0.1:5432/postgres".to_string())
        }),
        max_connections: 4,
    };
    let pg_pool = match intel_db::open_pool(&pg_cfg) {
        Ok(p) => p,
        Err(e) => {
            println!("{{\"error\":{{\"code\":\"pg_unavailable\",\"message\":{e:?}}}}}");
            std::process::exit(1);
        }
    };

    // SQLite 索引面（platform-search.db 既有 FTS 面；缺席降级空通道）。
    let index_db = opt("index-db").unwrap_or_else(|| "migrations/platform-search.db".to_string());
    let sqlite_pool = database::Pool::open(database::PoolConfig {
        path: database::DbPath::File(std::path::PathBuf::from(&index_db)),
        readonly: true,
        max_connections: 2,
        busy_timeout: std::time::Duration::from_secs(5),
    })
    .ok();

    // PG 嵌入容器配置（查询向量本地推理；绝对化与治理扫描面同口径）。
    let mut embed_cfg = intel_db::embed_worker::EmbedWorkerConfig::default();
    if let Some(p) = opt("run-db") {
        embed_cfg.run_db = std::path::PathBuf::from(p);
    }
    if let Some(p) = opt("manifest-dir") {
        embed_cfg.manifest_dir = std::path::PathBuf::from(p);
    }
    embed_cfg.run_db = std::path::absolute(&embed_cfg.run_db).expect("absolute");
    embed_cfg.workspace_root = std::path::absolute(&embed_cfg.workspace_root).expect("absolute");
    embed_cfg.sqlite_migrations_dir =
        std::path::absolute(&embed_cfg.sqlite_migrations_dir).expect("absolute");
    embed_cfg.manifest_dir = std::path::absolute(&embed_cfg.manifest_dir).expect("absolute");

    let knowledge_type = opt("type")
        .map(|t| domain::kcard::KnowledgeType::parse(&t))
        .transpose()
        .expect("type parse");
    let sqlite_filters = search_engine::model::SearchFilters::default();

    let input = UnifiedSearch {
        pg_pool: &pg_pool,
        sqlite_pool: match &sqlite_pool {
            Some(p) => p,
            None => {
                println!("{{\"error\":{{\"code\":\"index_db_unavailable\",\"message\":\"index-db 打不开（可重建派生面；跨库归并降级需显式提供）\"}}}}");
                std::process::exit(1);
            }
        },
        embed_config: &embed_cfg,
        workspace_id: workspace.clone(),
        knowledge_type,
        approved_only: opt("include-unapproved").is_none(),
        limit,
        sqlite_filters,
    };
    match unified_search::unified_search(&input, &query) {
        Ok(UnifiedOutcome { hits, channels }) => {
            println!(
                "{}",
                serde_json::to_string(&serde_json::json!({ "query": query, "channels": channels, "hits": hits }))
                    .unwrap_or_default()
            );
        }
        Err(e) => {
            println!("{{\"error\":{{\"code\":\"runtime_failed\",\"message\":{e:?}}}}}");
            std::process::exit(1);
        }
    }
}
