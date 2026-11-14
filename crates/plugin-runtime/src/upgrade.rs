//! 升级语义（PLUGIN-CONTRACT §5.3）：
//! 1. update_check 发现兼容新版本叠加 UPDATE_PENDING 标记，当前版本继续运行；
//! 2. 用户确认后新版本拉取到 installed/<id>/<version_new>/，与旧版本目录并存，
//!    新版本先走完整 VALIDATED 检查；
//! 3. 数据迁移钩子失败即回滚到旧版本，不切换指针（对齐计划 314 行「更新先
//!    验证再切换」原则）；
//! 4. 切换 active 版本指针，重新走 REGISTERED 到 ENABLED（重过依赖检查）；
//! 5. 回滚窗口默认保留上一版本一个（草案 D6），窗口结束且用户确认后清理。
//!
//! 本模块承载升级计划与结果的纯类型 + 前置检查（版本可比、身份一致）；
//! 落库与指针切换在 [`crate::store::PluginStore::apply_upgrade`]（单事务，
//! 失败自动回滚）；P10-2 补齐第 5 步的执行面：回滚窗口清理
//! （[`cleanup_rollback_window`]，D6 窗口结束且用户确认后清理旧目录）。

use std::path::Path;

use audit_engine::plugin_actions;
use serde_json::Value;

use crate::store::PluginStore;
use crate::{Plugin, PluginState, RuntimeError};

/// 升级计划（apply_upgrade 的输入快照）。
#[derive(Debug, Clone, serde::Serialize)]
pub struct UpgradePlan {
    /// 插件 id（新 Manifest 必须与在装插件一致，不可借升级换身份）。
    pub plugin_id: String,
    /// 当前 active 版本。
    pub from_version: String,
    /// 目标版本。
    pub version: String,
    /// 回滚窗口保留的版本（草案 D6：默认上一版本一个）。
    pub rollback_window_versions: Vec<String>,
}

/// 升级结果。
#[derive(Debug, Clone, serde::Serialize)]
pub struct UpgradeOutcome {
    /// 插件 id。
    pub plugin_id: String,
    /// 升级前版本。
    pub from_version: String,
    /// 升级后版本。
    pub to_version: String,
    /// 旧版本目录是否保留（回滚窗口，恒 true，清理属 P10-2）。
    pub old_dir_retained: bool,
    /// 新版本落库后状态（REGISTERED，enable 重过检查归 P6-7 激活层）。
    pub state_after: String,
}

/// 构造升级计划并做前置检查（身份一致、版本可比、可升级状态）。
pub fn plan(active: &Plugin, new_manifest: &Value) -> Result<UpgradePlan, RuntimeError> {
    let manifest_id = new_manifest
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(|| RuntimeError::new("manifest_invalid", "manifest has no `id`"))?;
    if manifest_id != active.plugin_id.as_str() {
        return Err(RuntimeError::new(
            "upgrade_identity_mismatch",
            format!(
                "new manifest id {manifest_id:?} does not match installed plugin {:?} (upgrade cannot change identity)",
                active.plugin_id.as_str()
            ),
        ));
    }
    let from_version = active.version.clone();
    let to_version = new_manifest
        .get("version")
        .and_then(Value::as_str)
        .ok_or_else(|| RuntimeError::new("manifest_invalid", "manifest has no `version`"))?;
    let from = plugin_registry::Semver::parse(&from_version).ok_or_else(|| {
        RuntimeError::new(
            "version_invalid",
            format!("active version {from_version:?} not semver"),
        )
    })?;
    let to = plugin_registry::Semver::parse(to_version).ok_or_else(|| {
        RuntimeError::new(
            "version_invalid",
            format!("new version {to_version:?} not semver"),
        )
    })?;
    if to <= from {
        return Err(RuntimeError::new(
            "upgrade_version_not_newer",
            format!("new version {to_version} is not greater than active version {from_version} (§6.2 semver)"),
        ));
    }
    // 升级从已装链任意态发起（当前版本继续运行，§5.3 第 1 步）。
    if active.state == PluginState::Uninstalled || active.state == PluginState::Invalid {
        return Err(RuntimeError::new(
            "illegal_transition",
            format!("upgrade from {} is not allowed (§5.3)", active.state),
        ));
    }
    Ok(UpgradePlan {
        plugin_id: active.plugin_id.as_str().to_string(),
        rollback_window_versions: vec![from_version.clone()],
        from_version,
        version: to_version.to_string(),
    })
}

