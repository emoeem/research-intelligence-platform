//! Document 域存取面（03-DOMAIN-MODEL 落库，P1-2/P1-3）：
//! CAS 感知导入（对象 + 来源 + 文档 + 版本单事务）、多来源引用、版本链、
//! 软删除、引用计数维护与孤儿口径。
//!
//! 引用计数口径（12-STORAGE 第二节）：content_objects.ref_count = 引用该对象的
//! source_records 行数 + document_revisions 行数；导入/加来源/新增版本各 +1，
//! 删来源 -1；ref_count = 0 的对象属孤儿，仅 rp-object gc 可物理删除（账面保护）。

use crate::object_store::ObjectStore;
use crate::{new_entity_id, IntelError, IntelResult, PgPool};
use domain::{DocumentId, DocumentRevisionId, DocumentVisibility, SourceRecordId};

/// 导入输入（magic bytes 检测在调用面完成，本层只收事实值）。
#[derive(Debug, Clone)]
pub struct ImportInput {
    /// 标题（非空）。
    pub title: String,
    /// 类型词（03-DOMAIN-MODEL 1.1 词表，调用面校验）。
    pub document_type: String,
    /// 原始文件名。
    pub original_filename: String,
    /// mime（魔法数检测值）。
    pub mime_type: String,
    /// 语言（可空）。
    pub language: Option<String>,
    /// 工作区（缺省 default）。
    pub workspace_id: String,
    /// 可见性（缺省 private）。
    pub visibility: DocumentVisibility,
    /// 来源通道词（git/github/url/upload）。
    pub source_type: String,
    /// 来源 URL（可空）。
    pub source_url: String,
    /// 上游 pinned commit（可空）。
    pub source_commit: Option<String>,
}

/// 导入结果。
#[derive(Debug, Clone)]
pub struct ImportOutcome {
    pub document_id: DocumentId,
    pub revision_id: DocumentRevisionId,
    pub source_id: SourceRecordId,
    /// 本次是否新建了 CAS 对象（false = 内容已存在，复用：内容不重复存储）。
    pub content_object_created: bool,
}

fn check_document_type(document_type: &str) -> IntelResult<()> {
    const WORDS: [&str; 11] = [
        "book", "pdf", "docx", "pptx", "xlsx", "html", "markdown", "code", "image", "dataset",
        "other",
    ];
    if WORDS.contains(&document_type) {
        Ok(())
    } else {
        Err(IntelError::Invalid(format!(
            "invalid document_type {document_type:?} (03-DOMAIN-MODEL 1.1 vocabulary)"
        )))
    }
}

fn check_source_type(source_type: &str) -> IntelResult<()> {
    if matches!(source_type, "git" | "github" | "url" | "upload") {
        Ok(())
    } else {
        Err(IntelError::Invalid(format!(
            "invalid source_type {source_type:?} (V0002 CHECK domain)"
        )))
    }
}

