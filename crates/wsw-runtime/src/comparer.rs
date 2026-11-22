//! `wsw_runtime::comparer`：Comparer 消费层（WSW-6；PLAN §4.7 :296-303 语义面）。
//!
//! - diff 原语 = [`wsw_message::diff`] **零 diff**（WSW-4 E.7 承接义务原文
//!   「diff 原语保持零依赖红线」——wsw-message 零 regex 结构性成立；本模块
//!   是消费层非第二 diff 实现，D6）；
//! - regex 忽略维 = 本模块预处理（skip_line_regexes 行级剔除 +
//!   normalize_key_regexes 键值掩码——regex=1.13.1 已在 wsw-runtime Cargo.toml
//!   转正声明，WSW-4 D4/D9 先例，零 Cargo.toml 改动），预处理后进
//!   [`wsw_message::diff::diff_normalized`]；
//! - 四 Diff 面（PLAN :298）：文本（diff_lines/DiffOp 区间投影）/ JSON 结构
//!   （serde_json 层级递归，键序不敏感）/ Header（头投影逐键对照）/
//!   二进制 Hex（字节级 + 首差异偏移归因）；
//! - 相同判定键 = sha256 **非长度**（PLAN :303——长度相同 ≠ 内容相同，
//!   [`wsw_message::cas::sha256_hex`] 唯一挂点）；
//! - 结果保存 = [`persist_report_as_blob`]（wsw_message cas/store 既有面：
//!   CAS 正文 + wsw_body_blob 行，artifact.rs Registrar 接线留平台面）。

use regex::Regex;
use serde::Serialize;
use wsw_message::diff::{diff_normalized, DiffHunk, DiffOp, NormalizeRules};

/// 忽略规则输入（三维度：子串/键值直通 diff 原语 + 正则两维本模块预处理；
/// PLAN :301「忽略动态字段、时间戳、随机 Token 和指定 Header」对齐）。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct IgnoreRules {
    /// 行级剔除正则（行内命中即从两侧比较中剔除）。
    pub skip_line_regexes: Vec<String>,
    /// 键值掩码正则（命中键名的值替换 `«key»` 占位）。
    pub normalize_key_regexes: Vec<String>,
    /// diff 原语两维直通（skip_line_substrings + normalize_keys）。
    pub base: NormalizeRules,
}

/// 编译面（正则编译失败 = 结构化拒绝——ComparerError::RegexInvalid）。
#[derive(Debug, Clone)]
pub struct CompiledIgnore {
    skip_res: Vec<Regex>,
    key_res: Vec<Regex>,
    base: NormalizeRules,
}

/// Comparer 错误面。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ComparerError {
    /// 正则编译失败（pattern + cause）。
    RegexInvalid(String, String),
}

impl std::fmt::Display for ComparerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ComparerError::RegexInvalid(p, c) => write!(f, "invalid regex {p:?}: {c}"),
        }
    }
}

impl std::error::Error for ComparerError {}

impl IgnoreRules {
    /// 编译（CLI/API 面；非法正则 fail-closed）。
    pub fn compile(&self) -> Result<CompiledIgnore, ComparerError> {
        let mut skip_res = Vec::new();
        for p in &self.skip_line_regexes {
            skip_res.push(
                Regex::new(p).map_err(|e| ComparerError::RegexInvalid(p.clone(), e.to_string()))?,
            );
        }
        let mut key_res = Vec::new();
        for p in &self.normalize_key_regexes {
            key_res.push(
                Regex::new(p).map_err(|e| ComparerError::RegexInvalid(p.clone(), e.to_string()))?,
            );
        }
        Ok(CompiledIgnore {
            skip_res,
            key_res,
            base: self.base.clone(),
        })
    }
}

/// 行切分（\n；尾空行不入行集——diff.rs split_lines 同语义，保持投影一致）。
fn split_lines_local(text: &str) -> Vec<&str> {
    if text.is_empty() {
        return Vec::new();
    }
    let mut lines: Vec<&str> = text.split('\n').collect();
    if lines.len() > 1 && lines.last().is_some_and(|l| l.is_empty()) {
        lines.pop();
    }
    lines
}

