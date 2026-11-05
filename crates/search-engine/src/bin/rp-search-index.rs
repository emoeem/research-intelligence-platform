//! rp-search-index：search-engine 索引构建与查询的 CLI 投影（真实索引运行与调试用）。
//!
//! 用法：
//! ```text
//! rp-search-index build  --platform-db PATH --catalog-db PATH --index-db PATH [--rebuild]
//! rp-search-index query  --index-db PATH --keywords k1,k2 [--limit N] [--offset N]
//!                        [--grade G] [--license-status S] [--maintenance-status S]
//!                        [--resource-type T] [--json]
//! ```
//! 退出码：0 成功；2 参数或执行错误。输出恒为 JSON。

use database::{Pool, PoolConfig};
use search_engine::{BuildReport, Hits, SearchEngine, SearchFilters, SearchIndex, SearchQuery};
use std::path::PathBuf;

enum Mode {
    Build {
        rebuild: bool,
    },
    Query {
        keywords: Vec<String>,
        limit: usize,
        offset: usize,
    },
}

struct Args {
    mode: Mode,
    platform_db: Option<PathBuf>,
    catalog_db: Option<PathBuf>,
    index_db: PathBuf,
    filters: SearchFilters,
}

fn usage() -> &'static str {
    "rp-search-index build --platform-db P --catalog-db C --index-db I [--rebuild]
rp-search-index query --index-db I --keywords k1,k2 [--limit N] [--offset N]
                      [--grade A] [--license-status S] [--maintenance-status S]
                      [--resource-type T]

build  从 platform 库（resource + resource_source）与 catalog 库（github_metadata）
       构建派生 FTS5 索引；--rebuild 先删既有索引表再重灌（可重建纪律）
query  对已构建索引库执行混合检索（bm25 50% / quality 30% / keyword 覆盖 20%）"
}

fn parse_args() -> Result<Args, String> {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    if argv.iter().any(|a| a == "--help" || a == "-h") || argv.is_empty() {
        println!("{}", usage());
        std::process::exit(0);
    }
    let mode = argv[0].as_str();
    if mode != "build" && mode != "query" {
        return Err(format!("unknown mode {mode}: expected build|query"));
    }
    let mut rebuild = false;
    let mut platform_db = None;
    let mut catalog_db = None;
    let mut index_db: Option<PathBuf> = None;
    let mut keywords: Vec<String> = Vec::new();
    let mut limit = 10usize;
    let mut offset = 0usize;
    let mut filters = SearchFilters::default();
    let mut i = 1;
    let need_value = |argv: &[String], i: &mut usize, flag: &str| -> Result<String, String> {
        *i += 1;
        argv.get(*i)
            .cloned()
            .ok_or_else(|| format!("{flag} requires a value"))
    };
    while i < argv.len() {
        match argv[i].as_str() {
            "--rebuild" => rebuild = true,
            "--platform-db" => {
                platform_db = Some(PathBuf::from(need_value(&argv, &mut i, "--platform-db")?))
            }
            "--catalog-db" => {
                catalog_db = Some(PathBuf::from(need_value(&argv, &mut i, "--catalog-db")?))
            }
            "--index-db" => {
                index_db = Some(PathBuf::from(need_value(&argv, &mut i, "--index-db")?))
            }
            "--keywords" => {
                let v = need_value(&argv, &mut i, "--keywords")?;
                keywords = v
                    .split(',')
                    .filter(|s| !s.trim().is_empty())
                    .map(String::from)
                    .collect();
            }
            "--limit" => {
                limit = need_value(&argv, &mut i, "--limit")?
                    .parse()
                    .map_err(|e| format!("--limit: {e}"))?
            }
            "--offset" => {
                offset = need_value(&argv, &mut i, "--offset")?
                    .parse()
                    .map_err(|e| format!("--offset: {e}"))?
            }
            "--grade" => filters.grade = Some(need_value(&argv, &mut i, "--grade")?),
            "--license-status" => {
                filters.license_status = Some(need_value(&argv, &mut i, "--license-status")?)
            }
            "--maintenance-status" => {
                filters.maintenance_status =
                    Some(need_value(&argv, &mut i, "--maintenance-status")?)
            }
            "--resource-type" => {
                filters.resource_type = Some(need_value(&argv, &mut i, "--resource-type")?)
            }
            other => return Err(format!("unknown flag {other}")),
        }
        i += 1;
    }
    let index_db = index_db.ok_or("--index-db is required")?;
    let m = match mode {
        "build" => {
            if platform_db.is_none() || catalog_db.is_none() {
                return Err("build requires --platform-db and --catalog-db".to_string());
            }
            Mode::Build { rebuild }
        }
        _ => Mode::Query {
            keywords,
            limit,
            offset,
        },
    };
    Ok(Args {
        mode: m,
        platform_db,
        catalog_db,
        index_db,
        filters,
    })
}

fn open_ro(path: &PathBuf, label: &str) -> Result<Pool, String> {
    if !path.exists() {
        return Err(format!("{label} not found: {}", path.display()));
    }
    Pool::open(PoolConfig::readonly_file(path)).map_err(|e| format!("open {label}: {e}"))
}

fn main() {
    let args = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("error: {e}\n\n{}", usage());
            std::process::exit(2);
        }
    };
    let result = run(args);
    if let Err(e) = result {
        eprintln!("error: {e}");
        std::process::exit(2);
    }
}

fn run(args: Args) -> Result<(), String> {
    match args.mode {
        Mode::Build { rebuild } => {
            let platform = open_ro(args.platform_db.as_ref().expect("checked"), "platform db")?;
            let catalog = open_ro(args.catalog_db.as_ref().expect("checked"), "catalog db")?;
            let index = Pool::open(PoolConfig::writable_file(&args.index_db))
                .map_err(|e| format!("open index db: {e}"))?;
            let report: BuildReport = if rebuild {
                SearchIndex::rebuild(&platform, &catalog, &index)
            } else {
                SearchIndex::build(&platform, &catalog, &index)
            }
            .map_err(|e| format!("build: {e}"))?;
            println!(
                "{}",
                serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?
            );
        }
        Mode::Query {
            keywords,
            limit,
            offset,
        } => {
            let index = open_ro(&args.index_db, "index db")?;
            let engine = SearchEngine::new(index);
            let mut q = SearchQuery::new(keywords);
            q.filters = args.filters;
            q.limit = limit;
            q.offset = offset;
            let hits: Hits = engine.search(&q).map_err(|e| format!("search: {e}"))?;
            println!(
                "{}",
                serde_json::to_string_pretty(&hits).map_err(|e| e.to_string())?
            );
        }
    }
    Ok(())
}
