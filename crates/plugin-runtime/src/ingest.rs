//! Knowledge 插件摄取管道（PLUGIN-CONTRACT §2 原则 2 第一红线 + §3.2；
//! ROADMAP P6-5 + P6-9/SK-1 通用化，裁决 docs/phase6/SK1-INGEST-ADR.md）。
//!
//! 红线语义落进代码：
//! - 权限闸：Manifest `permissions.data_access` 必须为
//!   [`INGEST_PERMISSION_REQUIRED`]（ingest_via_api），其余取值（read_only/
//!   none/缺省）一律拒绝，插件不得直写主库表，知识入主库只能经本管道；
//! - 溯源三元组：每行携带 plugin_id + plugin_version + corpus_id
//!   （RTIS 三类 → knowledge_ingest 表，010 迁移；SK-1 三类与 Nuclei 专用
//!   nuclei_template → knowledge_source_entry 表，011/032 迁移；卸载插件不删已入主库知识）；
//! - 幂等：knowledge_ingest 用 UNIQUE(plugin_id, plugin_version,
//!   content_kind, content_sha256) + INSERT OR IGNORE（M6 ETL Replay 口径）；
//!   knowledge_source_entry 用 UNIQUE(plugin_id, plugin_version, entry_kind,
//!   item_key) + INSERT OR IGNORE，键同内容变的行 skipped 并计入
//!   [`IngestReport::changed`]（不覆盖历史行，Evidence 可复核，ADR 第四节）；
//! - dry-run：[`IngestOptions::dry_run`] 以 SAVEPOINT 包裹写步后 ROLLBACK，
//!   报告计数与实跑完全同构，零落盘（P6-9 验收「三类新内容完成 dry-run」）。
//!
//! per-kind parser 注册表（P6-9 第 3/4 步）：
//! - RTIS 三类（poc/skill_md/method_case）：原逻辑逐行保留（knowledge.db
//!   mode=ro 只读、skills/*.md、pocs/ 递归），写 knowledge_ingest；
//! - SK-1 三类（catalog_entry/document_markdown/skill_candidate）：解析器为
//!   [`crate::knowledge_parse`] 纯文本实现（不执行 Markdown/HTML 任何代码、不访问链接，
//!   scheme 白名单 http/https）；Nuclei nuclei_template 使用 nuclei_yaml 专用
//!   解析器保留 YAML 原文并投影结构化字段。四类均写入 knowledge_source_entry，
//!   初始 review_status = DISCOVERED（检索/上下文面只收 APPROVED，
//!   翻转走 [`crate::knowledge_review`] 受控写面）。
//!
//! RTIS knowledge.db 条数口径（2026-11-11 实查收敛）：poc 表 14717 行，
//! rtis.py 头注「14664 条」为 2026-11-02 陈旧快照，README 的 14717 为当前
//! 实数（详见 docs/phase6/RTIS-REDTEAM-PLUGIN-NOTES.md）。

use std::collections::BTreeMap;
use std::path::Path;

const NUCLEI_TEMPLATE_SOURCE_ROOT: &str = "/usr/share/nuclei-templates";
const MAX_NUCLEI_TEMPLATE_BYTES: u64 = 2 * 1024 * 1024;
const MAX_CISA_KEV_BYTES: u64 = 16 * 1024 * 1024;

use serde::Serialize;
use serde_json::{json, Value};

use crate::knowledge_parse as kp;
use crate::RuntimeError;
use database::Pool;
use domain::knowledge::ReviewStatus;
use domain::time::now_iso;
use plugin_registry::hash::sha256_hex;

/// 摄取权限闸要求的唯一合法取值（PLUGIN-CONTRACT §2 原则 2）。
pub const INGEST_PERMISSION_REQUIRED: &str = "ingest_via_api";

/// 七类语料词表（v1.json capability content_kinds；010 三类 + SK-1 三类 + Nuclei 一类）。
pub const CONTENT_KINDS: [&str; 7] = [
    "poc",
    "skill_md",
    "method_case",
    "catalog_entry",
    "document_markdown",
    "skill_candidate",
    "nuclei_template",
];

/// RTIS 既有三类（knowledge_ingest 权威落点，010；本管道对这三类的解析与
/// 写面行为与 P6-5 逐字节一致，计数零回归）。
pub const RTIS_CONTENT_KINDS: [&str; 3] = ["poc", "skill_md", "method_case"];

/// 扩展四类（SK-1 三类 + Nuclei；knowledge_source_entry 权威落点，011/032）。
pub const EXTENDED_CONTENT_KINDS: [&str; 4] = [
    "catalog_entry",
    "document_markdown",
    "skill_candidate",
    "nuclei_template",
];

/// parser 注册表键与内容类型的配对（validate rule 3 同款，双处防御）。
pub const KIND_PARSER: [(&str, &str); 6] = [
    ("catalog_entry", "markdown_links"),
    ("catalog_entry", "cisa_kev_json"),
    ("catalog_entry", "pocindex_kev_json"),
    ("document_markdown", "markdown_doc"),
    ("skill_candidate", "skill_frontmatter"),
    ("nuclei_template", "nuclei_yaml"),
];

/// license/rights policy 词表（011 rights_policy CHECK 域）。
pub const LICENSE_POLICIES: [&str; 3] = ["catalog_only", "approved_only", "metadata_only"];

/// 语料条目中间形态（RTIS 三类；kind / title / source_pk / body / body_bytes / meta）。
pub type CorpusItem = (String, String, Option<String>, String, usize, Option<Value>);

/// SK-1 每类型摄取声明（provides.kinds.<kind>；validate rule 3 已保证存在）。
#[derive(Debug, Clone)]
pub struct KindConfig {
    /// 包内相对解析入口（单文件或目录）。
    pub source_path: String,
    /// 解析器键（KIND_PARSER 配对值）。
    pub parser: String,
    /// 许可与持久化策略（LICENSE_POLICIES 之一）。
    pub license_policy: String,
    /// 单类型条数硬上限（超限截断并在报告 filtered 显式计数）。
    pub max_entries: Option<u64>,
    /// 来源快照基准 Commit（缺省回落 manifest source.pinned.value）。
    pub source_commit: Option<String>,
}

/// 扩展知识类型共享的溯源列族值（Snapshot 为 ADR 3.1 列族形态）。
#[derive(Debug, Clone)]
pub struct SnapshotCtx {
    /// 来源许可（manifest license 原值）。
    pub license_status: String,
    /// 来源类型（manifest source.type 原值）。
    pub source_type: String,
    /// 来源主页/仓库 URL。
    pub source_url: String,
    /// 快照 Commit（kinds 声明优先，回落 source.pinned.value）。
    pub source_commit: Option<String>,
}

/// 扩展知识类型条目中间形态（011/032 knowledge_source_entry 行前身）。
#[derive(Debug, Clone)]
pub struct ExtendedItem {
    /// 内容类型（EXTENDED_CONTENT_KINDS 之一）。
    pub kind: String,
    /// 稳定条目键（catalog=规范化 URL；document/skill=包内相对路径）。
    pub item_key: String,
    /// 条目标题。
    pub title: String,
    /// 描述（可空）。
    pub description: Option<String>,
    /// 正文。
    pub body: String,
    /// 分类路径（章节标题路径，自顶向下）。
    pub category_path: Vec<String>,
    /// 规范化 URL（catalog_entry 必有）。
    pub canonical_url: Option<String>,
    /// 源位置 JSON（file/line/anchor）。
    pub source_location: Value,
    /// 其余源侧元数据 JSON。
    pub meta: Option<Value>,
    /// 许可状态（快照列族）。
    pub license_status: String,
    /// 许可与持久化策略（快照列族）。
    pub rights_policy: String,
    /// 来源类型（快照列族）。
    pub source_type: String,
    /// 来源主页/仓库 URL（快照列族）。
    pub source_url: String,
    /// 解析入口（provides.kinds 声明值，快照列族）。
    pub source_path: String,
    /// 快照 Commit。
    pub source_commit: Option<String>,
}

/// 摄取选项（P6-9 dry-run 载体）。
#[derive(Debug, Clone, Copy, Default)]
pub struct IngestOptions {
    /// dry-run：完整解析 + 去重判定，写步以 SAVEPOINT 包裹后回滚，零落盘。
    pub dry_run: bool,
}

/// 摄取报告（机器可读；CLI 与审计 detail 复用）。
#[derive(Debug, Clone, Serialize)]
pub struct IngestReport {
    /// 来源插件 id。
    pub plugin_id: String,
    /// 来源插件版本。
    pub plugin_version: String,
    /// 语料标识。
    pub corpus_id: String,
    /// 各类新插入条数（RTIS 三类与扩展四类合并口径）。
    pub counts: BTreeMap<String, u64>,
    /// 幂等跳过条数（内容不变行）。
    pub skipped: u64,
    /// 新插入正文总字节。
    pub total_bytes: u64,
    /// 各类跳过明细（幂等复核用）。
    pub skipped_by_kind: BTreeMap<String, u64>,
    /// 键同内容变的行数（knowledge_source_entry 专有；不覆盖历史行）。
    #[serde(default)]
    pub changed: u64,
    /// 解析期过滤计数（scheme 拒绝 / 无 scheme / max_entries 截断）。
    #[serde(default)]
    pub filtered: BTreeMap<String, u64>,
    /// 本次是否 dry-run（写步回滚，零落盘）。
    #[serde(default)]
    pub dry_run: bool,
}

/// 摄取一个 Knowledge 插件包（knowledge_source capability）到数据面。
pub fn ingest_knowledge_source(
    pool: &Pool,
    manifest: &Value,
    package_dir: &Path,
) -> Result<IngestReport, RuntimeError> {
    ingest_knowledge_source_with_opts(pool, manifest, package_dir, &IngestOptions::default())
}

/// 摄取（带选项；dry-run 走 SAVEPOINT 回滚）。
pub fn ingest_knowledge_source_with_opts(
    pool: &Pool,
    manifest: &Value,
    package_dir: &Path,
    opts: &IngestOptions,
) -> Result<IngestReport, RuntimeError> {
    ingest_knowledge_source_with_template_root(
        pool,
        manifest,
        package_dir,
        opts,
        Path::new(NUCLEI_TEMPLATE_SOURCE_ROOT),
    )
}

fn ingest_knowledge_source_with_template_root(
    pool: &Pool,
    manifest: &Value,
    package_dir: &Path,
    opts: &IngestOptions,
    nuclei_template_root: &Path,
) -> Result<IngestReport, RuntimeError> {
    let obj = manifest
        .as_object()
        .ok_or_else(|| RuntimeError::new("manifest_invalid", "manifest must be a JSON object"))?;
    let plugin_id = obj
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(|| RuntimeError::new("manifest_invalid", "manifest has no `id`"))?
        .to_string();
    let plugin_version = obj
        .get("version")
        .and_then(Value::as_str)
        .ok_or_else(|| RuntimeError::new("manifest_invalid", "manifest has no `version`"))?
        .to_string();

    // 权限闸（原则 2）：非 ingest_via_api 一律拒绝。
    let data_access = obj
        .get("permissions")
        .and_then(|p| p.get("data_access"))
        .and_then(Value::as_str)
        .unwrap_or_default();
    if data_access != INGEST_PERMISSION_REQUIRED {
        return Err(RuntimeError::new(
            "ingest_permission_denied",
            format!(
                "permissions.data_access {data_access:?} is not {INGEST_PERMISSION_REQUIRED:?}; plugins must not write main tables directly (PLUGIN-CONTRACT 2 principle 2)"
            ),
        ));
    }

    // corpus_id 与 content_kinds（knowledge_source capability）。
    let mut corpus_id = plugin_id.clone();
    let mut declared_kinds: Vec<String> = CONTENT_KINDS.iter().map(|s| s.to_string()).collect();
    if let Some(caps) = obj.get("capabilities").and_then(Value::as_array) {
        for cap in caps {
            if cap.get("type").and_then(Value::as_str) == Some("knowledge_source") {
                if let Some(provides) = cap.get("provides").and_then(Value::as_object) {
                    if let Some(cid) = provides.get("corpus_id").and_then(Value::as_str) {
                        corpus_id = cid.to_string();
                    }
                    if let Some(kinds) = provides.get("content_kinds").and_then(Value::as_array) {
                        declared_kinds = kinds
                            .iter()
                            .filter_map(Value::as_str)
                            .map(str::to_string)
                            .collect();
                    }
                }
            }
        }
    }
    for kind in &declared_kinds {
        if !CONTENT_KINDS.contains(&kind.as_str()) {
            return Err(RuntimeError::new(
                "ingest_kind_unknown",
                format!("content_kind {kind:?} not in {:?}", CONTENT_KINDS),
            ));
        }
    }

    // 来源快照列族字段（ADR 3.1：kinds 声明优先，回落 source.pinned.value）。
    let (source_type, source_url, pinned_commit) = source_snapshot_fields(obj);

    // RTIS 三类：原路径原行为（写 knowledge_ingest）。
    let rtis_declared: Vec<String> = declared_kinds
        .iter()
        .filter(|k| RTIS_CONTENT_KINDS.contains(&k.as_str()))
        .cloned()
        .collect();
    let rtis_items = collect_rtis_items(&rtis_declared, package_dir)?;

    // 扩展四类：per-kind parser 注册表（写 knowledge_source_entry）。
    let extended_declared: Vec<String> = declared_kinds
        .iter()
        .filter(|k| EXTENDED_CONTENT_KINDS.contains(&k.as_str()))
        .cloned()
        .collect();
    let mut extended_items: Vec<ExtendedItem> = Vec::new();
    let mut filtered: BTreeMap<String, u64> = BTreeMap::new();
    let mut nuclei_snapshot_complete = false;
    for kind in &extended_declared {
        let cfg = kind_config(obj, kind)?;
        let parser_registered = KIND_PARSER.iter().any(|(registered_kind, parser)| {
            *registered_kind == kind.as_str() && *parser == cfg.parser
        });
        if !parser_registered {
            let expected = KIND_PARSER
                .iter()
                .filter(|(registered_kind, _)| *registered_kind == kind.as_str())
                .map(|(_, parser)| *parser)
                .collect::<Vec<_>>();
            return Err(RuntimeError::new(
                "ingest_kind_parser_mismatch",
                format!(
                    "content_kind {kind:?} requires one of parsers {expected:?}, declared {:?}",
                    cfg.parser
                ),
            ));
        }
        if !LICENSE_POLICIES.contains(&cfg.license_policy.as_str()) {
            return Err(RuntimeError::new(
                "ingest_rights_policy_unknown",
                format!(
                    "rights_policy {:?} not in {LICENSE_POLICIES:?} for content_kind {kind:?}",
                    cfg.license_policy
                ),
            ));
        }
        let snap = SnapshotCtx {
            license_status: obj
                .get("license")
                .and_then(Value::as_str)
                .unwrap_or("UNKNOWN")
                .to_string(),
            source_type: source_type.clone(),
            source_url: source_url.clone(),
            source_commit: cfg.source_commit.clone().or_else(|| pinned_commit.clone()),
        };
        let (mut items, stats) = match kind.as_str() {
            "catalog_entry" => parse_catalog_entry(&cfg, package_dir, &snap)?,
            "document_markdown" => parse_document_markdown(&cfg, package_dir, &snap)?,
            "skill_candidate" => parse_skill_candidate(&cfg, package_dir, &snap)?,
            "nuclei_template" => parse_nuclei_template_items_from_root(
                &cfg,
                package_dir,
                nuclei_template_root,
                &snap,
            )?,
            other => {
                return Err(RuntimeError::new(
                    "ingest_kind_unknown",
                    format!("no parser registered for content_kind {other:?}"),
                ))
            }
        };
        // max_entries 截断（计划 §6.2A 超量条目截断报告）。
        let mut complete_snapshot = true;
        if let Some(cap) = cfg.max_entries {
            if items.len() as u64 > cap {
                let dropped = items.len() as u64 - cap;
                *filtered
                    .entry(format!("{kind}:max_entries_truncated"))
                    .or_insert(0) += dropped;
                items.truncate(cap as usize);
                complete_snapshot = false;
            }
        }
        if kind == "nuclei_template" {
            if items.is_empty() {
                filtered.insert(
                    "nuclei_template:empty_snapshot_not_reconciled".to_string(),
                    1,
                );
                complete_snapshot = false;
            }
            nuclei_snapshot_complete = complete_snapshot;
        }
        if stats.scheme_rejected > 0 {
            *filtered.entry("scheme_rejected".to_string()).or_insert(0) += stats.scheme_rejected;
        }
        if stats.no_scheme > 0 {
            *filtered.entry("no_scheme".to_string()).or_insert(0) += stats.no_scheme;
        }
        extended_items.extend(items);
    }

    // 写步：dry-run 用 SAVEPOINT 包裹后回滚（同一连接，计数与实跑同构）。
    let now = now_iso();
    let (counts, skipped_by_kind, skipped, total_bytes, changed) = if opts.dry_run {
        pool.with_conn(|c| -> Result<_, database::rusqlite::Error> {
            c.execute_batch("SAVEPOINT ingest_dry_run")?;
            let outcome = (|| {
                let result = write_all(
                    c,
                    &plugin_id,
                    &plugin_version,
                    &corpus_id,
                    &rtis_items,
                    &extended_items,
                    &now,
                )?;
                if nuclei_snapshot_complete {
                    reconcile_nuclei_source_snapshot(
                        c,
                        &plugin_id,
                        &plugin_version,
                        &extended_items,
                    )?;
                }
                Ok(result)
            })();
            // 无论写步成败都回滚（dry-run 零落盘）。
            let _ = c.execute_batch(
                "ROLLBACK TO SAVEPOINT ingest_dry_run; RELEASE SAVEPOINT ingest_dry_run;",
            );
            outcome
        })
        .map_err(RuntimeError::from)?
    } else {
        pool.with_tx(|tx| -> Result<_, database::rusqlite::Error> {
            let result = write_all(
                tx,
                &plugin_id,
                &plugin_version,
                &corpus_id,
                &rtis_items,
                &extended_items,
                &now,
            )?;
            if nuclei_snapshot_complete {
                reconcile_nuclei_source_snapshot(tx, &plugin_id, &plugin_version, &extended_items)?;
            }
            Ok(result)
        })
        .map_err(RuntimeError::from)?
    };

    Ok(IngestReport {
        plugin_id,
        plugin_version,
        corpus_id,
        counts,
        skipped,
        total_bytes,
        skipped_by_kind,
        changed,
        filtered,
        dry_run: opts.dry_run,
    })
}

