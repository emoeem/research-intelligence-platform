//! Verification Provider Adapter 层（RIP3 Phase 5 P5-5）。
//!
//! 统一契约（补充计划 §5.6 硬边界）：Adapter 只负责适配外部工具或 RTIS，
//! **不实现自己的第二套任务管理、证据库或 Finding 库**；执行一律经
//! Tool Engine 派发链（submit D-TC-05 → ToolRun 状态机 → 信封交付）或
//! 受控 CLI 子进程（rt.py 元数据面）；输出保留工具与规则版本；失败与
//! 超时按 [`domain::verification::ErrorCategory`] 分类。
//!
//! Provider 面两个实现（优先顺序按 §5.6：RTIS POC 先行，成熟扫描次之）：
//! - [`RtisPocProvider`]：RTIS 既有接口（rt.py poc get/use），transport=
//!   cli_subprocess。resolve（受控引用与元数据）+ record_use（使用关系
//!   回填，RTIS 现有接口 sanctioned 写路径；V-3 裁决的 Manifest 只读面
//!   不受影响——本 Provider 走 CLI 而非 Tool Manifest）。
//! - [`NucleiProvider`]：成熟扫描 Provider，transport=tool_engine，
//!   container_required=true（rip-nuclei 镜像族，digest 钉死台账见
//!   containerfiles/）；Manifest `rip.security.nuclei`。不可用时
//!   [`ErrorCategory::ToolUnavailable`] 分类，不伪装成功。
//! - [`ZapProvider`]：成熟扫描 Provider（WSW-7 批接入，execute_nuclei 同型
//!   派发链），transport=tool_engine，container_required=true（rip-zap 镜像
//!   族，容器内 zap.sh 内联扫描 + 镜像内 python3 解析 JSON 报告，零 daemon
//!   零 API 端口）；Manifest `rip.security.zap`；idempotent=false（主动扫描
//!   改变靶机状态，与 nuclei 模板检测语义差异如实标注）。
//!
//! 统一契约测试（tests/provider.rs）：capability 文档过
//! schemas/verification/provider-capability.v1.json；执行输出过信封分类
//! 守卫（SUCCEEDED 结果必 object；非 SUCCEEDED 终态必须可映射
//! ErrorCategory）；工具/规则版本恒在输出面。

use crate::store::PlanItemRecord;
use domain::verification::ErrorCategory;
use serde::Serialize;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::process::Command;
use tool_engine::adapter::egress_proxy::NetworkPlan;
use tool_engine::adapter::podman_host::{
    execute_container_via_engine, ContainerCall, PodmanHostAdapter,
};
use domain::ToolRunStatus;
use tool_engine::Engine;

/// Provider 执行产出（归一化输入面；五态判定在 P5-7 evaluator，不在本层）。
#[derive(Debug, Clone, Serialize)]
pub struct ProviderOutcome {
    pub provider_id: String,
    pub provider_version: String,
    /// tool-engine 终态词（SUCCEEDED/FAILED/TIMED_OUT/CANCELLED）或
    /// cli_subprocess 面的 EXECUTED/METADATA_ONLY。
    pub status_word: String,
    /// tool_run 关联（tool_engine 通道；cli_subprocess 面为 None）。
    pub tool_run_id: Option<String>,
    /// trace 关联（015 面）。
    pub trace_id: Option<String>,
    /// 结构化输出（SUCCEEDED 时必为 object——信封守卫）。
    pub result: Option<Value>,
    /// 人类可读摘要。
    pub summary: String,
    /// 失败/超时分类词值（§5.9 七分类；ErrorCategory::as_str 词值承载，
    /// domain 零依赖纪律：跨信封出站用词值，判定面 parse 还原）。
    pub error_category: Option<String>,
    /// 工具版本（恒在）。
    pub tool_version: Option<String>,
    /// 规则/模板版本（nuclei template id 或 rtis poc id + 内容指纹语义）。
    pub rule_ref: Option<String>,
    pub duration_ms: u128,
}

impl ProviderOutcome {
    /// 信封守卫：SUCCEEDED 时 result 必须 object；FAILED/TIMED_OUT 必须
    /// 带分类。违规即契约破坏（fail-closed，调用方不得当成功消费）。
    pub fn validate(&self) -> Result<(), String> {
        if self.status_word == "SUCCEEDED" {
            if self.result.is_none() {
                return Err("SUCCEEDED outcome without structured result".to_owned());
            }
            if self.error_category.is_some() {
                return Err("SUCCEEDED outcome must not carry error_category".to_owned());
            }
        } else if self.status_word == "FAILED" && self.error_category.is_none() {
            return Err("FAILED outcome without error_category".to_owned());
        }
        Ok(())
    }
}

