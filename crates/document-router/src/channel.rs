//! 通道表（DI-1 ADR D3）与帽值表（D7，对齐 RT-0.5-E §2 十四项）。
//!
//! 通道 = 输入嗅探签名 → capability_id → Manifest/argv 模板 → 输出规范化的
//! 编排计划（D3 表）。argv 模板本体在各 Manifest `runtime.argv_template`（本
//! 机实测冻结形态，2026-11-19 /tmp/di1-probe）；帽值在本层后端强制（帽前置
//! 门 + 链内门，不只前端）。
//!
//! **编排来源 = `router.rs` 管线本体；本文件的 [`channel_steps`] 是同一 D3
//! 表的对账镜像（评审修订 2026-11-19 显式标注，防第二份通道表静默漂移）**：
//! Router 编排（`route_pdf` / `route_office` / `route_image`）的步骤序列、
//! 帽门插步位置与规范化取材在 router.rs 硬编码（office 首步 profile_uri/
//! run_id 预定 + 转换产物回读、级联输入切换 pdf_bytes/staged 等编排语义
//! 无法以 `Step{tool_id, capability_ref, kind}` 表行承载）；扩展管线（DI-2
//! OCR 步骤）时**两处同改**：router.rs 管线本体（行为面）+ 本表行（对账
//! 面），并同步锁定用例（`channel_table_rows_match_d3` /
//! `d3_channel_table_rows_locked`）。

/// 通道（D3 统一输出 Schema `channel` 词表 + DI-2 D1 扩展：
/// pdf | office | image | ocr | media | archive）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Channel {
    /// PDF 通道（`%PDF-`）：pdfinfo + pdftotext 两个子 run。
    Pdf,
    /// Office 通道（ZIP→OOXML）：soffice → 级联 PDF 通道两子 run。
    Office,
    /// 图像通道（`\x89PNG` / `\xFF\xD8\xFF`）：magick identify -ping 单子 run。
    Image,
    /// OCR 通道（DI-2 D2）：仅显式请求位触发（图像嗅探签名 + input.ocr 并
    /// 存）；identify 门步 + tesseract TSV 两子 run。无请求位时 PNG/JPEG 走
    /// 图像通道（DI-1 行为零变化）。
    Ocr,
    /// 媒体通道（DI-2 D3）：七嗅探签名（wav/avi/mp4/webm/ogg/flac/mp3）；
    /// ffprobe 常步 + ffmpeg 抽帧条件步（视频流存在时）。
    Media,
    /// 归档通道（DI-2 D4）：zip_other + 7z 两签名；7z 两阶段（l -slt 列目
    /// 四闸 → x 解包本 run out/ + post-extract 符号链接终检）。
    Archive,
}

impl Channel {
    /// 统一输出 Schema `channel` 字面量。
    pub fn as_str(&self) -> &'static str {
        match self {
            Channel::Pdf => "pdf",
            Channel::Office => "office",
            Channel::Image => "image",
            Channel::Ocr => "ocr",
            Channel::Media => "media",
            Channel::Archive => "archive",
        }
    }
}

/// DI-1 四份 + DI-2 五份工具 Manifest id（D3/D2/D3/D4 通道表；Manifest 落
/// manifests/tools/，S4 登记）。
pub const TOOL_PDF_INFO: &str = "rip.doc.pdf.info";
pub const TOOL_PDF_TEXT: &str = "rip.doc.pdf.text";
pub const TOOL_OFFICE_TO_PDF: &str = "rip.doc.office.to_pdf";
pub const TOOL_IMAGE_META: &str = "rip.doc.image.meta";
/// DI-2 D2：OCR 步（tesseract TSV 单 run 承载文本 + 置信度）。
pub const TOOL_IMAGE_OCR: &str = "rip.doc.image.ocr";
/// DI-2 D3：媒体探测步（ffprobe JSON）。
pub const TOOL_MEDIA_PROBE: &str = "rip.media.probe";
/// DI-2 D3：媒体抽帧步（ffmpeg 受限抽帧，全常量 argv）。
pub const TOOL_MEDIA_FRAMES: &str = "rip.media.frames";
/// DI-2 D4：归档列目阶段（7z l -slt）。
pub const TOOL_ARCHIVE_LIST: &str = "rip.archive.list";
/// DI-2 D4：归档解包阶段（7z x -y -oout）。
pub const TOOL_ARCHIVE_EXTRACT: &str = "rip.archive.extract";

