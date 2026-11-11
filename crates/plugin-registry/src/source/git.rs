//! git 来源（PLUGIN-CONTRACT §6 第二行；草案 D7 并 hosted git 三家、草案 D8
//! 浮动分支拒绝；ROADMAP P6-4）。
//!
//! 语义：source.url + source.host（github/gitlab/gitee/self，D7）+ pinned
//! （commit/tag/release，value 必填）。校验：commit 存在性（临时 clone 后
//! rev-parse --verify）；子模块与 LFS 声明按 §6 校验列记录进协商快照
//! （.gitmodules 存在性与 .gitattributes lfs 声明，本批只探测不处理）。
//! 拉取：临时 clone → checkout pinned ref → 整树（排除 .git）复制进安装
//! 目录。浮动分支（pinned.kind=branch）在 verify 与 install 两侧都拒绝
//! （validate rule 5 已拒，此处 fail-closed 复核）。
//!
//! git 二进制依赖（如实声明）：经系统 git 命令执行（宿主机在位，测试用
//! file:// 本地仓闭环，不触网络）。

use std::path::Path;
use std::process::Command;

use serde_json::{json, Value};

use super::{copy_tree, FetchOutcome, Source, SourceError};
use tool_registry::hash::sha256_hex;

use super::TrustLevel;

/// git 来源实现。
pub struct GitSource;

const HOSTS: [&str; 4] = ["github", "gitlab", "gitee", "self"];

fn run_git(args: &[&str]) -> Result<(bool, String), SourceError> {
    let output = Command::new("git")
        .args(args)
        .output()
        .map_err(|e| SourceError::new("git_missing", format!("spawn git: {e}")))?;
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    Ok((output.status.success(), text))
}

/// pinned ref 结构读取（kind/value；install 与 verify 共用）。
fn pinned_of(source: &serde_json::Map<String, Value>) -> Result<(String, String), SourceError> {
    let pinned = source
        .get("pinned")
        .and_then(Value::as_object)
        .ok_or_else(|| SourceError::new("pin_missing", "git source requires `pinned`"))?;
    let kind = pinned
        .get("kind")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let value = pinned
        .get("value")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    Ok((kind, value))
}

/// D8 浮动分支拒绝（verify 与 install 侧 fail-closed 复核同一规则）。
fn reject_floating(kind: &str) -> Result<(), SourceError> {
    if !matches!(kind, "commit" | "tag" | "release") {
        return Err(SourceError::new(
            "floating_branch_rejected",
            format!(
                "git pinned.kind {kind:?} is not commit/tag/release; floating branches are forbidden for install (draft D8, PLUGIN-CONTRACT 6)"
            ),
        ));
    }
    Ok(())
}