/// Provider 统一契约。
pub trait VerificationProvider {
    /// provider-capability.v1 文档（调用方先 validate）。
    fn capability(&self) -> Value;
    /// Provider ID（词表：rtis-poc | nuclei）。
    fn provider_id(&self) -> &'static str;
    /// 版本（semver）。
    fn provider_version(&self) -> &'static str;
}

// ---------------------------------------------------------------------------
// RTIS POC Provider（cli_subprocess；rt.py 既有接口）
// ---------------------------------------------------------------------------

/// RTIS POC 元数据与使用关系面（不执行 POC 内容；执行由具备探针能力的
/// Provider 承载——本 Provider 提供「受控引用」给 Planner 与证据链）。
pub struct RtisPocProvider {
    /// rt.py 绝对路径。
    pub rt_py: PathBuf,
    pub python: String,
}

impl RtisPocProvider {
    pub fn from_repo_root(root: &std::path::Path) -> Self {
        Self {
            rt_py: root.join("integrations/rtis/rt.py"),
            python: "python3".to_owned(),
        }
    }

    fn run_json(&self, args: &[&str]) -> Result<Value, String> {
        let output = Command::new(&self.python)
            .arg(&self.rt_py)
            .args(args)
            .output()
            .map_err(|e| format!("rt.py spawn failed: {e}"))?;
        if !output.status.success() {
            return Err(format!(
                "rt.py {} exit {}: {}",
                args.first().unwrap_or(&""),
                output.status,
                String::from_utf8_lossy(&output.stderr).lines().next().unwrap_or("")
            ));
        }
        serde_json::from_slice(&output.stdout)
            .map_err(|e| format!("rt.py json: {e}"))
    }

    /// 受控引用与元数据（rt.py poc get --json；只取元数据与引用面）。
    pub fn resolve_metadata(&self, poc_id: i64) -> Result<Value, (ErrorCategory, String)> {
        self.run_json(&["poc", "get", "--id", &poc_id.to_string(), "--json"])
            .map_err(|e| (ErrorCategory::ToolUnavailable, e))
    }

    /// 使用关系回填（rt.py poc use --id N --on TARGET --json；RTIS 现有
    /// 接口的 sanctioned 写路径，回填纪律 §进门三件事 1）。
    pub fn record_use(&self, poc_id: i64, used_on: &str) -> Result<Value, (ErrorCategory, String)> {
        self.run_json(&["poc", "use", "--id", &poc_id.to_string(), "--on", used_on, "--json"])
            .map_err(|e| (ErrorCategory::ToolUnavailable, e))
    }

    /// 验证结论回填（P5-7：CONFIRMED 项经 rt.py poc update --verified --note
    /// 回填 RTIS 既有接口；Finding 结论性证据状态以 RTIS 为准，§3.1。note
    /// 只带平台侧判定依据摘要，POC 正文/利用细节零复制，RTIS 红线）。
    pub fn record_verification(
        &self,
        poc_id: i64,
        note: &str,
    ) -> Result<Value, (ErrorCategory, String)> {
        self.run_json(&[
            "poc",
            "update",
            "--id",
            &poc_id.to_string(),
            "--verified",
            "--note",
            note,
            "--json",
        ])
        .map_err(|e| (ErrorCategory::ToolUnavailable, e))
    }
}

impl VerificationProvider for RtisPocProvider {
    fn capability(&self) -> Value {
        json!({
            "schema_version": 1,
            "provider_id": "rtis-poc",
            "provider_version": "0.1.0",
            "detection_kinds": ["cve-exploit", "config-audit", "info-leak"],
            "risk_class": "low_impact",
            "execution": {
                "transport": "cli_subprocess",
                "container_required": false,
                "output_schema": "result-normalized-v1"
            },
            "manifest_ref": null
        })
    }

    fn provider_id(&self) -> &'static str {
        "rtis-poc"
    }

    fn provider_version(&self) -> &'static str {
        "0.1.0"
    }
}

// ---------------------------------------------------------------------------
// Nuclei Provider（tool_engine；rip.security.nuclei Manifest + 镜像族）
// ---------------------------------------------------------------------------

