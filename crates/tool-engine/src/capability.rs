//! Capability Provider 最小接线（RT-0.5-F §2/§3 形状；RT-1 ADR D2）。
//!
//! 职责：宿主**环境事实**的读取与 L0 静态探测（RT-0.5-F §1：探测只读，
//! 不执行目标二进制、不访问网络、不触发安装）。数据结构逐字段对齐
//! RT-0.5-F §2（CapabilityId/CapabilityStatus 五态/ProbeErrorKind 五态/
//! BinaryIdentity/PackageProvenance/ProbedFeature/Constraint/SecurityProfile/
//! CapabilityEntry 十二字段/CachedEntry）；接口形状对齐 F §3
//! [`CapabilityProvider`]（get/list/fingerprint/probe(L0) 最小面）。
//!
//! 本批边界（RT-1 ADR D2 裁决，完整面留 DI-1 前置批）：
//! - 种子数据 = `manifests/capabilities/host-catalog-seed.json`（RT-0.5-C §3
//!   十六条 2026-11-18 实测转录；15 available + 1 missing pdf.mutool）；
//! - probe 仅支持 [`ProbeLevel::L0Static`]：受信前缀路径 stat + 全量 sha256
//!   对照种子前缀；L1/L2/L3 请求返回 [`ProbeOutcome::NotProbed`]；
//! - 指纹纪律（RT-0.5-C §5.1:269）：`fingerprint()` 仅在 probe(L0) 实测
//!   全量 sha256 后按 F §5.1 公式计算返回；种子只录实测值（多数 16 位前缀，
//!   仅 archive.7z wrapper 有全值），**16 位前缀不升格为指纹、不编造**；
//! - TTL/失效事件完整面（F §5）不实现：`invalidate()` 只做 stale 标记，
//!   `refresh()` 为 probe 包装（stale 前条目不可供 Runtime Policy 决策）。
//!
//! 与 Tool Registry 的边界（RT-0.5-C §6）：本模块不注册工具、不承载
//! input/output_schema、不写 ToolRun/Artifact；Registry 不探测环境。

use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tool_registry::hash::sha256_hex;
use tool_registry::normalize_json;

// ---------------------------------------------------------------------------
// 基础标识与五态词表（RT-0.5-F §2）
// ---------------------------------------------------------------------------

/// 稳定能力标识：`domain.name` 两段小写（如 `media.ffmpeg`、`pdf.pdftotext`）。
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CapabilityId(Arc<str>);

impl CapabilityId {
    /// 形状校验构造（两段 `[a-z0-9][a-z0-9_-]*`；段首允许数字与 C 目录权威
    /// 对齐——archive.7z/7za/7zr 是目录实测 id，F §2「两段小写」以数据面为准，
    /// 与 validate.rs capability_ref 同规则）。
    pub fn new(s: &str) -> Result<Self, String> {
        let segs: Vec<&str> = s.split('.').collect();
        let ok = segs.len() == 2
            && segs.iter().all(|seg| {
                !seg.is_empty()
                    && seg.starts_with(|c: char| c.is_ascii_lowercase() || c.is_ascii_digit())
                    && seg.chars().all(|c| {
                        c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_'
                    })
            });
        if ok {
            Ok(Self(Arc::from(s)))
        } else {
            Err(format!(
                "capability id {s:?} must be two lowercase segments `domain.name` (RT-0.5-F 2)"
            ))
        }
    }

    /// 标识字符串。
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// 域段（`media.ffmpeg` → `media`）。
    pub fn domain(&self) -> &str {
        self.0.split('.').next().unwrap_or_default()
    }
}

impl fmt::Display for CapabilityId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl Serialize for CapabilityId {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for CapabilityId {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        Self::new(&s).map_err(serde::de::Error::custom)
    }
}

/// 能力状态五态（设计 §3.1；RT-0.5-C §2 映射）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CapabilityStatus {
    /// 在位可用（RT-0.5-C §4：15 条）。
    Available,
    /// 未安装（RT-0.5-C §4：pdf.mutool 一条）。
    Missing,
    /// 在位但环境异常（权限/兼容）。
    Incompatible,
    /// 探测失败（非"不存在"语义，F §7 五态区分硬要求）。
    ProbeFailed,
    /// 显式停用（工具事实类 disabled 不属本目录，C §3.17）。
    Disabled,
}

impl CapabilityStatus {
    /// 小写词表串（种子 JSON 序列化口径）。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Available => "available",
            Self::Missing => "missing",
            Self::Incompatible => "incompatible",
            Self::ProbeFailed => "probe_failed",
            Self::Disabled => "disabled",
        }
    }

    /// 词表串解析（种子加载用）。
    pub fn parse(s: &str) -> Result<Self, String> {
        match s {
            "available" => Ok(Self::Available),
            "missing" => Ok(Self::Missing),
            "incompatible" => Ok(Self::Incompatible),
            "probe_failed" => Ok(Self::ProbeFailed),
            "disabled" => Ok(Self::Disabled),
            other => Err(format!("unknown capability status {other:?}")),
        }
    }
}

/// 探测失败五态（设计 §3.2「探测失败不等于软件不存在」；F §2/§7）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProbeErrorKind {
    /// 受信路径/搜索根内无该名。
    Missing,
    /// 路径存在但 stat/读取被拒。
    PermissionDenied,
    /// 探测超预算。
    Timeout,
    /// 命令执行了但输出形态不符。
    UnsupportedOutput,
    /// 其余失败（身份不符、读失败等）。
    ProbeFailed,
}

impl ProbeErrorKind {
    /// 小写词表串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Missing => "missing",
            Self::PermissionDenied => "permission_denied",
            Self::Timeout => "timeout",
            Self::UnsupportedOutput => "unsupported_output",
            Self::ProbeFailed => "probe_failed",
        }
    }
}

/// 探测错误（F §3 五变体；禁止把 timeout/probe_failed 报成 missing）。
#[derive(Debug, Clone, PartialEq)]
pub enum ProbeError {
    /// 受信路径无该名（搜索根随错误报告）。
    Missing {
        /// 实际查找过的受信根/路径。
        searched_roots: Vec<PathBuf>,
    },
    /// 路径存在但访问被拒（不自动提权，F §7）。
    PermissionDenied {
        /// 被拒路径。
        path: PathBuf,
    },
    /// 超出探测预算。
    Timeout {
        /// 请求层级。
        level: ProbeLevel,
        /// 实际耗时。
        elapsed: Duration,
    },
    /// 输出形态不符（版本行/清单无法解析）。
    UnsupportedOutput {
        /// 说明。
        detail: String,
    },
    /// 其余失败（身份不符、读失败、进程异常）。
    ProbeFailed {
        /// 说明。
        detail: String,
    },
}

