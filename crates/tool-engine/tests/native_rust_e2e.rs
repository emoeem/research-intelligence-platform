//! RT-1 native_rust e2e 测试载具（RT-1-TASK §5 八项判据逐项；S6）。
//!
//! 判据对照（每项的测试函数名列于断言）：
//! 1. allowlist 正例 → [`allowlist_positive_run_succeeds`]；
//! 2. allowlist 负例（sha 不符/缺失/越白名单）→
//!    [`rejects_sha_mismatch`]/[`rejects_missing_binary`]/[`rejects_subcommand_not_in_whitelist`]；
//! 3. 任意路径拒绝 → [`rejects_relative_binary`]/[`rejects_input_path_escape`]/[`source_has_no_shell_entry`]；
//! 4. env 泄漏断言 → [`env_whitelist_exact_set`]（fixture 回显环境）；
//! 5. 超时击杀 → [`timeout_terminates_group_and_kills_grandchild`]；
//! 6. 取消 → [`cancel_partial_output_not_delivered`]；
//! 7. 输出帽 → [`read_side_cap_terminates_group`]/[`result_side_truncate_registers_artifact`]；
//! 8. 统一信封 → 各路径内 [`assert_unified_envelope`]（envelope::validate +
//!    transcript 与 tool_run 行一致 + runtime_type="native_rust"）。
//!
//! 门禁：`--test-threads=1`（收口命令口径）；fixture 与 sha256 一并入库
//! （fixtures/native_rust/env_echo.sh，setup 时实测核对钉值，测试自持）。

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use std::thread;
use std::time::{Duration, Instant};

use database::{migrate, DbPath, Pool, PoolConfig};
use domain::ToolRunStatus;
use serde_json::{json, Value};
use tool_engine::adapter::{
    execute_native_rust_via_engine, NativeRustAdapter, NativeRustConfig, ResourceEnforcement,
};
use tool_engine::capability::{
    CapabilityId, CapabilityProvider, InvalidationEvent, ProbeBudget, ProbeLevel, ProbeOutcome,
    SeedDocument, StaticCatalogProvider,
};
use tool_engine::envelope;
use tool_engine::Engine;
use tool_registry::{Options, Registry};

/// fixture 钉值（与入库文件实测一致；setup 核对防 fixture 被改后钉值漂移）。
const FIXTURE_SHA256: &str = "282af314ee118066dc64b6ccb59146e094287776a7053ced8873077241d73aa3";

static SEQ: AtomicUsize = AtomicUsize::new(0);

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root")
}

fn fixture_path() -> PathBuf {
    repo_root().join("crates/tool-engine/tests/fixtures/native_rust/env_echo.sh")
}

/// setup 核对：fixture 钉值与入库文件实测一致（测试自持的第一道闸）。
fn checked_fixture() -> PathBuf {
    let path = fixture_path();
    let actual = tool_registry::hash::sha256_hex(&std::fs::read(&path).expect("fixture read"));
    assert_eq!(
        actual, FIXTURE_SHA256,
        "fixture sha256 drifted; update FIXTURE_SHA256 with the pinned file together"
    );
    path
}

fn fresh_engine(manifest: &Value) -> Engine {
    let pool = Pool::open(PoolConfig {
        path: DbPath::Memory,
        readonly: false,
        max_connections: 1,
        busy_timeout: Duration::from_secs(5),
    })
    .unwrap();
    migrate(&pool, &repo_root().join("migrations")).unwrap();
    let registry = Registry::open(pool.clone()).with_options(Options {
        allow_placeholder_digest: false,
    });
    registry.register(manifest).expect("manifest registers");
    Engine::open(pool, registry)
}

/// 标准 manifest：fixture + 形态 B（subcommands 白名单 + argv 模板 + Manifest env）。
fn base_manifest(fixture: &Path, sha: &str) -> Value {
    json!({
        "schema_version": "1",
        "id": "rip.rt1.fixture",
        "name": "RT-1 Fixture Tool",
        "version": "0.1.0",
        "kind": "tool",
        "family": "utility",
        "description": "RT-1 native_rust e2e 受控探针",
        "runtime": {
            "type": "native_rust",
            "binary": fixture.display().to_string(),
            "binary_sha256": sha,
            "subcommands": ["env", "spawn", "slow", "big", "partial", "echo"],
            "argv_template": ["{{out_dir}}", "{{input_path}}"],
            "env": {"RIP_RT1_CUSTOM": "workspace-fact"}
        },
        "input_schema": {
            "type": "object",
            "additionalProperties": false,
            "required": ["subcommand", "input_path"],
            "properties": {
                "subcommand": {"type": "string", "enum": ["env", "spawn", "slow", "big", "partial", "echo"]},
                "input_path": {"type": "string", "minLength": 1},
                "input_content": {"type": "string"}
            }
        },
        "output_schema": {"type": "object"},
        "resources": {"cpu": "1", "memory": "256Mi", "timeout_seconds": 60, "max_output_bytes": 4194304},
        "network": {"mode": "none"},
        "secrets": [],
        "permission_level": "T0",
        "idempotent": false,
        "health": {"probe": {"type": "none"}, "on_run_recover": false},
        "provenance": {
            "origin": "rip-native",
            "license": "repo-internal",
            "dependencies": [],
            "tested": "RT-1 native_rust_e2e.rs（测试自持 fixture）"
        }
    })
}

fn workspace_root(tag: &str) -> PathBuf {
    let seq = SEQ.fetch_add(1, Ordering::SeqCst);
    let ws = std::env::temp_dir().join(format!("rt1-e2e-{tag}-{}-{seq}", std::process::id()));
    let _ = std::fs::remove_dir_all(&ws);
    std::fs::create_dir_all(&ws).unwrap();
    ws
}

fn adapter_for(ws_root: &Path) -> NativeRustAdapter {
    NativeRustAdapter::new(NativeRustConfig {
        workspace_root: ws_root.to_path_buf(),
        resource_enforcement: ResourceEnforcement::Off,
        ..NativeRustConfig::default()
    })
}

fn submit(engine: &Engine, run_id: &str, input: Value, timeout: u64, max_output: u64) {
    engine
        .submit(&json!({
            "protocol_version": "1",
            "tool_id": "rip.rt1.fixture",
            "run_id": run_id,
            "submit_key": format!("rt1-{run_id}"),
            "input": input,
            "workspace_id": "ws-rt1",
            "budget": {"timeout_seconds": timeout, "max_output_bytes": max_output},
            "result_schema_version": "1"
        }))
        .expect("submit accepted");
}