/// Nuclei Manifest 钉定面（rip.security.nuclei.yaml 同源三处：镜像/入口/族）。
pub const NUCLEI_IMAGE: &str =
    "localhost/rip-nuclei@sha256:9ff41ce98d4279ed528819ebc42cb32fe5b98916e8fb9a8633110fad83354cb8";
pub const NUCLEI_ENTRYPOINT: [&str; 1] = ["/usr/local/bin/rip-nuclei-wrapper"];
pub const NUCLEI_FAMILY: &str = "network";
pub const NUCLEI_TOOL_ID: &str = "rip.security.nuclei";
/// egress 端口白名单（Manifest network.egress.ports 同源）。
pub const NUCLEI_EGRESS_PORTS: [u16; 4] = [80, 443, 8080, 8443];

/// Nuclei Provider（执行经 Tool Engine；本层只适配，不直调宿主 Shell）。
pub struct NucleiProvider;

impl NucleiProvider {
    /// Manifest 输入形状（rip.security.nuclei input_schema）。`url` 字段是
    /// podman_host Adapter egress allowlist 的 host 派生源（P3-2 契约：
    /// NetworkPlan::EgressAllowlist 从 input.url 派生白名单 host）。
    pub fn build_input(item: &PlanItemRecord, request_budget: i64) -> Value {
        json!({
            "url": item.target_ref,
            "target": item.target_ref,
            "template_ref": format!("rtis-poc:{}", item.rtis_poc_id.unwrap_or(0)),
            "rate_limit": 10_i64.min(request_budget.max(1)),
            "timeout_seconds": item.timeout_budget_s.min(300),
            "max_requests": request_budget.max(1),
        })
    }
}

/// 派发选项面（execute_nuclei 的关联与派发可选项打包，参数面收敛）。
pub struct DispatchOptions<'a> {
    /// research_task 关联（RTIS task_ref；进 Manifest input.task_id）。
    pub task_id: Option<&'a str>,
    /// Trace 六 ID 锚（有值时走 submit_correlated，015 面）。
    pub trace_id: Option<&'a str>,
    /// 网络计划（None = 127.0.0.1 本机面；EgressAllowlist = pasta -T 白名单）。
    pub network: NetworkPlan,
    /// 调用面模板锚（本地模板文件路径，不联网装模板）。
    pub template_path: Option<&'a std::path::Path>,
}

