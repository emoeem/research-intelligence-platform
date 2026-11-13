//! definition-v1 图 DSL（P8-1）：nodes/edges JSON 文档模型 + JSON Schema 门。
//!
//! 口径（计划 §12:701「WorkflowDefinition：workflow_id、version、graph、
//! input_schema」+ PHASE-ROADMAP §9.3 P8-1）：graph 为节点/边两向量的
//! JSON 文档（参考 LangGraph 状态机建模：START/END 边界 + 条件/并行分支
//! 边词形，节点类型词表对齐计划 Phase 8 五类执行体），执行引擎沿用
//! Rust 自研（本批零执行面，P8-2/P8-3 落执行器）。
//!
//! 三层校验（fail-closed，全在装载期）：
//! 1. serde 形状层：[`parse_definition`] 反序列化（deny_unknown_fields，
//!    形状漂移即拒绝）；
//! 2. Schema 门：definition-v1 JSON Schema（schemas/workflow/
//!    definition-v1.schema.json，编译期 include_str! 内嵌，draft 2020-12，
//!    additionalProperties false + 词表 enum + 0 节点图仅 legacy 标记放行
//!    的条件约束）；
//! 3. 图静态层：[`crate::validate`] 纯函数（环/孤立节点/必填参数/非法边
//!    端点），put 前强制，validate 子命令独立可调。
//!
//! 字节一致性约定：put 存 put 时原文（`store` 不重排不规整），get 回读
//! 逐字节一致；[`canonical_document`] 仅供程序化生成（import-legacy、
//! 测试 fixture）产出确定形状，不进存储回读路径。

use crate::error::WorkflowError;
use domain::{
    WorkflowCompat, WorkflowDefinition, WorkflowEdge, WorkflowEdgeKind, WorkflowId, WorkflowNode,
    WorkflowNodeKind,
};
use serde::{Deserialize, Serialize};

/// definition 面标识（文档 `schema` 字段常量，与 Schema 文件 $id 词形一致）。
pub const DEFINITION_SCHEMA_ID: &str = "workflow/definition-v1";

/// 内嵌 Schema 相对仓库根的目录（编译期路径，仅文档说明用）。
pub const SCHEMA_DIR: &str = "schemas/workflow";

/// definition-v1 JSON Schema（磁盘文件为源，编译期快照；运行期无路径依赖）。
const DEFINITION_SCHEMA: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../schemas/workflow/definition-v1.schema.json"
));

/// 内嵌 Schema 文档视图。
pub fn schema_document() -> &'static str {
    DEFINITION_SCHEMA
}

/// Schema 文档自身合法性门（draft 2020-12 meta-schema 校验内嵌文档结构）。
pub fn check_schema_document() -> Result<(), String> {
    let doc: serde_json::Value = serde_json::from_str(DEFINITION_SCHEMA)
        .map_err(|e| format!("definition-v1 schema not valid JSON: {e}"))?;
    let meta = jsonschema::meta::validator_for(&doc)
        .map_err(|e| format!("definition-v1 not a valid JSON Schema document: {e}"))?;
    meta.validate(&doc)
        .map_err(|e| format!("definition-v1 schema self-check failed: {e}"))?;
    Ok(())
}

/// 编译内嵌 Schema 并校验实例文档（装载门第 2 层）。
fn validate_document(value: &serde_json::Value) -> Result<(), WorkflowError> {
    let schema: serde_json::Value = serde_json::from_str(DEFINITION_SCHEMA)
        .expect("embedded schema snapshot valid JSON (compile-time)");
    let validator = jsonschema::validator_for(&schema)
        .map_err(|e| WorkflowError::Schema(format!("schema compile failed: {e}")))?;
    validator
        .validate(value)
        .map_err(|e| WorkflowError::Schema(e.to_string()))
}

/// 节点类型词表（serde 面，词值与 domain::WorkflowNodeKind 一一对应）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NodeTypeV1 {
    /// 图入口（恰好一个）。
    Start,
    /// 图出口（至少一个）。
    End,
    /// Tool 节点。
    Tool,
    /// Knowledge 节点。
    Knowledge,
    /// Agent 节点。
    Agent,
    /// 人工介入节点（P8-3：停在 awaiting_approval，批准参数注入后恢复续跑）。
    Human,
    /// Condition 节点。
    Condition,
    /// Parallel 节点。
    Parallel,
}

impl NodeTypeV1 {
    /// 词形。
    pub fn as_str(&self) -> &'static str {
        match self {
            NodeTypeV1::Start => "start",
            NodeTypeV1::End => "end",
            NodeTypeV1::Tool => "tool",
            NodeTypeV1::Knowledge => "knowledge",
            NodeTypeV1::Agent => "agent",
            NodeTypeV1::Human => "human",
            NodeTypeV1::Condition => "condition",
            NodeTypeV1::Parallel => "parallel",
        }
    }
}

