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
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// 默认分层 TTL：L1 目录 10 分钟（设计稿 §7.1）。
pub const TTL_CATALOG: Duration = Duration::from_secs(600);

/// 默认分层 TTL：L2 知识摘要 1 小时。
pub const TTL_KNOWLEDGE: Duration = Duration::from_secs(3600);

/// 默认分层 TTL：工具 Schema 24 小时。
pub const TTL_SCHEMA: Duration = Duration::from_secs(86400);

/// 默认分层 TTL：RTIS 检索结果 10 分钟。
pub const TTL_RTIS: Duration = Duration::from_secs(600);

/// 缓存未命中归因（CB-1 D5：validation 三元组整体比对拆解为逐字段归因，
/// 六词形与模块头注七类失效触发器映射表逐条对应——触发器 1 拆
/// Version/ContentHash 两词形，触发器 2/3/4/5/7 同落 FingerprintMismatch，
/// 触发器 6 = PolicyVersionMismatch，TTL 兜底 = TtlExpired）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum CacheMissReason {
    /// 键对应条目文件不存在或坏条目视同缺失（首次取数，无负缓存）。
    EntryMissing,
    /// validation.version 不一致（触发器 1 载体之一：工具清单 version 变化）。
    VersionMismatch,
    /// validation.content_hash 不一致（触发器 1 载体之一：digest 变化）。
    ContentHashMismatch,
    /// validation.source_fingerprint 不一致（触发器 2/3/4/5/7 载体：治理/
    /// 索引 schema/RTIS 库文件/插件状态/broker 分量变化）。
    FingerprintMismatch,
    /// 条目 policy_version 与当前值不一致（触发器 6：策略变化整体失效）。
    PolicyVersionMismatch,
    /// TTL 兜底过期（文件 mtime 距今超过 ttl）。
    TtlExpired,
}

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

/// 进程内归因快照（CB-1 D5①：六归因桶累计计数；命中不计入）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct CacheAttributionStats {
    /// EntryMissing 累计。
    pub entry_missing: u64,
    /// VersionMismatch 累计。
    pub version_mismatch: u64,
    /// ContentHashMismatch 累计。
    pub content_hash_mismatch: u64,
    /// FingerprintMismatch 累计。
    pub fingerprint_mismatch: u64,
    /// PolicyVersionMismatch 累计。
    pub policy_version_mismatch: u64,
    /// TtlExpired 累计。
    pub ttl_expired: u64,
}

/// 六归因桶（CB-1 D5①；std AtomicU64 零依赖，红线 3）。BrokerCache Clone
/// 共享同一 Arc 实例——跨源计数汇总面。
#[derive(Debug)]
struct AttributionBuckets {
    entry_missing: AtomicU64,
    version_mismatch: AtomicU64,
    content_hash_mismatch: AtomicU64,
    fingerprint_mismatch: AtomicU64,
    policy_version_mismatch: AtomicU64,
    ttl_expired: AtomicU64,
}

impl AttributionBuckets {
    const fn new() -> Self {
        Self {
            entry_missing: AtomicU64::new(0),
            version_mismatch: AtomicU64::new(0),
            content_hash_mismatch: AtomicU64::new(0),
            fingerprint_mismatch: AtomicU64::new(0),
            policy_version_mismatch: AtomicU64::new(0),
            ttl_expired: AtomicU64::new(0),
        }
    }

    fn bump(&self, reason: CacheMissReason) {
        let bucket = match reason {
            CacheMissReason::EntryMissing => &self.entry_missing,
            CacheMissReason::VersionMismatch => &self.version_mismatch,
            CacheMissReason::ContentHashMismatch => &self.content_hash_mismatch,
            CacheMissReason::FingerprintMismatch => &self.fingerprint_mismatch,
            CacheMissReason::PolicyVersionMismatch => &self.policy_version_mismatch,
            CacheMissReason::TtlExpired => &self.ttl_expired,
        };
        bucket.fetch_add(1, Ordering::Relaxed);
    }

