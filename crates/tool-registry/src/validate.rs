//! Manifest 校验九条（TOOL-CONTRACT §2.5）。
//!
//! 分层：[`validate_static`] 覆盖不需要存储访问的条目（第 1、3、4、5、6、8
//! 条），[`check_version_comparable`] 实现第 2 条（需查 tool_version 既有
//! 版本），[`check_secrets`] 实现第 7 条（需平台 secrets 存储侧信）；第 9 条
//! 的落库动作在 [`crate::store`]。结构检查口径与本仓冻结契约
//! schemas/tool-manifest/v1.json（P1-1）一致：必填集、词表、域值、
//! additionalProperties: false。
//!
//! 占位 digest 让步（如实声明）：image 允许 `sha256:<digest>` 占位形态受
//! [`Options::allow_placeholder_digest`] 开关控制，默认 false（拒绝占位与
//! tag 引用）；仅 P1-4 验收流程在测试库对该开关显式放行，用于让 §9.1 示例
//! （镜像属 Phase 2/3 交付，TOOL-CONTRACT §11.1）走完登记全流程。

use crate::error::{RegistryError, ValidationRule};
use crate::hash::sha256_hex;
use crate::jsonschema_lite;
use database::Pool;
use serde_json::{Map, Value};

/// Registry 支持的 Manifest 主版本（§6.1：拒绝更高主版本）。
pub const SUPPORTED_SCHEMA_VERSION: &str = "1";

const ID_MAX_LEN: usize = 128;
const NAME_MAX_LEN: usize = 80;
const FAMILIES: [&str; 13] = [
    "web_http",
    "dns_domain",
    "network",
    "web_analysis",
    "source_code",
    "mobile_apk",
    "binary_reverse",
    "document",
    "data_processing",
    "knowledge",
    "evidence",
    "utility",
    "browser_automation",
];
const RUNTIME_TYPES: [&str; 7] = [
    "podman_vm",
    "python_worker",
    "native_rust",
    "mcp_client",
    "http_api",
    "cli_adapter",
    "remote_runner",
];

const REQUIRED_FIELDS: [&str; 17] = [
    "schema_version",
    "id",
    "name",
    "version",
    "kind",
    "family",
    "description",
    "runtime",
    "input_schema",
    "output_schema",
    "resources",
    "network",
    "secrets",
    "permission_level",
    "idempotent",
    "health",
    "provenance",
];

/// 静态校验产物：规范化 Manifest 及其身份与 content_hash。
#[derive(Debug, Clone)]
pub struct ValidatedManifest {
    /// 规范化后的 Manifest（serde_json Value，BTreeMap 键序确定性）。
    pub manifest: Value,
    /// 工具 id。
    pub id: String,
    /// 工具语义化版本。
    pub version: String,
    /// 规范化 JSON 的 sha256（§2.5 第 9 条 content_hash 列来源）。
    pub content_hash: String,
}

/// 校验开关（默认即生产语义：allow_placeholder_digest = false 时拒绝占位
/// digest 与 tag 引用，测试库验收流程可显式放行）。
#[derive(Debug, Clone, Default)]
pub struct Options {
    /// 允许 `sha256:<digest>` 占位镜像（仅测试库验收流程使用，默认拒绝）。
    pub allow_placeholder_digest: bool,
}

/// 规范化 JSON：serde_json 默认 BTreeMap 键序，to_string 确定性输出。
pub fn normalize_json(value: &Value) -> String {
    serde_json::to_string(value).expect("Value serialization cannot fail")
}

/// Manifest 规范化 JSON 的 sha256（小写 hex）。
pub fn content_hash(manifest: &Value) -> String {
    sha256_hex(normalize_json(manifest).as_bytes())
}

fn reject(rule: ValidationRule, id: Option<&str>, reason: String) -> RegistryError {
    RegistryError::Validation {
        rule,
        tool_id: id.map(str::to_string),
        reason,
    }
}

// ---------------------------------------------------------------------------
// 字段访问小助手（错误统一带 rule 1 语义）
// ---------------------------------------------------------------------------

fn req_str<'a>(o: &'a Map<String, Value>, key: &str) -> Result<&'a str, String> {
    o.get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("`{key}` must be a non-empty string"))
}

fn req_obj<'a>(o: &'a Map<String, Value>, key: &str) -> Result<&'a Map<String, Value>, String> {
    o.get(key)
        .and_then(Value::as_object)
        .ok_or_else(|| format!("`{key}` must be an object"))
}

fn req_arr<'a>(o: &'a Map<String, Value>, key: &str) -> Result<&'a Vec<Value>, String> {
    o.get(key)
        .and_then(Value::as_array)
        .ok_or_else(|| format!("`{key}` must be an array"))
}

fn req_bool(o: &Map<String, Value>, key: &str) -> Result<bool, String> {
    o.get(key)
        .and_then(Value::as_bool)
        .ok_or_else(|| format!("`{key}` must be a boolean"))
}

fn is_lower_seg_chars(seg: &str) -> bool {
    seg.chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
}