/// 通道内一个子 run 步骤（D3 表行）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Step {
    /// 工具 Manifest id。
    pub tool_id: &'static str,
    /// capability 目录引用（目录条目与 Manifest capability_ref 双载一致）。
    pub capability_ref: &'static str,
    /// 该步骤的产物面（stdout 文本 or out/ 产物），Router 规范化取材依据。
    pub kind: StepOutput,
}

/// 子 run 产物面（Router 回读载体；file 模式 data 只含产物引用，文本/
/// 产物按 D5 由 Router 从工作区回读）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepOutput {
    /// stdout 留档 out/stdout.log（pdfinfo 报告 / pdftotext 文本 / magick
    /// 单行元数据 / tesseract TSV / ffprobe JSON / 7z -slt 技术清单）。
    StdoutLog,
    /// out/ 转换产物 `<staged stem>.pdf`（soffice 命名确定性回读，D3）。
    ConvertedPdf,
    /// out/ 多文件产物（DI-2：ffmpeg 抽帧 frame-NNN.png / 7z 解包产物；
    /// cwd 相对模板常量落本 run out/，Router 按目录清点对账）。
    OutArtifacts,
}

/// magick identify 的 format 参数（D3 实测冻结：`PNG 834x250 sRGB depth=8`
/// 单行形态；`identify -json` 本机不支持——D3 否决项）。
pub const IDENTIFY_FMT: &str = "%m %wx%h %[colorspace] depth=%z";

/// soffice 转换目标格式（D3：`--convert-to pdf` 全链通过；txt/cat 过滤器
/// 本机挂死禁登，D3 否决项）。
pub const OFFICE_OUT_FORMAT: &str = "pdf";

/// 通道编排计划（D3 表程序化承载；**对账镜像，非编排来源**——Router 编排
/// 实际消费 router.rs 管线本体，本函数供表行锁定用例与外部对账；扩展时
/// 两处同改，见模块文档）。
pub fn channel_steps(channel: Channel) -> &'static [Step] {
    match channel {
        Channel::Pdf => &[
            Step {
                tool_id: TOOL_PDF_INFO,
                capability_ref: "pdf.pdfinfo",
                kind: StepOutput::StdoutLog,
            },
            Step {
                tool_id: TOOL_PDF_TEXT,
                capability_ref: "pdf.pdftotext",
                kind: StepOutput::StdoutLog,
            },
        ],
        Channel::Office => &[
            Step {
                tool_id: TOOL_OFFICE_TO_PDF,
                capability_ref: "document.libreoffice",
                kind: StepOutput::ConvertedPdf,
            },
            Step {
                tool_id: TOOL_PDF_INFO,
                capability_ref: "pdf.pdfinfo",
                kind: StepOutput::StdoutLog,
            },
            Step {
                tool_id: TOOL_PDF_TEXT,
                capability_ref: "pdf.pdftotext",
                kind: StepOutput::StdoutLog,
            },
        ],
        Channel::Image => &[Step {
            tool_id: TOOL_IMAGE_META,
            capability_ref: "image.magick",
            kind: StepOutput::StdoutLog,
        }],
        // DI-2 D2：OCR 通道两步（门步 identify 复用 DI-1 既有 Manifest 零改
        // + OCR 步 tesseract TSV）。仅显式请求位进入本通道。
        Channel::Ocr => &[
            Step {
                tool_id: TOOL_IMAGE_META,
                capability_ref: "image.magick",
                kind: StepOutput::StdoutLog,
            },
            Step {
                tool_id: TOOL_IMAGE_OCR,
                capability_ref: "ocr.tesseract",
                kind: StepOutput::StdoutLog,
            },
        ],
        // DI-2 D3：媒体通道（probe 常步 + frames 条件步——抽帧步仅在视频流
        // 存在且门前四序全过时提交；镜像表行承载步骤集合，条件语义归
        // router.rs 管线本体，见模块文档「两处同改」）。
        Channel::Media => &[
            Step {
                tool_id: TOOL_MEDIA_PROBE,
                capability_ref: "media.ffprobe",
                kind: StepOutput::StdoutLog,
            },
            Step {
                tool_id: TOOL_MEDIA_FRAMES,
                capability_ref: "media.ffmpeg",
                kind: StepOutput::OutArtifacts,
            },
        ],
        // DI-2 D4：归档通道两阶段（list 列目四闸 → extract 解包；四闸任一
        // 触发时 extract 永不提交，runs.len()==1——条件语义归 router.rs）。
        Channel::Archive => &[
            Step {
                tool_id: TOOL_ARCHIVE_LIST,
                capability_ref: "archive.7z",
                kind: StepOutput::StdoutLog,
            },
            Step {
                tool_id: TOOL_ARCHIVE_EXTRACT,
                capability_ref: "archive.7z",
                kind: StepOutput::OutArtifacts,
            },
        ],
    }
}

