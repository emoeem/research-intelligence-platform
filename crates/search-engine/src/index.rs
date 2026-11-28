//! FTS5 派生索引层（全部表 `search_index_` 前缀，可重建）。
//!
//! 落地形态（任务口径二选一）：**独立索引库**（`platform-search.db`），而非在
//! platform-dryrun.db 内建派生表。理由：
//! 1. platform-dryrun.db 是 M1 ETL 的对账产物（verify.py 100% 红线），在其内
//!    建表会改变 schema 并复杂化复核；独立索引库保持 M1 基线不变。
//! 2. §8.2.3 可重建纪律的最强形态：整个索引库可删可重灌，写面隔离。
//!
//! 数据源（只读）：
//! - platform 库：`resource` / `resource_source`（基础资源、质量、来源章节）
//! - catalog 库：`github_metadata`（topics / language / license / maintenance）
//! - platform 库可选：经审核且插件版本可见的 `knowledge_source_entry`
//! - RTIS 可选：仅 Overlay 明确审核的 POC 元数据；不读取正文、usage 或 verified_note
//!
//! FTS5 形态：contentless（content=''）虚拟表，bm25 可用，不支持增量删除，
//! 与"整库重建"策略一致（更新 = 重建，成本 O(文档数)，catalog 规模可接受）。

use crate::error::SearchError;
use crate::model::{BuildReport, IndexDoc};
use crate::tokenizer::{coverage_tokens, index_text};
use database::{rusqlite, DbResult, Pool};
use rusqlite::OptionalExtension;
use std::path::Path;
use std::time::Instant;

/// 索引库 schema 版本（结构变更即升版，查询端拒绝旧版）。
/// v6：在 v5 治理字段基础上纳入 Overlay 的上游来源 URL；旧索引必须整库重建。
pub const INDEX_SCHEMA_VERSION: i64 = 8;
/// Read-only compatibility for the previous approved-only index while a running
/// server is being upgraded. Schema 7 has the same physical columns but no
/// candidate lane; the next server startup upgrades it to schema 8.
const LEGACY_APPROVED_ONLY_SCHEMA_VERSION: i64 = 7;

