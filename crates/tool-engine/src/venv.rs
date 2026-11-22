//! python_worker 项目级 venv 探测面（RT-2，docs/phase0/RT-2-ADR-VENV-GOVERNANCE.md D3/D6）。
//!
//! 职责：对 `workers/python/.venv` 形态的项目级 venv 做 L0/L1/L2 分层探测并产出
//! [`VenvIdentity`]（对齐 RT-0.5-F §2 CapabilityEntry/BinaryIdentity 形态）：
//! - **L0** 路径/身份静态面：`pyvenv.cfg` 读面、解释器 symlink 解析、解释器全量
//!   sha256、owner/mode（纯 `std::fs` 读面，D3 否决项 3 同款——禁 spawn pip）；
//! - **L1** 版本面：`<venv>/<interpreter> -V` 受控探测（固定 argv、短超时、输出
//!   帽、超时 kill，进程范式沿用 adapter 的 std::process + 线程），与
//!   `pyvenv.cfg` version 对拍（major.minor 匹配为 available 必要条件；patch 级
//!   漂移即 A4② 形态，双录为证据不翻转状态）；
//! - **L2** 依赖集面：`site-packages/*.dist-info` 目录名静态解析（PEP 376）→
//!   排序 capset → `deps_fingerprint`；direct_deps 覆盖检查与 lock sha256 对账。
//!
//! 指纹公式实例化 RT-0.5-C §5.1（冻结）：`fingerprint = sha256(canonical_json(
//! {bin_sha256, version, capset}))` 前 16 hex；`bin_sha256` 取 symlink 解析后
//! 真实解释器文件的全量 sha256（wrapper_target 先例：symlink 目标身份随
//! [`InterpreterIdentity::symlink_target`] 双录）。
//!
//! 五态沿用 C §4：available / missing / incompatible / probe_failed /
//! permission_denied（disabled 属 Tool Registry 工具事实，本批不产出）；失败
//! 五类沿用 F:126-132 ProbeError 五态，D7 映射见
//! [`ProbeFailureKind::envelope_mapping`]。探测结果只进探测缓存与批次报告，
//! 零 Tool Registry / Manifest 写入入口（C §6 职责边界硬性红线）。

use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use tool_registry::hash::sha256_hex;

/// L1 版本探测超预算（F §2 建议初值 L1 ≤ 5s）。
pub const PROBE_TIMEOUT: Duration = Duration::from_secs(5);

/// L1 输出帽（`python -V` 单行远小于此；防异常解释器输出灌爆读面）。
pub const PROBE_OUTPUT_CAP: u64 = 4096;

/// venv 声明块（schemas/tool-manifest/v1.json python_worker 分支 additive 可选
/// 对象 `runtime.venv` 的执行面投影；形状冻结于 RT-2 ADR D5）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VenvConfig {
    /// venv 根路径（working_dir 相对或绝对；禁 `..`，validate.rs 同规则）。
    pub path: PathBuf,
    /// venv 根内解释器相对路径；缺省 `bin/python`。
    #[serde(default)]
    pub interpreter: Option<String>,
    /// lock 文件路径（working_dir 相对或绝对）；在场即做 sha256 对账（D5）。
    #[serde(default)]
    pub requirements_lock: Option<PathBuf>,
    /// 直接依赖非空字符串数组；capset 覆盖检查触发器（D5）。
    pub direct_deps: Vec<String>,
}

impl VenvConfig {
    /// venv 根内解释器相对路径（缺省 `bin/python`）。
    pub fn interpreter_rel(&self) -> &str {
        self.interpreter.as_deref().unwrap_or("bin/python")
    }

    /// 相对路径按 working_dir（repo:// 落地根）解析；绝对路径原样。
    pub fn resolve_path(&self, working_dir: Option<&Path>) -> PathBuf {
        resolve_against(&self.path, working_dir)
    }
}

/// 相对路径解析：绝对原样，相对按 working_dir 落地（无 working_dir 时按 CWD）。
fn resolve_against(path: &Path, working_dir: Option<&Path>) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        working_dir.unwrap_or_else(|| Path::new(".")).join(path)
    }
}

/// venv 条目状态（C §4 五态词表 + available；disabled 本批不产出）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum VenvStatus {
    /// venv 在位且身份/依赖/lock 对账全过。
    Available,
    /// venv 目录/解释器/lock 缺失。
    Missing,
    /// 身份不符（pyvenv.cfg version 与实测 major.minor 漂移、形态不符等）。
    Incompatible,
    /// 其余探测失败（IO 异常、L1 子进程异常退出等）。
    ProbeFailed,
    /// venv 根/解释器/lock 的 stat 或读被拒。
    PermissionDenied,
}

impl VenvStatus {
    /// C §4 词表串行化值。
    pub fn as_str(self) -> &'static str {
        match self {
            VenvStatus::Available => "available",
            VenvStatus::Missing => "missing",
            VenvStatus::Incompatible => "incompatible",
            VenvStatus::ProbeFailed => "probe_failed",
            VenvStatus::PermissionDenied => "permission_denied",
        }
    }
}

/// 探测失败五类（F:126-132 ProbeError 五态；D7 逐类映射，零兜底合并）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProbeFailureKind {
    /// 路径/解释器/lock 不存在。
    Missing,
    /// stat 或读被拒。
    PermissionDenied,
    /// L1 `-V` 受控探测子进程超预算。
    Timeout,
    /// 形态不符：pyvenv.cfg 不可解析/版本对拍不符/dist-info 名不可解析/
    /// direct_deps 覆盖失败/lock sha256 对账不符。
    UnsupportedOutput,
    /// 其余失败（IO 异常、子进程异常退出等）。
    ProbeFailed,
}

impl ProbeFailureKind {
    /// D7 五态映射（envelope category, code, retryable）——RT-1 D7 表逐类复用，
    /// 错误码 `VENV_` 前缀专属化；词表值全部落在信封九类内。
    pub fn envelope_mapping(self) -> (&'static str, serde_json::Value, bool) {
        match self {
            ProbeFailureKind::Missing => (
                "DEPENDENCY",
                serde_json::json!("VENV_NOT_PROVISIONED"),
                false,
            ),
            ProbeFailureKind::PermissionDenied => (
                "POLICY_DENIED",
                serde_json::json!("VENV_PERMISSION_DENIED"),
                false,
            ),
            ProbeFailureKind::Timeout => ("TIMEOUT", serde_json::json!(-32002), false),
            ProbeFailureKind::UnsupportedOutput => (
                "EXECUTION",
                serde_json::json!("VENV_IDENTITY_MISMATCH"),
                false,
            ),
            ProbeFailureKind::ProbeFailed => {
                ("INFRA", serde_json::json!("VENV_PROBE_FAILED"), true)
            }
        }
    }