// ---------------------------------------------------------------------------
// 帽值表（D7，对齐 RT-0.5-E §2 十四项；数值为任务书定稿值，本层后端强制）
// ---------------------------------------------------------------------------

/// #7 输入文件大小帽：64 MiB（对齐 worker-runtime `max_output_bytes` 上限
/// 67108864 同量级口径）。Router 嗅探前 stat 检查，超帽 INPUT 拒绝（无 run 行）。
pub const MAX_INPUT_FILE_BYTES: u64 = 67_108_864;

/// #7 页数帽：PDF 2000 页——pdfinfo 子 run 后解析 `Pages:`，超帽中止链
/// （office 级联同门；链内 FAILED，已存子 run 行保持真实终态，D7 拒绝语义）。
pub const MAX_PAGES: u64 = 2_000;

/// #8 像素帽：图像 100 MP（元数据路径 `-ping` 仅头部读；DI-2 渲染/OCR 批
/// 在派生前按同表收紧）。Router 门：identify 输出解析后校验。
pub const MAX_PIXELS: u64 = 100_000_000;

/// #10 子 run stdout 读帽：引擎缺省预算 max_output_bytes = 1048576（Router
/// 不显式传 budget 即落到该帽；e2e 断言面引用本常量对齐）。
pub const SUBRUN_MAX_OUTPUT_BYTES: u64 = 1_048_576;

// ---------------------------------------------------------------------------
// DI-2 帽值表（ADR D5：E §2 #4-#11 在 OCR/媒体/归档域的定稿承载）
// ---------------------------------------------------------------------------

/// #8 OCR 像素帽：50 MP（DI-1 元数据路径 100 MP 是 `-ping` 仅头部读口径；
/// OCR 全量解码，按 DI-2 ADR D2/D5 收紧）。Router 门：identify 门步输出
/// 解析后校验，超帽链内 FAILED（identify 行留证），tesseract 永不见超帽输入。
pub const OCR_PIXELS_MAX: u64 = 50_000_000;

/// #7 媒体内容时长帽：300 s（ffprobe format.duration 解析门，ffmpeg 派生
/// 前定案；超帽链内 FAILED，probe 行留证）。
pub const MEDIA_DURATION_MAX_SECONDS: f64 = 300.0;

/// #8 媒体单帧像素帽：16 MP（ffprobe streams[video].width×height 门）。
pub const MEDIA_FRAME_PIXELS_MAX: u64 = 16_000_000;

/// #7 媒体帧数帽：32（argv 模板 `-frames:v 32` 字面常量 + Router 帧预算
/// min(⌈2×duration⌉, 32) 的帽常量；前缀采样语义非拒绝，D5）。
pub const MEDIA_FRAMES_MAX: u32 = 32;

/// 抽帧速率常量（argv 模板 `fps=2` 字面量 + metadata.frames.fps 载体；
/// 调用方不可控速率参数——帽是 Router 属性，D3 否决项）。
pub const MEDIA_FRAME_FPS: u32 = 2;

/// 抽帧输出 pattern（argv 模板尾项字面量；cwd 相对 → 本 run out/，D3）。
pub const MEDIA_FRAME_PATTERN: &str = "out/frame-%03d.png";
/// metadata.frames.pattern 载体（去 out/ 前缀的 pattern 名）。
pub const MEDIA_FRAME_PATTERN_NAME: &str = "frame-%03d.png";

/// #9 归档条目数帽：512（含目录条目；闸 3，超帽链内 FAILED 解包零提交）。
pub const ARCHIVE_MAX_ENTRIES: usize = 512;

/// #9 归档解压总量帽：256 MiB（Σ Size；闸 4 第一判据）。
pub const ARCHIVE_MAX_TOTAL_BYTES: u64 = 256 * 1024 * 1024;

/// #9 归档单条目帽：64 MiB（对齐 64 MiB 输入帽量级；闸 4 第二判据）。
pub const ARCHIVE_MAX_ENTRY_BYTES: u64 = 64 * 1024 * 1024;

