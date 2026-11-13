//! Condition 节点执行器（P8-2）：纯函数谓词，分支边运行期判定。
//!
//! 口径（计划 §9:476「Workflow 负责确定性编排」+ P8-1 冻结基线
//! 「edge.when 条件载荷形状 P8-2 冻结」）：params.expression 为一份
//! JSON Schema 文档（draft 2020-12 子集，复用 jsonschema 0.56 既有依赖，
//! 零新增 package），谓词 = 对合并后的 run state 求值：validate 通过 =
//! true，否则 false。零运行期解释器、零随机性、零 IO（同 state 恒同
//! 结果），表达力由 JSON Schema 承载（const/enum/required/类型与数值
//! 约束等），坏谓词受理期即拒（accept_definition 编译门，fail-closed）。
//!
//! expression 内的 `$/` 状态引用先解析再编译（如 `{"const": "$/target"}`
//! 可引用 run 输入或上游节点输出），引用不可解析 fail-closed。
//!
//! 分支选择（exec::edge_view）：condition 节点输出
//! `{"condition_result": <bool>}` 并入 state；branch 出边携带 when 布尔
//! 字面量（受理门强制），`condition_result == edge.when` 的边选中，其余
//! 边 Dead（下游节点 skip 级联，节点状态 skipped 终态）。
//!
//! 输出即 `{"condition_result": bool}`（object 可并形）。

use super::NodeFailure;
use crate::exec::resolve_refs;
use serde_json::{Map, Value};

/// Condition 节点执行（纯函数；无 IO 无线程）。
pub fn execute(params_json: Option<&str>, state: &Value) -> Result<Value, NodeFailure> {
    let params: Map<String, Value> = match params_json {
        Some(raw) => serde_json::from_str(raw)
            .map_err(|e| NodeFailure::new("params_invalid", format!("condition node params parse failed: {e}")))?,
        None => Map::new(),
    };
    let expression = params.get("expression").ok_or_else(|| {
        NodeFailure::new("params_invalid", "condition node params missing expression")
    })?;
    // $/ 状态引用先解析再编译（谓词可引用 run 输入与上游节点输出）。
    let expression = resolve_refs(expression, state)?;
    let validator = jsonschema::validator_for(&expression).map_err(|e| {
        NodeFailure::new(
            "condition_invalid_expression",
            format!("condition expression is not a valid JSON Schema: {e}"),
        )
    })?;
    let result = validator.is_valid(state);
    Ok(serde_json::json!({ "condition_result": result }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::exec::STATE_REF_PREFIX;
    use serde_json::json;

    #[test]
    fn predicate_true_and_false_paths() {
        let expr = json!({
            "type": "object",
            "required": ["target"],
            "properties": {"target": {"const": "example.com"}}
        });
        let params = json!({"expression": expr}).to_string();
        let out = execute(Some(&params), &json!({"target": "example.com"})).expect("true path");
        assert_eq!(out, json!({"condition_result": true}));
        let out = execute(Some(&params), &json!({"target": "other"})).expect("false path");
        assert_eq!(out, json!({"condition_result": false}));
    }

    #[test]
    fn numeric_and_enum_constraints_work() {
        let params = json!({"expression": {"properties": {"hits": {"minimum": 1}}}}).to_string();
        let out = execute(Some(&params), &json!({"hits": 3})).expect("min ok");
        assert_eq!(out, json!({"condition_result": true}));
        let out = execute(Some(&params), &json!({"hits": 0})).expect("min fail");
        assert_eq!(out, json!({"condition_result": false}));
    }

    #[test]
    fn missing_expression_is_fail_closed() {
        let err = execute(Some("{}"), &json!({})).expect_err("missing expression");
        assert_eq!(err.code, "params_invalid");
    }

    #[test]
    fn invalid_schema_is_fail_closed() {
        let params = json!({"expression": {"type": "not-a-type"}}).to_string();
        let err = execute(Some(&params), &json!({})).expect_err("bad schema");
        assert_eq!(err.code, "condition_invalid_expression");
    }

    #[test]
    fn state_ref_in_expression_resolves_before_compile() {
        // $/ 引用先解析再编译：const 引用 run 输入。
        let dynamic = json!({"expression": {"properties": {"kw": {"const": "$/kw"}}}});
        let out = execute(
            Some(&dynamic.to_string()),
            &json!({"kw": "nmap"}),
        )
        .expect("resolved const matches");
        assert_eq!(out, json!({"condition_result": true}));
        // 不可解析引用 fail-closed。
        let broken = json!({"expression": {"properties": {"kw": {"const": "$/missing"}}}});
        let err = execute(Some(&broken.to_string()), &json!({})).expect_err("unresolvable");
        assert_eq!(err.code, "state_ref_unresolvable");
        assert_eq!(STATE_REF_PREFIX, "$/");
    }
}
