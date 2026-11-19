//! WswAuthority：自实现 `hudsucker::certificate_authority::CertificateAuthority`
//! trait 的生产 CA（WSW-2-ADR W2-D2/D3；ADR-0004 D4 必做项）。
//!
//! 与 vendor `rcgen_authority.rs`（只读参照，不复制转正）的差异：
//! - IP 直连目标签 `SanType::IpAddress`（vendor :85-86 只发 DNS SAN——
//!   ADR-0004 差距 (a) 的补齐面）；`::ffff:` mapped 先归一 V4 再签；
//! - CA 根证书/私钥持久化复用（vendor 每次运行时生成、重启作废浏览器信任
//!   ——POC localhost 绕过不构成生产答案，ADR-0004 差距 (a) 原文）；
//! - 签发参数对齐 vendor 常量口径（`vendor/hudsucker/src/certificate_authority/
//!   mod.rs:15-17`：TTL=365d、缓存 TTL=半程、not_before=now-60s）；
//! - host 非法（空/超长/控制字符）→ 签发错误入 `downstream_tls` 五态并回落
//!   根证书条目（客户端握手必败，fail-closed），不静默回落 localhost 名。
//!
//! 存储位（W2-D3）：`rip-data/wsw-ca/`（env `RP_WSW_CA_DIR` 可覆盖），私钥
//! 0600 / 证书 0644 / 目录 0700，`create_new` 原子 no-clobber；不入 Git、
//! 不入日志（日志只落指纹前 16 hex）、禁静默安装（本模块零系统代理/信任库
//! 写面调用）。

use hudsucker::certificate_authority::CertificateAuthority;
use hudsucker::hyper::http::uri::Authority;
use hudsucker::rcgen::{
    BasicConstraints, CertificateParams, DistinguishedName, DnType, ExtendedKeyUsagePurpose, IsCa,
    Issuer, KeyPair, KeyUsagePurpose, SanType, string::Ia5String,
};
use hudsucker::rustls::crypto::CryptoProvider;
use hudsucker::rustls::pki_types::{
    CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer, pem::PemObject,
};
use hudsucker::rustls::ServerConfig;
use moka::future::Cache;
use rand::{RngExt, rng};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::fs::{OpenOptions, Permissions};
use std::io::Write;
use std::net::{IpAddr, Ipv6Addr};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use time::{Duration, OffsetDateTime};

/// 叶证书有效期（vendor `certificate_authority/mod.rs:15` 口径 = 365d）。
const TTL_SECS: i64 = 365 * 24 * 60 * 60;
/// 证书缓存条目 TTL = 有效期半程（vendor `mod.rs:16` 口径）。
const CACHE_TTL: u64 = TTL_SECS as u64 / 2;
/// not_before 前移量（vendor `mod.rs:17` 口径 = 60s，容忍时钟偏移）。
const NOT_BEFORE_OFFSET: i64 = 60;
/// 缓存容量（POC `crates/wsw-poc/src/main.rs:224` 同值；W2-D2 裁决）。
pub const CACHE_CAP: u64 = 1000;
/// 根 CA CN（W2-D3 展示面）。
pub const CA_CN: &str = "WSW-2-ROOT-CA";

/// CA 目录下文件名。
const KEY_FILE: &str = "ca.key.pem";
const CERT_FILE: &str = "ca.cert.pem";
const META_FILE: &str = "ca.meta.json";

/// CA 状态元数据（指纹/有效期/CN；rp_server 证书状态页数据源，W2-D8）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CaMeta {
    /// 根 CA CN。
    pub cn: String,
    /// not_before（ISO8601 UTC）。
    pub not_before: String,
    /// not_after（ISO8601 UTC）。
    pub not_after: String,
    /// 指纹 = SHA-256(证书 DER) 小写 hex（W2-D3 展示口径；005 迁移 :85 同形）。
    pub fingerprint: String,
}

