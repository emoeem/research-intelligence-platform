//! native_rust Runtime Adapter（RT-1 主体；RT-1-ADR D3-D8，RT-1-TASK S4/S5）。
//!
//! 职责链（一次 [`execute_native_rust_via_engine`] 调用）：
//! 1. PREPARING：Manifest `secrets` 非空拒绝（D4，native 档本批不支持 secret
//!    注入）→ allowlist 解析（D3：binary 绝对路径 + 64hex 钉值；subcommands
//!    白名单；argv_template 槽位）→ 可执行文件在位/执行位/sha256 实测对照
//!    （missing → DEPENDENCY，identity 不符 → POLICY_DENIED，D7 五态映射）
//!    → wrapper 双钉（binary_target_sha256 在场时 fail-closed 解析 exec 目标，
//!    C §5.1:270）→ capability_ref 二道校验（F §6：fresh+available 才放行）
//!    → 工作区三目录 `<root>/<run_id>/{in,out,tmp}`（D8.3，podman_host 同形）；
//! 2. RUNNING：`std::process::Command` + `process_group(0)`（D5，进程组口径
//!    覆盖孙进程）+ `env_clear()` 显式白名单（D4：PATH=钉值二进制所在目录
//!    最小集 / TMPDIR=任务 tmp / LANG=C.UTF-8 / Manifest runtime.env；凭据
//!    类变量结构性不存在）+ 资源强制（D8.4：systemd-run --user --scope 优先，
//!    prlimit 兜底，两机制不可用即拒绝派发不裸跑，E §2 注 A）+ stdin 一行
//!    JSON 请求（run_id 不透传工具，D8.1）+ stdout 一行 JSON 结果帧
//!    （`{"status":"result","data":...}`，D-TC-03 同款）；
//! 3. 看门狗：try_wait 轮询 deadline（podman_host.rs:483-512 同范式，零
//!    tokio）→ killpg TERM → 优雅窗口 → killpg KILL（std 无 killpg，经
//!    固定路径 `/usr/bin/kill` 以负 pgid 投递——Adapter 自有基础设施面，
//!    非 Manifest 来源）；协作取消轮询 tool_run.cancel_requested_at
//!    （D-TC-04，podman_host.rs:595-608 同款 SQL）→ CANCELLED 终态；
//! 4. 输出帽双层（D8.2）：读侧流式帽（`max_output_bytes` + 64 KiB 帧封装
//!    容差）触顶即 killpg，防大输出整读 OOM；结果侧 `envelope::truncate`
//!    降级 + 原始字节落 Artifact + usage.truncated=true（podman_host.rs:902-917
//!    同款）；stderr 只采尾部 400 字节入错误消息，全量 stderr 落 out/
//!    stderr.log 经 Artifact（role=stderr）登记；
//! 5. 终局：output_schema 校验 → envelope::truncate → build_envelope
//!    （runtime_type="native_rust"），成功/失败/超时/取消四路径信封统一。
//!
//! 红线（RT-1-TASK §7）：零 Shell（`sh -c`/`bash -c` 在代码层无构造入口，
//! argv 只由 Manifest 模板 + input_schema 校验后参数构成）；零 tokio（std
//! 线程 + mpsc/轮询）；不建第二事实源（ToolRun/Artifact/预算/取消语义全部
//! 复用引擎既有面；错误信封只用 §5.3 九类）。

use std::io::Read;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use domain::ToolRunStatus;
use serde_json::{json, Value};
use tool_registry::hash::sha256_hex;

use crate::adapter::{AdapterFailure, AdapterOutcome};
use crate::capability::{CapabilityId, CapabilityProvider, Freshness};
use crate::envelope::{EnvelopeArtifact, EnvelopeError};
use crate::{DriveOutcome, Engine, EngineError, RunRow, TerminalParts};

/// 读侧流式帽的帧封装容差（结果侧真帽 = `max_output_bytes`，由
/// envelope::truncate 强制；读侧帽放宽 64 KiB 让合法帧完整到达，使截断
/// 语义可达——若读侧帽与结果帽相等，超帽帧会在封装开销处被杀，截断路径
/// 变成死代码，D8.2(a)(b) 双层语义失效）。
const READ_CAP_SLACK_BYTES: u64 = 64 * 1024;

/// stdout 读线程的单块大小（逐块计数帽，防整读 OOM）。
const READ_CHUNK_BYTES: usize = 8 * 1024;

/// stderr 尾部采集长度（podman_host.rs:640-647 tail 同款）。
const STDERR_TAIL_BYTES: usize = 400;

/// killpg 优雅窗口：超时路径 2s（D5）。
const KILL_GRACE_TIMEOUT: Duration = Duration::from_secs(2);

/// killpg 优雅窗口：取消路径 3s（D6）。
const KILL_GRACE_CANCEL: Duration = Duration::from_secs(3);

/// 结果面双模（DI-1-ADR D4，唯一 additive 增量）：`frame` = RT-1 既有帧协议
/// （stdout 首行 `{"status":"result",...}`，缺省，行为零改动）；`file` =
/// raw CLI 形态（pdftotext/pdfinfo/soffice/magick 无一可发帧）：exit 0 →
/// Succeeded，data = 产物收集面（exit_code/stdout_bytes/stdout_capped/artifacts），
/// 不做帧解析；stdout 读侧帽语义不变（超帽 → `NATIVE_OUTPUT_CAP_EXCEEDED`
/// FAILED，不静默丢字节——与帧模式同语义）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ResultMode {
    /// stdout 首行 JSON 结果帧（RT-1 D-TC-03 既有；缺省）。
    #[default]
    Frame,
    /// exit 0 + 产物收集 data（DI-1 ADR D4；stdout 留档 out/stdout.log）。
    File,
}

impl ResultMode {
    /// Manifest 字面量解析（缺省 frame；非法字面量 PREPARING 拒绝）。专属码
    /// `NATIVE_RESULT_MODE_INVALID`（评审修订 2026-11-19：此前借
    /// NATIVE_ARGV_TEMPLATE_DENIED——问题在 result_mode 枚举值非 argv 模板，
    /// 码表内借码污染按 code 检索的审计面；信封 error.code 为自由面，
    /// category 仍 POLICY_DENIED，schema/validate 词表零影响）。
    fn parse(value: Option<&str>) -> Result<Self, PreparingReject> {
        match value {
            None | Some("frame") => Ok(Self::Frame),
            Some("file") => Ok(Self::File),
            Some(other) => Err(PreparingReject::policy(
                "NATIVE_RESULT_MODE_INVALID",
                format!(
                    "runtime.result_mode {other:?} must be \"frame\" or \"file\" (DI-1 ADR D4)"
                ),
            )),
        }
    }
}

/// 资源强制机制（D8.4：systemd-run 优先 → prlimit 兜底 → 两机制不可用即
/// 拒绝派发不裸跑）。探测与派发同环境口径（D4 清环境最小集，见
/// [`NativeRustAdapter::probe_exec`]）。`Off` 仅限测试载具显式声明（生产默认
/// [`ResourceEnforcement::Auto`]，显式 Off 属配置审查面，不改变"机制不可用
/// 即拒绝"的 Auto 行为）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceEnforcement {
    /// 清环境口径下探测 systemd 用户会话可用则包裹派发，否则探测 prlimit，
    /// 均不可用拒绝（会话宿主上派发面 env 无会话变量时自然落 prlimit 兜底）。
    Auto,
    /// 强制 systemd-run --user --scope 包裹（不可用即拒绝）。
    SystemdRun,
    /// 强制 prlimit（RLIMIT_CPU/RLIMIT_AS）包裹（不可用即拒绝）。
    Prlimit,
    /// 不包裹（仅测试载具显式声明；生产禁用）。
    Off,
}

/// native_rust Adapter 配置。
#[derive(Debug, Clone)]
pub struct NativeRustConfig {
    /// 运行时工作区根（`<root>/<run_id>/{in,out,tmp}`；默认
    /// `~/rip-runtime/workspaces`，测试注入临时目录）。
    pub workspace_root: PathBuf,
    /// 环境指纹 runtime_node_id。
    pub node_id: String,
    /// 看门狗轮询间隔（进程内 DB 轮询粒度；默认 200ms，podman 同款）。
    pub cancel_poll: Duration,
    /// 资源强制机制（默认 Auto；D8.4）。
    pub resource_enforcement: ResourceEnforcement,
    /// 进程组信号投递用 kill 工具（std 无 killpg、零依赖约束下的最小解；
    /// Adapter 自有基础设施固定值，非 Manifest 来源）。
    pub kill_bin: String,
    /// systemd-run 绝对路径（Auto/SystemdRun 探测与包裹用）。
    pub systemd_run_bin: String,
    /// prlimit 绝对路径（Auto/Prlimit 探测与包裹用）。
    pub prlimit_bin: String,
}

