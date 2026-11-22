//! 行级 diff 原语（WSW-4 D6；PLAN §4.7 :296-303 语义面）。
//!
//! 交付三面（D6 裁决原文）：
//! - 行级 LCS diff（增/删/改区间，[`DiffOp`] 三词表：Equal/Insert/Delete，
//!   改 = 同位 Delete+Insert 区间对合并的 [`DiffOp::Replace`]）；
//! - 相似度比率 = `2·LCS / (左行数 + 右行数)`（Dice 系数口径，值域
//!   0.0..=1.0；**长度相同 ≠ 内容相同**——PLAN :303 明文，等长不同内容
//!   无公共行 → 0.0 < 1，负例钉在 `tests/diff_primitives.rs`）；
//! - 归一化比较（忽略规则清单：动态字段/时间戳键值掩码 + 行级跳过子串；
//!   PLAN :301 语义。**纯 std 零依赖**——D6 裁决 + D9 红线，正则面归
//!   wsw-runtime fuzz/analyzer，本模块不做模式匹配）。
//!
//! 规模帽（[`DIFF_LINE_CAP`]）：LCS DP O(n·m)，两侧各截前 2000 行参与
//! 比较，超出部分并入尾区间（如实降级，不炸内存；真实响应正文 diff
//! 场景常见差异集中在中部，前缀/后缀公共行先廉价剥离再进 DP）。
//!
//! 消费面：wsw-runtime fuzz/analyzer（baseline 对比 + similarity 派生列）
//! 与 WSW-6 Comparer（完整 UI 面，禁第二 diff 实现——D6）。

/// 单侧参与 DP 的最大行数（超出截断，尾区间合并；见模块 doc）。
pub const DIFF_LINE_CAP: usize = 2000;

/// 区间操作词表。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffOp {
    /// 两侧行相同。
    Equal,
    /// 右侧新增行（左区间空）。
    Insert,
    /// 左侧删除行（右区间空）。
    Delete,
    /// 改（同位 Delete+Insert 合并区间）。
    Replace,
}

/// 一个连续差异区间（行号 0 起，`*_len` 可为 0——纯增/纯删时另一侧空）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffHunk {
    /// 操作词表。
    pub op: DiffOp,
    /// 左侧起始行号（左区间空时 = 当前行位）。
    pub left_start: usize,
    /// 左侧行数（Insert = 0）。
    pub left_len: usize,
    /// 右侧起始行号（右区间空时 = 当前行位）。
    pub right_start: usize,
    /// 右侧行数（Delete = 0）。
    pub right_len: usize,
}

/// 归一化忽略规则（PLAN :301「动态字段/时间戳/指定头」的 std 面）。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct NormalizeRules {
    /// 行级跳过：行内包含任一子串即从两侧比较中剔除（如随机 id 噪声行）。
    pub skip_line_substrings: Vec<String>,
    /// 键值掩码：命中键名的值替换为 `«key»` 占位（时间戳/动态字段）。
    /// 匹配形态：JSON `"key": <value>`、表单 `key=<value>`（至 `&` 止）、
    /// 日志 `key: <value>`（至行尾）。键名逐字区分大小写（消息头大小写
    /// 归一由调用方先行）。
    pub normalize_keys: Vec<String>,
}

/// 按行切分（`\n`；尾 `\r` 保留原样——调用方字节面自管，本模块不做隐形
/// 归一，保证 diff 语义可复现）。
fn split_lines(text: &str) -> Vec<&str> {
    if text.is_empty() {
        return Vec::new();
    }
    let mut lines: Vec<&str> = text.split('\n').collect();
    // 尾换行产生的空尾行不入行集（"a\n" 与 "a" 同一行集——行数语义按
    // 可见行计，不被结尾换行影响）。
    if lines.len() > 1 && lines.last().is_some_and(|l| l.is_empty()) {
        lines.pop();
    }
    lines
}

