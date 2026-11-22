//! 负例拒收 + 词表单源对账（PHASE-6A 判据 4 + 红线 3 防漂移面）。
//!
//! 负例（D6）：severity 词表外值、五态词表外值、rtis_ref 非 rtis:// 词形 →
//! validate_ir 拒绝（fail-closed）且错误信息含字段名；附加词形负例（ID 前缀/
//! uri 词形/content_hash/未知字段/词表外 report_kind）与词表单源对账（schema
//! enum 与 Rust 常量逐值一致——两形态一事实源）。

use report_engine::ir::{
    IR_V1_SCHEMA, REMEDIATION_PRIORITIES, REPORT_KINDS, RESULT_STATES, SEVERITIES,
};
use report_engine::validate_ir;
use serde_json::{json, Value};

const MAIN_REPORT_JSON: &str = include_str!("fixtures/main-report.json");

/// 从 main fixture 深拷贝出发构造负例（其余字段全合法——负例单变量隔离）。
fn mutate_main(f: impl FnOnce(&mut Value)) -> Value {
    let mut value: Value = serde_json::from_str(MAIN_REPORT_JSON).expect("valid JSON");
    f(&mut value);
    value
}

fn first_finding_mut(value: &mut Value) -> &mut Value {
    &mut value["findings"][0]
}

fn assert_rejected_with_field_name(value: &Value, field: &str) {
    let err = validate_ir(value)
        .err()
        .unwrap_or_else(|| panic!("expected rejection mentioning {field:?}"));
    assert!(
        err.contains(field),
        "error must mention field {field:?}, got: {err}"
    );
}

/// 判据 4 负例 1：severity 词表外值拒绝。
#[test]
fn severity_outside_wordlist_rejected() {
    let mut value = mutate_main(|v| {
        first_finding_mut(v)["severity"] = json!("severe");
    });
    assert_rejected_with_field_name(&value, "severity");
    // Rust 侧兜底通道同样报字段名（jsonschema 先拒，两通道皆含字段名）。
    value["findings"][0]["rtis_ref"] = json!("rtis://poc/42");
    let err = validate_ir(&value)
        .err()
        .unwrap_or_else(|| panic!("expected rejection mentioning severity"));
    assert!(err.contains("severity"), "error mentions severity: {err}");
}

/// 判据 4 负例 2：五态词表外值拒绝。
#[test]
fn result_state_outside_wordlist_rejected() {
    let value = mutate_main(|v| {
        first_finding_mut(v)["result_state"] = json!("maybe");
    });
    assert_rejected_with_field_name(&value, "result_state");
}

/// 判据 4 负例 3：rtis_ref 非 rtis:// 词形拒绝。
#[test]
fn rtis_ref_wrong_shape_rejected() {
    for bad in [
        "rtis-poc:42",
        "rtis://task/42",
        "rtis://poc/",
        "rtis://poc/4x2",
    ] {
        let value = mutate_main(|v| {
            first_finding_mut(v)["rtis_ref"] = json!(bad);
        });
        let err = validate_ir(&value)
            .err()
            .unwrap_or_else(|| panic!("expected rejection for {bad:?}"));
        assert!(
            err.contains("rtis_ref"),
            "error must mention rtis_ref for {bad:?}, got: {err}"
        );
    }
}

/// 附加负例：ID 前缀词形（rep-/find-/re-）拒绝。
#[test]
fn id_prefix_shapes_rejected() {
    let value = mutate_main(|v| {
        v["report_id"] = json!("report-001");
    });
    assert_rejected_with_field_name(&value, "report_id");

    let value = mutate_main(|v| {
        first_finding_mut(v)["finding_id"] = json!("finding-001");
    });
    assert_rejected_with_field_name(&value, "finding_id");

    let value = mutate_main(|v| {
        first_finding_mut(v)["evidence"][0]["evidence_id"] = json!("evid-001");
    });
    assert_rejected_with_field_name(&value, "evidence_id");
}

