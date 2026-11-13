//! ContextSource 端口与候选/内容载荷类型（设计稿 CONTEXT-BROKER-DESIGN §10
//! 公开接口签名：`candidates(&IntentQuery, usize) -> Vec<Candidate>` 与
//! `load(&ContentRef) -> Content`；错误类型统一为 [`ContextBrokerError`]）。
//!
//! 端口纪律（设计稿 §10 依赖方向第 2 条 + PHASE4-DESIGN §5.1 总则 3）：
//! 上游取数面经本 trait 接入，assemble（P4-4）只面向 trait，不硬编码具体
//! 源；tool-registry 直连适配器落 sources/registry.rs（P4-0 决议 2），
//! Mock 仅作测试载具（决议 2 同款口径）。
//!
//! 打分与 matched_signals（设计稿 §4.3）属 P4-3 intent 面与 P4-4 assemble
//! 面：源只回中性候选（title/summary/family/引用/预算），不携带分数。

use crate::error::ContextBrokerError;
use crate::{ContentRef, ContextItemKind, ContextLayer, IntentQuery};

/// 源级候选条目（装配选择的输入；对应 Manifest item 的前身，P4-4 落库时
/// 携带 reason 与 matched_signals）。
#[derive(Debug, Clone, PartialEq)]
pub struct Candidate {
    /// 源内稳定 id（registry 为 tool_id；rulepack 为 rp-rules:<rule_id>）。
    pub item_id: String,
    /// 条目种类（domain::context 词表）。
    pub kind: ContextItemKind,
    /// 选中后落入的层级（工具/知识候选为 L2；规则包为 L0）。
    pub layer: ContextLayer,
    /// 标题（工具名 / 规则标题）。
    pub title: String,
    /// 摘要形态文本（L2 主形态，设计稿 §5.3 第 3 条）。
    pub summary: String,
    /// 工具族标签（manifest family 原值；规则包等无族源为 None；P4-3 打分输入）。
    pub family: Option<String>,
    /// 治理状态（源可知时携带；None = 源面不可见即已过滤，设计稿 §4.3
    /// 一票否决在 score 层对非 None 值执行，P4-3 起使用）。
    pub governance: Option<String>,
    /// 质量等级（A/B/C/D；SearchHit.quality_grade 等可知时携带，None = 缺证，
    /// 打分按 0 计，P4-3 起使用）。
    pub quality_grade: Option<String>,
    /// 权威引用（source/ref_id/ref_version/content_hash，设计稿 §9.2 citations 口径）。
    pub reference: ContentRef,
    /// 摘要形态 token 估算（入库预计算口径，设计稿 §5.2）。
    pub est_tokens: u64,
}

/// 按需展开的内容载荷（L3 full 形态；expand 记账在 P4-4）。
#[derive(Debug, Clone, PartialEq)]
pub struct Content {
    /// 被展开内容的权威引用。
    pub reference: ContentRef,
    /// 正文（知识全文 / 工具 Schema JSON / 规则正文 / 规则包说明）。
    pub body: String,
    /// 正文 token 估算（从 l3_reserved 扣账的依据，设计稿 §5.3 第 4 条）。
    pub est_tokens: u64,
}

/// 上下文源端口（设计稿 §10 签名；实现方：rulepack/registry/plugin/knowledge/
/// rtis/artifact 六源）。
///
/// P8-2 起补 `Send + Sync` 上界：workflow-engine Parallel 节点的 fan 子
/// 分支在线程内并发装配（Broker 以 Arc 跨线程共享），装配面要求源可跨
/// 线程（既有六实现均持 Pool/文件路径等 Send+Sync 载体，无共享可变态）。
pub trait ContextSource: Send + Sync {
    /// 源 id（审计与 Manifest item source 列词表：registry/rulepack/plugin/...）。
    fn id(&self) -> &str;

    /// 按意图取候选（至多 limit 条；源不做打分，q 由 P4-3/P4-4 的匹配面消费）。
    ///
    /// 空结果是合法结果（设计稿 §9.2 第 4 条：返回空集合而非错误）。
    fn candidates(
        &self,
        q: &IntentQuery,
        limit: usize,
    ) -> Result<Vec<Candidate>, ContextBrokerError>;

    /// 按权威引用取内容（L3 按需展开；ref 无法解析时返回 [`ContextBrokerError::Source`]）。
    fn load(&self, r: &ContentRef) -> Result<Content, ContextBrokerError>;

    /// 工具 brief 文本（设计稿 §8 tier2；仅工具说明生成器所在源有实现，
    /// 其余源默认 None：装配面装 L2 工具条目时以此替换摘要形态，P4-4 起
    /// 由 assemble 消费）。
    fn brief_for(&self, item_id: &str) -> Option<String> {
        let _ = item_id;
        None
    }
}
