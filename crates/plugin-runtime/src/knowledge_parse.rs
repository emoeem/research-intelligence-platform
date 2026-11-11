//! SK-1 Markdown 纯文本解析（P6-9 第 4 步：link extraction、frontmatter、
//! 标题路径、表格、源位置）。
//!
//! 纪律（安全知识源计划 §6.2A/§7 第 3 条）：纯解析，不执行 Markdown/HTML
//! 中任何代码，不访问链接；scheme 白名单 http/https，`javascript:`/`data:`/
//! `file:` 等一律过滤并计数；保留原始 URL，不从文字猜造 URL。
//! 无正则依赖（std 手写扫描），与 workspace 既有零新依赖纪律一致。

use std::collections::BTreeMap;

/// 单条原始链接（行号 1 起始；源位置语义）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawLink {
    /// 1 起始行号。
    pub line: u64,
    /// 链接文字（`[text]` 内原文，去方括号）。
    pub text: String,
    /// 原始 URL（未规范化）。
    pub url: String,
    /// 同行链接后缀描述（`[t](u): desc` / `[t](u) - desc` 形态，可空）。
    pub description: String,
    /// 是否位于表格行（行首 `|`，计划 §8 P6-9 第 4 步「表格」语义）。
    pub in_table: bool,
}

/// 过滤计数（scheme 拒绝等，进 knowledge_source_entry.filtered_note 复核）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FilterStats {
    /// 非 http/https scheme 被过滤的链接数。
    pub scheme_rejected: u64,
    /// 无 scheme（相对路径/纯锚点）被过滤的链接数。
    pub no_scheme: u64,
}

/// frontmatter 解析结果：键值对（保序 BTreeMap）+ 去 frontmatter 后正文。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Frontmatter {
    /// `---` 围栏内 `key: value` 键值对（value 去引号；列表/嵌套不解析）。
    pub pairs: BTreeMap<String, String>,
    /// 是否存在合法 frontmatter 围栏。
    pub present: bool,
    /// 正文（去围栏行后剩余全文，保留原行序）。
    pub body: String,
    /// 正文首行前的行数（frontmatter 围栏与键值行合计；正文第 i 行的
    /// 原文行号 = lines_before_body + i，1 起始）。
    pub lines_before_body: u64,
}

/// 拆 frontmatter（`---` 首行起至下一 `---`/`...` 行止；其余情况原样返回）。
pub fn strip_frontmatter(text: &str) -> Frontmatter {
    let mut lines = text.lines();
    let first = lines.next().map(str::trim_end);
    if first != Some("---") {
        return Frontmatter {
            pairs: BTreeMap::new(),
            present: false,
            body: text.to_string(),
            lines_before_body: 0,
        };
    }
    let mut pairs = BTreeMap::new();
    let mut rest: Vec<&str> = Vec::new();
    let mut in_fence = true;
    let mut consumed: u64 = 1; // 首行 --- 已消费
    for line in lines {
        let trimmed = line.trim_end();
        consumed += 1;
        if in_fence && (trimmed == "---" || trimmed == "...") {
            in_fence = false;
            continue;
        }
        if in_fence {
            if let Some((k, v)) = split_kv(trimmed) {
                pairs.insert(k, v);
            }
            continue;
        }
        rest.push(line);
    }
    // 围栏未闭合：按无 frontmatter 处理（不猜造）。
    if in_fence {
        return Frontmatter {
            pairs: BTreeMap::new(),
            present: false,
            body: text.to_string(),
            lines_before_body: 0,
        };
    }
    // consumed 含闭栏行，正文从其下一行起：前置行数 = consumed。
    let lines_before_body = consumed;
    let mut body = rest.join("\n");
    if text.ends_with('\n') {
        body.push('\n');
    }
    Frontmatter {
        pairs,
        present: true,
        body,
        lines_before_body,
    }
}

/// `key: value` 单行拆分（value 去成对引号；空 value 记空串）。
fn split_kv(line: &str) -> Option<(String, String)> {
    let (k, v) = line.split_once(':')?;
    let key = k.trim().to_string();
    if key.is_empty() {
        return None;
    }
    let mut value = v.trim().to_string();
    for quote in ['"', '\''] {
        if value.len() >= 2 && value.starts_with(quote) && value.ends_with(quote) {
            value = value[1..value.len() - 1].to_string();
        }
    }
    Some((key, value))
}