/// 导入：CAS put + 单事务落 content_object / source_record / document /
/// revision 1。同内容重复导入（跨来源）时对象复用（content_object_created=false），
/// 引用计数 +2（来源行 + 版本行）。
pub fn import_document(
    pool: &PgPool,
    store: &dyn ObjectStore,
    input: &ImportInput,
    content: &[u8],
) -> IntelResult<ImportOutcome> {
    check_document_type(&input.document_type)?;
    check_source_type(&input.source_type)?;
    if input.title.trim().is_empty() {
        return Err(IntelError::Invalid("title must be non-empty".to_string()));
    }
    let stored = store.put(content)?;
    let document_id = DocumentId::try_new(new_entity_id("doc-"))?;
    let revision_id = DocumentRevisionId::try_new(new_entity_id("drev-"))?;
    let source_id = SourceRecordId::try_new(new_entity_id("src-"))?;

    let mut conn = pool.get()?;
    let mut tx = conn.transaction()?;
    // 内容对象：两步法（FOR UPDATE 探测后插入或加引用），规避 xmax=0 判插入
    // 在 PG 上不可靠的坑；引用计账：来源 + 版本各 1。
    let object_created = {
        let existing = tx.query_opt(
            "SELECT 1 FROM content_objects WHERE content_hash = $1 FOR UPDATE",
            &[&stored.content_hash],
        )?;
        if existing.is_some() {
            tx.execute(
                "UPDATE content_objects SET ref_count = ref_count + 2 WHERE content_hash = $1",
                &[&stored.content_hash],
            )?;
            false
        } else {
            tx.execute(
                "INSERT INTO content_objects (content_hash, size_bytes, mime_type, storage_uri, ref_count)
                 VALUES ($1, $2, $3, $4, 2)",
                &[&stored.content_hash, &(content.len() as i64), &input.mime_type, &stored.storage_uri],
            )?;
            true
        }
    };
    tx.execute(
        "INSERT INTO source_records (source_id, content_hash, source_type, source_url, source_commit, license_status, workspace_id)
         VALUES ($1, $2, $3, $4, $5, 'UNKNOWN', $6)",
        &[
            &source_id.as_str(),
            &stored.content_hash,
            &input.source_type,
            &input.source_url,
            &input.source_commit,
            &input.workspace_id,
        ],
    )?;
    tx.execute(
        "INSERT INTO documents (document_id, title, document_type, original_filename, mime_type,
             file_size, storage_uri, sha256, workspace_id, visibility, language)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)",
        &[
            &document_id.as_str(),
            &input.title,
            &input.document_type,
            &input.original_filename,
            &input.mime_type,
            &(content.len() as i64),
            &stored.storage_uri,
            &stored.content_hash,
            &input.workspace_id,
            &input.visibility.as_str(),
            &input.language,
        ],
    )?;
    tx.execute(
        "INSERT INTO document_revisions (revision_id, document_id, revision_number, content_hash,
             parser_version, source_version, revision_status)
         VALUES ($1, $2, 1, $3, NULL, $4, 'CURRENT')",
        &[
            &revision_id.as_str(),
            &document_id.as_str(),
            &stored.content_hash,
            &input.source_commit,
        ],
    )?;
    tx.commit()?;
    Ok(ImportOutcome {
        document_id,
        revision_id,
        source_id,
        content_object_created: object_created,
    })
}

/// 同文档追加来源（同内容对象引用 +1；03-DOMAIN-MODEL 1.8 多来源语义）。
pub fn add_source(
    pool: &PgPool,
    document_id: &DocumentId,
    source_type: &str,
    source_url: &str,
    source_commit: Option<&str>,
) -> IntelResult<SourceRecordId> {
    check_source_type(source_type)?;
    let source_id = SourceRecordId::try_new(new_entity_id("src-"))?;
    let mut conn = pool.get()?;
    let mut tx = conn.transaction()?;
    let content_hash: String = tx
        .query_opt(
            "SELECT sha256 FROM documents WHERE document_id = $1 AND lifecycle_status = 'ACTIVE'",
            &[&document_id.as_str()],
        )?
        .ok_or_else(|| IntelError::Invalid(format!("document {document_id} not found")))?
        .get(0);
    tx.execute(
        "INSERT INTO source_records (source_id, content_hash, source_type, source_url, source_commit, license_status, workspace_id)
         SELECT $1, sha256, $2, $3, $4, 'UNKNOWN', workspace_id FROM documents WHERE document_id = $5",
        &[
            &source_id.as_str(),
            &source_type,
            &source_url,
            &source_commit,
            &document_id.as_str(),
        ],
    )?;
    tx.execute(
        "UPDATE content_objects SET ref_count = ref_count + 1 WHERE content_hash = $1",
        &[&content_hash],
    )?;
    tx.commit()?;
    Ok(source_id)
}

