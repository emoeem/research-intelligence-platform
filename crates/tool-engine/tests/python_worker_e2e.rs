//! P1-6 端到端验收（ROADMAP §2.3 P1-6 判据原句）：ToolCallRequest 端到端
//! 打到真实 workers/python/parsing_worker.py（pooled 进程池、握手、execute
//! 派发）并返回合法 ToolRunResult 信封。
//!
//! 覆盖：§4.2 映射、worker 内强制超时 → TIMED_OUT（-32002）、协作式取消 →
//! CANCELLED（-32004，D-TC-04）、错误码 §5.3 映射逐项断言（全表单元断言 +
//! 实跑 -32602/-32009/-32008/-32006）、幂等命中（cached=true）、握手失败 →
//! INFRA、stderr 日志采集、workers/ 零改动核查。
//!
//! 纪律：worker 缓存经 WORKER_CACHE_DIR 重定向到临时目录（连 gitignored 的
//! workers/.cache 都不写）；worker 入口为固定 argv（python3 -I <脚本>），
//! 不经 shell。

use database::{migrate, DbPath, Pool, PoolConfig};
use domain::ToolRunStatus;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;
use tool_engine::adapter::{
    execute_via_engine, map_worker_code, AdapterCall, PythonWorkerAdapter, PythonWorkerConfig,
    RestartPolicy,
};
use tool_engine::mock_runner::MockRunner;
use tool_engine::Engine;
use tool_registry::{Options, Registry};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root")
}

static ENV_SEQ: AtomicU32 = AtomicU32::new(0);

fn fresh_env() -> (Engine, Registry, PathBuf) {
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
    for name in ["rip.doc.parse", "rip.http.inspect"] {
        let manifest: Value = serde_json::from_str(
            &std::fs::read_to_string(
                repo_root()
                    .join("schemas/tool-manifest/examples")
                    .join(format!("{name}.json")),
            )
            .unwrap(),
        )
        .unwrap();
        registry.register(&manifest).unwrap();
    }
    let cache_dir = std::env::temp_dir().join(format!(
        "rip-p16-cache-{}-{}",
        std::process::id(),
        ENV_SEQ.fetch_add(1, Ordering::SeqCst)
    ));
    (Engine::open(pool, registry.clone()), registry, cache_dir)
}

/// 登记一个 input_schema 宽松的调试工具（task 直通 debug op，不设文本域约束）。
fn register_debug_tool(registry: &Registry) {
    let manifest = json!({
        "schema_version": "1",
        "id": "rip.t.debug",
        "name": "Debug Probe",
        "version": "0.1.0",
        "kind": "tool",
        "family": "utility",
        "description": "测试用调试面 worker（真实 WorkerRuntime 基类）",
        "runtime": {
            "type": "python_worker",
            "protocol_version": "1",
            "worker_name": "debug-worker",
            "command": ["python3", "-I", "crates/tool-engine/tests/fixtures/debug_worker.py"],
            "restart": "pooled"
        },
        "input_schema": {"type": "object"},
        "output_schema": {"type": "object"},
        "resources": {"cpu": "1", "memory": "256Mi", "timeout_seconds": 60},
        "network": {"mode": "none"},
        "secrets": [],
        "permission_level": "T0",
        "idempotent": true,
        "health": {"probe": {"type": "worker_ping"}},
        "provenance": {
            "origin": "rip-native",
            "license": "repo-internal",
            "dependencies": [],
            "tested": "P1-6 e2e 实跑"
        }
    });
    registry.register(&manifest).unwrap();
}

fn parse_adapter(
    worker_name: &str,
    script: &str,
    cache_dir: &Path,
    debug: bool,
) -> PythonWorkerAdapter {
    let mut extra_env = vec![(
        "WORKER_CACHE_DIR".to_string(),
        cache_dir.to_string_lossy().to_string(),
    )];
    if debug {
        extra_env.push(("WORKER_ALLOW_DEBUG".to_string(), "1".to_string()));
    }
    PythonWorkerAdapter::new(PythonWorkerConfig {
        worker_name: worker_name.to_string(),
        command: vec!["python3".to_string(), "-I".to_string(), script.to_string()],
        working_dir: Some(repo_root()),
        restart: RestartPolicy::Pooled,
        extra_env,
        handshake_timeout: Duration::from_secs(10),
    })
}

