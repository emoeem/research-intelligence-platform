//! rp-admin：database 运维 CLI 投影（P10-1 备份/恢复/归档的命令行入口）。
//!
//! 用法：
//! ```text
//! rp-admin backup         --db PATH --out ARCHIVE.tar [--extra FILE]...
//! rp-admin restore        --archive ARCHIVE.tar --db PATH [--extras-out DIR]
//! rp-admin verify-archive --archive ARCHIVE.tar
//! rp-admin reten-archive  --db PATH --archive-out ARCHIVE.db [--policies-json FILE]
//! rp-admin caller-add     --db PATH --name NAME --level T0|T1|T2 [--caller-id ID]
//! rp-admin caller-list    --db PATH [--include-revoked]
//! rp-admin key-gen        --db PATH --caller-id ID [--label LABEL]
//! rp-admin key-list       --db PATH [--caller-id ID] [--include-revoked]
//! rp-admin key-revoke     --db PATH --key-hash HASH
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
//! - MA-1（docs/rip3/MA-1-TASK.md §0 D6）caller/key 管理面五子命令：026
//!   caller/api_key 两表的唯一写入口（rp_server.py 校验路径零写，红线 3）。
//!   `key-gen` 明文 `rp1_<43 位 base64url>`（32 字节 /dev/urandom）仅在输出
//!   JSON `key` 字段出现一次（stderr 同步警示），库中仅存 sha256 hex
//!   （`key_hash = sha256(明文全文)`，零 salt，D2）；`key-list` 永不回明文
//!   （库中无明文可回）；`key-revoke` 幂等（重复 revoke 非致命，判据 6）。
//!   五子命令写前自动 `database::migrate`（幂等 no-op，rp-workflow/rp-plugin
//!   写路径 CLI 同款惯例；历史迁移禁改纪律不变）。
//!
//! 输出恒为 JSON；退出码 0 成功，2 参数或执行错误（与 rp-search-index 同款
//! P9-3 帮助面口径：无参 = usage 到 stderr + exit 2）。

use database::{
    backup_to_archive, builtin_policies, restore_from_archive, run_retention, verify_archive,
    RetentionPolicy,
};
use std::path::{Path, PathBuf};

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
    // MA-1（D6）caller/key 管理面五子命令。
    CallerAdd {
        db: PathBuf,
        name: String,
        level: String,
        caller_id: Option<String>,
    },
    CallerList {
        db: PathBuf,
        include_revoked: bool,
    },
    KeyGen {
        db: PathBuf,
        caller_id: String,
        label: Option<String>,
    },
    KeyList {
        db: PathBuf,
        caller_id: Option<String>,
        include_revoked: bool,
    },
    KeyRevoke {
        db: PathBuf,
        key_hash: String,
    },
}

fn usage() -> &'static str {
    "usage: rp-admin backup         --db PATH --out ARCHIVE.tar [--extra FILE]...
        rp-admin restore        --archive ARCHIVE.tar --db PATH [--extras-out DIR]
        rp-admin verify-archive --archive ARCHIVE.tar
        rp-admin reten-archive  --db PATH --archive-out ARCHIVE.db [--policies-json FILE]
        rp-admin caller-add     --db PATH --name NAME --level T0|T1|T2 [--caller-id ID]
        rp-admin caller-list    --db PATH [--include-revoked]
        rp-admin key-gen        --db PATH --caller-id ID [--label LABEL]
        rp-admin key-list       --db PATH [--caller-id ID] [--include-revoked]
        rp-admin key-revoke     --db PATH --key-hash HASH

backup         在线快照 + tar 归档（manifest 逐成员 sha256；源库只读零写入）
restore        解包校验（sha256/integrity_check/user_version）后原子替换目标库
verify-archive 只校验归档不替换
reten-archive  数据保留归档（默认内置策略 external_api_audit 保留 180 天）
caller-add     登记调用者身份（MA-1 D1；caller_id 缺省自动生成 caller- 前缀）
caller-list    列 caller（默认只列在用；--include-revoked 含已停用）
key-gen        生成 key：明文 rp1_<43 位 base64url> 仅在 JSON key 字段输出一次
               （库中仅存 sha256 hex，D2；先校验 caller 存在且未停用）