fn reconcile_nuclei_source_snapshot(
    c: &database::rusqlite::Connection,
    plugin_id: &str,
    plugin_version: &str,
    items: &[ExtendedItem],
) -> Result<(), database::rusqlite::Error> {
    let plugin_table_exists: bool = c.query_row(
        "SELECT EXISTS (
            SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'plugin'
         )",
        [],
        |row| row.get(0),
    )?;
    if !plugin_table_exists {
        return Ok(());
    }
    let active_version: bool = c.query_row(
        "SELECT EXISTS (
            SELECT 1 FROM plugin
            WHERE plugin_id = ?1 AND version = ?2 AND is_active = 1
              AND source_uninstalled = 0
              AND state IN ('ENABLED','LOADED','ACTIVE')
         )",
        database::rusqlite::params![plugin_id, plugin_version],
        |row| row.get(0),
    )?;
    if !active_version {
        return Ok(());
    }

    let mut snapshots = BTreeMap::<(String, String), Vec<String>>::new();
    for item in items.iter().filter(|item| item.kind == "nuclei_template") {
        snapshots
            .entry((item.source_url.clone(), item.source_path.clone()))
            .or_default()
            .push(item.item_key.clone());
    }
    for ((source_url, source_path), mut item_keys) in snapshots {
        item_keys.sort();
        item_keys.dedup();
        if item_keys.is_empty() {
            continue;
        }
        let current_keys = serde_json::to_string(&item_keys)
            .map_err(|error| database::rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?;
        c.execute(
            "UPDATE knowledge_source_entry
             SET upstream_missing = CASE
                 WHEN item_key IN (SELECT value FROM json_each(?4)) THEN 0 ELSE 1 END
             WHERE plugin_id = ?1 AND entry_kind = 'nuclei_template'
               AND source_url = ?2 AND source_path = ?3",
            database::rusqlite::params![plugin_id, source_url, source_path, current_keys],
        )?;
    }
    Ok(())
}

/// 写步（RTIS 与 SK-1 两写面在同一连接/事务内；dry-run 由调用方 SAVEPOINT 回滚）。
#[allow(clippy::type_complexity)]
fn write_all(
    c: &database::rusqlite::Connection,
    plugin_id: &str,
    plugin_version: &str,
    corpus_id: &str,
    rtis_items: &[CorpusItem],
    extended_items: &[ExtendedItem],
    now: &str,
) -> Result<(BTreeMap<String, u64>, BTreeMap<String, u64>, u64, u64, u64), database::rusqlite::Error>
{
    let mut counts: BTreeMap<String, u64> = BTreeMap::new();
    let mut skipped_by_kind: BTreeMap<String, u64> = BTreeMap::new();
    let mut skipped = 0u64;
    let mut total_bytes = 0u64;
    let mut changed = 0u64;

    // 1) RTIS 三类 → knowledge_ingest（P6-5 原写面，逐字段一致）。
    for (kind, title, source_pk, body, body_bytes, meta) in rtis_items {
        let sha = sha256_hex(body.as_bytes());
        let mut stmt = c.prepare_cached(
            "INSERT OR IGNORE INTO knowledge_ingest (plugin_id, plugin_version, corpus_id,
                content_kind, title, source_path, source_pk, body, body_bytes,
                content_sha256, meta_json, ingested_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)",
        )?;
        let source_path = match source_pk {
            Some(pk) => format!("knowledge.db:/poc#{pk}"),
            None => format!(
                "{kind}:{}",
                sha256_hex(title.as_bytes()).get(..12).unwrap_or_default()
            ),
        };
        let n = stmt.execute(database::rusqlite::params![
            plugin_id,
            plugin_version,
            corpus_id,
            kind,
            title,
            source_path,
            source_pk,
            body,
            *body_bytes as i64,
            sha,
            meta.as_ref().map(|m| m.to_string()),
            now,
        ])?;
        if n > 0 {
            *counts.entry(kind.clone()).or_insert(0) += 1;
            total_bytes += *body_bytes as u64;
        } else {
            skipped += 1;
            *skipped_by_kind.entry(kind.clone()).or_insert(0) += 1;
        }
    }

    // 2) SK-1 与 Nuclei 扩展类型 → knowledge_source_entry（011/032）。
    // 同一批次内的重复 item_key 只计 skipped，不误报为跨批次内容变更。
    let mut seen_extended_keys = std::collections::BTreeSet::new();
    for item in extended_items {
        let is_in_pass_duplicate =
            !seen_extended_keys.insert((item.kind.clone(), item.item_key.clone()));
        let sha = sha256_hex(item.body.as_bytes());
        let cat_json = serde_json::to_string(&item.category_path)
            .map_err(|e| database::rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
        let loc_json = item.source_location.to_string();
        let meta_json = item.meta.as_ref().map(|m| m.to_string());
        let metadata_material = format!(
            "{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}",
            item.title,
            cat_json,
            loc_json,
            item.description.clone().unwrap_or_default(),
            meta_json.as_deref().unwrap_or_default(),
            item.source_type,
            item.source_url,
            item.source_path,
            item.source_commit.as_deref().unwrap_or_default(),
            item.license_status,
        );
        let metadata_sha = sha256_hex(metadata_material.as_bytes());
        let mut stmt = c.prepare_cached(
            "INSERT OR IGNORE INTO knowledge_source_entry (plugin_id, plugin_version, corpus_id,
                entry_kind, item_key, title, description, body, body_bytes,
                content_sha256, metadata_sha256, category_path_json, canonical_url,
                source_type, source_url, source_path, source_commit, license_status,
                rights_policy, review_status, risk_flags_json, source_location_json,
                meta_json, first_seen_at, last_seen_at, upstream_missing)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21,?22,?23,?24,?25,0)",
        )?;
        let n = stmt.execute(database::rusqlite::params![
            plugin_id,
            plugin_version,
            corpus_id,
            item.kind,
            item.item_key,
            item.title,
            item.description,
            item.body,
            item.body.len() as i64,
            sha,
            metadata_sha,
            cat_json,
            item.canonical_url,
            item.source_type,
            item.source_url,
            item.source_path,
            item.source_commit,
            item.license_status,
            item.rights_policy,
            ReviewStatus::Discovered.as_str(),
            "[]",
            loc_json,
            meta_json,
            now,
            now,
        ])?;
        if n > 0 {
            *counts.entry(item.kind.clone()).or_insert(0) += 1;
            total_bytes += item.body.len() as u64;
        } else {
            // 键已存在：内容变 = changed（不覆盖历史行，ADR 第四节）。
            let existing: Option<(String, String)> = c
                .query_row(
                    "SELECT content_sha256, metadata_sha256 FROM knowledge_source_entry
                     WHERE plugin_id = ?1 AND plugin_version = ?2 AND entry_kind = ?3 AND item_key = ?4",
                    database::rusqlite::params![plugin_id, plugin_version, item.kind, item.item_key],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .map(Some)
                .or_else(|e| match e {
                    database::rusqlite::Error::QueryReturnedNoRows => Ok(None),
                    other => Err(other),
                })?;
            let metadata_changed = match existing.as_ref() {
                Some((content_hash, metadata_hash)) => {
                    content_hash != &sha || metadata_hash != &metadata_sha
                }
                None => true,
            };
            if !is_in_pass_duplicate && metadata_changed {
                changed += 1;
            }
            skipped += 1;
            *skipped_by_kind.entry(item.kind.clone()).or_insert(0) += 1;
        }
    }
    Ok((counts, skipped_by_kind, skipped, total_bytes, changed))
}

