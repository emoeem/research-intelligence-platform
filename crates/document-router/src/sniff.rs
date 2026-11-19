//! 魔数嗅探（DI-1 ADR D6 + DI-2 ADR D1 八签名扩展）：手写判定表，零第三方
//! MIME/魔数依赖（禁 mime_guess/file-type/infer 类，红线 1）。
//!
//! 判定序（D6 表 + D1 扩展行）：
//! 1. 头 4 字节 `%PDF-` → pdf；
//! 2. 头 8 字节 `89 50 4E 47 0D 0A 1A 0A` → png；
//! 3. 头 3 字节 `FF D8 FF` → jpeg；
//! 4. 头 4 字节 `PK\x03\x04` → ZIP：条目名顺序扫描命中 `[Content_Types].xml`
//!    → zip_ooxml（子型按 marker 条目 word/document.xml→docx、
//!    xl/workbook.xml→xlsx、ppt/presentation.xml→pptx）；未命中 → zip_other；
//! 5. 头 6 字节 `37 7A BC AF 27 1C` → 7z（D1 归档）；
//! 6. 头 4 字节 `RIFF` 且 8-11 字节 `WAVE` → wav / `AVI ` → avi（D1 媒体；
//!    RIFF+`WEBP` 不收——图像 magick 面留 DI-3，D1 否决项）；
//! 7. 4-7 字节 `ftyp` → mp4（D1 媒体）；
//! 8. 头 4 字节 `1A 45 DF A3`（EBML）→ webm / `OggS` → ogg / `fLaC` → flac /
//!    `ID3` → mp3（D1 媒体）；
//! 9. 全量 UTF-8 有效 → text；
//! 10. 其余 → unknown（tar/gzip/Rar 等本批不路由面落此——fail-closed INPUT
//!     拒绝，D1：归档通道本批仅承载 zip + 7z 两格式）。
//!
//! **ZipOther 路由变更（D1 行为变更登记）**：DI-1 中 zip_other 显式不支持
//! （归档两阶段面属 DI-3）；DI-2 起归档通道入列（rip.archive.list/extract
//! 两阶段四闸），zip 是归档防护的第一频次输入 → `routed_channel()` 改路由
//! 归档通道（DI-1 ADR 历史文本不改写，行为面以 DI-2 ADR D1 为准）。
//!
//! **禁「首条目 = [Content_Types].xml」判定**（D6 否决项）：本机实测
//! sample.xlsx 首条目为 `docProps/app.xml`（tests/fixtures 内钉值样本反例），
//! 首条目规则会把合法 xlsx 误判 zip_other。条目扫描 = 顺序 local-file-header
//! 名遍历（上限 256 条目 + 4 MiB 读前缓冲，超限按 zip_other 处理 fail-closed），
//! 无需 EOCD 全解析。
//!
//! 权威序（D6）：魔数权威、扩展名仅提示。嗅探与扩展名可路由类型不一致 →
//! 以嗅探为准路由 + `mismatch=true`（不拒绝）；嗅探结果本身不受支持 → 拒绝。
//! 嗅探发生于任何子 run 提交之前（URL/MSL/MVG 等 coder 输入在结构上到不了
//! magick argv——E §4.1 policy 空壳的补偿面；媒体/归档 URL 输入同理无入口）。

/// 嗅探签名（D6 表判据行）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Magic {
    /// 头 `%PDF-`（PDF 通道）。
    Pdf,
    /// 头 `89 50 4E 47 0D 0A 1A 0A`（图像通道）。
    Png,
    /// 头 `FF D8 FF`（图像通道）。
    Jpeg,
    /// ZIP 且条目含 `[Content_Types].xml`（Office 通道；子型见
    /// [`OfficeSubtype`]）。
    ZipOoxml(OfficeSubtype),
    /// ZIP 且无 `[Content_Types].xml`（DI-2 起路由归档通道两阶段四闸——
    /// DI-1 ADR D6 表「显式不支持」行由 DI-2 ADR D1 取代，行为变更登记）。
    ZipOther,
    /// 头 `37 7A BC AF 27 1C`（7z 容器，DI-2 ADR D1 归档通道）。
    SevenZip,
    /// 头 `RIFF` 且 8-11 字节 `WAVE`（DI-2 ADR D1 媒体通道）。
    Wav,
    /// 头 `RIFF` 且 8-11 字节 `AVI `（DI-2 ADR D1 媒体通道）。
    Avi,
    /// 4-7 字节 `ftyp`（ISO-BMFF 容器：mp4/mov/m4a 等，DI-2 ADR D1 媒体通道）。
    Mp4,
    /// 头 `1A 45 DF A3`（EBML：webm/mkv，DI-2 ADR D1 媒体通道）。
    Webm,
    /// 头 `OggS`（DI-2 ADR D1 媒体通道）。
    Ogg,
    /// 头 `fLaC`（DI-2 ADR D1 媒体通道）。
    Flac,
    /// 头 `ID3`（mp3 ID3v2 头形态；帧同步 `FF FB` 启发式不收，D1 否决项）。
    Mp3,
    /// 全量 UTF-8 有效且无上述魔数（DI-1 显式不路由：走既有 rip.doc.parse，
    /// DI-0 §3 冻结不迁不扩，禁造第二套 md/txt/html 解析）。
    Text,
    /// 未知魔数 / 非 UTF-8 二进制（INPUT 拒绝）。
    Unknown,
}

