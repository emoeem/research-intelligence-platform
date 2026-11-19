//! MITM loopback e2e（WSW-2-TASK §三 S6 / §四.2 负例矩阵 / §四.4 W0-b）。
//!
//! 夹具全部本机 loopback（禁外联）：明文 marker 服务 + 自签 TLS 端点 +
//! h2-only 上游夹具 + 真实 curl/openssl 子进程（loopback 目标）。
//! 纪律：`--test-threads=1` + tokio::time::timeout 保护（容器派发并发挂死
//! 先例）；curl 缺失时如实 skip——三处 curl 子进程用例打 `[SKIP]` 标记后
//! 空跑返回（不冒充 PASS；无 curl 环境实拍矩阵缩水为 0 断言，环境前提 =
//! 本机有 curl，见批次报告 E 节测试账目注记）。

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use hudsucker::hyper::{Request, Response};
use hudsucker::rcgen::{
    BasicConstraints, CertificateParams, DistinguishedName, DnType, ExtendedKeyUsagePurpose, IsCa,
    Issuer, KeyPair, KeyUsagePurpose, SanType,
};
use hudsucker::rustls::crypto::aws_lc_rs;
use hudsucker::rustls::pki_types::{
    pem::PemObject, CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer,
};
use hudsucker::rustls::ServerConfig;
use tokio::net::TcpListener;
use tokio_rustls::TlsAcceptor;
use wsw_runtime::authority::WswAuthority;
use wsw_runtime::listener::handler::InterceptMatcher;
use wsw_runtime::listener::{ListenerError, RuntimeOptions, WswListener};
use wsw_runtime::scope::ScopeRules;

const MARKER: &str = "RIP3-P59C-LOCAL-RANGE-MARKER";
const H2_ALPN_RECORDED: &str = "mitm_loopback_h2_alpn";

fn temp_root(tag: &str) -> PathBuf {
    static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("wsw2-mitm-{tag}-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp root");
    dir
}

