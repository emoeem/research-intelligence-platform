//! 激活层依赖检查（PLUGIN-CONTRACT §7 激活层；ROADMAP P6-7）。
//!
//! 三类检查，全部产出机器可读报告（enable 拒绝时错误 details 携带）：
//! 1. **插件依赖解析**：dependencies[] = {id, requirement}，requirement 为
//!    semver 区间（>=、>、<=、<、=、^、~，复用 plugin-registry ConstraintSet）；
//!    缺失或无满足版本即拒绝；
//! 2. **capability 依赖**：capabilities[].requires 指向其他插件的 provides，
//!    条目格式 `{plugin_id}/{capability_id}`；被依赖插件被禁用或已卸载时拒绝
//!    并提示原因（§7 原文语义）；
//! 3. **依赖环检测**：dependencies[] 与 requires 两个边源合成插件级有向图，
//!    DFS 找环；待启用插件参与任一环即拒绝（A→B→A 或 A→A 自依赖）。
//!
//! 被依赖插件状态纪律（dependencies 与 requires 同款）：DISABLED / INVALID /
//! UNINSTALLED / ERROR 态一律阻断，状态值进报告（dep_state / dep_disabled /
//! dep_uninstalled）。

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;
use serde_json::Value;

use crate::RuntimeError;
use database::Pool;
use plugin_registry::{ConstraintSet, Semver};

/// 依赖报告（机器可读；enable 拒绝时进错误 details）。
#[derive(Debug, Clone, Serialize)]
pub struct DependencyReport {
    /// 全部通过。
    pub ok: bool,
    /// 插件依赖解析结果。
    pub plugin_deps: Vec<PluginDepEntry>,
    /// capability requires 解析结果。
    pub capability_deps: Vec<CapabilityDepEntry>,
    /// 依赖环（每环一条路径，首尾同插件）。
    pub cycles: Vec<Vec<String>>,
}

impl DependencyReport {
    /// 单行 JSON。
    pub fn to_json_string(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{\"ok\":false}".to_string())
    }

    /// 是否全部通过。
    pub fn passes(&self) -> bool {
        self.ok
    }
}