/// 判据 8：统一信封（validate + transcript + tool_run 行 + runtime_type）。
fn assert_unified_envelope(engine: &Engine, run_id: &str, outcome: &tool_engine::DriveOutcome) {
    envelope::validate(&outcome.envelope).expect("envelope invariant");
    assert_eq!(
        outcome.envelope.environment.runtime_type, "native_rust",
        "runtime_type 写入 environment"
    );
    // transcript 与 tool_run 行一致（终点）。
    let row_status = engine.run_status(run_id).unwrap().expect("row exists");
    let transcript_end = *outcome.transcript.last().unwrap();
    assert_eq!(row_status, transcript_end, "tool_run 行 == transcript 终点");
    assert_eq!(outcome.envelope.status, transcript_end);
    // transcript 形状：QUEUED → PREPARING → (RUNNING) → 终态。
    assert_eq!(outcome.transcript.first(), Some(&ToolRunStatus::Queued));
    assert_eq!(outcome.transcript[1], ToolRunStatus::Preparing);
}

fn parse_env(data: &Value) -> Vec<(String, String)> {
    data["env"]
        .as_array()
        .expect("data.env array")
        .iter()
        .map(|kv| {
            let s = kv.as_str().expect("K=V string");
            let (k, v) = s.split_once('=').unwrap_or((s, ""));
            (k.to_string(), v.to_string())
        })
        .collect()
}

/// 判据 1 + 4：钉值命中 + 白名单子命令命中 → SUCCEEDED，stdout 帧解析为
/// 结构化结果；子进程环境 = 显式白名单集合（env_clear 效果实证）。
#[test]
fn native_rust_allowlist_positive_run_succeeds_and_env_is_whitelist_exact() {
    let fixture = checked_fixture();
    let manifest = base_manifest(&fixture, FIXTURE_SHA256);
    let engine = fresh_engine(&manifest);
    let ws_root = workspace_root("ok");
    let adapter = adapter_for(&ws_root);
    submit(
        &engine,
        "rn-rt1-ok",
        json!({
            "subcommand": "env",
            "input_path": "probe.txt",
            "input_content": "hello-rt1"
        }),
        30,
        1_048_576,
    );
    let outcome = execute_native_rust_via_engine(tool_engine::adapter::NativeRustCall {
        engine: &engine,
        run_id: "rn-rt1-ok",
        adapter: &adapter,
        manifest: &manifest,
        capability: None,
    })
    .expect("drive ok");
    assert_unified_envelope(&engine, "rn-rt1-ok", &outcome);
    assert_eq!(outcome.envelope.status, ToolRunStatus::Succeeded);
    assert_eq!(outcome.transcript.len(), 4, "[Q,P,RUNNING,SUCCEEDED]");
    let data = outcome.envelope.result.as_ref().expect("structured result");
    // 输入暂存链路：in/probe.txt 由 Adapter 落盘并以绝对路径进 argv。
    assert_eq!(data["input_file_bytes"], json!(9), "staged input bytes");
    assert_eq!(data["input_file_head"], json!("hello-rt1"));
    // 判据 4：环境 = 显式白名单集合，零继承。bash 启动自设 PWD/SHLVL/_
    // （结构性非继承，env_clear 无法阻止 bash 自身注入）；判据面 = 白名单
    // 键值精确 + 测试进程（宿主会话）的全部环境键零透传。
    let env = parse_env(data);
    let keys: Vec<&str> = env.iter().map(|(k, _)| k.as_str()).collect();
    for key in keys {
        assert!(
            matches!(
                key,
                "LANG" | "PATH" | "TMPDIR" | "RIP_RT1_CUSTOM" | "PWD" | "SHLVL" | "_"
            ),
            "环境键 {key:?} 不在白名单集合内（继承泄漏）"
        );
    }
    // 宿主会话（测试进程）环境键零透传（env_clear 效果的完备实证）：宿主
    // 任意非白名单键（HOME/CARGO_*/ANDROID_HOME/SSH_AUTH_SOCK 等）不得出现
    // 在子进程环境中。
    let child_keys: Vec<&str> = env.iter().map(|(k, _)| k.as_str()).collect();
    for (key, _) in std::env::vars() {
        if matches!(
            key.as_str(),
            "PATH" | "TMPDIR" | "LANG" | "SHLVL" | "PWD" | "_"
        ) {
            continue; // 白名单键或 bash 重算键（bash 自设非继承），另行精确断言
        }
        assert!(
            !child_keys.contains(&key.as_str()),
            "宿主环境键 {key:?} 透传进了子进程（env_clear 失效）"
        );
    }
    assert_eq!(
        env.iter().find(|(k, _)| k == "PWD").unwrap().1,
        ws_root.join("rn-rt1-ok").display().to_string(),
        "cwd 固定任务目录（工作区边界）"
    );
    let get = |k: &str| env.iter().find(|(key, _)| key == k).map(|(_, v)| v.clone());
    assert_eq!(
        get("PATH").unwrap(),
        fixture.parent().unwrap().display().to_string(),
        "PATH = 钉值二进制所在目录最小集（D4）"
    );
    assert_eq!(get("LANG").unwrap(), "C.UTF-8");
    assert!(
        get("TMPDIR").unwrap().ends_with("/tmp"),
        "TMPDIR = 任务 tmp 目录"
    );
    assert_eq!(get("RIP_RT1_CUSTOM").unwrap(), "workspace-fact");
    // 凭据类变量零继承（宿主会话实际持有 HOME/CARGO_* 等，env_clear 实证）。
    for hostile in [
        "SSH_AUTH_SOCK",
        "HTTP_PROXY",
        "HTTPS_PROXY",
        "HOME",
        "DBUS_SESSION_BUS_ADDRESS",
        "CARGO_HOME",
    ] {
        assert!(
            get(hostile).is_none(),
            "{hostile} 不得落入子进程（env_clear）"
        );
    }
    let _ = std::fs::remove_dir_all(&ws_root);
}

