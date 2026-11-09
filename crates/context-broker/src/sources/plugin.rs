//! 插件源（Phase 6 前空实现；设计稿 §4.3「插件：Phase 6 前返回空集，接口
//! 保留」原文）。插件 Registry 与状态机（DISCOVERED 到 ACTIVE）落地后由
//! 真实实现替换本源；接口形状与禁用/卸载失效信号（设计稿 §7.2）届时对齐。

use crate::error::ContextBrokerError;
use crate::source::{Candidate, Content, ContextSource};
use crate::{ContentRef, IntentQuery};

/// 源 id（Manifest item source 词表成员 plugin:<id> 的前缀段）。
pub const PLUGIN_SOURCE_ID: &str = "plugin";

/// 空插件源（零候选、不可展开；Phase 6 前的占位实现）。
#[derive(Debug, Clone, Copy, Default)]
pub struct PluginSource;

impl ContextSource for PluginSource {
    fn id(&self) -> &str {
        PLUGIN_SOURCE_ID
    }

    fn candidates(
        &self,
        _q: &IntentQuery,
        _limit: usize,
    ) -> Result<Vec<Candidate>, ContextBrokerError> {
        // Phase 6 前返回空集（设计稿 §4.3 原文）；空结果是合法结果（§9.2 第 4 条）。
        Ok(Vec::new())
    }

    fn load(&self, r: &ContentRef) -> Result<Content, ContextBrokerError> {
        Err(ContextBrokerError::Source {
            source: PLUGIN_SOURCE_ID.to_string(),
            reason: format!(
                "plugin registry lands in Phase 6; ref {:?} cannot be resolved by the empty source",
                r.ref_id
            ),
        })
    }
}
