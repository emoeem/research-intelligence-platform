//! P7-6 契约测试：14 份 Schema 装载全量门、契约往返、七能力 mock 行为、
//! Schema 符合性（含 FAILED 信封也必须过 result-v1）、六 ID 锚域类型接线。
//!
//! 口径：全部走 [`research_engine::contract`] 公开面（Schema 装载与校验即
//! crate 自身交付面），不直接依赖 jsonschema crate。

use research_engine::capabilities::citation::{CitationCapability, CitationInput, CitationStyle};
use research_engine::capabilities::discovery::{DiscoveryCapability, DiscoveryInput};
use research_engine::capabilities::evaluator::{
    EvaluatorCapability, EvaluatorCheck, EvaluatorInput, SubjectKind, SubjectRef,
};
use research_engine::capabilities::extraction::{ExtractionCapability, EvidenceKind, ExtractionInput};
use research_engine::capabilities::planner::{PlannerCapability, PlannerInput};
use research_engine::capabilities::synthesis::{SynthesisCapability, SynthesisInput, SynthesisItem};
use research_engine::capabilities::verification::{
    Claim, VerificationCapability, VerificationInput, VerificationPolicy,
};
use research_engine::capabilities::CapabilityExec;
use research_engine::contract::{
    check_all_schema_documents, digest16, execute_request, schema_document, validate_against,
    AnchorKind, CapabilityId, CapabilityRequest, CapabilityResult, CapabilityStatus, Correlation,
    ErrorCategory, ProvenanceAnchor, SchemaKind, CAPABILITY_VERSION, PROTOCOL_VERSION,
};
use serde_json::{json, Value};

