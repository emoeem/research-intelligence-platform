//! P2-1 集成测试：解析管线（handover 第七节验收骨架
//! `cargo test --offline -p intel-db --test parser_pipeline`）。
//!
//! 门禁口径：V-9 同款——RP_TEST_PG_DSN 未设或不可达时跳过（CI 无 PG
//! service）；容器解析面（真 rip-parser 镜像）按 digest 钉死引用探本地
//! 镜像库，缺失跳过（镜像属运行时数据 ADR §3.2，受控环境本地验收实跑）；
//! 纯逻辑面（IR 校验/树门/重映射/幂等提交的 PG 写面）在 PG 在场时实跑。
//!
//! 判据（任务书第二节逐条）：
//! 1. 结构正确：父子不压平（树门）、blocks 落库回读一致（roundtrip）；
//! 2. 页码可定位：page_number/location_metadata 对账；
//! 3. 失败可重试：幂等重放（replayed=true 零重复行）+ --force 语义
//!    （重放后块数稳定）+ review_approve 面；
//! 4. 解析器版本可记录：parser_version / ir_content_hash / extraction_quality；
//! 5. 风险标记判 REVIEW_PENDING（POSSIBLE_PROMPT_INJECTION → 人工 approve）。

use intel_db::commit;
use intel_db::document_repo::{self, ImportInput};
use intel_db::ir::{validate_ir, validate_tree, DocumentIr};
use intel_db::object_store::LocalFsStore;
use intel_db::{open_pool, run_migrations, PgConfig};
use std::path::PathBuf;

fn skip_unless_pg() -> Option<(
    r2d2::Pool<r2d2_postgres::PostgresConnectionManager<postgres::NoTls>>,
    LocalFsStore,
    PathBuf,
)> {
    let dsn = std::env::var("RP_TEST_PG_DSN").ok()?;
    let pool = match open_pool(&PgConfig {
        dsn,
        max_connections: 4,
    }) {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: RP_TEST_PG_DSN 不可达（V-9，容器端到端保持本地执行）");
            return None;
        }
    };
    let store_root = std::env::temp_dir().join(format!("rip-p2-it-{}", std::process::id()));
    std::fs::create_dir_all(&store_root).unwrap();
    Some((pool, LocalFsStore::new(&store_root), store_root))
}

fn import_input(title: &str, mime: &str, dtype: &str) -> ImportInput {
    ImportInput {
        title: title.to_string(),
        document_type: dtype.to_string(),
        original_filename: format!("{title}.bin"),
        mime_type: mime.to_string(),
        language: None,
        workspace_id: "ws-p2".to_string(),
        visibility: domain::DocumentVisibility::Private,
        source_type: "upload".to_string(),
        source_url: String::new(),
        source_commit: None,
    }
}

/// 容器产 IR 的事务时间字段逐 run 变化：测试 IR 用固定时间戳，重放键稳定。
fn sample_ir(injection: bool) -> (DocumentIr, serde_json::Value) {
    let flags = if injection {
        serde_json::json!(["POSSIBLE_PROMPT_INJECTION"])
    } else {
        serde_json::json!([])
    };
    let raw = serde_json::json!({
        "schema_version": "1",
        "document_metadata": {
            "document_id": "doc-pending", "title": "P2 集成样本", "document_type": "pdf",
            "original_filename": "sample.pdf", "mime_type": "application/pdf",
            "content_sha256": format!("{:064}", 7)
        },
        "parser_metadata": {
            "parser": "docling", "parser_version": "2.150.1", "options": {},
            "pipeline": ["format_detect", "parse", "normalize"],
            "started_at": "2026-11-15T00:00:00Z", "duration_ms": 100, "device": "cpu"
        },
        "structure": {"root": {"block_id": "blk-t", "children": ["blk-h1"]}},
        "blocks": [
            {"block_id": "blk-t", "block_type": "document_title", "order_index": 0,
             "content": "P2 集成样本", "extraction_method": "docling"},
            {"block_id": "blk-h1", "parent_id": "blk-t", "block_type": "heading",
             "heading_level": 1, "order_index": 0, "content": "第一章", "page": 1,
             "location": {"bbox": [72.0, 788.0, 180.0, 766.4]},
             "extraction_method": "docling"},
            {"block_id": "blk-p1", "parent_id": "blk-h1", "block_type": "paragraph",
             "order_index": 0, "content": "正文段落", "page": 1,
             "extraction_method": "docling"}
        ],
        "assets": [],
        "references": [],
        "extraction_quality": {
            "self_check": [{"check": "toc_present", "passed": true, "detail": "headings=1"}],
            "overall": "good", "flags": flags
        },
        "source_mapping": {"block_to_source": [
            {"block_id": "blk-h1", "page": 1, "bbox": [72.0, 788.0, 180.0, 766.4]},
            {"block_id": "blk-p1", "page": 1}
        ]}
    });
    let ir: DocumentIr = serde_json::from_value(raw.clone()).unwrap();
    (ir, raw)
}

