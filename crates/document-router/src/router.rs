//! Router 编排（DI-1 ADR D2/D3/D5/D7/D8）：三通道管线 = 前置门（嗅探/帽，
//! 无 run 行 INPUT 拒绝）→ 预写 in/ → 逐子 run `execute_native_rust_via_engine`
//! → 产物回读 → 规范化（D3 统一输出）。
//!
//! 纪律：
//! - Router 自身零 `std::process::Command`（D2）：每一步工具执行经 tool-engine
//!   公开 API [`tool_engine::adapter::execute_native_rust_via_engine`]，子 run
//!   全部是真实 ToolRun（submit_correlated trace_id = run_id 自锚定，dispatch
//!   先例，判据 10）；
//! - 二进制输入暂存（D5）：Router 在驱动每个子 run 之前预写
//!   `<workspace_root>/<run_id>/in/<staged 名>`；级联产物重写进下一 run 自己的
//!   in/（不引用上一 run 的 out/ 绝对路径——不绕过 in/ 前缀校验）；
//! - 链级失败（页数/像素超帽、soffice 产物缺失、子 run 终态失败）发生于子
//!   run 行已在库之后：[`RouteOutcome::Failed`] 携带子 run 记录（真实终态）
//!   + D8 映射 reject；前置门拒绝才走 `Err(RouterReject)`（无 run 行，D7）；
//! - 帽值在后端强制（[`crate::channel`] D7 表），不只前端；
//! - 统一输出的结构完整性由 [`crate::normalize::build_unified`] 构造承载，
//!   子 run 的 file-mode data 各自经其 Manifest output_schema 由引擎信封
//!   校验（D3「经引擎信封 output_schema 校验交付」的承载面）。

use std::collections::BTreeMap;
use std::path::PathBuf;

use domain::ToolRunStatus;
use serde_json::{json, Value};
use tool_engine::adapter::{execute_native_rust_via_engine, NativeRustAdapter, NativeRustCall};
use tool_engine::capability::CapabilityProvider;
use tool_engine::{DriveOutcome, Engine, EngineError};
use tool_registry::hash::sha256_hex;

use crate::channel::{
    Channel, ARCHIVE_MAX_ENTRIES, ARCHIVE_MAX_ENTRY_BYTES, ARCHIVE_MAX_TOTAL_BYTES, IDENTIFY_FMT,
    MAX_INPUT_FILE_BYTES, MAX_PAGES, MAX_PIXELS, MEDIA_DURATION_MAX_SECONDS, MEDIA_FRAMES_MAX,
    MEDIA_FRAME_FPS, MEDIA_FRAME_PATTERN_NAME, MEDIA_FRAME_PIXELS_MAX, OCR_LANG_DEFAULT_CANDIDATES,
    OCR_LANG_JOIN, OCR_LANG_ROUTES, OCR_PIXELS_MAX, OFFICE_OUT_FORMAT, TOOL_ARCHIVE_EXTRACT,
    TOOL_ARCHIVE_LIST, TOOL_IMAGE_META, TOOL_IMAGE_OCR, TOOL_MEDIA_FRAMES, TOOL_MEDIA_PROBE,
    TOOL_OFFICE_TO_PDF, TOOL_PDF_INFO, TOOL_PDF_TEXT,
};
use crate::error::RouterReject;
use crate::normalize::{
    build_unified, page_anchors, ArchiveListMeta, FfprobeMeta, ImageMeta, PdfinfoMeta, TessTsvMeta,
    UnifiedDocInput, OCR_LOW_CONF_THRESHOLD,
};
use crate::sniff::{self, Magic};

/// Router 配置（D5：workspace_root 与 NativeRustConfig 同一配置源）。
#[derive(Debug, Clone)]
pub struct RouterConfig {
    /// 运行时工作区根（`<root>/<run_id>/{in,out,tmp}`）。
    pub workspace_root: PathBuf,
}

impl Default for RouterConfig {
    fn default() -> Self {
        // 与 NativeRustConfig::default 同源（~/rip-runtime/workspaces）。
        Self {
            workspace_root: tool_engine::adapter::NativeRustConfig::default().workspace_root,
        }
    }
}

/// DI-1 四份 + DI-2 五份通道表 Manifest（D3/D2/D3/D4；json 扫描
/// manifests/tools 同源目录——登记面单一事实源，装载失败即 Err，通道编排
/// 不可半途降级）。
#[derive(Debug, Clone)]
pub struct RouterManifests {
    tools: BTreeMap<String, Value>,
}

impl RouterManifests {
    /// 从目录集装载九份通道表 Manifest（json 形态；缺一即 Err）。
    pub fn from_dirs(dirs: &[PathBuf]) -> Result<Self, String> {
        const REQUIRED: [&str; 9] = [
            TOOL_PDF_INFO,
            TOOL_PDF_TEXT,
            TOOL_OFFICE_TO_PDF,
            TOOL_IMAGE_META,
            TOOL_IMAGE_OCR,
            TOOL_MEDIA_PROBE,
            TOOL_MEDIA_FRAMES,
            TOOL_ARCHIVE_LIST,
            TOOL_ARCHIVE_EXTRACT,
        ];
        let mut tools: BTreeMap<String, Value> = BTreeMap::new();
        for dir in dirs {
            let entries = match std::fs::read_dir(dir) {
                Ok(e) => e,
                Err(_) => continue,
            };
            let mut paths: Vec<PathBuf> = entries
                .filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| p.extension().map(|x| x == "json").unwrap_or(false))
                .collect();
            paths.sort();
            for path in paths {
                let Ok(text) = std::fs::read_to_string(&path) else {
                    continue;
                };
                let Ok(value) = serde_json::from_str::<Value>(&text) else {
                    continue;
                };
                if let Some(id) = value.get("id").and_then(Value::as_str) {
                    if REQUIRED.contains(&id) && !tools.contains_key(id) {
                        tools.insert(id.to_string(), value);
                    }
                }
            }
        }
        let missing: Vec<&str> = REQUIRED
            .iter()
            .filter(|id| !tools.contains_key(**id))
            .copied()
            .collect();
        if !missing.is_empty() {
            return Err(format!(
                "DI-1/DI-2 channel-table manifests missing: {missing:?} (searched {dirs:?}; registered per S4 in manifests/tools/)"
            ));
        }
        Ok(Self { tools })
    }

    /// 通道表内工具 id 判定（dispatch 面承载范围守卫用）。
    pub fn contains(&self, tool_id: &str) -> bool {
        self.tools.contains_key(tool_id)
    }

    fn get(&self, tool_id: &str) -> &Value {
        self.tools
            .get(tool_id)
            .expect("tool id validated against channel table")
    }
}

/// Router（D1：文档域编排层）。
#[derive(Debug, Clone)]
pub struct Router {
    config: RouterConfig,
    manifests: RouterManifests,
}

/// 统一输出的 source/sniff 基座数据（D3 Schema；route() 定案后流入各通道）。
struct UnifiedBase {
    staged: String,
    source_bytes: u64,
    source_sha256: String,
    sniff_magic: &'static str,
    ext: String,
    mismatch: bool,
}

