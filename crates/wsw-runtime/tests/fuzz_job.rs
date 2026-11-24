//! JobRunner 全链集成测试（WSW-4 S7；判据 2/5/6/7/8/9 面）：
//!
//! - 真实靶场 = 进程内 `TcpListener` 127.0.0.1:0 随机端口（WSW-3
//!   sender_send.rs spawn_target 同型；禁外联——红线 7；`--test-threads=1`）；
//! - marker echo 端点 payload 变体真实命中（判据 11 禁模拟冒充——发送走
//!   `sender::send_request` 单点真实 h1 字节往返）；
//! - budget 截断 BUDGET_EXHAUSTED + 速率窗口 SKIPPED_BUDGET +
//!   min_request_interval 相邻间隔实测；
//! - scope 全拒零发送（判据 7 靶机请求计数 = 0）+ 取消/暂停/恢复 +
//!   重启防重派（case 行 status 权威）；
//! - 状态词表/非法迁移拒（028 CHECK + Rust 迁移表双面）；
//! - 429 限流分类 + baseline 相似度/去重派生列（判据 9）。

use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use wsw_message::serialize::{OutboundFraming, OutboundRequest};
use wsw_message::types::{HeaderRecord, HttpVersion};
use wsw_runtime::persist::{FuzzCaseRow, PersistStore};
use wsw_runtime::scope::ScopeRules;
use wsw_runtime::sender::SendPolicy;
use wsw_runtime::fuzz::{run_job, FuzzJobOptions};

/// 靶场行为。
#[derive(Clone)]
enum Behavior {
    /// 200 + 体 = echo（q 参数值原样回显；无 q = "noq"）。
    Echo,
    /// 200 + 固定体（baseline 去重面）。
    Fixed(&'static str),
    /// 恒 429（限流分类面）。
    Status429,
    /// 不响应（超时面）。
    Silent,
}

/// 收到的请求记录（请求行 + 全字节）。
type Received = Arc<Mutex<Vec<(String, Vec<u8>)>>>;

/// 测试自管 tempdir（std-only；CAS 工件根——不依赖会话环境变量）。
fn temp_root(tag: &str) -> std::path::PathBuf {
    static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "{tag}-{}-{}-{n:x}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).expect("create temp root");
    dir
}

async fn spawn_target(behavior: Behavior) -> (u16, Received, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("target bind");
    let port = listener.local_addr().unwrap().port();
    let received: Received = Arc::new(Mutex::new(Vec::new()));
    let rx = Arc::clone(&received);
    let handle = tokio::spawn(async move {
        loop {
            let Ok((mut sock, _)) = listener.accept().await else {
                break;
            };
            let rx = Arc::clone(&rx);
            let behavior = behavior.clone();
            tokio::spawn(async move {
                let mut buf = Vec::new();
                let mut chunk = [0u8; 4096];
                loop {
                    let n = sock.read(&mut chunk).await.unwrap_or(0);
                    if n == 0 {
                        break;
                    }
                    buf.extend_from_slice(&chunk[..n]);
                    if buf.windows(4).any(|w| w == b"\r\n\r\n") {
                        break;
                    }
                }
                let head = String::from_utf8_lossy(&buf).to_string();
                // q 参数值提取（请求行 `GET /fuzz?q=VAL HTTP/1.1`）。
                let qval = head
                    .lines()
                    .next()
                    .and_then(|line| line.split(' ').nth(1))
                    .and_then(|t| t.split('?').nth(1))
                    .and_then(|q| {
                        q.split('&').find_map(|p| p.strip_prefix("q="))
                    })
                    .unwrap_or("noq")
                    .to_string();
                rx.lock().expect("received mutex").push((qval, buf.clone()));
                let response = match behavior {
                    Behavior::Echo => {
                        let qv = rx.lock().expect("received mutex").last().unwrap().0.clone();
                        format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {}\r\n\r\n{}",
                            qv.len(),
                            qv
                        )
                    }
                    Behavior::Fixed(body) => format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {}\r\n\r\n{}",
                        body.len(),
                        body
                    ),
                    Behavior::Status429 => {
                        "HTTP/1.1 429 Too Many Requests\r\nContent-Length: 0\r\n\r\n".to_string()
                    }
                    Behavior::Silent => {
                        // 不响应（读超时面）；保持连接静默 5s（对端 read
                        // 超时 400ms 先到——防 EOF 被判 Read 错）。
                        tokio::time::sleep(Duration::from_secs(5)).await;
                        let _ = sock.flush().await;
                        return;
                    }
                };
                let _ = sock.write_all(response.as_bytes()).await;
                let _ = sock.flush().await;
            });
        }
    });
    (port, received, handle)
}

fn rules_for(port: u16) -> ScopeRules {
    ScopeRules {
        include: vec!["127.0.0.1".to_string()],
        exclude: vec![],
        allowed_ports: vec![port],
        allowed_path_prefixes: vec![],
        allow_private_targets: true,
        max_redirects: 5,
    }
}