key-list       列 key（只列 key_hash/label/时刻，永不回明文；默认只列在用）
key-revoke     吊销 key（幂等：重复 revoke 非致命；key-revoke 后下一请求 401）

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
        "backup"
            | "restore"
            | "verify-archive"
            | "reten-archive"
            | "caller-add"
            | "caller-list"
            | "key-gen"
            | "key-list"
            | "key-revoke"
    ) {
        return Err(format!(
            "unknown mode {mode}: expected backup|restore|verify-archive|reten-archive|caller-add|caller-list|key-gen|key-list|key-revoke"
        ));
    }
    let mut db: Option<PathBuf> = None;
    let mut out: Option<PathBuf> = None;
    let mut archive: Option<PathBuf> = None;
    let mut extras: Vec<PathBuf> = Vec::new();
    let mut extras_out: Option<PathBuf> = None;
    let mut archive_out: Option<PathBuf> = None;
    let mut policies: Option<PathBuf> = None;
    // MA-1 caller/key 面参数。
    let mut name: Option<String> = None;
    let mut level: Option<String> = None;
    let mut caller_id: Option<String> = None;
    let mut label: Option<String> = None;
    let mut key_hash: Option<String> = None;
    let mut include_revoked = false;
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
            "--name" => name = Some(need_value(&argv, &mut i, "--name")?),
            "--level" => {
                let value = need_value(&argv, &mut i, "--level")?;
                if !matches!(value.as_str(), "T0" | "T1" | "T2") {
                    return Err(format!(
                        "--level must be T0|T1|T2（D8 词表，001:289 同源），got {value}"
                    ));
                }
                level = Some(value);
            }
            "--caller-id" => caller_id = Some(need_value(&argv, &mut i, "--caller-id")?),
            "--label" => label = Some(need_value(&argv, &mut i, "--label")?),
            "--key-hash" => key_hash = Some(need_value(&argv, &mut i, "--key-hash")?),
            "--include-revoked" => include_revoked = true,
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
        "caller-add" => {
            let db = db.ok_or("caller-add requires --db")?;
            let name = name.ok_or("caller-add requires --name")?;
            let level = level.ok_or("caller-add requires --level T0|T1|T2")?;
            if let Some(id) = &caller_id {
                if !id.starts_with("caller-") {
                    return Err(format!("--caller-id must start with 'caller-'（026 CHECK），got {id}"));
                }
            }
            Ok(Mode::CallerAdd {
                db,
                name,
                level,
                caller_id,
            })
        }
        "caller-list" => {
            let db = db.ok_or("caller-list requires --db")?;
            Ok(Mode::CallerList {
                db,
                include_revoked,
            })
        }
        "key-gen" => {
            let db = db.ok_or("key-gen requires --db")?;
            let caller_id = caller_id.ok_or("key-gen requires --caller-id")?;
            Ok(Mode::KeyGen {
                db,
                caller_id,
                label,
            })
        }
        "key-list" => {
            let db = db.ok_or("key-list requires --db")?;
            Ok(Mode::KeyList {
                db,
                caller_id,
                include_revoked,
            })
        }
        "key-revoke" => {
            let db = db.ok_or("key-revoke requires --db")?;
            let key_hash = key_hash.ok_or("key-revoke requires --key-hash（key-list 输出）")?;
            if !valid_key_hash(&key_hash) {
                return Err("key-hash 须为 64 位小写 hex（sha256，key-list 输出形状）".into());
            }
            Ok(Mode::KeyRevoke { db, key_hash })
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
        Mode::CallerAdd {
            db,
            name,
            level,
            caller_id,
        } => {
            let pool = open_writable_migrated(&db)?;
            let caller_id = caller_id.unwrap_or_else(new_caller_id);
            let created_at: String = pool
                .with_conn(|c| -> rusqlite::Result<String> {
                    let ts: String = c.query_row(
                        "SELECT strftime('%Y-%m-%dT%H:%M:%fZ','now')",
                        [],
                        |r| r.get(0),
                    )?;
                    c.execute(
                        "INSERT INTO caller (caller_id, display_name, permission_level, created_at) \
                         VALUES (?1, ?2, ?3, ?4)",
                        rusqlite::params![caller_id, name, level, ts],
                    )?;
                    Ok(ts)
                })
                .map_err(|e| e.to_string())?;
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "caller_id": caller_id,
                    "display_name": name,
                    "permission_level": level,
                    "created_at": created_at,
                    "revoked_at": serde_json::Value::Null,
                }))
                .map_err(|e| e.to_string())?
            );
        }
        Mode::CallerList {
            db,
            include_revoked,
        } => {
            let pool = open_writable_migrated(&db)?;
            let rows: Vec<CallerRow> = pool
                .with_conn(|c| -> rusqlite::Result<Vec<CallerRow>> {
                    let mut sql = "SELECT caller_id, display_name, permission_level, created_at, revoked_at FROM caller".to_string();
                    if !include_revoked {
                        sql.push_str(" WHERE revoked_at IS NULL");
                    }
                    sql.push_str(" ORDER BY created_at, caller_id");
                    let mut stmt = c.prepare(&sql)?;
                    let rows = stmt
                        .query_map([], |r| {
                            Ok(CallerRow {
                                caller_id: r.get(0)?,
                                display_name: r.get(1)?,
                                permission_level: r.get(2)?,
                                created_at: r.get(3)?,
                                revoked_at: r.get(4)?,
                            })
                        })?
                        .collect::<rusqlite::Result<Vec<_>>>()?;
                    Ok(rows)
                })
                .map_err(|e| e.to_string())?;
            println!(
                "{}",
                serde_json::to_string_pretty(&rows).map_err(|e| e.to_string())?
            );
        }
        Mode::KeyGen {
            db,
            caller_id,
            label,
        } => {
            let pool = open_writable_migrated(&db)?;
            // caller 存在性 + 未停用（停用 caller 不发新 key，D1 校验 join 判定的写侧镜像）。
            let revoked: Option<Option<String>> = pool
                .with_conn(|c| {
                    c.query_row(
                        "SELECT revoked_at FROM caller WHERE caller_id = ?1",
                        rusqlite::params![caller_id],
                        |r| r.get::<_, Option<String>>(0),
                    )
                    .map(Some)
                    .or_else(|err| match err {
                        rusqlite::Error::QueryReturnedNoRows => Ok(None),
                        other => Err(other),
                    })
                })
                .map_err(|e| e.to_string())?;
            match revoked {
                None => return Err(format!("caller 不存在：{caller_id}（先 caller-add）")),
                Some(Some(revoked_at)) => {
                    return Err(format!(
                        "caller {caller_id} 已停用（revoked_at={revoked_at}），不发新 key"
                    ))
                }
                Some(None) => {}
            }
            let plaintext = new_key_plaintext()?;
            let key_hash = key_hash_hex(&plaintext);
            let created_at: String = pool
                .with_conn(|c| -> rusqlite::Result<String> {
                    let ts: String = c.query_row(
                        "SELECT strftime('%Y-%m-%dT%H:%M:%fZ','now')",
                        [],
                        |r| r.get(0),
                    )?;
                    c.execute(
                        "INSERT INTO api_key (key_hash, caller_id, label, created_at) \
                         VALUES (?1, ?2, ?3, ?4)",
                        rusqlite::params![key_hash, caller_id, label, ts],
                    )?;
                    Ok(ts)
                })
                .map_err(|e| e.to_string())?;
            eprintln!(
                "[rp-admin] 警示：key 明文仅此一次展示，请立即保存；库中仅存 sha256（MA-1 D2/D6）"
            );
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "key": plaintext,
                    "key_hash": key_hash,
                    "caller_id": caller_id,
                    "label": label,
                    "created_at": created_at,
                }))
                .map_err(|e| e.to_string())?
            );
        }
        Mode::KeyList {
            db,
            caller_id,
            include_revoked,
        } => {
            let pool = open_writable_migrated(&db)?;
            let rows: Vec<KeyRow> = pool
                .with_conn(|c| -> rusqlite::Result<Vec<KeyRow>> {
                    let mut sql = "SELECT key_hash, caller_id, label, created_at, last_used_at, revoked_at FROM api_key".to_string();
                    if caller_id.is_some() {
                        sql.push_str(" WHERE caller_id = ?1");
                    }
                    if !include_revoked {
                        sql.push_str(if caller_id.is_some() {
                            " AND revoked_at IS NULL"
                        } else {
                            " WHERE revoked_at IS NULL"
                        });
                    }
                    sql.push_str(" ORDER BY created_at DESC, key_hash");
                    let mut stmt = c.prepare(&sql)?;
                    let map = |r: &rusqlite::Row<'_>| {
                        Ok(KeyRow {
                            key_hash: r.get(0)?,
                            caller_id: r.get(1)?,
                            label: r.get(2)?,
                            created_at: r.get(3)?,
                            last_used_at: r.get(4)?,
                            revoked_at: r.get(5)?,
                        })
                    };
                    let rows = match caller_id {
                        Some(id) => stmt
                            .query_map(rusqlite::params![id], map)?
                            .collect::<rusqlite::Result<Vec<_>>>()?,
                        None => stmt
                            .query_map([], map)?
                            .collect::<rusqlite::Result<Vec<_>>>()?,
                    };
                    Ok(rows)
                })
                .map_err(|e| e.to_string())?;
            println!(
                "{}",
                serde_json::to_string_pretty(&rows).map_err(|e| e.to_string())?
            );
        }
        Mode::KeyRevoke { db, key_hash } => {
            let pool = open_writable_migrated(&db)?;
            let (updated, ts): (usize, String) = pool
                .with_conn(|c| -> rusqlite::Result<(usize, String)> {
                    let now: String = c.query_row(
                        "SELECT strftime('%Y-%m-%dT%H:%M:%fZ','now')",
                        [],
                        |r| r.get(0),
                    )?;
                    let n = c.execute(
                        "UPDATE api_key SET revoked_at = ?2 WHERE key_hash = ?1 AND revoked_at IS NULL",
                        rusqlite::params![key_hash, now],
                    )?;
                    Ok((n, now))
                })
                .map_err(|e| e.to_string())?;
            // 幂等（判据 6）：重复 revoke 非致命，记 already_revoked=true；
            // 完全不存在的 key_hash 仍报错（exit 2）。
            let already_revoked = if updated == 0 {
                let exists: Option<bool> = pool
                    .with_conn(|c| {
                        c.query_row(
                            "SELECT 1 FROM api_key WHERE key_hash = ?1",
                            rusqlite::params![key_hash],
                            |_| Ok(true),
                        )
                        .map(Some)
                        .or_else(|err| match err {
                            rusqlite::Error::QueryReturnedNoRows => Ok(None),
                            other => Err(other),
                        })
                    })
                    .map_err(|e| e.to_string())?;
                if exists.is_none() {
                    return Err(format!("key 不存在：{key_hash}（key-list 查看在用 key）"));
                }
                true
            } else {
                false
            };
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "key_hash": key_hash,
                    "already_revoked": already_revoked,
                    "revoked_at": if already_revoked { serde_json::Value::Null } else { serde_json::json!(ts) },
                }))
                .map_err(|e| e.to_string())?
            );
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// MA-1（D6）caller/key 管理面：写连接（自动迁移）+ key 生成 + 行投影
// ---------------------------------------------------------------------------

