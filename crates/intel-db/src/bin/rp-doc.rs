//! rp-doc：RIP3 情报核 Document 域 CLI（P1-4 起步面，JSON stdout，
//! 沿 rp-* 11 bin 帮助面统一口径）。
//!
//! 子命令：import / add-source / new-revision / delete-source / soft-delete /
//! list / show / health。运行配置 RP_PG_DSN（lib.rs 缺省口径）；对象落
//! Object Storage（LocalFsStore，RIP_OBJECT_STORE_DIR 口径见 object_store.rs）。
//!
//! exit: ok=0，运行失败=1，用法错误=2（rp-* 家族惯例）。

use intel_db::document_repo::{self, ImportInput};
use intel_db::object_store::LocalFsStore;
use intel_db::{health, run_migrations, IntelError, PgConfig};
use std::path::{Path, PathBuf};

fn usage() -> ! {
    eprintln!(
        "usage: rp-doc <sub> [args]\n\
\n\
subcommands:\n\
  import --file PATH --title T [--type T] [--source-type ST] [--source-url U]\n\
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
  health                          健康面（版本/扩展/迁移/行数）\n\
\n\
env: RP_PG_DSN（缺省 postgres://postgres:postgres@127.0.0.1:5432/postgres）\n\
     RIP_OBJECT_STORE_DIR（缺省 rip-data/object-store）\n\
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
    let mut flags = std::collections::HashMap::new();
    let mut subs = Vec::new();
    let mut i = iter.peekable();
    while let Some(arg) = i.next() {
        if let Some(name) = arg.strip_prefix("--") {
            let value = i.next().unwrap_or_else(|| usage());
            flags.insert(name.to_string(), value);
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
    } else if content.starts_with(b"PK\x03\x04") && matches!(ext.as_str(), "docx" | "pptx" | "xlsx") {
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

fn main() {
    let mut argv = std::env::args().peekable();
    let _bin = argv.next();
    if argv.peek().map(|s| s == "--help" || s == "-h").unwrap_or(false) {
        help_only();
    }
    let args = parse_args(argv);
    let sub = args.subs.first().cloned().unwrap_or_else(|| usage());
    let store = LocalFsStore::from_env();
    let pool = match intel_db::open_pool(&PgConfig::default()) {
        Ok(p) => p,
        Err(e) => die_json("pg_unavailable", &e.to_string()),
    };

    let result: intel_db::IntelResult<serde_json::Value> = (|| {
        match sub.as_str() {
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
                    original_filename: file
                        .file_name()
                        .map(|s| s.to_string_lossy().to_string())
                        .unwrap_or_default(),
                    mime_type: mime,
                    language: args.flags.get("language").cloned(),
                    workspace_id: args
                        .flags
                        .get("workspace")
                        .cloned()
                        .unwrap_or_else(|| "default".to_string()),
                    visibility: domain::DocumentVisibility::parse(
                        args.flags.get("visibility").map(|s| s.as_str()).unwrap_or("private"),
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
                let source =
                    document_repo::add_source(&pool, &document, &source_type, &source_url, source_commit)?;
                Ok(serde_json::json!({ "source_id": source.as_str() }))
            }
            "new-revision" => {
                let document = domain::DocumentId::try_new(require_flag(&args, "document"))?;
                let content = std::fs::read(require_flag(&args, "file")).map_err(IntelError::from)?;
                let parser_version = args.flags.get("parser-version").map(|s| s.as_str());
                let source_commit = args.flags.get("source-commit").map(|s| s.as_str());
                let revision = document_repo::new_revision(
                    &pool, &store, &document, &content, parser_version, source_commit,
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
            _ => usage(),
        }
    })();

    match result {
        Ok(value) => println!("{}", serde_json::to_string(&value).unwrap_or_default()),
        Err(IntelError::Invalid(m)) => die_json("invalid", &m),
        Err(e) => die_json("runtime_failed", &e.to_string()),
    }
}
