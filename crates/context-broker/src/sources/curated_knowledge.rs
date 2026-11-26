//! RIP 自有结构化知识源（migrations/platform.db 的 `knowledge` 表）。
//!
//! 只读边界：仅暴露 lifecycle=VERIFIED 且 governance=ACTIVE 的条目；
//! CANDIDATE / DISCOVERED 不进入模型上下文。负知识将误报边界、停止条件、
//! 不适用条件作为 L3 正文的一部分，避免只把「漏洞结论」交给 Agent。

use crate::error::ContextBrokerError;
use crate::source::{Candidate, Content, ContextSource};
use crate::{ContentRef, ContextItemKind, ContextLayer, IntentQuery};
use database::rusqlite::OptionalExtension;
use database::{DbResult, Pool};

/// Stable source identifier for RIP-owned, verified knowledge.
pub const CURATED_KNOWLEDGE_SOURCE_ID: &str = "curated-kb";

/// Resource tokenizer is ASCII-oriented; curated KB also contains Chinese text.
/// Keep the existing tokenizer for ASCII terms and add deterministic CJK bigrams
/// plus four-character windows (the previous ERI probe-token policy).
fn query_tokens(text: &str) -> Vec<String> {
    let mut tokens = resource_engine::tokenizer::tokenize(text);
    let mut run = Vec::new();
    let flush = |run: &mut Vec<char>, tokens: &mut Vec<String>| {
        if run.is_empty() {
            return;
        }
        if run.len() <= 4 {
            tokens.push(run.iter().collect());
        } else {
            for window in run.windows(2) {
                tokens.push(window.iter().collect());
            }
            for start in (0..=run.len() - 4).step_by(2) {
                tokens.push(run[start..start + 4].iter().collect());
            }
        }
        run.clear();
    };
    for ch in text.chars() {
        if ('\u{3400}'..='\u{4dbf}').contains(&ch)
            || ('\u{4e00}'..='\u{9fff}').contains(&ch)
            || ('\u{f900}'..='\u{faff}').contains(&ch)
        {
            run.push(ch);
        } else {
            flush(&mut run, &mut tokens);
        }
    }
    flush(&mut run, &mut tokens);
    tokens.sort();
    tokens.dedup();
    tokens
}

/// Read-only source for verified knowledge rows and their negative constraints.
#[derive(Clone)]
pub struct CuratedKnowledgeSource {
    pool: Pool,
}

#[derive(Debug, Clone)]
struct KnowledgeRow {
    knowledge_id: String,
    record_kind: String,
    title: String,
    description: Option<String>,
    category: Option<String>,
    tags: Option<String>,
    content: Option<String>,
    source: Option<String>,
    source_url: Option<String>,
    evidence_level: Option<String>,
    verification_basis: Option<String>,
    negates_knowledge_id: Option<String>,
    neg_source_knowledge_id: Option<String>,
    neg_do_not_conclude: Option<String>,
    neg_false_positive_cases: Option<String>,
    neg_known_failures: Option<String>,
    neg_stop_conditions: Option<String>,
    neg_not_applicable_when: Option<String>,
}