impl Default for NativeRustConfig {
    fn default() -> Self {
        Self {
            workspace_root: default_workspace_root(),
            node_id: "host-cachyos".to_string(),
            cancel_poll: Duration::from_millis(200),
            resource_enforcement: ResourceEnforcement::Auto,
            kill_bin: "/usr/bin/kill".to_string(),
            systemd_run_bin: "/usr/bin/systemd-run".to_string(),
            prlimit_bin: "/usr/bin/prlimit".to_string(),
        }
    }
}

/// 默认工作区根（ADR §5：`~/rip-runtime/`；podman_host 同款）。
fn default_workspace_root() -> PathBuf {
    if let Some(home) = std::env::var_os("HOME") {
        return Path::new(&home).join("rip-runtime").join("workspaces");
    }
    std::env::temp_dir().join("rip-runtime").join("workspaces")
}

/// 一次原生执行的工作区路径束（podman_host `WorkspacePaths` 同形状）。
#[derive(Debug, Clone)]
pub struct NativeWorkspace {
    /// `<root>/<run_id>`。
    pub root: PathBuf,
    /// 输入目录（`{{input_path}}` 槽位的前缀边界，只读语义由前缀校验承载）。
    pub in_dir: PathBuf,
    /// 输出目录（产物收集来源）。
    pub out_dir: PathBuf,
    /// 临时目录（TMPDIR 指向；用后即焚）。
    pub tmp_dir: PathBuf,
}

/// PREPARING 拒绝载荷（D7 五态映射的信封 error 字段来源）。
#[derive(Debug, Clone)]
struct PreparingReject {
    category: &'static str,
    code: Value,
    message: String,
    retryable: bool,
}

impl PreparingReject {
    fn policy(code: &str, message: impl Into<String>) -> Self {
        Self {
            category: "POLICY_DENIED",
            code: json!(code),
            message: message.into(),
            retryable: false,
        }
    }

    fn dependency(code: &str, message: impl Into<String>) -> Self {
        Self {
            category: "DEPENDENCY",
            code: json!(code),
            message: message.into(),
            retryable: false,
        }
    }

    fn infra(message: impl Into<String>) -> Self {
        Self {
            category: "INFRA",
            code: json!("NATIVE_PROBE_FAILED"),
            message: message.into(),
            retryable: true,
        }
    }
}

/// allowlist 解析产物（D3 两形态的统一内存表示）。
#[derive(Debug, Clone)]
pub struct NativeAllowlist {
    /// 可执行文件绝对路径（唯一来源 = Manifest 固定值）。
    pub binary: PathBuf,
    /// 64hex 全量钉值（执行面必填）。
    pub binary_sha256: String,
    /// wrapper exec 目标钉值（仅 wrapper 形态；C §5.1:270）。
    pub binary_target_sha256: Option<String>,
    /// Manifest 固定 subcommand（旧单值字段；无白名单时使用）。
    pub subcommand_fixed: Option<String>,
    /// 子命令/旗标白名单（生效时 input.subcommand 为唯一子命令来源）。
    pub subcommands: Option<Vec<String>>,
    /// 参数模板（`{{slot}}` 由 input_schema 校验后参数填充；`{{input_path}}`
    /// 前缀校验；`{{out_dir}}`/`{{tmp_dir}}` Adapter 保留槽位）。
    pub argv_template: Vec<String>,
    /// Capability 目录引用（派发前查目录，F §6）。
    pub capability_ref: Option<CapabilityId>,
    /// Manifest runtime.env 显式登记（键值字符串；D4 白名单追加面）。
    pub env: Vec<(String, String)>,
    /// 结果面双模（DI-1 ADR D4；缺省 frame，既有工具零改动）。
    pub result_mode: ResultMode,
}

/// argv 构造产物（type_complexity 拆分面）。
struct BuiltArgv {
    /// 构造完成的完整 argv（[binary, ...]）。
    argv: Vec<String>,
    /// `{{input_path}}` + input_content 暂存（RUNNING 期写入 in/）。
    staged: Option<(PathBuf, String)>,
}

/// 资源强制包裹机制解析结果（PREPARING 期探测定案，RUNNING 期直接构造）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ResolvedWrapper {
    /// Off：测试载具显式不包裹。
    Unconfined,
    /// systemd-run --user --scope 包裹（D8.4 优先档）。
    SystemdRun,
    /// prlimit（RLIMIT_CPU/RLIMIT_AS）兜底。
    Prlimit,
}

/// PREPARING 全链产物（S5 第 2 步输出）。
struct PreparedDispatch {
    /// allowlist（执行期 env 白名单来源）。
    allowlist: NativeAllowlist,
    /// 工作区三目录。
    ws: NativeWorkspace,
    /// 构造完成的完整 argv（[binary, ...]）。
    argv: Vec<String>,
    /// `{{input_path}}` + input_content 暂存（RUNNING 期写入 in/）。
    staged: Option<(PathBuf, String)>,
    /// Manifest resources.cpu（资源强制参数）。
    cpu: Option<String>,
    /// Manifest resources.memory（资源强制参数）。
    memory: Option<String>,
    /// 资源强制机制（PREPARING 期探测定案；D8.4 fail-closed）。
    wrapper: ResolvedWrapper,
}

/// native_rust Adapter。
#[derive(Clone)]
pub struct NativeRustAdapter {
    config: NativeRustConfig,
}

impl NativeRustAdapter {
    /// 以给定配置构造。
    pub fn new(config: NativeRustConfig) -> Self {
        Self { config }
    }

