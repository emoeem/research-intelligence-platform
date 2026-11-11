//! 数据保留规则（PLUGIN-CONTRACT §5.4 表：类别 × 禁用/卸载/升级处置）。
//!
//! 口径（计划 415 行原文语义）：
//! - 禁用（disable）不删除数据：installed 内容、已入主库知识、证据、Run、
//!   审计记录全部保留（[`disable_report`] 全 Retain）；
//! - 卸载（uninstall）删除插件本体与私有数据，不默认删除已进入主知识库的
//!   知识（保留 plugin_id 溯源并标记「来源已卸载」，孤儿来源在 Plugin Center
//!   可查）；persistent 声明的持久数据默认一并删除但列出明细要求显式确认；
//! - 升级（upgrade）新旧版本目录并存，旧版本保留至回滚窗口结束（草案 D6，
//!   窗口清理属 P10-2），Run/Artifact/Evidence/审计一律保留（计划 122 行）。
//!
//! 执行边界（P6-2）：物理删除仅对位于 data_root 之下的目录执行；外部目录
//! （如调用方以仓库目录作包体验装时）只出报告不执行，防止误删源目录；
//! rip-data/plugins 布局由 P6-3 接管后，installed/<id>/<version>/ 天然在
//! data_root 之下。

use std::path::{Path, PathBuf};

use serde_json::{json, Value};

/// 处置动作。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RetentionAction {
    /// 保留（禁用与卸载的默认面）。
    Retain,
    /// 删除（可执行时已执行）。
    Delete,
    /// 需显式确认后才可执行（persistent 明细，§5.4 卸载列）。
    RequiresConfirmation,
}

/// 保留处置条目。
#[derive(Debug, Clone, serde::Serialize)]
pub struct RetentionItem {
    /// §5.4 数据类别（plugin_body / ingested_knowledge / evidence_artifacts /
    /// runs_audit / plugin_private_data / download_cache）。
    pub category: &'static str,
    /// 处置动作。
    pub action: RetentionAction,
    /// 本批是否已实际执行（报告与执行分离；RequiresConfirmation 恒 false）。
    pub executed: bool,
    /// 物理目标（仅 plugin_body / plugin_private_data 的目录删除类条目）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<PathBuf>,
    /// 说明。
    pub detail: String,
}

/// 保留处置报告（机器可读，进 Plugin Center 与审计 detail）。
#[derive(Debug, Clone, serde::Serialize)]
pub struct RetentionReport {
    /// 插件 id。
    pub plugin_id: String,
    /// 版本。
    pub version: String,
    /// 操作（disable / uninstall / upgrade）。
    pub op: &'static str,
    /// 处置条目。
    pub items: Vec<RetentionItem>,
}

impl RetentionReport {
    /// 单行 JSON。
    pub fn to_json_string(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{\"items\":[]}".to_string())
    }
}

/// target 是否位于 data_root 之下（均在绝对路径域内比较，防前缀误配）。
fn under_root(target: &Path, root: &Path) -> bool {
    match (std::fs::canonicalize(target), std::fs::canonicalize(root)) {
        (Ok(t), Ok(r)) => t.starts_with(&r),
        _ => false,
    }
}

