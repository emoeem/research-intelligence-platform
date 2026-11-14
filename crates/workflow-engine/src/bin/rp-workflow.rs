//! rp-workflow CLI（P8-1/P8-2 验收入口，ROADMAP §9.4 命令形）：
//!
//! ~~~text
//! cargo run --offline -p workflow-engine --bin rp-workflow -- validate <file>
//! cargo run --offline -p workflow-engine --bin rp-workflow -- put <file> [--db <path>]
//! cargo run --offline -p workflow-engine --bin rp-workflow -- get <workflow_id> [--version <n>] [--db <path>]
//! cargo run --offline -p workflow-engine --bin rp-workflow -- list [--db <path>]
//! cargo run --offline -p workflow-engine --bin rp-workflow -- import-legacy --src <db> [--db <path>]
//! cargo run --offline -p workflow-engine --bin rp-workflow -- run <workflow_id> --input '<json>' [--version <n>] [--db <path>]
//! cargo run --offline -p workflow-engine --bin rp-workflow -- status <run_id> [--db <path>]
//! cargo run --offline -p workflow-engine --bin rp-workflow -- nodes <run_id> [--db <path>]
//! cargo run --offline -p workflow-engine --bin rp-workflow -- resume <run_id> [--input '<json>'] [--db <path>]
//! cargo run --offline -p workflow-engine --bin rp-workflow -- retry <run_id> --node <node_id> [--run-id <new>] [--db <path>]
//! cargo run --offline -p workflow-engine --bin rp-workflow -- pause <run_id> [--db <path>]
//! cargo run --offline -p workflow-engine --bin rp-workflow -- cancel <run_id> [--db <path>]
//! ~~~
//!
//! exit 约定（一处清单）：run/resume/retry 到终态 SUCCEEDED = 0；停等
//! Paused（协作暂停 / 人工介入 awaiting_approval）= 0（停等非错误）；到终态
//! FAILED = 1（终态是运行结果不是受理错误，outcome JSON 照实输出）；
//! 装载失败（JSON 语法 / Schema 违规）= 2；静态校验拒绝（环/孤立/必填
//! 参数/非法边端点，错误文本含 kind 词）= 1；受理拒绝（input 非 object /
//! 非法 branch 边形状 / run 或定义不存在等 InvalidInput）= 2；
//! 存储错误 = 1；用法错误 = 2。默认 `--db migrations/dev-p8.db`（*.db 已
//! gitignore；platform*.db 一律拒绝，生产库守卫与 rp-agent 同款）。
//!
//! run 受理面（P8-2/P8-3）：compat 必须 graph-v1（legacy 参考行永不执行）；
//! input 必须 JSON object 且合 input_schema（若声明）；branch 边形状按
//! 冻结口径校验（见 exec::accept_definition）。Tool 节点经 tool-engine
//! 派发链（submit D-TC-05 + ToolRun 状态机 + 信封，python_worker 真实
//! 派发按 Manifest runtime 块解析，不经 shell）；Knowledge 节点经
//! context-broker Broker::assemble（rp-agent 同款装配全源）；Condition
//! 为 jsonschema 纯函数谓词；Parallel 为 fork-join 并发收敛；Agent 节点
//! 经 agent-engine AgentLoop 真实闭环（fresh run / PAUSED 续接按 agent_run
//! 现态分派，trace_id = workflow run_id 落 015 链路）；Human 节点停在
//! awaiting_approval（018 控制行），resume --input 注入批准参数后续跑。
//! pause 子命令写 018 workflow_run_control 请求位并经 agent-engine
//! RunControl 向运行中 agent_run 传播（014 控制行）。

use std::path::{Path, PathBuf};
use std::process::exit;
use std::sync::Arc;

use agent_engine::broker_link::{BrokerLink, FirstFamilyConfirmer};
use agent_engine::evidence::EvidenceRegistrar;
use agent_engine::store::AgentStore;
use agent_engine::tool_link::{PythonWorkerDriver, ToolLink};
use agent_engine::{agent_loop::AgentLoop, LoopConfig};
use database::{migrate, DbPath, Pool, PoolConfig};
use tool_engine::adapter::python_worker::{
    execute_via_engine, AdapterCall, PythonWorkerAdapter, PythonWorkerConfig, RestartPolicy,
};
use workflow_engine::dsl::parse_definition;
use workflow_engine::exec::{
    node_agent, RunOutcome, ToolRunDriver, WorkflowExecutor, DEFAULT_NODE_TIMEOUT_MS,
};
use workflow_engine::store::{writable_pool, WorkflowStore};
use workflow_engine::validate::validate_definition;
use workflow_engine::WorkflowError;

fn main() {
    exit(run());
}

