//! RuntimeNode 数据面类型（P2-4a，ROADMAP §3.3 P2-4「RuntimeNode.health 落库」
//! 的 domain 侧，2.0 新实体唯一合法落点为本 crate）。
//!
//! 口径：
//! - 实体字段对齐计划 §12:696（id、kind、endpoint、health、version）；
//! - health 词表沿用 `tool_version.health` 的 CHECK 域 unknown/ok/degraded/broken
//!   （D-TC-07 不新增状态，`003_tools_workflow.sql` 重建节 b 同域）。工具级健康
//!   Phase 1 已落 tool_version.health；节点级健康由 Phase 2 Tool Gateway 心跳
//!   维护（TOOL-CONTRACT §7.1:334，计划 §5.6:335），本 crate 只承载状态身份
//!   与解析，心跳判定逻辑属 runtime-gateway（P2-4b）；
//! - kind 词表取计划 §6 Runtime 类型表本阶段（Phase 2）实际落面三值：
//!   `python_worker`（Phase 1 宿主派发器）、`podman_host`（ADR §3 决定第 1 条
//!   过渡 Runner）、`podman_vm`（目标拓扑词表身份，VM 执行面挂起
//!   docs/phase2/PHASE2-DESIGN.md §3.1，仅保留身份不承载行为）；
//!   `remote_runner` 按 TOOL-CONTRACT D-TC-01 不入词表；
//! - 纯 std：与 crate 头注边界一致（MODULE_BOUNDARIES §8.1），零外部依赖、
//!   零新增依赖（P2-4a 判据）。

use crate::error::DomainError;
use std::fmt;

/// RuntimeNode 节点健康（D-TC-07 四值，与 `tool_version.health` CHECK 域一致）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NodeHealth {
    /// 未探测或注册后尚未收到首次心跳（表列默认值）。
    Unknown,
    /// 心跳可达且探针通过（本地 runner：podman 命令可用性检查通过）。
    Ok,
    /// 心跳可用但探针降级（如 podman 可用但版本解析失败）。
    Degraded,
    /// 心跳缺席或探针判定不可用（Runner 离线，派发侧应进 WAITING_FOR_RUNNER）。
    Broken,
}

impl NodeHealth {
    /// 全部变体（按 D-TC-07 词表顺序）。
    pub const ALL: [NodeHealth; 4] = [
        NodeHealth::Unknown,
        NodeHealth::Ok,
        NodeHealth::Degraded,
        NodeHealth::Broken,
    ];

    /// 存储值（小写，与 `tool_version.health` CHECK 域字面一致）。
    pub fn as_str(&self) -> &'static str {
        match self {
            NodeHealth::Unknown => "unknown",
            NodeHealth::Ok => "ok",
            NodeHealth::Degraded => "degraded",
            NodeHealth::Broken => "broken",
        }
    }

    /// 从存储值解析（大小写不敏感，CHECK 域为小写字面）。
    pub fn parse(value: &str) -> Result<Self, DomainError> {
        let lower = value.to_ascii_lowercase();
        Self::ALL
            .into_iter()
            .find(|h| h.as_str() == lower)
            .ok_or_else(|| DomainError::UnknownNodeHealth(value.to_string()))
    }

    /// 是否表示 Runner 不可用（Broken 之外的值不触发 WAITING_FOR_RUNNER 判定）。
    pub fn is_offline(&self) -> bool {
        matches!(self, NodeHealth::Broken)
    }
}

impl fmt::Display for NodeHealth {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for NodeHealth {
    type Err = DomainError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        NodeHealth::parse(s)
    }
}

/// RuntimeNode 节点类型（计划 §6 Runtime 类型表本阶段落面三值）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RuntimeNodeKind {
    /// 现有 Python Workers 宿主派发器（Phase 1 P1-6 产物，进程池探测）。
    PythonWorker,
    /// 宿主 rootless Podman 分容器（ADR §3 决定第 1 条过渡 Runner）。
    PodmanHost,
    /// VM 内 Podman（目标拓扑；词表身份保留，执行面挂起 PHASE2-DESIGN §3.1）。
    PodmanVm,
}

impl RuntimeNodeKind {
    /// 全部变体。
    pub const ALL: [RuntimeNodeKind; 3] = [
        RuntimeNodeKind::PythonWorker,
        RuntimeNodeKind::PodmanHost,
        RuntimeNodeKind::PodmanVm,
    ];

    /// 存储值（小写下划线，与 `runtime_node.kind` CHECK 域字面一致）。
    pub fn as_str(&self) -> &'static str {
        match self {
            RuntimeNodeKind::PythonWorker => "python_worker",
            RuntimeNodeKind::PodmanHost => "podman_host",
            RuntimeNodeKind::PodmanVm => "podman_vm",
        }
    }

    /// 从存储值解析（大小写不敏感）。
    pub fn parse(value: &str) -> Result<Self, DomainError> {
        let lower = value.to_ascii_lowercase();
        Self::ALL
            .into_iter()
            .find(|k| k.as_str() == lower)
            .ok_or_else(|| DomainError::UnknownRuntimeNodeKind(value.to_string()))
    }
}

impl fmt::Display for RuntimeNodeKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for RuntimeNodeKind {
    type Err = DomainError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        RuntimeNodeKind::parse(s)
    }
}

