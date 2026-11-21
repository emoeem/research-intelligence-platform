//! RT-3 Tool Runtime Policy Resolver（纯函数决策层）。
//!
//! 落位与口径全部以 [RT-3-ADR-POLICY-RESOLVER.md](../../../docs/phase0/RT-3-ADR-POLICY-RESOLVER.md)
//! D1-D10 为唯一依据（本模块注释逐条引用其行号），上层策略 =
//! [NATIVE-FIRST-HYBRID-RUNTIME-STRATEGY.md](../../../docs/phase0/NATIVE-FIRST-HYBRID-RUNTIME-STRATEGY.md)
//! §3/§3.1/§3.2/§3.3/§9:225/§10:238。
//!
//! # 结构（ADR D1）
//!
//! 本模块是 `tool-engine` 内 `pub mod policy`（D1：不建独立 crate）。四维输入的
//! 既有承载全部在本 crate：`InputRisk`/`CapabilityEntry`/`Freshness`
//! （[`crate::capability`]，RT-1 落点）与 `VenvStatus`（[`crate::venv`]，RT-2
//! 落点）；决策输出的直接消费方是同 crate Adapter 层与 RT-4 迁移面。
//!
//! # 输入（ADR D2 / D10c）
//!
//! 四维词表枚举 + 宿主能力事实，全部为显式参数、零读写零探测零环境读取：
//! Host Capability 以 RT-0.5 目录数据形状注入（`CapabilityInput` 承载
//! `CapabilityStatus` + `Freshness`，由调用方经 RT-1 `CapabilityProvider::get`
//! 取 `CachedEntry` 后传入；venv 以 RT-2 五态 `VenvStatus` 传入；EgressProxy
//! 接线与 VM 就绪以布尔子事实传入）。本模块不查询 Provider、不探测宿主、
//! 不读时钟（无时间源）、不用随机源——同输入必同输出（D6 纯函数）。
//!
//! # 决策瀑布（ADR D6 R0-R7 + D7 N1-N10）
//!
//! [`derive_target`] = R0 词表闸 → R1 宿主能力闸 → R2 风险闸 → R3 信任闸 →
//! R4 隔离闸 → R5 依赖闸 → R6 目标合成（floor 合成 + §3.1 准入缺口收集），
//! 返回 Target 或 Reject；[`decide`] 在其上加声明面规则（D7 细则：N4/N6/N9
//! 在「声明值与目标冲突」语境落 Inconsistent）与 R7 交叉校验。floor 序
//! `native(0) < container(1) < vm(2)`，VM 未就绪一律 Reject 不降级（D7 N3）。
//!
//! # 输出（ADR D8）
//!
//! [`RuntimeDecision`]：verdict（assign/inconsistent/reject/not_applicable）+
//! 有序规则命中链 [`ReasonStep`]（每条含 rule id + hit + 文档/代码锚）+ inputs
//! 四维回显 + cross_check + constraints（词表 `e2:*` / `catalog:*` /
//! `permission_level=*`）+ reject（`RT3_` 码 + 九类信封 category）+ gap
//! （inconsistent 时的 RT-4 迁移输入）。
//!
//! # 只读红线（ADR D10b / 任务书红线 1）
//!
//! 本模块零库连接、零 Registry 写符号、零 Manifest 写路径、零进程启动——
//! 决策结果只返回给调用方内存；判据 8 的源码 grep 断言由测试面固化。

use serde::Serialize;

use crate::capability::{CapabilityStatus, Freshness, InputRisk, StaleReason};
use crate::venv::VenvStatus;

// ---------------------------------------------------------------------------
// 词表（ADR D2 冻结：enum + as_str/parse 双向，词表外值一律拒绝）
// ---------------------------------------------------------------------------

/// 信任档位（ADR D2；派生规则见 [`derive_trust`]/D3）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TrustLevel {
    /// 来源可信且经审核：自研，或第三方且来源/版本/许可证/身份钉值齐全
    /// （NATIVE-FIRST §3.1:68）。
    Audited,
    /// Manifest 在册可追责但身份证据不全。
    Registered,
    /// 未登记/来源不明/运行期动态取回代码（NATIVE-FIRST §10:238 不受信面）。
    /// 现仓零成员，由负例矩阵与未来登记（RT-4）消费。
    Untrusted,
}

impl TrustLevel {
    /// snake_case 词表串。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Audited => "audited",
            Self::Registered => "registered",
            Self::Untrusted => "untrusted",
        }
    }

    /// 词表串解析（词表外一律 Err，不猜默认值）。
    pub fn parse(s: &str) -> Result<Self, String> {
        match s {
            "audited" => Ok(Self::Audited),
            "registered" => Ok(Self::Registered),
            "untrusted" => Ok(Self::Untrusted),
            other => Err(format!(
                "unknown trust level {other:?}（词表 audited/registered/untrusted，RT-3-ADR D2）"
            )),
        }
    }
}

/// 依赖复杂度（ADR D2；派生规则见 [`derive_dependency_complexity`]/D4）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DependencyComplexity {
    /// 单源宿主依赖无冲突面。
    Simple,
    /// 依赖可哈希钉定且锁定可验证（RT-2 lock 面）。
    Locked,
    /// 版本/系统库冲突项目级隔离不能经济解决（NATIVE-FIRST §3.2:82 第 1 条）。
    Conflicting,
    /// 重型运行时/GPU/浏览器/大型二进制需独立生命周期（NATIVE-FIRST §3.2:83 第 2 条）。
    Heavy,
}

impl DependencyComplexity {
    /// snake_case 词表串。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Simple => "simple",
            Self::Locked => "locked",
            Self::Conflicting => "conflicting",
            Self::Heavy => "heavy",
        }
    }

    /// 词表串解析（词表外一律 Err）。
    pub fn parse(s: &str) -> Result<Self, String> {
        match s {
            "simple" => Ok(Self::Simple),
            "locked" => Ok(Self::Locked),
            "conflicting" => Ok(Self::Conflicting),
            "heavy" => Ok(Self::Heavy),
            other => Err(format!(
                "unknown dependency complexity {other:?}（词表 simple/locked/conflicting/heavy，RT-3-ADR D2）"
            )),
        }
    }
}

/// 隔离要求（ADR D2/D5）：超出 RT-0.5-E §2 全局清单之外的隔离要求。
/// 序 `none < fs < net < kernel`（kernel 蕴含 fs/net，net 蕴含 fs）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum IsolationRequirement {
    /// 无超出 E §2 全局清单的隔离要求。
    None,
    /// 文件系统边界要求（workspace 三目录/容器挂载可承载）。
    Fs,
    /// 受控出网要求（统一 Egress 承载）。
    Net,
    /// 内核级边界要求（共享内核容器不可承载，唯一合法后端 = VM）。
    Kernel,
}

impl IsolationRequirement {
    /// snake_case 词表串。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Fs => "fs",
            Self::Net => "net",
            Self::Kernel => "kernel",
        }
    }

    /// 词表串解析（词表外一律 Err）。
    pub fn parse(s: &str) -> Result<Self, String> {
        match s {
            "none" => Ok(Self::None),
            "fs" => Ok(Self::Fs),
            "net" => Ok(Self::Net),
            "kernel" => Ok(Self::Kernel),
            other => Err(format!(
                "unknown isolation requirement {other:?}（词表 none/fs/net/kernel，RT-3-ADR D2）"
            )),
        }
    }
}

/// 目标 runtime（ADR D2/D6；§3 总表行语义合法面）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Target {
    /// 宿主原生受控子进程（RT-1 Native CLI Adapter 通道）。
    NativeRust,
    /// 宿主项目级 venv / Worker 进程池（RT-2 通道）。
    PythonWorker,
    /// 按需 rootless 容器（podman_host）。
    PodmanHost,
    /// VM 内 rootless 容器（podman_vm，设计保留面；未就绪一律拒绝）。
    PodmanVm,
}

