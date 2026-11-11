//! 五类来源接入（PLUGIN-CONTRACT §6；ROADMAP P6-4）。
//!
//! 每类来源实现 [`Source`] trait：`verify` 做取包前校验（版本固定语义，
//! fail-fast 拒绝在触源之前），`fetch` 把来源内容拉取落位到安装目录并产出
//! 来源专属工件（negotiation.json / 契约快照）。信任级四档词表以
//! [`crate::lock::TrustLevel`] 为唯一权威（official/community/local/remote，
//! 计划 584 行），本模块提供来源缺省档位（P10-1 可被注册表元数据覆盖）。
//!
//! 网络边界（如实声明）：git 走系统 git 命令（file:// 与网络 URL 皆可）；
//! mcp_server 支持启动命令形态（stdio JSON-RPC 2.0 握手）；http_api 用
//! std TcpStream 最小 GET，首批仅 http://（TLS 归后续批次）；archive 支持
//! 本地路径与 file:// URL，远程 HTTP 下载归 P10（本批测试全部闭环在本地）。
//! mcp 握手与 http 契约的超时强控归 P6-7 运行层（本批 fixture 均快速退出）。

pub mod archive;
pub mod git;
pub mod http_api;
pub mod local_dir;
pub mod mcp_server;

use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::Value;

pub use crate::lock::TrustLevel;

/// 来源拉取结果：安装目录内容已就位 + 来源专属协商快照（进 lock）。
#[derive(Debug, Clone)]
pub struct FetchOutcome {
    /// 拉取落位的安装目录。
    pub target: PathBuf,
    /// 能力协商快照（mcp_server 握手、http_api 契约哈希；local_dir/git/archive 无）。
    pub negotiation: Option<Value>,
}

/// 来源错误（机器可读：code + message + 可选 details）。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SourceError {
    /// 稳定错误码。
    pub code: String,
    /// 人类可读原因。
    pub message: String,
    /// 结构化明细。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<Value>,
}

impl SourceError {
    /// 构造。
    pub fn new(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.to_string(),
            message: message.into(),
            details: None,
        }
    }

    /// 附结构化明细。
    pub fn with_details(mut self, details: Value) -> Self {
        self.details = Some(details);
        self
    }

    /// 单行 JSON（机器可读报告）。
    pub fn to_json_string(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{\"code\":\"source_error\"}".to_string())
    }
}

impl std::fmt::Display for SourceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "source error [{}]: {}", self.code, self.message)
    }
}

impl std::error::Error for SourceError {}

/// 来源语义（§6 五类各自实现；`manifest` 为完整 Manifest JSON）。
pub trait Source {
    /// 来源类型词形（v1.json source.type）。
    fn source_type(&self) -> &'static str;

    /// 取包前校验：版本固定语义（git commit 存在性、浮动分支拒绝等）。
    /// 失败即拒绝，不触源。
    fn verify(&self, manifest: &Value) -> Result<(), SourceError>;

    /// 拉取落位：把来源内容写入 `target`（installed/<id>/<version>/），
    /// 返回协商快照（无则 None）。Manifest 文件本身由调用方写入。
    fn fetch(&self, manifest: &Value, target: &Path) -> Result<FetchOutcome, SourceError>;

    /// 来源缺省信任档位（注册表元数据可覆盖，P10-1）。
    fn default_trust(&self) -> TrustLevel;
}

/// 从 Manifest 提取 source 对象（公共小助手）。
pub fn source_of(manifest: &Value) -> Result<&serde_json::Map<String, Value>, SourceError> {
    manifest
        .get("source")
        .and_then(Value::as_object)
        .ok_or_else(|| SourceError::new("source_missing", "manifest has no `source` object"))
}

