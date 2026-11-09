//! 六类上下文源（设计稿 CONTEXT-BROKER-DESIGN §10 sources/ 目录）。
//!
//! P4-2（M4.1）落：[`rulepack`]（L0 规则包）、[`registry`]（L1 目录，直连
//! tool-registry，P4-0 决议 2）、[`plugin`]（Phase 6 前空实现）。
//! P4-3（M4.2）落：[`knowledge`]（platform 知识面经 search-engine）、
//! [`rtis`]（只读子进程 Adapter）。P4-4（M4.3）落：[`artifact`]（只读引用源）。

pub mod artifact;
pub mod knowledge;
pub mod plugin;
pub mod registry;
pub mod rtis;
pub mod rulepack;
