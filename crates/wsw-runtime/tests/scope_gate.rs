//! Scope 闸集成测试（WSW-2-TASK §四.2 负例矩阵闸序面；闸只在此 Rust 后端，
//! PLAN :523）。正负例逐条断言（禁恒真）：include 命中 / exclude 优先 /
//! 端口不符 / default deny / private 拒与显式 allow_private 过留痕 /
//! 云元数据 allow_private 也拒 / mapped 归一 / max_redirects 帽 /
//! DNS rebinding（解析 IP 过闸拒 + remote_ip）/ 规则 JSON 来源面。
//!
//! 文件尾部两例为 DNS rebinding **集成级**口径（修订批落）：解析桩注入
//! `AuditedConnector::call` 拒 → `ConnectorError::ScopeRejected` 携带
//! remote_ip → `respond_to_scope_reject`（`handle_error` 同一实现）403 +
//! `x-wsw-scope-decision` 头——查点 C glue 全链真实执行。

use std::net::{IpAddr, Ipv4Addr};
use wsw_runtime::scope::{
    self, IpRejection, ScopeDecision, ScopeRejection, ScopeRules, ScopeTarget,
};

fn target<'a>(scheme: &'a str, host: &'a str, port: u16, path: &'a str) -> ScopeTarget<'a> {
    ScopeTarget {
        scheme,
        host,
        port,
        path,
    }
}

fn rules_with(include: &[&str]) -> ScopeRules {
    ScopeRules {
        include: include.iter().map(|s| s.to_string()).collect(),
        ..Default::default()
    }
}

#[test]
fn matrix_include_hit_passes_with_trace() {
    let mut rules = rules_with(&["127.0.0.1", "local-range.test"]);
    rules.allow_private_targets = true;
    let d = scope::check(&rules, &target("http", "127.0.0.1", 8443, "/p59c-marker"));
    let ScopeDecision::Allowed { traces } = d else {
        panic!("include hit must pass: {d:?}");
    };
    let kinds: Vec<&str> = traces.iter().map(|t| t.kind).collect();
    assert!(
        kinds.contains(&"included"),
        "include trace line: {traces:?}"
    );
    assert!(
        kinds.contains(&"private_allowed"),
        "private trace line: {traces:?}"
    );
    // IP 字面量 + allow_private → audit_resolved 同样放行 + 留痕。
    let resolved = vec![IpAddr::V4(Ipv4Addr::LOCALHOST)];
    let traces = scope::audit_resolved(&rules, &resolved).expect("connect audit pass");
    assert!(traces.iter().any(|t| t.kind == "private_allowed"));
}

#[test]
fn matrix_no_rules_default_deny() {
    let rules = ScopeRules::default();
    match scope::check(&rules, &target("http", "example.com", 80, "/")) {
        ScopeDecision::Blocked { reason } => {
            assert_eq!(reason.code(), "no_rules_default_deny");
        }
        other => panic!("default deny required, got {other:?}"),
    }
}

#[test]
fn matrix_exclude_wins() {
    let mut rules = rules_with(&["*.example"]);
    rules.exclude = vec!["api.example".to_string()];
    match scope::check(&rules, &target("https", "api.example", 443, "/v1")) {
        ScopeDecision::Blocked { reason } => assert_eq!(reason.code(), "host_excluded"),
        other => panic!("exclude must win, got {other:?}"),
    }
    assert!(matches!(
        scope::check(&rules, &target("https", "web.example", 443, "/")),
        ScopeDecision::Allowed { .. }
    ));
}

#[test]
fn matrix_port_mismatch_denied() {
    let mut rules = rules_with(&["127.0.0.1"]);
    rules.allowed_ports = vec![8443];
    rules.allow_private_targets = true;
    match scope::check(&rules, &target("http", "127.0.0.1", 9000, "/")) {
        ScopeDecision::Blocked { reason } => assert_eq!(reason.code(), "port_denied"),
        other => panic!("port gate required, got {other:?}"),
    }
    // 命中端口放行。
    assert!(matches!(
        scope::check(&rules, &target("http", "127.0.0.1", 8443, "/")),
        ScopeDecision::Allowed { .. }
    ));
}

