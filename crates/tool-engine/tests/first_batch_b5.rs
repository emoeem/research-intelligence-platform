//! P1-7d 验收（ROADMAP §2.3 P1-7d 判据原句，TOOL-CONTRACT §10.2 B5 行）：
//! rip.kb.rtis（T09）与 rip.kb.eri（T10）Adapter Manifest 指向现有入口登记，
//! rt.py 与 rtis_kb.mjs 只读调用经派发器打通（eri_adapter bridge-gate 同批
//! 实跑）；integrations/rtis/ 目录零修改（红线 TOOL-INVENTORY:161：
//! 不修改生产 RTIS 行为）；rtis.py 编排层与写路径不登记。
//! 前置：V-3（T1/T2 分界线）已裁决落盘 docs/phase1/T1-T2-BOUNDARY-RULING.md。
//! 开发库副本：migrations/dev-b5.db（*.db 已 gitignore，每次重建）。

use database::{migrate, DbPath, Pool, PoolConfig};
use domain::ToolRunStatus;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;
use tool_engine::adapter::{
    execute_via_engine, AdapterCall, PythonWorkerAdapter, PythonWorkerConfig, RestartPolicy,
};
use tool_engine::Engine;
use tool_registry::Registry;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root")
}

fn reset_dev_copy() {
    let base = repo_root().join("migrations/dev-b5.db");
    for suffix in ["", "-wal", "-shm"] {
        let mut p = base.clone().into_os_string();
        p.push(suffix);
        let p = PathBuf::from(p);
        if p.exists() {
            std::fs::remove_file(&p).unwrap_or_else(|e| panic!("remove {}: {e}", p.display()));
        }
    }
}

static ENV_SEQ: AtomicU32 = AtomicU32::new(0);

fn load_manifest(name: &str) -> Value {
    let text = std::fs::read_to_string(
        repo_root()
            .join("manifests/tools")
            .join(format!("{name}.json")),
    )
    .unwrap();
    serde_json::from_str(&text).unwrap()
}