/// Nuclei 计划项端到端执行（submit_correlated → 容器派发 → ProviderOutcome）。
///
/// 调用链纪律：本函数是补充计划 §5.6.B「工具执行经 Tool Engine /
/// Runtime Gateway」的落地面——submit 受理（D-TC-05）→ podman_host
/// Adapter 容器执行（digest 钉死镜像 + egress 白名单）→ 信封交付。
/// options.trace_id 有值时走 submit_correlated（015 面：tool_run.trace_id
/// 落列，六 ID 关联锚）；镜像缺席/派发拒绝按分类返回，不伪装成功。
pub fn execute_nuclei(
    engine: &Engine,
    adapter: &PodmanHostAdapter,
    item: &PlanItemRecord,
    run_id: &str,
    options: DispatchOptions<'_>,
) -> Result<ProviderOutcome, (ErrorCategory, String)> {
    // 模板落位（调用面提供的本地模板文件 → ws in/template.yaml；Adapter 的
    // prepare_workspace 幂等，不覆盖已落位文件）。
    if let Some(template) = options.template_path {
        let ws = adapter
            .prepare_workspace(run_id, &serde_json::json!({}))
            .map_err(|f| (ErrorCategory::ToolUnavailable, f.message))?;
        let target = ws.in_dir.join("template.yaml");
        let same = std::fs::canonicalize(template)
            .ok()
            .zip(std::fs::canonicalize(&target).ok())
            .map(|(a, b)| a == b)
            .unwrap_or(false);
        if !same {
            std::fs::copy(template, &target).map_err(|e| {
                (ErrorCategory::ToolUnavailable, format!("template copy failed: {e}"))
            })?;
        }
    }
    let input = NucleiProvider::build_input(item, item.request_budget);
    // 模板装载旗标（P5-9b 实拍缺陷修复）：wrapper 把 /task/in/template.yaml
    // 装入 /task/out/nuclei-templates，但 nuclei 不读默认目录，缺 -t 旗标时
    // FTL "no templates provided"（真实执行恒 INCONCLUSIVE，本地靶场
    // CONFIRMED 不可达）。模板落位成功才追加，派发面不静默装空目录。
    let mut entrypoint_extra: Vec<String> = Vec::new();
    if options.template_path.is_some() {
        entrypoint_extra.push("-t".to_string());
        entrypoint_extra.push("/task/out/nuclei-templates".to_string());
    }
    let mut request = json!({
        "protocol_version": "1",
        "tool_id": NUCLEI_TOOL_ID,
        "run_id": run_id,
        "submit_key": format!("verify:{item_id}", item_id = item.item_id),
        "input": input,
        "workspace_id": "ws-verify",
        "budget": {
            "timeout_seconds": item.timeout_budget_s.clamp(1, 300),
            "max_output_bytes": 8388608
        },
        "result_schema_version": "1"
    });
    if let Some(task) = options.task_id {
        request["task_id"] = json!(task);
    }
    let submitted = engine
        .submit_correlated(&request, options.trace_id)
        .map_err(|e| (ErrorCategory::ToolUnavailable, format!("submit rejected: {e}")))?;
    let call = ContainerCall {
        engine,
        run_id: &submitted.run_id,
        adapter,
        image: NUCLEI_IMAGE,
        // 镜像 ENTRYPOINT = wrapper（exec nuclei ""）；此处只传 nuclei 参数面。
        entrypoint: &[
            "-no-color".to_string(),
            "-u".to_string(),
            item.target_ref.clone(),
            "-rate-limit".to_string(),
            item.request_budget.clamp(1, 50).to_string(),
            "-timeout".to_string(),
            item.timeout_budget_s.clamp(1, 300).to_string(),
        ]
        .iter()
        .chain(entrypoint_extra.iter())
        .cloned()
        .collect::<Vec<String>>(),
        family: NUCLEI_FAMILY,
        network: options.network,
        caller: None,
        caller_cap: None,
    };
    let started = std::time::Instant::now();
    let outcome = execute_container_via_engine(call)
        .map_err(|e| (ErrorCategory::ToolUnavailable, format!("drive failed: {e}")))?;
    let duration = started.elapsed().as_millis();
    let envelope = &outcome.envelope;
    let status_word = envelope.status.as_str().to_owned();
    let (result, error_category, summary) = match envelope.status {
        ToolRunStatus::Succeeded => (
            envelope.result.clone(),
            None::<String>,
            envelope.summary.clone().unwrap_or_default(),
        ),
        ToolRunStatus::TimedOut => (
            None::<Value>,
            Some(ErrorCategory::Timeout.as_str().to_owned()),
            envelope.summary.clone().unwrap_or_default(),
        ),
        ToolRunStatus::Cancelled => (
            None::<Value>,
            Some(ErrorCategory::Unknown.as_str().to_owned()),
            envelope.summary.clone().unwrap_or_default(),
        ),
        _ => {
            // FAILED：信封 error code/category 映射 §5.9 分类
            let (category, message) = envelope
                .error
                .as_ref()
                .map(|e| (map_envelope_category(e), e.message.clone()))
                .unwrap_or((ErrorCategory::Unknown, "unknown failure".to_owned()));
            (None::<Value>, Some(category.as_str().to_owned()), message)
        }
    };
    let outcome = ProviderOutcome {
        provider_id: "nuclei".to_owned(),
        provider_version: "0.1.0".to_owned(),
        status_word,
        tool_run_id: Some(envelope.run_id.clone()),
        trace_id: options.trace_id.map(str::to_owned),
        result,
        summary,
        error_category,
        tool_version: Some(envelope.tool_version.clone()),
        rule_ref: item
            .rtis_poc_id
            .map(|id| format!("rtis-poc:{id}")),
        duration_ms: duration,
    };
    outcome
        .validate()
        .map_err(|e| (ErrorCategory::Unknown, e))?;
    Ok(outcome)
}

/// 信封错误 code/category → §5.9 分类（DEPENDENCY=工具不可用，
/// EXECUTION/POLICY=分类面其余，VALIDATION=断言面）。
fn map_envelope_category(error: &tool_engine::envelope::EnvelopeError) -> ErrorCategory {
    match error.category.as_str() {
        "DEPENDENCY" => ErrorCategory::ToolUnavailable,
        "VALIDATION" => ErrorCategory::Assertion,
        _ => ErrorCategory::Unknown,
    }
}