#[test]
fn matrix_private_denied_without_flag_then_allowed_with_trace() {
    let rules = rules_with(&["10.0.0.5"]);
    match scope::check(&rules, &target("http", "10.0.0.5", 80, "/")) {
        ScopeDecision::Blocked {
            reason: ScopeRejection::PrivateTarget { ip, reason },
        } => {
            assert_eq!(ip, IpAddr::V4(Ipv4Addr::new(10, 0, 0, 5)));
            assert_eq!(reason, "rfc1918");
        }
        other => panic!("private must be denied without allow flag, got {other:?}"),
    }
    let mut allowed = rules_with(&["10.0.0.5"]);
    allowed.allow_private_targets = true;
    let d = scope::check(&allowed, &target("http", "10.0.0.5", 80, "/"));
    let ScopeDecision::Allowed { traces } = d else {
        panic!("allow_private must pass: {d:?}");
    };
    assert!(traces.iter().any(|t| t.kind == "private_allowed"));
}

#[test]
fn matrix_metadata_deny_never_allow() {
    // allow_private=true + include 命中 → 仍拒（reason=metadata_endpoint）。
    let mut rules = rules_with(&[
        "169.254.169.254",
        "metadata.google.internal",
        "169.254.170.2",
    ]);
    rules.allow_private_targets = true;
    for host in [
        "169.254.169.254",
        "metadata.google.internal",
        "169.254.170.2",
    ] {
        match scope::check(&rules, &target("http", host, 80, "/latest/meta-data/")) {
            ScopeDecision::Blocked { reason } => assert_eq!(reason.code(), "metadata_endpoint"),
            other => panic!("{host} deny-never-allow violated: {other:?}"),
        }
    }
    // mapped 形态同样拒。
    assert_eq!(
        scope::check(&rules, &target("http", "::ffff:169.254.169.254", 80, "/"))
            .state_blocked_code()
            .unwrap_or_default(),
        "metadata_endpoint"
    );
}

trait BlockedCode {
    fn state_blocked_code(&self) -> Option<String>;
}

impl BlockedCode for ScopeDecision {
    fn state_blocked_code(&self) -> Option<String> {
        match self {
            ScopeDecision::Blocked { reason } => Some(reason.code().to_string()),
            _ => None,
        }
    }
}

#[test]
fn matrix_mapped_normalized_to_v4() {
    let rules = rules_with(&["::ffff:10.9.8.7"]);
    match scope::check(&rules, &target("http", "::ffff:10.9.8.7", 80, "/")) {
        ScopeDecision::Blocked {
            reason: ScopeRejection::PrivateTarget { ip, reason },
        } => {
            // mapped 归一后按 V4 私网判定（非 v6 面）。
            assert_eq!(ip, IpAddr::V4(Ipv4Addr::new(10, 9, 8, 7)));
            assert_eq!(reason, "rfc1918");
        }
        other => panic!("mapped must be judged as v4 private, got {other:?}"),
    }
}

#[test]
fn matrix_redirect_cap_and_out_of_range_hop() {
    let mut rules = rules_with(&["127.0.0.1"]);
    rules.allow_private_targets = true;
    rules.max_redirects = 3;
    assert!(matches!(
        scope::check_redirect_hop(&rules, &target("http", "127.0.0.1", 8443, "/"), 3),
        ScopeDecision::Allowed { .. }
    ));
    match scope::check_redirect_hop(&rules, &target("http", "127.0.0.1", 8443, "/"), 4) {
        ScopeDecision::Blocked {
            reason: ScopeRejection::RedirectLimit { hop, cap },
        } => {
            assert_eq!((hop, cap), (4, 3));
        }
        other => panic!("redirect cap required, got {other:?}"),
    }
    // 越界重定向第二跳（Location 指向 include 外 host）按同闸序拒。
    match scope::check_redirect_hop(
        &rules,
        &target("http", "out-of-range.example", 80, "/nope"),
        1,
    ) {
        ScopeDecision::Blocked { reason } => assert_eq!(reason.code(), "host_not_included"),
        other => panic!("redirect hop gate required, got {other:?}"),
    }
}