/// 单行键值掩码（regex 维；形态与 diff.rs mask_key 同型：JSON `"key": v` →
/// 表单 `key=v`（至 `&`/空白止）→ 日志 `key: v`（至行尾）——首形命中即整行
/// 按该形掩码；同形多键全部掩码（span 收集后逆序应用，键命中任一 key_res）。
fn mask_line_regex(line: &str, key_res: &[Regex]) -> String {
    if key_res.is_empty() {
        return line.to_string();
    }
    let key_hit = |key: &str| key_res.iter().any(|r| r.is_match(key));
    // 带引号值闭合位（非转义引号；\" 跳过——粗扫面，掩码投影专用）。
    let quoted_end = |value: &str| -> usize {
        let vb = value.as_bytes();
        let mut i = 1usize;
        while i < vb.len() {
            if vb[i] == b'\\' {
                i += 2;
                continue;
            }
            if vb[i] == b'"' {
                return i + 1;
            }
            i += 1;
        }
        vb.len()
    };
    // 形一：JSON `"key":`（行内任意位；全部命中键掩码）。
    let json_re = Regex::new(r#""([^"]+)"\s*:"#).expect("static json shape regex");
    let mut edits: Vec<(usize, usize, String)> = Vec::new();
    for c in json_re.captures_iter(line) {
        let key = c.get(1).unwrap().as_str();
        if !key_hit(key) {
            continue;
        }
        let colon_end = c.get(0).unwrap().end();
        let value = &line[colon_end..];
        let value_end = if value.starts_with('"') {
            colon_end + quoted_end(value)
        } else {
            colon_end + value.find([',', '}', ']']).unwrap_or(value.len())
        };
        edits.push((colon_end, value_end, format!("«{key}»")));
    }
    if !edits.is_empty() {
        let mut out = line.to_string();
        for (s, e, rep) in edits.iter().rev() {
            out.replace_range(s..e, rep);
        }
        return out;
    }
    // 形二：表单 key=value（至 & / 空白止；全部命中键掩码）。
    let form_re = Regex::new(r"([A-Za-z0-9_.\-\[\]]+)=([^&\s]*)").expect("static form shape regex");
    let mut edits = Vec::new();
    for c in form_re.captures_iter(line) {
        let key = c.get(1).unwrap().as_str();
        if !key_hit(key) {
            continue;
        }
        let vs = c.get(2).unwrap().start();
        let ve = c.get(2).unwrap().end();
        edits.push((vs, ve, format!("«{key}»")));
    }
    if !edits.is_empty() {
        let mut out = line.to_string();
        for (s, e, rep) in edits.iter().rev() {
            out.replace_range(s..e, rep);
        }
        return out;
    }
    // 形三：日志 key: value（至行尾；首个命中键）。
    let log_re = Regex::new(r"([A-Za-z0-9_.\-\[\]]+):\s(.*)$").expect("static log shape regex");
    if let Some(c) = log_re.captures(line) {
        let key = c.get(1).unwrap().as_str();
        if key_hit(key) {
            let vs = c.get(2).unwrap().start();
            let mut out = line.to_string();
            out.replace_range(vs.., &format!("«{key}»"));
            return out;
        }
    }
    line.to_string()
}

/// regex 维预处理（先键值掩码后行级剔除——掩码命中后行仍有其它内容则保留，
/// diff.rs normalize_text 同序）。
fn preprocess_regex(text: &str, ignore: &CompiledIgnore) -> String {
    let mut out: Vec<String> = Vec::new();
    for line in split_lines_local(text) {
        let mut owned = line.to_string();
        if !ignore.key_res.is_empty() {
            owned = mask_line_regex(&owned, &ignore.key_res);
        }
        if ignore.skip_res.iter().any(|r| r.is_match(&owned)) {
            continue;
        }
        out.push(owned);
    }
    out.join("\n")
}

/// 区间投影（serde 序列化面；DiffOp 词形 = equal/insert/delete/replace）。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct HunkProjection {
    pub op: &'static str,
    pub left_start: usize,
    pub left_len: usize,
    pub right_start: usize,
    pub right_len: usize,
}

fn op_word(op: DiffOp) -> &'static str {
    match op {
        DiffOp::Equal => "equal",
        DiffOp::Insert => "insert",
        DiffOp::Delete => "delete",
        DiffOp::Replace => "replace",
    }
}

fn project_hunks(hunks: &[DiffHunk]) -> Vec<HunkProjection> {
    hunks
        .iter()
        .map(|h| HunkProjection {
            op: op_word(h.op),
            left_start: h.left_start,
            left_len: h.left_len,
            right_start: h.right_start,
            right_len: h.right_len,
        })
        .collect()
}

