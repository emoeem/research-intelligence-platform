//! PDF 渲染器（PHASE-6B D1/D2/D7：`render_pdf`——IR→PDF 字节，CID Type0
//! 字体子集嵌入 + 手写分页排版，零 I/O 零 AI）。
//!
//! 选型链（D1）：skrifa（cmap/度量解析，Google Fonts fontations）→
//! subsetter（default-features=false 剥 variable-fonts，字体子集化）→
//! pdf-writer（PDF 对象图写出）。组合有 typst-pdf 0.x 历史实现同构背书
//! （D1 词形：pdf-writer/subsetter = typst 生态自产）。
//!
//! 字体面（D2）：字体字节经 `&[FontSource]` 参数注入（渲染纯函数零 I/O
//! 红线延续）；CJK 探测 = skrifa cmap 含 U+4E00-9FFF 锚点字形；缺 CJK 面
//! 渲染降级 .notdef 不 panic（红线 1），质检 qc_pdf「中文字体正确」项
//! fail 承载（:545 词形——fail 不 abort）。
//!
//! 排版面：手写分页（A4，逐字符宽度累加断行）——Latin kerning/两端对齐
//! 属手写排版 tradeoff 观察池（6B 观察池 10）。五段投影与 render_markdown
//! 五段同构（6A D5 词形）。
//!
//! 字节面锚点（判据 5/qc_pdf）：
//! - `%PDF-` 头 + `%%EOF` 尾；
//! - `/Type /Page` 计数与 Pages `/Count` 一致；
//! - CJK 面嵌入 `/FontFile2`；
//! - finding_id（ASCII）经 DocumentInfo `/Subject` 字面可见；
//! - title 中文经 ToUnicode CMap 的 UTF-16BE hex 词形可见。

use crate::fonts::FontSource;
use crate::ir::ReportIr;
use pdf_writer::{
    types::{CidFontType, FontFlags, SystemInfo, UnicodeCmap},
    Content, Name, Pdf, Rect, Ref, Str, TextStr,
};
use skrifa::{
    charmap::Charmap, instance::LocationRef, instance::Size, metrics::GlyphMetrics, GlyphId,
    MetadataProvider,
};
use std::collections::BTreeMap;

/// A4 纵向页面尺寸（pt）。
const PAGE_W: f32 = 595.0;
const PAGE_H: f32 = 842.0;
/// 页边距（上下左右统一）。
const MARGIN: f32 = 57.0;
/// 内容区宽（A4 - 双边距）。
const CONTENT_W: f32 = PAGE_W - 2.0 * MARGIN;
/// 页底保留区高度（页脚位）。
const BOTTOM_GUARD: f32 = 24.0;
/// PDF 嵌入字体内部资源名（Resources /Font 键 + content Tf 操作符引用）。
const FONT_KEY: &[u8] = b"F0";
/// BaseFont 词形（固定 ASCII——真实字形以 FontFile2 嵌入子集为准）。
const BASE_FONT: &[u8] = b"RipSans";
/// 无字体面（fonts 空切片）时的近似 char 宽度系数（0.6 em——降级 .notdef
/// 态排版用；qc_pdf CJK 项 fail 承载，渲染不 panic）。
const NOTDEF_WIDTH_RATIO: f32 = 0.6;

/// 单行排版内容（段落样式已折行展开）。
struct TextLine {
    text: String,
    /// 字号（pt）。
    size: f32,
    /// 行距系数（行高 = size * leading）。
    leading: f32,
    /// 行后附加空隙（pt）。
    space_after: f32,
}

impl TextLine {
    fn body(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            size: 10.5,
            leading: 1.6,
            space_after: 3.0,
        }
    }

    fn h1(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            size: 18.0,
            leading: 1.5,
            space_after: 8.0,
        }
    }

    fn h2(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            size: 13.5,
            leading: 1.5,
            space_after: 6.0,
        }
    }

    fn h3(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            size: 11.5,
            leading: 1.5,
            space_after: 5.0,
        }
    }

    fn meta(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            size: 9.5,
            leading: 1.5,
            space_after: 1.5,
        }
    }

    fn indent(text: impl Into<String>) -> Self {
        Self {
            text: format!("  {}", text.into()),
            size: 9.5,
            leading: 1.5,
            space_after: 1.5,
        }
    }
}