/// 插件依赖条目。
#[derive(Debug, Clone, Serialize)]
pub struct PluginDepEntry {
    /// 依赖的插件 id。
    pub id: String,
    /// 声明的 semver 区间。
    pub requirement: String,
    /// 解析状态：satisfied / missing / version_unsatisfied / dep_state。
    pub status: String,
    /// 详情（安装版本、被依赖插件状态等）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

/// capability 依赖条目。
#[derive(Debug, Clone, Serialize)]
pub struct CapabilityDepEntry {
    /// requires 条目（{plugin_id}/{capability_id}）。
    pub require: String,
    /// 解析状态：satisfied / missing / dep_disabled / dep_uninstalled / dep_state。
    pub status: String,
    /// 详情。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

/// 被依赖插件的状态纪律：这些状态阻断依赖方 enable。
const BLOCKING_DEP_STATES: [&str; 4] = ["DISABLED", "INVALID", "UNINSTALLED", "ERROR"];

/// 插件依赖 + capability 依赖 + 环检测（enable 激活层入口）。
///
/// `exclude_id` = 待启用插件自身（图中参与环检测，但依赖解析按其 Manifest）。
pub fn resolve_dependencies(
    pool: &Pool,
    manifest: &Value,
    exclude_id: &str,
) -> Result<DependencyReport, RuntimeError> {
    let store = crate::store::PluginStore::open(pool.clone());
    let rows = store.list().map_err(RuntimeError::from)?;
    // 插件级视图：id → (active version, state)。
    let mut plugins: BTreeMap<String, (String, String)> = BTreeMap::new();
    for p in &rows {
        if p.is_active {
            plugins
                .entry(p.plugin_id.as_str().to_string())
                .or_insert_with(|| (p.version.clone(), p.state.as_str().to_string()));
        }
    }

    let mut report = DependencyReport {
        ok: true,
        plugin_deps: Vec::new(),
        capability_deps: Vec::new(),
        cycles: Vec::new(),
    };

    // 1) 插件依赖解析。
    if let Some(deps) = manifest.get("dependencies").and_then(Value::as_array) {
        for dep in deps {
            let dep_id = dep.get("id").and_then(Value::as_str).unwrap_or_default();
            let requirement = dep
                .get("requirement")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            let entry = resolve_plugin_dep(&plugins, dep_id, &requirement);
            if entry.status != "satisfied" {
                report.ok = false;
            }
            report.plugin_deps.push(entry);
        }
    }

    // 2) capability requires（{plugin_id}/{capability_id}）。
    let caps_by_plugin = capability_index(&rows);
    if let Some(caps) = manifest.get("capabilities").and_then(Value::as_array) {
        for cap in caps {
            let Some(requires) = cap.get("requires").and_then(Value::as_array) else {
                continue;
            };
            for require in requires {
                let require = require.as_str().unwrap_or_default().to_string();
                let entry = resolve_capability_dep(&plugins, &caps_by_plugin, &require);
                if entry.status != "satisfied" {
                    report.ok = false;
                }
                report.capability_deps.push(entry);
            }
        }
    }

    // 3) 依赖环检测（dependencies[] 边 + requires 边，插件级图）。
    let edges = dependency_edges(&rows, exclude_id);
    let cycles = find_cycles(&edges, exclude_id);
    if !cycles.is_empty() {
        report.ok = false;
        report.cycles = cycles;
    }

    Ok(report)
}

/// 解析单条插件依赖。
fn resolve_plugin_dep(
    plugins: &BTreeMap<String, (String, String)>,
    dep_id: &str,
    requirement: &str,
) -> PluginDepEntry {
    let Some((version, state)) = plugins.get(dep_id) else {
        return PluginDepEntry {
            id: dep_id.to_string(),
            requirement: requirement.to_string(),
            status: "missing".to_string(),
            detail: Some("dependency plugin not installed".to_string()),
        };
    };
    if BLOCKING_DEP_STATES.contains(&state.as_str()) {
        return PluginDepEntry {
            id: dep_id.to_string(),
            requirement: requirement.to_string(),
            status: if state == "DISABLED" {
                "dep_disabled".to_string()
            } else if state == "UNINSTALLED" {
                "dep_uninstalled".to_string()
            } else {
                "dep_state".to_string()
            },
            detail: Some(format!("dependency plugin state is {state}")),
        };
    }
    let range = match ConstraintSet::parse(requirement) {
        Ok(r) => r,
        Err(e) => {
            return PluginDepEntry {
                id: dep_id.to_string(),
                requirement: requirement.to_string(),
                status: "requirement_invalid".to_string(),
                detail: Some(e),
            }
        }
    };
    let Some(installed) = Semver::parse(version) else {
        return PluginDepEntry {
            id: dep_id.to_string(),
            requirement: requirement.to_string(),
            status: "version_unparseable".to_string(),
            detail: Some(format!("installed version {version:?} is not semver")),
        };
    };
    if range.matches(installed) {
        PluginDepEntry {
            id: dep_id.to_string(),
            requirement: requirement.to_string(),
            status: "satisfied".to_string(),
            detail: Some(format!("installed {version} satisfies {requirement}")),
        }
    } else {
        PluginDepEntry {
            id: dep_id.to_string(),
            requirement: requirement.to_string(),
            status: "version_unsatisfied".to_string(),
            detail: Some(format!(
                "installed {version} does not satisfy {requirement}"
            )),
        }
    }
}

/// capability 索引：plugin_id → active 版本的 capability_id 集合。
fn capability_index(rows: &[crate::Plugin]) -> BTreeMap<String, BTreeSet<String>> {
    let mut index: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for p in rows {
        if !p.is_active {
            continue;
        }
        let manifest: Value = match serde_json::from_str(&p.manifest_json) {
            Ok(m) => m,
            Err(_) => continue,
        };
        let entry = index.entry(p.plugin_id.as_str().to_string()).or_default();
        if let Some(caps) = manifest.get("capabilities").and_then(Value::as_array) {
            for cap in caps {
                if let Some(id) = cap.get("id").and_then(Value::as_str) {
                    entry.insert(id.to_string());
                }
            }
        }
    }
    index
}

/// 解析单条 capability 依赖（{plugin_id}/{capability_id}）。
fn resolve_capability_dep(
    plugins: &BTreeMap<String, (String, String)>,
    caps_by_plugin: &BTreeMap<String, BTreeSet<String>>,
    require: &str,
) -> CapabilityDepEntry {
    let Some((dep_plugin, dep_cap)) = require.split_once('/') else {
        return CapabilityDepEntry {
            require: require.to_string(),
            status: "require_invalid".to_string(),
            detail: Some("requires entries use {plugin_id}/{capability_id}".to_string()),
        };
    };
    let Some((_, state)) = plugins.get(dep_plugin) else {
        return CapabilityDepEntry {
            require: require.to_string(),
            status: "missing".to_string(),
            detail: Some("providing plugin not installed".to_string()),
        };
    };
    if BLOCKING_DEP_STATES.contains(&state.as_str()) {
        return CapabilityDepEntry {
            require: require.to_string(),
            status: if state == "DISABLED" {
                "dep_disabled".to_string()
            } else if state == "UNINSTALLED" {
                "dep_uninstalled".to_string()
            } else {
                "dep_state".to_string()
            },
            detail: Some(format!(
                "providing plugin state is {state} (capability dependency, §7 item 2)"
            )),
        };
    }
    let has_cap = caps_by_plugin
        .get(dep_plugin)
        .map(|caps| caps.contains(dep_cap))
        .unwrap_or(false);
    if has_cap {
        CapabilityDepEntry {
            require: require.to_string(),
            status: "satisfied".to_string(),
            detail: None,
        }
    } else {
        CapabilityDepEntry {
            require: require.to_string(),
            status: "missing".to_string(),
            detail: Some(format!(
                "capability {dep_cap:?} not found on active version of {dep_plugin:?}"
            )),
        }
    }
}

/// 插件级依赖边：dependencies[]（全版本行取图，含非 active，环是静态事实）。
fn dependency_edges(rows: &[crate::Plugin], exclude_id: &str) -> BTreeMap<String, Vec<String>> {
    let mut edges: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for p in rows {
        let manifest: Value = match serde_json::from_str(&p.manifest_json) {
            Ok(m) => m,
            Err(_) => continue,
        };
        let from = p.plugin_id.as_str().to_string();
        if let Some(deps) = manifest.get("dependencies").and_then(Value::as_array) {
            for dep in deps {
                if let Some(to) = dep.get("id").and_then(Value::as_str) {
                    edges.entry(from.clone()).or_default().push(to.to_string());
                }
            }
        }
        if let Some(caps) = manifest.get("capabilities").and_then(Value::as_array) {
            for cap in caps {
                if let Some(requires) = cap.get("requires").and_then(Value::as_array) {
                    for require in requires {
                        if let Some(require) = require.as_str() {
                            if let Some((dep_plugin, _)) = require.split_once('/') {
                                edges
                                    .entry(from.clone())
                                    .or_default()
                                    .push(dep_plugin.to_string());
                            }
                        }
                    }
                }
            }
        }
    }
    // exclude_id 不作为环起点（其自身环由调用侧按启用的插件判定）。
    if !exclude_id.is_empty() {
        edges.entry(exclude_id.to_string()).or_default();
    }
    edges
}

/// DFS 找包含 `focus` 的环（路径首尾同插件；无则空）。
fn find_cycles(edges: &BTreeMap<String, Vec<String>>, focus: &str) -> Vec<Vec<String>> {
    let mut cycles = Vec::new();
    let mut path: Vec<String> = vec![focus.to_string()];
    let mut on_path: BTreeSet<String> = BTreeSet::from([focus.to_string()]);
    dfs(focus, focus, edges, &mut path, &mut on_path, &mut cycles, 0);
    cycles
}

fn dfs(
    start: &str,
    node: &str,
    edges: &BTreeMap<String, Vec<String>>,
    path: &mut Vec<String>,
    on_path: &mut BTreeSet<String>,
    cycles: &mut Vec<Vec<String>>,
    depth: usize,
) {
    if depth > 64 {
        return; // 深度兜底（图异常时不无限递归）
    }
    if let Some(nexts) = edges.get(node) {
        for next in nexts.clone() {
            if next == start {
                let mut cycle = path.clone();
                cycle.push(start.to_string());
                cycles.push(cycle);
                continue;
            }
            if on_path.contains(&next) {
                continue;
            }
            path.push(next.clone());
            on_path.insert(next.clone());
            dfs(start, &next, edges, path, on_path, cycles, depth + 1);
            path.pop();
            on_path.remove(&next);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requires_entry_format_documented() {
        let plugins = BTreeMap::from([(
            "a.p".to_string(),
            ("0.1.0".to_string(), "ENABLED".to_string()),
        )]);
        let caps = BTreeMap::from([("a.p".to_string(), BTreeSet::from(["k.x".to_string()]))]);
        let ok = resolve_capability_dep(&plugins, &caps, "a.p/k.x");
        assert_eq!(ok.status, "satisfied");
        let bad_format = resolve_capability_dep(&plugins, &caps, "a.p");
        assert_eq!(bad_format.status, "require_invalid");
    }

    #[test]
    fn self_dependency_is_cycle() {
        let edges = BTreeMap::from([("a.p".to_string(), vec!["a.p".to_string()])]);
        let cycles = find_cycles(&edges, "a.p");
        assert_eq!(cycles.len(), 1);
        assert_eq!(cycles[0], vec!["a.p", "a.p"]);
    }
}
