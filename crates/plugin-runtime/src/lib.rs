//! Phase 6 插件运行时（ROADMAP P6-2；协议基准
//! docs/phase0/PLUGIN-CONTRACT-DRAFT.md §5 生命周期）。
//!
//! 职责：
//! - [`transition_allowed`]：§5.2 跃迁表（REGISTERED→ENABLED→LOADED→ACTIVE
//!   主链 + disable/uninstall/error 分支，纯表驱动）；
//! - [`PluginRuntime`]：生命周期门面。install（校验 + checksums + lock +
//!   落库 REGISTERED）、enable（重过静态检查，§5.3 re-enable 语义）、
//!   disable（先 deactivate，数据不动）、uninstall（前置自动 disable +
//!   §5.4 数据保留处置 + 孤儿来源标记）、upgrade（§5.3 新旧版本目录并存 +
//!   失败自动回滚，[`crate::upgrade`]）；
//! - [`crate::retention`]：§5.4 数据保留表的机器可读处置报告；
//! - [`crate::store`]：plugin / plugin_capability 两表唯一写面（009 迁移）。
//!
//! 审计纪律（PLUGIN-CONTRACT §11.4）：install/enable/disable/upgrade/
//! uninstall 全部经 [`audit_engine::AuditEngine`] Emit（词形见
//! `audit_engine::plugin_actions`），不新增 JSONL 分叉；审计写入走旁路
//! emit_best_effort，审计失败不反压生命周期。
//!
//! 依赖方向（本批约束）：domain + database + audit-engine + plugin-registry，
//! 不反向；quality-engine 的 PluginGovernor（INVALID 处置）在测试环接线，
//! 运行期不依赖（dev-deps 注释与 tool-engine 先例同款）。

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod capability;
pub mod compat;
pub mod deps;
pub mod health;
pub mod ingest;
pub mod knowledge_parse;
pub mod knowledge_review;
pub mod paths;
pub mod retention;
pub mod source_fetch;
pub mod source_sync;
pub mod state_store;
pub mod store;
pub mod upgrade;

pub use compat::CompatReport;
pub use deps::DependencyReport;
pub use domain::plugin::{
    CapabilityType, Plugin, PluginCapability, PluginId, PluginKind, PluginState, PluginTrust,
};
pub use health::{
    probe_always_ok, probe_stdio_handshake, HealthAction, HealthCycleReport, HealthMonitor,
    ProbeOutcome, MIN_INTERVAL,
};
pub use ingest::{
    ingest_knowledge_source, ingest_knowledge_source_with_opts, report_json as ingest_report_json,
    IngestOptions, IngestReport, CONTENT_KINDS, EXTENDED_CONTENT_KINDS, INGEST_PERMISSION_REQUIRED,
    RTIS_CONTENT_KINDS,
};
pub use paths::{
    LayoutReport, PathsError, PluginPaths, ResolveOptions, CONFIG_KEY_DATA_DIR,
    CONFIG_KEY_PLUGINS_DIR, DEFAULT_DATA_DIR_NAME, ENV_DATA_DIR, MANIFEST_FILE_NAME,
    STATE_FILE_NAME, TRANSITIONS_FILE_NAME,
};
pub use retention::{RetentionAction, RetentionItem, RetentionReport};
pub use source_sync::{SyncAttempt, SyncOptions, SyncReport};
pub use state_store::{snapshot_of, transition_record, StateStore};
pub use store::{capabilities_from_manifest, PluginStore};
pub use upgrade::{UpgradeOutcome, UpgradePlan};

use std::path::{Path, PathBuf};

use audit_engine::{plugin_actions, AuditEngine};
use database::Pool;
use plugin_registry::{validate as p6_validate, Options as RegistryOptions};
use serde_json::{json, Value};

/// 运行时错误（机器可读：code + message + 可选 details）。
#[derive(Debug, Clone, serde::Serialize)]
pub struct RuntimeError {
    /// 稳定错误码。
    pub code: String,
    /// 人类可读原因。
    pub message: String,
    /// 结构化明细（如校验报告、校验和失败清单）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<Value>,
}

impl RuntimeError {
    fn new(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.to_string(),
            message: message.into(),
            details: None,
        }
    }

    fn with_details(mut self, details: Value) -> Self {
        self.details = Some(details);
        self
    }

    /// 单行 JSON（机器可读报告）。
    pub fn to_json_string(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{\"code\":\"unserializable\"}".to_string())
    }
}

impl std::fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "plugin runtime error [{}]: {}", self.code, self.message)
    }
}

impl std::error::Error for RuntimeError {}

impl From<database::DbError> for RuntimeError {
    fn from(e: database::DbError) -> Self {
        RuntimeError::new("db", e.to_string())
    }
}

impl From<plugin_registry::SourceError> for RuntimeError {
    fn from(e: plugin_registry::SourceError) -> Self {
        let mut err = RuntimeError::new(&e.code, e.message);
        err.details = e.details;
        err
    }
}

