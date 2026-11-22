//! rp-wsw-browser：`rip.wsw.browser`（WSW-8 Browser Evidence，native_rust
//! runtime）的可运行入口——native_rust 派发目标子进程（tokio rt +
//! chromiumoxide CDP 会话在子进程内承载）。
//!
//! 派发契约（tool-engine native_rust Adapter 既有面，零新协议）：
//! - argv：`rp-wsw-browser capture <out_dir>`（`capture` = Manifest
//!   runtime.subcommand 固定子命令；`{{out_dir}}` = Adapter 保留槽位）；
//! - stdin：一行 JSON 请求 `{"input": {...}, "budget": {...}}`（D8.1：run_id
//!   不透传工具）；
//! - stdout：一行 JSON 结果帧 `{"status":"result","data":{...}}`（D-TC-03）；
//! - 失败：stderr 一行结构化 JSON `{"error_code":..., "message":...}` +
//!   非零退出（Adapter EXECUTION FAILED，reason 进 tool_run summary；超时
//!   TIMED_OUT 由 Adapter 看门狗 killpg 承载——killpg 覆盖本进程与 Chromium
//!   孙进程同组）。
//!
//! 产物五文件（out/，collect_out_artifacts 自动登记 artifact 行，D3②）：
//! `screenshot.png` / `dom.json`（DOMSnapshot.captureSnapshot 原始 JSON——
//! strings/documents 结构化面，D4①非 MHTML）/ `page.txt`（document.body.innerText
//! 有界投影）/ `network.json`（Network 域事件摘要行）/ `meta.json`（PLAN :327
//! 九字段 + capture_fidelity 截断登记）。
//!
//! Chromium 来源（D2）：input.browser_bin = 统一入口 probe_chromium() 探针链
//! 解析注入；缺位时本进程探针链兜底（同链复用 [`wsw_runtime::browser::
//! probe_chromium`]，零第二探针）。headless 模式 + 一次性 user-data-dir（run
//! 终局清理）+ no_sandbox（容器/无显示环境必需）。
//!
//! 红线：零 Shell 构造；零迁移零新表；截图仅视觉状态（evidence 行 lifecycle
//! RAW——判定面零触碰）。

use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use chromiumoxide::cdp::browser_protocol::dom_snapshot::CaptureSnapshotParams;
use chromiumoxide::cdp::browser_protocol::network::{
    EventRequestWillBeSent, EventResponseReceived,
};
use chromiumoxide::cdp::browser_protocol::page::CaptureScreenshotFormat;
use chromiumoxide::page::ScreenshotParams;
use chromiumoxide::{Browser, BrowserConfig};
use futures::StreamExt;
use serde_json::{json, Value};
use wsw_runtime::browser::{
    scope_gate, BrowserCaptureParams, DOM_CAP_BYTES, NETWORK_CAP_BYTES, PAGE_TEXT_CAP_BYTES,
};

/// usage 文本（rp-knowledge-fetch P9-3 统一口径同款）。
fn usage_text() -> &'static str {
    "usage: rp-wsw-browser capture <out_dir>\n\n\
     native_rust 子进程：stdin 一行 JSON 请求（input+budget），stdout 一行 \
     D-TC-03 结果帧；显式 --help/-h = 本文本 stdout exit 0"
}

fn usage() -> i32 {
    eprintln!("{}", usage_text());
    2
}

/// 结构化失败面：stderr 一行 JSON + 退出码（Adapter EXECUTION FAILED 承载）。
fn fail(code: &str, message: impl std::fmt::Display) -> i32 {
    eprintln!(
        "{}",
        json!({"error_code": code, "message": message.to_string()})
    );
    1
}

/// 有界写盘：超帽字节级截断（原始语义不静默裁掉——capture_fidelity 登记，
/// DESIGN §3.3 层 1 口径）。
fn write_capped(path: &Path, bytes: &[u8], cap: usize) -> std::io::Result<(usize, usize, bool)> {
    let truncated = bytes.len() > cap;
    let take = if truncated { cap } else { bytes.len() };
    std::fs::write(path, &bytes[..take])?;
    Ok((take, bytes.len(), truncated))
}

