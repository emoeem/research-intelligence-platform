//! 检索执行：FTS5 MATCH + 过滤 + 混合排序（bm25 50% / quality 30% / keyword 覆盖 20%）。
//!
//! 执行计划（catalog 规模，索引为可重建派生物）：
//! 1. 关键词扩展（ALIASES + CJK_BRIDGE）-> FTS5 MATCH（OR 语义，eri SQL 预筛同口径）
//! 2. JOIN search_index_doc 过滤（治理可见性 + filters 等值条件）
//! 3. bm25 归一（结果集内相对归一：|bm25_i| / max|bm25|，确定性、单调、有界）
//! 4. 混合打分（缺证轴剔除后归一，eri 口径）-> 排序 -> limit/offset 分页

use crate::error::SearchError;
use crate::index::{coverage_token_set, ensure_ready};
use crate::model::{Hits, IndexDoc, SearchFilters, SearchHit, SearchQuery, WeightsSnapshot};
use crate::tokenizer::{cjk_grams, expand_query_terms, has_han};
use crate::weights::{FTS_BM25_WEIGHT, KEYWORD_COVER_BOOST, KEYWORD_COVER_WEIGHT, QUALITY_WEIGHT};
use database::{rusqlite, Pool};
use std::time::Instant;

/// 检索引擎（面向索引库的查询入口）。
#[derive(Clone)]
pub struct SearchEngine {
    index_pool: Pool,
}

/// FTS5 查询词转义：整体双引号包裹，内部双引号翻倍（防 FTS 语法注入）。
fn quote_fts(term: &str) -> String {
    format!("\"{}\"", term.replace('"', "\"\""))
}

/// 关键词 -> FTS5 MATCH 串。CJK 词为 gram 短语（要求连续命中），ASCII 词为
/// 转义 OR 项（eri SQL LIKE 预筛的 OR 语义）。
pub fn build_match_query(keywords: &[String]) -> Result<String, SearchError> {
    let expanded = expand_query_terms(keywords);
    if expanded.is_empty() {
        return Err(SearchError::InvalidQuery(
            "keywords produced no usable terms".to_string(),
        ));
    }
    let mut clauses: Vec<String> = Vec::new();
    for term in &expanded {
        if has_han(term) {
            let grams = cjk_grams(term);
            match grams.len() {
                0 => continue,
                1 => clauses.push(quote_fts(&grams[0])),
                // 多 gram 以短语连接：gram 必须连续出现，避免单 gram 误召回
                _ => clauses.push(quote_fts(&grams.join(" "))),
            }
        } else {
            clauses.push(quote_fts(term));
        }
    }
    if clauses.is_empty() {
        return Err(SearchError::InvalidQuery(
            "keywords produced no CJK/ASCII terms".to_string(),
        ));
    }
    Ok(clauses.join(" OR "))
}

impl SearchEngine {
    /// 面向已构建索引库构造引擎（不校验就绪，查询时校验）。
    pub fn new(index_pool: Pool) -> Self {
        Self { index_pool }
    }

