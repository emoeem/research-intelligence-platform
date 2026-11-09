//! P1-5 契约测试最小集（TOOL-CONTRACT §10.2，ROADMAP P1-5 判据）：
//! 1. SUCCEEDED/FAILED/TIMED_OUT/CANCELLED 四路径；
//! 2. WAITING_FOR_RUNNER 注入（含恢复与等待期取消）；
//! 3. 截断信封一致性（§5.4，两 runtime 共用同一截断函数）；
//! 4. 幂等命中（cached=true 透传，照常入库）；
//! 5. 状态机 13 条跃迁全路径（11 个合法状态对全覆盖 + 终态不可迁出）；
//! 6. 提交校验失败不产生 run 行（D-TC-05）；
//! 7. §8.3/§9.3 两示例往返。
//!
//! 测试库：内存库迁移全链 001..005；工具经 tool-registry 登记两示例
//! Manifest（占位 digest 显式放行，仅测试库）。

use database::{migrate, DbPath, Pool, PoolConfig};
use domain::ToolRunStatus;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tool_engine::mock_runner::{MockRunner, Outcome};
use tool_engine::{Engine, EngineError};
use tool_registry::{Options, Registry};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root")
}

fn migrations_dir() -> PathBuf {
    repo_root().join("migrations")
}

fn examples_dir() -> PathBuf {
    repo_root().join("schemas/tool-manifest/examples")
}

fn fresh_pool() -> Pool {
    let pool = Pool::open(PoolConfig {
        path: DbPath::Memory,
        readonly: false,
        max_connections: 1,
        busy_timeout: Duration::from_secs(1),
    })
    .unwrap();
    migrate(&pool, &migrations_dir()).unwrap();
    pool
}

fn test_env() -> (Engine, Registry) {
    let pool = fresh_pool();
    let registry = Registry::open(pool.clone()).with_options(Options {
        allow_placeholder_digest: true,
    });
    for name in ["rip.doc.parse", "rip.http.inspect"] {
        let manifest: Value = serde_json::from_str(
            &std::fs::read_to_string(examples_dir().join(format!("{name}.json"))).unwrap(),
        )
        .unwrap();
        registry.register(&manifest).unwrap();
    }
    (Engine::open(pool, registry.clone()), registry)
}

/// §8.3 调用示例的提交体（run_id 可替换防重）。
fn request_8_3(run_id: &str) -> Value {
    json!({
        "protocol_version": "1",
        "tool_id": "rip.doc.parse",
        "run_id": run_id,
        "submit_key": "task-42-doc-a",
        "input": {"text": "# 标题\n\n正文一段。", "format": "md"},
        "workspace_id": "ws-default",
        "budget": {"timeout_seconds": 60, "max_output_bytes": 1048576},
        "result_schema_version": "1"
    })
}

/// §8.3 信封骨架的 result 形状（parsing_worker 真实输出形状）。
fn result_8_3() -> Value {
    json!({
        "format": "md",
        "source": {"kind": "inline", "path": null, "bytes": 30, "sha256": "abc"},
        "sections": [],
        "heading_tree": [],
        "stats": {"chars": 30, "sections": 1, "headings": 1}
    })
}

fn run_count(engine: &Engine) -> i64 {
    engine
        .pool()
        .with_conn(|c| c.query_row("SELECT COUNT(*) FROM tool_run", [], |r| r.get::<_, i64>(0)))
        .unwrap()
}

fn row_field(engine: &Engine, run_id: &str, column: &str) -> Option<String> {
    assert!(
        matches!(
            column,
            "status" | "started_at" | "cancel_requested_at" | "finished_at" | "error_category"
        ),
        "whitelisted column"
    );
    engine
        .pool()
        .with_conn(|c| {
            let sql = format!("SELECT {column} FROM tool_run WHERE run_id = ?1");
            c.query_row(&sql, [run_id], |r| r.get::<_, Option<String>>(0))
        })
        .unwrap()
}

// ---------------------------------------------------------------------------
// 1. 四路径 + 协作式取消两语义
// ---------------------------------------------------------------------------

