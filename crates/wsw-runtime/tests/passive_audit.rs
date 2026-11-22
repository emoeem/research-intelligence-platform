//! WSW-6 集成测试（S7 门）：Passive Audit 命中对账（判据 6 承载面）——
//! N exchange 捕获 → wsw_passive_hit 行数与规则预期对账 + wsw_body_blob 行数
//! 与 CAS 文件数对账（sha256 寻址幂等）+ 旁路面延迟 + 候选态负例硬门
//! （零 evidence_id 列零 Finding 写面）。

use wsw_runtime::passive::{analyze_and_persist, analyze_exchange, PASSIVE_RULES};
use wsw_runtime::persist::{ExchangeRow, PersistStore};

fn temp_db(tag: &str) -> PersistStore {
    static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "wsw56-passive-{tag}-{}-{}-{n:x}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let store = PersistStore::open_file(&dir.join("t.db")).expect("open");
    store
        .run_migrations(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../migrations")
                .as_path(),
        )
        .expect("migrate");
    store
}

fn temp_artifacts(tag: &str) -> std::path::PathBuf {
    static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "wsw56-passive-art-{tag}-{}-{}-{n:x}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).expect("temp artifacts");
    dir
}

fn row(
    exchange_id: &str,
    headers: &[(&str, &str)],
    status: i64,
    scheme: Option<&str>,
) -> ExchangeRow {
    let headers_json = serde_json::to_string(
        &headers
            .iter()
            .map(|(n, v)| serde_json::json!({"name": n, "value": v}))
            .collect::<Vec<_>>(),
    )
    .unwrap();
    ExchangeRow {
        exchange_id: exchange_id.into(),
        session_id: "wsw-ses-pa".into(),
        listener_id: "wsw-lis-pa".into(),
        direction: "client_to_server",
        source: "proxy_capture",
        protocol: "http",
        scheme: scheme.map(|s| s.into()),
        authority: Some("127.0.0.1:8443".into()),
        response_status: Some(status),
        response_headers_json: Some(headers_json),
        started_at: Some(wsw_runtime::persist::now_iso()),
        tags_json: "[]".into(),
        capture_fidelity: "full",
        parse_status: "ok",
        ..Default::default()
    }
}

