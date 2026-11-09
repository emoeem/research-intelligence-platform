//! 预算策略：七分项分配、版本化加载与校验（设计稿 §5.1/§5.3；计划:444
//! 「计算 Token 预算」职责的确定性落面）。
//!
//! 分项口径（设计稿 §5.1 表，默认值随模块常量）：
//!
//! - `l0_rules` = min(l0_cap, total * l0_share)，默认 min(2000, 5% B)，
//!   规则包永不驱逐（设计稿 §5.3 第 1 条）；
//! - 其余六分项按「l0 之外的余量」份额分配，默认 8% / 25% / 10% / 27% /
//!   15% / 15%（l1_catalog / l2_knowledge / l2_tools / l3_reserved /
//!   memory_history / reserve_output）；前五项向下取整，reserve_output 取
//!   余量扣除前五项后的剩余（保证七分项之和恰等于总量，并与设计稿 §9.1
//!   示例数值逐项一致）；
//! - reserve_output 硬下限 10% 余量（设计稿 §5.1 reserve 行「硬下限 10%」），
//!   分配结果低于下限即拒绝（fail fast，不允许静默超预算的姊妹纪律）。
//!
//! 借用规则（设计稿 §5.1 原文）：分项间允许不挪用 reserve_output 的有条件
//! 借用，l3 空闲额度可借给 l2，反向不允许；任何借用不得触碰 reserve_output
//! （[`borrow_allowed`]，装配面 P4-4 执行借用并在 budget_report 披露）。
//!
//! 策略文件为版本化 JSON（policy_version 随装配落 context_manifest，
//! 策略变化即缓存失效，设计稿 §7.2），形状见 [`BudgetPolicy::from_json`]。
//! 加载用 serde_json::Value 手取数（不引 serde derive，锁定最少依赖面，
//! 与 tool-registry 的 Value 手法同款）。

use crate::error::ContextBrokerError;
use serde_json::Value;

/// 默认 l0 规则包封顶（tokens；设计稿 §5.1 l0 行 min 的第一支）。
pub const DEFAULT_L0_RULES_CAP_TOKENS: u64 = 2000;

/// 默认 l0 份额（占总量比例；设计稿 §5.1 l0 行 min 的第二支 5%）。
pub const DEFAULT_L0_RULES_SHARE: f64 = 0.05;

/// 默认 reserve_output 硬下限份额（占余量比例；设计稿 §5.1「硬下限 10%」）。
pub const DEFAULT_RESERVE_OUTPUT_FLOOR_SHARE: f64 = 0.10;

/// 份额和校验容差（JSON 浮点解析的表示误差远小于此）。
const SHARE_SUM_TOLERANCE: f64 = 1e-9;

/// 预算分项（设计稿 §5.1 七分项；as_str 即设计稿 §5.1 表行名，
/// AssembledContext.budget_report 的 JSON 键映射在 P4-4 落位）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BudgetComponent {
    /// 规则包（L0，永不驱逐）。
    L0Rules,
    /// 能力目录卡片（L1）。
    L1Catalog,
    /// 知识候选摘要（L2）。
    L2Knowledge,
    /// 工具 brief（L2）。
    L2Tools,
    /// L3 按需展开的动态额度池。
    L3Reserved,
    /// 任务记忆/历史摘要（Phase 7 接入，先留分项）。
    MemoryHistory,
    /// 输出保留（任何装配不得侵吞，硬下限 10% 余量）。
    ReserveOutput,
}

impl BudgetComponent {
    /// 全部变体（按设计稿 §5.1 表行序）。
    pub const ALL: [BudgetComponent; 7] = [
        BudgetComponent::L0Rules,
        BudgetComponent::L1Catalog,
        BudgetComponent::L2Knowledge,
        BudgetComponent::L2Tools,
        BudgetComponent::L3Reserved,
        BudgetComponent::MemoryHistory,
        BudgetComponent::ReserveOutput,
    ];

    /// 分项名（设计稿 §5.1 表行名，策略文件 shares 键同口径）。
    pub fn as_str(&self) -> &'static str {
        match self {
            BudgetComponent::L0Rules => "l0_rules",
            BudgetComponent::L1Catalog => "l1_catalog",
            BudgetComponent::L2Knowledge => "l2_knowledge",
            BudgetComponent::L2Tools => "l2_tools",
            BudgetComponent::L3Reserved => "l3_reserved",
            BudgetComponent::MemoryHistory => "memory_history",
            BudgetComponent::ReserveOutput => "reserve_output",
        }
    }
}