/// 子 run 驱动上下文（D2：引擎 + adapter + capability 一束传递）。
struct DriveCtx<'a> {
    engine: &'a Engine,
    adapter: &'a NativeRustAdapter,
    capability: Option<&'a dyn CapabilityProvider>,
}

impl Router {
    /// 以配置 + 通道表 Manifest 集构造。
    pub fn new(config: RouterConfig, manifests: RouterManifests) -> Self {
        Self { config, manifests }
    }

    /// 路由编排主入口（D6 嗅探权威 → D3 通道表 → D7 帽门 → D3 规范化）。
    ///
    /// 返回：
    /// - `Err(RouterReject)`：前置门拒绝（嗅探无通道签名 / text 不路由 / 文件
    ///   超 64 MiB，无任何 run 行，D7）或链级 INFRA 传输失败（子 run 行可能
    ///   已在库，真实状态留证）；
    /// - `Ok(RouteOutcome)`：编排完成（Succeeded 携带统一输出；Failed 携带
    ///   D8 映射 reject + 全部子 run 记录——已存行保持真实终态，D8 表口径）。
    pub fn route(
        &self,
        engine: &Engine,
        adapter: &NativeRustAdapter,
        capability: Option<&dyn CapabilityProvider>,
        source_bytes: &[u8],
        source_name: &str,
        ext: &str,
    ) -> Result<RouteOutcome, RouterReject> {
        // ---- D7 前置门（先于任何子 run 提交）----
        let bytes_len = source_bytes.len() as u64;
        if bytes_len > MAX_INPUT_FILE_BYTES {
            return Err(RouterReject::input_too_large(
                bytes_len,
                MAX_INPUT_FILE_BYTES,
            ));
        }
        let magic = sniff::sniff(source_bytes);
        let Some(channel) = magic.routed_channel() else {
            return Err(match magic {
                Magic::Text => RouterReject::text_not_routed(ext),
                other => RouterReject::unsupported_type(
                    &other,
                    "zip_other and 7z route to the archive channel since DI-2 (ADR D1 two-phase listing gates); tar/gzip/unknown magic stay fail-closed outside the DI-2 channel table (DI-3 scope)",
                ),
            });
        };
        let mismatch = sniff::mismatch(&magic, ext);
        let mut warnings: Vec<String> = Vec::new();
        if mismatch {
            warnings.push(format!(
                "sniff magic {:?} disagrees with extension hint {ext:?}: routed by magic (authoritative, D6); extension is a hint only",
                magic.as_str()
            ));
        }
        let base = UnifiedBase {
            staged: staged_name(source_name, ext),
            source_bytes: bytes_len,
            source_sha256: sha256_hex(source_bytes),
            sniff_magic: magic.as_str(),
            ext: ext.to_ascii_lowercase(),
            mismatch,
        };
        let mut runs: Vec<RunRecord> = Vec::new();
        let ctx = DriveCtx {
            engine,
            adapter,
            capability,
        };

        // ---- D3 通道表编排 + DI-2 D1/D3/D4 三通道扩展 ----
        let outcome = match channel {
            Channel::Pdf => self.route_pdf(&ctx, source_bytes, &base, &mut runs, &mut warnings)?,
            Channel::Office => self.route_office(
                &ctx,
                source_bytes,
                &base,
                magic.subtype_str(),
                &mut runs,
                &mut warnings,
            )?,
            Channel::Image => {
                self.route_image(&ctx, source_bytes, &base, &mut runs, &mut warnings)?
            }
            // DI-2：OCR 通道不经 route()——显式请求位入口 route_ocr（D2；
            // 嗅探无请求位形态在结构上不可达）。
            Channel::Ocr => {
                unreachable!("OCR channel is entered only via route_ocr (explicit request bit, DI-2 ADR D2); routed_channel() never yields Ocr")
            }
            Channel::Media => {
                self.route_media(&ctx, source_bytes, &base, &mut runs, &mut warnings)?
            }
            Channel::Archive => {
                self.route_archive(&ctx, source_bytes, &base, &mut runs, &mut warnings)?
            }
        };
        Ok(RouteOutcome {
            status: outcome.status,
            unified: (outcome.status == RouteStatus::Succeeded).then_some(outcome.value),
            reject: outcome.reject,
            warnings,
            runs,
        })
    }

    /// PDF 通道（pdfinfo → 页数帽门 → pdftotext；D3 表前两行）。
    fn route_pdf(
        &self,
        ctx: &DriveCtx<'_>,
        source_bytes: &[u8],
        base: &UnifiedBase,
        runs: &mut Vec<RunRecord>,
        warnings: &mut Vec<String>,
    ) -> Result<ChainOutcome, RouterReject> {
        let info = self.drive_step(ctx, TOOL_PDF_INFO, source_bytes, &base.staged, json!({}))?;
        if let Some(reject) = terminal_reject(&info) {
            runs.push(info);
            return Ok(ChainOutcome::failed(reject));
        }
        let report = self.read_stdout(&info.run_id)?;
        runs.push(info);
        let meta = PdfinfoMeta::parse(&report);
        // D7 #7 页数帽（2000）：pdfinfo 解析后定案，超帽中止链（链内 FAILED，
        // 已存 pdfinfo 行保持真实终态，D8 表口径）。帽定案输入缺席（Pages 行
        // 未解析）≠ 静默跳过：warnings 如实记录（与图像通道 identify 尺寸
        // 未解析同款口径，零静默降级——评审修订 2026-11-19）。
        if let Some(pages) = meta.pages {
            if pages > MAX_PAGES {
                return Ok(ChainOutcome::failed(RouterReject::page_budget_exceeded(
                    pages, MAX_PAGES,
                )));
            }
        } else {
            warnings.push(
                "pdfinfo report did not parse a Pages line: page cap left unresolved, chain continues without a page-budget verdict (honest reporting, zero fabrication)"
                    .to_string(),
            );
        }
        let text = self.drive_step(ctx, TOOL_PDF_TEXT, source_bytes, &base.staged, json!({}))?;
        if let Some(reject) = terminal_reject(&text) {
            runs.push(text);
            return Ok(ChainOutcome::failed(reject));
        }
        let raw = self.read_stdout(&text.run_id)?;
        runs.push(text);
        let (clean, anchors) = page_anchors(&raw);
        // 页锚计数与 pdfinfo Pages 不一致 → warnings 如实记录（零静默）。
        if let Some(pages) = meta.pages {
            if anchors.len() as u64 != pages {
                warnings.push(format!(
                    "page anchor count {} disagrees with pdfinfo Pages {pages}: anchors follow the form-feed markers in the extracted text (honest reporting)",
                    anchors.len()
                ));
            }
        }
        Ok(ChainOutcome::succeeded(build_unified(UnifiedDocInput {
            channel: Channel::Pdf.as_str(),
            source_name: &base.staged,
            source_bytes: base.source_bytes,
            source_sha256: &base.source_sha256,
            sniff_magic: base.sniff_magic,
            ext: &base.ext,
            mismatch: base.mismatch,
            text: clean,
            metadata: meta.to_metadata(),
            page_anchors: anchors,
            warnings: std::mem::take(warnings),
            runs: runs.iter().map(|r| r.run_id.clone()).collect(),
        })))
    }

