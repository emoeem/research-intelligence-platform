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

/// tesseract TSV 解析产物（DI-2 ADR D2 实测冻结形态）：首行 header
/// `level page_num block_num par_num line_num word_num left top width height
/// conf text`；词行 = level 5；正文重组 = 按 (block,par,line) 分组、词间
/// 空格连接、行间 `\n` 连接；置信度 = level 5 行 conf 均值（tesseract TSV
/// conf 实测浮点形态 92.203033，保留 1 位小数）。
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TessTsvMeta {
    /// 重组正文（TSV 词行 text 列重组；单 run 承载非二次 OCR）。
    pub text: String,
    /// level 5 词行计数。
    pub words: u64,
    /// level 5 行 conf 均值（保留 1 位小数；零词行 None——零编造）。
    pub mean_conf: Option<f64>,
    /// conf < 60 的词数（低置信诚实面）。
    pub low_conf_words: u64,
}

/// TSV 行字段数（header 12 列：level..text）。
const TSV_COLUMNS: usize = 12;
/// 词行 level 值（tesseract TSV 实测：5 = 词行）。
const TSV_WORD_LEVEL: &str = "5";
/// 低置信阈值（D2：mean_conf < 60 → warnings 追加诚实记录）。
pub const OCR_LOW_CONF_THRESHOLD: f64 = 60.0;

impl TessTsvMeta {
    /// TSV stdout 解析（tab 分列；`tesseract <in> stdout -l <lang> tsv`
    /// 实测形态）。
    pub fn parse(stdout: &str) -> Self {
        let mut meta = Self::default();
        let mut conf_sum = 0.0f64;
        let mut conf_count = 0u64;
        let mut lines: Vec<String> = Vec::new();
        let mut current_group: Option<(String, String, String)> = None;
        let mut current_line = String::new();
        for raw in stdout.lines() {
            let line = raw.trim_end_matches('\r');
            if line.is_empty() {
                continue;
            }
            let cols: Vec<&str> = line.split('\t').collect();
            if cols.len() < TSV_COLUMNS {
                continue; // header 行 / 截断行不计词
            }
            if cols[0] == "level" {
                continue; // header
            }
            if cols[0] != TSV_WORD_LEVEL {
                continue; // 页/块/段落锚行非词行
            }
            meta.words += 1;
            if let Ok(conf) = cols[10].trim().parse::<f64>() {
                conf_sum += conf;
                conf_count += 1;
                if conf < OCR_LOW_CONF_THRESHOLD {
                    meta.low_conf_words += 1;
                }
            }
            let word = cols[11];
            let group = (
                cols[2].to_string(),
                cols[3].to_string(),
                cols[4].to_string(),
            );
            match &current_group {
                Some(g) if *g == group => {
                    if !current_line.is_empty() {
                        current_line.push(' ');
                    }
                    current_line.push_str(word);
                }
                _ => {
                    if current_group.take().is_some() {
                        lines.push(std::mem::take(&mut current_line));
                    }
                    current_group = Some(group);
                    current_line.push_str(word);
                }
            }
        }
        if current_group.is_some() {
            lines.push(current_line);
        }
        meta.text = lines.join("\n");
        if conf_count > 0 {
            meta.mean_conf = Some((conf_sum / conf_count as f64 * 10.0).round() / 10.0);
        }
        meta
    }

    /// 统一输出 `metadata.ocr` 块（D2 Schema；language/requested_language 由
    /// Router 路由面补入）。
    pub fn to_metadata(&self) -> Value {
        let mut ocr = serde_json::Map::new();
        if let Some(conf) = self.mean_conf {
            ocr.insert("mean_conf".into(), json!(conf));
        }
        ocr.insert("words".into(), json!(self.words));
        ocr.insert("low_conf_words".into(), json!(self.low_conf_words));
        Value::Object(ocr)
    }
}

