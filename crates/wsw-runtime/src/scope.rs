//! Scope 闸（WSW-2-ADR W2-D5；PLAN §7.2 :521-528 逐条机制化）。
//!
//! 闸只在 Rust 后端（前端校验一律不作数，PLAN :523）；闸序 =
//! scheme → host（合法性 → 云元数据 deny-never-allow → exclude 优先 →
//! include；include 空 = **default deny**）→ port → path → IP 审计。
//!
//! - 私网/回环/链路本地目标仅显式 `allow_private_targets=true` 放行且逐条
//!   留痕（本机靶场 127.0.0.1:8443 靠此面通过）；
//! - 云元数据段（169.254.169.254 / ::ffff:169.254.169.254 /
//!   metadata.google.internal / 169.254.170.2 等）deny-never-allow，
//!   任何 allow_private/include 组合不可放行（DESIGN §4.3 词表项原文）；
//! - DNS rebinding：域名串过闸 ≠ IP 过闸——解析 IP 全地址过
//!   [`audit_resolved`]（查点 C，`document_fetch.rs:460-514` 全地址复核
//!   移植），命中私网即拒并落 remote_ip；
//! - `max_redirects` 帽：逐跳校验入口 [`check_redirect_hop`]（查点 B），
//!   跳数超帽即拒（`document_fetch.rs:63-71` FetchPolicy 先例）。
//!
//! out-of-scope 命中策略（blocked=403 / passive_forward=放行留痕）属
//! wsw_session 行级配置（W2-D4），由调用面（listener::handler）按
//! [`ScopeDecision::Blocked`] 分支落账，本模块只产出闸判定本身。

use serde::{Deserialize, Serialize};
use std::net::{IpAddr, Ipv4Addr};

/// 云元数据段主机词表（DESIGN §4.3 deny-never-allow；小写比较，尾点归一）。
pub const METADATA_HOSTS: [&str; 5] = [
    "169.254.169.254",
    "::ffff:169.254.169.254",
    "metadata.google.internal",
    "169.254.170.2",
    "100.100.100.200", // Aliyun ECS metadata（DESIGN 词表「等」扩项）
];

/// 云元数据段 IP 集（mapped 归一后比对）。
const METADATA_IPS: [Ipv4Addr; 3] = [
    Ipv4Addr::new(169, 254, 169, 254),
    Ipv4Addr::new(169, 254, 170, 2),
    Ipv4Addr::new(100, 100, 100, 200),
];

/// IP 私网/保留判定：返回 Some(原因) 即拒绝
/// （`crates/tool-engine/src/document_fetch.rs:448-514` 全函数移植，语义一致）。
pub fn private_reason(ip: IpAddr) -> Option<&'static str> {
    match ip {
        IpAddr::V4(v4) => {
            let o = v4.octets();
            if v4.is_loopback() {
                Some("loopback")
            } else if v4.is_unspecified() || o[0] == 0 {
                Some("unspecified_or_reserved")
            } else if v4.is_private() {
                Some("rfc1918")
            } else if v4.is_link_local() {
                // 169.254.0.0/16：云 metadata 端点 169.254.169.254 属本段
                Some("link_local")
            } else if o[0] == 100 && (o[1] & 0b1100_0000) == 0b0100_0000 {
                Some("cgnat") // 100.64.0.0/10
            } else if v4.is_broadcast() {
                Some("broadcast")
            } else if v4.is_documentation() {
                Some("documentation")
            } else if o[0] == 198 && (o[1] == 18 || o[1] == 19) {
                Some("benchmark") // 198.18.0/15
            } else if o[0] >= 224 {
                Some("multicast_or_reserved")
            } else {
                None
            }
        }
        IpAddr::V6(v6) => {
            let seg = v6.segments();
            if v6.is_loopback() {
                Some("loopback")
            } else if v6.is_unspecified() {
                Some("unspecified")
            } else if (seg[0] & 0xffc0) == 0xfe80 {
                Some("link_local") // fe80::/10
            } else if (seg[0] & 0xfe00) == 0xfc00 {
                Some("ula") // fc00::/7
            } else if seg[0] == 0x2001 && seg[1] == 0x0db8 {
                Some("documentation") // 2001:db8::/32
            } else if (seg[0] & 0xff00) == 0xff00 {
                Some("multicast") // ff00::/8
            } else if seg[0] == 0x0064 && seg[1] == 0xff9b {
                Some("nat64") // 64:ff9b::/96 映射 v4，一律按内网处理
            } else if seg[0] == 0
                && seg[1] == 0
                && seg[2] == 0
                && seg[3] == 0
                && seg[4] == 0
                && seg[5] == 0xffff
            {
                // ::ffff:0:0/96 IPv4-mapped：按内嵌 v4 复核
                let v4 = Ipv4Addr::new(
                    (seg[6] >> 8) as u8,
                    (seg[6] & 0xff) as u8,
                    (seg[7] >> 8) as u8,
                    (seg[7] & 0xff) as u8,
                );
                private_reason(IpAddr::V4(v4)).map(|r| match r {
                    "rfc1918" => "rfc1918_mapped",
                    "link_local" => "link_local_mapped",
                    other => other,
                })
            } else {
                None
            }
        }
    }
}

