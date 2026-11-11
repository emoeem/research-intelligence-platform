//! tool / tool_pack capability 落 Tool Registry（PLUGIN-CONTRACT §3.3；
//! ROADMAP P6-6）。
//!
//! 映射规则（插件 capability → tool-manifest v1 行，tool-registry 九条全过
//! 才落 tool/tool_version 表）：
//! - id = capability.id（tool.rip.text.parse 形态，D-TC-02 词表兼容）；
//! - runtime：capability.runtime.type 必须在计划 §6 七种之内
//!   （[`RUNTIME_SEVEN`]，超出即 capability_runtime_unknown）；remote_runner
//!   注册拒绝（TOOL-CONTRACT D-TC-01 Phase 1 口径，runtime_registration_refused）；
//!   python_worker 映射 command = ["python3", "-I", entry]（包内相对路径）；
//! - family 缺省 utility（provides.family 可覆盖）；
//! - permission_level：network=none 且 filesystem 只读 → T0，否则 T1；
//! - network：none 直映射；egress_allowlist 需要 egress.ports 明细（插件
//!   Manifest 无法表达）→ 注册拒绝（fail-closed）；
//! - input/output Schema 直传（rule 3 经 jsonschema_lite 复核文档合法性）；
//! - provenance：origin=rip-native（kit 来源由 project_endpoint 侧用
//!   internal-kit），license 继承插件；
//! - 幂等：同 id 同版本已在 Tool Registry → skipped（tool-registry rule 2
//!   要求新版本严格大于既有，重装同版本按 no-op 语义跳过）。

use std::path::Path;

use serde::Serialize;
use serde_json::{json, Value};

use crate::RuntimeError;
use database::Pool;
use tool_registry::{catalog, Registry};

/// 计划 §6 七种 runtime（PLUGIN-CONTRACT §3.3；tool capability 的合法词表）。
pub const RUNTIME_SEVEN: [&str; 7] = [
    "podman_vm",
    "python_worker",
    "native_rust",
    "mcp_client",
    "http_api",
    "cli_adapter",
    "remote_runner",
];

/// 注册报告（机器可读）。
#[derive(Debug, Clone, Serialize)]
pub struct ToolRegistrationReport {
    /// 来源插件 id。
    pub plugin_id: String,
    /// 来源插件版本。
    pub plugin_version: String,
    /// 新注册进 Tool Registry 的工具 id。
    pub registered: Vec<String>,
    /// 同版本已存在而跳过的工具 id（幂等 no-op）。
    pub skipped: Vec<String>,
    /// 版本冲突等未注册项（id, 原因）。
    pub deferred: Vec<(String, String)>,
}

impl ToolRegistrationReport {
    /// 单行 JSON。
    pub fn to_json_string(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{\"registered\":[]}".to_string())
    }
}

