//! WSW-8 Browser Evidence 统一入口（docs/rip3/wsw/WSW-8-TASK.md D2/D3/D4）。
//!
//! 页面捕获 = Chromium 控制面走 Rust Core（PLAN :331 逐词；:640 chromiumoxide
//! 候选锁定）：本模块是唯一统一入口——Engine 装配（rp-verify.rs:173-190
//! 同型）、`submit_correlated`（execute_nuclei provider.rs:241 同型）、
//! NativeRustAdapter spawn `rp-wsw-browser` 子进程（tokio + chromiumoxide
//! 会话在子进程内）。
//!
//! run 归属（D3）：rip.wsw.browser Manifest（native_rust + binary_sha256 构建产物
//! 钉值先例）→ tool_run/artifact/evidence 三既有表（005/001），零新表零新体系
//! （:761）；artifact 走 collect_out_artifacts 既有 role 词形自动登记；evidence
//! 落 001 evidence 表 `evidence_type='screenshot'`（词表 M1 预埋 001:252）——
//! 本模块终局直接 INSERT（agent-engine 四处直接 INSERT 先例同列清单：evidence.rs:142
//! / memory/evidence.rs:425 / trace.rs:642 / memory/research.rs:489，同一张表非
//! 第二存储）。**不借 fuzz JobRunner**（wsw_fuzz_job 九态 + FuzzBudget 域词表
//! 与 browser 捕获域零交集——D3⑤，WSW-7 D3④ 判例）。
//!
//! Chromium 二进制来源（D2）：宿主已装探针链五级（零安装零下载零 sudo；
//! fetcher feature 禁用），本模块 [`probe_chromium`] 为唯一实现——rp-wsw-browser
//! 子进程与测试同链复用，零第二探针。解析结果经 input.browser_bin 注入子进程
//! （Adapter env_clear 后仅 PATH/TMPDIR/LANG 白名单在位，env 面探针只能在
//! 统一入口层做）。
//!
//! scope 消费（A13 只读）：`scope_rules` 传入时经 [`crate::scope::check`] 单跳闸
//! （scheme/host/元数据 deny-never-allow/私网），scope.rs 逐字节零 diff。
//!
//! 红线：本模块零 `Command::new("chromium")` 词形（Chromium 启动仅
//! rp-wsw-browser 子进程内 chromiumoxide 承载，判据 4 反证面）；零 Finding/五态
//! 判定词形（evidence 行 = 采集证据非结论，PLAN :330/:766）；migrations 零触碰
//! （D5 零迁移）。

use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use tool_engine::adapter::native_rust::{
    execute_native_rust_via_engine, NativeRustAdapter, NativeRustCall,
};
use tool_engine::Engine;

use crate::persist::now_iso;
use crate::scope::{self, ScopeRules, ScopeTarget};

/// 工具 id（manifests/tools/rip.wsw.browser.json 同源）。
pub const BROWSER_TOOL_ID: &str = "rip.wsw.browser";

/// workspace_id 常量（browser 捕获域运行行归属；provider.rs "ws-verify" 先例同型）。
pub const WS_BROWSER: &str = "ws-browser";

/// 截图四模式词表（PLAN :326 四词逐字）。
pub const CAPTURE_MODES: [&str; 4] = ["viewport", "full_page", "element", "region"];

/// 产物帽（D4⑥）：dom.json 16MiB / page.txt 1MiB / network.json 1MiB。
pub const DOM_CAP_BYTES: usize = 16 * 1024 * 1024;
pub const PAGE_TEXT_CAP_BYTES: usize = 1024 * 1024;
pub const NETWORK_CAP_BYTES: usize = 1024 * 1024;

/// settle_ms 域（0..=15000；默认 800）。
const SETTLE_MS_MAX: u64 = 15_000;
const SETTLE_MS_DEFAULT: u64 = 800;

/// capture 预算（秒）：默认 60，帽 120（Manifest resources.timeout_seconds 同面）。
const TIMEOUT_S_DEFAULT: u64 = 60;
const TIMEOUT_S_CAP: u64 = 120;

/// capture 结构化错误词值面（ScanError 同形状：跨信封出站用词值，判定面
/// parse 还原；domain 零依赖纪律）。
#[derive(Debug, Clone)]
pub struct BrowserError {
    /// 分类词值（chromium_missing/scope_blocked/validation/submit_failed/…）。
    pub category: String,
    pub message: String,
}

impl BrowserError {
    fn new(category: &str, message: impl Into<String>) -> Self {
        Self {
            category: category.to_owned(),
            message: message.into(),
        }
    }
}

