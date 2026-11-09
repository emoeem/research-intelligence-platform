//! ToolRunResult 结果信封（TOOL-CONTRACT §5.1/§5.2/§5.3/§5.4）。
//!
//! 序列化形状与本仓冻结契约 schemas/tool-contract/result-v1.json 一致：
//! 空缺字段（artifacts/evidence_refs、TIMED_OUT 与 CANCELLED 的 summary、
//! SUCCEEDED 之外的 error/result）序列化时省略；条件必填由 [`validate`]
//! 按 status 强制。§5.4 截断由 [`truncate`] 统一实现，供所有 runtime 路径
//! 共用（两侧行为一致性的机制基础）。

use serde_json::{json, Map, Value};

use domain::ToolRunStatus;

/// 标准错误类别（§5.3 九类）。
pub const ERROR_CATEGORIES: [&str; 9] = [
    "VALIDATION",
    "INPUT",
    "TIMEOUT",
    "CANCELLED",
    "EXECUTION",
    "POLICY_DENIED",
    "DEPENDENCY",
    "INFRA",
    "RUNNER_UNAVAILABLE",
];

/// §5.1 error 对象。
#[derive(Debug, Clone, PartialEq)]
pub struct EnvelopeError {
    /// §5.3 类别。
    pub category: String,
    /// Worker Protocol 错误码（整数）或平台侧错误码字符串。
    pub code: Value,
    /// 错误消息。
    pub message: String,
    /// 可重试性。
    pub retryable: bool,
    /// 详细诊断 Artifact 引用（可选）。
    pub detail_ref: Option<String>,
}

/// §5.2 Artifact 条目。
#[derive(Debug, Clone, PartialEq)]
pub struct EnvelopeArtifact {
    /// 全局唯一 id（服务端登记时生成）。
    pub artifact_id: String,
    /// primary/attachment/stdout/stderr/log。
    pub role: String,
    /// 存储地址。
    pub uri: String,
    /// sha256 小写 hex（64）。
    pub sha256: String,
    /// 字节数。
    pub size_bytes: u64,
    /// MIME 类型。
    pub mime_type: String,
    /// 生成方式描述。
    pub provenance: String,
}

/// §5.1 usage 对象。
#[derive(Debug, Clone, PartialEq)]
pub struct Usage {
    /// 执行时长毫秒。
    pub elapsed_ms: u64,
    /// 输出字节数。
    pub output_bytes: u64,
    /// 是否截断（§5.4）。
    pub truncated: bool,
    /// CPU 毫秒（可得则填，否则 None）。
    pub cpu_ms: Option<u64>,
    /// 峰值内存字节（可得则填，否则 None）。
    pub max_rss_bytes: Option<u64>,
}

/// §5.1 environment 对象（按 runtime 类型取用，取不到为 None）。
#[derive(Debug, Clone, PartialEq)]
pub struct Environment {
    /// runtime 类型（词表见冻结契约，无 remote_runner）。
    pub runtime_type: String,
    /// runner 节点。
    pub runtime_node_id: Option<String>,
    /// 镜像 digest。
    pub image_digest: Option<String>,
    /// worker 协议版本。
    pub worker_protocol_version: Option<String>,
    /// 宿主指纹哈希。
    pub host_fingerprint_hash: Option<String>,
}

/// §5.1 replay 对象。
#[derive(Debug, Clone, PartialEq)]
pub struct Replay {
    /// 脱敏后输入。
    pub input_sanitized: Value,
    /// 幂等提交键。
    pub submit_key: Option<String>,
    /// 幂等命中标记（python_worker cached=true 透传）。
    pub cached: bool,
}

/// ToolRunResult 信封（§5.1）。
#[derive(Debug, Clone, PartialEq)]
pub struct ToolRunResult {
    /// 协议主版本（恒 1）。
    pub protocol_version: String,
    /// 运行 id。
    pub run_id: String,
    /// 工具 id。
    pub tool_id: String,
    /// 实际执行版本。
    pub tool_version: String,
    /// 终态（信封只承载终态）。
    pub status: ToolRunStatus,
    /// 结构化输出（仅 SUCCEEDED 有值）。
    pub result: Option<Value>,
    /// 人类可读摘要（SUCCEEDED/FAILED 必填）。
    pub summary: Option<String>,
    /// 标准错误（非 SUCCEEDED 必填）。
    pub error: Option<EnvelopeError>,
    /// Artifact 列表。
    pub artifacts: Vec<EnvelopeArtifact>,
    /// 证据引用。
    pub evidence_refs: Vec<String>,
    /// 执行用量。
    pub usage: Usage,
    /// 环境指纹。
    pub environment: Environment,
    /// 重放信息。
    pub replay: Replay,
}

