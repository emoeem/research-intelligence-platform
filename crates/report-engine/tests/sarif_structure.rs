//! SARIF 结构对账（PHASE-6A 判据 8）。
//!
//! 断言面（D6）：version == "2.1.0"、runs[0].results 长度 = findings 数、
//! result 标准键齐（ruleId/level/message.text/properties.rip）、severity→level
//! 映射逐 Finding 对账（critical/high→error、medium→warning、low/info→note
//! 实测五行）、result_state→kind 映射、properties.rip 三键在位、artifacts 与
//! evidence uri + sha-256 对账、rules 与 results ruleId 一一对应。

use report_engine::render::sarif::{render_sarif, result_state_to_kind, severity_to_level};

const MAIN_REPORT_JSON: &str = include_str!("fixtures/main-report.json");
const MIXED_REPORT_JSON: &str = include_str!("fixtures/mixed-report.json");

fn parse(value_str: &str, label: &str) -> report_engine::ReportIr {
    let value: serde_json::Value =
        serde_json::from_str(value_str).unwrap_or_else(|e| panic!("{label} is valid JSON: {e}"));
    report_engine::from_json(&value).unwrap_or_else(|e| panic!("{label} passes validate_ir: {e}"))
}

/// 顶层结构 + 标准键齐（main fixture 2 findings）。
#[test]
fn sarif_top_level_structure_and_result_keys() {
    let ir = parse(MAIN_REPORT_JSON, "main-report");
    let sarif = render_sarif(&ir);

    assert_eq!(sarif["version"], "2.1.0", "sarif version pin");
    assert!(
        sarif["$schema"]
            .as_str()
            .expect("$schema")
            .contains("sarif-2.1.0"),
        "$schema points at sarif 2.1.0"
    );
    let runs = sarif["runs"].as_array().expect("runs array");
    assert_eq!(runs.len(), 1, "exactly one run");
    let run = &runs[0];
    let results = run["results"].as_array().expect("results");
    assert_eq!(results.len(), ir.findings.len(), "results = findings count");

    for (idx, result) in results.iter().enumerate() {
        for key in ["ruleId", "level", "kind", "message", "properties"] {
            assert!(
                result.get(key).is_some(),
                "result[{idx}] standard key {key:?} present"
            );
        }
        assert!(
            result["message"]["text"].is_string(),
            "result[{idx}] message.text present"
        );
        let rip = &result["properties"]["rip"];
        for key in ["result_state", "six_ids", "rtis_refs"] {
            assert!(
                rip.get(key).is_some(),
                "result[{idx}] properties.rip.{key} present"
            );
        }
    }

    let driver = &run["tool"]["driver"];
    assert_eq!(driver["name"], "research-platform-report-engine");
    let rules = driver["rules"].as_array().expect("rules");
    assert_eq!(rules.len(), ir.findings.len(), "rules = findings count");
    for (idx, rule) in rules.iter().enumerate() {
        let result = &results[idx];
        assert_eq!(
            result["ruleId"], rule["id"],
            "rules[{idx}].id = results[{idx}].ruleId"
        );
        assert!(
            rule["shortDescription"]["text"].is_string(),
            "rule shortDescription.text"
        );
    }
}

