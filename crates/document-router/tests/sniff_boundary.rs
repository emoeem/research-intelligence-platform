//! DI-1 嗅探边界套件（S6；D6 表 fixture 语料逐项对照 + 反例三类 + 手写
//! ZIP 扫描边界）。语料来自入库 fixtures（tests/fixtures/，byte 级同源
//! 副本来自 rip-data/parse-smoke/in/；sha256 钉值见 fixture_pins.rs——
//! 评审修订 2026-11-19：rip-data 被根 .gitignore 排除不入库，语料入库后
//! CI fresh checkout 不再依赖本机运行时数据）；魔数权威、扩展名仅提示（D6）。

use std::path::{Path, PathBuf};

use document_router::channel::Channel;
use document_router::sniff::{mismatch, sniff, Magic, OfficeSubtype};

fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn smoke(name: &str) -> Vec<u8> {
    std::fs::read(fixtures_dir().join(name)).unwrap_or_else(|e| panic!("{name}: {e}"))
}

/// D6 表逐行：fixture 语料判定的通道面（可路由 / 显式不路由）。
#[test]
fn d6_table_fixture_corpus_routes_exactly() {
    // pdf / png → 可路由。
    assert_eq!(sniff(&smoke("sample.pdf")), Magic::Pdf);
    assert_eq!(
        sniff(&smoke("sample.pdf")).routed_channel(),
        Some(Channel::Pdf)
    );
    assert_eq!(sniff(&smoke("sample.png")), Magic::Png);
    assert_eq!(
        sniff(&smoke("sample.png")).routed_channel(),
        Some(Channel::Image)
    );
    // OOXML 三子型 marker 全命中（word/document.xml / xl/workbook.xml /
    // ppt/presentation.xml）。
    assert_eq!(
        sniff(&smoke("sample.docx")),
        Magic::ZipOoxml(OfficeSubtype::Docx)
    );
    assert_eq!(
        sniff(&smoke("sample.xlsx")),
        Magic::ZipOoxml(OfficeSubtype::Xlsx)
    );
    assert_eq!(
        sniff(&smoke("sample.pptx")),
        Magic::ZipOoxml(OfficeSubtype::Pptx)
    );
    assert_eq!(
        sniff(&smoke("sample.docx")).routed_channel(),
        Some(Channel::Office)
    );
    // zip_other 三类（epub 首条目 mimetype / zip docs/a.txt / evil.zip 穿越
    // 名）→ DI-1 显式不支持。
    for name in ["sample.epub", "sample.zip", "evil.zip"] {
        assert_eq!(sniff(&smoke(name)), Magic::ZipOther, "{name}");
        assert!(sniff(&smoke(name)).routed_channel().is_none(), "{name}");
    }
    // text（md/txt/html 全量 UTF-8 有效）→ DI-1 显式不路由（走 rip.doc.parse）。
    for name in ["clean.md", "sample.txt", "sample.html", "sample.md"] {
        assert_eq!(sniff(&smoke(name)), Magic::Text, "{name}");
        assert!(sniff(&smoke(name)).routed_channel().is_none(), "{name}");
    }
}

/// 伪 .pdf（纯文本体）→ text 签名权威拒绝（扩展名仅提示，不改变嗅探）。
#[test]
fn fake_pdf_extension_yields_text_signature() {
    let fake = b"not a pdf at all, just text pretending\n".to_vec();
    assert_eq!(sniff(&fake), Magic::Text);
    // mismatch 判定不适用（text 不路由；嗅探结果本身不受支持 → 拒绝）。
    assert!(!mismatch(&Magic::Text, "pdf"));
}

