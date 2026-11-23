//! rp-doc：RIP3 情报核 Document 域 CLI（P1-4 起步面，JSON stdout，
//! 沿 rp-* 11 bin 帮助面统一口径）。
//!
//! 子命令：import / add-source / new-revision / delete-source / soft-delete /
//! list / show / health；P2-1 增 parse（解析管线驱动）/ parse-status /
//! approve（ADR-0003 裁决 2/4）。运行配置 RP_PG_DSN（lib.rs 缺省口径）；对象落
//! Object Storage（LocalFsStore，RIP_OBJECT_STORE_DIR 口径见 object_store.rs）。
//! parse 面经 PodmanParserProvider（rip-parser 容器族，network=none）。
//!
//! exit: ok=0，运行失败=1，用法错误=2（rp-* 家族惯例）。

use intel_db::commit;
use intel_db::document_repo::{self, ImportInput};
use intel_db::ir::DocumentIr;
use intel_db::object_store::LocalFsStore;
use intel_db::parser::{DocumentParser, ParserProviderConfig, PodmanParserProvider};
use intel_db::{health, run_migrations, IntelError, IntelResult, PgConfig};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

fn usage() -> ! {
    eprintln!(
        "usage: rp-doc <sub> [args]\n\
\n\
subcommands:\n\
  import --file PATH --title T [--original-filename N] [--type T] [--source-type ST] [--source-url U]\n\
         [--source-commit C] [--workspace W] [--visibility V] [--language L]\n\
    magic bytes 检测 + CAS put + 单事务落对象/来源/文档/版本 1；JSON 输出\n\
  add-source --document D --source-type ST [--source-url U] [--source-commit C]\n\
    同内容对象追加来源（引用计数 +1，多来源引用面）\n\
  new-revision --document D --file PATH [--parser-version V] [--source-commit C]\n\
    新版本：旧版本转 SUPERSEDED（历史可访问）\n\
  delete-source --source S        删来源（引用计数 -1，对象账面保护）\n\
  soft-delete --document D        软删文档（可恢复）\n\
  list [--workspace W] [--all]    文档列表（权限前置谓词示例面）\n\
  show --document D               版本历史（含 SUPERSEDED）\n\
  parse --document D [--revision R] [--ocr] [--force] [--from-run WF_RUN] [--run-db P] [--manifest-dir D] [--timeout S]\n\
    解析管线驱动：detect 路由 -> rip-parser 容器 parse/ocr -> IR 校验 -> blocks/assets\n\
    落库 + parse_state 回填（quality/风险标记判 REVIEW_PENDING）；已提交版本幂等跳过，--force 重解析；\n\
  --from-run WF_RUN               run 产物接续：rp-workflow run 的解析产物直接落库（不经容器）\n\
  parse-status --document D [--revision R]   解析状态视图（九态/质量/计数）\n\
  approve --revision R            人工审核通过（REVIEW_PENDING -> APPROVED）\n\
  health                          健康面（版本/扩展/迁移/行数）\n\
\n\
env: RP_PG_DSN（缺省 postgres://postgres:postgres@127.0.0.1:5432/postgres）\n\
     RIP_OBJECT_STORE_DIR（缺省 rip-data/object-store）\n\
     RIP_DATA_DIR（parse 面 ops 库/工作区/暂存根的根，缺省 rip-data/）\n\
help: rp-doc --help（stdout, exit 0）"
    );
    std::process::exit(2);
}

fn help_only() -> ! {
    println!("rp-doc：RIP3 情报核 Document 域 CLI（子命令清单见 stderr usage；此行输出后 exit 0）");
    std::process::exit(0);
}

struct Args {
    flags: std::collections::HashMap<String, String>,
    subs: Vec<String>,
}

fn parse_args(iter: impl Iterator<Item = String>) -> Args {
    // 布尔旗标（不带值；出现即 true）。
    const BOOL_FLAGS: [&str; 3] = ["force", "all", "ocr"];
    let mut flags = std::collections::HashMap::new();
    let mut subs = Vec::new();
    let mut i = iter.peekable();
    while let Some(arg) = i.next() {
        if let Some(name) = arg.strip_prefix("--") {
            if BOOL_FLAGS.contains(&name) {
                flags.insert(name.to_string(), "true".to_string());
            } else {
                let value = i.next().unwrap_or_else(|| usage());
                flags.insert(name.to_string(), value);
            }
        } else {
            subs.push(arg);
        }
    }
    Args { flags, subs }
}