fn have_curl() -> bool {
    std::process::Command::new("curl")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// curl 缓跳：[SKIP] 标记落 stderr（--nocapture 可见；无 curl 环境该用例
/// 0 断言空跑，测试账目须注明环境前提——修订批 F7 口径）。
fn skip_curl_missing(test: &str) {
    eprintln!(
        "[SKIP] curl missing — {test} not executed (requires curl; \
         matrix runs on curl-present host only, see WSW-2-BATCH-REPORT.md §E)"
    );
}

/// 空闲 loopback 端口（bind :0 → 读实际端口 → 释放；单线程测试无竞争窗口）。
fn free_port() -> u16 {
    let l = std::net::TcpListener::bind("127.0.0.1:0").expect("bind :0");
    let port = l.local_addr().expect("local addr").port();
    drop(l);
    port
}

async fn serve_plain_http(listener: TcpListener, routes: Arc<MarkerRoutes>) {
    let builder = hudsucker::hyper_util::server::conn::auto::Builder::new(
        hudsucker::hyper_util::rt::TokioExecutor::new(),
    );
    let service = hudsucker::hyper::service::service_fn(
        move |req: Request<hudsucker::hyper::body::Incoming>| {
            let _routes = Arc::clone(&routes);
            async move {
                let path = req.uri().path().to_string();
                let (status, ctype, body): (u16, &str, Vec<u8>) = if path == "/p59c-marker" {
                    (200, "text/plain; charset=utf-8", MARKER.as_bytes().to_vec())
                } else if path == "/redirect-out-of-scope" {
                    (302, "text/plain; charset=utf-8", b"see other".to_vec())
                } else if path == "/" {
                    (
                        200,
                        "text/html; charset=utf-8",
                        b"<h1>RIP3 local range</h1>".to_vec(),
                    )
                } else {
                    (404, "text/plain; charset=utf-8", b"not found".to_vec())
                };
                let mut resp = Response::builder()
                    .status(status)
                    .header("content-type", ctype);
                if path == "/redirect-out-of-scope" {
                    // 越界重定向目标：scope 外 host（第二跳经代理复走查点 A 拒）。
                    resp = resp.header("location", "http://out-of-range.example.invalid:80/nope");
                }
                Ok::<_, std::convert::Infallible>(
                    resp.body(hudsucker::Body::from(body)).expect("resp"),
                )
            }
        },
    );
    loop {
        let Ok((stream, _)) = listener.accept().await else {
            continue;
        };
        let builder = builder.clone();
        let service = service.clone();
        tokio::spawn(async move {
            let _ = builder
                .serve_connection_with_upgrades(
                    hudsucker::hyper_util::rt::TokioIo::new(stream),
                    service,
                )
                .await;
        });
    }
}

struct MarkerRoutes;

/// 明文 marker 靶场（local_range.py 同语义夹具；loopback only）。
async fn spawn_marker_server() -> (u16, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("marker bind");
    let port = listener.local_addr().unwrap().port();
    let handle = tokio::spawn(serve_plain_http(listener, Arc::new(MarkerRoutes)));
    (port, handle)
}

/// 夹具 TLS 证书：CA（CA:TRUE+KeyCertSign）签 CA:FALSE+ServerAuth 叶
///（SAN=IP 127.0.0.1 + DnsName localhost）——ADR-0004 差距 (g) 教训面。
fn fixture_tls_files(root: &Path) -> (PathBuf, PathBuf, PathBuf) {
    let ca_key = KeyPair::generate().expect("ca key");
    let mut ca_params = CertificateParams::new(Vec::<String>::new()).expect("ca params");
    let mut dn = DistinguishedName::new();
    dn.push(DnType::CommonName, "WSW2-FIXTURE-CA");
    ca_params.distinguished_name = dn;
    ca_params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    ca_params.key_usages = vec![
        KeyUsagePurpose::DigitalSignature,
        KeyUsagePurpose::KeyCertSign,
    ];
    let ca_cert = ca_params.self_signed(&ca_key).expect("ca cert");

    let server_key = KeyPair::generate().expect("server key");
    let mut leaf_params = CertificateParams::new(Vec::<String>::new()).expect("leaf params");
    let mut leaf_dn = DistinguishedName::new();
    leaf_dn.push(DnType::CommonName, "127.0.0.1");
    leaf_params.distinguished_name = leaf_dn;
    leaf_params.subject_alt_names = vec![
        SanType::IpAddress(std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST)),
        SanType::DnsName(hudsucker::rcgen::string::Ia5String::try_from("localhost").expect("ia5")),
    ];
    leaf_params.key_usages = vec![KeyUsagePurpose::DigitalSignature];
    leaf_params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ServerAuth];
    leaf_params.use_authority_key_identifier_extension = true;
    let leaf = leaf_params
        .signed_by(&server_key, &Issuer::new(ca_params, ca_key))
        .expect("leaf sign");

    let ca_pem = root.join("fixture-ca.pem");
    std::fs::write(&ca_pem, ca_cert.pem()).expect("ca pem");
    let cert_pem = root.join("fixture-server.pem");
    std::fs::write(&cert_pem, leaf.pem()).expect("server pem");
    let key_pem = root.join("fixture-server.key");
    std::fs::write(&key_pem, server_key.serialize_pem()).expect("server key pem");
    (ca_pem, cert_pem, key_pem)
}

async fn tls_acceptor(cert_pem: &Path, key_pem: &Path, alpn: Option<&[u8]>) -> TlsAcceptor {
    let cert = CertificateDer::from_pem_file(cert_pem).expect("cert der");
    let key = PrivateKeyDer::from(PrivatePkcs8KeyDer::from_pem_file(key_pem).expect("key der"));
    let mut cfg = ServerConfig::builder_with_provider(Arc::new(aws_lc_rs::default_provider()))
        .with_safe_default_protocol_versions()
        .expect("versions")
        .with_no_client_auth()
        .with_single_cert(vec![cert], key)
        .expect("server config");
    cfg.alpn_protocols = alpn.map(|a| vec![a.to_vec()]).unwrap_or_default();
    TlsAcceptor::from(Arc::new(cfg))
}