/// 信封不变量校验（§5.1 条件必填与互斥；对应冻结契约 result-v1.json）。
pub fn validate(result: &ToolRunResult) -> Result<(), String> {
    if result.protocol_version != "1" {
        return Err(format!(
            "protocol_version {:?} must be \"1\"",
            result.protocol_version
        ));
    }
    if !result.status.is_terminal() {
        return Err(format!(
            "envelope status {:?} must be terminal",
            result.status
        ));
    }
    if result.status == ToolRunStatus::Succeeded {
        if result.result.is_none() {
            return Err("SUCCEEDED requires result".to_string());
        }
        if result.summary.as_deref().map(str::is_empty).unwrap_or(true) {
            return Err("SUCCEEDED requires non-empty summary".to_string());
        }
        if result.error.is_some() {
            return Err("SUCCEEDED must not carry error".to_string());
        }
    } else {
        let error = result
            .error
            .as_ref()
            .ok_or_else(|| format!("{:?} requires error", result.status))?;
        if !ERROR_CATEGORIES.contains(&error.category.as_str()) {
            return Err(format!(
                "error.category {:?} not in 5.3 vocabulary",
                error.category
            ));
        }
        if result.result.is_some() {
            return Err(format!("{:?} must not carry result", result.status));
        }
        if result.status == ToolRunStatus::Failed
            && result.summary.as_deref().map(str::is_empty).unwrap_or(true)
        {
            return Err("FAILED requires non-empty summary".to_string());
        }
    }
    for a in &result.artifacts {
        if !matches!(
            a.role.as_str(),
            "primary" | "attachment" | "stdout" | "stderr" | "log"
        ) {
            return Err(format!("artifact role {:?} not in 5.2 vocabulary", a.role));
        }
        if a.sha256.len() != 64
            || !a
                .sha256
                .chars()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
        {
            return Err(format!(
                "artifact sha256 {:?} is not 64 lowercase hex",
                a.sha256
            ));
        }
    }
    Ok(())
}

/// 序列化为冻结契约形状（空缺字段省略，键序由 serde_json BTreeMap 决定）。
pub fn to_value(result: &ToolRunResult) -> Value {
    let mut m = Map::new();
    m.insert("protocol_version".into(), json!(result.protocol_version));
    m.insert("run_id".into(), json!(result.run_id));
    m.insert("tool_id".into(), json!(result.tool_id));
    m.insert("tool_version".into(), json!(result.tool_version));
    m.insert("status".into(), json!(result.status.as_str()));
    if let Some(r) = &result.result {
        m.insert("result".into(), r.clone());
    }
    if let Some(s) = &result.summary {
        m.insert("summary".into(), json!(s));
    }
    if let Some(e) = &result.error {
        m.insert(
            "error".into(),
            json!({
                "category": e.category,
                "code": e.code,
                "message": e.message,
                "retryable": e.retryable,
                "detail_ref": e.detail_ref,
            }),
        );
    }
    if !result.artifacts.is_empty() {
        let arr: Vec<Value> = result
            .artifacts
            .iter()
            .map(|a| {
                json!({
                    "artifact_id": a.artifact_id,
                    "role": a.role,
                    "uri": a.uri,
                    "sha256": a.sha256,
                    "size_bytes": a.size_bytes,
                    "mime_type": a.mime_type,
                    "provenance": a.provenance,
                })
            })
            .collect();
        m.insert("artifacts".into(), Value::Array(arr));
    }
    if !result.evidence_refs.is_empty() {
        m.insert("evidence_refs".into(), json!(result.evidence_refs));
    }
    m.insert(
        "usage".into(),
        json!({
            "elapsed_ms": result.usage.elapsed_ms,
            "output_bytes": result.usage.output_bytes,
            "truncated": result.usage.truncated,
            "cpu_ms": result.usage.cpu_ms,
            "max_rss_bytes": result.usage.max_rss_bytes,
        }),
    );
    m.insert(
        "environment".into(),
        json!({
            "runtime_type": result.environment.runtime_type,
            "runtime_node_id": result.environment.runtime_node_id,
            "image_digest": result.environment.image_digest,
            "worker_protocol_version": result.environment.worker_protocol_version,
            "host_fingerprint_hash": result.environment.host_fingerprint_hash,
        }),
    );
    m.insert(
        "replay".into(),
        json!({
            "input_sanitized": result.replay.input_sanitized,
            "submit_key": result.replay.submit_key,
            "cached": result.replay.cached,
        }),
    );
    Value::Object(m)
}

/// §5.4 截断：结构化结果超过 `max_output_bytes` 时 result 降级为截断说明，
/// 原始字节交由调用方落 Artifact（返回值携带原始字节与建议 sha256/artifact_id），
/// usage.truncated = true。所有 runtime 路径共用本函数（两侧行为一致）。
pub struct TruncationOutcome {
    /// 处理后的结果值（超限时为截断说明对象）。
    pub result: Value,
    /// 是否发生截断。
    pub truncated: bool,
    /// 原始序列化字节数。
    pub original_bytes: u64,
    /// 原始字节 sha256（落 Artifact 用，截断时才有值）。
    pub original_sha256: Option<String>,
    /// 建议的 Artifact id（截断时才有值；mock 生成规则 art-<hash16>）。
    pub artifact_id: Option<String>,
}

