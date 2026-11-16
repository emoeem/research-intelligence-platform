//! Knowledge Matcher（RIP3 Phase 5 P5-3）：从知识资产里筛选可用于当前目标
//! 的候选检测，输出**命中与排除原因**（Knowledge Context 页解释面数据源）。
//!
//! 检索纪律（补充计划 §5.4 硬约束）：
//! - 3.0 知识面只走 Phase 4 统一检索（[`UnifiedSearchChannel`] 经 rp-kc
//!   `search --mode hybrid` 入口，pg_search BM25 + pgvector + RRF 融合 +
//!   双缓存），**禁止绕过统一检索直接遍历知识库**；
//! - RTIS POC 面走 rt.py `poc search --json` 元数据面（RTIS 既有接口，
//!   integrations/rtis 零修改；只取元数据字段，不取正文）；
//! - 向量/相关度分数只是候选排序信息，**最终适用性由结构化条件判定**
//!   （组件名匹配 + 版本约束 + 验证状态 + 组件锚存在性），每条命中/排除
//!   必须带可解释原因；
//! - 渠道失败降级为空渠道并如实标注（unified-search sqlite_error 同款
//!   fail-open 口径），单渠道不可用不阻断整体匹配；
//! - 坑位继承（HANDOVER 第七节）：RTIS poc search 中文零命中，组件名用
//!   英文（kingdee 有命中、金蝶没有），本模块不做中文转写（调用方画像
//!   输入负责）。
//!
//! 假设无命中：全部候选被排除或渠道零返回时，[`MatchResult::has_applicable`]
//! 为 false，调用方（P5-8 自适应研究）据此进入补充研究状态而非报告安全。

use serde::Serialize;
use serde_json::Value;
use std::path::PathBuf;
use std::process::Command;
use std::time::Instant;

/// 匹配输入（目标 + 技术画像 + 过滤）。
#[derive(Debug, Clone)]
pub struct MatchQuery {
    /// 规范化目标（展示与后续 Planner 目标锚）。
    pub target_ref: String,
    /// 技术画像组件名（英文优先；空 = 无结构化锚，全部候选按通用知识处理）。
    pub components: Vec<String>,
    /// 组件版本（component -> version；缺版本的条件记为未验证）。
    pub profile_versions: std::collections::HashMap<String, String>,
    /// 每渠道候选上限。
    pub limit: usize,
}

/// 渠道返回的单条候选（元数据面，两侧渠道统一形状）。
#[derive(Debug, Clone)]
pub struct ChannelHit {
    /// 渠道名：rtis-poc | pg-intel（unified-search）。
    pub channel: &'static str,
    /// 引用锚：RTIS POC 数字 ID 字符串 或 knowledge_id。
    pub ref_id: String,
    pub title: String,
    pub component: Option<String>,
    pub versions: Option<String>,
    pub severity: Option<String>,
    /// RTIS verified 位（pg-intel 渠道取审核态 APPROVED）。
    pub verified: bool,
    /// 渠道原始相关度分（展示用；不参与适用性判定）。
    pub score: f64,
}

/// 渠道抽象（生产两个实现 + 测试 mock）。
pub trait KnowledgeChannel {
    /// 按查询词拉候选（实现自行映射到统一检索/RTIS 接口）。
    fn fetch(&self, query: &str, limit: usize) -> Result<Vec<ChannelHit>, String>;
    /// 渠道名（结果标注）。
    fn name(&self) -> &'static str;
}

/// RTIS POC 元数据渠道（python3 rt.py poc search --q X --json）。
pub struct RtisPocChannel {
    /// rt.py 绝对路径（缺省仓库 integrations/rtis/rt.py）。
    pub rt_py: PathBuf,
    /// python 解释器（缺省 python3）。
    pub python: String,
}

impl RtisPocChannel {
    pub fn from_repo_root(root: &std::path::Path) -> Self {
        Self {
            rt_py: root.join("integrations/rtis/rt.py"),
            python: "python3".to_owned(),
        }
    }
}