/// `^[a-z][a-z0-9_-]*(\.[a-z][a-z0-9_-]*)+$` 的手工实现（D-TC-02；2026-11-09
/// P1-7b 修订：段内放宽为 [a-z0-9_-]，对齐 TOOL-CONTRACT §10.2 B2/B5 点名的
/// 工具名 rip.kb.keyword_search 与 rip.ops.etl_status，与冻结 schema 同步）。
fn manifest_id_ok(id: &str) -> bool {
    if id.is_empty() || id.len() > ID_MAX_LEN {
        return false;
    }
    let segs: Vec<&str> = id.split('.').collect();
    segs.len() >= 2
        && segs.iter().all(|s| {
            !s.is_empty()
                && s.starts_with(|c: char| c.is_ascii_lowercase())
                && is_lower_seg_chars(s)
        })
}

/// `^[0-9]+\.[0-9]+\.[0-9]+$` 的手工实现（§6.2 semver 三段数字）。
fn semver_parse(v: &str) -> Option<(u64, u64, u64)> {
    let segs: Vec<&str> = v.split('.').collect();
    if segs.len() != 3 {
        return None;
    }
    let mut out = [0u64; 3];
    for (i, s) in segs.iter().enumerate() {
        if s.is_empty() || !s.chars().all(|c| c.is_ascii_digit()) {
            return None;
        }
        out[i] = s.parse().ok()?;
    }
    Some((out[0], out[1], out[2]))
}

/// `@sha256:<64hex>` 结尾校验；Ok(None)=无占位，Ok(Some(()))=文档化占位形态。
fn image_digest_part(image: &str) -> Result<Option<&str>, String> {
    let rest = image
        .strip_suffix_after("@sha256:")
        .ok_or_else(|| "image must be digest-pinned `...@sha256:<64hex>` (tag references rejected, TOOL-CONTRACT 2.5 rule 4)".to_string())?;
    if rest.len() == 64
        && rest
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
    {
        Ok(None)
    } else if rest == "<digest>" {
        Ok(Some(rest))
    } else {
        Err(format!(
            "image digest portion `{rest}` is neither 64 lowercase hex nor the documented `<digest>` placeholder"
        ))
    }
}

trait StripSuffixAfter {
    fn strip_suffix_after(&self, sep: &str) -> Option<&str>;
}

impl StripSuffixAfter for str {
    fn strip_suffix_after(&self, sep: &str) -> Option<&str> {
        self.rsplit_once(sep).map(|(_, rest)| rest)
    }
}

/// `^[A-Z][A-Z0-9_]+$` 的手工实现。
fn secret_name_ok(name: &str) -> bool {
    let mut chars = name.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_uppercase())
        && chars.all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
}

/// Kubernetes cpu 数量格式 `^[0-9]+(\.[0-9]+)?(m)?$`。
fn cpu_quantity_ok(s: &str) -> bool {
    let core = s.strip_suffix('m').unwrap_or(s);
    let (int_part, frac_part) = match core.split_once('.') {
        Some((a, b)) => (a, Some(b)),
        None => (core, None),
    };
    !int_part.is_empty()
        && int_part.chars().all(|c| c.is_ascii_digit())
        && frac_part
            .map(|f| !f.is_empty() && f.chars().all(|c| c.is_ascii_digit()))
            .unwrap_or(true)
}

/// 字节量格式 `^[0-9]+(Mi|Gi|Ki)$`。
fn memory_quantity_ok(s: &str) -> bool {
    for unit in ["Mi", "Gi", "Ki"] {
        if let Some(num) = s.strip_suffix(unit) {
            return !num.is_empty() && num.chars().all(|c| c.is_ascii_digit());
        }
    }
    false
}

fn sha256_digest_ok(s: &str) -> bool {
    s.len() == 64
        && s.chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
}

// ---------------------------------------------------------------------------
// 第 1 条：顶层必填字段齐全 + 结构形状（对齐冻结 v1.json）
// ---------------------------------------------------------------------------