impl ProbeError {
    /// 五态类别。
    pub fn kind(&self) -> ProbeErrorKind {
        match self {
            Self::Missing { .. } => ProbeErrorKind::Missing,
            Self::PermissionDenied { .. } => ProbeErrorKind::PermissionDenied,
            Self::Timeout { .. } => ProbeErrorKind::Timeout,
            Self::UnsupportedOutput { .. } => ProbeErrorKind::UnsupportedOutput,
            Self::ProbeFailed { .. } => ProbeErrorKind::ProbeFailed,
        }
    }
}

impl fmt::Display for ProbeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Missing { searched_roots } => {
                write!(f, "missing (searched {:?})", searched_roots)
            }
            Self::PermissionDenied { path } => write!(f, "permission denied at {}", path.display()),
            Self::Timeout { level, elapsed } => {
                write!(f, "timeout at level {level:?} after {elapsed:?}")
            }
            Self::UnsupportedOutput { detail } => write!(f, "unsupported output: {detail}"),
            Self::ProbeFailed { detail } => write!(f, "probe failed: {detail}"),
        }
    }
}

// ---------------------------------------------------------------------------
// 身份 / 版本 / 包归属 / 能力集合（RT-0.5-F §2）
// ---------------------------------------------------------------------------

/// 二进制身份（L0 实测口径；C §5.1 取证登记为先 16 位前缀）。
#[derive(Debug, Clone, PartialEq)]
pub struct BinaryIdentity {
    /// 全量 sha256 前缀（种子实测值，16 位小写 hex）。
    pub sha256_prefix: String,
    /// 全量 sha256（仅 archive.7z wrapper 有种子实测全值；其余在 probe(L0)
    /// 实测后填充——前缀不升格为全值，不编造）。
    pub sha256_full: Option<String>,
    /// 文件类型（"elf-pie" | "shell-script" | "perl-script" | "symlink" |
    /// "elf-shared-object" 等，C §3 实测 `file` 输出转录）。
    pub file_type: String,
    /// 所有者与权限（如 "root:root 755"）。
    pub owner_mode: String,
    /// wrapper 场景 exec 目标身份（7z/7za/7zr/soffice；C §5.1:270 指纹必须
    /// 同时覆盖 wrapper 与 exec 目标）。
    pub wrapper_target: Option<Box<WrapperTarget>>,
}

/// wrapper exec 目标（路径 + 身份；C §3.4/§3.11 实测记录）。
#[derive(Debug, Clone, PartialEq)]
pub struct WrapperTarget {
    /// exec 目标绝对路径。
    pub executable_path: PathBuf,
    /// 目标身份（字段语义同 [`BinaryIdentity`]，无嵌套 wrapper）。
    pub sha256_prefix: String,
    /// 目标全量 sha256（同 [`BinaryIdentity::sha256_full`] 纪律）。
    pub sha256_full: Option<String>,
    /// 目标文件类型。
    pub file_type: String,
    /// 目标所有者与权限。
    pub owner_mode: String,
}

/// 版本信息（L1 探测回显；种子为 2026-11-18 实测转录）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VersionInfo {
    /// 归一化版本串（如 "9.0.2"）。
    pub normalized: String,
    /// 原始回显（如 "ffmpeg version 9.0.2"）。
    pub raw: String,
}

/// 包归属（实测口径 `pacman -Qo`；无法确认显式 unknown，不猜测）。
/// JSON 形状：`{"pacman": {"name": ..., "version": ...}}` 或 `"unknown"`。
#[derive(Debug, Clone, PartialEq)]
pub enum PackageProvenance {
    /// pacman 包归属。
    Pacman {
        /// 包名。
        name: String,
        /// 包版本。
        version: String,
    },
    /// 无法确认（C §3.9 pdf.mutool）。
    Unknown,
}

impl Serialize for PackageProvenance {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Pacman { name, version } => serde_json::json!({
                "pacman": {"name": name, "version": version},
            })
            .serialize(serializer),
            Self::Unknown => serializer.serialize_str("unknown"),
        }
    }
}

impl<'de> Deserialize<'de> for PackageProvenance {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let v = Value::deserialize(deserializer)?;
        if let Some(obj) = v.as_object() {
            let pacman = obj
                .get("pacman")
                .and_then(Value::as_object)
                .ok_or_else(|| {
                    serde::de::Error::custom(
                        "package_provenance object must be {\"pacman\": {\"name\", \"version\"}}",
                    )
                })?;
            let name = pacman
                .get("name")
                .and_then(Value::as_str)
                .ok_or_else(|| serde::de::Error::missing_field("name"))?;
            let version = pacman
                .get("version")
                .and_then(Value::as_str)
                .ok_or_else(|| serde::de::Error::missing_field("version"))?;
            return Ok(Self::Pacman {
                name: name.to_string(),
                version: version.to_string(),
            });
        }
        if v.as_str() == Some("unknown") {
            return Ok(Self::Unknown);
        }
        Err(serde::de::Error::custom(
            "package_provenance must be {\"pacman\": {...}} or \"unknown\"",
        ))
    }
}

/// L2 能力集合元素类型（设计 §3.1 FeatureKind）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum FeatureKind {
    /// 编码器（ffmpeg -encoders）。
    Encoder,
    /// 封装器（ffmpeg -muxers）。
    Muxer,
    /// 语言包（tesseract --list-langs）。
    Language,
    /// 格式（7z i / pandoc --list-*）。
    Format,
    /// delegate（magick -list delegate）。
    Delegate,
    /// 策略/特性开关（magick Features）。
    Policy,
    /// 组件（libreoffice Writer/Calc/Impress）。
    Component,
}

impl FeatureKind {
    /// 小写词表串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Encoder => "encoder",
            Self::Muxer => "muxer",
            Self::Language => "language",
            Self::Format => "format",
            Self::Delegate => "delegate",
            Self::Policy => "policy",
            Self::Component => "component",
        }
    }

    /// 词表串解析。
    pub fn parse(s: &str) -> Result<Self, String> {
        match s {
            "encoder" => Ok(Self::Encoder),
            "muxer" => Ok(Self::Muxer),
            "language" => Ok(Self::Language),
            "format" => Ok(Self::Format),
            "delegate" => Ok(Self::Delegate),
            "policy" => Ok(Self::Policy),
            "component" => Ok(Self::Component),
            other => Err(format!("unknown feature kind {other:?}")),
        }
    }
}

/// L2 能力集合元素（如 encoder libx264 / language chi_sim）。
#[derive(Debug, Clone, PartialEq)]
pub struct ProbedFeature {
    /// 元素类型。
    pub kind: FeatureKind,
    /// 元素名（"libx264" / "chi_sim" / "mp4" / "gs"）。
    pub name: String,
    /// 旗标（如 `["experimental"]`：ffmpeg 原生 opus/vorbis）。
    pub flags: Vec<String>,
}

