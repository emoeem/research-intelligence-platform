//! Artifact Adapter：HTTP 正文 CAS 写入（DESIGN §3.3 被动捕获面）。
//!
//! 职责（WSW-1-TASK §一.3）：
//! - Body 正文 CAS 写入：sha256 寻址、去重（同 sha256 幂等命中不重写）、
//!   超帽截断复用 tool-engine envelope 协议（红线 7：截断口径唯一）。
//! - 正文落 `<RP_ARTIFACTS_DIR>/wsw/<sha256 前2>/<sha256>`；uri 记相对路径
//!   `wsw/<前2>/<sha256>`（根目录经 [`resolve_artifacts_dir`] 在读取时解析，
//!   目录可迁移）。
//! - sha256 小写 hex（`migrations/005_tool_contract.sql:86` 口径）。
//!
//! 截断口径（tool-engine `envelope.rs:281-321` TruncationOutcome 语义，逐条
//! 对齐）：超帽时不修改正文原始字节，登记面携带**原始字节 sha256**
//! （envelope「原始字节交由调用方落 Artifact + 原始 sha256」同口径），
//! `fidelity=truncated` 承载降级标注（envelope `usage.truncated=true` 同义）；
//! 内联降级说明由消费方（Exchange 元数据/Display 层）生成，不另造截断字段。
//! 不发送任何网络请求（无 IO 出面）。

use crate::types::CaptureFidelity;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fmt;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// CAS 子目录（正文统一落 artifacts 根下 `wsw/`，与主动面 tool_run artifact
/// 目录分离，DESIGN §3.3 双层归属）。
pub const CAS_SUBDIR: &str = "wsw";

/// CAS 引用（DESIGN §1.2 request_body_artifact/response_body_artifact 行：
/// sha256 + uri + size + fidelity）。
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BodyRef {
    /// 正文 sha256（小写 hex，64 字符）。
    pub sha256: String,
    /// 存储相对 uri：`wsw/<sha256 前2>/<sha256>`（根 =
    /// [`resolve_artifacts_dir`]）。
    pub uri: String,
    /// 原始字节数。
    pub size: u64,
    /// 保真标注（超帽 → [`CaptureFidelity::Truncated`]）。
    pub fidelity: CaptureFidelity,
}

/// CAS 错误。
#[derive(Debug)]
pub enum CasError {
    /// 文件系统错误（目录创建/文件写入）。
    Io(std::io::Error),
    /// sha256 形状非法（防御面：非 64 字符小写 hex）。
    InvalidSha256(String),
    /// CAS URI 不符合固定相对路径形状。
    InvalidUri(String),
    /// CAS 目标已存在但内容哈希不匹配。
    IntegrityMismatch {
        /// URI 或目标路径声明的期望 SHA-256。
        expected: String,
        /// 文件内容实际 SHA-256。
        actual: String,
    },
    /// CAS 目标不是普通文件（例如符号链接或目录）。
    InvalidExistingEntry(PathBuf),
}

impl fmt::Display for CasError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "cas io error: {e}"),
            Self::InvalidSha256(s) => write!(f, "invalid sha256: {s:?}"),
            Self::InvalidUri(uri) => write!(f, "invalid CAS uri: {uri:?}"),
            Self::IntegrityMismatch { expected, actual } => {
                write!(
                    f,
                    "CAS integrity mismatch: expected {expected}, got {actual}"
                )
            }
            Self::InvalidExistingEntry(path) => {
                write!(f, "CAS entry is not a regular file: {}", path.display())
            }
        }
    }
}

impl std::error::Error for CasError {}

impl From<std::io::Error> for CasError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

/// 字节 sha256 小写 hex（005 迁移 artifact.sha256 口径；解析/测试共用）。
pub fn sha256_hex(bytes: &[u8]) -> String {
    hex_lower(&Sha256::digest(bytes))
}

/// 字节 → 小写 hex（无 hex crate 依赖，手写编码仅此一处）。
fn hex_lower(digest: &[u8]) -> String {
    let mut out = String::with_capacity(digest.len() * 2);
    for b in digest {
        out.push_str(&format!("{b:02x}"));
    }
    out
}

