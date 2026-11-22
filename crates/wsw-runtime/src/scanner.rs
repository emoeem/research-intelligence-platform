//! WSW-7 Scanner Adapter 统一入口（docs/rip3/wsw/WSW-7-TASK.md D1/D3/D7/D8）。
//!
//! 三 provider 借力接线（PLAN :310「复用现有 Nuclei、ZAP、RTIS POC Adapter
//! 和 Tool Engine」——零第二派发实现，:311 不新建第二套扫描器注册表）：
//! - nuclei = verification-engine execute_nuclei 既有链直接编排（D1①：
//!   submit_correlated → podman_host 容器派发 → ProviderOutcome，本批零
//!   provider 改动——PlanItemRecord 合成承载 target/budget，build_input 原样消费）；
//! - zap = execute_zap（D1②：S4 新增 provider，execute_nuclei :241 同型派发
//!   链；容器内 zap.sh 内联扫描 + 镜像内 python3 解析 JSON 报告，零 daemon）；
//! - rtis = RtisPocProvider poc get/use 两接口委派（D1③：rt.py subprocess
//!   零新集成面；**零接触负例硬门**——poc update 不在本批、poc add 零词形，
//!   REDLINES 红线 3「漏洞事实只走 rt.py / RTIS MCP，零直写零复制」）。
//!
//! 五态投影 = [`verification_engine::evaluator::evaluate`] 复用（五态判定
//! 唯一路径，DI-3 :190「禁第二决策实现」词形；命中 ≠ CONFIRMED——退出码
//! 0 + 命中面 INCONCLUSIVE，A9）。
//!
//! provider 契约面投影 = manifest json 直读防漂移 + plugin-registry catalog
//! 只读计数（D8：只读投影零新插件包，:761 禁第二套注册系统）。
//!
//! 红线（判据 7/8）：本模块零 `Command::new("podman")` 词形（容器面唯一
//! 入口 = PodmanHostAdapter execute_container_via_engine 既有链）；零
//! Finding/evidence 写面；scope.rs/sender.rs 零触碰（扫描目标边界经 Manifest
//! egress_allowlist 与 NetworkPlan 既有面承载，:313/:762 反证面）。

use std::path::Path;

use serde_json::{json, Value};
use tool_engine::adapter::egress_proxy::NetworkPlan;
use tool_engine::adapter::podman_host::PodmanHostAdapter;
use tool_engine::Engine;
use verification_engine::evaluator::{evaluate, NormalizedResult, OutcomeView};
use verification_engine::provider::{
    execute_nuclei, execute_zap, DispatchOptions, ProviderOutcome, RtisPocProvider,
    NUCLEI_EGRESS_PORTS, ZAP_EGRESS_PORTS,
};
use verification_engine::store::PlanItemRecord;

/// provider 词表（三面固定；词表外 rp_server 400 拒绝——D7②投影同源）。
pub const SCAN_PROVIDERS: [&str; 3] = ["nuclei", "zap", "rtis"];

/// rtis provider 元数据（ri.kb.rtis Manifest 直读面投影用的词形锚——RTIS
/// 走 RtisPocProvider cli_subprocess，非 tool-engine 派发面，D2⑤）。
pub const RTIS_TOOL_ID: &str = "rip.kb.rtis";

/// 扫描派发失败词值面（ErrorCategory 词值 + 结构化消息；domain 零依赖纪律
/// ——跨信封出站用词值，判定面 parse 还原，provider.rs 同款）。
#[derive(Debug, Clone)]
pub struct ScanError {
    /// §5.9 分类词值（tool_unavailable/timeout/…；ErrorCategory::as_str）。
    pub category: String,
    pub message: String,
}

// ---------------------------------------------------------------------------
// nuclei 统一入口（D1①：既有链直接编排，PlanItemRecord 合成）
// ---------------------------------------------------------------------------

