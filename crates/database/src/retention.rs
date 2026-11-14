//! 数据保留归档（ROADMAP §11.3 P10-5 / SESSION-HANDOFF §十 数据保留归档）。
//!
//! 语义：按策略把源库中「时刻列早于保留窗口」的行移入独立归档库（先入档
//! 后删除，同一事务，行数对账 `archived == deleted`），归档库复刻源表 DDL
//! （sqlite_master 原文，含 CHECK；索引一并复刻），可独立查询/导出。
//!
//! 安全边界（fail-closed）：
//! - 源连接与归档写在同一连接上以 ATTACH 打通（跨库 MOVE 无原子替代方案，
//!   SQLite 无移动行语句，逐行 INSERT/DELETE 属数据面操作不属通用库轮子）；
//! - 连接级 `foreign_keys=OFF`（SQLite 默认；归档库不复制父表，跨库外键
//!   本就不受支持），作为补偿在删除前扫描全库入站外键引用：候选行被任何
//!   表引用即拒绝该策略（记录 skipped 原因），杜绝静默孤儿；
//! - 表或时刻列缺失、时间列非标识符字符、rowid 不可用：表级 skipped 或
//!   全局 Validation 错误；
//! - 归档移动一次一个事务，`archived != deleted` 即回滚报错。

use crate::error::{DbError, DbResult};
use rusqlite::OptionalExtension;
use serde::Serialize;
use std::path::{Path, PathBuf};

/// 一条保留策略。
#[derive(Debug, Clone, Serialize)]
pub struct RetentionPolicy {
    /// 目标表名。
    pub table: String,
    /// 时刻列名（ISO8601 UTC 文本，与 `YYYY-MM-DDTHH:MM:SSZ` 逐字可比；
    /// 019_external_api_audit.sql 的 `ts` 同款口径）。
    pub time_column: String,
    /// 保留天数：早于 `now - keep_days` 的行进归档。
    pub keep_days: u32,
    /// 附加守卫 SQL（拼进 WHERE，策略作者自担语义；None 即无附加条件）。
    pub extra_guard_sql: Option<String>,
    /// 单次运行最多归档行数（0 = 不限；批量上限防大表一次搬完）。
    pub max_archive_rows: u64,
}

impl RetentionPolicy {
    /// 构造无附加守卫的策略。
    pub fn new(table: &str, time_column: &str, keep_days: u32) -> Self {
        Self {
            table: table.to_string(),
            time_column: time_column.to_string(),
            keep_days,
            extra_guard_sql: None,
            max_archive_rows: 0,
        }
    }
}

/// 内置策略：真实事件表（追加型审计面）。
///
/// - `external_api_audit`（019_external_api_audit.sql）：REST 网关访问审计，
///   无外键引用（全 migrations 零 REFERENCES 指向），`ts` 时刻列带索引；
///   默认保留 180 天。
///
/// `tool_run`（005）虽同为事件面，但被 `artifact.run_id` 外键引用且归档侧
/// 复刻 DDL 含对 `tool` 表的父引用，不进内置集（走自定义策略需自担守卫，
/// 入站引用扫描会先拒绝）。
pub fn builtin_policies() -> Vec<RetentionPolicy> {
    vec![RetentionPolicy::new("external_api_audit", "ts", 180)]
}

/// 单表保留执行结果。
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum RetentionOutcome {
    /// 已归档（行数对账全过）。
    Archived {
        /// 使用的截止时刻（`YYYY-MM-DDTHH:MM:SSZ`，早于它的行进归档）。
        cutoff: String,
        /// 执行前源表总行数。
        rows_before: u64,
        /// 入档行数（= 删除行数，事务对账）。
        archived: u64,
        /// 源表删除行数。
        deleted: u64,
        /// 执行后源表剩余行数。
        rows_after: u64,
    },
    /// 跳过（表不在该库 / 存在入站外键引用 / rowid 不可用等，原因见字段）。
    Skipped {
        /// 跳过原因。
        reason: String,
    },
}

/// 单表保留报告。
#[derive(Debug, Clone, Serialize)]
pub struct RetentionReport {
    /// 目标表名。
    pub table: String,
    /// 归档库路径。
    pub archive_db: PathBuf,
    /// 执行结果。
    #[serde(flatten)]
    pub outcome: RetentionOutcome,
}