/// 段落生成器：五段投影（与 render_markdown 五段同构——6A D5 词形）。
fn collect_lines(ir: &ReportIr) -> Vec<TextLine> {
    let mut lines: Vec<TextLine> = Vec::new();
    lines.push(TextLine::h1(ir.title.clone()));
    lines.push(TextLine::meta(format!("报告 ID：{}", ir.report_id)));
    lines.push(TextLine::meta(format!("报告类型：{}", ir.report_kind)));
    lines.push(TextLine::meta(format!("生成时间：{}", ir.generated_at)));
    lines.push(TextLine::meta(format!("语言：{}", ir.language)));
    if let Some(task_ref) = &ir.task_ref {
        if let Some(task_id) = &task_ref.task_id {
            lines.push(TextLine::meta(format!("研究任务：{task_id}")));
        }
        if let Some(run_id) = &task_ref.workflow_run_id {
            lines.push(TextLine::meta(format!("工作流运行：{run_id}")));
        }
    }
    if let Some(scope) = &ir.scope {
        if let Some(auth) = &scope.authorization {
            lines.push(TextLine::meta(format!("授权范围：{auth}")));
        }
        for asset in &scope.assets {
            lines.push(TextLine::meta(format!("资产：{asset}")));
        }
    }
    // 执行摘要
    lines.push(TextLine::h2("执行摘要"));
    if let Some(body) = ir.executive_summary.as_deref() {
        for para in body.split('\n') {
            lines.push(TextLine::body(para));
        }
    }
    // Finding 清单
    lines.push(TextLine::h2("Finding 清单"));
    for finding in &ir.findings {
        lines.push(TextLine::body(format!(
            "{}　{}（{} / {}）",
            finding.finding_id, finding.title, finding.severity, finding.result_state
        )));
    }
    // Finding 详情
    lines.push(TextLine::h2("Finding 详情"));
    for finding in &ir.findings {
        lines.push(TextLine::h3(format!(
            "{}：{}（{} / {}）",
            finding.finding_id, finding.title, finding.severity, finding.result_state
        )));
        lines.push(TextLine::indent(format!("漏洞类型：{}", finding.vuln_type)));
        if let Some(rtis_ref) = &finding.rtis_ref {
            lines.push(TextLine::indent(format!("RTIS 引用：{rtis_ref}")));
        }
        if let Some(location) = &finding.location {
            if let Some(uri) = &location.artifact_uri {
                lines.push(TextLine::indent(format!("位置：{uri}")));
            }
            if let (Some(start), Some(end)) = (location.start_line, location.end_line) {
                lines.push(TextLine::indent(format!("行区间：{start}-{end}")));
            }
            if let Some(snippet) = &location.snippet {
                lines.push(TextLine::indent("代码片段："));
                for code_line in snippet.split('\n') {
                    lines.push(TextLine::indent(code_line));
                }
            }
        }
        for scope_line in &finding.affected_scope {
            lines.push(TextLine::indent(format!("受影响范围：{scope_line}")));
        }
        push_markdown_field(&mut lines, "前置条件", finding.preconditions.as_deref());
        push_markdown_field(&mut lines, "原理", finding.principle.as_deref());
        push_markdown_field(&mut lines, "复现步骤", finding.repro_steps.as_deref());
        for rr in &finding.request_response {
            lines.push(TextLine::indent(format!("请求/响应（{}）：", rr.direction)));
            for content_line in rr.content.split('\n') {
                lines.push(TextLine::indent(content_line));
            }
        }
        for evidence in &finding.evidence {
            lines.push(TextLine::indent(format!(
                "证据 {}：{}（{}）",
                evidence.evidence_id, evidence.uri, evidence.content_hash
            )));
        }
        if let Some(impact) = finding.impact.as_deref() {
            push_markdown_field(&mut lines, "影响", Some(impact));
        }
        if let Some(remediation) = &finding.remediation {
            lines.push(TextLine::indent(format!(
                "修复优先级：{}",
                remediation.priority
            )));
            for suggestion in &remediation.suggestions {
                lines.push(TextLine::indent(format!("修复建议：{suggestion}")));
            }
        }
    }
    // 攻击路径
    lines.push(TextLine::h2("攻击路径"));
    if ir.attack_paths.is_empty() {
        lines.push(TextLine::body("（无攻击路径）"));
    } else {
        for path in &ir.attack_paths {
            lines.push(TextLine::h3(format!("{}：{}", path.path_id, path.title)));
            for (idx, step) in path.steps.iter().enumerate() {
                lines.push(TextLine::indent(format!("{}. {step}", idx + 1)));
            }
            for evidence_ref in &path.evidence_refs {
                lines.push(TextLine::indent(format!("证据引用：{evidence_ref}")));
            }
        }
    }
    // 附录
    lines.push(TextLine::h2("附录"));
    if let Some(body) = ir.appendix.as_deref() {
        for para in body.split('\n') {
            lines.push(TextLine::body(para));
        }
    }
    lines
}