impl KnowledgeRow {
    fn searchable_text(&self) -> String {
        [
            Some(self.knowledge_id.as_str()),
            Some(self.title.as_str()),
            self.description.as_deref(),
            self.category.as_deref(),
            self.tags.as_deref(),
            self.content.as_deref(),
            self.source.as_deref(),
            self.evidence_level.as_deref(),
            self.verification_basis.as_deref(),
            self.neg_do_not_conclude.as_deref(),
            self.neg_false_positive_cases.as_deref(),
            self.neg_known_failures.as_deref(),
            self.neg_stop_conditions.as_deref(),
            self.neg_not_applicable_when.as_deref(),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
    }

    fn summary(&self) -> String {
        if self.record_kind == "negative" {
            return self
                .neg_do_not_conclude
                .as_deref()
                .or(self.neg_false_positive_cases.as_deref())
                .unwrap_or(&self.title)
                .to_string();
        }
        self.description
            .as_deref()
            .filter(|s| !s.trim().is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| {
                self.content
                    .as_deref()
                    .unwrap_or(&self.title)
                    .chars()
                    .take(240)
                    .collect()
            })
    }

    fn body(&self) -> String {
        let mut out = format!("# {}\n\n", self.title);
        if let Some(category) = &self.category {
            out.push_str(&format!("- Category: {category}\n"));
        }
        if let Some(level) = &self.evidence_level {
            out.push_str(&format!("- Evidence level: {level}\n"));
        }
        if let Some(source) = &self.source {
            out.push_str(&format!("- Source: {source}\n"));
        }
        if let Some(url) = &self.source_url {
            out.push_str(&format!("- Source URL: {url}\n"));
        }
        if let Some(basis) = &self.verification_basis {
            out.push_str(&format!("- Verification basis: {basis}\n"));
        }
        if let Some(tags) = &self.tags {
            out.push_str(&format!("- Tags: {tags}\n"));
        }
        out.push('\n');
        if self.record_kind == "negative" {
            if let Some(id) = &self.negates_knowledge_id {
                out.push_str(&format!("## Negates\n\n`{id}`\n\n"));
            }
            if let Some(id) = &self.neg_source_knowledge_id {
                out.push_str(&format!("## Source knowledge ID\n\n`{id}`\n\n"));
            }
            for (heading, value) in [
                ("Do not conclude", self.neg_do_not_conclude.as_deref()),
                (
                    "False-positive cases",
                    self.neg_false_positive_cases.as_deref(),
                ),
                ("Known failures", self.neg_known_failures.as_deref()),
                ("Stop conditions", self.neg_stop_conditions.as_deref()),
                (
                    "Not applicable when",
                    self.neg_not_applicable_when.as_deref(),
                ),
            ] {
                if let Some(value) = value.filter(|s| !s.trim().is_empty()) {
                    out.push_str(&format!("## {heading}\n\n{value}\n\n"));
                }
            }
            if let Some(content) = self.content.as_deref().filter(|s| !s.trim().is_empty()) {
                out.push_str(&format!("## Additional content\n\n{content}\n"));
            }
        } else if let Some(content) = self.content.as_deref().filter(|s| !s.trim().is_empty()) {
            out.push_str(content);
        } else if let Some(description) = &self.description {
            out.push_str(description);
        }
        out
    }
}

impl CuratedKnowledgeSource {
    /// Construct the source over a read-only platform database pool.
    pub fn new(pool: Pool) -> Self {
        Self { pool }
    }

    fn read_verified(&self) -> DbResult<Vec<KnowledgeRow>> {
        self.pool.with_conn(|c| {
            let mut stmt = c.prepare(
                "SELECT knowledge_id, record_kind, title, description, category, tags,
                        content, source, source_url, evidence_level, verification_basis,
                        negates_knowledge_id, neg_source_knowledge_id,
                        neg_do_not_conclude, neg_false_positive_cases, neg_known_failures,
                        neg_stop_conditions, neg_not_applicable_when
                 FROM knowledge
                 WHERE lifecycle='VERIFIED' AND governance='ACTIVE'
                 ORDER BY knowledge_id",
            )?;
            let rows = stmt
                .query_map([], |r| {
                    Ok(KnowledgeRow {
                        knowledge_id: r.get(0)?,
                        record_kind: r.get(1)?,
                        title: r.get(2)?,
                        description: r.get(3)?,
                        category: r.get(4)?,
                        tags: r.get(5)?,
                        content: r.get(6)?,
                        source: r.get(7)?,
                        source_url: r.get(8)?,
                        evidence_level: r.get(9)?,
                        verification_basis: r.get(10)?,
                        negates_knowledge_id: r.get(11)?,
                        neg_source_knowledge_id: r.get(12)?,
                        neg_do_not_conclude: r.get(13)?,
                        neg_false_positive_cases: r.get(14)?,
                        neg_known_failures: r.get(15)?,
                        neg_stop_conditions: r.get(16)?,
                        neg_not_applicable_when: r.get(17)?,
                    })
                })?
                .collect();
            rows
        })
    }

