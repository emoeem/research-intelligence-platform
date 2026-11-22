//! `rp-wsw`：WSW-2 Listener/CA/DB 命令行面 + WSW-3 send 子命令（Rust-First；
//! rp_server 子进程消费，同 rp-doc/rp-workflow 先例）。
//!
//! 子命令：
//! - `listener`：跑 MITM 代理（loopback 拒绑 + Scope 闸 + 024 持久化 +
//!   状态心跳文件）；SIGTERM/SIGINT 优雅停机（拦截队列全 EXPIRED）。
//! - `send`：Repeater 发送（W3-D4）：Outbound → serialize → h1 字节写 →
//!   parse_wire；SIGTERM → cancelled 终态落行；pending→sending→终态自落
//!   wsw_repeater_send 行 + source=repeater exchange 行 + outbound CAS。
//! - `ca status|export|regenerate`：CA 状态/导出/重生成（导出与重生成均为
//!   显式用户动作——API 带 confirm 语义，本面不静默安装、不碰系统代理/信任库）。
//! - `db migrate`：迁移执行（只在开发库副本/测试内存库；platform*.db 禁触）。
//!
//! 用法示例（S8 实拍）：
//! `rp-wsw listener --bind 127.0.0.1:3128 --db rip-data/wsw-e2e.db
//!   --rules-json '{"include":["127.0.0.1"],"allow_private_targets":true}'
//!   --status-file rip-data/wsw-state/listener-status.json`
//!
//! `rp-wsw send --db rip-data/wsw-e2e.db --session-id wsw-ses-cli-0001
//!   --tab-id wsw-rpt-1 --send-id wsw-snd-1 --version-no 1
//!   --request-json /tmp/outbound.json --policy /tmp/policy.json`

use hudsucker::rustls::crypto::aws_lc_rs;
use std::path::PathBuf;
use wsw_runtime::listener::{handler::InterceptMatcher, RuntimeOptions};
use wsw_runtime::scope::ScopeRules;
use wsw_runtime::{authority, listener, persist, ListenerConfig};

const TAG: &str = "[rp-wsw]";

use tracing::error as tracing_error;