/// ffprobe JSON 解析产物（DI-2 ADR D3 实测冻结形态）：`format.duration` /
/// `format.bit_rate` 为**字符串形态**（实测 `"2.000000"` / `"128704"`），
/// `streams[].width/height/channels` 为**整数形态**；缺项零编造。
#[derive(Debug, Clone, PartialEq, Default)]
pub struct FfprobeMeta {
    /// format.format_name（mp4 实测逗号列表 `mov,mp4,m4a,3gp,3g2,mj2` 原样承载）。
    pub format_name: Option<String>,
    /// format.duration 浮点化（字符串形态实测；缺席 None 零编造）。
    pub duration_s: Option<f64>,
    /// format.bit_rate 数值化（字符串形态实测）。
    pub bit_rate: Option<u64>,
    /// format.size 数值化。
    pub size_bytes: Option<u64>,
    /// streams 精简投影（全量 JSON 留 stdout.log）。
    pub streams: Vec<StreamMeta>,
}

/// ffprobe 单流精简投影（D3 metadata.media.streams 形状）。
#[derive(Debug, Clone, PartialEq, Default)]
pub struct StreamMeta {
    /// codec_type（audio / video / ...）。
    pub codec_type: Option<String>,
    /// codec_name（pcm_s16le / h264 / ...）。
    pub codec_name: Option<String>,
    /// 视频流宽（整数形态实测）。
    pub width: Option<u64>,
    /// 视频流高（整数形态实测）。
    pub height: Option<u64>,
    /// 音频流采样率（字符串形态实测 `"8000"`）。
    pub sample_rate: Option<u64>,
    /// 音频流声道数（整数形态实测）。
    pub channels: Option<u64>,
}

impl StreamMeta {
    fn from_json(v: &Value) -> Self {
        let s = |key: &str| v.get(key).and_then(Value::as_str).map(str::to_string);
        Self {
            codec_type: s("codec_type"),
            codec_name: s("codec_name"),
            width: v.get("width").and_then(Value::as_u64),
            height: v.get("height").and_then(Value::as_u64),
            sample_rate: v
                .get("sample_rate")
                .and_then(Value::as_str)
                .and_then(|s| s.parse::<u64>().ok()),
            channels: v.get("channels").and_then(Value::as_u64),
        }
    }

    fn to_json(&self) -> Value {
        let mut m = serde_json::Map::new();
        if let Some(v) = &self.codec_type {
            m.insert("codec_type".into(), json!(v));
        }
        if let Some(v) = &self.codec_name {
            m.insert("codec_name".into(), json!(v));
        }
        if let (Some(w), Some(h)) = (self.width, self.height) {
            m.insert("width".into(), json!(w));
            m.insert("height".into(), json!(h));
        }
        if let Some(v) = self.sample_rate {
            m.insert("sample_rate".into(), json!(v));
        }
        if let Some(v) = self.channels {
            m.insert("channels".into(), json!(v));
        }
        Value::Object(m)
    }

    /// 单帧像素数（视频流；缺尺寸 None 不臆测）。
    pub fn pixel_count(&self) -> Option<u64> {
        Some(self.width?.saturating_mul(self.height?))
    }
}

impl FfprobeMeta {
    /// ffprobe `-print_format json -show_format -show_streams` stdout 解析。
    pub fn parse(stdout: &str) -> Self {
        let mut meta = Self::default();
        let Ok(v) = serde_json::from_str::<Value>(stdout) else {
            return meta;
        };
        if let Some(format) = v.get("format") {
            meta.format_name = format
                .get("format_name")
                .and_then(Value::as_str)
                .map(str::to_string);
            meta.duration_s = format
                .get("duration")
                .and_then(Value::as_str)
                .and_then(|s| s.parse::<f64>().ok());
            meta.bit_rate = format
                .get("bit_rate")
                .and_then(Value::as_str)
                .and_then(|s| s.parse::<u64>().ok());
            meta.size_bytes = format
                .get("size")
                .and_then(Value::as_str)
                .and_then(|s| s.parse::<u64>().ok());
        }
        if let Some(streams) = v.get("streams").and_then(Value::as_array) {
            meta.streams = streams.iter().map(StreamMeta::from_json).collect();
        }
        meta
    }

