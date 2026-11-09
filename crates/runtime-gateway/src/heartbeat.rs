//! 心跳探测（ROADMAP P2-4「心跳探测」；ADR §6 原文口径：过渡期 Runner 是
//! 本地 podman，心跳即本机命令可用性检查）。
//!
//! 不 tight-poll：探测周期取 Manifest `probe.interval_seconds` 基线
//! 默认 600s（schemas/tool-manifest/v1.json:270 `default: 600`，TOOL-CONTRACT
//! §7.2「探测周期，默认 600」）；[`due`] 判定距上次探测不足一个周期时
//! 沿用已落库健康值，不重复执行探针命令。
//!
//! 探针结果到 [`domain::NodeHealth`] 的映射沿用 D-TC-07 / M6 判定先例
//! （docs/M6_REPORT.md:42-43 口径向 runtime 节点的推广，TOOL-CONTRACT §7.1）：
//! 探测通过且版本可解析 → ok；探测通过但版本不可得 → degraded；
//! 探测失败/命令缺席 → broken。

use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use domain::NodeHealth;

/// Manifest `probe.interval_seconds` 的 schema 默认值（600s，最小 30）。
pub const DEFAULT_PROBE_INTERVAL: Duration = Duration::from_secs(600);

/// 单次探测结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProbeOutcome {
    /// 命令可用性检查是否通过（exit 0）。
    pub healthy: bool,
    /// 版本指纹（探针 stdout 首行去除空白；空输出为 None）。
    pub version: Option<String>,
    /// 失败/超时说明（审计与 Runtime 页排障用）。
    pub detail: String,
}

/// 心跳探针抽象：Runner 可用性检查的最小接口。
pub trait HeartbeatProbe: Send + Sync {
    /// 执行一次探测（实现方自行保证快速返回，见 [`CommandProbe`] 的超时守卫）。
    fn probe(&self) -> ProbeOutcome;
}

/// 过渡期真实探针：本机命令可用性检查（默认 `podman info --format
/// '{{.Host.Security.Rootless}}'`，ADR §6 口径），带超时守卫防挂死。
pub struct CommandProbe {
    program: String,
    args: Vec<String>,
    timeout: Duration,
}

impl CommandProbe {
    /// podman 默认探针（本机命令可用性检查，不拉镜像、不跑容器）。
    pub fn podman() -> Self {
        Self {
            program: "podman".to_string(),
            args: vec![
                "info".to_string(),
                "--format".to_string(),
                "{{.Host.Security.Rootless}}".to_string(),
            ],
            timeout: Duration::from_secs(5),
        }
    }

    /// 自定义命令探针（VM 阶段换 VM 心跳命令时复用本构造器）。
    pub fn with_command(program: impl Into<String>, args: Vec<String>) -> Self {
        Self {
            program: program.into(),
            args,
            timeout: Duration::from_secs(5),
        }
    }
}

impl HeartbeatProbe for CommandProbe {
    fn probe(&self) -> ProbeOutcome {
        let mut child = match Command::new(&self.program)
            .args(&self.args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .stdin(Stdio::null())
            .spawn()
        {
            Ok(child) => child,
            Err(e) => {
                return ProbeOutcome {
                    healthy: false,
                    version: None,
                    detail: format!("spawn {} failed: {e}", self.program),
                }
            }
        };
        // 超时守卫：轮询 try_wait，到期 kill（不依赖任何外部超时工具）。
        enum ProbeExit {
            Ok(std::process::ExitStatus),
            Failed(String),
        }
        let deadline = Instant::now() + self.timeout;
        let exit = loop {
            match child.try_wait() {
                Ok(Some(status)) => break ProbeExit::Ok(status),
                Ok(None) => {
                    if Instant::now() >= deadline {
                        let _ = child.kill();
                        let _ = child.wait();
                        break ProbeExit::Failed("timeout".to_string());
                    }
                    std::thread::sleep(Duration::from_millis(25));
                }
                Err(e) => break ProbeExit::Failed(format!("wait failed: {e}")),
            }
        };
        match exit {
            ProbeExit::Ok(status) if status.success() => {
                // 子进程已退出，take stdout 安全。
                let text = child
                    .stdout
                    .take()
                    .and_then(|mut s| {
                        use std::io::Read;
                        let mut buf = String::new();
                        s.read_to_string(&mut buf).ok()?;
                        Some(buf)
                    })
                    .map(|s| s.trim().to_string())
                    .unwrap_or_default();
                let version = (!text.is_empty()).then_some(text);
                ProbeOutcome {
                    healthy: true,
                    version,
                    detail: format!("{} exited ok", self.program),
                }
            }
            ProbeExit::Ok(status) => ProbeOutcome {
                healthy: false,
                version: None,
                detail: format!("{} exited with {status}", self.program),
            },
            ProbeExit::Failed(reason) => ProbeOutcome {
                healthy: false,
                version: None,
                detail: format!("{} probe failed: {reason}", self.program),
            },
        }
    }
}

/// 可切换探针（测试与手工演练载具）：内部 AtomicBool 控制健康态，
/// 调用计数供「不 tight-poll」断言使用。
#[derive(Debug, Default)]
pub struct SwitchProbe {
    /// 当前健康态（测试翻转）。
    pub healthy: AtomicBool,
    /// 探针实际执行次数。
    pub calls: AtomicUsize,
    /// 固定版本指纹（healthy 时返回）。
    pub version: std::sync::Mutex<Option<String>>,
}

impl SwitchProbe {
    /// 新建（初始态由调用方给定）。
    pub fn new(healthy: bool) -> Arc<Self> {
        Arc::new(Self {
            healthy: AtomicBool::new(healthy),
            calls: AtomicUsize::new(0),
            version: std::sync::Mutex::new(Some("6.1.4 (test)".to_string())),
        })
    }

