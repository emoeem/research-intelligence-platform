//! 运行层周期健康检查（PLUGIN-CONTRACT §7 运行层；ROADMAP P6-7）。
//!
//! 纪律落进代码：
//! - **间隔下限 1s**：[`MIN_INTERVAL`]，构造即校验（interval_too_short 拒绝），
//!   对齐工作区低并发纪律（不 tight-poll）；
//! - **失败进 ERROR 停止重试**：探测 Failed 的插件经
//!   [`crate::PluginRuntime::mark_error`] 进 ERROR 并从后续周期剔除
//!   （不做密集轮询；§7 运行层原文「失败进 ERROR 并停止重试」）；
//! - **探测可注入**：真实探测（podman 心跳、worker ping）由宿主注入；
//!   内置 stdio 握手探测（project_endpoint 的 health_check 声明，§3.4）
//!   复用 mcp_server 握手实现。
//!
//! 状态机衔接：LOADED / ACTIVE 参与检查；Ok 保持原态（LOADED 不伪装成
//! ACTIVE，§5.2 load/activate 行）；Degraded 保持原态但记录（告警面归
//! Plugin Center，P6-8）。

use std::time::Duration;

use serde::Serialize;

use crate::{PluginRuntime, PluginState, RuntimeError};

/// 检查间隔下限（1s，低并发纪律；不 tight-poll）。
pub const MIN_INTERVAL: Duration = Duration::from_secs(1);

/// 探测结论。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProbeOutcome {
    /// 健康（保持原态）。
    Ok,
    /// 降级（保持原态，告警面展示）。
    Degraded(String),
    /// 失败（进 ERROR，停止重试）。
    Failed(String),
}

/// 探测函数：plugin_id → 结论（真实探测由宿主注入）。
pub type Probe = dyn Fn(&str) -> ProbeOutcome + Send + Sync;

/// 周期健康监视器。
pub struct HealthMonitor {
    interval: Duration,
    probe: Box<Probe>,
}

/// 单插件单周期检查动作。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HealthAction {
    /// 健康，保持原态。
    Stay,
    /// 降级，保持原态。
    Degraded,
    /// 进 ERROR 并从后续周期剔除（停止重试）。
    MarkedError,
    /// 插件不在检查面（非 LOADED/ACTIVE，或已 ERROR）。
    Skipped,
}

/// 单周期报告（机器可读）。
#[derive(Debug, Clone, Serialize)]
pub struct HealthCycleReport {
    /// 周期序号（从 1 起）。
    pub cycle: u64,
    /// 各插件动作。
    pub actions: Vec<(String, HealthAction)>,
}

impl HealthMonitor {
    /// 构造（间隔 < [`MIN_INTERVAL`] 即拒绝：interval_too_short）。
    pub fn new(interval: Duration, probe: Box<Probe>) -> Result<Self, RuntimeError> {
        if interval < MIN_INTERVAL {
            return Err(RuntimeError::new(
                "interval_too_short",
                format!(
                    "health interval {:?} below the {}s minimum (low-concurrency discipline, no tight-polling)",
                    interval,
                    MIN_INTERVAL.as_secs()
                ),
            ));
        }
        Ok(Self { interval, probe })
    }

    /// 检查间隔。
    pub fn interval(&self) -> Duration {
        self.interval
    }

    /// 对单个插件执行一次检查（LOADED / ACTIVE 参与面）。
    /// Failed → mark_error（LOADED|ACTIVE → ERROR，§5.2），返回 MarkedError。
    pub fn check_plugin(
        &self,
        runtime: &PluginRuntime,
        plugin_id: &str,
    ) -> Result<HealthAction, RuntimeError> {
        let Some(plugin) = runtime
            .store()
            .get_active(plugin_id)
            .map_err(RuntimeError::from)?
        else {
            return Ok(HealthAction::Skipped);
        };
        if !matches!(plugin.state, PluginState::Loaded | PluginState::Active) {
            return Ok(HealthAction::Skipped);
        }
        match (self.probe)(plugin_id) {
            ProbeOutcome::Ok => Ok(HealthAction::Stay),
            ProbeOutcome::Degraded(reason) => {
                let _ = reason;
                Ok(HealthAction::Degraded)
            }
            ProbeOutcome::Failed(reason) => {
                // 失败原因进审计面（ERROR 态处置），此处仅落状态。
                let _ = reason;
                runtime.mark_error(plugin_id)?;
                Ok(HealthAction::MarkedError)
            }
        }
    }

    /// 周期执行：每周期对全部 LOADED/ACTIVE 插件各检查一次；Failed 进 ERROR
    /// 后从后续周期剔除（停止重试，不 tight-poll）。`max_cycles` 控制轮数
    /// （真实后台循环由宿主按 interval 驱动，本方法不内建 sleep，测试与
    /// CLI 用有限轮数）。
    pub fn run_cycles(
        &self,
        runtime: &PluginRuntime,
        max_cycles: u64,
    ) -> Result<Vec<HealthCycleReport>, RuntimeError> {
        let mut reports = Vec::new();
        let mut excluded: std::collections::BTreeSet<String> = Default::default();
        for cycle in 1..=max_cycles {
            let mut actions = Vec::new();
            for plugin in runtime.store().list().map_err(RuntimeError::from)? {
                if !plugin.is_active || excluded.contains(plugin.plugin_id.as_str()) {
                    continue;
                }
                let id = plugin.plugin_id.as_str().to_string();
                if !matches!(plugin.state, PluginState::Loaded | PluginState::Active) {
                    continue;
                }
                let action = self.check_plugin(runtime, &id)?;
                if matches!(action, HealthAction::MarkedError) {
                    excluded.insert(id.clone());
                }
                actions.push((id, action));
            }
            reports.push(HealthCycleReport { cycle, actions });
        }
        Ok(reports)
    }
}

/// 内置探测：stdio MCP 握手（project_endpoint 的 health_check 声明，§3.4）。
/// endpoint 为 `[...]` JSON 数组启动命令；握手 Ok/Degraded（异常但协议层
/// 成功）→ Failed（spawn/协议失败）。
pub fn probe_stdio_handshake(
    endpoint_command: Vec<String>,
) -> impl Fn(&str) -> ProbeOutcome + Send + Sync {
    move |_plugin_id| match plugin_registry::source::mcp_server::handshake(&endpoint_command) {
        Ok(_) => ProbeOutcome::Ok,
        Err(e) => ProbeOutcome::Failed(format!(
            "stdio handshake failed [{}]: {}",
            e.code, e.message
        )),
    }
}

/// 恒 Ok 探测（无真实探测面的插件缺省；显式占位，不伪装成真实探测）。
pub fn probe_always_ok() -> impl Fn(&str) -> ProbeOutcome + Send + Sync {
    move |_| ProbeOutcome::Ok
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interval_below_minimum_rejected() {
        let err = match HealthMonitor::new(Duration::from_millis(500), Box::new(probe_always_ok()))
        {
            Err(e) => e,
            Ok(_) => panic!("interval below minimum must be rejected"),
        };
        assert_eq!(err.code, "interval_too_short");
        assert!(HealthMonitor::new(MIN_INTERVAL, Box::new(probe_always_ok())).is_ok());
    }

    #[test]
    fn outcome_vocabulary() {
        let ok = serde_json::to_string(&ProbeOutcome::Ok).unwrap();
        assert_eq!(ok, "\"ok\"");
        let failed = serde_json::to_string(&ProbeOutcome::Failed("x".into())).unwrap();
        assert!(failed.contains("\"failed\""));
    }
}
