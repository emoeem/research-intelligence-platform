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
}