fn mem_store() -> PersistStore {
    let s = PersistStore::open_memory().expect("mem store");
    s.run_migrations(std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../migrations")
        .as_path())
    .expect("migrations");
    s.assert_watermark().expect("watermark 31");
    s
}

fn template(target: &str, headers: Vec<HeaderRecord>, body: &[u8]) -> OutboundRequest {
    OutboundRequest {
        method: "GET".into(),
        target: target.into(),
        version: HttpVersion::Http11,
        headers,
        body: body.to_vec(),
        trailers: vec![],
        framing: OutboundFraming::ContentLength,
    }
}

/// echo spec 组装（marker 在 target query；payload 列表 + budget 覆写）。
fn echo_spec_json(
    port: u16,
    payload_values: &[&str],
    mode: &str,
    budget: serde_json::Value,
    extra: serde_json::Value,
) -> String {
    let req = template(
        "/fuzz?q=§P§",
        vec![
            HeaderRecord::new("Host", format!("127.0.0.1:{port}")),
            HeaderRecord::new("Content-Type", "text/plain"),
        ],
        b"",
    );
    let mut obj = serde_json::Map::new();
    obj.insert("request".to_string(), serde_json::to_value(&req).unwrap());
    let mut extra = extra;
    let ex = extra.as_object_mut().expect("extra object");
    ex.insert("mode".to_string(), serde_json::json!(mode));
    ex.insert("budget".to_string(), budget);
    ex.insert(
        "payloads".to_string(),
        serde_json::json!([{"type":"list","values": payload_values}]),
    );
    for (k, v) in ex {
        obj.insert(k.clone(), v.clone());
    }
    serde_json::to_string(&serde_json::Value::Object(obj)).expect("spec")
}

fn budget_json(max_requests: u64, concurrency: u64) -> serde_json::Value {
    serde_json::json!({
        "max_requests": max_requests,
        "max_concurrency": concurrency,
        "rate_window_ms": 0,
        "rate_max_requests": 0,
        "min_request_interval_ms": 0,
        "max_duration_ms": 0
    })
}

/// job 行登记（QUEUED 入口；spec 预检走 prepare_spec 取 estimate）。
fn create_job(store: &PersistStore, session_id: &str, spec_json: &str) -> String {
    let prepared =
        wsw_runtime::fuzz::spec::prepare_spec(spec_json).expect("spec preflight");
    let job_id = wsw_message::ids::FuzzJobId::generate().into_inner();
    let row = wsw_runtime::fuzz::spec::job_row_from_prepared(
        &prepared, &job_id, session_id, spec_json, wsw_runtime::persist::now_iso(),
    );
    store.insert_fuzz_job(&row).expect("insert job");
    store
        .advance_fuzz_job(&job_id, "CREATED", &wsw_runtime::persist::FuzzJobPatch {
            state: "QUEUED",
            ..Default::default()
        })
        .expect("CREATED->QUEUED");
    job_id
}

fn job_opts(_job_id: &str, session_id: &str, rules: ScopeRules, port: u16) -> FuzzJobOptions {
    let (_, cancel_rx) = tokio::sync::watch::channel(false);
    let (_, pause_rx) = tokio::sync::watch::channel(false);
    let _ = port;
    FuzzJobOptions {
        session_id: session_id.to_string(),
        rules,
        policy: SendPolicy::default(),
        artifacts_root: temp_root("wsw4-job"),
        extra_ca: vec![],
        cancel: cancel_rx,
        pause: pause_rx,
    }
}

// ---------------------------------------------------------------------------
// 正例：marker echo 全链（判据 2/11）
// ---------------------------------------------------------------------------

#[tokio::test]
async fn full_run_echo_marker_hit_and_exchange_rows() {
    let (port, received, _t) = spawn_target(Behavior::Echo).await;
    let store = mem_store();
    let payloads = ["alpha", "bravo", "charlie"];
    let spec = echo_spec_json(port, &payloads, "cartesian", budget_json(10, 2), serde_json::json!({}));
    let job_id = create_job(&store, "wsw-ses-fz4-test", &spec);
    let summary = run_job(
        &store,
        &job_id,
        job_opts(&job_id, "wsw-ses-fz4-test", rules_for(port), port),
    )
    .await
    .expect("run ok");
    assert_eq!(summary.state, "COMPLETED");
    // 判据 2：估算 = 实际派发（cartesian 1 位置 3 payload）。
    assert_eq!(summary.estimated_requests, 3);
    assert_eq!(summary.dispatched, 3);
    assert_eq!(summary.responded, 3);
    // 真实命中：靶机收到 3 个 payload（marker echo）。
    let got: Vec<String> = received.lock().unwrap().iter().map(|(q, _)| q.clone()).collect();
    assert_eq!(got.len(), 3, "靶机请求数");
    for p in payloads {
        assert!(got.contains(&p.to_string()), "payload {p} 真实命中靶机：{got:?}");
    }
    // case 行：全 RESPONDED + payload/响应列。
    let cases = store.list_cases_by_job(&job_id).expect("cases");
    assert_eq!(cases.len(), 3);
    for c in &cases {
        assert_eq!(c.state, "RESPONDED");
        assert_eq!(c.response_status, Some(200));
        assert!(c.response_sha256.is_some());
        assert!(c.exchange_id.is_some(), "exchange 引用在位");
    }
    // exchange 行：source='fuzzer' + History 可查面（insert_exchanges 闭环）。
    let n: i64 = store
        .with_conn(|c| {
            c.query_row(
                "SELECT COUNT(*) FROM wsw_exchange WHERE source='fuzzer' AND listener_id='wsw-lis-fuzzer'",
                [],
                |r| r.get(0),
            )
        })
        .unwrap();
    assert_eq!(n, 3, "exchange(source=fuzzer) 行数");
    // baseline 派生列：首个 baseline，其余 unique（echo 体各异）。
    let baseline_cases = cases
        .iter()
        .filter(|c| c.diff_class.as_deref() == Some("baseline"))
        .count();
    assert_eq!(baseline_cases, 1, "恰一 baseline");
    for c in &cases {
        if c.diff_class.as_deref() != Some("baseline") {
            assert_eq!(c.diff_class.as_deref(), Some("unique"));
            let sim = c.similarity.expect("similarity");
            assert!(sim < 1.0, "不同 payload → 相似度 <1（{sim}）");
        }
    }
}