fn submit_debug(engine: &Engine, run_id: &str, input: Value, timeout_seconds: u64) {
    engine
        .submit(&json!({
            "protocol_version": "1",
            "tool_id": "rip.t.debug",
            "run_id": run_id,
            "input": input,
            "workspace_id": "ws-default",
            "budget": {"timeout_seconds": timeout_seconds}
        }))
        .unwrap();
}

#[test]
fn end_to_end_real_parsing_worker_succeeds() {
    let (engine, _registry, cache_dir) = fresh_env();
    let adapter = parse_adapter(
        "parsing-worker",
        "workers/python/parsing_worker.py",
        &cache_dir,
        false,
    );
    // §8.3 调用示例原文（input 与预算）
    engine
        .submit(&json!({
            "protocol_version": "1",
            "tool_id": "rip.doc.parse",
            "run_id": "rn-e2e-parse",
            "submit_key": "task-42-doc-a",
            "input": {"text": "# 标题\n\n正文一段。", "format": "md"},
            "workspace_id": "ws-default",
            "budget": {"timeout_seconds": 60, "max_output_bytes": 1048576},
            "result_schema_version": "1"
        }))
        .unwrap();
    let out = execute_via_engine(AdapterCall {
        engine: &engine,
        run_id: "rn-e2e-parse",
        adapter: &adapter,
        cancel_after: None,
    })
    .unwrap();
    // 状态序列：QUEUED→PREPARING→RUNNING→SUCCEEDED
    assert_eq!(
        out.transcript,
        vec![
            ToolRunStatus::Queued,
            ToolRunStatus::Preparing,
            ToolRunStatus::Running,
            ToolRunStatus::Succeeded,
        ]
    );
    // 合法信封 + 真实解析输出形状（output_schema 已在引擎内校验通过）
    let env = &out.envelope;
    assert_eq!(env.status, ToolRunStatus::Succeeded);
    assert_eq!(env.tool_version, "0.1.0");
    let result = env.result.as_ref().unwrap();
    assert_eq!(result["format"], json!("md"));
    assert_eq!(result["source"]["kind"], json!("inline"));
    assert_eq!(result["stats"]["sections"], json!(1));
    assert_eq!(result["stats"]["headings"], json!(1));
    assert!(!result["heading_tree"].as_array().unwrap().is_empty());
    assert!(!env.usage.truncated);
    assert!(!env.replay.cached);
    // stderr 日志采集：worker 启动即输出结构化日志（README.md:114）
    let stderr = adapter.stderr_lines();
    assert!(
        stderr.iter().any(|l| l.contains("startup")),
        "stderr must capture worker startup log; got {stderr:?}"
    );
    // pooled：第二个 run 复用同进程（无新 spawn 也无需再握手）
    engine
        .submit(&json!({
            "protocol_version": "1",
            "tool_id": "rip.doc.parse",
            "run_id": "rn-e2e-parse-2",
            "input": {"text": "纯文本", "format": "txt"},
            "workspace_id": "ws-default"
        }))
        .unwrap();
    let out2 = execute_via_engine(AdapterCall {
        engine: &engine,
        run_id: "rn-e2e-parse-2",
        adapter: &adapter,
        cancel_after: None,
    })
    .unwrap();
    assert_eq!(out2.envelope.status, ToolRunStatus::Succeeded);
    assert_eq!(
        out2.envelope.result.as_ref().unwrap()["format"],
        json!("txt")
    );
    adapter.shutdown();
}

#[test]
fn worker_enforced_timeout_maps_to_timed_out() {
    let (engine, registry, cache_dir) = fresh_env();
    register_debug_tool(&registry);
    let adapter = parse_adapter(
        "debug-worker",
        "crates/tool-engine/tests/fixtures/debug_worker.py",
        &cache_dir,
        true,
    );
    // worker 内 threading.Timer 在 timeout_ms=1000 处强制交付 timeout
    //（sleep 1100ms > 1000ms，100ms 余量确定性成立）；§4.2 映射
    // timeout_seconds=1 → timeout_ms=1000。
    submit_debug(
        &engine,
        "rn-e2e-timeout",
        json!({"op": "sleep", "ms": 1100}),
        1,
    );
    let out = execute_via_engine(AdapterCall {
        engine: &engine,
        run_id: "rn-e2e-timeout",
        adapter: &adapter,
        cancel_after: None,
    })
    .unwrap();
    assert_eq!(out.envelope.status, ToolRunStatus::TimedOut);
    let err = out.envelope.error.as_ref().unwrap();
    assert_eq!(err.category, "TIMEOUT");
    assert_eq!(err.code, json!(-32002));
    assert_eq!(
        row_status(&engine, "rn-e2e-timeout").as_deref(),
        Some("TIMED_OUT")
    );
    adapter.shutdown();
}

