import type { Edge, Node } from "@xyflow/react";
import { useState } from "react";
import { paletteEntry } from "../../lib/workflow-graph";

/** 第三区右侧节点参数 Schema 表单与运行配置（P8-4，计划 §10.4 G 右栏）：
 *  选中节点按类型词表出 Schema 表单（必填键与 workflow-engine validate 同
 *  口径提示），JSON 参数编辑器解析失败内联报错不静默丢改；选中边出词形
 *  （seq/branch）与条件载荷编辑。运行配置承担 run 受理面（P8-3 API）。 */

const NODE_ID_PATTERN = /^[A-Za-z0-9][A-Za-z0-9_.-]*$/;

function JsonField({
  label,
  value,
  onChange,
}: {
  label: string;
  value: unknown;
  onChange: (value: unknown) => void;
}) {
  const [draft, setDraft] = useState<string | null>(null);
  const text = draft ?? (value === undefined ? "" : JSON.stringify(value, null, 2));
  return (
    <label className="form-control">
      <span className="label-text pb-1 text-xs">{label}</span>
      <textarea
        className="textarea textarea-bordered font-mono text-xs"
        rows={4}
        spellCheck={false}
        value={text}
        onChange={(event) => setDraft(event.target.value)}
        onBlur={() => {
          if (draft === null) return;
          const trimmed = draft.trim();
          if (!trimmed) {
            onChange(undefined);
            setDraft(null);
            return;
          }
          try {
            onChange(JSON.parse(trimmed) as unknown);
            setDraft(null);
          } catch {
            /* 解析失败保留草稿，内联提示 */
          }
        }}
      />
      {draft !== null && draft.trim() ? (
        <span className="label-text-alt mt-1 text-error">JSON 解析失败：修正后再失焦提交</span>
      ) : null}
    </label>
  );
}

function TextField({
  label,
  value,
  required,
  onChange,
}: {
  label: string;
  value: string;
  required?: boolean;
  onChange: (value: string) => void;
}) {
  return (
    <label className="form-control">
      <span className="label-text pb-1 text-xs">
        {label}
        {required ? <span className="ml-1 text-warning">必填</span> : null}
      </span>
      <input
        type="text"
        className="input input-bordered input-sm w-full font-mono text-xs"
        value={value}
        onChange={(event) => onChange(event.target.value)}
      />
    </label>
  );
}

export interface NodeInspectorProps {
  selectedNode: Node | null;
  selectedEdge: Edge | null;
  allNodeIds: string[];
  onNodePatch: (nodeId: string, params: Record<string, unknown>) => void;
  onNodeIdChange: (oldId: string, newId: string) => void;
  onEdgePatch: (edgeId: string, patch: { kind?: string; when?: unknown }) => void;
  onNodeRemove: (nodeId: string) => void;
  /* 运行配置（run 受理面） */
  runWorkflowId: string;
  runVersion: number | null;
  runInput: string;
  runBusy: boolean;
  runError: string | null;
  onRunWorkflowIdChange: (value: string) => void;
  onRunVersionChange: (value: number | null) => void;
  onRunInputChange: (value: string) => void;
  onRun: () => void;
}

