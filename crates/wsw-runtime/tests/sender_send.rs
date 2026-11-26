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
use wsw_runtime::sender::{SendError, SendPolicy};

/// 靶场请求字节收集器（逐字节回读断言锚点）。
type ReceivedBytes = Arc<std::sync::Mutex<Vec<Vec<u8>>>>;

/// 本地 HTTP 靶场：GET → 200 marker；其余按 behavior。
#[derive(Clone)]
enum Behavior {
    /// 200 + marker 体。
    Marker,
    /// 302 + Location（自跳）；收到第 N 个请求后返回 200。
    RedirectLoop(usize),
    /// 302 → include 外域（越界 Location）。
    RedirectOut,
    /// 首请求 → 302 + 给定 Location（相对引用/protocol-relative 形态断言
    /// 面，W3 修订批）；其后 200 marker。
    RedirectRel(String),
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
            let behavior = behavior.clone();
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
                    Behavior::RedirectRel(loc) => {
                        if count == 1 {
                            format!(
                                "HTTP/1.1 302 Found\r\nLocation: {loc}\r\nContent-Length: 0\r\n\r\n"
                            )
                            .into_bytes()
                        } else {
                            b"HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: 13\r\n\r\nwsw3-marker\r\n".to_vec()
                        }
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
        .find_map(|l| {
            l.strip_prefix("Host: ")
                .and_then(|v| v.rsplit_once(':'))
                .and_then(|(_, p)| p.parse().ok())
        })
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
    s.run_migrations(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../migrations")
            .as_path(),
    )
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
        .advance_send(
            send_id,
            &SendPatch {
                state: "sending",
                ..Default::default()
            },
        )
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
        sent_bytes,
        serialized.as_bytes().to_vec(),
        "靶场收到的字节 = serialize_request 产物（编辑头序保持，hyper 写路径否决的正面证据）"
    );
    assert_eq!(
        outcome.outbound_bytes,
        serialized.as_bytes().to_vec(),
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
        &store,
        &artifacts,
        "wsw-snd-rt-0001",
        session_id,
        "wsw-lis-test",
        "wsw-rpt-test-0001",
        1,
        &req,
        &outcome,
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

/// 靶场收到的第 n 个请求的 origin-form 请求行（最终请求 path 断言锚点）。
fn request_line(received: &ReceivedBytes, n: usize) -> String {
    let buf = received.lock().unwrap()[n].clone();
    String::from_utf8_lossy(&buf)
        .lines()
        .next()
        .unwrap_or_default()
        .to_string()
}

#[tokio::test]
async fn redirect_relative_reference_joins_base_directory() {
    // Location: bar（RFC 3986 relative-path）自 /a/b/c：并入基准目录 →
    // /a/b/bar（无 // 双斜杠；修订批 F2 回归钉：旧实现拼出 //a/bar）。
    let (port, received, _server) = spawn_target(Behavior::RedirectRel("bar".into())).await;
    let rules = rules_for(port);
    let req = outbound("/a/b/c", &format!("127.0.0.1:{port}"));
    let policy = SendPolicy {
        redirect: wsw_runtime::sender::RedirectPolicy::Follow,
        ..Default::default()
    };
    let (_tx, rx) = tokio::sync::watch::channel(false);
    let outcome = wsw_runtime::sender::send_request(&rules, &req, &policy, &[], &rx)
        .await
        .expect("relative-path follow");
    assert_eq!(outcome.response_status, Some(200));
    assert_eq!(
        request_line(&received, 1),
        "GET /a/b/bar HTTP/1.1",
        "相对段并入基准目录，无双斜杠"
    );
    assert_eq!(
        outcome.redirect_chain[0].url,
        format!("http://127.0.0.1:{port}/a/b/bar"),
        "redirect_chain url 同面无双斜杠"
    );
}

#[tokio::test]
async fn redirect_dot_dot_reference_normalizes() {
    // Location: ../up 自 /a/b/c：../ 段归一化（RFC 3986 §5.2.4）→ /a/up。
    let (port, received, _server) = spawn_target(Behavior::RedirectRel("../up".into())).await;
    let rules = rules_for(port);
    let req = outbound("/a/b/c", &format!("127.0.0.1:{port}"));
    let policy = SendPolicy {
        redirect: wsw_runtime::sender::RedirectPolicy::Follow,
        ..Default::default()
    };
    let (_tx, rx) = tokio::sync::watch::channel(false);
    let outcome = wsw_runtime::sender::send_request(&rules, &req, &policy, &[], &rx)
        .await
        .expect("dot-dot follow");
    assert_eq!(outcome.response_status, Some(200));
    assert_eq!(
        request_line(&received, 1),
        "GET /a/up HTTP/1.1",
        "../ 段归一化（非字面拼进 path）"
    );
    assert_eq!(
        outcome.redirect_chain[0].url,
        format!("http://127.0.0.1:{port}/a/up"),
        "redirect_chain url 归一化同面"
    );
}

#[tokio::test]
async fn redirect_relative_from_root_path_no_double_slash() {
    // Location: bar 自 /hop（基准目录 = 根）：/bar——修复面：旧实现空目录
    // 段拼出 //bar。
    let (port, received, _server) = spawn_target(Behavior::RedirectRel("bar".into())).await;
    let rules = rules_for(port);
    let req = outbound("/hop", &format!("127.0.0.1:{port}"));
    let policy = SendPolicy {
        redirect: wsw_runtime::sender::RedirectPolicy::Follow,
        ..Default::default()
    };
    let (_tx, rx) = tokio::sync::watch::channel(false);
    let outcome = wsw_runtime::sender::send_request(&rules, &req, &policy, &[], &rx)
        .await
        .expect("root-base relative follow");
    assert_eq!(outcome.response_status, Some(200));
    assert_eq!(
        request_line(&received, 1),
        "GET /bar HTTP/1.1",
        "根基准目录相对段无双斜杠"
    );
    assert_eq!(
        outcome.redirect_chain[0].url,
        format!("http://127.0.0.1:{port}/bar"),
        "根基准 redirect_chain url 无双斜杠"
    );
}

#[tokio::test]
async fn redirect_protocol_relative_switches_host() {
    // Location: //127.0.0.1:{port_b}/other（RFC 3986 network-path 引用）：
    // host/port 切换到第二靶场（继承 http scheme），不误并进本 host path
    // （修复面：旧实现当普通相对段拼出 //other/...）。
    let (port_b, received_b, _s2) = spawn_target(Behavior::Marker).await;
    let (port_a, _received_a, _s1) =
        spawn_target(Behavior::RedirectRel(format!("//127.0.0.1:{port_b}/other"))).await;
    let rules = ScopeRules {
        include: vec!["127.0.0.1".to_string()],
        allow_private_targets: true,
        allowed_ports: vec![port_a, port_b],
        ..Default::default()
    };
    let req = outbound("/start", &format!("127.0.0.1:{port_a}"));
    let policy = SendPolicy {
        redirect: wsw_runtime::sender::RedirectPolicy::Follow,
        ..Default::default()
    };
    let (_tx, rx) = tokio::sync::watch::channel(false);
    let outcome = wsw_runtime::sender::send_request(&rules, &req, &policy, &[], &rx)
        .await
        .expect("protocol-relative follow");
    assert_eq!(outcome.response_status, Some(200));
    assert_eq!(outcome.redirect_chain.len(), 1);
    assert_eq!(
        outcome.redirect_chain[0].url,
        format!("http://127.0.0.1:{port_b}/other"),
        "protocol-relative → host/port 切换"
    );
    let buf_b = received_b.lock().unwrap();
    assert_eq!(buf_b.len(), 1, "第二请求落到 port_b 靶场");
    let head = String::from_utf8_lossy(&buf_b[0]);
    assert!(
        head.starts_with("GET /other HTTP/1.1\r\n"),
        "最终请求 origin-form = /other"
    );
    assert!(
        head.contains(&format!("Host: 127.0.0.1:{port_b}")),
        "Host 头重写为新 authority"
    );
}

#[tokio::test]
async fn redirect_policy_cap_tightens_below_scope_rules() {
    // policy.max_redirects=2 < rules 默认 10：生效帽 = min → 第 3 跳拒
    // （policy 帽消费面，修订批 F1：此前该字段定义未消费，--policy 配置
    // max_redirects 静默失效）。
    let (port, received, _server) = spawn_target(Behavior::RedirectLoop(99)).await;
    let rules = rules_for(port);
    let req = outbound("/hop", &format!("127.0.0.1:{port}"));
    let policy = SendPolicy {
        redirect: wsw_runtime::sender::RedirectPolicy::Follow,
        max_redirects: 2,
        ..Default::default()
    };
    let (_tx, rx) = tokio::sync::watch::channel(false);
    let err = wsw_runtime::sender::send_request(&rules, &req, &policy, &[], &rx)
        .await
        .unwrap_err();
    assert!(
        matches!(&err, SendError::Scope(code) if code == "redirect_limit"),
        "policy 帽拒 = redirect_limit 同码"
    );
    assert_eq!(
        received.lock().unwrap().len(),
        3,
        "首跳 + 2 次跟随；第 3 跳 policy 帽拒（rules 帽 10 不触发）"
    );
}

#[tokio::test]
async fn redirect_scope_rules_cap_bounds_policy() {
    // policy.max_redirects=10 > rules.max_redirects=1：生效帽 = rules——
    // --policy JSON 不可放大 scope 安全帽（min 收口另一侧）。
    let (port, received, _server) = spawn_target(Behavior::RedirectLoop(99)).await;
    let rules = ScopeRules {
        max_redirects: 1,
        ..rules_for(port)
    };
    let req = outbound("/hop", &format!("127.0.0.1:{port}"));
    let policy = SendPolicy {
        redirect: wsw_runtime::sender::RedirectPolicy::Follow,
        max_redirects: 10,
        ..Default::default()
    };
    let (_tx, rx) = tokio::sync::watch::channel(false);
    let err = wsw_runtime::sender::send_request(&rules, &req, &policy, &[], &rx)
        .await
        .unwrap_err();
    assert!(
        matches!(&err, SendError::Scope(code) if code == "redirect_limit"),
        "rules 帽兜底拒"
    );
    assert_eq!(
        received.lock().unwrap().len(),
        2,
        "首跳 + 1 次跟随；第 2 跳 rules 帽拒（policy 10 不放大）"
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
    assert!(
        matches!(err, SendError::Timeout(_)),
        "total 超时 → Timeout（timed_out 面）"
    );
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
        async move { wsw_runtime::sender::send_request(&rules, &req, &policy, &[], &rx).await }
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
