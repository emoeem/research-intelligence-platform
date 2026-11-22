//! Markdown 渲染器（PHASE-6A D4：`render_markdown`——IR→Markdown 全文，
//! 标题/清单/表格手写序列化，零 I/O 零 AI）。
//!
//! 结构五段（与 HTML 渲染器五段同构——D5 五段骨架）：报告头 / 执行摘要 /
//! Finding 清单 / Finding 详情 / 附录（攻击路径并入 Finding 清单后的独立
//! 小节，附录为末段）。

use crate::ir::{FindingIr, ReportIr};
use std::fmt::Write as _;

/// IR → Markdown 全文（确定性纯函数——同 IR 两次渲染字节一致）。
pub fn render_markdown(ir: &ReportIr) -> String {
    let mut out = String::with_capacity(4096);
    render_header(&mut out, ir);
    render_section(&mut out, "执行摘要", ir.executive_summary.as_deref());
    render_findings_list(&mut out, ir);
    render_finding_details(&mut out, ir);
    render_attack_paths(&mut out, ir);
    render_section(&mut out, "附录", ir.appendix.as_deref());
    out
}

/// 报告头段（标题 + 元数据清单 + 范围 + 方法论 + 时间线 + 覆盖矩阵）。
fn render_header(out: &mut String, ir: &ReportIr) {
    let _ = writeln!(out, "# {}", ir.title);
    let _ = writeln!(out);
    let _ = writeln!(out, "- 报告 ID：{}", ir.report_id);
    let _ = writeln!(out, "- 报告类型：{}", ir.report_kind);
    let _ = writeln!(out, "- 生成时间：{}", ir.generated_at);
    let _ = writeln!(out, "- 语言：{}", ir.language);
    if let Some(task_ref) = &ir.task_ref {
        if let Some(task_id) = &task_ref.task_id {
            let _ = writeln!(out, "- 研究任务：{task_id}");
        }
        if let Some(workflow_run_id) = &task_ref.workflow_run_id {
            let _ = writeln!(out, "- 工作流运行：{workflow_run_id}");
        }
    }
    if let Some(scope) = &ir.scope {
        if let Some(authorization) = &scope.authorization {
            let _ = writeln!(out, "- 授权范围：{authorization}");
        }
        for asset in &scope.assets {
            let _ = writeln!(out, "- 资产：{asset}");
        }
    }
    let _ = writeln!(out);
    render_section(out, "方法论", ir.methodology.as_deref());
    render_timeline(out, ir);
    render_coverage(out, ir);
}

/// 通用小节（标题 + markdown 原文段；None 时输出空节占位行——空集边界
/// 判据 9：MD 合法空节渲染）。
fn render_section(out: &mut String, title: &str, body: Option<&str>) {
    let _ = writeln!(out, "## {title}");
    let _ = writeln!(out);
    if let Some(body) = body {
        if !body.is_empty() {
            let _ = writeln!(out, "{body}");
            let _ = writeln!(out);
        }
    }
}

/// 时间线表格。
fn render_timeline(out: &mut String, ir: &ReportIr) {
    let _ = writeln!(out, "## 时间线");
    let _ = writeln!(out);
    if ir.timeline.is_empty() {
        let _ = writeln!(out, "（无时间线条目）");
        let _ = writeln!(out);
        return;
    }
    let _ = writeln!(out, "| 日期 | 事件 |");
    let _ = writeln!(out, "| --- | --- |");
    for item in &ir.timeline {
        let _ = writeln!(
            out,
            "| {} | {} |",
            table_cell(&item.date),
            table_cell(&item.event)
        );
    }
    let _ = writeln!(out);
}

/// 七维覆盖矩阵表格（022 投影位）。
fn render_coverage(out: &mut String, ir: &ReportIr) {
    let _ = writeln!(out, "## 覆盖矩阵");
    let _ = writeln!(out);
    let Some(coverage) = &ir.coverage else {
        let _ = writeln!(out, "（无覆盖矩阵数据）");
        let _ = writeln!(out);
        return;
    };
    if coverage.dimensions.is_empty() {
        let _ = writeln!(out, "（覆盖矩阵无维度条目）");
        let _ = writeln!(out);
        return;
    }
    let _ = writeln!(out, "| 维度 | 总数 | 已测 | 确认 |");
    let _ = writeln!(out, "| --- | --- | --- | --- |");
    for dim in &coverage.dimensions {
        let _ = writeln!(
            out,
            "| {} | {} | {} | {} |",
            table_cell(&dim.dimension),
            dim.total,
            dim.tested,
            dim.confirmed
        );
    }
    let _ = writeln!(out);
}

/// Finding 清单段（表格——finding 数三面一致对账面之一）。
fn render_findings_list(out: &mut String, ir: &ReportIr) {
    let _ = writeln!(out, "## Finding 清单");
    let _ = writeln!(out);
    if ir.findings.is_empty() {
        let _ = writeln!(out, "（无 Finding）");
        let _ = writeln!(out);
        return;
    }
    let _ = writeln!(out, "| ID | 标题 | 严重级 | 结论状态 | 类型 |");
    let _ = writeln!(out, "| --- | --- | --- | --- | --- |");
    for finding in &ir.findings {
        let _ = writeln!(
            out,
            "| {} | {} | {} | {} | {} |",
            table_cell(&finding.finding_id),
            table_cell(&finding.title),
            table_cell(&finding.severity),
            table_cell(&finding.result_state),
            table_cell(&finding.vuln_type)
        );
    }
    let _ = writeln!(out);
}

