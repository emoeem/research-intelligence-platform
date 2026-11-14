//! rp-admin：database 运维 CLI 投影（P10-1 备份/恢复/归档的命令行入口）。
//!
//! 用法：
//! ```text
//! rp-admin backup         --db PATH --out ARCHIVE.tar [--extra FILE]...
//! rp-admin restore        --archive ARCHIVE.tar --db PATH [--extras-out DIR]
//! rp-admin verify-archive --archive ARCHIVE.tar
//! rp-admin reten-archive  --db PATH --archive-out ARCHIVE.db [--policies-json FILE]
//! ```
//!
//! - `backup`：Online Backup API 在线快照 + tar 归档（成员：库快照、extras、
//!   manifest.json 逐成员 sha256）。源库只读打开，零写入。
//! - `restore`：解包校验（sha256/尺寸/integrity_check/user_version）后原子
//!   替换目标库。恢复是离线操作契约：目标库不应有活跃写入方。
//! - `verify-archive`：只校验归档不替换。
//! - `reten-archive`：数据保留归档（默认内置策略 external_api_audit 保留
//!   180 天；`--policies-json` 提供自定义策略数组，字段 table/time_column/
//!   keep_days/extra_guard_sql/max_archive_rows）。
//!
//! 输出恒为 JSON；退出码 0 成功，2 参数或执行错误（与 rp-search-index 同款
//! P9-3 帮助面口径：无参 = usage 到 stderr + exit 2）。

use database::{
    backup_to_archive, builtin_policies, restore_from_archive, run_retention, verify_archive,
    RetentionPolicy,
};
use std::path::PathBuf;

enum Mode {
    Backup {
        db: PathBuf,
        out: PathBuf,
        extras: Vec<PathBuf>,
    },
    Restore {
        archive: PathBuf,
        db: PathBuf,
        extras_out: Option<PathBuf>,
    },
    VerifyArchive {
        archive: PathBuf,
    },
    RetenArchive {
        db: PathBuf,
        archive_out: PathBuf,
        policies: Option<PathBuf>,
    },
}

fn usage() -> &'static str {
    "usage: rp-admin backup         --db PATH --out ARCHIVE.tar [--extra FILE]...
        rp-admin restore        --archive ARCHIVE.tar --db PATH [--extras-out DIR]
        rp-admin verify-archive --archive ARCHIVE.tar
        rp-admin reten-archive  --db PATH --archive-out ARCHIVE.db [--policies-json FILE]

backup         在线快照 + tar 归档（manifest 逐成员 sha256；源库只读零写入）
restore        解包校验（sha256/integrity_check/user_version）后原子替换目标库
verify-archive 只校验归档不替换
reten-archive  数据保留归档（默认内置策略 external_api_audit 保留 180 天）

exit: ok=0, 运行失败=2（error + usage 到 stderr）
help: rp-admin --help（stdout, exit 0）"
}

