//! rp-agent CLI（ROADMAP §8.4 验收命令基线，P7-2 定稿）：
//!
//! ~~~text
//! cargo run --offline -p agent-engine --bin rp-agent -- run <agent_id> --goal "<目标文本>" \
//!   [--input <json>] [--task-id <id>] [--agent-version <v>] [--db <path>]
//! cargo run --offline -p agent-engine --bin rp-agent -- def-add --agent-id <id> --version <v> \
//!   --name <n> [--caps "tool@ver,tool2"] [--policy <json>] [--db <path>]
//! cargo run --offline -p agent-engine --bin rp-agent -- def-show <agent_id> [--version <v>] [--db <path>]
//! cargo run --offline -p agent-engine --bin rp-agent -- run-show <run_id> [--db <path>]
//! ~~~
//!
//! 默认面（仓库根运行）：--db migrations/dev-p7.db（开发库副本；生产
//! platform*.db 一律拒接，红线同 rp-knowledge-fetch guard）。run 前对本库
//! 幂等执行迁移；Broker 源接 rulepack + registry（Manifest 目录索引）+
//! artifact + plugin（SK-1..3 知识源逐插件挂载），证据固化写面随装配
//! （SK-1/P6-9）。工具驱动走真实 python_worker Adapter（argv 固定清单，
//! 不经 shell；Manifest runtime 块来自 --manifest-dirs 文件面）。
//!
//! 结束报告单行 JSON 进 stdout；run 终态 SUCCEEDED 退出 0，FAILED 退出 1。

use agent_engine::agent_loop::RunRequest;
use agent_engine::broker_link::{BrokerLink, FirstFamilyConfirmer};
use agent_engine::evidence::EvidenceRegistrar;
use agent_engine::store::AgentStore;
use agent_engine::tool_link::{PythonWorkerDriver, ToolLink};
use agent_engine::LoopConfig;
use context_broker::{
    ArtifactSource, Broker, BrokerCache, IntentConfig, ManifestStore, PluginSource, RegistrySource,
    RulepackSource,
};
use database::{DbPath, Pool, PoolConfig};
use std::path::{Path, PathBuf};
use std::process::exit;
use std::sync::Arc;
use std::time::Duration;
use tool_engine::adapter::python_worker::{PythonWorkerAdapter, PythonWorkerConfig, RestartPolicy};
use tool_engine::Engine;

fn main() {
    exit(run());
}

fn usage() -> i32 {
    eprintln!(
        "usage: rp-agent <sub> [args] [flags]\n\
         \n\
         subcommands:\n\
         \x20 run <agent_id> --goal <text> [--input <json>] [--task-id <id>] [--agent-version <v>]\n\
         \x20   [--db <path>] [--budget <n>] [--workspace <id>] [--artifacts-dir <dir>]\n\
         \x20   [--max-tool-calls <n>] [--max-confirm-rounds <n>] [--rulepack-dir <dir>]\n\
         \x20   [--manifest-dirs <csv>] [--migrations-dir <dir>] [--out <path>]\n\
         \x20 def-add --agent-id <id> --version <v> --name <n> [--caps \"tool@ver,tool2\"]\n\
         \x20   [--policy <json>] [--db <path>]\n\
         \x20 def-show <agent_id> [--version <v>] [--db <path>]\n\
         \x20 run-show <run_id> [--db <path>]"
    );
    2
}

struct Flags {
    db: PathBuf,
    migrations_dir: PathBuf,
    goal: String,
    task_id: Option<String>,
    input: Option<serde_json::Value>,
    agent_version: Option<String>,
    budget: u64,
    workspace: String,
    artifacts_dir: Option<PathBuf>,
    max_tool_calls: usize,
    max_confirm_rounds: usize,
    rulepack_dir: PathBuf,
    manifest_dirs: Vec<PathBuf>,
    out: Option<PathBuf>,
    policy: String,
    caps: String,
}

