//! Phase 6 插件注册表（静态面，ROADMAP P6-1；协议基准
//! docs/phase0/PLUGIN-CONTRACT-DRAFT.md）。
//!
//! 职责：
//! - [`manifest`]：Manifest strict 解析（未知字段拒绝由 v1.json 表达，D4），
//!   JSON / YAML 双通道自动探测；
//! - [`validate`]：PLUGIN-CONTRACT §4.2 校验顺序七条，机器可读
//!   [`ValidationReport`]；校验器复用 tool-registry 的
//!   [`tool_registry::jsonschema_lite`]（draft 2020-12）；
//! - [`checksums`]：包内容 checksums.sha256 写入与 verify（fail-closed）；
//! - [`lock`]：plugin.lock（resolved version、pinned ref、sha256、许可证、
//!   来源 URL、安装时间、信任级，§6）；
//! - [`StaticChain`]：DISCOVERED → VALIDATED → INVALID / INSTALLED 静态链
//!   门面（§5.1 主链前三态；REGISTERED 起的运行链归 plugin-runtime，P6-2）。
//!
//! 依赖方向（MODULE_BOUNDARIES §8 纪律）：serde + serde_json +
//! serde_yaml_ng（仓库首个 yaml 依赖，维护中 fork）+ tool-registry（既有
//! 校验器与 sha256 封装复用），不依赖任何 engine crate，不触数据库。

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod catalog;
pub mod checksums;
pub mod lock;
pub mod manifest;
pub mod source;
pub mod trust_policy;
pub mod validate;

pub use checksums::{
    verify_checksums, write_checksums, ChecksumError, ChecksumIssue, ChecksumManifest,
    CHECKSUMS_FILENAME,
};
pub use lock::{
    now_unix, read_lock, write_lock, LockError, PinnedRef, PluginLock, TrustLevel,
    PLUGIN_LOCK_FILENAME,
};
pub use manifest::{
    parse_document, v1_schema, v1_schema_json, ParseError, SUPPORTED_SCHEMA_VERSION,
};
pub use source::{
    default_trust as source_default_trust, fetch as source_fetch, verify as source_verify,
    FetchOutcome, Source, SourceError,
};
// P10-2：信任级策略面（§6 确认强度分级 + D10 自动更新默认策略）单一事实来源。
pub use trust_policy::{
    auto_update_prompt_allowed, install_confirmation, policy_json as trust_policy_json,
    InstallConfirmation,
};
// P10-2：插件目录发布形态（ROADMAP P10-1「目录发布形态」落地）。
pub use catalog::{
    build_catalog, read_and_verify_catalog, write_catalog, CatalogEntry, CatalogError,
    PluginCatalog,
};
// sha256 复用面（来源校验测试与外部对账用；实现为 tool-registry 既有封装）。
pub use tool_registry::hash;
pub use validate::{
    render_report_text, validate, validate_document, Constraint, ConstraintSet, Options, Semver,
    ValidationCheck, ValidationIssue, ValidationReport,
};

use std::collections::{BTreeMap, HashSet};
use std::path::Path;

use serde::Serialize;
use serde_json::Value;

/// 插件生命周期状态（PLUGIN-CONTRACT §5.1：主链七态加辅助五态，草案 D2）。
///
/// P6-1 静态面只驱动 DISCOVERED / VALIDATED / INVALID / INSTALLED 四态；
/// REGISTERED 到 ACTIVE 主链与 DISABLED / ERROR / UNINSTALLED 语义归
/// plugin-runtime（P6-2），本枚举先行冻结全量词形避免二次迁移。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PluginState {
    /// 来源扫描发现，Manifest 元数据已解析进 registry 索引。
    Discovered,
    /// §4.2 全部静态检查通过。
    Validated,
    /// 内容落盘 installed/，校验和与 lock 写入。
    Installed,
    /// capabilities 登记进平台数据面（P6-2）。
    Registered,
    /// 允许被 Context Broker 匹配（P6-2）。
    Enabled,
    /// 运行资源已加载（P6-2）。
    Loaded,
    /// 至少一个 capability 正在服务调用（P6-2）。
    Active,
    /// 静态校验失败。
    Invalid,
    /// 用户禁用，数据全保留（P6-2）。
    Disabled,
    /// 远端存在兼容新版本（叠加标记，P6-2）。
    UpdatePending,
    /// 健康检查失败或运行期故障（P6-2）。
    Error,
    /// 终态：内容删除（P6-2）。
    Uninstalled,
}

// serde 词形：SCREAMING_SNAKE_CASE 与 PLUGIN-CONTRACT §5.1 表原文一致
// （DISCOVERED、UPDATE_PENDING）；运行时字串统一走 [`state_name`]。