/// JSON 结构差异条目（层级路径词形 `$.a.b[0]`）。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct JsonDiffEntry {
    pub path: String,
    pub kind: &'static str, // added/removed/changed/type_changed
    pub left_preview: Option<String>,
    pub right_preview: Option<String>,
}

/// Header 差异条目。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct HeaderDiffEntry {
    pub name: String,
    pub kind: &'static str, // added/removed/changed
    pub left_values: Option<Vec<String>>,
    pub right_values: Option<Vec<String>>,
}

/// Hex 差异报告（字节级；首差异偏移归因 = OFFSET 面）。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct HexDiffReport {
    pub first_diff_offset: Option<usize>,
    pub differing_bytes: usize,
    pub left_len: usize,
    pub right_len: usize,
    pub left_preview_hex: Option<String>,
    pub right_preview_hex: Option<String>,
}

/// 比较报告（相同判定键 = sha256；PLAN :303 长度相同 ≠ 内容相同）。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CompareReport {
    pub mode: &'static str, // text/json/header/hex
    pub identical: bool,
    pub left_sha256: String,
    pub right_sha256: String,
    pub left_len: usize,
    pub right_len: usize,
    pub similarity: Option<f64>,
    pub hunks: Option<Vec<HunkProjection>>,
    pub json_diff: Option<Vec<JsonDiffEntry>>,
    pub header_diff: Option<Vec<HeaderDiffEntry>>,
    pub hex_diff: Option<HexDiffReport>,
}

fn shas(left: &str, right: &str) -> (String, String, bool) {
    let l = wsw_message::cas::sha256_hex(left.as_bytes());
    let r = wsw_message::cas::sha256_hex(right.as_bytes());
    let identical = l == r;
    (l, r, identical)
}

/// 文本 diff（regex 维预处理 + diff 原语 normalize/diff/similarity）。相同
/// 判定键 = 归一化后文本的 sha256（regex 维 + base 两维全部生效后——比较
/// 语义 = 归一化面；原始输入 sha 只作 left/right 原始事实投影）。
pub fn compare_text(left: &str, right: &str, ignore: &CompiledIgnore) -> CompareReport {
    let l = preprocess_regex(left, ignore);
    let r = preprocess_regex(right, ignore);
    let (hunks, sim) = diff_normalized(&l, &r, &ignore.base);
    let ln = wsw_message::diff::normalize_text(&l, &ignore.base);
    let rn = wsw_message::diff::normalize_text(&r, &ignore.base);
    let (ls_raw, rs_raw, _) = shas(&l, &r);
    let (ls, rs, identical) = shas(&ln, &rn);
    let _ = (ls_raw, rs_raw);
    CompareReport {
        mode: "text",
        identical,
        left_sha256: ls,
        right_sha256: rs,
        left_len: l.len(),
        right_len: r.len(),
        similarity: Some(sim),
        hunks: Some(project_hunks(&hunks)),
        json_diff: None,
        header_diff: None,
        hex_diff: None,
    }
}

/// JSON 结构 diff（serde_json 层级递归，键序不敏感——Value::Object 键序
/// 无关性由解析归一承担）。
pub fn compare_json(left: &str, right: &str) -> Result<CompareReport, ComparerError> {
    let lv: serde_json::Value = serde_json::from_str(left)
        .map_err(|e| ComparerError::RegexInvalid("left-json".into(), e.to_string()))?;
    let rv: serde_json::Value = serde_json::from_str(right)
        .map_err(|e| ComparerError::RegexInvalid("right-json".into(), e.to_string()))?;
    let mut entries = Vec::new();
    walk_json(&lv, &rv, "$", &mut entries);
    // 相同判定键 = sha256 非长度（PLAN :303）；JSON 面的判定基准 = 规范化
    // 序列化（键序归一）——原始输入串 sha 只作 left/right 原始事实投影。
    let lcanon = serde_json::to_string(&lv).unwrap_or_default();
    let rcanon = serde_json::to_string(&rv).unwrap_or_default();
    let (ls, rs, identical) = shas(&lcanon, &rcanon);
    Ok(CompareReport {
        mode: "json",
        identical,
        left_sha256: ls,
        right_sha256: rs,
        left_len: left.len(),
        right_len: right.len(),
        similarity: None,
        hunks: None,
        json_diff: Some(entries),
        header_diff: None,
        hex_diff: None,
    })
}

fn preview(v: &serde_json::Value) -> String {
    let s = serde_json::to_string(v).unwrap_or_default();
    s.chars().take(120).collect()
}

