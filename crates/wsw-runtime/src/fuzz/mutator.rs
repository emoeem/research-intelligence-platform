//! `wsw_runtime::fuzz::mutator`：Payload 源展开链 + 变换链 + Processor 面
//! （WSW-4 D4；PLAN :235-244 语义面）。
//!
//! - 变换 = 编码链（URL/Base64/Base64URL/Hex/HTML Entity——PLAN :287 词表
//!   对齐）+ 前缀/后缀/大小写/替换/拼接/正则替换（:240-241）；
//! - Processor = payload 注入后的请求侧后处理（header_set/header_remove/
//!   body_replace；头值合法性复走 serialize 入出守卫——发送面守卫既有词形，
//!   本面不重复实现重定界，D4「不发明第二重定界」）；
//! - 正则面 = `regex` crate 传递依赖转正直接声明（D9；lock 实值 =1.13.1）。
//!
//! Mutator 语义 = 列表变换器：`Vec<String> → Vec<String>`（Concat 扇出；
//! 其余 1:1）。base64 编码器 std 内嵌实现（base64 crate 不进 wsw-runtime
//! 依赖面——D9 只允许 regex 一行转正）。

use super::spec::SpecError;
use serde::{Deserialize, Serialize};
use wsw_message::serialize::OutboundRequest;

/// 编码词表（PLAN :287 编码链）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EncodeKind {
    /// 百分号编码（unreserved 之外全量转义）。
    Url,
    /// Base64 标准（带 padding）。
    Base64,
    /// Base64URL（URL-safe 字母表，无 padding）。
    Base64Url,
    /// 小写十六进制。
    Hex,
    /// HTML 实体（&lt;&gt;&amp;&quot;&#39; 五实体）。
    HtmlEntity,
}

/// 大小写变换目标。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaseKind {
    Upper,
    Lower,
}

/// 拼接方位。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConcatPos {
    Prefix,
    Suffix,
}

/// Mutator 有序链项（声明式 `type` + params，D4）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Mutator {
    /// 编码链（顺序应用）。
    Encode { chain: Vec<EncodeKind> },
    /// 前缀。
    Prefix { value: String },
    /// 后缀。
    Suffix { value: String },
    /// 大小写。
    Case { to: CaseKind },
    /// 字面替换。
    Replace { from: String, to: String },
    /// 拼接扇出：payload × values 列表（方位：prefix/suffix）。
    Concat { values: Vec<String>, position: ConcatPos },
    /// 正则替换（regex crate；编译失败 = spec 拒）。
    RegexReplace { pattern: String, replacement: String },
}

/// Processor 请求侧后处理链项（D4）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Processor {
    /// 设/换头（同名覆盖，大小写不敏感）。
    HeaderSet { name: String, value: String },
    /// 移除头（全部同名）。
    HeaderRemove { name: String },
    /// 体字面替换（字节面）。
    BodyReplace { from: String, to: String },
}

/// Base64 标准字母表。
const B64_STD: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
/// Base64URL 字母表。
const B64_URL: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

/// std 内嵌 base64 编码（pad = 标准 4:3 分组补 `=`；URL-safe 无 pad）。
fn b64_encode(data: &[u8], alphabet: &[u8; 64], pad: bool) -> String {
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b = [
            chunk[0],
            chunk.get(1).copied().unwrap_or(0),
            chunk.get(2).copied().unwrap_or(0),
        ];
        let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | b[2] as u32;
        let quad = [
            alphabet[(n >> 18) as usize & 63],
            alphabet[(n >> 12) as usize & 63],
            alphabet[(n >> 6) as usize & 63],
            alphabet[n as usize & 63],
        ];
        let keep = match chunk.len() {
            3 => 4,
            2 => 3,
            _ => 2,
        };
        out.push_str(std::str::from_utf8(&quad[..keep]).unwrap_or(""));
        if pad {
            for _ in keep..4 {
                out.push('=');
            }
        }
    }
    out
}