    fn read_one(&self, id: &str) -> DbResult<Option<KnowledgeRow>> {
        self.pool.with_conn(|c| {
            c.query_row(
                "SELECT knowledge_id, record_kind, title, description, category, tags,
                        content, source, source_url, evidence_level, verification_basis,
                        negates_knowledge_id, neg_source_knowledge_id,
                        neg_do_not_conclude, neg_false_positive_cases, neg_known_failures,
                        neg_stop_conditions, neg_not_applicable_when
                 FROM knowledge
                 WHERE knowledge_id=?1 AND lifecycle='VERIFIED' AND governance='ACTIVE'",
                [id],
                |r| {
                    Ok(KnowledgeRow {
                        knowledge_id: r.get(0)?,
                        record_kind: r.get(1)?,
                        title: r.get(2)?,
                        description: r.get(3)?,
                        category: r.get(4)?,
                        tags: r.get(5)?,
                        content: r.get(6)?,
                        source: r.get(7)?,
                        source_url: r.get(8)?,
                        evidence_level: r.get(9)?,
                        verification_basis: r.get(10)?,
                        negates_knowledge_id: r.get(11)?,
                        neg_source_knowledge_id: r.get(12)?,
                        neg_do_not_conclude: r.get(13)?,
                        neg_false_positive_cases: r.get(14)?,
                        neg_known_failures: r.get(15)?,
                        neg_stop_conditions: r.get(16)?,
                        neg_not_applicable_when: r.get(17)?,
                    })
                },
            )
            .optional()
        })
    }
}

impl ContextSource for CuratedKnowledgeSource {
    fn id(&self) -> &str {
        CURATED_KNOWLEDGE_SOURCE_ID
    }

    fn candidates(
        &self,
        q: &IntentQuery,
        limit: usize,
    ) -> Result<Vec<Candidate>, ContextBrokerError> {
        let tokens = query_tokens(&q.text_normalized);
        if tokens.is_empty() {
            return Ok(Vec::new());
        }
        let rows = self
            .read_verified()
            .map_err(|e| ContextBrokerError::Source {
                source: self.id().to_string(),
                reason: format!("read verified knowledge: {e}"),
            })?;
        let mut matches: Vec<(usize, KnowledgeRow)> = rows
            .into_iter()
            .filter_map(|row| {
                let haystack = row.searchable_text();
                let count = tokens
                    .iter()
                    .filter(|token| haystack.contains(&token.to_lowercase()))
                    .count();
                (count > 0).then_some((count, row))
            })
            .collect();
        matches.sort_by(|a, b| {
            b.0.cmp(&a.0)
                .then_with(|| a.1.knowledge_id.cmp(&b.1.knowledge_id))
        });
        Ok(matches
            .into_iter()
            .take(limit.max(1))
            .map(|(_, row)| {
                let summary = row.summary();
                Candidate {
                    item_id: format!("ckb:{}", row.knowledge_id),
                    kind: ContextItemKind::Knowledge,
                    layer: ContextLayer::L2,
                    title: row.title.clone(),
                    est_tokens: crate::est_tokens(&summary),
                    summary,
                    family: None,
                    governance: Some("ACTIVE".to_string()),
                    quality_grade: None,
                    reference: ContentRef {
                        source: CURATED_KNOWLEDGE_SOURCE_ID.to_string(),
                        ref_id: row.knowledge_id.clone(),
                        ref_version: None,
                        content_hash: None,
                        uri: Some(format!("knowledge://{}", row.knowledge_id)),
                    },
                }
            })
            .collect())
    }