/// 余量六分项份额（占 l0 之外余量的比例；和必须为 1.0）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ComponentShares {
    /// 能力目录卡片份额（默认 8%）。
    pub l1_catalog: f64,
    /// 知识候选摘要份额（默认 25%）。
    pub l2_knowledge: f64,
    /// 工具 brief 份额（默认 10%）。
    pub l2_tools: f64,
    /// L3 动态额度池份额（默认 27%）。
    pub l3_reserved: f64,
    /// 任务记忆/历史摘要份额（默认 15%）。
    pub memory_history: f64,
    /// 输出保留份额（默认 15%；硬下限见 reserve_output_floor_share）。
    pub reserve_output: f64,
}

impl ComponentShares {
    /// 设计稿 §5.1 默认份额。
    pub fn default_shares() -> Self {
        Self {
            l1_catalog: 0.08,
            l2_knowledge: 0.25,
            l2_tools: 0.10,
            l3_reserved: 0.27,
            memory_history: 0.15,
            reserve_output: 0.15,
        }
    }

    /// 六份额之和（校验用）。
    fn sum(&self) -> f64 {
        self.l1_catalog
            + self.l2_knowledge
            + self.l2_tools
            + self.l3_reserved
            + self.memory_history
            + self.reserve_output
    }
}

/// 版本化预算策略（一经校验通过不可变；装配结果落 context_manifest.policy_version）。
#[derive(Debug, Clone, PartialEq)]
pub struct BudgetPolicy {
    policy_version: String,
    total_tokens: u64,
    l0_rules_cap_tokens: u64,
    l0_rules_share: f64,
    shares: ComponentShares,
    reserve_output_floor_share: f64,
}

/// 一次分配结果（七分项绝对 token 数；七项之和恒等于 total_tokens）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BudgetAllocation {
    /// 总预算（tokens）。
    pub total_tokens: u64,
    /// 规则包分项（min(cap, total * share) 向下取整）。
    pub l0_rules: u64,
    /// 能力目录卡片分项。
    pub l1_catalog: u64,
    /// 知识候选摘要分项。
    pub l2_knowledge: u64,
    /// 工具 brief 分项。
    pub l2_tools: u64,
    /// L3 动态额度池分项。
    pub l3_reserved: u64,
    /// 任务记忆/历史摘要分项。
    pub memory_history: u64,
    /// 输出保留分项（余量扣除前五项的剩余，不低于硬下限）。
    pub reserve_output: u64,
}

impl BudgetAllocation {
    /// 取某分项的绝对额度。
    pub fn get(&self, component: BudgetComponent) -> u64 {
        match component {
            BudgetComponent::L0Rules => self.l0_rules,
            BudgetComponent::L1Catalog => self.l1_catalog,
            BudgetComponent::L2Knowledge => self.l2_knowledge,
            BudgetComponent::L2Tools => self.l2_tools,
            BudgetComponent::L3Reserved => self.l3_reserved,
            BudgetComponent::MemoryHistory => self.memory_history,
            BudgetComponent::ReserveOutput => self.reserve_output,
        }
    }
}

/// 借用规则（设计稿 §5.1 原文：分项间允许不挪用 reserve_output 的有条件
/// 借用，l3 空闲额度可借给 l2，反向不允许）。装配面（P4-4）执行借用前
/// 调用本谓词，借用在 budget_report 披露。
pub fn borrow_allowed(from: BudgetComponent, to: BudgetComponent) -> bool {
    matches!(
        (from, to),
        (
            BudgetComponent::L3Reserved,
            BudgetComponent::L2Knowledge | BudgetComponent::L2Tools
        )
    )
}

impl BudgetPolicy {
    /// 按设计稿 §5.1 默认表构造（l0 min(2000, 5%)；余量六分项 8/25/10/27/15/15）。
    pub fn default_for(total_tokens: u64) -> Result<Self, ContextBrokerError> {
        Self::new(
            "policy-1".to_string(),
            total_tokens,
            DEFAULT_L0_RULES_CAP_TOKENS,
            DEFAULT_L0_RULES_SHARE,
            ComponentShares::default_shares(),
            DEFAULT_RESERVE_OUTPUT_FLOOR_SHARE,
        )
    }

    /// 全参构造并校验（校验拒绝即 [`ContextBrokerError::Policy`]，fail fast）。
    pub fn new(
        policy_version: String,
        total_tokens: u64,
        l0_rules_cap_tokens: u64,
        l0_rules_share: f64,
        shares: ComponentShares,
        reserve_output_floor_share: f64,
    ) -> Result<Self, ContextBrokerError> {
        let policy = Self {
            policy_version,
            total_tokens,
            l0_rules_cap_tokens,
            l0_rules_share,
            shares,
            reserve_output_floor_share,
        };
        policy.validate()?;
        Ok(policy)
    }