// ---------------------------------------------------------------------------
// Chromium 探针链（D2①五级；唯一实现——子进程与测试同链复用，D6）
// ---------------------------------------------------------------------------

/// 宿主已装 Chromium 探针链五级（零下载零安装零 sudo）：一级
/// `RP_BROWSER_BIN` env、二级 `PW_CHROMIUM` env（WSW-2 e2e
/// rip3_wsw2_browser_e2e.mjs:16 词形先例）、三级 `~/.cache/ms-playwright/
/// chromium-*/chrome-linux*/chromium` glob、四级 `/usr/sbin/chromium`、
/// 五级 `/usr/bin/chromium`。任一级命中（存在且可执行）即返回路径；全缺
/// 则 None。
///
/// 本函数只做文件系统存在性检查，零进程拉起（判据 4 反证：browser.rs 源面
/// grep 零 `Command::new("chromium")`）。
pub fn probe_chromium() -> Option<PathBuf> {
    for key in ["RP_BROWSER_BIN", "PW_CHROMIUM"] {
        if let Ok(path) = std::env::var(key) {
            let p = PathBuf::from(path.trim());
            if !p.as_os_str().is_empty() && executable_in_place(&p) {
                return Some(p);
            }
        }
    }
    if let Some(home) = std::env::var_os("HOME") {
        let cache = Path::new(&home).join(".cache").join("ms-playwright");
        if let Ok(rd) = std::fs::read_dir(&cache) {
            let mut dirs: Vec<PathBuf> = rd
                .filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| {
                    p.file_name()
                        .and_then(|n| n.to_str())
                        .map(|n| n.starts_with("chromium-"))
                        .unwrap_or(false)
                })
                .collect();
            dirs.sort();
            for dir in dirs {
                // chromium-*/chrome-linux*/chromium（headless_shell 变体目录
                // 不入探针：完整 chromium 面）。
                if let Ok(inner) = std::fs::read_dir(&dir) {
                    let mut subdirs: Vec<PathBuf> = inner
                        .filter_map(|e| e.ok())
                        .map(|e| e.path())
                        .filter(|p| {
                            p.file_name()
                                .and_then(|n| n.to_str())
                                .map(|n| n.starts_with("chrome-linux"))
                                .unwrap_or(false)
                        })
                        .collect();
                    subdirs.sort();
                    for sub in subdirs {
                        let bin = sub.join("chromium");
                        if executable_in_place(&bin) {
                            return Some(bin);
                        }
                    }
                }
            }
        }
    }
    for fixed in ["/usr/sbin/chromium", "/usr/bin/chromium"] {
        let p = PathBuf::from(fixed);
        if executable_in_place(&p) {
            return Some(p);
        }
    }
    None
}

/// 存在且可执行（mode 位含 owner execute；探针级检查，非 sha 钉值面）。
fn executable_in_place(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    match std::fs::metadata(path) {
        Ok(meta) => meta.is_file() && meta.permissions().mode() & 0o111 != 0,
        Err(_) => false,
    }
}

// ---------------------------------------------------------------------------
// capture 请求词表（D4：url/mode 四态/selector/clip/settle_ms/scope_rules）
// ---------------------------------------------------------------------------

/// capture 参数（统一入口装配面；词表校验先行，非法即 validation 结构化拒绝）。
#[derive(Debug, Clone, PartialEq)]
pub struct BrowserCaptureParams {
    /// 目标 URL（scheme http/https only；file:// 词表拒绝——判据 7）。
    pub url: String,
    /// 截图四模式（词表 [`CAPTURE_MODES`]；默认 viewport）。
    pub mode: String,
    /// element 模式选择器（与 clip 互斥）。
    pub selector: Option<String>,
    /// region 模式矩形（与 selector 互斥；{x,y,width,height} 正数）。
    pub clip: Option<Value>,
    /// 导航完成后的稳定等待毫秒（0..=15000；默认 800）。
    pub settle_ms: u64,
    /// scope 规则（`wsw_session.scope_rules_json` 同形状；None = 不设闸只校验
    /// scheme 词表——loopback fixture 面）。
    pub scope_rules: Option<Value>,
    /// 预算秒（默认 60 帽 120）。
    pub timeout_s: u64,
}

