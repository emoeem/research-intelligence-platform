//! plugin.lock 插件级锁定记录（PLUGIN-CONTRACT §6：安装时写 plugin.lock，
//! 记录 resolved version、pinned ref、sha256、许可证、来源 URL、安装时间、
//! 信任级；升级与回滚一律以 lock 为准）。
//!
//! 序列化为 JSON（serde derive，与平台数据面 serde_json 同栈）；落盘位置
//! `installed/<id>/<version>/plugin.lock`（§5.1 INSTALLED 行，P6-3 起由
//! rip-data 布局承载）。`sha256` 字段 = 包内 checksums.sha256 文件字节的
//! sha256：清单覆盖每个内容文件，再对清单文件取哈希，得到可独立复核的
//! 包内容指纹。`installed_at_unix` 用 Unix 秒（RFC3339 需引入时间 crate，
//! P6-1 不为此新增依赖；语义无歧义且机器可比）。

use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

/// plugin.lock 文件名（PLUGIN-CONTRACT §8）。
pub const PLUGIN_LOCK_FILENAME: &str = "plugin.lock";

/// lock 错误（机器可读）。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LockError {
    /// 稳定错误码：read_failed / parse_failed / write_failed。
    pub code: String,
    /// 人类可读原因。
    pub message: String,
}

impl LockError {
    fn new(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.to_string(),
            message: message.into(),
        }
    }
}

impl std::fmt::Display for LockError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "plugin.lock error [{}]: {}", self.code, self.message)
    }
}

impl std::error::Error for LockError {}

/// 信任级四档（PLUGIN-CONTRACT §6，计划 584 行：影响安装确认提示强度与
/// 自动更新默认策略，草案 D10 仅 official 允许自动提示）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TrustLevel {
    /// 官方来源。
    Official,
    /// 社区来源。
    Community,
    /// 本地来源。
    Local,
    /// 远程来源。
    Remote,
}

impl TrustLevel {
    /// 小写稳定词形（与 serde 序列化一致）。
    pub fn as_str(&self) -> &'static str {
        match self {
            TrustLevel::Official => "official",
            TrustLevel::Community => "community",
            TrustLevel::Local => "local",
            TrustLevel::Remote => "remote",
        }
    }
}

/// 版本固定记录（对应 Manifest source.pinned；archive 来源以 sha256 为
/// pin，kind 记 "sha256"）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PinnedRef {
    /// pin 类型：commit / tag / release / sha256。
    pub kind: String,
    /// pin 值（commit sha、tag 名、release 名或归档 sha256）。
    pub value: String,
}

/// plugin.lock 内容（§6 字段全集）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PluginLock {
    /// 插件 id。
    pub plugin_id: String,
    /// 实际安装解析出的版本。
    pub resolved_version: String,
    /// 来源类型（Manifest source.type，五类之一）。
    pub source_type: String,
    /// 来源定位（url / path / endpoint / base_url）。
    pub source_ref: String,
    /// 版本固定记录。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pinned: Option<PinnedRef>,
    /// 包内容指纹：checksums.sha256 文件字节的 sha256。
    pub sha256: String,
    /// 许可证（来自 Manifest license）。
    pub license: String,
    /// 信任级。
    pub trust_level: TrustLevel,
    /// 安装时间（Unix 秒）。
    pub installed_at_unix: i64,
    /// 能力协商快照（mcp_server 握手 / http_api 契约哈希，P6-4；local_dir
    /// 携带 resolved HEAD，git 携带 resolved commit；无协商来源为 None）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub negotiation: Option<serde_json::Value>,
}

impl PluginLock {
    /// 单行 JSON（机器可读）。
    pub fn to_json_string(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{\"broken\":true}".to_string())
    }
}

/// 写 plugin.lock 到 `root/plugin.lock`。
pub fn write_lock(root: &Path, lock: &PluginLock) -> Result<(), LockError> {
    let body = serde_json::to_string_pretty(lock)
        .map_err(|e| LockError::new("write_failed", format!("serialize: {e}")))?;
    let target = root.join(PLUGIN_LOCK_FILENAME);
    fs::write(&target, body)
        .map_err(|e| LockError::new("write_failed", format!("write {:?}: {e}", target.display())))
}

/// 从 `root/plugin.lock` 读取。
pub fn read_lock(root: &Path) -> Result<PluginLock, LockError> {
    let target = root.join(PLUGIN_LOCK_FILENAME);
    let body = fs::read_to_string(&target)
        .map_err(|e| LockError::new("read_failed", format!("read {:?}: {e}", target.display())))?;
    serde_json::from_str(&body)
        .map_err(|e| LockError::new("parse_failed", format!("parse {:?}: {e}", target.display())))
}

/// 当前 Unix 秒（安装时间戳）。
pub fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> PluginLock {
        PluginLock {
            plugin_id: "knowledge.rtis-redteam".to_string(),
            resolved_version: "0.1.0".to_string(),
            source_type: "local_dir".to_string(),
            source_ref: "integrations/rtis".to_string(),
            pinned: Some(PinnedRef {
                kind: "commit".to_string(),
                value: "022a717".to_string(),
            }),
            sha256: "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855".to_string(),
            license: "UNLICENSED".to_string(),
            trust_level: TrustLevel::Local,
            installed_at_unix: 1_760_000_000,
            negotiation: None,
        }
    }

    #[test]
    fn json_roundtrip_keeps_all_fields() {
        let lock = sample();
        let json = lock.to_json_string();
        let parsed: PluginLock = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, lock);
        assert!(json.contains("\"trust_level\":\"local\""));
        assert!(json.contains("\"pinned\""));
    }

    #[test]
    fn trust_level_vocabulary() {
        for (level, name) in [
            (TrustLevel::Official, "official"),
            (TrustLevel::Community, "community"),
            (TrustLevel::Local, "local"),
            (TrustLevel::Remote, "remote"),
        ] {
            assert_eq!(level.as_str(), name);
            let json = serde_json::to_string(&level).unwrap();
            assert_eq!(json, format!("\"{name}\""));
        }
    }

    #[test]
    fn write_and_read_lock_file() {
        let dir = std::env::temp_dir().join(format!("p6-lock-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let lock = sample();
        write_lock(&dir, &lock).unwrap();
        assert_eq!(read_lock(&dir).unwrap(), lock);
        let _ = fs::remove_dir_all(&dir);
    }
}