/// TLS 端点（可选 h2-only：ALPN 只广告 h2，服务端用 hyper http2 builder）。
async fn spawn_tls_server(
    alpn_h2_only: bool,
    alpn_record: Arc<std::sync::Mutex<Option<String>>>,
    ca_pem: &Path,
    cert_pem: &Path,
    key_pem: &Path,
) -> (u16, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("tls bind");
    let port = listener.local_addr().unwrap().port();
    let acceptor = if alpn_h2_only {
        tls_acceptor(cert_pem, key_pem, Some(b"h2")).await
    } else {
        tls_acceptor(cert_pem, key_pem, None).await
    };
    let service = hudsucker::hyper::service::service_fn(
        |_req: Request<hudsucker::hyper::body::Incoming>| async move {
            Ok::<_, std::convert::Infallible>(
                Response::builder()
                    .status(200)
                    .header("content-type", "text/plain; charset=utf-8")
                    .body(hudsucker::Body::from(MARKER.as_bytes().to_vec()))
                    .expect("resp"),
            )
        },
    );
    let handle = tokio::spawn(async move {
        loop {
            let Ok((stream, _)) = listener.accept().await else {
                continue;
            };
            let acceptor = acceptor.clone();
            let record = Arc::clone(&alpn_record);
            tokio::spawn(async move {
                let Ok(tls) = acceptor.accept(stream).await else {
                    return;
                };
                if let Some(alpn) = tls.get_ref().1.alpn_protocol() {
                    *record.lock().expect("alpn mutex") =
                        Some(String::from_utf8_lossy(alpn).to_string());
                }
                let io = hudsucker::hyper_util::rt::TokioIo::new(tls);
                if alpn_h2_only {
                    let builder = hudsucker::hyper::server::conn::http2::Builder::new(
                        hudsucker::hyper_util::rt::TokioExecutor::new(),
                    );
                    let _ = builder.serve_connection(io, service).await;
                } else {
                    let builder = hudsucker::hyper_util::server::conn::auto::Builder::new(
                        hudsucker::hyper_util::rt::TokioExecutor::new(),
                    );
                    let _ = builder.serve_connection_with_upgrades(io, service).await;
                }
            });
        }
    });
    let _ = ca_pem;
    (port, handle)
}

async fn start_listener(
    tag: &str,
    rules: ScopeRules,
    db: Option<&Path>,
    artifacts: &Path,
    upstream_cas: &[PathBuf],
    intercept: bool,
    ttl_secs: u64,
) -> (Arc<WswListener>, u16, PathBuf) {
    let port = free_port();
    let ca_dir = temp_root(&format!("{tag}-ca"));
    let mut options = RuntimeOptions {
        listener_id: format!("wsw-lis-{tag}-0001"),
        name: format!("{tag} e2e"),
        session_id: "wsw-ses-e2e-0001".to_string(),
        out_of_scope_policy: "blocked".to_string(),
        config: wsw_runtime::ListenerConfig {
            bind: SocketAddr::from(([127, 0, 0, 1], port)),
            queue_capacity: 16,
            max_header_bytes: 64 * 1024,
            max_body_bytes: 1024 * 1024,
            queue_ttl_seconds: ttl_secs,
        },
        scope_rules: rules,
        intercept_enabled: intercept,
        intercept_matcher: InterceptMatcher {
            hosts: vec!["127.0.0.1".to_string()],
            methods: vec!["GET".to_string()],
            path_prefixes: vec!["/p59c-marker".to_string()],
        },
        ca_dir: Some(ca_dir.clone()),
        upstream_ca_paths: upstream_cas.to_vec(),
        db_path: db.map(Path::to_path_buf),
        artifacts_root: artifacts.to_path_buf(),
        capture_cas: true,
    };
    options.config.bind = SocketAddr::from(([127, 0, 0, 1], port));
    let migrations_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations");
    let listener = WswListener::start(options, &migrations_dir)
        .await
        .expect("listener start");
    (listener, port, ca_dir)
}

/// curl 子进程（spawn_blocking：current_thread runtime 下 std::output() 会
/// 阻塞执行线程造成 curl↔proxy 死锁——子进程必须走 blocking 线程池）。
async fn curl(args: Vec<String>) -> (i32, String, String) {
    tokio::task::spawn_blocking(move || {
        let out = std::process::Command::new("curl")
            .args(&args)
            .output()
            .expect("curl run");
        (
            out.status.code().unwrap_or(-1),
            String::from_utf8_lossy(&out.stdout).to_string(),
            String::from_utf8_lossy(&out.stderr).to_string(),
        )
    })
    .await
    .expect("blocking join")
}

/// curl 参数构造辅助（&str → owned String）。
fn curl_args(parts: &[&str]) -> Vec<String> {
    parts.iter().map(|s| s.to_string()).collect()
}

fn rules_for_range() -> ScopeRules {
    ScopeRules {
        include: vec!["127.0.0.1".to_string()],
        exclude: vec![],
        allowed_ports: vec![],
        allowed_path_prefixes: vec![],
        allow_private_targets: true,
        max_redirects: 5,
    }
}