fn require_flag(args: &Args, name: &str) -> String {
    args.flags.get(name).cloned().unwrap_or_else(|| usage())
}

fn die_json(code: &str, message: &str) -> ! {
    println!("{{\"error\":{{\"code\":{code:?},\"message\":{message:?}}}}}");
    std::process::exit(1);
}

/// 魔法数检测（P1-1 轻面：常见格式前缀 + 兜底；infer crate 于 P1-4 批按
/// 成熟库口径评估引入，本文件先 stdlib 零依赖）。
fn detect_mime(path: &Path, content: &[u8]) -> String {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .unwrap_or_default();
    if content.starts_with(b"%PDF-") {
        "application/pdf".to_string()
    } else if content.starts_with(b"PK\x03\x04") && matches!(ext.as_str(), "docx" | "pptx" | "xlsx")
    {
        format!("application/{ext}")
    } else if content.starts_with(b"\x89PNG") {
        "image/png".to_string()
    } else if content.starts_with(b"\xff\xd8\xff") {
        "image/jpeg".to_string()
    } else if matches!(ext.as_str(), "md" | "markdown") {
        "text/markdown".to_string()
    } else if matches!(ext.as_str(), "html" | "htm") {
        "text/html".to_string()
    } else if matches!(ext.as_str(), "epub") {
        "application/epub+zip".to_string()
    } else if std::str::from_utf8(content).is_ok() {
        "text/plain".to_string()
    } else {
        "application/octet-stream".to_string()
    }
}

fn document_type_from(ext: &str) -> String {
    match ext {
        "pdf" => "pdf",
        "docx" => "docx",
        "pptx" => "pptx",
        "xlsx" => "xlsx",
        "html" | "htm" => "html",
        "md" | "markdown" => "markdown",
        "epub" => "book",
        "png" | "jpg" | "jpeg" => "image",
        _ => "other",
    }
    .to_string()
}

/// 解析版本选择：显式 revision 或文档当前 CURRENT 版本。
fn resolve_revision(
    pool: &intel_db::PgPool,
    document: &domain::DocumentId,
    revision: Option<&str>,
) -> IntelResult<domain::DocumentRevisionId> {
    match revision {
        Some(r) => domain::DocumentRevisionId::try_new(r.to_string()).map_err(IntelError::from),
        None => {
            let mut conn = pool.get()?;
            let row: Option<String> = conn
                .query_opt(
                    "SELECT revision_id FROM document_revisions
                     WHERE document_id = $1 AND revision_status = 'CURRENT'
                     ORDER BY revision_number DESC LIMIT 1",
                    &[&document.as_str()],
                )?
                .map(|r| r.get(0));
            row.map(|r| domain::DocumentRevisionId::try_new(r).map_err(IntelError::from))
                .unwrap_or_else(|| {
                    Err(IntelError::Invalid(format!(
                        "document {document} has no revision"
                    )))
                })
        }
    }
}

