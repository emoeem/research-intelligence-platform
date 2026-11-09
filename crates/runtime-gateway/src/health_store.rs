//! RuntimeNode.health 落库（ROADMAP P2-4「RuntimeNode.health 落库」；
//! runtime_node 表由 migrations/007_runtime_node.sql 建立，P2-4a）。
//!
//! 职责边界：本模块只做 health/last_heartbeat_at/version 的读写；
//! WAITING_FOR_RUNNER 判定（health == broken 时派发侧怎么走）属
//! [`crate::Gateway`] 与 tool-engine 状态机，健康词表身份属 domain::node。

use database::{DbError, Pool};
use domain::{NodeHealth, RuntimeNode};

/// 网关错误。
#[derive(Debug, Clone, PartialEq)]
pub enum GatewayError {
    /// 未注册任何 Runner 节点即派发。
    NoRunnerRegistered,
    /// 节点不存在（未 register 或已删）。
    UnknownNode(String),
    /// 心跳恢复判定时 Runner 仍不可用（run 保持 WAITING_FOR_RUNNER）。
    RunnerUnavailable {
        /// 节点 id。
        node_id: String,
        /// 当前健康。
        health: NodeHealth,
    },
    /// tool-engine 侧错误（提交/跃迁被拒）。
    Engine(tool_engine::EngineError),
    /// 底层存储错误。
    Storage(String),
}

impl std::fmt::Display for GatewayError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GatewayError::NoRunnerRegistered => {
                write!(f, "no runner node registered for dispatch")
            }
            GatewayError::UnknownNode(id) => write!(f, "unknown runtime node {id:?}"),
            GatewayError::RunnerUnavailable { node_id, health } => {
                write!(f, "runner {node_id} unavailable (health={health})")
            }
            GatewayError::Engine(e) => write!(f, "engine error: {e}"),
            GatewayError::Storage(r) => write!(f, "storage error: {r}"),
        }
    }
}

impl std::error::Error for GatewayError {}

impl From<tool_engine::EngineError> for GatewayError {
    fn from(e: tool_engine::EngineError) -> Self {
        GatewayError::Engine(e)
    }
}

impl From<DbError> for GatewayError {
    fn from(e: DbError) -> Self {
        GatewayError::Storage(e.to_string())
    }
}

/// runtime_node 健康读写面（007 表）。
#[derive(Clone)]
pub struct HealthStore {
    pool: Pool,
}

impl HealthStore {
    /// 在连接池之上打开（须已应用 007 迁移）。
    pub fn open(pool: Pool) -> Self {
        Self { pool }
    }

    /// 登记或更新节点身份行（幂等 upsert，health 保持现状不覆盖）。
    pub fn upsert_node(&self, node: &RuntimeNode) -> Result<(), GatewayError> {
        node.validate()
            .map_err(|e| GatewayError::Storage(e.to_string()))?;
        let now = domain::time::now_iso();
        self.pool
            .with_conn(|c| {
                c.execute(
                    "INSERT INTO runtime_node (node_id, kind, endpoint, health, version,
                     created_at, updated_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)
                     ON CONFLICT(node_id) DO UPDATE SET
                       kind = excluded.kind,
                       endpoint = excluded.endpoint,
                       updated_at = excluded.updated_at",
                    database::rusqlite::params![
                        node.node_id,
                        node.kind.as_str(),
                        node.endpoint,
                        node.health.as_str(),
                        node.version,
                        now
                    ],
                )?;
                Ok::<(), database::rusqlite::Error>(())
            })
            .map_err(GatewayError::from)?;
        Ok(())
    }

    /// 心跳落库：更新 health / version / last_heartbeat_at（节点必须已登记）。
    pub fn record(
        &self,
        node_id: &str,
        health: NodeHealth,
        version: Option<&str>,
    ) -> Result<(), GatewayError> {
        let now = domain::time::now_iso();
        let changed = self
            .pool
            .with_conn(|c| {
                c.execute(
                    "UPDATE runtime_node SET health = ?2, version = ?3,
                     last_heartbeat_at = ?4, updated_at = ?4 WHERE node_id = ?1",
                    database::rusqlite::params![node_id, health.as_str(), version, now],
                )
            })
            .map_err(GatewayError::from)?;
        if changed == 0 {
            return Err(GatewayError::UnknownNode(node_id.to_string()));
        }
        Ok(())
    }

    /// 读取当前健康（未登记为 None）。
    pub fn health(&self, node_id: &str) -> Result<Option<NodeHealth>, GatewayError> {
        let text: Option<String> = self
            .pool
            .with_conn(|c| {
                c.query_row(
                    "SELECT health FROM runtime_node WHERE node_id = ?1",
                    [node_id],
                    |r| r.get(0),
                )
                .map(Some)
                .or_else(|e| match e {
                    database::rusqlite::Error::QueryReturnedNoRows => Ok(None),
                    other => Err(other),
                })
            })
            .map_err(GatewayError::from)?;
        text.map(|t| NodeHealth::parse(&t).map_err(|e| GatewayError::Storage(e.to_string())))
            .transpose()
    }

    /// 读取最近心跳时刻（ISO8601 UTC；未收到过心跳为 None）。
    pub fn last_heartbeat(&self, node_id: &str) -> Result<Option<String>, GatewayError> {
        self.pool
            .with_conn(|c| {
                c.query_row(
                    "SELECT last_heartbeat_at FROM runtime_node WHERE node_id = ?1",
                    [node_id],
                    |r| r.get(0),
                )
                .map(Some)
                .or_else(|e| match e {
                    database::rusqlite::Error::QueryReturnedNoRows => Ok(None),
                    other => Err(other),
                })
            })
            .map_err(GatewayError::from)
    }
}
