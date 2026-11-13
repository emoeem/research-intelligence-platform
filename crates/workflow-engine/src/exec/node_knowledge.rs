//! Knowledge 节点执行器（P8-2）：context-broker Broker::assemble 检索
//! （Agent 与知识库之间的唯一上下文装配层，装配全量复用既有 Broker，
//! 本层零 SQL 不自查库，P7-3 判据同源）。
//!
//! 节点 params：
//! - `query`（必填，validate.rs 词表）：检索文本；`$/` 前缀字符串按 run
//!   state JSON Pointer 解析（如 `$/target` 取 run 输入 target）；
//! - `actor`（可选，缺省 `rp-workflow`）：装配审计 actor。
//!
//! 输出（JSON object，浅合并进 run state）：装配摘要投影
//! `{"query", "context_id", "manifest_id", "intent_status", "confidence",
//! "knowledge_count", "knowledge": [{item_id, ref_id, source, title,
//! grade, summary 截 160 字}], "tool_brief_count", "warnings"}`。
//! Broker 未注入时 fail-closed code=broker_unavailable；装配失败按
//! ContextBrokerError 原文 failed（fail-closed，不静默放空集装成功）。
//! 装配副作用（manifest 只追加 + audit 事件 + expand 记账）由 Broker 内部
//! 完成，correlation_id = run_id（审计链同源）。

use super::NodeFailure;
use crate::exec::resolve_refs;
use crate::state::type_word;
use context_broker::{AssembleRequest, AssembledContext, Broker};
use serde_json::{json, Map, Value};

/// Knowledge 节点执行。
pub fn execute(
    broker: Option<&Broker>,
    run_id: &str,
    _node_id: &str,
    params_json: Option<&str>,
    state: &Value,
) -> Result<Value, NodeFailure> {
    let params: Map<String, Value> = match params_json {
        Some(raw) => serde_json::from_str(raw)
            .map_err(|e| NodeFailure::new("params_invalid", format!("knowledge node params parse failed: {e}")))?,
        None => Map::new(),
    };
    let query_raw = params
        .get("query")
        .and_then(Value::as_str)
        .ok_or_else(|| NodeFailure::new("params_invalid", "knowledge node params missing query"))?;
    let query = match resolve_refs(&json!(query_raw), state)? {
        Value::String(s) => s,
        other => {
            return Err(NodeFailure::new(
                "params_invalid",
                format!("knowledge query must resolve to a string, got {}", type_word(&other)),
            ))
        }
    };
    let Some(broker) = broker else {
        return Err(NodeFailure::new(
            "broker_unavailable",
            "no Broker configured; install one via WorkflowExecutor::with_broker",
        ));
    };
    let actor = params
        .get("actor")
        .and_then(Value::as_str)
        .unwrap_or("rp-workflow");
    let context = broker
        .assemble(&AssembleRequest {
            task_id: run_id.to_string(),
            text: query.clone(),
            correlation_id: run_id.to_string(),
            trace_id: None,
            actor: actor.to_string(),
        })
        .map_err(|e| NodeFailure::new("broker_assemble_failed", format!("broker assemble failed: {e}")))?;
    Ok(summarize(&context, &query))
}

