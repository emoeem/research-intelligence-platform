//! PLUGIN-CONTRACT §4.2 校验顺序七条（VALIDATED 状态的动作清单）。
//!
//! 分层与 tool-registry 的九条同风格：v1.json 冻结契约承担结构与词表
//! （rule 2 全文校验，strict 未知字段拒绝由 additionalProperties: false
//! 表达），本模块承担需要精确错误码与运行时输入的语义条目，按 §4.2 原文
//! 顺序执行，任何一条失败即产出机器可读 [`ValidationReport`]（进 INVALID，
//! PLUGIN-CONTRACT §4.2 末段与 §5.2 validate 行）。
//!
//! 复用优先：rule 2 与 capability 内嵌 input/output Schema 文档校验走
//! [`tool_registry::jsonschema_lite`]（P1-4 已 vendor 的 jsonschema draft
//! 2020-12 门面），semver 区间求值是 tool-registry 没有的新能力
//! （PLUGIN-CONTRACT §7 的 requirement 语法），在本模块实现。
//!
//! rule 7 的包根参数：[`Options`] 与 [`validate`] 都接受 `package_root`
//! （包内容落盘位置）。None 表示元数据级校验（来源扫描阶段尚无包体），
//! rule 7 以 skipped 记录并在报告注记中显式声明，不伪装成已检查。

use std::collections::HashSet;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::{Map, Value};

use crate::manifest::{v1_schema, SUPPORTED_SCHEMA_VERSION};
use tool_registry::jsonschema_lite;

/// 校验器配置。
#[derive(Debug, Clone)]
pub struct Options {
    /// 当前平台 rip_core 版本（rule 6 求值对象）。默认对齐 workspace
    /// Cargo.toml 的 version = "0.2.0"（2026-11-11 实读）。
    pub platform_version: String,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            platform_version: "0.2.0".to_string(),
        }
    }
}

/// 单条规则的执行记录（机器可读）。
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ValidationCheck {
    /// §4.2 条目号（1 到 7）。
    pub rule: u8,
    /// 通过与否。
    pub ok: bool,
    /// 未执行（前序规则已失败或缺包体）时的注记。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// 单条拒绝原因（机器可读报告行）。
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ValidationIssue {
    /// 违反的 §4.2 条目号。
    pub rule: u8,
    /// 稳定错误码。
    pub code: String,
    /// 人类可读原因。
    pub message: String,
}

/// 七条校验的机器可读报告：`serde_json::to_string` 即 Plugin Center
/// 「兼容性」列展示的 JSON（PLUGIN-CONTRACT §4.2 末段、计划 584 行）。
#[derive(Debug, Clone, Serialize)]
pub struct ValidationReport {
    /// 七条全过（ok=false 即进 INVALID）。
    pub ok: bool,
    /// 插件 id（解析不出来时为 None）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plugin_id: Option<String>,
    /// 插件版本（同上）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// 插件 kind（同上）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    /// 拒绝原因清单（ok=true 时为空）。
    pub errors: Vec<ValidationIssue>,
    /// 已执行规则的逐条记录（按 §4.2 顺序，失败即停）。
    pub checks: Vec<ValidationCheck>,
}

impl ValidationReport {
    /// 单行 JSON（机器可读报告）。
    pub fn to_json_string(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{\"ok\":false}".to_string())
    }
}

fn issue(rule: u8, code: &str, message: impl Into<String>) -> ValidationIssue {
    ValidationIssue {
        rule,
        code: code.to_string(),
        message: message.into(),
    }
}

type RuleResult = Result<(), ValidationIssue>;

// ---------------------------------------------------------------------------
// 语义域常量（PLUGIN-CONTRACT §3 / §6）
// ---------------------------------------------------------------------------

/// kind 到合法 capability type 词表（§3.1 表：knowledge 插件不得声明
/// tool_pack 等，§4.2 rule 3）。
fn allowed_capability_types(kind: &str) -> Option<&'static [&'static str]> {
    match kind {
        "knowledge" => Some(&["knowledge_source"]),
        "tool" | "tool_pack" => Some(&["tool", "tool_pack"]),
        "project" => Some(&["project_endpoint"]),
        "agent" => Some(&["agent_definition"]),
        "workflow" => Some(&["workflow_definition"]),
        "integration" => Some(&["integration_connector"]),
        _ => None,
    }
}

const SOURCE_TYPES: [&str; 5] = ["local_dir", "git", "archive", "mcp_server", "http_api"];

