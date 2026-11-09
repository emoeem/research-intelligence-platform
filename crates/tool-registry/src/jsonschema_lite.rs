//! JSON Schema draft 2020-12 校验门面（registry 内部用）。
//!
//! 用途（TOOL-CONTRACT §2.5 第 3、8 条）：校验 Manifest 的 input_schema /
//! output_schema 是合法 JSON Schema 文档，并用其校验 examples 的 input /
//! output_hint 与调用侧 input / result。
//!
//! 复用优先裁决（P1-4 复核）：本模块原为手写声明子集校验器（type/enum/
//! oneOf 等有限关键字，pattern 等强制型关键字出现即拒绝）。按工作区复用
//! 优先纪律（JSON Schema 校验属通用逻辑，不手写造轮子）与 ROADMAP §2.3
//! P1-4 的增量 vendor 通道，替换为维护中的 `jsonschema` crate（0.61，
//! default-features 关闭：工具 Schema 均为内联文档，远程/文件 $ref 解析
//! 关闭后外部引用在编译期即拒绝，fail-closed）。
//!
//! 模块名保留 `jsonschema_lite`：tool-engine 的调用点
//! （src/lib.rs、src/adapter/python_worker.rs）按既有路径引用本模块，
//! 改名属跨 crate 重命名，留待后续批次统一处理。
//!
//! 语义边界（与原实现一致的部分）：
//! - [`check_schema_document`] 按官方 2020-12 meta-schema 校验文档结构，
//!   覆盖「是否合法 JSON Schema 文档」（第 3 条）；
//! - [`validate_instance`] 编译 schema 后对实例做全量校验；
//! - output_hint 的部分提示语义（hint 只给部分键）不属本模块，由
//!   [`crate::validate::check_output_hint`] 按键级子集规则处理。

use serde_json::Value;

/// 用 schema 校验实例。Err 携带首个失败的说明。
pub fn validate_instance(schema: &Value, instance: &Value) -> Result<(), String> {
    let validator =
        jsonschema::validator_for(schema).map_err(|e| format!("schema compile failed: {e}"))?;
    validator.validate(instance).map_err(|e| e.to_string())
}

/// 校验一个 schema 文档本身的结构合法性（§2.5 第 3 条）：按 draft 2020-12
/// 官方 meta-schema 校验（未知 type 值、properties 非对象等一律拒绝）。
pub fn check_schema_document(schema: &Value) -> Result<(), String> {
    let meta = jsonschema::meta::validator_for(schema)
        .map_err(|e| format!("not a valid JSON Schema document: {e}"))?;
    meta.validate(schema)
        .map_err(|e| format!("not a valid JSON Schema document: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn object_required_and_additional_properties() {
        let schema = json!({
            "type": "object",
            "additionalProperties": false,
            "required": ["a"],
            "properties": {"a": {"type": "string"}}
        });
        assert!(validate_instance(&schema, &json!({"a": "x"})).is_ok());
        assert!(validate_instance(&schema, &json!({})).is_err());
        assert!(validate_instance(&schema, &json!({"a": "x", "b": 1})).is_err());
    }

    #[test]
    fn one_of_exactly_one() {
        let schema = json!({"oneOf": [{"required": ["a"]}, {"required": ["b"]}]});
        assert!(validate_instance(&schema, &json!({"a": 1})).is_ok());
        assert!(validate_instance(&schema, &json!({"a": 1, "b": 2})).is_err());
        assert!(validate_instance(&schema, &json!({})).is_err());
    }

    #[test]
    fn integer_and_bounds() {
        let schema = json!({"type": "integer", "minimum": 1, "maximum": 10});
        assert!(validate_instance(&schema, &json!(5)).is_ok());
        assert!(validate_instance(&schema, &json!(0)).is_err());
        assert!(validate_instance(&schema, &json!(11)).is_err());
        assert!(validate_instance(&schema, &json!(1.5)).is_err());
    }

    #[test]
    fn pattern_keyword_enforced_by_real_validator() {
        // 原子集校验器对 pattern 出现即拒绝；真库按语义实施。
        let schema = json!({"type": "string", "pattern": "^a+$"});
        assert!(validate_instance(&schema, &json!("aaa")).is_ok());
        assert!(validate_instance(&schema, &json!("bbb")).is_err());
    }

    #[test]
    fn schema_document_checks() {
        assert!(check_schema_document(&json!({"type": "object"})).is_ok());
        assert!(check_schema_document(&json!({"type": "nosuchtype"})).is_err());
        assert!(check_schema_document(&json!({"properties": 1})).is_err());
        assert!(check_schema_document(&json!(true)).is_ok());
    }
}
