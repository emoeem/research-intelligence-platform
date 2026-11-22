//! WSW-5 集成测试（S3 门）：tokio-tungstenite 内存 echo 夹具 + 经代理真实
//! WS 连接（真实 loopback socket，禁模拟冒充——判据 1/2 承载面）。
//!
//! 链路：手工 RFC6455 绝对形式升级握手 → rp-wsw 同源 Proxy（WswHandler
//! handle_message 结构化捕获）→ tokio-tungstenite echo 靶机 → 帧回声。
//! 断言：握手行 protocol='ws' 单行 + 帧行与线缆帧一一对应 + opcode 全词表
//! （text/binary/ping/pong/close）+ sequence 连续零缺口 + direction 归向 +
//! echo 往返字节一致 + CAS payload 读回一致 + scope 外握手 403 负例。

use hudsucker::futures::{SinkExt, StreamExt};
use hudsucker::tokio_tungstenite::tungstenite::protocol::Role;
use hudsucker::tokio_tungstenite::tungstenite::Message;
use hudsucker::tokio_tungstenite::WebSocketStream;
use tokio::net::TcpStream;
use wsw_runtime::listener::handler::RuntimeOptions;
use wsw_runtime::listener::WswListener;
use wsw_runtime::scope::ScopeRules;

/// 测试自管目录（std-only；退出留对账不清理——selftest 同纪律）。
fn temp_dir(tag: &str) -> std::path::PathBuf {
    static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "wsw56-frame-{tag}-{}-{}-{n:x}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

/// 随机空闲 loopback 端口（bind :0 即取即弃；测试窗口内复用竞态可接受）。
fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .expect("bind :0")
        .local_addr()
        .expect("local addr")
        .port()
}

/// RFC6455 echo 靶机（tokio-tungstenite accept_async；text/binary 逐帧回声，
/// ping/pong 由协议层自动应答，close 自动收口）。
async fn echo_server(listener: tokio::net::TcpListener) {
    while let Ok((stream, _)) = listener.accept().await {
        tokio::spawn(async move {
            let ws = match hudsucker::tokio_tungstenite::accept_async(stream).await {
                Ok(ws) => ws,
                Err(_) => return,
            };
            let (mut sink, mut stream) = ws.split();
            while let Some(item) = stream.next().await {
                match item {
                    Ok(Message::Text(t)) => {
                        let _ = sink.send(Message::text(t)).await;
                    }
                    Ok(Message::Binary(b)) => {
                        let _ = sink.send(Message::binary(b)).await;
                    }
                    Ok(Message::Close(_)) | Err(_) => break,
                    _ => {} // ping/pong 协议层自动应答
                }
            }
            let _ = sink.close().await;
        });
    }
}

/// 绝对形式手工升级握手（tungstenite client_async 只写 origin-form——经
/// 代理面必须绝对形式 request-target，手工字节面承载；返回原始 socket 供
/// from_raw_socket 续接帧面）。
async fn proxy_ws_handshake(proxy_port: u16, target: &str) -> (TcpStream, u16) {
    let mut tcp = TcpStream::connect(("127.0.0.1", proxy_port))
        .await
        .expect("connect proxy");
    let host = format!("127.0.0.1:{proxy_port}");
    let req = format!(
        "GET {target} HTTP/1.1\r\nHost: {host}\r\nConnection: Upgrade\r\nUpgrade: websocket\r\n\
         Sec-WebSocket-Version: 13\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n\r\n"
    );
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    tcp.write_all(req.as_bytes())
        .await
        .expect("write handshake");
    let mut buf: Vec<u8> = Vec::new();
    let mut tmp = [0u8; 512];
    loop {
        let n = tcp.read(&mut tmp).await.expect("read handshake");
        assert!(n > 0, "proxy closed before handshake response");
        buf.extend_from_slice(&tmp[..n]);
        if buf.windows(4).any(|w| w == b"\r\n\r\n") {
            break;
        }
    }
    let head = String::from_utf8_lossy(&buf).to_string();
    let status: u16 = head
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    (tcp, status)
}