/// 解析管线驱动（rp-doc parse）：状态置位 → CAS 读出输入 → 容器 detect 路由
/// → parse/ocr → staging 资产按内容哈希配对 → 提交服务落库。
fn parse_command(
    args: &Args,
    pool: &intel_db::PgPool,
    store: &LocalFsStore,
) -> IntelResult<serde_json::Value> {
    let document = domain::DocumentId::try_new(require_flag(args, "document"))?;
    let revision = resolve_revision(
        pool,
        &document,
        args.flags.get("revision").map(|s| s.as_str()),
    )?;
    let mut cfg = ParserProviderConfig::default();
    if let Some(p) = args.flags.get("run-db") {
        cfg.run_db = PathBuf::from(p);
    }
    if let Some(p) = args.flags.get("manifest-dir") {
        cfg.manifest_dir = PathBuf::from(p);
    }
    if let Some(s) = args.flags.get("timeout") {
        cfg.timeout_seconds = s
            .parse()
            .map_err(|_| IntelError::Invalid("timeout must be integer seconds".to_string()))?;
    }
    // podman 挂载与文件库路径一律绝对化（相对路径只对 cwd 成立，容器面不认）。
    cfg.run_db = std::path::absolute(&cfg.run_db)?;
    cfg.workspace_root = std::path::absolute(&cfg.workspace_root)?;
    cfg.staging_root = std::path::absolute(&cfg.staging_root)?;
    cfg.sqlite_migrations_dir = std::path::absolute(&cfg.sqlite_migrations_dir)?;
    cfg.manifest_dir = std::path::absolute(&cfg.manifest_dir)?;

    // 幂等门（R2 4.6.2：以 content_hash 为幂等键；驱动层口径 = 已提交版本
    // 跳过重解析，--force 强制重解析（解析器升级后重解析的入口））。
    let force = args.flags.contains_key("force");
    let mut conn = pool.get()?;
    let row = conn.query_one(
        "SELECT r.content_hash, o.storage_uri, d.original_filename, r.parse_state, r.ir_content_hash
         FROM document_revisions r
         JOIN documents d ON d.document_id = r.document_id
         JOIN content_objects o ON o.content_hash = r.content_hash
         WHERE r.revision_id = $1",
        &[&revision.as_str()],
    )?;
    let (content_hash, storage_uri, original_filename, parse_state, ir_hash): (
        String,
        String,
        String,
        String,
        Option<String>,
    ) = (row.get(0), row.get(1), row.get(2), row.get(3), row.get(4));
    drop(conn);
    use intel_db::object_store::ObjectStore as _;
    if !force && ir_hash.is_some() && matches!(parse_state.as_str(), "APPROVED" | "REVIEW_PENDING")
    {
        return Ok(serde_json::json!({
            "document_id": document.as_str(),
            "revision_id": revision.as_str(),
            "parse_state": parse_state,
            "replayed": true,
            "ir_content_hash": ir_hash,
            "note": "idempotent skip (already committed; --force to re-parse)",
        }));
    }

    // 输入文件：版本对象 CAS 读出到临时文件（read 自带 sha256 复核）；
    // 文件名保留原始名（IR title 兜底用 original_filename，非临时名）。
    let bytes = store.read(&storage_uri)?;
    if document_repo::content_sha256_hex(&bytes) != content_hash {
        return Err(IntelError::Invalid(
            "revision content integrity mismatch".to_string(),
        ));
    }
    let input_dir = std::env::temp_dir()
        .join("rp-doc-parse")
        .join(&revision.as_str()[..20]);
    std::fs::create_dir_all(&input_dir)?;
    let input_path = input_dir.join(&original_filename);
    std::fs::write(&input_path, &bytes)?;

    // run 产物接续（ADR-0003 裁决 4 的取回通道）：--from-run <workflow_run_id>
    // 直接读 ops 库 workflow_run_node.output_json 的 IR（rp-workflow run 产出）
    // + 工作区 out 资产，落库不经容器（run 已完成解析）。
    if let Some(wf_run) = args.flags.get("from-run") {
        return parse_from_run(pool, store, &document, &revision, wf_run, &cfg);
    }

    let provider = PodmanParserProvider::new(cfg);
    let outcome = if args.flags.contains_key("ocr") {
        let ocr: &dyn intel_db::parser::OCRProvider = &provider;
        ocr.ocr(&input_path, "eng+chi_sim")?
    } else {
        let parser: &dyn DocumentParser = &provider;
        parser.parse(&input_path)?
    };
    let _ = std::fs::remove_dir_all(&input_dir);

    // 资产配对：staging/assets 按内容 sha256 匹配 IR asset_id（容器文件名与
    // asset_id 无关，哈希即身份）。
    let mut asset_files: HashMap<String, PathBuf> = HashMap::new();
    let assets_dir = outcome.staging_dir.join("assets");
    if assets_dir.exists() {
        for entry in std::fs::read_dir(&assets_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_file() {
                let content = std::fs::read(&path)?;
                let hash = document_repo::content_sha256_hex(&content);
                asset_files.entry(hash).or_insert(path);
            }
        }
    }
    let mut by_asset: HashMap<String, PathBuf> = HashMap::new();
    for asset in &outcome.ir.assets {
        if let Some(path) = asset_files.get(&asset.content_sha256) {
            by_asset.insert(asset.asset_id.clone(), path.clone());
        }
    }

    let commit_outcome =
        commit::commit_parse_result(pool, store, &document, &revision, &outcome.ir, &by_asset)?;
    Ok(serde_json::json!({
        "document_id": document.as_str(),
        "revision_id": revision.as_str(),
        "format": outcome.format,
        "parser": outcome.ir.parser_metadata.parser,
        "parser_version": outcome.ir.parser_metadata.parser_version,
        "blocks": outcome.ir.blocks.len(),
        "assets": outcome.ir.assets.len(),
        "overall": outcome.ir.extraction_quality.overall,
        "flags": outcome.ir.extraction_quality.flags,
        "parse_state": commit_outcome.parse_state,
        "ir_content_hash": commit_outcome.ir_content_hash,
        "replayed": commit_outcome.replayed,
        "staging_dir": outcome.staging_dir.to_string_lossy(),
    }))
}

