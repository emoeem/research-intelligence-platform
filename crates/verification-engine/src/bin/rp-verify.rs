//! rp-verify CLI（RIP3 Phase 5 验证编排入口，P5-2 契约面验收 + P5-3 起逐批扩展）：
//!
//! ~~~text
//! cargo run --offline -p verification-engine --bin rp-verify -- plan-create \
//!   --task <rtis-task-id> --title '<t>' [--target '<summary>'] [--budget '<json>']
//! cargo run --offline -p verification-engine --bin rp-verify -- plan-get --plan <id>
//! cargo run --offline -p verification-engine --bin rp-verify -- plan-list [--limit N]
//! cargo run --offline -p verification-engine --bin rp-verify -- plan-status --plan <id> --status ready
//! cargo run --offline -p verification-engine --bin rp-verify -- item-add --plan <id> \
//!   --provider rtis-poc --target '<normalized>' --fingerprint <sha256> \
//!   [--poc-id 900008] [--knowledge <knowledge_id>] [--reasons '<json array>'] \
//!   [--applicability applicable|excluded|needs_context] [--risk low_impact] \
//!   [--requests N] [--timeout-s N] [--order N] [--expected '<text>']
//! cargo run --offline -p verification-engine --bin rp-verify -- item-get --item <id>
//! cargo run --offline -p verification-engine --bin rp-verify -- item-list --plan <id>
//! cargo run --offline -p verification-engine --bin rp-verify -- result-set --item <id> \
//!   --status CONFIRMED --reason '<依据>' [--error-category timeout] [--run-id <id>] [--trace-id <id>]
//! cargo run --offline -p verification-engine --bin rp-verify -- event-list --plan <id>
//! cargo run --offline -p verification-engine --bin rp-verify -- fingerprint \
//!   --target h --endpoint e --poc p --provider pv [--params '<json>'] [--authctx <fingerprint>]
//! ~~~
//!
//! 输出契约：单行 JSON 进 stdout（rp-doc 惯例），错误帧 {"error":{code,message}}
//! 也走 stdout，退出码 0 = 成功 / 2 = 用法违规 / 1 = 守卫或 DB 拒绝。
//! RP_DB_PATH 指定 dev 库副本（缺省 memory 仅自检）；迁移由本 CLI 幂等应用
//! （database::migrate，020 已在链）。

use database::{migrate, DbPath, Pool, PoolConfig};
use std::path::PathBuf;
use std::process::exit;
use std::time::Duration;
use tool_engine::adapter::egress_proxy::NetworkPlan;
use tool_engine::adapter::podman_host::{PodmanHostAdapter, PodmanHostConfig};
use tool_engine::Engine;
use tool_registry::Registry;
use verification_engine::evaluator::{evaluate, normalized_doc, OutcomeView};
use verification_engine::fingerprint::dedup_fingerprint;
use verification_engine::provider::{
    execute_nuclei, RtisPocProvider,
};
use verification_engine::store::{NewItem, PlanStore};

fn main() {
    exit(run());
}