fn ws_rules() -> ScopeRules {
    ScopeRules::from_json(r#"{"include":["127.0.0.1"],"allow_private_targets":true}"#)
        .expect("rules")
}

/// 帧库行轮询等待（persist worker 50ms flush 批面——上限 5s）。
async fn wait_for<T: Send>(what: &str, mut probe: impl FnMut() -> Option<T> + Send) -> T {
    for _ in 0..100 {
        if let Some(v) = probe() {
            return v;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    panic!("timeout waiting for {what}");
}

#[tokio::test]
async fn ws_passthrough_capture_reconciliation_full_opcode_table() {
    let dir = temp_dir("e2e");
    let db = dir.join("ws.db");
    let artifacts = dir.join("artifacts");
    let ca = dir.join("ca");
    let echo_port = free_port();
    let echo_listener = tokio::net::TcpListener::bind(("127.0.0.1", echo_port))
        .await
        .expect("echo bind");
    tokio::spawn(echo_server(echo_listener));

    let proxy_port = free_port();
    let migrations = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations");
    let options = RuntimeOptions {
        listener_id: "wsw-lis-frame-test".to_string(),
        name: "wsw5 frame e2e".to_string(),
        session_id: "wsw-ses-frame-test".to_string(),
        config: wsw_runtime::ListenerConfig {
            bind: format!("127.0.0.1:{proxy_port}").parse().expect("bind"),
            ..wsw_runtime::ListenerConfig {
                bind: format!("127.0.0.1:{proxy_port}").parse().unwrap(),
                queue_capacity: 64,
                max_header_bytes: 64 * 1024,
                max_body_bytes: 8 * 1024 * 1024,
                queue_ttl_seconds: 120,
            }
        },
        scope_rules: ws_rules(),
        ca_dir: Some(ca),
        db_path: Some(db.clone()),
        artifacts_root: artifacts.clone(),
        ..Default::default()
    };
    let listener = WswListener::start(options, &migrations)
        .await
        .expect("listener start");

    // 升级握手（绝对形式 → 经代理 → echo 靶机 101）。
    let target = format!("http://127.0.0.1:{echo_port}/wsw5-test");
    let (tcp, status) = proxy_ws_handshake(proxy_port, &target).await;
    assert_eq!(status, 101, "代理侧升级握手必须 101（真实 socket 回显）");
    let ws = WebSocketStream::from_raw_socket(tcp, Role::Client, None).await;
    let (mut sink, mut stream) = ws.split();

    // 线缆帧：text/binary/ping 三类 + close 收口（协议层 pong 自动应答）。
    let text_payload = "wsw5-héllo-帧";
    let binary_payload: Vec<u8> = (0..=255u8).chain(0..16).collect();
    sink.send(Message::text(text_payload))
        .await
        .expect("send text");
    sink.send(Message::Binary(binary_payload.clone().into()))
        .await
        .expect("send binary");
    sink.send(Message::Ping(b"ping5".as_slice().into()))
        .await
        .expect("send ping");

    // 回声读面：text/binary echo + pong（协议层自动）——逐字节保真断言。
    let mut echo_text = None;
    let mut echo_binary = None;
    let mut saw_pong = false;
    loop {
        let msg = tokio::time::timeout(std::time::Duration::from_secs(5), stream.next())
            .await
            .expect("echo timeout")
            .expect("stream end before close")
            .expect("ws error");
        match msg {
            Message::Text(t) => echo_text = Some(t.to_string()),
            Message::Binary(b) => echo_binary = Some(b.to_vec()),
            Message::Pong(_) => saw_pong = true,
            Message::Close(_) => break,
            Message::Ping(_) => {}
            Message::Frame(_) => {}
        }
        if echo_text.is_some() && echo_binary.is_some() && saw_pong {
            break;
        }
    }
    assert_eq!(
        echo_text.as_deref(),
        Some(text_payload),
        "echo 往返字节一致（text）"
    );
    assert_eq!(
        echo_binary.as_deref(),
        Some(binary_payload.as_slice()),
        "echo 往返字节一致（binary）"
    );
    assert!(saw_pong, "ping→pong 协议层回环（server_to_client pong 帧）");
    sink.send(Message::Close(None)).await.expect("send close");

    // 落库对账（判据 1）：握手行 protocol='ws' = 1 + 帧行 = 8（双向 4+4）
    // + opcode 词表逐帧正确 + sequence 连续零缺口 + direction 归向。
    let store = wsw_runtime::persist::PersistStore::open_file(&db).expect("open read");
    wait_for("ws handshake row", || {
        let n = store
            .with_conn(|c| {
                c.query_row(
                    "SELECT COUNT(*) FROM wsw_exchange WHERE protocol='ws'",
                    [],
                    |r| r.get::<_, i64>(0),
                )
            })
            .ok()?;
        (n == 1).then_some(n)
    })
    .await;
    let total = wait_for("8 frame rows", || {
        let n = store.ws_frame_count().ok()?;
        (n == 8).then_some(n)
    })
    .await;
    assert_eq!(total, 8);

    let frames = store
        .with_conn(|c| {
            let mut stmt = c
                .prepare(
                    "SELECT frame_id, connection_exchange_id, direction, sequence_no, opcode, \
                        payload_len, payload_body_sha256, payload_body_uri, payload_body_len, \
                        payload_body_fidelity, masked, captured_at, scope_decision \
                 FROM wsw_ws_frame ORDER BY sequence_no",
                )
                .unwrap();
            let rows = stmt
                .query_map([], |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, String>(2)?,
                        r.get::<_, i64>(3)?,
                        r.get::<_, String>(4)?,
                        r.get::<_, i64>(5)?,
                        r.get::<_, Option<String>>(6)?,
                        r.get::<_, Option<String>>(7)?,
                        r.get::<_, Option<i64>>(8)?,
                        r.get::<_, Option<String>>(9)?,
                        r.get::<_, i64>(10)?,
                        r.get::<_, String>(11)?,
                        r.get::<_, Option<String>>(12)?,
                    ))
                })
                .unwrap()
                .collect::<Result<Vec<_>, _>>()
                .unwrap();
            Ok::<Vec<_>, database::DbError>(rows)
        })
        .expect("frame rows");

    // opcode 词表 + direction 归向 + masked 方向词形逐帧断言（按方向分组——
    // 双泵交错序由调度决定，非确定序；组内对账为确定面）。
    let conn_id = frames[0].1.clone();
    for (i, f) in frames.iter().enumerate() {
        assert_eq!(f.3, i as i64, "frame {i} sequence 连续零缺口（连接内单调）");
        assert_eq!(
            f.10,
            if f.2 == "client_to_server" { 1 } else { 0 },
            "frame {i} masked 方向词形"
        );
        assert_eq!(
            f.12.as_deref(),
            Some("in_scope"),
            "frame {i} scope_decision 三态"
        );
        assert_eq!(f.1, conn_id, "frame {i} 同连接握手行引用");
    }
    // 帧行元组投影（列序 = SELECT 同形；类型别名收编 clippy type_complexity）。
    type FrameRow = (
        String,
        String,
        String,
        i64,
        String,
        i64,
        Option<String>,
        Option<String>,
        Option<i64>,
        Option<String>,
        i64,
        String,
        Option<String>,
    );
    let group = |dir: &str| -> Vec<&FrameRow> { frames.iter().filter(|f| f.2 == dir).collect() };
    let c2s: std::collections::BTreeSet<&str> = group("client_to_server")
        .iter()
        .map(|f| f.4.as_str())
        .collect();
    let s2c: std::collections::BTreeSet<&str> = group("server_to_client")
        .iter()
        .map(|f| f.4.as_str())
        .collect();
    assert_eq!(
        c2s,
        ["binary", "close", "ping", "text"].into_iter().collect(),
        "client_to_server opcode 词表面"
    );
    assert_eq!(
        s2c,
        ["binary", "close", "pong", "text"].into_iter().collect(),
        "server_to_client opcode 词表面"
    );
    assert_eq!(group("client_to_server").len(), 4);
    assert_eq!(group("server_to_client").len(), 4);
    // 双泵交错序由调度决定（wire 观察序）：连接内全局 0..7 唯一零缺口已由
    // 逐帧断言承载；组内唯一性复证（UNIQUE(connection_exchange_id,
    // sequence_no) 锚）。
    for dir in ["client_to_server", "server_to_client"] {
        let mut seqs: Vec<i64> = group(dir).iter().map(|f| f.3).collect();
        seqs.sort();
        let mut uniq = seqs.clone();
        uniq.dedup();
        assert_eq!(seqs.len(), uniq.len(), "{dir} 组内 sequence 唯一（幂等锚）");
    }

    // opcode 全词表覆盖（PLAN :276/:678 五词形逐帧在库）。
    let opcodes: std::collections::BTreeSet<&str> = frames.iter().map(|f| f.4.as_str()).collect();
    assert_eq!(
        opcodes,
        ["binary", "close", "ping", "pong", "text"]
            .into_iter()
            .collect(),
        "opcode 全词表覆盖"
    );

    // CAS payload 读回（判据 2 前半）：text/binary 帧字节逐字节一致。
    let text_row = frames
        .iter()
        .find(|f| f.2 == "client_to_server" && f.4 == "text")
        .expect("text frame row");
    let binary_row = frames
        .iter()
        .find(|f| f.2 == "client_to_server" && f.4 == "binary")
        .expect("binary frame row");
    let text_bytes =
        wsw_message::cas::read_body(&artifacts, text_row.7.as_deref().expect("text uri"))
            .expect("read text cas");
    assert_eq!(String::from_utf8(text_bytes).unwrap(), text_payload);
    let binary_bytes =
        wsw_message::cas::read_body(&artifacts, binary_row.7.as_deref().expect("binary uri"))
            .expect("read binary cas");
    assert_eq!(binary_bytes, binary_payload);
    // CAS 寻址幂等：payload_len 与 payload_body_len 一致 + sha256 寻址形态。
    assert_eq!(binary_row.5, binary_payload.len() as i64);
    assert_eq!(binary_row.8, Some(binary_payload.len() as i64));
    assert_eq!(binary_row.9.as_deref(), Some("full"));
    assert_eq!(
        wsw_message::cas::sha256_hex(&binary_payload),
        binary_row.6.as_deref().expect("binary sha")
    );

    // 握手行形状（D3）：method=GET + scheme=ws + authority/path = 升级目标 +
    // capture_fidelity=degraded_no_wire（握手头区不经本挂点透出——W2-D9）。
    let hs = store
        .get_exchange_row(&conn_id)
        .expect("exchange row")
        .expect("handshake row present");
    assert_eq!(hs.protocol, "ws");
    assert_eq!(hs.source, "proxy_capture");
    assert_eq!(hs.method.as_deref(), Some("GET"));
    assert_eq!(hs.scheme.as_deref(), Some("ws"));
    assert_eq!(
        hs.authority.as_deref(),
        Some(format!("127.0.0.1:{echo_port}").as_str())
    );
    assert_eq!(hs.path.as_deref(), Some("/wsw5-test"));
    assert_eq!(hs.capture_fidelity, "degraded_no_wire");

    listener.stop().await.expect("listener stop");
}

