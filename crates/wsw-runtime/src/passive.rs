//! `wsw_runtime::passive`：Passive Audit 内置规则注册表 + 逐行分析入口
//! （WSW-6；PLAN §4.8 :305-311 语义面）。
//!
//! - 注册表 = 固定 Rust 常量起步（D7：五类 14 规则——安全 Header 缺失 /
//!   Cookie 属性 / 信息泄露迹象 / 异常跳转 / 常见配置问题；rule_id CHECK
//!   词表 = `migrations/030_wsw_ws_frame.sql` 逐字同源；规则参数 = 注册表
//!   常量，本批零规则文件加载面——JSON 规则文件加载登记观察池）；
//! - 分析面 = [`analyze_exchange`] 行级纯函数（headers/status/scheme 列投影，
//!   零 CAS 读零网络 IO——旁路面延迟约束）；
//! - 落库面 = [`analyze_and_persist`]（capture→落库异步 worker 旁路接线点，
//!   exchange 行落库后逐行分析，candidate 词形落 030 `wsw_passive_hit`——
//!   PLAN :309「被动发现只能生成候选项，不得直接成为已确认 Finding」：
//!   零 evidence_id 零 Finding 提升面）；
//! - 主动扫描 Provider 零实现（PLAN :310-311 归 WSW-7 Tool Engine 面；本
//!   注册表 = WSW-7 被动/主动入口统一时的唯一规则源，禁第二注册表）。

use crate::persist::{
    generate_hit_id, BatchInsertOutcome, ExchangeRow, PassiveHitRow, PersistStore,
};

/// 规则严重度词表（030 severity CHECK：info/low/medium/high）。
pub const SEVERITIES: [&str; 4] = ["info", "low", "medium", "high"];

/// 规则类别词表（PLAN :308 五类投影；注册表只读投影 `class` 字段词形）。
pub const RULE_CLASSES: [&str; 5] = [
    "missing_security_header",
    "cookie_attribute",
    "info_leak",
    "abnormal_redirect",
    "common_config",
];

/// 内置规则注册表项（固定常量起步，D7）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PassiveRule {
    /// 规则 ID（030 CHECK 词表逐字）。
    pub id: &'static str,
    /// 规则版本（行为语义变更即递增）。
    pub version: u64,
    /// 严重度（030 severity 词表）。
    pub severity: &'static str,
    /// 类别（[`RULE_CLASSES`] 词表）。
    pub class: &'static str,
    /// 单行说明（注册表只读投影面）。
    pub description: &'static str,
}