/// 从 Manifest data_retention 块读取 persistent 明细名称。
fn persistent_names(manifest: &Value) -> Vec<String> {
    manifest
        .get("data_retention")
        .and_then(Value::as_object)
        .and_then(|d| d.get("persistent"))
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(|p| p.get("name").and_then(Value::as_str).map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// 禁用处置报告（§5.4 禁用列：全保留）。
pub fn disable_report(plugin_id: &str, version: &str) -> RetentionReport {
    let items = [
        (
            "plugin_body",
            "installed 内容保留（禁用不删除数据，计划 415 行）",
        ),
        ("ingested_knowledge", "已入主库知识保留"),
        ("evidence_artifacts", "Evidence 与 Artifact 保留"),
        ("runs_audit", "Run 与审计事件保留"),
        ("plugin_private_data", "插件私有数据保留"),
        ("download_cache", "下载与构建缓存保留"),
    ]
    .into_iter()
    .map(|(category, detail)| RetentionItem {
        category,
        action: RetentionAction::Retain,
        executed: false,
        target: None,
        detail: detail.to_string(),
    })
    .collect();
    RetentionReport {
        plugin_id: plugin_id.to_string(),
        version: version.to_string(),
        op: "disable",
        items,
    }
}

/// 卸载处置报告（§5.4 卸载列；物理删除仅对 data_root 之下执行）。
pub fn uninstall_report(
    plugin_id: &str,
    version: &str,
    manifest: &Value,
    installed_dir: Option<&Path>,
    data_root: Option<&Path>,
) -> RetentionReport {
    let mut items = Vec::new();

    // 1. 插件本体（installed/<id>/<version>/）。
    match (installed_dir, data_root) {
        (Some(dir), Some(root)) if under_root(dir, root) => {
            items.push(RetentionItem {
                category: "plugin_body",
                action: RetentionAction::Delete,
                executed: true,
                target: Some(dir.to_path_buf()),
                detail: format!("删除安装目录 {:?}（位于 data_root 之下）", dir.display()),
            });
        }
        (Some(dir), _) => {
            items.push(RetentionItem {
                category: "plugin_body",
                action: RetentionAction::Delete,
                executed: false,
                target: None,
                detail: format!(
                    "安装目录 {:?} 不在 data_root 之下，本批不执行物理删除（防误删调用方目录；P6-3 布局接管后按 installed/<id> 删除）",
                    dir.display()
                ),
            });
        }
        (None, _) => {
            items.push(RetentionItem {
                category: "plugin_body",
                action: RetentionAction::Retain,
                executed: false,
                target: None,
                detail: "无安装目录记录（P6-3 布局接管前可空）".to_string(),
            });
        }
    }

    // 2. 已入主库知识（计划 415 行原文语义）。
    let policy = manifest
        .get("data_retention")
        .and_then(Value::as_object)
        .and_then(|d| d.get("ingested_policy"))
        .and_then(Value::as_str)
        .unwrap_or("retain_with_provenance");
    items.push(RetentionItem {
        category: "ingested_knowledge",
        action: if policy == "delete_on_uninstall" {
            RetentionAction::Delete
        } else {
            RetentionAction::Retain
        },
        executed: false,
        target: None,
        detail: if policy == "delete_on_uninstall" {
            "策略声明 delete_on_uninstall：知识行删除属摄取管道（P6-5）执行面".to_string()
        } else {
            "保留带 plugin_id 溯源，标记来源已卸载（孤儿来源清单可查）".to_string()
        },
    });

    // 3/4. 统一证据面与 Run/审计（计划 122 行：一律保留）。
    for (category, detail) in [
        ("evidence_artifacts", "Evidence 与 Artifact 保留"),
        ("runs_audit", "Run 与审计事件保留"),
    ] {
        items.push(RetentionItem {
            category,
            action: RetentionAction::Retain,
            executed: false,
            target: None,
            detail: detail.to_string(),
        });
    }

    // 5. 插件私有数据（persistent 明细要求显式确认）。
    let persistent = persistent_names(manifest);
    for name in &persistent {
        items.push(RetentionItem {
            category: "plugin_private_data",
            action: RetentionAction::RequiresConfirmation,
            executed: false,
            target: None,
            detail: format!("persistent 声明 {name:?}：卸载前列明细要求显式确认（§5.4）"),
        });
    }
    if let (Some(dir), Some(root)) = (installed_dir, data_root) {
        let data_dir = dir.join("data");
        if data_dir.is_dir() && under_root(&data_dir, root) {
            items.push(RetentionItem {
                category: "plugin_private_data",
                action: RetentionAction::Delete,
                executed: true,
                target: Some(data_dir.clone()),
                detail: format!(
                    "删除插件私有数据目录 {:?}（未声明 persistent）",
                    data_dir.display()
                ),
            });
        }
    }

    // 6. 下载与构建缓存（cache/ 布局归 P6-3）。
    items.push(RetentionItem {
        category: "download_cache",
        action: RetentionAction::Delete,
        executed: false,
        target: None,
        detail: "cache/ 目录布局由 P6-3 承载，本批只出报告".to_string(),
    });

    RetentionReport {
        plugin_id: plugin_id.to_string(),
        version: version.to_string(),
        op: "uninstall",
        items,
    }
}

/// 升级处置报告（§5.4 升级列：新旧并存 + 保留面全留）。
pub fn upgrade_report(plugin_id: &str, from_version: &str, to_version: &str) -> RetentionReport {
    let items = vec![
        RetentionItem {
            category: "plugin_body",
            action: RetentionAction::Retain,
            executed: false,
            target: None,
            detail: format!(
                "新旧版本目录并存：旧版本 {from_version} 保留至回滚窗口结束（草案 D6，窗口清理属 P10-2）"
            ),
        },
        RetentionItem {
            category: "ingested_knowledge",
            action: RetentionAction::Retain,
            executed: false,
            target: None,
            detail: "已入主库知识保留（升级时）".to_string(),
        },
        RetentionItem {
            category: "evidence_artifacts",
            action: RetentionAction::Retain,
            executed: false,
            target: None,
            detail: "Evidence 与 Artifact 保留".to_string(),
        },
        RetentionItem {
            category: "runs_audit",
            action: RetentionAction::Retain,
            executed: false,
            target: None,
            detail: "Run 与审计事件保留".to_string(),
        },
        RetentionItem {
            category: "plugin_private_data",
            action: RetentionAction::Retain,
            executed: false,
            target: None,
            detail: "私有数据保留，迁移钩子执行属 P10-2（data_retention 生命周期钩子面）".to_string(),
        },
    ];
    RetentionReport {
        plugin_id: plugin_id.to_string(),
        version: to_version.to_string(),
        op: "upgrade",
        items,
    }
}

/// JSON 视图便捷构造（审计 detail 用）。
pub fn report_json(report: &RetentionReport) -> Value {
    json!({
        "plugin_id": report.plugin_id,
        "version": report.version,
        "op": report.op,
        "items": report.items.iter().map(|i| json!({
            "category": i.category,
            "action": i.action,
            "executed": i.executed,
            "detail": i.detail,
        })).collect::<Vec<_>>(),
    })
}