impl Target {
    /// snake_case 词表串。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NativeRust => "native_rust",
            Self::PythonWorker => "python_worker",
            Self::PodmanHost => "podman_host",
            Self::PodmanVm => "podman_vm",
        }
    }

    /// 词表串解析（词表外一律 Err；`mcp_client`/`http_api`/`cli_adapter` 等
    /// 总表行不属本 Resolver 可派发目标词表，D7 N10 落 RT3_RUNTIME_VOCAB）。
    pub fn parse(s: &str) -> Result<Self, String> {
        match s {
            "native_rust" => Ok(Self::NativeRust),
            "python_worker" => Ok(Self::PythonWorker),
            "podman_host" => Ok(Self::PodmanHost),
            "podman_vm" => Ok(Self::PodmanVm),
            other => Err(format!(
                "runtime.type {other:?} 词表外（可派发目标词表 native_rust/python_worker/podman_host/podman_vm，RT-3-ADR D2/D7 N10）"
            )),
        }
    }

    /// 是否原生档（宿主进程通道；N1/N8 硬拒绝面）。
    pub fn is_native_tier(self) -> bool {
        matches!(self, Self::NativeRust | Self::PythonWorker)
    }
}

/// 隔离下限 floor（ADR D6 floor 合成序：native(0) < container(1) < vm(2)）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Floor {
    Native,
    Container,
    Vm,
}

// ---------------------------------------------------------------------------
// D2 别名映射：文档短名 ↔ 既有 InputRisk 代码枚举（不建第二词表）
// ---------------------------------------------------------------------------

/// 文档短名（D2 表）：`trusted_low_risk`↔low、`untrusted`↔unknown、
/// `high_risk`↔high。代码与测试断言用既有 as_str 串，文档叙述用短名。
pub fn input_risk_short_alias(risk: InputRisk) -> &'static str {
    match risk {
        InputRisk::TrustedLowRisk => "low",
        InputRisk::Untrusted => "unknown",
        InputRisk::HighRisk => "high",
    }
}

/// 短名解析回既有枚举（词表外一律 Err；与 [`input_risk_short_alias`] 双向）。
pub fn input_risk_from_short_alias(s: &str) -> Result<InputRisk, String> {
    match s {
        "low" => Ok(InputRisk::TrustedLowRisk),
        "unknown" => Ok(InputRisk::Untrusted),
        "high" => Ok(InputRisk::HighRisk),
        other => Err(format!(
            "unknown input risk alias {other:?}（D2 别名词表 low/unknown/high）"
        )),
    }
}

// ---------------------------------------------------------------------------
// 派生函数（ADR D3/D4/D5：输入 = serde Manifest 值子事实，输出 = 四维枚举）
// ---------------------------------------------------------------------------

/// D3 信任派生：`origin == rip-native|internal-kit → audited`；
/// `opensource + 身份钉值在位 → audited`（第三方 §3.1:68 四要素齐）；
/// `opensource 无钉值 → registered`；origin 未声明 / license 缺 → registered
/// （fail-closed）；`untrusted` 不由本函数产出（未登记 Manifest/动态取回代码
/// 属调用方显式输入，现仓零成员，D3 论据）。
///
/// `identity_pinned` 判据：native_rust = `runtime.binary_sha256` 为 64hex；
/// podman_host = image ref 内 `@sha256:<64hex>` digest（D3）。
pub fn derive_trust(
    origin: Option<&str>,
    license_present: bool,
    identity_pinned: bool,
) -> TrustLevel {
    let Some(origin) = origin else {
        return TrustLevel::Registered;
    };
    if !license_present {
        // provenance 缺 license → registered（D3；fail-closed，不因 origin 自研豁免）。
        return TrustLevel::Registered;
    }
    match origin {
        "rip-native" => TrustLevel::Audited,
        "internal-kit" => TrustLevel::Audited,
        "opensource" if identity_pinned => TrustLevel::Audited,
        // opensource 钉值缺失/残缺 与 其余未知 origin 一律 registered（D3）。
        _ => TrustLevel::Registered,
    }
}

/// D4 依赖复杂度派生：python_worker → locked（判据 = 仓库级事实
/// `workers/python/requirements.in` + `requirements.lock.txt` 在位，RT-2 S2；
/// 未声明 venv 不改变 complexity——lock 是仓库单例事实，声明缺位是部署缝）；
/// native_rust → simple（单源宿主包/自研静态二进制）；podman_host → heavy
/// （镜像族独立生命周期语义）。conflicting/heavy 例外面保留，现仓零成员。
pub fn derive_dependency_complexity(runtime_type: &str) -> Result<DependencyComplexity, String> {
    match runtime_type {
        "python_worker" => Ok(DependencyComplexity::Locked),
        "native_rust" => Ok(DependencyComplexity::Simple),
        "podman_host" => Ok(DependencyComplexity::Heavy),
        other => Err(format!(
            "runtime.type {other:?} 无 complexity 派生判据（词表外声明，RT-3-ADR D4/D7 N10）"
        )),
    }
}

/// D5 隔离要求派生：多来源取 max（none < fs < net < kernel）。
/// - `network.mode == egress_allowlist` → 至少 net（受控出网，E §2:45 条目 11）；
/// - `network.mode == vm_local` → kernel（TOOL-CONTRACT-DRAFT.md:106 仅 VM 内部互通）；
/// - `input_risk == unknown`（代码枚举 `untrusted`）→ 至少 fs（E §1:16 隔离执行文件系统形态）；
/// - `input_risk == high`（代码枚举 `high_risk`）→ kernel（E §1:17）；
/// - secrets 非空 → 至少 fs（native 档禁 secret 注入，RT-1 D4/native_rust.rs:829-831）。
pub fn derive_isolation_requirement(
    network_mode: Option<&str>,
    input_risk: InputRisk,
    secrets_present: bool,
) -> IsolationRequirement {
    let mut req = IsolationRequirement::None;
    if network_mode == Some("egress_allowlist") {
        req = req.max(IsolationRequirement::Net);
    }
    if network_mode == Some("vm_local") {
        req = req.max(IsolationRequirement::Kernel);
    }
    if input_risk == InputRisk::HighRisk {
        req = req.max(IsolationRequirement::Kernel);
    } else if input_risk == InputRisk::Untrusted {
        req = req.max(IsolationRequirement::Fs);
    }
    if secrets_present {
        req = req.max(IsolationRequirement::Fs);
    }
    req
}

/// R7 元数据条目判定（协议底座/资产条目，不派发面）：`health.probe.type=none`
/// × `python_worker` 声明。worker 无探活面 = 登记即停用的元数据形态；现仓锚
/// rip.ops.worker-runtime known_limits「元数据条目：登记即置 DISABLED，不派发
/// 不探活（probe=none）」+ context.tags metadata。native_rust/podman_host 的
/// probe=none 是常态（无 worker ping 面），不触发本判定。
pub fn is_metadata_entry(declared_runtime: Option<&str>, probe_type: Option<&str>) -> bool {
    probe_type == Some("none") && declared_runtime == Some("python_worker")
}

// ---------------------------------------------------------------------------
// 决策输入（全部显式参数；ADR D2/D10c 注入式形状，禁直连探测）
// ---------------------------------------------------------------------------

/// 宿主能力输入（RT-0.5 目录数据形状；调用方经 RT-1 `CapabilityProvider::get`
/// 取 `CachedEntry` 后装配——本层不查询、不缓存、不复制探测逻辑，D10c）。
/// 全 owned（目录条目回显面直接转 String），调用方持有 CachedEntry 生命周期。
#[derive(Debug, Clone, PartialEq)]
pub struct CapabilityInput {
    /// 目录条目 id（= Manifest `runtime.capability_ref`）。
    pub capability_id: String,
    /// 五态状态（[`crate::capability::CapabilityStatus`]）。
    pub status: CapabilityStatus,
    /// 新鲜度（[`crate::capability::Freshness`]；stale 不可供决策，C §5.3:290）。
    pub freshness: Freshness,
    /// 指纹 16hex 或缺省。
    pub fingerprint: Option<String>,
    /// C 目录 constraints 直通（调用方从 `CapabilityEntry.constraints` 以
    /// `catalog:<code>:<detail>` 形态格式化；D8 constraints 词表）。
    pub catalog_constraints: Vec<String>,
}

