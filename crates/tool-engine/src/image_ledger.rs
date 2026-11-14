//! 镜像 digest 台账（ADR TOOL-RUNTIME §6 故障恢复「镜像回滚」语义落地，
//! ROADMAP P10-2）：
//!
//! - 镜像固定版本与 digest：`pin` 把当前 digest 入账（sha256:<64hex> 校验）；
//! - 更新先验证再切换：`update` 只在 `verified=true`（调用方先验证新
//!   Manifest，本仓库口径为 tool-registry `validate_static` 第 1/3/4/5/6/8
//!   条 + digest 与 ledger 入参对账）时切换指针，`previous_digest` 保留旧
//!   digest 作回滚位；
//! - 升级失败回滚旧 digest：`verified=false` 时指针绝不切换（attempt 记
//!   rolled_back），与「更新先验证再切换」互为两面；
//! - 回滚：`rollback` 把指针拨回 previous_digest（无回滚位拒绝），显式
//!   操作留痕。
//!
//! 存储为调用方给定的 JSON 文件（镜像状态属运行时数据，红线：源码零
//! rip-data 绝对路径字面量，路径一律由 CLI/selftest 显式传入）。历史记录
//! 有界（默认保留最近 50 条 attempt），完整留痕以 audit 面为准（调用方
//! Emit）。

use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

/// 台账文件名（约定；路径仍由调用方拼接，本模块不假设目录布局）。
pub const LEDGER_FILENAME: &str = "image-ledger.json";

/// ledger 错误（机器可读：code + message）。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LedgerError {
    /// 稳定错误码：digest_invalid / image_unknown / image_exists /
    /// rollback_unavailable / io / parse。
    pub code: String,
    /// 人类可读原因。
    pub message: String,
}

impl LedgerError {
    fn new(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.to_string(),
            message: message.into(),
        }
    }
}

impl std::fmt::Display for LedgerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "image ledger error [{}]: {}", self.code, self.message)
    }
}

impl std::error::Error for LedgerError {}

/// digest 严格格式校验：`sha256:<64 小写 hex>`（ADR §3.2/§6 与
/// containerfiles digest 固定规则同口径；大写或非法字符一律拒绝）。
pub fn digest_valid(digest: &str) -> bool {
    let Some(hex) = digest.strip_prefix("sha256:") else {
        return false;
    };
    hex.len() == 64
        && hex
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// 单次 attempt（pin / update / rollback 的历史留痕）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImageAttempt {
    /// Unix 秒。
    pub at_unix: i64,
    /// 动作：pin / update / rollback。
    pub action: String,
    /// 本次动作涉及的目标 digest（update 失败时为新 digest）。
    pub digest: String,
    /// 结果：ok / rolled_back / rejected。
    pub result: String,
    /// 原因（失败与回滚必填）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// 单镜像 pin 记录（指针 + 回滚位 + 有界历史）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImagePin {
    /// 镜像引用（不带 digest 部分，如 localhost/rip-tool-base）。
    pub image: String,
    /// 当前生效 digest（sha256:<64hex>）。
    pub current_digest: String,
    /// 上一 digest（回滚位；首次 pin 后为 None）。
    pub previous_digest: Option<String>,
    /// 最后操作时间（Unix 秒）。
    pub updated_at_unix: i64,
    /// 有界历史（最新在后，默认保留 50 条）。
    pub history: Vec<ImageAttempt>,
}

/// 台账文档（schema_version 1；pins 按 image 排序，确定性输出）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImageLedger {
    /// schema 版本。
    pub schema_version: String,
    /// pin 记录（按 image 排序）。
    pub pins: Vec<ImagePin>,
}

impl Default for ImageLedger {
    fn default() -> Self {
        Self {
            schema_version: "1".to_string(),
            pins: Vec::new(),
        }
    }
}

/// update 结果（机器可读，selftest 与 CLI 直接回显）。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct UpdateOutcome {
    /// switched（验证通过并切换）/ rolled_back（验证失败，指针未切换）。
    pub result: String,
    /// 操作后生效 digest（rolled_back 时即旧 digest）。
    pub current_digest: String,
    /// 切换前的 digest（回滚位）。
    pub previous_digest: Option<String>,
}