    /// 从版本化 JSON 策略文档加载（形状如下，l0 三参可缺省取默认值）：
    ///
    /// ~~~text
    /// {
    ///   "policy_version": "policy-1",
    ///   "total_tokens": 8000,
    ///   "l0_rules_cap_tokens": 2000,
    ///   "l0_rules_share": 0.05,
    ///   "reserve_output_floor_share": 0.10,
    ///   "shares": {
    ///     "l1_catalog": 0.08, "l2_knowledge": 0.25, "l2_tools": 0.10,
    ///     "l3_reserved": 0.27, "memory_history": 0.15, "reserve_output": 0.15
    ///   }
    /// }
    /// ~~~
    ///
    /// shares 六键必填（版本化策略显式声明，不做隐式补齐）；解析或校验
    /// 拒绝一律 [`ContextBrokerError::Policy`]。
    pub fn from_json(json: &str) -> Result<Self, ContextBrokerError> {
        let doc: Value = serde_json::from_str(json).map_err(|e| ContextBrokerError::Policy {
            field: "json".to_string(),
            reason: format!("not valid JSON: {e}"),
        })?;
        let obj = doc.as_object().ok_or_else(|| ContextBrokerError::Policy {
            field: "json".to_string(),
            reason: "top level must be an object".to_string(),
        })?;

        let policy_version = obj
            .get("policy_version")
            .and_then(Value::as_str)
            .ok_or_else(|| ContextBrokerError::Policy {
                field: "policy_version".to_string(),
                reason: "must be a non-empty string".to_string(),
            })?
            .to_string();

        let total_tokens = get_u64(obj, "total_tokens")?;

        let l0_rules_cap_tokens = match obj.get("l0_rules_cap_tokens") {
            None => DEFAULT_L0_RULES_CAP_TOKENS,
            Some(_) => get_u64(obj, "l0_rules_cap_tokens")?,
        };
        let l0_rules_share = match obj.get("l0_rules_share") {
            None => DEFAULT_L0_RULES_SHARE,
            Some(_) => get_f64(obj, "l0_rules_share")?,
        };
        let reserve_output_floor_share = match obj.get("reserve_output_floor_share") {
            None => DEFAULT_RESERVE_OUTPUT_FLOOR_SHARE,
            Some(_) => get_f64(obj, "reserve_output_floor_share")?,
        };

        let shares_val = obj
            .get("shares")
            .ok_or_else(|| ContextBrokerError::Policy {
                field: "shares".to_string(),
                reason: "missing; six component shares are required".to_string(),
            })?;
        let shares_obj = shares_val
            .as_object()
            .ok_or_else(|| ContextBrokerError::Policy {
                field: "shares".to_string(),
                reason: "must be an object".to_string(),
            })?;
        let shares = ComponentShares {
            l1_catalog: get_f64(shares_obj, "l1_catalog")?,
            l2_knowledge: get_f64(shares_obj, "l2_knowledge")?,
            l2_tools: get_f64(shares_obj, "l2_tools")?,
            l3_reserved: get_f64(shares_obj, "l3_reserved")?,
            memory_history: get_f64(shares_obj, "memory_history")?,
            reserve_output: get_f64(shares_obj, "reserve_output")?,
        };

        Self::new(
            policy_version,
            total_tokens,
            l0_rules_cap_tokens,
            l0_rules_share,
            shares,
            reserve_output_floor_share,
        )
    }

