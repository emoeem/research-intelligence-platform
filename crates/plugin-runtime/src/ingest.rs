//! Knowledge 插件摄取管道（PLUGIN-CONTRACT §2 原则 2 第一红线 + §3.2；
//! ROADMAP P6-5 + P6-9/SK-1 通用化，裁决 docs/phase6/SK1-INGEST-ADR.md）。
//!
//! 红线语义落进代码：
//! - 权限闸：Manifest `permissions.data_access` 必须为
//!   [`INGEST_PERMISSION_REQUIRED`]（ingest_via_api），其余取值（read_only/
//!   none/缺省）一律拒绝，插件不得直写主库表，知识入主库只能经本管道；
//! - 溯源三元组：每行携带 plugin_id + plugin_version + corpus_id
//!   （RTIS 三类 → knowledge_ingest 表，010 迁移；SK-1 三类 →
//!   knowledge_source_entry 表，011 迁移；卸载插件不删已入主库知识）；
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
//!   [`crate::knowledge_parse`] 纯文本实现（不执行 Markdown/HTML 任何代码、
//!   不访问链接，scheme 白名单 http/https），写 knowledge_source_entry，
//!   初始 review_status = DISCOVERED（检索/上下文面只收 APPROVED，
//!   翻转走 [`crate::knowledge_review`] 受控写面）。
//!
//! RTIS knowledge.db 条数口径（2026-11-11 实查收敛）：poc 表 14717 行，
//! rtis.py 头注「14664 条」为 2026-11-02 陈旧快照，README 的 14717 为当前
//! 实数（详见 docs/phase6/RTIS-REDTEAM-PLUGIN-NOTES.md）。

use std::collections::BTreeMap;
use std::path::Path;

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

/// 六类语料词表（v1.json capability content_kinds；010 三类 + SK-1 三类）。
pub const CONTENT_KINDS: [&str; 6] = [
    "poc",
    "skill_md",
    "method_case",
    "catalog_entry",
    "document_markdown",
    "skill_candidate",
];

/// RTIS 既有三类（knowledge_ingest 权威落点，010；本管道对这三类的解析与
/// 写面行为与 P6-5 逐字节一致，计数零回归）。
pub const RTIS_CONTENT_KINDS: [&str; 3] = ["poc", "skill_md", "method_case"];

/// SK-1 三类（knowledge_source_entry 权威落点，011）。
pub const EXTENDED_CONTENT_KINDS: [&str; 3] =
    ["catalog_entry", "document_markdown", "skill_candidate"];

