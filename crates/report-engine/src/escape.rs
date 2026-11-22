//! HTML 转义单点函数（PHASE-6A D5——零模板引擎，手写拼接的转义面）。
//!
//! 单点 `escape_html`：`< > & " '` 五字符穷举转义（属性上下文安全——
//! `"`/`'` 同时覆盖双引号与单引号属性）。escape 单点函数可单测穷举（D5）；
//! HTML 产物中所有文本/属性插值必须且只能经本函数（render::html 内聚）。

/// 转义 HTML 文本与属性上下文的五个保留字符。
pub fn escape_html(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for c in input.chars() {
        match c {
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '&' => out.push_str("&amp;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}