/// 决策输入全集（ADR D2 + F §6:229-233 契约：Manifest 声明面 + 四维 +
/// 宿主能力事实；全部显式参数，零环境读取）。
#[derive(Debug, Clone, PartialEq)]
pub struct DecisionInput<'a> {
    /// 工具 id（回显与诊断用，决策逻辑不按 tool_id 分支——D7 否决豁免名单）。
    pub tool_id: &'a str,
    /// Manifest `runtime.type` 原文（None/空 = 缺省；词表外 = N10 RT3_RUNTIME_VOCAB）。
    pub declared_runtime: Option<&'a str>,
    /// 信任档位（None = 缺省，N10 RT3_INPUT_INCOMPLETE）。
    pub trust_level: Option<TrustLevel>,
    /// 输入风险档位（既有 [`InputRisk`] 枚举；None = 缺省）。
    pub input_risk: Option<InputRisk>,
    /// 依赖复杂度（None = 缺省）。
    pub dependency_complexity: Option<DependencyComplexity>,
    /// 隔离要求（None = 缺省）。
    pub isolation_requirement: Option<IsolationRequirement>,
    /// trust 派生来源（D3 派生路径回显，如 "provenance.origin=opensource + binary_sha256 在位"）。
    pub trust_source: Option<&'a str>,
    /// Manifest `runtime.capability_ref` 原文（在场时 R1 宿主能力闸生效）。
    pub capability_ref: Option<&'a str>,
    /// 目录条目事实（capability_ref 在场而本值为 None = 目录缺条目 → N7 拒绝）。
    pub capability: Option<CapabilityInput>,
    /// Manifest `runtime.venv` 是否声明（python_worker；RT-2 缝 4 前 16 份全裸名）。
    pub venv_declared: bool,
    /// venv 探测五态（venv_declared 时必须 available，否则 RT3_VENV_UNAVAILABLE）。
    pub venv_status: Option<VenvStatus>,
    /// venv 探测身份指纹（回显用）。
    pub venv_fingerprint: Option<&'a str>,
    /// Manifest `network.mode` 原文（N6 vm_local 判定 + constraints 回显）。
    pub network_mode: Option<&'a str>,
    /// `secrets` 非空（N8）。
    pub secrets_present: bool,
    /// Manifest `health.probe.type` 原文（元数据条目判定）。
    pub probe_type: Option<&'a str>,
    /// EgressProxy 接线子事实（net 维核验；无接线事实 fail-closed 升级容器，D5/R4）。
    pub egress_wired: bool,
    /// VM 后端就绪子事实（podman_vm adapter 未实现 = false；false 时 floor=vm 一律拒绝，N3）。
    pub vm_ready: bool,
    /// Manifest `runtime.binary` 原文（§3.1 准入：绝对路径核验）。
    pub binary_path: Option<&'a str>,
    /// Manifest `runtime.binary_sha256` 原文（§3.1 准入：64hex 钉值核验）。
    pub binary_sha256: Option<&'a str>,
    /// `runtime.argv_template` / `runtime.subcommand` 允许面在位（§3.1:69）。
    pub has_argv_or_subcommand: bool,
    /// `resources` 预算面在位（timeout_seconds + max_output_bytes；§3.1:70）。
    pub has_budget: bool,
    /// Manifest `permission_level` 原文（constraints 回显；T 档授权面不入四维，D2 否决项）。
    pub permission_level: Option<&'a str>,
}

impl<'a> DecisionInput<'a> {
    /// 宿主事实缺省基底（矩阵/集成测试装配基底；现状宿主 = VM 未就绪）。
    pub fn base(tool_id: &'a str) -> Self {
        Self {
            tool_id,
            declared_runtime: None,
            trust_level: None,
            input_risk: None,
            dependency_complexity: None,
            isolation_requirement: None,
            trust_source: None,
            capability_ref: None,
            capability: None,
            venv_declared: false,
            venv_status: None,
            venv_fingerprint: None,
            network_mode: Some("none"),
            secrets_present: false,
            probe_type: None,
            egress_wired: true,
            vm_ready: false,
            binary_path: None,
            binary_sha256: None,
            has_argv_or_subcommand: false,
            has_budget: false,
            permission_level: None,
        }
    }
}

// ---------------------------------------------------------------------------
// 输出 Schema（ADR D8）
// ---------------------------------------------------------------------------

/// 有序规则命中链条目：rule id + hit 描述 + 文档/代码锚（D8 可解释性硬要求）。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ReasonStep {
    /// 规则 id（R0-R7 / N1-N10）。
    pub rule: &'static str,
    /// 命中描述（含四维取值/判定来源回显）。
    pub hit: String,
    /// 文档/代码锚（策略 §3.x 行号 / E §1 行 / C 条目 / Manifest 字段路径）。
    pub anchor: &'static str,
}

/// verdict 四态（D8）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    /// 派生目标与声明一致且准入无缺口。
    Assign,
    /// 声明与派生目标冲突 / 准入缺口（gap = RT-4 迁移输入）。
    Inconsistent,
    /// 硬拒绝（不安全降级/能力缺失/词表外；runtime = null）。
    Reject,
    /// 元数据条目（不派发面）。
    NotApplicable,
}

impl Verdict {
    /// snake_case 串。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Assign => "assign",
            Self::Inconsistent => "inconsistent",
            Self::Reject => "reject",
            Self::NotApplicable => "not_applicable",
        }
    }
}

/// 拒绝码表（`RT3_` 前缀；先例 RT-1 `NATIVE_` 前缀码，D7）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RejectCode {
    /// 四维任一缺省/词表外（R0）。
    InputIncomplete,
    /// Manifest runtime.type 词表外声明（R7 前置）。
    RuntimeVocab,
    /// capability_ref 在场但五态非 available / stale / 目录缺条目（R1/N7）。
    CapabilityUnavailable,
    /// venv 声明在场但五态非 available（R1）。
    VenvUnavailable,
    /// floor=vm × VM 未就绪（R6/N3；禁降级）。
    VmNotReady,
    /// untrusted × 声明原生档（N1；§10:238）。
    UntrustedNativeDenied,
    /// secrets 非空 × 原生候选目标（N8；RT-1 D4 同源）。
    SecretNativeDenied,
}

impl RejectCode {
    /// `RT3_` 词表串。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::InputIncomplete => "RT3_INPUT_INCOMPLETE",
            Self::RuntimeVocab => "RT3_RUNTIME_VOCAB",
            Self::CapabilityUnavailable => "RT3_CAPABILITY_UNAVAILABLE",
            Self::VenvUnavailable => "RT3_VENV_UNAVAILABLE",
            Self::VmNotReady => "RT3_VM_NOT_READY",
            Self::UntrustedNativeDenied => "RT3_UNTRUSTED_NATIVE_DENIED",
            Self::SecretNativeDenied => "RT3_SECRET_NATIVE_DENIED",
        }
    }

    /// 九类信封 category（envelope.rs:14-23 词表内，D7 禁发明词表外 category）。
    pub fn category(self) -> &'static str {
        match self {
            Self::InputIncomplete | Self::RuntimeVocab => "VALIDATION",
            Self::CapabilityUnavailable | Self::VenvUnavailable => "DEPENDENCY",
            Self::VmNotReady => "RUNNER_UNAVAILABLE",
            Self::UntrustedNativeDenied | Self::SecretNativeDenied => "POLICY_DENIED",
        }
    }
}

impl Serialize for RejectCode {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

/// 拒绝信息（verdict=reject 时必填，D8）。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RejectInfo {
    /// `RT3_` 拒绝码。
    pub code: RejectCode,
    /// 九类信封 category。
    pub category: &'static str,
    /// 人类可读说明（含锚）。
    pub message: String,
}

/// inputs 回显（决策可复现最小集；D8）。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct InputsEcho {
    /// 信任档位词表串（缺省回显 "missing"）。
    pub trust_level: String,
    /// 输入风险：既有代码枚举串（trusted_low_risk/untrusted/high_risk，D2）。
    pub input_risk: String,
    /// 依赖复杂度词表串。
    pub dependency_complexity: String,
    /// 隔离要求词表串。
    pub isolation_requirement: String,
    /// 宿主能力子事实（capability 目录条目 / venv 五态 / null）。
    pub host_capability: Option<HostCapabilityEcho>,
}

/// 宿主能力回显两形态（D8）：capability 目录条目 或 venv 探测身份 或 null。
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
pub enum HostCapabilityEcho {
    /// capability_ref 在场：目录条目五态 + freshness + 指纹。
    Capability {
        /// 目录条目 id。
        capability_id: String,
        /// 五态词表串。
        status: String,
        /// freshness 词表串（fresh / stale(<原因>) / missing）。
        freshness: String,
        /// 指纹 16hex 或缺省。
        fingerprint: Option<String>,
    },
    /// python_worker 声明 venv：探测五态 + 指纹。
    Venv {
        /// venv 探测身份回显。
        venv: VenvEcho,
    },
}

/// venv 探测身份回显。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct VenvEcho {
    /// 五态词表串。
    pub status: String,
    /// 探测身份指纹或缺省。
    pub fingerprint: Option<String>,
}