/// 百分号编码（unreserved = ALPHA/DIGIT/-/./_/~ 之外全转义）。
pub fn url_encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for b in text.as_bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(*b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// HTML 实体转义（五实体）。
pub fn html_entity_encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

/// 单 payload 编码链应用。
fn apply_encode(payload: &str, chain: &[EncodeKind]) -> String {
    let mut cur = payload.to_string();
    for kind in chain {
        cur = match kind {
            EncodeKind::Url => url_encode(&cur),
            EncodeKind::Base64 => b64_encode(cur.as_bytes(), B64_STD, true),
            EncodeKind::Base64Url => b64_encode(cur.as_bytes(), B64_URL, false),
            EncodeKind::Hex => cur
                .as_bytes()
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<Vec<_>>()
                .join(""),
            EncodeKind::HtmlEntity => html_entity_encode(&cur),
        };
    }
    cur
}

/// 单 Mutator 项应用（列表 → 列表；Concat 扇出）。
fn apply_one(items: &[String], m: &Mutator) -> Result<Vec<String>, SpecError> {
    let out = match m {
        Mutator::Encode { chain } => items
            .iter()
            .map(|p| apply_encode(p, chain))
            .collect::<Vec<_>>(),
        Mutator::Prefix { value } => items
            .iter()
            .map(|p| format!("{value}{p}"))
            .collect::<Vec<_>>(),
        Mutator::Suffix { value } => items
            .iter()
            .map(|p| format!("{p}{value}"))
            .collect::<Vec<_>>(),
        Mutator::Case { to } => items
            .iter()
            .map(|p| match to {
                CaseKind::Upper => p.to_uppercase(),
                CaseKind::Lower => p.to_lowercase(),
            })
            .collect::<Vec<_>>(),
        Mutator::Replace { from, to } => items
            .iter()
            .map(|p| p.replace(from.as_str(), to))
            .collect::<Vec<_>>(),
        Mutator::Concat { values, position } => {
            let mut out = Vec::with_capacity(items.len() * values.len().max(1));
            for p in items {
                for v in values {
                    match position {
                        ConcatPos::Prefix => out.push(format!("{v}{p}")),
                        ConcatPos::Suffix => out.push(format!("{p}{v}")),
                    }
                }
            }
            out
        }
        Mutator::RegexReplace { pattern, replacement } => {
            let re = regex::Regex::new(pattern)
                .map_err(|e| SpecError::MutatorInvalid(format!("regex {pattern:?}: {e}")))?;
            items
                .iter()
                .map(|p| re.replace_all(p, replacement.as_str()).into_owned())
                .collect::<Vec<_>>()
        }
    };
    if out.len() > super::spec::MAX_PAYLOAD_ITEMS {
        return Err(SpecError::PayloadSource(format!(
            "mutator fan-out exceeds {} items",
            super::spec::MAX_PAYLOAD_ITEMS
        )));
    }
    Ok(out)
}

/// 有序链应用（base 列表 → 逐项链变换；判据 4 面入口）。
pub fn apply_mutators(base: &[String], chain: &[Mutator]) -> Result<Vec<String>, SpecError> {
    let mut cur = base.to_vec();
    for m in chain {
        cur = apply_one(&cur, m)?;
    }
    Ok(cur)
}

/// Processor 链应用（请求侧后处理；HeaderSet 值含 CR/LF 即拒——头注入
/// 守卫 fail-closed 前置，发送面 serialize 守卫复用不替代）。
pub fn apply_processors(
    req: &mut OutboundRequest,
    chain: &[Processor],
) -> Result<(), SpecError> {
    for p in chain {
        match p {
            Processor::HeaderSet { name, value } => {
                if value.contains('\r') || value.contains('\n') {
                    return Err(SpecError::CaseBuild(format!(
                        "header_set {}: CR/LF in value rejected",
                        name
                    )));
                }
                if let Some(h) = req
                    .headers
                    .iter_mut()
                    .find(|h| h.name.eq_ignore_ascii_case(name))
                {
                    h.value = value.clone();
                } else {
                    req.headers.push(wsw_message::types::HeaderRecord::new(
                        name, value,
                    ));
                }
            }
            Processor::HeaderRemove { name } => {
                req.headers
                    .retain(|h| !h.name.eq_ignore_ascii_case(name));
            }
            Processor::BodyReplace { from, to } => {
                let text = req.body.clone();
                req.body = replace_bytes(&text, from.as_bytes(), to.as_bytes());
            }
        }
    }
    Ok(())
}

/// 字节面字面替换（std 搜索；空 needle 原样）。
fn replace_bytes(hay: &[u8], from: &[u8], to: &[u8]) -> Vec<u8> {
    if from.is_empty() {
        return hay.to_vec();
    }
    let mut out = Vec::with_capacity(hay.len());
    let mut pos = 0usize;
    while pos < hay.len() {
        if let Some(found) = hay[pos..]
            .windows(from.len())
            .position(|w| w == from)
            .map(|p| p + pos)
        {
            out.extend_from_slice(&hay[pos..found]);
            out.extend_from_slice(to);
            pos = found + from.len();
        } else {
            out.extend_from_slice(&hay[pos..]);
            break;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn b64_std_and_url_match_known_vectors() {
        assert_eq!(b64_encode(b"foobar", B64_STD, true), "Zm9vYmFy");
        assert_eq!(b64_encode(b"foob", B64_STD, true), "Zm9vYg==");
        assert_eq!(b64_encode(b"foob", B64_STD, false), "Zm9vYg");
        assert_eq!(b64_encode(b"foob", B64_URL, false), "Zm9vYg");
        assert_eq!(b64_encode(b"", B64_STD, true), "");
    }

    #[test]
    fn encode_chain_url_hex_entity() {
        assert_eq!(url_encode("a b&c"), "a%20b%26c");
        assert_eq!(html_entity_encode("<a>&\"'"), "&lt;a&gt;&amp;&quot;&#39;");
        let out = apply_one(
            &["abc".to_string()],
            &Mutator::Encode { chain: vec![EncodeKind::Hex] },
        )
        .unwrap();
        assert_eq!(out, vec!["616263"]);
        // 编码链顺序：url 后 base64。
        let out = apply_one(
            &["a b".to_string()],
            &Mutator::Encode { chain: vec![EncodeKind::Url, EncodeKind::Base64] },
        )
        .unwrap();
        assert_eq!(out, vec![b64_encode(b"a%20b", B64_STD, true)]);
    }

    #[test]
    fn mutator_chain_prefix_case_replace_regex() {
        let base = vec!["payload".to_string()];
        let chain = vec![
            Mutator::Prefix { value: "/".into() },
            Mutator::Case { to: CaseKind::Upper },
            Mutator::Replace { from: "LOAD".into(), to: "X".into() },
            Mutator::Suffix { value: ".".into() },
            Mutator::RegexReplace { pattern: r"\.$".into(), replacement: "!".into() },
        ];
        let out = apply_mutators(&base, &chain).unwrap();
        assert_eq!(out, vec!["/PAYX!".to_string()]);
    }

    #[test]
    fn concat_fans_out() {
        let out = apply_one(
            &["p1".to_string(), "p2".to_string()],
            &Mutator::Concat { values: vec!["a".into(), "b".into()], position: ConcatPos::Suffix },
        )
        .unwrap();
        assert_eq!(out, vec!["p1a", "p1b", "p2a", "p2b"]);
    }

    #[test]
    fn regex_invalid_rejected() {
        let out = apply_mutators(
            &["x".to_string()],
            &[Mutator::RegexReplace { pattern: "([".into(), replacement: "".into() }],
        );
        assert!(matches!(out, Err(SpecError::MutatorInvalid(_))));
    }

    #[test]
    fn processors_set_remove_body_replace() {
        let mut req = OutboundRequest {
            method: "POST".into(),
            target: "/x".into(),
            version: wsw_message::types::HttpVersion::Http11,
            headers: vec![wsw_message::types::HeaderRecord::new("Content-Type", "text/plain")],
            body: b"hello world".to_vec(),
            trailers: vec![],
            framing: wsw_message::serialize::OutboundFraming::ContentLength,
        };
        apply_processors(
            &mut req,
            &[
                Processor::HeaderSet { name: "X-Fuzz".into(), value: "on".into() },
                Processor::HeaderSet { name: "content-type".into(), value: "application/json".into() },
                Processor::BodyReplace { from: "world".into(), to: "fuzz".into() },
            ],
        )
        .unwrap();
        assert_eq!(req.body, b"hello fuzz");
        assert!(req.headers.iter().any(|h| h.name == "X-Fuzz" && h.value == "on"));
        assert!(req
            .headers
            .iter()
            .any(|h| h.name == "Content-Type" && h.value == "application/json"));
        apply_processors(&mut req, &[Processor::HeaderRemove { name: "x-fuzz".into() }])
            .unwrap();
        assert!(!req.headers.iter().any(|h| h.name == "X-Fuzz"));
    }

    #[test]
    fn processor_crlf_header_value_rejected() {
        let mut req = OutboundRequest {
            method: "GET".into(),
            target: "/".into(),
            version: wsw_message::types::HttpVersion::Http11,
            headers: vec![],
            body: vec![],
            trailers: vec![],
            framing: wsw_message::serialize::OutboundFraming::ContentLength,
        };
        let err = apply_processors(
            &mut req,
            &[Processor::HeaderSet { name: "X".into(), value: "a\r\nX: y".into() }],
        )
        .unwrap_err();
        assert!(matches!(err, SpecError::CaseBuild(_)));
    }
}
