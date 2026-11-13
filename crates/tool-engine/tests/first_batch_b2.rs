//! P1-7b 验收（ROADMAP §2.3 P1-7b 判据原句，TOOL-CONTRACT §10.2 B2 行）：
//! B2 三工具（rip.kb.keyword_search/T05、rip.kb.inventory/T06、
//! rip.ops.etl_status/T07）薄包装登记后各跑一次只读调用成功且输出符合
//! 各自 output_schema；scripts/ 三脚本本体零修改（git status 核查）；
//! 调用走固定入口 argv（薄包装 scripts/rip_cli_worker.py，子进程固定
//! [python3, -I, <script>, *args]，不经 shell）。
//!
//! 开发库副本：migrations/dev-b2.db（*.db 已 gitignore，每次重建）。

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

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root")
}

fn dev_copy_path() -> PathBuf {
    repo_root().join("migrations/dev-b2.db")
}

fn reset_dev_copy() {
    for suffix in ["", "-wal", "-shm"] {
        let mut p = dev_copy_path().clone().into_os_string();
        p.push(suffix);
        let p = PathBuf::from(p);
        if p.exists() {
            std::fs::remove_file(&p).unwrap_or_else(|e| panic!("remove {}: {e}", p.display()));
        }
    }
}

fn fresh_engine() -> (Engine, Registry, PathBuf) {
    reset_dev_copy();
    let pool = Pool::open(PoolConfig {
        path: DbPath::File(dev_copy_path()),
        readonly: false,
        max_connections: 1,
        busy_timeout: Duration::from_secs(5),
    })
    .expect("open dev copy pool");
    migrate(&pool, &repo_root().join("migrations")).expect("migrations 001..005");
    let registry = Registry::open(pool.clone());
    (Engine::open(pool, registry.clone()), registry, repo_root())
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

fn cli_adapter(cache_dir: &Path, command: &[String]) -> PythonWorkerAdapter {
    PythonWorkerAdapter::new(PythonWorkerConfig {
        worker_name: "rip-cli-worker".to_string(),
        command: command.to_vec(),
        working_dir: Some(repo_root()),
        restart: RestartPolicy::Pooled,
        extra_env: vec![(
            "WORKER_CACHE_DIR".to_string(),
            cache_dir.to_string_lossy().to_string(),
        )],
        handshake_timeout: Duration::from_secs(10),
    })
}

#[test]
fn b2_three_readonly_calls_succeed_with_schema_valid_output() {
    // 三个 CLI worker 以只读方式打开 migrations/platform.db（gitignore 的
    // 本机运行时数据；脚本本体按 TOOL-CONTRACT §10.2 B2 行红线零修改，
    // 生产库零写入）。该文件不在仓库内时（CI、新机器）按 V-9 跳过：
    // B2 工具面在受控环境（本地验收）实测。
    if !repo_root().join("migrations/platform.db").exists() {
        eprintln!(
            "SKIP: migrations/platform.db 不在本地（gitignore 的运行时数据），B2 三件只读工具无法打开库（V-9，受控环境端到端）"
        );
        return;
    }
    let (engine, registry, _) = fresh_engine();
    let cache_dir = std::env::temp_dir().join(format!(
        "rip-b2-cache-{}-{}",
        std::process::id(),
        ENV_SEQ.fetch_add(1, Ordering::SeqCst)
    ));

    // 登记：生产语义直登 B2 三件（薄包装 python_worker，无镜像 digest 面）。
    for name in [
        "rip.kb.keyword_search",
        "rip.kb.inventory",
        "rip.ops.etl_status",
    ] {
        registry
            .register(&load_manifest(name))
            .unwrap_or_else(|e| panic!("register {name}: {e}"));
    }
    let found = registry.discover().unwrap();
    let ids: Vec<&str> = found.iter().map(|t| t.tool_id.as_str()).collect();
    assert_eq!(
        ids,
        vec![
            "rip.kb.inventory",
            "rip.kb.keyword_search",
            "rip.ops.etl_status"
        ]
    );

    // T05：关键词检索（只读；keyword 必经 input_schema，命中数随库态 ≥ 0）
    {
        let manifest = load_manifest("rip.kb.keyword_search");
        let command: Vec<String> = manifest["runtime"]["command"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_string())
            .collect();
        let adapter = cli_adapter(&cache_dir, &command);
        engine
            .submit(&json!({
                "protocol_version": "1",
                "tool_id": "rip.kb.keyword_search",
                "run_id": "rn-b2-kwsearch",
                "input": {"keyword": "nmap", "per_group_limit": 2},
                "workspace_id": "ws-default"
            }))
            .unwrap();
        let out = execute_via_engine(AdapterCall {
            engine: &engine,
            run_id: "rn-b2-kwsearch",
            adapter: &adapter,
            cancel_after: None,
        })
        .unwrap();
        let env = &out.envelope;
        assert_eq!(env.status, ToolRunStatus::Succeeded, "{:?}", env.error);
        let result = env.result.as_ref().unwrap();
        // 引擎已按 output_schema 校验；此处断言结构化解析的语义面。
        assert_eq!(result["keyword"], "nmap");
        assert_eq!(result["per_group_limit"], 2);
        assert!(result["total_hits"].is_u64());
        assert_eq!(result["groups"].as_array().unwrap().len(), 5);
        let tool_group = &result["groups"][0];
        assert_eq!(tool_group["label"], "工具");
        let hits = tool_group["hits"].as_array().unwrap();
        assert!(
            hits.iter().any(|h| h.as_str().unwrap().starts_with("nmap")),
            "nmap tool must be found, groups: {result}"
        );
        assert_eq!(
            row_status(&engine, "rn-b2-kwsearch").as_deref(),
            Some("SUCCEEDED")
        );
        adapter.shutdown();
    }

    // T06：清单统计 + A 级明细（只读）
    {
        let manifest = load_manifest("rip.kb.inventory");
        let command: Vec<String> = manifest["runtime"]["command"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_string())
            .collect();
        let adapter = cli_adapter(&cache_dir, &command);
        engine
            .submit(&json!({
                "protocol_version": "1",
                "tool_id": "rip.kb.inventory",
                "run_id": "rn-b2-inventory",
                "input": {"grade": "A"},
                "workspace_id": "ws-default"
            }))
            .unwrap();
        let out = execute_via_engine(AdapterCall {
            engine: &engine,
            run_id: "rn-b2-inventory",
            adapter: &adapter,
            cancel_after: None,
        })
        .unwrap();
        let env = &out.envelope;
        assert_eq!(env.status, ToolRunStatus::Succeeded, "{:?}", env.error);
        let result = env.result.as_ref().unwrap();
        assert!(result["total"].is_u64());
        assert!(result["by_grade"].is_object());
        assert_eq!(result["grade_filter"], "A");
        let entries = result["grade_entries"].as_array().unwrap();
        assert!(
            entries.iter().all(|e| e["raw"].is_string()),
            "every entry keeps its raw line: {result}"
        );
        assert_eq!(
            row_status(&engine, "rn-b2-inventory").as_deref(),
            Some("SUCCEEDED")
        );
        adapter.shutdown();
    }

    // T07：ETL 健康（只读 quick_check）
    {
        let manifest = load_manifest("rip.ops.etl_status");
        let command: Vec<String> = manifest["runtime"]["command"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_string())
            .collect();
        let adapter = cli_adapter(&cache_dir, &command);
        engine
            .submit(&json!({
                "protocol_version": "1",
                "tool_id": "rip.ops.etl_status",
                "run_id": "rn-b2-etl",
                "input": {},
                "workspace_id": "ws-default"
            }))
            .unwrap();
        let out = execute_via_engine(AdapterCall {
            engine: &engine,
            run_id: "rn-b2-etl",
            adapter: &adapter,
            cancel_after: None,
        })
        .unwrap();
        let env = &out.envelope;
        assert_eq!(env.status, ToolRunStatus::Succeeded, "{:?}", env.error);
        let result = env.result.as_ref().unwrap();
        assert_eq!(result["platform"]["quick_check"], "ok");
        assert!(result["platform"]["resources"].is_u64());
        assert_eq!(result["etl_files"].as_array().unwrap().len(), 3);
        assert_eq!(
            row_status(&engine, "rn-b2-etl").as_deref(),
            Some("SUCCEEDED")
        );
        adapter.shutdown();
    }
}

/// TOOL-CONTRACT §10.2 B2 行红线：scripts/ 三脚本本体零修改。
/// git status --porcelain -- scripts/ 只允许出现未跟踪新文件（?? 前缀，
/// 本批的薄包装 rip_cli_worker.py），三个受保护脚本不得出现在任何改动行。
#[test]
fn b2_protected_scripts_untouched_by_tests() {
    let output = Command::new("git")
        .args(["status", "--porcelain", "--", "scripts/"])
        .current_dir(repo_root())
        .output()
        .unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    for line in stdout.lines() {
        let mut parts = line.splitn(2, ' ');
        let code = parts.next().unwrap_or_default();
        let path = parts.next().unwrap_or_default().trim();
        assert!(
            code == "??",
            "scripts/ must carry no tracked modifications, got line: {line:?}"
        );
        for protected in ["rip_kb_search.py", "rip_inventory.py", "rip_etl_status.py"] {
            assert!(
                !path.ends_with(protected),
                "protected script must not appear in git status: {line:?}"
            );
        }
    }
    // 三脚本在受保护清单内且文件在位（防测试空转）。
    for protected in ["rip_kb_search.py", "rip_inventory.py", "rip_etl_status.py"] {
        assert!(
            repo_root().join("scripts").join(protected).is_file(),
            "protected script missing: {protected}"
        );
    }
}