/// 已知限制码（F §2 三例 + RT-0.5-C §3 实测约束面 RT-1 定稿词表）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConstraintCode {
    /// 实验性编码器（ffmpeg 原生 opus/vorbis）。
    ExperimentalEncoder,
    /// 语言包缺失（tesseract chi_tra；显式报告禁静默降级）。
    LanguageMissing,
    /// policy.xml 空壳（ImageMagick 高危面无兜底）。
    PolicyEmptyShell,
    /// wrapper 指纹须覆盖 exec 目标（7z 系/soffice）。
    WrapperFingerprint,
    /// 所在目录不在受信前缀集合（meta.exiftool /usr/bin/vendor_perl）。
    TrustPrefixExclusion,
    /// 旧实现（unzip 6.00 2009；zip64 面未知）。
    LegacyImplementation,
    /// L2 探测面不全（demuxer 未清点 / bsdtar --formats 不适用）。
    ProbeScopePartial,
    /// 同包多二进制版本联动（poppler 三件套 / ffmpeg+ffprobe）。
    VersionCoupled,
    /// 禁并发共享用户 profile（soffice 运行时隔离临时 profile）。
    ProfileIsolationRequired,
    /// 格式子集限制（7za 无 Rar / 7zr 仅 7z 系）。
    FormatSubset,
    /// 成熟替代已裁决（pdf.mutool → pdftotext/pdfinfo/pdftoppm+gs）。
    ReplacementRuled,
    /// 兼容别名弃用告警（convert → magick）。
    AliasDeprecated,
    /// delegate 注入面（magick delegates 含 curl 等外联面）。
    DelegateInjection,
    /// 输入格式域限制（pandoc 不承担 PDF 输入解析）。
    InputFormatScope,
}

impl ConstraintCode {
    /// snake_case 词表串（种子 JSON 序列化口径）。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ExperimentalEncoder => "experimental_encoder",
            Self::LanguageMissing => "language_missing",
            Self::PolicyEmptyShell => "policy_empty_shell",
            Self::WrapperFingerprint => "wrapper_fingerprint",
            Self::TrustPrefixExclusion => "trust_prefix_exclusion",
            Self::LegacyImplementation => "legacy_implementation",
            Self::ProbeScopePartial => "probe_scope_partial",
            Self::VersionCoupled => "version_coupled",
            Self::ProfileIsolationRequired => "profile_isolation_required",
            Self::FormatSubset => "format_subset",
            Self::ReplacementRuled => "replacement_ruled",
            Self::AliasDeprecated => "alias_deprecated",
            Self::DelegateInjection => "delegate_injection",
            Self::InputFormatScope => "input_format_scope",
        }
    }

    /// 词表串解析。
    pub fn parse(s: &str) -> Result<Self, String> {
        match s {
            "experimental_encoder" => Ok(Self::ExperimentalEncoder),
            "language_missing" => Ok(Self::LanguageMissing),
            "policy_empty_shell" => Ok(Self::PolicyEmptyShell),
            "wrapper_fingerprint" => Ok(Self::WrapperFingerprint),
            "trust_prefix_exclusion" => Ok(Self::TrustPrefixExclusion),
            "legacy_implementation" => Ok(Self::LegacyImplementation),
            "probe_scope_partial" => Ok(Self::ProbeScopePartial),
            "version_coupled" => Ok(Self::VersionCoupled),
            "profile_isolation_required" => Ok(Self::ProfileIsolationRequired),
            "format_subset" => Ok(Self::FormatSubset),
            "replacement_ruled" => Ok(Self::ReplacementRuled),
            "alias_deprecated" => Ok(Self::AliasDeprecated),
            "delegate_injection" => Ok(Self::DelegateInjection),
            "input_format_scope" => Ok(Self::InputFormatScope),
            other => Err(format!("unknown constraint code {other:?}")),
        }
    }
}

/// 已知限制（供 Runtime Policy Resolver / 档位路由读取）。
#[derive(Debug, Clone, PartialEq)]
pub struct Constraint {
    /// 限制码。
    pub code: ConstraintCode,
    /// 人类可读说明（C §3 各条 constraints 转录）。
    pub detail: String,
}

/// 允许的调用方式（E §1 三档路由的执行面表达）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallMode {
    /// 受限原生子进程（RT-1 NativeRustAdapter 通道）。
    RestrictedNativeSubprocess,
    /// 隔离容器（rootless Podman）。
    IsolatedContainer,
    /// VM 隔离（podman_vm，未就绪即拒绝派发）。
    VmIsolated,
}

impl CallMode {
    /// snake_case 词表串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::RestrictedNativeSubprocess => "restricted_native_subprocess",
            Self::IsolatedContainer => "isolated_container",
            Self::VmIsolated => "vm_isolated",
        }
    }

    /// 词表串解析。
    pub fn parse(s: &str) -> Result<Self, String> {
        match s {
            "restricted_native_subprocess" => Ok(Self::RestrictedNativeSubprocess),
            "isolated_container" => Ok(Self::IsolatedContainer),
            "vm_isolated" => Ok(Self::VmIsolated),
            other => Err(format!("unknown call mode {other:?}")),
        }
    }
}

/// 输入风险档位（E §1；native 档语义 = 受限原生子进程可接受的最大风险）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputRisk {
    /// 可信低风险。
    TrustedLowRisk,
    /// 来源不明（走隔离）。
    Untrusted,
    /// 高风险/对抗性（VM 档，未就绪即拒绝）。
    HighRisk,
}

impl InputRisk {
    /// snake_case 词表串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::TrustedLowRisk => "trusted_low_risk",
            Self::Untrusted => "untrusted",
            Self::HighRisk => "high_risk",
        }
    }

    /// 词表串解析。
    pub fn parse(s: &str) -> Result<Self, String> {
        match s {
            "trusted_low_risk" => Ok(Self::TrustedLowRisk),
            "untrusted" => Ok(Self::Untrusted),
            "high_risk" => Ok(Self::HighRisk),
            other => Err(format!("unknown input risk {other:?}")),
        }
    }
}

/// 安全档案（允许的调用方式、输入风险、隔离要求；E §4 本机特化裁决）。
#[derive(Debug, Clone, PartialEq)]
pub struct SecurityProfile {
    /// 允许的调用方式。
    pub allowed_call_modes: Vec<CallMode>,
    /// 受限原生档可接受的最大输入风险（更高风险按 notes/E §1 路由隔离）。
    pub max_input_risk: InputRisk,
    /// 隔离与路由要求备注（E 文档裁决引用锚）。
    pub notes: Vec<String>,
}

// ---------------------------------------------------------------------------
// CapabilityEntry / CachedEntry（RT-0.5-F §2 十二字段）
// ---------------------------------------------------------------------------

