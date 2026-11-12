//! FTS5 派生索引层（全部表 `search_index_` 前缀，可重建）。
//!
//! 落地形态（任务口径二选一）：**独立索引库**（`platform-search.db`），而非在
//! platform-dryrun.db 内建派生表。理由：
//! 1. platform-dryrun.db 是 M1 ETL 的对账产物（verify.py 100% 红线），在其内
//!    建表会改变 schema 并复杂化复核；独立索引库保持 M1 基线不变。
//! 2. §8.2.3 可重建纪律的最强形态：整个索引库可删可重灌，写面隔离。
//!
//! 数据源（双源只读）：
//! - platform 库：`resource`（正文 + 质量 + 双轴状态）、`resource_source`
//!   （alias/name 并入：source_section / source_repository 折叠进 aliases 列）
//! - production catalog 库：`github_metadata`（topics / language / license_status /
//!   maintenance_status，补齐 M2 报告差异表 #2 的 topics 缺口）
//!
//! FTS5 形态：contentless（content=''）虚拟表，bm25 可用，不支持增量删除，
//! 与"整库重建"策略一致（更新 = 重建，成本 O(文档数)，catalog 规模可接受）。

use crate::error::SearchError;
use crate::model::{BuildReport, IndexDoc};
use crate::tokenizer::{coverage_tokens, index_text};
use database::{rusqlite, DbResult, Pool};
use rusqlite::OptionalExtension;
use std::time::Instant;

/// 索引库 schema 版本（结构变更即升版，查询端拒绝旧版）。
/// v2（SK-1/P6-9）：数据源双源扩三源（platform resource/resource_source +
/// catalog github_metadata + platform knowledge_source_entry 审核通过行），
/// 旧索引必须整库重建。
pub const INDEX_SCHEMA_VERSION: i64 = 2;

/// 索引库 DDL（头注声明可重建，MODULE_BOUNDARIES §8.2.3）。
pub const INDEX_SCHEMA: &str = r#"
-- 可重建派生索引（MODULE_BOUNDARIES §8.2.3）：本库全部内容可由
-- platform-dryrun.db（resource + resource_source）与 production catalog.db
-- （github_metadata）全量重灌恢复；损坏或升版即整库删除重建，不做增量维护。
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

    /// 全量重建：删除既有索引表后从双源重灌（可重建纪律的显式入口）。
    pub fn rebuild(
        platform: &Pool,
        catalog: &Pool,
        index: &Pool,
    ) -> Result<BuildReport, SearchError> {
        Self::build_inner(platform, catalog, index, true)
    }

    /// 构建索引（目标库应为空库或从未建过索引；已有索引请走 [`SearchIndex::rebuild`]）。
    pub fn build(
        platform: &Pool,
        catalog: &Pool,
        index: &Pool,
    ) -> Result<BuildReport, SearchError> {
        Self::build_inner(platform, catalog, index, false)
    }

    fn build_inner(
        platform: &Pool,
        catalog: &Pool,
        index: &Pool,
        drop_existing: bool,
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

        let docs = load_docs(platform, catalog)?;
        let built_at = domain::time::now_iso();
        let mut fts_rows = 0usize;
        index.with_tx(|c| -> rusqlite::Result<()> {
            for d in &docs {
                c.execute(
                    "INSERT INTO search_index_doc(resource_id, name, description,
                       canonical_url, resource_type, lifecycle, governance,
                       quality_final_score, quality_grade, license_status, maintenance_status,
                       topics_json, language, source_sections_json, source_repos_json, built_at)
                     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
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
                            "{} {}",
                            d.source_sections.join(" "),
                            d.source_repositories.join(" ")
                        )),
                        index_text(&d.topics.join(" ")),
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

/// 校验索引就绪（meta 存在 + schema 版本一致）。索引库缺表/缺 meta 一律视为
/// 未就绪（需要 build/rebuild），不透传底层 Db 错误。
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
        Some(v) if v == INDEX_SCHEMA_VERSION => Ok(()),
        Some(v) => Err(SearchError::IndexNotReady(format!(
            "index schema_version={v}, engine expects {INDEX_SCHEMA_VERSION}: rebuild required"
        ))),
        None => Err(SearchError::IndexNotReady(
            "search_index_meta missing: index not built (run build/rebuild first)".to_string(),
        )),
    }
}

/// 双源只读加载并折叠为 IndexDoc（platform 为主表，catalog/github_metadata 左联）。
fn load_docs(platform: &Pool, catalog: &Pool) -> DbResult<Vec<IndexDoc>> {
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
    load_plugin_entries(platform, &mut docs)?;

    Ok(docs)
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
        // 可见插件集（缺 plugin 表 = 全部不可见，fail-closed）。
        let mut visible: std::collections::HashSet<String> = std::collections::HashSet::new();
        if plugin_table.as_deref() == Some("plugin") {
            let mut stmt = c.prepare(
                "SELECT DISTINCT plugin_id FROM plugin
                 WHERE is_active = 1
                   AND state IN ('ENABLED','LOADED','ACTIVE')
                   AND source_uninstalled = 0",
            )?;
            let rows = stmt
                .query_map([], |r| r.get::<_, String>(0))?
                .collect::<Result<Vec<_>, rusqlite::Error>>()?;
            visible.extend(rows);
        }
        if visible.is_empty() {
            return Ok(());
        }
        let mut stmt = c.prepare(
            "SELECT plugin_id, plugin_version, entry_kind, item_key, title, description,
                    body, canonical_url, category_path_json, license_status,
                    source_location_json
             FROM knowledge_source_entry
             WHERE review_status = 'APPROVED'
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
        ) in rows {
            if !visible.contains(&plugin_id) {
                continue;
            }
            let category_path = parse_topics(Some(category_path_json.as_str()));
            let source_file = parse_source_file(&source_location_json);
            let snippet: String = body.chars().take(200).collect();
            docs.push(IndexDoc {
                resource_id: format!("kse:{plugin_id}@{plugin_version}:{entry_kind}:{item_key}"),
                name: title,
                description: description.or_else(|| {
                    if snippet.trim().is_empty() {
                        None
                    } else {
                        Some(snippet)
                    }
                }),
                canonical_url: canonical_url.unwrap_or_else(|| format!("plugin://{plugin_id}")),
                resource_type: entry_kind,
                lifecycle: "INDEXED".to_string(),
                governance: "ACTIVE".to_string(),
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
                source_sections: source_file.into_iter().collect(),
                source_repositories: vec![format!("plugin:{plugin_id}")],
            });
        }
        Ok(())
    })
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
        "{} {} {} {}",
        doc.name,
        doc.description.as_deref().unwrap_or(""),
        doc.topics.join(" "),
        doc.language.as_deref().unwrap_or("")
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
                       source_location_json TEXT NOT NULL DEFAULT '{}',
                       review_status TEXT NOT NULL DEFAULT 'DISCOVERED');",
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
        platform
            .with_conn(|c| {
                c.execute(
                    "INSERT INTO plugin (plugin_id, version, state, is_active) VALUES (?1,'0.1.0',?2,?3)",
                    rusqlite::params![id, state, active],
                )
            })
            .unwrap();
    }

    #[test]
    fn third_source_indexes_only_approved_and_visible() {
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
                "kse:knowledge.a@0.1.0:document_markdown:docs/g.md".to_string(),
            ],
            "third source admits only APPROVED + visible-plugin rows (disable hides knowledge.b; DISCOVERED hidden)"
        );
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
}