/// 远程来源（§4.2 rule 5：远程来源必须带版本固定）；local_dir 不在此列。
fn is_remote_source(source_type: &str) -> bool {
    matches!(source_type, "git" | "archive" | "mcp_server" | "http_api")
}

// ---------------------------------------------------------------------------
// semver 与区间求值（§4.2 rule 3/6、§7 requirement 语法）
// ---------------------------------------------------------------------------

/// 三段数字 semver（X.Y.Z，与 tool-registry §6.2 口径一致）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Semver {
    /// 主版本。
    pub major: u64,
    /// 次版本。
    pub minor: u64,
    /// 修订号。
    pub patch: u64,
}

impl Semver {
    /// 解析 X.Y.Z 纯数字三段；非法返回 None。
    pub fn parse(s: &str) -> Option<Semver> {
        let segs: Vec<&str> = s.split('.').collect();
        if segs.len() != 3 {
            return None;
        }
        let mut out = [0u64; 3];
        for (i, seg) in segs.iter().enumerate() {
            if seg.is_empty() || !seg.chars().all(|c| c.is_ascii_digit()) {
                return None;
            }
            out[i] = seg.parse().ok()?;
        }
        Some(Semver {
            major: out[0],
            minor: out[1],
            patch: out[2],
        })
    }
}

impl std::fmt::Display for Semver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

/// 单个区间约束（PLUGIN-CONTRACT §7：requirement 用 semver 区间，支持
/// >=、>、<=、<、=、^、~；裸版本按 = 处理）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Constraint {
    /// 大于等于。
    Ge,
    /// 大于。
    Gt,
    /// 小于等于。
    Le,
    /// 小于。
    Lt,
    /// 精确等于。
    Eq,
    /// caret：>=X.Y.Z 且 <X+1.0.0。
    Caret,
    /// tilde：>=X.Y.Z 且 <X.(Y+1).0。
    Tilde,
}

/// 一组 AND 约束（逗号分隔，与示例 ">=0.2.0, <1.0.0" 同构）。
#[derive(Debug, Clone, PartialEq)]
pub struct ConstraintSet {
    parts: Vec<(Constraint, Semver)>,
}

impl ConstraintSet {
    /// 解析逗号分隔的区间表达式；任一段非法即整串拒绝。
    pub fn parse(s: &str) -> Result<ConstraintSet, String> {
        let mut parts = Vec::new();
        for seg in s.split(',') {
            let seg = seg.trim();
            if seg.is_empty() {
                return Err(format!("empty range segment in {s:?}"));
            }
            let (constraint, rest) = if let Some(rest) = seg.strip_prefix(">=") {
                (Constraint::Ge, rest)
            } else if let Some(rest) = seg.strip_prefix("<=") {
                (Constraint::Le, rest)
            } else if let Some(rest) = seg.strip_prefix('^') {
                (Constraint::Caret, rest)
            } else if let Some(rest) = seg.strip_prefix('~') {
                (Constraint::Tilde, rest)
            } else if let Some(rest) = seg.strip_prefix('>') {
                (Constraint::Gt, rest)
            } else if let Some(rest) = seg.strip_prefix('<') {
                (Constraint::Lt, rest)
            } else if let Some(rest) = seg.strip_prefix('=') {
                (Constraint::Eq, rest)
            } else {
                (Constraint::Eq, seg)
            };
            let v = Semver::parse(rest.trim()).ok_or_else(|| {
                format!("range segment {seg:?} is not a semver constraint (X.Y.Z)")
            })?;
            parts.push((constraint, v));
        }
        if parts.is_empty() {
            return Err(format!("empty constraint set {s:?}"));
        }
        Ok(ConstraintSet { parts })
    }

    /// 求值：全部约束同时满足才返回 true。
    pub fn matches(&self, v: Semver) -> bool {
        self.parts
            .iter()
            .all(|(constraint, base)| match *constraint {
                Constraint::Ge => v >= *base,
                Constraint::Gt => v > *base,
                Constraint::Le => v <= *base,
                Constraint::Lt => v < *base,
                Constraint::Eq => v == *base,
                Constraint::Caret => v >= *base && v < caret_upper(*base),
                Constraint::Tilde => v >= *base && v < next_minor(*base),
            })
    }
}

/// caret 上界（标准 semver caret 语义：最左非零段加一；全零段按 patch 进位）。
fn caret_upper(v: Semver) -> Semver {
    if v.major > 0 {
        Semver {
            major: v.major + 1,
            minor: 0,
            patch: 0,
        }
    } else if v.minor > 0 {
        next_minor(v)
    } else {
        Semver {
            major: 0,
            minor: 0,
            patch: v.patch + 1,
        }
    }
}

