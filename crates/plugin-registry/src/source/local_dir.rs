//! local_dir 来源（PLUGIN-CONTRACT §6 第一行；ROADMAP P6-4 先行通道）。
//!
//! 语义：source.path 指向本地目录（仓库相对或绝对路径），整树复制进安装
//! 目录；版本固定 = 目录为 git 工作树且 pinned.kind=commit 时以 HEAD sha
//! 复核（pinned.value 缺省取 HEAD 实际值并回填协商快照；目录非工作树时
//! pinned 仅做记录不校验）。校验 = 安装链对包内 checksums.sha256 逐文件
//! 复核（install 步执行，本模块不重复）。

use std::path::Path;
use std::process::Command;

use serde_json::{json, Value};

use super::{copy_tree, local_file_url_to_path, FetchOutcome, Source, SourceError, TrustLevel};

/// local_dir 来源实现。
pub struct LocalDirSource;

/// 读取目录的 git HEAD 全量 sha（非工作树返回 None）。
pub fn git_head(dir: &Path) -> Option<String> {
    let output = Command::new("git")
        .args(["-C", &dir.to_string_lossy(), "rev-parse", "HEAD"])
        .output()
        .ok()?;
    if output.status.success() {
        let sha = String::from_utf8_lossy(&output.stdout).trim().to_string();
        (sha.len() == 40).then_some(sha)
    } else {
        None
    }
}

impl Source for LocalDirSource {
    fn source_type(&self) -> &'static str {
        "local_dir"
    }

    fn verify(&self, manifest: &Value) -> Result<(), SourceError> {
        let source = super::source_of(manifest)?;
        let path = source
            .get("path")
            .and_then(Value::as_str)
            .ok_or_else(|| SourceError::new("path_missing", "local_dir source requires `path`"))?;
        let dir = local_file_url_to_path(path);
        if !dir.is_dir() {
            return Err(SourceError::new(
                "path_not_found",
                format!("local_dir path {path:?} is not a directory"),
            ));
        }
        // pinned commit 复核：目录为 git 工作树且 pinned.value 给定时，
        // HEAD sha 必须匹配（§6：取 HEAD sha；短 sha 前缀匹配）。
        let pinned = source.get("pinned").and_then(Value::as_object);
        if let Some(pinned) = pinned {
            let expected = pinned
                .get("value")
                .and_then(Value::as_str)
                .filter(|v| !v.is_empty());
            if let (Some(expected), Some(actual)) = (expected, git_head(&dir)) {
                let matches = actual.starts_with(expected) || expected == actual;
                if !matches {
                    return Err(SourceError::new(
                        "pin_mismatch",
                        format!("local_dir HEAD {actual} does not match pinned commit {expected}"),
                    )
                    .with_details(json!({"actual_head": actual, "pinned": expected})));
                }
            }
        }
        Ok(())
    }

    fn fetch(&self, manifest: &Value, target: &Path) -> Result<FetchOutcome, SourceError> {
        let source = super::source_of(manifest)?;
        let path = source
            .get("path")
            .and_then(Value::as_str)
            .ok_or_else(|| SourceError::new("path_missing", "local_dir source requires `path`"))?;
        let dir = local_file_url_to_path(path);
        std::fs::create_dir_all(target)
            .map_err(|e| SourceError::new("io", format!("create_dir {target:?}: {e}")))?;
        // 整树复制（排除来源侧可能残留的安装链工件，安装链会重新生成）。
        copy_tree(&dir, target, &["checksums.sha256", "plugin.lock"])?;
        let negotiation = git_head(&dir).map(|head| json!({"resolved_head": head}));
        // 协商快照统一落盘 negotiation.json（安装链读它写 plugin.lock.negotiation）。
        if let Some(n) = &negotiation {
            std::fs::write(target.join("negotiation.json"), n.to_string())
                .map_err(|e| SourceError::new("io", format!("write negotiation.json: {e}")))?;
        }
        Ok(FetchOutcome {
            target: target.to_path_buf(),
            negotiation,
        })
    }

    fn default_trust(&self) -> TrustLevel {
        TrustLevel::Local
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_path_rejected() {
        let manifest = serde_json::json!({"source": {"type": "local_dir"}});
        let err = LocalDirSource.verify(&manifest).unwrap_err();
        assert_eq!(err.code, "path_missing");
    }

    #[test]
    fn nonexistent_dir_rejected() {
        let manifest = serde_json::json!({"source": {"type": "local_dir", "path": "/tmp/definitely-not-here-p6"}});
        let err = LocalDirSource.verify(&manifest).unwrap_err();
        assert_eq!(err.code, "path_not_found");
    }
}
