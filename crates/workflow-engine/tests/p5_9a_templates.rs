//! P5-9a 十三步 Workflow 模板钉值（Phase 5 计划 §6.1 / §6.2）。
//!
//! 两份模板（通用渗透 wf-pentest-research-v1 / SRC 漏洞挖掘 wf-src-bughunt-v1，
//! workflows/ 目录 JSON 定义惯例）随本测试进 CI：
//! - 装载门（parse + Schema 门）+ 静态校验（环/孤立/必填参数/边端点）零 issue；
//! - 十三步逐条节点 id 钉值（映射对账见 docs/rip3/batches/P5-9a-BATCH-REPORT.md）；
//! - tool_id 全集钉值（检测面与受控验证面分离、低并发与请求预算参数钉住）；
//! - Finding 与报告事实面仅经 human 停等节点承载（RTIS 零直写红线：
//!   模板无 rtis 直写工具节点，finding 回填走 rp-verify finding-link /
//!   rt.py poc update 既有接口，由人工在 human 节点放行时执行）；
//! - 非执行面实例化：临时库 put + get 往返 + 版本面（017 控制面）。
//!
//! 文件经 CARGO_MANIFEST_DIR 锚定仓库 workflows/ 目录（p8_3_control.rs 先例，
//! cargo test CWD 为 crate 目录）。

use std::path::{Path, PathBuf};
use std::time::Duration;

use database::{migrate, DbPath, Pool, PoolConfig};
use workflow_engine::dsl::{parse_definition, to_domain, DefinitionV1};
use workflow_engine::store::WorkflowStore;
use workflow_engine::validate::validate_definition;

const PENTEST_PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../workflows/pentest-research-v1.json"
);
const SRC_PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../workflows/src-bughunt-v1.json"
);
const MIGRATIONS_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../migrations");

fn load(path: &str) -> DefinitionV1 {
    let raw = std::fs::read_to_string(path).expect("template file readable");
    parse_definition(&raw).expect("template passes parse + schema gate")
}

fn assert_valid(doc: &DefinitionV1) {
    let def = to_domain(doc).expect("template converts to domain graph");
    let issues = validate_definition(&def);
    assert!(
        issues.is_empty(),
        "静态校验须零 issue，实际：{:?}",
        issues.iter().map(|i| i.message.clone()).collect::<Vec<_>>()
    );
}

/// 十三步映射节点 id 全集钉值（多节点承载一步属正常：第 7 步分析三节点、
/// 第 12 步查重三节点；start/end 为图骨架不计步）。
fn assert_node_ids(doc: &DefinitionV1, expected: &[&str]) {
    let mut actual: Vec<String> = doc.graph.nodes.iter().map(|n| n.id.clone()).collect();
    let mut expect: Vec<String> = expected.iter().map(|s| s.to_string()).collect();
    actual.sort();
    expect.sort();
    assert_eq!(actual, expect, "节点 id 全集须与十三步映射钉值一致");
}

/// input_schema 形状钉值：object + additionalProperties false + required 非空
/// （run 输入白名单面，egress host 与模板锚由 run 输入承载）。
fn assert_input_schema(doc: &DefinitionV1) {
    let schema = doc
        .input_schema
        .as_ref()
        .expect("模板必须声明 input_schema（run 受理门）");
    assert_eq!(schema["type"], "object");
    assert_eq!(schema["additionalProperties"], false);
    let required = schema["required"].as_array().expect("required array");
    assert!(!required.is_empty(), "required 键不得为空");
}