#[test]
fn four_terminal_paths_and_cooperative_cancel_semantics() {
    let (engine, _registry) = test_env();

    // SUCCEEDED（第 9 行）
    engine.submit(&request_8_3("rn-succeed")).unwrap();
    let out = engine
        .drive(
            "rn-succeed",
            &MockRunner::succeed(result_8_3(), "解析 md 30 字节"),
        )
        .unwrap();
    assert_eq!(out.envelope.status, ToolRunStatus::Succeeded);
    assert_eq!(out.envelope.result.as_ref().unwrap(), &result_8_3());
    assert_eq!(out.envelope.summary.as_deref(), Some("解析 md 30 字节"));
    assert!(!out.envelope.usage.truncated);
    assert!(!out.envelope.replay.cached);
    assert_eq!(
        row_field(&engine, "rn-succeed", "status").as_deref(),
        Some("SUCCEEDED")
    );
    assert!(row_field(&engine, "rn-succeed", "started_at").is_some());
    assert!(row_field(&engine, "rn-succeed", "finished_at").is_some());

    // FAILED（第 10 行，EXECUTION）
    engine.submit(&request_8_3("rn-failed")).unwrap();
    let out = engine
        .drive(
            "rn-failed",
            &MockRunner::fail("EXECUTION", json!(-32006), "worker exploded"),
        )
        .unwrap();
    assert_eq!(out.envelope.status, ToolRunStatus::Failed);
    assert!(out.envelope.result.is_none());
    let err = out.envelope.error.as_ref().unwrap();
    assert_eq!(err.category, "EXECUTION");
    assert_eq!(err.code, json!(-32006));
    assert!(out.envelope.summary.is_some());
    assert_eq!(
        row_field(&engine, "rn-failed", "error_category").as_deref(),
        Some("EXECUTION")
    );

    // TIMED_OUT（第 11 行）
    engine.submit(&request_8_3("rn-timeout")).unwrap();
    let out = engine
        .drive("rn-timeout", &MockRunner::timeout("budget exhausted"))
        .unwrap();
    assert_eq!(out.envelope.status, ToolRunStatus::TimedOut);
    assert_eq!(out.envelope.error.as_ref().unwrap().category, "TIMEOUT");
    assert_eq!(out.envelope.error.as_ref().unwrap().code, json!(-32002));

    // CANCELLED（第 12 行：取消请求后终止，cancel_requested_at 已记）
    engine.submit(&request_8_3("rn-cancelled")).unwrap();
    let out = engine
        .drive(
            "rn-cancelled",
            &MockRunner {
                outcome: Outcome::CancelThenTerminate {
                    message: "terminated by cancel token".into(),
                },
                ..MockRunner::bare()
            },
        )
        .unwrap();
    assert_eq!(out.envelope.status, ToolRunStatus::Cancelled);
    assert_eq!(out.envelope.error.as_ref().unwrap().category, "CANCELLED");
    assert_eq!(out.envelope.error.as_ref().unwrap().code, json!(-32004));
    assert!(row_field(&engine, "rn-cancelled", "cancel_requested_at").is_some());

    // 第 13 行：取消请求后任务仍自行完成 → SUCCEEDED（协作式取消语义）
    engine.submit(&request_8_3("rn-cancel-completes")).unwrap();
    let out = engine
        .drive(
            "rn-cancel-completes",
            &MockRunner {
                outcome: Outcome::CancelButCompletes {
                    result: result_8_3(),
                    summary: "完成于取消请求之后".into(),
                },
                ..MockRunner::bare()
            },
        )
        .unwrap();
    assert_eq!(out.envelope.status, ToolRunStatus::Succeeded);
    assert!(row_field(&engine, "rn-cancel-completes", "cancel_requested_at").is_some());
    assert_eq!(
        row_field(&engine, "rn-cancel-completes", "status").as_deref(),
        Some("SUCCEEDED")
    );
}

// ---------------------------------------------------------------------------
// 2. WAITING_FOR_RUNNER 注入（第 3、4、5 行）
// ---------------------------------------------------------------------------