// ---------------------------------------------------------------------------
// 预算四闸（判据 5）
// ---------------------------------------------------------------------------

#[tokio::test]
async fn budget_truncation_budget_exhausted() {
    let (port, received, _t) = spawn_target(Behavior::Echo).await;
    let store = mem_store();
    let spec = echo_spec_json(
        port,
        &["p1", "p2", "p3", "p4", "p5", "p6"],
        "cartesian",
        serde_json::json!({
            "max_requests": 2, "max_concurrency": 1,
            "rate_window_ms": 0, "rate_max_requests": 0,
            "min_request_interval_ms": 0, "max_duration_ms": 0
        }),
        serde_json::json!({}),
    );
    let job_id = create_job(&store, "wsw-ses-fz4-test", &spec);
    let summary = run_job(&store, &job_id, job_opts(&job_id, "wsw-ses-fz4-test", rules_for(port), port))
        .await
        .expect("run ok");
    assert_eq!(summary.state, "BUDGET_EXHAUSTED", "max_requests 截断");
    assert_eq!(summary.estimated_requests, 6);
    assert_eq!(summary.dispatched, 2, "只派发 2 个");
    assert_eq!(summary.responded, 2, "已发送 case 终态齐全（:269）");
    let got = received.lock().unwrap().len();
    assert_eq!(got, 2, "靶机实际收到 = 派发数");
    // 终态行对账。
    let st: String = store.get_fuzz_job(&job_id).unwrap().unwrap().state;
    assert_eq!(st, "BUDGET_EXHAUSTED");
}

#[tokio::test]
async fn rate_window_skip_budget() {
    let (port, received, _t) = spawn_target(Behavior::Echo).await;
    let store = mem_store();
    let spec = echo_spec_json(
        port,
        &["r1", "r2", "r3", "r4"],
        "cartesian",
        serde_json::json!({
            "max_requests": 10, "max_concurrency": 1,
            "rate_window_ms": 600, "rate_max_requests": 1,
            "min_request_interval_ms": 0, "max_duration_ms": 0
        }),
        serde_json::json!({}),
    );
    let job_id = create_job(&store, "wsw-ses-fz4-test", &spec);
    let summary = run_job(&store, &job_id, job_opts(&job_id, "wsw-ses-fz4-test", rules_for(port), port))
        .await
        .expect("run ok");
    // 600ms 窗口内 4 case 快速派发：1 发送 + 3 窗口超帽 SKIPPED_BUDGET。
    assert_eq!(summary.state, "COMPLETED");
    assert_eq!(summary.responded, 1, "窗口帽 1");
    assert!(summary.skipped_budget >= 1, "超额 case SKIPPED_BUDGET 计数");
    assert_eq!(received.lock().unwrap().len() as u64, summary.responded);
}