/// markdown 片段字段（标题行 + 原文——markdown.rs 同位词形）。
fn push_markdown_field(lines: &mut Vec<TextLine>, label: &str, body: Option<&str>) {
    let Some(body) = body else { return };
    if body.is_empty() {
        return;
    }
    lines.push(TextLine::indent(format!("{label}：")));
    for para in body.split('\n') {
        lines.push(TextLine::indent(para));
    }
}

/// 字体度量面（不可变——resolve 一次性解析；glyph 分配器独立于本结构以
/// 避免排版期可变借用冲突）。
struct Face<'a> {
    /// skrifa 解析面（None = 全降级 .notdef——fonts 空或全不可解析）。
    font: Option<skrifa::FontRef<'a>>,
    charmap: Option<Charmap<'a>>,
    /// char → advance（font units）查询面。
    glyph_metrics: Option<GlyphMetrics<'a>>,
    units_per_em: f32,
}

impl<'a> Face<'a> {
    /// 从注入字体字节解析（fail-soft：全不可解析 → 降级面；CJK 面优先
    /// ——D2④「选含中文字形的首个字体」）。
    fn resolve(sources: &'a [FontSource]) -> Self {
        let mut first_face: Option<skrifa::FontRef<'a>> = None;
        let mut cjk_face: Option<skrifa::FontRef<'a>> = None;
        for source in sources {
            let Ok(font) = skrifa::FontRef::from_index(&source.data, source.index) else {
                continue;
            };
            let is_cjk = has_cjk_glyphs(&font);
            if first_face.is_none() {
                first_face = Some(font.clone());
            }
            if cjk_face.is_none() && is_cjk {
                cjk_face = Some(font);
                break;
            }
        }
        let chosen = cjk_face.or(first_face);
        match chosen {
            Some(font) => {
                let charmap = font.charmap();
                let glyph_metrics = font.glyph_metrics(Size::unscaled(), LocationRef::default());
                let units_per_em = font
                    .metrics(Size::unscaled(), LocationRef::default())
                    .units_per_em
                    .max(1) as f32;
                Self {
                    font: Some(font),
                    charmap: Some(charmap),
                    glyph_metrics: Some(glyph_metrics),
                    units_per_em,
                }
            }
            None => Self {
                font: None,
                charmap: None,
                glyph_metrics: None,
                units_per_em: 1000.0,
            },
        }
    }

    /// char → glyph id（缺字 → gid 0 = .notdef——D2④ 词形）。
    fn glyph_id(&self, c: char) -> u16 {
        self.charmap
            .as_ref()
            .and_then(|cm| cm.map(c))
            .map(|gid| gid.to_u32() as u16)
            .unwrap_or(0)
    }

    /// char 宽度（pt）——advance（font units）* size / upem；无字体面用
    /// 近似系数（NOTDEF_WIDTH_RATIO em）。
    fn char_width(&self, c: char, size: f32) -> f32 {
        let units = match &self.glyph_metrics {
            Some(metrics) => {
                let gid = GlyphId::new(self.glyph_id(c) as u32);
                metrics
                    .advance_width(gid)
                    .unwrap_or_else(|| NOTDEF_WIDTH_RATIO * self.units_per_em)
            }
            None => NOTDEF_WIDTH_RATIO * self.units_per_em,
        };
        units * size / self.units_per_em
    }
}

