//! rp-context CLI（设计稿 CONTEXT-BROKER-DESIGN §10 bin/rp-context；ROADMAP
//! §5.4 第 2 组命令形式）：
//!
//! ~~~text
//! cargo run --offline -p context-broker --bin rp-context -- assemble --task-id t-acceptance
//! cargo run --offline -p context-broker --bin rp-context -- expand --manifest-id <id> --item-id <id>
//! cargo run --offline -p context-broker --bin rp-context -- manifest-show --manifest-id <id>
//! cargo run --offline -p context-broker --bin rp-context -- cache-stats
//! ~~~
//!
//! 默认面（仓库根运行）：--db migrations/dev-p4.db（开发库副本，红线：生产
//! platform*.db 不接）；源接 rulepack + registry + artifact。knowledge/rtis
//! 未接 CLI（需索引库与 kit 参数，已移交 Phase 7 Agent Engine 消费场景定参，
//! PHASE4-ACCEPTANCE §七.5）；缓存挂默认目录
//! （`$RIP_DATA_DIR/runtime/broker-cache`，缺省回落临时目录）。
//!
//! 装配前对 --db 幂等执行迁移（重复执行 no-op，migrate.rs 幂等机制）；
//! 只追加写面与测试内存库同纪律。

use context_broker::{
    ArtifactSource, AssembleRequest, Broker, BrokerCache, BudgetPolicy, IntentConfig,
    ManifestIndex, ManifestStore, RegistrySource, RulepackSource,
};
use database::{DbPath, Pool, PoolConfig};
use std::path::{Path, PathBuf};
use std::process::exit;
use std::sync::Arc;
use std::time::Duration;

fn main() {
    exit(run());
}

fn usage() -> i32 {
    eprintln!(
        "usage: rp-context <sub> [flags]\n\
         \n\
         subcommands:\n\
         \x20 assemble --task-id <id> [--text <text>] [--db <path>] [--budget <n>] [--migrations-dir <dir>]\n\
         \x20 expand --manifest-id <id> --item-id <id> [--db <path>]\n\
         \x20 manifest-show --manifest-id <id> [--db <path>]\n\
         \x20 cache-stats [--data-dir <dir>]"
    );
    2
}

struct Flags {
    db: PathBuf,
    migrations_dir: PathBuf,
    task_id: Option<String>,
    text: String,
    manifest_id: Option<String>,
    item_id: Option<String>,
    budget: u64,
    data_dir: Option<PathBuf>,
    rulepack_dir: PathBuf,
    manifest_dirs: Vec<PathBuf>,
}

fn parse_flags(args: &[String]) -> Flags {
    let mut flags = Flags {
        db: PathBuf::from("migrations/dev-p4.db"),
        migrations_dir: PathBuf::from("migrations"),
        task_id: None,
        text: "kingdee HTTP 检查".to_string(),
        manifest_id: None,
        item_id: None,
        budget: 8000,
        data_dir: None,
        rulepack_dir: PathBuf::from("rulepacks/rip-rules"),
        manifest_dirs: vec![
            PathBuf::from("manifests/tools"),
            PathBuf::from("containerfiles/rip-tool-webresearch"),
            PathBuf::from("containerfiles/rip-tool-coreutils"),
        ],
    };
    let mut iter = args.iter();
    while let Some(flag) = iter.next() {
        let value = |iter: &mut std::slice::Iter<String>| iter.next().cloned().unwrap_or_default();
        match flag.as_str() {
            "--db" => flags.db = PathBuf::from(value(&mut iter)),
            "--migrations-dir" => flags.migrations_dir = PathBuf::from(value(&mut iter)),
            "--task-id" => flags.task_id = Some(value(&mut iter)),
            "--text" => flags.text = value(&mut iter),
            "--manifest-id" => flags.manifest_id = Some(value(&mut iter)),
            "--item-id" => flags.item_id = Some(value(&mut iter)),
            "--budget" => flags.budget = value(&mut iter).parse().unwrap_or(8000),
            "--data-dir" => flags.data_dir = Some(PathBuf::from(value(&mut iter))),
            "--rulepack-dir" => flags.rulepack_dir = PathBuf::from(value(&mut iter)),
            "--manifest-dirs" => {
                flags.manifest_dirs = value(&mut iter).split(',').map(PathBuf::from).collect();
            }
            _ => {}
        }
    }
    flags
}

fn open_pool(path: &Path) -> Pool {
    Pool::open(PoolConfig {
        path: DbPath::File(path.to_path_buf()),
        readonly: false,
        max_connections: 1,
        busy_timeout: Duration::from_secs(5),
    })
    .unwrap_or_else(|e| {
        eprintln!("open {}: {e}", path.display());
        exit(1);
    })
}