impl KnowledgeChannel for RtisPocChannel {
    fn fetch(&self, query: &str, limit: usize) -> Result<Vec<ChannelHit>, String> {
        let output = Command::new(&self.python)
            .arg(&self.rt_py)
            .args(["poc", "search", "--q", query, "--json", "--limit", &limit.to_string()])
            .output()
            .map_err(|e| format!("rt.py spawn failed: {e}"))?;
        if !output.status.success() {
            return Err(format!(
                "rt.py poc search exit {}: {}",
                output.status,
                String::from_utf8_lossy(&output.stderr).lines().next().unwrap_or("")
            ));
        }
        let payload: Value =
            serde_json::from_slice(&output.stdout).map_err(|e| format!("rt.py json: {e}"))?;
        let rows = payload
            .get("results")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        Ok(rows
            .into_iter()
            .map(|row| ChannelHit {
                channel: "rtis-poc",
                ref_id: row
                    .get("id")
                    .and_then(Value::as_i64)
                    .map(|id| id.to_string())
                    .unwrap_or_default(),
                title: row
                    .get("title")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned(),
                component: row
                    .get("component")
                    .and_then(Value::as_str)
                    .map(str::to_owned),
                versions: row.get("versions").and_then(Value::as_str).map(str::to_owned),
                severity: row.get("severity").and_then(Value::as_str).map(str::to_owned),
                verified: row.get("verified").and_then(Value::as_i64).unwrap_or(0) == 1,
                score: row.get("hit_count").and_then(Value::as_f64).unwrap_or(0.0),
            })
            .collect())
    }

    fn name(&self) -> &'static str {
        "rtis-poc"
    }
}

/// Phase 4 统一检索渠道（rp-kc search --mode hybrid，JSON stdout 行数组）。
pub struct UnifiedSearchChannel {
    pub rp_kc: PathBuf,
}

impl KnowledgeChannel for UnifiedSearchChannel {
    fn fetch(&self, query: &str, limit: usize) -> Result<Vec<ChannelHit>, String> {
        let output = Command::new(&self.rp_kc)
            .args([
                "search",
                "--query",
                query,
                "--mode",
                "hybrid",
                "--limit",
                &limit.to_string(),
            ])
            .output()
            .map_err(|e| format!("rp-kc spawn failed: {e}"))?;
        if !output.status.success() {
            return Err(format!(
                "rp-kc search exit {}: {}",
                output.status,
                String::from_utf8_lossy(&output.stderr).lines().next().unwrap_or("")
            ));
        }
        let text = String::from_utf8_lossy(&output.stdout);
        let rows: Vec<Value> = serde_json::from_str(text.trim())
            .map_err(|e| format!("rp-kc json: {e}"))?;
        Ok(rows
            .into_iter()
            .map(|row| ChannelHit {
                channel: "pg-intel",
                ref_id: row
                    .get("knowledge_id")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned(),
                title: row
                    .get("title")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned(),
                component: None,
                versions: None,
                severity: None,
                // 统一检索命中默认按审核面已放行处理（rp-kc search 只出
                // APPROVED 面）；来源标注 pg-intel。
                verified: true,
                score: row.get("score").and_then(Value::as_f64).unwrap_or(0.0),
            })
            .collect())
    }

    fn name(&self) -> &'static str {
        "pg-intel"
    }
}

/// 命中（结构化条件判定通过，原因可解释）。
#[derive(Debug, Clone, Serialize)]
pub struct MatchHit {
    pub channel: &'static str,
    pub ref_id: String,
    pub title: String,
    pub component: Option<String>,
    pub versions: Option<String>,
    pub severity: Option<String>,
    pub verified: bool,
    pub channel_score: f64,
    /// 命中原因（每条可解释：组件匹配/版本匹配/已验证/通用知识）。
    pub match_reasons: Vec<String>,
}

/// 排除候选（带排除原因；Knowledge Context 页解释面）。
#[derive(Debug, Clone, Serialize)]
pub struct MatchExcluded {
    pub channel: &'static str,
    pub ref_id: String,
    pub title: String,
    pub component: Option<String>,
    pub versions: Option<String>,
    /// 排除原因（组件不匹配/版本不匹配/无组件锚）。
    pub exclude_reasons: Vec<String>,
}

