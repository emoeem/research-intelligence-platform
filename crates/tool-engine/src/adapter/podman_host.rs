//! podman_host Runtime Adapter（P2-5 批次，ROADMAP P2-5 本体；ADR §3 决定
//! 第 1 条过渡 Runner，docs/phase2/PHASE2-DESIGN.md §5.1）。
//!
//! 职责链（一次 [`execute_container_via_engine`] 调用）：
//! 1. PREPARING：镜像 digest 引用复核（`@sha256:<64hex>` + `podman image
//!    exists`，TOOL-CONTRACT §3.2 第 7 行前置）+ 工作区准备
//!    `workspace_root/<run_id>/{in,out,tmp}`（ADR §5 第 1 条，in/ 落
//!    request.json 供排障，out/ 为产物收集目录，tmp/ 用后即焚）；
//! 2. RUNNING：容器创建按计划 §5.4 约束清单逐条注入（PHASE2-DESIGN
//!    §5.1.2：非 root、只读根、独立任务目录、只挂任务 in/out、不挂
//!    socket、非 privileged、`--cap-drop ALL`、cpu/memory/pids 上限、
//!    `--network none`、`--rm` 用后即删、`--userns=keep-id` 产物属主
//!    对齐宿主用户，消解 ADR 待验证项 5）；stdin 一行 JSON 请求、stdout
//!    一行 JSON 结果（D-TC-03，与 worker 分帧一致）；
//! 3. 看门狗：超 budget 强制结束 → TIMED_OUT；协作式取消：轮询 tool_run
//!    行 cancel_requested_at（D-TC-04 语义，[`Engine::request_cancel`]
//!    写入）→ SIGTERM 优雅回收 → CANCELLED；
//! 4. 终局：out/ 全部产物 sha256/size 登记 artifact 表（部分产出照常
//!    登记，provenance 标注 partial，ADR §5 第 3 条），tmp/ 删除，容器
//!    `--rm` 自动回收（kill 路径补 `podman rm -f` 兜底）。
//!
//! 边界：宿主挂载白名单由构造保证（Adapter 只挂载 workspace_root 之下的
//! in/out 两个目录，不接受任意挂载参数，socket/家目录/敏感路径在命令
//! 构造层即不存在入口）；HTTP API 面属 P5-4，本批经 tool-engine 派发链
//! （tests/podman_host_e2e.rs）端到端验证。

use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use domain::ToolRunStatus;
use serde_json::{json, Value};
use tool_registry::hash::sha256_hex;

use crate::adapter::{AdapterFailure, AdapterOutcome};
use crate::envelope;
use crate::envelope::{EnvelopeArtifact, EnvelopeError};
use crate::RunRow;
use crate::{DriveOutcome, Engine, EngineError, TerminalParts};

/// Adapter 默认资源与行为配置（PHASE2-DESIGN §5.1.3 基线，可按工具族收紧；
/// 放宽须经 Tool Policy Resolver，本批不开放）。
#[derive(Debug, Clone)]
pub struct PodmanHostConfig {
    /// podman 可执行文件（默认 `podman`，不经 shell）。
    pub podman_bin: String,
    /// 运行时工作区根（`<root>/<run_id>/{in,out,tmp}`；默认
    /// `~/rip-runtime/workspaces`，测试注入临时目录）。
    pub workspace_root: PathBuf,
    /// 每容器 CPU 核数（基线 2，ADR §7）。
    pub cpus: f64,
    /// 每容器内存字节（基线 2 GiB，ADR §7）。
    pub memory_bytes: u64,
    /// PIDs 上限（基线 256，ADR §7）。
    pub pids_limit: i64,
    /// 任务 tmp tmpfs 封顶（基线 1g，ADR §7）。
    pub tmpfs_size: String,
    /// 环境指纹 runtime_node_id。
    pub node_id: String,
    /// 看门狗轮询间隔（进程内 DB 轮询粒度，非网络探针；默认 200ms）。
    pub cancel_poll: Duration,
    /// 每工具族并发默认上限（ADR §7「每工具族默认 1」；按族可收紧）。
    pub family_concurrency_default: usize,
}