    /// Office 通道（soffice → 确定性回读 → 级联 PDF 通道两子 run；D3 表第三行）。
    fn route_office(
        &self,
        ctx: &DriveCtx<'_>,
        source_bytes: &[u8],
        base: &UnifiedBase,
        subtype: &'static str,
        runs: &mut Vec<RunRecord>,
        warnings: &mut Vec<String>,
    ) -> Result<ChainOutcome, RouterReject> {
        warnings.push(
            "Office text extraction goes through the soffice → PDF → pdftotext cascade (office→pdf conversion-face declaration, D3): conversion losses are reported honestly, not presented as native extraction; txt/cat filters are banned (verified hang-on-host, D3 rejected)".to_string(),
        );
        // 隔离临时 profile（E §4.2）：每 run 唯一，file:/// 本 run tmp/ 派生
        // 绝对 URI；profile 位于 tmp/ 由 adapter cleanup_tmp 用后即焚（D7 #2）。
        let office_run_id = new_run_id();
        let profile_uri = format!(
            "file://{}",
            self.config
                .workspace_root
                .join(&office_run_id)
                .join("tmp")
                .join("lo-profile")
                .display()
        );
        let office = self.drive_step_at(
            ctx,
            TOOL_OFFICE_TO_PDF,
            &office_run_id,
            source_bytes,
            &base.staged,
            json!({"profile_uri": profile_uri, "out_format": OFFICE_OUT_FORMAT}),
        )?;
        if let Some(reject) = terminal_reject(&office) {
            runs.push(office);
            return Ok(ChainOutcome::failed(reject));
        }
        // soffice 命名确定性回读：out/<staged stem>.pdf（D3）。
        let stem = base
            .staged
            .rsplit_once('.')
            .map(|(s, _)| s.to_string())
            .unwrap_or_else(|| base.staged.clone());
        let converted = self
            .config
            .workspace_root
            .join(&office.run_id)
            .join("out")
            .join(format!("{stem}.pdf"));
        let pdf_bytes = match std::fs::read(&converted) {
            Ok(b) => b,
            Err(e) => {
                let run_id = office.run_id.clone();
                runs.push(office);
                return Ok(ChainOutcome::failed(RouterReject::office_no_output(
                    &run_id,
                    &format!(
                        "expected {} missing after a successful soffice exit: {e} (lu*.tmp residues surface in stderr.log honestly)",
                        converted.display()
                    ),
                )));
            }
        };
        runs.push(office);
        let pdf_staged = format!("{stem}.pdf");
        // 级联 PDF 通道（页数帽同门，D7 #7 office 级联同门）。
        let info = self.drive_step(ctx, TOOL_PDF_INFO, &pdf_bytes, &pdf_staged, json!({}))?;
        if let Some(reject) = terminal_reject(&info) {
            runs.push(info);
            return Ok(ChainOutcome::failed(reject));
        }
        let report = self.read_stdout(&info.run_id)?;
        runs.push(info);
        let meta = PdfinfoMeta::parse(&report);
        // office 级联页数帽同门 + 帽定案输入缺席同款 warnings（零静默，
        // 评审修订 2026-11-19；与 route_pdf / 图像通道口径拉齐）。
        if let Some(pages) = meta.pages {
            if pages > MAX_PAGES {
                return Ok(ChainOutcome::failed(RouterReject::page_budget_exceeded(
                    pages, MAX_PAGES,
                )));
            }
        } else {
            warnings.push(
                "pdfinfo report did not parse a Pages line: page cap left unresolved, chain continues without a page-budget verdict (honest reporting, zero fabrication)"
                    .to_string(),
            );
        }
        let text = self.drive_step(ctx, TOOL_PDF_TEXT, &pdf_bytes, &pdf_staged, json!({}))?;
        if let Some(reject) = terminal_reject(&text) {
            runs.push(text);
            return Ok(ChainOutcome::failed(reject));
        }
        let raw = self.read_stdout(&text.run_id)?;
        runs.push(text);
        let (clean, anchors) = page_anchors(&raw);
        if let Some(pages) = meta.pages {
            if anchors.len() as u64 != pages {
                warnings.push(format!(
                    "page anchor count {} disagrees with pdfinfo Pages {pages}: anchors follow the form-feed markers in the extracted text (honest reporting)",
                    anchors.len()
                ));
            }
        }
        let mut metadata = meta.to_metadata();
        if let Some(obj) = metadata.as_object_mut() {
            obj.insert("office_subtype".into(), json!(subtype));
            obj.insert("converted_pdf_bytes".into(), json!(pdf_bytes.len() as u64));
        }
        Ok(ChainOutcome::succeeded(build_unified(UnifiedDocInput {
            channel: Channel::Office.as_str(),
            source_name: &base.staged,
            source_bytes: base.source_bytes,
            source_sha256: &base.source_sha256,
            sniff_magic: base.sniff_magic,
            ext: &base.ext,
            mismatch: base.mismatch,
            text: clean,
            metadata,
            page_anchors: anchors,
            warnings: std::mem::take(warnings),
            runs: runs.iter().map(|r| r.run_id.clone()).collect(),
        })))
    }

    /// 图像通道（magick identify -ping → 像素帽门；D3 表第四行 / D7 #8）。
    fn route_image(
        &self,
        ctx: &DriveCtx<'_>,
        source_bytes: &[u8],
        base: &UnifiedBase,
        runs: &mut Vec<RunRecord>,
        warnings: &mut Vec<String>,
    ) -> Result<ChainOutcome, RouterReject> {
        let run = self.drive_step(
            ctx,
            TOOL_IMAGE_META,
            source_bytes,
            &base.staged,
            json!({"subcommand": "identify", "identify_fmt": IDENTIFY_FMT}),
        )?;
        if let Some(reject) = terminal_reject(&run) {
            runs.push(run);
            return Ok(ChainOutcome::failed(reject));
        }
        let line = self.read_stdout(&run.run_id)?;
        runs.push(run);
        let meta = ImageMeta::parse(&line);
        // D7 #8 像素帽（100 MP）：identify 输出解析定案；超帽链内 FAILED。
        if let Some(pixels) = meta.pixel_count() {
            if pixels > MAX_PIXELS {
                return Ok(ChainOutcome::failed(RouterReject::pixel_budget_exceeded(
                    pixels, MAX_PIXELS,
                )));
            }
        } else {
            warnings.push(
                "identify output did not parse width/height: metadata partially missing (honest reporting, zero fabrication)"
                    .to_string(),
            );
        }
        Ok(ChainOutcome::succeeded(build_unified(UnifiedDocInput {
            channel: Channel::Image.as_str(),
            source_name: &base.staged,
            source_bytes: base.source_bytes,
            source_sha256: &base.source_sha256,
            sniff_magic: base.sniff_magic,
            ext: &base.ext,
            mismatch: base.mismatch,
            text: String::new(),
            metadata: meta.to_metadata(),
            page_anchors: Vec::new(),
            warnings: std::mem::take(warnings),
            runs: runs.iter().map(|r| r.run_id.clone()).collect(),
        })))
    }