    /// 准备工作区三目录 + 落 in/request.json（排障审计；D8.3）。
    fn prepare_workspace(
        &self,
        run_id: &str,
        input: &Value,
    ) -> Result<NativeWorkspace, AdapterFailure> {
        let root = self.config.workspace_root.join(run_id);
        let ws = NativeWorkspace {
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

    /// tmp/ 用后即焚（四路径统一）。
    fn cleanup_tmp(&self, ws: &NativeWorkspace) {
        let _ = std::fs::remove_dir_all(&ws.tmp_dir);
    }

    // -----------------------------------------------------------------
    // allowlist 解析与派发前置校验（PREPARING；零子进程拉起）
    // -----------------------------------------------------------------

    /// allowlist 解析（D3 两形态；执行面强制）：
    /// - binary 必须绝对路径（`/` 前缀；PATH 名称解析拒绝）；
    /// - binary_sha256 必填 64hex（缺钉值拒绝）；
    /// - subcommand/subcommands/argv_template/env/capability_ref 按域校验。
    fn parse_allowlist(runtime: &Value) -> Result<NativeAllowlist, PreparingReject> {
        let rtype = runtime.get("type").and_then(Value::as_str).unwrap_or("");
        if rtype != "native_rust" {
            return Err(PreparingReject::policy(
                "NATIVE_ARGV_TEMPLATE_DENIED",
                format!("runtime.type {rtype:?} is not native_rust"),
            ));
        }
        let binary = runtime
            .get("binary")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                PreparingReject::policy(
                    "NATIVE_ARGV_TEMPLATE_DENIED",
                    "runtime.binary must be a string",
                )
            })?;
        // 可执行路径唯一来源 = Manifest 固定绝对值（D3 否决 PATH 名称解析）。
        if !binary.starts_with('/') {
            return Err(PreparingReject::policy(
                "NATIVE_ARGV_TEMPLATE_DENIED",
                format!(
                    "runtime.binary {binary:?} must be an absolute path (`/` prefix); PATH name resolution is rejected (RT-1 ADR D3)"
                ),
            ));
        }
        let binary_sha256 = runtime
            .get("binary_sha256")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                PreparingReject::policy(
                    "NATIVE_IDENTITY_UNPINNED",
                    "runtime.binary_sha256 is required at dispatch (identity pin, RT-1 ADR D3)",
                )
            })?;
        if !sha256_digest_ok(binary_sha256) {
            return Err(PreparingReject::policy(
                "NATIVE_IDENTITY_UNPINNED",
                format!("runtime.binary_sha256 {binary_sha256:?} must be 64 lowercase hex"),
            ));
        }
        let binary_target_sha256 = match runtime.get("binary_target_sha256") {
            None => None,
            Some(v) => {
                let s = v.as_str().ok_or_else(|| {
                    PreparingReject::policy(
                        "NATIVE_IDENTITY_UNPINNED",
                        "runtime.binary_target_sha256 must be a string",
                    )
                })?;
                if !sha256_digest_ok(s) {
                    return Err(PreparingReject::policy(
                        "NATIVE_IDENTITY_UNPINNED",
                        format!("runtime.binary_target_sha256 {s:?} must be 64 lowercase hex"),
                    ));
                }
                Some(s.to_string())
            }
        };
        let subcommand_fixed = match runtime.get("subcommand") {
            None => None,
            Some(v) => {
                let s = v.as_str().ok_or_else(|| {
                    PreparingReject::policy(
                        "NATIVE_ARGV_TEMPLATE_DENIED",
                        "runtime.subcommand must be a string",
                    )
                })?;
                if s.is_empty() {
                    return Err(PreparingReject::policy(
                        "NATIVE_ARGV_TEMPLATE_DENIED",
                        "runtime.subcommand must be non-empty",
                    ));
                }
                Some(s.to_string())
            }
        };
        let subcommands = match runtime.get("subcommands") {
            None => None,
            Some(v) => {
                let arr = v.as_array().ok_or_else(|| {
                    PreparingReject::policy(
                        "NATIVE_SUBCOMMAND_NOT_ALLOWED",
                        "runtime.subcommands must be an array",
                    )
                })?;
                let mut out = Vec::with_capacity(arr.len());
                for item in arr {
                    let s = item.as_str().ok_or_else(|| {
                        PreparingReject::policy(
                            "NATIVE_SUBCOMMAND_NOT_ALLOWED",
                            "runtime.subcommands items must be strings",
                        )
                    })?;
                    if s.is_empty() {
                        return Err(PreparingReject::policy(
                            "NATIVE_SUBCOMMAND_NOT_ALLOWED",
                            "runtime.subcommands items must be non-empty",
                        ));
                    }
                    out.push(s.to_string());
                }
                if out.is_empty() {
                    return Err(PreparingReject::policy(
                        "NATIVE_SUBCOMMAND_NOT_ALLOWED",
                        "runtime.subcommands must be non-empty",
                    ));
                }
                Some(out)
            }
        };
        let argv_template = match runtime.get("argv_template") {
            None => Vec::new(),
            Some(v) => {
                let arr = v.as_array().ok_or_else(|| {
                    PreparingReject::policy(
                        "NATIVE_ARGV_TEMPLATE_DENIED",
                        "runtime.argv_template must be an array",
                    )
                })?;
                let mut out = Vec::with_capacity(arr.len());
                for item in arr {
                    let s = item.as_str().ok_or_else(|| {
                        PreparingReject::policy(
                            "NATIVE_ARGV_TEMPLATE_DENIED",
                            "runtime.argv_template items must be strings",
                        )
                    })?;
                    if s.contains('\0') || s.contains('\n') || s.contains('\r') {
                        return Err(PreparingReject::policy(
                            "NATIVE_ARGV_TEMPLATE_DENIED",
                            format!("argv token {s:?} carries NUL/newline"),
                        ));
                    }
                    out.push(s.to_string());
                }
                out
            }
        };
        let capability_ref = match runtime.get("capability_ref") {
            None => None,
            Some(v) => {
                let s = v.as_str().ok_or_else(|| {
                    PreparingReject::policy(
                        "NATIVE_ARGV_TEMPLATE_DENIED",
                        "runtime.capability_ref must be a string",
                    )
                })?;
                Some(
                    CapabilityId::new(s)
                        .map_err(|e| PreparingReject::policy("NATIVE_ARGV_TEMPLATE_DENIED", e))?,
                )
            }
        };
        let env = match runtime.get("env") {
            None => Vec::new(),
            Some(v) => {
                let obj = v.as_object().ok_or_else(|| {
                    PreparingReject::policy(
                        "NATIVE_ARGV_TEMPLATE_DENIED",
                        "runtime.env must be an object",
                    )
                })?;
                let mut out = Vec::with_capacity(obj.len());
                // D4 保留键语义（评审修订）：PATH/TMPDIR/LANG 是 Adapter 结构性
                // 白名单（PATH=钉值二进制所在目录最小集），Manifest env 不得覆盖
                // ——否则 {"PATH": ...} 可使工具按名解析任意目录下的辅助二进制，
                // 撞穿 D4 的结构性保证。冲突一律 PREPARING 拒绝（fail-closed）。
                const RESERVED_ENV_KEYS: [&str; 3] = ["PATH", "TMPDIR", "LANG"];
                for (k, val) in obj {
                    let s = val.as_str().ok_or_else(|| {
                        PreparingReject::policy(
                            "NATIVE_ARGV_TEMPLATE_DENIED",
                            format!("runtime.env[{k:?}] value must be a string"),
                        )
                    })?;
                    if RESERVED_ENV_KEYS.contains(&k.as_str()) {
                        return Err(PreparingReject::policy(
                            "NATIVE_ENV_RESERVED_CONFLICT",
                            format!(
                                "runtime.env key {k:?} conflicts with the D4 reserved whitelist (PATH/TMPDIR/LANG are Adapter-set structural values); reserved keys cannot be overridden via Manifest env"
                            ),
                        ));
                    }
                    out.push((k.clone(), s.to_string()));
                }
                out
            }
        };
        let result_mode = ResultMode::parse(runtime.get("result_mode").and_then(Value::as_str))?;
        Ok(NativeAllowlist {
            binary: PathBuf::from(binary),
            binary_sha256: binary_sha256.to_string(),
            binary_target_sha256,
            subcommand_fixed,
            subcommands,
            argv_template,
            capability_ref,
            env,
            result_mode,
        })
    }

    /// 可执行文件身份校验：在位 + 执行位 + 全量 sha256 对照钉值。
    fn verify_binary_identity(allowlist: &NativeAllowlist) -> Result<String, PreparingReject> {
        let path = &allowlist.binary;
        let metadata = std::fs::metadata(path).map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                PreparingReject::dependency(
                    "NATIVE_BINARY_MISSING",
                    format!("pinned binary {} not present on host", path.display()),
                )
            } else if e.kind() == std::io::ErrorKind::PermissionDenied {
                PreparingReject::policy(
                    "NATIVE_PERMISSION_DENIED",
                    format!(
                        "pinned binary {} stat denied (no auto-escalation)",
                        path.display()
                    ),
                )
            } else {
                PreparingReject::infra(format!("pinned binary {} stat failed: {e}", path.display()))
            }
        })?;
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o111 == 0 {
            return Err(PreparingReject::policy(
                "NATIVE_PERMISSION_DENIED",
                format!(
                    "pinned binary {} lacks execute bit (no auto-escalation)",
                    path.display()
                ),
            ));
        }
        let bytes = std::fs::read(path).map_err(|e| {
            if e.kind() == std::io::ErrorKind::PermissionDenied {
                PreparingReject::policy(
                    "NATIVE_PERMISSION_DENIED",
                    format!("pinned binary {} read denied", path.display()),
                )
            } else {
                PreparingReject::infra(format!("pinned binary {} read failed: {e}", path.display()))
            }
        })?;
        let actual = sha256_hex(&bytes);
        if actual != allowlist.binary_sha256 {
            return Err(PreparingReject::policy(
                "NATIVE_IDENTITY_MISMATCH",
                format!(
                    "pinned binary {} sha256 mismatch: pinned {}, measured {} (untrusted binary, RT-1 ADR D3)",
                    path.display(),
                    allowlist.binary_sha256,
                    actual
                ),
            ));
        }
        Ok(actual)
    }

    /// wrapper 双钉（C §5.1:270）：binary_target_sha256 在场时 fail-closed
    /// 解析脚本 wrapper 的 exec 目标（仅识别 `exec <绝对路径>` 形态；无法
    /// 唯一解析即拒绝派发，不做宽松猜测），并实测对照目标钉值。
    fn verify_wrapper_target(allowlist: &NativeAllowlist) -> Result<String, PreparingReject> {
        let Some(target_pin) = &allowlist.binary_target_sha256 else {
            return Ok(String::new());
        };
        let bytes = std::fs::read(&allowlist.binary).map_err(|e| {
            PreparingReject::infra(format!(
                "wrapper {} read failed: {e}",
                allowlist.binary.display()
            ))
        })?;
        let text = String::from_utf8_lossy(&bytes);
        let mut targets: Vec<String> = Vec::new();
        for line in text.lines() {
            let trimmed = line.trim();
            if let Some(rest) = trimmed.strip_prefix("exec ") {
                let token = rest
                    .split_whitespace()
                    .next()
                    .unwrap_or_default()
                    .trim_matches('"')
                    .trim_matches('\'');
                if token.starts_with('/') {
                    targets.push(token.to_string());
                }
            }
        }
        targets.dedup();
        if targets.len() != 1 {
            return Err(PreparingReject::policy(
                "NATIVE_WRAPPER_TARGET_UNRESOLVED",
                format!(
                    "wrapper {} declares binary_target_sha256 but {} unique `exec <abs-path>` target(s) found; fail-closed (RT-0.5-C 5.1:270)",
                    allowlist.binary.display(),
                    targets.len()
                ),
            ));
        }
        let target = PathBuf::from(&targets[0]);
        let tbytes = std::fs::read(&target).map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                PreparingReject::dependency(
                    "NATIVE_BINARY_MISSING",
                    format!("wrapper exec target {} not present", target.display()),
                )
            } else {
                PreparingReject::infra(format!(
                    "wrapper exec target {} read failed: {e}",
                    target.display()
                ))
            }
        })?;
        let actual = sha256_hex(&tbytes);
        if actual != *target_pin {
            return Err(PreparingReject::policy(
                "NATIVE_IDENTITY_MISMATCH",
                format!(
                    "wrapper exec target {} sha256 mismatch: pinned {target_pin}, measured {actual} (untrusted exec target, RT-0.5-C 5.1:270)",
                    target.display()
                ),
            ));
        }
        Ok(actual)
    }

    /// capability_ref 二道校验（F §6:226-228）：fresh + available 才放行；
    /// missing → DEPENDENCY；stale/身份不符 → POLICY_DENIED。钉值与目录身份
    /// 做前缀级对照（全量等值由 probe 实测后的 fingerprint 承载，属完整探测
    /// 面；本批不扩 trait 形状）。
    fn verify_capability(
        allowlist: &NativeAllowlist,
        provider: Option<&dyn CapabilityProvider>,
        measured_binary_sha256: &str,
    ) -> Result<(), PreparingReject> {
        let Some(capability_ref) = &allowlist.capability_ref else {
            return Ok(());
        };
        let Some(provider) = provider else {
            return Err(PreparingReject::policy(
                "NATIVE_CAPABILITY_UNAVAILABLE",
                format!(
                    "runtime.capability_ref {capability_ref} declared but no capability provider configured"
                ),
            ));
        };
        let Some(cached) = provider.get(capability_ref) else {
            return Err(PreparingReject::dependency(
                "NATIVE_CAPABILITY_MISSING",
                format!(
                    "capability {capability_ref} not present in catalog (RT-0.5-F 6: None -> dispatch rejected)"
                ),
            ));
        };
        match &cached.freshness {
            Freshness::Fresh => {}
            Freshness::Stale { reason } => {
                return Err(PreparingReject::policy(
                    "NATIVE_CAPABILITY_STALE",
                    format!(
                        "capability {capability_ref} is stale ({reason:?}); stale entries must not serve dispatch (RT-0.5-F 5.3)"
                    ),
                ));
            }
        }
        if cached.entry.status != crate::capability::CapabilityStatus::Available {
            return Err(PreparingReject::dependency(
                "NATIVE_CAPABILITY_MISSING",
                format!(
                    "capability {capability_ref} status is {} (RT-0.5-F 6: Missing -> dispatch rejected)",
                    cached.entry.status.as_str()
                ),
            ));
        }
        // 钉值与目录身份互为二道校验（前缀级；全量等值由 probe 实测承载）。
        if let Some(identity) = &cached.entry.binary_identity {
            let pin_prefix = &allowlist.binary_sha256[..16];
            if identity.sha256_prefix != pin_prefix {
                return Err(PreparingReject::policy(
                    "NATIVE_IDENTITY_MISMATCH",
                    format!(
                        "capability {capability_ref} catalog identity prefix {} does not match manifest pin prefix {pin_prefix} (RT-0.5-F 6: allowlist cross-check)",
                        identity.sha256_prefix
                    ),
                ));
            }
            // 实测钉值前缀 == 目录前缀已对照；manifest 全量实测值与目录前缀
            // 的一致性由本函数上方 prefix 比对覆盖（measured 的 [..16]）。
            if !measured_binary_sha256.starts_with(&identity.sha256_prefix) {
                return Err(PreparingReject::policy(
                    "NATIVE_IDENTITY_MISMATCH",
                    format!(
                        "capability {capability_ref} measured binary identity prefix {} does not match manifest pin (full-identity cross-check)",
                        identity.sha256_prefix
                    ),
                ));
            }
        }
        Ok(())
    }

    // -----------------------------------------------------------------
    // argv 构造（D3/D8.3：槽位只由 input_schema 校验后参数填充）
    // -----------------------------------------------------------------

    /// argv 构造：[binary, subcommand?, template...]。白名单生效时
    /// input.subcommand（经 input_schema 校验）为唯一子命令来源且必须命中
    /// 白名单；`{{input_path}}` 槽位值必须落在任务 in/ 目录前缀内（防任意
    /// 文件读取）；`{{out_dir}}`/`{{tmp_dir}}` 为 Adapter 保留槽位。
    fn build_argv(
        allowlist: &NativeAllowlist,
        input: &Value,
        ws: &NativeWorkspace,
    ) -> Result<BuiltArgv, PreparingReject> {
        let mut argv = vec![allowlist.binary.display().to_string()];
        if let Some(whitelist) = &allowlist.subcommands {
            let sc = input.get("subcommand").and_then(Value::as_str).ok_or_else(|| {
                PreparingReject::policy(
                    "NATIVE_SUBCOMMAND_NOT_ALLOWED",
                    "runtime.subcommands whitelist is declared; input.subcommand (string) is the only subcommand source",
                )
            })?;
            if !whitelist.iter().any(|w| w == sc) {
                return Err(PreparingReject::policy(
                    "NATIVE_SUBCOMMAND_NOT_ALLOWED",
                    format!("input.subcommand {sc:?} is not in the runtime.subcommands whitelist (zero spawn, RT-1 judge 2)"),
                ));
            }
            // 固定 subcommand 与白名单并存时只做一致性校验，不重复入 argv。
            if let Some(fixed) = &allowlist.subcommand_fixed {
                if !whitelist.iter().any(|w| w == fixed) {
                    return Err(PreparingReject::policy(
                        "NATIVE_SUBCOMMAND_NOT_ALLOWED",
                        format!(
                            "runtime.subcommand {fixed:?} is not in its own subcommands whitelist"
                        ),
                    ));
                }
            }
            argv.push(sc.to_string());
        } else if let Some(fixed) = &allowlist.subcommand_fixed {
            argv.push(fixed.clone());
        }
        let mut staged: Option<(PathBuf, String)> = None;
        for token in &allowlist.argv_template {
            if token == "{{input_path}}" {
                let v = input
                    .get("input_path")
                    .and_then(Value::as_str)
                    .ok_or_else(|| {
                        PreparingReject::policy(
                            "NATIVE_ARGV_TEMPLATE_DENIED",
                            "{{input_path}} slot requires string input.input_path",
                        )
                    })?;
                let resolved = resolve_input_path(v, &ws.in_dir)?;
                if let Some(content) = input.get("input_content").and_then(Value::as_str) {
                    staged = Some((resolved.clone(), content.to_string()));
                }
                argv.push(resolved.display().to_string());
            } else if token == "{{out_dir}}" {
                argv.push(ws.out_dir.display().to_string());
            } else if token == "{{tmp_dir}}" {
                argv.push(ws.tmp_dir.display().to_string());
            } else if token.starts_with("{{") && token.ends_with("}}") {
                let slot = &token[2..token.len() - 2];
                let v = input.get(slot).and_then(Value::as_str).ok_or_else(|| {
                    PreparingReject::policy(
                        "NATIVE_ARGV_TEMPLATE_DENIED",
                        format!("argv slot {{{{{slot}}}}} requires string input.{slot} (schema-validated parameter fill only)"),
                    )
                })?;
                if v.contains('\0') {
                    return Err(PreparingReject::policy(
                        "NATIVE_ARGV_TEMPLATE_DENIED",
                        format!("slot {slot} value carries NUL byte"),
                    ));
                }
                argv.push(v.to_string());
            } else {
                argv.push(token.clone());
            }
        }
        Ok(BuiltArgv { argv, staged })
    }

    // -----------------------------------------------------------------
    // 派发前置（PREPARING 全链；拒绝零子进程拉起）
    // -----------------------------------------------------------------

    /// PREPARING 全链（S5 第 2 步）：secrets 非空拒绝 → allowlist 解析 →
    /// 可执行身份校验（在位/执行位/钉值对照）→ wrapper 双钉 → capability
    /// 二道校验 → 工作区三目录 → argv 构造（槽位前缀校验 + 输入暂存解析）。
    fn prepare(
        &self,
        manifest: &Value,
        row: &crate::RunRow,
        capability: Option<&dyn CapabilityProvider>,
    ) -> Result<PreparedDispatch, PreparingReject> {
        // D4：native 档本批不支持 secret 注入，Manifest secrets 非空即拒绝。
        // 纵深兜底 fail-closed：仅 None / 空数组放行；非数组形状（对象/字符串
        // 等污染面）一律拒绝，不走 `as_array().unwrap_or(true)` 的 fail-open
        // 分支（登记面 validate.rs 强制数组，此处防派发面形状漂移）。
        let secrets_ok = match manifest.get("secrets") {
            None => true,
            Some(Value::Array(a)) => a.is_empty(),
            Some(_) => false,
        };
        if !secrets_ok {
            return Err(PreparingReject::policy(
                "NATIVE_SECRETS_UNSUPPORTED",
                "manifest declares secrets (non-empty array or non-array shape); native_rust dispatch does not support secret injection (env whitelist only, D4)",
            ));
        }
        let runtime = manifest.get("runtime").ok_or_else(|| {
            PreparingReject::policy(
                "NATIVE_ARGV_TEMPLATE_DENIED",
                "manifest.runtime is required",
            )
        })?;
        let allowlist = Self::parse_allowlist(runtime)?;
        // 白名单早检（判据 2：子命令越界在 PREPARING 即拒，先于任何工作区
        // 创建——零 spawn 的结构性佐证；build_argv 终检兜底）。
        if let Some(whitelist) = &allowlist.subcommands {
            let sc = row
                .input_sanitized
                .get("subcommand")
                .and_then(Value::as_str);
            let ok = sc
                .map(|s| whitelist.iter().any(|w| w == s))
                .unwrap_or(false);
            if !ok {
                return Err(PreparingReject::policy(
                    "NATIVE_SUBCOMMAND_NOT_ALLOWED",
                    format!(
                        "input.subcommand {:?} is not in the runtime.subcommands whitelist (zero spawn, RT-1 judge 2)",
                        sc.unwrap_or("<missing>")
                    ),
                ));
            }
        }
        let measured = Self::verify_binary_identity(&allowlist)?;
        Self::verify_wrapper_target(&allowlist)?;
        Self::verify_capability(&allowlist, capability, &measured)?;
        let ws = self
            .prepare_workspace(&row.run_id, &row.input_sanitized)
            .map_err(|f| PreparingReject::infra(f.message))?;
        let built = Self::build_argv(&allowlist, &row.input_sanitized, &ws)?;
        let resources = manifest.get("resources");
        // 资源强制解析（D8.4：优先 systemd-run → prlimit 兜底 → 两机制不可用
        // 即拒绝派发不裸跑，E §2 注 A；探测在 PREPARING 完成，失败零派发）。
        let wrapper = match self.config.resource_enforcement {
            ResourceEnforcement::Off => ResolvedWrapper::Unconfined,
            ResourceEnforcement::SystemdRun => {
                if !Self::probe_exec(
                    &self.config.systemd_run_bin,
                    &["--user", "--scope", "--quiet", "/bin/true"],
                ) {
                    return Err(PreparingReject::infra(
                        "systemd-run resource enforcement unavailable; refusing to dispatch unconfined (E §2 note A)",
                    ));
                }
                ResolvedWrapper::SystemdRun
            }
            ResourceEnforcement::Prlimit => {
                if !Self::probe_exec(&self.config.prlimit_bin, &["--version"]) {
                    return Err(PreparingReject::infra(
                        "prlimit resource enforcement unavailable; refusing to dispatch unconfined (E §2 note A)",
                    ));
                }
                ResolvedWrapper::Prlimit
            }
            ResourceEnforcement::Auto => {
                if Self::probe_exec(
                    &self.config.systemd_run_bin,
                    &["--user", "--scope", "--quiet", "/bin/true"],
                ) {
                    ResolvedWrapper::SystemdRun
                } else if Self::probe_exec(&self.config.prlimit_bin, &["--version"]) {
                    ResolvedWrapper::Prlimit
                } else {
                    return Err(PreparingReject::infra(
                        "neither systemd-run nor prlimit resource enforcement available; refusing to dispatch unconfined (E §2 note A)",
                    ));
                }
            }
        };
        Ok(PreparedDispatch {
            allowlist,
            ws,
            argv: built.argv,
            staged: built.staged,
            cpu: resources
                .and_then(|r| r.get("cpu"))
                .and_then(Value::as_str)
                .map(str::to_string),
            memory: resources
                .and_then(|r| r.get("memory"))
                .and_then(Value::as_str)
                .map(str::to_string),
            wrapper,
        })
    }

    // -----------------------------------------------------------------
    // 资源强制（D8.4 / E §2 注 A）
    // -----------------------------------------------------------------

    /// 短探测：spawn 固定参数 + 2s 有界等待（探测命令固定、无 Shell）。
    ///
    /// 探测环境与真派发同口径（D8.4 对称性）：`env_clear()` 后注入与
    /// [`Self::execute`] 派发一致的 D4 最小集（PATH=被探机制二进制所在目录 /
    /// TMPDIR / LANG=C.UTF-8）。否则探测继承父进程全量 env（含
    /// XDG_RUNTIME_DIR）而真派发是清环境——`systemd-run --user --scope` 依赖
    /// XDG_RUNTIME_DIR/DBUS_SESSION_BUS_ADDRESS 连接用户会话总线，会话宿主上
    /// 探测恒过、派发恒败（本机 2026-11-19 实测：继承 env exit=0 / 清环境
    /// exit=1 "Failed to connect to user scope bus"），首选机制不可达且失败
    /// 被误归为工具 EXECUTION。Manifest env 不入探测（机制可用性是平台事实
    /// 而非 Manifest 属性；探测只可能误判保守方向 → 兜底机制，不会反向失败）。
    fn probe_exec(bin: &str, args: &[&str]) -> bool {
        let mut cmd = Command::new(bin);
        cmd.args(args)
            .env_clear()
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        if let Some(parent) = Path::new(bin).parent() {
            cmd.env("PATH", parent.display().to_string());
        }
        cmd.env("TMPDIR", std::env::temp_dir().display().to_string());
        cmd.env("LANG", "C.UTF-8");
        let Ok(mut child) = cmd.spawn() else {
            return false;
        };
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            match child.try_wait() {
                Ok(Some(status)) => return status.success(),
                Ok(None) if Instant::now() >= deadline => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return false;
                }
                Ok(None) => std::thread::sleep(Duration::from_millis(50)),
                Err(_) => return false,
            }
        }
    }

    /// k8s cpu quantity → systemd CPUQuota 百分比（"1"→"100%"，"500m"→"50%"）。
    fn cpu_quota_percent(cpu: Option<&str>) -> Option<String> {
        let cpu = cpu?;
        let cores: f64 = if let Some(milli) = cpu.strip_suffix('m') {
            milli.parse::<f64>().ok()? / 1000.0
        } else {
            cpu.parse::<f64>().ok()?
        };
        if cores <= 0.0 {
            return None;
        }
        Some(format!("{}%", (cores * 100.0).round()))
    }

    /// k8s memory quantity → 字节（"256Mi"→268435456；"1Gi"→1073741824）。
    fn memory_bytes(memory: Option<&str>) -> Option<u64> {
        let memory = memory?;
        for (unit, mult) in [("Gi", 1024u64.pow(3)), ("Mi", 1024u64.pow(2)), ("Ki", 1024)] {
            if let Some(num) = memory.strip_suffix(unit) {
                return num.parse::<u64>().ok().map(|n| n * mult);
            }
        }
        memory.parse::<u64>().ok()
    }

    /// 资源强制包裹的命令构造（D8.4）：机制已由 PREPARING 探测定案
    /// （[`ResolvedWrapper`]），此处只构造——探测与构造分离，机制不可用的
    /// 拒绝发生在派发前置，RUNNING 期不再回退裸跑。
    fn resource_command(
        &self,
        wrapper: ResolvedWrapper,
        argv: &[String],
        cpu_quota: Option<&str>,
        memory: Option<&str>,
        cpu_seconds: u64,
        cwd: &Path,
    ) -> Command {
        let mem_bytes = Self::memory_bytes(memory);
        let quota = Self::cpu_quota_percent(cpu_quota);
        let mut cmd = match wrapper {
            ResolvedWrapper::Unconfined => {
                let mut cmd = Command::new(&argv[0]);
                cmd.args(&argv[1..]);
                cmd
            }
            ResolvedWrapper::SystemdRun => {
                let mut cmd = Command::new(&self.config.systemd_run_bin);
                cmd.args(["--user", "--scope", "--quiet"]);
                if let Some(q) = &quota {
                    cmd.arg(format!("-pCPUQuota={q}"));
                }
                if let Some(bytes) = mem_bytes {
                    cmd.arg(format!("-pMemoryMax={bytes}"));
                }
                cmd.arg("--");
                cmd.arg(&argv[0]);
                cmd.args(&argv[1..]);
                cmd
            }
            ResolvedWrapper::Prlimit => {
                let mut cmd = Command::new(&self.config.prlimit_bin);
                // RLIMIT_CPU 与墙钟预算对齐（CPU 秒）；RLIMIT_AS = Manifest memory 字节。
                let secs = cpu_seconds.clamp(1, 600);
                cmd.arg(format!("--cpu={secs}:{secs}"));
                if let Some(bytes) = mem_bytes {
                    cmd.arg(format!("--as={bytes}:{bytes}"));
                }
                cmd.arg("--");
                cmd.arg(&argv[0]);
                cmd.args(&argv[1..]);
                cmd
            }
        };
        cmd.current_dir(cwd);
        cmd
    }

    // -----------------------------------------------------------------
    // 进程面（D5/D6/D8：进程组 + 看门狗 + 取消 + 输出帽）
    // -----------------------------------------------------------------

    /// killpg 投递（std 无 killpg；固定路径 kill 工具负 pgid，Adapter 自有
    /// 基础设施面）。
    fn signal_group(&self, pgid: u32, signal: &str) {
        let _ = Command::new(&self.config.kill_bin)
            .args(["-s", signal, "--", &format!("-{pgid}")])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }

    /// 有界等待子进程退出（try_wait 轮询）；到期未退返回 false 供 KILL 兜底。
    fn wait_child(child: &mut Child, grace: Duration) -> bool {
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

    /// 进程组 TERM → 优雅窗口 → KILL → wait 收尸（超时与取消两路径共用）。
    fn terminate_group(&self, child: &mut Child, grace: Duration) {
        let pgid = child.id();
        self.signal_group(pgid, "TERM");
        if !Self::wait_child(child, grace) {
            self.signal_group(pgid, "KILL");
            let _ = child.wait();
        }
    }

    fn cancel_requested(engine: &Engine, run_id: &str) -> Result<bool, AdapterFailure> {
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

    /// 原生执行核心（RUNNING 期）：spawn + 看门狗（超时/取消/读帽）→ 中性终局。
    /// `result_mode` 为 DI-1 ADR D4 双模（frame 既有零改动 | file 产物收集）。
    #[allow(clippy::too_many_arguments)]
    fn execute(
        &self,
        engine: &Engine,
        run_id: &str,
        input: &Value,
        argv: Vec<String>,
        staged: Option<(PathBuf, String)>,
        ws: &NativeWorkspace,
        timeout: Duration,
        cpu_seconds: u64,
        cpu_quota: Option<&str>,
        memory: Option<&str>,
        max_output_bytes: u64,
        manifest_env: &[(String, String)],
        wrapper: ResolvedWrapper,
        result_mode: ResultMode,
    ) -> Result<AdapterOutcome, AdapterFailure> {
        if let Some((path, content)) = &staged {
            std::fs::write(path, content)
                .map_err(|e| AdapterFailure::new(format!("input staging write failed: {e}")))?;
        }

        let mut cmd =
            self.resource_command(wrapper, &argv, cpu_quota, memory, cpu_seconds, &ws.root);
        // D4：env_clear 后显式白名单（PATH=钉值二进制所在目录最小集 /
        // TMPDIR=任务 tmp / LANG=C.UTF-8 / Manifest env）。凭据类变量
        // （SSH_AUTH_SOCK/HTTP_PROXY/数据库路径/浏览器 Profile）结构性不存在。
        // 保留键不可覆盖：parse_allowlist 对 PATH/TMPDIR/LANG 冲突 PREPARING
        // 即拒（NATIVE_ENV_RESERVED_CONFLICT），此循环不可能改写白名单键。
        cmd.env_clear();
        if let Some(parent) = argv.first().and_then(|b| Path::new(b).parent()) {
            cmd.env("PATH", parent.display().to_string());
        }
        cmd.env("TMPDIR", ws.tmp_dir.display().to_string());
        cmd.env("LANG", "C.UTF-8");
        for (k, v) in manifest_env {
            cmd.env(k, v);
        }
        // D5：子进程独立进程组（孙进程同组，killpg 覆盖）。
        cmd.process_group(0);
        cmd.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let mut child = cmd
            .spawn()
            .map_err(|e| AdapterFailure::new(format!("native spawn failed: {e}")))?;

        // stdin 一行 JSON 请求（D8.1：run_id 不透传工具）；写毕即关。
        if let Some(mut stdin) = child.stdin.take() {
            use std::io::Write;
            let request = json!({
                "input": input,
                "budget": {
                    "timeout_seconds": timeout.as_secs(),
                    "max_output_bytes": max_output_bytes,
                },
            });
            let _ = writeln!(stdin, "{request}");
            let _ = stdin.flush();
            drop(stdin);
        }

        // stdout/stderr 并发读（podman_host.rs:461-481 P4-4 死锁修复同教训）：
        // spawn 后立即起读线程排空管道；stdout 逐块计数帽（D8.2a），超帽后
        // 继续排空（丢弃）防子进程写阻塞，置位由看门狗击杀。
        let read_cap = max_output_bytes.saturating_add(READ_CAP_SLACK_BYTES);
        let cap_flag = Arc::new(AtomicBool::new(false));
        let mut stdout_pipe = child.stdout.take();
        let cap_flag_reader = Arc::clone(&cap_flag);
        let stdout_reader = std::thread::spawn(move || {
            let mut kept: Vec<u8> = Vec::new();
            let mut total: u64 = 0;
            let mut chunk = [0u8; READ_CHUNK_BYTES];
            if let Some(pipe) = stdout_pipe.as_mut() {
                loop {
                    match pipe.read(&mut chunk) {
                        Ok(0) => break,
                        Ok(n) => {
                            total = total.saturating_add(n as u64);
                            if kept.len() < read_cap as usize {
                                let take = ((read_cap as usize) - kept.len()).min(n);
                                kept.extend_from_slice(&chunk[..take]);
                            }
                            if total > read_cap {
                                cap_flag_reader.store(true, Ordering::SeqCst);
                            }
                        }
                        Err(_) => break,
                    }
                }
            }
            (kept, total)
        });
        let mut stderr_pipe = child.stderr.take();
        let stderr_reader = std::thread::spawn(move || {
            let mut buf = Vec::new();
            if let Some(pipe) = stderr_pipe.as_mut() {
                let _ = pipe.read_to_end(&mut buf);
            }
            buf
        });

        enum WatchdogExit {
            Exited(std::process::ExitStatus),
            Timeout,
            Cancelled,
            CapHit,
            Wait(String),
        }

        let deadline = Instant::now() + timeout;
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break WatchdogExit::Exited(status),
                Ok(None) => {
                    if cap_flag.load(Ordering::SeqCst) {
                        // 读帽触顶：进程组击杀（D8.2a 防大输出整读 OOM）。
                        self.terminate_group(&mut child, KILL_GRACE_TIMEOUT);
                        let _ = child.wait();
                        break WatchdogExit::CapHit;
                    }
                    if Instant::now() >= deadline {
                        // 超时强制终止：killpg TERM → 2s 优雅窗口 → KILL（D5）。
                        self.terminate_group(&mut child, KILL_GRACE_TIMEOUT);
                        let _ = child.wait();
                        break WatchdogExit::Timeout;
                    }
                    match Self::cancel_requested(engine, run_id) {
                        Ok(true) => {
                            // 协作式取消（D-TC-04）：TERM → 3s 窗口 → KILL（D6）。
                            self.terminate_group(&mut child, KILL_GRACE_CANCEL);
                            let _ = child.wait();
                            break WatchdogExit::Cancelled;
                        }
                        Ok(false) => {}
                        Err(e) => break WatchdogExit::Wait(e.message),
                    }
                    std::thread::sleep(self.config.cancel_poll);
                }
                Err(e) => break WatchdogExit::Wait(format!("wait failed: {e}")),
            }
        };

        let (stdout_bytes, stdout_total) = stdout_reader.join().unwrap_or_default();
        let stderr_bytes = stderr_reader.join().unwrap_or_default();
        let stderr_text = String::from_utf8_lossy(&stderr_bytes).to_string();
        // 全量 stderr 落 out/stderr.log（D8.1：stderr 走 Artifact role=stderr）。
        if !stderr_bytes.is_empty() {
            let _ = std::fs::write(ws.out_dir.join("stderr.log"), &stderr_bytes);
        }
        // stdout 原始字节留档（截断/读帽 Artifact 对账载体）。
        let _ = std::fs::write(ws.out_dir.join("stdout.log"), &stdout_bytes);

        let cap_hit = cap_flag.load(Ordering::SeqCst);
        match status {
            WatchdogExit::Exited(exit) if exit.success() && result_mode == ResultMode::File => {
                // DI-1 ADR D4 file 模式：exit 0 → Succeeded，data = 产物收集面，
                // 不做帧解析。stdout 留档 out/stdout.log（上方已写）；读帽触顶
                // 时不静默丢字节：kept 只有 read_cap 字节、超出部分读线程已
                // 丢弃，FILE 模式无帧可截断交付 → 与帧模式 :1248-1255 同语义
                // FAILED（NATIVE_OUTPUT_CAP_EXCEEDED）。
                if cap_hit {
                    return Ok(AdapterOutcome::Failed {
                        category: "EXECUTION".to_string(),
                        code: json!("NATIVE_OUTPUT_CAP_EXCEEDED"),
                        message: format!(
                            "stdout exceeded read cap {read_cap} bytes (file mode); {stdout_total} bytes total, partial stdout.log retained for audit, bytes beyond cap not delivered (D8.2a / DI-1 ADR D4)"
                        ),
                        retryable: false,
                    });
                }
                Ok(AdapterOutcome::Succeeded {
                    data: json!({
                        "exit_code": 0,
                        "stdout_bytes": stdout_total,
                        "stdout_capped": false,
                        "artifacts": out_listing(&ws.out_dir),
                    }),
                    cached: false,
                    usage: Some(json!({
                        "output_bytes": stdout_total,
                        "stdout_capped": false,
                    })),
                })
            }
            WatchdogExit::Exited(exit) if exit.success() => {
                let text = String::from_utf8_lossy(&stdout_bytes);
                let first_line = text.lines().next().unwrap_or("");
                match serde_json::from_str::<Value>(first_line) {
                    Ok(frame) if frame.get("status").and_then(Value::as_str) == Some("result") => {
                        Ok(AdapterOutcome::Succeeded {
                            data: frame.get("data").cloned().unwrap_or(Value::Null),
                            cached: false,
                            usage: Some(json!({
                                "output_bytes": stdout_total,
                                "stdout_capped": cap_hit,
                            })),
                        })
                    }
                    Ok(frame) => Ok(AdapterOutcome::Failed {
                        category: "EXECUTION".to_string(),
                        code: json!("NATIVE_UNSUPPORTED_OUTPUT"),
                        message: format!(
                            "native stdout frame status unexpected: {}",
                            frame.get("status").and_then(Value::as_str).unwrap_or("<missing>")
                        ),
                        retryable: false,
                    }),
                    Err(_e) if cap_hit => Ok(AdapterOutcome::Failed {
                        category: "EXECUTION".to_string(),
                        code: json!("NATIVE_OUTPUT_CAP_EXCEEDED"),
                        message: format!(
                            "stdout exceeded read cap {read_cap} bytes (frame incomplete after cap, process exited normally); {stdout_total} bytes total (D8.2a)"
                        ),
                        retryable: false,
                    }),
                    Err(e) => Ok(AdapterOutcome::Failed {
                        category: "EXECUTION".to_string(),
                        code: json!("NATIVE_UNSUPPORTED_OUTPUT"),
                        message: format!("native stdout frame parse failed: {e} (D-TC-03: stdout 一行 JSON 结果帧)"),
                        retryable: false,
                    }),
                }
            }
            WatchdogExit::Exited(exit) => Ok(AdapterOutcome::Failed {
                category: "EXECUTION".to_string(),
                code: json!(exit.code().unwrap_or(-1)),
                message: format!(
                    "native tool exited {}: {}",
                    exit.code().unwrap_or(-1),
                    tail(&stderr_text, STDERR_TAIL_BYTES)
                ),
                retryable: false,
            }),
            WatchdogExit::CapHit => {
                // 读帽触顶：若首行帧已完整则按截断语义交付（结果侧
                // envelope::truncate 承接）；否则失败语义交付。
                // DI-1 ADR D4 file 模式：无帧可完整交付（kept 超 read_cap 部分已
                // 丢弃），不静默丢字节 → 一律 FAILED（与帧模式 Err 分支同语义）。
                if result_mode == ResultMode::File {
                    return Ok(AdapterOutcome::Failed {
                        category: "EXECUTION".to_string(),
                        code: json!("NATIVE_OUTPUT_CAP_EXCEEDED"),
                        message: format!(
                            "stdout exceeded read cap {read_cap} bytes before completion (file mode); process group terminated (D8.2a), {stdout_total} bytes total"
                        ),
                        retryable: false,
                    });
                }
                let text = String::from_utf8_lossy(&stdout_bytes);
                let first_line = text.lines().next().unwrap_or("");
                match serde_json::from_str::<Value>(first_line) {
                    Ok(frame) if frame.get("status").and_then(Value::as_str) == Some("result") => {
                        Ok(AdapterOutcome::Succeeded {
                            data: frame.get("data").cloned().unwrap_or(Value::Null),
                            cached: false,
                            usage: Some(json!({
                                "output_bytes": stdout_total,
                                "stdout_capped": true,
                            })),
                        })
                    }
                    _ => Ok(AdapterOutcome::Failed {
                        category: "EXECUTION".to_string(),
                        code: json!("NATIVE_OUTPUT_CAP_EXCEEDED"),
                        message: format!(
                            "stdout exceeded read cap {read_cap} bytes before a complete result frame; process group terminated (D8.2a), {stdout_total} bytes total"
                        ),
                        retryable: false,
                    }),
                }
            }
            WatchdogExit::Timeout => Ok(AdapterOutcome::TimedOut {
                message: format!(
                    "native watchdog fired after {}ms (budget.timeout_seconds); process group TERM→KILL",
                    timeout.as_millis()
                ),
            }),
            WatchdogExit::Cancelled => Ok(AdapterOutcome::Cancelled {
                message: "cooperative cancel observed via cancel_requested_at (D-TC-04); partial stdout not delivered as result"
                    .to_string(),
            }),
            WatchdogExit::Wait(reason) => Err(AdapterFailure::new(reason)),
        }
    }
}

