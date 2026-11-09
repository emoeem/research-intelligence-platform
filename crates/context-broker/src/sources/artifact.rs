//! artifact 只读引用源（设计稿 CONTEXT-BROKER-DESIGN §4.3 候选第三类 +
//! §10 sources/artifact.rs；Phase 2/3 产物存储，005 迁移 artifact 表）。
//!
//! 纪律（设计稿 §5.3 第 7 条）：工具输出永不预装，Run 产出的正文以
//! Artifact 引用进上下文（计划 §14:794 第三条验收）；本源只读（SELECT），
//! 候选进装配时以 ref-only 形态挂 l3_refs/citations（引用占极小 ref 预算），
//! 全文内容经 `load` 按需展开（P4-4 expand 记账）。

use crate::error::ContextBrokerError;
/// artifact 行查询投影（clippy type_complexity 拆名）。
type ArtifactRowTuple = (String, String, Option<String>, String, String, i64, String);
use crate::source::{Candidate, Content, ContextSource};
use crate::{ContentRef, ContextItemKind, ContextLayer, IntentQuery};
use database::Pool;

/// 源 id（Manifest item source 词表成员，设计稿 §6.1 artifact-store）。
pub const ARTIFACT_SOURCE_ID: &str = "artifact-store";

/// artifact 只读源（进程内持只读用途连接池；写面属 Tool Gateway）。
pub struct ArtifactSource {
    pool: Pool,
}

impl ArtifactSource {
    /// 用连接池构造（开发库副本或测试内存库；生产库不经本源）。
    pub fn new(pool: Pool) -> Self {
        Self { pool }
    }
}

impl ContextSource for ArtifactSource {
    fn id(&self) -> &str {
        ARTIFACT_SOURCE_ID
    }

    fn candidates(
        &self,
        _q: &IntentQuery,
        limit: usize,
    ) -> Result<Vec<Candidate>, ContextBrokerError> {
        // 最近登记优先（证据新鲜度；关键词过滤随 P4-6 校准后接 run/task 维度）。
        let rows: Vec<ArtifactRowTuple> = self
            .pool
            .with_conn(|c| {
                let mut stmt = c.prepare(
                    "SELECT artifact_id, run_id, role, uri, sha256, size_bytes, mime_type
                     FROM artifact ORDER BY rowid DESC LIMIT ?1",
                )?;
                let mapped = stmt.query_map([limit.max(1) as i64], |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, Option<String>>(2)?,
                        r.get::<_, String>(3)?,
                        r.get::<_, String>(4)?,
                        r.get::<_, i64>(5)?,
                        r.get::<_, String>(6)?,
                    ))
                })?;
                mapped.collect::<Result<Vec<_>, database::rusqlite::Error>>()
            })
            .map_err(|e| ContextBrokerError::Source {
                source: ARTIFACT_SOURCE_ID.to_string(),
                reason: e.to_string(),
            })?;
        Ok(rows
            .into_iter()
            .map(
                |(artifact_id, run_id, role, uri, sha256, size_bytes, mime_type)| {
                    let summary = format!(
                    "artifact {artifact_id}（run {run_id}，role {}，{mime_type}，{size_bytes}B）",
                    role.as_deref().unwrap_or("-")
                );
                    Candidate {
                        item_id: format!("artifact:{artifact_id}"),
                        kind: ContextItemKind::Artifact,
                        layer: ContextLayer::L3,
                        title: artifact_id.clone(),
                        summary,
                        family: None,
                        governance: None,
                        quality_grade: None,
                        reference: ContentRef {
                            source: ARTIFACT_SOURCE_ID.to_string(),
                            ref_id: format!("artifact:{artifact_id}"),
                            ref_version: None,
                            content_hash: Some(sha256),
                            uri: Some(uri),
                        },
                        est_tokens: crate::est_tokens(&mime_type),
                    }
                },
            )
            .collect())
    }

    fn load(&self, r: &ContentRef) -> Result<Content, ContextBrokerError> {
        if r.source != ARTIFACT_SOURCE_ID {
            return Err(ContextBrokerError::Source {
                source: ARTIFACT_SOURCE_ID.to_string(),
                reason: format!("ref source {:?} is not artifact-store", r.source),
            });
        }
        let artifact_id =
            r.ref_id
                .strip_prefix("artifact:")
                .ok_or_else(|| ContextBrokerError::Source {
                    source: ARTIFACT_SOURCE_ID.to_string(),
                    reason: format!("ref_id {:?} is not artifact:<id>", r.ref_id),
                })?;
        let row: Option<(String, Option<String>, String, String, i64, String)> = self
            .pool
            .with_conn(|c| {
                c.query_row(
                    "SELECT run_id, role, uri, sha256, size_bytes, mime_type
                     FROM artifact WHERE artifact_id = ?1",
                    [artifact_id],
                    |r| {
                        Ok((
                            r.get::<_, String>(0)?,
                            r.get::<_, Option<String>>(1)?,
                            r.get::<_, String>(2)?,
                            r.get::<_, String>(3)?,
                            r.get::<_, i64>(4)?,
                            r.get::<_, String>(5)?,
                        ))
                    },
                )
                .map(Some)
                .or_else(|e| match e {
                    database::rusqlite::Error::QueryReturnedNoRows => Ok(None),
                    other => Err(other),
                })
            })
            .map_err(|e| ContextBrokerError::Source {
                source: ARTIFACT_SOURCE_ID.to_string(),
                reason: e.to_string(),
            })?;
        let Some((run_id, role, uri, sha256, size_bytes, mime_type)) = row else {
            return Err(ContextBrokerError::Source {
                source: ARTIFACT_SOURCE_ID.to_string(),
                reason: format!("artifact {artifact_id:?} not found"),
            });
        };
        let body = serde_json::json!({
            "artifact_id": artifact_id,
            "run_id": run_id,
            "role": role,
            "uri": uri,
            "sha256": sha256,
            "size_bytes": size_bytes,
            "mime_type": mime_type,
        })
        .to_string();
        Ok(Content {
            reference: ContentRef {
                uri: Some(uri),
                content_hash: Some(sha256),
                ..r.clone()
            },
            est_tokens: crate::est_tokens(&body),
            body,
        })
    }
}
