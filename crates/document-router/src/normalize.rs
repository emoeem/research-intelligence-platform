//! 统一输出规范化（DI-1 ADR D3 Schema）：文本 + 元数据 + 页锚/尺寸 + 警告，
//! Router 组装后经引擎信封 output_schema 校验交付；warnings 如实呈现
//! （office→pdf 转换面声明、嗅探/扩展名不一致、页锚计数与 pdfinfo 不一致），
//! 零静默降级。
//!
//! 解析面均为本机实测冻结形态（D3 否决项：pdfinfo 无 `-json` 旗标、
//! `magick identify -json` 不支持 → 行/单行解析承载）：
//! - pdfinfo 报告：`Pages: \d+`、`Page size: W x H pts`、`PDF version: ...`、
//!   `Producer: ...`；
//! - magick identify `-format "%m %wx%h %[colorspace] depth=%z"` 单行：
//!   `PNG 834x250 sRGB depth=8`；
//! - pdftotext 文本页锚 = `\f`（0x0C）分隔切分（sample.pdf 实测 2 页 = 2 个
//!   `\f`）。

use serde_json::{json, Value};

/// pdfinfo 报告解析产物（D3：pages、page_size{width,height,unit:"pts"}、
/// pdf_version、producer 可得则带）。
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PdfinfoMeta {
    /// `Pages:` 行值。
    pub pages: Option<u64>,
    /// `Page size: W x H pts` 的 W。
    pub page_width: Option<u64>,
    /// `Page size: W x H pts` 的 H。
    pub page_height: Option<u64>,
    /// `PDF version:` 行值。
    pub pdf_version: Option<String>,
    /// `Producer:` 行值。
    pub producer: Option<String>,
}

impl PdfinfoMeta {
    /// 报告文本解析（行首精确匹配，容忍 CRLF）。
    pub fn parse(stdout: &str) -> Self {
        let mut meta = Self::default();
        for line in stdout.lines() {
            let line = line.trim_end_matches('\r');
            if let Some(rest) = line.strip_prefix("Pages:") {
                meta.pages = rest.trim().parse::<u64>().ok();
            } else if let Some(rest) = line.strip_prefix("Page size:") {
                // `Page size: 595 x 842 pts (A4)`：取 W x H 数值。
                let rest = rest.trim();
                let mut it = rest.split_whitespace();
                if let (Some(w), Some("x"), Some(h)) = (it.next(), it.next(), it.next()) {
                    meta.page_width = w.parse::<u64>().ok();
                    meta.page_height = h.parse::<u64>().ok();
                }
            } else if let Some(rest) = line.strip_prefix("PDF version:") {
                meta.pdf_version = Some(rest.trim().to_string());
            } else if let Some(rest) = line.strip_prefix("Producer:") {
                meta.producer = Some(rest.trim().to_string());
            }
        }
        meta
    }

    /// 统一输出 `metadata` 对象（pdf/office 通道；可得则带，缺项零编造）。
    pub fn to_metadata(&self) -> Value {
        let mut m = serde_json::Map::new();
        if let Some(p) = self.pages {
            m.insert("pages".into(), json!(p));
        }
        if let (Some(w), Some(h)) = (self.page_width, self.page_height) {
            m.insert(
                "page_size".into(),
                json!({"width": w, "height": h, "unit": "pts"}),
            );
        }
        if let Some(v) = &self.pdf_version {
            m.insert("pdf_version".into(), json!(v));
        }
        if let Some(p) = &self.producer {
            m.insert("producer".into(), json!(p));
        }
        Value::Object(m)
    }
}

/// magick identify 单行解析产物（D3：format/width/height/colorspace/depth）。
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ImageMeta {
    /// 魔数词（`PNG` / `JPEG`，与嗅探签名一致面）。
    pub format: Option<String>,
    /// `WxH` 的 W。
    pub width: Option<u64>,
    /// `WxH` 的 H。
    pub height: Option<u64>,
    /// 颜色空间（`sRGB` 等）。
    pub colorspace: Option<String>,
    /// 位深（`depth=%z`）。
    pub depth: Option<u64>,
}

impl ImageMeta {
    /// 单行解析（`%m %wx%h %[colorspace] depth=%z` → `PNG 834x250 sRGB depth=8`）。
    pub fn parse(stdout: &str) -> Self {
        let mut meta = Self::default();
        let line = stdout.lines().next().unwrap_or("").trim_end_matches('\r');
        let mut it = line.split_whitespace();
        if let Some(fmt) = it.next() {
            meta.format = Some(fmt.to_string());
        }
        if let Some(dims) = it.next() {
            if let Some((w, h)) = dims.split_once('x') {
                meta.width = w.parse::<u64>().ok();
                meta.height = h.parse::<u64>().ok();
            }
        }
        if let Some(cs) = it.next() {
            meta.colorspace = Some(cs.to_string());
        }
        for token in it {
            if let Some(rest) = token.strip_prefix("depth=") {
                meta.depth = rest.parse::<u64>().ok();
            }
        }
        meta
    }