/// `^[0-9a-f]{64}` 校验（与 validate.rs sha256_digest_ok 同规则）。
fn sha256_digest_ok(s: &str) -> bool {
    s.len() == 64
        && s.chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
}

/// `{{input_path}}` 槽位解析：in/ 相对名归一 + 前缀校验（D8.3 防任意文件
/// 读取；绝对路径/穿越路径/符号链接逃逸一律拒绝）。存在文件时 canonicalize
/// 消解符号链接；不存在时词法归一。
fn resolve_input_path(value: &str, in_dir: &Path) -> Result<PathBuf, PreparingReject> {
    let reject = |msg: String| {
        PreparingReject::policy(
            "NATIVE_ARGV_TEMPLATE_DENIED",
            format!("{{input_path}} value {value:?}: {msg} (task in/ prefix enforcement, D8.3)"),
        )
    };
    let in_canon = in_dir
        .canonicalize()
        .map_err(|e| PreparingReject::infra(format!("in/ canonicalize failed: {e}")))?;
    let joined = in_dir.join(value);
    let resolved = if joined.exists() {
        joined
            .canonicalize()
            .map_err(|e| reject(format!("canonicalize failed: {e}")))?
    } else {
        lexical_normalize(&joined)
    };
    if !resolved.starts_with(&in_canon) {
        return Err(reject(
            "resolves outside the task in/ directory".to_string(),
        ));
    }
    Ok(resolved)
}