/// 附加负例：evidence uri 词形（ripartifact:// | rtis://）与 content_hash
/// 64 hex 拒绝。
#[test]
fn evidence_uri_and_content_hash_shapes_rejected() {
    let value = mutate_main(|v| {
        first_finding_mut(v)["evidence"][0]["uri"] = json!("file:///tmp/x.json");
    });
    assert_rejected_with_field_name(&value, "uri");

    let value = mutate_main(|v| {
        first_finding_mut(v)["evidence"][0]["content_hash"] = json!("DEADBEEF");
    });
    assert_rejected_with_field_name(&value, "content_hash");
}

/// 附加负例：词表外 report_kind 与未知顶层字段拒绝（fail-closed 未知面）。
#[test]
fn report_kind_and_unknown_fields_rejected() {
    let value = mutate_main(|v| {
        v["report_kind"] = json!("spreadsheet_report");
    });
    assert_rejected_with_field_name(&value, "report_kind");

    let value = mutate_main(|v| {
        v["unknown_top_level"] = json!(true);
    });
    assert!(
        validate_ir(&value).is_err(),
        "unknown top-level field rejected"
    );

    let value = mutate_main(|v| {
        first_finding_mut(v)["unknown_finding_field"] = json!(1);
    });
    assert!(
        validate_ir(&value).is_err(),
        "unknown finding field rejected"
    );
}

/// 附加一致性负例：location start_line > end_line（schema 无法表达——Rust
/// 兜底拒绝）。
#[test]
fn location_start_line_greater_than_end_line_rejected() {
    let value = mutate_main(|v| {
        first_finding_mut(v)["location"]["start_line"] = json!(30);
        first_finding_mut(v)["location"]["end_line"] = json!(20);
    });
    assert_rejected_with_field_name(&value, "location");
}

/// 红线 3 词表单源对账：schema 五处 enum 与 Rust 词表常量逐值一致。
#[test]
fn schema_enums_match_rust_wordlists() {
    let schema: Value = serde_json::from_str(IR_V1_SCHEMA).expect("schema JSON");
    let enum_values = |pointer: &str| -> Vec<String> {
        schema
            .pointer(pointer)
            .unwrap_or_else(|| panic!("schema pointer {pointer}"))
            .as_array()
            .expect("enum array")
            .iter()
            .map(|v| v.as_str().expect("enum string").to_string())
            .collect()
    };
    assert_eq!(
        enum_values("/properties/report_kind/enum"),
        REPORT_KINDS
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>(),
        "report_kind enum single source"
    );
    assert_eq!(
        enum_values("/$defs/finding/properties/severity/enum"),
        SEVERITIES.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
        "severity enum single source"
    );
    assert_eq!(
        enum_values("/$defs/finding/properties/result_state/enum"),
        RESULT_STATES
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>(),
        "result_state enum single source"
    );
    assert_eq!(
        enum_values("/$defs/evidence_ref/properties/evidence_type/enum"),
        report_engine::ir::EVIDENCE_TYPES
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>(),
        "evidence_type enum single source"
    );
    assert_eq!(
        enum_values("/$defs/finding/properties/remediation/properties/priority/enum"),
        REMEDIATION_PRIORITIES
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>(),
        "remediation.priority enum single source"
    );
    assert_eq!(
        enum_values("/properties/coverage/properties/dimensions/items/properties/dimension/enum"),
        report_engine::ir::COVERAGE_DIMENSIONS
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>(),
        "coverage dimension enum single source"
    );
}

/// schema 单源内嵌一致：include_str! 文本与落盘文件字节一致（编译期指针面）。
#[test]
fn embedded_schema_is_the_single_source_file() {
    assert!(
        IR_V1_SCHEMA.contains("research-platform/schemas/report-ir/report-ir-v1.schema.json"),
        "$id pins the single source path"
    );
    assert!(
        IR_V1_SCHEMA.contains("draft/2020-12/schema"),
        "draft 2020-12 meta-schema"
    );
}
