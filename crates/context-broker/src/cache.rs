//! 内容寻址缓存（设计稿 CONTEXT-BROKER-DESIGN §7；worker runtime 结果缓存
//! 先例的同构 Rust 落面：`workers/.cache/<sha256>.json` 原子写，worker_
//! runtime.py:587-591 的 tmp + rename 口径）。
//!
//! 纪律（设计稿 §7.1/§7.3）：
//! - 键 = `sha256(kind, id, version, params, source_fingerprint)`，内容寻址；
//! - 条目含 `validation`（version / content_hash / source_fingerprint），
//!   读时逐字段比对，不一致即未命中重建；TTL 只是兜底（分层默认见
//!   [`TTL_CATALOG`] 等，调用方传入）；
//! - 目录可配置不硬编码：默认 `<rip-data>/runtime/broker-cache/`（env
//!   `RIP_DATA_DIR`，缺省回落临时目录），先例与可配置要求来自计划:405-409；
//! - 失败不落缓存、不做负缓存：[`BrokerCache::put`] 只在取数成功后由调用
//!   方写入，API 面不存在失败条目；
//! - 只加速「读源」与「摘要生成」，不缓存装配结果本身（§7.3）；历史
//!   Manifest 不受缓存失效影响（不可变清单）。
//!
//! 七类失效触发器（设计稿 §7.2 表）到 validation 字段的映射约定（调用方
//! 组装 fingerprint 时逐条并入，本模块提供比对执行）：
//!
//! | 触发源 | 载体 |
//! |---|---|
//! | 工具清单 version/digest 变化 | validation.version + content_hash |
//! | 知识治理跃迁 | source_fingerprint 内 governance 分量 |
//! | 检索索引重建（INDEX_SCHEMA_VERSION） | source_fingerprint 内 index_schema 分量 |
//! | RTIS knowledge.db mtime+size 变化 | source_fingerprint 内 rtis_db 分量 |
//! | 插件禁用/卸载 | source_fingerprint 内 plugin 分量 |
//! | 策略变化 | 条目 policy_version 与当前值比对 |
//! | 平台版本变化 | source_fingerprint 内 broker 分量（BROKER_VERSION） |

use crate::error::ContextBrokerError;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// 默认分层 TTL：L1 目录 10 分钟（设计稿 §7.1）。
pub const TTL_CATALOG: Duration = Duration::from_secs(600);

/// 默认分层 TTL：L2 知识摘要 1 小时。
pub const TTL_KNOWLEDGE: Duration = Duration::from_secs(3600);

/// 默认分层 TTL：工具 Schema 24 小时。
pub const TTL_SCHEMA: Duration = Duration::from_secs(86400);

/// 默认分层 TTL：RTIS 检索结果 10 分钟。
pub const TTL_RTIS: Duration = Duration::from_secs(600);

/// 读时校验三元组（设计稿 §7.1 条目 validation 字段）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CacheValidation {
    /// 版本信号（工具 version / 引用 version；缺省口径 current）。
    pub version: String,
    /// 内容哈希（sha256 hex；缺证空串）。
    pub content_hash: String,
    /// 源指纹（治理/索引/库文件/插件/broker 分量的组合串，见模块注映射表）。
    pub source_fingerprint: String,
}

/// 缓存条目（磁盘 JSON 形状）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CacheEntry {
    /// 内容寻址键（sha256 hex）。
    pub key: String,
    /// 条目种类（brief / load / rtis-search 等）。
    pub kind: String,
    /// 读时校验三元组。
    pub validation: CacheValidation,
    /// 载荷（生成的 brief 文本 / 源 load 正文等）。
    pub payload: String,
    /// 写入时刻（ISO8601 UTC）。
    pub created_at: String,
    /// 预算策略版本（策略变化即整体失效，设计稿 §7.2）。
    pub policy_version: String,
}

/// 缓存统计（cache-stats 子命令数据面）。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CacheStats {
    /// 缓存目录。
    pub dir: PathBuf,
    /// 条目数。
    pub entries: usize,
    /// 总字节数。
    pub bytes: u64,
}

/// 内容寻址缓存（目录句柄；无进程内状态，读时校验）。
#[derive(Debug, Clone)]
pub struct BrokerCache {
    dir: PathBuf,
}

impl BrokerCache {
    /// 打开（建目录）。
    pub fn open(dir: PathBuf) -> Result<Self, ContextBrokerError> {
        std::fs::create_dir_all(&dir).map_err(|e| ContextBrokerError::Source {
            source: "broker-cache".to_string(),
            reason: format!("create dir {}: {e}", dir.display()),
        })?;
        Ok(Self { dir })
    }