/// 词法路径归一（消解 `.`/`..`，不触文件系统）。
fn lexical_normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for comp in path.components() {
        match comp {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

fn tail(text: &str, max: usize) -> String {
    let trimmed = text.trim();
    if trimmed.len() <= max {
        trimmed.to_string()
    } else {
        trimmed[trimmed.len() - max..].to_string()
    }
}

/// out/ 产物收集 data 列表（DI-1 ADR D4 file 模式；不登记 DB 行——
/// Artifact 行由 execute_native_rust_via_engine 既有 collect_out_artifacts
/// 统一登记，本函数只产出随 Succeeded data 交付的对账面：name/size/sha256，
/// 供调用方与 envelope artifacts 逐条对账）。
fn out_listing(out_dir: &Path) -> Vec<Value> {
    let mut entries: Vec<PathBuf> = std::fs::read_dir(out_dir)
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| p.is_file())
                .collect()
        })
        .unwrap_or_default();
    entries.sort();
    entries
        .iter()
        .filter_map(|path| {
            let bytes = std::fs::read(path).ok()?;
            let name = path.file_name()?.to_str()?.to_string();
            Some(json!({
                "name": name,
                "size_bytes": bytes.len() as u64,
                "sha256": sha256_hex(&bytes),
            }))
        })
        .collect()
}