fn usage_text() -> &'static str {
    "usage: rp-workflow <sub> [args]\n\
     \n\
     subcommands:\n\
     \x20 validate <file>\n\
     \x20   装载门 + 静态校验，不改库；通过 exit 0\n\
     \x20 put <file> [--db <path>]\n\
     \x20   四道门后追加一版定义，输出 {{workflow_id, version, ...}} 单行 JSON\n\
     \x20 get <workflow_id> [--version <n>] [--db <path>]\n\
     \x20   回读定义原文（缺 --version 取最新版；历史版本逐字节可查）\n\
     \x20 list [--db <path>]\n\
     \x20   每 workflow_id 一行（最新版 + 版本数）+ 汇总行\n\
     \x20 import-legacy --src <db> [--db <path>]\n\
     \x20   003 workflow 表存量行只读参考导入（compat=legacy-method-reference）\n\
     \x20 run <workflow_id> --input '<json>' [--version <n>] [--node-timeout-ms <n>]\n\
     \x20      [--db <path>] [--rulepack-dir <dir>] [--manifest-dirs <csv>]\n\
     \x20   受理门后执行到终态（四类非智能节点，Agent 节点属 P8-3 拒绝）\n\
     \x20 status <run_id> [--db <path>]\n\
     \x20   run 头行投影（六态 + 输入/输出/错误载荷）\n\
     \x20 nodes <run_id> [--db <path>]\n\
     \x20   节点状态行（五态 + attempt + 输出/错误）+ checkpoint 汇总\n\
     \x20 resume <run_id> [--db <path>]\n\
     \x20   断点恢复：从最近 checkpoint 续跑，已完成节点不重跑\n\
     \x20 retry <run_id> --node <node_id> [--db <path>]\n\
     \x20   失败重试：新 run 承载，只重跑目标失败节点（attempt 递增）\n\
     \x20 cancel <run_id> [--db <path>]\n\
     \x20   协作式取消（queued/running/paused -> cancelled）\n\
     \n\
     exit: ok=0, run 终态 FAILED/静态校验拒绝/存储失败=1,\n\
     装载失败/受理拒绝/用法错误=2\n\
     默认 --db migrations/dev-p8.db（platform*.db 一律拒绝）\n\
     \n\
     help: rp-workflow --help（stdout, exit 0）"
}

fn usage() -> i32 {
    // P9-3 帮助面统一口径：误用 = usage 到 stderr + exit 2；
    // 显式 --help/-h = usage 到 stdout + exit 0（见 run()）。
    eprintln!("{}", usage_text());
    2
}

fn run() -> i32 {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!("{}", usage_text());
        return 0;
    }
    match args.first().map(String::as_str) {
        Some("validate") => {
            if args.len() < 2 {
                usage()
            } else {
                cmd_validate(&args[1])
            }
        }
        Some("put") => {
            if args.len() < 2 {
                usage()
            } else {
                cmd_put(&args[1..])
            }
        }
        Some("get") => {
            if args.len() < 2 {
                usage()
            } else {
                cmd_get(&args[1..])
            }
        }
        Some("list") => cmd_list(&args[1..]),
        Some("import-legacy") => cmd_import_legacy(&args[1..]),
        Some("run") => {
            if args.len() < 2 {
                usage()
            } else {
                cmd_run(&args[1..])
            }
        }
        Some("status") => {
            if args.len() < 2 {
                usage()
            } else {
                cmd_status(&args[1..])
            }
        }
        Some("nodes") => {
            if args.len() < 2 {
                usage()
            } else {
                cmd_nodes(&args[1..])
            }
        }
        Some("resume") => {
            if args.len() < 2 {
                usage()
            } else {
                cmd_resume(&args[1..])
            }
        }
        Some("retry") => {
            if args.len() < 2 {
                usage()
            } else {
                cmd_retry(&args[1..])
            }
        }
        Some("pause") => {
            if args.len() < 2 {
                usage()
            } else {
                cmd_pause(&args[1..])
            }
        }
        Some("cancel") => {
            if args.len() < 2 {
                usage()
            } else {
                cmd_cancel(&args[1..])
            }
        }
        _ => usage(),
    }
}

/// 生产库守卫（红线：platform*.db 一律不接；rp-agent 同款）。
fn guard_db_path(db: &Path) {
    let name = db
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    if name.starts_with("platform") {
        eprintln!(
            "{}",
            serde_json::json!({"code": "production_db_refused",
                   "message": "--db must be a dev copy or temp file; platform*.db is forbidden (ROADMAP §13)"})
        );
        exit(1);
    }
}

/// 默认开发库（*.db 全局 gitignore，不入库）。
fn default_db() -> PathBuf {
    PathBuf::from("migrations/dev-p8.db")
}

/// 读旗标值。
fn flag_value<'a>(iter: &mut impl Iterator<Item = &'a String>) -> String {
    iter.next().cloned().unwrap_or_default()
}

/// 解析 --db 与 --migrations-dir（缺省 dev-p8.db / migrations）。
fn parse_db(args: &[String]) -> (PathBuf, PathBuf) {
    let mut db = default_db();
    let mut migrations_dir = PathBuf::from("migrations");
    let mut iter = args.iter();
    while let Some(flag) = iter.next() {
        match flag.as_str() {
            "--db" => db = PathBuf::from(flag_value(&mut iter)),
            "--migrations-dir" => migrations_dir = PathBuf::from(flag_value(&mut iter)),
            _ => {}
        }
    }
    (db, migrations_dir)
}

/// 打开可写库并跑迁移（001..017，幂等）；生产库守卫前置。
fn open_store(db: &Path, migrations_dir: &Path) -> WorkflowStore {
    guard_db_path(db);
    let pool = writable_pool(db).unwrap_or_else(|e| {
        eprintln!("open {}: {e}", db.display());
        exit(1);
    });
    let applied = migrate(&pool, migrations_dir).unwrap_or_else(|e| {
        eprintln!("migrate {}: {e}", db.display());
        exit(1);
    });
    if !applied.is_empty() {
        eprintln!(
            "{}",
            serde_json::json!({"code": "migrations_applied",
                   "count": applied.len(),
                   "latest": applied.last().map(|m| m.version)})
        );
    }
    WorkflowStore::new(pool)
}

/// 打开可写库池（执行面用；迁移与守卫同 open_store）。
fn open_pool(db: &Path, migrations_dir: &Path) -> Pool {
    guard_db_path(db);
    let pool = Pool::open(PoolConfig {
        path: DbPath::File(db.to_path_buf()),
        readonly: false,
        max_connections: 1,
        busy_timeout: std::time::Duration::from_secs(5),
    })
    .unwrap_or_else(|e| {
        eprintln!("open {}: {e}", db.display());
        exit(1);
    });
    let applied = migrate(&pool, migrations_dir).unwrap_or_else(|e| {
        eprintln!("migrate {}: {e}", db.display());
        exit(1);
    });
    if !applied.is_empty() {
        eprintln!(
            "{}",
            serde_json::json!({"code": "migrations_applied",
                   "count": applied.len(),
                   "latest": applied.last().map(|m| m.version)})
        );
    }
    pool
}

