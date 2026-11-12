//! 七研究能力确定性 mock 实现（P7-6 交付物本体）。
//!
//! 纪律（SESSION-HANDOFF §十追加项 2）：交付物是「输入输出契约 Schema 加确定性
//! mock 实现加测试」，不是模型调用体；每个能力是一个纯函数（typed 入参进，
//! typed 载荷加摘要加 payload 锚出，无 IO、无时钟、无随机），同输入恒同输出。
//! Phase 8 的 agent_loop 真实接线替换的是实现体，契约面不变。
//!
//! 公共执行面 [`CapabilityExec`]：能力标记类型实现本 trait，由
//! [`crate::contract::execute_request`] 统一分发。

pub mod citation;
pub mod discovery;
pub mod evaluator;
pub mod extraction;
pub mod planner;
pub mod synthesis;
pub mod verification;

use crate::contract::{CapabilityFailure, CapabilityId, Correlation, ProvenanceAnchor};
use serde::de::DeserializeOwned;
use serde::Serialize;

/// 能力执行面：标记类型实现本 trait，纯函数 [`CapabilityExec::run`] 为实现体。
pub trait CapabilityExec {
    /// 能力专属输入载荷（typed）。
    type Input: DeserializeOwned;
    /// 能力专属输出载荷（typed）。
    type Output: Serialize;
    /// 能力标识。
    const ID: CapabilityId;

    /// 纯函数执行：typed 入参进，载荷加摘要加 payload 锚出；
    /// 入参语义非法（Schema 层拦不住的域校验，如 ID 前缀口径）返回 [`CapabilityFailure`]。
    fn run(
        correlation: &Correlation,
        input: Self::Input,
    ) -> Result<(Self::Output, String, Vec<ProvenanceAnchor>), CapabilityFailure>;
}