/// out/ 产物收集与登记（podman_host collect_out_artifacts 同形态；stderr.log
/// 以 role=stderr 登记，D8.1）。
fn collect_out_artifacts(
    engine: &Engine,
    run_id: &str,
    ws: &NativeWorkspace,
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
        let role = if name == "stderr.log" {
            "stderr"
        } else if idx == 0 {
            "primary"
        } else {
            "attachment"
        };
        let artifact = EnvelopeArtifact {
            artifact_id: format!("art-{run_id}-{idx}-{name}"),
            role: role.to_string(),
            uri: format!("file://{}", path.display()),
            sha256: sha256_hex(&bytes),
            size_bytes: bytes.len() as u64,
            mime_type: mime.to_string(),
            provenance: if partial {
                format!("native_rust out/ 收集（partial：run 未正常完成，run_id={run_id}）")
            } else {
                format!("native_rust out/ 收集（run_id={run_id}）")
            },
        };
        engine.insert_artifact(run_id, &artifact)?;
        out.push(artifact);
    }
    Ok(out)
}

/// 端到端调用束（S5）。
pub struct NativeRustCall<'a> {
    /// 引擎（同一测试库上的受理/状态机/信封面）。
    pub engine: &'a Engine,
    /// 已受理（QUEUED）的 run id。
    pub run_id: &'a str,
    /// native_rust Adapter。
    pub adapter: &'a NativeRustAdapter,
    /// 工具 Manifest（runtime/secrets/resources 面由此读取；Agent 无任何
    /// argv 拼接入口）。
    pub manifest: &'a Value,
    /// Capability Provider（capability_ref 在场时必传，F §6）。
    pub capability: Option<&'a dyn CapabilityProvider>,
}