#[test]
fn ir_validation_and_tree_gates() {
    let (ir, raw) = sample_ir(false);
    validate_ir(&ir, &raw).unwrap();
    // 环拒：title 指回 paragraph。
    let (mut ir_c, raw_c) = sample_ir(false);
    ir_c.blocks[0].parent_id = Some("blk-p1".to_string());
    assert!(validate_tree(&ir_c).is_err());
    let _ = raw_c;
    // children 声明漂移拒。
    let (mut ir_d, raw_d) = sample_ir(false);
    ir_d.structure.root.children.clear();
    assert!(validate_ir(&ir_d, &raw_d).is_err());
}

#[test]
fn commit_roundtrip_and_idempotent_replay() {
    let Some((pool, store, _root)) = skip_unless_pg() else {
        return;
    };
    let mut conn = pool.get().unwrap();
    run_migrations(&mut conn).unwrap();
    drop(conn);

    let content = b"pdf-like content for p2 it";
    let outcome = document_repo::import_document(
        &pool,
        &store,
        &import_input("p2-roundtrip", "application/pdf", "pdf"),
        content,
    )
    .unwrap();

    let (ir, _raw) = sample_ir(false);
    let empty_assets = std::collections::HashMap::new();
    let first = commit::commit_parse_result(
        &pool,
        &store,
        &outcome.document_id,
        &outcome.revision_id,
        &ir,
        &empty_assets,
    )
    .unwrap();
    assert_eq!(first.parse_state, "APPROVED");
    assert_eq!(first.blocks_written, 3);
    assert!(!first.replayed);

    // 回读对账：块数/父子/页码/方法。
    let blocks = commit::read_blocks(&pool, &outcome.revision_id).unwrap();
    assert_eq!(blocks.len(), 3);
    let title = &blocks[0];
    assert_eq!(title["block_type"], "document_title");
    let heading = blocks
        .iter()
        .find(|b| b["block_type"] == "heading")
        .expect("heading block");
    assert_eq!(heading["page_number"], 1);
    assert_eq!(heading["extraction_method"], "docling");
    assert_eq!(
        heading["parent_id"].as_str().unwrap(),
        title["block_id"].as_str().unwrap(),
        "父子结构不压平"
    );
    assert!(
        heading["location_metadata"]["bbox"].is_array(),
        "bbox 落 location_metadata"
    );

    // 幂等重放：同 IR 再提交 = replayed=true，块数零重复。
    let second = commit::commit_parse_result(
        &pool,
        &store,
        &outcome.document_id,
        &outcome.revision_id,
        &ir,
        &empty_assets,
    )
    .unwrap();
    assert!(second.replayed, "同 IR 二次提交必须命中重放键");
    let blocks2 = commit::read_blocks(&pool, &outcome.revision_id).unwrap();
    assert_eq!(blocks2.len(), 3, "重放不得翻倍行数");

    // 解析器版本可记录（parser_version 列 + ir_content_hash 凭证）。
    let mut conn = pool.get().unwrap();
    let row = conn
        .query_one(
            "SELECT parser_version, ir_content_hash, extraction_quality, parse_state
             FROM document_revisions WHERE revision_id = $1",
            &[&outcome.revision_id.as_str()],
        )
        .unwrap();
    assert_eq!(
        row.get::<_, Option<String>>(0).as_deref(),
        Some("docling 2.150.1")
    );
    assert!(row.get::<_, Option<String>>(1).is_some());
    assert_eq!(row.get::<_, Option<String>>(3).as_deref(), Some("APPROVED"));
    let quality: serde_json::Value = row.get::<_, Option<serde_json::Value>>(2).unwrap();
    assert_eq!(quality["overall"], "good");
}

#[test]
fn injection_flag_routes_review_and_approve() {
    let Some((pool, store, _root)) = skip_unless_pg() else {
        return;
    };
    let mut conn = pool.get().unwrap();
    run_migrations(&mut conn).unwrap();
    drop(conn);

    let content = b"prompt injection sample content";
    let outcome = document_repo::import_document(
        &pool,
        &store,
        &import_input("p2-injection", "application/pdf", "pdf"),
        content,
    )
    .unwrap();

    let (ir, _raw) = sample_ir(true);
    let empty_assets = std::collections::HashMap::new();
    let committed = commit::commit_parse_result(
        &pool,
        &store,
        &outcome.document_id,
        &outcome.revision_id,
        &ir,
        &empty_assets,
    )
    .unwrap();
    assert_eq!(
        committed.parse_state, "REVIEW_PENDING",
        "POSSIBLE_PROMPT_INJECTION 必须判 REVIEW_PENDING（13 号冻结物第三节）"
    );
    // 人工审核面：approve 后 APPROVED。
    commit::review_approve(&pool, &outcome.revision_id).unwrap();
    // 重复 approve 拒（幂等防线）。
    assert!(commit::review_approve(&pool, &outcome.revision_id).is_err());
    let mut conn = pool.get().unwrap();
    let state: String = conn
        .query_one(
            "SELECT parse_state FROM document_revisions WHERE revision_id = $1",
            &[&outcome.revision_id.as_str()],
        )
        .unwrap()
        .get(0);
    assert_eq!(state, "APPROVED");
}

