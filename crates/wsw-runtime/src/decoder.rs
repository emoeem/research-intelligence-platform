//! `wsw_runtime::decoder`：Decoder 变换链（WSW-6；PLAN §4.6 :285-294 语义面）。
//!
//! 词表落地 12/17 项（D5 裁决）：五变换双向（Url/Base64/Base64Url/Hex/
//! HtmlEntity——编码方向直用 [`crate::fuzz::mutator`] 既有 pub 纯函数面
//! `apply_mutators`/`url_encode`/`html_entity_encode`——Decoder 是消费面
//! 非第二实现；解码方向补逆变换 std 纯函数）+ Unicode escape（std
//! `char::from_u32`）+ JWT 结构解析（三段 header/payload/signature 展示 +
//! **不验签不猜测密钥**——PLAN :292 原文）+ JSON 三态（serde_json pretty/
//! compact/validate）+ sha2（`wsw_message::cas::sha256_hex` 唯一挂点）。
//!
//! **裁剪登记（D5）**：Gzip/Deflate/Brotli 解压不入本批——flate2/brotli/
//! zstd 在 Cargo.lock 与 vendor/ 双零命中，vendor 红线（REDLINES 1「新增
//! 依赖必须 cargo vendor 全量入库 + deny 门」）下解压库引入成本不成比例，
//! 留后续批 vendor 立项裁决。
//!
//! 多级编码链 = 逐级变换纯函数链（mutator.rs apply 形状复用）；全部变换
//! 零 IO 零网络（REDLINES 边界纪律）。

use crate::fuzz::mutator::{apply_mutators, html_entity_encode, url_encode, EncodeKind, Mutator};
use serde::{Deserialize, Serialize};

/// 输入长度帽（8MiB = 025 request_json 帽同型；API/CLI 双面共用）。
pub const DECODER_INPUT_CAP: usize = 8 * 1024 * 1024;

/// Decoder 变换词表（17 op = 12 词表项落地投影；词形 = snake_case CLI/API
/// 面）。Gzip/Deflate/Brotli 三面不在表内（D5 裁剪登记）。
pub const DECODER_OPS: [&str; 17] = [
    "url_encode",
    "url_decode",
    "b64_encode",
    "b64_decode",
    "b64url_encode",
    "b64url_decode",
    "hex_encode",
    "hex_decode",
    "html_entity_encode",
    "html_entity_decode",
    "unicode_escape",
    "unicode_unescape",
    "json_pretty",
    "json_compact",
    "json_validate",
    "jwt_decode",
    "sha256",
];

/// 变换项（有序链；serde snake_case 词形 = [`DECODER_OPS`] 逐字）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DecodeOp {
    UrlEncode,
    UrlDecode,
    B64Encode,
    B64Decode,
    B64UrlEncode,
    B64UrlDecode,
    HexEncode,
    HexDecode,
    HtmlEntityEncode,
    HtmlEntityDecode,
    UnicodeEscape,
    UnicodeUnescape,
    JsonPretty,
    JsonCompact,
    JsonValidate,
    JwtDecode,
    Sha256,
}