/// key-hash 形状规则：64 位小写 hex（sha256 输出，key-list 输出形状同源）。
fn valid_key_hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// repo 根锚（rp-verify 同款：RP_REPO_ROOT 优先，缺省 CARGO_MANIFEST_DIR 编译期锚）。
fn repo_root() -> PathBuf {
    if let Ok(root) = std::env::var("RP_REPO_ROOT") {
        return PathBuf::from(root);
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn migrations_dir() -> PathBuf {
    repo_root().join("migrations")
}

/// 五子命令写连接：writable_file 打开 + database::migrate 幂等补齐
/// （no-op 当已到链头；写路径 CLI 自动迁移 = rp-workflow/rp-plugin 惯例）。
fn open_writable_migrated(db: &Path) -> Result<database::Pool, String> {
    let pool = database::Pool::open(database::PoolConfig::writable_file(db))
        .map_err(|e| format!("open db: {e}"))?;
    let dir = migrations_dir();
    database::migrate(&pool, &dir).map_err(|e| format!("migrate {}: {e}", dir.display()))?;
    Ok(pool)
}

/// /dev/urandom 32 字节（std 零新依赖，D6）。
fn urandom32() -> Result<[u8; 32], String> {
    use std::io::Read;
    let mut file =
        std::fs::File::open("/dev/urandom").map_err(|e| format!("open /dev/urandom: {e}"))?;
    let mut buf = [0u8; 32];
    file.read_exact(&mut buf)
        .map_err(|e| format!("read /dev/urandom: {e}"))?;
    Ok(buf)
}

/// D2 key 明文：`rp1_` + 43 位 base64url 无填充（32 字节），全文 47 字符。
fn new_key_plaintext() -> Result<String, String> {
    Ok(format!(
        "rp1_{}",
        data_encoding::BASE64URL_NOPAD.encode(&urandom32()?)
    ))
}

/// D2 key_hash：sha256(明文全文) hex 小写（sha2 既有依赖 + data-encoding HEXLOWER）。
fn key_hash_hex(plaintext: &str) -> String {
    use sha2::{Digest, Sha256};
    data_encoding::HEXLOWER.encode(&Sha256::digest(plaintext.as_bytes()))
}

/// caller_id 生成：`caller-` 前缀 + 微秒时间戳 + 序号 + 随机 4 hex
/// （生成格式属服务端实现细节不进校验面，domain::ids 范式；026 CHECK 只锁
/// caller- 前缀）。
fn new_caller_id() -> String {
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hasher};
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let seq = SEQ.fetch_add(1, Ordering::Relaxed);
    let micros = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_micros())
        .unwrap_or(0);
    let rand4 = {
        let mut h = RandomState::new().build_hasher();
        h.write_u64(seq);
        h.write(&std::process::id().to_le_bytes());
        format!("{:04x}", h.finish() as u16)
    };
    format!("caller-{micros}-{seq:x}-{rand4}")
}

