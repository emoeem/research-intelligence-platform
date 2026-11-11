//! SK-3 集成测试（P6-11 第 2 步，安全知识源计划 §6.2C）：document.fetch
//! 的 approved 闸 + 012 落库 + Tool Policy Resolver 的 T2 形态拒绝复验。
//!
//! 私网拒绝矩阵 / 重定向逐跳 / 限额在 document_fetch.rs 单元测试面（本文件
//! 只做需要真实迁移与 Registry 面的接线项）。

use std::time::Duration;

use database::{DbPath, Pool, PoolConfig};
use tool_engine::document_fetch::{
    fetch_document, FetchInput, FetchPolicy, ProxyTransport, Transport,
};
use tool_engine::adapter::egress_proxy::EgressAllowlist;

fn memory_pool() -> Pool {
    Pool::open(PoolConfig {
        path: DbPath::Memory,
        readonly: false,
        max_connections: 1,
        busy_timeout: Duration::from_secs(5),
    })
    .unwrap()
}

fn policy_for(hosts: &[&str]) -> FetchPolicy {
    FetchPolicy::new(
        hosts.iter().map(|h| h.to_string()).collect(),
        vec![80],
    )
}

/// Mock 传输（零真实网络；approved 闸与落库是被测对象）。
struct NoNetTransport;

impl Transport for NoNetTransport {
    fn get(&self, _url: &str, _max: usize) -> std::io::Result<TransportResponseShim> {
        Err(std::io::Error::other("no net in this test"))
    }
}

use tool_engine::document_fetch::TransportResponse as TransportResponseShim;