impl Default for PodmanHostConfig {
    fn default() -> Self {
        Self {
            podman_bin: "podman".to_string(),
            workspace_root: default_workspace_root(),
            cpus: 2.0,
            memory_bytes: 2 * 1024 * 1024 * 1024,
            pids_limit: 256,
            tmpfs_size: "1g".to_string(),
            node_id: "host-cachyos".to_string(),
            cancel_poll: Duration::from_millis(200),
            family_concurrency_default: 1,
        }
    }
}

/// 默认工作区根（ADR §5：`~/rip-runtime/`）。
fn default_workspace_root() -> PathBuf {
    if let Some(home) = std::env::var_os("HOME") {
        return Path::new(&home).join("rip-runtime").join("workspaces");
    }
    std::env::temp_dir().join("rip-runtime").join("workspaces")
}

/// 每工具族并发闸（ADR §7「并发 | 每工具族默认 1」的执行面）。
///
/// 族槽位在容器进入 RUNNING 前占用，run 终局信封组装后随 Guard Drop 释放；
/// 同族超限的 run 在 PREPARING 停留等待（阻塞式 acquire），不伪装失败。
#[derive(Clone)]
pub struct FamilyLimiter {
    inner: Arc<LimiterInner>,
}

struct LimiterInner {
    slots: Mutex<HashMap<String, Slot>>,
    default_limit: usize,
}

struct Slot {
    active: usize,
    cv: Arc<Condvar>,
}

/// 族槽位占用凭据：Drop 即释放（超时/取消/失败四路径统一）。
pub struct FamilyGuard {
    inner: Arc<LimiterInner>,
    family: String,
}

impl Drop for FamilyGuard {
    fn drop(&mut self) {
        let mut slots = self.inner.slots.lock().expect("family limiter mutex");
        if let Some(slot) = slots.get_mut(&self.family) {
            slot.active = slot.active.saturating_sub(1);
            slot.cv.notify_one();
        }
    }
}

impl FamilyLimiter {
    /// 以默认族并发上限构造（ADR §7 基线 = 1）。
    pub fn new(default_limit: usize) -> Self {
        Self {
            inner: Arc::new(LimiterInner {
                slots: Mutex::new(HashMap::new()),
                default_limit: default_limit.max(1),
            }),
        }
    }

    /// 非阻塞占位：族内槽位已满返回 None（测试与探测面）。
    pub fn try_acquire(&self, family: &str) -> Option<FamilyGuard> {
        let mut slots = self.inner.slots.lock().expect("family limiter mutex");
        Self::try_locked(&self.inner, &mut slots, family)
    }

    /// 锁内尝试占位（try_acquire 与 acquire 共用的临界区；Guard 持同一
    /// LimiterInner 以保证 Drop 释放正确落位）。
    fn try_locked(
        inner: &Arc<LimiterInner>,
        slots: &mut HashMap<String, Slot>,
        family: &str,
    ) -> Option<FamilyGuard> {
        let slot = slots.entry(family.to_string()).or_insert_with(|| Slot {
            active: 0,
            cv: Arc::new(Condvar::new()),
        });
        if slot.active < inner.default_limit.max(1) {
            slot.active += 1;
            Some(FamilyGuard {
                inner: Arc::clone(inner),
                family: family.to_string(),
            })
        } else {
            None
        }
    }