#[tokio::test]
async fn min_request_interval_enforced() {
    let (port, _received, _t) = spawn_target(Behavior::Echo).await;
    let store = mem_store();
    let spec = echo_spec_json(
        port,
        &["i1", "i2", "i3"],
        "cartesian",
        serde_json::json!({
            "max_requests": 10, "max_concurrency": 1,
            "rate_window_ms": 0, "rate_max_requests": 0,
            "min_request_interval_ms": 90, "max_duration_ms": 0
        }),
        serde_json::json!({}),
    );
    let job_id = create_job(&store, "wsw-ses-fz4-test", &spec);
    let summary = run_job(&store, &job_id, job_opts(&job_id, "wsw-ses-fz4-test", rules_for(port), port))
        .await
        .expect("run ok");
    assert_eq!(summary.state, "COMPLETED");
    // 相邻 case started_at 间隔实测 ≥ ~85ms（ISO ms 宽度；容差 5ms 调度）。
    let cases = store.list_cases_by_job(&job_id).unwrap();
    assert_eq!(cases.len(), 3);
    let parse_ms = |iso: &str| -> i64 {
        // YYYY-MM-DDTHH:MM:SS.mmmZ → epoch ms（纯整数换算，容差断言用）。
        let parts: Vec<i64> = iso
            .split(&['-', 'T', ':', '.', 'Z'][..])
            .filter_map(|x| x.parse().ok())
            .collect();
        let (y, mo, d, h, mi, s, ms) =
            (parts[0], parts[1], parts[2], parts[3], parts[4], parts[5], parts[6]);
        // days-from-civil。
        let y2 = if mo <= 2 { y - 1 } else { y };
        let era = if y2 >= 0 { y2 } else { y2 - 399 } / 400;
        let yoe = y2 - era * 400;
        let mp = (mo + 9) % 12;
        let doy = (153 * mp + 2) / 5 + d - 1;
        let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
        (era * 146_097 + doe - 719_468) * 86_400_000 + h * 3_600_000 + mi * 60_000 + s * 1_000 + ms
    };
    for w in cases.windows(2) {
        let a = parse_ms(w[0].started_at.as_deref().expect("started_at"));
        let b = parse_ms(w[1].started_at.as_deref().expect("started_at"));
        let gap = b - a;
        assert!(gap >= 80, "相邻 case 间隔实测 {gap}ms < 80ms（interval 90ms）");
    }
}

// ---------------------------------------------------------------------------
// Scope 后端强制（判据 7）
// ---------------------------------------------------------------------------

#[tokio::test]
async fn scope_default_deny_zero_send() {
    let (port, received, _t) = spawn_target(Behavior::Echo).await;
    let store = mem_store();
    let spec = echo_spec_json(port, &["s1", "s2"], "cartesian", budget_json(10, 2), serde_json::json!({}));
    let job_id = create_job(&store, "wsw-ses-fz4-test", &spec);
    // default deny（include 空）。
    let mut opts = job_opts(&job_id, "wsw-ses-fz4-test", ScopeRules::default(), port);
    let (_, pause_rx) = tokio::sync::watch::channel(false);
    opts.pause = pause_rx;
    let summary = run_job(&store, &job_id, opts).await.expect("run ok");
    assert_eq!(summary.state, "COMPLETED");
    assert_eq!(summary.skipped_scope, 2, "全 case SKIPPED_SCOPE");
    assert_eq!(summary.dispatched, 0, "零发送");
    assert_eq!(received.lock().unwrap().len(), 0, "靶机请求计数 = 0（负例实证）");
    // error_code = scope.rs:234 词形（default deny）。
    let cases = store.list_cases_by_job(&job_id).unwrap();
    for c in &cases {
        assert_eq!(c.state, "SKIPPED_SCOPE");
        assert_eq!(c.error_code.as_deref(), Some("no_rules_default_deny"));
    }
}

#[tokio::test]
async fn scope_redirect_out_rejected_with_scope_word() {
    // include 命中 + 越界重定向 → 跳数 1 拒（scope 词形 error_code 可见）。
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let port = listener.local_addr().unwrap().port();
    let redirect_target = "http://169.254.169.254/latest".to_string();
    tokio::spawn(async move {
        loop {
            let Ok((mut sock, _)) = listener.accept().await else { break };
            let loc = redirect_target.clone();
            tokio::spawn(async move {
                let mut buf = Vec::new();
                let mut chunk = [0u8; 4096];
                loop {
                    let n = sock.read(&mut chunk).await.unwrap_or(0);
                    if n == 0 { break; }
                    buf.extend_from_slice(&chunk[..n]);
                    if buf.windows(4).any(|w| w == b"\r\n\r\n") { break; }
                }
                let resp = format!(
                    "HTTP/1.1 302 Found\r\nLocation: {loc}\r\nContent-Length: 0\r\n\r\n"
                );
                let _ = sock.write_all(resp.as_bytes()).await;
            });
        }
    });
    let store = mem_store();
    // spec 用 marker 注入 path（目标为本靶 /hop）。
    let req = template(
        "/hop?q=§P§",
        vec![HeaderRecord::new("Host", format!("127.0.0.1:{port}"))],
        b"",
    );
    let mut obj = serde_json::Map::new();
    obj.insert("request".to_string(), serde_json::to_value(&req).unwrap());
    obj.insert("mode".to_string(), serde_json::json!("cartesian"));
    obj.insert("budget".to_string(), budget_json(10, 1));
    obj.insert(
        "payloads".to_string(),
        serde_json::json!([{"type":"list","values":["x"]}]),
    );
    let spec = serde_json::to_string(&serde_json::Value::Object(obj)).unwrap();
    let job_id = create_job(&store, "wsw-ses-fz4-test", &spec);
    let mut opts = job_opts(&job_id, "wsw-ses-fz4-test", rules_for(port), port);
    let (_, pause_rx) = tokio::sync::watch::channel(false);
    opts.pause = pause_rx;
    // 重定向跟随策略（逐跳 scope 闸在 send_request 内——D1 权威面）。
    opts.policy = SendPolicy {
        redirect: wsw_runtime::sender::RedirectPolicy::Follow,
        ..Default::default()
    };
    let summary = run_job(&store, &job_id, opts).await.expect("run ok");
    assert_eq!(summary.state, "COMPLETED");
    let cases = store.list_cases_by_job(&job_id).unwrap();
    assert_eq!(cases.len(), 1);
    // 越界重定向 = scope 拒（send_request hop≥1 → SendError::Scope）。
    assert_eq!(cases[0].state, "NETWORK_ERROR");
    assert_eq!(cases[0].error_code.as_deref(), Some("scope_blocked"), "scope 词形可见（:528）");
}