    /// 首个视频流（codec_type=video；缺席 None——纯音频分支判据）。
    pub fn video_stream(&self) -> Option<&StreamMeta> {
        self.streams
            .iter()
            .find(|s| s.codec_type.as_deref() == Some("video"))
    }

    /// 统一输出 `metadata.media` 块（D3 Schema：缺项零编造；nb_frames 不入
    /// metadata——流级帧数与抽帧计数语义易混，抽帧计数以 out/ 实收为准）。
    pub fn to_metadata(&self) -> Value {
        let mut m = serde_json::Map::new();
        if let Some(v) = &self.format_name {
            m.insert("format_name".into(), json!(v));
        }
        if let Some(v) = self.duration_s {
            m.insert("duration_s".into(), json!(v));
        }
        if let Some(v) = self.bit_rate {
            m.insert("bit_rate".into(), json!(v));
        }
        if let Some(v) = self.size_bytes {
            m.insert("size_bytes".into(), json!(v));
        }
        if !self.streams.is_empty() {
            m.insert(
                "streams".into(),
                json!(self
                    .streams
                    .iter()
                    .map(StreamMeta::to_json)
                    .collect::<Vec<_>>()),
            );
        }
        Value::Object(m)
    }
}

/// `7z l -slt` 技术清单解析产物（DI-2 ADR D4 实测冻结形态）：头块
/// （`Path = <archive>` / `Type = zip|7z|…` / `Physical Size`）以
/// `----------` 分隔线与条目块分界；条目块字段 `Path`（穿越/绝对路径逐字
/// 原样呈现）、`Folder = +|-`、`Size`、`Attributes`（zip 符号链接条目实测
/// `lrwxrwxrwx` 权限前缀）、`Symbolic Link = <目标>`（tar 形态）。
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ArchiveListMeta {
    /// 头块 `Type = ` 值（容器格式；zip / 7z / …）。None = 头块未解析。
    pub format: Option<String>,
    /// 条目块清单（`----------` 之后的逐块解析）。
    pub entries: Vec<ArchiveEntryMeta>,
}

/// 单条目块解析产物（闸 1-4 判定输入）。
#[derive(Debug, Clone, PartialEq)]
pub struct ArchiveEntryMeta {
    /// 条目路径（7z -slt 逐字原样呈现——闸 1 判定输入完整性前提）。
    pub path: String,
    /// `Folder = +`（目录条目）。
    pub folder: bool,
    /// 声明解压字节数（`Size = `；缺席按 0——目录条目典型形态）。
    pub size: u64,
    /// `Attributes = ` 值原样（含权限前导位）。
    pub attributes: Option<String>,
    /// `Symbolic Link = ` 目标（tar 形态；zip 符号链接无此字段）。
    pub symlink_target: Option<String>,
}

impl ArchiveEntryMeta {
    /// 符号链接判定（D4 闸 2 双判据）：`Symbolic Link = ` 字段非空，
    /// 或 Attributes 含 `l` 权限前缀（`lrwxrwxrwx` 实测形态——zip unix 模式
    /// 位承载；普通文件 `-` 起头、目录 `D` 起头）。
    pub fn is_symlink(&self) -> bool {
        if self.symlink_target.as_deref().map(|s| !s.trim().is_empty()) == Some(true) {
            return true;
        }
        self.attributes
            .as_deref()
            .map(|a| a.trim_start().starts_with('l') || a.contains("lrwxrwxrwx"))
            .unwrap_or(false)
    }
}