/// ATX 标题行（`#{1,6} `）解析结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Heading {
    /// 1 起始行号。
    pub line: u64,
    /// 标题级别 1..=6。
    pub level: u8,
    /// 标题文本（去 `#` 与围空）。
    pub text: String,
}

/// 提取 ATX 标题（跳过 frontmatter 与围栏代码块；setext 不在词表内，漏报
/// 优于误报，标题路径只服务分类与锚点）。
pub fn extract_headings(text: &str) -> Vec<Heading> {
    let fm = strip_frontmatter(text);
    let mut out = Vec::new();
    let mut in_fence = false;
    let base_line: u64 = fm.lines_before_body;
    for (i, raw) in fm.body.lines().enumerate() {
        let trimmed = raw.trim_start();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            in_fence = !in_fence;
            continue;
        }
        if in_fence {
            continue;
        }
        let hashes = trimmed.chars().take_while(|c| *c == '#').count();
        if (1..=6).contains(&hashes) {
            let rest = &trimmed[hashes..];
            if let Some(text_after) = rest.strip_prefix(' ') {
                let title = text_after.trim().trim_end_matches('#').trim();
                if !title.is_empty() {
                    out.push(Heading {
                        line: base_line + i as u64 + 1,
                        level: hashes as u8,
                        text: title.to_string(),
                    });
                }
            }
        }
    }
    out
}

/// 某行所在标题路径（自顶向下 h1 到最近祖先标题；无祖先为空表）。
pub fn heading_path_at(headings: &[Heading], line: u64) -> Vec<String> {
    let mut path: Vec<(u8, String)> = Vec::new();
    for h in headings {
        if h.line > line {
            break;
        }
        // 弹出同级与更深级标题（h2 之后遇 h1 即清栈）。
        while let Some((lvl, _)) = path.last() {
            if *lvl >= h.level {
                path.pop();
            } else {
                break;
            }
        }
        path.push((h.level, h.text.clone()));
    }
    path.into_iter().map(|(_, t)| t).collect()
}

/// GitHub 风格章节锚点 slug（小写、空格转连字符、去标点；CJK 保留原字）。
pub fn slugify(heading: &str) -> String {
    let mut out = String::new();
    for c in heading.chars() {
        if c.is_whitespace() {
            out.push('-');
        } else if c.is_alphanumeric() || c == '-' || c == '_' {
            out.push(c.to_ascii_lowercase());
        }
        // 其余标点丢弃（GitHub slug 语义近似）。
    }
    out.trim_matches('-').to_string()
}

/// 提取行内链接 `[text](url)`（跳过 frontmatter 与围栏代码块；一行多条；
/// 引用式 `[t]: u` 与图片 `![alt](u)` 不在词表内，图片按链接文字前缀
/// `!` 剔除）。
pub fn extract_markdown_links(text: &str) -> (Vec<RawLink>, FilterStats) {
    let fm = strip_frontmatter(text);
    let base_line: u64 = fm.lines_before_body;
    let mut links = Vec::new();
    let mut stats = FilterStats::default();
    let mut in_fence = false;
    for (i, raw) in fm.body.lines().enumerate() {
        let line_no = base_line + i as u64 + 1;
        let trimmed_start = raw.trim_start();
        if trimmed_start.starts_with("```") || trimmed_start.starts_with("~~~") {
            in_fence = !in_fence;
            continue;
        }
        if in_fence {
            continue;
        }
        let in_table = trimmed_start.starts_with('|');
        let mut rest = raw;
        while let Some(open) = rest.find('[') {
            // 图片 `![`：跳过（图片链接不是目录条目）。
            let label_start = open + 1;
            if open > 0 && rest.as_bytes()[open - 1] == b'!' {
                rest = &rest[label_start..];
                continue;
            }
            let Some(close) = find_byte(&rest[label_start..], b']') else {
                break;
            };
            let label = &rest[label_start..label_start + close];
            let after = &rest[label_start + close + 1..];
            if !after.starts_with('(') {
                rest = after;
                continue;
            }
            let Some(paren_close) = find_byte(&after[1..], b')') else {
                break;
            };
            let url = after[1..1 + paren_close].trim().to_string();
            let desc_raw = &after[1 + paren_close + 1..];
            let description = desc_raw
                .trim()
                .trim_start_matches([':', '-'])
                .trim()
                .to_string();
            // scheme 白名单过滤（计划 §6.2A）：`scheme:` 形态（含 javascript:/
            // data:/file:/mailto:）非 http/https 即拒绝；无 scheme（相对路径/
            // 纯锚点）单独计数，不从文字猜造 URL。
            let mut rejected = false;
            match url.find(':') {
                Some(colon) => {
                    let scheme = &url[..colon];
                    let valid = !scheme.is_empty()
                        && scheme
                            .chars()
                            .all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '-' || c == '.');
                    let lower = scheme.to_ascii_lowercase();
                    if valid && !matches!(lower.as_str(), "http" | "https") {
                        stats.scheme_rejected += 1;
                        rejected = true;
                    }
                }
                None => {
                    stats.no_scheme += 1;
                    rejected = true;
                }
            }
            if rejected {
                rest = &after[1 + paren_close + 1..];
                continue;
            }
            links.push(RawLink {
                line: line_no,
                text: label.trim().to_string(),
                url,
                description,
                in_table,
            });
            rest = &after[1 + paren_close + 1..];
        }
    }
    (links, stats)
}