impl Source for GitSource {
    fn source_type(&self) -> &'static str {
        "git"
    }

    fn verify(&self, manifest: &Value) -> Result<(), SourceError> {
        let source = super::source_of(manifest)?;
        let url = source
            .get("url")
            .and_then(Value::as_str)
            .ok_or_else(|| SourceError::new("url_missing", "git source requires `url`"))?;
        let host = source
            .get("host")
            .and_then(Value::as_str)
            .ok_or_else(|| SourceError::new("host_missing", "git source requires `host` (D7)"))?;
        if !HOSTS.contains(&host) {
            return Err(SourceError::new(
                "host_invalid",
                format!("git host {host:?} not in {:?}", HOSTS),
            ));
        }
        let (kind, value) = pinned_of(source)?;
        reject_floating(&kind)?;
        if value.is_empty() {
            return Err(SourceError::new(
                "pin_value_missing",
                format!("git pinned.kind {kind:?} requires non-empty `value` (PLUGIN-CONTRACT 6)"),
            ));
        }
        // commit 存在性：临时 clone 后 rev-parse --verify（§6 校验列）。
        let tmp = std::env::temp_dir().join(format!(
            "p6-git-verify-{}-{}",
            sha256_hex(url.as_bytes()).get(..16).unwrap_or_default(),
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&tmp);
        let (ok, text) = run_git(&["clone", "--quiet", url, &tmp.to_string_lossy()])?;
        if !ok {
            return Err(SourceError::new(
                "clone_failed",
                format!("git clone {url:?} failed: {}", text.trim()),
            ));
        }
        if kind == "commit" {
            let spec = format!("{value}^{{commit}}");
            let (ok, text) =
                run_git(&["-C", &tmp.to_string_lossy(), "rev-parse", "--verify", &spec])?;
            if !ok {
                return Err(SourceError::new(
                    "commit_not_found",
                    format!("commit {value:?} does not exist in {url:?}"),
                )
                .with_details(json!({"git_output": text.trim()})));
            }
        }
        let _ = std::fs::remove_dir_all(&tmp);
        Ok(())
    }

    fn fetch(&self, manifest: &Value, target: &Path) -> Result<FetchOutcome, SourceError> {
        let source = super::source_of(manifest)?;
        let url = source
            .get("url")
            .and_then(Value::as_str)
            .ok_or_else(|| SourceError::new("url_missing", "git source requires `url`"))?;
        let (kind, value) = pinned_of(source)?;
        // install 侧 fail-closed 复核浮动分支（草案 D8）。
        reject_floating(&kind)?;

        let tmp = std::env::temp_dir().join(format!(
            "p6-git-fetch-{}-{}",
            sha256_hex(url.as_bytes()).get(..16).unwrap_or_default(),
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&tmp);
        let (ok, text) = run_git(&["clone", "--quiet", url, &tmp.to_string_lossy()])?;
        if !ok {
            return Err(SourceError::new(
                "clone_failed",
                format!("git clone {url:?} failed: {}", text.trim()),
            ));
        }
        // checkout pinned ref（tag/release 按名 checkout；commit 按 sha detach）。
        let spec = if kind == "commit" {
            format!("{value}^{{commit}}")
        } else {
            value.clone()
        };
        let (ok, text) = run_git(&[
            "-C",
            &tmp.to_string_lossy(),
            "checkout",
            "--quiet",
            "--detach",
            &spec,
        ])?;
        if !ok {
            return Err(SourceError::new(
                "ref_not_found",
                format!("git checkout {spec:?} failed: {}", text.trim()),
            ));
        }
        // 实际落位 sha（全量，回填协商快照）。
        let (_, resolved) = run_git(&["-C", &tmp.to_string_lossy(), "rev-parse", "HEAD"])?;
        let resolved = resolved.trim().to_string();

        // 子模块与 LFS 声明探测（§6 校验列：本批只记录不处理）。
        let has_submodules = tmp.join(".gitmodules").is_file();
        let lfs_declared = tmp.join(".gitattributes").is_file();

        std::fs::create_dir_all(target)
            .map_err(|e| SourceError::new("io", format!("create_dir {target:?}: {e}")))?;
        copy_tree(&tmp, target, &[".git"])?;
        let _ = std::fs::remove_dir_all(&tmp);

        let negotiation = json!({
            "resolved_commit": resolved,
            "pinned": {"kind": kind, "value": value},
            "has_submodules": has_submodules,
            "lfs_declared": lfs_declared,
        });
        // 协商快照统一落盘 negotiation.json（安装链读它写 plugin.lock.negotiation）。
        std::fs::write(target.join("negotiation.json"), negotiation.to_string())
            .map_err(|e| SourceError::new("io", format!("write negotiation.json: {e}")))?;
        Ok(FetchOutcome {
            target: target.to_path_buf(),
            negotiation: Some(negotiation),
        })
    }

    fn default_trust(&self) -> TrustLevel {
        TrustLevel::Community
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_vocabulary_is_d7() {
        let manifest = serde_json::json!({"source": {"type": "git", "url": "https://example.com/x", "host": "sourceforge", "pinned": {"kind": "commit", "value": "abc"} }});
        let err = GitSource.verify(&manifest).unwrap_err();
        assert_eq!(err.code, "host_invalid");
    }

    #[test]
    fn floating_branch_rejected_before_clone() {
        let manifest = serde_json::json!({"source": {"type": "git", "url": "https://example.com/x", "host": "github", "pinned": {"kind": "branch", "value": "main"} }});
        let err = GitSource.verify(&manifest).unwrap_err();
        assert_eq!(err.code, "floating_branch_rejected");
    }

    #[test]
    fn missing_value_rejected_before_clone() {
        let manifest = serde_json::json!({"source": {"type": "git", "url": "https://example.com/x", "host": "github", "pinned": {"kind": "commit"} }});
        let err = GitSource.verify(&manifest).unwrap_err();
        assert_eq!(err.code, "pin_value_missing");
    }
}
