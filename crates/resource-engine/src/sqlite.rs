//! platform 库（001_init.sql schema）上的 ResourceRepository 实现。

use crate::error::EngineError;
use crate::model::{
    CapabilityGap, ResolveFilters, ResolveQuery, ResolveResult, ResourceHit, RetrievalStats,
    WeightsConfig,
};
use crate::repository::ResourceRepository;
use crate::tokenizer::{expand_keywords, keyword_hits, tokenize};
use crate::weights::{KEYWORD_COVER_BOOST, KEYWORD_WEIGHT, QUALITY_WEIGHT};
use database::{rusqlite, DbResult, Pool};
use domain::{GovernanceStatus, LifecycleStatus, QualityGrade, Resource, ResourceId};
use std::cmp::Ordering;

const RESOURCE_COLUMNS: &str = "resource_id, identity, canonical_url, resource_type, name, \
     description, owner, repo_name, lifecycle, governance, source_status, quality_score, \
     trust_level, quality_final_score, quality_grade, quality_adoption_blocked, quality_reasons, \
     first_seen, last_seen, source_system, created_at, updated_at, schema_version";

/// SQLite 资源仓库。
#[derive(Clone)]
pub struct SqliteResourceRepository {
    pool: Pool,
}

impl SqliteResourceRepository {
    /// 用连接池构造仓库。
    pub fn new(pool: Pool) -> Self {
        Self { pool }
    }

    // ------------------------------------------------------------------
    // SK-3 候选映射（P6-11 第 3 步）：knowledge_source_entry 的
    // skill_candidate 行到 Resource 候选的受控映射写面。审核闸：只映射
    // review_status='APPROVED' 行（与 Search/Context 双闸同口径），
    // DISCOVERED/UNREVIEWED 不映射，REJECTED/DEPRECATED 永不映射。落点
    // resource 表：lifecycle=CANDIDATE、governance=ACTIVE、
    // resource_type='skill_candidate'，resource_id/identity =
    // `kse:<plugin_id>:<item_key>`（可回跳 knowledge_source_entry 源行）。
    // 幂等：INSERT OR IGNORE（既有行不覆盖），重跑只计 skipped_existing。
    // ------------------------------------------------------------------

