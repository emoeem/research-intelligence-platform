//! `wsw_message::diff` 原语集成测试（WSW-4 S2；D6 判据面）：
//!
//! - 正例：单行增/删/改区间、多块区间、归一化命中（键值掩码 + 行跳过）；
//! - 负例：**等长不同内容 → 相似度 <1 非同响应**（PLAN :303 判据钉值）；
//! - 空文本/单行/超帽降级边界。
//!
//! 纪律：`--test-threads=1`（WSW 批测试口径；本文件纯函数面无 socket，
//! 串行仅为口径一致）。

use wsw_message::diff::{diff_lines, diff_normalized, similarity_ratio, NormalizeRules, DiffOp};

#[test]
fn identical_no_diff_full_similarity() {
    let t = "GET /\nHost: x\nbody";
    let hunks = diff_lines(t, t);
    assert_eq!(hunks.len(), 1, "全等 = 单 Equal 区间");
    assert_eq!(hunks[0].op, DiffOp::Equal);
    assert!((similarity_ratio(t, t) - 1.0).abs() < 1e-9);
}

#[test]
fn single_line_insert_delete_replace() {
    // 插入：右多一行。
    let hunks = diff_lines("a\nc", "a\nb\nc");
    let ops: Vec<DiffOp> = hunks.iter().map(|h| h.op).collect();
    assert_eq!(ops, vec![DiffOp::Equal, DiffOp::Insert, DiffOp::Equal]);
    assert_eq!(hunks[1].right_len, 1);
    assert_eq!(hunks[1].left_len, 0);
    // 删除：右少一行。
    let hunks = diff_lines("a\nb\nc", "a\nc");
    let ops: Vec<DiffOp> = hunks.iter().map(|h| h.op).collect();
    assert_eq!(ops, vec![DiffOp::Equal, DiffOp::Delete, DiffOp::Equal]);
    // 改：同位替换合并为 Replace。
    let hunks = diff_lines("a\nold\nc", "a\nnew\nc");
    let ops: Vec<DiffOp> = hunks.iter().map(|h| h.op).collect();
    assert_eq!(ops, vec![DiffOp::Equal, DiffOp::Replace, DiffOp::Equal]);
    assert_eq!(hunks[1].left_len, 1);
    assert_eq!(hunks[1].right_len, 1);
}

#[test]
fn multi_block_intervals_ordered() {
    let hunks = diff_lines("H1\nA\nM1\nB\nH2", "H1\nX\nM1\nY\nH2");
    let non_equal: Vec<&wsw_message::diff::DiffHunk> =
        hunks.iter().filter(|h| h.op != DiffOp::Equal).collect();
    assert_eq!(non_equal.len(), 2, "两个独立差异块");
    assert_eq!(non_equal[0].left_start, 1);
    assert_eq!(non_equal[1].left_start, 3);
}

#[test]
fn negative_equal_length_different_content_similarity_below_one() {
    // PLAN :303：响应正文长度相同 ≠ 内容相同——等长不同内容必须 <1。
    let left = "alpha\nbravo\ncharlie\ndelta";
    let right = "alpha\nbravo\ncharlie\ndeltaX";
    let sim = similarity_ratio(left, right);
    assert!(sim < 1.0, "等长不同内容相似度必须 <1：{sim}");
    assert!(sim > 0.0, "公共前缀行贡献部分相似度：{sim}");
    // 完全无公共行 → 0。
    let sim0 = similarity_ratio("a\nb", "c\nd");
    assert!(sim0 < 1.0);
    assert!((sim0 - 0.0).abs() < 1e-9);
}

#[test]
fn normalized_timestamp_mask_makes_equal() {
    let rules = NormalizeRules {
        skip_line_substrings: vec![],
        normalize_keys: vec!["timestamp".into()],
    };
    let a = "status=ok\ntimestamp=2026-11-22T10:00:00.000Z\nbody";
    let b = "status=ok\ntimestamp=2026-11-22T10:00:07.512Z\nbody";
    let (hunks, sim) = diff_normalized(a, b, &rules);
    assert!(hunks.iter().all(|h| h.op == DiffOp::Equal), "掩码后全等");
    assert!((sim - 1.0).abs() < 1e-9);
    // 未掩码时这两行是 Replace（证明掩码确实生效而非恒等）。
    let raw_sim = similarity_ratio(a, b);
    assert!(raw_sim < 1.0);
}

#[test]
fn normalized_json_quoted_and_unquoted_values() {
    let rules = NormalizeRules {
        skip_line_substrings: vec![],
        normalize_keys: vec!["session".into(), "ts".into()],
    };
    let a = "{\"ts\": 1700000001, \"session\": \"abc-111\", \"ok\": true}";
    let b = "{\"ts\": 1799999999, \"session\": \"zzz-999\", \"ok\": true}";
    let (_, sim) = diff_normalized(a, b, &rules);
    assert!((sim - 1.0).abs() < 1e-9, "JSON 双键掩码后全等：sim={sim}");
}

#[test]
fn normalized_skip_line_drops_dynamic_noise() {
    let rules = NormalizeRules {
        skip_line_substrings: vec!["request_id".into()],
        normalize_keys: vec![],
    };
    let (_, sim) = diff_normalized(
        "H\nrequest_id=aaa-1\nT",
        "H\nrequest_id=bbb-2\nT",
        &rules,
    );
    assert!((sim - 1.0).abs() < 1e-9);
}

#[test]
fn cap_degraded_ranges_cover_all_lines() {
    let left: Vec<String> = (0..3000).map(|i| format!("line-{i}")).collect();
    let mut right = left.clone();
    right[2999] = "tail-changed".into();
    let hunks = diff_lines(&left.join("\n"), &right.join("\n"));
    let covered_left: usize = hunks.iter().map(|h| h.left_len).sum();
    let covered_right: usize = hunks.iter().map(|h| h.right_len).sum();
    assert_eq!(covered_left, 3000, "区间表必须闭合覆盖左侧行数");
    assert_eq!(covered_right, 3000);
    assert!(similarity_ratio(&left.join("\n"), &right.join("\n")) < 1.0);
}

#[test]
fn empty_edges() {
    assert!(diff_lines("", "").is_empty());
    assert!((similarity_ratio("", "") - 1.0).abs() < 1e-9);
    let hunks = diff_lines("", "x");
    assert_eq!(hunks.len(), 1);
    assert_eq!(hunks[0].op, DiffOp::Insert);
}