    // -----------------------------------------------------------------
    // DI-2 三通道管线（D2 OCR / D3 媒体 / D4 归档；复用前置门 + drive_step/
    // read_stdout 既有面，零新子进程类型，红线 7）
    // -----------------------------------------------------------------

    /// OCR 通道（DI-2 ADR D2）：**显式请求位入口**——dispatch 层 input.ocr
    /// 在场时调用（`route()` 签名与既有管线零改动，红线 9）。图像嗅探签名
    /// （Png | Jpeg）+ 请求位并存才路由 OCR；语言路由以 capability 目录
    /// ocr.tesseract 条目 language 集为唯一权威（chi_tra 缺失前置门显式
    /// error 零 run 行）；identify 门步 50 MP 像素帽（OCR 全量解码派生帽）
    /// → tesseract TSV 单 run 承载文本 + 置信度。
    #[allow(clippy::too_many_arguments)]
    pub fn route_ocr(
        &self,
        engine: &Engine,
        adapter: &NativeRustAdapter,
        capability: Option<&dyn CapabilityProvider>,
        source_bytes: &[u8],
        source_name: &str,
        ext: &str,
        requested_language: Option<&str>,
    ) -> Result<RouteOutcome, RouterReject> {
        // ---- D7 前置门（同 route() 门层；64 MiB 输入帽 + 图像签名限定）----
        let bytes_len = source_bytes.len() as u64;
        if bytes_len > MAX_INPUT_FILE_BYTES {
            return Err(RouterReject::input_too_large(
                bytes_len,
                MAX_INPUT_FILE_BYTES,
            ));
        }
        let magic = sniff::sniff(source_bytes);
        if !matches!(magic, Magic::Png | Magic::Jpeg) {
            return Err(match magic {
                Magic::Text => RouterReject::text_not_routed(ext),
                other => RouterReject::unsupported_type(
                    &other,
                    "the OCR channel serves image input only (PNG/JPEG sniff signatures + explicit request bit, DI-2 ADR D2); PDF render OCR is DI-3 scope",
                ),
            });
        }
        // 语言路由（catalog 唯一权威；chi_tra 面前置门拒绝，零 run 行）。
        let (ocr_lang, requested_label) =
            Self::resolve_ocr_language(capability, requested_language)?;
        let mismatch = sniff::mismatch(&magic, ext);
        let mut warnings: Vec<String> = Vec::new();
        if mismatch {
            warnings.push(format!(
                "sniff magic {:?} disagrees with extension hint {ext:?}: routed by magic (authoritative, D6); extension is a hint only",
                magic.as_str()
            ));
        }
        let base = UnifiedBase {
            staged: staged_name(source_name, ext),
            source_bytes: bytes_len,
            source_sha256: sha256_hex(source_bytes),
            sniff_magic: magic.as_str(),
            ext: ext.to_ascii_lowercase(),
            mismatch,
        };
        let mut runs: Vec<RunRecord> = Vec::new();
        let ctx = DriveCtx {
            engine,
            adapter,
            capability,
        };
        let outcome = self.route_ocr_chain(
            &ctx,
            source_bytes,
            &base,
            &ocr_lang,
            &requested_label,
            &mut runs,
            &mut warnings,
        )?;
        Ok(RouteOutcome {
            status: outcome.status,
            unified: (outcome.status == RouteStatus::Succeeded).then_some(outcome.value),
            reject: outcome.reject,
            warnings,
            runs,
        })
    }

    /// OCR 通道链本体（identify 门步 → 像素帽 → tesseract TSV → 规范化）。
    #[allow(clippy::too_many_arguments)]
    fn route_ocr_chain(
        &self,
        ctx: &DriveCtx<'_>,
        source_bytes: &[u8],
        base: &UnifiedBase,
        ocr_lang: &str,
        requested_label: &str,
        runs: &mut Vec<RunRecord>,
        warnings: &mut Vec<String>,
    ) -> Result<ChainOutcome, RouterReject> {
        // 门步 1：identify（复用 DI-1 既有 Manifest 零改；-ping 仅头部读，
        // 50 MP 判定零解码成本——像素内存放大器在 OCR 派生前封死）。
        let identify = self.drive_step(
            ctx,
            TOOL_IMAGE_META,
            source_bytes,
            &base.staged,
            json!({"subcommand": "identify", "identify_fmt": IDENTIFY_FMT}),
        )?;
        if let Some(reject) = terminal_reject(&identify) {
            runs.push(identify);
            return Ok(ChainOutcome::failed(reject));
        }
        let line = self.read_stdout(&identify.run_id)?;
        runs.push(identify);
        let meta = ImageMeta::parse(&line);
        // OCR 派生像素帽（50 MP，D2/D5；码沿用，消息注明 OCR 帽）。
        if let Some(pixels) = meta.pixel_count() {
            if pixels > OCR_PIXELS_MAX {
                return Ok(ChainOutcome::failed(
                    RouterReject::pixel_budget_exceeded_ocr(pixels, OCR_PIXELS_MAX),
                ));
            }
        } else {
            warnings.push(
                "identify output did not parse width/height: metadata partially missing (honest reporting, zero fabrication)"
                    .to_string(),
            );
        }

        // OCR 步 2：tesseract TSV（{{ocr_lang}} 为整 token 槽位，值由路由表
        // 生成后填 input；Manifest input_schema enum 三值二道收紧）。
        let ocr_run = self.drive_step(
            ctx,
            TOOL_IMAGE_OCR,
            source_bytes,
            &base.staged,
            json!({"ocr_lang": ocr_lang}),
        )?;
        if let Some(reject) = terminal_reject(&ocr_run) {
            runs.push(ocr_run);
            return Ok(ChainOutcome::failed(reject));
        }
        let tsv = self.read_stdout(&ocr_run.run_id)?;
        runs.push(ocr_run);
        let tess = TessTsvMeta::parse(&tsv);
        // 空结果 / 低置信 → warnings 如实记录，不拒绝不静默（D2）。
        if tess.words == 0 {
            warnings.push(
                "no words recognized: empty OCR result reported honestly (zero fabrication)"
                    .to_string(),
            );
        }
        if let Some(conf) = tess.mean_conf {
            if conf < OCR_LOW_CONF_THRESHOLD {
                warnings.push(format!(
                    "OCR mean confidence {conf} below low-confidence threshold {OCR_LOW_CONF_THRESHOLD}: reported honestly, not rejected, not silenced"
                ));
            }
        }
        let mut metadata = meta.to_metadata();
        if let Some(obj) = metadata.as_object_mut() {
            let mut ocr = serde_json::Map::new();
            ocr.insert("language".into(), json!(ocr_lang));
            ocr.insert("requested_language".into(), json!(requested_label));
            if let Some(conf) = tess.mean_conf {
                ocr.insert("mean_conf".into(), json!(conf));
            }
            ocr.insert("words".into(), json!(tess.words));
            ocr.insert("low_conf_words".into(), json!(tess.low_conf_words));
            obj.insert("ocr".into(), Value::Object(ocr));
        }
        // 单图单页锚（char 区间 = 重组正文全长）。
        let char_end = tess.text.chars().count();
        Ok(ChainOutcome::succeeded(build_unified(UnifiedDocInput {
            channel: Channel::Ocr.as_str(),
            source_name: &base.staged,
            source_bytes: base.source_bytes,
            source_sha256: &base.source_sha256,
            sniff_magic: base.sniff_magic,
            ext: &base.ext,
            mismatch: base.mismatch,
            text: tess.text,
            metadata,
            page_anchors: vec![json!({
                "page": 1,
                "char_start": 0,
                "char_end": char_end,
            })],
            warnings: std::mem::take(warnings),
            runs: runs.iter().map(|r| r.run_id.clone()).collect(),
        })))
    }

