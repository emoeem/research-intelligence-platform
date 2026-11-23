//! render_docx 判据面（PHASE-6B 判据 6）。
//!
//! 对账面（S4 门）：
//! - 字节非空 + `PK\x03\x04` 头 + central directory EOCD 在位；
//! - `[Content_Types].xml` 与 `word/document.xml` 两零件在位；
//! - 逐条目 stored CRC32 全对（自解析断言——ooxml::qc_zip_entries 共用）；
//! - 中文/finding_id/rtis_ref 在 document.xml 字节面 grep 同值（6A 判据 7
//!   词形延续——XML UTF-8 原样，零编码面）；
//! - 确定性：同 IR 两次渲染字节一致（判据 9 语义延续）；
//! - 负例面（判据 8 预置）：单字节翻转 → CRC 对账 fail。

use report_engine::ooxml::qc_zip_entries;
use report_engine::{render_docx, ReportIr};

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

/// 子串在字节面出现的次数。
fn count_sub(bytes: &[u8], needle: &[u8]) -> usize {
    bytes.windows(needle.len()).filter(|w| *w == needle).count()
}

/// 判据 6：fixture IR → DOCX 容器结构对账。
#[test]
fn render_docx_container_structure() {
    let ir = parse_main();
    let bytes = render_docx(&ir);
    assert!(!bytes.is_empty(), "render_docx output must be non-empty");
    assert!(bytes.starts_with(b"PK\x03\x04"), "ZIP local header magic");
    assert!(
        bytes.ends_with(b"PK\x05\x06") == false,
        "EOCD has trailing comment length field"
    );
    assert!(
        bytes.windows(4).rposition(|w| w == b"PK\x05\x06").is_some(),
        "EOCD signature present"
    );
    let entries = qc_zip_entries(&bytes).expect("container parses cleanly");
    let names: Vec<&str> = entries.iter().map(|(n, _, _)| n.as_str()).collect();
    assert!(
        names.contains(&"[Content_Types].xml"),
        "[Content_Types].xml part present: {names:?}"
    );
    assert!(
        names.contains(&"_rels/.rels"),
        "_rels/.rels part present: {names:?}"
    );
    assert!(
        names.contains(&"word/document.xml"),
        "word/document.xml part present: {names:?}"
    );
    // stored 方法面：全条目 compressed == uncompressed（qc_zip_entries 内断）。
    assert_eq!(names.len(), 3, "minimal OOXML package = 3 parts");
}

/// 判据 6：逐条目 CRC32 全对 + 内容词形 grep（中文/finding_id/rtis_ref）。
#[test]
fn render_docx_crc_and_content_words() {
    let ir = parse_main();
    let bytes = render_docx(&ir);
    let entries = qc_zip_entries(&bytes).expect("CRC32 all entries pass");
    for (name, _, size) in &entries {
        assert!(*size > 0, "part {name} non-empty");
    }
    // word/document.xml 提取（最后一条目——条目序对账）。
    let part = document_part(&bytes);
    let doc = std::str::from_utf8(&part).expect("document.xml is UTF-8");
    // 中文原样（title 与 finding title——XML UTF-8 直存）。
    assert!(doc.contains(&ir.title), "title zh bytes in document.xml");
    for finding in &ir.findings {
        assert!(
            doc.contains(&finding.finding_id),
            "finding_id {} in document.xml",
            finding.finding_id
        );
        assert!(
            doc.contains(&finding.title),
            "finding title zh in document.xml"
        );
        if let Some(rtis_ref) = &finding.rtis_ref {
            // rtis:// 词形原样透传（红线 2——零解析零改写）。
            assert!(
                doc.contains(rtis_ref),
                "rtis_ref {rtis_ref} verbatim in document.xml"
            );
        }
        // evidence 六 ID 投影（evidence_id 词形 grep——D6① 事实可追溯面）。
        for evidence in &finding.evidence {
            assert!(
                doc.contains(&evidence.evidence_id),
                "evidence_id {} in document.xml",
                evidence.evidence_id
            );
        }
    }
    // [Content_Types].xml 声明面。
    let bytes_str = String::from_utf8_lossy(&bytes);
    assert!(
        bytes_str.contains("wordprocessingml.document.main+xml"),
        "document.xml content-type declared"
    );
}