fn usage_text() -> &'static str {
    "usage: rp-verify <sub> [args]\n\
     \n\
     subcommands:\n\
     \x20 plan-create --task <id> --title '<t>' [--workspace w] [--target '<s>'] [--budget '<json>']\n\
     \x20 plan-get --plan <id>\n\
     \x20 plan-list [--limit N]\n\
     \x20 plan-status --plan <id> --status draft|ready|executing|paused|completed|cancelled\n\
     \x20 item-add --plan <id> --provider <id> --target '<ref>' --fingerprint <sha256>\n\
     \x20   [--poc-id N] [--knowledge <id>] [--applicability ...] [--reasons '<json>']\n\
     \x20   [--risk ...] [--requests N] [--timeout-s N] [--order N] [--expected '<t>'] [--version v]\n\
     \x20 item-get --item <id>\n\
     \x20 item-list --plan <id>\n\
     \x20 result-set --item <id> --status <五态> --reason '<r>' [--error-category <c>]\n\
     \x20   [--run-id <id>] [--trace-id <id>]\n\
     \x20 event-list --plan <id>\n\
     \x20 fingerprint --target h --endpoint e --poc p --provider pv [--params '<json>'] [--authctx <fp>]\n\
     \x20 match --target '<t>' [--components 'a,b'] [--profile '<json>'] [--limit N]\n\
     \x20   双渠道匹配：pg-intel（rp-kc hybrid 统一检索）+ rtis-poc（rt.py poc 元数据面），\n\
     \x20   命中与排除原因逐条可解释；--components 中文零命中（RTIS 英文优先）\n\\
     \x20 execute-plan --plan <id> --template '<path>' [--ws-root <dir>] [--network none|egress]\n\
     \x20   --template：调用面提供的本地 nuclei 模板文件（检测逻辑不自动生成）\n\
     \x20   计划级派发：executing 状态推进 + 逐项 Provider 执行 + 五态归一化回写；\n\
     \x20   幂等恢复：重跑只派发未判定项（服务重启/暂停恢复不重复派发）；\n\
     \x20   运行期预算复核：Σ 放行 + 本项 ≤ 计划 max_requests（超项留痕\n\
     \x20   NOT_TESTED + budget_exhausted）；\n\
     \x20   取消传播：tool_run.cancel_requested_at（rp-tool cancel）经看门狗生效，\n\
     \x20   计划级 plan-cancel/plan-pause 在项间检查位生效\n\
     \x20 plan-cancel --plan <id>\n\
     \x20   计划级取消（executing/paused→cancelled；终态拒绝）\n\
     \x20 plan-pause --plan <id>\n\
     \x20   计划级暂停（executing/ready→paused；执行进程项间检查位停）\n\
     \x20 plan-resume --plan <id>\n\
     \x20   计划级恢复（paused→ready；续跑 execute-plan 幂等只派未判定项）\n\
     \x20 evidence-list --item <id>\n\
     \x20   证据档检索（021 research_evidence；SHA256 去重 append-only 快照）\n\
     \x20 finding-link --item <id> [--note '<t>']\n\
     \x20   CONFIRMED 项验证结论回填 RTIS（rt.py poc update --verified；\n\
     \x20   非 CONFIRMED 或无 rtis_poc_id 引用拒绝）\n\\
     \x20 plan-build --task <id> --title '<t>' --target '<t>' [--components 'a,b']\n\
     \x20   [--profile '<json>'] [--max-requests N] [--timeout-s N] [--params '<json>']\n\
     \x20   match → planner（预算封顶/缓存决策/排序）→ 落库，输出 plan_id + 预览摘要\n\
     \n\
     env: RP_DB_PATH（dev 库副本路径；缺省内存库）\n\
     exit: 0 成功 / 1 守卫或 DB 拒绝 / 2 用法违规\n\
     \n\
     help: rp-verify --help（stdout, exit 0）"
}

fn usage() -> i32 {
    eprintln!("{}", usage_text());
    2
}

/// 参数表（--key value 形态；布尔/缺省由调用侧处理）。
struct Args {
    map: std::collections::HashMap<String, String>,
}

impl Args {
    fn parse(all: &[String]) -> Args {
        let mut map = std::collections::HashMap::new();
        let mut index = 0;
        while index < all.len() {
            let arg = &all[index];
            if let Some(key) = arg.strip_prefix("--") {
                if let Some(value) = all.get(index + 1) {
                    map.insert(key.to_owned(), value.clone());
                    index += 2;
                    continue;
                }
            }
            index += 1;
        }
        Args { map }
    }

    fn get(&self, key: &str) -> Option<&str> {
        self.map.get(key).map(String::as_str)
    }

    fn required(&self, key: &str) -> Result<String, i32> {
        match self.get(key) {
            Some(value) => Ok(value.to_owned()),
            None => Err(usage()),
        }
    }
}

fn open_store() -> Result<PlanStore, String> {
    let path = std::env::var("RP_DB_PATH").unwrap_or_default();
    let db_path = if path.trim().is_empty() {
        DbPath::Memory
    } else {
        DbPath::File(PathBuf::from(path))
    };
    let pool = Pool::open(PoolConfig {
        path: db_path,
        readonly: false,
        max_connections: 1,
        busy_timeout: Duration::from_secs(5),
    })
    .map_err(|e| e.to_string())?;
    let migrations_dir = repo_root().join("migrations");
    migrate(&pool, &migrations_dir).map_err(|e| e.to_string())?;
    Ok(PlanStore::new(pool))
}

fn engine_with_nuclei() -> Engine {
    let pool = Pool::open(PoolConfig {
        path: DbPath::Memory,
        readonly: false,
        max_connections: 1,
        busy_timeout: Duration::from_secs(5),
    })
    .expect("engine memory pool");
    migrate(&pool, &repo_root().join("migrations")).expect("migrations");
    let registry = Registry::open(pool.clone());
    let manifest: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(repo_root().join("manifests/tools/rip.security.nuclei.json"))
            .expect("nuclei manifest"),
    )
    .expect("manifest json");
    registry.register(&manifest).expect("register nuclei");
    Engine::open(pool.clone(), registry)
}

