//! 三格式内容一致性 + 确定性 + 空集边界（PHASE-6A 判据 7/9/10）。
//!
//! 对账面（D6）：同一 IR 三格式渲染，finding 数 / finding_id / title /
//! severity 逐一在三格式出现；`rtis://poc/42` 与六 ID 词形三面 grep 同值；
//! 同 IR 两次渲染字节一致；零 findings IR 三渲染合法空节。

use report_engine::{render_html, render_json, render_markdown, render_sarif, ReportIr};

const MAIN_REPORT_JSON: &str = include_str!("fixtures/main-report.json");
const MIXED_REPORT_JSON: &str = include_str!("fixtures/mixed-report.json");

/// 解析 + 校验 fixture（校验不过即 panic——fixture 是判据 4 正例面）。
fn parse_main() -> ReportIr {
    let value: serde_json::Value =
        serde_json::from_str(MAIN_REPORT_JSON).expect("main-report.json is valid JSON");
    report_engine::from_json(&value).expect("main-report fixture passes validate_ir")
}

fn parse_mixed() -> ReportIr {
    let value: serde_json::Value =
        serde_json::from_str(MIXED_REPORT_JSON).expect("mixed-report.json is valid JSON");
    report_engine::from_json(&value).expect("mixed-report fixture passes validate_ir")
}

/// 判据 7：main fixture（2 Finding）三格式 finding 数一致。
#[test]
fn main_report_finding_count_consistent_across_three_formats() {
    let ir = parse_main();
    assert_eq!(ir.findings.len(), 2, "main fixture carries 2 findings");
    let markdown = render_markdown(&ir);
    let html = render_html(&ir);
    let json = render_json(&ir);
    let sarif = render_sarif(&ir);

    let md_count = markdown.matches("### find-20261123-").count();
    assert_eq!(
        md_count, 2,
        "markdown finding detail headings = 2: {md_count}"
    );
    let html_count = html.matches("class=\"finding finding-").count();
    assert_eq!(html_count, 2, "html finding articles = 2: {html_count}");
    let json_count = json["findings"].as_array().expect("findings array").len();
    assert_eq!(json_count, 2, "json findings = 2");
    let sarif_count = sarif["runs"][0]["results"]
        .as_array()
        .expect("results array")
        .len();
    assert_eq!(sarif_count, 2, "sarif results = 2");
}

/// 判据 7：每 finding_id/title/severity 在 MD/HTML/JSON 逐一出现。
#[test]
fn main_report_finding_fields_present_in_three_formats() {
    let ir = parse_main();
    let markdown = render_markdown(&ir);
    let html = render_html(&ir);
    let json = render_json(&ir);

    for finding in &ir.findings {
        for field in [
            finding.finding_id.as_str(),
            finding.title.as_str(),
            finding.severity.as_str(),
            finding.vuln_type.as_str(),
        ] {
            assert!(
                markdown.contains(field),
                "markdown missing {field:?} for {}",
                finding.finding_id
            );
            assert!(
                html.contains(field),
                "html missing {field:?} for {}",
                finding.finding_id
            );
        }
        let json_finding = json["findings"]
            .as_array()
            .expect("findings array")
            .iter()
            .find(|f| f["finding_id"] == finding.finding_id)
            .unwrap_or_else(|| panic!("json missing {}", finding.finding_id));
        assert_eq!(json_finding["title"], finding.title);
        assert_eq!(json_finding["severity"], finding.severity);
        assert_eq!(json_finding["result_state"], finding.result_state);
    }
}