    /// 把指定知识插件的 APPROVED skill_candidate 映射为 Resource 候选。
    pub fn promote_knowledge_candidates(
        &self,
        plugin_id: &str,
        limit: u64,
    ) -> DbResult<CandidateMappingReport> {
        type Row = (
            String,
            Option<String>,
            Option<String>,
            String,
            Option<String>,
            Option<String>,
            Option<String>,
        );
        let rows: Vec<Row> = self.pool.with_conn(|c| {
            let mut stmt = c.prepare(
                "SELECT item_key, title, description, review_status, canonical_url,
                        source_url, source_commit
                 FROM knowledge_source_entry
                 WHERE plugin_id = ?1 AND entry_kind = 'skill_candidate'
                 ORDER BY entry_id LIMIT ?2",
            )?;
            let it = stmt.query_map(rusqlite::params![plugin_id, limit], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, Option<String>>(1)?,
                    r.get::<_, Option<String>>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, Option<String>>(4)?,
                    r.get::<_, Option<String>>(5)?,
                    r.get::<_, Option<String>>(6)?,
                ))
            })?;
            it.collect::<Result<Vec<_>, _>>()
        })?;
        let mut report = CandidateMappingReport {
            plugin_id: plugin_id.to_string(),
            ..CandidateMappingReport::default()
        };
        report.scanned = rows.len() as u64;
        let now = domain::time::now_iso();
        for (item_key, title, description, review, canonical_url, source_url, _commit) in rows {
            if review != "APPROVED" {
                report.skipped_not_approved += 1;
                continue;
            }
            // resource.canonical_url NOT NULL：条目无 canonical_url 时回落
            // 插件 source_url（两者皆缺 = 缺溯源，拒映射不猜造）。
            let canonical = canonical_url
                .clone()
                .or_else(|| source_url.clone())
                .unwrap_or_default();
            if canonical.is_empty() {
                report.skipped_missing_url += 1;
                continue;
            }
            let resource_id = format!("kse:{plugin_id}:{item_key}");
            let inserted = self.pool.with_conn(|c| -> rusqlite::Result<bool> {
                let n = c.execute(
                    "INSERT OR IGNORE INTO resource
                     (resource_id, identity, canonical_url, resource_type, name,
                      description, lifecycle, governance, source_system,
                      first_seen, last_seen, created_at, updated_at)
                     VALUES (?1, ?1, ?2, 'skill_candidate', ?3, ?4,
                             'CANDIDATE', 'ACTIVE', 'knowledge-plugin',
                             ?5, ?5, ?5, ?5)",
                    rusqlite::params![
                        resource_id,
                        canonical,
                        title.unwrap_or_else(|| item_key.clone()),
                        description,
                        now,
                    ],
                )?;
                Ok::<bool, rusqlite::Error>(n > 0)
            })?;
            if inserted {
                report.mapped += 1;
                report.resource_ids.push(resource_id);
            } else {
                report.skipped_existing += 1;
            }
        }
        Ok(report)
    }

    fn load_all_where(&self, extra_where: &str, params: &[String]) -> DbResult<Vec<Resource>> {
        let sql = format!(
            "SELECT {RESOURCE_COLUMNS} FROM resource WHERE {extra_where} ORDER BY resource_id"
        );
        let conn = self.pool.get()?;
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(rusqlite::params_from_iter(params.iter()), row_to_resource)?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    /// 治理可见性 SQL 片段（值来自枚举，无注入面）。
    fn governance_clause(filters: &ResolveFilters) -> String {
        match filters.governance {
            Some(g) => format!("governance = '{}'", g.as_str()),
            // 与 eri EXCLUDED_STATUSES 对应：隔离/拒绝/归档不可见。
            None => "governance NOT IN ('QUARANTINED','REJECTED','ARCHIVED')".to_string(),
        }
    }

    // ------------------------------------------------------------------
    // SK-1 引用证据固化（P6-9 第 7 步；011 context_citation_evidence 表）：
    // 把「实际给模型的引用片段 + 内容哈希 + 来源位置」固化成只追加行，
    // 压缩/重试后凭 excerpt_sha256/content_sha256 复核。context-broker 是
    // 唯一调用方（装配 L2 装入与 L3 expand），本模块是该表唯一写面。
    // ------------------------------------------------------------------

    /// 固化一条引用证据（内容寻址幂等：同 manifest+item+event+excerpt 重试
    /// no-op；返回是否新插入）。
    pub fn insert_citation_evidence(&self, row: &CitationEvidenceRow) -> DbResult<bool> {
        let excerpt_sha = sha256_hex(row.excerpt.as_bytes());
        let evidence_id = format!(
            "ciev-{}",
            &sha256_hex(
                format!(
                    "{}|{}|{}|{}|{excerpt_sha}",
                    row.manifest_id, row.item_id, row.event_kind, row.ref_id
                )
                .as_bytes()
            )[..24]
        );
        let inserted = self.pool.with_conn(|c| -> rusqlite::Result<bool> {
            let n = c.execute(
                "INSERT OR IGNORE INTO context_citation_evidence (evidence_id, manifest_id,
                    item_id, event_kind, ref_source, ref_id, ref_version, excerpt,
                    excerpt_sha256, content_sha256, source_id, plugin_version, source_commit,
                    canonical_url, section_anchor, source_location_json, created_at)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17)",
                rusqlite::params![
                    evidence_id,
                    row.manifest_id,
                    row.item_id,
                    row.event_kind,
                    row.ref_source,
                    row.ref_id,
                    row.ref_version,
                    row.excerpt,
                    excerpt_sha,
                    row.content_sha256,
                    row.source_id,
                    row.plugin_version,
                    row.source_commit,
                    row.canonical_url,
                    row.section_anchor,
                    row.source_location_json,
                    domain::time::now_iso(),
                ],
            )?;
            Ok::<bool, rusqlite::Error>(n > 0)
        })?;
        Ok(inserted)
    }

    /// 按装配清单读证据行（对账与 manifest-show 消费）。
    pub fn list_citation_evidence(&self, manifest_id: &str) -> DbResult<Vec<CitationEvidenceRow>> {
        self.pool
            .with_conn(|c| -> rusqlite::Result<Vec<CitationEvidenceRow>> {
                let mut stmt = c.prepare(
                    "SELECT evidence_id, manifest_id, item_id, event_kind, ref_source, ref_id,
                        ref_version, excerpt, excerpt_sha256, content_sha256, source_id,
                        plugin_version, source_commit, canonical_url, section_anchor,
                        source_location_json, created_at
                 FROM context_citation_evidence WHERE manifest_id = ?1 ORDER BY rowid",
                )?;
                let rows = stmt
                    .query_map([manifest_id], row_to_citation_evidence)?
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(rows)
            })
    }

    /// 证据复核（压缩/重试后仍可验证的落点）：excerpt 与落库哈希互核；
    /// ref_source 为 `plugin:<id>` 词形时再对 knowledge_source_entry 源行
    /// content_sha256 互核（源行不存在按 unverifiable 报告，不伪装通过）。
    pub fn verify_citation_evidence(&self, evidence_id: &str) -> DbResult<CitationVerification> {
        let row = self
            .pool
            .with_conn(|c| -> rusqlite::Result<Option<CitationEvidenceRow>> {
                let mut stmt = c.prepare(
                    "SELECT evidence_id, manifest_id, item_id, event_kind, ref_source, ref_id,
                            ref_version, excerpt, excerpt_sha256, content_sha256, source_id,
                            plugin_version, source_commit, canonical_url, section_anchor,
                            source_location_json, created_at
                     FROM context_citation_evidence WHERE evidence_id = ?1",
                )?;
                let mut rows = stmt
                    .query_map([evidence_id], row_to_citation_evidence)?
                    .collect::<Result<Vec<_>, _>>()?;
                Ok::<_, rusqlite::Error>(rows.pop())
            })?
            .ok_or_else(|| {
                crate::error::EngineError::NotFound(format!("citation evidence {evidence_id:?}"))
            })
            .map_err(|e: crate::error::EngineError| database::DbError::Validation(e.to_string()))?;
        let excerpt_ok = sha256_hex(row.excerpt.as_bytes()) == row.excerpt_sha256;
        let mut source_ok: Option<bool> = None;
        if let Some(plugin_id) = row.ref_source.strip_prefix("plugin:").map(str::to_string) {
            // ref_id 词形：kse:<plugin_id>:<entry_kind>:<item_key>。
            let tail = row.ref_id.strip_prefix(&format!("kse:{plugin_id}:"));
            if let Some((kind, key)) = tail.and_then(|t| t.split_once(':')) {
                let source_sha: Option<String> =
                    self.pool
                        .with_conn(|c| -> rusqlite::Result<Option<String>> {
                            c.query_row(
                                "SELECT content_sha256 FROM knowledge_source_entry
                         WHERE plugin_id = ?1 AND entry_kind = ?2 AND item_key = ?3",
                                rusqlite::params![plugin_id, kind, key],
                                |r| r.get(0),
                            )
                            .map(Some)
                            .or_else(|e| match e {
                                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                                other => Err(other),
                            })
                        })?;
                // 源行存在且哈希一致 = verified；存在但哈希不一致 = drift；
                // 源行缺失（插件禁用不删行，通常为误配 ref）= unverifiable。
                source_ok = source_sha
                    .as_ref()
                    .map(|s| *s == row.content_sha256.clone().unwrap_or_default());
            }
        }
        Ok(CitationVerification {
            evidence_id: row.evidence_id,
            excerpt_ok,
            source_ok,
        })
    }
}