/// run 产物接续提交（P2-2）：workflow run 终态后从 ops 库取回 IR 与工作区
/// 资产，提交服务落库。取回通道 = workflow_run_node.output_json（parsing
/// 节点，tool 信封 result 数据含 IR）与 tool_run（trace_id 为 workflow
/// run_id、tool_id 为 rip.parser.docling、status 为 SUCCEEDED）的工作区
/// out 目录（哈希即身份）。
fn parse_from_run(
    pool: &intel_db::PgPool,
    store: &LocalFsStore,
    document: &domain::DocumentId,
    revision: &domain::DocumentRevisionId,
    wf_run: &str,
    cfg: &ParserProviderConfig,
) -> IntelResult<serde_json::Value> {
    let ops = database::Pool::open(database::PoolConfig {
        path: database::DbPath::File(cfg.run_db.clone()),
        readonly: false,
        max_connections: 4,
        busy_timeout: std::time::Duration::from_secs(5),
    })
    .map_err(|e| IntelError::Invalid(format!("ops db open: {e}")))?;
    let node_row: Option<String> = ops
        .with_conn(|c| {
            c.query_row(
                "SELECT output_json FROM workflow_run_node
                 WHERE run_id = ?1 AND node_id = 'parsing' AND state = 'succeeded'",
                [wf_run],
                |r| r.get(0),
            )
            .map_err(database::DbError::Sqlite)
        })
        .ok();
    let node_row = node_row.ok_or_else(|| {
        IntelError::Invalid(format!(
            "workflow run {wf_run} has no succeeded parsing node (in this ops db: {:?})",
            cfg.run_db
        ))
    })?;
    let data: serde_json::Value = serde_json::from_str(&node_row)
        .map_err(|e| IntelError::Invalid(format!("node output parse: {e}")))?;
    let raw_ir = data
        .get("ir")
        .cloned()
        .ok_or_else(|| IntelError::Invalid("parsing node output missing ir".to_string()))?;
    let ir: DocumentIr = serde_json::from_value(raw_ir.clone())
        .map_err(|e| IntelError::Invalid(format!("ir deserialize: {e}")))?;

    // 解析工具 run 的工作区（资产文件面；驱动不清理，哈希即身份）。
    let tool_run: Option<String> = ops
        .with_conn(|c| {
            c.query_row(
                "SELECT run_id FROM tool_run
                 WHERE trace_id = ?1 AND tool_id = 'rip.parser.docling' AND status = 'SUCCEEDED'
                 ORDER BY finished_at DESC LIMIT 1",
                [wf_run],
                |r| r.get(0),
            )
            .map_err(database::DbError::Sqlite)
        })
        .ok();
    let mut asset_files: HashMap<String, PathBuf> = HashMap::new();
    let mut ws_out_note = None;
    if let Some(tool_run_id) = &tool_run {
        let ws_out = cfg.workspace_root.join(tool_run_id).join("out");
        let assets_dir = ws_out.join("assets");
        if assets_dir.exists() {
            for entry in std::fs::read_dir(&assets_dir)? {
                let path = entry?.path();
                if path.is_file() {
                    let content = std::fs::read(&path)?;
                    let hash = document_repo::content_sha256_hex(&content);
                    asset_files.entry(hash).or_insert(path);
                }
            }
        }
        ws_out_note = Some(ws_out.to_string_lossy().to_string());
    }
    let mut by_asset: HashMap<String, PathBuf> = HashMap::new();
    for asset in &ir.assets {
        if let Some(path) = asset_files.get(&asset.content_sha256) {
            by_asset.insert(asset.asset_id.clone(), path.clone());
        }
    }
    let missing = ir.assets.len() - by_asset.len();
    if missing > 0 {
        return Err(IntelError::Invalid(format!(
            "run asset files missing for {missing} asset(s) (workspace out dir gone or hash mismatch)"
        )));
    }
    let committed = commit::commit_parse_result(pool, store, document, revision, &ir, &by_asset)?;
    Ok(serde_json::json!({
        "document_id": document.as_str(),
        "revision_id": revision.as_str(),
        "from_run": wf_run,
        "tool_run_id": tool_run,
        "format": data.get("format").cloned().unwrap_or(serde_json::json!("unknown")),
        "parser": ir.parser_metadata.parser,
        "parser_version": ir.parser_metadata.parser_version,
        "blocks": ir.blocks.len(),
        "assets": ir.assets.len(),
        "overall": ir.extraction_quality.overall,
        "flags": ir.extraction_quality.flags,
        "parse_state": committed.parse_state,
        "ir_content_hash": committed.ir_content_hash,
        "replayed": committed.replayed,
        "ws_out": ws_out_note,
    }))
}

