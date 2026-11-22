//! HTML 转义与中文面（PHASE-6A 判据 11）。
//!
//! escape_html 五字符穷举（`<>&"'`）+ 含中文 title/解释字段渲染 HTML 后
//! UTF-8 原样（:670 中文正确——中文字面在产物 grep 命中）+ comrak 片段面
//! `<h2>`/`<strong>` 标签产出实测（D5）。

use report_engine::escape::escape_html;
use report_engine::{render_html, render_markdown};
use serde_json::json;

/// 五字符穷举：`< > & " '` 逐一映射。
#[test]
fn escape_html_covers_five_reserved_chars() {
    assert_eq!(escape_html("<"), "&lt;");
    assert_eq!(escape_html(">"), "&gt;");
    assert_eq!(escape_html("&"), "&amp;");
    assert_eq!(escape_html("\""), "&quot;");
    assert_eq!(escape_html("'"), "&#39;");
    assert_eq!(
        escape_html("<script>&\"'</script>"),
        "&lt;script&gt;&amp;&quot;&#39;&lt;/script&gt;",
        "compound input escapes every occurrence"
    );
    assert_eq!(
        escape_html("普通中文不变"),
        "普通中文不变",
        "non-reserved untouched"
    );
    assert_eq!(escape_html(""), "", "empty input");
}

/// 转义进入属性上下文（title 带引号注入不破结构）。
#[test]
fn escaping_holds_in_attribute_context() {
    let ir: serde_json::Value = json!({
        "schema_version": "1",
        "report_id": "rep-escape-001",
        "report_kind": "src_vuln_report",
        "title": "带\"引号\"与<标签>的标题",
        "generated_at": "2026-11-23T00:00:00Z",
        "findings": []
    });
    let ir = report_engine::from_json(&ir).expect("valid IR");
    let html = render_html(&ir);
    assert!(
        html.contains("<title>带&quot;引号&quot;与&lt;标签&gt;的标题</title>"),
        "title escaped in <title> element"
    );
    assert!(
        !html.contains("<标签>"),
        "raw angle brackets never survive into output"
    );
}

/// 中文面：含中文 title/解释字段渲染 HTML 与 MD 后 UTF-8 原样。
#[test]
fn chinese_content_survives_rendering() {
    let ir: serde_json::Value = json!({
        "schema_version": "1",
        "report_id": "rep-zh-001",
        "report_kind": "management_report",
        "title": "中文验收报告：通用漏电保护器缺陷",
        "generated_at": "2026-11-23T00:00:00Z",
        "findings": [
            {
                "finding_id": "find-zh-001",
                "title": "中文 Finding 标题：验收面",
                "severity": "medium",
                "result_state": "confirmed",
                "vuln_type": "验收样例",
                "plain_explanation": "通俗解释：好比插座没接地，有触电风险。"
            }
        ]
    });
    let ir = report_engine::from_json(&ir).expect("valid IR");
    let html = render_html(&ir);
    let markdown = render_markdown(&ir);
    for text in [
        "中文验收报告：通用漏电保护器缺陷",
        "中文 Finding 标题：验收面",
        "通俗解释：好比插座没接地，有触电风险。",
    ] {
        assert!(html.contains(text), "html keeps UTF-8 literal: {text}");
        assert!(
            markdown.contains(text),
            "markdown keeps UTF-8 literal: {text}"
        );
    }
}

/// comrak 片段面：markdown 字段经 comrak 产出 `<h2>`/`<strong>` 标签（D5）。
#[test]
fn comrak_fragments_produce_heading_and_strong_tags() {
    let ir: serde_json::Value = json!({
        "schema_version": "1",
        "report_id": "rep-comrak-001",
        "report_kind": "src_vuln_report",
        "title": "comrak 片段面报告",
        "generated_at": "2026-11-23T00:00:00Z",
        "executive_summary": "## 二级标题片段\n\n这一段含 **加粗强调** 文本。",
        "findings": []
    });
    let ir = report_engine::from_json(&ir).expect("valid IR");
    let html = render_html(&ir);
    assert!(
        html.contains("<h2>二级标题片段</h2>"),
        "comrak renders <h2> heading"
    );
    assert!(
        html.contains("<strong>加粗强调</strong>"),
        "comrak renders <strong> emphasis"
    );
    // markdown 面同字段原文直出（不经 comrak——手写序列化）。
    let markdown = render_markdown(&ir);
    assert!(markdown.contains("## 二级标题片段"));
    assert!(markdown.contains("**加粗强调**"));
}

/// comrak 安全面：IR markdown 字段中的原始 HTML 标签被 comrak 转义（不是
/// 原样透传——转义面强于透传）。
#[test]
fn comrak_escapes_raw_html_in_markdown_fields() {
    let ir: serde_json::Value = json!({
        "schema_version": "1",
        "report_id": "rep-safe-001",
        "report_kind": "src_vuln_report",
        "title": "原始 HTML 安全面",
        "generated_at": "2026-11-23T00:00:00Z",
        "executive_summary": "正文含 <img src=x onerror=alert(1)> 字面。",
        "findings": []
    });
    let ir = report_engine::from_json(&ir).expect("valid IR");
    let html = render_html(&ir);
    assert!(
        !html.contains("<img src=x onerror"),
        "raw html tag never passes through comrak fragment"
    );
}