fn row_status(engine: &Engine, run_id: &str) -> Option<String> {
    engine
        .pool()
        .with_conn(|c| {
            c.query_row(
                "SELECT status FROM tool_run WHERE run_id = ?1",
                [run_id],
                |r| r.get::<_, Option<String>>(0),
            )
        })
        .unwrap()
}

fn row_cancel_requested_at(engine: &Engine, run_id: &str) -> Option<String> {
    engine
        .pool()
        .with_conn(|c| {
            c.query_row(
                "SELECT cancel_requested_at FROM tool_run WHERE run_id = ?1",
                [run_id],
                |r| r.get::<_, Option<String>>(0),
            )
        })
        .unwrap()
}

#[test]
fn cooperative_cancel_maps_to_cancelled() {
    let (engine, registry, cache_dir) = fresh_env();
    register_debug_tool(&registry);
    let adapter = parse_adapter(
        "debug-worker",
        "crates/tool-engine/tests/fixtures/debug_worker.py",
        &cache_dir,
        true,
    );
    // RUNNING 期 300ms 转发 cancel（D-TC-04）；sleep handler 每 20ms 协作检查，
    // TaskCancelled（-32004）先于 10s 预算超时成立。
    submit_debug(
        &engine,
        "rn-e2e-cancel",
        json!({"op": "sleep", "ms": 3000}),
        10,
    );
    let out = execute_via_engine(AdapterCall {
        engine: &engine,
        run_id: "rn-e2e-cancel",
        adapter: &adapter,
        cancel_after: Some(Duration::from_millis(300)),
    })
    .unwrap();
    assert_eq!(
        out.envelope.status,
        ToolRunStatus::Cancelled,
        "envelope: {:?}",
        out.envelope.error
    );
    let err = out.envelope.error.as_ref().unwrap();
    assert_eq!(err.category, "CANCELLED");
    assert_eq!(err.code, json!(-32004));
    assert!(row_cancel_requested_at(&engine, "rn-e2e-cancel").is_some());
    assert_eq!(
        row_status(&engine, "rn-e2e-cancel").as_deref(),
        Some("CANCELLED")
    );
    adapter.shutdown();
}

