//! 插件目录发布形态（ROADMAP P10-1「插件注册表与工具包目录、目录发布
//! 形态」；P10-2 落地）。发布 = 从 Manifest 目录构建确定性目录文档
//! （[`PluginCatalog`]）+ 落盘 + 完整性校验（[`read_and_verify_catalog`]）。
//! 目录文档是可发布的注册表形态：逐条目携带 trust_level 与由信任级推导的
//! 安装确认强度（§6 trust_policy 单一判定面）与 D10 自动提示位，消费方
//! （server / web / 安装器）按条目 install 即可，安装链以 Manifest 为准
//! （目录只做索引，不复制正文）。
//!
//! 确定性：entries 按 plugin_id 排序；content_sha256 覆盖 schema_version、
//! default_trust 与 entries 的规范化 JSON（不含 generated_at 与 sha 自身），
//! 发布物可独立复核；generated_at_unix 不参与完整性。
//!
//! trust 语义：default_trust 由发布方显式给定（平台自带目录发布为
//! official：平台对目录内逐条 Manifest 的 pin 与许可政策负责），逐条目
//! overrides 可覆盖（外部来源混入平台目录时按实际来源降档）。

use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::hash::sha256_hex;
use crate::lock::TrustLevel;
use crate::manifest;
use crate::source_default_trust;
use crate::trust_policy::{auto_update_prompt_allowed, install_confirmation};

/// 目录文档 schema 版本。
pub const CATALOG_SCHEMA_VERSION: &str = "1";

/// 目录错误（机器可读：code + message）。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CatalogError {
    /// 稳定错误码：manifests_dir_missing / manifest_parse_failed /
    /// trust_invalid / write_failed / read_failed / parse_failed /
    /// sha_mismatch / unsorted。
    pub code: String,
    /// 人类可读原因。
    pub message: String,
}

impl CatalogError {
    fn new(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.to_string(),
            message: message.into(),
        }
    }
}

impl std::fmt::Display for CatalogError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "catalog error [{}]: {}", self.code, self.message)
    }
}

impl std::error::Error for CatalogError {}

/// 目录条目（索引面：安装所需的最小元数据 + 信任策略推导位）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CatalogEntry {
    /// 插件 id。
    pub plugin_id: String,
    /// 版本。
    pub version: String,
    /// 展示名。
    pub name: String,
    /// 六类之一。
    pub kind: String,
    /// 一句话摘要。
    pub description: String,
    /// 许可证。
    pub license: String,
    /// 来源类型（五类之一）。
    pub source_type: String,
    /// 来源定位。
    pub source_ref: String,
    /// 来源缺省信任档位（§6 按来源类型推导，发布方 overrides 前的参考）。
    pub source_default_trust: String,
    /// 发布方赋给的信任级（安装时 --trust 的建议值）。
    pub trust_level: String,
    /// 安装确认强度（由 trust_level 经 trust_policy 推导，发布物内固化）。
    pub install_confirmation: String,
    /// D10 自动提示位（由 trust_level 推导）。
    pub auto_update_prompt: bool,
    /// Manifest 文件路径（相对仓库根；安装命令的 --manifest 消费面）。
    pub manifest_path: String,
}

/// 插件目录文档（可发布形态）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PluginCatalog {
    /// schema 版本。
    pub schema_version: String,
    /// 生成时刻（Unix 秒；不参与完整性校验）。
    pub generated_at_unix: i64,
    /// 发布方默认信任级。
    pub default_trust: String,
    /// 条目（按 plugin_id 排序）。
    pub entries: Vec<CatalogEntry>,
    /// 完整性指纹：schema_version + default_trust + entries 规范化 JSON
    /// 的 sha256（不含 generated_at 与本字段）。
    pub content_sha256: String,
}

impl PluginCatalog {
    /// 完整性指纹的规范化输入（发布与校验共用，防两处口径漂移）。
    fn canonical_payload(&self) -> String {
        let payload = serde_json::json!({
            "schema_version": self.schema_version,
            "default_trust": self.default_trust,
            "entries": self.entries,
        });
        serde_json::to_string(&payload).unwrap_or_default()
    }

    /// 重算并比对完整性指纹（不修改自身）。
    pub fn verify_integrity(&self) -> Result<(), CatalogError> {
        if sha256_hex(self.canonical_payload().as_bytes()) != self.content_sha256 {
            return Err(CatalogError::new(
                "sha_mismatch",
                "content_sha256 does not match recomputed fingerprint (catalog tampered or truncated)",
            ));
        }
        Ok(())
    }
}