/// 一次性 user-data-dir（D2③ cookie/storage/进程三面隔离；run 终局清理）。
///
/// 目录锚 = 系统 /tmp（非 std::env::temp_dir()——Adapter env_clear 白名单
/// TMPDIR = 任务深层 tmp，Chromium process_singleton SingletonSocket 取
/// TMPDIR/XDG_RUNTIME_DIR 面，超 Unix socket 108 字符帽即 FATAL Socket
/// path too long，2026-11-23 e2e 实录）。/tmp 不可用时回退 std::env::temp_dir()。
fn fresh_user_data_dir() -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let base = if Path::new("/tmp").is_dir() {
        PathBuf::from("/tmp")
    } else {
        std::env::temp_dir()
    };
    let dir = base.join(format!(
        "rp-wsw-browser-udir-{}-{nanos}",
        std::process::id()
    ));
    let _ = std::fs::create_dir_all(&dir);
    dir
}

fn main() {
    std::process::exit(run_cli());
}

fn run_cli() -> i32 {
    let raw: Vec<String> = std::env::args().skip(1).collect();
    if raw.iter().any(|a| a == "--help" || a == "-h") {
        println!("{}", usage_text());
        return 0;
    }
    let (sub, out_dir) = match raw.as_slice() {
        [s, o] => (s.as_str(), PathBuf::from(o)),
        _ => return usage(),
    };
    if sub != "capture" {
        return usage();
    }
    // stdin 一行 JSON 请求（D8.1）。
    let mut line = String::new();
    if std::io::stdin().read_to_string(&mut line).is_err() || line.trim().is_empty() {
        return fail("usage", "stdin 缺一行 JSON 请求（input+budget）");
    }
    let request: Value = match serde_json::from_str(line.trim()) {
        Ok(v) => v,
        Err(e) => return fail("usage", format!("stdin JSON 解析失败: {e}")),
    };
    let input = request.get("input").cloned().unwrap_or(Value::Null);
    let params = match BrowserCaptureParams::from_input(&input) {
        Ok(p) => p,
        Err(e) => return fail("validation", format!("{}: {}", e.category, e.message)),
    };
    match run_capture(&params, &input, &out_dir) {
        Ok(data) => {
            // D-TC-03：stdout 一行结果帧（首行；adapter 帧模式解析面）。
            println!("{}", json!({"status": "result", "data": data}));
            0
        }
        Err((code, message)) => fail(&code, message),
    }
}

type CaptureFailure = (String, String);

fn run_capture(
    params: &BrowserCaptureParams,
    input: &Value,
    out_dir: &Path,
) -> Result<Value, CaptureFailure> {
    let started = Instant::now();
    // scope 闸子进程同链复检（统一入口已过闸；直调面纵深兜底）。
    if let Some(rules) = &params.scope_rules {
        scope_gate(rules, &params.url).map_err(|e| (e.category, e.message))?;
    }
    // Chromium 二进制：input.browser_bin（统一入口探针链注入）→ 本进程探针链兜底。
    let browser_bin = input
        .get("browser_bin")
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty())
        .map(PathBuf::from)
        .or_else(wsw_runtime::browser::probe_chromium)
        .ok_or_else(|| {
            (
                "browser_missing".to_owned(),
                "Chromium 探针链五级全缺（browser_bin 注入缺位 + 本进程探针兜底未命中）".to_owned(),
            )
        })?;
    let trace_id = input.get("trace_id").and_then(Value::as_str);

    let budget_timeout_s = input
        .get("timeout_s")
        .and_then(Value::as_u64)
        .unwrap_or(params.timeout_s);
    let settle = std::time::Duration::from_millis(params.settle_ms);

    tokio_runtime_capture(
        &browser_bin,
        params,
        trace_id,
        settle,
        budget_timeout_s,
        out_dir,
        started,
    )
}

