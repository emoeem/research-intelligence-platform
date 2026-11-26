//! `rp-wsw`：WSW-2 Listener/CA/DB 命令行面 + WSW-3 send 子命令（Rust-First；
//! rp_server 子进程消费，同 rp-doc/rp-workflow 先例）。
//!
//! 子命令：
//! - `listener`：跑 MITM 代理（loopback 拒绑 + Scope 闸 + 024 持久化 +
//!   状态心跳文件）；SIGTERM/SIGINT 优雅停机（拦截队列全 EXPIRED）。
//! - `send`：Repeater 发送（W3-D4）：Outbound → serialize → h1 字节写 →
//!   parse_wire；SIGTERM → cancelled 终态落行；pending→sending→终态自落
//!   wsw_repeater_send 行 + source=repeater exchange 行 + outbound CAS。
//! - `ws connect`：主动 WS 客户端连靶机发单帧（WSW-5 D4；scope 闸前置，
//!   ws/wss scheme 映射 http/https 复走既有查点 A 词形——scope.rs 零 diff）。
//! - `ws replay`：读 030 帧行与靶机直连重发单帧（编辑/原样两态，D4）。
//! - `decode`：Decoder 变换链单步/多级执行（WSW-6 D5；词表 12/17 项，
//!   mutator.rs 既有 pub 纯函数面消费 + 逆变换 std 纯函数）。
//! - `compare`：Comparer 两对象 diff（WSW-6 D6；text/json/header/hex 四面 +
//!   regex 忽略维，diff 原语零 diff 消费）。
//! - `ca status|export|regenerate`：CA 状态/导出/重生成（导出与重生成均为
//!   显式用户动作——API 带 confirm 语义，本面不静默安装、不碰系统代理/信任库）。
//! - `db migrate`：迁移执行（只在开发库副本/测试内存库；platform*.db 禁触）。
//! - `scan nuclei|zap|rtis`：三 Scanner Adapter 统一入口（WSW-7 D7①；
//!   wsw_runtime::scanner 既有派发链消费——execute_nuclei/execute_zap 经
//!   tool-engine Engine+podman_host Adapter 容器链、rtis 走 rt.py poc
//!   get/use 两接口；五态投影 = evaluator::evaluate 复用，命中 ≠ CONFIRMED）。
//!
//! 用法示例（S8 实拍）：
//! `rp-wsw listener --bind 127.0.0.1:3128 --db rip-data/wsw-e2e.db
//!   --rules-json '{"include":["127.0.0.1"],"allow_private_targets":true}'
//!   --status-file rip-data/wsw-state/listener-status.json`
//!
//! `rp-wsw send --db rip-data/wsw-e2e.db --session-id wsw-ses-cli-0001
//!   --tab-id wsw-rpt-1 --send-id wsw-snd-1 --version-no 1
//!   --request-json /tmp/outbound.json --policy /tmp/policy.json`
//!
//! `rp-wsw ws connect --url ws://127.0.0.1:8765/echo --payload "hi"
//!   --db rip-data/wsw-e2e.db --session-id wsw-ses-cli-0001`
//!
//! `rp-wsw ws replay --db rip-data/wsw-e2e.db --frame-id wsw-fr-...`
//!
//! `rp-wsw decode --ops url_decode,hex_decode --input "%32%33"`
//!
//! `rp-wsw compare --mode text --left-file a.txt --right-file b.txt
//!   --skip-line-regex "^ts:" --db rip-data/wsw-e2e.db`

use hudsucker::rustls::crypto::aws_lc_rs;
use std::path::{Path, PathBuf};
use wsw_runtime::listener::{handler::InterceptMatcher, RuntimeOptions};
use wsw_runtime::scope::ScopeRules;
use wsw_runtime::{authority, listener, persist, ListenerConfig};

const TAG: &str = "[rp-wsw]";

use tracing::error as tracing_error;

fn usage_text() -> String {
    format!(
        "{TAG} usage:\n  rp-wsw listener --bind 127.0.0.1:3128 [--db <path>] [--ca-dir <dir>] \
[--rules-json <json>] [--scope-rules-file <path>] [--intercept] [--matcher-hosts a,b] \
[--matcher-paths /p59c] [--queue-ttl <secs>] [--status-file <path>] [--pid-file <path>] \
[--upstream-ca <pem>]... [--no-capture]\n  \
rp-wsw send --db <path> --session-id <ses> --tab-id <rpt> --send-id <snd> --version-no <n> \
--request-json <file> [--policy <file>] [--upstream-ca <pem>]... [--artifacts-root <dir>] \
[--pid-file <path>]\n  \
rp-wsw fuzz --db <path> --session-id <ses> --spec-json <file> [--job-id <fzj>] \
[--policy <file>] [--upstream-ca <pem>]... [--artifacts-root <dir>] [--pid-file <path>]\n  \
rp-wsw ws connect --url ws://host:port/path [--payload <text>] [--binary-hex <hex>] \
[--opcode text|binary|ping] [--db <path>] [--session-id <ses>] [--rules-json <json>] \
[--echo-timeout <secs>]\n  \
rp-wsw ws replay --db <path> --frame-id <wsw-fr-...> [--payload <text>] [--binary-hex <hex>] \
[--target ws://host:port/path] [--session-id <ses>] [--rules-json <json>] [--artifacts-root <dir>] \
[--echo-timeout <secs>]\n  \
rp-wsw decode [--input <str> | --input-file <path> | --input-base64 <b64>] --ops url_encode,b64_encode,... \
[--out-file <path>]\n  \
rp-wsw compare --mode text|json|header|hex (--left <str>|--left-file <p>|--left-exchange <id> \
|--left-frame <id>) (same for --right*) [--direction request|response] [--db <path>] \
[--artifacts-root <dir>] [--skip-line-regex r1,r2] [--normalize-key-regex r1,r2] [--save-artifact] \
[--out-file <path>]\n  \
rp-wsw ca status [--ca-dir <dir>]\n  \
rp-wsw ca export [--ca-dir <dir>] --out <path>\n  \
rp-wsw ca regenerate [--ca-dir <dir>]\n  \
rp-wsw db migrate --db <path>\n  \
rp-wsw scan nuclei --url <u> [--template-ref rtis-poc:N | --template-file <p>] \
[--rate N --max-requests N --timeout-s S] [--db <path>] [--task-id <id>] [--trace-id <id>] \
[--run-id <id>]（WSW-7；rate 与 max-requests 同钮——P5 request_budget 单钮语义，同发 max-requests 优先）\n  \
rp-wsw scan zap --url <u> [--timeout-s S] [--alert-budget N] [--db <path>] [--task-id <id>] \
[--trace-id <id>] [--run-id <id>]（WSW-7；scan_mode 词表单值 baseline）\n  \
rp-wsw scan rtis --poc-id N --target <t>（WSW-7；poc get 元数据锚 + poc use 回填——零扫描执行面）\n  \
rp-wsw redact-headers（stdin 头数组 JSON → stdout 脱敏投影）\n  \
rp-wsw browser capture --db <path> --url <u> [--mode viewport|full_page|element|region] \
[--selector <css>] [--clip-json '{{\"x\":0,\"y\":0,\"width\":100,\"height\":100}}'] [--settle-ms N] \
[--scope-rules-json <json>] [--timeout-s S] [--task-id <id>] [--trace-id <id>] \
[--run-id <id>]（WSW-8；截图四模式 + DOMSnapshot + 页面文本 + 网络摘要；evidence 'screenshot' 终局写入）"
    )
}