/// §5.4 截断入口：结果序列化字节数超过帽时降级为截断说明并携带原始字节
/// 的 sha256 与建议 artifact_id；未超限则原样返回。
/// artifact_id 由 run_id + 结果内容共同派生（同内容不同 run 不碰撞，
/// 全局唯一性按 §5.2「服务端登记时生成」语义由派生保证）。
pub fn truncate(run_id: &str, result: &Value, max_output_bytes: u64) -> TruncationOutcome {
    let serialized = tool_registry::normalize_json(result);
    let bytes = serialized.len() as u64;
    if bytes <= max_output_bytes {
        return TruncationOutcome {
            result: result.clone(),
            truncated: false,
            original_bytes: bytes,
            original_sha256: None,
            artifact_id: None,
        };
    }
    let sha = tool_registry::content_hash(result);
    let artifact_id = format!(
        "art-{}",
        &tool_registry::hash::sha256_hex(format!("{run_id}:{serialized}").as_bytes())[..16]
    );
    let notice = json!({
        "truncated": true,
        "reason": "structured result exceeded max_output_bytes; original bytes stored as artifact (TOOL-CONTRACT 5.4)",
        "original_bytes": bytes,
        "artifact_id": artifact_id,
        "run_id": run_id,
    });
    TruncationOutcome {
        result: notice,
        truncated: true,
        original_bytes: bytes,
        original_sha256: Some(sha),
        artifact_id: Some(artifact_id),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn succeeded_envelope(result: Value) -> ToolRunResult {
        ToolRunResult {
            protocol_version: "1".into(),
            run_id: "rn-x".into(),
            tool_id: "rip.t.x".into(),
            tool_version: "0.1.0".into(),
            status: ToolRunStatus::Succeeded,
            result: Some(result),
            summary: Some("ok".into()),
            error: None,
            artifacts: Vec::new(),
            evidence_refs: Vec::new(),
            usage: Usage {
                elapsed_ms: 1,
                output_bytes: 1,
                truncated: false,
                cpu_ms: None,
                max_rss_bytes: None,
            },
            environment: Environment {
                runtime_type: "python_worker".into(),
                runtime_node_id: None,
                image_digest: None,
                worker_protocol_version: Some("1".into()),
                host_fingerprint_hash: None,
            },
            replay: Replay {
                input_sanitized: json!({}),
                submit_key: None,
                cached: false,
            },
        }
    }

    #[test]
    fn validate_enforces_conditionals() {
        let good = succeeded_envelope(json!({"a": 1}));
        assert!(validate(&good).is_ok());
        // 非 SUCCEEDED 不携带 result
        let mut failed = good.clone();
        failed.status = ToolRunStatus::Failed;
        assert!(validate(&failed).is_err());
        // FAILED 需 error + summary
        failed.result = None;
        assert!(validate(&failed).is_err(), "missing error");
        failed.error = Some(EnvelopeError {
            category: "EXECUTION".into(),
            code: json!(-32006),
            message: "boom".into(),
            retryable: true,
            detail_ref: None,
        });
        assert!(validate(&failed).is_ok(), "error+summary 的 FAILED 合法");
        failed.summary = None;
        assert!(validate(&failed).is_err(), "FAILED 缺 summary 必须拒绝");
        failed.summary = Some("boom happened".into());
        assert!(validate(&failed).is_ok());
        // 非法类别
        failed.error.as_mut().unwrap().category = "NOPE".into();
        assert!(validate(&failed).is_err());
    }

    #[test]
    fn to_value_matches_frozen_shape() {
        let v = to_value(&succeeded_envelope(json!({"a": 1})));
        let obj = v.as_object().unwrap();
        for key in [
            "protocol_version",
            "run_id",
            "tool_id",
            "tool_version",
            "status",
            "result",
            "summary",
            "usage",
            "environment",
            "replay",
        ] {
            assert!(obj.contains_key(key), "missing {key}");
        }
        // 空 artifacts/evidence_refs 省略（§8.3 骨架形状）
        assert!(!obj.contains_key("artifacts"));
        assert!(!obj.contains_key("evidence_refs"));
        assert_eq!(obj["status"], json!("SUCCEEDED"));
        assert_eq!(obj["usage"]["truncated"], json!(false));
    }

    #[test]
    fn truncate_below_and_above_limit() {
        let small = json!({"a": 1});
        let out = truncate("rn-x", &small, 1024);
        assert!(!out.truncated);
        assert!(out.original_sha256.is_none());

        let big = json!({"blob": "x".repeat(4096)});
        let out = truncate("rn-x", &big, 256);
        assert!(out.truncated);
        assert_eq!(out.original_sha256.as_deref().unwrap().len(), 64);
        assert!(out.artifact_id.as_deref().unwrap().starts_with("art-"));
        let notice = out.result.as_object().unwrap();
        assert_eq!(notice["truncated"], json!(true));
        assert_eq!(
            notice["artifact_id"],
            json!(out.artifact_id.clone().unwrap())
        );
        assert_eq!(notice["original_bytes"], json!(out.original_bytes));
    }
}
