//! `wsw_runtime::fuzz::persist`：fuzz case → exchange(source='fuzzer') 落库
//! （WSW-4 D7；`persist_send_result` sender.rs:873 同型新函数，复用
//! CAS/blob/next_sequence_no 全链——不建第二事实源，红线 3）。
//!
//! 发送成功的 case 才落 exchange 行（SKIPPED_SCOPE/SKIPPED_BUDGET 不发送
//! 不落 exchange——D7）；listener_id 用常量 [`FUZZ_LISTENER_ID`] =
//! `"wsw-lis-fuzzer"`（rp-wsw send `"wsw-lis-repeater"` 常量先例
//! rp-wsw.rs:505）；`wsw_exchange.source` CHECK 已含 'fuzzer'
//! （025:54 词形在位，0 零改动）。

use crate::persist::{ExchangeRow, PersistStore};
use std::path::Path;
use std::sync::Mutex;
use wsw_message::serialize::OutboundRequest;
use wsw_message::types::ParseStatus;

/// fuzz exchange 行 listener_id 常量（D1）。
pub const FUZZ_LISTENER_ID: &str = "wsw-lis-fuzzer";

/// exchange 落库互斥（进程内）：next_sequence_no → insert 非原子读改序，
/// 并发 finalize 任务可撞 UNIQUE(listener_id, sequence_no)——persist_send_result
/// 的 repeater 面单发送无此竞态；fuzz 并发面以本互斥串行化落库段
/// （CAS + exchange 行同锁；落库段毫秒级，非发送路径）。
static EXCHANGE_PERSIST_LOCK: Mutex<()> = Mutex::new(());

/// fuzz case 落库产出引用（case 行回填面）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FuzzCaseExchangeRefs {
    /// exchange_id（case 行 exchange_id 列引用）。
    pub exchange_id: String,
    /// outbound bytes sha256（逐字节重发唯一依据列——W3-D6.3 同口径）。
    pub outbound_sha256: String,
}

/// fuzz case 发送成功 → exchange 行（source='fuzzer'）+ CAS 三面
/// （outbound/请求体/响应体；persist_send_result 同型）。
pub fn persist_fuzz_case_exchange(
    store: &PersistStore,
    artifacts_root: &Path,
    session_id: &str,
    request: &OutboundRequest,
    outcome: &crate::sender::SendOutcome,
) -> Result<FuzzCaseExchangeRefs, database::DbError> {
    let _guard = EXCHANGE_PERSIST_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    // 1) CAS 三面（非空才落；同 persist_send_result 口径）。
    let request_ref = (!request.body.is_empty())
        .then(|| {
            wsw_message::cas::store_body(
                artifacts_root,
                &request.body,
                request.body.len() as u64,
            )
            .map_err(|e| database::DbError::Validation(format!("request CAS store: {e}")))
        })
        .transpose()?;
    let outbound_ref = wsw_message::cas::store_body(
        artifacts_root,
        &outcome.outbound_bytes,
        outcome.outbound_bytes.len() as u64,
    )
    .map_err(|e| database::DbError::Validation(format!("outbound CAS store: {e}")))?;
    let response_ref = (!outcome.response_bytes.is_empty())
        .then(|| {
            wsw_message::cas::store_body(
                artifacts_root,
                &outcome.response_bytes,
                outcome.response_bytes.len() as u64,
            )
            .map_err(|e| database::DbError::Validation(format!("response CAS store: {e}")))
        })
        .transpose()?;

    // 2) exchange 行组装（source='fuzzer'；capture_fidelity=full——outbound
    //    bytes 即原始 wire）。
    let exchange_id = crate::listener::handler::new_exchange_id();
    let seq = store.next_sequence_no(FUZZ_LISTENER_ID)?;
    let req_json = request;
    let res_json = outcome.parsed.as_ref().and_then(|p| p.message.as_ref());
    let req_headers_json = serde_json::to_string(
        &req_json
            .headers
            .iter()
            .map(|h| serde_json::json!({"name": h.name, "value": h.value}))
            .collect::<Vec<_>>(),
    )
    .unwrap_or_else(|_| "[]".into());
    let res_headers_json = res_json
        .map(|m| {
            serde_json::to_string(
                &m.headers
                    .iter()
                    .map(|h| serde_json::json!({"name": h.name, "value": h.value}))
                    .collect::<Vec<_>>(),
            )
            .unwrap_or_else(|_| "[]".into())
        })
        .unwrap_or_else(|| "[]".into());
    // 目标投影（resolve_target 私有——fuzz 面镜像解析只用于行字段；
    // 发送闸权威面在 send_request，D1 单点不复制）。
    let target = fuzz_target_view(req_json);
    let started_at = crate::persist::now_iso();
    let completed_at = crate::persist::now_iso();
    let search = crate::listener::handler::build_search_text(
        target
            .as_ref()
            .map(|t| format!("{}:{}", t.host, t.port))
            .as_deref()
            .unwrap_or(""),
        target.as_ref().map(|t| t.path.as_str()).unwrap_or(""),
        &req_json
            .headers
            .iter()
            .map(|h| (h.name.as_str(), h.value.as_str()))
            .collect::<Vec<_>>(),
        &[crate::listener::handler::body_snippet(&req_json.body)],
    );
    let response_mime = res_json.and_then(|m| {
        m.headers
            .iter()
            .find(|h| h.name.eq_ignore_ascii_case("content-type"))
            .map(|h| h.value.clone())
    });
    let parse_status = match outcome.parsed.as_ref().map(|p| p.parse_status) {
        Some(ParseStatus::Ok) => "ok",
        Some(ParseStatus::Partial) => "partial",
        _ => "failed",
    };
    let row = ExchangeRow {
        exchange_id: exchange_id.clone(),
        session_id: session_id.to_string(),
        listener_id: FUZZ_LISTENER_ID.to_string(),
        sequence_no: seq,
        direction: "client_to_server",
        source: "fuzzer",
        protocol: "http",
        http_version: Some(req_json.version.as_str().to_string()),
        tls_version: None,
        alpn: None,
        method: Some(req_json.method.clone()),
        scheme: target.as_ref().map(|t| t.scheme.clone()),
        authority: target.as_ref().map(|t| format!("{}:{}", t.host, t.port)),
        path: target.as_ref().map(|t| t.path.clone()),
        response_status: outcome.response_status.map(|s| s as i64),
        request_headers_json: Some(req_headers_json),
        response_headers_json: Some(res_headers_json),
        request_body_sha256: (!req_json.body.is_empty())
            .then(|| wsw_message::cas::sha256_hex(&req_json.body)),
        request_body_uri: request_ref.as_ref().map(|r| r.uri.clone()),
        request_body_len: Some(req_json.body.len() as i64),
        request_mime: req_json
            .headers
            .iter()
            .find(|h| h.name.eq_ignore_ascii_case("content-type"))
            .map(|h| h.value.clone()),
        request_body_fidelity: Some("full"),
        response_body_sha256: (!outcome.response_bytes.is_empty())
            .then(|| wsw_message::cas::sha256_hex(&outcome.response_bytes)),
        response_body_uri: response_ref.as_ref().map(|r| r.uri.clone()),
        response_body_len: Some(outcome.response_bytes.len() as i64),
        response_mime,
        response_body_fidelity: Some("full"),
        started_at: Some(started_at),
        completed_at: Some(completed_at),
        duration_ms: Some(outcome.duration_ms),
        ttfb_ms: outcome.ttfb_ms,
        remote_ip: outcome.remote_ip.map(|ip| ip.to_string()),
        scope_decision: Some("in_scope"),
        edited: false,
        tags_json: "[]".to_string(),
        notes: None,
        trace_id: None,
        task_id: None,
        tool_run_id: None,
        artifact_id: None,
        evidence_id: None,
        capture_fidelity: "full",
        parse_status,
        error_code: outcome.error_code.clone(),
        search_text: Some(search),
    };
    store.insert_exchanges(std::slice::from_ref(&row))?;
    Ok(FuzzCaseExchangeRefs {
        exchange_id,
        outbound_sha256: outbound_ref.sha256,
    })
}