/// 从容器字节提取 word/document.xml 数据段（自解析——判据 6 断言面）。
fn document_part(bytes: &[u8]) -> Vec<u8> {
    let eocd_sig = b"PK\x05\x06";
    let eocd_pos = bytes
        .windows(4)
        .rposition(|w| w == eocd_sig)
        .expect("EOCD present");
    let cd_offset = u32::from_le_bytes([
        bytes[eocd_pos + 16],
        bytes[eocd_pos + 17],
        bytes[eocd_pos + 18],
        bytes[eocd_pos + 19],
    ]) as usize;
    // central directory 逐条目遍历，匹配 word/document.xml。
    let mut pos = cd_offset;
    while bytes[pos..].starts_with(b"PK\x01\x02") {
        let name_len = u16::from_le_bytes([bytes[pos + 28], bytes[pos + 29]]) as usize;
        let extra_len = u16::from_le_bytes([bytes[pos + 30], bytes[pos + 31]]) as usize;
        let comment_len = u16::from_le_bytes([bytes[pos + 32], bytes[pos + 33]]) as usize;
        let uncompressed = u32::from_le_bytes([
            bytes[pos + 24],
            bytes[pos + 25],
            bytes[pos + 26],
            bytes[pos + 27],
        ]) as usize;
        let local_offset = u32::from_le_bytes([
            bytes[pos + 42],
            bytes[pos + 43],
            bytes[pos + 44],
            bytes[pos + 45],
        ]) as usize;
        let name = std::str::from_utf8(&bytes[pos + 46..pos + 46 + name_len]).expect("name UTF-8");
        if name == "word/document.xml" {
            let local_name_len =
                u16::from_le_bytes([bytes[local_offset + 26], bytes[local_offset + 27]]) as usize;
            let local_extra_len =
                u16::from_le_bytes([bytes[local_offset + 28], bytes[local_offset + 29]]) as usize;
            let data_start = local_offset + 30 + local_name_len + local_extra_len;
            return bytes[data_start..data_start + uncompressed].to_vec();
        }
        pos += 46 + name_len + extra_len + comment_len;
    }
    panic!("word/document.xml part not found");
}

/// 确定性：同 IR 两次渲染字节一致。
#[test]
fn render_docx_deterministic() {
    let ir = parse_main();
    let a = render_docx(&ir);
    let b = render_docx(&ir);
    assert_eq!(a, b, "render_docx must be byte-deterministic");
}

/// mixed fixture 同过（5 Finding 五态覆盖——判据 6 mixed 面）。
#[test]
fn render_docx_mixed_fixture() {
    let ir = parse_mixed();
    let bytes = render_docx(&ir);
    let entries = qc_zip_entries(&bytes).expect("CRC32 all entries pass");
    assert_eq!(entries.len(), 3);
    let part = document_part(&bytes);
    let doc = std::str::from_utf8(&part).expect("document.xml is UTF-8");
    for finding in &ir.findings {
        assert!(doc.contains(&finding.finding_id));
    }
}

/// 负例面（判据 8 预置）：单字节翻转 → CRC 对账 fail（qc_docx fail 面
/// 底座——message 非空）。
#[test]
fn render_docx_crc_tamper_fails() {
    let ir = parse_main();
    let mut bytes = render_docx(&ir);
    // 翻转 word/document.xml 数据段的一个字节（容器尾段——条目数据面）。
    let mid = bytes.len() / 2;
    bytes[mid] ^= 0xFF;
    let err = qc_zip_entries(&bytes)
        .err()
        .expect("tampered container must fail CRC/structure reconciliation");
    assert!(
        !err.is_empty(),
        "error message must be non-empty (:545 保留错误信息词形)"
    );
    assert!(
        count_sub(&bytes, b"PK\x05\x06") >= 1,
        "EOCD signature present"
    );
}