/// OCR 语言路由表（DI-2 ADR D2：请求语言 → tesseract `-l` 候选语言集）。
///
/// **唯一权威 = capability 目录 `ocr.tesseract` 条目 language 集**（本机
/// chi_sim/eng/osd，RT-0.5-C §3.3）：本表只做「请求语言 → 候选语言」映射，
/// 放行判定在 Router 层对候选语言逐一对照目录 language 集——目录重探
/// （补装 chi_tra）后路由自动放行，Router 零改码。表外请求语言（ja/ko/fr/
/// zh-Hant/chi_tra 等一切未列形态）一律前置门拒绝（禁静默回退，D2）。
///
/// 缺省 / `auto` 不入表：Router 直接取缺省候选集
/// [`OCR_LANG_DEFAULT_CANDIDATES`]。
pub const OCR_LANG_ROUTES: &[(&str, &[&str])] = &[
    ("zh-Hans", &["chi_sim"]),
    ("zh-CN", &["chi_sim"]),
    ("zh", &["chi_sim"]),
    ("chi_sim", &["chi_sim"]),
    ("en", &["eng"]),
    ("eng", &["eng"]),
    // 繁体面：候选 chi_tra——目录 language 集无 chi_tra → Router 前置门
    // ROUTER_OCR_LANG_MISSING 拒绝（禁静默回退 chi_sim，D2；补装后目录
    // 重探路由自动放行，Router 零改码）。
    ("zh-Hant", &["chi_tra"]),
    ("zh-TW", &["chi_tra"]),
    ("zh-HK", &["chi_tra"]),
    ("chi_tra", &["chi_tra"]),
];

/// 缺省 / `auto` 请求的候选语言集（目录双语言放行形态，组合 `-l
/// chi_sim+eng` 本机实测 exit 0）。
pub const OCR_LANG_DEFAULT_CANDIDATES: &[&str] = &["chi_sim", "eng"];