fn adapter_for(manifest: &Value, cache_dir: &Path) -> PythonWorkerAdapter {
    let command: Vec<String> = manifest["runtime"]["command"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();
    let worker_name = manifest["runtime"]["worker_name"]
        .as_str()
        .unwrap()
        .to_string();
    PythonWorkerAdapter::new(PythonWorkerConfig {
        worker_name,
        command,
        working_dir: Some(repo_root()),
        restart: RestartPolicy::Pooled,
        extra_env: vec![(
            "WORKER_CACHE_DIR".to_string(),
            cache_dir.to_string_lossy().to_string(),
        )],
        handshake_timeout: Duration::from_secs(10),
    })
}

/// B5 两件是本机 RTIS sidecar 的只读集成面：rt.py 依赖 kit 目录下的
/// knowledge.db（gitignore 运行时数据），eri_adapter 依赖 kit data/ 与
/// migrations/platform.db（同为 gitignore 运行时数据）。CI 与新机器上
/// 这些运行时数据不在，按 V-9 跳过；受控环境（本地验收）实跑。
fn rtis_runtime_ready() -> bool {
    let kit = repo_root().join("integrations/rtis");
    kit.join("knowledge.db").exists()
        && kit.join("data").exists()
        && repo_root().join("migrations/platform.db").exists()
}

#[test]
fn b5_rtis_and_eri_readonly_dispatch_end_to_end() {
    if !rtis_runtime_ready() {
        eprintln!(
            "SKIP: RTIS sidecar 运行时数据不在本地（integrations/rtis/knowledge.db、integrations/rtis/data、migrations/platform.db 均为 gitignore 运行时数据），B5 只读集成面无法实测（V-9，受控环境端到端）"
        );
        return;
    }
    reset_dev_copy();
    let pool = Pool::open(PoolConfig {
        path: DbPath::File(repo_root().join("migrations/dev-b5.db")),
        readonly: false,
        max_connections: 1,
        busy_timeout: Duration::from_secs(5),
    })
    .expect("open dev copy pool");
    migrate(&pool, &repo_root().join("migrations")).expect("migrations 001..005");
    let registry = Registry::open(pool.clone());
    // 生产语义直登 B5 两件（T0 只读面，V-3 裁决见 docs/phase1/）。
    registry.register(&load_manifest("rip.kb.rtis")).unwrap();
    registry.register(&load_manifest("rip.kb.eri")).unwrap();
    let engine = Engine::open(pool, registry);
    let cache_dir = std::env::temp_dir().join(format!(
        "rip-b5-cache-{}-{}",
        std::process::id(),
        ENV_SEQ.fetch_add(1, Ordering::SeqCst)
    ));

    // 判据原句 1/2：rt.py 只读调用经派发器打通（op=cli）。
    {
        let manifest = load_manifest("rip.kb.rtis");
        let adapter = adapter_for(&manifest, &cache_dir);
        engine
            .submit(&json!({
                "protocol_version": "1",
                "tool_id": "rip.kb.rtis",
                "run_id": "rn-b5-rt-py",
                "input": {"surface": "cli", "q": "kingdee", "limit": 2},
                "workspace_id": "ws-default"
            }))
            .unwrap();
        let out = execute_via_engine(AdapterCall {
            engine: &engine,
            run_id: "rn-b5-rt-py",
            adapter: &adapter,
            cancel_after: None,
        })
        .unwrap();
        let env = &out.envelope;
        assert_eq!(env.status, ToolRunStatus::Succeeded, "{:?}", env.error);
        let result = env.result.as_ref().unwrap();
        assert_eq!(result["exit_code"], 0);
        let stdout = result["stdout"].as_str().unwrap();
        assert!(
            stdout.contains("900008") || stdout.contains("kingdee"),
            "rt.py output: {stdout}"
        );
        adapter.shutdown();
    }

    // 判据原句 2/2：rtis_kb.mjs 只读调用经派发器打通（op=engine，stdin JSON）。
    {
        let manifest = load_manifest("rip.kb.rtis");
        let adapter = adapter_for(&manifest, &cache_dir);
        engine
            .submit(&json!({
                "protocol_version": "1",
                "tool_id": "rip.kb.rtis",
                "run_id": "rn-b5-kb-mjs",
                "input": {"surface": "engine", "q": "kingdee", "limit": 2},
                "workspace_id": "ws-default"
            }))
            .unwrap();
        let out = execute_via_engine(AdapterCall {
            engine: &engine,
            run_id: "rn-b5-kb-mjs",
            adapter: &adapter,
            cancel_after: None,
        })
        .unwrap();
        let env = &out.envelope;
        assert_eq!(env.status, ToolRunStatus::Succeeded, "{:?}", env.error);
        let result = env.result.as_ref().unwrap();
        assert_eq!(result["exit_code"], 0);
        let stdout = result["stdout"].as_str().unwrap();
        let parsed: Value = serde_json::from_str(stdout)
            .unwrap_or_else(|e| panic!("mjs stdout must be JSON ({e}): {stdout}"));
        assert_eq!(parsed["ok"], true, "mjs payload: {parsed}");
        assert!(
            parsed["result"]["items"].is_array(),
            "mjs payload: {parsed}"
        );
        adapter.shutdown();
    }

    // T10：eri_adapter bridge-gate 只读复查经派发器打通（task_file 物化）。
    {
        let manifest = load_manifest("rip.kb.eri");
        let adapter = adapter_for(&manifest, &cache_dir);
        engine
            .submit(&json!({
                "protocol_version": "1",
                "tool_id": "rip.kb.eri",
                "run_id": "rn-b5-eri",
                "input": {"task_json": "{\"task_id\":\"rip-b5-eri-probe\",\"objective\":\"read path probe\",\"keywords\":[\"nmap\"]}"},
                "workspace_id": "ws-default"
            }))
            .unwrap();
        let out = execute_via_engine(AdapterCall {
            engine: &engine,
            run_id: "rn-b5-eri",
            adapter: &adapter,
            cancel_after: None,
        })
        .unwrap();
        let env = &out.envelope;
        assert_eq!(env.status, ToolRunStatus::Succeeded, "{:?}", env.error);
        let result = env.result.as_ref().unwrap();
        assert_eq!(result["exit_code"], 0);
        let stdout = result["stdout"].as_str().unwrap();
        let parsed: Value = serde_json::from_str(stdout)
            .unwrap_or_else(|e| panic!("eri stdout must be JSON ({e}): {stdout}"));
        assert!(
            parsed["external_resource_review"].is_object(),
            "eri payload: {parsed}"
        );
        assert!(
            parsed["adapter"]["resolver_used"].is_string(),
            "eri adapter metadata: {parsed}"
        );
        adapter.shutdown();
    }
}

/// 红线（TOOL-INVENTORY:161）：integrations/rtis/ 零修改。
/// git status --porcelain -- integrations/rtis/ 必须零输出
///（审计 JSONL 已用固定 env RTIS_ERI_ADAPTER_AUDIT=off 关闭，
/// knowledge.db 等运行时数据本就在 gitignore 内）。
#[test]
fn b5_rtis_kit_directory_untouched() {
    let output = Command::new("git")
        .args(["status", "--porcelain", "--", "integrations/rtis/"])
        .current_dir(repo_root())
        .output()
        .unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        stdout.trim().is_empty(),
        "integrations/rtis/ must stay untouched, got: {stdout:?}"
    );
    // 受保护入口文件在位（防测试空转）。
    for p in [
        "integrations/rtis/rt.py",
        "integrations/rtis/rtis_kb.mjs",
        "integrations/rtis/eri_adapter.py",
    ] {
        assert!(repo_root().join(p).is_file(), "missing: {p}");
    }
    // V-3 裁决文档已落盘（登记前定稿的判据）。
    assert!(
        repo_root()
            .join("docs/phase1/T1-T2-BOUNDARY-RULING.md")
            .is_file(),
        "V-3 ruling must exist before B5 registration"
    );
    // rtis.py 编排层不在登记面（写路径不登记）。
    let found = std::fs::read_dir(repo_root().join("manifests/tools")).unwrap();
    for entry in found.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        assert!(
            !name.contains("rtis.py") && !name.contains("rt.py"),
            "write-path entrypoints must not be registered: {name}"
        );
    }
}
