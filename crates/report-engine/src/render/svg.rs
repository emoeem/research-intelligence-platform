//! SVG 攻击路径渲染器（PHASE-6B D5：`render_attack_path_svg`——attack_paths
//! 单类最小面：path 步骤 → 方框节点 + 箭头连线 + evidence_refs 文本标注，
//! 字符串组装零依赖，零 I/O 零 AI）。
//!
//! 裁决背景（D5）：cetz/fletcher 停批——typst 包生态非 crates.io，库模式
//! 需运行时包缓存播种 + 网络，违 --offline 零安装；且 typst 本体已停批
//! D1。SVG = 文本零依赖可判据化（well-formed + evidence 可追溯 grep）。
//!
//! 图表事实以 RTIS 攻击链为准（:531 词形）——IR attack_paths 字段位只承载
//! 引用面（6A 冻结面 §7）。PDF/DOCX 面图表以攻击路径结构化步骤列表呈现
//! （文本化——矢量图嵌入需 SVG path→PDF 转换器，本批不立）。

use crate::ir::ReportIr;

/// 节点框尺寸（px）。
const NODE_W: f32 = 170.0;
const NODE_H: f32 = 44.0;
/// 节点水平间距（px）。
const GAP_X: f32 = 56.0;
/// 节点垂直间距（px）。
const GAP_Y: f32 = 64.0;
/// 画布边距（px）。
const PADDING: f32 = 24.0;
/// 每行最大节点数（超出换行——手写布局最小面）。
const PER_ROW: usize = 3;

/// XML 转义单点（复用 escape.rs 五字符集——6A escape.rs:1 词形；XML 转义与
/// HTML 转义同五字符集 `< > & " '`，D3 词形）。
fn xml_escape(input: &str) -> String {
    crate::escape::escape_html(input)
}

/// IR → SVG 攻击路径图（确定性纯函数——同 IR 两次渲染字符串一致）。
///
/// 每条 attack_path 输出独立 `<g class="attack-path">` 组：步骤 → 方框节点
/// （path_id+步骤文本）→ 箭头连线；evidence_refs 以文本标注行落于组尾
/// （判据 10：evidence_refs 词形在 svg 字符串 grep 命中——图表关联 Evidence
/// 验收点）。IR 无 attack_paths 时输出空画布（合法空集渲染——6A 判据 9
/// 空集边界词形延续）。
pub fn render_attack_path_svg(ir: &ReportIr) -> String {
    // 画布高度：路径标题 + 节点行数 + evidence 标注行（节点数 = 步骤数
    // 断言由测试数 data-step-index 出现次数承载——判据 10）。
    let mut canvas_h: f32 = PADDING;
    for path in &ir.attack_paths {
        let rows = (path.steps.len().max(1) + PER_ROW - 1) / PER_ROW;
        canvas_h += 24.0 + rows as f32 * (NODE_H + GAP_Y) + path.evidence_refs.len() as f32 * 18.0
            + 12.0;
    }
    if ir.attack_paths.is_empty() {
        canvas_h = PADDING * 2.0 + 24.0;
    }
    let canvas_w = PADDING * 2.0 + (PER_ROW as f32) * NODE_W + ((PER_ROW - 1) as f32) * GAP_X;

    let mut out = String::with_capacity(4096);
    out.push_str(&format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}\" height=\"{}\" viewBox=\"0 0 {} {}\">",
        canvas_w as i32, canvas_h as i32, canvas_w as i32, canvas_h as i32
    ));
    out.push_str(&format!(
        "<title>{}</title>",
        xml_escape(&format!("攻击路径：{}", ir.report_id))
    ));
    // 箭头 marker（连线复用面）。
    out.push_str("<defs><marker id=\"arrow\" markerWidth=\"8\" markerHeight=\"8\" refX=\"7\" refY=\"4\" orient=\"auto\"><path d=\"M0,0 L8,4 L0,8 Z\" fill=\"#44546a\"/></marker></defs>");
    out.push_str(&format!(
        "<text class=\"report-title\" x=\"{}\" y=\"{}\" font-size=\"16\" fill=\"#1f2937\">{}</text>",
        PADDING as i32, (PADDING + 16.0) as i32, xml_escape(&ir.title)
    ));

    let mut cursor_y: f32 = PADDING + 32.0;
    for path in &ir.attack_paths {
        out.push_str(&format!(
            "<g class=\"attack-path\" data-path-id=\"{}\">",
            xml_escape(&path.path_id)
        ));
        out.push_str(&format!(
            "<text class=\"path-title\" x=\"{}\" y=\"{}\" font-size=\"13\" fill=\"#1f2937\">{}：{}</text>",
            PADDING as i32, cursor_y as i32, xml_escape(&path.path_id), xml_escape(&path.title)
        ));
        cursor_y += 12.0;
        for (idx, step) in path.steps.iter().enumerate() {
            let slot = idx % PER_ROW;
            let row = idx / PER_ROW;
            let x = PADDING + (slot as f32) * (NODE_W + GAP_X);
            let y = cursor_y + (row as f32) * (NODE_H + GAP_Y);
            out.push_str(&format!(
                "<g class=\"attack-step\" data-step-index=\"{}\"><rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" rx=\"6\" fill=\"#eef2f7\" stroke=\"#44546a\"/><text x=\"{}\" y=\"{}\" font-size=\"12\" fill=\"#1f2937\">{}</text></g>",
                idx + 1,
                x as i32, y as i32, NODE_W as i32, NODE_H as i32,
                (x + 10.0) as i32, (y + 28.0) as i32,
                xml_escape(&format!("{}. {step}", idx + 1))
            ));
            if slot > 0 || row > 0 {
                // 与前一节点连线（同行左邻 / 换行时自上节点）。
                let (px, py): (f32, f32);
                let (qx, qy): (f32, f32);
                if slot > 0 {
                    px = x - GAP_X;
                    py = y + NODE_H / 2.0;
                    qx = x;
                    qy = y + NODE_H / 2.0;
                } else {
                    px = x + NODE_W / 2.0;
                    py = y - GAP_Y;
                    qx = x + NODE_W / 2.0;
                    qy = y;
                }
                out.push_str(&format!(
                    "<line x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\" stroke=\"#44546a\" marker-end=\"url(#arrow)\"/>",
                    px as i32, py as i32, qx as i32, (qy - 1.0) as i32
                ));
            }
        }
        cursor_y += ((path.steps.len().max(1) + PER_ROW - 1) / PER_ROW) as f32 * (NODE_H + GAP_Y);
        for evidence_ref in &path.evidence_refs {
            out.push_str(&format!(
                "<text class=\"evidence-ref\" x=\"{}\" y=\"{}\" font-size=\"11\" fill=\"#6b7280\">证据引用：{}</text>",
                PADDING as i32, cursor_y as i32, xml_escape(evidence_ref)
            ));
            cursor_y += 18.0;
        }
        cursor_y += 12.0;
        out.push_str("</g>");
    }
    out.push_str("</svg>");
    out
}