/// 回滚窗口版本集（P10-2 窗口清理的输入；当前恒为上一版本一个，草案 D6）。
pub fn rollback_window(store: &PluginStore, plugin_id: &str) -> Result<Vec<String>, RuntimeError> {
    let rows = store.list().map_err(RuntimeError::from)?;
    Ok(rows
        .iter()
        .filter(|p| p.plugin_id.as_str() == plugin_id && !p.is_active)
        .map(|p| p.version.clone())
        .collect())
}

/// 跳过的目录清理项（红线守卫留痕：未执行物理删除的原因机器可读）。
#[derive(Debug, Clone, serde::Serialize)]
pub struct SkippedDir {
    /// 跳过的版本。
    pub version: String,
    /// 目录路径。
    pub path: String,
    /// 未删除原因（outside_data_root / illegal_transition / dir_missing）。
    pub reason: String,
}

/// 回滚窗口清理报告（D6：窗口结束且用户确认后清理旧目录）。
#[derive(Debug, Clone, serde::Serialize)]
pub struct CleanupReport {
    /// 插件 id。
    pub plugin_id: String,
    /// 清理时的 active 版本（恒不被清理）。
    pub active_version: String,
    /// 本次清理（跃迁 UNINSTALLED）的版本。
    pub cleaned: Vec<String>,
    /// 保留的版本（active 版本与已是 UNINSTALLED 的历史版本）。
    pub retained: Vec<String>,
    /// 已物理删除的目录（仅 data_root 之下）。
    pub dirs_removed: Vec<String>,
    /// 未删除项与原因（红线守卫：data_root 之外的目录绝不删除）。
    pub dirs_skipped: Vec<SkippedDir>,
    /// 用户确认标记（恒 true，未确认在入口直接拒绝）。
    pub confirmed: bool,
}

/// 路径是否位于 root 之下（词法组件比较；root 为 None 恒 false）。
fn path_under_root(path: &Path, root: Option<&Path>) -> bool {
    match root {
        Some(root) => path.starts_with(root),
        None => false,
    }
}