/// LCS DP 表（`t[(i*(m+1))+j]` = a[i..] 与 b[j..] 的 LCS 长度）。
fn lcs_table(a: &[&str], b: &[&str]) -> Vec<u32> {
    let n = a.len();
    let m = b.len();
    let mut t = vec![0u32; (n + 1) * (m + 1)];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            t[i * (m + 1) + j] = if a[i] == b[j] {
                t[(i + 1) * (m + 1) + (j + 1)] + 1
            } else {
                t[(i + 1) * (m + 1) + j].max(t[i * (m + 1) + (j + 1)])
            };
        }
    }
    t
}

/// 行级 LCS diff（前缀/后缀公共行剥离 + 帽内 DP + 同位 Delete/Insert 合并
/// 为 Replace）。返回按序区间表（Equal 区间也在内，消费方可直接渲染）。
pub fn diff_lines(left: &str, right: &str) -> Vec<DiffHunk> {
    let a: Vec<&str> = split_lines(left);
    let b: Vec<&str> = split_lines(right);

    // 前缀/后缀公共行剥离（廉价面：让 DP 只处理中部差异区）。
    let mut pre = 0usize;
    while pre < a.len() && pre < b.len() && a[pre] == b[pre] {
        pre += 1;
    }
    let mut suf = 0usize;
    while suf < a.len() - pre && suf < b.len() - pre
        && a[a.len() - 1 - suf] == b[b.len() - 1 - suf]
    {
        suf += 1;
    }
    let a_mid = &a[pre..a.len() - suf];
    let b_mid = &b[pre..b.len() - suf];

    // 帽截断（超出部分并入尾部 Replace 区间，如实降级）。
    let a_trunc = a_mid.len() > DIFF_LINE_CAP;
    let b_trunc = b_mid.len() > DIFF_LINE_CAP;
    let a_dp = &a_mid[..a_mid.len().min(DIFF_LINE_CAP)];
    let b_dp = &b_mid[..b_mid.len().min(DIFF_LINE_CAP)];

    let mut hunks: Vec<DiffHunk> = Vec::new();
    // 前缀公共行 = 头部 Equal 区间（覆盖对账面：区间表必须闭合覆盖两侧行数）。
    if pre > 0 {
        hunks.push(DiffHunk {
            op: DiffOp::Equal,
            left_start: 0,
            left_len: pre,
            right_start: 0,
            right_len: pre,
        });
    }
    if a_mid.is_empty() && b_mid.is_empty() {
        return hunks;
    }

    let t = lcs_table(a_dp, b_dp);
    let (n, m) = (a_dp.len(), b_dp.len());
    // 逐行前向走表 → 操作序列（同向连续合并由 hunk 收尾统一做）。
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum Step {
        Equal,
        Del,
        Ins,
    }
    let mut steps: Vec<Step> = Vec::with_capacity(n + m);
    let (mut i, mut j) = (0usize, 0usize);
    while i < n && j < m {
        if a_dp[i] == b_dp[j] {
            steps.push(Step::Equal);
            i += 1;
            j += 1;
        } else if t[(i + 1) * (m + 1) + j] >= t[i * (m + 1) + (j + 1)] {
            steps.push(Step::Del);
            i += 1;
        } else {
            steps.push(Step::Ins);
            j += 1;
        }
    }
    while i < n {
        steps.push(Step::Del);
        i += 1;
    }
    while j < m {
        steps.push(Step::Ins);
        j += 1;
    }

    // 步骤序列 → 区间（同位 Del+Ins 对合并为 Replace）。
    let mut k = 0usize;
    let mut li = pre;
    let mut ri = pre;
    while k < steps.len() {
        match steps[k] {
            Step::Equal => {
                let start_k = k;
                while k < steps.len() && steps[k] == Step::Equal {
                    k += 1;
                    li += 1;
                    ri += 1;
                }
                hunks.push(DiffHunk {
                    op: DiffOp::Equal,
                    left_start: li - (k - start_k),
                    left_len: k - start_k,
                    right_start: ri - (k - start_k),
                    right_len: k - start_k,
                });
            }
            Step::Del => {
                let del_start = li;
                let mut del_len = 0usize;
                while k < steps.len() && steps[k] == Step::Del {
                    k += 1;
                    li += 1;
                    del_len += 1;
                }
                // 紧随的 Ins 与 Del 同位 → Replace（改）。
                let ins_start = ri;
                let mut ins_len = 0usize;
                while k < steps.len() && steps[k] == Step::Ins {
                    k += 1;
                    ri += 1;
                    ins_len += 1;
                }
                let op = if ins_len > 0 { DiffOp::Replace } else { DiffOp::Delete };
                hunks.push(DiffHunk {
                    op,
                    left_start: del_start,
                    left_len: del_len,
                    right_start: ins_start,
                    right_len: ins_len,
                });
            }
            Step::Ins => {
                let ins_start = ri;
                let mut ins_len = 0usize;
                while k < steps.len() && steps[k] == Step::Ins {
                    k += 1;
                    ri += 1;
                    ins_len += 1;
                }
                hunks.push(DiffHunk {
                    op: DiffOp::Insert,
                    left_start: li,
                    left_len: 0,
                    right_start: ins_start,
                    right_len: ins_len,
                });
            }
        }
    }

    // 帽截断降级：中部超出帽的剩余行并入尾 Replace 区间（两截断位对齐）。
    if a_trunc || b_trunc {
        let tail_left = a_mid.len() - a_dp.len();
        let tail_right = b_mid.len() - b_dp.len();
        if tail_left > 0 || tail_right > 0 {
            hunks.push(DiffHunk {
                op: DiffOp::Replace,
                left_start: pre + a_dp.len(),
                left_len: tail_left,
                right_start: pre + b_dp.len(),
                right_len: tail_right,
            });
        }
        // 截断后尾部 suf 公共行区间照常追加。
        if suf > 0 {
            hunks.push(DiffHunk {
                op: DiffOp::Equal,
                left_start: a.len() - suf,
                left_len: suf,
                right_start: b.len() - suf,
                right_len: suf,
            });
        }
    } else if suf > 0 {
        hunks.push(DiffHunk {
            op: DiffOp::Equal,
            left_start: a.len() - suf,
            left_len: suf,
            right_start: b.len() - suf,
            right_len: suf,
        });
    }
    hunks
}

