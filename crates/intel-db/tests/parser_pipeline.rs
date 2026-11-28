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

use intel_db::batch_review;
use intel_db::commit;
use intel_db::document_repo::{self, ImportInput};
use intel_db::ingestion::{self, IngestionSource, KnowledgeIngestionRequest, CONTRACT_VERSION};
use intel_db::ir::{validate_ir, validate_tree, Block, DocumentIr};
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

struct SemanticTempDir(PathBuf);

impl SemanticTempDir {
    fn new() -> Self {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock after Unix epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "rip-ingestion-semantic-{}-{nonce}",
            std::process::id()
        ));
        std::fs::create_dir(&path).expect("create isolated ingestion semantic test directory");
        Self(path)
    }

    fn path(&self) -> &std::path::Path {
        &self.0
    }
}

impl Drop for SemanticTempDir {
    fn drop(&mut self) {
        if let Err(error) = std::fs::remove_dir_all(&self.0) {
            eprintln!(
                "failed to remove isolated ingestion semantic test directory {:?}: {error}",
                self.0
            );
        }
    }
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

fn invalid_nuclei_block() -> Block {
    Block {
        block_id: "blk-invalid-nuclei".to_string(),
        parent_id: Some("blk-t".to_string()),
        block_type: "paragraph".to_string(),
        order_index: 1,
        content: "id: community/unsupported-severity\ninfo:\n  name: Unsupported sample\n  severity: unknown"
            .to_string(),
        heading_level: None,
        table: None,
        code: None,
        image_ref: None,
        page: Some(1),
        location: None,
        extraction_method: "manual".to_string(),
        extraction_confidence: None,
    }
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
fn parsed_document_ingestion_maps_identifiers_to_evidence_and_review_ledger() {
    let Some((pool, store, _root)) = skip_unless_pg() else {
        return;
    };
    let mut conn = pool.get().unwrap();
    run_migrations(&mut conn).unwrap();
    drop(conn);

    let numeric_tag = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos()
        % 100_000_000;
    let cve = format!("CVE-2099-{numeric_tag:08}");
    let workspace = format!("ws-p2-sem-{numeric_tag}");
    let text = format!(
        "研究记录提及 {cve} 和 CWE-79；重复提及 {cve}，这不是事实结论。\nid: community/test-template\ninfo:\n  name: Static metadata sample\n  severity: high\n  tags: sample"
    );
    let input = ImportInput {
        title: format!("ingestion-document-{numeric_tag}"),
        document_type: "pdf".to_string(),
        original_filename: "ingestion-document.pdf".to_string(),
        mime_type: "application/pdf".to_string(),
        language: None,
        workspace_id: workspace.clone(),
        visibility: domain::DocumentVisibility::Private,
        source_type: "upload".to_string(),
        source_url: String::new(),
        source_commit: None,
    };
    let imported = document_repo::import_document(
        &pool,
        &store,
        &input,
        format!("source-{numeric_tag}").as_bytes(),
    )
    .unwrap();
    let (mut ir, _) = sample_ir(false);
    ir.blocks.push(Block {
        block_id: "blk-nuclei-related".to_string(),
        parent_id: Some("blk-t".to_string()),
        block_type: "paragraph".to_string(),
        order_index: 1,
        content: "id: community/test-template-related\ninfo:\n  name: Static metadata example\n  severity: high"
            .to_string(),
        heading_level: None,
        table: None,
        code: None,
        image_ref: None,
        page: Some(1),
        location: None,
        extraction_method: "manual".to_string(),
        extraction_confidence: None,
    });
    ir.structure
        .root
        .children
        .push("blk-nuclei-related".to_string());
    let paragraph = ir
        .blocks
        .iter_mut()
        .find(|block| block.block_type == "paragraph")
        .unwrap();
    paragraph.content = text.clone();
    ir.blocks.push(invalid_nuclei_block());
    ir.structure
        .root
        .children
        .push("blk-invalid-nuclei".to_string());
    commit::commit_parse_result(
        &pool,
        &store,
        &imported.document_id,
        &imported.revision_id,
        &ir,
        &std::collections::HashMap::new(),
    )
    .unwrap();

    let request = KnowledgeIngestionRequest {
        contract_version: CONTRACT_VERSION.to_string(),
        workspace_id: workspace.clone(),
        idempotency_key: format!("document-ingestion-{numeric_tag}"),
        source: IngestionSource::DocumentRevision {
            document_id: imported.document_id.to_string(),
            revision_id: imported.revision_id.to_string(),
        },
        requester: "integration-test".to_string(),
        run_id: None,
        trace_id: None,
    };
    let result = ingestion::submit_report(&pool, &request).unwrap();
    assert_eq!(result.status, "REVIEW_PENDING");
    let payload = result.result.unwrap();
    assert_eq!(payload["blocks_with_evidence"], 5);
    assert_eq!(payload["candidates_created"], 4);
    assert_eq!(payload["published"], false);
    assert_eq!(
        payload["extractor_diagnostics"][0]["code"],
        "nuclei_severity_unsupported"
    );
    assert_eq!(
        payload["extractor_diagnostics"][0]["source_text_untrusted"],
        true
    );
    assert_eq!(payload["extractor_diagnostics_truncated"], 0);
    let batch_id = payload["review_batch_id"].as_str().unwrap();
    let candidate_ids: Vec<String> = payload["candidate_ids"]
        .as_array()
        .unwrap()
        .iter()
        .map(|value| value.as_str().unwrap().to_string())
        .collect();
    assert_eq!(candidate_ids.len(), 4);

    let mut conn = pool.get().unwrap();
    let evidence_occurrences: i64 = conn
        .query_one(
            "SELECT count(*) FROM knowledge_ingestion_candidate_evidence ce
             JOIN knowledge_ingestion_candidates c USING(candidate_id)
             WHERE c.ingestion_id=$1",
            &[&result.ingestion_id],
        )
        .unwrap()
        .get(0);
    assert_eq!(
        evidence_occurrences, 5,
        "repeated identifiers and both static templates retain evidence offsets"
    );
    let page_number: Option<i32> = conn
        .query_one(
            "SELECT (e.location_json->>'page_number')::integer
             FROM knowledge_ingestion_candidate_evidence ce
             JOIN knowledge_ingestion_evidence e USING(evidence_id)
             WHERE ce.candidate_id=$1 LIMIT 1",
            &[&candidate_ids[0]],
        )
        .unwrap()
        .get(0);
    assert_eq!(page_number, Some(1));
    drop(conn);

    let cve_candidate = {
        let mut conn = pool.get().unwrap();
        conn.query_one(
            "SELECT candidate_id FROM knowledge_ingestion_candidates
             WHERE ingestion_id=$1 AND canonical_key=$2",
            &[&result.ingestion_id, &format!("cve:{cve}")],
        )
        .unwrap()
        .get::<_, String>(0)
    };
    let candidate_evidence =
        ingestion::get_candidate_evidence(&pool, &workspace, &result.ingestion_id, &cve_candidate)
            .unwrap()
            .unwrap();
    assert_eq!(candidate_evidence["evidence_count"], 2);
    assert_eq!(candidate_evidence["evidence"].as_array().unwrap().len(), 2);
    assert_eq!(
        candidate_evidence["evidence"][0]["source_text_untrusted"],
        true
    );
    assert!(candidate_evidence["evidence"][0]["snippet"]
        .as_str()
        .unwrap()
        .contains(&cve));
    let nuclei_candidate: String = {
        let mut conn = pool.get().unwrap();
        conn.query_one(
            "SELECT candidate_id FROM knowledge_ingestion_candidates
             WHERE ingestion_id=$1 AND candidate_type='nuclei_template'
             ORDER BY canonical_key LIMIT 1",
            &[&result.ingestion_id],
        )
        .unwrap()
        .get(0)
    };
    let nuclei_evidence = ingestion::get_candidate_evidence(
        &pool,
        &workspace,
        &result.ingestion_id,
        &nuclei_candidate,
    )
    .unwrap()
    .unwrap();
    assert_eq!(
        nuclei_evidence["candidate_schema_version"],
        "nuclei-template-metadata.v1"
    );
    assert_eq!(
        nuclei_evidence["structured_metadata"],
        serde_json::json!({
            "template_id": "community/test-template",
            "name": "Static metadata sample",
            "severity": "high"
        })
    );
    assert_eq!(nuclei_evidence["structured_metadata_untrusted"], true);
    assert_eq!(
        nuclei_evidence["assertion"],
        "static template metadata only; not a verified vulnerability or weakness claim"
    );
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let embed_manifest: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(repo_root.join("containerfiles/rip-embed/rip.embed.text.json"))
            .expect("read pinned embed manifest"),
    )
    .unwrap();
    let embed_image = embed_manifest["runtime"]["image"].as_str().unwrap();
    let embed_available = std::process::Command::new("podman")
        .args(["image", "exists", embed_image])
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false);
    if embed_available {
        let temp_root = SemanticTempDir::new();
        let embed_config = intel_db::embed_worker::EmbedWorkerConfig {
            run_db: temp_root.path().join("ops.db"),
            sqlite_migrations_dir: repo_root.join("migrations"),
            manifest_dir: repo_root.join("containerfiles/rip-embed"),
            workspace_root: temp_root.path().join("ws"),
            ..Default::default()
        };
        let unavailable_config = intel_db::embed_worker::EmbedWorkerConfig {
            run_db: temp_root.path().join("unavailable-ops.db"),
            manifest_dir: temp_root.path().join("missing-manifest"),
            workspace_root: temp_root.path().join("unavailable-ws"),
            ..embed_config.clone()
        };
        let unavailable = intel_db::candidate_semantic::scan_candidates(
            &pool,
            &workspace,
            &result.ingestion_id,
            0.70,
            10,
            &unavailable_config,
        )
        .unwrap_err();
        assert!(unavailable.to_string().contains("manifest read"));
        let mut conn = pool.get().unwrap();
        let vectors_before_retry: i64 = conn
            .query_one(
                "SELECT count(*) FROM knowledge_ingestion_candidate_vectors
                 WHERE workspace_id=$1",
                &[&workspace],
            )
            .unwrap()
            .get(0);
        assert_eq!(
            vectors_before_retry, 0,
            "failed embed writes no partial batch"
        );
        drop(conn);

        let semantic_outcome = intel_db::candidate_semantic::scan_candidates(
            &pool,
            &workspace,
            &result.ingestion_id,
            0.70,
            10,
            &embed_config,
        )
        .unwrap();
        assert_eq!(semantic_outcome.source_candidates, 4);
        assert_eq!(semantic_outcome.eligible_source_candidates, 2);
        assert_eq!(semantic_outcome.vectors_embedded, 2);
        assert!(
            semantic_outcome.suggestions_persisted > 0,
            "similar static candidate names should produce a review-only suggestion"
        );
        assert!(semantic_outcome.review_only);

        let mut conn = pool.get().unwrap();
        let vector_rows = conn
            .query_one(
                "SELECT count(*),
                        count(*) FILTER (
                            WHERE dimension=512 AND vector_dims(embedding)=512
                        ),
                        count(*) FILTER (
                            WHERE embedding_model_id='bge-small-zh-v1.5'
                              AND embedding_version='0.2.0'
                        )
                 FROM knowledge_ingestion_candidate_vectors
                 WHERE workspace_id=$1",
                &[&workspace],
            )
            .unwrap();
        assert_eq!(vector_rows.get::<_, i64>(0), 2);
        assert_eq!(vector_rows.get::<_, i64>(1), 2);
        assert_eq!(vector_rows.get::<_, i64>(2), 2);
        let candidate_rows: i64 = conn
            .query_one(
                "SELECT count(*) FROM knowledge_ingestion_candidates
                 WHERE ingestion_id=$1 AND candidate_type='nuclei_template'",
                &[&result.ingestion_id],
            )
            .unwrap()
            .get(0);
        assert_eq!(candidate_rows, 2, "semantic scan does not merge candidates");
        drop(conn);

        let mut conn = pool.get().unwrap();
        conn.batch_execute(
            "ALTER TABLE knowledge_ingestion_candidate_similarity_suggestions
             ADD CONSTRAINT rip_test_semantic_insert_failure
             CHECK (cosine_similarity < -1) NOT VALID",
        )
        .unwrap();
        drop(conn);
        let persistence_failure = intel_db::candidate_semantic::scan_candidates(
            &pool,
            &workspace,
            &result.ingestion_id,
            0.70,
            10,
            &embed_config,
        )
        .unwrap_err();
        assert!(matches!(persistence_failure, intel_db::IntelError::Pg(_)));
        let mut conn = pool.get().unwrap();
        let suggestions_after_failure: i64 = conn
            .query_one(
                "SELECT count(*) FROM knowledge_ingestion_candidate_similarity_suggestions
                 WHERE ingestion_id=$1 AND workspace_id=$2",
                &[&result.ingestion_id, &workspace],
            )
            .unwrap()
            .get(0);
        assert_eq!(
            suggestions_after_failure, semantic_outcome.suggestions_persisted as i64,
            "failed suggestion transaction preserves the previous result"
        );
        conn.batch_execute(
            "ALTER TABLE knowledge_ingestion_candidate_similarity_suggestions
             DROP CONSTRAINT rip_test_semantic_insert_failure",
        )
        .unwrap();
        drop(conn);

        let other_candidate: String = {
            let mut conn = pool.get().unwrap();
            conn.query_one(
                "SELECT candidate_id FROM knowledge_ingestion_candidates
                 WHERE ingestion_id=$1 AND candidate_type='nuclei_template'
                   AND candidate_id<>$2 LIMIT 1",
                &[&result.ingestion_id, &nuclei_candidate],
            )
            .unwrap()
            .get(0)
        };
        let first_detail = ingestion::get_candidate_evidence(
            &pool,
            &workspace,
            &result.ingestion_id,
            &nuclei_candidate,
        )
        .unwrap()
        .unwrap();
        let second_detail = ingestion::get_candidate_evidence(
            &pool,
            &workspace,
            &result.ingestion_id,
            &other_candidate,
        )
        .unwrap()
        .unwrap();
        let details = [&first_detail, &second_detail];
        assert!(details.iter().any(|detail| {
            detail["similarity_suggestions"]
                .as_array()
                .is_some_and(|suggestions| !suggestions.is_empty())
        }));
        for suggestion in details
            .iter()
            .flat_map(|detail| detail["similarity_suggestions"].as_array().unwrap())
        {
            assert_eq!(suggestion["review_only"], true);
        }

        let repeated = intel_db::candidate_semantic::scan_candidates(
            &pool,
            &workspace,
            &result.ingestion_id,
            0.70,
            10,
            &embed_config,
        )
        .unwrap();
        assert_eq!(
            repeated.vectors_embedded, 0,
            "candidate vectors are idempotent"
        );
        assert_eq!(
            repeated.suggestions_persisted, semantic_outcome.suggestions_persisted,
            "repeated scan replaces suggestions deterministically"
        );
    } else {
        eprintln!("SKIP candidate semantic integration: pinned image {embed_image} is unavailable");
    }
    assert!(
        ingestion::get_candidate_evidence(&pool, &workspace, "ing-wrong", &cve_candidate)
            .unwrap()
            .is_none()
    );
    let (cve_start, cve_end, matched_text, source_block): (i32, i32, String, String) = {
        let mut conn = pool.get().unwrap();
        let row = conn
            .query_one(
                "SELECT ce.char_start,ce.char_end,ce.matched_text,b.content
             FROM knowledge_ingestion_candidate_evidence ce
             JOIN knowledge_ingestion_evidence e USING(evidence_id)
             JOIN document_blocks b USING(block_id)
             WHERE ce.candidate_id=$1 ORDER BY ce.char_start LIMIT 1",
                &[&cve_candidate],
            )
            .unwrap();
        (row.get(0), row.get(1), row.get(2), row.get(3))
    };
    let expected_char_start = text[..text.find(&cve).unwrap()].chars().count() as i32;
    assert_eq!(
        cve_start, expected_char_start,
        "offsets count Unicode characters"
    );
    let source_chars: Vec<char> = source_block.chars().collect();
    assert_eq!(
        source_chars[cve_start as usize..cve_end as usize]
            .iter()
            .collect::<String>(),
        matched_text,
        "stored character offsets resolve to the original identifier"
    );
    let selector = batch_review::Selector::Ids(vec![cve_candidate.clone()]);
    batch_review::decide(
        &pool,
        batch_id,
        &selector,
        "APPROVED",
        "reviewer-test",
        "reviewed as an identifier mention",
        true,
    )
    .unwrap();
    let applied =
        batch_review::apply_batch(&pool, batch_id, Some(vec![cve_candidate.clone()]), true)
            .unwrap();
    assert_eq!(applied["mode"], "reviewed_candidate_only");
    let mut conn = pool.get().unwrap();
    let reviewed_status: String = conn
        .query_one(
            "SELECT status FROM knowledge_ingestion_candidates WHERE candidate_id=$1",
            &[&cve_candidate],
        )
        .unwrap()
        .get(0);
    assert_eq!(reviewed_status, "REVIEWED");
    drop(conn);
    batch_review::undo(
        &pool,
        batch_id,
        std::slice::from_ref(&cve_candidate),
        "reviewer-test",
        true,
    )
    .unwrap();