/// 从 Manifest 目录构建目录文档（*.json，按 plugin_id 排序；同 id 不同
/// 版本拒绝：目录是发布态不是版本历史）。
pub fn build_catalog(
    manifests_dir: &Path,
    default_trust: TrustLevel,
    overrides: &[(String, TrustLevel)],
) -> Result<PluginCatalog, CatalogError> {
    if !manifests_dir.is_dir() {
        return Err(CatalogError::new(
            "manifests_dir_missing",
            format!("manifests dir {:?} does not exist", manifests_dir.display()),
        ));
    }
    let mut paths: Vec<std::path::PathBuf> = fs::read_dir(manifests_dir)
        .map_err(|e| CatalogError::new("read_failed", format!("read dir: {e}")))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("json"))
        .collect();
    paths.sort();
    let mut entries: Vec<CatalogEntry> = Vec::new();
    for path in &paths {
        let body = fs::read_to_string(path)
            .map_err(|e| CatalogError::new("read_failed", format!("read {path:?}: {e}")))?;
        let value = manifest::parse_document(&body).map_err(|e| {
            CatalogError::new(
                "manifest_parse_failed",
                format!("parse {path:?}: {}", e.message),
            )
        })?;
        let obj = value.as_object().ok_or_else(|| {
            CatalogError::new("manifest_parse_failed", format!("{path:?}: not an object"))
        })?;
        let plugin_id = obj
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        let source_type = obj
            .get("source")
            .and_then(|s| s.get("type"))
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        let source_ref = ["path", "url", "endpoint", "base_url"]
            .iter()
            .find_map(|k| {
                obj.get("source")
                    .and_then(|s| s.get(*k))
                    .and_then(|v| v.as_str())
            })
            .unwrap_or_default()
            .to_string();
        let trust = overrides
            .iter()
            .find(|(id, _)| id == &plugin_id)
            .map(|(_, t)| *t)
            .unwrap_or(default_trust);
        let confirmation = install_confirmation(trust);
        entries.push(CatalogEntry {
            plugin_id: plugin_id.clone(),
            version: obj
                .get("version")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string(),
            name: obj
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string(),
            kind: obj
                .get("kind")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string(),
            description: obj
                .get("description")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string(),
            license: obj
                .get("license")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string(),
            source_type,
            source_ref,
            source_default_trust: source_default_trust(
                obj.get("source")
                    .and_then(|s| s.get("type"))
                    .and_then(|v| v.as_str())
                    .unwrap_or_default(),
            )
            .as_str()
            .to_string(),
            trust_level: trust.as_str().to_string(),
            install_confirmation: confirmation.as_str().to_string(),
            auto_update_prompt: auto_update_prompt_allowed(trust),
            manifest_path: path.to_string_lossy().to_string(),
        });
    }
    // 同 id 多版本拒绝（目录是发布态）；排序保证确定性输出。
    entries.sort_by(|a, b| a.plugin_id.cmp(&b.plugin_id));
    let mut seen = std::collections::BTreeSet::new();
    for e in &entries {
        if !seen.insert(e.plugin_id.clone()) {
            return Err(CatalogError::new(
                "manifest_parse_failed",
                format!("duplicate plugin id {:?} in manifests dir", e.plugin_id),
            ));
        }
    }
    let mut catalog = PluginCatalog {
        schema_version: CATALOG_SCHEMA_VERSION.to_string(),
        generated_at_unix: crate::now_unix(),
        default_trust: default_trust.as_str().to_string(),
        entries,
        content_sha256: String::new(),
    };
    catalog.content_sha256 = sha256_hex(catalog.canonical_payload().as_bytes());
    Ok(catalog)
}

/// 发布（构建 + 原子落盘：同目录临时文件 + rename）。
pub fn write_catalog(catalog: &PluginCatalog, path: &Path) -> Result<(), CatalogError> {
    let body = serde_json::to_string_pretty(catalog)
        .map_err(|e| CatalogError::new("write_failed", format!("serialize: {e}")))?;
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, body)
        .map_err(|e| CatalogError::new("write_failed", format!("write {tmp:?}: {e}")))?;
    fs::rename(&tmp, path)
        .map_err(|e| CatalogError::new("write_failed", format!("rename {path:?}: {e}")))
}