    /// OCR 语言路由（D2：请求语言 → tesseract `-l` 值；capability 目录
    /// language 集为唯一权威）。返回 (tesseract -l 参数, requested_language
    /// 记录形态)。Err = ROUTER_OCR_LANG_MISSING 前置门拒绝（任何子 run 提交
    /// 之前，零 run 行）。
    fn resolve_ocr_language(
        capability: Option<&dyn CapabilityProvider>,
        requested: Option<&str>,
    ) -> Result<(String, String), RouterReject> {
        let label = requested.unwrap_or("auto").to_string();
        let candidates: Vec<String> = match label.as_str() {
            "auto" => OCR_LANG_DEFAULT_CANDIDATES
                .iter()
                .map(|s| s.to_string())
                .collect(),
            other => match OCR_LANG_ROUTES.iter().find(|(k, _)| *k == other) {
                Some((_, v)) => v.iter().map(|s| s.to_string()).collect(),
                // 表外形态：请求原值作候选（目录无对应 language → 拒绝）。
                None => vec![other.to_string()],
            },
        };
        // 目录条目 language 集读取（静态种子纯缓存读；条目缺席 → 语言路由
        // 唯一权威不可得，fail-closed 拒绝）。
        let catalog_langs: Option<std::collections::BTreeSet<String>> = capability.and_then(|p| {
            let id = tool_engine::capability::CapabilityId::new("ocr.tesseract").ok()?;
            p.get(&id).map(|cached| {
                cached
                    .entry
                    .capabilities
                    .iter()
                    .filter(|f| f.kind == tool_engine::capability::FeatureKind::Language)
                    .map(|f| f.name.clone())
                    .collect()
            })
        });
        let Some(catalog_langs) = catalog_langs else {
            return Err(RouterReject::ocr_lang_missing(&label, &candidates));
        };
        let missing: Vec<String> = candidates
            .iter()
            .filter(|c| !catalog_langs.contains(*c))
            .cloned()
            .collect();
        if !missing.is_empty() {
            return Err(RouterReject::ocr_lang_missing(&label, &missing));
        }
        Ok((candidates.join(&OCR_LANG_JOIN.to_string()), label))
    }

    /// 媒体通道（DI-2 ADR D3）：ffprobe JSON 常步 → 门序（畸形透传 / 时长
    /// 300 s / 视频流分支 / 单帧 16 MP / 帧数预算 32）→ ffmpeg 受限抽帧
    /// 条件步（全常量 argv；纯音频 metadata-only 诚实分支）。
    fn route_media(
        &self,
        ctx: &DriveCtx<'_>,
        source_bytes: &[u8],
        base: &UnifiedBase,
        runs: &mut Vec<RunRecord>,
        warnings: &mut Vec<String>,
    ) -> Result<ChainOutcome, RouterReject> {
        // 步 1：ffprobe（畸形媒体 exit != 0 → EXECUTION 透传，terminal_reject）。
        let probe =
            self.drive_step(ctx, TOOL_MEDIA_PROBE, source_bytes, &base.staged, json!({}))?;
        if let Some(reject) = terminal_reject(&probe) {
            runs.push(probe);
            return Ok(ChainOutcome::failed(reject));
        }
        let raw = self.read_stdout(&probe.run_id)?;
        runs.push(probe);
        let meta = FfprobeMeta::parse(&raw);

        // 门 2：内容时长帽 300 s（ffmpeg 派生前定案；duration 字符串形态
        // 解析；缺席 → warnings 如实 + 门不伪装已定案）。
        if let Some(duration) = meta.duration_s {
            if duration > MEDIA_DURATION_MAX_SECONDS {
                return Ok(ChainOutcome::failed(RouterReject::media_duration_exceeded(
                    duration,
                    MEDIA_DURATION_MAX_SECONDS,
                )));
            }
        } else {
            warnings.push(
                "ffprobe output did not parse format.duration: media duration cap left unresolved, chain continues without a duration verdict (honest reporting, zero fabrication)"
                    .to_string(),
            );
        }

        // 门 3：视频流存在性分支（纯音频 → metadata-only + 诚实 warnings，
        // 抽帧步不提交）。
        let Some(video) = meta.video_stream() else {
            warnings.push(
                "no video stream: frame extraction skipped, metadata-only output (honest branch, DI-2 ADR D3)"
                    .to_string(),
            );
            return Ok(ChainOutcome::succeeded(build_unified(UnifiedDocInput {
                channel: Channel::Media.as_str(),
                source_name: &base.staged,
                source_bytes: base.source_bytes,
                source_sha256: &base.source_sha256,
                sniff_magic: base.sniff_magic,
                ext: &base.ext,
                mismatch: base.mismatch,
                text: String::new(),
                metadata: json!({"media": meta.to_metadata()}),
                page_anchors: Vec::new(),
                warnings: std::mem::take(warnings),
                runs: runs.iter().map(|r| r.run_id.clone()).collect(),
            })));
        };

        // 门 4：单帧像素帽 16 MP（streams[video].width×height 整数形态）。
        if let Some(pixels) = video.pixel_count() {
            if pixels > MEDIA_FRAME_PIXELS_MAX {
                return Ok(ChainOutcome::failed(
                    RouterReject::media_pixel_budget_exceeded(pixels, MEDIA_FRAME_PIXELS_MAX),
                ));
            }
        } else {
            warnings.push(
                "ffprobe video stream did not parse width/height: frame pixel cap left unresolved (honest reporting, zero fabrication)"
                    .to_string(),
            );
        }

        // 门 5：帧数预算（前缀采样语义非拒绝，D5）。
        let expected = meta
            .duration_s
            .map(|d| ((MEDIA_FRAME_FPS as f64 * d).ceil() as u64).min(MEDIA_FRAMES_MAX as u64));
        let capped = meta
            .duration_s
            .map(|d| d > MEDIA_FRAMES_MAX as f64 / MEDIA_FRAME_FPS as f64)
            .unwrap_or(false);
        if capped {
            warnings.push(format!(
                "frame budget capped at {MEDIA_FRAMES_MAX} frames (prefix sampling preview semantics, not a rejection; DI-2 ADR D3/D5)"
            ));
        }

        // 步 2：ffmpeg 受限抽帧（全常量 argv：fps=2 / -frames:v 32 / -c:v png
        // 显式编码器铁律 / out/ cwd 相对落本 run out/）。
        let frames = self.drive_step(
            ctx,
            TOOL_MEDIA_FRAMES,
            source_bytes,
            &base.staged,
            json!({}),
        )?;
        if let Some(reject) = terminal_reject(&frames) {
            runs.push(frames);
            return Ok(ChainOutcome::failed(reject));
        }
        let extracted = count_frame_files(&self.frame_out_dir(&frames.run_id));
        runs.push(frames);
        // 实收帧数 vs 预算对账（不一致 → warnings 如实）。
        if let Some(exp) = expected {
            if extracted != exp {
                warnings.push(format!(
                    "extracted {extracted} frame files disagree with the frame budget {exp}: count follows the actual out/ listing (honest reporting)"
                ));
            }
        }
        let mut metadata = json!({"media": meta.to_metadata()});
        if let Some(obj) = metadata.as_object_mut() {
            obj.insert(
                "frames".into(),
                json!({
                    "pattern": MEDIA_FRAME_PATTERN_NAME,
                    "extracted": extracted,
                    "fps": MEDIA_FRAME_FPS,
                    "capped": capped,
                }),
            );
        }
        Ok(ChainOutcome::succeeded(build_unified(UnifiedDocInput {
            channel: Channel::Media.as_str(),
            source_name: &base.staged,
            source_bytes: base.source_bytes,
            source_sha256: &base.source_sha256,
            sniff_magic: base.sniff_magic,
            ext: &base.ext,
            mismatch: base.mismatch,
            text: String::new(),
            metadata,
            page_anchors: Vec::new(),
            warnings: std::mem::take(warnings),
            runs: runs.iter().map(|r| r.run_id.clone()).collect(),
        })))
    }

