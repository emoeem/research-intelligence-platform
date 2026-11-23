//! SARIF 渲染器（PHASE-6A D4/D1：`render_sarif`——SARIF 2.1.0 导出，D1
//! 映射表逐条落，零 I/O）。
//!
//! 对齐面（D1）：取 SARIF result/location/rule/artifact 四模型与 property
//! bag 扩展位；自有维度（severity/六 ID/rtis_ref/result_state）全部走
//! `properties.rip.*`，标准键零污染（外部 SARIF 工具可读标准面，自有维度
//! 无损可逆）。A16 模型要点：顶层 `$schema`+`version`+`runs[]`；run =
//! tool.driver{name,version,rules[]}/results[]/artifacts[]/properties。
//!
//! D1 映射表：
//! - severity → level：critical/high→error、medium→warning、low/info→note；
//! - result_state → kind：confirmed/false_positive→fail、其余→review；
//! - 六 ID → `properties.rip.six_ids`；rtis_ref → `properties.rip.rtis_refs`；
//!   result_state 原值 → `properties.rip.result_state`（三键恒在位）。
//!
//! 登记取舍：kind=review 时 result 仍携带 level（严格 SARIF 2.1.0 规范建议
//! kind≠fail 时省略 level——判据 8 要求 severity→level 逐 Finding 对账可见，
//! 按 D1 映射表字面全量携带；差距登记批次报告，6b 官方 SARIF schema 校验批
//! 收口——§8⑦）。

use crate::ir::{FindingIr, ReportIr};
use serde_json::{json, Map, Value};

/// severity → SARIF level 三值映射（D1）。
pub fn severity_to_level(severity: &str) -> &'static str {
    match severity {
        "critical" | "high" => "error",
        "medium" => "warning",
        // low / info（词表外值由 validate_ir fail-closed 拒绝，此处兜底 note）
        _ => "note",
    }
}

/// result_state → SARIF kind 二值映射（D1）。
pub fn result_state_to_kind(result_state: &str) -> &'static str {
    match result_state {
        "confirmed" | "false_positive" => "fail",
        // not_reproduced / not_tested / inconclusive
        _ => "review",
    }
}

/// IR → SARIF 2.1.0 Value（确定性纯函数——同 IR 两次渲染字节一致）。
pub fn render_sarif(ir: &ReportIr) -> Value {
    let rules: Vec<Value> = ir
        .findings
        .iter()
        .map(|f| {
            let mut rule = json!({
                "id": f.finding_id,
                "shortDescription": {"text": f.title},
                "defaultConfiguration": {"level": severity_to_level(&f.severity)},
            });
            if let Some(help_md) = f.technical_explanation.as_deref() {
                // 6B 收口（6A §8⑦ 登记差距——官方 SARIF schema 校验批）：
                // multiformatMessageString required ["text"]——补 text 键
                // （markdown 原文同值，语义无损 additive 一行 diff；红线 3
                // 冲突点登记批次报告坏消息）。
                rule["help"] = json!({"text": help_md, "markdown": help_md});
            }
            rule
        })
        .collect();

    let results: Vec<Value> = ir.findings.iter().map(render_result).collect();
    let artifacts: Vec<Value> = collect_artifacts(ir);

    json!({
        "$schema": "https://json-schema.org/sarif/sarif-2.1.0/schema.json",
        "version": "2.1.0",
        "runs": [
            {
                "tool": {
                    "driver": {
                        "name": "research-platform-report-engine",
                        "version": env!("CARGO_PKG_VERSION"),
                        "rules": rules,
                    }
                },
                "results": results,
                "artifacts": artifacts,
                "properties": {
                    "rip": {
                        "report_id": ir.report_id,
                        "report_kind": ir.report_kind,
                        "language": ir.language,
                    }
                },
            }
        ],
    })
}

/// 单 Finding → SARIF result（D1 映射：level/kind/locations/properties.rip）。
fn render_result(finding: &FindingIr) -> Value {
    let mut result = json!({
        "ruleId": finding.finding_id,
        "level": severity_to_level(&finding.severity),
        "kind": result_state_to_kind(&finding.result_state),
        "message": {"text": finding.title},
        "properties": {
            "rip": {
                "result_state": finding.result_state,
                "six_ids": six_ids_object(finding),
                "rtis_refs": rtis_refs_array(finding),
            }
        },
    });
    if let Some(location_value) = physical_location(finding) {
        result["locations"] = json!([location_value]);
    }
    result
}

/// physicalLocation 对齐位（location{artifact_uri, start_line, snippet}）。
fn physical_location(finding: &FindingIr) -> Option<Value> {
    let location = finding.location.as_ref()?;
    let mut physical = Map::new();
    if let Some(uri) = &location.artifact_uri {
        physical.insert("artifactLocation".to_string(), json!({"uri": uri}));
    }
    if let Some(start_line) = location.start_line {
        let mut region = Map::new();
        region.insert("startLine".to_string(), json!(start_line));
        if let Some(snippet) = &location.snippet {
            region.insert("snippet".to_string(), json!({"text": snippet}));
        }
        physical.insert("region".to_string(), Value::Object(region));
    }
    if physical.is_empty() {
        None
    } else {
        Some(json!({"physicalLocation": Value::Object(physical)}))
    }
}

/// 六 ID → properties.rip.six_ids（四 ID 列名键 + 去重值列表——015/021
/// 同款列名；键恒在位，空列表保形）。
fn six_ids_object(finding: &FindingIr) -> Value {
    let mut evidence_ids = Vec::new();
    let mut artifact_ids = Vec::new();
    let mut tool_run_ids = Vec::new();
    let mut trace_ids = Vec::new();
    for evidence in &finding.evidence {
        push_unique(&mut evidence_ids, &evidence.evidence_id);
        push_unique(&mut artifact_ids, &evidence.artifact_id);
        push_unique(&mut tool_run_ids, &evidence.tool_run_id);
        push_unique(&mut trace_ids, &evidence.trace_id);
    }
    json!({
        "evidence_ids": evidence_ids,
        "artifact_ids": artifact_ids,
        "tool_run_ids": tool_run_ids,
        "trace_ids": trace_ids,
    })
}

/// rtis_ref → properties.rip.rtis_refs（数组词形——键恒在位，无引用时空数组）。
fn rtis_refs_array(finding: &FindingIr) -> Value {
    match &finding.rtis_ref {
        Some(rtis_ref) => json!([rtis_ref]),
        None => json!([]),
    }
}

/// run.artifacts：全部 evidence uri + finding location artifact_uri（去重，
/// sha-256 hash 对账——判据 8 artifacts 与 evidence uri 对账面）。
fn collect_artifacts(ir: &ReportIr) -> Vec<Value> {
    let mut seen: Vec<String> = Vec::new();
    let mut artifacts = Vec::new();
    for finding in &ir.findings {
        if let Some(location) = &finding.location {
            if let Some(uri) = &location.artifact_uri {
                if !seen.contains(uri) {
                    seen.push(uri.clone());
                    artifacts.push(json!({"location": {"uri": uri}}));
                }
            }
        }
        for evidence in &finding.evidence {
            if seen.contains(&evidence.uri) {
                continue;
            }
            seen.push(evidence.uri.clone());
            artifacts.push(json!({
                "location": {"uri": evidence.uri},
                "hashes": {"sha-256": evidence.content_hash},
            }));
        }
    }
    artifacts
}

/// 去重追加（保序——确定性）。
fn push_unique(list: &mut Vec<String>, value: &str) {
    if !list.iter().any(|item| item == value) {
        list.push(value.to_string());
    }
}
