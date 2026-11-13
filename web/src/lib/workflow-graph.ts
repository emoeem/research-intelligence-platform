/** Workflow Studio 画布模型层（P8-4）：definition-v1 graph 与 @xyflow/react
 *  节点/边互转、确定性分层布局、保存装配。纯函数零 DOM 依赖（jsdom 单测
 *  直接覆盖）。节点类型词表以 workflow-engine DSL 为单源（八类：start/end/
 *  tool/knowledge/agent/human/condition/parallel），计划 §10.4 G 的「数据
 *  转换/证据保存」分类暂无 DSL 词值，面板不伪造（DSL 扩词后在此追加）。 */

import type { Edge, Node } from "@xyflow/react";
import type { WorkflowGraph, WorkflowGraphEdge, WorkflowGraphNode } from "./api";

/** 面板目录（P8-4）：type 为 DSL 词值单源；required 为该类型必填参数键
 *  （workflow-engine validate 同口径：tool 必填 tool_id、knowledge 必填
 *  query、agent 必填 agent_id、condition 必填 expression）。 */
export interface PaletteEntry {
  type: string;
  label: string;
  hint: string;
  required: string[];
  optional: string[];
}

export const NODE_CATALOG: PaletteEntry[] = [
  { type: "start", label: "开始边界", hint: "恰一个，入度起点", required: [], optional: [] },
  { type: "end", label: "结束边界", hint: "至少一个，终态", required: [], optional: [] },
  {
    type: "tool",
    label: "工具调用",
    hint: "tool-engine 派发链",
    required: ["tool_id"],
    optional: ["input"],
  },
  {
    type: "knowledge",
    label: "知识检索",
    hint: "context-broker 装配",
    required: ["query"],
    optional: [],
  },
  {
    type: "agent",
    label: "Agent",
    hint: "AgentLoop 闭环",
    required: ["agent_id"],
    optional: ["goal"],
  },
  {
    type: "human",
    label: "人工介入",
    hint: "awaiting_approval 停等",
    required: [],
    optional: ["prompt", "key"],
  },
  {
    type: "condition",
    label: "条件",
    hint: "jsonschema 谓词，branch 出边",
    required: ["expression"],
    optional: [],
  },
  {
    type: "parallel",
    label: "并行",
    hint: "fork-join 收敛，branch 出边",
    required: [],
    optional: [],
  },
];

export function paletteEntry(type: string): PaletteEntry | undefined {
  return NODE_CATALOG.find((entry) => entry.type === type);
}

/** 条件/并行节点的出边为 branch 词形（definition-v1 冻结口径）。 */
export function edgeKindFor(sourceType: string | undefined): "seq" | "branch" {
  return sourceType === "condition" || sourceType === "parallel" ? "branch" : "seq";
}

/** 确定性分层布局（最长路松弛，层内按 id 字典序排 y）：definition-v1 不存
 *  坐标，同 graph 恒同布局，历史回看与编辑视图不跳动。环图有迭代上限
 *  （节点数）兜底，不死循环。 */
export function layeredPositions(
  nodes: WorkflowGraphNode[],
  edges: WorkflowGraphEdge[],
): Map<string, { x: number; y: number }> {
  const layer = new Map<string, number>();
  for (const node of nodes) layer.set(node.id, 0);
  for (let i = 0; i < nodes.length; i += 1) {
    let changed = false;
    for (const edge of edges) {
      if (!layer.has(edge.from) || !layer.has(edge.to)) continue;
      const next = (layer.get(edge.from) ?? 0) + 1;
      if (next > (layer.get(edge.to) ?? 0) && i < nodes.length) {
        layer.set(edge.to, next);
        changed = true;
      }
    }
    if (!changed) break;
  }
  const byLayer = new Map<number, string[]>();
  for (const node of nodes) {
    const l = layer.get(node.id) ?? 0;
    byLayer.set(l, [...(byLayer.get(l) ?? []), node.id]);
  }
  const positions = new Map<string, { x: number; y: number }>();
  for (const [l, ids] of [...byLayer.entries()].sort((a, b) => a[0] - b[0])) {
    for (const [index, id] of [...ids].sort().entries()) {
      positions.set(id, { x: l * 240, y: index * 130 });
    }
  }
  return positions;
}