    /// 默认目录：`$RIP_DATA_DIR/runtime/broker-cache`，env 缺省回落系统
    /// 临时目录（不硬编码宿主路径，计划:405-409 可配置口径）。
    pub fn default_dir() -> PathBuf {
        match std::env::var("RIP_DATA_DIR") {
            Ok(data_dir) if !data_dir.trim().is_empty() => {
                Path::new(&data_dir).join("runtime").join("broker-cache")
            }
            _ => std::env::temp_dir()
                .join("rip-data")
                .join("runtime")
                .join("broker-cache"),
        }
    }

    /// 内容寻址键：sha256(kind, id, version, params, source_fingerprint)。
    pub fn cache_key(
        kind: &str,
        id: &str,
        version: &str,
        params: &str,
        source_fingerprint: &str,
    ) -> String {
        let material =
            format!("{kind}\u{1f}{id}\u{1f}{version}\u{1f}{params}\u{1f}{source_fingerprint}");
        tool_registry::hash::sha256_hex(material.as_bytes())
    }

    fn path_for(&self, key: &str) -> PathBuf {
        self.dir.join(format!("{key}.json"))
    }

    /// 写入（原子：tmp + rename，worker runtime 先例；仅成功取数后调用，
    /// 失败不落缓存、无负缓存条目）。返回内容寻址键。
    pub fn put(
        &self,
        kind: &str,
        id: &str,
        validation: &CacheValidation,
        payload: &str,
        policy_version: &str,
    ) -> Result<String, ContextBrokerError> {
        let key = Self::cache_key(
            kind,
            id,
            &validation.version,
            "v1",
            &validation.source_fingerprint,
        );
        let entry = CacheEntry {
            key: key.clone(),
            kind: kind.to_string(),
            validation: validation.clone(),
            payload: payload.to_string(),
            created_at: domain::time::now_iso(),
            policy_version: policy_version.to_string(),
        };
        let path = self.path_for(&key);
        let tmp = path.with_extension("json.tmp");
        let text = serde_json::to_string(&entry).map_err(|e| ContextBrokerError::Source {
            source: "broker-cache".to_string(),
            reason: format!("serialize cache entry: {e}"),
        })?;
        std::fs::write(&tmp, text).map_err(|e| ContextBrokerError::Source {
            source: "broker-cache".to_string(),
            reason: format!("write {}: {e}", tmp.display()),
        })?;
        std::fs::rename(&tmp, &path).map_err(|e| ContextBrokerError::Source {
            source: "broker-cache".to_string(),
            reason: format!("atomic rename {} -> {}: {e}", tmp.display(), path.display()),
        })?;
        Ok(key)
    }

    /// 读（读时校验：validation 三元组逐字段比对 + policy_version 比对 +
    /// TTL 兜底；任一不符即未命中返回 None，由调用方重建）。
    pub fn get(
        &self,
        key: &str,
        expected: &CacheValidation,
        policy_version: &str,
        ttl: Duration,
    ) -> Result<Option<String>, ContextBrokerError> {
        let path = self.path_for(key);
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => {
                return Err(ContextBrokerError::Source {
                    source: "broker-cache".to_string(),
                    reason: format!("read {}: {e}", path.display()),
                })
            }
        };
        let entry: CacheEntry = match serde_json::from_str(&text) {
            Ok(entry) => entry,
            Err(_) => return Ok(None), // 坏条目视同未命中（重建），不缓存失败态
        };
        if entry.validation != *expected || entry.policy_version != policy_version {
            return Ok(None);
        }
        // TTL 兜底：文件 mtime = 原子改名写入时刻（跨进程可靠）。
        if expired_at(&path, ttl) {
            return Ok(None);
        }
        Ok(Some(entry.payload))
    }

    /// 统计（cache-stats 数据面）。
    pub fn stats(&self) -> Result<CacheStats, ContextBrokerError> {
        let reader = std::fs::read_dir(&self.dir).map_err(|e| ContextBrokerError::Source {
            source: "broker-cache".to_string(),
            reason: format!("read dir {}: {e}", self.dir.display()),
        })?;
        let mut entries = 0usize;
        let mut bytes = 0u64;
        for entry in reader {
            let path = match entry {
                Ok(entry) => entry.path(),
                Err(_) => continue,
            };
            if path.extension().and_then(|e| e.to_str()) == Some("json") {
                entries += 1;
                bytes += std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
            }
        }
        Ok(CacheStats {
            dir: self.dir.clone(),
            entries,
            bytes,
        })
    }
}

/// TTL 兜底判定（文件 mtime 距今超过 ttl 即过期；put 原子改名后 mtime 即
/// 写入时刻，跨进程可靠；元数据不可得视同过期重建）。
fn expired_at(path: &Path, ttl: Duration) -> bool {
    match std::fs::metadata(path).and_then(|m| m.modified()) {
        Ok(modified) => match modified.duration_since(UNIX_EPOCH) {
            Ok(age_base) => {
                let now = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default();
                now.saturating_sub(age_base) > ttl
            }
            Err(_) => true,
        },
        Err(_) => true,
    }
}
