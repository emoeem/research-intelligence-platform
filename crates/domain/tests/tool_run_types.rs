//! P1-2 纯类型测试载具：ToolRunStatus 八态 + RunId / ArtifactId 新类型。
//!
//! 口径（TOOL-CONTRACT §3.1 / §4.1 / §5.2，PHASE-ROADMAP §2.3 P1-2）；
//! domain 零依赖纯类型库，本文件只测类型行为，不触数据库。

use domain::{ArtifactId, RunId, ToolRunStatus};
use std::collections::HashSet;
use std::str::FromStr;

#[test]
fn tool_run_status_covers_exactly_eight_states() {
    assert_eq!(ToolRunStatus::ALL.len(), 8);
    let stored: Vec<&str> = ToolRunStatus::ALL.iter().map(|s| s.as_str()).collect();
    assert_eq!(
        stored,
        vec![
            "QUEUED",
            "WAITING_FOR_RUNNER",
            "PREPARING",
            "RUNNING",
            "SUCCEEDED",
            "FAILED",
            "TIMED_OUT",
            "CANCELLED",
        ]
    );
}

#[test]
fn tool_run_status_parse_roundtrip_and_rejects_unknown() {
    for s in ToolRunStatus::ALL {
        assert_eq!(ToolRunStatus::parse(s.as_str()).unwrap(), s);
        assert_eq!(ToolRunStatus::from_str(s.as_str()).unwrap(), s);
        let lower = s.as_str().to_ascii_lowercase();
        assert_eq!(ToolRunStatus::parse(&lower).unwrap(), s);
    }
    assert!(ToolRunStatus::parse("FINISHED").is_err());
    assert!(ToolRunStatus::parse("").is_err());
    let err = ToolRunStatus::parse("PAUSED").unwrap_err().to_string();
    assert!(err.contains("PAUSED"), "error should echo input: {err}");
}

#[test]
fn tool_run_status_terminal_flags_match_contract_3_2() {
    for s in ToolRunStatus::ALL {
        let expected = matches!(
            s,
            ToolRunStatus::Succeeded
                | ToolRunStatus::Failed
                | ToolRunStatus::TimedOut
                | ToolRunStatus::Cancelled
        );
        assert_eq!(s.is_terminal(), expected, "state {s} terminal flag");
    }
}

#[test]
fn tool_run_status_is_hashable_distinct_eight() {
    let set: HashSet<&str> = ToolRunStatus::ALL.iter().map(|s| s.as_str()).collect();
    assert_eq!(set.len(), 8);
    assert_eq!(ToolRunStatus::Running.to_string(), "RUNNING");
}

#[test]
fn run_id_accepts_contract_example_shape() {
    // 样例取 TOOL-CONTRACT §8.3 调用示例的 run_id。
    let id = RunId::try_new("rn-01HTXYZEXAMPLE").unwrap();
    assert_eq!(id.as_str(), "rn-01HTXYZEXAMPLE");
    assert_eq!(id.to_string(), "rn-01HTXYZEXAMPLE");
    assert_eq!(id.as_ref(), "rn-01HTXYZEXAMPLE");
    assert_eq!(RunId::try_from("rn-01HTXYZEXAMPLE").unwrap(), id);
    assert_eq!(
        RunId::try_from(String::from("rn-01HTXYZEXAMPLE")).unwrap(),
        id
    );
    assert_eq!(id.clone().into_inner(), "rn-01HTXYZEXAMPLE");
}

#[test]
fn run_id_rejects_empty_and_over_128_chars() {
    assert!(RunId::try_new("").is_err());
    let oversized = "r".repeat(RunId::MAX_LEN + 1);
    let err = RunId::try_new(oversized).unwrap_err();
    assert!(err.to_string().contains("128"), "error: {err}");
    // 边界内合法。
    assert!(RunId::try_new("r".repeat(RunId::MAX_LEN)).is_ok());
}

#[test]
fn artifact_id_accepts_nonempty_and_rejects_empty() {
    let id = ArtifactId::try_new("art-01HTXYZEXAMPLE").unwrap();
    assert_eq!(id.as_str(), "art-01HTXYZEXAMPLE");
    assert_eq!(ArtifactId::try_from("art-01HTXYZEXAMPLE").unwrap(), id);
    assert_eq!(
        ArtifactId::try_from(String::from("art-01HTXYZEXAMPLE")).unwrap(),
        id
    );
    assert_eq!(id.clone().into_inner(), "art-01HTXYZEXAMPLE");
    assert!(ArtifactId::try_new("").is_err());
    let err = ArtifactId::try_new("").unwrap_err().to_string();
    assert!(err.contains("empty artifact id"), "error: {err}");
}

#[test]
fn ids_equality_and_hash_semantics() {
    let a = RunId::try_new("rn-same").unwrap();
    let b = RunId::try_new("rn-same").unwrap();
    let c = RunId::try_new("rn-other").unwrap();
    assert_eq!(a, b);
    assert_ne!(a, c);
    let mut set = HashSet::new();
    set.insert(a.clone());
    set.insert(b);
    assert_eq!(set.len(), 1, "equal run ids hash to one entry");

    let x = ArtifactId::try_new("art-same").unwrap();
    let y = ArtifactId::try_new("art-same").unwrap();
    let mut artifacts = HashSet::new();
    artifacts.insert(x);
    artifacts.insert(y);
    assert_eq!(artifacts.len(), 1);
}

#[test]
fn re_exports_reachable_from_crate_root() {
    // lib.rs re-export 面：ToolRunStatus / RunId / ArtifactId 可从 crate 根直达。
    let _status: ToolRunStatus = ToolRunStatus::Queued;
    let _run: RunId = RunId::try_new("rn-reexport").unwrap();
    let _artifact: ArtifactId = ArtifactId::try_new("art-reexport").unwrap();
}