/// CJK 探测（D2④ 词形：skrifa 解析 cmap 含 U+4E00-9FFF——三个锚点字形
/// （一 4E00 / 水 6C34 / 龥 9FA5）全映射即视为含中文字形）。
fn has_cjk_glyphs(font: &skrifa::FontRef<'_>) -> bool {
    let charmap = font.charmap();
    charmap.map('\u{4E00}').is_some()
        && charmap.map('\u{6C34}').is_some()
        && charmap.map('\u{9FA5}').is_some()
}

/// 逐字符断行（中文逐字断 + 英文空格优先——手写排版最小面；kerning/两端
/// 对齐属观察池 10）。返回按内容区宽折行后的行序列。
fn wrap_line(line: &TextLine, face: &Face, max_w: f32) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut cur_w = 0.0;
    for c in line.text.chars() {
        let w = face.char_width(c, line.size);
        if c == ' ' && cur_w + w > max_w {
            // 空格断行优先：空格宽度不计入下一行。
            out.push(std::mem::take(&mut cur));
            cur_w = 0.0;
            continue;
        }
        if cur_w + w > max_w && !cur.is_empty() {
            out.push(std::mem::take(&mut cur));
            cur_w = 0.0;
        }
        cur.push(c);
        cur_w += w;
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    if out.is_empty() {
        out.push(String::new());
    }
    out
}

/// 排版放置行（页内绝对基线 y + 行样式 + 文本）。
struct PlacedLine {
    y: f32,
    size: f32,
    text: String,
}

/// 排版分页：行流 → 每页放置行（y 从页顶向下排；页底 guard 内断页）。
fn paginate(lines: &[TextLine], face: &Face) -> Vec<Vec<PlacedLine>> {
    let mut pages: Vec<Vec<PlacedLine>> = Vec::new();
    let mut current: Vec<PlacedLine> = Vec::new();
    let mut y = PAGE_H - MARGIN;
    let bottom = MARGIN + BOTTOM_GUARD;
    for line in lines {
        for text in wrap_line(line, face, CONTENT_W) {
            let line_h = line.size * line.leading;
            if y - line_h < bottom {
                pages.push(std::mem::take(&mut current));
                y = PAGE_H - MARGIN;
            }
            y -= line_h;
            current.push(PlacedLine {
                y,
                size: line.size,
                text,
            });
            y -= line.space_after;
        }
    }
    if !current.is_empty() || pages.is_empty() {
        pages.push(current);
    }
    pages
}

/// Identity-H 编码 CMap 字节（PDF 标准固定词形——CID == GID 恒等映射面）。
fn identity_h_cmap() -> &'static [u8] {
    b"/CIDInit /ProcSet findresource begin\n12 dict begin\nbegincmap\n/CIDSystemInfo << /Registry (Adobe) /Ordering (Identity) /Supplement 0 >> def\n/CMapName /Adobe-Identity def\n/CMapType 0 def\n1 begincodespacerange\n<0000> <FFFF>\nendcodespacerange\n1 usecmap\nendcmap\nCMapName currentdict /CMap defineresource pop\nend\nend"
}

/// glyph 分配结果（remap 收集 + ToUnicode first-win 记录 + 逐行 CID 字节）。
struct Encoded {
    /// 每页 → 每行 → (y, size, cid 字节)。
    pages: Vec<Vec<(f32, f32, Vec<u8>)>>,
    /// cid → unicode 字符（first-win——cmap 多 char 同 gid 时首个胜出，
    /// ToUnicode 单点降级登记批次报告）。
    to_unicode: BTreeMap<u16, Vec<char>>,
    /// remapper 本体（子集化必须与编码共用同一 remapper——subsetter 按
    /// remap 调用序重排 gid）。
    remapper: subsetter::GlyphRemapper,
    /// CID 总数（= num_gids）。
    num_cids: u16,
}