/// 端到端：QUEUED run 经 native_rust 受控子进程执行并交付合法 ToolRunResult
/// 信封（S5；QUEUED→PREPARING 校验→RUNNING 看门狗→终局 output_schema +
/// envelope::truncate + build_envelope，runtime_type="native_rust"；四路径
/// 信封统一）。
pub fn execute_native_rust_via_engine(
    call: NativeRustCall<'_>,
) -> Result<DriveOutcome, EngineError> {
    let engine = call.engine;
    let adapter = call.adapter;
    let row = engine.load_row(call.run_id)?;
    if row.status != ToolRunStatus::Queued {
        return Err(EngineError::NotStartable {
            status: row.status,
            run_id: call.run_id.to_string(),
        });
    }
    let started = Instant::now();
    let mut transcript = vec![ToolRunStatus::Queued];

    // 第 2 行：QUEUED → PREPARING（allowlist/钉值/能力校验，D7 五态映射）。
    engine.advance(
        call.run_id,
        ToolRunStatus::Queued,
        ToolRunStatus::Preparing,
        false,
    )?;
    transcript.push(ToolRunStatus::Preparing);

    // PREPARING 校验（拒绝即 preparing_failed，零子进程拉起）。
    let prepared = match adapter.prepare(call.manifest, &row, call.capability) {
        Ok(p) => p,
        Err(reject) => {
            return preparing_failed(
                engine,
                adapter,
                call.run_id,
                &row,
                started,
                transcript,
                &reject,
            );
        }
    };

    // 第 6 行：PREPARING → RUNNING（子进程 + 看门狗 + 取消轮询）。
    engine.advance(
        call.run_id,
        ToolRunStatus::Preparing,
        ToolRunStatus::Running,
        false,
    )?;
    transcript.push(ToolRunStatus::Running);

    // 看门狗预算：budget.timeout_seconds（[1, 600000] 域，与 worker/podman 同帽）。
    let budget = engine.resolved_budget(call.run_id)?;
    let timeout = Duration::from_millis(
        budget
            .timeout_seconds
            .saturating_mul(1000)
            .clamp(1, 600_000),
    );

    let outcome = adapter.execute(
        engine,
        call.run_id,
        &row.input_sanitized,
        prepared.argv,
        prepared.staged,
        &prepared.ws,
        timeout,
        budget.timeout_seconds,
        prepared.cpu.as_deref(),
        prepared.memory.as_deref(),
        budget.max_output_bytes,
        &prepared.allowlist.env,
        prepared.wrapper,
        prepared.allowlist.result_mode,
    );

    // tmp 清理（四路径统一）。
    adapter.cleanup_tmp(&prepared.ws);

    let ws = &prepared.ws;
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
                artifacts = collect_out_artifacts(engine, call.run_id, ws, true)?;
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
                // out/ 产物登记（stdout/stderr 留档与 sha256 对账载体）。
                artifacts = collect_out_artifacts(engine, call.run_id, ws, false)?;
                // §5.4 截断：与 Mock/worker/podman 侧共用同一函数（D8.2b）。
                let out = crate::envelope::truncate(call.run_id, &data, budget.max_output_bytes);
                truncated_flag = out.truncated;
                if truncated_flag {
                    let artifact_id = out.artifact_id.clone().unwrap_or_default();
                    let artifact = EnvelopeArtifact {
                        artifact_id: artifact_id.clone(),
                        role: "primary".to_string(),
                        uri: format!("native://artifacts/{artifact_id}"),
                        sha256: out.original_sha256.clone().unwrap_or_default(),
                        size_bytes: out.original_bytes,
                        mime_type: "application/json".to_string(),
                        provenance: "native_rust 截断原始输出（TOOL-CONTRACT 5.4）".to_string(),
                    };
                    engine.insert_artifact(call.run_id, &artifact)?;
                    artifacts.push(artifact);
                }
                (
                    ToolRunStatus::Succeeded,
                    Some(out.result.clone()),
                    Some(format!(
                        "{} v{} 原生执行成功（node {}）",
                        row.tool_id, row.tool_version, adapter.config.node_id
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
            artifacts = collect_out_artifacts(engine, call.run_id, ws, true)?;
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
            artifacts = collect_out_artifacts(engine, call.run_id, ws, true)?;
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
            artifacts = collect_out_artifacts(engine, call.run_id, ws, true)?;
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
                Some(format!("原生执行失败：{}", failure.message)),
                Some(EnvelopeError {
                    category: "INFRA".to_string(),
                    code: json!("NATIVE_TRANSPORT_FAILED"),
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
            runtime_type: "native_rust".to_string(),
            runtime_node_id: Some(adapter.config.node_id.clone()),
        },
        elapsed_ms,
    )?;
    Ok(DriveOutcome {
        envelope,
        transcript,
    })
}

/// 第 7 行：PREPARING → FAILED（allowlist/钉值/能力/资源面拒绝，D7 映射）。
fn preparing_failed(
    engine: &Engine,
    adapter: &NativeRustAdapter,
    run_id: &str,
    row: &RunRow,
    started: Instant,
    mut transcript: Vec<ToolRunStatus>,
    reject: &PreparingReject,
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
                category: reject.category.to_string(),
                code: reject.code.clone(),
                message: reject.message.clone(),
                retryable: reject.retryable,
                detail_ref: None,
            }),
            summary: Some(format!("原生派发准备期拒绝（{}）", reject.category)),
            artifacts: Vec::new(),
            truncated: false,
            cached: false,
            runtime_type: "native_rust".to_string(),
            runtime_node_id: Some(adapter.config.node_id.clone()),
        },
        started.elapsed().as_millis() as u64,
    )?;
    Ok(DriveOutcome {
        envelope,
        transcript,
    })
}
