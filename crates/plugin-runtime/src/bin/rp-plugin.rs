//! rp-plugin CLI（ROADMAP §7.4 验收命令载体；P6-2/P6-3）。
//!
//! 五类生命周期操作（install/enable/disable/upgrade/uninstall）+ 主链辅助
//! 步（load/activate/deactivate/mark-error）+ update-check 与 list。
//!
//! P6-3 起接 rip-data 布局（PLUGIN-CONTRACT §8）：--data-dir / --plugins-dir
//! 或环境变量 RIP_DATA_DIR 指定位置（缺省随部署根取 rip-data）；install/
//! upgrade 先经 [`plugin_runtime::PluginPaths::stage_package`] 把来源
//! Manifest 与包内容拉取落位到 installed/<id>/<version>/（manifest 统一落名
//! manifest.yaml），再走安装链；每次成功操作后向 state/transitions.jsonl
//! 追加跃迁流水并把 plugin 表行同步为 state/plugin-state.json 快照。
//!
//! 用法（均在仓库根执行，--db 必须是开发库副本或临时文件，禁止生产库）：
//!
//! ~~~bash
//! cargo run --offline -p plugin-runtime --bin rp-plugin -- \
//!   --db <开发库副本> install --manifest <manifest 路径> --package <包目录> [--trust local]
//! cargo run --offline -p plugin-runtime --bin rp-plugin -- --db <开发库副本> enable <plugin_id>
//! cargo run --offline -p plugin-runtime --bin rp-plugin -- --db <开发库副本> disable <plugin_id>
//! cargo run --offline -p plugin-runtime --bin rp-plugin -- --db <开发库副本> uninstall <plugin_id> [--data-root <dir>]
//! cargo run --offline -p plugin-runtime --bin rp-plugin -- \
//!   --db <开发库副本> upgrade <plugin_id> --manifest <新 manifest> --package <新包目录>
//! cargo run --offline -p plugin-runtime --bin rp-plugin -- \
//!   --db <开发库副本> update-check <plugin_id> --new-version X.Y.Z
//! cargo run --offline -p plugin-runtime --bin rp-plugin -- --db <开发库副本> list
//! ~~~
//!
//! 红线守卫：--db 指向 platform*.db 一律拒绝（ROADMAP §13 迁移红线）。

use std::path::{Path, PathBuf};
use std::time::Duration;

use database::{DbPath, Pool, PoolConfig};
use plugin_runtime::{
    transition_record, LayoutReport, PluginPaths, PluginRuntime, PluginState, PluginTrust,
    ResolveOptions, StateStore,
};
use serde_json::{json, Value};

fn die(err: plugin_runtime::RuntimeError) -> ! {
    eprintln!("{}", err.to_json_string());
    std::process::exit(1);
}

fn die_paths(err: plugin_runtime::PathsError) -> ! {
    eprintln!("{}", err.to_json_string());
    std::process::exit(1);
}

fn usage() -> ! {
    eprintln!(
        "usage: rp-plugin --db <path> [--migrations <dir>] [--data-dir <dir>] [--plugins-dir <dir>] \
         [--data-root <dir>] <install|enable|disable|uninstall|upgrade|update-check|ingest [--dry-run]|sync <id> [--dry-run] [--idempotency-key K] [--max-files N] [--max-bytes N] [--max-seconds S] [--retries N] [--new-version V] [--package DIR] [--map-candidates]|review <id> --keys k1,k2 --status APPROVED|load|activate|deactivate|mark-error|list> ..."
    );
    std::process::exit(2);
}

/// 红线守卫：生产库一律拒绝（ROADMAP §13）。
fn guard_db_path(db: &Path) {
    let name = db
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    if name.starts_with("platform") {
        eprintln!(
            "{}",
            json!({"code":"production_db_refused",
                   "message":"--db must be a dev copy or temp file; platform*.db is forbidden (ROADMAP §13)"})
        );
        std::process::exit(1);
    }
}