/// 行集相似度 = `2·LCS / (左行数 + 右行数)`（两侧皆空 = 1.0；帽内 DP，
/// 超帽按截断行集计——如实降级语义与 [`diff_lines`] 同源）。
pub fn similarity_ratio(left: &str, right: &str) -> f64 {
    let a = split_lines(left);
    let b = split_lines(right);
    if a.is_empty() && b.is_empty() {
        return 1.0;
    }
    // LCS 只需长度：剥前缀/后缀后按帽截断求表值。
    let mut pre = 0usize;
    while pre < a.len() && pre < b.len() && a[pre] == b[pre] {
        pre += 1;
    }
    let mut suf = 0usize;
    while suf < a.len() - pre && suf < b.len() - pre
        && a[a.len() - 1 - suf] == b[b.len() - 1 - suf]
    {
        suf += 1;
    }
    let a_mid = &a[pre..a.len() - suf];
    let b_mid = &b[pre..b.len() - suf];
    let a_dp = &a_mid[..a_mid.len().min(DIFF_LINE_CAP)];
    let b_dp = &b_mid[..b_mid.len().min(DIFF_LINE_CAP)];
    let lcs = lcs_table(a_dp, b_dp)[0] as usize + pre + suf;
    let denom = a.len() + b.len();
    if denom == 0 {
        return 1.0;
    }
    (2.0 * lcs as f64 / denom as f64).min(1.0)
}

