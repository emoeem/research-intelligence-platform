//! 来源拉取接线（P6-4）：把 plugin-registry 五类来源实现接入 rip-data 布局。
//!
//! 职责：按 Manifest source.type 分发到
//! [`plugin_registry::source::fetch`]，把来源内容拉取落位到安装目录
//! （installed/<id>/<version>/），并把 Manifest 文档统一写入
//! manifest.yaml（JSON 亦为合法 YAML 1.2 子集，parse_document 通道一致）。
//! 取包前校验（版本固定语义，fail-fast 不触源）经
//! [`plugin_registry::source::verify`] 单独暴露给 CLI。

use std::path::Path;

use serde_json::Value;

use crate::RuntimeError;
use plugin_registry::source;

/// 安装目录内 Manifest 统一落名（与 plugin-registry stage_package 同款）。
const MANIFEST_FILE_NAME: &str = "manifest.yaml";

/// 取包前校验（git commit 存在性、archive 哈希、浮动分支等；失败不触源）。
pub fn verify(manifest: &Value) -> Result<(), RuntimeError> {
    source::verify(manifest).map_err(RuntimeError::from)
}

/// 拉取落位：写入 manifest.yaml + 按 source.type 分发拉取；返回协商快照
/// （mcp_server 握手 / http_api 契约 / local_dir·git resolved ref；安装链
/// 会读包内 negotiation.json 写入 plugin.lock.negotiation）。
pub fn fetch_into(manifest: &Value, install_dir: &Path) -> Result<Option<Value>, RuntimeError> {
    std::fs::create_dir_all(install_dir).map_err(|e| {
        RuntimeError::new("io", format!("create_dir {:?}: {e}", install_dir.display()))
    })?;
    std::fs::write(install_dir.join(MANIFEST_FILE_NAME), manifest.to_string())
        .map_err(|e| RuntimeError::new("io", format!("write manifest.yaml: {e}")))?;
    let outcome = source::fetch(manifest, install_dir).map_err(RuntimeError::from)?;
    Ok(outcome.negotiation)
}
