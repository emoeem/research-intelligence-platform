//! Exchange 结构（DESIGN §1.2 全字段；PLAN §6.3 :484-493）。
//!
//! 字段冻结对照（DESIGN §1.2 表逐行）：
//! | 表行 | 字段 |
//! |---|---|
//! | exchange_id | [`Exchange::exchange_id`] |
//! | session_id / listener_id / sequence_no | 三字段 |
//! | direction / source | [`Direction`]/[`MessageSource`] |
//! | protocol / http_version / tls_version / alpn | 四字段（小写字符串） |
//! | method / scheme / authority / path | 请求面四元组（authority 含 host:port） |
//! | request_headers / response_headers | 头序保持数组（name,value） |
//! | request_body_artifact / response_body_artifact | [`BodyRef`] CAS 引用 |
//! | response_status | u16 |
//! | started_at / completed_at / duration_ms / ttfb_ms | 时间面 |
//! | remote_ip | IpAddr（连接实际对端，DNS rebinding 审计锚 §4.2） |
//! | scope_decision | [`ScopeDecision`]；**None（NULL）= Unknown**（Scope
//!   规则未命中第三态，词表不设 unknown 值，DESIGN §1.2 明文） |
//! | edited / tags / notes | 三字段 |
//! | trace_id / task_id / tool_run_id / artifact_id / evidence_id | 可空，六 ID
//!   引用 domain 再导出面（校验经 [`Exchange::validate_correlation_ids`]） |
//! | capture_fidelity | [`CaptureFidelity`] |
//! | parse_status / error_code | [`ParseStatus`]/[`ErrorCode`] |
//!
//! 关联 ID 以 String 承载（domain 六 ID 类型无 serde 派生，且域层不动 ——
//! WSW-1 文件清单不含 domain 修改）；写入路径经
//! [`Exchange::validate_correlation_ids`] 走 domain::ids 校验面。

use crate::cas::BodyRef;
use crate::ids::{ExchangeId, ListenerId, SessionId};
use crate::types::{CaptureFidelity, ErrorCode, HeaderRecord, ParseStatus};
use serde::{Deserialize, Serialize};

/// 方向（消息对面；DESIGN §1.2 direction 行）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    /// client → server。
    ClientToServer,
    /// server → client。
    ServerToClient,
}

/// 消息来源（DESIGN §1.2 source 行；PLAN §8.3 :594「导入数据必须
/// source=import，不能伪装成真实捕获」）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageSource {
    /// MITM 代理捕获（WSW-2）。
    ProxyCapture,
    /// Repeater 重放（WSW-3）。
    Repeater,
    /// Fuzzer（WSW-3）。
    Fuzzer,
    /// 导入（HAR 等，WSW-3/6）。
    Import,
}

/// Scope 判定词表（DESIGN §1.2 scope_decision 行 + PLAN §7.2 :528）。
///
/// 第三态 Unknown 由 **NULL（None）** 表达（Scope 规则未命中），词表不设
/// unknown 值；UI 展示层把 None 渲染为 Unknown（DESIGN §1.2 明文）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScopeDecision {
    /// 范围内（可主动交互）。
    InScope,
    /// 范围外被动捕获（可见，不主动交互）。
    OutOfScopePassive,
    /// 范围外拦截（主动发送前拒）。
    OutOfScopeBlocked,
}

/// Exchange 记录（DESIGN §1.2 全字段；存储投影 = §3.2 wsw_exchange 表，
/// 表结构 WSW-3 落位，本批为类型/serde 冻结面）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Exchange {
    /// 主键。
    pub exchange_id: ExchangeId,
    /// 会话。
    pub session_id: SessionId,
    /// 监听器实例。
    pub listener_id: ListenerId,
    /// listener 内单调序号。
    pub sequence_no: u64,
    /// 方向。
    pub direction: Direction,
    /// 来源（导入必须 Import）。
    pub source: MessageSource,
    /// 应用协议族（小写："http"/"ws"；PLAN :490）。
    pub protocol: String,
    /// 线缆版本（小写："http/1.0"/"http/1.1"/"h2"…）。
    pub http_version: String,
    /// TLS 版本（明文捕获 None）。
    pub tls_version: Option<String>,
    /// ALPN（明文捕获 None）。
    pub alpn: Option<String>,
    /// 请求方法。
    pub method: Option<String>,
    /// scheme（绝对形式/CONNECT 派生）。
    pub scheme: Option<String>,
    /// authority（含 host:port）。
    pub authority: Option<String>,
    /// 路径。
    pub path: Option<String>,
    /// 请求头序保持数组（Parsed 投影）。
    pub request_headers: Option<Vec<HeaderRecord>>,
    /// 响应头序保持数组（Parsed 投影）。
    pub response_headers: Option<Vec<HeaderRecord>>,
    /// 请求体 CAS 引用。
    pub request_body_artifact: Option<BodyRef>,
    /// 响应体 CAS 引用。
    pub response_body_artifact: Option<BodyRef>,
    /// 响应状态码。
    pub response_status: Option<u16>,
    /// 开始时刻（ISO8601 UTC）。
    pub started_at: Option<String>,
    /// 完成时刻（ISO8601 UTC）。
    pub completed_at: Option<String>,
    /// 总时长（毫秒）。
    pub duration_ms: Option<u64>,
    /// TTFB（毫秒）。
    pub ttfb_ms: Option<u64>,
    /// 连接实际对端 IP（DNS rebinding 事后审计锚，DESIGN §4.2）。
    pub remote_ip: Option<std::net::IpAddr>,
    /// Scope 判定（None = Unknown，第三态 NULL 表达）。
    pub scope_decision: Option<ScopeDecision>,
    /// 编辑标记（Outbound ≠ Wire）。
    pub edited: bool,
    /// 用户标签。
    pub tags: Vec<String>,
    /// 用户备注。
    pub notes: Option<String>,
    /// trace 关联（六 ID，domain 再导出面校验）。
    pub trace_id: Option<String>,
    /// 任务关联。
    pub task_id: Option<String>,
    /// 工具运行关联。
    pub tool_run_id: Option<String>,
    /// 产物关联。
    pub artifact_id: Option<String>,
    /// 证据关联。
    pub evidence_id: Option<String>,
    /// 捕获保真词表。
    pub capture_fidelity: CaptureFidelity,
    /// 解析状态词表。
    pub parse_status: ParseStatus,
    /// 错误码词表。
    pub error_code: Option<ErrorCode>,
}

