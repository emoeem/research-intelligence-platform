//! P4-3 源接入测试（M4.2 判据）：rtis 只读子进程 Adapter（超时与失败语义
//! 实测，设计稿 §13.6 eri_adapter.py 5s 三态回落为参照）与 knowledge 源
//! （SearchEngine::search 直连，不重写 FTS5；治理轴一票否决有拒绝用例）。
//!
//! 真实 kit 用例（TOOL-INVENTORY §8 冒烟口径：`python3 -I rt.py poc search
//! --q kingdee --limit 2 --json`）在 knowledge.db 缺席时自动跳过（CI 无
//! RTIS 数据面，podman 门禁同款纪律）；RTIS 本体零改动（红线）。

use context_broker::{
    classify, vetoes, ContentRef, ContextBrokerError, ContextItemKind, ContextLayer, ContextSource,
    IntentConfig, IntentQuery, KnowledgeSource, RtisSource, RtisStatus,
};
use database::{DbPath, Pool, PoolConfig};
use search_engine::index::SearchIndex;
use search_engine::query::SearchEngine;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root must exist")
}

fn empty_intent() -> IntentQuery {
    IntentQuery {
        text_normalized: String::new(),
        families: Vec::new(),
        knowledge_intents: Vec::new(),
        target_slots: Vec::new(),
        task_verbs: Vec::new(),
        constraints: Vec::new(),
        confidence: 0.0,
    }
}

// ---------------------------------------------------------------------------
// rtis 源：假 kit 载具（超时 / 非零退出 / 坏 JSON / 正常解析）
// ---------------------------------------------------------------------------

const FAKE_RT_PY: &str = r#"
import sys, time, json, pathlib
here = pathlib.Path(__file__).parent
mode = (here / "mode.txt").read_text().strip() if (here / "mode.txt").exists() else "ok"
if mode == "slow":
    with (here / "attempts.txt").open("a") as f:
        f.write("1\n")
    time.sleep(5)
    print(json.dumps({"count": 0, "results": []}))
elif mode == "fail":
    sys.exit(3)
elif mode == "badjson":
    print("not json at all")
else:
    print(json.dumps({"count": 1, "limit": 5, "results": [
        {"id": 42, "title": "fake poc", "description": "fake description", "verified": 1}
    ]}))
"#;

fn make_fake_kit(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("rip-p43-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create fake kit dir");
    std::fs::write(dir.join("rt.py"), FAKE_RT_PY).expect("write fake rt.py");
    dir
}

#[test]
fn rtis_fake_kit_parses_search_results_into_candidates() {
    let kit = make_fake_kit("ok");
    let source = RtisSource::new(kit.clone());
    let (candidates, status) = source.search_with_status("kingdee http", 5);
    assert_eq!(status, RtisStatus::Ok);
    assert_eq!(candidates.len(), 1);
    let candidate = &candidates[0];
    assert_eq!(candidate.item_id, "kb:42");
    assert_eq!(candidate.kind, ContextItemKind::Knowledge);
    assert_eq!(candidate.layer, ContextLayer::L2);
    assert_eq!(candidate.reference.source, "rtis");
    assert_eq!(candidate.reference.ref_id, "kb:42");
    assert_eq!(
        candidate.reference.uri.as_deref(),
        Some("rtis://poc/42"),
        "设计稿 §9.1 引用口径"
    );
    assert!(candidate.est_tokens > 0);

    // load：poc get 全文展开（fake 脚本同款 JSON 即可解析）。
    let content = source
        .load(&ContentRef {
            source: "rtis".to_string(),
            ref_id: "kb:42".to_string(),
            ref_version: None,
            content_hash: None,
            uri: None,
        })
        .expect("load poc 42");
    assert!(content.body.contains("\"id\":42"), "body: {}", content.body);
    assert!(content.est_tokens > 0);

    // 空查询合法空集。
    let (empty, status) = source.search_with_status("   ", 5);
    assert!(empty.is_empty());
    assert_eq!(status, RtisStatus::Ok);

    // ContextSource candidates：经 IntentQuery 归一化文本驱动。
    let candidates = source
        .candidates(
            &IntentQuery {
                text_normalized: "kingdee http 检查".to_string(),
                ..empty_intent()
            },
            5,
        )
        .expect("candidates via trait");
    assert_eq!(candidates.len(), 1);

    // 越权源引用拒绝。
    let foreign = source.load(&ContentRef {
        source: "registry".to_string(),
        ref_id: "kb:42".to_string(),
        ref_version: None,
        content_hash: None,
        uri: None,
    });
    assert!(foreign.is_err());

    let _ = std::fs::remove_dir_all(&kit);
}