    /// 阻塞占位：等待族内槽位空闲（生产路径）。等待在该族 Condvar 上进行，
    /// 500ms 超时回退防错过通知（Drop 侧 notify_one）。
    pub fn acquire(&self, family: &str) -> FamilyGuard {
        loop {
            let cv = {
                let mut slots = self.inner.slots.lock().expect("family limiter mutex");
                let slot = slots.entry(family.to_string()).or_insert_with(|| Slot {
                    active: 0,
                    cv: Arc::new(Condvar::new()),
                });
                if slot.active < self.inner.default_limit.max(1) {
                    slot.active += 1;
                    return FamilyGuard {
                        inner: Arc::clone(&self.inner),
                        family: family.to_string(),
                    };
                }
                Arc::clone(&slot.cv)
            };
            let mut slots = self.inner.slots.lock().expect("family limiter mutex");
            let (guard, _) = cv
                .wait_timeout(slots, Duration::from_millis(500))
                .expect("family limiter condvar");
            slots = guard;
        }
    }
}

/// podman_host Adapter。
#[derive(Clone)]
pub struct PodmanHostAdapter {
    config: PodmanHostConfig,
    limiter: FamilyLimiter,
}

/// 一次容器执行的工作区路径束。
#[derive(Debug, Clone)]
pub struct WorkspacePaths {
    /// `<root>/<run_id>`。
    pub root: PathBuf,
    /// 输入目录（容器内 :ro 挂载）。
    pub in_dir: PathBuf,
    /// 输出目录（容器内 :rw 挂载，产物收集来源）。
    pub out_dir: PathBuf,
    /// 临时目录（tmpfs 封顶挂载，宿主侧目录用后即焚）。
    pub tmp_dir: PathBuf,
}

/// 容器执行调用（含引擎接缝与 Manifest runtime 分支字段）。
pub struct ContainerCall<'a> {
    /// 引擎（同一测试库上的受理/状态机/信封面）。
    pub engine: &'a Engine,
    /// 已受理（QUEUED）的 run id。
    pub run_id: &'a str,
    /// podman_host Adapter。
    pub adapter: &'a PodmanHostAdapter,
    /// 镜像引用（`name@sha256:<64hex>`，来自 Manifest runtime.image）。
    pub image: &'a str,
    /// 入口命令（Manifest runtime.entrypoint）。
    pub entrypoint: &'a [String],
    /// 工具族（Manifest family；每工具族并发闸的键，ADR §7）。
    pub family: &'a str,
}

impl PodmanHostAdapter {
    /// 以给定配置构造（资源基线见 [`PodmanHostConfig::default`]）。
    pub fn new(config: PodmanHostConfig) -> Self {
        let limiter = FamilyLimiter::new(config.family_concurrency_default);
        Self { config, limiter }
    }