/// 内置规则注册表（五类 14 规则；030 rule_id CHECK 词表逐字同源）。
pub const PASSIVE_RULES: [PassiveRule; 14] = [
    // 类一：安全 Header 缺失（PLAN :308 逐类第一类）。
    PassiveRule {
        id: "missing_header_strict_transport_security",
        version: 1,
        severity: "low",
        class: "missing_security_header",
        description: "HTTPS 响应缺失 Strict-Transport-Security 头",
    },
    PassiveRule {
        id: "missing_header_content_security_policy",
        version: 1,
        severity: "low",
        class: "missing_security_header",
        description: "响应缺失 Content-Security-Policy 头",
    },
    PassiveRule {
        id: "missing_header_x_content_type_options",
        version: 1,
        severity: "low",
        class: "missing_security_header",
        description: "响应缺失 X-Content-Type-Options 头",
    },
    PassiveRule {
        id: "missing_header_x_frame_options",
        version: 1,
        severity: "info",
        class: "missing_security_header",
        description: "响应缺失 X-Frame-Options 头",
    },
    // 类二：Cookie 属性。
    PassiveRule {
        id: "cookie_missing_secure",
        version: 1,
        severity: "medium",
        class: "cookie_attribute",
        description: "Set-Cookie 缺 Secure 属性",
    },
    PassiveRule {
        id: "cookie_missing_httponly",
        version: 1,
        severity: "low",
        class: "cookie_attribute",
        description: "Set-Cookie 缺 HttpOnly 属性",
    },
    PassiveRule {
        id: "cookie_missing_samesite",
        version: 1,
        severity: "low",
        class: "cookie_attribute",
        description: "Set-Cookie 缺 SameSite 属性",
    },
    // 类三：信息泄露迹象。
    PassiveRule {
        id: "info_leak_server_version",
        version: 1,
        severity: "info",
        class: "info_leak",
        description: "Server 头携带版本号迹象（值含 /<digits>）",
    },
    PassiveRule {
        id: "info_leak_x_powered_by",
        version: 1,
        severity: "info",
        class: "info_leak",
        description: "X-Powered-By 头在场",
    },
    PassiveRule {
        id: "info_leak_via_header",
        version: 1,
        severity: "info",
        class: "info_leak",
        description: "Via 头在场（代理链路暴露）",
    },
    // 类四：异常跳转。
    PassiveRule {
        id: "redirect_cross_origin",
        version: 1,
        severity: "medium",
        class: "abnormal_redirect",
        description: "3xx Location 跨源（绝对 URL host 与请求 authority 不一致）",
    },
    PassiveRule {
        id: "redirect_scheme_downgrade",
        version: 1,
        severity: "medium",
        class: "abnormal_redirect",
        description: "HTTPS 响应 Location 降级 http://",
    },
    // 类五：常见配置问题。
    PassiveRule {
        id: "config_permissive_cors",
        version: 1,
        severity: "medium",
        class: "common_config",
        description: "Access-Control-Allow-Origin: * 全放行",
    },
    PassiveRule {
        id: "config_set_cookie_cacheable",
        version: 1,
        severity: "low",
        class: "common_config",
        description: "Set-Cookie 响应缺 Cache-Control: no-store",
    },
];

/// 分析产出草稿（→ [`PassiveHitRow`] 组装侧承载）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PassiveHitDraft {
    pub rule_id: &'static str,
    pub rule_version: u64,
    pub severity: &'static str,
    pub match_detail_json: Option<String>,
}

/// 响应头数组解析（response_headers_json 投影：`[{"name":..,"value":..}]`；
/// 解析失败 = 零头（如实缺测，不伪造））。
fn response_headers(row: &ExchangeRow) -> Vec<(String, String)> {
    let Some(json) = &row.response_headers_json else {
        return Vec::new();
    };
    #[derive(serde::Deserialize)]
    struct H {
        name: String,
        value: String,
    }
    serde_json::from_str::<Vec<H>>(json)
        .map(|hs| hs.into_iter().map(|h| (h.name, h.value)).collect())
        .unwrap_or_default()
}

/// 头取值（名大小写不敏感；返回全部同名值——Set-Cookie 多值保留）。
fn header_values<'a>(headers: &'a [(String, String)], name: &str) -> Vec<&'a str> {
    headers
        .iter()
        .filter(|(n, _)| n.eq_ignore_ascii_case(name))
        .map(|(_, v)| v.as_str())
        .collect()
}

/// 头在场判定（大小写不敏感）。
fn has_header(headers: &[(String, String)], name: &str) -> bool {
    headers.iter().any(|(n, _)| n.eq_ignore_ascii_case(name))
}

/// 命中明细 JSON（最小投影：头名 + 值预览 ≤120 字符——凭据值不落明细，
/// Set-Cookie 属性面只记缺失属性名不记 Cookie 值）。
fn detail(header: &str, value: &str) -> Option<String> {
    let preview: String = value.chars().take(120).collect();
    serde_json::json!({ "header": header, "value_preview": preview })
        .to_string()
        .into()
}

/// Set-Cookie 属性判定（属性名大小写不敏感；值形 `name=value; Attr; Attr=v`）。
fn cookie_has_attr(cookie_value: &str, attr: &str) -> bool {
    cookie_value.split(';').skip(1).any(|part| {
        part.trim()
            .split('=')
            .next()
            .unwrap_or("")
            .trim()
            .eq_ignore_ascii_case(attr)
    })
}

