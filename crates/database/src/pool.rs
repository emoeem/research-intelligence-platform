//! rusqlite 连接池（同步）。
//!
//! 设计：`Mutex<PoolState>` + `Condvar`，签出 `PooledConn`，Drop 归还。
//! 内存库（`DbPath::Memory`）因 SQLite `:memory:` 每连接独立，强制单连接池。

use crate::error::{DbError, DbResult};
use rusqlite::Connection;
use std::path::PathBuf;
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

/// 数据库位置。
#[derive(Debug, Clone)]
pub enum DbPath {
    /// 内存库（单连接池，进程内共享同一连接）。
    Memory,
    /// 文件库。
    File(PathBuf),
}

/// 池配置。
#[derive(Debug, Clone)]
pub struct PoolConfig {
    /// 数据库位置。
    pub path: DbPath,
    /// 只读打开（dryrun 库等只读消费场景）。内存库忽略此字段。
    pub readonly: bool,
    /// 最大连接数（内存库强制为 1）。
    pub max_connections: usize,
    /// busy_timeout。
    pub busy_timeout: Duration,
}

impl PoolConfig {
    /// 只读文件库配置（冒烟 / 对照场景）。
    pub fn readonly_file(path: impl Into<PathBuf>) -> Self {
        Self {
            path: DbPath::File(path.into()),
            readonly: true,
            max_connections: 2,
            busy_timeout: Duration::from_secs(5),
        }
    }

    /// 可写文件库配置。
    pub fn writable_file(path: impl Into<PathBuf>) -> Self {
        Self {
            path: DbPath::File(path.into()),
            readonly: false,
            max_connections: 4,
            busy_timeout: Duration::from_secs(5),
        }
    }
}

struct PoolState {
    idle: Vec<Connection>,
    opened: usize,
}

/// 连接池。
#[derive(Clone)]
pub struct Pool {
    inner: Arc<PoolInner>,
}

struct PoolInner {
    path: DbPath,
    readonly: bool,
    max: usize,
    busy_timeout: Duration,
    state: Mutex<PoolState>,
    cond: Condvar,
}

impl std::fmt::Debug for Pool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Pool")
            .field("path", &self.inner.path)
            .field("readonly", &self.inner.readonly)
            .field("max", &self.inner.max)
            .finish()
    }
}

impl Pool {
    /// 打开池（不建立连接，首次签出时惰性建立）。
    pub fn open(config: PoolConfig) -> DbResult<Self> {
        let max = match config.path {
            DbPath::Memory => 1,
            DbPath::File(_) => config.max_connections.max(1),
        };
        if let DbPath::File(ref p) = config.path {
            if !config.readonly && !p.exists() {
                // 让 rusqlite 创建文件并立即校验可写性，避免首次签出才报错。
                let probe = open_connection(&config.path, false, config.busy_timeout)?;
                probe.close().map_err(|(_conn, e)| DbError::Sqlite(e))?;
            }
        }
        Ok(Self {
            inner: Arc::new(PoolInner {
                path: config.path,
                readonly: config.readonly,
                max,
                busy_timeout: config.busy_timeout,
                state: Mutex::new(PoolState {
                    idle: Vec::new(),
                    opened: 0,
                }),
                cond: Condvar::new(),
            }),
        })
    }

    /// 签出连接（池满则阻塞等待）。
    pub fn get(&self) -> DbResult<PooledConn> {
        let mut st = self.inner.state.lock().map_err(|_| DbError::PoolBusy {
            max: self.inner.max,
        })?;
        loop {
            if let Some(conn) = st.idle.pop() {
                return Ok(PooledConn {
                    conn: Some(conn),
                    pool: Arc::clone(&self.inner),
                });
            }
            if st.opened < self.inner.max {
                st.opened += 1;
                let conn = open_connection(
                    &self.inner.path,
                    self.inner.readonly,
                    self.inner.busy_timeout,
                );
                match conn {
                    Ok(c) => {
                        return Ok(PooledConn {
                            conn: Some(c),
                            pool: Arc::clone(&self.inner),
                        })
                    }
                    Err(e) => {
                        st.opened -= 1;
                        self.inner.cond.notify_one();
                        return Err(e);
                    }
                }
            }
            // 到期后循环重查空闲栈；仍满则继续等（外部以 busy_timeout 粒度重试）。
            let (guard, _timeout) = self
                .inner
                .cond
                .wait_timeout(st, self.inner.busy_timeout)
                .map_err(|_| DbError::PoolBusy {
                    max: self.inner.max,
                })?;
            st = guard;
        }
    }

    /// 借出连接执行一次闭包，完毕自动归还。
    ///
    /// 闭包可返回 rusqlite::Result 或 DbResult（经 `From` 统一）。
    pub fn with_conn<T, E>(&self, f: impl FnOnce(&Connection) -> Result<T, E>) -> DbResult<T>
    where
        DbError: From<E>,
    {
        let conn = self.get()?;
        f(&conn).map_err(DbError::from)
    }

