//! token 估算器（设计稿 §5.2 口径，纯函数，零 IO）。
//!
//! `est_tokens(text) = ceil(ascii_chars / 4 + cjk_chars * 1.0 + other_chars / 2)`。
//!
//! 这是近似值，未对真实 tokenizer 校准（设计稿 §13.1 待验证；ROADMAP §5.5
//! 风险 1：P4-6 前用 models/bge-small-zh-v1.5/tokenizer.json 离线抽样校准，
//! 校准只修常数不改结构）。分母口径：ASCII 约 4 字符 1 token，CJK 表意
//! 字符约 1 字符 1 token，其余约 2 字符 1 token。
//!
//! CJK 判定范围（本实现口径，校准时随常数一并复核）：U+3000..=U+303F
//! （CJK 符号与标点）、U+3040..=U+30FF（平假名/片假名）、U+3400..=U+4DBF
//! （汉字扩展 A）、U+4E00..=U+9FFF（CJK 统一表意）、U+F900..=U+FAFF（CJK
//! 兼容表意）、U+FF00..=U+FFEF（半角全角形式）、U+20000..=U+2FA1F（表意
//! 补充与兼容补充）。
//!
//! 使用纪律（设计稿 §5.2）：候选入库时预计算 est_tokens（摘要与正文分开
//! 算），装配期只做加法，不重复扫文本。

/// 字符分类计数（est_tokens 的中间量，暴露供测试与校准抽样使用）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CharCounts {
    /// ASCII 字符数（U+0000 到 U+007F）。
    pub ascii: u64,
    /// CJK 表意字符数（判定范围见模块注）。
    pub cjk: u64,
    /// 其余字符数（emoji、西文带符号、其他文字系统等）。
    pub other: u64,
}

impl CharCounts {
    /// 三类计数之和（= 字符总数，UTF-32 语义，即 chars().count()）。
    pub fn total(&self) -> u64 {
        self.ascii + self.cjk + self.other
    }
}

/// 是否 CJK 表意字符（判定范围见模块注；私有分类函数）。
fn is_cjk_like(c: char) -> bool {
    let cp = c as u32;
    matches!(cp,
        0x3000..=0x303F   // CJK 符号与标点
        | 0x3040..=0x30FF // 平假名/片假名
        | 0x3400..=0x4DBF // 汉字扩展 A
        | 0x4E00..=0x9FFF // CJK 统一表意
        | 0xF900..=0xFAFF // CJK 兼容表意
        | 0xFF00..=0xFFEF // 半角全角形式
        | 0x20000..=0x2FA1F // 表意补充 + 兼容补充
    )
}

/// 按三类（ascii / cjk / other）统计字符数。
pub fn count_chars(text: &str) -> CharCounts {
    let mut counts = CharCounts {
        ascii: 0,
        cjk: 0,
        other: 0,
    };
    for c in text.chars() {
        if c.is_ascii() {
            counts.ascii += 1;
        } else if is_cjk_like(c) {
            counts.cjk += 1;
        } else {
            counts.other += 1;
        }
    }
    counts
}

/// 估算文本 token 数（设计稿 §5.2 公式，向上取整；空文本为 0）。
pub fn est_tokens(text: &str) -> u64 {
    let counts = count_chars(text);
    let raw = counts.ascii as f64 / 4.0 + counts.cjk as f64 * 1.0 + counts.other as f64 / 2.0;
    raw.ceil() as u64
}
