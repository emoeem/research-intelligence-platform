//! 迁移执行器。
//!
//! 约定：migrations 目录下文件名 `NNN_name.sql`（NNN 为零填充版本号），
//! 按版本号升序执行；`PRAGMA user_version` 已 >= 版本号则跳过（幂等）；
//! 执行后若 user_version 低于版本号（脚本未自带 PRAGMA），由执行器补写。

use crate::error::{DbError, DbResult};
use crate::pool::Pool;
use std::path::{Path, PathBuf};

/// 一个迁移文件。
#[derive(Debug, Clone)]
pub struct Migration {
    /// 版本号（文件名前缀）。
    pub version: i64,
    /// 迁移名（文件名去版本号）。
    pub name: String,
    /// 文件路径。
    pub path: PathBuf,
}

/// 一次实际执行的迁移。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppliedMigration {
    /// 版本号。
    pub version: i64,
    /// 迁移名。
    pub name: String,
}

/// 扫描目录并按版本号排序返回迁移列表。
pub fn discover_migrations(dir: &Path) -> DbResult<Vec<Migration>> {
    let mut out = Vec::new();
    let entries = std::fs::read_dir(dir)?;
    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if !name.to_ascii_lowercase().ends_with(".sql") {
            continue;
        }
        let Some((version, rest)) = split_version(name) else {
            return Err(DbError::MigrationName {
                path: name.to_string(),
                reason: "expected NNN_name.sql with numeric prefix".to_string(),
            });
        };
        if out.iter().any(|m: &Migration| m.version == version) {
            return Err(DbError::DuplicateMigrationVersion {
                version,
                second_file: name.to_string(),
            });
        }
        out.push(Migration {
            version,
            name: rest.to_string(),
            path,
        });
    }
    out.sort_by_key(|m| m.version);
    Ok(out)
}

/// ALTER TABLE ADD COLUMN 行守卫（SQLite 无条件 DDL；002_tasks.sql 头注已
/// 记录该缺口，015_trace_correlation.sql 起在执行器补齐）：目标列已存在的
/// ALTER 语句按 CREATE TABLE IF NOT EXISTS 同款幂等语义跳过（行替换为注释
/// 留痕），防 raw file replay 与 009 式 user_version 自钉重放炸
/// duplicate column name。逐行处理：ALTER 语句须单行书写（迁移文件纪律）。
fn guard_alter_add_columns(
    conn: &rusqlite::Connection,
    sql: &str,
) -> rusqlite::Result<String> {
    let mut out = String::with_capacity(sql.len());
    for line in sql.lines() {
        let trimmed = line.trim();
        let upper = trimmed.to_ascii_uppercase();
        if upper.starts_with("ALTER TABLE") && upper.contains("ADD COLUMN") {
            let tokens: Vec<&str> = trimmed.split_whitespace().collect();
            // 形状：ALTER TABLE <table> ADD COLUMN <column> <decl>...
            let table = tokens.get(2).copied().unwrap_or("").trim_matches('"');
            let add_pos = tokens
                .iter()
                .position(|t| t.eq_ignore_ascii_case("COLUMN"))
                .and_then(|i| tokens.get(i + 1))
                .copied()
                .unwrap_or("")
                .trim_matches('"');
            let exists = table
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_')
                && !table.is_empty()
                && conn
                    .prepare(&format!("PRAGMA table_info(\"{table}\")"))?
                    .query_map([], |r| r.get::<_, String>(1))?
                    .collect::<Result<Vec<String>, rusqlite::Error>>()?
                    .into_iter()
                    .any(|name| name == add_pos);
            if exists {
                out.push_str(&format!(
                    "-- [executor guard] skipped ALTER ADD COLUMN {}.{} (column exists)\n",
                    table, add_pos
                ));
                continue;
            }
        }
        out.push_str(line);
        out.push('\n');
    }
    Ok(out)
}

/// 执行迁移（幂等）。返回本次实际执行的迁移列表。
pub fn migrate(pool: &Pool, dir: &Path) -> DbResult<Vec<AppliedMigration>> {
    let migrations = discover_migrations(dir)?;
    let mut applied = Vec::new();
    for m in migrations {
        let current: i64 =
            pool.with_conn(|c| c.query_row("PRAGMA user_version", [], |r| r.get(0)))?;
        if current >= m.version {
            continue;
        }
        let sql = std::fs::read_to_string(&m.path)?;
        let sql = pool.with_conn(|c| guard_alter_add_columns(c, &sql))?;
        pool.with_conn(|c| c.execute_batch(&sql))?;
        let after: i64 =
            pool.with_conn(|c| c.query_row("PRAGMA user_version", [], |r| r.get(0)))?;
        if after < m.version {
            pool.with_conn(|c| c.pragma_update(None, "user_version", m.version))?;
        }
        let final_v: i64 =
            pool.with_conn(|c| c.query_row("PRAGMA user_version", [], |r| r.get(0)))?;
        if final_v != m.version {
            return Err(DbError::MigrationVersionMismatch {
                file: m.path.display().to_string(),
                expected: m.version,
                actual: final_v,
            });
        }
        applied.push(AppliedMigration {
            version: m.version,
            name: m.name.clone(),
        });
    }
    Ok(applied)
}