/// 判据 7：`rtis://poc/42` 与六 ID 词形三面 grep 同值。
#[test]
fn main_report_rtis_ref_and_six_ids_consistent_across_three_formats() {
    let ir = parse_main();
    let markdown = render_markdown(&ir);
    let html = render_html(&ir);
    let sarif = render_sarif(&ir);
    let sarif_text = serde_json::to_string(&sarif).expect("sarif to string");

    let rtis_ref = "rtis://poc/42";
    assert!(markdown.contains(rtis_ref), "markdown missing {rtis_ref}");
    assert!(html.contains(rtis_ref), "html missing {rtis_ref}");
    assert!(sarif_text.contains(rtis_ref), "sarif missing {rtis_ref}");

    let six_ids = [
        "re-20261123-001",
        "artifact-20261123-003",
        "toolrun-20261123-007",
        "trace-20261123-abc123",
    ];
    for id in six_ids {
        assert!(markdown.contains(id), "markdown missing six-id {id}");
        assert!(html.contains(id), "html missing six-id {id}");
        assert!(sarif_text.contains(id), "sarif missing six-id {id}");
    }

    // SARIF 面六 ID 结构化在位：properties.rip.six_ids 四键。
    let rip = &sarif["runs"][0]["results"][0]["properties"]["rip"];
    let six_ids_obj = &rip["six_ids"];
    for key in ["evidence_ids", "artifact_ids", "tool_run_ids", "trace_ids"] {
        assert!(
            six_ids_obj[key].as_array().expect("six_ids list").len() == 1,
            "sarif six_ids.{key} carries one value"
        );
    }
    assert_eq!(
        rip["rtis_refs"].as_array().expect("rtis_refs")[0],
        "rtis://poc/42",
        "sarif rip.rtis_refs[0] = rtis://poc/42"
    );
}

/// 判据 8 前置：mixed fixture 五态 + 五 severity 全覆盖通过校验。
#[test]
fn mixed_report_covers_five_states_and_five_severities() {
    let ir = parse_mixed();
    let states: Vec<&str> = ir
        .findings
        .iter()
        .map(|f| f.result_state.as_str())
        .collect();
    assert_eq!(
        states,
        vec![
            "confirmed",
            "false_positive",
            "not_reproduced",
            "not_tested",
            "inconclusive"
        ],
        "mixed fixture covers five result states"
    );
    let mut severities: Vec<&str> = ir.findings.iter().map(|f| f.severity.as_str()).collect();
    severities.sort_unstable();
    assert_eq!(
        severities,
        vec!["critical", "high", "info", "low", "medium"],
        "mixed fixture covers five severities"
    );
}

/// 判据 9：零 findings IR 三渲染 exit 0、计数一致 = 0、SARIF results = []、
/// MD/HTML 合法空节。
#[test]
fn empty_findings_ir_renders_legal_empty_sections() {
    let empty_ir: serde_json::Value = serde_json::json!({
        "schema_version": "1",
        "report_id": "rep-empty-001",
        "report_kind": "management_report",
        "title": "空集边界报告",
        "generated_at": "2026-11-23T00:00:00Z",
        "findings": []
    });
    let ir = report_engine::from_json(&empty_ir).expect("empty findings IR passes validate_ir");
    let markdown = render_markdown(&ir);
    let html = render_html(&ir);
    let json = render_json(&ir);
    let sarif = render_sarif(&ir);

    assert_eq!(json["findings"].as_array().expect("findings").len(), 0);
    assert_eq!(
        sarif["runs"][0]["results"]
            .as_array()
            .expect("results")
            .len(),
        0,
        "sarif results = []"
    );
    assert_eq!(
        sarif["runs"][0]["tool"]["driver"]["rules"]
            .as_array()
            .expect("rules")
            .len(),
        0,
        "sarif rules = []"
    );
    assert_eq!(
        sarif["runs"][0]["artifacts"]
            .as_array()
            .expect("artifacts")
            .len(),
        0,
        "sarif artifacts = []"
    );
    assert!(markdown.contains("（无 Finding）"), "md legal empty list");
    assert!(html.contains("（无 Finding）"), "html legal empty list");
    assert!(
        html.starts_with("<!DOCTYPE html>"),
        "html document still complete for empty findings"
    );
}

/// 判据 10：同 IR 两次渲染字节一致（四函数逐个实测）。
#[test]
fn rendering_is_deterministic_across_four_functions() {
    let ir = parse_main();
    assert_eq!(
        render_markdown(&ir),
        render_markdown(&ir),
        "markdown stable"
    );
    assert_eq!(render_html(&ir), render_html(&ir), "html stable");
    assert_eq!(render_json(&ir), render_json(&ir), "json stable");
    assert_eq!(render_sarif(&ir), render_sarif(&ir), "sarif stable");

    let mixed = parse_mixed();
    assert_eq!(
        render_sarif(&mixed),
        render_sarif(&mixed),
        "sarif stable on mixed fixture"
    );
}

/// 语言缺省：无 language 字段时缺省 zh（:670 中文正确验收）。
#[test]
fn language_defaults_to_zh_when_absent() {
    let ir = parse_mixed();
    assert_eq!(ir.language, "zh", "mixed fixture has no language field");
}
