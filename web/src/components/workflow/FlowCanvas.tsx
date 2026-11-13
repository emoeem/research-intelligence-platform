import {
  Background,
  BackgroundVariant,
  Controls,
  Handle,
  MiniMap,
  Position,
  ReactFlow,
  ReactFlowProvider,
  useReactFlow,
  type Edge,
  type EdgeChange,
  type Node,
  type NodeChange,
  type NodeProps,
} from "@xyflow/react";
import "@xyflow/react/dist/style.css";
import { useCallback } from "react";
import { edgeKindFor, paletteEntry } from "../../lib/workflow-graph";
import { PALETTE_DND_TYPE } from "./NodePalette";

/** 第二区中央可视化画布（P8-4，计划 §10.4 G 中栏）：@xyflow/react v12 引擎
 *  （独立 vendor-xyflow 分组，仅本页懒加载消费），自定义 wf 节点（类型徽标
 *  + id + 必填参数摘要），branch 边词形带标签，面板拖放落点经
 *  screenToFlowPosition 换算。只读模式（移动端 / 历史回看）关闭拖拽连线，
 *  保留选择以便查看。 */

type WfNodeData = {
  nodeType: string;
  params: Record<string, unknown>;
};

function WfNode({ data, selected }: NodeProps<Node<WfNodeData>>) {
  const entry = paletteEntry(data.nodeType);
  const paramSummary = Object.keys(data.params ?? {})
    .slice(0, 3)
    .join("、");
  const isStart = data.nodeType === "start";
  const isEnd = data.nodeType === "end";
  return (
    <div
      className={`min-w-[150px] max-w-[220px] rounded-control border bg-base-100 px-3 py-2 shadow-sm ${
        selected ? "border-primary ring-1 ring-primary" : "border-edge"
      }`}
      data-node-type={data.nodeType}
    >
      {!isStart ? <Handle type="target" position={Position.Left} /> : null}
      <p className="text-xs font-semibold">
        <span className="badge badge-xs badge-outline mr-1 align-middle">{data.nodeType}</span>
        {entry?.label ?? data.nodeType}
      </p>
      {paramSummary ? (
        <p className="mt-1 truncate text-[10px] text-base-content/50">{paramSummary}</p>
      ) : null}
      {!isEnd ? <Handle type="source" position={Position.Right} /> : null}
    </div>
  );
}

const nodeTypes = { wf: WfNode };

export interface FlowCanvasProps {
  nodes: Node[];
  edges: Edge[];
  readOnly: boolean;
  selectedEdgeId: string | null;
  onNodesChange: (changes: NodeChange<Node>[]) => void;
  onEdgesChange: (changes: EdgeChange<Edge>[]) => void;
  onConnect: (source: string, target: string) => void;
  onNodeSelect: (nodeId: string | null) => void;
  onEdgeSelect: (edgeId: string | null) => void;
  onDropNode: (type: string, position: { x: number; y: number }) => void;
}

function CanvasInner({
  nodes,
  edges,
  readOnly,
  selectedEdgeId,
  onNodesChange,
  onEdgesChange,
  onConnect,
  onNodeSelect,
  onEdgeSelect,
  onDropNode,
}: FlowCanvasProps) {
  const { screenToFlowPosition } = useReactFlow();

  const onDragOver = useCallback(
    (event: React.DragEvent) => {
      if (readOnly) return;
      event.preventDefault();
      event.dataTransfer.dropEffect = "move";
    },
    [readOnly],
  );

  const onDrop = useCallback(
    (event: React.DragEvent) => {
      if (readOnly) return;
      event.preventDefault();
      const type = event.dataTransfer.getData(PALETTE_DND_TYPE);
      if (!type) return;
      const position = screenToFlowPosition({ x: event.clientX, y: event.clientY });
      onDropNode(type, position);
    },
    [readOnly, screenToFlowPosition, onDropNode],
  );

  const handleConnect = useCallback(
    (params: { source: string | null; target: string | null }) => {
      if (!params.source || !params.target) return;
      onConnect(params.source, params.target);
    },
    [onConnect],
  );

  return (
    <div
      data-testid="workflows-canvas-section"
      aria-label="工作流画布"
      className="relative h-full min-h-0 overflow-hidden rounded-surface border border-edge bg-base-200"
    >
      <ReactFlow
        nodes={nodes}
        edges={edges}
        nodeTypes={nodeTypes}
        onNodesChange={onNodesChange}
        onEdgesChange={onEdgesChange}
        onConnect={handleConnect}
        onNodeClick={(_, node) => onNodeSelect(node.id)}
        onPaneClick={() => {
          onNodeSelect(null);
          onEdgeSelect(null);
        }}
        onEdgeClick={(_, edge) => onEdgeSelect(edge.id)}
        nodesDraggable={!readOnly}
        nodesConnectable={!readOnly}
        edgesFocusable={!readOnly}
        elementsSelectable={true}
        deleteKeyCode={readOnly ? null : ["Backspace", "Delete"]}
        onDrop={onDrop}
        onDragOver={onDragOver}
        fitView
        minZoom={0.2}
        proOptions={{ hideAttribution: false }}
      >
        <Background variant={BackgroundVariant.Dots} gap={18} size={1} />
        <Controls showInteractive={false} position="bottom-left" />
        <MiniMap pannable zoomable position="bottom-right" />
      </ReactFlow>
      {selectedEdgeId && !readOnly ? (
        <span className="pointer-events-none absolute left-2 top-2 z-10 badge badge-ghost badge-sm">
          已选中边（右栏可编辑词形与条件）
        </span>
      ) : null}
    </div>
  );
}

/** 对外包装 Provider（screenToFlowPosition 依赖 flow 实例上下文）。 */
export function FlowCanvas(props: FlowCanvasProps) {
  return (
    <ReactFlowProvider>
      <CanvasInner {...props} />
    </ReactFlowProvider>
  );
}

/** 连线落边装配：kind 按 source 节点词形冻结（condition/parallel 出边为
 *  branch），branch 带 label。导出供页面 onConnect 消费（保持纯函数可测）。 */
export function newEdgeFor(sourceType: string | undefined, source: string, target: string): Edge {
  const kind = edgeKindFor(sourceType);
  return {
    id: `e-${source}-${target}-${Date.now()}`,
    source,
    target,
    label: kind === "branch" ? "branch" : undefined,
    data: { kind, when: undefined },
  };
}