fn rule1_structural(manifest: &Value) -> Result<(String, String), RegistryError> {
    let obj = manifest.as_object().ok_or_else(|| {
        reject(
            ValidationRule::RequiredFields,
            None,
            "manifest must be a JSON object".to_string(),
        )
    })?;

    for key in REQUIRED_FIELDS {
        if !obj.contains_key(key) {
            return Err(reject(
                ValidationRule::RequiredFields,
                None,
                format!("missing required field `{key}`"),
            ));
        }
    }
    const KNOWN: [&str; 20] = [
        "schema_version",
        "id",
        "name",
        "version",
        "kind",
        "family",
        "description",
        "runtime",
        "input_schema",
        "output_schema",
        "resources",
        "network",
        "secrets",
        "permission_level",
        "idempotent",
        "health",
        "provenance",
        "context",
        "tags",
        "examples",
    ];
    for key in obj.keys() {
        if !KNOWN.contains(&key.as_str()) {
            return Err(reject(
                ValidationRule::RequiredFields,
                None,
                format!("unknown top-level field `{key}` (additionalProperties: false)"),
            ));
        }
    }

    let sv = req_str(obj, "schema_version")
        .map_err(|e| reject(ValidationRule::RequiredFields, None, e))?;
    if sv != SUPPORTED_SCHEMA_VERSION {
        return Err(reject(
            ValidationRule::RequiredFields,
            None,
            format!(
                "schema_version {sv:?} unsupported; registry supports {SUPPORTED_SCHEMA_VERSION:?} (upgrade guidance per 6.1)"
            ),
        ));
    }

    let id = req_str(obj, "id").map_err(|e| reject(ValidationRule::RequiredFields, None, e))?;
    if !manifest_id_ok(id) {
        return Err(reject(
            ValidationRule::RequiredFields,
            Some(id),
            "id must match ^[a-z][a-z0-9_-]*(\\.[a-z][a-z0-9_-]*)+$ with max length 128"
                .to_string(),
        ));
    }
    let err1 = |reason: String| reject(ValidationRule::RequiredFields, Some(id), reason);

    let name = req_str(obj, "name").map_err(err1)?;
    if name.is_empty() || name.chars().count() > NAME_MAX_LEN {
        return Err(err1(format!("name length must be 1..{NAME_MAX_LEN}")));
    }
    let version = req_str(obj, "version").map_err(err1)?;
    if semver_parse(version).is_none() {
        return Err(err1(format!(
            "version {version:?} is not X.Y.Z numeric semver (6.2)"
        )));
    }
    if req_str(obj, "kind").map_err(err1)? != "tool" {
        return Err(err1("kind must be `tool`".to_string()));
    }
    let family = req_str(obj, "family").map_err(err1)?;
    if !FAMILIES.contains(&family) {
        return Err(err1(format!(
            "family {family:?} not in the 13-family vocabulary"
        )));
    }
    if req_str(obj, "description").map_err(err1)?.is_empty() {
        return Err(err1("description must be non-empty".to_string()));
    }
    req_bool(obj, "idempotent").map_err(err1)?;
    let permission = req_str(obj, "permission_level").map_err(err1)?;
    if !matches!(permission, "T0" | "T1" | "T2") {
        return Err(err1(format!(
            "permission_level {permission:?} not in T0/T1/T2"
        )));
    }

    // runtime 只做词表与对象形状（分支必填在第 4 条）
    let runtime = req_obj(obj, "runtime").map_err(err1)?;
    let rtype = runtime
        .get("type")
        .and_then(Value::as_str)
        .ok_or_else(|| err1("runtime.type must be a string".to_string()))?;
    if !RUNTIME_TYPES.contains(&rtype) {
        return Err(err1(format!(
            "runtime.type {rtype:?} not in vocabulary (D-TC-01)"
        )));
    }

    // resources 形状与域值
    let resources = req_obj(obj, "resources").map_err(err1)?;
    const RESOURCE_KEYS: [&str; 6] = [
        "cpu",
        "memory",
        "timeout_seconds",
        "max_output_bytes",
        "pid_limit",
        "disk_mb",
    ];
    for key in resources.keys() {
        if !RESOURCE_KEYS.contains(&key.as_str()) {
            return Err(err1(format!(
                "resources.{key} unknown (additionalProperties: false)"
            )));
        }
    }
    let cpu = req_str(resources, "cpu").map_err(err1)?;
    if !cpu_quantity_ok(cpu) {
        return Err(err1(format!("resources.cpu {cpu:?} is not a k8s quantity")));
    }
    let memory = req_str(resources, "memory").map_err(err1)?;
    if !memory_quantity_ok(memory) {
        return Err(err1(format!(
            "resources.memory {memory:?} is not Ki/Mi/Gi sized"
        )));
    }
    // timeout_seconds / max_output_bytes 的类型检查在第 1 条，域值检查在第 6 条。
    resources
        .get("timeout_seconds")
        .and_then(Value::as_i64)
        .ok_or_else(|| err1("resources.timeout_seconds must be an integer".to_string()))?;
    if let Some(mob) = resources.get("max_output_bytes") {
        mob.as_i64()
            .ok_or_else(|| err1("resources.max_output_bytes must be an integer".to_string()))?;
    }
    for key in ["pid_limit", "disk_mb"] {
        if let Some(v) = resources.get(key) {
            let n = v
                .as_i64()
                .ok_or_else(|| err1(format!("resources.{key} must be an integer")))?;
            if n < 1 {
                return Err(err1(format!("resources.{key} must be >= 1")));
            }
        }
    }

    // network
    let network = req_obj(obj, "network").map_err(err1)?;
    for key in network.keys() {
        if !matches!(key.as_str(), "mode" | "egress") {
            return Err(err1(format!(
                "network.{key} unknown (additionalProperties: false)"
            )));
        }
    }
    let mode = req_str(network, "mode").map_err(err1)?;
    if !matches!(mode, "none" | "egress_allowlist" | "vm_local") {
        return Err(err1(format!(
            "network.mode {mode:?} not in none/egress_allowlist/vm_local"
        )));
    }
    if let Some(egress) = network.get("egress") {
        let e = egress
            .as_object()
            .ok_or_else(|| err1("network.egress must be an object".to_string()))?;
        if let Some(ports) = e.get("ports") {
            let arr = ports
                .as_array()
                .ok_or_else(|| err1("network.egress.ports must be an array".to_string()))?;
            for p in arr {
                let n = p.as_i64().ok_or_else(|| {
                    err1("network.egress.ports items must be integers".to_string())
                })?;
                if !(1..=65535).contains(&n) {
                    return Err(err1(format!("egress port {n} outside [1, 65535]")));
                }
            }
        }
        if mode == "egress_allowlist" {
            let has_ports = e
                .get("ports")
                .and_then(Value::as_array)
                .map(|a| !a.is_empty())
                .unwrap_or(false);
            if !has_ports {
                return Err(err1(
                    "network.mode=egress_allowlist requires egress.ports".to_string(),
                ));
            }
        }
    } else if mode == "egress_allowlist" {
        return Err(err1(
            "network.mode=egress_allowlist requires egress.ports".to_string(),
        ));
    }

    // secrets
    let secrets = req_arr(obj, "secrets").map_err(err1)?;
    for s in secrets {
        let item = s
            .as_object()
            .ok_or_else(|| err1("secrets items must be objects".to_string()))?;
        for key in item.keys() {
            if !matches!(key.as_str(), "name" | "inject_as" | "required") {
                return Err(err1(format!("secrets item key {key:?} unknown")));
            }
        }
        let name = req_str(item, "name").map_err(err1)?;
        if !secret_name_ok(name) {
            return Err(err1(format!(
                "secrets[].name {name:?} must match ^[A-Z][A-Z0-9_]+$"
            )));
        }
        let inject = req_str(item, "inject_as").map_err(err1)?;
        if !matches!(inject, "env" | "file") {
            return Err(err1(format!("secrets[].inject_as {inject:?} not env/file")));
        }
        req_bool(item, "required").map_err(err1)?;
    }

    // health
    let health = req_obj(obj, "health").map_err(err1)?;
    let probe = req_obj(health, "probe").map_err(err1)?;
    let ptype = req_str(probe, "type").map_err(err1)?;
    if !matches!(ptype, "worker_ping" | "exec" | "http_get" | "none") {
        return Err(err1(format!(
            "health.probe.type {ptype:?} not in vocabulary"
        )));
    }
    if ptype == "exec" {
        let command_ok = probe
            .get("command")
            .and_then(Value::as_array)
            .map(|a| !a.is_empty() && a.iter().all(|v| v.is_string()))
            .unwrap_or(false);
        if !command_ok {
            return Err(err1(
                "health.probe.type=exec requires non-empty command array".to_string(),
            ));
        }
    }
    if ptype == "http_get" && req_str(probe, "endpoint").is_err() {
        return Err(err1(
            "health.probe.type=http_get requires endpoint".to_string(),
        ));
    }
    for (key, min) in [
        ("timeout_ms", 100i64),
        ("interval_seconds", 30),
        ("degraded_after", 1),
        ("broken_after", 1),
    ] {
        if let Some(v) = probe.get(key) {
            let n = v
                .as_i64()
                .ok_or_else(|| err1(format!("health.probe.{key} must be an integer")))?;
            if n < min {
                return Err(err1(format!("health.probe.{key} {n} below minimum {min}")));
            }
        }
    }
    if let Some(v) = health.get("on_run_recover") {
        if v.as_bool().is_none() {
            return Err(err1("health.on_run_recover must be a boolean".to_string()));
        }
    }

    // provenance 形状（license=unknown 与 digest 一致性在第 5 条）
    let provenance = req_obj(obj, "provenance").map_err(err1)?;
    let origin = req_str(provenance, "origin").map_err(err1)?;
    if !matches!(origin, "rip-native" | "opensource" | "internal-kit") {
        return Err(err1(format!(
            "provenance.origin {origin:?} not in vocabulary"
        )));
    }
    if req_str(provenance, "license").map_err(err1)?.is_empty() {
        return Err(err1("provenance.license must be non-empty".to_string()));
    }
    let deps = req_arr(provenance, "dependencies").map_err(err1)?;
    if deps.iter().any(|d| !d.is_string()) {
        return Err(err1(
            "provenance.dependencies items must be strings".to_string(),
        ));
    }
    if req_str(provenance, "tested").map_err(err1)?.is_empty() {
        return Err(err1("provenance.tested must be non-empty".to_string()));
    }
    if let Some(d) = provenance.get("image_digest") {
        let s = d
            .as_str()
            .ok_or_else(|| err1("provenance.image_digest must be a string".to_string()))?;
        let ok = s
            .strip_prefix("sha256:")
            .map(|rest| rest == "<digest>" || sha256_digest_ok(rest))
            .unwrap_or(false);
        if !ok {
            return Err(err1(format!(
                "provenance.image_digest {s:?} must be sha256:<64hex> or documented placeholder"
            )));
        }
    }
    if let Some(kl) = provenance.get("known_limits") {
        let arr = kl
            .as_array()
            .ok_or_else(|| err1("provenance.known_limits must be an array".to_string()))?;
        if arr.iter().any(|v| !v.is_string()) {
            return Err(err1(
                "provenance.known_limits items must be strings".to_string(),
            ));
        }
    }

    // 可选块形状
    if let Some(ctx) = obj.get("context") {
        let c = ctx
            .as_object()
            .ok_or_else(|| err1("context must be an object".to_string()))?;
        for key in ["summary", "docs"] {
            if let Some(v) = c.get(key) {
                if v.as_str().is_none() {
                    return Err(err1(format!("context.{key} must be a string")));
                }
            }
        }
    }
    if let Some(tags) = obj.get("tags") {
        let arr = tags
            .as_array()
            .ok_or_else(|| err1("tags must be an array".to_string()))?;
        if arr.iter().any(|v| !v.is_string()) {
            return Err(err1("tags items must be strings".to_string()));
        }
    }
    if let Some(examples) = obj.get("examples") {
        let arr = examples
            .as_array()
            .ok_or_else(|| err1("examples must be an array".to_string()))?;
        for ex in arr {
            let item = ex
                .as_object()
                .ok_or_else(|| err1("examples items must be objects".to_string()))?;
            if item.get("input").and_then(Value::as_object).is_none() {
                return Err(err1("examples[].input must be an object".to_string()));
            }
            if let Some(oh) = item.get("output_hint") {
                if oh.as_object().is_none() {
                    return Err(err1("examples[].output_hint must be an object".to_string()));
                }
            }
        }
    }

    Ok((id.to_string(), version.to_string()))
}