fn parse_flags(args: &[String]) -> Flags {
    let mut flags = Flags {
        db: PathBuf::from("migrations/dev-p7.db"),
        migrations_dir: PathBuf::from("migrations"),
        goal: String::new(),
        task_id: None,
        input: None,
        agent_version: None,
        budget: 8000,
        workspace: "ws-default".to_string(),
        artifacts_dir: None,
        max_tool_calls: 4,
        max_confirm_rounds: 2,
        rulepack_dir: PathBuf::from("rulepacks/rip-rules"),
        manifest_dirs: vec![
            PathBuf::from("manifests/tools"),
            PathBuf::from("containerfiles/rip-tool-webresearch"),
            PathBuf::from("containerfiles/rip-tool-coreutils"),
        ],
        out: None,
        policy: "{}".to_string(),
        caps: String::new(),
    };
    let mut iter = args.iter();
    while let Some(flag) = iter.next() {
        let value = |iter: &mut std::slice::Iter<String>| iter.next().cloned().unwrap_or_default();
        match flag.as_str() {
            "--db" => flags.db = PathBuf::from(value(&mut iter)),
            "--migrations-dir" => flags.migrations_dir = PathBuf::from(value(&mut iter)),
            "--goal" => flags.goal = value(&mut iter),
            "--task-id" => flags.task_id = Some(value(&mut iter)),
            "--input" => {
                flags.input = serde_json::from_str(&value(&mut iter)).ok();
            }
            "--agent-version" => flags.agent_version = Some(value(&mut iter)),
            "--budget" => flags.budget = value(&mut iter).parse().unwrap_or(8000),
            "--workspace" => flags.workspace = value(&mut iter),
            "--artifacts-dir" => flags.artifacts_dir = Some(PathBuf::from(value(&mut iter))),
            "--max-tool-calls" => {
                flags.max_tool_calls = value(&mut iter).parse().unwrap_or(4);
            }
            "--max-confirm-rounds" => {
                flags.max_confirm_rounds = value(&mut iter).parse().unwrap_or(2);
            }
            "--rulepack-dir" => flags.rulepack_dir = PathBuf::from(value(&mut iter)),
            "--manifest-dirs" => {
                flags.manifest_dirs = value(&mut iter).split(',').map(PathBuf::from).collect();
            }
            "--out" => flags.out = Some(PathBuf::from(value(&mut iter))),
            "--policy" => flags.policy = value(&mut iter),
            "--caps" => flags.caps = value(&mut iter),
            _ => {}
        }
    }
    flags
}

/// 生产库守卫（红线：platform*.db 一律不接，开发副本/临时文件才放行）。
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

fn open_pool(db: &Path, migrations_dir: &Path) -> Pool {
    guard_db_path(db);
    let pool = Pool::open(PoolConfig {
        path: DbPath::File(db.to_path_buf()),
        readonly: false,
        max_connections: 1,
        busy_timeout: Duration::from_secs(5),
    })
    .unwrap_or_else(|e| {
        eprintln!("open {}: {e}", db.display());
        exit(1);
    });
    database::migrate(&pool, migrations_dir).unwrap_or_else(|e| {
        eprintln!("migrate {}: {e}", migrations_dir.display());
        exit(1);
    });
    pool
}

fn build_broker_link(flags: &Flags, pool: &Pool) -> BrokerLink {
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
    let index =
        context_broker::ManifestIndex::from_dirs(&flags.manifest_dirs).unwrap_or_else(|e| {
            eprintln!("manifest index: {e}");
            exit(1);
        });
    let registry_source = RegistrySource::new(tool_registry::Registry::open(pool.clone()), index)
        .with_cache(cache.clone(), "policy-1");
    // SK-1..3（011/012 迁移）：库内插件逐个挂 plugin:<id> 知识源（L2 摘要
    // + L3 expand；禁用/未审插件候选空集合法，准入闸逐查询重读）。
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
        context_broker::BudgetPolicy::default_for(flags.budget).unwrap_or_else(|e| {
            eprintln!("policy: {e}");
            exit(1);
        }),
        intent_config.clone(),
        ManifestStore::new(pool.clone()),
        Some(audit_engine::AuditEngine::new(pool.clone())),
    )
    .with_cache(cache)
    .with_evidence_store(resource_engine::sqlite::SqliteResourceRepository::new(
        pool.clone(),
    ));
    BrokerLink::new(broker).with_max_confirm_rounds(flags.max_confirm_rounds)
}

/// 解析 capability refs 文本（`tool@ver,tool2`；无 @ 即 current）。
fn parse_caps(caps: &str) -> Result<Vec<domain::CapabilityRef>, String> {
    caps.split(',')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| {
            if let Some((tool, ver)) = s.split_once('@') {
                domain::CapabilityRef::try_new(tool.trim(), Some(ver.trim().to_string()))
                    .map_err(|e| e.to_string())
            } else {
                domain::CapabilityRef::try_new(s, None).map_err(|e| e.to_string())
            }
        })
        .collect()
}