/// 静态链动作（§5.2 跃迁表在 P6-1 范围内的子集）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StaticAction {
    /// 来源扫描发现（discover 行）。
    Discover,
    /// 静态校验通过（validate 行）。
    Validate,
    /// 静态校验拒绝，进 INVALID（validate 行失败分支）。
    ValidateReject,
    /// 内容落盘，校验和与 lock 写入（install 行）。
    Install,
}

impl StaticAction {
    /// snake_case 稳定词形（transitions.jsonl 的 action 字段）。
    pub fn as_str(&self) -> &'static str {
        match self {
            StaticAction::Discover => "discover",
            StaticAction::Validate => "validate",
            StaticAction::ValidateReject => "validate_reject",
            StaticAction::Install => "install",
        }
    }
}

/// 链级错误（机器可读：code + message + 可选 details）。
#[derive(Debug, Clone, Serialize)]
pub struct MachineError {
    /// 稳定错误码。
    pub code: String,
    /// 人类可读原因。
    pub message: String,
    /// 结构化明细（如校验和失败清单）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<Value>,
}

impl MachineError {
    fn new(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.to_string(),
            message: message.into(),
            details: None,
        }
    }

    /// 单行 JSON（机器可读报告）。
    pub fn to_json_string(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{\"code\":\"unserializable\"}".to_string())
    }
}

impl std::fmt::Display for MachineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "plugin chain error [{}]: {}", self.code, self.message)
    }
}

impl std::error::Error for MachineError {}

impl From<ParseError> for MachineError {
    fn from(e: ParseError) -> Self {
        MachineError::new(&e.code, e.message)
    }
}

impl From<ChecksumError> for MachineError {
    fn from(e: ChecksumError) -> Self {
        MachineError::new(&e.code, e.message)
    }
}

impl From<LockError> for MachineError {
    fn from(e: LockError) -> Self {
        MachineError::new(&e.code, e.message)
    }
}

impl From<source::SourceError> for MachineError {
    fn from(e: source::SourceError) -> Self {
        let mut err = MachineError::new(&e.code, e.message);
        err.details = e.details;
        err
    }
}

/// 一次状态跃迁的结构化记录（§8 state/transitions.jsonl 行形状，落盘归
/// P6-3，本结构先行冻结字段）。
#[derive(Debug, Clone, Serialize)]
pub struct TransitionRecord {
    /// 流水号（从 1 起）。
    pub seq: u64,
    /// 插件 id。
    pub plugin_id: String,
    /// 动作。
    pub action: String,
    /// 跃迁前状态（discover 无前状态）。
    pub from: Option<String>,
    /// 跃迁后状态。
    pub to: String,
    /// 时间戳（Unix 秒）。
    pub at_unix: i64,
}

/// 静态链中的插件条目：状态与 discover 时解析的 Manifest。
#[derive(Debug, Clone)]
struct PluginEntry {
    state: PluginState,
    manifest: Value,
}

/// DISCOVERED → VALIDATED → INVALID / INSTALLED 静态链门面。
///
/// 数据保存在内存索引（§8 registry-index 的进程内形态；文件承载归 P6-3），
/// 全部动作产出机器可读结果：validate 返回 [`ValidationReport`]，install
/// 返回 [`PluginLock`]，错误统一 [`MachineError`]，跃迁流水经
/// [`StaticChain::records_jsonl`] 导出。
#[derive(Debug, Clone)]
pub struct StaticChain {
    options: Options,
    plugins: BTreeMap<String, PluginEntry>,
    records: Vec<TransitionRecord>,
}

impl Default for StaticChain {
    fn default() -> Self {
        Self::new()
    }
}

impl StaticChain {
    /// 以默认 [`Options`]（平台版本 0.2.0）打开静态链。
    pub fn new() -> Self {
        Self {
            options: Options::default(),
            plugins: BTreeMap::new(),
            records: Vec::new(),
        }
    }

    /// 覆盖校验配置（平台版本自定义用，如未来 0.3.0 开发分支）。
    pub fn with_options(mut self, options: Options) -> Self {
        self.options = options;
        self
    }

    /// 当前状态视图。
    pub fn state_of(&self, plugin_id: &str) -> Option<PluginState> {
        self.plugins.get(plugin_id).map(|e| e.state)
    }

    /// 已解析的 Manifest 视图。
    pub fn manifest_of(&self, plugin_id: &str) -> Option<&Value> {
        self.plugins.get(plugin_id).map(|e| &e.manifest)
    }