#[test]
fn waiting_for_runner_injection_recover_and_cancel() {
    let (engine, _registry) = test_env();

    // 等待 → 心跳恢复 → 正常完成（第 3、4 行）
    engine.submit(&request_8_3("rn-wait-recover")).unwrap();
    let runner = MockRunner {
        wait_for_runner: true,
        recover_after_wait: true,
        outcome: Outcome::Succeed {
            result: result_8_3(),
            summary: "恢复后完成".into(),
        },
        ..MockRunner::bare()
    };
    let out = engine.drive("rn-wait-recover", &runner).unwrap();
    assert_eq!(
        out.transcript,
        vec![
            ToolRunStatus::Queued,
            ToolRunStatus::WaitingForRunner,
            ToolRunStatus::Preparing,
            ToolRunStatus::Running,
            ToolRunStatus::Succeeded,
        ]
    );

    // 等待期内取消（第 5 行）
    engine.submit(&request_8_3("rn-wait-cancel")).unwrap();
    let runner = MockRunner {
        wait_for_runner: true,
        cancel_while_waiting: true,
        ..MockRunner::bare()
    };
    let out = engine.drive("rn-wait-cancel", &runner).unwrap();
    assert_eq!(
        out.transcript,
        vec![
            ToolRunStatus::Queued,
            ToolRunStatus::WaitingForRunner,
            ToolRunStatus::Cancelled,
        ]
    );
    assert_eq!(out.envelope.status, ToolRunStatus::Cancelled);
    assert!(row_field(&engine, "rn-wait-cancel", "cancel_requested_at").is_some());

    // 既不恢复也不取消 → Stuck（引擎显式报错，不伪装失败）
    engine.submit(&request_8_3("rn-wait-stuck")).unwrap();
    let runner = MockRunner {
        wait_for_runner: true,
        recover_after_wait: false,
        ..MockRunner::bare()
    };
    assert!(matches!(
        engine.drive("rn-wait-stuck", &runner),
        Err(EngineError::Stuck(_))
    ));
}

// ---------------------------------------------------------------------------
// 3. 截断信封一致性（§5.4）
// ---------------------------------------------------------------------------