#[tokio::test]
async fn ws_out_of_scope_upgrade_rejected_at_http_gate() {
    // 判据 8 负例（传输级）：include 127.0.0.1 之外目标 → 查点 A 403，
    // 升级握手拿不到 101，零 ws 握手行零帧行（scope 闸后端强制）。
    let dir = temp_dir("oos");
    let db = dir.join("ws.db");
    let echo_port = free_port();
    let echo_listener = tokio::net::TcpListener::bind(("127.0.0.1", echo_port))
        .await
        .expect("echo bind");
    tokio::spawn(echo_server(echo_listener));
    let proxy_port = free_port();
    let migrations = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations");
    let options = RuntimeOptions {
        listener_id: "wsw-lis-frame-oos".to_string(),
        session_id: "wsw-ses-frame-oos".to_string(),
        config: wsw_runtime::ListenerConfig {
            bind: format!("127.0.0.1:{proxy_port}").parse().expect("bind"),
            ..wsw_runtime::ListenerConfig {
                bind: format!("127.0.0.1:{proxy_port}").parse().unwrap(),
                queue_capacity: 64,
                max_header_bytes: 64 * 1024,
                max_body_bytes: 8 * 1024 * 1024,
                queue_ttl_seconds: 120,
            }
        },
        scope_rules: ws_rules(),
        ca_dir: Some(dir.join("ca")),
        db_path: Some(db.clone()),
        artifacts_root: dir.join("artifacts"),
        ..Default::default()
    };
    let listener = WswListener::start(options, &migrations)
        .await
        .expect("listener start");

    // scope 外目标 = include 未命中 host（127.0.0.2 loopback 亦闸外）。
    let (_, status) =
        proxy_ws_handshake(proxy_port, &format!("http://127.0.0.2:{echo_port}/x")).await;
    assert_eq!(status, 403, "scope 外升级握手必须 403");
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    let store = wsw_runtime::persist::PersistStore::open_file(&db).expect("open read");
    let ws_rows = store
        .with_conn(|c| {
            c.query_row(
                "SELECT COUNT(*) FROM wsw_exchange WHERE protocol='ws'",
                [],
                |r| r.get::<_, i64>(0),
            )
        })
        .expect("count ws rows");
    assert_eq!(ws_rows, 0, "scope 外零 WS 握手行");
    assert_eq!(
        store.ws_frame_count().expect("frame count"),
        0,
        "scope 外零帧行"
    );
    listener.stop().await.expect("listener stop");
}