/// 云元数据判定（deny-never-allow；host 词表 + mapped 归一 IP 双面）。
pub fn is_metadata_host(host: &str) -> bool {
    let h = host.trim_end_matches('.').to_ascii_lowercase();
    let stripped = h
        .strip_prefix('[')
        .and_then(|s| s.strip_suffix(']'))
        .unwrap_or(&h);
    METADATA_HOSTS.contains(&stripped)
}

pub fn is_metadata_ip(ip: IpAddr) -> bool {
    let normalized = match ip {
        IpAddr::V6(v6) => crate::authority::mapped_to_v4_or_v6(v6),
        IpAddr::V4(_) => ip,
    };
    match normalized {
        IpAddr::V4(v4) => METADATA_IPS.contains(&v4),
        IpAddr::V6(_) => false,
    }
}

/// Scope 规则（规则源 = `wsw_session.scope_rules_json`，PLAN :521 复用面；
/// 过渡期未配置 = default deny，DESIGN §4.1 原文）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScopeRules {
    /// 允许目标 host：小写精确匹配或 `*.suffix` 通配（exclude 优先）。
    #[serde(default)]
    pub include: Vec<String>,
    /// 拒绝目标 host（优先于 include；用于打洞收紧）。
    #[serde(default)]
    pub exclude: Vec<String>,
    /// 允许端口集；**空 = 不限端口**（端口面收口靠 include 命中，非本列表）。
    #[serde(default)]
    pub allowed_ports: Vec<u16>,
    /// 允许路径前缀；**空 = 不限路径**（闸序 path 位的表达式）。
    #[serde(default)]
    pub allowed_path_prefixes: Vec<String>,
    /// 显式放行私网/回环/链路本地目标（仍逐条留痕；云元数据不可放行）。
    #[serde(default)]
    pub allow_private_targets: bool,
    /// 重定向跳数帽（查点 B；document_fetch.rs:63-71 FetchPolicy 先例）。
    #[serde(default = "default_max_redirects")]
    pub max_redirects: u32,
}

fn default_max_redirects() -> u32 {
    10
}

impl Default for ScopeRules {
    fn default() -> Self {
        Self {
            include: Vec::new(),
            exclude: Vec::new(),
            allowed_ports: Vec::new(),
            allowed_path_prefixes: Vec::new(),
            allow_private_targets: false,
            max_redirects: default_max_redirects(),
        }
    }
}

impl ScopeRules {
    /// 从 `wsw_session.scope_rules_json` 解析。
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }
}