/// Exchange 校验错误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExchangeError {
    /// 关联 ID 未过 domain::ids 校验面。
    InvalidCorrelationId {
        /// 字段名。
        field: &'static str,
        /// 校验原因（domain::ids 文案）。
        reason: String,
    },
}

impl std::fmt::Display for ExchangeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidCorrelationId { field, reason } => {
                write!(f, "invalid correlation id ({field}): {reason}")
            }
        }
    }
}

impl std::error::Error for ExchangeError {}

impl Exchange {
    /// 六 ID 关联引用校验（引用 domain 再导出面
    /// `domain::{TraceId, TaskId, ToolRunId, ArtifactId, EvidenceId}`，
    /// `crates/domain/src/lib.rs:59`；域层不动、本 crate 不重复实现校验口径）。
    pub fn validate_correlation_ids(&self) -> Result<(), ExchangeError> {
        let checks: [(&'static str, Option<&String>); 5] = [
            ("trace_id", self.trace_id.as_ref()),
            ("task_id", self.task_id.as_ref()),
            ("tool_run_id", self.tool_run_id.as_ref()),
            ("artifact_id", self.artifact_id.as_ref()),
            ("evidence_id", self.evidence_id.as_ref()),
        ];
        for (field, value) in checks {
            let Some(v) = value else { continue };
            let result: Result<(), domain::DomainError> = match field {
                "trace_id" => domain::TraceId::try_new(v.clone()).map(|_| ()),
                "task_id" => domain::TaskId::try_new(v.clone()).map(|_| ()),
                "tool_run_id" => domain::ToolRunId::try_new(v.clone()).map(|_| ()),
                "artifact_id" => domain::ArtifactId::try_new(v.clone()).map(|_| ()),
                _ => domain::EvidenceId::try_new(v.clone()).map(|_| ()),
            };
            result.map_err(|e| ExchangeError::InvalidCorrelationId {
                field,
                reason: e.to_string(),
            })?;
        }
        Ok(())
    }

    /// 方向 ↔ 消息种类一致性（请求面消息 direction=client_to_server，
    /// 响应面 = server_to_client；WSW-2/3 组装入口守卫）。
    pub fn direction_matches_kind(&self, kind: crate::types::MessageKind) -> bool {
        matches!(
            (self.direction, kind),
            (
                Direction::ClientToServer,
                crate::types::MessageKind::Request
            ) | (
                Direction::ServerToClient,
                crate::types::MessageKind::Response
            )
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn full_exchange() -> Exchange {
        Exchange {
            exchange_id: ExchangeId::try_new("wsw-20261118120000123456-0-ab12").unwrap(),
            session_id: SessionId::try_new("wsw-ses-20261118120000123456-0-ab12").unwrap(),
            listener_id: ListenerId::try_new("wsw-lis-20261118120000123456-0-ab12").unwrap(),
            sequence_no: 7,
            direction: Direction::ClientToServer,
            source: MessageSource::ProxyCapture,
            protocol: "http".into(),
            http_version: "http/1.1".into(),
            tls_version: Some("tls1.3".into()),
            alpn: Some("h2".into()),
            method: Some("GET".into()),
            scheme: Some("https".into()),
            authority: Some("example.com:443".into()),
            path: Some("/p".into()),
            request_headers: Some(vec![HeaderRecord::new("Host", "example.com:443")]),
            response_headers: Some(vec![HeaderRecord::new("Server", "t")]),
            request_body_artifact: Some(BodyRef {
                sha256: "ab".repeat(32),
                uri: "wsw/ab/".to_string() + &"ab".repeat(32),
                size: 10,
                fidelity: crate::types::CaptureFidelity::Full,
            }),
            response_body_artifact: None,
            response_status: Some(200),
            started_at: Some("2026-11-18T00:00:00Z".into()),
            completed_at: Some("2026-11-18T00:00:01Z".into()),
            duration_ms: Some(1000),
            ttfb_ms: Some(120),
            remote_ip: Some(std::net::IpAddr::from([93, 184, 216, 34])),
            scope_decision: Some(ScopeDecision::InScope),
            edited: false,
            tags: vec!["login".into()],
            notes: Some("note".into()),
            trace_id: Some("tr-20261118-1".into()),
            task_id: Some("ta-20261118-1".into()),
            tool_run_id: Some("rn-20261118-1".into()),
            artifact_id: Some("art-20261118-1".into()),
            evidence_id: Some("re-20261118-1".into()),
            capture_fidelity: CaptureFidelity::Full,
            parse_status: ParseStatus::Ok,
            error_code: None,
        }
    }

    #[test]
    fn serde_roundtrip_full_fields() {
        let ex = full_exchange();
        let json = serde_json::to_string_pretty(&ex).unwrap();
        let back: Exchange = serde_json::from_str(&json).unwrap();
        assert_eq!(back, ex, "DESIGN §1.2 全字段 serde round-trip");
    }

    #[test]
    fn pinned_json_vocabulary_deserialize() {
        // 词表钉值（小写）反序列化 + 第三态 Unknown = 字段缺失（NULL 表达）
        let json = r#"{
            "exchange_id": "wsw-1",
            "session_id": "wsw-ses-1",
            "listener_id": "wsw-lis-1",
            "sequence_no": 0,
            "direction": "client_to_server",
            "source": "import",
            "protocol": "http",
            "http_version": "http/1.1",
            "tls_version": null,
            "alpn": null,
            "method": null,
            "scheme": null,
            "authority": null,
            "path": null,
            "request_headers": null,
            "response_headers": null,
            "request_body_artifact": null,
            "response_body_artifact": null,
            "response_status": null,
            "started_at": null,
            "completed_at": null,
            "duration_ms": null,
            "ttfb_ms": null,
            "remote_ip": null,
            "scope_decision": null,
            "edited": false,
            "tags": [],
            "notes": null,
            "trace_id": null,
            "task_id": null,
            "tool_run_id": null,
            "artifact_id": null,
            "evidence_id": null,
            "capture_fidelity": "degraded_no_wire",
            "parse_status": "partial",
            "error_code": "truncated_body"
        }"#;
        let ex: Exchange = serde_json::from_str(json).unwrap();
        assert_eq!(ex.capture_fidelity, CaptureFidelity::DegradedNoWire);
        assert_eq!(ex.parse_status, ParseStatus::Partial);
        assert_eq!(ex.error_code, Some(ErrorCode::TruncatedBody));
        assert_eq!(ex.source, MessageSource::Import, "导入必须 source=import");
        assert_eq!(
            ex.scope_decision, None,
            "第三态 Unknown = NULL 表达（词表无 unknown 值）"
        );
        assert_eq!(ex.direction, Direction::ClientToServer);
    }

    #[test]
    fn correlation_validation_via_domain_reexport() {
        let mut ex = full_exchange();
        assert!(ex.validate_correlation_ids().is_ok());
        ex.trace_id = Some(String::new());
        let err = ex.validate_correlation_ids().unwrap_err();
        assert_eq!(err.field_name(), "trace_id");
        ex.trace_id = Some("x".repeat(domain_trace_cap() + 1));
        assert!(ex.validate_correlation_ids().is_err());
    }

    fn domain_trace_cap() -> usize {
        128
    }

    impl ExchangeError {
        fn field_name(&self) -> &'static str {
            match self {
                Self::InvalidCorrelationId { field, .. } => field,
            }
        }
    }

    #[test]
    fn direction_matches_kind_guard() {
        let mut ex = full_exchange();
        ex.direction = Direction::ClientToServer;
        assert!(ex.direction_matches_kind(crate::types::MessageKind::Request));
        assert!(!ex.direction_matches_kind(crate::types::MessageKind::Response));
        ex.direction = Direction::ServerToClient;
        assert!(ex.direction_matches_kind(crate::types::MessageKind::Response));
    }

    #[test]
    fn scope_decision_vocabulary() {
        assert_eq!(
            serde_json::to_string(&ScopeDecision::OutOfScopePassive).unwrap(),
            "\"out_of_scope_passive\""
        );
        assert_eq!(
            serde_json::to_string(&ScopeDecision::OutOfScopeBlocked).unwrap(),
            "\"out_of_scope_blocked\""
        );
        assert_eq!(
            serde_json::to_string(&MessageSource::ProxyCapture).unwrap(),
            "\"proxy_capture\""
        );
    }
}
