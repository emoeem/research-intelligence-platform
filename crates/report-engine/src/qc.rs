//! 导出质检三面（PHASE-6B D6——:545 十项原文分组：结构完整性/资产正确性/
//! 事实可追溯；QcReport 纯函数零 I/O）。
//!
//! 十项原文（:545 A2 词形）：「每次正式导出自动验证：章节完整、图片存在
//! 且未损坏、目录正确、页码正确、中文字体正确、代码块完整、表格不溢出、
//! 引用可追溯、Finding 与 Evidence 对应、图表与内容一致。导出失败保留
//! 错误信息并允许重新导出。」
//!
//! 三面分组与检查名映射表落批次报告 §质检映射（D6① 词形）。承载口径：
//! - 「导出失败保留错误信息并允许重新导出」= QcCheck.message 结构化承载
//!   （fail 不 panic 不 abort——红线 1 + :545 词形）；
//! - 6b 产物零图片位（IR 无图片字段）→ 资产面图片项对 6b 产物恒 pass
//!   空集（登记口径——D6①）；
//! - qc_pdf 探测 IR 中文内容（U+4E00-9FFF）→ 产物须有 /FontFile2（D2④
//!   「中文字体正确」fail 不 abort 承载——渲染降级 .notdef 态可被质检
//!   捕捉）；
//! - qc_sarif 经既有 tool_registry::jsonschema_lite::validate_instance 校验
//!   render_sarif 输出（D6③——零新校验依赖；官方 schema 资产
//!   schemas/sarif/sarif-schema-2.1.0.json 编译期内嵌）。

use crate::ir::ReportIr;
use serde::Serialize;
use serde_json::Value;
use tool_registry::jsonschema_lite;

/// SARIF 2.1.0 官方 schema 文本（编译期内嵌；资产单点
/// schemas/sarif/sarif-schema-2.1.0.json——oasis-tcs/sarif-spec main 分支
/// 112768 字节 draft-04；sha256 落批次报告）。
pub const SARIF_SCHEMA: &str =
    include_str!("../../../schemas/sarif/sarif-schema-2.1.0.json");

/// 单项检查（D6②：name/passed/message——「保留错误信息」原文词形）。
#[derive(Debug, Serialize)]
pub struct QcCheck {
    /// 检查名（三面映射表键——批次报告表格对账位）。
    pub name: &'static str,
    pub passed: bool,
    /// fail 时非空说明（pass 时空串——最小字节面）。
    pub message: String,
}

/// 质检报告（format + checks 列表；serde Serialize）。
#[derive(Debug, Serialize)]
pub struct QcReport {
    pub format: &'static str,
    pub checks: Vec<QcCheck>,
}

impl QcReport {
    /// 全过判定（:545「导出失败」= 含 fail 项即失败——fail 不 abort）。
    pub fn all_passed(&self) -> bool {
        self.checks.iter().all(|c| c.passed)
    }

    /// fail 项名列表（诊断面）。
    pub fn failed_checks(&self) -> Vec<&'static str> {
        self.checks.iter().filter(|c| !c.passed).map(|c| c.name).collect()
    }
}

/// 构造 pass 项。
fn ok(name: &'static str) -> QcCheck {
    QcCheck { name, passed: true, message: String::new() }
}

/// 构造 fail 项（message 非空——:545 保留错误信息词形）。
fn fail(name: &'static str, message: impl Into<String>) -> QcCheck {
    QcCheck { name, passed: false, message: message.into() }
}

/// IR 是否携带中文内容（U+4E00-9FFF 锚点——qc_pdf CJK 需求面探测；纯函数
/// 字节面判断：title/finding title/executive_summary 扫描）。
fn ir_needs_cjk(ir: &ReportIr) -> bool {
    let mut probe = String::new();
    probe.push_str(&ir.title);
    for finding in &ir.findings {
        probe.push_str(&finding.title);
    }
    if let Some(summary) = &ir.executive_summary {
        probe.push_str(summary);
    }
    probe.chars().any(|c| ('\u{4E00}'..='\u{9FFF}').contains(&c))
}

/// 字节面子串存在。
fn contains(bytes: &[u8], needle: &[u8]) -> bool {
    bytes.windows(needle.len()).any(|w| w == needle)
}

/// 中文 char → UTF-16BE hex 词形（ToUnicode CMap 字节面——大写 hex，与
/// pdf-writer push_hex_u16 输出同形）。
fn char_hex(c: char) -> String {
    c.encode_utf16(&mut [0u16; 2])
        .iter()
        .map(|part| format!("{part:04X}"))
        .collect()
}