/// validate：装载门 + 静态校验，不改库。
fn cmd_validate(path: &str) -> i32 {
    let raw = match std::fs::read_to_string(path) {
        Ok(raw) => raw,
        Err(e) => {
            eprintln!("read {path}: {e}");
            return 2;
        }
    };
    let doc = match parse_definition(&raw) {
        Ok(doc) => doc,
        Err(e) => {
            eprintln!("{e}");
            return 2;
        }
    };
    let def = match workflow_engine::dsl::to_domain(&doc) {
        Ok(def) => def,
        Err(e) => {
            eprintln!("{e}");
            return 2;
        }
    };
    let issues = validate_definition(&def);
    if !issues.is_empty() {
        for issue in &issues {
            println!("{}", serde_json::json!({"kind": issue.kind.as_str(), "node": issue.node, "edge": issue.edge, "message": issue.message}));
        }
        println!(
            "{}",
            serde_json::json!({"result": "REJECTED", "issues": issues.len()})
        );
        return 1;
    }
    println!(
        "{}",
        serde_json::json!({
            "result": "PASSED",
            "workflow_id": doc.workflow_id,
            "version": doc.version,
            "compat": doc.compat.as_str(),
            "nodes": doc.graph.nodes.len(),
            "edges": doc.graph.edges.len(),
        })
    );
    0
}

/// put：四道门后追加一版定义。
fn cmd_put(args: &[String]) -> i32 {
    let path = &args[0];
    let (db, migrations_dir) = parse_db(args);
    let raw = match std::fs::read_to_string(path) {
        Ok(raw) => raw,
        Err(e) => {
            eprintln!("read {path}: {e}");
            return 2;
        }
    };
    let doc = match parse_definition(&raw) {
        Ok(doc) => doc,
        Err(e) => {
            eprintln!("{e}");
            return 2;
        }
    };
    let def = match workflow_engine::dsl::to_domain(&doc) {
        Ok(def) => def,
        Err(e) => {
            eprintln!("{e}");
            return 2;
        }
    };
    let issues = validate_definition(&def);
    if !issues.is_empty() {
        for issue in &issues {
            println!("{}", serde_json::json!({"kind": issue.kind.as_str(), "node": issue.node, "edge": issue.edge, "message": issue.message}));
        }
        println!(
            "{}",
            serde_json::json!({"result": "REJECTED", "issues": issues.len()})
        );
        return 1;
    }
    let store = open_store(&db, &migrations_dir);
    let outcome = store
        .put(&doc, "rp-workflow-put")
        .unwrap_or_else(|e| {
            eprintln!("{e}");
            exit(1);
        });
    println!(
        "{}",
        serde_json::json!({
            "result": "PUT",
            "workflow_id": outcome.workflow_id,
            "version": outcome.version,
            "compat": outcome.compat,
            "nodes": outcome.node_count,
            "edges": outcome.edge_count,
        })
    );
    0
}

/// get：回读定义原文（--version 缺省取最新版）。
fn cmd_get(args: &[String]) -> i32 {
    let workflow_id = &args[0];
    let (db, migrations_dir) = parse_db(args);
    let mut version: Option<u32> = None;
    let mut iter = args.iter().skip(1);
    while let Some(flag) = iter.next() {
        if flag.as_str() == "--version" {
            let raw = flag_value(&mut iter);
            match raw.parse::<u32>() {
                Ok(v) if v >= 1 => version = Some(v),
                _ => {
                    eprintln!("--version must be an integer >= 1, got {raw:?}");
                    return 2;
                }
            }
        }
    }
    let store = open_store(&db, &migrations_dir);
    let stored = store.get(workflow_id, version).unwrap_or_else(|e| {
        eprintln!("{e}");
        exit(1);
    });
    // 回读原文逐字节输出（含行尾，不改写不规整）。
    print!("{}", stored.definition_json);
    0
}

/// list：每 workflow_id 一行 + 汇总行。
fn cmd_list(args: &[String]) -> i32 {
    let (db, migrations_dir) = parse_db(args);
    let store = open_store(&db, &migrations_dir);
    let rows = store.list().unwrap_or_else(|e| {
        eprintln!("{e}");
        exit(1);
    });
    for row in &rows {
        println!(
            "{}",
            serde_json::json!({
                "workflow_id": row.workflow_id,
                "latest_version": row.latest_version,
                "version_count": row.version_count,
                "name": row.name,
                "compat": row.compat,
                "nodes": row.node_count,
                "edges": row.edge_count,
                "created_at": row.created_at,
            })
        );
    }
    println!(
        "{}",
        serde_json::json!({"total_workflows": rows.len(),
               "db": db.display().to_string()})
    );
    0
}

