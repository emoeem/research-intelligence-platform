//! rp-knowledge-fetch CLI：`rip.knowledge.document.fetch`（T1，SK-3/P6-11）
//! 的可运行入口（manifest runtime native_rust binary=rp-knowledge-fetch,
//! subcommand=fetch）。
//!
//! 策略面在 tool_engine::document_fetch（allowlist + 私网拒绝 + 重定向
//! 逐跳 + 限额 + approved 闸 + 012 落库），本 CLI 只做参数解析、红线守卫
//! 与结构化输出。
//!
//! 用法（--db 必须是开发库副本或临时文件，禁止生产库）：
//!
//! ~~~bash
//! cargo run --offline -p tool-engine --bin rp-knowledge-fetch -- \
//!   --db <开发库副本> --url http://example.com/doc \
//!   --allow-domain example.com [--source-id knowledge.xxx] \
//!   [--entry-key <item_key>] [--max-bytes 2097152] [--max-redirects 3] \
//!   [--no-require-approved] [--out /tmp/body.md]
//! ~~~
//!
//! 输出单行 JSON（机器可读）；exit 0 抓取成功、1 拒绝（含结构化
//! reject_reason）、2 用法错误。

use std::path::{Path, PathBuf};
use std::time::Duration;

use database::{rusqlite, DbPath, Pool, PoolConfig};
use serde_json::{json, Value};
use tool_engine::adapter::egress_proxy::EgressAllowlist;
use tool_engine::document_fetch::{fetch_document, FetchInput, FetchPolicy, ProxyTransport};

fn usage() -> ! {
    eprintln!(
        "usage: rp-knowledge-fetch --db <path> --url <url> [--allow-domain D]...\n       \
         [--source-id S] [--entry-key K] [--max-bytes N] [--max-redirects N]\n       \
         [--allow-port P]... [--no-require-approved] [--out PATH]"
    );
    std::process::exit(2);
}

fn flag_values(raw: &[String], flag: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < raw.len() {
        if raw[i] == flag {
            if let Some(v) = raw.get(i + 1) {
                out.push(v.clone());
            }
            i += 2;
        } else {
            i += 1;
        }
    }
    out
}

fn flag_value(raw: &[String], flag: &str) -> Option<String> {
    flag_values(raw, flag).into_iter().next()
}

fn parse_num(raw: &[String], flag: &str) -> Option<usize> {
    flag_value(raw, flag).and_then(|v| v.parse().ok())
}

fn guard_db_path(db: &Path) {
    let name = db
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    if name.starts_with("platform") {
        eprintln!(
            "{}",
            json!({"code": "production_db_refused",
                   "message": "--db must be a dev copy or temp file; platform*.db is forbidden (ROADMAP §13)"})
        );
        std::process::exit(1);
    }
}