/// PDF 质检（D6① 三面——:545 十项映射批次报告；bytes = render_pdf 产物）。
pub fn qc_pdf(ir: &ReportIr, bytes: &[u8]) -> QcReport {
    let mut checks = Vec::new();

    // 结构完整性面。
    checks.push(if bytes.starts_with(b"%PDF-") {
        ok("pdf_header")
    } else {
        fail("pdf_header", "missing %PDF- magic header")
    });
    // 章节完整：五段标题（执行摘要/Finding 清单/Finding 详情/攻击路径/附录）
    // 每个非 ASCII 字符的 ToUnicode UTF-16BE hex 词形在字节面（render_pdf
    // 恒渲染五段标题——缺段即缺映射）。
    let section_titles = ["执行摘要", "Finding 清单", "Finding 详情", "攻击路径", "附录"];
    let missing_sections: Vec<&str> = section_titles
        .iter()
        .filter(|title| {
            title
                .chars()
                .filter(|c| (*c as u32) > 0x7F)
                .any(|c| !contains(bytes, char_hex(c).as_bytes()))
        })
        .copied()
        .collect();
    checks.push(if missing_sections.is_empty() {
        ok("pdf_sections_present")
    } else {
        fail(
            "pdf_sections_present",
            format!("sections missing from ToUnicode face: {missing_sections:?}"),
        )
    });
    // 页码正确：/Type /Page 计数与 Pages /Count 一致（/Type /Pages 页树
    // 对象含同前缀——计数时排除 's' 后缀命中）。
    let page_objects = count_occurrences_exact(bytes, b"/Type /Page");
    let declared_count = parse_pdf_count(bytes);
    checks.push(match declared_count {
        Some(count) if count == page_objects && page_objects >= 1 => ok("pdf_page_count"),
        Some(count) => fail(
            "pdf_page_count",
            format!("/Count {count} != /Type /Page objects {page_objects}"),
        ),
        None => fail("pdf_page_count", "Pages /Count not found in bytes"),
    });

    // 资产正确性面。
    // 中文字体正确：IR 含中文内容 → /FontFile2 必须在位（缺 CJK 字体面渲染
    // 降级 .notdef 可被捕捉——D2④）；IR 零中文 → 空集 pass（无需嵌入）。
    if ir_needs_cjk(ir) {
        checks.push(if contains(bytes, b"/FontFile2") {
            ok("pdf_cjk_font_embedded")
        } else {
            fail(
                "pdf_cjk_font_embedded",
                "IR carries CJK text but /FontFile2 absent (renderer degraded to .notdef)",
            )
        });
    } else {
        checks.push(ok("pdf_cjk_font_embedded"));
    }
    // 代码块完整：location.snippet 每个非 ASCII 字符映射在位 + 换行经分段
    // 行渲染（render_pdf snippet 逐行投影——缺行即缺映射）。
    let missing_snippet_chars: Vec<char> = ir
        .findings
        .iter()
        .filter_map(|f| f.location.as_ref())
        .filter_map(|l| l.snippet.as_ref())
        .flat_map(|snippet| snippet.chars())
        .filter(|c| (*c as u32) > 0x7F)
        .filter(|c| !contains(bytes, char_hex(*c).as_bytes()))
        .collect();
    checks.push(if missing_snippet_chars.is_empty() {
        ok("pdf_code_blocks")
    } else {
        fail(
            "pdf_code_blocks",
            format!("snippet chars missing from ToUnicode face: {missing_snippet_chars:?}"),
        )
    });
    // 图片存在且未损坏：6b 产物零图片位（IR 无图片字段——D6① 登记口径，
    // 恒 pass 空集）。
    checks.push(ok("pdf_images"));
    // 尾完整性（资产面补充）：%%EOF 终结。
    checks.push(if bytes.ends_with(b"%%EOF") {
        ok("pdf_eof")
    } else {
        fail("pdf_eof", "PDF trailer does not end with %%EOF")
    });

    // 事实可追溯面。
    let missing_ids: Vec<&str> = ir
        .findings
        .iter()
        .flat_map(|f| {
            let mut needles = vec![f.finding_id.as_str()];
            if let Some(rtis_ref) = &f.rtis_ref {
                needles.push(rtis_ref.as_str());
            }
            needles
        })
        .filter(|needle| !contains(bytes, needle.as_bytes()))
        .collect();
    checks.push(if missing_ids.is_empty() {
        ok("pdf_finding_refs")
    } else {
        fail(
            "pdf_finding_refs",
            format!("finding_id/rtis_ref missing from bytes: {missing_ids:?}"),
        )
    });
    // Finding 与 Evidence 对应：evidence_id（re- 前缀 ASCII）经 CID 流不可
    // 直 grep——承载在 ToUnicode 面（每字符 hex 词形）+ 六 ID 集合计数 =
    // IR 对账。
    let missing_evidence: Vec<&str> = ir
        .findings
        .iter()
        .flat_map(|f| f.evidence.iter())
        .map(|e| e.evidence_id.as_str())
        .filter(|id| !evidence_id_traced(bytes, id))
        .collect();
    checks.push(if missing_evidence.is_empty() {
        ok("pdf_evidence_refs")
    } else {
        fail(
            "pdf_evidence_refs",
            format!("evidence_id chars missing from ToUnicode face: {missing_evidence:?}"),
        )
    });
    // 引用可追溯：ToUnicode CMap 对象在位。
    checks.push(if contains(bytes, b"/ToUnicode") {
        ok("pdf_to_unicode")
    } else {
        fail("pdf_to_unicode", "ToUnicode CMap object missing")
    });
    // 图表与内容一致：attack_paths 步骤数 = 产物文本面投影（PDF 面以攻击
    // 路径结构化步骤列表呈现——D5；每步骤 char 映射在位承载）。
    let missing_steps: Vec<String> = ir
        .attack_paths
        .iter()
        .flat_map(|p| p.steps.iter())
        .filter(|step| {
            step.chars()
                .filter(|c| (*c as u32) > 0x7F)
                .any(|c| !contains(bytes, char_hex(c).as_bytes()))
        })
        .cloned()
        .collect();
    checks.push(if missing_steps.is_empty() {
        ok("pdf_attack_paths")
    } else {
        fail(
            "pdf_attack_paths",
            format!("attack path step chars missing: {missing_steps:?}"),
        )
    });

    QcReport { format: "pdf", checks }
}

