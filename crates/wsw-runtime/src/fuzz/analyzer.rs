//! `wsw_runtime::fuzz::analyzer`：结果分析（WSW-4 D6；PLAN :256-263）。
//!
//! 四面：
//! - **错误分类**（PLAN :260 网络错误/超时/限流区分）：`SendError` 词形映射
//!   （sender.rs:204 词表直通）+ HTTP 429/5xx 限流面（`fuzz_rate_limited` /
//!   `fuzz_server_error` 自别前缀——红线 6）；
//! - **关键字/正则命中**（PLAN :258）：match_hits_json 投影（计数可见）；
//! - **sha256 分组去重**（PLAN :261）：去重键 = body sha256 非（长度——
//!   PLAN :303「长度相同 ≠ 内容相同」负例钉 diff_primitives）；
//! - **baseline 对比**（D6）：显式 baseline_case_id 或首个 RESPONDED case
//!   缺省 → `wsw_message::diff` 原语（禁第二 diff 实现——D6）计算
//!   similarity + diff_class 派生列。
//!
//! diff_class 词表：`baseline`（baseline case 本身）/`duplicate`（body sha
//! 与 baseline 或先前 case 相同）/`unique`（sha 无重复且异于 baseline）。

use crate::sender::SendError;
use serde::{Deserialize, Serialize};
use wsw_message::diff;

/// diff_class 词表（analyzer 落 028 diff_class 列）。
pub const DIFF_CLASS_BASELINE: &str = "baseline";
pub const DIFF_CLASS_DUPLICATE: &str = "duplicate";
pub const DIFF_CLASS_UNIQUE: &str = "unique";

/// 错误分类产物：(case 七态, error_code 词形)。
/// SendError 词形直通（sender.rs:143/:204 既有词形；Cancelled →
/// NETWORK_ERROR + "cancelled"——七态无独立取消态，如实映射登记批次报告）。
pub fn classify_send_error(err: &SendError) -> (&'static str, &'static str) {
    match err {
        SendError::Timeout(_) => ("TIMEOUT", "timed_out"),
        SendError::Connect(_) => ("NETWORK_ERROR", "connect_failed"),
        SendError::Tls(_) => ("NETWORK_ERROR", "tls_failed"),
        SendError::Read(_) => ("NETWORK_ERROR", "read_failed"),
        SendError::Serialize(_) => ("NETWORK_ERROR", "serialize_failed"),
        SendError::Scope(_) => ("NETWORK_ERROR", "scope_blocked"),
        SendError::CapExceeded(_) => ("NETWORK_ERROR", "cap_exceeded"),
        SendError::Cancelled => ("NETWORK_ERROR", "cancelled"),
    }
}

/// HTTP 状态分类（429 限流 / 5xx 服务端错误；2xx-4xx 正常 → None）。
pub fn classify_status(status: u16) -> Option<&'static str> {
    match status {
        429 => Some("fuzz_rate_limited"),
        500..=599 => Some("fuzz_server_error"),
        _ => None,
    }
}

/// 命中记录（match_hits_json 数组项）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MatchHit {
    /// 'keyword'/'regex'。
    pub kind: &'static str,
    /// 命中模式原文。
    pub pattern: String,
    /// 体文本命中次数。
    pub count: usize,
}

/// 关键字/正则命中（body 文本面 lossy；PLAN :258）。
pub fn match_hits(
    body: &[u8],
    keywords: &[String],
    regexes: &[regex::Regex],
) -> Vec<MatchHit> {
    let text = String::from_utf8_lossy(body);
    let mut hits = Vec::new();
    for kw in keywords {
        if kw.is_empty() {
            continue;
        }
        let count = text.matches(kw.as_str()).count();
        if count > 0 {
            hits.push(MatchHit { kind: "keyword", pattern: kw.clone(), count });
        }
    }
    for re in regexes {
        let count = re.find_iter(&text).count();
        if count > 0 {
            hits.push(MatchHit { kind: "regex", pattern: re.as_str().to_string(), count });
        }
    }
    hits
}

/// 体文本相似度（`wsw_message::diff::similarity_ratio` 单点；lossy UTF-8
/// 投影——禁第二 diff 实现，D6）。
pub fn similarity(baseline: &[u8], body: &[u8]) -> f64 {
    diff::similarity_ratio(
        &String::from_utf8_lossy(baseline),
        &String::from_utf8_lossy(body),
    )
}

/// 组值 sha256（payload 向量对账锚）。
pub fn payload_sha256(assignments_json: &str) -> String {
    wsw_message::cas::sha256_hex(assignments_json.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn send_error_classification_matrix() {
        let cases: Vec<(SendError, &str, &str)> = vec![
            (SendError::Timeout("t".into()), "TIMEOUT", "timed_out"),
            (SendError::Connect("c".into()), "NETWORK_ERROR", "connect_failed"),
            (SendError::Tls("t".into()), "NETWORK_ERROR", "tls_failed"),
            (SendError::Read("r".into()), "NETWORK_ERROR", "read_failed"),
            (SendError::Serialize("s".into()), "NETWORK_ERROR", "serialize_failed"),
            (SendError::Scope("scope_blocked".into()), "NETWORK_ERROR", "scope_blocked"),
            (SendError::CapExceeded(8), "NETWORK_ERROR", "cap_exceeded"),
            (SendError::Cancelled, "NETWORK_ERROR", "cancelled"),
        ];
        for (err, state, code) in cases {
            assert_eq!(classify_send_error(&err), (state, code));
        }
    }

    #[test]
    fn status_classification() {
        assert_eq!(classify_status(429), Some("fuzz_rate_limited"));
        assert_eq!(classify_status(500), Some("fuzz_server_error"));
        assert_eq!(classify_status(503), Some("fuzz_server_error"));
        assert_eq!(classify_status(200), None);
        assert_eq!(classify_status(404), None);
    }

    #[test]
    fn keyword_and_regex_hits() {
        let re = regex::Regex::new(r"error-\d+").unwrap();
        let hits = match_hits(b"ok error-1 x error-22 none", &["x".to_string()], &[re]);
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0], MatchHit { kind: "keyword", pattern: "x".into(), count: 1 });
        assert_eq!(hits[1], MatchHit { kind: "regex", pattern: r"error-\d+".into(), count: 2 });
        let none = match_hits(b"nothing here", &["zzz".to_string()], &[]);
        assert!(none.is_empty());
    }

    #[test]
    fn similarity_via_diff_primitive() {
        // 等长不同内容 → <1（PLAN :303，经 wsw_message::diff 单点）。
        assert!(similarity(b"a\nb\nc", b"a\nb\nx") < 1.0);
        assert!((similarity(b"a\nb", b"a\nb") - 1.0).abs() < 1e-9);
    }
}