/// 注册全部 tool / tool_pack capability（install 的 REGISTERED 半边）。
pub fn register_tool_capabilities(
    pool: &Pool,
    manifest: &Value,
    package_dir: &Path,
) -> Result<ToolRegistrationReport, RuntimeError> {
    let obj = manifest
        .as_object()
        .ok_or_else(|| RuntimeError::new("manifest_invalid", "manifest must be a JSON object"))?;
    let plugin_id = obj
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let plugin_version = obj
        .get("version")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let license = obj
        .get("license")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let mut report = ToolRegistrationReport {
        plugin_id,
        plugin_version: plugin_version.clone(),
        registered: Vec::new(),
        skipped: Vec::new(),
        deferred: Vec::new(),
    };
    let registry = Registry::open(pool.clone());

    let Some(caps) = obj.get("capabilities").and_then(Value::as_array) else {
        return Ok(report);
    };
    for cap in caps {
        let cap_type = cap.get("type").and_then(Value::as_str).unwrap_or_default();
        if !matches!(cap_type, "tool" | "tool_pack") {
            continue;
        }
        let cap_id = cap.get("id").and_then(Value::as_str).unwrap_or_default();
        let cap_obj = cap
            .as_object()
            .ok_or_else(|| RuntimeError::new("manifest_invalid", "capability must be an object"))?;

        // runtime 七种之内校验（§3.3）。
        let runtime_type = cap
            .pointer("/runtime/type")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                RuntimeError::new(
                    "capability_runtime_missing",
                    format!("capability {cap_id:?} tool capability requires runtime.type"),
                )
            })?;
        if !RUNTIME_SEVEN.contains(&runtime_type) {
            return Err(RuntimeError::new(
                "capability_runtime_unknown",
                format!(
                    "capability {cap_id:?} runtime.type {runtime_type:?} not in the plan-seven vocabulary {RUNTIME_SEVEN:?}"
                ),
            ));
        }
        if runtime_type == "remote_runner" {
            return Err(RuntimeError::new(
                "runtime_registration_refused",
                format!(
                    "capability {cap_id:?} runtime.type remote_runner is not open for registration (TOOL-CONTRACT D-TC-01 Phase 1)"
                ),
            ));
        }

        // 幂等：同 id 同版本已登记 → skip。
        match catalog::query(pool, cap_id) {
            Ok(detail) => {
                if detail.versions.iter().any(|v| v.version == plugin_version) {
                    report.skipped.push(cap_id.to_string());
                    continue;
                }
                report.deferred.push((
                    cap_id.to_string(),
                    format!("existing versions {:?} conflict with {plugin_version} (tool-registry rule 2)", 
                        detail.versions.iter().map(|v| v.version.clone()).collect::<Vec<_>>()),
                ));
                continue;
            }
            Err(tool_registry::RegistryError::NotFound { .. }) => {}
            Err(e) => return Err(RuntimeError::new("db", e.to_string())),
        }

        let cap_val = Value::Object(cap_obj.clone());
        let tool_manifest =
            build_tool_manifest(&cap_val, cap_id, &plugin_version, &license, package_dir)?;
        registry.register(&tool_manifest).map_err(|e| {
            RuntimeError::new("tool_registration_failed", format!("{cap_id:?}: {e}"))
        })?;
        report.registered.push(cap_id.to_string());
    }
    Ok(report)
}

