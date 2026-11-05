//! 检索分词：ASCII 沿用 eri 口径，中文 2-gram 预分词。
//!
//! **eri 口径事实核对（M4 任务要求的"查 eri.py 实现方式"结论）**：eri 全链
//! （eri_p1.tokens / eri_p2.resolve 的 blob_toks）没有 2-gram 实现，其正则
//! `[a-z0-9+#.\-]{3,}` 会把 CJK 字符整体丢弃；eri 对中文的唯一处理是
//! `CJK_BRIDGE` 查表（中文画像特征 -> 英文检索词，确定性，不做翻译）。
//! 本 crate 的分词策略因此为：
//! - ASCII：与 eri 同策略（连续 `[a-z0-9+#.\-]` 段 >=3 字符、按 `-` 拆分、
//!   ALIASES 别名扩展），词元函数直接复用 resource-engine::tokenizer（单一
//!   同步源，数据表非代码）。
//! - 中文（Han 字符）：2-gram 预分词入库（FTS5 unicode61 不切 CJK，整句
//!   会变成单一长 token 无法子串命中；入库前把 Han 连续段切成重叠 2-gram
//!   是 FTS5 的标准做法）。查询侧同策略切词，多 gram 词组以 FTS5 短语查询
//!   要求连续命中，避免单 gram 误召回。
//! - CJK_BRIDGE：同步自 eri_p2.py（数据表非代码，勿自行增删），查询扩展
//!   与 eri 同口径（子串命中即桥接英文词）。

/// 确定性中文桥接表（同步自 eri_p2.py CJK_BRIDGE，数据非代码，勿自行增删）。
pub const CJK_BRIDGE: &[(&str, &[&str])] = &[
    ("落地页", &["landing", "redirect"]),
    ("跳转", &["redirect"]),
    ("资产", &["asset", "discovery"]),
    ("加密", &["crypto", "encryption"]),
    ("密文", &["crypto", "encryption"]),
    ("小程序", &["miniapp", "wechat"]),
    ("边缘", &["cdn", "edge"]),
    ("代理", &["proxy"]),
    ("移动", &["mobile", "android"]),
    ("安卓", &["android", "apk"]),
    ("接口", &["api"]),
    ("渗透", &["pentest"]),
    ("溯源", &["osint"]),
    ("口令", &["password"]),
    ("密码", &["password"]),
    ("端口", &["scan", "recon"]),
    ("跳板", &["tunnel", "pivot"]),
    ("隧道", &["tunnel"]),
];

/// 字符是否为 Han（CJK 统一表意文字区段）。
pub fn is_han(c: char) -> bool {
    ('\u{4E00}'..='\u{9FFF}').contains(&c)
        || ('\u{3400}'..='\u{4DBF}').contains(&c)
        || ('\u{F900}'..='\u{FAFF}').contains(&c)
}

/// 文本是否含 Han 字符。
pub fn has_han(text: &str) -> bool {
    text.chars().any(is_han)
}

/// Han 连续段的重叠 2-gram 切分；单字符段产出自身（保证可命中）。
/// 非连续 Han 字符各自成段（与 FTS 入库切分同函数，保证查询/索引对称）。
pub fn cjk_grams(text: &str) -> Vec<String> {
    let mut grams = Vec::new();
    let mut run: Vec<char> = Vec::new();
    let flush = |run: &mut Vec<char>, grams: &mut Vec<String>| {
        if run.len() == 1 {
            grams.push(run[0].to_string());
        } else if run.len() >= 2 {
            for w in run.windows(2) {
                grams.push(w.iter().collect::<String>());
            }
        }
        run.clear();
    };
    for ch in text.chars() {
        if is_han(ch) {
            run.push(ch);
        } else {
            flush(&mut run, &mut grams);
        }
    }
    flush(&mut run, &mut grams);
    grams
}

/// 索引入库文本：ASCII 段（eri 正则口径，按 `-` 拆分）与 Han 2-gram 以空格
/// 连接，供 FTS5 unicode61 tokenizer 二次切分。保持原文顺序（CJK 短语查询
/// 依赖 gram 连续性）。纯 ASCII 段少于 3 字符且不含 Han 时按 eri 口径丢弃。
pub fn index_text(text: &str) -> String {
    let mut parts: Vec<String> = Vec::new();
    let mut run = String::new();
    let flush = |run: &mut String, parts: &mut Vec<String>| {
        if run.chars().count() >= 3 {
            for seg in run.split('-') {
                parts.push(seg.to_string());
            }
        }
        run.clear();
    };
    for ch in text.to_lowercase().chars() {
        if is_han(ch) {
            flush(&mut run, &mut parts);
            parts.push(ch.to_string());
        } else if ch.is_whitespace() {
            flush(&mut run, &mut parts);
        } else {
            run.push(ch);
        }
    }
    flush(&mut run, &mut parts);
    // 二次展开：单字 Han 字符 -> 2-gram（相邻单字合成 gram，保持顺序）
    expand_single_han_to_grams(&parts).join(" ")
}

