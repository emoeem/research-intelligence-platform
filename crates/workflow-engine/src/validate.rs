//! graph 静态校验纯函数（P8-1 判据）：环 / 孤立节点 / 必填参数 / 非法边端点。
//!
//! 口径（PHASE-ROADMAP §9.3 P8-1「graph 静态校验（环、孤立节点、必填参数）」
//! + §9.5 风险 2「不为兼容旧形态扭曲 DSL」）：
//! - 规则只看定义文档自身（图拓扑 + 类型词表 + 参数键集），不触存储、
//!   不触运行时（tool/agent 存在性校验属执行器受理面，P8-2/P8-3）；
//! - 一次收集全部问题（不短路），CLI 逐条透传，便于画布编辑一次改完；
//! - 错误文本含 kind 词（`cycle` / `orphan` / `required param` / `illegal
//!   edge endpoint`），machine 可 grep；
//! - `legacy-method-reference` 参考行（空图）整组图规则豁免（规则 0），
//!   该词面语义即「非可执行图，永不进入执行器」。

use domain::{WorkflowCompat, WorkflowDefinition, WorkflowNodeKind};

/// 问题类别（词形进错误文本，machine 可 grep）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum IssueKind {
    /// 环（含自环；跨连通分量也查）。
    Cycle,
    /// 孤立节点（从 start 不可达）。
    Orphan,
    /// 节点缺类型词表要求的必填参数。
    MissingParam,
    /// 边端点指向图中不存在的节点。
    IllegalEndpoint,
    /// 恰好一个 start 的约束违反（0 个或多个）。
    StartBoundary,
    /// 至少一个 end 的约束违反。
    EndBoundary,
    /// 空图（graph-v1 不允许 0 节点）。
    EmptyGraph,
}

impl IssueKind {
    /// 错误文本词形。
    pub fn as_str(&self) -> &'static str {
        match self {
            IssueKind::Cycle => "cycle",
            IssueKind::Orphan => "orphan node",
            IssueKind::MissingParam => "required param",
            IssueKind::IllegalEndpoint => "illegal edge endpoint",
            IssueKind::StartBoundary => "start boundary",
            IssueKind::EndBoundary => "end boundary",
            IssueKind::EmptyGraph => "empty graph",
        }
    }
}

/// 一条静态校验问题（kind + 定位 + 全文；文本以 kind 词形开头）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Issue {
    /// 问题类别。
    pub kind: IssueKind,
    /// 关联节点 id（端点/参数类问题有值）。
    pub node: Option<String>,
    /// 关联边（from, to）。
    pub edge: Option<(String, String)>,
    /// 问题全文（含 kind 词形，可直接透传 stderr/stdout）。
    pub message: String,
}

impl std::fmt::Display for Issue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

/// 节点类型词表要求的必填参数键（P8-1 冻结基线；P8-2/P8-3 执行器按此受理）：
/// - tool 节点必填 `tool_id`（P8-2 经 Tool Engine 执行）；
/// - knowledge 节点必填 `query`（P8-2 经 Broker 检索）；
/// - agent 节点必填 `agent_id`（P8-3 经 agent-engine 执行）；
/// - human 节点无必填键（P8-3：可选 prompt / request / key 三参，缺省
///   即「等待批准」语义，批准参数经 resume 注入）；
/// - condition 节点必填 `expression`（P8-2 分支判定输入）；
/// - parallel 节点扇出走 branch 边，无额外必填键；
/// - start/end 为图边界，无业务参数。
pub fn required_params(kind: WorkflowNodeKind) -> &'static [&'static str] {
    match kind {
        WorkflowNodeKind::Tool => &["tool_id"],
        WorkflowNodeKind::Knowledge => &["query"],
        WorkflowNodeKind::Agent => &["agent_id"],
        WorkflowNodeKind::Human => &[],
        WorkflowNodeKind::Condition => &["expression"],
        WorkflowNodeKind::Start | WorkflowNodeKind::End | WorkflowNodeKind::Parallel => &[],
    }
}