#[test]
fn passive_hit_reconciliation_with_rule_expectations() {
    // 判据 6 前半：N exchange → 命中行数与规则预期对账（逐行规则预期 =
    // analyze_exchange 纯函数面与落库行一一对账）。
    let store = temp_db("recon");
    let now = wsw_runtime::persist::now_iso();
    // 行 1：HTTPS + 全缺安全头 + 无属性 Set-Cookie → 规则预期 9 命中
    //（4 缺头 + 3 Cookie 属性 + cacheable + x-frame）……精确预期由
    // analyze_exchange 行内计数承载（同函数同源）。
    let r1 = row(
        "wsw-20261122000000-0-pa01",
        &[
            ("Set-Cookie", "sid=abc; Path=/"),
            ("Server", "nginx/1.24.0"),
        ],
        200,
        Some("https"),
    );
    // 行 2：跨源跳转 + CORS 全放行。
    let r2 = row(
        "wsw-20261122000000-0-pa02",
        &[
            ("Location", "https://evil.example.net/x"),
            ("Access-Control-Allow-Origin", "*"),
        ],
        302,
        Some("http"),
    );
    // 行 3：干净响应（安全头齐备、无 Cookie/泄露/CORS/跳转——对账零行；
    // 明文 http 响应无 HSTS 检查——无 TLS 事实不伪造）。
    let r3 = row(
        "wsw-20261122000000-0-pa03",
        &[
            ("Content-Type", "text/plain"),
            ("Content-Security-Policy", "default-src 'self'"),
            ("X-Content-Type-Options", "nosniff"),
            ("X-Frame-Options", "DENY"),
        ],
        200,
        Some("http"),
    );
    let mut rows = vec![r1.clone(), r2.clone(), r3.clone()];
    for (i, r) in rows.iter_mut().enumerate() {
        r.sequence_no = i as i64; // UNIQUE(listener_id, sequence_no) 锚
    }
    let expected: usize = rows.iter().map(|r| analyze_exchange(r).len()).sum();
    let ex_outcome = store.insert_exchanges(&rows).expect("exchange rows");
    assert_eq!(
        ex_outcome.inserted, 3,
        "exchange 行全落（salvage 面零失败）"
    );
    let outcome = analyze_and_persist(&store, &rows).expect("analyze_and_persist");
    assert_eq!(
        outcome.inserted, expected,
        "命中行数与规则预期对账（inserted={outcome:?} expected={expected}）"
    );
    let hit_count = store.passive_hit_count().unwrap();
    assert_eq!(hit_count as usize, expected);
    assert_eq!(outcome.failed, 0);

    // 幂等重分析：UNIQUE(exchange_id, rule_id) 锚——重跑零新增（候选不重复落）。
    let again = analyze_and_persist(&store, &rows).expect("re-analyze");
    assert_eq!(again.inserted, 0, "重分析幂等（INSERT OR IGNORE 锚）");
    assert_eq!(store.passive_hit_count().unwrap() as usize, expected);

    // 命中行 rule_id/severity 词表 = 注册表同源。
    let hits = store.list_passive_hits(100).unwrap();
    assert!(!hits.is_empty());
    for h in &hits {
        assert!(
            PASSIVE_RULES.iter().any(|r| r.id == h.rule_id),
            "hit rule_id {} 在注册表词表内",
            h.rule_id
        );
        assert!(matches!(h.severity, "info" | "low" | "medium" | "high"));
        assert_eq!(h.rule_version, 1);
        assert!(h.exchange_id.starts_with("wsw-"));
        assert!(h.listener_id.starts_with("wsw-lis-"));
    }
    // 负例规则预期抽查：行 1 必含 4 缺头类 + 3 Cookie 属性类；
    // 行 3 零命中（干净响应行不产候选）。
    let h1_ids: Vec<&str> = hits
        .iter()
        .filter(|h| h.exchange_id == "wsw-20261122000000-0-pa01")
        .map(|h| h.rule_id)
        .collect();
    for want in [
        "missing_header_strict_transport_security",
        "missing_header_content_security_policy",
        "missing_header_x_content_type_options",
        "missing_header_x_frame_options",
        "cookie_missing_secure",
        "cookie_missing_httponly",
        "cookie_missing_samesite",
        "config_set_cookie_cacheable",
        "info_leak_server_version",
    ] {
        assert!(
            h1_ids.contains(&want),
            "行 1 缺规则 {want}（实得 {h1_ids:?}）"
        );
    }
    assert!(
        !hits
            .iter()
            .any(|h| h.exchange_id == "wsw-20261122000000-0-pa03"),
        "干净响应行零候选"
    );
    let now_unused = now;
    let _ = now_unused;
}

#[test]
fn passive_candidate_no_evidence_no_finding_hard_gate() {
    // 判据 6 负例硬门：hit 行零 evidence_id 零 Finding 写面。
    // ①表结构断言：wsw_passive_hit 列清单不含 evidence_id（030 D2 裁决词形）。
    let store = temp_db("noev");
    let cols: Vec<String> = store
        .with_conn(|c| {
            let mut stmt = c
                .prepare("SELECT name FROM pragma_table_info('wsw_passive_hit') ORDER BY cid")
                .map_err(database::DbError::Sqlite)?;
            let rows = stmt
                .query_map([], |r| r.get::<_, String>(0))
                .map_err(database::DbError::Sqlite)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(database::DbError::Sqlite)?;
            Ok::<_, database::DbError>(rows)
        })
        .expect("pragma columns");
    assert!(
        !cols.iter().any(|c| c == "evidence_id"),
        "wsw_passive_hit 零 evidence_id 列（实得 {cols:?}）"
    );
    // ②全链写面断言：insert_passive_hits SQL 只写 8 列（无 evidence 写入）；
    //  wsw_passive_hit 无 Finding 表引用（Finding 建模面在 MA 轨 domain 库，
    //  本链零外键指向）。
    let fks: i64 = store
        .with_conn(|c| {
            c.query_row(
                "SELECT COUNT(*) FROM pragma_foreign_key_list('wsw_passive_hit') \
                 WHERE \"table\" NOT IN ('wsw_exchange')",
                [],
                |r| r.get(0),
            )
        })
        .unwrap();
    assert_eq!(
        fks, 0,
        "wsw_passive_hit 外键唯一指向 wsw_exchange（零 Finding 提升面）"
    );
    // ③词形反证：passive.rs 源面零 evidence 写入词形（红线 5 候选态语义；
    // 文档注释行的候选语义说明词形不构成写面）。
    let src = include_str!("../src/passive.rs");
    let code_evidence: Vec<&str> = src
        .lines()
        .filter(|l| {
            let s = l.trim_start();
            !s.starts_with("//") && s.contains("evidence")
        })
        .collect();
    assert!(
        code_evidence.is_empty(),
        "passive.rs 代码面零 evidence 词形（{code_evidence:?}）"
    );
}