    fn snapshot(&self) -> CacheAttributionStats {
        let load = |b: &AtomicU64| b.load(Ordering::Relaxed);
        CacheAttributionStats {
            entry_missing: load(&self.entry_missing),
            version_mismatch: load(&self.version_mismatch),
            content_hash_mismatch: load(&self.content_hash_mismatch),
            fingerprint_mismatch: load(&self.fingerprint_mismatch),
            policy_version_mismatch: load(&self.policy_version_mismatch),
            ttl_expired: load(&self.ttl_expired),
        }
    }
}

/// 目录级静态归因报告（CB-1 D5② cache-attrib 子命令数据面：磁盘条目静态
/// 扫描，与进程内 [`CacheAttributionStats`] 互补——跨进程重启可复算）。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CacheAttributionReport {
    /// 缓存目录。
    pub dir: PathBuf,
    /// 条目数（目录内全部 .json 文件，含坏条目）。
    pub entries: usize,
    /// 按 TTL 类过期计数（四分层 TTL 常量逐类比对 mtime；类间嵌套口径——
    /// 同一过期条目计入所有更短 TTL 类）。
    pub expired: TtlClassExpired,
    /// policy_version 与给定当前值不一致的条目数（坏条目不计）。
    pub policy_mismatch: usize,
}

/// 按 TTL 类过期计数（CB-1 D5②）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct TtlClassExpired {
    /// TTL_CATALOG 口径过期条目数。
    pub catalog: usize,
    /// TTL_KNOWLEDGE 口径过期条目数。
    pub knowledge: usize,
    /// TTL_SCHEMA 口径过期条目数。
    pub schema: usize,
    /// TTL_RTIS 口径过期条目数。
    pub rtis: usize,
}

/// 内容寻址缓存（目录句柄 + 归因桶句柄；读时校验）。
#[derive(Debug, Clone)]
pub struct BrokerCache {
    dir: PathBuf,
    attribution: Arc<AttributionBuckets>,
}