/// 图静态校验主入口：返回全部问题（空向量 = 通过）。
///
/// 分工：身份面校验（节点 id 唯一性等）已在 domain 构造器拒绝，此处不重查；
/// 本函数只做拓扑与词表面规则。
pub fn validate_definition(def: &WorkflowDefinition) -> Vec<Issue> {
    // 规则 0：legacy 参考行豁免图规则（空图合法，永不执行）。
    if def.compat() == WorkflowCompat::LegacyMethodReference {
        return Vec::new();
    }
    let mut issues = Vec::new();
    let nodes = def.nodes();
    let edges = def.edges();

    // 规则 1：graph-v1 空图拒绝。
    if nodes.is_empty() {
        issues.push(Issue {
            kind: IssueKind::EmptyGraph,
            node: None,
            edge: None,
            message: "empty graph: graph-v1 definition requires at least one node \
                      (legacy-method-reference rows are exempt)"
                .to_string(),
        });
        return issues;
    }

    let ids: std::collections::BTreeSet<&str> = nodes.iter().map(|n| n.id()).collect();
    let starts: Vec<&str> = nodes
        .iter()
        .filter(|n| n.kind() == WorkflowNodeKind::Start)
        .map(|n| n.id())
        .collect();
    let ends: Vec<&str> = nodes
        .iter()
        .filter(|n| n.kind() == WorkflowNodeKind::End)
        .map(|n| n.id())
        .collect();

    // 规则 2：恰好一个 start。
    if starts.is_empty() {
        issues.push(Issue {
            kind: IssueKind::StartBoundary,
            node: None,
            edge: None,
            message: "start boundary: graph-v1 definition requires exactly one start node, got 0"
                .to_string(),
        });
    } else if starts.len() > 1 {
        issues.push(Issue {
            kind: IssueKind::StartBoundary,
            node: None,
            edge: None,
            message: format!(
                "start boundary: graph-v1 definition requires exactly one start node, got {}: {}",
                starts.len(),
                starts.join(", ")
            ),
        });
    }

    // 规则 3：至少一个 end。
    if ends.is_empty() {
        issues.push(Issue {
            kind: IssueKind::EndBoundary,
            node: None,
            edge: None,
            message: "end boundary: graph-v1 definition requires at least one end node, got 0"
                .to_string(),
        });
    }

    // 规则 4：非法边端点（from/to 必须指向图中存在的节点）。
    for edge in edges {
        for (field, value) in [("from", edge.from()), ("to", edge.to())] {
            if !ids.contains(value) {
                issues.push(Issue {
                    kind: IssueKind::IllegalEndpoint,
                    node: Some(value.to_string()),
                    edge: Some((edge.from().to_string(), edge.to().to_string())),
                    message: format!(
                        "illegal edge endpoint: edge {}->{} field {field} references missing node {:?}",
                        edge.from(),
                        edge.to(),
                        value
                    ),
                });
            }
        }
    }

    // 规则 5：环检测（三色 DFS，跨连通分量；发现即回溯出路径）。
    if let Some(path) = find_cycle(nodes, edges) {
        issues.push(Issue {
            kind: IssueKind::Cycle,
            node: Some(path.first().cloned().unwrap_or_default()),
            edge: None,
            message: format!("cycle detected: {}", path.join(" -> ")),
        });
    }

    // 规则 6：孤立节点（从 start 不可达；无 start 时跳过，规则 2 已报）。
    if starts.len() == 1 {
        let start = starts[0];
        let reachable = reachable_from(start, nodes, edges);
        let orphans: Vec<&str> = nodes
            .iter()
            .map(|n| n.id())
            .filter(|id| !reachable.contains(*id))
            .collect();
        if !orphans.is_empty() {
            issues.push(Issue {
                kind: IssueKind::Orphan,
                node: Some(orphans[0].to_string()),
                edge: None,
                message: format!(
                    "orphan node(s) unreachable from start {start:?}: {}",
                    orphans.join(", ")
                ),
            });
        }
    }

    // 规则 7：类型词表必填参数键。
    for node in nodes {
        for required in required_params(node.kind()) {
            let present = node
                .params_json()
                .and_then(|raw| serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(raw).ok())
                .map(|params| {
                    params
                        .get(*required)
                        .map(|v| !v.is_null())
                        .unwrap_or(false)
                })
                .unwrap_or(false);
            if !present {
                issues.push(Issue {
                    kind: IssueKind::MissingParam,
                    node: Some(node.id().to_string()),
                    edge: None,
                    message: format!(
                        "required param missing: node {:?} (type {}) requires param {:?}",
                        node.id(),
                        node.kind().as_str(),
                        required
                    ),
                });
            }
        }
    }

    // 稳定输出序：kind 词形升序，再按 message。
    issues.sort_by(|a, b| {
        (a.kind.as_str(), a.message.as_str()).cmp(&(b.kind.as_str(), b.message.as_str()))
    });
    issues
}

