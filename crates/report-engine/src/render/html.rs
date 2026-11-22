//! HTML 渲染器（PHASE-6A D4/D5：`render_html`——IR→完整 HTML 文档
//! `<!DOCTYPE html>`，markdown 片段位经 comrak `markdown_to_html`，零模板
//! 引擎手写拼接，文本/属性插值单点走 [`crate::escape::escape_html`]，零 I/O）。
//!
//! 报告结构五段固定（:517 §8.2 B 类骨架投影）：报告头 / 执行摘要 / Finding
//! 清单 / Finding 详情 / 附录（攻击路径并入详情后独立小节）。

use crate::escape::escape_html;
use crate::ir::{Coverage, FindingIr, ReportIr};
use std::fmt::Write as _;

/// IR → 完整 HTML 文档（确定性纯函数——同 IR 两次渲染字节一致）。
pub fn render_html(ir: &ReportIr) -> String {
    let mut out = String::with_capacity(8192);
    let _ = write!(
        out,
        "<!DOCTYPE html>\n<html lang=\"{}\">\n<head>\n<meta charset=\"utf-8\">\n<title>{}</title>\n</head>\n<body>\n",
        escape_html(&ir.language),
        escape_html(&ir.title)
    );
    render_header_section(&mut out, ir);
    render_markdown_section(
        &mut out,
        "executive-summary",
        "执行摘要",
        ir.executive_summary.as_deref(),
    );
    render_findings_list_section(&mut out, ir);
    render_finding_details_section(&mut out, ir);
    render_attack_paths_section(&mut out, ir);
    render_markdown_section(&mut out, "appendix", "附录", ir.appendix.as_deref());
    let _ = write!(out, "</body>\n</html>\n");
    out
}

/// 报告头段（标题 + 元数据清单 + 范围 + 方法论 + 时间线 + 覆盖矩阵）。
fn render_header_section(out: &mut String, ir: &ReportIr) {
    let _ = writeln!(
        out,
        "<header id=\"report-header\">\n<h1>{}</h1>\n<ul>",
        escape_html(&ir.title)
    );
    let _ = writeln!(out, "<li>报告 ID：{}</li>", escape_html(&ir.report_id));
    let _ = writeln!(out, "<li>报告类型：{}</li>", escape_html(&ir.report_kind));
    let _ = writeln!(out, "<li>生成时间：{}</li>", escape_html(&ir.generated_at));
    let _ = writeln!(out, "<li>语言：{}</li>", escape_html(&ir.language));
    if let Some(task_ref) = &ir.task_ref {
        if let Some(task_id) = &task_ref.task_id {
            let _ = writeln!(out, "<li>研究任务：{}</li>", escape_html(task_id));
        }
        if let Some(workflow_run_id) = &task_ref.workflow_run_id {
            let _ = writeln!(out, "<li>工作流运行：{}</li>", escape_html(workflow_run_id));
        }
    }
    if let Some(scope) = &ir.scope {
        if let Some(authorization) = &scope.authorization {
            let _ = writeln!(out, "<li>授权范围：{}</li>", escape_html(authorization));
        }
        for asset in &scope.assets {
            let _ = writeln!(out, "<li>资产：{}</li>", escape_html(asset));
        }
    }
    let _ = writeln!(out, "</ul>\n</header>");
    render_markdown_section(out, "methodology", "方法论", ir.methodology.as_deref());
    render_timeline_section(out, ir);
    render_coverage_section(out, ir);
}

/// markdown 全文段小节（comrak 片段嵌入——D5；空体渲染合法空节）。
fn render_markdown_section(out: &mut String, id: &str, title: &str, body: Option<&str>) {
    let _ = writeln!(
        out,
        "<section id=\"{}\">\n<h2>{}</h2>\n",
        escape_html(id),
        escape_html(title)
    );
    match body {
        Some(body) if !body.is_empty() => {
            let _ = writeln!(out, "{}", markdown_fragment(body));
        }
        _ => {
            let _ = writeln!(out, "<p>（空）</p>");
        }
    }
    let _ = writeln!(out, "</section>");
}

/// 时间线表格。
fn render_timeline_section(out: &mut String, ir: &ReportIr) {
    let _ = writeln!(out, "<section id=\"timeline\">\n<h2>时间线</h2>");
    if ir.timeline.is_empty() {
        let _ = writeln!(out, "<p>（无时间线条目）</p>\n</section>");
        return;
    }
    let _ = writeln!(out, "<table>\n<tr><th>日期</th><th>事件</th></tr>");
    for item in &ir.timeline {
        let _ = writeln!(
            out,
            "<tr><td>{}</td><td>{}</td></tr>",
            escape_html(&item.date),
            escape_html(&item.event)
        );
    }
    let _ = writeln!(out, "</table>\n</section>");
}