/// caller-list 行投影（serde JSON 输出面）。
#[derive(serde::Serialize)]
struct CallerRow {
    caller_id: String,
    display_name: String,
    permission_level: String,
    created_at: String,
    revoked_at: Option<String>,
}

/// key-list 行投影：只列 key_hash/label/时刻，永不回明文（库中零明文，判据 6）。
#[derive(serde::Serialize)]
struct KeyRow {
    key_hash: String,
    caller_id: String,
    label: Option<String>,
    created_at: String,
    last_used_at: Option<String>,
    revoked_at: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    const B64URL_ALPHABET: &[u8] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

    /// D2/S4 key 形状：`rp1_` 前缀 + 43 位 base64url 无填充 = 全文 47 字符
    /// （32 字节熵；两次生成必不同）。
    #[test]
    fn key_plaintext_shape() {
        let a = new_key_plaintext().unwrap();
        assert_eq!(a.len(), 47, "rp1_(4) + 43 位 base64url 无填充");
        assert!(a.starts_with("rp1_"));
        let body = &a[4..];
        assert_eq!(body.len(), 43);
        assert!(
            body.bytes().all(|b| B64URL_ALPHABET.contains(&b)),
            "body 必须全部落在 base64url 无填充字母表（无 '=' 填充）：{a}"
        );
        let b = new_key_plaintext().unwrap();
        assert_ne!(a, b, "两次生成必须不同（/dev/urandom 32 字节）");
    }

