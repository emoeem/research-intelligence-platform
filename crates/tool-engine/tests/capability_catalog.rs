//! RT-1 能力面测试载具（RT-1-TASK §5 能力面判据 + S6；RT-1 ADR D2）。
//!
//! 判据对照：
//! - 种子 16 条加载成功（15 available + 1 missing pdf.mutool，RT-0.5-C §4:249-250
//!   实测口径）；
//! - probe(L0) 对种子路径 stat/sha256 校验（全量实测 + 前缀对照 + wrapper
//!   双钉）；
//! - 指纹仅在实测全量 sha256 后计算，16 位前缀不升格为指纹（C §5.1:269
//!   「不预生成、不编造」——期望值在测试内按 F §5.1 公式独立重算对照）；
//! - 五态区分硬要求：身份不符/文件缺失不得报成 missing（F §7）。

use std::path::{Path, PathBuf};

use tool_engine::capability::{
    CapabilityFilter, CapabilityId, CapabilityProvider, CapabilityStatus, Freshness,
    InvalidationEvent, ProbeBudget, ProbeError, ProbeLevel, ProbeOutcome, StaticCatalogProvider,
};
use tool_registry::hash::sha256_hex;
use tool_registry::normalize_json;

fn repo_root() -> PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root")
}

fn seed_path() -> PathBuf {
    repo_root().join("manifests/capabilities/host-catalog-seed.json")
}

fn provider() -> StaticCatalogProvider {
    StaticCatalogProvider::load(&seed_path()).expect("seed loads")
}

fn budget() -> ProbeBudget {
    ProbeBudget::default()
}

/// F §5.1 公式的测试内独立重算（capset 按 (kind,name) 排序后 canonical 序列化，
/// 与 StaticCatalogProvider::compute_fingerprint 同式；期望值不取自实现）。
fn expected_fingerprint(
    bin_sha256: &str,
    target_sha256: Option<&str>,
    version: &str,
    capabilities: &[tool_engine::capability::ProbedFeature],
) -> String {
    let mut capset: Vec<&tool_engine::capability::ProbedFeature> = capabilities.iter().collect();
    capset.sort_by(|a, b| {
        (a.kind.as_str(), a.name.as_str()).cmp(&(b.kind.as_str(), b.name.as_str()))
    });
    let capset_value: Vec<serde_json::Value> = capset
        .iter()
        .map(|f| {
            serde_json::json!({
                "flags": f.flags,
                "kind": f.kind.as_str(),
                "name": f.name,
            })
        })
        .collect();
    let mut obj = serde_json::json!({
        "bin_sha256": bin_sha256,
        "capset": capset_value,
    });
    if let Some(target) = target_sha256 {
        obj["target_sha256"] = serde_json::json!(target);
    }
    obj["version"] = serde_json::json!(version);
    sha256_hex(normalize_json(&obj).as_bytes())[..16].to_string()
}

#[test]
fn seed_loads_16_entries_15_available_1_missing() {
    let p = provider();
    assert_eq!(p.len(), 16, "RT-0.5-C §3 十六条全量");
    let all = p.list(&CapabilityFilter::default());
    assert_eq!(all.len(), 16);
    let available = p.list(&CapabilityFilter {
        status: Some(CapabilityStatus::Available),
        ..Default::default()
    });
    assert_eq!(available.len(), 15, "RT-0.5-C §4:249 实测口径");
    let missing = p.list(&CapabilityFilter {
        status: Some(CapabilityStatus::Missing),
        ..Default::default()
    });
    let ids: Vec<&str> = missing
        .iter()
        .map(|c| c.entry.capability_id.as_str())
        .collect();
    assert_eq!(ids, vec!["pdf.mutool"], "RT-0.5-C §4:250 唯一 missing");
    // 十二字段冻结面抽查：available 条目带路径/版本/身份/指纹位（None 直至实测）。
    let pdftotext = p
        .get(&CapabilityId::new("pdf.pdftotext").unwrap())
        .expect("entry");
    assert_eq!(
        pdftotext.entry.executable_path.as_deref(),
        Some(Path::new("/usr/bin/pdftotext"))
    );
    assert_eq!(
        pdftotext.entry.version.as_ref().unwrap().normalized,
        "26.11.0"
    );
    assert_eq!(
        pdftotext
            .entry
            .binary_identity
            .as_ref()
            .unwrap()
            .sha256_prefix,
        "f85872f94fb67433"
    );
    assert!(pdftotext.entry.fingerprint.is_none(), "未实测不产指纹");
    assert_eq!(pdftotext.last_probe_level, ProbeLevel::L0Static);
    assert_eq!(pdftotext.freshness, Freshness::Fresh);
    // wrapper 双钉转录：archive.7z wrapper 全值在案（C §3.11:168）。
    let z7 = p
        .get(&CapabilityId::new("archive.7z").unwrap())
        .expect("entry");
    let identity = z7.entry.binary_identity.as_ref().unwrap();
    assert_eq!(
        identity.sha256_full.as_deref(),
        Some("f802383f13b8331aabaf779def00a356c8d2b8148255170b75b03028f4d23ad4")
    );
    let target = identity.wrapper_target.as_ref().unwrap();
    assert_eq!(target.executable_path, PathBuf::from("/usr/lib/7zip/7z"));
    assert_eq!(target.sha256_prefix, "d48e622e9009acdb");
    // missing 条目「—」字段以 None 承载。
    let mutool = p
        .get(&CapabilityId::new("pdf.mutool").unwrap())
        .expect("entry");
    assert!(mutool.entry.executable_path.is_none());
    assert!(mutool.entry.version.is_none());
    assert!(mutool.entry.binary_identity.is_none());
}