    /// 抽帧 run 的 out/ 目录（cwd 相对 pattern 的绝对落位）。
    fn frame_out_dir(&self, run_id: &str) -> PathBuf {
        self.config.workspace_root.join(run_id).join("out")
    }

    /// 归档通道（DI-2 ADR D4）：7z 两阶段——`l -slt` 列目 → Router 四闸
    /// （路径安全 / 符号链接 / 条目数 512 / 解压总量 256 MiB + 单条目 64 MiB）
    /// → `x -y -oout` 解包本 run out/ → post-extract 符号链接终检。四闸任一
    /// 触发 = 链内 FAILED，解包阶段永不提交（list 行留证）。
    fn route_archive(
        &self,
        ctx: &DriveCtx<'_>,
        source_bytes: &[u8],
        base: &UnifiedBase,
        runs: &mut Vec<RunRecord>,
        warnings: &mut Vec<String>,
    ) -> Result<ChainOutcome, RouterReject> {
        // 阶段 1：列目（畸形归档 exit 2 → EXECUTION 透传，terminal_reject）。
        let list = self.drive_step(
            ctx,
            TOOL_ARCHIVE_LIST,
            source_bytes,
            &base.staged,
            json!({}),
        )?;
        if let Some(reject) = terminal_reject(&list) {
            runs.push(list);
            return Ok(ChainOutcome::failed(reject));
        }
        let raw = self.read_stdout(&list.run_id)?;
        let list_run_id = list.run_id.clone();
        runs.push(list);
        let meta = ArchiveListMeta::parse(&raw);

        // 列目成功但清单零条目 / 头块解析失败 → EXECUTION
        // ARCHIVE_LIST_NO_OUTPUT（D6 表同位语义）。
        let format = meta.format.clone();
        if format.is_none() || meta.entries.is_empty() {
            let detail = if format.is_none() {
                "listing did not parse a `Type = ` header (unparsable -slt output)".to_string()
            } else {
                "listing reported zero entries".to_string()
            };
            return Ok(ChainOutcome::failed(RouterReject::archive_list_no_output(
                &list_run_id,
                &detail,
            )));
        }

        // 闸 1 路径安全：`..` 段 / 以 `/` 起头 / 含 `\`（保守 fail-closed）。
        for entry in &meta.entries {
            let p = entry.path.as_str();
            let reason = if p.starts_with('/') {
                Some("absolute path entry")
            } else if p.contains('\\') {
                Some("contains a backslash separator (Windows semantics, fail-closed)")
            } else if p.split('/').any(|seg| seg == "..") {
                Some("contains a `..` segment")
            } else {
                None
            };
            if let Some(reason) = reason {
                return Ok(ChainOutcome::failed(RouterReject::archive_entry_invalid(
                    p, reason,
                )));
            }
        }
        // 闸 2 符号链接：`Symbolic Link = ` 字段非空或 Attributes `l` 前缀。
        for entry in &meta.entries {
            if entry.is_symlink() {
                let detail = entry
                    .symlink_target
                    .as_deref()
                    .map(|t| format!("Symbolic Link = {t}"))
                    .unwrap_or_else(|| {
                        format!(
                            "Attributes = {:?}",
                            entry.attributes.as_deref().unwrap_or("")
                        )
                    });
                return Ok(ChainOutcome::failed(RouterReject::archive_entry_symlink(
                    &entry.path,
                    &detail,
                )));
            }
        }
        // 闸 3 条目数帽 512（含目录条目）。
        if meta.entries.len() > ARCHIVE_MAX_ENTRIES {
            return Ok(ChainOutcome::failed(
                RouterReject::archive_entry_budget_exceeded(
                    meta.entries.len(),
                    ARCHIVE_MAX_ENTRIES,
                ),
            ));
        }
        // 闸 4 解压总量 256 MiB / 单条目 64 MiB（-slt Size 声明判定）。
        let total = meta.total_uncompressed_bytes();
        let worst = meta
            .entries
            .iter()
            .max_by_key(|e| e.size)
            .map(|e| (e.path.as_str(), e.size))
            .unwrap_or(("", 0));
        if total > ARCHIVE_MAX_TOTAL_BYTES || worst.1 > ARCHIVE_MAX_ENTRY_BYTES {
            return Ok(ChainOutcome::failed(RouterReject::archive_budget_exceeded(
                total,
                ARCHIVE_MAX_TOTAL_BYTES,
                worst,
                ARCHIVE_MAX_ENTRY_BYTES,
            )));
        }

        // 阶段 2：解包（-y 覆盖确认抑制；-oout cwd 相对 → 本 run out/，
        // cwd=ws.root 由行为级 e2e 钉死，D4）。
        let extract = self.drive_step(
            ctx,
            TOOL_ARCHIVE_EXTRACT,
            source_bytes,
            &base.staged,
            json!({}),
        )?;
        if let Some(reject) = terminal_reject(&extract) {
            runs.push(extract);
            return Ok(ChainOutcome::failed(reject));
        }
        let extract_run_id = extract.run_id.clone();
        runs.push(extract);
        let out_dir = self.config.workspace_root.join(&extract_run_id).join("out");

        // post-extract 符号链接终检（闸 2 之后的第二道防线兜底，D4）。
        // extract 行已在上方成功路径推入（真实终态 SUCCEEDED 留证）。
        let mut survived: Vec<String> = Vec::new();
        collect_surviving_symlinks(&out_dir, &mut survived);
        if !survived.is_empty() {
            return Ok(ChainOutcome::failed(
                RouterReject::archive_symlink_survived(&survived),
            ));
        }

        // 对账：out/ 实收文件 vs 列目清单（stdout.log/stderr.log 排除；
        // 不一致 → warnings 如实记录）。
        let actual = list_regular_files(&out_dir);
        let files: Vec<(String, u64)> = meta
            .entries
            .iter()
            .filter(|e| !e.folder)
            .map(|e| (e.path.clone(), e.size))
            .collect();
        if actual.len() != files.len() {
            warnings.push(format!(
                "extracted {} file(s) disagree(s) with the listing's {} file entries: reconciliation follows actual out/ contents (honest reporting)",
                actual.len(),
                files.len()
            ));
        }

        let metadata = json!({
            "archive": {
                "format": format,
                "entries": meta.entries.len(),
                "directories": meta.entries.iter().filter(|e| e.folder).count(),
                "total_uncompressed_bytes": total,
                "files": files
                    .iter()
                    .map(|(path, size)| json!({"path": path, "size": size}))
                    .collect::<Vec<_>>(),
            }
        });
        Ok(ChainOutcome::succeeded(build_unified(UnifiedDocInput {
            channel: Channel::Archive.as_str(),
            source_name: &base.staged,
            source_bytes: base.source_bytes,
            source_sha256: &base.source_sha256,
            sniff_magic: base.sniff_magic,
            ext: &base.ext,
            mismatch: base.mismatch,
            text: String::new(),
            metadata,
            page_anchors: Vec::new(),
            warnings: std::mem::take(warnings),
            runs: runs.iter().map(|r| r.run_id.clone()).collect(),
        })))
    }