/// 历史条数上限（有界防无界增长；完整留痕走 audit 面）。
const HISTORY_CAP: usize = 50;

fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

impl ImageLedger {
    /// 从文件读取（不存在 = 空台账；解析失败显式报错不静默重建）。
    pub fn load(path: &Path) -> Result<Self, LedgerError> {
        match fs::read_to_string(path) {
            Ok(body) => serde_json::from_str(&body)
                .map_err(|e| LedgerError::new("parse", format!("parse {}: {e}", path.display()))),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(LedgerError::new(
                "io",
                format!("read {}: {e}", path.display()),
            )),
        }
    }

    /// 原子落盘（同目录临时文件 + rename；父目录不存在即报错）。
    pub fn save(&self, path: &Path) -> Result<(), LedgerError> {
        let body = serde_json::to_string_pretty(self)
            .map_err(|e| LedgerError::new("io", format!("serialize: {e}")))?;
        let tmp = path.with_extension("json.tmp");
        fs::write(&tmp, body)
            .map_err(|e| LedgerError::new("io", format!("write {}: {e}", tmp.display())))?;
        fs::rename(&tmp, path)
            .map_err(|e| LedgerError::new("io", format!("rename {}: {e}", path.display())))
    }

    fn pin_mut(&mut self, image: &str) -> Result<&mut ImagePin, LedgerError> {
        self.pins
            .iter_mut()
            .find(|p| p.image == image)
            .ok_or_else(|| {
                LedgerError::new(
                    "image_unknown",
                    format!("image {image:?} is not pinned; pin it first"),
                )
            })
    }

    fn push_attempt(pin: &mut ImagePin, attempt: ImageAttempt) {
        pin.updated_at_unix = attempt.at_unix;
        pin.history.push(attempt);
        if pin.history.len() > HISTORY_CAP {
            let overflow = pin.history.len() - HISTORY_CAP;
            pin.history.drain(0..overflow);
        }
    }

    /// 初始 pin（镜像首次入账；重复 pin 同 digest 幂等，异 digest 拒绝，
    /// 换 digest 一律走 update）。
    pub fn pin(&mut self, image: &str, digest: &str) -> Result<(), LedgerError> {
        if !digest_valid(digest) {
            return Err(LedgerError::new(
                "digest_invalid",
                format!("digest {digest:?} must be sha256:<64 lowercase hex>"),
            ));
        }
        if let Some(existing) = self.pins.iter_mut().find(|p| p.image == image) {
            if existing.current_digest == digest {
                return Ok(()); // 幂等
            }
            return Err(LedgerError::new(
                "image_exists",
                format!(
                    "image {image:?} already pinned at {}; use update to change digest",
                    existing.current_digest
                ),
            ));
        }
        let pin = ImagePin {
            image: image.to_string(),
            current_digest: digest.to_string(),
            previous_digest: None,
            updated_at_unix: now_unix(),
            history: vec![ImageAttempt {
                at_unix: now_unix(),
                action: "pin".to_string(),
                digest: digest.to_string(),
                result: "ok".to_string(),
                reason: None,
            }],
        };
        self.pins.push(pin);
        self.pins.sort_by(|a, b| a.image.cmp(&b.image));
        Ok(())
    }