/// 索引库 DDL（头注声明可重建，MODULE_BOUNDARIES §8.2.3）。
pub const INDEX_SCHEMA: &str = r#"
-- 可重建派生索引（MODULE_BOUNDARIES §8.2.3）：本库全部内容可由
-- platform DB（resource/resource_source/plugin knowledge）与 catalog DB
-- （github_metadata）以及可选 RTIS + RIP curation overlay 全量重灌恢复；
-- 损坏或升版即整库删除重建，不做增量维护。
CREATE TABLE IF NOT EXISTS search_index_meta (
  key   TEXT PRIMARY KEY,
  value TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS search_index_doc (
  rowid                INTEGER PRIMARY KEY,
  resource_id          TEXT NOT NULL UNIQUE,
  name                 TEXT NOT NULL,
  description          TEXT,
  canonical_url        TEXT NOT NULL,
  resource_type        TEXT NOT NULL,
  lifecycle            TEXT NOT NULL,
  governance           TEXT NOT NULL,
  quality_final_score  REAL,
  quality_grade        TEXT,
  license_status       TEXT NOT NULL DEFAULT 'UNKNOWN',
  maintenance_status   TEXT NOT NULL DEFAULT 'UNKNOWN',
  topics_json          TEXT NOT NULL DEFAULT '[]',
  language             TEXT,
  source_sections_json TEXT NOT NULL DEFAULT '[]',
  source_repos_json    TEXT NOT NULL DEFAULT '[]',
  nuclei_severity      TEXT,
  nuclei_protocols_json TEXT NOT NULL DEFAULT '[]',
  nuclei_cve_ids_json  TEXT NOT NULL DEFAULT '[]',
  nuclei_cwe_ids_json  TEXT NOT NULL DEFAULT '[]',
  nuclei_tags_json     TEXT NOT NULL DEFAULT '[]',
  evidence_level       TEXT,
  knowledge_lifecycle  TEXT,
  poc_category         TEXT,
  poc_severity         TEXT,
  poc_cve_id           TEXT,
  source_path          TEXT,
  source_commit        TEXT,
  related_poc_ids_json TEXT NOT NULL DEFAULT '[]',
  related_entities_json TEXT NOT NULL DEFAULT '[]',
  poc_source_integrity TEXT,
  poc_verification_result TEXT,
  poc_dedup_status TEXT,
  poc_independent_knowledge_status TEXT,
  poc_missing_evidence_json TEXT NOT NULL DEFAULT '[]',
  poc_evidence_refs_json TEXT NOT NULL DEFAULT '[]',
  poc_review_gate TEXT,
  poc_source_urls_json TEXT NOT NULL DEFAULT '[]',
  identity_json        TEXT NOT NULL DEFAULT 'null',
  built_at             TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS ix_search_doc_governance ON search_index_doc(governance);
CREATE INDEX IF NOT EXISTS ix_search_doc_grade ON search_index_doc(quality_grade);
CREATE INDEX IF NOT EXISTS ix_search_doc_license ON search_index_doc(license_status);
CREATE INDEX IF NOT EXISTS ix_search_doc_maintenance ON search_index_doc(maintenance_status);
CREATE VIRTUAL TABLE IF NOT EXISTS search_index_fts USING fts5(
  name,
  description,
  aliases,
  topics,
  language,
  content=''
);
"#;

/// 派生索引构建器 / 打开器。
#[derive(Clone)]
pub struct SearchIndex;

impl SearchIndex {
    /// 在索引库上确保 schema 存在（幂等）。
    pub fn ensure_schema(pool: &Pool) -> DbResult<()> {
        pool.with_conn(|c| c.execute_batch(INDEX_SCHEMA))
    }

    /// 全量重建：删除既有索引表后从基础源重灌（可重建纪律的显式入口）。
    pub fn rebuild(
        platform: &Pool,
        catalog: &Pool,
        index: &Pool,
    ) -> Result<BuildReport, SearchError> {
        Self::build_inner(platform, catalog, index, true, None, None)
    }

    /// Rebuild while loading approved plugin knowledge from its separate read-only database.
    pub fn rebuild_with_plugin_db(
        platform: &Pool,
        plugin_db: &Pool,
        catalog: &Pool,
        index: &Pool,
    ) -> Result<BuildReport, SearchError> {
        Self::build_inner(platform, catalog, index, true, None, Some(plugin_db))
    }

    /// Build while loading approved plugin knowledge from its separate read-only database.
    pub fn build_with_plugin_db(
        platform: &Pool,
        plugin_db: &Pool,
        catalog: &Pool,
        index: &Pool,
    ) -> Result<BuildReport, SearchError> {
        Self::build_inner(platform, catalog, index, false, None, Some(plugin_db))
    }

    /// Rebuild the standard index and append the RTIS POC records explicitly curated by RIP.
    /// Only safe metadata columns are read from the RTIS source; POC body and verified_note
    /// are intentionally not selected. The RTIS DB is expected to be opened read-only.
    pub fn rebuild_with_rtis_curation(
        platform: &Pool,
        catalog: &Pool,
        rtis: &Pool,
        overlay_path: &Path,
        index: &Pool,
    ) -> Result<BuildReport, SearchError> {
        Self::build_inner(
            platform,
            catalog,
            index,
            true,
            Some((rtis, overlay_path)),
            None,
        )
    }

    /// Rebuild platform resources, separate plugin knowledge and explicitly curated RTIS POCs.
    pub fn rebuild_with_rtis_curation_and_plugin_db(
        platform: &Pool,
        plugin_db: &Pool,
        catalog: &Pool,
        rtis: &Pool,
        overlay_path: &Path,
        index: &Pool,
    ) -> Result<BuildReport, SearchError> {
        Self::build_inner(
            platform,
            catalog,
            index,
            true,
            Some((rtis, overlay_path)),
            Some(plugin_db),
        )
    }

    /// Build platform resources, separate plugin knowledge and explicitly curated RTIS POCs.
    pub fn build_with_rtis_curation_and_plugin_db(
        platform: &Pool,
        plugin_db: &Pool,
        catalog: &Pool,
        rtis: &Pool,
        overlay_path: &Path,
        index: &Pool,
    ) -> Result<BuildReport, SearchError> {
        Self::build_inner(
            platform,
            catalog,
            index,
            false,
            Some((rtis, overlay_path)),
            Some(plugin_db),
        )
    }

    /// Build the standard index with explicitly curated RTIS POCs into a new index database.
    pub fn build_with_rtis_curation(
        platform: &Pool,
        catalog: &Pool,
        rtis: &Pool,
        overlay_path: &Path,
        index: &Pool,
    ) -> Result<BuildReport, SearchError> {
        Self::build_inner(
            platform,
            catalog,
            index,
            false,
            Some((rtis, overlay_path)),
            None,
        )
    }

    /// 构建索引（目标库应为空库或从未建过索引；已有索引请走 [`SearchIndex::rebuild`]）。
    pub fn build(
        platform: &Pool,
        catalog: &Pool,
        index: &Pool,
    ) -> Result<BuildReport, SearchError> {
        Self::build_inner(platform, catalog, index, false, None, None)
    }

    fn build_inner(
        platform: &Pool,
        catalog: &Pool,
        index: &Pool,
        drop_existing: bool,
        rtis_curation: Option<(&Pool, &Path)>,
        plugin_db: Option<&Pool>,
    ) -> Result<BuildReport, SearchError> {
        let t0 = Instant::now();
        // 删表重建：fts 虚表先删，再建 schema
        if drop_existing {
            index.with_conn(|c| {
                c.execute_batch(
                    "DROP TABLE IF EXISTS search_index_fts;
                     DROP TABLE IF EXISTS search_index_doc;
                     DROP TABLE IF EXISTS search_index_meta;",
                )
            })?;
        }
        Self::ensure_schema(index)?;

        let docs = load_docs(platform, catalog, rtis_curation, plugin_db)?;
        let built_at = domain::time::now_iso();
        let mut fts_rows = 0usize;
        index.with_tx(|c| -> rusqlite::Result<()> {
            for d in &docs {
                c.execute(
                    "INSERT INTO search_index_doc(resource_id, name, description,
                       canonical_url, resource_type, lifecycle, governance,
                       quality_final_score, quality_grade, license_status, maintenance_status,
                       topics_json, language, source_sections_json, source_repos_json,
                       nuclei_severity, nuclei_protocols_json, nuclei_cve_ids_json,
                       nuclei_cwe_ids_json, nuclei_tags_json, evidence_level,
                       knowledge_lifecycle, poc_category, poc_severity, poc_cve_id,
                       source_path, source_commit, related_poc_ids_json, related_entities_json,
                       poc_source_integrity, poc_verification_result, poc_dedup_status,
                       poc_independent_knowledge_status, poc_missing_evidence_json,
                       poc_evidence_refs_json, poc_review_gate, poc_source_urls_json,
                       identity_json, built_at)
                     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                    rusqlite::params![
                        d.resource_id,
                        d.name,
                        d.description,
                        d.canonical_url,
                        d.resource_type,
                        d.lifecycle,
                        d.governance,
                        d.quality_final_score,
                        d.quality_grade,
                        d.license_status,
                        d.maintenance_status,
                        serde_json::to_string(&d.topics)
                            .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?,
                        d.language,
                        serde_json::to_string(&d.source_sections)
                            .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?,
                        serde_json::to_string(&d.source_repositories)
                            .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?,
                        d.nuclei_severity,
                        serde_json::to_string(&d.nuclei_protocols)
                            .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?,
                        serde_json::to_string(&d.nuclei_cve_ids)
                            .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?,
                        serde_json::to_string(&d.nuclei_cwe_ids)
                            .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?,
                        serde_json::to_string(&d.nuclei_tags)
                            .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?,
                        d.evidence_level,
                        d.knowledge_lifecycle,
                        d.poc_category,
                        d.poc_severity,
                        d.poc_cve_id,
                        d.source_path,
                        d.source_commit,
                        serde_json::to_string(&d.related_poc_ids)
                            .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?,
                        serde_json::to_string(&d.related_entities)
                            .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?,
                        d.poc_source_integrity,
                        d.poc_verification_result,
                        d.poc_dedup_status,
                        d.poc_independent_knowledge_status,
                        serde_json::to_string(&d.poc_missing_evidence)
                            .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?,
                        serde_json::to_string(&d.poc_evidence_refs)
                            .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?,
                        d.poc_review_gate,
                        serde_json::to_string(&d.poc_source_urls)
                            .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?,
                        serde_json::to_string(&d.identity)
                            .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?,
                        built_at,
                    ],
                )?;
                // doc 与 fts 行同 rowid 对齐（doc 先插，rowid 随后取）
                let rowid = c.last_insert_rowid();
                c.execute(
                    "INSERT INTO search_index_fts(rowid, name, description, aliases, topics, language)
                     VALUES (?, ?, ?, ?, ?, ?)",
                    rusqlite::params![
                        rowid,
                        index_text(&d.name),
                        index_text(d.description.as_deref().unwrap_or("")),
                        index_text(&format!(
                            "{} {} {} {} {} {} {}",
                            d.source_sections.join(" "),
                            d.source_repositories.join(" "),
                            d.source_path.as_deref().unwrap_or(""),
                            d.source_commit.as_deref().unwrap_or(""),
                            d.related_poc_ids.join(" "),
                            d.related_entities.join(" "),
                            d.poc_source_urls.join(" ")
                        )),
                        index_text(&format!("{} {} {} {} {} {} {} {} {} {} {} {}", d.topics.join(" "),
                            d.evidence_level.as_deref().unwrap_or(""),
                            d.knowledge_lifecycle.as_deref().unwrap_or(""),
                            d.poc_category.as_deref().unwrap_or(""),
                            d.poc_severity.as_deref().unwrap_or(""),
                            d.poc_cve_id.as_deref().unwrap_or(""),
                            d.poc_source_integrity.as_deref().unwrap_or(""),
                            d.poc_verification_result.as_deref().unwrap_or(""),
                            d.poc_dedup_status.as_deref().unwrap_or(""),
                            d.poc_missing_evidence.join(" "),
                            d.poc_review_gate.as_deref().unwrap_or(""),
                            d.related_entities.join(" "))),
                        index_text(d.language.as_deref().unwrap_or("")),
                    ],
                )?;
                fts_rows += 1;
            }
            Ok(())
        })?;

        let doc_count = docs.len();
        index.with_tx(|c| -> rusqlite::Result<()> {
            for (k, v) in [
                ("schema_version", INDEX_SCHEMA_VERSION.to_string()),
                ("doc_count", doc_count.to_string()),
                ("fts_rows", fts_rows.to_string()),
                ("built_at", built_at),
                ("rebuilt", if drop_existing { "1" } else { "0" }.to_string()),
                (
                    "coverage_note",
                    "coverage tokens 在查询期从 doc 原文重算，不入索引列".to_string(),
                ),
            ] {
                c.execute(
                    "INSERT INTO search_index_meta(key, value) VALUES (?1, ?2)
                     ON CONFLICT(key) DO UPDATE SET value=excluded.value",
                    rusqlite::params![k, v],
                )?;
            }
            Ok(())
        })?;

        Ok(BuildReport {
            doc_count,
            fts_rows,
            rebuilt: drop_existing,
            schema_version: INDEX_SCHEMA_VERSION,
            elapsed_ms: t0.elapsed().as_secs_f64() * 1000.0,
        })
    }

    /// 索引文档总数（含治理不可见行；查询端再过滤）。
    pub fn doc_count(pool: &Pool) -> Result<usize, SearchError> {
        ensure_ready(pool)?;
        let n: i64 = pool.with_conn(|c| {
            c.query_row("SELECT COUNT(*) FROM search_index_doc", [], |r| r.get(0))
        })?;
        Ok(n as usize)
    }

    /// 读取索引元数据。
    pub fn meta(pool: &Pool, key: &str) -> Result<Option<String>, SearchError> {
        ensure_ready(pool)?;
        let v: Option<String> = pool.with_conn(|c| {
            c.query_row(
                "SELECT value FROM search_index_meta WHERE key = ?1",
                [key],
                |r| r.get(0),
            )
            .optional()
        })?;
        Ok(v)
    }
}

/// 校验索引就绪。当前 schema 8 可检索候选；schema 7 仅兼容已批准通道，
/// 供运行中的旧 API 平滑升级，下一次服务启动会自动重建为 schema 8。
pub(crate) fn ensure_ready(pool: &Pool) -> Result<(), SearchError> {
    let version: Option<i64> = match pool.with_conn(|c| {
        c.query_row(
            "SELECT value FROM search_index_meta WHERE key='schema_version'",
            [],
            |r| r.get::<_, String>(0),
        )
        .optional()
    }) {
        Ok(v) => v.and_then(|s| s.parse().ok()),
        Err(_) => None, // 表不存在等：未构建
    };
    match version {
        Some(v) if v == INDEX_SCHEMA_VERSION || v == LEGACY_APPROVED_ONLY_SCHEMA_VERSION => Ok(()),
        Some(v) => Err(SearchError::IndexNotReady(format!(
            "index schema_version={v}, engine expects {INDEX_SCHEMA_VERSION}: rebuild required"
        ))),
        None => Err(SearchError::IndexNotReady(
            "search_index_meta missing: index not built (run build/rebuild first)".to_string(),
        )),
    }
}

/// 只读加载基础资源、catalog 元数据、可见插件知识和可选 Overlay POC。
fn load_docs(
    platform: &Pool,
    catalog: &Pool,
    rtis_curation: Option<(&Pool, &Path)>,
    plugin_db: Option<&Pool>,
) -> Result<Vec<IndexDoc>, SearchError> {
    // 1) platform resource 主行
    let mut docs: Vec<IndexDoc> = Vec::new();
    platform.with_conn(|c| -> rusqlite::Result<()> {
        let mut stmt = c.prepare(
            "SELECT resource_id, name, description, canonical_url, resource_type,
                    lifecycle, governance, quality_final_score, quality_grade
             FROM resource ORDER BY resource_id",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(IndexDoc {
                resource_id: r.get(0)?,
                name: r.get(1)?,
                description: r.get(2)?,
                canonical_url: r.get(3)?,
                resource_type: r.get(4)?,
                lifecycle: r.get(5)?,
                governance: r.get(6)?,
                quality_final_score: r.get(7)?,
                quality_grade: r.get(8)?,
                identity: Some(crate::model::SearchIdentity {
                    source_system: Some("platform".to_string()),
                    source_entity_id: Some(r.get(0)?),
                    ..crate::model::SearchIdentity::default()
                }),
                // 缺 github_metadata 行的文档按缺证归一（与 eri 输出口径一致）
                license_status: "UNKNOWN".to_string(),
                maintenance_status: "UNKNOWN".to_string(),
                ..IndexDoc::default()
            })
        })?;
        for row in rows {
            docs.push(row?);
        }
        Ok(())
    })?;

    // 2) resource_source alias/name 折叠（sections + repositories 去重，按 position 序）
    platform.with_conn(|c| -> rusqlite::Result<()> {
        let mut stmt = c.prepare(
            "SELECT resource_id, source_section, source_repository FROM resource_source
             ORDER BY resource_id, source_position",
        )?;
        let rows = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                ))
            })?
            .collect::<Result<Vec<_>, rusqlite::Error>>()?;
        for (rid, section, repo) in rows {
            if let Some(d) = docs.iter_mut().find(|d| d.resource_id == rid) {
                if !d.source_sections.contains(&section) {
                    d.source_sections.push(section);
                }
                if !d.source_repositories.contains(&repo) {
                    d.source_repositories.push(repo);
                }
            }
        }
        Ok(())
    })?;

    // 3) catalog github_metadata 补齐（topics/language/license/maintenance；
    //    缺证 license/maintenance 归一 UNKNOWN，与 eri 输出口径一致）
    catalog.with_conn(|c| -> rusqlite::Result<()> {
        let table: Option<String> = c
            .query_row(
                "SELECT name FROM sqlite_master WHERE type='table' AND name='github_metadata'",
                [],
                |r| r.get(0),
            )
            .optional()?;
        if table.as_deref() != Some("github_metadata") {
            return Ok(());
        }
        let mut stmt = c.prepare(
            "SELECT resource_id, topics, language, license_status, maintenance_status
             FROM github_metadata",
        )?;
        let rows = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, Option<String>>(1)?,
                    r.get::<_, Option<String>>(2)?,
                    r.get::<_, Option<String>>(3)?,
                    r.get::<_, Option<String>>(4)?,
                ))
            })?
            .collect::<Result<Vec<_>, rusqlite::Error>>()?;
        for (rid, topics, language, license_status, maintenance_status) in rows {
            if let Some(d) = docs.iter_mut().find(|d| d.resource_id == rid) {
                d.topics = parse_topics(topics.as_deref());
                d.language = language;
                d.license_status = license_status.unwrap_or_else(|| "UNKNOWN".to_string());
                d.maintenance_status = maintenance_status.unwrap_or_else(|| "UNKNOWN".to_string());
            }
        }
        Ok(())
    })?;

    // 4) SK-1 第三源：插件知识条目（platform 库 knowledge_source_entry，
    //    011 迁移）。准入双闸（ADR 第七节）：审核通过（review_status=
    //    APPROVED）且来源插件处于可见状态（is_active=1 且 state 在
    //    ENABLED/LOADED/ACTIVE 且来源未卸载）；禁用即撤出匹配面（重建即刷新，
    //    索引为可整库重建派生物）。表缺失（旧库/内存夹具未迁移）按空源跳过。
    load_plugin_entries(plugin_db.unwrap_or(platform), &mut docs)?;
    if let Some((rtis, overlay_path)) = rtis_curation {
        load_rtis_curated_pocs(rtis, overlay_path, &mut docs)?;
        load_rtis_candidate_weapons(rtis, overlay_path, &mut docs)?;
    }
    load_platform_skills(platform, &mut docs)?;
    load_platform_tools(platform, &mut docs)?;

    Ok(docs)
}