/// DOCX 质检（bytes = render_docx 产物）。
pub fn qc_docx(ir: &ReportIr, bytes: &[u8]) -> QcReport {
    let mut checks = Vec::new();

    // 结构完整性面。
    checks.push(if bytes.starts_with(b"PK\x03\x04") {
        ok("docx_header")
    } else {
        fail("docx_header", "missing PK local header magic")
    });
    // 容器逐条目对账（central directory + CRC32 全对——D6②；结构损坏 /
    // CRC 篡改在此 fail）。
    let entries = crate::ooxml::qc_zip_entries(bytes);
    checks.push(match &entries {
        Ok(_) => ok("docx_container_crc"),
        Err(e) => fail("docx_container_crc", format!("zip reconciliation: {e}")),
    });
    // 零件在位（[Content_Types].xml / _rels/.rels / word/document.xml）。
    let part_names = entries.as_ref().map(|e| e.iter().map(|(n, _, _)| n.clone()).collect::<Vec<_>>()).unwrap_or_default();
    let required_parts = ["[Content_Types].xml", "_rels/.rels", "word/document.xml"];
    let missing_parts: Vec<&str> = required_parts
        .iter()
        .filter(|p| !part_names.iter().any(|n| n == *p))
        .copied()
        .collect();
    checks.push(if missing_parts.is_empty() {
        ok("docx_parts")
    } else {
        fail("docx_parts", format!("missing parts: {missing_parts:?}"))
    });
    // 章节完整 + 目录正确：document.xml 五段标题词形（XML UTF-8 直 grep）。
    let doc = entries
        .as_ref()
        .ok()
        .and_then(|_| extract_part(bytes, "word/document.xml"))
        .unwrap_or_default();
    let section_titles = ["执行摘要", "Finding 清单", "Finding 详情", "攻击路径", "附录"];
    let missing_sections: Vec<&str> = section_titles
        .iter()
        .filter(|t| !doc.windows(t.len()).any(|w| w == t.as_bytes()))
        .copied()
        .collect();
    checks.push(if missing_sections.is_empty() {
        ok("docx_sections_present")
    } else {
        fail("docx_sections_present", format!("sections missing: {missing_sections:?}"))
    });
    // 目录正确（Heading 层级结构）：pStyle Heading 出现次数 ≥ 段标题总数
    // （五段 H2 + 每 finding H3——计数对账）。
    let heading_count = count_occurrences(bytes, b"w:val=\"Heading");
    let expected_headings = 5 + ir.findings.len();
    checks.push(if heading_count >= expected_headings {
        ok("docx_heading_structure")
    } else {
        fail(
            "docx_heading_structure",
            format!("heading styles {heading_count} < expected {expected_headings}"),
        )
    });

    // 资产正确性面。
    // 图片存在且未损坏：6b 产物零图片位（D6① 登记口径——恒 pass 空集）。
    checks.push(ok("docx_images"));
    // 表格不溢出：findings 非空时 <w:tbl> 与 </w:tbl> 恰好一对（auto 宽度
    // tcW w:w=\"0\" 不产生固定溢出宽度）。
    let open_tables = count_occurrences(bytes, b"<w:tbl>");
    let close_tables = count_occurrences(bytes, b"</w:tbl>");
    let expected_tables = if ir.findings.is_empty() { 0 } else { 1 };
    checks.push(if open_tables == expected_tables && close_tables == expected_tables {
        ok("docx_table_closed")
    } else {
        fail(
            "docx_table_closed",
            format!("<w:tbl> {open_tables} / </w:tbl> {close_tables} != expected {expected_tables}"),
        )
    });

    // 事实可追溯面。
    let missing_words: Vec<String> = ir
        .findings
        .iter()
        .flat_map(|f| {
            let mut words = vec![f.finding_id.clone(), f.title.clone()];
            if let Some(rtis_ref) = &f.rtis_ref {
                words.push(rtis_ref.clone());
            }
            words.extend(f.evidence.iter().map(|e| e.evidence_id.clone()));
            words
        })
        .filter(|w| !doc.windows(w.len()).any(|win| win == w.as_bytes()))
        .collect();
    checks.push(if missing_words.is_empty() {
        ok("docx_fact_traceability")
    } else {
        fail(
            "docx_fact_traceability",
            format!("finding_id/title/rtis_ref/evidence_id missing from document.xml: {missing_words:?}"),
        )
    });

    QcReport { format: "docx", checks }
}