/// import-legacy：003 workflow 表存量行只读参考导入。
fn cmd_import_legacy(args: &[String]) -> i32 {
    let (db, migrations_dir) = parse_db(args);
    let mut src: Option<PathBuf> = None;
    let mut iter = args.iter();
    while let Some(flag) = iter.next() {
        if flag.as_str() == "--src" {
            src = Some(PathBuf::from(flag_value(&mut iter)));
        }
    }
    let Some(src) = src else {
        eprintln!("import-legacy requires --src <db> (003 形状源库)");
        return usage();
    };
    // --src 以只读池打开（SQLITE_OPEN_READ_ONLY，物理不可写），存量实查与
    // 生产库只读审计同口径，不适用 platform 守卫；守卫只拦可写 --db。
    let source = Pool::open(PoolConfig {
        path: DbPath::File(src.clone()),
        readonly: true,
        max_connections: 1,
        busy_timeout: std::time::Duration::from_secs(5),
    })
    .unwrap_or_else(|e| {
        eprintln!("open src {}: {e}", src.display());
        exit(1);
    });
    let store = open_store(&db, &migrations_dir);
    let report = store.import_legacy(&source).unwrap_or_else(|e| {
        eprintln!("{e}");
        exit(1);
    });
    println!(
        "{}",
        serde_json::json!({
            "result": "IMPORT-LEGACY",
            "source_rows": report.source_rows,
            "imported": report.imported,
            "skipped": report.skipped,
            "conclusion": report.conclusion,
        })
    );
    0
}

// ---------------------------------------------------------------------------
// P8-2 执行面（run/status/nodes/resume/retry/cancel）
// ---------------------------------------------------------------------------

/// CLI 运行旗标。
struct RunFlags {
    db: PathBuf,
    migrations_dir: PathBuf,
    version: Option<u32>,
    input: Option<String>,
    node_timeout_ms: u64,
    rulepack_dir: PathBuf,
    manifest_dirs: Vec<PathBuf>,
    node: Option<String>,
    run_id: Option<String>,
    workspace: String,
    max_tool_calls: usize,
    quiet: bool,
}

/// 运行面旗标解析（run/resume/retry/cancel/status/nodes 共用子集）。
fn parse_run_flags(args: &[String]) -> RunFlags {
    let (db, migrations_dir) = parse_db(args);
    let mut flags = RunFlags {
        db,
        migrations_dir,
        version: None,
        input: None,
        node_timeout_ms: DEFAULT_NODE_TIMEOUT_MS,
        rulepack_dir: PathBuf::from("rulepacks/rip-rules"),
        // rp-agent 同款默认（tools 目录 + 容器工具清单目录）。
        manifest_dirs: vec![
            PathBuf::from("manifests/tools"),
            PathBuf::from("containerfiles/rip-tool-webresearch"),
            PathBuf::from("containerfiles/rip-tool-coreutils"),
        ],
        node: None,
        run_id: None,
        workspace: "ws-workflow".to_string(),
        max_tool_calls: 4,
        quiet: false,
    };
    let mut iter = args.iter();
    while let Some(flag) = iter.next() {
        match flag.as_str() {
            "--input" => flags.input = Some(flag_value(&mut iter)),
            "--version" => {
                let raw = flag_value(&mut iter);
                match raw.parse::<u32>() {
                    Ok(v) if v >= 1 => flags.version = Some(v),
                    _ => {
                        eprintln!("--version must be an integer >= 1, got {raw:?}");
                        exit(2);
                    }
                }
            }
            "--node-timeout-ms" => match flag_value(&mut iter).parse::<u64>() {
                Ok(v) if v >= 1 => flags.node_timeout_ms = v,
                other => {
                    eprintln!("--node-timeout-ms must be an integer >= 1, got {other:?}");
                    exit(2);
                }
            },
            "--rulepack-dir" => flags.rulepack_dir = PathBuf::from(flag_value(&mut iter)),
            "--manifest-dirs" => {
                flags.manifest_dirs = flag_value(&mut iter)
                    .split(',')
                    .map(PathBuf::from)
                    .collect();
            }
            "--node" => flags.node = Some(flag_value(&mut iter)),
            "--run-id" => flags.run_id = Some(flag_value(&mut iter)),
            "--workspace" => flags.workspace = flag_value(&mut iter),
            "--max-tool-calls" => match flag_value(&mut iter).parse::<usize>() {
                Ok(v) if v >= 1 => flags.max_tool_calls = v,
                other => {
                    eprintln!("--max-tool-calls must be an integer >= 1, got {other:?}");
                    exit(2);
                }
            },
            "--quiet" => flags.quiet = true,
            _ => {}
        }
    }
    flags
}

/// Tool 节点真实运行面：按 tool_run 行回查 tool_id，逐工具从 Manifest
/// runtime 块解析 python_worker 适配器（rp-agent resolve_python_worker
/// 同款口径：argv 固定清单不经 shell，repo:// 前缀落仓库根）。
struct PythonWorkerToolDriver {
    pool: Pool,
    manifest_dirs: Vec<PathBuf>,
}

impl ToolRunDriver for PythonWorkerToolDriver {
    fn drive(
        &self,
        engine: &tool_engine::Engine,
        run_id: &str,
    ) -> Result<tool_engine::DriveOutcome, tool_engine::EngineError> {
        let tool_id: String = self
            .pool
            .with_conn(|c| {
                c.query_row(
                    "SELECT tool_id FROM tool_run WHERE run_id = ?1",
                    [run_id],
                    |r| r.get(0),
                )
                .map_err(database::DbError::Sqlite)
            })
            .map_err(|e| tool_engine::EngineError::Storage(format!("tool_run lookup failed: {e}")))?;
        let adapter = resolve_python_worker(&self.manifest_dirs, &tool_id)
            .map_err(tool_engine::EngineError::Storage)?;
        execute_via_engine(AdapterCall {
            engine,
            run_id,
            adapter: &adapter,
            cancel_after: None,
        })
        .map_err(|failure| {
            tool_engine::EngineError::Storage(format!("python_worker dispatch failed: {failure}"))
        })
    }
}

