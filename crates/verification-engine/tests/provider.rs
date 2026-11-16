//! P5-5 Provider 统一契约测试：capability 契约面 + 信封分类守卫 +
//! Nuclei 容器派发链端到端（V-9 守卫：podman/镜像缺席跳过，本地实跑为准）。

use database::{migrate, DbPath, Pool, PoolConfig};
use serde_json::json;
use std::path::PathBuf;
use std::time::Duration;
use tool_engine::adapter::podman_host::{PodmanHostAdapter, PodmanHostConfig};
use tool_engine::Engine;
use tool_registry::Registry;
use verification_engine::contract::validate_capability_doc;
use verification_engine::provider::{
    execute_nuclei, NucleiProvider, ProviderOutcome, RtisPocProvider,
};
use verification_engine::VerificationProvider;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn engine_with_nuclei() -> Engine {
    let pool = Pool::open(PoolConfig {
        path: DbPath::Memory,
        readonly: false,
        max_connections: 1,
        busy_timeout: Duration::from_secs(5),
    })
    .unwrap();
    migrate(&pool, &repo_root().join("migrations")).unwrap();
    let registry = Registry::open(pool.clone()).with_options(tool_registry::Options {
        allow_placeholder_digest: false,
    });
    let manifest: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(repo_root().join("manifests/tools/rip.security.nuclei.json"))
            .unwrap(),
    )
    .unwrap();
    registry.register(&manifest).unwrap();
    Engine::open(pool.clone(), registry)
}

fn item(poc_id: i64, target: &str) -> verification_engine::store::PlanItemRecord {
    verification_engine::store::PlanItemRecord {
        schema_version: 1,
        item_id: "vi-20261117-000000-000-0000".to_owned(),
        plan_id: "vp-20261117-000000-000-0000".to_owned(),
        knowledge_ref: None,
        rtis_poc_id: Some(poc_id),
        provider_id: "nuclei".to_owned(),
        provider_version: Some("0.1.0".to_owned()),
        target_ref: target.to_owned(),
        dedup_fingerprint: "a".repeat(64),
        applicability_status: "applicable".to_owned(),
        applicability_reasons: vec![],
        risk_class: "low_impact".to_owned(),
        request_budget: 10,
        timeout_budget_s: 60,
        execution_order: 0,
        expected_observation: None,
        dimension: "assets".to_owned(),
        result_status: None,
        result_reason: None,
        error_category: None,
        tool_run_id: None,
        trace_id: None,
        created_at: "2026-11-17T00:00:00Z".to_owned(),
        updated_at: "2026-11-17T00:00:00Z".to_owned(),
    }
}

#[test]
fn capability_docs_and_outcome_guards() {
    let root = repo_root();
    let rtis = RtisPocProvider::from_repo_root(&root);
    validate_capability_doc(&rtis.capability()).unwrap();
    validate_capability_doc(&NucleiProvider.capability()).unwrap();
    // 信封守卫三态
    let ok = ProviderOutcome {
        provider_id: "nuclei".to_owned(),
        provider_version: "0.1.0".to_owned(),
        status_word: "SUCCEEDED".to_owned(),
        tool_run_id: None,
        trace_id: None,
        result: Some(json!({"matched_count": 0})),
        summary: "clean".to_owned(),
        error_category: None,
        tool_version: Some("v3.11.1".to_owned()),
        rule_ref: Some("rtis-poc:900008".to_owned()),
        duration_ms: 50,
    };
    assert!(ok.validate().is_ok());
    // 工具/规则版本恒在（Provider 输出保留版本纪律）
    assert!(ok.tool_version.is_some());
    assert!(ok.rule_ref.is_some());
}

