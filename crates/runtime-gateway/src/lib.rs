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
//! P10-3 扩展（多 Runner/多 VM 故障转移）：[`Gateway`] 维护按优先序排列的
//! 候选节点表（主节点恒居首，其余按登记序），[`Gateway::dispatch`] 逐候选
//! 探测、首个可用节点承接（主节点离线转投备用，run 落 QUEUED 不丢）；全部
//! 候选离线才落 WAITING_FOR_RUNNER（挂主节点，§5.6:336 语义不变）。
//! [`Gateway::probe_and_resume`] 与 [`Gateway::recover_waiting_runs`] 按同一
//! 优先序做恢复判定：任一候选恢复即续跑（恢复判定每节点每批一次强制探测，
//! 不 tight-poll）。remote_runner 按 TOOL-CONTRACT D-TC-01 仍不入词表
//! （ROADMAP P10-3：开放与否维持 P2 评审，本批不开放）。
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
    CommandProbe, DEFAULT_PROBE_INTERVAL, HeartbeatProbe, ProbeOutcome, SwitchProbe,
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

/// 批量恢复报告（[`Gateway::recover_waiting_runs`] 返回）。
#[derive(Debug, Clone, PartialEq)]
pub struct RecoveryReport {
    /// 本批探测出可用（非 broken）的候选节点，按派发优先序。
    pub healthy_nodes: Vec<String>,
    /// 本批从 WAITING_FOR_RUNNER 恢复回 PREPARING 的 run id（created_at 序）。
    pub resumed_run_ids: Vec<String>,
    /// 扫描到但状态已漂移（不再 WAITING_FOR_RUNNER）、未触碰的 run id。
    pub skipped_run_ids: Vec<String>,
}

