//! WSW runtime（WSW-2 Proxy / MITM Runtime，`docs/rip3/wsw/WSW-2-TASK.md`；
//! WSW-2-ADR W2-D1「原地扩展五模块」）。
//!
//! - 根（本文件）：既有 Safe primitives 原样保留——`ListenerConfig`/
//!   `ListenerState`/`InterceptState`/`PendingQueue`/`is_loopback_peer`。
//! - [`listener`]：socket 运行时（bind 守卫 + hudsucker Proxy 组装 +
//!   六态生命周期 + [`listener::handler`] 处理器与错误五态）。
//! - [`authority`]：WswAuthority（自实现 CertificateAuthority trait，IP SAN
//!   + 证书缓存 + CA 持久化 0600；ADR-0004 D4）。
//! - [`scope`]：Scope 闸（include/exclude/端口/路径 + 私网审计 + 云元数据
//!   deny-never-allow；W2-D5）。
//! - [`persist`]：024 三表 store（listener/session 登记 + intercept
//!   enqueue/resolve/重启对账；W2-D4/D6）。
//!
//! 红线：本 crate 不做任何系统代理设置或信任库写面调用（CA 信任安装
//! 全为用户显式动作，PLAN §7.1 :516-517）。
#![forbid(unsafe_code)]

pub mod authority;
pub mod browser;
pub mod comparer;
pub mod decoder;
pub mod fuzz;
pub mod listener;
pub mod passive;
pub mod persist;
pub mod scanner;
pub mod scope;
pub mod sender;
pub mod wsframe;

