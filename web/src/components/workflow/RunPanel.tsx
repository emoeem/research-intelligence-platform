import { Link } from "react-router";
import { formatDateTime } from "../../lib/format";
import type { WorkflowRunDetail, WorkflowRunNodeRow, WorkflowRunRow } from "../../lib/api";

/** 第四区底部执行日志/节点状态/产物（P8-4，计划 §10.4 G 底栏）：run 列表
 *  与 ?id= 详情（017 节点行五态 + 018 控制行 awaiting_approval 视图 + 六 ID
 *  关联 + checkpoint）。控制按钮接 P8-3 API：暂停（同步请求位）/ 恢复
 *  （inject 批准参数）/ 失败节点重试（新 run 承载）。 */

const RUN_STATE_BADGE: Record<string, string> = {
  succeeded: "badge-success",
  failed: "badge-error",
  cancelled: "badge-warning",
  running: "badge-info",
  paused: "badge-info",
  queued: "badge-ghost",
};

const NODE_STATE_BADGE: Record<string, string> = {
  succeeded: "badge-success",
  failed: "badge-error",
  cancelled: "badge-warning",
  running: "badge-info",
  paused: "badge-info",
};

export function StateBadge({ state }: { state: string | null | undefined }) {
  if (!state) return null;
  return (
    <span className={`badge badge-sm ${RUN_STATE_BADGE[state] ?? "badge-ghost"}`}>{state}</span>
  );
}

function TraceIdLink({ id }: { id: string }) {
  return (
    <Link
      to={`/traces?id=${encodeURIComponent(id)}`}
      className="break-all text-primary underline underline-offset-2"
    >
      {id}
    </Link>
  );
}

function NodeRow({
  node,
  onRetry,
  busy,
}: {
  node: WorkflowRunNodeRow;
  onRetry: (nodeId: string) => void;
  busy: boolean;
}) {
  return (
    <li
      data-testid="workflow-node-row"
      className="flex flex-wrap items-center gap-2 rounded-control border border-edge-soft p-2 text-xs"
    >
      <span className="badge badge-sm badge-ghost">{node.node_kind}</span>
      <span className="break-all font-medium">{node.node_id}</span>
      <span className={`badge badge-sm ${NODE_STATE_BADGE[node.state] ?? "badge-ghost"}`}>
        {node.state}
      </span>
      <span className="text-muted">attempt {node.attempt}</span>
      {node.tool_run_id ? (
        <Link
          to={`/runs/${encodeURIComponent(node.tool_run_id)}`}
          className="break-all text-primary underline underline-offset-2"
        >
          tool_run {node.tool_run_id}
        </Link>
      ) : null}
      {node.agent_run_id ? <TraceIdLink id={node.agent_run_id} /> : null}
      {node.state === "failed" ? (
        <button
          type="button"
          className="btn btn-outline btn-warning btn-xs ml-auto"
          disabled={busy}
          onClick={() => onRetry(node.node_id)}
        >
          重试该节点
        </button>
      ) : null}
      {node.output !== null && node.output !== undefined ? (
        <details className="w-full">
          <summary className="cursor-pointer text-[10px] text-muted">节点产物</summary>
          <pre className="mt-1 max-h-32 overflow-auto whitespace-pre-wrap rounded-control bg-base-200 p-2 text-[10px] leading-4">
            {JSON.stringify(node.output, null, 2)}
          </pre>
        </details>
      ) : null}
      {node.error !== null && node.error !== undefined ? (
        <pre className="w-full whitespace-pre-wrap text-[10px] text-error">
          {JSON.stringify(node.error, null, 2)}
        </pre>
      ) : null}
    </li>
  );
}

export interface RunPanelProps {
  runs: WorkflowRunRow[];
  runsLoading: boolean;
  selectedRunId: string | null;
  onSelectRun: (runId: string) => void;
  runDetail: WorkflowRunDetail | null;
  detailLoading: boolean;
  busyAction: string | null;
  injectInput: string;
  onInjectChange: (value: string) => void;
  onAction: (action: "pause" | "resume" | "retry", nodeId?: string) => void;
  retry: () => void;
}