fn walk_json(
    l: &serde_json::Value,
    r: &serde_json::Value,
    path: &str,
    out: &mut Vec<JsonDiffEntry>,
) {
    if l == r {
        return;
    }
    match (l, r) {
        (serde_json::Value::Object(lo), serde_json::Value::Object(ro)) => {
            for (k, lv) in lo {
                match ro.get(k) {
                    None => out.push(JsonDiffEntry {
                        path: format!("{path}.{k}"),
                        kind: "removed",
                        left_preview: Some(preview(lv)),
                        right_preview: None,
                    }),
                    Some(rv) => walk_json(lv, rv, &format!("{path}.{k}"), out),
                }
            }
            for (k, rv) in ro {
                if !lo.contains_key(k) {
                    out.push(JsonDiffEntry {
                        path: format!("{path}.{k}"),
                        kind: "added",
                        left_preview: None,
                        right_preview: Some(preview(rv)),
                    });
                }
            }
        }
        (serde_json::Value::Array(la), serde_json::Value::Array(ra)) => {
            let n = la.len().max(ra.len());
            for i in 0..n {
                match (la.get(i), ra.get(i)) {
                    (Some(lv), Some(rv)) => walk_json(lv, rv, &format!("{path}[{i}]"), out),
                    (Some(lv), None) => out.push(JsonDiffEntry {
                        path: format!("{path}[{i}]"),
                        kind: "removed",
                        left_preview: Some(preview(lv)),
                        right_preview: None,
                    }),
                    (None, Some(rv)) => out.push(JsonDiffEntry {
                        path: format!("{path}[{i}]"),
                        kind: "added",
                        left_preview: None,
                        right_preview: Some(preview(rv)),
                    }),
                    (None, None) => unreachable!(),
                }
            }
        }
        _ => {
            let kind = if std::mem::discriminant(l) != std::mem::discriminant(r) {
                "type_changed"
            } else {
                "changed"
            };
            out.push(JsonDiffEntry {
                path: path.to_string(),
                kind,
                left_preview: Some(preview(l)),
                right_preview: Some(preview(r)),
            });
        }
    }
}

/// Header 逐键对照（`[{"name","value"}]` JSON 数组投影；键 = name 小写
/// 归一，多值保留）。
pub fn compare_headers(left_json: &str, right_json: &str) -> Result<CompareReport, ComparerError> {
    let parse = |text: &str| -> Result<Vec<(String, String)>, ComparerError> {
        #[derive(serde::Deserialize)]
        struct H {
            name: String,
            value: String,
        }
        let hs: Vec<H> = serde_json::from_str(text)
            .map_err(|e| ComparerError::RegexInvalid("headers-json".into(), e.to_string()))?;
        Ok(hs.into_iter().map(|h| (h.name, h.value)).collect())
    };
    let l = parse(left_json)?;
    let r = parse(right_json)?;
    let mut map = std::collections::BTreeMap::new();
    for (n, v) in &l {
        map.entry(n.to_ascii_lowercase())
            .or_insert_with(Vec::new)
            .push(v.clone());
    }
    let mut rmap = std::collections::BTreeMap::new();
    for (n, v) in &r {
        rmap.entry(n.to_ascii_lowercase())
            .or_insert_with(Vec::new)
            .push(v.clone());
    }
    let mut entries = Vec::new();
    for (k, lv) in &map {
        match rmap.get(k) {
            None => entries.push(HeaderDiffEntry {
                name: k.clone(),
                kind: "removed",
                left_values: Some(lv.clone()),
                right_values: None,
            }),
            Some(rv) if rv != lv => entries.push(HeaderDiffEntry {
                name: k.clone(),
                kind: "changed",
                left_values: Some(lv.clone()),
                right_values: Some(rv.clone()),
            }),
            _ => {}
        }
    }
    for (k, rv) in &rmap {
        if !map.contains_key(k) {
            entries.push(HeaderDiffEntry {
                name: k.clone(),
                kind: "added",
                left_values: None,
                right_values: Some(rv.clone()),
            });
        }
    }
    // 相同判定键 = 键序归一投影（BTreeMap name→values）的 sha256——
    // 数组序不敏感（逐键对照语义）。
    let canon = |m: &std::collections::BTreeMap<String, Vec<String>>| {
        serde_json::to_string(m).unwrap_or_default()
    };
    let (ls, rs, identical) = shas(&canon(&map), &canon(&rmap));
    Ok(CompareReport {
        mode: "header",
        identical,
        left_sha256: ls,
        right_sha256: rs,
        left_len: left_json.len(),
        right_len: right_json.len(),
        similarity: None,
        hunks: None,
        json_diff: None,
        header_diff: Some(entries),
        hex_diff: None,
    })
}