/// 能力条目（设计 §3.1 十二字段一个不缺；RT-0.5-C §2 映射表）。
///
/// missing 条目的 C 目录「—」字段以 `Option::None` 承载（executable_path/
/// version/binary_identity），fingerprint 在 probe 实测全量 sha256 前恒
/// `None`（C §5.1:269「不预生成、不编造」）。
#[derive(Debug, Clone, PartialEq)]
pub struct CapabilityEntry {
    /// 稳定能力标识。
    pub capability_id: CapabilityId,
    /// 软件名 + 发行项目（C §3 provider）。
    pub provider: String,
    /// 经验证绝对路径（missing 条目为 None）。
    pub executable_path: Option<PathBuf>,
    /// 版本（missing 条目为 None）。
    pub version: Option<VersionInfo>,
    /// 二进制身份（missing 条目为 None）。
    pub binary_identity: Option<BinaryIdentity>,
    /// 包归属。
    pub package_provenance: PackageProvenance,
    /// L2 能力集合（按 [`FeatureKind`] 分组；种子为实测转录子集）。
    pub capabilities: Vec<ProbedFeature>,
    /// 五态状态。
    pub status: CapabilityStatus,
    /// 探测时刻（ISO 日期；种子统一 2026-11-18，与 domain::time 同以字符串
    /// 承载——本 crate 依赖面无 time 库，零增量约束下不为时间戳引依赖）。
    pub probe_timestamp: String,
    /// 指纹（仅在 probe(L0) 实测全量 sha256 后计算；F §5.1 公式）。
    pub fingerprint: Option<Fingerprint>,
    /// 已知限制。
    pub constraints: Vec<Constraint>,
    /// 安全档案。
    pub security_profile: SecurityProfile,
}

/// 指纹（F §5.1 公式产出，16 位小写 hex）。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Fingerprint(pub String);

impl fmt::Display for Fingerprint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// 新鲜度（F §5：stale 条目不可供 Runtime Policy 决策与路由使用）。
#[derive(Debug, Clone, PartialEq)]
pub enum Freshness {
    /// 与目录快照一致且 probe 未发现身份变化。
    Fresh,
    /// 失效（携带原因；仅供诊断展示）。
    Stale {
        /// 失效原因。
        reason: StaleReason,
    },
}

/// 失效原因（对齐 F §5.3 失效事件族）。
#[derive(Debug, Clone, PartialEq)]
pub enum StaleReason {
    /// 二进制身份变化（前缀对照不符）。
    BinaryChanged,
    /// 包归属/版本变化。
    PackageChanged,
    /// 能力集合变化。
    CapabilitySetChanged,
    /// 配置文件变化（policy.xml 等）。
    ConfigFileChanged,
    /// 探测失败（五态之一）。
    ProbeFailed(ProbeErrorKind),
    /// 外部手动失效。
    ManualInvalidate,
}

/// 缓存读出的包装：条目 + 新鲜度 + 上次实际探到层级。
#[derive(Debug, Clone, PartialEq)]
pub struct CachedEntry {
    /// 条目。
    pub entry: CapabilityEntry,
    /// 新鲜度。
    pub freshness: Freshness,
    /// 该条目上次实际探到的层级（种子条目 = L0 静态转录）。
    pub last_probe_level: ProbeLevel,
}

// ---------------------------------------------------------------------------
// 探测分层与预算（RT-0.5-F §3/§4）
// ---------------------------------------------------------------------------

/// 探测分层（设计 §3.3）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ProbeLevel {
    /// 受信路径查找/文件元信息/身份对照（不执行目标二进制）。
    L0Static,
    /// 固定版本参数（--version 等），短超时。
    L1Version,
    /// 只读能力清单（-encoders/--list-langs 等）。
    L2Capability,
    /// 内置 fixture 无副作用验证（仅安装/升级验收或工具首次启用）。
    L3Smoke,
}

/// 协作式取消令牌（探测/执行面共用语义；进程组回收由实现保证）。
#[derive(Debug, Clone, Default)]
pub struct CancelToken(Arc<std::sync::atomic::AtomicBool>);

impl CancelToken {
    /// 触发取消。
    pub fn cancel(&self) {
        self.0.store(true, std::sync::atomic::Ordering::SeqCst);
    }

    /// 是否已请求取消。
    pub fn cancelled(&self) -> bool {
        self.0.load(std::sync::atomic::Ordering::SeqCst)
    }
}

/// 探测预算：探测命令固定、参数固定、无 Shell 拼接；短超时 + 输出上限
/// + 取消（设计 §3.2）。建议初值（F §3 注）：L0 ≤1s、L1 ≤5s、L2 ≤10s。
#[derive(Debug, Clone)]
pub struct ProbeBudget {
    /// 墙钟上限。
    pub timeout: Duration,
    /// 输出字节上限。
    pub max_output_bytes: u64,
    /// 协作式取消令牌。
    pub cancel: CancelToken,
}

impl Default for ProbeBudget {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(1),
            max_output_bytes: 64 * 1024,
            cancel: CancelToken::default(),
        }
    }
}

/// 探测结果：完整条目或「请求层级超出实现面」。
#[derive(Debug, Clone)]
pub enum ProbeOutcome {
    /// 探测成功（含 available/missing 状态判定）。Box 为实现细节（条目体量
    /// 大，clippy large_enum_variant），F §3 形状语义不变。
    Entry(Box<CapabilityEntry>),
    /// 请求层级超出本实现面（RT-1 仅 L0；完整探测留 DI-1 前置批）。
    NotProbed {
        /// 实现实际达到的层级。
        level_reached: ProbeLevel,
    },
}

/// 目录检索过滤器（F §3）。
#[derive(Debug, Clone, Default)]
pub struct CapabilityFilter {
    /// 域前缀（"pdf." / "archive." 等）。
    pub domain_prefix: Option<String>,
    /// 状态过滤。
    pub status: Option<CapabilityStatus>,
    /// 能力集合元素查询。
    pub feature: Option<FeatureQuery>,
}

/// 能力集合元素查询（如 language=chi_tra / muxer=webm）。
#[derive(Debug, Clone)]
pub struct FeatureQuery {
    /// 元素类型。
    pub kind: Option<FeatureKind>,
    /// 元素名（全匹配）。
    pub name: Option<String>,
}

