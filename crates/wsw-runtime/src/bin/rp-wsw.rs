//! `rp-wsw`：WSW-2 Listener/CA/DB 命令行面（Rust-First；rp_server 子进程
//! 消费，同 rp-doc/rp-workflow 先例）。
//!
//! 子命令：
//! - `listener`：跑 MITM 代理（loopback 拒绑 + Scope 闸 + 024 持久化 +
//!   状态心跳文件）；SIGTERM/SIGINT 优雅停机（拦截队列全 EXPIRED）。
//! - `ca status|export|regenerate`：CA 状态/导出/重生成（导出与重生成均为
//!   显式用户动作——API 带 confirm 语义，本面不静默安装、不碰系统代理/信任库）。
//! - `db migrate`：迁移执行（只在开发库副本/测试内存库；platform*.db 禁触）。
//!
//! 用法示例（S8 实拍）：
//! `rp-wsw listener --bind 127.0.0.1:3128 --db rip-data/wsw-e2e.db
//!   --rules-json '{"include":["127.0.0.1"],"allow_private_targets":true}'
//!   --status-file rip-data/wsw-state/listener-status.json`

use hudsucker::rustls::crypto::aws_lc_rs;
use std::path::PathBuf;
use wsw_runtime::listener::{handler::InterceptMatcher, RuntimeOptions};
use wsw_runtime::scope::ScopeRules;
use wsw_runtime::{authority, listener, ListenerConfig};

const TAG: &str = "[rp-wsw]";

fn usage() -> ! {
    eprintln!(
        "{TAG} usage:\n  rp-wsw listener --bind 127.0.0.1:3128 [--db <path>] [--ca-dir <dir>] \
[--rules-json <json>] [--scope-rules-file <path>] [--intercept] [--matcher-hosts a,b] \
[--matcher-paths /p59c] [--queue-ttl <secs>] [--status-file <path>] [--pid-file <path>] \
[--upstream-ca <pem>]... [--no-capture]\n  \
rp-wsw ca status [--ca-dir <dir>]\n  \
rp-wsw ca export [--ca-dir <dir>] --out <path>\n  \
rp-wsw ca regenerate [--ca-dir <dir>]\n  \
rp-wsw db migrate --db <path>"
    );
    std::process::exit(2);
}

struct Args {
    bind: Option<String>,
    db: Option<PathBuf>,
    ca_dir: Option<PathBuf>,
    rules_json: Option<String>,
    rules_file: Option<PathBuf>,
    intercept: bool,
    matcher_hosts: Vec<String>,
    matcher_paths: Vec<String>,
    queue_ttl: Option<u64>,
    status_file: Option<PathBuf>,
    pid_file: Option<PathBuf>,
    upstream_cas: Vec<PathBuf>,
    no_capture: bool,
    out: Option<PathBuf>,
}

fn parse_args(next: &mut impl Iterator<Item = String>) -> Args {
    let mut a = Args {
        bind: None,
        db: None,
        ca_dir: None,
        rules_json: None,
        rules_file: None,
        intercept: false,
        matcher_hosts: Vec::new(),
        matcher_paths: Vec::new(),
        queue_ttl: None,
        status_file: None,
        pid_file: None,
        upstream_cas: Vec::new(),
        no_capture: false,
        out: None,
    };
    while let Some(arg) = next.next() {
        let mut take = |what: &str| -> String {
            next.next()
                .unwrap_or_else(|| panic!("{TAG} {what} needs a value"))
        };
        match arg.as_str() {
            "--bind" => a.bind = Some(take("--bind")),
            "--db" => a.db = Some(take("--db").into()),
            "--ca-dir" => a.ca_dir = Some(take("--ca-dir").into()),
            "--rules-json" => a.rules_json = Some(take("--rules-json")),
            "--scope-rules-file" => a.rules_file = Some(take("--scope-rules-file").into()),
            "--intercept" => a.intercept = true,
            "--matcher-hosts" => {
                a.matcher_hosts = take("--matcher-hosts")
                    .split(',')
                    .map(String::from)
                    .collect()
            }
            "--matcher-paths" => {
                a.matcher_paths = take("--matcher-paths")
                    .split(',')
                    .map(String::from)
                    .collect()
            }
            "--queue-ttl" => a.queue_ttl = Some(take("--queue-ttl").parse().expect("ttl secs")),
            "--status-file" => a.status_file = Some(take("--status-file").into()),
            "--pid-file" => a.pid_file = Some(take("--pid-file").into()),
            "--upstream-ca" => a.upstream_cas.push(take("--upstream-ca").into()),
            "--no-capture" => a.no_capture = true,
            "--out" => a.out = Some(take("--out").into()),
            other => {
                eprintln!("{TAG} unknown arg {other}");
                usage();
            }
        }
    }
    a
}