fn parse_args() -> Result<Mode, String> {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    if argv.iter().any(|a| a == "--help" || a == "-h") {
        println!("{}", usage());
        std::process::exit(0);
    }
    if argv.is_empty() {
        eprintln!("{}", usage());
        std::process::exit(2);
    }
    let mode = argv[0].as_str();
    if !matches!(
        mode,
        "backup" | "restore" | "verify-archive" | "reten-archive"
    ) {
        return Err(format!(
            "unknown mode {mode}: expected backup|restore|verify-archive|reten-archive"
        ));
    }
    let mut db: Option<PathBuf> = None;
    let mut out: Option<PathBuf> = None;
    let mut archive: Option<PathBuf> = None;
    let mut extras: Vec<PathBuf> = Vec::new();
    let mut extras_out: Option<PathBuf> = None;
    let mut archive_out: Option<PathBuf> = None;
    let mut policies: Option<PathBuf> = None;
    let need_value = |argv: &[String], i: &mut usize, flag: &str| -> Result<String, String> {
        *i += 1;
        argv.get(*i)
            .cloned()
            .ok_or_else(|| format!("{flag} requires a value"))
    };
    let mut i = 1;
    while i < argv.len() {
        match argv[i].as_str() {
            "--db" => db = Some(PathBuf::from(need_value(&argv, &mut i, "--db")?)),
            "--out" => out = Some(PathBuf::from(need_value(&argv, &mut i, "--out")?)),
            "--archive" => archive = Some(PathBuf::from(need_value(&argv, &mut i, "--archive")?)),
            "--extra" => extras.push(PathBuf::from(need_value(&argv, &mut i, "--extra")?)),
            "--extras-out" => {
                extras_out = Some(PathBuf::from(need_value(&argv, &mut i, "--extras-out")?))
            }
            "--archive-out" => {
                archive_out = Some(PathBuf::from(need_value(&argv, &mut i, "--archive-out")?))
            }
            "--policies-json" => {
                policies = Some(PathBuf::from(need_value(&argv, &mut i, "--policies-json")?))
            }
            other => return Err(format!("unknown flag {other}")),
        }
        i += 1;
    }
    match mode {
        "backup" => {
            let db = db.ok_or("backup requires --db")?;
            let out = out.ok_or("backup requires --out")?;
            Ok(Mode::Backup { db, out, extras })
        }
        "restore" => {
            let archive = archive.ok_or("restore requires --archive")?;
            let db = db.ok_or("restore requires --db")?;
            Ok(Mode::Restore {
                archive,
                db,
                extras_out,
            })
        }
        "verify-archive" => {
            let archive = archive.ok_or("verify-archive requires --archive")?;
            Ok(Mode::VerifyArchive { archive })
        }
        _ => {
            let db = db.ok_or("reten-archive requires --db")?;
            let archive_out = archive_out.ok_or("reten-archive requires --archive-out")?;
            Ok(Mode::RetenArchive {
                db,
                archive_out,
                policies,
            })
        }
    }
}

/// 自定义策略 JSON（数组，字段同 RetentionPolicy 序列化面）。
#[derive(serde::Deserialize)]
struct PolicyJson {
    table: String,
    time_column: String,
    keep_days: u32,
    #[serde(default)]
    extra_guard_sql: Option<String>,
    #[serde(default)]
    max_archive_rows: u64,
}

impl From<PolicyJson> for RetentionPolicy {
    fn from(p: PolicyJson) -> Self {
        RetentionPolicy {
            table: p.table,
            time_column: p.time_column,
            keep_days: p.keep_days,
            extra_guard_sql: p.extra_guard_sql,
            max_archive_rows: p.max_archive_rows,
        }
    }
}

fn main() {
    let mode = match parse_args() {
        Ok(m) => m,
        Err(e) => {
            eprintln!("error: {e}\n\n{}", usage());
            std::process::exit(2);
        }
    };
    let result = run(mode);
    if let Err(e) = result {
        eprintln!("error: {e}");
        std::process::exit(2);
    }
}

fn run(mode: Mode) -> Result<(), String> {
    match mode {
        Mode::Backup { db, out, extras } => {
            let report = backup_to_archive(&db, &out, &extras).map_err(|e| e.to_string())?;
            println!(
                "{}",
                serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?
            );
        }
        Mode::Restore {
            archive,
            db,
            extras_out,
        } => {
            let report = restore_from_archive(&archive, &db, extras_out.as_deref())
                .map_err(|e| e.to_string())?;
            println!(
                "{}",
                serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?
            );
        }
        Mode::VerifyArchive { archive } => {
            let manifest = verify_archive(&archive).map_err(|e| e.to_string())?;
            println!(
                "{}",
                serde_json::to_string_pretty(&manifest).map_err(|e| e.to_string())?
            );
        }
        Mode::RetenArchive {
            db,
            archive_out,
            policies,
        } => {
            let pool = database::Pool::open(database::PoolConfig::writable_file(&db))
                .map_err(|e| format!("open db: {e}"))?;
            let policies: Vec<RetentionPolicy> = match policies {
                Some(p) => {
                    let raw = std::fs::read_to_string(&p)
                        .map_err(|e| format!("read policies json: {e}"))?;
                    let parsed: Vec<PolicyJson> = serde_json::from_str(&raw)
                        .map_err(|e| format!("parse policies json: {e}"))?;
                    parsed.into_iter().map(Into::into).collect()
                }
                None => builtin_policies(),
            };
            let reports =
                run_retention(&pool, &policies, &archive_out).map_err(|e| e.to_string())?;
            println!(
                "{}",
                serde_json::to_string_pretty(&reports).map_err(|e| e.to_string())?
            );
        }
    }
    Ok(())
}
