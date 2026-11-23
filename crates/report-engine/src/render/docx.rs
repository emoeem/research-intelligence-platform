//! DOCX 渲染器（PHASE-6B D3：`render_docx`——IR→OOXML（手写序列化）+
//! stored-ZIP 容器，零 I/O 零 AI）。
//!
//! 最小 OOXML 包（D3②）：
//! - `[Content_Types].xml`（parts 声明）；
//! - `_rels/.rels`（officeDocument 关系指向 word/document.xml）；
//! - `word/document.xml`（标题/段落/表格三 XML 面，escape 单点复用——XML
//!   转义与 HTML 转义同五字符集 `< > & " '`，6A escape.rs:1 词形）。
//!
//! D3④ 登记取舍：DOCX 零字体零图片面（文本 XML 由 Word 端解析渲染——中文
//! 零嵌入负担，:670 中文正确 = XML UTF-8 原样 + qc_docx 断言）。
//!
//! 五段投影与 render_markdown/render_pdf 同构（6A D5 词形）。rtis:// 引用
//! 只读透传（红线 2——PDF/DOCX/SVG 产物中 `rtis://` 词形原样出现，零解析
//! 零改写）。

use crate::ir::ReportIr;
use crate::ooxml::OoxmlZip;

/// XML 转义单点（复用 escape.rs——D3 词形）。
fn xml_escape(input: &str) -> String {
    crate::escape::escape_html(input)
}

/// `word/document.xml` 组装（手写序列化——标题/段落/表格）。
fn document_xml(ir: &ReportIr) -> String {
    let mut out = String::with_capacity(16 * 1024);
    out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n");
    out.push_str(
        "<w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\">",
    );
    out.push_str("<w:body>");
    // 报告头（标题 + 元数据）。
    heading(&mut out, 1, &ir.title);
    para(&mut out, &format!("报告 ID：{}", ir.report_id));
    para(&mut out, &format!("报告类型：{}", ir.report_kind));
    para(&mut out, &format!("生成时间：{}", ir.generated_at));
    para(&mut out, &format!("语言：{}", ir.language));
    if let Some(task_ref) = &ir.task_ref {
        if let Some(task_id) = &task_ref.task_id {
            para(&mut out, &format!("研究任务：{task_id}"));
        }
        if let Some(run_id) = &task_ref.workflow_run_id {
            para(&mut out, &format!("工作流运行：{run_id}"));
        }
    }
    if let Some(scope) = &ir.scope {
        if let Some(auth) = &scope.authorization {
            para(&mut out, &format!("授权范围：{auth}"));
        }
        for asset in &scope.assets {
            para(&mut out, &format!("资产：{asset}"));
        }
    }
    // 执行摘要。
    heading(&mut out, 2, "执行摘要");
    if let Some(body) = ir.executive_summary.as_deref() {
        for para_line in body.split('\n') {
            para(&mut out, para_line);
        }
    }
    // Finding 清单（表格面——D3② 三 XML 面之一）。
    heading(&mut out, 2, "Finding 清单");
    if ir.findings.is_empty() {
        para(&mut out, "（无 Finding）");
    } else {
        let header = ["ID", "标题", "严重级", "结论状态", "类型"];
        let rows: Vec<Vec<&str>> = ir
            .findings
            .iter()
            .map(|f| {
                vec![
                    f.finding_id.as_str(),
                    f.title.as_str(),
                    f.severity.as_str(),
                    f.result_state.as_str(),
                    f.vuln_type.as_str(),
                ]
            })
            .collect();
        table(&mut out, &header, &rows);
    }
    // Finding 详情。
    heading(&mut out, 2, "Finding 详情");
    for finding in &ir.findings {
        heading(
            &mut out,
            3,
            &format!(
                "{}：{}（{} / {}）",
                finding.finding_id, finding.title, finding.severity, finding.result_state
            ),
        );
        para(&mut out, &format!("漏洞类型：{}", finding.vuln_type));
        if let Some(rtis_ref) = &finding.rtis_ref {
            // rtis:// 引用只读透传——词形原样，零解析零改写（红线 2）。
            para(&mut out, &format!("RTIS 引用：{rtis_ref}"));
        }
        if let Some(location) = &finding.location {
            if let Some(uri) = &location.artifact_uri {
                para(&mut out, &format!("位置：{uri}"));
            }
            if let (Some(start), Some(end)) = (location.start_line, location.end_line) {
                para(&mut out, &format!("行区间：{start}-{end}"));
            }
            if let Some(snippet) = &location.snippet {
                para(&mut out, "代码片段：");
                for code_line in snippet.split('\n') {
                    para(&mut out, code_line);
                }
            }
        }
        for scope_line in &finding.affected_scope {
            para(&mut out, &format!("受影响范围：{scope_line}"));
        }
        markdown_field(&mut out, "前置条件", finding.preconditions.as_deref());
        markdown_field(&mut out, "原理", finding.principle.as_deref());
        markdown_field(&mut out, "复现步骤", finding.repro_steps.as_deref());
        for rr in &finding.request_response {
            para(&mut out, &format!("请求/响应（{}）：", rr.direction));
            for content_line in rr.content.split('\n') {
                para(&mut out, content_line);
            }
        }
        for evidence in &finding.evidence {
            para(
                &mut out,
                &format!(
                    "证据 {}：{}（{}）",
                    evidence.evidence_id, evidence.uri, evidence.content_hash
                ),
            );
        }
        if let Some(impact) = finding.impact.as_deref() {
            markdown_field(&mut out, "影响", Some(impact));
        }
        if let Some(remediation) = &finding.remediation {
            para(&mut out, &format!("修复优先级：{}", remediation.priority));
            for suggestion in &remediation.suggestions {
                para(&mut out, &format!("修复建议：{suggestion}"));
            }
        }
    }
    // 攻击路径。
    heading(&mut out, 2, "攻击路径");
    if ir.attack_paths.is_empty() {
        para(&mut out, "（无攻击路径）");
    } else {
        for path in &ir.attack_paths {
            heading(&mut out, 3, &format!("{}：{}", path.path_id, path.title));
            for (idx, step) in path.steps.iter().enumerate() {
                para(&mut out, &format!("{}. {step}", idx + 1));
            }
            for evidence_ref in &path.evidence_refs {
                para(&mut out, &format!("证据引用：{evidence_ref}"));
            }
        }
    }
    // 附录。
    heading(&mut out, 2, "附录");
    if let Some(body) = ir.appendix.as_deref() {
        for para_line in body.split('\n') {
            para(&mut out, para_line);
        }
    }
    // 节属性（A4 纵向——最小合法面）。
    out.push_str(
        "<w:sectPr><w:pgSz w:w=\"11906\" w:h=\"16838\"/><w:pgMar w:top=\"1134\" w:right=\"1134\" w:bottom=\"1134\" w:left=\"1134\"/></w:sectPr>",
    );
    out.push_str("</w:body></w:document>");
    out
}