/// 生命周期动作（PLUGIN-CONTRACT §5.2 跃迁表在 P6-2 范围内的单步动作；
/// discover/validate/install 静态三步归 plugin-registry，P6-1 已交付）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LifecycleAction {
    /// capabilities 登记进平台数据面（INSTALLED → REGISTERED）。
    Register,
    /// 启用 / 重启用（REGISTERED|DISABLED|ERROR → ENABLED；重过检查）。
    Enable,
    /// 禁用（ENABLED → DISABLED；LOADED/ACTIVE 先 deactivate 走复合步）。
    Disable,
    /// 运行资源加载（ENABLED → LOADED；系统按需）。
    Load,
    /// 首个 capability 服务调用成功（LOADED → ACTIVE）。
    Activate,
    /// 释放运行资源（LOADED|ACTIVE → ENABLED）。
    Deactivate,
    /// 健康检查失败或运行期故障（LOADED|ACTIVE → ERROR）。
    MarkError,
    /// 卸载（REGISTERED|ENABLED|DISABLED → UNINSTALLED；LOADED/ACTIVE 复合步）。
    Uninstall,
}

impl LifecycleAction {
    /// snake_case 稳定词形。
    pub fn as_str(&self) -> &'static str {
        match self {
            LifecycleAction::Register => "register",
            LifecycleAction::Enable => "enable",
            LifecycleAction::Disable => "disable",
            LifecycleAction::Load => "load",
            LifecycleAction::Activate => "activate",
            LifecycleAction::Deactivate => "deactivate",
            LifecycleAction::MarkError => "mark_error",
            LifecycleAction::Uninstall => "uninstall",
        }
    }
}

/// §5.2 跃迁表（P6-2 范围单步动作 × 十二态；纯表驱动，无 IO）。
pub const TRANSITIONS: &[(LifecycleAction, PluginState, PluginState)] = &[
    (
        LifecycleAction::Register,
        PluginState::Installed,
        PluginState::Registered,
    ),
    (
        LifecycleAction::Enable,
        PluginState::Registered,
        PluginState::Enabled,
    ),
    (
        LifecycleAction::Enable,
        PluginState::Disabled,
        PluginState::Enabled,
    ),
    (
        LifecycleAction::Enable,
        PluginState::Error,
        PluginState::Enabled,
    ),
    (
        LifecycleAction::Disable,
        PluginState::Enabled,
        PluginState::Disabled,
    ),
    (
        LifecycleAction::Load,
        PluginState::Enabled,
        PluginState::Loaded,
    ),
    (
        LifecycleAction::Activate,
        PluginState::Loaded,
        PluginState::Active,
    ),
    (
        LifecycleAction::Deactivate,
        PluginState::Loaded,
        PluginState::Enabled,
    ),
    (
        LifecycleAction::Deactivate,
        PluginState::Active,
        PluginState::Enabled,
    ),
    (
        LifecycleAction::MarkError,
        PluginState::Loaded,
        PluginState::Error,
    ),
    (
        LifecycleAction::MarkError,
        PluginState::Active,
        PluginState::Error,
    ),
    (
        LifecycleAction::Uninstall,
        PluginState::Registered,
        PluginState::Uninstalled,
    ),
    (
        LifecycleAction::Uninstall,
        PluginState::Enabled,
        PluginState::Uninstalled,
    ),
    (
        LifecycleAction::Uninstall,
        PluginState::Disabled,
        PluginState::Uninstalled,
    ),
];

/// 单步跃迁合法性：合法返回目标态，非法返回 None。
pub fn transition_allowed(action: LifecycleAction, from: PluginState) -> Option<PluginState> {
    TRANSITIONS
        .iter()
        .find(|(a, f, _)| *a == action && *f == from)
        .map(|(_, _, t)| *t)
}

/// install 结果。
#[derive(Debug, Clone, serde::Serialize)]
pub struct InstalledOutcome {
    /// 插件 id。
    pub plugin_id: String,
    /// 安装版本。
    pub version: String,
    /// 落库后状态（install + register 两步后恒为 REGISTERED）。
    pub state_after: String,
    /// 包指纹（checksums.sha256 文件 sha256）。
    pub lock_sha256: String,
    /// tool/tool_pack capability 注册报告（P6-6）。
    pub tool_registration: capability::tool_register::ToolRegistrationReport,
    /// project_endpoint capability 注册报告（P6-6）。
    pub endpoint_registration: Vec<capability::project_endpoint::EndpointRegistrationReport>,
}

/// enable 结果。
#[derive(Debug, Clone, serde::Serialize)]
pub struct EnabledOutcome {
    /// 插件 id。
    pub plugin_id: String,
    /// 启用的版本。
    pub version: String,
    /// 跃迁前状态（REGISTERED 首启 / DISABLED 重启用 / ERROR 修复恢复）。
    pub from_state: String,
}