#[test]
fn rtis_timeout_retries_once_then_reports_unavailable() {
    let kit = make_fake_kit("slow");
    std::fs::write(kit.join("mode.txt"), "slow").unwrap();
    let source = RtisSource::new(kit.clone()).with_timeout(Duration::from_millis(300));

    let started = Instant::now();
    let (candidates, status) = source.search_with_status("kingdee", 5);
    let elapsed = started.elapsed();

    assert!(
        candidates.is_empty(),
        "不可用态映射空集（设计稿 §9.2 第 4 条）"
    );
    match &status {
        RtisStatus::Unavailable(reason) => {
            assert!(reason.contains("timeout after 0.3"), "reason: {reason}");
        }
        RtisStatus::Ok => panic!("slow kit must not report Ok"),
    }
    // 两次尝试（单次重试，eri_adapter.py 口径）+ 一次 1s 重试间隔，整体有界。
    // 语义断言是恰两次尝试（上一行）；时间上界取宽值防并行负载抖动。
    assert!(elapsed < Duration::from_secs(10), "elapsed: {elapsed:?}");
    let attempts = std::fs::read_to_string(kit.join("attempts.txt")).unwrap();
    assert_eq!(
        attempts.lines().count(),
        2,
        "fail single retry: exactly two attempts (eri_adapter.py:23 口径)"
    );
    let _ = std::fs::remove_dir_all(&kit);
}

#[test]
fn rtis_exit_failure_and_bad_json_are_unavailable_after_retry() {
    let kit = make_fake_kit("fail");
    std::fs::write(kit.join("mode.txt"), "fail").unwrap();
    let source = RtisSource::new(kit.clone()).with_timeout(Duration::from_millis(500));
    let (candidates, status) = source.search_with_status("kingdee", 5);
    assert!(candidates.is_empty());
    match &status {
        RtisStatus::Unavailable(reason) => {
            assert!(
                reason.contains("exited with exit status: 3"),
                "reason: {reason}"
            );
            assert!(reason.contains("2 attempts"), "reason: {reason}");
        }
        RtisStatus::Ok => panic!("failing kit must not report Ok"),
    }

    std::fs::write(kit.join("mode.txt"), "badjson").unwrap();
    let (candidates, status) = source.search_with_status("kingdee", 5);
    assert!(candidates.is_empty());
    assert!(matches!(status, RtisStatus::Unavailable(ref r) if r.contains("not valid JSON")));
    let _ = std::fs::remove_dir_all(&kit);
}

#[test]
fn rtis_load_rejects_bad_ref_ids_and_foreign_sources() {
    let kit = make_fake_kit("badref");
    let source = RtisSource::new(kit.clone());
    let bad_ref = source.load(&ContentRef {
        source: "rtis".to_string(),
        ref_id: "not-an-id".to_string(),
        ref_version: None,
        content_hash: None,
        uri: None,
    });
    let err: ContextBrokerError = bad_ref.expect_err("non kb:<id> ref must be refused");
    assert!(err.to_string().contains("kb:<id>"), "err: {err}");
    let _ = std::fs::remove_dir_all(&kit);
}

// ---------------------------------------------------------------------------
// rtis 真实 kit（M4.2 判据：kingdee 只读命中；knowledge.db 缺席时跳过）
// ---------------------------------------------------------------------------

#[test]
fn rtis_real_kit_returns_readonly_hits_for_kingdee_task() {
    let kit = repo_root().join("integrations/rtis");
    if !kit.join("knowledge.db").exists() {
        println!("skip: RTIS knowledge.db absent (CI 口径，podman 门禁同款纪律)");
        return;
    }
    let source = RtisSource::new(kit.clone());
    let (candidates, status) = source.search_with_status("kingdee", 2);
    assert_eq!(status, RtisStatus::Ok);
    assert!(
        !candidates.is_empty(),
        "M4.2 判据：给定 kingdee 类任务 rtis 源返回只读命中"
    );
    let first = &candidates[0];
    assert_eq!(first.reference.source, "rtis");
    assert!(
        first.reference.ref_id.starts_with("kb:"),
        "ref_id: {}",
        first.reference.ref_id
    );
    assert!(first.est_tokens > 0);

    // L3 展开：poc get 同链路只读。
    let content = source.load(&first.reference).expect("load real poc");
    assert!(content.body.contains("\"id\""), "body head ok");
    assert!(content.est_tokens > 0);

    // ContextSource 全链：意图归一化文本直送（「kingdee HTTP 检查」类任务）。
    let config = IntentConfig::from_dir(&repo_root().join("rulepacks/rip-rules")).unwrap();
    let intent = classify("kingdee HTTP 检查", &config);
    let via_trait = source.candidates(&intent.query, 2).expect("candidates");
    assert!(!via_trait.is_empty(), "trait 面同样命中（M4.2 判据全链）");
}

// ---------------------------------------------------------------------------
// knowledge 源：索引夹具（SearchEngine::search 直连 + 治理一票否决拒绝用例）
// ---------------------------------------------------------------------------

