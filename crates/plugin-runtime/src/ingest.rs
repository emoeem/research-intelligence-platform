//! Knowledge 插件摄取管道（PLUGIN-CONTRACT §2 原则 2 第一红线 + §3.2；
//! ROADMAP P6-5）。
//!
//! 红线语义落进代码：
//! - 权限闸：Manifest `permissions.data_access` 必须为
//!   [`INGEST_PERMISSION_REQUIRED`]（ingest_via_api），其余取值（read_only/
//!   none/缺省）一律拒绝，插件不得直写主库表，知识入主库只能经本管道；
//! - 溯源三元组：每行携带 plugin_id + plugin_version + corpus_id
//!   （knowledge_ingest 表，010 迁移；卸载插件不删已入主库知识，P6-2 §5.4）；
//! - 幂等：UNIQUE(plugin_id, plugin_version, content_kind, content_sha256)
//!   + INSERT OR IGNORE，重跑摄取对内容不变行 no-op（M6 ETL Replay 口径）。
//!
//! 语料来源（包内，只读）：
//! - `knowledge.db`（RTIS 主知识库，SQLite）：poc 表逐行 → `poc`；以
//!   `mode=ro` URI 只读打开，生产文件零改动（P6-5 红线核查项）；
//! - `skills/*.md` → `skill_md`（125 个技能文档，文件全文）；
//! - `pocs/` 递归全部文件 → `method_case`（编排案例正文）。
//!
//! RTIS knowledge.db 条数口径（2026-11-11 实查收敛）：poc 表 14717 行，
//! rtis.py 头注「14664 条」为 2026-11-02 陈旧快照，README 的 14717 为当前
//! 实数（详见 docs/phase6/RTIS-REDTEAM-PLUGIN-NOTES.md）。

use std::collections::BTreeMap;
use std::path::Path;

use serde::Serialize;
use serde_json::{json, Value};

use crate::RuntimeError;
use database::Pool;
use domain::time::now_iso;
use plugin_registry::hash::sha256_hex;

/// 摄取权限闸要求的唯一合法取值（PLUGIN-CONTRACT §2 原则 2）。
pub const INGEST_PERMISSION_REQUIRED: &str = "ingest_via_api";

/// 三类语料词表（v1.json capability content_kinds）。
pub const CONTENT_KINDS: [&str; 3] = ["poc", "skill_md", "method_case"];

/// 语料条目中间形态：kind / title / source_pk / body / body_bytes / meta。
pub type CorpusItem = (String, String, Option<String>, String, usize, Option<Value>);

/// 摄取报告（机器可读；CLI 与审计 detail 复用）。
#[derive(Debug, Clone, Serialize)]
pub struct IngestReport {
    /// 来源插件 id。
    pub plugin_id: String,
    /// 来源插件版本。
    pub plugin_version: String,
    /// 语料标识。
    pub corpus_id: String,
    /// 各类新插入条数。
    pub counts: BTreeMap<String, u64>,
    /// 幂等跳过条数（内容不变行）。
    pub skipped: u64,
    /// 新插入正文总字节。
    pub total_bytes: u64,
    /// 各类跳过明细（幂等复核用）。
    pub skipped_by_kind: BTreeMap<String, u64>,
}

/// 摄取一个 Knowledge 插件包（knowledge_source capability）到数据面。
pub fn ingest_knowledge_source(
    pool: &Pool,
    manifest: &Value,
    package_dir: &Path,
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

    // 4) 幂等写入（单事务；UNIQUE 冲突即 skipped）。
    let mut counts: BTreeMap<String, u64> = BTreeMap::new();
    let mut skipped_by_kind: BTreeMap<String, u64> = BTreeMap::new();
    let mut skipped = 0u64;
    let mut total_bytes = 0u64;
    let now = now_iso();
    pool.with_tx(|tx| -> Result<(), database::rusqlite::Error> {
        for (kind, title, source_pk, body, body_bytes, meta) in &items {
            let sha = sha256_hex(body.as_bytes());
            let mut stmt = tx.prepare_cached(
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
        Ok(())
    })
    .map_err(RuntimeError::from)?;

    Ok(IngestReport {
        plugin_id,
        plugin_version,
        corpus_id,
        counts,
        skipped,
        total_bytes,
        skipped_by_kind,
    })
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
    })
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
        assert!(CONTENT_KINDS.contains(&"poc"));
        assert!(CONTENT_KINDS.contains(&"skill_md"));
        assert!(CONTENT_KINDS.contains(&"method_case"));
    }
}
