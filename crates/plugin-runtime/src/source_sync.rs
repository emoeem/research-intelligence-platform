//! source_sync：`rip.knowledge.source.sync`（T1，P6-11 第 2 步，安全知识源
//! 计划 §6.2D）的受控同步编排。
//!
//! 纪律（计划 §6.2D 逐条落码）：
//! - 只调 Plugin Runtime 受控 API（update-check + ingest），不直写 SQLite；
//!   本模块的写面只有 ingest（SAVEPOINT dry-run 语义在 ingest 内）与
//!   sync-keys.jsonl 侧车文件（幂等键台账，非数据库）；
//! - dry-run：完整解析 + 去重判定，写步回滚零落盘（IngestReport.dry_run）；
//! - idempotency_key：与既有成功 sync 键相同则整体跳过（零落盘），键台账
//!   只在实跑成功后追加；
//! - 限额：包内文件数/字节总量上限（超限不摄取，fail-fast），墙钟耗时上限
//!   （摄取为同步调用不可中断，超限如实计入 limits_exceeded 部分失败）；
//! - 失败重试：每次尝试机器可读（attempts[]），重试间隔固定 100ms；
//! - Git 更新先固定解析到的完整 Commit：pin 前置闸在本模块内
//!   （floating_ref_rejected），git pinned.kind 只接 commit/tag/release，
//!   kind=commit 必须全 40 位（完整 Commit 语义，比 registry 的 7-64 短哈希
//!   更严）；无 --package 的取包路径走 source_fetch::verify + fetch_into
//!   （P6-4 既有面），不执行仓库中的任何脚本。

use std::path::Path;
use std::time::{Duration, Instant};

use serde::Serialize;
use serde_json::{json, Value};

use crate::ingest::{ingest_knowledge_source_with_opts, IngestOptions, IngestReport};
use crate::RuntimeError;

/// 固定 100ms 重试间隔（避免紧贴重试放大瞬时故障）。
const RETRY_BACKOFF: Duration = Duration::from_millis(100);

/// sync 键台账文件名（state/sync-keys.jsonl；非数据库侧车）。
pub const SYNC_KEYS_FILE_NAME: &str = "sync-keys.jsonl";

/// 同步选项。
#[derive(Debug, Clone, Default)]
pub struct SyncOptions {
    /// dry-run：完整解析 + 去重判定，写步 SAVEPOINT 回滚零落盘。
    pub dry_run: bool,
    /// 幂等键：与既有成功 sync 键相同则整体跳过。
    pub idempotency_key: Option<String>,
    /// 包内文件数上限（超限不摄取）。
    pub max_files: Option<u64>,
    /// 包内字节总量上限（超限不摄取）。
    pub max_bytes: Option<u64>,
    /// 墙钟耗时上限秒（摄取不可中断，超限计部分失败）。
    pub max_seconds: Option<u64>,
    /// 失败额外重试次数（0 = 单次尝试）。
    pub retries: u32,
    /// update-check 结果（受控 API；CLI 传入，None = 未执行）。
    pub update_check_result: Option<bool>,
    /// 摄取成功后是否映射候选资源（rp-plugin 传 --map-candidates 时置位，
    /// 映射本体由 CLI 调 resource-engine 受控方法并审计）。
    pub map_candidates: bool,
    /// 既有成功 sync 键（调用方从 sync-keys.jsonl 读出；库内不读文件）。
    pub prior_sync_keys: Vec<String>,
}

/// 单次尝试记录（机器可读）。
#[derive(Debug, Clone, Serialize)]
pub struct SyncAttempt {
    /// 1 起始尝试序号。
    pub index: u32,
    /// 本尝试是否成功。
    pub ok: bool,
    /// 失败错误码（RuntimeError.code）。
    pub error_code: Option<String>,
    /// 失败信息。
    pub error_message: Option<String>,
}

/// 同步报告（机器可读；CLI 输出与审计 detail 共用）。
#[derive(Debug, Clone, Serialize)]
pub struct SyncReport {
    /// 来源插件 id。
    pub plugin_id: String,
    /// 来源插件版本（manifest version）。
    pub plugin_version: String,
    /// source.pinned.kind（commit/tag/release；无 source 块为 None）。
    pub pin_kind: Option<String>,
    /// 固定 Commit（kind=commit 的 pinned.value）。
    pub pinned_commit: Option<String>,
    /// 本次是否 dry-run。
    pub dry_run: bool,
    /// 幂等键。
    pub idempotency_key: Option<String>,
    /// 幂等键命中既有成功 sync（整体跳过，零落盘）。
    pub idempotent_skipped: bool,
    /// update-check 结果（None = 未执行）。
    pub update_check_new_version: Option<bool>,
    /// 包内文件数（限额核对面）。
    pub package_files: Option<u64>,
    /// 包内字节总量（限额核对面）。
    pub package_bytes: Option<u64>,
    /// 逐次尝试记录。
    pub attempts: Vec<SyncAttempt>,
    /// 最终摄取报告（摄取执行过且未超限时给出）。
    pub final_report: Option<IngestReport>,
    /// 超限项（max_files/max_bytes/max_seconds）。
    pub limits_exceeded: Vec<String>,
    /// 部分失败说明（机器可读短语）。
    pub partial_failures: Vec<String>,
    /// 总耗时毫秒。
    pub elapsed_ms: u64,
    /// 整体成功（无未恢复失败且未超限）。
    pub ok: bool,
}