#[test]
fn truncation_envelope_consistency_both_runtime_paths() {
    // 共用函数：同输入同帽 → 完全一致（python_worker 侧与容器侧同源）。
    let big = json!({"blob": "x".repeat(4096)});
    let a = tool_engine::envelope::truncate("rn-a", &big, 256);
    let b = tool_engine::envelope::truncate("rn-a", &big, 256);
    assert_eq!(a.result, b.result);
    assert_eq!(a.original_sha256, b.original_sha256);
    assert_eq!(a.artifact_id, b.artifact_id);
    assert!(a.truncated);

    // 驱动面：预算帽 256 时成功结果被截断，原始字节落 artifact（§9.3 第 4 步）。
    let (engine, _registry) = test_env();
    engine
        .submit(&json!({
            "protocol_version": "1",
            "tool_id": "rip.doc.parse",
            "run_id": "rn-truncated",
            "input": {"text": "x"},
            "workspace_id": "ws-default",
            "budget": {"timeout_seconds": 60, "max_output_bytes": 256}
        }))
        .unwrap();
    let big_result = json!({
        "format": "md",
        "source": {"kind": "inline", "path": null, "bytes": 1, "sha256": "abc"},
        "sections": [],
        "heading_tree": [],
        "stats": {"chars": 1, "sections": 0, "headings": 0, "blob": "y".repeat(2048)}
    });
    let out = engine
        .drive(
            "rn-truncated",
            &MockRunner::succeed(big_result.clone(), "大结果"),
        )
        .unwrap();
    assert!(out.envelope.usage.truncated);
    let notice = out.envelope.result.as_ref().unwrap().as_object().unwrap();
    assert_eq!(notice["truncated"], json!(true));
    let artifact = out.envelope.artifacts.first().unwrap();
    assert_eq!(artifact.role, "primary");
    assert_eq!(artifact.sha256.len(), 64);
    assert_eq!(artifact.mime_type, "application/json");
    // artifact 行落库对账（§5.2 七字段）
    let row: (String, String, String, i64) = engine
        .pool()
        .with_conn(|c| {
            c.query_row(
                "SELECT role, sha256, mime_type, size_bytes FROM artifact WHERE run_id = ?1 AND role = 'primary'",
                ["rn-truncated"],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
        })
        .unwrap();
    assert_eq!(row.0, "primary");
    assert_eq!(row.1, artifact.sha256);
    assert!(row.3 >= 256, "truncation artifact size must exceed the cap");
    // 容器 runtime 指纹下同一函数路径：信封 usage.truncated 一致
    let container_runner = MockRunner {
        runtime_type: "podman_vm".into(),
        outcome: Outcome::Succeed {
            result: big_result,
            summary: "大结果（容器面）".into(),
        },
        ..MockRunner::bare()
    };
    engine
        .submit(&json!({
            "protocol_version": "1",
            "tool_id": "rip.doc.parse",
            "run_id": "rn-truncated-container",
            "input": {"text": "x"},
            "workspace_id": "ws-default",
            "budget": {"timeout_seconds": 60, "max_output_bytes": 256}
        }))
        .unwrap();
    let out2 = engine
        .drive("rn-truncated-container", &container_runner)
        .unwrap();
    assert!(out2.envelope.usage.truncated);
    assert_eq!(
        out2.envelope.usage.truncated, out.envelope.usage.truncated,
        "两 runtime 面截断行为必须一致（§5.4）"
    );
}

// ---------------------------------------------------------------------------
// 4. 幂等命中（cached=true 透传，照常入库）
// ---------------------------------------------------------------------------

#[test]
fn idempotent_hit_marks_cached_and_still_records_run() {
    let (engine, _registry) = test_env();
    // 第一次：真实执行
    engine.submit(&request_8_3("rn-idem-1")).unwrap();
    let first = engine
        .drive("rn-idem-1", &MockRunner::succeed(result_8_3(), "解析 md"))
        .unwrap();
    assert!(!first.envelope.replay.cached);
    // 第二次：同 submit_key，runner 返回幂等命中结果
    engine.submit(&request_8_3("rn-idem-2")).unwrap();
    let runner = MockRunner {
        cached: true,
        outcome: Outcome::Succeed {
            result: first.envelope.result.clone().unwrap(),
            summary: "解析 md".into(),
        },
        ..MockRunner::bare()
    };
    let second = engine.drive("rn-idem-2", &runner).unwrap();
    assert!(second.envelope.replay.cached);
    assert_eq!(
        second.envelope.replay.submit_key.as_deref(),
        Some("task-42-doc-a")
    );
    assert_eq!(
        second.envelope.result, first.envelope.result,
        "幂等命中必须返回相同结果"
    );
    assert_eq!(second.envelope.status, ToolRunStatus::Succeeded);
    // 幂等命中也是一次合法运行记录：两行均 SUCCEEDED
    assert_eq!(run_count(&engine), 2);
    assert_eq!(
        row_field(&engine, "rn-idem-1", "status").as_deref(),
        Some("SUCCEEDED")
    );
    assert_eq!(
        row_field(&engine, "rn-idem-2", "status").as_deref(),
        Some("SUCCEEDED")
    );
}

// ---------------------------------------------------------------------------
// 5. 状态机 13 条跃迁全路径
// ---------------------------------------------------------------------------

#[test]
fn state_machine_thirteen_rows_full_path_coverage() {
    // 表面：11 个合法状态对（13 行去重；RUNNING→SUCCEEDED 两行同对、
    // 初始落库行由每次 submit 承载）。
    let legal: std::collections::HashSet<(ToolRunStatus, ToolRunStatus)> =
        tool_engine::state::LEGAL_TRANSITIONS
            .iter()
            .copied()
            .collect();
    assert_eq!(legal.len(), 11);

    // 驱动面：各注入路径的 transcript 状态对并集必须覆盖全部 11 对。
    let (engine, _registry) = test_env();
    let mut covered: std::collections::HashSet<(ToolRunStatus, ToolRunStatus)> =
        std::collections::HashSet::new();
    let mut drive_case = |run_id: &str, runner: &MockRunner| {
        engine.submit(&request_8_3(run_id)).unwrap();
        let out = engine.drive(run_id, runner).unwrap();
        for pair in out.transcript.windows(2) {
            covered.insert((pair[0], pair[1]));
        }
    };

    drive_case("sm-normal", &MockRunner::succeed(result_8_3(), "ok"));
    drive_case(
        "sm-wait-recover",
        &MockRunner {
            wait_for_runner: true,
            outcome: Outcome::Succeed {
                result: result_8_3(),
                summary: "ok".into(),
            },
            ..MockRunner::bare()
        },
    );
    drive_case(
        "sm-wait-cancel",
        &MockRunner {
            wait_for_runner: true,
            cancel_while_waiting: true,
            ..MockRunner::bare()
        },
    );
    drive_case(
        "sm-prep-fail",
        &MockRunner {
            fail_in_preparing: true,
            preparing_error_category: "POLICY_DENIED".into(),
            ..MockRunner::bare()
        },
    );
    drive_case(
        "sm-run-fail",
        &MockRunner::fail("INPUT", json!(-32008), "bad input"),
    );
    drive_case(
        "sm-node-loss",
        &MockRunner {
            preparing_node_loss: true,
            outcome: Outcome::Succeed {
                result: result_8_3(),
                summary: "ok".into(),
            },
            ..MockRunner::bare()
        },
    );
    drive_case("sm-timeout", &MockRunner::timeout("t"));
    drive_case(
        "sm-cancel",
        &MockRunner {
            outcome: Outcome::CancelThenTerminate {
                message: "c".into(),
            },
            ..MockRunner::bare()
        },
    );
    drive_case(
        "sm-cancel-completes",
        &MockRunner {
            outcome: Outcome::CancelButCompletes {
                result: result_8_3(),
                summary: "ok".into(),
            },
            ..MockRunner::bare()
        },
    );
    // 第 9 行与第 13 行同为 RUNNING→SUCCEEDED：两条不同语义路径均已驱动。
    assert!(covered.contains(&(ToolRunStatus::Running, ToolRunStatus::Succeeded)));
    assert_eq!(
        covered, legal,
        "驱动路径状态对并集必须覆盖 §3.2 全部 11 个合法状态对（13 行去重）"
    );

    // 终态不可迁出：再次 drive 终态 run 报 NotStartable。
    engine.submit(&request_8_3("sm-terminal")).unwrap();
    engine
        .drive("sm-terminal", &MockRunner::succeed(result_8_3(), "ok"))
        .unwrap();
    assert!(matches!(
        engine.drive("sm-terminal", &MockRunner::succeed(result_8_3(), "again")),
        Err(EngineError::NotStartable { .. })
    ));
}

// ---------------------------------------------------------------------------
// 6. D-TC-05：提交校验失败不产生 run 行
// ---------------------------------------------------------------------------

#[test]
fn d_tc05_rejected_submissions_produce_no_run_row() {
    let (engine, registry) = test_env();
    assert_eq!(run_count(&engine), 0);

    // 未知工具
    let mut req = request_8_3("rn-rej-1");
    req["tool_id"] = json!("rip.no.such");
    assert!(matches!(
        engine.submit(&req),
        Err(EngineError::Rejected(
            tool_engine::request::Rejection::UnknownTool(_)
        ))
    ));
    // 未登记版本
    let mut req = request_8_3("rn-rej-2");
    req["tool_version"] = json!("9.9.9");
    assert!(matches!(
        engine.submit(&req),
        Err(EngineError::Rejected(
            tool_engine::request::Rejection::UnknownVersion { .. }
        ))
    ));
    // 预算超帽（timeout 61 > manifest 60）
    let mut req = request_8_3("rn-rej-3");
    req["budget"]["timeout_seconds"] = json!(61);
    assert!(matches!(
        engine.submit(&req),
        Err(EngineError::Rejected(
            tool_engine::request::Rejection::BudgetExceeds(_)
        ))
    ));
    // input 不合 input_schema（text 与 path 皆缺，违反 oneOf）
    let mut req = request_8_3("rn-rej-4");
    req["input"] = json!({});
    assert!(matches!(
        engine.submit(&req),
        Err(EngineError::Rejected(
            tool_engine::request::Rejection::InputSchemaViolation(_)
        ))
    ));
    // 结构层：禁用字段（secrets 出现在调用对象，D-TC-06）
    let mut req = request_8_3("rn-rej-5");
    req["secrets"] = json!([]);
    assert!(matches!(
        engine.submit(&req),
        Err(EngineError::Rejected(
            tool_engine::request::Rejection::Structural(_)
        ))
    ));

    // T2 缺授权引用（POLICY_DENIED，§2.4 / D-TC-05）
    let mut t2 = serde_json::from_str::<Value>(
        &std::fs::read_to_string(examples_dir().join("rip.doc.parse.json")).unwrap(),
    )
    .unwrap();
    t2["id"] = json!("rip.t.t2");
    t2["name"] = json!("T2 Probe");
    t2["permission_level"] = json!("T2");
    t2["version"] = json!("0.1.0");
    registry.register(&t2).unwrap();
    let mut req = request_8_3("rn-rej-6");
    req["tool_id"] = json!("rip.t.t2");
    assert!(matches!(
        engine.submit(&req),
        Err(EngineError::Rejected(
            tool_engine::request::Rejection::MissingAuthorization(_)
        ))
    ));
    // 带授权引用即可受理
    let mut req_ok = req.clone();
    req_ok["run_id"] = json!("rn-acc-t2");
    req_ok["authorization_ref"] = json!("authz-2026-001");
    assert!(engine.submit(&req_ok).is_ok());

    // 全程仅两行：T2 受理 1 行 + rn-acc-t2 之外全部拒绝
    assert_eq!(run_count(&engine), 1, "被拒绝的提交不得产生 run 行");
}

// ---------------------------------------------------------------------------
// 7. §8.3 / §9.3 两示例往返
// ---------------------------------------------------------------------------

#[test]
fn round_trip_contract_8_3_envelope_skeleton() {
    let (engine, _registry) = test_env();
    engine.submit(&request_8_3("rn-01HTXYZEXAMPLE")).unwrap();
    let out = engine
        .drive(
            "rn-01HTXYZEXAMPLE",
            &MockRunner::succeed(result_8_3(), "解析 md 30 字节，1 个章节 1 个标题"),
        )
        .unwrap();
    let v = tool_engine::envelope::to_value(&out.envelope);
    let obj = v.as_object().unwrap();
    // §8.3 骨架键集逐键在位（无多余 artifacts/evidence_refs）
    for key in [
        "protocol_version",
        "run_id",
        "tool_id",
        "tool_version",
        "status",
        "result",
        "summary",
        "usage",
        "environment",
        "replay",
    ] {
        assert!(obj.contains_key(key), "§8.3 envelope missing {key}");
    }
    assert!(!obj.contains_key("artifacts"));
    assert!(!obj.contains_key("evidence_refs"));
    assert_eq!(obj["protocol_version"], json!("1"));
    assert_eq!(obj["run_id"], json!("rn-01HTXYZEXAMPLE"));
    assert_eq!(obj["tool_id"], json!("rip.doc.parse"));
    assert_eq!(obj["tool_version"], json!("0.1.0"));
    assert_eq!(obj["status"], json!("SUCCEEDED"));
    assert_eq!(obj["result"], result_8_3());
    assert_eq!(obj["replay"]["submit_key"], json!("task-42-doc-a"));
    assert_eq!(obj["environment"]["runtime_type"], json!("python_worker"));
    assert!(obj["usage"]["elapsed_ms"].is_u64());
    // 行侧对账：permission_ctx 记 T0（§3.3）
    let ctx: String = engine
        .pool()
        .with_conn(|c| {
            c.query_row(
                "SELECT permission_ctx FROM tool_run WHERE run_id = ?1",
                ["rn-01HTXYZEXAMPLE"],
                |r| r.get(0),
            )
        })
        .unwrap();
    assert!(ctx.contains("T0"), "permission_ctx: {ctx}");
}

#[test]
fn round_trip_contract_9_3_flow() {
    let (engine, _registry) = test_env();
    // §9.3 第 1 步：QUEUED 提交，Policy 校验 T1 无需授权引用
    engine
        .submit(&json!({
            "protocol_version": "1",
            "tool_id": "rip.http.inspect",
            "run_id": "rn-http-93",
            "input": {"url": "https://example.com/", "method": "GET"},
            "workspace_id": "ws-default"
        }))
        .unwrap();
    assert_eq!(
        row_field(&engine, "rn-http-93", "status").as_deref(),
        Some("QUEUED")
    );
    // §9.3 第 2 到 4 步：PREPARING（挂载/env 渲染）→ RUNNING（出站）→ 终态
    let runner = MockRunner {
        runtime_type: "podman_vm".into(),
        outcome: Outcome::Succeed {
            result: json!({"status_code": 200, "final_url": "https://example.com/"}),
            summary: "HTTP 200".into(),
        },
        ..MockRunner::bare()
    };
    let out = engine.drive("rn-http-93", &runner).unwrap();
    assert_eq!(
        out.transcript,
        vec![
            ToolRunStatus::Queued,
            ToolRunStatus::Preparing,
            ToolRunStatus::Running,
            ToolRunStatus::Succeeded,
        ],
        "§9.3 步骤序列 QUEUED→PREPARING→RUNNING→SUCCEEDED"
    );
    assert_eq!(out.envelope.tool_id, "rip.http.inspect");
    assert_eq!(
        out.envelope.result.as_ref().unwrap()["status_code"],
        json!(200)
    );
    // §3.3 行侧：T1 权限上下文 + started_at 对应 RUNNING 进入时刻
    let ctx: String = engine
        .pool()
        .with_conn(|c| {
            c.query_row(
                "SELECT permission_ctx FROM tool_run WHERE run_id = ?1",
                ["rn-http-93"],
                |r| r.get(0),
            )
        })
        .unwrap();
    assert!(ctx.contains("T1"), "permission_ctx: {ctx}");
    assert!(row_field(&engine, "rn-http-93", "started_at").is_some());
    assert!(row_field(&engine, "rn-http-93", "finished_at").is_some());
}