/// 执行一组保留策略（同一源库、同一归档库；策略间独立事务）。
///
/// 归档库不存在则创建；源表 DDL 与索引按 sqlite_master 原文复刻进归档库
/// （表已存在则跳过 DDL，幂等）。
pub fn run_retention(
    pool: &crate::pool::Pool,
    policies: &[RetentionPolicy],
    archive_db: &Path,
) -> DbResult<Vec<RetentionReport>> {
    if let Some(parent) = archive_db.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    let mut reports = Vec::new();
    // 单连接打通双库：main = 归档库，rip_src = 源库。FK 保持 SQLite 默认
    // OFF（见模块文档安全边界），删除侧由入站外键扫描兜底。
    let conn = rusqlite::Connection::open(archive_db)?;
    conn.busy_timeout(std::time::Duration::from_secs(30))?;
    let src_display = pool.with_conn(|c| -> DbResult<String> {
        Ok(c.query_row(
            "SELECT file FROM pragma_database_list WHERE name='main'",
            [],
            |r| r.get::<_, String>(0),
        )?)
    })?;
    if src_display.is_empty() || src_display == ":memory:" {
        return Err(DbError::Validation(
            "retention requires a file-backed source pool (memory pool cannot be attached)"
                .to_string(),
        ));
    }
    conn.execute(
        "ATTACH DATABASE ?1 AS rip_src",
        rusqlite::params![src_display],
    )?;

    for policy in policies {
        let report = run_one(&conn, policy, archive_db)?;
        reports.push(report);
    }
    let _ = conn.execute("DETACH DATABASE rip_src", []);
    Ok(reports)
}