/// 七维覆盖矩阵表格（022 投影位）。
fn render_coverage_section(out: &mut String, ir: &ReportIr) {
    let _ = writeln!(out, "<section id=\"coverage\">\n<h2>覆盖矩阵</h2>");
    let Some(coverage) = &ir.coverage else {
        let _ = writeln!(out, "<p>（无覆盖矩阵数据）</p>\n</section>");
        return;
    };
    render_coverage_table(out, coverage);
    let _ = writeln!(out, "</section>");
}

/// 覆盖矩阵表格体（维度词表七值——022 同源）。
fn render_coverage_table(out: &mut String, coverage: &Coverage) {
    if coverage.dimensions.is_empty() {
        let _ = writeln!(out, "<p>（覆盖矩阵无维度条目）</p>");
        return;
    }
    let _ = writeln!(
        out,
        "<table>\n<tr><th>维度</th><th>总数</th><th>已测</th><th>确认</th></tr>"
    );
    for dim in &coverage.dimensions {
        let _ = writeln!(
            out,
            "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
            escape_html(&dim.dimension),
            dim.total,
            dim.tested,
            dim.confirmed
        );
    }
    let _ = writeln!(out, "</table>");
}

/// Finding 清单段（表格——finding 数三面一致对账面之一）。
fn render_findings_list_section(out: &mut String, ir: &ReportIr) {
    let _ = writeln!(out, "<section id=\"findings-list\">\n<h2>Finding 清单</h2>");
    if ir.findings.is_empty() {
        let _ = writeln!(out, "<p>（无 Finding）</p>\n</section>");
        return;
    }
    let _ = writeln!(
        out,
        "<table>\n<tr><th>ID</th><th>标题</th><th>严重级</th><th>结论状态</th><th>类型</th></tr>"
    );
    for finding in &ir.findings {
        let _ = writeln!(
            out,
            "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
            escape_html(&finding.finding_id),
            escape_html(&finding.title),
            escape_html(&finding.severity),
            escape_html(&finding.result_state),
            escape_html(&finding.vuln_type)
        );
    }
    let _ = writeln!(out, "</table>\n</section>");
}

/// Finding 详情段（每 finding 全字段——rtis_ref/severity/六 ID 三面 grep
/// 对账面之一）。
fn render_finding_details_section(out: &mut String, ir: &ReportIr) {
    let _ = writeln!(
        out,
        "<section id=\"finding-details\">\n<h2>Finding 详情</h2>"
    );
    if ir.findings.is_empty() {
        let _ = writeln!(out, "<p>（无 Finding 详情）</p>\n</section>");
        return;
    }
    for finding in &ir.findings {
        render_finding(out, finding);
    }
    let _ = writeln!(out, "</section>");
}

/// 单个 Finding 详情块。
fn render_finding(out: &mut String, finding: &FindingIr) {
    let _ = writeln!(
        out,
        "<article class=\"finding finding-{}\">\n<h3>{}：{}（{} / {}）</h3>\n<ul>",
        escape_html(&finding.result_state),
        escape_html(&finding.finding_id),
        escape_html(&finding.title),
        escape_html(&finding.severity),
        escape_html(&finding.result_state)
    );
    let _ = writeln!(
        out,
        "<li>漏洞类型：{}</li>",
        escape_html(&finding.vuln_type)
    );
    if let Some(rtis_ref) = &finding.rtis_ref {
        let _ = writeln!(out, "<li>RTIS 引用：{}</li>", escape_html(rtis_ref));
    }
    if let Some(location) = &finding.location {
        if let Some(uri) = &location.artifact_uri {
            let _ = writeln!(out, "<li>位置：{}</li>", escape_html(uri));
        }
        if let (Some(start), Some(end)) = (location.start_line, location.end_line) {
            let _ = writeln!(out, "<li>行区间：{start}-{end}</li>");
        }
        if let Some(snippet) = &location.snippet {
            let _ = writeln!(
                out,
                "<li>代码片段：<pre><code>{}</code></pre></li>",
                escape_html(snippet)
            );
        }
    }
    for scope in &finding.affected_scope {
        let _ = writeln!(out, "<li>受影响范围：{}</li>", escape_html(scope));
    }
    if let Some(remediation) = &finding.remediation {
        let _ = writeln!(
            out,
            "<li>修复优先级：{}</li>",
            escape_html(&remediation.priority)
        );
        for suggestion in &remediation.suggestions {
            let _ = writeln!(out, "<li>修复建议：{}</li>", escape_html(suggestion));
        }
    }
    let _ = writeln!(out, "</ul>");
    render_request_response_block(out, finding);
    render_evidence_block(out, finding);
    render_markdown_field(
        out,
        "preconditions",
        "前置条件",
        finding.preconditions.as_deref(),
    );
    render_markdown_field(out, "principle", "原理", finding.principle.as_deref());
    render_markdown_field(
        out,
        "repro-steps",
        "复现步骤",
        finding.repro_steps.as_deref(),
    );
    render_markdown_field(out, "impact", "影响", finding.impact.as_deref());
    render_markdown_field(
        out,
        "technical-explanation",
        "技术解释",
        finding.technical_explanation.as_deref(),
    );
    render_markdown_field(
        out,
        "plain-explanation",
        "通俗解释",
        finding.plain_explanation.as_deref(),
    );
    let _ = writeln!(out, "</article>");
}

