import {
  addEdge,
  useEdgesState,
  useNodesState,
  type Edge,
  type EdgeChange,
  type Node,
  type NodeChange,
} from "@xyflow/react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { FlowCanvas, newEdgeFor } from "../components/workflow/FlowCanvas";
import { NodeInspector } from "../components/workflow/NodeInspector";
import { NodePalette, PALETTE_DND_TYPE } from "../components/workflow/NodePalette";
import { RunPanel, StateBadge } from "../components/workflow/RunPanel";
import { VersionList } from "../components/workflow/VersionList";
import { ErrorPanel, PageHeader } from "../components/ui";
import { useApi } from "../lib/hooks";
import {
  fetchWorkflowRunDetail,
  fetchWorkflowRuns,
  fetchWorkflows,
  workflowPut,
  workflowRun,
  workflowRunAction,
  ApiError,
  type WorkflowList,
  type WorkflowRunDetail,
  type WorkflowRunList,
} from "../lib/api";
import { buildDefinition, flowToGraph, graphToFlow, precheckIssues } from "../lib/workflow-graph";

/** Workflow Studio 实页（P8-4，计划 §10.4 G 五区壳层）：
 *  1. 左 节点分类面板（NodePalette，DSL 词表单源）；
 *  2. 中 可视化画布（FlowCanvas，@xyflow/react v12，vendor-xyflow 独立分组）；
 *  3. 右 节点参数 Schema 表单与运行配置（NodeInspector）；
 *  4. 底 执行日志、节点状态与产物（RunPanel，run/pause/resume/retry 接
 *     P8-3 API，活动 run 2s 轮询）；
 *  5. 版本列表与历史回看（VersionList，保存走 P8-4 put 版本存储侧递增）。
 *  移动端（<768px）只读降级：画布关闭拖拽连线，显示切换宽屏提示（计划
 *  §10.5）；历史回看版本同样只读。 */

const MOBILE_QUERY = "(max-width: 767px)";

function useIsNarrowViewport(): boolean {
  const [narrow, setNarrow] = useState<boolean>(() => {
    if (typeof window === "undefined" || typeof window.matchMedia !== "function") return false;
    return window.matchMedia(MOBILE_QUERY).matches;
  });
  useEffect(() => {
    if (typeof window.matchMedia !== "function") return;
    const mql = window.matchMedia(MOBILE_QUERY);
    const onChange = (event: MediaQueryListEvent) => setNarrow(event.matches);
    mql.addEventListener("change", onChange);
    return () => mql.removeEventListener("change", onChange);
  }, []);
  return narrow;
}

function uniqueNodeId(nodes: Node[], type: string): string {
  const base = type === "start" || type === "end" ? type : `${type}-1`;
  if (!nodes.some((node) => node.id === base)) return base;
  let n = 2;
  while (nodes.some((node) => node.id === `${type}-${n}`)) n += 1;
  return `${type}-${n}`;
}