    /// 统一输出 `metadata` 对象（image 通道）。
    pub fn to_metadata(&self) -> Value {
        let mut m = serde_json::Map::new();
        if let Some(v) = &self.format {
            m.insert("format".into(), json!(v));
        }
        if let (Some(w), Some(h)) = (self.width, self.height) {
            m.insert("dimensions".into(), json!({"width": w, "height": h}));
        }
        if let Some(v) = &self.colorspace {
            m.insert("colorspace".into(), json!(v));
        }
        if let Some(v) = self.depth {
            m.insert("depth".into(), json!(v));
        }
        Value::Object(m)
    }

    /// 像素总数（D7 #8 像素帽定案输入；缺尺寸返回 None 不臆测）。
    pub fn pixel_count(&self) -> Option<u64> {
        Some(self.width?.saturating_mul(self.height?))
    }
}

/// 页锚切分（D3：按 `\f`（0x0C）分隔；返回去分隔符正文与逐页 char 区间）。
///
/// pdftotext 在每页末尾输出一个 `\f`（含末页）：`p1\x0Cp2\x0C` → 2 页。正文
/// = 去除 `\f` 后的全文；页锚 = 逐页在该正文中的 char 区间（char_start 含、
/// char_end 不含；空页区间为 start==end）。
pub fn page_anchors(text_with_feeds: &str) -> (String, Vec<Value>) {
    let pages: Vec<&str> = text_with_feeds.split('\u{0C}').collect();
    // 末尾 `\f` 产生空尾段：属分页符形态而非第 N+1 空页。
    let mut pages: Vec<&str> =
        if pages.len() > 1 && pages.last().map(|s| s.is_empty()) == Some(true) {
            pages[..pages.len() - 1].to_vec()
        } else {
            pages
        };
    if pages.is_empty() {
        pages.push("");
    }
    let mut clean = String::with_capacity(text_with_feeds.len());
    let mut anchors = Vec::with_capacity(pages.len());
    for (idx, page) in pages.iter().enumerate() {
        let start = clean.chars().count();
        clean.push_str(page);
        let end = clean.chars().count();
        anchors.push(json!({
            "page": idx + 1,
            "char_start": start,
            "char_end": end,
        }));
    }
    (clean, anchors)
}

/// 统一输出构造（D3 Schema 全形状；经子 run Manifest output_schema 校验交付
/// ——统一输出本体由 Router 组装，超信封帽由 envelope::truncate 降级 +
/// usage.truncated，D7 #10）。
pub struct UnifiedDocInput<'a> {
    /// 通道字面量（pdf | office | image）。
    pub channel: &'a str,
    /// 暂存名（staged 名，D5）。
    pub source_name: &'a str,
    /// 源文件字节数。
    pub source_bytes: u64,
    /// 源文件 sha256（64hex 小写）。
    pub source_sha256: &'a str,
    /// 嗅探魔数字面量。
    pub sniff_magic: &'a str,
    /// 输入扩展名（小写）。
    pub ext: &'a str,
    /// 嗅探/扩展名不一致标记（D6：魔数权威）。
    pub mismatch: bool,
    /// 正文（pdf/office 为抽取文本；image 为 ""）。
    pub text: String,
    /// metadata 对象（通道面：pdfinfo/identify/office 附加）。
    pub metadata: Value,
    /// 页锚数组（pdf/office；image 为 []）。
    pub page_anchors: Vec<Value>,
    /// 警告（如实呈现）。
    pub warnings: Vec<String>,
    /// 本链全部子 run_id（ToolRun 行锚）。
    pub runs: Vec<String>,
}