/** definition graph → ReactFlow 节点/边（画布消费；type 即自定义节点型）。 */
export function graphToFlow(graph: WorkflowGraph | null | undefined): {
  nodes: Node[];
  edges: Edge[];
} {
  if (!graph || !Array.isArray(graph.nodes) || !Array.isArray(graph.edges)) {
    return { nodes: [], edges: [] };
  }
  const positions = layeredPositions(graph.nodes, graph.edges);
  const nodes: Node[] = graph.nodes.map((node) => ({
    id: node.id,
    type: "wf",
    position: positions.get(node.id) ?? { x: 0, y: 0 },
    data: { nodeType: node.type, params: node.params ?? {} },
  }));
  const validIds = new Set(graph.nodes.map((node) => node.id));
  const edgesOut: Edge[] = graph.edges
    .filter((edge) => validIds.has(edge.from) && validIds.has(edge.to))
    .map((edge, index) => ({
      id: `e-${edge.from}-${edge.to}-${index}`,
      source: edge.from,
      target: edge.to,
      label: edge.kind === "branch" ? "branch" : undefined,
      data: { kind: edge.kind ?? "seq", when: edge.when },
    }));
  return { nodes, edges: edgesOut };
}

/** ReactFlow 节点/边 → definition graph（保存消费；指向缺失节点的边丢弃，
 *  data.kind 缺省 seq）。 */
export function flowToGraph(nodes: Node[], edges: Edge[]): WorkflowGraph {
  const graphNodes: WorkflowGraphNode[] = nodes.map((node) => ({
    id: node.id,
    type: String((node.data as { nodeType?: string } | undefined)?.nodeType ?? "tool"),
    params: ((node.data as { params?: Record<string, unknown> } | undefined)?.params ??
      {}) as Record<string, unknown>,
  }));
  const ids = new Set(graphNodes.map((node) => node.id));
  const graphEdges: WorkflowGraphEdge[] = edges
    .filter((edge) => ids.has(edge.source) && ids.has(edge.target))
    .map((edge) => {
      const data = (edge.data ?? {}) as { kind?: string; when?: unknown };
      return {
        from: edge.source,
        to: edge.target,
        ...(data.kind === "branch" ? { kind: "branch" } : {}),
        ...(data.when !== undefined && data.when !== null ? { when: data.when } : {}),
      };
    });
  return { nodes: graphNodes, edges: graphEdges };
}

/** 保存装配：definition-v1 文档（version 仅作声明随行，存储侧分配递增）。 */
export function buildDefinition(input: {
  workflowId: string;
  name: string;
  description?: string;
  inputSchema: unknown;
  graph: WorkflowGraph;
  version: number;
}): Record<string, unknown> {
  return {
    schema: "workflow/definition-v1",
    workflow_id: input.workflowId,
    version: input.version,
    name: input.name,
    compat: "graph-v1",
    ...(input.description ? { description: input.description } : {}),
    ...(input.inputSchema && typeof input.inputSchema === "object"
      ? { input_schema: input.inputSchema }
      : {}),
    graph: input.graph,
  };
}

/** 保存前客户端预检（与 workflow-engine validate 同口径的提示性检查，最终
 *  以 rp-workflow put 静态校验为准）：非空图、恰一个 start、至少一个 end。 */
export function precheckIssues(graph: WorkflowGraph): string[] {
  const issues: string[] = [];
  if (graph.nodes.length === 0) issues.push("图为空：至少需要一个节点");
  const starts = graph.nodes.filter((node) => node.type === "start").length;
  if (starts !== 1) issues.push(`start 边界须恰一个（现 ${starts} 个）`);
  const ends = graph.nodes.filter((node) => node.type === "end").length;
  if (graph.nodes.length > 0 && ends === 0) issues.push("至少需要一个 end 边界");
  return issues;
}