/// disable 结果。
#[derive(Debug, Clone, serde::Serialize)]
pub struct DisabledOutcome {
    /// 插件 id。
    pub plugin_id: String,
    /// 禁用的版本。
    pub version: String,
    /// 跃迁前状态（ENABLED 直禁 / LOADED|ACTIVE 先 deactivate 复合禁）。
    pub from_state: String,
    /// 复合步是否发生过（LOADED|ACTIVE → ENABLED 的内部 deactivate）。
    pub deactivated_first: bool,
}

/// 插件运行时门面（生命周期 + 审计 + 存储唯一写面）。
pub struct PluginRuntime {
    store: PluginStore,
    audit: AuditEngine,
    registry_options: RegistryOptions,
    data_root: Option<PathBuf>,
    platform: compat::Platform,
}

impl PluginRuntime {
    /// 在连接池 + 审计引擎上打开运行时（生产语义：平台版本 0.2.0，
    /// 未配置 data root 时卸载只出报告不执行删除）。
    pub fn open(pool: Pool, audit: AuditEngine) -> Self {
        Self {
            store: PluginStore::open(pool),
            audit,
            registry_options: RegistryOptions::default(),
            data_root: None,
            platform: compat::Platform::default(),
        }
    }

    /// 覆盖激活层兼容检查的平台版本面（enable 求值对象；缺省 0.2.0 / 1 / v1）。
    pub fn with_platform(mut self, platform: compat::Platform) -> Self {
        self.platform = platform;
        self
    }

    /// 覆盖 plugin-registry 校验配置（平台版本自定义用）。
    pub fn with_registry_options(mut self, options: RegistryOptions) -> Self {
        self.registry_options = options;
        self
    }

    /// 配置数据根（rip.data_dir，P6-3 布局承载）：uninstall 仅对位于
    /// data_root 之下的 installed 目录执行物理删除，外部目录只出报告
    /// （红线：绝不删调用方源目录）。
    pub fn with_data_root(mut self, data_root: impl Into<PathBuf>) -> Self {
        self.data_root = Some(data_root.into());
        self
    }

    /// 底层存储视图（测试与只读对账用）。
    pub fn store(&self) -> &PluginStore {
        &self.store
    }

    fn emit(&self, actor: &str, action: &str, plugin_id: &str, version: &str, detail: Value) {
        let event = domain::audit::AuditEvent::new(actor, action)
            .with_entity("plugin", &format!("{plugin_id}@{version}"))
            .with_detail(detail.to_string());
        // §11.4：唯一 Emit 出口走旁路（审计失败不反压生命周期）。
        self.audit.emit_best_effort(event);
    }

    fn manifest_value(manifest_json: &str) -> Result<Value, RuntimeError> {
        serde_json::from_str(manifest_json)
            .map_err(|e| RuntimeError::new("manifest_corrupt", format!("stored manifest: {e}")))
    }

    /// 装前静态校验（plugin-registry 七条，复用 P6-1 校验器）。
    /// existing_ids 传入「除自身外的已激活插件 id」执行 id 唯一性。
    fn validate_manifest(
        &self,
        manifest: &Value,
        package_dir: Option<&Path>,
        exclude_id: Option<&str>,
    ) -> Result<p6_validate::ValidationReport, RuntimeError> {
        let existing: std::collections::HashSet<String> = self
            .store
            .active_ids()?
            .into_iter()
            .filter(|id| Some(id.as_str()) != exclude_id)
            .collect();
        let report =
            p6_validate::validate(manifest, package_dir, &existing, &self.registry_options);
        if !report.ok {
            let details = serde_json::to_value(&report).unwrap_or(Value::Null);
            return Err(RuntimeError::new(
                "validation_failed",
                format!("manifest rejected: {}", report.to_json_string()),
            )
            .with_details(details));
        }
        Ok(report)
    }

