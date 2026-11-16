//! 契约 Schema 装载与版本常量（P5-2）。
//!
//! 装载惯例（research-engine contract.rs 同款）：`schemas/verification/`
//! 四份 v1 Schema 编译期 `include_str!` 内嵌（磁盘文件为源），运行期无路径
//! 依赖；本模块提供：
//! - 版本常量（[`SCHEMA_VERSION`]，升级需同步 Schema `$id` 与契约测试）；
//! - [`validate_against`]：serde_json Value 对指定 Schema 的校验入口
//!   （jsonschema 0.56 draft 2020-12，default-features 关闭远程 $ref）；
//! - [`validate_plan_doc`] / [`validate_item_doc`] / [`validate_event_doc`] /
//!   [`validate_capability_doc`]：四个契约面的校验入口（契约测试与 store
//!   写面共用）；
//! - [`id_prefix_matches`]：ID 前缀形状守卫（vp-/vi-/ve-）。

use serde_json::Value;

/// 内嵌 Schema 相对仓库根的目录（编译期路径，仅文档说明用）。
pub const SCHEMA_DIR: &str = "schemas/verification";

/// 契约版本（P5-2 冻结为 1；升级需同步四份 Schema 的 `$id` 尾与本常量）。
pub const SCHEMA_VERSION: i64 = 1;

const PLAN_SCHEMA: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../schemas/verification/verification-plan.v1.json"
));
const ITEM_SCHEMA: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../schemas/verification/verification-item.v1.json"
));
const EVENT_SCHEMA: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../schemas/verification/verification-event.v1.json"
));
const CAPABILITY_SCHEMA: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../schemas/verification/provider-capability.v1.json"
));

/// Schema 编译（进程内缓存一次；非法 Schema 是编译期开发错误，panic 快失败）。
fn compile(schema_text: &str) -> jsonschema::Validator {
    let schema: Value = serde_json::from_str(schema_text).expect("verification schema json");
    jsonschema::validator_for(&schema).expect("verification schema compile")
}