/// markdown 片段字段块（comrak 片段嵌入——D5；None/空体跳过）。
fn render_markdown_field(out: &mut String, id: &str, label: &str, body: Option<&str>) {
    let Some(body) = body else {
        return;
    };
    if body.is_empty() {
        return;
    }
    let _ = writeln!(
        out,
        "<div class=\"finding-field\" id=\"{}\">\n<h4>{}</h4>\n{}\n</div>",
        escape_html(id),
        escape_html(label),
        markdown_fragment(body).trim_end()
    );
}

/// 请求/响应对块（pre/code 转义形态）。
fn render_request_response_block(out: &mut String, finding: &FindingIr) {
    for rr in &finding.request_response {
        let _ = writeln!(
            out,
            "<div class=\"request-response\">\n<h4>请求/响应（{}）</h4>\n<pre><code>{}</code></pre>\n</div>",
            escape_html(&rr.direction),
            escape_html(&rr.content)
        );
    }
}

/// 证据清单块（evidence_id/uri/content_hash 词形三面 grep 对账面之一）。
fn render_evidence_block(out: &mut String, finding: &FindingIr) {
    if finding.evidence.is_empty() {
        return;
    }
    let _ = writeln!(out, "<div class=\"evidence\">\n<h4>证据</h4>\n<ul>");
    for evidence in &finding.evidence {
        let _ = writeln!(
            out,
            "<li>证据 ID：{}（<code>{}</code>，sha-256：<code>{}</code>）</li>",
            escape_html(&evidence.evidence_id),
            escape_html(&evidence.uri),
            escape_html(&evidence.content_hash)
        );
        let _ = writeln!(
            out,
            "<li>工具运行 ID：<code>{}</code></li>",
            escape_html(&evidence.tool_run_id)
        );
        let _ = writeln!(
            out,
            "<li>追踪 ID：<code>{}</code></li>",
            escape_html(&evidence.trace_id)
        );
        let _ = writeln!(
            out,
            "<li>产物 ID：<code>{}</code></li>",
            escape_html(&evidence.artifact_id)
        );
    }
    let _ = writeln!(out, "</ul>\n</div>");
}

/// 攻击路径段（只承载引用——:531）。
fn render_attack_paths_section(out: &mut String, ir: &ReportIr) {
    let _ = writeln!(out, "<section id=\"attack-paths\">\n<h2>攻击路径</h2>");
    if ir.attack_paths.is_empty() {
        let _ = writeln!(out, "<p>（无攻击路径）</p>\n</section>");
        return;
    }
    for path in &ir.attack_paths {
        let _ = writeln!(
            out,
            "<article class=\"attack-path\">\n<h3>{}：{}</h3>\n<ol>",
            escape_html(&path.path_id),
            escape_html(&path.title)
        );
        for step in &path.steps {
            let _ = writeln!(out, "<li>{}</li>", escape_html(step));
        }
        let _ = writeln!(out, "</ol>\n<ul>");
        for evidence_ref in &path.evidence_refs {
            let _ = writeln!(out, "<li>证据引用：{}</li>", escape_html(evidence_ref));
        }
        let _ = writeln!(out, "</ul>\n</article>");
    }
    let _ = writeln!(out, "</section>");
}

/// comrak markdown→HTML 片段（D5——Options::default() 免 bon 可用，A12 冒烟实测）。
fn markdown_fragment(md: &str) -> String {
    comrak::markdown_to_html(md, &comrak::Options::default())
}
