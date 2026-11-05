//! M4：search-engine 与 eri_p2.resolve 行为对照（同一批关键词，双轨跑）。
//!
//! 判定口径：
//! - 硬断言（双方命中非空、top-5 集合交集非空）沿用 M2 parity 纪律；
//!   top-1 一致性按任务口径记录不作为硬断言（权重不同允许顺序差异）。
//! - **M2 差异修复回归断言**：M2_REPORT 差异表 #2 指出 rust 侧缺 topics 数据源
//!   导致 fuzzing 组漏召回（awesome-rtc-hacking、rfsec-toolkit 经 topics 命中）。
//!   M4 索引从 catalog github_metadata 折叠 topics 后必须召回这两条
//!   （hard：若 catalog topics 数据漂移导致失败，即为真实的 parity 回归信号）。
//! - CJK 桥接组（渗透 -> CJK_BRIDGE pentest）验证中文查询的跨语言对齐：
//!   双方非空、top-1 一致、top-5 交集非空（FTS 与 LIKE 的召回面差异允许
//!   集合不完全相等）。

use rp_eri_parity::{
    build_search_index, fresh_tmp_dir, parity_prereq, run_eri_resolve, run_rust_search,
    snapshot_catalog, KEYWORD_GROUPS, LIMIT, REQUIRED_HIT_FIELDS,
};
use serde_json::Value;

/// M2 报告差异表 #2 的两条 topics-only 召回资源（fuzzing 组）。
const M2_TOPICS_RECALL: [&str; 2] = [
    "gh:enablesecurity/awesome-rtc-hacking",
    "gh:cn0xroot/rfsec-toolkit",
];

fn eri_top_ids(v: &Value) -> Vec<String> {
    v["matched_resources"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|h| h["resource_id"].as_str())
                .map(String::from)
                .collect()
        })
        .unwrap_or_default()
}

fn overlap(a: &[String], b: &[String]) -> Vec<String> {
    a.iter().filter(|x| b.contains(x)).cloned().collect()
}

/// 单组对照（ASCII 组），返回报告行。
fn compare_group(label: &str, keywords: &[&str]) {
    match parity_prereq() {
        Ok(()) => {}
        Err(reason) => {
            eprintln!("SKIP m4-parity[{label}]: {reason}");
            return;
        }
    }
    let tmp = fresh_tmp_dir(&format!("m4-{label}"));
    let catalog_copy = snapshot_catalog(&tmp);
    let eri_json = run_eri_resolve(&catalog_copy, keywords);
    let index_db = build_search_index(&tmp);
    let rust_json = run_rust_search(&index_db, keywords);

    // 双方非空
    let eri_ids = eri_top_ids(&eri_json);
    let rust_ids: Vec<String> = rust_json
        .hits
        .iter()
        .map(|h| h.resource_id.clone())
        .collect();
    assert!(
        !eri_ids.is_empty(),
        "eri returned no hits for {keywords:?}: {}",
        eri_json["search_status"]
    );
    assert!(
        !rust_ids.is_empty(),
        "rust search returned no hits for {keywords:?}: {}",
        rust_json.search_status
    );

    // eri 命中元素必备字段（形状基准面，§8.4）
    let hits = eri_json["matched_resources"].as_array().unwrap();
    for (i, h) in hits.iter().enumerate() {
        for f in REQUIRED_HIT_FIELDS {
            assert!(
                h.get(f).is_some(),
                "eri hit[{i}] missing required field {f}"
            );
        }
    }

    // 核心命中集合重叠（硬断言）
    let common = overlap(&eri_ids, &rust_ids);
    assert!(
        !common.is_empty(),
        "top-{LIMIT} hit sets do not overlap for {keywords:?}: eri={eri_ids:?} rust={rust_ids:?}"
    );

    let top1_match = eri_ids.first() == rust_ids.first();
    println!(
        "M4-PARITY [{label}] kw={keywords:?} eri_top1={} rust_top1={} top1_match={} \
         overlap={}/{} eri_set={eri_ids:?} rust_set={rust_ids:?}",
        eri_ids.first().map(String::as_str).unwrap_or(""),
        rust_ids.first().map(String::as_str).unwrap_or(""),
        top1_match,
        common.len(),
        LIMIT,
    );

    // M2 差异修复回归：topics-only 召回资源必须在 rust 命中集中（hard）
    if label == "fuzzing" {
        for rid in M2_TOPICS_RECALL {
            assert!(
                rust_ids.iter().any(|x| x == rid),
                "M2 topics-recall regression: {rid} missing from rust search hits \
                 (catalog github_metadata.topics folded index required): rust={rust_ids:?}"
            );
        }
    }

    // 对照报告片段（构建产物目录，不入库）
    let report = serde_json::json!({
        "group": label,
        "keywords": keywords,
        "engine": "search-engine",
        "eri_top1": eri_ids.first(),
        "rust_top1": rust_ids.first(),
        "top1_match": top1_match,
        "overlap": common,
        "eri_top5": eri_ids,
        "rust_top5": rust_ids,
        "rust_scores": rust_json.hits.iter().map(|h| h.score).collect::<Vec<_>>(),
        "rust_weights": serde_json::to_value(rust_json.weights).unwrap(),
        "rust_source_annotations": rust_json.hits.iter()
            .map(|h| serde_json::to_value(&h.source_annotations).unwrap())
            .collect::<Vec<_>>(),
        "eri_stats": eri_json["retrieval_stats"].clone(),
    });
    let out = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("target")
        .join(format!("parity-m4-{label}.json"));
    std::fs::write(&out, serde_json::to_string_pretty(&report).unwrap())
        .expect("write m4 parity report fragment");
    println!("M4-PARITY-REPORT {label} -> {}", out.display());
}