    /// install（§5.2 install 行）：静态校验 → checksums（有则验、无则写）→
    /// plugin.lock 写入 → 版本行落库 INSTALLED → register 跃迁 REGISTERED。
    /// Emit `plugin.install`（§11.4 五类事件之一）。
    pub fn install(
        &self,
        actor: &str,
        manifest: &Value,
        package_dir: &Path,
        trust: PluginTrust,
    ) -> Result<InstalledOutcome, RuntimeError> {
        let manifest = normalize(manifest)?;
        let plugin_id = manifest
            .get("id")
            .and_then(Value::as_str)
            .ok_or_else(|| RuntimeError::new("manifest_invalid", "manifest has no `id`"))?;
        let version = manifest
            .get("version")
            .and_then(Value::as_str)
            .ok_or_else(|| RuntimeError::new("manifest_invalid", "manifest has no `version`"))?;
        // P6-6：project_endpoint 握手与工件预检（validate 之前：entry 描述
        // 文件由 stage 生成，rule 7 依赖其在包内；endpoint 形态与缺失在
        // stage 内显式拒绝，不触源即 fail-closed。对 unvalidated Manifest
        // spawn 的信任口径同 §4.1 lifecycle hooks：经受控 runtime 执行，
        // 深度加固归 P6-7 运行层）。stage 产出协商快照与 capability 描述
        // 文件进包，tools 快照哈希随之进 lock.negotiation。
        capability::project_endpoint::stage(&manifest, package_dir)?;
        self.validate_manifest(&manifest, Some(package_dir), Some(plugin_id))?;

        // 校验和：包内已有清单则 verify（fail-closed），没有则先写入。
        let checksum_manifest = if package_dir
            .join(plugin_registry::CHECKSUMS_FILENAME)
            .is_file()
        {
            plugin_registry::verify_checksums(package_dir).map_err(|(e, issues)| {
                RuntimeError::new(
                    "checksum_verification_failed",
                    format!("plugin {plugin_id:?}: {e}"),
                )
                .with_details(serde_json::to_value(&issues).unwrap_or(Value::Null))
            })?
        } else {
            plugin_registry::write_checksums(package_dir).map_err(|e| {
                RuntimeError::new(
                    "checksum_write_failed",
                    format!("plugin {plugin_id:?}: {e}"),
                )
            })?
        };

        // plugin.lock（§5.2 install 行：校验和与 lock 写入）。
        let negotiation = read_negotiation(package_dir);
        let lock = build_lock(
            &manifest,
            plugin_id,
            version,
            &checksum_manifest.file_sha256,
            trust,
            negotiation,
        )?;
        plugin_registry::write_lock(package_dir, &lock).map_err(|e| {
            RuntimeError::new("lock_write_failed", format!("plugin {plugin_id:?}: {e}"))
        })?;

        let entity = plugin_from_manifest(&manifest, trust)?;
        let mut plugin = entity;
        plugin.state = PluginState::Installed;
        plugin.is_active = true;
        plugin.lock_sha256 = Some(checksum_manifest.file_sha256.clone());
        plugin.installed_dir = Some(package_dir.to_string_lossy().to_string());
        let capabilities = capabilities_from_manifest(&plugin.plugin_id, version, &manifest)?;
        self.store.insert_installed(&plugin, &capabilities)?;
        // register 跃迁（capabilities 已随版本行落库，即数据面登记完成）。
        self.store
            .set_state(plugin_id, version, PluginState::Registered)?;

        // P6-6：capability 深注册（REGISTERED 半边）。tool/tool_pack → Tool
        // Registry；project_endpoint → mcp_client 工具映射（注册阶段二次
        // 握手，与 stage 快照对账）。
        let tool_report = capability::tool_register::register_tool_capabilities(
            self.store.pool(),
            &manifest,
            package_dir,
        )?;
        let endpoint_reports = capability::project_endpoint::register_tools(
            self.store.pool(),
            &manifest,
            package_dir,
        )?;

        self.emit(
            actor,
            plugin_actions::INSTALL,
            plugin_id,
            version,
            json!({"result": "installed", "lock_sha256": checksum_manifest.file_sha256,
                   "trust_level": trust.as_str(), "capabilities": capabilities.len(),
                   "tool_registration": tool_report,
                   "endpoint_registration": endpoint_reports}),
        );
        Ok(InstalledOutcome {
            plugin_id: plugin_id.to_string(),
            version: version.to_string(),
            state_after: PluginState::Registered.as_str().to_string(),
            lock_sha256: checksum_manifest.file_sha256,
            tool_registration: tool_report,
            endpoint_registration: endpoint_reports,
        })
    }

    /// enable（§5.2 enable 行 / §5.3 re-enable）：重过静态检查（兼容 +
    /// 结构，包体在位时含 entry 存在性），失败拒绝且状态不变（机器可读
    /// 报告）；通过则跃迁 ENABLED 并 Emit `plugin.enable`。
    pub fn enable(&self, actor: &str, plugin_id: &str) -> Result<EnabledOutcome, RuntimeError> {
        let plugin = self.store.get_active(plugin_id)?.ok_or_else(|| {
            RuntimeError::new(
                "unknown_plugin",
                format!("plugin {plugin_id:?} not installed"),
            )
        })?;
        let to = transition_allowed(LifecycleAction::Enable, plugin.state).ok_or_else(|| {
            RuntimeError::new(
                "illegal_transition",
                format!("enable from {} is not allowed (§5.2)", plugin.state),
            )
        })?;
        let manifest = Self::manifest_value(&plugin.manifest_json)?;
        let package_dir = plugin
            .installed_dir
            .as_deref()
            .map(Path::new)
            .filter(|d| d.is_dir());
        // §5.3 re-enable：重过依赖与兼容检查。三层口径：
        // - 静态层（VALIDATED 七条，validate_manifest）；
        // - 激活层（P6-7）：依赖解析 + capability requires + 依赖环 +
        //   compatibility 三项区间比对，失败即拒绝且状态不变；
        // - 运行层（周期健康检查）归 health.rs（LOADED/ACTIVE 期间）。
        self.validate_manifest(&manifest, package_dir, Some(plugin_id))?;
        let dep_report = deps::resolve_dependencies(self.store.pool(), &manifest, plugin_id)?;
        if !dep_report.passes() {
            let mut err = RuntimeError::new(
                "dependency_check_failed",
                format!("plugin {plugin_id:?} enable rejected by dependency resolution (machine-readable list in details)"),
            );
            err.details = Some(serde_json::to_value(&dep_report).unwrap_or(Value::Null));
            return Err(err);
        }
        let compat_report = compat::check_compatibility(&manifest, &self.platform)?;
        if !compat_report.ok {
            let mut err = RuntimeError::new(
                "compat_check_failed",
                format!("plugin {plugin_id:?} enable rejected by compatibility check (report in details)"),
            );
            err.details = Some(serde_json::to_value(&compat_report).unwrap_or(Value::Null));
            return Err(err);
        }
        self.store.set_state(plugin_id, &plugin.version, to)?;
        self.emit(
            actor,
            plugin_actions::ENABLE,
            plugin_id,
            &plugin.version,
            json!({"from_state": plugin.state.as_str(), "recheck": "static_seven_rules"}),
        );
        Ok(EnabledOutcome {
            plugin_id: plugin_id.to_string(),
            version: plugin.version.clone(),
            from_state: plugin.state.as_str().to_string(),
        })
    }

