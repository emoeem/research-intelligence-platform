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
    // 名）→ DI-2 ADR D1 行为变更：改路由归档通道（两阶段四闸承接）。
    for name in ["sample.epub", "sample.zip", "evil.zip"] {
        assert_eq!(sniff(&smoke(name)), Magic::ZipOther, "{name}");
        assert_eq!(
            sniff(&smoke(name)).routed_channel(),
            Some(Channel::Archive),
            "{name} 按 DI-2 D1 路由归档通道"
        );
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

// ---------------------------------------------------------------------------
// DI-2 ADR D1 八新签名（精确字节判定）+ 不路由面 fail-closed 维持
// ---------------------------------------------------------------------------

/// D1 表八签名逐行（2026-11-20 hexdump 实测判据）：7z / wav / avi / mp4 /
/// webm / ogg / flac / mp3。
#[test]
fn di2_eight_new_signatures_route_exactly() {
    use document_router::sniff::Magic as M;
    // 7z：头 6 字节 37 7A BC AF 27 1C（demo.7z 实测）→ 归档。
    let seven_zip = [0x37u8, 0x7A, 0xBC, 0xAF, 0x27, 0x1C, 0x00, 0x02];
    assert_eq!(sniff(&seven_zip), M::SevenZip);
    assert_eq!(sniff(&seven_zip).routed_channel(), Some(Channel::Archive));
    // wav：RIFF + @8 WAVE → 媒体；RIFF 但 @8 非 WAVE/AVI → 不判 wav/avi。
    let wav = b"RIFF\x24\x00\x00\x00WAVEfmt \x10\x00".to_vec();
    assert_eq!(sniff(&wav), M::Wav);
    assert_eq!(sniff(&wav).routed_channel(), Some(Channel::Media));
    // avi：RIFF + @8 `AVI `（尾随空格实测判据）→ 媒体。
    let avi = b"RIFF\x00\x01\x00\x00AVI LISTxxxx".to_vec();
    assert_eq!(sniff(&avi), M::Avi);
    assert_eq!(sniff(&avi).routed_channel(), Some(Channel::Media));
    // RIFF+WEBP 形态不收（D1 否决项：图像 magick 面留 DI-3）。全 ASCII+NUL
    // 体是合法 UTF-8（NUL 为合法码点，D6 判定序）→ 落 Text → 同在嗅探门
    // 拒绝（不可路由性一致）；含非 UTF-8 字节的 WEBP 体落 Unknown。
    let webp_ascii = b"RIFF\x00\x00\x00\x00WEBPVP8 ".to_vec();
    assert_eq!(sniff(&webp_ascii), M::Text);
    assert!(sniff(&webp_ascii).routed_channel().is_none());
    let webp_binary = b"RIFF\x00\x00\x00\x00WEBPVP8 \xff\xfe".to_vec();
    assert_eq!(sniff(&webp_binary), M::Unknown);
    assert!(sniff(&webp_binary).routed_channel().is_none());
    // mp4：4-7 字节 ftyp（small.mp4 实测 ....ftypisom）→ 媒体。
    let mp4 = b"\x00\x00\x00\x18ftypisom\x00\x00\x02\x00isomiso2".to_vec();
    assert_eq!(sniff(&mp4), M::Mp4);
    assert_eq!(sniff(&mp4).routed_channel(), Some(Channel::Media));
    // webm：EBML 头 1A 45 DF A3（v.mkv 实测）→ 媒体。
    let webm = [0x1Au8, 0x45, 0xDF, 0xA3, 0x01, 0x00, 0x00, 0x00];
    assert_eq!(sniff(&webm), M::Webm);
    assert_eq!(sniff(&webm).routed_channel(), Some(Channel::Media));
    // ogg / flac / mp3（ID3 头形态；帧同步 FF FB 启发式不收，D1 否决项）。
    for (bytes, want) in [
        (b"OggS\x00\x02\x00\x00".to_vec(), M::Ogg),
        (b"fLaC\x00\x00\x00\x22".to_vec(), M::Flac),
        (b"ID3\x04\x00\x00\x00\x00\x00\x00".to_vec(), M::Mp3),
    ] {
        assert_eq!(sniff(&bytes), want);
        assert_eq!(sniff(&bytes).routed_channel(), Some(Channel::Media));
    }
    // 帧同步形态不判 mp3（启发式误判率高，D1）：FF FB 非 UTF-8 → Unknown。
    let frame_sync = [0xFFu8, 0xFB, 0x90, 0x00];
    assert_eq!(sniff(&frame_sync), M::Unknown);
}

/// tar/gzip/Rar 不入嗅探表：本批 fail-closed INPUT 拒绝（D1——归档通道
/// 本批仅承载 zip + 7z 两格式；tar @257 ustar 与 gzip 1F 8B 实测可判但
/// 不路由，落 Unknown）。
#[test]
fn tar_gzip_family_stays_fail_closed() {
    // gzip 魔数 1F 8B（a.txt.gz 实测）→ Unknown → 无通道。
    let gz = [0x1Fu8, 0x8B, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00];
    assert_eq!(sniff(&gz), Magic::Unknown);
    assert!(sniff(&gz).routed_channel().is_none());
    // tar @257 ustar（t.tar 实测判据）：NUL 是合法 UTF-8 码点——全 ASCII
    // tar 头本身落 Text，含二进制内容字节（0xFF）的 tar 落 Unknown；两态
    // 都在嗅探门 INPUT 拒绝（不可路由性一致，判据 12 零 run 行）。
    let mut tar_ascii = vec![0u8; 512];
    tar_ascii[..4].copy_from_slice(b"name");
    tar_ascii[257..262].copy_from_slice(b"ustar");
    assert_eq!(sniff(&tar_ascii), Magic::Text);
    assert!(sniff(&tar_ascii).routed_channel().is_none());
    let mut tar_binary = tar_ascii.clone();
    tar_binary[512 - 1] = 0xFF; // 文件数据区非 UTF-8 字节（真实 tar 常态）
    assert_eq!(sniff(&tar_binary), Magic::Unknown);
    assert!(sniff(&tar_binary).routed_channel().is_none());
    // rar 魔数（Rar!）不入表：真实 rar 体含二进制字节（0xFF）→ Unknown；
    // 纯 ASCII 头形态落 Text——两态都在嗅探门拒绝（DI-2 D1：本批不路由）。
    let rar = b"Rar!\x1a\x07\x01\x00\xff\xfe\x03".to_vec();
    assert_eq!(sniff(&rar), Magic::Unknown);
    assert!(sniff(&rar).routed_channel().is_none());
}

/// DI-2 判据 12 后半：Rar/7za/7zr 面在通道三源文件 grep 零命中（路由面
/// 只承载 zip + 7z；7za/7zr 死 Manifest 否决，D7）。
#[test]
fn no_rar_7za_7zr_route_tokens_in_channel_sources() {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    for file in ["sniff.rs", "channel.rs", "router.rs"] {
        let src = std::fs::read_to_string(manifest_dir.join(file))
            .unwrap_or_else(|e| panic!("{file}: {e}"));
        for token in ["7za", "7zr", "rar"] {
            assert!(
                !src.contains(token),
                "{file} 不得出现 {token:?} 路由面（DI-2 ADR D1/D7：归档通道仅 zip+7z）"
            );
        }
    }
}

/// 新通道扩展名提示面：媒体/归档 ext 家族进 mismatch 矩阵（魔数权威）。
#[test]
fn mismatch_covers_media_and_archive_families() {
    let wav = b"RIFF\x24\x00\x00\x00WAVEfmt ".to_vec();
    // .wav 提示与 wav 魔数一致 → 无 mismatch。
    assert!(!mismatch(&sniff(&wav), "wav"));
    // .txt 提示 + wav 魔数 → mismatch（嗅探权威路由媒体 + 提示失真）。
    assert!(mismatch(&sniff(&wav), "txt"));
    // 7z 魔数 + .zip 提示 → mismatch（同为归档家族才不报；7z≠zip 家族内
    // 子型不细分，同族一致 → 无 mismatch）。
    let seven_zip = [0x37u8, 0x7A, 0xBC, 0xAF, 0x27, 0x1C, 0x00, 0x02];
    assert!(!mismatch(&sniff(&seven_zip), "zip"));
    assert!(!mismatch(&sniff(&seven_zip), "7z"));
    // zip_other + .zip → 归档家族一致。
    assert!(!mismatch(&Magic::ZipOther, "zip"));
    // zip_other + .pdf → mismatch。
    assert!(mismatch(&Magic::ZipOther, "pdf"));
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

    // LFH 签名后接非 ZIP 体（PDF 名伪装）→ 扫描终止 → zip_other（DI-2 起
    // 路由归档通道，四闸在其内做防护）。
    let mut z3 = vec![0x50u8, 0x4B, 0x03, 0x04];
    z3.extend_from_slice(b"%PDF-1.7 fake body not a zip");
    assert_eq!(sniff(&z3), Magic::ZipOther);
    assert_eq!(sniff(&z3).routed_channel(), Some(Channel::Archive));
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
