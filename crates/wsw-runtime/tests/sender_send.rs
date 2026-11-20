//! sender 发送引擎集成测试（WSW-3-TASK §四.2 断言矩阵 S4 面）：
//!
//! - 发送往返：Outbound → serialize → h1 字节写 → 靶场响应；outbound CAS
//!   sha256 与实际写入字节一致（逐字节回读断言）；exchange(source=repeater)
//!   行 + send 行终态 succeeded；
//! - 重定向帽：follow 策略第 11 跳拒（default max_redirects=10）+
//!   redirect_chain 可见；越界 Location → failed + scope_blocked；
//!   none 策略不跟随（3xx 原样返回）；
//! - scope 拒发：default deny → state=failed + error_code=scope_blocked
//!   + 未建连（PLAN :528）；
//! - 超时：total 超时 → timed_out；SIGTERM 取消面（watch 置位）→ cancelled；
//! - 编辑头序保持：outbound bytes 逐字节 = serialize_request 产物（头序/
//!   重复头不被传输面重排——W3-D4.3 否决 hyper 写路径的正面证据）。
//!
//! 纪律：靶场 local_range loopback（127.0.0.1 随机端口）；--test-threads=1；
//! 禁外联。

use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use wsw_message::serialize::{OutboundFraming, OutboundRequest};
use wsw_message::types::{HeaderRecord, HttpVersion};
use wsw_runtime::persist::{PersistStore, SendPatch, SendRow};
use wsw_runtime::scope::ScopeRules;
use wsw_runtime::sender::{SendPolicy, SendError};

/// 靶场请求字节收集器（逐字节回读断言锚点）。
type ReceivedBytes = Arc<std::sync::Mutex<Vec<Vec<u8>>>>;

/// 本地 HTTP 靶场：GET → 200 marker；其余按 behavior。
#[derive(Clone, Copy)]
enum Behavior {
    /// 200 + marker 体。
    Marker,
    /// 302 + Location（自跳）；收到第 N 个请求后返回 200。
    RedirectLoop(usize),
    /// 302 → include 外域（越界 Location）。
    RedirectOut,
    /// 收到请求后不响应（超时面）。
    Silent,
}

async fn spawn_target(behavior: Behavior) -> (u16, ReceivedBytes, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("target bind");
    let port = listener.local_addr().unwrap().port();
    let received: ReceivedBytes = Arc::new(std::sync::Mutex::new(Vec::new()));
    let rx = Arc::clone(&received);
    let handle = tokio::spawn(async move {
        loop {
            let Ok((mut sock, _)) = listener.accept().await else {
                break;
            };
            let rx = Arc::clone(&rx);
            tokio::spawn(async move {
                // 读完整请求头 + CL 体（本地靶场请求必带 CL）。
                let mut buf = Vec::new();
                let mut chunk = [0u8; 4096];
                loop {
                    let n = sock.read(&mut chunk).await.unwrap_or(0);
                    if n == 0 {
                        break;
                    }
                    buf.extend_from_slice(&chunk[..n]);
                    if let Some(pos) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                        let head = String::from_utf8_lossy(&buf[..pos]).to_ascii_lowercase();
                        let cl: usize = head
                            .lines()
                            .find_map(|l| {
                                l.strip_prefix("content-length:")
                                    .and_then(|v| v.trim().parse().ok())
                            })
                            .unwrap_or(0);
                        if buf.len() >= pos + 4 + cl {
                            break;
                        }
                    }
                }
                rx.lock().expect("received mutex").push(buf.clone());
                let count = rx.lock().expect("received mutex").len();
                let response = match behavior {
                    Behavior::Marker => {
                        b"HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: 13\r\n\r\nwsw3-marker\r\n".to_vec()
                    }
                    Behavior::RedirectLoop(cap) => {
                        if count < cap {
                            format!(
                                "HTTP/1.1 302 Found\r\nLocation: http://127.0.0.1:{}/hop\r\nContent-Length: 0\r\n\r\n",
                                local_port_of(&buf)
                            )
                            .into_bytes()
                        } else {
                            b"HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: 13\r\n\r\nwsw3-marker\r\n".to_vec()
                        }
                    }
                    Behavior::RedirectOut => {
                        b"HTTP/1.1 302 Found\r\nLocation: http://169.254.169.254/latest/meta-data\r\nContent-Length: 0\r\n\r\n".to_vec()
                    }
                    Behavior::Silent => {
                        // 不响应：挂住直至对端超时。
                        tokio::time::sleep(std::time::Duration::from_secs(30)).await;
                        Vec::new()
                    }
                };
                let _ = sock.write_all(&response).await;
                let _ = sock.flush().await;
            });
        }
    });
    (port, received, handle)
}