/// 误用面（stderr + exit 2；P9-3 统一口径）。
fn usage() -> ! {
    eprintln!("{}", usage_text());
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
    // ws 子命令（WSW-5）
    url: Option<String>,
    payload: Option<String>,
    binary_hex: Option<String>,
    opcode: Option<String>,
    echo_timeout: Option<u64>,
    target: Option<String>,
    frame_id: Option<String>,
    // decode 子命令（WSW-6）
    input: Option<String>,
    input_file: Option<PathBuf>,
    input_base64: Option<String>,
    ops: Option<String>,
    // compare 子命令（WSW-6）
    mode: Option<String>,
    left: Option<String>,
    left_file: Option<PathBuf>,
    left_exchange: Option<String>,
    left_frame: Option<String>,
    right: Option<String>,
    right_file: Option<PathBuf>,
    right_exchange: Option<String>,
    right_frame: Option<String>,
    direction: Option<String>,
    skip_line_regexes: Option<String>,
    normalize_key_regexes: Option<String>,
    save_artifact: bool,
    out_file: Option<PathBuf>,
    // scan 子命令（WSW-7 D7①）
    poc_id: Option<i64>,
    // browser 子命令（WSW-8 D8②；mode 复用 compare 既有字段——子命令互斥面）
    selector: Option<String>,
    clip_json: Option<String>,
    settle_ms: Option<u64>,
    scope_rules_json: Option<String>,
    rate: Option<i64>,
    max_requests: Option<i64>,
    timeout_s: Option<i64>,
    alert_budget: Option<i64>,
    template_ref: Option<String>,
    template_file: Option<PathBuf>,
    task_id: Option<String>,
    trace_id: Option<String>,
    run_id: Option<String>,
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
        url: None,
        payload: None,
        binary_hex: None,
        opcode: None,
        echo_timeout: None,
        target: None,
        frame_id: None,
        input: None,
        input_file: None,
        input_base64: None,
        ops: None,
        mode: None,
        left: None,
        left_file: None,
        left_exchange: None,
        left_frame: None,
        right: None,
        right_file: None,
        right_exchange: None,
        right_frame: None,
        direction: None,
        skip_line_regexes: None,
        normalize_key_regexes: None,
        save_artifact: false,
        out_file: None,
        poc_id: None,
        selector: None,
        clip_json: None,
        settle_ms: None,
        scope_rules_json: None,
        rate: None,
        max_requests: None,
        timeout_s: None,
        alert_budget: None,
        template_ref: None,
        template_file: None,
        task_id: None,
        trace_id: None,
        run_id: None,
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
            "--version-no" => {
                a.version_no = Some(take("--version-no").parse().expect("version no"))
            }
            "--request-json" => a.request_json = Some(take("--request-json").into()),
            "--policy" => a.policy_file = Some(take("--policy").into()),
            "--artifacts-root" => a.artifacts_root = Some(take("--artifacts-root").into()),
            "--job-id" => a.job_id = Some(take("--job-id")),
            "--spec-json" => a.spec_json = Some(take("--spec-json").into()),
            "--url" => a.url = Some(take("--url")),
            "--payload" => a.payload = Some(take("--payload")),
            "--binary-hex" => a.binary_hex = Some(take("--binary-hex")),
            "--opcode" => a.opcode = Some(take("--opcode")),
            "--echo-timeout" => {
                a.echo_timeout = Some(take("--echo-timeout").parse().expect("secs"))
            }
            "--target" => a.target = Some(take("--target")),
            "--frame-id" => a.frame_id = Some(take("--frame-id")),
            "--input" => a.input = Some(take("--input")),
            "--input-file" => a.input_file = Some(take("--input-file").into()),
            "--input-base64" => a.input_base64 = Some(take("--input-base64")),
            "--ops" => a.ops = Some(take("--ops")),
            "--mode" => a.mode = Some(take("--mode")),
            "--left" => a.left = Some(take("--left")),
            "--left-file" => a.left_file = Some(take("--left-file").into()),
            "--left-exchange" => a.left_exchange = Some(take("--left-exchange")),
            "--left-frame" => a.left_frame = Some(take("--left-frame")),
            "--right" => a.right = Some(take("--right")),
            "--right-file" => a.right_file = Some(take("--right-file").into()),
            "--right-exchange" => a.right_exchange = Some(take("--right-exchange")),
            "--right-frame" => a.right_frame = Some(take("--right-frame")),
            "--direction" => a.direction = Some(take("--direction")),
            "--skip-line-regex" => a.skip_line_regexes = Some(take("--skip-line-regex")),
            "--normalize-key-regex" => {
                a.normalize_key_regexes = Some(take("--normalize-key-regex"))
            }
            "--save-artifact" => a.save_artifact = true,
            "--out-file" => a.out_file = Some(take("--out-file").into()),
            // scan 子命令旗标（WSW-7 D7①）
            "--poc-id" => a.poc_id = Some(take("--poc-id").parse().expect("poc id")),
            "--rate" => a.rate = Some(take("--rate").parse().expect("rate")),
            "--max-requests" => {
                a.max_requests = Some(take("--max-requests").parse().expect("max requests"))
            }
            "--timeout-s" => a.timeout_s = Some(take("--timeout-s").parse().expect("timeout s")),
            "--alert-budget" => {
                a.alert_budget = Some(take("--alert-budget").parse().expect("alert budget"))
            }
            "--template-ref" => a.template_ref = Some(take("--template-ref")),
            "--template-file" => a.template_file = Some(take("--template-file").into()),
            "--task-id" => a.task_id = Some(take("--task-id")),
            "--trace-id" => a.trace_id = Some(take("--trace-id")),
            "--run-id" => a.run_id = Some(take("--run-id")),
            // browser 子命令旗标（WSW-8 D8②）
            "--selector" => a.selector = Some(take("--selector")),
            "--clip-json" => a.clip_json = Some(take("--clip-json")),
            "--settle-ms" => a.settle_ms = Some(take("--settle-ms").parse().expect("settle ms")),
            "--scope-rules-json" => a.scope_rules_json = Some(take("--scope-rules-json")),
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
            ws_connections_total: shared
                .counters
                .ws_connections_total
                .load(std::sync::atomic::Ordering::Relaxed),
            ws_frames_captured_total: shared
                .counters
                .ws_frames_captured_total
                .load(std::sync::atomic::Ordering::Relaxed),
            ws_frames_dropped_total: shared
                .counters
                .ws_frames_dropped_total
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
            store.advance_fuzz_job(
                &jid,
                "CREATED",
                &persist::FuzzJobPatch {
                    state: "QUEUED",
                    ..Default::default()
                },
            )?;
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
                    let _ = store.advance_fuzz_job(
                        &job_id,
                        &row.state,
                        &persist::FuzzJobPatch {
                            state: "FAILED",
                            error_code: Some("fuzz_spec_invalid".into()),
                            completed_at: Some(persist::now_iso()),
                            ..Default::default()
                        },
                    );
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

// ===========================================================================
// WSW-5/6 子命令（D4：独立入口 = CLI 直用 + API 直用双面，非独立端口）。
// ===========================================================================

/// ws 子命令 scope 闸前置（判据 8：ws/wss scheme 映射 http/https 复走既有
/// 查点 A 词形——scope.rs 零 diff 纯调用面）。规则源：--rules-json > wsw_session
/// 行（--db + --session-id；缺行 = default deny 安全默认，send 先例）> default deny。
fn ws_scope_rules(a: &Args, store: Option<&persist::PersistStore>) -> Result<ScopeRules, String> {
    if let Some(raw) = &a.rules_json {
        return ScopeRules::from_json(raw).map_err(|e| format!("rules-json parse: {e}"));
    }
    if let (Some(store), Some(session_id)) = (store, a.session_id.as_deref()) {
        return store
            .get_session(session_id)
            .map_err(|e| format!("session read: {e}"))?
            .map(|s| ScopeRules::from_json(&s.scope_rules_json))
            .transpose()
            .map_err(|e| format!("session rules parse: {e}"))?
            .ok_or_else(|| "session not found (default deny)".to_string());
    }
    Ok(ScopeRules::default())
}

/// url 词形 → host/port/path + scope 映射 scheme（ws→http、wss→https）。
fn parse_ws_url(url: &str) -> Result<(String, u16, String, &'static str), String> {
    let parsed: hudsucker::hyper::Uri = url.parse().map_err(|e| format!("--url parse: {e}"))?;
    let scheme = parsed.scheme_str().ok_or("--url missing scheme")?;
    let host = parsed.host().ok_or("--url missing host")?.to_string();
    let port = parsed.port_u16().unwrap_or(match scheme {
        "wss" => 443,
        "ws" => 80,
        other => return Err(format!("--url scheme 词表外（ws/wss）：{other}")),
    });
    let path = if parsed.path().is_empty() {
        "/"
    } else {
        parsed.path()
    };
    let mapped = match scheme {
        "wss" => "https",
        _ => "http",
    };
    Ok((host, port, path.to_string(), mapped))
}

/// ws connect/replay 公共体：scope 闸 → tokio-tungstenite 客户端直连 → 单帧
/// 发送 → echo 等待（echo_timeout 秒；超时如实 none）→ Close 收口 → JSON
/// 摘要 stdout（rp_server 侧消费）。
async fn ws_send_one(
    url: &str,
    opcode_word: &str,
    payload: Vec<u8>,
    echo_timeout_secs: u64,
) -> Result<serde_json::Value, String> {
    use hudsucker::futures::{SinkExt, StreamExt};
    use hudsucker::tokio_tungstenite::tungstenite::Message;
    let (ws, _resp) = hudsucker::tokio_tungstenite::connect_async(url)
        .await
        .map_err(|e| format!("ws connect: {e}"))?;
    let started = std::time::Instant::now();
    let (mut sink, mut stream) = ws.split();
    let msg = match opcode_word {
        "binary" => Message::Binary(payload.clone().into()),
        "ping" => Message::Ping(payload.clone().into()),
        "text" => Message::text(String::from_utf8_lossy(&payload).to_string()),
        other => return Err(format!("opcode 词表外（text/binary/ping）：{other}")),
    };
    sink.send(msg).await.map_err(|e| format!("ws send: {e}"))?;
    // echo/回读面：timeout 内读一帧（echo 靶机回声证据面；超时 = none 如实）。
    let echo = match tokio::time::timeout(
        std::time::Duration::from_secs(echo_timeout_secs.max(1)),
        stream.next(),
    )
    .await
    {
        Ok(Some(Ok(m))) => match m {
            Message::Text(t) => {
                serde_json::json!({"opcode": "text", "len": t.len(), "text": t.to_string()})
            }
            Message::Binary(b) => serde_json::json!({"opcode": "binary", "len": b.len()}),
            Message::Pong(_) => serde_json::json!({"opcode": "pong", "len": 0}),
            Message::Ping(_) => serde_json::json!({"opcode": "ping", "len": 0}),
            Message::Close(_) => serde_json::json!({"opcode": "close", "len": 0}),
            Message::Frame(_) => serde_json::json!({"opcode": "frame", "len": 0}),
        },
        Ok(Some(Err(e))) => serde_json::json!({"error": e.to_string()}),
        Ok(None) => serde_json::json!({"opcode": "eof"}),
        Err(_) => serde_json::json!({"opcode": "none", "note": "echo timeout"}),
    };
    let _ = sink.send(Message::Close(None)).await;
    let _ = sink.close().await;
    Ok(serde_json::json!({
        "url": url,
        "sent": {"opcode": opcode_word, "len": payload.len()},
        "echo": echo,
        "duration_ms": started.elapsed().as_millis() as u64,
    }))
}

/// `ws connect`：主动 WS 客户端连靶机发单帧（D4；scope 闸前置——闸外连接
/// 前拒绝，零越闸）。
async fn run_ws_connect(a: Args) -> Result<(), Box<dyn std::error::Error>> {
    let url = a
        .url
        .clone()
        .ok_or("ws connect requires --url ws://host:port/path")?;
    let (host, port, path, mapped_scheme) = parse_ws_url(&url)?;
    // 规则源 store（--db 在场时打开读面——wsw_session 行规则源，send 先例）。
    let store = match &a.db {
        Some(db) => {
            let s = persist::PersistStore::open_file(db)?;
            let migrations_dir =
                std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations");
            s.run_migrations(&migrations_dir)?;
            Some(s)
        }
        None => None,
    };
    let rules = ws_scope_rules(&a, store.as_ref()).map_err(|e| format!("scope rules: {e}"))?;
    let target = wsw_runtime::scope::ScopeTarget {
        scheme: mapped_scheme,
        host: &host,
        port,
        path: &path,
    };
    if let wsw_runtime::scope::ScopeDecision::Blocked { reason } =
        wsw_runtime::scope::check(&rules, &target)
    {
        println!(
            "{}",
            serde_json::json!({
                "state": "scope_blocked",
                "reason_code": reason.code(),
                "url": url,
            })
        );
        std::process::exit(3);
    }
    let opcode_word = a.opcode.as_deref().unwrap_or("text");
    let payload = ws_payload_bytes(&a, opcode_word)?;
    let echo_timeout = a.echo_timeout.unwrap_or(2);
    let summary = ws_send_one(&url, opcode_word, payload, echo_timeout)
        .await
        .map_err(|e| format!("ws connect: {e}"))?;
    println!(
        "{}",
        serde_json::json!({"state": "connected", "op": "connect"})
    );
    println!("{summary}");
    Ok(())
}

/// 帧载荷解析（--binary-hex > --payload > 空；opcode=text 时 binary-hex 仍可
/// 承载任意字节面——CLI 层 utf8 校验放行 lossy，靶机面字节保真）。
fn ws_payload_bytes(a: &Args, opcode_word: &str) -> Result<Vec<u8>, String> {
    let _ = opcode_word;
    if let Some(hex) = &a.binary_hex {
        let hex = hex.trim();
        if hex.len() % 2 != 1 {
            // 偶数位才可能是合法 hex（奇数位直接拒）。
            return Ok(hex
                .as_bytes()
                .chunks(2)
                .map(|p| {
                    u8::from_str_radix(std::str::from_utf8(p).unwrap_or("zz"), 16).unwrap_or(0)
                })
                .collect());
        }
        return Err("--binary-hex odd length".into());
    }
    Ok(a.payload.clone().unwrap_or_default().into_bytes())
}

/// `ws replay`：读 030 帧行 + 握手行还原目标 → 直连重发单帧（--payload/
/// --binary-hex 编辑态 > CAS 原始 payload；判据 2 承载面）。
async fn run_ws_replay(a: Args) -> Result<(), Box<dyn std::error::Error>> {
    let db = a.db.clone().ok_or("ws replay requires --db <path>")?;
    let frame_id = a
        .frame_id
        .clone()
        .ok_or("ws replay requires --frame-id <wsw-fr-...>")?;
    let artifacts_root = a
        .artifacts_root
        .clone()
        .unwrap_or_else(wsw_message::cas::resolve_artifacts_dir);
    let store = persist::PersistStore::open_file(&db)?;
    let migrations_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations");
    store.run_migrations(&migrations_dir)?;
    store.assert_watermark()?;
    let frame = store
        .get_ws_frame(&frame_id)?
        .ok_or(format!("frame not found: {frame_id}"))?;
    let handshake = store
        .get_exchange_row(&frame.connection_exchange_id)?
        .ok_or(format!(
            "handshake row not found for {}",
            frame.connection_exchange_id
        ))?;
    let target = match &a.target {
        Some(t) => t.clone(),
        None => {
            let scheme = handshake.scheme.as_deref().unwrap_or("ws");
            let authority = handshake.authority.clone().unwrap_or_default();
            let path = handshake.path.clone().unwrap_or_else(|| "/".into());
            format!("{scheme}://{authority}{path}")
        }
    };
    let (host, port, path, mapped_scheme) = parse_ws_url(&target)?;
    let rules = ws_scope_rules(&a, Some(&store)).map_err(|e| format!("scope rules: {e}"))?;
    let target_scope = wsw_runtime::scope::ScopeTarget {
        scheme: mapped_scheme,
        host: &host,
        port,
        path: &path,
    };
    if let wsw_runtime::scope::ScopeDecision::Blocked { reason } =
        wsw_runtime::scope::check(&rules, &target_scope)
    {
        println!(
            "{}",
            serde_json::json!({
                "state": "scope_blocked",
                "reason_code": reason.code(),
                "frame_id": frame_id,
            })
        );
        std::process::exit(3);
    }

    // 载荷：--payload/--binary-hex 编辑态 > CAS 原始字节。
    let payload: Vec<u8> = if a.payload.is_some() || a.binary_hex.is_some() {
        ws_payload_bytes(&a, frame.opcode)?
    } else {
        match &frame.payload_body_uri {
            Some(uri) => wsw_message::cas::read_body(&artifacts_root, uri)
                .map_err(|e| format!("cas read {uri}: {e}"))?,
            None => Vec::new(),
        }
    };
    let opcode_word = frame.opcode;
    let url = if target.starts_with("ws://") || target.starts_with("wss://") {
        target.clone()
    } else {
        format!("ws://{host}:{port}{path}")
    };
    let echo_timeout = a.echo_timeout.unwrap_or(2);
    let summary = ws_send_one(&url, opcode_word, payload, echo_timeout)
        .await
        .map_err(|e| format!("ws replay: {e}"))?;
    println!(
        "{}",
        serde_json::json!({
            "state": "ok",
            "op": "replay",
            "frame_id": frame_id,
            "edited": a.payload.is_some() || a.binary_hex.is_some(),
        })
    );
    println!("{summary}");
    Ok(())
}

/// `decode`：变换链单步/多级执行（--ops 逗号分隔词表链；stdout JSON 摘要）。
fn run_decode(a: Args) -> Result<(), Box<dyn std::error::Error>> {
    let input = if let Some(s) = &a.input {
        s.clone()
    } else if let Some(f) = &a.input_file {
        std::fs::read_to_string(f)?
    } else if let Some(b64) = &a.input_base64 {
        let ops = [wsw_runtime::decoder::DecodeOp::B64Decode];
        wsw_runtime::decoder::apply_ops(b64.trim(), &ops)
            .map_err(|e| format!("--input-base64 decode: {e}"))?
    } else {
        return Err("decode requires --input / --input-file / --input-base64".into());
    };
    let ops_word = a.ops.clone().ok_or("decode requires --ops <word list>")?;
    let mut ops = Vec::new();
    for w in ops_word.split(',').map(str::trim).filter(|w| !w.is_empty()) {
        let op = wsw_runtime::decoder::DecodeOp::from_word(w)
            .ok_or_else(|| format!("unknown op: {w:?}（词表 = rp-wsw decode --ops-list）"))?;
        ops.push(op);
    }
    match wsw_runtime::decoder::apply_ops(&input, &ops) {
        Ok(result) => {
            let payload = serde_json::json!({"ops": ops_word, "result": result, "error": null});
            emit_json(&payload, a.out_file.as_deref())?;
            println!("{payload}");
            Ok(())
        }
        Err(e) => {
            let payload =
                serde_json::json!({"ops": ops_word, "result": null, "error": e.to_string()});
            println!("{payload}");
            Err(format!("decode failed: {e}").into())
        }
    }
}

fn emit_json(
    payload: &serde_json::Value,
    out: Option<&Path>,
) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(path) = out {
        std::fs::write(path, payload.to_string())?;
    }
    Ok(())
}