/// 按 tool id 从 Manifest 目录解析 python_worker runtime 块（文件面登记件，
/// tool 表不存 runtime 块；argv 固定清单不经 shell）。
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
                    "tool {tool_id} runtime type {rtype:?} is not python_worker; rp-agent CLI 基线仅承载 python_worker（P7-2）"
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
            // repo:// 前缀落到仓库根（本 CLI 约定从仓库根运行，即 CWD）。
            let working_dir = std::env::current_dir().ok();
            return Ok(PythonWorkerAdapter::new(PythonWorkerConfig {
                worker_name,
                command,
                working_dir,
                restart: RestartPolicy::Pooled,
                extra_env: Vec::new(),
                handshake_timeout: Duration::from_secs(10),
            }));
        }
    }
    Err(format!(
        "manifest for {tool_id} not found in {:?}（--manifest-dirs 可覆盖）",
        manifest_dirs
    ))
}

fn run() -> i32 {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(sub) = args.first().cloned() else {
        return usage();
    };
    let flags = parse_flags(&args[1..]);
    match sub.as_str() {
        "def-add" => {
            let Some(agent_id) = flag_value(&args[1..], "--agent-id") else {
                eprintln!("def-add requires --agent-id");
                return usage();
            };
            let Some(version) = flag_value(&args[1..], "--version") else {
                eprintln!("def-add requires --version");
                return usage();
            };
            let Some(name) = flag_value(&args[1..], "--name") else {
                eprintln!("def-add requires --name");
                return usage();
            };
            let pool = open_pool(&flags.db, &flags.migrations_dir);
            let store = AgentStore::open(pool.clone(), tool_registry::Registry::open(pool));
            let refs = match parse_caps(&flags.caps) {
                Ok(refs) => refs,
                Err(e) => {
                    eprintln!("caps parse: {e}");
                    return 1;
                }
            };
            let agent_id_newtype = match domain::AgentId::try_new(agent_id.clone()) {
                Ok(id) => id,
                Err(e) => {
                    eprintln!("agent id invalid: {e}");
                    return 1;
                }
            };
            let definition = match domain::AgentDefinition::try_new(
                agent_id_newtype,
                version.clone(),
                flags.policy.clone(),
                refs,
            ) {
                Ok(d) => d,
                Err(e) => {
                    eprintln!("definition invalid: {e}");
                    return 1;
                }
            };
            match store.register_definition(&definition, &name) {
                Ok(()) => {
                    println!(
                        "{}",
                        serde_json::json!({
                            "command": "def-add",
                            "agent_id": agent_id,
                            "version": version,
                            "name": name,
                            "capability_refs": flags.caps,
                            "db": flags.db.display().to_string(),
                        })
                    );
                    0
                }
                Err(e) => {
                    eprintln!("def-add rejected: {e}");
                    1
                }
            }
        }
        "def-show" => {
            let agent_id = args.iter().skip(1).find(|a| !a.starts_with("--")).cloned();
            let Some(agent_id) = agent_id else {
                eprintln!("def-show requires <agent_id>");
                return usage();
            };
            let pool = open_pool(&flags.db, &flags.migrations_dir);
            let store = AgentStore::open(pool.clone(), tool_registry::Registry::open(pool));
            let stored = match flag_value(&args[1..], "--version") {
                Some(v) => store.definition(&agent_id, &v),
                None => store.latest_definition(&agent_id),
            };
            match stored {
                Ok(Some(s)) => {
                    println!(
                        "{}",
                        serde_json::json!({
                            "agent_id": s.definition.agent_id().as_str(),
                            "version": s.definition.version(),
                            "name": s.name,
                            "context_policy": s.definition.context_policy(),
                            "capability_refs": s.definition.capability_refs().iter().map(|r| {
                                serde_json::json!({"tool_id": r.tool_id(), "version": r.version()})
                            }).collect::<Vec<_>>(),
                            "created_at": s.created_at,
                            "updated_at": s.updated_at,
                        })
                    );
                    0
                }
                Ok(None) => {
                    eprintln!("definition {agent_id} not found");
                    1
                }
                Err(e) => {
                    eprintln!("def-show failed: {e}");
                    1
                }
            }
        }
        "run-show" => {
            let run_id = args.iter().skip(1).find(|a| !a.starts_with("--")).cloned();
            let Some(run_id) = run_id else {
                eprintln!("run-show requires <run_id>");
                return usage();
            };
            let pool = open_pool(&flags.db, &flags.migrations_dir);
            let store = AgentStore::open(pool.clone(), tool_registry::Registry::open(pool));
            match store.run(&run_id) {
                Ok(Some(s)) => {
                    println!(
                        "{}",
                        serde_json::json!({
                            "run_id": s.run.run_id().as_str(),
                            "task_id": s.run.task_id(),
                            "agent_id": s.run.agent_id().as_str(),
                            "agent_version": s.run.agent_version(),
                            "state": s.run.state().as_str(),
                            "memory_refs": s.run.memory_refs(),
                            "created_at": s.created_at,
                            "started_at": s.started_at,
                            "finished_at": s.finished_at,
                        })
                    );
                    0
                }
                Ok(None) => {
                    eprintln!("run {run_id} not found");
                    1
                }
                Err(e) => {
                    eprintln!("run-show failed: {e}");
                    1
                }
            }
        }
        "run" => {
            let agent_id = args.iter().skip(1).find(|a| !a.starts_with("--")).cloned();
            let Some(agent_id) = agent_id else {
                eprintln!("run requires <agent_id>");
                return usage();
            };
            if flags.goal.trim().is_empty() {
                eprintln!("run requires --goal");
                return usage();
            }
            let pool = open_pool(&flags.db, &flags.migrations_dir);
            let registry = tool_registry::Registry::open(pool.clone());
            let store = AgentStore::open(pool.clone(), registry.clone());
            // 先解析定义，确定目标工具清单（驱动面按绑定首工具组装）。
            let stored = match &flags.agent_version {
                Some(v) => store.definition(&agent_id, v),
                None => store.latest_definition(&agent_id),
            }
            .unwrap_or_else(|e| {
                eprintln!("definition lookup: {e}");
                exit(1);
            })
            .unwrap_or_else(|| {
                eprintln!("definition {agent_id} not found");
                exit(1);
            });
            let Some(first_tool) = stored
                .definition
                .capability_refs()
                .first()
                .map(|r| r.tool_id().to_string())
            else {
                eprintln!("definition {agent_id} has empty capability_refs");
                return 1;
            };
            let adapter =
                resolve_python_worker(&flags.manifest_dirs, &first_tool).unwrap_or_else(|e| {
                    eprintln!("python worker resolve: {e}");
                    exit(1);
                });
            let engine = Engine::open(pool.clone(), registry);
            let tool_link = ToolLink::new(engine, Box::new(PythonWorkerDriver { adapter }));
            let broker_link = build_broker_link(&flags, &pool);
            let evidence = match &flags.artifacts_dir {
                Some(dir) => EvidenceRegistrar::new(pool.clone()).with_artifacts_dir(dir.clone()),
                None => EvidenceRegistrar::new(pool.clone()),
            };
            let audit = Some(audit_engine::AuditEngine::new(pool.clone()));
            let loop_ = agent_engine::agent_loop::AgentLoop::new(
                store,
                broker_link,
                tool_link,
                evidence,
                audit,
                LoopConfig {
                    workspace_id: flags.workspace.clone(),
                    max_tool_calls: flags.max_tool_calls,
                },
            );
            let confirmer = FirstFamilyConfirmer::new(
                context_broker::IntentConfig::from_dir(&flags.rulepack_dir).unwrap_or_else(|e| {
                    eprintln!("intent config: {e}");
                    exit(1);
                }),
            );
            let mut request = RunRequest {
                agent_id,
                agent_version: flags.agent_version.clone(),
                goal: flags.goal.clone(),
                task_id: flags.task_id.clone(),
                input: flags.input.clone(),
                planner: Box::new(agent_engine::KeywordSearchPlanner),
                confirmer: Box::new(confirmer),
            };
            match loop_.run(&mut request) {
                Ok(report) => {
                    let text = serde_json::to_string(&report).unwrap_or_default();
                    println!("{text}");
                    if let Some(out) = &flags.out {
                        if let Err(e) = std::fs::write(out, &text) {
                            eprintln!("write report {}: {e}", out.display());
                        }
                    }
                    if report.state == "SUCCEEDED" {
                        0
                    } else {
                        1
                    }
                }
                Err(e) => {
                    eprintln!("run failed: {e}");
                    1
                }
            }
        }
        _ => usage(),
    }
}

fn flag_value(raw: &[String], flag: &str) -> Option<String> {
    raw.iter()
        .position(|a| a == flag)
        .and_then(|i| raw.get(i + 1))
        .cloned()
}