    /// 校验（构造时执行；独立成便于测试与 from_json 复用）。
    fn validate(&self) -> Result<(), ContextBrokerError> {
        if self.policy_version.trim().is_empty() {
            return Err(ContextBrokerError::Policy {
                field: "policy_version".to_string(),
                reason: "must be a non-empty string".to_string(),
            });
        }
        if self.total_tokens == 0 {
            return Err(ContextBrokerError::Policy {
                field: "total_tokens".to_string(),
                reason: "must be > 0".to_string(),
            });
        }
        if self.l0_rules_cap_tokens == 0 {
            return Err(ContextBrokerError::Policy {
                field: "l0_rules_cap_tokens".to_string(),
                reason: "must be >= 1 (rules are never evicted, zero cap is meaningless)"
                    .to_string(),
            });
        }
        if !(self.l0_rules_share > 0.0 && self.l0_rules_share <= 1.0) {
            return Err(ContextBrokerError::Policy {
                field: "l0_rules_share".to_string(),
                reason: format!("must be in (0, 1], got {}", self.l0_rules_share),
            });
        }
        for (name, share) in [
            ("l1_catalog", self.shares.l1_catalog),
            ("l2_knowledge", self.shares.l2_knowledge),
            ("l2_tools", self.shares.l2_tools),
            ("l3_reserved", self.shares.l3_reserved),
            ("memory_history", self.shares.memory_history),
            ("reserve_output", self.shares.reserve_output),
        ] {
            if !(0.0..=1.0).contains(&share) {
                return Err(ContextBrokerError::Policy {
                    field: name.to_string(),
                    reason: format!("must be in [0, 1], got {share}"),
                });
            }
        }
        let sum = self.shares.sum();
        if (sum - 1.0).abs() > SHARE_SUM_TOLERANCE {
            return Err(ContextBrokerError::Policy {
                field: "shares".to_string(),
                reason: format!("six shares must sum to 1.0, got {sum}"),
            });
        }
        if !(0.0..=1.0).contains(&self.reserve_output_floor_share) {
            return Err(ContextBrokerError::Policy {
                field: "reserve_output_floor_share".to_string(),
                reason: format!("must be in [0, 1], got {}", self.reserve_output_floor_share),
            });
        }
        if self.reserve_output_floor_share > self.shares.reserve_output {
            return Err(ContextBrokerError::Policy {
                field: "reserve_output_floor_share".to_string(),
                reason: format!(
                    "floor {} must not exceed reserve_output share {}",
                    self.reserve_output_floor_share, self.shares.reserve_output
                ),
            });
        }
        Ok(())
    }

    /// 策略版本（落 context_manifest.policy_version）。
    pub fn policy_version(&self) -> &str {
        &self.policy_version
    }

    /// 总预算（tokens）。
    pub fn total_tokens(&self) -> u64 {
        self.total_tokens
    }

    /// 七分项分配（纯函数；总和恒等于 total_tokens，reserve 不低于硬下限）。
    pub fn allocate(&self) -> Result<BudgetAllocation, ContextBrokerError> {
        // l0 = min(cap, floor(total * share))（设计稿 §5.1 min 语义）。
        let l0_by_share = (self.total_tokens as f64 * self.l0_rules_share).floor() as u64;
        let l0 = l0_by_share.min(self.l0_rules_cap_tokens);
        let remainder = self.total_tokens - l0;
        let floor_share = |share: f64| (share * remainder as f64).floor() as u64;
        let l1 = floor_share(self.shares.l1_catalog);
        let l2k = floor_share(self.shares.l2_knowledge);
        let l2t = floor_share(self.shares.l2_tools);
        let l3 = floor_share(self.shares.l3_reserved);
        let memory = floor_share(self.shares.memory_history);
        // reserve 取余量剩余：保证七分项之和恰等于总量（设计稿 §9.1 示例口径）。
        let reserve = remainder - (l1 + l2k + l2t + l3 + memory);
        let floor_min = (self.reserve_output_floor_share * remainder as f64).floor() as u64;
        if reserve < floor_min {
            return Err(ContextBrokerError::Policy {
                field: "reserve_output".to_string(),
                reason: format!(
                    "allocated {reserve} tokens below hard floor {floor_min} of remainder {remainder}"
                ),
            });
        }
        Ok(BudgetAllocation {
            total_tokens: self.total_tokens,
            l0_rules: l0,
            l1_catalog: l1,
            l2_knowledge: l2k,
            l2_tools: l2t,
            l3_reserved: l3,
            memory_history: memory,
            reserve_output: reserve,
        })
    }
}

/// 从 JSON 对象取必填非负整数字段。
fn get_u64(obj: &serde_json::Map<String, Value>, key: &str) -> Result<u64, ContextBrokerError> {
    match obj.get(key) {
        Some(v) => v.as_u64().ok_or_else(|| ContextBrokerError::Policy {
            field: key.to_string(),
            reason: format!("must be a non-negative integer, got {v}"),
        }),
        None => Err(ContextBrokerError::Policy {
            field: key.to_string(),
            reason: "missing required field".to_string(),
        }),
    }
}

/// 从 JSON 对象取必填数字字段（整数或浮点均可）。
fn get_f64(obj: &serde_json::Map<String, Value>, key: &str) -> Result<f64, ContextBrokerError> {
    match obj.get(key) {
        Some(v) => v.as_f64().ok_or_else(|| ContextBrokerError::Policy {
            field: key.to_string(),
            reason: format!("must be a number, got {v}"),
        }),
        None => Err(ContextBrokerError::Policy {
            field: key.to_string(),
            reason: "missing required field".to_string(),
        }),
    }
}
