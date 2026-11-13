//! Parallel 节点执行器（P8-2）：branch 边扇出并发执行 + join 收敛。
//!
//! 口径（计划 §14:829 Parallel 节点 + ROADMAP §9.3 P8-2「并发分支 join
//! 收敛」）：parallel 节点是 fork-join 协调器，不是普通计算节点：
//! 1. 进入 running（attempt +1）；
//! 2. 全部 branch 出边目标（fan 子节点，受理门限定 tool/knowledge/condition
//!    且入度恰 1）各自派发到独立线程并发执行（每子节点独立 attempt 独立
//!    node row 独立 checkpoint，子节点超时由自身看门狗收敛）；
//! 3. join：逐个收齐子节点终态（收齐顺序与完成顺序解耦，全部到终态才算
//!    join 完成）；全部 succeeded → parallel 节点 succeeded（输出 = 子输出
//!    按子节点 id 升序浅合并的 object）；任一 failed → parallel 节点
//!    failed（code=parallel_child_failed，载荷列出失败子节点），run 随
//!    fail-fast 终止；
//! 4. 子节点输出由主线程合并进 state（确定性顺序：子节点 id 升序），
//!    parallel 节点自身输出为同一合并结果（幂等，重复合并无害）。
//!
//! 并发纪律（AGENTS §3 低并发先例的边界说明）：fan 子节点并发度 =
//! branch 出边数（画布声明面，非引擎自动放大）；每子节点独立 SQLite 行，
//! 写面经 Pool 串行化（rusqlite 连接池互斥），无共享可变状态。

use super::{NodeFailure, WorkflowExecutor};
use crate::error::WorkflowError;
use crate::state::{merge_output, type_word};
use domain::{WorkflowEdgeKind, WorkflowNodeRunState, WorkflowRunState};
use serde_json::{Map, Value};

/// Parallel fork-join（由 exec::WorkflowExecutor::execute_node 调度）。
pub fn execute_fan_out(
    executor: &WorkflowExecutor,
    run_id: &str,
    node: &domain::WorkflowNode,
    state: Value,
) -> Result<Value, WorkflowError> {
    let def = executor.pinned_definition(run_id)?;
    let fans: Vec<&domain::WorkflowEdge> = def
        .edges()
        .iter()
        .filter(|e| e.from() == node.id() && e.kind() == WorkflowEdgeKind::Branch)
        .collect();
    // 受理门已保证 fans 非空；防御面再断言一次。
    if fans.is_empty() {
        return Err(WorkflowError::InvalidInput(format!(
            "parallel node {:?} has no fan-out edges",
            node.id()
        )));
    }
    executor.runs.node_start(run_id, node.id())?;
    // fork：每子节点独立派发线程（独立 attempt / node row / checkpoint）。
    let mut receivers = Vec::with_capacity(fans.len());
    for fan in &fans {
        let Some(child) = def.nodes().iter().find(|n| n.id() == fan.to()) else {
            return Err(WorkflowError::InvalidInput(format!(
                "parallel fan edge {}->{} targets missing node",
                fan.from(),
                fan.to()
            )));
        };
        let (rx, _attempt) = executor.dispatch_node(run_id, child, &state)?;
        receivers.push((child.id().to_string(), rx));
    }
    // join：收齐全部子节点终态（每子节点独立看门狗超时）。
    let mut merged: Map<String, Value> = match state {
        Value::Object(map) => map,
        other => {
            return Err(WorkflowError::InvalidInput(format!(
                "run state must be an object, got {}",
                type_word(&other)
            )))
        }
    };
    let mut failures: Vec<String> = Vec::new();
    let mut child_outputs: Vec<(String, Value)> = Vec::new();
    for (child_id, rx) in receivers {
        // fan 子节点限定 tool/knowledge/condition（受理门），无 Paused/
        // AwaitingApproval 信号面；防御面按 failed 收口（与 fail-fast 一致）。
        match executor.await_dispatch(&child_id, rx)? {
            super::AwaitedOutcome::Succeeded(output) => {
                let output = output.unwrap_or(Value::Null);
                executor.runs.node_finish(
                    run_id,
                    &child_id,
                    WorkflowNodeRunState::Succeeded,
                    Some(&output),
                    None,
                )?;
                executor.runs.record_checkpoint(run_id, &child_id)?;
                if !output.is_null() {
                    child_outputs.push((child_id, output));
                }
            }
            other => {
                let failure = match other {
                    super::AwaitedOutcome::Failed(failure) => failure,
                    _ => NodeFailure::new(
                        "parallel_child_failed",
                        "child returned pause/awaiting signal (not allowed for fan children)",
                    ),
                };
                failures.push(format!("{}: {} ({})", child_id, failure.code, failure.message));
                executor
                    .runs
                    .node_finish(
                        run_id,
                        &child_id,
                        WorkflowNodeRunState::Failed,
                        None,
                        Some(&failure.to_json(run_id, &child_id)),
                    )?;
                executor.runs.record_checkpoint(run_id, &child_id)?;
            }
        }
    }
    if !failures.is_empty() {
        let failure = NodeFailure::new(
            "parallel_child_failed",
            format!("parallel {:?} join failed: {}", node.id(), failures.join("; ")),
        );
        let error = failure.to_json(run_id, node.id());
        executor
            .runs
            .node_finish(run_id, node.id(), WorkflowNodeRunState::Failed, None, Some(&error))?;
        executor.runs.record_checkpoint(run_id, node.id())?;
        executor.finish_run(run_id, WorkflowRunState::Failed, None, Some(&error))?;
        return Err(WorkflowError::Exec(error.to_string()));
    }
    // join 收敛：子输出按子节点 id 升序浅合并（确定性）。
    child_outputs.sort_by(|a, b| a.0.cmp(&b.0));
    for (_, output) in &child_outputs {
        merge_output(&mut merged, output, node.id())?;
    }
    let parallel_output = child_outputs
        .iter()
        .fold(Map::new(), |mut acc, (_, output)| {
            if let Value::Object(map) = output {
                for (k, v) in map {
                    acc.insert(k.clone(), v.clone());
                }
            }
            acc
        });
    executor.runs.node_finish(
        run_id,
        node.id(),
        WorkflowNodeRunState::Succeeded,
        Some(&Value::Object(parallel_output)),
        None,
    )?;
    executor.runs.record_checkpoint(run_id, node.id())?;
    Ok(Value::Object(merged))
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::WorkflowNodeKind;
    use serde_json::json;

    #[test]
    fn merge_of_child_outputs_is_idempotent_object() {
        let mut merged = Map::new();
        let a = json!({"x": 1});
        merge_output(&mut merged, &a, "p").expect("merge a");
        // 重复合并同形状（parallel 自身输出路径）幂等。
        merge_output(&mut merged, &a, "p").expect("merge a again");
        assert_eq!(Value::Object(merged), json!({"x": 1}));
    }

    #[test]
    fn non_object_state_is_rejected_by_type_word() {
        let bad = json!([1, 2]);
        assert_eq!(type_word(&bad), "array");
    }

    #[test]
    fn node_kind_wordtable_boundary() {
        assert_eq!(WorkflowNodeKind::Parallel.as_str(), "parallel");
        assert_eq!(WorkflowNodeKind::Condition.as_str(), "condition");
    }
}