/// 单跳闸目标（查点 A：CONNECT host / 明文 authority；查点 B：Location 目标）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScopeTarget<'a> {
    /// scheme（http/https 之外一律拒）。
    pub scheme: &'a str,
    /// host（不含端口；IPv6 可带 `[]` 括号，统一归一）。
    pub host: &'a str,
    /// 端口。
    pub port: u16,
    /// 路径（无 = "/"）。
    pub path: &'a str,
}

/// 拒绝理由（结构化落账面：scope_decision 行 + 日志，PLAN :528 可见性）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScopeRejection {
    /// include 空 = default deny（未配置规则）。
    NoRules,
    /// host 非法（空/超长/控制字符）。
    InvalidHost(String),
    /// 云元数据段：任何策略不可放行（deny-never-allow）。
    MetadataEndpoint(String),
    /// exclude 命中（优先于 include）。
    HostExcluded(String),
    /// include 未命中。
    HostNotIncluded(String),
    /// 端口不在 allowed_ports。
    PortDenied(u16),
    /// 路径不在 allowed_path_prefixes。
    PathDenied(String),
    /// 私网/保留 IP 拒（IP 审计命中；remote_ip 落行 = DNS rebinding 面）。
    PrivateTarget {
        /// 命中 IP（mapped 归一后）。
        ip: IpAddr,
        /// private_reason 原因串。
        reason: String,
    },
    /// 重定向跳数超帽。
    RedirectLimit {
        /// 当前跳数。
        hop: u32,
        /// 帽值。
        cap: u32,
    },
    /// scheme 不在 {http, https}。
    SchemeDenied(String),
}

impl ScopeRejection {
    /// 机器可读码（日志/落账 reason 字段口径）。
    pub fn code(&self) -> &'static str {
        match self {
            ScopeRejection::NoRules => "no_rules_default_deny",
            ScopeRejection::InvalidHost(_) => "invalid_host",
            ScopeRejection::MetadataEndpoint(_) => "metadata_endpoint",
            ScopeRejection::HostExcluded(_) => "host_excluded",
            ScopeRejection::HostNotIncluded(_) => "host_not_included",
            ScopeRejection::PortDenied(_) => "port_denied",
            ScopeRejection::PathDenied(_) => "path_denied",
            ScopeRejection::PrivateTarget { .. } => "private_target",
            ScopeRejection::RedirectLimit { .. } => "redirect_limit",
            ScopeRejection::SchemeDenied(_) => "scheme_denied",
        }
    }
}

/// 放行留痕行（W2-D5「放行私网并逐条留痕」的表达式）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScopeTrace {
    /// 留痕码。
    pub kind: &'static str,
    /// 明细（含 host/ip/reason）。
    pub detail: String,
}

/// 闸判定。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScopeDecision {
    /// 放行（附留痕行；include 命中 + 私网显式放行各一条）。
    Allowed {
        /// 留痕行。
        traces: Vec<ScopeTrace>,
    },
    /// 拒（附结构化理由）。
    Blocked {
        /// 理由。
        reason: ScopeRejection,
    },
}

/// host 归一（小写 + IPv6 去括号 + 尾点归一）。
pub fn normalize_host(host: &str) -> String {
    let h = host.trim().trim_end_matches('.').to_ascii_lowercase();
    match (h.strip_prefix('['), h.strip_suffix(']')) {
        (Some(inner), Some(_)) => inner.to_string(),
        _ => h,
    }
}

/// host 模式匹配：小写精确或 `*.suffix` 通配（suffix 与点后缀均命中）。
fn host_matches(pattern: &str, host: &str) -> bool {
    let pattern = normalize_host(pattern);
    if let Some(suffix) = pattern.strip_prefix("*.") {
        // 通配仅命中子域（*.example → foo.example / a.b.example），不命中裸域。
        return host.ends_with(&format!(".{suffix}"));
    }
    pattern == host
}