impl DecodeOp {
    /// 词表串（CLI/API 词形）。
    pub fn as_str(self) -> &'static str {
        // serde rename_all = snake_case 词形与 DECODER_OPS 同源；此处显式
        // 词表串（Copy enum 手写映射，两表一致性由 tests 往返矩阵钉住）。
        match self {
            DecodeOp::UrlEncode => "url_encode",
            DecodeOp::UrlDecode => "url_decode",
            DecodeOp::B64Encode => "b64_encode",
            DecodeOp::B64Decode => "b64_decode",
            DecodeOp::B64UrlEncode => "b64url_encode",
            DecodeOp::B64UrlDecode => "b64url_decode",
            DecodeOp::HexEncode => "hex_encode",
            DecodeOp::HexDecode => "hex_decode",
            DecodeOp::HtmlEntityEncode => "html_entity_encode",
            DecodeOp::HtmlEntityDecode => "html_entity_decode",
            DecodeOp::UnicodeEscape => "unicode_escape",
            DecodeOp::UnicodeUnescape => "unicode_unescape",
            DecodeOp::JsonPretty => "json_pretty",
            DecodeOp::JsonCompact => "json_compact",
            DecodeOp::JsonValidate => "json_validate",
            DecodeOp::JwtDecode => "jwt_decode",
            DecodeOp::Sha256 => "sha256",
        }
    }

    /// 词表串解析（非法即 None——不静默映射）。
    pub fn from_word(word: &str) -> Option<Self> {
        DECODER_OPS
            .iter()
            .position(|w| *w == word)
            .map(|i| DECODER_OP_ENUMS[i])
    }

    /// 本批五变换双向配对（往返恒等矩阵用；其余 op 非双向）。
    pub fn inverse(self) -> Option<Self> {
        match self {
            DecodeOp::UrlEncode => Some(DecodeOp::UrlDecode),
            DecodeOp::UrlDecode => Some(DecodeOp::UrlEncode),
            DecodeOp::B64Encode => Some(DecodeOp::B64Decode),
            DecodeOp::B64Decode => Some(DecodeOp::B64Encode),
            DecodeOp::B64UrlEncode => Some(DecodeOp::B64UrlDecode),
            DecodeOp::B64UrlDecode => Some(DecodeOp::B64UrlEncode),
            DecodeOp::HexEncode => Some(DecodeOp::HexDecode),
            DecodeOp::HexDecode => Some(DecodeOp::HexEncode),
            DecodeOp::HtmlEntityEncode => Some(DecodeOp::HtmlEntityDecode),
            DecodeOp::HtmlEntityDecode => Some(DecodeOp::HtmlEntityEncode),
            _ => None,
        }
    }
}

/// 与 [`DECODER_OPS`] 逐位同序的 enum 表（from_word 位投影）。
const DECODER_OP_ENUMS: [DecodeOp; 17] = [
    DecodeOp::UrlEncode,
    DecodeOp::UrlDecode,
    DecodeOp::B64Encode,
    DecodeOp::B64Decode,
    DecodeOp::B64UrlEncode,
    DecodeOp::B64UrlDecode,
    DecodeOp::HexEncode,
    DecodeOp::HexDecode,
    DecodeOp::HtmlEntityEncode,
    DecodeOp::HtmlEntityDecode,
    DecodeOp::UnicodeEscape,
    DecodeOp::UnicodeUnescape,
    DecodeOp::JsonPretty,
    DecodeOp::JsonCompact,
    DecodeOp::JsonValidate,
    DecodeOp::JwtDecode,
    DecodeOp::Sha256,
];

/// 结构化拒绝词表（判据 4 负例硬门：非法输入 → 结构化拒绝，不静默透传）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecodeError {
    /// 非法 hex/percent 转义（奇长或非 hex 字符；pos = 字节位）。
    InvalidHex { pos: usize },
    /// 非法 base64 字符/长度（pos = 字节位）。
    InvalidBase64 { pos: usize },
    /// 超界/孤立代理区 Unicode escape（char::from_u32 None）。
    InvalidUnicodeEscape(String),
    /// 非法 JSON。
    InvalidJson(String),
    /// 非法 JWT 结构（段数 ≠ 3 / 段不可解 / 段非 JSON）。
    InvalidJwt(String),
    /// 词表外 op。
    UnknownOp(String),
    /// 输入超帽（DECODER_INPUT_CAP）。
    InputTooLarge(usize),
}