#[test]
fn seed_rejects_duplicate_ids_and_bad_shapes() {
    // 重复 id 拒绝（形状校验面）。
    let doc: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(seed_path()).unwrap()).unwrap();
    let mut bad = doc.clone();
    let first = bad["entries"][0].clone();
    bad["entries"].as_array_mut().unwrap().push(first);
    let err = StaticCatalogProvider::from_seed_document(serde_json::from_value(bad).unwrap())
        .unwrap_err();
    assert!(err.contains("duplicate capability id"), "err: {err}");

    // 前缀形状校验：16hex 之外的值拒绝（不编造纪律的结构面）。
    let mut bad = doc;
    bad["entries"][0]["binary_identity"]["sha256_prefix"] = serde_json::json!("short-prefix");
    let err = StaticCatalogProvider::from_seed_document(serde_json::from_value(bad).unwrap())
        .unwrap_err();
    assert!(
        err.contains("sha256_prefix must be 16 lowercase hex"),
        "err: {err}"
    );
}

#[test]
fn probe_l0_measures_full_sha256_and_fingerprint() {
    let p = provider();
    let id = CapabilityId::new("pdf.pdftotext").unwrap();
    // 未实测：fingerprint None（前缀不升格）。
    assert!(p.fingerprint(&id).is_none());
    let outcome = p
        .probe(&id, ProbeLevel::L0Static, &budget())
        .expect("probe ok");
    let ProbeOutcome::Entry(entry) = outcome else {
        panic!("expect Entry");
    };
    assert_eq!(entry.status, CapabilityStatus::Available);
    let full = entry
        .binary_identity
        .as_ref()
        .unwrap()
        .sha256_full
        .as_ref()
        .expect("probe 实测全量 sha256");
    assert_eq!(full.len(), 64);
    assert!(
        full.starts_with("f85872f94fb67433"),
        "实测值与种子前缀一致（对照通过才返回 Entry）"
    );
    // 实际文件 sha256 独立重算对照（L0 实测正确性）。
    let actual = sha256_hex(&std::fs::read("/usr/bin/pdftotext").unwrap());
    assert_eq!(full, &actual, "probe 实测 == 独立 sha256_hex");
    // 指纹 16 位小写 hex，且与 F §5.1 公式测试内独立重算一致。
    let fp = entry.fingerprint.as_ref().expect("fingerprint after probe");
    assert_eq!(fp.0.len(), 16);
    let expected = expected_fingerprint(
        full,
        None,
        &entry.version.as_ref().unwrap().normalized,
        &entry.capabilities,
    );
    assert_eq!(fp.0, expected, "fingerprint == F §5.1 公式重算值");
    assert_eq!(p.fingerprint(&id).as_ref(), Some(fp), "缓存读一致");
}

#[test]
fn probe_l0_wrapper_double_pin_measured() {
    let p = provider();
    let id = CapabilityId::new("archive.7z").unwrap();
    let outcome = p
        .probe(&id, ProbeLevel::L0Static, &budget())
        .expect("probe ok");
    let ProbeOutcome::Entry(entry) = outcome else {
        panic!("expect Entry");
    };
    let identity = entry.binary_identity.as_ref().unwrap();
    assert_eq!(
        identity.sha256_full.as_deref(),
        Some("f802383f13b8331aabaf779def00a356c8d2b8148255170b75b03028f4d23ad4"),
        "wrapper 全值与种子在案实测一致"
    );
    let target = identity.wrapper_target.as_ref().unwrap();
    let target_full = target.sha256_full.as_ref().expect("exec 目标全量实测");
    let actual = sha256_hex(&std::fs::read("/usr/lib/7zip/7z").unwrap());
    assert_eq!(target_full, &actual, "wrapper exec 目标 L0 实测");
    // 指纹含 target_sha256 维度（C §5.1:263 wrapper 场景公式）。
    let fp = entry.fingerprint.as_ref().expect("fingerprint");
    let expected = expected_fingerprint(
        identity.sha256_full.as_ref().unwrap(),
        Some(target_full),
        &entry.version.as_ref().unwrap().normalized,
        &entry.capabilities,
    );
    assert_eq!(fp.0, expected);
}