/// 比较对象源（D6：手工输入 / exchange Body CAS 回读 / 030 帧 payload）。
enum CompareSource {
    Text(String),
    CasBytes(Vec<u8>),
}

/// 比较源解析（--left/--left-file/--left-exchange/--left-frame 四形互斥）。
fn resolve_compare_source(
    side: &str,
    text: &Option<String>,
    file: &Option<PathBuf>,
    exchange: &Option<String>,
    frame: &Option<String>,
    store: Option<&persist::PersistStore>,
    artifacts_root: &Path,
) -> Result<CompareSource, String> {
    let provided = [
        text.is_some(),
        file.is_some(),
        exchange.is_some(),
        frame.is_some(),
    ]
    .iter()
    .filter(|b| **b)
    .count();
    if provided != 1 {
        return Err(format!(
            "{side} 须且仅须一个源：--{side} / --{side}-file / --{side}-exchange / --{side}-frame"
        ));
    }
    if let Some(t) = text {
        return Ok(CompareSource::Text(t.clone()));
    }
    if let Some(f) = file {
        return std::fs::read_to_string(f)
            .map(CompareSource::Text)
            .map_err(|e| format!("read {f:?}: {e}"));
    }
    let store = store.ok_or(format!("{side}-exchange/--{side}-frame 需要 --db <path>"))?;
    if let Some(ex_id) = exchange {
        let row = store
            .get_exchange_row(ex_id)
            .map_err(|e| e.to_string())?
            .ok_or(format!("exchange not found: {ex_id}"))?;
        let uri = row
            .request_body_uri
            .or(row.response_body_uri)
            .ok_or(format!("exchange {ex_id} 零 body CAS 引用"))?;
        let bytes = wsw_message::cas::read_body(artifacts_root, &uri)
            .map_err(|e| format!("cas read {uri}: {e}"))?;
        return Ok(CompareSource::CasBytes(bytes));
    }
    let fr_id = frame.as_ref().unwrap();
    let row = store
        .get_ws_frame(fr_id)
        .map_err(|e| e.to_string())?
        .ok_or(format!("frame not found: {fr_id}"))?;
    let uri = row
        .payload_body_uri
        .ok_or(format!("frame {fr_id} 零 payload CAS 引用"))?;
    let bytes = wsw_message::cas::read_body(artifacts_root, &uri)
        .map_err(|e| format!("cas read {uri}: {e}"))?;
    Ok(CompareSource::CasBytes(bytes))
}