impl ArchiveListMeta {
    /// `-slt` stdout 解析：定位 `----------` 分隔线（首个全横线行），前段
    /// 提头块 `Type`（banner 行无 ` = ` 形态不干扰），后段按空行分块逐条目
    /// 解析（`Key = Value` 形态，`split_once(" = ")`）。
    pub fn parse(stdout: &str) -> Self {
        let mut meta = Self::default();
        let sep_idx = stdout.lines().position(|l| {
            let t = l.trim();
            t.len() >= 10 && t.chars().all(|c| c == '-')
        });
        let Some(sep_idx) = sep_idx else {
            return meta; // 无分隔线：头块未解析（Router 判 ARCHIVE_LIST_NO_OUTPUT 面）
        };
        for line in stdout.lines().take(sep_idx) {
            if let Some(v) = line.trim_start().strip_prefix("Type =") {
                meta.format = Some(v.trim().to_string());
            }
        }
        let entry_text: Vec<&str> = stdout.lines().skip(sep_idx + 1).collect();
        let mut blocks: Vec<Vec<(&str, &str)>> = Vec::new();
        let mut current: Vec<(&str, &str)> = Vec::new();
        for line in entry_text {
            let t = line.trim();
            if t.is_empty() {
                if !current.is_empty() {
                    blocks.push(std::mem::take(&mut current));
                }
                continue;
            }
            if let Some((k, v)) = t.split_once(" = ") {
                current.push((k, v));
            }
        }
        if !current.is_empty() {
            blocks.push(current);
        }
        for block in blocks {
            let get = |key: &str| -> Option<String> {
                block
                    .iter()
                    .find(|(k, _)| *k == key)
                    .map(|(_, v)| v.to_string())
            };
            let Some(path) = get("Path") else {
                continue;
            };
            meta.entries.push(ArchiveEntryMeta {
                folder: get("Folder").as_deref() == Some("+"),
                size: get("Size")
                    .and_then(|s| s.trim().parse::<u64>().ok())
                    .unwrap_or(0),
                attributes: get("Attributes"),
                symlink_target: get("Symbolic Link"),
                path,
            });
        }
        meta
    }