    /// 翻转到健康态。
    pub fn set_healthy(&self, healthy: bool) {
        self.healthy.store(healthy, Ordering::SeqCst);
    }
}

impl HeartbeatProbe for SwitchProbe {
    fn probe(&self) -> ProbeOutcome {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let healthy = self.healthy.load(Ordering::SeqCst);
        let version = if healthy {
            self.version.lock().expect("version mutex").clone()
        } else {
            None
        };
        ProbeOutcome {
            healthy,
            version,
            detail: if healthy {
                "switch probe: healthy".to_string()
            } else {
                "switch probe: runner offline".to_string()
            },
        }
    }
}

/// 探针结果到节点健康的映射（D-TC-07 四值；见模块头注释的 M6 判定先例）。
pub fn health_from_probe(outcome: &ProbeOutcome) -> NodeHealth {
    if !outcome.healthy {
        NodeHealth::Broken
    } else if outcome.version.is_some() {
        NodeHealth::Ok
    } else {
        NodeHealth::Degraded
    }
}

/// 是否到达下次探测时点（不 tight-poll 守卫）：
/// 无上次记录（首探）或距上次已满一个周期。
pub fn due(last: Option<Instant>, interval: Duration, now: Instant) -> bool {
    match last {
        None => true,
        Some(last) => now.duration_since(last) >= interval,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn health_mapping_follows_m6_precedent() {
        let ok = health_from_probe(&ProbeOutcome {
            healthy: true,
            version: Some("6.1.4".to_string()),
            detail: String::new(),
        });
        assert_eq!(ok, NodeHealth::Ok);
        let degraded = health_from_probe(&ProbeOutcome {
            healthy: true,
            version: None,
            detail: String::new(),
        });
        assert_eq!(degraded, NodeHealth::Degraded);
        let broken = health_from_probe(&ProbeOutcome {
            healthy: false,
            version: None,
            detail: "exit 1".to_string(),
        });
        assert_eq!(broken, NodeHealth::Broken);
        assert!(broken.is_offline());
    }

    #[test]
    fn due_respects_interval_not_tight_poll() {
        let now = Instant::now();
        assert!(due(None, DEFAULT_PROBE_INTERVAL, now), "first probe is due");
        assert!(!due(Some(now), DEFAULT_PROBE_INTERVAL, now));
        assert!(due(
            Some(now - DEFAULT_PROBE_INTERVAL),
            DEFAULT_PROBE_INTERVAL,
            now
        ));
        let zero = Duration::ZERO;
        assert!(
            due(Some(now), zero, now),
            "zero interval always due (tests)"
        );
    }

    #[test]
    fn command_probe_maps_true_and_false_commands() {
        let ok = CommandProbe::with_command("/bin/true".to_string(), vec![]).probe();
        assert!(ok.healthy, "/bin/true must be healthy: {}", ok.detail);
        let bad = CommandProbe::with_command("/bin/false".to_string(), vec![]).probe();
        assert!(!bad.healthy, "/bin/false must be unhealthy: {}", bad.detail);
        assert_eq!(health_from_probe(&bad), NodeHealth::Broken);
        let missing =
            CommandProbe::with_command("/nonexistent-rip-probe".to_string(), vec![]).probe();
        assert!(!missing.healthy);
        assert!(
            missing.detail.contains("spawn"),
            "detail: {}",
            missing.detail
        );
    }
}