/// 单行 JSON（CLI/审计复用）。
pub fn report_json(report: &SyncReport) -> Value {
    serde_json::to_value(report).unwrap_or_else(|_| json!({"error": "report serialize"}))
}

/// pin 前置闸：git 来源必须有 pinned（kind 只接 commit/tag/release；
/// kind=commit 必须全 40 位完整 Commit）；local_dir 的 pinned（若给）必须是
/// commit。浮动分支 fail-fast 拒绝（floating_ref_rejected），不触源。
pub fn verify_pin(manifest: &Value) -> Result<(Option<String>, Option<String>), RuntimeError> {
    let source = match manifest.get("source") {
        Some(s) if s.is_object() => s,
        None => return Ok((None, None)), // 无 source 块（纯本地 fixture 形态）
        Some(_) => {
            return Err(RuntimeError::new(
                "manifest_invalid",
                "source must be an object",
            ))
        }
    };
    let source_type = source
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let pinned = match source.get("pinned").filter(|p| !p.is_null()) {
        Some(p) if p.is_object() => p,
        None if source_type == "git" => {
            return Err(RuntimeError::new(
                "pin_missing",
                "git source requires pinned (sync requires a pinned snapshot)",
            ))
        }
        None => return Ok((None, None)),
        Some(_) => {
            return Err(RuntimeError::new(
                "manifest_invalid",
                "source.pinned must be an object",
            ))
        }
    };
    let kind = pinned
        .get("kind")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let value = pinned
        .get("value")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let is_remote = matches!(
        source_type.as_str(),
        "git" | "archive" | "mcp_server" | "http_api"
    );
    if is_remote && !matches!(kind.as_str(), "commit" | "tag" | "release") {
        return Err(RuntimeError::new(
            "floating_ref_rejected",
            format!(
                "pinned.kind {kind:?} is not commit/tag/release; floating refs are forbidden \
                 for source sync (draft D8, PLUGIN-CONTRACT 6)"
            ),
        ));
    }
    if !is_remote && kind != "commit" {
        return Err(RuntimeError::new(
            "floating_ref_rejected",
            format!("local pinned.kind {kind:?} must be \"commit\""),
        ));
    }
    if kind == "commit" {
        if !(value.len() == 40 && value.bytes().all(|b| b.is_ascii_hexdigit())) {
            return Err(RuntimeError::new(
                "pin_value_invalid",
                format!(
                    "pinned.value {value:?} must be a full 40-hex commit for source sync \
                     (完整 Commit 语义)"
                ),
            ));
        }
        return Ok((Some(kind), Some(value.to_lowercase())));
    }
    Ok((Some(kind), None))
}

/// 包统计（递归文件数与字节总量；限额核对面）。
pub fn package_stats(package_dir: &Path) -> Result<(u64, u64), RuntimeError> {
    fn walk(dir: &Path, files: &mut u64, bytes: &mut u64) -> Result<(), RuntimeError> {
        let it = std::fs::read_dir(dir)
            .map_err(|e| RuntimeError::new("io", format!("read_dir {dir:?}: {e}")))?;
        for entry in it {
            let entry = entry.map_err(|e| RuntimeError::new("io", format!("entry: {e}")))?;
            let path = entry.path();
            let meta = entry
                .metadata()
                .map_err(|e| RuntimeError::new("io", format!("metadata {path:?}: {e}")))?;
            if meta.is_dir() {
                walk(&path, files, bytes)?;
            } else if meta.is_file() {
                *files += 1;
                *bytes += meta.len();
            }
        }
        Ok(())
    }
    let mut files = 0u64;
    let mut bytes = 0u64;
    walk(package_dir, &mut files, &mut bytes)?;
    Ok((files, bytes))
}

/// 追加一条 sync 键记录（state/sync-keys.jsonl；原子性同 transitions 追加）。
pub fn append_sync_key(state_dir: &Path, key: &str, plugin_id: &str) -> Result<(), RuntimeError> {
    std::fs::create_dir_all(state_dir)
        .map_err(|e| RuntimeError::new("io", format!("create_dir {state_dir:?}: {e}")))?;
    let path = state_dir.join(SYNC_KEYS_FILE_NAME);
    let line = json!({
        "key": key,
        "plugin_id": plugin_id,
        "at_unix": plugin_registry::now_unix(),
    });
    use std::io::Write;
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|e| RuntimeError::new("io", format!("open {path:?}: {e}")))?;
    writeln!(f, "{line}").map_err(|e| RuntimeError::new("io", format!("write: {e}")))?;
    f.sync_all()
        .map_err(|e| RuntimeError::new("io", format!("sync: {e}")))?;
    Ok(())
}