    /// 已知插件 id 集（validate 的 id 唯一性输入）。
    pub fn known_ids(&self) -> Vec<String> {
        self.plugins.keys().cloned().collect()
    }

    /// 跃迁流水（结构化）。
    pub fn records(&self) -> &[TransitionRecord] {
        &self.records
    }

    /// 跃迁流水 JSONL 导出（每行一个 JSON 对象，§8 transitions.jsonl 形状）。
    pub fn records_jsonl(&self) -> String {
        let mut out = String::new();
        for record in &self.records {
            let line =
                serde_json::to_string(record).unwrap_or_else(|_| "{\"broken\":true}".to_string());
            out.push_str(&line);
            out.push('\n');
        }
        out
    }

    fn record(
        &mut self,
        plugin_id: &str,
        action: StaticAction,
        from: Option<PluginState>,
        to: PluginState,
    ) {
        let seq = (self.records.len() as u64) + 1;
        self.records.push(TransitionRecord {
            seq,
            plugin_id: plugin_id.to_string(),
            action: action.as_str().to_string(),
            from: from.map(state_name).map(str::to_string),
            to: state_name(to).to_string(),
            at_unix: now_unix(),
        });
    }

    /// discover：解析 Manifest 文本（JSON / YAML 自动探测），元数据进索引，
    /// 状态置 DISCOVERED。id 冲突与解析失败即拒绝（§5.2 discover 行），
    /// 不产生状态跃迁记录。
    pub fn discover(&mut self, doc: &str) -> Result<String, MachineError> {
        let manifest = parse_document(doc)?;
        let obj = manifest
            .as_object()
            .ok_or_else(|| MachineError::new("not_object", "manifest must be a JSON object"))?;
        let id = obj
            .get("id")
            .and_then(Value::as_str)
            .ok_or_else(|| MachineError::new("id_invalid", "`id` must be a non-empty string"))?
            .to_string();
        if self.plugins.contains_key(&id) {
            return Err(MachineError::new(
                "id_conflict",
                format!("plugin id {id:?} already discovered (global uniqueness, §5.2 discover)"),
            ));
        }
        self.plugins.insert(
            id.clone(),
            PluginEntry {
                state: PluginState::Discovered,
                manifest,
            },
        );
        self.record(&id, StaticAction::Discover, None, PluginState::Discovered);
        Ok(id)
    }

    /// validate：按 §4.2 七条对已 DISCOVERED 的插件执行静态校验。通过则
    /// 进 VALIDATED，失败进 INVALID（报告机器可读，进 Plugin Center）。
    /// 仅 DISCOVERED 态可执行（重跑须重新 discover，§5.2）。
    pub fn validate(
        &mut self,
        plugin_id: &str,
        package_root: Option<&Path>,
    ) -> Result<ValidationReport, MachineError> {
        let entry = self.plugins.get(plugin_id).ok_or_else(|| {
            MachineError::new(
                "unknown_plugin",
                format!("plugin {plugin_id:?} not discovered"),
            )
        })?;
        if entry.state != PluginState::Discovered {
            return Err(MachineError::new(
                "illegal_transition",
                format!(
                    "validate requires state DISCOVERED, plugin {plugin_id:?} is {}",
                    state_name(entry.state)
                ),
            ));
        }
        let manifest = entry.manifest.clone();
        let existing_ids: HashSet<String> = self
            .plugins
            .keys()
            .filter(|k| k.as_str() != plugin_id)
            .cloned()
            .collect();
        let report = validate::validate(&manifest, package_root, &existing_ids, &self.options);
        if report.ok {
            self.record(
                plugin_id,
                StaticAction::Validate,
                Some(PluginState::Discovered),
                PluginState::Validated,
            );
            if let Some(entry) = self.plugins.get_mut(plugin_id) {
                entry.state = PluginState::Validated;
            }
        } else {
            self.record(
                plugin_id,
                StaticAction::ValidateReject,
                Some(PluginState::Discovered),
                PluginState::Invalid,
            );
            if let Some(entry) = self.plugins.get_mut(plugin_id) {
                entry.state = PluginState::Invalid;
            }
        }
        Ok(report)
    }