#[test]
fn matrix_dns_rebinding_rejected_with_remote_ip() {
    // 域名串过闸（include 命中）但解析 IP 落私网 → 查点 C 拒 + remote_ip。
    let rules = rules_with(&["rebinding.test"]);
    assert!(matches!(
        scope::check(&rules, &target("http", "rebinding.test", 80, "/")),
        ScopeDecision::Allowed { .. }
    ));
    let resolved: Vec<IpAddr> = vec![
        "93.184.216.34".parse().unwrap(),
        "10.0.0.5".parse().unwrap(),
        "::ffff:192.168.1.9".parse().unwrap(),
    ];
    let err: Vec<IpRejection> = scope::audit_resolved(&rules, &resolved).expect_err("must reject");
    assert_eq!(err.len(), 2, "all private resolved ips rejected: {err:?}");
    assert_eq!(err[0].ip, IpAddr::V4(Ipv4Addr::new(10, 0, 0, 5)));
    assert_eq!(err[0].reason, "rfc1918");
    // mapped 归一后 v4 私网（192.168.1.9 = rfc1918）。
    assert_eq!(err[1].ip, IpAddr::V4(Ipv4Addr::new(192, 168, 1, 9)));
}

#[test]
fn matrix_dns_rebinding_metadata_ip_even_with_allow() {
    let mut rules = rules_with(&["metadata.test"]);
    rules.allow_private_targets = true;
    let resolved = vec!["169.254.169.254".parse::<IpAddr>().unwrap()];
    let err = scope::audit_resolved(&rules, &resolved).expect_err("deny-never-allow");
    assert_eq!(err[0].reason, "metadata_endpoint");
}

#[test]
fn rules_json_session_source_shape() {
    // wsw_session.scope_rules_json 源面（blocked 策略默认值不属规则 JSON）。
    let json = r#"{"include":["127.0.0.1","*.range.test"],"exclude":["bad.range.test"],"allowed_ports":[8443,9443],"allow_private_targets":true,"max_redirects":4}"#;
    let rules = ScopeRules::from_json(json).expect("parse");
    assert!(matches!(
        scope::check(&rules, &target("http", "127.0.0.1", 8443, "/p59c-marker")),
        ScopeDecision::Allowed { .. }
    ));
    assert_eq!(
        scope::check(&rules, &target("http", "bad.range.test", 8443, "/")).state_blocked_code(),
        Some("host_excluded".to_string())
    );
    assert!(
        matches!(
            scope::check(&rules, &target("http", "other.range.test", 8443, "/")),
            ScopeDecision::Allowed { .. }
        ),
        "wildcard matches subdomains of range.test"
    );
    assert_eq!(
        scope::check(&rules, &target("http", "outside.example", 8443, "/")).state_blocked_code(),
        Some("host_not_included".to_string())
    );
    // 反序列化非法 JSON 拒（fail-closed：宁可拒不可空规则放行）。
    assert!(ScopeRules::from_json("{not json").is_err());
}

#[test]
fn invalid_host_and_scheme_denied() {
    let rules = rules_with(&["127.0.0.1"]);
    assert_eq!(
        scope::check(&rules, &target("ftp", "127.0.0.1", 21, "/")).state_blocked_code(),
        Some("scheme_denied".to_string())
    );
    assert_eq!(
        scope::check(&rules, &target("http", "bad host\r\n", 80, "/")).state_blocked_code(),
        Some("invalid_host".to_string())
    );
}

// ---------------------------------------------------------------------------
// DNS rebinding 集成级负例（WSW-2-TASK §四.2 行集成级口径；WSW-2 修订批落）：
// 查点 C 端到端 glue = AuditedConnector::call（解析桩注入，免改 /etc/hosts）
// 拒 → ConnectorError::ScopeRejected 携带 remote_ip → respond_to_scope_reject
// （handle_error 同一实现）403 + x-wsw-scope-decision 头。单测级面
// matrix_dns_rebinding_* 只覆盖 audit_resolved 本体，不覆盖此 glue。
// ---------------------------------------------------------------------------

use std::sync::Arc;
use tower_service::Service as _;
use wsw_runtime::listener::handler::{respond_to_scope_reject, ListenerShared, RuntimeOptions};
use wsw_runtime::listener::{AuditedConnector, ConnectorError, DnsResolver};

