//! 信任级策略（PLUGIN-CONTRACT §6 与草案 D10 的机器可读落地，ROADMAP
//! P10-1/P10-2）：
//!
//! 1. 安装确认强度分级（§6「影响安装确认提示强度」）：四档信任级映射到
//!    四级确认强度，从 official 的常规确认到 remote 的显式二次确认（输入
//!    插件 id 才能继续）。安装确认由前端对话框执行，服务端 / CLI 侧按
//!    [`policy_json`] 同一表强制（fail-closed：策略不可得时按最强档执行）。
//! 2. 自动更新默认策略（D10：仅 official 允许 update_check 自动提示，其余
//!    须手动检查）：[`auto_update_prompt_allowed`] 是唯一判定面，manual
//!    检查对所有信任级保持可用。
//!
//! 本模块纯函数 + 序列化面，无 IO；上游（plugin-runtime、rp-server、
//! rp-plugin CLI、web）一律消费同一份表，不各自硬编码。

use serde::Serialize;

use crate::lock::TrustLevel;

/// 安装确认强度四级（§6 信任级驱动的确认提示强度）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum InstallConfirmation {
    /// official：常规确认（一次点击，提示官方来源）。
    Single,
    /// community：确认对话框（明确同意一次，提示社区来源）。
    Confirm,
    /// local：确认 + 来源核对（展示 source_type 与路径，需勾选「已核对」）。
    ConfirmWithProvenance,
    /// remote：显式二次确认（输入插件 id 才能继续，提示远程任意来源风险）。
    DoubleConfirmTyped,
}

impl InstallConfirmation {
    /// 稳定词形（snake_case，与 serde 序列化一致）。
    pub fn as_str(&self) -> &'static str {
        match self {
            InstallConfirmation::Single => "single",
            InstallConfirmation::Confirm => "confirm",
            InstallConfirmation::ConfirmWithProvenance => "confirm_with_provenance",
            InstallConfirmation::DoubleConfirmTyped => "double_confirm_typed",
        }
    }

    /// 对话框标题（中文，前端直接展示）。
    pub fn label(&self) -> &'static str {
        match self {
            InstallConfirmation::Single => "官方来源：常规确认",
            InstallConfirmation::Confirm => "社区来源：确认安装",
            InstallConfirmation::ConfirmWithProvenance => "本地来源：核对来源后确认",
            InstallConfirmation::DoubleConfirmTyped => "远程来源：输入插件 id 二次确认",
        }
    }

    /// 除最终点击外需要的额外确认交互次数（1 = 对话框内一次确认）。
    pub fn extra_steps(&self) -> u8 {
        match self {
            InstallConfirmation::Single => 0,
            InstallConfirmation::Confirm => 1,
            InstallConfirmation::ConfirmWithProvenance => 1,
            InstallConfirmation::DoubleConfirmTyped => 1,
        }
    }

    /// 是否需要展示并确认来源定位（source_type / source_ref）。
    pub fn requires_provenance(&self) -> bool {
        matches!(self, InstallConfirmation::ConfirmWithProvenance)
    }

    /// 是否需要输入插件 id（remote 显式二次确认）。
    pub fn requires_typed_id(&self) -> bool {
        matches!(self, InstallConfirmation::DoubleConfirmTyped)
    }

    /// 服务端强制面需要的请求字段（None = 无额外字段；确认交互在客户端）。
    pub fn required_confirm_fields(&self) -> &'static [&'static str] {
        match self {
            InstallConfirmation::Single => &[],
            InstallConfirmation::Confirm | InstallConfirmation::ConfirmWithProvenance => {
                &["confirm"]
            }
            InstallConfirmation::DoubleConfirmTyped => &["confirm", "confirm_id"],
        }
    }

    /// 解析稳定词形。
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "single" => Some(InstallConfirmation::Single),
            "confirm" => Some(InstallConfirmation::Confirm),
            "confirm_with_provenance" => Some(InstallConfirmation::ConfirmWithProvenance),
            "double_confirm_typed" => Some(InstallConfirmation::DoubleConfirmTyped),
            _ => None,
        }
    }
}

/// 信任级 → 安装确认强度（§6 四档映射，单一判定面）。
pub fn install_confirmation(level: TrustLevel) -> InstallConfirmation {
    match level {
        TrustLevel::Official => InstallConfirmation::Single,
        TrustLevel::Community => InstallConfirmation::Confirm,
        TrustLevel::Local => InstallConfirmation::ConfirmWithProvenance,
        TrustLevel::Remote => InstallConfirmation::DoubleConfirmTyped,
    }
}