/// 把 [`index_text`] 产出的单字 Han 序列重切为重叠 2-gram。
fn expand_single_han_to_grams(parts: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut han_run: Vec<String> = Vec::new();
    let flush = |han_run: &mut Vec<String>, out: &mut Vec<String>| {
        if han_run.len() == 1 {
            out.push(han_run[0].clone());
        } else if han_run.len() >= 2 {
            for w in han_run.windows(2) {
                out.push(format!("{}{}", w[0], w[1]));
            }
        }
        han_run.clear();
    };
    for p in parts {
        if p.chars().count() == 1 && p.chars().next().is_some_and(is_han) {
            han_run.push(p.clone());
        } else {
            flush(&mut han_run, &mut out);
            out.push(p.clone());
        }
    }
    flush(&mut han_run, &mut out);
    out
}

/// 查询词覆盖计分用的词元集（eri blob 口径：整词或复数命中，见
/// resource-engine::tokenizer::keyword_hits）。CJK 词走 gram 集命中判定：
/// 查询词的 gram 全部出现在文档 gram 集中即视为命中（等价子串存在）。
pub fn coverage_tokens(text: &str) -> Vec<String> {
    let mut tokens: Vec<String> = resource_engine::tokenizer::tokenize(text);
    for g in cjk_grams(text) {
        tokens.push(g);
    }
    tokens.sort();
    tokens.dedup();
    tokens
}

/// 查询词扩展：ALIASES 别名（resource-engine 单一同步源）+ CJK_BRIDGE 桥接
/// （eri `_cjk_bridge` 口径：桥接词出现在任一关键词子串中即并入）。
pub fn expand_query_terms(keywords: &[String]) -> Vec<String> {
    let mut out: Vec<String> = resource_engine::tokenizer::expand_keywords(keywords);
    let joined = keywords.join(" ");
    for (cjk, en) in CJK_BRIDGE {
        if joined.contains(cjk) {
            for w in *en {
                out.push((*w).to_string());
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

/// 查询词是否为 CJK 词（含 Han 字符即走 gram 短语路径）。
pub fn is_cjk_term(term: &str) -> bool {
    has_han(term)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cjk_grams_overlap() {
        assert_eq!(
            cjk_grams("渗透测试"),
            vec!["渗透".to_string(), "透测".to_string(), "测试".to_string()]
        );
        assert_eq!(cjk_grams("密"), vec!["密".to_string()]);
        assert!(cjk_grams("abc").is_empty());
        // 非连续 Han 各自成段
        assert_eq!(cjk_grams("渗a透"), vec!["渗".to_string(), "透".to_string()]);
    }

    #[test]
    fn index_text_uses_eri_ascii_and_cjk_grams() {
        let t = index_text("Payload 渗透测试 bypass-all");
        // ASCII 段按 eri 口径拆分，顺序保持
        assert!(t.contains("payload"));
        assert!(t.contains("bypass"));
        assert!(t.contains("all"));
        // Han 段切为重叠 2-gram，顺序连续（短语查询依赖）
        assert!(t.contains("渗透 透测 测试"));
        // 少于 3 字符的独立 ASCII 段丢弃（eri 口径）
        let t2 = index_text("ip 渗透");
        assert!(!t2.contains(" ip "), "short ascii run dropped: {t2}");
        assert!(t2.contains("渗透"));
    }

    #[test]
    fn index_text_handles_dotted_tokens_like_eri() {
        // eri 正则把点留在段内："tools." 是一个词元；FTS 侧 unicode61 再切，
        // 但覆盖计分词元集（coverage_tokens）与 eri 完全一致。
        assert!(coverage_tokens("tools.").contains(&"tools.".to_string()));
    }

    #[test]
    fn expand_query_terms_bridges_and_aliases() {
        let kw = expand_query_terms(&["渗透".to_string()]);
        assert!(
            kw.contains(&"pentest".to_string()),
            "CJK bridge applied: {kw:?}"
        );
        let kw2 = expand_query_terms(&["android".to_string()]);
        assert_eq!(kw2, vec!["android", "apk", "mobile"]);
        let kw3 = expand_query_terms(&["cdn".to_string(), "加密".to_string()]);
        assert!(kw3.contains(&"edge".to_string()));
        assert!(kw3.contains(&"crypto".to_string()));
    }

    #[test]
    fn coverage_tokens_cjk_hit_semantics() {
        // 文档含 "渗透测试"：gram 集 = {渗透, 透测, 测试}
        let doc = coverage_tokens("网络渗透测试工具");
        // 查询词 "渗透测试"：gram 全部出现 -> 命中
        let q = cjk_grams("渗透测试");
        assert!(q.iter().all(|g| doc.contains(g)));
        // 查询词 "测渗透"（不同组合）：gram "测渗" 不在文档 -> 不命中
        let q2 = cjk_grams("测渗透");
        assert!(!q2.iter().all(|g| doc.contains(g)));
    }
}