/// 判据 2：sha256 与钉值不符 → PREPARING 拒绝（POLICY_DENIED），零子进程拉起。
#[test]
fn native_rust_rejects_sha_mismatch() {
    let fixture = checked_fixture();
    let bad_sha = "0".repeat(64);
    let manifest = base_manifest(&fixture, &bad_sha);
    let engine = fresh_engine(&manifest);
    let ws_root = workspace_root("sha");
    let adapter = adapter_for(&ws_root);
    submit(
        &engine,
        "rn-rt1-sha",
        json!({"subcommand": "env", "input_path": "probe.txt"}),
        30,
        1_048_576,
    );
    let started = Instant::now();
    let outcome = execute_native_rust_via_engine(tool_engine::adapter::NativeRustCall {
        engine: &engine,
        run_id: "rn-rt1-sha",
        adapter: &adapter,
        manifest: &manifest,
        capability: None,
    })
    .expect("drive ok");
    assert_unified_envelope(&engine, "rn-rt1-sha", &outcome);
    assert_eq!(outcome.envelope.status, ToolRunStatus::Failed);
    assert_eq!(
        outcome.transcript,
        vec![
            ToolRunStatus::Queued,
            ToolRunStatus::Preparing,
            ToolRunStatus::Failed
        ]
    );
    let err = outcome.envelope.error.as_ref().unwrap();
    assert_eq!(
        err.category, "POLICY_DENIED",
        "D7：identity 不符 → POLICY_DENIED"
    );
    assert_eq!(err.code, json!("NATIVE_IDENTITY_MISMATCH"));
    assert!(err.message.contains("sha256 mismatch"));
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "PREPARING 即拒，无进程等待"
    );
    // 零子进程拉起的可观察面：工作区未创建（校验先于工作区准备）。
    assert!(
        !ws_root.join("rn-rt1-sha").exists(),
        "拒绝路径零工作区（零 spawn 的结构性佐证）"
    );
    let _ = std::fs::remove_dir_all(&ws_root);
}

/// 判据 2：binary 文件缺失 → PREPARING 拒绝（DEPENDENCY/NATIVE_BINARY_MISSING）。
#[test]
fn native_rust_rejects_missing_binary() {
    let manifest = base_manifest(Path::new("/nonexistent/rt1/missing-bin"), &"a".repeat(64));
    let engine = fresh_engine(&manifest);
    let ws_root = workspace_root("missing");
    let adapter = adapter_for(&ws_root);
    submit(
        &engine,
        "rn-rt1-missing",
        json!({"subcommand": "env", "input_path": "probe.txt"}),
        30,
        1_048_576,
    );
    let outcome = execute_native_rust_via_engine(tool_engine::adapter::NativeRustCall {
        engine: &engine,
        run_id: "rn-rt1-missing",
        adapter: &adapter,
        manifest: &manifest,
        capability: None,
    })
    .expect("drive ok");
    assert_unified_envelope(&engine, "rn-rt1-missing", &outcome);
    assert_eq!(outcome.envelope.status, ToolRunStatus::Failed);
    let err = outcome.envelope.error.as_ref().unwrap();
    assert_eq!(err.category, "DEPENDENCY", "D7：missing → DEPENDENCY");
    assert_eq!(err.code, json!("NATIVE_BINARY_MISSING"));
    assert_eq!(
        outcome.transcript,
        vec![
            ToolRunStatus::Queued,
            ToolRunStatus::Preparing,
            ToolRunStatus::Failed
        ]
    );
    let _ = std::fs::remove_dir_all(&ws_root);
}

/// 判据 2：子命令不在白名单 → PREPARING 拒绝（POLICY_DENIED），零子进程拉起。
#[test]
fn native_rust_rejects_subcommand_not_in_whitelist() {
    let fixture = checked_fixture();
    let mut manifest = base_manifest(&fixture, FIXTURE_SHA256);
    manifest["runtime"]["subcommands"] = json!(["env", "slow"]);
    manifest["input_schema"]["properties"]["subcommand"]["enum"] =
        json!(["env", "slow", "forbidden"]);
    let engine = fresh_engine(&manifest);
    let ws_root = workspace_root("sc");
    let adapter = adapter_for(&ws_root);
    submit(
        &engine,
        "rn-rt1-sc",
        json!({"subcommand": "forbidden", "input_path": "probe.txt"}),
        30,
        1_048_576,
    );
    let outcome = execute_native_rust_via_engine(tool_engine::adapter::NativeRustCall {
        engine: &engine,
        run_id: "rn-rt1-sc",
        adapter: &adapter,
        manifest: &manifest,
        capability: None,
    })
    .expect("drive ok");
    assert_unified_envelope(&engine, "rn-rt1-sc", &outcome);
    assert_eq!(outcome.envelope.status, ToolRunStatus::Failed);
    let err = outcome.envelope.error.as_ref().unwrap();
    assert_eq!(err.category, "POLICY_DENIED");
    assert_eq!(err.code, json!("NATIVE_SUBCOMMAND_NOT_ALLOWED"));
    assert!(err
        .message
        .contains("not in the runtime.subcommands whitelist"));
    assert_eq!(
        outcome.transcript,
        vec![
            ToolRunStatus::Queued,
            ToolRunStatus::Preparing,
            ToolRunStatus::Failed
        ]
    );
    assert!(
        !ws_root.join("rn-rt1-sc").exists(),
        "零 spawn（工作区未创建）"
    );
    let _ = std::fs::remove_dir_all(&ws_root);
}

/// 判据 3：派发面拒绝 PATH 名称解析（相对 binary；登记面负例在 sk3）。
#[test]
fn native_rust_rejects_relative_binary() {
    // 登记面负例（相对路径 + 钉值字段在场）已在 sk3_tool_manifests.rs 落；
    // 此处验派发面兜底：登记干净 manifest，派发传相对路径副本。
    let fixture = checked_fixture();
    let manifest = base_manifest(&fixture, FIXTURE_SHA256);
    let engine = fresh_engine(&manifest);
    let mut dispatch_manifest = manifest.clone();
    dispatch_manifest["runtime"]["binary"] = json!("tests/fixtures/native_rust/env_echo.sh");
    let ws_root = workspace_root("rel");
    let adapter = adapter_for(&ws_root);
    submit(
        &engine,
        "rn-rt1-rel",
        json!({"subcommand": "env", "input_path": "probe.txt"}),
        30,
        1_048_576,
    );
    let outcome = execute_native_rust_via_engine(tool_engine::adapter::NativeRustCall {
        engine: &engine,
        run_id: "rn-rt1-rel",
        adapter: &adapter,
        manifest: &dispatch_manifest,
        capability: None,
    })
    .expect("drive ok");
    assert_unified_envelope(&engine, "rn-rt1-rel", &outcome);
    let err = outcome.envelope.error.as_ref().unwrap();
    assert_eq!(err.category, "POLICY_DENIED");
    assert!(err.message.contains("must be an absolute path"));
    let _ = std::fs::remove_dir_all(&ws_root);
}

