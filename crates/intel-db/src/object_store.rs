//! Object Storage Adapter：本地文件系统实现 + trait 面（S3 预留，R2 10.1B/10.3）。
//!
//! 路径纪律：业务代码只持有 cas:// URI，禁止拼文件系统路径；目录布局
//! `&lt;root&gt;/&lt;hash 前两字节&gt;/&lt;hash&gt;`（防单目录爆炸）。读时 sha256 复核
//! （完整性自证，12-STORAGE 第五节）。

use crate::{IntelError, IntelResult};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

/// 存取结果（put 返回）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredObject {
    /// sha256 hex（CAS 寻址键 = content_objects.content_hash）。
    pub content_hash: String,
    /// 字节数。
    pub size_bytes: u64,
    /// cas://&lt;hash&gt; 形 URI（落 documents.storage_uri / content_objects.storage_uri）。
    pub storage_uri: String,
}

/// 统一存取接口（S3 兼容实现预留：R2 10.3 Provider 接口，业务零供应商耦合）。
pub trait ObjectStore: Send + Sync {
    /// 写入内容（幂等：同 hash 同对象；原子写 tmp+rename）。
    fn put(&self, content: &[u8]) -> IntelResult<StoredObject>;
    /// 读回并 sha256 复核。
    fn read(&self, storage_uri: &str) -> IntelResult<Vec<u8>>;
    /// 对象是否在位。
    fn exists(&self, storage_uri: &str) -> IntelResult<bool>;
    /// 物理删除（仅 gc 面；ref_count 保护在账面层）。
    fn delete(&self, storage_uri: &str) -> IntelResult<()>;
}

/// sha256 hex（本 crate 通用）。
pub fn sha256_hex(content: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content);
    format!("{:x}", hasher.finalize())
}

/// cas:// URI 解析（非法形态拒绝）。
fn uri_to_hash(storage_uri: &str) -> Result<&str, IntelError> {
    storage_uri
        .strip_prefix("cas://")
        .filter(|h| !h.is_empty() && h.len() == 64 && h.chars().all(|c| c.is_ascii_hexdigit()))
        .ok_or_else(|| IntelError::Invalid(format!("invalid cas uri: {storage_uri:?}")))
}

fn object_path(root: &Path, content_hash: &str) -> PathBuf {
    root.join(&content_hash[0..2]).join(&content_hash[2..4]).join(content_hash)
}

/// 本地文件系统实现（P1-3 起步面；数据根沿 RIP_DATA_DIR 惯例：默认
/// rip-data/object-store，与 plugin-runtime paths.rs 的 env 优先级口径一致）。
#[derive(Debug, Clone)]
pub struct LocalFsStore {
    root: PathBuf,
}

impl LocalFsStore {
    /// 数据根：显式路径 &gt; RIP_OBJECT_STORE_DIR &gt; RIP_DATA_DIR/object-store &gt;
    /// ./rip-data/object-store（后者为仓库根运行口径）。
    pub fn from_env() -> Self {
        let root = std::env::var("RIP_OBJECT_STORE_DIR")
            .ok()
            .filter(|s| !s.trim().is_empty())
            .or_else(|| {
                std::env::var("RIP_DATA_DIR").ok().map(|d| {
                    PathBuf::from(d).join("object-store").to_string_lossy().to_string()
                })
            })
            .unwrap_or_else(|| "rip-data/object-store".to_string());
        Self { root: PathBuf::from(root) }
    }

    /// 显式根构造（测试用）。
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    fn path_for(&self, content_hash: &str) -> Result<PathBuf, IntelError> {
        if content_hash.len() != 64 || !content_hash.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(IntelError::Invalid(format!(
                "invalid content hash: {content_hash:?}"
            )));
        }
        Ok(object_path(&self.root, content_hash))
    }
}

impl ObjectStore for LocalFsStore {
    fn put(&self, content: &[u8]) -> IntelResult<StoredObject> {
        let content_hash = sha256_hex(content);
        let path = self.path_for(&content_hash)?;
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        if !path.exists() {
            // 原子写：tmp + rename（并发同 hash 时 rename 覆盖等价内容）。
            let tmp = path.with_extension(format!("tmp{}", std::process::id()));
            std::fs::write(&tmp, content)?;
            std::fs::rename(&tmp, &path)?;
        }
        Ok(StoredObject {
            content_hash: content_hash.clone(),
            size_bytes: content.len() as u64,
            storage_uri: format!("cas://{content_hash}"),
        })
    }

    fn read(&self, storage_uri: &str) -> IntelResult<Vec<u8>> {
        let hash = uri_to_hash(storage_uri)?;
        let path = self.path_for(hash)?;
        let content = std::fs::read(path)?;
        let actual = sha256_hex(&content);
        if actual != *hash {
            return Err(IntelError::Invalid(format!(
                "object integrity mismatch: expected {hash}, got {actual}"
            )));
        }
        Ok(content)
    }

    fn exists(&self, storage_uri: &str) -> IntelResult<bool> {
        let hash = uri_to_hash(storage_uri)?;
        Ok(self.path_for(hash)?.exists())
    }

    fn delete(&self, storage_uri: &str) -> IntelResult<()> {
        let hash = uri_to_hash(storage_uri)?;
        let path = self.path_for(hash)?;
        match std::fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn put_read_roundtrip_with_integrity() {
        let store = LocalFsStore::new(std::env::temp_dir().join(format!("os-{}", std::process::id())));
        let content = b"RIP3 CAS roundtrip test content";
        let stored = store.put(content).unwrap();
        assert!(stored.storage_uri.starts_with("cas://"));
        assert_eq!(stored.size_bytes, content.len() as u64);
        let back = store.read(&stored.storage_uri).unwrap();
        assert_eq!(back, content.to_vec());
        assert!(store.exists(&stored.storage_uri).unwrap());
        store.delete(&stored.storage_uri).unwrap();
        assert!(!store.exists(&stored.storage_uri).unwrap());
    }

    #[test]
    fn dedup_same_content_same_object() {
        let store = LocalFsStore::new(std::env::temp_dir().join(format!("os2-{}", std::process::id())));
        let a = store.put(b"same bytes").unwrap();
        let b = store.put(b"same bytes").unwrap();
        assert_eq!(a.content_hash, b.content_hash);
        assert_eq!(a.storage_uri, b.storage_uri);
    }

    #[test]
    fn invalid_uri_rejected() {
        let store = LocalFsStore::new(std::env::temp_dir().join(format!("os3-{}", std::process::id())));
        assert!(store.read("cas://short").is_err());
        assert!(store.read("file:///etc/passwd").is_err());
    }

    #[test]
    fn corruption_detected_on_read() {
        let store = LocalFsStore::new(std::env::temp_dir().join(format!("os4-{}", std::process::id())));
        let stored = store.put(b"original").unwrap();
        let hash = stored.storage_uri.strip_prefix("cas://").unwrap();
        let path = store.path_for(hash).unwrap();
        std::fs::write(&path, b"tampered").unwrap();
        assert!(store.read(&stored.storage_uri).is_err());
    }
}