/// 引用证据行（011 context_citation_evidence；只追加）。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CitationEvidenceRow {
    /// 证据 id（ciev-<sha24>，内容寻址）。
    pub evidence_id: String,
    /// 装配清单 id。
    pub manifest_id: String,
    /// 装配条目 id。
    pub item_id: String,
    /// assembled（L2 装入）| expanded（L3 展开）。
    pub event_kind: String,
    /// 引用来源（plugin:<id> 词形等）。
    pub ref_source: String,
    /// 引用对象（kse:<plugin>:<kind>:<key>）。
    pub ref_id: String,
    /// 引用版本（插件版本）。
    pub ref_version: Option<String>,
    /// 实际给模型的引用片段（expand 场景头部 4KiB 截断保存，整段哈希另存）。
    pub excerpt: String,
    /// 片段哈希（写入时计算，复核互核）。
    pub excerpt_sha256: String,
    /// 源侧完整内容哈希（与来源行互核）。
    pub content_sha256: Option<String>,
    /// Source ID（= plugin_id）。
    pub source_id: String,
    /// Plugin Version。
    pub plugin_version: Option<String>,
    /// 来源快照 Commit。
    pub source_commit: Option<String>,
    /// 原始 URL。
    pub canonical_url: Option<String>,
    /// 章节锚点。
    pub section_anchor: Option<String>,
    /// 源位置 JSON（file/line）。
    pub source_location_json: String,
    /// 固化时刻（ISO8601 UTC）。
    pub created_at: String,
}

/// 证据复核结论（压缩/重试后的可验证性面）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CitationVerification {
    /// 证据 id。
    pub evidence_id: String,
    /// excerpt 与 excerpt_sha256 互核通过。
    pub excerpt_ok: bool,
    /// 源侧互核：Some(true)=verified / Some(false)=内容漂移 /
    /// None=源行缺失或 ref 词形不匹配（unverifiable，不伪装通过）。
    pub source_ok: Option<bool>,
}

/// skill_candidate 到 Resource 候选的映射报告（SK-3 受控写面）。
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct CandidateMappingReport {
    /// 来源插件 id。
    pub plugin_id: String,
    /// 扫描的 skill_candidate 行数（含未过审）。
    pub scanned: u64,
    /// 新插入的 Resource 候选数。
    pub mapped: u64,
    /// 已存在（幂等跳过）。
    pub skipped_existing: u64,
    /// 未过审核闸（非 APPROVED）。
    pub skipped_not_approved: u64,
    /// 缺 URL 溯源（canonical_url 与 source_url 皆空）。
    pub skipped_missing_url: u64,
    /// 本次新映射的 resource_id 列表。
    pub resource_ids: Vec<String>,
}

