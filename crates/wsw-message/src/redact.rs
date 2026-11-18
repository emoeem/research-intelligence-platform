//! 脱敏接口：规则输入 → Display 输出（DESIGN §4.5 唯一挂点）。
//!
//! 词表（WSW-1-TASK §三 S5「authorization/cookie/api-key/token/password/
//! set-cookie 七头」）：任务书点名的六头为 authorization / cookie / api-key /
//! token / password / set-cookie，第七项取 authorization 的标准姊妹头
//! **proxy-authorization**（Burp 侧会话脱敏同名单；DESIGN §4.5
//! 「Cookie/Authorization/API Key/Token/密码」语义族覆盖）。自定义规则位 =
//! [`RedactRules::extra_header_names`]（DESIGN §4.5「自定义规则」位）。
//!
//! 纪律（不变式 3）：本模块为纯函数，输入 `&ParsedMessage` 只读借用，输出
//! [`DisplayPayload`] 独立新值 —— 原始字节不动，仅 Display 变换；值替换为
//! 掩码，头名与位置原样（展示序保持）。凭据不落任何日志/错误信息（§4.5）；
//! 导出脱敏预览面 WSW-3+ 经本函数消费。

use crate::types::{DisplayPayload, HeaderRecord, ParsedMessage};
use serde::{Deserialize, Serialize};

/// 默认脱敏词表（七头，小写；匹配大小写不敏感）。
pub const DEFAULT_REDACT_HEADERS: [&str; 7] = [
    "authorization",
    "proxy-authorization",
    "cookie",
    "set-cookie",
    "api-key",
    "token",
    "password",
];

/// 脱敏规则输入（DESIGN §4.5 规则面）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RedactRules {
    /// 自定义规则位：额外脱敏头名（大小写不敏感匹配）。
    pub extra_header_names: Vec<String>,
    /// 掩码串（缺省 `[REDACTED]`）。
    pub mask: String,
    /// 体预览帽（字节；缺省 200 —— POC 先例 `crates/wsw-poc/src/main.rs:45-47`）。
    pub body_preview_cap: usize,
}

impl Default for RedactRules {
    fn default() -> Self {
        Self {
            extra_header_names: Vec::new(),
            mask: "[REDACTED]".to_string(),
            body_preview_cap: 200,
        }
    }
}

/// 脱敏（纯函数；原始字节不动，仅 Display 变换）。
///
/// 匹配：头名大小写不敏感比对默认七头 + 自定义词表；命中的头值替换为
/// 掩码（名/位置原样），并在 [`DisplayPayload::redacted_header_names`]
/// 记录（保序去重）。体预览 lossy + 帽截断（POC 先例）。
pub fn redact(msg: &ParsedMessage, rules: &RedactRules) -> DisplayPayload {
    let names: Vec<&str> = DEFAULT_REDACT_HEADERS
        .iter()
        .copied()
        .chain(rules.extra_header_names.iter().map(|s| s.as_str()))
        .collect();
    let mut redacted: Vec<String> = Vec::new();
    let headers = msg
        .headers
        .iter()
        .map(|h| {
            let hit = names.iter().any(|n| h.name.eq_ignore_ascii_case(n));
            if hit {
                if !redacted.iter().any(|r| r == &h.name) {
                    redacted.push(h.name.clone());
                }
                HeaderRecord::new(h.name.clone(), rules.mask.clone())
            } else {
                h.clone()
            }
        })
        .collect();
    let cap = rules.body_preview_cap;
    let head = &msg.body[..msg.body.len().min(cap)];
    DisplayPayload {
        headers,
        body_preview: String::from_utf8_lossy(head).into_owned(),
        body_preview_lossy: std::str::from_utf8(head).is_err(),
        body_preview_truncated: msg.body.len() > cap,
        redacted_header_names: redacted,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::MessageKind;

    fn msg_with(headers: Vec<HeaderRecord>) -> ParsedMessage {
        ParsedMessage {
            kind: MessageKind::Request,
            method: Some("POST".into()),
            target: Some("/login".into()),
            status: None,
            reason: None,
            version: crate::types::HttpVersion::Http11,
            headers,
            trailers: vec![],
            body: b"user=me&password=secret".to_vec(),
            delimiting: crate::types::BodyDelimiting::ContentLength,
        }
    }

    #[test]
    fn seven_header_vocabulary_matched_case_insensitive() {
        let msg = msg_with(vec![
            HeaderRecord::new("Authorization", "Bearer abc"),
            HeaderRecord::new("PROXY-AUTHORIZATION", "Basic def"),
            HeaderRecord::new("Cookie", "sid=1"),
            HeaderRecord::new("set-COOKIE", "sid=2"),
            HeaderRecord::new("API-KEY", "k2"),
            HeaderRecord::new("Token", "t1"),
            HeaderRecord::new("password", "p1"),
            HeaderRecord::new("X-Clean", "keep"),
        ]);
        let out = redact(&msg, &RedactRules::default());
        assert_eq!(out.redacted_header_names.len(), 7, "七头全命中");
        for h in &out.headers {
            match h.name.to_ascii_lowercase().as_str() {
                "authorization"
                | "proxy-authorization"
                | "cookie"
                | "set-cookie"
                | "api-key"
                | "token"
                | "password" => assert_eq!(h.value, "[REDACTED]"),
                _ => assert_eq!(h.value, "keep"),
            }
        }
        // 头名/位置原样
        assert_eq!(out.headers[0].name, "Authorization");
        assert_eq!(out.headers[7].name, "X-Clean");
    }

    #[test]
    fn custom_rules_bit_and_mask() {
        let msg = msg_with(vec![
            HeaderRecord::new("X-Secret", "s"),
            HeaderRecord::new("Authorization", "Bearer abc"),
        ]);
        let rules = RedactRules {
            extra_header_names: vec!["x-secret".into()],
            mask: "***".into(),
            body_preview_cap: 200,
        };
        let out = redact(&msg, &rules);
        assert_eq!(out.headers[0].value, "***");
        assert_eq!(out.headers[1].value, "***");
        assert_eq!(out.redacted_header_names, vec!["X-Secret", "Authorization"]);
    }

    #[test]
    fn purity_original_bytes_untouched() {
        let msg = msg_with(vec![HeaderRecord::new("Cookie", "sid=1")]);
        let before = msg.clone();
        let _ = redact(&msg, &RedactRules::default());
        assert_eq!(msg, before, "不变式 3：Display 生成不改变 Parsed 存储");
    }

    #[test]
    fn body_preview_cap_and_lossy() {
        let mut msg = msg_with(vec![]);
        msg.body = b"abc".to_vec();
        let out = redact(&msg, &RedactRules::default());
        assert_eq!(out.body_preview, "abc");
        assert!(!out.body_preview_lossy);
        assert!(!out.body_preview_truncated);

        msg.body = vec![0x61, 0x62, 0x63, 0xff];
        let out = redact(&msg, &RedactRules::default());
        assert_eq!(out.body_preview, "abc\u{fffd}");
        assert!(out.body_preview_lossy);

        msg.body = b"0123456789".repeat(30); // 300 > 200
        let out = redact(&msg, &RedactRules::default());
        assert!(out.body_preview_truncated);
        assert_eq!(out.body_preview.len(), 200);
    }
}