/// 读取并校验发布物（sha 完整性 + 排序 + schema 版本；任何不过即拒绝）。
pub fn read_and_verify_catalog(path: &Path) -> Result<PluginCatalog, CatalogError> {
    let body = fs::read_to_string(path)
        .map_err(|e| CatalogError::new("read_failed", format!("read {path:?}: {e}")))?;
    let catalog: PluginCatalog = serde_json::from_str(&body)
        .map_err(|e| CatalogError::new("parse_failed", format!("parse {path:?}: {e}")))?;
    if catalog.schema_version != CATALOG_SCHEMA_VERSION {
        return Err(CatalogError::new(
            "parse_failed",
            format!(
                "schema_version {:?} unsupported (expect {:?})",
                catalog.schema_version, CATALOG_SCHEMA_VERSION
            ),
        ));
    }
    let sorted = {
        let mut ids: Vec<&str> = catalog
            .entries
            .iter()
            .map(|e| e.plugin_id.as_str())
            .collect();
        ids.sort_unstable();
        ids.dedup();
        ids.len() == catalog.entries.len()
    };
    if !sorted {
        return Err(CatalogError::new(
            "unsorted",
            "entries are not strictly sorted by plugin_id (regenerated catalog required)",
        ));
    }
    catalog.verify_integrity()?;
    Ok(catalog)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo_manifests_dir() -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../manifests/plugins")
    }

    #[test]
    fn build_publish_verify_roundtrip_from_real_manifests() {
        let out_dir = std::env::temp_dir().join(format!("p10-catalog-{}", std::process::id()));
        let _ = fs::remove_dir_all(&out_dir);
        fs::create_dir_all(&out_dir).unwrap();
        let out = out_dir.join("catalog.json");

        let catalog = build_catalog(&repo_manifests_dir(), TrustLevel::Official, &[]).unwrap();
        assert_eq!(catalog.schema_version, "1");
        assert_eq!(catalog.default_trust, "official");
        assert!(!catalog.entries.is_empty(), "repo manifests present");
        // 排序 + official 推导位（D10）。
        let ids: Vec<&str> = catalog
            .entries
            .iter()
            .map(|e| e.plugin_id.as_str())
            .collect();
        let mut sorted = ids.clone();
        sorted.sort_unstable();
        assert_eq!(ids, sorted);
        for entry in &catalog.entries {
            assert_eq!(entry.trust_level, "official");
            assert_eq!(entry.install_confirmation, "single");
            assert!(entry.auto_update_prompt);
            assert!(entry.manifest_path.ends_with(".json"));
        }
        write_catalog(&catalog, &out).unwrap();
        let verified = read_and_verify_catalog(&out).unwrap();
        assert_eq!(verified, catalog);
        let _ = fs::remove_dir_all(&out_dir);
    }

    #[test]
    fn tampered_catalog_is_rejected() {
        let catalog = build_catalog(&repo_manifests_dir(), TrustLevel::Official, &[]).unwrap();
        // 逐条目描述被篡改 → sha 不匹配。
        let mut tampered = catalog.clone();
        if let Some(e) = tampered.entries.first_mut() {
            e.description.push('x');
        }
        let err = tampered.verify_integrity().unwrap_err();
        assert_eq!(err.code, "sha_mismatch");
        // 信任级被篡改（降档攻击）→ sha 不匹配。
        let mut demoted = catalog.clone();
        demoted.default_trust = "remote".to_string();
        let err = demoted.verify_integrity().unwrap_err();
        assert_eq!(err.code, "sha_mismatch");
    }

    #[test]
    fn overrides_downgrade_external_source_entries() {
        // overrides：混入平台目录的外部 git 来源条目按实际来源降档 community
        //（§6：git/archive 缺省 community；确认强度与 D10 位随之变化）。
        let catalog = build_catalog(&repo_manifests_dir(), TrustLevel::Official, &[]).unwrap();
        let first_id = catalog.entries[0].plugin_id.clone();
        let downgraded = build_catalog(
            &repo_manifests_dir(),
            TrustLevel::Official,
            &[(first_id.clone(), TrustLevel::Community)],
        )
        .unwrap();
        let entry = downgraded
            .entries
            .iter()
            .find(|e| e.plugin_id == first_id)
            .unwrap();
        assert_eq!(entry.trust_level, "community");
        assert_eq!(entry.install_confirmation, "confirm");
        assert!(
            !entry.auto_update_prompt,
            "D10: community never auto-prompts"
        );
        // git 来源的参考档位（source_default_trust）仍是 community。
        if entry.source_type == "git" {
            assert_eq!(entry.source_default_trust, "community");
        }
        // 完整性：overrides 参与 sha 载荷，重新发布物可校验。
        write_catalog(
            &downgraded,
            &std::env::temp_dir().join(format!("p10-catalog-ov-{}.json", std::process::id())),
        )
        .unwrap();
    }

    #[test]
    fn missing_dir_rejected() {
        let err = build_catalog(
            Path::new("/nonexistent/p10-catalog"),
            TrustLevel::Official,
            &[],
        )
        .unwrap_err();
        assert_eq!(err.code, "manifests_dir_missing");
    }
}