fn row_to_citation_evidence(r: &rusqlite::Row<'_>) -> rusqlite::Result<CitationEvidenceRow> {
    Ok(CitationEvidenceRow {
        evidence_id: r.get(0)?,
        manifest_id: r.get(1)?,
        item_id: r.get(2)?,
        event_kind: r.get(3)?,
        ref_source: r.get(4)?,
        ref_id: r.get(5)?,
        ref_version: r.get(6)?,
        excerpt: r.get(7)?,
        excerpt_sha256: r.get(8)?,
        content_sha256: r.get(9)?,
        source_id: r.get(10)?,
        plugin_version: r.get(11)?,
        source_commit: r.get(12)?,
        canonical_url: r.get(13)?,
        section_anchor: r.get(14)?,
        source_location_json: r.get(15)?,
        created_at: r.get(16)?,
    })
}

/// sha256 hex（tool-registry::hash 复用，与 workspace 哈希口径一致）。
fn sha256_hex(data: &[u8]) -> String {
    tool_registry::hash::sha256_hex(data)
}

impl ResourceRepository for SqliteResourceRepository {
    fn get(&self, id: &ResourceId) -> Result<Option<Resource>, EngineError> {
        let sql = format!("SELECT {RESOURCE_COLUMNS} FROM resource WHERE resource_id = ?1");
        let conn = self.pool.get()?;
        let mut stmt = conn.prepare(&sql)?;
        let mut rows = stmt.query_map([id.as_str()], row_to_resource)?;
        match rows.next() {
            Some(Ok(r)) => Ok(Some(r)),
            Some(Err(e)) => Err(e.into()),
            None => Ok(None),
        }
    }

    fn count(&self) -> Result<i64, EngineError> {
        let conn = self.pool.get()?;
        let n = conn.query_row("SELECT COUNT(*) FROM resource", [], |r| r.get(0))?;
        Ok(n)
    }

    fn resolve(&self, query: &ResolveQuery) -> Result<ResolveResult, EngineError> {
        let kw = expand_keywords(&query.keywords);

        // 1) SQL 预筛候选（治理可见性 + 过滤条件 + LIKE 命中，eri 口径）
        let mut where_clause = Self::governance_clause(&query.filters);
        let mut params: Vec<String> = Vec::new();
        if let Some(rt) = &query.filters.resource_type {
            where_clause.push_str(" AND resource_type = ?");
            params.push(rt.clone());
        }
        if let Some(lc) = &query.filters.lifecycle {
            where_clause.push_str(" AND lifecycle = ?");
            params.push(lc.as_str().to_string());
        }
        if !kw.is_empty() {
            let like_groups: Vec<String> = kw
                .iter()
                .map(|_| "(name LIKE ? OR description LIKE ? OR canonical_url LIKE ?)".to_string())
                .collect();
            where_clause.push_str(&format!(" AND ({})", like_groups.join(" OR ")));
            for k in &kw {
                let pat = format!("%{k}%");
                params.push(pat.clone());
                params.push(pat.clone());
                params.push(pat);
            }
        }
        let candidates = self.load_all_where(&where_clause, &params)?;

        // 2) 词元级打分（keyword 60% + quality 40%，权重口径见 weights.rs）
        let mut scored: Vec<ResourceHit> = Vec::new();
        let mut filtered = 0usize;
        for c in &candidates {
            // eri 口径：blob = name + description（+ topics/language，platform 库无此二列）。
            // canonical_url 只参与 SQL LIKE 预筛（上方），不参与词元命中，与 eri 一致。
            let blob = format!("{} {}", c.name, c.description.as_deref().unwrap_or(""));
            let tokens = tokenize(&blob);
            let hits = keyword_hits(&kw, &tokens);
            if hits.is_empty() {
                // eri 口径：无 keyword 证据且无 profile/skill 证据的候选不进入打分。
                filtered += 1;
                continue;
            }
            let cov = hits.len() as f64 / kw.len().max(1) as f64;
            let mut acc = KEYWORD_WEIGHT * (cov * KEYWORD_COVER_BOOST).min(1.0);
            let mut wsum = KEYWORD_WEIGHT;
            let mut reasons = vec![format!(
                "keyword hits: [{}]",
                hits.iter().take(5).copied().collect::<Vec<_>>().join(", ")
            )];
            let quality_used = c.quality_final_score.is_some();
            if let Some(q) = c.quality_final_score {
                acc += QUALITY_WEIGHT * (q / 100.0).clamp(0.0, 1.0);
                wsum += QUALITY_WEIGHT;
                let grade = c
                    .quality_grade
                    .map(|g| g.to_string())
                    .unwrap_or_else(|| "None".to_string());
                // {:?} 使 58.0 打印为 "58.0"，与 python str(float) 一致。
                reasons.push(format!("quality {q:?} ({grade})"));
            }
            if !quality_used {
                reasons.push("no evidence for: quality (权重已按证据归一)".to_string());
            }
            let score = (acc / wsum * 10_000.0).round() / 10_000.0;
            scored.push(ResourceHit {
                resource_id: c.resource_id.to_string(),
                name: c.name.clone(),
                canonical_url: c.canonical_url.clone(),
                description: c.description.clone(),
                resource_type: c.resource_type.clone(),
                category_matches: Vec::new(),
                keyword_matches: hits.iter().copied().map(String::from).collect(),
                skill_relationships: Vec::new(),
                workflow_relationships: Vec::new(),
                quality: c.quality_final_score,
                grade: c.quality_grade.map(|g| g.to_string()),
                // platform 库无 license / maintenance 元数据列：置 UNKNOWN 并附注意事项
                //（eri 对 license 证据缺失的资源同样给出该 note，行为对齐）。
                license_status: "UNKNOWN".to_string(),
                maintenance_status: "UNKNOWN".to_string(),
                adoptable_as_dependency: false,
                license_note: Some("license=UNKNOWN: 仅供参考，不可直接采用为依赖".to_string()),
                relevance_score: score,
                relevance_reasons: reasons,
            });
        }

        // 3) 排序（与 eri 同 tie-break：分数降序、质量降序、ID 升序）+ 截断
        scored.sort_by(|a, b| {
            b.relevance_score
                .partial_cmp(&a.relevance_score)
                .unwrap_or(Ordering::Equal)
                .then_with(|| {
                    b.quality
                        .unwrap_or(0.0)
                        .partial_cmp(&a.quality.unwrap_or(0.0))
                        .unwrap_or(Ordering::Equal)
                })
                .then_with(|| a.resource_id.cmp(&b.resource_id))
        });
        let returned = scored.len().min(query.limit);
        let top: Vec<ResourceHit> = scored.into_iter().take(query.limit).collect();

        // 4) 能力缺口（platform 库无 skill_mappings：全部 top 命中即缺口候选，eri 口径）
        let capability_gaps: Vec<CapabilityGap> = top
            .iter()
            .take(5)
            .map(|h| CapabilityGap {
                resource_id: h.resource_id.clone(),
                reason: "任务相关度高但未映射到任何现有 skill，可能为能力缺口候选".to_string(),
            })
            .collect();

        let search_status = if top.is_empty() { "no_match" } else { "ok" };
        Ok(ResolveResult {
            query_id: make_query_id(&query.task_id, &kw),
            task_id: query.task_id.clone(),
            retrieval_stats: RetrievalStats {
                candidates: candidates.len(),
                filtered,
                returned,
                cache_hits: 0,
            },
            matched_resources: top,
            already_covered_by_internal: Vec::new(),
            capability_gaps,
            search_status: search_status.to_string(),
            weights_config: WeightsConfig {
                keyword: KEYWORD_WEIGHT,
                quality: QUALITY_WEIGHT,
            },
        })
    }
}

