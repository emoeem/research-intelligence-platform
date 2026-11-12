//! 插件知识源（SK-1/P6-9 实装，替换 Phase 6 前空实现；设计稿 §4.3 候选
//! 第二类与 §7.2 失效信号；裁决 docs/phase6/SK1-INGEST-ADR.md 第七节）。
//!
//! 数据面：platform 库 `knowledge_source_entry`（011 迁移，SK-1 三类）。
//! 准入闸（candidates/load 逐查询重读，无进程内缓存即无失效信号缺口）：
//! - 内容闸：`review_status = 'APPROVED'`（审核未过的 DISCOVERED/REJECTED
//!   行对模型面不可见）；
//! - 插件闸：plugin 行 `is_active = 1 AND state IN ('ENABLED','LOADED',
//!   'ACTIVE') AND source_uninstalled = 0`（禁用即撤出匹配面，验收口径）。
//!
//! source 词表：`plugin:<id>` 前缀形态（008 source 列注释与 domain::
//! context::ContentRef 同口径），一个 [`PluginSource`] 实例承载一个插件，
//! [`PluginSource::mount_all`] 按库内插件清单挂载（含禁用插件：候选空集
//! 是合法结果，设计稿 §9.2 第 4 条）。
//!
//! 三级加载（计划 §8:437-443 层表）：L1 目录元数据走 assemble 的 catalog
//! digest 计数（kind=Plugin）；L2 摘要（description / 正文前 200 字符）；
//! L3 按需正文展开（[`ContextSource::load`] 返回全文，expand 记账在
//! assemble）。内容隔离：插件正文只作 L2/L3 参考内容，不入 L0 规则层。

use crate::error::ContextBrokerError;
use crate::source::{Candidate, Content, ContextSource};
use crate::{ContentRef, ContextItemKind, ContextLayer, IntentQuery};
use database::rusqlite::OptionalExtension;
use database::{rusqlite, DbResult, Pool};

/// 测试便捷构造（内存库；表缺失 = 空候选，fail-closed）。
#[cfg(test)]
pub(crate) fn memory_pool() -> Pool {
    Pool::open(database::PoolConfig {
        path: database::DbPath::Memory,
        readonly: false,
        max_connections: 1,
        busy_timeout: std::time::Duration::from_secs(5),
    })
    .unwrap()
}

/// 源 id 前缀（008 source 词表 `plugin:<id>` 前缀形态）。
pub const PLUGIN_SOURCE_PREFIX: &str = "plugin:";

/// L2 摘要的正文截断长度（字符）。
const SUMMARY_SNIPPET_CHARS: usize = 200;

/// 单插件知识源（pool 只读使用；写面归 plugin-runtime 摄取/审核管道）。
#[derive(Clone)]
pub struct PluginSource {
    pool: Pool,
    plugin_id: String,
    /// `plugin:<plugin_id>`（008 source 词表；构造即定，id() 借用返回）。
    source_id: String,
}

impl PluginSource {
    /// 构造单插件源（source id = `plugin:<plugin_id>`）。
    pub fn new(pool: Pool, plugin_id: impl Into<String>) -> Self {
        let plugin_id = plugin_id.into();
        let source_id = format!("{PLUGIN_SOURCE_PREFIX}{plugin_id}");
        Self {
            pool,
            plugin_id,
            source_id,
        }
    }

    /// 库内全部插件各挂一源（含禁用/未审插件：候选空集合法；表缺失 =
    /// 空挂载，旧库零回归）。
    pub fn mount_all(pool: &Pool) -> Vec<PluginSource> {
        let ids: Vec<String> = pool
            .with_conn(|c| -> rusqlite::Result<Vec<String>> {
                let exists: Option<String> = c
                    .query_row(
                        "SELECT name FROM sqlite_master WHERE type='table' AND name='plugin'",
                        [],
                        |r| r.get(0),
                    )
                    .optional()?;
                if exists.is_none() {
                    return Ok(Vec::new());
                }
                let mut stmt =
                    c.prepare("SELECT DISTINCT plugin_id FROM plugin ORDER BY plugin_id")?;
                let rows = stmt
                    .query_map([], |r| r.get::<_, String>(0))?
                    .collect::<Result<Vec<_>, rusqlite::Error>>()?;
                Ok(rows)
            })
            .unwrap_or_default();
        ids.into_iter()
            .map(|id| PluginSource::new(pool.clone(), id))
            .collect()
    }

    /// source id（`plugin:<id>`）。
    pub fn source_id(&self) -> &str {
        &self.source_id
    }

    /// 承载的插件 id。
    pub fn plugin_id(&self) -> &str {
        &self.plugin_id
    }