/// tool_id 全集钉值 + RTIS 零直写面：tool 节点参数键只允许 tool_id/input，
/// tool_id 与既有 manifests 词一致；模板无任何 rtis 写面工具。
fn assert_tool_face(doc: &DefinitionV1, expected_tool_ids: &[&str]) {
    let mut actual: Vec<String> = Vec::new();
    for node in &doc.graph.nodes {
        if doc.graph.nodes.is_empty() {
            break;
        }
        let params = match &node.params {
            Some(p) => p,
            None => continue,
        };
        if node.node_type != workflow_engine::dsl::NodeTypeV1::Tool {
            continue;
        }
        let keys: Vec<&String> = params.keys().collect();
        assert!(
            keys.iter()
                .all(|k| k.as_str() == "tool_id" || k.as_str() == "input"),
            "tool 节点 {} 参数键越表（P8-4 表单与 DSL 口径）：{:?}",
            node.id,
            keys
        );
        actual.push(
            params
                .get("tool_id")
                .and_then(|v| v.as_str())
                .expect("tool_id str")
                .to_string(),
        );
    }
    let mut expect: Vec<String> = expected_tool_ids.iter().map(|s| s.to_string()).collect();
    actual.sort();
    expect.sort();
    assert_eq!(actual, expect, "tool_id 全集须钉值（禁 rtis 直写面混入）");
    for tool_id in &actual {
        assert!(
            !tool_id.contains("rtis.write") && !tool_id.contains("rtis_poc_add"),
            "模板出现 rtis 直写工具 {tool_id}：违反 RTIS 红线"
        );
    }
}

#[test]
fn pentest_template_thirteen_steps_valid() {
    let doc = load(PENTEST_PATH);
    assert_eq!(doc.workflow_id, "wf-pentest-research-v1");
    assert_eq!(doc.version, 1);
    assert_eq!(doc.compat.as_str(), "graph-v1");
    assert_valid(&doc);
    assert_input_schema(&doc);
    assert_node_ids(
        &doc,
        &[
            // 骨架
            "start",
            "done",
            // 步 1 目标范围确认（human 停等，授权审批不内置）
            "scope_confirm",
            // 步 2+3 资产清点与技术栈识别（parallel 扇出收敛）
            "recon_fan",
            "asset_inventory",
            "tech_fingerprint",
            // 步 4 知识检索
            "kb_context",
            // 步 5 威胁建模
            "threat_model",
            // 步 6 特征检测
            "feature_detection",
            // 步 7 人工或 Agent 分析（agent + condition + human 复核分支）
            "deep_analysis",
            "escalation_gate",
            "analyst_review",
            // 步 8 受控漏洞验证（与检测面分离）
            "controlled_verification",
            // 步 9 Evidence 收集
            "evidence_collect",
            // 步 10 Finding 创建或关联（human 停等，经 RTIS 既有接口）
            "finding_link",
            // 步 11 风险评价
            "risk_rating",
            // 步 12 修复建议
            "remediation",
            // 步 13 报告生成
            "report_generate",
        ],
    );
    assert_tool_face(
        &doc,
        &[
            "rip.kb.inventory",
            "rip.http.tls_inspect",
            "rip.security.nuclei",
            "rip.security.nuclei",
            "rip.evidence.rtk.ledger",
            "rip.evidence.rtk.lint",
        ],
    );
    // Finding 与报告事实面仅 human 停等节点（finding_link），无 rtis 直写节点
    let mut human_ids: Vec<&str> = doc
        .graph
        .nodes
        .iter()
        .filter(|n| n.node_type == workflow_engine::dsl::NodeTypeV1::Human)
        .map(|n| n.id.as_str())
        .collect();
    human_ids.sort_unstable();
    assert_eq!(
        human_ids,
        vec!["analyst_review", "finding_link", "scope_confirm"]
    );
}