#[tokio::test]
async fn plaintext_and_connect_and_scope_matrix() {
    tokio::time::timeout(std::time::Duration::from_secs(120), async {
        if !have_curl() {
            skip_curl_missing("plaintext_and_connect_and_scope_matrix");
            return;
        }
        let root = temp_root("matrix");
        let (marker_port, marker_task) = spawn_marker_server().await;
        let (fixture_ca, tls_cert, tls_key) = fixture_tls_files(&root);
        let alpn_record = Arc::new(std::sync::Mutex::new(None));
        let (tls_port, tls_task) = spawn_tls_server(
            false,
            Arc::clone(&alpn_record),
            &fixture_ca,
            &tls_cert,
            &tls_key,
        )
        .await;
        let db = root.join("e2e.db");
        let artifacts = root.join("artifacts");
        let (listener, proxy_port, ca_dir_actual) = start_listener(
            "matrix",
            rules_for_range(),
            Some(&db),
            &artifacts,
            std::slice::from_ref(&fixture_ca),
            false,
            30,
        )
        .await;

        // 明文面：靶场 marker 经代理 200。
        let (code, out, _) = curl(curl_args(&[
            "-s",
            "-x",
            &format!("http://127.0.0.1:{proxy_port}"),
            &format!("http://127.0.0.1:{marker_port}/p59c-marker"),
        ]))
        .await;
        assert_eq!(code, 0, "curl exit");
        assert_eq!(out.trim(), MARKER, "plaintext marker via proxy");
        // 再取首页（不同正文 → 不同 CAS 对象；marker 响应同 sha 去重）。
        let (code_home, _, _) = curl(curl_args(&[
            "-s",
            "-x",
            &format!("http://127.0.0.1:{proxy_port}"),
            &format!("http://127.0.0.1:{marker_port}/"),
        ]))
        .await;
        assert_eq!(code_home, 0, "index page via proxy");

        // CONNECT MITM 面：curl --cacert 信任 WSW CA → IP SAN 主机名校验通过。
        let ca_pem = root.join("wsw-ca.pem");
        let authority =
            WswAuthority::open(&ca_dir_actual, aws_lc_rs::default_provider()).expect("ca");
        std::fs::write(&ca_pem, authority.cert_pem()).expect("write ca pem");
        let (code, out, err) = curl(curl_args(&[
            "-s",
            "--cacert",
            ca_pem.to_str().unwrap(),
            "-x",
            &format!("http://127.0.0.1:{proxy_port}"),
            &format!("https://127.0.0.1:{tls_port}/p59c-marker"),
        ]))
        .await;
        assert_eq!(code, 0, "curl https via proxy failed: {err}");
        assert_eq!(out.trim(), MARKER, "CONNECT MITM marker");

        // 超范围拒（default deny 面：include 未命中）→ 403。
        let (code2, body2, _) = curl(curl_args(&[
            "-s",
            "-w",
            "\n%{http_code}",
            "-x",
            &format!("http://127.0.0.1:{proxy_port}"),
            "http://example.com:80/",
        ]))
        .await;
        assert_eq!(code2, 0);
        assert!(
            body2.trim_end().ends_with("403"),
            "out-of-scope must be 403, got: {body2}"
        );

        // 云元数据 deny-never-allow：allow_private=true + include 命中仍拒。
        let (code3, body3, _) = curl(curl_args(&[
            "-s",
            "-w",
            "\n%{http_code}",
            "-x",
            &format!("http://127.0.0.1:{proxy_port}"),
            "http://169.254.169.254/latest/meta-data/",
        ]))
        .await;
        assert_eq!(code3, 0);
        assert!(
            body3.trim_end().ends_with("403"),
            "metadata endpoint must be denied even with allow_private, got: {body3}"
        );
        assert!(
            body3.contains("metadata_endpoint"),
            "reason must be metadata_endpoint"
        );

        // 重定向越界拒：靶场 302 → scope 外 host，第二跳经代理 403。
        let (code4, body4, _) = curl(curl_args(&[
            "-sL",
            "-w",
            "\n%{http_code}",
            "-x",
            &format!("http://127.0.0.1:{proxy_port}"),
            &format!("http://127.0.0.1:{marker_port}/redirect-out-of-scope"),
        ]))
        .await;
        assert_eq!(code4, 0);
        assert!(
            body4.trim_end().ends_with("403"),
            "out-of-scope redirect hop must be blocked, got: {body4}"
        );

        // 捕获事实面（D9）：wsw_body_blob 行 + CAS 文件。
        tokio::time::sleep(std::time::Duration::from_millis(800)).await;
        let store = wsw_runtime::persist::PersistStore::open_file(&db).expect("db open");
        let _ =
            store.run_migrations(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations"));
        let blob_count = store
            .with_conn(|conn| {
                conn.query_row("SELECT COUNT(*) FROM wsw_body_blob", [], |r| {
                    r.get::<_, i64>(0)
                })
            })
            .expect("blob count");
        assert!(
            blob_count >= 2,
            "plaintext+https response bodies captured, got {blob_count}"
        );

        let status = listener.status();
        assert!(
            status.requests_total >= 5,
            "requests counted: {}",
            status.requests_total
        );
        assert!(
            status.scope_blocked_total >= 3,
            "blocked counted: {}",
            status.scope_blocked_total
        );
        assert_eq!(status.state, "running");

        listener.stop().await.expect("stop");
        marker_task.abort();
        tls_task.abort();
    })
    .await
    .expect("matrix e2e timeout (120s)");
    let _ = H2_ALPN_RECORDED;
}

#[tokio::test]
async fn non_loopback_bind_refused() {
    // 负例矩阵第 1 行：非 loopback bind → ConfigError::NonLoopback（exit 级）。
    let mut options = RuntimeOptions {
        ..Default::default()
    };
    options.config.bind = SocketAddr::from(([0, 0, 0, 0], free_port()));
    let migrations_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations");
    let err = match WswListener::start(options, &migrations_dir).await {
        Err(e) => e,
        Ok(_) => panic!("must refuse non-loopback bind"),
    };
    assert!(
        matches!(
            err,
            ListenerError::Config(wsw_runtime::ConfigError::NonLoopback)
        ),
        "expected NonLoopback, got {err:?}"
    );
    // :: 非回环地址同理。
    let mut options = RuntimeOptions::default();
    options.config.bind = SocketAddr::from(([0xfe80, 0, 0, 0, 0, 0, 0, 1], free_port()));
    let err = match WswListener::start(options, &migrations_dir).await {
        Err(e) => e,
        Ok(_) => panic!("must refuse non-loopback v6 bind"),
    };
    assert!(matches!(
        err,
        ListenerError::Config(wsw_runtime::ConfigError::NonLoopback)
    ));
}

#[tokio::test]
async fn intercept_queue_ttl_flow_via_live_proxy() {
    tokio::time::timeout(std::time::Duration::from_secs(60), async {
        if !have_curl() {
            skip_curl_missing("intercept_queue_ttl_flow_via_live_proxy");
            return;
        }
        let root = temp_root("intercept");
        let (marker_port, marker_task) = spawn_marker_server().await;
        let db = root.join("intercept.db");
        let artifacts = root.join("artifacts");
        let (listener, proxy_port, _ca) = start_listener(
            "intercept",
            rules_for_range(),
            Some(&db),
            &artifacts,
            &[],
            true,
            1, // TTL 1s：命中请求入队 → 等待超时 → EXPIRED + 自动放行
        )
        .await;

        let (code, out, _) = curl(curl_args(&[
            "-s",
            "-x",
            &format!("http://127.0.0.1:{proxy_port}"),
            &format!("http://127.0.0.1:{marker_port}/p59c-marker"),
        ]))
        .await;
        assert_eq!(code, 0);
        assert_eq!(
            out.trim(),
            MARKER,
            "intercepted request auto-forwarded after TTL"
        );

        // 终态面：persist 行 EXPIRED（TTL 到期路径）。
        let store = wsw_runtime::persist::PersistStore::open_file(&db).expect("db open");
        let _ =
            store.run_migrations(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations"));
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        let rows = store
            .list_intercepts_state("expired")
            .expect("expired rows");
        assert!(
            rows.iter().any(|r| r.host.as_deref() == Some("127.0.0.1")),
            "expired intercept row must be visible: {rows:?}"
        );
        listener.stop().await.expect("stop");
        // 停机后 queued 全 expired（本流只有一行，已 expired）。
        let queued = store.list_queued().expect("queued");
        assert!(queued.is_empty(), "no queued rows after stop: {queued:?}");
        marker_task.abort();
    })
    .await
    .expect("intercept e2e timeout");
}

/// W0 遗留 (b)：上游纯 h2-only 本地夹具实测（禁外联）。
/// 断言：请求成功 + 夹具服务端协商 ALPN == h2。
#[tokio::test]
async fn h2_only_upstream_fixture() {
    tokio::time::timeout(std::time::Duration::from_secs(60), async {
        if !have_curl() {
            skip_curl_missing("h2_only_upstream_fixture");
            return;
        }
        let root = temp_root("h2only");
        let (fixture_ca, tls_cert, tls_key) = fixture_tls_files(&root);
        let alpn_record = Arc::new(std::sync::Mutex::new(None));
        let (h2_port, h2_task) = spawn_tls_server(
            true,
            Arc::clone(&alpn_record),
            &fixture_ca,
            &tls_cert,
            &tls_key,
        )
        .await;
        let artifacts = root.join("artifacts");
        let (listener, proxy_port, ca_dir_actual) = start_listener(
            "h2only",
            rules_for_range(),
            None,
            &artifacts,
            std::slice::from_ref(&fixture_ca),
            false,
            30,
        )
        .await;

        // 导出 WSW CA 供下游信任。
        let authority =
            WswAuthority::open(&ca_dir_actual, aws_lc_rs::default_provider()).expect("ca");
        let ca_pem = root.join("wsw-ca.pem");
        std::fs::write(&ca_pem, authority.cert_pem()).expect("ca pem");
        let (code, out, err) = curl(curl_args(&[
            "-s",
            "--cacert",
            ca_pem.to_str().unwrap(),
            "-x",
            &format!("http://127.0.0.1:{proxy_port}"),
            &format!("https://127.0.0.1:{h2_port}/p59c-marker"),
        ]))
        .await;
        assert_eq!(code, 0, "h2-only upstream request failed: {err}");
        assert_eq!(out.trim(), MARKER, "h2-only marker");
        let alpn = alpn_record.lock().expect("alpn").clone();
        assert_eq!(
            alpn.as_deref(),
            Some("h2"),
            "upstream must negotiate h2 against h2-only fixture (recorded ALPN: {alpn:?})"
        );
        listener.stop().await.expect("stop");
        h2_task.abort();
    })
    .await
    .expect("h2-only e2e timeout");
}

/// CA 重生成显式动作语义（regenerate = 指纹变化；不静默）。
#[tokio::test]
async fn ca_regenerate_changes_fingerprint() {
    let root = temp_root("regen");
    let ca = WswAuthority::open(&root, aws_lc_rs::default_provider()).expect("open");
    let fp1 = ca.meta().fingerprint.clone();
    drop(ca);
    // 显式重生成 = 删三文件后重建（rp-wsw ca regenerate 同语义）。
    for f in ["ca.key.pem", "ca.cert.pem", "ca.meta.json"] {
        std::fs::remove_file(root.join(f)).expect("remove");
    }
    let ca2 = WswAuthority::open(&root, aws_lc_rs::default_provider()).expect("regen");
    assert_ne!(
        fp1,
        ca2.meta().fingerprint,
        "regenerate must change fingerprint"
    );
}

/// 敏感面（任务书 §五.12）：日志投影零头值/体值——单测代理面 grep 语义由
/// S8 实拍日志核验承载，此处固化 capture_finish 超帽不落 CAS 行为。
#[tokio::test]
async fn oversized_capture_is_truncated_without_cas_row() {
    let root = temp_root("trunc");
    let artifacts = root.join("artifacts");
    let rules = rules_for_range();
    let shared = Arc::new(wsw_runtime::listener::handler::ListenerShared::new(
        RuntimeOptions {
            scope_rules: rules,
            artifacts_root: artifacts.clone(),
            capture_cas: true,
            config: wsw_runtime::ListenerConfig {
                bind: SocketAddr::from(([127, 0, 0, 1], free_port())),
                queue_capacity: 8,
                max_header_bytes: 64 * 1024,
                max_body_bytes: 64,
                queue_ttl_seconds: 30,
            },
            ..Default::default()
        },
        None,
    ));
    let meta = wsw_runtime::listener::handler::__test_capture_meta("wsw-1-0-0001");
    // 100 字节 > cap 64 → truncated 路径不落 CAS。
    wsw_runtime::listener::handler::__test_capture_finish(&shared, &meta, &[b'x'; 100], true, 64);
    let dir_ok = std::fs::read_dir(&artifacts)
        .map(|mut d| d.next().is_none())
        .unwrap_or(true);
    assert!(dir_ok, "truncated capture must not write CAS files");
}