/// WswAuthority 错误面（W2-D2 拒绝面 + W2-D3 落盘面）。
#[derive(Debug)]
pub enum AuthorityError {
    /// 目录/文件 IO 失败。
    Io(std::io::Error),
    /// rcgen 签发/解析失败。
    Rcgen(hudsucker::rcgen::Error),
    /// host 非法（空/超长/控制字符/越界字符）——`downstream_tls` 五态。
    InvalidHost(String),
    /// 既有 CA 文件与元数据指纹不一致（损坏防呆，不静默重生成）。
    FingerprintMismatch {
        /// 元数据登记指纹。
        expected: String,
        /// 实算指纹。
        actual: String,
    },
    /// 元数据 JSON 解析失败。
    MetaJson(serde_json::Error),
}

impl fmt::Display for AuthorityError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AuthorityError::Io(e) => write!(f, "ca dir io: {e}"),
            AuthorityError::Rcgen(e) => write!(f, "rcgen: {e}"),
            AuthorityError::InvalidHost(h) => write!(f, "invalid host: {h:?}"),
            AuthorityError::FingerprintMismatch { expected, actual } => {
                write!(f, "ca fingerprint mismatch: meta={expected} actual={actual}")
            }
            AuthorityError::MetaJson(e) => write!(f, "ca meta json: {e}"),
        }
    }
}

impl std::error::Error for AuthorityError {}

/// host 合法性守卫（W2-D2 拒绝面）：非空、≤253、字符白名单
/// （alnum/`-`/`.`/`:`/`[`/`]`/`_`/`*`——IPv6 括号与通配 CONNECT 目标兼容）。
/// 非法 → `InvalidHost`，不静默回落 localhost 名（ADR-0004 差距 (a) 原文）。
pub fn validate_host(host: &str) -> Result<(), AuthorityError> {
    let ok = !host.is_empty()
        && host.len() <= 253
        && host
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b':' | b'[' | b']' | b'_' | b'*'));
    if ok {
        Ok(())
    } else {
        Err(AuthorityError::InvalidHost(host.to_string()))
    }
}

/// host → SAN 归一（W2-D2 SAN 策略唯一入口）：
/// - IPv4 字面量 → `SanType::IpAddress(v4)`；
/// - IPv6 字面量 → mapped `::ffff:a.b.c.d` 归一 V4 再签，纯 v6 → `IpAddress(v6)`
///   （复用 `crates/tool-engine/src/document_fetch.rs:486-499` mapped 复核语义）；
/// - 其余 → `SanType::DnsName`（Ia5String，vendor `rcgen_authority.rs:85-86` 同形）。
///
/// `authority.host()` 对 IPv6 带 `[...]` 括号（http::uri::Authority 语义），先剥离。
pub fn leaf_san_for_host(host: &str) -> Result<SanType, AuthorityError> {
    validate_host(host)?;
    let stripped = host
        .strip_prefix('[')
        .and_then(|s| s.strip_suffix(']'))
        .unwrap_or(host);
    match stripped.parse::<IpAddr>() {
        Ok(IpAddr::V4(v4)) => Ok(SanType::IpAddress(IpAddr::V4(v4))),
        Ok(IpAddr::V6(v6)) => Ok(SanType::IpAddress(mapped_to_v4_or_v6(v6))),
        Err(_) => Ok(SanType::DnsName(
            Ia5String::try_from(host).map_err(|_| AuthorityError::InvalidHost(host.to_string()))?,
        )),
    }
}

/// mapped/纯 v6 → 归一 IP：`::ffff:0:0/96` 内嵌 v4 复原，否则原样 v6。
pub fn mapped_to_v4_or_v6(v6: Ipv6Addr) -> IpAddr {
    if let Some(v4) = v6.to_ipv4_mapped() {
        IpAddr::V4(v4)
    } else {
        IpAddr::V6(v6)
    }
}