/// M8 修复：Skill 注册表入统一检索投影。enabled=1 → ACTIVE，其余 →
/// CANDIDATE（候选通道）；治理状态随表照抄，不做任何提升。
fn load_platform_skills(platform: &Pool, docs: &mut Vec<IndexDoc>) -> Result<(), SearchError> {
    platform.with_conn(|c| -> rusqlite::Result<()> {
        // 表缺失（旧库/内存夹具）按空源跳过。
        let exists: bool = c
            .query_row(
                "SELECT count(*)>0 FROM sqlite_master WHERE type='table' AND name='skill'",
                [],
                |r| r.get(0),
            )
            .unwrap_or(false);
        if !exists {
            return Ok(());
        }
        let mut stmt = c.prepare(
            "SELECT skill_id, name, description, when_to_use, governance, enabled
             FROM skill ORDER BY skill_id",
        )?;
        let rows = stmt.query_map([], |r| {
            let skill_id: String = r.get(0)?;
            let name: String = r.get(1)?;
            let description: Option<String> = r.get(2)?;
            let when_to_use: Option<String> = r.get(3)?;
            let _governance: Option<String> = r.get(4)?;
            let enabled: Option<i64> = r.get(5)?;
            let lane = if enabled.unwrap_or(0) == 1 { "ACTIVE" } else { "CANDIDATE" };
            let summary = match (description, when_to_use) {
                (Some(d), Some(w)) => Some(format!("{d}；适用场景：{w}")),
                (Some(d), None) => Some(d),
                (None, Some(w)) => Some(format!("适用场景：{w}")),
                _ => None,
            };
            Ok(IndexDoc {
                resource_id: format!("skill:{skill_id}"),
                name,
                description: summary,
                canonical_url: format!("ripskill://{skill_id}"),
                resource_type: "skill".to_string(),
                lifecycle: lane.to_string(),
                governance: lane.to_string(),
                identity: Some(crate::model::SearchIdentity {
                    entity_type: Some("skill".to_string()),
                    canonical_key: Some(format!("skill:{skill_id}")),
                    source_system: Some("platform.skill".to_string()),
                    source_entity_id: Some(skill_id),
                    ..crate::model::SearchIdentity::default()
                }),
                ..IndexDoc::default()
            })
        })?;
        for row in rows {
            docs.push(row?);
        }
        Ok(())
    })?;
    Ok(())
}

/// M8 修复：安全工具注册表入统一检索投影。lifecycle 非 DISABLED 且
/// governance=ACTIVE → ACTIVE，其余 → CANDIDATE。
fn load_platform_tools(platform: &Pool, docs: &mut Vec<IndexDoc>) -> Result<(), SearchError> {
    platform.with_conn(|c| -> rusqlite::Result<()> {
        let exists: bool = c
            .query_row(
                "SELECT count(*)>0 FROM sqlite_master WHERE type='table' AND name='tool'",
                [],
                |r| r.get(0),
            )
            .unwrap_or(false);
        if !exists {
            return Ok(());
        }
        // purpose 列在旧夹具 schema 可能缺席（CI sk1_extended_ingest 夹具），
        // 列探测后以 NULL 占位对齐列位。
        let has_purpose: bool = c
            .query_row(
                "SELECT count(*)>0 FROM pragma_table_info('tool') WHERE name='purpose'",
                [],
                |r| r.get(0),
            )
            .unwrap_or(false);
        let tool_sql = if has_purpose {
            "SELECT tool_id, name, purpose, lifecycle, governance, permission_level
             FROM tool ORDER BY tool_id"
        } else {
            "SELECT tool_id, name, NULL, lifecycle, governance, permission_level
             FROM tool ORDER BY tool_id"
        };
        let mut stmt = c.prepare(tool_sql)?;
        let rows = stmt.query_map([], |r| {
            let tool_id: String = r.get(0)?;
            let name: String = r.get(1)?;
            let purpose: Option<String> = r.get(2)?;
            let lifecycle: Option<String> = r.get(3)?;
            let governance: Option<String> = r.get(4)?;
            let permission_level: Option<String> = r.get(5)?;
            let lane = if lifecycle.as_deref() != Some("DISABLED")
                && governance.as_deref() == Some("ACTIVE")
            {
                "ACTIVE"
            } else {
                "CANDIDATE"
            };
            Ok(IndexDoc {
                resource_id: format!("tool:{tool_id}"),
                name,
                description: purpose,
                canonical_url: format!("riptool://{tool_id}"),
                resource_type: "security_tool".to_string(),
                lifecycle: lifecycle.unwrap_or_else(|| "UNKNOWN".to_string()),
                governance: lane.to_string(),
                identity: Some(crate::model::SearchIdentity {
                    entity_type: Some("security_tool".to_string()),
                    canonical_key: Some(format!("tool:{tool_id}")),
                    source_system: Some("platform.tool".to_string()),
                    source_entity_id: Some(tool_id),
                    ..crate::model::SearchIdentity::default()
                }),
                topics: permission_level.into_iter().collect(),
                ..IndexDoc::default()
            })
        })?;
        for row in rows {
            docs.push(row?);
        }
        Ok(())
    })?;
    Ok(())
}

/// 第三源装载（知识条目 + 可见插件过滤；两表任一缺失即空集跳过）。
fn load_plugin_entries(platform: &Pool, docs: &mut Vec<IndexDoc>) -> DbResult<()> {
    platform.with_conn(|c| -> rusqlite::Result<()> {
        let kse_table: Option<String> = c
            .query_row(
                "SELECT name FROM sqlite_master WHERE type='table' AND name='knowledge_source_entry'",
                [],
                |r| r.get(0),
            )
            .optional()?;
        let plugin_table: Option<String> = c
            .query_row(
                "SELECT name FROM sqlite_master WHERE type='table' AND name='plugin'",
                [],
                |r| r.get(0),
            )
            .optional()?;
        if kse_table.as_deref() != Some("knowledge_source_entry") {
            return Ok(());
        }
        // 可见插件的唯一 active version（缺 plugin 表 / 多 active 版本均
        // fail-closed）；不能把历史版本的 APPROVED 条目混入当前索引。
        let mut visible_versions: std::collections::HashMap<String, String> =
            std::collections::HashMap::new();
        if plugin_table.as_deref() == Some("plugin") {
            let mut stmt = c.prepare(
                "SELECT plugin_id, MIN(version) AS version
                 FROM plugin
                 WHERE is_active = 1
                   AND state IN ('ENABLED','LOADED','ACTIVE')
                   AND source_uninstalled = 0
                 GROUP BY plugin_id
                 HAVING COUNT(*) = 1",
            )?;
            let rows = stmt
                .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
                .collect::<Result<Vec<_>, rusqlite::Error>>()?;
            visible_versions.extend(rows);
        }
        if visible_versions.is_empty() {
            return Ok(());
        }
        let mut stmt = c.prepare(
            "SELECT plugin_id, plugin_version, entry_kind, item_key, title, description,
                    body, canonical_url, category_path_json, license_status,
                    source_location_json, meta_json, source_commit,
                    content_sha256, metadata_sha256, review_status
             FROM knowledge_source_entry
             WHERE review_status IN ('APPROVED','DISCOVERED')
               AND upstream_missing = 0
             ORDER BY plugin_id, entry_kind, item_key",
        )?;
        let rows = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, String>(4)?,
                    r.get::<_, Option<String>>(5)?,
                    r.get::<_, String>(6)?,
                    r.get::<_, Option<String>>(7)?,
                    r.get::<_, String>(8)?,
                    r.get::<_, String>(9)?,
                    r.get::<_, String>(10)?,
                    r.get::<_, Option<String>>(11)?,
                    r.get::<_, Option<String>>(12)?,
                    r.get::<_, Option<String>>(13)?,
                    r.get::<_, Option<String>>(14)?,
                    r.get::<_, String>(15)?,
                ))
            })?
            .collect::<Result<Vec<_>, rusqlite::Error>>()?;
        for (
            plugin_id,
            plugin_version,
            entry_kind,
            item_key,
            title,
            description,
            body,
            canonical_url,
            category_path_json,
            license_status,
            source_location_json,
            meta_json,
            source_commit,
            content_sha256,
            metadata_sha256,
            review_status,
        ) in rows {
            if visible_versions.get(&plugin_id).map(String::as_str) != Some(plugin_version.as_str()) {
                continue;
            }
            let mut category_path = parse_topics(Some(category_path_json.as_str()));
            let source_file = parse_source_file(&source_location_json);
            let nuclei = parse_nuclei_index_metadata(meta_json.as_deref());
            for topic in nuclei
                .tags
                .iter()
                .chain(nuclei.protocols.iter())
                .chain(nuclei.cve_ids.iter())
                .chain(nuclei.cwe_ids.iter())
                .chain(nuclei.severity.iter())
            {
                if !category_path.iter().any(|existing| existing.eq_ignore_ascii_case(topic)) {
                    category_path.push(topic.clone());
                }
            }
            let snippet: String = body.chars().take(200).collect();
            let meta: serde_json::Value = meta_json
                .as_deref()
                .and_then(|raw| serde_json::from_str(raw).ok())
                .unwrap_or_else(|| serde_json::json!({}));
            let nuclei_cves = nuclei.cve_ids.clone();
            let pocindex_cve = meta
                .get("pocindex_record")
                .and_then(|value| value.get("cve_id"))
                .and_then(serde_json::Value::as_str)
                .and_then(normalize_cve_id);
            let cisa_kev_cve = if plugin_id == "knowledge.cisa-kev" {
                normalize_cve_id(&item_key)
            } else {
                None
            };
            let template_id = meta
                .get("nuclei_template")
                .and_then(|value| value.get("template_id"))
                .and_then(serde_json::Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string);
            let (entity_type, canonical_key, vulnerability_ids) = if entry_kind == "nuclei_template" {
                (
                    Some("detection_template".to_string()),
                    template_id.map(|id| format!("nuclei:{id}")),
                    nuclei_cves,
                )
            } else if plugin_id == "knowledge.pocindex-kev" {
                (
                    Some("vulnerability".to_string()),
                    pocindex_cve.as_ref().map(|cve| format!("cve:{cve}")),
                    pocindex_cve.into_iter().collect(),
                )
            } else if plugin_id == "knowledge.cisa-kev" {
                (
                    Some("vulnerability".to_string()),
                    cisa_kev_cve.as_ref().map(|cve| format!("cve:{cve}")),
                    cisa_kev_cve.into_iter().collect(),
                )
            } else {
                (None, None, Vec::new())
            };
            docs.push(IndexDoc {
                resource_id: format!("kse:{plugin_id}@{plugin_version}:{entry_kind}:{item_key}"),
                name: title,
                description: description.or_else(|| {
                    if review_status == "APPROVED" && !snippet.trim().is_empty() {
                        Some(snippet)
                    } else {
                        None
                    }
                }),
                canonical_url: canonical_url.unwrap_or_else(|| format!("plugin://{plugin_id}")),
                resource_type: entry_kind,
                lifecycle: if review_status == "APPROVED" { "INDEXED" } else { "DISCOVERED" }.to_string(),
                governance: if review_status == "APPROVED" { "ACTIVE" } else { "CANDIDATE" }.to_string(),
                quality_final_score: None,
                quality_grade: None,
                license_status: if license_status.is_empty() {
                    "UNKNOWN".to_string()
                } else {
                    license_status
                },
                maintenance_status: "UNKNOWN".to_string(),
                topics: category_path,
                language: None,
                source_sections: source_file.iter().cloned().collect(),
                source_repositories: vec![format!("plugin:{plugin_id}")],
                source_path: source_file,
                source_commit,
                identity: Some(crate::model::SearchIdentity {
                    entity_type,
                    canonical_key,
                    source_system: Some(plugin_id.clone()),
                    source_entity_id: Some(item_key.clone()),
                    source_version: Some(plugin_version.clone()),
                    content_sha256,
                    metadata_sha256,
                    vulnerability_ids,
                }),
                nuclei_severity: nuclei.severity,
                nuclei_protocols: nuclei.protocols,
                nuclei_cve_ids: nuclei.cve_ids,
                nuclei_cwe_ids: nuclei.cwe_ids,
                nuclei_tags: nuclei.tags,
                ..IndexDoc::default()
            });
        }
        Ok(())
    })
}