// ---------------------------------------------------------------------------
// 第 3 条：input_schema / output_schema 是合法 JSON Schema 文档
// ---------------------------------------------------------------------------

fn rule3_io_schemas(manifest: &Map<String, Value>, id: &str) -> Result<(), RegistryError> {
    for key in ["input_schema", "output_schema"] {
        let schema = manifest
            .get(key)
            .ok_or_else(|| reject(ValidationRule::IoSchema, Some(id), format!("missing {key}")))?;
        jsonschema_lite::check_schema_document(schema).map_err(|e| {
            reject(
                ValidationRule::IoSchema,
                Some(id),
                format!("{key} is not a valid JSON Schema document: {e}"),
            )
        })?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// 第 4 条：runtime 分支必填 + 镜像 digest 固定 + remote_runner 拒绝
// ---------------------------------------------------------------------------

fn rule4_runtime(
    manifest: &Map<String, Value>,
    id: &str,
    opts: &Options,
) -> Result<(), RegistryError> {
    let runtime =
        req_obj(manifest, "runtime").map_err(|e| reject(ValidationRule::Runtime, Some(id), e))?;
    let rtype =
        req_str(runtime, "type").map_err(|e| reject(ValidationRule::Runtime, Some(id), e))?;
    let err4 = |reason: String| reject(ValidationRule::Runtime, Some(id), reason);

    if rtype == "remote_runner" {
        return Err(err4(
            "remote_runner is not open for registration in Phase 1 (plan 6:368; vocabulary D-TC-01)".to_string(),
        ));
    }

    if matches!(rtype, "podman_vm" | "cli_adapter") {
        let image = req_str(runtime, "image").map_err(err4)?;
        match image_digest_part(image) {
            Err(e) => return Err(err4(format!("runtime.image {image:?}: {e}"))),
            Ok(Some(_)) if !opts.allow_placeholder_digest => {
                return Err(err4(format!(
                    "runtime.image {image:?} carries the `<digest>` placeholder; replace with the real 64-hex digest before registration (TOOL-CONTRACT 9.2/11.1)"
                )));
            }
            Ok(_) => {}
        }
    }

    match rtype {
        "podman_vm" => {
            let ep = req_arr(runtime, "entrypoint").map_err(err4)?;
            if ep.is_empty() || ep.iter().any(|v| !v.is_string()) {
                return Err(err4(
                    "runtime.entrypoint must be a non-empty array of strings".to_string(),
                ));
            }
            if let Some(mounts) = runtime.get("mounts") {
                let arr = mounts
                    .as_array()
                    .ok_or_else(|| err4("runtime.mounts must be an array".to_string()))?;
                for m in arr {
                    let item = m
                        .as_object()
                        .ok_or_else(|| err4("runtime.mounts items must be objects".to_string()))?;
                    let role = req_str(item, "role").map_err(err4)?;
                    if !matches!(role, "input" | "output") {
                        return Err(err4(format!("mounts[].role {role:?} not input/output")));
                    }
                    if req_str(item, "path").map_err(err4)?.is_empty() {
                        return Err(err4("mounts[].path must be non-empty".to_string()));
                    }
                    let mode = req_str(item, "mode").map_err(err4)?;
                    if !matches!(mode, "ro" | "rw") {
                        return Err(err4(format!("mounts[].mode {mode:?} not ro/rw")));
                    }
                }
            }
            if let Some(env) = runtime.get("env") {
                let e = env
                    .as_object()
                    .ok_or_else(|| err4("runtime.env must be an object".to_string()))?;
                if e.values().any(|v| !v.is_string()) {
                    return Err(err4("runtime.env values must be strings".to_string()));
                }
            }
            if let Some(v) = runtime.get("readonly_rootfs") {
                if v.as_bool().is_none() {
                    return Err(err4(
                        "runtime.readonly_rootfs must be a boolean".to_string(),
                    ));
                }
            }
        }
        "python_worker" => {
            let pv = req_str(runtime, "protocol_version").map_err(err4)?;
            if pv != "1" {
                return Err(err4(format!(
                    "runtime.protocol_version {pv:?} must be \"1\""
                )));
            }
            if req_str(runtime, "worker_name").map_err(err4)?.is_empty() {
                return Err(err4("runtime.worker_name must be non-empty".to_string()));
            }
            let cmd = req_arr(runtime, "command").map_err(err4)?;
            if cmd.is_empty() || cmd.iter().any(|v| !v.is_string()) {
                return Err(err4(
                    "runtime.command must be a non-empty array of strings".to_string(),
                ));
            }
            if let Some(r) = runtime.get("restart") {
                let s = r
                    .as_str()
                    .ok_or_else(|| err4("runtime.restart must be a string".to_string()))?;
                if !matches!(s, "pooled" | "per_run") {
                    return Err(err4(format!("runtime.restart {s:?} not pooled/per_run")));
                }
            }
        }
        "native_rust" => {
            if req_str(runtime, "binary").map_err(err4)?.is_empty() {
                return Err(err4("runtime.binary must be non-empty".to_string()));
            }
            if req_str(runtime, "subcommand").map_err(err4)?.is_empty() {
                return Err(err4("runtime.subcommand must be non-empty".to_string()));
            }
        }
        "mcp_client" => {
            if req_str(runtime, "endpoint").map_err(err4)?.is_empty() {
                return Err(err4("runtime.endpoint must be non-empty".to_string()));
            }
            if req_str(runtime, "tool_name").map_err(err4)?.is_empty() {
                return Err(err4("runtime.tool_name must be non-empty".to_string()));
            }
        }
        "http_api" => {
            let base = req_str(runtime, "base_url").map_err(err4)?;
            if !base.contains("://") {
                return Err(err4(
                    "runtime.base_url must look like a URI (scheme://...)".to_string(),
                ));
            }
            if req_str(runtime, "method").map_err(err4)?.is_empty() {
                return Err(err4("runtime.method must be non-empty".to_string()));
            }
            let auth = req_str(runtime, "auth").map_err(err4)?;
            if !matches!(auth, "none" | "bearer_secret" | "header_secret") {
                return Err(err4(format!(
                    "runtime.auth {auth:?} not in none/bearer_secret/header_secret"
                )));
            }
            if let Some(rm) = runtime.get("response_mapping") {
                if rm.as_object().is_none() {
                    return Err(err4(
                        "runtime.response_mapping must be an object".to_string(),
                    ));
                }
            }
        }
        "cli_adapter" => {
            let argv = req_arr(runtime, "argv_template").map_err(err4)?;
            if argv.iter().any(|v| !v.is_string()) {
                return Err(err4(
                    "runtime.argv_template items must be strings".to_string(),
                ));
            }
        }
        _ => unreachable!("vocabulary checked in rule 1"),
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// 第 5 条：license 非 unknown + image_digest 一致性
// ---------------------------------------------------------------------------

fn rule5_provenance(manifest: &Map<String, Value>, id: &str) -> Result<(), RegistryError> {
    let provenance = req_obj(manifest, "provenance")
        .map_err(|e| reject(ValidationRule::Provenance, Some(id), e))?;
    let err5 = |reason: String| reject(ValidationRule::Provenance, Some(id), reason);

    let license = req_str(provenance, "license").map_err(err5)?;
    if license == "unknown" {
        return Err(err5(
            "provenance.license=unknown must be resolved before registration (TOOL-INVENTORY TPscan ruling)".to_string(),
        ));
    }
    let runtime = req_obj(manifest, "runtime").map_err(err5)?;
    let rtype = req_str(runtime, "type").map_err(err5)?;
    if rtype == "podman_vm" {
        let digest = provenance
            .get("image_digest")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                err5("provenance.image_digest is required when runtime.type=podman_vm (TOOL-CONTRACT 2.3)".to_string())
            })?;
        let image = req_str(runtime, "image").map_err(err5)?;
        let image_digest = image_digest_part(image)
            .map_err(|e| err5(format!("runtime.image {image:?}: {e}")))
            .map(|placeholder| {
                if placeholder.is_some() {
                    "sha256:<digest>".to_string()
                } else {
                    format!("sha256:{}", &image[image.len() - 64..])
                }
            })?;
        if digest != image_digest {
            return Err(err5(format!(
                "provenance.image_digest {digest:?} does not match runtime.image digest {image_digest:?}"
            )));
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// 第 6 条：timeout 与 runtime 类型上限不冲突
// ---------------------------------------------------------------------------

fn rule6_resources(manifest: &Map<String, Value>, id: &str) -> Result<(), RegistryError> {
    let resources = req_obj(manifest, "resources")
        .map_err(|e| reject(ValidationRule::Resources, Some(id), e))?;
    let err6 = |reason: String| reject(ValidationRule::Resources, Some(id), reason);
    let timeout = resources
        .get("timeout_seconds")
        .and_then(Value::as_i64)
        .ok_or_else(|| err6("resources.timeout_seconds must be an integer".to_string()))?;
    if !(1..=600).contains(&timeout) {
        return Err(err6(format!(
            "resources.timeout_seconds {timeout} outside [1, 600]"
        )));
    }
    if let Some(mob) = resources.get("max_output_bytes") {
        let v = mob
            .as_i64()
            .ok_or_else(|| err6("resources.max_output_bytes must be an integer".to_string()))?;
        if !(256..=67108864).contains(&v) {
            return Err(err6(format!(
                "resources.max_output_bytes {v} outside [256, 67108864]"
            )));
        }
    }
    let runtime = req_obj(manifest, "runtime").map_err(err6)?;
    let rtype = req_str(runtime, "type").map_err(err6)?;
    if rtype == "python_worker" {
        let ms = timeout
            .checked_mul(1000)
            .ok_or_else(|| err6("timeout_seconds overflow".to_string()))?;
        if !(1..=600000).contains(&ms) {
            return Err(err6(format!(
                "python_worker budget.timeout_ms {ms} outside [1, 600000] (worker_protocol v1)"
            )));
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// 第 8 条：examples 逐条合 input_schema / output_schema
// ---------------------------------------------------------------------------

fn rule8_examples(manifest: &Map<String, Value>, id: &str) -> Result<(), RegistryError> {
    // examples 为可选字段（§2.5 第 8 条：「examples 存在时」才逐条校验）。
    let Some(examples_value) = manifest.get("examples") else {
        return Ok(());
    };
    let input_schema = manifest.get("input_schema").ok_or_else(|| {
        reject(
            ValidationRule::Examples,
            Some(id),
            "missing input_schema".to_string(),
        )
    })?;
    let output_schema = manifest.get("output_schema");
    let examples = examples_value.as_array().ok_or_else(|| {
        reject(
            ValidationRule::Examples,
            Some(id),
            "examples must be an array".to_string(),
        )
    })?;
    for (i, ex) in examples.iter().enumerate() {
        let item = ex.as_object().ok_or_else(|| {
            reject(
                ValidationRule::Examples,
                Some(id),
                format!("examples[{i}] must be an object"),
            )
        })?;
        let input = item.get("input").ok_or_else(|| {
            reject(
                ValidationRule::Examples,
                Some(id),
                format!("examples[{i}].input missing"),
            )
        })?;
        jsonschema_lite::validate_instance(input_schema, input).map_err(|e| {
            reject(
                ValidationRule::Examples,
                Some(id),
                format!("examples[{i}].input fails input_schema: {e}"),
            )
        })?;
        if let Some(hint) = item.get("output_hint") {
            if let Some(os) = output_schema {
                // output_hint 按 hint 语义做子集校验（§8.1 示例的 output_hint
                // 即为部分提示）：hint 中出现的每个键须合 output_schema 的
                // 对应属性 schema；整体完整性由运行期信封校验把关。
                check_output_hint(os, hint).map_err(|e| {
                    reject(
                        ValidationRule::Examples,
                        Some(id),
                        format!("examples[{i}].output_hint fails output_schema: {e}"),
                    )
                })?;
            }
        }
    }
    Ok(())
}

/// output_hint 子集校验：hint 键须出现在 output_schema.properties 且值合其
/// 属性 schema；output_schema 声明 additionalProperties: false 时未知键拒绝。
fn check_output_hint(output_schema: &Value, hint: &Value) -> Result<(), String> {
    let Some(os) = output_schema.as_object() else {
        return Ok(());
    };
    let Some(hint_obj) = hint.as_object() else {
        return Ok(());
    };
    let props = os.get("properties").and_then(Value::as_object);
    let closed = os.get("additionalProperties") == Some(&Value::Bool(false));
    for (key, value) in hint_obj {
        match props.and_then(|p| p.get(key)) {
            Some(sub) => {
                jsonschema_lite::validate_instance(sub, value)
                    .map_err(|e| format!("hint key `{key}`: {e}"))?;
            }
            None => {
                if closed {
                    return Err(format!("hint key `{key}` not in output_schema properties"));
                }
            }
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// 静态汇总
// ---------------------------------------------------------------------------

/// 静态校验（第 1、3、4、5、6、8 条）。通过后拿 [`ValidatedManifest`]
/// 进入第 2 / 7 条与落库（第 9 条）。
pub fn validate_static(
    manifest: &Value,
    opts: &Options,
) -> Result<ValidatedManifest, RegistryError> {
    let (id, version) = rule1_structural(manifest)?;
    let obj = manifest.as_object().expect("checked object in rule 1");
    rule3_io_schemas(obj, &id)?;
    rule4_runtime(obj, &id, opts)?;
    rule5_provenance(obj, &id)?;
    rule6_resources(obj, &id)?;
    rule8_examples(obj, &id)?;
    Ok(ValidatedManifest {
        manifest: manifest.clone(),
        id,
        version,
        content_hash: content_hash(manifest),
    })
}

// ---------------------------------------------------------------------------
// 第 2 条：id 不冲突且 version 不回退（§6.2 semver，UNIQUE 兜底）
// ---------------------------------------------------------------------------

/// 检查 (id, version) 可登记：同 id 下新版本必须严格大于全部既有版本。
pub fn check_version_comparable(pool: &Pool, id: &str, version: &str) -> Result<(), RegistryError> {
    let versions: Vec<String> = pool
        .with_conn(|c| {
            let mut stmt = c.prepare("SELECT version FROM tool_version WHERE tool_id = ?1")?;
            let rows = stmt.query_map([id], |r| r.get::<_, String>(0))?;
            let mut out = Vec::new();
            for r in rows {
                out.push(r?);
            }
            Ok::<Vec<String>, database::rusqlite::Error>(out)
        })
        .map_err(RegistryError::from)?;

    let new_v = semver_parse(version).ok_or_else(|| {
        reject(
            ValidationRule::VersionComparable,
            Some(id),
            format!("version {version:?} is not X.Y.Z numeric semver"),
        )
    })?;
    for existing in &versions {
        if let Some(old) = semver_parse(existing) {
            if old >= new_v {
                return Err(reject(
                    ValidationRule::VersionComparable,
                    Some(id),
                    format!(
                        "version {version} is not greater than existing version {existing} (duplicate or downgrade, 6.2 semver)"
                    ),
                ));
            }
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// 第 7 条：secrets[].name 在平台 secrets 存储中存在
// ---------------------------------------------------------------------------

/// 平台 secrets 存储侧信：返回键是否存在。Phase 1 数据面尚无 secrets 表，
/// 由调用方注入（缺省 None = 无存储可查）。
pub type SecretsLookup<'a> = &'a (dyn Fn(&str) -> bool + Send + Sync);

/// 第 7 条：required=true 的 secret 须在平台 secrets 存储中存在；
/// required=false 按宽告警语义放行（§2.5 第 7 条原文）。
pub fn check_secrets(
    manifest: &Map<String, Value>,
    id: &str,
    lookup: Option<SecretsLookup<'_>>,
) -> Result<(), RegistryError> {
    let secrets =
        req_arr(manifest, "secrets").map_err(|e| reject(ValidationRule::Secrets, Some(id), e))?;
    for s in secrets {
        let item = s.as_object().ok_or_else(|| {
            reject(
                ValidationRule::Secrets,
                Some(id),
                "secrets items must be objects".to_string(),
            )
        })?;
        let name =
            req_str(item, "name").map_err(|e| reject(ValidationRule::Secrets, Some(id), e))?;
        let required = item
            .get("required")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        if !required {
            // required=false：宽告警语义（§2.5 第 7 条），允许登记。
            continue;
        }
        let exists = lookup.ok_or_else(|| {
            reject(
                ValidationRule::Secrets,
                Some(id),
                format!("secret {name:?} is required but no platform secrets store is configured"),
            )
        })?(name);
        if !exists {
            return Err(reject(
                ValidationRule::Secrets,
                Some(id),
                format!("secret {name:?} not found in platform secrets store"),
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn minimal_valid_manifest() -> Value {
        json!({
            "schema_version": "1",
            "id": "rip.test.min",
            "name": "Minimal",
            "version": "0.1.0",
            "kind": "tool",
            "family": "utility",
            "description": "最小合法 Manifest",
            "runtime": {
                "type": "python_worker",
                "protocol_version": "1",
                "worker_name": "w",
                "command": ["python3", "-I", "x.py"]
            },
            "input_schema": {"type": "object"},
            "output_schema": {"type": "object"},
            "resources": {"cpu": "1", "memory": "256Mi", "timeout_seconds": 60},
            "network": {"mode": "none"},
            "secrets": [],
            "permission_level": "T0",
            "idempotent": true,
            "health": {"probe": {"type": "worker_ping"}},
            "provenance": {
                "origin": "rip-native",
                "license": "repo-internal",
                "dependencies": [],
                "tested": "结构测试"
            }
        })
    }

    #[test]
    fn minimal_manifest_passes_static() {
        let m = minimal_valid_manifest();
        let vm = validate_static(&m, &Options::default()).unwrap();
        assert_eq!(vm.id, "rip.test.min");
        assert_eq!(vm.version, "0.1.0");
        assert_eq!(vm.content_hash.len(), 64);
    }

    #[test]
    fn rule1_missing_field_and_unknown_field() {
        let mut m = minimal_valid_manifest();
        m.as_object_mut().unwrap().remove("health");
        assert!(matches!(
            validate_static(&m, &Options::default()).unwrap_err(),
            RegistryError::Validation {
                rule: ValidationRule::RequiredFields,
                ..
            }
        ));
        let mut m2 = minimal_valid_manifest();
        m2.as_object_mut()
            .unwrap()
            .insert("hacks".to_string(), json!(1));
        assert!(validate_static(&m2, &Options::default()).is_err());
    }

    #[test]
    fn rule4_remote_runner_and_tag_image_rejected() {
        let mut m = minimal_valid_manifest();
        m.as_object_mut().unwrap()["runtime"] = json!({"type": "remote_runner"});
        assert!(matches!(
            validate_static(&m, &Options::default()).unwrap_err(),
            RegistryError::Validation {
                rule: ValidationRule::Runtime,
                ..
            }
        ));
        let mut m2 = minimal_valid_manifest();
        m2.as_object_mut().unwrap()["runtime"] = json!({
            "type": "podman_vm",
            "image": "registry.local/x:latest",
            "entrypoint": ["/bin/tool"]
        });
        assert!(validate_static(&m2, &Options::default()).is_err());
    }

    #[test]
    fn rule5_license_unknown_rejected() {
        let mut m = minimal_valid_manifest();
        m.as_object_mut().unwrap()["provenance"]["license"] = json!("unknown");
        assert!(matches!(
            validate_static(&m, &Options::default()).unwrap_err(),
            RegistryError::Validation {
                rule: ValidationRule::Provenance,
                ..
            }
        ));
    }

    #[test]
    fn id_and_semver_helpers() {
        assert!(manifest_id_ok("rip.doc.parse"));
        assert!(manifest_id_ok("a.b"));
        assert!(manifest_id_ok("rip.doc"));
        // P1-7b 修订：段内下划线合法（B2/B5 点名工具名）。
        assert!(manifest_id_ok("rip.kb.keyword_search"));
        assert!(manifest_id_ok("rip.ops.etl_status"));
        assert!(!manifest_id_ok("Rip.doc"));
        assert!(!manifest_id_ok("ripdoc"));
        assert!(!manifest_id_ok(".rip.doc"));
        assert!(!manifest_id_ok("rip.doc."));
        assert!(!manifest_id_ok("rip..parse"));
        // 放宽后段内任意位置下划线均合法（与冻结 schema pattern 一致）。
        assert!(manifest_id_ok("rip.doc.parse_"));
        // 大写仍非法（放宽仅限段内字符集小写域）。
        assert!(!manifest_id_ok("rip.doc.Parse_x"));
        assert_eq!(semver_parse("1.2.3"), Some((1, 2, 3)));
        assert_eq!(semver_parse("1.2"), None);
        assert_eq!(semver_parse("1.2.x"), None);
    }

    #[test]
    fn image_digest_helper() {
        let good = "registry.local/rip/x@sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        assert!(image_digest_part(good).unwrap().is_none());
        assert!(image_digest_part("registry.local/x@sha256:<digest>")
            .unwrap()
            .is_some());
        assert!(image_digest_part("registry.local/x:latest").is_err());
        assert!(image_digest_part("registry.local/x@sha256:XYZ").is_err());
    }
}