/// Value 对 Schema 文本校验；返回全部错误（空 = 通过）。
pub fn validate_against(schema_text: &str, value: &Value) -> Result<(), Vec<String>> {
    let validator = compile(schema_text);
    let errors: Vec<String> = validator
        .iter_errors(value)
        .map(|e| e.to_string())
        .collect();
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

/// verification-plan.v1 校验入口。
pub fn validate_plan_doc(doc: &Value) -> Result<(), Vec<String>> {
    validate_against(PLAN_SCHEMA, doc)
}

/// verification-item.v1 校验入口。
pub fn validate_item_doc(doc: &Value) -> Result<(), Vec<String>> {
    validate_against(ITEM_SCHEMA, doc)
}

/// verification-event.v1 校验入口。
pub fn validate_event_doc(doc: &Value) -> Result<(), Vec<String>> {
    validate_against(EVENT_SCHEMA, doc)
}

/// provider-capability.v1 校验入口（P5-5 Adapter 契约测试消费）。
pub fn validate_capability_doc(doc: &Value) -> Result<(), Vec<String>> {
    validate_against(CAPABILITY_SCHEMA, doc)
}

/// ID 前缀形状守卫（vp-/vi-/ve-；store 生成与外部注入共用）。
pub fn id_prefix_matches(id: &str, prefix: &str) -> bool {
    id.starts_with(prefix) && id.len() > prefix.len()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn schemas_embed_and_declare_version() {
        for (text, id) in [
            (PLAN_SCHEMA, "verification-plan.v1.json"),
            (ITEM_SCHEMA, "verification-item.v1.json"),
            (EVENT_SCHEMA, "verification-event.v1.json"),
            (CAPABILITY_SCHEMA, "provider-capability.v1.json"),
        ] {
            let doc: Value = serde_json::from_str(text).expect("schema json");
            assert_eq!(doc["$schema"], "https://json-schema.org/draft/2020-12/schema");
            assert!(
                doc["$id"]
                    .as_str()
                    .unwrap_or_default()
                    .ends_with(id),
                "$id tail must match file name: {id}"
            );
        }
    }

    #[test]
    fn minimal_plan_doc_validates() {
        let doc = json!({
            "schema_version": 1,
            "plan_id": "vp-20261117-010203-004-abcd",
            "status": "draft",
            "workspace_id": "default",
            "created_at": "2026-11-17T01:02:03Z",
            "updated_at": "2026-11-17T01:02:03Z"
        });
        assert!(validate_plan_doc(&doc).is_ok());
    }

    #[test]
    fn plan_doc_rejects_unknown_status_and_bad_id() {
        let base = json!({
            "schema_version": 1,
            "plan_id": "vp-20261117-010203-004-abcd",
            "status": "running",
            "workspace_id": "default",
            "created_at": "2026-11-17T01:02:03Z",
            "updated_at": "2026-11-17T01:02:03Z"
        });
        let errs = validate_plan_doc(&base).unwrap_err();
        assert!(!errs.is_empty(), "unknown plan status must reject");
        let bad_id = json!({
            "schema_version": 1,
            "plan_id": "plan-1",
            "status": "draft",
            "workspace_id": "default",
            "created_at": "2026-11-17T01:02:03Z",
            "updated_at": "2026-11-17T01:02:03Z"
        });
        assert!(validate_plan_doc(&bad_id).is_err());
    }

    #[test]
    fn minimal_item_doc_validates() {
        let doc = json!({
            "schema_version": 1,
            "item_id": "vi-20261117-010203-004-abcd",
            "plan_id": "vp-20261117-010203-004-abcd",
            "provider_id": "rtis-poc",
            "target_ref": "https://target.example:443/",
            "dedup_fingerprint": "a".repeat(64),
            "applicability_status": "applicable",
            "risk_class": "low_impact",
            "request_budget": 10,
            "timeout_budget_s": 60,
            "execution_order": 0,
            "created_at": "2026-11-17T01:02:03Z",
            "updated_at": "2026-11-17T01:02:03Z"
        });
        assert!(validate_item_doc(&doc).is_ok());
    }

    #[test]
    fn item_doc_rejects_bad_fingerprint_and_rtis_poc_zero() {
        let mut doc = json!({
            "schema_version": 1,
            "item_id": "vi-20261117-010203-004-abcd",
            "plan_id": "vp-20261117-010203-004-abcd",
            "provider_id": "rtis-poc",
            "rtis_poc_id": 0,
            "target_ref": "https://target.example:443/",
            "dedup_fingerprint": "zz".repeat(32),
            "applicability_status": "applicable",
            "risk_class": "low_impact",
            "request_budget": 10,
            "timeout_budget_s": 60,
            "execution_order": 0,
            "created_at": "2026-11-17T01:02:03Z",
            "updated_at": "2026-11-17T01:02:03Z"
        });
        let errs = validate_item_doc(&doc).unwrap_err();
        assert!(errs.len() >= 2, "fingerprint + poc id both reject: {errs:?}");
        doc["rtis_poc_id"] = json!(900008);
        doc["dedup_fingerprint"] = json!("a".repeat(64));
        assert!(validate_item_doc(&doc).is_ok());
    }

    #[test]
    fn minimal_event_doc_validates() {
        let doc = json!({
            "schema_version": 1,
            "event_id": "ve-20261117-010203-004-abcd",
            "event_type": "plan_created",
            "created_at": "2026-11-17T01:02:03Z"
        });
        assert!(validate_event_doc(&doc).is_ok());
        let bad = json!({
            "schema_version": 1,
            "event_id": "ve-20261117-010203-004-abcd",
            "event_type": "random",
            "created_at": "2026-11-17T01:02:03Z"
        });
        assert!(validate_event_doc(&bad).is_err());
    }

    #[test]
    fn capability_doc_validates_and_rejects_shell_transport() {
        let doc = json!({
            "schema_version": 1,
            "provider_id": "rtis-poc",
            "provider_version": "0.1.0",
            "detection_kinds": ["cve-exploit"],
            "risk_class": "low_impact",
            "execution": {
                "transport": "cli_subprocess",
                "container_required": false
            }
        });
        assert!(validate_capability_doc(&doc).is_ok());
        let bad = json!({
            "schema_version": 1,
            "provider_id": "rtis-poc",
            "provider_version": "0.1.0",
            "detection_kinds": ["cve-exploit"],
            "risk_class": "low_impact",
            "execution": {
                "transport": "shell",
                "container_required": false
            }
        });
        assert!(validate_capability_doc(&bad).is_err());
    }

    #[test]
    fn id_prefix_guard() {
        assert!(id_prefix_matches("vp-20261117-010203-004-abcd", "vp-"));
        assert!(!id_prefix_matches("vi-20261117-010203-004-abcd", "vp-"));
        assert!(!id_prefix_matches("vp-", "vp-"));
    }
}