impl BrowserCaptureParams {
    /// 从提交 input 构造 + 词表校验（非法返回 validation 结构化错误）。
    pub fn from_input(input: &Value) -> Result<Self, BrowserError> {
        let url = input
            .get("url")
            .and_then(Value::as_str)
            .ok_or_else(|| BrowserError::new("validation", "input.url 缺失（必填字符串）"))?
            .to_owned();
        if url.len() > 4096 {
            return Err(BrowserError::new("validation", "input.url 超长（>4096）"));
        }
        let mode = input
            .get("mode")
            .and_then(Value::as_str)
            .unwrap_or("viewport")
            .to_owned();
        if !CAPTURE_MODES.contains(&mode.as_str()) {
            return Err(BrowserError::new(
                "validation",
                format!("input.mode 词表外（{:?}）：{mode:?}", CAPTURE_MODES),
            ));
        }
        let selector = input
            .get("selector")
            .and_then(Value::as_str)
            .map(str::to_owned);
        let clip = input.get("clip").filter(|v| !v.is_null()).cloned();
        // selector/clip 互斥（element 与 region 双态不可同发）。
        if selector.is_some() && clip.is_some() {
            return Err(BrowserError::new(
                "validation",
                "input.selector 与 input.clip 互斥（element/region 双态同发拒绝）",
            ));
        }
        if mode == "element" && selector.is_none() {
            return Err(BrowserError::new(
                "validation",
                "mode=element 需要 input.selector",
            ));
        }
        if mode == "region" {
            let Some(clip) = &clip else {
                return Err(BrowserError::new(
                    "validation",
                    "mode=region 需要 input.clip（{x,y,width,height} 正数矩形）",
                ));
            };
            validate_clip(clip)?;
        }
        if mode != "element" && mode != "region" && (selector.is_some() || clip.is_some()) {
            return Err(BrowserError::new(
                "validation",
                "selector/clip 仅 element/region 模式可给",
            ));
        }
        let settle_ms = match input.get("settle_ms") {
            None | Some(Value::Null) => SETTLE_MS_DEFAULT,
            Some(v) => {
                let n = v.as_u64().ok_or_else(|| {
                    BrowserError::new("validation", "input.settle_ms 须为非负整数")
                })?;
                if n > SETTLE_MS_MAX {
                    return Err(BrowserError::new(
                        "validation",
                        format!("input.settle_ms 超帽（>{SETTLE_MS_MAX}）"),
                    ));
                }
                n
            }
        };
        let scope_rules = input.get("scope_rules").filter(|v| !v.is_null()).cloned();
        if let Some(rules) = &scope_rules {
            if !rules.is_object() {
                return Err(BrowserError::new(
                    "validation",
                    "input.scope_rules 须为对象（ScopeRules 形状）",
                ));
            }
        }
        let timeout_s = match input.get("timeout_s") {
            None | Some(Value::Null) => TIMEOUT_S_DEFAULT,
            Some(v) => {
                let n = v.as_u64().ok_or_else(|| {
                    BrowserError::new("validation", "input.timeout_s 须为非负整数")
                })?;
                if n == 0 || n > TIMEOUT_S_CAP {
                    return Err(BrowserError::new(
                        "validation",
                        format!("input.timeout_s 超域（1..={TIMEOUT_S_CAP}）"),
                    ));
                }
                n
            }
        };
        // scheme 词表（http/https only；file:// 等一律拒绝——判据 7）。
        // 先显式词表拒（file:// 等拒绝面点名 scheme），再结构拆分（host/port）。
        let declared_scheme = url.split("://").next().unwrap_or("").to_ascii_lowercase();
        if !url.contains("://") || (declared_scheme != "http" && declared_scheme != "https") {
            return Err(BrowserError::new(
                "validation",
                format!("input.url scheme 词表外（{declared_scheme}:）；仅 http/https"),
            ));
        }
        let (scheme, _, _, _) = split_url(&url)
            .ok_or_else(|| BrowserError::new("validation", format!("input.url 不可解析：{url}")))?;
        if scheme != "http" && scheme != "https" {
            return Err(BrowserError::new(
                "validation",
                format!("input.url scheme 词表外（{scheme}:）；仅 http/https"),
            ));
        }
        Ok(Self {
            url,
            mode,
            selector,
            clip,
            settle_ms,
            scope_rules,
            timeout_s,
        })
    }
}