fn repo_root() -> PathBuf {
    // rp-verify 约定从仓库根或 web/ 外任意目录运行：优先 RP_REPO_ROOT，
    // 缺省按 CARGO_MANIFEST_DIR 编译期锚（target/debug 运行时同样成立）。
    if let Ok(root) = std::env::var("RP_REPO_ROOT") {
        return PathBuf::from(root);
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn emit(value: &serde_json::Value) {
    println!(
        "{}",
        serde_json::to_string(value).unwrap_or_else(|_| "{\"error\":{\"code\":\"serialize\",\"message\":\"serde_json\"}}".to_owned())
    );
}

fn emit_error(code: &str, message: &str) -> i32 {
    emit(&serde_json::json!({ "error": { "code": code, "message": message } }));
    1
}

/// 归一化文档组装 + result-normalized.v1 契约校验（execute 落库/发事件前
/// fail fast；契约违规 = 实现漂移，退出码 1 同守卫拒绝口径）。
fn checked_normalized_doc(
    item_id: &str,
    normalized: &verification_engine::evaluator::NormalizedResult,
) -> serde_json::Value {
    let doc = normalized_doc(item_id, normalized);
    if let Err(errs) = verification_engine::contract::validate_normalized_doc(&doc) {
        emit(&serde_json::json!({ "error": {
            "code": "normalized_contract",
            "message": format!("result-normalized.v1 violation for {item_id}: {errs:?}"),
        } }));
        exit(1);
    }
    doc
}

fn run() -> i32 {
    let all: Vec<String> = std::env::args().skip(1).collect();
    if all.iter().any(|a| a == "--help" || a == "-h") {
        println!("{}", usage_text());
        return 0;
    }
    let Some(sub) = all.first() else {
        return usage();
    };
    let args = Args::parse(&all[1..]);
    let store = match open_store() {
        Ok(store) => store,
        Err(message) => return emit_error("db_open", &message),
    };
    match sub.as_str() {
        "plan-create" => {
            let task = args.get("task").map(str::to_owned);
            let title = args.get("title").map(str::to_owned);
            let workspace = args.get("workspace").unwrap_or("default").to_owned();
            let target = args.get("target").map(str::to_owned);
            let budget: Option<serde_json::Value> = args
                .get("budget")
                .and_then(|b| serde_json::from_str(b).ok());
            match store.create_plan(
                task.as_deref(),
                &workspace,
                title.as_deref(),
                target.as_deref(),
                budget.as_ref(),
            ) {
                Ok(record) => {
                    emit(&serde_json::to_value(&record).unwrap_or_default());
                    0
                }
                Err(e) => emit_error("plan_create", &e.to_string()),
            }
        }
        "plan-get" => match args.required("plan") {
            Ok(plan_id) => match store.get_plan(&plan_id) {
                Ok(Some(record)) => {
                    emit(&serde_json::to_value(&record).unwrap_or_default());
                    0
                }
                Ok(None) => emit_error("not_found", &plan_id),
                Err(e) => emit_error("plan_get", &e.to_string()),
            },
            Err(code) => code,
        },
        "plan-list" => {
            let limit: u32 = args
                .get("limit")
                .and_then(|v| v.parse().ok())
                .unwrap_or(50);
            match store.list_plans(limit) {
                Ok(rows) => {
                    emit(&serde_json::json!({ "items": rows, "limit": limit }));
                    0
                }
                Err(e) => emit_error("plan_list", &e.to_string()),
            }
        }
        "plan-status" => {
            let (plan, status) = match (args.required("plan"), args.required("status")) {
                (Ok(a), Ok(b)) => (a, b),
                _ => return usage(),
            };
            match store.set_plan_status(&plan, &status) {
                Ok(()) => {
                    emit(&serde_json::json!({ "plan_id": plan, "status": status }));
                    0
                }
                Err(e) => emit_error("plan_status", &e.to_string()),
            }
        }
        "item-add" => {
            let plan = match args.required("plan") {
                Ok(v) => v,
                Err(code) => return code,
            };
            let provider = match args.required("provider") {
                Ok(v) => v,
                Err(code) => return code,
            };
            let target = match args.required("target") {
                Ok(v) => v,
                Err(code) => return code,
            };
            let fingerprint = match args.required("fingerprint") {
                Ok(v) => v,
                Err(code) => return code,
            };
            let reasons: Vec<String> = args
                .get("reasons")
                .and_then(|r| serde_json::from_str(r).ok())
                .unwrap_or_default();
            let item = NewItem {
                knowledge_ref: args.get("knowledge").map(str::to_owned),
                rtis_poc_id: args.get("poc-id").and_then(|v| v.parse().ok()),
                provider_id: provider,
                provider_version: args.get("version").map(str::to_owned),
                target_ref: target,
                dedup_fingerprint: fingerprint,
                applicability_status: args
                    .get("applicability")
                    .unwrap_or("needs_context")
                    .to_owned(),
                applicability_reasons: reasons,
                risk_class: args.get("risk").unwrap_or("low_impact").to_owned(),
                request_budget: args.get("requests").and_then(|v| v.parse().ok()).unwrap_or(10),
                timeout_budget_s: args.get("timeout-s").and_then(|v| v.parse().ok()).unwrap_or(60),
                execution_order: args.get("order").and_then(|v| v.parse().ok()).unwrap_or(0),
                expected_observation: args.get("expected").map(str::to_owned),
            };
            match store.add_item(&plan, item) {
                Ok(record) => {
                    emit(&serde_json::to_value(&record).unwrap_or_default());
                    0
                }
                Err(e) => emit_error("item_add", &e.to_string()),
            }
        }
        "item-get" => match args.required("item") {
            Ok(item_id) => match store.get_item(&item_id) {
                Ok(Some(record)) => {
                    emit(&serde_json::to_value(&record).unwrap_or_default());
                    0
                }
                Ok(None) => emit_error("not_found", &item_id),
                Err(e) => emit_error("item_get", &e.to_string()),
            },
            Err(code) => code,
        },
        "item-list" => match args.required("plan") {
            Ok(plan_id) => match store.list_items(&plan_id) {
                Ok(rows) => {
                    emit(&serde_json::json!({ "items": rows, "plan_id": plan_id }));
                    0
                }
                Err(e) => emit_error("item_list", &e.to_string()),
            },
            Err(code) => code,
        },
        "result-set" => {
            let item = match args.required("item") {
                Ok(v) => v,
                Err(code) => return code,
            };
            let status = match args.required("status") {
                Ok(v) => v,
                Err(code) => return code,
            };
            match store.set_result(
                &item,
                &status,
                args.get("reason"),
                args.get("error-category"),
                args.get("run-id"),
                args.get("trace-id"),
            ) {
                Ok(record) => {
                    emit(&serde_json::to_value(&record).unwrap_or_default());
                    0
                }
                Err(e) => emit_error("result_set", &e.to_string()),
            }
        }
        "event-list" => match args.required("plan") {
            Ok(plan_id) => match store.list_events(&plan_id) {
                Ok(rows) => {
                    emit(&serde_json::json!({ "items": rows, "plan_id": plan_id }));
                    0
                }
                Err(e) => emit_error("event_list", &e.to_string()),
            },
            Err(code) => code,
        },
        "execute-plan" => {
            let plan_id = match args.required("plan") {
                Ok(v) => v,
                Err(code) => return code,
            };
            let ws_root = args
                .get("ws-root")
                .map(PathBuf::from)
                .unwrap_or_else(|| std::env::temp_dir().join(format!("rip-verify-ws-{}", std::process::id())));
            let template_path = args.get("template").map(PathBuf::from);
            let network = match args.get("network").unwrap_or("egress") {
                "none" => NetworkPlan::None,
                _ => NetworkPlan::EgressAllowlist {
                    ports: verification_engine::provider::NUCLEI_EGRESS_PORTS.to_vec(),
                },
            };
            let plan = match store.get_plan(&plan_id) {
                Ok(Some(p)) => p,
                Ok(None) => return emit_error("not_found", &plan_id),
                Err(e) => return emit_error("execute_plan", &e.to_string()),
            };
            if plan.status != "ready" {
                return emit_error(
                    "plan_not_ready",
                    &format!("plan {} status={} (execute requires ready)", plan_id, plan.status),
                );
            }
            store.set_plan_status(&plan_id, "executing").map_err(|e| emit_error("execute_plan", &e.to_string())).ok();
            let engine = engine_with_nuclei();
            let adapter = PodmanHostAdapter::new(PodmanHostConfig {
                workspace_root: ws_root.clone(),
                ..PodmanHostConfig::default()
            });
            let rtis = RtisPocProvider::from_repo_root(&repo_root());
            let items = match store.list_items(&plan_id) {
                Ok(items) => items,
                Err(e) => return emit_error("execute_plan", &e.to_string()),
            };
            // 幂等恢复（P5-6 完成标准：服务重启/暂停恢复后不重复派发）：
            // 只派发未判定项，已判定项原样跳过（结论不重算不覆盖）。
            let (items, resumed_skipped) =
                verification_engine::scheduler::pending_items(items);
            // 运行期执行侧预算复核（P5-4 G 遗留落地）：已放行项预算之和 +
            // 本项 ≤ 计划 cap 的第二道闸（planner 生成侧封顶之外的 §5.9 面）。
            let plan_cap = plan
                .budget
                .as_ref()
                .and_then(|b| b.get("max_requests"))
                .and_then(|v| v.as_i64());
            let mut consumed = 0i64;
            let mut dispatched = 0usize;
            let mut budget_skipped = 0usize;
            let mut cancelled_at = Option::<String>::None;
            for item in &items {
                // 计划级取消检查位（项间生效；项内经 tool_run cancel_requested_at 看门狗）
                let current = match store.get_plan(&plan_id) {
                    Ok(Some(p)) => p,
                    _ => break,
                };
                if current.status != "executing" {
                    cancelled_at = Some(current.status.clone());
                    break;
                }
                // 运行期预算复核：超 cap 项留痕 NOT_TESTED + budget_exhausted
                // （未执行 ≠ 任何结论；覆盖缺口带剩余量说明供补充研究消费）。
                if let Some(cap) = plan_cap {
                    if !verification_engine::scheduler::runtime_budget_ok(
                        consumed,
                        item.request_budget,
                        Some(cap),
                    ) {
                        let outcome = verification_engine::scheduler::budget_exhausted_result(
                            consumed,
                            item.request_budget,
                            cap,
                        );
                        let _ = store.set_result(
                            &item.item_id,
                            &outcome.result_status,
                            Some(&outcome.result_reason),
                            outcome.error_category.as_deref(),
                            None,
                            None,
                        );
                        let _ = store.record_event(
                            Some(&plan_id),
                            Some(&item.item_id),
                            None,
                            None,
                            "item_skipped",
                            Some(serde_json::json!({
                                "reason": "runtime_budget_exhausted",
                                "consumed": consumed,
                                "item_budget": item.request_budget,
                                "plan_cap": cap,
                            })),
                        );
                        budget_skipped += 1;
                        continue;
                    }
                }
                consumed += item.request_budget;
                let run_id = format!("rn-verify-{}", item.item_id.trim_start_matches("vi-").replace('-', ""));
                // Trace 六 ID 锚（P5-7）：一次 execute 动作一个 trace，贯通
                // tool_run（submit_correlated）与 evidence 事件流（015 面语义）。
                let trace_id = format!(
                    "tr-exec-{}-{}",
                    plan_id.trim_start_matches("vp-"),
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_micros()
                );
                let record = match item.provider_id.as_str() {
                    "nuclei" => {
                        let task_id = plan.task_ref.clone();
                        let outcome = match execute_nuclei(
                            &engine,
                            &adapter,
                            item,
                            &run_id,
                            verification_engine::provider::DispatchOptions {
                                task_id: task_id.as_deref(),
                                trace_id: Some(&trace_id),
                                network: network.clone(),
                                template_path: template_path.as_deref(),
                            },
                        ) {
                            Ok(outcome) => outcome,
                            Err((category, message)) => {
                                // 派发拒绝也归一化（§5.9：失败/不可用分类），不静默
                                let view = OutcomeView {
                                    status_word: "FAILED",
                                    result: None,
                                    summary: &message,
                                };
                                let normalized = evaluate(&view);
                                let category_word = if message.contains("image")
                                    || message.contains("submit rejected")
                                    || message.contains("drive failed")
                                {
                                    "tool_unavailable"
                                } else {
                                    category.as_str()
                                };
                                let _ = store.set_result(
                                    &item.item_id,
                                    &normalized.result_status,
                                    Some(&normalized.result_reason),
                                    Some(category_word),
                                    None,
                                    Some(&trace_id),
                                );
                                // 派发拒绝帧也是证据（log_excerpt）：失败现场留档
                                let _ = store.record_evidence(
                                    &item.item_id,
                                    &plan_id,
                                    None,
                                    Some(&trace_id),
                                    "log_excerpt",
                                    &serde_json::json!({
                                        "dispatch": "rejected",
                                        "reason": message,
                                        "provider": item.provider_id,
                                    }),
                                );
                                let _ = store.record_event(
                                    Some(&plan_id),
                                    Some(&item.item_id),
                                    None,
                                    Some(&trace_id),
                                    "result_assigned",
                                    Some(serde_json::json!({"dispatch": "rejected", "reason": message})),
                                );
                                continue;
                            }
                        };
                        dispatched += 1;
                        let view = OutcomeView {
                            status_word: &outcome.status_word,
                            result: outcome.result.as_ref(),
                            summary: &outcome.summary,
                        };
                        let normalized = evaluate(&view);
                        // Evidence 双帧落档（P5-7）：原始 Provider 结果帧 +
                        // 归一化结论帧（SHA256 去重，append-only）。
                        let evidence_provider = store
                            .record_evidence(
                                &item.item_id,
                                &plan_id,
                                outcome.tool_run_id.as_deref(),
                                Some(&trace_id),
                                "provider_result",
                                &serde_json::json!({
                                    "provider": outcome.provider_id,
                                    "provider_version": outcome.provider_version,
                                    "tool_version": outcome.tool_version,
                                    "status_word": outcome.status_word,
                                    "result": outcome.result,
                                    "summary": outcome.summary,
                                    "duration_ms": outcome.duration_ms,
                                    "rule_ref": outcome.rule_ref,
                                }),
                            )
                            .ok();
                        let evidence_normalized = store
                            .record_evidence(
                                &item.item_id,
                                &plan_id,
                                outcome.tool_run_id.as_deref(),
                                Some(&trace_id),
                                "normalized_result",
                                &checked_normalized_doc(&item.item_id, &normalized),
                            )
                            .ok();
                        let evidence_provider_id =
                            evidence_provider.as_ref().map(|(e, _)| e.evidence_id.clone());
                        let evidence_normalized_id =
                            evidence_normalized.as_ref().map(|(e, _)| e.evidence_id.clone());
                        store
                            .set_result(
                                &item.item_id,
                                &normalized.result_status,
                                Some(&normalized.result_reason),
                                normalized.error_category.as_deref(),
                                outcome.tool_run_id.as_deref(),
                                Some(&trace_id),
                            )
                            .map_err(|e| emit_error("execute_plan", &e.to_string()))
                            .ok();
                        store
                            .record_event(
                                Some(&plan_id),
                                Some(&item.item_id),
                                outcome.tool_run_id.as_deref(),
                                Some(&trace_id),
                                "dispatched",
                                Some(serde_json::json!({
                                    "provider": outcome.provider_id,
                                    "tool_version": outcome.tool_version,
                                    "normalized": checked_normalized_doc(&item.item_id, &normalized),
                                    "evidence_ids": [
                                        evidence_provider_id.clone(),
                                        evidence_normalized_id.clone(),
                                    ],
                                })),
                            )
                            .ok();
                        serde_json::json!({
                            "item_id": item.item_id,
                            "status_word": outcome.status_word,
                            "tool_run_id": outcome.tool_run_id,
                            "trace_id": trace_id,
                            "evidence_ids": [
                                evidence_provider_id,
                                evidence_normalized_id,
                            ],
                            "normalized": checked_normalized_doc(&item.item_id, &normalized),
                        })
                    }
                    _ => {
                        // RTIS POC 引用面：记录使用关系（sanctioned CLI 写路径）+
                        // 结论 = 引用面登记（无探针 Provider 不冒结论）
                        let meta = if let Some(poc) = item.rtis_poc_id {
                            let _ = rtis.record_use(poc, plan.target_summary.as_deref().unwrap_or("rip3-p5-execute"));
                            serde_json::json!({"rtis_poc_id": poc, "use_recorded": true})
                        } else {
                            serde_json::json!({"rtis_poc_id": null})
                        };
                        let meta_ref = &meta;
                        store
                            .set_result(
                                &item.item_id,
                                "INCONCLUSIVE",
                                Some("RTIS POC reference recorded; probe provider required for automated conclusion"),
                                None,
                                None,
                                Some(&trace_id),
                            )
                            .map_err(|e| emit_error("execute_plan", &e.to_string()))
                            .ok();
                        store
                            .record_event(
                                Some(&plan_id),
                                Some(&item.item_id),
                                None,
                                Some(&trace_id),
                                "dispatched",
                                Some(meta_ref.clone()),
                            )
                            .ok();
                        dispatched += 1;
                        serde_json::json!({"item_id": item.item_id, "trace_id": trace_id, "reference": meta_ref})
                    }
                };
                emit(&record);
            }
            // 终局：若计划仍 executing（未被取消），按五态面收敛 completed
            let final_plan = store.get_plan(&plan_id).ok().flatten();
            if let Some(p) = final_plan {
                if p.status == "executing" {
                    let _ = store.set_plan_status(&plan_id, "completed");
                }
            }
            emit(&serde_json::json!({
                "plan_id": plan_id,
                "dispatched": dispatched,
                "resumed_skipped": resumed_skipped,
                "budget_skipped": budget_skipped,
                "stopped_by": cancelled_at,
            }));
            0
        }
        "plan-cancel" => {
            let plan_id = match args.required("plan") {
                Ok(v) => v,
                Err(code) => return code,
            };
            match store.set_plan_status(&plan_id, "cancelled") {
                Ok(()) => {
                    emit(&serde_json::json!({ "plan_id": plan_id, "status": "cancelled" }));
                    0
                }
                Err(e) => emit_error("plan_cancel", &e.to_string()),
            }
        }
        "plan-pause" => {
            // 计划级暂停（P5-6）：executing/ready → paused；执行中进程在
            // 项间检查位读到非 executing 即停（跨进程经 SQLite 状态行传播）。
            let plan_id = match args.required("plan") {
                Ok(v) => v,
                Err(code) => return code,
            };
            match store.set_plan_status(&plan_id, "paused") {
                Ok(()) => {
                    emit(&serde_json::json!({ "plan_id": plan_id, "status": "paused" }));
                    0
                }
                Err(e) => emit_error("plan_pause", &e.to_string()),
            }
        }
        "plan-resume" => {
            // 计划级恢复（P5-6）：paused → ready；续跑经 execute-plan 重跑
            // （幂等恢复只派发未判定项，已完成项不重复执行）。
            let plan_id = match args.required("plan") {
                Ok(v) => v,
                Err(code) => return code,
            };
            match store.set_plan_status(&plan_id, "ready") {
                Ok(()) => {
                    emit(&serde_json::json!({ "plan_id": plan_id, "status": "ready" }));
                    0
                }
                Err(e) => emit_error("plan_resume", &e.to_string()),
            }
        }
        "evidence-list" => {
            // 证据档检索入口（P5-7，021 research_evidence）：计划项维度
            // 时间升序，UI 跳转面与覆盖矩阵消费。
            let item_id = match args.required("item") {
                Ok(v) => v,
                Err(code) => return code,
            };
            match store.list_evidence(&item_id) {
                Ok(records) => {
                    emit(&serde_json::json!({
                        "item_id": item_id,
                        "items": records,
                    }));
                    0
                }
                Err(e) => emit_error("evidence_list", &e.to_string()),
            }
        }
        "finding-link" => {
            // RTIS Finding 闭环写面（P5-7）：CONFIRMED 项的验证结论经
            // rt.py poc update --verified --note 回填（RTIS 既有接口
            // sanctioned 写路径；结论性证据状态以 RTIS 为准，§3.1）。
            // 守卫：非 CONFIRMED 项拒绝（避免把未确认结论当成漏洞事实）；
            // 无 rtis_poc_id 引用的项拒绝（Finding 锚在 RTIS，RIP3 零自建）。
            let item_id = match args.required("item") {
                Ok(v) => v,
                Err(code) => return code,
            };
            let item = match store.get_item(&item_id) {
                Ok(Some(item)) => item,
                Ok(None) => return emit_error("not_found", &item_id),
                Err(e) => return emit_error("finding_link", &e.to_string()),
            };
            if item.result_status.as_deref() != Some("CONFIRMED") {
                return emit_error(
                    "finding_link_requires_confirmed",
                    &format!(
                        "item {item_id} result_status={:?} (finding-link requires CONFIRMED with evidence review)",
                        item.result_status
                    ),
                );
            }
            let Some(poc_id) = item.rtis_poc_id else {
                return emit_error(
                    "finding_link_requires_rtis_poc",
                    &format!("item {item_id} has no rtis_poc_id reference (finding anchor lives in RTIS)"),
                );
            };
            let note = args
                .get("note")
                .map(str::to_owned)
                .unwrap_or_else(|| {
                    item.result_reason
                        .clone()
                        .unwrap_or_else(|| "RIP3 verification confirmed".to_owned())
                });
            let rtis = RtisPocProvider::from_repo_root(&repo_root());
            match rtis.record_verification(poc_id, &note) {
                Ok(payload) => {
                    store
                        .record_event(
                            Some(&item.plan_id),
                            Some(&item.item_id),
                            item.tool_run_id.as_deref(),
                            item.trace_id.as_deref(),
                            "result_assigned",
                            Some(serde_json::json!({
                                "finding_link": true,
                                "rtis_poc_id": poc_id,
                                "note": note,
                            })),
                        )
                        .ok();
                    emit(&serde_json::json!({
                        "item_id": item.item_id,
                        "rtis_poc_id": poc_id,
                        "linked": true,
                        "rtis_response": payload,
                    }));
                    0
                }
                Err((category, message)) => emit_error(
                    "finding_link_rtis_failed",
                    &format!("rt.py poc update failed ({category:?}): {message}"),
                ),
            }
        }
        "match" => {
            use verification_engine::matcher::{
                run_match, KnowledgeChannel, MatchQuery, RtisPocChannel, UnifiedSearchChannel,
            };
            let target = match args.required("target") {
                Ok(v) => v,
                Err(code) => return code,
            };
            let components: Vec<String> = args
                .get("components")
                .map(|c| {
                    c.split(',')
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .map(str::to_owned)
                        .collect()
                })
                .unwrap_or_default();
            let profile_versions: std::collections::HashMap<String, String> = args
                .get("profile")
                .and_then(|p| serde_json::from_str(p).ok())
                .unwrap_or_default();
            let limit: usize = args
                .get("limit")
                .and_then(|v| v.parse().ok())
                .unwrap_or(10);
            let root = repo_root();
            let rtis = RtisPocChannel::from_repo_root(&root);
            let unified = UnifiedSearchChannel {
                rp_kc: root.join("target/debug/rp-kc"),
            };
            let query = MatchQuery {
                target_ref: target,
                components,
                profile_versions,
                limit,
            };
            let result = run_match(
                &[
                    &unified as &dyn KnowledgeChannel,
                    &rtis as &dyn KnowledgeChannel,
                ],
                &query,
                limit,
            );
            emit(&serde_json::to_value(&result).unwrap_or_default());
            0
        }
        "plan-build" => {
            use verification_engine::matcher::{
                run_match, KnowledgeChannel, MatchQuery, RtisPocChannel, UnifiedSearchChannel,
            };
            use verification_engine::planner::{draft_from_match, persist_draft, PlanBudget};
            let target = match args.required("target") {
                Ok(v) => v,
                Err(code) => return code,
            };
            let task = args.get("task").map(str::to_owned);
            let title = args.get("title").map(str::to_owned);
            let components: Vec<String> = args
                .get("components")
                .map(|c| {
                    c.split(',')
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .map(str::to_owned)
                        .collect()
                })
                .unwrap_or_default();
            let profile_versions: std::collections::HashMap<String, String> = args
                .get("profile")
                .and_then(|p| serde_json::from_str(p).ok())
                .unwrap_or_default();
            let params_json = args.get("params").unwrap_or("{}").to_owned();
            let budget = PlanBudget {
                max_requests: args.get("max-requests").and_then(|v| v.parse().ok()).unwrap_or(30),
                timeout_s: args.get("timeout-s").and_then(|v| v.parse().ok()).unwrap_or(60),
                concurrency: 1,
            };
            let limit: usize = args.get("limit").and_then(|v| v.parse().ok()).unwrap_or(10);
            let root = repo_root();
            let rtis = RtisPocChannel::from_repo_root(&root);
            let unified = UnifiedSearchChannel {
                rp_kc: root.join("target/debug/rp-kc"),
            };
            let query = MatchQuery {
                target_ref: target.clone(),
                components,
                profile_versions,
                limit,
            };
            let matched = run_match(
                &[
                    &unified as &dyn KnowledgeChannel,
                    &rtis as &dyn KnowledgeChannel,
                ],
                &query,
                limit,
            );
            let draft = draft_from_match(&matched, &budget, &store, |h| {
                if h.channel == "rtis-poc" {
                    format!("rtis-poc:{}", h.ref_id)
                } else {
                    format!("knowledge:{}", h.ref_id)
                }
            }, &params_json, "-");
            match persist_draft(&store, &draft, task.as_deref(), "default", title.as_deref()) {
                Ok(plan_id) => {
                    emit(&serde_json::json!({
                        "plan_id": plan_id,
                        "preview": draft,
                        "match_channels": matched.channels,
                    }));
                    0
                }
                Err(e) => emit_error("plan_build", &e.to_string()),
            }
        }
        "fingerprint" => {
            let (target, endpoint, poc, provider) = match (
                args.required("target"),
                args.required("endpoint"),
                args.required("poc"),
                args.required("provider"),
            ) {
                (Ok(a), Ok(b), Ok(c), Ok(d)) => (a, b, c, d),
                _ => return usage(),
            };
            let value = dedup_fingerprint(
                &target,
                &endpoint,
                &poc,
                &provider,
                args.get("params").unwrap_or("{}"),
                args.get("authctx").unwrap_or("-"),
            );
            emit(&serde_json::json!({ "dedup_fingerprint": value }));
            0
        }
        other => {
            eprintln!("unknown subcommand: {other}");
            usage()
        }
    }
}