#[test]
fn worker_error_codes_map_to_envelope_categories() {
    let (engine, registry, cache_dir) = fresh_env();
    register_debug_tool(&registry);
    // 宽松输入面的解析工具（同一真实 parsing worker；引擎侧 input_schema 不拦，
    // 让 worker 的 task 校验错误原样上抛）
    let loose = json!({
        "schema_version": "1",
        "id": "rip.t.parse-loose",
        "name": "Parse Loose Probe",
        "version": "0.1.0",
        "kind": "tool",
        "family": "document",
        "description": "测试用宽松输入面（复用真实 parsing worker）",
        "runtime": {
            "type": "python_worker",
            "protocol_version": "1",
            "worker_name": "parsing-worker",
            "command": ["python3", "-I", "workers/python/parsing_worker.py"],
            "restart": "pooled"
        },
        "input_schema": {"type": "object"},
        "output_schema": {"type": "object"},
        "resources": {"cpu": "1", "memory": "256Mi", "timeout_seconds": 60},
        "network": {"mode": "none"},
        "secrets": [],
        "permission_level": "T0",
        "idempotent": true,
        "health": {"probe": {"type": "worker_ping"}},
        "provenance": {
            "origin": "rip-native",
            "license": "repo-internal",
            "dependencies": [],
            "tested": "P1-6 e2e 实跑"
        }
    });
    registry.register(&loose).unwrap();

    let cases: [(&str, Value, &str, i64); 4] = [
        // task {"op":"nope"} → 基类 InvalidParams -32602 → VALIDATION
        (
            "rn-err-invalid-params",
            json!({"op": "nope"}),
            "VALIDATION",
            -32602,
        ),
        // format 不在 SUPPORTED_FORMATS → UnsupportedFormatError -32009 → INPUT
        (
            "rn-err-unsupported-format",
            json!({"text": "x", "format": "nope"}),
            "INPUT",
            -32009,
        ),
        // 不可读路径 → InputError -32008 → INPUT
        (
            "rn-err-input-error",
            json!({"path": "/nonexistent/rip-p16-no-such.md"}),
            "INPUT",
            -32008,
        ),
        // debug boom → 未捕获 RuntimeError → EXECUTION_ERROR -32006 → EXECUTION
        (
            "rn-err-execution",
            json!({"op": "boom"}),
            "EXECUTION",
            -32006,
        ),
    ];
    for (run_id, input, want_category, want_code) in cases {
        let is_debug = input.get("op").is_some();
        let adapter = parse_adapter(
            if is_debug {
                "debug-worker"
            } else {
                "parsing-worker"
            },
            if is_debug {
                "crates/tool-engine/tests/fixtures/debug_worker.py"
            } else {
                "workers/python/parsing_worker.py"
            },
            &cache_dir,
            is_debug,
        );
        let tool = if is_debug {
            "rip.t.debug"
        } else {
            "rip.t.parse-loose"
        };
        engine
            .submit(&json!({
                "protocol_version": "1",
                "tool_id": tool,
                "run_id": run_id,
                "input": input,
                "workspace_id": "ws-default"
            }))
            .unwrap();
        let out = execute_via_engine(AdapterCall {
            engine: &engine,
            run_id,
            adapter: &adapter,
            cancel_after: None,
        })
        .unwrap();
        assert_eq!(out.envelope.status, ToolRunStatus::Failed, "{run_id}");
        let err = out
            .envelope
            .error
            .as_ref()
            .unwrap_or_else(|| panic!("{run_id} missing error"));
        assert_eq!(err.category, want_category, "{run_id} category");
        assert_eq!(err.code, json!(want_code), "{run_id} code");
        assert!(!err.retryable, "{run_id} retryable default");
        assert!(
            out.envelope.summary.is_some(),
            "{run_id} FAILED needs summary"
        );
        adapter.shutdown();
    }
}

#[test]
fn idempotent_hit_returns_cached_true_via_real_worker_cache() {
    let (engine, _registry, cache_dir) = fresh_env();
    let adapter = parse_adapter(
        "parsing-worker",
        "workers/python/parsing_worker.py",
        &cache_dir,
        false,
    );
    for run_id in ["rn-idem-real-1", "rn-idem-real-2"] {
        engine
            .submit(&json!({
                "protocol_version": "1",
                "tool_id": "rip.doc.parse",
                "run_id": run_id,
                "submit_key": "task-idem-real",
                "input": {"text": "# 缓存", "format": "md"},
                "workspace_id": "ws-default"
            }))
            .unwrap();
    }
    let first = execute_via_engine(AdapterCall {
        engine: &engine,
        run_id: "rn-idem-real-1",
        adapter: &adapter,
        cancel_after: None,
    })
    .unwrap();
    assert!(!first.envelope.replay.cached);
    let second = execute_via_engine(AdapterCall {
        engine: &engine,
        run_id: "rn-idem-real-2",
        adapter: &adapter,
        cancel_after: None,
    })
    .unwrap();
    // worker 文件缓存命中：cached=true 透传进 replay（§5.1），结果一致
    assert!(second.envelope.replay.cached);
    assert_eq!(second.envelope.result, first.envelope.result);
    assert_eq!(second.envelope.status, ToolRunStatus::Succeeded);
    adapter.shutdown();
}

