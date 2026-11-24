//! 025 exchange 落库链路集成测试（WSW-3-TASK §四.2 断言矩阵 S2/S3 面）：
//!
//! - 链尾水位断言（assert_watermark 方法 + user_version 直查，Projects/Research Assets 迁移后链尾 031（WSW-5/6 为 030））；
//! - 单行纪律：同请求+响应落一行；响应中断 → 响应体 NULL + parse_status
//!   ='partial'（W3-D1/D2.1）；
//! - 批量落行（insert_exchanges 单事务 + salvage 计数）；
//! - 背压满载丢弃（mpsc try_send Err(Full) → persist_dropped_total 计数 +
//!   push 返回 false，不阻塞——W3-D2.2）；
//! - tags/notes 编辑（D7 小控制行写面）；
//! - WAL 并发读写（open_file enable_wal + ro 连接并发可见）；
//! - 清扫兜底（双侧流末未触发的静默死亡 → 超龄 partial 行强制落库）。
//!
//! 纪律：只在测试内存库与临时副本执行；--test-threads=1。

use std::sync::Arc;
use std::time::Duration;
use wsw_runtime::listener::handler::{
    BodyOutcome, ExchangeCapture, ListenerShared, RuntimeOptions,
};
use wsw_runtime::persist::{
    ExchangeRow, PersistStore, BatchInsertOutcome, TabRow, TabVersionRow, SendRow, SendPatch,
};

fn mem_store() -> PersistStore {
    let s = PersistStore::open_memory().expect("open memory");
    s.run_migrations(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations").as_path())
        .expect("migrate full chain");
    s
}

fn shared_with_persist(store: PersistStore) -> Arc<ListenerShared> {
    Arc::new(ListenerShared::new(
        RuntimeOptions {
            ..Default::default()
        },
        Some(store),
    ))
}

fn body_full(bytes: &[u8], mime: &str) -> BodyOutcome {
    BodyOutcome {
        sha256: Some(wsw_message::cas::sha256_hex(bytes)),
        uri: Some(format!("wsw/{}/{}", &wsw_message::cas::sha256_hex(bytes)[..2], wsw_message::cas::sha256_hex(bytes))),
        len: Some(bytes.len() as i64),
        mime: mime.to_string(),
        fidelity: "full",
        snippet: wsw_runtime::listener::handler::body_snippet(bytes),
    }
}

fn body_interrupted(mime: &str) -> BodyOutcome {
    BodyOutcome {
        sha256: None,
        uri: None,
        len: None,
        mime: mime.to_string(),
        fidelity: "truncated",
        snippet: None,
    }
}

fn minimal_row(exchange_id: &str, seq: i64) -> ExchangeRow {
    ExchangeRow {
        exchange_id: exchange_id.to_string(),
        session_id: "wsw-ses-default".to_string(),
        listener_id: "wsw-lis-default".to_string(),
        sequence_no: seq,
        direction: "client_to_server",
        source: "proxy_capture",
        protocol: "http",
        capture_fidelity: "degraded_no_wire",
        parse_status: "partial",
        tags_json: "[]".to_string(),
        ..Default::default()
    }
}

#[test]
fn watermark_and_tables() {
    let s = mem_store();
    s.assert_watermark().expect("watermark");
    let uv: i64 = s
        .with_conn(|c| c.query_row("PRAGMA user_version", [], |r| r.get(0)))
        .unwrap();
    assert_eq!(uv, 31, "031 chain head");
    for table in [
        "wsw_exchange",
        "wsw_repeater_tab",
        "wsw_repeater_tab_version",
        "wsw_repeater_send",
    ] {
        let n: i64 = s
            .with_conn(|c| {
                c.query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?",
                    [&table],
                    |r| r.get(0),
                )
            })
            .unwrap();
        assert_eq!(n, 1, "025 table {table} present");
    }
}

