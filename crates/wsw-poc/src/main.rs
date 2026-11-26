//! WSW-2 POC：hudsucker 0.25.0 最小化配置 MITM 代理（Rust-First）。
//!
//! 验收面（docs/rip3/wsw/01-LIBRARY-EVALUATION.md §6.1）：
//! - `python3 scripts/local_range.py --port 8443` 起靶场（纯标准库，绑 127.0.0.1）
//! - 本代理只监听 127.0.0.1（缺省 127.0.0.1:3128，非 loopback 拒绝绑定）
//! - `curl -x http://127.0.0.1:3128 http://127.0.0.1:8443/p59c-marker`
//!   → marker 透传 + POC 日志抓到该请求 = PASS
//! - HTTPS CONNECT：运行时生成 rcgen 根 CA（PEM 落盘供 curl --cacert），
//!   动态证书 MITM 解密 + 响应注入 `X-WSW-POC: mitm-ok` 证明可改写面
//!
//! 纪律：不接 UI、不碰数据库；POC 日志走 stdout（运行时重定向留证）。

use http_body_util::BodyExt;
use hudsucker::{
    certificate_authority::RcgenAuthority,
    hyper::{
        body::Bytes,
        header::{HeaderName, HeaderValue},
        Request, Response,
    },
    rcgen::{
        BasicConstraints, CertificateParams, DistinguishedName, DnType, IsCa, Issuer, KeyPair,
        KeyUsagePurpose,
    },
    rustls::{
        crypto::aws_lc_rs,
        pki_types::{pem::PemObject, CertificateDer},
        RootCertStore,
    },
    tokio_tungstenite::tungstenite::Message,
    Body, HttpContext, HttpHandler, Proxy, RequestOrResponse, WebSocketContext, WebSocketHandler,
};
use std::{
    error::Error as StdError,
    net::{IpAddr, SocketAddr},
    path::PathBuf,
    sync::Arc,
};

/// POC 日志前缀（evidence 消费面：grep 'WSW-POC' poc.log）。
const TAG: &str = "[WSW-POC]";
/// 请求/响应体预览上限（POC 只采样，不承诺完整缓冲语义）。
const PREVIEW_CAP: usize = 200;

fn preview(bytes: &[u8]) -> std::borrow::Cow<'_, str> {
    String::from_utf8_lossy(&bytes[..bytes.len().min(PREVIEW_CAP)])
}

/// HTTP 处理器：透传 + 日志 + 响应注入头。
///
/// POC 以聚合缓冲换取日志可读性；生产面（WSW-3+）改流式 tee + 尺寸上限
/// （Body::from_stream 保留 hyper Incoming 流式语义，见评估文档 §2.1）。
#[derive(Clone, Copy, Debug, Default)]
struct PocHandler;

impl HttpHandler for PocHandler {
    async fn handle_request(&mut self, ctx: &HttpContext, req: Request<Body>) -> RequestOrResponse {
        let (parts, body) = req.into_parts();
        let (bytes, req) = match body.collect().await {
            Ok(collected) => {
                let bytes: Bytes = collected.to_bytes();
                let req = Request::from_parts(parts, Body::from(bytes.clone()));
                (bytes, req)
            }
            Err(e) => {
                println!("{TAG}[ERR] request body collect failed: {e}");
                (Bytes::new(), Request::from_parts(parts, Body::empty()))
            }
        };
        println!(
            "{TAG}[REQ] client={} method={} uri={} host={} body_len={} body_preview={:?}",
            ctx.client_addr,
            req.method(),
            req.uri(),
            req.headers()
                .get("host")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("-"),
            bytes.len(),
            preview(&bytes),
        );
        req.into()
    }

    async fn handle_response(&mut self, _ctx: &HttpContext, res: Response<Body>) -> Response<Body> {
        let (mut parts, body) = res.into_parts();
        let collected = match body.collect().await {
            Ok(collected) => collected,
            Err(e) => {
                println!("{TAG}[ERR] response body collect failed: {e}");
                return Response::from_parts(parts, Body::empty());
            }
        };
        let bytes: Bytes = collected.to_bytes();
        println!(
            "{TAG}[RES] status={} body_len={} body_preview={:?}",
            parts.status.as_u16(),
            bytes.len(),
            preview(&bytes),
        );
        // 注入头 = MITM 解密 + 可改写面的直接证据（直连靶场时不存在）。
        parts.headers.insert(
            HeaderName::from_static("x-wsw-poc"),
            HeaderValue::from_static("mitm-ok"),
        );
        println!("{TAG}[RES] injected header X-WSW-POC: mitm-ok");
        Response::from_parts(parts, Body::from(bytes))
    }

    async fn handle_error(
        &mut self,
        _ctx: &HttpContext,
        err: hudsucker::hyper_util::client::legacy::Error,
    ) -> Response<Body> {
        // 错误链全量打印（POC 定位面：上游连接/TLS 校验失败原因可见）。
        let mut chain = String::new();
        let mut src: Option<&dyn StdError> = Some(&err);
        while let Some(e) = src {
            chain.push_str(&format!("\n  <- {e}"));
            src = e.source();
        }
        println!("{TAG}[ERR] forward failed: {err}{chain}");
        Response::builder()
            .status(hudsucker::hyper::StatusCode::BAD_GATEWAY)
            .body(Body::empty())
            .expect("failed to build 502")
    }
}

