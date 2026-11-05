//! 词元化与关键词扩展（与 eri_p2.resolve 的词元级匹配口径对齐）。
//!
//! eri 口径（eri_p2.py）：
//! - 候选 blob = name + description + topics + language 小写；本批 platform 库
//!   无 topics / language 列，blob = name + description + canonical_url
//! - 词元：正则 `[a-z0-9+#.\\-]{3,}` 提取连续段（>=3 字符），再按 `-` 拆分
//! - 命中：keyword 在词元集中，或 keyword+"s" 在（复数容忍）
//! - ALIASES 显式扩展合成词（js/spa/api/android/...），不做前缀匹配防误中

/// 确定性别名扩展表（同步自 eri_p2.py ALIASES，数据非代码，勿自行增删）。
pub const ALIASES: &[(&str, &[&str])] = &[
    ("js", &["javascript"]),
    ("spa", &["javascript", "bundle", "frontend"]),
    ("api", &["openapi", "swagger", "rest"]),
    ("android", &["apk", "mobile"]),
    ("crypto", &["cryptography", "jwt", "encryption"]),
    ("cdn", &["edge", "cloudfront"]),
    ("web", &["webapp", "webapplication"]),
    ("landing", &["landing", "redirect"]),
    ("miniapp", &["wechat", "wxapp"]),
    ("ios", &["osx", "iphone"]),
];

/// 关键词扩展：小写化、去空白、别名展开、去重排序（eri `_expand` 口径）。
pub fn expand_keywords(keywords: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for k in keywords {
        let k = k.to_lowercase().trim().to_string();
        if k.is_empty() {
            continue;
        }
        out.push(k.clone());
        for alias in aliases_of(&k) {
            out.push(alias.to_string());
        }
    }
    out.sort();
    out.dedup();
    out
}

/// 词元化：连续 `[a-z0-9+#.\-]` 段（>=3 字符）按 `-` 拆分（eri 正则口径）。
pub fn tokenize(text: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut run = String::new();
    let flush = |run: &mut String, tokens: &mut Vec<String>| {
        if run.chars().count() >= 3 {
            for seg in run.split('-') {
                tokens.push(seg.to_string());
            }
        }
        run.clear();
    };
    for ch in text.to_lowercase().chars() {
        if is_token_char(ch) {
            run.push(ch);
        } else {
            flush(&mut run, &mut tokens);
        }
    }
    flush(&mut run, &mut tokens);
    tokens.sort();
    tokens.dedup();
    tokens
}

/// 词元级关键词命中判定（整词或复数）。返回引用指向 `keywords`。
pub fn keyword_hits<'a>(keywords: &'a [String], tokens: &[String]) -> Vec<&'a str> {
    keywords
        .iter()
        .filter(|k| {
            tokens.iter().any(|t| t.as_str() == k.as_str())
                || tokens.iter().any(|t| t.as_str() == format!("{k}s"))
        })
        .map(|k| k.as_str())
        .collect()
}

fn aliases_of(k: &str) -> &'static [&'static str] {
    ALIASES
        .iter()
        .find(|(key, _)| *key == k)
        .map(|(_, aliases)| *aliases)
        .unwrap_or(&[])
}

fn is_token_char(c: char) -> bool {
    c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '+' | '#' | '.' | '-')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokenize_matches_eri_regex_semantics() {
        // 短词（<3）丢弃；连字符拆分；大小写归一；非词字符切断。
        let toks = tokenize("Awesome Fuzzing: ILIT-UI, jwt-auth tools. web-app");
        assert!(toks.contains(&"fuzzing".to_string()));
        assert!(toks.contains(&"ilit".to_string()));
        assert!(toks.contains(&"ui".to_string()));
        assert!(toks.contains(&"jwt".to_string()));
        assert!(toks.contains(&"auth".to_string()));
        assert!(toks.contains(&"web".to_string()));
        assert!(toks.contains(&"app".to_string()));
        // "tools." 整段（含点）长 6：eri 正则把点留在段内，故词元为 "tools."
        //（eri 同款行为，真实目录里 keywords 命中不受点尾影响）。
        assert!(toks.contains(&"tools.".to_string()));
        assert!(!toks.contains(&"tools".to_string()));
        // 2 字符独立段（"ip"）不出现
        assert!(!toks.contains(&"ip".to_string()));
    }

    #[test]
    fn expand_aliases_android() {
        let kw = expand_keywords(&["android".to_string()]);
        assert_eq!(kw, vec!["android", "apk", "mobile"]);
        let kw2 = expand_keywords(&["  Payload ".to_string(), "PAYLOAD".to_string()]);
        assert_eq!(kw2, vec!["payload"]);
    }

    #[test]
    fn plural_and_exact_match() {
        let toks = tokenize("fuzzers payload-bypass");
        let kw = vec!["fuzzer".to_string(), "bypass".to_string()];
        let hits = keyword_hits(&kw, &toks);
        assert!(hits.contains(&"fuzzer"), "fuzzers matches fuzzer+s");
        assert!(hits.contains(&"bypass"));
        let miss_kw = vec!["fuzz".to_string()];
        let miss = keyword_hits(&miss_kw, &toks);
        assert!(miss.is_empty(), "no prefix matching");
    }
}