/// AssembledContext -> 节点输出投影（摘要形态；正文永不预装，ref-only
/// 条目的 summary 为空串原样保留，设计稿 §5.3 形态不扭曲）。
fn summarize(context: &AssembledContext, query: &str) -> Value {
    let knowledge: Vec<Value> = context
        .layers
        .l2
        .knowledge_summaries
        .iter()
        .map(|k| {
            json!({
                "item_id": k.item_id,
                "ref_id": k.ref_id,
                "source": k.source,
                "title": k.title,
                "grade": k.grade,
                "summary": k.summary.chars().take(160).collect::<String>(),
            })
        })
        .collect();
    let tool_brief_count = context.layers.l2.tool_briefs.len();
    json!({
        "query": query,
        "context_id": context.context_id,
        "manifest_id": context.manifest_id,
        "intent_status": context.intent.status,
        "confidence": context.intent.confidence,
        "knowledge_count": knowledge.len(),
        "knowledge": knowledge,
        "tool_brief_count": tool_brief_count,
        "warnings": context.warnings,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use context_broker::{Candidate, Content, ContentRef, ContextBrokerError, ContextItemKind, ContextLayer, IntentQuery};

    /// 测试载具源（ContextSource 端口；Mock 仅作测试载具，决议 2 口径）。
    struct StubSource;

    impl context_broker::ContextSource for StubSource {
        fn id(&self) -> &str {
            "stub"
        }
        fn candidates(
            &self,
            _q: &IntentQuery,
            _limit: usize,
        ) -> Result<Vec<Candidate>, ContextBrokerError> {
            Ok(Vec::new())
        }
        fn load(&self, _r: &ContentRef) -> Result<Content, ContextBrokerError> {
            Err(ContextBrokerError::Source {
                source: "stub".to_string(),
                reason: "nothing to load".to_string(),
            })
        }
    }

    /// 独立临时库（同名路径并行测试互删文件会触发 migrate 版本错配竞态，
    /// 每用例独立 tag）。
    fn stub_broker(tag: &str) -> Broker {
        let dir = std::env::temp_dir().join(format!(
            "rip-wf-node-kb-{}-{}",
            tag,
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).expect("dir");
        let path = dir.join("kb.db");
        let _ = std::fs::remove_file(&path);
        let pool = database::Pool::open(database::PoolConfig {
            path: database::DbPath::File(path),
            readonly: false,
            max_connections: 1,
            busy_timeout: std::time::Duration::from_secs(5),
        })
        .expect("pool");
        database::migrate(
            &pool,
            std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../migrations")),
        )
        .expect("migrate");
        // 意图配置取仓库 rulepack（与 context-broker 既有测试同源锚定）。
        let rulepack_dir = std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../rulepacks/rip-rules"));
        Broker::new(
            vec![Box::new(StubSource)],
            context_broker::BudgetPolicy::default_for(8000).expect("policy"),
            context_broker::IntentConfig::from_dir(rulepack_dir).expect("intent config"),
            context_broker::ManifestStore::new(pool),
            None,
        )
    }

    #[test]
    fn params_missing_query_is_fail_closed() {
        let err = execute(None, "wfr-x", "k1", Some("{}"), &json!({})).expect_err("missing query");
        assert_eq!(err.code, "params_invalid");
    }

    #[test]
    fn broker_unavailable_is_fail_closed() {
        let err = execute(None, "wfr-x", "k1", Some(r#"{"query":"q"}"#), &json!({}))
            .expect_err("no broker");
        assert_eq!(err.code, "broker_unavailable");
    }

    #[test]
    fn assemble_with_stub_source_succeeds_and_projects_summary() {
        let broker = stub_broker("kb-1");
        let out = execute(
            Some(&broker),
            "wfr-kb-1",
            "k1",
            Some(r#"{"query":"example.com"}"#),
            &json!({"target": "example.com"}),
        )
        .expect("assemble ok");
        assert_eq!(out["query"], "example.com");
        assert!(out["context_id"].as_str().expect("ctx id").starts_with("ctx-"));
        assert!(out["manifest_id"].as_str().is_some());
        assert_eq!(out["knowledge_count"], 0);
        assert!(out["intent_status"].as_str().is_some());
    }

    #[test]
    fn query_state_ref_resolves() {
        let broker = stub_broker("kb-2");
        let out = execute(
            Some(&broker),
            "wfr-kb-2",
            "k1",
            Some(r#"{"query":"$/target"}"#),
            &json!({"target": "example.com"}),
        )
        .expect("assemble ok");
        assert_eq!(out["query"], "example.com");
    }

    #[test]
    fn intent_layer_word_is_recognized() {
        // 词表冒烟：ContextLayer 枚举可用（保持 import 一致性）。
        assert_ne!(ContextItemKind::Tool, ContextItemKind::Knowledge);
        assert_ne!(ContextLayer::L0, ContextLayer::L2);
    }
}