fn next_minor(v: Semver) -> Semver {
    Semver {
        major: v.major,
        minor: v.minor + 1,
        patch: 0,
    }
}

// ---------------------------------------------------------------------------
// 字段访问小助手
// ---------------------------------------------------------------------------

fn obj_of(v: &Value) -> Option<&Map<String, Value>> {
    v.as_object()
}

fn str_field<'a>(o: &'a Map<String, Value>, key: &str) -> Option<&'a str> {
    o.get(key).and_then(Value::as_str)
}

// ---------------------------------------------------------------------------
// rule 1：schema_version 受支持
// ---------------------------------------------------------------------------

fn rule1_schema_version(manifest: &Value) -> RuleResult {
    let sv = obj_of(manifest)
        .and_then(|o| str_field(o, "schema_version"))
        .ok_or_else(|| {
            issue(
                1,
                "schema_version_unsupported",
                "schema_version must be a string",
            )
        })?;
    if sv != SUPPORTED_SCHEMA_VERSION {
        return Err(issue(
            1,
            "schema_version_unsupported",
            format!(
                "schema_version {sv:?} unsupported; registry supports {SUPPORTED_SCHEMA_VERSION:?}"
            ),
        ));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// rule 2：全文 JSON Schema 校验（strict）+ capability 内嵌 Schema 文档合法
// ---------------------------------------------------------------------------

fn rule2_full_schema(manifest: &Value) -> RuleResult {
    jsonschema_lite::validate_instance(v1_schema(), manifest).map_err(|e| {
        issue(
            2,
            "schema_violation",
            format!("manifest fails frozen v1.json: {e}"),
        )
    })?;
    // tool/tool_pack capability 内嵌的 input/output_schema 是 JSON Schema
    // 文档（§3.1），文档合法性随 rule 2 一并检查（fail-early，语义同
    // TOOL-CONTRACT §2.5 rule 3）。
    if let Some(caps) = obj_of(manifest)
        .and_then(|o| o.get("capabilities"))
        .and_then(Value::as_array)
    {
        for cap in caps {
            let Some(cap_obj) = cap.as_object() else {
                continue; // 结构错误已由 rule 2 全文校验拒绝
            };
            let cap_id = str_field(cap_obj, "id").unwrap_or("<unidentified>");
            for key in ["input_schema", "output_schema"] {
                if let Some(schema) = cap_obj.get(key) {
                    jsonschema_lite::check_schema_document(schema).map_err(|e| {
                        issue(
                            2,
                            "capability_schema_invalid",
                            format!("capability {cap_id:?} {key} is not a valid JSON Schema document: {e}"),
                        )
                    })?;
                }
            }
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// rule 3：id 全局唯一 + version 合法 semver + kind 与 capability type 匹配
// ---------------------------------------------------------------------------

fn rule3_identity(manifest: &Map<String, Value>, existing_ids: &HashSet<String>) -> RuleResult {
    let id = str_field(manifest, "id")
        .ok_or_else(|| issue(3, "id_invalid", "`id` must be a non-empty string"))?;
    if existing_ids.contains(id) {
        return Err(issue(
            3,
            "id_conflict",
            format!("plugin id {id:?} already exists in the registry index (global uniqueness)"),
        ));
    }
    let version = str_field(manifest, "version").ok_or_else(|| {
        issue(
            3,
            "version_not_semver",
            "`version` must be a non-empty string",
        )
    })?;
    if Semver::parse(version).is_none() {
        return Err(issue(
            3,
            "version_not_semver",
            format!("version {version:?} is not X.Y.Z numeric semver"),
        ));
    }
    let kind = str_field(manifest, "kind")
        .ok_or_else(|| issue(3, "kind_invalid", "`kind` must be a non-empty string"))?;
    let allowed = allowed_capability_types(kind).ok_or_else(|| {
        issue(
            3,
            "kind_invalid",
            format!("kind {kind:?} not in the 6-kind vocabulary"),
        )
    })?;
    if let Some(caps) = manifest.get("capabilities").and_then(Value::as_array) {
        for cap in caps {
            let Some(cap_obj) = cap.as_object() else {
                continue;
            };
            let cap_id = str_field(cap_obj, "id").unwrap_or("<unidentified>");
            let cap_type = str_field(cap_obj, "type").ok_or_else(|| {
                issue(
                    3,
                    "kind_capability_mismatch",
                    format!("capability {cap_id:?} type must be a string"),
                )
            })?;
            if !allowed.contains(&cap_type) {
                return Err(issue(
                    3,
                    "kind_capability_mismatch",
                    format!(
                        "kind {kind:?} must not declare capability type {cap_type:?} (allowed: {allowed:?})"
                    ),
                ));
            }
        }
    }
    if let Some(deps) = manifest.get("dependencies").and_then(Value::as_array) {
        for dep in deps {
            let Some(dep_obj) = dep.as_object() else {
                continue;
            };
            let dep_id = str_field(dep_obj, "id").unwrap_or("<unidentified>");
            let req = str_field(dep_obj, "requirement").ok_or_else(|| {
                issue(
                    3,
                    "dependency_requirement_invalid",
                    format!("dependency {dep_id:?} requirement must be a non-empty string"),
                )
            })?;
            if let Err(e) = ConstraintSet::parse(req) {
                return Err(issue(
                    3,
                    "dependency_requirement_invalid",
                    format!("dependency {dep_id:?} requirement {req:?}: {e}"),
                ));
            }
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// rule 4：license 与 source 信息完整（计划 417 行）
// ---------------------------------------------------------------------------

fn rule4_license_source(manifest: &Map<String, Value>) -> RuleResult {
    let license = str_field(manifest, "license").ok_or_else(|| {
        issue(
            4,
            "license_missing",
            "`license` must be a non-empty string (SPDX or UNLICENSED)",
        )
    })?;
    if license == "unknown" {
        return Err(issue(
            4,
            "license_unknown",
            "license=unknown must be resolved before validate (aligns tool-registry rule 5)",
        ));
    }
    let source = manifest
        .get("source")
        .and_then(Value::as_object)
        .ok_or_else(|| issue(4, "source_incomplete", "`source` must be an object"))?;
    let source_type = str_field(source, "type").ok_or_else(|| {
        issue(
            4,
            "source_incomplete",
            "`source.type` must be a non-empty string",
        )
    })?;
    if !SOURCE_TYPES.contains(&source_type) {
        return Err(issue(
            4,
            "source_incomplete",
            format!("source.type {source_type:?} not in {:?}", SOURCE_TYPES),
        ));
    }
    // 每类来源的必填键（§6 表）在 rule 4 复核一遍：schema（rule 2）管结构，
    // 这里给出 source_incomplete 的精确归因。
    let required: &[&str] = match source_type {
        "local_dir" => &["path"],
        "git" => &["url", "host", "pinned"],
        "archive" => &["url", "sha256", "size"],
        "mcp_server" => &["endpoint", "protocol_version"],
        "http_api" => &["base_url", "api_version"],
        _ => &[],
    };
    for key in required {
        if source.get(*key).map(Value::is_null).unwrap_or(true) {
            return Err(issue(
                4,
                "source_incomplete",
                format!("source.type {source_type:?} requires non-empty `{key}`"),
            ));
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// rule 5：远程来源必须带版本固定（§6 表；草案 D8 浮动分支拒绝）
// ---------------------------------------------------------------------------

/// commit 短 sha 或全量 sha（PLUGIN-CONTRACT §10：安装时以 rev-parse HEAD
/// 全量复核，7 到 64 位小写 hex 均收）。
fn commit_ref_ok(value: &str) -> bool {
    (7..=64).contains(&value.len())
        && value
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
}

fn rule5_source_pinning(manifest: &Map<String, Value>) -> RuleResult {
    let source = manifest
        .get("source")
        .and_then(Value::as_object)
        .ok_or_else(|| issue(5, "pin_missing", "`source` must be an object"))?;
    let source_type = str_field(source, "type").unwrap_or_default();
    if !is_remote_source(source_type) {
        // local_dir：非远程来源；pinned 可选，给了就按 commit 语义复核。
        if let Some(pinned) = source.get("pinned").and_then(Value::as_object) {
            let kind = str_field(pinned, "kind").unwrap_or_default();
            if kind != "commit" {
                return Err(issue(
                    5,
                    "pin_kind_invalid",
                    format!("local_dir pinned.kind {kind:?} must be \"commit\""),
                ));
            }
            if let Some(value) = str_field(pinned, "value") {
                if !commit_ref_ok(value) {
                    return Err(issue(
                        5,
                        "pin_value_invalid",
                        format!(
                            "local_dir pinned.value {value:?} must be 7 to 64 lowercase hex (commit ref)"
                        ),
                    ));
                }
            }
        }
        return Ok(());
    }
    if source_type == "git" {
        let pinned = source
            .get("pinned")
            .and_then(Value::as_object)
            .ok_or_else(|| issue(5, "pin_missing", "git source requires `pinned`"))?;
        let kind = str_field(pinned, "kind").unwrap_or_default();
        if !matches!(kind, "commit" | "tag" | "release") {
            return Err(issue(
                    5,
                    "floating_branch_rejected",
                    format!(
                        "git pinned.kind {kind:?} is not commit/tag/release; floating branches are forbidden for install (draft D8, PLUGIN-CONTRACT 6)"
                    ),
                ));
        }
        let value = str_field(pinned, "value").unwrap_or_default();
        if value.is_empty() {
            return Err(issue(
                5,
                "pin_value_missing",
                format!("git pinned.kind {kind:?} requires non-empty `value` (PLUGIN-CONTRACT 6)"),
            ));
        }
        if kind == "commit" && !commit_ref_ok(value) {
            return Err(issue(
                5,
                "pin_value_invalid",
                format!("git pinned.value {value:?} must be 7 to 64 lowercase hex for kind=commit"),
            ));
        }
    }
    // archive 的 sha256 即版本固定（schema rule 2 已强制 64 hex）。
    // mcp_server 的快照哈希与 http_api 的契约哈希在 install 阶段取得
    // （§6 校验列，P6-4 落地），validate 阶段静态放行并留注记。
    Ok(())
}

// ---------------------------------------------------------------------------
// rule 6：compatibility.rip_core 与当前平台版本比对
// ---------------------------------------------------------------------------

fn rule6_compatibility(manifest: &Map<String, Value>, opts: &Options) -> RuleResult {
    let compat = manifest
        .get("compatibility")
        .and_then(Value::as_object)
        .ok_or_else(|| {
            issue(
                6,
                "compat_range_invalid",
                "`compatibility` must be an object",
            )
        })?;
    let rip_core = str_field(compat, "rip_core").ok_or_else(|| {
        issue(
            6,
            "compat_range_invalid",
            "`compatibility.rip_core` must be a non-empty string",
        )
    })?;
    let range = ConstraintSet::parse(rip_core).map_err(|e| {
        issue(
            6,
            "compat_range_invalid",
            format!("compatibility.rip_core {rip_core:?}: {e}"),
        )
    })?;
    let platform = Semver::parse(&opts.platform_version).ok_or_else(|| {
        issue(
            6,
            "platform_version_invalid",
            format!(
                "configured platform version {:?} is not X.Y.Z semver",
                opts.platform_version
            ),
        )
    })?;
    if !range.matches(platform) {
        return Err(issue(
            6,
            "compat_unsatisfied",
            format!(
                "compatibility.rip_core {rip_core:?} does not admit platform version {platform}"
            ),
        ));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// rule 7：capabilities[].entry 指向的文件存在于包内（含路径穿越拒绝）
// ---------------------------------------------------------------------------

/// 包内相对路径安全检查：拒绝绝对路径与 `..` 上跳（路径穿越面 fail-closed，
/// 对齐工作区安全清单口径）。
fn entry_relpath_ok(entry: &str) -> bool {
    if entry.is_empty() || entry.starts_with('/') || entry.starts_with('\\') {
        return false;
    }
    let p = Path::new(entry);
    !p.components().any(|c| {
        matches!(
            c,
            std::path::Component::ParentDir
                | std::path::Component::RootDir
                | std::path::Component::Prefix(_)
                | std::path::Component::CurDir
        )
    })
}

fn rule7_entry_files(manifest: &Map<String, Value>, package_root: &Path) -> RuleResult {
    let caps = manifest
        .get("capabilities")
        .and_then(Value::as_array)
        .ok_or_else(|| issue(7, "entry_missing", "`capabilities` must be an array"))?;
    for cap in caps {
        let Some(cap_obj) = cap.as_object() else {
            continue;
        };
        let cap_id = str_field(cap_obj, "id").unwrap_or("<unidentified>");
        let entry = str_field(cap_obj, "entry").unwrap_or_default();
        if !entry_relpath_ok(entry) {
            return Err(issue(
                7,
                "entry_escapes_package",
                format!(
                    "capability {cap_id:?} entry {entry:?} is not a safe package-relative path (traversal rejected)"
                ),
            ));
        }
        let target: PathBuf = package_root.join(entry);
        if !target.is_file() {
            return Err(issue(
                7,
                "entry_missing",
                format!(
                    "capability {cap_id:?} entry {entry:?} does not exist in the package at {:?}",
                    package_root.display()
                ),
            ));
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// 七条汇总（按 §4.2 顺序，失败即停）
// ---------------------------------------------------------------------------

/// 按 PLUGIN-CONTRACT §4.2 顺序执行七条，产出机器可读报告。
///
/// `package_root` 为 None 时 rule 7 记为 skipped（报告注记显式声明
/// deferred，不伪装成已检查）；其余六条不受影响。
pub fn validate(
    manifest: &Value,
    package_root: Option<&Path>,
    existing_ids: &HashSet<String>,
    opts: &Options,
) -> ValidationReport {
    let mut report = ValidationReport {
        ok: true,
        plugin_id: None,
        version: None,
        kind: None,
        errors: Vec::new(),
        checks: Vec::new(),
    };
    let mut check = |rule: u8, result: RuleResult, skipped_note: Option<&str>| {
        match (&result, skipped_note) {
            (Ok(()), Some(note)) => report.checks.push(ValidationCheck {
                rule,
                ok: true,
                note: Some((*note).to_string()),
            }),
            (Ok(()), None) => report.checks.push(ValidationCheck {
                rule,
                ok: true,
                note: None,
            }),
            (Err(e), _) => {
                report.checks.push(ValidationCheck {
                    rule,
                    ok: false,
                    note: None,
                });
                report.errors.push(e.clone());
            }
        }
        result.is_ok()
    };

    let obj = match manifest.as_object() {
        Some(o) => o,
        None => {
            report.ok = false;
            report.errors.push(issue(
                2,
                "schema_violation",
                "manifest must be a JSON object at the top level",
            ));
            return report;
        }
    };
    report.plugin_id = str_field(obj, "id").map(str::to_string);
    report.version = str_field(obj, "version").map(str::to_string);
    report.kind = str_field(obj, "kind").map(str::to_string);

    let mut all_pass = check(1, rule1_schema_version(manifest), None);
    if all_pass {
        all_pass &= check(2, rule2_full_schema(manifest), None);
    }
    if all_pass {
        all_pass &= check(3, rule3_identity(obj, existing_ids), None);
    }
    if all_pass {
        all_pass &= check(4, rule4_license_source(obj), None);
    }
    if all_pass {
        all_pass &= check(5, rule5_source_pinning(obj), None);
    }
    if all_pass {
        all_pass &= check(6, rule6_compatibility(obj, opts), None);
    }
    if all_pass {
        let note = if package_root.is_some() {
            None
        } else {
            Some("package_root not provided: entry existence check deferred to install")
        };
        let rule7 = match package_root {
            Some(root) => rule7_entry_files(obj, root),
            None => Ok(()),
        };
        all_pass &= check(7, rule7, note);
    }
    report.ok = report.errors.is_empty() && all_pass;
    report
}

/// 便捷封装：给定 Manifest 文本（JSON 或 YAML）、包根与既有 id 集，先解析
/// 再走七条。解析失败时报告 ok=false 且错误行 code 为解析错误码。
pub fn validate_document(
    doc: &str,
    package_root: Option<&Path>,
    existing_ids: &HashSet<String>,
    opts: &Options,
) -> ValidationReport {
    match crate::manifest::parse_document(doc) {
        Ok(manifest) => validate(&manifest, package_root, existing_ids, opts),
        Err(e) => ValidationReport {
            ok: false,
            plugin_id: None,
            version: None,
            kind: None,
            errors: vec![ValidationIssue {
                rule: 1,
                code: e.code.clone(),
                message: e.message,
            }],
            checks: vec![],
        },
    }
}

/// 把机器可读报告渲染为多行文本（调试输出；结构化消费请走
/// [`ValidationReport::to_json_string`]）。
pub fn render_report_text(report: &ValidationReport) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "plugin_id={:?}", report.plugin_id);
    for c in &report.checks {
        let _ = writeln!(
            out,
            "rule {} ok={}{}",
            c.rule,
            c.ok,
            c.note
                .as_deref()
                .map(|n| format!(" note={n}"))
                .unwrap_or_default()
        );
    }
    for e in &report.errors {
        let _ = writeln!(
            out,
            "error rule={} code={} message={}",
            e.rule, e.code, e.message
        );
    }
    let _ = writeln!(out, "ok={}", report.ok);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> Value {
        serde_json::json!({
            "schema_version": "1",
            "id": "x.y",
            "name": "X",
            "version": "0.1.0",
            "kind": "knowledge",
            "description": "d",
            "license": "MIT",
            "source": {"type": "local_dir", "path": "p"},
            "compatibility": {"rip_core": ">=0.2.0"},
            "capabilities": [
                {"id": "x.k", "type": "knowledge_source", "entry": "e.yaml", "summary": "s"}
            ],
            "context": {"summary": "s", "docs": "d.md"}
        })
    }

    #[test]
    fn semver_and_ranges() {
        assert!(Semver::parse("0.2.0").is_some());
        assert!(Semver::parse("0.2").is_none());
        assert!(Semver::parse("0.2.x").is_none());
        let range = ConstraintSet::parse(">=0.2.0, <1.0.0").unwrap();
        assert!(range.matches(Semver::parse("0.2.0").unwrap()));
        assert!(range.matches(Semver::parse("0.99.0").unwrap()));
        assert!(!range.matches(Semver::parse("1.0.0").unwrap()));
        assert!(!range.matches(Semver::parse("0.1.9").unwrap()));
        let caret = ConstraintSet::parse("^0.2.0").unwrap();
        assert!(caret.matches(Semver::parse("0.2.9").unwrap()));
        assert!(!caret.matches(Semver::parse("0.3.0").unwrap()));
        let tilde = ConstraintSet::parse("~0.2.0").unwrap();
        assert!(tilde.matches(Semver::parse("0.2.9").unwrap()));
        assert!(!tilde.matches(Semver::parse("0.3.0").unwrap()));
        assert!(ConstraintSet::parse("1.2.3")
            .unwrap()
            .matches(Semver::parse("1.2.3").unwrap()));
        assert!(ConstraintSet::parse(">=0.2.0").is_ok());
        assert!(ConstraintSet::parse("").is_err());
        assert!(ConstraintSet::parse(">=0.2.0, ").is_err());
    }

    #[test]
    fn rule1_rejects_unsupported_version() {
        let mut m = base();
        m.as_object_mut().unwrap()["schema_version"] = serde_json::json!("2");
        let report = validate(&m, None, &HashSet::new(), &Options::default());
        assert!(!report.ok);
        assert_eq!(report.errors[0].rule, 1);
        assert_eq!(report.errors[0].code, "schema_version_unsupported");
        assert_eq!(report.checks.len(), 1, "failure stops the chain");
    }

    #[test]
    fn rule2_rejects_unknown_field() {
        let mut m = base();
        m.as_object_mut()
            .unwrap()
            .insert("extra_field".into(), serde_json::json!(1));
        let report = validate(&m, None, &HashSet::new(), &Options::default());
        assert!(!report.ok);
        assert_eq!(report.errors[0].rule, 2);
        assert_eq!(report.errors[0].code, "schema_violation");
        assert!(
            report.errors[0].message.contains("extra_field"),
            "{}",
            report.errors[0].message
        );
    }

    #[test]
    fn rule2_rejects_missing_required_license() {
        let mut m = base();
        m.as_object_mut().unwrap().remove("license");
        let report = validate(&m, None, &HashSet::new(), &Options::default());
        assert!(!report.ok);
        assert_eq!(report.errors[0].rule, 2);
        assert!(
            report.errors[0].message.contains("license"),
            "{}",
            report.errors[0].message
        );
    }

    #[test]
    fn rule3_rejects_kind_capability_mismatch_and_conflict() {
        let mut m = base();
        // 结构完整的 tool capability（runtime + 输入输出 Schema 齐），否则
        // rule 2 的 tool-capability 结构条件先拒，到不了 rule 3 的归因。
        let tool_cap = serde_json::json!({
            "id": "x.t",
            "type": "tool_pack",
            "entry": "e.py",
            "summary": "s",
            "runtime": {"type": "python_worker"},
            "input_schema": {"type": "object"},
            "output_schema": {"type": "object"}
        });
        m.as_object_mut().unwrap()["capabilities"] = serde_json::json!([tool_cap]);
        let report = validate(&m, None, &HashSet::new(), &Options::default());
        assert_eq!(report.errors[0].rule, 3, "{}", report.errors[0].message);
        assert_eq!(report.errors[0].code, "kind_capability_mismatch");

        let ids: HashSet<String> = ["x.y".to_string()].into_iter().collect();
        let report2 = validate(&base(), None, &ids, &Options::default());
        assert_eq!(report2.errors[0].rule, 3);
        assert_eq!(report2.errors[0].code, "id_conflict");
    }

    #[test]
    fn rule4_rejects_license_unknown_and_incomplete_source() {
        let mut m = base();
        m.as_object_mut().unwrap()["license"] = serde_json::json!("unknown");
        let report = validate(&m, None, &HashSet::new(), &Options::default());
        assert_eq!(report.errors[0].rule, 4);
        assert_eq!(report.errors[0].code, "license_unknown");

        let mut m2 = base();
        m2.as_object_mut().unwrap()["source"] = serde_json::json!({"type": "local_dir"});
        let report2 = validate(&m2, None, &HashSet::new(), &Options::default());
        assert_eq!(report2.errors[0].rule, 4);
        assert_eq!(report2.errors[0].code, "source_incomplete");
    }

    #[test]
    fn rule5_rejects_floating_branch_and_missing_pin_value() {
        let mut m = base();
        m.as_object_mut().unwrap()["source"] = serde_json::json!({
            "type": "git", "url": "https://example.com/x", "host": "github",
            "pinned": {"kind": "branch", "value": "main"}
        });
        let report = validate(&m, None, &HashSet::new(), &Options::default());
        assert_eq!(report.errors[0].rule, 5);
        assert_eq!(report.errors[0].code, "floating_branch_rejected");

        let mut m2 = base();
        m2.as_object_mut().unwrap()["source"] = serde_json::json!({
            "type": "git", "url": "https://example.com/x", "host": "github",
            "pinned": {"kind": "commit"}
        });
        let report2 = validate(&m2, None, &HashSet::new(), &Options::default());
        assert_eq!(report2.errors[0].rule, 5);
        assert_eq!(report2.errors[0].code, "pin_value_missing");

        let mut m3 = base();
        m3.as_object_mut().unwrap()["source"] = serde_json::json!({
            "type": "git", "url": "https://example.com/x", "host": "github",
            "pinned": {"kind": "commit", "value": "main"}
        });
        let report3 = validate(&m3, None, &HashSet::new(), &Options::default());
        assert_eq!(report3.errors[0].code, "pin_value_invalid");
    }

    #[test]
    fn rule6_rejects_unsatisfied_platform() {
        let mut m = base();
        m.as_object_mut().unwrap()["compatibility"] = serde_json::json!({"rip_core": ">=0.3.0"});
        let report = validate(&m, None, &HashSet::new(), &Options::default());
        assert_eq!(report.errors[0].rule, 6);
        assert_eq!(report.errors[0].code, "compat_unsatisfied");

        let mut m2 = base();
        m2.as_object_mut().unwrap()["compatibility"] = serde_json::json!({"rip_core": "^0.2.0"});
        assert!(validate(&m2, None, &HashSet::new(), &Options::default()).ok);
    }

    #[test]
    fn rule7_rejects_traversal_and_missing_entry() {
        let mut m = base();
        m.as_object_mut().unwrap()["capabilities"][0]["entry"] =
            serde_json::json!("../../escape.yaml");
        let tmp = std::env::temp_dir().join("p6-rule7-test");
        std::fs::create_dir_all(&tmp).unwrap();
        let report = validate(&m, Some(&tmp), &HashSet::new(), &Options::default());
        assert_eq!(report.errors[0].rule, 7);
        assert_eq!(report.errors[0].code, "entry_escapes_package");

        let mut m2 = base();
        m2.as_object_mut().unwrap()["capabilities"][0]["entry"] = serde_json::json!("missing.yaml");
        let report2 = validate(&m2, Some(&tmp), &HashSet::new(), &Options::default());
        assert_eq!(report2.errors[0].code, "entry_missing");
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn rule7_deferred_when_no_package_root() {
        let report = validate(&base(), None, &HashSet::new(), &Options::default());
        assert!(report.ok);
        let rule7 = report.checks.iter().find(|c| c.rule == 7).unwrap();
        assert!(rule7.ok);
        assert!(rule7.note.as_deref().unwrap().contains("deferred"));
    }

    #[test]
    fn report_is_machine_readable_json() {
        let report = validate(&base(), None, &HashSet::new(), &Options::default());
        let json = report.to_json_string();
        let parsed: Value = serde_json::from_str(&json).expect("report must be valid JSON");
        assert_eq!(parsed["ok"], Value::Bool(true));
        assert!(parsed["checks"].is_array());
    }
}