    let mut duplicate_request = request;
    duplicate_request.idempotency_key.push_str("-duplicate");
    let duplicate = ingestion::submit_report(&pool, &duplicate_request).unwrap();
    let mut conn = pool.get().unwrap();
    let duplicate_candidates: i64 = conn
        .query_one(
            "SELECT count(*) FROM knowledge_ingestion_candidates
             WHERE ingestion_id=$1 AND status='DUPLICATE_REVIEW'",
            &[&duplicate.ingestion_id],
        )
        .unwrap()
        .get(0);
    assert_eq!(duplicate_candidates, 4);

    let revision_two = document_repo::new_revision(
        &pool,
        &store,
        &imported.document_id,
        format!("source-revision-two-{numeric_tag}").as_bytes(),
        Some("docling 2.150.1"),
        None,
    )
    .unwrap();
    let (mut second_ir, _) = sample_ir(false);
    second_ir
        .blocks
        .iter_mut()
        .find(|block| block.block_type == "paragraph")
        .unwrap()
        .content = format!("修订版补充信息：{cve} 和 CWE-79，描述发生变化。");
    second_ir.blocks.push(invalid_nuclei_block());
    second_ir
        .structure
        .root
        .children
        .push("blk-invalid-nuclei".to_string());
    commit::commit_parse_result(
        &pool,
        &store,
        &imported.document_id,
        &revision_two,
        &second_ir,
        &std::collections::HashMap::new(),
    )
    .unwrap();
    let mut revision_request = duplicate_request;
    revision_request.idempotency_key.push_str("-revision-two");
    revision_request.source = IngestionSource::DocumentRevision {
        document_id: imported.document_id.to_string(),
        revision_id: revision_two.to_string(),
    };
    let revision_result = ingestion::submit_report(&pool, &revision_request).unwrap();
    let revision_payload = revision_result.result.unwrap();
    assert_eq!(
        revision_payload["revision_diff"]["previous_revision_id"],
        imported.revision_id.to_string()
    );
    assert_eq!(
        revision_payload["revision_diff"]["added_block_hashes"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        revision_payload["revision_diff"]["removed_block_hashes"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        revision_payload["revision_diff"]["unchanged_block_count"],
        3
    );
    let mut conn = pool.get().unwrap();
    let version_change_candidates: i64 = conn
        .query_one(
            "SELECT count(*) FROM knowledge_ingestion_candidates
             WHERE ingestion_id=$1 AND status='VERSION_CHANGE_REVIEW'
               AND prior_candidate_id IS NOT NULL",
            &[&revision_result.ingestion_id],
        )
        .unwrap()
        .get(0);
    assert_eq!(version_change_candidates, 2);
    drop(conn);

    let mut risky_input = import_input(
        &format!("risk-ingestion-document-{numeric_tag}"),
        "application/pdf",
        "pdf",
    );
    risky_input.workspace_id = workspace.clone();
    let risky_document = document_repo::import_document(
        &pool,
        &store,
        &risky_input,
        format!("risk-source-{numeric_tag}").as_bytes(),
    )
    .unwrap();
    let risky_cve = format!("CVE-2098-{numeric_tag:08}");
    let (mut risky_ir, _) = sample_ir(true);
    risky_ir
        .blocks
        .iter_mut()
        .find(|block| block.block_type == "paragraph")
        .unwrap()
        .content = format!("Untrusted text mentions {risky_cve}.");
    commit::commit_parse_result(
        &pool,
        &store,
        &risky_document.document_id,
        &risky_document.revision_id,
        &risky_ir,
        &std::collections::HashMap::new(),
    )
    .unwrap();
    let risky_request = KnowledgeIngestionRequest {
        contract_version: CONTRACT_VERSION.to_string(),
        workspace_id: workspace.clone(),
        idempotency_key: format!("risk-document-{numeric_tag}"),
        source: IngestionSource::DocumentRevision {
            document_id: risky_document.document_id.to_string(),
            revision_id: risky_document.revision_id.to_string(),
        },
        requester: "integration-test".to_string(),
        run_id: None,
        trace_id: None,
    };
    let risky_result = ingestion::submit_report(&pool, &risky_request).unwrap();
    let risky_batch_id = risky_result.result.unwrap()["review_batch_id"]
        .as_str()
        .unwrap()
        .to_string();
    let mut conn = pool.get().unwrap();
    let (priority_rank, priority_band, risk_flag): (i32, String, String) = {
        let row = conn
            .query_one(
                "SELECT i.priority_rank,i.priority_band,i.payload->'risk'->'flags'->>0
             FROM review_batch_item i
             WHERE i.batch_id=$1",
                &[&risky_batch_id],
            )
            .unwrap();
        (row.get(0), row.get(1), row.get(2))
    };
    assert_eq!(priority_rank, 1);
    assert_eq!(priority_band, "P1");
    assert_eq!(risk_flag, "POSSIBLE_PROMPT_INJECTION");
    drop(conn);

    let unparsed = document_repo::import_document(
        &pool,
        &store,
        &ImportInput {
            title: format!("unparsed-ingestion-document-{numeric_tag}"),
            original_filename: "unparsed.txt".to_string(),
            mime_type: "text/plain".to_string(),
            document_type: "markdown".to_string(),
            language: None,
            workspace_id: workspace.clone(),
            visibility: domain::DocumentVisibility::Private,
            source_type: "upload".to_string(),
            source_url: String::new(),
            source_commit: None,
        },
        format!("unparsed-source-{numeric_tag}").as_bytes(),
    )
    .unwrap();
    let unparsed_request = KnowledgeIngestionRequest {
        contract_version: CONTRACT_VERSION.to_string(),
        workspace_id: workspace.clone(),
        idempotency_key: format!("unparsed-document-{numeric_tag}"),
        source: IngestionSource::DocumentRevision {
            document_id: unparsed.document_id.to_string(),
            revision_id: unparsed.revision_id.to_string(),
        },
        requester: "integration-test".to_string(),
        run_id: None,
        trace_id: None,
    };
    let error = ingestion::submit_report(&pool, &unparsed_request).unwrap_err();
    assert!(error
        .to_string()
        .contains("must be parsed before ingestion"));
    let mut conn = pool.get().unwrap();
    let (status, error_message): (String, Option<String>) = conn
        .query_one(
            "SELECT status,error_message FROM knowledge_ingestions
             WHERE workspace_id=$1 AND idempotency_key=$2",
            &[&workspace, &unparsed_request.idempotency_key],
        )
        .map(|row| (row.get(0), row.get(1)))
        .unwrap();
    assert_eq!(status, "FAILED");
    assert!(error_message
        .as_deref()
        .unwrap_or_default()
        .contains("parse_state=DISCOVERED"));
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

    let ingestion = intel_db::ingestion::submit_report(
        &pool,
        &intel_db::ingestion::KnowledgeIngestionRequest {
            contract_version: intel_db::ingestion::CONTRACT_VERSION.to_string(),
            workspace_id: "ws-p2".to_string(),
            idempotency_key: format!("container-parse-{}", outcome.revision_id),
            source: intel_db::ingestion::IngestionSource::DocumentRevision {
                document_id: outcome.document_id.to_string(),
                revision_id: outcome.revision_id.to_string(),
            },
            requester: "parser_pipeline_test".to_string(),
            run_id: None,
            trace_id: None,
        },
    )
    .unwrap();
    assert_eq!(ingestion.status, "COMPLETED");
    let result = ingestion.result.unwrap();
    assert_eq!(result["revision_id"], outcome.revision_id.to_string());
    assert_eq!(result["candidates_created"], 0);
}