/// URL 规范化（去首尾空白、去尾部 `/`；保留大小写，路径大小写敏感）。
/// 返回 None 表示空白（调用方跳过）。
pub fn normalize_url(url: &str) -> Option<String> {
    let trimmed = url.trim();
    if trimmed.is_empty() {
        return None;
    }
    let lower_scheme = trimmed
        .split("://")
        .next()
        .unwrap_or("")
        .to_lowercase();
    let mut normalized = trimmed.to_string();
    if matches!(lower_scheme.as_str(), "http" | "https") {
        while normalized.ends_with('/') {
            normalized.pop();
        }
    }
    Some(normalized)
}

fn find_byte(haystack: &str, byte: u8) -> Option<usize> {
    haystack.bytes().position(|b| b == byte)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frontmatter_split_and_pairs() {
        let fm = strip_frontmatter("---\ntitle: A 技能\nversion: \"0.2\"\n---\n\n正文");
        assert!(fm.present);
        assert_eq!(fm.pairs.get("title").map(String::as_str), Some("A 技能"));
        assert_eq!(fm.pairs.get("version").map(String::as_str), Some("0.2"));
        assert!(fm.body.starts_with("\n正文"));
        let no = strip_frontmatter("# 无围栏\n正文");
        assert!(!no.present);
        assert_eq!(no.body, "# 无围栏\n正文");
        // 未闭合围栏：按无 frontmatter。
        let open = strip_frontmatter("---\ntitle: x\n正文");
        assert!(!open.present);
    }

    #[test]
    fn links_extracted_with_line_and_table_flag_and_scheme_filter() {
        let doc = "# 分类\n\n- [Name](https://a.example/x): 描述一\n\n| 列 | 链接 |\n|---|---|\n| a | [Tbl](http://b.example/y) |\n\n```sh\n[bad](javascript:alert(1))\n```\n\n[rel](./other.md) 与 [evil](javascript:x) 与 [anchor](#top)\n";
        let (links, stats) = extract_markdown_links(doc);
        let urls: Vec<&str> = links.iter().map(|l| l.url.as_str()).collect();
        assert_eq!(urls, vec!["https://a.example/x", "http://b.example/y"]);
        assert_eq!(links[0].line, 3);
        assert_eq!(links[0].description, "描述一");
        assert!(!links[0].in_table);
        assert!(links[1].in_table, "table-row link flagged");
        assert_eq!(stats.scheme_rejected, 1, "javascript: inside fence skipped by fence AND scheme filter counts the unfenced one");
        assert_eq!(stats.no_scheme, 2, "relative + anchor-only rejected");
    }

    #[test]
    fn heading_path_and_anchor() {
        let doc = "# A\n\n## B\n\n文本\n\n### C\n\n## D\n";
        let hs = extract_headings(doc);
        assert_eq!(hs.len(), 4);
        assert_eq!(
            heading_path_at(&hs, 7),
            vec!["A".to_string(), "B".to_string(), "C".to_string()]
        );
        assert_eq!(heading_path_at(&hs, 9), vec!["A".to_string(), "D".to_string()]);
        assert_eq!(slugify("A B (C)!"), "a-b-c");
        assert_eq!(slugify("渗透 检查"), "渗透-检查");
    }

    #[test]
    fn url_normalization_trailing_slash() {
        assert_eq!(
            normalize_url("https://a.example/x/").as_deref(),
            Some("https://a.example/x")
        );
        assert_eq!(normalize_url("   "), None);
    }
}
