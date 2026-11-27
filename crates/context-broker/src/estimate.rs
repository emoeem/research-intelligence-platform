//! token 估算器（模型感知的 prompt 预算口径）。
//!
//! `est_tokens(text)` 优先使用 `RIP_LLM_TOKENIZER_DIR` 指向的模型匹配 HF
//! tokenizer.json；未配置时回退 `est_tokens_formula`。回退值明确是估算值，
//! 不代表 DeepSeek/Kimi 或其他生成模型的精确 Token 数。
//!
//! `EMBEDDING_TOKENIZER_DIR` / BGE tokenizer 只属于 Embedding 模型，不参与
//! 生成模型上下文预算。候选来源自带的 `est_tokens` 仅作来源元数据；装配时
//! 对实际摘要文本按当前进程的 prompt tokenizer 重新计量，避免跨模型复用旧值。
//!
//! CJK 字符分类范围：U+3000..=U+303F、U+3040..=U+30FF、U+3400..=U+4DBF、
//! U+4E00..=U+9FFF、U+F900..=U+FAFF、U+FF00..=U+FFEF、U+20000..=U+2FA1F。

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
///
/// `RIP_LLM_TOKENIZER_DIR` 指向模型匹配 tokenizer.json 时走 HF tokenizers
/// 真实计数；不使用 BGE/Embedding tokenizer。未配置时使用保守 Unicode 公式，
/// 该值是估算而非真实模型 token 数。所有预算调用点由本函数单点承载。
pub fn est_tokens(text: &str) -> u64 {
    if let Some(n) = crate::tokenizer::count_tokens(text) {
        return n;
    }
    est_tokens_formula(text)
}

/// Unicode 公式回退口径（adapter 缺席时使用；始终标记为估算）。
pub fn est_tokens_formula(text: &str) -> u64 {
    let counts = count_chars(text);
    let raw = counts.ascii as f64 / 4.0 + counts.cjk as f64 * 1.0 + counts.other as f64 / 2.0;
    raw.ceil() as u64
}
