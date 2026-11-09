//! Phase 1 工具注册表（TOOL-CONTRACT §10.1 落地物第 3 行，ROADMAP P1-4）。
//!
//! 职责：
//! - [`validate`]：Manifest 校验九条（TOOL-CONTRACT §2.5）+ 轻量 JSON Schema
//!   子集校验器（[`jsonschema_lite`]）
//! - [`store`]：tool / tool_version 读写面（沿用既有列不新增列，第 9 条落库）
//! - [`catalog`]：发现 / 查询 / Schema 获取接口
//! - [`Registry`]：九条串接的门面（静态校验 → secrets 校验 → 版本可比 → 落库）
//!
//! 依赖方向（MODULE_BOUNDARIES §8.1 纪律）：database + domain + serde_json +
//! jsonschema（draft 2020-12 校验，复用维护中的成熟实现）+ sha2（content_hash），
//! 不依赖任何 engine crate。

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod catalog;
pub mod error;
pub mod hash;
pub mod jsonschema_lite;
pub mod store;
pub mod validate;

use std::sync::Arc;

use database::Pool;
use serde_json::Value;

pub use error::{RegistryError, ValidationRule};
pub use store::{Registered, ToolRow, VersionRow};
pub use validate::{
    content_hash, normalize_json, Options, ValidatedManifest, SUPPORTED_SCHEMA_VERSION,
};

/// 平台 secrets 存储侧信（键名是否存在），跨线程共享。
pub type SharedSecretsLookup = Arc<dyn Fn(&str) -> bool + Send + Sync>;

/// 注册表门面：串接 §2.5 九条（1/3/4/5/6/8 静态校验，7 secrets，
/// 2 版本可比，9 落库）与 catalog 读路径。
#[derive(Clone)]
pub struct Registry {
    pool: Pool,
    options: Options,
    secrets_lookup: Option<SharedSecretsLookup>,
}

impl Registry {
    /// 在给定连接池上打开注册表（默认生产语义：拒绝占位 digest，
    /// 未配置 secrets 存储）。
    pub fn open(pool: Pool) -> Self {
        Self {
            pool,
            options: Options::default(),
            secrets_lookup: None,
        }
    }

    /// 覆盖校验开关（`allow_placeholder_digest` 仅测试库验收流程使用）。
    pub fn with_options(mut self, options: Options) -> Self {
        self.options = options;
        self
    }

    /// 注入平台 secrets 存储侧信（§2.5 第 7 条；Phase 1 数据面尚无 secrets 表）。
    pub fn with_secrets_lookup(mut self, lookup: SharedSecretsLookup) -> Self {
        self.secrets_lookup = Some(lookup);
        self
    }

    /// 底层连接池视图。
    pub fn pool(&self) -> &Pool {
        &self.pool
    }

    /// 登记：九条按序执行，全部通过后事务落库（任一步拒绝不留半登记）。
    pub fn register(&self, manifest: &Value) -> Result<Registered, RegistryError> {
        let vm = validate::validate_static(manifest, &self.options)?;
        {
            let obj = manifest
                .as_object()
                .ok_or_else(|| RegistryError::Validation {
                    rule: ValidationRule::RequiredFields,
                    tool_id: None,
                    reason: "manifest must be a JSON object".to_string(),
                })?;
            validate::check_secrets(obj, &vm.id, self.secrets_lookup.as_deref())?;
        }
        validate::check_version_comparable(&self.pool, &vm.id, &vm.version)?;
        store::register(&self.pool, &vm)
    }

    /// 发现：列出全部已登记工具。
    pub fn discover(&self) -> Result<Vec<catalog::ToolSummary>, RegistryError> {
        catalog::discover(&self.pool)
    }

    /// 查询：按 id 取工具详情（含版本面）。
    pub fn query(&self, tool_id: &str) -> Result<catalog::ToolDetail, RegistryError> {
        catalog::query(&self.pool, tool_id)
    }

    /// Schema 获取：返回 (input_schema, output_schema)。
    pub fn schemas(
        &self,
        tool_id: &str,
        version: Option<&str>,
    ) -> Result<(Value, Value), RegistryError> {
        catalog::get_schemas(&self.pool, tool_id, version)
    }
}