export function RunPanel({
  runs,
  runsLoading,
  selectedRunId,
  onSelectRun,
  runDetail,
  detailLoading,
  busyAction,
  injectInput,
  onInjectChange,
  onAction,
  retry,
}: RunPanelProps) {
  const control = runDetail?.control;
  const run = runDetail?.run;
  const active = run ? ["queued", "running", "paused"].includes(run.state) : false;
  const detail = runDetail;
  const correlation = detail?.correlation;

  return (
    <section
      data-testid="workflows-run-section"
      className="rounded-surface border border-edge bg-base-100 p-3"
      aria-label="执行日志节点状态与产物"
    >
      <h2 className="text-xs font-semibold text-base-content/70">
        执行日志、节点状态与产物
        <span className="ml-2 text-[10px] font-normal text-muted">
          WorkflowRun {runs.length} 条 · 节点行 017 · 控制行 018
        </span>
      </h2>
      <div className="mt-2 grid gap-3 lg:grid-cols-[minmax(260px,1fr)_2fr]">
        {/* run 列表 */}
        <div className="min-w-0">
          {runsLoading ? (
            <p className="text-xs text-muted">加载中…</p>
          ) : runs.length === 0 ? (
            <p className="text-xs text-muted">
              暂无运行记录：在上方运行配置发起一次 run。
            </p>
          ) : (
            <ul className="max-h-40 space-y-1 overflow-y-auto pr-1">
              {runs.map((item) => (
                <li key={item.run_id}>
                  <button
                    type="button"
                    data-testid="workflow-run-row"
                    onClick={() => onSelectRun(item.run_id)}
                    className={`flex w-full flex-wrap items-center gap-2 rounded-control border p-2 text-left text-xs ${
                      selectedRunId === item.run_id ? "border-primary" : "border-edge-soft"
                    }`}
                  >
                    <StateBadge state={item.state} />
                    <span className="break-all font-mono">{item.run_id}</span>
                    <span className="ml-auto text-[10px] text-muted">
                      {item.created_at ? formatDateTime(item.created_at) : ""}
                    </span>
                  </button>
                </li>
              ))}
            </ul>
          )}
        </div>

        {/* 选中 run 详情 */}
        <div className="min-w-0 border-l border-edge-soft pl-3">
          {!selectedRunId ? (
            <p className="text-xs text-muted">
              选择左侧一条 run 查看节点状态、控制行与产物。
            </p>
          ) : detailLoading && !detail ? (
            <p className="text-xs text-muted">加载详情…</p>
          ) : detail && detail.run ? (
            <div className="space-y-2">
              <div className="flex flex-wrap items-center gap-2">
                <StateBadge state={detail.run.state} />
                {control?.awaiting_approval ? (
                  <span
                    className="badge badge-sm badge-warning"
                    data-testid="awaiting-approval-badge"
                  >
                    awaiting_approval @ {control.approval_node_id ?? "?"}
                  </span>
                ) : null}
                {control?.pause_requested_at ? (
                  <span className="badge badge-sm badge-info" data-testid="pause-requested-badge">
                    PAUSE-REQUESTED {formatDateTime(control.pause_requested_at)}
                  </span>
                ) : null}
                {detail.checkpoint_seq !== null && detail.checkpoint_seq !== undefined ? (
                  <span className="text-[10px] text-muted">
                    checkpoint seq {detail.checkpoint_seq}
                  </span>
                ) : null}
                <span className="ml-auto flex gap-1.5">
                  <button
                    type="button"
                    data-testid="run-pause-btn"
                    className="btn btn-outline btn-info btn-xs"
                    disabled={busyAction !== null || !active}
                    onClick={() => onAction("pause")}
                  >
                    暂停
                  </button>
                  <button
                    type="button"
                    data-testid="run-resume-btn"
                    className="btn btn-outline btn-success btn-xs"
                    disabled={busyAction !== null || run?.state !== "paused"}
                    onClick={() => onAction("resume")}
                  >
                    恢复
                  </button>
                </span>
              </div>
              <p className="break-all font-mono text-xs">{detail.run.run_id}</p>
              <p className="text-[11px] text-muted">
                {detail.run.workflow_id} @ v{detail.run.workflow_version}
                {detail.run.trace_id ? (
                  <>
                    {" · trace "}
                    <TraceIdLink id={detail.run.trace_id} />
                  </>
                ) : null}
              </p>
              {control?.awaiting_approval ? (
                <div
                  className="rounded-control border border-edge-soft p-2"
                  data-testid="approval-request-view"
                >
                  <p className="text-[10px] font-semibold text-base-content/70">
                    人工介入请求（018）
                  </p>
                  {typeof control.approval_request === "object" &&
                  control.approval_request !== null ? (
                    <pre className="mt-1 max-h-32 overflow-auto whitespace-pre-wrap rounded-control bg-base-200 p-2 text-[10px] leading-4">
                      {JSON.stringify(control.approval_request, null, 2)}
                    </pre>
                  ) : null}
                  <label className="form-control mt-1.5">
                    <span className="label-text pb-1 text-[10px]">
                      批准参数 inject（按 human 节点 key 包键，缺省 approval）
                    </span>
                    <textarea
                      className="textarea textarea-bordered font-mono text-[11px]"
                      rows={2}
                      spellCheck={false}
                      data-testid="resume-inject-input"
                      value={injectInput}
                      onChange={(event) => onInjectChange(event.target.value)}
                    />
                  </label>
                </div>
              ) : null}
              {correlation ? (
                <p className="flex flex-wrap gap-x-3 gap-y-1 text-[10px] text-muted">
                  {correlation.task_id ? <span>task {correlation.task_id}</span> : null}
                  {correlation.tool_run_id ? <span>tool_run {correlation.tool_run_id}</span> : null}
                  {correlation.agent_run_id ? (
                    <span>agent_run {correlation.agent_run_id}</span>
                  ) : null}
                  {correlation.artifact_id ? <span>artifact {correlation.artifact_id}</span> : null}
                  {correlation.evidence_id ? <span>evidence {correlation.evidence_id}</span> : null}
                </p>
              ) : null}
              {detail.nodes.length > 0 ? (
                <ul className="max-h-56 space-y-1 overflow-y-auto pr-1">
                  {detail.nodes.map((node) => (
                    <NodeRow
                      key={node.node_id}
                      node={node}
                      busy={busyAction !== null}
                      onRetry={(nodeId) => onAction("retry", nodeId)}
                    />
                  ))}
                </ul>
              ) : (
                <p className="text-[11px] text-muted">该 run 暂无节点状态行。</p>
              )}
              {run?.output !== null && run?.output !== undefined ? (
                <details>
                  <summary className="cursor-pointer text-[10px] text-muted">
                    run 产物 output
                  </summary>
                  <pre className="mt-1 max-h-32 overflow-auto whitespace-pre-wrap rounded-control bg-base-200 p-2 text-[10px] leading-4">
                    {JSON.stringify(run?.output, null, 2)}
                  </pre>
                </details>
              ) : null}
              {run?.error !== null && run?.error !== undefined ? (
                <pre className="whitespace-pre-wrap text-[10px] text-error">
                  {JSON.stringify(run?.error, null, 2)}
                </pre>
              ) : null}
              <button type="button" className="btn btn-ghost btn-xs" onClick={retry}>
                刷新
              </button>
            </div>
          ) : (
            <p className="text-xs text-muted">详情不可读（服务端守卫降级）。</p>
          )}
        </div>
      </div>
    </section>
  );
}