/// 状态心跳写入（原子 rename；rp_server /api/wsw/proxy-status 数据源）。
async fn status_heartbeat(
    status_file: PathBuf,
    shared: std::sync::Arc<listener::ListenerShared>,
    listener_id: String,
    name: String,
    bind: String,
) {
    loop {
        let snapshot = listener::ListenerStatus {
            listener_id: listener_id.clone(),
            name: name.clone(),
            bind: bind.clone(),
            state: wsw_runtime::persist::listener_state_to_db(shared.state()).to_string(),
            intercept_enabled: shared.options.intercept_enabled,
            requests_total: shared
                .counters
                .requests_total
                .load(std::sync::atomic::Ordering::Relaxed),
            responses_total: shared
                .counters
                .responses_total
                .load(std::sync::atomic::Ordering::Relaxed),
            scope_blocked_total: shared
                .counters
                .scope_blocked_total
                .load(std::sync::atomic::Ordering::Relaxed),
            out_of_scope_passive_total: shared
                .counters
                .out_of_scope_passive_total
                .load(std::sync::atomic::Ordering::Relaxed),
            queue_pending: shared.coordinator.pending(),
            captures_total: shared
                .counters
                .captures_total
                .load(std::sync::atomic::Ordering::Relaxed),
            error_counts: shared.counters.error_counts(),
            started_at: shared.started_at.clone(),
        };
        let json = serde_json::to_string_pretty(&snapshot).unwrap_or_else(|_| "{}".into());
        let tmp = status_file.with_extension("json.tmp");
        if std::fs::write(&tmp, json)
            .and_then(|_| std::fs::rename(&tmp, &status_file))
            .is_err()
        {
            eprintln!(
                "{TAG} status heartbeat write failed: {}",
                status_file.display()
            );
        }
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    }
}

async fn run_listener(a: Args) -> Result<(), Box<dyn std::error::Error>> {
    let bind: std::net::SocketAddr = a
        .bind
        .clone()
        .unwrap_or_else(|| "127.0.0.1:3128".to_string())
        .parse()
        .map_err(|e| format!("--bind parse: {e}"))?;
    let mut config = ListenerConfig {
        bind,
        ..ListenerConfig {
            bind,
            queue_capacity: 64,
            max_header_bytes: 64 * 1024,
            max_body_bytes: 8 * 1024 * 1024,
            queue_ttl_seconds: a.queue_ttl.unwrap_or(120),
        }
    };
    config.queue_ttl_seconds = a.queue_ttl.unwrap_or(120);
    let rules = if let Some(f) = &a.rules_file {
        let raw = std::fs::read_to_string(f)?;
        ScopeRules::from_json(&raw)?
    } else if let Some(raw) = &a.rules_json {
        ScopeRules::from_json(raw)?
    } else {
        ScopeRules::default()
    };
    let listener_id = format!(
        "wsw-lis-{}-{:04x}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_micros(),
        std::process::id() as u16
    );
    let mut options = RuntimeOptions {
        listener_id: listener_id.clone(),
        name: "wsw-2 listener (rp-wsw)".to_string(),
        session_id: "wsw-ses-cli-0001".to_string(),
        out_of_scope_policy: "blocked".to_string(),
        config: config.clone(),
        scope_rules: rules,
        intercept_enabled: a.intercept,
        intercept_matcher: InterceptMatcher {
            hosts: a.matcher_hosts.clone(),
            methods: Vec::new(),
            path_prefixes: a.matcher_paths.clone(),
        },
        ca_dir: a.ca_dir.clone(),
        upstream_ca_paths: a.upstream_cas.clone(),
        db_path: a.db.clone(),
        artifacts_root: wsw_message::cas::resolve_artifacts_dir(),
        capture_cas: !a.no_capture,
    };
    options.config = config;
    options.config.bind = bind;

    let migrations_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations");
    let handle = listener::WswListener::start(options, &migrations_dir).await?;
    if let Some(pid_file) = &a.pid_file {
        std::fs::write(pid_file, format!("{}\n", std::process::id()))?;
    }
    let mut final_status_file: Option<PathBuf> = None;
    if let Some(status_file) = a.status_file.clone() {
        if let Some(parent) = status_file.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        final_status_file = Some(status_file.clone());
        let shared = std::sync::Arc::clone(&handle.shared);
        tokio::spawn(status_heartbeat(
            status_file,
            shared,
            listener_id,
            "wsw-2 listener (rp-wsw)".to_string(),
            bind.to_string(),
        ));
    }
    println!("{TAG} listener running on {bind} (loopback only); SIGTERM/SIGINT to stop");

    // 优雅停机：SIGTERM/SIGINT → stop（队列全 EXPIRED）→ 退出。
    let mut sigterm = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
    let mut sigint = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())?;
    tokio::select! {
        _ = sigterm.recv() => println!("{TAG} SIGTERM received"),
        _ = sigint.recv() => println!("{TAG} SIGINT received"),
    }
    handle.stop().await?;
    // 终态心跳：进程退出前把 stopped 快照写进状态文件（否则残留最后一次
    // running 心跳，rp_server 只读快照会误显示 running）。
    let final_status = handle.status();
    if let Some(sf) = final_status_file {
        let json = serde_json::to_string_pretty(&final_status).unwrap_or_default();
        let _ = std::fs::write(&sf, json);
    }
    println!("{TAG} listener stopped cleanly");
    Ok(())
}