/// 交叉校验块（D8）：声明 runtime.type 与决策一致性。
/// `consistent = (verdict == assign)`（声明=目标且准入无缺口）。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CrossCheck {
    /// Manifest 声明原文（缺省回显空串）。
    pub manifest_runtime: String,
    /// 决策整体一致性。
    pub consistent: bool,
}

/// 决策输出（D8 Schema；可序列化，同输入逐字段确定）。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RuntimeDecision {
    /// 工具 id。
    pub tool_id: String,
    /// verdict 四态。
    pub verdict: Verdict,
    /// 目标 runtime（reject/not_applicable 时 null）。
    pub runtime: Option<Target>,
    /// 有序规则命中链。
    pub reason: Vec<ReasonStep>,
    /// inputs 四维回显 + 宿主能力子事实。
    pub inputs: InputsEcho,
    /// 交叉校验块。
    pub cross_check: CrossCheck,
    /// 约束（`e2:*` + `catalog:*` 直通 + `permission_level=*`）。
    pub constraints: Vec<String>,
    /// 拒绝信息（verdict=reject 时必填）。
    pub reject: Option<RejectInfo>,
    /// 声明↔目标差距清单（verdict=inconsistent 时必填；RT-4 迁移输入）。
    pub gap: Vec<String>,
}

/// 派生目标成功态（`derive_target` Ok 支路）：目标 + §3.1 准入缺口 + 证据链。
#[derive(Debug, Clone, PartialEq)]
pub struct DerivedTarget {
    /// 合成目标。
    pub target: Target,
    /// §3.1 准入缺口（原生候选；decide 层落 Inconsistent 的 gap 原料）。
    pub admission_gaps: Vec<String>,
    /// 有序规则命中链（R0-R6）。
    pub reason: Vec<ReasonStep>,
    /// 约束。
    pub constraints: Vec<String>,
    /// inputs 回显。
    pub inputs: InputsEcho,
}

/// 派生拒绝态（`derive_target` Err 支路）：拒绝信息 + 证据链。
#[derive(Debug, Clone, PartialEq)]
pub struct RejectOutcome {
    /// 拒绝信息。
    pub reject: RejectInfo,
    /// 有序规则命中链（截至拒绝规则）。
    pub reason: Vec<ReasonStep>,
    /// 约束。
    pub constraints: Vec<String>,
    /// inputs 回显。
    pub inputs: InputsEcho,
}

// ---------------------------------------------------------------------------
// 内部：R0-R5 floor 合成（R2/R3/R4/R5 有序闸；R0/R1 前置闸）
// ---------------------------------------------------------------------------

/// 合成中间态（R0-R5 步 + floor + 原生候选；R6 由 derive_target/decide 各自消费）。
struct Synthesis {
    floor: Floor,
    /// floor=Native 时的候选目标（locked → python_worker；simple → native_rust）。
    candidate: Option<Target>,
    /// trust 档位（N1 判定用）。
    trust: TrustLevel,
    /// R0-R5 步。
    steps: Vec<ReasonStep>,
    /// constraints / inputs（decide 直通用）。
    constraints: Vec<String>,
    inputs: InputsEcho,
}

fn freshness_str(freshness: &Freshness) -> String {
    match freshness {
        Freshness::Fresh => "fresh".to_string(),
        Freshness::Stale { reason } => format!("stale({})", stale_reason_str(reason)),
    }
}

fn stale_reason_str(reason: &StaleReason) -> &'static str {
    match reason {
        StaleReason::BinaryChanged => "binary_changed",
        StaleReason::PackageChanged => "package_changed",
        StaleReason::CapabilitySetChanged => "capability_set_changed",
        StaleReason::ConfigFileChanged => "config_file_changed",
        StaleReason::ProbeFailed(_) => "probe_failed",
        StaleReason::ManualInvalidate => "manual_invalidate",
    }
}

fn echo_inputs(input: &DecisionInput) -> InputsEcho {
    let host_capability = if input.capability_ref.is_some() {
        Some(HostCapabilityEcho::Capability {
            capability_id: input.capability_ref.unwrap_or("").to_string(),
            status: input
                .capability
                .as_ref()
                .map(|c| c.status.as_str().to_string())
                .unwrap_or_else(|| "missing".to_string()),
            freshness: input
                .capability
                .as_ref()
                .map(|c| freshness_str(&c.freshness))
                .unwrap_or_else(|| "missing".to_string()),
            fingerprint: input
                .capability
                .as_ref()
                .and_then(|c| c.fingerprint.clone()),
        })
    } else if input.venv_declared {
        Some(HostCapabilityEcho::Venv {
            venv: VenvEcho {
                status: input
                    .venv_status
                    .map(|s| s.as_str().to_string())
                    .unwrap_or_else(|| "missing".to_string()),
                fingerprint: input.venv_fingerprint.map(str::to_string),
            },
        })
    } else {
        None
    };
    InputsEcho {
        trust_level: input
            .trust_level
            .map(|t| t.as_str().to_string())
            .unwrap_or_else(|| "missing".to_string()),
        input_risk: input
            .input_risk
            .map(|r| r.as_str().to_string())
            .unwrap_or_else(|| "missing".to_string()),
        dependency_complexity: input
            .dependency_complexity
            .map(|c| c.as_str().to_string())
            .unwrap_or_else(|| "missing".to_string()),
        isolation_requirement: input
            .isolation_requirement
            .map(|i| i.as_str().to_string())
            .unwrap_or_else(|| "missing".to_string()),
        host_capability,
    }
}

fn build_constraints(input: &DecisionInput) -> Vec<String> {
    let mut out = Vec::new();
    // E §2 统一强制（所有档位；E §2:31-50 原文「所有档位统一强制」）。
    out.push("e2:resource-limits".to_string());
    match input.network_mode {
        Some("egress_allowlist") => out.push("e2:egress-allowlist".to_string()),
        _ => out.push("e2:network-default-off".to_string()),
    }
    if input.has_budget {
        // 输出上限 + 截断信封（E §2 条目 10）。
        out.push("e2:output-cap".to_string());
    }
    if let Some(cap) = &input.capability {
        out.extend(cap.catalog_constraints.iter().cloned());
    }
    if let Some(p) = input.permission_level {
        out.push(format!(
            "permission_level={p}（授权面归既有 Tool Policy，不入四维；TOOL-CONTRACT-DRAFT.md:132-134）"
        ));
    }
    out
}

/// R0：四维词表/缺省闸（D6；N10a）。Ok 支路返回四维取值 + 回显步。
fn gate_r0(
    input: &DecisionInput,
) -> Result<
    (
        TrustLevel,
        InputRisk,
        DependencyComplexity,
        IsolationRequirement,
        ReasonStep,
    ),
    Box<RejectOutcome>,
> {
    let constraints = build_constraints(input);
    let inputs = echo_inputs(input);
    let missing: Vec<&str> = [
        input.trust_level.is_none().then_some("trust_level"),
        input.input_risk.is_none().then_some("input_risk"),
        input
            .dependency_complexity
            .is_none()
            .then_some("dependency_complexity"),
        input
            .isolation_requirement
            .is_none()
            .then_some("isolation_requirement"),
    ]
    .into_iter()
    .flatten()
    .collect();
    if !missing.is_empty() {
        return Err(Box::new(RejectOutcome {
            reject: RejectInfo {
                code: RejectCode::InputIncomplete,
                category: RejectCode::InputIncomplete.category(),
                message: format!(
                    "四维输入缺省：{missing:?}（词表外/缺省一律拒绝不猜默认值，RT-3-ADR D2/D6 R0/D7 N10）"
                ),
            },
            reason: vec![ReasonStep {
                rule: "R0",
                hit: format!("四维输入缺省 {missing:?} → Reject(RT3_INPUT_INCOMPLETE)"),
                anchor: "RT-3-ADR D6 R0 / D7 N10",
            }],
            constraints,
            inputs,
        }));
    }
    // 词表枚举由类型系统承载（parse 拒绝词表外串，调用方入口已校验）；
    // 本步回显四维取值 + trust 派生来源（D8 四维回显硬要求）。
    let trust = input.trust_level.unwrap();
    let risk = input.input_risk.unwrap();
    let cx = input.dependency_complexity.unwrap();
    let iso = input.isolation_requirement.unwrap();
    let source = input.trust_source.unwrap_or("调用方显式输入");
    let step = ReasonStep {
        rule: "R0",
        hit: format!(
            "四维词表齐备：trust={}（来源：{source}）、input_risk={}、dependency_complexity={}、isolation_requirement={}",
            trust.as_str(),
            risk.as_str(),
            cx.as_str(),
            iso.as_str()
        ),
        anchor: "RT-3-ADR D2/D6 R0",
    };
    Ok((trust, risk, cx, iso, step))
}

