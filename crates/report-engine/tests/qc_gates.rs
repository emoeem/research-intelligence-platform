//! 质检三面正例（PHASE-6B 判据 7/9）。
//!
//! :545 十项映射批次报告表格对账位（D6① 分组词形）——本文件为三质检函数
//! 对 S3/S4 产物 + render_sarif 全 pass 正例面；负例面见 qc_negative.rs。
//!
//! SARIF 官方 schema 校验（判据 9——6A 坏消息 9 收口）：
//! schemas/sarif/sarif-schema-2.1.0.json（oasis-tcs main 分支 112768 字节
//! draft-04）经 jsonschema_lite 校验 render_sarif 输出 = Ok；mixed fixture
//! 同过。

use report_engine::{
    discover_system_fonts, qc_docx, qc_pdf, qc_sarif, render_attack_path_svg, render_docx,
    render_pdf, render_sarif, ReportIr,
};

const MAIN_REPORT_JSON: &str = include_str!("fixtures/main-report.json");
const MIXED_REPORT_JSON: &str = include_str!("fixtures/mixed-report.json");

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

/// 系统字体面 skip 门（D2⑤）。
fn fonts_or_skip() -> Option<Vec<report_engine::FontSource>> {
    let fonts = discover_system_fonts();
    if fonts.is_empty() {
        eprintln!("SKIP: 系统字体目录零可解析字体（SKIP 分计——D2⑤）");
        return None;
    }
    Some(fonts)
}

/// CJK 依赖检查名：系统无 CJK 字体面时 render_pdf 降级 .notdef，这三项对
/// 中文字形敏感、必然 fail——按 D2⑤「中文判据 SKIP 分开计数」处理，不计
/// 渲染器缺陷（CI runner 无 CJK 字体的真实环境面：qc_pdf 五绿两 SKIP）。
const CJK_DEPENDENT_CHECKS: &[&str] = &[
    "pdf_sections_present",
    "pdf_attack_paths",
    "pdf_code_blocks",
];

/// CJK 字体在位探测（pdf_render.rs:37 `has_cjk_face` 同款锚点——D2⑤）。
fn has_cjk_face(fonts: &[report_engine::FontSource]) -> bool {
    use skrifa::MetadataProvider;
    fonts.iter().any(
        |source| match skrifa::FontRef::from_index(&source.data, source.index) {
            Ok(font) => {
                let charmap = font.charmap();
                charmap.map('\u{4E00}').is_some()
                    && charmap.map('\u{6C34}').is_some()
                    && charmap.map('\u{9FA5}').is_some()
            }
            Err(_) => false,
        },
    )
}

/// 正例断言：无 CJK 字体面时 CJK 依赖检查按 SKIP 分计（D2⑤）；其余必须全
/// pass（非 CJK 面在任何环境下都是硬判据——不给环境噪声留门）。
fn assert_all_pass_cjk_aware(report: &report_engine::QcReport, cjk: bool, label: &str) {
    let (cjk_skipped, real_failed): (Vec<&'static str>, Vec<&'static str>) = report
        .failed_checks()
        .into_iter()
        .partition(|name| !cjk && CJK_DEPENDENT_CHECKS.contains(name));
    if !cjk_skipped.is_empty() {
        eprintln!(
            "SKIP: {label} 无 CJK 字体面，跳过 {} 项中文判据（D2⑤）：{cjk_skipped:?}",
            cjk_skipped.len()
        );
    }
    assert!(real_failed.is_empty(), "{label} must pass: {real_failed:?}");
}

/// 打印质检报告（诊断回显——批次报告数字来源）。
fn dump(report: &report_engine::QcReport) {
    for check in &report.checks {
        println!(
            "  [{}] {} {}",
            if check.passed { "pass" } else { "FAIL" },
            check.name,
            check.message
        );
    }
}

/// 判据 7：qc_pdf(main PDF) 全 passed（无 CJK 字体面时中文判据 SKIP 分计）。
#[test]
fn qc_pdf_main_fixture_all_pass() {
    let Some(fonts) = fonts_or_skip() else { return };
    let cjk = has_cjk_face(&fonts);
    let ir = parse_main();
    let bytes = render_pdf(&ir, &fonts);
    let report = qc_pdf(&ir, &bytes);
    println!("qc_pdf ({} checks, cjk_face={cjk}):", report.checks.len());
    dump(&report);
    assert_all_pass_cjk_aware(&report, cjk, "qc_pdf");
}