fn ca_status(a: &Args) -> Result<(), Box<dyn std::error::Error>> {
    let dir = a.ca_dir.clone().unwrap_or_else(authority::resolve_ca_dir);
    let ca = authority::WswAuthority::open(&dir, aws_lc_rs::default_provider())?;
    let meta = ca.meta();
    let key_mode = authority::key_file_mode(&dir)?;
    let status = serde_json::json!({
        "cn": meta.cn,
        "not_before": meta.not_before,
        "not_after": meta.not_after,
        "fingerprint": meta.fingerprint,
        "ca_dir": dir.display().to_string(),
        "key_mode_octal": format!("{key_mode:o}"),
        "cache_entry_count": ca.cache_entry_count(),
    });
    println!("{status}");
    Ok(())
}

fn ca_export(a: &Args) -> Result<(), Box<dyn std::error::Error>> {
    let out = a.out.clone().ok_or("ca export requires --out <path>")?;
    let dir = a.ca_dir.clone().unwrap_or_else(authority::resolve_ca_dir);
    let ca = authority::WswAuthority::open(&dir, aws_lc_rs::default_provider())?;
    std::fs::write(&out, ca.cert_pem())?;
    println!(
        "{TAG} CA PEM exported to {} (fingerprint {}); trust install remains an explicit user action",
        out.display(),
        ca.meta().fingerprint
    );
    Ok(())
}

fn ca_regenerate(a: &Args) -> Result<(), Box<dyn std::error::Error>> {
    let dir = a.ca_dir.clone().unwrap_or_else(authority::resolve_ca_dir);
    let _ = std::fs::remove_file(dir.join("ca.key.pem"));
    let _ = std::fs::remove_file(dir.join("ca.cert.pem"));
    let _ = std::fs::remove_file(dir.join("ca.meta.json"));
    let ca = authority::WswAuthority::open(&dir, aws_lc_rs::default_provider())?;
    println!(
        "{TAG} CA regenerated (new fingerprint {}); previously trusted clients must re-trust explicitly",
        ca.meta().fingerprint
    );
    Ok(())
}

fn db_migrate(a: &Args) -> Result<(), Box<dyn std::error::Error>> {
    let db = a.db.clone().ok_or("db migrate requires --db <path>")?;
    let store = wsw_runtime::persist::PersistStore::open_file(&db)?;
    let migrations_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations");
    let applied = store.run_migrations(&migrations_dir)?;
    let version = store
        .with_conn(|conn| conn.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0)))?;
    println!(
        "{TAG} migrations applied to {}: {:?}; user_version={version}",
        db.display(),
        applied.iter().map(|m| m.version).collect::<Vec<_>>()
    );
    Ok(())
}

#[tokio::main]
async fn main() {
    // 日志可见面：拒绝记录/五态/捕获计数落 stderr 单行（PLAN :528）。
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_max_level(tracing::Level::INFO)
        .init();
    let mut args = std::env::args().skip(1);
    let Some(cmd) = args.next() else {
        usage();
    };
    let result = match cmd.as_str() {
        "listener" => run_listener(parse_args(&mut args)).await,
        "ca" => match args.next().as_deref() {
            Some("status") => ca_status(&parse_args(&mut args)),
            Some("export") => ca_export(&parse_args(&mut args)),
            Some("regenerate") => ca_regenerate(&parse_args(&mut args)),
            _ => usage(),
        },
        "db" => match args.next().as_deref() {
            Some("migrate") => db_migrate(&parse_args(&mut args)),
            _ => usage(),
        },
        _ => usage(),
    };
    if let Err(e) = result {
        eprintln!("{TAG} error: {e}");
        std::process::exit(1);
    }
}