#[test]
fn parse_state_words_gated() {
    let Some((pool, store, _root)) = skip_unless_pg() else {
        return;
    };
    let mut conn = pool.get().unwrap();
    run_migrations(&mut conn).unwrap();
    drop(conn);
    let content = b"parse state words content";
    let outcome = document_repo::import_document(
        &pool,
        &store,
        &import_input("p2-states", "application/pdf", "pdf"),
        content,
    )
    .unwrap();
    // 合法词放行（DISCOVERED 缺省 → PARSING）。
    commit::set_parse_state(&pool, &outcome.revision_id, "PARSING").unwrap();
    // 词表外拒绝（V0003 CHECK 域）。
    assert!(commit::set_parse_state(&pool, &outcome.revision_id, "BOGUS").is_err());
    assert!(commit::set_parse_state(&pool, &outcome.revision_id, "APPROVED").is_ok());
}

#[test]
fn container_parse_end_to_end() {
    // 真容器面（V-9 双守卫：PG + 镜像 digest 钉死引用在本地镜像库）。
    let Some((pool, store, _root)) = skip_unless_pg() else {
        return;
    };
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let manifest_text = std::fs::read_to_string(
        repo_root.join("containerfiles/rip-parser/rip.parser.docling.json"),
    )
    .expect("parse manifest");
    let manifest: serde_json::Value = serde_json::from_str(&manifest_text).unwrap();
    let image = manifest["runtime"]["image"].as_str().unwrap().to_string();
    let exists = std::process::Command::new("podman")
        .args(["image", "exists", &image])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    if !exists {
        eprintln!("SKIP: 验收镜像 {image} 不在本地镜像库（V-9，受控环境端到端）");
        return;
    }

    // 冒烟输入：仓库内固定 MD 样本（无注入，approve 直通分支）。
    let md_path = repo_root.join("rip-data/parse-smoke/in/clean.md");
    if !md_path.exists() {
        eprintln!("SKIP: 冒烟样本 {md_path:?} 缺席（V-9，运行时数据）");
        return;
    }
    let content = std::fs::read(&md_path).unwrap();
    let outcome = document_repo::import_document(
        &pool,
        &store,
        &import_input("p2-e2e-md", "text/markdown", "markdown"),
        &content,
    )
    .unwrap();

    let cfg = intel_db::parser::ParserProviderConfig {
        run_db: repo_root.join("rip-data/parse/test-ops.db"),
        sqlite_migrations_dir: repo_root.join("migrations"),
        manifest_dir: repo_root.join("containerfiles/rip-parser"),
        workspace_root: repo_root.join("rip-data/parse/test-ws"),
        staging_root: repo_root.join("rip-data/parse/test-staging"),
        ..Default::default()
    };
    let provider = intel_db::parser::PodmanParserProvider::new(cfg);
    let parser: &dyn intel_db::parser::DocumentParser = &provider;
    let detect = parser.detect(&md_path).expect("detect");
    assert_eq!(detect.format, "markdown");
    assert_eq!(
        intel_db::parser::route_parse(&detect.format),
        Some(intel_db::parser::TOOL_PARSE)
    );
    let parsed = parser.parse(&md_path).expect("parse");
    assert!(parsed.ir.blocks.len() >= 2, "MD 样本至少产出标题+段落");

    // 资产配对（哈希即身份；本样本零资产，映射应为空）。
    let mut asset_files = std::collections::HashMap::new();
    let assets_dir = parsed.staging_dir.join("assets");
    if assets_dir.exists() {
        for entry in std::fs::read_dir(&assets_dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_file() {
                let bytes = std::fs::read(&path).unwrap();
                let hash = intel_db::object_store::sha256_hex(&bytes);
                asset_files.entry(hash).or_insert(path);
            }
        }
    }
    let committed = commit::commit_parse_result(
        &pool,
        &store,
        &outcome.document_id,
        &outcome.revision_id,
        &parsed.ir,
        &asset_files,
    )
    .unwrap();
    assert_eq!(committed.parse_state, "APPROVED", "干净样本直通 approve");
    let blocks = commit::read_blocks(&pool, &outcome.revision_id).unwrap();
    assert_eq!(blocks.len(), parsed.ir.blocks.len(), "落库块数与 IR 一致");
}