/// D10：自动 update_check 提示仅 official 允许；manual 检查不经过本判定
/// （对所有信任级可用）。
pub fn auto_update_prompt_allowed(level: TrustLevel) -> bool {
    matches!(level, TrustLevel::Official)
}

/// 四档策略表（机器可读，rp-plugin trust-policy / rp-server
/// /api/plugins/trust-policy / web 对话框共用同一份）。
pub fn policy_json() -> serde_json::Value {
    use serde_json::json;
    let tiers = [
        TrustLevel::Official,
        TrustLevel::Community,
        TrustLevel::Local,
        TrustLevel::Remote,
    ]
    .map(|level| {
        let confirmation = install_confirmation(level);
        json!({
            "trust_level": level.as_str(),
            "install_confirmation": confirmation.as_str(),
            "confirmation_label": confirmation.label(),
            "extra_steps": confirmation.extra_steps(),
            "requires_provenance": confirmation.requires_provenance(),
            "requires_typed_id": confirmation.requires_typed_id(),
            "required_confirm_fields": confirmation.required_confirm_fields(),
            "auto_update_prompt": auto_update_prompt_allowed(level),
        })
    });
    json!({
        "schema_version": "1",
        "source": "plugin-registry::trust_policy (PLUGIN-CONTRACT §6 + D10)",
        "tiers": tiers,
        "notes": "D10：仅 official 允许 update_check 自动提示；manual 检查全部信任级可用",
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn confirmation_ladder_is_monotonic_strict() {
        // 四档强度互异且额外步数单调不减（official 最弱，remote 最强）。
        let ladder = [
            install_confirmation(TrustLevel::Official),
            install_confirmation(TrustLevel::Community),
            install_confirmation(TrustLevel::Local),
            install_confirmation(TrustLevel::Remote),
        ];
        assert_eq!(ladder[0], InstallConfirmation::Single);
        assert_eq!(ladder[1], InstallConfirmation::Confirm);
        assert_eq!(ladder[2], InstallConfirmation::ConfirmWithProvenance);
        assert_eq!(ladder[3], InstallConfirmation::DoubleConfirmTyped);
        for pair in ladder.windows(2) {
            assert!(pair[0].extra_steps() <= pair[1].extra_steps());
        }
        assert_eq!(ladder[0].extra_steps(), 0);
        assert!(ladder[1].extra_steps() >= 1);
        assert!(ladder[3].extra_steps() >= 1);
    }

    #[test]
    fn d10_only_official_gets_auto_prompt() {
        assert!(auto_update_prompt_allowed(TrustLevel::Official));
        for level in [TrustLevel::Community, TrustLevel::Local, TrustLevel::Remote] {
            assert!(
                !auto_update_prompt_allowed(level),
                "D10: {:?} must not auto-prompt",
                level
            );
        }
    }

    #[test]
    fn remote_requires_typed_id_and_server_fields() {
        let remote = install_confirmation(TrustLevel::Remote);
        assert!(remote.requires_typed_id());
        assert_eq!(remote.required_confirm_fields(), ["confirm", "confirm_id"]);
        let official = install_confirmation(TrustLevel::Official);
        assert!(!official.requires_typed_id());
        assert_eq!(official.required_confirm_fields(), [] as [&str; 0]);
    }

    #[test]
    fn policy_json_covers_all_four_tiers_and_roundtrips() {
        let policy = policy_json();
        let tiers = policy["tiers"].as_array().unwrap();
        assert_eq!(tiers.len(), 4);
        for tier in tiers {
            let word = tier["install_confirmation"].as_str().unwrap();
            assert!(
                InstallConfirmation::parse(word).is_some(),
                "{word} parseable"
            );
            let level = tier["trust_level"].as_str().unwrap();
            // 表内强度与逐档判定一致（单一事实来源不漂移）。
            let expect = install_confirmation(TrustLevel::parse(level).unwrap());
            assert_eq!(expect.as_str(), word);
            assert_eq!(
                tier["auto_update_prompt"].as_bool().unwrap(),
                auto_update_prompt_allowed(TrustLevel::parse(level).unwrap())
            );
        }
        let s = serde_json::to_string(&policy).unwrap();
        assert!(s.contains("double_confirm_typed"));
        assert!(s.contains("confirm_with_provenance"));
    }
}