/// clip 矩形校验：{x,y,width,height} 全正数（width/height > 0；x/y >= 0）。
fn validate_clip(clip: &Value) -> Result<(), BrowserError> {
    let obj = clip
        .as_object()
        .ok_or_else(|| BrowserError::new("validation", "input.clip 须为对象 {x,y,width,height}"))?;
    for key in ["x", "y", "width", "height"] {
        let v = obj.get(key).and_then(Value::as_f64).ok_or_else(|| {
            BrowserError::new("validation", format!("input.clip.{key} 缺失或非数值"))
        })?;
        let min_ok = if key == "width" || key == "height" {
            v > 0.0
        } else {
            v >= 0.0
        };
        if !min_ok || !v.is_finite() {
            return Err(BrowserError::new(
                "validation",
                format!("input.clip.{key}={v} 越界（width/height>0，x/y>=0）"),
            ));
        }
    }
    Ok(())
}

/// URL 拆分（scheme, host, port, path）：仅 http/https 词面；零 url crate 依赖
/// （wsw-runtime 无 url 声明——scope.rs 同款手拆面）。
fn split_url(url: &str) -> Option<(&str, &str, u16, String)> {
    let (scheme, rest) = url.split_once("://")?;
    if scheme != "http" && scheme != "https" {
        return None;
    }
    let (authority, path) = match rest.find('/') {
        Some(i) => (&rest[..i], rest[i..].to_string()),
        None => (rest, "/".to_string()),
    };
    let default_port = if scheme == "https" { 443 } else { 80 };
    let (host, port) = match authority.rsplit_once(':') {
        Some((h, p)) if p.chars().all(|c| c.is_ascii_digit()) && !p.is_empty() => {
            (h, p.parse().ok()?)
        }
        _ => (authority, default_port),
    };
    if host.is_empty() {
        return None;
    }
    Some((scheme, host, port, path))
}