/// Hex 字节 diff（首差异偏移归因 + 差异字节数 + 差异窗预览 32B hex）。
pub fn compare_bytes(left: &[u8], right: &[u8]) -> CompareReport {
    let first = (0..left.len().min(right.len())).find(|&i| left[i] != right[i]);
    let first = first.or(if left.len() != right.len() {
        Some(left.len().min(right.len()))
    } else {
        None
    });
    let differing = left
        .iter()
        .zip(right.iter())
        .filter(|(a, b)| a != b)
        .count()
        + left.len().abs_diff(right.len());
    let window = |bytes: &[u8], at: Option<usize>| -> Option<String> {
        let at = at?;
        let start = at.saturating_sub(8);
        let end = (at + 32).min(bytes.len());
        if start >= end {
            return None;
        }
        Some(
            bytes[start..end]
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>(),
        )
    };
    let ls = wsw_message::cas::sha256_hex(left);
    let rs = wsw_message::cas::sha256_hex(right);
    CompareReport {
        mode: "hex",
        identical: ls == rs,
        left_sha256: ls,
        right_sha256: rs,
        left_len: left.len(),
        right_len: right.len(),
        similarity: None,
        hunks: None,
        json_diff: None,
        header_diff: None,
        hex_diff: Some(HexDiffReport {
            first_diff_offset: first,
            differing_bytes: differing,
            left_len: left.len(),
            right_len: right.len(),
            left_preview_hex: window(left, first),
            right_preview_hex: window(right, first),
        }),
    }
}

