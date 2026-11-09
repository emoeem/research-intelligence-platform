//! python_worker Runtime Adapter（宿主派发器，TOOL-CONTRACT §4.2 映射 +
//! §3.1「RUNNING 进入条件：python_worker 已完成 initialize 且 execute 已送达」）。
//!
//! 职责（盘点差距第 4 条修补）：进程池（pooled 复用 / per_run 每次新进程）、
//! initialize/get_capabilities 握手（worker_name 须与响应一致，§2.2）、
//! execute/cancel 转发（§4.2 映射表：input→task、submit_key 直通
//! idempotency_key、timeout_seconds×1000→timeout_ms、run_id 不进 task）、
//! stderr 结构化日志采集（worker 纪律「stdout 不承载日志」在宿主侧分轨）。
//!
//! 固定 worker 入口 argv（Command 直拉，不经 shell）；协议行 framing 为
//! 「stdin/stdout 各一行 JSON」（D-TC-03）。

use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use serde_json::{json, Map, Value};

use crate::adapter::{map_worker_code, AdapterFailure, AdapterOutcome};
use crate::envelope::{self, EnvelopeError};
use crate::{DriveOutcome, Engine, EngineError, TerminalParts};
use domain::ToolRunStatus;

/// worker 进程配置（Manifest runtime 块的运行面投影；登记侧持久化属 P1-7）。
#[derive(Debug, Clone)]
pub struct PythonWorkerConfig {
    /// Manifest runtime.worker_name（须与 initialize 响应一致）。
    pub worker_name: String,
    /// 固定入口 argv（如 ["python3", "-I", "workers/python/parsing_worker.py"]），
    /// 不经 shell、不拼接用户输入。
    pub command: Vec<String>,
    /// working_dir（repo:// 前缀落到仓库根目录路径）。
    pub working_dir: Option<PathBuf>,
    /// pooled：同进程串行复用；per_run：每次执行新进程。
    pub restart: RestartPolicy,
    /// 追加环境变量（如 WORKER_ALLOW_DEBUG=1 调试面、WORKER_CACHE_DIR 重定向）。
    pub extra_env: Vec<(String, String)>,
    /// 握手超时（默认 10s）。
    pub handshake_timeout: Duration,
}

/// 重启策略（Manifest runtime.restart 词表）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RestartPolicy {
    /// 同进程串行复用（默认）。
    Pooled,
    /// 每次运行新进程。
    PerRun,
}

/// 一个已握手的 worker 进程。
struct PooledWorker {
    child: Child,
    stdin: ChildStdin,
    rx: Receiver<String>,
    worker_version: String,
}

