//! knowledge 知识源（platform 知识面经 search-engine 公开接口；设计稿
//! CONTEXT-BROKER-DESIGN §4.3 候选第二类 + §12 复用对照首行）。
//!
//! 纪律：检索只走 [`SearchEngine::search`]（search-engine/src/query.rs:66），
//! 不重写 FTS5、不读索引库内部表（引擎间不互读库，MODULE_BOUNDARIES §8.2）；
//! 治理轴 QUARANTINED/REJECTED/ARCHIVED 不可见由 search-engine 查询面同款
//! 域约束兜底（query.rs:268），score 层 [`vetoes`] 对携带治理值的候选二次
//! 把关（设计稿 §4.3 一票否决）。L3 全文展开随 Phase 7 知识正文存储定义（D-13）。

use crate::error::ContextBrokerError;
use crate::source::{Candidate, Content, ContextSource};
use crate::{ContentRef, ContextItemKind, ContextLayer, IntentQuery};
use search_engine::model::{SearchHit, SearchQuery};

/// 源 id（Manifest item source 词表成员，设计稿 §6.1 platform-kb）。
pub const KNOWLEDGE_SOURCE_ID: &str = "platform-kb";

/// platform 知识源（进程内持有 SearchEngine）。
pub struct KnowledgeSource {
    engine: search_engine::query::SearchEngine,
}

impl KnowledgeSource {
    /// 用已构建索引库的引擎构造。
    pub fn new(engine: search_engine::query::SearchEngine) -> Self {
        Self { engine }
    }

    /// 关键词检索（SearchEngine::search 直连；空关键词为浏览模式，
    /// 检索面按设计稿 §4.3 候选语义不使用）。
    pub fn search(
        &self,
        keywords: &[String],
        limit: usize,
    ) -> Result<Vec<Candidate>, ContextBrokerError> {
        if keywords.is_empty() {
            return Ok(Vec::new());
        }
        let mut query = SearchQuery::new(keywords.to_vec());
        query.limit = limit.max(1);
        let hits = self
            .engine
            .search(&query)
            .map_err(|e| ContextBrokerError::Source {
                source: KNOWLEDGE_SOURCE_ID.to_string(),
                reason: e.to_string(),
            })?;
        Ok(hits.hits.iter().map(hit_to_candidate).collect())
    }
}

/// SearchHit 到源级候选的映射（引用与预算预计算；quality_grade 随候选
/// 携带供打分，治理值引擎面已过滤故不携带）。
fn hit_to_candidate(hit: &SearchHit) -> Candidate {
    let summary = hit.description.clone().unwrap_or_default();
    Candidate {
        item_id: hit.resource_id.clone(),
        kind: ContextItemKind::Knowledge,
        layer: ContextLayer::L2,
        title: hit.name.clone(),
        summary: summary.clone(),
        family: None,
        governance: None,
        quality_grade: hit.quality_grade.clone(),
        reference: ContentRef {
            source: KNOWLEDGE_SOURCE_ID.to_string(),
            ref_id: hit.resource_id.clone(),
            ref_version: None,
            content_hash: None,
            uri: Some(hit.canonical_url.clone()),
        },
        est_tokens: crate::est_tokens(&summary),
    }
}

impl ContextSource for KnowledgeSource {
    fn id(&self) -> &str {
        KNOWLEDGE_SOURCE_ID
    }

    fn candidates(
        &self,
        q: &IntentQuery,
        limit: usize,
    ) -> Result<Vec<Candidate>, ContextBrokerError> {
        // 关键词取归一化文本的分词结果（resource-engine tokenize 口径，
        // 与 search-engine 的 expand_query_terms 衔接；设计稿 §4.3 知识候选）。
        let keywords = resource_engine::tokenizer::tokenize(&q.text_normalized);
        self.search(&keywords, limit)
    }

    fn load(&self, r: &ContentRef) -> Result<Content, ContextBrokerError> {
        if r.source != KNOWLEDGE_SOURCE_ID {
            return Err(ContextBrokerError::Source {
                source: KNOWLEDGE_SOURCE_ID.to_string(),
                reason: format!("ref source {:?} is not platform-kb", r.source),
            });
        }
        // 索引库只承载元数据（设计稿 §7 索引可重建），且 platform 库
        // resource 表无正文载荷列（name/description/canonical_url 即全部）：
        // L3「全文」在 Phase 4 数据面不存在。接线随 Phase 7 知识正文存储
        // 定义（PHASE4-ACCEPTANCE §七.6 / 实施差异文档 D-13）；rtis 源
        // poc get 不受此限。
        Err(ContextBrokerError::Source {
            source: KNOWLEDGE_SOURCE_ID.to_string(),
            reason: format!(
                "full-text expand for {:?} unavailable: platform-kb has no body payload column in the Phase 4 data plane; wiring lands with Phase 7 knowledge body storage (PHASE4-ACCEPTANCE §7.6 / D-13)",
                r.ref_id
            ),
        })
    }
}