/// `001_init.sql` -> (1, "init.sql")。
fn split_version(file_name: &str) -> Option<(i64, &str)> {
    let stem = file_name.strip_suffix(".sql").unwrap_or(file_name);
    let idx = stem.find('_')?;
    let (num, rest) = stem.split_at(idx);
    let version: i64 = num.parse().ok()?;
    Some((version, rest))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pool::{DbPath, PoolConfig};
    use std::time::Duration;

    fn tmp_dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("rip-migrate-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn discover_sorts_and_rejects_bad_names() {
        let dir = tmp_dir("discover");
        std::fs::write(dir.join("002_b.sql"), "SELECT 1;").unwrap();
        std::fs::write(dir.join("001_a.sql"), "SELECT 1;").unwrap();
        std::fs::write(dir.join("README.md"), "x").unwrap();
        let ms = discover_migrations(&dir).unwrap();
        assert_eq!(ms.len(), 2);
        assert_eq!(ms[0].version, 1);
        assert_eq!(ms[1].version, 2);
        std::fs::write(dir.join("badname.sql"), "SELECT 1;").unwrap();
        assert!(discover_migrations(&dir).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn migrate_is_idempotent_against_real_schema() {
        let dir = tmp_dir("run");
        std::fs::write(
            dir.join("001_t.sql"),
            "CREATE TABLE IF NOT EXISTS t(x); PRAGMA user_version = 1;",
        )
        .unwrap();
        let pool = Pool::open(PoolConfig {
            path: DbPath::Memory,
            readonly: false,
            max_connections: 1,
            busy_timeout: Duration::from_secs(1),
        })
        .unwrap();
        let first = migrate(&pool, &dir).unwrap();
        assert_eq!(first.len(), 1);
        let second = migrate(&pool, &dir).unwrap();
        assert!(second.is_empty(), "second run must be a no-op");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn executor_backfills_user_version() {
        let dir = tmp_dir("backfill");
        // 脚本不带 PRAGMA user_version，执行器应补写。
        std::fs::write(dir.join("003_t.sql"), "CREATE TABLE IF NOT EXISTS t(x);").unwrap();
        let pool = Pool::open(PoolConfig {
            path: DbPath::Memory,
            readonly: false,
            max_connections: 1,
            busy_timeout: Duration::from_secs(1),
        })
        .unwrap();
        migrate(&pool, &dir).unwrap();
        let v: i64 = pool
            .with_conn(|c| c.query_row("PRAGMA user_version", [], |r| r.get(0)))
            .unwrap();
        assert_eq!(v, 3);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// ALTER ADD COLUMN 执行器守卫（015 起口径）：文件自钉 user_version 后
    /// 重放（009 式语义，经 migrate()）不炸 duplicate column（列已存在的
    /// ALTER 行被跳过），列首次仍正常落。
    #[test]
    fn guarded_alter_add_column_replay_is_idempotent() {
        let dir = tmp_dir("guard-alter");
        std::fs::write(
            dir.join("001_t.sql"),
            "CREATE TABLE IF NOT EXISTS t(x TEXT);\n\
             ALTER TABLE t ADD COLUMN y TEXT;\n",
        )
        .unwrap();
        let pool = Pool::open(PoolConfig {
            path: DbPath::Memory,
            readonly: false,
            max_connections: 1,
            busy_timeout: Duration::from_secs(1),
        })
        .unwrap();
        migrate(&pool, &dir).unwrap();
        // 受守卫重放：user_version 自钉回 0（模拟 009 式文件自钉语义）后再
        // migrate，文件经执行器守卫重放，列已存在的 ALTER 行跳过不炸。
        pool.with_conn(|c| c.pragma_update(None, "user_version", 0)).unwrap();
        {
            let sql = std::fs::read_to_string(dir.join("001_t.sql")).unwrap();
            let guarded = pool
                .with_conn(|c| guard_alter_add_columns(c, &sql))
                .unwrap();
            eprintln!("GUARDED-OUT: {guarded}");
        }
        migrate(&pool, &dir).unwrap();
        let cols: Vec<String> = pool
            .with_conn(|c| {
                let mut stmt = c.prepare("PRAGMA table_info(t)")?;
                let rows = stmt
                    .query_map([], |r| r.get::<_, String>(1))?
                    .collect::<Result<Vec<_>, rusqlite::Error>>()?;
                Ok::<_, rusqlite::Error>(rows)
            })
            .unwrap();
        assert_eq!(cols, vec!["x", "y"], "列恰一份，重放零重复");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