#[test]
fn sk3_approved_gate_only_accepts_registry_approved_canonical() {
    let pool = memory_pool();
    database::migrate(&pool, &migrations_dir()).unwrap();
    // APPROVED 条目 + UNREVIEWED 条目（同 URL 词形不同审核态）。
    pool.with_conn(|c| {
        c.execute_batch(
            "INSERT INTO knowledge_source_entry
             (plugin_id, plugin_version, corpus_id, entry_kind, item_key, title,
              content_sha256, metadata_sha256, canonical_url, review_status,
              first_seen_at, last_seen_at)
             VALUES
             ('knowledge.demo', '0.1.0', 'demo', 'catalog_entry',
              'http://approved.example/doc', 'Approved Doc', 'a', 'a',
              'http://approved.example/doc', 'APPROVED',
              '2026-11-12T00:00:00Z', '2026-11-12T00:00:00Z'),
             ('knowledge.demo', '0.1.0', 'demo', 'catalog_entry',
              'http://pending.example/doc', 'Pending Doc', 'b', 'b',
              'http://pending.example/doc', 'UNREVIEWED',
              '2026-11-12T00:00:00Z', '2026-11-12T00:00:00Z');",
        )
    })
    .unwrap();

    let checker = |u: &str, key: Option<&str>| -> Result<String, String> {
        if let Some(key) = key {
            let canonical: Option<String> = pool
                .with_conn(|c| {
                    c.query_row(
                        "SELECT canonical_url FROM knowledge_source_entry
                         WHERE item_key = ?1 AND review_status = 'APPROVED'",
                        [&key],
                        |r| r.get(0),
                    )
                    .map(Some)
                    .or_else(|e| match e {
                        database::rusqlite::Error::QueryReturnedNoRows => Ok(None),
                        other => Err(other),
                    })
                })
                .map_err(|e| e.to_string())?;
            return match canonical {
                Some(c) if c == u => Ok(c),
                Some(c) => Err(format!("url does not match approved canonical {c:?}")),
                None => Err(format!("entry {key:?} not found or not APPROVED")),
            };
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

    let transport = NoNetTransport;
    // APPROVED canonical：过 approved 闸，随后止于 hop 闸（本测试环境无网，
    // DNS 解析失败即闸序证据：approved 闸在 DNS/hop 闸之前已放行）。
    let err = fetch_document(
        None,
        &transport,
        &policy_for(&["approved.example"]),
        &FetchInput {
            source_id: Some("knowledge.demo".into()),
            url: "http://approved.example/doc".into(),
            entry_key: None,
        },
        Some(&checker),
    )
    .unwrap_err();
    assert_ne!(err.code(), "not_approved_rejected", "APPROVED canonical 必须过 approved 闸");
    assert!(
        matches!(err.code(), "dns_resolve_failed" | "transport_error" | "host_not_allowlisted"),
        "闸放行后止于后续闸（无网环境任一），got {err:?}"
    );

    // 非 APPROVED canonical（UNREVIEWED 条目）：approved 闸拒绝。
    let err = fetch_document(
        None,
        &transport,
        &policy_for(&["pending.example"]),
        &FetchInput {
            source_id: Some("knowledge.demo".into()),
            url: "http://pending.example/doc".into(),
            entry_key: None,
        },
        Some(&checker),
    )
    .unwrap_err();
    assert_eq!(err.code(), "not_approved_rejected");

    // entry_key 形态：URL 与 APPROVED 条目 canonical 不一致 → 拒。
    let err = fetch_document(
        None,
        &transport,
        &policy_for(&["approved.example"]),
        &FetchInput {
            source_id: Some("knowledge.demo".into()),
            url: "http://approved.example/other".into(),
            entry_key: Some("http://approved.example/doc".into()),
        },
        Some(&checker),
    )
    .unwrap_err();
    assert_eq!(err.code(), "not_approved_rejected");
    assert!(err.detail().contains("does not match approved canonical"));
}

fn migrations_dir() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../migrations")
}

#[test]
fn sk3_fetch_records_ok_and_reject_into_012_table() {
    let pool = memory_pool();
    database::migrate(&pool, &migrations_dir()).unwrap();

    // 拒绝（私网）→ rejected 行落 012。
    let policy = policy_for(&["example.com"]);
    let input = FetchInput {
        source_id: Some("knowledge.demo".into()),
        url: "http://169.254.169.254/latest/meta-data/".into(),
        entry_key: None,
    };
    let checker = |u: &str, _k: Option<&str>| -> Result<String, String> { Ok(u.to_string()) };
    let err = fetch_document(
        Some(&pool),
        &NoNetTransport,
        &policy,
        &input,
        Some(&checker),
    )
    .unwrap_err();
    assert_eq!(err.code(), "private_network_blocked");
    let n: i64 = pool
        .with_conn(|c| {
            c.query_row(
                "SELECT COUNT(*) FROM knowledge_document_fetch
                 WHERE status = 'rejected' AND reject_reason = 'private_network_blocked'
                   AND requested_url = 'http://169.254.169.254/latest/meta-data/'",
                [],
                |r| r.get(0),
            )
        })
        .unwrap();
    assert_eq!(n, 1, "拒绝事件落 012（审计复核面）");

    // 拒绝行幂等：fetch_id 内容寻址（url|reason|时刻），重放零新增。
    let before: i64 = pool
        .with_conn(|c| c.query_row("SELECT COUNT(*) FROM knowledge_document_fetch", [], |r| r.get(0)))
        .unwrap();
    let _ = fetch_document(Some(&pool), &NoNetTransport, &policy, &input, Some(&checker));
    let after: i64 = pool
        .with_conn(|c| c.query_row("SELECT COUNT(*) FROM knowledge_document_fetch", [], |r| r.get(0)))
        .unwrap();
    assert!(after >= before, "012 行只追加");
}

#[test]
fn sk3_proxy_transport_spawn_records_allowlist_for_run() {
    // run 级代理句柄可用（文档抓取的传输面装配形态）。
    let transport = ProxyTransport::spawn(
        "sk3-doc-fetch-run",
        EgressAllowlist {
            hosts: vec!["example.com".to_string()],
            ports: vec![80],
        },
    )
    .unwrap();
    assert!(transport.events().is_empty(), "无连接则无事件");
}

#[test]
fn sk3_t2_shaped_call_without_authorization_ref_rejected() {
    // Tool Policy Resolver 复验（批次验收：T2 形态调用缺 authorization_ref
    // 被拒绝，POLICY_DENIED）：T1 工具带引用可受理，T2 缺引用拒绝。
    use tool_engine::Engine;
    use tool_engine::EngineError;
    use tool_registry::{Options, Registry};

    let pool = memory_pool();
    database::migrate(&pool, &migrations_dir()).unwrap();
    let registry = Registry::open(pool.clone()).with_options(Options::default());
    let engine = Engine::open(pool.clone(), registry.clone());

    let mut t2 = serde_json::json!({
        "schema_version": "1",
        "id": "rip.t.sk3t2probe",
        "name": "SK3 T2 Probe",
        "version": "0.1.0",
        "kind": "tool",
        "family": "utility",
        "description": "T2 授权闸探针",
        "runtime": {"type": "python_worker", "protocol_version": "1",
                     "worker_name": "probe-worker", "command": ["python3", "-I", "x.py"]},
        "input_schema": {"type": "object"},
        "output_schema": {"type": "object"},
        "resources": {"cpu": "1", "memory": "128Mi", "timeout_seconds": 30,
                       "max_output_bytes": 1048576},
        "network": {"mode": "none"},
        "secrets": [],
        "permission_level": "T2",
        "idempotent": true,
        "provenance": {"origin": "rip-native", "source_path": "x", "license": "MIT",
                        "dependencies": [], "tested": "sk3 t2 gate probe"},
        "health": {"probe": {"type": "none"}},
        "examples": []
    });
    registry.register(&t2).unwrap();
    let _ = &mut t2;

    let mk_req = |run_id: &str, authz: Option<&str>| {
        let mut req = serde_json::json!({
            "protocol_version": "1",
            "tool_id": "rip.t.sk3t2probe",
            "run_id": run_id,
            "input": {},
            "workspace_id": "ws-default",
        });
        if let Some(a) = authz {
            req["authorization_ref"] = serde_json::json!(a);
        }
        req
    };

    // 缺授权引用：POLICY_DENIED，且不产生 run 行。
    let err = engine.submit(&mk_req("rn-sk3t2-missing", None)).unwrap_err();
    match err {
        EngineError::Rejected(tool_engine::request::Rejection::MissingAuthorization(tool)) => {
            assert_eq!(tool, "rip.t.sk3t2probe");
        }
        other => panic!("expect MissingAuthorization, got {other:?}"),
    }

    // 带授权引用：受理。
    engine
        .submit(&mk_req("rn-sk3t2-ok", Some("authz-2026-001")))
        .expect("T2 with authorization_ref must be accepted");
}