    // -----------------------------------------------------------------
    // 子 run 驱动（D2/D5）
    // -----------------------------------------------------------------

    /// 单步子 run：生成 run_id → 预写 in/ → submit_correlated（trace 自锚定）
    /// → execute_native_rust_via_engine。终态信封由调用方检（链级失败语义）。
    fn drive_step(
        &self,
        ctx: &DriveCtx<'_>,
        tool_id: &'static str,
        bytes: &[u8],
        staged: &str,
        extra_input: Value,
    ) -> Result<RunRecord, RouterReject> {
        let run_id = new_run_id();
        self.drive_step_at(ctx, tool_id, &run_id, bytes, staged, extra_input)
    }

    /// 指定 run_id 的单步子 run（office 通道 profile_uri 需先定 run_id）。
    fn drive_step_at(
        &self,
        ctx: &DriveCtx<'_>,
        tool_id: &'static str,
        run_id: &str,
        bytes: &[u8],
        staged: &str,
        extra_input: Value,
    ) -> Result<RunRecord, RouterReject> {
        // D5 预写：`<workspace_root>/<run_id>/in/<staged>`（adapter
        // prepare_workspace create_dir_all 不清目录，PREPARING 后仍在位；
        // {{input_path}} 前缀校验对预写面照常生效，D8.3 边界不放松）。
        let in_dir = self.config.workspace_root.join(run_id).join("in");
        std::fs::create_dir_all(&in_dir)
            .and_then(|_| std::fs::write(in_dir.join(staged), bytes))
            .map_err(|e| {
                RouterReject::chain_failed(
                    "INFRA",
                    "NATIVE_TRANSPORT_FAILED",
                    format!("router prewrite {staged} failed for run {run_id}: {e}"),
                    true,
                )
            })?;
        let mut input = extra_input;
        if let Some(obj) = input.as_object_mut() {
            obj.insert("input_path".into(), json!(staged));
        }
        // submit_correlated：trace_id = run_id 自锚定（dispatch.rs:227-229
        // 先例；run 可经 REST /api/traces 反查，判据 10）。
        let request = json!({
            "protocol_version": "1",
            "run_id": run_id,
            "tool_id": tool_id,
            "input": input,
            "workspace_id": "ws-doc-router",
            "submit_key": format!("di1-router-{run_id}"),
        });
        ctx.engine
            .submit_correlated(&request, Some(run_id))
            .map_err(submit_reject)?;
        let manifest = self.manifests.get(tool_id);
        let outcome = execute_native_rust_via_engine(NativeRustCall {
            engine: ctx.engine,
            run_id,
            adapter: ctx.adapter,
            manifest,
            capability: ctx.capability,
        })
        .map_err(|e| {
            RouterReject::chain_failed(
                "INFRA",
                "NATIVE_TRANSPORT_FAILED",
                format!("router drive {tool_id} run {run_id} failed: {e}"),
                true,
            )
        })?;
        Ok(RunRecord {
            run_id: run_id.to_string(),
            tool_id: tool_id.to_string(),
            outcome,
        })
    }

    /// 子 run stdout 留档回读（file 模式 data 只含产物引用，文本按 D5 由
    /// Router 从工作区回读；out/stdout.log 为 adapter 既有留档面）。
    fn read_stdout(&self, run_id: &str) -> Result<String, RouterReject> {
        let path = self
            .config
            .workspace_root
            .join(run_id)
            .join("out")
            .join("stdout.log");
        let bytes = std::fs::read(&path).map_err(|e| {
            RouterReject::chain_failed(
                "INFRA",
                "NATIVE_TRANSPORT_FAILED",
                format!("router read-back {} failed: {e}", path.display()),
                true,
            )
        })?;
        Ok(String::from_utf8_lossy(&bytes).to_string())
    }
}

/// 子 run 信封终态失败 → D8 映射 reject（真实终态留在子 run 行，D8 表；
/// Succeeded 返回 None——信封 validate 保证非终态成功必带 error）。
fn terminal_reject(record: &RunRecord) -> Option<RouterReject> {
    let env = &record.outcome.envelope;
    if env.status == ToolRunStatus::Succeeded {
        return None;
    }
    let error = env
        .error
        .as_ref()
        .expect("non-succeeded envelope carries error (envelope::validate)");
    Some(RouterReject::chain_failed(
        error.category.as_str(),
        &code_to_string(&error.code),
        error.message.clone(),
        error.retryable,
    ))
}