    /// 借出连接执行一次事务闭包：Ok 提交，Err 回滚。
    pub fn with_tx<T, E>(&self, f: impl FnOnce(&Connection) -> Result<T, E>) -> DbResult<T>
    where
        DbError: From<E>,
    {
        let mut conn = self.get()?;
        let tx = conn.transaction()?;
        match f(&tx) {
            Ok(v) => {
                tx.commit().map_err(DbError::Sqlite)?;
                Ok(v)
            }
            Err(e) => {
                // 回滚失败不吞掉原错误，但记录到 stderr 便于排查。
                if let Err(re) = tx.rollback() {
                    eprintln!("database: tx rollback failed after error: {re}");
                }
                Err(DbError::from(e))
            }
        }
    }
}

/// 签出的连接句柄，Drop 时归还池。
pub struct PooledConn {
    conn: Option<Connection>,
    pool: Arc<PoolInner>,
}

impl PooledConn {
    /// 在此连接上开启事务（等价 `Connection::transaction`）。
    pub fn transaction(&mut self) -> DbResult<rusqlite::Transaction<'_>> {
        self.conn
            .as_mut()
            .expect("pooled connection present")
            .transaction()
            .map_err(DbError::Sqlite)
    }
}

impl std::ops::Deref for PooledConn {
    type Target = Connection;

    fn deref(&self) -> &Connection {
        self.conn.as_ref().expect("pooled connection present")
    }
}

impl Drop for PooledConn {
    fn drop(&mut self) {
        if let Some(conn) = self.conn.take() {
            if let Ok(mut st) = self.pool.state.lock() {
                st.idle.push(conn);
                self.pool.cond.notify_one();
            }
            // 锁中毒时丢弃连接（opened 计数不回收，池退化为小一号的池，不影响正确性）。
        }
    }
}

fn open_connection(path: &DbPath, readonly: bool, busy_timeout: Duration) -> DbResult<Connection> {
    let conn = match (path, readonly) {
        (DbPath::Memory, _) => Connection::open_in_memory()?,
        (DbPath::File(p), true) => Connection::open_with_flags(
            p,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?,
        (DbPath::File(p), false) => Connection::open_with_flags(
            p,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE
                | rusqlite::OpenFlags::SQLITE_OPEN_CREATE
                | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?,
    };
    conn.busy_timeout(busy_timeout)?;
    // 库级约定（001_init.sql 头部同款）：外键约束开启。
    conn.pragma_update(None, "foreign_keys", "ON")?;
    Ok(conn)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_pool_is_single_connection() {
        let pool = Pool::open(PoolConfig {
            path: DbPath::Memory,
            readonly: false,
            max_connections: 8,
            busy_timeout: Duration::from_secs(1),
        })
        .unwrap();
        pool.with_conn(|c| -> rusqlite::Result<()> {
            c.execute_batch("CREATE TABLE t(x INTEGER); INSERT INTO t VALUES (7);")?;
            Ok(())
        })
        .unwrap();
        let v: i64 = pool
            .with_conn(|c| c.query_row("SELECT x FROM t", [], |r| r.get(0)))
            .unwrap();
        assert_eq!(v, 7);
    }

    #[test]
    fn file_pool_roundtrip_and_tx_rollback() {
        let dir = std::env::temp_dir().join(format!("rip-db-pool-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let db = dir.join("pool.db");
        let pool = Pool::open(PoolConfig::writable_file(&db)).unwrap();
        pool.with_conn(|c| c.execute_batch("CREATE TABLE t(x INTEGER);"))
            .unwrap();
        let r = pool.with_tx(|c| {
            c.execute("INSERT INTO t VALUES (1)", [])?;
            Err::<(), DbError>(DbError::PoolBusy { max: 1 })
        });
        assert!(r.is_err());
        let n: i64 = pool
            .with_conn(|c| c.query_row("SELECT COUNT(*) FROM t", [], |r| r.get(0)))
            .unwrap();
        assert_eq!(n, 0, "tx rollback must undo insert");
        let _ = std::fs::remove_file(&db);
        let _ = std::fs::remove_dir(&dir);
    }

    #[test]
    fn readonly_open_rejects_writes() {
        let dir = std::env::temp_dir().join(format!("rip-db-ro-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let db = dir.join("ro.db");
        {
            let pool = Pool::open(PoolConfig::writable_file(&db)).unwrap();
            pool.with_conn(|c| c.execute_batch("CREATE TABLE t(x);"))
                .unwrap();
        }
        let pool = Pool::open(PoolConfig::readonly_file(&db)).unwrap();
        let err = pool.with_conn(|c| c.execute("INSERT INTO t VALUES (1)", []));
        assert!(err.is_err(), "readonly pool must not allow writes");
        let _ = std::fs::remove_file(&db);
        let _ = std::fs::remove_dir(&dir);
    }
}
