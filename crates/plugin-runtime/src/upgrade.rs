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
//! 失败自动回滚）；迁移钩子执行面属 P10-2（本批保留接口位）。

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