    /// 对应的 C §4 条目状态。
    pub fn status(self) -> VenvStatus {
        match self {
            ProbeFailureKind::Missing => VenvStatus::Missing,
            ProbeFailureKind::PermissionDenied => VenvStatus::PermissionDenied,
            ProbeFailureKind::Timeout => VenvStatus::ProbeFailed,
            ProbeFailureKind::UnsupportedOutput => VenvStatus::Incompatible,
            ProbeFailureKind::ProbeFailed => VenvStatus::ProbeFailed,
        }
    }
}

/// 探测失败（kind 决定 D7 信封映射；status 为 C §4 条目状态词）。
#[derive(Debug, Clone, PartialEq)]
pub struct VenvProbeFailure {
    /// 失败五类（D7 映射触发器）。
    pub kind: ProbeFailureKind,
    /// C §4 条目状态。
    pub status: VenvStatus,
    /// 判定说明（批次报告/信封 message 用）。
    pub detail: String,
}

impl VenvProbeFailure {
    fn new(kind: ProbeFailureKind, detail: impl Into<String>) -> Self {
        Self {
            kind,
            status: kind.status(),
            detail: detail.into(),
        }
    }
}

/// 解释器身份（BinaryIdentity 形态，F §2；D6 interpreter_identity 冻结字段 +
/// symlink 目标双录 additive 字段）。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct InterpreterIdentity {
    /// symlink 解析后真实解释器文件的全量 sha256（64 hex 小写）。
    pub sha256_hex: String,
    /// 文件类型（"elf-pie" | "elf-exec" | "script" | "data"，魔数静态分类）。
    pub file_type: String,
    /// 所有者与权限（"emo:emo 755" 形态；uid/gid 反查失败回退 "uidN" 形态）。
    pub owner_mode: String,
    /// symlink 目标双录（D3 wrapper_target 先例：声明解释器路径为 symlink 时
    /// 记录 read_link 原值；非 symlink 为 None）。
    pub symlink_target: Option<String>,
}

/// venv 身份记录（RT-2 ADR D6 冻结 schema；记录去处 = 探测缓存 + 批次报告，
/// 禁入 Tool Registry / Manifest）。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct VenvIdentity {
    /// venv 根（归一绝对路径）。
    pub venv_root: PathBuf,
    /// `<venv>/<interpreter>` 绝对路径（**派发面，不 canonicalize**：venv 激活
    /// 依赖该路径形态，CPython 以 argv[0] 逐级查 pyvenv.cfg）；symlink 解析后
    /// 真实文件身份见 [`VenvIdentity::interpreter_identity`]。
    pub interpreter_path: PathBuf,
    /// 解释器身份（BinaryIdentity 形态）。
    pub interpreter_identity: InterpreterIdentity,
    /// 基础解释器（pyvenv.cfg home 字段）。
    pub base_interpreter: PathBuf,
    /// L1 实测版本（`<interpreter> -V` 输出解析）。
    pub interpreter_version: String,
    /// pyvenv.cfg version 字段（与上一字段并录，A4② 漂移证据面）。
    pub python_version_cfg: String,
    /// 依赖 capset（dist-info 解析，(name, version) 排序 canonical）。
    pub deps_capset: Vec<(String, String)>,
    /// `sha256(canonical_json(capset))` 前 16 hex。
    pub deps_fingerprint: String,
    /// lock 文件全量 sha256（声明 requirements_lock 在场时必填）。
    pub requirements_lock_sha256: Option<String>,
    /// direct_deps ⊆ capset 判定。
    pub direct_deps_satisfied: bool,
    /// 条目状态（available 时恒 Available）。
    pub status: VenvStatus,
    /// 探测时刻（UNIX epoch 毫秒；零新依赖约束下的 D6 timestamp 承载）。
    pub probe_timestamp_unix_ms: u64,
    /// C §5.1 指纹公式前 16 hex。
    pub fingerprint: String,
}

impl VenvIdentity {
    /// C §5.3 stale 判定：与上次身份相比，解释器 sha256 / pyvenv.cfg version /
    /// 依赖集指纹 / lock sha256 任一变化即 stale（stale 不可供派发判定使用，
    /// fail-closed 重探）。
    pub fn is_stale_vs(&self, previous: &VenvIdentity) -> bool {
        self.interpreter_identity.sha256_hex != previous.interpreter_identity.sha256_hex
            || self.python_version_cfg != previous.python_version_cfg
            || self.deps_fingerprint != previous.deps_fingerprint
            || self.requirements_lock_sha256 != previous.requirements_lock_sha256
    }
}

/// L0+L1+L2 全量探测（默认预算 [`PROBE_TIMEOUT`]）。
///
/// `expected_lock_sha256` 在场时（探测缓存重探场景）与 lock 文件实测 sha256
/// 对账，不符即 identity mismatch（C §5.3 stale 失效的 fail-closed 路径）。
pub fn probe(
    config: &VenvConfig,
    working_dir: Option<&Path>,
    expected_lock_sha256: Option<&str>,
) -> Result<VenvIdentity, VenvProbeFailure> {
    probe_with_budget(config, working_dir, expected_lock_sha256, PROBE_TIMEOUT)
}