use serde::{Deserialize, Serialize};
use std::net::{IpAddr, SocketAddr};
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ListenerConfig {
    pub bind: SocketAddr,
    pub queue_capacity: usize,
    pub max_header_bytes: usize,
    pub max_body_bytes: usize,
    pub queue_ttl_seconds: u64,
}
impl ListenerConfig {
    pub fn validate(&self) -> Result<(), ConfigError> {
        if !self.bind.ip().is_loopback() {
            return Err(ConfigError::NonLoopback);
        }
        if self.queue_capacity == 0 || self.queue_capacity > 10_000 {
            return Err(ConfigError::QueueCapacity);
        }
        if self.max_header_bytes < 1024 || self.max_header_bytes > 1024 * 1024 {
            return Err(ConfigError::HeaderLimit);
        }
        if self.max_body_bytes == 0 || self.max_body_bytes > 64 * 1024 * 1024 {
            return Err(ConfigError::BodyLimit);
        }
        if self.queue_ttl_seconds == 0 || self.queue_ttl_seconds > 3600 {
            return Err(ConfigError::QueueTtl);
        }
        Ok(())
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigError {
    NonLoopback,
    QueueCapacity,
    HeaderLimit,
    BodyLimit,
    QueueTtl,
}
pub fn is_loopback_peer(peer: IpAddr) -> bool {
    peer.is_loopback()
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ListenerState {
    Stopped,
    Starting,
    Running,
    Degraded,
    Stopping,
    Failed,
}
impl ListenerState {
    pub fn transition(self, next: Self) -> Result<Self, StateError> {
        use ListenerState::*;
        let valid = matches!(
            (self, next),
            (Stopped, Starting)
                | (Starting, Running)
                | (Starting, Failed)
                | (Running, Degraded)
                | (Degraded, Running)
                | (Running, Stopping)
                | (Degraded, Stopping)
                | (Stopping, Stopped)
                | (Stopping, Failed)
        );
        if valid {
            Ok(next)
        } else {
            Err(StateError {
                from: self,
                to: next,
            })
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StateError {
    pub from: ListenerState,
    pub to: ListenerState,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum InterceptState {
    Queued,
    Forwarded,
    Dropped,
    ModifiedForwarded,
    Expired,
}
impl InterceptState {
    pub fn transition(self, next: Self) -> Result<Self, QueueStateError> {
        if self == Self::Queued
            && matches!(
                next,
                Self::Forwarded | Self::Dropped | Self::ModifiedForwarded | Self::Expired
            )
        {
            Ok(next)
        } else {
            Err(QueueStateError {
                from: self,
                to: next,
            })
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QueueStateError {
    pub from: InterceptState,
    pub to: InterceptState,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueueBudget {
    pub capacity: usize,
    pub pending: usize,
}
impl QueueBudget {
    pub fn enqueue(&mut self) -> Result<(), QueueFull> {
        if self.capacity == 0 || self.pending >= self.capacity {
            return Err(QueueFull);
        }
        self.pending += 1;
        Ok(())
    }
    pub fn resolve(&mut self) {
        self.pending = self.pending.saturating_sub(1)
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QueueFull;
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueueEntry {
    pub id: String,
    pub created_at_ms: u64,
    pub expires_at_ms: u64,
    pub state: InterceptState,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QueueError {
    Full,
    DuplicateId,
    InvalidId,
    Expired,
    NotFound,
    AlreadyResolved,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingQueue {
    capacity: usize,
    ttl_ms: u64,
    entries: Vec<QueueEntry>,
}
impl PendingQueue {
    pub fn new(capacity: usize, ttl_ms: u64) -> Result<Self, QueueError> {
        if capacity == 0 || ttl_ms == 0 {
            return Err(QueueError::Full);
        }
        Ok(Self {
            capacity,
            ttl_ms,
            entries: Vec::new(),
        })
    }
    pub fn enqueue(&mut self, id: &str, now_ms: u64) -> Result<&QueueEntry, QueueError> {
        if id.is_empty()
            || id.len() > 128
            || !id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        {
            return Err(QueueError::InvalidId);
        }
        self.expire(now_ms);
        if self.entries.iter().any(|e| e.id == id) {
            return Err(QueueError::DuplicateId);
        }
        if self.pending() >= self.capacity {
            return Err(QueueError::Full);
        }
        let history_limit = self.capacity.saturating_mul(4).max(self.capacity);
        while self.entries.len() >= history_limit {
            if let Some(pos) = self
                .entries
                .iter()
                .position(|e| e.state != InterceptState::Queued)
            {
                self.entries.remove(pos);
            } else {
                return Err(QueueError::Full);
            }
        }
        let entry = QueueEntry {
            id: id.to_owned(),
            created_at_ms: now_ms,
            expires_at_ms: now_ms.saturating_add(self.ttl_ms),
            state: InterceptState::Queued,
        };
        self.entries.push(entry);
        Ok(self.entries.last().expect("entry inserted"))
    }
    pub fn get(&self, id: &str) -> Option<&QueueEntry> {
        self.entries.iter().find(|e| e.id == id)
    }
    pub fn resolve(
        &mut self,
        id: &str,
        next: InterceptState,
        now_ms: u64,
    ) -> Result<(), QueueError> {
        self.expire(now_ms);
        let entry = self
            .entries
            .iter_mut()
            .find(|e| e.id == id)
            .ok_or(QueueError::NotFound)?;
        if entry.state != InterceptState::Queued {
            return Err(QueueError::AlreadyResolved);
        }
        entry.state = entry
            .state
            .transition(next)
            .map_err(|_| QueueError::AlreadyResolved)?;
        Ok(())
    }
    pub fn expire(&mut self, now_ms: u64) -> usize {
        let before = self.entries.len();
        self.entries.retain_mut(|e| {
            if e.state == InterceptState::Queued && now_ms >= e.expires_at_ms {
                e.state = InterceptState::Expired;
                false
            } else {
                e.state != InterceptState::Expired
            }
        });
        before - self.entries.len()
    }
    pub fn pending(&self) -> usize {
        self.entries
            .iter()
            .filter(|e| e.state == InterceptState::Queued)
            .count()
    }
    pub fn len(&self) -> usize {
        self.entries.len()
    }
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn cfg(ip: IpAddr) -> ListenerConfig {
        ListenerConfig {
            bind: SocketAddr::new(ip, 8080),
            queue_capacity: 64,
            max_header_bytes: 16 * 1024,
            max_body_bytes: 2 * 1024 * 1024,
            queue_ttl_seconds: 120,
        }
    }
    #[test]
    fn rejects_nonloopback() {
        assert_eq!(
            cfg("0.0.0.0".parse().unwrap()).validate(),
            Err(ConfigError::NonLoopback)
        )
    }
    #[test]
    fn accepts_loopback() {
        assert!(cfg("127.0.0.1".parse().unwrap()).validate().is_ok())
    }
    #[test]
    fn queue_cap() {
        let mut q = QueueBudget {
            capacity: 1,
            pending: 0,
        };
        assert!(q.enqueue().is_ok());
        assert_eq!(q.enqueue(), Err(QueueFull));
        q.resolve();
        assert!(q.enqueue().is_ok())
    }
    #[test]
    fn terminal_cannot_reopen() {
        let s = InterceptState::Queued
            .transition(InterceptState::Dropped)
            .unwrap();
        assert!(s.transition(InterceptState::Queued).is_err())
    }
    #[test]
    fn listener_transitions() {
        assert!(ListenerState::Stopped
            .transition(ListenerState::Running)
            .is_err());
        assert_eq!(
            ListenerState::Stopped
                .transition(ListenerState::Starting)
                .unwrap(),
            ListenerState::Starting
        )
    }
    #[test]
    fn queue_ttl_expires_pending_entries_and_releases_capacity() {
        let mut q = PendingQueue::new(1, 50).unwrap();
        q.enqueue("item_1", 10).unwrap();
        assert_eq!(q.pending(), 1);
        assert_eq!(q.expire(60), 1);
        assert_eq!(q.pending(), 0);
        assert_eq!(q.enqueue("item_2", 60).unwrap().id, "item_2");
    }
    #[test]
    fn queue_resolve_is_terminal_and_id_validated() {
        let mut q = PendingQueue::new(2, 100).unwrap();
        q.enqueue("safe-id", 0).unwrap();
        q.resolve("safe-id", InterceptState::Dropped, 1).unwrap();
        assert_eq!(
            q.resolve("safe-id", InterceptState::Forwarded, 2),
            Err(QueueError::AlreadyResolved)
        );
        assert_eq!(q.enqueue("bad id", 3), Err(QueueError::InvalidId));
    }
    #[test]
    fn queue_expires_at_exact_deadline() {
        let mut q = PendingQueue::new(1, 5).unwrap();
        q.enqueue("x", 100).unwrap();
        assert_eq!(q.enqueue("y", 105).unwrap().id, "y");
    }
    #[test]
    fn peer_guard() {
        assert!(is_loopback_peer("::1".parse().unwrap()));
        assert!(!is_loopback_peer("192.168.1.2".parse().unwrap()))
    }
}