impl Magic {
    /// 统一输出 Schema `source.sniff.magic` 字面量（D3 + D1 扩展；unknown/
    /// text 不进统一输出——均在嗅探门拒绝，此处仅供错误消息与测试引用）。
    pub fn as_str(&self) -> &'static str {
        match self {
            Magic::Pdf => "pdf",
            Magic::Png => "png",
            Magic::Jpeg => "jpeg",
            Magic::ZipOoxml(_) => "zip_ooxml",
            Magic::ZipOther => "zip_other",
            Magic::SevenZip => "7z",
            Magic::Wav => "wav",
            Magic::Avi => "avi",
            Magic::Mp4 => "mp4",
            Magic::Webm => "webm",
            Magic::Ogg => "ogg",
            Magic::Flac => "flac",
            Magic::Mp3 => "mp3",
            Magic::Text => "text",
            Magic::Unknown => "unknown",
        }
    }

    /// OOXML 子型字面量（office 通道 metadata.office_subtype 载体）。
    pub fn subtype_str(&self) -> &'static str {
        match self {
            Magic::ZipOoxml(sub) => sub.as_str(),
            _ => "",
        }
    }

    /// 嗅探签名可路由的通道（None = 不支持 / 不路由）。
    ///
    /// DI-2 ADR D1 行为变更：ZipOther → Archive（归档两阶段四闸承接）；
    /// 八新签名 → 7z 归档 + 七媒体签名；tar/gzip/Rar/unknown 维持
    /// fail-closed None（本批不路由，D1）。
    pub fn routed_channel(&self) -> Option<crate::channel::Channel> {
        match self {
            Magic::Pdf => Some(crate::channel::Channel::Pdf),
            Magic::Png | Magic::Jpeg => Some(crate::channel::Channel::Image),
            Magic::ZipOoxml(_) => Some(crate::channel::Channel::Office),
            Magic::ZipOther | Magic::SevenZip => Some(crate::channel::Channel::Archive),
            Magic::Wav
            | Magic::Avi
            | Magic::Mp4
            | Magic::Webm
            | Magic::Ogg
            | Magic::Flac
            | Magic::Mp3 => Some(crate::channel::Channel::Media),
            Magic::Text | Magic::Unknown => None,
        }
    }
}

/// OOXML 子型（D6 marker 条目判定）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OfficeSubtype {
    /// `word/document.xml`。
    Docx,
    /// `xl/workbook.xml`。
    Xlsx,
    /// `ppt/presentation.xml`。
    Pptx,
}

impl OfficeSubtype {
    /// 统一输出 Schema `metadata.office_subtype` 字面量（D3）。
    pub fn as_str(&self) -> &'static str {
        match self {
            OfficeSubtype::Docx => "docx",
            OfficeSubtype::Xlsx => "xlsx",
            OfficeSubtype::Pptx => "pptx",
        }
    }
}

/// 条目扫描上限（D6：上限 256 条目）。
const ZIP_MAX_ENTRIES: usize = 256;
/// 读前缓冲上限（D6：4 MiB；超限 fail-closed）。
const ZIP_MAX_SCAN_BYTES: usize = 4 * 1024 * 1024;

