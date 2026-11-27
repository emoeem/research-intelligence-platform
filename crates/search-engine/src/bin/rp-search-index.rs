//! rp-search-index：search-engine 索引构建与查询的 CLI 投影（真实索引运行与调试用）。
//!
//! 用法：
//! ```text
//! rp-search-index build  --platform-db PATH --catalog-db PATH --index-db PATH [--rtis-db PATH --curation-overlay PATH] [--rebuild]
//! rp-search-index query  --index-db PATH --keywords k1,k2 [--limit N] [--offset N]
//!                        [--grade G] [--license-status S] [--maintenance-status S]
//!                        [--resource-type T] [--nuclei-severity S] [--nuclei-protocol P]
//!                        [--cve-id CVE] [--cwe-id CWE] [--evidence-level K0..K4]
//!                        [--knowledge-lifecycle L] [--poc-severity S] [--poc-category C] [--poc-cve-id CVE] [--json]
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
    plugin_db: Option<PathBuf>,
    catalog_db: Option<PathBuf>,
    rtis_db: Option<PathBuf>,
    curation_overlay: Option<PathBuf>,
    index_db: PathBuf,
    filters: SearchFilters,
}

fn usage() -> &'static str {
    "usage: rp-search-index build --platform-db P [--plugin-db P] --catalog-db C --index-db I [--rtis-db R --curation-overlay O] [--rebuild]
        rp-search-index query --index-db I --keywords k1,k2 [--limit N] [--offset N]
                              [--grade A] [--license-status S] [--maintenance-status S]
                              [--resource-type T] [--nuclei-severity S]
                              [--nuclei-protocol P] [--cve-id CVE] [--cwe-id CWE]
                              [--evidence-level K0..K4] [--knowledge-lifecycle L]
                              [--poc-severity S] [--poc-category C] [--poc-cve-id CVE]
                              [--poc-source-integrity S] [--poc-verification-result R]
                              [--poc-dedup-status S] [--poc-missing-evidence true|false]
                              [--canonical-key KEY] [--vulnerability-id CVE]

build  从 platform/catalog 构建基础索引；可选 --plugin-db 装载独立插件知识库；可选 --rtis-db + --curation-overlay
       将 Overlay 明确审核的自有 POC 元数据加入同一 FTS5 索引。RTIS 只读；
       不读取 POC 正文、description、usage 或 verified_note。--rebuild 重建派生索引
query  对已构建索引库执行混合检索（bm25 50% / quality 30% / keyword 覆盖 20%）

exit: ok=0, 运行失败=2（error + usage 到 stderr）
help: rp-search-index --help（stdout, exit 0）"
}