#[derive(Debug)]
struct RtisCandidateRow {
    id: i64,
    code: String,
    title: String,
    kind: Option<String>,
    category: Option<String>,
    cve: Option<String>,
    component: Option<String>,
    versions: Option<String>,
    severity: Option<String>,
    language: Option<String>,
    source: Option<String>,
    source_url: Option<String>,
    tags: Option<String>,
    legacy_verified: i64,
    updated_at: Option<String>,
    evidence_level: Option<String>,
    knowledge_lifecycle: Option<String>,
}

/// Index non-Nuclei RTIS assets as explicit candidates, never as trusted POCs.
/// The loader reads metadata only: it deliberately excludes content, usage,
/// verified_note and local source files from the retrieval projection.
fn load_rtis_candidate_weapons(
    rtis: &Pool,
    overlay_path: &Path,
    docs: &mut Vec<IndexDoc>,
) -> Result<(), SearchError> {
    let raw = std::fs::read_to_string(overlay_path).map_err(|e| {
        SearchError::IndexNotReady(format!(
            "cannot read RTIS curation overlay {}: {e}",
            overlay_path.display()
        ))
    })?;
    let overlay: serde_json::Value = serde_json::from_str(&raw).map_err(|e| {
        SearchError::IndexNotReady(format!(
            "invalid RTIS curation overlay {}: {e}",
            overlay_path.display()
        ))
    })?;
    let curated_ids: std::collections::HashSet<i64> = overlay
        .get("entries")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|entry| {
            entry
                .get("knowledge_id")
                .and_then(serde_json::Value::as_i64)
        })
        .collect();

    let rows = rtis.with_conn(|c| -> rusqlite::Result<Vec<RtisCandidateRow>> {
        let has_meta: bool = c.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='k_meta')",
            [], |row| row.get(0)
        )?;
        let sql = if has_meta {
            "SELECT p.id,p.code,p.title,p.kind,p.category,p.cve,p.component,p.versions,p.severity,p.language,p.source,p.source_url,p.tags,p.verified,p.updated_at,m.evidence_level,m.lifecycle
             FROM poc p LEFT JOIN k_meta m ON m.knowledge_id=p.id
             WHERE lower(COALESCE(p.source,'')) <> 'nuclei' ORDER BY p.id"
        } else {
            "SELECT p.id,p.code,p.title,p.kind,p.category,p.cve,p.component,p.versions,p.severity,p.language,p.source,p.source_url,p.tags,p.verified,p.updated_at,NULL,NULL
             FROM poc p WHERE lower(COALESCE(p.source,'')) <> 'nuclei' ORDER BY p.id"
        };
        let mut stmt = c.prepare(sql)?;
        let mapped = stmt.query_map([], |row| Ok(RtisCandidateRow {
            id: row.get(0)?, code: row.get(1)?, title: row.get(2)?, kind: row.get(3)?,
            category: row.get(4)?, cve: row.get(5)?, component: row.get(6)?, versions: row.get(7)?,
            severity: row.get(8)?, language: row.get(9)?, source: row.get(10)?, source_url: row.get(11)?,
            tags: row.get(12)?, legacy_verified: row.get(13)?, updated_at: row.get(14)?,
            evidence_level: row.get(15)?, knowledge_lifecycle: row.get(16)?,
        }))?;
        mapped.collect()
    })?;

    for row in rows {
        let RtisCandidateRow {
            id,
            code,
            title,
            kind,
            category,
            cve,
            component,
            versions,
            severity,
            language,
            source,
            source_url,
            tags,
            legacy_verified,
            updated_at,
            evidence_level,
            knowledge_lifecycle,
        } = row;
        if curated_ids.contains(&id) {
            continue;
        }
        let mut topics = parse_topics(tags.as_deref());
        if let Some(raw_tags) = tags.as_deref() {
            if topics.is_empty() {
                topics.extend(
                    raw_tags
                        .split(',')
                        .map(str::trim)
                        .filter(|v| !v.is_empty())
                        .map(str::to_string),
                );
            }
        }
        let mut add_topic = |value: Option<String>| {
            if let Some(value) = value.filter(|v| !v.trim().is_empty()) {
                if !topics
                    .iter()
                    .any(|existing| existing.eq_ignore_ascii_case(&value))
                {
                    topics.push(value);
                }
            }
        };
        add_topic(kind.clone());
        add_topic(category.clone());
        add_topic(component.clone());
        add_topic(cve.clone());
        add_topic(severity.clone());
        add_topic(source.clone());
        let cve_ids: Vec<String> = cve
            .as_deref()
            .unwrap_or("")
            .split(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '-'))
            .map(str::trim)
            .filter(|token| {
                let upper = token.to_ascii_uppercase();
                upper.starts_with("CVE-")
                    && upper.len() >= 13
                    && upper.len() <= 27
                    && upper[4..8].bytes().all(|b| b.is_ascii_digit())
                    && upper[9..].bytes().all(|b| b.is_ascii_digit())
            })
            .map(str::to_ascii_uppercase)
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect();
        let mut missing = Vec::new();
        if source_url
            .as_deref()
            .map(str::trim)
            .filter(|v| v.starts_with("http://") || v.starts_with("https://"))
            .is_none()
        {
            missing.push("source_url_missing".to_string());
        }
        if versions
            .as_deref()
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .is_none()
        {
            missing.push("affected_versions_missing".to_string());
        }
        let source_urls = source_url
            .into_iter()
            .filter(|url| url.starts_with("http://") || url.starts_with("https://"))
            .collect();
        let is_true_poc = kind
            .as_deref()
            .map(|v| v.eq_ignore_ascii_case("poc"))
            .unwrap_or(false);
        docs.push(IndexDoc {
            resource_id: format!("rtis:weapon-candidate:{id}"),
            name: title,
            description: None,
            canonical_url: format!("rtis://weapon-candidate/{id}"),
            resource_type: "rtis_weapon_candidate".to_string(),
            lifecycle: "DISCOVERED".to_string(),
            governance: "CANDIDATE".to_string(),
            license_status: "UNKNOWN".to_string(),
            maintenance_status: "UNKNOWN".to_string(),
            topics,
            language,
            evidence_level,
            knowledge_lifecycle,
            poc_category: category,
            poc_severity: severity,
            poc_cve_id: cve.clone(),
            poc_verification_result: Some(if legacy_verified == 1 { "legacy_claim_only" } else { "not_assessed" }.to_string()),
            poc_source_integrity: Some("unknown".to_string()),
            poc_dedup_status: Some("unreviewed".to_string()),
            poc_independent_knowledge_status: Some("unreviewed".to_string()),
            poc_missing_evidence: missing,
            poc_review_gate: Some("Candidate only: review provenance, license, affected versions, safety and reproducibility before activation.".to_string()),
            poc_source_urls: source_urls,
            identity: Some(crate::model::SearchIdentity {
                entity_type: if is_true_poc { Some("poc".to_string()) } else { None },
                canonical_key: if is_true_poc { Some(format!("rtis:poc:{id}")) } else { None },
                source_system: Some("rtis".to_string()),
                source_entity_id: Some(id.to_string()),
                source_version: updated_at.or_else(|| Some("rtis-snapshot".to_string())),
                vulnerability_ids: cve_ids,
                ..crate::model::SearchIdentity::default()
            }),
            ..IndexDoc::default()
        });
        let _ = code; // Stable RTIS ID is the identity; code is intentionally not exported.
    }
    Ok(())
}

type CuratedPocRow = (
    i64,
    String,
    String,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
);