/// `compare`：两对象 diff（四面 + regex 忽略维；stdout JSON 报告；
/// --save-artifact = 报告落 CAS + wsw_body_blob（wsw_message 既有面））。
fn run_compare(a: Args) -> Result<(), Box<dyn std::error::Error>> {
    let mode = a.mode.as_deref().unwrap_or("text");
    if !matches!(mode, "text" | "json" | "header" | "hex") {
        return Err(format!("--mode 词表外（text/json/header/hex）：{mode}").into());
    }
    let artifacts_root = a
        .artifacts_root
        .clone()
        .unwrap_or_else(wsw_message::cas::resolve_artifacts_dir);
    let store = match &a.db {
        Some(db) => {
            let s = persist::PersistStore::open_file(db)?;
            let migrations_dir =
                std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations");
            s.run_migrations(&migrations_dir)?;
            Some(s)
        }
        None => None,
    };
    if (a.left_exchange.is_some()
        || a.left_frame.is_some()
        || a.right_exchange.is_some()
        || a.right_frame.is_some())
        && store.is_none()
    {
        return Err("exchange/frame 比较源需要 --db <path>".into());
    }
    if a.save_artifact && store.is_none() {
        return Err("--save-artifact 需要 --db <path>（wsw_body_blob 登记面）".into());
    }
    let left_src = resolve_compare_source(
        "left",
        &a.left,
        &a.left_file,
        &a.left_exchange,
        &a.left_frame,
        store.as_ref(),
        &artifacts_root,
    )?;
    let right_src = resolve_compare_source(
        "right",
        &a.right,
        &a.right_file,
        &a.right_exchange,
        &a.right_frame,
        store.as_ref(),
        &artifacts_root,
    )?;
    let to_string = |src: &CompareSource| -> String {
        match src {
            CompareSource::Text(t) => t.clone(),
            CompareSource::CasBytes(b) => String::from_utf8_lossy(b).to_string(),
        }
    };
    let to_bytes = |src: &CompareSource| -> Vec<u8> {
        match src {
            CompareSource::Text(t) => t.as_bytes().to_vec(),
            CompareSource::CasBytes(b) => b.clone(),
        }
    };
    let ignore = wsw_runtime::comparer::IgnoreRules {
        skip_line_regexes: a
            .skip_line_regexes
            .as_deref()
            .unwrap_or("")
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(String::from)
            .collect(),
        normalize_key_regexes: a
            .normalize_key_regexes
            .as_deref()
            .unwrap_or("")
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(String::from)
            .collect(),
        base: Default::default(),
    }
    .compile()
    .map_err(|e| e.to_string())?;
    let report = match mode {
        "json" => {
            wsw_runtime::comparer::compare_json(&to_string(&left_src), &to_string(&right_src))
                .map_err(|e| e.to_string())?
        }
        "header" => {
            wsw_runtime::comparer::compare_headers(&to_string(&left_src), &to_string(&right_src))
                .map_err(|e| e.to_string())?
        }
        "hex" => wsw_runtime::comparer::compare_bytes(&to_bytes(&left_src), &to_bytes(&right_src)),
        _ => wsw_runtime::comparer::compare_text(
            &to_string(&left_src),
            &to_string(&right_src),
            &ignore,
        ),
    };
    let mut payload = serde_json::to_value(&report).map_err(|e| e.to_string())?;
    if let (Some(store), true) = (store.as_ref(), a.save_artifact) {
        match wsw_runtime::comparer::persist_report_as_blob(store, &artifacts_root, &report) {
            Ok(body_ref) => {
                payload["artifact"] = serde_json::json!({
                    "sha256": body_ref.sha256,
                    "uri": body_ref.uri,
                    "size": body_ref.size,
                    "fidelity": format!("{:?}", body_ref.fidelity).to_lowercase(),
                });
            }
            Err(e) => {
                payload["artifact_error"] = serde_json::json!(e);
            }
        }
    }
    emit_json(&payload, a.out_file.as_deref())?;
    println!("{payload}");
    Ok(())
}

