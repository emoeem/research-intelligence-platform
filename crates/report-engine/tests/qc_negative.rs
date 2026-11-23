//! 质检负例面（PHASE-6B 判据 8）。
//!
//! 三面负例逐个 fail 且 QcReport.message 非空（:545「导出失败保留错误信息
//! 并允许重新导出」词形——fail 不 panic 不 abort）：
//! 1. 截断 PDF（尾部字节切除 fixture）→ qc_pdf 含 fail 项；
//! 2. CRC 篡改 DOCX（单字节翻转）→ qc_docx fail；
//! 3. SARIF 注入词表外 level/version → qc_sarif fail（official schema
//!    enum 拒收 + 结构项 fail——负例面实测落档）。

use report_engine::{
    discover_system_fonts, qc_docx, qc_pdf, qc_sarif, render_docx, render_pdf,
    render_sarif, ReportIr,
};

const MAIN_REPORT_JSON: &str = include_str!("fixtures/main-report.json");

fn parse_main() -> ReportIr {
    let value: serde_json::Value =
        serde_json::from_str(MAIN_REPORT_JSON).expect("main-report.json is valid JSON");
    report_engine::from_json(&value).expect("main-report fixture passes validate_ir")
}

fn fonts_or_skip() -> Option<Vec<report_engine::FontSource>> {
    let fonts = discover_system_fonts();
    if fonts.is_empty() {
        eprintln!("SKIP: 系统字体目录零可解析字体（SKIP 分计——D2⑤）");
        return None;
    }
    Some(fonts)
}

/// 负例 1：截断 PDF（尾部字节切除）→ qc_pdf 含 fail 项且 message 非空。
#[test]
fn qc_pdf_truncated_bytes_fail() {
    let Some(fonts) = fonts_or_skip() else { return };
    let ir = parse_main();
    let mut bytes = render_pdf(&ir, &fonts);
    // 尾部切除：%%EOF 尾 + xref/trailer 面残缺。
    let cut = bytes.len() * 9 / 10;
    bytes.truncate(cut);
    let report = qc_pdf(&ir, &bytes);
    assert!(
        !report.all_passed(),
        "truncated PDF must fail qc_pdf: {:?}",
        report.failed_checks()
    );
    assert!(
        report.failed_checks().contains(&"pdf_eof"),
        "pdf_eof check must fail on truncated bytes: {:?}",
        report.failed_checks()
    );
    for check in &report.checks {
        if !check.passed {
            assert!(!check.message.is_empty(), "fail check {} must carry message", check.name);
        }
    }
}

/// 负例 2：CRC 篡改 DOCX（单字节翻转）→ qc_docx fail 且 message 非空。
#[test]
fn qc_docx_crc_tamper_fail() {
    let ir = parse_main();
    let mut bytes = render_docx(&ir);
    // 翻转 word/document.xml 数据段字节（容器中段——条目数据面）。
    let mid = bytes.len() / 2;
    bytes[mid] ^= 0xFF;
    let report = qc_docx(&ir, &bytes);
    assert!(
        !report.all_passed(),
        "CRC-tampered DOCX must fail qc_docx: {:?}",
        report.failed_checks()
    );
    assert!(
        report.failed_checks().contains(&"docx_container_crc"),
        "docx_container_crc must fail: {:?}",
        report.failed_checks()
    );
    let crc_check = report
        .checks
        .iter()
        .find(|c| c.name == "docx_container_crc")
        .expect("crc check present");
    assert!(!crc_check.message.is_empty(), "message non-empty (:545 保留错误信息词形)");
}

/// 负例 3：SARIF 注入词表外 level（official schema enum 拒收）→ qc_sarif
/// fail 且 message 非空。
#[test]
fn qc_sarif_out_of_wordlist_level_fail() {
    let ir = parse_main();
    let mut sarif = render_sarif(&ir);
    // 注入词表外 level（"critical" 非 SARIF level enum——severity 词表值直
    // 注 result 面；official schema enum [error,warning,note,pass,none] 拒）。
    sarif["runs"][0]["results"][0]["level"] = serde_json::json!("critical");
    let report = qc_sarif(&ir, &sarif);
    assert!(
        !report.all_passed(),
        "out-of-wordlist level must fail qc_sarif: {:?}",
        report.failed_checks()
    );
    assert!(
        report.failed_checks().contains(&"sarif_finding_mapping")
            || report.failed_checks().contains(&"sarif_official_schema"),
        "mapping or official-schema check must fail: {:?}",
        report.failed_checks()
    );
}

/// 负例 3b：SARIF version 词表外值（"2.0.0"）→ 结构项 fail。
#[test]
fn qc_sarif_wrong_version_fail() {
    let ir = parse_main();
    let mut sarif = render_sarif(&ir);
    sarif["version"] = serde_json::json!("2.0.0");
    let report = qc_sarif(&ir, &sarif);
    assert!(
        !report.all_passed(),
        "wrong version must fail qc_sarif: {:?}",
        report.failed_checks()
    );
    assert!(
        report.failed_checks().contains(&"sarif_version"),
        "sarif_version check must fail: {:?}",
        report.failed_checks()
    );
    let version_check = report
        .checks
        .iter()
        .find(|c| c.name == "sarif_version")
        .expect("version check present");
    assert!(!version_check.message.is_empty(), "message non-empty");
}

/// 边界：零字体面降级渲染 → qc_pdf「中文字体正确」fail（D2④——渲染降级
/// .notdef 被质检捕捉，fail 不 panic 不 abort）。
#[test]
fn qc_pdf_degraded_no_cjk_font_fail() {
    let ir = parse_main();
    let bytes = render_pdf(&ir, &[]);
    let report = qc_pdf(&ir, &bytes);
    assert!(
        report.failed_checks().contains(&"pdf_cjk_font_embedded"),
        "degraded render (no fonts) must fail cjk check: {:?}",
        report.failed_checks()
    );
    // 结构完整性面不因降级丢失（%PDF- 头仍在——渲染器不 panic 红线 1）。
    assert!(bytes.starts_with(b"%PDF-"), "degraded render still yields PDF structure");
}
