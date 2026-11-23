//! 检索面缓存（RIP3 P4-3，R2 6.6 Query Cache；context-broker cache.rs:117-207
//! 内容寻址骨架的同构 Rust 落面——cache.rs 在 context-broker crate 内，intel-db
//! 不依赖该 crate，同构移植而非跨 crate 引用）。
//!
//! 两个缓存共用一套条目形状。查询向量缓存 kind="query-vector"：key =
//! sha256(模型指纹 ‖ query)，payload = pgvector 向量 JSON 数组；命中即跳过
//! rip.embed.text 容器派发（秒级推理 → 磁盘读）；validation.version = 嵌入
//! 模型指纹（manifest_dir 路径 + 模型 id，模型更换即整体失效）。
//!
//! 检索结果缓存 kind="retrieval"：key = sha256(mode ‖ filters ‖ query ‖
//! limit)，payload = Vec<SearchHit> JSON；validation.version = 检索面版本，
//! source_fingerprint = 索引 schema 版本 + workspace 治理态（APPROVED 卡数 +
//! max(updated_at)——review 跃迁/建卡即失效，cache.rs:17-28 七类触发器的
//! index_schema 与 governance 分量映射）。
//!
//! 纪律（沿 cache.rs 同构先例）：失败不落缓存、无负缓存；读时校验三元组 +
//! TTL mtime 兜底；原子写 tmp + rename；坏条目视同未命中重建。

use crate::{IntelError, IntelResult};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// 查询向量缓存默认 TTL（1h；模型指纹不变则稳定）。
pub const TTL_QUERY_VECTOR: Duration = Duration::from_secs(3600);
/// 检索结果缓存默认 TTL（10min；沿 TTL_RTIS 先例）。
pub const TTL_RETRIEVAL: Duration = Duration::from_secs(600);

/// 默认目录：`$RIP_DATA_DIR/runtime/search-cache`（env 缺省 rip-data）。
pub fn default_dir() -> PathBuf {
    let data_root = std::env::var("RIP_DATA_DIR")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "rip-data".to_string());
    PathBuf::from(data_root)
        .join("runtime")
        .join("search-cache")
}

/// 内容寻址键（cache.rs cache_key 同构：sha256(kind ‖ id ‖ version ‖ params
/// ‖ fingerprint)，\u{1f} 分隔）。
pub fn cache_key(kind: &str, id: &str, version: &str, params: &str, fingerprint: &str) -> String {
    let material = format!("{kind}\u{1f}{id}\u{1f}{version}\u{1f}{params}\u{1f}{fingerprint}");
    tool_registry::hash::sha256_hex(material.as_bytes())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CacheEntry {
    version: String,
    fingerprint: String,
    payload: String,
}

/// 检索面缓存句柄（目录句柄，无进程内状态）。
pub struct SearchCache {
    dir: PathBuf,
}

impl SearchCache {
    /// 打开（建目录；失败即返回 Err，调用方决定是否禁用缓存降级直查）。
    pub fn open(dir: PathBuf) -> IntelResult<Self> {
        std::fs::create_dir_all(&dir).map_err(|e| {
            IntelError::Invalid(format!("search-cache create dir {}: {e}", dir.display()))
        })?;
        Ok(Self { dir })
    }

    /// 缺省目录打开。
    pub fn open_default() -> IntelResult<Self> {
        Self::open(default_dir())
    }

    fn path_for(&self, key: &str) -> PathBuf {
        self.dir.join(format!("{key}.json"))
    }

    /// 读（validation.version + fingerprint 逐字段比对 + TTL mtime 兜底）。
    pub fn get(
        &self,
        key: &str,
        version: &str,
        fingerprint: &str,
        ttl: Duration,
    ) -> Option<String> {
        let path = self.path_for(key);
        let text = std::fs::read_to_string(&path).ok()?;
        let entry: CacheEntry = serde_json::from_str(&text).ok()?; // 坏条目视同未命中
        if entry.version != version || entry.fingerprint != fingerprint {
            return None;
        }
        if expired_at(&path, ttl) {
            return None;
        }
        Some(entry.payload)
    }

    /// 写（原子 tmp + rename；仅成功取数后调用——失败不落缓存、无负缓存）。
    pub fn put(
        &self,
        key: &str,
        version: &str,
        fingerprint: &str,
        payload: &str,
    ) -> IntelResult<()> {
        let path = self.path_for(key);
        let tmp = path.with_extension("json.tmp");
        let entry = CacheEntry {
            version: version.to_string(),
            fingerprint: fingerprint.to_string(),
            payload: payload.to_string(),
        };
        let text = serde_json::to_string(&entry)
            .map_err(|e| IntelError::Invalid(format!("search-cache serialize: {e}")))?;
        std::fs::write(&tmp, text).map_err(|e| {
            IntelError::Invalid(format!("search-cache write {}: {e}", tmp.display()))
        })?;
        std::fs::rename(&tmp, &path).map_err(|e| {
            IntelError::Invalid(format!(
                "search-cache rename {} -> {}: {e}",
                tmp.display(),
                path.display()
            ))
        })?;
        Ok(())
    }
}

/// TTL 兜底判定（mtime 距今超 ttl 即过期；元数据不可得视同过期——cache.rs
/// expired_at 同构）。
fn expired_at(path: &Path, ttl: Duration) -> bool {
    match std::fs::metadata(path).and_then(|m| m.modified()) {
        Ok(modified) => match modified.duration_since(std::time::UNIX_EPOCH) {
            Ok(age_base) => {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default();
                now.saturating_sub(age_base) > ttl
            }
            Err(_) => true,
        },
        Err(_) => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_roundtrip_and_invalidations() {
        let dir =
            std::env::temp_dir().join(format!("rip-search-cache-test-{}", std::process::id()));
        let cache = SearchCache::open(dir.clone()).unwrap();
        let key = cache_key("query-vector", "ws-x", "embed-v1", "SQL 注入", "fp-a");
        cache
            .put(&key, "embed-v1", "fp-a", "[0.1,0.2,0.3]")
            .unwrap();
        assert_eq!(
            cache
                .get(&key, "embed-v1", "fp-a", TTL_QUERY_VECTOR)
                .as_deref(),
            Some("[0.1,0.2,0.3]")
        );
        // 指纹变化 = 未命中（模型更换/治理跃迁失效语义）。
        assert!(cache
            .get(&key, "embed-v1", "fp-b", TTL_QUERY_VECTOR)
            .is_none());
        assert!(cache
            .get(&key, "embed-v2", "fp-a", TTL_QUERY_VECTOR)
            .is_none());
        // TTL 过期 = 未命中。
        assert!(cache
            .get(&key, "embed-v1", "fp-a", Duration::from_secs(0))
            .is_none());
        // 坏条目视同未命中（不 panic）。
        let bad = cache.path_for(&key);
        std::fs::write(&bad, "not-json").unwrap();
        assert!(cache
            .get(&key, "embed-v1", "fp-a", TTL_QUERY_VECTOR)
            .is_none());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn key_differs_by_kind_and_params() {
        let a = cache_key("query-vector", "w", "v1", "q", "f");
        let b = cache_key("retrieval", "w", "v1", "q", "f");
        let c = cache_key("query-vector", "w", "v1", "q2", "f");
        assert_ne!(a, b);
        assert_ne!(a, c);
    }
}