/// 键值掩码（单键单行；命中形态：`"key": value`（JSON）/ `key=value`（至
/// `&` 止）/ `key: value`（至行尾）。未命中原样返回）。
fn mask_key(line: &str, key: &str) -> String {
    let placeholder = format!("«{key}»");
    // JSON 形态："key": <value>（value 带引号取闭合引号，不带引号取至
    // `,`/`}`/`]` 止）。
    let json_pat = format!("\"{key}\":");
    if let Some(pos) = line.find(&json_pat) {
        let rest = &line[pos + json_pat.len()..];
        let trimmed = rest.trim_start();
        let skip = rest.len() - trimmed.len();
        if trimmed.starts_with('"') {
            // 带引号值：取到下一个非转义引号（占位符换值，引号保留）。
            let bytes = trimmed.as_bytes();
            let mut end = 1usize;
            while end < bytes.len() {
                if bytes[end] == b'\\' {
                    end += 2;
                    continue;
                }
                if bytes[end] == b'"' {
                    break;
                }
                end += 1;
            }
            let close = (end + 1).min(trimmed.len());
            return format!(
                "{}{}{}",
                &line[..pos + json_pat.len() + skip],
                placeholder,
                &trimmed[close..]
            );
        }
        // 无引号值：到逗号/花括号/行尾。
        let stop = trimmed
            .find([',', '}', ']'])
            .unwrap_or(trimmed.len());
        return format!(
            "{}{}{}",
            &line[..pos + json_pat.len() + skip],
            placeholder,
            &trimmed[stop..]
        );
    }
    // 表单形态：key=value（至 & 止）。
    let form_pat = format!("{key}=");
    if let Some(pos) = line.find(&form_pat) {
        let rest = &line[pos + form_pat.len()..];
        let stop = rest.find('&').unwrap_or(rest.len());
        return format!(
            "{}{}{}",
            &line[..pos + form_pat.len()],
            placeholder,
            &rest[stop..]
        );
    }
    // 日志形态：key: value（至行尾）。
    let log_pat = format!("{key}: ");
    if let Some(pos) = line.find(&log_pat) {
        return format!(
            "{}{}",
            &line[..pos + log_pat.len()],
            placeholder
        );
    }
    line.to_string()
}

/// 归一化：行级跳过 + 键值掩码（顺序 = 先掩码后跳过判定——掩码命中后行
/// 仍有其它内容则保留；跳过判定基于掩码后文本，动态值噪声不再触发误跳）。
pub fn normalize_text(text: &str, rules: &NormalizeRules) -> String {
    let mut out: Vec<String> = Vec::new();
    for line in split_lines(text) {
        let mut owned = line.to_string();
        for key in &rules.normalize_keys {
            owned = mask_key(&owned, key);
        }
        if rules
            .skip_line_substrings
            .iter()
            .any(|s| !s.is_empty() && owned.contains(s.as_str()))
        {
            continue;
        }
        out.push(owned);
    }
    out.join("\n")
}