    /// 任务容器名（`rip-tool-` 前缀 + run_id 消毒；Gateway 清理面按
    /// `rip.runtime=rip-tool` label 与本前缀双过滤，不触碰用户容器）。
    pub fn container_name(&self, run_id: &str) -> String {
        let safe: String = run_id
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '-' {
                    c
                } else {
                    '-'
                }
            })
            .collect();
        let trimmed = safe.trim_matches('-');
        let short: String = trimmed.chars().take(40).collect();
        format!("rip-tool-{short}")
    }

    /// 镜像引用复核：必须 `@sha256:<64hex>` 结尾（Adapter 侧二道校验，
    /// Registry 九条第 4 条同规则；tag 引用与占位在登记面已拒绝）。
    pub fn image_ref_ok(image: &str) -> bool {
        match image.rsplit_once("@sha256:") {
            Some((_, digest)) => Self::digest_ok(digest),
            None => false,
        }
    }

    fn digest_ok(digest: &str) -> bool {
        digest.len() == 64
            && digest
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    }

    /// 准备工作区（mkdir in/out/tmp + 落 in/request.json）。
    pub fn prepare_workspace(
        &self,
        run_id: &str,
        input: &Value,
    ) -> Result<WorkspacePaths, AdapterFailure> {
        let root = self.config.workspace_root.join(run_id);
        let ws = WorkspacePaths {
            in_dir: root.join("in"),
            out_dir: root.join("out"),
            tmp_dir: root.join("tmp"),
            root,
        };
        std::fs::create_dir_all(&ws.in_dir)
            .and_then(|_| std::fs::create_dir_all(&ws.out_dir))
            .and_then(|_| std::fs::create_dir_all(&ws.tmp_dir))
            .map_err(|e| AdapterFailure::new(format!("workspace create failed: {e}")))?;
        let request = json!({
            "run_id": run_id,
            "input": input,
        });
        std::fs::write(ws.in_dir.join("request.json"), format!("{request}\n"))
            .map_err(|e| AdapterFailure::new(format!("in/request.json write failed: {e}")))?;
        Ok(ws)
    }

    /// tmp/ 用后即焚（成功、超时、取消、失败四路径统一调用）。
    pub fn cleanup_tmp(&self, ws: &WorkspacePaths) {
        let _ = std::fs::remove_dir_all(&ws.tmp_dir);
    }

    /// 容器兜底回收（`--rm` 正常路径无容器可删；kill 路径双保险）。
    pub fn cleanup_container(&self, name: &str) {
        let _ = Command::new(&self.config.podman_bin)
            .args(["rm", "-f", name])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }

    /// 约束复核面：`podman inspect` 全量 JSON（e2e 断言与 Gateway 复核用）。
    pub fn inspect(&self, name: &str) -> Result<Value, AdapterFailure> {
        let out = Command::new(&self.config.podman_bin)
            .args(["inspect", "--format", "{{json .}}", name])
            .output()
            .map_err(|e| AdapterFailure::new(format!("podman inspect spawn failed: {e}")))?;
        if !out.status.success() {
            return Err(AdapterFailure::new(format!(
                "podman inspect {name} failed: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            )));
        }
        serde_json::from_slice::<Value>(&out.stdout)
            .map_err(|e| AdapterFailure::new(format!("inspect json parse failed: {e}")))
    }

    fn image_exists(&self, image: &str) -> Result<bool, AdapterFailure> {
        let out = Command::new(&self.config.podman_bin)
            .args(["image", "exists", image])
            .output()
            .map_err(|e| AdapterFailure::new(format!("podman image exists spawn failed: {e}")))?;
        Ok(out.status.success())
    }

    /// §5.4 约束清单逐条注入的容器命令（计划 §5.4:304-317 + PHASE2-DESIGN
    /// §5.1.2 表；字段名与 podman 6.1.4 inspect 实测一致）。
    fn run_command(&self, ws: &WorkspacePaths, call: &ContainerCall, name: &str) -> Command {
        let mut cmd = Command::new(&self.config.podman_bin);
        cmd.args([
            "run",
            "--rm",
            // stdin 一行 JSON 请求（D-TC-03）：podman 默认不转发 stdin，须显式 -i。
            "-i",
            "--name",
            name,
            "--label",
            "rip.runtime=rip-tool",
            // 网络由执行配置决定（network.mode=none），工具不能任意改变。
            "--network",
            "none",
            // 根文件系统只读，写入目录单独挂载。
            "--read-only",
            // rootless + 产物属主对齐宿主用户（ADR §5 第 4 条）。
            "--userns=keep-id",
            // 关闭不必要 capabilities（实测 CapEff 归零）。
            "--cap-drop",
            "ALL",
            // 资源上限（ADR §7 基线）。
            "--pids-limit",
            &self.config.pids_limit.to_string(),
            "--cpus",
            &format!("{}", self.config.cpus),
            "--memory",
            &self.config.memory_bytes.to_string(),
            // 只挂任务目录（白名单由构造保证）：in 只读、out 可写。
            "-v",
            &format!("{}:/task/in:ro", ws.in_dir.display()),
            "-v",
            &format!("{}:/task/out:rw", ws.out_dir.display()),
            // 任务 tmp：tmpfs 封顶（不落宿主磁盘）。
            "--tmpfs",
            &format!("/tmp:rw,size={},nosuid,nodev", self.config.tmpfs_size),
            // 非 privileged：不传 --privileged 即默认关闭（inspect 复核）。
        ]);
        cmd.arg("--");
        cmd.arg(call.image);
        for arg in call.entrypoint {
            cmd.arg(arg);
        }
        cmd.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        cmd
    }

    /// 容器内执行（帧协议 D-TC-03）+ 看门狗（超时/协作取消）→ 中性终局。
    fn execute(
        &self,
        call: &ContainerCall,
        ws: &WorkspacePaths,
        timeout: Duration,
    ) -> Result<AdapterOutcome, AdapterFailure> {
        let name = self.container_name(call.run_id);
        let input = call
            .engine
            .load_row(call.run_id)
            .map_err(|e| AdapterFailure::new(format!("row load failed: {e}")))?
            .input_sanitized;
        let budget_frame = self.budget_frame(call.engine, call.run_id)?;
        let mut child = self
            .run_command(ws, call, &name)
            .spawn()
            .map_err(|e| AdapterFailure::new(format!("podman run spawn failed: {e}")))?;

        // stdin 一行 JSON 请求（D-TC-03）；写毕即关。
        if let Some(mut stdin) = child.stdin.take() {
            let request = json!({
                "run_id": call.run_id,
                "input": input,
                "budget": budget_frame,
            });
            let _ = writeln!(stdin, "{request}");
            let _ = stdin.flush();
            drop(stdin);
        }

        let deadline = Instant::now() + timeout;
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break Ok(status),
                Ok(None) => {
                    if Instant::now() >= deadline {
                        // 超时强制终止：TERM 优雅窗口 2s，随后容器级 KILL
                        // （PID1 不可忽略；只杀 client 会泄漏容器）。
                        self.stop_container(&name);
                        if !wait_child(&mut child, Duration::from_secs(2)) {
                            self.kill_container(&name);
                            let _ = child.wait();
                        }
                        break Err(WatchdogExit::Timeout);
                    }
                    if self.cancel_requested(call.engine, call.run_id)? {
                        // 协作式取消（D-TC-04）：SIGTERM 给工具收尾窗口，
                        // 3s 后容器级 KILL 兜底；run 终态 CANCELLED。
                        self.stop_container(&name);
                        if !wait_child(&mut child, Duration::from_secs(3)) {
                            self.kill_container(&name);
                            let _ = child.wait();
                        }
                        break Err(WatchdogExit::Cancelled);
                    }
                    std::thread::sleep(self.config.cancel_poll);
                }
                Err(e) => break Err(WatchdogExit::Wait(format!("wait failed: {e}"))),
            }
        };

        // 终局读取（容器已退出，管道读尽）。
        let mut stdout = String::new();
        if let Some(mut s) = child.stdout.take() {
            let _ = s.read_to_string(&mut stdout);
        }
        let mut stderr = String::new();
        if let Some(mut s) = child.stderr.take() {
            let _ = s.read_to_string(&mut stderr);
        }

        match status {
            Ok(exit) if exit.success() => {
                let first_line = stdout.lines().next().unwrap_or("");
                match serde_json::from_str::<Value>(first_line) {
                    Ok(frame) if frame.get("status").and_then(Value::as_str) == Some("result") => {
                        Ok(AdapterOutcome::Succeeded {
                            data: frame.get("data").cloned().unwrap_or(Value::Null),
                            cached: false,
                            usage: Some(json!({
                                "output_bytes": stdout.len(),
                            })),
                        })
                    }
                    Ok(frame) => Ok(AdapterOutcome::Failed {
                        category: "EXECUTION".to_string(),
                        code: json!(-32006),
                        message: format!(
                            "container frame status unexpected: {}",
                            frame
                                .get("status")
                                .and_then(Value::as_str)
                                .unwrap_or("<missing>")
                        ),
                        retryable: false,
                    }),
                    Err(e) => Ok(AdapterOutcome::Failed {
                        category: "EXECUTION".to_string(),
                        code: json!(-32006),
                        message: format!("container stdout frame parse failed: {e}"),
                        retryable: false,
                    }),
                }
            }
            Ok(exit) => Ok(AdapterOutcome::Failed {
                category: "EXECUTION".to_string(),
                code: json!(exit.code().unwrap_or(-1)),
                message: format!(
                    "container exited {}: {}",
                    exit.code().unwrap_or(-1),
                    tail(&stderr, 400)
                ),
                retryable: false,
            }),
            Err(WatchdogExit::Timeout) => Ok(AdapterOutcome::TimedOut {
                message: format!(
                    "container watchdog fired after {}ms (budget.timeout_seconds)",
                    timeout.as_millis()
                ),
            }),
            Err(WatchdogExit::Cancelled) => Ok(AdapterOutcome::Cancelled {
                message: "cooperative cancel observed via cancel_requested_at (D-TC-04)"
                    .to_string(),
            }),
            Err(WatchdogExit::Wait(reason)) => Err(AdapterFailure::new(reason)),
        }
    }

    /// 优雅停止（SIGTERM）：协作式取消的默认信号；容器 PID1 若未装
    /// 处理器会按默认忽略语义放行（实测），须配合 [`Self::kill_container`]。
    fn stop_container(&self, name: &str) {
        let _ = Command::new(&self.config.podman_bin)
            .args(["kill", "--signal", "TERM", name])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }

    /// 容器级 SIGKILL（PID1 不可忽略）：优雅窗口到期后的强制兜底，
    /// 超时与取消两路径共用。
    fn kill_container(&self, name: &str) {
        let _ = Command::new(&self.config.podman_bin)
            .args(["kill", "--signal", "KILL", name])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }

    fn cancel_requested(&self, engine: &Engine, run_id: &str) -> Result<bool, AdapterFailure> {
        // cancel_requested_at 可空列：mapper 显式 Option，NULL 即未请求取消。
        let value: Option<String> = engine
            .pool()
            .with_conn(|c| {
                c.query_row(
                    "SELECT cancel_requested_at FROM tool_run WHERE run_id = ?1",
                    [run_id],
                    |r| r.get::<_, Option<String>>(0),
                )
            })
            .map_err(|e| AdapterFailure::new(format!("cancel poll failed: {e}")))?;
        Ok(value.is_some())
    }

    fn budget_frame(&self, engine: &Engine, run_id: &str) -> Result<Value, AdapterFailure> {
        let budget = engine
            .resolved_budget(run_id)
            .map_err(|e| AdapterFailure::new(format!("budget resolve failed: {e}")))?;
        Ok(json!({
            "timeout_seconds": budget.timeout_seconds,
            "max_output_bytes": budget.max_output_bytes,
        }))
    }
}