impl std::fmt::Display for DecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DecodeError::InvalidHex { pos } => {
                write!(f, "invalid hex/percent escape at byte {pos}")
            }
            DecodeError::InvalidBase64 { pos } => write!(f, "invalid base64 at byte {pos}"),
            DecodeError::InvalidUnicodeEscape(d) => write!(f, "invalid unicode escape: {d}"),
            DecodeError::InvalidJson(d) => write!(f, "invalid json: {d}"),
            DecodeError::InvalidJwt(d) => write!(f, "invalid jwt: {d}"),
            DecodeError::UnknownOp(w) => write!(f, "unknown op: {w:?}"),
            DecodeError::InputTooLarge(n) => {
                write!(f, "input exceeds {DECODER_INPUT_CAP} byte cap: {n}")
            }
        }
    }
}

impl std::error::Error for DecodeError {}

/// Base64 标准字母表（与 fuzz/mutator.rs B64_STD 同源——字母表面唯一词形）。
const B64_STD: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
/// Base64URL 字母表（与 fuzz/mutator.rs B64_URL 同源）。
const B64_URL: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

/// std base64 解码（alphabet 参数化；接受 0/1/2 位 padding，非法字符/奇位
/// → 结构化拒绝。pad_allowed=false（URL-safe 编码器零 pad）时仍容忍尾 pad）。
fn b64_decode_alphabet(input: &str, alphabet: &[u8; 64]) -> Result<Vec<u8>, DecodeError> {
    let b = input.as_bytes();
    // 剥尾 padding（≤2 个 '='，且只在尾部）。
    let mut end = b.len();
    let mut pad = 0usize;
    while end > 0 && b[end - 1] == b'=' {
        end -= 1;
        pad += 1;
        if pad > 2 {
            return Err(DecodeError::InvalidBase64 { pos: end });
        }
    }
    let body = &b[..end];
    if body.len() % 4 == 1 {
        return Err(DecodeError::InvalidBase64 { pos: end });
    }
    let mut out = Vec::with_capacity(body.len() * 3 / 4 + 3);
    let mut acc: u32 = 0;
    let mut bits = 0u32;
    for (i, &c) in body.iter().enumerate() {
        let v = alphabet
            .iter()
            .position(|&a| a == c)
            .ok_or(DecodeError::InvalidBase64 { pos: i })? as u32;
        acc = (acc << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    Ok(out)
}

/// 百分号解码（`%XX`；非法/奇缺 → InvalidHex{pos}——percent escape 语义 =
/// hex 对）。
pub fn url_decode(text: &str) -> Result<String, DecodeError> {
    let b = text.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0usize;
    while i < b.len() {
        if b[i] == b'%' {
            if i + 2 >= b.len() || !b[i + 1].is_ascii_hexdigit() || !b[i + 2].is_ascii_hexdigit() {
                return Err(DecodeError::InvalidHex { pos: i });
            }
            let hi = (b[i + 1] as char).to_digit(16).unwrap_or(0) as u16;
            let lo = (b[i + 2] as char).to_digit(16).unwrap_or(0) as u16;
            out.push((hi * 16 + lo) as u8);
            i += 3;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8(out).map_err(|_| DecodeError::InvalidHex { pos: 0 })
}

/// hex 解码（奇长/非 hex 字符 → InvalidHex{pos}）。
pub fn hex_decode(text: &str) -> Result<Vec<u8>, DecodeError> {
    let b = text.as_bytes();
    if b.len() % 2 == 1 {
        return Err(DecodeError::InvalidHex { pos: b.len() });
    }
    let mut out = Vec::with_capacity(b.len() / 2);
    for pair in b.chunks(2) {
        let hi = (pair[0] as char)
            .to_digit(16)
            .ok_or(DecodeError::InvalidHex { pos: out.len() * 2 })?;
        let lo = (pair[1] as char)
            .to_digit(16)
            .ok_or(DecodeError::InvalidHex {
                pos: out.len() * 2 + 1,
            })?;
        out.push((hi * 16 + lo) as u8);
    }
    Ok(out)
}

/// HTML 实体解码（五实体 + 数字 `&#NN;` / `&#xHH;`；未知实体原样保留——
/// 解码器宽容面；数字超界/孤立代理区 → InvalidUnicodeEscape）。
pub fn html_entity_decode(text: &str) -> Result<String, DecodeError> {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(pos) = rest.find('&') {
        out.push_str(&rest[..pos]);
        let tail = &rest[pos..];
        let semi = tail.find(';').unwrap_or(tail.len());
        // entity 含分号（&amp; 五字符——[..semi] 会丢 ';' 使词表永不命中）。
        let entity = &tail[..(semi + 1).min(tail.len())];
        let decoded: Option<String> = match entity {
            "&amp;" => Some("&".into()),
            "&lt;" => Some("<".into()),
            "&gt;" => Some(">".into()),
            "&quot;" => Some("\"".into()),
            "&#39;" => Some("'".into()),
            _ => None,
        };
        if let Some(d) = decoded {
            out.push_str(&d);
            rest = &tail[semi + 1..];
            continue;
        }
        // 数字实体（十进制 &#NN; / 十六进制 &#xHH;）。
        let numeric = if let Some(hex) = entity.strip_prefix("&#x").or(entity.strip_prefix("&#X")) {
            u32::from_str_radix(hex, 16).ok()
        } else if let Some(dec) = entity.strip_prefix("&#") {
            dec.parse::<u32>().ok()
        } else {
            None
        };
        match numeric.and_then(char::from_u32) {
            Some(c) => {
                out.push(c);
                rest = &tail[semi + 1..];
            }
            None => {
                // 无 ';' 或非法数字/超界：'&' 原样保留，从下一位继续扫描
                // （宽容面）；孤立代理区数字 → 结构化拒绝。
                if entity.starts_with("&#") && semi < tail.len() {
                    return Err(DecodeError::InvalidUnicodeEscape(format!(
                        "entity {entity}; out of range or surrogate"
                    )));
                }
                out.push('&');
                rest = &tail[1..];
            }
        }
    }
    out.push_str(rest);
    Ok(out)
}

/// Unicode escape（非 ASCII → `\uXXXX`，增补平面 → 代理对双 `\uXXXX`；
/// ASCII 可打印区原样）。
pub fn unicode_escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        if (c as u32) < 0x80 {
            out.push(c);
        } else {
            let cp = c as u32;
            if cp > 0xFFFF {
                let v = cp - 0x10000;
                let hi = 0xD800 + (v >> 10);
                let lo = 0xDC00 + (v & 0x3FF);
                out.push_str(&format!("\\u{hi:04X}\\u{lo:04X}"));
            } else {
                out.push_str(&format!("\\u{cp:04X}"));
            }
        }
    }
    out
}

/// Unicode unescape（`\uXXXX` + 代理对重组；孤立代理/超界 → 结构化拒绝）。
pub fn unicode_unescape(text: &str) -> Result<String, DecodeError> {
    let b = text.as_bytes();
    let mut out = String::with_capacity(text.len());
    let mut i = 0usize;
    while i < b.len() {
        if b[i] == b'\\' && i + 5 < b.len() && b[i + 1] == b'u' {
            let hex = std::str::from_utf8(&b[i + 2..i + 6])
                .map_err(|_| DecodeError::InvalidUnicodeEscape("bad utf8 at escape".into()))?;
            let v = u32::from_str_radix(hex, 16)
                .map_err(|_| DecodeError::InvalidUnicodeEscape(format!("bad hex \\u{hex}")))?;
            // 代理区分支先于 from_u32（代理值不是合法 char——从_u32 会先拒）。
            if (0xD800..0xDC00).contains(&v) {
                // 高代理：须紧跟低代理重组（孤立 → 拒）。
                if i + 11 < b.len() && b[i + 6] == b'\\' && b[i + 7] == b'u' {
                    let hex2 = std::str::from_utf8(&b[i + 8..i + 12]).map_err(|_| {
                        DecodeError::InvalidUnicodeEscape("bad utf8 at low surrogate".into())
                    })?;
                    let v2 = u32::from_str_radix(hex2, 16).map_err(|_| {
                        DecodeError::InvalidUnicodeEscape(format!("bad hex \\u{hex2}"))
                    })?;
                    if !(0xDC00..0xE000).contains(&v2) {
                        return Err(DecodeError::InvalidUnicodeEscape(format!(
                            "lone high surrogate \\u{hex}"
                        )));
                    }
                    let combined = 0x10000 + ((v - 0xD800) << 10) + (v2 - 0xDC00);
                    let c = char::from_u32(combined).ok_or_else(|| {
                        DecodeError::InvalidUnicodeEscape("combined out of range".into())
                    })?;
                    out.push(c);
                    i += 12;
                    continue;
                }
                return Err(DecodeError::InvalidUnicodeEscape(format!(
                    "lone high surrogate \\u{hex}"
                )));
            }
            if (0xDC00..0xE000).contains(&v) {
                return Err(DecodeError::InvalidUnicodeEscape(format!(
                    "lone low surrogate \\u{hex}"
                )));
            }
            let c = char::from_u32(v).ok_or_else(|| {
                DecodeError::InvalidUnicodeEscape(format!("\\u{hex} out of range"))
            })?;
            out.push(c);
            i += 6;
        } else {
            // 多字节 UTF-8 序列按字符步进（String 切面安全：按字节推进但
            // 整段 char 边界由 from_utf8 校验承担——此处按 ASCII/UTF8 尾节
            // 复制）。
            let ch_len = utf8_len(b[i]);
            let end = (i + ch_len).min(b.len());
            match std::str::from_utf8(&b[i..end]) {
                Ok(s) => out.push_str(s),
                Err(_) => {
                    return Err(DecodeError::InvalidUnicodeEscape(
                        "input is not valid utf8".into(),
                    ))
                }
            }
            i = end;
        }
    }
    Ok(out)
}

/// UTF-8 序列长度（首字节 → 1..4）。
fn utf8_len(first: u8) -> usize {
    match first {
        0x00..=0x7F => 1,
        0xC0..=0xDF => 2,
        0xE0..=0xEF => 3,
        _ => 4,
    }
}

/// JWT 结构解析（三段 header/payload/signature 展示；**不验签不猜测密钥**
/// ——PLAN :292 原文；输出 JSON 文本 = 展示投影）。段数 ≠ 3 / 段 base64url
/// 不可解 / header·payload 非 JSON → InvalidJwt。
pub fn jwt_decode(token: &str) -> Result<String, DecodeError> {
    let token = token.trim();
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() != 3 {
        return Err(DecodeError::InvalidJwt(format!(
            "expected 3 segments, got {}",
            parts.len()
        )));
    }
    let decode_seg = |seg: &str| -> Result<serde_json::Value, DecodeError> {
        let bytes = b64_decode_alphabet(seg, B64_URL)
            .map_err(|_| DecodeError::InvalidJwt("segment is not base64url decodable".into()))?;
        let text = String::from_utf8(bytes)
            .map_err(|_| DecodeError::InvalidJwt("segment is not utf8".into()))?;
        serde_json::from_str(&text)
            .map_err(|e| DecodeError::InvalidJwt(format!("segment is not json: {e}")))
    };
    let header = decode_seg(parts[0])?;
    let payload = decode_seg(parts[1])?;
    let out = serde_json::json!({
        "header": header,
        "payload": payload,
        "signature_b64url": parts[2],
        "verified": false,
        "note": "结构解析不验签不猜测密钥（PLAN :292）",
    });
    Ok(serde_json::to_string_pretty(&out).unwrap_or_default())
}

/// 有序变换链应用（多级编码链 = 逐级变换纯函数链，mutator.rs apply 形状
/// 复用）。超帽/非法输入 → 结构化拒绝。
pub fn apply_ops(input: &str, ops: &[DecodeOp]) -> Result<String, DecodeError> {
    if input.len() > DECODER_INPUT_CAP {
        return Err(DecodeError::InputTooLarge(input.len()));
    }
    let mut cur = input.to_string();
    for op in ops {
        cur = apply_one(&cur, *op)?;
    }
    Ok(cur)
}

/// 单变换应用（词表 17 项逐项）。
fn apply_one(input: &str, op: DecodeOp) -> Result<String, DecodeError> {
    match op {
        DecodeOp::UrlEncode => Ok(url_encode(input)),
        DecodeOp::UrlDecode => url_decode(input),
        // 编码方向 = mutator 既有 pub 纯函数面（apply_mutators 单项单链；
        // Decoder 是消费面非第二实现——D5）。
        DecodeOp::B64Encode => encode_via_mutator(input, EncodeKind::Base64),
        DecodeOp::B64UrlEncode => encode_via_mutator(input, EncodeKind::Base64Url),
        DecodeOp::HexEncode => encode_via_mutator(input, EncodeKind::Hex),
        DecodeOp::HtmlEntityEncode => Ok(html_entity_encode(input)),
        DecodeOp::B64Decode => {
            let bytes = b64_decode_alphabet(input, B64_STD)?;
            String::from_utf8(bytes).map_err(|_| DecodeError::InvalidBase64 { pos: 0 })
        }
        DecodeOp::B64UrlDecode => {
            let bytes = b64_decode_alphabet(input, B64_URL)?;
            String::from_utf8(bytes).map_err(|_| DecodeError::InvalidBase64 { pos: 0 })
        }
        DecodeOp::HexDecode => {
            let bytes = hex_decode(input)?;
            String::from_utf8(bytes).map_err(|_| DecodeError::InvalidHex { pos: 0 })
        }
        DecodeOp::HtmlEntityDecode => html_entity_decode(input),
        DecodeOp::UnicodeEscape => Ok(unicode_escape(input)),
        DecodeOp::UnicodeUnescape => unicode_unescape(input),
        DecodeOp::JsonPretty => {
            let v: serde_json::Value =
                serde_json::from_str(input).map_err(|e| DecodeError::InvalidJson(e.to_string()))?;
            serde_json::to_string_pretty(&v).map_err(|e| DecodeError::InvalidJson(e.to_string()))
        }
        DecodeOp::JsonCompact => {
            let v: serde_json::Value =
                serde_json::from_str(input).map_err(|e| DecodeError::InvalidJson(e.to_string()))?;
            serde_json::to_string(&v).map_err(|e| DecodeError::InvalidJson(e.to_string()))
        }
        // 校验面：通过 = 输入原样返回（失败 = InvalidJson 结构化拒绝）。
        DecodeOp::JsonValidate => {
            serde_json::from_str::<serde_json::Value>(input)
                .map_err(|e| DecodeError::InvalidJson(e.to_string()))?;
            Ok(input.to_string())
        }
        DecodeOp::JwtDecode => jwt_decode(input),
        DecodeOp::Sha256 => Ok(wsw_message::cas::sha256_hex(input.as_bytes())),
    }
}

/// 编码方向 mutator 通道（apply_mutators pub 面；单项单链扇出 = 1:1）。
fn encode_via_mutator(input: &str, kind: EncodeKind) -> Result<String, DecodeError> {
    let out = apply_mutators(
        &[input.to_string()],
        &[Mutator::Encode { chain: vec![kind] }],
    )
    .map_err(|e| DecodeError::UnknownOp(e.to_string()))?;
    out.into_iter()
        .next()
        .ok_or(DecodeError::UnknownOp("mutator produced no output".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn word_table_and_enum_table_align() {
        assert_eq!(DECODER_OPS.len(), DECODER_OP_ENUMS.len());
        for (word, op) in DECODER_OPS.iter().zip(DECODER_OP_ENUMS.iter()) {
            assert_eq!(DecodeOp::from_word(word), Some(*op));
            assert_eq!(op.as_str(), *word);
        }
        assert_eq!(DecodeOp::from_word("gzip_decode"), None, "裁剪面不在词表");
    }

    #[test]
    fn five_transforms_roundtrip_identity_both_directions() {
        // 判据 4：encode(decode(x))==x 与 decode(encode(x))==x 逐变换双断言。
        let samples = [
            "plain text",
            "a b&c=d/e?f",
            "héllo 世界 🚀",
            "<script>alert(\"x\")</script>&'x'",
            "",
        ];
        let pairs = [
            (DecodeOp::UrlEncode, DecodeOp::UrlDecode),
            (DecodeOp::B64Encode, DecodeOp::B64Decode),
            (DecodeOp::B64UrlEncode, DecodeOp::B64UrlDecode),
            (DecodeOp::HexEncode, DecodeOp::HexDecode),
            (DecodeOp::HtmlEntityEncode, DecodeOp::HtmlEntityDecode),
        ];
        for s in samples {
            for (enc, dec) in pairs {
                let encoded = apply_one(s, enc).unwrap();
                let back = apply_one(&encoded, dec).unwrap();
                assert_eq!(
                    back, s,
                    "decode(encode(x))==x failed for {enc:?}: {s:?} -> {encoded:?}"
                );
            }
        }
        // 正向恒等 encode(decode(x))==x：x 须取编码像（编码器满射域内的
        // 稳定点——如 "a%20b" 解码回 "a b" 再编码还原原串）。
        let encoded_forms = [
            "a%20b%26c%3Dd",
            "Zm9vYmFy",
            "Zm9vYg",
            "616263",
            "&lt;a&gt;&amp;&quot;&#39;",
        ];
        let dec_first = [
            DecodeOp::UrlDecode,
            DecodeOp::B64Decode,
            DecodeOp::B64UrlDecode,
            DecodeOp::HexDecode,
            DecodeOp::HtmlEntityDecode,
        ];
        let enc_back = [
            DecodeOp::UrlEncode,
            DecodeOp::B64Encode,
            DecodeOp::B64UrlEncode,
            DecodeOp::HexEncode,
            DecodeOp::HtmlEntityEncode,
        ];
        for ((x, dec), enc) in encoded_forms.iter().zip(dec_first).zip(enc_back) {
            let decoded = apply_one(x, dec).unwrap();
            let re = apply_one(&decoded, enc).unwrap();
            assert_eq!(
                re, *x,
                "encode(decode(x))==x failed for {dec:?}: {x:?} -> {decoded:?}"
            );
        }
    }

    #[test]
    fn b64_std_matches_mutator_surface() {
        // 编码方向 mutator 通道与 std 解码互逆（跨面一致性钉）。
        let samples = ["foobar", "foob", "", "héllo 世界"];
        for s in samples {
            let via_mutator = encode_via_mutator(s, EncodeKind::Base64).unwrap();
            let back = apply_one(&via_mutator, DecodeOp::B64Decode).unwrap();
            assert_eq!(back, s);
        }
        assert_eq!(
            encode_via_mutator("foob", EncodeKind::Base64).unwrap(),
            "Zm9vYg=="
        );
        assert_eq!(
            encode_via_mutator("foob", EncodeKind::Base64Url).unwrap(),
            "Zm9vYg"
        );
    }

    #[test]
    fn negative_cases_structured_rejection() {
        // 判据 4 负例硬门。
        assert!(matches!(
            apply_one("abc", DecodeOp::HexDecode),
            Err(DecodeError::InvalidHex { .. })
        ));
        assert!(matches!(
            apply_one("zz", DecodeOp::HexDecode),
            Err(DecodeError::InvalidHex { .. })
        ));
        assert!(matches!(
            apply_one("a%2", DecodeOp::UrlDecode),
            Err(DecodeError::InvalidHex { .. })
        ));
        assert!(matches!(
            apply_one("ab%zz", DecodeOp::UrlDecode),
            Err(DecodeError::InvalidHex { .. })
        ));
        assert!(matches!(
            apply_one("ab*cd", DecodeOp::B64Decode),
            Err(DecodeError::InvalidBase64 { .. })
        ));
        assert!(matches!(
            apply_one("A", DecodeOp::B64Decode),
            Err(DecodeError::InvalidBase64 { .. })
        ));
        assert!(matches!(
            apply_one("\\uD83D", DecodeOp::UnicodeUnescape),
            Err(DecodeError::InvalidUnicodeEscape(_))
        ));
        assert!(matches!(
            apply_one("\\uDC00", DecodeOp::UnicodeUnescape),
            Err(DecodeError::InvalidUnicodeEscape(_))
        ));
        assert!(matches!(
            apply_one("{bad", DecodeOp::JsonPretty),
            Err(DecodeError::InvalidJson(_))
        ));
        assert!(matches!(
            apply_one("a.b", DecodeOp::JwtDecode),
            Err(DecodeError::InvalidJwt(_))
        ));
    }

    #[test]
    fn unicode_escape_roundtrip_astral_and_cjk() {
        let src = "中🚀é";
        let escaped = apply_one(src, DecodeOp::UnicodeEscape).unwrap();
        assert_eq!(escaped, "\\u4E2D\\uD83D\\uDE80\\u00E9");
        let back = apply_one(&escaped, DecodeOp::UnicodeUnescape).unwrap();
        assert_eq!(back, src);
    }

    #[test]
    fn json_and_sha_and_jwt_faces() {
        let compact = apply_one("{\"b\": 2, \"a\": 1}", DecodeOp::JsonPretty).unwrap();
        assert!(compact.contains("\n"), "pretty 多行");
        let back = apply_one(&compact, DecodeOp::JsonCompact).unwrap();
        // serde_json Value 默认键序归一（BTreeMap）——压缩输出键序稳定。
        assert_eq!(back, "{\"a\":1,\"b\":2}");
        assert_eq!(
            apply_one("{\"a\":1}", DecodeOp::Sha256).unwrap(),
            wsw_message::cas::sha256_hex(b"{\"a\":1}"),
            "sha256 面 = wsw_message::cas 唯一挂点"
        );
        // JWT：header.payload.signature 结构展示 + 不验签断言。
        let jwt = format!(
            "{}.{}.sig",
            encode_via_mutator("{\"alg\":\"HS256\"}", EncodeKind::Base64Url).unwrap(),
            encode_via_mutator("{\"sub\":\"u1\"}", EncodeKind::Base64Url).unwrap()
        );
        let out = apply_one(&jwt, DecodeOp::JwtDecode).unwrap();
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["header"]["alg"], "HS256");
        assert_eq!(v["payload"]["sub"], "u1");
        assert_eq!(v["verified"], false, "不验签断言");
    }

    #[test]
    fn multi_level_chain_and_cap() {
        // 多级链：src → hex → b64 → hex 三级编码，逆序解码还原。
        let src = "chain-payload";
        let l1 = apply_ops(src, &[DecodeOp::HexEncode]).unwrap();
        let l2 = apply_ops(&l1, &[DecodeOp::B64Encode]).unwrap();
        let l3 = apply_ops(&l2, &[DecodeOp::HexEncode]).unwrap();
        let back = apply_ops(
            &l3,
            &[
                DecodeOp::HexDecode,
                DecodeOp::B64Decode,
                DecodeOp::HexDecode,
            ],
        )
        .unwrap();
        assert_eq!(back, src, "三级链逆序解码恒等");
        let big = "x".repeat(DECODER_INPUT_CAP + 1);
        assert!(matches!(
            apply_ops(&big, &[DecodeOp::Sha256]),
            Err(DecodeError::InputTooLarge(_))
        ));
    }
}