impl From<NodeTypeV1> for WorkflowNodeKind {
    fn from(value: NodeTypeV1) -> Self {
        match value {
            NodeTypeV1::Start => WorkflowNodeKind::Start,
            NodeTypeV1::End => WorkflowNodeKind::End,
            NodeTypeV1::Tool => WorkflowNodeKind::Tool,
            NodeTypeV1::Knowledge => WorkflowNodeKind::Knowledge,
            NodeTypeV1::Agent => WorkflowNodeKind::Agent,
            NodeTypeV1::Human => WorkflowNodeKind::Human,
            NodeTypeV1::Condition => WorkflowNodeKind::Condition,
            NodeTypeV1::Parallel => WorkflowNodeKind::Parallel,
        }
    }
}

impl From<WorkflowNodeKind> for NodeTypeV1 {
    fn from(value: WorkflowNodeKind) -> Self {
        match value {
            WorkflowNodeKind::Start => NodeTypeV1::Start,
            WorkflowNodeKind::End => NodeTypeV1::End,
            WorkflowNodeKind::Tool => NodeTypeV1::Tool,
            WorkflowNodeKind::Knowledge => NodeTypeV1::Knowledge,
            WorkflowNodeKind::Agent => NodeTypeV1::Agent,
            WorkflowNodeKind::Human => NodeTypeV1::Human,
            WorkflowNodeKind::Condition => NodeTypeV1::Condition,
            WorkflowNodeKind::Parallel => NodeTypeV1::Parallel,
        }
    }
}

/// 边类型词表（serde 面，缺省 seq）。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EdgeKindV1 {
    /// 顺序流边（默认）。
    #[default]
    Seq,
    /// 分支边（condition/parallel 出边）。
    Branch,
}

impl From<EdgeKindV1> for WorkflowEdgeKind {
    fn from(value: EdgeKindV1) -> Self {
        match value {
            EdgeKindV1::Seq => WorkflowEdgeKind::Seq,
            EdgeKindV1::Branch => WorkflowEdgeKind::Branch,
        }
    }
}

impl From<WorkflowEdgeKind> for EdgeKindV1 {
    fn from(value: WorkflowEdgeKind) -> Self {
        match value {
            WorkflowEdgeKind::Seq => EdgeKindV1::Seq,
            WorkflowEdgeKind::Branch => EdgeKindV1::Branch,
        }
    }
}

/// 兼容面标记（serde 面，缺省 graph-v1；词值与 016 CHECK 域一致）。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CompatV1 {
    /// 可执行节点图（默认）。
    #[default]
    GraphV1,
    /// 003 方法层定义只读参考（非可执行）。
    LegacyMethodReference,
}

impl CompatV1 {
    /// 词形（CLI 输出与 016 CHECK 域同词值）。
    pub fn as_str(&self) -> &'static str {
        match self {
            CompatV1::GraphV1 => "graph-v1",
            CompatV1::LegacyMethodReference => "legacy-method-reference",
        }
    }
}

impl From<CompatV1> for WorkflowCompat {
    fn from(value: CompatV1) -> Self {
        match value {
            CompatV1::GraphV1 => WorkflowCompat::GraphV1,
            CompatV1::LegacyMethodReference => WorkflowCompat::LegacyMethodReference,
        }
    }
}

impl From<WorkflowCompat> for CompatV1 {
    fn from(value: WorkflowCompat) -> Self {
        match value {
            WorkflowCompat::GraphV1 => CompatV1::GraphV1,
            WorkflowCompat::LegacyMethodReference => CompatV1::LegacyMethodReference,
        }
    }
}

/// definition-v1 节点。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NodeV1 {
    /// 节点 id（图内唯一，`[A-Za-z0-9][A-Za-z0-9_.-]*`，上限 64）。
    pub id: String,
    /// 节点类型。
    #[serde(rename = "type")]
    pub node_type: NodeTypeV1,
    /// 节点参数（对象；必填参数键按类型词表在静态校验层检查）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub params: Option<serde_json::Map<String, serde_json::Value>>,
}

/// definition-v1 边。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EdgeV1 {
    /// 起点节点 id。
    pub from: String,
    /// 终点节点 id。
    pub to: String,
    /// 边类型（缺省 seq）。
    #[serde(default, skip_serializing_if = "is_default")]
    pub kind: EdgeKindV1,
    /// 条件载荷（branch 边运行期判定的声明式载荷，形状 P8-2 冻结）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub when: Option<serde_json::Value>,
}