impl Drop for PooledWorker {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// python_worker 派发器。
pub struct PythonWorkerAdapter {
    config: PythonWorkerConfig,
    pooled: Mutex<Option<PooledWorker>>,
    next_id: AtomicI64,
    exec_lock: Mutex<()>,
    stderr_lines: Arc<Mutex<Vec<String>>>,
}

impl PythonWorkerAdapter {
    /// 以给定配置构造派发器。
    pub fn new(config: PythonWorkerConfig) -> Self {
        Self {
            config,
            pooled: Mutex::new(None),
            next_id: AtomicI64::new(1),
            exec_lock: Mutex::new(()),
            stderr_lines: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// 采集到的 worker stderr 行（结构化 JSON 日志，README.md:114）。
    pub fn stderr_lines(&self) -> Vec<String> {
        self.stderr_lines
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// PREPARING 期握手预热：pooled 池为空则拉起进程并握手；per_run 为 no-op
    /// （进程在 execute 时拉起，握手失败同样映射 INFRA）。
    pub fn ensure_ready(&self) -> Result<(), AdapterFailure> {
        match self.config.restart {
            RestartPolicy::Pooled => {
                let mut guard = self
                    .pooled
                    .lock()
                    .map_err(|_| AdapterFailure::new("pool mutex poisoned"))?;
                if guard.is_none() {
                    *guard = Some(self.spawn_and_handshake()?);
                }
                Ok(())
            }
            RestartPolicy::PerRun => Ok(()),
        }
    }

    /// 单次执行：obtain（复用或新进程）→ §4.2 映射下发 execute → 等待响应行
    /// （cancel_after 到点转发 cancel，协作式取消）→ 中性终局。
    pub fn execute(
        &self,
        task: &Value,
        timeout_ms: u64,
        max_output_bytes: u64,
        idempotency_key: Option<&str>,
        cancel_after: Option<Duration>,
    ) -> Result<AdapterOutcome, AdapterFailure> {
        let _serial = self
            .exec_lock
            .lock()
            .map_err(|_| AdapterFailure::new("exec mutex poisoned"))?;
        let mut worker = self.obtain_worker()?;
        let request_id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let mut params = Map::new();
        params.insert("task".into(), task.clone());
        params.insert(
            "budget".into(),
            json!({"timeout_ms": timeout_ms, "max_output_bytes": max_output_bytes}),
        );
        if let Some(key) = idempotency_key {
            params.insert("idempotency_key".into(), json!(key));
        }
        let request = json!({
            "jsonrpc": "2.0",
            "id": request_id,
            "method": "execute",
            "params": Value::Object(params),
        });
        send_line(&mut worker.stdin, &request)
            .map_err(|e| AdapterFailure::new(format!("write execute: {e}")))?;

        // 读循环：cancel_after 到点转发 cancel（§4.2），总期限 = worker 预算
        // + 平台余量（worker 内 timer 会在 timeout_ms 处主动交付 timeout 响应）。
        let overall = Duration::from_millis(timeout_ms).saturating_add(Duration::from_secs(15));
        let start = Instant::now();
        let mut cancel_sent = false;
        loop {
            let elapsed = start.elapsed();
            if elapsed >= overall {
                return Err(AdapterFailure::new(format!(
                    "execute deadline exceeded ({} ms budget + platform margin)",
                    timeout_ms
                )));
            }
            if let Some(after) = cancel_after {
                if !cancel_sent && elapsed >= after {
                    let cancel_req = json!({
                        "jsonrpc": "2.0",
                        "id": self.next_id.fetch_add(1, Ordering::SeqCst),
                        "method": "cancel",
                        "params": {"target_request_id": request_id},
                    });
                    send_line(&mut worker.stdin, &cancel_req)
                        .map_err(|e| AdapterFailure::new(format!("write cancel: {e}")))?;
                    cancel_sent = true;
                }
            }
            let until_cancel = cancel_after
                .filter(|_| !cancel_sent)
                .map(|a| a.saturating_sub(elapsed));
            let wait = until_cancel
                .unwrap_or(overall - elapsed)
                .max(Duration::from_millis(5));
            match worker.rx.recv_timeout(wait) {
                Ok(line) => {
                    let reply = parse_reply_line(&line, request_id)
                        .map_err(|e| AdapterFailure::new(format!("bad protocol line: {e}")))?;
                    if let Some(reply) = reply {
                        let outcome = match reply {
                            Reply::Result(v) => execute_outcome_from_result(&v).map_err(|e| {
                                AdapterFailure::new(format!("bad execute response: {e}"))
                            })?,
                            Reply::Error { code, message } => {
                                let (category, retryable) = map_worker_code(code);
                                AdapterOutcome::Failed {
                                    category: category.to_string(),
                                    code: json!(code),
                                    message,
                                    retryable,
                                }
                            }
                        };
                        self.release_worker(worker);
                        return Ok(outcome);
                    }
                    // 其余行（cancel 响应、乱序响应）忽略并继续等待本 id。
                }
                Err(RecvTimeoutError::Timeout) => continue,
                Err(RecvTimeoutError::Disconnected) => {
                    return Err(AdapterFailure::new("worker stdout closed unexpectedly"))
                }
            }
        }
    }

    /// 关闭 pooled 进程（测试收尾用；Drop 亦兜底 kill）。
    pub fn shutdown(&self) {
        if let Ok(mut guard) = self.pooled.lock() {
            *guard = None;
        }
    }

    fn obtain_worker(&self) -> Result<PooledWorker, AdapterFailure> {
        match self.config.restart {
            RestartPolicy::Pooled => {
                let mut guard = self
                    .pooled
                    .lock()
                    .map_err(|_| AdapterFailure::new("pool mutex poisoned"))?;
                match guard.take() {
                    Some(w) => Ok(w),
                    None => self.spawn_and_handshake(),
                }
            }
            RestartPolicy::PerRun => self.spawn_and_handshake(),
        }
    }

    fn release_worker(&self, worker: PooledWorker) {
        if self.config.restart == RestartPolicy::Pooled {
            if let Ok(mut guard) = self.pooled.lock() {
                *guard = Some(worker);
                return;
            }
        }
        drop(worker);
    }

    fn spawn_and_handshake(&self) -> Result<PooledWorker, AdapterFailure> {
        if self.config.command.is_empty() {
            return Err(AdapterFailure::new("empty worker command"));
        }
        let mut cmd = Command::new(&self.config.command[0]);
        cmd.args(&self.config.command[1..])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if let Some(dir) = &self.config.working_dir {
            cmd.current_dir(dir);
        }
        for (k, v) in &self.config.extra_env {
            cmd.env(k, v);
        }
        let mut child = cmd
            .spawn()
            .map_err(|e| AdapterFailure::new(format!("spawn worker: {e}")))?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| AdapterFailure::new("no stdin"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| AdapterFailure::new("no stdout"))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| AdapterFailure::new("no stderr"))?;

        // stdout 读线程：行 → channel（协议纪律：stdout 只承载协议行）。
        let (tx, rx) = channel::<String>();
        thread::spawn(move || {
            let reader = BufReader::new(stdout);
            for line in reader.lines() {
                match line {
                    Ok(l) => {
                        if tx.send(l).is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        });
        // stderr 采集线程：结构化日志进共享缓冲，绝不混入 stdout 协议轨。
        let stderr_sink = Arc::clone(&self.stderr_lines);
        thread::spawn(move || {
            let reader = BufReader::new(stderr);
            for line in reader.lines().map_while(Result::ok) {
                if let Ok(mut buf) = stderr_sink.lock() {
                    buf.push(line);
                }
            }
        });

        let mut worker = PooledWorker {
            child,
            stdin,
            rx,
            worker_version: String::new(),
        };

        // initialize（协议版本恒 "1"）。
        let init = self.call_method(&mut worker, "initialize", json!({"protocol_version": "1"}))?;
        let init = match init {
            Reply::Result(v) => v,
            Reply::Error { code, message } => {
                let (category, _) = map_worker_code(code);
                return Err(AdapterFailure::new(format!(
                    "initialize failed [{category}] code {code}: {message}"
                )));
            }
        };
        let name = init
            .get("worker_name")
            .and_then(Value::as_str)
            .ok_or_else(|| AdapterFailure::new("initialize response missing worker_name"))?;
        if name != self.config.worker_name {
            return Err(AdapterFailure::new(format!(
                "worker_name mismatch: manifest {:?} vs initialize {:?} (TOOL-CONTRACT 2.2)",
                self.config.worker_name, name
            )));
        }
        worker.worker_version = init
            .get("worker_version")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        // get_capabilities：确认 execute 方法在册。
        let caps = self.call_method(&mut worker, "get_capabilities", json!({}))?;
        match caps {
            Reply::Result(v) => {
                let methods = v.get("methods").and_then(Value::as_array);
                if let Some(methods) = methods {
                    if !methods.iter().any(|m| m == "execute") {
                        return Err(AdapterFailure::new(
                            "worker capabilities missing execute method",
                        ));
                    }
                }
            }
            Reply::Error { code, message } => {
                return Err(AdapterFailure::new(format!(
                    "get_capabilities failed code {code}: {message}"
                )));
            }
        }
        Ok(worker)
    }

    /// 单方法同步调用（握手期专用；execute 的异步响应读循环独立实现）。
    fn call_method(
        &self,
        worker: &mut PooledWorker,
        method: &str,
        params: Value,
    ) -> Result<Reply, AdapterFailure> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let request = json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params});
        send_line(&mut worker.stdin, &request)
            .map_err(|e| AdapterFailure::new(format!("write {method}: {e}")))?;
        let deadline = Instant::now() + self.config.handshake_timeout;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Err(AdapterFailure::new(format!("{method} handshake timeout")));
            }
            match worker.rx.recv_timeout(left) {
                Ok(line) => {
                    if let Some(reply) = parse_reply_line(&line, id).map_err(|e| {
                        AdapterFailure::new(format!("{method} response invalid: {e}"))
                    })? {
                        return Ok(reply);
                    }
                }
                Err(RecvTimeoutError::Timeout) => {
                    return Err(AdapterFailure::new(format!("{method} handshake timeout")))
                }
                Err(RecvTimeoutError::Disconnected) => {
                    return Err(AdapterFailure::new("worker stdout closed during handshake"))
                }
            }
        }
    }
}

/// 协议响应行的解析结果（目标 id 匹配时才有值）。
enum Reply {
    /// JSON-RPC result 对象。
    Result(Value),
    /// JSON-RPC error 对象（握手/入参层错误）。
    Error { code: i64, message: String },
}

/// 解析一行协议响应：匹配 id 时返回 Reply，否则 None（cancel 响应等）。
fn parse_reply_line(line: &str, request_id: i64) -> Result<Option<Reply>, String> {
    let v: Value = serde_json::from_str(line).map_err(|e| format!("not JSON: {e}"))?;
    let id_matches = match v.get("id") {
        Some(Value::Number(n)) => n.as_i64() == Some(request_id),
        Some(Value::String(s)) => s.parse::<i64>().ok() == Some(request_id),
        _ => false,
    };
    if !id_matches {
        return Ok(None);
    }
    if let Some(err) = v.get("error") {
        return Ok(Some(Reply::Error {
            code: err.get("code").and_then(Value::as_i64).unwrap_or(0),
            message: err
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
        }));
    }
    let result = v
        .get("result")
        .and_then(Value::as_object)
        .ok_or_else(|| "response has neither result nor error".to_string())?;
    Ok(Some(Reply::Result(Value::Object(result.clone()))))
}

/// execute 响应（Reply::Result 形状）到适配器终局的映射（§4.2/README 码表）。
fn execute_outcome_from_result(result: &Value) -> Result<AdapterOutcome, String> {
    let status = result
        .get("status")
        .and_then(Value::as_str)
        .ok_or_else(|| "execute response missing status".to_string())?;
    let usage = result.get("usage").cloned();
    match status {
        "result" => Ok(AdapterOutcome::Succeeded {
            data: result.get("data").cloned().unwrap_or_else(|| json!({})),
            cached: result
                .get("cached")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            usage,
        }),
        "error" => {
            let err = result
                .get("error")
                .and_then(Value::as_object)
                .ok_or_else(|| "status=error without error object".to_string())?;
            let code = err.get("code").and_then(Value::as_i64).unwrap_or(0);
            let (category, retryable) = map_worker_code(code);
            Ok(AdapterOutcome::Failed {
                category: category.to_string(),
                code: json!(code),
                message: err
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                retryable,
            })
        }
        "timeout" => Ok(AdapterOutcome::TimedOut {
            message: result
                .get("error")
                .and_then(|e| e.get("message"))
                .and_then(Value::as_str)
                .unwrap_or("worker budget timeout")
                .to_string(),
        }),
        other => Err(format!("unknown execute status {other:?}")),
    }
}

fn send_line(stdin: &mut ChildStdin, value: &Value) -> std::io::Result<()> {
    let mut line = serde_json::to_string(value)?;
    line.push('\n');
    stdin.write_all(line.as_bytes())?;
    stdin.flush()
}

// ---------------------------------------------------------------------------
// 引擎集成：ToolCallRequest 端到端（受理 → 状态机 → 真实 worker → 信封）
// ---------------------------------------------------------------------------

/// 端到端调用的参数束。
pub struct AdapterCall<'a> {
    /// 引擎（同一测试库上的受理/状态机/信封面）。
    pub engine: &'a Engine,
    /// 已受理（QUEUED）的 run id。
    pub run_id: &'a str,
    /// python_worker 派发器。
    pub adapter: &'a PythonWorkerAdapter,
    /// 协作式取消触发点（RUNNING 期经过该时长后转发 cancel，D-TC-04）。
    pub cancel_after: Option<Duration>,
}