/// Agent 节点真实运行面（P8-3）：Phase 7 agent-engine AgentLoop 真实闭环。
/// fresh run / PAUSED 续接按 agent_run 行现态分派（引擎预生成 agent_run_id
/// 已落节点链接列，本驱动以其 id 落 agent_run 行，trace_id = workflow
/// run_id 落 015 链路）；控制面复用 agent-engine RunControl（pause 请求位
/// 由 rp-workflow pause 经 WorkflowControl 传播，agent 循环步间消费）。
struct AgentLoopDriver {
    pool: Pool,
    rulepack_dir: PathBuf,
    manifest_dirs: Vec<PathBuf>,
    workspace: String,
    max_tool_calls: usize,
}

impl node_agent::AgentNodeDriver for AgentLoopDriver {
    fn execute(&self, request: &node_agent::AgentNodeRequest) -> node_agent::AgentNodeOutcome {
        match self.execute_inner(request) {
            Ok(outcome) => outcome,
            Err(e) => node_agent::AgentNodeOutcome::Failed(
                workflow_engine::exec::NodeFailure::new("agent_failed", e),
            ),
        }
    }
}

impl AgentLoopDriver {
    fn execute_inner(
        &self,
        request: &node_agent::AgentNodeRequest,
    ) -> Result<node_agent::AgentNodeOutcome, String> {
        let registry = tool_registry::Registry::open(self.pool.clone());
        let store = AgentStore::open(self.pool.clone(), registry.clone());
        // fresh run vs PAUSED 续接：按 agent_run 行现态分派（引擎钉定 id）。
        let existing = store
            .run(&request.agent_run_id)
            .map_err(|e| format!("agent run lookup failed: {e}"))?;
        // 目标 agent 定义 → 首个能力工具（驱动面按绑定首工具组装，rp-agent
        // 同款口径）。
        let definition = match &request.agent_version {
            Some(v) => store.definition(&request.agent_id, v),
            None => store.latest_definition(&request.agent_id),
        }
        .map_err(|e| format!("definition lookup failed: {e}"))?
        .ok_or_else(|| {
            format!(
                "definition {:?} not found (register via rp-agent def-add)",
                request.agent_id
            )
        })?;
        let first_tool = definition
            .definition
            .capability_refs()
            .first()
            .map(|r| r.tool_id().to_string())
            .ok_or_else(|| "agent definition has empty capability_refs".to_string())?;
        let adapter = resolve_python_worker(&self.manifest_dirs, &first_tool)?;
        let engine = tool_engine::Engine::open(self.pool.clone(), registry);
        let tool_link = ToolLink::new(engine, Box::new(PythonWorkerDriver { adapter }));
        let broker_link = self.build_broker_link();
        let evidence = EvidenceRegistrar::new(self.pool.clone());
        let audit = Some(audit_engine::AuditEngine::new(self.pool.clone()));
        let loop_ = AgentLoop::new(
            store,
            broker_link,
            tool_link,
            evidence,
            audit,
            LoopConfig {
                workspace_id: request.workspace_id.clone().unwrap_or_else(|| self.workspace.clone()),
                max_tool_calls: self.max_tool_calls,
            },
        );
        let report = match existing {
            Some(stored) if stored.run.state() == domain::AgentRunState::Paused => {
                // PAUSED 续接：目标文本从 014 goal_text 续接，已完成步骤不重跑。
                let mut req = agent_engine::agent_loop::ResumeRequest {
                    run_id: request.agent_run_id.clone(),
                    planner: Box::new(agent_engine::KeywordSearchPlanner),
                    confirmer: Box::new(self.build_confirmer()?),
                };
                loop_.resume(&mut req).map_err(|e| format!("agent resume failed: {e}"))?
            }
            _ => {
                // fresh run：run_id 钉定（受理面以此 id 落行）。
                let mut req = agent_engine::agent_loop::RunRequest {
                    agent_id: request.agent_id.clone(),
                    agent_version: request.agent_version.clone(),
                    goal: request.goal.clone(),
                    task_id: request.task_id.clone(),
                    trace_id: Some(request.run_id.clone()),
                    run_id: Some(request.agent_run_id.clone()),
                    input: None,
                    planner: Box::new(agent_engine::KeywordSearchPlanner),
                    confirmer: Box::new(self.build_confirmer()?),
                };
                loop_.run(&mut req).map_err(|e| format!("agent run failed: {e}"))?
            }
        };
        match report.state.as_str() {
            "SUCCEEDED" => Ok(node_agent::AgentNodeOutcome::Succeeded {
                output: node_agent::report_projection(
                    &report.run_id,
                    &report.state,
                    &report.finish_reason,
                    report.steps.len(),
                    &report.warnings,
                    &report.manifest_id,
                    &report.intent_status,
                ),
            }),
            "PAUSED" => Ok(node_agent::AgentNodeOutcome::Paused {
                agent_run_id: report.run_id.clone(),
            }),
            other => Err(format!(
                "agent run {} ended {} (finish_reason: {})",
                report.run_id, other, report.finish_reason
            )),
        }
    }

    fn build_confirmer(&self) -> Result<FirstFamilyConfirmer, String> {
        let intent_config = context_broker::IntentConfig::from_dir(&self.rulepack_dir)
            .map_err(|e| format!("intent config: {e}"))?;
        Ok(FirstFamilyConfirmer::new(intent_config))
    }