/// fuzz 目标视图（absolute/origin form 双形；仅行字段投影用）。
struct FuzzTargetView {
    scheme: String,
    host: String,
    port: u16,
    path: String,
}

fn fuzz_target_view(req: &OutboundRequest) -> Option<FuzzTargetView> {
    let default_port = |scheme: &str| if scheme == "https" { 443 } else { 80 };
    if req.target.contains("://") {
        let uri: hudsucker::hyper::Uri = req.target.parse().ok()?;
        let scheme = uri.scheme_str().unwrap_or("http").to_string();
        let host = uri.host()?.trim_matches(['[', ']']).to_string();
        let port = uri.port_u16().unwrap_or_else(|| default_port(&scheme));
        let path = if uri.path().is_empty() {
            "/".to_string()
        } else {
            format!(
                "{}{}",
                uri.path(),
                uri.query().map(|q| format!("?{q}")).unwrap_or_default()
            )
        };
        return Some(FuzzTargetView { scheme, host, port, path });
    }
    let host_header = req
        .headers
        .iter()
        .find(|h| h.name.eq_ignore_ascii_case("host"))
        .map(|h| h.value.clone())?;
    let (host, port) = split_authority(&host_header, default_port("http"));
    Some(FuzzTargetView {
        scheme: "http".to_string(),
        host,
        port,
        path: if req.target.is_empty() {
            "/".to_string()
        } else {
            req.target.clone()
        },
    })
}

/// host:port 拆分（无端口取 default；IPv6 括号形态）。
fn split_authority(authority: &str, default_port: u16) -> (String, u16) {
    if let Some(rest) = authority.strip_prefix('[') {
        if let Some(end) = rest.find(']') {
            let host = rest[..end].to_string();
            let port = rest[end + 1..]
                .strip_prefix(':')
                .and_then(|p| p.parse().ok())
                .unwrap_or(default_port);
            return (host, port);
        }
    }
    match authority.rsplit_once(':') {
        Some((h, p)) => (
            h.to_string(),
            p.parse().unwrap_or(default_port),
        ),
        None => (authority.to_string(), default_port),
    }
}

/// scope 预检目标（scheme/host/port/path；闸判定复用 scope::check 单点）。
pub struct ScopePrecheckTarget {
    pub scheme: String,
    pub host: String,
    pub port: u16,
    pub path: String,
}

/// 从模板请求提取 scope 预检目标（fuzz_target_view 同源；发送权威闸仍在
/// send_request——预检只决定 SKIPPED_SCOPE 分流，不替代闸，D1）。
pub fn scope_precheck_target(req: &OutboundRequest) -> Option<ScopePrecheckTarget> {
    fuzz_target_view(req).map(|t| ScopePrecheckTarget {
        scheme: t.scheme,
        host: t.host,
        port: t.port,
        path: t.path,
    })
}
