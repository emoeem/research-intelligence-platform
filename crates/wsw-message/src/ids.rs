//! Exchange/Session/Listener ID 新类型（沿 `domain::ids` 范式）。
//!
//! 范式（`crates/domain/src/ids.rs:10-17` 六 ID 同源）：
//! - 域层只做非空与长度校验（生成前缀与时间戳形态属服务端实现细节，
//!   不进校验面 —— 先例 TraceId 文档：「生成策略属服务端实现细节，域层
//!   只做非空与长度校验」）；
//! - 长度上限与 domain `CORRELATION_ID_MAX_LEN` 同口径 = 128 字符；
//! - 生成格式沿 022 迁移 `hi-YYYYMMDD-HHMMSS-ms-rand` 先例形状：
//!   `wsw-<YYYYMMDDHHMMSSffffff>-<seq>-<rand4>`（ExchangeId 前缀 `wsw-`
//!   为 DESIGN §1.2 明文要求；Session/Listener 用 `wsw-ses-`/`wsw-lis-`
//!   同族前缀）；
//! - 随机位取 `RandomState` 进程级随机种子派生（纯 std，零新依赖），
//!   与序列号/微秒时间戳叠加保证进程内唯一。
//!
//! 六关联 ID（trace/task/tool_run/artifact/evidence）引用 domain 再导出面
//! （`domain::{TraceId, TaskId, ToolRunId, ArtifactId, EvidenceId}`，
//! `crates/domain/src/lib.rs:59`），由 [`crate::exchange`] 校验承载，
//! 本模块不重复定义。

use serde::{Deserialize, Serialize};
use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};

/// ID 通用长度上限（与 `domain::ids::CORRELATION_ID_MAX_LEN` 同口径：非空
/// 且不超过 128 字符）。
pub const ID_MAX_LEN: usize = 128;

fn check_id(value: &str, kind: &str) -> Result<(), String> {
    if value.is_empty() {
        return Err(format!("empty {kind} id"));
    }
    if value.chars().count() > ID_MAX_LEN {
        return Err(format!("{kind} id exceeds {ID_MAX_LEN} chars"));
    }
    Ok(())
}

fn generate_id(prefix: &str) -> String {
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let seq = SEQ.fetch_add(1, Ordering::Relaxed);
    // domain::time::now_compact_micros() = YYYYMMDDHHMMSSffffff（纯 std 实现）
    let ts = domain::time::now_compact_micros();
    // rand4：进程级随机种子（RandomState）派生，std-only
    let rand4 = {
        use std::hash::{BuildHasher, Hasher};
        let mut h = std::collections::hash_map::RandomState::new().build_hasher();
        h.write_u64(seq);
        h.write(&std::process::id().to_le_bytes());
        format!("{:04x}", h.finish() as u16)
    };
    format!("{prefix}{ts}-{seq:x}-{rand4}")
}

macro_rules! wsw_id {
    ($(#[$meta:meta])* $name:ident, $kind:literal, $prefix:literal) => {
        $(#[$meta])*
        #[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            /// ID 前缀（生成侧承载，校验面不含前缀 —— domain 范式）。
            pub const PREFIX: &'static str = $prefix;

            /// 校验并构造：非空且不超过 [`ID_MAX_LEN`] 字符。
            pub fn try_new(value: impl Into<String>) -> Result<Self, String> {
                let value = value.into();
                check_id(&value, $kind)?;
                Ok(Self(value))
            }

            /// 生成新 ID（前缀 + 微秒时间戳 + 序号 + 随机 4 hex）。
            pub fn generate() -> Self {
                Self(generate_id($prefix))
            }

            /// 内部值视图。
            pub fn as_str(&self) -> &str {
                &self.0
            }

            /// 消费自身，返回内部 String。
            pub fn into_inner(self) -> String {
                self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl AsRef<str> for $name {
            fn as_ref(&self) -> &str {
                &self.0
            }
        }

        impl TryFrom<String> for $name {
            type Error = String;

            fn try_from(value: String) -> Result<Self, Self::Error> {
                Self::try_new(value)
            }
        }

        impl TryFrom<&str> for $name {
            type Error = String;

            fn try_from(value: &str) -> Result<Self, Self::Error> {
                Self::try_new(value.to_string())
            }
        }
    };
}

wsw_id!(
    /// Exchange ID：一条 HTTP 消息记录主键（DESIGN §1.2，`wsw-` 前缀）。
    ExchangeId,
    "exchange",
    "wsw-"
);
wsw_id!(
    /// Session ID：WSW 会话/工作区范围（DESIGN §3.2 wsw_session）。
    SessionId,
    "session",
    "wsw-ses-"
);
wsw_id!(
    /// Listener ID：MITM 监听器实例（DESIGN §3.2 wsw_listener，WSW-2 消费）。
    ListenerId,
    "listener",
    "wsw-lis-"
);
wsw_id!(
    /// Fuzz Job ID：Web Fuzzer 任务行主键（028 wsw_fuzz_job，WSW-4 消费）。
    FuzzJobId,
    "fuzz_job",
    "wsw-fzj-"
);
wsw_id!(
    /// Fuzz Case ID：fuzz 单 case 行主键（028 wsw_fuzz_case，WSW-4 消费）。
    FuzzCaseId,
    "fuzz_case",
    "wsw-fzc-"
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn try_new_validation() {
        assert!(ExchangeId::try_new("wsw-20261118120000-0-ab12").is_ok());
        assert!(ExchangeId::try_new("").is_err());
        let long = "x".repeat(ID_MAX_LEN + 1);
        assert!(ExchangeId::try_new(long).is_err());
        // 恰好上限长度合法
        let ok = "x".repeat(ID_MAX_LEN);
        assert!(ExchangeId::try_new(ok).is_ok());
    }

    #[test]
    fn generate_shape_and_uniqueness() {
        let a = ExchangeId::generate();
        assert!(a.as_str().starts_with("wsw-"));
        let mut ids = Vec::new();
        for _ in 0..200 {
            ids.push(ExchangeId::generate().into_inner());
        }
        let uniq = ids.iter().collect::<std::collections::HashSet<_>>();
        assert_eq!(uniq.len(), ids.len(), "generate 必须进程内唯一");
        assert!(SessionId::generate().as_str().starts_with("wsw-ses-"));
        assert!(ListenerId::generate().as_str().starts_with("wsw-lis-"));
        assert_eq!(ExchangeId::PREFIX, "wsw-");
        assert_eq!(SessionId::PREFIX, "wsw-ses-");
        assert_eq!(ListenerId::PREFIX, "wsw-lis-");
    }

    #[test]
    fn serde_roundtrip() {
        let id = ExchangeId::try_new("wsw-20261118-1-ab12").unwrap();
        let json = serde_json::to_string(&id).unwrap();
        assert_eq!(json, "\"wsw-20261118-1-ab12\"");
        let back: ExchangeId = serde_json::from_str(&json).unwrap();
        assert_eq!(back, id);
    }
}