    /// 执行检索。
    pub fn search(&self, query: &SearchQuery) -> Result<Hits, SearchError> {
        let t0 = Instant::now();
        ensure_ready(&self.index_pool)?;
        if query.limit == 0 {
            return Err(SearchError::InvalidQuery(
                "limit must be > 0 (use offset for paging)".to_string(),
            ));
        }
        let expanded_kw = expand_query_terms(&query.keywords);
        let has_keywords = !expanded_kw.is_empty();

        // 1) FTS 匹配行（rowid + bm25 原值）；浏览模式无 FTS 轴（bm25_by_rowid 空）
        struct RawRow {
            rowid: i64,
            bm25_raw: f64,
        }
        let raw_rows: Vec<RawRow> = if has_keywords {
            let match_query = build_match_query(&query.keywords)?;
            self.index_pool
                .with_conn(|c| -> rusqlite::Result<Vec<RawRow>> {
                    let mut stmt = c.prepare(
                        "SELECT rowid, bm25(search_index_fts) FROM search_index_fts
                     WHERE search_index_fts MATCH ?1",
                    )?;
                    let rows = stmt
                        .query_map([&match_query], |r| {
                            Ok(RawRow {
                                rowid: r.get(0)?,
                                bm25_raw: r.get(1)?,
                            })
                        })?
                        .collect::<Result<Vec<_>, rusqlite::Error>>()?;
                    Ok(rows)
                })?
        } else {
            Vec::new()
        };
        let mut bm25_by_rowid: std::collections::HashMap<i64, f64> = raw_rows
            .into_iter()
            .map(|r| (r.rowid, r.bm25_raw))
            .collect();

        // 2) 文档过滤（治理可见性 + filters），与 FTS 匹配求交
        let docs = self.load_docs_filtered(&query.filters)?;
        let matched: Vec<(&IndexDoc, Option<f64>)> = docs
            .iter()
            .filter_map(|d| {
                let bm = bm25_by_rowid.remove(&d.rowid);
                if has_keywords && bm.is_none() {
                    None // FTS 未命中
                } else {
                    Some((&d.doc, bm))
                }
            })
            .collect();

        // 3) bm25 归一（结果集内相对归一：确定性、单调、有界 0..1）
        let max_abs = matched
            .iter()
            .filter_map(|(_, bm)| bm.map(|v| v.abs()))
            .fold(0.0f64, |acc, v| acc.max(v));

        // 4) 混合打分（缺证轴剔除后归一）+ 排序（eri tie-break：分数、质量、ID）
        let mut hits: Vec<SearchHit> = Vec::new();
        for (d, bm) in &matched {
            let mut reasons: Vec<String> = Vec::new();
            let mut weights_used: Vec<(f64, f64)> = Vec::new();

            // bm25 轴（有关键词即有证据）
            let mut fts_norm: Option<f64> = None;
            if let Some(raw) = bm {
                let v = if max_abs <= 0.0 {
                    1.0
                } else {
                    raw.abs() / max_abs
                };
                fts_norm = Some(round4(v));
                weights_used.push((FTS_BM25_WEIGHT, v));
                reasons.push(format!("fts bm25 (normalized): {v:.4}"));
            }

            // quality 轴
            if let Some(q) = d.quality_final_score {
                let v = (q / 100.0).clamp(0.0, 1.0);
                weights_used.push((QUALITY_WEIGHT, v));
                let grade = d
                    .quality_grade
                    .clone()
                    .unwrap_or_else(|| "None".to_string());
                reasons.push(format!("quality {q:?} ({grade})"));
            }

            // keyword 覆盖轴（eri 口径：扩展词在 blob 词元集中的覆盖，奖励 1.5 倍）
            let mut coverage_val = 0.0f64;
            let mut kw_hits: Vec<String> = Vec::new();
            if has_keywords {
                let tokens = coverage_token_set(d);
                kw_hits = expanded_kw
                    .iter()
                    .filter(|k| {
                        tokens.iter().any(|t| t == k.as_str())
                            || tokens.iter().any(|t| t.as_str() == format!("{k}s"))
                    })
                    .cloned()
                    .collect();
                if !kw_hits.is_empty() {
                    let cov = kw_hits.len() as f64 / expanded_kw.len().max(1) as f64;
                    coverage_val = (cov * KEYWORD_COVER_BOOST).min(1.0);
                    weights_used.push((KEYWORD_COVER_WEIGHT, coverage_val));
                    reasons.push(format!(
                        "keyword hits: [{}]",
                        kw_hits
                            .iter()
                            .take(5)
                            .cloned()
                            .collect::<Vec<_>>()
                            .join(", ")
                    ));
                }
            }

            // 缺证轴提示（可解释性，eri 文案口径）
            let mut missing: Vec<&str> = Vec::new();
            if fts_norm.is_none() {
                missing.push("fts_bm25");
            }
            if d.quality_final_score.is_none() {
                missing.push("quality");
            }
            if has_keywords && kw_hits.is_empty() {
                missing.push("keyword_coverage");
            }
            if !missing.is_empty() {
                reasons.push(format!(
                    "no evidence for: {} (权重已按证据归一)",
                    missing.join(",")
                ));
            }

            let wsum: f64 = weights_used.iter().map(|(w, _)| w).sum();
            let score = if wsum > 0.0 {
                round4(weights_used.iter().map(|(w, v)| w * v).sum::<f64>() / wsum)
            } else {
                0.0 // 全轴无证据（浏览模式且无质量证据）：0 分保留在结果中
            };
            hits.push(SearchHit {
                resource_id: d.resource_id.clone(),
                name: d.name.clone(),
                canonical_url: d.canonical_url.clone(),
                description: d.description.clone(),
                resource_type: d.resource_type.clone(),
                quality_final_score: d.quality_final_score,
                quality_grade: d.quality_grade.clone(),
                license_status: Some(d.license_status.clone()),
                maintenance_status: Some(d.maintenance_status.clone()),
                topics: d.topics.clone(),
                source_sections: d.source_sections.clone(),
                score,
                fts_score: fts_norm,
                keyword_coverage: round4(coverage_val),
                keyword_matches: kw_hits,
                reasons,
                source_annotations: source_annotations(d),
            });
        }
        hits.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| {
                    b.quality_final_score
                        .unwrap_or(0.0)
                        .partial_cmp(&a.quality_final_score.unwrap_or(0.0))
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .then_with(|| a.resource_id.cmp(&b.resource_id))
        });