/// sha256 形状校验（64 字符小写 hex）。
pub fn is_valid_sha256(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// Artifact 根目录解析：`RP_ARTIFACTS_DIR` env，缺省 `rip-data/artifacts`
/// （00 审计 §2.8：默认 rip-data/artifacts；mcp-server rp-mcp.rs:116 先例）。
///
/// 测试纪律（WSW-1-TASK §四.4）：测试不依赖会话环境变量 —— 测试内自管
/// tempdir 并显式传根目录给 [`store_body`]，不走本函数。
pub fn resolve_artifacts_dir() -> PathBuf {
    std::env::var("RP_ARTIFACTS_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("rip-data/artifacts"))
}

/// 正文 CAS 写入（sha256 寻址 + 去重 + 超帽截断 envelope 口径）。
///
/// - `cap`：体帽（字节）。`bytes.len() > cap` 时 fidelity=Truncated；
///   正文仍写**原始全量字节**（envelope §5.4：原始字节落 Artifact，原始
///   sha256 登记 —— 内联降级由消费方承载）。
/// - 去重：目标文件已存在时先重新计算哈希；只有内容与寻址哈希一致才幂等命中。
///   新内容先写同目录临时文件，再用 hard-link 原子无覆盖落位，避免半写文件成为 CAS 命中。
/// - 返回 [`BodyRef`]；`size` 为原始字节数。
pub fn store_body(artifacts_root: &Path, bytes: &[u8], cap: u64) -> Result<BodyRef, CasError> {
    let sha = sha256_hex(bytes);
    if !is_valid_sha256(&sha) {
        return Err(CasError::InvalidSha256(sha));
    }
    let dir = artifacts_root.join(CAS_SUBDIR).join(&sha[..2]);
    create_private_dir(&dir)?;
    let path = dir.join(&sha);

    // 命中已有 CAS 项时先验证内容，不把同名损坏文件当作成功去重。
    if path.exists() {
        verify_existing_file(&path, &sha)?;
    } else {
        static TEMP_SEQ: AtomicU64 = AtomicU64::new(0);
        let mut temp_path = None;
        let mut temp_file = None;
        for _ in 0..16 {
            let seq = TEMP_SEQ.fetch_add(1, Ordering::Relaxed);
            let candidate = dir.join(format!(".{sha}.tmp-{}-{seq}", std::process::id()));
            let mut options = OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            match options.open(&candidate) {
                Ok(file) => {
                    temp_path = Some(candidate);
                    temp_file = Some(file);
                    break;
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error.into()),
            }
        }
        let temp_path = temp_path.ok_or_else(|| {
            CasError::Io(std::io::Error::new(
                std::io::ErrorKind::AlreadyExists,
                "could not allocate a unique CAS temporary file",
            ))
        })?;
        let write_result = (|| -> Result<(), CasError> {
            let mut file = temp_file.expect("temp file allocated with path");
            file.write_all(bytes)?;
            file.sync_all()?;
            drop(file);
            match fs::hard_link(&temp_path, &path) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                    verify_existing_file(&path, &sha)?;
                }
                Err(error) => return Err(error.into()),
            }
            Ok(())
        })();
        let cleanup_result = fs::remove_file(&temp_path);
        if let Err(error) = write_result {
            let _ = cleanup_result;
            return Err(error);
        }
        cleanup_result?;
        #[cfg(unix)]
        fs::File::open(&dir)?.sync_all()?;
    }

    let fidelity = if bytes.len() as u64 > cap {
        CaptureFidelity::Truncated
    } else {
        CaptureFidelity::Full
    };
    Ok(BodyRef {
        uri: format!("{CAS_SUBDIR}/{}/{}", &sha[..2], sha),
        sha256: sha,
        size: bytes.len() as u64,
        fidelity,
    })
}