    /// 插件可见性（逐查询重读：禁用/卸载立即生效，无缓存失效窗口）。
    fn plugin_visible(&self) -> DbResult<bool> {
        self.pool.with_conn(|c| -> rusqlite::Result<bool> {
            let exists: Option<String> = c
                .query_row(
                    "SELECT name FROM sqlite_master WHERE type='table' AND name='plugin'",
                    [],
                    |r| r.get(0),
                )
                .optional()?;
            if exists.is_none() {
                return Ok(false); // fail-closed：无插件表即不可见
            }
            let n: i64 = c.query_row(
                "SELECT COUNT(*) FROM plugin
                 WHERE plugin_id = ?1 AND is_active = 1
                   AND state IN ('ENABLED','LOADED','ACTIVE')
                   AND source_uninstalled = 0",
                [self.plugin_id.as_str()],
                |r| r.get(0),
            )?;
            Ok(n > 0)
        })
    }

    /// 审核通过条目（kind/item_key/标题/描述/正文/哈希/版本/URL）。
    #[allow(clippy::type_complexity)]
    fn approved_entries(
        &self,
    ) -> DbResult<
        Vec<(
            String,
            String,
            String,
            Option<String>,
            String,
            String,
            String,
            Option<String>,
        )>,
    > {
        self.pool.with_conn(|c| -> rusqlite::Result<Vec<(String, String, String, Option<String>, String, String, String, Option<String>)>> {
            let exists: Option<String> = c
                .query_row(
                    "SELECT name FROM sqlite_master WHERE type='table' AND name='knowledge_source_entry'",
                    [],
                    |r| r.get(0),
                )
                .optional()?;
            if exists.is_none() {
                return Ok(Vec::new());
            }
            let mut stmt = c.prepare(
                "SELECT entry_kind, item_key, title, description, body, content_sha256,
                        plugin_version, canonical_url
                 FROM knowledge_source_entry
                 WHERE plugin_id = ?1 AND review_status = 'APPROVED'
                 ORDER BY entry_kind, item_key",
            )?;
            let rows = stmt
                .query_map([self.plugin_id.as_str()], |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, String>(2)?,
                        r.get::<_, Option<String>>(3)?,
                        r.get::<_, String>(4)?,
                        r.get::<_, String>(5)?,
                        r.get::<_, String>(6)?,
                        r.get::<_, Option<String>>(7)?,
                    ))
                })?
                .collect::<Result<Vec<_>, rusqlite::Error>>()?;
            Ok(rows)
        })
    }
}

/// 关键词命中（ASCII 词忽略大小写，CJK 字符直接包含；任一词命中即召回，
/// 与 knowledge 源的检索召回面同向）。
fn keyword_hit(keywords: &[String], haystack: &str) -> bool {
    let lower = haystack.to_lowercase();
    keywords.iter().any(|kw| {
        if kw.is_empty() {
            return false;
        }
        if kw.is_ascii() {
            lower.contains(&kw.to_lowercase())
        } else {
            haystack.contains(kw.as_str())
        }
    })
}

impl ContextSource for PluginSource {
    fn id(&self) -> &str {
        &self.source_id
    }

    fn candidates(
        &self,
        q: &IntentQuery,
        limit: usize,
    ) -> Result<Vec<Candidate>, ContextBrokerError> {
        // 插件闸先判（禁用即空集，合法结果，设计稿 §9.2 第 4 条）。
        let visible = self
            .plugin_visible()
            .map_err(|e| ContextBrokerError::Source {
                source: self.source_id.to_string(),
                reason: e.to_string(),
            })?;
        if !visible {
            return Ok(Vec::new());
        }
        // 关键词取 coverage_tokens（resource-engine ASCII 词 + CJK 2-gram，
        // search-engine 覆盖计分同源口径；CJK 意图同样可召回）。
        let keywords = search_engine::tokenizer::coverage_tokens(&q.text_normalized);
        if keywords.is_empty() {
            return Ok(Vec::new());
        }
        let entries = self
            .approved_entries()
            .map_err(|e| ContextBrokerError::Source {
                source: self.source_id.to_string(),
                reason: e.to_string(),
            })?;
        let mut out = Vec::new();
        for (kind, key, title, description, body, sha, version, url) in entries {
            if out.len() >= limit.max(1) {
                break;
            }
            let snippet: String = body.chars().take(SUMMARY_SNIPPET_CHARS).collect();
            let summary = description.clone().unwrap_or(snippet.clone());
            let haystack = format!("{title} {summary} {snippet}");
            if !keyword_hit(&keywords, &haystack) {
                continue;
            }
            let ref_id = format!("kse:{}:{kind}:{key}", self.plugin_id);
            out.push(Candidate {
                item_id: ref_id.clone(),
                kind: ContextItemKind::Plugin,
                layer: ContextLayer::L2,
                title,
                summary: summary.clone(),
                family: None,
                governance: Some("ACTIVE".to_string()),
                quality_grade: None,
                reference: ContentRef {
                    source: self.source_id.clone(),
                    ref_id,
                    ref_version: Some(version),
                    content_hash: Some(sha),
                    uri: url,
                },
                est_tokens: crate::est_tokens(&summary),
            });
        }
        Ok(out)
    }