/// 查点 A/B：单跳闸（连接前 CONNECT host / 明文 authority / Location 目标）。
/// 闸序 = scheme → host → port → path → IP 审计（host 为 IP 字面量时就地审；
/// 域名的解析 IP 由查点 C [`audit_resolved`] 复核——域名串过闸 ≠ IP 过闸）。
pub fn check(rules: &ScopeRules, target: &ScopeTarget<'_>) -> ScopeDecision {
    // 1) scheme
    if target.scheme != "http" && target.scheme != "https" {
        return ScopeDecision::Blocked {
            reason: ScopeRejection::SchemeDenied(target.scheme.to_string()),
        };
    }
    // 2) host：合法性 → 云元数据（deny-never-allow，先于一切放行面）→
    //    exclude 优先 → include（空 = default deny）
    let host = normalize_host(target.host);
    if crate::authority::validate_host(&host).is_err() {
        return ScopeDecision::Blocked {
            reason: ScopeRejection::InvalidHost(target.host.to_string()),
        };
    }
    if is_metadata_host(&host) {
        return ScopeDecision::Blocked {
            reason: ScopeRejection::MetadataEndpoint(target.host.to_string()),
        };
    }
    if rules.exclude.iter().any(|p| host_matches(p, &host)) {
        return ScopeDecision::Blocked {
            reason: ScopeRejection::HostExcluded(target.host.to_string()),
        };
    }
    if rules.include.is_empty() {
        return ScopeDecision::Blocked {
            reason: ScopeRejection::NoRules,
        };
    }
    if !rules.include.iter().any(|p| host_matches(p, &host)) {
        return ScopeDecision::Blocked {
            reason: ScopeRejection::HostNotIncluded(target.host.to_string()),
        };
    }
    // 3) port（空列表 = 不限）
    if !rules.allowed_ports.is_empty() && !rules.allowed_ports.contains(&target.port) {
        return ScopeDecision::Blocked {
            reason: ScopeRejection::PortDenied(target.port),
        };
    }
    // 4) path（空列表 = 不限）
    if !rules.allowed_path_prefixes.is_empty()
        && !rules
            .allowed_path_prefixes
            .iter()
            .any(|p| target.path.starts_with(p))
    {
        return ScopeDecision::Blocked {
            reason: ScopeRejection::PathDenied(target.path.to_string()),
        };
    }
    // 5) IP 审计（host 为 IP 字面量）
    if let Ok(ip) = host.parse::<IpAddr>() {
        let ip = match ip {
            IpAddr::V6(v6) => crate::authority::mapped_to_v4_or_v6(v6),
            IpAddr::V4(_) => ip,
        };
        if is_metadata_ip(ip) {
            return ScopeDecision::Blocked {
                reason: ScopeRejection::MetadataEndpoint(target.host.to_string()),
            };
        }
        if let Some(reason) = private_reason(ip) {
            if !rules.allow_private_targets {
                return ScopeDecision::Blocked {
                    reason: ScopeRejection::PrivateTarget {
                        ip,
                        reason: reason.to_string(),
                    },
                };
            }
            return ScopeDecision::Allowed {
                traces: vec![
                    ScopeTrace {
                        kind: "included",
                        detail: format!("host={} matched include rules", target.host),
                    },
                    ScopeTrace {
                        kind: "private_allowed",
                        detail: format!(
                            "ip={} reason={} (explicit allow_private_targets)",
                            ip, reason
                        ),
                    },
                ],
            };
        }
    }
    ScopeDecision::Allowed {
        traces: vec![ScopeTrace {
            kind: "included",
            detail: format!("host={} matched include rules", target.host),
        }],
    }
}

/// 查点 B 逐跳帽：`hop` = 该跳之前已跟随的重定向次数（0 = 初始请求）。
/// `hop > max_redirects` → `RedirectLimit` 拒；否则进入 [`check`] 同闸序。
pub fn check_redirect_hop(
    rules: &ScopeRules,
    target: &ScopeTarget<'_>,
    hop: u32,
) -> ScopeDecision {
    if hop > rules.max_redirects {
        return ScopeDecision::Blocked {
            reason: ScopeRejection::RedirectLimit {
                hop,
                cap: rules.max_redirects,
            },
        };
    }
    check(rules, target)
}