struct Args {
    db: PathBuf,
    migrations: PathBuf,
    data_dir: Option<PathBuf>,
    plugins_dir: Option<PathBuf>,
    data_root: Option<PathBuf>,
    command: String,
    rest: Vec<String>,
}

fn parse_args() -> Args {
    let raw: Vec<String> = std::env::args().skip(1).collect();
    let mut db = None;
    let mut migrations = None;
    let mut data_dir = None;
    let mut plugins_dir = None;
    let mut data_root = None;
    let mut positional = Vec::new();
    let mut i = 0;
    while i < raw.len() {
        match raw[i].as_str() {
            "--db" => {
                db = raw.get(i + 1).map(PathBuf::from);
                i += 2;
            }
            "--migrations" => {
                migrations = raw.get(i + 1).map(PathBuf::from);
                i += 2;
            }
            "--data-dir" => {
                data_dir = raw.get(i + 1).map(PathBuf::from);
                i += 2;
            }
            "--plugins-dir" => {
                plugins_dir = raw.get(i + 1).map(PathBuf::from);
                i += 2;
            }
            "--data-root" => {
                data_root = raw.get(i + 1).map(PathBuf::from);
                i += 2;
            }
            flag if flag.starts_with("--") => {
                // 子命令自己的 --flag 留给 rest。
                positional.push(raw[i].clone());
                i += 1;
            }
            _ => {
                positional.push(raw[i].clone());
                i += 1;
            }
        }
    }
    let Some(db) = db else { usage() };
    guard_db_path(&db);
    let mut it = positional.into_iter();
    let Some(command) = it.next() else { usage() };
    Args {
        db,
        migrations: migrations.unwrap_or_else(|| PathBuf::from("migrations")),
        data_dir,
        plugins_dir,
        data_root,
        command,
        rest: it.collect(),
    }
}

fn flag_value(rest: &[String], flag: &str) -> Option<String> {
    rest.iter()
        .position(|a| a == flag)
        .and_then(|i| rest.get(i + 1))
        .cloned()
}

fn positional_arg(rest: &[String], index: usize) -> Option<String> {
    rest.iter()
        .filter(|a| !a.starts_with("--"))
        .nth(index)
        .cloned()
}

struct Context {
    pool: Pool,
    runtime: PluginRuntime,
    paths: PluginPaths,
    layout: LayoutReport,
    state_store: StateStore,
}

fn open_context(args: &Args) -> Context {
    let pool = Pool::open(PoolConfig {
        path: DbPath::File(args.db.clone()),
        readonly: false,
        max_connections: 4,
        busy_timeout: Duration::from_secs(5),
    })
    .expect("open db pool");
    database::migrate(&pool, &args.migrations).expect("migrate dev copy");
    let audit = audit_engine::AuditEngine::new(pool.clone());
    let mut runtime = PluginRuntime::open(pool.clone(), audit);
    if let Some(root) = &args.data_root {
        runtime = runtime.with_data_root(root);
    }
    let paths = PluginPaths::resolve(ResolveOptions {
        deployment_root: None,
        data_dir: args.data_dir.as_deref(),
        plugins_dir: args.plugins_dir.as_deref(),
        env: None,
    })
    .unwrap_or_else(|e| die_paths(e));
    let layout = paths.ensure_layout().unwrap_or_else(|e| die_paths(e));
    let state_store = StateStore::open(&paths);
    state_store.ensure_dir().unwrap_or_else(|e| die_paths(e));
    Context {
        pool,
        runtime,
        paths,
        layout,
        state_store,
    }
}

impl Context {
    /// 操作成功后的 state 文件双写：追加快照 + 跃迁流水行。
    fn sync_state(&self, records: &[Value]) -> Value {
        let appended = self
            .state_store
            .append_transitions(records.iter())
            .unwrap_or_else(|e| die_paths(e));
        let plugins = self.runtime.store().list().unwrap_or_else(|e| {
            die(plugin_runtime::RuntimeError {
                code: "db".into(),
                message: e.to_string(),
                details: None,
            })
        });
        self.state_store
            .write_snapshot(&plugins)
            .unwrap_or_else(|e| die_paths(e));
        json!({
            "state_file": self.paths.state_file().to_string_lossy(),
            "transitions_file": self.paths.transitions_file().to_string_lossy(),
            "transitions_appended": appended,
            "plugins_in_snapshot": plugins.len(),
        })
    }