/// 判据 3：argv 模板 {{input_path}} 注入 `../` → 派发面前缀校验拒绝。
#[test]
fn native_rust_rejects_input_path_escape() {
    let fixture = checked_fixture();
    let manifest = base_manifest(&fixture, FIXTURE_SHA256);
    let engine = fresh_engine(&manifest);
    let ws_root = workspace_root("escape");
    let adapter = adapter_for(&ws_root);
    submit(
        &engine,
        "rn-rt1-escape",
        json!({"subcommand": "env", "input_path": "../escape.txt"}),
        30,
        1_048_576,
    );
    let outcome = execute_native_rust_via_engine(tool_engine::adapter::NativeRustCall {
        engine: &engine,
        run_id: "rn-rt1-escape",
        adapter: &adapter,
        manifest: &manifest,
        capability: None,
    })
    .expect("drive ok");
    assert_unified_envelope(&engine, "rn-rt1-escape", &outcome);
    assert_eq!(outcome.envelope.status, ToolRunStatus::Failed);
    let err = outcome.envelope.error.as_ref().unwrap();
    assert_eq!(err.category, "POLICY_DENIED");
    assert_eq!(err.code, json!("NATIVE_ARGV_TEMPLATE_DENIED"));
    assert!(err.message.contains("outside the task in/ directory"));
    // in/ 前缀强制的正面：绝对路径注入同样拒绝。
    submit(
        &engine,
        "rn-rt1-abs",
        json!({"subcommand": "env", "input_path": "/etc/passwd"}),
        30,
        1_048_576,
    );
    let outcome = execute_native_rust_via_engine(tool_engine::adapter::NativeRustCall {
        engine: &engine,
        run_id: "rn-rt1-abs",
        adapter: &adapter,
        manifest: &manifest,
        capability: None,
    })
    .expect("drive ok");
    let err = outcome.envelope.error.as_ref().unwrap();
    assert_eq!(err.code, json!("NATIVE_ARGV_TEMPLATE_DENIED"));
    let _ = std::fs::remove_dir_all(&ws_root);
}

/// 判据 3（代码层 grep 断言）：`sh -c`/`bash -c` 形态在 adapter 源码层无构造入口。
#[test]
fn native_rust_source_has_no_shell_entry() {
    let src =
        std::fs::read_to_string(repo_root().join("crates/tool-engine/src/adapter/native_rust.rs"))
            .expect("adapter source");
    // 判据 3 口径为「代码层无构造入口」：剥注释（// 及行内 // 之后）再查
    // 禁词（模块文档引用禁词属说明文字，不构成构造入口）。
    let code: String = src
        .lines()
        .map(|l| l.split("//").next().unwrap_or(l))
        .collect::<Vec<_>>()
        .join("\n");
    for banned in [
        "sh -c",
        "bash -c",
        "/bin/sh",
        "/bin/bash",
        "cmd /c",
        "shell_words",
        "Command::new(\"sh\")",
        "Command::new(\"bash\")",
        "\"-c\"",
    ] {
        assert!(
            !code.contains(banned),
            "adapter 源码不得含 shell 构造入口: {banned:?}"
        );
    }
}

/// 判据 5：超时 → killpg TERM → 2s 窗口 → killpg KILL；孙进程零残留
/// （fixture 派生 sleep 孙进程并忽略 TERM，实测 KILL 路径 + /proc 复核）。
#[test]
fn native_rust_timeout_terminates_group_and_kills_grandchild() {
    let fixture = checked_fixture();
    let manifest = base_manifest(&fixture, FIXTURE_SHA256);
    let engine = fresh_engine(&manifest);
    let ws_root = workspace_root("timeout");
    let adapter = adapter_for(&ws_root);
    submit(
        &engine,
        "rn-rt1-timeout",
        json!({"subcommand": "spawn", "input_path": "ignored.txt"}),
        1,
        1_048_576,
    );
    let started = Instant::now();
    let outcome = execute_native_rust_via_engine(tool_engine::adapter::NativeRustCall {
        engine: &engine,
        run_id: "rn-rt1-timeout",
        adapter: &adapter,
        manifest: &manifest,
        capability: None,
    })
    .expect("drive ok");
    assert_unified_envelope(&engine, "rn-rt1-timeout", &outcome);
    assert_eq!(outcome.envelope.status, ToolRunStatus::TimedOut);
    assert_eq!(outcome.transcript.len(), 4);
    let err = outcome.envelope.error.as_ref().unwrap();
    assert_eq!(err.category, "TIMEOUT");
    assert_eq!(err.code, json!(-32002), "与 worker/podman 同码（D7）");
    assert!(err.message.contains("watchdog"));
    // TERM 被忽略 → KILL 路径实测（预算 1s + TERM 窗口 2s + 轮询粒度）。
    assert!(
        started.elapsed() >= Duration::from_millis(2500),
        "TERM→KILL 窗口实测"
    );
    // 部分产物照常登记（out/child.pid 为 partial 语义载体）。
    let pid_file = ws_root.join("rn-rt1-timeout/out/child.pid");
    let pid = std::fs::read_to_string(&pid_file)
        .expect("孙进程 pid 已登记（partial collect）")
        .trim()
        .to_string();
    // 进程组零残留：孙进程 /proc 条目在击杀后消失。
    let proc = PathBuf::from("/proc").join(&pid);
    let deadline = Instant::now() + Duration::from_secs(5);
    while proc.exists() {
        assert!(Instant::now() < deadline, "孙进程 {pid} 残留：进程组未回收");
        thread::sleep(Duration::from_millis(100));
    }
    let _ = std::fs::remove_dir_all(&ws_root);
}