/// ZIP local-file-header 定长前缀（签名 4 + 版本 2 + 旗标 2 + 方法 2 +
/// 时间 2 + 日期 2 + crc 4 + csize 4 + usize 4 + namelen 2 + extralen 2）。
const ZIP_LFH_FIXED: usize = 30;
/// ZIP local-file-header 签名。
const ZIP_LFH_SIG: [u8; 4] = [0x50, 0x4B, 0x03, 0x04];

/// 条目名扫描（D6：顺序 local-file-header 名遍历，fail-closed）。
///
/// 返回 (是否命中 [Content_Types].xml, 子型 marker)。扫描终止条件：LFH 签名
/// 不符（到达 central directory / 数据描述符区）、条目数超限、读前缓冲超限、
/// 头部被截断、data-descriptor 形态（flags bit 3 置位且 LFH csize=0，无法
/// 按 csize 跳转）——任一终止即按已见条目定论（fail-closed：未命中即
/// zip_other）。
fn scan_zip_entries(bytes: &[u8]) -> (bool, Option<OfficeSubtype>) {
    let mut has_content_types = false;
    let mut subtype: Option<OfficeSubtype> = None;
    let mut off = 0usize;
    let mut entries = 0usize;
    let limit = bytes.len().min(ZIP_MAX_SCAN_BYTES);
    while entries < ZIP_MAX_ENTRIES && off + ZIP_LFH_FIXED <= limit {
        if bytes[off..off + 4] != ZIP_LFH_SIG {
            break;
        }
        let flags = u16::from_le_bytes([bytes[off + 6], bytes[off + 7]]);
        let csize = u32::from_le_bytes([
            bytes[off + 18],
            bytes[off + 19],
            bytes[off + 20],
            bytes[off + 21],
        ]);
        let namelen = u16::from_le_bytes([bytes[off + 26], bytes[off + 27]]) as usize;
        let extralen = u16::from_le_bytes([bytes[off + 28], bytes[off + 29]]) as usize;
        let name_start = off + ZIP_LFH_FIXED;
        if name_start + namelen > bytes.len() {
            break;
        }
        let name = &bytes[name_start..name_start + namelen];
        if name == b"[Content_Types].xml" {
            has_content_types = true;
        }
        match name {
            b"word/document.xml" if subtype.is_none() => {
                subtype = Some(OfficeSubtype::Docx);
            }
            b"xl/workbook.xml" if subtype.is_none() => {
                subtype = Some(OfficeSubtype::Xlsx);
            }
            b"ppt/presentation.xml" if subtype.is_none() => {
                subtype = Some(OfficeSubtype::Pptx);
            }
            _ => {}
        }
        entries += 1;
        // data-descriptor 形态（bit 3）且 LFH csize=0：压缩数据长度未知，
        // 无法安全跳到下一 LFH → 终止（fail-closed，不猜测）。
        if csize == 0 && (flags & 0x8) != 0 {
            break;
        }
        let advance = namelen + extralen + csize as usize;
        off = name_start + advance;
    }
    (has_content_types, subtype)
}

/// 嗅探主入口（D6 判定序 + D1 八签名扩展；全量字节在手——Router 已读入源文件）。
pub fn sniff(bytes: &[u8]) -> Magic {
    if bytes.starts_with(b"%PDF-") {
        return Magic::Pdf;
    }
    if bytes.starts_with(&[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]) {
        return Magic::Png;
    }
    if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return Magic::Jpeg;
    }
    if bytes.starts_with(&ZIP_LFH_SIG) {
        let (content_types, subtype) = scan_zip_entries(bytes);
        if content_types {
            return Magic::ZipOoxml(subtype.unwrap_or(OfficeSubtype::Docx));
        }
        return Magic::ZipOther;
    }
    // ---- DI-2 ADR D1 八新签名（精确字节判定，无启发式）----
    if bytes.starts_with(&[0x37, 0x7A, 0xBC, 0xAF, 0x27, 0x1C]) {
        return Magic::SevenZip;
    }
    if bytes.starts_with(b"RIFF") && bytes.len() >= 12 && &bytes[8..12] == b"WAVE" {
        return Magic::Wav;
    }
    if bytes.starts_with(b"RIFF") && bytes.len() >= 12 && &bytes[8..12] == b"AVI " {
        return Magic::Avi;
    }
    if bytes.len() >= 8 && &bytes[4..8] == b"ftyp" {
        return Magic::Mp4;
    }
    if bytes.starts_with(&[0x1A, 0x45, 0xDF, 0xA3]) {
        return Magic::Webm;
    }
    if bytes.starts_with(b"OggS") {
        return Magic::Ogg;
    }
    if bytes.starts_with(b"fLaC") {
        return Magic::Flac;
    }
    if bytes.starts_with(b"ID3") {
        return Magic::Mp3;
    }
    if std::str::from_utf8(bytes).is_ok() {
        return Magic::Text;
    }
    Magic::Unknown
}