/// 单渠道统计（延迟与错误如实标注）。
#[derive(Debug, Clone, Serialize)]
pub struct ChannelStat {
    pub channel: &'static str,
    pub query: String,
    pub returned: usize,
    pub latency_ms: u128,
    pub error: Option<String>,
}

/// 匹配结果。
#[derive(Debug, Clone, Serialize)]
pub struct MatchResult {
    pub target_ref: String,
    pub components: Vec<String>,
    pub hits: Vec<MatchHit>,
    pub excluded: Vec<MatchExcluded>,
    pub channels: Vec<ChannelStat>,
    /// 是否存在适用候选（false = 进入补充研究状态，P5-8 消费）。
    pub has_applicable: bool,
    /// 总耗时（毫秒）。
    pub elapsed_ms: u128,
}

/// 规范化组件名（小写 + 去空白；匹配口径与 RTIS 检索同源英文优先）。
fn normalize_component(value: &str) -> String {
    value.trim().to_lowercase()
}

/// 单渠道匹配（结构化条件判定，原因逐条可解释）。
fn match_channel(
    channel: &dyn KnowledgeChannel,
    query: &MatchQuery,
    per_query_limit: usize,
    stats: &mut Vec<ChannelStat>,
) -> (Vec<MatchHit>, Vec<MatchExcluded>) {
    let mut hits = Vec::new();
    let mut excluded = Vec::new();
    // 查询词 = 目标组件拼接（英文画像；多组件一次查询交给统一检索排序，
    // 精确匹配在结构化判定层做）。
    let query_text = if query.components.is_empty() {
        query.target_ref.clone()
    } else {
        query.components.join(" ")
    };
    let started = Instant::now();
    let result = channel.fetch(&query_text, per_query_limit);
    let latency = started.elapsed().as_millis();
    match result {
        Ok(rows) => {
            for hit in rows {
                let normalized_hit = hit.component.as_deref().map(normalize_component);
                let matched = query
                    .components
                    .iter()
                    .map(|c| normalize_component(c))
                    .find(|wanted| {
                        normalized_hit
                            .as_ref()
                            .map(|hc| hc.contains(wanted.as_str()) || wanted.contains(hc.as_str()))
                            .unwrap_or(false)
                    });
                match (matched, normalized_hit) {
                    (Some(wanted), Some(hit_component)) => {
                        let mut reasons = vec![format!(
                            "component match: profile [{wanted}] vs knowledge [{hit_component}]"
                        )];
                        let knowledge_versions = hit.versions.clone();
                        // 版本条件：画像有版本且知识带版本 → 一致性判定；
                        // 缺任一侧 → 记为未验证条件（不静默通过）。
                        let profile_version = query
                            .profile_versions
                            .iter()
                            .find(|(k, _)| normalize_component(k) == wanted)
                            .map(|(_, v)| v.clone());
                        match (&profile_version, knowledge_versions.as_deref()) {
                            (Some(pv), Some(kv)) => {
                                if kv.contains(pv.as_str()) || pv.contains(kv) {
                                    reasons.push(format!("version match: {pv}"));
                                } else {
                                    excluded.push(MatchExcluded {
                                        channel: hit.channel,
                                        ref_id: hit.ref_id,
                                        title: hit.title,
                                        component: hit.component,
                                        versions: hit.versions,
                                        exclude_reasons: vec![format!(
                                            "version mismatch: profile [{pv}] vs knowledge [{kv}]"
                                        )],
                                    });
                                    continue;
                                }
                            }
                            (pv, kv) => {
                                let missing = match (pv, kv) {
                                    (Some(_), None) => "knowledge version missing",
                                    (None, Some(_)) => "profile version missing",
                                    (None, None) => "both versions missing",
                                    _ => unreachable!(),
                                };
                                reasons.push(format!("version condition unverified ({missing})"));
                            }
                        }
                        if hit.verified {
                            reasons.push("RTIS verified".to_owned());
                        }
                        hits.push(MatchHit {
                            channel: hit.channel,
                            ref_id: hit.ref_id,
                            title: hit.title,
                            component: hit.component,
                            versions: hit.versions,
                            severity: hit.severity,
                            verified: hit.verified,
                            channel_score: hit.score,
                            match_reasons: reasons,
                        });
                    }
                    // matched=Some 必然伴随 normalized_hit=Some（matched 从其派生）；
                    // 该分支不可达，防御式排除并标注内部不变量破坏。
                    (Some(_), None) => {
                        excluded.push(MatchExcluded {
                            channel: hit.channel,
                            ref_id: hit.ref_id,
                            title: hit.title,
                            component: hit.component.clone(),
                            versions: hit.versions.clone(),
                            exclude_reasons: vec!["internal invariant broken (matched without component anchor)".to_owned()],
                        });
                    }
                    (None, Some(hit_component_value)) => {
                        // 知识带组件锚但画像没有该组件：明确排除（不凭标题相似度放行）。
                        excluded.push(MatchExcluded {
                            channel: hit.channel,
                            ref_id: hit.ref_id,
                            title: hit.title,
                            component: hit.component.clone(),
                            versions: hit.versions.clone(),
                            exclude_reasons: vec![format!(
                                "component not in target profile: [{hit_component_value}]"
                            )],
                        });
                    }
                    (None, None) => {
                        if query.components.is_empty() {
                            // 无画像输入：全部候选按通用知识放行（原因明示）。
                            hits.push(MatchHit {
                                channel: hit.channel,
                                ref_id: hit.ref_id,
                                title: hit.title,
                                component: None,
                                versions: hit.versions.clone(),
                                severity: hit.severity,
                                verified: hit.verified,
                                channel_score: hit.score,
                                match_reasons: vec![
                                    "generic knowledge (no component profile provided)"
                                        .to_owned(),
                                ],
                            });
                        } else {
                            excluded.push(MatchExcluded {
                                channel: hit.channel,
                                ref_id: hit.ref_id,
                                title: hit.title,
                                component: None,
                                versions: hit.versions.clone(),
                                exclude_reasons: vec![
                                    "no component anchor (cannot verify applicability)"
                                        .to_owned(),
                                ],
                            });
                        }
                    }
                }
            }
            stats.push(ChannelStat {
                channel: channel.name(),
                query: query_text,
                returned: hits.len() + excluded.len(),
                latency_ms: latency,
                error: None,
            });
        }
        Err(error) => {
            // 渠道失败降级：空渠道如实标注（fail-open，不阻断整体）。
            stats.push(ChannelStat {
                channel: channel.name(),
                query: query_text,
                returned: 0,
                latency_ms: latency,
                error: Some(error),
            });
        }
    }
    (hits, excluded)
}