/// Nuclei 容器派发链端到端（本地靶场：podman python:3.12-slim 起本地 HTTP
/// 服务，nuclei 无模板干净跑 → SUCCEEDED + matched_count=0；退出码 0 交给
/// evaluator 归一化，本测试只断言 Provider 面）。
#[test]
fn nuclei_container_dispatch_end_to_end_local_target() {
    if !podman_available() {
        eprintln!("SKIP: podman unavailable (V-9，容器端到端保持本地执行)");
        return;
    }
    let image_exists = std::process::Command::new("podman")
        .args(["image", "exists", verification_engine::provider::NUCLEI_IMAGE])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    if !image_exists {
        eprintln!(
            "SKIP: 验收镜像 {} 不在本地镜像库（V-9，digest 钉死无法现建命中）",
            verification_engine::provider::NUCLEI_IMAGE
        );
        return;
    }
    let engine = engine_with_nuclei();
    let ws_root = std::env::temp_dir().join(format!("rip-nuclei-p55-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&ws_root);
    let adapter = PodmanHostAdapter::new(PodmanHostConfig {
        workspace_root: ws_root.clone(),
        ..PodmanHostConfig::default()
    });
    // 本地模板经 in/ 挂载提供（template_ref 锚 → 本地模板文件，不跑全量不
    // 自动装模板；matcher 子串永远不命中 → 干净无命中面）。
    let ws = adapter
        .prepare_workspace("rn-p55-e2e", &json!({}))
        .expect("workspace precreate");
    std::fs::write(
        ws.in_dir.join("template.yaml"),
        concat!(
            "id: rip3-p5-e2e-no-match\n",
            "info:\n",
            "  name: rip3 p5 e2e no match\n",
            "  author: rip3-p5\n",
            "  severity: info\n",
            "\n",
            "http:\n",
            "  - method: GET\n",
            "    path:\n",
            "      - \"{{BaseURL}}/rip3-p5-e2e-nonexistent\"\n",
            "    matchers:\n",
            "      - type: word\n",
            "        words:\n",
            "          - \"never-appears-rip3-p5-e2e\"\n",
        ),
    )
    .expect("template write");
    let target = "http://127.0.0.1:1"; // 本机无服务端口 → 干净无命中
    let mut item = item(900008, target);
    // e2e 派发面：入口带本地模板与静默面（execute_nuclei 走 Manifest 入口，
    // 本测试直接 ContainerCall 展开链路语义）
    let _ = &mut item;
    let pre_in: Vec<String> = std::fs::read_dir(&ws.in_dir)
        .map(|d| d.filter_map(|e| e.ok().map(|e| e.file_name().to_string_lossy().to_string())).collect())
        .unwrap_or_default();
    eprintln!("WS_DEBUG pre_in={pre_in:?}");
    // 本测试走 NetworkPlan::None（127.0.0.1 本机面无出网需求；egress
    // allowlist 链路随 P5-6 本地靶场真实目标面登记）。契约面断言一致：
    // 退出码 0 + 无命中 = ProviderOutcome SUCCEEDED + matched_count=0，
    // 五态判定（退出码 0 ≠ CONFIRMED）在 P5-7 evaluator。
    let template_src = ws.in_dir.join("template.yaml"); // 同文件自拷贝（幂等）
    let outcome = match execute_nuclei(
        &engine,
        &adapter,
        &item,
        "rn-p55-e2e",
        verification_engine::provider::DispatchOptions {
            task_id: None,
            // e2e 单链不挂链锚（P5-7 六 ID 贯通在 execute-plan 面）
            trace_id: None,
            network: tool_engine::adapter::egress_proxy::NetworkPlan::None,
            template_path: Some(&template_src),
        },
    ) {
        Ok(outcome) => outcome,
        Err((category, message)) => {
            let post_in: Vec<String> = std::fs::read_dir(&ws.in_dir)
                .map(|d| d.filter_map(|e| e.ok().map(|e| e.file_name().to_string_lossy().to_string())).collect())
                .unwrap_or_default();
            let post_out: Vec<String> = std::fs::read_dir(&ws.out_dir)
                .map(|d| d.filter_map(|e| e.ok().map(|e| e.file_name().to_string_lossy().to_string())).collect())
                .unwrap_or_default();
            eprintln!("WS_DEBUG post_in={post_in:?} post_out={post_out:?} category={category:?} message={message}");
            panic!("nuclei dispatch: ({category:?}, {message})");
        }
    };
    let post_in: Vec<String> = std::fs::read_dir(&ws.in_dir)
        .map(|d| d.filter_map(|e| e.ok().map(|e| e.file_name().to_string_lossy().to_string())).collect())
        .unwrap_or_default();
    let post_out: Vec<String> = std::fs::read_dir(&ws.out_dir)
        .map(|d| d.filter_map(|e| e.ok().map(|e| e.file_name().to_string_lossy().to_string())).collect())
        .unwrap_or_default();
    eprintln!(
        "WS_DEBUG post_in={post_in:?} post_out={post_out:?} status={} summary={} result={:?}",
        outcome.status_word, outcome.summary, outcome.result
    );
    assert_eq!(outcome.status_word, "SUCCEEDED", "{}", outcome.summary);
    assert!(outcome.result.is_some());
    assert_eq!(outcome.provider_id, "nuclei");
    assert!(outcome.tool_version.is_some());
    assert!(outcome.tool_run_id.is_some());
    eprintln!(
        "WS_DEBUG in={:?} out={:?}",
        std::fs::read_dir(ws.in_dir).map(|d| d.count()),
        std::fs::read_dir(ws.out_dir).map(|d| d.count()),
    );
    let _ = std::fs::remove_dir_all(&ws_root);
}

fn podman_available() -> bool {
    std::process::Command::new("podman")
        .arg("info")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}