/// 判据 6：cancel_requested_at 轮询命中 → CANCELLED 终态信封（-32004）；
/// 取消窗口内已到达的部分 stdout 帧不作为结果交付（不伪装成功）。
#[test]
fn native_rust_cancel_partial_output_not_delivered() {
    let fixture = checked_fixture();
    let manifest = base_manifest(&fixture, FIXTURE_SHA256);
    let engine = fresh_engine(&manifest);
    let ws_root = workspace_root("cancel");
    let adapter = adapter_for(&ws_root);
    submit(
        &engine,
        "rn-rt1-cancel",
        json!({"subcommand": "partial", "input_path": "ignored.txt"}),
        30,
        1_048_576,
    );
    // RUNNING 期取消触发（D-TC-04）：独立线程 400ms 后写 cancel_requested_at。
    let cancel_engine = engine.clone();
    let handle = thread::spawn(move || {
        thread::sleep(Duration::from_millis(400));
        assert!(cancel_engine.request_cancel("rn-rt1-cancel").unwrap());
    });
    let outcome = execute_native_rust_via_engine(tool_engine::adapter::NativeRustCall {
        engine: &engine,
        run_id: "rn-rt1-cancel",
        adapter: &adapter,
        manifest: &manifest,
        capability: None,
    })
    .expect("drive ok");
    handle.join().expect("cancel thread");
    assert_unified_envelope(&engine, "rn-rt1-cancel", &outcome);
    assert_eq!(outcome.envelope.status, ToolRunStatus::Cancelled);
    let err = outcome.envelope.error.as_ref().unwrap();
    assert_eq!(err.category, "CANCELLED");
    assert_eq!(
        err.code,
        json!(-32004),
        "python_worker.rs:734-748 映射先例同码"
    );
    assert!(
        outcome.envelope.result.is_none(),
        "部分输出帧已到达 stdout，但不得伪装成 SUCCEEDED result"
    );
    // 部分产物按 partial 语义登记（stdout/stderr 留档）。
    let artifacts = &outcome.envelope.artifacts;
    assert!(
        artifacts.iter().any(|a| a.uri.ends_with("stdout.log")),
        "取消路径 stdout 留档 Artifact"
    );
    let _ = std::fs::remove_dir_all(&ws_root);
}

/// 判据 7b：流式读帽触顶 → 进程组终止 + 失败语义交付，不整读 OOM
/// （200KB 帧 vs 4KB 帽：只保留帽内字节， Artifact 对账）。
#[test]
fn native_rust_read_side_cap_terminates_group() {
    let fixture = checked_fixture();
    let manifest = base_manifest(&fixture, FIXTURE_SHA256);
    let engine = fresh_engine(&manifest);
    let ws_root = workspace_root("cap");
    let adapter = adapter_for(&ws_root);
    submit(
        &engine,
        "rn-rt1-cap",
        json!({"subcommand": "big", "input_path": "ignored.txt"}),
        30,
        4096,
    );
    let outcome = execute_native_rust_via_engine(tool_engine::adapter::NativeRustCall {
        engine: &engine,
        run_id: "rn-rt1-cap",
        adapter: &adapter,
        manifest: &manifest,
        capability: None,
    })
    .expect("drive ok");
    assert_unified_envelope(&engine, "rn-rt1-cap", &outcome);
    assert_eq!(outcome.envelope.status, ToolRunStatus::Failed);
    let err = outcome.envelope.error.as_ref().unwrap();
    assert_eq!(err.category, "EXECUTION");
    assert_eq!(err.code, json!("NATIVE_OUTPUT_CAP_EXCEEDED"));
    assert!(err.message.contains("exceeded read cap"));
    // 帽内字节留档（Artifact 对账：不整读 OOM 的证据面）。
    let kept = std::fs::read(ws_root.join("rn-rt1-cap/out/stdout.log")).unwrap();
    assert_eq!(
        kept.len() as u64,
        4096 + 64 * 1024,
        "读侧只保留 max_output_bytes + 64KiB 容差（D8.2a）"
    );
    let artifacts = &outcome.envelope.artifacts;
    let stdout_art = artifacts
        .iter()
        .find(|a| a.uri.ends_with("stdout.log"))
        .expect("stdout.log artifact 登记");
    assert_eq!(stdout_art.sha256, tool_registry::hash::sha256_hex(&kept));
    let _ = std::fs::remove_dir_all(&ws_root);
}

/// 判据 7a：结构化结果超 max_output_bytes → envelope::truncate 降级 +
/// Artifact 登记 + usage.truncated=true（podman_host.rs:902-917 同款）。
#[test]
fn native_rust_result_side_truncate_registers_artifact() {
    let fixture = checked_fixture();
    let mut manifest = base_manifest(&fixture, FIXTURE_SHA256);
    // echo 内容经 Manifest env 承载（fixture 从环境读，避免解析 stdin JSON）。
    let content = "y".repeat(100_000);
    manifest["runtime"]["env"] = json!({
        "RIP_RT1_CUSTOM": "workspace-fact",
        "RIP_RT1_ECHO_CONTENT": content
    });
    let engine = fresh_engine(&manifest);
    let ws_root = workspace_root("trunc");
    let adapter = adapter_for(&ws_root);
    submit(
        &engine,
        "rn-rt1-trunc",
        json!({"subcommand": "echo", "input_path": "ignored.txt"}),
        30,
        65536,
    );
    let outcome = execute_native_rust_via_engine(tool_engine::adapter::NativeRustCall {
        engine: &engine,
        run_id: "rn-rt1-trunc",
        adapter: &adapter,
        manifest: &manifest,
        capability: None,
    })
    .expect("drive ok");
    assert_unified_envelope(&engine, "rn-rt1-trunc", &outcome);
    assert_eq!(outcome.envelope.status, ToolRunStatus::Succeeded);
    assert!(outcome.envelope.usage.truncated, "usage.truncated=true");
    let notice = outcome.envelope.result.as_ref().unwrap();
    assert_eq!(notice["truncated"], json!(true));
    assert!(notice["original_bytes"].as_u64().unwrap() > 65536);
    // 截断原始字节 Artifact（podman_host.rs:902-917 同款逻辑 URI）：
    // 条目 size/sha256 与 notice 对账；实体字节在 out/stdout.log（原始帧）。
    let artifact = outcome
        .envelope
        .artifacts
        .iter()
        .find(|a| a.artifact_id == notice["artifact_id"])
        .expect("截断原始字节 Artifact");
    assert_eq!(
        artifact.size_bytes,
        notice["original_bytes"].as_u64().unwrap()
    );
    assert_eq!(artifact.sha256.len(), 64);
    let stdout_log =
        std::fs::read(ws_root.join("rn-rt1-trunc/out/stdout.log")).expect("原始帧留档");
    assert!(
        stdout_log
            .windows(content.len())
            .any(|w| w == content.as_bytes()),
        "原始输出完整留档（非静默截断）"
    );
    let _ = std::fs::remove_dir_all(&ws_root);
}