    fn load(&self, r: &ContentRef) -> Result<Content, ContextBrokerError> {
        if r.source != CURATED_KNOWLEDGE_SOURCE_ID {
            return Err(ContextBrokerError::Source {
                source: self.id().to_string(),
                reason: format!("ref source {:?} is not curated-kb", r.source),
            });
        }
        let row = self
            .read_one(&r.ref_id)
            .map_err(|e| ContextBrokerError::Source {
                source: self.id().to_string(),
                reason: format!("read knowledge {}: {e}", r.ref_id),
            })?
            .ok_or_else(|| ContextBrokerError::Source {
                source: self.id().to_string(),
                reason: format!(
                    "knowledge {:?} is not VERIFIED/ACTIVE or does not exist",
                    r.ref_id
                ),
            })?;
        let body = row.body();
        Ok(Content {
            reference: ContentRef {
                uri: Some(format!("knowledge://{}", row.knowledge_id)),
                ..r.clone()
            },
            est_tokens: crate::est_tokens(&body),
            body,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use database::{DbPath, PoolConfig};
    use std::time::Duration;

    fn fixture() -> Pool {
        let pool = Pool::open(PoolConfig {
            path: DbPath::Memory,
            readonly: false,
            max_connections: 1,
            busy_timeout: Duration::from_secs(5),
        })
        .unwrap();
        pool.with_conn(|c| {
            c.execute_batch(
                "CREATE TABLE knowledge (
                    knowledge_id TEXT PRIMARY KEY, record_kind TEXT NOT NULL,
                    lifecycle TEXT NOT NULL, governance TEXT NOT NULL, title TEXT NOT NULL,
                    description TEXT, category TEXT, tags TEXT, content TEXT, source TEXT,
                    source_url TEXT, evidence_level TEXT, verification_basis TEXT,
                    negates_knowledge_id TEXT, neg_source_knowledge_id TEXT,
                    neg_do_not_conclude TEXT, neg_false_positive_cases TEXT,
                    neg_known_failures TEXT, neg_stop_conditions TEXT,
                    neg_not_applicable_when TEXT
                 );
                 INSERT INTO knowledge (knowledge_id,record_kind,lifecycle,governance,title,description,category,tags,content,source,evidence_level)
                 VALUES ('rule-file-stream','own','VERIFIED','ACTIVE','文件流端点绕过统一响应加密','下载与流式端点可能绕过 JSON 加密','info-leak','文件流,响应加密','完整的文件流判别方法与验证步骤','self','K3');
                 INSERT INTO knowledge (knowledge_id,record_kind,lifecycle,governance,title,description,category,tags,content)
                 VALUES ('rule-draft','own','CANDIDATE','ACTIVE','候选规则','不应自动可见','info-leak','draft','candidate body');
                 INSERT INTO knowledge (knowledge_id,record_kind,lifecycle,governance,title,negates_knowledge_id,neg_do_not_conclude,neg_false_positive_cases,neg_known_failures,neg_stop_conditions,neg_not_applicable_when)
                 VALUES ('neg-file-stream','negative','VERIFIED','ACTIVE','文件流误报边界','rule-file-stream','不能推出端到端加密已被绕过','SPA catch-all 返回 HTML','将文件流响应误判成敏感数据泄露','未取得真实文件内容时停止','接口只返回公开静态资源');",
            )
        })
        .unwrap();
        pool
    }

    #[test]
    fn exposes_only_verified_active_knowledge_and_loads_full_body() {
        let source = CuratedKnowledgeSource::new(fixture());
        let q = IntentQuery {
            text_normalized: "文件流 响应加密".to_string(),
            families: Vec::new(),
            knowledge_intents: Vec::new(),
            target_slots: Vec::new(),
            task_verbs: Vec::new(),
            constraints: Vec::new(),
            confidence: 0.8,
        };
        let candidates = source.candidates(&q, 10).unwrap();
        assert_eq!(candidates.len(), 2);
        assert!(candidates
            .iter()
            .all(|c| c.governance.as_deref() == Some("ACTIVE")));
        assert!(candidates
            .iter()
            .any(|c| c.reference.ref_id == "rule-file-stream"));
        assert!(candidates
            .iter()
            .any(|c| c.reference.ref_id == "neg-file-stream"));
        assert!(!candidates
            .iter()
            .any(|c| c.reference.ref_id == "rule-draft"));

        let own = source
            .load(
                &candidates
                    .iter()
                    .find(|c| c.reference.ref_id == "rule-file-stream")
                    .unwrap()
                    .reference,
            )
            .unwrap();
        assert!(own.body.contains("完整的文件流判别方法"));
        let negative = source
            .load(
                &candidates
                    .iter()
                    .find(|c| c.reference.ref_id == "neg-file-stream")
                    .unwrap()
                    .reference,
            )
            .unwrap();
        assert!(negative.body.contains("SPA catch-all 返回 HTML"));
        assert!(negative.body.contains("未取得真实文件内容时停止"));
    }

    #[test]
    fn load_rejects_wrong_source_and_unverified_ids() {
        let source = CuratedKnowledgeSource::new(fixture());
        let wrong = source.load(&ContentRef {
            source: "rtis".to_string(),
            ref_id: "rule-file-stream".to_string(),
            ref_version: None,
            content_hash: None,
            uri: None,
        });
        assert!(wrong.is_err());
        let draft = source.load(&ContentRef {
            source: CURATED_KNOWLEDGE_SOURCE_ID.to_string(),
            ref_id: "rule-draft".to_string(),
            ref_version: None,
            content_hash: None,
            uri: None,
        });
        assert!(draft.is_err());
    }
}