/// 三色 DFS 找环（含自环），返回环路径（形如 `a -> b -> a` 的节点序列）；
/// 跨连通分量同样覆盖（每个未染色节点都作 DFS 起点）。确定性：按节点
/// id 升序遍历，同图恒同路径。
fn find_cycle(
    nodes: &[domain::WorkflowNode],
    edges: &[domain::WorkflowEdge],
) -> Option<Vec<String>> {
    let mut adjacency: std::collections::BTreeMap<&str, Vec<&str>> = std::collections::BTreeMap::new();
    for node in nodes {
        adjacency.entry(node.id()).or_default();
    }
    for edge in edges {
        // 目标不存在的端点由规则 4 报告，此处忽略避免 panic。
        if adjacency.contains_key(edge.to()) {
            if let Some(targets) = adjacency.get_mut(edge.from()) {
                targets.push(edge.to());
            }
        }
    }
    // 邻接表目标排序，路径确定。
    for targets in adjacency.values_mut() {
        targets.sort_unstable();
        targets.dedup();
    }

    #[derive(Clone, Copy, PartialEq)]
    enum Color {
        White,
        Gray,
        Black,
    }
    let mut color: std::collections::BTreeMap<&str, Color> =
        adjacency.keys().map(|k| (*k, Color::White)).collect();
    let mut stack: Vec<&str> = Vec::new();

    fn dfs<'a>(
        node: &'a str,
        adjacency: &std::collections::BTreeMap<&'a str, Vec<&'a str>>,
        color: &mut std::collections::BTreeMap<&'a str, Color>,
        stack: &mut Vec<&'a str>,
    ) -> Option<Vec<String>> {
        color.insert(node, Color::Gray);
        stack.push(node);
        if let Some(targets) = adjacency.get(node) {
            for next in targets {
                match color.get(*next) {
                    Some(Color::Gray) => {
                        // 环：从栈中 next 的位置截到栈顶，再闭合到 next。
                        let pos = stack
                            .iter()
                            .position(|n| *n == *next)
                            .unwrap_or_default();
                        let mut path: Vec<String> =
                            stack[pos..].iter().map(|s| (*s).to_string()).collect();
                        path.push((*next).to_string());
                        return Some(path);
                    }
                    Some(Color::Black) | None => {}
                    Some(Color::White) => {
                        if let Some(path) = dfs(next, adjacency, color, stack) {
                            return Some(path);
                        }
                    }
                }
            }
        }
        stack.pop();
        color.insert(node, Color::Black);
        None
    }

    for node in adjacency.keys() {
        if color.get(node) == Some(&Color::White) {
            if let Some(path) = dfs(node, &adjacency, &mut color, &mut stack) {
                return Some(path);
            }
        }
    }
    None
}