/// nuclei 扫描参数（CLI/api 装配面；rate 与 max_requests 同钮——P5 request_budget
/// 单钮语义：build_input rate_limit=max(1,min(10,budget))、-rate-limit 旗标 =
/// clamp(budget,1,50)、max_requests=budget 同源派生；同发时 max_requests 优先）。
#[derive(Debug, Clone)]
pub struct NucleiScanParams {
    pub target: String,
    /// 模板引用锚（rtis-poc:<id>；None = 无模板引用面；执行面零模板时干净跑）。
    pub template_ref: Option<String>,
    /// 调用面本地模板文件（--template-file；execute_nuclei 落位 ws in/
    /// template.yaml + -t 旗标装载——P5-9b 修复面复用）。
    pub template_file: Option<std::path::PathBuf>,
    /// 请求预算（max_requests；>1000 由 Manifest input_schema 结构化拒绝——
    /// BudgetExceeds/InputSchemaViolation VALIDATION 词形，判据 6 负例）。
    pub max_requests: i64,
    pub timeout_s: i64,
}

/// Nuclei 真实扫描统一入口（execute_nuclei 既有链消费——零 provider 改动）。
///
/// 装配面：PlanItemRecord 合成（target_ref/template_ref/request_budget/
/// timeout_budget_s 四字段落 CLI 词形，其余字段为 record 形状占位——不落
/// research_plan 表，rp-verify plan 通道不适用）；network 恒 EgressAllowlist
/// （Manifest network.mode 同面，host 从 input.url 派生——判据 7）。
pub fn scan_nuclei(
    engine: &Engine,
    adapter: &PodmanHostAdapter,
    params: &NucleiScanParams,
    run_id: &str,
    task_id: Option<&str>,
    trace_id: Option<&str>,
) -> Result<ProviderOutcome, ScanError> {
    let item = synthetic_plan_item(
        params.target.as_str(),
        params.template_ref.as_deref(),
        params.max_requests,
        params.timeout_s,
    );
    execute_nuclei(
        engine,
        adapter,
        &item,
        run_id,
        DispatchOptions {
            task_id,
            trace_id,
            network: NetworkPlan::EgressAllowlist {
                ports: NUCLEI_EGRESS_PORTS.to_vec(),
            },
            template_path: params.template_file.as_deref(),
        },
    )
    .map_err(|(category, message)| ScanError {
        category: category.as_str().to_owned(),
        message,
    })
}

// ---------------------------------------------------------------------------
// zap 统一入口（D1②：execute_zap S4 provider 同型派发链）
// ---------------------------------------------------------------------------

/// ZAP 扫描参数（CLI/api 装配面；scan_mode 词表单值 baseline——深扫扩展
/// 归观察池；timeout/alert_budget 装配面词形）。
#[derive(Debug, Clone)]
pub struct ZapScanParams {
    pub target: String,
    pub timeout_s: i64,
    pub alert_budget: i64,
}

/// ZAP 基线扫描统一入口（scan_mode 词表单值 baseline——深扫扩展归观察池）。
///
/// network 恒 EgressAllowlist（Manifest network.mode 同面；proxy env 注入 +
/// wrapper 显式旗标承载——D1② 核实项 2 实拍词形）。
pub fn scan_zap(
    engine: &Engine,
    adapter: &PodmanHostAdapter,
    params: &ZapScanParams,
    run_id: &str,
    task_id: Option<&str>,
    trace_id: Option<&str>,
) -> Result<ProviderOutcome, ScanError> {
    execute_zap(
        engine,
        adapter,
        params.target.as_str(),
        params.timeout_s,
        params.alert_budget,
        run_id,
        DispatchOptions {
            task_id,
            trace_id,
            network: NetworkPlan::EgressAllowlist {
                ports: ZAP_EGRESS_PORTS.to_vec(),
            },
            template_path: None,
        },
    )
    .map_err(|(category, message)| ScanError {
        category: category.as_str().to_owned(),
        message,
    })
}

// ---------------------------------------------------------------------------
// rtis 统一入口（D1③：poc get/use 两接口——零接触负例硬门）
// ---------------------------------------------------------------------------