    /// 更新指针（ADR §6「更新先验证再切换，升级失败回滚旧 digest」）：
    /// `verified=false` 时指针绝不切换，attempt 记 rolled_back（新 digest
    /// 不生效，旧 digest 保持 current）；`verified=true` 且新 digest 合法
    /// 时切换并把旧 digest 移入回滚位；同 digest 幂等。
    pub fn update(
        &mut self,
        image: &str,
        new_digest: &str,
        verified: bool,
        reason: Option<&str>,
    ) -> Result<UpdateOutcome, LedgerError> {
        if !digest_valid(new_digest) {
            // 格式非法在验证阶段就拒绝（attempt 记 rejected，指针不动）。
            let pin = self.pin_mut(image)?;
            Self::push_attempt(
                pin,
                ImageAttempt {
                    at_unix: now_unix(),
                    action: "update".to_string(),
                    digest: new_digest.to_string(),
                    result: "rejected".to_string(),
                    reason: Some("digest_invalid".to_string()),
                },
            );
            return Err(LedgerError::new(
                "digest_invalid",
                format!("digest {new_digest:?} must be sha256:<64 lowercase hex>"),
            ));
        }
        let pin = self.pin_mut(image)?;
        let current = pin.current_digest.clone();
        if new_digest == current {
            Self::push_attempt(
                pin,
                ImageAttempt {
                    at_unix: now_unix(),
                    action: "update".to_string(),
                    digest: new_digest.to_string(),
                    result: "ok".to_string(),
                    reason: Some("already_current".to_string()),
                },
            );
            return Ok(UpdateOutcome {
                result: "switched".to_string(),
                current_digest: current,
                previous_digest: pin.previous_digest.clone(),
            });
        }
        if !verified {
            // 升级失败回滚旧 digest：指针不切换，旧 digest 继续 current。
            Self::push_attempt(
                pin,
                ImageAttempt {
                    at_unix: now_unix(),
                    action: "update".to_string(),
                    digest: new_digest.to_string(),
                    result: "rolled_back".to_string(),
                    reason: Some(reason.unwrap_or("verification_failed").to_string()),
                },
            );
            return Ok(UpdateOutcome {
                result: "rolled_back".to_string(),
                current_digest: current,
                previous_digest: pin.previous_digest.clone(),
            });
        }
        pin.previous_digest = Some(current.clone());
        pin.current_digest = new_digest.to_string();
        Self::push_attempt(
            pin,
            ImageAttempt {
                at_unix: now_unix(),
                action: "update".to_string(),
                digest: new_digest.to_string(),
                result: "ok".to_string(),
                reason: None,
            },
        );
        Ok(UpdateOutcome {
            result: "switched".to_string(),
            current_digest: new_digest.to_string(),
            previous_digest: Some(current),
        })
    }

    /// 显式回滚：指针拨回 previous_digest（无回滚位拒绝）。ADR §6 回滚语义
    /// 的操作面（升级失败自动保持旧 digest 之外，人为拨回亦留痕）。
    pub fn rollback(&mut self, image: &str) -> Result<UpdateOutcome, LedgerError> {
        let pin = self.pin_mut(image)?;
        let Some(previous) = pin.previous_digest.clone() else {
            return Err(LedgerError::new(
                "rollback_unavailable",
                format!("image {image:?} has no previous digest to roll back to"),
            ));
        };
        let current = pin.current_digest.clone();
        pin.current_digest = previous.clone();
        pin.previous_digest = Some(current.clone());
        Self::push_attempt(
            pin,
            ImageAttempt {
                at_unix: now_unix(),
                action: "rollback".to_string(),
                digest: previous.clone(),
                result: "ok".to_string(),
                reason: Some("manual_rollback".to_string()),
            },
        );
        Ok(UpdateOutcome {
            result: "rolled_back".to_string(),
            current_digest: previous,
            previous_digest: pin.previous_digest.clone(),
        })
    }

