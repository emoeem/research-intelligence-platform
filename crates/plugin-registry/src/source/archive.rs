//! archive 来源（PLUGIN-CONTRACT §6 第三行；ROADMAP P6-4）。
//!
//! 语义：source.url + sha256（必填，即版本固定形态）+ size。校验：归档
//! 文件实际 sha256 与清单声明逐字节比对（不匹配即拒绝并产出机器可读
//! 报告，由链记 INVALID），size 对账。拉取：tar 解包（复用维护中的 tar
//! crate）进安装目录，解压后逐文件 checksums 由安装链统一生成（§6 校验
//! 列口径）。
//!
//! 网络边界（如实声明）：url 支持本地路径与 file:// URL；远程 HTTP 下载
//! 归 P10（本批测试闭环在本地 fixture）。tar 形态为无压缩 .tar（gz 支持
//! 按需再加，见 plugin-registry Cargo.toml 注释）。

use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use super::{local_file_url_to_path, FetchOutcome, Source, SourceError};
use tool_registry::hash::sha256_hex;

use super::TrustLevel;

/// archive 来源实现。
pub struct ArchiveSource;

fn archive_path_of(source: &serde_json::Map<String, Value>) -> Result<PathBuf, SourceError> {
    let url = source
        .get("url")
        .and_then(Value::as_str)
        .ok_or_else(|| SourceError::new("url_missing", "archive source requires `url`"))?;
    Ok(local_file_url_to_path(url))
}

fn expected_sha_of(source: &serde_json::Map<String, Value>) -> Result<String, SourceError> {
    source
        .get("sha256")
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| SourceError::new("sha_missing", "archive source requires `sha256`"))
}

impl Source for ArchiveSource {
    fn source_type(&self) -> &'static str {
        "archive"
    }

    fn verify(&self, manifest: &Value) -> Result<(), SourceError> {
        let source = super::source_of(manifest)?;
        let path = archive_path_of(source)?;
        if !path.is_file() {
            return Err(SourceError::new(
                "archive_not_found",
                format!("archive file {:?} is not a file", path.display()),
            ));
        }
        let expected = expected_sha_of(source)?;
        let bytes = std::fs::read(&path)
            .map_err(|e| SourceError::new("io", format!("read {path:?}: {e}")))?;
        // size 对账（§6 必填字段）。
        if let Some(size) = source.get("size").and_then(Value::as_i64) {
            let actual = bytes.len() as i64;
            if actual != size {
                return Err(SourceError::new(
                    "size_mismatch",
                    format!("archive size {actual} does not match declared size {size}"),
                )
                .with_details(json!({"actual": actual, "declared": size})));
            }
        }
        let actual = sha256_hex(&bytes);
        if actual != expected {
            return Err(SourceError::new(
                "archive_hash_mismatch",
                format!(
                    "archive sha256 {actual} does not match declared {expected} (PLUGIN-CONTRACT 6)"
                ),
            )
            .with_details(json!({"actual": actual, "declared": expected})));
        }
        Ok(())
    }

    fn fetch(&self, manifest: &Value, target: &Path) -> Result<FetchOutcome, SourceError> {
        // install 侧 fail-closed 复核哈希（validate 之后内容仍可能变化）。
        self.verify(manifest)?;
        let source = super::source_of(manifest)?;
        let path = archive_path_of(source)?;
        std::fs::create_dir_all(target)
            .map_err(|e| SourceError::new("io", format!("create_dir {target:?}: {e}")))?;
        let file = std::fs::File::open(&path)
            .map_err(|e| SourceError::new("io", format!("open {path:?}: {e}")))?;
        let mut archive = tar::Archive::new(file);
        // tar crate unpack 对 `..` 上跳与绝对路径条目默认拒绝（fail-closed），
        // 路径穿越面由解包器兜底，无需二次扫描。
        archive
            .unpack(target)
            .map_err(|e| SourceError::new("unpack_failed", format!("tar unpack {path:?}: {e}")))?;
        Ok(FetchOutcome {
            target: target.to_path_buf(),
            negotiation: None,
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
    fn missing_url_rejected() {
        let manifest =
            serde_json::json!({"source": {"type": "archive", "sha256": "0".repeat(64), "size": 1}});
        let err = ArchiveSource.verify(&manifest).unwrap_err();
        assert_eq!(err.code, "url_missing");
    }

    #[test]
    fn nonexistent_archive_rejected() {
        let manifest = serde_json::json!({"source": {"type": "archive", "url": "/tmp/p6-no-such.tar", "sha256": "0".repeat(64), "size": 1}});
        let err = ArchiveSource.verify(&manifest).unwrap_err();
        assert_eq!(err.code, "archive_not_found");
    }
}