/// Append only POCs explicitly present in the RIP curation overlay.
/// Only metadata columns are selected from RTIS. POC body, description, usage,
/// verified_note and other free-form verification material are not read.
fn load_rtis_curated_pocs(
    rtis: &Pool,
    overlay_path: &Path,
    docs: &mut Vec<IndexDoc>,
) -> Result<(), SearchError> {
    let raw = std::fs::read_to_string(overlay_path).map_err(|e| {
        SearchError::IndexNotReady(format!(
            "cannot read RTIS curation overlay {}: {e}",
            overlay_path.display()
        ))
    })?;
    let overlay: serde_json::Value = serde_json::from_str(&raw).map_err(|e| {
        SearchError::IndexNotReady(format!(
            "invalid RTIS curation overlay {}: {e}",
            overlay_path.display()
        ))
    })?;
    if overlay
        .get("schema_version")
        .and_then(serde_json::Value::as_i64)
        != Some(1)
    {
        return Err(SearchError::IndexNotReady(
            "unsupported RTIS curation overlay schema_version".into(),
        ));
    }
    let entries = overlay
        .get("entries")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| {
            SearchError::IndexNotReady("RTIS curation overlay entries must be an array".into())
        })?;
    let relations = overlay
        .get("relations")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| {
            SearchError::IndexNotReady("RTIS curation overlay relations must be an array".into())
        })?;
    let mut code_to_id = std::collections::HashMap::<String, String>::new();
    let mut seen_ids = std::collections::HashSet::<i64>::new();
    for entry in entries {
        let id = entry
            .get("knowledge_id")
            .and_then(serde_json::Value::as_i64)
            .ok_or_else(|| {
                SearchError::IndexNotReady("curation entry has invalid knowledge_id".into())
            })?;
        let code = entry
            .get("code")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| {
                SearchError::IndexNotReady(format!("curation entry {id} has no code"))
            })?;
        if !seen_ids.insert(id) {
            return Err(SearchError::IndexNotReady(format!(
                "duplicate POC ID in curation overlay: {id}"
            )));
        }
        if code_to_id
            .insert(code.to_string(), id.to_string())
            .is_some()
        {
            return Err(SearchError::IndexNotReady(format!(
                "duplicate POC code in curation overlay: {code}"
            )));
        }
    }
    let external_entities = overlay
        .get("external_entities")
        .and_then(serde_json::Value::as_array)
        .cloned()
        .unwrap_or_default();
    let external_codes: std::collections::HashSet<String> = external_entities
        .iter()
        .filter_map(|entity| entity.get("code").and_then(serde_json::Value::as_str))
        .map(str::to_string)
        .collect();
    let mut related = std::collections::HashMap::<String, Vec<String>>::new();
    let mut related_external = std::collections::HashMap::<String, Vec<String>>::new();
    for edge in relations {
        let (Some(src), Some(dst), Some(kind)) = (
            edge.get("src").and_then(serde_json::Value::as_str),
            edge.get("dst").and_then(serde_json::Value::as_str),
            edge.get("type").and_then(serde_json::Value::as_str),
        ) else {
            return Err(SearchError::IndexNotReady(
                "curation relation has invalid endpoints/type".into(),
            ));
        };
        let src_id = code_to_id.get(src);
        let dst_id = code_to_id.get(dst);
        if src_id.is_none() && dst_id.is_none() {
            continue;
        }
        if src_id.is_none() && !external_codes.contains(src) {
            return Err(SearchError::IndexNotReady(format!(
                "unresolved curation relation source: {src}"
            )));
        }
        if dst_id.is_none() && !external_codes.contains(dst) {
            return Err(SearchError::IndexNotReady(format!(
                "unresolved curation relation target: {dst}"
            )));
        }
        match (src_id, dst_id) {
            (Some(src_id), Some(dst_id)) => {
                related
                    .entry(src_id.clone())
                    .or_default()
                    .push(format!("{kind}:{dst_id}"));
                related
                    .entry(dst_id.clone())
                    .or_default()
                    .push(format!("{kind}:{src_id}"));
            }
            (Some(src_id), None) => related_external
                .entry(src_id.clone())
                .or_default()
                .push(format!("{kind}:{dst}")),
            (None, Some(dst_id)) => related_external
                .entry(dst_id.clone())
                .or_default()
                .push(format!("{kind}:{src}")),
            (None, None) => {}
        }
    }
    let rows = rtis.with_conn(|c| -> rusqlite::Result<Vec<CuratedPocRow>> {
        let mut out = Vec::with_capacity(entries.len());
        for entry in entries {
            let id = entry
                .get("knowledge_id")
                .and_then(serde_json::Value::as_i64)
                .unwrap();
            let expected_code = entry
                .get("code")
                .and_then(serde_json::Value::as_str)
                .unwrap();
            let row = c.query_row(
                "SELECT id, code, title, kind, category, cve, component, severity, tags
                 FROM poc WHERE id = ?1",
                [id],
                |r| {
                    Ok((
                        r.get(0)?,
                        r.get(1)?,
                        r.get(2)?,
                        r.get(3)?,
                        r.get(4)?,
                        r.get(5)?,
                        r.get(6)?,
                        r.get(7)?,
                        r.get(8)?,
                    ))
                },
            )?;
            if row.1 != expected_code {
                return Err(rusqlite::Error::InvalidQuery);
            }
            out.push(row);
        }
        Ok(out)
    })?;
    for (id, _code, title, kind, category, cve, component, severity, tags) in rows {
        let entry = entries
            .iter()
            .find(|e| e.get("knowledge_id").and_then(serde_json::Value::as_i64) == Some(id))
            .expect("entry matched row");
        let string_field = |key: &str| {
            entry
                .get(key)
                .and_then(serde_json::Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
        };
        let category = category.filter(|value| !value.trim().is_empty());
        let cve = cve.filter(|value| !value.trim().is_empty());
        let component = component.filter(|value| !value.trim().is_empty());
        let severity = severity.filter(|value| !value.trim().is_empty());
        let mut topics = Vec::<String>::new();
        let mut add = |value: Option<String>| {
            if let Some(value) = value.filter(|s| !s.trim().is_empty()) {
                if !topics.iter().any(|v| v.eq_ignore_ascii_case(&value)) {
                    topics.push(value);
                }
            }
        };
        for raw_tags in [
            tags,
            string_field("technology_tags"),
            string_field("method_tags"),
            string_field("archetypes"),
        ]
        .into_iter()
        .flatten()
        {
            if let Ok(values) = serde_json::from_str::<Vec<String>>(&raw_tags) {
                for value in values {
                    add(Some(value));
                }
            } else {
                for value in raw_tags.split(',').map(str::trim).filter(|v| !v.is_empty()) {
                    add(Some(value.to_string()));
                }
            }
        }
        add(category.clone());
        add(component.clone());
        add(cve.clone());
        add(severity.clone());
        add(string_field("original_category"));
        add(string_field("normalized_type"));
        let mut related_ids = related.remove(&id.to_string()).unwrap_or_default();
        related_ids.sort();
        related_ids.dedup();
        let source_path = string_field("source_path");
        let source_commit = string_field("source_commit");
        let mut related_entities = related_external.remove(&id.to_string()).unwrap_or_default();
        related_entities.sort();
        related_entities.dedup();
        docs.push(IndexDoc {
            resource_id: format!("rtis:poc:{id}"),
            name: title,
            description: None,
            canonical_url: format!("rtis://poc/{id}"),
            resource_type: "rtis_poc".to_string(),
            lifecycle: "CURATED".to_string(),
            governance: "ACTIVE".to_string(),
            quality_final_score: None,
            quality_grade: None,
            license_status: "UNKNOWN".to_string(),
            maintenance_status: "UNKNOWN".to_string(),
            topics,
            language: kind,
            source_sections: source_path.iter().cloned().collect(),
            source_repositories: Vec::new(),
            nuclei_severity: None,
            nuclei_protocols: Vec::new(),
            nuclei_cve_ids: Vec::new(),
            nuclei_cwe_ids: Vec::new(),
            nuclei_tags: Vec::new(),
            evidence_level: string_field("evidence_level"),
            knowledge_lifecycle: string_field("lifecycle"),
            poc_category: category,
            poc_severity: severity,
            poc_cve_id: cve.clone(),
            source_path,
            source_commit,
            identity: Some(crate::model::SearchIdentity {
                entity_type: Some("poc".to_string()),
                canonical_key: Some(format!("rtis:poc:{id}")),
                source_system: Some("rtis".to_string()),
                source_entity_id: Some(id.to_string()),
                source_version: Some("curation-overlay-v1".to_string()),
                vulnerability_ids: cve.iter().map(|value| value.to_ascii_uppercase()).collect(),
                ..crate::model::SearchIdentity::default()
            }),
            related_poc_ids: related_ids,
            related_entities,
            poc_source_integrity: string_field("source_integrity"),
            poc_verification_result: string_field("verification_result"),
            poc_dedup_status: string_field("dedup_status"),
            poc_independent_knowledge_status: string_field("independent_knowledge_status"),
            poc_missing_evidence: entry
                .get("missing_evidence")
                .and_then(serde_json::Value::as_array)
                .map(|values| {
                    values
                        .iter()
                        .filter_map(serde_json::Value::as_str)
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default(),
            poc_evidence_refs: entry
                .get("evidence_refs")
                .and_then(serde_json::Value::as_array)
                .map(|values| {
                    values
                        .iter()
                        .filter_map(serde_json::Value::as_str)
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default(),
            poc_review_gate: string_field("review_gate"),
            poc_source_urls: entry
                .get("source_urls")
                .and_then(serde_json::Value::as_array)
                .map(|values| {
                    values
                        .iter()
                        .filter_map(serde_json::Value::as_str)
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default(),
        });
    }
    Ok(())
}

/// 已审核 Nuclei 条目进入搜索索引的结构化投影。
#[derive(Default)]
struct NucleiIndexMetadata {
    severity: Option<String>,
    protocols: Vec<String>,
    cve_ids: Vec<String>,
    cwe_ids: Vec<String>,
    tags: Vec<String>,
}

/// 从已审核 Nuclei 条目的 meta_json 投影结构化字段；坏 JSON/非 Nuclei 条目归空。
fn normalize_cve_id(value: &str) -> Option<String> {
    let upper = value.trim().to_ascii_uppercase();
    let parts: Vec<&str> = upper.split('-').collect();
    (parts.len() == 3
        && parts[0] == "CVE"
        && parts[1].len() == 4
        && parts[1].bytes().all(|b| b.is_ascii_digit())
        && parts[2].len() >= 4
        && parts[2].bytes().all(|b| b.is_ascii_digit()))
    .then_some(upper)
}

fn parse_nuclei_index_metadata(raw: Option<&str>) -> NucleiIndexMetadata {
    let Some(meta) = raw.and_then(|value| serde_json::from_str::<serde_json::Value>(value).ok())
    else {
        return NucleiIndexMetadata::default();
    };
    let Some(template) = meta.get("nuclei_template") else {
        return NucleiIndexMetadata::default();
    };
    let strings = |value: Option<&serde_json::Value>| -> Vec<String> {
        let mut values = match value {
            Some(serde_json::Value::String(value)) => vec![value.clone()],
            Some(serde_json::Value::Array(values)) => values
                .iter()
                .filter_map(serde_json::Value::as_str)
                .map(str::to_string)
                .collect(),
            _ => Vec::new(),
        };
        values.retain(|value| !value.trim().is_empty());
        values.sort();
        values.dedup();
        values
    };
    let severity = template
        .get("severity")
        .and_then(serde_json::Value::as_str)
        .map(str::to_ascii_lowercase);
    let protocols = strings(template.get("protocols"));
    let tags = strings(template.get("tags"));
    let classification = template.get("classification");
    let cve_ids = strings(classification.and_then(|value| value.get("cve-id")))
        .into_iter()
        .map(|value| value.to_ascii_uppercase())
        .collect();
    let cwe_ids = strings(classification.and_then(|value| value.get("cwe-id")))
        .into_iter()
        .map(|value| value.to_ascii_uppercase())
        .collect();
    NucleiIndexMetadata {
        severity,
        protocols,
        cve_ids,
        cwe_ids,
        tags,
    }
}

/// source_location_json 的 file 字段抽取（坏 JSON 归空，宽容度与 parse_topics 同）。
fn parse_source_file(raw: &str) -> Option<String> {
    serde_json::from_str::<serde_json::Value>(raw)
        .ok()
        .and_then(|v| v.get("file").and_then(|f| f.as_str()).map(str::to_string))
        .filter(|s| !s.is_empty())
}

/// 解析 catalog topics JSON 数组（`["a","b"]`；空/坏值归一空数组，eri 同宽容度）。
fn parse_topics(raw: Option<&str>) -> Vec<String> {
    match raw {
        None => Vec::new(),
        Some(s) if s.trim() == "[]" || s.trim().is_empty() => Vec::new(),
        Some(s) => serde_json::from_str::<Vec<String>>(s).unwrap_or_default(),
    }
}

/// 查询期覆盖计分词元集：eri blob 口径（name + description + topics + language）。
/// source_sections 只参与 FTS 别名列召回，不参与覆盖计分（与 eri blob 语义一致，
/// 见 crate 级文档）。
pub(crate) fn coverage_blob(doc: &IndexDoc) -> String {
    format!(
        "{} {} {} {} {} {} {} {} {} {}",
        doc.name,
        doc.description.as_deref().unwrap_or(""),
        doc.topics.join(" "),
        doc.language.as_deref().unwrap_or(""),
        doc.evidence_level.as_deref().unwrap_or(""),
        doc.knowledge_lifecycle.as_deref().unwrap_or(""),
        doc.poc_category.as_deref().unwrap_or(""),
        doc.poc_severity.as_deref().unwrap_or(""),
        doc.poc_cve_id.as_deref().unwrap_or(""),
        doc.source_path.as_deref().unwrap_or("")
    )
}

/// 覆盖计分词元集（供 query 模块复用同一口径）。
pub(crate) fn coverage_token_set(doc: &IndexDoc) -> Vec<String> {
    coverage_tokens(&coverage_blob(doc))
}

#[cfg(test)]
pub(crate) mod testutil {
    use super::*;
    use database::{DbPath, PoolConfig};
    use std::time::Duration;

    /// 内存三库：platform（resource + resource_source）、catalog（github_metadata）、索引库。
    pub(crate) fn fixture_pools() -> (Pool, Pool, Pool) {
        let platform = Pool::open(PoolConfig {
            path: DbPath::Memory,
            readonly: false,
            max_connections: 1,
            busy_timeout: Duration::from_secs(1),
        })
        .unwrap();
        platform
            .with_conn(|c| {
                c.execute_batch(
                    "CREATE TABLE resource (
                       resource_id TEXT PRIMARY KEY, canonical_url TEXT NOT NULL,
                       resource_type TEXT NOT NULL, name TEXT NOT NULL, description TEXT,
                       lifecycle TEXT NOT NULL, governance TEXT NOT NULL,
                       quality_final_score REAL, quality_grade TEXT);
                     CREATE TABLE resource_source (
                       id INTEGER PRIMARY KEY, resource_id TEXT NOT NULL,
                       source_repository TEXT NOT NULL, source_file TEXT NOT NULL,
                       source_section TEXT NOT NULL, source_position INTEGER);
                     INSERT INTO resource VALUES
                       ('gh:a/fuzz-tool', 'https://github.com/a/fuzz-tool', 'github_repo',
                        'Fuzz Tool', 'a fuzzing tool for payloads 渗透', 'INDEXED', 'ACTIVE',
                        80.0, 'B'),
                       ('gh:b/cjk-tool', 'https://github.com/b/cjk-tool', 'github_repo',
                        '中文代理工具', '端口扫描与代理', 'INDEXED', 'ACTIVE', 70.0, 'C'),
                       ('gh:c/quarantined', 'https://github.com/c/q', 'github_repo',
                        'Quarantined', 'fuzzing but hidden', 'INDEXED', 'QUARANTINED',
                        95.0, 'A');
                     INSERT INTO resource_source VALUES
                       (1, 'gh:a/fuzz-tool', 'Hack-with-Github/Awesome-Hacking', 'README.md',
                        'Awesome Repositories', 24),
                       (2, 'gh:b/cjk-tool', 'Org/Chinese-List', 'README.md', '常用工具', 1);",
                )
            })
            .unwrap();
        let catalog = Pool::open(PoolConfig {
            path: DbPath::Memory,
            readonly: false,
            max_connections: 1,
            busy_timeout: Duration::from_secs(1),
        })
        .unwrap();
        catalog
            .with_conn(|c| {
                c.execute_batch(
                    "CREATE TABLE github_metadata (
                       resource_id TEXT PRIMARY KEY, topics TEXT, language TEXT,
                       license_status TEXT, maintenance_status TEXT);
                     INSERT INTO github_metadata VALUES
                       ('gh:a/fuzz-tool', '[\"fuzzing\",\"security\"]', 'Python',
                        'IDENTIFIED', 'ACTIVE'),
                       ('gh:c/quarantined', '[\"fuzzing\"]', null, null, null);",
                )
            })
            .unwrap();
        let index = Pool::open(PoolConfig {
            path: DbPath::Memory,
            readonly: false,
            max_connections: 1,
            busy_timeout: Duration::from_secs(1),
        })
        .unwrap();
        (platform, catalog, index)
    }

    /// 按 rowid 序取文档 ID（重建一致性断言用）。
    pub(crate) fn doc_ids(pool: &Pool) -> Vec<String> {
        pool.with_conn(|c| -> rusqlite::Result<Vec<String>> {
            let mut stmt = c.prepare("SELECT resource_id FROM search_index_doc ORDER BY rowid")?;
            let rows = stmt
                .query_map([], |r| r.get::<_, String>(0))?
                .collect::<Result<Vec<_>, rusqlite::Error>>()?;
            Ok(rows)
        })
        .unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use testutil::{doc_ids, fixture_pools};

    #[test]
    fn build_creates_doc_and_fts_rows() {
        let (p, cat, idx) = fixture_pools();
        let report = SearchIndex::build(&p, &cat, &idx).unwrap();
        assert_eq!(report.doc_count, 3);
        assert_eq!(report.fts_rows, 3);
        assert!(!report.rebuilt);
        assert_eq!(report.schema_version, INDEX_SCHEMA_VERSION);
        // 元数据落盘
        assert_eq!(
            SearchIndex::meta(&idx, "doc_count").unwrap().as_deref(),
            Some("3")
        );
        assert_eq!(SearchIndex::doc_count(&idx).unwrap(), 3);
        // topics 从 catalog 补齐 + license 缺证归一 UNKNOWN
        let n: i64 = idx
            .with_conn(|c| {
                c.query_row(
                    "SELECT COUNT(*) FROM search_index_doc WHERE license_status='UNKNOWN'",
                    [],
                    |r| r.get(0),
                )
            })
            .unwrap();
        assert_eq!(n, 2, "gh:b 无元数据行与 gh:c 的 NULL 值均归一 UNKNOWN");
        let topics: String = idx
            .with_conn(|c| {
                c.query_row(
                    "SELECT topics_json FROM search_index_doc WHERE resource_id='gh:a/fuzz-tool'",
                    [],
                    |r| r.get(0),
                )
            })
            .unwrap();
        assert!(topics.contains("fuzzing"));
    }

    #[test]
    fn rebuild_drops_and_rebuilds_identically() {
        let (p, cat, idx) = fixture_pools();
        let r1 = SearchIndex::build(&p, &cat, &idx).unwrap();
        let ids_before = doc_ids(&idx);
        let r2 = SearchIndex::rebuild(&p, &cat, &idx).unwrap();
        let ids_after = doc_ids(&idx);
        assert!(r2.rebuilt);
        assert_eq!(r1.doc_count, r2.doc_count);
        assert_eq!(r1.fts_rows, r2.fts_rows);
        assert_eq!(
            ids_before, ids_after,
            "rebuild must reproduce identical row set"
        );
    }

    #[test]
    fn unready_index_rejected() {
        let (_, _, idx) = fixture_pools();
        assert!(matches!(
            SearchIndex::doc_count(&idx),
            Err(SearchError::IndexNotReady(_))
        ));
    }

    #[test]
    fn fts_matches_ascii_and_cjk() {
        let (p, cat, idx) = fixture_pools();
        SearchIndex::build(&p, &cat, &idx).unwrap();
        let count = |term: &str| -> i64 {
            idx.with_conn(|c| {
                c.query_row(
                    "SELECT COUNT(*) FROM search_index_fts WHERE search_index_fts MATCH ?",
                    [term],
                    |r| r.get(0),
                )
            })
            .unwrap()
        };
        // ASCII 命中
        assert_eq!(
            count("\"fuzzing\""),
            2,
            "fuzz-tool(description+topics) + quarantined(desc+topics)"
        );
        // CJK 2-gram：文档 "中文代理工具" grams = 中文,文代,代理,理工
        assert_eq!(count("\"代理\""), 1, "CJK gram term hits cjk-tool");
        // CJK 短语（gram 连续）：文档含 文代,代理 相邻
        assert_eq!(
            count("\"文代 代理\""),
            1,
            "CJK consecutive-gram phrase hits"
        );
        // 别名列：章节名可命中（resource_source alias/name 并入）
        assert_eq!(count("\"repositories\""), 1);
        // 章节 "常用工具" grams = 常用,用工,工具
        assert_eq!(count("\"常用\""), 1, "alias column indexed via 2-gram");
    }

    /// SK-1 第三源夹具行（plugin_id, version, kind, key, title, desc, review）。
    type PluginRow<'a> = (
        &'a str,
        &'a str,
        &'a str,
        &'a str,
        &'a str,
        Option<&'a str>,
        &'a str,
    );

    /// SK-1 第三源夹具：platform 库补 plugin + knowledge_source_entry 两表。
    fn with_plugin_tables(platform: &Pool, rows: &[PluginRow<'_>]) {
        platform
            .with_conn(|c| {
                c.execute_batch(
                    "CREATE TABLE plugin (
                       plugin_id TEXT NOT NULL, version TEXT NOT NULL,
                       state TEXT NOT NULL, is_active INTEGER NOT NULL,
                       source_uninstalled INTEGER NOT NULL DEFAULT 0);
                     CREATE TABLE knowledge_source_entry (
                       plugin_id TEXT NOT NULL, plugin_version TEXT NOT NULL,
                       entry_kind TEXT NOT NULL, item_key TEXT NOT NULL,
                       title TEXT NOT NULL, description TEXT,
                       body TEXT NOT NULL DEFAULT '', canonical_url TEXT,
                       category_path_json TEXT NOT NULL DEFAULT '[]',
                       license_status TEXT NOT NULL DEFAULT 'UNKNOWN',
                       source_location_json TEXT NOT NULL DEFAULT '{}', meta_json TEXT,
                       source_commit TEXT, content_sha256 TEXT, metadata_sha256 TEXT,
                       review_status TEXT NOT NULL DEFAULT 'DISCOVERED',
                       upstream_missing INTEGER NOT NULL DEFAULT 0);",
                )
            })
            .unwrap();
        for (pid, ver, kind, key, title, desc, review) in rows {
            platform
                .with_conn(|c| {
                    c.execute(
                        "INSERT INTO knowledge_source_entry (plugin_id, plugin_version,
                            entry_kind, item_key, title, description, body, canonical_url,
                            review_status) VALUES (?1,?2,?3,?4,?5,?6,'',NULL,?7)",
                        rusqlite::params![pid, ver, kind, key, title, desc, review],
                    )
                })
                .unwrap();
        }
    }

    fn insert_plugin(platform: &Pool, id: &str, state: &str, active: i64) {
        insert_plugin_version(platform, id, "0.1.0", state, active);
    }

    fn insert_plugin_version(platform: &Pool, id: &str, version: &str, state: &str, active: i64) {
        platform
            .with_conn(|c| {
                c.execute(
                    "INSERT INTO plugin (plugin_id, version, state, is_active) VALUES (?1,?2,?3,?4)",
                    rusqlite::params![id, version, state, active],
                )
            })
            .unwrap();
    }

    #[test]
    fn approved_upstream_missing_nuclei_template_is_excluded_from_search() {
        let (platform, catalog, index) = fixture_pools();
        with_plugin_tables(
            &platform,
            &[(
                "knowledge.nuclei",
                "0.2.0",
                "nuclei_template",
                "http/cves/2025/CVE-2025-12345.yaml",
                "retired-template-marker",
                Some("Old template"),
                "APPROVED",
            )],
        );
        insert_plugin_version(&platform, "knowledge.nuclei", "0.2.0", "ENABLED", 1);
        platform
            .with_conn(|c| {
                c.execute(
                    "UPDATE knowledge_source_entry SET upstream_missing = 1
                     WHERE plugin_id = 'knowledge.nuclei'",
                    [],
                )
            })
            .unwrap();

        SearchIndex::build(&platform, &catalog, &index).unwrap();
        let engine = crate::query::SearchEngine::new(index);
        let query = crate::model::SearchQuery::new(vec!["retired-template-marker".to_string()]);

        assert_eq!(
            engine.search(&query).unwrap().total_matched,
            0,
            "approved rows removed from the current upstream snapshot must not be searchable"
        );
    }

    #[test]
    fn nuclei_structured_facets_are_indexed_and_exactly_filterable() {
        let (platform, catalog, index) = fixture_pools();
        with_plugin_tables(
            &platform,
            &[(
                "knowledge.nuclei",
                "0.2.0",
                "nuclei_template",
                "http/cves/2025/CVE-2025-12345.yaml",
                "Example SQL Injection",
                Some("SQL injection detection"),
                "APPROVED",
            )],
        );
        insert_plugin_version(&platform, "knowledge.nuclei", "0.2.0", "ENABLED", 1);
        platform
            .with_conn(|c| {
                c.execute(
                    "UPDATE knowledge_source_entry SET meta_json = ?1
                     WHERE plugin_id = 'knowledge.nuclei'",
                    [serde_json::json!({
                        "nuclei_template": {
                            "parse_status": "parsed",
                            "severity": "high",
                            "protocols": ["http"],
                            "tags": ["cve", "sqli"],
                            "classification": {
                                "cve-id": "CVE-2025-12345",
                                "cwe-id": "CWE-89"
                            }
                        }
                    })
                    .to_string()],
                )
            })
            .unwrap();

        SearchIndex::build(&platform, &catalog, &index).unwrap();
        let engine = crate::query::SearchEngine::new(index);
        let mut query = crate::model::SearchQuery::new(vec!["CVE-2025-12345".to_string()]);
        query.filters.nuclei_severity = Some("HIGH".to_string());
        query.filters.nuclei_protocol = Some("HTTP".to_string());
        query.filters.cve_id = Some("cve-2025-12345".to_string());
        query.filters.cwe_id = Some("cwe-89".to_string());
        let hits = engine.search(&query).unwrap();
        assert_eq!(hits.total_matched, 1);
        assert_eq!(hits.hits[0].resource_type, "nuclei_template");
        assert_eq!(hits.hits[0].nuclei_severity.as_deref(), Some("high"));
        assert_eq!(hits.hits[0].nuclei_protocols, vec!["http"]);
        assert_eq!(hits.hits[0].nuclei_cve_ids, vec!["CVE-2025-12345"]);
        assert_eq!(hits.hits[0].nuclei_cwe_ids, vec!["CWE-89"]);
        assert!(hits.hits[0].topics.contains(&"sqli".to_string()));

        query.filters.cve_id = Some("CVE-2024-00001".to_string());
        assert_eq!(engine.search(&query).unwrap().total_matched, 0);
    }

    #[test]
    fn rtis_curated_pocs_are_indexed_with_overlay_facets_and_relations() {
        use database::{DbPath, PoolConfig};
        use std::time::Duration;
        let (platform, catalog, index) = fixture_pools();
        let rtis = Pool::open(PoolConfig {
            path: DbPath::Memory,
            readonly: false,
            max_connections: 1,
            busy_timeout: Duration::from_secs(1),
        })
        .unwrap();
        rtis.with_conn(|c| {
            c.execute_batch(
                "CREATE TABLE poc (id INTEGER PRIMARY KEY, code TEXT NOT NULL, title TEXT NOT NULL,
                kind TEXT, category TEXT, cve TEXT, component TEXT, severity TEXT, tags TEXT,
                versions TEXT, language TEXT, source TEXT, source_url TEXT, verified INTEGER DEFAULT 0, updated_at TEXT);
             CREATE TABLE k_meta (knowledge_id INTEGER PRIMARY KEY, evidence_level TEXT, lifecycle TEXT, verification_basis TEXT);
             INSERT INTO poc (id,code,title,kind,category,cve,component,severity,tags,source,verified) VALUES
                (19, 'gateway-openapi', 'Spring Gateway OpenAPI exposure', 'poc', 'unauthorized',
                 'CVE-2025-9999', 'Spring Cloud Gateway', 'high', 'spring,gateway,openapi','self',1),
                (31, 'h5-token', 'H5 token signature validation', 'poc', 'auth-bypass',
                 NULL, 'H5 API', 'high', 'token,signature','self',0),
                (42, 'log4j-local-check', 'Log4j CVE-2021-44228 local verification helper', 'poc', 'rce',
                 'CVE-2021-44228', 'Apache Log4j', 'critical', 'log4j,java,rce','self',1);",
            )
        })
        .unwrap();
        let overlay_path =
            std::env::temp_dir().join(format!("rip-curation-test-{}.json", std::process::id()));
        let overlay = serde_json::json!({
            "schema_version": 1,
            "entries": [
                {"knowledge_id":19,"code":"gateway-openapi","evidence_level":"K2","lifecycle":"Researched",
                 "source_path":"integrations/rtis/pocs/gateway-openapi","source_commit":"abc123",
                 "technology_tags":"[\"spring\",\"gateway\"]","method_tags":"[\"openapi\"]",
                 "original_category":"poc:unauthorized","normalized_type":"Case"},
                {"knowledge_id":31,"code":"h5-token","evidence_level":"K0","lifecycle":"Candidate",
                 "source_integrity":"missing","verification_result":"candidate_not_verified",
                 "dedup_status":"independent","independent_knowledge_status":"independent",
                 "missing_evidence":["raw_request_response","negative_control"],
                 "evidence_refs":["integrations/rtis/pocs/h5-token/NOTES.md"],
                 "source_urls":["https://example.org/h5-token-reference"],
                 "review_gate":"Recover paired request/response and a negative control."}
            ],
            "external_entities":[{"code":"case-gateway-audit","entity_type":"case_group","title":"Gateway audit case group"}],
            "relations": [
                {"src":"gateway-openapi","dst":"h5-token","type":"supports","confidence":0.8},
                {"src":"gateway-openapi","dst":"case-gateway-audit","type":"has_case","confidence":0.9}
            ]
        });
        std::fs::write(&overlay_path, serde_json::to_vec(&overlay).unwrap()).unwrap();
        let report = SearchIndex::rebuild_with_rtis_curation(
            &platform,
            &catalog,
            &rtis,
            &overlay_path,
            &index,
        )
        .unwrap();
        assert_eq!(report.doc_count, 6);
        let engine = crate::query::SearchEngine::new(index);
        let mut query = crate::model::SearchQuery::new(vec!["Gateway".to_string()]);
        query.filters.resource_type = Some("rtis_poc".to_string());
        query.filters.evidence_level = Some("K2".to_string());
        query.filters.knowledge_lifecycle = Some("Researched".to_string());
        query.filters.poc_severity = Some("HIGH".to_string());
        query.filters.poc_category = Some("unauthorized".to_string());
        query.filters.poc_cve_id = Some("cve-2025-9999".to_string());
        let hits = engine.search(&query).unwrap();
        assert_eq!(hits.total_matched, 1);
        let hit = &hits.hits[0];
        assert_eq!(hit.resource_id, "rtis:poc:19");
        assert_eq!(hit.evidence_level.as_deref(), Some("K2"));
        assert_eq!(hit.knowledge_lifecycle.as_deref(), Some("Researched"));
        assert_eq!(
            hit.source_path.as_deref(),
            Some("integrations/rtis/pocs/gateway-openapi")
        );
        assert_eq!(hit.source_commit.as_deref(), Some("abc123"));
        let identity = hit.identity.as_ref().unwrap();
        assert_eq!(identity.entity_type.as_deref(), Some("poc"));
        assert_eq!(identity.canonical_key.as_deref(), Some("rtis:poc:19"));
        assert_eq!(identity.source_system.as_deref(), Some("rtis"));
        assert_eq!(identity.source_entity_id.as_deref(), Some("19"));
        assert_eq!(identity.vulnerability_ids, vec!["CVE-2025-9999"]);
        assert_eq!(hit.related_poc_ids, vec!["supports:31"]);
        assert_eq!(hit.related_entities, vec!["has_case:case-gateway-audit"]);

        let mut queue = crate::model::SearchQuery::new(Vec::new());
        queue.filters.resource_type = Some("rtis_poc".to_string());
        queue.filters.poc_missing_evidence = Some(true);
        queue.filters.poc_source_integrity = Some("missing".to_string());
        queue.filters.poc_verification_result = Some("candidate_not_verified".to_string());
        queue.filters.poc_dedup_status = Some("independent".to_string());
        let queue_hits = engine.search(&queue).unwrap();
        assert_eq!(queue_hits.total_matched, 1);
        let candidate = &queue_hits.hits[0];
        assert_eq!(candidate.resource_id, "rtis:poc:31");
        assert_eq!(
            candidate.poc_missing_evidence,
            vec!["raw_request_response", "negative_control"]
        );
        assert_eq!(
            candidate.poc_evidence_refs,
            vec!["integrations/rtis/pocs/h5-token/NOTES.md"]
        );
        assert_eq!(
            candidate.poc_review_gate.as_deref(),
            Some("Recover paired request/response and a negative control.")
        );
        assert_eq!(candidate.poc_source_integrity.as_deref(), Some("missing"));
        assert_eq!(
            candidate.poc_verification_result.as_deref(),
            Some("candidate_not_verified")
        );
        assert_eq!(candidate.poc_dedup_status.as_deref(), Some("independent"));

        let mut candidate_query = crate::model::SearchQuery::new(vec!["Log4j".to_string()]);
        candidate_query.filters.resource_type = Some("rtis_weapon_candidate".to_string());
        candidate_query.filters.poc_cve_id = Some("CVE-2021-44228".to_string());
        assert_eq!(
            engine.search(&candidate_query).unwrap().total_matched,
            0,
            "candidate assets are excluded by default"
        );
        candidate_query.filters.include_candidates = true;
        let candidate_hits = engine.search(&candidate_query).unwrap();
        assert_eq!(candidate_hits.total_matched, 1);
        let candidate = &candidate_hits.hits[0];
        assert_eq!(candidate.resource_id, "rtis:weapon-candidate:42");
        assert_eq!(candidate.governance_status, "CANDIDATE");
        assert_eq!(candidate.lifecycle_status, "DISCOVERED");
        assert_eq!(
            candidate.description, None,
            "raw descriptions and PoC bodies are not indexed for unreviewed RTIS records"
        );
        assert_eq!(
            candidate.poc_verification_result.as_deref(),
            Some("legacy_claim_only")
        );
        assert!(candidate
            .poc_missing_evidence
            .contains(&"source_url_missing".to_string()));
        std::fs::remove_file(overlay_path).unwrap();
    }

    #[test]
    fn third_source_indexes_only_the_unique_active_plugin_version() {
        let (p, cat, idx) = fixture_pools();
        with_plugin_tables(
            &p,
            &[
                (
                    "knowledge.versioned",
                    "0.1.0",
                    "catalog_entry",
                    "https://a/old",
                    "Old version",
                    Some("old body"),
                    "APPROVED",
                ),
                (
                    "knowledge.versioned",
                    "0.2.0",
                    "catalog_entry",
                    "https://a/new",
                    "Current version",
                    Some("new body"),
                    "APPROVED",
                ),
            ],
        );
        insert_plugin_version(&p, "knowledge.versioned", "0.1.0", "DISABLED", 0);
        insert_plugin_version(&p, "knowledge.versioned", "0.2.0", "ENABLED", 1);
        SearchIndex::build(&p, &cat, &idx).unwrap();
        let ids = doc_ids(&idx);
        assert!(ids
            .iter()
            .any(|id| id.contains("@0.2.0:catalog_entry:https://a/new")));
        assert!(!ids
            .iter()
            .any(|id| id.contains("@0.1.0:catalog_entry:https://a/old")));
    }

    #[test]
    fn third_source_can_be_loaded_from_a_separate_plugin_database() {
        use database::{DbPath, PoolConfig};
        use std::time::Duration;

        let (platform, catalog, index) = fixture_pools();
        let plugin = Pool::open(PoolConfig {
            path: DbPath::Memory,
            readonly: false,
            max_connections: 1,
            busy_timeout: Duration::from_secs(1),
        })
        .unwrap();
        with_plugin_tables(
            &plugin,
            &[(
                "knowledge.separate",
                "0.3.0",
                "catalog_entry",
                "https://example.test/separate",
                "Separate plugin source",
                Some("separate database content"),
                "APPROVED",
            )],
        );
        insert_plugin_version(&plugin, "knowledge.separate", "0.3.0", "ENABLED", 1);

        let report =
            SearchIndex::rebuild_with_plugin_db(&platform, &plugin, &catalog, &index).unwrap();
        assert_eq!(
            report.doc_count, 4,
            "three platform resources + one plugin row"
        );
        let ids = doc_ids(&index);
        assert!(ids.iter().any(|id| {
            id == "kse:knowledge.separate@0.3.0:catalog_entry:https://example.test/separate"
        }));
        let engine = crate::query::SearchEngine::new(index);
        let hits = engine
            .search(&crate::model::SearchQuery::new(
                vec!["separate".to_string()],
            ))
            .unwrap();
        assert_eq!(hits.total_matched, 1);
        assert_eq!(hits.hits[0].resource_type, "catalog_entry");
    }

    #[test]
    fn third_source_indexes_approved_and_candidates_but_hides_candidates_by_default() {
        let (p, cat, idx) = fixture_pools();
        with_plugin_tables(
            &p,
            &[
                (
                    "knowledge.a",
                    "0.1.0",
                    "catalog_entry",
                    "https://a/x",
                    "A 工具目录",
                    Some("链接描述"),
                    "APPROVED",
                ),
                (
                    "knowledge.a",
                    "0.1.0",
                    "document_markdown",
                    "docs/g.md",
                    "文档标题",
                    Some("文档摘要"),
                    "APPROVED",
                ),
                (
                    "knowledge.a",
                    "0.1.0",
                    "catalog_entry",
                    "https://a/y",
                    "未审条目",
                    Some("d"),
                    "DISCOVERED",
                ),
                (
                    "knowledge.b",
                    "0.1.0",
                    "catalog_entry",
                    "https://b/z",
                    "B 条目",
                    Some("d"),
                    "APPROVED",
                ),
            ],
        );
        insert_plugin(&p, "knowledge.a", "ENABLED", 1);
        insert_plugin(&p, "knowledge.b", "DISABLED", 1);
        SearchIndex::build(&p, &cat, &idx).unwrap();
        let ids = doc_ids(&idx);
        assert_eq!(
            ids,
            vec![
                "gh:a/fuzz-tool".to_string(),
                "gh:b/cjk-tool".to_string(),
                "gh:c/quarantined".to_string(),
                "kse:knowledge.a@0.1.0:catalog_entry:https://a/x".to_string(),
                "kse:knowledge.a@0.1.0:catalog_entry:https://a/y".to_string(),
                "kse:knowledge.a@0.1.0:document_markdown:docs/g.md".to_string(),
            ],
            "third source indexes APPROVED and DISCOVERED rows from visible plugins; DISABLED plugin rows remain absent"
        );
        let engine = crate::query::SearchEngine::new(idx.clone());
        let default_hits = engine
            .search(&crate::model::SearchQuery::new(
                vec!["未审条目".to_string()],
            ))
            .unwrap();
        assert_eq!(
            default_hits.total_matched, 0,
            "candidate rows are excluded from default retrieval"
        );
        let mut candidate_query = crate::model::SearchQuery::new(vec!["未审条目".to_string()]);
        candidate_query.filters.include_candidates = true;
        let candidate_hits = engine.search(&candidate_query).unwrap();
        assert_eq!(candidate_hits.total_matched, 1);
        assert_eq!(candidate_hits.hits[0].governance_status, "CANDIDATE");
        assert_eq!(candidate_hits.hits[0].lifecycle_status, "DISCOVERED");
        // 第三源条目参与 FTS（name + description + topics/category_path）。
        let count = |term: &str| -> i64 {
            idx.with_conn(|c| {
                c.query_row(
                    "SELECT COUNT(*) FROM search_index_fts WHERE search_index_fts MATCH ?",
                    [term],
                    |r| r.get(0),
                )
            })
            .unwrap()
        };
        // CJK 2-gram：标题 "文档标题" grams = 文档,档标,标题。
        assert_eq!(count("\"文档\""), 1, "CJK title 2-gram indexed");
        // 禁用后再 rebuild：knowledge.a 条目撤出匹配面。
        p.with_conn(|c| {
            c.execute(
                "UPDATE plugin SET state = 'DISABLED' WHERE plugin_id = 'knowledge.a'",
                [],
            )
        })
        .unwrap();
        SearchIndex::rebuild(&p, &cat, &idx).unwrap();
        assert_eq!(
            doc_ids(&idx),
            vec![
                "gh:a/fuzz-tool".to_string(),
                "gh:b/cjk-tool".to_string(),
                "gh:c/quarantined".to_string(),
            ],
            "rebuild after disable removes plugin entries (disable hides from match surface)"
        );
    }

    #[test]
    fn third_source_absent_tables_skip_cleanly() {
        // 无 knowledge_source_entry/plugin 表（旧库形态）：双源行为不变。
        let (p, cat, idx) = fixture_pools();
        let report = SearchIndex::build(&p, &cat, &idx).unwrap();
        assert_eq!(report.doc_count, 3);
    }

    #[test]
    fn kse_source_identity_and_vulnerability_ids_are_projected() {
        let (platform, catalog, index) = fixture_pools();
        with_plugin_tables(
            &platform,
            &[
                (
                    "knowledge.nuclei-templates",
                    "0.1.1",
                    "nuclei_template",
                    "http/cves/2025/CVE-2025-12345.yaml",
                    "Example Nuclei detection template",
                    Some("Detects Example Product vulnerability"),
                    "APPROVED",
                ),
                (
                    "knowledge.pocindex-kev",
                    "0.1.0",
                    "catalog_entry",
                    "CVE-2025-12346",
                    "CVE-2025-12346 — PoC Index risk enrichment",
                    Some("Risk enrichment record"),
                    "APPROVED",
                ),
                (
                    "knowledge.cisa-kev",
                    "0.1.2",
                    "catalog_entry",
                    "CVE-2025-12347",
                    "CVE-2025-12347 — CISA KEV vulnerability",
                    Some("Known exploited vulnerability"),
                    "APPROVED",
                ),
            ],
        );
        insert_plugin_version(
            &platform,
            "knowledge.nuclei-templates",
            "0.1.1",
            "ENABLED",
            1,
        );
        insert_plugin_version(&platform, "knowledge.pocindex-kev", "0.1.0", "ENABLED", 1);
        insert_plugin_version(&platform, "knowledge.cisa-kev", "0.1.2", "ENABLED", 1);
        platform
            .with_conn(|c| -> rusqlite::Result<()> {
                c.execute(
                    "UPDATE knowledge_source_entry
                     SET source_commit=?1, content_sha256=?2, metadata_sha256=?3,
                         source_location_json=?4, meta_json=?5
                     WHERE plugin_id='knowledge.nuclei-templates'",
                    rusqlite::params![
                        "a".repeat(40),
                        "b".repeat(64),
                        "c".repeat(64),
                        r#"{"file":"http/cves/2025/CVE-2025-12345.yaml"}"#,
                        r#"{"nuclei_template":{"template_id":"example-cve-2025-12345","classification":{"cve-id":"CVE-2025-12345"}}}"#,
                    ],
                )?;
                c.execute(
                    "UPDATE knowledge_source_entry
                     SET source_commit=?1, content_sha256=?2, metadata_sha256=?3,
                         source_location_json=?4, meta_json=?5
                     WHERE plugin_id='knowledge.pocindex-kev'",
                    rusqlite::params![
                        "d".repeat(40),
                        "e".repeat(64),
                        "f".repeat(64),
                        r#"{"file":"pocindex-kev.json"}"#,
                        r#"{"pocindex_record":{"cve_id":"CVE-2025-12346"}}"#,
                    ],
                )?;
                Ok(())
            })
            .unwrap();

        SearchIndex::build(&platform, &catalog, &index).unwrap();
        let engine = crate::query::SearchEngine::new(index);

        let mut nuclei_query = crate::model::SearchQuery::new(Vec::new());
        nuclei_query.filters.vulnerability_id = Some("cve-2025-12345".to_string());
        let nuclei = engine.search(&nuclei_query).unwrap();
        assert_eq!(nuclei.total_matched, 1);
        let nuclei_hit = &nuclei.hits[0];
        assert_eq!(nuclei_hit.resource_type, "nuclei_template");
        assert_eq!(
            nuclei_hit.source_commit.as_deref(),
            Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
        );
        let identity = nuclei_hit.identity.as_ref().unwrap();
        assert_eq!(identity.entity_type.as_deref(), Some("detection_template"));
        assert_eq!(
            identity.canonical_key.as_deref(),
            Some("nuclei:example-cve-2025-12345")
        );
        assert_eq!(
            identity.source_system.as_deref(),
            Some("knowledge.nuclei-templates")
        );
        assert_eq!(
            identity.source_entity_id.as_deref(),
            Some("http/cves/2025/CVE-2025-12345.yaml")
        );
        assert_eq!(identity.source_version.as_deref(), Some("0.1.1"));
        assert_eq!(identity.content_sha256.as_ref(), Some(&"b".repeat(64)));
        assert_eq!(identity.metadata_sha256.as_ref(), Some(&"c".repeat(64)));
        assert_eq!(
            identity.vulnerability_ids,
            vec!["CVE-2025-12345".to_string()]
        );

        let mut key_query = crate::model::SearchQuery::new(Vec::new());
        key_query.filters.canonical_key = Some("nuclei:example-cve-2025-12345".to_string());
        assert_eq!(engine.search(&key_query).unwrap().total_matched, 1);

        let mut pocindex_query = crate::model::SearchQuery::new(Vec::new());
        pocindex_query.filters.vulnerability_id = Some("CVE-2025-12346".to_string());
        let pocindex = engine.search(&pocindex_query).unwrap();
        assert_eq!(pocindex.total_matched, 1);
        let identity = pocindex.hits[0].identity.as_ref().unwrap();
        assert_eq!(identity.entity_type.as_deref(), Some("vulnerability"));
        assert_eq!(
            identity.canonical_key.as_deref(),
            Some("cve:CVE-2025-12346")
        );

        let mut cisa_query = crate::model::SearchQuery::new(Vec::new());
        cisa_query.filters.vulnerability_id = Some("CVE-2025-12347".to_string());
        let cisa = engine.search(&cisa_query).unwrap();
        assert_eq!(cisa.total_matched, 1);
        let cisa_identity = cisa.hits[0].identity.as_ref().unwrap();
        assert_eq!(cisa_identity.entity_type.as_deref(), Some("vulnerability"));
        assert_eq!(
            cisa_identity.canonical_key.as_deref(),
            Some("cve:CVE-2025-12347")
        );
        assert_eq!(cisa_identity.vulnerability_ids, vec!["CVE-2025-12347"]);
    }
}
