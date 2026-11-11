//! Plugin Manifest 解析（strict 模式，PLUGIN-CONTRACT §4.1 与草案 D4）。
//!
//! 职责：把 manifest.yaml / manifest.yml / manifest.json 文本解析为
//! [`serde_json::Value`]（YAML 经 serde 数据模型转换），供 [`crate::validate`]
//! 按冻结契约 [`V1_SCHEMA`] 做全文校验。strict 语义：
//!
//! - 未知字段拒绝：由 v1.json 顶层与每个对象层的 `additionalProperties: false`
//!   表达，校验在 [`crate::validate::rule2`]（复用 tool-registry 的
//!   [`tool_registry::jsonschema_lite`]，jsonschema draft 2020-12）；
//! - YAML duplicate key 拒绝：serde_yaml_ng 反序列化到 Value 时对重复键
//!   报错（fail-closed，单测锁定该行为）；
//! - 非 JSON object 顶层（数组、标量）拒绝。
//!
//! 格式探测：去除前导空白后以 `{` 起始的文档按 JSON 解析，其余按 YAML
//! 解析（manifest.json 走 JSON 通道，manifest.yaml / manifest.yml 走 YAML）。

use std::sync::LazyLock;

use serde::Serialize;
use serde_json::Value;

/// Registry 支持的 Manifest 规范主版本（PLUGIN-CONTRACT §4.1：平台按此选
/// 校验器，不支持即 INVALID；§6.1 升级语义同 tool-registry 口径）。
pub const SUPPORTED_SCHEMA_VERSION: &str = "1";

/// 冻结契约 schemas/plugin-manifest/v1.json（draft 2020-12，P6-1）。
/// 编译期嵌入，保证离线构建与运行时校验用的是同一份冻结文本。
static V1_SCHEMA_JSON: &str = include_str!("../../../schemas/plugin-manifest/v1.json");

static V1_SCHEMA: LazyLock<Value> = LazyLock::new(|| {
    serde_json::from_str(V1_SCHEMA_JSON).expect("frozen schema file must be valid JSON")
});

/// 冻结契约 Schema 文档（draft 2020-12）的 JSON 视图。
pub fn v1_schema() -> &'static Value {
    &V1_SCHEMA
}

/// 冻结契约 Schema 的原始文本（诊断与外部工具对账用）。
pub fn v1_schema_json() -> &'static str {
    V1_SCHEMA_JSON
}

/// 解析错误（机器可读：code + message，serde_json 序列化即报告行）。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ParseError {
    /// 稳定错误码：yaml_syntax / json_syntax / non_string_key / not_object。
    pub code: String,
    /// 人类可读原因。
    pub message: String,
}

impl ParseError {
    fn new(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.to_string(),
            message: message.into(),
        }
    }

    /// 单行 JSON（机器可读报告）。
    pub fn to_json_string(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| {
            format!(
                "{{\"code\":\"{}\",\"message\":\"unserializable\"}}",
                self.code
            )
        })
    }
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "manifest parse error [{}]: {}", self.code, self.message)
    }
}

impl std::error::Error for ParseError {}

/// 解析一份 Manifest 文本（JSON 或 YAML 自动探测）为 `serde_json::Value`。
///
/// strict 边界（如实声明）：JSON 通道的重复键沿用 serde_json 默认覆盖语义
/// （不拒绝），strict 未知字段拒绝统一在 schema 校验层执行；YAML 通道的
/// 重复键由 serde_yaml_ng 拒绝（单测锁定），非字符串键由
/// [`ensure_yaml_string_keys`] 显式拒绝（实测 serde_yaml_ng 会把数字键
/// 字符串化，必须前置拦截，单测锁定）。
pub fn parse_document(doc: &str) -> Result<Value, ParseError> {
    let trimmed = doc.trim_start();
    if trimmed.starts_with('{') {
        let value: Value =
            serde_json::from_str(doc).map_err(|e| ParseError::new("json_syntax", e.to_string()))?;
        ensure_object(&value)?;
        Ok(value)
    } else {
        let yaml: serde_yaml_ng::Value = serde_yaml_ng::from_str(doc)
            .map_err(|e| ParseError::new("yaml_syntax", e.to_string()))?;
        ensure_yaml_string_keys(&yaml)?;
        let value = yaml_to_json(&yaml)?;
        ensure_object(&value)?;
        Ok(value)
    }
}

/// YAML 映射键全量检查：只接受字符串键（fail-closed，防静默字符串化）。
fn ensure_yaml_string_keys(yaml: &serde_yaml_ng::Value) -> Result<(), ParseError> {
    use serde_yaml_ng::Value as Y;
    match yaml {
        Y::Mapping(map) => {
            for (key, value) in map {
                if !matches!(key, Y::String(_)) {
                    return Err(ParseError::new(
                        "non_string_key",
                        format!("yaml mapping key must be a string, got {key:?}"),
                    ));
                }
                ensure_yaml_string_keys(value)?;
            }
            Ok(())
        }
        Y::Sequence(seq) => {
            for value in seq {
                ensure_yaml_string_keys(value)?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

fn ensure_object(value: &Value) -> Result<(), ParseError> {
    if value.is_object() {
        Ok(())
    } else {
        Err(ParseError::new(
            "not_object",
            "manifest document must be a JSON object at the top level",
        ))
    }
}

/// serde_yaml_ng Value 到 serde_json Value 的转换；非字符串键拒绝。
fn yaml_to_json(yaml: &serde_yaml_ng::Value) -> Result<Value, ParseError> {
    serde_json::to_value(yaml).map_err(|e| {
        ParseError::new(
            "non_string_key",
            format!("yaml mapping keys must be strings: {e}"),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn yaml_and_json_channels_agree() {
        let yaml = "schema_version: \"1\"\nid: \"a.b\"\nname: x\n";
        let json = r#"{"schema_version": "1", "id": "a.b", "name": "x"}"#;
        assert_eq!(parse_document(yaml).unwrap(), parse_document(json).unwrap());
    }

    #[test]
    fn yaml_duplicate_key_rejected() {
        let yaml = "id: \"a.b\"\nid: \"c.d\"\n";
        let err = parse_document(yaml).unwrap_err();
        assert_eq!(err.code, "yaml_syntax", "dup key must fail closed: {err}");
    }

    #[test]
    fn yaml_non_string_key_rejected() {
        let yaml = "1: \"x\"\nid: \"a.b\"\n";
        let err = parse_document(yaml).unwrap_err();
        assert_eq!(err.code, "non_string_key");
    }

    #[test]
    fn non_object_top_level_rejected() {
        assert_eq!(parse_document("[1, 2]").unwrap_err().code, "not_object");
        assert_eq!(parse_document("\"x\"").unwrap_err().code, "not_object");
    }

    #[test]
    fn json_syntax_error_carries_message() {
        let err = parse_document("{\"id\": }").unwrap_err();
        assert_eq!(err.code, "json_syntax");
        assert!(err.to_json_string().contains("\"code\":\"json_syntax\""));
    }

    #[test]
    fn frozen_schema_is_valid_json() {
        let schema = v1_schema();
        assert!(schema.is_object());
        assert_eq!(
            schema.get("$schema").and_then(Value::as_str),
            Some("https://json-schema.org/draft/2020-12/schema")
        );
    }
}