/// 解析状态视图。
fn parse_status_command(args: &Args, pool: &intel_db::PgPool) -> IntelResult<serde_json::Value> {
    let document = domain::DocumentId::try_new(require_flag(args, "document"))?;
    let revision = resolve_revision(
        pool,
        &document,
        args.flags.get("revision").map(|s| s.as_str()),
    )?;
    let mut conn = pool.get()?;
    let row = conn
        .query_opt(
            "SELECT r.parse_state, r.parser_version, r.extraction_quality, r.ir_content_hash,
                    (SELECT COUNT(*) FROM document_blocks b WHERE b.revision_id = r.revision_id),
                    (SELECT COUNT(*) FROM document_assets a WHERE a.revision_id = r.revision_id),
                    r.revision_status, r.revision_number
             FROM document_revisions r WHERE r.revision_id = $1",
            &[&revision.as_str()],
        )?
        .ok_or_else(|| IntelError::Invalid(format!("revision {} not found", revision.as_str())))?;
    let blocks: i64 = row.get(4);
    let assets: i64 = row.get(5);
    Ok(serde_json::json!({
        "document_id": document.as_str(),
        "revision_id": revision.as_str(),
        "revision_number": row.get::<_, i64>(7),
        "revision_status": row.get::<_, String>(6),
        "parse_state": row.get::<_, String>(0),
        "parser_version": row.get::<_, Option<String>>(1),
        "extraction_quality": row.get::<_, Option<serde_json::Value>>(2),
        "ir_content_hash": row.get::<_, Option<String>>(3),
        "blocks": blocks,
        "assets": assets,
    }))
}