/// 逐行规则分析（行级纯函数：response_status/scheme/headers 列投影；
/// 无响应行 = 零命中——请求侧行不产候选，D7 被动面只审响应事实）。
pub fn analyze_exchange(row: &ExchangeRow) -> Vec<PassiveHitDraft> {
    let Some(status) = row.response_status else {
        return Vec::new();
    };
    let headers = response_headers(row);
    let mut hits = Vec::new();
    let rule = |id: &'static str| -> Option<PassiveRule> {
        PASSIVE_RULES.iter().find(|r| r.id == id).copied()
    };
    let push = |id: &'static str, d: Option<String>, hits: &mut Vec<PassiveHitDraft>| {
        if let Some(r) = rule(id) {
            hits.push(PassiveHitDraft {
                rule_id: r.id,
                rule_version: r.version,
                severity: r.severity,
                match_detail_json: d,
            });
        }
    };

    // 类一：安全 Header 缺失（HSTS 仅 https 响应——明文响应无 TLS 事实，
    // 报缺失即伪造）。
    if row.scheme.as_deref() == Some("https") && !has_header(&headers, "strict-transport-security")
    {
        push(
            "missing_header_strict_transport_security",
            detail("Strict-Transport-Security", "(absent)"),
            &mut hits,
        );
    }
    if !has_header(&headers, "content-security-policy") {
        push(
            "missing_header_content_security_policy",
            detail("Content-Security-Policy", "(absent)"),
            &mut hits,
        );
    }
    if !has_header(&headers, "x-content-type-options") {
        push(
            "missing_header_x_content_type_options",
            detail("X-Content-Type-Options", "(absent)"),
            &mut hits,
        );
    }
    if !has_header(&headers, "x-frame-options") {
        push(
            "missing_header_x_frame_options",
            detail("X-Frame-Options", "(absent)"),
            &mut hits,
        );
    }

    // 类二：Cookie 属性。
    let cookies = header_values(&headers, "set-cookie");
    if !cookies.is_empty() {
        if cookies.iter().any(|c| !cookie_has_attr(c, "Secure")) {
            push(
                "cookie_missing_secure",
                detail("Set-Cookie", "(Secure attr missing)"),
                &mut hits,
            );
        }
        if cookies.iter().any(|c| !cookie_has_attr(c, "HttpOnly")) {
            push(
                "cookie_missing_httponly",
                detail("Set-Cookie", "(HttpOnly attr missing)"),
                &mut hits,
            );
        }
        if cookies.iter().any(|c| !cookie_has_attr(c, "SameSite")) {
            push(
                "cookie_missing_samesite",
                detail("Set-Cookie", "(SameSite attr missing)"),
                &mut hits,
            );
        }
        // 类五附：Set-Cookie 响应缺 Cache-Control: no-store。
        let cacheable = match header_values(&headers, "cache-control").first() {
            Some(v) => !v.to_ascii_lowercase().contains("no-store"),
            None => true,
        };
        if cacheable {
            push(
                "config_set_cookie_cacheable",
                detail("Set-Cookie", "(no Cache-Control: no-store)"),
                &mut hits,
            );
        }
    }

    // 类三：信息泄露迹象。
    if let Some(v) = header_values(&headers, "server").first() {
        // 版本迹象 = 值含 `/` 后接数字（Apache/2.4.1、nginx/1.24 等词形）。
        let version_marker = v
            .split('/')
            .nth(1)
            .map(|tail| tail.starts_with(|c: char| c.is_ascii_digit()))
            .unwrap_or(false);
        if version_marker {
            push("info_leak_server_version", detail("Server", v), &mut hits);
        }
    }
    if let Some(v) = header_values(&headers, "x-powered-by").first() {
        push(
            "info_leak_x_powered_by",
            detail("X-Powered-By", v),
            &mut hits,
        );
    }
    if let Some(v) = header_values(&headers, "via").first() {
        push("info_leak_via_header", detail("Via", v), &mut hits);
    }

    // 类四：异常跳转（仅 3xx）。
    if (300..400).contains(&status) {
        if let Some(loc) = header_values(&headers, "location").first() {
            if let Some(rest) = loc.strip_prefix("http://") {
                if row.scheme.as_deref() == Some("https") {
                    push(
                        "redirect_scheme_downgrade",
                        detail("Location", loc),
                        &mut hits,
                    );
                }
                let _ = rest;
            }
            if let Some((_, host_part)) = loc.split_once("://") {
                let loc_host = host_part
                    .split(['/', ':', '?', '#'])
                    .next()
                    .unwrap_or("")
                    .to_ascii_lowercase();
                let req_host = row
                    .authority
                    .as_deref()
                    .unwrap_or("")
                    .rsplit_once(':')
                    .map(|(h, _)| h)
                    .unwrap_or(row.authority.as_deref().unwrap_or(""))
                    .trim_matches(['[', ']'])
                    .to_ascii_lowercase();
                if !loc_host.is_empty() && !req_host.is_empty() && loc_host != req_host {
                    push("redirect_cross_origin", detail("Location", loc), &mut hits);
                }
            }
        }
    }

    // 类五：常见配置问题。
    if let Some(v) = header_values(&headers, "access-control-allow-origin").first() {
        if v.trim() == "*" {
            push(
                "config_permissive_cors",
                detail("Access-Control-Allow-Origin", v),
                &mut hits,
            );
        }
    }

    hits
}