/// 指定 L1 预算的探测（测试面用；生产口径 = [`probe`]）。
pub fn probe_with_budget(
    config: &VenvConfig,
    working_dir: Option<&Path>,
    expected_lock_sha256: Option<&str>,
    l1_timeout: Duration,
) -> Result<VenvIdentity, VenvProbeFailure> {
    // 防御：`..` 分量拒绝（validate.rs 登记面同规则；探测面纵深防御）。
    for candidate in [
        &config.path,
        config.requirements_lock.as_deref().unwrap_or(Path::new("")),
    ] {
        if candidate.components().any(|c| c.as_os_str() == "..") {
            return Err(VenvProbeFailure::new(
                ProbeFailureKind::UnsupportedOutput,
                format!(
                    "path {:?} contains `..` component (traversal rejected)",
                    candidate
                ),
            ));
        }
    }
    // 防御：interpreter 必须 venv 根内相对路径（validate.rs 登记面同规则；探测
    // 面纵深防御）。绝对值在 `Path::join` 语义下整体替换 base——派发目标逃出
    // venv 根而 capset/lock 对账仍按 venv 面通过，即「声明 venv = 锁定依赖集」
    // 被静默旁路（D4 否决项 1「回退系统解释器」的同源降级面，D5 冻结形状）。
    let interp_rel = config.interpreter_rel();
    if Path::new(interp_rel).is_absolute()
        || interp_rel.starts_with('\\')
        || (interp_rel.len() >= 2
            && interp_rel.as_bytes()[1] == b':'
            && interp_rel.as_bytes()[0].is_ascii_alphabetic())
        || Path::new(interp_rel)
            .components()
            .any(|c| c.as_os_str() == "..")
    {
        return Err(VenvProbeFailure::new(
            ProbeFailureKind::UnsupportedOutput,
            format!(
                "interpreter {interp_rel:?} must be a venv-root-relative path without \
                 `..` (absolute interpreter would replace the venv base under Path::join \
                 and escape the frozen dependency face; traversal/escape rejected)"
            ),
        ));
    }
    if config.direct_deps.is_empty() {
        return Err(VenvProbeFailure::new(
            ProbeFailureKind::UnsupportedOutput,
            "direct_deps must be a non-empty array",
        ));
    }

    let root = config.resolve_path(working_dir);
    let root = canonicalize_or_fail(&root, "venv root")?;

    // ---- L0：pyvenv.cfg 读面 + 解释器身份 ----
    let cfg_text = fs::read_to_string(root.join("pyvenv.cfg"))
        .map_err(|e| io_failure(&e, "read pyvenv.cfg"))?;
    let cfg = PyVenvCfg::parse(&cfg_text)?;

    // 派发面解释器路径 = `<venv>/<interpreter>` 绝对路径（**不 canonicalize**：
    // CPython 以 argv[0] 路径逐级查找 pyvenv.cfg 完成 venv 激活，换成解析后
    // 真实二进制路径会丢掉 venv 激活面；D4「解析到 <venv>/bin/python 绝对路径」）。
    let dispatch_interp = root.join(config.interpreter_rel());
    let raw_meta = fs::symlink_metadata(&dispatch_interp)
        .map_err(|e| io_failure(&e, format!("stat {}", dispatch_interp.display())))?;
    let symlink_target = if raw_meta.file_type().is_symlink() {
        Some(
            fs::read_link(&dispatch_interp)
                .map_err(|e| io_failure(&e, format!("readlink {}", dispatch_interp.display())))?
                .to_string_lossy()
                .into_owned(),
        )
    } else {
        None
    };
    // 身份面：symlink 解析后真实解释器文件（sha256/类型/属主）。
    let resolved_interp = canonicalize_or_fail(&dispatch_interp, "interpreter")?;
    let interp_bytes = fs::read(&resolved_interp)
        .map_err(|e| io_failure(&e, format!("read {}", resolved_interp.display())))?;
    let interp_sha = sha256_hex(&interp_bytes);
    let interpreter_identity = InterpreterIdentity {
        sha256_hex: interp_sha,
        file_type: classify_executable(&interp_bytes),
        owner_mode: owner_mode_of(&resolved_interp),
        symlink_target,
    };

    // ---- L1：版本面（受控探测 + cfg 对拍；经 venv 内派发路径执行，与派发同面） ----
    let actual_version = run_version_probe(&dispatch_interp, l1_timeout)?;
    if major_minor(&actual_version) != major_minor(&cfg.version) {
        return Err(VenvProbeFailure {
            kind: ProbeFailureKind::UnsupportedOutput,
            status: VenvStatus::Incompatible,
            detail: format!(
                "python version mismatch: pyvenv.cfg version {:?} vs actual {:?} \
                 (major.minor must match; patch-level drift is recorded not fatal)",
                cfg.version, actual_version
            ),
        });
    }

    // ---- L2：依赖集面（dist-info 静态解析 → capset） ----
    let site = find_site_packages(&root, &actual_version)?;
    let capset = parse_capset(&site)?;
    let direct_deps_satisfied = direct_deps_covered(&config.direct_deps, &capset);
    if !direct_deps_satisfied {
        let missing: Vec<String> = config
            .direct_deps
            .iter()
            .map(|d| normalize_dist_name(d))
            .filter(|d| !capset.iter().any(|(n, _)| n == d))
            .collect();
        return Err(VenvProbeFailure::new(
            ProbeFailureKind::UnsupportedOutput,
            format!(
                "direct_deps not covered by capset: declared {:?}, missing {:?} \
                 (site-packages dist-info capset {:?})",
                config.direct_deps, missing, capset
            ),
        ));
    }
    let deps_fingerprint = short_fingerprint(&canonical_capset_json(&capset));

    // ---- lock 对账（声明在场即做；expected 与实测不符 = 身份不符） ----
    let requirements_lock_sha256 = match &config.requirements_lock {
        Some(lock) => {
            let lock_path = resolve_against(lock, working_dir);
            let bytes = fs::read(&lock_path)
                .map_err(|e| io_failure(&e, format!("read lock {}", lock_path.display())))?;
            let actual = sha256_hex(&bytes);
            if let Some(expected) = expected_lock_sha256 {
                if expected != actual {
                    return Err(VenvProbeFailure::new(
                        ProbeFailureKind::UnsupportedOutput,
                        format!(
                            "requirements lock sha256 mismatch: expected {expected}, actual {actual} \
                             (lock changed since recorded identity; C §5.3 stale fail-closed)"
                        ),
                    ));
                }
            }
            Some(actual)
        }
        None => None,
    };

    let fingerprint = short_fingerprint(&canonical_fingerprint_json(
        &interpreter_identity.sha256_hex,
        &actual_version,
        &capset,
    ));

    Ok(VenvIdentity {
        venv_root: root,
        interpreter_path: dispatch_interp,
        interpreter_identity,
        base_interpreter: cfg.home,
        interpreter_version: actual_version,
        python_version_cfg: cfg.version,
        deps_capset: capset,
        deps_fingerprint,
        requirements_lock_sha256,
        direct_deps_satisfied,
        status: VenvStatus::Available,
        probe_timestamp_unix_ms: unix_ms(),
        fingerprint,
    })
}

/// pyvenv.cfg 解析结果（L0 读面）。
struct PyVenvCfg {
    /// home 字段（基础解释器目录；executable 字段缺省时同录 home）。
    home: PathBuf,
    /// version 字段原值（如 "3.14.9"）。
    version: String,
}