/// 创建 CAS 前缀目录并收紧权限；抓包正文可能含 Cookie/Authorization。
fn create_private_dir(path: &Path) -> Result<(), CasError> {
    fs::create_dir_all(path)?;
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.file_type().is_dir() {
        return Err(CasError::InvalidExistingEntry(path.to_path_buf()));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

fn verify_existing_file(path: &Path, expected_sha: &str) -> Result<(), CasError> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.file_type().is_file() {
        return Err(CasError::InvalidExistingEntry(path.to_path_buf()));
    }
    let existing = fs::read(path)?;
    let actual = sha256_hex(&existing);
    if actual != expected_sha {
        return Err(CasError::IntegrityMismatch {
            expected: expected_sha.to_string(),
            actual,
        });
    }
    Ok(())
}

/// 将 CAS URI 严格解析为固定的 wsw/<前2>/<sha256> 路径。
fn resolve_body_path(artifacts_root: &Path, uri: &str) -> Result<(PathBuf, String), CasError> {
    let parts = uri.split('/').collect::<Vec<_>>();
    if parts.len() != 3
        || parts[0] != CAS_SUBDIR
        || parts[1].len() != 2
        || !is_valid_sha256(parts[2])
        || !parts[2].starts_with(parts[1])
        || uri.contains('\\')
    {
        return Err(CasError::InvalidUri(uri.to_string()));
    }
    Ok((
        artifacts_root
            .join(CAS_SUBDIR)
            .join(parts[1])
            .join(parts[2]),
        parts[2].to_string(),
    ))
}

/// 读回 CAS 正文并校验 SHA-256；拒绝路径穿越、符号链接与损坏内容。
pub fn read_body(artifacts_root: &Path, uri: &str) -> Result<Vec<u8>, CasError> {
    let (path, expected_sha) = resolve_body_path(artifacts_root, uri)?;
    let metadata = fs::symlink_metadata(&path)?;
    if !metadata.file_type().is_file() {
        return Err(CasError::InvalidExistingEntry(path));
    }
    let bytes = fs::read(&path)?;
    let actual = sha256_hex(&bytes);
    if actual != expected_sha {
        return Err(CasError::IntegrityMismatch {
            expected: expected_sha,
            actual,
        });
    }
    Ok(bytes)
}

/// 测试自管 tempdir（std-only，不引 tempfile；不依赖会话环境变量）。
#[cfg(test)]
pub(crate) fn temp_root(tag: &str) -> PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let n = SEQ.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "{}-{}-{}-{:x}",
        tag,
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos(),
        n
    ));
    std::fs::create_dir_all(&dir).expect("create temp root");
    dir
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_known_vector_lowercase_hex() {
        // sha256("abc") 钉值
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(sha256_hex(b"").len(), 64);
        assert!(is_valid_sha256(&sha256_hex(b"")));
        assert!(!is_valid_sha256(
            "BA7816BF8F01CFEA414140DE5DAE2223B00361A396177A9CB410FF61F20015AD"
        ));
        assert!(!is_valid_sha256(&"x".repeat(64)));
    }

    #[test]
    fn store_body_self_managed_tempdir_dedup_and_fidelity() {
        // 测试自管 tempdir（§四.4：不依赖会话环境变量）
        let root = temp_root("wswmsg-cas-unit");
        let bytes: Vec<u8> = (0..4096u32).map(|i| (i % 251) as u8).collect();
        let sha = sha256_hex(&bytes);

        // 1. 超帽 → truncated + 原始字节全量落档
        let r1 = store_body(&root, &bytes, 1024).unwrap();
        assert_eq!(r1.sha256, sha);
        assert_eq!(r1.uri, format!("wsw/{}/{}", &sha[..2], sha));
        assert_eq!(r1.size, 4096);
        assert_eq!(r1.fidelity, CaptureFidelity::Truncated);
        let path = root.join("wsw").join(&sha[..2]).join(&sha);
        assert!(path.exists());
        let back = std::fs::read(&path).unwrap();
        assert_eq!(back, bytes);
        assert_eq!(sha256_hex(&back), r1.sha256, "读回字节 sha256 与登记值全等");

        // 2. 同字节二次写入 → 同 BodyRef（去重实证，PLAN :576）
        let r2 = store_body(&root, &bytes, 1024).unwrap();
        assert_eq!(r1, r2);

        // 3. 帽内 → full
        let r3 = store_body(&root, b"tiny", 1024).unwrap();
        assert_eq!(r3.fidelity, CaptureFidelity::Full);
        assert_eq!(r3.size, 4);

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn corrupt_existing_cas_entry_is_not_accepted_as_a_dedup_hit() {
        let root = temp_root("wswmsg-cas-corrupt");
        let bytes = b"expected body";
        let sha = sha256_hex(bytes);
        let dir = root.join("wsw").join(&sha[..2]);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(&sha), b"tampered").unwrap();

        assert!(matches!(
            store_body(&root, bytes, 1024),
            Err(CasError::IntegrityMismatch { .. })
        ));
        let uri = format!("wsw/{}/{}", &sha[..2], sha);
        assert!(matches!(
            read_body(&root, &uri),
            Err(CasError::IntegrityMismatch { .. })
        ));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn read_body_rejects_path_traversal_and_noncanonical_uri() {
        let root = temp_root("wswmsg-cas-uri");
        assert!(matches!(
            read_body(&root, "wsw/../../etc/passwd"),
            Err(CasError::InvalidUri(_))
        ));
        assert!(matches!(
            read_body(&root, "wsw/aa/../aa"),
            Err(CasError::InvalidUri(_))
        ));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn concurrent_cas_writes_are_atomic_and_deduplicated() {
        let root = temp_root("wswmsg-cas-race");
        let bytes = b"concurrent body".repeat(4096);
        let expected = sha256_hex(&bytes);
        let workers = (0..8)
            .map(|_| {
                let root = root.clone();
                let bytes = bytes.clone();
                std::thread::spawn(move || store_body(&root, &bytes, 1024).unwrap())
            })
            .collect::<Vec<_>>();
        for worker in workers {
            let reference = worker.join().unwrap();
            assert_eq!(reference.sha256, expected);
            assert_eq!(reference.fidelity, CaptureFidelity::Truncated);
        }
        let uri = format!("wsw/{}/{}", &expected[..2], expected);
        assert_eq!(read_body(&root, &uri).unwrap(), bytes);
        let temp_files = std::fs::read_dir(root.join("wsw").join(&expected[..2]))
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().contains(".tmp-"))
            .count();
        assert_eq!(temp_files, 0, "temporary files are removed after commit");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[cfg(unix)]
    #[test]
    fn cas_files_and_prefix_directories_are_private() {
        use std::os::unix::fs::PermissionsExt;

        let root = temp_root("wswmsg-cas-mode");
        let reference = store_body(&root, b"private body", 1024).unwrap();
        let path = root.join(&reference.uri);
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(
            std::fs::metadata(path.parent().unwrap())
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn truncation_protocol_parity_with_tool_engine_envelope() {
        // 红线 7：截断口径唯一。语义对照（envelope.rs:281-321 TruncationOutcome）：
        // envelope 超帽 → truncated=true + original_bytes=原始序列化字节数 +
        // original_sha256=原始字节 sha256（原始字节交由调用方落 Artifact）；
        // 本 crate store_body 超帽 → fidelity=Truncated + size=原始字节数 +
        // sha256=原始字节 sha256（原始字节全量落档）。
        // 结构面逐项对齐：truncated 标记 / 原始字节数 / 原始 sha256 三要素
        // 两侧同在且都指向「原始」而非降级说明（envelope 序列化走
        // tool_registry::normalize_json，跨 crate 不复制其 canonical 化，
        // 故 hash 值按各自原始字节比对）。
        let value = serde_json::json!({ "body_preview": "x".repeat(600) });
        let outcome = tool_engine::envelope::truncate("rn-parity", &value, 128);
        assert!(outcome.truncated, "envelope 超帽标记");
        // normalize_json = serde_json::to_string（tool-registry validate.rs:94-96）
        let compact = serde_json::to_string(&value).unwrap();
        assert_eq!(
            outcome.original_bytes,
            compact.len() as u64,
            "envelope original_bytes = 原始序列化字节数（非降级说明长度）"
        );
        let env_sha = outcome
            .original_sha256
            .expect("envelope 截断必须携带原始 sha256");
        assert_eq!(env_sha.len(), 64);
        assert_eq!(
            env_sha,
            sha256_hex(compact.as_bytes()),
            "envelope original_sha256 = 原始序列化字节的 sha256"
        );

        let bytes = b"payload".repeat(64); // 448 > cap 128
        let root = temp_root("wswmsg-cas-parity");
        let r = store_body(&root, &bytes, 128).unwrap();
        // 结构面逐项对齐
        assert_eq!(
            r.fidelity,
            CaptureFidelity::Truncated,
            "⇔ envelope truncated=true"
        );
        assert_eq!(
            r.size,
            bytes.len() as u64,
            "⇔ envelope original_bytes=原始长度"
        );
        assert_eq!(
            r.sha256,
            sha256_hex(&bytes),
            "登记原始字节 sha256（⇔ envelope original_sha256=原始序列化字节 sha）"
        );
        // 落档内容 = 原始全量字节（envelope「原始字节落 Artifact」同口径）
        let back = std::fs::read(root.join("wsw").join(&r.sha256[..2]).join(&r.sha256)).unwrap();
        assert_eq!(back, bytes);
        let _ = std::fs::remove_dir_all(&root);
    }
}