/// 按 source.type 分发到对应来源（§6 五类；未知类型拒绝）。
pub fn fetch(manifest: &Value, target: &Path) -> Result<FetchOutcome, SourceError> {
    let source = source_of(manifest)?;
    let source_type = source
        .get("type")
        .and_then(Value::as_str)
        .ok_or_else(|| SourceError::new("source_missing", "`source.type` must be a string"))?;
    let source: &dyn Source = match source_type {
        "local_dir" => &local_dir::LocalDirSource,
        "git" => &git::GitSource,
        "archive" => &archive::ArchiveSource,
        "mcp_server" => &mcp_server::McpServerSource,
        "http_api" => &http_api::HttpApiSource,
        other => {
            return Err(SourceError::new(
                "source_type_unknown",
                format!("source.type {other:?} is not one of the five source kinds"),
            ))
        }
    };
    source.fetch(manifest, target)
}

/// 按 source.type 做取包前校验（未知类型拒绝）。
pub fn verify(manifest: &Value) -> Result<(), SourceError> {
    let source = source_of(manifest)?;
    let source_type = source
        .get("type")
        .and_then(Value::as_str)
        .ok_or_else(|| SourceError::new("source_missing", "`source.type` must be a string"))?;
    let source: &dyn Source = match source_type {
        "local_dir" => &local_dir::LocalDirSource,
        "git" => &git::GitSource,
        "archive" => &archive::ArchiveSource,
        "mcp_server" => &mcp_server::McpServerSource,
        "http_api" => &http_api::HttpApiSource,
        other => {
            return Err(SourceError::new(
                "source_type_unknown",
                format!("source.type {other:?} is not one of the five source kinds"),
            ))
        }
    };
    source.verify(manifest)
}

/// 来源缺省信任档位（§6：影响安装确认强度与自动更新策略，P10-1 细化）。
pub fn default_trust(source_type: &str) -> TrustLevel {
    match source_type {
        "local_dir" => TrustLevel::Local,
        "git" | "archive" => TrustLevel::Community,
        "mcp_server" | "http_api" => TrustLevel::Remote,
        _ => TrustLevel::Remote,
    }
}

/// file:// URL 或裸本地路径解析为文件系统路径。
pub fn local_file_url_to_path(url: &str) -> PathBuf {
    let raw = url.strip_prefix("file://").unwrap_or(url);
    // file://host/path 形态只取 path（本地场景 host 为空）。
    PathBuf::from(raw)
}

/// 递归复制目录树（排除 `skip` 文件名集合）。
pub(crate) fn copy_tree(src: &Path, dst: &Path, skip: &[&str]) -> Result<(), SourceError> {
    use std::fs;
    for entry in
        fs::read_dir(src).map_err(|e| SourceError::new("io", format!("read_dir {src:?}: {e}")))?
    {
        let entry = entry.map_err(|e| SourceError::new("io", format!("dir entry: {e}")))?;
        let path = entry.path();
        let name = entry.file_name();
        let name_str = name.to_string_lossy().to_string();
        if skip.contains(&name_str.as_str()) {
            continue;
        }
        let target = dst.join(&name);
        if path.is_dir() {
            fs::create_dir_all(&target)
                .map_err(|e| SourceError::new("io", format!("create_dir {target:?}: {e}")))?;
            copy_tree(&path, &target, skip)?;
        } else {
            fs::copy(&path, &target)
                .map_err(|e| SourceError::new("io", format!("copy {path:?}: {e}")))?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_trust_matches_source_kinds() {
        assert_eq!(default_trust("local_dir"), TrustLevel::Local);
        assert_eq!(default_trust("git"), TrustLevel::Community);
        assert_eq!(default_trust("archive"), TrustLevel::Community);
        assert_eq!(default_trust("mcp_server"), TrustLevel::Remote);
        assert_eq!(default_trust("http_api"), TrustLevel::Remote);
    }

    #[test]
    fn dispatch_rejects_unknown_type() {
        let manifest = serde_json::json!({"source": {"type": "carrier_pigeon"}});
        let err = fetch(&manifest, Path::new("/tmp/nowhere")).unwrap_err();
        assert_eq!(err.code, "source_type_unknown");
    }

    #[test]
    fn file_url_to_path() {
        assert_eq!(
            local_file_url_to_path("file:///tmp/x"),
            PathBuf::from("/tmp/x")
        );
        assert_eq!(local_file_url_to_path("/tmp/y"), PathBuf::from("/tmp/y"));
    }
}