/// RTIS POC 委派（poc get 元数据锚 + poc use 使用回填——RTIS 现有接口
/// sanctioned 写路径；**poc update 不进本扫描链、poc add 零词形**——红线 5）。
///
/// 返回帧：metadata（poc get **id 元数据锚投影**——正文 content/usage/
/// verified_note/PATH 零透出，POC 正文零复制，PLAN :244）+ use_recorded
/// （poc use JSON 回执）+ 引用锚。
pub fn scan_rtis(
    provider: &RtisPocProvider,
    poc_id: i64,
    target: &str,
) -> Result<Value, ScanError> {
    let metadata = provider
        .resolve_metadata(poc_id)
        .map_err(|(c, m)| ScanError {
            category: c.as_str().to_owned(),
            message: format!("poc get failed: {m}"),
        })?;
    let use_recorded = provider
        .record_use(poc_id, target)
        .map_err(|(c, m)| ScanError {
            category: c.as_str().to_owned(),
            message: format!("poc use failed: {m}"),
        })?;
    // id 元数据锚投影（POC 正文零复制——content/usage/verified_note/path
    // 全部不出帧；rt.py poc get 全量 JSON 只进本函数边界，零落库零 artifact）。
    let metadata_anchor = json!({
        "id": metadata["id"],
        "title": metadata["title"],
        "category": metadata["category"],
        "kind": metadata["kind"],
        "language": metadata["language"],
        "severity": metadata["severity"],
        "verified": metadata["verified"],
        "hit_count": metadata["hit_count"],
        "updated_at": metadata["updated_at"],
        "content_omitted": true,
    });
    Ok(json!({
        "provider_id": "rtis-poc",
        "provider_version": "0.1.0",
        "poc_id": poc_id,
        "target": target,
        "metadata": metadata_anchor,
        "use_recorded": use_recorded,
        "note": "poc get/use 两接口委派（poc update 归 P5-7 evaluator/finding-link 链；poc add 零词形——V-3 裁决）"
    }))
}

// ---------------------------------------------------------------------------
// 五态投影（evaluator 复用——判定唯一路径）
// ---------------------------------------------------------------------------

/// ProviderOutcome → 五态研究结论（evaluator::evaluate 复用；命中面
/// INCONCLUSIVE：退出码 0 ≠ CONFIRMED，证据复核前置——A9）。
pub fn project_normalized(outcome: &ProviderOutcome) -> NormalizedResult {
    evaluate(&OutcomeView {
        status_word: &outcome.status_word,
        result: outcome.result.as_ref(),
        summary: &outcome.summary,
    })
}

/// RTIS 委派帧五态投影（cli_subprocess 面无容器信封：成功 = 元数据回读即
/// NOT_TESTED→非扫描结论面——RTIS provider 只承载受控引用，扫描判定面在
/// nuclei/zap；此处按 outcome 无五态语义，恒 SUCCEEDED/METADATA 词形）。
pub fn project_rtis(rtis_frame: &Value) -> Value {
    json!({
        "provider_id": "rtis-poc",
        "status_word": "EXECUTED",
        "result_status": "not_tested",
        "result_reason": "rtis poc get/use metadata delegation only: no scan executed (scan verdict face belongs to nuclei/zap providers)",
        "poc_id": rtis_frame["poc_id"],
        "target": rtis_frame["target"],
    })
}

// ---------------------------------------------------------------------------
// provider 契约面投影（D8：GET /api/wsw/scan/providers 词形）
// ---------------------------------------------------------------------------