/// manifest source 对象的快照字段（source_type/source_url/pinned commit）。
fn source_snapshot_fields(
    obj: &serde_json::Map<String, Value>,
) -> (String, String, Option<String>) {
    let source = obj.get("source").and_then(Value::as_object);
    let source_type = source
        .and_then(|s| s.get("type"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let url = source
        .and_then(|s| {
            s.get("url")
                .or_else(|| s.get("path"))
                .or_else(|| s.get("base_url"))
                .and_then(Value::as_str)
        })
        .unwrap_or_default()
        .to_string();
    let pinned = source
        .and_then(|s| s.get("pinned"))
        .and_then(|p| p.get("value"))
        .and_then(Value::as_str)
        .map(str::to_string);
    (source_type, url, pinned)
}

/// 取某 SK-1 类型的摄取声明（validate rule 3 已管存在性，此处幂等防御）。
fn kind_config(
    obj: &serde_json::Map<String, Value>,
    kind: &str,
) -> Result<KindConfig, RuntimeError> {
    let provides = obj
        .get("capabilities")
        .and_then(Value::as_array)
        .and_then(|caps| {
            caps.iter()
                .find(|c| c.get("type").and_then(Value::as_str) == Some("knowledge_source"))
                .and_then(|c| c.get("provides"))
                .and_then(Value::as_object)
        })
        .ok_or_else(|| {
            RuntimeError::new(
                "ingest_kind_config_missing",
                format!(
                    "content_kind {kind:?} requires a knowledge_source capability provides.kinds.{kind} declaration"
                ),
            )
        })?;
    let decl = provides
        .get("kinds")
        .and_then(Value::as_object)
        .and_then(|k| k.get(kind))
        .and_then(Value::as_object)
        .ok_or_else(|| {
            RuntimeError::new(
                "ingest_kind_config_missing",
                format!(
                    "content_kind {kind:?} requires provides.kinds.{kind} (source_path/parser/license_policy)"
                ),
            )
        })?;
    let get_str = |key: &str| decl.get(key).and_then(Value::as_str).map(str::to_string);
    Ok(KindConfig {
        source_path: get_str("source_path").ok_or_else(|| {
            RuntimeError::new(
                "ingest_kind_config_missing",
                format!("provides.kinds.{kind}.source_path missing"),
            )
        })?,
        parser: get_str("parser").unwrap_or_default(),
        license_policy: get_str("license_policy").unwrap_or_default(),
        max_entries: decl.get("max_entries").and_then(Value::as_u64),
        source_commit: get_str("source_commit"),
    })
}

// ---------------------------------------------------------------------------
// SK-1 parser 注册表实现（catalog_entry / document_markdown / skill_candidate）
// ---------------------------------------------------------------------------

/// 解析入口文件集合：source_path 为单文件或目录（目录即递归 *.md，确定性排序）。
fn resolve_md_files(
    base: &Path,
    source_path: &str,
) -> Result<Vec<std::path::PathBuf>, RuntimeError> {
    let root = base.join(source_path);
    let mut files = Vec::new();
    if root.is_file() {
        files.push(root);
    } else if root.is_dir() {
        collect_md_files(&root, &mut files)?;
        files.sort();
    }
    Ok(files)
}

fn collect_md_files(dir: &Path, out: &mut Vec<std::path::PathBuf>) -> Result<(), RuntimeError> {
    for entry in std::fs::read_dir(dir)
        .map_err(|e| RuntimeError::new("io", format!("read_dir {dir:?}: {e}")))?
    {
        let entry = entry.map_err(|e| RuntimeError::new("io", format!("entry: {e}")))?;
        let path = entry.path();
        if path.is_dir() {
            collect_md_files(&path, out)?;
        } else if path.extension().map(|e| e == "md").unwrap_or(false) {
            out.push(path);
        }
    }
    Ok(())
}

/// 包内相对路径（item_key 与 source_location.file 的统一口径）。
fn rel_to_package(base: &Path, path: &Path) -> String {
    path.strip_prefix(base)
        .unwrap_or(path)
        .to_string_lossy()
        .trim_start_matches('/')
        .to_string()
}

/// 给条目补齐快照列族（三类 parser 共用收尾）。
fn with_snapshot(mut item: ExtendedItem, cfg: &KindConfig, snap: &SnapshotCtx) -> ExtendedItem {
    item.license_status = snap.license_status.clone();
    item.rights_policy = cfg.license_policy.clone();
    item.source_type = snap.source_type.clone();
    item.source_url = snap.source_url.clone();
    item.source_path = cfg.source_path.clone();
    if item.source_commit.is_none() {
        item.source_commit = snap.source_commit.clone();
    }
    item
}

/// catalog_entry：链接目录条目（markdown_links）。
pub fn parse_catalog_entry(
    cfg: &KindConfig,
    package_dir: &Path,
    snap: &SnapshotCtx,
) -> Result<(Vec<ExtendedItem>, kp::FilterStats), RuntimeError> {
    match cfg.parser.as_str() {
        "markdown_links" => parse_catalog_markdown_entries(cfg, package_dir, snap),
        "cisa_kev_json" => parse_cisa_kev_json(cfg, package_dir, snap),
        "pocindex_kev_json" => parse_pocindex_kev_json(cfg, package_dir, snap),
        other => Err(RuntimeError::new(
            "ingest_kind_parser_mismatch",
            format!("unsupported catalog_entry parser {other:?}"),
        )),
    }
}

fn parse_catalog_markdown_entries(
    cfg: &KindConfig,
    package_dir: &Path,
    snap: &SnapshotCtx,
) -> Result<(Vec<ExtendedItem>, kp::FilterStats), RuntimeError> {
    let mut items = Vec::new();
    let mut total_stats = kp::FilterStats::default();
    for file in resolve_md_files(package_dir, &cfg.source_path)? {
        let text = std::fs::read_to_string(&file)
            .map_err(|e| RuntimeError::new("io", format!("read {file:?}: {e}")))?;
        let rel = rel_to_package(package_dir, &file);
        let headings = kp::extract_headings(&text);
        let (links, stats) = kp::extract_markdown_links(&text);
        total_stats.scheme_rejected += stats.scheme_rejected;
        total_stats.no_scheme += stats.no_scheme;
        for link in links {
            let Some(canonical) = kp::normalize_url(&link.url) else {
                continue;
            };
            let title = if link.text.trim().is_empty() {
                canonical.clone()
            } else {
                link.text.trim().to_string()
            };
            let category_path = kp::heading_path_at(&headings, link.line);
            let anchor = category_path.last().map(|h| kp::slugify(h));
            let body = if link.description.is_empty() {
                title.clone()
            } else {
                format!("{}: {}", title, link.description)
            };
            let source_location = json!({
                "file": rel,
                "line": link.line,
                "anchor": anchor,
            });
            let meta = json!({
                "raw_link_text": link.text,
                "in_table": link.in_table,
                "resource_kind": "link",
            });
            items.push(with_snapshot(
                ExtendedItem {
                    kind: "catalog_entry".to_string(),
                    item_key: canonical,
                    title,
                    description: if link.description.is_empty() {
                        None
                    } else {
                        Some(link.description)
                    },
                    body,
                    category_path,
                    canonical_url: Some(link.url),
                    source_location,
                    meta: Some(meta),
                    license_status: String::new(),
                    rights_policy: String::new(),
                    source_type: String::new(),
                    source_url: String::new(),
                    source_path: String::new(),
                    source_commit: None,
                },
                cfg,
                snap,
            ));
        }
    }
    Ok((items, total_stats))
}

/// CISA KEV JSON parser：只导入官方目录的漏洞元数据，不导入或执行 PoC。
/// 每条记录以 CVE ID 作为稳定键；CVSS/EPSS/PoC 状态明确留空或未评估，
/// 避免把 KEV 收录误读为 CVSS 分数、PoC 可用性或目标当前仍易受攻击。
fn parse_cisa_kev_json(
    cfg: &KindConfig,
    package_dir: &Path,
    snap: &SnapshotCtx,
) -> Result<(Vec<ExtendedItem>, kp::FilterStats), RuntimeError> {
    let root = package_dir.canonicalize().map_err(|e| {
        RuntimeError::new(
            "ingest_source_open_failed",
            format!("canonicalize package root: {e}"),
        )
    })?;
    let candidate = package_dir.join(&cfg.source_path);
    let path = candidate.canonicalize().map_err(|e| {
        RuntimeError::new(
            "ingest_source_open_failed",
            format!("resolve CISA KEV source {candidate:?}: {e}"),
        )
    })?;
    if !path.starts_with(&root) {
        return Err(RuntimeError::new(
            "ingest_source_path_escape",
            "CISA KEV source path resolves outside the pinned package",
        ));
    }
    let size = std::fs::metadata(&path)
        .map_err(|e| {
            RuntimeError::new(
                "ingest_source_read_failed",
                format!("stat CISA KEV source: {e}"),
            )
        })?
        .len();
    if size == 0 || size > MAX_CISA_KEV_BYTES {
        return Err(RuntimeError::new(
            "ingest_source_size_invalid",
            format!("CISA KEV JSON size {size} is outside 1..={MAX_CISA_KEV_BYTES} bytes"),
        ));
    }
    let bytes = std::fs::read(&path).map_err(|e| {
        RuntimeError::new(
            "ingest_source_read_failed",
            format!("read CISA KEV JSON: {e}"),
        )
    })?;
    let feed: Value = serde_json::from_slice(&bytes).map_err(|e| {
        RuntimeError::new(
            "ingest_source_parse_failed",
            format!("parse CISA KEV JSON: {e}"),
        )
    })?;
    let upstream_commit = cfg
        .source_commit
        .as_deref()
        .filter(|value| value.len() == 40 && value.bytes().all(|b| b.is_ascii_hexdigit()))
        .ok_or_else(|| {
            RuntimeError::new(
                "ingest_source_provenance_invalid",
                "CISA KEV parser requires a full 40-character upstream Git commit pin",
            )
        })?;
    let catalog_version = feed
        .get("catalogVersion")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| {
            RuntimeError::new(
                "ingest_source_schema_invalid",
                "CISA KEV catalogVersion missing",
            )
        })?;
    let date_released = feed
        .get("dateReleased")
        .and_then(Value::as_str)
        .filter(|s| s.len() >= 20 && s.as_bytes().get(10) == Some(&b'T'))
        .ok_or_else(|| {
            RuntimeError::new(
                "ingest_source_schema_invalid",
                "CISA KEV dateReleased missing or not an ISO date-time",
            )
        })?;
    let vulnerabilities = feed
        .get("vulnerabilities")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            RuntimeError::new(
                "ingest_source_schema_invalid",
                "CISA KEV vulnerabilities array missing",
            )
        })?;
    let declared_count = feed.get("count").and_then(Value::as_u64).ok_or_else(|| {
        RuntimeError::new(
            "ingest_source_schema_invalid",
            "CISA KEV count missing or invalid",
        )
    })?;
    if declared_count != vulnerabilities.len() as u64 || vulnerabilities.is_empty() {
        return Err(RuntimeError::new(
            "ingest_source_count_mismatch",
            format!(
                "CISA KEV count={declared_count}, actual records={}",
                vulnerabilities.len()
            ),
        ));
    }

    let mut seen = std::collections::HashSet::with_capacity(vulnerabilities.len());
    let mut items = Vec::with_capacity(vulnerabilities.len());
    for (index, record) in vulnerabilities.iter().enumerate() {
        let field = |name: &str| {
            record
                .get(name)
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
        };
        let cve_id = field("cveID").ok_or_else(|| {
            RuntimeError::new(
                "ingest_source_record_invalid",
                format!("CISA KEV record {index}: cveID missing"),
            )
        })?;
        if !is_cve_identifier(cve_id) || !seen.insert(cve_id.to_string()) {
            return Err(RuntimeError::new(
                "ingest_source_record_invalid",
                format!("CISA KEV record {index}: invalid or duplicate CVE ID {cve_id:?}"),
            ));
        }
        let vendor = field("vendorProject").ok_or_else(|| {
            RuntimeError::new(
                "ingest_source_record_invalid",
                format!("{cve_id}: vendorProject missing"),
            )
        })?;
        let product = field("product").ok_or_else(|| {
            RuntimeError::new(
                "ingest_source_record_invalid",
                format!("{cve_id}: product missing"),
            )
        })?;
        let vulnerability_name = field("vulnerabilityName").ok_or_else(|| {
            RuntimeError::new(
                "ingest_source_record_invalid",
                format!("{cve_id}: vulnerabilityName missing"),
            )
        })?;
        let short_description = field("shortDescription").ok_or_else(|| {
            RuntimeError::new(
                "ingest_source_record_invalid",
                format!("{cve_id}: shortDescription missing"),
            )
        })?;
        let date_added = field("dateAdded").ok_or_else(|| {
            RuntimeError::new(
                "ingest_source_record_invalid",
                format!("{cve_id}: dateAdded missing"),
            )
        })?;
        if !is_iso_date(date_added) {
            return Err(RuntimeError::new(
                "ingest_source_record_invalid",
                format!("{cve_id}: invalid dateAdded {date_added:?}"),
            ));
        }
        let due_date = field("dueDate").ok_or_else(|| {
            RuntimeError::new(
                "ingest_source_record_invalid",
                format!("{cve_id}: dueDate missing"),
            )
        })?;
        if !is_iso_date(due_date) {
            return Err(RuntimeError::new(
                "ingest_source_record_invalid",
                format!("{cve_id}: invalid dueDate {due_date:?}"),
            ));
        }
        let required_action = field("requiredAction").ok_or_else(|| {
            RuntimeError::new(
                "ingest_source_record_invalid",
                format!("{cve_id}: requiredAction missing"),
            )
        })?;
        for optional_string in ["knownRansomwareCampaignUse", "forensicTriage", "notes"] {
            if record
                .get(optional_string)
                .is_some_and(|value| !value.is_string())
            {
                return Err(RuntimeError::new(
                    "ingest_source_record_invalid",
                    format!("{cve_id}: {optional_string} must be a string when present"),
                ));
            }
        }
        let ransomware_use = field("knownRansomwareCampaignUse").unwrap_or("Unknown");
        if !matches!(ransomware_use, "Known" | "Unknown") {
            return Err(RuntimeError::new(
                "ingest_source_record_invalid",
                format!("{cve_id}: unexpected ransomware-use value {ransomware_use:?}"),
            ));
        }
        let forensic_triage = field("forensicTriage").unwrap_or("Unknown");
        if !matches!(forensic_triage, "Yes" | "No" | "Unknown") {
            return Err(RuntimeError::new(
                "ingest_source_record_invalid",
                format!("{cve_id}: unexpected forensicTriage value {forensic_triage:?}"),
            ));
        }
        let notes = field("notes");
        let cwes = match record.get("cwes") {
            None => Vec::new(),
            Some(Value::Array(values)) => values
                .iter()
                .map(|value| {
                    value.as_str().map(str::to_string).ok_or_else(|| {
                        RuntimeError::new(
                            "ingest_source_record_invalid",
                            format!("{cve_id}: each CWE value must be a string"),
                        )
                    })
                })
                .collect::<Result<Vec<_>, _>>()?,
            Some(_) => {
                return Err(RuntimeError::new(
                    "ingest_source_record_invalid",
                    format!("{cve_id}: cwes must be an array when present"),
                ));
            }
        };
        if cwes.iter().any(|cwe| !is_cwe_identifier(cwe)) {
            return Err(RuntimeError::new(
                "ingest_source_record_invalid",
                format!("{cve_id}: invalid CWE value in {cwes:?}"),
            ));
        }
        let title = format!("{cve_id} — {vulnerability_name}");
        let description = if short_description.is_empty() {
            None
        } else {
            Some(short_description.to_string())
        };
        let body = format!(
            "CISA Known Exploited Vulnerabilities (KEV)\nCVE: {cve_id}\nVendor: {vendor}\nProduct: {product}\nVulnerability: {vulnerability_name}\nDate added: {date_added}\nDue date: {}\nKnown ransomware campaign use: {ransomware_use}\nForensic triage (BOD 26-04): {forensic_triage}\nCWE: {}\nDescription: {short_description}\nRequired action: {required_action}\nNotes: {}\nCatalog version: {catalog_version}\nCatalog released: {date_released}\nCVSS: not supplied by the KEV feed\nEPSS: not supplied by the KEV feed\nPoC status: not assessed\nCurrent target status: not tested",
            due_date,
            if cwes.is_empty() { "not supplied".to_string() } else { cwes.join(", ") },
            notes.unwrap_or("none"),
        );
        let meta = json!({
            "cve_record": {
                "schema": "cisa-kev-v1",
                "cve_id": cve_id,
                "source_authority": "CISA",
                "record_type": "known_exploited",
                "known_exploited": true,
                "vendor_project": vendor,
                "product": product,
                "vulnerability_name": vulnerability_name,
                "short_description": short_description,
                "date_added": date_added,
                "due_date": due_date,
                "known_ransomware_campaign_use": ransomware_use,
                "forensic_triage": forensic_triage,
                "required_action": required_action,
                "notes": notes,
                "cwes": cwes,
                "catalog_version": catalog_version,
                "catalog_date_released": date_released,
                "cvss_score": null,
                "cvss_severity": "not_provided_by_kev",
                "epss_score": null,
                "epss_percentile": null,
                "poc_status": "not_assessed",
                "current_target_status": "not_tested"
            }
        });
        items.push(ExtendedItem {
            kind: "catalog_entry".to_string(),
            item_key: cve_id.to_string(),
            title,
            description,
            body,
            category_path: vec!["CISA KEV".to_string(), vendor.to_string(), product.to_string()],
            canonical_url: Some(format!("https://www.cisa.gov/known-exploited-vulnerabilities-catalog?field_cve={cve_id}")),
            source_location: json!({"file": cfg.source_path, "record_index": index, "catalog_version": catalog_version}),
            meta: Some(meta),
            license_status: snap.license_status.clone(),
            rights_policy: cfg.license_policy.clone(),
            source_type: "git".to_string(),
            source_url: "https://github.com/cisagov/kev-data".to_string(),
            source_path: cfg.source_path.clone(),
            source_commit: Some(upstream_commit.to_string()),
        });
    }
    Ok((items, kp::FilterStats::default()))
}

/// PoC Index CVE enrichment parser：只导入 CVSS/EPSS/公告链接与 Nuclei 元数据，不导入 PoC 正文。
/// 所有评分保留原始来源字段；PoC Index KEV membership 仅表示该 CVE 出现在聚合索引中。
fn parse_pocindex_kev_json(
    cfg: &KindConfig,
    package_dir: &Path,
    snap: &SnapshotCtx,
) -> Result<(Vec<ExtendedItem>, kp::FilterStats), RuntimeError> {
    let root = package_dir.canonicalize().map_err(|e| {
        RuntimeError::new(
            "ingest_source_open_failed",
            format!("canonicalize package root: {e}"),
        )
    })?;
    let candidate = package_dir.join(&cfg.source_path);
    let path = candidate.canonicalize().map_err(|e| {
        RuntimeError::new(
            "ingest_source_open_failed",
            format!("canonicalize PoC Index JSON: {e}"),
        )
    })?;
    if !path.starts_with(&root) || !path.is_file() {
        return Err(RuntimeError::new(
            "ingest_source_path_invalid",
            "PoC Index JSON path escapes the package or is not a file",
        ));
    }
    const MAX_BYTES: u64 = 16 * 1024 * 1024;
    let size = std::fs::metadata(&path)
        .map_err(|e| RuntimeError::new("io", format!("stat {path:?}: {e}")))?
        .len();
    if size == 0 || size > MAX_BYTES {
        return Err(RuntimeError::new(
            "ingest_source_size_invalid",
            format!("PoC Index JSON size {size} is outside 1..={MAX_BYTES}"),
        ));
    }
    let bytes =
        std::fs::read(&path).map_err(|e| RuntimeError::new("io", format!("read {path:?}: {e}")))?;
    let payload: Value = serde_json::from_slice(&bytes).map_err(|e| {
        RuntimeError::new(
            "ingest_source_json_invalid",
            format!("parse PoC Index JSON: {e}"),
        )
    })?;
    if payload.get("schema").and_then(Value::as_str) != Some("pocindex-kev-enrichment-v1") {
        return Err(RuntimeError::new(
            "ingest_source_schema_invalid",
            "unsupported PoC Index enrichment schema",
        ));
    }
    let repo_commit = cfg
        .source_commit
        .as_deref()
        .filter(|value| value.len() == 40 && value.bytes().all(|b| b.is_ascii_hexdigit()))
        .ok_or_else(|| {
            RuntimeError::new(
                "ingest_source_provenance_invalid",
                "PoC Index parser requires a full 40-character repository commit pin",
            )
        })?;
    let snapshot = payload.get("snapshot").ok_or_else(|| {
        RuntimeError::new(
            "ingest_source_snapshot_invalid",
            "PoC Index snapshot metadata missing",
        )
    })?;
    if snapshot.get("repository").and_then(Value::as_str)
        != Some("https://github.com/0xMarcio/pocindex")
        || snapshot.get("repository_commit").and_then(Value::as_str) != Some(repo_commit)
        || snapshot.get("cdn").and_then(Value::as_str) != Some("https://pocindex.io")
    {
        return Err(RuntimeError::new(
            "ingest_source_snapshot_invalid",
            "PoC Index snapshot repository/commit/CDN does not match the pinned source",
        ));
    }
    let retrieved_at = snapshot
        .get("retrieved_at")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if !retrieved_at.contains('T') {
        return Err(RuntimeError::new(
            "ingest_source_snapshot_invalid",
            "PoC Index retrieved_at must be an ISO-8601 timestamp",
        ));
    }
    let files = snapshot
        .get("files")
        .and_then(Value::as_object)
        .ok_or_else(|| {
            RuntimeError::new(
                "ingest_source_snapshot_invalid",
                "PoC Index file provenance missing",
            )
        })?;
    for name in ["kev.json", "epss.json", "cve_metadata.json", "nuclei.json"] {
        let file = files.get(name).and_then(Value::as_object).ok_or_else(|| {
            RuntimeError::new(
                "ingest_source_snapshot_invalid",
                format!("PoC Index file provenance missing {name}"),
            )
        })?;
        let sha = file
            .get("sha256")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if sha.len() != 64 || !sha.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(RuntimeError::new(
                "ingest_source_snapshot_invalid",
                format!("PoC Index {name} SHA-256 is invalid"),
            ));
        }
        if file.get("bytes").and_then(Value::as_u64).unwrap_or(0) == 0
            || file
                .get("url")
                .and_then(Value::as_str)
                .is_none_or(|url| !url.starts_with("https://pocindex.io/"))
        {
            return Err(RuntimeError::new(
                "ingest_source_snapshot_invalid",
                format!("PoC Index {name} file provenance is incomplete"),
            ));
        }
    }
    let records = payload
        .get("records")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            RuntimeError::new(
                "ingest_source_schema_invalid",
                "PoC Index records array missing",
            )
        })?;
    let count = payload.get("count").and_then(Value::as_u64).unwrap_or(0) as usize;
    if records.is_empty() || records.len() != count || records.len() > 2_000 {
        return Err(RuntimeError::new(
            "ingest_source_count_mismatch",
            format!(
                "PoC Index count {count} does not match {} records",
                records.len()
            ),
        ));
    }
    let mut seen = std::collections::HashSet::new();
    let mut items = Vec::with_capacity(records.len());
    for record in records {
        let cve_id = record
            .get("cve_id")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if !is_cve_identifier(cve_id) || !seen.insert(cve_id.to_string()) {
            return Err(RuntimeError::new(
                "ingest_source_record_invalid",
                format!("invalid or duplicate PoC Index CVE ID: {cve_id:?}"),
            ));
        }
        let cisa_date_added = record
            .get("cisa_date_added")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if !is_iso_date(cisa_date_added) {
            return Err(RuntimeError::new(
                "ingest_source_record_invalid",
                format!("{cve_id}: invalid CISA date_added"),
            ));
        }
        let kev_present = record
            .get("pocindex_kev_present")
            .and_then(Value::as_bool)
            .ok_or_else(|| {
                RuntimeError::new(
                    "ingest_source_record_invalid",
                    format!("{cve_id}: pocindex_kev_present must be boolean"),
                )
            })?;
        let kev_value = record
            .get("pocindex_kev_value")
            .filter(|value| !value.is_null());
        if kev_present != kev_value.is_some() || kev_value.is_some_and(|value| !value.is_array()) {
            return Err(RuntimeError::new(
                "ingest_source_record_invalid",
                format!("{cve_id}: PoC Index KEV membership/value mismatch"),
            ));
        }
        let optional_score = |key: &str| -> Result<Option<f64>, RuntimeError> {
            match record.get(key) {
                None | Some(Value::Null) => Ok(None),
                Some(value) => {
                    let score = value.as_f64().ok_or_else(|| {
                        RuntimeError::new(
                            "ingest_source_record_invalid",
                            format!("{cve_id}: {key} must be numeric"),
                        )
                    })?;
                    if !(0.0..=1.0).contains(&score) {
                        return Err(RuntimeError::new(
                            "ingest_source_record_invalid",
                            format!("{cve_id}: {key} outside 0..=1"),
                        ));
                    }
                    Ok(Some(score))
                }
            }
        };
        let epss_score = optional_score("epss_score")?;
        let epss_percentile = optional_score("epss_percentile")?;
        if epss_score.is_some() != epss_percentile.is_some() {
            return Err(RuntimeError::new(
                "ingest_source_record_invalid",
                format!("{cve_id}: EPSS score and percentile must both be present or absent"),
            ));
        }
        let cvss_values = record
            .get("cvss_assessments")
            .and_then(Value::as_array)
            .ok_or_else(|| {
                RuntimeError::new(
                    "ingest_source_record_invalid",
                    format!("{cve_id}: cvss_assessments must be an array"),
                )
            })?;
        let mut cvss_assessments = Vec::with_capacity(cvss_values.len());
        for assessment in cvss_values {
            let version = assessment
                .get("version")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let score = assessment
                .get("score")
                .and_then(Value::as_f64)
                .ok_or_else(|| {
                    RuntimeError::new(
                        "ingest_source_record_invalid",
                        format!("{cve_id}: CVSS score missing"),
                    )
                })?;
            let severity = assessment
                .get("severity")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let vector = assessment
                .get("vector")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let source = assessment
                .get("source")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let assessment_type = assessment
                .get("assessment_type")
                .and_then(Value::as_str)
                .unwrap_or_default();
            if !matches!(version, "2.0" | "3.0" | "3.1" | "4.0")
                || !(0.0..=10.0).contains(&score)
                || !matches!(severity, "NONE" | "LOW" | "MEDIUM" | "HIGH" | "CRITICAL")
                || vector.is_empty()
                || source.is_empty()
                || assessment_type.is_empty()
            {
                return Err(RuntimeError::new(
                    "ingest_source_record_invalid",
                    format!("{cve_id}: malformed CVSS assessment"),
                ));
            }
            cvss_assessments.push(assessment.clone());
        }
        let advisory_values = record
            .get("advisories")
            .and_then(Value::as_array)
            .ok_or_else(|| {
                RuntimeError::new(
                    "ingest_source_record_invalid",
                    format!("{cve_id}: advisories must be an array"),
                )
            })?;
        let mut advisories = Vec::with_capacity(advisory_values.len());
        for advisory in advisory_values {
            let url = advisory
                .get("url")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let tags = advisory
                .get("tags")
                .and_then(Value::as_array)
                .ok_or_else(|| {
                    RuntimeError::new(
                        "ingest_source_record_invalid",
                        format!("{cve_id}: advisory tags must be an array"),
                    )
                })?;
            if !(url.starts_with("https://") || url.starts_with("http://"))
                || tags.iter().any(|tag| !tag.is_string())
            {
                return Err(RuntimeError::new(
                    "ingest_source_record_invalid",
                    format!("{cve_id}: malformed advisory URL/tags"),
                ));
            }
            advisories.push(advisory.clone());
        }
        let nuclei_metadata = record
            .get("nuclei_metadata")
            .filter(|value| !value.is_null());
        if let Some(nuclei) = nuclei_metadata {
            let severity = nuclei
                .get("severity")
                .and_then(Value::as_str)
                .unwrap_or_default();
            if !matches!(
                severity,
                "info" | "low" | "medium" | "high" | "critical" | "unknown"
            ) {
                return Err(RuntimeError::new(
                    "ingest_source_record_invalid",
                    format!("{cve_id}: invalid Nuclei severity"),
                ));
            }
            if let Some(score) = nuclei.get("cvss").filter(|value| !value.is_null()) {
                let score = score.as_f64().ok_or_else(|| {
                    RuntimeError::new(
                        "ingest_source_record_invalid",
                        format!("{cve_id}: Nuclei CVSS must be numeric"),
                    )
                })?;
                if !(0.0..=10.0).contains(&score) {
                    return Err(RuntimeError::new(
                        "ingest_source_record_invalid",
                        format!("{cve_id}: Nuclei CVSS outside 0..=10"),
                    ));
                }
            }
        }
        let title = format!("{cve_id} — PoC Index risk enrichment");
        let mut body = format!(
            "PoC Index CVE enrichment\nCVE: {cve_id}\nCISA KEV date added: {cisa_date_added}\nPoC Index KEV map membership: {kev_present}\nEPSS score: {}\nEPSS percentile: {}\nCVSS assessments: {}\nAdvisories: {}\n",
            epss_score.map_or_else(|| "not available".to_string(), |v| v.to_string()),
            epss_percentile.map_or_else(|| "not available".to_string(), |v| v.to_string()),
            cvss_assessments.len(),
            advisories.len(),
        );
        for assessment in &cvss_assessments {
            body.push_str(&format!(
                "CVSS {}: {} {} [{}; {}]\n",
                assessment["version"].as_str().unwrap_or_default(),
                assessment["score"].as_f64().unwrap_or_default(),
                assessment["severity"].as_str().unwrap_or_default(),
                assessment["source"].as_str().unwrap_or_default(),
                assessment["assessment_type"].as_str().unwrap_or_default(),
            ));
        }
        if let Some(nuclei) = nuclei_metadata {
            body.push_str(&format!(
                "PoC Index Nuclei metadata: severity={}, CVSS={}, CWE={}\n",
                nuclei
                    .get("severity")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown"),
                nuclei
                    .get("cvss")
                    .map_or("not available".to_string(), |v| v.to_string()),
                nuclei
                    .get("cwe")
                    .and_then(Value::as_str)
                    .unwrap_or("not available"),
            ));
        }
        let meta = json!({
            "pocindex_record": {
                "cve_id": cve_id,
                "cisa_date_added": cisa_date_added,
                "pocindex_kev_present": kev_present,
                "pocindex_kev_value": kev_value,
                "epss_score": epss_score,
                "epss_percentile": epss_percentile,
                "cvss_assessments": cvss_assessments,
                "advisories": advisories,
                "nuclei_metadata": nuclei_metadata,
                "snapshot": snapshot,
            }
        });
        let source_location = json!({"file": cfg.source_path, "snapshot": snapshot});
        items.push(ExtendedItem {
            kind: "catalog_entry".to_string(),
            item_key: cve_id.to_string(),
            title,
            description: Some(
                "CVSS/EPSS and advisory metadata aggregated by PoC Index; no PoC body included"
                    .to_string(),
            ),
            body,
            category_path: vec!["PoC Index".to_string(), "CISA KEV enrichment".to_string()],
            canonical_url: Some("https://github.com/0xMarcio/pocindex".to_string()),
            source_location,
            meta: Some(meta),
            license_status: snap.license_status.clone(),
            rights_policy: cfg.license_policy.clone(),
            source_type: "http_snapshot".to_string(),
            source_url: "https://pocindex.io".to_string(),
            source_path: cfg.source_path.clone(),
            source_commit: Some(repo_commit.to_string()),
        });
    }
    Ok((items, kp::FilterStats::default()))
}