/// 有界等待容器退出（try_wait 轮询）；到期未退返回 false 供 KILL 兜底。
fn wait_child(child: &mut std::process::Child, grace: Duration) -> bool {
    let deadline = Instant::now() + grace;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => return true,
            Ok(None) if Instant::now() >= deadline => return false,
            Ok(None) => std::thread::sleep(Duration::from_millis(50)),
            Err(_) => return true,
        }
    }
}

enum WatchdogExit {
    Timeout,
    Cancelled,
    Wait(String),
}

fn tail(text: &str, max: usize) -> String {
    let trimmed = text.trim();
    if trimmed.len() <= max {
        trimmed.to_string()
    } else {
        trimmed[trimmed.len() - max..].to_string()
    }
}

/// out/ 产物收集与登记（成功、超时、取消、失败四路径统一；部分产出
/// provenance 标注 partial，ADR §5 第 3 条）。
fn collect_out_artifacts(
    engine: &Engine,
    run_id: &str,
    ws: &WorkspacePaths,
    partial: bool,
) -> Result<Vec<EnvelopeArtifact>, EngineError> {
    let mut out = Vec::new();
    let mut entries: Vec<PathBuf> = std::fs::read_dir(&ws.out_dir)
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| p.is_file())
                .collect()
        })
        .unwrap_or_default();
    entries.sort();
    for (idx, path) in entries.iter().enumerate() {
        let bytes = std::fs::read(path).map_err(|e| EngineError::Storage(e.to_string()))?;
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("artifact")
            .to_string();
        let mime = match path.extension().and_then(|e| e.to_str()) {
            Some("json") => "application/json",
            Some("txt") | Some("log") => "text/plain",
            _ => "application/octet-stream",
        };
        let artifact = EnvelopeArtifact {
            artifact_id: format!("art-{run_id}-{idx}-{name}"),
            role: if idx == 0 { "primary" } else { "attachment" }.to_string(),
            uri: format!("file://{}", path.display()),
            sha256: sha256_hex(&bytes),
            size_bytes: bytes.len() as u64,
            mime_type: mime.to_string(),
            provenance: if partial {
                format!("podman_host out/ 收集（partial：run 未正常完成，run_id={run_id}）")
            } else {
                format!("podman_host out/ 收集（run_id={run_id}）")
            },
        };
        engine.insert_artifact(run_id, &artifact)?;
        out.push(artifact);
    }
    Ok(out)
}

