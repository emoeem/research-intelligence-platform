//! P7-1 纯类型测试载具：AgentDefinition / AgentRun / CapabilityRef /
//! AgentRunState（PHASE-ROADMAP §8.3 P7-1）。domain 零依赖纯类型库，
//! 本文件只测类型行为，不触数据库。绑定引用校验判据（指向不存在工具
//! 即拒绝）经谓词注入在纯函数面断言。

use domain::{
    AgentDefinition, AgentId, AgentRun, AgentRunState, CapabilityRef, DomainError, RunId,
};
use std::collections::HashSet;

fn def(agent: &str, refs: Vec<CapabilityRef>) -> AgentDefinition {
    AgentDefinition::try_new(
        AgentId::try_new(agent).unwrap(),
        "0.1.0".to_string(),
        "{}".to_string(),
        refs,
    )
    .unwrap()
}

#[test]
fn agent_id_accepts_and_rejects() {
    assert!(AgentId::try_new("agent.researcher").is_ok());
    assert!(AgentId::try_new("").is_err());
    let oversized = "a".repeat(AgentId::MAX_LEN + 1);
    let err = AgentId::try_new(oversized).unwrap_err();
    assert!(matches!(err, DomainError::InvalidAgentId { .. }));
}

#[test]
fn capability_ref_shape_matches_frozen_tool_id_contract() {
    assert!(CapabilityRef::try_new("rip.doc.parse", None).is_ok());
    // P1-7b 修订后的段内下划线（B5 工具名）。
    assert!(CapabilityRef::try_new("rip.ops.etl_status", Some("0.1.0".into())).is_ok());
    assert!(CapabilityRef::try_new("Rip.doc", None).is_err());
    assert!(CapabilityRef::try_new("nodot", None).is_err());
    assert!(CapabilityRef::try_new("rip.doc", Some("1.x.0".into())).is_err());
}

#[test]
fn capability_binding_rejects_unknown_tool() {
    // P7-1 判据原句：绑定引用校验（指向不存在工具即拒绝）。
    let registry: HashSet<&str> = ["rip.doc.parse", "rip.kb.rtis", "rip.kb.eri"].into();
    let exists = |t: &str| registry.contains(t);

    let ok = def(
        "agent.ok",
        vec![
            CapabilityRef::try_new("rip.doc.parse", Some("0.1.0".into())).unwrap(),
            CapabilityRef::try_new("rip.kb.eri", None).unwrap(),
        ],
    );
    assert!(ok.validate_capability_refs(&exists).is_ok());

    let bad = def(
        "agent.bad",
        vec![CapabilityRef::try_new("rip.ghost.tool", None).unwrap()],
    );
    let err = bad.validate_capability_refs(&exists).unwrap_err();
    assert!(
        matches!(err, DomainError::UnknownCapabilityTarget { ref tool_id }
                    if tool_id == "rip.ghost.tool"),
        "err: {err:?}"
    );
    assert!(err.to_string().contains("rip.ghost.tool"));
}

#[test]
fn definition_rejects_bad_version_and_policy() {
    let id = AgentId::try_new("agent.x").unwrap();
    assert!(AgentDefinition::try_new(id.clone(), "1.0".into(), "{}".into(), vec![]).is_err());
    assert!(AgentDefinition::try_new(id.clone(), "1.0.0".into(), " ".into(), vec![]).is_err());
    let d =
        AgentDefinition::try_new(id.clone(), "1.0.0".into(), "{\"k\":1}".into(), vec![]).unwrap();
    assert_eq!(d.version(), "1.0.0");
    assert!(d.capability_refs().is_empty());
}

#[test]
fn agent_run_state_vocabulary_is_six_with_three_terminal() {
    let stored: Vec<&str> = AgentRunState::ALL.iter().map(|s| s.as_str()).collect();
    assert_eq!(
        stored,
        vec![
            "QUEUED",
            "PAUSED",
            "RUNNING",
            "SUCCEEDED",
            "FAILED",
            "CANCELLED"
        ]
    );
    for s in [
        AgentRunState::Succeeded,
        AgentRunState::Failed,
        AgentRunState::Cancelled,
    ] {
        assert!(s.is_terminal());
    }
    for s in [
        AgentRunState::Queued,
        AgentRunState::Paused,
        AgentRunState::Running,
    ] {
        assert!(!s.is_terminal());
    }
}

#[test]
fn agent_run_builds_and_validates_fields() {
    let run = AgentRun::try_new(
        RunId::try_new("rn-agent-e2e").unwrap(),
        Some("t-7".into()),
        AgentId::try_new("agent.researcher").unwrap(),
        "0.1.0".into(),
        AgentRunState::Paused,
        vec!["mem://task-memory/1".into()],
    )
    .unwrap();
    assert_eq!(run.run_id().as_str(), "rn-agent-e2e");
    assert_eq!(run.state(), AgentRunState::Paused);
    assert_eq!(run.memory_refs().len(), 1);

    // 版本非法 / task_id 空串 / memory_refs 空条目均拒绝。
    assert!(AgentRun::try_new(
        RunId::try_new("rn-a").unwrap(),
        None,
        AgentId::try_new("agent.a").unwrap(),
        "0".into(),
        AgentRunState::Queued,
        vec![]
    )
    .is_err());
    assert!(AgentRun::try_new(
        RunId::try_new("rn-a").unwrap(),
        Some(String::new()),
        AgentId::try_new("agent.a").unwrap(),
        "0.1.0".into(),
        AgentRunState::Queued,
        vec![]
    )
    .is_err());
    assert!(AgentRun::try_new(
        RunId::try_new("rn-a").unwrap(),
        None,
        AgentId::try_new("agent.a").unwrap(),
        "0.1.0".into(),
        AgentRunState::Queued,
        vec![String::new()]
    )
    .is_err());
}

#[test]
fn crate_root_reexports_reachable() {
    let _id: AgentId = AgentId::try_new("agent.reexport").unwrap();
    let _r: AgentRunState = AgentRunState::Queued;
    let _c: CapabilityRef = CapabilityRef::try_new("rip.doc.parse", None).unwrap();
}
