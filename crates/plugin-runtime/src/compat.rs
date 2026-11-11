//! 激活层兼容检查（PLUGIN-CONTRACT §7 激活层第 4 条；ROADMAP P6-7）。
//!
//! 三项区间比对（enable 时对当前平台求值，防「停用期间平台已变化」，§5.3）：
//! - compatibility.rip_core：平台版本区间（rule 6 静态层已查一次，enable 重查）；
//! - compatibility.manifest_schema：数组须包含平台支持的 Manifest 规范版本；
//! - compatibility.worker_protocol：声明值须与平台 worker 协议版本一致
//!   （schemas/worker-protocol/，当前 "v1"）。
//!
//! 静态层（VALIDATED 七条）与运行层（周期健康检查）分别是 validate.rs 与
//! health.rs 的职责，本模块只承担激活层兼容面。

use serde::Serialize;
use serde_json::Value;

use crate::RuntimeError;
use plugin_registry::{ConstraintSet, Semver};

/// 平台版本面（enable 求值对象；缺省对齐 workspace 与冻结契约）。
#[derive(Debug, Clone)]
pub struct Platform {
    /// 平台 rip_core 版本（X.Y.Z）。
    pub rip_core: String,
    /// 平台支持的 Manifest 规范版本。
    pub manifest_schema: String,
    /// 平台 worker 协议版本（schemas/worker-protocol/ 词形，当前 "v1"）。
    pub worker_protocol: String,
}

impl Default for Platform {
    fn default() -> Self {
        Self {
            rip_core: "0.2.0".to_string(),
            manifest_schema: "1".to_string(),
            worker_protocol: "v1".to_string(),
        }
    }
}

/// 兼容报告（机器可读；enable 拒绝时进错误 details）。
#[derive(Debug, Clone, Serialize)]
pub struct CompatReport {
    /// 三项全过。
    pub ok: bool,
    /// rip_core 区间比对。
    pub rip_core: RangeCheck,
    /// manifest_schema 比对。
    pub manifest_schema: ValueCheck,
    /// worker_protocol 比对。
    pub worker_protocol: ValueCheck,
}

impl CompatReport {
    /// 单行 JSON。
    pub fn to_json_string(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{\"ok\":false}".to_string())
    }
}

/// 区间比对结果。
#[derive(Debug, Clone, Serialize)]
pub struct RangeCheck {
    /// Manifest 声明。
    pub declared: String,
    /// 平台版本。
    pub platform: String,
    /// 是否满足。
    pub satisfied: bool,
    /// 未满足说明。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

/// 词值比对结果。
#[derive(Debug, Clone, Serialize)]
pub struct ValueCheck {
    /// Manifest 声明（未声明为 None）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub declared: Option<String>,
    /// 平台值。
    pub platform: String,
    /// 是否满足。
    pub satisfied: bool,
    /// 未满足说明。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

/// 激活层兼容检查（Manifest compatibility 块 vs 平台版本面）。
pub fn check_compatibility(
    manifest: &Value,
    platform: &Platform,
) -> Result<CompatReport, RuntimeError> {
    let compat = manifest
        .get("compatibility")
        .and_then(Value::as_object)
        .ok_or_else(|| {
            RuntimeError::new("compat_missing", "manifest has no `compatibility` object")
        })?;

    // 1) rip_core 区间。
    let declared_rip = compat
        .get("rip_core")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let platform_semver = Semver::parse(&platform.rip_core).ok_or_else(|| {
        RuntimeError::new(
            "platform_version_invalid",
            format!(
                "platform rip_core {:?} is not X.Y.Z semver",
                platform.rip_core
            ),
        )
    })?;
    let rip_core = match ConstraintSet::parse(&declared_rip) {
        Ok(range) => {
            let satisfied = range.matches(platform_semver);
            RangeCheck {
                declared: declared_rip.clone(),
                platform: platform.rip_core.clone(),
                satisfied,
                detail: (!satisfied).then(|| {
                    format!("platform {platform_semver} outside declared range {declared_rip:?}")
                }),
            }
        }
        Err(e) => RangeCheck {
            declared: declared_rip.clone(),
            platform: platform.rip_core.clone(),
            satisfied: false,
            detail: Some(format!("declared range invalid: {e}")),
        },
    };

    // 2) manifest_schema：声明数组须包含平台版本（未声明视为只认当前规范）。
    let manifest_schema = match compat.get("manifest_schema").and_then(Value::as_array) {
        Some(list) => {
            let declared: Vec<String> = list
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect();
            let satisfied = declared.iter().any(|v| v == &platform.manifest_schema);
            ValueCheck {
                declared: Some(declared.join(", ")),
                platform: platform.manifest_schema.clone(),
                satisfied,
                detail: (!satisfied).then(|| {
                    format!(
                        "platform manifest schema {:?} not in declared {:?}",
                        platform.manifest_schema, declared
                    )
                }),
            }
        }
        None => ValueCheck {
            declared: None,
            platform: platform.manifest_schema.clone(),
            satisfied: true,
            detail: None,
        },
    };

    // 3) worker_protocol：声明值与平台一致（未声明视为不依赖 worker 面）。
    let worker_protocol = match compat.get("worker_protocol").and_then(Value::as_str) {
        Some(declared) => {
            let satisfied = declared == platform.worker_protocol;
            ValueCheck {
                declared: Some(declared.to_string()),
                platform: platform.worker_protocol.clone(),
                satisfied,
                detail: (!satisfied).then(|| {
                    format!(
                        "declared worker protocol {declared:?} differs from platform {:?}",
                        platform.worker_protocol
                    )
                }),
            }
        }
        None => ValueCheck {
            declared: None,
            platform: platform.worker_protocol.clone(),
            satisfied: true,
            detail: None,
        },
    };

    let ok = rip_core.satisfied && manifest_schema.satisfied && worker_protocol.satisfied;
    Ok(CompatReport {
        ok,
        rip_core,
        manifest_schema,
        worker_protocol,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn all_three_checks_satisfied_and_unsatisfied() {
        let platform = Platform::default();
        let good = json!({"compatibility": {"rip_core": ">=0.2.0, <1.0.0",
            "manifest_schema": ["1"], "worker_protocol": "v1"}});
        let report = check_compatibility(&good, &platform).unwrap();
        assert!(report.ok);
        assert!(report.rip_core.satisfied);
        assert!(report.manifest_schema.satisfied);
        assert!(report.worker_protocol.satisfied);

        let bad = json!({"compatibility": {"rip_core": ">=0.2.0, <1.0.0",
            "manifest_schema": ["1"], "worker_protocol": "v2"}});
        let report2 = check_compatibility(&bad, &platform).unwrap();
        assert!(!report2.ok);
        assert!(!report2.worker_protocol.satisfied);

        let older_platform = Platform {
            rip_core: "0.1.0".to_string(),
            ..Platform::default()
        };
        let report3 = check_compatibility(&good, &older_platform).unwrap();
        assert!(!report3.ok);
        assert!(!report3.rip_core.satisfied);
    }
}