fn main() {
    let mut argv = std::env::args().peekable();
    let _bin = argv.next();
    if argv
        .peek()
        .map(|s| s == "--help" || s == "-h")
        .unwrap_or(false)
    {
        help_only();
    }
    let args = parse_args(argv);
    let sub = args.subs.first().cloned().unwrap_or_else(|| usage());
    let store = LocalFsStore::from_env();
    let pool = match intel_db::open_pool(&PgConfig::default()) {
        Ok(p) => p,
        Err(e) => die_json("pg_unavailable", &e.to_string()),
    };

    let result: intel_db::IntelResult<serde_json::Value> = (|| match sub.as_str() {
        "health" => {
            let mut conn = pool.get()?;
            run_migrations(&mut conn)?;
            serde_json::to_value(health(&pool)?).map_err(|e| IntelError::Invalid(e.to_string()))
        }
        "import" => {
            let file = PathBuf::from(require_flag(&args, "file"));
            let content = std::fs::read(&file).map_err(IntelError::from)?;
            let mime = detect_mime(&file, &content);
            let ext = file
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or_default()
                .to_ascii_lowercase();
            let input = ImportInput {
                title: require_flag(&args, "title"),
                document_type: args
                    .flags
                    .get("type")
                    .cloned()
                    .unwrap_or_else(|| document_type_from(&ext)),
                original_filename: args.flags.get("original-filename").cloned().unwrap_or_else(
                    || {
                        file.file_name()
                            .map(|s| s.to_string_lossy().to_string())
                            .unwrap_or_default()
                    },
                ),
                mime_type: mime,
                language: args.flags.get("language").cloned(),
                workspace_id: args
                    .flags
                    .get("workspace")
                    .cloned()
                    .unwrap_or_else(|| "default".to_string()),
                visibility: domain::DocumentVisibility::parse(
                    args.flags
                        .get("visibility")
                        .map(|s| s.as_str())
                        .unwrap_or("private"),
                )?,
                source_type: args
                    .flags
                    .get("source-type")
                    .cloned()
                    .unwrap_or_else(|| "upload".to_string()),
                source_url: args.flags.get("source-url").cloned().unwrap_or_default(),
                source_commit: args.flags.get("source-commit").cloned(),
            };
            let mut conn = pool.get()?;
            run_migrations(&mut conn)?;
            let outcome = document_repo::import_document(&pool, &store, &input, &content)?;
            let content_hash = document_repo::content_sha256_hex(&content);
            Ok(serde_json::json!({
                "document_id": outcome.document_id.as_str(),
                "revision_id": outcome.revision_id.as_str(),
                "source_id": outcome.source_id.as_str(),
                "content_object_created": outcome.content_object_created,
                "content_hash": content_hash,
            }))
        }
        "add-source" => {
            let document = domain::DocumentId::try_new(require_flag(&args, "document"))?;
            let source_type = require_flag(&args, "source-type");
            let source_url = args.flags.get("source-url").cloned().unwrap_or_default();
            let source_commit = args.flags.get("source-commit").map(|s| s.as_str());
            let source = document_repo::add_source(
                &pool,
                &document,
                &source_type,
                &source_url,
                source_commit,
            )?;
            Ok(serde_json::json!({ "source_id": source.as_str() }))
        }
        "new-revision" => {
            let document = domain::DocumentId::try_new(require_flag(&args, "document"))?;
            let content = std::fs::read(require_flag(&args, "file")).map_err(IntelError::from)?;
            let parser_version = args.flags.get("parser-version").map(|s| s.as_str());
            let source_commit = args.flags.get("source-commit").map(|s| s.as_str());
            let revision = document_repo::new_revision(
                &pool,
                &store,
                &document,
                &content,
                parser_version,
                source_commit,
            )?;
            Ok(serde_json::json!({ "revision_id": revision.as_str() }))
        }
        "delete-source" => {
            let source = domain::SourceRecordId::try_new(require_flag(&args, "source"))?;
            let ref_count = document_repo::delete_source(&pool, &source)?;
            Ok(serde_json::json!({ "ref_count": ref_count }))
        }
        "soft-delete" => {
            let document = domain::DocumentId::try_new(require_flag(&args, "document"))?;
            document_repo::soft_delete_document(&pool, &document)?;
            Ok(serde_json::json!({ "deleted": true }))
        }
        "list" => {
            let mut conn = pool.get()?;
            run_migrations(&mut conn)?;
            let workspace = args
                .flags
                .get("workspace")
                .cloned()
                .unwrap_or_else(|| "default".to_string());
            let items =
                document_repo::list_documents(&pool, &workspace, args.flags.contains_key("all"))?;
            Ok(serde_json::json!({ "items": items }))
        }
        "show" => {
            let document = domain::DocumentId::try_new(require_flag(&args, "document"))?;
            let revisions = document_repo::list_revisions(&pool, &document)?;
            Ok(serde_json::json!({ "revisions": revisions }))
        }
        "parse" => Ok(parse_command(&args, &pool, &store)?),
        "parse-status" => Ok(parse_status_command(&args, &pool)?),
        "approve" => {
            let revision = domain::DocumentRevisionId::try_new(require_flag(&args, "revision"))?;
            commit::review_approve(&pool, &revision)?;
            Ok(serde_json::json!({ "revision_id": revision.as_str(), "parse_state": "APPROVED" }))
        }
        _ => usage(),
    })();

    match result {
        Ok(value) => println!("{}", serde_json::to_string(&value).unwrap_or_default()),
        Err(IntelError::Invalid(m)) => die_json("invalid", &m),
        Err(e) => die_json("runtime_failed", &e.to_string()),
    }
}