/// markdown 片段字段（标题行 + 原文——markdown.rs 同位词形）。
fn markdown_field(out: &mut String, label: &str, body: Option<&str>) {
    let Some(body) = body else { return };
    if body.is_empty() {
        return;
    }
    para(out, &format!("{label}："));
    for para_line in body.split('\n') {
        para(out, para_line);
    }
}

/// 标题段落（Heading1-3 pStyle——最小样式面）。
fn heading(out: &mut String, level: u8, text: &str) {
    out.push_str(&format!(
        "<w:p><w:pPr><w:pStyle w:val=\"Heading{level}\"/></w:pPr><w:r><w:t xml:space=\"preserve\">{}</w:t></w:r></w:p>",
        xml_escape(text)
    ));
}

/// 普通段落。
fn para(out: &mut String, text: &str) {
    out.push_str(&format!(
        "<w:p><w:r><w:t xml:space=\"preserve\">{}</w:t></w:r></w:p>",
        xml_escape(text)
    ));
}

/// 表格（w:tbl——D3② 三 XML 面之一；表头行 + 数据行）。
fn table(out: &mut String, header: &[&str], rows: &[Vec<&str>]) {
    out.push_str(
        "<w:tbl><w:tblPr><w:tblBorders><w:top w:val=\"single\"/><w:left w:val=\"single\"/><w:bottom w:val=\"single\"/><w:right w:val=\"single\"/><w:insideH w:val=\"single\"/><w:insideV w:val=\"single\"/></w:tblBorders></w:tblPr>",
    );
    table_row(out, header);
    for row in rows {
        table_row(out, row);
    }
    out.push_str("</w:tbl>");
}

/// 表格单行（每格一 w:tc 内嵌 w:p）。
fn table_row(out: &mut String, cells: &[&str]) {
    out.push_str("<w:tr>");
    for cell in cells {
        out.push_str(&format!(
            "<w:tc><w:tcPr><w:tcW w:w=\"0\" w:type=\"auto\"/></w:tcPr><w:p><w:r><w:t xml:space=\"preserve\">{}</w:t></w:r></w:p></w:tc>",
            xml_escape(cell)
        ));
    }
    out.push_str("</w:tr>");
}

/// `[Content_Types].xml` 组装（D3② 最小合法面——rels/xml Default +
/// document.xml Override）。
fn content_types_xml() -> String {
    let mut out = String::with_capacity(512);
    out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n");
    out.push_str("<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\">");
    out.push_str("<Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/>");
    out.push_str("<Default Extension=\"xml\" ContentType=\"application/xml\"/>");
    out.push_str("<Override PartName=\"/word/document.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml\"/>");
    out.push_str("</Types>");
    out
}

/// `_rels/.rels` 组装（officeDocument 关系——最小合法面）。
fn rels_xml() -> String {
    let mut out = String::with_capacity(512);
    out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n");
    out.push_str("<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">");
    out.push_str("<Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" Target=\"word/document.xml\"/>");
    out.push_str("</Relationships>");
    out
}

/// IR → DOCX 字节（确定性纯函数——同 IR 两次渲染字节一致）。
///
/// 条目序（[Content_Types].xml 恒首位惯例）：[Content_Types].xml →
/// _rels/.rels → word/document.xml。stored-ZIP 容器经 [`OoxmlZip`]（D3③
/// XLSX/PPTX 第二批复用底座）。
pub fn render_docx(ir: &ReportIr) -> Vec<u8> {
    let mut zip = OoxmlZip::new();
    zip.add("[Content_Types].xml", content_types_xml());
    zip.add("_rels/.rels", rels_xml());
    zip.add("word/document.xml", document_xml(ir));
    zip.build()
}