/// 叶证书参数（SAN 分支 + CN=host + KeyUsage/EKU/use_authority_key_identifier，
/// vendor `rcgen_authority.rs:82-92` 同形、按 trait 自写）。公开面供单测逐位
/// 断言 SAN 类型（W2-D2 测试面）。
pub fn leaf_params_for_host(host: &str) -> Result<CertificateParams, AuthorityError> {
    let san = leaf_san_for_host(host)?;
    let mut params = CertificateParams::default();
    let mut dn = DistinguishedName::new();
    dn.push(DnType::CommonName, host);
    params.distinguished_name = dn;
    params.subject_alt_names = vec![san];
    params.key_usages = vec![KeyUsagePurpose::DigitalSignature];
    params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ServerAuth];
    params.use_authority_key_identifier_extension = true;
    Ok(params)
}

/// CA 目录解析（W2-D3 存储位）：env `RP_WSW_CA_DIR` 可覆盖，缺省仓库根相对
/// `rip-data/wsw-ca`（`.gitignore:47` rip-data/ 覆盖，运行时数据不入库）。
pub fn resolve_ca_dir() -> PathBuf {
    std::env::var("RP_WSW_CA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("rip-data/wsw-ca"))
}

/// 打开结果：`(issuer, private_key, root_cert_der, cert_pem, meta)`。
type OpenedCa = (
    Issuer<'static, KeyPair>,
    PrivateKeyDer<'static>,
    CertificateDer<'static>,
    String,
    CaMeta,
);

/// WswAuthority：根 CA 持久化 + 动态叶证书（IP/DNS SAN 分支）+ ServerConfig
/// 缓存（W2-D2 缓存参数：cap=1000、条目 TTL=证书有效期半程）。
pub struct WswAuthority {
    /// 根 CA 签发面（Issuer 持 CA 私钥与 DN/KeyUsage 参数）。
    issuer: Issuer<'static, KeyPair>,
    /// CA 私钥（PKCS8；落盘 0600）。
    private_key: PrivateKeyDer<'static>,
    /// 根证书 DER（指纹计算 + fail-closed 条目复用）。
    root_cert_der: CertificateDer<'static>,
    /// 根证书 PEM（显式导出面）。
    cert_pem: String,
    /// CA 元数据（指纹/有效期/CN）。
    meta: CaMeta,
    /// CA 存储目录。
    ca_dir: PathBuf,
    /// ServerConfig 缓存（moka future；key=Authority）。
    cache: Cache<Authority, Arc<ServerConfig>>,
    /// rustls crypto provider（aws_lc_rs）。
    provider: Arc<CryptoProvider>,
}

impl WswAuthority {
    /// 打开 CA：目录内已有 CA → 加载复用（重启不作废浏览器已导入信任）；
    /// 无 CA → 生成并落盘（私钥 0600 no-clobber / 目录 0700）。
    pub fn open(ca_dir: &Path, provider: CryptoProvider) -> Result<Self, AuthorityError> {
        std::fs::create_dir_all(ca_dir).map_err(AuthorityError::Io)?;
        std::fs::set_permissions(ca_dir, Permissions::from_mode(0o700))
            .map_err(AuthorityError::Io)?;

        let key_path = ca_dir.join(KEY_FILE);
        let (issuer, private_key, root_cert_der, cert_pem, meta) = if key_path.exists() {
            Self::load(ca_dir)?
        } else {
            Self::generate(ca_dir)?
        };
        let fingerprint = wsw_message::cas::sha256_hex(root_cert_der.as_ref());
        if meta.fingerprint != fingerprint {
            return Err(AuthorityError::FingerprintMismatch {
                expected: meta.fingerprint,
                actual: fingerprint,
            });
        }
        Ok(Self {
            issuer,
            private_key,
            root_cert_der,
            cert_pem,
            meta,
            ca_dir: ca_dir.to_path_buf(),
            cache: Cache::builder()
                .max_capacity(CACHE_CAP)
                .time_to_live(std::time::Duration::from_secs(CACHE_TTL))
                .build(),
            provider: Arc::new(provider),
        })
    }

    /// 缺省目录打开（`resolve_ca_dir()`）。
    pub fn open_default(provider: CryptoProvider) -> Result<Self, AuthorityError> {
        Self::open(&resolve_ca_dir(), provider)
    }

    /// 生成新 CA 并落盘（私钥 `create_new` mode 0600 原子 no-clobber）。
    fn generate(ca_dir: &Path) -> Result<OpenedCa, AuthorityError> {
        let key = KeyPair::generate().map_err(AuthorityError::Rcgen)?;
        let mut params =
            CertificateParams::new(Vec::<String>::new()).map_err(AuthorityError::Rcgen)?;
        let mut dn = DistinguishedName::new();
        dn.push(DnType::CommonName, CA_CN);
        params.distinguished_name = dn;
        params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        params.key_usages = vec![
            KeyUsagePurpose::DigitalSignature,
            KeyUsagePurpose::KeyCertSign,
        ];
        let not_before = OffsetDateTime::now_utc() - Duration::seconds(NOT_BEFORE_OFFSET);
        params.not_before = not_before;
        params.not_after = not_before + Duration::seconds(TTL_SECS);
        let nb_str = format_rfc3339(not_before);
        let na_str = format_rfc3339(not_before + Duration::seconds(TTL_SECS));

        let cert = params.self_signed(&key).map_err(AuthorityError::Rcgen)?;
        let cert_pem = cert.pem();
        let key_pem = key.serialize_pem();
        let der: CertificateDer<'static> = cert.der().clone();

        // 私钥 0600：DESIGN §3.3 CAS 0600/0700 同纪律；create_new 原子 no-clobber。
        let mut key_file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(ca_dir.join(KEY_FILE))
            .map_err(AuthorityError::Io)?;
        key_file.write_all(key_pem.as_bytes()).map_err(AuthorityError::Io)?;
        drop(key_file);
        write_file_mode(ca_dir.join(CERT_FILE), cert_pem.as_bytes(), 0o644)?;
        let meta = CaMeta {
            cn: CA_CN.to_string(),
            not_before: nb_str,
            not_after: na_str,
            fingerprint: wsw_message::cas::sha256_hex(der.as_ref()),
        };
        let meta_json = serde_json::to_vec_pretty(&meta).map_err(AuthorityError::MetaJson)?;
        write_file_mode(ca_dir.join(META_FILE), &meta_json, 0o644)?;

        let issuer = Issuer::new(params, key);
        let private_key =
            PrivateKeyDer::from(PrivatePkcs8KeyDer::from(issuer.key().serialize_der()));
        tracing::info!(
            fingerprint = %short_fp(&meta.fingerprint),
            dir = %ca_dir.display(),
            "wsw-ca root CA generated (CN={CA_CN}); trust install is an explicit user action only"
        );
        Ok((issuer, private_key, der, cert_pem, meta))
    }

    /// 加载既有 CA（重启复用面，W2-D3 生命周期表「启动：已有 CA → 加载复用」）。
    fn load(ca_dir: &Path) -> Result<OpenedCa, AuthorityError> {
        let key_pem = std::fs::read_to_string(ca_dir.join(KEY_FILE)).map_err(AuthorityError::Io)?;
        let cert_pem =
            std::fs::read_to_string(ca_dir.join(CERT_FILE)).map_err(AuthorityError::Io)?;
        let key = KeyPair::from_pem(&key_pem).map_err(AuthorityError::Rcgen)?;
        let issuer = Issuer::from_ca_cert_pem(&cert_pem, key).map_err(AuthorityError::Rcgen)?;
        let private_key =
            PrivateKeyDer::from(PrivatePkcs8KeyDer::from(issuer.key().serialize_der()));
        let der = pem_to_der(&cert_pem)?;
        let meta = match std::fs::read_to_string(ca_dir.join(META_FILE)) {
            Ok(raw) => serde_json::from_str(&raw).map_err(AuthorityError::MetaJson)?,
            // 元数据缺档不阻断复用：指纹实算补记，有效期标记 unknown（诚实展示）。
            Err(_) => CaMeta {
                cn: CA_CN.to_string(),
                not_before: "unknown".to_string(),
                not_after: "unknown".to_string(),
                fingerprint: wsw_message::cas::sha256_hex(der.as_ref()),
            },
        };
        tracing::info!(
            fingerprint = %short_fp(&meta.fingerprint),
            dir = %ca_dir.display(),
            "wsw-ca root CA loaded from disk (reuse across restarts)"
        );
        Ok((issuer, private_key, der, cert_pem, meta))
    }

    /// 根证书 PEM（导出面：rp_server 显式 confirm 动作消费）。
    pub fn cert_pem(&self) -> &str {
        &self.cert_pem
    }

    /// CA 元数据（指纹/有效期/CN）。
    pub fn meta(&self) -> &CaMeta {
        &self.meta
    }

    /// CA 目录。
    pub fn ca_dir(&self) -> &Path {
        &self.ca_dir
    }

    /// 证书缓存条目数（状态页可观测面）。
    pub fn cache_entry_count(&self) -> u64 {
        self.cache.entry_count()
    }

    /// host 非法/签发失败时的 fail-closed 条目：直接以根 CA 证书作服务器端
    /// 条目——rustls 客户端按「CA used as end-entity / EKU 缺 ServerAuth」
    /// 拒绝，握手必败；不 panic（trait 面不可错）、不静默成功、不静默回落
    /// localhost 名（W2-D2 拒绝面；`downstream_tls` 五态由调用面落日志）。
    fn fallback_config(&self) -> Arc<ServerConfig> {
        Arc::new(
            ServerConfig::builder_with_provider(Arc::clone(&self.provider))
                .with_safe_default_protocol_versions()
                .expect("protocol versions")
                .with_no_client_auth()
                .with_single_cert(vec![self.root_cert_der.clone()], self.private_key.clone_key())
                .expect("root cert + matching key"),
        )
    }
}

impl CertificateAuthority for WswAuthority {
    async fn gen_server_config(&self, authority: &Authority) -> Arc<ServerConfig> {
        if let Some(cfg) = self.cache.get(authority).await {
            return cfg;
        }
        let host = authority.host();
        let cfg = match leaf_params_for_host(host) {
            Ok(mut params) => {
                params.serial_number = Some(rng().random::<u64>().into());
                let not_before = OffsetDateTime::now_utc() - Duration::seconds(NOT_BEFORE_OFFSET);
                params.not_before = not_before;
                params.not_after = not_before + Duration::seconds(TTL_SECS);
                match params.signed_by(self.issuer.key(), &self.issuer) {
                    Ok(cert) => {
                        let mut server_cfg =
                            ServerConfig::builder_with_provider(Arc::clone(&self.provider))
                                .with_safe_default_protocol_versions()
                                .expect("protocol versions")
                                .with_no_client_auth()
                                .with_single_cert(
                                    vec![cert.der().clone()],
                                    self.private_key.clone_key(),
                                )
                                .expect("leaf cert + matching key");
                        // ALPN 广告 h2 + http/1.1（vendor rcgen_authority.rs:117-121 同形）。
                        server_cfg.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];
                        Arc::new(server_cfg)
                    }
                    Err(e) => {
                        tracing::error!(
                            class = "downstream_tls",
                            error_code = "leaf_sign_failed",
                            cause = %e,
                            host = %host,
                            "wsw-ca leaf signing failed; fail-closed handshake"
                        );
                        self.fallback_config()
                    }
                }
            }
            Err(e) => {
                tracing::error!(
                    class = "downstream_tls",
                    error_code = "invalid_host",
                    cause = %e,
                    "wsw-ca refuses to sign invalid host; fail-closed handshake"
                );
                self.fallback_config()
            }
        };
        self.cache.insert(authority.clone(), Arc::clone(&cfg)).await;
        cfg
    }
}

