//! Phase 2 Tool Gateway 骨架（ROADMAP P2-4 本体，P2-4b 批次）。
//!
//! 职责链（计划 §5.6:335-338 + §14:765「建立 Tool Gateway 与心跳机制」）：
//! 1. [`heartbeat`]：Runner 可用性探测（过渡期 = 本机 podman 命令可用性
//!    检查，ADR §6 原文口径；不 tight-poll，周期取 Manifest
//!    `probe.interval_seconds` 基线 600s）。
//! 2. [`health_store`]：RuntimeNode.health 落库（runtime_node 表，007）。
//! 3. WAITING_FOR_RUNNER 判定接通：[`Gateway::dispatch`] 派发时探测目标
//!    节点，探测失败即 `Engine::mark_waiting_for_runner`（tool-engine §3.2
//!    第 3 行，lib.rs:316 注入点同一跃迁对），run 落 WAITING_FOR_RUNNER
//!    而**不伪装成工具失败**（计划 §5.6:336 原文语义）；心跳恢复后
//!    [`Gateway::probe_and_resume`] 走第 4 行回 PREPARING，再由
//!    `Engine::drive_resumed` 续跑到终态。
//!
//! 边界：本 crate 不实现容器执行面（podman_host Adapter 属 P2-2/P2-3），
//! 不实现 VM 心跳命令（S4 解锁后经 `CommandProbe::with_command` 换装），
//! 不起常驻循环/systemd 单元（编排属 P2-4c 收口批次）。

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod health_store;
pub mod heartbeat;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use domain::{NodeHealth, RuntimeNode};
use tool_engine::Engine;

pub use health_store::GatewayError;
pub use health_store::HealthStore;
pub use heartbeat::{
    CommandProbe, HeartbeatProbe, ProbeOutcome, SwitchProbe, DEFAULT_PROBE_INTERVAL,
};

/// 网关配置。
#[derive(Debug, Clone)]
pub struct GatewayConfig {
    /// 最小探测间隔（不 tight-poll 守卫；默认取 Manifest
    /// `probe.interval_seconds` 基线 600s）。
    pub probe_interval: std::time::Duration,
}

impl Default for GatewayConfig {
    fn default() -> Self {
        Self {
            probe_interval: DEFAULT_PROBE_INTERVAL,
        }
    }
}

/// 派发结果：QUEUED（Runner 可用）或 WAITING_FOR_RUNNER（探测失败，
/// 计划 §5.6:336 语义，不 FAILED）。
#[derive(Debug, Clone, PartialEq)]
pub enum DispatchOutcome {
    /// Runner 可用，run 停在 QUEUED 等待正常调度驱动。
    Queued {
        /// run id。
        run_id: String,
        /// 承接节点。
        node_id: String,
        /// 探测时点健康。
        health: NodeHealth,
    },
    /// Runner 不可用，run 已落 WAITING_FOR_RUNNER（第 3 行跃迁）。
    WaitingForRunner {
        /// run id。
        run_id: String,
        /// 目标节点。
        node_id: String,
        /// 探测时点健康（broken）。
        health: NodeHealth,
    },
}

impl DispatchOutcome {
    /// run id（两形态共用取值）。
    pub fn run_id(&self) -> &str {
        match self {
            DispatchOutcome::Queued { run_id, .. } => run_id,
            DispatchOutcome::WaitingForRunner { run_id, .. } => run_id,
        }
    }
}

/// Tool Gateway（ROADMAP P2-4 骨架）。
pub struct Gateway {
    engine: Engine,
    store: HealthStore,
    probes: Mutex<HashMap<String, Arc<dyn HeartbeatProbe>>>,
    last_probe: Mutex<HashMap<String, Instant>>,
    probe_interval: std::time::Duration,
    primary_node: Mutex<Option<String>>,
}

impl Gateway {
    /// 在引擎与健康库之上打开网关。
    pub fn new(engine: Engine, store: HealthStore, config: GatewayConfig) -> Self {
        Self {
            engine,
            store,
            probes: Mutex::new(HashMap::new()),
            last_probe: Mutex::new(HashMap::new()),
            probe_interval: config.probe_interval,
            primary_node: Mutex::new(None),
        }
    }