/// capability → tool-manifest v1 行（九条校验的输入）。
fn build_tool_manifest(
    cap: &Value,
    cap_id: &str,
    plugin_version: &str,
    license: &str,
    package_dir: &Path,
) -> Result<Value, RuntimeError> {
    let summary = cap
        .get("summary")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let runtime_type = cap
        .pointer("/runtime/type")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let entry = cap
        .get("entry")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();

    // resources：input_max_bytes 为插件侧语义，tool resources 词表不含，丢弃。
    let mut resources = json!({"cpu": "1", "memory": "256Mi", "timeout_seconds": 60});
    if let Some(cap_res) = cap.get("resources").and_then(Value::as_object) {
        for key in ["cpu", "memory", "timeout_seconds", "max_output_bytes"] {
            if let Some(v) = cap_res.get(key) {
                resources[key] = v.clone();
            }
        }
    }

    // permissions → network/permission_level（egress_allowlist 无 ports 明细，
    // fail-closed 拒绝）。
    let network_mode = cap
        .pointer("/permissions/network")
        .and_then(Value::as_str)
        .unwrap_or("none");
    if network_mode == "egress_allowlist" {
        return Err(RuntimeError::new(
            "network_ports_unexpressible",
            format!(
                "capability {cap_id:?} network.mode egress_allowlist requires egress.ports which the plugin Manifest cannot express; registration refused (fail-closed)"
            ),
        ));
    }
    let fs_perm = cap
        .pointer("/permissions/filesystem")
        .and_then(Value::as_str)
        .unwrap_or("none");
    let permission_level = if network_mode == "none"
        && matches!(fs_perm, "none" | "task_workspace_ro" | "model_dir_ro")
    {
        "T0"
    } else {
        "T1"
    };

    // runtime 分支映射（TOOL-CONTRACT D-TC-01 分支必填）。
    let runtime = match runtime_type.as_str() {
        "python_worker" => {
            let worker_name = Path::new(&entry)
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "worker".to_string());
            json!({
                "type": "python_worker",
                "protocol_version": "1",
                "worker_name": worker_name,
                "command": ["python3", "-I", entry],
                "working_dir": package_dir.to_string_lossy(),
                "restart": "pooled",
            })
        }
        "podman_vm" | "podman_host" => {
            let image = cap
                .pointer("/runtime/image")
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    RuntimeError::new(
                        "tool_registration_failed",
                        format!("capability {cap_id:?} {runtime_type} requires runtime.image (digest-pinned)"),
                    )
                })?
                .to_string();
            let digest = image
                .rsplit("@sha256:")
                .next()
                .map(|d| format!("sha256:{d}"))
                .unwrap_or_else(|| "sha256:<digest>".to_string());
            let mut rt = json!({
                "type": runtime_type,
                "image": image,
                "entrypoint": cap.pointer("/runtime/entrypoint").cloned().unwrap_or(json!([])),
            });
            rt["provenance_image_digest"] = json!(digest);
            rt
        }
        "native_rust" => json!({
            "type": "native_rust",
            "binary": cap.pointer("/runtime/binary").and_then(Value::as_str).unwrap_or_default(),
            "subcommand": cap.pointer("/runtime/subcommand").and_then(Value::as_str).unwrap_or_default(),
        }),
        "mcp_client" => json!({
            "type": "mcp_client",
            "endpoint": cap.pointer("/runtime/endpoint").and_then(Value::as_str).unwrap_or_default(),
            "tool_name": cap.pointer("/runtime/tool_name").and_then(Value::as_str).unwrap_or_default(),
        }),
        "http_api" => json!({
            "type": "http_api",
            "base_url": cap.pointer("/runtime/base_url").and_then(Value::as_str).unwrap_or_default(),
            "method": cap.pointer("/runtime/method").and_then(Value::as_str).unwrap_or("GET"),
            "auth": "none",
        }),
        "cli_adapter" => json!({
            "type": "cli_adapter",
            "argv_template": cap.pointer("/runtime/argv_template").cloned().unwrap_or(json!([])),
        }),
        other => {
            return Err(RuntimeError::new(
                "capability_runtime_unknown",
                format!("runtime.type {other:?} not in the plan-seven vocabulary"),
            ))
        }
    };

    let mut manifest = json!({
        "schema_version": "1",
        "id": cap_id,
        "name": summary.chars().take(80).collect::<String>(),
        "version": plugin_version,
        "kind": "tool",
        "family": cap.pointer("/provides/family").and_then(Value::as_str).unwrap_or("utility"),
        "description": summary,
        "runtime": runtime,
        "input_schema": cap.get("input_schema").cloned().unwrap_or(json!({"type": "object"})),
        "output_schema": cap.get("output_schema").cloned().unwrap_or(json!({"type": "object"})),
        "resources": resources,
        "network": {"mode": network_mode},
        "secrets": [],
        "permission_level": permission_level,
        "idempotent": true,
        "health": {"probe": {"type": if runtime_type == "python_worker" { "worker_ping" } else { "none" }}},
        "provenance": {
            "origin": "rip-native",
            "license": license,
            "dependencies": [],
            "tested": "P6-6 tool plugin capability registration",
        },
    });
    // podman 分支的 provenance.image_digest（rule 5 一致性）。
    if let Some(digest) = manifest
        .pointer("/runtime/provenance_image_digest")
        .and_then(Value::as_str)
    {
        manifest["provenance"]["image_digest"] = json!(digest);
        if let Some(rt_obj) = manifest.get_mut("runtime").and_then(Value::as_object_mut) {
            rt_obj.remove("provenance_image_digest");
        }
    }
    Ok(manifest)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_vocabulary_is_plan_seven() {
        assert_eq!(RUNTIME_SEVEN.len(), 7);
        assert!(RUNTIME_SEVEN.contains(&"python_worker"));
        assert!(RUNTIME_SEVEN.contains(&"remote_runner"));
        // podman_host 是 Phase 2 过渡型，不在计划七种内（v1.json capability
        // runtime 词表含它，tool_register 七种校验会拒）。
        assert!(!RUNTIME_SEVEN.contains(&"podman_host"));
    }
}
