//! CB-1 B2 缓存失效归因验收（docs/rip3/CB-1-TASK.md §0 D5 / §4 判据 7）：
//!
//! 1. 六类 CacheMissReason 各有触发单测，七类失效触发器映射表
//!    （cache.rs 模块头注）逐条对得上：触发器 1 拆 version/content_hash 两
//!    词形，触发器 2/3/4/5/7 同落 FingerprintMismatch，触发器 6 =
//!    PolicyVersionMismatch，TTL 兜底 = TtlExpired。
//! 2. 既有 get/put 行为零 diff：get 薄包装不进归因桶（零副作用负例）。
//! 3. 归因桶 Arc 共享跨源（Clone 共享同一计数实例）。
//! 4. cache-attrib 目录级静态归因报告（entries / 按 TTL 类过期计数 /
//!    policy_version 不匹配计数）。

use context_broker::{
    BrokerCache, CacheAttributionStats, CacheValidation, TtlClassExpired, TTL_RTIS, TTL_SCHEMA,
};
use std::path::PathBuf;
use std::time::Duration;
use std::time::SystemTime;

fn temp_cache(tag: &str) -> (BrokerCache, PathBuf) {
    let dir = std::env::temp_dir().join(format!("rip-cb1-attrib-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let cache = BrokerCache::open(dir.clone()).expect("open cache");
    (cache, dir)
}

fn validation(version: &str, content_hash: &str, fingerprint: &str) -> CacheValidation {
    CacheValidation {
        version: version.to_string(),
        content_hash: content_hash.to_string(),
        source_fingerprint: fingerprint.to_string(),
    }
}

fn put_base(cache: &BrokerCache, tag: &str, fp: &str) -> String {
    cache
        .put(
            "load",
            &format!("kb:{tag}"),
            &validation("1.0.0", "bb".repeat(32).as_str(), fp),
            "payload",
            "policy-1",
        )
        .expect("put base entry")
}

fn zero_stats() -> CacheAttributionStats {
    CacheAttributionStats {
        entry_missing: 0,
        version_mismatch: 0,
        content_hash_mismatch: 0,
        fingerprint_mismatch: 0,
        policy_version_mismatch: 0,
        ttl_expired: 0,
    }
}

/// EntryMissing：未知键未命中（首次取数，无负缓存条目产生）。
#[test]
fn cb1_miss_reason_entry_missing() {
    let (cache, dir) = temp_cache("entry-missing");
    let (payload, reason) = cache
        .get_with_attribution(
            "nonexistent",
            &validation("1", "", ""),
            "policy-1",
            TTL_SCHEMA,
        )
        .unwrap();
    assert!(payload.is_none());
    assert_eq!(reason, Some(context_broker::CacheMissReason::EntryMissing));
    let stats = cache.attribution_stats();
    assert_eq!(stats.entry_missing, 1);
    assert_eq!(
        stats,
        CacheAttributionStats {
            entry_missing: 1,
            ..zero_stats()
        },
        "其余桶零计数"
    );
    assert_eq!(cache.stats().unwrap().entries, 0, "无负缓存条目产生");
    let _ = std::fs::remove_dir_all(&dir);
}

/// 触发器 1（载体一）：工具清单 version 变化 → VersionMismatch。
#[test]
fn cb1_miss_reason_version_mismatch_trigger1() {
    let (cache, dir) = temp_cache("version");
    let fp = "broker=0.1.0;registry";
    let key = put_base(&cache, "version", fp);
    let (payload, reason) = cache
        .get_with_attribution(
            &key,
            &validation("2.0.0", "bb".repeat(32).as_str(), fp),
            "policy-1",
            TTL_SCHEMA,
        )
        .unwrap();
    assert!(payload.is_none());
    assert_eq!(
        reason,
        Some(context_broker::CacheMissReason::VersionMismatch)
    );
    assert_eq!(cache.attribution_stats().version_mismatch, 1);
    let _ = std::fs::remove_dir_all(&dir);
}

/// 触发器 1（载体二）：digest（content_hash）变化 → ContentHashMismatch。
#[test]
fn cb1_miss_reason_content_hash_mismatch_trigger1() {
    let (cache, dir) = temp_cache("content-hash");
    let fp = "broker=0.1.0;registry";
    let key = put_base(&cache, "content-hash", fp);
    let (payload, reason) = cache
        .get_with_attribution(
            &key,
            &validation("1.0.0", "cc".repeat(32).as_str(), fp),
            "policy-1",
            TTL_SCHEMA,
        )
        .unwrap();
    assert!(payload.is_none());
    assert_eq!(
        reason,
        Some(context_broker::CacheMissReason::ContentHashMismatch)
    );
    assert_eq!(cache.attribution_stats().content_hash_mismatch, 1);
    let _ = std::fs::remove_dir_all(&dir);
}

/// 触发器 2/3/4/5/7：治理跃迁 / 索引 schema / RTIS 库文件 / 插件状态 /
/// broker 分量变化——五分量同落 FingerprintMismatch。
#[test]
fn cb1_miss_reason_fingerprint_mismatch_triggers_2_3_4_5_7() {
    let (cache, dir) = temp_cache("fingerprint");
    let key = put_base(&cache, "fingerprint", "broker=0.1.0");
    let variants = [
        // 触发器 2：知识治理跃迁。
        "broker=0.1.0;governance=QUARANTINED",
        // 触发器 3：检索索引重建（INDEX_SCHEMA_VERSION）。
        "broker=0.1.0;index_schema=2",
        // 触发器 4：RTIS knowledge.db mtime+size 变化。
        "broker=0.1.0;rtis_db=1731290000:28301100",
        // 触发器 5：插件禁用/卸载。
        "broker=0.1.0;plugin=DISABLED",
        // 触发器 7：平台版本变化（BROKER_VERSION）。
        "broker=0.2.0",
    ];
    for (idx, fp) in variants.iter().enumerate() {
        let (payload, reason) = cache
            .get_with_attribution(
                &key,
                &validation("1.0.0", "bb".repeat(32).as_str(), fp),
                "policy-1",
                TTL_SCHEMA,
            )
            .unwrap();
        assert!(payload.is_none(), "variant {idx} payload");
        assert_eq!(
            reason,
            Some(context_broker::CacheMissReason::FingerprintMismatch),
            "variant {idx} 归因"
        );
    }
    assert_eq!(cache.attribution_stats().fingerprint_mismatch, 5);
    let _ = std::fs::remove_dir_all(&dir);
}

/// 触发器 6：策略变化 → PolicyVersionMismatch（整体失效）。
#[test]
fn cb1_miss_reason_policy_version_mismatch_trigger6() {
    let (cache, dir) = temp_cache("policy");
    let key = put_base(&cache, "policy", "broker=0.1.0");
    let (payload, reason) = cache
        .get_with_attribution(
            &key,
            &validation("1.0.0", "bb".repeat(32).as_str(), "broker=0.1.0"),
            "policy-2",
            TTL_SCHEMA,
        )
        .unwrap();
    assert!(payload.is_none());
    assert_eq!(
        reason,
        Some(context_broker::CacheMissReason::PolicyVersionMismatch)
    );
    assert_eq!(cache.attribution_stats().policy_version_mismatch, 1);
    let _ = std::fs::remove_dir_all(&dir);
}

/// TTL 兜底过期 → TtlExpired（mtime 距今超过 ttl；ttl=0 即任何已写入
/// 条目过期）。
#[test]
fn cb1_miss_reason_ttl_expired() {
    let (cache, dir) = temp_cache("ttl");
    let key = put_base(&cache, "ttl", "broker=0.1.0");
    std::thread::sleep(Duration::from_millis(5));
    let (payload, reason) = cache
        .get_with_attribution(
            &key,
            &validation("1.0.0", "bb".repeat(32).as_str(), "broker=0.1.0"),
            "policy-1",
            Duration::ZERO,
        )
        .unwrap();
    assert!(payload.is_none());
    assert_eq!(reason, Some(context_broker::CacheMissReason::TtlExpired));
    assert_eq!(cache.attribution_stats().ttl_expired, 1);

    // 同条目 TTL 宽松口径（TTL_RTIS 600s）：命中，reason 无且零计数。
    let (payload_hit, reason_hit) = cache
        .get_with_attribution(
            &key,
            &validation("1.0.0", "bb".repeat(32).as_str(), "broker=0.1.0"),
            "policy-1",
            TTL_RTIS,
        )
        .unwrap();
    assert_eq!(payload_hit.as_deref(), Some("payload"));
    assert_eq!(reason_hit, None, "命中无 miss 归因");
    assert_eq!(cache.attribution_stats().ttl_expired, 1, "命中不计数");
    let _ = std::fs::remove_dir_all(&dir);
}

/// 既有 get 行为零 diff 负例：get 未命中不进归因桶（薄包装零副作用）。
#[test]
fn cb1_plain_get_has_no_bucket_side_effect() {
    let (cache, dir) = temp_cache("plain-get");
    let key = put_base(&cache, "plain-get", "broker=0.1.0");
    let before = cache.attribution_stats();
    assert_eq!(before, zero_stats());

    // get 版本不匹配未命中：返回 None 但桶计数不动。
    let miss = cache
        .get(
            &key,
            &validation("9.9.9", "bb".repeat(32).as_str(), "broker=0.1.0"),
            "policy-1",
            TTL_SCHEMA,
        )
        .unwrap();
    assert!(miss.is_none());
    assert_eq!(cache.attribution_stats(), before, "get 零归因副作用");

    // get 命中：行为与既有一致（载荷返回），桶计数仍不动。
    let hit = cache
        .get(
            &key,
            &validation("1.0.0", "bb".repeat(32).as_str(), "broker=0.1.0"),
            "policy-1",
            TTL_SCHEMA,
        )
        .unwrap();
    assert_eq!(hit.as_deref(), Some("payload"));
    assert_eq!(cache.attribution_stats(), before, "get 命中亦零计数");

    // 归因只在显式 get_with_attribution 入口累计。
    let _ = cache
        .get_with_attribution(
            &key,
            &validation("9.9.9", "bb".repeat(32).as_str(), "broker=0.1.0"),
            "policy-1",
            TTL_SCHEMA,
        )
        .unwrap();
    assert_eq!(
        cache.attribution_stats().version_mismatch,
        1,
        "get_with_attribution 计数"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// 归因桶 Arc 共享跨源（BrokerCache Clone 共享同一计数实例，D5①）。
#[test]
fn cb1_attribution_buckets_shared_across_clone() {
    let (cache, dir) = temp_cache("clone-share");
    let clone = cache.clone();
    let _ = put_base(&cache, "clone-share", "broker=0.1.0");
    let (_, reason) = clone
        .get_with_attribution(
            "nonexistent-clone",
            &validation("1", "", ""),
            "policy-1",
            TTL_SCHEMA,
        )
        .unwrap();
    assert_eq!(reason, Some(context_broker::CacheMissReason::EntryMissing));
    assert_eq!(
        cache.attribution_stats().entry_missing,
        1,
        "clone 侧未命中计入原句柄快照（同一桶实例）"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// cache-attrib 目录级静态归因报告：entries / 按 TTL 类过期计数 /
/// policy_version 不匹配计数；坏条目计入 entries 但不参与归因。
#[test]
fn cb1_attribution_report_static_directory_scan() {
    let (cache, dir) = temp_cache("report");
    let fp = "broker=0.1.0";
    let key_a = put_base(&cache, "report-a", fp);
    let _key_b = put_base(&cache, "report-b", fp);

    // 新写条目：全 TTL 类不过期，policy 全匹配。
    let report = cache.attribution_report("policy-1").unwrap();
    assert_eq!(report.entries, 2);
    assert_eq!(
        report.expired,
        TtlClassExpired {
            catalog: 0,
            knowledge: 0,
            schema: 0,
            rtis: 0
        }
    );
    assert_eq!(report.policy_mismatch, 0);

    // policy_version 不匹配口径（当前值 policy-2）。
    let report = cache.attribution_report("policy-2").unwrap();
    assert_eq!(report.policy_mismatch, 2);

    // 坏条目：计入 entries，不参与归因；真实键仍可 get 命中（静态报告
    // 不动条目本体）。
    std::fs::write(dir.join("broken.json"), "not json").unwrap();
    let report = cache.attribution_report("policy-1").unwrap();
    assert_eq!(report.entries, 3);
    assert_eq!(report.policy_mismatch, 0);
    let hit = cache
        .get(
            &key_a,
            &validation("1.0.0", "bb".repeat(32).as_str(), fp),
            "policy-1",
            TTL_SCHEMA,
        )
        .unwrap();
    assert_eq!(hit.as_deref(), Some("payload"));

    // 过期口径嵌套自检（真实回溯 mtime）：条目 A 回溯 2 小时（> TTL_RTIS
    // 600s 与 TTL_KNOWLEDGE 3600s，< TTL_SCHEMA 86400s），条目 B 回溯 30 天
    // （四类全过期）。类间嵌套口径 = 同一过期条目计入所有更短 TTL 类。
    assert!(TTL_RTIS <= Duration::from_secs(600));
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap();
    let backdate = |key: &str, secs: u64| {
        let f = std::fs::OpenOptions::new()
            .write(true)
            .open(dir.join(format!("{key}.json")))
            .unwrap();
        f.set_modified(SystemTime::UNIX_EPOCH + Duration::from_secs(now.as_secs() - secs))
            .unwrap();
    };
    backdate(&key_a, 2 * 3600);
    backdate(&_key_b, 30 * 86400);
    let report = cache.attribution_report("policy-1").unwrap();
    assert_eq!(report.entries, 3, "坏条目仍计入 entries");
    assert_eq!(
        report.expired,
        TtlClassExpired {
            catalog: 2,
            knowledge: 2,
            schema: 1,
            rtis: 2
        },
        "2h 条目过 catalog/knowledge/rtis 三类，30d 条目四类全过"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