fn build_broker(flags: &Flags) -> Broker {
    let pool = open_pool(&flags.db);
    if let Err(e) = database::migrate(&pool, &flags.migrations_dir) {
        eprintln!("migrate {}: {e}", flags.db.display());
        exit(1);
    }
    let cache_dir = flags
        .data_dir
        .clone()
        .map(|d| d.join("runtime").join("broker-cache"))
        .unwrap_or_else(BrokerCache::default_dir);
    let cache = Arc::new(BrokerCache::open(cache_dir).unwrap_or_else(|e| {
        eprintln!("open cache: {e}");
        exit(1);
    }));
    let registry = tool_registry::Registry::open(pool.clone());
    let index = ManifestIndex::from_dirs(&flags.manifest_dirs).unwrap_or_else(|e| {
        eprintln!("manifest index: {e}");
        exit(1);
    });
    let registry_source =
        RegistrySource::new(registry, index).with_cache(cache.clone(), "policy-1");
    let rulepack = RulepackSource::from_dir(&flags.rulepack_dir).unwrap_or_else(|e| {
        eprintln!("rulepack: {e}");
        exit(1);
    });
    let intent_config = IntentConfig::from_dir(&flags.rulepack_dir).unwrap_or_else(|e| {
        eprintln!("intent config: {e}");
        exit(1);
    });
    let policy = BudgetPolicy::default_for(flags.budget).unwrap_or_else(|e| {
        eprintln!("policy: {e}");
        exit(1);
    });
    Broker::new(
        vec![
            Box::new(rulepack),
            Box::new(registry_source),
            Box::new(ArtifactSource::new(pool.clone())),
        ],
        policy,
        intent_config,
        ManifestStore::new(pool.clone()),
        None,
    )
    .with_cache(cache)
}

fn run() -> i32 {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(sub) = args.first() else {
        return usage();
    };
    let flags = parse_flags(&args[1..]);
    match sub.as_str() {
        "assemble" => {
            let Some(task_id) = flags.task_id.clone() else {
                eprintln!("assemble requires --task-id");
                return usage();
            };
            let broker = build_broker(&flags);
            let context = broker.assemble(&AssembleRequest {
                task_id,
                text: flags.text.clone(),
                correlation_id: format!("cli-{}", flags.task_id.clone().unwrap_or_default()),
                actor: "rp-context".to_string(),
            });
            match context {
                Ok(context) => println!(
                    "{}",
                    serde_json::to_string_pretty(&context).unwrap_or_default()
                ),
                Err(e) => {
                    eprintln!("assemble failed: {e}");
                    return 1;
                }
            }
        }
        "expand" => {
            let (Some(manifest_id), Some(item_id)) =
                (flags.manifest_id.clone(), flags.item_id.clone())
            else {
                eprintln!("expand requires --manifest-id and --item-id");
                return usage();
            };
            let broker = build_broker(&flags);
            match broker.expand(&manifest_id, &item_id) {
                Ok(expanded) => println!(
                    "{}",
                    serde_json::to_string_pretty(&serde_json::json!({
                        "manifest_id": expanded.manifest_id,
                        "item_id": expanded.item_id,
                        "granted_tokens": expanded.granted_tokens,
                        "remaining_l3": expanded.remaining_l3,
                        "body": expanded.content.body,
                    }))
                    .unwrap_or_default()
                ),
                Err(e) => {
                    eprintln!("expand failed: {e}");
                    return 1;
                }
            }
        }
        "manifest-show" => {
            let Some(manifest_id) = flags.manifest_id.clone() else {
                eprintln!("manifest-show requires --manifest-id");
                return usage();
            };
            let broker = build_broker(&flags);
            match broker.manifest(&manifest_id) {
                Ok(view) => println!(
                    "{}",
                    serde_json::to_string_pretty(&serde_json::json!({
                        "manifest": {
                            "manifest_id": view.manifest.manifest_id,
                            "task_id": view.manifest.task_id,
                            "intent_status": view.manifest.intent_status.as_str(),
                            "policy_version": view.manifest.policy_version,
                            "broker_version": view.manifest.broker_version,
                            "selection_hash": view.manifest.selection_hash,
                            "created_at": view.manifest.created_at,
                        },
                        "items": view.items.iter().map(|i| serde_json::json!({
                            "item_id": i.item_id,
                            "layer": i.layer.as_str(),
                            "kind": i.kind.as_str(),
                            "source": i.source,
                            "ref_id": i.ref_id,
                            "ref_version": i.ref_version,
                            "included": i.included.as_str(),
                            "est_tokens": i.est_tokens,
                            "reason": i.reason,
                        })).collect::<Vec<_>>(),
                        "ref_events": view.ref_events.iter().map(|e| serde_json::json!({
                            "log_id": e.log_id,
                            "item_id": e.item_id,
                            "event": e.event.as_str(),
                            "created_at": e.created_at,
                        })).collect::<Vec<_>>(),
                    }))
                    .unwrap_or_default()
                ),
                Err(e) => {
                    eprintln!("manifest-show failed: {e}");
                    return 1;
                }
            }
        }
        "cache-stats" => {
            let cache_dir = flags
                .data_dir
                .clone()
                .map(|d| d.join("runtime").join("broker-cache"))
                .unwrap_or_else(BrokerCache::default_dir);
            let cache = BrokerCache::open(cache_dir).unwrap_or_else(|e| {
                eprintln!("open cache: {e}");
                exit(1);
            });
            match cache.stats() {
                Ok(stats) => println!(
                    "{}",
                    serde_json::to_string_pretty(&serde_json::json!({
                        "dir": stats.dir.display().to_string(),
                        "entries": stats.entries,
                        "bytes": stats.bytes,
                    }))
                    .unwrap_or_default()
                ),
                Err(e) => {
                    eprintln!("cache-stats failed: {e}");
                    return 1;
                }
            }
        }
        _ => return usage(),
    }
    0
}