/// chromiumoxide 会话（tokio rt 内；Browser 生命周期 = 本子进程一次 run）。
#[tokio::main(flavor = "multi_thread")]
async fn tokio_runtime_capture(
    browser_bin: &Path,
    params: &BrowserCaptureParams,
    trace_id: Option<&str>,
    settle: std::time::Duration,
    budget_timeout_s: u64,
    out_dir: &Path,
    started: Instant,
) -> Result<Value, CaptureFailure> {
    // 内部自清理死线 = 预算 + 30s 余量（TIMED_OUT 语义由 Adapter 看门狗在
    // 预算点 killpg 承载——本死线只在看门狗失效时兜底清理浏览器进程）。
    let deadline = std::time::Duration::from_secs(budget_timeout_s.saturating_add(30));
    let inner = tokio::time::timeout(
        deadline,
        capture_once(browser_bin, params, trace_id, settle, out_dir),
    )
    .await;
    match inner {
        Err(_) => Err((
            "capture_timeout".to_owned(),
            format!("capture exceeded internal deadline {deadline:?}（Adapter 看门狗预算 {budget_timeout_s}s 先行 TIMED_OUT 语义承载）"),
        )),
        Ok(Err(fail)) => Err(fail),
        Ok(Ok(mut data)) => {
            data["elapsed_ms"] = json!(started.elapsed().as_millis() as u64);
            Ok(data)
        }
    }
}

async fn capture_once(
    browser_bin: &Path,
    params: &BrowserCaptureParams,
    trace_id: Option<&str>,
    settle: std::time::Duration,
    out_dir: &Path,
) -> Result<Value, CaptureFailure> {
    if !browser_bin.exists() {
        return Err((
            "browser_missing".to_owned(),
            format!("browser_bin 不存在：{}", browser_bin.display()),
        ));
    }
    let user_data_dir = fresh_user_data_dir();
    // Chromium 子进程 env 注入（Adapter env_clear 白名单 TMPDIR = 任务深层
    // tmp → process_singleton SingletonSocket 超 Unix socket 108 字符帽
    // FATAL——Socket path too long，2026-11-23 e2e 实录）：XDG_RUNTIME_DIR/
    // TMPDIR 双面压短到一次性 user-data-dir 面（同 run 终局清理承载）。
    let chromium_tmp = user_data_dir
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("/tmp"));
    let config = BrowserConfig::builder()
        .chrome_executable(browser_bin)
        .no_sandbox()
        .user_data_dir(&user_data_dir)
        .env("XDG_RUNTIME_DIR", user_data_dir.display().to_string())
        .env("TMPDIR", chromium_tmp.display().to_string())
        .build()
        .map_err(|e| {
            (
                "launch_failed".to_owned(),
                format!("BrowserConfig 构造失败: {e}"),
            )
        })?;
    let (mut browser, mut handler) = Browser::launch(config).await.map_err(|e| {
        (
            "launch_failed".to_owned(),
            format!("Chromium 启动失败: {e}"),
        )
    })?;
    let pump = tokio::spawn(async move {
        while let Some(event) = handler.next().await {
            let _ = event;
        }
    });
    let result = capture_session(&browser, params, trace_id, settle, out_dir).await;
    let _ = browser.close().await;
    // close 后等待 chromium 子进程完全退出（Browser::wait 既有面——不等即
    // remove 与 chromium 收尾写盘竞态，user-data-dir 残留）。
    let _ = browser.wait().await;
    let _ = pump.await;
    // 一次性 user-data-dir 终局清理（判据 6：tmp 残留 = 0）。
    let _ = std::fs::remove_dir_all(&user_data_dir);
    result
}