    /// disable（§5.3）：LOADED/ACTIVE 先 deactivate（复合步），再禁用；
    /// installed 内容与数据全保留（不调用任何删除路径），Emit
    /// `plugin.disable`。
    pub fn disable(&self, actor: &str, plugin_id: &str) -> Result<DisabledOutcome, RuntimeError> {
        let plugin = self.store.get_active(plugin_id)?.ok_or_else(|| {
            RuntimeError::new(
                "unknown_plugin",
                format!("plugin {plugin_id:?} not installed"),
            )
        })?;
        let mut deactivated_first = false;
        let from = plugin.state;
        if matches!(from, PluginState::Loaded | PluginState::Active) {
            // 复合步：deactivate（LOADED|ACTIVE → ENABLED）后禁用。
            let to = transition_allowed(LifecycleAction::Deactivate, from).ok_or_else(|| {
                RuntimeError::new(
                    "illegal_transition",
                    format!("deactivate from {from} not allowed"),
                )
            })?;
            self.store.set_state(plugin_id, &plugin.version, to)?;
            deactivated_first = true;
        }
        let to = transition_allowed(
            LifecycleAction::Disable,
            if deactivated_first {
                PluginState::Enabled
            } else {
                from
            },
        )
        .ok_or_else(|| {
            RuntimeError::new(
                "illegal_transition",
                format!("disable from {} is not allowed (§5.2)", from),
            )
        })?;
        self.store.set_state(plugin_id, &plugin.version, to)?;
        // §5.3 disable：capability 撤出匹配面（P6-7 Broker 接线），数据不动。
        self.emit(
            actor,
            plugin_actions::DISABLE,
            plugin_id,
            &plugin.version,
            json!({"from_state": from.as_str(), "deactivated_first": deactivated_first,
                   "data_retained": true}),
        );
        Ok(DisabledOutcome {
            plugin_id: plugin_id.to_string(),
            version: plugin.version.clone(),
            from_state: from.as_str().to_string(),
            deactivated_first,
        })
    }

    /// uninstall（§5.3）：前置自动 disable（复合步），跃迁 UNINSTALLED，
    /// 标记来源已卸载（已入主库知识保留带 plugin_id 溯源，计划 415 行），
    /// 按 §5.4 出保留处置报告；物理删除仅对 data_root 之下的目录执行。
    /// Emit `plugin.uninstall`。
    pub fn uninstall(&self, actor: &str, plugin_id: &str) -> Result<RetentionReport, RuntimeError> {
        let plugin = self.store.get_active(plugin_id)?.ok_or_else(|| {
            RuntimeError::new(
                "unknown_plugin",
                format!("plugin {plugin_id:?} not installed"),
            )
        })?;
        let from = plugin.state;
        // 前置自动 disable（复合步）。
        let mut intermediate = from;
        if matches!(from, PluginState::Loaded | PluginState::Active) {
            let to = transition_allowed(LifecycleAction::Deactivate, from).ok_or_else(|| {
                RuntimeError::new(
                    "illegal_transition",
                    format!("deactivate from {from} not allowed"),
                )
            })?;
            self.store.set_state(plugin_id, &plugin.version, to)?;
            intermediate = PluginState::Enabled;
        }
        let to = transition_allowed(LifecycleAction::Uninstall, intermediate).ok_or_else(|| {
            RuntimeError::new(
                "illegal_transition",
                format!("uninstall from {} is not allowed (§5.2)", from),
            )
        })?;
        self.store.set_state(plugin_id, &plugin.version, to)?;
        self.store
            .mark_source_uninstalled(plugin_id, &plugin.version)?;

        let manifest = Self::manifest_value(&plugin.manifest_json)?;
        let installed_dir = plugin.installed_dir.as_deref().map(Path::new);
        let report = retention::uninstall_report(
            plugin_id,
            &plugin.version,
            &manifest,
            installed_dir,
            self.data_root.as_deref(),
        );
        // §5.4 处置执行：仅对 data_root 之下的插件目录执行物理删除。
        for item in &report.items {
            if item.action == RetentionAction::Delete && item.executed {
                if let Some(target) = &item.target {
                    let _ = std::fs::remove_dir_all(target);
                }
            }
        }
        self.emit(
            actor,
            plugin_actions::UNINSTALL,
            plugin_id,
            &plugin.version,
            json!({"from_state": from.as_str(), "source_uninstalled": true,
                   "retention": report.to_json_string()}),
        );
        Ok(report)
    }