/// 读回既有成功 sync 键（只读侧车，无文件返回空表）。
pub fn read_sync_keys(state_dir: &Path) -> Vec<String> {
    let path = state_dir.join(SYNC_KEYS_FILE_NAME);
    let Ok(body) = std::fs::read_to_string(&path) else {
        return Vec::new();
    };
    body.lines()
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
        .filter_map(|v| v.get("key").and_then(Value::as_str).map(str::to_string))
        .collect()
}

/// 执行受控同步（pin 前置闸 → 幂等键 → 限额 → 重试摄取 → 报告）。
pub fn sync_knowledge_source(
    pool: &database::Pool,
    manifest: &Value,
    package_dir: &Path,
    opts: &SyncOptions,
) -> Result<SyncReport, RuntimeError> {
    let started = Instant::now();
    let plugin_id = manifest
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(|| RuntimeError::new("manifest_invalid", "manifest has no `id`"))?
        .to_string();
    let plugin_version = manifest
        .get("version")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();

    // 1) pin 前置闸（floating 分支 fail-fast，不触源）。
    let (pin_kind, pinned_commit) = verify_pin(manifest)?;

    let mut report = SyncReport {
        plugin_id: plugin_id.clone(),
        plugin_version: plugin_version.clone(),
        pin_kind,
        pinned_commit: pinned_commit.clone(),
        dry_run: opts.dry_run,
        idempotency_key: opts.idempotency_key.clone(),
        idempotent_skipped: false,
        update_check_new_version: opts.update_check_result,
        package_files: None,
        package_bytes: None,
        attempts: Vec::new(),
        final_report: None,
        limits_exceeded: Vec::new(),
        partial_failures: Vec::new(),
        elapsed_ms: 0,
        ok: false,
    };

    // 2) 幂等键：命中既有成功 sync 键则整体跳过（零落盘）。
    if let Some(key) = &opts.idempotency_key {
        if opts.prior_sync_keys.iter().any(|k| k == key) {
            report.idempotent_skipped = true;
            report.ok = true;
            report.elapsed_ms = started.elapsed().as_millis() as u64;
            return Ok(report);
        }
    }

    // 3) 包统计 + 限额（超限不摄取，fail-fast）。
    let (files, bytes) = package_stats(package_dir)?;
    report.package_files = Some(files);
    report.package_bytes = Some(bytes);
    if let Some(cap) = opts.max_files {
        if files > cap {
            report.limits_exceeded.push("max_files".to_string());
        }
    }
    if let Some(cap) = opts.max_bytes {
        if bytes > cap {
            report.limits_exceeded.push("max_bytes".to_string());
        }
    }
    if let Some(cap) = opts.max_seconds {
        if started.elapsed().as_secs() > cap {
            report.limits_exceeded.push("max_seconds".to_string());
        }
    }
    if !report.limits_exceeded.is_empty() {
        report.partial_failures.push(format!(
            "limits exceeded: {:?}; ingest not attempted (fail-fast)",
            report.limits_exceeded
        ));
        report.elapsed_ms = started.elapsed().as_millis() as u64;
        return Ok(report); // ok=false 由调用方语义表（limits_exceeded 非空）
    }

    // 4) 墙钟预算 + 重试摄取（受控 API：ingest_knowledge_source_with_opts）。
    let opts_ingest = IngestOptions {
        dry_run: opts.dry_run,
    };
    let total_attempts = opts.retries + 1;
    let mut last_err: Option<RuntimeError> = None;
    for attempt in 1..=total_attempts {
        if let Some(cap) = opts.max_seconds {
            if started.elapsed().as_secs() > cap {
                report.limits_exceeded.push("max_seconds".to_string());
                report
                    .partial_failures
                    .push("max_seconds exceeded before ingest attempt".to_string());
                break;
            }
        }
        match ingest_knowledge_source_with_opts(pool, manifest, package_dir, &opts_ingest) {
            Ok(r) => {
                report.attempts.push(SyncAttempt {
                    index: attempt,
                    ok: true,
                    error_code: None,
                    error_message: None,
                });
                report.final_report = Some(r);
                break;
            }
            Err(e) => {
                report.attempts.push(SyncAttempt {
                    index: attempt,
                    ok: false,
                    error_code: Some(e.code.clone()),
                    error_message: Some(e.message.clone()),
                });
                last_err = Some(e);
                if attempt < total_attempts {
                    std::thread::sleep(RETRY_BACKOFF);
                }
            }
        }
    }

    if report.final_report.is_none() {
        if let Some(e) = last_err {
            report.partial_failures.push(format!(
                "ingest failed after {} attempt(s): [{}] {}",
                report.attempts.len(),
                e.code,
                e.message
            ));
        }
    }
    if let Some(cap) = opts.max_seconds {
        if started.elapsed().as_secs() > cap
            && !report.limits_exceeded.contains(&"max_seconds".to_string())
        {
            report.limits_exceeded.push("max_seconds".to_string());
            report.partial_failures.push(
                "max_seconds exceeded after ingest (synchronous call not interruptible)"
                    .to_string(),
            );
        }
    }
    report.ok = report.final_report.is_some() && report.limits_exceeded.is_empty();
    report.elapsed_ms = started.elapsed().as_millis() as u64;
    Ok(report)
}