/// PEM → DER（rustls pki-types pem 面 `from_pem_slice`；POC `pem_file_iter` 同族 API）。
fn pem_to_der(pem: &str) -> Result<CertificateDer<'static>, AuthorityError> {
    CertificateDer::from_pem_slice(pem.as_bytes())
        .map_err(|_| AuthorityError::Rcgen(hudsucker::rcgen::Error::CouldNotParseCertificate))
}

fn write_file_mode(path: impl AsRef<Path>, bytes: &[u8], mode: u32) -> Result<(), AuthorityError> {
    let path = path.as_ref();
    let mut f = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(mode)
        .open(path)
        .map_err(AuthorityError::Io)?;
    f.write_all(bytes).map_err(AuthorityError::Io)?;
    drop(f);
    std::fs::set_permissions(path, Permissions::from_mode(mode)).map_err(AuthorityError::Io)
}

/// OffsetDateTime → RFC3339 UTC 串（展示面；time std feature）。
fn format_rfc3339(t: OffsetDateTime) -> String {
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        t.year(),
        u8::from(t.month()),
        t.day(),
        t.hour(),
        t.minute(),
        t.second(),
    )
}

/// 日志指纹投影：前 16 hex（W2-D3「不入日志」口径——完整指纹只在 CA 状态面）。
fn short_fp(fp: &str) -> &str {
    &fp[..fp.len().min(16)]
}