    /// rp-agent build_broker_link 同款装配（rulepack + registry + artifact +
    /// 库内 plugin 全源 + 缓存 + 审计 + 引用证据固化）。
    fn build_broker_link(&self) -> BrokerLink {
        use context_broker::{
            ArtifactSource, Broker as CbBroker, BrokerCache, BudgetPolicy, ManifestStore,
            PluginSource, RegistrySource, RulepackSource,
        };
        let cache_dir = std::env::var("RIP_DATA_DIR")
            .map(|d| PathBuf::from(d).join("runtime").join("broker-cache"))
            .unwrap_or_else(|_| BrokerCache::default_dir());
        let cache = Arc::new(BrokerCache::open(cache_dir).unwrap_or_else(|e| {
            eprintln!("open cache: {e}");
            exit(1);
        }));
        let rulepack = RulepackSource::from_dir(&self.rulepack_dir).unwrap_or_else(|e| {
            eprintln!("rulepack: {e}");
            exit(1);
        });
        let intent_config = context_broker::IntentConfig::from_dir(&self.rulepack_dir)
            .unwrap_or_else(|e| {
                eprintln!("intent config: {e}");
                exit(1);
            });
        let index = context_broker::ManifestIndex::from_dirs(&self.manifest_dirs)
            .unwrap_or_else(|e| {
                eprintln!("manifest index: {e}");
                exit(1);
            });
        let registry_source = RegistrySource::new(
            tool_registry::Registry::open(self.pool.clone()),
            index,
        )
        .with_cache(cache.clone(), "policy-1");
        let mut sources: Vec<Box<dyn context_broker::ContextSource>> = vec![
            Box::new(rulepack),
            Box::new(registry_source),
            Box::new(ArtifactSource::new(self.pool.clone())),
        ];
        for plugin_source in PluginSource::mount_all(&self.pool) {
            sources.push(Box::new(plugin_source));
        }
        let broker = CbBroker::new(
            sources,
            BudgetPolicy::default_for(8000).unwrap_or_else(|e| {
                eprintln!("policy: {e}");
                exit(1);
            }),
            intent_config,
            ManifestStore::new(self.pool.clone()),
            Some(audit_engine::AuditEngine::new(self.pool.clone())),
        )
        .with_cache(cache)
        .with_evidence_store(resource_engine::sqlite::SqliteResourceRepository::new(
            self.pool.clone(),
        ));
        BrokerLink::new(broker)
    }
}

/// 按 tool id 从 Manifest 目录解析 python_worker runtime 块（文件面登记件，
/// tool 表不存 runtime 块；rp-agent 同款）。
fn resolve_python_worker(
    manifest_dirs: &[PathBuf],
    tool_id: &str,
) -> Result<PythonWorkerAdapter, String> {
    for dir in manifest_dirs {
        let entries = match std::fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => continue,
        };
        let mut paths: Vec<PathBuf> = entries
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.extension().map(|x| x == "json").unwrap_or(false))
            .collect();
        paths.sort();
        for path in paths {
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else {
                continue;
            };
            if value.get("id").and_then(|v| v.as_str()) != Some(tool_id) {
                continue;
            }
            let runtime = value
                .get("runtime")
                .ok_or_else(|| format!("manifest {tool_id} has no runtime block"))?;
            let rtype = runtime.get("type").and_then(|v| v.as_str()).unwrap_or("");
            if rtype != "python_worker" {
                return Err(format!(
                    "tool {tool_id} runtime type {rtype:?} is not python_worker; \
                     rp-workflow CLI 基线仅承载 python_worker（P8-2）"
                ));
            }
            let worker_name = runtime
                .get("worker_name")
                .and_then(|v| v.as_str())
                .unwrap_or("rip-cli-worker")
                .to_string();
            let command: Vec<String> = runtime
                .get("command")
                .and_then(|v| v.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|x| x.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default();
            if command.is_empty() {
                return Err(format!("manifest {tool_id} runtime.command is empty"));
            }
            let working_dir = std::env::current_dir().ok();
            return Ok(PythonWorkerAdapter::new(PythonWorkerConfig {
                worker_name,
                command,
                working_dir,
                restart: RestartPolicy::Pooled,
                extra_env: Vec::new(),
                handshake_timeout: std::time::Duration::from_secs(10),
            }));
        }
    }
    Err(format!(
        "manifest for {tool_id} not found in {manifest_dirs:?}（--manifest-dirs 可覆盖）"
    ))
}

/// Knowledge 节点检索面：rp-agent build_broker_link 同款装配（rulepack +
/// registry + artifact + 库内 plugin 全源 + 缓存 + 审计 + 引用证据固化）。
fn build_broker(flags: &RunFlags, pool: &Pool) -> Arc<context_broker::Broker> {
    use context_broker::{ArtifactSource, Broker, BrokerCache, IntentConfig, ManifestStore, PluginSource, RegistrySource, RulepackSource};
    let cache_dir = std::env::var("RIP_DATA_DIR")
        .map(|d| PathBuf::from(d).join("runtime").join("broker-cache"))
        .unwrap_or_else(|_| BrokerCache::default_dir());
    let cache = Arc::new(BrokerCache::open(cache_dir).unwrap_or_else(|e| {
        eprintln!("open cache: {e}");
        exit(1);
    }));
    let rulepack = RulepackSource::from_dir(&flags.rulepack_dir).unwrap_or_else(|e| {
        eprintln!("rulepack: {e}");
        exit(1);
    });
    let intent_config = IntentConfig::from_dir(&flags.rulepack_dir).unwrap_or_else(|e| {
        eprintln!("intent config: {e}");
        exit(1);
    });
    let index = context_broker::ManifestIndex::from_dirs(&flags.manifest_dirs)
        .unwrap_or_else(|e| {
            eprintln!("manifest index: {e}");
            exit(1);
        });
    let registry_source =
        RegistrySource::new(tool_registry::Registry::open(pool.clone()), index)
            .with_cache(cache.clone(), "policy-1");
    let mut sources: Vec<Box<dyn context_broker::ContextSource>> = vec![
        Box::new(rulepack),
        Box::new(registry_source),
        Box::new(ArtifactSource::new(pool.clone())),
    ];
    for plugin_source in PluginSource::mount_all(pool) {
        sources.push(Box::new(plugin_source));
    }
    let broker = Broker::new(
        sources,
        context_broker::BudgetPolicy::default_for(8000).unwrap_or_else(|e| {
            eprintln!("policy: {e}");
            exit(1);
        }),
        intent_config,
        ManifestStore::new(pool.clone()),
        Some(audit_engine::AuditEngine::new(pool.clone())),
    )
    .with_cache(cache)
    .with_evidence_store(resource_engine::sqlite::SqliteResourceRepository::new(
        pool.clone(),
    ));
    Arc::new(broker)
}

