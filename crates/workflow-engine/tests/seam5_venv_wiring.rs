//! RT-3 S5 缝 5 承接验收（RT-2-ADR D8 缝 5:194 原文口径）。
//!
//! 缝 5 = rp-workflow `resolve_python_worker_value` 读 `runtime.venv` 构造
//! `VenvConfig` 注入（dispatch.rs:920-933 同款）。该函数在 bin 内（私有），
//! workflow 派发链 = `PythonWorkerToolDriver::drive` → resolve → adapter →
//! execute_via_engine，验收断言必须驱动同一构造点——经 `CARGO_BIN_EXE_rp-workflow`
//! CLI 全链承载（put + run，`--manifest-dirs` 指向 fixture 目录）：
//!
//! 1. **venv 声明 Manifest 探测拒绝**：runtime.venv 指向缺省根 → venv 探测
//!    Missing → PREPARING 拒绝落 FAILED + D7 五态信封（DEPENDENCY /
//!    VENV_NOT_PROVISIONED）、零 worker 进程拉起（信封为探测拒绝码而非执行
//!    错误）、零系统解释器回退（唯一终局 FAILED，无 SUCCEEDED 行）。
//! 2. **未声明 venv 的 Manifest 行为零变化（回归断言）**：同款 worker 裸名
//!    python3 直派 → run SUCCEEDED（探测零介入，adapter 构造与基线一致）。
//!
//! 判据 7 红线：正式 Manifest 零 diff——fixture Manifest 落临时目录
//! （RT-2 缝 4 同口径），登记面经 tool-registry 落临时库，用后即焚。

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use database::{migrate, DbPath, Pool, PoolConfig};
use serde_json::json;

const VENV_WORKER: &str = "crates/tool-engine/tests/fixtures/venv_parity_worker.py";

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// python_worker fixture Manifest（登记面与派发面同源；RT-2 e2e fresh_env 同款形状）。
/// worker_name 必须与 venv_parity_worker.py:31 的 initialize 身份一致
/// （TOOL-CONTRACT 2.2 握手校验，RT-2 判据 8；RT-2 e2e 同款硬编码）。
fn fixture_manifest(tool_id: &str, venv: Option<serde_json::Value>) -> serde_json::Value {
    let mut runtime = json!({
        "type": "python_worker",
        "protocol_version": "1",
        "worker_name": "venv-parity-worker",
        "command": ["python3", "-I", VENV_WORKER],
    });
    if let Some(v) = venv {
        runtime["venv"] = v;
    }
    json!({
        "schema_version": "1",
        "id": tool_id,
        "name": format!("RT-3 S5 seam5 fixture ({tool_id})"),
        "version": "0.1.0",
        "kind": "tool",
        "family": "utility",
        "description": "RT-3 S5 缝 5 承接验收 fixture（正式 Manifest 零 diff，RT-2 缝 4 同口径）",
        "runtime": runtime,
        "input_schema": {"type": "object"},
        "output_schema": {"type": "object"},
        "resources": {"cpu": "1", "memory": "256Mi", "timeout_seconds": 60, "max_output_bytes": 1048576},
        "network": {"mode": "none"},
        "secrets": [],
        "permission_level": "T0",
        "idempotent": true,
        "health": {"probe": {"type": "worker_ping"}},
        "provenance": {
            "origin": "rip-native",
            "source_path": "crates/tool-engine/tests/fixtures/venv_parity_worker.py",
            "license": "repo-internal",
            "dependencies": [],
            "tested": "RT-3 S5 e2e 实跑"
        }
    })
}

fn tool_node_definition(workflow_id: &str, tool_id: &str) -> serde_json::Value {
    json!({
        "schema": "workflow/definition-v1",
        "workflow_id": workflow_id,
        "version": 1,
        "name": "RT-3 S5 seam5 workflow",
        "description": "start -> tool -> end（p8-tool-node 同款最小图）",
        "compat": "graph-v1",
        "input_schema": {"type": "object"},
        "graph": {
            "nodes": [
                {"id": "start", "type": "start"},
                {"id": "dispatch", "type": "tool", "params": {"tool_id": tool_id, "input": {"op": "echo"}}},
                {"id": "done", "type": "end"}
            ],
            "edges": [
                {"from": "start", "to": "dispatch"},
                {"from": "dispatch", "to": "done"}
            ]
        }
    })
}