fn is_cve_identifier(value: &str) -> bool {
    let Some(rest) = value.strip_prefix("CVE-") else {
        return false;
    };
    let Some((year, sequence)) = rest.split_once('-') else {
        return false;
    };
    year.len() == 4
        && year.bytes().all(|b| b.is_ascii_digit())
        && (4..=19).contains(&sequence.len())
        && sequence.bytes().all(|b| b.is_ascii_digit())
}

fn is_cwe_identifier(value: &str) -> bool {
    let Some(sequence) = value.strip_prefix("CWE-") else {
        return false;
    };
    !sequence.is_empty() && sequence.bytes().all(|b| b.is_ascii_digit())
}

fn is_iso_date(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() == 10
        && bytes[4] == b'-'
        && bytes[7] == b'-'
        && bytes
            .iter()
            .enumerate()
            .all(|(i, b)| matches!(i, 4 | 7) || b.is_ascii_digit())
}

/// document_markdown：Markdown 文档正文（markdown_doc）。
pub fn parse_document_markdown(
    cfg: &KindConfig,
    package_dir: &Path,
    snap: &SnapshotCtx,
) -> Result<(Vec<ExtendedItem>, kp::FilterStats), RuntimeError> {
    let mut items = Vec::new();
    for file in resolve_md_files(package_dir, &cfg.source_path)? {
        let text = std::fs::read_to_string(&file)
            .map_err(|e| RuntimeError::new("io", format!("read {file:?}: {e}")))?;
        let rel = rel_to_package(package_dir, &file);
        let fm = kp::strip_frontmatter(&text);
        let headings = kp::extract_headings(&text);
        let title = fm
            .pairs
            .get("title")
            .cloned()
            .or_else(|| headings.first().map(|h| h.text.clone()))
            .unwrap_or_else(|| {
                file.file_stem()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_default()
            });
        // 分类路径：h1/h2 级前三个标题（文档级分类，非逐段路径）。
        let category_path: Vec<String> = headings
            .iter()
            .filter(|h| h.level <= 2)
            .take(3)
            .map(|h| h.text.clone())
            .collect();
        let anchor = headings
            .first()
            .map(|h| kp::slugify(&h.text))
            .filter(|s| !s.is_empty());
        let body = fm.body;
        let source_location = json!({
            "file": rel,
            "line": headings.first().map(|h| h.line),
            "anchor": anchor,
        });
        let meta = json!({
            "frontmatter_present": fm.present,
            "frontmatter": fm.pairs,
        });
        items.push(with_snapshot(
            ExtendedItem {
                kind: "document_markdown".to_string(),
                item_key: rel,
                title,
                description: fm.pairs.get("description").cloned(),
                body,
                category_path,
                canonical_url: None,
                source_location,
                meta: Some(meta),
                license_status: String::new(),
                rights_policy: String::new(),
                source_type: String::new(),
                source_url: String::new(),
                source_path: String::new(),
                source_commit: None,
            },
            cfg,
            snap,
        ));
    }
    Ok((items, kp::FilterStats::default()))
}

/// skill_candidate：Skill/Agent/MCP 候选元数据（skill_frontmatter）。
pub fn parse_skill_candidate(
    cfg: &KindConfig,
    package_dir: &Path,
    snap: &SnapshotCtx,
) -> Result<(Vec<ExtendedItem>, kp::FilterStats), RuntimeError> {
    let mut items = Vec::new();
    for file in resolve_md_files(package_dir, &cfg.source_path)? {
        let text = std::fs::read_to_string(&file)
            .map_err(|e| RuntimeError::new("io", format!("read {file:?}: {e}")))?;
        let rel = rel_to_package(package_dir, &file);
        let fm = kp::strip_frontmatter(&text);
        let name = fm.pairs.get("name").cloned().unwrap_or_else(|| {
            file.file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default()
        });
        let description = fm.pairs.get("description").cloned();
        // metadata_only 语义：正文 = 声明的描述（正文列允许短，ADR 第五节）。
        let body = description.clone().unwrap_or_default();
        let source_location = json!({
            "file": rel,
            "line": if fm.present { Some(1u64) } else { None },
            "anchor": kp::slugify(&name),
        });
        let meta = json!({
            "candidate_kind": "skill",
            "frontmatter_present": fm.present,
            "frontmatter": fm.pairs,
        });
        items.push(with_snapshot(
            ExtendedItem {
                kind: "skill_candidate".to_string(),
                item_key: rel,
                title: name,
                description: description.clone(),
                body,
                category_path: Vec::new(),
                canonical_url: None,
                source_location,
                meta: Some(meta),
                license_status: String::new(),
                rights_policy: String::new(),
                source_type: String::new(),
                source_url: String::new(),
                source_path: String::new(),
                source_commit: None,
            },
            cfg,
            snap,
        ));
    }
    Ok((items, kp::FilterStats::default()))
}

fn parse_nuclei_template_items_from_root(
    cfg: &KindConfig,
    package_dir: &Path,
    template_root: &Path,
    snap: &SnapshotCtx,
) -> Result<(Vec<ExtendedItem>, kp::FilterStats), RuntimeError> {
    const OFFICIAL_SOURCE_URL: &str = "https://github.com/projectdiscovery/nuclei-templates";
    let source_commit = cfg
        .source_commit
        .as_deref()
        .or(snap.source_commit.as_deref())
        .filter(|commit| commit.len() == 40 && commit.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .map(str::to_ascii_lowercase)
        .ok_or_else(|| {
            RuntimeError::new(
                "nuclei_source_commit_invalid",
                "Nuclei source requires a full 40-character hexadecimal commit pin",
            )
        })?;
    let db_path = package_dir.join(&cfg.source_path);
    let conn = database::rusqlite::Connection::open_with_flags(
        &db_path,
        database::rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY
            | database::rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|e| {
        RuntimeError::new(
            "ingest_source_open_failed",
            format!("open RTIS source {db_path:?} ro for Nuclei templates: {e}"),
        )
    })?;
    let mut stmt = conn
        .prepare(
            "SELECT id, code, title, description, usage, severity, cve, tags, path
             FROM poc
             WHERE category = 'nuclei'
               AND source = 'nuclei'
               AND source_url = ?1
             ORDER BY id",
        )
        .map_err(|e| {
            RuntimeError::new("ingest_source_read_failed", format!("Nuclei query: {e}"))
        })?;
    let rows = stmt
        .query_map([OFFICIAL_SOURCE_URL], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, Option<String>>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, Option<String>>(3)?,
                r.get::<_, Option<String>>(4)?,
                r.get::<_, Option<String>>(5)?,
                r.get::<_, Option<String>>(6)?,
                r.get::<_, Option<String>>(7)?,
                r.get::<_, Option<String>>(8)?,
            ))
        })
        .map_err(|e| RuntimeError::new("ingest_source_read_failed", format!("Nuclei rows: {e}")))?;

    let mut items = Vec::new();
    for row in rows {
        let (
            id,
            code,
            upstream_title,
            upstream_description,
            usage,
            upstream_severity,
            cve,
            tags,
            path,
        ) = row.map_err(|e| {
            RuntimeError::new("ingest_source_read_failed", format!("Nuclei row: {e}"))
        })?;
        let yaml_body = path
            .as_deref()
            .and_then(|p| read_nuclei_template_body_from_root(template_root, p));
        let body_source = if yaml_body.is_some() {
            "nuclei_template_file"
        } else {
            "body_unavailable"
        };
        let template_meta = yaml_body
            .as_deref()
            .map(nuclei_template_metadata)
            .unwrap_or_else(nuclei_template_metadata_unavailable);
        let title = template_meta
            .get("name")
            .and_then(Value::as_str)
            .filter(|s| !s.trim().is_empty())
            .unwrap_or(&upstream_title)
            .to_string();
        let description = template_meta
            .get("description")
            .and_then(Value::as_str)
            .filter(|s| !s.trim().is_empty())
            .map(str::to_string)
            .or(upstream_description);
        let severity = template_meta
            .get("severity")
            .and_then(Value::as_str)
            .or(upstream_severity.as_deref())
            .map(str::to_ascii_lowercase);
        let relative_path = path.as_deref().and_then(|value| {
            Path::new(value)
                .strip_prefix(NUCLEI_TEMPLATE_SOURCE_ROOT)
                .ok()
                .map(|p| p.to_string_lossy().replace('\\', "/"))
        });
        let item_key = relative_path
            .clone()
            .filter(|s| !s.is_empty() && !s.starts_with("../"))
            .unwrap_or_else(|| format!("rtis-poc:{id}"));
        let body = yaml_body
            .or(usage)
            .or_else(|| description.clone())
            .unwrap_or_default();
        let protocols = template_meta
            .get("protocols")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let template_tags = template_meta
            .get("tags")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let category_path = std::iter::once("Nuclei".to_string())
            .chain(severity.clone())
            .collect::<Vec<_>>();
        let license_evidence_url = format!("{OFFICIAL_SOURCE_URL}/blob/{source_commit}/LICENSE.md");
        let source_location = json!({
            "file": relative_path,
            "line": 1,
            "anchor": template_meta.get("template_id").and_then(Value::as_str),
        });
        let meta = json!({
            "nuclei_template": template_meta,
            "rtis_source": {
                "poc_id": id,
                "code": code,
                "cve": cve,
                "tags": tags,
                "severity": upstream_severity,
                "source_path": path,
                "body_source": body_source,
            },
            "protocols": protocols,
            "tags": template_tags,
            "source_license": {
                "spdx_id": "MIT",
                "evidence_url": license_evidence_url,
            },
        });
        items.push(ExtendedItem {
            kind: "nuclei_template".to_string(),
            item_key,
            title,
            description,
            body,
            category_path,
            canonical_url: None,
            source_location,
            meta: Some(meta),
            // Nuclei templates are a separate upstream source with a pinned commit.
            license_status: "MIT".to_string(),
            rights_policy: cfg.license_policy.clone(),
            source_type: "git".to_string(),
            source_url: OFFICIAL_SOURCE_URL.to_string(),
            source_path: cfg.source_path.clone(),
            source_commit: Some(source_commit.clone()),
        });
    }
    Ok((items, kp::FilterStats::default()))
}

/// IngestReport 单行 JSON。
pub fn report_json(report: &IngestReport) -> Value {
    json!({
        "plugin_id": report.plugin_id,
        "plugin_version": report.plugin_version,
        "corpus_id": report.corpus_id,
        "counts": report.counts,
        "skipped": report.skipped,
        "skipped_by_kind": report.skipped_by_kind,
        "total_bytes": report.total_bytes,
        "changed": report.changed,
        "filtered": report.filtered,
        "dry_run": report.dry_run,
    })
}

// ---------------------------------------------------------------------------
// RTIS 三类采集（P6-5 原逻辑，逐行保留）
// ---------------------------------------------------------------------------