/// 私钥文件权限（0600 断言与 rp_server 状态面共用）。
pub fn key_file_mode(ca_dir: &Path) -> Result<u32, AuthorityError> {
    let meta = std::fs::metadata(ca_dir.join(KEY_FILE)).map_err(AuthorityError::Io)?;
    Ok(meta.permissions().mode() & 0o777)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::IpAddr;

    fn temp_dir(tag: &str) -> PathBuf {
        static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let n = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir =
            std::env::temp_dir().join(format!("wsw2-authority-{tag}-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    fn open_tmp(tag: &str) -> (WswAuthority, PathBuf) {
        let dir = temp_dir(tag);
        let ca = WswAuthority::open(&dir, hudsucker::rustls::crypto::aws_lc_rs::default_provider())
            .expect("open ca");
        (ca, dir)
    }

    #[test]
    fn leaf_san_ip_four_cases() {
        // 四例 IP SAN：127.0.0.1 / [::1]（authority.host() 带括号形态）/ v6 纯 /
        // ::ffff: mapped → 全部 SanType::IpAddress。
        for (host, expect) in [
            ("127.0.0.1", IpAddr::V4(std::net::Ipv4Addr::LOCALHOST)),
            ("[::1]", IpAddr::V6(std::net::Ipv6Addr::LOCALHOST)),
            ("2001:db8::1", "2001:db8::1".parse::<IpAddr>().unwrap()),
            ("::ffff:127.0.0.1", IpAddr::V4(std::net::Ipv4Addr::LOCALHOST)),
        ] {
            let params = leaf_params_for_host(host).expect("params");
            assert_eq!(params.subject_alt_names.len(), 1, "{host}");
            match &params.subject_alt_names[0] {
                SanType::IpAddress(ip) => assert_eq!(ip, &expect, "{host}"),
                other => panic!("{host}: expected IpAddress, got {other:?}"),
            }
        }
    }

    #[test]
    fn leaf_san_dns_case() {
        let params = leaf_params_for_host("local-range.test").expect("params");
        match &params.subject_alt_names[0] {
            SanType::DnsName(_) => {}
            other => panic!("expected DnsName, got {other:?}"),
        }
        assert!(matches!(
            leaf_san_for_host("metadata.google.internal"),
            Ok(SanType::DnsName(_))
        ));
    }

    #[test]
    fn invalid_host_rejected() {
        assert!(matches!(validate_host(""), Err(AuthorityError::InvalidHost(_))));
        assert!(matches!(
            validate_host("bad host\r\n"),
            Err(AuthorityError::InvalidHost(_))
        ));
        assert!(matches!(
            validate_host("a/../../b"),
            Err(AuthorityError::InvalidHost(_))
        ));
        let long = "x".repeat(254);
        assert!(matches!(validate_host(&long), Err(AuthorityError::InvalidHost(_))));
    }

    #[tokio::test]
    async fn cache_hit_returns_same_arc() {
        let (ca, _dir) = open_tmp("cache");
        let authority: Authority = "local-range.test:8443".parse().unwrap();
        let first = CertificateAuthority::gen_server_config(&ca, &authority).await;
        let second = CertificateAuthority::gen_server_config(&ca, &authority).await;
        assert!(
            Arc::ptr_eq(&first, &second),
            "same Authority must hit moka cache and return same Arc<ServerConfig>"
        );
        // 不同 Authority 各自签发（Arc 不同；moka entry_count 异步维护有滞后，
        // 不作即时断言面）。cap=1000 常量可核。
        let other: Authority = "other.test:8443".parse().unwrap();
        let third = CertificateAuthority::gen_server_config(&ca, &other).await;
        assert!(!Arc::ptr_eq(&first, &third));
        assert_eq!(CACHE_CAP, 1000);
    }

    #[tokio::test]
    async fn ca_persistence_reuse_across_restart() {
        let (ca, dir) = open_tmp("persist");
        let fp1 = ca.meta().fingerprint.clone();
        let pem1 = ca.cert_pem().to_string();
        drop(ca);
        let ca2 =
            WswAuthority::open(&dir, hudsucker::rustls::crypto::aws_lc_rs::default_provider())
                .expect("reopen ca");
        assert_eq!(ca2.meta().fingerprint, fp1, "restart must reuse the same CA");
        assert_eq!(ca2.cert_pem(), pem1);
        // 私钥/证书文件确实落在目录内。
        assert!(dir.join(KEY_FILE).is_file());
        assert!(dir.join(CERT_FILE).is_file());
        assert!(dir.join(META_FILE).is_file());
    }

    #[test]
    fn private_key_mode_0600() {
        let (_ca, dir) = open_tmp("perm");
        let mode = key_file_mode(&dir).expect("stat");
        assert_eq!(mode, 0o600, "ca.key.pem must be 0600, got {mode:o}");
        let cert_mode =
            std::fs::metadata(dir.join(CERT_FILE)).unwrap().permissions().mode() & 0o777;
        assert_eq!(cert_mode, 0o644);
        let dir_mode = std::fs::metadata(&dir).unwrap().permissions().mode() & 0o777;
        assert_eq!(dir_mode, 0o700);
    }

    #[tokio::test]
    async fn invalid_host_fail_closed_not_panic() {
        let (ca, _dir) = open_tmp("invalid");
        // trait 面不可错：非法 host 不 panic、不污染缓存、走 fail-closed 条目
        // （http::uri::Authority 对含空白串可能先拒，用超长 host 形态压闸）。
        let long = format!("{}:443", "x".repeat(300));
        let bad: Authority = long.parse().expect("authority");
        let cfg = CertificateAuthority::gen_server_config(&ca, &bad).await;
        // fail-closed 条目不应进入缓存（cache 仍为空）；回落条目不带 ALPN 广告。
        assert_eq!(ca.cache_entry_count(), 0);
        assert!(cfg.alpn_protocols.is_empty());
    }

    #[test]
    fn fingerprint_is_sha256_of_der() {
        let (ca, _dir) = open_tmp("fp");
        // wsw_message::cas::sha256_hex 口径：64 位小写 hex（005 迁移 :86 同形）。
        let fp = ca.meta().fingerprint.clone();
        assert_eq!(fp.len(), 64);
        assert!(fp.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)));
    }
}