fn knowledge_fixture() -> (Pool, Pool, Pool) {
    let platform = Pool::open(PoolConfig {
        path: DbPath::Memory,
        readonly: false,
        max_connections: 1,
        busy_timeout: Duration::from_secs(5),
    })
    .unwrap();
    platform
        .with_conn(|c| {
            c.execute_batch(
                "CREATE TABLE resource (
                   resource_id TEXT PRIMARY KEY, canonical_url TEXT NOT NULL,
                   resource_type TEXT NOT NULL, name TEXT NOT NULL, description TEXT,
                   lifecycle TEXT NOT NULL, governance TEXT NOT NULL,
                   quality_final_score REAL, quality_grade TEXT);
                 CREATE TABLE resource_source (
                   id INTEGER PRIMARY KEY, resource_id TEXT NOT NULL,
                   source_repository TEXT NOT NULL, source_file TEXT NOT NULL,
                   source_section TEXT NOT NULL, source_position INTEGER);
                 INSERT INTO resource VALUES
                   ('gh:a/kingdee-tool', 'https://github.com/a/kingdee-tool', 'github_repo',
                    'Kingdee Tool', 'kingdee http inspection utility', 'INDEXED', 'ACTIVE',
                    80.0, 'B'),
                   ('gh:b/normal-tool', 'https://github.com/b/normal-tool', 'github_repo',
                    'Normal Tool', 'plain keyword scanner', 'INDEXED', 'ACTIVE', 70.0, 'C'),
                   ('gh:c/hidden-tool', 'https://github.com/c/hidden', 'github_repo',
                    'Hidden Tool', 'kingdee fuzzing but hidden', 'INDEXED', 'QUARANTINED',
                    95.0, 'A');",
            )
        })
        .unwrap();
    let catalog = Pool::open(PoolConfig {
        path: DbPath::Memory,
        readonly: false,
        max_connections: 1,
        busy_timeout: Duration::from_secs(5),
    })
    .unwrap();
    let index = Pool::open(PoolConfig {
        path: DbPath::Memory,
        readonly: false,
        max_connections: 1,
        busy_timeout: Duration::from_secs(5),
    })
    .unwrap();
    (platform, catalog, index)
}

#[test]
fn knowledge_source_searches_via_search_engine_and_hides_quarantined() {
    let (platform, catalog, index) = knowledge_fixture();
    SearchIndex::build(&platform, &catalog, &index).expect("index build");
    let source = KnowledgeSource::new(SearchEngine::new(index));

    let keywords = vec!["kingdee".to_string()];
    let candidates = source.search(&keywords, 10).expect("search hits");
    assert!(!candidates.is_empty(), "kingdee 命中 ACTIVE 文档");
    for candidate in &candidates {
        assert_eq!(candidate.kind, ContextItemKind::Knowledge);
        assert_eq!(candidate.reference.source, "platform-kb");
        assert_ne!(
            candidate.item_id, "gh:c/hidden-tool",
            "QUARANTINED 文档由 search-engine 查询面同款域约束兜底（query.rs:268）"
        );
    }
    assert!(
        candidates.iter().any(|c| c.item_id == "gh:a/kingdee-tool"),
        "hits: {:?}",
        candidates.iter().map(|c| &c.item_id).collect::<Vec<_>>()
    );
    // 质量等级随候选携带（打分输入，P4-4 装配消费）。
    let hit = candidates
        .iter()
        .find(|c| c.item_id == "gh:a/kingdee-tool")
        .unwrap();
    assert_eq!(hit.quality_grade.as_deref(), Some("B"));
    assert!(
        hit.reference
            .uri
            .as_deref()
            .unwrap_or("")
            .starts_with("https://"),
        "canonical_url 进引用 uri"
    );

    // 治理轴一票否决（score 层拒绝用例；引擎面已过滤，双层把守）。
    assert!(vetoes(Some("QUARANTINED")).is_some());
    assert!(vetoes(Some("REJECTED")).is_some());
    assert!(vetoes(Some("ARCHIVED")).is_some());

    // ContextSource candidates：意图归一化分词直送 SearchEngine::search。
    let config = IntentConfig::from_dir(&repo_root().join("rulepacks/rip-rules")).unwrap();
    let intent = classify("kingdee http 检索", &config);
    let via_trait = source.candidates(&intent.query, 10).expect("candidates");
    assert!(
        via_trait.iter().all(|c| c.item_id != "gh:c/hidden-tool"),
        "trait 面同样不见被治理隐藏的文档"
    );

    // L3 load：显式拒绝（D-13：Phase 4 数据面无正文载荷列，随 Phase 7
    // 知识正文存储定义；索引库只承载元数据）。
    let load = source.load(&ContentRef {
        source: "platform-kb".to_string(),
        ref_id: "gh:a/kingdee-tool".to_string(),
        ref_version: None,
        content_hash: None,
        uri: None,
    });
    let err: ContextBrokerError = load.expect_err("platform-kb load refused (D-13)");
    let msg = err.to_string();
    assert!(
        msg.contains("no body payload column") && msg.contains("D-13"),
        "err: {msg}"
    );
}