/// 逐行分析 → 030 命中行落库（worker 旁路接线点；INSERT OR IGNORE 幂等锚，
/// 返回 inserted/failed 计数）。analyze_and_persist 不抛错——旁路面失败只
/// 计数（listener 转发路径零同步调用约束的对偶：本函数在 worker 侧运行，
/// 失败不炸 worker）。
pub fn analyze_and_persist(
    store: &PersistStore,
    rows: &[ExchangeRow],
) -> DbResultOf<BatchInsertOutcome> {
    let now = crate::persist::now_iso();
    let mut hit_rows = Vec::new();
    for row in rows {
        for draft in analyze_exchange(row) {
            hit_rows.push(PassiveHitRow {
                hit_id: generate_hit_id(),
                exchange_id: row.exchange_id.clone(),
                listener_id: row.listener_id.clone(),
                rule_id: draft.rule_id,
                rule_version: draft.rule_version as i64,
                severity: draft.severity,
                match_detail_json: draft.match_detail_json,
                created_at: now.clone(),
            });
        }
    }
    if hit_rows.is_empty() {
        return Ok(BatchInsertOutcome::default());
    }
    store.insert_passive_hits(&hit_rows)
}

/// 本模块结果别名（database::DbResult 直用别名，避免签名噪音）。
type DbResultOf<T> = Result<T, database::DbError>;

#[cfg(test)]
mod tests {
    use super::*;

    fn row_with(headers: &[(&str, &str)], status: i64, scheme: Option<&str>) -> ExchangeRow {
        let headers_json = serde_json::to_string(
            &headers
                .iter()
                .map(|(n, v)| serde_json::json!({"name": n, "value": v}))
                .collect::<Vec<_>>(),
        )
        .unwrap();
        ExchangeRow {
            exchange_id: "wsw-test-0-0-0001".into(),
            session_id: "wsw-ses-test-0001".into(),
            listener_id: "wsw-lis-test-0001".into(),
            scheme: scheme.map(|s| s.to_string()),
            authority: Some("127.0.0.1:8443".into()),
            response_status: Some(status),
            response_headers_json: Some(headers_json),
            ..Default::default()
        }
    }

    #[test]
    fn registry_word_table_matches_migration_check() {
        // 注册表词表 = 030 CHECK 词表逐字同源（迁移文件内嵌词表反证）。
        let sql = include_str!("../../../migrations/030_wsw_ws_frame.sql");
        for r in PASSIVE_RULES {
            assert!(
                sql.contains(&format!("'{r}'", r = r.id)),
                "030 缺规则词 {r}",
                r = r.id
            );
            assert!(SEVERITIES.contains(&r.severity), "severity 词表外");
            assert!(RULE_CLASSES.contains(&r.class), "class 词表外");
        }
        assert_eq!(PASSIVE_RULES.len(), 14);
    }