fn collect_rtis_items(
    declared_kinds: &[String],
    package_dir: &Path,
) -> Result<Vec<CorpusItem>, RuntimeError> {
    let mut items: Vec<CorpusItem> = Vec::new();
    let db_path = package_dir.join("knowledge.db");
    let want_poc = declared_kinds.iter().any(|k| k == "poc");
    let want_skill = declared_kinds.iter().any(|k| k == "skill_md");
    let want_method = declared_kinds.iter().any(|k| k == "method_case");
    let (curated_meta, curated_relations) = if want_poc {
        load_rtis_meta_overlay(package_dir)?
    } else {
        (BTreeMap::new(), BTreeMap::new())
    };

    // 1) knowledge.db（mode=ro 只读，生产文件零改动）。
    if want_poc && db_path.is_file() {
        let conn = database::rusqlite::Connection::open_with_flags(
            &db_path,
            database::rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY
                | database::rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .map_err(|e| {
            RuntimeError::new(
                "ingest_source_open_failed",
                format!("open {db_path:?} ro: {e}"),
            )
        })?;
        // Older RTIS snapshots predate verified_note; select a NULL compatibility
        // column when absent so ingestion remains backward-compatible.
        let has_verified_note: bool = conn
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM pragma_table_info('poc') WHERE name='verified_note')",
                [],
                |row| row.get(0),
            )
            .map_err(|e| {
                RuntimeError::new("ingest_source_read_failed", format!("poc schema: {e}"))
            })?;
        let has_column = |name: &str| -> Result<bool, RuntimeError> {
            conn.query_row(
                "SELECT EXISTS(SELECT 1 FROM pragma_table_info('poc') WHERE name = ?1)",
                [name],
                |row| row.get(0),
            )
            .map_err(|e| {
                RuntimeError::new(
                    "ingest_source_read_failed",
                    format!("poc schema {name}: {e}"),
                )
            })
        };
        let source_col = if has_column("source")? {
            "source"
        } else {
            "NULL"
        };
        let source_url_col = if has_column("source_url")? {
            "source_url"
        } else {
            "NULL"
        };
        let path_col = if has_column("path")? { "path" } else { "NULL" };
        let verified_note_col = if has_verified_note {
            "verified_note"
        } else {
            "NULL"
        };
        let poc_sql = format!(
            "SELECT id, code, title, content, usage, description, kind, category, cve,
                    component, versions, severity, language, tags, verified, {verified_note_col},
                    {source_col}, {source_url_col}, {path_col}
             FROM poc ORDER BY id"
        );
        let mut stmt = conn.prepare(&poc_sql).map_err(|e| {
            RuntimeError::new("ingest_source_read_failed", format!("poc table: {e}"))
        })?;
        let rows = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, Option<String>>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, Option<String>>(3)?,
                    r.get::<_, Option<String>>(4)?,
                    r.get::<_, Option<String>>(5)?,
                    r.get::<_, Option<String>>(6)?,
                    r.get::<_, Option<String>>(7)?,
                    r.get::<_, Option<String>>(8)?,
                    r.get::<_, Option<String>>(9)?,
                    r.get::<_, Option<String>>(10)?,
                    r.get::<_, Option<String>>(11)?,
                    r.get::<_, Option<String>>(12)?,
                    r.get::<_, Option<String>>(13)?,
                    r.get::<_, i64>(14)?,
                    r.get::<_, Option<String>>(15)?,
                    r.get::<_, Option<String>>(16)?,
                    r.get::<_, Option<String>>(17)?,
                    r.get::<_, Option<String>>(18)?,
                ))
            })
            .map_err(|e| {
                RuntimeError::new("ingest_source_read_failed", format!("poc query: {e}"))
            })?;
        for row in rows {
            let (
                id,
                code,
                title,
                content,
                usage,
                description,
                kind,
                category,
                cve,
                component,
                versions,
                severity,
                language,
                tags,
                verified,
                verified_note,
                source,
                source_url,
                source_path,
            ) = row.map_err(|e| {
                RuntimeError::new("ingest_source_read_failed", format!("poc row: {e}"))
            })?;
            let content_body = content.filter(|s| !s.trim().is_empty());
            // Nuclei rows intentionally keep `poc.content` empty and point to the
            // installed official template tree. Read only the known template root,
            // only for the trusted source marker, and only YAML files under 2 MiB.
            // If the local template tree is absent (e.g. another platform), retain
            // the existing usage/description fallback instead of failing ingestion.
            let template_body = if content_body.is_none()
                && category.as_deref() == Some("nuclei")
                && source.as_deref() == Some("nuclei")
                && source_url.as_deref()
                    == Some("https://github.com/projectdiscovery/nuclei-templates")
            {
                source_path.as_deref().and_then(read_nuclei_template_body)
            } else {
                None
            };
            let (body, body_source) = if let Some(body) = content_body {
                (body, "knowledge.db.content")
            } else if let Some(body) = template_body {
                (body, "nuclei_template_file")
            } else if let Some(body) = usage.clone().filter(|s| !s.trim().is_empty()) {
                (body, "poc.usage")
            } else if let Some(body) = description.clone().filter(|s| !s.trim().is_empty()) {
                (body, "poc.description")
            } else {
                (String::new(), "empty")
            };
            // Preserve the upstream verification caveat and descriptive summary.
            // A boolean `verified=true` alone can overstate partial verification
            // (e.g. only a self-account path was tested, not arbitrary third parties).
            let mut meta = json!({
                "code": code, "kind": kind, "category": category, "cve": cve,
                "component": component, "versions": versions, "severity": severity,
                "language": language, "tags": tags, "verified": verified != 0,
                "verified_note": verified_note, "description": description, "usage": usage,
                "source": source, "source_url": source_url, "source_path": source_path,
                "body_source": body_source,
            });
            if category.as_deref() == Some("nuclei")
                && source.as_deref() == Some("nuclei")
                && source_url.as_deref()
                    == Some("https://github.com/projectdiscovery/nuclei-templates")
            {
                // Keep the raw YAML as the searchable body, while exposing its
                // stable fields separately for filtering and structured retrieval.
                // Summary fallbacks are never parsed as if they were template YAML.
                if body_source == "nuclei_template_file" {
                    meta["nuclei_template"] = nuclei_template_metadata(&body);
                } else {
                    meta["nuclei_template"] = nuclei_template_metadata_unavailable();
                    meta["nuclei_template"]["template_path"] = json!(source_path);
                }
            }
            if let Some(curated) = curated_meta.get(&id) {
                apply_curation_metadata(&mut meta, curated);
            }
            if let Some(code) = code.as_deref() {
                if let Some(edges) = curated_relations.get(code) {
                    meta["curation_relations"] = json!(edges);
                }
            }
            let body_len = body.len();
            items.push((
                "poc".to_string(),
                title,
                Some(id.to_string()),
                body,
                body_len,
                Some(meta),
            ));
        }
    }

    // 2) skills/*.md（确定性排序）。
    if want_skill {
        let skills = package_dir.join("skills");
        if skills.is_dir() {
            let mut files: Vec<_> = std::fs::read_dir(&skills)
                .map_err(|e| RuntimeError::new("io", format!("read_dir skills: {e}")))?
                .filter_map(|e| e.ok().map(|e| e.path()))
                .filter(|p| p.is_file() && p.extension().map(|e| e == "md").unwrap_or(false))
                .collect();
            files.sort();
            for path in files {
                let title = path
                    .file_stem()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_default();
                let body = std::fs::read_to_string(&path)
                    .map_err(|e| RuntimeError::new("io", format!("read {path:?}: {e}")))?;
                let body_len = body.len();
                items.push(("skill_md".to_string(), title, None, body, body_len, None));
            }
        }
    }

    // 3) pocs/ 递归全部文件（确定性排序）。
    if want_method {
        let pocs = package_dir.join("pocs");
        if pocs.is_dir() {
            let mut files = Vec::new();
            collect_files(&pocs, &mut files)?;
            files.sort();
            for path in files {
                let relative = path.strip_prefix(&pocs).unwrap_or(&path);
                if is_generated_method_case_noise(relative) {
                    continue;
                }
                let rel = relative.to_string_lossy().to_string();
                let body = std::fs::read(&path)
                    .map_err(|e| RuntimeError::new("io", format!("read {path:?}: {e}")))?;
                let body = String::from_utf8_lossy(&body).to_string();
                // Empty files and generated `stub` notes are not useful retrieval
                // documents. Their source files remain untouched in the RTIS tree.
                if body.trim().is_empty()
                    || (path.file_name().is_some_and(|name| name == "NOTES.md")
                        && body
                            .lines()
                            .any(|line| line.trim().starts_with("> 状态: `stub`")))
                {
                    continue;
                }
                let title = rel;
                let body_len = body.len();
                items.push(("method_case".to_string(), title, None, body, body_len, None));
            }
        }
    }
    Ok(items)
}

/// Parse stable Nuclei template fields into a compact, queryable metadata object.
/// The original YAML remains the document body; this projection is additive and
/// deliberately does not infer exploitability or verification from template fields.
fn nuclei_template_metadata(body: &str) -> Value {
    let template_content_sha256 = sha256_hex(body.as_bytes());
    let status = |parse_status: &str, yaml_syntax_status: &str, shape_status: &str| {
        json!({
            "parse_status": parse_status,
            "yaml_syntax_status": yaml_syntax_status,
            "template_shape_status": shape_status,
            "detection_validation_status": "NOT_TESTED",
            "template_content_sha256": template_content_sha256,
        })
    };
    let yaml: serde_yaml_ng::Value = match serde_yaml_ng::from_str(body) {
        Ok(value) => value,
        Err(_) => return status("invalid_yaml", "invalid", "invalid_yaml"),
    };
    let root = match serde_json::to_value(yaml) {
        Ok(Value::Object(value)) => Value::Object(value),
        Ok(_) => return status("invalid_template", "valid", "root_not_mapping"),
        Err(_) => return status("invalid_template", "valid", "not_json_compatible"),
    };
    let Some(info) = root.get("info").and_then(Value::as_object) else {
        let has_id = root.get("id").and_then(Value::as_str).is_some();
        return status(
            if has_id {
                "missing_info"
            } else {
                "not_template"
            },
            "valid",
            if has_id {
                "missing_info"
            } else {
                "not_template"
            },
        );
    };
    let Some(template_id) = root.get("id").and_then(Value::as_str) else {
        return status("missing_template_id", "valid", "missing_template_id");
    };

    fn values(value: Option<&Value>, split_commas: bool) -> Vec<String> {
        let mut out = match value {
            Some(Value::String(value)) => {
                if split_commas {
                    value
                        .split(',')
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .map(str::to_string)
                        .collect()
                } else {
                    vec![value.trim().to_string()]
                }
            }
            Some(Value::Array(items)) => items
                .iter()
                .filter_map(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .collect(),
            Some(value) if value.is_number() || value.is_boolean() => vec![value.to_string()],
            _ => Vec::new(),
        };
        out.sort();
        out.dedup();
        out
    }

    fn collect_named(value: &Value, key: &str, out: &mut Vec<Value>) {
        match value {
            Value::Object(object) => {
                for (name, child) in object {
                    if name == key {
                        match child {
                            Value::Array(items) => out.extend(items.iter().cloned()),
                            Value::Null => {}
                            value => out.push(value.clone()),
                        }
                    }
                    collect_named(child, key, out);
                }
            }
            Value::Array(items) => {
                for item in items {
                    collect_named(item, key, out);
                }
            }
            _ => {}
        }
    }

    const PROTOCOL_KEYS: [&str; 13] = [
        "http",
        "dns",
        "tcp",
        "ssl",
        "websocket",
        "headless",
        "code",
        "javascript",
        "file",
        "network",
        "whois",
        "cloud",
        "workflows",
    ];
    let protocols: Vec<String> = PROTOCOL_KEYS
        .iter()
        .filter(|key| root.get(**key).is_some())
        .map(|key| (*key).to_string())
        .collect();
    let request_count: usize = PROTOCOL_KEYS
        .iter()
        .map(|key| {
            root.get(*key)
                .map(|value| value.as_array().map_or(1, Vec::len))
                .unwrap_or(0)
        })
        .sum();

    let classification = info.get("classification").cloned().unwrap_or(Value::Null);
    let template_metadata = info.get("metadata").cloned().unwrap_or(Value::Null);
    let mut matchers = Vec::new();
    collect_named(&root, "matchers", &mut matchers);
    let mut extractors = Vec::new();
    collect_named(&root, "extractors", &mut extractors);
    let mut projected = status("parsed", "valid", "metadata_projected");
    let fields = json!({
        "template_id": template_id,
        "name": info.get("name").and_then(Value::as_str),
        "author": values(info.get("author"), false),
        "severity": info.get("severity").and_then(Value::as_str)
            .map(str::to_ascii_lowercase),
        "description": info.get("description").and_then(Value::as_str),
        "tags": values(info.get("tags"), true),
        "references": values(info.get("reference"), false),
        "classification": classification,
        "template_metadata": template_metadata,
        "protocols": protocols,
        "request_block_count": request_count,
        "matcher_count": matchers.len(),
        "extractor_count": extractors.len(),
        "matchers": matchers,
        "extractors": extractors,
        "has_flow": root.get("flow").is_some(),
    });
    projected
        .as_object_mut()
        .expect("Nuclei metadata status is a JSON object")
        .extend(
            fields
                .as_object()
                .expect("Nuclei metadata fields are a JSON object")
                .clone(),
        );
    projected
}

fn nuclei_template_metadata_unavailable() -> Value {
    json!({
        "parse_status": "body_unavailable",
        "yaml_syntax_status": "unavailable",
        "template_shape_status": "unavailable",
        "detection_validation_status": "NOT_TESTED",
        "template_content_sha256": Value::Null,
    })
}

/// Read a Nuclei template only when its source path maps below the trusted root.
/// Canonicalization rejects traversal and symlinks that escape the root.
fn read_nuclei_template_body(source_path: &str) -> Option<String> {
    read_nuclei_template_body_from_root(Path::new(NUCLEI_TEMPLATE_SOURCE_ROOT), source_path)
}

fn read_nuclei_template_body_from_root(root: &Path, source_path: &str) -> Option<String> {
    let canonical_root = root.canonicalize().ok()?;
    let relative = Path::new(source_path)
        .strip_prefix(NUCLEI_TEMPLATE_SOURCE_ROOT)
        .ok()?;
    if relative.as_os_str().is_empty()
        || relative.components().any(|component| {
            matches!(
                component,
                std::path::Component::ParentDir
                    | std::path::Component::RootDir
                    | std::path::Component::Prefix(_)
            )
        })
        || !matches!(
            relative.extension().and_then(|ext| ext.to_str()),
            Some("yaml" | "yml")
        )
    {
        return None;
    }
    let resolved = canonical_root.join(relative).canonicalize().ok()?;
    if !resolved.starts_with(&canonical_root) || !resolved.is_file() {
        return None;
    }
    if resolved.metadata().ok()?.len() > MAX_NUCLEI_TEMPLATE_BYTES {
        return None;
    }
    std::fs::read_to_string(resolved).ok()
}

type RtIsCurationOverlay = (BTreeMap<i64, Value>, BTreeMap<String, Vec<Value>>);

/// Load the platform-owned curation overlay without mutating the protected RTIS DB.
/// Existing RTIS k_meta rows always win; overlay rows only fill metadata gaps.
fn load_rtis_meta_overlay(package_dir: &Path) -> Result<RtIsCurationOverlay, RuntimeError> {
    let path = package_dir.join("curation/k_meta_overlay.json");
    if !path.is_file() {
        return Ok((BTreeMap::new(), BTreeMap::new()));
    }
    let raw = std::fs::read_to_string(&path)
        .map_err(|e| RuntimeError::new("curation_overlay_read_failed", format!("{path:?}: {e}")))?;
    let root: Value = serde_json::from_str(&raw)
        .map_err(|e| RuntimeError::new("curation_overlay_invalid", format!("{path:?}: {e}")))?;
    if root.get("schema_version").and_then(Value::as_i64) != Some(1) {
        return Err(RuntimeError::new(
            "curation_overlay_invalid",
            format!("unsupported schema_version in {path:?}"),
        ));
    }
    let mut metadata = BTreeMap::new();
    for entry in root
        .get("entries")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let id = entry
            .get("knowledge_id")
            .and_then(Value::as_i64)
            .ok_or_else(|| {
                RuntimeError::new(
                    "curation_overlay_invalid",
                    "entry missing integer knowledge_id",
                )
            })?;
        if metadata.insert(id, entry.clone()).is_some() {
            return Err(RuntimeError::new(
                "curation_overlay_invalid",
                format!("duplicate knowledge_id {id} in {path:?}"),
            ));
        }
    }
    let mut relations: BTreeMap<String, Vec<Value>> = BTreeMap::new();
    for edge in root
        .get("relations")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        for endpoint in ["src", "dst"] {
            if let Some(key) = edge.get(endpoint).and_then(Value::as_str) {
                relations
                    .entry(key.to_string())
                    .or_default()
                    .push(edge.clone());
            }
        }
    }
    Ok((metadata, relations))
}

/// Apply RIP-owned curation metadata to the searchable projection only.
/// The RTIS source database remains read-only and retains its original note.
fn apply_curation_metadata(meta: &mut Value, curated: &Value) {
    if curated.get("redact_verified_note").and_then(Value::as_bool) == Some(true) {
        let replacement = curated
            .get("redacted_verified_note")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .unwrap_or("[REDACTED: sensitive verification evidence]");
        meta["verified_note"] = json!(replacement);
    }
    for field in [
        "evidence_level",
        "lifecycle",
        "dedup_status",
        "source_integrity",
        "verification_result",
        "independent_knowledge_status",
        "missing_evidence",
        "source_path",
        "source_commit",
        "evidence_refs",
    ] {
        if let Some(value) = curated.get(field) {
            meta[field] = value.clone();
        }
    }
    meta["curation"] = curated.clone();
}