/// BFS 可达集（seq 与 branch 边同为流边：静态阶段分支走向未知，两词形
/// 都按可达计；运行期 Skipped 由 P8-2 条件执行器落节点状态）。
fn reachable_from(
    start: &str,
    nodes: &[domain::WorkflowNode],
    edges: &[domain::WorkflowEdge],
) -> std::collections::BTreeSet<String> {
    let mut adjacency: std::collections::BTreeMap<&str, Vec<&str>> = std::collections::BTreeMap::new();
    for node in nodes {
        adjacency.entry(node.id()).or_default();
    }
    for edge in edges {
        if adjacency.contains_key(edge.to()) {
            if let Some(targets) = adjacency.get_mut(edge.from()) {
                targets.push(edge.to());
            }
        }
    }
    let mut seen: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    let mut queue = std::collections::VecDeque::new();
    queue.push_back(start);
    seen.insert(start.to_string());
    while let Some(current) = queue.pop_front() {
        if let Some(targets) = adjacency.get(current) {
            for next in targets {
                if seen.insert((*next).to_string()) {
                    queue.push_back(*next);
                }
            }
        }
    }
    seen
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::{WorkflowEdge, WorkflowEdgeKind, WorkflowId, WorkflowNode, WorkflowNodeState};

    /// 组装 helper：nodes = [(id, kind)]，edges = [(from, to, kind)]。
    fn build_def(
        nodes: Vec<(&str, WorkflowNodeKind, Option<serde_json::Value>)>,
        edges: Vec<(&str, &str, WorkflowEdgeKind)>,
    ) -> WorkflowDefinition {
        let node_vec = nodes
            .into_iter()
            .map(|(id, kind, params)| {
                WorkflowNode::try_new(
                    id,
                    kind,
                    params.map(|p| serde_json::to_string(&p).expect("params json")),
                )
                .expect("node")
            })
            .collect();
        let edge_vec = edges
            .into_iter()
            .map(|(from, to, kind)| {
                WorkflowEdge::try_new(from, to, kind, None).expect("edge")
            })
            .collect();
        WorkflowDefinition::try_new(
            WorkflowId::try_new("wf-test").expect("id"),
            1,
            "test workflow",
            None,
            WorkflowCompat::GraphV1,
            node_vec,
            edge_vec,
            None,
        )
        .expect("definition")
    }

    fn node_param(id: &str, value: serde_json::Value) -> Option<serde_json::Value> {
        let _ = id;
        Some(value)
    }

    #[test]
    fn four_node_happy_path_passes() {
        let def = build_def(
            vec![
                ("start", WorkflowNodeKind::Start, None),
                ("fetch", WorkflowNodeKind::Tool, node_param("fetch", serde_json::json!({"tool_id": "webresearch.fetch"}))),
                ("summarize", WorkflowNodeKind::Knowledge, node_param("summarize", serde_json::json!({"query": "summarize"}))),
                ("done", WorkflowNodeKind::End, None),
            ],
            vec![
                ("start", "fetch", WorkflowEdgeKind::Seq),
                ("fetch", "summarize", WorkflowEdgeKind::Seq),
                ("summarize", "done", WorkflowEdgeKind::Seq),
            ],
        );
        assert_eq!(validate_definition(&def), Vec::new());
    }

    #[test]
    fn cycle_detection_reports_closed_path_with_cycle_word() {
        // start -> a -> b -> a（环）+ b -> end（end 可达，只报环一条）
        let def = build_def(
            vec![
                ("start", WorkflowNodeKind::Start, None),
                ("a", WorkflowNodeKind::Tool, node_param("a", serde_json::json!({"tool_id": "t"}))),
                ("b", WorkflowNodeKind::Knowledge, node_param("b", serde_json::json!({"query": "q"}))),
                ("done", WorkflowNodeKind::End, None),
            ],
            vec![
                ("start", "a", WorkflowEdgeKind::Seq),
                ("a", "b", WorkflowEdgeKind::Seq),
                ("b", "a", WorkflowEdgeKind::Seq),
                ("b", "done", WorkflowEdgeKind::Seq),
            ],
        );
        let issues = validate_definition(&def);
        assert_eq!(issues.len(), 1, "只报环一条: {issues:?}");
        assert_eq!(issues[0].kind, IssueKind::Cycle);
        assert!(issues[0].message.contains("cycle"), "{}", issues[0].message);
        assert_eq!(issues[0].message, "cycle detected: a -> b -> a");
    }

    #[test]
    fn self_loop_is_reported_as_cycle() {
        let def = build_def(
            vec![
                ("start", WorkflowNodeKind::Start, None),
                ("a", WorkflowNodeKind::Tool, node_param("a", serde_json::json!({"tool_id": "t"}))),
                ("done", WorkflowNodeKind::End, None),
            ],
            vec![
                ("start", "a", WorkflowEdgeKind::Seq),
                ("a", "a", WorkflowEdgeKind::Seq),
                ("a", "done", WorkflowEdgeKind::Seq),
            ],
        );
        let issues = validate_definition(&def);
        assert_eq!(issues.len(), 1);
        assert!(issues[0].message.contains("cycle detected: a -> a"));
    }

    #[test]
    fn orphan_nodes_unreachable_from_start_are_reported() {
        // x 无人指向（孤立）；start -> y -> done 正常链
        let def = build_def(
            vec![
                ("start", WorkflowNodeKind::Start, None),
                ("x", WorkflowNodeKind::Tool, node_param("x", serde_json::json!({"tool_id": "t"}))),
                ("y", WorkflowNodeKind::Knowledge, node_param("y", serde_json::json!({"query": "q"}))),
                ("done", WorkflowNodeKind::End, None),
            ],
            vec![
                ("start", "y", WorkflowEdgeKind::Seq),
                ("y", "done", WorkflowEdgeKind::Seq),
            ],
        );
        let issues = validate_definition(&def);
        assert_eq!(issues.len(), 1, "{issues:?}");
        assert!(issues[0].message.contains("orphan node"), "{}", issues[0].message);
        assert!(issues[0].message.ends_with(": x"), "{}", issues[0].message);
        assert_eq!(issues[0].node.as_deref(), Some("x"));
    }

    #[test]
    fn missing_required_params_per_node_type_wordtable() {
        // tool 缺 tool_id、knowledge 缺 query、condition 缺 expression
        let def = build_def(
            vec![
                ("start", WorkflowNodeKind::Start, None),
                ("t1", WorkflowNodeKind::Tool, None),
                ("k1", WorkflowNodeKind::Knowledge, Some(serde_json::json!({"other": 1}))),
                ("c1", WorkflowNodeKind::Condition, Some(serde_json::json!({"expression": null}))),
                ("done", WorkflowNodeKind::End, None),
            ],
            vec![
                ("start", "t1", WorkflowEdgeKind::Seq),
                ("start", "k1", WorkflowEdgeKind::Branch),
                ("start", "c1", WorkflowEdgeKind::Branch),
                ("t1", "done", WorkflowEdgeKind::Seq),
                ("k1", "done", WorkflowEdgeKind::Seq),
                ("c1", "done", WorkflowEdgeKind::Branch),
            ],
        );
        let issues = validate_definition(&def);
        let missing: Vec<&str> = issues
            .iter()
            .filter(|i| i.kind == IssueKind::MissingParam)
            .map(|i| i.message.as_str())
            .collect();
        assert_eq!(missing.len(), 3, "{missing:?}");
        assert!(missing.iter().any(|m| m.contains("requires param \"tool_id\"")));
        assert!(missing.iter().any(|m| m.contains("requires param \"query\"")));
        assert!(missing.iter().any(|m| m.contains("requires param \"expression\"")));
    }

    #[test]
    fn illegal_edge_endpoints_reference_missing_nodes() {
        let def = build_def(
            vec![
                ("start", WorkflowNodeKind::Start, None),
                ("done", WorkflowNodeKind::End, None),
            ],
            vec![
                ("start", "done", WorkflowEdgeKind::Seq),
                ("start", "ghost", WorkflowEdgeKind::Seq),
                ("phantom", "done", WorkflowEdgeKind::Seq),
            ],
        );
        let issues = validate_definition(&def);
        let endpoint: Vec<&Issue> = issues
            .iter()
            .filter(|i| i.kind == IssueKind::IllegalEndpoint)
            .collect();
        assert_eq!(endpoint.len(), 2, "{issues:?}");
        assert!(endpoint.iter().all(|i| i.message.contains("illegal edge endpoint")));
        assert!(endpoint.iter().any(|i| i.message.contains("references missing node \"ghost\"")));
        assert!(endpoint.iter().any(|i| i.message.contains("references missing node \"phantom\"")));
    }

    #[test]
    fn start_end_boundary_and_empty_graph_rules() {
        // 0 start（两个 tool 互连成环会先报环；改用无环两节点）
        let def = build_def(
            vec![
                ("a", WorkflowNodeKind::Tool, node_param("a", serde_json::json!({"tool_id": "t"}))),
                ("b", WorkflowNodeKind::Knowledge, node_param("b", serde_json::json!({"query": "q"}))),
            ],
            vec![("a", "b", WorkflowEdgeKind::Seq)],
        );
        let issues = validate_definition(&def);
        assert!(issues.iter().any(|i| i.kind == IssueKind::StartBoundary));
        assert!(issues.iter().any(|i| i.kind == IssueKind::EndBoundary));
        // 孤立集（无 start 跳过规则 6）不应出现
        assert!(!issues.iter().any(|i| i.kind == IssueKind::Orphan));

        // 空图
        let empty = build_def(vec![], vec![]);
        let issues = validate_definition(&empty);
        assert_eq!(issues.len(), 1);
        assert!(issues[0].message.contains("empty graph"));
    }

    #[test]
    fn legacy_method_reference_rows_exempt_from_graph_rules() {
        let node_vec = vec![WorkflowNode::try_new("a", WorkflowNodeKind::Tool, None).expect("n")];
        let def = WorkflowDefinition::try_new(
            WorkflowId::try_new("WF-00").expect("id"),
            1,
            "legacy reference",
            None,
            WorkflowCompat::LegacyMethodReference,
            node_vec,
            vec![WorkflowEdge::try_new("a", "ghost", WorkflowEdgeKind::Seq, None).expect("e")],
            None,
        )
        .expect("definition");
        assert_eq!(validate_definition(&def), Vec::new(), "legacy 参考行豁免图规则");
    }

    #[test]
    fn branch_edges_count_as_flow_for_reachability() {
        // parallel 扇出走 branch 边，两端可达不算孤立
        let def = build_def(
            vec![
                ("start", WorkflowNodeKind::Start, None),
                ("fan", WorkflowNodeKind::Parallel, None),
                ("w1", WorkflowNodeKind::Tool, node_param("w1", serde_json::json!({"tool_id": "t1"}))),
                ("w2", WorkflowNodeKind::Tool, node_param("w2", serde_json::json!({"tool_id": "t2"}))),
                ("done", WorkflowNodeKind::End, None),
            ],
            vec![
                ("start", "fan", WorkflowEdgeKind::Seq),
                ("fan", "w1", WorkflowEdgeKind::Branch),
                ("fan", "w2", WorkflowEdgeKind::Branch),
                ("w1", "done", WorkflowEdgeKind::Seq),
                ("w2", "done", WorkflowEdgeKind::Seq),
            ],
        );
        assert_eq!(validate_definition(&def), Vec::new());
    }

    #[test]
    fn required_params_wordtable_is_frozen() {
        assert_eq!(required_params(WorkflowNodeKind::Tool), &["tool_id"]);
        assert_eq!(required_params(WorkflowNodeKind::Knowledge), &["query"]);
        assert_eq!(required_params(WorkflowNodeKind::Agent), &["agent_id"]);
        assert_eq!(required_params(WorkflowNodeKind::Human), &[] as &[&str]);
        assert_eq!(required_params(WorkflowNodeKind::Condition), &["expression"]);
        assert_eq!(required_params(WorkflowNodeKind::Parallel), &[] as &[&str]);
        assert_eq!(required_params(WorkflowNodeKind::Start), &[] as &[&str]);
        assert_eq!(required_params(WorkflowNodeKind::End), &[] as &[&str]);
    }

    #[test]
    fn node_state_helpers_align_with_definition_ids() {
        // WorkflowNodeState 复用同款节点 id 校验（形状一致性冒烟）
        assert!(WorkflowNodeState::try_new("start", domain::WorkflowNodeRunState::Pending, 0).is_ok());
        assert!(WorkflowNodeState::try_new("bad id", domain::WorkflowNodeRunState::Pending, 0).is_err());
    }
}