/// 文本流编码（remap 阶段：全部放置文本逐 char 分配 CID 并缓存字节面）。
fn encode_pages(pages: &[Vec<PlacedLine>], face: &Face) -> Encoded {
    let mut remapper = subsetter::GlyphRemapper::new();
    let mut to_unicode: BTreeMap<u16, Vec<char>> = BTreeMap::new();
    let mut encoded: Vec<Vec<(f32, f32, Vec<u8>)>> = Vec::with_capacity(pages.len());
    for page in pages {
        let mut lines: Vec<(f32, f32, Vec<u8>)> = Vec::with_capacity(page.len());
        for placed in page {
            let mut bytes = Vec::with_capacity(placed.text.len() * 2);
            for c in placed.text.chars() {
                let gid = face.glyph_id(c);
                let cid = remapper.remap(gid);
                to_unicode.entry(cid).or_insert_with(|| vec![c]);
                bytes.extend_from_slice(&cid.to_be_bytes());
            }
            lines.push((placed.y, placed.size, bytes));
        }
        encoded.push(lines);
    }
    let num_cids = remapper.num_gids();
    Encoded {
        pages: encoded,
        to_unicode,
        remapper,
        num_cids,
    }
}

/// IR → PDF 字节（确定性纯函数——同 IR 同 fonts 两次渲染字节一致）。
pub fn render_pdf(ir: &ReportIr, fonts: &[FontSource]) -> Vec<u8> {
    let face = Face::resolve(fonts);
    let lines = collect_lines(ir);
    let pages = paginate(&lines, &face);
    let encoded = encode_pages(&pages, &face);
    let page_count = pages.len();

    // 子集字节（remap 收集完成后执行——subsetter::subset(data, index, mapper)；
    // 子集失败 → FontFile2 缺省（qc_pdf fail 承载），不 panic）。
    let subset_bytes: Option<Vec<u8>> = face.font.as_ref().and_then(|font| {
        let data = font.data();
        let raw: &[u8] = data.as_ref();
        let index = font.ttc_index().unwrap_or(0);
        subsetter::subset(raw, index, &encoded.remapper).ok()
    });
    let has_font_file = subset_bytes
        .as_ref()
        .map(|b| !b.is_empty())
        .unwrap_or(false);

    // ToUnicode CMap（cid → unicode——pdf-writer UnicodeCmap builder）。
    let mut cmap = UnicodeCmap::new(
        Name(BASE_FONT),
        SystemInfo {
            registry: Str(b"Adobe"),
            ordering: Str(b"UCS"),
            supplement: 0,
        },
    );
    for (cid, chars) in &encoded.to_unicode {
        cmap.pair_with_multiple(*cid, chars.iter().copied());
    }
    let cmap_bytes = cmap.finish();

    // W widths：cid 顺序数组（PDF 宽度 = font units * 1000 / upem）。
    let widths: Vec<f32> = (0..encoded.num_cids)
        .map(|cid| {
            let gid = encoded.remapper.get(cid).unwrap_or(0);
            let units = face
                .glyph_metrics
                .as_ref()
                .and_then(|gm| gm.advance_width(GlyphId::new(gid as u32)))
                .unwrap_or(NOTDEF_WIDTH_RATIO * face.units_per_em);
            units * 1000.0 / face.units_per_em
        })
        .collect();
    let default_width = widths.first().copied().unwrap_or(1000.0);

    // 度量面（FontDescriptor 字段——font units → PDF 1000-em units）。
    let metrics = face
        .font
        .as_ref()
        .map(|font| font.metrics(Size::unscaled(), LocationRef::default()));
    let scale = 1000.0 / face.units_per_em;
    let ascent = metrics.as_ref().map(|m| m.ascent * scale).unwrap_or(800.0);
    let descent = metrics
        .as_ref()
        .map(|m| m.descent * scale)
        .unwrap_or(-200.0);
    let cap_height = metrics
        .as_ref()
        .and_then(|m| m.cap_height)
        .map(|c| (c * scale).max(1.0))
        .unwrap_or(700.0);
    let bbox = metrics
        .as_ref()
        .and_then(|m| m.bounds)
        .map(|b| {
            Rect::new(
                b.x_min * scale,
                b.y_min * scale,
                b.x_max * scale,
                b.y_max * scale,
            )
        })
        .unwrap_or_else(|| Rect::new(0.0, -200.0, 1000.0, 900.0));
    let italic_angle = metrics.as_ref().map(|m| m.italic_angle).unwrap_or(0.0);

    // 对象图组装。
    let mut pdf = Pdf::new();
    let catalog_id = Ref::new(1);
    let pages_id = Ref::new(2);
    let font_type0 = Ref::new(1000);
    let font_cid = Ref::new(1001);
    let font_desc = Ref::new(1002);
    let font_file = Ref::new(1003);
    let font_cmap_enc = Ref::new(1004);
    let font_tounicode = Ref::new(1005);
    let info_id = Ref::new(1006);

    pdf.catalog(catalog_id).pages(pages_id);

    // 字体簇：Type0(font) → CIDFontType2 → FontDescriptor → FontFile2 +
    // Identity-H encoding CMap + ToUnicode CMap（stream 默认无 filter = 未压缩
    // 存储面——零 FlateDecode 依赖）。
    if has_font_file {
        pdf.stream(font_file, subset_bytes.as_deref().unwrap_or_default());
    }
    pdf.stream(font_cmap_enc, identity_h_cmap());
    pdf.stream(font_tounicode, cmap_bytes.as_slice());

    let mut descriptor = pdf.font_descriptor(font_desc);
    descriptor.name(Name(BASE_FONT));
    descriptor.flags(FontFlags::SYMBOLIC);
    descriptor.bbox(bbox);
    descriptor.italic_angle(italic_angle);
    descriptor.ascent(ascent);
    descriptor.descent(descent);
    descriptor.cap_height(cap_height);
    descriptor.stem_v(80.0);
    if has_font_file {
        descriptor.font_file2(font_file);
    }
    drop(descriptor);

    let mut cid_font = pdf.cid_font(font_cid);
    cid_font.subtype(CidFontType::Type2);
    cid_font.base_font(Name(BASE_FONT));
    cid_font.system_info(SystemInfo {
        registry: Str(b"Adobe"),
        ordering: Str(b"Identity"),
        supplement: 0,
    });
    cid_font.font_descriptor(font_desc);
    cid_font.default_width(default_width);
    if !widths.is_empty() {
        cid_font.widths().consecutive(0, widths.iter().copied());
    }
    cid_font.cid_to_gid_map_predefined(Name(b"Identity"));
    drop(cid_font);

    let mut type0_font = pdf.type0_font(font_type0);
    type0_font.base_font(Name(BASE_FONT));
    type0_font.encoding_cmap(font_cmap_enc);
    type0_font.descendant_font(font_cid);
    type0_font.to_unicode(font_tounicode);
    drop(type0_font);

    // 页树：/Count + /Kids。
    let page_ids: Vec<Ref> = (0..page_count)
        .map(|i| Ref::new(3 + (i as i32) * 2))
        .collect();
    pdf.pages(pages_id)
        .kids(page_ids.iter().copied())
        .count(page_count as i32);

    // 页对象 + 内容流。
    for (i, page_lines) in encoded.pages.iter().enumerate() {
        let page_id = Ref::new(3 + (i as i32) * 2);
        let content_id = Ref::new(4 + (i as i32) * 2);
        let mut content = Content::new();
        for (y, size, cids) in page_lines {
            content.begin_text();
            content.set_font(Name(FONT_KEY), *size);
            content.next_line(MARGIN, *y);
            content.show(Str(cids));
            content.end_text();
        }
        let content_bytes = content.finish();
        pdf.stream(content_id, &content_bytes);

        let mut page = pdf.page(page_id);
        page.parent(pages_id);
        page.media_box(Rect::new(0.0, 0.0, PAGE_W, PAGE_H));
        page.contents(content_id);
        page.resources().fonts().pair(Name(FONT_KEY), font_type0);
    }

    // DocumentInfo：finding_id（ASCII）经 /Subject 字面可见（判据 5 字节面
    // 对账位）；title 中文经 TextStr（UTF-16BE）落 /Title。
    {
        let mut info = pdf.document_info(info_id);
        info.title(TextStr(&ir.title));
        // finding_id + rtis_ref（全 ASCII）经 /Subject 字面 grep（判据 5/
        // qc_pdf 事实可追溯面——rtis:// 词形原样透传零改写）。
        let subject: Vec<String> = ir
            .findings
            .iter()
            .flat_map(|f| {
                let mut items = vec![f.finding_id.clone()];
                if let Some(rtis_ref) = &f.rtis_ref {
                    items.push(rtis_ref.clone());
                }
                items
            })
            .collect();
        info.subject(TextStr(&subject.join(" ")));
        info.keywords(TextStr(&ir.report_id));
        info.creator(TextStr("research-platform-report-engine"));
    }

    pdf.finish()
}