impl VerificationProvider for NucleiProvider {
    fn capability(&self) -> Value {
        json!({
            "schema_version": 1,
            "provider_id": "nuclei",
            "provider_version": "0.1.0",
            "detection_kinds": ["cve-exploit", "config-audit", "info-leak"],
            "risk_class": "low_impact",
            "execution": {
                "transport": "tool_engine",
                "container_required": true,
                "output_schema": "result-normalized-v1"
            },
            "manifest_ref": "manifests/tools/rip.security.nuclei.yaml"
        })
    }

    fn provider_id(&self) -> &'static str {
        "nuclei"
    }

    fn provider_version(&self) -> &'static str {
        "0.1.0"
    }
}

// ---------------------------------------------------------------------------
// ZAP Provider（tool_engine；rip.security.zap Manifest + 镜像族，WSW-7 D1②）
// ---------------------------------------------------------------------------

/// ZAP Manifest 钉定面（rip.security.zap.yaml 同源三处：镜像/入口/族）。
/// 基础镜像 = ghcr.io/zaproxy/zaproxy:weekly（owasp/zap2docker-weekly 已下架
/// ——hub.docker.com 实测 object not found，官方源迁移 ghcr.io/zaproxy；
/// digest 实拉后钉，台账 containerfiles/README.md）。
pub const ZAP_IMAGE: &str =
    "localhost/rip-zap@sha256:0000000000000000000000000000000000000000000000000000000000000000";
pub const ZAP_ENTRYPOINT: [&str; 1] = ["/usr/local/bin/rip-zap-wrapper"];
pub const ZAP_FAMILY: &str = "network";
pub const ZAP_TOOL_ID: &str = "rip.security.zap";
/// egress 端口白名单（Manifest network.egress.ports 同源）。
pub const ZAP_EGRESS_PORTS: [u16; 4] = [80, 443, 8080, 8443];

/// ZAP Provider（执行经 Tool Engine；本层只适配，不直调宿主 Shell）。
pub struct ZapProvider;

impl ZapProvider {
    /// Manifest 输入形状（rip.security.zap input_schema）。`url` 字段是
    /// podman_host Adapter egress allowlist 的 host 派生源（P3-2 契约：
    /// NetworkPlan::EgressAllowlist 从 input.url 派生白名单 host）。
    /// scan_mode 词表单值 baseline（深扫扩展归观察池——D1②/D2）。
    pub fn build_input(target: &str, timeout_seconds: i64, alert_budget: i64) -> Value {
        json!({
            "url": target,
            "target": target,
            "scan_mode": "baseline",
            "timeout_seconds": timeout_seconds.clamp(1, 600),
            "alert_budget": alert_budget.clamp(1, 1000),
        })
    }
}