    /// 解压总量声明（Σ Size，全条目；闸 4 第一判据输入）。
    pub fn total_uncompressed_bytes(&self) -> u64 {
        self.entries.iter().map(|e| e.size).sum()
    }
}
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

    // -----------------------------------------------------------------
    // DI-2 三解析面（D2 TSV / D3 ffprobe JSON / D4 7z -slt）
    // -----------------------------------------------------------------

    /// DI-2 D2 TSV 重组规则钉死：header 跳过、level 5 词行、(block,par,line)
    /// 分组、词间空格、行间 `\n`、conf 均值 1 位小数、低置信计数。
    #[test]
    fn tesseract_tsv_parse_reassembles_d2_anchor() {
        // 形态按本机实测：conf 浮点、text 列可能含空格（tab 分列安全）。
        let tsv = "level\tpage_num\tblock_num\tpar_num\tline_num\tword_num\tleft\ttop\twidth\theight\tconf\ttext\n\
                   1\t1\t0\t0\t0\t0\t0\t0\t460\t80\t-1\t\n\
                   5\t1\t1\t1\t1\t1\t20\t30\t100\t20\t92.203033\tHELLO\n\
                   5\t1\t1\t1\t1\t2\t130\t30\t60\t20\t92.232201\tOCR\n\
                   5\t1\t1\t1\t2\t1\t20\t60\t140\t20\t96.249260\t123\n";
        let meta = TessTsvMeta::parse(tsv);
        assert_eq!(meta.words, 3);
        assert_eq!(
            meta.text, "HELLO OCR\n123",
            "(block,par,line) 分组 + 行间 \\n"
        );
        let mean = meta.mean_conf.unwrap();
        assert!(
            (mean - (92.203033 + 92.232201 + 96.249260) / 3.0).abs() < 0.1,
            "conf 均值 1 位小数：{mean}"
        );
        assert_eq!(meta.low_conf_words, 0);
        let m = meta.to_metadata();
        assert_eq!(m["words"], serde_json::json!(3));
        assert_eq!(m["low_conf_words"], serde_json::json!(0));
        // 无词行（空白图 exit 0 实测形态）→ words=0、mean_conf None（零编造）。
        let empty = TessTsvMeta::parse("level\tpage_num\tblock_num\tpar_num\tline_num\tword_num\tleft\ttop\twidth\theight\tconf\ttext\n");
        assert_eq!(empty.words, 0);
        assert_eq!(empty.text, "");
        assert!(empty.mean_conf.is_none());
    }

    /// DI-2 D2 低置信面：conf < 60 词如实计数。
    #[test]
    fn tesseract_tsv_counts_low_conf_words() {
        let tsv = "level\tpage_num\tblock_num\tpar_num\tline_num\tword_num\tleft\ttop\twidth\theight\tconf\ttext\n\
                   5\t1\t1\t1\t1\t1\t0\t0\t10\t10\t55.500000\tmeh\n\
                   5\t1\t1\t1\t1\t2\t20\t0\t10\t10\t95.000000\tok\n";
        let meta = TessTsvMeta::parse(tsv);
        assert_eq!(meta.words, 2);
        assert_eq!(meta.low_conf_words, 1);
    }

    /// DI-2 D3 ffprobe 实测冻结形态：duration/bit_rate/nb_frames 字符串形态
    /// 数值化；width/height/channels 整数形态；format_name 逗号列表原样；
    /// nb_frames 不入 metadata。
    #[test]
    fn ffprobe_json_parses_d3_anchor_facts() {
        let wav = r#"{"format":{"filename":"tone.wav","format_name":"wav","duration":"0.500000","size":"8044","bit_rate":"128704"},"streams":[{"codec_name":"pcm_s16le","codec_type":"audio","sample_rate":"8000","channels":1}]}"#;
        let meta = FfprobeMeta::parse(wav);
        assert_eq!(meta.format_name.as_deref(), Some("wav"));
        assert_eq!(meta.duration_s, Some(0.5));
        assert_eq!(meta.bit_rate, Some(128704));
        assert_eq!(meta.size_bytes, Some(8044));
        assert_eq!(meta.streams.len(), 1);
        assert_eq!(meta.streams[0].codec_name.as_deref(), Some("pcm_s16le"));
        assert_eq!(meta.streams[0].sample_rate, Some(8000));
        assert_eq!(meta.streams[0].channels, Some(1));
        assert!(
            meta.video_stream().is_none(),
            "纯音频无视频流（D3 分支判据）"
        );
        let mp4 = r#"{"format":{"format_name":"mov,mp4,m4a,3gp,3g2,mj2","duration":"2.000000"},"streams":[{"codec_name":"h264","codec_type":"video","width":128,"height":64,"nb_frames":"20"},{"codec_name":"aac","codec_type":"audio","sample_rate":"44100","channels":2}]}"#;
        let meta = FfprobeMeta::parse(mp4);
        assert_eq!(
            meta.format_name.as_deref(),
            Some("mov,mp4,m4a,3gp,3g2,mj2"),
            "format_name 逗号列表原样承载"
        );
        let video = meta.video_stream().expect("video stream found");
        assert_eq!(video.pixel_count(), Some(128 * 64));
        let m = meta.to_metadata();
        assert!(m.get("streams").is_some());
        let stream_json = &m["streams"][0];
        assert_eq!(stream_json["width"], serde_json::json!(128));
        assert!(
            stream_json.get("nb_frames").is_none(),
            "nb_frames 不入 metadata"
        );
    }

    /// DI-2 D3 畸形输出：非 JSON stdout → 全缺省零编造（Router 以子 run
    /// exit!=0 透传为主路径，此为双保险）。
    #[test]
    fn ffprobe_malformed_stdout_defaults_empty() {
        let meta = FfprobeMeta::parse("not audio at all\n");
        assert!(meta.format_name.is_none());
        assert!(meta.duration_s.is_none());
        assert!(meta.streams.is_empty());
    }

    /// DI-2 D4 -slt 解析钉死：头块 Type / 条目块 Path/Folder/Size/Attributes
    /// /穿越名逐字原样 / 符号链接双判据。banner 行（7-Zip 版本行 /
    /// `Listing archive:`）无 ` = ` 形态不干扰。
    #[test]
    fn archive_slt_listing_parses_d4_anchor_facts() {
        let slt = "7-Zip 26.06 (x64) : Copyright (c) 1999-2026 Igor Pavlov : 2026-11-06\n\
                   \n\
                   Listing archive: demo.zip\n\
                   \n\
                   --\n\
                   Path = demo.zip\n\
                   Type = zip\n\
                   Physical Size = 285\n\
                   \n\
                   ----------\n\
                   Path = a.txt\n\
                   Folder = -\n\
                   Size = 6\n\
                   Attributes =  -rw-r--r--\n\
                   \n\
                   Path = ../escape.txt\n\
                   Folder = -\n\
                   Size = 8\n\
                   Attributes =  -rw-r--r--\n\
                   \n\
                   Path = docs\n\
                   Folder = +\n\
                   Size = 0\n\
                   Attributes =  drwxr-xr-x\n\
                   \n\
                   Path = link\n\
                   Folder = -\n\
                   Size = 11\n\
                   Attributes =  lrwxrwxrwx\n";
        let meta = ArchiveListMeta::parse(slt);
        assert_eq!(meta.format.as_deref(), Some("zip"));
        assert_eq!(meta.entries.len(), 4);
        assert_eq!(meta.entries[0].path, "a.txt");
        assert_eq!(meta.entries[0].size, 6);
        assert!(!meta.entries[0].folder);
        assert!(!meta.entries[0].is_symlink());
        // 穿越名逐字原样（闸 1 判定输入完整性）。
        assert_eq!(meta.entries[1].path, "../escape.txt");
        assert!(!meta.entries[1].is_symlink());
        // 目录条目：Folder = + / Attributes D 起头（非符号链接）。
        assert!(meta.entries[2].folder);
        assert_eq!(meta.entries[2].size, 0);
        assert!(!meta.entries[2].is_symlink());
        // zip 符号链接：Attributes lrwxrwxrwx → 闸 2 判真。
        assert!(meta.entries[3].is_symlink(), "attributes lrwxrwxrwx 判链接");
        assert_eq!(meta.total_uncompressed_bytes(), 25);
    }

    /// DI-2 D4 tar 形态符号链接：`Symbolic Link = ` 字段判真（双判据第二面）。
    #[test]
    fn archive_slt_symbolic_link_field_judges_symlink() {
        let slt = "Path = links.tar\n\
                   Type = tar\n\
                   \n\
                   ----------\n\
                   Path = evil\n\
                   Folder = -\n\
                   Size = 5\n\
                   Symbolic Link = /etc/passwd\n";
        let meta = ArchiveListMeta::parse(slt);
        assert_eq!(meta.format.as_deref(), Some("tar"));
        assert_eq!(meta.entries.len(), 1);
        assert!(meta.entries[0].is_symlink(), "Symbolic Link 字段判链接");
    }

    /// DI-2 D4 无分隔线（畸形输出）→ format None / 零条目（Router 判
    /// ARCHIVE_LIST_NO_OUTPUT 面）。
    #[test]
    fn archive_slt_malformed_output_parses_empty() {
        let meta = ArchiveListMeta::parse("garbage output without separator\n");
        assert!(meta.format.is_none());
        assert!(meta.entries.is_empty());
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