/// jpeg 魔数（fixture 由 magick 本地生成，零下载；magick 缺席跳过）。
#[test]
fn jpeg_magic_detected_on_locally_generated_fixture() {
    let dir = std::env::temp_dir().join(format!("di1-sniff-jpeg-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let out = dir.join("gen.jpg");
    let ok = std::process::Command::new("/usr/bin/magick")
        .args(["-size", "8x8", "xc:red", out.to_str().unwrap()])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    if !ok {
        println!("SKIP: /usr/bin/magick unavailable for local JPEG fixture");
        let _ = std::fs::remove_dir_all(&dir);
        return;
    }
    let bytes = std::fs::read(&out).unwrap();
    assert_eq!(bytes[..3], [0xFF, 0xD8, 0xFF], "fixture 为 JPEG 魔数形态");
    assert_eq!(sniff(&bytes), Magic::Jpeg);
    assert_eq!(sniff(&bytes).routed_channel(), Some(Channel::Image));
    let _ = std::fs::remove_dir_all(&dir);
}

/// 未知魔数 / 非 UTF-8 二进制 → unknown（INPUT 拒绝）；控制字节序列属
/// UTF-8 有效面 → text（D6 判定序：UTF-8 有效即 text，text 走显式不路由
/// 拒绝——两态都在嗅探门拒绝，不可路由性一致）。
#[test]
fn unknown_magic_and_non_utf8_binary() {
    // 0xFF 非 UTF-8 起始字节 → unknown。
    assert_eq!(sniff(&[0xFF, 0xFE, 0x00, 0x01]), Magic::Unknown);
    assert!(sniff(&[0xFF, 0xFE, 0x00, 0x01]).routed_channel().is_none());
    // 0x00/0x01/0x02 是合法 UTF-8（控制字符）→ text（D6 判定序）。
    assert_eq!(sniff(&[0x00, 0x01, 0x02]), Magic::Text);
    assert!(sniff(&[0x00, 0x01, 0x02]).routed_channel().is_none());
    // UTF-8 有效但带魔数前缀的优先序：魔数先于 UTF-8 判定。
    let pdf_then_text = b"%PDF-1.7\nnot really a pdf body".to_vec();
    assert_eq!(sniff(&pdf_then_text), Magic::Pdf);
}

/// xlsx 首条目反例钉值断言（D6 否决项的实证前提）：首条目 docProps/app.xml
/// 非 [Content_Types].xml，首条目规则将误判 zip_other。
#[test]
fn xlsx_first_entry_counterexample_pin() {
    let bytes = smoke("sample.xlsx");
    let namelen = u16::from_le_bytes([bytes[26], bytes[27]]) as usize;
    let first = std::str::from_utf8(&bytes[30..30 + namelen]).unwrap();
    assert_eq!(
        first, "docProps/app.xml",
        "反例前提：xlsx 首条目非 [Content_Types].xml"
    );
    assert_eq!(sniff(&bytes), Magic::ZipOoxml(OfficeSubtype::Xlsx));
}

/// 手写 ZIP 扫描 fail-closed 边界：data-descriptor 形态（csize=0 + bit3）
/// 终止不猜测；截断头部终止；非 LFH 数据区终止。
#[test]
fn zip_scan_fail_closed_edges() {
    // descriptor 形态：首条目后无法安全跳转 → 已见条目内未命中 → zip_other。
    let mut z: Vec<u8> = Vec::new();
    z.extend_from_slice(&[0x50, 0x4B, 0x03, 0x04]);
    z.extend_from_slice(&20u16.to_le_bytes()); // version
    z.extend_from_slice(&0x8u16.to_le_bytes()); // flags bit3
    z.extend_from_slice(&0u16.to_le_bytes()); // method
    z.extend_from_slice(&0u32.to_le_bytes()); // time+date
    z.extend_from_slice(&0u32.to_le_bytes()); // crc
    z.extend_from_slice(&0u32.to_le_bytes()); // csize（未知）
    z.extend_from_slice(&0u32.to_le_bytes()); // usize
    z.extend_from_slice(&5u16.to_le_bytes()); // namelen
    z.extend_from_slice(&0u16.to_le_bytes()); // extralen
    z.extend_from_slice(b"a.bin");
    z.extend_from_slice(b"\x50\x4b\x03\x04[Content_Types].xml-later");
    assert_eq!(sniff(&z), Magic::ZipOther, "descriptor 形态 fail-closed");

    // 截断头部（namelen 越界）→ 终止定论。
    let mut z2: Vec<u8> = Vec::new();
    z2.extend_from_slice(&[0x50, 0x4B, 0x03, 0x04]);
    z2.extend_from_slice(&[0u8; 22]);
    z2.extend_from_slice(&200u16.to_le_bytes()); // namelen 越界
    z2.extend_from_slice(&0u16.to_le_bytes());
    z2.extend_from_slice(b"cut");
    assert_eq!(sniff(&z2), Magic::ZipOther);

    // LFH 签名后接非 ZIP 体（PDF 名伪装）→ 扫描终止 → zip_other（拒绝）。
    let mut z3 = vec![0x50u8, 0x4B, 0x03, 0x04];
    z3.extend_from_slice(b"%PDF-1.7 fake body not a zip");
    assert_eq!(sniff(&z3), Magic::ZipOther);
    assert!(sniff(&z3).routed_channel().is_none());
}

/// mismatch 判定矩阵（D6：魔数权威；不一致不拒绝、以嗅探为准路由）。
#[test]
fn mismatch_matrix_ext_hint_vs_sniff() {
    let png: Vec<u8> = vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
    // 判据 9 形态：.pdf 扩展名 + PNG 魔数 → mismatch。
    assert!(mismatch(&sniff(&png), "pdf"));
    assert!(mismatch(&sniff(&png), "PDF"), "扩展名大小写不敏感");
    // 一致形态。
    assert!(!mismatch(&Magic::Pdf, "pdf"));
    assert!(!mismatch(&Magic::ZipOoxml(OfficeSubtype::Xlsx), "xlsx"));
    assert!(!mismatch(&Magic::ZipOoxml(OfficeSubtype::Pptx), "pptx"));
    assert!(!mismatch(&Magic::Jpeg, "jpg"));
    assert!(!mismatch(&Magic::Jpeg, "jpeg"));
    // 扩展名无提示 → 无 mismatch 可言。
    assert!(!mismatch(&Magic::Png, ""));
    assert!(!mismatch(&Magic::Pdf, "weird"));
    // md 提示（rip.doc.parse 面）+ 图像魔数 → mismatch（路由图像，提示失真）。
    assert!(mismatch(&sniff(&png), "md"));
}