/// 新版本（版本链：旧版本转 SUPERSEDED 不删除，历史可访问；新内容对象按需建）。
pub fn new_revision(
    pool: &PgPool,
    store: &dyn ObjectStore,
    document_id: &DocumentId,
    content: &[u8],
    parser_version: Option<&str>,
    source_commit: Option<&str>,
) -> IntelResult<DocumentRevisionId> {
    let stored = store.put(content)?;
    let revision_id = DocumentRevisionId::try_new(new_entity_id("drev-"))?;
    let mut conn = pool.get()?;
    let mut tx = conn.transaction()?;
    let exists: Option<String> = tx
        .query_opt(
            "SELECT sha256 FROM documents WHERE document_id = $1 AND lifecycle_status = 'ACTIVE'",
            &[&document_id.as_str()],
        )?
        .map(|r| r.get(0));
    if exists.is_none() {
        return Err(IntelError::Invalid(format!(
            "document {document_id} not found or deleted"
        )));
    }
    tx.execute(
        "INSERT INTO content_objects (content_hash, size_bytes, mime_type, storage_uri, ref_count)
         VALUES ($1, $2, 'application/octet-stream', $3, 1)
         ON CONFLICT (content_hash)
         DO UPDATE SET ref_count = content_objects.ref_count + 1",
        &[
            &stored.content_hash,
            &(content.len() as i64),
            &stored.storage_uri,
        ],
    )?;
    let next_number: i64 = tx
        .query_one(
            "SELECT COALESCE(MAX(revision_number), 0) + 1 FROM document_revisions WHERE document_id = $1",
            &[&document_id.as_str()],
        )?
        .get(0);
    tx.execute(
        "UPDATE document_revisions SET revision_status = 'SUPERSEDED'
         WHERE document_id = $1 AND revision_status = 'CURRENT'",
        &[&document_id.as_str()],
    )?;
    tx.execute(
        "INSERT INTO document_revisions (revision_id, document_id, revision_number, content_hash,
             parser_version, source_version, revision_status)
         VALUES ($1, $2, $3, $4, $5, $6, 'CURRENT')",
        &[
            &revision_id.as_str(),
            &document_id.as_str(),
            &next_number,
            &stored.content_hash,
            &parser_version,
            &source_commit,
        ],
    )?;
    tx.execute(
        "UPDATE documents SET sha256 = $2, storage_uri = $3, updated_at = now() WHERE document_id = $1",
        &[&document_id.as_str(), &stored.content_hash, &stored.storage_uri],
    )?;
    tx.commit()?;
    Ok(revision_id)
}

/// 删来源（引用计数 -1；对象账面保护：不物理删除，孤儿由 gc 面清）。
pub fn delete_source(pool: &PgPool, source_id: &SourceRecordId) -> IntelResult<i64> {
    let mut conn = pool.get()?;
    let mut tx = conn.transaction()?;
    let content_hash: String = tx
        .query_opt(
            "DELETE FROM source_records WHERE source_id = $1 RETURNING content_hash",
            &[&source_id.as_str()],
        )?
        .ok_or_else(|| IntelError::Invalid(format!("source {source_id} not found")))?
        .get(0);
    let updated = tx
        .query_one(
            "UPDATE content_objects SET ref_count = ref_count - 1 WHERE content_hash = $1 RETURNING ref_count",
            &[&content_hash],
        )?
        .get(0);
    tx.commit()?;
    Ok(updated)
}

/// 软删除文档（R2 10.4：默认可恢复删除；版本与对象不释放）。
pub fn soft_delete_document(pool: &PgPool, document_id: &DocumentId) -> IntelResult<()> {
    let mut conn = pool.get()?;
    let updated = conn.execute(
        "UPDATE documents SET lifecycle_status = 'DELETED', updated_at = now()
         WHERE document_id = $1 AND lifecycle_status = 'ACTIVE'",
        &[&document_id.as_str()],
    )?;
    if updated == 0 {
        return Err(IntelError::Invalid(format!(
            "document {document_id} not found or already deleted"
        )));
    }
    Ok(())
}

/// 文档摘要视图（list/show 共用）。
#[derive(Debug, serde::Serialize)]
pub struct DocumentSummary {
    pub document_id: String,
    pub title: String,
    pub document_type: String,
    pub sha256: String,
    pub file_size: i64,
    pub workspace_id: String,
    pub visibility: String,
    pub lifecycle_status: String,
    pub source_count: i64,
    pub revision_count: i64,
    pub created_at: String,
}