/// 比较报告保存 Artifact（wsw_message cas/store 既有面：CAS 正文 +
/// wsw_body_blob 行；报告 JSON 本体即正文）。返回 BodyRef（sha256/uri/
/// size/fidelity）。
pub fn persist_report_as_blob(
    store: &crate::persist::PersistStore,
    artifacts_root: &std::path::Path,
    report: &CompareReport,
) -> Result<wsw_message::cas::BodyRef, String> {
    let text = serde_json::to_string_pretty(report).map_err(|e| e.to_string())?;
    let bytes = text.as_bytes();
    let body_ref = wsw_message::cas::store_body(
        artifacts_root,
        bytes,
        crate::decoder::DECODER_INPUT_CAP as u64,
    )
    .map_err(|e| e.to_string())?;
    let row = wsw_message::store::BodyBlobRow {
        sha256: body_ref.sha256.clone(),
        uri: body_ref.uri.clone(),
        size_bytes: body_ref.size as i64,
        mime_type: "application/json".to_string(),
        first_seen_exchange_id: None,
        created_at: crate::persist::now_iso(),
    };
    store
        .with_conn(|conn| wsw_message::store::register_body_blob(conn, &row))
        .map_err(|e| e.to_string())?;
    Ok(body_ref)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ignore(skip: &[&str], keys: &[&str]) -> CompiledIgnore {
        IgnoreRules {
            skip_line_regexes: skip.iter().map(|s| s.to_string()).collect(),
            normalize_key_regexes: keys.iter().map(|s| s.to_string()).collect(),
            base: NormalizeRules::default(),
        }
        .compile()
        .unwrap()
    }

    #[test]
    fn text_hunks_attribution() {
        // 判据 5：增删改区间归因正确。
        let rep = compare_text("a\nb\nc", "a\nx\nc\nd", &ignore(&[], &[]));
        let hunks = rep.hunks.as_ref().unwrap();
        assert!(hunks
            .iter()
            .any(|h| h.op == "replace" && h.left_start == 1 && h.left_len == 1));
        assert!(hunks
            .iter()
            .any(|h| h.op == "insert" && h.right_start == 3 && h.right_len == 1));
        assert!(rep.similarity.unwrap() < 1.0);
        // 相同判定：sha256 键。
        assert!(!rep.identical);
        let same = compare_text("a\nb", "a\nb", &ignore(&[], &[]));
        assert!(same.identical);
        assert!(same.similarity.unwrap() == 1.0);
    }

    #[test]
    fn regex_skip_and_key_mask_converge() {
        // 判据 5：regex 剔除后区间收敛（skip_line_regexes）。
        let rep = compare_text(
            "line1\nrequest_id=abc-1\nline2",
            "line1\nrequest_id=xyz-99\nline2",
            &ignore(&[r"^request_id="], &[]),
        );
        assert!(rep.hunks.as_ref().unwrap().iter().all(|h| h.op == "equal"));
        assert!((rep.similarity.unwrap() - 1.0).abs() < 1e-9);
        // normalize_key_regexes 键值掩码收敛（JSON 形态）。
        let rep2 = compare_text(
            "{\"ts\": 1700000001, \"v\": 1}",
            "{\"ts\": 1799999999, \"v\": 1}",
            &ignore(&[], &[r"^ts$"]),
        );
        assert!(rep2.identical, "掩码后 sha256 收敛");
        assert!(rep2.hunks.as_ref().unwrap().iter().all(|h| h.op == "equal"));
        // 表单形态掩码。
        let rep3 = compare_text(
            "token=aaa111&keep=1",
            "token=bbb222&keep=1",
            &ignore(&[], &[r"^token$"]),
        );
        assert!(rep3.identical);
        // 日志形态掩码。
        let rep4 = compare_text(
            "ts: 111\nkeep: 1",
            "ts: 222\nkeep: 1",
            &ignore(&[], &[r"^ts$"]),
        );
        assert!(rep4.identical);
    }

    #[test]
    fn regex_invalid_rejected() {
        let err = IgnoreRules {
            skip_line_regexes: vec!["([".into()],
            normalize_key_regexes: vec![],
            base: NormalizeRules::default(),
        }
        .compile();
        assert!(matches!(err, Err(ComparerError::RegexInvalid(_, _))));
    }

    #[test]
    fn json_key_order_insensitive_and_paths() {
        // 判据 5：JSON 键序不敏感 + 归因路径。
        let rep = compare_json("{\"a\":1,\"b\":{\"c\":2}}", "{\"b\":{\"c\":2},\"a\":1}").unwrap();
        assert!(rep.identical);
        assert!(rep.json_diff.as_ref().unwrap().is_empty());
        let rep2 = compare_json("{\"a\":1,\"b\":2}", "{\"a\":2,\"c\":3}").unwrap();
        let entries = rep2.json_diff.as_ref().unwrap();
        assert!(entries
            .iter()
            .any(|e| e.path == "$.a" && e.kind == "changed"));
        assert!(entries
            .iter()
            .any(|e| e.path == "$.b" && e.kind == "removed"));
        assert!(entries.iter().any(|e| e.path == "$.c" && e.kind == "added"));
        assert!(matches!(
            compare_json("{bad", "{}"),
            Err(ComparerError::RegexInvalid(_, _))
        ));
    }

    #[test]
    fn header_per_key_compare() {
        let l = r#"[{"name":"Host","value":"a:1"},{"name":"X-Mark","value":"v1"}]"#;
        let r = r#"[{"name":"X-Mark","value":"v2"},{"name":"X-New","value":"n"}]"#;
        let rep = compare_headers(l, r).unwrap();
        let entries = rep.header_diff.as_ref().unwrap();
        assert!(entries
            .iter()
            .any(|e| e.name == "host" && e.kind == "removed"));
        assert!(entries
            .iter()
            .any(|e| e.name == "x-mark" && e.kind == "changed"));
        assert!(entries
            .iter()
            .any(|e| e.name == "x-new" && e.kind == "added"));
        assert!(!rep.identical);
    }

    #[test]
    fn hex_first_diff_offset_and_length_trap() {
        // 判据 5：Hex 首差异 OFFSET 归因。
        let rep = compare_bytes(&[0, 1, 2, 3], &[0, 1, 9, 3]);
        let hex = rep.hex_diff.as_ref().unwrap();
        assert_eq!(hex.first_diff_offset, Some(2));
        assert_eq!(hex.differing_bytes, 1);
        assert!(!rep.identical);
        // 负例（PLAN :303）：长度相同内容不同 → 相同判定 false。
        let rep2 = compare_bytes(b"AAAA", b"AAAB");
        assert!(rep2.left_len == rep2.right_len);
        assert!(!rep2.identical, "长度相同 ≠ 内容相同");
        // 长度差异：尾部偏移归因。
        let rep3 = compare_bytes(b"AB", b"ABCD");
        assert_eq!(rep3.hex_diff.as_ref().unwrap().first_diff_offset, Some(2));
        assert!(rep3.hex_diff.as_ref().unwrap().differing_bytes >= 2);
        // 相同字节 = identical true。
        assert!(compare_bytes(b"", b"").identical);
    }
}