/// 判据 2 扩展（capability 面）：capability_ref 目录校验——available+fresh
/// 放行；missing → DEPENDENCY 拒绝。
#[test]
fn native_rust_capability_ref_gate_positive_and_missing() {
    let fixture = checked_fixture();
    let actual = FIXTURE_SHA256;
    // 临时种子：fixture 注册为 test.fixturebin 能力（实测前缀 = 钉值前缀）。
    let doc: SeedDocument = serde_json::from_value(json!({
        "schema_version": "1",
        "source": "native_rust_e2e",
        "entries": [{
            "capability_id": "test.fixturebin",
            "provider": "rt1-fixture",
            "executable_path": fixture.display().to_string(),
            "version": {"normalized": "1.0", "raw": "1.0"},
            "binary_identity": {
                "sha256_prefix": &actual[..16],
                "file_type": "shell-script",
                "owner_mode": "emo:emo 755",
                "wrapper_target": null
            },
            "package_provenance": "unknown",
            "capabilities": [],
            "status": "available",
            "probe_timestamp": "2026-11-19",
            "constraints": [],
            "security_profile": {
                "allowed_call_modes": ["restricted_native_subprocess"],
                "max_input_risk": "trusted_low_risk",
                "notes": []
            },
            "search_name": null
        }]
    }))
    .unwrap();
    let mut provider = StaticCatalogProvider::from_seed_document(doc).unwrap();
    let id = CapabilityId::new("test.fixturebin").unwrap();
    // 先探（provider.get 需要 fresh+available；probe 实测全量 sha）。
    match provider
        .probe(&id, ProbeLevel::L0Static, &ProbeBudget::default())
        .expect("probe")
    {
        ProbeOutcome::Entry(_) => {}
        other => panic!("expect entry: {other:?}"),
    }

    // 正例：目录校验通过 → 派发放行。
    let mut manifest = base_manifest(&fixture, FIXTURE_SHA256);
    manifest["runtime"]["capability_ref"] = json!("test.fixturebin");
    let engine = fresh_engine(&manifest);
    let ws_root = workspace_root("cap-ok");
    let adapter = adapter_for(&ws_root);
    submit(
        &engine,
        "rn-rt1-cap-ok",
        json!({"subcommand": "env", "input_path": "probe.txt", "input_content": "via-capability"}),
        30,
        1_048_576,
    );
    let outcome = execute_native_rust_via_engine(tool_engine::adapter::NativeRustCall {
        engine: &engine,
        run_id: "rn-rt1-cap-ok",
        adapter: &adapter,
        manifest: &manifest,
        capability: Some(&provider),
    })
    .expect("drive ok");
    assert_unified_envelope(&engine, "rn-rt1-cap-ok", &outcome);
    assert_eq!(outcome.envelope.status, ToolRunStatus::Succeeded);
    assert_eq!(
        outcome.envelope.result.as_ref().unwrap()["input_file_head"],
        json!("via-capability")
    );
    let _ = std::fs::remove_dir_all(&ws_root);

    // 负例：目录无该能力 → DEPENDENCY（F §6:228 None -> 派发拒绝）。
    let manifest_missing = base_manifest(&fixture, FIXTURE_SHA256);
    let mut manifest_missing = manifest_missing;
    manifest_missing["runtime"]["capability_ref"] = json!("test.absent");
    let engine = fresh_engine(&manifest_missing);
    let ws_root = workspace_root("cap-miss");
    let adapter = adapter_for(&ws_root);
    submit(
        &engine,
        "rn-rt1-cap-miss",
        json!({"subcommand": "env", "input_path": "probe.txt"}),
        30,
        1_048_576,
    );
    let outcome = execute_native_rust_via_engine(tool_engine::adapter::NativeRustCall {
        engine: &engine,
        run_id: "rn-rt1-cap-miss",
        adapter: &adapter,
        manifest: &manifest_missing,
        capability: Some(&provider),
    })
    .expect("drive ok");
    assert_unified_envelope(&engine, "rn-rt1-cap-miss", &outcome);
    assert_eq!(outcome.envelope.status, ToolRunStatus::Failed);
    let err = outcome.envelope.error.as_ref().unwrap();
    assert_eq!(err.category, "DEPENDENCY");
    assert_eq!(err.code, json!("NATIVE_CAPABILITY_MISSING"));
    let _ = std::fs::remove_dir_all(&ws_root);

    // 负例（评审修订补测）：目录条目 stale（手动失效事件后）→ POLICY_DENIED
    // NATIVE_CAPABILITY_STALE——实现为 stale 早拒（fail-closed），与 F §6:227
    // 「refresh 后重判」的偏差登记见批次报告 §4。
    let stale_count = provider.invalidate(&InvalidationEvent::ManualInvalidate {
        capability_id: Some(id.clone()),
    });
    assert_eq!(stale_count, 1, "失效事件恰好标记 test.fixturebin 一条");
    let manifest_stale = base_manifest(&fixture, FIXTURE_SHA256);
    let mut manifest_stale = manifest_stale;
    manifest_stale["runtime"]["capability_ref"] = json!("test.fixturebin");
    let engine = fresh_engine(&manifest_stale);
    let ws_root = workspace_root("cap-stale");
    let adapter = adapter_for(&ws_root);
    submit(
        &engine,
        "rn-rt1-cap-stale",
        json!({"subcommand": "env", "input_path": "probe.txt"}),
        30,
        1_048_576,
    );
    let outcome = execute_native_rust_via_engine(tool_engine::adapter::NativeRustCall {
        engine: &engine,
        run_id: "rn-rt1-cap-stale",
        adapter: &adapter,
        manifest: &manifest_stale,
        capability: Some(&provider),
    })
    .expect("drive ok");
    assert_unified_envelope(&engine, "rn-rt1-cap-stale", &outcome);
    assert_eq!(outcome.envelope.status, ToolRunStatus::Failed);
    let err = outcome.envelope.error.as_ref().unwrap();
    assert_eq!(err.category, "POLICY_DENIED");
    assert_eq!(err.code, json!("NATIVE_CAPABILITY_STALE"));
    assert!(err.message.contains("stale"), "stale 原因入 message");
    assert!(outcome.transcript.len() == 3, "Q→P→FAILED，无 RUNNING");
    let _ = std::fs::remove_dir_all(&ws_root);
}