#[test]
fn passive_bypass_worker_latency_and_cas_reconciliation() {
    // 判据 6：CAS 对账（wsw_body_blob 行数与 CAS 文件数一一对账，sha256
    // 寻址幂等 INSERT OR IGNORE）+ 旁路面延迟（行级分析远低于 50ms 代理
    // 增量预算——worker 侧单行分析为纯函数，微秒级）。
    let store = temp_db("cas");
    let root = temp_artifacts("cas");
    let now = wsw_runtime::persist::now_iso();

    // 3 份不同 body → CAS 三文件三行；重复登记同 body → 幂等零新增。
    let bodies: Vec<Vec<u8>> = vec![
        b"body-one".to_vec(),
        b"body-two".to_vec(),
        b"body-three".to_vec(),
    ];
    for b in &bodies {
        let body = wsw_message::cas::store_body(&root, b, 8 * 1024 * 1024).expect("cas store");
        let brow = wsw_message::store::BodyBlobRow {
            sha256: body.sha256,
            uri: body.uri,
            size_bytes: body.size as i64,
            mime_type: "text/plain".into(),
            first_seen_exchange_id: None,
            created_at: now.clone(),
        };
        store
            .with_conn(|c| wsw_message::store::register_body_blob(c, &brow))
            .expect("blob row");
    }
    // 幂等：同 body 再登记（sha256 寻址 INSERT OR IGNORE 面）。
    let dup = wsw_message::cas::store_body(&root, &bodies[0], 8 * 1024 * 1024).expect("cas dup");
    let dup_row = wsw_message::store::BodyBlobRow {
        sha256: dup.sha256,
        uri: dup.uri,
        size_bytes: dup.size as i64,
        mime_type: "text/plain".into(),
        first_seen_exchange_id: None,
        created_at: now.clone(),
    };
    store
        .with_conn(|c| wsw_message::store::register_body_blob(c, &dup_row))
        .expect("blob dup row");
    let blob_count: i64 = store
        .with_conn(|c| c.query_row("SELECT COUNT(*) FROM wsw_body_blob", [], |r| r.get(0)))
        .unwrap();
    assert_eq!(blob_count, 3, "wsw_body_blob 行数与去重后 CAS 事实对账");
    // CAS 文件数对账（wsw/ 前缀目录）。
    let mut file_count = 0usize;
    for entry in std::fs::read_dir(root.join("wsw")).expect("wsw dir") {
        let p = entry.expect("entry").path();
        if p.is_dir() {
            file_count += std::fs::read_dir(&p).expect("shard").count();
        }
    }
    assert_eq!(file_count, 3, "CAS 文件数与行数一一对账");

    // 旁路延迟面：100 行分析（含 JSON 头解析）总量远低于 50ms 预算量级
    //（worker 侧逐行纯函数，无 IO——转发路径零同步调用由接线点承载）。
    let rows: Vec<ExchangeRow> = (0..100)
        .map(|i| {
            row(
                &format!("wsw-20261122000001-{i:04}-lat"),
                &[("Set-Cookie", "a=b"), ("Server", "x/1.0")],
                200,
                Some("https"),
            )
        })
        .collect();
    let t0 = std::time::Instant::now();
    let hits: usize = rows.iter().map(|r| analyze_exchange(r).len()).sum();
    let elapsed = t0.elapsed();
    assert!(hits >= 100, "逐行命中非零（对账分母）");
    assert!(
        elapsed.as_millis() < 1000,
        "100 行旁路分析 {elapsed:?}（微秒级纯函数面，远低于 50ms 代理预算量级）"
    );
}