    /// base64url 无填充：RFC 4648 向量（0xFB 0xFF → "-_8"，'+'→'-'、'/'→'_'）
    /// + 随机往返 + 43 长度恒定。
    #[test]
    fn base64url_nopad_vector_and_roundtrip() {
        assert_eq!(data_encoding::BASE64URL_NOPAD.encode(&[0xfb, 0xff]), "-_8");
        for _ in 0..8 {
            let raw = urandom32().unwrap();
            let enc = data_encoding::BASE64URL_NOPAD.encode(&raw);
            assert_eq!(enc.len(), 43, "32 字节 → 43 位无填充");
            assert!(!enc.contains('='));
            assert_eq!(
                data_encoding::BASE64URL_NOPAD.decode(enc.as_bytes()).unwrap(),
                raw
            );
        }
    }

    /// D2 key_hash：sha256(明文全文) hex 小写（RFC 6234 sha256("abc") 向量）。
    #[test]
    fn key_hash_is_sha256_hex_lowercase() {
        assert_eq!(
            key_hash_hex("abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        let k = new_key_plaintext().unwrap();
        let h = key_hash_hex(&k);
        assert_eq!(h.len(), 64);
        assert!(
            h.bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
            "key_hash 必须为 64 位小写 hex：{h}"
        );
    }

    /// caller_id 形状：caller- 前缀（026 CHECK）+ 进程内唯一。
    #[test]
    fn caller_id_shape() {
        let a = new_caller_id();
        let b = new_caller_id();
        assert!(a.starts_with("caller-"));
        assert!(b.starts_with("caller-"));
        assert_ne!(a, b, "序号叠加保证进程内唯一");
    }

    /// key-revoke 参数面：key-hash 形状规则拒绝 65 位 / 大写 / 非 hex；
    /// 合法 64 位小写 hex 放行（parse_args 与本函数同源）。
    #[test]
    fn key_revoke_rejects_bad_hash_shape() {
        let bad = [
            "a".repeat(65),
            "A".repeat(64),
            "g".repeat(64),
            String::new(),
        ];
        for case in bad {
            assert!(!valid_key_hash(&case), "形状规则必须拒绝：{case}");
        }
        assert!(valid_key_hash(&"ab".repeat(32)));
    }
}