/// provider 契约面投影（manifest json 直读防漂移 + plugin-registry catalog
/// 只读计数——D8 只读零新插件包；投影词形与 manifest 逐字段一致，判据 4）。
pub fn providers_projection(repo_root: &Path) -> Result<Value, String> {
    let manifest_path = |id: &str| repo_root.join("manifests/tools").join(format!("{id}.json"));
    let read_manifest = |id: &str| -> Result<Value, String> {
        let text = std::fs::read_to_string(manifest_path(id))
            .map_err(|e| format!("manifest {id} read failed: {e}"))?;
        serde_json::from_str(&text).map_err(|e| format!("manifest {id} parse failed: {e}"))
    };
    let nuclei = read_manifest("rip.security.nuclei")?;
    let zap = read_manifest("rip.security.zap")?;
    let rtis = read_manifest("rip.kb.rtis")?;

    let provider_row = |id: &str, tool_id: &str, m: &Value, adapter_note: &str| {
        let ports: Vec<Value> = m["network"]["egress"]["ports"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        json!({
            "provider_id": id,
            "tool_id": m["id"],
            "name": m["name"],
            "runtime": m["runtime"]["type"],
            "family": m["family"],
            "permission_level": m["permission_level"],
            "idempotent": m["idempotent"],
            "network_mode": m["network"]["mode"],
            "egress_ports": ports,
            "image": m["runtime"].get("image").cloned().unwrap_or(Value::Null),
            "version": m["version"],
            "adapter_note": adapter_note,
            "manifest": format!("manifests/tools/{tool_id}.json"),
        })
    };

    // plugin-registry catalog 只读计数（D8：entries + plugin_id 词表；
    // plugin-registry/plugin-runtime crate 零 diff——纯文件投影）。
    let catalog_path = repo_root.join("manifests/plugins/catalog.json");
    let (plugin_count, plugin_ids) = match std::fs::read_to_string(&catalog_path) {
        Ok(text) => {
            let catalog: Value = serde_json::from_str(&text)
                .map_err(|e| format!("plugin catalog parse failed: {e}"))?;
            let ids: Vec<Value> = catalog["entries"]
                .as_array()
                .map(|a| a.iter().map(|e| e["plugin_id"].clone()).collect())
                .unwrap_or_default();
            (ids.len(), ids)
        }
        Err(e) => return Err(format!("plugin catalog read failed: {e}")),
    };

    Ok(json!({
        "providers": [
            provider_row("nuclei", "rip.security.nuclei", &nuclei,
                "execute_nuclei 既有链直接编排（podman_host 容器派发；模板经 template_ref/文件面）"),
            provider_row("zap", "rip.security.zap", &zap,
                "execute_zap（容器内 zap.sh 内联 baseline + 镜像内 python3 解析 JSON 报告；零 daemon 零 API 端口）"),
            provider_row("rtis", RTIS_TOOL_ID, &rtis,
                "RtisPocProvider poc get/use 两接口委派（rt.py subprocess；T0 只读检索 + sanctioned 使用回填）"),
        ],
        "plugins": {
            "catalog_entry_count": plugin_count,
            "plugin_ids": plugin_ids,
            "source": "manifests/plugins/catalog.json",
            "note": "plugin-registry catalog 只读投影（D8 零新插件包零登记面）"
        }
    }))
}

// ---------------------------------------------------------------------------
// 内部装配面
// ---------------------------------------------------------------------------

/// 合成 PlanItemRecord（CLI/api 词形 → execute_nuclei 消费形状；不落
/// research_plan 表——rp-verify plan 通道不适用，scanner 面直接派发）。
fn synthetic_plan_item(
    target: &str,
    template_ref: Option<&str>,
    max_requests: i64,
    timeout_s: i64,
) -> PlanItemRecord {
    let rtis_poc_id = template_ref.and_then(|r| {
        r.strip_prefix("rtis-poc:")
            .and_then(|id| id.parse::<i64>().ok())
    });
    PlanItemRecord {
        schema_version: 1,
        item_id: "vi-wsw7-scan-000000-000-0000".to_owned(),
        plan_id: "vp-wsw7-scan-000000-000-0000".to_owned(),
        knowledge_ref: None,
        rtis_poc_id,
        provider_id: "nuclei".to_owned(),
        provider_version: Some("0.1.0".to_owned()),
        target_ref: target.to_owned(),
        dedup_fingerprint: "0".repeat(64),
        applicability_status: "applicable".to_owned(),
        applicability_reasons: vec![],
        risk_class: "low_impact".to_owned(),
        request_budget: max_requests.max(1),
        timeout_budget_s: timeout_s,
        execution_order: 0,
        expected_observation: None,
        dimension: "assets".to_owned(),
        result_status: None,
        result_reason: None,
        error_category: None,
        tool_run_id: None,
        trace_id: None,
        created_at: "1970-01-01T00:00:00Z".to_owned(),
        updated_at: "1970-01-01T00:00:00Z".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo_root() -> std::path::PathBuf {
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    /// stub rt.py fixture（V-9：RTIS 单测零真实库依赖——D6；脚本经 python3
    /// 子进程走 RtisPocProvider run_json 既有面）。
    fn write_stub_rt_py(dir: &Path) -> std::path::PathBuf {
        let stub = dir.join("rt.py");
        std::fs::write(
            &stub,
            r#"#!/usr/bin/env python3
import argparse, json, sys
p = argparse.ArgumentParser()
sub = p.add_subparsers(dest="cmd")
g = sub.add_parser("poc"); g.add_argument("action")
g.add_argument("--id", type=int)
g.add_argument("--on")
g.add_argument("--json", action="store_true")
args = p.parse_args()
if args.cmd == "poc" and args.action == "get":
    print(json.dumps({"id": args.id, "title": "stub poc", "category": "other", "kind": "script",
                      "language": "bash", "severity": "info", "verified": 0,
                      "hit_count": 1, "updated_at": "2026-01-01T00:00:00Z",
                      "content": "SECRET-POC-BODY-SHOULD-NOT-LEAK"}))
    sys.exit(0)
if args.cmd == "poc" and args.action == "use":
    print(json.dumps({"poc_id": args.id, "used_on": args.on, "recorded": True}))
    sys.exit(0)
print(json.dumps({"error": "unsupported"})); sys.exit(3)
"#,
        )
        .expect("stub rt.py write");
        stub
    }

    #[test]
    fn nuclei_params_synthesize_plan_item_budget_face() {
        let params = NucleiScanParams {
            target: "http://127.0.0.1:8080".to_owned(),
            template_ref: Some("rtis-poc:900008".to_owned()),
            template_file: None,
            max_requests: 10,
            timeout_s: 60,
        };
        let item = synthetic_plan_item(
            params.target.as_str(),
            params.template_ref.as_deref(),
            params.max_requests,
            params.timeout_s,
        );
        assert_eq!(item.target_ref, "http://127.0.0.1:8080");
        assert_eq!(item.rtis_poc_id, Some(900008));
        assert_eq!(item.request_budget, 10);
        assert_eq!(item.timeout_budget_s, 60);
        // build_input 原样消费（D1①：P5 单钮语义——rate/max_requests 同源）
        let input = verification_engine::provider::NucleiProvider::build_input(&item, 10);
        assert_eq!(input["url"], "http://127.0.0.1:8080");
        assert_eq!(input["max_requests"], 10);
        assert_eq!(input["template_ref"], "rtis-poc:900008");
    }

    #[test]
    fn scan_rtis_stub_matrix_poc_get_use_and_error() {
        let tmp = std::env::temp_dir().join(format!("wsw7-rtis-{}", std::process::id()));
        std::fs::create_dir_all(&tmp).unwrap();
        let stub = write_stub_rt_py(&tmp);
        let provider = RtisPocProvider {
            rt_py: stub,
            python: "python3".to_owned(),
        };
        // 正例：poc get 元数据锚 + poc use 回填（两接口帧全量在场）
        let frame = scan_rtis(&provider, 900008, "http://127.0.0.1:8080").expect("stub success");
        assert_eq!(frame["provider_id"], "rtis-poc");
        assert_eq!(frame["poc_id"], 900008);
        assert_eq!(frame["target"], "http://127.0.0.1:8080");
        assert_eq!(frame["metadata"]["id"], 900008);
        assert_eq!(frame["metadata"]["content_omitted"], true);
        // POC 正文零复制硬门（红线 5：content/usage/verified_note/path 零透出）
        let frame_text = serde_json::to_string(&frame).unwrap();
        assert!(
            !frame_text.contains("SECRET-POC-BODY-SHOULD-NOT-LEAK"),
            "POC 正文零透出（PLAN :244）"
        );
        assert_eq!(frame["use_recorded"]["recorded"], true);
        // 五态投影：RTIS 委派零扫描结论（not_tested 词形）
        let projected = project_rtis(&frame);
        assert_eq!(projected["result_status"], "not_tested");
        // 负例：词表外 action（stub exit 3）→ 结构化错误透出（非 panic）
        let bad = std::path::PathBuf::from(&tmp).join("rt_bad.py");
        std::fs::write(&bad, "#!/usr/bin/env python3\nimport sys\nsys.exit(4)\n").unwrap();
        let bad_provider = RtisPocProvider {
            rt_py: bad,
            python: "python3".to_owned(),
        };
        let err = scan_rtis(&bad_provider, 900008, "http://127.0.0.1:8080")
            .expect_err("stub failure surfaces");
        assert_eq!(err.category, "tool_unavailable");
        assert!(err.message.contains("poc get failed"), "{}", err.message);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn project_normalized_hit_is_inconclusive_not_confirmed() {
        // 命中面：SUCCEEDED + matched_count>0 → INCONCLUSIVE（退出码 0 ≠
        // CONFIRMED——五态判定唯一路径 = evaluator::evaluate，A9）
        let outcome = ProviderOutcome {
            provider_id: "nuclei".to_owned(),
            provider_version: "0.1.0".to_owned(),
            status_word: "SUCCEEDED".to_owned(),
            tool_run_id: Some("rn-1".to_owned()),
            trace_id: Some("tr-1".to_owned()),
            result: Some(json!({"matched_count": 3})),
            summary: "matched".to_owned(),
            error_category: None,
            tool_version: Some("v3.11.1".to_owned()),
            rule_ref: Some("rtis-poc:900008".to_owned()),
            duration_ms: 50,
        };
        let normalized = project_normalized(&outcome);
        assert_eq!(normalized.result_status, "INCONCLUSIVE");
        assert!(normalized.result_reason.contains("evidence review"));
        // 干净面：SUCCEEDED + matched_count=0 → NOT_REPRODUCED
        let mut clean = outcome.clone();
        clean.result = Some(json!({"matched_count": 0}));
        let normalized = project_normalized(&clean);
        assert_eq!(normalized.result_status, "NOT_REPRODUCED");
        // 超时面：TIMED_OUT → INCONCLUSIVE + Timeout 分类（执行失败 ≠
        // NOT_REPRODUCED——五态纪律规则 2）
        let mut timed_out = outcome;
        timed_out.status_word = "TIMED_OUT".to_owned();
        timed_out.result = None;
        let normalized = project_normalized(&timed_out);
        assert_eq!(normalized.result_status, "INCONCLUSIVE");
        assert_eq!(normalized.error_category.as_deref(), Some("timeout"));
    }

    #[test]
    fn providers_projection_matches_manifest_fields() {
        let root = repo_root();
        let projection = providers_projection(&root).expect("projection ok");
        let rows = projection["providers"].as_array().unwrap();
        assert_eq!(rows.len(), 3, "三 provider 词表（nuclei/zap/rtis）");
        assert_eq!(SCAN_PROVIDERS, ["nuclei", "zap", "rtis"]);
        let by_id = |id: &str| {
            rows.iter()
                .find(|r| r["provider_id"] == id)
                .unwrap_or_else(|| panic!("provider {id} 在投影"))
        };
        let nuclei = by_id("nuclei");
        assert_eq!(nuclei["tool_id"], "rip.security.nuclei");
        assert_eq!(nuclei["runtime"], "podman_host");
        assert_eq!(nuclei["family"], "network");
        assert_eq!(nuclei["permission_level"], "T1");
        assert_eq!(nuclei["idempotent"], true);
        assert_eq!(nuclei["network_mode"], "egress_allowlist");
        assert!(nuclei["image"]
            .as_str()
            .unwrap()
            .starts_with("localhost/rip-nuclei@sha256:"));
        let zap = by_id("zap");
        assert_eq!(zap["tool_id"], "rip.security.zap");
        assert_eq!(zap["runtime"], "podman_host");
        assert_eq!(zap["family"], "network");
        assert_eq!(zap["permission_level"], "T1");
        assert_eq!(zap["idempotent"], false, "主动扫描改变靶机状态");
        assert_eq!(zap["network_mode"], "egress_allowlist");
        assert!(zap["image"]
            .as_str()
            .unwrap()
            .starts_with("localhost/rip-zap@sha256:"));
        let rtis = by_id("rtis");
        assert_eq!(rtis["tool_id"], "rip.kb.rtis");
        assert_eq!(rtis["runtime"], "python_worker");
        assert_eq!(rtis["family"], "knowledge");
        assert_eq!(rtis["permission_level"], "T0");
        // D8 plugin catalog 只读计数（实测 3 entries）
        let plugins = &projection["plugins"];
        assert_eq!(plugins["catalog_entry_count"], 3);
        assert_eq!(plugins["source"], "manifests/plugins/catalog.json");
    }
}
