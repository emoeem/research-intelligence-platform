//! rp-review：quality-engine Review 队列的 CLI 投影（治理库真实运行与调试用）。
//!
//! 用法：
//! ```text
//! rp-review propose  --db PATH --quarantine-dir DIR
//! rp-review decide   --db PATH --review-id ID --decision D [--reviewer R]
//!                    [--confidence F] [--reason "..."]
//! rp-review pending  --db PATH
//! rp-review counts   --db PATH
//! ```
//! 决议只写 review 表（M1 口径：不自动改资源实体）。退出码：0 成功；2 错误。

use quality_engine::{proposals_from_quarantine_dir, GovernanceError, ReviewStore};
use std::path::PathBuf;

struct Args {
    cmd: String,
    db: PathBuf,
    quarantine_dir: Option<PathBuf>,
    review_id: Option<String>,
    decision: Option<String>,
    reviewer: String,
    confidence: Option<f64>,
    reason: Option<String>,
}

fn usage() -> &'static str {
    "usage: rp-review propose --db GOV_DB --quarantine-dir migrations/quarantine
        rp-review decide  --db GOV_DB --review-id ID --decision D [--reviewer R]
                          [--confidence F] [--reason TEXT]
        rp-review pending --db GOV_DB
        rp-review counts  --db GOV_DB

exit: ok=0, 运行失败=2（error + usage 到 stderr）
help: rp-review --help（stdout, exit 0）"
}

fn parse_args() -> Result<Args, String> {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    if argv.iter().any(|a| a == "--help" || a == "-h") {
        println!("{}", usage());
        std::process::exit(0);
    }
    if argv.is_empty() {
        // P9-3 帮助面统一口径：无参 = 误用（usage 到 stderr + exit 2），
        // 不再与显式 --help 同路径（原实现无参打印帮助 exit 0）。
        eprintln!("{}", usage());
        std::process::exit(2);
    }
    let mut db = None;
    let mut quarantine_dir = None;
    let mut review_id = None;
    let mut decision = None;
    let mut reviewer = "quality-engine".to_string();
    let mut confidence = None;
    let mut reason = None;
    let mut i = 1;
    while i < argv.len() {
        let flag = argv[i].as_str();
        match flag {
            "--db" => {
                i += 1;
                db = Some(PathBuf::from(argv.get(i).ok_or("--db requires a value")?));
            }
            "--quarantine-dir" => {
                i += 1;
                quarantine_dir = Some(PathBuf::from(
                    argv.get(i).ok_or("--quarantine-dir requires a value")?,
                ));
            }
            "--review-id" => {
                i += 1;
                review_id = Some(argv.get(i).ok_or("--review-id requires a value")?.clone());
            }
            "--decision" => {
                i += 1;
                decision = Some(argv.get(i).ok_or("--decision requires a value")?.clone());
            }
            "--reviewer" => {
                i += 1;
                reviewer = argv.get(i).ok_or("--reviewer requires a value")?.clone();
            }
            "--confidence" => {
                i += 1;
                confidence = Some(
                    argv.get(i)
                        .ok_or("--confidence requires a value")?
                        .parse()
                        .map_err(|e| format!("--confidence: {e}"))?,
                );
            }
            "--reason" => {
                i += 1;
                reason = Some(argv.get(i).ok_or("--reason requires a value")?.clone());
            }
            other => return Err(format!("unknown flag {other}")),
        }
        i += 1;
    }
    Ok(Args {
        cmd: argv[0].clone(),
        db: db.ok_or("--db is required")?,
        quarantine_dir,
        review_id,
        decision,
        reviewer,
        confidence,
        reason,
    })
}

fn main() {
    let args = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("error: {e}\n\n{}", usage());
            std::process::exit(2);
        }
    };
    if let Err(e) = run(args) {
        match e {
            GovernanceError::AlreadyDecided(id) => {
                eprintln!("error: review already decided: {id}");
            }
            other => eprintln!("error: {other}"),
        }
        std::process::exit(2);
    }
}

fn run(args: Args) -> Result<(), GovernanceError> {
    /// 本 crate 自有类型的 JSON 序列化（纯数据类型，失败视为内部错误）。
    fn emit<T: serde::Serialize>(v: &T) -> Result<String, GovernanceError> {
        serde_json::to_string_pretty(v)
            .map_err(|e| GovernanceError::InvalidDecision(format!("serialize: {e}")))
    }

    let pool = database::Pool::open(database::PoolConfig::writable_file(&args.db))
        .map_err(GovernanceError::from)?;
    let store = ReviewStore::open(pool)?;
    match args.cmd.as_str() {
        "propose" => {
            let dir = args.quarantine_dir.ok_or_else(|| {
                GovernanceError::InvalidDecision("--quarantine-dir is required".into())
            })?;
            let (proposals, files) = proposals_from_quarantine_dir(&dir)?;
            let mut report = store.propose_all(&proposals)?;
            report.files = files;
            println!("{}", emit(&report)?);
        }
        "decide" => {
            let review_id = args.review_id.ok_or_else(|| {
                GovernanceError::InvalidDecision("--review-id is required".into())
            })?;
            let decision = args
                .decision
                .ok_or_else(|| GovernanceError::InvalidDecision("--decision is required".into()))?;
            let row = store.decide(
                &review_id,
                &decision,
                &args.reviewer,
                args.confidence,
                args.reason.as_deref(),
            )?;
            println!("{}", emit(&row)?);
        }
        "pending" => {
            let rows = store.pending()?;
            println!("{}", emit(&rows)?);
        }
        "counts" => {
            let rows = store.counts()?;
            let map: std::collections::BTreeMap<String, i64> = rows.into_iter().collect();
            println!("{}", emit(&map)?);
        }
        other => {
            eprintln!("unknown command {other}\n\n{}", usage());
            std::process::exit(2);
        }
    }
    Ok(())
}