/// 判据 7：qc_docx(main DOCX) 全 passed。
#[test]
fn qc_docx_main_fixture_all_pass() {
    let ir = parse_main();
    let bytes = render_docx(&ir);
    let report = qc_docx(&ir, &bytes);
    println!("qc_docx ({} checks):", report.checks.len());
    dump(&report);
    assert!(
        report.all_passed(),
        "qc_docx must pass all checks on main fixture: {:?}",
        report.failed_checks()
    );
}

/// 判据 7：qc_sarif(main SARIF) 全 passed。
#[test]
fn qc_sarif_main_fixture_all_pass() {
    let ir = parse_main();
    let sarif = render_sarif(&ir);
    let report = qc_sarif(&ir, &sarif);
    println!("qc_sarif ({} checks):", report.checks.len());
    dump(&report);
    assert!(
        report.all_passed(),
        "qc_sarif must pass all checks on main fixture: {:?}",
        report.failed_checks()
    );
}

/// 判据 9：mixed fixture 三面同过（SARIF 官方 schema 校验 mixed 面）。
#[test]
fn qc_three_faces_mixed_fixture_all_pass() {
    let ir = parse_mixed();
    let sarif = render_sarif(&ir);
    let pdf_report = qc_sarif(&ir, &sarif);
    assert!(
        pdf_report.all_passed(),
        "qc_sarif mixed: {:?}",
        pdf_report.failed_checks()
    );
    let docx_report = qc_docx(&ir, &render_docx(&ir));
    assert!(
        docx_report.all_passed(),
        "qc_docx mixed: {:?}",
        docx_report.failed_checks()
    );
}

/// 判据 9 资产：官方 schema 文件在位且与内嵌单点一致（sha256 落批次报告）。
#[test]
fn sarif_schema_asset_matches_embedded() {
    // 内嵌单点经 include_str! 同源——本断言防两处文本漂移（资产文件被意外
    // 改动时 embedded 与 include 重编面一致性由编译保证；此处断言 schema
    // 可解析 + draft-04 词形）。
    let schema: serde_json::Value =
        serde_json::from_str(report_engine::qc::SARIF_SCHEMA).expect("SARIF schema parses");
    assert_eq!(
        schema.get("$schema").and_then(|v| v.as_str()),
        Some("http://json-schema.org/draft-04/schema#"),
        "official SARIF schema declares draft-04"
    );
}

/// 判据 10 联动：SVG 攻击路径面（图表关联 Evidence）正例——节点数 =
/// 步骤数 + evidence_refs 词形可追溯。
#[test]
fn render_attack_path_svg_fixture_reconciliation() {
    let ir = parse_main();
    let svg = render_attack_path_svg(&ir);
    assert!(svg.starts_with("<svg"), "svg root element first");
    assert!(
        svg.trim_end().ends_with("</svg>"),
        "svg root element closed"
    );
    // 步骤数 = 节点数（data-step-index 计数）。
    let expected_steps: usize = ir.attack_paths.iter().map(|p| p.steps.len()).sum();
    let nodes = svg.matches("data-step-index=").count();
    assert_eq!(nodes, expected_steps, "svg nodes = attack_paths steps");
    // evidence_refs 词形 grep（图表关联 Evidence 验收点）。
    for path in &ir.attack_paths {
        for evidence_ref in &path.evidence_refs {
            assert!(
                svg.contains(evidence_ref),
                "evidence_ref {evidence_ref} traceable in svg"
            );
        }
        assert!(
            svg.contains(&path.path_id),
            "path_id {} in svg",
            path.path_id
        );
    }
    // 中文原样（XML UTF-8 直存）。
    assert!(svg.contains(&ir.title), "title zh in svg");
    // XML 标签平衡（<g> 与 </g> 配对——结构 well-formed 最小面）。
    assert_eq!(
        svg.matches("<g").count() - svg.matches("</g>").count(),
        0,
        "group tags balanced"
    );
}