#[test]
fn m4_parity_fuzzing() {
    compare_group("fuzzing", KEYWORD_GROUPS[0]);
}

#[test]
fn m4_parity_payload_bypass() {
    compare_group("payload_bypass", KEYWORD_GROUPS[1]);
}

#[test]
fn m4_parity_android() {
    compare_group("android", KEYWORD_GROUPS[2]);
}

/// CJK 桥接组：中文查询跨语言对齐（eri `_cjk_bridge` 与 rust `CJK_BRIDGE`
/// 同表移植，数据非代码）。
#[test]
fn m4_parity_cjk_bridge() {
    match parity_prereq() {
        Ok(()) => {}
        Err(reason) => {
            eprintln!("SKIP m4-parity[cjk]: {reason}");
            return;
        }
    }
    let tmp = fresh_tmp_dir("m4-cjk");
    let catalog_copy = snapshot_catalog(&tmp);
    let eri_json = run_eri_resolve(&catalog_copy, &["渗透"]);
    let index_db = build_search_index(&tmp);
    let rust_json = run_rust_search(&index_db, &["渗透"]);

    let eri_ids = eri_top_ids(&eri_json);
    let rust_ids: Vec<String> = rust_json
        .hits
        .iter()
        .map(|h| h.resource_id.clone())
        .collect();
    assert!(!eri_ids.is_empty(), "eri cjk bridge returned no hits");
    assert!(!rust_ids.is_empty(), "rust cjk bridge returned no hits");
    let common = overlap(&eri_ids, &rust_ids);
    assert!(
        !common.is_empty(),
        "cjk bridge hit sets do not overlap: eri={eri_ids:?} rust={rust_ids:?}"
    );
    // 桥接词必须出现在 rust 命中证据里（keyword_matches 含桥接词 pentest）
    assert!(
        rust_json
            .hits
            .iter()
            .any(|h| h.keyword_matches.iter().any(|k| k == "pentest")),
        "rust search must expand 渗透 via CJK_BRIDGE to pentest"
    );
    let top1_match = eri_ids.first() == rust_ids.first();
    println!(
        "M4-PARITY [cjk] 渗透 eri_top1={} rust_top1={} top1_match={} overlap={}/{} \
         eri_set={eri_ids:?} rust_set={rust_ids:?}",
        eri_ids.first().map(String::as_str).unwrap_or(""),
        rust_ids.first().map(String::as_str).unwrap_or(""),
        top1_match,
        common.len(),
        LIMIT,
    );
}