/// parser 注册表键与内容类型的配对（validate rule 3 同款，双处防御）。
pub const KIND_PARSER: [(&str, &str); 3] = [
    ("catalog_entry", "markdown_links"),
    ("document_markdown", "markdown_doc"),
    ("skill_candidate", "skill_frontmatter"),
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

/// SK-1 快照上下文（三类共享的溯源列族值；Snapshot 为 ADR 3.1 列族形态）。
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

/// SK-1 三类条目中间形态（011 knowledge_source_entry 行前身）。
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
    /// 各类新插入条数（RTIS 与 SK-1 三类合并口径）。
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

    // SK-1 三类：per-kind parser 注册表（写 knowledge_source_entry）。
    let extended_declared: Vec<String> = declared_kinds
        .iter()
        .filter(|k| EXTENDED_CONTENT_KINDS.contains(&k.as_str()))
        .cloned()
        .collect();
    let mut extended_items: Vec<ExtendedItem> = Vec::new();
    let mut filtered: BTreeMap<String, u64> = BTreeMap::new();
    for kind in &extended_declared {
        let cfg = kind_config(obj, kind)?;
        let parser = KIND_PARSER
            .iter()
            .find(|(k, _)| *k == kind.as_str())
            .map(|(_, p)| *p)
            .unwrap_or_default();
        if cfg.parser != parser {
            return Err(RuntimeError::new(
                "ingest_kind_parser_mismatch",
                format!(
                    "content_kind {kind:?} requires parser {parser:?}, declared {:?}",
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
            other => {
                return Err(RuntimeError::new(
                    "ingest_kind_unknown",
                    format!("no parser registered for content_kind {other:?}"),
                ))
            }
        };
        // max_entries 截断（计划 §6.2A 超量条目截断报告）。
        if let Some(cap) = cfg.max_entries {
            if items.len() as u64 > cap {
                let dropped = items.len() as u64 - cap;
                *filtered
                    .entry(format!("{kind}:max_entries_truncated"))
                    .or_insert(0) += dropped;
                items.truncate(cap as usize);
            }
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
            let outcome = write_all(
                c,
                &plugin_id,
                &plugin_version,
                &corpus_id,
                &rtis_items,
                &extended_items,
                &now,
            );
            // 无论写步成败都回滚（dry-run 零落盘）。
            let _ = c.execute_batch(
                "ROLLBACK TO SAVEPOINT ingest_dry_run; RELEASE SAVEPOINT ingest_dry_run;",
            );
            outcome
        })
        .map_err(RuntimeError::from)?
    } else {
        pool.with_tx(|tx| {
            write_all(
                tx,
                &plugin_id,
                &plugin_version,
                &corpus_id,
                &rtis_items,
                &extended_items,
                &now,
            )
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

    // 2) SK-1 三类 → knowledge_source_entry（011；去重键 + 内容变更侦测）。
    for item in extended_items {
        let sha = sha256_hex(item.body.as_bytes());
        let cat_json = serde_json::to_string(&item.category_path)
            .map_err(|e| database::rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
        let loc_json = item.source_location.to_string();
        let meta_json = item.meta.as_ref().map(|m| m.to_string());
        let metadata_material = format!(
            "{}\n{}\n{}\n{}",
            item.title,
            cat_json,
            loc_json,
            item.description.clone().unwrap_or_default()
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
            let existing: Option<String> = c
                .query_row(
                    "SELECT content_sha256 FROM knowledge_source_entry
                     WHERE plugin_id = ?1 AND plugin_version = ?2 AND entry_kind = ?3 AND item_key = ?4",
                    database::rusqlite::params![plugin_id, plugin_version, item.kind, item.item_key],
                    |r| r.get(0),
                )
                .map(Some)
                .or_else(|e| match e {
                    database::rusqlite::Error::QueryReturnedNoRows => Ok(None),
                    other => Err(other),
                })?;
            if existing.as_deref() != Some(sha.as_str()) {
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
        let mut stmt = conn
            .prepare(
                "SELECT id, code, title, content, usage, description, kind, category, cve,
                        component, versions, severity, language, tags, verified
                 FROM poc ORDER BY id",
            )
            .map_err(|e| {
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
            ) = row.map_err(|e| {
                RuntimeError::new("ingest_source_read_failed", format!("poc row: {e}"))
            })?;
            let body = content
                .filter(|s| !s.trim().is_empty())
                .or(usage)
                .or(description)
                .unwrap_or_default();
            let meta = json!({
                "code": code, "kind": kind, "category": category, "cve": cve,
                "component": component, "versions": versions, "severity": severity,
                "language": language, "tags": tags, "verified": verified != 0,
            });
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
            let base = pocs.to_string_lossy().to_string();
            for path in files {
                let rel = path
                    .strip_prefix(&base)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .trim_start_matches('/')
                    .to_string();
                let title = rel.clone();
                let body = std::fs::read(&path)
                    .map_err(|e| RuntimeError::new("io", format!("read {path:?}: {e}")))?;
                let body = String::from_utf8_lossy(&body).to_string();
                let body_len = body.len();
                items.push(("method_case".to_string(), title, None, body, body_len, None));
            }
        }
    }
    Ok(items)
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
    fn permission_gate_values() {
        assert_eq!(INGEST_PERMISSION_REQUIRED, "ingest_via_api");
        for k in CONTENT_KINDS {
            assert!(RTIS_CONTENT_KINDS.contains(&k) || EXTENDED_CONTENT_KINDS.contains(&k));
        }
        // RTIS / SK-1 词表互补不相交（ADR 第四节）。
        for k in RTIS_CONTENT_KINDS {
            assert!(!EXTENDED_CONTENT_KINDS.contains(&k));
        }
        // parser 配对表覆盖三类。
        for k in EXTENDED_CONTENT_KINDS {
            assert!(KIND_PARSER.iter().any(|(kind, _)| *kind == k));
        }
    }
}