/// 扩展名提示家族（D6：扩展名仅提示；小写化后匹配）。
/// 返回 None = 扩展名不承载可路由类型提示（与嗅探结果无矛盾可言）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExtFamily {
    Pdf,
    Office,
    Image,
    /// zip/7z 归档面（DI-2 D1）。
    Archive,
    /// wav/avi/mp4/webm/ogg/flac/mp3 媒体面（DI-2 D1）。
    Media,
    /// md/txt/html 属 rip.doc.parse 面（DI-0 §3 冻结）；DI-1 不路由但作
    /// mismatch 提示对照（PNG 名 .md → 嗅探图像权威路由 + mismatch）。
    Text,
}

fn ext_family(ext: &str) -> Option<ExtFamily> {
    match ext.to_ascii_lowercase().as_str() {
        "pdf" => Some(ExtFamily::Pdf),
        "docx" | "xlsx" | "pptx" => Some(ExtFamily::Office),
        "png" | "jpg" | "jpeg" => Some(ExtFamily::Image),
        "zip" | "7z" => Some(ExtFamily::Archive),
        "wav" | "avi" | "mp4" | "webm" | "ogg" | "flac" | "mp3" => Some(ExtFamily::Media),
        "md" | "txt" | "html" => Some(ExtFamily::Text),
        _ => None,
    }
}

fn channel_family(c: crate::channel::Channel) -> ExtFamily {
    match c {
        crate::channel::Channel::Pdf => ExtFamily::Pdf,
        crate::channel::Channel::Office => ExtFamily::Office,
        crate::channel::Channel::Image => ExtFamily::Image,
        crate::channel::Channel::Ocr => ExtFamily::Image,
        crate::channel::Channel::Media => ExtFamily::Media,
        crate::channel::Channel::Archive => ExtFamily::Archive,
    }
}