    /// 登记 Runner 节点（upsert runtime_node 行 + 装探针）；首个节点自动
    /// 成为主节点，可用 [`Gateway::set_primary`] 更换。
    pub fn register_node(
        &self,
        node: RuntimeNode,
        probe: Arc<dyn HeartbeatProbe>,
    ) -> Result<(), GatewayError> {
        self.store.upsert_node(&node)?;
        self.probes
            .lock()
            .expect("probe registry mutex")
            .insert(node.node_id.clone(), probe);
        let mut primary = self.primary_node.lock().expect("primary node mutex");
        if primary.is_none() {
            *primary = Some(node.node_id);
        }
        Ok(())
    }

    /// 指定主节点（派发与恢复默认探测的节点）。
    pub fn set_primary(&self, node_id: &str) -> Result<(), GatewayError> {
        if !self
            .probes
            .lock()
            .expect("probe registry mutex")
            .contains_key(node_id)
        {
            return Err(GatewayError::UnknownNode(node_id.to_string()));
        }
        *self.primary_node.lock().expect("primary node mutex") = Some(node_id.to_string());
        Ok(())
    }

    /// 主节点当前健康（落库值；不触发探测）。
    pub fn primary_health(&self) -> Result<Option<NodeHealth>, GatewayError> {
        let primary = self
            .primary_node
            .lock()
            .expect("primary node mutex")
            .clone()
            .ok_or(GatewayError::NoRunnerRegistered)?;
        self.store.health(&primary)
    }

    /// 对指定节点执行一次探测并落库（遵守探测间隔：不足一个周期时返回
    /// 已落库健康值，不重复跑探针命令）。
    pub fn probe_node(&self, node_id: &str) -> Result<NodeHealth, GatewayError> {
        let last = self
            .last_probe
            .lock()
            .expect("last probe mutex")
            .get(node_id)
            .copied();
        if !heartbeat::due(last, self.probe_interval, Instant::now()) {
            return self
                .store
                .health(node_id)?
                .ok_or(GatewayError::UnknownNode(node_id.to_string()));
        }
        self.probe_node_forced(node_id)
    }

    /// 无视间隔强制探测一次并落库（心跳恢复判定与测试用）。
    pub fn probe_node_forced(&self, node_id: &str) -> Result<NodeHealth, GatewayError> {
        let probe = self
            .probes
            .lock()
            .expect("probe registry mutex")
            .get(node_id)
            .cloned()
            .ok_or_else(|| GatewayError::UnknownNode(node_id.to_string()))?;
        let outcome = probe.probe();
        let health = heartbeat::health_from_probe(&outcome);
        self.store
            .record(node_id, health, outcome.version.as_deref())?;
        self.last_probe
            .lock()
            .expect("last probe mutex")
            .insert(node_id.to_string(), Instant::now());
        Ok(health)
    }

    /// 派发：引擎受理（D-TC-05）后按主节点真实心跳判定走 QUEUED 或
    /// WAITING_FOR_RUNNER（第 3 行跃迁；探测失败不产生 FAILED，计划
    /// §5.6:336 原文语义）。
    pub fn dispatch(&self, request: &serde_json::Value) -> Result<DispatchOutcome, GatewayError> {
        let submitted = self.engine.submit(request)?;
        let node_id = self
            .primary_node
            .lock()
            .expect("primary node mutex")
            .clone()
            .ok_or(GatewayError::NoRunnerRegistered)?;
        let health = self.probe_node(&node_id)?;
        if health.is_offline() {
            self.engine.mark_waiting_for_runner(&submitted.run_id)?;
            Ok(DispatchOutcome::WaitingForRunner {
                run_id: submitted.run_id,
                node_id,
                health,
            })
        } else {
            Ok(DispatchOutcome::Queued {
                run_id: submitted.run_id,
                node_id,
                health,
            })
        }
    }

    /// 心跳恢复判定：重探主节点；仍离线则报 [`GatewayError::RunnerUnavailable`]
    /// 且 run 保持 WAITING_FOR_RUNNER；恢复则走第 4 行跃迁回 PREPARING 并
    /// 返回（继续驱动到终态用 `Engine::drive_resumed`，执行面属后续批次）。
    pub fn probe_and_resume(&self, run_id: &str) -> Result<domain::ToolRunStatus, GatewayError> {
        let node_id = self
            .primary_node
            .lock()
            .expect("primary node mutex")
            .clone()
            .ok_or(GatewayError::NoRunnerRegistered)?;
        let health = self.probe_node_forced(&node_id)?;
        if health.is_offline() {
            return Err(GatewayError::RunnerUnavailable { node_id, health });
        }
        self.engine.resume_to_preparing(run_id)?;
        Ok(domain::ToolRunStatus::Preparing)
    }
}