async fn capture_session(
    browser: &Browser,
    params: &BrowserCaptureParams,
    trace_id: Option<&str>,
    settle: std::time::Duration,
    out_dir: &Path,
) -> Result<Value, CaptureFailure> {
    let page = browser
        .new_page("about:blank")
        .await
        .map_err(|e| ("session_failed".to_owned(), format!("new_page 失败: {e}")))?;
    // Network 域事件摘要（D4③）：请求行收集器在 navigate 前挂订。
    let methods: Arc<Mutex<BTreeMap<String, String>>> = Arc::new(Mutex::new(BTreeMap::new()));
    let rows: Arc<Mutex<Vec<Value>>> = Arc::new(Mutex::new(Vec::new()));
    let mut responses = page
        .event_listener::<EventResponseReceived>()
        .await
        .map_err(|e| {
            (
                "session_failed".to_owned(),
                format!("event_listener 失败: {e}"),
            )
        })?;
    let mut requests = page
        .event_listener::<EventRequestWillBeSent>()
        .await
        .map_err(|e| {
            (
                "session_failed".to_owned(),
                format!("event_listener 失败: {e}"),
            )
        })?;
    let methods_task = methods.clone();
    let req_collector = tokio::spawn(async move {
        while let Some(ev) = requests.next().await {
            methods_task
                .lock()
                .expect("methods lock")
                .insert(ev.request_id.inner().clone(), ev.request.method.clone());
        }
    });
    let methods_res = methods.clone();
    let rows_task = rows.clone();
    let res_collector = tokio::spawn(async move {
        while let Some(ev) = responses.next().await {
            let response = &ev.response;
            let method = methods_res
                .lock()
                .expect("methods lock")
                .get(ev.request_id.inner())
                .cloned()
                .unwrap_or_default();
            rows_task.lock().expect("rows lock").push(json!({
                "url": response.url,
                "method": method,
                "status": response.status,
                "resource_type": resource_type_of(&response.mime_type),
                "size": response.encoded_data_length,
            }));
        }
    });

    // navigate → load（goto 在 URL 完全加载后返回）→ settle。
    page.goto(params.url.as_str())
        .await
        .map_err(|e| ("navigate_failed".to_owned(), format!("goto 失败: {e}")))?;
    page.wait_for_navigation().await.map_err(|e| {
        (
            "navigate_failed".to_owned(),
            format!("wait_for_navigation 失败: {e}"),
        )
    })?;
    tokio::time::sleep(settle).await;

    // 元数据面（meta.json 九字段，PLAN :327 逐词）。
    let final_url = page
        .url()
        .await
        .map_err(|e| ("session_failed".to_owned(), format!("url 失败: {e}")))?
        .unwrap_or_else(|| params.url.clone());
    let title = page
        .get_title()
        .await
        .map_err(|e| ("session_failed".to_owned(), format!("get_title 失败: {e}")))?
        .unwrap_or_default();
    let load_status = eval_string(&page, "String(document.readyState)").await?;
    let dpr = eval_value(&page, "Number(window.devicePixelRatio || 1)")
        .await
        .ok()
        .and_then(|v| v.as_f64())
        .unwrap_or(1.0);
    let viewport_value = eval_value(
        &page,
        "JSON.stringify({width: window.innerWidth || 0, height: window.innerHeight || 0})",
    )
    .await
    .ok()
    .and_then(|v| serde_json::from_str::<Value>(v.as_str().unwrap_or("{}")).ok())
    .unwrap_or_else(|| json!({"width": 0, "height": 0}));
    let browser_version = eval_string(&page, "String(navigator.userAgent)").await?;
    let captured_at = wsw_runtime::persist::now_iso();

    // 截图四模式（D4⑤：viewport/full_page/element/region；CDP 原生支持零自研裁剪）。
    let png: Vec<u8> = match params.mode.as_str() {
        "viewport" => page
            .screenshot(
                ScreenshotParams::builder()
                    .format(CaptureScreenshotFormat::Png)
                    .build(),
            )
            .await
            .map_err(|e| {
                (
                    "screenshot_failed".to_owned(),
                    format!("viewport 截图失败: {e}"),
                )
            })?,
        "full_page" => page
            .screenshot(
                ScreenshotParams::builder()
                    .format(CaptureScreenshotFormat::Png)
                    .full_page(true)
                    .build(),
            )
            .await
            .map_err(|e| {
                (
                    "screenshot_failed".to_owned(),
                    format!("full_page 截图失败: {e}"),
                )
            })?,
        "element" => {
            let selector = params.selector.as_deref().ok_or_else(|| {
                (
                    "validation".to_owned(),
                    "element 模式缺 selector".to_owned(),
                )
            })?;
            let element = page.find_element(selector).await.map_err(|_| {
                (
                    "selector_not_found".to_owned(),
                    format!("selector 无命中（结构化失败）：{selector}"),
                )
            })?;
            element
                .screenshot(CaptureScreenshotFormat::Png)
                .await
                .map_err(|e| {
                    (
                        "screenshot_failed".to_owned(),
                        format!("element 截图失败: {e}"),
                    )
                })?
        }
        "region" => {
            let clip = params
                .clip
                .as_ref()
                .ok_or_else(|| ("validation".to_owned(), "region 模式缺 clip".to_owned()))?;
            let vp = chromiumoxide::cdp::browser_protocol::page::Viewport {
                x: clip.get("x").and_then(Value::as_f64).unwrap_or(0.0),
                y: clip.get("y").and_then(Value::as_f64).unwrap_or(0.0),
                width: clip.get("width").and_then(Value::as_f64).unwrap_or(0.0),
                height: clip.get("height").and_then(Value::as_f64).unwrap_or(0.0),
                scale: 1.0,
            };
            page.screenshot(
                ScreenshotParams::builder()
                    .format(CaptureScreenshotFormat::Png)
                    .clip(vp)
                    .build(),
            )
            .await
            .map_err(|e| {
                (
                    "screenshot_failed".to_owned(),
                    format!("region 截图失败: {e}"),
                )
            })?
        }
        other => return Err(("validation".to_owned(), format!("mode 词表外：{other}"))),
    };

    // DOMSnapshot 结构化提取（D4①：strings/documents 结构化 JSON）。
    // computedStyles 为 CDP 必填白名单（空数组会被 skip 序列化 → -32602
    // Invalid parameters）；取 display 单项 = 最小白名单面。
    let mut snap = CaptureSnapshotParams::new(vec!["display".to_string()]);
    snap.include_dom_rects = Some(true);
    let snapshot = page
        .execute(snap)
        .await
        .map_err(|e| ("dom_failed".to_owned(), format!("DOMSnapshot 失败: {e}")))?;
    let dom_value = serde_json::to_value(&snapshot.result).map_err(|e| {
        (
            "dom_failed".to_owned(),
            format!("DOMSnapshot 序列化失败: {e}"),
        )
    })?;
    let dom_strings = dom_value
        .get("strings")
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or(0);
    let dom_documents = dom_value
        .get("documents")
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or(0);

    // 页面文本投影（D4②：document.body.innerText 有界投影）。
    let page_text = eval_string(
        &page,
        "String(document.body ? document.body.innerText : '')",
    )
    .await
    .unwrap_or_default();

    // out/ 五文件落盘（帽 + capture_fidelity 登记）。
    std::fs::create_dir_all(out_dir)
        .map_err(|e| ("io_failed".to_owned(), format!("out/ 创建失败: {e}")))?;
    let mut fidelity: Vec<Value> = Vec::new();
    let (screenshot_bytes, _, _) = write_capped(&out_dir.join("screenshot.png"), &png, usize::MAX)
        .map_err(|e| {
            (
                "io_failed".to_owned(),
                format!("screenshot.png 写盘失败: {e}"),
            )
        })?;
    let dom_bytes = serde_json::to_vec(&dom_value)
        .map_err(|e| ("io_failed".to_owned(), format!("dom.json 序列化失败: {e}")))?;
    let (dom_written, dom_raw, dom_trunc) =
        write_capped(&out_dir.join("dom.json"), &dom_bytes, DOM_CAP_BYTES)
            .map_err(|e| ("io_failed".to_owned(), format!("dom.json 写盘失败: {e}")))?;
    if dom_trunc {
        fidelity.push(json!({
            "artifact": "dom.json",
            "reason": "cap_exceeded",
            "cap_bytes": DOM_CAP_BYTES,
            "raw_bytes": dom_raw,
            "written_bytes": dom_written,
        }));
    }
    let text_bytes = page_text.as_bytes();
    let (text_written, text_raw, text_trunc) =
        write_capped(&out_dir.join("page.txt"), text_bytes, PAGE_TEXT_CAP_BYTES)
            .map_err(|e| ("io_failed".to_owned(), format!("page.txt 写盘失败: {e}")))?;
    if text_trunc {
        fidelity.push(json!({
            "artifact": "page.txt",
            "reason": "cap_exceeded",
            "cap_bytes": PAGE_TEXT_CAP_BYTES,
            "raw_bytes": text_raw,
            "written_bytes": text_written,
        }));
    }
    let network_rows = rows.lock().expect("rows lock").clone();
    let network_value = json!({"requests": network_rows});
    let network_bytes = serde_json::to_vec(&network_value).map_err(|e| {
        (
            "io_failed".to_owned(),
            format!("network.json 序列化失败: {e}"),
        )
    })?;
    let (net_written, net_raw, net_trunc) = write_capped(
        &out_dir.join("network.json"),
        &network_bytes,
        NETWORK_CAP_BYTES,
    )
    .map_err(|e| {
        (
            "io_failed".to_owned(),
            format!("network.json 写盘失败: {e}"),
        )
    })?;
    if net_trunc {
        fidelity.push(json!({
            "artifact": "network.json",
            "reason": "cap_exceeded",
            "cap_bytes": NETWORK_CAP_BYTES,
            "raw_bytes": net_raw,
            "written_bytes": net_written,
        }));
    }
    // meta.json：PLAN :327 九字段逐词 + capture_fidelity（截断登记面）。
    let meta = json!({
        "current_url": params.url,
        "final_url": final_url,
        "title": title,
        "captured_at": captured_at,
        "viewport": viewport_value,
        "dpr": dpr,
        "browser_version": browser_version,
        "load_status": load_status,
        "trace_id": trace_id,
        "capture_fidelity": fidelity,
    });
    std::fs::write(
        out_dir.join("meta.json"),
        serde_json::to_vec(&meta)
            .map_err(|e| ("io_failed".to_owned(), format!("meta.json 序列化失败: {e}")))?,
    )
    .map_err(|e| ("io_failed".to_owned(), format!("meta.json 写盘失败: {e}")))?;

    let network_count = network_rows.len();
    req_collector.abort();
    res_collector.abort();
    Ok(json!({
        "status": "ok",
        "mode": params.mode,
        "current_url": params.url,
        "final_url": final_url,
        "title": title,
        "screenshot_bytes": screenshot_bytes,
        "dom_strings": dom_strings,
        "dom_documents": dom_documents,
        "page_text_bytes": text_written,
        "network_rows": network_count,
        "capture_fidelity": fidelity,
    }))
}