/// 校验标识符（表/列名只允许 ASCII 字母数字下划线，拼 SQL 前防注入）。
fn valid_identifier(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// 执行单条策略。
fn run_one(
    conn: &rusqlite::Connection,
    policy: &RetentionPolicy,
    archive_db: &Path,
) -> DbResult<RetentionReport> {
    let table = &policy.table;
    let time_col = &policy.time_column;
    if !valid_identifier(table) || !valid_identifier(time_col) {
        return Err(DbError::Validation(format!(
            "retention policy identifiers must be [A-Za-z0-9_]+: {table}.{time_col}"
        )));
    }

    // 表在场？
    let present: Option<String> = conn
        .query_row(
            "SELECT name FROM rip_src.sqlite_master WHERE type='table' AND name=?1",
            [table],
            |r| r.get(0),
        )
        .optional()?;
    let Some(_) = present else {
        return Ok(RetentionReport {
            table: table.clone(),
            archive_db: archive_db.to_path_buf(),
            outcome: RetentionOutcome::Skipped {
                reason: format!("table {table} absent in source db"),
            },
        });
    };

    // 时刻列在场？
    let cols: Vec<String> = {
        let mut stmt = conn.prepare(&format!("PRAGMA rip_src.table_info(\"{table}\")"))?;
        let rows = stmt
            .query_map([], |r| r.get::<_, String>(1))?
            .collect::<Result<Vec<_>, rusqlite::Error>>()?;
        rows
    };
    if !cols.iter().any(|c| c == time_col) {
        return Err(DbError::Validation(format!(
            "time column {time_col} not found in table {table}"
        )));
    }

    // rowid 可用？（WITHOUT ROWID 表无 rowid，批量 LIMIT 语义依赖它）
    let rowid_ok: Result<i64, rusqlite::Error> = conn.query_row(
        &format!("SELECT COUNT(rowid) FROM rip_src.\"{table}\" LIMIT 1"),
        [],
        |r| r.get(0),
    );
    if rowid_ok.is_err() {
        return Ok(RetentionReport {
            table: table.clone(),
            archive_db: archive_db.to_path_buf(),
            outcome: RetentionOutcome::Skipped {
                reason: format!("table {table} has no rowid (WITHOUT ROWID?)"),
            },
        });
    }

    // 入站外键引用扫描（fail-closed）：任何表的 FK 指向本表即跳过，杜绝
    // 静默孤儿（连接级 FK OFF 的补偿守卫）。
    let mut inbound: Vec<String> = Vec::new();
    {
        let mut stmt = conn.prepare(
            "SELECT name FROM rip_src.sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'",
        )?;
        let tables: Vec<String> = stmt
            .query_map([], |r| r.get(0))?
            .collect::<Result<Vec<_>, rusqlite::Error>>()?;
        for t in tables {
            if &t == table {
                continue;
            }
            let mut fk = conn.prepare(&format!("PRAGMA rip_src.foreign_key_list(\"{t}\")"))?;
            let refs: Vec<String> = fk
                .query_map([], |r| r.get::<_, String>(2))?
                .collect::<Result<Vec<_>, rusqlite::Error>>()?;
            if refs.iter().any(|r| r == table) {
                inbound.push(t);
            }
        }
    }
    if !inbound.is_empty() {
        return Ok(RetentionReport {
            table: table.clone(),
            archive_db: archive_db.to_path_buf(),
            outcome: RetentionOutcome::Skipped {
                reason: format!(
                    "inbound foreign key references exist from: {}",
                    inbound.join(", ")
                ),
            },
        });
    }

    // 归档库 schema 复刻（表 + 索引，sqlite_master 原文；已存在则跳过）。
    let archive_has_table: bool = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
            [table],
            |r| r.get::<_, i64>(0),
        )
        .map(|n| n > 0)?;
    if !archive_has_table {
        let ddl: String = conn.query_row(
            "SELECT sql FROM rip_src.sqlite_master WHERE type='table' AND name=?1",
            [table],
            |r| r.get(0),
        )?;
        conn.execute_batch(&ddl)?;
        let mut idx = conn.prepare(
            "SELECT sql FROM rip_src.sqlite_master WHERE type='index' AND tbl_name=?1 AND sql IS NOT NULL",
        )?;
        let idx_sql: Vec<String> = idx
            .query_map([table], |r| r.get(0))?
            .collect::<Result<Vec<_>, rusqlite::Error>>()?;
        for s in idx_sql {
            conn.execute_batch(&s)?;
        }
    }

    // 截止时刻：SQLite strftime 计算（成熟实现不手写日历算术），与行内
    // ISO8601 Z 文本逐字可比（domain::time::now_iso 同款格式）。
    let cutoff: String = conn.query_row(
        "SELECT strftime('%Y-%m-%dT%H:%M:%SZ', 'now', printf('-%d days', ?1))",
        [policy.keep_days as i64],
        |r| r.get(0),
    )?;

    // 守卫拼装：time_col < cutoff（cutoff 来自 strftime，无引号注入面；
    // time_col 已过标识符校验）+ 可选 extra_guard_sql（策略作者自担）。
    let guard = match &policy.extra_guard_sql {
        Some(g) => format!("\"{time_col}\" < '{cutoff}' AND ({g})"),
        None => format!("\"{time_col}\" < '{cutoff}'"),
    };
    let cap = |sql: String| -> String {
        if policy.max_archive_rows > 0 {
            format!(
                "{sql} AND rowid IN (SELECT rowid FROM rip_src.\"{table}\" WHERE {guard} LIMIT {})",
                policy.max_archive_rows
            )
        } else {
            sql
        }
    };

    let rows_before: u64 = conn.query_row(
        &format!("SELECT COUNT(*) FROM rip_src.\"{table}\""),
        [],
        |r| r.get(0),
    )?;

    conn.execute_batch("BEGIN IMMEDIATE")?;
    let ins = conn.execute(
        &cap(format!(
            "INSERT INTO \"{table}\" SELECT * FROM rip_src.\"{table}\" WHERE {guard}"
        )),
        [],
    )? as u64;
    let del = conn.execute(
        &cap(format!("DELETE FROM rip_src.\"{table}\" WHERE {guard}")),
        [],
    )? as u64;
    if ins != del {
        conn.execute_batch("ROLLBACK")?;
        return Err(DbError::Validation(format!(
            "retention {table}: archived {ins} != deleted {del}, rolled back"
        )));
    }
    conn.execute_batch("COMMIT")?;

    let rows_after: u64 = conn.query_row(
        &format!("SELECT COUNT(*) FROM rip_src.\"{table}\""),
        [],
        |r| r.get(0),
    )?;
    if rows_before.saturating_sub(del) != rows_after {
        return Err(DbError::Validation(format!(
            "retention {table}: count reconcile failed (before {rows_before} - deleted {del} != after {rows_after})"
        )));
    }
    let archive_quick: String = conn.query_row("PRAGMA quick_check", [], |r| r.get(0))?;
    if archive_quick != "ok" {
        return Err(DbError::Validation(format!(
            "retention {table}: archive db quick_check: {archive_quick}"
        )));
    }

    Ok(RetentionReport {
        table: table.clone(),
        archive_db: archive_db.to_path_buf(),
        outcome: RetentionOutcome::Archived {
            cutoff,
            rows_before,
            archived: ins,
            deleted: del,
            rows_after,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pool::{Pool, PoolConfig};

    fn file_pool(dir: &Path, name: &str) -> Pool {
        let db = dir.join(name);
        Pool::open(PoolConfig::writable_file(&db)).unwrap()
    }

    /// 建 external_api_audit 同款表（019 DDL 摘要）+ 2 旧 2 新行。
    fn fixture(pool: &crate::pool::Pool) {
        pool.with_conn(|c| {
            c.execute_batch(
                "CREATE TABLE external_api_audit (
                   id INTEGER PRIMARY KEY AUTOINCREMENT,
                   ts TEXT NOT NULL, event TEXT NOT NULL, method TEXT NOT NULL,
                   path TEXT NOT NULL, remote TEXT, status INTEGER NOT NULL,
                   duration_ms INTEGER, trace_id TEXT, correlation_id TEXT,
                   rate_limited INTEGER NOT NULL DEFAULT 0 CHECK (rate_limited IN (0,1)),
                   auth_enabled INTEGER NOT NULL CHECK (auth_enabled IN (0,1)),
                   meta_json TEXT);
                 CREATE INDEX ix_external_api_audit_ts ON external_api_audit(ts);
                 INSERT INTO external_api_audit (ts, event, method, path, status, auth_enabled)
                   VALUES ('2020-01-01T00:00:00Z','request','GET','/a',200,0),
                          ('2020-06-01T12:00:00Z','request','POST','/b',404,0),
                          ('2099-01-01T00:00:00Z','request','GET','/c',200,1),
                          ('2099-02-01T00:00:00Z','request','GET','/d',200,1);",
            )
        })
        .unwrap();
    }

    #[test]
    fn moves_old_rows_and_reconciles() {
        let dir = std::env::temp_dir().join(format!("rip-retention-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let pool = file_pool(&dir, "src.db");
        fixture(&pool);
        let archive = dir.join("archive.db");
        let reports = run_retention(&pool, &builtin_policies(), &archive).unwrap();
        assert_eq!(reports.len(), 1);
        match &reports[0].outcome {
            RetentionOutcome::Archived {
                archived,
                deleted,
                rows_after,
                rows_before,
                ..
            } => {
                assert_eq!((*archived, *deleted), (2, 2));
                assert_eq!((*rows_before, *rows_after), (4, 2));
            }
            other => panic!("expected archived, got {other:?}"),
        }
        // 归档库行数与 DDL 复刻对账
        let arc = rusqlite::Connection::open(&archive).unwrap();
        let n: i64 = arc
            .query_row("SELECT COUNT(*) FROM external_api_audit", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 2);
        // 幂等重跑：旧行已搬空，第二次 archived=0
        let again = run_retention(&pool, &builtin_policies(), &archive).unwrap();
        match &again[0].outcome {
            RetentionOutcome::Archived { archived, .. } => assert_eq!(*archived, 0),
            other => panic!("expected archived, got {other:?}"),
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn skips_absent_table_and_rejects_bad_identifier() {
        let dir = std::env::temp_dir().join(format!("rip-retention-skip-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let pool = file_pool(&dir, "src.db");
        let archive = dir.join("archive.db");
        let reports =
            run_retention(&pool, &[RetentionPolicy::new("nope", "ts", 30)], &archive).unwrap();
        assert!(matches!(
            &reports[0].outcome,
            RetentionOutcome::Skipped { .. }
        ));
        let bad = run_retention(
            &pool,
            &[RetentionPolicy::new("t; DROP TABLE x", "ts", 30)],
            &archive,
        );
        assert!(matches!(bad, Err(DbError::Validation(_))));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