#[test]
fn seam5_venv_probe_rejected_and_no_venv_unchanged() {
    let dir = std::env::temp_dir().join(format!("rip-rt3-seam5-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let db = dir.join("seam5-test.db");
    let _ = std::fs::remove_file(&db);
    let _ = std::fs::remove_file(dir.join("seam5-test.db-wal"));

    // 1. 临时库 + 迁移 + 登记两份 fixture 工具（CLI run 复用同一库文件）。
    let pool = Pool::open(PoolConfig {
        path: DbPath::File(db.clone()),
        readonly: false,
        max_connections: 1,
        busy_timeout: Duration::from_secs(5),
    })
    .expect("pool");
    migrate(&pool, &repo_root().join("migrations")).expect("migrate");
    let registry = tool_registry::Registry::open(pool.clone());
    // venv 声明：根路径缺省（不创建）→ probe Missing → VENV_NOT_PROVISIONED。
    let absent_venv = dir.join("absent-venv-root");
    let manifest_venv = fixture_manifest(
        "rip.t.seam5.venv",
        Some(json!({
            "path": absent_venv.to_str().expect("utf8"),
            "direct_deps": ["rp-test-fixture"]
        })),
    );
    registry
        .register(&manifest_venv)
        .expect("register venv fixture");
    let manifest_novenv = fixture_manifest("rip.t.seam5.novenv", None);
    registry
        .register(&manifest_novenv)
        .expect("register novenv fixture");

    // 2. fixture Manifest 目录（--manifest-dirs 指向；manifests/tools/ 零 diff）。
    let fixture_dir = dir.join("manifests-fixture");
    std::fs::create_dir_all(&fixture_dir).expect("fixture dir");
    std::fs::write(
        fixture_dir.join("rip.t.seam5.venv.json"),
        serde_json::to_string_pretty(&manifest_venv).unwrap(),
    )
    .unwrap();
    std::fs::write(
        fixture_dir.join("rip.t.seam5.novenv.json"),
        serde_json::to_string_pretty(&manifest_novenv).unwrap(),
    )
    .unwrap();

    // 3. workflow 定义 put（CLI）。
    let def_venv = dir.join("wf-venv.json");
    std::fs::write(
        &def_venv,
        serde_json::to_string_pretty(&tool_node_definition(
            "wf-rt3-seam5-venv",
            "rip.t.seam5.venv",
        ))
        .unwrap(),
    )
    .unwrap();
    let def_novenv = dir.join("wf-novenv.json");
    std::fs::write(
        &def_novenv,
        serde_json::to_string_pretty(&tool_node_definition(
            "wf-rt3-seam5-novenv",
            "rip.t.seam5.novenv",
        ))
        .unwrap(),
    )
    .unwrap();

    let bin = PathBuf::from(env!("CARGO_BIN_EXE_rp-workflow"));
    let migrations = repo_root()
        .join("migrations")
        .to_str()
        .expect("utf8")
        .to_string();
    let db_str = db.to_str().expect("utf8").to_string();
    let fixture_str = fixture_dir.to_str().expect("utf8").to_string();
    let run_cli = |args: Vec<String>| -> (i32, String, String) {
        let out = Command::new(&bin)
            .current_dir(repo_root())
            .args(&args)
            .output()
            .expect("rp-workflow 可执行");
        (
            out.status.code().unwrap_or(-1),
            String::from_utf8_lossy(&out.stdout).to_string(),
            String::from_utf8_lossy(&out.stderr).to_string(),
        )
    };
    let put = |def: &Path| -> (i32, String, String) {
        run_cli(vec![
            "put".to_string(),
            def.to_str().expect("utf8").to_string(),
            "--db".to_string(),
            db_str.clone(),
            "--migrations-dir".to_string(),
            migrations.clone(),
        ])
    };
    let (rc, out, err) = put(&def_venv);
    assert_eq!(rc, 0, "put venv workflow：{out}{err}");
    let (rc, out, err) = put(&def_novenv);
    assert_eq!(rc, 0, "put novenv workflow：{out}{err}");

    // 4a. venv 声明 Manifest 经 workflow 派发链 → PREPARING 探测拒绝。
    let (rc, out, err) = run_cli(vec![
        "run".to_string(),
        "wf-rt3-seam5-venv".to_string(),
        "--db".to_string(),
        db_str.clone(),
        "--migrations-dir".to_string(),
        migrations.clone(),
        "--manifest-dirs".to_string(),
        fixture_str.clone(),
    ]);
    let outcome: serde_json::Value = serde_json::from_str(out.trim())
        .unwrap_or_else(|e| panic!("run 输出 JSON：{e}；stdout={out} stderr={err}"));
    assert_eq!(rc, 1, "venv run 失败退出码：{outcome}");
    assert_eq!(outcome["result"], "RUN-OUTCOME");
    assert_eq!(
        outcome["state"], "failed",
        "venv 探测拒绝 → run failed（state.rs:47 终态词表小写）"
    );
    let nodes = outcome["nodes"].as_array().expect("nodes");
    let tool_node = nodes
        .iter()
        .find(|n| n["node_id"] == "dispatch")
        .expect("tool 节点行");
    assert_eq!(tool_node["state"], "failed", "tool 节点 failed");

    // 5. DB 断言：PREPARING 拒绝落 FAILED + D7 五态信封（DEPENDENCY /
    //    VENV_NOT_PROVISIONED）；零系统解释器回退（无 SUCCEEDED 行）。
    let venv_rows: Vec<(String, Option<String>, Option<String>)> = pool
        .with_conn(|c| {
            let mut stmt = c
                .prepare(
                    "SELECT status, error_category, error_json FROM tool_run WHERE tool_id = ?1",
                )
                .expect("prepare");
            let rows = stmt
                .query_map(["rip.t.seam5.venv"], |r| {
                    Ok((r.get(0)?, r.get(1)?, r.get(2)?))
                })
                .expect("query")
                .collect::<Result<Vec<_>, _>>()
                .expect("rows");
            Ok::<Vec<(String, Option<String>, Option<String>)>, database::DbError>(rows)
        })
        .expect("conn");
    assert_eq!(venv_rows.len(), 1, "venv tool_run 单行");
    let (status, category, error_json) = &venv_rows[0];
    assert_eq!(status, "FAILED", "PREPARING 拒绝落 FAILED");
    assert_eq!(category.as_deref(), Some("DEPENDENCY"), "五态信封 category");
    let err_obj: serde_json::Value =
        serde_json::from_str(error_json.as_deref().expect("error_json")).expect("信封 JSON");
    assert_eq!(
        err_obj["code"], "VENV_NOT_PROVISIONED",
        "venv 五态 Missing → 探测拒绝码"
    );
    assert_eq!(err_obj["retryable"], false, "fail-closed 非重试面");
    // 零 worker 进程拉起面：信封消息为 venv 探测拒绝语义（非 worker 执行错误）。
    assert!(
        err_obj["message"]
            .as_str()
            .expect("message")
            .contains("venv"),
        "探测拒绝消息：{}",
        err_obj["message"]
    );

    // 4b/6. 未声明 venv 的 Manifest：同链路派发 → SUCCEEDED（行为零变化）。
    let (rc, out, err) = run_cli(vec![
        "run".to_string(),
        "wf-rt3-seam5-novenv".to_string(),
        "--db".to_string(),
        db_str.clone(),
        "--migrations-dir".to_string(),
        migrations.clone(),
        "--manifest-dirs".to_string(),
        fixture_str.clone(),
    ]);
    let outcome: serde_json::Value = serde_json::from_str(out.trim())
        .unwrap_or_else(|e| panic!("run 输出 JSON：{e}；stdout={out} stderr={err}"));
    assert_eq!(rc, 0, "novenv run 成功：{outcome} {err}");
    assert_eq!(
        outcome["state"], "succeeded",
        "裸名 python3 直派行为零变化（state.rs:47 小写）"
    );
    let nodes = outcome["nodes"].as_array().expect("nodes");
    let tool_node = nodes
        .iter()
        .find(|n| n["node_id"] == "dispatch")
        .expect("tool 节点行");
    assert_eq!(tool_node["state"], "succeeded");
    let novenv_rows: Vec<(String, Option<String>)> = pool
        .with_conn(|c| {
            let mut stmt = c
                .prepare("SELECT status, error_json FROM tool_run WHERE tool_id = ?1")
                .expect("prepare");
            let rows = stmt
                .query_map(["rip.t.seam5.novenv"], |r| Ok((r.get(0)?, r.get(1)?)))
                .expect("query")
                .collect::<Result<Vec<_>, _>>()
                .expect("rows");
            Ok::<Vec<(String, Option<String>)>, database::DbError>(rows)
        })
        .expect("conn");
    assert_eq!(novenv_rows.len(), 1);
    assert_eq!(novenv_rows[0].0, "SUCCEEDED");
    assert!(
        novenv_rows[0].1.is_none(),
        "无信封错误（worker 真实执行 echo 成功）"
    );

    let _ = std::fs::remove_dir_all(&dir);
}