/// Runtime.evaluate 字符串面（D4②投影载体）。
async fn eval_string(
    page: &chromiumoxide::Page,
    expression: &str,
) -> Result<String, CaptureFailure> {
    let result = page
        .evaluate(expression)
        .await
        .map_err(|e| ("evaluate_failed".to_owned(), format!("evaluate 失败: {e}")))?;
    result.into_value::<String>().map_err(|e| {
        (
            "evaluate_failed".to_owned(),
            format!("evaluate 值类型失败: {e}"),
        )
    })
}

/// Runtime.evaluate 任意值面。
async fn eval_value(page: &chromiumoxide::Page, expression: &str) -> Result<Value, CaptureFailure> {
    let result = page
        .evaluate(expression)
        .await
        .map_err(|e| ("evaluate_failed".to_owned(), format!("evaluate 失败: {e}")))?;
    result.into_value::<Value>().map_err(|e| {
        (
            "evaluate_failed".to_owned(),
            format!("evaluate 值类型失败: {e}"),
        )
    })
}

/// mime_type → resource_type 词面（PLAN :328「网络请求摘要」词形承载；
/// 本 CDP revision Response 无原生 resource_type 字段，按 mime 投影）。
fn resource_type_of(mime_type: &str) -> &'static str {
    let m = mime_type.split(';').next().unwrap_or("").trim();
    if m.starts_with("text/html") || m.starts_with("application/xhtml") {
        "document"
    } else if m.starts_with("text/css") {
        "stylesheet"
    } else if m.starts_with("image/") {
        "image"
    } else if m.starts_with("font/") || m.contains("font") {
        "font"
    } else if m.starts_with("application/javascript") || m.starts_with("text/javascript") {
        "script"
    } else if m.starts_with("application/json") {
        "fetch"
    } else {
        "other"
    }
}