fn is_default<T: Default + PartialEq>(value: &T) -> bool {
    *value == T::default()
}

/// definition-v1 graph 块。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GraphV1 {
    /// 节点向量（graph-v1 至少 1 个，Schema 条件约束；legacy 参考行放行空图）。
    pub nodes: Vec<NodeV1>,
    /// 边向量。
    pub edges: Vec<EdgeV1>,
}

/// definition-v1 完整文档模型（serde 面）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DefinitionV1 {
    /// 面标识，恒 [`DEFINITION_SCHEMA_ID`]。
    pub schema: String,
    /// workflow id。
    pub workflow_id: String,
    /// 定义版本（>= 1；put 语义为每版追加）。
    pub version: u32,
    /// 人类可读名。
    pub name: String,
    /// 描述。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// 兼容面标记（缺省 graph-v1）。
    #[serde(default)]
    pub compat: CompatV1,
    /// 输入 Schema（JSON Schema 文档，形状 P8-2 执行器消费）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_schema: Option<serde_json::Value>,
    /// 图。
    pub graph: GraphV1,
}

/// 装载 definition-v1 文档（serde 形状层 + Schema 门）。
///
/// 注意：图静态校验（环/孤立/必填参数/边端点）不在此处，见
/// [`crate::validate::validate_definition`]；put 路径两者都跑。
pub fn parse_definition(text: &str) -> Result<DefinitionV1, WorkflowError> {
    let value: serde_json::Value =
        serde_json::from_str(text).map_err(|e| WorkflowError::Parse(e.to_string()))?;
    let doc: DefinitionV1 =
        serde_json::from_value(value.clone()).map_err(|e| WorkflowError::Parse(e.to_string()))?;
    if doc.schema != DEFINITION_SCHEMA_ID {
        return Err(WorkflowError::Parse(format!(
            "schema field must be {DEFINITION_SCHEMA_ID:?}, got {:?}",
            doc.schema
        )));
    }
    validate_document(&value)?;
    Ok(doc)
}

/// serde 文档 -> domain 向量（字段级校验经 domain try_new；拓扑校验仍属
/// [`crate::validate`]）。
pub fn to_domain(doc: &DefinitionV1) -> Result<WorkflowDefinition, WorkflowError> {
    let mut nodes = Vec::with_capacity(doc.graph.nodes.len());
    for node in &doc.graph.nodes {
        nodes.push(WorkflowNode::try_new(
            node.id.clone(),
            node.node_type.into(),
            node.params
                .as_ref()
                .map(serde_json::to_string)
                .transpose()
                .map_err(|e| WorkflowError::Parse(format!("params serialize failed: {e}")))?,
        )?);
    }
    let mut edges = Vec::with_capacity(doc.graph.edges.len());
    for edge in &doc.graph.edges {
        edges.push(WorkflowEdge::try_new(
            edge.from.clone(),
            edge.to.clone(),
            edge.kind.into(),
            edge.when
                .as_ref()
                .map(serde_json::to_string)
                .transpose()
                .map_err(|e| WorkflowError::Parse(format!("when serialize failed: {e}")))?,
        )?);
    }
    Ok(WorkflowDefinition::try_new(
        WorkflowId::try_new(doc.workflow_id.clone())?,
        doc.version,
        doc.name.clone(),
        doc.description.clone(),
        doc.compat.into(),
        nodes,
        edges,
        doc.input_schema
            .as_ref()
            .map(serde_json::to_string)
            .transpose()
            .map_err(|e| WorkflowError::Parse(format!("input_schema serialize failed: {e}")))?,
    )?)
}

/// domain 向量 -> serde 文档（params/when/input_schema JSON 串回解析）。
pub fn from_domain(def: &WorkflowDefinition) -> Result<DefinitionV1, WorkflowError> {
    let mut nodes = Vec::with_capacity(def.nodes().len());
    for node in def.nodes() {
        let params = match node.params_json() {
            Some(raw) => Some(serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(
                raw,
            )?),
            None => None,
        };
        nodes.push(NodeV1 {
            id: node.id().to_string(),
            node_type: node.kind().into(),
            params,
        });
    }
    let mut edges = Vec::with_capacity(def.edges().len());
    for edge in def.edges() {
        let when = match edge.when_json() {
            Some(raw) => Some(serde_json::from_str::<serde_json::Value>(raw)?),
            None => None,
        };
        edges.push(EdgeV1 {
            from: edge.from().to_string(),
            to: edge.to().to_string(),
            kind: edge.kind().into(),
            when,
        });
    }
    let input_schema = match def.input_schema_json() {
        Some(raw) => Some(serde_json::from_str::<serde_json::Value>(raw)?),
        None => None,
    };
    Ok(DefinitionV1 {
        schema: DEFINITION_SCHEMA_ID.to_string(),
        workflow_id: def.workflow_id().as_str().to_string(),
        version: def.version(),
        name: def.name().to_string(),
        description: def.description().map(str::to_string),
        compat: def.compat().into(),
        input_schema,
        graph: GraphV1 { nodes, edges },
    })
}