/// 回滚窗口清理（§5.3 第 5 步执行面，P10-2）：把非 active 旧版本跃迁
/// UNINSTALLED（LOADED|ACTIVE 先 deactivate 复合步），物理删除仅对
/// data_root 之下的 installed 目录执行（红线：绝不删调用方源目录）；
/// active 版本与历史 UNINSTALLED 行不动。每次清理 Emit
/// `plugin.uninstall`（detail result=rollback_window_cleanup）。
///
/// `confirmed=false` 直接拒绝（合同 197 行「用户确认后清理」；CLI 侧
/// `--confirm`、server 侧 body.confirm 同一口径）。
pub fn cleanup_rollback_window(
    store: &PluginStore,
    audit: &audit_engine::AuditEngine,
    actor: &str,
    data_root: Option<&Path>,
    plugin_id: &str,
    confirmed: bool,
) -> Result<CleanupReport, RuntimeError> {
    if !confirmed {
        return Err(RuntimeError::new(
            "cleanup_requires_confirmation",
            "rollback window cleanup requires explicit confirmation (§5.3 step 5; pass --confirm)",
        ));
    }
    let rows = store.list().map_err(RuntimeError::from)?;
    let own: Vec<&Plugin> = rows
        .iter()
        .filter(|p| p.plugin_id.as_str() == plugin_id)
        .collect();
    if own.is_empty() {
        return Err(RuntimeError::new(
            "unknown_plugin",
            format!("plugin {plugin_id:?} not installed"),
        ));
    }
    let Some(active) = own.iter().find(|p| p.is_active) else {
        return Err(RuntimeError::new(
            "unknown_plugin",
            format!("plugin {plugin_id:?} has no active version pointer"),
        ));
    };
    let mut report = CleanupReport {
        plugin_id: plugin_id.to_string(),
        active_version: active.version.clone(),
        cleaned: Vec::new(),
        retained: Vec::new(),
        dirs_removed: Vec::new(),
        dirs_skipped: Vec::new(),
        confirmed: true,
    };
    for row in &own {
        if row.is_active {
            continue; // active 版本恒保留
        }
        if row.state == PluginState::Uninstalled {
            report.retained.push(row.version.clone());
            continue;
        }
        if matches!(row.state, PluginState::Installed | PluginState::Invalid) {
            // 跃迁表不允许从这两态卸载（§5.2），跳过留痕不硬改状态。
            report.dirs_skipped.push(SkippedDir {
                version: row.version.clone(),
                path: row.installed_dir.clone().unwrap_or_default(),
                reason: format!("illegal_transition from {}", row.state),
            });
            continue;
        }
        // 复合步：LOADED|ACTIVE 先 deactivate 再卸载（uninstall 同款纪律）。
        let mut intermediate = row.state;
        if matches!(intermediate, PluginState::Loaded | PluginState::Active) {
            let to = crate::transition_allowed(crate::LifecycleAction::Deactivate, intermediate)
                .ok_or_else(|| {
                    RuntimeError::new(
                        "illegal_transition",
                        format!("deactivate from {intermediate} not allowed"),
                    )
                })?;
            store.set_state(plugin_id, &row.version, to)?;
            intermediate = PluginState::Enabled;
        }
        let to = crate::transition_allowed(crate::LifecycleAction::Uninstall, intermediate)
            .ok_or_else(|| {
                RuntimeError::new(
                    "illegal_transition",
                    format!("uninstall from {intermediate} is not allowed (§5.2)"),
                )
            })?;
        store.set_state(plugin_id, &row.version, to)?;
        report.cleaned.push(row.version.clone());
        // 物理删除守卫：仅 data_root 之下的 installed 目录执行。
        let dir = row.installed_dir.as_deref().map(Path::new);
        let mut dir_removed = false;
        match dir {
            Some(dir) if dir.is_dir() && path_under_root(dir, data_root) => {
                std::fs::remove_dir_all(dir).map_err(|e| {
                    RuntimeError::new(
                        "dir_remove_failed",
                        format!("remove {}: {e}", dir.display()),
                    )
                })?;
                report.dirs_removed.push(dir.to_string_lossy().to_string());
                dir_removed = true;
            }
            Some(dir) => {
                let reason = if dir.is_dir() {
                    "outside_data_root"
                } else {
                    "dir_missing"
                };
                report.dirs_skipped.push(SkippedDir {
                    version: row.version.clone(),
                    path: dir.to_string_lossy().to_string(),
                    reason: reason.to_string(),
                });
            }
            None => {
                report.dirs_skipped.push(SkippedDir {
                    version: row.version.clone(),
                    path: String::new(),
                    reason: "no_installed_dir".to_string(),
                });
            }
        }
        audit.emit_best_effort(
            domain::audit::AuditEvent::new(actor, plugin_actions::UNINSTALL)
                .with_entity("plugin", &format!("{}@{}", plugin_id, row.version))
                .with_detail(
                    serde_json::json!({
                        "result": "rollback_window_cleanup",
                        "active_version": active.version,
                        "dir_removed": dir_removed,
                    })
                    .to_string(),
                ),
        );
    }
    Ok(report)
}