/// R1：宿主能力闸（capability 五态×freshness / venv 五态；C §5.3:290 / E §3:75 / F §7）。
fn gate_r1(input: &DecisionInput, steps: &mut Vec<ReasonStep>) -> Result<(), Box<RejectOutcome>> {
    if let Some(cap_ref) = input.capability_ref {
        match &input.capability {
            None => {
                steps.push(ReasonStep {
                    rule: "R1",
                    hit: format!("capability_ref={cap_ref} 目录缺条目（missing）→ Reject(RT3_CAPABILITY_UNAVAILABLE)"),
                    anchor: "RT-0.5-C §5.3:290 / F §7:248-257",
                });
                return Err(Box::new(RejectOutcome {
                    reject: RejectInfo {
                        code: RejectCode::CapabilityUnavailable,
                        category: RejectCode::CapabilityUnavailable.category(),
                        message: format!(
                            "capability_ref={cap_ref} 在 Host Capability Catalog 无条目（missing）：缺失能力=决策拒绝而非猜测（C §5.3:290/E §3:75）"
                        ),
                    },
                    reason: std::mem::take(steps),
                    constraints: build_constraints(input),
                    inputs: echo_inputs(input),
                }));
            }
            Some(cap) => {
                if cap.status != CapabilityStatus::Available {
                    steps.push(ReasonStep {
                        rule: "R1",
                        hit: format!(
                            "capability_ref={cap_ref} 五态非 available（status={}）→ Reject(RT3_CAPABILITY_UNAVAILABLE)",
                            cap.status.as_str()
                        ),
                        anchor: "RT-0.5-C §5.3:290 / F §7:248-257",
                    });
                    return Err(Box::new(RejectOutcome {
                        reject: RejectInfo {
                            code: RejectCode::CapabilityUnavailable,
                            category: RejectCode::CapabilityUnavailable.category(),
                            message: format!(
                                "capability_ref={cap_ref} status={}：五态非 available 不可供决策使用，不静默换实现（C §5.3:290/E §3:75/F §7:248-257）",
                                cap.status.as_str()
                            ),
                        },
                        reason: std::mem::take(steps),
                        constraints: build_constraints(input),
                        inputs: echo_inputs(input),
                    }));
                }
                if let Freshness::Stale { reason } = &cap.freshness {
                    steps.push(ReasonStep {
                        rule: "R1",
                        hit: format!(
                            "capability_ref={cap_ref} freshness=stale({}) → Reject(RT3_CAPABILITY_UNAVAILABLE)",
                            stale_reason_str(reason)
                        ),
                        anchor: "RT-0.5-C §5.3:290（stale 条目不可供 Runtime Policy 决策使用）",
                    });
                    return Err(Box::new(RejectOutcome {
                        reject: RejectInfo {
                            code: RejectCode::CapabilityUnavailable,
                            category: RejectCode::CapabilityUnavailable.category(),
                            message: format!(
                                "capability_ref={cap_ref} freshness=stale({})：stale 条目不可供决策，重探失败回退错误态语义（C §5.3:290）",
                                stale_reason_str(reason)
                            ),
                        },
                        reason: std::mem::take(steps),
                        constraints: build_constraints(input),
                        inputs: echo_inputs(input),
                    }));
                }
                steps.push(ReasonStep {
                    rule: "R1",
                    hit: format!(
                        "capability_ref={cap_ref} available+fresh → 宿主能力闸通过（fingerprint={:?}）",
                        cap.fingerprint
                    ),
                    anchor: "RT-0.5-C §5.3:290 / F §6:229-236",
                });
            }
        }
    }
    if input.venv_declared {
        match input.venv_status {
            None => {
                steps.push(ReasonStep {
                    rule: "R1",
                    hit: "python_worker 声明 venv 但无探测五态 → Reject(RT3_VENV_UNAVAILABLE)"
                        .to_string(),
                    anchor: "RT-2 D4 解释器三档 / venv.rs 五态 fail-closed",
                });
                return Err(Box::new(RejectOutcome {
                    reject: RejectInfo {
                        code: RejectCode::VenvUnavailable,
                        category: RejectCode::VenvUnavailable.category(),
                        message: "python_worker 声明 venv 但缺探测五态：探测失败零系统解释器回退（RT-2 D4）".to_string(),
                    },
                    reason: std::mem::take(steps),
                    constraints: build_constraints(input),
                    inputs: echo_inputs(input),
                }));
            }
            Some(VenvStatus::Available) => {
                steps.push(ReasonStep {
                    rule: "R1",
                    hit: format!(
                        "venv 五态 available → 闸通过（fingerprint={:?}）",
                        input.venv_fingerprint
                    ),
                    anchor: "RT-2 D4 / venv.rs:83-104",
                });
            }
            Some(status) => {
                steps.push(ReasonStep {
                    rule: "R1",
                    hit: format!(
                        "venv 五态非 available（status={}）→ Reject(RT3_VENV_UNAVAILABLE)",
                        status.as_str()
                    ),
                    anchor: "RT-2 D4 解释器三档 / venv.rs:83-104 五态 fail-closed",
                });
                return Err(Box::new(RejectOutcome {
                    reject: RejectInfo {
                        code: RejectCode::VenvUnavailable,
                        category: RejectCode::VenvUnavailable.category(),
                        message: format!(
                            "venv 五态 {}：venv 探测失败即拒绝，零系统解释器回退（RT-2 D4 解释器三档）",
                            status.as_str()
                        ),
                    },
                    reason: std::mem::take(steps),
                    constraints: build_constraints(input),
                    inputs: echo_inputs(input),
                }));
            }
        }
    }
    Ok(())
}