    /// update_check（§5.1）：远端存在兼容新版本时叠加 UPDATE_PENDING 标记
    /// （状态本身不变），无新版（版本相同或更旧）清除标记。
    pub fn update_check(
        &self,
        plugin_id: &str,
        remote_version: &str,
    ) -> Result<bool, RuntimeError> {
        let plugin = self.store.get_active(plugin_id)?.ok_or_else(|| {
            RuntimeError::new(
                "unknown_plugin",
                format!("plugin {plugin_id:?} not installed"),
            )
        })?;
        let current = plugin_registry::Semver::parse(&plugin.version).ok_or_else(|| {
            RuntimeError::new(
                "version_invalid",
                format!("stored version {:?} not semver", plugin.version),
            )
        })?;
        let has_new = plugin_registry::Semver::parse(remote_version)
            .map(|remote| remote > current)
            .unwrap_or(false);
        self.store
            .set_update_pending(plugin_id, &plugin.version, has_new)?;
        Ok(has_new)
    }

    /// upgrade（§5.3）：见 [`crate::upgrade`]。Emit `plugin.upgrade`
    /// （成功 result=switched；失败 result=rolled_back，指针不切换）。
    pub fn upgrade(
        &self,
        actor: &str,
        plugin_id: &str,
        new_manifest: &Value,
        new_package_dir: &Path,
        trust: PluginTrust,
    ) -> Result<UpgradeOutcome, RuntimeError> {
        let active = self.store.get_active(plugin_id)?.ok_or_else(|| {
            RuntimeError::new(
                "unknown_plugin",
                format!("plugin {plugin_id:?} not installed"),
            )
        })?;
        let new_manifest = normalize(new_manifest)?;
        let plan = upgrade::plan(&active, &new_manifest)?;
        // 新版本先走完整静态检查（§5.3 第 2 步）。
        self.validate_manifest(&new_manifest, Some(new_package_dir), Some(plugin_id))?;

        // 新版本 checksums + lock（拉取校验步骤，失败即回滚不切指针）。
        let checksum_result = if new_package_dir
            .join(plugin_registry::CHECKSUMS_FILENAME)
            .is_file()
        {
            plugin_registry::verify_checksums(new_package_dir)
        } else {
            plugin_registry::write_checksums(new_package_dir).map_err(|e| (e, Vec::new()))
        };
        let checksum_manifest = match checksum_result {
            Ok(m) => m,
            Err((e, issues)) => {
                self.emit_upgrade_rolled_back(actor, &active, &plan.version, &e.to_string());
                return Err(RuntimeError::new(
                    "checksum_verification_failed",
                    format!("plugin {plugin_id:?}: {e}"),
                )
                .with_details(serde_json::to_value(&issues).unwrap_or(Value::Null)));
            }
        };
        let negotiation = read_negotiation(new_package_dir);
        let lock = match build_lock(
            &new_manifest,
            plugin_id,
            &plan.version,
            &checksum_manifest.file_sha256,
            trust,
            negotiation,
        ) {
            Ok(l) => l,
            Err(e) => {
                self.emit_upgrade_rolled_back(actor, &active, &plan.version, &e.message);
                return Err(e);
            }
        };
        if let Err(e) = plugin_registry::write_lock(new_package_dir, &lock) {
            self.emit_upgrade_rolled_back(actor, &active, &plan.version, &e.message);
            return Err(RuntimeError::new(
                "lock_write_failed",
                format!("plugin {plugin_id:?}: {e}"),
            ));
        }

        // 新版本落库 + 指针切换（单事务：失败自动回滚，不切指针，§5.3 第 3 步）。
        let new_entity = plugin_from_manifest(&new_manifest, trust)?;
        let mut new_plugin = new_entity;
        new_plugin.state = PluginState::Installed;
        new_plugin.is_active = false; // 事务内切换
        new_plugin.lock_sha256 = Some(checksum_manifest.file_sha256.clone());
        new_plugin.installed_dir = Some(new_package_dir.to_string_lossy().to_string());
        let capabilities =
            capabilities_from_manifest(&new_plugin.plugin_id, &plan.version, &new_manifest)?;
        self.store
            .apply_upgrade(&active, &new_plugin, &capabilities)?;

        self.emit(
            actor,
            plugin_actions::UPGRADE,
            plugin_id,
            &plan.version,
            json!({"result": "switched", "from_version": plan.from_version,
                   "old_dir_retained": true, "rollback_window": plan.rollback_window_versions}),
        );
        Ok(UpgradeOutcome {
            plugin_id: plugin_id.to_string(),
            from_version: plan.from_version.clone(),
            to_version: plan.version.clone(),
            old_dir_retained: true,
            state_after: PluginState::Registered.as_str().to_string(),
        })
    }