impl BrokerCache {
    /// 打开（建目录）。
    pub fn open(dir: PathBuf) -> Result<Self, ContextBrokerError> {
        std::fs::create_dir_all(&dir).map_err(|e| ContextBrokerError::Source {
            source: "broker-cache".to_string(),
            reason: format!("create dir {}: {e}", dir.display()),
        })?;
        Ok(Self {
            dir,
            attribution: Arc::new(AttributionBuckets::new()),
        })
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

    /// 读时校验本体（get / get_with_attribution 共用；逐字段归因拆解，
    /// 读取行为与既有 get 逐字节一致）。返回 (载荷, 未命中归因)；命中时
    /// reason 为 None。坏条目（解析失败）视同缺失（EntryMissing）。
    fn read_entry(
        &self,
        key: &str,
        expected: &CacheValidation,
        policy_version: &str,
        ttl: Duration,
    ) -> Result<(Option<String>, Option<CacheMissReason>), ContextBrokerError> {
        let path = self.path_for(key);
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Ok((None, Some(CacheMissReason::EntryMissing)))
            }
            Err(e) => {
                return Err(ContextBrokerError::Source {
                    source: "broker-cache".to_string(),
                    reason: format!("read {}: {e}", path.display()),
                })
            }
        };
        let entry: CacheEntry = match serde_json::from_str(&text) {
            Ok(entry) => entry,
            // 坏条目视同未命中（重建），不缓存失败态。
            Err(_) => return Ok((None, Some(CacheMissReason::EntryMissing))),
        };
        // CB-1 D5：整体比对拆解为逐字段归因（version → content_hash →
        // source_fingerprint → policy_version → TTL，先命中先归因）。
        if entry.validation.version != expected.version {
            return Ok((None, Some(CacheMissReason::VersionMismatch)));
        }
        if entry.validation.content_hash != expected.content_hash {
            return Ok((None, Some(CacheMissReason::ContentHashMismatch)));
        }
        if entry.validation.source_fingerprint != expected.source_fingerprint {
            return Ok((None, Some(CacheMissReason::FingerprintMismatch)));
        }
        if entry.policy_version != policy_version {
            return Ok((None, Some(CacheMissReason::PolicyVersionMismatch)));
        }
        // TTL 兜底：文件 mtime = 原子改名写入时刻（跨进程可靠）。
        if expired_at(&path, ttl) {
            return Ok((None, Some(CacheMissReason::TtlExpired)));
        }
        Ok((Some(entry.payload), None))
    }

    /// 读（读时校验：validation 三元组逐字段比对 + policy_version 比对 +
    /// TTL 兜底；任一不符即未命中返回 None，由调用方重建）。CB-1 D5：改
    /// 为 [`Self::read_entry`] 薄包装（签名与行为零 diff，丢弃归因且不
    /// 计入归因桶——既有调用方零副作用）。
    pub fn get(
        &self,
        key: &str,
        expected: &CacheValidation,
        policy_version: &str,
        ttl: Duration,
    ) -> Result<Option<String>, ContextBrokerError> {
        Ok(self.read_entry(key, expected, policy_version, ttl)?.0)
    }

    /// 读 + 未命中归因（CB-1 D5 additive API）：与 [`Self::get`] 同一读时
    /// 校验逻辑；未命中返回六词形 [`CacheMissReason`]（命中 reason 为
    /// None），且每次未命中计入进程内六归因桶（[`Self::attribution_stats`]
    /// 快照读取；[`Self::get`] 不计数——归因只在显式归因入口累计）。
    pub fn get_with_attribution(
        &self,
        key: &str,
        expected: &CacheValidation,
        policy_version: &str,
        ttl: Duration,
    ) -> Result<(Option<String>, Option<CacheMissReason>), ContextBrokerError> {
        let (payload, reason) = self.read_entry(key, expected, policy_version, ttl)?;
        if let Some(reason) = reason {
            self.attribution.bump(reason);
        }
        Ok((payload, reason))
    }

    /// 归因快照（CB-1 D5①：进程内六桶累计；Clone 共享同一实例）。
    pub fn attribution_stats(&self) -> CacheAttributionStats {
        self.attribution.snapshot()
    }

    /// 目录级静态归因报告（CB-1 D5② cache-attrib 数据面）：扫描目录内全部
    /// .json 条目，逐条目按四分层 TTL 常量比对 mtime 计过期（类间嵌套）+
    /// 比对 policy_version 计不匹配；坏条目计入 entries 但不参与归因。
    pub fn attribution_report(
        &self,
        policy_version: &str,
    ) -> Result<CacheAttributionReport, ContextBrokerError> {
        let reader = std::fs::read_dir(&self.dir).map_err(|e| ContextBrokerError::Source {
            source: "broker-cache".to_string(),
            reason: format!("read dir {}: {e}", self.dir.display()),
        })?;
        let mut entries = 0usize;
        let mut expired = TtlClassExpired {
            catalog: 0,
            knowledge: 0,
            schema: 0,
            rtis: 0,
        };
        let mut policy_mismatch = 0usize;
        for entry in reader {
            let path = match entry {
                Ok(entry) => entry.path(),
                Err(_) => continue,
            };
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            entries += 1;
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            let Ok(entry) = serde_json::from_str::<CacheEntry>(&text) else {
                continue; // 坏条目不可归因（计入 entries）
            };
            if entry.policy_version != policy_version {
                policy_mismatch += 1;
            }
            let age = match std::fs::metadata(&path).and_then(|m| m.modified()) {
                Ok(modified) => match modified.duration_since(UNIX_EPOCH) {
                    Ok(base) => SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap_or_default()
                        .saturating_sub(base),
                    Err(_) => Duration::MAX,
                },
                Err(_) => Duration::MAX, // 元数据不可得视同过期
            };
            if age > TTL_CATALOG {
                expired.catalog += 1;
            }
            if age > TTL_KNOWLEDGE {
                expired.knowledge += 1;
            }
            if age > TTL_SCHEMA {
                expired.schema += 1;
            }
            if age > TTL_RTIS {
                expired.rtis += 1;
            }
        }
        Ok(CacheAttributionReport {
            dir: self.dir.clone(),
            entries,
            expired,
            policy_mismatch,
        })
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