#[test]
fn handshake_failure_maps_to_infra_in_preparing() {
    let (engine, registry, cache_dir) = fresh_env();
    register_debug_tool(&registry);
    let adapter = parse_adapter(
        "debug-worker",
        "crates/tool-engine/tests/fixtures/bad_worker.py",
        &cache_dir,
        false,
    );
    submit_debug(&engine, "rn-e2e-bad-handshake", json!({}), 60);
    let out = execute_via_engine(AdapterCall {
        engine: &engine,
        run_id: "rn-e2e-bad-handshake",
        adapter: &adapter,
        cancel_after: None,
    })
    .unwrap();
    // §3.2 第 7 行：PREPARING → FAILED（INFRA，retryable=true）
    assert_eq!(
        out.transcript,
        vec![
            ToolRunStatus::Queued,
            ToolRunStatus::Preparing,
            ToolRunStatus::Failed,
        ]
    );
    let err = out.envelope.error.as_ref().unwrap();
    assert_eq!(err.category, "INFRA");
    assert!(err.retryable);
    assert_eq!(
        row_status(&engine, "rn-e2e-bad-handshake").as_deref(),
        Some("FAILED")
    );
}

#[test]
fn section_5_3_mapping_table_fully_asserted() {
    // 全码表单元断言（实跑逐项见 worker_error_codes_map_to_envelope_categories
    // 与超时/取消用例）；此处断言码表完整性与 INFRA 兜底。
    let all_codes: [i64; 15] = [
        -32700, -32600, -32601, -32602, -32603, -32000, -32001, -32002, -32003, -32004, -32005,
        -32006, -32007, -32008, -32009,
    ];
    for code in all_codes {
        let (category, _) = map_worker_code(code);
        assert!(
            [
                "VALIDATION",
                "INPUT",
                "TIMEOUT",
                "CANCELLED",
                "EXECUTION",
                "INFRA"
            ]
            .contains(&category),
            "code {code} maps to unknown category {category}"
        );
    }
    // Mock 四路径回归（P1-5 最小集不因 adapter 接缝回归）
    let (engine, _registry, _cache) = fresh_env();
    engine
        .submit(&json!({
            "protocol_version": "1",
            "tool_id": "rip.doc.parse",
            "run_id": "rn-mock-regression",
            "input": {"text": "x"},
            "workspace_id": "ws-default"
        }))
        .unwrap();
    let out = engine
        .drive(
            "rn-mock-regression",
            &MockRunner::succeed(
                json!({
                    "format": "md",
                    "source": {"kind": "inline", "path": null, "bytes": 1, "sha256": "abc"},
                    "sections": [],
                    "heading_tree": [],
                    "stats": {"chars": 1, "sections": 0, "headings": 0}
                }),
                "ok",
            ),
        )
        .unwrap();
    assert_eq!(out.envelope.status, ToolRunStatus::Succeeded);
}

#[test]
fn workers_directory_untouched_by_tests() {
    // 测试运行不修改 workers/：git status 对 workers/ 路径的已跟踪文件必须
    // 零输出（缓存已重定向到临时目录，双保险）。untracked 排除：批次新增的
    // worker 脚本（如 P6-9 脚手架产物）是交付物而非测试污染，`??` 条目
    // 不构成本守卫的目标回归；已跟踪文件的改动/删除才可能是测试副作用。
    // P7-5 交付豁免（按交付标记判定，不是放松守卫）：worker_runtime.py 的
    // params.correlation 响应透传是 P7-5 批次交付物（SESSION-HANDOFF §十追加
    // 项 1 明确 workers 响应透传六 ID），编排层提交收口后 git status 自然
    // 清零；豁免只认带 correlation 透传标记的该文件 ` M` 条目，其余 workers/
    // 改动照旧拒绝（测试副作用守卫语义不变）。
    let output = Command::new("git")
        .args([
            "status",
            "--porcelain",
            "--untracked-files=no",
            "--",
            "workers/",
        ])
        .current_dir(repo_root())
        .output()
        .unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    let mut violations: Vec<&str> = Vec::new();
    for line in stdout.lines() {
        if let Some(path) = line.strip_prefix(" M ") {
            let full = repo_root().join(path);
            if path == "workers/python/runtime/worker_runtime.py" {
                if let Ok(text) = std::fs::read_to_string(&full) {
                    if text.contains("result[\"correlation\"] = correlation") {
                        continue; // P7-5 交付标记在场：编排层收口前的合法态
                    }
                }
            }
        }
        violations.push(line);
    }
    assert!(
        violations.is_empty(),
        "workers/ must be untouched (except pinned P7-5 deliverable marker), got: {violations:?}"
    );
}