/// Tool Gateway（ROADMAP P2-4 骨架；P10-3 起带多候选故障转移）。
pub struct Gateway {
    engine: Engine,
    store: HealthStore,
    probes: Mutex<HashMap<String, Arc<dyn HeartbeatProbe>>>,
    last_probe: Mutex<HashMap<String, Instant>>,
    probe_interval: std::time::Duration,
    primary_node: Mutex<Option<String>>,
    /// 派发优先序（主节点居首，其余按登记序；set_primary 会重排）。
    registration_order: Mutex<Vec<String>>,
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
            registration_order: Mutex::new(Vec::new()),
        }
    }

    /// 登记 Runner 节点（upsert runtime_node 行 + 装探针）；首个节点自动
    /// 成为主节点，可用 [`Gateway::set_primary`] 更换；后续节点按登记序
    /// 追加为故障转移候选（P10-3）。
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
        let mut order = self
            .registration_order
            .lock()
            .expect("registration order mutex");
        if !order.contains(&node.node_id) {
            order.push(node.node_id.clone());
        }
        let mut primary = self.primary_node.lock().expect("primary node mutex");
        if primary.is_none() {
            *primary = Some(node.node_id);
        }
        Ok(())
    }

    /// 指定主节点（派发与恢复的第一优先候选；其余候选按登记序殿后）。
    pub fn set_primary(&self, node_id: &str) -> Result<(), GatewayError> {
        if !self
            .probes
            .lock()
            .expect("probe registry mutex")
            .contains_key(node_id)
        {
            return Err(GatewayError::UnknownNode(node_id.to_string()));
        }
        {
            let mut order = self
                .registration_order
                .lock()
                .expect("registration order mutex");
            order.retain(|id| id != node_id);
            order.insert(0, node_id.to_string());
        }
        *self.primary_node.lock().expect("primary node mutex") = Some(node_id.to_string());
        Ok(())
    }

    /// 派发候选节点（优先序：主节点居首，其余按登记序；P10-3 故障转移序）。
    pub fn candidate_nodes(&self) -> Vec<String> {
        self.registration_order
            .lock()
            .expect("registration order mutex")
            .clone()
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

    /// 派发（P10-3 故障转移语义）：引擎受理（D-TC-05）后按候选优先序逐节点
    /// 探测（每节点遵守探测间隔），首个可用节点承接（run 落 QUEUED，不丢）；
    /// 全部候选离线才走第 3 行跃迁落 WAITING_FOR_RUNNER（挂主节点；探测失败
    /// 不产生 FAILED，计划 §5.6:336 原文语义）。单节点登记时行为与 P2-4b
    /// 完全一致。
    pub fn dispatch(&self, request: &serde_json::Value) -> Result<DispatchOutcome, GatewayError> {
        let submitted = self.engine.submit(request)?;
        let candidates = self.candidate_nodes();
        let primary = candidates
            .first()
            .cloned()
            .ok_or(GatewayError::NoRunnerRegistered)?;
        let mut primary_health = None;
        for node_id in &candidates {
            let health = self.probe_node(node_id)?;
            if *node_id == primary {
                primary_health = Some(health);
            }
            if !health.is_offline() {
                return Ok(DispatchOutcome::Queued {
                    run_id: submitted.run_id,
                    node_id: node_id.clone(),
                    health,
                });
            }
        }
        self.engine.mark_waiting_for_runner(&submitted.run_id)?;
        Ok(DispatchOutcome::WaitingForRunner {
            run_id: submitted.run_id,
            node_id: primary,
            health: primary_health.expect("primary is always probed"),
        })
    }

    /// 心跳恢复判定（P10-3 多候选）：按候选优先序强制重探；任一候选恢复则
    /// 走第 4 行跃迁回 PREPARING 并返回（继续驱动到终态用
    /// `Engine::drive_resumed`，执行面属后续批次）；全部候选仍离线则报
    /// [`GatewayError::RunnerUnavailable`]（报主节点的健康）且 run 保持
    /// WAITING_FOR_RUNNER。
    pub fn probe_and_resume(&self, run_id: &str) -> Result<domain::ToolRunStatus, GatewayError> {
        let candidates = self.candidate_nodes();
        let primary = candidates
            .first()
            .cloned()
            .ok_or(GatewayError::NoRunnerRegistered)?;
        let mut primary_health = None;
        for node_id in &candidates {
            let health = self.probe_node_forced(node_id)?;
            if *node_id == primary {
                primary_health = Some(health);
            }
            if !health.is_offline() {
                self.engine.resume_to_preparing(run_id)?;
                return Ok(domain::ToolRunStatus::Preparing);
            }
        }
        Err(GatewayError::RunnerUnavailable {
            node_id: primary,
            health: primary_health.expect("primary is always probed"),
        })
    }

    /// 批量恢复（P10-3「恢复续跑」）：每候选节点强制探测一次（结果供本批
    /// 全部待恢复 run 复用，不 tight-poll），然后把 WAITING_FOR_RUNNER 的
    /// run（created_at 序，至多 limit 条）逐条恢复回 PREPARING。任一候选
    /// 可用即恢复；全部候选离线报 [`GatewayError::RunnerUnavailable`] 且
    /// 不触碰任何 run。状态已漂移的 run 记入 skipped（并发迁移不误伤）。
    pub fn recover_waiting_runs(&self, limit: usize) -> Result<RecoveryReport, GatewayError> {
        let candidates = self.candidate_nodes();
        let primary = candidates
            .first()
            .cloned()
            .ok_or(GatewayError::NoRunnerRegistered)?;
        let mut healthy_nodes = Vec::new();
        let mut primary_health = None;
        for node_id in &candidates {
            let health = self.probe_node_forced(node_id)?;
            if *node_id == primary {
                primary_health = Some(health);
            }
            if !health.is_offline() {
                healthy_nodes.push(node_id.clone());
            }
        }
        if healthy_nodes.is_empty() {
            return Err(GatewayError::RunnerUnavailable {
                node_id: primary,
                health: primary_health.expect("primary is always probed"),
            });
        }
        let mut report = RecoveryReport {
            healthy_nodes,
            resumed_run_ids: Vec::new(),
            skipped_run_ids: Vec::new(),
        };
        for run_id in self
            .engine
            .runs_by_status(domain::ToolRunStatus::WaitingForRunner, limit)?
        {
            match self.engine.resume_to_preparing(&run_id) {
                Ok(()) => report.resumed_run_ids.push(run_id),
                Err(tool_engine::EngineError::NotStartable { run_id, .. }) => {
                    report.skipped_run_ids.push(run_id)
                }
                Err(e) => return Err(e.into()),
            }
        }
        Ok(report)
    }
}