    /// install：VALIDATED 态插件内容落盘（写/验 checksums.sha256 与
    /// plugin.lock），进 INSTALLED（§5.2 install 行）。校验和失败进
    /// INVALID？不：校验和失败属安装期失败，按 §5.2「校验和失败进
    /// INVALID」处理，返回错误并将状态置 INVALID。
    pub fn install(
        &mut self,
        plugin_id: &str,
        package_root: &Path,
        trust_level: TrustLevel,
    ) -> Result<PluginLock, MachineError> {
        let entry = self.plugins.get(plugin_id).ok_or_else(|| {
            MachineError::new(
                "unknown_plugin",
                format!("plugin {plugin_id:?} not discovered"),
            )
        })?;
        if entry.state != PluginState::Validated {
            return Err(MachineError::new(
                "illegal_transition",
                format!(
                    "install requires state VALIDATED, plugin {plugin_id:?} is {}",
                    state_name(entry.state)
                ),
            ));
        }
        let manifest = entry.manifest.clone();
        let obj = manifest.as_object().ok_or_else(|| {
            MachineError::new("not_object", "stored manifest must be a JSON object")
        })?;

        // 校验和：包内已有清单则 verify（fail-closed），没有则先写入。
        let checksum_manifest = if package_root.join(CHECKSUMS_FILENAME).is_file() {
            verify_checksums(package_root).map_err(|(e, issues)| {
                let mut err = MachineError::new(
                    "checksum_verification_failed",
                    format!("plugin {plugin_id:?}: {e}"),
                );
                err.details = Some(serde_json::to_value(&issues).unwrap_or(Value::Null));
                // §5.2 install 失败处理：校验和失败进 INVALID。
                self.mark_invalid(plugin_id);
                err
            })?
        } else {
            write_checksums(package_root).map_err(|e| {
                MachineError::new(
                    "checksum_write_failed",
                    format!("plugin {plugin_id:?}: {e}"),
                )
            })?
        };

        // lock 字段（§6）：resolved version、pinned ref、sha256、许可证、
        // 来源 URL、安装时间、信任级。
        let source = obj
            .get("source")
            .and_then(Value::as_object)
            .ok_or_else(|| {
                MachineError::new("source_incomplete", "stored manifest has no source object")
            })?;
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
        let pinned = source
            .get("pinned")
            .and_then(Value::as_object)
            .map(|p| PinnedRef {
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
                        // archive 来源以 sha256 为 pin（§6）。
                        source
                            .get("sha256")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_string()
                    }),
            });
        // 包内协商快照（mcp_server 握手 / http_api 契约 / local_dir·git 的
        // resolved ref，P6-4）随 lock 落盘（§6 能力协商记录进 lock）。
        let negotiation = std::fs::read(package_root.join("negotiation.json"))
            .ok()
            .and_then(|b| serde_json::from_str(&String::from_utf8_lossy(&b)).ok());
        let lock = PluginLock {
            plugin_id: plugin_id.to_string(),
            resolved_version: obj
                .get("version")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            source_type,
            source_ref,
            pinned,
            sha256: checksum_manifest.file_sha256,
            license: obj
                .get("license")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            trust_level,
            installed_at_unix: now_unix(),
            negotiation,
        };
        write_lock(package_root, &lock).map_err(|e| {
            MachineError::new("lock_write_failed", format!("plugin {plugin_id:?}: {e}"))
        })?;

        self.record(
            plugin_id,
            StaticAction::Install,
            Some(PluginState::Validated),
            PluginState::Installed,
        );
        if let Some(entry) = self.plugins.get_mut(plugin_id) {
            entry.state = PluginState::Installed;
        }
        Ok(lock)
    }

    /// fetch_reject：来源拉取失败（archive 哈希不匹配、git clone 失败等）
    /// 将 DISCOVERED / VALIDATED 态插件置 INVALID（§5.2 install 失败处理：
    /// 校验和失败进 INVALID 同语义），跃迁流水记 install 动作落到 INVALID。
    pub fn fetch_reject(&mut self, plugin_id: &str) -> Result<(), MachineError> {
        let entry = self.plugins.get(plugin_id).ok_or_else(|| {
            MachineError::new(
                "unknown_plugin",
                format!("plugin {plugin_id:?} not discovered"),
            )
        })?;
        let from = entry.state;
        if !matches!(from, PluginState::Discovered | PluginState::Validated) {
            return Err(MachineError::new(
                "illegal_transition",
                format!(
                    "fetch_reject requires DISCOVERED or VALIDATED, plugin {plugin_id:?} is {}",
                    state_name(from)
                ),
            ));
        }
        self.record(
            plugin_id,
            StaticAction::Install,
            Some(from),
            PluginState::Invalid,
        );
        if let Some(entry) = self.plugins.get_mut(plugin_id) {
            entry.state = PluginState::Invalid;
        }
        Ok(())
    }

    fn mark_invalid(&mut self, plugin_id: &str) {
        self.record(
            plugin_id,
            StaticAction::ValidateReject,
            Some(PluginState::Validated),
            PluginState::Invalid,
        );
        if let Some(entry) = self.plugins.get_mut(plugin_id) {
            entry.state = PluginState::Invalid;
        }
    }
}