        let total_matched = hits.len();
        let page: Vec<SearchHit> = hits
            .into_iter()
            .skip(query.offset)
            .take(query.limit)
            .collect();
        let search_status = if total_matched > 0 { "ok" } else { "no_match" };
        Ok(Hits {
            query_id: make_query_id(query, &expanded_kw),
            total_matched,
            hits: page,
            search_status: search_status.to_string(),
            weights: WeightsSnapshot {
                fts_bm25: FTS_BM25_WEIGHT,
                quality: QUALITY_WEIGHT,
                keyword_coverage: KEYWORD_COVER_WEIGHT,
            },
            elapsed_ms: round2(t0.elapsed().as_secs_f64() * 1000.0),
        })
    }

    /// 过滤后的文档行（治理可见性 + filters 等值条件，参数化查询）。
    fn load_docs_filtered(&self, filters: &SearchFilters) -> Result<Vec<IndexedRow>, SearchError> {
        let mut where_clause =
            "governance NOT IN ('QUARANTINED','REJECTED','ARCHIVED')".to_string();
        let mut params: Vec<String> = Vec::new();
        if let Some(grade) = &filters.grade {
            where_clause.push_str(" AND quality_grade = ?");
            params.push(grade.clone());
        }
        if let Some(lic) = &filters.license_status {
            where_clause.push_str(" AND license_status = ?");
            params.push(lic.clone());
        }
        if let Some(maint) = &filters.maintenance_status {
            where_clause.push_str(" AND maintenance_status = ?");
            params.push(maint.clone());
        }
        if let Some(rt) = &filters.resource_type {
            where_clause.push_str(" AND resource_type = ?");
            params.push(rt.clone());
        }
        let sql = format!(
            "SELECT rowid, resource_id, name, description, canonical_url, resource_type,
                    lifecycle, governance, quality_final_score, quality_grade,
                    license_status, maintenance_status, topics_json, language,
                    source_sections_json, source_repos_json
             FROM search_index_doc WHERE {where_clause} ORDER BY resource_id"
        );
        let rows = self
            .index_pool
            .with_conn(|c| -> rusqlite::Result<Vec<IndexedRow>> {
                let mut stmt = c.prepare(&sql)?;
                let rows = stmt
                    .query_map(rusqlite::params_from_iter(params.iter()), row_to_indexed)?
                    .collect::<Result<Vec<_>, rusqlite::Error>>()?;
                Ok(rows)
            })?;
        Ok(rows)
    }
}

/// 索引文档行（查询期形状：IndexDoc + rowid）。
struct IndexedRow {
    rowid: i64,
    doc: IndexDoc,
}

fn row_to_indexed(row: &rusqlite::Row<'_>) -> rusqlite::Result<IndexedRow> {
    let parse_json = |raw: String| -> rusqlite::Result<Vec<String>> {
        serde_json::from_str(&raw).map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
        })
    };
    Ok(IndexedRow {
        rowid: row.get(0)?,
        doc: IndexDoc {
            resource_id: row.get(1)?,
            name: row.get(2)?,
            description: row.get(3)?,
            canonical_url: row.get(4)?,
            resource_type: row.get(5)?,
            lifecycle: row.get(6)?,
            governance: row.get(7)?,
            quality_final_score: row.get(8)?,
            quality_grade: row.get(9)?,
            license_status: row.get(10)?,
            maintenance_status: row.get(11)?,
            topics: parse_json(row.get(12)?)?,
            language: row.get(13)?,
            source_sections: parse_json(row.get(14)?)?,
            source_repositories: parse_json(row.get(15)?)?,
        },
    })
}

/// 来源标注（文档字段出处声明）。
fn source_annotations(d: &IndexDoc) -> Vec<String> {
    let mut out = vec!["platform:resource".to_string()];
    if !d.source_sections.is_empty() {
        out.push(format!(
            "platform:resource_source(sections:{})",
            d.source_sections.len()
        ));
    }
    if !d.topics.is_empty() || d.license_status != "UNKNOWN" {
        out.push("catalog:github_metadata".to_string());
    }
    out
}