/// 归一化 diff 一步面：`(区间表, 相似度)`，两侧先 [`normalize_text`] 再
/// [`diff_lines`] / [`similarity_ratio`]。
pub fn diff_normalized(
    left: &str,
    right: &str,
    rules: &NormalizeRules,
) -> (Vec<DiffHunk>, f64) {
    let l = normalize_text(left, rules);
    let r = normalize_text(right, rules);
    let hunks = diff_lines(&l, &r);
    let sim = similarity_ratio(&l, &r);
    (hunks, sim)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_text_single_equal_hunk() {
        let hunks = diff_lines("a\nb\nc", "a\nb\nc");
        assert_eq!(hunks.len(), 1);
        assert_eq!(hunks[0].op, DiffOp::Equal);
        assert_eq!(similarity_ratio("a\nb\nc", "a\nb\nc"), 1.0);
    }

    #[test]
    fn equal_length_different_content_similarity_below_one() {
        // PLAN :303 负例：等长不同内容 ≠ 同响应。
        let sim = similarity_ratio("alpha\nbravo\ncharlie", "alpha\nbravo\ncharlieX");
        assert!(sim < 1.0, "等长不同内容相似度必须 <1，得到 {sim}");
        assert!(sim > 0.0, "公共前缀行应有部分相似度");
        let hunks = diff_lines("alpha\nbravo\ncharlie", "alpha\nbravo\ncharlieX");
        assert!(hunks.iter().any(|h| h.op == DiffOp::Replace));
    }

    #[test]
    fn pure_insert_and_delete_hunks() {
        let hunks = diff_lines("a\nc", "a\nb\nc");
        // 前缀/后缀公共行各产出 Equal 区间，中部 Insert。
        assert_eq!(hunks.len(), 3);
        assert_eq!(hunks[0].op, DiffOp::Equal);
        assert_eq!(hunks[1].op, DiffOp::Insert);
        assert_eq!(hunks[1].right_len, 1);
        assert_eq!(hunks[2].op, DiffOp::Equal);
        let back = diff_lines("a\nb\nc", "a\nc");
        assert_eq!(back[1].op, DiffOp::Delete);
        // 无公共前缀的纯插入：单 Insert 区间。
        let only = diff_lines("", "only");
        assert_eq!(only.len(), 1);
        assert_eq!(only[0].op, DiffOp::Insert);
    }

    #[test]
    fn multi_block_diff() {
        let hunks = diff_lines("a\nX\nb\nY\nc", "a\nx\nb\ny\nc");
        // 两个中部 Replace 区间 + 前后 Equal。
        let replaces: Vec<_> = hunks.iter().filter(|h| h.op == DiffOp::Replace).collect();
        assert_eq!(replaces.len(), 2);
        assert_eq!(replaces[0].left_start, 1);
        assert_eq!(replaces[1].left_start, 3);
    }

    #[test]
    fn normalize_masks_json_and_form_values() {
        let rules = NormalizeRules {
            skip_line_substrings: vec![],
            normalize_keys: vec!["ts".to_string()],
        };
        let a = "{\"ts\": 1700000001, \"v\": 1}";
        let b = "{\"ts\": 1799999999, \"v\": 1}";
        let na = normalize_text(a, &rules);
        let nb = normalize_text(b, &rules);
        assert_eq!(na, nb, "时间戳键掩码后应相等");
        assert!(na.contains("«ts»"), "占位符在场");
        let sim = similarity_ratio(&na, &nb);
        assert!((sim - 1.0).abs() < 1e-9);
    }

    #[test]
    fn normalize_skip_line_drops_noise() {
        let rules = NormalizeRules {
            skip_line_substrings: vec!["request_id=".to_string()],
            normalize_keys: vec![],
        };
        let (hunks, sim) = diff_normalized(
            "line1\nrequest_id=abc-1\nline2",
            "line1\nrequest_id=xyz-99\nline2",
            &rules,
        );
        assert!(hunks.iter().all(|h| h.op == DiffOp::Equal));
        assert!((sim - 1.0).abs() < 1e-9);
    }

    #[test]
    fn cap_degrades_without_panic() {
        // 超帽行集：不炸内存、区间表仍闭合（总覆盖两侧行数）。
        let big: String = (0..3000).map(|i| format!("l{i}")).collect::<Vec<_>>().join("\n");
        let mut other: Vec<String> = (0..3000).map(|i| format!("l{i}")).collect();
        other[2500] = "changed".into();
        let hunks = diff_lines(&big, &other.join("\n"));
        let covered_left: usize = hunks.iter().map(|h| h.left_len).sum();
        let covered_right: usize = hunks.iter().map(|h| h.right_len).sum();
        assert_eq!(covered_left, 3000);
        assert_eq!(covered_right, 3000);
    }

    #[test]
    fn empty_and_single_line_edges() {
        assert!(diff_lines("", "").is_empty());
        assert_eq!(similarity_ratio("", ""), 1.0);
        let hunks = diff_lines("", "only");
        assert_eq!(hunks.len(), 1);
        assert_eq!(hunks[0].op, DiffOp::Insert);
        assert_eq!(similarity_ratio("", "only"), 0.0);
    }
}