/// 失效事件（F §5.3；任一命中即把旧条目标记 stale）。
#[derive(Debug, Clone)]
pub enum InvalidationEvent {
    /// 二进制身份变化。
    BinaryChanged {
        /// 能力 id。
        capability_id: CapabilityId,
        /// 旧指纹。
        old: Fingerprint,
        /// 新指纹。
        new: Fingerprint,
    },
    /// 包版本变化。
    PackageChanged {
        /// 包名。
        package: String,
        /// 旧版本。
        old_version: String,
        /// 新版本。
        new_version: String,
    },
    /// 能力集合变化（语言包/编码器/格式清单增删）。
    CapabilitySetChanged {
        /// 能力 id。
        capability_id: CapabilityId,
    },
    /// 配置文件变化（policy.xml/delegates.xml 等）。
    ConfigFileChanged {
        /// 配置路径。
        path: PathBuf,
    },
    /// 探测失败。
    ProbeFailed {
        /// 能力 id。
        capability_id: CapabilityId,
        /// 五态之一。
        kind: ProbeErrorKind,
    },
    /// 外部手动失效（None = 全量）。
    ManualInvalidate {
        /// 能力 id（可选）。
        capability_id: Option<CapabilityId>,
    },
}

/// CapabilityProvider 接口形状（F §3 签名冻结；RT-1 实现 L0 最小面）。
pub trait CapabilityProvider: Send + Sync {
    /// 分层探测：level 即请求层级；实现按 F §4 语义逐层聚合，返回该层级
    /// 完整 Entry。任一层失败即返回对应 [`ProbeError`]，不返回半截 Entry。
    fn probe(
        &self,
        id: &CapabilityId,
        level: ProbeLevel,
        budget: &ProbeBudget,
    ) -> Result<ProbeOutcome, ProbeError>;

    /// 缓存读：命中返回条目 + 新鲜度；stale 条目由调用方决定是否重探。
    fn get(&self, id: &CapabilityId) -> Option<CachedEntry>;

    /// 目录检索（按域前缀/状态/能力集合过滤）。
    fn list(&self, filter: &CapabilityFilter) -> Vec<CachedEntry>;

    /// 当前指纹（无实测全量 sha256 时 None——16 位前缀不升格为指纹）。
    fn fingerprint(&self, id: &CapabilityId) -> Option<Fingerprint>;

    /// 失效事件入队（F §5.3）；返回被新标 stale 的条目数。
    fn invalidate(&mut self, event: &InvalidationEvent) -> usize;

    /// 按需重探（受预算约束；stale 前条目不可供决策使用）。
    fn refresh(
        &mut self,
        id: &CapabilityId,
        level: ProbeLevel,
        budget: &ProbeBudget,
    ) -> Result<CachedEntry, ProbeError>;
}

// ---------------------------------------------------------------------------
// 种子文档（manifests/capabilities/host-catalog-seed.json）
// ---------------------------------------------------------------------------

/// 种子条目 JSON 形状（文件内自描述；RT-0.5-C §3 转录）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SeedEntry {
    capability_id: String,
    provider: String,
    /// missing 条目为 null。
    executable_path: Option<String>,
    /// missing 条目为 null。
    version: Option<VersionInfo>,
    /// missing 条目为 null。
    binary_identity: Option<SeedBinaryIdentity>,
    /// pacman 归属对象或字符串 "unknown"。
    package_provenance: PackageProvenance,
    capabilities: Vec<SeedFeature>,
    status: String,
    probe_timestamp: String,
    constraints: Vec<SeedConstraint>,
    security_profile: SeedSecurityProfile,
    /// missing 条目的受信根搜索名（probe(L0) 复核"仍未安装"用）。
    #[serde(default)]
    search_name: Option<String>,
}

/// 种子二进制身份 JSON 形状。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SeedBinaryIdentity {
    sha256_prefix: String,
    /// 全量值（仅实测在案者，如 archive.7z wrapper）。
    #[serde(default)]
    sha256_full: Option<String>,
    file_type: String,
    owner_mode: String,
    #[serde(default)]
    wrapper_target: Option<SeedWrapperTarget>,
}

/// 种子 wrapper exec 目标 JSON 形状（sha256_prefix 可空：7za/7zr 的 exec
/// 目标前缀本批未取证，照录不编造——对照跳过、实测仍落全值）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SeedWrapperTarget {
    executable_path: String,
    #[serde(default)]
    sha256_prefix: Option<String>,
    #[serde(default)]
    sha256_full: Option<String>,
    file_type: String,
    owner_mode: String,
}

/// 种子能力集合元素 JSON 形状。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SeedFeature {
    kind: String,
    name: String,
    #[serde(default)]
    flags: Vec<String>,
}

/// 种子限制 JSON 形状。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SeedConstraint {
    code: String,
    detail: String,
}

/// 种子安全档案 JSON 形状。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SeedSecurityProfile {
    allowed_call_modes: Vec<String>,
    max_input_risk: String,
    #[serde(default)]
    notes: Vec<String>,
}

/// 种子文档 JSON 形状（pub：from_seed_document 入参；测试构造用）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SeedDocument {
    schema_version: String,
    source: String,
    entries: Vec<SeedEntry>,
}