/// R2-R5：风险/信任/隔离/依赖四闸的有序 floor 合成（D6 R2-R5 逐行）。
/// 返回合成后的 floor 与原生候选（floor=Native 时）。
fn gate_r2_to_r5(
    trust: TrustLevel,
    risk: InputRisk,
    cx: DependencyComplexity,
    iso: IsolationRequirement,
    input: &DecisionInput,
    steps: &mut Vec<ReasonStep>,
) -> (Floor, Option<Target>) {
    let mut floor = Floor::Native;
    // R2 风险闸（E §1:13-17 逐行）。
    match risk {
        InputRisk::HighRisk => {
            floor = Floor::Vm;
            steps.push(ReasonStep {
                rule: "R2",
                hit: "input_risk=high_risk → floor=vm（高风险/对抗性需内核级边界）".to_string(),
                anchor: "RT-0.5-E §1:17",
            });
        }
        InputRisk::Untrusted => {
            floor = floor.max(Floor::Container);
            steps.push(ReasonStep {
                rule: "R2",
                hit: "input_risk=untrusted → floor≥container（来源不明不静默原生化，隔离执行 rootless Podman 优先）".to_string(),
                anchor: "RT-0.5-E §1:16",
            });
        }
        InputRisk::TrustedLowRisk => {
            steps.push(ReasonStep {
                rule: "R2",
                hit: "input_risk=trusted_low_risk → floor=0（受限宿主原生子进程可承载）"
                    .to_string(),
                anchor: "RT-0.5-E §1:13-14",
            });
        }
    }
    // R3 信任闸（§3.1:64-76 全条件；§3 总表:56-59）。
    if trust == TrustLevel::Untrusted {
        floor = floor.max(Floor::Container);
        steps.push(ReasonStep {
            rule: "R3",
            hit: "trust=untrusted → floor≥container（不受信工具不得选择 native/python 路径绕过隔离策略）".to_string(),
            anchor: "NATIVE-FIRST §3.1:68 / §3 总表:57-58 / §10:238",
        });
    } else {
        steps.push(ReasonStep {
            rule: "R3",
            hit: format!(
                "trust={} → 不设隔离下限（准入缺口后置 R6 逐条核验）",
                trust.as_str()
            ),
            anchor: "NATIVE-FIRST §3.1:64-76 / §3 总表:56-59",
        });
    }
    // R4 隔离闸（D5 派生值消费）。
    match iso {
        IsolationRequirement::Kernel => {
            floor = Floor::Vm;
            steps.push(ReasonStep {
                rule: "R4",
                hit: "isolation_requirement=kernel → floor=vm（共享内核容器不可承载，唯一合法后端 = VM）".to_string(),
                anchor: "NATIVE-FIRST §3.3:92-95 / RT-0.5-E §1:17",
            });
        }
        IsolationRequirement::Net => {
            if trust == TrustLevel::Audited && input.egress_wired {
                steps.push(ReasonStep {
                    rule: "R4",
                    hit: "isolation_requirement=net：audited 且 EgressProxy 接线在位 → 不升级（统一 Egress 承载受控出网）".to_string(),
                    anchor: "RT-0.5-E §2:45 条目 11 / RT-3-ADR D5",
                });
            } else {
                floor = floor.max(Floor::Container);
                steps.push(ReasonStep {
                    rule: "R4",
                    hit: format!(
                        "isolation_requirement=net：无 audited+接线承载事实（trust={}，egress_wired={}）→ fail-closed floor≥container",
                        trust.as_str(),
                        input.egress_wired
                    ),
                    anchor: "RT-0.5-E §2:45 条目 11 / RT-3-ADR D5/R4",
                });
            }
        }
        IsolationRequirement::Fs => {
            steps.push(ReasonStep {
                rule: "R4",
                hit:
                    "isolation_requirement=fs → 无强制升级（native workspace 三目录/容器挂载承载）"
                        .to_string(),
                anchor: "NATIVE-FIRST §3.1:72 / RT-3-ADR D5",
            });
        }
        IsolationRequirement::None => {
            steps.push(ReasonStep {
                rule: "R4",
                hit: "isolation_requirement=none → E §2 全局清单已统一强制，无额外隔离要求"
                    .to_string(),
                anchor: "RT-0.5-E §2:31-50",
            });
        }
    }
    // R5 依赖闸（§3.2:82-85 准入条件逐条）。
    let candidate = match cx {
        DependencyComplexity::Conflicting => {
            floor = floor.max(Floor::Container);
            steps.push(ReasonStep {
                rule: "R5",
                hit: "dependency_complexity=conflicting → floor≥container（版本/系统库冲突项目级隔离不能经济解决）".to_string(),
                anchor: "NATIVE-FIRST §3.2:82 第 1 条",
            });
            None
        }
        DependencyComplexity::Heavy => {
            if trust == TrustLevel::Untrusted {
                floor = Floor::Vm;
                steps.push(ReasonStep {
                    rule: "R5",
                    hit: "dependency_complexity=heavy × trust=untrusted → floor=vm（不受信+重型需 VM 级边界）".to_string(),
                    anchor: "NATIVE-FIRST §3.3:92 第 1 条",
                });
            } else {
                floor = floor.max(Floor::Container);
                steps.push(ReasonStep {
                    rule: "R5",
                    hit:
                        "dependency_complexity=heavy → floor≥container（重型运行时需独立生命周期）"
                            .to_string(),
                    anchor: "NATIVE-FIRST §3.2:83 第 2 条",
                });
            }
            None
        }
        DependencyComplexity::Locked => {
            steps.push(ReasonStep {
                rule: "R5",
                hit: "dependency_complexity=locked → 候选 python_worker（依赖可哈希钉定，workers/python lock 面在位）".to_string(),
                anchor: "NATIVE-FIRST §5:134-135 / RT-2 S2",
            });
            Some(Target::PythonWorker)
        }
        DependencyComplexity::Simple => {
            steps.push(ReasonStep {
                rule: "R5",
                hit:
                    "dependency_complexity=simple → 候选 native_rust（§3.1 全条件后置 R6 逐条核验）"
                        .to_string(),
                anchor: "NATIVE-FIRST §3.1:64-76",
            });
            Some(Target::NativeRust)
        }
    };
    (floor, candidate)
}

/// R0-R5 全程（gate_r0 → gate_r1 → gate_r2_to_r5）。
fn synthesize(input: &DecisionInput) -> Result<Synthesis, Box<RejectOutcome>> {
    let (trust, risk, cx, iso, r0_step) = gate_r0(input)?;
    let mut steps = vec![r0_step];
    gate_r1(input, &mut steps)?;
    let (floor, candidate) = gate_r2_to_r5(trust, risk, cx, iso, input, &mut steps);
    Ok(Synthesis {
        floor,
        candidate,
        trust,
        steps,
        constraints: build_constraints(input),
        inputs: echo_inputs(input),
    })
}

// ---------------------------------------------------------------------------
// R6：目标合成 + §3.1 准入核验；derive_target（Target | Reject，ADR D6）
// ---------------------------------------------------------------------------

/// 64hex 钉值核验。
fn is_sha256_hex(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
}

/// R6 原生候选 §3.1 准入核验（NATIVE-FIRST §3.1:68-74 逐条；缺口 = Inconsistent
/// 的 gap 原料，与 RT-1 adapter PREPARING 拒绝同源：native_rust.rs:330-437）。
fn admission_gaps_native(input: &DecisionInput, gaps: &mut Vec<String>) {
    match input.binary_path {
        Some(p) if p.starts_with('/') => {}
        Some(p) => gaps.push(format!(
            "runtime.binary 非绝对路径（{p:?}）：§3.1:69 固定可执行文件身份"
        )),
        None => gaps.push("runtime.binary 缺省：§3.1:69 固定可执行文件身份".to_string()),
    }
    match input.binary_sha256 {
        Some(sha) if is_sha256_hex(sha) => {}
        Some(sha) => gaps.push(format!(
            "runtime.binary_sha256 非 64hex 钉值（{sha:?}）：§3.1:68 SHA-256 发布校验"
        )),
        None => gaps.push(
            "runtime.binary_sha256 缺钉值：§3.1:68 第三方需 SHA-256/发布校验信息".to_string(),
        ),
    }
    if !input.has_argv_or_subcommand {
        gaps.push(
            "runtime 缺 argv_template/subcommand 允许面：§3.1:69 允许的子命令与参数 Schema"
                .to_string(),
        );
    }
    if !input.has_budget {
        gaps.push(
            "resources 预算面缺失（timeout_seconds/max_output_bytes）：§3.1:70 超时与输出上限"
                .to_string(),
        );
    }
    if input.network_mode == Some("egress_allowlist") && !input.egress_wired {
        gaps.push(
            "network.mode=egress_allowlist 但 EgressProxy 接线事实缺失：§3.1:72 统一 Egress Policy"
                .to_string(),
        );
    }
}