    /// 只读视图（CLI show / selftest 断言）。
    pub fn get(&self, image: &str) -> Option<&ImagePin> {
        self.pins.iter().find(|p| p.image == image)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const D1: &str = "sha256:1111111111111111111111111111111111111111111111111111111111111111";
    const D2: &str = "sha256:2222222222222222222222222222222222222222222222222222222222222222";
    const D3: &str = "sha256:3333333333333333333333333333333333333333333333333333333333333333";

    #[test]
    fn digest_format_is_strict() {
        assert!(digest_valid(D1));
        assert!(!digest_valid("sha256:XYZ"));
        assert!(!digest_valid("sha256:1111"));
        assert!(!digest_valid(&D1.to_uppercase()));
        assert!(!digest_valid("localhost/rip-tool-base:smoke"));
        assert!(!digest_valid(""));
    }

    #[test]
    fn pin_idempotent_and_rejects_drift() {
        let mut ledger = ImageLedger::default();
        ledger.pin("localhost/rip-tool-base", D1).unwrap();
        ledger.pin("localhost/rip-tool-base", D1).unwrap(); // 幂等
        let err = ledger.pin("localhost/rip-tool-base", D2).unwrap_err();
        assert_eq!(err.code, "image_exists");
        let err = ledger.pin("localhost/rip-tool-base", "bogus").unwrap_err();
        assert_eq!(err.code, "digest_invalid");
    }

    #[test]
    fn failed_update_keeps_old_digest_and_rollback_recovers() {
        let mut ledger = ImageLedger::default();
        ledger.pin("localhost/rip-tool-webresearch", D1).unwrap();

        // 升级失败（verified=false）：rolled_back，指针不切换（ADR §6）。
        let out = ledger
            .update(
                "localhost/rip-tool-webresearch",
                D2,
                false,
                Some("manifest digest mismatch"),
            )
            .unwrap();
        assert_eq!(out.result, "rolled_back");
        assert_eq!(out.current_digest, D1);
        assert_eq!(
            ledger
                .get("localhost/rip-tool-webresearch")
                .unwrap()
                .current_digest,
            D1
        );
        let pin = ledger.get("localhost/rip-tool-webresearch").unwrap();
        assert_eq!(pin.history.last().unwrap().result, "rolled_back");
        assert_eq!(
            pin.history.last().unwrap().reason.as_deref(),
            Some("manifest digest mismatch")
        );

        // 显式回滚在无回滚位时拒绝（首 pin 之后从未成功切换过）。
        let err = ledger
            .rollback("localhost/rip-tool-webresearch")
            .unwrap_err();
        assert_eq!(err.code, "rollback_unavailable");

        // 更新成功：切换，旧 digest 进回滚位。
        let out = ledger
            .update("localhost/rip-tool-webresearch", D2, true, None)
            .unwrap();
        assert_eq!(out.result, "switched");
        assert_eq!(out.current_digest, D2);
        assert_eq!(out.previous_digest.as_deref(), Some(D1));

        // 显式回滚：拨回旧 digest，回滚位换成切换前 digest（可再滚回）。
        let out = ledger.rollback("localhost/rip-tool-webresearch").unwrap();
        assert_eq!(out.result, "rolled_back");
        assert_eq!(out.current_digest, D1);
        assert_eq!(out.previous_digest.as_deref(), Some(D2));

        // 再更新到 D3 后指针为 D3（previous=D1 已被 rollback 交换成 D2，
        // 再 update 覆盖 previous 为切换前 current=D1）。
        let out = ledger
            .update("localhost/rip-tool-webresearch", D3, true, None)
            .unwrap();
        assert_eq!(out.current_digest, D3);
        assert_eq!(out.previous_digest.as_deref(), Some(D1));
    }

    #[test]
    fn load_save_roundtrip_and_unknown_image() {
        let dir = std::env::temp_dir().join(format!("p10-imgledger-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join(LEDGER_FILENAME);
        // 不存在 = 空台账。
        let mut ledger = ImageLedger::load(&path).unwrap();
        assert!(ledger.pins.is_empty());
        ledger.pin("localhost/a", D1).unwrap();
        ledger.save(&path).unwrap();
        let round = ImageLedger::load(&path).unwrap();
        assert_eq!(round, ledger);
        let err = ledger
            .update("localhost/never-pinned", D2, true, None)
            .unwrap_err();
        assert_eq!(err.code, "image_unknown");
        // 篡改文件显式报 parse（不静默重建）。
        fs::write(&path, "{not json").unwrap();
        let err = ImageLedger::load(&path).unwrap_err();
        assert_eq!(err.code, "parse");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn history_is_bounded() {
        let mut ledger = ImageLedger::default();
        ledger.pin("localhost/a", D1).unwrap();
        for i in 0..60 {
            let d = format!("sha256:{:064x}", i + 2);
            ledger.update("localhost/a", &d, true, None).unwrap();
        }
        let pin = ledger.get("localhost/a").unwrap();
        assert!(pin.history.len() <= HISTORY_CAP);
        assert_eq!(pin.current_digest, format!("sha256:{:064x}", 61));
    }
}