impl PyVenvCfg {
    /// 形态不符（缺 home/version、version 不可解析为 semver 形态）即
    /// UnsupportedOutput（D7 形态不符路径）。
    fn parse(text: &str) -> Result<Self, VenvProbeFailure> {
        let mut home: Option<PathBuf> = None;
        let mut version: Option<String> = None;
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let key = key.trim();
            let value = value.trim();
            match key {
                "home" => home = Some(PathBuf::from(value)),
                "version" => version = Some(value.to_string()),
                _ => {}
            }
        }
        let home = home.ok_or_else(|| {
            VenvProbeFailure::new(
                ProbeFailureKind::UnsupportedOutput,
                "pyvenv.cfg missing `home` field (shape mismatch)",
            )
        })?;
        let version = version.ok_or_else(|| {
            VenvProbeFailure::new(
                ProbeFailureKind::UnsupportedOutput,
                "pyvenv.cfg missing `version` field (shape mismatch)",
            )
        })?;
        if parse_semver_triplet(&version).is_none() {
            return Err(VenvProbeFailure::new(
                ProbeFailureKind::UnsupportedOutput,
                format!("pyvenv.cfg version {version:?} is not a parseable x.y.z shape"),
            ));
        }
        Ok(Self { home, version })
    }
}

/// "3.14.10" → (3, 14, 10)；不可解析返回 None。
fn parse_semver_triplet(s: &str) -> Option<(u64, u64, u64)> {
    let mut it = s.split('.');
    let major = it.next()?.parse::<u64>().ok()?;
    let minor = it.next()?.parse::<u64>().ok()?;
    let patch = it.next().unwrap_or("0").parse::<u64>().ok()?;
    Some((major, minor, patch))
}

/// "Python 3.14.10" → "3.14.10"。
fn major_minor(version: &str) -> (u64, u64) {
    match parse_semver_triplet(version) {
        Some((major, minor, _)) => (major, minor),
        None => (0, 0),
    }
}

/// canonical 指纹 JSON（键序手工固定：bin_sha256 / capset / version 字典序；
/// 不依赖 serde_json Map 的 feature 形态，确定性优先）。
fn canonical_fingerprint_json(
    bin_sha256: &str,
    version: &str,
    capset: &[(String, String)],
) -> String {
    let mut out = String::with_capacity(128 + capset.len() * 48);
    out.push_str("{\"bin_sha256\":");
    out.push_str(&json_scalar(bin_sha256));
    out.push_str(",\"capset\":[");
    for (idx, (name, ver)) in capset.iter().enumerate() {
        if idx > 0 {
            out.push(',');
        }
        out.push('[');
        out.push_str(&json_scalar(name));
        out.push(',');
        out.push_str(&json_scalar(ver));
        out.push(']');
    }
    out.push_str("],\"version\":");
    out.push_str(&json_scalar(version));
    out.push('}');
    out
}

/// capset 单独 canonical（deps_fingerprint 输入）。
fn canonical_capset_json(capset: &[(String, String)]) -> String {
    let mut out = String::with_capacity(64 + capset.len() * 48);
    out.push('[');
    for (idx, (name, ver)) in capset.iter().enumerate() {
        if idx > 0 {
            out.push(',');
        }
        out.push('[');
        out.push_str(&json_scalar(name));
        out.push(',');
        out.push_str(&json_scalar(ver));
        out.push(']');
    }
    out.push(']');
    out
}

/// 标量 JSON 转义（serde_json 标量序列化确定性可靠）。
fn json_scalar(s: &str) -> String {
    serde_json::to_string(s).unwrap_or_else(|_| format!("{s:?}"))
}

/// sha256 前 16 hex（C §5.1 取证口径：记录全量、指纹只取前缀，前缀不升格校验钉值）。
fn short_fingerprint(canonical: &str) -> String {
    sha256_hex(canonical.as_bytes())[..16].to_string()
}

/// PEP 503 分发名归一（lowercase + 连续 `[-_.]` 归一为 `-`）。
fn normalize_dist_name(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut prev_sep = false;
    for ch in name.chars() {
        if matches!(ch, '-' | '_' | '.') {
            if !prev_sep {
                out.push('-');
            }
            prev_sep = true;
        } else {
            out.extend(ch.to_lowercase());
            prev_sep = false;
        }
    }
    out
}

/// direct_deps（归一后）⊆ capset 名集。
fn direct_deps_covered(direct_deps: &[String], capset: &[(String, String)]) -> bool {
    direct_deps
        .iter()
        .map(|d| normalize_dist_name(d))
        .all(|d| capset.iter().any(|(n, _)| *n == d))
}

/// site-packages 定位：`lib/pythonX.Y/site-packages`（X.Y 取实测版本）；
/// 缺失 = venv 形态不符。
fn find_site_packages(root: &Path, version: &str) -> Result<PathBuf, VenvProbeFailure> {
    let (mm_major, mm_minor) = major_minor(version);
    let candidate = root.join(format!("lib/python{mm_major}.{mm_minor}/site-packages"));
    if candidate.is_dir() {
        return Ok(candidate);
    }
    // 版本串形态差异（如 3.14 vs 3.14.0）兜底扫 lib/python*/
    let lib = root.join("lib");
    let entries =
        fs::read_dir(&lib).map_err(|e| io_failure(&e, format!("read {}", lib.display())))?;
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with("python") {
            let candidate = entry.path().join("site-packages");
            if candidate.is_dir() {
                return Ok(candidate);
            }
        }
    }
    Err(VenvProbeFailure::new(
        ProbeFailureKind::UnsupportedOutput,
        format!("site-packages not found under {}", lib.display()),
    ))
}

/// dist-info 目录名解析（PEP 376 `{name}-{version}.dist-info`）→ 排序 capset。
/// PEP 376 名称段允许连字符（sdist/legacy 安装面：`python-dateutil-2.9.0`
/// 的分发名本身含 `-`），故按**最后一个** `-` 切分 name/version——分发名内
/// 连字符不得误判为多段形态而整探拒绝（fail-closed 误伤可用性面）；完全无
/// `-` 的目录名不可解析 = 形态不符（D7）。
fn parse_capset(site: &Path) -> Result<Vec<(String, String)>, VenvProbeFailure> {
    let mut capset = Vec::new();
    let entries =
        fs::read_dir(site).map_err(|e| io_failure(&e, format!("read {}", site.display())))?;
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        let Some(dist) = name.strip_suffix(".dist-info") else {
            continue;
        };
        let Some((dist_name, version)) = dist.rsplit_once('-') else {
            return Err(VenvProbeFailure::new(
                ProbeFailureKind::UnsupportedOutput,
                format!("unparseable dist-info name {name:?} (PEP 376 name-version shape)"),
            ));
        };
        if dist_name.is_empty() || version.is_empty() {
            return Err(VenvProbeFailure::new(
                ProbeFailureKind::UnsupportedOutput,
                format!("unparseable dist-info name {name:?} (PEP 376 name-version shape)"),
            ));
        }
        capset.push((normalize_dist_name(dist_name), version.to_string()));
    }
    capset.sort();
    capset.dedup();
    Ok(capset)
}

