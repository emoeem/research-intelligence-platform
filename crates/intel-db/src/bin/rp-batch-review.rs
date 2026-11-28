//! rp-batch-review：M10 批次审核台账 CLI（rip_platform PG，V0014 三表）。
//!
//! 用法：
//! ```text
//! rp-batch-review import  --file F [--domain D]
//! rp-batch-review list
//! rp-batch-review show    --batch B [--decision D] [--limit N] [--offset O]
//! rp-batch-review decide  --batch B (--ids CSV | --band P0..P3 | --pending)
//!                         --decision APPROVED|REJECTED|DEFERRED
//!                         --reviewer R --reason TEXT [--apply]
//! rp-batch-review undo    --batch B --ids CSV --reviewer R [--apply]
//! rp-batch-review apply   --batch B [--ids CSV] [--apply]
//! rp-batch-review export  --batch B [--out F]
//! ```
//! decide/undo/apply 默认 dry-run（精确显示影响条数），`--apply` 显式提交。
//! 退出码：ok=0；用法/运行失败=2（error + usage 到 stderr）。

use intel_db::{batch_review, open_pool, run_migrations, PgConfig};
use serde_json::json;
use std::path::PathBuf;

struct Args {
    cmd: String,
    file: Option<PathBuf>,
    domain: Option<String>,
    batch: Option<String>,
    decision: Option<String>,
    ids: Option<Vec<String>>,
    band: Option<String>,
    pending: bool,
    reviewer: Option<String>,
    reason: Option<String>,
    apply: bool,
    limit: i64,
    offset: i64,
    out: Option<PathBuf>,
}

fn usage() -> &'static str {
    "usage: rp-batch-review import  --file F [--domain D]
        rp-batch-review list
        rp-batch-review show    --batch B [--decision D] [--limit N] [--offset O]
        rp-batch-review decide  --batch B (--ids CSV | --band P0..P3 | --pending)
                                --decision APPROVED|REJECTED|DEFERRED
                                --reviewer R --reason TEXT [--apply]
        rp-batch-review undo    --batch B --ids CSV --reviewer R [--apply]
        rp-batch-review apply   --batch B [--ids CSV] [--apply]
        rp-batch-review export  --batch B [--out F]

decide/undo/apply default to dry-run; pass --apply to commit.
exit: ok=0, failure=2 (error + usage to stderr)"
}

fn parse_args() -> Result<Args, String> {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    if argv.iter().any(|a| a == "--help" || a == "-h") {
        println!("{}", usage());
        std::process::exit(0);
    }
    if argv.is_empty() {
        eprintln!("{}", usage());
        std::process::exit(2);
    }
    let mut args = Args {
        cmd: argv[0].clone(),
        file: None,
        domain: None,
        batch: None,
        decision: None,
        ids: None,
        band: None,
        pending: false,
        reviewer: None,
        reason: None,
        apply: false,
        limit: 25,
        offset: 0,
        out: None,
    };
    let mut i = 1;
    while i < argv.len() {
        let flag = argv[i].as_str();
        let value = |i: &mut usize| -> Result<String, String> {
            *i += 1;
            argv.get(*i)
                .cloned()
                .ok_or_else(|| format!("{flag} requires a value"))
        };
        match flag {
            "--file" => args.file = Some(PathBuf::from(value(&mut i)?)),
            "--domain" => args.domain = Some(value(&mut i)?),
            "--batch" => args.batch = Some(value(&mut i)?),
            "--decision" => args.decision = Some(value(&mut i)?),
            "--ids" => {
                args.ids = Some(
                    value(&mut i)?
                        .split(',')
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .map(str::to_string)
                        .collect(),
                )
            }
            "--band" => args.band = Some(value(&mut i)?),
            "--pending" => args.pending = true,
            "--reviewer" => args.reviewer = Some(value(&mut i)?),
            "--reason" => args.reason = Some(value(&mut i)?),
            "--apply" => args.apply = true,
            "--limit" => args.limit = value(&mut i)?.parse().map_err(|_| "--limit invalid")?,
            "--offset" => args.offset = value(&mut i)?.parse().map_err(|_| "--offset invalid")?,
            "--out" => args.out = Some(PathBuf::from(value(&mut i)?)),
            other => return Err(format!("unknown flag {other}")),
        }
        i += 1;
    }
    Ok(args)
}

fn die(msg: &str) -> ! {
    eprintln!("{{\"error\":{}}}", json!(msg));
    eprintln!("{}", usage());
    std::process::exit(2);
}

fn main() {
    let args = match parse_args() {
        Ok(a) => a,
        Err(e) => die(&e),
    };
    let pool = match open_pool(&PgConfig::default()) {
        Ok(p) => p,
        Err(e) => die(&format!("{e} [{e:?}]")),
    };
    // 台账表尚不存在时（新库首跑）随迁移补齐；已应用的库 refinery 幂等跳过。
    if let Ok(mut conn) = pool.get() {
        let _ = run_migrations(&mut conn);
    }
    let result = match args.cmd.as_str() {
        "import" => {
            let Some(file) = args.file else {
                die("import requires --file")
            };
            batch_review::import_batch_file(&pool, &file, args.domain.as_deref())
        }
        "list" => batch_review::list_batches(&pool),
        "show" => {
            let Some(batch) = args.batch.clone() else {
                die("show requires --batch")
            };
            batch_review::batch_detail(
                &pool,
                &batch,
                args.decision.as_deref(),
                args.limit,
                args.offset,
            )
        }
        "decide" => {
            let Some(batch) = args.batch.clone() else {
                die("decide requires --batch")
            };
            let Some(decision) = args.decision.clone() else {
                die("decide requires --decision")
            };
            let Some(reviewer) = args.reviewer.clone() else {
                die("decide requires --reviewer")
            };
            let Some(reason) = args.reason.clone() else {
                die("decide requires --reason")
            };
            let selector = if let Some(ids) = args.ids.clone() {
                batch_review::Selector::Ids(ids)
            } else if let Some(band) = args.band.clone() {
                batch_review::Selector::Band(band)
            } else if args.pending {
                batch_review::Selector::AllPending
            } else {
                die("decide requires one of --ids / --band / --pending")
            };
            batch_review::decide(
                &pool, &batch, &selector, &decision, &reviewer, &reason, args.apply,
            )
        }
        "undo" => {
            let Some(batch) = args.batch.clone() else {
                die("undo requires --batch")
            };
            let Some(ids) = args.ids.clone() else {
                die("undo requires --ids")
            };
            let Some(reviewer) = args.reviewer.clone() else {
                die("undo requires --reviewer")
            };
            batch_review::undo(&pool, &batch, &ids, &reviewer, args.apply)
        }
        "apply" => {
            let Some(batch) = args.batch.clone() else {
                die("apply requires --batch")
            };
            batch_review::apply_batch(&pool, &batch, args.ids.clone(), args.apply)
        }
        "export" => {
            let Some(batch) = args.batch.clone() else {
                die("export requires --batch")
            };
            batch_review::export_batch(&pool, &batch)
        }
        "project" => batch_review::projection_plan(&pool),
        other => die(&format!("unknown command {other}")),
    };
    match result {
        Ok(payload) => {
            let text = serde_json::to_string_pretty(&payload).unwrap_or_default();
            match args.out {
                Some(out) => {
                    if std::fs::write(&out, text).is_err() {
                        die("export write failed");
                    }
                    println!(
                        "{}",
                        serde_json::json!({"command": args.cmd, "written": out.display().to_string()})
                    );
                }
                None => println!("{text}"),
            }
        }
        Err(e) => die(&format!("{e} [{e:?}]")),
    }
}