/// SARIF 质检（sarif = render_sarif 产物；D6③ 官方 schema 校验收口）。
pub fn qc_sarif(ir: &ReportIr, sarif: &Value) -> QcReport {
    let mut checks = Vec::new();

    // 结构完整性面。
    checks.push(if sarif.get("version").and_then(Value::as_str) == Some("2.1.0") {
        ok("sarif_version")
    } else {
        fail(
            "sarif_version",
            format!("version = {:?} != \"2.1.0\"", sarif.get("version")),
        )
    });
    let runs = sarif.get("runs").and_then(Value::as_array).cloned().unwrap_or_default();
    checks.push(if runs.len() == 1 {
        ok("sarif_runs")
    } else {
        fail("sarif_runs", format!("runs length = {} != 1", runs.len()))
    });
    let driver_name = runs
        .first()
        .and_then(|r| r.get("tool"))
        .and_then(|t| t.get("driver"))
        .and_then(|d| d.get("name"))
        .and_then(Value::as_str)
        .unwrap_or("");
    checks.push(if driver_name == "research-platform-report-engine" {
        ok("sarif_tool_driver")
    } else {
        fail("sarif_tool_driver", format!("tool.driver.name = {driver_name:?}"))
    });

    // 事实可追溯面（Finding 与 Evidence 对应——六 ID/rtis_ref/level/kind 逐
    // Finding 对账）。
    let results = runs
        .first()
        .and_then(|r| r.get("results"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    checks.push(if results.len() == ir.findings.len() {
        ok("sarif_results_count")
    } else {
        fail(
            "sarif_results_count",
            format!("results {} != findings {}", results.len(), ir.findings.len()),
        )
    });
    // level/kind 六 ID/rtis_ref 逐 result 对账（ruleId = finding_id 键）。
    let mut mapping_failures: Vec<String> = Vec::new();
    for finding in &ir.findings {
        let result = results
            .iter()
            .find(|r| r.get("ruleId").and_then(Value::as_str) == Some(&finding.finding_id));
        let Some(result) = result else {
            mapping_failures.push(format!("result for {} missing", finding.finding_id));
            continue;
        };
        let expected_level = crate::render::sarif::severity_to_level(&finding.severity);
        if result.get("level").and_then(Value::as_str) != Some(expected_level) {
            mapping_failures.push(format!(
                "{} level {:?} != {expected_level:?}",
                finding.finding_id,
                result.get("level")
            ));
        }
        let expected_kind = crate::render::sarif::result_state_to_kind(&finding.result_state);
        if result.get("kind").and_then(Value::as_str) != Some(expected_kind) {
            mapping_failures.push(format!(
                "{} kind {:?} != {expected_kind:?}",
                finding.finding_id,
                result.get("kind")
            ));
        }
        // rtis_ref 词形（properties.rip.rtis_refs 同值）。
        let expected_rtis: Vec<&String> = finding.rtis_ref.iter().collect();
        let actual_rtis: Vec<String> = result
            .pointer("/properties/rip/rtis_refs")
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(Value::as_str).map(String::from).collect())
            .unwrap_or_default();
        if actual_rtis.len() != expected_rtis.len()
            || actual_rtis.iter().zip(&expected_rtis).any(|(a, b)| a != *b)
        {
            mapping_failures.push(format!(
                "{} rtis_refs {actual_rtis:?} != {expected_rtis:?}",
                finding.finding_id
            ));
        }
    }
    checks.push(if mapping_failures.is_empty() {
        ok("sarif_finding_mapping")
    } else {
        fail("sarif_finding_mapping", format!("mapping failures: {mapping_failures:?}"))
    });
    // 引用可追溯（六 ID 集合）：properties.rip.six_ids.evidence_ids 与 IR
    // evidence 集合对账。
    let mut six_id_failures: Vec<String> = Vec::new();
    for finding in &ir.findings {
        let result = results
            .iter()
            .find(|r| r.get("ruleId").and_then(Value::as_str) == Some(&finding.finding_id));
        let Some(result) = result else { continue };
        let actual: Vec<String> = result
            .pointer("/properties/rip/six_ids/evidence_ids")
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(Value::as_str).map(String::from).collect())
            .unwrap_or_default();
        let mut expected: Vec<&str> =
            finding.evidence.iter().map(|e| e.evidence_id.as_str()).collect();
        expected.sort_unstable();
        let mut actual_sorted = actual.clone();
        actual_sorted.sort_unstable();
        if expected != actual_sorted {
            six_id_failures.push(format!(
                "{} evidence_ids {actual:?} != {expected:?}",
                finding.finding_id
            ));
        }
    }
    checks.push(if six_id_failures.is_empty() {
        ok("sarif_six_ids")
    } else {
        fail("sarif_six_ids", format!("six_ids failures: {six_id_failures:?}"))
    });

    // 资产正确性面（artifacts 与 evidence uri 对账——产物内容完整性）。
    let artifacts = runs
        .first()
        .and_then(|r| r.get("artifacts"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let artifact_uris: Vec<&str> = artifacts
        .iter()
        .filter_map(|a| a.pointer("/location/uri").and_then(Value::as_str))
        .collect();
    let mut uri_failures: Vec<String> = Vec::new();
    for finding in &ir.findings {
        for evidence in &finding.evidence {
            if !artifact_uris.contains(&evidence.uri.as_str()) {
                uri_failures.push(format!("artifact uri {} missing", evidence.uri));
            }
        }
    }
    checks.push(if uri_failures.is_empty() {
        ok("sarif_artifacts")
    } else {
        fail("sarif_artifacts", format!("artifact failures: {uri_failures:?}"))
    });

    // 官方 SARIF 2.1.0 schema 校验（D6③——jsonschema_lite 零新校验依赖；
    // draft-04 兼容面失败 → 停批上报负例面预登记词形——实测在 S5 收口）。
    checks.push(match parse_schema_and_validate(sarif) {
        Ok(()) => ok("sarif_official_schema"),
        Err(e) => fail("sarif_official_schema", format!("official SARIF schema: {e}")),
    });

    QcReport { format: "sarif", checks }
}

/// 解析内嵌官方 schema 并校验实例（编译期单点 + 失败信息透传）。
fn parse_schema_and_validate(instance: &Value) -> Result<(), String> {
    let schema: Value = serde_json::from_str(SARIF_SCHEMA)
        .map_err(|e| format!("SARIF schema is not valid JSON: {e}"))?;
    jsonschema_lite::validate_instance(&schema, instance)
}

/// evidence_id 可追溯判定：id 全字符在 ToUnicode hex 词形或 ASCII 直 grep
/// （六 ID 组成 [0-9a-z-]——ASCII 字符若在 content 直排不可 grep，经 CMap
/// hex 承载；两者任一命中即可追溯）。
fn evidence_id_traced(bytes: &[u8], id: &str) -> bool {
    if contains(bytes, id.as_bytes()) {
        return true;
    }
    id.chars().all(|c| {
        (c as u32) <= 0x7F && contains(bytes, format!("{:04X}", c as u32).as_bytes())
            || (c as u32) > 0x7F && contains(bytes, char_hex(c).as_bytes())
    })
}

/// 字节面计数（needle 命中处后继字节 ≠ b's'——排除 /Type /Pages 词形）。
fn count_occurrences_exact(bytes: &[u8], needle: &[u8]) -> usize {
    if needle.is_empty() || bytes.len() < needle.len() {
        return 0;
    }
    bytes
        .windows(needle.len())
        .enumerate()
        .filter(|(i, w)| {
            *w == needle
                && bytes
                    .get(i + needle.len())
                    .map(|next| *next != b's')
                    .unwrap_or(true)
        })
        .count()
}

/// 字节面计数。
fn count_occurrences(bytes: &[u8], needle: &[u8]) -> usize {
    if needle.is_empty() || bytes.len() < needle.len() {
        return 0;
    }
    bytes.windows(needle.len()).filter(|w| *w == needle).count()
}

/// 解析 Pages /Count 值（字节面 "/Count N" 扫描——第一处命中）。
fn parse_pdf_count(bytes: &[u8]) -> Option<usize> {
    let needle = b"/Count ";
    let mut idx = 0;
    while idx + needle.len() <= bytes.len() {
        if &bytes[idx..idx + needle.len()] == needle {
            let start = idx + needle.len();
            let mut end = start;
            while end < bytes.len() && bytes[end].is_ascii_digit() {
                end += 1;
            }
            if end > start {
                return std::str::from_utf8(&bytes[start..end])
                    .ok()
                    .and_then(|s| s.parse().ok());
            }
        }
        idx += 1;
    }
    None
}

/// 从容器字节提取指定零件数据段（qc_docx 内部自解析——qc_zip_entries 已验
/// 结构面，此处只按名取数据）。
fn extract_part(bytes: &[u8], part_name: &str) -> Option<Vec<u8>> {
    let eocd_sig = b"PK\x05\x06";
    let eocd_pos = bytes.windows(4).rposition(|w| w == eocd_sig)?;
    let cd_offset = u32::from_le_bytes([
        bytes.get(eocd_pos + 16).copied()?,
        bytes.get(eocd_pos + 17).copied()?,
        bytes.get(eocd_pos + 18).copied()?,
        bytes.get(eocd_pos + 19).copied()?,
    ]) as usize;
    let mut pos = cd_offset;
    while pos + 46 <= bytes.len() && bytes[pos..].starts_with(b"PK\x01\x02") {
        let name_len = u16::from_le_bytes([bytes[pos + 28], bytes[pos + 29]]) as usize;
        let extra_len = u16::from_le_bytes([bytes[pos + 30], bytes[pos + 31]]) as usize;
        let comment_len = u16::from_le_bytes([bytes[pos + 32], bytes[pos + 33]]) as usize;
        let uncompressed = u32::from_le_bytes([
            bytes[pos + 24],
            bytes[pos + 25],
            bytes[pos + 26],
            bytes[pos + 27],
        ]) as usize;
        let local_offset = u32::from_le_bytes([
            bytes[pos + 42],
            bytes[pos + 43],
            bytes[pos + 44],
            bytes[pos + 45],
        ]) as usize;
        if pos + 46 + name_len > bytes.len() {
            return None;
        }
        let name = std::str::from_utf8(&bytes[pos + 46..pos + 46 + name_len]).ok()?;
        if name == part_name {
            let local_name_len = u16::from_le_bytes([
                bytes.get(local_offset + 26).copied()?,
                bytes.get(local_offset + 27).copied()?,
            ]) as usize;
            let local_extra_len = u16::from_le_bytes([
                bytes.get(local_offset + 28).copied()?,
                bytes.get(local_offset + 29).copied()?,
            ]) as usize;
            let data_start = local_offset + 30 + local_name_len + local_extra_len;
            return bytes.get(data_start..data_start + uncompressed).map(Vec::from);
        }
        pos += 46 + name_len + extra_len + comment_len;
    }
    None
}