/// L1 受控版本探测：固定 argv `[interpreter, "-V"]`，短超时到点 kill，输出帽。
/// 进程范式沿用 adapter 既有 std::process + 线程面（零 tokio 红线）。
fn run_version_probe(interpreter: &Path, timeout: Duration) -> Result<String, VenvProbeFailure> {
    let mut child = Command::new(interpreter)
        .arg("-V")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| io_failure(&e, format!("spawn {} -V", interpreter.display())))?;
    let stdout_cap = child
        .stdout
        .take()
        .map(|s| capped_reader(s, PROBE_OUTPUT_CAP));
    let stderr_cap = child
        .stderr
        .take()
        .map(|s| capped_reader(s, PROBE_OUTPUT_CAP));

    let deadline = Instant::now() + timeout;
    let outcome = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Ok(status),
            Ok(None) => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    break Err(VenvProbeFailure::new(
                        ProbeFailureKind::Timeout,
                        format!("L1 version probe exceeded budget {:?} (killed)", timeout),
                    ));
                }
                thread::sleep(Duration::from_millis(5));
            }
            Err(e) => break Err(io_failure(&e, "wait version probe")),
        }
    };
    let stdout = stdout_cap.and_then(|h| h.join().ok()).unwrap_or_default();
    let stderr = stderr_cap.and_then(|h| h.join().ok()).unwrap_or_default();
    match outcome {
        Ok(status) => {
            let combined = format!("{stdout}{stderr}");
            let version = combined
                .lines()
                .find_map(|l| l.trim().strip_prefix("Python "))
                .map(str::trim)
                .unwrap_or_default()
                .to_string();
            if parse_semver_triplet(&version).is_none() {
                return Err(VenvProbeFailure::new(
                    ProbeFailureKind::UnsupportedOutput,
                    format!(
                        "version probe output not parseable: exit={status}, out={stdout:?}, err={stderr:?}"
                    ),
                ));
            }
            Ok(version)
        }
        Err(e) => Err(e),
    }
}

/// 带帽读线程句柄（读至帽即停，防异常输出灌爆）。
fn capped_reader(
    stream: impl std::io::Read + Send + 'static,
    cap: u64,
) -> thread::JoinHandle<String> {
    thread::spawn(move || {
        let mut buf = Vec::new();
        let mut reader = stream.take(cap);
        if std::io::Read::read_to_end(&mut reader, &mut buf).is_err() {
            return String::new();
        }
        String::from_utf8_lossy(&buf).into_owned()
    })
}

/// IO 失败 → 五态归类（NotFound → Missing；PermissionDenied →
/// PermissionDenied；其余 → ProbeFailed）。
fn io_failure(e: &std::io::Error, what: impl std::fmt::Display) -> VenvProbeFailure {
    let detail = format!("{what}: {e}");
    match e.kind() {
        std::io::ErrorKind::NotFound => VenvProbeFailure::new(ProbeFailureKind::Missing, detail),
        std::io::ErrorKind::PermissionDenied => {
            VenvProbeFailure::new(ProbeFailureKind::PermissionDenied, detail)
        }
        _ => VenvProbeFailure::new(ProbeFailureKind::ProbeFailed, detail),
    }
}

/// canonicalize 失败 → io_failure（venv 根/解释器缺失即 Missing）。
fn canonicalize_or_fail(path: &Path, what: &str) -> Result<PathBuf, VenvProbeFailure> {
    path.canonicalize()
        .map_err(|e| io_failure(&e, format!("resolve {what} {}", path.display())))
}

/// ELF/脚本静态分类（魔数 + e_type；零外部命令）。
fn classify_executable(bytes: &[u8]) -> String {
    if bytes.len() >= 18 && bytes.starts_with(b"\x7fELF") {
        let e_type = u16::from_le_bytes([bytes[16], bytes[17]]);
        return match e_type {
            2 => "elf-exec".to_string(),
            3 => "elf-pie".to_string(),
            _ => "elf".to_string(),
        };
    }
    if bytes.starts_with(b"#!") {
        return "script".to_string();
    }
    "data".to_string()
}

/// owner/mode（"emo:emo 755" 形态；/etc/passwd、/etc/group 反查 best-effort，
/// 失败回退 "uidN:gidN mode" 形态——反查失败不构成探测失败）。
fn owner_mode_of(path: &Path) -> String {
    let meta = match fs::metadata(path) {
        Ok(m) => m,
        Err(_) => return "unknown".to_string(),
    };
    let mode = format!("{:o}", meta.mode() & 0o7777);
    let user = id_name_lookup("/etc/passwd", meta.uid(), 0, "uid")
        .unwrap_or_else(|| format!("uid{}", meta.uid()));
    let group = id_name_lookup("/etc/group", meta.gid(), 2, "gid")
        .unwrap_or_else(|| format!("gid{}", meta.gid()));
    format!("{user}:{group} {mode}")
}

/// `name:x:<id>:...` 形态文件的第 `id_col` 列反查。
fn id_name_lookup(file: &str, id: u32, id_col: usize, _fallback: &str) -> Option<String> {
    let text = fs::read_to_string(file).ok()?;
    for line in text.lines() {
        let cols: Vec<&str> = line.split(':').collect();
        if cols.len() > id_col && cols[id_col].parse::<u32>().ok() == Some(id) {
            return Some(cols[0].to_string());
        }
    }
    None
}