/// R6 消费合成态：floor → 目标（VM 闸 + §3.1 准入缺口收集）。
/// `prefix_steps` = 调用方已累积的命中链（R0-R5 + 声明面规则步），R6 步续写
/// 其后（derive_target 传 synth.steps.clone()；decide 传声明规则增补后的链）。
fn r6_consume(
    synth: &Synthesis,
    input: &DecisionInput,
    prefix_steps: Vec<ReasonStep>,
) -> Result<(Target, Vec<String>, Vec<ReasonStep>), Box<RejectOutcome>> {
    let mut steps = prefix_steps;
    match synth.floor {
        Floor::Vm => {
            if input.vm_ready {
                steps.push(ReasonStep {
                    rule: "R6",
                    hit: "floor=vm → 目标 podman_vm（VM 已启用）".to_string(),
                    anchor: "NATIVE-FIRST §3.3:90-97 / RT-0.5-E §1:17",
                });
                return Ok((Target::PodmanVm, Vec::new(), steps));
            }
            steps.push(ReasonStep {
                rule: "R6",
                hit: "floor=vm × VM 未就绪 → Reject(RT3_VM_NOT_READY)，禁降级 podman_host/受限宿主原生".to_string(),
                anchor: "RT-0.5-E §1:17 原文行为（未启用一律拒绝派发并报告）",
            });
            steps.push(ReasonStep {
                rule: "N3",
                hit: "floor=vm × VM 未就绪 → Reject(RT3_VM_NOT_READY)，无任何降级目标（podman_host/native 均非输出值）".to_string(),
                anchor: "RT-0.5-E §1:17 原文行为 / D7 N3",
            });
            Err(Box::new(RejectOutcome {
                reject: RejectInfo {
                    code: RejectCode::VmNotReady,
                    category: RejectCode::VmNotReady.category(),
                    message: "VM 隔离后端未就绪（podman_vm 设计保留未启用）：floor=vm 一律拒绝派发并报告，禁止降级 podman_host 或受限宿主原生（RT-0.5-E §1:17）".to_string(),
                },
                reason: steps,
                constraints: synth.constraints.clone(),
                inputs: synth.inputs.clone(),
            }))
        }
        Floor::Container => {
            steps.push(ReasonStep {
                rule: "R6",
                hit: "floor=container → 目标 podman_host（§3.2 准入理由见 R2/R3/R4/R5 命中行）"
                    .to_string(),
                anchor: "NATIVE-FIRST §3.2:78-88",
            });
            Ok((Target::PodmanHost, Vec::new(), steps))
        }
        Floor::Native => {
            let candidate = synth.candidate.unwrap_or(Target::NativeRust);
            let mut gaps = Vec::new();
            match candidate {
                Target::NativeRust => {
                    admission_gaps_native(input, &mut gaps);
                    if gaps.is_empty() {
                        steps.push(ReasonStep {
                            rule: "R6",
                            hit: "floor=native → 目标 native_rust；§3.1 准入核验通过（绝对路径+64hex 钉值+允许面+预算+egress 面）".to_string(),
                            anchor: "NATIVE-FIRST §3.1:68-74",
                        });
                    } else {
                        steps.push(ReasonStep {
                            rule: "R6",
                            hit: format!(
                                "floor=native → 候选 native_rust；§3.1 准入核验缺口 {} 项（gap 清单，与 RT-1 adapter PREPARING 拒绝同源）",
                                gaps.len()
                            ),
                            anchor: "NATIVE-FIRST §3.1:68-74 / native_rust.rs:330-437",
                        });
                    }
                }
                Target::PythonWorker => {
                    steps.push(ReasonStep {
                        rule: "R6",
                        hit: "floor=native → 目标 python_worker（可信自研 Worker、依赖可锁定、协议兼容；venv 闸见 R1，lock 面见 D4）".to_string(),
                        anchor: "NATIVE-FIRST §3 总表:57 / §5:134-135",
                    });
                }
                _ => unreachable!("floor=Native 候选只能是原生档"),
            }
            Ok((candidate, gaps, steps))
        }
    }
}

/// 派生目标（ADR D6 R0-R6：`resolve(inputs) -> Target | Reject`）。
///
/// R0 词表闸 → R1 宿主能力闸 → R2 风险闸 → R3 信任闸 → R4 隔离闸 → R5 依赖闸
/// → R6 目标合成（VM 未就绪一律 Reject；原生候选附 §3.1 准入缺口清单）。
/// 声明面规则（N1/N6/N8/N9 交叉校验）在 [`decide`]。
pub fn derive_target(input: &DecisionInput) -> Result<DerivedTarget, Box<RejectOutcome>> {
    let synth = synthesize(input)?;
    let steps = synth.steps.clone();
    let (target, gaps, steps) = r6_consume(&synth, input, steps)?;
    Ok(DerivedTarget {
        target,
        admission_gaps: gaps,
        reason: steps,
        constraints: synth.constraints,
        inputs: synth.inputs,
    })
}

// ---------------------------------------------------------------------------
// decide：R0-R7 全瀑布 + 声明面规则（ADR D6/D7/D9c：决策不因声明值偏置——
// 先派生目标再对比声明，声明面规则只决定 verdict 形态）
// ---------------------------------------------------------------------------

