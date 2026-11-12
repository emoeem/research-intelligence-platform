//! P7-5 trace 关联写面（SESSION-HANDOFF §十追加项 1；迁移 015）：
//! 1. submit_correlated 受理 + run 行 trace_id 落列；trace_id 为 None 时行
//!    内列 NULL（工具直调面不伪造关联）；
//! 2. 非法 trace_id 拒绝（domain::ids::TraceId 校验，行已受理不回滚 =
//!    受理语义不变，仅关联列不落）；
//! 3. Mock 驱动到终态登记 artifact：产物行 trace_id 从 run 行同链传播。
//!
//! 测试库：内存库迁移全链 001..015；工具经 tool-registry 登记示例 Manifest
//! （占位 digest 显式放行，仅测试库）。

use database::{migrate, rusqlite, DbPath, Pool, PoolConfig};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::time::Duration;
use tool_engine::mock_runner::MockRunner;
use tool_engine::Engine;
use tool_registry::{Options, Registry};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root")
}

fn fresh_engine() -> Engine {
    let pool = Pool::open(PoolConfig {
        path: DbPath::Memory,
        readonly: false,
        max_connections: 1,
        busy_timeout: Duration::from_secs(1),
    })
    .unwrap();
    migrate(&pool, &repo_root().join("migrations")).unwrap();
    let registry = Registry::open(pool.clone()).with_options(Options {
        allow_placeholder_digest: true,
    });
    let manifest: Value = serde_json::from_str(
        &std::fs::read_to_string(
            repo_root().join("schemas/tool-manifest/examples/rip.doc.parse.json"),
        )
        .unwrap(),
    )
    .unwrap();
    registry.register(&manifest).unwrap();
    Engine::open(pool, registry)
}

fn request(run_id: &str) -> Value {
    json!({
        "protocol_version": "1",
        "tool_id": "rip.doc.parse",
        "run_id": run_id,
        "submit_key": format!("task-42-{run_id}"),
        "input": {"text": "# 标题\n\n正文一段。", "format": "md"},
        "workspace_id": "ws-default",
        "budget": {"timeout_seconds": 60, "max_output_bytes": 1048576},
        "result_schema_version": "1"
    })
}

fn trace_of(pool: &Pool, run_id: &str) -> Option<String> {
    pool.with_conn(|c| {
        c.query_row(
            "SELECT trace_id FROM tool_run WHERE run_id = ?1",
            [run_id],
            |r| r.get::<_, Option<String>>(0),
        )
    })
    .unwrap()
}

/// 1. 关联提交落列 + None 等价 submit。
#[test]
fn submit_correlated_persists_trace_id_and_none_keeps_null() {
    let engine = fresh_engine();
    engine
        .submit_correlated(&request("rn-trace-a"), Some("tr-corr-1"))
        .unwrap();
    assert_eq!(
        trace_of(engine.pool(), "rn-trace-a").as_deref(),
        Some("tr-corr-1")
    );
    assert_eq!(
        engine.trace_id_of("rn-trace-a").unwrap().as_deref(),
        Some("tr-corr-1")
    );

    engine
        .submit_correlated(&request("rn-plain"), None)
        .unwrap();
    assert_eq!(
        trace_of(engine.pool(), "rn-plain"),
        None,
        "None 提交不伪造关联列"
    );
    assert_eq!(engine.trace_id_of("rn-plain").unwrap(), None);
}

/// 2. 非法 trace_id：受理校验面照常落 run 行（D-TC-05 语义不变），关联列
///    拒落（空串不合法；domain::TraceId 校验拒绝）。
#[test]
fn submit_correlated_rejects_invalid_trace_id() {
    let engine = fresh_engine();
    let err = engine
        .submit_correlated(&request("rn-bad-trace"), Some(""))
        .unwrap_err();
    assert!(
        err.to_string().contains("trace id"),
        "拒绝原因为 trace id 校验：{err}"
    );
    assert_eq!(trace_of(engine.pool(), "rn-bad-trace"), None);
}

/// 3. Mock 驱动到 SUCCEEDED：产物行 trace_id 从 run 行同链传播。
///    （mock 路径登记产物的入口是 §5.4 截断面，预算 max_output_bytes 压到
///    256 触发截断，产物行必落。）
#[test]
fn artifact_rows_inherit_trace_id_from_run_row() {
    let engine = fresh_engine();
    let mut req = request("rn-art-trace");
    req["budget"]["max_output_bytes"] = json!(256);
    engine.submit_correlated(&req, Some("tr-corr-2")).unwrap();
    engine
        .drive(
            "rn-art-trace",
            &MockRunner::succeed(
                json!({
                    "format": "md",
                    "source": {"kind": "inline", "path": null, "bytes": 30, "sha256": "abc"},
                    "sections": [],
                    "heading_tree": [],
                    "stats": {"chars": 30, "sections": 1, "headings": 1},
                    "padding": "x".repeat(600)
                }),
                "解析完成（P7-5 trace 传播用例）",
            ),
        )
        .unwrap();
    let tool_run_trace = trace_of(engine.pool(), "rn-art-trace");
    assert_eq!(tool_run_trace.as_deref(), Some("tr-corr-2"));
    let artifact_traces: Vec<Option<String>> = engine
        .pool()
        .with_conn(|c| {
            let mut stmt =
                c.prepare("SELECT trace_id FROM artifact WHERE run_id = ?1 ORDER BY artifact_id")?;
            let rows = stmt
                .query_map(["rn-art-trace"], |r| r.get::<_, Option<String>>(0))?
                .collect::<Result<Vec<_>, rusqlite::Error>>()?;
            Ok::<Vec<Option<String>>, database::rusqlite::Error>(rows)
        })
        .unwrap();
    assert!(
        !artifact_traces.is_empty(),
        "SUCCEEDED 截断面登记产物（envelope 产物收集面）"
    );
    for t in artifact_traces {
        assert_eq!(t.as_deref(), Some("tr-corr-2"), "产物行 trace_id 同链传播");
    }
}