/// 执行匹配（多渠道合并；渠道顺序即输出顺序）。
pub fn run_match(
    channels: &[&dyn KnowledgeChannel],
    query: &MatchQuery,
    per_query_limit: usize,
) -> MatchResult {
    let started = Instant::now();
    let mut hits = Vec::new();
    let mut excluded = Vec::new();
    let mut stats = Vec::new();
    for &channel in channels {
        let (channel_hits, channel_excluded) =
            match_channel(channel, query, per_query_limit, &mut stats);
        hits.extend(channel_hits);
        excluded.extend(channel_excluded);
    }
    let has_applicable = !hits.is_empty();
    MatchResult {
        target_ref: query.target_ref.clone(),
        components: query.components.clone(),
        hits,
        excluded,
        channels: stats,
        has_applicable,
        elapsed_ms: started.elapsed().as_millis(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    struct MockChannel {
        rows: Vec<ChannelHit>,
        fail: bool,
    }

    impl KnowledgeChannel for MockChannel {
        fn fetch(&self, _query: &str, limit: usize) -> Result<Vec<ChannelHit>, String> {
            if self.fail {
                return Err("channel down".to_owned());
            }
            Ok(self.rows.iter().take(limit).cloned().collect())
        }
        fn name(&self) -> &'static str {
            "mock"
        }
    }

    fn hit(component: Option<&str>, versions: Option<&str>, verified: bool) -> ChannelHit {
        ChannelHit {
            channel: "mock",
            ref_id: "900008".to_owned(),
            title: "K3Cloud ServiceFacade 判读".to_owned(),
            component: component.map(str::to_owned),
            versions: versions.map(str::to_owned),
            severity: Some("high".to_owned()),
            verified,
            score: 3.0,
        }
    }

    #[test]
    fn component_and_version_match_with_reasons() {
        let mut versions = HashMap::new();
        versions.insert("kingdee k3cloud".to_owned(), "8.2.886".to_owned());
        let query = MatchQuery {
            target_ref: "https://target.example".to_owned(),
            components: vec!["Kingdee K3Cloud".to_owned()],
            profile_versions: versions,
            limit: 10,
        };
        let channel = MockChannel {
            rows: vec![hit(Some("Kingdee K3Cloud ServiceFacade"), Some("8.2"), true)],
            fail: false,
        };
        let result = run_match(&[&channel], &query, 10);
        assert!(result.has_applicable);
        assert_eq!(result.hits.len(), 1);
        let reasons = &result.hits[0].match_reasons;
        assert!(reasons.iter().any(|r| r.starts_with("component match")));
        assert!(reasons.iter().any(|r| r.starts_with("version match")));
        assert!(reasons.iter().any(|r| r == "RTIS verified"));
        assert!(result.excluded.is_empty());
    }

    #[test]
    fn version_mismatch_excluded_with_reason() {
        let mut versions = HashMap::new();
        versions.insert("kingdee".to_owned(), "9.0".to_owned());
        let query = MatchQuery {
            target_ref: "https://target.example".to_owned(),
            components: vec!["kingdee".to_owned()],
            profile_versions: versions,
            limit: 10,
        };
        let channel = MockChannel {
            rows: vec![hit(Some("kingdee k3cloud"), Some("8.2"), false)],
            fail: false,
        };
        let result = run_match(&[&channel], &query, 10);
        assert!(!result.has_applicable);
        assert_eq!(result.hits.len(), 0);
        assert_eq!(result.excluded.len(), 1);
        assert!(result.excluded[0].exclude_reasons[0].starts_with("version mismatch"));
    }

    #[test]
    fn missing_version_recorded_as_unverified_not_silent_pass() {
        let mut versions = HashMap::new();
        versions.insert("kingdee".to_owned(), "8.2".to_owned());
        let query = MatchQuery {
            target_ref: "t".to_owned(),
            components: vec!["kingdee".to_owned()],
            profile_versions: versions,
            limit: 10,
        };
        let channel = MockChannel {
            rows: vec![hit(Some("kingdee"), None, false)],
            fail: false,
        };
        let result = run_match(&[&channel], &query, 10);
        assert!(result.has_applicable);
        assert!(result.hits[0]
            .match_reasons
            .iter()
            .any(|r| r.starts_with("version condition unverified")));
    }

    #[test]
    fn no_anchor_excluded_when_profile_present() {
        let query = MatchQuery {
            target_ref: "t".to_owned(),
            components: vec!["nginx".to_owned()],
            profile_versions: HashMap::new(),
            limit: 10,
        };
        let channel = MockChannel {
            rows: vec![hit(None, None, false)],
            fail: false,
        };
        let result = run_match(&[&channel], &query, 10);
        assert!(!result.has_applicable);
        assert_eq!(result.excluded.len(), 1);
        assert_eq!(
            result.excluded[0].exclude_reasons[0],
            "no component anchor (cannot verify applicability)"
        );
    }

    #[test]
    fn generic_hits_when_no_profile() {
        let query = MatchQuery {
            target_ref: "https://t.example".to_owned(),
            components: vec![],
            profile_versions: HashMap::new(),
            limit: 10,
        };
        let channel = MockChannel {
            rows: vec![hit(None, None, false)],
            fail: false,
        };
        let result = run_match(&[&channel], &query, 10);
        assert!(result.has_applicable);
        assert_eq!(result.hits[0].match_reasons[0], "generic knowledge (no component profile provided)");
    }

    #[test]
    fn channel_failure_degrades_with_stat() {
        let query = MatchQuery {
            target_ref: "t".to_owned(),
            components: vec![],
            profile_versions: HashMap::new(),
            limit: 10,
        };
        let failing = MockChannel {
            rows: vec![],
            fail: true,
        };
        let result = run_match(&[&failing], &query, 10);
        assert!(!result.has_applicable);
        assert_eq!(result.channels.len(), 1);
        assert_eq!(result.channels[0].error.as_deref(), Some("channel down"));
    }
}