/// 决策主入口（ADR D6 R7 + D7 声明面细则 + D8 输出）。
///
/// 顺序（优先级从环境事实到声明冲突逐层收窄）：
/// 1. R7 元数据条目短路（不派发面 → NotApplicable）；
/// 2. R0-R5 闸（词表/宿主能力/floor 合成）失败 → Reject；
/// 3. N10a 声明缺省 → Reject(RT3_INPUT_INCOMPLETE)；N10b 词表外声明 →
///    Reject(RT3_RUNTIME_VOCAB)；
/// 4. N1 untrusted × 声明原生档 → Reject(RT3_UNTRUSTED_NATIVE_DENIED)；
/// 5. N6 vm_local 声明 × 原生档声明 → Inconsistent（D7 细则：声明值与目标
///    冲突语境，gap = vm_local→kernel→vm 迁移清单）；
/// 6. N8 secrets 非空 × 原生候选 → Reject(RT3_SECRET_NATIVE_DENIED)；
/// 7. R6 目标合成（floor=vm × 未就绪 → Reject(RT3_VM_NOT_READY)，N3）；
/// 8. R7 交叉校验：目标==声明且准入无缺口 → assign；否则 inconsistent
///    （gap 含迁移建议；unknown 源标 N4 命中步、conflicting/heavy 源标 N9）。
pub fn decide(input: &DecisionInput) -> RuntimeDecision {
    // 1. R7 元数据条目短路（probe=none × python_worker；不派发面）。
    if is_metadata_entry(input.declared_runtime, input.probe_type) {
        return RuntimeDecision {
            tool_id: input.tool_id.to_string(),
            verdict: Verdict::NotApplicable,
            runtime: None,
            reason: vec![ReasonStep {
                rule: "R7",
                hit: format!(
                    "health.probe.type={} × 声明 {} → 元数据条目（登记即停用，不派发不探活）→ NotApplicable",
                    input.probe_type.unwrap_or("none"),
                    input.declared_runtime.unwrap_or("python_worker")
                ),
                anchor: "RT-3-ADR D6 R7 / rip.ops.worker-runtime known_limits（元数据条目：登记即置 DISABLED）",
            }],
            inputs: echo_inputs(input),
            cross_check: CrossCheck {
                manifest_runtime: input.declared_runtime.unwrap_or_default().to_string(),
                consistent: false,
            },
            constraints: build_constraints(input),
            reject: None,
            gap: Vec::new(),
        };
    }
    // 2. R0-R5 闸（词表/能力/floor 合成；R6 目标合成后置——声明面规则
    //    （N1/N6/N8）须在 VM 闸之前判定，D7 细则「声明值与目标冲突语境」）。
    let synth = match synthesize(input) {
        Ok(s) => s,
        Err(rej) => return reject_decision(input, *rej),
    };
    let mut steps = synth.steps.clone();
    // 3a. N10a：runtime.type 缺省。
    let Some(declared_raw) = input
        .declared_runtime
        .map(str::trim)
        .filter(|s| !s.is_empty())
    else {
        steps.push(ReasonStep {
            rule: "N10",
            hit: "runtime.type 缺省 → Reject(RT3_INPUT_INCOMPLETE)".to_string(),
            anchor: "RT-3-ADR D6 R7 前置 / D7 N10",
        });
        return reject_decision(
            input,
            RejectOutcome {
                reject: RejectInfo {
                    code: RejectCode::InputIncomplete,
                    category: RejectCode::InputIncomplete.category(),
                    message: "runtime.type 缺省：声明面不可交叉校验，拒绝不猜默认值（D7 N10）"
                        .to_string(),
                },
                reason: steps,
                constraints: synth.constraints.clone(),
                inputs: synth.inputs.clone(),
            },
        );
    };
    // 3b. N10b：词表外声明（mcp_client/http_api/cli_adapter 等）。
    let declared_target = match Target::parse(declared_raw) {
        Ok(t) => t,
        Err(err) => {
            steps.push(ReasonStep {
                rule: "N10",
                hit: format!("runtime.type={declared_raw:?} 词表外 → Reject(RT3_RUNTIME_VOCAB)"),
                anchor: "RT-3-ADR D6 R7 前置 / D7 N10（D2 词表冻结）",
            });
            return reject_decision(
                input,
                RejectOutcome {
                    reject: RejectInfo {
                        code: RejectCode::RuntimeVocab,
                        category: RejectCode::RuntimeVocab.category(),
                        message: err,
                    },
                    reason: steps,
                    constraints: synth.constraints.clone(),
                    inputs: synth.inputs.clone(),
                },
            );
        }
    };
    // 4. N1：untrusted × 声明原生档 → 硬拒绝（§10:238 不受信工具无法选择
    //    native/python 路径绕过隔离策略）。
    if synth.trust == TrustLevel::Untrusted && declared_target.is_native_tier() {
        steps.push(ReasonStep {
            rule: "N1",
            hit: format!(
                "trust=untrusted × 声明 {} → Reject(RT3_UNTRUSTED_NATIVE_DENIED)（floor≥container 仍不等于放行原生路径）",
                declared_raw
            ),
            anchor: "NATIVE-FIRST §3.1:68 / §3 总表:57-58 / §10:238",
        });
        return reject_decision(
            input,
            RejectOutcome {
                reject: RejectInfo {
                    code: RejectCode::UntrustedNativeDenied,
                    category: RejectCode::UntrustedNativeDenied.category(),
                    message: "不受信工具不得运行在宿主原生/python 通道（§10:238）；隔离策略不因声明值放行（D7 N1）".to_string(),
                },
                reason: steps,
                constraints: synth.constraints.clone(),
                inputs: synth.inputs.clone(),
            },
        );
    }
    // 5. N6：vm_local 声明 × 原生档声明 → 声明级 Inconsistent（D7 细则）。
    if input.network_mode == Some("vm_local") && declared_target.is_native_tier() {
        steps.push(ReasonStep {
            rule: "N6",
            hit: format!(
                "network.mode=vm_local（仅 VM 内部互通）→ isolation_requirement=kernel → floor=vm；声明 {declared_raw} 宿主进程不可承载 kernel 隔离 → Inconsistent"
            ),
            anchor: "TOOL-CONTRACT-DRAFT.md:106 / NATIVE-FIRST §3.3:92-95 / RT-0.5-D:37 悬案裁决（RT-3-ADR D5/D7 N6）",
        });
        let mut gap = vec![
            "network.mode=vm_local → isolation_requirement=kernel（TOOL-CONTRACT-DRAFT.md:106 仅 VM 内部互通）".to_string(),
            "isolation=kernel → floor=vm（NATIVE-FIRST §3.3:92-95）；声明 python_worker/native_rust 宿主进程不可承载 kernel 隔离".to_string(),
        ];
        if input.vm_ready {
            gap.push(
                "迁移建议：runtime.type → podman_vm（RT-3-ADR D7 N6；目标 podman_vm 就绪）"
                    .to_string(),
            );
        } else {
            gap.push("迁移建议：runtime.type → podman_vm（RT-3-ADR D7 N6）；VM 未就绪期间一律不可派发（RT-0.5-E §1:17，RT3_VM_NOT_READY）".to_string());
        }
        return RuntimeDecision {
            tool_id: input.tool_id.to_string(),
            verdict: Verdict::Inconsistent,
            runtime: Some(Target::PodmanVm),
            reason: steps,
            inputs: synth.inputs.clone(),
            cross_check: CrossCheck {
                manifest_runtime: declared_raw.to_string(),
                consistent: false,
            },
            constraints: synth.constraints.clone(),
            reject: None,
            gap,
        };
    }
    // 6. N8：secrets 非空 × 原生候选目标 → 硬拒绝（floor 语义 = 需容器/VM 承载）。
    if input.secrets_present && synth.floor == Floor::Native {
        steps.push(ReasonStep {
            rule: "N8",
            hit: "secrets 非空 × 原生候选目标 → Reject(RT3_SECRET_NATIVE_DENIED)（floor≥container：secret 承载面需容器/VM）".to_string(),
            anchor: "RT-1 D4（native_rust.rs:829-831 同源）/ RT-3-ADR D5/D7 N8",
        });
        return reject_decision(
            input,
            RejectOutcome {
                reject: RejectInfo {
                    code: RejectCode::SecretNativeDenied,
                    category: RejectCode::SecretNativeDenied.category(),
                    message: "secrets 非空不可运行于原生档：宿主进程禁 secret 注入（RT-1 D4 同源），需容器/VM 承载（D7 N8）".to_string(),
                },
                reason: steps,
                constraints: synth.constraints.clone(),
                inputs: synth.inputs.clone(),
            },
        );
    }
    // 7. R6 目标合成（VM 闸 + §3.1 准入缺口收集；floor=vm × 未就绪 = N3）。
    let (target, admission_gaps, mut steps) = match r6_consume(&synth, input, steps) {
        Ok(ok) => ok,
        Err(rej) => return reject_decision(input, *rej),
    };
    // 8. R7 交叉校验。
    let consistent = target == declared_target && admission_gaps.is_empty();
    steps.push(ReasonStep {
        rule: "R7",
        hit: format!(
            "目标 {} vs 声明 {declared_raw} → {}",
            target.as_str(),
            if consistent {
                "assign"
            } else {
                "inconsistent（差距见 gap）"
            }
        ),
        anchor: "RT-3-ADR D6 R7 / D9c（决策不因声明值偏置：先派生目标再对比声明）",
    });
    if consistent {
        return RuntimeDecision {
            tool_id: input.tool_id.to_string(),
            verdict: Verdict::Assign,
            runtime: Some(target),
            reason: steps,
            inputs: synth.inputs.clone(),
            cross_check: CrossCheck {
                manifest_runtime: declared_raw.to_string(),
                consistent: true,
            },
            constraints: synth.constraints.clone(),
            reject: None,
            gap: Vec::new(),
        };
    }
    // inconsistent：gap = 迁移建议 + 差距清单（RT-4 迁移输入；N4/N9 命中步
    // 与 gap 源标名一致，D7 细则「声明值与目标冲突语境落 Inconsistent」）。
    let mut gap = Vec::new();
    if target == declared_target {
        // 声明与目标一致但准入有缺口（§3.1 准入缺口通道）。
        gap.extend(admission_gaps.iter().cloned());
        gap.push(format!(
            "迁移建议：补齐 §3.1 准入钉值/允许面/预算面后保持 runtime.type={declared_raw}（RT-3-ADR D6 R6）"
        ));
    } else {
        if input.input_risk == Some(InputRisk::Untrusted) {
            steps.push(ReasonStep {
                rule: "N4",
                hit: format!(
                    "input_risk=untrusted（unknown 短名）× 声明 {declared_raw} → 不静默原生化，目标 {}",
                    target.as_str()
                ),
                anchor: "RT-0.5-E §1:16 / D7 N4",
            });
            gap.push(
                "input_risk=untrusted（unknown 短名）→ 隔离执行 rootless Podman 优先，不静默原生化（RT-0.5-E §1:16，D7 N4）".to_string(),
            );
        }
        if matches!(
            input.dependency_complexity,
            Some(DependencyComplexity::Conflicting) | Some(DependencyComplexity::Heavy)
        ) {
            steps.push(ReasonStep {
                rule: "N9",
                hit: format!(
                    "dependency_complexity={} × 声明 {declared_raw} → §3.2 准入不成立，目标 {}",
                    input
                        .dependency_complexity
                        .map(|c| c.as_str())
                        .unwrap_or("?"),
                    target.as_str()
                ),
                anchor: "NATIVE-FIRST §3.2:82-83 / D7 N9",
            });
            gap.push(format!(
                "dependency_complexity={} → §3.2 准入不成立（冲突/重型需容器/VM 承载；NATIVE-FIRST §3.2:82-83，D7 N9）",
                input.dependency_complexity.map(|c| c.as_str()).unwrap_or("?")
            ));
        }
        if input.network_mode == Some("egress_allowlist") && !input.egress_wired {
            gap.push(
                "network.mode=egress_allowlist 但 EgressProxy 接线事实缺失 → fail-closed 升级容器（RT-0.5-E §2:45 条目 11 / D5/R4）".to_string(),
            );
        }
        gap.extend(admission_gaps.iter().cloned());
        gap.push(format!(
            "迁移建议：runtime.type {declared_raw} → {}（RT-3-ADR D6 R7 交叉校验）",
            target.as_str()
        ));
    }
    RuntimeDecision {
        tool_id: input.tool_id.to_string(),
        verdict: Verdict::Inconsistent,
        runtime: Some(target),
        reason: steps,
        inputs: synth.inputs.clone(),
        cross_check: CrossCheck {
            manifest_runtime: declared_raw.to_string(),
            consistent: false,
        },
        constraints: synth.constraints.clone(),
        reject: None,
        gap,
    }
}

/// reject 态 RuntimeDecision 装配（runtime=null + reject 必填，D8）。
fn reject_decision(input: &DecisionInput, outcome: RejectOutcome) -> RuntimeDecision {
    RuntimeDecision {
        tool_id: input.tool_id.to_string(),
        verdict: Verdict::Reject,
        runtime: None,
        reason: outcome.reason,
        inputs: outcome.inputs,
        cross_check: CrossCheck {
            manifest_runtime: input.declared_runtime.unwrap_or_default().to_string(),
            consistent: false,
        },
        constraints: outcome.constraints,
        reject: Some(outcome.reject),
        gap: Vec::new(),
    }
}