/// WebSocket 帧级处理器：透传 + 日志（能力面证明，本 POC 无 WS 端点）。
impl WebSocketHandler for PocHandler {
    async fn handle_message(&mut self, ctx: &WebSocketContext, msg: Message) -> Option<Message> {
        let endpoint = match ctx {
            WebSocketContext::ClientToServer { dst, .. } => dst.to_string(),
            WebSocketContext::ServerToClient { src, .. } => src.to_string(),
        };
        println!("{TAG}[WS] endpoint={endpoint} message={msg:?}");
        Some(msg)
    }
}

fn parse_args() -> (SocketAddr, PathBuf, Vec<PathBuf>) {
    let mut listen = SocketAddr::from(([127, 0, 0, 1], 3128));
    let mut ca_out = PathBuf::from("/tmp/wsw-poc-ca.pem");
    let mut upstream_cas = Vec::new();
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--listen" => {
                listen = args
                    .next()
                    .expect("--listen needs a value")
                    .parse()
                    .expect("--listen needs host:port")
            }
            "--ca-out" => ca_out = args.next().expect("--ca-out needs a value").into(),
            "--upstream-ca" => {
                upstream_cas.push(args.next().expect("--upstream-ca needs a value").into())
            }
            other => {
                eprintln!(
                    "{TAG} unknown arg {other} (usage: [--listen 127.0.0.1:3128] [--ca-out /tmp/wsw-poc-ca.pem] [--upstream-ca extra-root.pem]...)"
                );
                std::process::exit(2);
            }
        }
    }
    (listen, ca_out, upstream_cas)
}

/// 上游 TLS 根库：webpki 公共根 + `--upstream-ca` 追加面（本地回环 TLS 夹具
/// 用自签证书，标准 webpki 面会 502，见 POC notes）。
fn build_upstream_roots(extra: &[PathBuf]) -> Result<RootCertStore, Box<dyn StdError>> {
    let mut roots = RootCertStore::empty();
    roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    let mut n = 0;
    for path in extra {
        for cert in CertificateDer::pem_file_iter(path)? {
            roots.add(cert?)?;
            n += 1;
        }
    }
    println!(
        "{TAG}[ROOTS] upstream roots = webpki({}) + extra({n} from {} --upstream-ca)",
        roots.len() - n,
        extra.len()
    );
    Ok(roots)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn StdError>> {
    let (listen, ca_out, upstream_cas) = parse_args();

    // 硬约束：代理只听 loopback（127.0.0.1 / ::1），拒绝其它绑定面。
    let loopback = match listen.ip() {
        IpAddr::V4(v4) => v4.is_loopback(),
        IpAddr::V6(v6) => v6.is_loopback(),
    };
    if !loopback {
        eprintln!("{TAG} refuse to bind non-loopback address: {listen}");
        std::process::exit(2);
    }

    // 运行时生成根 CA（rcgen 0.14.10 ring 后端），PEM 落盘供 curl --cacert。
    let ca_key = KeyPair::generate()?;
    let mut ca_params = CertificateParams::new(Vec::<String>::new())?;
    let mut dn = DistinguishedName::new();
    dn.push(DnType::CommonName, "WSW-POC-ROOT-CA");
    ca_params.distinguished_name = dn;
    ca_params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    ca_params.key_usages = vec![
        KeyUsagePurpose::DigitalSignature,
        KeyUsagePurpose::KeyCertSign,
    ];
    let ca_cert = ca_params.self_signed(&ca_key)?;
    std::fs::write(&ca_out, ca_cert.pem())?;
    println!(
        "{TAG}[CA] root CA written to {} (CN=WSW-POC-ROOT-CA)",
        ca_out.display()
    );

    let issuer = Issuer::new(ca_params, ca_key);
    let ca = RcgenAuthority::new(issuer, 1_000, aws_lc_rs::default_provider());

    // 上游 connector：webpki 根 + 追加信任面（与 hudsucker with_rustls_connector
    // 同构，仅根库可扩展；https_or_http 支持明文 HTTP 靶场面）。
    let upstream_roots = build_upstream_roots(&upstream_cas)?;
    let client_config = hudsucker::rustls::ClientConfig::builder_with_provider(Arc::new(
        aws_lc_rs::default_provider(),
    ))
    .with_safe_default_protocol_versions()?
    .with_root_certificates(upstream_roots)
    .with_no_client_auth();
    let connector = hyper_rustls::HttpsConnectorBuilder::new()
        .with_tls_config(client_config)
        .https_or_http()
        .enable_http1()
        .enable_http2()
        .build();

    let proxy = Proxy::builder()
        .with_addr(listen)
        .with_ca(ca)
        .with_http_connector(connector)
        .with_http_handler(PocHandler)
        .with_websocket_handler(PocHandler)
        .build()?;
    println!(
        "{TAG}[BOOT] listening on {listen} (http pass-through + https CONNECT MITM, dynamic cert via rcgen CA)"
    );

    proxy.start().await?;
    Ok(())
}