fn main() {
    let raw: Vec<String> = std::env::args().skip(1).collect();
    let db = flag_value(&raw, "--db");
    let url = match flag_value(&raw, "--url") {
        Some(u) if !u.is_empty() => u,
        _ => usage(),
    };
    let source_id = flag_value(&raw, "--source-id");
    let entry_key = flag_value(&raw, "--entry-key");
    let hosts: Vec<String> = flag_values(&raw, "--allow-domain");
    let mut ports: Vec<u16> = flag_values(&raw, "--allow-port")
        .iter()
        .filter_map(|p| p.parse().ok())
        .collect();
    if ports.is_empty() {
        ports = vec![80]; // 本批 http 明文传输面
    }
    let max_bytes = parse_num(&raw, "--max-bytes").unwrap_or(2 * 1024 * 1024);
    let max_redirects = parse_num(&raw, "--max-redirects").unwrap_or(3) as u32;
    let require_approved = !raw.iter().any(|a| a == "--no-require-approved");
    let out_path = flag_value(&raw, "--out");

    let pool = db.map(|db| {
        let path = PathBuf::from(&db);
        guard_db_path(&path);
        Pool::open(PoolConfig {
            path: DbPath::File(path),
            readonly: false,
            max_connections: 1,
            busy_timeout: Duration::from_secs(5),
        })
        .expect("open db pool")
    });
    if let Some(pool) = &pool {
        database::migrate(
            pool,
            &PathBuf::from(
                flag_value(&raw, "--migrations").unwrap_or_else(|| "migrations".to_string()),
            ),
        )
        .expect("migrate dev copy");
    }

    // approved 闸复核闭包：URL 必须命中 APPROVED canonical_url（entry_key
    // 给出时额外核对条目归属）；无 --db 且 require_approved 时直接拒绝
    // （approved 闸没有 Registry 就没有 approved 面，fail-closed）。
    let approved = |u: &str, key: Option<&str>| -> Result<String, String> {
        let Some(pool) = &pool else {
            return Err("approved gate requires --db (registry approved set)".to_string());
        };
        if let Some(key) = key {
            let source = source_id.clone().unwrap_or_default();
            let canonical: Option<String> = pool
                .with_conn(|c| {
                    c.query_row(
                        "SELECT canonical_url FROM knowledge_source_entry
                         WHERE (?1 = '' OR plugin_id = ?1) AND item_key = ?2
                           AND review_status = 'APPROVED'",
                        rusqlite::params![source, key],
                        |r| r.get(0),
                    )
                    .map(Some)
                    .or_else(|e| match e {
                        database::rusqlite::Error::QueryReturnedNoRows => Ok(None),
                        other => Err(other),
                    })
                })
                .map_err(|e| e.to_string())?;
            let Some(canonical) = canonical else {
                return Err(format!(
                    "entry {key:?} not found or not APPROVED for source {source:?}"
                ));
            };
            if canonical != u {
                return Err(format!(
                    "url does not match approved canonical {canonical:?}"
                ));
            }
            return Ok(canonical);
        }
        let n: i64 = pool
            .with_conn(|c| {
                c.query_row(
                    "SELECT COUNT(*) FROM knowledge_source_entry
                     WHERE canonical_url = ?1 AND review_status = 'APPROVED'",
                    [&u],
                    |r| r.get(0),
                )
            })
            .map_err(|e| e.to_string())?;
        if n == 0 {
            return Err("url not in APPROVED canonical set".to_string());
        }
        Ok(u.to_string())
    };

    let policy = FetchPolicy {
        allowed_hosts: hosts.clone(),
        allowed_ports: ports.clone(),
        max_bytes,
        max_redirects,
        require_approved,
    };

    // 传输：run 级白名单代理（host=策略 allowlist；端口=策略 ports）。
    // 代理层逐请求 allowlist 复核与策略面双重防御（egress_proxy 语义）。
    let proxy = ProxyTransport::spawn(
        &format!("fetch-{}", domain::time::now_compact_micros()),
        EgressAllowlist {
            hosts: hosts.clone(),
            ports: ports.clone(),
        },
    )
    .expect("spawn egress proxy");
    let input = FetchInput {
        source_id: source_id.clone(),
        url: url.clone(),
        entry_key: entry_key.clone(),
    };
    let outcome = fetch_document(
        pool.as_ref(),
        &proxy,
        &policy,
        &input,
        if require_approved {
            Some(&approved)
        } else {
            None
        },
    );
    match outcome {
        Ok(out) => {
            if let Some(path) = out_path {
                std::fs::write(&path, &out.body).expect("write --out body file");
            }
            println!(
                "{}",
                serde_json::to_string(&json_record(&out.record)).unwrap()
            );
        }
        Err(rej) => {
            println!(
                "{}",
                serde_json::to_string(&json!({
                    "status": "rejected",
                    "reject_reason": rej.code(),
                    "reject_detail": rej.detail(),
                    "requested_url": url,
                }))
                .unwrap()
            );
            std::process::exit(1);
        }
    }
}

fn json_record(r: &tool_engine::document_fetch::FetchRecord) -> Value {
    json!({
        "fetch_id": r.fetch_id,
        "source_id": r.source_id,
        "requested_url": r.requested_url,
        "final_url": r.final_url,
        "hops": r.hops,
        "status": r.status,
        "reject_reason": r.reject_reason,
        "content_sha256": r.content_sha256,
        "content_bytes": r.content_bytes,
        "mime_type": r.mime_type,
        "fetched_at": r.fetched_at,
        "elapsed_ms": r.elapsed_ms,
    })
}
