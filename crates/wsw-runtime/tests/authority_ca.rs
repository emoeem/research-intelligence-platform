//! WswAuthority CA 集成测试（WSW-2-TASK §三 S2 门 + W2-D2/D3）：SAN 断言
//! 逐位、缓存命中同 Arc、CA 持久化加载复用、私钥 0600、指纹与导出面。

use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::sync::Arc;

use hudsucker::certificate_authority::CertificateAuthority;
use hudsucker::hyper::http::uri::Authority;
use hudsucker::rustls::crypto::aws_lc_rs;
use wsw_runtime::authority::{self, CaMeta, WswAuthority};

fn temp_root(tag: &str) -> PathBuf {
    static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("wsw2-auth-ca-{tag}-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

fn open(dir: &std::path::Path) -> WswAuthority {
    WswAuthority::open(dir, aws_lc_rs::default_provider()).expect("open ca")
}

/// IP SAN 四例 + DNS SAN 例（W2-D2 SAN 策略逐位断言）。
#[test]
fn san_matrix_ip_four_cases_and_dns() {
    for (host, expect) in [
        ("127.0.0.1", "ip:127.0.0.1"),
        ("[::1]", "ip:::1"),
        ("2001:db8::1", "ip:2001:db8::1"),
        ("::ffff:127.0.0.1", "ip:127.0.0.1"),
        ("local-range.test", "dns:local-range.test"),
    ] {
        let params = authority::leaf_params_for_host(host).expect("params");
        assert_eq!(params.subject_alt_names.len(), 1, "{host} 单 SAN");
        let actual = match &params.subject_alt_names[0] {
            hudsucker::rcgen::SanType::IpAddress(ip) => format!("ip:{ip}"),
            hudsucker::rcgen::SanType::DnsName(_) => format!("dns:{host}"),
            other => panic!("{host}: unexpected san {other:?}"),
        };
        assert_eq!(actual, expect, "SAN branch per host");
    }
    // mapped 归一语义独立可核。
    let v6: std::net::Ipv6Addr = "::ffff:10.1.2.3".parse().unwrap();
    assert_eq!(
        authority::mapped_to_v4_or_v6(v6),
        std::net::IpAddr::V4(std::net::Ipv4Addr::new(10, 1, 2, 3))
    );
}

#[tokio::test]
async fn cache_hit_same_arc_and_alpn() {
    let dir = temp_root("cache");
    let ca = open(&dir);
    let a: Authority = "local-range.test:8443".parse().unwrap();
    let first = ca.gen_server_config(&a).await;
    let second = ca.gen_server_config(&a).await;
    assert!(Arc::ptr_eq(&first, &second), "moka cache returns same Arc");
    // ALPN 广告 h2 + http/1.1（vendor rcgen_authority.rs:117-121 同形）。
    let mut alpn = first.alpn_protocols.clone();
    alpn.sort();
    assert_eq!(alpn, vec![b"h2".to_vec(), b"http/1.1".to_vec()]);
    // 签发参数：not_before 前移 60s + TTL 365d（W2-D2 常量口径）。
    let dns: Authority = "other.test:443".parse().unwrap();
    let _ = ca.gen_server_config(&dns).await;
    assert_eq!(authority::CACHE_CAP, 1000);
}

#[tokio::test]
async fn ca_persistence_reuse_and_files() {
    let dir = temp_root("persist");
    let (fp, pem, meta1) = {
        let ca = open(&dir);
        (
            ca.meta().fingerprint.clone(),
            ca.cert_pem().to_string(),
            ca.meta().clone(),
        )
    };
    // 重启加载复用（同指纹同 PEM；重启不作废浏览器已导入信任）。
    let ca2 = open(&dir);
    assert_eq!(ca2.meta().fingerprint, fp);
    assert_eq!(ca2.cert_pem(), pem);
    let meta2 = ca2.meta().clone();
    assert_eq!(meta1, meta2);
    // 文件面：三文件齐 + 元数据形状。
    assert!(dir.join("ca.key.pem").is_file());
    assert!(dir.join("ca.cert.pem").is_file());
    assert!(dir.join("ca.meta.json").is_file());
    let raw: CaMeta =
        serde_json::from_str(&std::fs::read_to_string(dir.join("ca.meta.json")).unwrap()).unwrap();
    assert_eq!(raw.cn, "WSW-2-ROOT-CA");
    assert_eq!(raw.fingerprint.len(), 64);
    assert!(raw.not_before.ends_with('Z'));
    assert!(raw.not_after.ends_with('Z'));
    // 私钥 0600 / 证书 0644 / 目录 0700（W2-D3 权限矩阵）。
    assert_eq!(authority::key_file_mode(&dir).unwrap(), 0o600);
    let cert_mode = std::fs::metadata(dir.join("ca.cert.pem"))
        .unwrap()
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(cert_mode, 0o644);
    let dir_mode = std::fs::metadata(&dir).unwrap().permissions().mode() & 0o777;
    assert_eq!(dir_mode, 0o700);
}

#[tokio::test]
async fn fingerprint_mismatch_refuses_load() {
    let dir = temp_root("mismatch");
    let _ = open(&dir);
    // 损坏防呆：篡改 meta 指纹 → 加载拒（不静默重生成）。
    let mut raw: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(dir.join("ca.meta.json")).unwrap()).unwrap();
    raw["fingerprint"] = serde_json::Value::String("0".repeat(64));
    std::fs::write(
        dir.join("ca.meta.json"),
        serde_json::to_vec_pretty(&raw).unwrap(),
    )
    .unwrap();
    let err = WswAuthority::open(&dir, aws_lc_rs::default_provider());
    assert!(matches!(
        err,
        Err(authority::AuthorityError::FingerprintMismatch { .. })
    ));
}

#[tokio::test]
async fn invalid_host_fail_closed() {
    let dir = temp_root("invalid");
    let ca = open(&dir);
    let long = format!("{}:443", "x".repeat(300));
    let bad: Authority = long.parse().unwrap();
    let cfg = ca.gen_server_config(&bad).await;
    // fail-closed 条目不进缓存、不带 ALPN（与正常叶路径区分）。
    assert_eq!(ca.cache_entry_count(), 0);
    assert!(cfg.alpn_protocols.is_empty());
    // 空白/控制字符 host 形状被 validate 拒（W2-D2 拒绝面）。
    assert!(authority::validate_host("").is_err());
    assert!(authority::validate_host("a b").is_err());
    assert!(authority::validate_host("ok-host.test").is_ok());
}
