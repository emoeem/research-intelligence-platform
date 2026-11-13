//! Phase 8 Workflow Studio 引擎 crate（ROADMAP §9.3 P8-1/P8-2）。
//!
//! 职责：
//! - [`dsl`]：definition-v1 图 DSL（nodes/edges JSON 文档模型 + JSON Schema
//!   门，`schemas/workflow/definition-v1.schema.json` 编译期内嵌）；
//!   参考 LangGraph 状态机建模（START/END 边界 + seq/branch 边词形）；
//! - [`validate`]：graph 静态校验纯函数（环 / 孤立节点 / 必填参数 / 非法边
//!   端点 + start/end 边界 + 空图 + legacy 豁免），一次收集全部问题；
//! - [`store`]：workflow_definition 版本化存储（016 迁移，workflow_id +
//!   version 复合主键，put 每版追加历史不可变，get 回读逐字节一致）+
//!   003 存量行只读参考导入（import-legacy，compat=legacy-method-reference）；
//! - [`state`]：workflow 运行持久化（017 迁移三表：workflow_run 六态 +
//!   workflow_run_node 五态 attempt + workflow_checkpoint 每步快照只追加），
//!   乐观跃迁守卫 + 快照恢复 + 恢复态合并；
//! - [`exec`]：执行引擎与四类非智能节点执行器（P8-2）：Tool（tool-engine
//!   派发链，运行面 [`exec::node_tool::ToolRunDriver`] 端口注入）、
//!   Knowledge（context-broker Broker::assemble 检索）、Condition（jsonschema
//!   纯函数谓词）、Parallel（fork-join 并发分支 join 收敛）；run 循环
//!   fail-fast，节点完成即落 checkpoint，节点超时显式 failed 终态
//!   （code=node_timeout）；失败重试以新 run 承载（只重跑失败节点，
//!   attempt 递增），resume 从最近 checkpoint 续跑（已完成节点不重跑）；
//! - [`bin/rp-workflow`]：validate / put / get / list / import-legacy /
//!   run / status / nodes / retry / resume / cancel 子命令（ROADMAP §9.4
//!   验收命令入口）。
//!
//! 边界（批次冻结）：
//! - Agent 节点执行器属 P8-3（Phase 7 agent-engine 为执行体）；含 agent
//!   节点的定义在本批执行器受理面即拒绝（fail-closed，词表已冻结）；
//! - 反方向依赖禁止：tool-engine / context-broker / agent-engine 不依赖
//!   workflow-engine（先例 research-engine 同款）；
//! - legacy-method-reference 参考行（003 存量 13 行）永不进入执行器：
//!   compat 非 graph-v1 的定义 start 即拒绝。

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod control;
pub mod dsl;
pub mod error;
pub mod exec;
pub mod state;
pub mod store;
pub mod validate;

pub use control::{ControlRow, WorkflowControl};
pub use dsl::{
    canonical_document, check_schema_document, from_domain, parse_definition, schema_document,
    to_domain, CompatV1, DefinitionV1, EdgeKindV1, EdgeV1, GraphV1, NodeV1, NodeTypeV1,
    DEFINITION_SCHEMA_ID, SCHEMA_DIR,
};
pub use error::WorkflowError;
pub use exec::{
    node_agent, RunOutcome, WorkflowExecutor, DEFAULT_NODE_TIMEOUT_MS,
};
pub use state::{
    snapshot_node_states, CheckpointRow, NodeEntityColumn, NodeRow, NodeSnapshot, RunEntityColumn,
    RunRow, RunStore,
};
pub use store::{
    readonly_pool, writable_pool, LegacyImportReport, ListRow, PutOutcome, StoredDefinition,
    WorkflowStore,
};
pub use validate::{validate_definition, Issue, IssueKind};