/// 查点 C：解析 IP 审计（上游 DNS 解析完成、TCP 连接前；全地址复核 =
/// `document_fetch.rs:460-514` 移植——任一解析地址命中私网/保留段即拒）。
///
/// - `resolved` 为该域名本次解析得到的全部 IP；
/// - 云元数据 IP：deny-never-allow，allow_private 也不放行；
/// - 私网命中：无 allow_private → 拒（remote_ip 逐条返回，DNS rebinding 面）；
///   有 allow_private → 放行 + 逐条留痕；
/// - 公网 IP：不计私网，放行不带私网留痕。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IpRejection {
    /// 被拒 IP（mapped 归一后）。
    pub ip: IpAddr,
    /// private_reason 原因串。
    pub reason: String,
}

pub fn audit_resolved(
    rules: &ScopeRules,
    resolved: &[IpAddr],
) -> Result<Vec<ScopeTrace>, Vec<IpRejection>> {
    let mut traces = Vec::new();
    let mut rejections = Vec::new();
    for raw in resolved {
        let ip = match *raw {
            IpAddr::V6(v6) => crate::authority::mapped_to_v4_or_v6(v6),
            IpAddr::V4(_) => *raw,
        };
        if is_metadata_ip(ip) {
            rejections.push(IpRejection {
                ip,
                reason: "metadata_endpoint".to_string(),
            });
            continue;
        }
        if let Some(reason) = private_reason(ip) {
            if rules.allow_private_targets {
                traces.push(ScopeTrace {
                    kind: "private_allowed",
                    detail: format!("ip={} reason={} (explicit allow_private_targets)", ip, reason),
                });
            } else {
                rejections.push(IpRejection {
                    ip,
                    reason: reason.to_string(),
                });
            }
        }
    }
    if rejections.is_empty() {
        Ok(traces)
    } else {
        Err(rejections)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target<'a>(scheme: &'a str, host: &'a str, port: u16, path: &'a str) -> ScopeTarget<'a> {
        ScopeTarget {
            scheme,
            host,
            port,
            path,
        }
    }

    fn rules_with(include: &[&str]) -> ScopeRules {
        ScopeRules {
            include: include.iter().map(|s| s.to_string()).collect(),
            ..Default::default()
        }
    }

    #[test]
    fn include_hit_allows() {
        let mut rules = rules_with(&["127.0.0.1"]);
        rules.allowed_ports = vec![8443];
        rules.allow_private_targets = true;
        let d = check(&rules, &target("http", "127.0.0.1", 8443, "/p59c-marker"));
        let ScopeDecision::Allowed { traces } = d else {
            panic!("expected allowed, got {d:?}");
        };
        // 留痕：include 命中 + 私网显式放行各一条（W2-D5「放行留痕」）。
        let kinds: Vec<&str> = traces.iter().map(|t| t.kind).collect();
        assert!(kinds.contains(&"included") && kinds.contains(&"private_allowed"));
    }

    #[test]
    fn no_rules_default_deny() {
        let rules = ScopeRules::default();
        assert_eq!(
            check(&rules, &target("http", "example.com", 80, "/")),
            ScopeDecision::Blocked {
                reason: ScopeRejection::NoRules
            }
        );
    }

    #[test]
    fn include_miss_denies() {
        let rules = rules_with(&["127.0.0.1"]);
        assert!(matches!(
            check(&rules, &target("http", "example.com", 80, "/")),
            ScopeDecision::Blocked {
                reason: ScopeRejection::HostNotIncluded(_)
            }
        ));
    }

    #[test]
    fn exclude_wins_over_include() {
        let mut rules = rules_with(&["*.example", "api.example"]);
        rules.exclude = vec!["api.example".to_string()];
        assert!(matches!(
            check(&rules, &target("https", "api.example", 443, "/")),
            ScopeDecision::Blocked {
                reason: ScopeRejection::HostExcluded(_)
            }
        ));
        // 通配命中仍在（include *.example 放 web.example）。
        assert!(matches!(
            check(&rules, &target("https", "web.example", 443, "/")),
            ScopeDecision::Allowed { .. }
        ));
    }

    #[test]
    fn wildcard_matches_subdomains_only() {
        let rules = rules_with(&["*.example"]);
        assert!(matches!(
            check(&rules, &target("https", "a.b.example", 443, "/")),
            ScopeDecision::Allowed { .. }
        ));
        assert!(matches!(
            check(&rules, &target("https", "example", 443, "/")),
            ScopeDecision::Blocked {
                reason: ScopeRejection::HostNotIncluded(_)
            }
        ));
    }

    #[test]
    fn port_mismatch_denies() {
        let mut rules = rules_with(&["127.0.0.1"]);
        rules.allowed_ports = vec![8443];
        rules.allow_private_targets = true;
        assert!(matches!(
            check(&rules, &target("http", "127.0.0.1", 9000, "/")),
            ScopeDecision::Blocked {
                reason: ScopeRejection::PortDenied(9000)
            }
        ));
    }

    #[test]
    fn scheme_denied() {
        let rules = rules_with(&["127.0.0.1"]);
        assert!(matches!(
            check(&rules, &target("ftp", "127.0.0.1", 21, "/")),
            ScopeDecision::Blocked {
                reason: ScopeRejection::SchemeDenied(_)
            }
        ));
    }

    #[test]
    fn private_denied_without_allow_flag() {
        let rules = rules_with(&["127.0.0.1"]);
        match check(&rules, &target("http", "127.0.0.1", 8443, "/")) {
            ScopeDecision::Blocked {
                reason:
                    ScopeRejection::PrivateTarget {
                        ip,
                        reason,
                    },
            } => {
                assert_eq!(ip, IpAddr::V4(Ipv4Addr::LOCALHOST));
                assert_eq!(reason, "loopback");
            }
            other => panic!("expected private blocked, got {other:?}"),
        }
    }

    #[test]
    fn metadata_denied_even_with_allow_private() {
        // 云元数据 deny-never-allow：allow_private=true + include 命中仍拒
        //（reason=metadata_endpoint；W2-D5 / DESIGN §4.3 词表项原文）。
        let mut rules = rules_with(&["169.254.169.254", "metadata.google.internal"]);
        rules.allow_private_targets = true;
        for host in ["169.254.169.254", "metadata.google.internal", "::ffff:169.254.169.254"] {
            assert!(matches!(
                check(&rules, &target("http", host, 80, "/latest/meta-data/")),
                ScopeDecision::Blocked {
                    reason: ScopeRejection::MetadataEndpoint(_)
                }
            ));
        }
    }

    #[test]
    fn mapped_host_judged_as_v4_private() {
        // ::ffff:127.0.0.1 目标 → 按 V4 私网判定（归一后 loopback），而非 v6 面。
        let rules = rules_with(&["::ffff:127.0.0.1"]);
        match check(&rules, &target("http", "::ffff:127.0.0.1", 8443, "/")) {
            ScopeDecision::Blocked {
                reason: ScopeRejection::PrivateTarget { ip, reason },
            } => {
                assert_eq!(ip, IpAddr::V4(Ipv4Addr::LOCALHOST));
                assert_eq!(reason, "loopback");
            }
            other => panic!("expected mapped->v4 blocked, got {other:?}"),
        }
    }

    #[test]
    fn redirect_cap_enforced() {
        let mut rules = rules_with(&["127.0.0.1"]);
        rules.allow_private_targets = true;
        rules.max_redirects = 2;
        // hop 0/2 内放行，hop 3 超帽拒。
        assert!(matches!(
            check_redirect_hop(&rules, &target("http", "127.0.0.1", 8443, "/"), 0),
            ScopeDecision::Allowed { .. }
        ));
        assert!(matches!(
            check_redirect_hop(&rules, &target("http", "127.0.0.1", 8443, "/"), 2),
            ScopeDecision::Allowed { .. }
        ));
        assert!(matches!(
            check_redirect_hop(&rules, &target("http", "127.0.0.1", 8443, "/"), 3),
            ScopeDecision::Blocked {
                reason: ScopeRejection::RedirectLimit { hop: 3, cap: 2 }
            }
        ));
        // 越界重定向第二跳（Location 指向 include 外 host）按同闸序拒。
        assert!(matches!(
            check_redirect_hop(&rules, &target("http", "out-of-range.example", 80, "/"), 1),
            ScopeDecision::Blocked {
                reason: ScopeRejection::HostNotIncluded(_)
            }
        ));
    }

    #[test]
    fn audit_resolved_rebinding_denied_with_remote_ip() {
        // DNS rebinding 面：域名串过闸（include 命中）但解析 IP 落私网 →
        // 查点 C 拒 + remote_ip 逐条落行。
        let mut rules = rules_with(&["rebinding.test"]);
        rules.allow_private_targets = false;
        let resolved = vec![
            "93.184.216.34".parse::<IpAddr>().unwrap(),
            "10.0.0.5".parse::<IpAddr>().unwrap(),
        ];
        let err = audit_resolved(&rules, &resolved).expect_err("must reject private resolved ip");
        assert_eq!(err.len(), 1);
        assert_eq!(err[0].ip, IpAddr::V4(Ipv4Addr::new(10, 0, 0, 5)));
        assert_eq!(err[0].reason, "rfc1918");
    }

    #[test]
    fn audit_resolved_metadata_ip_denied_with_allow_private() {
        let mut rules = rules_with(&["169.254.169.254"]);
        rules.allow_private_targets = true;
        let resolved = vec!["169.254.169.254".parse::<IpAddr>().unwrap()];
        let err = audit_resolved(&rules, &resolved).expect_err("metadata deny-never-allow");
        assert_eq!(err[0].reason, "metadata_endpoint");
    }

    #[test]
    fn audit_resolved_allows_public_and_private_with_allow() {
        let mut rules = rules_with(&["local-range.test"]);
        rules.allow_private_targets = true;
        let resolved = vec![
            "8.8.8.8".parse::<IpAddr>().unwrap(),
            "127.0.0.1".parse::<IpAddr>().unwrap(),
        ];
        let traces = audit_resolved(&rules, &resolved).expect("allow_private passes with trace");
        assert_eq!(traces.len(), 1);
        assert!(traces[0].detail.contains("127.0.0.1"));
    }

    #[test]
    fn private_reason_mapped_branch() {
        // mapped 私网：rfc1918_mapped / link_local_mapped 归一分支。
        assert_eq!(
            private_reason("::ffff:10.1.2.3".parse().unwrap()),
            Some("rfc1918_mapped")
        );
        assert_eq!(
            private_reason("::ffff:169.254.1.2".parse().unwrap()),
            Some("link_local_mapped")
        );
        assert_eq!(private_reason("::ffff:8.8.8.8".parse().unwrap()), None);
        assert_eq!(private_reason("64:ff9b::1.2.3.4".parse().unwrap()), Some("nat64"));
    }

    #[test]
    fn rules_json_roundtrip() {
        let mut rules = rules_with(&["127.0.0.1"]);
        rules.allowed_ports = vec![8443];
        rules.allow_private_targets = true;
        rules.max_redirects = 3;
        let json = serde_json::to_string(&rules).unwrap();
        let back = ScopeRules::from_json(&json).unwrap();
        assert_eq!(rules, back);
    }
}