/// UNIX epoch 毫秒。
fn unix_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use std::os::unix::fs::PermissionsExt;
    use std::sync::atomic::{AtomicU32, Ordering};

    static FIXTURE_SEQ: AtomicU32 = AtomicU32::new(0);

    /// 离线零安装 fixture venv（测试内 `python3 -m venv`，含 pip →
    /// direct_deps=["pip"] 可满足；A10 实测裸建成功，零网络依赖）。
    fn make_fixture_venv() -> PathBuf {
        let seq = FIXTURE_SEQ.fetch_add(1, Ordering::SeqCst);
        let root = env::temp_dir().join(format!("rt2-venv-fixture-{}-{seq}", std::process::id()));
        let out = Command::new("python3")
            .args(["-m", "venv"])
            .arg(&root)
            .output()
            .expect("spawn python3 -m venv");
        assert!(
            out.status.success(),
            "fixture venv creation failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        root
    }

    fn fixture_config(root: &Path) -> VenvConfig {
        VenvConfig {
            path: root.to_path_buf(),
            interpreter: None,
            requirements_lock: None,
            direct_deps: vec!["pip".to_string()],
        }
    }

    fn cleanup(root: &Path) {
        let _ = fs::remove_dir_all(root);
    }

    /// 非根运行启发式：mode 000 文件可读即视为 root（root 下权限负例不成立）。
    fn running_as_root() -> bool {
        let probe = env::temp_dir().join(format!("rt2-root-check-{}", std::process::id()));
        let _ = fs::write(&probe, b"x");
        let _ = std::fs::set_permissions(&probe, std::fs::Permissions::from_mode(0o000));
        let readable = fs::read(&probe).is_ok();
        let _ = std::fs::set_permissions(&probe, std::fs::Permissions::from_mode(0o644));
        let _ = fs::remove_file(&probe);
        readable
    }

    #[test]
    fn fixture_venv_probes_available_with_full_identity() {
        let root = make_fixture_venv();
        let identity =
            probe(&fixture_config(&root), None, None).expect("fixture venv must probe available");
        // 判据 2（fixture 载体）：五要素非空
        assert_eq!(identity.status, VenvStatus::Available);
        assert_eq!(identity.interpreter_identity.sha256_hex.len(), 64);
        assert!(identity
            .interpreter_identity
            .sha256_hex
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
        assert!(!identity.interpreter_version.is_empty());
        assert!(!identity.python_version_cfg.is_empty());
        assert!(!identity.deps_capset.is_empty(), "bare venv ships pip");
        assert!(identity.deps_capset.iter().any(|(n, _)| n == "pip"));
        assert!(identity.direct_deps_satisfied);
        assert_eq!(identity.fingerprint.len(), 16);
        assert!(identity
            .fingerprint
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
        assert!(identity.venv_root.is_absolute());
        assert!(identity.interpreter_path.is_absolute());
        // 解释器 = venv 内派发路径（<venv>/bin/python，不 canonicalize 的 venv
        // 激活面）；symlink 双录在位
        assert!(identity.interpreter_path.starts_with(&identity.venv_root));
        assert!(identity.interpreter_identity.symlink_target.is_some());
        // 指纹复算一致性（C §5.1 公式）
        let recomputed = short_fingerprint(&canonical_fingerprint_json(
            &identity.interpreter_identity.sha256_hex,
            &identity.interpreter_version,
            &identity.deps_capset,
        ));
        assert_eq!(identity.fingerprint, recomputed);
        // deps_fingerprint 复算
        assert_eq!(
            identity.deps_fingerprint,
            short_fingerprint(&canonical_capset_json(&identity.deps_capset))
        );
        cleanup(&root);
    }

    #[test]
    fn real_venv_dual_branch_available_or_missing() {
        // D8.2 双分支：真实 workers/python/.venv 在 → available 正例；不在 → missing。
        let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .canonicalize()
            .expect("repo root canonicalize");
        let real = repo_root.join("workers/python/.venv");
        let lock = repo_root.join("workers/python/requirements.lock.txt");
        let config = VenvConfig {
            path: real.clone(),
            interpreter: None,
            requirements_lock: if lock.is_file() { Some(lock) } else { None },
            direct_deps: vec![
                "numpy".to_string(),
                "onnxruntime".to_string(),
                "tokenizers".to_string(),
            ],
        };
        match probe(&config, Some(&repo_root), None) {
            Ok(identity) => {
                assert_eq!(identity.status, VenvStatus::Available);
                assert_eq!(identity.interpreter_identity.sha256_hex.len(), 64);
                assert!(!identity.interpreter_version.is_empty());
                assert!(!identity.python_version_cfg.is_empty());
                assert!(identity.direct_deps_satisfied);
                assert!(identity.requirements_lock_sha256.is_some());
                assert_eq!(identity.fingerprint.len(), 16);
                // A4② 漂移双录：cfg version 与实测版本允许 patch 级漂移
                //（major.minor 必须一致，否则 probe 已拒绝）。
                let (cfg_mm, act_mm) = (
                    major_minor(&identity.python_version_cfg),
                    major_minor(&identity.interpreter_version),
                );
                assert_eq!(cfg_mm, act_mm, "real venv major.minor drift must not exist");
            }
            Err(failure) => {
                assert_eq!(
                    failure.kind,
                    ProbeFailureKind::Missing,
                    "dual branch: {}",
                    failure.detail
                );
                assert_eq!(failure.status, VenvStatus::Missing);
            }
        }
    }

    #[test]
    fn missing_interpreter_maps_to_missing_state() {
        let root = make_fixture_venv();
        fs::remove_file(root.join("bin/python")).expect("remove bin/python");
        let failure = probe(&fixture_config(&root), None, None).expect_err("must fail missing");
        assert_eq!(failure.kind, ProbeFailureKind::Missing);
        assert_eq!(failure.status, VenvStatus::Missing);
        let (category, code, retryable) = failure.kind.envelope_mapping();
        assert_eq!(category, "DEPENDENCY");
        assert_eq!(code, serde_json::json!("VENV_NOT_PROVISIONED"));
        assert!(!retryable);
        cleanup(&root);
    }

    #[test]
    fn missing_venv_root_maps_to_missing_state() {
        let root = env::temp_dir().join(format!("rt2-venv-absent-{}", std::process::id()));
        let failure = probe(&fixture_config(&root), None, None).expect_err("must fail missing");
        assert_eq!(failure.kind, ProbeFailureKind::Missing);
        cleanup(&root);
    }

    #[test]
    fn cfg_version_tamper_maps_to_identity_mismatch() {
        let root = make_fixture_venv();
        let cfg_path = root.join("pyvenv.cfg");
        let text = fs::read_to_string(&cfg_path).unwrap();
        let tampered = text
            .lines()
            .map(|l| {
                if l.trim_start().starts_with("version") {
                    "version = 3.99.0".to_string()
                } else {
                    l.to_string()
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
        fs::write(&cfg_path, tampered).unwrap();
        let failure = probe(&fixture_config(&root), None, None).expect_err("must fail mismatch");
        assert_eq!(failure.kind, ProbeFailureKind::UnsupportedOutput);
        assert_eq!(failure.status, VenvStatus::Incompatible);
        let (category, code, retryable) = failure.kind.envelope_mapping();
        assert_eq!(category, "EXECUTION");
        assert_eq!(code, serde_json::json!("VENV_IDENTITY_MISMATCH"));
        assert!(!retryable);
        cleanup(&root);
    }

    #[test]
    fn cfg_version_corrupt_maps_to_unsupported_output() {
        let root = make_fixture_venv();
        let cfg_path = root.join("pyvenv.cfg");
        let text = fs::read_to_string(&cfg_path).unwrap();
        let corrupted = text
            .lines()
            .map(|l| {
                if l.trim_start().starts_with("version") {
                    "version = not-a-version".to_string()
                } else {
                    l.to_string()
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
        fs::write(&cfg_path, corrupted).unwrap();
        let failure = probe(&fixture_config(&root), None, None).expect_err("must fail shape");
        assert_eq!(failure.kind, ProbeFailureKind::UnsupportedOutput);
        assert_eq!(failure.status, VenvStatus::Incompatible);
        cleanup(&root);
    }

    #[test]
    fn uncovered_direct_dep_maps_to_identity_mismatch() {
        let root = make_fixture_venv();
        let mut config = fixture_config(&root);
        config.direct_deps = vec!["numpy".to_string()];
        let failure = probe(&config, None, None).expect_err("bare venv lacks numpy");
        assert_eq!(failure.kind, ProbeFailureKind::UnsupportedOutput);
        assert!(failure.detail.contains("direct_deps not covered"));
        assert_eq!(
            failure.kind.envelope_mapping(),
            (
                "EXECUTION",
                serde_json::json!("VENV_IDENTITY_MISMATCH"),
                false
            )
        );
        cleanup(&root);
    }

    #[test]
    fn lock_sha256_mismatch_maps_to_identity_mismatch() {
        let root = make_fixture_venv();
        let lock = root
            .parent()
            .unwrap()
            .join(format!("rt2-venv-lock-{}.txt", std::process::id()));
        fs::write(&lock, b"numpy==2.5.4 --hash=sha256:deadbeef\n").unwrap();
        let mut config = fixture_config(&root);
        config.requirements_lock = Some(lock.clone());
        // 首探（无 expected）→ 记录 sha256
        let identity = probe(&config, None, None).expect("first probe records lock sha");
        let recorded = identity.requirements_lock_sha256.clone().unwrap();
        assert_eq!(
            recorded,
            sha256_hex(b"numpy==2.5.4 --hash=sha256:deadbeef\n")
        );
        // 重探 expected 不符 → VENV_IDENTITY_MISMATCH（stale fail-closed）
        let failure = probe(&config, None, Some("wrong")).expect_err("must fail mismatch");
        assert_eq!(failure.kind, ProbeFailureKind::UnsupportedOutput);
        assert!(failure.detail.contains("lock sha256 mismatch"));
        // 重探 expected 一致 → 通过
        probe(&config, None, Some(&recorded)).expect("matching expected passes");
        let _ = fs::remove_file(&lock);
        cleanup(&root);
    }

    #[test]
    fn missing_lock_maps_to_missing_state() {
        let root = make_fixture_venv();
        let mut config = fixture_config(&root);
        config.requirements_lock = Some(root.join("absent.lock.txt"));
        let failure = probe(&config, None, None).expect_err("lock missing");
        assert_eq!(failure.kind, ProbeFailureKind::Missing);
        cleanup(&root);
    }

    #[test]
    fn no_read_permission_maps_to_permission_denied() {
        if running_as_root() {
            // root 下权限拒绝不成立（CI runner 与本地均为非根；root 场景跳过断言）
            return;
        }
        let root = make_fixture_venv();
        let cfg_path = root.join("pyvenv.cfg");
        std::fs::set_permissions(&cfg_path, std::fs::Permissions::from_mode(0o000)).unwrap();
        let failure = probe(&fixture_config(&root), None, None).expect_err("must fail permission");
        assert_eq!(failure.kind, ProbeFailureKind::PermissionDenied);
        assert_eq!(failure.status, VenvStatus::PermissionDenied);
        let (category, code, retryable) = failure.kind.envelope_mapping();
        assert_eq!(category, "POLICY_DENIED");
        assert_eq!(code, serde_json::json!("VENV_PERMISSION_DENIED"));
        assert!(!retryable);
        std::fs::set_permissions(&cfg_path, std::fs::Permissions::from_mode(0o644)).unwrap();
        cleanup(&root);
    }

    #[test]
    fn l1_timeout_maps_to_timeout_envelope() {
        let root = make_fixture_venv();
        // 预算检查点在 run_version_probe 的 Ok(None) 轮询分支——已退出的成功
        // 子进程不判定预算（成功路径无超时语义，产品行为正确）。CI runner 上
        // python3 -V 快到首次 try_wait 前退出 → 预算从未触发 → probe Ok
        //（45071110402 实证）。测试载具面修复：bin/python 换 sleep wrapper
        // 强制子进程存活 >1ns → Ok(None) 必进 → Timeout 确定触发。
        let wrapper = root.join("bin").join("python");
        std::fs::remove_file(&wrapper).unwrap();
        std::fs::write(
            &wrapper,
            "#!/bin/sh\nsleep 0.2\nexec \"$@\"\n",
        )
        .unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&wrapper, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let failure =
            probe_with_budget(&fixture_config(&root), None, None, Duration::from_nanos(1))
                .expect_err("slow wrapper must time out");
        assert_eq!(failure.kind, ProbeFailureKind::Timeout);
        assert_eq!(failure.status, VenvStatus::ProbeFailed);
        let (category, code, retryable) = failure.kind.envelope_mapping();
        assert_eq!(category, "TIMEOUT");
        assert_eq!(code, serde_json::json!(-32002));
        assert!(!retryable);
        cleanup(&root);
    }

    #[test]
    fn d7_mapping_table_fully_asserted_no_merge() {
        // D7 表逐行断言（category/code/retryable），零兜底合并。
        let table: [(ProbeFailureKind, &str, serde_json::Value, bool); 5] = [
            (
                ProbeFailureKind::Missing,
                "DEPENDENCY",
                serde_json::json!("VENV_NOT_PROVISIONED"),
                false,
            ),
            (
                ProbeFailureKind::PermissionDenied,
                "POLICY_DENIED",
                serde_json::json!("VENV_PERMISSION_DENIED"),
                false,
            ),
            (
                ProbeFailureKind::Timeout,
                "TIMEOUT",
                serde_json::json!(-32002),
                false,
            ),
            (
                ProbeFailureKind::UnsupportedOutput,
                "EXECUTION",
                serde_json::json!("VENV_IDENTITY_MISMATCH"),
                false,
            ),
            (
                ProbeFailureKind::ProbeFailed,
                "INFRA",
                serde_json::json!("VENV_PROBE_FAILED"),
                true,
            ),
        ];
        for (kind, category, code, retryable) in table {
            assert_eq!(
                kind.envelope_mapping(),
                (category, code, retryable),
                "{kind:?}"
            );
        }
    }

    #[test]
    fn stale_detection_covers_all_four_faces() {
        let root = make_fixture_venv();
        let identity = probe(&fixture_config(&root), None, None).expect("probe ok");
        // 同一身份不 stale
        assert!(!identity.is_stale_vs(&identity));
        // 解释器 sha256 变化
        let mut other = identity.clone();
        other.interpreter_identity.sha256_hex = "0".repeat(64);
        assert!(identity.is_stale_vs(&other));
        // cfg version 变化
        let mut other = identity.clone();
        other.python_version_cfg = "9.9.9".to_string();
        assert!(identity.is_stale_vs(&other));
        // 依赖集变化
        let mut other = identity.clone();
        other.deps_fingerprint = "deadbeefdeadbeef".to_string();
        assert!(identity.is_stale_vs(&other));
        // lock 变化
        let mut other = identity.clone();
        other.requirements_lock_sha256 = Some("f".repeat(64));
        assert!(identity.is_stale_vs(&other));
        cleanup(&root);
    }

    #[test]
    fn canonical_json_key_order_is_fixed() {
        let capset = vec![("pip".to_string(), "26.2.2".to_string())];
        let canonical = canonical_fingerprint_json("aa", "3.14.10", &capset);
        // 键序固定 bin_sha256 → capset → version（字典序），不依赖 Map feature。
        assert!(canonical.starts_with("{\"bin_sha256\":"));
        assert!(canonical.contains(",\"capset\":[[\"pip\",\"26.2.2\"]],\"version\":"));
        assert!(canonical.ends_with("}"));
        // PEP 503 归一
        assert_eq!(
            normalize_dist_name("typing_extensions"),
            "typing-extensions"
        );
        assert_eq!(normalize_dist_name("HF-xet"), "hf-xet");
        assert_eq!(parse_semver_triplet("3.14.10"), Some((3, 14, 10)));
        // 双段形态容忍（patch 缺省 0）
        assert_eq!(parse_semver_triplet("3.14"), Some((3, 14, 0)));
        // ELF 分类（本测试进程自身即 ELF）
        let self_bytes = fs::read("/proc/self/exe").unwrap_or_default();
        if !self_bytes.is_empty() {
            assert!(classify_executable(&self_bytes).starts_with("elf-"));
        }
    }

    #[test]
    fn traversal_and_empty_direct_deps_rejected() {
        let root = make_fixture_venv();
        let mut config = fixture_config(&root);
        config.path = PathBuf::from("workers/../escape");
        let failure = probe(&config, None, None).expect_err("traversal rejected");
        assert_eq!(failure.kind, ProbeFailureKind::UnsupportedOutput);
        let mut config = fixture_config(&root);
        config.direct_deps = Vec::new();
        let failure = probe(&config, None, None).expect_err("empty direct_deps rejected");
        assert_eq!(failure.kind, ProbeFailureKind::UnsupportedOutput);
        cleanup(&root);
    }

    #[test]
    fn absolute_and_traversal_interpreter_rejected_at_probe() {
        // D5 冻结形状：interpreter 必须 venv 根内相对路径；绝对值在 Path::join
        // 语义下整体替换 base——派发目标逃出 venv 根而 capset/lock 对账仍按
        // venv 面通过（评审收口三面之一：sk3 登记面负例 + 本单测探测面纵深）。
        let root = make_fixture_venv();
        let mut config = fixture_config(&root);
        config.interpreter = Some("/usr/bin/python3".to_string());
        let failure = probe(&config, None, None).expect_err("absolute interpreter rejected");
        assert_eq!(failure.kind, ProbeFailureKind::UnsupportedOutput);
        assert!(
            failure.detail.contains("venv-root-relative"),
            "{}",
            failure.detail
        );
        let mut config = fixture_config(&root);
        config.interpreter = Some("bin/../python".to_string());
        let failure = probe(&config, None, None).expect_err("interpreter traversal rejected");
        assert_eq!(failure.kind, ProbeFailureKind::UnsupportedOutput);
        // 相对形态（含缺省 bin/python）不受影响：正常 available。
        let mut config = fixture_config(&root);
        config.interpreter = Some("bin/python".to_string());
        let identity = probe(&config, None, None).expect("relative interpreter accepted");
        assert_eq!(identity.status, VenvStatus::Available);
        cleanup(&root);
    }

    #[test]
    fn hyphenated_dist_info_name_parses_by_last_hyphen() {
        // PEP 376 名称段允许连字符：python-dateutil-2.9.0.dist-info 按最后一个
        // '-' 切分（旧 splitn(3) 误判三段整探拒绝的回归钉值）。
        let site = env::temp_dir().join(format!("rt2-capset-{}", std::process::id()));
        let _ = fs::remove_dir_all(&site);
        for name in [
            "python_dateutil-2.9.0.dist-info",
            "typing_extensions-4.16.0.dist-info",
            "hf_xet-1.8.0.dist-info",
            "pip-26.2.2.dist-info",
        ] {
            fs::create_dir_all(site.join(name)).expect("create dist-info dir");
        }
        let capset = parse_capset(&site).expect("hyphenated dist-info names parse");
        assert_eq!(
            capset,
            vec![
                ("hf-xet".to_string(), "1.8.0".to_string()),
                ("pip".to_string(), "26.2.2".to_string()),
                ("python-dateutil".to_string(), "2.9.0".to_string()),
                ("typing-extensions".to_string(), "4.16.0".to_string()),
            ]
        );
        // 完全无 `-` 的形态不符仍拒绝（D7 形态路径保持）。
        fs::create_dir_all(site.join("unparseable.dist-info")).unwrap();
        let failure = parse_capset(&site).expect_err("no-hyphen dist-info rejected");
        assert_eq!(failure.kind, ProbeFailureKind::UnsupportedOutput);
        let _ = fs::remove_dir_all(&site);
    }
}