/// 端到端：QUEUED run 经真实 python_worker 执行并交付合法 ToolRunResult
/// 信封（ROADMAP P1-6 判据）。状态跃迁与 tool_run 行同步复用引擎内部机制。
pub fn execute_via_engine(call: AdapterCall<'_>) -> Result<DriveOutcome, EngineError> {
    let engine = call.engine;
    let row = engine.load_row(call.run_id)?;
    if row.status != ToolRunStatus::Queued {
        return Err(EngineError::NotStartable {
            status: row.status,
            run_id: call.run_id.to_string(),
        });
    }
    let started = Instant::now();
    let mut transcript = vec![ToolRunStatus::Queued];

    // 第 2 行：QUEUED → PREPARING（握手预热在本阶段，§3.1 PREPARING 语义）。
    engine.advance(
        call.run_id,
        ToolRunStatus::Queued,
        ToolRunStatus::Preparing,
        false,
    )?;
    transcript.push(ToolRunStatus::Preparing);
    if let Err(failure) = call.adapter.ensure_ready() {
        // 第 7 行：PREPARING → FAILED（握手/进程失败，INFRA）。
        engine.advance(
            call.run_id,
            ToolRunStatus::Preparing,
            ToolRunStatus::Failed,
            true,
        )?;
        transcript.push(ToolRunStatus::Failed);
        let envelope = engine.build_envelope(
            &row,
            ToolRunStatus::Failed,
            TerminalParts {
                result: None,
                error: Some(EnvelopeError {
                    category: "INFRA".to_string(),
                    code: json!("WORKER_HANDSHAKE_FAILED"),
                    message: failure.message,
                    retryable: true,
                    detail_ref: None,
                }),
                summary: Some("worker 握手失败（INFRA）".to_string()),
                artifacts: Vec::new(),
                truncated: false,
                cached: false,
                runtime_type: "python_worker".to_string(),
                runtime_node_id: None,
            },
            started.elapsed().as_millis() as u64,
        )?;
        return Ok(DriveOutcome {
            envelope,
            transcript,
        });
    }

    // 第 6 行：PREPARING → RUNNING（initialize 完成后 execute 送达）。
    engine.advance(
        call.run_id,
        ToolRunStatus::Preparing,
        ToolRunStatus::Running,
        false,
    )?;
    transcript.push(ToolRunStatus::Running);

    // D-TC-04：本驱动将转发协作式取消，先把取消请求时刻记入 run 行。
    if call.cancel_after.is_some() {
        engine.request_cancel(call.run_id)?;
    }

    // §4.2 映射：timeout_seconds×1000 → timeout_ms（[1, 600000] 域）。
    let budget = engine.resolved_budget(call.run_id)?;
    let timeout_ms = budget
        .timeout_seconds
        .saturating_mul(1000)
        .clamp(1, 600_000);
    // §4.2 映射：input → task（run_id 不进 task）；submit_key 直通 idempotency_key。
    // B2 薄包装工具先经结构化 Adapter 的请求侧整形（面向工具的 input →
    // {"args": [...]}，无规则工具恒等返回）；整形失败按 VALIDATION 拒绝。
    let outcome =
        match crate::adapter::structured_output::shape_task(&row.tool_id, &row.input_sanitized) {
            Err(e) => Ok(AdapterOutcome::Failed {
                category: "VALIDATION".to_string(),
                code: json!(-32602),
                message: format!("task shaping failed: {e}"),
                retryable: false,
            }),
            Ok(task_for_worker) => call.adapter.execute(
                &task_for_worker,
                timeout_ms,
                budget.max_output_bytes,
                row.submit_key.as_deref(),
                call.cancel_after,
            ),
        };
    // B2 响应侧结构化：薄包装原样 stdout → output_schema 形状（失败 EXECUTION）。
    let outcome = match outcome {
        Ok(AdapterOutcome::Succeeded {
            data,
            cached,
            usage,
        }) => {
            match crate::adapter::structured_output::adapt(
                &row.tool_id,
                &row.input_sanitized,
                &data,
            ) {
                Ok(v) => Ok(AdapterOutcome::Succeeded {
                    data: v,
                    cached,
                    usage,
                }),
                Err(e) => Ok(AdapterOutcome::Failed {
                    category: "EXECUTION".to_string(),
                    code: json!(-32006),
                    message: format!("structured output adaptation failed: {e}"),
                    retryable: false,
                }),
            }
        }
        other => other,
    };

    let mut artifacts: Vec<envelope::EnvelopeArtifact> = Vec::new();
    let mut truncated_flag = false;
    let cached;
    let (target, result, summary, error) = match outcome {
        Ok(AdapterOutcome::Succeeded {
            data, cached: hit, ..
        }) => {
            cached = hit;
            // §3.2 第 10 行：结果不合 output_schema → FAILED（EXECUTION）。
            let (_, output_schema) = engine
                .registry()
                .schemas(&row.tool_id, Some(&row.tool_version))?;
            if let Err(e) = tool_registry::jsonschema_lite::validate_instance(&output_schema, &data)
            {
                (
                    ToolRunStatus::Failed,
                    None,
                    Some(format!("结构化结果校验失败：{e}")),
                    Some(EnvelopeError {
                        category: "EXECUTION".to_string(),
                        code: json!(-32006),
                        message: format!("structured result failed output_schema: {e}"),
                        retryable: false,
                        detail_ref: None,
                    }),
                )
            } else {
                // §5.4 截断：与 Mock/容器侧共用同一函数。
                let out = envelope::truncate(call.run_id, &data, budget.max_output_bytes);
                truncated_flag = out.truncated;
                if truncated_flag {
                    let artifact_id = out.artifact_id.clone().unwrap_or_default();
                    let artifact = envelope::EnvelopeArtifact {
                        artifact_id: artifact_id.clone(),
                        role: "primary".to_string(),
                        uri: format!("worker://artifacts/{artifact_id}"),
                        sha256: out.original_sha256.clone().unwrap_or_default(),
                        size_bytes: out.original_bytes,
                        mime_type: "application/json".to_string(),
                        provenance: "python_worker 截断原始输出（TOOL-CONTRACT 5.4）".to_string(),
                    };
                    engine.insert_artifact(call.run_id, &artifact)?;
                    artifacts.push(artifact);
                }
                (
                    ToolRunStatus::Succeeded,
                    Some(out.result.clone()),
                    Some(format!(
                        "{} v{} 执行成功（worker_name {}）",
                        row.tool_id, row.tool_version, call.adapter.config.worker_name
                    )),
                    None,
                )
            }
        }
        Ok(AdapterOutcome::Failed {
            category,
            code,
            message,
            retryable,
        }) => {
            cached = false;
            // §3.2 第 12 行：worker 侧取消生效（error.code=-32004）映射
            // CANCELLED 终态；其余 error 归 FAILED。
            if category == "CANCELLED" {
                (
                    ToolRunStatus::Cancelled,
                    None,
                    None,
                    Some(EnvelopeError {
                        category,
                        code,
                        message,
                        retryable,
                        detail_ref: None,
                    }),
                )
            } else {
                (
                    ToolRunStatus::Failed,
                    None,
                    Some(format!("{category}: {message}")),
                    Some(EnvelopeError {
                        category,
                        code,
                        message,
                        retryable,
                        detail_ref: None,
                    }),
                )
            }
        }
        Ok(AdapterOutcome::TimedOut { message }) => {
            cached = false;
            (
                ToolRunStatus::TimedOut,
                None,
                None,
                Some(EnvelopeError {
                    category: "TIMEOUT".to_string(),
                    code: json!(-32002),
                    message,
                    retryable: false,
                    detail_ref: None,
                }),
            )
        }
        Ok(AdapterOutcome::Cancelled { message }) => {
            cached = false;
            (
                ToolRunStatus::Cancelled,
                None,
                None,
                Some(EnvelopeError {
                    category: "CANCELLED".to_string(),
                    code: json!(-32004),
                    message,
                    retryable: false,
                    detail_ref: None,
                }),
            )
        }
        Err(failure) => {
            // RUNNING 期传输层失败：INFRA（第 10 行平台侧归类）。
            cached = false;
            (
                ToolRunStatus::Failed,
                None,
                Some(format!("worker 传输失败：{}", failure.message)),
                Some(EnvelopeError {
                    category: "INFRA".to_string(),
                    code: json!("WORKER_TRANSPORT_FAILED"),
                    message: failure.message,
                    retryable: true,
                    detail_ref: None,
                }),
            )
        }
    };

    engine.advance(call.run_id, ToolRunStatus::Running, target, true)?;
    transcript.push(target);

    let elapsed_ms = started.elapsed().as_millis() as u64;
    let envelope = engine.build_envelope(
        &row,
        target,
        TerminalParts {
            result,
            error,
            summary,
            artifacts,
            truncated: truncated_flag,
            cached,
            runtime_type: "python_worker".to_string(),
            runtime_node_id: Some("host-cachyos".to_string()),
        },
        elapsed_ms,
    )?;
    Ok(DriveOutcome {
        envelope,
        transcript,
    })
}