/// D4：Manifest secrets 非空 → 派发拒绝（native 档不支持 secret 注入）；
/// 非数组形状（对象/字符串）同拒——纵深兜底 fail-closed，无 fail-open 分支
/// （评审修订：原 `as_array().unwrap_or(true)` 把非数组形状当空集放行）。
#[test]
fn native_rust_rejects_manifest_with_secrets() {
    // 登记面先行拦截（rule 7 平台 secrets store 校验），派发面为纵深兜底：
    // 登记干净 manifest，派发传入带 secrets 的副本（Call 直接携带 manifest）。
    let fixture = checked_fixture();
    let manifest = base_manifest(&fixture, FIXTURE_SHA256);
    let engine = fresh_engine(&manifest);

    // 数组形态非空 + 非数组形状（对象 / 字符串）三类污染面逐一拒绝。
    let polluted = [
        json!([{"name": "API_TOKEN", "inject_as": "env", "required": true}]),
        json!({"API_TOKEN": "leak"}),
        json!("API_TOKEN=leak"),
    ];
    for (idx, secrets) in polluted.into_iter().enumerate() {
        let mut dispatch_manifest = manifest.clone();
        dispatch_manifest["secrets"] = secrets;
        let tag = format!("secrets-{idx}");
        let ws_root = workspace_root(&tag);
        let adapter = adapter_for(&ws_root);
        let run_id = format!("rn-rt1-secrets-{idx}");
        submit(
            &engine,
            &run_id,
            json!({"subcommand": "env", "input_path": "probe.txt"}),
            30,
            1_048_576,
        );
        let outcome = execute_native_rust_via_engine(tool_engine::adapter::NativeRustCall {
            engine: &engine,
            run_id: &run_id,
            adapter: &adapter,
            manifest: &dispatch_manifest,
            capability: None,
        })
        .expect("drive ok");
        assert_unified_envelope(&engine, &run_id, &outcome);
        let err = outcome.envelope.error.as_ref().unwrap();
        assert_eq!(err.category, "POLICY_DENIED");
        assert_eq!(err.code, json!("NATIVE_SECRETS_UNSUPPORTED"));
        // 零子进程拉起：PREPARING 拒绝即 FAILED，工作区不推进到 RUNNING。
        assert!(outcome.transcript.len() == 3, "Q→P→FAILED，无 RUNNING");
        let _ = std::fs::remove_dir_all(&ws_root);
    }
}

/// D8.4：资源强制 fail-closed——Auto 下两机制不可用（探测面注入不存在
/// 路径）→ PREPARING 拒绝，不裸跑；Off 仅测试载具显式声明。
#[test]
fn native_rust_resource_enforcement_unavailable_rejects_dispatch() {
    let fixture = checked_fixture();
    let manifest = base_manifest(&fixture, FIXTURE_SHA256);
    let engine = fresh_engine(&manifest);
    let ws_root = workspace_root("res");
    let adapter = NativeRustAdapter::new(NativeRustConfig {
        workspace_root: ws_root.clone(),
        resource_enforcement: ResourceEnforcement::Auto,
        systemd_run_bin: "/nonexistent/rt1/systemd-run".to_string(),
        prlimit_bin: "/nonexistent/rt1/prlimit".to_string(),
        ..NativeRustConfig::default()
    });
    submit(
        &engine,
        "rn-rt1-res",
        json!({"subcommand": "env", "input_path": "probe.txt"}),
        30,
        1_048_576,
    );
    let outcome = execute_native_rust_via_engine(tool_engine::adapter::NativeRustCall {
        engine: &engine,
        run_id: "rn-rt1-res",
        adapter: &adapter,
        manifest: &manifest,
        capability: None,
    })
    .expect("drive ok");
    assert_unified_envelope(&engine, "rn-rt1-res", &outcome);
    assert_eq!(outcome.envelope.status, ToolRunStatus::Failed);
    let err = outcome.envelope.error.as_ref().unwrap();
    assert_eq!(err.category, "INFRA", "机制不可用 = 平台面（retryable）");
    assert!(err.retryable);
    assert!(err.message.contains("refusing to dispatch unconfined"));
    let _ = std::fs::remove_dir_all(&ws_root);
}

/// D4 保留键语义（评审修订）：Manifest runtime.env 覆盖 PATH/TMPDIR/LANG
/// 保留白名单键 → PREPARING 拒绝（NATIVE_ENV_RESERVED_CONFLICT），
/// {"PATH": ...} 不能撞穿「PATH=钉值二进制所在目录最小集」结构性保证。
#[test]
fn native_rust_rejects_reserved_env_key_conflict() {
    let fixture = checked_fixture();
    let manifest = base_manifest(&fixture, FIXTURE_SHA256);
    let engine = fresh_engine(&manifest);
    let mut dispatch_manifest = manifest.clone();
    dispatch_manifest["runtime"]["env"] = json!({"PATH": "/tmp/attacker/bin"});
    let ws_root = workspace_root("reserved-env");
    let adapter = adapter_for(&ws_root);
    submit(
        &engine,
        "rn-rt1-reserved-env",
        json!({"subcommand": "env", "input_path": "probe.txt"}),
        30,
        1_048_576,
    );
    let outcome = execute_native_rust_via_engine(tool_engine::adapter::NativeRustCall {
        engine: &engine,
        run_id: "rn-rt1-reserved-env",
        adapter: &adapter,
        manifest: &dispatch_manifest,
        capability: None,
    })
    .expect("drive ok");
    assert_unified_envelope(&engine, "rn-rt1-reserved-env", &outcome);
    let err = outcome.envelope.error.as_ref().unwrap();
    assert_eq!(err.category, "POLICY_DENIED");
    assert_eq!(err.code, json!("NATIVE_ENV_RESERVED_CONFLICT"));
    assert!(outcome.transcript.len() == 3, "Q→P→FAILED，无 RUNNING");
    let _ = std::fs::remove_dir_all(&ws_root);
}