    fn emit_upgrade_rolled_back(
        &self,
        actor: &str,
        active: &Plugin,
        attempted: &str,
        reason: &str,
    ) {
        self.emit(
            actor,
            plugin_actions::UPGRADE,
            active.plugin_id.as_str(),
            &active.version,
            json!({"result": "rolled_back", "attempted_version": attempted, "reason": reason,
                   "active_version_unchanged": true}),
        );
    }

    /// load（ENABLED → LOADED，系统按需；runtime 健康检查在 P6-7 接线）。
    pub fn load(&self, plugin_id: &str) -> Result<PluginState, RuntimeError> {
        self.simple_transition(plugin_id, LifecycleAction::Load)
    }

    /// activate（LOADED → ACTIVE，首个 capability 服务调用成功）。
    pub fn activate(&self, plugin_id: &str) -> Result<PluginState, RuntimeError> {
        self.simple_transition(plugin_id, LifecycleAction::Activate)
    }

    /// deactivate（LOADED|ACTIVE → ENABLED，释放运行资源）。
    pub fn deactivate(&self, plugin_id: &str) -> Result<PluginState, RuntimeError> {
        self.simple_transition(plugin_id, LifecycleAction::Deactivate)
    }

    /// mark_error（LOADED|ACTIVE → ERROR，健康检查失败；不做密集轮询）。
    pub fn mark_error(&self, plugin_id: &str) -> Result<PluginState, RuntimeError> {
        self.simple_transition(plugin_id, LifecycleAction::MarkError)
    }

    fn simple_transition(
        &self,
        plugin_id: &str,
        action: LifecycleAction,
    ) -> Result<PluginState, RuntimeError> {
        let plugin = self.store.get_active(plugin_id)?.ok_or_else(|| {
            RuntimeError::new(
                "unknown_plugin",
                format!("plugin {plugin_id:?} not installed"),
            )
        })?;
        let to = transition_allowed(action, plugin.state).ok_or_else(|| {
            RuntimeError::new(
                "illegal_transition",
                format!(
                    "{} from {} is not allowed (§5.2)",
                    action.as_str(),
                    plugin.state
                ),
            )
        })?;
        self.store.set_state(plugin_id, &plugin.version, to)?;
        Ok(to)
    }
}

/// Manifest 规范化（BTreeMap 键序确定性 JSON，存储列口径与 content_hash 一致）。
fn normalize(manifest: &Value) -> Result<Value, RuntimeError> {
    serde_json::to_value(manifest).map_err(|e| {
        RuntimeError::new(
            "manifest_invalid",
            format!("manifest not JSON-serializable: {e}"),
        )
    })
}

/// 从 Manifest 构造 Plugin 实体（install/upgrade 共用；状态由调用方置）。
fn plugin_from_manifest(manifest: &Value, trust: PluginTrust) -> Result<Plugin, RuntimeError> {
    let obj = manifest
        .as_object()
        .ok_or_else(|| RuntimeError::new("manifest_invalid", "manifest must be a JSON object"))?;
    let plugin_id = PluginId::try_new(
        obj.get("id")
            .and_then(Value::as_str)
            .ok_or_else(|| RuntimeError::new("manifest_invalid", "manifest has no `id`"))?,
    )
    .map_err(|e| RuntimeError::new("manifest_invalid", e.to_string()))?;
    let version = obj
        .get("version")
        .and_then(Value::as_str)
        .ok_or_else(|| RuntimeError::new("manifest_invalid", "manifest has no `version`"))?
        .to_string();
    let kind = PluginKind::parse(
        obj.get("kind")
            .and_then(Value::as_str)
            .ok_or_else(|| RuntimeError::new("manifest_invalid", "manifest has no `kind`"))?,
    )
    .map_err(|e| RuntimeError::new("manifest_invalid", e.to_string()))?;
    let source = obj
        .get("source")
        .and_then(Value::as_object)
        .ok_or_else(|| RuntimeError::new("manifest_invalid", "manifest has no `source`"))?;
    let source_type = source
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let source_ref = ["path", "url", "endpoint", "base_url"]
        .iter()
        .find_map(|k| source.get(*k).and_then(Value::as_str))
        .unwrap_or_default()
        .to_string();
    let pinned_json = source.get("pinned").map(|p| p.to_string()).or_else(|| {
        source
            .get("sha256")
            .map(|s| json!({"kind": "sha256", "value": s}).to_string())
    });
    let data_retention = obj.get("data_retention").and_then(Value::as_object);
    let ingested_policy = data_retention
        .and_then(|d| d.get("ingested_policy"))
        .and_then(Value::as_str)
        .map(str::to_string);
    let now = domain::time::now_iso();
    Ok(Plugin {
        plugin_id,
        version,
        name: obj
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        kind,
        description: obj
            .get("description")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        license: obj
            .get("license")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        trust_level: trust,
        state: PluginState::Installed,
        update_pending: false,
        is_active: true,
        source_type,
        source_ref,
        pinned_json,
        lock_sha256: None,
        manifest_json: manifest.to_string(),
        installed_dir: None,
        ingested_policy,
        source_uninstalled: false,
        created_at: now.clone(),
        updated_at: now,
    })
}