/// Exclude generated/interpreter artifacts and hidden corpus indexes from the
/// method-case retrieval corpus. This is an ingest-only filter: source files are
/// never removed or modified.
fn is_generated_method_case_noise(relative: &Path) -> bool {
    if relative
        .components()
        .any(|component| component.as_os_str() == "__pycache__")
    {
        return true;
    }
    let Some(name) = relative.file_name() else {
        return true;
    };
    if name.to_string_lossy().starts_with('.') || name == "gen_notes.py" {
        return true;
    }
    matches!(
        relative.extension().and_then(|ext| ext.to_str()),
        Some("pyc" | "pyo")
    )
}

fn collect_files(dir: &Path, out: &mut Vec<std::path::PathBuf>) -> Result<(), RuntimeError> {
    for entry in std::fs::read_dir(dir)
        .map_err(|e| RuntimeError::new("io", format!("read_dir {dir:?}: {e}")))?
    {
        let entry = entry.map_err(|e| RuntimeError::new("io", format!("entry: {e}")))?;
        let path = entry.path();
        if path.is_dir() {
            collect_files(&path, out)?;
        } else {
            out.push(path);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn method_case_ingest_excludes_generated_noise_and_stub_notes() {
        let root = std::env::temp_dir().join(format!("rip-method-filter-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let pocs = root.join("pocs");
        std::fs::create_dir_all(pocs.join("case-a/__pycache__")).unwrap();
        std::fs::create_dir_all(pocs.join("case-b")).unwrap();
        std::fs::create_dir_all(pocs.join("_lib/__pycache__")).unwrap();
        std::fs::write(pocs.join("case-a/poc.sh"), "#!/bin/sh\necho safe\n").unwrap();
        std::fs::write(
            pocs.join("case-b/NOTES.md"),
            "> 状态: `script_ok` — 配套脚本 `poc.sh`\n\n实证说明。\n",
        )
        .unwrap();
        std::fs::write(
            pocs.join("case-a/NOTES.md"),
            "> 状态: `stub` — 只有方法论，无自动脚本。\n",
        )
        .unwrap();
        std::fs::write(pocs.join("_lib/gen_notes.py"), "# internal generator\n").unwrap();
        std::fs::write(
            pocs.join("_lib/__pycache__/gen_notes.cpython-314.pyc"),
            [0, 1, 2],
        )
        .unwrap();
        std::fs::write(pocs.join(".nuclei-index.json"), "{}\n").unwrap();
        std::fs::write(pocs.join("empty.txt"), "  ").unwrap();

        let items = collect_rtis_items(&["method_case".to_string()], &root).unwrap();
        let titles: Vec<&str> = items.iter().map(|item| item.1.as_str()).collect();
        assert_eq!(titles, vec!["case-a/poc.sh", "case-b/NOTES.md"]);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    #[ignore = "manual end-to-end ingestion against the installed RTIS/Nuclei corpus; disposable in-memory DB"]
    fn audit_rtis_nuclei_full_ingest_into_disposable_database() {
        let package = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../integrations/rtis");
        let manifest_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../schemas/plugin-manifest/examples/knowledge.rtis-redteam.json");
        let manifest: Value =
            serde_json::from_slice(&std::fs::read(manifest_path).unwrap()).unwrap();
        let pool = database::Pool::open(database::PoolConfig {
            path: database::DbPath::Memory,
            readonly: false,
            max_connections: 1,
            busy_timeout: std::time::Duration::from_secs(10),
        })
        .unwrap();
        let migrations = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations");
        database::migrate(&pool, &migrations).unwrap();
        let report = ingest_knowledge_source_with_opts(
            &pool,
            &manifest,
            &package,
            &IngestOptions::default(),
        )
        .expect("complete RTIS/Nuclei ingestion into disposable memory DB");
        assert_eq!(report.counts.get("nuclei_template"), Some(&14_111));
        let (total, discovered, parsed, invalid, unavailable): (i64, i64, i64, i64, i64) =
            pool.with_conn(|c| {
                c.query_row(
                    "SELECT COUNT(*),
                            SUM(review_status='DISCOVERED'),
                            SUM(json_extract(meta_json,'$.nuclei_template.parse_status')='parsed'),
                            SUM(json_extract(meta_json,'$.nuclei_template.parse_status')='invalid_yaml'),
                            SUM(json_extract(meta_json,'$.nuclei_template.parse_status')='body_unavailable')
                     FROM knowledge_source_entry
                     WHERE plugin_id='knowledge.rtis-redteam' AND entry_kind='nuclei_template'",
                    [],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
                )
            })
            .unwrap();
        assert_eq!(total, 14_111);
        assert_eq!(
            discovered, 14_111,
            "all templates must enter the review queue"
        );
        assert_eq!(parsed, 14_109);
        assert_eq!(invalid, 1);
        assert_eq!(unavailable, 1);
        let legacy_count: i64 = pool
            .with_conn(|c| {
                c.query_row(
                    "SELECT COUNT(*) FROM knowledge_ingest
                     WHERE plugin_id='knowledge.rtis-redteam' AND content_kind IN ('poc','skill_md','method_case')",
                    [],
                    |r| r.get(0),
                )
            })
            .unwrap();
        for code in [
            "n-a-configuration-exposure-spring-cloud-alibaba-actuator-泄露-nacos-凭据-绕过-show-val",
            "com.mltech.sign-签名密钥下发接口未授权利用-自传公钥换回-hmac-key-免签路径清单-rsa-oaep-sha256-aes-256-gcm",
            "明文身份-cookie-伪造-user_id-user_time-无签名即完成鉴权-thinkphp-后台任意用户接管",
        ] {
            let (note, redact_flag): (String, i64) = pool
                .with_conn(|c| {
                    c.query_row(
                        "SELECT json_extract(meta_json,'$.verified_note'),
                                json_extract(meta_json,'$.curation.redact_verified_note')
                         FROM knowledge_ingest
                         WHERE plugin_id='knowledge.rtis-redteam'
                           AND content_kind='poc'
                           AND json_extract(meta_json,'$.code')=?1",
                        [code],
                        |r| Ok((r.get(0)?, r.get(1)?)),
                    )
                })
                .unwrap();
            assert!(note.starts_with("[REDACTED:"), "sensitive note leaked for {code}");
            assert_eq!(redact_flag, 1, "redaction marker missing for {code}");
        }

        let overlay: Value = serde_json::from_slice(
            &std::fs::read(package.join("curation/k_meta_overlay.json")).unwrap(),
        )
        .unwrap();
        let overlay_entries = overlay["entries"].as_array().unwrap();
        for entry in overlay_entries {
            let code = entry["code"].as_str().unwrap();
            let projected: String = pool
                .with_conn(|c| {
                    c.query_row(
                        "SELECT meta_json FROM knowledge_ingest
                         WHERE plugin_id='knowledge.rtis-redteam'
                           AND content_kind='poc'
                           AND json_extract(meta_json,'$.code')=?1",
                        [code],
                        |r| r.get(0),
                    )
                })
                .unwrap_or_else(|e| panic!("missing projected overlay entry {code}: {e}"));
            let projected: Value = serde_json::from_str(&projected).unwrap();
            assert_eq!(
                projected["evidence_level"], entry["evidence_level"],
                "evidence facet mismatch for {code}"
            );
            assert_eq!(
                projected["lifecycle"], entry["lifecycle"],
                "lifecycle facet mismatch for {code}"
            );
            assert_eq!(
                projected["curation"]["source_path"], entry["source_path"],
                "source path mismatch for {code}"
            );
        }
        let relation_count = overlay["relations"].as_array().unwrap().len();
        let projected_relation_count: i64 = pool
            .with_conn(|c| {
                c.query_row(
                    "SELECT COUNT(DISTINCT json_extract(meta_json,'$.curation_relations'))
                     FROM knowledge_ingest
                     WHERE plugin_id='knowledge.rtis-redteam'
                       AND content_kind='poc'
                       AND json_type(meta_json,'$.curation_relations')='array'",
                    [],
                    |r| r.get(0),
                )
            })
            .unwrap();
        assert!(projected_relation_count > 0);
        assert!(relation_count > 0, "overlay relation set must not be empty");
        println!(
            "Disposable RTIS ingest: nuclei_template={} parsed={} invalid_yaml={} body_unavailable={} legacy_rows={} overlay_entries={} relations={} projected_relation_rows={}",
            total, parsed, invalid, unavailable, legacy_count, overlay_entries.len(), relation_count, projected_relation_count
        );
    }

    #[test]
    #[ignore = "manual audit against the installed RTIS/Nuclei corpus; read-only"]
    fn audit_rtis_nuclei_template_ingest_projection() {
        let package = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../integrations/rtis");
        let cfg = KindConfig {
            source_path: "knowledge.db".to_string(),
            parser: "nuclei_yaml".to_string(),
            license_policy: "approved_only".to_string(),
            max_entries: Some(20_000),
            source_commit: Some("a346d8f00c61bf95bbd30365f12cf923be51ba3d".to_string()),
        };
        let snap = SnapshotCtx {
            license_status: "UNLICENSED".to_string(),
            source_type: "local_dir".to_string(),
            source_url: "local://rtis".to_string(),
            source_commit: None,
        };
        let (items, _) = parse_nuclei_template_items_from_root(
            &cfg,
            &package,
            Path::new(NUCLEI_TEMPLATE_SOURCE_ROOT),
            &snap,
        )
        .expect("read-only Nuclei projection");
        let statuses = items
            .iter()
            .fold(BTreeMap::<String, usize>::new(), |mut counts, item| {
                let status = item
                    .meta
                    .as_ref()
                    .and_then(|m| m["nuclei_template"]["parse_status"].as_str())
                    .unwrap_or("missing_status");
                *counts.entry(status.to_string()).or_insert(0) += 1;
                counts
            });
        let parsed = statuses.get("parsed").copied().unwrap_or_default();
        let mut keys = std::collections::HashSet::new();
        assert!(items.iter().all(|item| keys.insert(item.item_key.clone())));
        println!(
            "RTIS Nuclei KSE projection: total={} statuses={:?} sample_key={}",
            items.len(),
            statuses,
            items
                .first()
                .map(|item| item.item_key.as_str())
                .unwrap_or("(none)")
        );
        assert!(
            !items.is_empty(),
            "RTIS source query returned no Nuclei templates"
        );
        assert!(parsed > 0, "no template body was recovered");
    }

    #[test]
    fn nuclei_template_parser_reads_only_official_rows_and_keeps_discovered_provenance() {
        let root = std::env::temp_dir().join(format!("rip-nuclei-ingest-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let package = root.join("package");
        let templates = root.join("templates");
        std::fs::create_dir_all(&package).unwrap();
        std::fs::create_dir_all(templates.join("cloud/test")).unwrap();
        let yaml = r#"id: cve-2025-12345
info:
  name: Example SQL Injection
  author: alice
  severity: high
  description: SQL injection detection.
  reference:
    - https://nvd.nist.gov/vuln/detail/CVE-2025-12345
  tags: cve,sqli
  classification:
    cve-id: CVE-2025-12345
    cwe-id: CWE-89
http:
  - method: GET
    path:
      - "{{BaseURL}}/item?id=1"
    matchers:
      - type: word
        words: ["sql error"]
"#;
        std::fs::write(templates.join("cloud/test/example.yaml"), yaml).unwrap();

        let db = database::rusqlite::Connection::open(package.join("knowledge.db")).unwrap();
        db.execute_batch(
            "CREATE TABLE poc (
                id INTEGER PRIMARY KEY, code TEXT, title TEXT, description TEXT,
                usage TEXT, severity TEXT, cve TEXT, tags TEXT, path TEXT,
                category TEXT, source TEXT, source_url TEXT
             );",
        )
        .unwrap();
        db.execute(
            "INSERT INTO poc VALUES (1,'nuclei-example','upstream title','fallback description',
                'fallback usage','high','CVE-2025-12345','cve,sqli',
                '/usr/share/nuclei-templates/cloud/test/example.yaml','nuclei','nuclei',
                'https://github.com/projectdiscovery/nuclei-templates')",
            [],
        )
        .unwrap();
        db.execute(
            "INSERT INTO poc VALUES (2,'not-nuclei','must not ingest','', '', 'low','', '',
                '/tmp/untrusted.yaml','other','self','https://example.invalid')",
            [],
        )
        .unwrap();
        db.execute(
            "INSERT INTO poc VALUES (3,'missing-file','missing body','fallback description',
                'fallback usage','medium','','',
                '/usr/share/nuclei-templates/cloud/test/missing.yaml','nuclei','nuclei',
                'https://github.com/projectdiscovery/nuclei-templates')",
            [],
        )
        .unwrap();
        drop(db);

        let cfg = KindConfig {
            source_path: "knowledge.db".to_string(),
            parser: "nuclei_yaml".to_string(),
            license_policy: "approved_only".to_string(),
            max_entries: None,
            source_commit: Some("a346d8f00c61bf95bbd30365f12cf923be51ba3d".to_string()),
        };
        let snap = SnapshotCtx {
            license_status: "UNLICENSED".to_string(),
            source_type: "local_dir".to_string(),
            source_url: "local://rtis".to_string(),
            source_commit: Some("rtis-commit-must-not-leak".to_string()),
        };
        let (items, _) =
            parse_nuclei_template_items_from_root(&cfg, &package, &templates, &snap).unwrap();
        assert_eq!(
            items.len(),
            2,
            "only rows with the official source tuple are accepted"
        );
        let item = &items[0];
        assert_eq!(item.kind, "nuclei_template");
        assert_eq!(item.item_key, "cloud/test/example.yaml");
        assert_eq!(item.title, "Example SQL Injection");
        assert_eq!(
            item.description.as_deref(),
            Some("SQL injection detection.")
        );
        assert_eq!(item.body, yaml);
        assert_eq!(item.source_type, "git");
        assert_eq!(
            item.source_url,
            "https://github.com/projectdiscovery/nuclei-templates"
        );
        assert_eq!(
            item.source_commit.as_deref(),
            Some("a346d8f00c61bf95bbd30365f12cf923be51ba3d"),
            "use the pinned Nuclei revision, not the RTIS commit"
        );
        let invalid_cfg = KindConfig {
            source_commit: Some("a346d8f".to_string()),
            ..cfg.clone()
        };
        let invalid_pin =
            parse_nuclei_template_items_from_root(&invalid_cfg, &package, &templates, &snap)
                .unwrap_err();
        assert_eq!(invalid_pin.code, "nuclei_source_commit_invalid");
        assert_eq!(item.license_status, "MIT");
        assert_eq!(item.rights_policy, "approved_only");
        assert_eq!(
            item.meta.as_ref().unwrap()["nuclei_template"]["classification"]["cve-id"],
            "CVE-2025-12345"
        );
        assert_eq!(
            item.meta.as_ref().unwrap()["nuclei_template"]["severity"],
            "high"
        );
        assert_eq!(
            item.meta.as_ref().unwrap()["source_license"]["evidence_url"],
            "https://github.com/projectdiscovery/nuclei-templates/blob/a346d8f00c61bf95bbd30365f12cf923be51ba3d/LICENSE.md"
        );
        assert!(item.meta.as_ref().unwrap()["source_license"]
            .get("local_package")
            .is_none());
        assert!(item.meta.as_ref().unwrap()["nuclei_template"]
            .get("verified")
            .is_none());
        let unavailable = &items[1];
        let unavailable_meta = &unavailable.meta.as_ref().unwrap()["nuclei_template"];
        assert_eq!(unavailable.body, "fallback usage");
        assert_eq!(unavailable_meta["parse_status"], "body_unavailable");
        assert_eq!(unavailable_meta["yaml_syntax_status"], "unavailable");
        assert_eq!(
            unavailable_meta["detection_validation_status"],
            "NOT_TESTED"
        );
        assert!(unavailable_meta["template_content_sha256"].is_null());

        // Exercise the real KSE writer and migration: parsed templates remain
        // DISCOVERED until a reviewer explicitly approves them.
        let pool = database::Pool::open(database::PoolConfig {
            path: database::DbPath::Memory,
            readonly: false,
            max_connections: 1,
            busy_timeout: std::time::Duration::from_secs(2),
        })
        .unwrap();
        let migrations = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations");
        database::migrate(&pool, &migrations).unwrap();
        let write_counts = pool
            .with_conn(|c| {
                write_all(
                    c,
                    "knowledge.rtis-redteam",
                    "0.2.0",
                    "rtis-redteam",
                    &[],
                    &items,
                    "2026-11-26T00:00:00Z",
                )
                .map(|(counts, _, _, _, _)| counts)
            })
            .unwrap();
        assert_eq!(write_counts.get("nuclei_template"), Some(&2));
        let stored: (String, String, String, String) = pool
            .with_conn(|c| {
                c.query_row(
                    "SELECT entry_kind, review_status, body, meta_json
                     FROM knowledge_source_entry
                     WHERE plugin_id='knowledge.rtis-redteam'
                       AND item_key='cloud/test/example.yaml'",
                    [],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
                )
            })
            .unwrap();
        assert_eq!(stored.0, "nuclei_template");
        assert_eq!(stored.1, "DISCOVERED");
        assert_eq!(stored.2, yaml);
        assert!(stored.3.contains("CVE-2025-12345"));

        // Metadata-only parser changes are detected, but the existing immutable
        // row is not silently overwritten under the same plugin version.
        let mut metadata_changed_items = items.clone();
        metadata_changed_items[0].meta.as_mut().unwrap()["nuclei_template"]["severity"] =
            json!("critical");
        let changed = pool
            .with_conn(|c| {
                write_all(
                    c,
                    "knowledge.rtis-redteam",
                    "0.2.0",
                    "rtis-redteam",
                    &[],
                    &metadata_changed_items,
                    "2026-11-26T00:00:01Z",
                )
                .map(|(_, _, _, _, changed)| changed)
            })
            .unwrap();
        assert_eq!(changed, 1, "metadata hash changes must be detected");
        let persisted_severity: String = pool
            .with_conn(|c| {
                c.query_row(
                    "SELECT json_extract(meta_json,'$.nuclei_template.severity')
                     FROM knowledge_source_entry WHERE plugin_id='knowledge.rtis-redteam'",
                    [],
                    |r| r.get(0),
                )
            })
            .unwrap();
        assert_eq!(
            persisted_severity, "high",
            "same-version re-ingest must not overwrite historical metadata"
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    fn nuclei_test_manifest(version: &str, source_commit: &str, max_entries: Option<u64>) -> Value {
        let mut kind = json!({
            "source_path": "knowledge.db",
            "parser": "nuclei_yaml",
            "license_policy": "approved_only",
            "source_commit": source_commit,
        });
        if let Some(max_entries) = max_entries {
            kind["max_entries"] = json!(max_entries);
        }
        json!({
            "id": "knowledge.nuclei-lifecycle",
            "version": version,
            "license": "MIT",
            "permissions": {"data_access": INGEST_PERMISSION_REQUIRED},
            "capabilities": [{
                "type": "knowledge_source",
                "provides": {
                    "corpus_id": "nuclei-lifecycle",
                    "content_kinds": ["nuclei_template"],
                    "kinds": {"nuclei_template": kind},
                }
            }]
        })
    }

    fn write_nuclei_test_package(
        package_dir: &Path,
        template_root: &Path,
        templates: &[(&str, &str)],
    ) {
        std::fs::create_dir_all(package_dir).unwrap();
        let conn = database::rusqlite::Connection::open(package_dir.join("knowledge.db")).unwrap();
        conn.execute_batch(
            "CREATE TABLE poc (
                id INTEGER PRIMARY KEY, code TEXT, title TEXT, description TEXT,
                usage TEXT, severity TEXT, cve TEXT, tags TEXT, path TEXT,
                category TEXT, source TEXT, source_url TEXT
             );",
        )
        .unwrap();
        for (index, (relative_path, yaml)) in templates.iter().enumerate() {
            let source_path = format!("{NUCLEI_TEMPLATE_SOURCE_ROOT}/{relative_path}");
            let template_path = template_root.join(relative_path);
            std::fs::create_dir_all(template_path.parent().unwrap()).unwrap();
            std::fs::write(template_path, yaml).unwrap();
            conn.execute(
                "INSERT INTO poc VALUES (?1,?2,?3,'fixture description','fixture usage',
                    'low','','',?4,'nuclei','nuclei',
                    'https://github.com/projectdiscovery/nuclei-templates')",
                database::rusqlite::params![
                    index as i64 + 1,
                    relative_path,
                    relative_path,
                    source_path
                ],
            )
            .unwrap();
        }
    }

    fn activate_nuclei_test_version(pool: &Pool, manifest: &Value) {
        let plugin_id = manifest["id"].as_str().unwrap();
        let version = manifest["version"].as_str().unwrap();
        pool.with_conn(|c| -> Result<(), database::rusqlite::Error> {
            c.execute(
                "UPDATE plugin SET is_active = 0 WHERE plugin_id = ?1",
                [plugin_id],
            )?;
            c.execute(
                "INSERT INTO plugin (
                    plugin_id, version, name, kind, description, license, trust_level,
                    state, is_active, source_type, source_ref, manifest_json,
                    created_at, updated_at
                 ) VALUES (?1,?2,?3,'knowledge','Nuclei lifecycle fixture','MIT','official',
                    'ENABLED',1,'git',
                    'https://github.com/projectdiscovery/nuclei-templates',?4,
                    '2026-01-01T00:00:00Z','2026-01-01T00:00:00Z')",
                database::rusqlite::params![
                    plugin_id,
                    version,
                    format!("Nuclei fixture {version}"),
                    manifest.to_string()
                ],
            )?;
            Ok(())
        })
        .unwrap();
    }

    fn nuclei_test_yaml(id: &str, name: &str, description: &str) -> String {
        format!(
            "id: {id}\ninfo:\n  name: {name}\n  severity: low\n  description: {description}\nhttp:\n  - method: GET\n    path:\n      - /health\n    matchers:\n      - type: word\n        words: [ok]\n"
        )
    }

    #[test]
    fn active_nuclei_snapshots_reconcile_removals_without_erasing_history() {
        let root =
            std::env::temp_dir().join(format!("rip-nuclei-lifecycle-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let templates = root.join("templates");
        let pool = database::Pool::open(database::PoolConfig {
            path: database::DbPath::Memory,
            readonly: false,
            max_connections: 1,
            busy_timeout: std::time::Duration::from_secs(2),
        })
        .unwrap();
        let migrations = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations");
        database::migrate(&pool, &migrations).unwrap();
        let plugin_id = "knowledge.nuclei-lifecycle";
        let path_a = "http/test/template-a.yaml";
        let path_b = "http/test/template-b.yaml";
        let yaml_a = nuclei_test_yaml("template-a", "Template A", "Version one");
        let yaml_b_v1 = nuclei_test_yaml("template-b", "Template B", "Version one");
        let yaml_b_v2 = nuclei_test_yaml("template-b", "Template B", "Version two");

        let manifest_v1 = nuclei_test_manifest("1.0.0", &"1".repeat(40), None);
        let package_v1 = root.join("package-v1");
        write_nuclei_test_package(
            &package_v1,
            &templates,
            &[(path_a, &yaml_a), (path_b, &yaml_b_v1)],
        );
        activate_nuclei_test_version(&pool, &manifest_v1);
        ingest_knowledge_source_with_template_root(
            &pool,
            &manifest_v1,
            &package_v1,
            &IngestOptions::default(),
            &templates,
        )
        .unwrap();
        pool.with_conn(|c| {
            c.execute(
                "UPDATE knowledge_source_entry SET review_status = 'APPROVED'
                 WHERE plugin_id = ?1 AND plugin_version = '1.0.0' AND item_key = ?2",
                database::rusqlite::params![plugin_id, path_a],
            )
        })
        .unwrap();

        let manifest_v2 = nuclei_test_manifest("2.0.0", &"2".repeat(40), None);
        let package_v2 = root.join("package-v2");
        write_nuclei_test_package(&package_v2, &templates, &[(path_b, &yaml_b_v2)]);
        activate_nuclei_test_version(&pool, &manifest_v2);
        ingest_knowledge_source_with_template_root(
            &pool,
            &manifest_v2,
            &package_v2,
            &IngestOptions::default(),
            &templates,
        )
        .unwrap();

        let removed_a: (String, String, i64, String) = pool
            .with_conn(
                |c| -> Result<(String, String, i64, String), database::rusqlite::Error> {
                    c.query_row(
                        "SELECT body, review_status, upstream_missing, source_commit
                     FROM knowledge_source_entry
                     WHERE plugin_id = ?1 AND plugin_version = '1.0.0' AND item_key = ?2",
                        database::rusqlite::params![plugin_id, path_a],
                        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
                    )
                },
            )
            .unwrap();
        assert_eq!(removed_a.0, yaml_a);
        assert_eq!(removed_a.1, "APPROVED");
        assert_eq!(removed_a.2, 1);
        assert_eq!(removed_a.3, "1".repeat(40));

        let versioned_b: Vec<(String, String, i64)> = pool
            .with_conn(
                |c| -> Result<Vec<(String, String, i64)>, database::rusqlite::Error> {
                    let mut stmt = c.prepare(
                        "SELECT plugin_version, body, upstream_missing
                     FROM knowledge_source_entry
                     WHERE plugin_id = ?1 AND item_key = ?2
                     ORDER BY plugin_version",
                    )?;
                    let rows = stmt
                        .query_map(database::rusqlite::params![plugin_id, path_b], |row| {
                            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
                        })?
                        .collect::<Result<Vec<_>, _>>()?;
                    Ok(rows)
                },
            )
            .unwrap();
        assert_eq!(
            versioned_b,
            vec![
                ("1.0.0".to_string(), yaml_b_v1, 0),
                ("2.0.0".to_string(), yaml_b_v2.clone(), 0),
            ],
            "new pinned versions retain changed bodies without overwriting prior rows"
        );

        let manifest_v3 = nuclei_test_manifest("3.0.0", &"3".repeat(40), None);
        let package_v3 = root.join("package-v3");
        write_nuclei_test_package(
            &package_v3,
            &templates,
            &[(path_a, &yaml_a), (path_b, &yaml_b_v2)],
        );
        activate_nuclei_test_version(&pool, &manifest_v3);
        ingest_knowledge_source_with_template_root(
            &pool,
            &manifest_v3,
            &package_v3,
            &IngestOptions::default(),
            &templates,
        )
        .unwrap();
        let restored_missing: i64 = pool
            .with_conn(|c| {
                c.query_row(
                    "SELECT upstream_missing FROM knowledge_source_entry
                     WHERE plugin_id = ?1 AND plugin_version = '1.0.0' AND item_key = ?2",
                    database::rusqlite::params![plugin_id, path_a],
                    |row| row.get(0),
                )
            })
            .unwrap();
        assert_eq!(
            restored_missing, 0,
            "reappearing templates clear the removal flag"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn truncated_nuclei_snapshot_does_not_mark_omitted_rows_missing() {
        let root =
            std::env::temp_dir().join(format!("rip-nuclei-truncated-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let templates = root.join("templates");
        let pool = database::Pool::open(database::PoolConfig {
            path: database::DbPath::Memory,
            readonly: false,
            max_connections: 1,
            busy_timeout: std::time::Duration::from_secs(2),
        })
        .unwrap();
        let migrations = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations");
        database::migrate(&pool, &migrations).unwrap();
        let plugin_id = "knowledge.nuclei-lifecycle";
        let path_a = "http/test/template-a.yaml";
        let path_b = "http/test/template-b.yaml";
        let yaml_a = nuclei_test_yaml("template-a", "Template A", "Version one");
        let yaml_b = nuclei_test_yaml("template-b", "Template B", "Version one");
        let manifest_v1 = nuclei_test_manifest("1.0.0", &"1".repeat(40), None);
        let package_v1 = root.join("package-v1");
        write_nuclei_test_package(
            &package_v1,
            &templates,
            &[(path_a, &yaml_a), (path_b, &yaml_b)],
        );
        activate_nuclei_test_version(&pool, &manifest_v1);
        ingest_knowledge_source_with_template_root(
            &pool,
            &manifest_v1,
            &package_v1,
            &IngestOptions::default(),
            &templates,
        )
        .unwrap();

        let manifest_v2 = nuclei_test_manifest("2.0.0", &"2".repeat(40), Some(1));
        let package_v2 = root.join("package-v2");
        write_nuclei_test_package(
            &package_v2,
            &templates,
            &[(path_a, &yaml_a), (path_b, &yaml_b)],
        );
        activate_nuclei_test_version(&pool, &manifest_v2);
        let report = ingest_knowledge_source_with_template_root(
            &pool,
            &manifest_v2,
            &package_v2,
            &IngestOptions::default(),
            &templates,
        )
        .unwrap();
        assert_eq!(
            report.filtered.get("nuclei_template:max_entries_truncated"),
            Some(&1)
        );
        let missing: i64 = pool
            .with_conn(|c| {
                c.query_row(
                    "SELECT upstream_missing FROM knowledge_source_entry
                     WHERE plugin_id = ?1 AND plugin_version = '1.0.0' AND item_key = ?2",
                    database::rusqlite::params![plugin_id, path_b],
                    |row| row.get(0),
                )
            })
            .unwrap();
        assert_eq!(
            missing, 0,
            "truncated input is not evidence of upstream removal"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn nuclei_template_metadata_extracts_searchable_fields_without_claiming_verification() {
        let yaml = r#"id: cve-2025-12345
info:
  name: Example Product SQL Injection
  author:
    - alice
    - bob
  severity: critical
  description: |
    A SQL injection test template.
  reference:
    - https://example.com/advisory
    - https://nvd.nist.gov/vuln/detail/CVE-2025-12345
  tags: cve,sqli,example
  classification:
    cve-id: CVE-2025-12345
    cwe-id: CWE-89
  metadata:
    max-request: 2
http:
  - method: GET
    path:
      - "{{BaseURL}}/item?id=1"
    matchers:
      - type: word
        words: ["sql error"]
    extractors:
      - type: regex
        regex: ["version"]
"#;
        let meta = nuclei_template_metadata(yaml);
        assert_eq!(meta["parse_status"], "parsed");
        assert_eq!(meta["template_id"], "cve-2025-12345");
        assert_eq!(meta["name"], "Example Product SQL Injection");
        assert_eq!(meta["severity"], "critical");
        assert_eq!(meta["author"], json!(["alice", "bob"]));
        assert_eq!(meta["tags"], json!(["cve", "example", "sqli"]));
        assert_eq!(meta["references"].as_array().unwrap().len(), 2);
        assert_eq!(meta["classification"]["cve-id"], "CVE-2025-12345");
        assert_eq!(meta["classification"]["cwe-id"], "CWE-89");
        assert_eq!(meta["protocols"], json!(["http"]));
        assert_eq!(meta["request_block_count"], 1);
        assert_eq!(meta["matcher_count"], 1);
        assert_eq!(meta["extractor_count"], 1);
        assert_eq!(
            meta["matchers"],
            json!([{"type": "word", "words": ["sql error"]}])
        );
        assert_eq!(
            meta["extractors"],
            json!([{"type": "regex", "regex": ["version"]}])
        );
        assert_eq!(meta["yaml_syntax_status"], "valid");
        assert_eq!(meta["template_shape_status"], "metadata_projected");
        assert_eq!(meta["detection_validation_status"], "NOT_TESTED");
        assert_eq!(meta["has_flow"], false);
        assert!(
            meta.get("verified").is_none(),
            "Nuclei metadata must not be interpreted as RIP verification"
        );
    }

    #[test]
    fn nuclei_template_metadata_hashes_exact_yaml_bytes_and_separates_validation_states() {
        let yaml = "id: t\ninfo:\n  name: T\nhttp:\n  - matchers:\n      - type: word\n        words: [ok]\n    extractors:\n      - type: regex\n        regex: [v1]\n";
        let meta = nuclei_template_metadata(yaml);

        assert_eq!(
            meta["template_content_sha256"],
            "92a845058e5657343ad6330f80b58ff086d7f1d364025568bd6a10ae9bcb5ea7"
        );
        assert_eq!(meta["detection_validation_status"], "NOT_TESTED");
        assert_eq!(meta["yaml_syntax_status"], "valid");
        assert_eq!(meta["template_shape_status"], "metadata_projected");
        assert_eq!(meta["matchers"], json!([{"type": "word", "words": ["ok"]}]));
        assert_eq!(
            meta["extractors"],
            json!([{"type": "regex", "regex": ["v1"]}])
        );

        let invalid_yaml = nuclei_template_metadata("id: [broken\n");
        assert_eq!(invalid_yaml["yaml_syntax_status"], "invalid");
        assert_eq!(invalid_yaml["detection_validation_status"], "NOT_TESTED");
        let missing_info = nuclei_template_metadata("id: valid\ninfo: null\n");
        assert_eq!(missing_info["yaml_syntax_status"], "valid");
        assert_eq!(missing_info["template_shape_status"], "missing_info");
        assert_eq!(missing_info["detection_validation_status"], "NOT_TESTED");
    }

    #[test]
    fn nuclei_template_metadata_marks_invalid_yaml_and_missing_info() {
        assert_eq!(
            nuclei_template_metadata("id: [broken\n")["parse_status"],
            "invalid_yaml"
        );
        assert_eq!(
            nuclei_template_metadata("id: valid\ninfo: null\n")["parse_status"],
            "missing_info"
        );
    }

    #[test]
    fn nuclei_template_metadata_parses_installed_official_template_when_available() {
        let path = Path::new(NUCLEI_TEMPLATE_SOURCE_ROOT)
            .join("cloud/alibaba/ack/ack-cluster-api-public.yaml");
        let Ok(body) = std::fs::read_to_string(path) else {
            return;
        };
        let meta = nuclei_template_metadata(&body);
        assert_eq!(meta["parse_status"], "parsed");
        assert_eq!(meta["template_id"], "ack-cluster-api-public");
        assert_eq!(meta["severity"], "high");
        assert!(meta["protocols"]
            .as_array()
            .is_some_and(|items| !items.is_empty()));
    }

    #[test]
    #[ignore = "manual full-corpus audit; requires /usr/share/nuclei-templates"]
    fn audit_installed_nuclei_template_corpus() {
        fn collect_yaml_files(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
            let Ok(entries) = std::fs::read_dir(dir) else {
                return;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                let Ok(kind) = entry.file_type() else {
                    continue;
                };
                if kind.is_dir() {
                    collect_yaml_files(&path, out);
                } else if kind.is_file()
                    && matches!(
                        path.extension().and_then(|ext| ext.to_str()),
                        Some("yaml" | "yml")
                    )
                {
                    out.push(path);
                }
            }
        }

        let root = Path::new(NUCLEI_TEMPLATE_SOURCE_ROOT);
        if !root.is_dir() {
            return;
        }
        let mut files = Vec::new();
        collect_yaml_files(root, &mut files);
        files.sort();
        let mut statuses = BTreeMap::<String, usize>::new();
        let mut oversized = 0usize;
        let mut unreadable = 0usize;
        for path in &files {
            let Ok(metadata) = path.metadata() else {
                unreadable += 1;
                continue;
            };
            if metadata.len() > MAX_NUCLEI_TEMPLATE_BYTES {
                oversized += 1;
                println!("oversized: {}", path.display());
                continue;
            }
            let Ok(body) = std::fs::read_to_string(path) else {
                unreadable += 1;
                continue;
            };
            let status = nuclei_template_metadata(&body);
            let label = status["parse_status"]
                .as_str()
                .unwrap_or("missing_status")
                .to_string();
            if label != "parsed" && label != "not_template" {
                println!("{label}: {}", path.display());
            }
            *statuses.entry(label).or_default() += 1;
        }
        println!(
            "Nuclei YAML audit: total={} statuses={:?} oversized={} unreadable={}",
            files.len(),
            statuses,
            oversized,
            unreadable,
        );
        assert!(
            statuses.get("parsed").copied().unwrap_or_default() > 0,
            "no installed Nuclei templates parsed"
        );
    }

    #[test]
    fn nuclei_template_reader_is_rooted_and_size_limited() {
        let root = std::env::temp_dir().join(format!("rip-nuclei-root-{}", std::process::id()));
        let outside =
            std::env::temp_dir().join(format!("rip-nuclei-outside-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let _ = std::fs::remove_file(&outside);
        std::fs::create_dir_all(root.join("cloud/test")).unwrap();
        std::fs::write(root.join("cloud/test/safe.yaml"), "id: safe\ninfo: {}\n").unwrap();
        std::fs::write(&outside, "id: outside\n").unwrap();
        assert_eq!(
            read_nuclei_template_body_from_root(
                &root,
                "/usr/share/nuclei-templates/cloud/test/safe.yaml"
            )
            .as_deref(),
            Some("id: safe\ninfo: {}\n")
        );
        assert!(read_nuclei_template_body_from_root(
            &root,
            "/usr/share/nuclei-templates/../outside.yaml"
        )
        .is_none());
        assert!(read_nuclei_template_body_from_root(&root, "/etc/passwd").is_none());
        assert!(read_nuclei_template_body_from_root(
            &root,
            "/usr/share/nuclei-templates/cloud/test/safe.txt"
        )
        .is_none());
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&outside, root.join("cloud/test/escape.yaml")).unwrap();
            assert!(read_nuclei_template_body_from_root(
                &root,
                "/usr/share/nuclei-templates/cloud/test/escape.yaml"
            )
            .is_none());
        }
        std::fs::write(
            root.join("cloud/test/oversized.yaml"),
            vec![b'x'; (MAX_NUCLEI_TEMPLATE_BYTES + 1) as usize],
        )
        .unwrap();
        assert!(read_nuclei_template_body_from_root(
            &root,
            "/usr/share/nuclei-templates/cloud/test/oversized.yaml"
        )
        .is_none());
        let _ = std::fs::remove_dir_all(&root);
        let _ = std::fs::remove_file(&outside);
    }

    #[test]
    fn curation_overlay_redacts_sensitive_verified_note_only_in_projection() {
        let mut meta = json!({
            "verified_note": "password=do-not-index-this",
            "description": "safe summary",
        });
        let curated = json!({
            "redact_verified_note": true,
            "redacted_verified_note": "[REDACTED: credential-bearing source note]",
        });
        apply_curation_metadata(&mut meta, &curated);
        let serialized = serde_json::to_string(&meta).unwrap();
        assert!(!serialized.contains("do-not-index-this"));
        assert!(serialized.contains("credential-bearing source note"));
        assert_eq!(meta["description"], "safe summary");
    }

    #[test]
    fn curation_overlay_projects_searchable_evidence_facets() {
        let mut meta = json!({
            "code": "high-poc-example",
            "description": "safe summary",
        });
        let curated = json!({
            "evidence_level": "K2",
            "lifecycle": "Researched",
            "dedup_status": "independent",
            "source_integrity": "partial",
            "verification_result": "historical_evidence_supported",
            "independent_knowledge_status": "independent",
            "missing_evidence": [],
            "source_commit": null,
            "evidence_refs": ["../runs/example/response.hdr"],
        });
        apply_curation_metadata(&mut meta, &curated);
        assert_eq!(meta["evidence_level"], "K2");
        assert_eq!(meta["lifecycle"], "Researched");
        assert_eq!(meta["dedup_status"], "independent");
        assert_eq!(meta["source_integrity"], "partial");
        assert_eq!(meta["verification_result"], "historical_evidence_supported");
        assert_eq!(meta["independent_knowledge_status"], "independent");
        assert_eq!(meta["curation"]["evidence_level"], "K2");
    }

    #[test]
    fn cisa_kev_parser_keeps_authoritative_metadata_without_claiming_poc_or_cvss() {
        let dir = std::env::temp_dir().join(format!("cisa-kev-parser-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let feed = json!({
            "title": "CISA Catalog of Known Exploited Vulnerabilities",
            "catalogVersion": "2026.11.25",
            "dateReleased": "2026-11-25T22:59:40.0000Z",
            "count": 1,
            "vulnerabilities": [{
                "cveID": "CVE-2026-109618",
                "vendorProject": "PHP Group",
                "product": "PHP",
                "vulnerabilityName": "PHP Use After Free",
                "dateAdded": "2026-10-26",
                "shortDescription": "Use after free can lead to remote code execution.",
                "requiredAction": "Apply the vendor security update.",
                "dueDate": "2026-11-16",
                "knownRansomwareCampaignUse": "Known",
                "forensicTriage": "Yes",
                "notes": "Synthetic fixture only.",
                "cwes": ["CWE-416"]
            }]
        });
        std::fs::write(dir.join("kev.json"), serde_json::to_vec(&feed).unwrap()).unwrap();
        let cfg = KindConfig {
            source_path: "kev.json".to_string(),
            parser: "cisa_kev_json".to_string(),
            license_policy: "metadata_only".to_string(),
            max_entries: Some(100),
            source_commit: Some("51e0a777743fa06957afbaeb280f8735d292c1be".to_string()),
        };
        let snap = SnapshotCtx {
            license_status: "CC0-1.0".to_string(),
            source_type: "local_dir".to_string(),
            source_url: "target/cisa-kev-package".to_string(),
            source_commit: None,
        };
        let (items, stats) = parse_catalog_entry(&cfg, &dir, &snap).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].kind, "catalog_entry");
        assert_eq!(items[0].item_key, "CVE-2026-109618");
        assert_eq!(items[0].license_status, "CC0-1.0");
        assert_eq!(items[0].rights_policy, "metadata_only");
        assert_eq!(
            items[0].source_commit.as_deref(),
            Some("51e0a777743fa06957afbaeb280f8735d292c1be")
        );
        assert_eq!(
            items[0].meta.as_ref().unwrap()["cve_record"]["known_exploited"],
            true
        );
        assert_eq!(
            items[0].meta.as_ref().unwrap()["cve_record"]["known_ransomware_campaign_use"],
            "Known"
        );
        assert_eq!(
            items[0].meta.as_ref().unwrap()["cve_record"]["cvss_score"],
            Value::Null
        );
        assert_eq!(
            items[0].meta.as_ref().unwrap()["cve_record"]["epss_score"],
            Value::Null
        );
        assert_eq!(
            items[0].meta.as_ref().unwrap()["cve_record"]["poc_status"],
            "not_assessed"
        );
        assert_eq!(
            items[0].meta.as_ref().unwrap()["cve_record"]["current_target_status"],
            "not_tested"
        );
        assert!(items[0].body.contains("CVSS: not supplied by the KEV feed"));
        assert!(items[0].body.contains("PoC status: not assessed"));
        assert_eq!(stats, kp::FilterStats::default());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn pocindex_kev_parser_preserves_epss_cvss_and_source_provenance() {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("pocindex-{}-{nonce}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let commit = "e9076fea96095e014fc107c02dd23ee8dc21476d";
        let files = json!({
            "kev.json": {"url":"https://pocindex.io/kev.json","etag":"\"kev\"","last_modified":"2026-11-26T06:41:10Z","sha256":"a".repeat(64),"bytes":100},
            "epss.json": {"url":"https://pocindex.io/epss.json","etag":"\"epss\"","last_modified":"2026-11-26T06:41:10Z","sha256":"b".repeat(64),"bytes":100},
            "cve_metadata.json": {"url":"https://pocindex.io/cve_metadata.json","etag":"\"cvss\"","last_modified":"2026-11-26T06:41:10Z","sha256":"c".repeat(64),"bytes":100},
            "nuclei.json": {"url":"https://pocindex.io/nuclei.json","etag":"\"nuclei\"","last_modified":"2026-11-26T06:41:10Z","sha256":"d".repeat(64),"bytes":100}
        });
        let payload = json!({
            "schema":"pocindex-kev-enrichment-v1",
            "count":1,
            "snapshot":{
                "repository":"https://github.com/0xMarcio/pocindex",
                "repository_commit":commit,
                "cdn":"https://pocindex.io",
                "retrieved_at":"2026-11-26T06:41:10Z",
                "cisa_catalog_version":"2026.11.25",
                "files":files
            },
            "records":[{
                "cve_id":"CVE-2099-12345",
                "cisa_date_added":"2099-01-01",
                "pocindex_kev_present":true,
                "pocindex_kev_value":["2099-01-01",0],
                "epss_score":0.9,
                "epss_percentile":0.99,
                "cvss_assessments":[{"version":"3.1","score":9.8,"severity":"CRITICAL","vector":"CVSS:3.1/AV:N","source":"nvd@nist.gov","assessment_type":"Primary"}],
                "advisories":[{"url":"https://example.com/advisory","tags":["Vendor Advisory"]}],
                "nuclei_metadata":{"severity":"critical","cvss":9.8,"cvss_vector":"CVSS:3.1/AV:N","cwe":"CWE-78"}
            }]
        });
        std::fs::write(
            dir.join("pocindex-kev.json"),
            serde_json::to_vec(&payload).unwrap(),
        )
        .unwrap();
        let cfg = KindConfig {
            source_path: "pocindex-kev.json".to_string(),
            parser: "pocindex_kev_json".to_string(),
            license_policy: "metadata_only".to_string(),
            max_entries: Some(2),
            source_commit: Some(commit.to_string()),
        };
        let snap = SnapshotCtx {
            source_type: "local_dir".to_string(),
            source_url: "target/pocindex-kev-package".to_string(),
            source_commit: None,
            license_status: "MIT".to_string(),
        };
        let (items, _) = parse_catalog_entry(&cfg, &dir, &snap).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].item_key, "CVE-2099-12345");
        assert_eq!(items[0].source_type, "http_snapshot");
        assert_eq!(items[0].source_url, "https://pocindex.io");
        assert_eq!(items[0].source_commit.as_deref(), Some(commit));
        let meta = items[0].meta.as_ref().unwrap();
        assert_eq!(meta["pocindex_record"]["epss_score"], 0.9);
        assert_eq!(
            meta["pocindex_record"]["cvss_assessments"][0]["source"],
            "nvd@nist.gov"
        );
        assert_eq!(meta["pocindex_record"]["nuclei_metadata"]["cwe"], "CWE-78");
        assert!(items[0].body.contains("EPSS score: 0.9"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn pocindex_kev_parser_rejects_invalid_epss_and_duplicate_cves() {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir =
            std::env::temp_dir().join(format!("pocindex-invalid-{}-{nonce}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let commit = "e9076fea96095e014fc107c02dd23ee8dc21476d";
        let files = json!({
            "kev.json":{"url":"https://pocindex.io/kev.json","sha256":"a".repeat(64),"bytes":1},
            "epss.json":{"url":"https://pocindex.io/epss.json","sha256":"b".repeat(64),"bytes":1},
            "cve_metadata.json":{"url":"https://pocindex.io/cve_metadata.json","sha256":"c".repeat(64),"bytes":1},
            "nuclei.json":{"url":"https://pocindex.io/nuclei.json","sha256":"d".repeat(64),"bytes":1}
        });
        let record = json!({
            "cve_id":"CVE-2099-12345","cisa_date_added":"2099-01-01",
            "pocindex_kev_present":false,"pocindex_kev_value":null,
            "epss_score":1.1,"epss_percentile":0.9,
            "cvss_assessments":[],"advisories":[],"nuclei_metadata":null
        });
        let payload = json!({"schema":"pocindex-kev-enrichment-v1","count":1,
            "snapshot":{"repository":"https://github.com/0xMarcio/pocindex","repository_commit":commit,"cdn":"https://pocindex.io","retrieved_at":"2026-11-26T06:41:10Z","files":files},
            "records":[record]});
        std::fs::write(
            dir.join("pocindex-kev.json"),
            serde_json::to_vec(&payload).unwrap(),
        )
        .unwrap();
        let cfg = KindConfig {
            source_path: "pocindex-kev.json".to_string(),
            parser: "pocindex_kev_json".to_string(),
            license_policy: "metadata_only".to_string(),
            max_entries: Some(2),
            source_commit: Some(commit.to_string()),
        };
        let snap = SnapshotCtx {
            license_status: "MIT".to_string(),
            source_type: "local_dir".to_string(),
            source_url: "target/pocindex-kev-package".to_string(),
            source_commit: None,
        };
        assert!(parse_catalog_entry(&cfg, &dir, &snap).is_err());
        let mut duplicate = payload.clone();
        let mut duplicate_record = payload["records"][0].clone();
        duplicate_record["epss_score"] = json!(0.9);
        duplicate_record["epss_percentile"] = json!(0.9);
        duplicate["records"] = json!([duplicate_record.clone(), duplicate_record]);
        duplicate["count"] = json!(2);
        std::fs::write(
            dir.join("pocindex-kev.json"),
            serde_json::to_vec(&duplicate).unwrap(),
        )
        .unwrap();
        assert!(parse_catalog_entry(&cfg, &dir, &snap).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn cisa_kev_parser_rejects_count_mismatch_and_invalid_cve_ids() {
        let dir = std::env::temp_dir().join(format!("cisa-kev-invalid-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let cfg = KindConfig {
            source_path: "kev.json".to_string(),
            parser: "cisa_kev_json".to_string(),
            license_policy: "metadata_only".to_string(),
            max_entries: Some(100),
            source_commit: None,
        };
        let snap = SnapshotCtx {
            license_status: "CC0-1.0".to_string(),
            source_type: "git".to_string(),
            source_url: "https://github.com/cisagov/kev-data".to_string(),
            source_commit: None,
        };
        let mut feed = json!({
            "catalogVersion": "2026.11.25",
            "dateReleased": "2026-11-25T22:59:40Z",
            "count": 2,
            "vulnerabilities": []
        });
        std::fs::write(dir.join("kev.json"), serde_json::to_vec(&feed).unwrap()).unwrap();
        assert!(parse_catalog_entry(&cfg, &dir, &snap).is_err());
        feed["count"] = json!(1);
        feed["vulnerabilities"] = json!([{
            "cveID": "NOT-A-CVE", "vendorProject": "Vendor", "product": "Product",
            "dateAdded": "2026-11-01", "knownRansomwareCampaignUse": "Unknown"
        }]);
        std::fs::write(dir.join("kev.json"), serde_json::to_vec(&feed).unwrap()).unwrap();
        assert!(parse_catalog_entry(&cfg, &dir, &snap).is_err());
        feed["vulnerabilities"] = json!([{
            "cveID": "CVE-2099-12345", "vendorProject": "Vendor", "product": "Product",
            "vulnerabilityName": "Synthetic", "dateAdded": "2099-01-01",
            "shortDescription": "Synthetic fixture", "requiredAction": "Patch it",
            "dueDate": "2099-02-01", "knownRansomwareCampaignUse": "Unknown",
            "cwes": [416]
        }]);
        std::fs::write(dir.join("kev.json"), serde_json::to_vec(&feed).unwrap()).unwrap();
        assert!(parse_catalog_entry(&cfg, &dir, &snap).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn permission_gate_values() {
        assert_eq!(INGEST_PERMISSION_REQUIRED, "ingest_via_api");
        for k in CONTENT_KINDS {
            assert!(RTIS_CONTENT_KINDS.contains(&k) || EXTENDED_CONTENT_KINDS.contains(&k));
        }
        // RTIS / SK-1 词表互补不相交（ADR 第四节）。
        for k in RTIS_CONTENT_KINDS {
            assert!(!EXTENDED_CONTENT_KINDS.contains(&k));
        }
        // parser 配对表覆盖全部扩展类型。
        for k in EXTENDED_CONTENT_KINDS {
            assert!(KIND_PARSER.iter().any(|(kind, _)| *kind == k));
        }
    }
}