/// 查询 ID：srq-<紧凑时间戳20位>-<输入哈希前6位hex>（eri query_id 形状约定，
/// 前缀区分检索面）。
fn make_query_id(query: &SearchQuery, expanded: &[String]) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325; // FNV-1a offset basis
    let mut feed = |bytes: &[u8]| {
        for b in bytes {
            hash ^= u64::from(*b);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3); // FNV prime
        }
    };
    feed(query.keywords.join("\u{1}").as_bytes());
    feed(&[0x1f]);
    feed(format!("{:?}", query.filters).as_bytes());
    feed(&[0x1f]);
    feed(expanded.join("\u{1}").as_bytes());
    feed(&[0x1f]);
    feed(query.offset.to_string().as_bytes());
    let hex = format!("{hash:016x}");
    format!("srq-{}-{}", domain::time::now_compact_micros(), &hex[..6])
}

fn round4(v: f64) -> f64 {
    (v * 10_000.0).round() / 10_000.0
}

fn round2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::testutil::{doc_ids, fixture_pools};
    use crate::index::SearchIndex;

    fn built_engine() -> SearchEngine {
        let (p, cat, idx) = fixture_pools();
        SearchIndex::build(&p, &cat, &idx).unwrap();
        SearchEngine::new(idx)
    }

    fn hits_of(h: &Hits) -> Vec<String> {
        h.hits.iter().map(|x| x.resource_id.clone()).collect()
    }

    #[test]
    fn search_matches_ascii_and_excludes_quarantined() {
        let engine = built_engine();
        let q = SearchQuery::new(vec!["fuzzing".to_string()]);
        let r = engine.search(&q).unwrap();
        assert_eq!(r.search_status, "ok");
        let ids = hits_of(&r);
        assert!(ids.contains(&"gh:a/fuzz-tool".to_string()));
        assert!(
            !ids.contains(&"gh:c/quarantined".to_string()),
            "治理轴 QUARANTINED 不可见"
        );
        // 三轴证据齐全：fts/quality/keyword
        let hit = r
            .hits
            .iter()
            .find(|h| h.resource_id == "gh:a/fuzz-tool")
            .unwrap();
        assert!(hit.fts_score.is_some());
        assert!(hit.keyword_matches.contains(&"fuzzing".to_string()));
        assert!(hit.reasons.iter().any(|s| s.contains("quality 80")));
        assert!(hit.reasons.iter().any(|s| s.contains("fts bm25")));
        assert_eq!(
            hit.source_annotations,
            vec![
                "platform:resource".to_string(),
                "platform:resource_source(sections:1)".to_string(),
                "catalog:github_metadata".to_string()
            ]
        );
        assert!(r.query_id.starts_with("srq-"));
    }

    #[test]
    fn weights_snapshot_is_50_30_20() {
        let engine = built_engine();
        let r = engine
            .search(&SearchQuery::new(vec!["fuzzing".to_string()]))
            .unwrap();
        assert_eq!(r.weights.fts_bm25, 0.5);
        assert_eq!(r.weights.quality, 0.3);
        assert_eq!(r.weights.keyword_coverage, 0.2);
    }

    #[test]
    fn cjk_query_hits_2gram_docs() {
        let engine = built_engine();
        let q = SearchQuery::new(vec!["代理".to_string()]);
        let r = engine.search(&q).unwrap();
        assert!(
            hits_of(&r).contains(&"gh:b/cjk-tool".to_string()),
            "CJK gram 命中"
        );
        // 短语查询（连续 gram）："中文代理" 三字两 gram，需连续
        let q2 = SearchQuery::new(vec!["中文代理".to_string()]);
        let r2 = engine.search(&q2).unwrap();
        assert!(hits_of(&r2).contains(&"gh:b/cjk-tool".to_string()));
        // CJK_BRIDGE 桥接：扩展词 = [代理, proxy]，blob 词元集含 代理 gram
        // -> 覆盖 1/2 * 1.5 = 0.75（eri coverage 公式）
        let hit = r
            .hits
            .iter()
            .find(|h| h.resource_id == "gh:b/cjk-tool")
            .unwrap();
        assert_eq!(hit.keyword_matches, vec!["代理".to_string()]);
        assert_eq!(hit.keyword_coverage, 0.75);
    }

    #[test]
    fn filters_combine() {
        let engine = built_engine();
        let mut q = SearchQuery::new(vec!["fuzzing".to_string()]);
        q.filters.grade = Some("B".to_string());
        let r = engine.search(&q).unwrap();
        assert!(hits_of(&r).contains(&"gh:a/fuzz-tool".to_string()));
        assert!(!hits_of(&r).contains(&"gh:c/quarantined".to_string()));

        let mut q2 = SearchQuery::new(vec!["fuzzing".to_string()]);
        q2.filters.grade = Some("A".to_string());
        let r2 = engine.search(&q2).unwrap();
        assert_eq!(r2.search_status, "no_match", "A 级行被治理过滤");

        let mut q3 = SearchQuery::new(vec!["fuzzing".to_string()]);
        q3.filters.license_status = Some("IDENTIFIED".to_string());
        let r3 = engine.search(&q3).unwrap();
        assert_eq!(hits_of(&r3), vec!["gh:a/fuzz-tool".to_string()]);

        let mut q4 = SearchQuery::new(vec![]);
        q4.filters.maintenance_status = Some("ACTIVE".to_string());
        let r4 = engine.search(&q4).unwrap();
        // 浏览模式（无关键词）：quality 单轴。ACTIVE 仅 gh:a（gh:b 无元数据行
        // 归一 UNKNOWN，gh:c 被治理过滤）
        assert_eq!(r4.total_matched, 1);
        assert_eq!(hits_of(&r4), vec!["gh:a/fuzz-tool".to_string()]);
        let hit = &r4.hits[0];
        assert!(hit.fts_score.is_none(), "浏览模式无 bm25 轴");
        assert!(
            (hit.score - 0.8).abs() < 1e-9,
            "quality 单轴归一: score = 80/100"
        );

        // UNKNOWN 归一过滤：无元数据的 gh:b 可按 maintenance=UNKNOWN 检索
        let mut q5 = SearchQuery::new(vec![]);
        q5.filters.maintenance_status = Some("UNKNOWN".to_string());
        let r5 = engine.search(&q5).unwrap();
        assert_eq!(hits_of(&r5), vec!["gh:b/cjk-tool".to_string()]);
        assert!((r5.hits[0].score - 0.7).abs() < 1e-9);
    }

    #[test]
    fn pagination_offset_and_total() {
        let engine = built_engine();
        let mut q = SearchQuery::new(vec![]);
        q.limit = 1;
        let r = engine.search(&q).unwrap();
        assert_eq!(r.total_matched, 2);
        assert_eq!(r.hits.len(), 1);
        assert_eq!(hits_of(&r), vec!["gh:a/fuzz-tool".to_string()]);
        let mut q2 = SearchQuery::new(vec![]);
        q2.limit = 1;
        q2.offset = 1;
        let r2 = engine.search(&q2).unwrap();
        assert_eq!(r2.total_matched, 2);
        assert_eq!(hits_of(&r2), vec!["gh:b/cjk-tool".to_string()]);
    }

    #[test]
    fn resource_source_alias_name_recalled() {
        let engine = built_engine();
        // 章节名（alias/name）可独立召回资源
        let q = SearchQuery::new(vec!["awesome".to_string(), "repositories".to_string()]);
        let r = engine.search(&q).unwrap();
        assert!(hits_of(&r).contains(&"gh:a/fuzz-tool".to_string()));
        let hit = r
            .hits
            .iter()
            .find(|h| h.resource_id == "gh:a/fuzz-tool")
            .unwrap();
        assert_eq!(
            hit.source_sections,
            vec!["Awesome Repositories".to_string()]
        );
    }

    #[test]
    fn rebuild_keeps_search_results_stable() {
        let (p, cat, idx) = fixture_pools();
        SearchIndex::build(&p, &cat, &idx).unwrap();
        let engine = SearchEngine::new(idx.clone());
        let before = engine
            .search(&SearchQuery::new(vec!["fuzzing".to_string()]))
            .unwrap();
        let ids_before = doc_ids(&idx);
        SearchIndex::rebuild(&p, &cat, &idx).unwrap();
        let after = engine
            .search(&SearchQuery::new(vec!["fuzzing".to_string()]))
            .unwrap();
        assert_eq!(doc_ids(&idx), ids_before);
        // 分数与命中逐项一致（elapsed/query_id 不参与断言）
        let strip = |h: &Hits| -> Vec<(String, f64, Vec<String>)> {
            h.hits
                .iter()
                .map(|x| (x.resource_id.clone(), x.score, x.reasons.clone()))
                .collect()
        };
        assert_eq!(strip(&before), strip(&after));
        assert_eq!(before.total_matched, after.total_matched);
    }

    #[test]
    fn invalid_query_rejected() {
        let engine = built_engine();
        let mut q = SearchQuery::new(vec![]);
        q.limit = 0;
        let r = engine.search(&q);
        assert!(matches!(r, Err(SearchError::InvalidQuery(_))), "{r:?}");
    }
}