/// severity→level 与 result_state→kind 映射逐 Finding 对账（mixed fixture
/// 五态五级实测五行——判据 8 实测三行扩展面）。
#[test]
fn sarif_severity_level_and_state_kind_mapping() {
    let ir = parse(MIXED_REPORT_JSON, "mixed-report");
    let sarif = render_sarif(&ir);
    let results = sarif["runs"][0]["results"].as_array().expect("results");
    assert_eq!(results.len(), ir.findings.len());

    for (finding, result) in ir.findings.iter().zip(results.iter()) {
        assert_eq!(
            result["level"],
            severity_to_level(&finding.severity),
            "level mapping for {} ({})",
            finding.finding_id,
            finding.severity
        );
        assert_eq!(
            result["kind"],
            result_state_to_kind(&finding.result_state),
            "kind mapping for {} ({})",
            finding.finding_id,
            finding.result_state
        );
        assert_eq!(
            result["properties"]["rip"]["result_state"], finding.result_state,
            "rip.result_state carries the original five-state value"
        );
    }

    // 判据 8 实测行：high→error / medium→warning / low→note（+ critical→error
    // / info→note 扩展）。
    let level_of = |finding_id: &str| {
        results
            .iter()
            .find(|r| r["ruleId"] == finding_id)
            .expect("result for finding")["level"]
            .as_str()
            .expect("level string")
            .to_string()
    };
    assert_eq!(level_of("find-20261123-105"), "error", "high → error");
    assert_eq!(level_of("find-20261123-103"), "warning", "medium → warning");
    assert_eq!(level_of("find-20261123-104"), "note", "low → note");
    assert_eq!(level_of("find-20261123-101"), "error", "critical → error");
    assert_eq!(level_of("find-20261123-102"), "note", "info → note");

    let kind_of = |finding_id: &str| {
        results
            .iter()
            .find(|r| r["ruleId"] == finding_id)
            .expect("result for finding")["kind"]
            .as_str()
            .expect("kind string")
            .to_string()
    };
    assert_eq!(kind_of("find-20261123-101"), "fail", "confirmed → fail");
    assert_eq!(
        kind_of("find-20261123-102"),
        "fail",
        "false_positive → fail"
    );
    assert_eq!(
        kind_of("find-20261123-103"),
        "review",
        "not_reproduced → review"
    );
    assert_eq!(
        kind_of("find-20261123-104"),
        "review",
        "not_tested → review"
    );
    assert_eq!(
        kind_of("find-20261123-105"),
        "review",
        "inconclusive → review"
    );
}

/// artifacts 与 evidence uri 对账（uri + sha-256 hash；D1 artifacts 模型）。
#[test]
fn sarif_artifacts_reconcile_with_evidence_uris() {
    let ir = parse(MAIN_REPORT_JSON, "main-report");
    let sarif = render_sarif(&ir);
    let artifacts = sarif["runs"][0]["artifacts"].as_array().expect("artifacts");

    let main_finding = &ir.findings[0];
    let evidence = main_finding.evidence.first().expect("main evidence");
    let matched = artifacts
        .iter()
        .find(|a| a["location"]["uri"] == evidence.uri)
        .expect("evidence uri in run artifacts");
    assert_eq!(
        matched["hashes"]["sha-256"], evidence.content_hash,
        "artifact sha-256 = evidence content_hash"
    );

    // findings[].location.artifact_uri 也进 artifacts（find-002 无 evidence
    // 但有 location.artifact_uri）。
    let second_location_uri = ir.findings[1]
        .location
        .as_ref()
        .expect("second finding location")
        .artifact_uri
        .as_ref()
        .expect("artifact_uri");
    assert!(
        artifacts
            .iter()
            .any(|a| a["location"]["uri"] == *second_location_uri),
        "location artifact_uri present in run artifacts"
    );

    // 两个 finding location 与一个 evidence uri = 3 artifact 条目（无重复）。
    assert_eq!(artifacts.len(), 3, "artifacts dedup: 3 unique uris");
}

/// run.properties.rip 报告级元数据在位。
#[test]
fn sarif_run_properties_rip_metadata() {
    let ir = parse(MAIN_REPORT_JSON, "main-report");
    let sarif = render_sarif(&ir);
    let rip = &sarif["runs"][0]["properties"]["rip"];
    assert_eq!(rip["report_id"], ir.report_id);
    assert_eq!(rip["report_kind"], ir.report_kind);
    assert_eq!(rip["language"], ir.language);
}

/// help.markdown：technical_explanation 进 rule.help（D1 rule 模型）。
#[test]
fn sarif_rule_help_carries_technical_explanation() {
    let ir = parse(MAIN_REPORT_JSON, "main-report");
    let sarif = render_sarif(&ir);
    let rules = sarif["runs"][0]["tool"]["driver"]["rules"]
        .as_array()
        .expect("rules");
    let first_explanation = ir.findings[0]
        .technical_explanation
        .as_ref()
        .expect("technical_explanation field position");
    assert!(
        rules[0]["help"]["markdown"]
            .as_str()
            .expect("help markdown")
            .contains("双解释生成属 6b"),
        "rule.help.markdown carries technical_explanation content: {first_explanation}"
    );
}