/// tesseract `-l` 组合分隔符（目录双语言组合形态）。
pub const OCR_LANG_JOIN: char = '+';

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn channel_table_rows_match_d3() {
        let pdf = channel_steps(Channel::Pdf);
        assert_eq!(pdf.len(), 2);
        assert_eq!(pdf[0].tool_id, TOOL_PDF_INFO);
        assert_eq!(pdf[0].capability_ref, "pdf.pdfinfo");
        assert_eq!(pdf[1].tool_id, TOOL_PDF_TEXT);
        assert_eq!(pdf[1].capability_ref, "pdf.pdftotext");

        // Office 通道 = soffice 转换 + 级联 PDF 通道两子 run（D3：Office 文本
        // 抽取一律走 Office→PDF→pdftotext 级联，禁 txt/cat 直转挂死面）。
        let office = channel_steps(Channel::Office);
        assert_eq!(office.len(), 3);
        assert_eq!(office[0].tool_id, TOOL_OFFICE_TO_PDF);
        assert_eq!(office[0].capability_ref, "document.libreoffice");
        assert_eq!(office[0].kind, StepOutput::ConvertedPdf);
        assert_eq!(office[1].tool_id, TOOL_PDF_INFO);
        assert_eq!(office[2].tool_id, TOOL_PDF_TEXT);

        let image = channel_steps(Channel::Image);
        assert_eq!(image.len(), 1);
        assert_eq!(image[0].tool_id, TOOL_IMAGE_META);
        assert_eq!(image[0].capability_ref, "image.magick");

        // DI-2 D2：OCR 通道两步（identify 门步复用 DI-1 件 + tesseract 步）。
        let ocr = channel_steps(Channel::Ocr);
        assert_eq!(ocr.len(), 2);
        assert_eq!(ocr[0].tool_id, TOOL_IMAGE_META);
        assert_eq!(ocr[0].capability_ref, "image.magick");
        assert_eq!(ocr[1].tool_id, TOOL_IMAGE_OCR);
        assert_eq!(ocr[1].capability_ref, "ocr.tesseract");
        assert_eq!(ocr[1].kind, StepOutput::StdoutLog);

        // DI-2 D3：媒体通道两步（probe 常步 + frames 条件步镜像行）。
        let media = channel_steps(Channel::Media);
        assert_eq!(media.len(), 2);
        assert_eq!(media[0].tool_id, TOOL_MEDIA_PROBE);
        assert_eq!(media[0].capability_ref, "media.ffprobe");
        assert_eq!(media[1].tool_id, TOOL_MEDIA_FRAMES);
        assert_eq!(media[1].capability_ref, "media.ffmpeg");
        assert_eq!(media[1].kind, StepOutput::OutArtifacts);

        // DI-2 D4：归档通道两阶段（list + extract，同一 archive.7z 能力）。
        let archive = channel_steps(Channel::Archive);
        assert_eq!(archive.len(), 2);
        assert_eq!(archive[0].tool_id, TOOL_ARCHIVE_LIST);
        assert_eq!(archive[0].capability_ref, "archive.7z");
        assert_eq!(archive[1].tool_id, TOOL_ARCHIVE_EXTRACT);
        assert_eq!(archive[1].capability_ref, "archive.7z");
        assert_eq!(archive[1].kind, StepOutput::OutArtifacts);
    }

    #[test]
    fn cap_table_values_match_d7() {
        assert_eq!(MAX_INPUT_FILE_BYTES, 64 * 1024 * 1024);
        assert_eq!(MAX_PAGES, 2_000);
        assert_eq!(MAX_PIXELS, 100_000_000);
        assert_eq!(SUBRUN_MAX_OUTPUT_BYTES, 1_048_576);
        assert_eq!(IDENTIFY_FMT, "%m %wx%h %[colorspace] depth=%z");
        assert_eq!(OFFICE_OUT_FORMAT, "pdf");
        // DI-2 D5 定稿值。
        assert_eq!(OCR_PIXELS_MAX, 50_000_000, "OCR 像素帽 50 MP（D2/D5）");
        assert_eq!(
            MEDIA_DURATION_MAX_SECONDS, 300.0,
            "媒体内容时长帽 300 s（D3/D5）"
        );
        assert_eq!(
            MEDIA_FRAME_PIXELS_MAX, 16_000_000,
            "媒体单帧像素帽 16 MP（D3/D5）"
        );
        assert_eq!(MEDIA_FRAMES_MAX, 32, "媒体帧数帽 32（D3/D5）");
        assert_eq!(MEDIA_FRAME_FPS, 2, "抽帧速率常量 fps=2（D3）");
        assert_eq!(MEDIA_FRAME_PATTERN, "out/frame-%03d.png");
        assert_eq!(MEDIA_FRAME_PATTERN_NAME, "frame-%03d.png");
        assert_eq!(ARCHIVE_MAX_ENTRIES, 512, "归档条目数帽（D4/D5 闸 3）");
        assert_eq!(
            ARCHIVE_MAX_TOTAL_BYTES,
            256 * 1024 * 1024,
            "归档解压总量帽 256 MiB（D4/D5 闸 4）"
        );
        assert_eq!(
            ARCHIVE_MAX_ENTRY_BYTES,
            64 * 1024 * 1024,
            "归档单条目帽 64 MiB（D4/D5 闸 4）"
        );
    }

    /// DI-2 D2 语言路由表：请求语言 → 候选集形态锁定（放行判定归 Router
    /// 对 catalog language 集的对照，表只承载映射）。
    #[test]
    fn ocr_lang_routes_match_d2_table() {
        let route = |req: &str| {
            OCR_LANG_ROUTES
                .iter()
                .find(|(k, _)| *k == req)
                .map(|(_, v)| *v)
        };
        assert_eq!(route("zh-Hans"), Some(&["chi_sim"][..]));
        assert_eq!(route("zh-CN"), Some(&["chi_sim"][..]));
        assert_eq!(route("zh"), Some(&["chi_sim"][..]));
        assert_eq!(route("chi_sim"), Some(&["chi_sim"][..]));
        assert_eq!(route("en"), Some(&["eng"][..]));
        assert_eq!(route("eng"), Some(&["eng"][..]));
        // 繁体面映射到 chi_tra 候选（目录无 chi_tra → Router 前置门拒绝，
        // 禁静默回退，D2/D6）；其余语言不在表 → 请求原值作候选拒绝。
        assert_eq!(route("zh-Hant"), Some(&["chi_tra"][..]));
        assert_eq!(route("zh-TW"), Some(&["chi_tra"][..]));
        assert_eq!(route("zh-HK"), Some(&["chi_tra"][..]));
        assert_eq!(route("chi_tra"), Some(&["chi_tra"][..]));
        assert!(route("ja").is_none());
        assert!(route("fr").is_none());
        // 缺省 / auto 候选集 = 目录双语言组合。
        assert_eq!(OCR_LANG_DEFAULT_CANDIDATES, &["chi_sim", "eng"]);
        assert_eq!(OCR_LANG_JOIN, '+');
    }
}