// ---------------------------------------------------------------------------
// WSW-7 scan 子命令（D7①：三 provider 分派；wsw_runtime::scanner 既有
// 派发链消费——零第二派发实现，D3④ Resolver 闸维持 mcp-server 单点）
// ---------------------------------------------------------------------------

/// 仓库根锚（rp-verify.rs repo_root 同款：RP_REPO_ROOT 优先，缺省编译期锚）。
fn scan_repo_root() -> PathBuf {
    if let Ok(root) = std::env::var("RP_REPO_ROOT") {
        return PathBuf::from(root);
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// scan 运行库 Engine（文件库落库面——判据 1 tool_run/artifact 行对账载体；
/// registry query-first 复用登记：首次 register rp-verify engine_with_nuclei
/// 先例，重跑复用既有登记——rule 2 幂等面）。
fn scan_engine(db: &Path, repo_root: &Path) -> Result<tool_engine::Engine, String> {
    let pool = database::Pool::open(database::PoolConfig {
        path: database::DbPath::File(db.to_path_buf()),
        readonly: false,
        max_connections: 1,
        busy_timeout: std::time::Duration::from_secs(5),
    })
    .map_err(|e| format!("pool open: {e}"))?;
    database::migrate(&pool, &repo_root.join("migrations")).map_err(|e| format!("migrate: {e}"))?;
    let registry = tool_registry::Registry::open(pool.clone());
    for (tool_id, manifest_rel) in [
        (
            "rip.security.nuclei",
            "manifests/tools/rip.security.nuclei.json",
        ),
        ("rip.security.zap", "manifests/tools/rip.security.zap.json"),
    ] {
        if registry.query(tool_id).is_ok() {
            continue; // 已登记复用（同 content_hash 重登记被 rule 2 拒——幂等面）
        }
        let text = std::fs::read_to_string(repo_root.join(manifest_rel))
            .map_err(|e| format!("manifest read {manifest_rel}: {e}"))?;
        let manifest: serde_json::Value = serde_json::from_str(&text)
            .map_err(|e| format!("manifest parse {manifest_rel}: {e}"))?;
        registry
            .register(&manifest)
            .map_err(|e| format!("register {tool_id}: {e}"))?;
    }
    Ok(tool_engine::Engine::open(pool, registry))
}

/// scan 运行 ID（--run-id 显式锚优先；缺省时间微秒唯一面）。
fn scan_run_id(a: &Args) -> String {
    a.run_id.clone().unwrap_or_else(|| {
        format!(
            "rn-wsw7-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_micros()
        )
    })
}

/// scan 运行库打开失败结构化 JSON（stdout 词形回传 + exit 1）。
fn emit_scan_error(scan_error: &wsw_runtime::scanner::ScanError) {
    let payload = serde_json::json!({
        "error": {
            "category": scan_error.category,
            "message": scan_error.message,
        }
    });
    println!("{payload}");
}

/// 成功/失败 outcome 统一投影帧（provider 面 + 五态投影 normalize——
/// evaluator::evaluate 复用，命中 ≠ CONFIRMED）。
fn scan_outcome_payload(
    outcome: &verification_engine::provider::ProviderOutcome,
    normalized: &verification_engine::evaluator::NormalizedResult,
) -> serde_json::Value {
    serde_json::json!({
        "provider_id": outcome.provider_id,
        "provider_version": outcome.provider_version,
        "status_word": outcome.status_word,
        "tool_run_id": outcome.tool_run_id,
        "trace_id": outcome.trace_id,
        "result": outcome.result,
        "summary": outcome.summary,
        "error_category": outcome.error_category,
        "tool_version": outcome.tool_version,
        "rule_ref": outcome.rule_ref,
        "duration_ms": outcome.duration_ms,
        "normalized": {
            "schema_version": normalized.schema_version,
            "result_status": normalized.result_status,
            "result_reason": normalized.result_reason,
            "error_category": normalized.error_category,
        }
    })
}

/// `scan nuclei`（D7①）：真实扫描派发（local_range 靶机面——e2e 判据 1）。
fn run_scan_nuclei(a: Args) -> Result<(), Box<dyn std::error::Error>> {
    let repo_root = scan_repo_root();
    let db = a.db.clone().ok_or("scan nuclei requires --db <path>")?;
    let url = a.url.clone().ok_or("scan nuclei requires --url <u>")?;
    // 预算装配（P5 单钮语义：max_requests 优先；缺省 10/60 与 manifest default 同源）
    let max_requests = a.max_requests.unwrap_or(a.rate.unwrap_or(10));
    let timeout_s = a.timeout_s.unwrap_or(60);
    let params = wsw_runtime::scanner::NucleiScanParams {
        target: url.clone(),
        template_ref: a.template_ref.clone(),
        template_file: a.template_file.clone(),
        max_requests,
        timeout_s,
    };
    let engine = scan_engine(&db, &repo_root)?;
    let adapter = tool_engine::adapter::podman_host::PodmanHostAdapter::new(
        tool_engine::adapter::podman_host::PodmanHostConfig::default(),
    );
    let run_id = scan_run_id(&a);
    match wsw_runtime::scanner::scan_nuclei(
        &engine,
        &adapter,
        &params,
        &run_id,
        a.task_id.as_deref(),
        a.trace_id.as_deref(),
    ) {
        Ok(outcome) => {
            let normalized = wsw_runtime::scanner::project_normalized(&outcome);
            let payload = scan_outcome_payload(&outcome, &normalized);
            println!("{payload}");
            Ok(())
        }
        Err(scan_error) => {
            emit_scan_error(&scan_error);
            Err(format!("scan nuclei failed: {}", scan_error.message).into())
        }
    }
}

/// `scan zap`（D7①）：baseline 真实扫描派发（scan_mode 词表单值）。
fn run_scan_zap(a: Args) -> Result<(), Box<dyn std::error::Error>> {
    let repo_root = scan_repo_root();
    let db = a.db.clone().ok_or("scan zap requires --db <path>")?;
    let url = a.url.clone().ok_or("scan zap requires --url <u>")?;
    let timeout_s = a.timeout_s.unwrap_or(300);
    let alert_budget = a.alert_budget.unwrap_or(100);
    let engine = scan_engine(&db, &repo_root)?;
    let adapter = tool_engine::adapter::podman_host::PodmanHostAdapter::new(
        tool_engine::adapter::podman_host::PodmanHostConfig::default(),
    );
    let run_id = scan_run_id(&a);
    let params = wsw_runtime::scanner::ZapScanParams {
        target: url,
        timeout_s,
        alert_budget,
    };
    match wsw_runtime::scanner::scan_zap(
        &engine,
        &adapter,
        &params,
        &run_id,
        a.task_id.as_deref(),
        a.trace_id.as_deref(),
    ) {
        Ok(outcome) => {
            let normalized = wsw_runtime::scanner::project_normalized(&outcome);
            let payload = scan_outcome_payload(&outcome, &normalized);
            println!("{payload}");
            Ok(())
        }
        Err(scan_error) => {
            emit_scan_error(&scan_error);
            Err(format!("scan zap failed: {}", scan_error.message).into())
        }
    }
}

/// `scan rtis`（D7①）：poc get 元数据锚 + poc use 回填委派（零扫描执行面；
/// 真实库缺位 = rt.py 结构化失败透出——V-9 gitignore 运行时数据词形）。
fn run_scan_rtis(a: Args) -> Result<(), Box<dyn std::error::Error>> {
    let repo_root = scan_repo_root();
    let poc_id = a.poc_id.ok_or("scan rtis requires --poc-id N")?;
    let target = a.target.clone().ok_or("scan rtis requires --target <t>")?;
    let provider = verification_engine::provider::RtisPocProvider::from_repo_root(&repo_root);
    match wsw_runtime::scanner::scan_rtis(&provider, poc_id, &target) {
        Ok(frame) => {
            let normalized = wsw_runtime::scanner::project_rtis(&frame);
            let payload = serde_json::json!({
                "rtis": frame,
                "normalized": normalized,
            });
            println!("{payload}");
            Ok(())
        }
        Err(scan_error) => {
            emit_scan_error(&scan_error);
            Err(format!("scan rtis failed: {}", scan_error.message).into())
        }
    }
}

// ---------------------------------------------------------------------------
// WSW-8 browser 子命令（D8②：独立入口第十面——wsw_runtime::browser 统一入口
// 既有派发链消费：probe_chromium 探针链 + scope 消费 + submit_correlated +
// NativeRustAdapter spawn rp-wsw-browser 子进程 + evidence 'screenshot' 终局
// 写入；零第二派发实现，:761 禁第二套）
// ---------------------------------------------------------------------------

/// `browser capture`（D8②）：真实页面捕获派发（loopback fixture/授权目标面）。
fn run_browser_capture(a: Args) -> Result<(), Box<dyn std::error::Error>> {
    let repo_root = scan_repo_root();
    let db = a.db.clone().ok_or("browser capture requires --db <path>")?;
    let url = a.url.clone().ok_or("browser capture requires --url <u>")?;
    let mut input = serde_json::json!({
        "url": url,
        "mode": a.mode.clone().unwrap_or_else(|| "viewport".to_string()),
        "timeout_s": a.timeout_s.unwrap_or(60),
    });
    if let Some(sel) = &a.selector {
        input["selector"] = serde_json::json!(sel);
    }
    if let Some(clip_text) = &a.clip_json {
        let clip: serde_json::Value =
            serde_json::from_str(clip_text).map_err(|e| format!("--clip-json 解析失败: {e}"))?;
        input["clip"] = clip;
    }
    if let Some(settle) = a.settle_ms {
        input["settle_ms"] = serde_json::json!(settle);
    }
    if let Some(rules_text) = &a.scope_rules_json {
        let rules: serde_json::Value = serde_json::from_str(rules_text)
            .map_err(|e| format!("--scope-rules-json 解析失败: {e}"))?;
        input["scope_rules"] = rules;
    }
    let params = wsw_runtime::browser::BrowserCaptureParams::from_input(&input)
        .map_err(|e| format!("{}: {}", e.category, e.message))?;
    let engine = wsw_runtime::browser::open_browser_engine(&db, &repo_root)?;
    let adapter = tool_engine::adapter::native_rust::NativeRustAdapter::new(
        tool_engine::adapter::native_rust::NativeRustConfig::default(),
    );
    let manifest = wsw_runtime::browser::browser_manifest(&repo_root)?;
    let run_id = a.run_id.clone().unwrap_or_else(|| {
        format!(
            "rn-wsw8-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_micros()
        )
    });
    match wsw_runtime::browser::capture(
        &engine,
        &adapter,
        &manifest,
        &params,
        &run_id,
        a.task_id.as_deref(),
        a.trace_id.as_deref(),
    ) {
        Ok(outcome) => {
            let payload = serde_json::json!({
                "run_id": outcome.run_id,
                "status_word": outcome.status_word,
                "evidence_id": outcome.evidence_id,
                "artifacts": outcome.artifacts,
            });
            println!("{payload}");
            Ok(())
        }
        Err(e) => {
            let payload = serde_json::json!({
                "error": {"category": e.category, "message": e.message}
            });
            println!("{payload}");
            Err(format!("browser capture failed: {}", e.message).into())
        }
    }
}

/// `scan providers`（D7②）：三 adapter 契约面只读投影（manifest 直读 +
/// plugin catalog 只读计数——投影逻辑单源 wsw_runtime::scanner::
/// providers_projection；GET /api/wsw/scan/providers 子进程委派载体）。
fn run_scan_providers() -> Result<(), Box<dyn std::error::Error>> {
    let repo_root = scan_repo_root();
    match wsw_runtime::scanner::providers_projection(&repo_root) {
        Ok(projection) => {
            println!("{projection}");
            Ok(())
        }
        Err(message) => {
            let payload =
                serde_json::json!({"error": {"category": "tool_unavailable", "message": message}});
            println!("{payload}");
            Err(format!("scan providers failed: {message}").into())
        }
    }
}

#[tokio::main]
async fn main() {
    // 日志可见面：拒绝记录/五态/捕获计数落 stderr 单行（PLAN :528）。
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_max_level(tracing::Level::INFO)
        .init();
    let argv: Vec<String> = std::env::args().skip(1).collect();
    // P9-3 帮助面统一口径（rp-tool 先例）：显式 --help/-h = usage 到
    // stdout + exit 0（rp-wsw scan --help / scan nuclei --help 判据 9 面）。
    if argv.iter().any(|a| a == "--help" || a == "-h") {
        println!("{}", usage_text());
        std::process::exit(0);
    }
    let mut args = argv.into_iter();
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
        "ws" => match args.next().as_deref() {
            Some("connect") => run_ws_connect(parse_args(&mut args)).await,
            Some("replay") => run_ws_replay(parse_args(&mut args)).await,
            _ => usage(),
        },
        "decode" => run_decode(parse_args(&mut args)),
        "compare" => run_compare(parse_args(&mut args)),
        "scan" => match args.next().as_deref() {
            Some("nuclei") => run_scan_nuclei(parse_args(&mut args)),
            Some("zap") => run_scan_zap(parse_args(&mut args)),
            Some("rtis") => run_scan_rtis(parse_args(&mut args)),
            // D7②：provider 契约面只读投影（GET /api/wsw/scan/providers 的
            // Rust 载体——投影逻辑单源 scanner.rs providers_projection）。
            Some("providers") => run_scan_providers(),
            _ => usage(),
        },
        "browser" => match args.next().as_deref() {
            // WSW-8 D8②：独立入口第十面（browser.rs 统一入口分派——
            // Engine 装配 + submit_correlated + native_rust 派发链复用）。
            Some("capture") => run_browser_capture(parse_args(&mut args)),
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
        assert!(
            out.contains("\"redacted_header_names\""),
            "脱敏头名清单在场"
        );
        // 大小写不敏感命中（Authorization/authorization 各记一条，保序去重）。
        assert!(out.contains("authorization"), "命中头名清单含小写形式");
    }
}