// ---------------------------------------------------------------------------
// 取消 / 暂停恢复 / 重启防重派（判据 8）
// ---------------------------------------------------------------------------

#[tokio::test]
async fn cancel_mid_run_graceful() {
    let (port, received, _t) = spawn_target(Behavior::Echo).await;
    let store = mem_store();
    let spec = echo_spec_json(
        port,
        &["c1", "c2", "c3", "c4", "c5", "c6"],
        "cartesian",
        serde_json::json!({
            "max_requests": 10, "max_concurrency": 1,
            "rate_window_ms": 0, "rate_max_requests": 0,
            "min_request_interval_ms": 120, "max_duration_ms": 0
        }),
        serde_json::json!({}),
    );
    let job_id = create_job(&store, "wsw-ses-fz4-test", &spec);
    // 间隔闸 120ms 拉长运行 → 取消窗口可靠。
    let (cancel_tx, cancel_rx) = tokio::sync::watch::channel(false);
    let (_, pause_rx) = tokio::sync::watch::channel(false);
    let opts = FuzzJobOptions {
        session_id: "wsw-ses-fz4-test".to_string(),
        rules: rules_for(port),
        policy: SendPolicy::default(),
        artifacts_root: temp_root("wsw4-cancel"),
        extra_ca: vec![],
        cancel: cancel_rx,
        pause: pause_rx,
    };
    let store2 = store.clone();
    let jid = job_id.clone();
    let runner = tokio::spawn(async move {
        run_job(&store2, &jid, opts).await.expect("run ok")
    });
    // 等 2 case 发出后取消。
    let deadline = tokio::time::Instant::now() + Duration::from_millis(5000);
    while received.lock().unwrap().len() < 2 {
        assert!(tokio::time::Instant::now() < deadline, "等待派发超时");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    cancel_tx.send(true).expect("cancel send");
    let summary = runner.await.expect("join");
    assert_eq!(summary.state, "CANCELLED", "SIGTERM → CANCELLING → CANCELLED");
    // 全 case 行终态（在途 drain；未派 ordinal 无行）。
    let cases = store.list_cases_by_job(&job_id).unwrap();
    assert!(!cases.is_empty());
    for c in &cases {
        assert!(c.state != "PENDING" && c.state != "SENT", "取消后无悬挂行：{}", c.state);
    }
}

#[tokio::test]
async fn pause_resume_continues_pending() {
    let (port, _received, _t) = spawn_target(Behavior::Echo).await;
    let store = mem_store();
    let spec = echo_spec_json(
        port,
        &["pa", "pb", "pc"],
        "cartesian",
        budget_json(10, 1),
        serde_json::json!({}),
    );
    let job_id = create_job(&store, "wsw-ses-fz4-test", &spec);
    let (_, cancel_rx) = tokio::sync::watch::channel(false);
    let (pause_tx, pause_rx) = tokio::sync::watch::channel(true); // 起跑即暂停
    let opts = FuzzJobOptions {
        session_id: "wsw-ses-fz4-test".to_string(),
        rules: rules_for(port),
        policy: SendPolicy::default(),
        artifacts_root: temp_root("wsw4-pause"),
        extra_ca: vec![],
        cancel: cancel_rx,
        pause: pause_rx,
    };
    let store2 = store.clone();
    let jid = job_id.clone();
    let runner = tokio::spawn(async move {
        run_job(&store2, &jid, opts).await.expect("run ok")
    });
    // 等 PAUSED 落行。
    let deadline = tokio::time::Instant::now() + Duration::from_millis(5000);
    loop {
        let st = store.get_fuzz_job(&job_id).unwrap().unwrap().state;
        if st == "PAUSED" {
            break;
        }
        assert!(tokio::time::Instant::now() < deadline, "等待 PAUSED 超时");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    // 恢复 → 续派 PENDING → COMPLETED。
    pause_tx.send(false).expect("resume");
    let summary = runner.await.expect("join");
    assert_eq!(summary.state, "COMPLETED");
    assert_eq!(summary.dispatched, 3, "恢复后续派全部 PENDING");
}

#[tokio::test]
async fn resume_does_not_redispatch_terminal_cases() {
    // kill/重启模拟：job 行 RUNNING + ordinal 0/1 已 RESPONDED（人工落行），
    // 续跑只派 PENDING 段（case 行 status 权威——判据 8 派发集合无重）。
    let (port, received, _t) = spawn_target(Behavior::Echo).await;
    let store = mem_store();
    let payloads = ["v0", "v1", "v2", "v3"];
    let spec = echo_spec_json(port, &payloads, "cartesian", budget_json(10, 1), serde_json::json!({}));
    let prepared = wsw_runtime::fuzz::spec::prepare_spec(&spec).expect("spec");
    let job_id = wsw_message::ids::FuzzJobId::generate().into_inner();
    let row = wsw_runtime::fuzz::spec::job_row_from_prepared(
        &prepared, &job_id, "wsw-ses-fz4-test", &spec, wsw_runtime::persist::now_iso(),
    );
    store.insert_fuzz_job(&row).expect("insert job");
    // kill 残留态推进：CREATED→QUEUED→RUNNING（合法迁移链）。
    store
        .advance_fuzz_job(&job_id, "CREATED", &wsw_runtime::persist::FuzzJobPatch {
            state: "QUEUED",
            ..Default::default()
        })
        .expect("to QUEUED");
    store
        .advance_fuzz_job(&job_id, "QUEUED", &wsw_runtime::persist::FuzzJobPatch {
            state: "RUNNING",
            started_at: Some(wsw_runtime::persist::now_iso()),
            ..Default::default()
        })
        .expect("to RUNNING");
    // ordinal 0/1 人工 RESPONDED（kill 前已终态）。
    for (ord, val) in [(0i64, "v0"), (1, "v1")] {
        let assignment: Vec<Option<String>> = vec![Some(val.to_string())];
        let pj = wsw_runtime::fuzz::spec::payload_vector_json(&assignment);
        store
            .insert_fuzz_case(&FuzzCaseRow {
                case_id: wsw_message::ids::FuzzCaseId::generate().into_inner(),
                job_id: job_id.clone(),
                ordinal: ord,
                state: "RESPONDED".to_string(),
                payload_json: pj.clone(),
                payload_sha256: wsw_runtime::fuzz::analyzer::payload_sha256(&pj),
                response_status: Some(200),
                started_at: Some(wsw_runtime::persist::now_iso()),
                completed_at: Some(wsw_runtime::persist::now_iso()),
                created_at: wsw_runtime::persist::now_iso(),
                ..Default::default()
            })
            .expect("insert case");
    }
    // 续跑（入口 RUNNING = kill 残留态）。
    let mut opts = job_opts(&job_id, "wsw-ses-fz4-test", rules_for(port), port);
    let (_, pause_rx) = tokio::sync::watch::channel(false);
    opts.pause = pause_rx;
    let summary = run_job(&store, &job_id, opts).await.expect("run ok");
    assert_eq!(summary.state, "COMPLETED");
    assert_eq!(summary.dispatched, 4, "总派发口径 = 既有 2 + 本次 2");
    // 靶机只收到 v2/v3（v0/v1 不重派）。
    let got: Vec<String> = received.lock().unwrap().iter().map(|(q, _)| q.clone()).collect();
    assert_eq!(got, vec!["v2".to_string(), "v3".to_string()], "已终态 case 不重派：{got:?}");
    // case 行 4 行全 RESPONDED（2 人工 + 2 本次）。
    let cases = store.list_cases_by_job(&job_id).unwrap();
    assert_eq!(cases.len(), 4);
    assert!(cases.iter().all(|c| c.state == "RESPONDED"));
}

// ---------------------------------------------------------------------------
// 状态机词表与非法迁移（判据 6）
// ---------------------------------------------------------------------------

#[test]
fn job_state_transition_table_rejects_illegal() {
    use wsw_runtime::persist::fuzz_job_transition_allowed;
    // 合法面（九态主链）。
    assert!(fuzz_job_transition_allowed("CREATED", "QUEUED"));
    assert!(fuzz_job_transition_allowed("QUEUED", "RUNNING"));
    assert!(fuzz_job_transition_allowed("RUNNING", "PAUSED"));
    assert!(fuzz_job_transition_allowed("PAUSED", "RUNNING"));
    assert!(fuzz_job_transition_allowed("RUNNING", "CANCELLING"));
    assert!(fuzz_job_transition_allowed("CANCELLING", "CANCELLED"));
    assert!(fuzz_job_transition_allowed("RUNNING", "COMPLETED"));
    assert!(fuzz_job_transition_allowed("RUNNING", "BUDGET_EXHAUSTED"));
    // 非法直跳（QUEUED→COMPLETED 等终态直跳拒）。
    assert!(!fuzz_job_transition_allowed("QUEUED", "COMPLETED"));
    assert!(!fuzz_job_transition_allowed("CREATED", "RUNNING"));
    assert!(!fuzz_job_transition_allowed("CANCELLED", "RUNNING"), "终态无出边");
    assert!(!fuzz_job_transition_allowed("COMPLETED", "QUEUED"));
    assert!(!fuzz_job_transition_allowed("BUDGET_EXHAUSTED", "RUNNING"));
    // 词表外词值拒。
    assert!(!fuzz_job_transition_allowed("RUNNING", "FAILED_X"));
    // case 七态迁移表。
    use wsw_runtime::persist::fuzz_case_transition_allowed;
    assert!(fuzz_case_transition_allowed("PENDING", "SENT"));
    assert!(fuzz_case_transition_allowed("PENDING", "SKIPPED_SCOPE"));
    assert!(fuzz_case_transition_allowed("PENDING", "SKIPPED_BUDGET"));
    assert!(fuzz_case_transition_allowed("SENT", "RESPONDED"));
    assert!(fuzz_case_transition_allowed("SENT", "TIMEOUT"));
    assert!(fuzz_case_transition_allowed("SENT", "NETWORK_ERROR"));
    assert!(!fuzz_case_transition_allowed("PENDING", "RESPONDED"), "PENDING 直跳 RESPONDED 拒");
    assert!(!fuzz_case_transition_allowed("RESPONDED", "SENT"), "终态无出边");
    assert!(!fuzz_case_transition_allowed("SENT", "PENDING"));
}

#[tokio::test]
async fn illegal_transition_and_state_words_rejected() {
    let store = mem_store();
    // Rust 侧拒：QUEUED→COMPLETED。
    let spec = echo_spec_json(1, &["x"], "cartesian", budget_json(1, 1), serde_json::json!({}));
    let prepared = wsw_runtime::fuzz::spec::prepare_spec(&spec).expect("spec");
    let job_id = wsw_message::ids::FuzzJobId::generate().into_inner();
    let row = wsw_runtime::fuzz::spec::job_row_from_prepared(
        &prepared, &job_id, "wsw-ses-fz4-test", &spec, wsw_runtime::persist::now_iso(),
    );
    store.insert_fuzz_job(&row).expect("insert");
    store
        .advance_fuzz_job(&job_id, "CREATED", &wsw_runtime::persist::FuzzJobPatch {
            state: "QUEUED",
            ..Default::default()
        })
        .expect("to QUEUED");
    let err = store.advance_fuzz_job(&job_id, "QUEUED", &wsw_runtime::persist::FuzzJobPatch {
        state: "COMPLETED",
        ..Default::default()
    });
    assert!(err.is_err(), "QUEUED→COMPLETED 直跳 Rust 侧拒");
    // SQL CHECK 拒：非法九态词值 INSERT。
    let mut bad = row.clone();
    bad.job_id = wsw_message::ids::FuzzJobId::generate().into_inner();
    bad.state = "RUNNIN".to_string();
    assert!(store.insert_fuzz_job(&bad).is_err(), "非法 job 状态词 INSERT 拒");
    // SQL CHECK 拒：非法七态词值 INSERT。
    let case_row = FuzzCaseRow {
        case_id: wsw_message::ids::FuzzCaseId::generate().into_inner(),
        job_id: job_id.clone(),
        ordinal: 0,
        state: "DONE".to_string(),
        payload_json: "[\"x\"]".to_string(),
        payload_sha256: wsw_runtime::fuzz::analyzer::payload_sha256("[\"x\"]"),
        created_at: wsw_runtime::persist::now_iso(),
        ..Default::default()
    };
    assert!(store.insert_fuzz_case(&case_row).is_err(), "非法 case 状态词 INSERT 拒（028 CHECK）");
    // case PENDING→RESPONDED 直跳拒（Rust 迁移表）。
    let ok_case = FuzzCaseRow {
        case_id: wsw_message::ids::FuzzCaseId::generate().into_inner(),
        job_id: job_id.clone(),
        ordinal: 1,
        state: "PENDING".to_string(),
        payload_json: "[\"y\"]".to_string(),
        payload_sha256: wsw_runtime::fuzz::analyzer::payload_sha256("[\"y\"]"),
        created_at: wsw_runtime::persist::now_iso(),
        ..Default::default()
    };
    store.insert_fuzz_case(&ok_case).expect("insert PENDING case");
    let err = store.advance_fuzz_case(&ok_case.case_id, "PENDING", &wsw_runtime::persist::FuzzCasePatch {
        state: "RESPONDED",
        ..Default::default()
    });
    assert!(err.is_err(), "PENDING→RESPONDED 直跳拒");
}

// ---------------------------------------------------------------------------
// 分类 / baseline（判据 9）
// ---------------------------------------------------------------------------

#[tokio::test]
async fn rate_limited_429_classified() {
    let (port, _received, _t) = spawn_target(Behavior::Status429).await;
    let store = mem_store();
    let spec = echo_spec_json(port, &["q1"], "cartesian", budget_json(5, 1), serde_json::json!({}));
    let job_id = create_job(&store, "wsw-ses-fz4-test", &spec);
    let summary = run_job(&store, &job_id, job_opts(&job_id, "wsw-ses-fz4-test", rules_for(port), port))
        .await
        .expect("run ok");
    assert_eq!(summary.state, "COMPLETED");
    let cases = store.list_cases_by_job(&job_id).unwrap();
    assert_eq!(cases[0].state, "RESPONDED", "429 = 已响应（有响应面）");
    assert_eq!(cases[0].response_status, Some(429));
    assert_eq!(cases[0].error_code.as_deref(), Some("fuzz_rate_limited"), "限流分类词形");
}

#[tokio::test]
async fn timeout_silent_target() {
    let (port, _received, _t) = spawn_target(Behavior::Silent).await;
    let store = mem_store();
    let spec = echo_spec_json(port, &["t1"], "cartesian", budget_json(5, 1), serde_json::json!({}));
    let job_id = create_job(&store, "wsw-ses-fz4-test", &spec);
    let mut opts = job_opts(&job_id, "wsw-ses-fz4-test", rules_for(port), port);
    opts.policy = SendPolicy {
        read_timeout_ms: 400,
        total_timeout_ms: 1500,
        ..Default::default()
    };
    let (_, pause_rx) = tokio::sync::watch::channel(false);
    opts.pause = pause_rx;
    let summary = run_job(&store, &job_id, opts).await.expect("run ok");
    assert_eq!(summary.state, "COMPLETED");
    let cases = store.list_cases_by_job(&job_id).unwrap();
    assert_eq!(cases[0].state, "TIMEOUT", "静默靶 → TIMEOUT 态");
    assert_eq!(cases[0].error_code.as_deref(), Some("timed_out"));
}

#[tokio::test]
async fn duplicate_bodies_grouped_by_sha() {
    // 同 payload 两次 → 同 body → sha 分组 duplicate（PLAN :261/:303）。
    let (port, _received, _t) = spawn_target(Behavior::Fixed("same-body")).await;
    let store = mem_store();
    let spec = echo_spec_json(
        port,
        &["dup", "dup"],
        "cartesian",
        budget_json(5, 1),
        serde_json::json!({}),
    );
    let job_id = create_job(&store, "wsw-ses-fz4-test", &spec);
    let summary = run_job(&store, &job_id, job_opts(&job_id, "wsw-ses-fz4-test", rules_for(port), port))
        .await
        .expect("run ok");
    assert_eq!(summary.state, "COMPLETED");
    let cases = store.list_cases_by_job(&job_id).unwrap();
    assert_eq!(cases.len(), 2);
    assert_eq!(cases[0].diff_class.as_deref(), Some("baseline"));
    assert_eq!(cases[1].diff_class.as_deref(), Some("duplicate"), "同 sha → duplicate");
    assert_eq!(cases[0].response_sha256, cases[1].response_sha256, "同 body → 同 sha");
    assert!((cases[1].similarity.unwrap() - 1.0).abs() < 1e-9);
}

#[tokio::test]
async fn header_marker_crlf_payload_serialize_guard() {
    // 头注入 payload（CRLF）→ serialize 守卫拒（NETWORK_ERROR +
    // serialize_failed——判据 4 负例；守卫词形 sender 既有面直通）。
    let (port, _received, _t) = spawn_target(Behavior::Echo).await;
    let store = mem_store();
    let req = template(
        "/fuzz",
        vec![
            HeaderRecord::new("Host", format!("127.0.0.1:{port}")),
            HeaderRecord::new("X-Fuzz", "§P§"),
        ],
        b"",
    );
    let mut obj = serde_json::Map::new();
    obj.insert("request".to_string(), serde_json::to_value(&req).unwrap());
    obj.insert("mode".to_string(), serde_json::json!("cartesian"));
    obj.insert("budget".to_string(), budget_json(5, 1));
    obj.insert(
        "payloads".to_string(),
        serde_json::json!([{"type":"list","values":["crlf\ru\r\nX-Inject: 1"]}]),
    );
    let spec = serde_json::to_string(&serde_json::Value::Object(obj)).unwrap();
    let job_id = create_job(&store, "wsw-ses-fz4-test", &spec);
    let summary = run_job(&store, &job_id, job_opts(&job_id, "wsw-ses-fz4-test", rules_for(port), port))
        .await
        .expect("run ok");
    assert_eq!(summary.state, "COMPLETED");
    let cases = store.list_cases_by_job(&job_id).unwrap();
    assert_eq!(cases[0].state, "NETWORK_ERROR");
    assert_eq!(cases[0].error_code.as_deref(), Some("serialize_failed"));
}

#[tokio::test]
async fn match_hits_keywords_recorded() {
    // 关键字命中投影（analyze.keywords → match_hits_json）。
    let (port, _received, _t) = spawn_target(Behavior::Fixed("TOKEN-FOUND here")).await;
    let store = mem_store();
    let spec = echo_spec_json(
        port,
        &["k1"],
        "cartesian",
        budget_json(5, 1),
        serde_json::json!({"analyze": {"keywords": ["TOKEN-FOUND", "absent"]}}),
    );
    let job_id = create_job(&store, "wsw-ses-fz4-test", &spec);
    let summary = run_job(&store, &job_id, job_opts(&job_id, "wsw-ses-fz4-test", rules_for(port), port))
        .await
        .expect("run ok");
    assert_eq!(summary.state, "COMPLETED");
    let cases = store.list_cases_by_job(&job_id).unwrap();
    let hits = cases[0].match_hits_json.as_deref().expect("match_hits_json");
    assert!(hits.contains("TOKEN-FOUND"), "命中关键字在场：{hits}");
    assert!(!hits.contains("absent"), "未命中不投影");
}