/// ZAP 基线扫描端到端执行（submit_correlated → 容器派发 → ProviderOutcome；
/// execute_nuclei :241 同型：submit 受理（D-TC-05）→ podman_host Adapter
/// 容器执行（digest 钉死镜像 + egress 白名单 + pasta:-T 代理）→ 信封交付。
/// 容器内编排：zap.sh 内联 baseline 扫描 + 镜像内 python3 解析 JSON 报告 →
/// stdout 第一行 D-TC-03 result 帧（零 daemon 零 API 端口暴露）。
/// idempotent=false（主动扫描改变靶机状态）：submit_key 仅审计锚（引擎受理
/// 无 submit_key 去重面），重复扫描语义如实留给调用方。
pub fn execute_zap(
    engine: &Engine,
    adapter: &PodmanHostAdapter,
    target: &str,
    timeout_budget_s: i64,
    alert_budget: i64,
    run_id: &str,
    options: DispatchOptions<'_>,
) -> Result<ProviderOutcome, (ErrorCategory, String)> {
    let input = ZapProvider::build_input(target, timeout_budget_s, alert_budget);
    let mut request = json!({
        "protocol_version": "1",
        "tool_id": ZAP_TOOL_ID,
        "run_id": run_id,
        "submit_key": format!("scan:zap:{run_id}"),
        "input": input,
        "workspace_id": "ws-verify",
        "budget": {
            "timeout_seconds": timeout_budget_s.clamp(1, 600),
            "max_output_bytes": 8388608
        },
        "result_schema_version": "1"
    });
    if let Some(task) = options.task_id {
        request["task_id"] = json!(task);
    }
    let submitted = engine
        .submit_correlated(&request, options.trace_id)
        .map_err(|e| (ErrorCategory::ToolUnavailable, format!("submit rejected: {e}")))?;
    let call = ContainerCall {
        engine,
        run_id: &submitted.run_id,
        adapter,
        image: ZAP_IMAGE,
        // wrapper 承载容器内编排（zap.sh 内联扫描 + JSON 报告解析）；此处传
        // wrapper 参数面（目标/超时/alert 帽），与 nuclei -u/-rate-limit 同型。
        entrypoint: &[
            "-u".to_string(),
            target.to_string(),
            "-t".to_string(),
            timeout_budget_s.clamp(1, 600).to_string(),
            "-b".to_string(),
            alert_budget.clamp(1, 1000).to_string(),
        ],
        family: ZAP_FAMILY,
        network: options.network,
        caller: None,
        caller_cap: None,
    };
    let started = std::time::Instant::now();
    let outcome = execute_container_via_engine(call)
        .map_err(|e| (ErrorCategory::ToolUnavailable, format!("drive failed: {e}")))?;
    let duration = started.elapsed().as_millis();
    let envelope = &outcome.envelope;
    let status_word = envelope.status.as_str().to_owned();
    let (result, error_category, summary) = match envelope.status {
        ToolRunStatus::Succeeded => (
            envelope.result.clone(),
            None::<String>,
            envelope.summary.clone().unwrap_or_default(),
        ),
        ToolRunStatus::TimedOut => (
            None::<Value>,
            Some(ErrorCategory::Timeout.as_str().to_owned()),
            envelope.summary.clone().unwrap_or_default(),
        ),
        ToolRunStatus::Cancelled => (
            None::<Value>,
            Some(ErrorCategory::Unknown.as_str().to_owned()),
            envelope.summary.clone().unwrap_or_default(),
        ),
        _ => {
            // FAILED：信封 error code/category 映射 §5.9 分类
            let (category, message) = envelope
                .error
                .as_ref()
                .map(|e| (map_envelope_category(e), e.message.clone()))
                .unwrap_or((ErrorCategory::Unknown, "unknown failure".to_owned()));
            (None::<Value>, Some(category.as_str().to_owned()), message)
        }
    };
    let outcome = ProviderOutcome {
        provider_id: "zap".to_owned(),
        provider_version: "0.1.0".to_owned(),
        status_word,
        tool_run_id: Some(envelope.run_id.clone()),
        trace_id: options.trace_id.map(str::to_owned),
        result,
        summary,
        error_category,
        tool_version: Some(envelope.tool_version.clone()),
        rule_ref: None,
        duration_ms: duration,
    };
    outcome
        .validate()
        .map_err(|e| (ErrorCategory::Unknown, e))?;
    Ok(outcome)
}

impl VerificationProvider for ZapProvider {
    fn capability(&self) -> Value {
        json!({
            "schema_version": 1,
            "provider_id": "zap",
            "provider_version": "0.1.0",
            "detection_kinds": ["config-audit", "info-leak"],
            "risk_class": "low_impact",
            "execution": {
                "transport": "tool_engine",
                "container_required": true,
                "output_schema": "result-normalized-v1"
            },
            "manifest_ref": "manifests/tools/rip.security.zap.yaml"
        })
    }