    fn active_state(&self, plugin_id: &str) -> Option<PluginState> {
        self.runtime
            .store()
            .get_active(plugin_id)
            .unwrap_or_else(|e| {
                die(plugin_runtime::RuntimeError {
                    code: "db".into(),
                    message: e.to_string(),
                    details: None,
                })
            })
            .map(|p| p.state)
    }
}

fn trust_from(rest: &[String]) -> PluginTrust {
    let raw = flag_value(rest, "--trust").unwrap_or_else(|| "local".to_string());
    PluginTrust::parse(&raw).unwrap_or_else(|_| {
        die(plugin_runtime::RuntimeError {
            code: "trust_invalid".into(),
            message: format!("--trust {raw:?} not in official/community/local/remote"),
            details: None,
        })
    })
}

/// --manifest 的来源文件路径（staging 用）与解析后的 Manifest。
fn manifest_with_path(rest: &[String]) -> (PathBuf, Value) {
    let path = flag_value(rest, "--manifest").unwrap_or_else(|| {
        die(plugin_runtime::RuntimeError {
            code: "args_missing".into(),
            message: "install/upgrade requires --manifest <path>".into(),
            details: None,
        })
    });
    let doc = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        die(plugin_runtime::RuntimeError {
            code: "manifest_read_failed".into(),
            message: format!("read {path:?}: {e}"),
            details: None,
        })
    });
    let manifest = plugin_registry::parse_document(&doc).unwrap_or_else(|e| {
        die(plugin_runtime::RuntimeError {
            code: e.code,
            message: e.message,
            details: None,
        })
    });
    (PathBuf::from(path), manifest)
}

fn package_from(rest: &[String]) -> Option<PathBuf> {
    let path = flag_value(rest, "--package")?;
    let p = PathBuf::from(path);
    if !p.is_dir() {
        die(plugin_runtime::RuntimeError {
            code: "package_missing".into(),
            message: format!("--package {:?} is not a directory", p.display()),
            details: None,
        });
    }
    Some(p)
}

fn manifest_id_version(manifest: &Value) -> (String, String) {
    let id = manifest
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let version = manifest
        .get("version")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    (id, version)
}