export function NodeInspector({
  selectedNode,
  selectedEdge,
  allNodeIds,
  onNodePatch,
  onNodeIdChange,
  onEdgePatch,
  onNodeRemove,
  runWorkflowId,
  runVersion,
  runInput,
  runBusy,
  runError,
  onRunWorkflowIdChange,
  onRunVersionChange,
  onRunInputChange,
  onRun,
}: NodeInspectorProps) {
  const data = selectedNode?.data as
    { nodeType?: string; params?: Record<string, unknown> } | undefined;
  const entry = data?.nodeType ? paletteEntry(data.nodeType) : undefined;
  const params = data?.params ?? {};
  const setParam = (key: string, value: unknown) => {
    if (!selectedNode) return;
    const next = { ...params };
    if (value === undefined || value === "") delete next[key];
    else next[key] = value;
    onNodePatch(selectedNode.id, next);
  };
  const [idDraft, setIdDraft] = useState<string | null>(null);
  const idValue = idDraft ?? selectedNode?.id ?? "";
  const idError =
    idDraft !== null && idDraft !== selectedNode?.id
      ? !NODE_ID_PATTERN.test(idDraft)
        ? "id 形状须为 ^[A-Za-z0-9][A-Za-z0-9_.-]*$（definition-v1）"
        : allNodeIds.includes(idDraft)
          ? "id 与现有节点重复"
          : null
      : null;

  return (
    <aside
      data-testid="workflows-inspector-section"
      aria-label="节点参数与运行配置"
      className="flex h-full min-h-0 flex-col gap-3 overflow-y-auto rounded-surface border border-edge bg-base-100 p-3"
    >
      <h2 className="text-xs font-semibold text-base-content/70">节点参数 Schema 表单与运行配置</h2>

      {selectedNode && data ? (
        <div className="rounded-control border border-edge-soft p-2">
          <p className="text-[10px] text-muted">
            选中节点 <span className="font-mono">{data.nodeType}</span>
            {entry ? ` · ${entry.label}` : ""}
          </p>
          <label className="form-control mt-1.5">
            <span className="label-text pb-1 text-xs">节点 id</span>
            <input
              type="text"
              className="input input-bordered input-sm w-full font-mono text-xs"
              value={idValue}
              onChange={(event) => setIdDraft(event.target.value)}
              onBlur={() => {
                if (idDraft === null || idError !== null || idDraft === selectedNode.id) {
                  setIdDraft(null);
                  return;
                }
                onNodeIdChange(selectedNode.id, idDraft);
                setIdDraft(null);
              }}
            />
            {idError ? <span className="label-text-alt mt-1 text-error">{idError}</span> : null}
          </label>

          {/* 类型词表 Schema 表单（与 workflow-engine validate 必填口径一致） */}
          {data.nodeType === "tool" ? (
            <>
              <div className="mt-2">
                <TextField
                  label="tool_id"
                  required
                  value={String(params.tool_id ?? "")}
                  onChange={(value) => setParam("tool_id", value)}
                />
              </div>
              <div className="mt-2">
                <JsonField
                  label="input（JSON，$/ 指 run state）"
                  value={params.input}
                  onChange={(value) => setParam("input", value)}
                />
              </div>
            </>
          ) : null}
          {data.nodeType === "knowledge" ? (
            <div className="mt-2">
              <TextField
                label="query"
                required
                value={String(params.query ?? "")}
                onChange={(value) => setParam("query", value)}
              />
            </div>
          ) : null}
          {data.nodeType === "agent" ? (
            <>
              <div className="mt-2">
                <TextField
                  label="agent_id"
                  required
                  value={String(params.agent_id ?? "")}
                  onChange={(value) => setParam("agent_id", value)}
                />
              </div>
              <div className="mt-2">
                <TextField
                  label="goal"
                  value={String(params.goal ?? "")}
                  onChange={(value) => setParam("goal", value)}
                />
              </div>
            </>
          ) : null}
          {data.nodeType === "human" ? (
            <>
              <div className="mt-2">
                <TextField
                  label="prompt（审批说明）"
                  value={String(params.prompt ?? "")}
                  onChange={(value) => setParam("prompt", value)}
                />
              </div>
              <div className="mt-2">
                <TextField
                  label="key（批准参数落键，缺省 approval）"
                  value={String(params.key ?? "")}
                  onChange={(value) => setParam("key", value)}
                />
              </div>
            </>
          ) : null}
          {data.nodeType === "condition" ? (
            <div className="mt-2">
              <TextField
                label="expression"
                required
                value={String(params.expression ?? "")}
                onChange={(value) => setParam("expression", value)}
              />
            </div>
          ) : null}
          {data.nodeType === "start" || data.nodeType === "end" || data.nodeType === "parallel" ? (
            <p className="mt-2 text-[10px] text-muted">该类型无参数（DSL 词表口径）。</p>
          ) : null}

          <button
            type="button"
            data-testid="inspector-remove-node"
            className="btn btn-error btn-outline btn-xs mt-3"
            onClick={() => onNodeRemove(selectedNode.id)}
          >
            删除节点
          </button>
        </div>
      ) : (
        <p className="text-[11px] text-muted">
          点击或拖拽画布节点后在此编辑参数；未选中时保持空白（不伪造表单）。
        </p>
      )}

      {selectedEdge ? (
        <div
          className="rounded-control border border-edge-soft p-2"
          data-testid="inspector-edge-form"
        >
          <p className="text-[10px] text-muted">
            选中边{" "}
            <span className="font-mono">
              {String(selectedEdge.source)} → {String(selectedEdge.target)}
            </span>
          </p>
          <label className="form-control mt-1.5">
            <span className="label-text pb-1 text-xs">词形 kind</span>
            <select
              className="select select-bordered select-sm text-xs"
              value={String((selectedEdge.data as { kind?: string } | undefined)?.kind ?? "seq")}
              onChange={(event) => onEdgePatch(selectedEdge.id, { kind: event.target.value })}
            >
              <option value="seq">seq（顺序）</option>
              <option value="branch">branch（分支）</option>
            </select>
          </label>
          <div className="mt-2">
            <JsonField
              label="when（branch 条件载荷，JSON）"
              value={(selectedEdge.data as { when?: unknown } | undefined)?.when}
              onChange={(value) => onEdgePatch(selectedEdge.id, { when: value })}
            />
          </div>
        </div>
      ) : null}

      <div className="rounded-control border border-edge-soft p-2" data-testid="inspector-run-form">
        <h3 className="text-xs font-semibold">运行配置（P8-3 run 受理面）</h3>
        <div className="mt-2">
          <TextField label="workflow_id" value={runWorkflowId} onChange={onRunWorkflowIdChange} />
        </div>
        <label className="form-control mt-2">
          <span className="label-text pb-1 text-xs">version（缺省最新版）</span>
          <input
            type="number"
            min={1}
            className="input input-bordered input-sm w-full font-mono text-xs"
            value={runVersion ?? ""}
            onChange={(event) => {
              const parsed = Number(event.target.value);
              onRunVersionChange(Number.isInteger(parsed) && parsed >= 1 ? parsed : null);
            }}
          />
        </label>
        <label className="form-control mt-2">
          <span className="label-text pb-1 text-xs">input（JSON object）</span>
          <textarea
            className="textarea textarea-bordered font-mono text-xs"
            rows={4}
            spellCheck={false}
            data-testid="run-input-json"
            value={runInput}
            onChange={(event) => onRunInputChange(event.target.value)}
          />
        </label>
        <button
          type="button"
          data-testid="run-start-btn"
          className="btn btn-primary btn-sm mt-2 w-full"
          disabled={runBusy || !runWorkflowId.trim()}
          onClick={onRun}
        >
          {runBusy ? "受理中…" : "运行"}
        </button>
        {runError ? (
          <p className="mt-1.5 break-all text-[10px] text-error" data-testid="run-error">
            {runError}
          </p>
        ) : null}
      </div>
    </aside>
  );
}