/// 端到端：QUEUED run 经 podman_host 容器执行并交付合法 ToolRunResult 信封
/// （P2-5 判据：契约测试载具经 tool-engine 派发链发起；状态跃迁与 tool_run
/// 行同步复用引擎内部机制）。
pub fn execute_container_via_engine(call: ContainerCall<'_>) -> Result<DriveOutcome, EngineError> {
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

    // 第 2 行：QUEUED → PREPARING（镜像复核与工作区准备，§3.1 语义）。
    engine.advance(
        call.run_id,
        ToolRunStatus::Queued,
        ToolRunStatus::Preparing,
        false,
    )?;
    transcript.push(ToolRunStatus::Preparing);

    // PREPARING：digest 引用二道校验 + 镜像在位（第 7 行：镜像缺失/digest 不符）。
    if !PodmanHostAdapter::image_ref_ok(call.image) {
        return preparing_failed(
            engine,
            call.run_id,
            &row,
            started,
            transcript,
            "DEPENDENCY",
            format!(
                "image ref {:?} is not digest-pinned @sha256:<64hex>",
                call.image
            ),
            &call.adapter.config.node_id,
        );
    }
    if !call
        .adapter
        .image_exists(call.image)
        .map_err(|f| EngineError::Storage(f.message))?
    {
        return preparing_failed(
            engine,
            call.run_id,
            &row,
            started,
            transcript,
            "DEPENDENCY",
            format!("image {:?} not present in local podman store", call.image),
            &call.adapter.config.node_id,
        );
    }
    let ws = call
        .adapter
        .prepare_workspace(call.run_id, &row.input_sanitized)
        .map_err(|f| EngineError::Storage(f.message))?;

    // 每工具族并发默认 1（ADR §7）：RUNNING 前占族内槽位，Guard 随本函数
    // 返回释放（超时/取消/失败路径统一）。
    let _family_guard = call.adapter.limiter.acquire(call.family);

    // 第 6 行：PREPARING → RUNNING（容器创建）。
    engine.advance(
        call.run_id,
        ToolRunStatus::Preparing,
        ToolRunStatus::Running,
        false,
    )?;
    transcript.push(ToolRunStatus::Running);

    // 看门狗预算：budget.timeout_seconds（[1, 600000] 域，与 worker 路径同帽）。
    let budget = engine.resolved_budget(call.run_id)?;
    let timeout = Duration::from_millis(
        budget
            .timeout_seconds
            .saturating_mul(1000)
            .clamp(1, 600_000),
    );

    let outcome = call.adapter.execute(&call, &ws, timeout);

    // 容器回收与 tmp 清理（四路径统一；ADR §6 超时取消回收容器并清理 tmp）。
    let partial = !matches!(outcome, Ok(AdapterOutcome::Succeeded { .. }));
    call.adapter.cleanup_tmp(&ws);
    if partial {
        call.adapter
            .cleanup_container(&call.adapter.container_name(call.run_id));
    }

    let mut artifacts: Vec<EnvelopeArtifact> = Vec::new();
    let mut truncated_flag = false;
    let cached;
    let (target, result, summary, error) = match outcome {
        Ok(AdapterOutcome::Succeeded {
            data,
            cached: hit,
            usage: _,
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
                // out/ 产物登记（sha256 与 artifact 表对账载体）。
                artifacts = collect_out_artifacts(engine, call.run_id, &ws, false)?;
                // §5.4 截断：与 Mock/worker 侧共用同一函数。
                let out = envelope::truncate(call.run_id, &data, budget.max_output_bytes);
                truncated_flag = out.truncated;
                if truncated_flag {
                    let artifact_id = out.artifact_id.clone().unwrap_or_default();
                    let artifact = EnvelopeArtifact {
                        artifact_id: artifact_id.clone(),
                        role: "primary".to_string(),
                        uri: format!("container://artifacts/{artifact_id}"),
                        sha256: out.original_sha256.clone().unwrap_or_default(),
                        size_bytes: out.original_bytes,
                        mime_type: "application/json".to_string(),
                        provenance: "podman_host 截断原始输出（TOOL-CONTRACT 5.4）".to_string(),
                    };
                    engine.insert_artifact(call.run_id, &artifact)?;
                    artifacts.push(artifact);
                }
                (
                    ToolRunStatus::Succeeded,
                    Some(out.result.clone()),
                    Some(format!(
                        "{} v{} 容器执行成功（node {}）",
                        row.tool_id, row.tool_version, call.adapter.config.node_id
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
            artifacts = collect_out_artifacts(engine, call.run_id, &ws, true)?;
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
        Ok(AdapterOutcome::TimedOut { message }) => {
            cached = false;
            artifacts = collect_out_artifacts(engine, call.run_id, &ws, true)?;
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
            artifacts = collect_out_artifacts(engine, call.run_id, &ws, true)?;
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
                Some(format!("容器执行失败：{}", failure.message)),
                Some(EnvelopeError {
                    category: "INFRA".to_string(),
                    code: json!("CONTAINER_TRANSPORT_FAILED"),
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
            runtime_type: "podman_host".to_string(),
            runtime_node_id: Some(call.adapter.config.node_id.clone()),
        },
        elapsed_ms,
    )?;
    Ok(DriveOutcome {
        envelope,
        transcript,
    })
}

/// 第 7 行：PREPARING → FAILED（镜像缺失/digest 不符等准备期失败）。
#[allow(clippy::too_many_arguments)]
fn preparing_failed(
    engine: &Engine,
    run_id: &str,
    row: &RunRow,
    started: Instant,
    mut transcript: Vec<ToolRunStatus>,
    category: &str,
    message: String,
    node_id: &str,
) -> Result<DriveOutcome, EngineError> {
    engine.advance(
        run_id,
        ToolRunStatus::Preparing,
        ToolRunStatus::Failed,
        true,
    )?;
    transcript.push(ToolRunStatus::Failed);
    let envelope = engine.build_envelope(
        row,
        ToolRunStatus::Failed,
        TerminalParts {
            result: None,
            error: Some(EnvelopeError {
                category: category.to_string(),
                code: json!("PREPARING_FAILED"),
                message,
                retryable: false,
                detail_ref: None,
            }),
            summary: Some("容器准备期失败（镜像/工作区，DEPENDENCY）".to_string()),
            artifacts: Vec::new(),
            truncated: false,
            cached: false,
            runtime_type: "podman_host".to_string(),
            runtime_node_id: Some(node_id.to_string()),
        },
        started.elapsed().as_millis() as u64,
    )?;
    Ok(DriveOutcome {
        envelope,
        transcript,
    })
}