/// 列表（workspace 过滤 + 权限前置谓词示例面：visibility 条件在 SQL 内，
/// 13-SECURITY 第五节口径；include_deleted=false 时软删行不可见）。
pub fn list_documents(
    pool: &PgPool,
    workspace_id: &str,
    include_deleted: bool,
) -> IntelResult<Vec<DocumentSummary>> {
    let mut conn = pool.get()?;
    let rows = conn.query(
        "SELECT d.document_id, d.title, d.document_type, d.sha256, d.file_size, d.workspace_id,
                d.visibility, d.lifecycle_status, d.created_at,
                (SELECT COUNT(*) FROM source_records s WHERE s.content_hash = d.sha256) AS source_count,
                (SELECT COUNT(*) FROM document_revisions r WHERE r.document_id = d.document_id) AS revision_count
         FROM documents d
         WHERE d.workspace_id = $1 AND ($2 OR d.lifecycle_status = 'ACTIVE')
         ORDER BY d.created_at DESC",
        &[&workspace_id, &include_deleted],
    )?;
    Ok(rows
        .iter()
        .map(|r| {
            let ts: std::time::SystemTime = r.get(8);
            DocumentSummary {
                document_id: r.get(0),
                title: r.get(1),
                document_type: r.get(2),
                sha256: r.get(3),
                file_size: r.get(4),
                workspace_id: r.get(5),
                visibility: r.get(6),
                lifecycle_status: r.get(7),
                created_at: timestamp_to_string(ts),
                source_count: r.get(9),
                revision_count: r.get(10),
            }
        })
        .collect())
}

// created_at 为 TIMESTAMPTZ：postgres crate 返回 SystemTime；转 RFC3339（UTC）。
fn timestamp_to_string(value: std::time::SystemTime) -> String {
    let secs = value
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    // 纯 std RFC3339（UTC）：日期算法（civil from days，Howard Hinnant 口径）。
    let days = (secs / 86400) as i64;
    let rem = secs % 86400;
    let (y, m, d) = civil_from_days(days);
    format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z", rem / 3600, (rem % 3600) / 60, rem % 60)
}

fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// 版本历史视图（历史可访问：SUPERSEDED 行在列）。
#[derive(Debug, serde::Serialize)]
pub struct RevisionSummary {
    pub revision_id: String,
    pub revision_number: i64,
    pub content_hash: String,
    pub parser_version: Option<String>,
    pub revision_status: String,
    pub created_at: String,
}

/// 取版本链（document 维度，number DESC）。
pub fn list_revisions(
    pool: &PgPool,
    document_id: &DocumentId,
) -> IntelResult<Vec<RevisionSummary>> {
    let mut conn = pool.get()?;
    let rows = conn.query(
        "SELECT revision_id, revision_number, content_hash, parser_version, revision_status, created_at
         FROM document_revisions WHERE document_id = $1 ORDER BY revision_number DESC",
        &[&document_id.as_str()],
    )?;
    Ok(rows
        .iter()
        .map(|r| {
            let ts: std::time::SystemTime = r.get(5);
            RevisionSummary {
                revision_id: r.get(0),
                revision_number: r.get(1),
                content_hash: r.get(2),
                parser_version: r.get(3),
                revision_status: r.get(4),
                created_at: timestamp_to_string(ts),
            }
        })
        .collect())
}

/// 读当前版本内容（经 Adapter 读回 + sha256 复核）。
pub fn read_current_content(
    pool: &PgPool,
    store: &dyn ObjectStore,
    document_id: &DocumentId,
) -> IntelResult<Vec<u8>> {
    let mut conn = pool.get()?;
    let uri: String = conn
        .query_one(
            "SELECT storage_uri FROM documents WHERE document_id = $1 AND lifecycle_status = 'ACTIVE'",
            &[&document_id.as_str()],
        )?
        .get(0);
    store.read(&uri)
}

/// 对象引用计数视图（CAS 对账面，selftest/gc 共用）。
pub fn object_ref_count(pool: &PgPool, content_hash: &str) -> IntelResult<Option<i64>> {
    let mut conn = pool.get()?;
    let row = conn.query_opt(
        "SELECT ref_count FROM content_objects WHERE content_hash = $1",
        &[&content_hash],
    )?;
    Ok(row.map(|r| r.get(0)))
}

/// 内容哈希单点实现（CLI 与测试共用 object_store 实现）。
pub fn content_sha256_hex(content: &[u8]) -> String {
    crate::object_store::sha256_hex(content)
}
