//! render_pdf 判据面（PHASE-6B 判据 4/5）。
//!
//! 门禁口径（S3-S7）：render_pdf 纯函数零 I/O——字体字节经 [`FontSource`]
//! 参数注入（e2e 读系统字体文件传入——D2③）；系统无 CJK 字体时中文判据
//! SKIP 分开计数（D2⑤——document_domain.rs:21 skip 词形先例）。
//!
//! 对账面：
//! - 字节非空 + `%PDF-` 头 + `%%EOF` 尾；
//! - `/Type /Page` 计数 = Pages `/Count`（排版预期页数一致性——判据 5）；
//! - CJK 字体嵌入 `/FontFile2`（本机 A9 在位态；skip 守卫分计）；
//! - finding_id 经 DocumentInfo `/Subject` 字面 grep；
//! - title 中文经 ToUnicode CMap UTF-16BE hex 词形可见；
//! - 同 IR 同 fonts 两次渲染字节一致（确定性——判据 9 语义延续）。

use report_engine::{discover_system_fonts, render_pdf, FontSource, ReportIr};

const MAIN_REPORT_JSON: &str = include_str!("fixtures/main-report.json");

/// 解析 + 校验 fixture（校验不过即 panic——fixture 是判据 4 正例面）。
fn parse_main() -> ReportIr {
    let value: serde_json::Value =
        serde_json::from_str(MAIN_REPORT_JSON).expect("main-report.json is valid JSON");
    report_engine::from_json(&value).expect("main-report fixture passes validate_ir")
}

/// 系统字体面 skip 门（D2⑤）：零可解析字体时整面 SKIP（计数分立）。
fn fonts_or_skip() -> Option<Vec<FontSource>> {
    let fonts = discover_system_fonts();
    if fonts.is_empty() {
        eprintln!("SKIP: 系统字体目录零可解析字体（字体面 SKIP 分计——D2⑤）");
        return None;
    }
    Some(fonts)
}

/// CJK 字体在位探测（判据 5 CJK 嵌入面 skip 守卫——A9 本机在位态）。
fn has_cjk_face(fonts: &[FontSource]) -> bool {
    fonts
        .iter()
        .any(|source| skrifa_probe_has_cjk(&source.data, source.index))
}

/// skrifa cmap CJK 探测（render_pdf 同款锚点——3 面独立）。
fn skrifa_probe_has_cjk(data: &[u8], index: u32) -> bool {
    use skrifa::MetadataProvider;
    match skrifa::FontRef::from_index(data, index) {
        Ok(font) => {
            let charmap = font.charmap();
            charmap.map('\u{4E00}').is_some()
                && charmap.map('\u{6C34}').is_some()
                && charmap.map('\u{9FA5}').is_some()
        }
        Err(_) => false,
    }
}

/// `/Type /Page` 计数（字节面 grep——判据 5 排版页数对账位）。
fn count_type_page(bytes: &[u8]) -> usize {
    bytes
        .windows(b"/Type /Page".len())
        .filter(|w| w == b"/Type /Page")
        .count()
}

/// 子串在字节面出现的次数。
fn count_sub(bytes: &[u8], needle: &[u8]) -> usize {
    bytes.windows(needle.len()).filter(|w| *w == needle).count()
}

/// 判据 5：fixture IR → PDF 字节非空 + 头尾锚点 + 页数对账。
#[test]
fn render_pdf_fixture_bytes_and_page_count() {
    let Some(fonts) = fonts_or_skip() else { return };
    let ir = parse_main();
    let bytes = render_pdf(&ir, &fonts);
    assert!(!bytes.is_empty(), "render_pdf output must be non-empty");
    assert!(
        bytes.starts_with(b"%PDF-"),
        "PDF magic header: {:?}",
        &bytes[..8.min(bytes.len())]
    );
    assert!(bytes.ends_with(b"%%EOF"), "PDF trailer must end with %%EOF");
    let page_count = count_type_page(&bytes);
    assert!(page_count >= 1, "at least one page object");
}

/// 判据 5 CJK 面：CJK 字体在位时 /FontFile2 嵌入 + finding_id/中文可追溯。
#[test]
fn render_pdf_cjk_embed_and_traceability() {
    let Some(fonts) = fonts_or_skip() else { return };
    if !has_cjk_face(&fonts) {
        eprintln!("SKIP: 系统无 CJK 字体（qc 中文判据 SKIP 分计——D2⑤）");
        return;
    }
    let ir = parse_main();
    let bytes = render_pdf(&ir, &fonts);
    assert!(
        count_sub(&bytes, b"/FontFile2") >= 1,
        "CJK face embedded via /FontFile2"
    );
    // finding_id（ASCII）经 DocumentInfo /Subject 字面可见。
    let finding_ids: Vec<&str> = ir.findings.iter().map(|f| f.finding_id.as_str()).collect();
    for id in &finding_ids {
        assert!(
            count_sub(&bytes, id.as_bytes()) >= 1,
            "finding_id {id} must appear in PDF bytes"
        );
    }
    // title 中文经 ToUnicode CMap UTF-16BE hex 词形（U+793A → "793A"）。
    for c in ir.title.chars() {
        if (c as u32) > 0x7F {
            let hex = format!("{:04X}", c as u32);
            assert!(
                count_sub(&bytes, hex.as_bytes()) >= 1,
                "ToUnicode UTF-16BE hex {hex} (char {}) must appear",
                c
            );
        }
    }
}

/// 确定性：同 IR 同 fonts 两次渲染字节一致（判据 9 语义——禁每格式重跑 AI）。
#[test]
fn render_pdf_deterministic() {
    let Some(fonts) = fonts_or_skip() else { return };
    let ir = parse_main();
    let a = render_pdf(&ir, &fonts);
    let b = render_pdf(&ir, &fonts);
    assert_eq!(a, b, "render_pdf must be byte-deterministic");
}

/// 红线 1 边界：零字体面（空 fonts 切片）不 panic，仍产出 PDF 结构面。
#[test]
fn render_pdf_empty_fonts_no_panic() {
    let ir = parse_main();
    let bytes = render_pdf(&ir, &[]);
    assert!(!bytes.is_empty(), "degraded render still produces bytes");
    assert!(bytes.starts_with(b"%PDF-"));
}
