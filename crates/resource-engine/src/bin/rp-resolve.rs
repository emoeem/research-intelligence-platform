//! rp-resolve：resource-engine resolve 的 CLI 投影（容器冒烟与本地调试用）。
//!
//! 用法：
//! ```text
//! rp-resolve --db /db/platform-dryrun.db --keywords fuzzing --limit 5 --json
//! rp-resolve --db :memory: --migrate-dir /db --keywords "payload,bypass" --json
//! ```
//! `--json` 为兼容旗标（输出恒为 JSON）。退出码：0 成功；2 参数或执行错误。

use database::{DbPath, Pool, PoolConfig};
use resource_engine::{ResolveFilters, ResolveQuery, ResourceRepository, SqliteResourceRepository};
use std::path::PathBuf;
use std::time::Duration;

struct Args {
    db: String,
    migrate_dir: Option<String>,
    keywords: Vec<String>,
    limit: usize,
    task_id: String,
    resource_type: Option<String>,
    writable: bool,
}

fn usage() -> &'static str {
    "usage: rp-resolve --db PATH [--migrate-dir DIR] --keywords k1,k2 [--limit N]
                   [--task-id T] [--resource-type T] [--writable]

--db           SQLite 路径，或 :memory:（内存库，配合 --migrate-dir）
--migrate-dir  执行迁移的目录（NNN_name.sql，幂等）
--keywords     逗号分隔关键词
--limit        返回上限，默认 5
--task-id      任务 ID，默认 adhoc
--writable     以可写方式打开文件库（默认只读）

exit: ok=0, 运行失败=2（error + usage 到 stderr）
help: rp-resolve --help（stdout, exit 0）"
}

fn parse_args() -> Result<Args, String> {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    if argv.iter().any(|a| a == "--help" || a == "-h") {
        println!("{}", usage());
        std::process::exit(0);
    }
    let mut db: Option<String> = None;
    let mut migrate_dir: Option<String> = None;
    let mut keywords: Vec<String> = Vec::new();
    let mut limit = 5usize;
    let mut task_id = "adhoc".to_string();
    let mut resource_type: Option<String> = None;
    let mut writable = false;
    let mut i = 0;
    while i < argv.len() {
        let flag = argv[i].as_str();
        // 无值旗标先行：不消费下一个参数
        match flag {
            "--writable" => {
                writable = true;
                i += 1;
                continue;
            }
            "--json" => {
                i += 1; // 输出恒为 JSON，旗标仅为 eri CLI 兼容
                continue;
            }
            _ => {}
        }
        let val = argv
            .get(i + 1)
            .ok_or_else(|| format!("missing value for {flag}"))?;
        match flag {
            "--db" => db = Some(val.clone()),
            "--migrate-dir" => migrate_dir = Some(val.clone()),
            "--keywords" => {
                keywords = val.split(',').map(|s| s.trim().to_string()).collect();
            }
            "--limit" => {
                limit = val.parse().map_err(|_| format!("bad --limit: {val}"))?;
            }
            "--task-id" => task_id = val.clone(),
            "--resource-type" => resource_type = Some(val.clone()),
            other => return Err(format!("unknown flag: {other}")),
        }
        i += 2;
    }
    Ok(Args {
        db: db.ok_or("missing --db")?,
        migrate_dir,
        keywords,
        limit,
        task_id,
        resource_type,
        writable,
    })
}

fn main() {
    if let Err(err) = run() {
        // P9-3 帮助面统一口径：误用/失败 = error + usage 到 stderr + exit 2；
        // 显式 --help/-h = usage 到 stdout + exit 0（见 parse_args）。
        eprintln!("rp-resolve: error: {err}\n\n{}", usage());
        std::process::exit(2);
    }
}

fn run() -> Result<(), String> {
    let args = parse_args()?;
    if args.keywords.is_empty() {
        return Err("missing --keywords".to_string());
    }
    let pool = open_pool(&args).map_err(|e| e.to_string())?;
    if let Some(dir) = &args.migrate_dir {
        let applied =
            database::migrate(&pool, PathBuf::from(dir).as_path()).map_err(|e| e.to_string())?;
        eprintln!("rp-resolve: applied {} migration(s)", applied.len());
    }
    let repo = SqliteResourceRepository::new(pool);
    let mut q = ResolveQuery::new(args.keywords.clone());
    q.task_id = args.task_id.clone();
    q.limit = args.limit;
    q.filters = ResolveFilters {
        resource_type: args.resource_type.clone(),
        ..ResolveFilters::default()
    };
    let result = ResourceRepository::resolve(&repo, &q).map_err(|e| e.to_string())?;
    let json = serde_json::to_string_pretty(&result).map_err(|e| e.to_string())?;
    println!("{json}");
    Ok(())
}

fn open_pool(args: &Args) -> Result<Pool, database::DbError> {
    let config = if args.db == ":memory:" {
        PoolConfig {
            path: DbPath::Memory,
            readonly: false,
            max_connections: 1,
            busy_timeout: Duration::from_secs(5),
        }
    } else {
        if args.writable {
            PoolConfig::writable_file(&args.db)
        } else {
            PoolConfig::readonly_file(&args.db)
        }
    };
    Pool::open(config)
}