fn usage() -> ! {
    eprintln!(
        "{TAG} usage:\n  rp-wsw listener --bind 127.0.0.1:3128 [--db <path>] [--ca-dir <dir>] \
[--rules-json <json>] [--scope-rules-file <path>] [--intercept] [--matcher-hosts a,b] \
[--matcher-paths /p59c] [--queue-ttl <secs>] [--status-file <path>] [--pid-file <path>] \
[--upstream-ca <pem>]... [--no-capture]\n  \
rp-wsw send --db <path> --session-id <ses> --tab-id <rpt> --send-id <snd> --version-no <n> \
--request-json <file> [--policy <file>] [--upstream-ca <pem>]... [--artifacts-root <dir>] \
[--pid-file <path>]\n  \
rp-wsw fuzz --db <path> --session-id <ses> --spec-json <file> [--job-id <fzj>] \
[--policy <file>] [--upstream-ca <pem>]... [--artifacts-root <dir>] [--pid-file <path>]\n  \
rp-wsw ca status [--ca-dir <dir>]\n  \
rp-wsw ca export [--ca-dir <dir>] --out <path>\n  \
rp-wsw ca regenerate [--ca-dir <dir>]\n  \
rp-wsw db migrate --db <path>\n  \
rp-wsw redact-headers（stdin 头数组 JSON → stdout 脱敏投影）"
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
    // send 子命令
    session_id: Option<String>,
    tab_id: Option<String>,
    send_id: Option<String>,
    version_no: Option<i64>,
    request_json: Option<PathBuf>,
    policy_file: Option<PathBuf>,
    artifacts_root: Option<PathBuf>,
    // fuzz 子命令（WSW-4）
    job_id: Option<String>,
    spec_json: Option<PathBuf>,
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
        session_id: None,
        tab_id: None,
        send_id: None,
        version_no: None,
        request_json: None,
        policy_file: None,
        artifacts_root: None,
        job_id: None,
        spec_json: None,
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
            "--session-id" => a.session_id = Some(take("--session-id")),
            "--tab-id" => a.tab_id = Some(take("--tab-id")),
            "--send-id" => a.send_id = Some(take("--send-id")),
            "--version-no" => a.version_no = Some(take("--version-no").parse().expect("version no")),
            "--request-json" => a.request_json = Some(take("--request-json").into()),
            "--policy" => a.policy_file = Some(take("--policy").into()),
            "--artifacts-root" => a.artifacts_root = Some(take("--artifacts-root").into()),
            "--job-id" => a.job_id = Some(take("--job-id")),
            "--spec-json" => a.spec_json = Some(take("--spec-json").into()),
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
            persist_dropped_total: shared
                .counters
                .persist_dropped_total
                .load(std::sync::atomic::Ordering::Relaxed),
            persist_enqueued_total: shared
                .counters
                .persist_enqueued_total
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

/// fuzz 子命令（WSW-4）：spec 预检（fail-closed，失败不建行）→ job 行
/// CREATED→QUEUED → JobRunner 全链（九态状态机 + 预算四闸 + 发送引擎单点
/// sender::send_request）；SIGTERM/SIGINT → CANCELLING→CANCELLED graceful；
/// stdout 输出终态汇总 JSON（rp_server 侧消费）。幂等防呆沿 run_send :
/// 379-386 形态：--job-id 行已终态 → 直接回显退出；非终态 → 续派
/// （case 行 status 权威——PENDING 未派、SENT/终态不重派，D5/判据 8）。
async fn run_fuzz(a: Args) -> Result<(), Box<dyn std::error::Error>> {
    let db = a.db.clone().ok_or("fuzz requires --db <path>")?;
    let session_id = a.session_id.clone().ok_or("fuzz requires --session-id")?;
    let spec_path = a.spec_json.clone().ok_or("fuzz requires --spec-json")?;
    let artifacts_root = a
        .artifacts_root
        .clone()
        .unwrap_or_else(wsw_message::cas::resolve_artifacts_dir);

    let store = persist::PersistStore::open_file(&db)?;
    let migrations_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations");
    store.run_migrations(&migrations_dir)?;
    store.assert_watermark()?;

    // 幂等/恢复分派。
    let job_id = match &a.job_id {
        Some(jid) if store.get_fuzz_job(jid)?.is_some() => jid.clone(),
        other => {
            let jid = other
                .clone()
                .unwrap_or_else(|| wsw_message::ids::FuzzJobId::generate().into_inner());
            // 新 job：spec 预检先行（失败不建行——fail-closed）。
            let spec_text = std::fs::read_to_string(&spec_path)
                .map_err(|e| format!("spec read {}: {e}", spec_path.display()))?;
            let prepared = wsw_runtime::fuzz::spec::prepare_spec(&spec_text)
                .map_err(|e| format!("spec preflight: {e}"))?;
            let row = wsw_runtime::fuzz::spec::job_row_from_prepared(
                &prepared,
                &jid,
                &session_id,
                &spec_text,
                persist::now_iso(),
            );
            store.insert_fuzz_job(&row)?;
            store.advance_fuzz_job(&jid, "CREATED", &persist::FuzzJobPatch {
                state: "QUEUED",
                ..Default::default()
            })?;
            jid
        }
    };

    // 既有终态行 → 幂等回显退出（重放调用防呆）。
    if let Some(existing) = store.get_fuzz_job(&job_id)? {
        if matches!(
            existing.state.as_str(),
            "CANCELLED" | "COMPLETED" | "FAILED" | "BUDGET_EXHAUSTED"
        ) {
            println!(
                "{}",
                serde_json::json!({"job_id": job_id, "state": existing.state, "note": "already terminal"})
            );
            return Ok(());
        }
    }

    if let Some(pid_file) = &a.pid_file {
        std::fs::write(pid_file, format!("{}\n", std::process::id()))?;
    }

    // SIGTERM/SIGINT → cancel watch（graceful：在途 case 等终态 → CANCELLED）。
    let (cancel_tx, cancel_rx) = tokio::sync::watch::channel(false);
    let (_, pause_rx) = tokio::sync::watch::channel(false);
    let mut sigterm = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
    let mut sigint = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())?;
    tokio::spawn(async move {
        tokio::select! {
            _ = sigterm.recv() => println!("{TAG} SIGTERM received (fuzz job cancelling)"),
            _ = sigint.recv() => println!("{TAG} SIGINT received (fuzz job cancelling)"),
        }
        let _ = cancel_tx.send(true);
    });

    // scope 规则源：wsw_session 行（缺行 = default deny——安全默认，同 send）。
    let rules = store
        .get_session(&session_id)?
        .map(|s| ScopeRules::from_json(&s.scope_rules_json))
        .transpose()?
        .unwrap_or_default();
    let policy = match &a.policy_file {
        Some(f) => {
            let text = std::fs::read_to_string(f)?;
            wsw_runtime::sender::SendPolicy::from_json(&text)?
        }
        None => wsw_runtime::sender::SendPolicy::default(),
    };
    let opts = wsw_runtime::fuzz::FuzzJobOptions {
        session_id,
        rules,
        policy,
        artifacts_root,
        extra_ca: a.upstream_cas.clone(),
        cancel: cancel_rx,
        pause: pause_rx,
    };
    match wsw_runtime::fuzz::run_job(&store, &job_id, opts).await {
        Ok(summary) => {
            println!("{}", serde_json::to_string(&summary)?);
            Ok(())
        }
        Err(wsw_runtime::fuzz::FuzzError::JobTerminal(state)) => {
            // 终态竞态（并发重放）：幂等回显。
            println!(
                "{}",
                serde_json::json!({"job_id": job_id, "state": state, "note": "already terminal"})
            );
            Ok(())
        }
        Err(wsw_runtime::fuzz::FuzzError::Spec(e)) => {
            // spec 面失败（resume 期词表文件丢失等）→ FAILED 收敛。
            let msg = e.to_string();
            if let Some(row) = store.get_fuzz_job(&job_id)? {
                if !matches!(
                    row.state.as_str(),
                    "CANCELLED" | "COMPLETED" | "FAILED" | "BUDGET_EXHAUSTED"
                ) {
                    let _ = store.advance_fuzz_job(&job_id, &row.state, &persist::FuzzJobPatch {
                        state: "FAILED",
                        error_code: Some("fuzz_spec_invalid".into()),
                        completed_at: Some(persist::now_iso()),
                        ..Default::default()
                    });
                }
            }
            Err(format!("fuzz spec invalid: {msg}").into())
        }
        Err(e) => Err(format!("fuzz run: {e}").into()),
    }
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

/// send 子命令（W3-D4）：pending→sending→终态自落 + 引擎发送 + 落库收尾；
/// SIGTERM → cancelled（非终态时）。stdout 输出终态 JSON（rp_server 侧消费）。
async fn run_send(a: Args) -> Result<(), Box<dyn std::error::Error>> {
    let db = a.db.clone().ok_or("send requires --db <path>")?;
    let session_id = a.session_id.clone().ok_or("send requires --session-id")?;
    let tab_id = a.tab_id.clone().ok_or("send requires --tab-id")?;
    let send_id = a.send_id.clone().ok_or("send requires --send-id")?;
    let version_no = a.version_no.ok_or("send requires --version-no")?;
    let artifacts_root = a
        .artifacts_root
        .clone()
        .unwrap_or_else(wsw_message::cas::resolve_artifacts_dir);

    let store = persist::PersistStore::open_file(&db)?;
    let migrations_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations");
    store.run_migrations(&migrations_dir)?;
    store.assert_watermark()?;

    // 幂等防呆：已有终态行 = 重放调用，直接回显终态退出。
    if let Some(existing) = store.get_send(&send_id)? {
        if !matches!(existing.state.as_str(), "pending" | "sending") {
            println!(
                "{}",
                serde_json::json!({"send_id": send_id, "state": existing.state, "note": "already terminal"})
            );
            return Ok(());
        }
    } else {
        store.insert_send(&persist::SendRow {
            send_id: send_id.clone(),
            tab_id: tab_id.clone(),
            version_no,
            session_id: session_id.clone(),
            state: "pending".to_string(),
            created_at: persist::now_iso(),
            ..Default::default()
        })?;
    }
    if let Some(pid_file) = &a.pid_file {
        std::fs::write(pid_file, format!("{}\n", std::process::id()))?;
    }

    // 预飞面（rules 读取 / request-json 解析 / policy 解析 / sending 推进）：
    // 任一失败 = send 行落 failed 终态再退出（六态收敛：禁 pending 悬挂——
    // 此前解析坏输入直接 return Err，行永久滞留 pending；e2e 实测暴露面）。
    let preflight = || -> Result<
        (ScopeRules, wsw_message::serialize::OutboundRequest, wsw_runtime::sender::SendPolicy),
        (String, String),
    > {
        // scope 规则源：wsw_session 行（缺行 = default deny——安全默认）。
        let rules = store
            .get_session(&session_id)
            .map_err(|e| ("preflight_failed".to_string(), e.to_string()))?
            .map(|s| ScopeRules::from_json(&s.scope_rules_json))
            .transpose()
            .map_err(|e| ("preflight_failed".to_string(), e.to_string()))?
            .unwrap_or_default();

        // 发送面：request-json 显式优先——rp_server 已从版本行读出落盘。
        let request: wsw_message::serialize::OutboundRequest = match &a.request_json {
            Some(f) => {
                let text = std::fs::read_to_string(f)
                    .map_err(|e| ("request_invalid".to_string(), format!("read {}: {e}", f.display())))?;
                serde_json::from_str(&text)
                    .map_err(|e| ("request_invalid".to_string(), e.to_string()))?
            }
            None => {
                return Err(("request_invalid".to_string(), "send requires --request-json".into()))
            }
        };
        let policy = match &a.policy_file {
            Some(f) => {
                let text = std::fs::read_to_string(f)
                    .map_err(|e| ("preflight_failed".to_string(), format!("read {}: {e}", f.display())))?;
                wsw_runtime::sender::SendPolicy::from_json(&text)
                    .map_err(|e| ("preflight_failed".to_string(), e.to_string()))?
            }
            None => wsw_runtime::sender::SendPolicy::default(),
        };

        // 发送中态推进。
        store
            .advance_send(
                &send_id,
                &persist::SendPatch {
                    state: "sending",
                    started_at: Some(persist::now_iso()),
                    ..Default::default()
                },
            )
            .map_err(|e| ("preflight_failed".to_string(), e.to_string()))?;
        Ok((rules, request, policy))
    };
    let (rules, request, policy) = match preflight() {
        Ok(v) => v,
        Err((code, reason)) => {
            let o = wsw_runtime::sender::SendOutcome {
                state: "failed",
                error_code: Some(code),
                reason: Some(reason.clone()),
                response_status: None,
                response_bytes: Vec::new(),
                parsed: None,
                redirect_chain: Vec::new(),
                outbound_sha256: None,
                outbound_bytes: Vec::new(),
                remote_ip: None,
                duration_ms: 0,
                ttfb_ms: None,
            };
            wsw_runtime::sender::persist_send_failure(&store, &send_id, &o)?;
            tracing_error!(class = "send_preflight",
                error_code = %o.error_code.clone().unwrap_or_default(),
                reason = %reason, send_id = %send_id, "send preflight failed (row landed failed)");
            println!(
                "{}",
                serde_json::json!({"send_id": send_id, "state": "failed", "error_code": o.error_code})
            );
            return Err(format!("send preflight failed: {reason}").into());
        }
    };
    let _ = version_no;

    // SIGTERM/SIGINT → cancelled watch（graceful：非终态时落 cancelled 终态）。
    let (cancel_tx, cancel_rx) = tokio::sync::watch::channel(false);
    let mut sigterm = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
    let mut sigint = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())?;
    tokio::spawn(async move {
        tokio::select! {
            _ = sigterm.recv() => println!("{TAG} SIGTERM received (send cancelled)"),
            _ = sigint.recv() => println!("{TAG} SIGINT received (send cancelled)"),
        }
        let _ = cancel_tx.send(true);
    });

    // 引擎发送。
    let outcome =
        wsw_runtime::sender::send_request(&rules, &request, &policy, &a.upstream_cas, &cancel_rx)
            .await;
    let (outcome, exchange_id) = match outcome {
        Ok(outcome) => {
            let exchange_id = wsw_runtime::sender::persist_send_result(
                &store,
                &artifacts_root,
                &send_id,
                &session_id,
                "wsw-lis-repeater",
                &tab_id,
                version_no,
                &request,
                &outcome,
            )?;
            (outcome, exchange_id)
        }
        Err(wsw_runtime::sender::SendError::Cancelled) => {
            let o = wsw_runtime::sender::SendOutcome {
                state: "cancelled",
                error_code: Some("cancelled".into()),
                reason: None,
                response_status: None,
                response_bytes: Vec::new(),
                parsed: None,
                redirect_chain: Vec::new(),
                outbound_sha256: None,
                outbound_bytes: Vec::new(),
                remote_ip: None,
                duration_ms: 0,
                ttfb_ms: None,
            };
            wsw_runtime::sender::persist_send_failure(&store, &send_id, &o)?;
            (o, None)
        }
        Err(wsw_runtime::sender::SendError::Timeout(m)) => {
            let o = wsw_runtime::sender::SendOutcome {
                state: "timed_out",
                error_code: Some("timed_out".into()),
                reason: Some(m),
                response_status: None,
                response_bytes: Vec::new(),
                parsed: None,
                redirect_chain: Vec::new(),
                outbound_sha256: None,
                outbound_bytes: Vec::new(),
                remote_ip: None,
                duration_ms: 0,
                ttfb_ms: None,
            };
            wsw_runtime::sender::persist_send_failure(&store, &send_id, &o)?;
            (o, None)
        }
        Err(wsw_runtime::sender::SendError::Scope(reason)) => {
            // scope 拒发：state=failed + error_code=scope_blocked（PLAN :528
            // 越界拒绝记录可见；reason 串落日志不落列）。
            tracing_error!(class = "scope_blocked", reason = %reason, send_id = %send_id, "send rejected by scope gate");
            let o = wsw_runtime::sender::SendOutcome {
                state: "failed",
                error_code: Some("scope_blocked".into()),
                reason: Some(reason),
                response_status: None,
                response_bytes: Vec::new(),
                parsed: None,
                redirect_chain: Vec::new(),
                outbound_sha256: None,
                outbound_bytes: Vec::new(),
                remote_ip: None,
                duration_ms: 0,
                ttfb_ms: None,
            };
            wsw_runtime::sender::persist_send_failure(&store, &send_id, &o)?;
            (o, None)
        }
        Err(e) => {
            let code = match &e {
                wsw_runtime::sender::SendError::Serialize(_) => "serialize_failed",
                wsw_runtime::sender::SendError::Connect(_) => "connect_failed",
                wsw_runtime::sender::SendError::Tls(_) => "tls_failed",
                wsw_runtime::sender::SendError::Read(_) => "read_failed",
                wsw_runtime::sender::SendError::CapExceeded(_) => "cap_exceeded",
                wsw_runtime::sender::SendError::Scope(_) => "scope_blocked",
                wsw_runtime::sender::SendError::Cancelled => "cancelled",
                wsw_runtime::sender::SendError::Timeout(_) => "timed_out",
            };
            let o = wsw_runtime::sender::SendOutcome {
                state: "failed",
                error_code: Some(code.into()),
                reason: Some(e.to_string()),
                response_status: None,
                response_bytes: Vec::new(),
                parsed: None,
                redirect_chain: Vec::new(),
                outbound_sha256: None,
                outbound_bytes: Vec::new(),
                remote_ip: None,
                duration_ms: 0,
                ttfb_ms: None,
            };
            wsw_runtime::sender::persist_send_failure(&store, &send_id, &o)?;
            (o, None)
        }
    };
    let summary = serde_json::json!({
        "send_id": send_id,
        "state": outcome.state,
        "error_code": outcome.error_code,
        "exchange_id": exchange_id,
        "response_status": outcome.response_status,
        "redirect_hops": outcome.redirect_chain.len(),
        "duration_ms": outcome.duration_ms,
    });
    println!("{summary}");
    if let Some(pid_file) = &a.pid_file {
        let _ = std::fs::remove_file(pid_file);
    }
    Ok(())
}

/// redact-headers 展示投影面（W3-D7/红线 10）：stdin 头数组 JSON →
/// `wsw_message::redact`（redact.rs:55 唯一挂点，实现零复制）→ stdout
/// `{"headers":[…],"redacted_header_names":[…]}`。rp_server detail 端点
/// 展示投影消费（原始行面不动，原始与脱敏展示分离）；凭据值永不进展示层。
fn redact_headers_json(input: &str) -> Result<String, Box<dyn std::error::Error>> {
    #[derive(serde::Deserialize)]
    struct HeadersIn {
        headers: Vec<wsw_message::types::HeaderRecord>,
    }
    let parsed_in: HeadersIn = serde_json::from_str(input)?;
    let msg = wsw_message::types::ParsedMessage {
        kind: wsw_message::types::MessageKind::Request,
        method: Some("GET".into()),
        target: Some("/".into()),
        status: None,
        reason: None,
        version: wsw_message::types::HttpVersion::Http11,
        headers: parsed_in.headers,
        trailers: Vec::new(),
        body: Vec::new(),
        delimiting: wsw_message::types::BodyDelimiting::ContentLength,
    };
    let display = wsw_message::redact::redact(&msg, &wsw_message::redact::RedactRules::default());
    Ok(serde_json::json!({
        "headers": display.headers,
        "redacted_header_names": display.redacted_header_names,
    })
    .to_string())
}

fn run_redact_headers() -> Result<(), Box<dyn std::error::Error>> {
    use std::io::Read as _;
    let mut input = String::new();
    std::io::stdin()
        .read_to_string(&mut input)
        .map_err(|e| format!("read stdin: {e}"))?;
    println!("{}", redact_headers_json(&input)?);
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
        "send" => run_send(parse_args(&mut args)).await,
        "fuzz" => run_fuzz(parse_args(&mut args)).await,
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
        "redact-headers" => run_redact_headers(),
        _ => usage(),
    };
    if let Err(e) = result {
        eprintln!("{TAG} error: {e}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redact_headers_masks_credentials_and_keeps_others() {
        // 脱敏面单测（wsw_message::redact 唯一挂点消费；红线 10：凭据值
        // 不进展示投影——命中头值替换掩码，未命中头值原样，名/位保序）。
        let input = r#"{"headers":[
            {"name":"Authorization","value":"Bearer wsw3-secret-token"},
            {"name":"Cookie","value":"session=abc123; theme=dark"},
            {"name":"Accept","value":"text/html"},
            {"name":"authorization","value":"Bearer lowercase-dup"}]}"#;
        let out = redact_headers_json(input).expect("redact projection");
        assert!(out.contains("[REDACTED]"), "命中头值替换掩码");
        assert!(!out.contains("wsw3-secret-token"), "凭据值不进展示层");
        assert!(!out.contains("session=abc123"), "Cookie 值不进展示层");
        assert!(out.contains("text/html"), "未命中头值原样保留");
        assert!(out.contains("\"redacted_header_names\""), "脱敏头名清单在场");
        // 大小写不敏感命中（Authorization/authorization 各记一条，保序去重）。
        assert!(out.contains("authorization"), "命中头名清单含小写形式");
    }
}