    fn load(&self, r: &ContentRef) -> Result<Content, ContextBrokerError> {
        if r.source != self.source_id {
            return Err(ContextBrokerError::Source {
                source: self.source_id.clone(),
                reason: format!("ref source {:?} is not {}", r.source, self.source_id),
            });
        }
        // 双闸复核（load 与 candidates 同口径；ref 无法解析或未过闸即拒绝）。
        let visible = self
            .plugin_visible()
            .map_err(|e| ContextBrokerError::Source {
                source: self.source_id.clone(),
                reason: e.to_string(),
            })?;
        if !visible {
            return Err(ContextBrokerError::Source {
                source: self.source_id.clone(),
                reason: format!(
                    "plugin {:?} is not visible (disabled/uninstalled); access denied",
                    self.plugin_id
                ),
            });
        }
        // ref_id 词形：kse:<plugin_id>:<kind>:<key>。
        let prefix = format!("kse:{}:", self.plugin_id);
        let rest = r
            .ref_id
            .strip_prefix(&prefix)
            .ok_or_else(|| ContextBrokerError::Source {
                source: self.source_id.clone(),
                reason: format!(
                    "ref_id {:?} is not a kse ref of plugin {:?}",
                    r.ref_id, self.plugin_id
                ),
            })?;
        let (kind, key) = rest
            .split_once(':')
            .ok_or_else(|| ContextBrokerError::Source {
                source: self.source_id.clone(),
                reason: format!("ref_id {:?} lacks <entry_kind>:<item_key> tail", r.ref_id),
            })?;
        let row = self
            .pool
            .with_conn(|c| {
                c.query_row(
                    "SELECT title, body, content_sha256, plugin_version, canonical_url, review_status
                     FROM knowledge_source_entry
                     WHERE plugin_id = ?1 AND entry_kind = ?2 AND item_key = ?3",
                    rusqlite::params![self.plugin_id, kind, key],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, String>(2)?,
                            row.get::<_, String>(3)?,
                            row.get::<_, Option<String>>(4)?,
                            row.get::<_, String>(5)?,
                        ))
                    },
                )
                .optional()
            })
            .map_err(|e| ContextBrokerError::Source {
                source: self.source_id.clone(),
                reason: e.to_string(),
            })?;
        let Some((title, body, sha, version, url, review)) = row else {
            return Err(ContextBrokerError::Source {
                source: self.source_id.clone(),
                reason: format!("entry {:?} not found", r.ref_id),
            });
        };
        if review != "APPROVED" {
            return Err(ContextBrokerError::Source {
                source: self.source_id.clone(),
                reason: format!(
                    "entry {:?} review_status={review}; only APPROVED expands",
                    r.ref_id
                ),
            });
        }
        Ok(Content {
            reference: ContentRef {
                source: self.source_id.clone(),
                ref_id: r.ref_id.clone(),
                ref_version: Some(version),
                content_hash: Some(sha),
                uri: url,
            },
            est_tokens: crate::est_tokens(&format!("{title}\n{body}")),
            body,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seed(pool: &Pool) {
        let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../migrations");
        database::migrate(pool, &dir).unwrap();
    }

    fn insert_entry(
        pool: &Pool,
        plugin: &str,
        kind: &str,
        key: &str,
        title: &str,
        body: &str,
        review: &str,
    ) {
        pool.with_conn(|c| {
            c.execute(
                "INSERT INTO knowledge_source_entry (plugin_id, plugin_version, corpus_id,
                    entry_kind, item_key, title, body, content_sha256, metadata_sha256,
                    first_seen_at, last_seen_at, review_status)
                 VALUES (?1,'0.1.0','c',?2,?3,?4,?5,'deadbeef','meta','2026-11-12T00:00:00Z','2026-11-12T00:00:00Z',?6)",
                rusqlite::params![plugin, kind, key, title, body, review],
            )
        })
        .unwrap();
    }

    fn query(text: &str) -> IntentQuery {
        IntentQuery {
            text_normalized: text.to_string(),
            families: Vec::new(),
            knowledge_intents: Vec::new(),
            target_slots: Vec::new(),
            task_verbs: Vec::new(),
            constraints: Vec::new(),
            confidence: 1.0,
        }
    }

    #[test]
    fn candidates_only_when_visible_and_approved() {
        let pool = memory_pool();
        seed(&pool);
        insert_entry(
            &pool,
            "p.a",
            "catalog_entry",
            "https://x/1",
            "端口扫描工具",
            "body",
            "APPROVED",
        );
        insert_entry(
            &pool,
            "p.a",
            "catalog_entry",
            "https://x/2",
            "未审条目",
            "body",
            "DISCOVERED",
        );
        let src = PluginSource::new(pool.clone(), "p.a");
        // 插件未登记：空集（fail-closed）。
        assert!(src.candidates(&query("端口"), 10).unwrap().is_empty());
        pool.with_conn(|c| {
            c.execute(
                "INSERT INTO plugin (plugin_id, version, name, kind, description, license, trust_level, state, source_type, source_ref, manifest_json, created_at, updated_at, is_active) VALUES ('p.a','0.1.0','A','knowledge','d','MIT','local','ENABLED','local_dir','.','{}','2026-11-12T00:00:00Z','2026-11-12T00:00:00Z',1)",
                [],
            )
        })
        .unwrap();
        let hits = src.candidates(&query("端口扫描"), 10).unwrap();
        assert_eq!(hits.len(), 1, "only APPROVED row surfaces");
        assert_eq!(hits[0].kind, ContextItemKind::Plugin);
        assert_eq!(hits[0].reference.source, "plugin:p.a");
        assert_eq!(
            hits[0].reference.ref_id,
            "kse:p.a:catalog_entry:https://x/1"
        );
        // 禁用即撤出。
        pool.with_conn(|c| {
            c.execute(
                "UPDATE plugin SET state='DISABLED' WHERE plugin_id='p.a'",
                [],
            )
        })
        .unwrap();
        assert!(src.candidates(&query("端口扫描"), 10).unwrap().is_empty());
        // load 同闸拒绝。
        let r = ContentRef {
            source: "plugin:p.a".to_string(),
            ref_id: "kse:p.a:catalog_entry:https://x/1".to_string(),
            ref_version: None,
            content_hash: None,
            uri: None,
        };
        assert!(src.load(&r).is_err());
    }

    #[test]
    fn load_returns_full_body_when_approved_and_visible() {
        let pool = memory_pool();
        seed(&pool);
        insert_entry(
            &pool,
            "p.b",
            "document_markdown",
            "docs/g.md",
            "指南",
            "全文正文",
            "APPROVED",
        );
        pool.with_conn(|c| {
            c.execute(
                "INSERT INTO plugin (plugin_id, version, name, kind, description, license, trust_level, state, source_type, source_ref, manifest_json, created_at, updated_at, is_active) VALUES ('p.b','0.1.0','B','knowledge','d','MIT','local','ENABLED','local_dir','.','{}','2026-11-12T00:00:00Z','2026-11-12T00:00:00Z',1)",
                [],
            )
        })
        .unwrap();
        let src = PluginSource::new(pool, "p.b");
        let r = ContentRef {
            source: "plugin:p.b".to_string(),
            ref_id: "kse:p.b:document_markdown:docs/g.md".to_string(),
            ref_version: None,
            content_hash: None,
            uri: None,
        };
        let content = src.load(&r).unwrap();
        assert_eq!(content.body, "全文正文");
        assert_eq!(content.reference.content_hash.as_deref(), Some("deadbeef"));
    }

    #[test]
    fn mount_all_covers_every_plugin() {
        let pool = memory_pool();
        seed(&pool);
        pool.with_conn(|c| {
            c.execute_batch(
                "INSERT INTO plugin (plugin_id, version, name, kind, description, license, trust_level, state, source_type, source_ref, manifest_json, created_at, updated_at, is_active)
                 VALUES ('p.x','0.1.0','X','knowledge','d','MIT','local','ENABLED','local_dir','.','{}','2026-11-12T00:00:00Z','2026-11-12T00:00:00Z',1),
                        ('p.y','0.1.0','Y','tool','d','MIT','local','DISABLED','local_dir','.','{}','2026-11-12T00:00:00Z','2026-11-12T00:00:00Z',1);",
            )
        })
        .unwrap();
        let sources = PluginSource::mount_all(&pool);
        let ids: Vec<&str> = sources.iter().map(|s| s.id()).collect();
        assert_eq!(ids, vec!["plugin:p.x", "plugin:p.y"]);
    }
}