fn main() {
    let args = parse_args();
    let ctx = open_context(&args);
    let out: Value = match args.command.as_str() {
        "install" => {
            let trust = trust_from(&args.rest);
            let (manifest_src, manifest) = manifest_with_path(&args.rest);
            let package = package_from(&args.rest);
            let (id, version) = manifest_id_version(&manifest);
            // 拉取落位：--package 走本地整树复制（stage_package）；省略时按
            // source.type 走五类来源实现（P6-4），取包前先做版本固定校验。
            let staged = match package.as_deref() {
                Some(pkg) => ctx
                    .paths
                    .stage_package(&id, &version, &manifest_src, Some(pkg))
                    .unwrap_or_else(|e| die_paths(e)),
                None => {
                    plugin_runtime::source_fetch::verify(&manifest).unwrap_or_else(|e| die(e));
                    let target = ctx
                        .paths
                        .installed_plugin_dir(&id, &version)
                        .unwrap_or_else(|e| die_paths(e));
                    plugin_runtime::source_fetch::fetch_into(&manifest, &target)
                        .unwrap_or_else(|e| die(e));
                    target
                }
            };
            let outcome = ctx
                .runtime
                .install("user:cli", &manifest, &staged, trust)
                .unwrap_or_else(|e| die(e));
            let state = ctx.sync_state(&[
                transition_record("install", &id, &version, None, "INSTALLED"),
                transition_record("register", &id, &version, Some("INSTALLED"), "REGISTERED"),
            ]);
            json!({"command": "install", "outcome": outcome, "staged": staged.to_string_lossy(), "state_files": state, "layout": ctx.layout})
        }
        "enable" => {
            let id = positional_arg(&args.rest, 0).unwrap_or_else(|| usage());
            let from = ctx.active_state(&id);
            let outcome = ctx
                .runtime
                .enable("user:cli", &id)
                .unwrap_or_else(|e| die(e));
            let state = ctx.sync_state(&[transition_record(
                "enable",
                &id,
                &outcome.version,
                Some(outcome.from_state.as_str()),
                "ENABLED",
            )]);
            json!({"command": "enable", "outcome": outcome, "state_files": state, "state_before_arg": from.map(|s| s.as_str())})
        }
        "disable" => {
            let id = positional_arg(&args.rest, 0).unwrap_or_else(|| usage());
            let outcome = ctx
                .runtime
                .disable("user:cli", &id)
                .unwrap_or_else(|e| die(e));
            let state = ctx.sync_state(&[transition_record(
                "disable",
                &id,
                &outcome.version,
                Some(outcome.from_state.as_str()),
                "DISABLED",
            )]);
            json!({"command": "disable", "outcome": outcome, "state_files": state})
        }
        "uninstall" => {
            let id = positional_arg(&args.rest, 0).unwrap_or_else(|| usage());
            let from = ctx.active_state(&id);
            let report = ctx
                .runtime
                .uninstall("user:cli", &id)
                .unwrap_or_else(|e| die(e));
            let version = report.version.clone();
            let state = ctx.sync_state(&[transition_record(
                "uninstall",
                &id,
                &version,
                Some(&outcome_from(from)),
                "UNINSTALLED",
            )]);
            json!({"command": "uninstall", "retention_report": report, "state_files": state})
        }
        "ingest" => {
            let id = positional_arg(&args.rest, 0).unwrap_or_else(|| usage());
            let dry_run = args.rest.iter().any(|a| a == "--dry-run");
            let plugin = ctx.runtime.store().get_active(&id).unwrap_or_else(|e| {
                die(plugin_runtime::RuntimeError {
                    code: "db".into(),
                    message: e.to_string(),
                    details: None,
                })
            });
            let plugin = match plugin {
                Some(p) => p,
                None => die(plugin_runtime::RuntimeError {
                    code: "unknown_plugin".into(),
                    message: format!("plugin {id:?} not installed"),
                    details: None,
                }),
            };
            let manifest: Value = serde_json::from_str(&plugin.manifest_json).unwrap_or_else(|e| {
                die(plugin_runtime::RuntimeError {
                    code: "manifest_corrupt".into(),
                    message: format!("stored manifest: {e}"),
                    details: None,
                })
            });
            let package_dir = plugin.installed_dir.clone().unwrap_or_else(|| {
                die(plugin_runtime::RuntimeError {
                    code: "installed_dir_missing".into(),
                    message: format!("plugin {id:?} has no installed_dir"),
                    details: None,
                })
            });
            let report = plugin_runtime::ingest_knowledge_source_with_opts(
                &ctx.pool,
                &manifest,
                Path::new(&package_dir),
                &plugin_runtime::IngestOptions { dry_run },
            )
            .unwrap_or_else(|e| die(e));
            // SK-1：摄取动作审计（dry-run 亦留痕，KNOWLEDGE_INGEST 词形）。
            let audit = audit_engine::AuditEngine::new(ctx.pool.clone());
            audit.emit_best_effort(
                domain::audit::AuditEvent::new(
                    "user:cli",
                    audit_engine::plugin_actions::KNOWLEDGE_INGEST,
                )
                .with_entity(
                    "plugin",
                    &format!("{}@{}", report.plugin_id, report.plugin_version),
                )
                .with_detail(
                    serde_json::json!({
                        "counts": report.counts,
                        "skipped": report.skipped,
                        "changed": report.changed,
                        "dry_run": report.dry_run,
                    })
                    .to_string(),
                ),
            );
            let state = ctx.sync_state(&[]);
            json!({"command": "ingest", "report": report, "state_files": state})
        }
        "review" => {
            // SK-1：审核状态翻转（Search/Context 只收 APPROVED，受控写面 + 审计）。
            let id = positional_arg(&args.rest, 0).unwrap_or_else(|| usage());
            let keys_raw = flag_value(&args.rest, "--keys").unwrap_or_else(|| usage());
            let status_raw = flag_value(&args.rest, "--status").unwrap_or_else(|| usage());
            let to = domain::knowledge::ReviewStatus::parse(&status_raw).unwrap_or_else(|e| {
                die(plugin_runtime::RuntimeError {
                    code: "review_status_invalid".into(),
                    message: e.to_string(),
                    details: None,
                })
            });
            let keys: Vec<&str> = keys_raw
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .collect();
            let audit = audit_engine::AuditEngine::new(ctx.pool.clone());
            let updated = plugin_runtime::knowledge_review::set_review_status(
                &ctx.pool, &audit, "user:cli", &id, &keys, to,
            )
            .unwrap_or_else(|e| die(e));
            let counts = plugin_runtime::knowledge_review::review_counts(&ctx.pool, &id)
                .unwrap_or_else(|e| die(e));
            let state = ctx.sync_state(&[]);
            json!({"command": "review", "plugin_id": id, "to": to.as_str(), "updated": updated, "counts": counts, "state_files": state})
        }
        "sync" => {
            // SK-3（P6-11 第 2 步）：rip.knowledge.source.sync 受控入口
            // （update-check + ingest via 受控 API，可选候选映射）。
            let id = positional_arg(&args.rest, 0).unwrap_or_else(|| usage());
            let dry_run = args.rest.iter().any(|a| a == "--dry-run");
            let map_candidates = args.rest.iter().any(|a| a == "--map-candidates");
            let idempotency_key = flag_value(&args.rest, "--idempotency-key");
            let max_files = flag_value(&args.rest, "--max-files").and_then(|v| v.parse().ok());
            let max_bytes = flag_value(&args.rest, "--max-bytes").and_then(|v| v.parse().ok());
            let max_seconds = flag_value(&args.rest, "--max-seconds").and_then(|v| v.parse().ok());
            let retries = flag_value(&args.rest, "--retries")
                .and_then(|v| v.parse().ok())
                .unwrap_or(0);
            let new_version = flag_value(&args.rest, "--new-version");

            let plugin = ctx.runtime.store().get_active(&id).unwrap_or_else(|e| {
                die(plugin_runtime::RuntimeError {
                    code: "db".into(),
                    message: e.to_string(),
                    details: None,
                })
            });
            let plugin = match plugin {
                Some(p) => p,
                None => die(plugin_runtime::RuntimeError {
                    code: "unknown_plugin".into(),
                    message: format!("plugin {id:?} not installed"),
                    details: None,
                }),
            };
            let manifest: Value = serde_json::from_str(&plugin.manifest_json).unwrap_or_else(|e| {
                die(plugin_runtime::RuntimeError {
                    code: "manifest_corrupt".into(),
                    message: format!("stored manifest: {e}"),
                    details: None,
                })
            });

            // update-check（受控 API；--new-version 给出时执行）。
            let update_check_result = match &new_version {
                Some(v) => Some(ctx.runtime.update_check(&id, v).unwrap_or_else(|e| die(e))),
                None => None,
            };

            // 取包路径：--package 本地 staging（离线面）；省略时按
            // source_fetch::verify + fetch_into（P6-4 git 通道，先固定完整
            // Commit 再取包，不执行仓库中任何脚本）。
            let package_dir = match package_from(&args.rest) {
                Some(pkg) => pkg,
                None => {
                    plugin_runtime::source_sync::verify_pin(&manifest).unwrap_or_else(|e| die(e));
                    plugin_runtime::source_fetch::verify(&manifest).unwrap_or_else(|e| die(e));
                    let target = ctx
                        .paths
                        .installed_plugin_dir(&id, &plugin.version)
                        .unwrap_or_else(|e| die_paths(e));
                    plugin_runtime::source_fetch::fetch_into(&manifest, &target)
                        .unwrap_or_else(|e| die(e));
                    target
                }
            };

            let prior_keys = plugin_runtime::source_sync::read_sync_keys(&ctx.paths.state_dir());
            let opts = plugin_runtime::SyncOptions {
                dry_run,
                idempotency_key,
                max_files,
                max_bytes,
                max_seconds,
                retries,
                update_check_result,
                map_candidates,
                prior_sync_keys: prior_keys.clone(),
            };
            let report = plugin_runtime::source_sync::sync_knowledge_source(
                &ctx.pool,
                &manifest,
                &package_dir,
                &opts,
            )
            .unwrap_or_else(|e| die(e));

            // 实跑成功且非 dry-run：幂等键入账（侧车文件，非数据库）。
            if !report.dry_run && report.ok && !report.idempotent_skipped {
                if let Some(key) = &opts.idempotency_key {
                    plugin_runtime::source_sync::append_sync_key(&ctx.paths.state_dir(), key, &id)
                        .unwrap_or_else(|e| die(e));
                }
            }

            // 审计：sync 动作留痕（成功与部分失败都落，detail 机器可读）。
            let audit = audit_engine::AuditEngine::new(ctx.pool.clone());
            audit.emit_best_effort(
                domain::audit::AuditEvent::new(
                    "user:cli",
                    audit_engine::plugin_actions::KNOWLEDGE_SYNC,
                )
                .with_entity(
                    "plugin",
                    &format!("{}@{}", report.plugin_id, report.plugin_version),
                )
                .with_detail(
                    serde_json::json!({
                        "ok": report.ok,
                        "dry_run": report.dry_run,
                        "idempotency_key": report.idempotency_key,
                        "idempotent_skipped": report.idempotent_skipped,
                        "attempts": report.attempts,
                        "limits_exceeded": report.limits_exceeded,
                        "partial_failures": report.partial_failures,
                        "counts": report.final_report.as_ref().map(|r| r.counts.clone()),
                    })
                    .to_string(),
                ),
            );

            // 候选映射（受控写面：resource-engine 唯一写者 + 审计留痕）。
            let mapping = if map_candidates && !report.dry_run && report.ok {
                let repo = resource_engine::SqliteResourceRepository::new(ctx.pool.clone());
                let m = repo
                    .promote_knowledge_candidates(&id, 500)
                    .unwrap_or_else(|e| {
                        die(plugin_runtime::RuntimeError {
                            code: "db".into(),
                            message: e.to_string(),
                            details: None,
                        })
                    });
                audit.emit_best_effort(
                    domain::audit::AuditEvent::new(
                        "user:cli",
                        audit_engine::plugin_actions::KNOWLEDGE_MAP_CANDIDATES,
                    )
                    .with_entity("plugin", &id)
                    .with_detail(serde_json::to_string(&m).unwrap_or_default()),
                );
                Some(m)
            } else {
                None
            };

            let state = ctx.sync_state(&[transition_record(
                "sync",
                &id,
                &report.plugin_version,
                None,
                if report.ok { "SYNCED" } else { "SYNC_FAILED" },
            )]);
            json!({"command": "sync", "report": report, "candidate_mapping": mapping, "state_files": state})
        }
        "update-check" => {
            let id = positional_arg(&args.rest, 0).unwrap_or_else(|| usage());
            let new_version = flag_value(&args.rest, "--new-version").unwrap_or_else(|| usage());
            let has_new = ctx
                .runtime
                .update_check(&id, &new_version)
                .unwrap_or_else(|e| die(e));
            let state = ctx.sync_state(&[]);
            json!({"command": "update_check", "has_compatible_new_version": has_new, "state_files": state})
        }
        "upgrade" => {
            let id = positional_arg(&args.rest, 0).unwrap_or_else(|| usage());
            let trust = trust_from(&args.rest);
            let (manifest_src, manifest) = manifest_with_path(&args.rest);
            let package = package_from(&args.rest);
            let (_, new_version) = manifest_id_version(&manifest);
            // 拉取落位：--package 走本地整树复制；省略时按 source.type 走
            // 五类来源实现（P6-4），取包前先做版本固定校验。
            let staged = match package.as_deref() {
                Some(pkg) => ctx
                    .paths
                    .stage_package(&id, &new_version, &manifest_src, Some(pkg))
                    .unwrap_or_else(|e| die_paths(e)),
                None => {
                    plugin_runtime::source_fetch::verify(&manifest).unwrap_or_else(|e| die(e));
                    let target = ctx
                        .paths
                        .installed_plugin_dir(&id, &new_version)
                        .unwrap_or_else(|e| die_paths(e));
                    plugin_runtime::source_fetch::fetch_into(&manifest, &target)
                        .unwrap_or_else(|e| die(e));
                    target
                }
            };
            let outcome = ctx
                .runtime
                .upgrade("user:cli", &id, &manifest, &staged, trust)
                .unwrap_or_else(|e| die(e));
            let state = ctx.sync_state(&[transition_record(
                "upgrade",
                &id,
                &outcome.to_version,
                Some(&outcome.from_version),
                "REGISTERED",
            )]);
            json!({"command": "upgrade", "outcome": outcome, "staged": staged.to_string_lossy(), "state_files": state})
        }
        "load" | "activate" | "deactivate" | "mark-error" => {
            let id = positional_arg(&args.rest, 0).unwrap_or_else(|| usage());
            let from = ctx.active_state(&id);
            let to: PluginState = match args.command.as_str() {
                "load" => ctx.runtime.load(&id).unwrap_or_else(|e| die(e)),
                "activate" => ctx.runtime.activate(&id).unwrap_or_else(|e| die(e)),
                "deactivate" => ctx.runtime.deactivate(&id).unwrap_or_else(|e| die(e)),
                _ => ctx.runtime.mark_error(&id).unwrap_or_else(|e| die(e)),
            };
            let version = ctx
                .runtime
                .store()
                .get_active(&id)
                .unwrap_or_else(|e| {
                    die(plugin_runtime::RuntimeError {
                        code: "db".into(),
                        message: e.to_string(),
                        details: None,
                    })
                })
                .map(|p| p.version)
                .unwrap_or_default();
            let state = ctx.sync_state(&[transition_record(
                args.command.as_str(),
                &id,
                &version,
                Some(&outcome_from(from)),
                to.as_str(),
            )]);
            json!({"command": args.command, "state_after": to.as_str(), "state_files": state})
        }
        "list" => {
            let rows = ctx.runtime.store().list().unwrap_or_else(|e| {
                die(plugin_runtime::RuntimeError {
                    code: "db".into(),
                    message: e.to_string(),
                    details: None,
                })
            });
            let state = ctx.sync_state(&[]);
            json!({"command": "list", "count": rows.len(), "rows": rows.iter().map(|p| json!({
                "plugin_id": p.plugin_id.as_str(),
                "version": p.version,
                "kind": p.kind.as_str(),
                "state": p.state.as_str(),
                "update_pending": p.update_pending,
                "is_active": p.is_active,
                "trust_level": p.trust_level.as_str(),
                "source_uninstalled": p.source_uninstalled,
            })).collect::<Vec<_>>(), "state_files": state})
        }
        other => {
            eprintln!("unknown command {other:?}");
            usage();
        }
    };
    println!("{}", serde_json::to_string_pretty(&out).unwrap_or_default());
    let _ = ctx.pool;
}

fn outcome_from(state: Option<PluginState>) -> String {
    state
        .map(|s| s.as_str().to_string())
        .unwrap_or_else(|| "UNKNOWN".to_string())
}