#[test]
fn probe_l0_missing_entry_rechecked_still_missing() {
    let p = provider();
    let id = CapabilityId::new("pdf.mutool").unwrap();
    match p.probe(&id, ProbeLevel::L0Static, &budget()) {
        Ok(ProbeOutcome::Entry(entry)) => {
            assert_eq!(entry.status, CapabilityStatus::Missing);
            assert!(entry.fingerprint.is_none(), "missing 不产指纹");
        }
        Ok(ProbeOutcome::NotProbed { .. }) => panic!("L0 应支持"),
        Err(ProbeError::ProbeFailed { detail }) => {
            if detail.contains("candidate found") {
                panic!("宿主已装 mutool：RT-0.5-C §3.9 实测口径失真，目录需重探（{detail}）");
            }
            panic!("unexpected probe failure: {detail}");
        }
        Err(e) => panic!("unexpected: {e}"),
    }
}

#[test]
fn probe_l0_identity_mismatch_marks_stale_not_missing() {
    // 五态区分硬要求（F §7）：身份不符 → ProbeFailed + 标 stale，不得报 missing。
    let dir = std::env::temp_dir().join(format!("rt1-cap-mismatch-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let bin = dir.join("fakebin");
    std::fs::write(&bin, b"content-v1").unwrap();
    let doc: tool_engine::capability::SeedDocument = serde_json::from_value(serde_json::json!({
        "schema_version": "1",
        "source": "test",
        "entries": [{
            "capability_id": "test.fakebin",
            "provider": "test",
            "executable_path": bin.display().to_string(),
            "version": {"normalized": "1.0", "raw": "1.0"},
            "binary_identity": {
                "sha256_prefix": "0000000000000000",
                "file_type": "elf-pie",
                "owner_mode": "test:test 644",
                "wrapper_target": null
            },
            "package_provenance": "unknown",
            "capabilities": [],
            "status": "available",
            "probe_timestamp": "2026-11-19",
            "constraints": [],
            "security_profile": {
                "allowed_call_modes": ["restricted_native_subprocess"],
                "max_input_risk": "trusted_low_risk",
                "notes": []
            },
            "search_name": null
        }]
    }))
    .unwrap();
    let p = StaticCatalogProvider::from_seed_document(doc).unwrap();
    let id = CapabilityId::new("test.fakebin").unwrap();
    let err = p
        .probe(&id, ProbeLevel::L0Static, &budget())
        .expect_err("prefix mismatch must fail");
    assert!(
        matches!(err, ProbeError::ProbeFailed { .. })
            && err.to_string().contains("prefix mismatch"),
        "身份不符必须 ProbeFailed（不得 missing）：{err}"
    );
    let cached = p.get(&id).expect("entry still cached");
    assert_eq!(
        cached.freshness,
        Freshness::Stale {
            reason: tool_engine::capability::StaleReason::BinaryChanged
        },
        "身份不符 → 标 stale（不可供决策）"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn probe_l0_missing_file_reports_missing_five_state() {
    // 钉值路径无文件 → Missing 五态（非 probe_failed，F §7 路由语义）。
    let doc: tool_engine::capability::SeedDocument = serde_json::from_value(serde_json::json!({
        "schema_version": "1",
        "source": "test",
        "entries": [{
            "capability_id": "test.gone",
            "provider": "test",
            "executable_path": "/nonexistent/rt1/gone-bin",
            "version": null,
            "binary_identity": {
                "sha256_prefix": "1111111111111111",
                "file_type": "elf-pie",
                "owner_mode": "root:root 755",
                "wrapper_target": null
            },
            "package_provenance": "unknown",
            "capabilities": [],
            "status": "available",
            "probe_timestamp": "2026-11-19",
            "constraints": [],
            "security_profile": {
                "allowed_call_modes": ["restricted_native_subprocess"],
                "max_input_risk": "trusted_low_risk",
                "notes": []
            },
            "search_name": null
        }]
    }))
    .unwrap();
    let p = StaticCatalogProvider::from_seed_document(doc).unwrap();
    let id = CapabilityId::new("test.gone").unwrap();
    let err = p
        .probe(&id, ProbeLevel::L0Static, &budget())
        .expect_err("missing file");
    match err {
        ProbeError::Missing { searched_roots } => {
            assert_eq!(
                searched_roots,
                vec![PathBuf::from("/nonexistent/rt1/gone-bin")]
            );
        }
        other => panic!("expect Missing, got {other}"),
    }
}

#[test]
fn probe_above_l0_returns_not_probed() {
    // RT-1 最小面只支持 L0（完整探测留 DI-1 前置批，ADR D2）。
    let p = provider();
    let id = CapabilityId::new("media.ffprobe").unwrap();
    for level in [
        ProbeLevel::L1Version,
        ProbeLevel::L2Capability,
        ProbeLevel::L3Smoke,
    ] {
        match p.probe(&id, level, &budget()).expect("not probed") {
            ProbeOutcome::NotProbed { level_reached } => {
                assert_eq!(level_reached, ProbeLevel::L0Static);
            }
            ProbeOutcome::Entry(_) => panic!("level {level:?} should not be probed in RT-1"),
        }
    }
}

#[test]
fn list_filter_domain_status_feature() {
    let p = provider();
    let pdf = p.list(&CapabilityFilter {
        domain_prefix: Some("pdf.".to_string()),
        ..Default::default()
    });
    let mut ids: Vec<&str> = pdf.iter().map(|c| c.entry.capability_id.as_str()).collect();
    ids.sort();
    assert_eq!(
        ids,
        vec!["pdf.mutool", "pdf.pdfinfo", "pdf.pdftoppm", "pdf.pdftotext"]
    );
    // feature 过滤：language=chi_sim → 仅 tesseract。
    let lang = p.list(&CapabilityFilter {
        feature: Some(tool_engine::capability::FeatureQuery {
            kind: Some(tool_engine::capability::FeatureKind::Language),
            name: Some("chi_sim".to_string()),
        }),
        ..Default::default()
    });
    assert_eq!(lang.len(), 1);
    assert_eq!(lang[0].entry.capability_id.as_str(), "ocr.tesseract");
    // 未知能力 id 的 list 为空、get 为 None。
    assert!(p
        .list(&CapabilityFilter {
            domain_prefix: Some("nosuch.".to_string()),
            ..Default::default()
        })
        .is_empty());
    assert!(p.get(&CapabilityId::new("nosuch.tool").unwrap()).is_none());
}

#[test]
fn invalidate_marks_stale_with_counts() {
    let mut p = provider();
    let id = CapabilityId::new("pdf.pdftotext").unwrap();
    let old = tool_engine::capability::Fingerprint("0".repeat(16));
    let new = tool_engine::capability::Fingerprint("1".repeat(16));
    assert_eq!(
        p.invalidate(&InvalidationEvent::BinaryChanged {
            capability_id: id.clone(),
            old: old.clone(),
            new: new.clone(),
        }),
        1
    );
    let cached = p.get(&id).expect("entry");
    assert!(matches!(cached.freshness, Freshness::Stale { .. }));
    // 已 stale 再失效 → 计数 0。
    assert_eq!(
        p.invalidate(&InvalidationEvent::ManualInvalidate {
            capability_id: Some(id.clone()),
        }),
        0
    );
    // probe 失效事件带五态（media.ffmpeg 此前未标 stale → 1）。
    assert_eq!(
        p.invalidate(&InvalidationEvent::ProbeFailed {
            capability_id: CapabilityId::new("media.ffmpeg").unwrap(),
            kind: tool_engine::capability::ProbeErrorKind::Timeout,
        }),
        1
    );
    // 全量手动失效 → 其余 14 条（pdftotext/ffmpeg 已 stale）。
    assert_eq!(
        p.invalidate(&InvalidationEvent::ManualInvalidate {
            capability_id: None,
        }),
        14
    );
    assert_eq!(p.list(&CapabilityFilter::default()).len(), 16);
}

#[test]
fn refresh_after_probe_returns_fresh_entry() {
    let mut p = provider();
    let id = CapabilityId::new("pdf.pdfinfo").unwrap();
    p.invalidate(&InvalidationEvent::ManualInvalidate {
        capability_id: Some(id.clone()),
    });
    assert!(matches!(
        p.get(&id).unwrap().freshness,
        Freshness::Stale { .. }
    ));
    let refreshed = p
        .refresh(&id, ProbeLevel::L0Static, &budget())
        .expect("refresh ok");
    assert_eq!(refreshed.freshness, Freshness::Fresh, "重探成功回 Fresh");
    assert!(refreshed.entry.fingerprint.is_some(), "重探实测产指纹");
}