fn row_to_resource(row: &rusqlite::Row<'_>) -> rusqlite::Result<Resource> {
    let parse_grade = |v: Option<String>| -> rusqlite::Result<Option<QualityGrade>> {
        match v {
            None => Ok(None),
            Some(s) => QualityGrade::parse(&s).map(Some).map_err(|e| {
                rusqlite::Error::FromSqlConversionFailure(
                    0,
                    rusqlite::types::Type::Text,
                    Box::new(e),
                )
            }),
        }
    };
    Ok(Resource {
        resource_id: ResourceId::try_new(row.get::<_, String>(0)?).map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
        })?,
        identity: row.get(1)?,
        canonical_url: row.get(2)?,
        resource_type: row.get(3)?,
        name: row.get(4)?,
        description: row.get(5)?,
        owner: row.get(6)?,
        repo_name: row.get(7)?,
        lifecycle: LifecycleStatus::parse(&row.get::<_, String>(8)?).map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(8, rusqlite::types::Type::Text, Box::new(e))
        })?,
        governance: GovernanceStatus::parse(&row.get::<_, String>(9)?).map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(9, rusqlite::types::Type::Text, Box::new(e))
        })?,
        source_status: row.get(10)?,
        quality_score: row.get(11)?,
        trust_level: parse_grade(row.get(12)?)?,
        quality_final_score: row.get(13)?,
        quality_grade: parse_grade(row.get(14)?)?,
        quality_adoption_blocked: row.get::<_, Option<i64>>(15)?.map(|v| v != 0),
        quality_reasons: row.get(16)?,
        first_seen: row.get(17)?,
        last_seen: row.get(18)?,
        source_system: row.get(19)?,
        created_at: row.get(20)?,
        updated_at: row.get(21)?,
        schema_version: row.get(22)?,
    })
}