/// 执行器装配（Tool 派发链 + Knowledge 检索面 + Agent 闭环 + 节点超时）。
fn build_executor(flags: &RunFlags, pool: Pool) -> WorkflowExecutor {
    let driver = Arc::new(PythonWorkerToolDriver {
        pool: pool.clone(),
        manifest_dirs: flags.manifest_dirs.clone(),
    });
    let agent_driver = Arc::new(AgentLoopDriver {
        pool: pool.clone(),
        rulepack_dir: flags.rulepack_dir.clone(),
        manifest_dirs: flags.manifest_dirs.clone(),
        workspace: flags.workspace.clone(),
        max_tool_calls: flags.max_tool_calls,
    });
    let broker = build_broker(flags, &pool);
    WorkflowExecutor::new(pool)
        .with_tool_driver(driver)
        .with_agent_driver(agent_driver)
        .with_broker(broker)
        .with_node_timeout_ms(flags.node_timeout_ms)
}

/// run 输入解析（--input 缺省 = 空对象；非法 JSON exit 2）。
fn parse_input(flags: &RunFlags) -> serde_json::Value {
    match &flags.input {
        None => serde_json::json!({}),
        Some(raw) => match serde_json::from_str(raw) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("--input is not valid JSON: {e}");
                exit(2);
            }
        },
    }
}

/// 终态结果输出（run/resume/retry 共用）：outcome 单行 JSON 到 stdout，
/// 返回 exit 码（SUCCEEDED = 0，FAILED/CANCELLED = 1）。
fn print_outcome(outcome: &RunOutcome) -> i32 {
    let nodes: Vec<serde_json::Value> = outcome
        .nodes
        .iter()
        .map(|n| {
            serde_json::json!({
                "node_id": n.node_id,
                "node_kind": n.node_kind,
                "state": n.state.as_str(),
                "attempt": n.attempt,
            })
        })
        .collect();
    println!(
        "{}",
        serde_json::json!({
            "result": "RUN-OUTCOME",
            "run_id": outcome.run_id,
            "state": outcome.state.as_str(),
            "nodes": nodes,
        })
    );
    match outcome.state {
        // Paused（协作暂停 / 人工介入 awaiting_approval 停等）非错误：run 到
        // 达稳定停等态，exit 0（P8-3 口径）。
        domain::WorkflowRunState::Succeeded | domain::WorkflowRunState::Paused => 0,
        _ => 1,
    }
}

/// 受理错误 exit 映射：InvalidInput / NotFound / Parse / Schema = 2；
/// 其余（存储 / 非预期）= 1。
fn exit_code_of(err: &WorkflowError) -> i32 {
    match err {
        WorkflowError::InvalidInput(_)
        | WorkflowError::NotFound { .. }
        | WorkflowError::Parse(_)
        | WorkflowError::Schema(_)
        | WorkflowError::RunNotFound { .. } => 2,
        _ => 1,
    }
}

/// run：受理门后执行到终态（--run-id 钉定 run id，API 写面异步承载用）。
fn cmd_run(args: &[String]) -> i32 {
    let flags = parse_run_flags(args);
    let Some(workflow_id) = args.first().cloned() else {
        return usage();
    };
    let input = parse_input(&flags);
    let pool = open_pool(&flags.db, &flags.migrations_dir);
    let executor = build_executor(&flags, pool);
    match executor.start_with_id(&workflow_id, flags.version, &input, flags.run_id.as_deref()) {
        Ok(outcome) => print_outcome(&outcome),
        Err(err) => {
            eprintln!("{err}");
            exit_code_of(&err)
        }
    }
}

/// status：run 头行投影。
fn cmd_status(args: &[String]) -> i32 {
    let flags = parse_run_flags(args);
    let Some(run_id) = args.first().cloned() else {
        return usage();
    };
    let pool = open_pool(&flags.db, &flags.migrations_dir);
    let executor = WorkflowExecutor::new(pool);
    match executor.status(&run_id) {
        Ok((row, nodes)) => {
            let awaiting = executor
                .awaiting_approval(&run_id)
                .ok()
                .flatten()
                .map(|r| {
                    serde_json::json!({
                        "node_id": r.approval_node_id,
                        "request": r.approval_request(),
                    })
                })
                .unwrap_or(serde_json::Value::Null);
            println!(
                "{}",
                serde_json::json!({
                    "run_id": row.run_id,
                    "workflow_id": row.workflow_id,
                    "workflow_version": row.workflow_version,
                    "state": row.state.as_str(),
                    "input": row.input,
                    "output": row.output,
                    "error": row.error,
                    "created_at": row.created_at,
                    "started_at": row.started_at,
                    "finished_at": row.finished_at,
                    "correlation": {
                        "trace_id": row.trace_id,
                        "task_id": row.task_id,
                        "agent_run_id": row.agent_run_id,
                        "tool_run_id": row.tool_run_id,
                        "artifact_id": row.artifact_id,
                        "evidence_id": row.evidence_id,
                    },
                    "awaiting_approval": awaiting,
                    "node_count": nodes.len(),
                    "succeeded": nodes.iter().filter(|n| n.state == domain::WorkflowNodeRunState::Succeeded).count(),
                    "failed": nodes.iter().filter(|n| n.state == domain::WorkflowNodeRunState::Failed).count(),
                    "skipped": nodes.iter().filter(|n| n.state == domain::WorkflowNodeRunState::Skipped).count(),
                    "pending": nodes.iter().filter(|n| n.state == domain::WorkflowNodeRunState::Pending).count(),
                    "running": nodes.iter().filter(|n| n.state == domain::WorkflowNodeRunState::Running).count(),
                })
            );
            0
        }
        Err(err) => {
            eprintln!("{err}");
            exit_code_of(&err)
        }
    }
}