/// 程序化生成的确定形状文档（import-legacy 与测试 fixture 用；
/// 单行紧凑 JSON，键序由 serde_json::Map（BTreeMap）决定，确定可重放）。
pub fn canonical_document(def: &WorkflowDefinition) -> Result<String, WorkflowError> {
    serde_json::to_string(&from_domain(def)?)
        .map_err(|e| WorkflowError::Parse(format!("canonical serialize failed: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const FOUR_NODE: &str = r#"{
      "schema": "workflow/definition-v1",
      "workflow_id": "wf-four-node",
      "version": 1,
      "name": "four node demo",
      "graph": {
        "nodes": [
          { "id": "start", "type": "start" },
          { "id": "fetch", "type": "tool", "params": { "tool_id": "webresearch.fetch" } },
          { "id": "summarize", "type": "knowledge", "params": { "query": "summarize findings" } },
          { "id": "done", "type": "end" }
        ],
        "edges": [
          { "from": "start", "to": "fetch" },
          { "from": "fetch", "to": "summarize" },
          { "from": "summarize", "to": "done" }
        ]
      }
    }"#;

    #[test]
    fn schema_document_passes_meta_schema_gate() {
        check_schema_document().expect("embedded schema self-check");
    }

    #[test]
    fn parse_four_node_document_into_domain() {
        let doc = parse_definition(FOUR_NODE).expect("parse");
        assert_eq!(doc.workflow_id, "wf-four-node");
        assert_eq!(doc.compat, CompatV1::GraphV1);
        assert_eq!(doc.graph.nodes.len(), 4);
        assert_eq!(doc.graph.edges.len(), 3);
        let def = to_domain(&doc).expect("to_domain");
        assert_eq!(def.nodes().len(), 4);
        assert_eq!(def.version(), 1);
        // 回转等值（params 串化 -> 回解析无损）
        assert_eq!(from_domain(&def).expect("from_domain"), doc);
    }

    #[test]
    fn parse_rejects_unknown_fields_and_bad_schema_const() {
        assert!(parse_definition("{not json").is_err());
        let doc = parse_definition(FOUR_NODE).expect("parse");
        let mut broken = doc.clone();
        broken.schema = "workflow/definition-v2".to_string();
        let raw = serde_json::to_string(&broken).expect("serialize");
        let err = parse_definition(&raw).expect_err("bad schema const rejected");
        assert!(err.to_string().contains("workflow/definition-v1"));
        // params 子对象是开放面（键集不进 Schema 门），顶层未知键才拒绝：
        let raw = serde_json::to_string(&json!({
            "schema": DEFINITION_SCHEMA_ID,
            "workflow_id": "wf-x",
            "version": 1,
            "name": "x",
            "graph": {"nodes": [], "edges": []},
            "bogus_top": true
        }))
        .expect("serialize");
        assert!(parse_definition(&raw).is_err(), "deny_unknown_fields gate");
    }

    #[test]
    fn canonical_document_is_deterministic() {
        let doc = parse_definition(FOUR_NODE).expect("parse");
        let def = to_domain(&doc).expect("to_domain");
        let a = canonical_document(&def).expect("canonical a");
        let b = canonical_document(&def).expect("canonical b");
        assert_eq!(a, b, "同输入恒同形状（键序确定）");
    }

    #[test]
    fn parse_rejects_out_of_wordtable_node_type() {
        let raw = json!({
            "schema": DEFINITION_SCHEMA_ID,
            "workflow_id": "wf-x",
            "version": 1,
            "name": "x",
            "graph": {
                "nodes": [{ "id": "a", "type": "subgraph" }],
                "edges": []
            }
        });
        // serde 层（词表外类型拒绝）
        let value = raw.clone();
        assert!(serde_json::from_value::<DefinitionV1>(value).is_err());
        // Schema 层（enum 拒绝）
        let schema: serde_json::Value =
            serde_json::from_str(DEFINITION_SCHEMA).expect("schema json");
        let validator =
            jsonschema::validator_for(&schema).expect("schema compile");
        assert!(validator.validate(&raw).is_err());
    }
}