/// 靶场从请求 Host 头还原自身端口（RedirectLoop 自跳 Location 需要）。
fn local_port_of(request: &[u8]) -> u16 {
    let head = String::from_utf8_lossy(request);
    head.lines()
        .find_map(|l| l.strip_prefix("Host: ").and_then(|v| v.rsplit_once(':')).and_then(|(_, p)| p.parse().ok()))
        .unwrap_or(0)
}

fn outbound(target_path: &str, host: &str) -> OutboundRequest {
    OutboundRequest {
        method: "GET".to_string(),
        target: target_path.to_string(),
        version: HttpVersion::Http11,
        headers: vec![
            HeaderRecord::new("Host", host.to_string()),
            HeaderRecord::new("X-Trace", "wsw3-sender-test".to_string()),
            HeaderRecord::new("Accept", "*/*".to_string()),
        ],
        body: Vec::new(),
        trailers: Vec::new(),
        framing: OutboundFraming::ContentLength,
    }
}

fn rules_for(port: u16) -> ScopeRules {
    ScopeRules {
        include: vec!["127.0.0.1".to_string()],
        allow_private_targets: true,
        allowed_ports: vec![port],
        ..Default::default()
    }
}

fn mem_store() -> PersistStore {
    let s = PersistStore::open_memory().expect("open memory");
    s.run_migrations(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations").as_path())
        .expect("migrate full chain");
    s
}

fn send_row(store: &PersistStore, send_id: &str, session_id: &str) {
    store
        .insert_send(&SendRow {
            send_id: send_id.to_string(),
            tab_id: "wsw-rpt-test-0001".to_string(),
            version_no: 1,
            session_id: session_id.to_string(),
            state: "pending".to_string(),
            created_at: wsw_runtime::persist::now_iso(),
            ..Default::default()
        })
        .unwrap();
    store
        .advance_send(send_id, &SendPatch { state: "sending", ..Default::default() })
        .unwrap();
}

#[tokio::test]
async fn send_roundtrip_and_outbound_byte_equality() {
    let (port, received, _server) = spawn_target(Behavior::Marker).await;
    let rules = rules_for(port);
    let req = outbound("/p59c-marker", &format!("127.0.0.1:{port}"));
    let policy = SendPolicy::default();
    let (_tx, rx) = tokio::sync::watch::channel(false);
    let outcome = wsw_runtime::sender::send_request(&rules, &req, &policy, &[], &rx)
        .await
        .expect("send roundtrip");
    assert_eq!(outcome.state, "succeeded");
    assert_eq!(outcome.response_status, Some(200));
    assert!(
        String::from_utf8_lossy(&outcome.response_bytes).contains("wsw3-marker"),
        "响应视图含 marker"
    );
    // 逐字节回读：靶场实际收到的字节 == outbound bytes == serialize 产物。
    let serialized = wsw_message::serialize::serialize_request(&req).unwrap();
    let sent_bytes = received.lock().unwrap()[0].clone();
    assert_eq!(
        sent_bytes, serialized.as_bytes().to_vec(),
        "靶场收到的字节 = serialize_request 产物（编辑头序保持，hyper 写路径否决的正面证据）"
    );
    assert_eq!(
        outcome.outbound_bytes, serialized.as_bytes().to_vec(),
        "引擎返回 outbound = serialize 产物"
    );
    // 落库：outbound CAS 逐字节回读 + exchange(source=repeater) 行 + send 行 succeeded。
    let store = mem_store();
    let session_id = "wsw-ses-test-0001";
    store
        .register_session(&wsw_runtime::persist::SessionRow {
            session_id: session_id.to_string(),
            title: "t".into(),
            scope_rules_json: serde_json::to_string(&rules).unwrap(),
            out_of_scope_policy: "blocked".into(),
            created_at: wsw_runtime::persist::now_iso(),
            updated_at: wsw_runtime::persist::now_iso(),
        })
        .unwrap();
    send_row(&store, "wsw-snd-rt-0001", session_id);
    let artifacts = std::env::temp_dir().join(format!(
        "wsw3-sender-cas-{}-{:?}",
        std::process::id(),
        std::time::SystemTime::now()
    ));
    std::fs::create_dir_all(&artifacts).unwrap();
    let exchange_id = wsw_runtime::sender::persist_send_result(
        &store, &artifacts, "wsw-snd-rt-0001", session_id, "wsw-lis-test", "wsw-rpt-test-0001", 1,
        &req, &outcome,
    )
    .unwrap();
    // send 行终态 succeeded + outbound sha 列。
    let send = store.get_send("wsw-snd-rt-0001").unwrap().unwrap();
    assert_eq!(send.state, "succeeded");
    assert_eq!(
        send.request_outbound_sha256.as_deref(),
        Some(wsw_message::cas::sha256_hex(&outcome.outbound_bytes).as_str()),
        "outbound CAS sha256 列 = 实际写入字节摘要"
    );
    // CAS 文件字节读回 = outbound bytes（逐字节回读断言）。
    let sha = wsw_message::cas::sha256_hex(&outcome.outbound_bytes);
    let uri = format!("wsw/{}/{}", &sha[..2], sha);
    let cas_bytes = wsw_message::cas::read_body(&artifacts, &uri).unwrap();
    assert_eq!(cas_bytes, outcome.outbound_bytes);
    // exchange 行 source=repeater。
    let exchange_id = exchange_id.unwrap();
    let (source, method, path): (String, String, String) = store
        .with_conn(|c| {
            c.query_row(
                "SELECT source, method, path FROM wsw_exchange WHERE exchange_id=?1",
                [&exchange_id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
        })
        .unwrap();
    assert_eq!(source, "repeater");
    assert_eq!(method, "GET");
    assert_eq!(path, "/p59c-marker");
    let _ = std::fs::remove_dir_all(&artifacts);
}

#[tokio::test]
async fn redirect_follow_cap_and_chain_visibility() {
    // 靶场对前 11 个请求一律 302（初始请求 + 重定向跳 1..10）→ 第 11 跳
    // （hop 11 > max_redirects 10）scope 闸在建连前拒；第 12 请求永不发生。
    let (port, received, _server) = spawn_target(Behavior::RedirectLoop(12)).await;
    let rules = rules_for(port);
    let req = outbound("/hop", &format!("127.0.0.1:{port}"));
    let policy = SendPolicy {
        redirect: wsw_runtime::sender::RedirectPolicy::Follow,
        ..Default::default()
    };
    let (_tx, rx) = tokio::sync::watch::channel(false);
    let err = wsw_runtime::sender::send_request(&rules, &req, &policy, &[], &rx)
        .await
        .unwrap_err();
    match &err {
        SendError::Scope(code) => assert_eq!(code, "redirect_limit", "第 11 跳 = 帽拒"),
        other => panic!("expected scope redirect_limit, got {other:?}"),
    }
    assert_eq!(
        received.lock().unwrap().len(),
        11,
        "首跳 + 10 次跟随；第 11 跳 scope 闸在建连前拒"
    );
    // redirect_chain 可见性：引擎错误面不带链（失败即断）——帽可见性由
    // rp-wsw 落库面断言（redirect_chain_json 列）；此处断言实际跳数。
}

#[tokio::test]
async fn redirect_follow_within_cap_reaches_marker() {
    // 自跳 3 次后 200：跟随成功面。
    let (port, _received, _server) = spawn_target(Behavior::RedirectLoop(4)).await;
    let rules = rules_for(port);
    let req = outbound("/hop", &format!("127.0.0.1:{port}"));
    let policy = SendPolicy {
        redirect: wsw_runtime::sender::RedirectPolicy::Follow,
        ..Default::default()
    };
    let (_tx, rx) = tokio::sync::watch::channel(false);
    let outcome = wsw_runtime::sender::send_request(&rules, &req, &policy, &[], &rx)
        .await
        .expect("follow within cap");
    assert_eq!(outcome.response_status, Some(200));
    assert_eq!(outcome.redirect_chain.len(), 3, "逐跳链记录可见");
    assert_eq!(outcome.redirect_chain[0].status, 302);
    assert!(outcome.redirect_chain[0].remote_ip.is_some());
}

#[tokio::test]
async fn redirect_none_policy_returns_3xx_asis() {
    let (port, received, _server) = spawn_target(Behavior::RedirectLoop(99)).await;
    let rules = rules_for(port);
    let req = outbound("/hop", &format!("127.0.0.1:{port}"));
    let policy = SendPolicy {
        redirect: wsw_runtime::sender::RedirectPolicy::None,
        ..Default::default()
    };
    let (_tx, rx) = tokio::sync::watch::channel(false);
    let outcome = wsw_runtime::sender::send_request(&rules, &req, &policy, &[], &rx)
        .await
        .expect("none policy");
    assert_eq!(outcome.response_status, Some(302), "none 策略不跟随");
    assert!(outcome.redirect_chain.is_empty());
    assert_eq!(received.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn redirect_out_of_scope_location_rejected() {
    // 302 → Location = 云元数据端点（deny-never-allow）。
    let (port, _received, _server) = spawn_target(Behavior::RedirectOut).await;
    let rules = rules_for(port);
    let req = outbound("/hop", &format!("127.0.0.1:{port}"));
    let policy = SendPolicy {
        redirect: wsw_runtime::sender::RedirectPolicy::Follow,
        ..Default::default()
    };
    let (_tx, rx) = tokio::sync::watch::channel(false);
    let err = wsw_runtime::sender::send_request(&rules, &req, &policy, &[], &rx)
        .await
        .unwrap_err();
    assert!(
        matches!(&err, SendError::Scope(code) if code == "metadata_endpoint"),
        "越界 Location → scope 拒（metadata deny-never-allow）"
    );
}

#[tokio::test]
async fn scope_default_deny_rejects_without_connecting() {
    let (port, received, _server) = spawn_target(Behavior::Marker).await;
    let rules = ScopeRules::default(); // include 空 = default deny
    let req = outbound("/x", &format!("127.0.0.1:{port}"));
    let policy = SendPolicy::default();
    let (_tx, rx) = tokio::sync::watch::channel(false);
    let err = wsw_runtime::sender::send_request(&rules, &req, &policy, &[], &rx)
        .await
        .unwrap_err();
    assert!(matches!(&err, SendError::Scope(code) if code == "no_rules_default_deny"));
    assert_eq!(received.lock().unwrap().len(), 0, "default deny 未建连");
    // 落库面：failed + scope_blocked（rp-wsw 映射面等价断言——子命令路径）。
    let store = mem_store();
    send_row(&store, "wsw-snd-deny-0001", "wsw-ses-test-0001");
    let outcome = wsw_runtime::sender::SendOutcome {
        state: "failed",
        error_code: Some("scope_blocked".into()),
        reason: Some("no_rules_default_deny".into()),
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
    wsw_runtime::sender::persist_send_failure(&store, "wsw-snd-deny-0001", &outcome).unwrap();
    let row = store.get_send("wsw-snd-deny-0001").unwrap().unwrap();
    assert_eq!(row.state, "failed");
    assert_eq!(row.error_code.as_deref(), Some("scope_blocked"));
}

#[tokio::test]
async fn total_timeout_yields_timed_out() {
    let (port, _received, _server) = spawn_target(Behavior::Silent).await;
    let rules = rules_for(port);
    let req = outbound("/x", &format!("127.0.0.1:{port}"));
    let policy = SendPolicy {
        read_timeout_ms: 200,
        total_timeout_ms: 500,
        ..Default::default()
    };
    let (_tx, rx) = tokio::sync::watch::channel(false);
    let err = wsw_runtime::sender::send_request(&rules, &req, &policy, &[], &rx)
        .await
        .unwrap_err();
    assert!(matches!(err, SendError::Timeout(_)), "total 超时 → Timeout（timed_out 面）");
}

#[tokio::test]
async fn cancel_watch_yields_cancelled() {
    let (port, _received, _server) = spawn_target(Behavior::Silent).await;
    let rules = rules_for(port);
    let req = outbound("/x", &format!("127.0.0.1:{port}"));
    let policy = SendPolicy {
        read_timeout_ms: 10_000,
        total_timeout_ms: 20_000,
        ..Default::default()
    };
    let (tx, rx) = tokio::sync::watch::channel(false);
    let handle = tokio::spawn({
        let rules = rules.clone();
        let rx = rx.clone();
        async move {
            wsw_runtime::sender::send_request(&rules, &req, &policy, &[], &rx).await
        }
    });
    // 发送已启动（建连挂读）→ 取消面置位。
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    tx.send(true).unwrap();
    let err = tokio::time::timeout(std::time::Duration::from_secs(3), handle)
        .await
        .expect("cancel settles quickly")
        .unwrap();
    assert!(
        matches!(err, Err(SendError::Cancelled)),
        "SIGTERM 取消面 → cancelled（非终态时）"
    );
}