#[test]
fn src_template_thirteen_steps_valid() {
    let doc = load(SRC_PATH);
    assert_eq!(doc.workflow_id, "wf-src-bughunt-v1");
    assert_eq!(doc.version, 1);
    assert_eq!(doc.compat.as_str(), "graph-v1");
    assert_valid(&doc);
    assert_input_schema(&doc);
    assert_node_ids(
        &doc,
        &[
            // 骨架
            "start",
            "done",
            // 步 1 收集目标公开资料（Registry approved 闸默认开）
            "public_recon",
            // 步 2 识别资产与业务功能
            "asset_function_map",
            // 步 3 分析技术栈
            "stack_fingerprint",
            // 步 4 检索相似漏洞案例
            "case_search",
            // 步 5 建立测试覆盖矩阵（七维五态口径，rp-verify item-add 承载）
            "coverage_matrix",
            // 步 6 执行低影响检测
            "low_impact_scan",
            // 步 7 分析业务逻辑与权限边界
            "authz_boundary_analysis",
            // 步 8 建立安全假设（盲区假设草稿口径）
            "hypothesis_draft",
            // 步 9 受控验证（与检测面分离）
            "controlled_verify",
            // 步 10 记录证据
            "evidence_record",
            // 步 11 评估影响
            "impact_assessment",
            // 步 12 查重（tool 检索 + condition 分流 + human 核对三节点承载）
            "dedup_search",
            "dedup_gate",
            "dedup_review",
            // 步 13 生成 SRC 报告
            "src_report_lint",
        ],
    );
    assert_tool_face(
        &doc,
        &[
            "rip.knowledge.document.fetch",
            "rip.kb.inventory",
            "rip.http.tls_inspect",
            "rip.kb.rtis",
            "rip.security.nuclei",
            "rip.security.nuclei",
            "rip.evidence.rtk.ledger",
            "rip.kb.rtis",
            "rip.evidence.rtk.lint",
        ],
    );
}

/// 非执行面实例化：两模板 put 入临时迁移库（装载门 + 静态校验门再跑一遍）
/// → get 往返回读逐字段一致 → list 行在（workflows/ 模板经 put 即可被
/// WorkflowsPage /api/workflows 列表面选用）。
#[test]
fn templates_instantiate_into_store_non_exec_face() {
    let dir = std::env::temp_dir().join(format!("rip-wf-p59a-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let path: PathBuf = dir.join("p59a.db");
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(dir.join("p59a.db-wal"));

    let pool = Pool::open(PoolConfig {
        path: DbPath::File(path.clone()),
        readonly: false,
        max_connections: 1,
        busy_timeout: Duration::from_secs(5),
    })
    .expect("open pool");
    let applied = migrate(&pool, Path::new(MIGRATIONS_DIR)).expect("migrate");
    assert!(
        applied.iter().any(|m| m.version == 17),
        "017 控制面迁移必须在本次迁移中应用"
    );

    let store = WorkflowStore::new(pool);
    let pentest = load(PENTEST_PATH);
    let src = load(SRC_PATH);

    let put_pentest = store
        .put(&pentest, "p5-9a-templates-test")
        .expect("put pentest");
    assert_eq!(put_pentest.workflow_id, "wf-pentest-research-v1");
    assert_eq!(put_pentest.version, 1);
    assert_eq!(put_pentest.node_count, 18);
    assert_eq!(put_pentest.edge_count, 19);

    let put_src = store.put(&src, "p5-9a-templates-test").expect("put src");
    assert_eq!(put_src.workflow_id, "wf-src-bughunt-v1");
    assert_eq!(put_src.version, 1);
    assert_eq!(put_src.node_count, 17);
    assert_eq!(put_src.edge_count, 17);

    let got = store
        .get("wf-pentest-research-v1", None)
        .expect("get pentest");
    let roundtrip: DefinitionV1 =
        serde_json::from_str(&got.definition_json).expect("stored doc parses back");
    assert_eq!(roundtrip, pentest, "put/get 往返须逐字段一致");
    // 版本面：put 同 id 新版追加（现有最大版本 + 1）
    let mut v2 = pentest.clone();
    v2.version = 2;
    let put_v2 = store
        .put(&v2, "p5-9a-templates-test")
        .expect("put pentest v2");
    assert_eq!(put_v2.version, 2, "版本面：每版追加");
    let got_v2 = store
        .get("wf-pentest-research-v1", None)
        .expect("get latest");
    assert_eq!(got_v2.definition.version(), 2);

    let rows = store.list().expect("list");
    let ids: Vec<&str> = rows.iter().map(|r| r.workflow_id.as_str()).collect();
    assert!(ids.contains(&"wf-pentest-research-v1"));
    assert!(ids.contains(&"wf-src-bughunt-v1"));
}