/// 七能力合法样本输入（与各 request-v1 的 input 形状一一对应）。
fn sample_inputs() -> [(&'static str, Value); 7] {
    [
        (
            "research.planner",
            json!({"goal": "检索 kingdee 相关知识并核验 example.com 响应特征", "max_steps": 6}),
        ),
        (
            "research.source-discovery",
            json!({
                "query": "kingdee OA 接口鉴权特征",
                "resource_hints": ["gh:owner/repo", "url:0123456789abcdef"],
                "limit": 5
            }),
        ),
        (
            "research.evidence-extraction",
            json!({
                "evidence_refs": ["ev-alpha-001", "ev-beta-002"],
                "artifact_refs": ["art-alpha-001"],
                "extract_as": "http_pair"
            }),
        ),
        (
            "research.verification",
            json!({
                "claims": [
                    {"claim_id": "claim-1", "evidence_ids": ["ev-1", "ev-2"]},
                    {"claim_id": "claim-2", "evidence_ids": ["ev-9"]}
                ],
                "policy": "advisory"
            }),
        ),
        (
            "research.synthesis",
            json!({
                "topic": "kingdee 接口鉴权特征综合",
                "items": [
                    {"evidence_id": "ev-1", "note": "200 响应携带管理面板链接"},
                    {"evidence_id": "ev-2", "artifact_id": "art-1"}
                ]
            }),
        ),
        (
            "research.citation",
            json!({
                "synthesis_id": "syn-0123456789abcdef",
                "anchors": [{"evidence_id": "ev-1"}, {"artifact_id": "art-1"}],
                "style": "footnote"
            }),
        ),
        (
            "research.evaluator",
            json!({
                "subject": {"kind": "synthesis", "id": "syn-0123456789abcdef"},
                "checks": ["completeness", "provenance_coverage", "consistency"]
            }),
        ),
    ]
}

fn sample_correlation() -> Correlation {
    Correlation::try_new(
        "rn-test-run-0001",
        "tr-test-trace-0001",
        Some("rtis-20261113-010203-001".to_string()),
    )
    .expect("sample correlation must be valid")
}

/// 装配完整请求信封（合法头 + 样本输入）。
fn sample_request(capability: CapabilityId, input: &Value) -> Value {
    json!({
        "protocol_version": PROTOCOL_VERSION,
        "capability_id": capability.as_str(),
        "capability_version": CAPABILITY_VERSION,
        "correlation": {
            "run_id": "rn-test-run-0001",
            "trace_id": "tr-test-trace-0001",
            "task_id": "rtis-20261113-010203-001"
        },
        "input": input,
    })
}

/// 词表枚举的字符串形式（serde 序列化面，与 Schema enum 同源）。
fn enum_str<T: serde::Serialize>(value: &T) -> String {
    serde_json::to_value(value)
        .expect("wordlist enum must serialize")
        .as_str()
        .expect("wordlist enum must serialize as string")
        .to_string()
}

// ---------------------------------------------------------------------------
// Schema 装载全量门（14 份）
// ---------------------------------------------------------------------------

#[test]
fn all_14_schema_documents_are_valid() {
    check_all_schema_documents().expect("all 14 embedded schemas must pass meta-schema gate");
    // 双保险：逐份可解析且 $id/const 与词表对齐。
    for capability in CapabilityId::ALL {
        for kind in [SchemaKind::Request, SchemaKind::Result] {
            let doc = schema_document(capability, kind).expect("schema must parse");
            let expected_id = format!(
                "research-platform/schemas/research-capability/{}.{}.json",
                match capability {
                    CapabilityId::Planner => "planner",
                    CapabilityId::SourceDiscovery => "discovery",
                    CapabilityId::EvidenceExtraction => "evidence-extraction",
                    CapabilityId::Verification => "verification",
                    CapabilityId::Synthesis => "synthesis",
                    CapabilityId::Citation => "citation",
                    CapabilityId::Evaluator => "evaluator",
                },
                match kind {
                    SchemaKind::Request => "request-v1",
                    SchemaKind::Result => "result-v1",
                }
            );
            assert_eq!(doc["$id"].as_str(), Some(expected_id.as_str()));
            assert_eq!(
                doc["properties"]["capability_id"]["const"].as_str(),
                Some(capability.as_str())
            );
        }
    }
}

// ---------------------------------------------------------------------------
// 请求 Schema 正反例（七能力各一）
// ---------------------------------------------------------------------------

#[test]
fn request_schemas_accept_samples_and_reject_malformed() {
    for (id, input) in sample_inputs() {
        let capability = CapabilityId::try_from(id).expect("capability id must parse");
        let request = sample_request(capability, &input);
        validate_against(capability, SchemaKind::Request, &request)
            .unwrap_or_else(|e| panic!("{id} sample request must pass request-v1: {e}"));
    }

    // 反例 1：缺 protocol_version。
    let request = json!({
        "capability_id": "research.planner",
        "capability_version": CAPABILITY_VERSION,
        "correlation": {"run_id": "rn-1", "trace_id": "tr-1"},
        "input": {"goal": "g"}
    });
    assert!(validate_against(CapabilityId::Planner, SchemaKind::Request, &request).is_err());

    // 反例 2：capability_id 与 const 不符（串能力）。
    let request = sample_request(CapabilityId::Planner, &json!({"goal": "g"}));
    let mut swapped = request.clone();
    swapped["capability_id"] = json!("research.synthesis");
    assert!(validate_against(CapabilityId::Planner, SchemaKind::Request, &swapped).is_err());

    // 反例 3：correlation 缺 trace_id。
    let request = json!({
        "protocol_version": "1",
        "capability_id": "research.planner",
        "capability_version": CAPABILITY_VERSION,
        "correlation": {"run_id": "rn-1"},
        "input": {"goal": "g"}
    });
    assert!(validate_against(CapabilityId::Planner, SchemaKind::Request, &request).is_err());

    // 反例 4：correlation.run_id 超 128 字符（RunId 冻结形状）。
    let request = json!({
        "protocol_version": "1",
        "capability_id": "research.planner",
        "capability_version": CAPABILITY_VERSION,
        "correlation": {"run_id": "x".repeat(129), "trace_id": "tr-1"},
        "input": {"goal": "g"}
    });
    assert!(validate_against(CapabilityId::Planner, SchemaKind::Request, &request).is_err());

    // 反例 5：input 未知字段（additionalProperties: false）。
    let request = json!({
        "protocol_version": "1",
        "capability_id": "research.planner",
        "capability_version": CAPABILITY_VERSION,
        "correlation": {"run_id": "rn-1", "trace_id": "tr-1"},
        "input": {"goal": "g", "unknown_field": 1}
    });
    assert!(validate_against(CapabilityId::Planner, SchemaKind::Request, &request).is_err());
}

// ---------------------------------------------------------------------------
// 契约往返：typed 信封 序列化 过 Schema 反序列化 再序列化 恒等
// ---------------------------------------------------------------------------

#[test]
fn result_envelope_roundtrips_for_all_seven() {
    for (id, input) in sample_inputs() {
        let capability = CapabilityId::try_from(id).expect("capability id must parse");
        let request = sample_request(capability, &input);
        let envelope = execute_request(capability, &request)
            .unwrap_or_else(|e| panic!("{id} must execute: {e}"));
        validate_against(capability, SchemaKind::Result, &envelope)
            .unwrap_or_else(|e| panic!("{id} envelope must pass result-v1: {e}"));

        // typed 往返：Value 反序列化为 typed 信封，再序列化必须逐字节恒等。
        let typed: CapabilityResult<Value> = serde_json::from_value(envelope.clone())
            .unwrap_or_else(|e| panic!("{id} envelope must deserialize: {e}"));
        let reserialized = serde_json::to_value(&typed).expect("re-serialize must succeed");
        assert_eq!(envelope, reserialized, "{id} envelope must roundtrip");
        assert_eq!(typed.status, CapabilityStatus::Succeeded, "{id}");
        assert!(typed.result.is_some(), "{id} SUCCEEDED carries result");
        assert!(
            typed.provenance.iter().any(|a| a.kind == AnchorKind::Run)
                && typed.provenance.iter().any(|a| a.kind == AnchorKind::Trace)
                && typed.provenance.iter().any(|a| a.kind == AnchorKind::Task),
            "{id} provenance must carry correlation anchors"
        );
    }
}

#[test]
fn typed_request_roundtrips_through_schema() {
    // typed CapabilityRequest<PlannerInput> 序列化必须过 request-v1 且反序列化恒等。
    let typed = CapabilityRequest {
        protocol_version: PROTOCOL_VERSION.to_string(),
        capability_id: CapabilityId::Planner,
        capability_version: CAPABILITY_VERSION.to_string(),
        correlation: sample_correlation(),
        input: PlannerInput {
            goal: "typed 往返样本".to_string(),
            max_steps: Some(3),
            workspace_id: Some("ws-default".to_string()),
        },
    };
    let value = serde_json::to_value(&typed).expect("typed request must serialize");
    validate_against(CapabilityId::Planner, SchemaKind::Request, &value)
        .expect("typed request must pass request-v1");
    let back: CapabilityRequest<PlannerInput> =
        serde_json::from_value(value).expect("typed request must deserialize");
    assert_eq!(typed, back);
}

// ---------------------------------------------------------------------------
// 确定性：同输入恒同输出
// ---------------------------------------------------------------------------

#[test]
fn all_capabilities_are_deterministic() {
    for (id, input) in sample_inputs() {
        let capability = CapabilityId::try_from(id).expect("capability id must parse");
        let request = sample_request(capability, &input);
        let first = execute_request(capability, &request).expect("first run");
        let second = execute_request(capability, &request).expect("second run");
        assert_eq!(first, second, "{id} must be deterministic");
    }
}

// ---------------------------------------------------------------------------
// mock 行为断言（逐能力）
// ---------------------------------------------------------------------------

#[test]
fn planner_builds_fixed_pipeline_and_truncates() {
    let correlation = sample_correlation();
    let (output, _, _) = PlannerCapability::run(
        &correlation,
        PlannerInput {
            goal: "固定流水线样本".to_string(),
            max_steps: None,
            workspace_id: None,
        },
    )
    .expect("valid input must succeed");
    assert_eq!(output.step_count, 6);
    assert_eq!(output.steps.len(), 6);
    assert_eq!(
        enum_str(&output.steps[0].capability),
        "research.source-discovery"
    );
    // step_id 全互异且确定性派生。
    let ids: Vec<&str> = output.steps.iter().map(|s| s.step_id.as_str()).collect();
    let mut unique = ids.clone();
    unique.sort_unstable();
    unique.dedup();
    assert_eq!(ids.len(), unique.len());
    assert_eq!(
        output.plan_id,
        format!("plan-{}", digest16(&["plan", "固定流水线样本"]))
    );

    let (truncated, _, _) = PlannerCapability::run(
        &correlation,
        PlannerInput {
            goal: "固定流水线样本".to_string(),
            max_steps: Some(3),
            workspace_id: None,
        },
    )
    .expect("valid input must succeed");
    assert_eq!(truncated.step_count, 3);
    assert_eq!(truncated.plan_id, output.plan_id, "plan_id 只随 goal 变化");

    // 空 goal：typed 层 VALIDATION（Schema 层 minLength 为第一道）。
    let failure = PlannerCapability::run(
        &correlation,
        PlannerInput {
            goal: "   ".to_string(),
            max_steps: None,
            workspace_id: None,
        },
    )
    .expect_err("blank goal must fail");
    assert_eq!(failure.category, ErrorCategory::Validation);
}

#[test]
fn discovery_puts_hints_first_and_limits() {
    let correlation = sample_correlation();
    let (output, _, anchors) = DiscoveryCapability::run(
        &correlation,
        DiscoveryInput {
            query: "hint 优先样本".to_string(),
            resource_hints: vec![
                "gh:owner/repo".to_string(),
                "url:0123456789abcdef".to_string(),
            ],
            limit: Some(2),
        },
    )
    .expect("valid input must succeed");
    assert_eq!(output.total, 2);
    assert_eq!(output.sources.len(), 2);
    assert_eq!(enum_str(&output.sources[0].kind), "resource_hint");
    assert_eq!(output.sources[0].resource_id, "gh:owner/repo");
    assert_eq!(anchors.len(), 2, "anchors 与 sources 一一对应");

    // 无 hint 缺省 limit：三族固定目录齐全。
    let (bare, _, _) = DiscoveryCapability::run(
        &correlation,
        DiscoveryInput {
            query: "bare".to_string(),
            resource_hints: Vec::new(),
            limit: None,
        },
    )
    .expect("valid input must succeed");
    assert_eq!(bare.total, 3);

    // 非法 hint（缺 gh:/url: 前缀）：域校验 VALIDATION 拒绝。
    let failure = DiscoveryCapability::run(
        &correlation,
        DiscoveryInput {
            query: "bad hint".to_string(),
            resource_hints: vec!["poc:xx".to_string()],
            limit: None,
        },
    )
    .expect_err("invalid hint must fail");
    assert_eq!(failure.category, ErrorCategory::Validation);
}

#[test]
fn extraction_hashes_deterministically_and_attaches_artifacts() {
    let correlation = sample_correlation();
    let (output, _, anchors) = ExtractionCapability::run(
        &correlation,
        ExtractionInput {
            evidence_refs: vec!["ev-alpha-001".to_string(), "ev-beta-002".to_string()],
            artifact_refs: vec!["art-alpha-001".to_string()],
            extract_as: EvidenceKind::HttpPair,
        },
    )
    .expect("valid input must succeed");
    assert_eq!(output.extracted_count, 2);
    assert_eq!(output.extractions[0].artifact_id.as_deref(), Some("art-alpha-001"));
    assert_eq!(output.extractions[1].artifact_id, None);
    for extraction in &output.extractions {
        assert_eq!(extraction.content_hash.len(), 64);
        assert!(
            extraction
                .content_hash
                .chars()
                .all(|c| c.is_ascii_hexdigit()),
            "content_hash must be 64 hex"
        );
    }
    // 锚：2 evidence + 1 artifact。
    assert_eq!(
        anchors
            .iter()
            .filter(|a| a.kind == AnchorKind::Evidence)
            .count(),
        2
    );
    assert_eq!(
        anchors
            .iter()
            .filter(|a| a.kind == AnchorKind::Artifact)
            .count(),
        1
    );

    // 非法证据 ID：VALIDATION 拒绝整请求。
    let failure = ExtractionCapability::run(
        &correlation,
        ExtractionInput {
            evidence_refs: vec!["".to_string()],
            artifact_refs: Vec::new(),
            extract_as: EvidenceKind::Log,
        },
    )
    .expect_err("empty evidence id must fail");
    assert_eq!(failure.category, ErrorCategory::Validation);
}

#[test]
fn verification_thresholds_follow_policy() {
    let correlation = sample_correlation();
    // advisory：1 条合法证据即 VERIFIED。
    let (advisory, _, _) = VerificationCapability::run(
        &correlation,
        VerificationInput {
            claims: vec![Claim {
                claim_id: "c1".to_string(),
                evidence_ids: vec!["ev-9".to_string()],
            }],
            policy: Some(VerificationPolicy::Advisory),
        },
    )
    .expect("valid input must succeed");
    assert_eq!(advisory.verified_count, 1);
    assert_eq!(enum_str(&advisory.verdicts[0].verdict), "VERIFIED");

    // strict：同输入降为 UNVERIFIED。
    let (strict, _, _) = VerificationCapability::run(
        &correlation,
        VerificationInput {
            claims: vec![Claim {
                claim_id: "c1".to_string(),
                evidence_ids: vec!["ev-9".to_string()],
            }],
            policy: Some(VerificationPolicy::Strict),
        },
    )
    .expect("valid input must succeed");
    assert_eq!(strict.unverified_count, 1);

    // 重复引用去重后只剩 1 条互异证据，strict 仍 UNVERIFIED；非法 ID（空串，
    // EvidenceId 域校验拒绝的唯一形状之一）只进 notes，不进 checked 列。
    let (mixed, _, anchors) = VerificationCapability::run(
        &correlation,
        VerificationInput {
            claims: vec![
                Claim {
                    claim_id: "dup".to_string(),
                    evidence_ids: vec!["ev-1".to_string(), "ev-1".to_string()],
                },
                Claim {
                    claim_id: "invalid".to_string(),
                    evidence_ids: vec![String::new()],
                },
            ],
            policy: Some(VerificationPolicy::Strict),
        },
    )
    .expect("valid input must succeed");
    assert_eq!(mixed.verdicts[0].checked_evidence_ids.len(), 1);
    assert_eq!(enum_str(&mixed.verdicts[0].verdict), "UNVERIFIED");
    assert!(mixed.verdicts[1].checked_evidence_ids.is_empty());
    assert!(mixed.verdicts[1].notes.contains("非法 EvidenceId"));
    assert_eq!(
        anchors
            .iter()
            .filter(|a| a.kind == AnchorKind::Evidence)
            .count(),
        0,
        "strict 下两条均 UNVERIFIED，无证据锚（锚只随 VERIFIED 判定）"
    );
}

#[test]
fn synthesis_expands_items_in_order() {
    let correlation = sample_correlation();
    let (output, _, anchors) = SynthesisCapability::run(
        &correlation,
        SynthesisInput {
            topic: "顺序样本".to_string(),
            items: vec![
                SynthesisItem {
                    evidence_id: "ev-1".to_string(),
                    artifact_id: None,
                    note: Some("素材 A".to_string()),
                },
                SynthesisItem {
                    evidence_id: "ev-2".to_string(),
                    artifact_id: Some("art-2".to_string()),
                    note: None,
                },
            ],
        },
    )
    .expect("valid input must succeed");
    assert_eq!(output.sections.len(), 2);
    assert_eq!(output.sections[0].body, "素材 A", "note 原样进 body");
    assert!(
        output.sections[1].body.contains("确定性综合占位"),
        "无 note 落固定占位文案"
    );
    assert_eq!(output.sections[1].anchored_evidence_ids, vec!["ev-2"]);
    assert_eq!(output.synthesis_id, format!("syn-{}", digest16(&["syn", "顺序样本"])));
    assert_eq!(
        anchors
            .iter()
            .filter(|a| a.kind == AnchorKind::Artifact)
            .count(),
        1
    );

    // 非法 evidence_id：VALIDATION 拒绝（综合必须可锚定）。
    let failure = SynthesisCapability::run(
        &correlation,
        SynthesisInput {
            topic: "t".to_string(),
            items: vec![SynthesisItem {
                evidence_id: String::new(),
                artifact_id: None,
                note: None,
            }],
        },
    )
    .expect_err("empty evidence id must fail");
    assert_eq!(failure.category, ErrorCategory::Validation);
}

#[test]
fn citation_markers_follow_style_and_order() {
    let correlation = sample_correlation();
    let (footnote, _, _) = CitationCapability::run(
        &correlation,
        CitationInput {
            synthesis_id: "syn-0123456789abcdef".to_string(),
            anchors: vec![
                research_engine::capabilities::citation::CitationAnchorRef {
                    evidence_id: Some("ev-1".to_string()),
                    artifact_id: None,
                },
                research_engine::capabilities::citation::CitationAnchorRef {
                    evidence_id: None,
                    artifact_id: Some("art-1".to_string()),
                },
            ],
            style: Some(CitationStyle::Footnote),
        },
    )
    .expect("valid input must succeed");
    assert_eq!(footnote.citations[0].marker, "[1]");
    assert_eq!(footnote.citations[1].marker, "[2]");
    assert_eq!(footnote.citations[1].artifact_id.as_deref(), Some("art-1"));
    assert_eq!(footnote.synthesis_ref, "syn-0123456789abcdef");

    let (inline, _, _) = CitationCapability::run(
        &correlation,
        CitationInput {
            synthesis_id: "syn-0123456789abcdef".to_string(),
            anchors: vec![research_engine::capabilities::citation::CitationAnchorRef {
                evidence_id: Some("ev-1".to_string()),
                artifact_id: None,
            }],
            style: None,
        },
    )
    .expect("valid input must succeed");
    assert_eq!(inline.style.as_str(), "inline", "缺省 inline");
    assert_eq!(inline.citations[0].marker, "(1)");

    // 双字段全缺的锚：VALIDATION 拒绝。
    let failure = CitationCapability::run(
        &correlation,
        CitationInput {
            synthesis_id: "syn-x".to_string(),
            anchors: vec![research_engine::capabilities::citation::CitationAnchorRef {
                evidence_id: None,
                artifact_id: None,
            }],
            style: None,
        },
    )
    .expect_err("empty anchor must fail");
    assert_eq!(failure.category, ErrorCategory::Validation);
}

#[test]
fn evaluator_scores_shape_heuristics() {
    let correlation = sample_correlation();
    // 规范前缀：三项全 100，verdict PASS。
    let (canonical, _, _) = EvaluatorCapability::run(
        &correlation,
        EvaluatorInput {
            subject: SubjectRef {
                kind: SubjectKind::Synthesis,
                id: "syn-0123456789abcdef".to_string(),
            },
            checks: vec![
                EvaluatorCheck::Completeness,
                EvaluatorCheck::ProvenanceCoverage,
                EvaluatorCheck::Consistency,
            ],
        },
    )
    .expect("valid input must succeed");
    assert_eq!(canonical.overall_score, 100);
    assert_eq!(enum_str(&canonical.verdict), "PASS");
    assert_eq!(canonical.scores.len(), 3);
    assert!(canonical.scores.iter().all(|s| s.score == 100 && s.passed));

    // 前缀不符：provenance_coverage 40，overall (100+40+100)/3=80，仍 PASS。
    let (mismatched, _, _) = EvaluatorCapability::run(
        &correlation,
        EvaluatorInput {
            subject: SubjectRef {
                kind: SubjectKind::Citation,
                id: "not-canonical-prefix".to_string(),
            },
            checks: vec![
                EvaluatorCheck::Completeness,
                EvaluatorCheck::ProvenanceCoverage,
                EvaluatorCheck::Consistency,
            ],
        },
    )
    .expect("valid input must succeed");
    assert_eq!(mismatched.scores[1].score, 40);
    assert_eq!(mismatched.overall_score, 80);

    // 仅 provenance_coverage 且前缀不符：overall 40，verdict REVIEW。
    let (failing, _, _) = EvaluatorCapability::run(
        &correlation,
        EvaluatorInput {
            subject: SubjectRef {
                kind: SubjectKind::Plan,
                id: "odd-id".to_string(),
            },
            checks: vec![EvaluatorCheck::ProvenanceCoverage],
        },
    )
    .expect("valid input must succeed");
    assert_eq!(failing.overall_score, 40);
    assert_eq!(enum_str(&failing.verdict), "REVIEW");

    // 重复检查项：typed 层 VALIDATION 拒绝。
    let failure = EvaluatorCapability::run(
        &correlation,
        EvaluatorInput {
            subject: SubjectRef {
                kind: SubjectKind::Plan,
                id: "plan-x".to_string(),
            },
            checks: vec![EvaluatorCheck::Completeness, EvaluatorCheck::Completeness],
        },
    )
    .expect_err("duplicate check must fail");
    assert_eq!(failure.category, ErrorCategory::Validation);
}

// ---------------------------------------------------------------------------
// FAILED 信封也必须过 result-v1（契约自洽）
// ---------------------------------------------------------------------------

#[test]
fn invalid_input_yields_schema_valid_failed_envelope() {
    // 每能力各构造一个 type 违规的 input（绕过 request-v1 的形状约束是
    // 不可能的：这里直接走 execute_request 的 typed 层，模拟库内直调）。
    let cases: [(CapabilityId, Value); 7] = [
        (CapabilityId::Planner, json!({})),                    // 缺 goal
        (CapabilityId::SourceDiscovery, json!({"query": 42})), // query 类型错
        (CapabilityId::EvidenceExtraction, json!({"evidence_refs": [], "extract_as": "log"})), // 空 refs
        (CapabilityId::Verification, json!({"claims": []})),   // 空 claims
        (CapabilityId::Synthesis, json!({"topic": "t", "items": []})), // 空 items
        (CapabilityId::Citation, json!({"synthesis_id": "s", "anchors": []})), // 空 anchors
        (CapabilityId::Evaluator, json!({"subject": {"kind": "plan", "id": "p"}, "checks": []})), // 空 checks
    ];
    for (capability, input) in cases {
        let request = sample_request(capability, &input);
        let envelope = execute_request(capability, &request)
            .unwrap_or_else(|e| panic!("{capability} must produce envelope, got: {e}"));
        validate_against(capability, SchemaKind::Result, &envelope).unwrap_or_else(|e| {
            panic!("{capability} FAILED envelope must still pass result-v1: {e}")
        });
        assert_eq!(envelope["status"].as_str(), Some("FAILED"));
        assert_eq!(
            envelope["error"]["category"].as_str(),
            Some("VALIDATION"),
            "{capability} typed 层拒收只产 VALIDATION"
        );
        assert_eq!(envelope["error"]["retryable"].as_bool(), Some(false));
    }
}

#[test]
fn invalid_correlation_is_rejected_without_envelope() {
    let request = sample_request(CapabilityId::Planner, &json!({"goal": "g"}));
    let mut bad = request.clone();
    bad["correlation"]["run_id"] = json!("x".repeat(129));
    let error = execute_request(CapabilityId::Planner, &bad)
        .expect_err("129-char run_id must be rejected at envelope level");
    assert!(
        error.to_string().contains("invalid correlation"),
        "unexpected error: {error}"
    );

    let mut bad_version = request.clone();
    bad_version["protocol_version"] = json!("2");
    let error = execute_request(CapabilityId::Planner, &bad_version)
        .expect_err("protocol_version 2 must be rejected");
    assert!(error.to_string().contains("protocol_version"));
}

// ---------------------------------------------------------------------------
// 六 ID 锚域类型接线
// ---------------------------------------------------------------------------

#[test]
fn provenance_anchors_are_built_from_domain_ids() {
    use domain::ids::{ArtifactId, EvidenceId, ResourceId, RunId, TaskId, TraceId};
    let run = RunId::try_new("rn-anchor-1").unwrap();
    let trace = TraceId::try_new("tr-anchor-1").unwrap();
    let task = TaskId::try_new("rtis-20261113-010203-001").unwrap();
    let artifact = ArtifactId::try_new("art-anchor-1").unwrap();
    let evidence = EvidenceId::try_new("ev-anchor-1").unwrap();
    let resource = ResourceId::try_new("gh:owner/repo").unwrap();
    let anchors = [
        ProvenanceAnchor::run(&run),
        ProvenanceAnchor::trace(&trace),
        ProvenanceAnchor::task(&task),
        ProvenanceAnchor::artifact(&artifact),
        ProvenanceAnchor::evidence(&evidence),
        ProvenanceAnchor::resource(&resource),
    ];
    let kinds: [(&str, &str); 6] = [
        ("run", "rn-anchor-1"),
        ("trace", "tr-anchor-1"),
        ("task", "rtis-20261113-010203-001"),
        ("artifact", "art-anchor-1"),
        ("evidence", "ev-anchor-1"),
        ("resource", "gh:owner/repo"),
    ];
    for (anchor, (kind, id)) in anchors.iter().zip(kinds) {
        assert_eq!(enum_str(&anchor.kind), kind);
        assert_eq!(anchor.id, id);
    }

    // correlation 锚派生：run/trace 恒在，task 随缺省。
    let full = sample_correlation().anchors();
    assert_eq!(full.len(), 3);
    let bare = Correlation::try_new("rn-b", "tr-b", None)
        .expect("bare correlation must be valid")
        .anchors();
    assert_eq!(bare.len(), 2);

    // 空串 task_id 视为缺省（不产空锚）。
    let empty_task = Correlation::try_new("rn-c", "tr-c", Some(String::new()))
        .expect("empty task id must be treated as absent");
    assert!(empty_task.task_id.is_none());
}

// ---------------------------------------------------------------------------
// 词表完整性
// ---------------------------------------------------------------------------

#[test]
fn capability_wordlist_is_complete() {
    let expected = [
        "research.planner",
        "research.source-discovery",
        "research.evidence-extraction",
        "research.verification",
        "research.synthesis",
        "research.citation",
        "research.evaluator",
    ];
    assert_eq!(CapabilityId::ALL.len(), 7);
    for (capability, id) in CapabilityId::ALL.iter().zip(expected) {
        assert_eq!(capability.as_str(), id);
        assert!(CapabilityId::try_from(id).is_ok());
    }
    assert!(CapabilityId::try_from("research.unknown").is_err());
    // tool-contract tool_id 同口径：段内字符集（小写字母/数字/下划线/连字符加点分）。
    for id in expected {
        assert!(
            id.split('.').all(|segment| {
                !segment.is_empty()
                    && segment
                        .chars()
                        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
            }),
            "{id} must match tool_id charset convention"
        );
    }
}