/// 共享运行面构造（blocked 策略默认；无持久面，不触网络）。
fn shared_for(rules: ScopeRules) -> Arc<ListenerShared> {
    let options = RuntimeOptions {
        scope_rules: rules,
        ..Default::default()
    };
    Arc::new(ListenerShared::new(options, None))
}

/// 解析桩：域名解析指向私网 IP（DNS rebinding 面；免改 /etc/hosts）。
struct RebindResolver;

impl DnsResolver for RebindResolver {
    fn resolve(&self, _host: &str, _port: u16) -> Result<Vec<std::net::IpAddr>, ConnectorError> {
        Ok(vec![
            "93.184.216.34".parse().unwrap(),
            "10.0.0.5".parse().unwrap(),
        ])
    }
}

#[tokio::test]
async fn connector_call_rejects_resolved_private_ip_carrying_remote_ip() {
    let shared = shared_for(rules_with(&["rebinding.test"]));
    let mut connector = AuditedConnector::with_resolver(&shared, Arc::new(RebindResolver));
    let uri: hudsucker::hyper::Uri = "http://rebinding.test:80/".parse().unwrap();
    let err = match connector.call(uri).await {
        Ok(_) => panic!("resolved private ip must be rejected at connector (DNS rebinding)"),
        Err(e) => e,
    };
    let ConnectorError::ScopeRejected(record) = err else {
        panic!("expected ScopeRejected, got {err:?}");
    };
    assert_eq!(record.host, "rebinding.test");
    // remote_ip 落记录（域名串过闸 ≠ IP 过闸，PLAN :525）。
    assert_eq!(record.ip, IpAddr::V4(Ipv4Addr::new(10, 0, 0, 5)));
    assert_eq!(record.reason, "rfc1918");
    assert!(
        shared
            .counters
            .scope_blocked_total
            .load(std::sync::atomic::Ordering::Relaxed)
            >= 1,
        "connector rejection must bump visible counter"
    );
}

/// connector 失败包装链（生产形状 = hudsucker→hyper-util 把 connector 错误
/// 挂为 source 链；legacy::Error 无公共构造器，此处以同链形包装直喂同一
/// 实现函数 respond_to_scope_reject = handle_error 的查点 C 决策路径）。
#[derive(Debug)]
struct ConnectorFailure(ConnectorError);

impl std::fmt::Display for ConnectorFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "connector failure: {}", self.0)
    }
}

impl std::error::Error for ConnectorFailure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.0)
    }
}

#[tokio::test]
async fn connector_scope_reject_maps_to_403_with_decision_header() {
    let shared = shared_for(rules_with(&["rebinding.test"]));
    let err = ConnectorFailure(ConnectorError::ScopeRejected(
        wsw_runtime::listener::handler::ScopeRejectRecord {
            host: "rebinding.test".to_string(),
            ip: IpAddr::V4(Ipv4Addr::new(10, 0, 0, 5)),
            reason: "rfc1918".to_string(),
        },
    ));
    let resp = respond_to_scope_reject(&shared, &err)
        .expect("scope reject in error chain must map to 403 response");
    assert_eq!(resp.status(), hudsucker::hyper::StatusCode::FORBIDDEN);
    assert_eq!(
        resp.headers()
            .get("x-wsw-scope-decision")
            .and_then(|v| v.to_str().ok()),
        Some("out_of_scope_blocked")
    );
    assert_eq!(
        resp.headers()
            .get("x-wsw-reason")
            .and_then(|v| v.to_str().ok()),
        Some("rfc1918")
    );
    assert_eq!(
        shared
            .counters
            .scope_blocked_total
            .load(std::sync::atomic::Ordering::Relaxed),
        1,
        "handle_error-path rejection counted exactly once"
    );
    // 非 scope 面错误链（纯 IO）不走本函数。
    let io_err = ConnectorFailure(ConnectorError::Io(std::io::Error::new(
        std::io::ErrorKind::ConnectionRefused,
        "upstream refused",
    )));
    assert!(
        respond_to_scope_reject(&shared, &io_err).is_none(),
        "plain io error must not be treated as scope reject"
    );
}