/// D8.4 Auto 全链正例（真实机制端到端，评审缺口补测——批次报告 §5 自认
/// Auto 全链此前从未端到端跑通，测试载具全部 Off）：生产默认 Auto + 真实
/// 探测面（默认 /usr/bin/systemd-run 与 /usr/bin/prlimit）→ 按与派发一致的
/// 清环境口径探测定案机制 → 包裹派发成功。本机（systemd 262 会话宿主）
/// systemd-run 探测因派发面 env 无 XDG_RUNTIME_DIR 失败 → 自然落 prlimit
/// 兜底（RLIMIT_CPU/RLIMIT_AS 包裹 bash fixture）；在 systemd-run 可用的
/// 宿主上同测试直接覆盖首选机制路径。断言机制无关：探测与派发同口径下
/// whichever mechanism wins，Auto 全链必须派发成功且信封统一。
#[test]
fn native_rust_auto_enforcement_real_chain_dispatches() {
    let fixture = checked_fixture();
    let manifest = base_manifest(&fixture, FIXTURE_SHA256);
    let engine = fresh_engine(&manifest);
    let ws_root = workspace_root("res-auto");
    let adapter = NativeRustAdapter::new(NativeRustConfig {
        workspace_root: ws_root.clone(),
        resource_enforcement: ResourceEnforcement::Auto,
        ..NativeRustConfig::default()
    });
    submit(
        &engine,
        "rn-rt1-res-auto",
        json!({"subcommand": "env", "input_path": "probe.txt", "input_content": "via-auto"}),
        30,
        1_048_576,
    );
    let outcome = execute_native_rust_via_engine(tool_engine::adapter::NativeRustCall {
        engine: &engine,
        run_id: "rn-rt1-res-auto",
        adapter: &adapter,
        manifest: &manifest,
        capability: None,
    })
    .expect("drive ok");
    assert_unified_envelope(&engine, "rn-rt1-res-auto", &outcome);
    assert_eq!(
        outcome.envelope.status,
        ToolRunStatus::Succeeded,
        "Auto 真实机制全链必须派发成功（fail-closed 仅对双机制不可用）: {:?}",
        outcome.envelope.error
    );
    assert_eq!(
        outcome.envelope.result.as_ref().unwrap()["input_file_head"],
        json!("via-auto")
    );
    let _ = std::fs::remove_dir_all(&ws_root);
}

/// D3 wrapper 形态：binary_target_sha256 双钉正例（test 生成 wrapper，
/// exec 目标 = fixture）与身份不符负例。
#[test]
fn native_rust_wrapper_double_pin_dispatch() {
    let fixture = checked_fixture();
    let dir = std::env::temp_dir().join(format!("rt1-e2e-wrapper-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let wrapper = dir.join("wrap.sh");
    std::fs::write(
        &wrapper,
        format!("#!/bin/bash\nexec \"{}\" \"$@\"\n", fixture.display()),
    )
    .unwrap();
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&wrapper, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let wrapper_sha = tool_registry::hash::sha256_hex(&std::fs::read(&wrapper).unwrap());
    let target_sha = FIXTURE_SHA256;

    // 正例：双钉齐 → wrapper exec fixture → SUCCEEDED。
    let mut manifest = base_manifest(&wrapper, &wrapper_sha);
    manifest["runtime"]["binary_target_sha256"] = json!(target_sha);
    let engine = fresh_engine(&manifest);
    let ws_root = workspace_root("wrap-ok");
    let adapter = adapter_for(&ws_root);
    submit(
        &engine,
        "rn-rt1-wrap-ok",
        json!({"subcommand": "env", "input_path": "probe.txt", "input_content": "via-wrapper"}),
        30,
        1_048_576,
    );
    let outcome = execute_native_rust_via_engine(tool_engine::adapter::NativeRustCall {
        engine: &engine,
        run_id: "rn-rt1-wrap-ok",
        adapter: &adapter,
        manifest: &manifest,
        capability: None,
    })
    .expect("drive ok");
    assert_unified_envelope(&engine, "rn-rt1-wrap-ok", &outcome);
    assert_eq!(
        outcome.envelope.status,
        ToolRunStatus::Succeeded,
        "wrap-ok failed: {:?}",
        outcome.envelope.error
    );
    assert_eq!(
        outcome.envelope.result.as_ref().unwrap()["input_file_head"],
        json!("via-wrapper")
    );
    let _ = std::fs::remove_dir_all(&ws_root);

    // 负例：exec 目标钉值不符 → POLICY（C §5.1:270 双钉）。
    let mut bad = base_manifest(&wrapper, &wrapper_sha);
    bad["runtime"]["binary_target_sha256"] = json!("0".repeat(64));
    let engine = fresh_engine(&bad);
    let ws_root = workspace_root("wrap-bad");
    let adapter = adapter_for(&ws_root);
    submit(
        &engine,
        "rn-rt1-wrap-bad",
        json!({"subcommand": "env", "input_path": "probe.txt"}),
        30,
        1_048_576,
    );
    let outcome = execute_native_rust_via_engine(tool_engine::adapter::NativeRustCall {
        engine: &engine,
        run_id: "rn-rt1-wrap-bad",
        adapter: &adapter,
        manifest: &bad,
        capability: None,
    })
    .expect("drive ok");
    assert_unified_envelope(&engine, "rn-rt1-wrap-bad", &outcome);
    let err = outcome.envelope.error.as_ref().unwrap();
    assert_eq!(err.category, "POLICY_DENIED");
    assert_eq!(err.code, json!("NATIVE_IDENTITY_MISMATCH"));
    assert!(err.message.contains("wrapper exec target"));
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(&ws_root);
}

/// 共享锁防并发测试的 /proc 断言窗口交错（workspace/capability 面各自
/// 独立临时目录，此锁仅为超时/取消路径的观测稳定）。
#[allow(dead_code)]
static OBSERVATION_LOCK: Mutex<()> = Mutex::new(());