/// 信封 error.code（整数或字符串）→ 透传字符串（D8 chain_failed 契约）。
fn code_to_string(code: &Value) -> String {
    match code {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

/// 引擎受理拒绝 → INFRA（登记面/输入面由调用方设置错误，属派发面配置错误）。
fn submit_reject(e: EngineError) -> RouterReject {
    RouterReject::chain_failed(
        "INFRA",
        "NATIVE_TRANSPORT_FAILED",
        format!("router submit rejected: {e}"),
        true,
    )
}

/// 通道编排产物（成功 = 统一输出；失败 = D8 reject；两者互斥）。
struct ChainOutcome {
    status: RouteStatus,
    value: Value,
    reject: Option<RouterReject>,
}

impl ChainOutcome {
    fn succeeded(value: Value) -> Self {
        Self {
            status: RouteStatus::Succeeded,
            value,
            reject: None,
        }
    }

    fn failed(reject: RouterReject) -> Self {
        Self {
            status: RouteStatus::Failed,
            value: Value::Null,
            reject: Some(reject),
        }
    }
}

/// 路由终局状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RouteStatus {
    /// 全链子 run SUCCEEDED，统一输出在 `unified`。
    Succeeded,
    /// 链级失败（D8 映射 reject 在 `reject`；子 run 记录保持真实终态）。
    Failed,
}

/// 路由结果（D3 统一输出 + 全部子 run 记录 + 警告）。
#[derive(Debug)]
pub struct RouteOutcome {
    /// 终局状态。
    pub status: RouteStatus,
    /// 统一输出（Succeeded 时在位；D3 Schema 形状）。
    pub unified: Option<Value>,
    /// 链级失败 reject（Failed 时在位；D8 映射）。
    pub reject: Option<RouterReject>,
    /// 警告（mismatch / office 转换面声明 / 页锚计数不一致，如实呈现）。
    pub warnings: Vec<String>,
    /// 本链全部子 run 记录（ToolRun 行锚；信封 = engine 既有面）。
    pub runs: Vec<RunRecord>,
}

/// 子 run 记录（ToolRun 行锚）。
#[derive(Debug)]
pub struct RunRecord {
    /// run id（rn- 形状；submit_correlated trace_id 自锚定）。
    pub run_id: String,
    /// 工具 Manifest id。
    pub tool_id: String,
    /// 引擎信封与状态轨迹（既有面，零复制）。
    pub outcome: DriveOutcome,
}

impl RunRecord {
    /// 信封投影（tool_engine::envelope::to_value；evidence/artifacts 全形状）。
    pub fn envelope_value(&self) -> Value {
        tool_engine::envelope::to_value(&self.outcome.envelope)
    }
}

// ---------------------------------------------------------------------------
// 内部工具
// ---------------------------------------------------------------------------

/// 抽帧产物清点（DI-2 D3：out/frame-NNN.png 计数以实收为准——`-frames:v 32`
/// 帽生效后实际帧数即断言对象）。
fn count_frame_files(out_dir: &std::path::Path) -> u64 {
    std::fs::read_dir(out_dir)
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .filter(|e| {
                    let name = e.file_name();
                    let name = name.to_string_lossy();
                    name.starts_with("frame-") && name.ends_with(".png")
                })
                .count() as u64
        })
        .unwrap_or(0)
}

/// post-extract 符号链接遍历（DI-2 D4 终检：`symlink_metadata` 不跟随链接，
/// 目录仅对真实目录递归——符号链接目录本身即上报目标，不进入遍历）。
fn collect_surviving_symlinks(dir: &std::path::Path, out: &mut Vec<String>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in rd.filter_map(|e| e.ok()) {
        let path = entry.path();
        let Ok(md) = std::fs::symlink_metadata(&path) else {
            continue;
        };
        if md.file_type().is_symlink() {
            out.push(path.display().to_string());
        } else if md.is_dir() {
            collect_surviving_symlinks(&path, out);
        }
    }
}

/// 解包产物实收清单（DI-2 D4 对账面：递归正则文件，stdout.log/stderr.log
/// 为 adapter 留档面排除；符号链接已被终检拦截，遍历只遇真实文件/目录）。
fn list_regular_files(dir: &std::path::Path) -> Vec<(String, u64)> {
    let mut out = Vec::new();
    walk_regular_files(dir, dir, &mut out);
    out.sort();
    out
}

fn walk_regular_files(root: &std::path::Path, dir: &std::path::Path, out: &mut Vec<(String, u64)>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in rd.filter_map(|e| e.ok()) {
        let path = entry.path();
        let Ok(md) = std::fs::symlink_metadata(&path) else {
            continue;
        };
        let name = match path.strip_prefix(root) {
            Ok(rel) => rel.to_string_lossy().to_string(),
            Err(_) => path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string(),
        };
        if name == "stdout.log" || name == "stderr.log" {
            continue;
        }
        if md.is_file() {
            out.push((name, md.len()));
        } else if md.is_dir() {
            walk_regular_files(root, &path, out);
        }
    }
}

/// staged 名派生的测试可见面（D5 规则断言用；生产路径走 route() 内部
/// 同名私有函数，规则单源）。
#[doc(hidden)]
pub fn staged_name_for_test(source_name: &str, ext: &str) -> String {
    staged_name(source_name, ext)
}

/// staged 名派生（D5：Router 控制的确定性名；字母数字与 .-_ 外字符折叠为
/// `_`；无扩展名时补 ext 小写；避免与 adapter 审计文件 request.json 冲突）。
fn staged_name(source_name: &str, ext: &str) -> String {
    let base = source_name
        .rsplit('/')
        .next()
        .unwrap_or(source_name)
        .rsplit('\\')
        .next()
        .unwrap_or(source_name);
    let mut keep: String = base
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_') {
                c
            } else {
                '_'
            }
        })
        .collect();
    if keep.is_empty() {
        keep.push_str("document");
    }
    if !keep.contains('.') && !ext.is_empty() {
        keep.push('.');
        keep.push_str(&ext.to_ascii_lowercase());
    }
    if keep == "request.json" {
        keep = format!("doc-{keep}");
    }
    keep.truncate(96);
    keep
}

/// run id 生成（rn-<16hex><8hex-seq>，dispatch.rs new_run_id 同形状；本 crate
/// 独立计数器——形状一致，非事实源）。
fn new_run_id() -> String {
    use std::hash::{BuildHasher, Hasher};
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQ: AtomicU64 = AtomicU64::new(0);
    static SALT: std::sync::OnceLock<(u64, u64)> = std::sync::OnceLock::new();
    let seq = SEQ.fetch_add(1, Ordering::Relaxed);
    let (salt_a, salt_b) = *SALT.get_or_init(|| {
        (
            std::collections::hash_map::RandomState::new()
                .build_hasher()
                .finish(),
            std::collections::hash_map::RandomState::new()
                .build_hasher()
                .finish(),
        )
    });
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
    hasher.write(
        format!(
            "{nanos}|{salt_a:x}{salt_b:x}|{seq}|{}|document-router",
            std::process::id()
        )
        .as_bytes(),
    );
    let digest = hasher.finish();
    format!("rn-{digest:016x}{seq:08x}")
}