/// scope 单跳闸（统一入口与子进程同链复用，零第二实现）：`scope_rules` 为
/// `ScopeRules` JSON 形状，url 为目标。Blocked → scope_blocked 结构化错误。
pub fn scope_gate(scope_rules: &Value, url: &str) -> Result<(), BrowserError> {
    let rules_text = serde_json::to_string(scope_rules)
        .map_err(|e| BrowserError::new("validation", format!("scope_rules 序列化失败: {e}")))?;
    let rules = ScopeRules::from_json(&rules_text)
        .map_err(|e| BrowserError::new("validation", format!("scope_rules 解析失败: {e}")))?;
    let (scheme, host, port, path) = split_url(url)
        .ok_or_else(|| BrowserError::new("validation", format!("url 不可解析：{url}")))?;
    let target = ScopeTarget {
        scheme,
        host,
        port,
        path: &path,
    };
    if let scope::ScopeDecision::Blocked { reason } = scope::check(&rules, &target) {
        return Err(BrowserError::new(
            "scope_blocked",
            format!("{}: {}", reason.code(), reason_summary(&reason)),
        ));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Engine 装配（rp-verify.rs:173-190 同型；scanner.rs scan_engine 同款 query-first）
// ---------------------------------------------------------------------------

/// repo root 锚（rp-verify.rs repo_root / rp-wsw scan_repo_root 同款）。
pub fn repo_root() -> PathBuf {
    if let Ok(root) = std::env::var("RP_REPO_ROOT") {
        return PathBuf::from(root);
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// browser Manifest 直读（防漂移：登记面单源 manifests/tools/rip.wsw.browser.json）。
pub fn browser_manifest(repo_root: &Path) -> Result<Value, String> {
    let rel = "manifests/tools/rip.wsw.browser.json";
    let text = std::fs::read_to_string(repo_root.join(rel))
        .map_err(|e| format!("manifest read {rel}: {e}"))?;
    serde_json::from_str(&text).map_err(|e| format!("manifest parse {rel}: {e}"))
}

/// Engine 装配（文件库落库面：Pool→migrate→Registry::open→register→Engine::open；
/// registry query-first 幂等复用既有登记——rule 2 幂等面）。
///
/// `manifest_override` = None 时读 repo manifest（生产面）；Some(m) 供测试面
/// 注入自持 manifest（CARGO_BIN_EXE debug 二进制 + 运行时 sha256）。
pub fn open_engine_with_manifest(
    db: &Path,
    repo_root: &Path,
    manifest_override: Option<&Value>,
) -> Result<Engine, String> {
    let pool = database::Pool::open(database::PoolConfig {
        path: database::DbPath::File(db.to_path_buf()),
        readonly: false,
        max_connections: 1,
        busy_timeout: std::time::Duration::from_secs(5),
    })
    .map_err(|e| format!("pool open: {e}"))?;
    database::migrate(&pool, &repo_root.join("migrations")).map_err(|e| format!("migrate: {e}"))?;
    let registry = tool_registry::Registry::open(pool.clone());
    let manifest: Value = match manifest_override {
        Some(m) => m.clone(),
        None => browser_manifest(repo_root)?,
    };
    let tool_id = manifest
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or(BROWSER_TOOL_ID)
        .to_owned();
    if registry.query(&tool_id).is_err() {
        registry
            .register(&manifest)
            .map_err(|e| format!("register {tool_id}: {e}"))?;
    }
    Ok(tool_engine::Engine::open(pool, registry))
}

/// scan 运行库同款生产装配（repo manifest 直读）。
pub fn open_browser_engine(db: &Path, repo_root: &Path) -> Result<Engine, String> {
    open_engine_with_manifest(db, repo_root, None)
}

// ---------------------------------------------------------------------------
// 统一入口（submit_correlated → NativeRustAdapter → 终局 evidence 'screenshot'）
// ---------------------------------------------------------------------------

/// capture 终局投影（run/tool/artifact/evidence 四面回执）。
#[derive(Debug, Clone)]
pub struct CaptureOutcome {
    pub run_id: String,
    /// 信封状态词（succeeded/failed/…；ToolRunStatus 词值面）。
    pub status_word: String,
    /// evidence 'screenshot' 行 id（仅 Succeeded 且 screenshot artifact 在场）。
    pub evidence_id: Option<String>,
    /// artifact 行投影（collect_out_artifacts 登记面回读）。
    pub artifacts: Vec<Value>,
    /// 终态信封（完整面）。
    pub envelope: Value,
}

/// 真实页面捕获统一入口（probe → scope → submit_correlated → 子进程派发 →
/// 终局 evidence INSERT）。run_id 由调用面给（rp-wsw CLI 时间微秒唯一面）。
pub fn capture(
    engine: &Engine,
    adapter: &NativeRustAdapter,
    manifest: &Value,
    params: &BrowserCaptureParams,
    run_id: &str,
    task_id: Option<&str>,
    trace_id: Option<&str>,
) -> Result<CaptureOutcome, BrowserError> {
    // 1) scope 消费（A13 只读；scope_rules 传入时单跳闸同序——scheme/host/
    //    元数据 deny-never-allow/私网；check(:296) 词形，scope_gate 子进程同链）。
    if let Some(rules) = &params.scope_rules {
        scope_gate(rules, &params.url)?;
    }
    // 2) Chromium 探针链（五级；全缺 = chromium_missing——D6/V-9 词形）。
    let browser_bin = probe_chromium().ok_or_else(|| {
        BrowserError::new(
            "chromium_missing",
            "Chromium 探针链五级全缺（RP_BROWSER_BIN/PW_CHROMIUM/ms-playwright 缓存//usr/sbin/chromium//usr/bin/chromium）；V-9 分面 SKIP 词形",
        )
    })?;
    // 3) 提交 input（browser_bin 经统一入口解析注入——Adapter env_clear 白名单
    //    结构性无 env 探针面；可选键仅在场时入 input——input_schema
    //    additionalProperties=false + 非空类型约束面）。
    let mut input = json!({
        "url": params.url,
        "mode": params.mode,
        "settle_ms": params.settle_ms,
        "browser_bin": browser_bin.display().to_string(),
    });
    if let Some(selector) = &params.selector {
        input["selector"] = json!(selector);
    }
    if let Some(clip) = &params.clip {
        input["clip"] = clip.clone();
    }
    if let Some(rules) = &params.scope_rules {
        input["scope_rules"] = rules.clone();
    }
    if let Some(trace) = trace_id {
        input["trace_id"] = json!(trace);
    }
    let mut request = json!({
        "protocol_version": "1",
        "tool_id": BROWSER_TOOL_ID,
        "run_id": run_id,
        "input": input,
        "workspace_id": WS_BROWSER,
        "budget": {
            "timeout_seconds": params.timeout_s,
            "max_output_bytes": 8388608
        }
    });
    if let Some(task) = task_id {
        request["task_id"] = json!(task);
    }
    // 4) 受理（关联提交；trace_id 落 tool_run.trace_id——015 面六 ID 锚）。
    let submitted = engine
        .submit_correlated(&request, trace_id)
        .map_err(|e| BrowserError::new("submit_failed", format!("submit rejected: {e}")))?;
    // 5) native_rust 派发（QUEUED→PREPARING→RUNNING→终局；collect_out_artifacts
    //    自动登记 artifact 行——run_id NOT NULL FK 语义满足）。
    let call = NativeRustCall {
        engine,
        run_id: &submitted.run_id,
        adapter,
        manifest,
        capability: None,
    };
    let outcome = execute_native_rust_via_engine(call)
        .map_err(|e| BrowserError::new("dispatch_failed", format!("native drive failed: {e}")))?;
    // 信封投影（ToolRunResult 无 Serialize derive——字段面手工投影，零派发语义）。
    let envelope_val = json!({
        "protocol_version": outcome.envelope.protocol_version,
        "run_id": outcome.envelope.run_id,
        "tool_id": outcome.envelope.tool_id,
        "tool_version": outcome.envelope.tool_version,
        "status": outcome.envelope.status.as_str(),
        "result": outcome.envelope.result,
        "summary": outcome.envelope.summary,
        "error": outcome.envelope.error.map(|e| json!({
            "category": e.category,
            "code": e.code,
            "message": e.message,
            "retryable": e.retryable,
        })),
        "artifacts": outcome.envelope.artifacts.iter().map(|a| json!({
            "artifact_id": a.artifact_id,
            "role": a.role,
            "uri": a.uri,
            "sha256": a.sha256,
            "size_bytes": a.size_bytes,
            "mime_type": a.mime_type,
            "provenance": a.provenance,
        })).collect::<Vec<_>>(),
        "evidence_refs": outcome.envelope.evidence_refs,
    });
    let status_word = outcome.envelope.status.as_str().to_owned();
    // 6) artifact 行回读（Engine::pool 直查 artifact 表——单源读面）。
    let artifacts = artifact_rows(engine, &submitted.run_id)?;
    // 7) 终局 evidence 'screenshot' 直 INSERT（D3③：仅 Succeeded 且 screenshot
    //    artifact 在场；storage_ref=artifact uri、content_hash=artifact sha256
    //    同哈希可复核、source_pk=run_id、task_ref=task_id、trace_id 同链）。
    let evidence_id = if status_word == "SUCCEEDED" {
        let screenshot = artifacts.iter().find(|a| {
            a.get("uri")
                .and_then(Value::as_str)
                .map(|u| u.ends_with("/out/screenshot.png"))
                .unwrap_or(false)
        });
        match screenshot {
            Some(art) => {
                let id =
                    insert_screenshot_evidence(engine, &submitted.run_id, art, task_id, trace_id)?;
                Some(id)
            }
            None => None,
        }
    } else {
        None
    };
    Ok(CaptureOutcome {
        run_id: submitted.run_id,
        status_word,
        evidence_id,
        artifacts,
        envelope: envelope_val,
    })
}

/// ScopeRejection 摘要（machine code 已在词值面；本函数给人类可读尾缀）。
fn reason_summary(reason: &scope::ScopeRejection) -> String {
    match reason {
        scope::ScopeRejection::NoRules => "include 空 = default deny".to_owned(),
        scope::ScopeRejection::InvalidHost(h) => format!("host 非法：{h}"),
        scope::ScopeRejection::MetadataEndpoint(h) => format!("云元数据 deny-never-allow：{h}"),
        scope::ScopeRejection::HostExcluded(h) => format!("exclude 命中：{h}"),
        scope::ScopeRejection::HostNotIncluded(h) => format!("include 未命中：{h}"),
        scope::ScopeRejection::PortDenied(p) => format!("端口不在 allowed_ports：{p}"),
        scope::ScopeRejection::PathDenied(p) => format!("路径不在 allowed_path_prefixes：{p}"),
        scope::ScopeRejection::PrivateTarget { ip, reason } => {
            format!("私网目标：{ip}（{reason}）")
        }
        scope::ScopeRejection::RedirectLimit { hop, cap } => format!("重定向超帽：{hop}>{cap}"),
        scope::ScopeRejection::SchemeDenied(s) => format!("scheme 拒：{s}"),
    }
}

/// artifact 行投影回读（run_id 全行；Engine::pool 单源读面）。
fn artifact_rows(engine: &Engine, run_id: &str) -> Result<Vec<Value>, BrowserError> {
    engine
        .pool()
        .with_conn(|c| {
            let mut stmt = c.prepare(
                "SELECT artifact_id, role, uri, sha256, size_bytes, mime_type, trace_id
                 FROM artifact WHERE run_id = ?1 ORDER BY artifact_id",
            )?;
            let rows = stmt.query_map([run_id], |r| {
                Ok(json!({
                    "artifact_id": r.get::<_, String>(0)?,
                    "role": r.get::<_, String>(1)?,
                    "uri": r.get::<_, String>(2)?,
                    "sha256": r.get::<_, String>(3)?,
                    "size_bytes": r.get::<_, i64>(4)?,
                    "mime_type": r.get::<_, String>(5)?,
                    "trace_id": r.get::<_, Option<String>>(6)?,
                }))
            })?;
            let mut out = Vec::new();
            for row in rows {
                out.push(row?);
            }
            Ok::<Vec<Value>, database::rusqlite::Error>(out)
        })
        .map_err(|e| BrowserError::new("dispatch_failed", format!("artifact 回读失败: {e}")))
}

/// evidence 'screenshot' 行直 INSERT（001 表唯一 evidence 事实源；A10 四处
/// 直接 INSERT 先例同列清单：evidence_type='screenshot' 词表 001:252 M1 预埋，
/// lifecycle='RAW' + governance 默认 ACTIVE；PLAN :330 截图仅视觉状态——
/// 零 Finding 零五态判定词形）。
fn insert_screenshot_evidence(
    engine: &Engine,
    run_id: &str,
    artifact: &Value,
    task_id: Option<&str>,
    trace_id: Option<&str>,
) -> Result<String, BrowserError> {
    let evidence_id = format!("ev-{run_id}");
    let now = now_iso();
    let uri = artifact
        .get("uri")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    let sha = artifact
        .get("sha256")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    engine
        .pool()
        .with_conn(|c| {
            c.execute(
                "INSERT INTO evidence (evidence_id, evidence_type, storage_ref, content_hash,
                 captured_at, captured_by, task_ref, lifecycle, governance, source_system,
                 source_pk, trace_id, created_at, updated_at, schema_version)
                 VALUES (?1, 'screenshot', ?2, ?3, ?4, ?5, ?6, 'RAW', 'ACTIVE', 'wsw-runtime',
                 ?7, ?8, ?4, ?4, 1)",
                database::rusqlite::params![
                    evidence_id,
                    uri,
                    sha,
                    now,
                    "rp-wsw-browser",
                    task_id,
                    run_id,
                    trace_id
                ],
            )?;
            Ok::<(), database::rusqlite::Error>(())
        })
        .map_err(|e| BrowserError::new("dispatch_failed", format!("evidence INSERT 失败: {e}")))?;
    Ok(evidence_id)
}

// ---------------------------------------------------------------------------
// 单测（探针链矩阵 + 词表校验 + 截断帽；零浏览器拉起——真捕获面在
// tests/browser_adapter.rs，S7）
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn probe_chain_env_level_hits() {
        // 一级探针：RP_BROWSER_BIN 命中在位路径（/bin/true 可执行在位）。
        std::env::set_var("RP_BROWSER_BIN", "/bin/true");
        let hit = probe_chromium().expect("env 一级命中");
        assert_eq!(hit, PathBuf::from("/bin/true"));
        // RP_BROWSER_BIN 指向不存在路径 → 跳过该级继续（负例模拟面）。
        std::env::set_var("RP_BROWSER_BIN", "/nonexistent/chromium-missing");
        let hit2 = probe_chromium();
        // 本机 /usr/sbin/chromium 在位（A4 双命中面）→ 应落到固定路径级。
        assert!(
            hit2.is_some(),
            "env 缺位时探针链应继续（本机固定路径级在位）"
        );
        std::env::remove_var("RP_BROWSER_BIN");
    }

    #[test]
    fn probe_chain_empty_env_is_skipped() {
        // 空串 env = 该级缺位（trim 后空路径不返回）。
        std::env::set_var("RP_BROWSER_BIN", "   ");
        let hit = probe_chromium();
        std::env::remove_var("RP_BROWSER_BIN");
        assert!(
            hit.as_deref() != Some(Path::new("   ")),
            "空 env 不得作为命中返回"
        );
    }

    #[test]
    fn probe_chain_fixed_path_level() {
        // 固定路径级：本机 /usr/sbin/chromium 在位（A4 实测 Chromium 158）。
        let hit = probe_chromium();
        assert!(hit.is_some(), "本机探针链至少一级命中（S1 门）");
        let p = hit.unwrap();
        assert!(
            p.starts_with("/usr/sbin/chromium")
                || p.starts_with("/usr/bin/chromium")
                || p.to_string_lossy().contains("ms-playwright")
                || p.is_absolute(),
            "命中路径形态：{p:?}"
        );
    }

    #[test]
    fn capture_modes_word_table() {
        assert_eq!(
            CAPTURE_MODES,
            ["viewport", "full_page", "element", "region"]
        );
    }

    #[test]
    fn params_validation_rejects_bad_mode() {
        let err = BrowserCaptureParams::from_input(&json!({
            "url": "http://127.0.0.1:1/x", "mode": "panorama"
        }))
        .unwrap_err();
        assert_eq!(err.category, "validation");
        assert!(err.message.contains("词表外"), "{}", err.message);
    }

    #[test]
    fn params_validation_rejects_file_scheme() {
        let err = BrowserCaptureParams::from_input(&json!({
            "url": "file:///etc/passwd", "mode": "viewport"
        }))
        .unwrap_err();
        assert_eq!(err.category, "validation");
        assert!(err.message.contains("scheme"), "{}", err.message);
    }

    #[test]
    fn params_validation_rejects_metadata_host_only_via_scope() {
        // 词表层不判元数据 host（那是 scope 闸职责）；词表只管 scheme/词表。
        let ok = BrowserCaptureParams::from_input(&json!({
            "url": "http://169.254.169.254/latest/meta-data/", "mode": "viewport"
        }))
        .expect("词表放行（scope 闸承载）");
        assert_eq!(ok.mode, "viewport");
    }

    #[test]
    fn params_validation_element_requires_selector() {
        let err = BrowserCaptureParams::from_input(&json!({
            "url": "http://127.0.0.1:1/x", "mode": "element"
        }))
        .unwrap_err();
        assert!(err.message.contains("selector"), "{}", err.message);
    }

    #[test]
    fn params_validation_region_requires_clip() {
        let err = BrowserCaptureParams::from_input(&json!({
            "url": "http://127.0.0.1:1/x", "mode": "region"
        }))
        .unwrap_err();
        assert!(err.message.contains("clip"), "{}", err.message);
    }

    #[test]
    fn params_validation_selector_clip_mutex() {
        let err = BrowserCaptureParams::from_input(&json!({
            "url": "http://127.0.0.1:1/x", "mode": "element",
            "selector": "#a", "clip": {"x": 0, "y": 0, "width": 10, "height": 10}
        }))
        .unwrap_err();
        assert!(err.message.contains("互斥"), "{}", err.message);
    }

    #[test]
    fn params_validation_clip_bounds() {
        let err = BrowserCaptureParams::from_input(&json!({
            "url": "http://127.0.0.1:1/x", "mode": "region",
            "clip": {"x": 0, "y": 0, "width": -1, "height": 10}
        }))
        .unwrap_err();
        assert!(err.message.contains("width"), "{}", err.message);
    }

    #[test]
    fn params_validation_settle_and_timeout_caps() {
        let err = BrowserCaptureParams::from_input(&json!({
            "url": "http://127.0.0.1:1/x", "settle_ms": 99999
        }))
        .unwrap_err();
        assert!(err.message.contains("settle_ms"), "{}", err.message);
        let err2 = BrowserCaptureParams::from_input(&json!({
            "url": "http://127.0.0.1:1/x", "timeout_s": 600
        }))
        .unwrap_err();
        assert!(err2.message.contains("timeout_s"), "{}", err2.message);
        let ok = BrowserCaptureParams::from_input(&json!({
            "url": "http://127.0.0.1:1/x"
        }))
        .unwrap();
        assert_eq!(ok.settle_ms, SETTLE_MS_DEFAULT);
        assert_eq!(ok.timeout_s, TIMEOUT_S_DEFAULT);
    }

    #[test]
    fn split_url_shapes() {
        let (scheme, host, port, path) = split_url("http://127.0.0.1:8099/a?b=c").unwrap();
        assert_eq!(
            (scheme, host, port, path.as_str()),
            ("http", "127.0.0.1", 8099, "/a?b=c")
        );
        let (_, host2, port2, path2) = split_url("https://example.com").unwrap();
        assert_eq!((host2, port2, path2.as_str()), ("example.com", 443, "/"));
        assert!(split_url("gopher://x/1").is_none());
        assert!(split_url("http://").is_none());
    }

    #[test]
    fn caps_are_worded() {
        // D4⑥ 帽词形：dom 16MiB / page.txt 1MiB / network 1MiB。
        assert_eq!(DOM_CAP_BYTES, 16 * 1024 * 1024);
        assert_eq!(PAGE_TEXT_CAP_BYTES, 1024 * 1024);
        assert_eq!(NETWORK_CAP_BYTES, 1024 * 1024);
    }
}
