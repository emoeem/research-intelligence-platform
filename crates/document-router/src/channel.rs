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

/// 通道（D3 统一输出 Schema `channel` 词表：pdf | office | image）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Channel {
    /// PDF 通道（`%PDF-`）：pdfinfo + pdftotext 两个子 run。
    Pdf,
    /// Office 通道（ZIP→OOXML）：soffice → 级联 PDF 通道两子 run。
    Office,
    /// 图像通道（`\x89PNG` / `\xFF\xD8\xFF`）：magick identify -ping 单子 run。
    Image,
}

impl Channel {
    /// 统一输出 Schema `channel` 字面量。
    pub fn as_str(&self) -> &'static str {
        match self {
            Channel::Pdf => "pdf",
            Channel::Office => "office",
            Channel::Image => "image",
        }
    }
}

/// DI-1 四份工具 Manifest id（D3 通道表；Manifest 落 manifests/tools/，
/// S4 登记）。
pub const TOOL_PDF_INFO: &str = "rip.doc.pdf.info";
pub const TOOL_PDF_TEXT: &str = "rip.doc.pdf.text";
pub const TOOL_OFFICE_TO_PDF: &str = "rip.doc.office.to_pdf";
pub const TOOL_IMAGE_META: &str = "rip.doc.image.meta";

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
    /// 单行元数据）。
    StdoutLog,
    /// out/ 转换产物 `<staged stem>.pdf`（soffice 命名确定性回读，D3）。
    ConvertedPdf,
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
    }

    #[test]
    fn cap_table_values_match_d7() {
        assert_eq!(MAX_INPUT_FILE_BYTES, 64 * 1024 * 1024);
        assert_eq!(MAX_PAGES, 2_000);
        assert_eq!(MAX_PIXELS, 100_000_000);
        assert_eq!(SUBRUN_MAX_OUTPUT_BYTES, 1_048_576);
        assert_eq!(IDENTIFY_FMT, "%m %wx%h %[colorspace] depth=%z");
        assert_eq!(OFFICE_OUT_FORMAT, "pdf");
    }
}