/// 构造 plugin.lock（复用 plugin-registry 的 PluginLock 面）。
fn build_lock(
    manifest: &Value,
    plugin_id: &str,
    version: &str,
    checksums_sha256: &str,
    trust: PluginTrust,
    negotiation: Option<Value>,
) -> Result<plugin_registry::PluginLock, RuntimeError> {
    let obj = manifest
        .as_object()
        .ok_or_else(|| RuntimeError::new("manifest_invalid", "manifest must be a JSON object"))?;
    let source = obj
        .get("source")
        .and_then(Value::as_object)
        .ok_or_else(|| RuntimeError::new("manifest_invalid", "manifest has no `source`"))?;
    let source_type = source
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let source_ref = ["path", "url", "endpoint", "base_url"]
        .iter()
        .find_map(|k| source.get(*k).and_then(Value::as_str))
        .unwrap_or_default()
        .to_string();
    let pinned =
        source
            .get("pinned")
            .and_then(Value::as_object)
            .map(|p| plugin_registry::PinnedRef {
                kind: p
                    .get("kind")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                value: p
                    .get("value")
                    .and_then(Value::as_str)
                    .map(str::to_string)
                    .unwrap_or_else(|| {
                        source
                            .get("sha256")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_string()
                    }),
            });
    let trust_level = match trust {
        PluginTrust::Official => plugin_registry::TrustLevel::Official,
        PluginTrust::Community => plugin_registry::TrustLevel::Community,
        PluginTrust::Local => plugin_registry::TrustLevel::Local,
        PluginTrust::Remote => plugin_registry::TrustLevel::Remote,
    };
    Ok(plugin_registry::PluginLock {
        plugin_id: plugin_id.to_string(),
        resolved_version: version.to_string(),
        source_type,
        source_ref,
        pinned,
        sha256: checksums_sha256.to_string(),
        license: obj
            .get("license")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        trust_level,
        installed_at_unix: plugin_registry::now_unix(),
        negotiation,
    })
}

/// 读取包内协商快照（mcp_server 握手 / http_api 契约，P6-4；无则 None）。
fn read_negotiation(package_dir: &Path) -> Option<Value> {
    std::fs::read(package_dir.join("negotiation.json"))
        .ok()
        .and_then(|b| serde_json::from_str(&String::from_utf8_lossy(&b)).ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transition_table_covers_contract_5_2() {
        // 主链四态 + enable 三入口 + 复合步基础。
        assert_eq!(
            transition_allowed(LifecycleAction::Register, PluginState::Installed),
            Some(PluginState::Registered)
        );
        assert_eq!(
            transition_allowed(LifecycleAction::Enable, PluginState::Registered),
            Some(PluginState::Enabled)
        );
        assert_eq!(
            transition_allowed(LifecycleAction::Enable, PluginState::Disabled),
            Some(PluginState::Enabled)
        );
        assert_eq!(
            transition_allowed(LifecycleAction::Enable, PluginState::Error),
            Some(PluginState::Enabled)
        );
        assert_eq!(
            transition_allowed(LifecycleAction::Load, PluginState::Enabled),
            Some(PluginState::Loaded)
        );
        assert_eq!(
            transition_allowed(LifecycleAction::Activate, PluginState::Loaded),
            Some(PluginState::Active)
        );
        // 非法跃迁。
        assert_eq!(
            transition_allowed(LifecycleAction::Enable, PluginState::Installed),
            None
        );
        assert_eq!(
            transition_allowed(LifecycleAction::Disable, PluginState::Registered),
            None
        );
        assert_eq!(
            transition_allowed(LifecycleAction::Load, PluginState::Loaded),
            None
        );
        assert_eq!(
            transition_allowed(LifecycleAction::Uninstall, PluginState::Installed),
            None
        );
        // 终态不可再跃迁。
        for action in [
            LifecycleAction::Enable,
            LifecycleAction::Disable,
            LifecycleAction::Uninstall,
            LifecycleAction::Load,
        ] {
            assert_eq!(transition_allowed(action, PluginState::Uninstalled), None);
        }
    }
}