/// 状态的稳定词形（错误消息与记录用）。
pub fn state_name(state: PluginState) -> &'static str {
    match state {
        PluginState::Discovered => "DISCOVERED",
        PluginState::Validated => "VALIDATED",
        PluginState::Installed => "INSTALLED",
        PluginState::Registered => "REGISTERED",
        PluginState::Enabled => "ENABLED",
        PluginState::Loaded => "LOADED",
        PluginState::Active => "ACTIVE",
        PluginState::Invalid => "INVALID",
        PluginState::Disabled => "DISABLED",
        PluginState::UpdatePending => "UPDATE_PENDING",
        PluginState::Error => "ERROR",
        PluginState::Uninstalled => "UNINSTALLED",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn minimal_doc(id: &str) -> String {
        format!(
            r#"{{
                "schema_version": "1",
                "id": "{id}",
                "name": "Minimal",
                "version": "0.1.0",
                "kind": "knowledge",
                "description": "最小静态链样例",
                "license": "MIT",
                "source": {{"type": "local_dir", "path": "p"}},
                "compatibility": {{"rip_core": ">=0.2.0"}},
                "capabilities": [
                    {{"id": "k.m", "type": "knowledge_source", "entry": "e.yaml", "summary": "s"}}
                ],
                "context": {{"summary": "s", "docs": "d.md"}}
            }}"#
        )
    }

    #[test]
    fn discover_validate_install_full_static_chain() {
        let tmp = std::env::temp_dir().join(format!("p6-chain-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        std::fs::write(tmp.join("e.yaml"), "kind: knowledge_source\n").unwrap();

        let mut chain = StaticChain::new();
        let id = chain.discover(&minimal_doc("k.min")).unwrap();
        assert_eq!(chain.state_of(&id), Some(PluginState::Discovered));

        let report = chain.validate(&id, Some(&tmp)).unwrap();
        assert!(report.ok, "{}", render_report_text(&report));
        assert_eq!(chain.state_of(&id), Some(PluginState::Validated));

        let lock = chain.install(&id, &tmp, TrustLevel::Local).unwrap();
        assert_eq!(lock.plugin_id, "k.min");
        assert_eq!(lock.sha256.len(), 64);
        assert_eq!(lock.trust_level, TrustLevel::Local);
        assert_eq!(chain.state_of(&id), Some(PluginState::Installed));

        // 落盘物在位：checksums.sha256 与 plugin.lock。
        assert!(tmp.join(CHECKSUMS_FILENAME).is_file());
        assert_eq!(read_lock(&tmp).unwrap(), lock);

        // 跃迁流水 JSONL：discover -> validate -> install 三行。
        let jsonl = chain.records_jsonl();
        let lines: Vec<&str> = jsonl.lines().collect();
        assert_eq!(lines.len(), 3);
        assert!(lines[0].contains("\"action\":\"discover\""));
        assert!(lines[0].contains("\"to\":\"DISCOVERED\""));
        assert!(lines[1].contains("\"action\":\"validate\""));
        assert!(lines[2].contains("\"action\":\"install\""));
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn discover_id_conflict_rejected() {
        let mut chain = StaticChain::new();
        chain.discover(&minimal_doc("k.dup")).unwrap();
        let err = chain.discover(&minimal_doc("k.dup")).unwrap_err();
        assert_eq!(err.code, "id_conflict");
    }

    #[test]
    fn validate_failure_moves_to_invalid() {
        let doc = minimal_doc("k.bad").replacen(
            "\"version\": \"0.1.0\"",
            "\"version\": \"not-semver\"",
            1,
        );
        let mut chain = StaticChain::new();
        let id = chain.discover(&doc).unwrap();
        let report = chain.validate(&id, None).unwrap();
        assert!(!report.ok);
        assert_eq!(chain.state_of(&id), Some(PluginState::Invalid));
        // 非法跃迁：INVALID 不能直接 install。
        let tmp = std::env::temp_dir().join(format!("p6-chain2-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        let err = chain.install(&id, &tmp, TrustLevel::Local).unwrap_err();
        assert_eq!(err.code, "illegal_transition");
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn install_requires_validated_state() {
        let tmp = std::env::temp_dir().join(format!("p6-chain3-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        let mut chain = StaticChain::new();
        let id = chain.discover(&minimal_doc("k.early")).unwrap();
        let err = chain.install(&id, &tmp, TrustLevel::Local).unwrap_err();
        assert_eq!(err.code, "illegal_transition");
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
