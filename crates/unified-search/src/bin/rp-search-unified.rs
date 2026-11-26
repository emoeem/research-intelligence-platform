//! rp-search-unified：跨库统一检索 CLI（RIP3 P4-2，R2 6.1 唯一检索面的对外
//! 投影之一）：SQLite 运营核知识面 + PG 情报核面跨库 RRF 归并，JSON stdout。
//!
//! exit: ok=0，运行失败=1，用法错误=2（rp-* 家族惯例）。

use unified_search::{UnifiedOutcome, UnifiedSearch};

fn usage() -> ! {
    eprintln!(
        "usage: rp-search-unified --query Q [--workspace W] [--limit N] [--type T]\n\
         [--index-db PATH] [--sqlite-filters grade=A,license=…,evidence-level=K2,poc-severity=high,poc-missing-evidence=true]\n\
         [--pg-dsn DSN] [--run-db P] [--manifest-dir D] [--include-unapproved]\n\
\n\
env: RP_PG_DSN（PG 情报核，intel-db 缺省口径）\n\
     index-db 缺省 migrations/platform-search.db（2.0 FTS 索引面）\n\
help: rp-search-unified --help（stdout, exit 0）"
    );
    std::process::exit(2);
}

fn help_only() -> ! {
    println!(
        "rp-search-unified：RIP3 跨库统一检索 CLI（子命令清单见 stderr usage；此行输出后 exit 0）"
    );
    std::process::exit(0);
}

fn main() {
    let mut argv = std::env::args().peekable();
    let _bin = argv.next();
    if argv
        .peek()
        .map(|s| s == "--help" || s == "-h")
        .unwrap_or(false)
    {
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
            std::env::var("RP_PG_DSN").unwrap_or_else(|_| {
                "postgres://postgres:postgres@127.0.0.1:5432/postgres".to_string()
            })
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
    // sqlite-filters 解析（P4-2 结构体已备，P4-4 CLI 面展开）：
    // k=v 等值对 → SearchFilters；支持基础资源、Nuclei 和 RTIS POC facets。
    let sqlite_filters = parse_sqlite_filters(opt("sqlite-filters").as_deref());

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
                serde_json::to_string(
                    &serde_json::json!({ "query": query, "channels": channels, "hits": hits })
                )
                .unwrap_or_default()
            );
        }
        Err(e) => {
            println!("{{\"error\":{{\"code\":\"runtime_failed\",\"message\":{e:?}}}}}");
            std::process::exit(1);
        }
    }
}

/// "k=v[,k=v]" 等值对解析（P4-4 CLI 面）：支持基础资源、Nuclei 与 RTIS POC
/// facets；未知键与空值显式报错退出（fail-closed，防静默吞过滤条件）。
fn parse_sqlite_filters(raw: Option<&str>) -> search_engine::model::SearchFilters {
    let mut f = search_engine::model::SearchFilters::default();
    let Some(raw) = raw else { return f };
    for pair in raw.split(',') {
        let pair = pair.trim();
        if pair.is_empty() {
            continue;
        }
        let (k, v) = pair.split_once('=').unwrap_or_else(|| {
            eprintln!("sqlite-filters 段缺少 '='：{pair:?}（期望 k=v）");
            std::process::exit(2);
        });
        match k.trim() {
            "grade" => f.grade = Some(v.trim().to_string()),
            "license" => f.license_status = Some(v.trim().to_string()),
            "maintenance" => f.maintenance_status = Some(v.trim().to_string()),
            "resource-type" => f.resource_type = Some(v.trim().to_string()),
            "nuclei-severity" => f.nuclei_severity = Some(v.trim().to_ascii_lowercase()),
            "nuclei-protocol" => f.nuclei_protocol = Some(v.trim().to_string()),
            "cve-id" => f.cve_id = Some(v.trim().to_ascii_uppercase()),
            "cwe-id" => f.cwe_id = Some(v.trim().to_ascii_uppercase()),
            "evidence-level" => f.evidence_level = Some(v.trim().to_ascii_uppercase()),
            "knowledge-lifecycle" => f.knowledge_lifecycle = Some(v.trim().to_string()),
            "poc-severity" => f.poc_severity = Some(v.trim().to_ascii_lowercase()),
            "poc-category" => f.poc_category = Some(v.trim().to_ascii_lowercase()),
            "poc-cve-id" => f.poc_cve_id = Some(v.trim().to_ascii_uppercase()),
            "poc-source-integrity" => f.poc_source_integrity = Some(v.trim().to_ascii_lowercase()),
            "poc-verification-result" => {
                f.poc_verification_result = Some(v.trim().to_ascii_lowercase())
            }
            "poc-dedup-status" => f.poc_dedup_status = Some(v.trim().to_ascii_lowercase()),
            "poc-missing-evidence" => {
                f.poc_missing_evidence = Some(match v.trim().to_ascii_lowercase().as_str() {
                    "true" => true,
                    "false" => false,
                    _ => {
                        eprintln!("poc-missing-evidence 仅支持 true/false：{v:?}");
                        std::process::exit(2);
                    }
                });
            }
            other => {
                eprintln!(
                    "sqlite-filters 未知键 {other:?}（grade/license/maintenance/resource-type/nuclei-severity/nuclei-protocol/cve-id/cwe-id/evidence-level/knowledge-lifecycle/poc-severity/poc-category/poc-cve-id/poc-source-integrity/poc-verification-result/poc-dedup-status/poc-missing-evidence）"
                );
                std::process::exit(2);
            }
        }
    }
    f
}

#[cfg(test)]
mod tests {
    use super::parse_sqlite_filters;

    #[test]
    fn sqlite_filters_parse_curated_poc_facets() {
        let filters = parse_sqlite_filters(Some(
            "resource-type=rtis_poc,evidence-level=k2,knowledge-lifecycle=Researched,poc-severity=HIGH,poc-category=Unauthorized,poc-cve-id=cve-2025-1234,poc-source-integrity=partial,poc-verification-result=not_applicable,poc-dedup-status=related-family,poc-missing-evidence=true",
        ));
        assert_eq!(filters.resource_type.as_deref(), Some("rtis_poc"));
        assert_eq!(filters.evidence_level.as_deref(), Some("K2"));
        assert_eq!(filters.knowledge_lifecycle.as_deref(), Some("Researched"));
        assert_eq!(filters.poc_severity.as_deref(), Some("high"));
        assert_eq!(filters.poc_category.as_deref(), Some("unauthorized"));
        assert_eq!(filters.poc_cve_id.as_deref(), Some("CVE-2025-1234"));
        assert_eq!(filters.poc_source_integrity.as_deref(), Some("partial"));
        assert_eq!(
            filters.poc_verification_result.as_deref(),
            Some("not_applicable")
        );
        assert_eq!(filters.poc_dedup_status.as_deref(), Some("related-family"));
        assert_eq!(filters.poc_missing_evidence, Some(true));
    }
}