export function WorkflowsPage() {
  const narrow = useIsNarrowViewport();

  // 数据面：定义列表（含 ?id= 详情逐版 graph）+ run 列表
  const defs = useApi<WorkflowList>("/api/workflows?limit=100", {
    fetcher: (_path, signal) => fetchWorkflows("", { signal }),
  });
  const runs = useApi<WorkflowRunList>("/api/workflow-runs?limit=50", {
    fetcher: (_path, signal) => fetchWorkflowRuns("", { signal }),
  });

  // 选择态
  const [workflowId, setWorkflowId] = useState("");
  const [selectedVersion, setSelectedVersion] = useState<number | null>(null);
  const [selectedNodeId, setSelectedNodeId] = useState<string | null>(null);
  const [selectedEdgeId, setSelectedEdgeId] = useState<string | null>(null);
  const [selectedRunId, setSelectedRunId] = useState<string | null>(null);

  // 画布态（RF 托管）
  const [nodes, setNodes, onNodesChangeBase] = useNodesState<Node>([]);
  const [edges, setEdges, onEdgesChangeBase] = useEdgesState<Edge>([]);
  const [dirty, setDirty] = useState(false);

  // 保存态
  const [saving, setSaving] = useState(false);
  const [saveMessage, setSaveMessage] = useState<string | null>(null);
  const [saveIssues, setSaveIssues] = useState<string[]>([]);

  // 运行配置态
  const [runVersion, setRunVersion] = useState<number | null>(null);
  const [runInput, setRunInput] = useState("{}");
  const [runBusy, setRunBusy] = useState(false);
  const [runError, setRunError] = useState<string | null>(null);
  const [busyAction, setBusyAction] = useState<string | null>(null);
  const [injectInput, setInjectInput] = useState('{"approval": {"ok": true}}');
  const [actionError, setActionError] = useState<string | null>(null);

  // workflow 详情（逐版 graph，历史回看渲染消费；workflowId 空则不取）
  const detail = useApi<WorkflowList>(
    workflowId ? `/api/workflows?id=${encodeURIComponent(workflowId)}` : null,
    { fetcher: (_path, signal) => fetchWorkflows(workflowId, { signal }) },
  );

  // run 详情（节点行/控制行/六 ID；活动 run 2s 轮询）
  const runDetail = useApi<WorkflowRunDetail>(
    selectedRunId ? `/api/workflow-runs?id=${encodeURIComponent(selectedRunId)}` : null,
    { fetcher: (_path, signal) => fetchWorkflowRunDetail(selectedRunId ?? "", { signal }) },
  );
  const runState = runDetail.data?.run?.state ?? null;
  const runDetailRetry = runDetail.retry;
  // run 受理/控制动作均为异步承载（P8-3）：run 行落库晚于受理响应（详情首拉
  // 可能得到「无 run 行」的列表形状），resume/retry 后单次刷新也可能仍读到
  // 旧态（paused）而错过 running 窗口。轮询条件三路并集：无行有界补拉
  // （800ms，最多 10 次）、活动 run 2s 轮询（queued/running）、动作后脉冲
  // 轮询（800ms，8s 窗口，动作受理处 bump）。
  const detailMissRef = useRef(0);
  const pulseUntilRef = useRef(0);
  const [actionPulse, setActionPulse] = useState(0);
  const bumpPulse = useCallback(() => {
    pulseUntilRef.current = Date.now() + 8000;
    setActionPulse((n) => n + 1);
  }, []);
  useEffect(() => {
    detailMissRef.current = 0;
  }, [selectedRunId]);
  useEffect(() => {
    if (!selectedRunId) return;
    const missingRow = !runDetail.data?.run;
    const activeState = runState !== null && ["queued", "running"].includes(runState);
    const withinPulse = Date.now() < pulseUntilRef.current;
    const missing = missingRow && detailMissRef.current < 10;
    if (!activeState && !withinPulse && !missing) return;
    if (missingRow) detailMissRef.current += 1;
    const timer = setInterval(() => runDetailRetry(), activeState ? 2000 : 800);
    return () => clearInterval(timer);
  }, [selectedRunId, runState, runDetail.data, runDetailRetry, actionPulse]);

  const versions = useMemo(
    () =>
      (detail.data?.items ?? [])
        .filter((row) => row.workflow_id === workflowId)
        .sort((a, b) => a.version - b.version),
    [detail.data, workflowId],
  );
  const workflowOptions = useMemo(() => {
    const ids = new Set((defs.data?.items ?? []).map((row) => row.workflow_id));
    if (workflowId.trim()) ids.add(workflowId.trim());
    return [...ids].sort();
  }, [defs.data, workflowId]);
  const latestVersion = useMemo(
    () => (versions.length > 0 ? Math.max(...versions.map((row) => row.version)) : null),
    [versions],
  );

  // 选中 workflow / 版本变化时装载画布（历史回看渲染该版 graph，编辑态渲染
  // 最新版 graph；同输入恒同布局）。
  const versionGraph = useMemo(() => {
    if (selectedVersion === null) return null;
    return versions.find((row) => row.version === selectedVersion)?.graph ?? null;
  }, [selectedVersion, versions]);
  const latestGraph = useMemo(() => {
    if (latestVersion === null) return null;
    return versions.find((row) => row.version === latestVersion)?.graph ?? null;
  }, [latestVersion, versions]);
  useEffect(() => {
    const graph = selectedVersion !== null ? versionGraph : latestGraph;
    const flow = graphToFlow(graph);
    setNodes(flow.nodes);
    setEdges(flow.edges);
    setDirty(false);
    setSelectedNodeId(null);
    setSelectedEdgeId(null);
    // setNodes/setEdges 为稳定引用（xyflow useState 封装），不进依赖。
  }, [selectedVersion, versionGraph, latestGraph, setNodes, setEdges]);

  const readOnly = narrow || selectedVersion !== null;

  // ---- 画布操作 ----
  const changesDirty = (changes: (NodeChange<Node> | EdgeChange<Edge>)[]): boolean =>
    changes.some((change) => change.type !== "select" && change.type !== "dimensions");

  const onNodesChange = useCallback(
    (changes: NodeChange<Node>[]) => {
      if (changesDirty(changes)) setDirty(true);
      onNodesChangeBase(changes);
    },
    [onNodesChangeBase],
  );
  const onEdgesChange = useCallback(
    (changes: EdgeChange<Edge>[]) => {
      if (changesDirty(changes)) setDirty(true);
      onEdgesChangeBase(changes);
    },
    [onEdgesChangeBase],
  );

  const addNodeAt = useCallback(
    (type: string, position?: { x: number; y: number }) => {
      if (readOnly) return;
      setNodes((current) => {
        const id = uniqueNodeId(current, type);
        return [
          ...current,
          {
            id,
            type: "wf",
            position: position ?? {
              x: 40 + ((current.length * 37) % 260),
              y: 40 + ((current.length * 61) % 300),
            },
            data: { nodeType: type, params: {} },
          },
        ];
      });
      setDirty(true);
    },
    [readOnly, setNodes],
  );

  const onConnect = useCallback(
    (source: string, target: string) => {
      if (readOnly || source === target) return;
      setEdges((current) => {
        const duplicate = current.some((edge) => edge.source === source && edge.target === target);
        if (duplicate) return current;
        const sourceType = nodes.find((node) => node.id === source)?.data as
          { nodeType?: string } | undefined;
        return addEdge(newEdgeFor(sourceType?.nodeType, source, target), current);
      });
      setDirty(true);
    },
    [readOnly, nodes, setEdges],
  );

  const onDropNode = useCallback(
    (type: string, position: { x: number; y: number }) => addNodeAt(type, position),
    [addNodeAt],
  );

  const onDragStart = useCallback((event: React.DragEvent, type: string) => {
    event.dataTransfer.setData(PALETTE_DND_TYPE, type);
    event.dataTransfer.effectAllowed = "move";
  }, []);

  const onNodePatch = useCallback(
    (nodeId: string, params: Record<string, unknown>) => {
      setNodes((current) =>
        current.map((node) =>
          node.id === nodeId
            ? { ...node, data: { ...(node.data as object), params } as Node["data"] }
            : node,
        ),
      );
      setDirty(true);
    },
    [setNodes],
  );

  const onNodeIdChange = useCallback(
    (oldId: string, newId: string) => {
      setNodes((current) =>
        current.map((node) => (node.id === oldId ? { ...node, id: newId } : node)),
      );
      setEdges((current) =>
        current.map((edge) => ({
          ...edge,
          source: edge.source === oldId ? newId : edge.source,
          target: edge.target === oldId ? newId : edge.target,
        })),
      );
      setSelectedNodeId((current) => (current === oldId ? newId : current));
      setDirty(true);
    },
    [setNodes, setEdges],
  );

  const onNodeRemove = useCallback(
    (nodeId: string) => {
      setNodes((current) => current.filter((node) => node.id !== nodeId));
      setEdges((current) =>
        current.filter((edge) => edge.source !== nodeId && edge.target !== nodeId),
      );
      setSelectedNodeId((current) => (current === nodeId ? null : current));
      setDirty(true);
    },
    [setNodes, setEdges],
  );

  const onEdgePatch = useCallback(
    (edgeId: string, patch: { kind?: string; when?: unknown }) => {
      setEdges((current) =>
        current.map((edge) => {
          if (edge.id !== edgeId) return edge;
          const data = { ...(edge.data as object), ...patch } as Edge["data"];
          return {
            ...edge,
            data,
            label: (data as { kind?: string }).kind === "branch" ? "branch" : undefined,
          };
        }),
      );
      setDirty(true);
    },
    [setEdges],
  );

  const selectedNode = useMemo(
    () => nodes.find((node) => node.id === selectedNodeId) ?? null,
    [nodes, selectedNodeId],
  );
  const selectedEdge = useMemo(
    () => edges.find((edge) => edge.id === selectedEdgeId) ?? null,
    [edges, selectedEdgeId],
  );

  // ---- 保存（P8-4 put：画布图转 definition JSON，版本存储侧递增） ----
  const onSave = useCallback(async () => {
    if (!workflowId.trim()) {
      setSaveIssues(["workflow_id 为空：在版本区键入 workflow_id 后保存"]);
      return;
    }
    const graph = flowToGraph(nodes, edges);
    const issues = precheckIssues(graph);
    if (issues.length > 0) {
      setSaveIssues(issues);
      return;
    }
    setSaveIssues([]);
    setSaving(true);
    setSaveMessage(null);
    try {
      const latestRow = versions.find((row) => row.version === latestVersion);
      const definition = buildDefinition({
        workflowId: workflowId.trim(),
        name: latestRow?.name || workflowId.trim(),
        description: latestRow?.description ?? undefined,
        inputSchema: latestRow?.input_schema,
        graph,
        version: (latestVersion ?? 0) + 1,
      });
      const result = await workflowPut(definition);
      setSaveMessage(
        `已保存 ${result.workflow_id} @ v${result.version}（${result.nodes ?? "?"} 节点 ${
          result.edges ?? "?"
        } 边）`,
      );
      setDirty(false);
      await defs.retry();
      await detail.retry();
    } catch (error) {
      setSaveMessage(null);
      setSaveIssues([
        error instanceof ApiError
          ? `保存被拒绝（HTTP ${error.status}）：${error.message}`
          : `保存失败：${String(error)}`,
      ]);
    } finally {
      setSaving(false);
    }
  }, [workflowId, nodes, edges, versions, latestVersion, defs, detail]);

  // ---- 运行（P8-3 run 受理面） ----
  const onRun = useCallback(async () => {
    setRunError(null);
    let input: Record<string, unknown> = {};
    const trimmed = runInput.trim();
    if (trimmed) {
      try {
        input = JSON.parse(trimmed) as Record<string, unknown>;
      } catch {
        setRunError("input 不是合法 JSON object：修正后再运行");
        return;
      }
    }
    setRunBusy(true);
    try {
      const accepted = await workflowRun(workflowId.trim(), {
        input,
        version: runVersion ?? undefined,
      });
      setSelectedRunId(accepted.run_id);
      bumpPulse();
      await runs.retry();
      // run 行落库晚于受理响应：延迟再刷一次列表（列表条目补齐）。
      setTimeout(() => void runs.retry(), 2000);
    } catch (error) {
      setRunError(
        error instanceof ApiError
          ? `运行受理失败（HTTP ${error.status}）：${error.message}`
          : `运行失败：${String(error)}`,
      );
    } finally {
      setRunBusy(false);
    }
  }, [runInput, workflowId, runVersion, runs, bumpPulse]);

  // ---- 控制动作（P8-3 pause/resume/retry） ----
  const onAction = useCallback(
    async (action: "pause" | "resume" | "retry", nodeId?: string) => {
      if (!selectedRunId) return;
      setActionError(null);
      setBusyAction(action);
      try {
        if (action === "pause") {
          await workflowRunAction("pause", selectedRunId);
        } else if (action === "resume") {
          const trimmed = injectInput.trim();
          const inject = trimmed ? (JSON.parse(trimmed) as Record<string, unknown>) : undefined;
          await workflowRunAction("resume", selectedRunId, inject ? { inject } : {});
        } else if (action === "retry" && nodeId) {
          await workflowRunAction("retry", selectedRunId, { node_id: nodeId });
        }
        await runDetail.retry();
        await runs.retry();
        // resume/retry 异步承载：受理响应时 run 行可能仍是旧态，动作后脉冲
        // 轮询覆盖 running 窗口直至终态（轮询条件见上方 effect）。
        bumpPulse();
        setTimeout(() => void runs.retry(), 2000);
      } catch (error) {
        setActionError(
          error instanceof ApiError
            ? `${action} 失败（HTTP ${error.status}）：${error.message}`
            : `${action} 失败：${String(error)}`,
        );
      } finally {
        setBusyAction(null);
      }
    },
    [selectedRunId, injectInput, runDetail, runs],
  );

  const defsError = defs.error;

  return (
    <div className="flex min-h-0 flex-1 flex-col gap-3" data-testid="workflows-page-root">
      <PageHeader
        title="Workflow Studio"
        actions={
          <>
            <span className="badge badge-ghost">工作区 8</span>
            {readOnly ? (
              <span className="badge badge-warning badge-sm" data-testid="readonly-banner">
                {narrow ? "移动端只读查看：编辑请切换宽屏" : "历史回看只读"}
              </span>
            ) : null}
          </>
        }
      />

      {defsError !== null ? (
        <ErrorPanel error={defsError} onRetry={defs.retry} />
      ) : (
        <>
          {/* 第五区：版本列表与历史回看（顶部工具条位，五区之一） */}
          <VersionList
            workflowOptions={workflowOptions}
            workflowId={workflowId}
            versions={versions}
            selectedVersion={selectedVersion}
            dirty={dirty}
            saving={saving}
            saveMessage={saveMessage}
            saveIssues={saveIssues}
            onSelectWorkflow={(id) => {
              setWorkflowId(id);
              setSelectedVersion(null);
            }}
            onSelectVersion={(version) => setSelectedVersion(version)}
            onSave={() => void onSave()}
          />

          {/* 第一/二/三区：左面板 + 中画布 + 右 Inspector */}
          <div
            data-testid="workflows-studio-grid"
            className={`grid min-h-0 flex-1 gap-3 ${
              narrow ? "grid-rows-[auto_auto_auto]" : "grid-cols-[200px_minmax(0,1fr)_300px]"
            }`}
            style={narrow ? undefined : { minHeight: 460 }}
          >
            <NodePalette onAdd={(type) => addNodeAt(type)} onDragStart={onDragStart} />
            <div className={narrow ? "h-[420px]" : "min-h-0"}>
              <FlowCanvas
                nodes={nodes}
                edges={edges}
                readOnly={readOnly}
                selectedEdgeId={selectedEdgeId}
                onNodesChange={onNodesChange}
                onEdgesChange={onEdgesChange}
                onConnect={onConnect}
                onNodeSelect={(id) => {
                  setSelectedNodeId(id);
                  setSelectedEdgeId(null);
                }}
                onEdgeSelect={(id) => {
                  setSelectedEdgeId(id);
                  setSelectedNodeId(null);
                }}
                onDropNode={onDropNode}
              />
            </div>
            <div className={narrow ? "" : "min-h-0"}>
              <NodeInspector
                selectedNode={selectedNode}
                selectedEdge={selectedEdge}
                allNodeIds={nodes.map((node) => node.id)}
                onNodePatch={onNodePatch}
                onNodeIdChange={onNodeIdChange}
                onEdgePatch={onEdgePatch}
                onNodeRemove={onNodeRemove}
                runWorkflowId={workflowId}
                runVersion={runVersion}
                runInput={runInput}
                runBusy={runBusy}
                runError={runError}
                onRunWorkflowIdChange={setWorkflowId}
                onRunVersionChange={setRunVersion}
                onRunInputChange={setRunInput}
                onRun={() => void onRun()}
              />
            </div>
          </div>

          {/* 第四区：底部执行日志、节点状态与产物 */}
          <RunPanel
            runs={runs.data?.items ?? []}
            runsLoading={runs.loading}
            selectedRunId={selectedRunId}
            onSelectRun={(id) => {
              setSelectedRunId(id);
              setInjectInput('{"approval": {"ok": true}}');
            }}
            runDetail={runDetail.data}
            detailLoading={runDetail.loading}
            busyAction={busyAction}
            injectInput={injectInput}
            onInjectChange={setInjectInput}
            onAction={(action, nodeId) => void onAction(action, nodeId)}
            retry={() => {
              void runDetail.retry();
              void runs.retry();
            }}
          />

          {/* 错误与空态说明汇总（不伪造数据） */}
          {actionError ? (
            <p className="break-all text-xs text-error" data-testid="action-error">
              {actionError}
            </p>
          ) : null}
          {runs.error !== null ? <ErrorPanel error={runs.error} onRetry={runs.retry} /> : null}
          {runs.data?.message ? (
            <p className="text-[11px] text-muted">{runs.data.message}</p>
          ) : null}
          {defs.data?.message && workflowId === "" ? (
            <p className="text-[11px] text-muted" data-testid="defs-empty-note">
              {defs.data.message}
            </p>
          ) : null}
          {selectedRunId && runDetail.data ? (
            <p className="text-[10px] text-muted">
              当前 run 状态：
              <StateBadge state={runDetail.data.run?.state} />
            </p>
          ) : null}
        </>
      )}
    </div>
  );
}