/// 嗅探与扩展名不一致判定（D6：魔数权威，扩展名仅提示；不一致以嗅探为准
/// 路由 + mismatch=true + warnings 记录，不拒绝）。
pub fn mismatch(magic: &Magic, ext: &str) -> bool {
    match (magic.routed_channel(), ext_family(ext)) {
        (Some(routed), Some(hint)) => channel_family(routed) != hint,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::channel::Channel;

    #[test]
    fn magic_head_bytes_decide() {
        assert_eq!(sniff(b"%PDF-1.7\n..."), Magic::Pdf);
        let png: Vec<u8> = vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 1, 2, 3];
        assert_eq!(sniff(&png), Magic::Png);
        let jpeg: Vec<u8> = vec![0xFF, 0xD8, 0xFF, 0xE0, 1, 2];
        assert_eq!(sniff(&jpeg), Magic::Jpeg);
        assert_eq!(sniff(b"hello world\n"), Magic::Text);
        assert_eq!(sniff(&[0x00, 0x01, 0x02, 0xFE]), Magic::Unknown);
    }

    /// D6 反例钉值：xlsx 首条目 `docProps/app.xml`（非 [Content_Types].xml），
    /// 首条目规则会误判 zip_other；条目扫描必须正确判 zip_ooxml/xlsx。
    /// 语料 = 入库 fixtures（评审修订 2026-11-19：rip-data 不入库，语料改读
    /// tests/fixtures/，钉值见 tests/fixture_pins.rs）。
    #[test]
    fn xlsx_first_entry_counterexample_routes_ooxml_xlsx() {
        let path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sample.xlsx");
        let bytes = std::fs::read(&path).expect("sample.xlsx fixture (tests/fixtures)");
        assert_eq!(sniff(&bytes), Magic::ZipOoxml(OfficeSubtype::Xlsx));
        // 首条目反例实证：LFH 扫到的第一个条目名不是 [Content_Types].xml。
        let name_start = 30usize;
        let namelen = u16::from_le_bytes([bytes[26], bytes[27]]) as usize;
        let first = std::str::from_utf8(&bytes[name_start..name_start + namelen]).unwrap();
        assert_ne!(
            first, "[Content_Types].xml",
            "首条目规则反例前提失效？实测 {first:?}"
        );
        assert_eq!(first, "docProps/app.xml");
        assert_eq!(
            sniff(&bytes).routed_channel(),
            Some(Channel::Office),
            "xlsx 反例必须正确路由 Office 通道"
        );
    }

    #[test]
    fn docx_pptx_markers_and_zip_other() {
        let docx = std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sample.docx"),
        )
        .expect("sample.docx fixture (tests/fixtures)");
        assert_eq!(sniff(&docx), Magic::ZipOoxml(OfficeSubtype::Docx));
        let pptx = std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sample.pptx"),
        )
        .expect("sample.pptx fixture (tests/fixtures)");
        assert_eq!(sniff(&pptx), Magic::ZipOoxml(OfficeSubtype::Pptx));
        // zip_other 三类（epub 首条目 mimetype / zip 普通 / evil.zip 穿越名）：
        // DI-2 ADR D1 行为变更——zip_other 改路由归档通道（两阶段四闸承接）。
        for name in ["sample.epub", "sample.zip", "evil.zip"] {
            let bytes = std::fs::read(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join(format!("tests/fixtures/{name}")),
            )
            .unwrap_or_else(|e| panic!("{name}: {e}"));
            assert_eq!(sniff(&bytes), Magic::ZipOther, "{name}");
            assert_eq!(
                sniff(&bytes).routed_channel(),
                Some(Channel::Archive),
                "{name} 按 DI-2 D1 路由归档通道"
            );
        }
    }

    /// 嗅探器手写 ZIP 面边界：data-descriptor 形态（csize=0 + bit3）终止不猜测；
    /// 伪 ZIP（LFH 签名后截断）fail-closed。
    #[test]
    fn zip_scan_edges_fail_closed() {
        // 手工构造：[Content_Types].xml 在第 2 个条目（首个条目反例形态）。
        let mut z: Vec<u8> = Vec::new();
        let lfh = |name: &[u8], csize: u32, flags: u16, out: &mut Vec<u8>| {
            out.extend_from_slice(&ZIP_LFH_SIG);
            out.extend_from_slice(&20u16.to_le_bytes());
            out.extend_from_slice(&flags.to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(&0u32.to_le_bytes());
            out.extend_from_slice(&csize.to_le_bytes());
            out.extend_from_slice(&csize.to_le_bytes());
            out.extend_from_slice(&(name.len() as u16).to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(name);
            out.extend(std::iter::repeat_n(0u8, csize as usize));
        };
        lfh(b"docProps/app.xml", 4, 0, &mut z);
        lfh(b"[Content_Types].xml", 4, 0, &mut z);
        lfh(b"word/document.xml", 4, 0, &mut z);
        assert_eq!(sniff(&z), Magic::ZipOoxml(OfficeSubtype::Docx));

        // data-descriptor 形态：首个条目 csize=0 + bit3 → 终止，未命中 → zip_other。
        let mut z2: Vec<u8> = Vec::new();
        lfh(b"first.bin", 0, 0x8, &mut z2);
        z2.extend_from_slice(&ZIP_LFH_SIG);
        z2.extend_from_slice(b"[Content_Types].xml-later");
        assert_eq!(sniff(&z2), Magic::ZipOther);

        // 非 ZIP 内容混入（PDF 名 + ZIP 头部后接非 LFH）→ 扫描终止定论。
        let mut z3 = ZIP_LFH_SIG.to_vec();
        z3.extend_from_slice(b"%PDF-not-a-zip-body");
        assert_eq!(sniff(&z3), Magic::ZipOther);
    }

    #[test]
    fn mismatch_is_ext_hint_vs_sniff_channel() {
        // 判据 9：.pdf 扩展名 + PNG 魔数 → 路由图像通道 + mismatch=true。
        let png: Vec<u8> = vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
        assert_eq!(sniff(&png), Magic::Png);
        assert!(mismatch(&Magic::Png, "pdf"));
        // 一致形态不标 mismatch。
        assert!(!mismatch(&Magic::Pdf, "pdf"));
        assert!(!mismatch(&Magic::Png, "png"));
        assert!(!mismatch(&Magic::ZipOoxml(OfficeSubtype::Xlsx), "xlsx"));
        // 扩展名无提示 → 无 mismatch 可言。
        assert!(!mismatch(&Magic::Png, ""));
        assert!(!mismatch(&Magic::Pdf, "weird"));
    }
}