/// 组装统一输出 Value（D3 Schema 形状）。
pub fn build_unified(input: UnifiedDocInput<'_>) -> Value {
    json!({
        "channel": input.channel,
        "source": {
            "kind": "file",
            "name": input.source_name,
            "bytes": input.source_bytes,
            "sha256": input.source_sha256,
            "sniff": {
                "magic": input.sniff_magic,
                "ext": input.ext,
                "mismatch": input.mismatch,
            },
        },
        "text": input.text,
        "metadata": input.metadata,
        "page_anchors": input.page_anchors,
        "warnings": input.warnings,
        "runs": input.runs,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// D3 实测锚点：sample.pdf 的 pdfinfo 报告要点（Pages: 2 / 595 x 842 pts
    /// (A4) / PDF version: 1.7）。
    #[test]
    fn pdfinfo_report_parses_d3_anchor_facts() {
        let report = "Creator:        Writer\n\
                      Producer:       LibreOffice 26.8\n\
                      CreationDate:   Mon Nov 15 18:36:00 2026\n\
                      Pages:          2\n\
                      Encrypted:      no\n\
                      Page size:      595 x 842 pts (A4)\n\
                      PDF version:    1.7\n";
        let meta = PdfinfoMeta::parse(report);
        assert_eq!(meta.pages, Some(2));
        assert_eq!(meta.page_width, Some(595));
        assert_eq!(meta.page_height, Some(842));
        assert_eq!(meta.pdf_version.as_deref(), Some("1.7"));
        assert_eq!(meta.producer.as_deref(), Some("LibreOffice 26.8"));
        let m = meta.to_metadata();
        assert_eq!(m["pages"], json!(2));
        assert_eq!(m["page_size"]["width"], json!(595));
        assert_eq!(m["page_size"]["unit"], json!("pts"));
        assert_eq!(m["pdf_version"], json!("1.7"));
    }

    /// 缺项零编造：无 Producer 行时不产出 producer 键。
    #[test]
    fn pdfinfo_missing_fields_are_omitted() {
        let meta = PdfinfoMeta::parse("Pages: 1\n");
        let m = meta.to_metadata();
        assert!(m.get("producer").is_none());
        assert!(m.get("page_size").is_none());
        assert!(m.get("pdf_version").is_none());
        assert_eq!(m["pages"], json!(1));
    }

    /// D3 实测锚点：`PNG 834x250 sRGB depth=8` 单行解析。
    #[test]
    fn identify_line_parses_d3_anchor_facts() {
        let meta = ImageMeta::parse("PNG 834x250 sRGB depth=8\n");
        assert_eq!(meta.format.as_deref(), Some("PNG"));
        assert_eq!(meta.width, Some(834));
        assert_eq!(meta.height, Some(250));
        assert_eq!(meta.colorspace.as_deref(), Some("sRGB"));
        assert_eq!(meta.depth, Some(8));
        assert_eq!(meta.pixel_count(), Some(834 * 250));
        let m = meta.to_metadata();
        assert_eq!(m["format"], json!("PNG"));
        assert_eq!(m["dimensions"]["width"], json!(834));
        assert_eq!(m["colorspace"], json!("sRGB"));
        assert_eq!(m["depth"], json!(8));
    }

    #[test]
    fn identify_malformed_line_omits_fields() {
        let meta = ImageMeta::parse("");
        assert!(meta.format.is_none());
        assert!(meta.pixel_count().is_none());
        let meta = ImageMeta::parse("JPEG notdims sRGB depth=16");
        assert_eq!(meta.format.as_deref(), Some("JPEG"));
        assert!(meta.width.is_none());
        assert_eq!(meta.depth, Some(16));
    }

    /// D3 页锚口径：`\f` ×2 = 2 页锚（sample.pdf 实测形态）；正文去分隔符；
    /// 区间 char_start 含 / char_end 不含。
    #[test]
    fn page_anchors_split_on_form_feed() {
        let (clean, anchors) = page_anchors("page-one\u{0C}page-two\u{0C}");
        assert_eq!(clean, "page-onepage-two");
        assert_eq!(anchors.len(), 2, "2 个 \\f = 2 页锚");
        assert_eq!(anchors[0]["page"], json!(1));
        assert_eq!(anchors[0]["char_start"], json!(0));
        assert_eq!(anchors[0]["char_end"], json!(8));
        assert_eq!(anchors[1]["page"], json!(2));
        assert_eq!(anchors[1]["char_start"], json!(8));
        assert_eq!(anchors[1]["char_end"], json!(16));
        // 无分页符单页形态。
        let (clean, anchors) = page_anchors("solo");
        assert_eq!(clean, "solo");
        assert_eq!(anchors.len(), 1);
    }

    #[test]
    fn unified_doc_matches_d3_schema_shape() {
        let v = build_unified(UnifiedDocInput {
            channel: "pdf",
            source_name: "sample.pdf",
            source_bytes: 2279,
            source_sha256: "ab".repeat(32).as_str(),
            sniff_magic: "pdf",
            ext: "pdf",
            mismatch: false,
            text: "body".into(),
            metadata: json!({"pages": 2}),
            page_anchors: vec![json!({"page": 1, "char_start": 0, "char_end": 4})],
            warnings: vec![],
            runs: vec!["rn-a".into(), "rn-b".into()],
        });
        assert_eq!(v["channel"], json!("pdf"));
        assert_eq!(v["source"]["kind"], json!("file"));
        assert_eq!(v["source"]["name"], json!("sample.pdf"));
        assert_eq!(v["source"]["sniff"]["mismatch"], json!(false));
        assert_eq!(v["text"], json!("body"));
        assert_eq!(v["metadata"]["pages"], json!(2));
        assert_eq!(v["page_anchors"].as_array().unwrap().len(), 1);
        assert_eq!(v["warnings"], json!([]));
        assert_eq!(v["runs"].as_array().unwrap().len(), 2);
    }
}