    #[test]
    fn missing_security_headers_and_cookie_attributes() {
        let row = row_with(&[("Set-Cookie", "sid=abc; Path=/")], 200, Some("https"));
        let hits = analyze_exchange(&row);
        let ids: Vec<&str> = hits.iter().map(|h| h.rule_id).collect();
        assert!(ids.contains(&"missing_header_strict_transport_security"));
        assert!(ids.contains(&"missing_header_content_security_policy"));
        assert!(ids.contains(&"missing_header_x_content_type_options"));
        assert!(ids.contains(&"missing_header_x_frame_options"));
        assert!(ids.contains(&"cookie_missing_secure"));
        assert!(ids.contains(&"cookie_missing_httponly"));
        assert!(ids.contains(&"cookie_missing_samesite"));
        assert!(ids.contains(&"config_set_cookie_cacheable"));
        // 明文响应不报 HSTS 缺失（无 TLS 事实不伪造）。
        let plain = row_with(&[], 200, Some("http"));
        assert!(!analyze_exchange(&plain)
            .iter()
            .any(|h| h.rule_id == "missing_header_strict_transport_security"));
    }

    #[test]
    fn cookie_attrs_present_means_no_cookie_hits() {
        let row = row_with(
            &[
                ("Set-Cookie", "sid=abc; Secure; HttpOnly; SameSite=Strict"),
                ("Cache-Control", "no-store"),
            ],
            200,
            Some("https"),
        );
        let ids: Vec<&str> = analyze_exchange(&row).iter().map(|h| h.rule_id).collect();
        assert!(!ids.contains(&"cookie_missing_secure"));
        assert!(!ids.contains(&"cookie_missing_httponly"));
        assert!(!ids.contains(&"cookie_missing_samesite"));
        assert!(!ids.contains(&"config_set_cookie_cacheable"));
    }

    #[test]
    fn info_leak_and_cors_and_redirects() {
        let leak = row_with(
            &[
                ("Server", "nginx/1.24.0"),
                ("X-Powered-By", "PHP/8.2"),
                ("Via", "1.1 proxy"),
            ],
            200,
            Some("http"),
        );
        let ids: Vec<&str> = analyze_exchange(&leak).iter().map(|h| h.rule_id).collect();
        assert!(ids.contains(&"info_leak_server_version"));
        assert!(ids.contains(&"info_leak_x_powered_by"));
        assert!(ids.contains(&"info_leak_via_header"));

        let cors = row_with(&[("Access-Control-Allow-Origin", "*")], 200, Some("http"));
        assert!(analyze_exchange(&cors)
            .iter()
            .any(|h| h.rule_id == "config_permissive_cors"));

        let cross = row_with(
            &[("Location", "https://evil.example.net/x")],
            302,
            Some("http"),
        );
        assert!(analyze_exchange(&cross)
            .iter()
            .any(|h| h.rule_id == "redirect_cross_origin"));
        // 同源相对跳转零命中。
        let same = row_with(&[("Location", "/next")], 302, Some("http"));
        assert!(!analyze_exchange(&same)
            .iter()
            .any(|h| h.rule_id == "redirect_cross_origin"));
        // https → http 降级跳转。
        let down = row_with(&[("Location", "http://127.0.0.1/x")], 302, Some("https"));
        assert!(analyze_exchange(&down)
            .iter()
            .any(|h| h.rule_id == "redirect_scheme_downgrade"));
    }

    #[test]
    fn request_side_rows_and_bad_headers_yield_zero_hits() {
        let mut no_response = row_with(&[], 200, Some("http"));
        no_response.response_status = None;
        assert!(analyze_exchange(&no_response).is_empty());
        let mut bad = row_with(&[], 200, Some("http"));
        bad.response_headers_json = Some("not-json".into());
        // 解析失败 = 零头 → 缺失类规则不伪造命中（headers_json 缺测如实）。
        assert!(!analyze_exchange(&bad)
            .iter()
            .any(|h| h.rule_id == "cookie_missing_secure"));
    }
}
