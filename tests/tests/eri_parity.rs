//! M2-B：ERI 与 Rust resolve 行为对照（同一批关键词，双轨跑，比对核心命中集合）。
//!
//! 判定口径（任务书）：排序权重不同（eri 六轴 vs 本批 keyword 60%+quality 40%），
//! 允许顺序差异；硬断言为 top-K 命中集合重叠非空；top-1 是否一致写入对照报告，
//! 不作为硬断言（见 M2_REPORT.md 差异表）。

use rp_eri_parity::{
    fresh_tmp_dir, parity_prereq, run_eri_resolve, run_rust_resolve, snapshot_catalog,
    KEYWORD_GROUPS, LIMIT, REQUIRED_HIT_FIELDS,
};
use serde_json::Value;

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

fn rust_top_ids(v: &resource_engine::ResolveResult) -> Vec<String> {
    v.matched_resources
        .iter()
        .map(|h| h.resource_id.clone())
        .collect()
}

fn overlap(a: &[String], b: &[String]) -> Vec<String> {
    a.iter().filter(|x| b.contains(x)).cloned().collect()
}

/// 单组对照：断言核心命中集合重叠，返回报告行。
fn compare_group(label: &str, keywords: &[&str]) {
    match parity_prereq() {
        Ok(()) => {}
        Err(reason) => {
            eprintln!("SKIP parity[{label}]: {reason}");
            return;
        }
    }
    let tmp = fresh_tmp_dir(label);
    let catalog_copy = snapshot_catalog(&tmp);
    let eri_json = run_eri_resolve(&catalog_copy, keywords);
    let rust_json = run_rust_resolve(keywords);

    // 双方都应有非空命中
    let eri_ids = eri_top_ids(&eri_json);
    let rust_ids = rust_top_ids(&rust_json);
    assert!(
        !eri_ids.is_empty(),
        "eri returned no hits for {keywords:?}: {}",
        eri_json["search_status"]
    );
    assert!(
        !rust_ids.is_empty(),
        "rust returned no hits for {keywords:?}: {}",
        rust_json.search_status
    );

    // eri 命中元素必备字段存在（形状兼容性的基准面）
    let hits = eri_json["matched_resources"].as_array().unwrap();
    for (i, h) in hits.iter().enumerate() {
        for f in REQUIRED_HIT_FIELDS {
            assert!(
                h.get(f).is_some(),
                "eri hit[{i}] missing required field {f}"
            );
        }
    }

    // Rust 命中元素字段级兼容：必备字段全部在序列化输出中
    let rust_hits = serde_json::to_value(&rust_json.matched_resources).unwrap();
    for (i, h) in rust_hits.as_array().unwrap().iter().enumerate() {
        for f in REQUIRED_HIT_FIELDS {
            assert!(
                h.get(f).is_some(),
                "rust hit[{i}] missing required field {f}"
            );
        }
    }

    // 核心命中集合重叠（硬断言）：允许顺序差异，但 top-K 集合必须有交集
    let common = overlap(&eri_ids, &rust_ids);
    assert!(
        !common.is_empty(),
        "top-{LIMIT} hit sets do not overlap for {keywords:?}: eri={eri_ids:?} rust={rust_ids:?}"
    );

    let top1_match = eri_ids.first() == rust_ids.first();
    println!(
        "PARITY [{label}] kw={keywords:?} eri_top1={} rust_top1={} top1_match={} \
         overlap={}/{} eri_set={:?} rust_set={:?} common={:?}",
        eri_ids.first().map(String::as_str).unwrap_or(""),
        rust_ids.first().map(String::as_str).unwrap_or(""),
        top1_match,
        common.len(),
        LIMIT,
        eri_ids,
        rust_ids,
        common,
    );
    // 报告行同时落到 target/parity-<label>.json（构建产物目录，不入库）
    let report = serde_json::json!({
        "group": label,
        "keywords": keywords,
        "eri_top1": eri_ids.first(),
        "rust_top1": rust_ids.first(),
        "top1_match": top1_match,
        "overlap": common,
        "eri_top5": eri_ids,
        "rust_top5": rust_ids,
        "eri_scores": hits.iter().map(|h| h["relevance_score"].clone()).collect::<Vec<_>>(),
        "rust_scores": rust_json.matched_resources.iter().map(|h| h.relevance_score).collect::<Vec<_>>(),
        "eri_weights": eri_json["weights_config"].clone(),
        "rust_weights": serde_json::to_value(rust_json.weights_config).unwrap(),
        "eri_stats": eri_json["retrieval_stats"].clone(),
        "rust_stats": serde_json::to_value(rust_json.retrieval_stats).unwrap(),
    });
    let out = target_dir().join(format!("parity-{label}.json"));
    std::fs::write(&out, serde_json::to_string_pretty(&report).unwrap())
        .expect("write parity report fragment");
    println!("PARITY-REPORT {label} -> {}", out.display());
}

fn target_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("target")
}

#[test]
fn parity_fuzzing() {
    compare_group("fuzzing", KEYWORD_GROUPS[0]);
}

#[test]
fn parity_payload_bypass() {
    compare_group("payload_bypass", KEYWORD_GROUPS[1]);
}

#[test]
fn parity_android() {
    compare_group("android", KEYWORD_GROUPS[2]);
}