/// RuntimeNode 纯数据结构（计划 §12:696 五字段；心跳时间戳属迁移表列
/// `last_heartbeat_at`，Gateway 写库时落列，本结构不重复承载时刻值）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeNode {
    /// 节点稳定 id（Gateway 登记时生成或 Runner 自报；非空且不超过 128 字符，
    /// 与 RunId 长度口径一致）。
    pub node_id: String,
    /// 节点类型。
    pub kind: RuntimeNodeKind,
    /// 可达地址（本地 runner 可空；VM 节点为 Gateway 通道地址）。
    pub endpoint: Option<String>,
    /// 当前健康（Gateway 心跳维护）。
    pub health: NodeHealth,
    /// Runner 版本指纹（如 podman version 输出；未取得时可空，对应 degraded）。
    pub version: Option<String>,
}

/// 节点 id 长度上限（与 [`crate::ids`] RunId 口径一致）。
pub const NODE_ID_MAX_LEN: usize = 128;

impl RuntimeNode {
    /// 结构校验（P2-4a 纯函数面；落库 FK/CHECK 由 007 迁移承载）。
    ///
    /// 校验项：node_id 非空且不超过 128 字符；endpoint/version 若存在则非空。
    pub fn validate(&self) -> Result<(), DomainError> {
        if self.node_id.is_empty() {
            return Err(DomainError::InvalidRuntimeNode {
                field: "node_id".to_string(),
                reason: "must be non-empty".to_string(),
            });
        }
        if self.node_id.len() > NODE_ID_MAX_LEN {
            return Err(DomainError::InvalidRuntimeNode {
                field: "node_id".to_string(),
                reason: format!("must not exceed {NODE_ID_MAX_LEN} chars"),
            });
        }
        for (field, value) in [("endpoint", &self.endpoint), ("version", &self.version)] {
            if let Some(v) = value {
                if v.is_empty() {
                    return Err(DomainError::InvalidRuntimeNode {
                        field: field.to_string(),
                        reason: "optional field must be non-empty when present".to_string(),
                    });
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn health_covers_dtc07_vocabulary() {
        assert_eq!(NodeHealth::ALL.len(), 4);
        assert_eq!(NodeHealth::Unknown.as_str(), "unknown");
        assert_eq!(NodeHealth::Ok.as_str(), "ok");
        assert_eq!(NodeHealth::Degraded.as_str(), "degraded");
        assert_eq!(NodeHealth::Broken.as_str(), "broken");
    }

    #[test]
    fn health_parse_roundtrip_case_insensitive() {
        for h in NodeHealth::ALL {
            assert_eq!(NodeHealth::parse(h.as_str()).unwrap(), h);
            let upper = h.as_str().to_ascii_uppercase();
            assert_eq!(NodeHealth::parse(&upper).unwrap(), h);
        }
        assert!(NodeHealth::parse("UP").is_err());
        assert!(NodeHealth::parse("").is_err());
    }

    #[test]
    fn health_offline_only_broken() {
        assert!(!NodeHealth::Unknown.is_offline());
        assert!(!NodeHealth::Ok.is_offline());
        assert!(!NodeHealth::Degraded.is_offline());
        assert!(NodeHealth::Broken.is_offline());
    }

    #[test]
    fn kind_covers_phase2_vocabulary() {
        assert_eq!(RuntimeNodeKind::ALL.len(), 3);
        assert_eq!(RuntimeNodeKind::PythonWorker.as_str(), "python_worker");
        assert_eq!(RuntimeNodeKind::PodmanHost.as_str(), "podman_host");
        assert_eq!(RuntimeNodeKind::PodmanVm.as_str(), "podman_vm");
        for k in RuntimeNodeKind::ALL {
            assert_eq!(RuntimeNodeKind::parse(k.as_str()).unwrap(), k);
            let upper = k.as_str().to_ascii_uppercase();
            assert_eq!(RuntimeNodeKind::parse(&upper).unwrap(), k);
        }
        // D-TC-01：remote_runner 不入词表。
        assert!(RuntimeNodeKind::parse("remote_runner").is_err());
        assert!(RuntimeNodeKind::parse("").is_err());
    }

    #[test]
    fn validate_accepts_minimal_local_node() {
        let node = RuntimeNode {
            node_id: "host-cachyos".to_string(),
            kind: RuntimeNodeKind::PodmanHost,
            endpoint: None,
            health: NodeHealth::Unknown,
            version: None,
        };
        assert!(node.validate().is_ok());
    }

    #[test]
    fn validate_rejects_empty_and_oversized_node_id() {
        let base = |node_id: String| RuntimeNode {
            node_id,
            kind: RuntimeNodeKind::PythonWorker,
            endpoint: None,
            health: NodeHealth::Unknown,
            version: None,
        };
        assert!(base(String::new()).validate().is_err());
        let oversized = "n".repeat(NODE_ID_MAX_LEN + 1);
        assert!(base(oversized).validate().is_err());
        let exact = "n".repeat(NODE_ID_MAX_LEN);
        assert!(base(exact).validate().is_ok());
    }

    #[test]
    fn validate_rejects_empty_optional_fields() {
        let node = RuntimeNode {
            node_id: "node-1".to_string(),
            kind: RuntimeNodeKind::PodmanVm,
            endpoint: Some(String::new()),
            health: NodeHealth::Unknown,
            version: Some(String::new()),
        };
        let err = node.validate().unwrap_err();
        assert!(
            matches!(err, DomainError::InvalidRuntimeNode { ref field, .. } if field == "endpoint"),
            "empty endpoint must be rejected first, got {err:?}"
        );
    }
}