#[test]
fn batch_insert_and_single_row_discipline() {
    let s = mem_store();
    // 单行纪律：请求+响应同行（一行两方向列），非拆两行。
    let rows = vec![ExchangeRow {
        method: Some("GET".into()),
        scheme: Some("http".into()),
        authority: Some("127.0.0.1:8443".into()),
        path: Some("/p59c-marker".into()),
        response_status: Some(200),
        request_headers_json: Some(r#"[{"name":"Host","value":"127.0.0.1:8443"}]"#.into()),
        response_headers_json: Some(r#"[{"name":"Content-Type","value":"text/plain"}]"#.into()),
        request_body_sha256: Some("ab".repeat(32)),
        request_body_len: Some(3),
        request_body_fidelity: Some("full"),
        response_body_len: Some(5),
        response_mime: Some("text/plain".into()),
        response_body_fidelity: Some("full"),
        started_at: Some(wsw_runtime::persist::epoch_ms_to_iso(1_000)),
        completed_at: Some(wsw_runtime::persist::epoch_ms_to_iso(1_050)),
        duration_ms: Some(50),
        ttfb_ms: Some(20),
        capture_fidelity: "degraded_no_wire",
        parse_status: "ok",
        search_text: Some("127.0.0.1:8443 /p59c-marker".into()),
        ..minimal_row("wsw-1-0-0001", 0)
    }];
    let outcome = s.insert_exchanges(&rows).unwrap();
    assert_eq!(
        outcome,
        BatchInsertOutcome { inserted: 1, failed: 0 },
        "single row = request+response same row (one INSERT)"
    );
    assert_eq!(s.exchange_count().unwrap(), 1);
    let (method, status, parse): (String, i64, String) = s
        .with_conn(|c| {
            c.query_row(
                "SELECT method, response_status, parse_status FROM wsw_exchange WHERE exchange_id='wsw-1-0-0001'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
        })
        .unwrap();
    assert_eq!(method, "GET");
    assert_eq!(status, 200);
    assert_eq!(parse, "ok");
}

#[test]
fn response_interrupted_partial_row() {
    let shared = shared_with_persist(mem_store());
    let mut rx = shared.take_persist_rx().expect("rx present with persist");
    let started = wsw_runtime::persist::epoch_ms_to_iso(2_000);
    let _ = started;
    let capture = ExchangeCapture::__test_new(&shared, "wsw-2-0-0001", 0, 2_000);
    // 请求侧完整：headers + body。
    let mut headers = hudsucker::hyper::HeaderMap::new();
    headers.insert("host", hudsucker::hyper::header::HeaderValue::from_static("127.0.0.1:8443"));
    capture.register_request_headers(&headers);
    capture.register_request_body(body_full(b"abc", "text/plain"));
    // 响应头到了，体流被连接中断（Drop 语义 = interrupted outcome）。
    capture.register_response_head(200, &headers);
    capture.register_response_body(body_interrupted("text/plain"));
    // 响应侧登记即推行。
    let row = rx.try_recv().expect("row pushed on response registration");
    assert_eq!(row.exchange_id, "wsw-2-0-0001");
    assert_eq!(row.response_status, Some(200), "响应头已到：状态列落");
    assert_eq!(row.response_body_sha256, None, "中断体不落 CAS（NULL）");
    assert_eq!(row.parse_status, "partial", "响应中断 → partial");
    assert_eq!(row.request_body_fidelity, Some("full"));
    // 同请求+响应只落一行：channel 恰一条。
    assert!(rx.try_recv().is_err(), "single row discipline: exactly one row");
}

#[test]
fn response_never_arrives_partial_after_sweep() {
    let shared = shared_with_persist(mem_store());
    let mut rx = shared.take_persist_rx().expect("rx");
    let capture = ExchangeCapture::__test_new(&shared, "wsw-3-0-0001", 0, 5_000);
    let mut headers = hudsucker::hyper::HeaderMap::new();
    headers.insert("host", hudsucker::hyper::header::HeaderValue::from_static("127.0.0.1:8443"));
    capture.register_request_headers(&headers);
    capture.register_request_body(body_full(b"abc", "text/plain"));
    // 无响应且双侧流末未触发 → 清扫兜底（判龄改写绕过 10s 实等）。
    capture.__test_set_started_ms(1);
    assert_eq!(
        capture.sweep_push_stale(20_000),
        1,
        "stale capture swept into one partial row"
    );
    let row = rx.try_recv().expect("swept row");
    assert_eq!(row.response_status, None, "响应未到 = 响应列 NULL");
    assert_eq!(row.parse_status, "partial");
    // 二次清扫不再重复落行。
    assert_eq!(capture.sweep_push_stale(30_000), 0);
}

#[test]
fn backpressure_channel_full_drops_without_blocking() {
    let shared = shared_with_persist(mem_store());
    let rx = shared.take_persist_rx().expect("rx");
    // 满载注入 2048（cap=2048）。
    for i in 0..2048 {
        let row = minimal_row(&format!("wsw-4-{i}-0001"), i);
        assert!(shared.push_exchange_row(row), "row {i} should enqueue");
    }
    let t0 = std::time::Instant::now();
    let row = minimal_row("wsw-4-over-1", 100_000);
    assert!(
        !shared.push_exchange_row(row),
        "满载 → try_send 失败返回 false（不阻塞不等待）"
    );
    let elapsed = t0.elapsed();
    assert!(
        elapsed < Duration::from_millis(50),
        "满载拒绝必须即时（实测 {elapsed:?} < 50ms）"
    );
    assert_eq!(
        shared
            .counters
            .persist_dropped_total
            .load(std::sync::atomic::Ordering::Relaxed),
        1,
        "persist_dropped_total 计数可见"
    );
    // channel 内恰 2048 条（计数面分母 = 入通道数）。
    assert_eq!(
        shared
            .counters
            .persist_enqueued_total
            .load(std::sync::atomic::Ordering::Relaxed),
        2048
    );
    drop(rx); // 通道排空
}

#[test]
fn tags_notes_edit_updates_row() {
    let s = mem_store();
    s.insert_exchanges(&[minimal_row("wsw-5-0-0001", 0)]).unwrap();
    let ok = s
        .update_exchange_tags_notes("wsw-5-0-0001", r#"["login","target-a"]"#, Some("复核通过"))
        .unwrap();
    assert!(ok);
    let (tags, notes): (String, String) = s
        .with_conn(|c| {
            c.query_row(
                "SELECT tags_json, notes FROM wsw_exchange WHERE exchange_id='wsw-5-0-0001'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
        })
        .unwrap();
    assert_eq!(tags, r#"["login","target-a"]"#);
    assert_eq!(notes, "复核通过");
    // 不存在的行 = false。
    assert!(!s
        .update_exchange_tags_notes("wsw-nonexistent", "[]", None)
        .unwrap());
}

#[test]
fn wal_enabled_and_concurrent_read_visible() {
    let dir = std::env::temp_dir().join(format!(
        "wsw3-exchange-wal-{}-{:?}",
        std::process::id(),
        std::time::SystemTime::now()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let db = dir.join("wal.db");
    let store = PersistStore::open_file(&db).expect("open file store");
    store
        .run_migrations(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations").as_path())
        .unwrap();
    // WAL 开启（open_file 内 enable_wal；W3-D2.4）。
    let mode: String = store
        .with_conn(|c| c.query_row("PRAGMA journal_mode", [], |r| r.get(0)))
        .unwrap();
    assert_eq!(mode, "wal", "enable_wal 接线生效");
    store.insert_exchanges(&[minimal_row("wsw-6-0-0001", 0)]).unwrap();
    // 并发只读连接（rp_server mode=ro 同型）立即可见（WAL 快照面）。
    let ro = database::Pool::open(database::PoolConfig::readonly_file(&db)).unwrap();
    let n: i64 = ro
        .with_conn(|c| c.query_row("SELECT COUNT(*) FROM wsw_exchange", [], |r| r.get(0)))
        .unwrap();
    assert_eq!(n, 1, "WAL 下 ro 连接写后可见");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn invalid_row_salvaged_out_of_batch() {
    let s = mem_store();
    let mut bad = minimal_row("wsw-7-bad-0001", 0);
    bad.session_id = "not-a-session".to_string(); // CHECK 词表外 → 单行拒
    let good = minimal_row("wsw-7-good-0001", 1);
    let outcome = s.insert_exchanges(&[bad, good]).unwrap();
    assert_eq!(
        outcome,
        BatchInsertOutcome { inserted: 1, failed: 1 },
        "salvage 语义：坏行只跳过计数，好行照落"
    );
    assert_eq!(s.exchange_count().unwrap(), 1);
}

#[test]
fn repeater_tab_version_append_only_and_send_rows() {
    let s = mem_store();
    let tab = TabRow {
        tab_id: wsw_runtime::persist::generate_tab_id(),
        session_id: "wsw-ses-default".to_string(),
        title: "t1".to_string(),
        created_at: wsw_runtime::persist::now_iso(),
        updated_at: wsw_runtime::persist::now_iso(),
        closed_at: None,
    };
    s.create_tab(&tab).unwrap();
    // append-only：连续 3 版本 3 行；同 (tab_id, version_no) 重写报错。
    for v in 1..=3 {
        s.append_tab_version(&TabVersionRow {
            tab_id: tab.tab_id.clone(),
            version_no: v,
            request_json: format!(r#"{{"version":{v}}}"#),
            source_exchange_id: None,
            origin: "manual".to_string(),
            created_at: wsw_runtime::persist::now_iso(),
        })
        .unwrap();
    }
    assert_eq!(s.list_tab_versions(&tab.tab_id, 500).unwrap().len(), 3);
    let dup = TabVersionRow {
        tab_id: tab.tab_id.clone(),
        version_no: 2,
        request_json: "x".into(),
        source_exchange_id: None,
        origin: "manual".to_string(),
        created_at: wsw_runtime::persist::now_iso(),
    };
    assert!(s.append_tab_version(&dup).is_err(), "UNIQUE(tab_id,version_no) 锚：重写必拒");
    // send 行六态推进：pending → sending → succeeded（终态不可再推）。
    let send = SendRow {
        send_id: wsw_runtime::persist::generate_send_id(),
        tab_id: tab.tab_id.clone(),
        version_no: 3,
        session_id: "wsw-ses-default".to_string(),
        state: "pending".to_string(),
        created_at: wsw_runtime::persist::now_iso(),
        ..Default::default()
    };
    s.insert_send(&send).unwrap();
    assert!(s
        .advance_send(&send.send_id, &SendPatch { state: "sending", started_at: Some(wsw_runtime::persist::now_iso()), ..Default::default() })
        .unwrap());
    assert!(s
        .advance_send(&send.send_id, &SendPatch { state: "succeeded", response_status: Some(200), completed_at: Some(wsw_runtime::persist::now_iso()), duration_ms: Some(12), ..Default::default() })
        .unwrap());
    // 终态后推进 = false（SQL WHERE state IN ('pending','sending') 强制）。
    assert!(!s
        .advance_send(&send.send_id, &SendPatch { state: "failed", ..Default::default() })
        .unwrap());
    let got = s.get_send(&send.send_id).unwrap().unwrap();
    assert_eq!(got.state, "succeeded");
    assert_eq!(got.response_status, Some(200));
    assert_eq!(s.list_sends(&tab.tab_id, 500).unwrap().len(), 1);
    // 词表越界拒。
    assert!(s
        .advance_send(&send.send_id, &SendPatch { state: "exploded", ..Default::default() })
        .is_err());
}

#[test]
fn repeater_tab_close_and_open_list() {
    let s = mem_store();
    let tab = TabRow {
        tab_id: wsw_runtime::persist::generate_tab_id(),
        session_id: "wsw-ses-default".to_string(),
        title: "t2".to_string(),
        created_at: wsw_runtime::persist::now_iso(),
        updated_at: wsw_runtime::persist::now_iso(),
        closed_at: None,
    };
    s.create_tab(&tab).unwrap();
    assert_eq!(s.list_open_tabs("wsw-ses-default").unwrap().len(), 1);
    assert!(s.close_tab(&tab.tab_id, &wsw_runtime::persist::now_iso()).unwrap());
    assert!(s.list_open_tabs("wsw-ses-default").unwrap().is_empty());
}

#[test]
fn latest_version_readback_for_editor() {
    let s = mem_store();
    let tab = TabRow {
        tab_id: wsw_runtime::persist::generate_tab_id(),
        session_id: "wsw-ses-default".to_string(),
        title: "t3".to_string(),
        created_at: wsw_runtime::persist::now_iso(),
        updated_at: wsw_runtime::persist::now_iso(),
        closed_at: None,
    };
    s.create_tab(&tab).unwrap();
    s.append_tab_version(&TabVersionRow {
        tab_id: tab.tab_id.clone(),
        version_no: 1,
        request_json: r#"{"method":"GET"}"#.into(),
        source_exchange_id: Some("wsw-9-0-0001".into()),
        origin: "from_history".to_string(),
        created_at: wsw_runtime::persist::now_iso(),
    })
    .unwrap();
    let latest = s.latest_tab_version(&tab.tab_id).unwrap().unwrap();
    assert_eq!(latest.version_no, 1);
    assert_eq!(latest.origin, "from_history");
    assert_eq!(latest.source_exchange_id.as_deref(), Some("wsw-9-0-0001"));
}

#[test]
fn insert_batch_throughput_measurement() {
    // S9 落库吞吐实测（WSW-3-TASK §四.4：无预设门值，PLAN :754 禁预填——
    // 数字打印进批次报告 F 节登记，不做吞吐断言）。临时文件库（WAL 开启
    // 同 listener 实态）× 128 行/批 × 40 批 = 5120 行。
    let dir = std::env::temp_dir().join(format!(
        "wsw3-throughput-{}-{:?}",
        std::process::id(),
        std::time::SystemTime::now()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let store = PersistStore::open_file(&dir.join("throughput.db")).expect("open file store");
    store
        .run_migrations(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations").as_path())
        .unwrap();
    let started = std::time::Instant::now();
    const BATCHES: usize = 40;
    const ROWS_PER_BATCH: usize = 128;
    let mut seq = 0i64;
    for _ in 0..BATCHES {
        let rows: Vec<ExchangeRow> = (0..ROWS_PER_BATCH)
            .map(|_| {
                seq += 1;
                minimal_row(
                    &format!("wsw-{}-0-{seq:06}", std::process::id() % 100_000),
                    seq,
                )
            })
            .collect();
        let outcome = store.insert_exchanges(&rows).expect("batch insert");
        assert_eq!(outcome.inserted, ROWS_PER_BATCH);
    }
    let elapsed = started.elapsed();
    let total = BATCHES * ROWS_PER_BATCH;
    println!(
        "insert throughput: {:.0} rows/s ({total} rows in {:.1}ms, {BATCHES} batches x {ROWS_PER_BATCH} rows, file WAL db)",
        total as f64 / elapsed.as_secs_f64(),
        elapsed.as_secs_f64() * 1000.0,
    );
    assert_eq!(store.exchange_count().unwrap(), total as u64);
    let _ = std::fs::remove_dir_all(&dir);
}