fn parse_args() -> Result<Args, String> {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    if argv.iter().any(|a| a == "--help" || a == "-h") {
        println!("{}", usage());
        std::process::exit(0);
    }
    if argv.is_empty() {
        // P9-3 帮助面统一口径：无参 = 误用（usage 到 stderr + exit 2），
        // 不再与显式 --help 同路径（原实现无参打印帮助 exit 0）。
        eprintln!("{}", usage());
        std::process::exit(2);
    }
    let mode = argv[0].as_str();
    if mode != "build" && mode != "query" {
        return Err(format!("unknown mode {mode}: expected build|query"));
    }
    let mut rebuild = false;
    let mut platform_db = None;
    let mut plugin_db = None;
    let mut catalog_db = None;
    let mut rtis_db = None;
    let mut curation_overlay = None;
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
            "--plugin-db" => {
                plugin_db = Some(PathBuf::from(need_value(&argv, &mut i, "--plugin-db")?))
            }
            "--catalog-db" => {
                catalog_db = Some(PathBuf::from(need_value(&argv, &mut i, "--catalog-db")?))
            }
            "--rtis-db" => rtis_db = Some(PathBuf::from(need_value(&argv, &mut i, "--rtis-db")?)),
            "--curation-overlay" => {
                curation_overlay = Some(PathBuf::from(need_value(
                    &argv,
                    &mut i,
                    "--curation-overlay",
                )?))
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
            "--nuclei-severity" => {
                filters.nuclei_severity = Some(need_value(&argv, &mut i, "--nuclei-severity")?)
            }
            "--nuclei-protocol" => {
                filters.nuclei_protocol = Some(need_value(&argv, &mut i, "--nuclei-protocol")?)
            }
            "--cve-id" => filters.cve_id = Some(need_value(&argv, &mut i, "--cve-id")?),
            "--cwe-id" => filters.cwe_id = Some(need_value(&argv, &mut i, "--cwe-id")?),
            "--canonical-key" => {
                filters.canonical_key = Some(need_value(&argv, &mut i, "--canonical-key")?)
            }
            "--vulnerability-id" => {
                filters.vulnerability_id = Some(need_value(&argv, &mut i, "--vulnerability-id")?)
            }
            "--evidence-level" => {
                filters.evidence_level = Some(need_value(&argv, &mut i, "--evidence-level")?)
            }
            "--knowledge-lifecycle" => {
                filters.knowledge_lifecycle =
                    Some(need_value(&argv, &mut i, "--knowledge-lifecycle")?)
            }
            "--poc-severity" => {
                filters.poc_severity = Some(need_value(&argv, &mut i, "--poc-severity")?)
            }
            "--poc-category" => {
                filters.poc_category = Some(need_value(&argv, &mut i, "--poc-category")?)
            }
            "--poc-cve-id" => filters.poc_cve_id = Some(need_value(&argv, &mut i, "--poc-cve-id")?),
            "--poc-source-integrity" => {
                filters.poc_source_integrity =
                    Some(need_value(&argv, &mut i, "--poc-source-integrity")?)
            }
            "--poc-verification-result" => {
                filters.poc_verification_result =
                    Some(need_value(&argv, &mut i, "--poc-verification-result")?)
            }
            "--poc-dedup-status" => {
                filters.poc_dedup_status = Some(need_value(&argv, &mut i, "--poc-dedup-status")?)
            }
            "--poc-missing-evidence" => {
                let value = need_value(&argv, &mut i, "--poc-missing-evidence")?;
                filters.poc_missing_evidence = Some(match value.as_str() {
                    "true" => true,
                    "false" => false,
                    _ => return Err("--poc-missing-evidence must be true|false".into()),
                });
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
            if rtis_db.is_some() != curation_overlay.is_some() {
                return Err(
                    "--rtis-db and --curation-overlay must be supplied together".to_string()
                );
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
        plugin_db,
        catalog_db,
        rtis_db,
        curation_overlay,
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
            let plugin = args
                .plugin_db
                .as_ref()
                .map(|path| open_ro(path, "plugin db"))
                .transpose()?;
            let catalog = open_ro(args.catalog_db.as_ref().expect("checked"), "catalog db")?;
            let index = Pool::open(PoolConfig::writable_file(&args.index_db))
                .map_err(|e| format!("open index db: {e}"))?;
            let report: BuildReport =
                match (plugin.as_ref(), &args.rtis_db, &args.curation_overlay) {
                    (Some(plugin), Some(rtis_path), Some(overlay_path)) => {
                        let rtis = open_ro(rtis_path, "RTIS db")?;
                        if rebuild {
                            SearchIndex::rebuild_with_rtis_curation_and_plugin_db(
                                &platform,
                                plugin,
                                &catalog,
                                &rtis,
                                overlay_path,
                                &index,
                            )
                        } else {
                            SearchIndex::build_with_rtis_curation_and_plugin_db(
                                &platform,
                                plugin,
                                &catalog,
                                &rtis,
                                overlay_path,
                                &index,
                            )
                        }
                    }
                    (Some(plugin), None, None) if rebuild => {
                        SearchIndex::rebuild_with_plugin_db(&platform, plugin, &catalog, &index)
                    }
                    (Some(plugin), None, None) => {
                        SearchIndex::build_with_plugin_db(&platform, plugin, &catalog, &index)
                    }
                    (None, Some(rtis_path), Some(overlay_path)) => {
                        let rtis = open_ro(rtis_path, "RTIS db")?;
                        if rebuild {
                            SearchIndex::rebuild_with_rtis_curation(
                                &platform,
                                &catalog,
                                &rtis,
                                overlay_path,
                                &index,
                            )
                        } else {
                            SearchIndex::build_with_rtis_curation(
                                &platform,
                                &catalog,
                                &rtis,
                                overlay_path,
                                &index,
                            )
                        }
                    }
                    (None, None, None) if rebuild => {
                        SearchIndex::rebuild(&platform, &catalog, &index)
                    }
                    (None, None, None) => SearchIndex::build(&platform, &catalog, &index),
                    _ => unreachable!("RTIS database and overlay are validated as a pair"),
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