/// nodes：节点状态行 + checkpoint 汇总。
fn cmd_nodes(args: &[String]) -> i32 {
    let flags = parse_run_flags(args);
    let Some(run_id) = args.first().cloned() else {
        return usage();
    };
    let pool = open_pool(&flags.db, &flags.migrations_dir);
    let executor = WorkflowExecutor::new(pool);
    let runs = workflow_engine::RunStore::new(match Pool::open(PoolConfig {
        path: DbPath::File(flags.db.clone()),
        readonly: false,
        max_connections: 1,
        busy_timeout: std::time::Duration::from_secs(5),
    }) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("open: {e}");
            exit(1);
        }
    });
    match executor.status(&run_id) {
        Ok((_row, nodes)) => {
            for n in &nodes {
                println!(
                    "{}",
                    serde_json::json!({
                        "run_id": run_id,
                        "node_id": n.node_id,
                        "node_kind": n.node_kind,
                        "state": n.state.as_str(),
                        "attempt": n.attempt,
                        "output": n.output,
                        "error": n.error,
                        "started_at": n.started_at,
                        "finished_at": n.finished_at,
                        "tool_run_id": n.tool_run_id,
                        "agent_run_id": n.agent_run_id,
                    })
                );
            }
            let checkpoints = runs
                .latest_checkpoint(&run_id)
                .map(|cp| cp.map(|c| c.seq).unwrap_or(0))
                .unwrap_or(0);
            println!(
                "{}",
                serde_json::json!({
                    "run_id": run_id,
                    "total_nodes": nodes.len(),
                    "latest_checkpoint_seq": checkpoints,
                })
            );
            0
        }
        Err(err) => {
            eprintln!("{err}");
            exit_code_of(&err)
        }
    }
}

/// pause：协作式暂停（请求位 + agent run 014 控制行传播；等完成再
/// checkpoint：run 循环波间消费，当前节点完成并落 checkpoint 后转 paused）。
fn cmd_pause(args: &[String]) -> i32 {
    let flags = parse_run_flags(args);
    let Some(run_id) = args.first().cloned() else {
        return usage();
    };
    let pool = open_pool(&flags.db, &flags.migrations_dir);
    let executor = WorkflowExecutor::new(pool);
    match executor.pause(&run_id) {
        Ok(state) => {
            println!(
                "{}",
                serde_json::json!({"result": "PAUSE-REQUESTED", "run_id": run_id,
                       "state_at_request": state.as_str()})
            );
            if state.is_terminal() {
                1
            } else {
                0
            }
        }
        Err(err) => {
            eprintln!("{err}");
            exit_code_of(&err)
        }
    }
}

/// resume：断点恢复（已完成节点不重跑）；--input 注入批准/注入参数
/// （P8-3 人工介入 awaiting_approval 的批准通道：参数浅并入恢复态并落
/// 018 控制行 approval_json 留痕）。
fn cmd_resume(args: &[String]) -> i32 {
    let flags = parse_run_flags(args);
    let Some(run_id) = args.first().cloned() else {
        return usage();
    };
    let inject = match &flags.input {
        None => None,
        Some(raw) => match serde_json::from_str::<serde_json::Value>(raw) {
            Ok(v) if v.is_object() => Some(v),
            Ok(v) => {
                eprintln!("--input injection must be a JSON object, got {}", v);
                return 2;
            }
            Err(e) => {
                eprintln!("--input is not valid JSON: {e}");
                return 2;
            }
        },
    };
    let pool = open_pool(&flags.db, &flags.migrations_dir);
    let executor = build_executor(&flags, pool);
    match executor.resume_with_injection(&run_id, inject.as_ref()) {
        Ok(outcome) => print_outcome(&outcome),
        Err(err) => {
            eprintln!("{err}");
            exit_code_of(&err)
        }
    }
}

/// retry：失败重试（新 run 承载，只重跑目标节点）。
fn cmd_retry(args: &[String]) -> i32 {
    let flags = parse_run_flags(args);
    let Some(run_id) = args.first().cloned() else {
        return usage();
    };
    let Some(node_id) = flags.node.clone() else {
        eprintln!("retry requires --node <node_id>");
        return usage();
    };
    let pool = open_pool(&flags.db, &flags.migrations_dir);
    let executor = build_executor(&flags, pool);
    match executor.retry_with_id(&run_id, &node_id, flags.run_id.as_deref()) {
        Ok(outcome) => print_outcome(&outcome),
        Err(err) => {
            eprintln!("{err}");
            exit_code_of(&err)
        }
    }
}

/// cancel：协作式取消。
fn cmd_cancel(args: &[String]) -> i32 {
    let flags = parse_run_flags(args);
    let Some(run_id) = args.first().cloned() else {
        return usage();
    };
    let pool = open_pool(&flags.db, &flags.migrations_dir);
    let executor = WorkflowExecutor::new(pool);
    match executor.cancel(&run_id) {
        Ok(state) => {
            println!(
                "{}",
                serde_json::json!({"result": "CANCELLED", "run_id": run_id, "state": state.as_str()})
            );
            if state == domain::WorkflowRunState::Cancelled {
                0
            } else {
                1
            }
        }
        Err(err) => {
            eprintln!("{err}");
            exit_code_of(&err)
        }
    }
}