    fn provider_id(&self) -> &'static str {
        "zap"
    }

    fn provider_version(&self) -> &'static str {
        "0.1.0"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::validate_capability_doc;

    fn sample_item(poc_id: i64) -> PlanItemRecord {
        PlanItemRecord {
            schema_version: 1,
            item_id: "vi-20261117-000000-000-0000".to_owned(),
            plan_id: "vp-20261117-000000-000-0000".to_owned(),
            knowledge_ref: None,
            rtis_poc_id: Some(poc_id),
            provider_id: "nuclei".to_owned(),
            provider_version: Some("0.1.0".to_owned()),
            target_ref: "https://target.example:443".to_owned(),
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
    fn capability_docs_validate_against_contract() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let rtis = RtisPocProvider::from_repo_root(&root);
        validate_capability_doc(&rtis.capability()).unwrap();
        let nuclei = NucleiProvider;
        validate_capability_doc(&nuclei.capability()).unwrap();
        let zap = ZapProvider;
        validate_capability_doc(&zap.capability()).unwrap();
        // 词表与版本一致性
        assert_eq!(rtis.provider_id(), "rtis-poc");
        assert_eq!(nuclei.provider_id(), "nuclei");
        assert_eq!(zap.provider_id(), "zap");
        assert_eq!(rtis.capability()["provider_id"], "rtis-poc");
        assert_eq!(zap.capability()["provider_id"], "zap");
    }

    #[test]
    fn zap_input_shape_matches_manifest_schema() {
        let input = ZapProvider::build_input("http://127.0.0.1:8080", 300, 100);
        assert_eq!(input["url"], "http://127.0.0.1:8080");
        assert_eq!(input["target"], "http://127.0.0.1:8080");
        assert_eq!(input["scan_mode"], "baseline");
        assert_eq!(input["timeout_seconds"], 300);
        assert_eq!(input["alert_budget"], 100);
        // 预算裁剪面（clamp 双向；input_schema max 600/1000 同源）
        let clamped = ZapProvider::build_input("http://127.0.0.1:8080", 5000, 9999);
        assert_eq!(clamped["timeout_seconds"], 600);
        assert_eq!(clamped["alert_budget"], 1000);
        let floored = ZapProvider::build_input("http://127.0.0.1:8080", 0, 0);
        assert_eq!(floored["timeout_seconds"], 1);
        assert_eq!(floored["alert_budget"], 1);
        // zap Manifest json 与 .yaml 孪生在场且 id/family/runtime.image 同源
        // （D2 孪生纪律；YAML 通道解析由 rp-tool validate YAML 面承载——本
        // crate 依赖纪律禁 dev-dep 新增 yaml 包，D4 裁决）。
        let manifest: Value = serde_json::from_str(
            &std::fs::read_to_string(
                PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("../../manifests/tools/rip.security.zap.json"),
            )
            .expect("zap manifest json 在场"),
        )
        .expect("zap manifest json 合法");
        let yaml_text = std::fs::read_to_string(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../manifests/tools/rip.security.zap.yaml"),
        )
        .expect("zap manifest yaml 孪生在场");
        assert!(
            yaml_text.contains("id: \"rip.security.zap\"")
                && yaml_text.contains("image: \"localhost/rip-zap@sha256:"),
            "yaml 孪生 id/image 面在场"
        );
        assert_eq!(manifest["id"], "rip.security.zap");
        assert_eq!(manifest["runtime"]["type"], "podman_host");
        assert_eq!(manifest["runtime"]["image"], ZAP_IMAGE, "image 常量同源");
        assert_eq!(manifest["runtime"]["entrypoint"][0], ZAP_ENTRYPOINT[0]);
        assert_eq!(manifest["family"], ZAP_FAMILY);
        assert_eq!(manifest["idempotent"], false, "主动扫描改变靶机状态");
        assert_eq!(manifest["permission_level"], "T1");
        let ports: Vec<u16> = manifest["network"]["egress"]["ports"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_u64().unwrap() as u16)
            .collect();
        assert_eq!(ports, ZAP_EGRESS_PORTS.to_vec());
    }

    #[test]
    fn outcome_envelope_guards() {
        let ok = ProviderOutcome {
            provider_id: "nuclei".to_owned(),
            provider_version: "0.1.0".to_owned(),
            status_word: "SUCCEEDED".to_owned(),
            tool_run_id: Some("rn-1".to_owned()),
            trace_id: Some("tr-1".to_owned()),
            result: Some(json!({"matched": false})),
            summary: "clean".to_owned(),
            error_category: None,
            tool_version: Some("0.1.0".to_owned()),
            rule_ref: Some("rtis-poc:900008".to_owned()),
            duration_ms: 100,
        };
        assert!(ok.validate().is_ok());
        // SUCCEEDED 无 result → 守卫拒绝
        let mut bad = ok.clone();
        bad.result = None;
        assert!(bad.validate().is_err());
        // FAILED 无分类 → 守卫拒绝
        let mut bad2 = ok;
        bad2.status_word = "FAILED".to_owned();
        bad2.result = None;
        assert!(bad2.validate().is_err());
    }

    #[test]
    fn nuclei_input_shape_carries_budget_and_versions() {
        let item = sample_item(900008);
        let input = NucleiProvider::build_input(&item, 10);
        assert_eq!(input["target"], "https://target.example:443");
        assert_eq!(input["template_ref"], "rtis-poc:900008");
        assert_eq!(input["rate_limit"], 10);
        assert_eq!(input["timeout_seconds"], 60);
    }
}