/// Finding 详情段（每 finding 全字段——rtis_ref/severity/六 ID 三面 grep
/// 对账面之一）。
fn render_finding_details(out: &mut String, ir: &ReportIr) {
    let _ = writeln!(out, "## Finding 详情");
    let _ = writeln!(out);
    if ir.findings.is_empty() {
        let _ = writeln!(out, "（无 Finding 详情）");
        let _ = writeln!(out);
        return;
    }
    for finding in &ir.findings {
        let _ = writeln!(
            out,
            "### {}：{}（{} / {}）",
            finding.finding_id, finding.title, finding.severity, finding.result_state
        );
        let _ = writeln!(out);
        let _ = writeln!(out, "- 漏洞类型：{}", finding.vuln_type);
        if let Some(rtis_ref) = &finding.rtis_ref {
            let _ = writeln!(out, "- RTIS 引用：{rtis_ref}");
        }
        if let Some(location) = &finding.location {
            if let Some(uri) = &location.artifact_uri {
                let _ = writeln!(out, "- 位置：{uri}");
            }
            if let (Some(start), Some(end)) = (location.start_line, location.end_line) {
                let _ = writeln!(out, "- 行区间：{start}-{end}");
            }
            if let Some(snippet) = &location.snippet {
                let _ = writeln!(out, "- 代码片段：");
                let _ = writeln!(out);
                let _ = writeln!(out, "```");
                let _ = writeln!(out, "{snippet}");
                let _ = writeln!(out, "```");
                let _ = writeln!(out);
            }
        }
        for scope in &finding.affected_scope {
            let _ = writeln!(out, "- 受影响范围：{scope}");
        }
        render_markdown_field(out, "前置条件", finding.preconditions.as_deref());
        render_markdown_field(out, "原理", finding.principle.as_deref());
        render_markdown_field(out, "复现步骤", finding.repro_steps.as_deref());
        render_request_response(out, finding);
        render_evidence(out, finding);
        render_markdown_field(out, "影响", finding.impact.as_deref());
        if let Some(remediation) = &finding.remediation {
            let _ = writeln!(out, "- 修复优先级：{}", remediation.priority);
            for suggestion in &remediation.suggestions {
                let _ = writeln!(out, "- 修复建议：{suggestion}");
            }
        }
        render_markdown_field(out, "技术解释", finding.technical_explanation.as_deref());
        render_markdown_field(out, "通俗解释", finding.plain_explanation.as_deref());
        let _ = writeln!(out);
    }
}

/// markdown 片段字段渲染（标题行 + 原文——HTML 面同位经 comrak，D5）。
fn render_markdown_field(out: &mut String, label: &str, body: Option<&str>) {
    let Some(body) = body else {
        return;
    };
    if body.is_empty() {
        return;
    }
    let _ = writeln!(out, "**{label}**：");
    let _ = writeln!(out);
    let _ = writeln!(out, "{body}");
    let _ = writeln!(out);
}

/// 请求/响应对（代码块形态）。
fn render_request_response(out: &mut String, finding: &FindingIr) {
    if finding.request_response.is_empty() {
        return;
    }
    for rr in &finding.request_response {
        let _ = writeln!(out, "**请求/响应（{}）**：", rr.direction);
        let _ = writeln!(out);
        let _ = writeln!(out, "```");
        let _ = writeln!(out, "{}", rr.content);
        let _ = writeln!(out, "```");
        let _ = writeln!(out);
    }
}

/// 证据清单（evidence_id/uri/content_hash 词形三面 grep 对账面之一）。
fn render_evidence(out: &mut String, finding: &FindingIr) {
    if finding.evidence.is_empty() {
        return;
    }
    let _ = writeln!(out, "**证据**：");
    let _ = writeln!(out);
    for evidence in &finding.evidence {
        let _ = writeln!(out, "- 证据 ID：{}", evidence.evidence_id);
        let _ = writeln!(out, "  - uri：{}", evidence.uri);
        let _ = writeln!(out, "  - 工具运行 ID：{}", evidence.tool_run_id);
        let _ = writeln!(out, "  - 追踪 ID：{}", evidence.trace_id);
        let _ = writeln!(out, "  - 产物 ID：{}", evidence.artifact_id);
        let _ = writeln!(out, "  - content_hash：{}", evidence.content_hash);
    }
    let _ = writeln!(out);
}

/// 攻击路径段（只承载引用——:531）。
fn render_attack_paths(out: &mut String, ir: &ReportIr) {
    let _ = writeln!(out, "## 攻击路径");
    let _ = writeln!(out);
    if ir.attack_paths.is_empty() {
        let _ = writeln!(out, "（无攻击路径）");
        let _ = writeln!(out);
        return;
    }
    for path in &ir.attack_paths {
        let _ = writeln!(out, "### {}：{}", path.path_id, path.title);
        let _ = writeln!(out);
        for (idx, step) in path.steps.iter().enumerate() {
            let _ = writeln!(out, "{}. {step}", idx + 1);
        }
        for evidence_ref in &path.evidence_refs {
            let _ = writeln!(out, "- 证据引用：{evidence_ref}");
        }
        let _ = writeln!(out);
    }
}

/// markdown 表格单元格内 `|` 转义（手写序列化的最小转义面）。
fn table_cell(input: &str) -> String {
    input.replace('|', "\\|")
}