fn seed_entry_to_entry(seed: &SeedEntry) -> Result<CapabilityEntry, String> {
    let capability_id = CapabilityId::new(&seed.capability_id)?;
    let status = CapabilityStatus::parse(&seed.status)?;
    let binary_identity = seed
        .binary_identity
        .as_ref()
        .map(|b| {
            if b.sha256_prefix.len() != 16
                || !b
                    .sha256_prefix
                    .chars()
                    .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
            {
                return Err(format!(
                    "{}: sha256_prefix must be 16 lowercase hex (C 5.1 取证口径)",
                    seed.capability_id
                ));
            }
            if let Some(full) = &b.sha256_full {
                if full.len() != 64
                    || !full
                        .chars()
                        .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
                {
                    return Err(format!(
                        "{}: sha256_full must be 64 lowercase hex",
                        seed.capability_id
                    ));
                }
                if !full.starts_with(&b.sha256_prefix) {
                    return Err(format!(
                        "{}: sha256_full does not start with recorded prefix (fabrication guard)",
                        seed.capability_id
                    ));
                }
            }
            let wrapper_target = b
                .wrapper_target
                .as_ref()
                .map(|w| {
                    if let Some(prefix) = &w.sha256_prefix {
                        if prefix.len() != 16
                            || !prefix
                                .chars()
                                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
                        {
                            return Err(format!(
                                "{}: wrapper_target.sha256_prefix must be 16 lowercase hex",
                                seed.capability_id
                            ));
                        }
                    }
                    Ok::<WrapperTarget, String>(WrapperTarget {
                        executable_path: PathBuf::from(&w.executable_path),
                        sha256_prefix: w.sha256_prefix.clone().unwrap_or_default(),
                        sha256_full: w.sha256_full.clone(),
                        file_type: w.file_type.clone(),
                        owner_mode: w.owner_mode.clone(),
                    })
                })
                .transpose()?;
            Ok::<BinaryIdentity, String>(BinaryIdentity {
                sha256_prefix: b.sha256_prefix.clone(),
                sha256_full: b.sha256_full.clone(),
                file_type: b.file_type.clone(),
                owner_mode: b.owner_mode.clone(),
                wrapper_target: wrapper_target.map(Box::new),
            })
        })
        .transpose()?;
    let capabilities = seed
        .capabilities
        .iter()
        .map(|f| {
            Ok::<ProbedFeature, String>(ProbedFeature {
                kind: FeatureKind::parse(&f.kind)?,
                name: f.name.clone(),
                flags: f.flags.clone(),
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let constraints = seed
        .constraints
        .iter()
        .map(|c| {
            Ok::<Constraint, String>(Constraint {
                code: ConstraintCode::parse(&c.code)?,
                detail: c.detail.clone(),
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let allowed_call_modes = seed
        .security_profile
        .allowed_call_modes
        .iter()
        .map(|m| CallMode::parse(m))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(CapabilityEntry {
        capability_id,
        provider: seed.provider.clone(),
        executable_path: seed.executable_path.as_ref().map(PathBuf::from),
        version: seed.version.clone(),
        binary_identity,
        package_provenance: seed.package_provenance.clone(),
        capabilities,
        status,
        probe_timestamp: seed.probe_timestamp.clone(),
        fingerprint: None,
        constraints,
        security_profile: SecurityProfile {
            allowed_call_modes,
            max_input_risk: InputRisk::parse(&seed.security_profile.max_input_risk)?,
            notes: seed.security_profile.notes.clone(),
        },
    })
}

// ---------------------------------------------------------------------------
// StaticCatalogProvider（RT-1 ADR D2：种子加载 + 受信前缀路径存在性 +
// sha256 对照；L0 最小面）
// ---------------------------------------------------------------------------

/// 实测身份缓存（probe(L0) 成功后写入；指纹仅由此产生）。
#[derive(Debug, Clone)]
struct MeasuredIdentity {
    bin_sha256_full: String,
    target_sha256_full: Option<String>,
    fingerprint: Fingerprint,
}

/// 静态目录 Provider（种子 = RT-0.5-C §3 十六条转录）。
pub struct StaticCatalogProvider {
    inner: Mutex<ProviderInner>,
    trusted_roots: Vec<PathBuf>,
}

struct ProviderInner {
    entries: BTreeMap<CapabilityId, CapabilityEntry>,
    /// missing 条目的受信根搜索名（seed.search_name；missing 复核用）。
    search_names: BTreeMap<CapabilityId, String>,
    measured: BTreeMap<CapabilityId, MeasuredIdentity>,
    stale: BTreeMap<CapabilityId, StaleReason>,
}

impl fmt::Debug for StaticCatalogProvider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let inner = self.lock();
        f.debug_struct("StaticCatalogProvider")
            .field("entries", &inner.entries.len())
            .field("measured", &inner.measured.len())
            .field("stale", &inner.stale.len())
            .finish()
    }
}

/// 单文件 L0 实测的失败信号（区分五态错误与身份不符：后者由调用方标 stale）。
enum MeasureFail {
    /// 五态之一（Missing/PermissionDenied/ProbeFailed）。
    Probe(ProbeError),
    /// 身份不符（前缀对照失败；对应 F §5.3 BinaryChanged → 标 stale）。
    IdentityMismatch {
        path: PathBuf,
        expected: String,
        measured: String,
    },
}

impl StaticCatalogProvider {
    /// 受信前缀默认集（设计 §3.2 + probe-path-audit verdict：/usr/local/bin、
    /// /usr/bin 及 usrmerge 别名 /usr/sbin、/sbin、/bin）。
    pub const DEFAULT_TRUSTED_ROOTS: &'static [&'static str] =
        &["/usr/local/bin", "/usr/bin", "/bin", "/usr/sbin", "/sbin"];

    /// 从种子文件加载（16 条全量校验：形状/词表/前缀/全值一致性）。
    pub fn load(seed_path: &Path) -> Result<Self, String> {
        let text = std::fs::read_to_string(seed_path)
            .map_err(|e| format!("seed read failed {}: {e}", seed_path.display()))?;
        let doc: SeedDocument =
            serde_json::from_str(&text).map_err(|e| format!("seed parse failed: {e}"))?;
        if doc.schema_version != "1" {
            return Err(format!(
                "seed schema_version {:?} unsupported (expect \"1\")",
                doc.schema_version
            ));
        }
        Self::from_seed_document(doc)
    }

    /// 从内存种子文档构造（测试与转录校验面）。
    pub fn from_seed_document(doc: SeedDocument) -> Result<Self, String> {
        let mut entries = BTreeMap::new();
        let mut search_names = BTreeMap::new();
        for seed in &doc.entries {
            let entry = seed_entry_to_entry(seed)?;
            if entries.insert(entry.capability_id.clone(), entry).is_some() {
                return Err(format!("duplicate capability id {}", seed.capability_id));
            }
            if let Some(name) = &seed.search_name {
                search_names.insert(CapabilityId::new(&seed.capability_id)?, name.clone());
            }
        }
        Ok(Self {
            inner: Mutex::new(ProviderInner {
                entries,
                search_names,
                measured: BTreeMap::new(),
                stale: BTreeMap::new(),
            }),
            trusted_roots: Self::DEFAULT_TRUSTED_ROOTS
                .iter()
                .map(PathBuf::from)
                .collect(),
        })
    }

    /// 种子条目数。
    pub fn len(&self) -> usize {
        self.lock().entries.len()
    }

    /// 种子是否为空。
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn lock(&self) -> MutexGuard<'_, ProviderInner> {
        self.inner.lock().expect("capability provider mutex")
    }

    /// 指纹计算（F §5.1 公式）：sha256(canonical_json({bin_sha256, capset,
    /// version[, target_sha256]})) 前 16 位。capset 为能力集合按 (kind,name)
    /// 排序后的 canonical 序列化；wrapper 条目另含 target_sha256。
    fn compute_fingerprint(
        bin_sha256_full: &str,
        target_sha256_full: Option<&str>,
        version: Option<&VersionInfo>,
        capabilities: &[ProbedFeature],
    ) -> Fingerprint {
        let mut capset: Vec<&ProbedFeature> = capabilities.iter().collect();
        capset.sort_by(|a, b| {
            (a.kind.as_str(), a.name.as_str()).cmp(&(b.kind.as_str(), b.name.as_str()))
        });
        let capset_value: Vec<Value> = capset
            .iter()
            .map(|f| {
                serde_json::json!({
                    "flags": f.flags.iter().collect::<Vec<_>>(),
                    "kind": f.kind.as_str(),
                    "name": f.name,
                })
            })
            .collect();
        let mut obj = serde_json::json!({
            "bin_sha256": bin_sha256_full,
            "capset": capset_value,
        });
        if let Some(target) = target_sha256_full {
            obj["target_sha256"] = serde_json::json!(target);
        }
        if let Some(v) = version {
            obj["version"] = serde_json::json!(v.normalized);
        }
        Fingerprint(sha256_hex(normalize_json(&obj).as_bytes())[..16].to_string())
    }

    /// 单文件 L0 实测：stat + 全量 sha256 + 前缀对照。
    /// 返回 Ok(Some(full))=对照通过；Err=五态之一（禁止把失败报成 missing）。
    fn measure_path(
        path: &Path,
        expect_prefix: Option<&str>,
    ) -> Result<Option<String>, MeasureFail> {
        let metadata = match std::fs::metadata(path) {
            Ok(m) => m,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Err(MeasureFail::Probe(ProbeError::Missing {
                    searched_roots: vec![path.to_path_buf()],
                }));
            }
            Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
                return Err(MeasureFail::Probe(ProbeError::PermissionDenied {
                    path: path.to_path_buf(),
                }));
            }
            Err(e) => {
                return Err(MeasureFail::Probe(ProbeError::ProbeFailed {
                    detail: format!("stat failed {}: {e}", path.display()),
                }));
            }
        };
        if !metadata.is_file() {
            return Err(MeasureFail::Probe(ProbeError::ProbeFailed {
                detail: format!("{} is not a regular file", path.display()),
            }));
        }
        let bytes = std::fs::read(path).map_err(|e| {
            if e.kind() == std::io::ErrorKind::PermissionDenied {
                MeasureFail::Probe(ProbeError::PermissionDenied {
                    path: path.to_path_buf(),
                })
            } else {
                MeasureFail::Probe(ProbeError::ProbeFailed {
                    detail: format!("read failed {}: {e}", path.display()),
                })
            }
        })?;
        let full = sha256_hex(&bytes);
        if let Some(prefix) = expect_prefix {
            if !full.starts_with(prefix) {
                return Err(MeasureFail::IdentityMismatch {
                    path: path.to_path_buf(),
                    expected: prefix.to_string(),
                    measured: full,
                });
            }
        }
        Ok(Some(full))
    }

    /// probe(L0) 实测的全量 sha256（manifest 钉值二道校验用；未实测为 None）。
    pub fn measured_sha256(&self, id: &CapabilityId) -> Option<String> {
        self.lock()
            .measured
            .get(id)
            .map(|m| m.bin_sha256_full.clone())
    }

    /// probe(L0) 实测的 wrapper exec 目标全量 sha256（wrapper 双钉二道校验用；
    /// 未实测/非 wrapper 为 None）。
    pub fn measured_target_sha256(&self, id: &CapabilityId) -> Option<String> {
        self.lock()
            .measured
            .get(id)
            .and_then(|m| m.target_sha256_full.clone())
    }

    /// stale 标记辅助（probe 失败路径统一调用）。
    fn mark_stale(&self, id: &CapabilityId, reason: StaleReason) {
        let mut inner = self.lock();
        inner.stale.insert(id.clone(), reason);
    }
}

impl CapabilityProvider for StaticCatalogProvider {
    fn probe(
        &self,
        id: &CapabilityId,
        level: ProbeLevel,
        budget: &ProbeBudget,
    ) -> Result<ProbeOutcome, ProbeError> {
        // RT-1 最小面只支持 L0（完整探测留 DI-1 前置批，F §8.1）。
        if level != ProbeLevel::L0Static {
            return Ok(ProbeOutcome::NotProbed {
                level_reached: ProbeLevel::L0Static,
            });
        }
        if budget.cancel.cancelled() {
            return Err(ProbeError::Timeout {
                level,
                elapsed: Duration::ZERO,
            });
        }
        let entry = {
            let inner = self.lock();
            inner
                .entries
                .get(id)
                .cloned()
                .ok_or_else(|| ProbeError::Missing {
                    searched_roots: self.trusted_roots.clone(),
                })?
        };
        match entry.status {
            CapabilityStatus::Missing => {
                // 复核"仍未安装"：受信根搜索 recorded search_name；仍缺 → 原样
                // 返回 missing 条目；出现候选 → 目录过期（不编造版本/身份，
                // 交由完整探测面刷新）。
                let search_name = {
                    let inner = self.lock();
                    inner.search_names.get(id).cloned()
                };
                if let Some(name) = &search_name {
                    for root in &self.trusted_roots {
                        let candidate = root.join(name);
                        if candidate.is_file() {
                            self.mark_stale(id, StaleReason::BinaryChanged);
                            return Err(ProbeError::ProbeFailed {
                                detail: format!(
                                    "seed recorded missing but candidate found at {}; catalog refresh required (RT-0.5-C 4:250 口径)",
                                    candidate.display()
                                ),
                            });
                        }
                    }
                }
                Ok(ProbeOutcome::Entry(Box::new(entry)))
            }
            _ => {
                let path =
                    entry
                        .executable_path
                        .clone()
                        .ok_or_else(|| ProbeError::ProbeFailed {
                            detail: "available entry without executable_path".to_string(),
                        })?;
                let expect_prefix = entry
                    .binary_identity
                    .as_ref()
                    .map(|b| b.sha256_prefix.clone());
                let bin_full = Self::measure_path(&path, expect_prefix.as_deref())
                    .map_err(|f| match f {
                        MeasureFail::Probe(e) => e,
                        MeasureFail::IdentityMismatch {
                            path,
                            expected,
                            measured,
                        } => {
                            self.mark_stale(id, StaleReason::BinaryChanged);
                            ProbeError::ProbeFailed {
                                detail: format!(
                                    "sha256 prefix mismatch at {}: expected {expected}, measured {} (binary changed since catalog snapshot; entry marked stale, F §5.3)",
                                    path.display(),
                                    &measured[..16.min(measured.len())]
                                ),
                            }
                        }
                    })?;
                // wrapper 双钉（C §5.1:270）：wrapper 与 exec 目标两者都对照。
                let mut target_full = None;
                if let Some(target) = entry
                    .binary_identity
                    .as_ref()
                    .and_then(|b| b.wrapper_target.as_deref())
                {
                    let expect = if target.sha256_prefix.is_empty() {
                        None
                    } else {
                        Some(target.sha256_prefix.as_str())
                    };
                    target_full = Self::measure_path(&target.executable_path, expect)
                        .map_err(|f| match f {
                            MeasureFail::Probe(e) => e,
                            MeasureFail::IdentityMismatch {
                                path,
                                expected,
                                measured,
                            } => {
                                self.mark_stale(id, StaleReason::BinaryChanged);
                                ProbeError::ProbeFailed {
                                    detail: format!(
                                        "wrapper target sha256 prefix mismatch at {}: expected {expected}, measured {} (exec target changed since catalog snapshot; entry marked stale, F §5.3)",
                                        path.display(),
                                        &measured[..16.min(measured.len())]
                                    ),
                                }
                            }
                        })?;
                }
                // 指纹仅在实测全量 sha256 后计算（前缀不升格）。
                let fingerprint = match (&bin_full, entry.version.as_ref()) {
                    (Some(bin), Some(version)) => Some(Self::compute_fingerprint(
                        bin,
                        target_full.as_deref(),
                        Some(version),
                        &entry.capabilities,
                    )),
                    _ => None,
                };
                if let (Some(bin), Some(fp)) = (&bin_full, &fingerprint) {
                    let mut inner = self.lock();
                    inner.measured.insert(
                        id.clone(),
                        MeasuredIdentity {
                            bin_sha256_full: bin.clone(),
                            target_sha256_full: target_full.clone(),
                            fingerprint: fp.clone(),
                        },
                    );
                    inner.stale.remove(id);
                }
                // 返回实测后的条目（全量 sha 填充、指纹挂载）。
                let mut measured_entry = entry;
                if let Some(bin) = &bin_full {
                    if let Some(identity) = &mut measured_entry.binary_identity {
                        identity.sha256_full = Some(bin.clone());
                    }
                }
                if let (Some(target), Some(full)) = (
                    measured_entry
                        .binary_identity
                        .as_mut()
                        .and_then(|b| b.wrapper_target.as_mut()),
                    &target_full,
                ) {
                    target.sha256_full = Some(full.clone());
                }
                measured_entry.fingerprint = fingerprint;
                Ok(ProbeOutcome::Entry(Box::new(measured_entry)))
            }
        }
    }

    fn get(&self, id: &CapabilityId) -> Option<CachedEntry> {
        let inner = self.lock();
        let entry = inner.entries.get(id)?.clone();
        let fingerprint = inner
            .measured
            .get(id)
            .map(|m| m.fingerprint.clone())
            .or(entry.fingerprint.clone());
        let mut entry = entry;
        entry.fingerprint = fingerprint;
        let freshness = match inner.stale.get(id) {
            Some(reason) => Freshness::Stale {
                reason: reason.clone(),
            },
            None => Freshness::Fresh,
        };
        Some(CachedEntry {
            entry,
            freshness,
            last_probe_level: ProbeLevel::L0Static,
        })
    }

    fn list(&self, filter: &CapabilityFilter) -> Vec<CachedEntry> {
        let inner = self.lock();
        inner
            .entries
            .iter()
            .filter(|(id, entry)| {
                if let Some(prefix) = &filter.domain_prefix {
                    if !id.domain().starts_with(prefix.trim_end_matches('.')) {
                        return false;
                    }
                }
                if let Some(status) = filter.status {
                    if entry.status != status {
                        return false;
                    }
                }
                if let Some(q) = &filter.feature {
                    let hit = entry.capabilities.iter().any(|f| {
                        q.kind.map(|k| f.kind == k).unwrap_or(true)
                            && q.name.as_deref().map(|n| f.name == n).unwrap_or(true)
                    });
                    if !hit {
                        return false;
                    }
                }
                true
            })
            .map(|(id, entry)| {
                let fingerprint = inner
                    .measured
                    .get(id)
                    .map(|m| m.fingerprint.clone())
                    .or_else(|| entry.fingerprint.clone());
                let mut entry = entry.clone();
                entry.fingerprint = fingerprint;
                let freshness = match inner.stale.get(id) {
                    Some(reason) => Freshness::Stale {
                        reason: reason.clone(),
                    },
                    None => Freshness::Fresh,
                };
                CachedEntry {
                    entry,
                    freshness,
                    last_probe_level: ProbeLevel::L0Static,
                }
            })
            .collect()
    }

    fn fingerprint(&self, id: &CapabilityId) -> Option<Fingerprint> {
        let inner = self.lock();
        inner
            .measured
            .get(id)
            .map(|m| m.fingerprint.clone())
            .or_else(|| inner.entries.get(id).and_then(|e| e.fingerprint.clone()))
    }

    fn invalidate(&mut self, event: &InvalidationEvent) -> usize {
        let mut inner = self.lock();
        let mark = |inner: &mut ProviderInner, id: &CapabilityId, reason: StaleReason| -> usize {
            if inner.entries.contains_key(id) && inner.stale.insert(id.clone(), reason).is_none() {
                1
            } else {
                0
            }
        };
        match event {
            InvalidationEvent::BinaryChanged {
                capability_id,
                old: _,
                new: _,
            } => mark(&mut inner, capability_id, StaleReason::BinaryChanged),
            InvalidationEvent::PackageChanged { .. } => {
                // 包事件不带能力 id：全量标记（同名包可能归属多条目）。
                let ids: Vec<CapabilityId> = inner.entries.keys().cloned().collect();
                ids.iter()
                    .map(|id| mark(&mut inner, id, StaleReason::PackageChanged))
                    .sum()
            }
            InvalidationEvent::CapabilitySetChanged { capability_id } => {
                mark(&mut inner, capability_id, StaleReason::CapabilitySetChanged)
            }
            InvalidationEvent::ConfigFileChanged { .. } => {
                // 配置事件按全量标记（policy.xml 影响 magick 等；精确映射留
                // 完整探测面）。
                let ids: Vec<CapabilityId> = inner.entries.keys().cloned().collect();
                ids.iter()
                    .map(|id| mark(&mut inner, id, StaleReason::ConfigFileChanged))
                    .sum()
            }
            InvalidationEvent::ProbeFailed {
                capability_id,
                kind,
            } => mark(&mut inner, capability_id, StaleReason::ProbeFailed(*kind)),
            InvalidationEvent::ManualInvalidate { capability_id } => match capability_id {
                Some(id) => mark(&mut inner, id, StaleReason::ManualInvalidate),
                None => {
                    let ids: Vec<CapabilityId> = inner.entries.keys().cloned().collect();
                    ids.iter()
                        .map(|id| mark(&mut inner, id, StaleReason::ManualInvalidate))
                        .sum()
                }
            },
        }
    }

    fn refresh(
        &mut self,
        id: &CapabilityId,
        level: ProbeLevel,
        budget: &ProbeBudget,
    ) -> Result<CachedEntry, ProbeError> {
        match self.probe(id, level, budget)? {
            ProbeOutcome::Entry(_) => {
                // 成功：probe 内部已清 stale；get 返回 Fresh 条目。
                Ok(self.get(id).expect("entry exists after successful probe"))
            }
            // NotProbed（层级超出实现面）：原样返回缓存条目，不标 stale。
            ProbeOutcome::NotProbed { .. } => Ok(self.get(id).expect("seed entry exists")),
        }
        // 失败路径：Err 直接返回；stale 标记由 probe 失败分支完成
        // （重探失败不沿用旧结果，F §5.3）。
    }
}