/// eri 口径 query_id：`erq-<紧凑时间戳20位>-<输入哈希前6位hex>`。
fn make_query_id(task_id: &str, kw: &[String]) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325; // FNV-1a offset basis
    let mut feed = |bytes: &[u8]| {
        for b in bytes {
            hash ^= u64::from(*b);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3); // FNV prime
        }
    };
    feed(task_id.as_bytes());
    feed(&[0x1f]);
    feed(kw.join("\u{1}").as_bytes());
    let hex = format!("{hash:016x}");
    format!("erq-{}-{}", domain::time::now_compact_micros(), &hex[..6])
}

#[cfg(test)]
mod tests {
    use super::*;
    use database::{DbPath, PoolConfig};
    use std::time::Duration;

    /// 建内存库 + audit_event/resource 最小 schema + 3 条 fixture。
    fn repo_with_fixtures() -> SqliteResourceRepository {
        let pool = Pool::open(PoolConfig {
            path: DbPath::Memory,
            readonly: false,
            max_connections: 1,
            busy_timeout: Duration::from_secs(1),
        })
        .unwrap();
        pool.with_conn(|c| {
            c.execute_batch(
                "CREATE TABLE resource (
                   resource_id TEXT PRIMARY KEY, identity TEXT UNIQUE, canonical_url TEXT NOT NULL,
                   resource_type TEXT NOT NULL, name TEXT NOT NULL, description TEXT,
                   owner TEXT, repo_name TEXT, lifecycle TEXT NOT NULL DEFAULT 'DISCOVERED',
                   governance TEXT NOT NULL DEFAULT 'ACTIVE', source_status TEXT,
                   quality_score REAL, trust_level TEXT,
                   quality_final_score REAL, quality_grade TEXT, quality_adoption_blocked INTEGER,
                   quality_reasons TEXT, first_seen TEXT, last_seen TEXT,
                   source_system TEXT NOT NULL, created_at TEXT NOT NULL, updated_at TEXT NOT NULL,
                   schema_version INTEGER NOT NULL DEFAULT 1);
                 INSERT INTO resource (resource_id, canonical_url, resource_type, name, description,
                   lifecycle, governance, quality_final_score, quality_grade, source_system,
                   created_at, updated_at) VALUES
                 ('gh:a/fuzz-tool', 'https://github.com/a/fuzz-tool', 'github_repo', 'Fuzz Tool',
                   'a fuzzing tool for payloads', 'INDEXED', 'ACTIVE', 80.0, 'B', 'eri-catalog', 't', 't'),
                 ('gh:b/fuzzer-lab', 'https://github.com/b/fuzzer-lab', 'github_repo', 'Fuzzer Lab',
                   'fuzzing lab', 'INDEXED', 'ACTIVE', 60.0, 'C', 'eri-catalog', 't', 't'),
                 ('gh:c/other', 'https://github.com/c/other', 'github_repo', 'Other',
                   'unrelated', 'INDEXED', 'QUARANTINED', 95.0, 'A', 'eri-catalog', 't', 't');",
            )
        })
        .unwrap();
        SqliteResourceRepository::new(pool)
    }

    #[test]
    fn resolve_orders_by_keyword_then_quality() {
        let repo = repo_with_fixtures();
        let q = ResolveQuery::new(vec!["fuzzing".to_string()]);
        let r = repo.resolve(&q).unwrap();
        assert_eq!(r.search_status, "ok");
        // fuzz-tool 命中 fuzzing 整词 + 高质量；fuzzer-lab 靠复数命中。
        assert_eq!(r.matched_resources[0].resource_id, "gh:a/fuzz-tool");
        assert!(r
            .matched_resources
            .iter()
            .any(|h| h.resource_id == "gh:b/fuzzer-lab"));
        // 隔离资源不可见。
        assert!(!r
            .matched_resources
            .iter()
            .any(|h| h.resource_id == "gh:c/other"));
        assert_eq!(r.weights_config.keyword, 0.6);
        assert_eq!(r.weights_config.quality, 0.4);
        assert!(r.query_id.starts_with("erq-"));
        assert_eq!(r.retrieval_stats.cache_hits, 0);
    }

    #[test]
    fn resolve_scores_missing_quality_normalized() {
        let pool = Pool::open(PoolConfig {
            path: DbPath::Memory,
            readonly: false,
            max_connections: 1,
            busy_timeout: Duration::from_secs(1),
        })
        .unwrap();
        pool.with_conn(|c| {
            c.execute_batch(
                "CREATE TABLE resource (
                   resource_id TEXT PRIMARY KEY, identity TEXT, canonical_url TEXT NOT NULL,
                   resource_type TEXT NOT NULL, name TEXT NOT NULL, description TEXT,
                   owner TEXT, repo_name TEXT, lifecycle TEXT NOT NULL, governance TEXT NOT NULL,
                   source_status TEXT, quality_score REAL, trust_level TEXT,
                   quality_final_score REAL, quality_grade TEXT, quality_adoption_blocked INTEGER,
                   quality_reasons TEXT, first_seen TEXT, last_seen TEXT,
                   source_system TEXT NOT NULL, created_at TEXT NOT NULL, updated_at TEXT NOT NULL,
                   schema_version INTEGER NOT NULL DEFAULT 1);
                 INSERT INTO resource (resource_id, canonical_url, resource_type, name, lifecycle,
                   governance, source_system, created_at, updated_at) VALUES
                 ('gh:x/noq', 'https://github.com/x/noq', 'github_repo', 'Android thing',
                   'INDEXED', 'ACTIVE', 's', 't', 't');",
            )
        })
        .unwrap();
        let repo = SqliteResourceRepository::new(pool);
        let q = ResolveQuery::new(vec!["android".to_string()]);
        let r = repo.resolve(&q).unwrap();
        assert_eq!(r.matched_resources.len(), 1);
        let hit = &r.matched_resources[0];
        // eri 口径：android 展开为 [android, apk, mobile]，仅 android 命中，
        // cov=1/3，值 min(1, 1/3*1.5)=0.5；quality 缺证归一到 keyword 单轴 -> 0.5。
        assert_eq!(
            hit.relevance_score, 0.5,
            "keyword-only score with coverage factor"
        );
        assert!(hit
            .relevance_reasons
            .iter()
            .any(|s| s.contains("no evidence for: quality")));
        assert_eq!(hit.license_status, "UNKNOWN");
        assert!(!hit.adoptable_as_dependency);
    }

    #[test]
    fn filters_narrow_candidates() {
        let repo = repo_with_fixtures();
        let mut q = ResolveQuery::new(vec!["fuzzing".to_string()]);
        q.filters.resource_type = Some("github_repo".to_string());
        q.filters.lifecycle = Some(LifecycleStatus::Indexed);
        let r = repo.resolve(&q).unwrap();
        assert!(!r.matched_resources.is_empty());

        let mut q2 = ResolveQuery::new(vec!["fuzzing".to_string()]);
        q2.filters.resource_type = Some("no_such_type".to_string());
        let r2 = repo.resolve(&q2).unwrap();
        assert_eq!(r2.search_status, "no_match");
        assert_eq!(r2.retrieval_stats.candidates, 0);
    }

    #[test]
    fn get_and_count() {
        let repo = repo_with_fixtures();
        assert_eq!(repo.count().unwrap(), 3);
        let id = ResourceId::try_new("gh:a/fuzz-tool").unwrap();
        let r = repo.get(&id).unwrap().unwrap();
        assert_eq!(r.name, "Fuzz Tool");
        assert_eq!(r.quality_grade, Some(QualityGrade::B));
        let missing = ResourceId::try_new("url:deadbeef00000000").unwrap();
        assert!(repo.get(&missing).unwrap().is_none());
    }

    #[test]
    fn query_id_shape_matches_eri() {
        let qid = make_query_id("T1", &["a".to_string()]);
        assert!(qid.starts_with("erq-"));
        let tail = qid.rsplit('-').next().unwrap();
        assert_eq!(tail.len(), 6);
        let ts = qid.trim_start_matches("erq-").split('-').next().unwrap();
        assert_eq!(ts.len(), 20);
    }

    // ------------------------------------------------------------------ SK-3

    /// SK-3 候选映射 fixture：resource + knowledge_source_entry 最小 schema。
    fn repo_with_kse_fixtures() -> SqliteResourceRepository {
        let pool = Pool::open(PoolConfig {
            path: DbPath::Memory,
            readonly: false,
            max_connections: 1,
            busy_timeout: Duration::from_secs(1),
        })
        .unwrap();
        pool.with_conn(|c| {
            c.execute_batch(
                "CREATE TABLE resource (
                   resource_id TEXT PRIMARY KEY, identity TEXT UNIQUE, canonical_url TEXT NOT NULL,
                   resource_type TEXT NOT NULL, name TEXT NOT NULL, description TEXT,
                   owner TEXT, repo_name TEXT, lifecycle TEXT NOT NULL DEFAULT 'DISCOVERED',
                   governance TEXT NOT NULL DEFAULT 'ACTIVE', source_status TEXT,
                   quality_score REAL, trust_level TEXT,
                   quality_final_score REAL, quality_grade TEXT, quality_adoption_blocked INTEGER,
                   quality_reasons TEXT, first_seen TEXT, last_seen TEXT,
                   source_system TEXT NOT NULL, created_at TEXT NOT NULL, updated_at TEXT NOT NULL,
                   schema_version INTEGER NOT NULL DEFAULT 1);
                 CREATE TABLE knowledge_source_entry (
                   entry_id INTEGER PRIMARY KEY AUTOINCREMENT,
                   plugin_id TEXT NOT NULL, plugin_version TEXT NOT NULL, corpus_id TEXT NOT NULL,
                   entry_kind TEXT NOT NULL, item_key TEXT NOT NULL, title TEXT NOT NULL,
                   description TEXT, body TEXT NOT NULL DEFAULT '', body_bytes INTEGER NOT NULL DEFAULT 0,
                   content_sha256 TEXT NOT NULL DEFAULT '', metadata_sha256 TEXT NOT NULL DEFAULT '',
                   canonical_url TEXT, source_url TEXT NOT NULL DEFAULT '',
                   source_commit TEXT, review_status TEXT NOT NULL DEFAULT 'DISCOVERED');
                 INSERT INTO knowledge_source_entry
                   (plugin_id, plugin_version, corpus_id, entry_kind, item_key, title,
                    description, review_status, canonical_url, source_url) VALUES
                   ('knowledge.sk3-skill-pack', '0.1.0', 'sk3', 'skill_candidate',
                    'skills/alpha/SKILL.md', 'static-analyzer-alpha', '静态分析候选',
                    'APPROVED', NULL, 'https://github.com/fixture/skill-pack'),
                   ('knowledge.sk3-skill-pack', '0.1.0', 'sk3', 'skill_candidate',
                    'skills/beta/SKILL.md', 'dependency-checker-beta', '依赖检查候选',
                    'DISCOVERED', NULL, 'https://github.com/fixture/skill-pack'),
                   ('knowledge.sk3-skill-pack', '0.1.0', 'sk3', 'skill_candidate',
                    'skills/gamma/SKILL.md', 'rejected-candidate', '已拒绝候选',
                    'REJECTED', NULL, 'https://github.com/fixture/skill-pack');",
            )
        })
        .unwrap();
        SqliteResourceRepository::new(pool)
    }

    #[test]
    fn promote_candidates_maps_only_approved() {
        let repo = repo_with_kse_fixtures();
        let report = repo
            .promote_knowledge_candidates("knowledge.sk3-skill-pack", 100)
            .unwrap();
        assert_eq!(report.scanned, 3);
        assert_eq!(report.mapped, 1, "只映射 APPROVED 行（审核闸）");
        assert_eq!(report.skipped_not_approved, 2);
        assert_eq!(
            report.resource_ids,
            vec!["kse:knowledge.sk3-skill-pack:skills/alpha/SKILL.md"]
        );
        // 落点形态：lifecycle=CANDIDATE / governance=ACTIVE / type=skill_candidate
        let (lifecycle, governance, rtype, canonical): (String, String, String, String) = repo
            .pool
            .with_conn(|c| {
                c.query_row(
                    "SELECT lifecycle, governance, resource_type, canonical_url
                     FROM resource WHERE resource_id = ?1",
                    ["kse:knowledge.sk3-skill-pack:skills/alpha/SKILL.md"],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
                )
            })
            .unwrap();
        assert_eq!(
            (lifecycle.as_str(), governance.as_str(), rtype.as_str()),
            ("CANDIDATE", "ACTIVE", "skill_candidate")
        );
        assert_eq!(
            canonical, "https://github.com/fixture/skill-pack",
            "条目无 canonical_url 时回落 source_url"
        );
        // 幂等二跑：skipped_existing，不覆盖既有行
        let report2 = repo
            .promote_knowledge_candidates("knowledge.sk3-skill-pack", 100)
            .unwrap();
        assert_eq!(report2.mapped, 0);
        assert_eq!(report2.skipped_existing, 1);
        assert_eq!(report2.skipped_not_approved, 2);
        let total: i64 = repo
            .pool
            .with_conn(|c| c.query_row("SELECT COUNT(*) FROM resource", [], |r| r.get(0)))
            .unwrap();
        assert_eq!(total, 1);
    }

    #[test]
    fn promote_candidates_missing_url_is_skipped() {
        let repo = repo_with_kse_fixtures();
        repo.pool
            .with_conn(|c| {
                c.execute(
                    "INSERT INTO knowledge_source_entry
                     (plugin_id, plugin_version, corpus_id, entry_kind, item_key, title,
                      review_status, canonical_url, source_url)
                     VALUES ('knowledge.sk3-skill-pack', '0.1.0', 'sk3', 'skill_candidate',
                             'skills/delta/SKILL.md', 'no-url-candidate', 'APPROVED', '', '')",
                    [],
                )
            })
            .unwrap();
        let report = repo
            .promote_knowledge_candidates("knowledge.sk3-skill-pack", 100)
            .unwrap();
        assert_eq!(report.mapped, 1); // alpha 仍映射
        assert_eq!(report.skipped_missing_url, 1, "缺 URL 溯源拒映射不猜造");
    }

    #[test]
    fn promote_candidates_limit_bounds_scan() {
        let repo = repo_with_kse_fixtures();
        let report = repo
            .promote_knowledge_candidates("knowledge.sk3-skill-pack", 1)
            .unwrap();
        assert_eq!(report.scanned, 1);
        assert_eq!(report.mapped, 1);
    }
}
