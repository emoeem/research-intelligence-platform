import { useState, type ReactNode } from "react";
import { Link } from "react-router";
import { useApi } from "../lib/hooks";
import { EmptyPanel, ErrorPanel, PageHeader, SkeletonPanel } from "../components/ui";
import { formatDateTime } from "../lib/format";
import {
  fetchAgentRunDetail,
  fetchAgentRuns,
  fetchAgents,
  ApiError,
  type AgentDefinitionRow,
  type AgentDetail,
  type AgentList,
  type AgentRunDetail,
  type AgentRunList,
  type AgentRunStepRow,
} from "../lib/api";

/** Agent Studio（P7-7，SESSION-HANDOFF §11.4 接入点 4）：三区实页
 *  1. Agent 定义与能力绑定（工具/知识引用，/api/agents 列表与 ?id= 详情）；
 *  2. 运行记录与步骤轨迹回放（/api/agent-runs 列表与 ?id= 详情，014
 *     agent_run_step seq 升序回放）；
 *  3. Task/Evidence/Research Memory 视图（013 三记忆表，run_id 归并）。
 *  六 ID 点击跳转 /traces 与 RunDetail（消费 P7-5 关联面）；缺表库按
 *  服务端守卫显真实空态说明，不伪造数据。 */

const AGENT_STATE_BADGE: Record<string, string> = {
  SUCCEEDED: "badge-success",
  FAILED: "badge-error",
  CANCELLED: "badge-warning",
  RUNNING: "badge-info",
  PAUSED: "badge-info",
  QUEUED: "badge-ghost",
};

const STEP_STATUS_BADGE: Record<string, string> = {
  SUCCEEDED: "badge-success",
  FAILED: "badge-error",
  TIMED_OUT: "badge-error",
  CANCELLED: "badge-warning",
  RUNNING: "badge-info",
  QUEUED: "badge-ghost",
  SUBMIT_REJECTED: "badge-error",
};

function StateBadge({ state }: { state?: string | null }) {
  if (!state) return null;
  return (
    <span className={`badge badge-sm ${AGENT_STATE_BADGE[state] ?? "badge-ghost"}`}>{state}</span>
  );
}

function StepStatusBadge({ status }: { status?: string | null }) {
  if (!status) return null;
  return (
    <span className={`badge badge-sm ${STEP_STATUS_BADGE[status] ?? "badge-ghost"}`}>{status}</span>
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

/** P7-5 关联面跳转行：六 ID 各自可点击，tool_run_id 跳 RunDetail
 *  （/runs/<id>），其余跳 /traces?id=（六 ID 统一关联链页）。 */
function CorrelationRow({ label, ids }: { label: string; ids: string[] }) {
  if (ids.length === 0) return null;
  return (
    <div className="flex gap-2 text-xs">
      <span className="w-24 shrink-0 text-muted">{label}</span>
      <span className="flex min-w-0 flex-1 flex-wrap gap-x-3 gap-y-1 break-all">
        {ids.map((id) =>
          label === "tool_run" ? (
            <Link
              key={id}
              to={`/runs/${encodeURIComponent(id)}`}
              className="break-all text-primary underline underline-offset-2"
            >
              {id}
            </Link>
          ) : (
            <TraceIdLink key={id} id={id} />
          ),
        )}
      </span>
    </div>
  );
}

function DefinitionCard({
  definition,
  selected,
  onSelect,
}: {
  definition: AgentDefinitionRow;
  selected: boolean;
  onSelect: () => void;
}) {
  return (
    <button
      type="button"
      data-testid="agent-definition-row"
      onClick={onSelect}
      className={`block w-full rounded-surface border bg-base-100 p-3 text-left transition-colors hover:border-base-content/30 ${
        selected ? "border-primary" : "border-edge"
      }`}
    >
      <p className="break-all text-sm font-medium">
        {definition.agent_id} <span className="text-muted">@ v{definition.version}</span>
      </p>
      <p className="mt-0.5 text-xs text-muted">{definition.name}</p>
      {(definition.capability_refs ?? []).length > 0 ? (
        <div className="mt-1.5 flex flex-wrap gap-1">
          {definition.capability_refs?.map((ref) => (
            <span
              key={`${ref.tool_id}@${ref.version ?? ""}`}
              className="badge badge-sm badge-outline"
            >
              {ref.tool_id}
              {ref.version ? ` @ v${ref.version}` : ""}
            </span>
          ))}
        </div>
      ) : null}
    </button>
  );
}

/** 第一区详情面板：全版本定义 + 能力绑定对账（tool 注册面 lifecycle）+
 *  该 agent 的 run 摘要（点击 run 行即选中第二区详情）。 */
function AgentDetailPanel({
  detail,
  loading,
  error,
  retry,
  selectedRun,
  onSelectRun,
}: {
  detail: AgentDetail | null;
  loading: boolean;
  error: ApiError | null;
  retry: () => void;
  selectedRun: string | null;
  onSelectRun: (runId: string) => void;
}) {
  if (loading) return <SkeletonPanel lines={4} />;
  if (error !== null) return <ErrorPanel error={error} onRetry={retry} />;
  if (!detail) return null;
  return (
    <div className="mt-3 space-y-3">
      <div className="rounded-control border border-edge-soft p-3">
        <h3 className="text-xs font-semibold">能力绑定对账（tool 注册面）</h3>
        {detail.tool_bindings.length === 0 ? (
          <p className="mt-1 text-xs text-muted">本 agent 无能力绑定引用。</p>
        ) : (
          <ul className="mt-2 space-y-1">
            {detail.tool_bindings.map((binding) => (
              <li key={binding.tool_id} className="flex flex-wrap items-center gap-2 text-xs">
                <span className="break-all font-medium">{binding.tool_id}</span>
                {binding.version ? <span className="text-muted">v{binding.version}</span> : null}
                {binding.registered === true ? (
                  <span className="badge badge-sm badge-success">
                    已注册{binding.lifecycle ? ` · ${binding.lifecycle}` : ""}
                  </span>
                ) : binding.registered === false ? (
                  <span className="badge badge-sm badge-warning">未注册（tool 表无此 id）</span>
                ) : (
                  <span className="badge badge-sm badge-ghost">注册面不可读</span>
                )}
              </li>
            ))}
          </ul>
        )}
      </div>
      <div className="rounded-control border border-edge-soft p-3">
        <h3 className="text-xs font-semibold">
          运行摘要
          <span className="ml-2 font-normal text-muted">
            共 {detail.runs_total} 条
            {detail.runs_by_state && Object.keys(detail.runs_by_state).length > 0
              ? ` · ${Object.entries(detail.runs_by_state)
                  .map(([k, v]) => `${k} ${v}`)
                  .join(" · ")}`
              : ""}
          </span>
        </h3>
        {detail.runs.length === 0 ? (
          <p className="mt-1 text-xs text-muted">该 agent 暂无运行记录。</p>
        ) : (
          <ul className="mt-2 space-y-1">
            {detail.runs.map((run) => (
              <li key={run.run_id} className="flex flex-wrap items-center gap-2 text-xs">
                <button
                  type="button"
                  data-testid="agent-run-row"
                  onClick={() => onSelectRun(run.run_id)}
                  className={`break-all font-medium underline underline-offset-2 ${
                    selectedRun === run.run_id ? "text-primary" : "text-base-content"
                  }`}
                >
                  {run.run_id}
                </button>
                <StateBadge state={run.state} />
                {run.trace_id ? <TraceIdLink id={run.trace_id} /> : null}
                {run.steps_count !== undefined ? (
                  <span className="text-muted">{run.steps_count} 步</span>
                ) : null}
                {run.created_at ? (
                  <span className="text-muted">{formatDateTime(run.created_at)}</span>
                ) : null}
              </li>
            ))}
          </ul>
        )}
      </div>
    </div>
  );
}

function StepRow({ step }: { step: AgentRunStepRow }) {
  return (
    <li data-testid="agent-step-row" className="rounded-control border border-edge-soft p-3">
      <div className="flex flex-wrap items-center gap-2">
        <span className="badge badge-sm badge-ghost">步骤 {step.seq}</span>
        <StepStatusBadge status={step.status} />
        <span className="break-all text-xs font-medium">
          {step.tool_id}
          {step.tool_version ? ` @ v${step.tool_version}` : ""}
        </span>
        <span className="text-[11px] text-muted">round {step.round}</span>
        {step.elapsed_ms !== undefined ? (
          <span className="text-[11px] text-muted">{step.elapsed_ms} ms</span>
        ) : null}
        {step.tool_run_id ? (
          <Link
            to={`/runs/${encodeURIComponent(step.tool_run_id)}`}
            className="ml-auto break-all text-[11px] text-primary underline underline-offset-2"
          >
            tool_run {step.tool_run_id}
          </Link>
        ) : null}
      </div>
      {step.summary ? <p className="mt-1 text-xs text-muted">{step.summary}</p> : null}
      {step.error ? <p className="mt-1 text-xs text-error">{step.error}</p> : null}
      {step.artifact_id || step.evidence_id ? (
        <p className="mt-1 flex flex-wrap gap-x-3 text-[11px]">
          {step.artifact_id ? <TraceIdLink id={step.artifact_id} /> : null}
          {step.evidence_id ? <TraceIdLink id={step.evidence_id} /> : null}
          {step.content_hash ? (
            <span className="break-all text-muted">sha256 {step.content_hash.slice(0, 16)}…</span>
          ) : null}
        </p>
      ) : null}
      {step.input !== null && step.input !== undefined ? (
        <details className="mt-1.5">
          <summary className="cursor-pointer text-[11px] text-muted">计划入参快照</summary>
          <pre className="mt-1 max-h-40 overflow-auto whitespace-pre-wrap rounded-control bg-base-200 p-2 text-[11px] leading-4">
            {JSON.stringify(step.input, null, 2)}
          </pre>
        </details>
      ) : null}
    </li>
  );
}

function MemoryBlock({
  title,
  count,
  children,
}: {
  title: string;
  count: number;
  children: ReactNode;
}) {
  return (
    <div className="rounded-control border border-edge-soft p-3">
      <h3 className="text-xs font-semibold">
        {title}
        <span className="ml-2 font-normal text-muted">{count} 行</span>
      </h3>
      {children}
    </div>
  );
}

export function AgentsPage() {
  const [selectedAgent, setSelectedAgent] = useState<string | null>(null);
  const [selectedRun, setSelectedRun] = useState<string | null>(null);

  // 第一区：定义列表（/api/agents）。取数走 lib/api fetch 函数（useApi
  // fetcher 注入，P7-7 三个函数的页面消费面），三态与其余工作区一致。
  const defs = useApi<AgentList>(`/api/agents?limit=50`, {
    fetcher: (_path, signal) => fetchAgents({ limit: 50 }, { signal }),
  });
  const agentDetail = useApi<AgentDetail>(
    selectedAgent ? `/api/agents?id=${encodeURIComponent(selectedAgent)}` : null,
    { fetcher: (_path, signal) => fetchAgents(selectedAgent ?? "", { signal }) },
  );

  // 第二区：运行列表 + 选中 run 详情（步骤轨迹与 correlation）。
  const runs = useApi<AgentRunList>(`/api/agent-runs?limit=50`, {
    fetcher: (_path, signal) => fetchAgentRuns({ limit: 50 }, { signal }),
  });
  const runDetail = useApi<AgentRunDetail>(
    selectedRun ? `/api/agent-runs?id=${encodeURIComponent(selectedRun)}` : null,
    { fetcher: (_path, signal) => fetchAgentRunDetail(selectedRun ?? "", { signal }) },
  );

  const runItems = runs.data?.items ?? [];
  const detail = runDetail.data;
  const memories = detail?.memories;

  return (
    <div className="space-y-4">
      <PageHeader
        title="Agent Studio"
        actions={<span className="badge badge-ghost">工作区 7</span>}
      />

      {/* 第一区：Agent 定义与能力绑定 */}
      <section
        data-testid="agents-defs-section"
        className="rounded-surface border border-edge bg-base-100 p-4"
      >
        <h2 className="text-sm font-semibold">
          Agent 定义与能力绑定
          <span className="ml-2 text-xs font-normal text-muted">
            共 {defs.data?.total ?? 0} 条定义（{defs.data?.agents ?? 0} 个 agent）
          </span>
        </h2>
        {defs.loading ? (
          <SkeletonPanel lines={4} />
        ) : defs.error !== null ? (
          <ErrorPanel error={defs.error} onRetry={defs.retry} />
        ) : defs.data?.available === false || (defs.data?.items ?? []).length === 0 ? (
          <EmptyPanel
            text={
              defs.data?.message ??
              "暂无 Agent 定义：rp-agent def-add 登记面为空（006 迁移表未产生行）。"
            }
          />
        ) : (
          <>
            <p className="mt-1 text-[11px] text-muted">
              {defs.data?.runs_by_state && Object.keys(defs.data.runs_by_state).length > 0
                ? `运行合计 ${defs.data.runs_total ?? 0} 条 · ${Object.entries(
                    defs.data.runs_by_state,
                  )
                    .map(([k, v]) => `${k} ${v}`)
                    .join(" · ")}`
                : ""}
            </p>
            <div className="mt-2 grid gap-2 md:grid-cols-2">
              {(defs.data?.items ?? []).map((definition) => (
                <DefinitionCard
                  key={`${definition.agent_id}@${definition.version}`}
                  definition={definition}
                  selected={selectedAgent === definition.agent_id}
                  onSelect={() =>
                    setSelectedAgent((current) =>
                      current === definition.agent_id ? null : definition.agent_id,
                    )
                  }
                />
              ))}
            </div>
            {selectedAgent ? (
              <div className="mt-3 border-t border-edge-soft pt-3">
                <p className="text-xs text-muted">
                  当前 agent：<span className="break-all font-medium">{selectedAgent}</span>
                  <button
                    type="button"
                    className="ml-2 underline underline-offset-2"
                    onClick={() => setSelectedAgent(null)}
                  >
                    收起
                  </button>
                </p>
                <AgentDetailPanel
                  detail={agentDetail.data}
                  loading={agentDetail.loading}
                  error={agentDetail.error}
                  retry={agentDetail.retry}
                  selectedRun={selectedRun}
                  onSelectRun={setSelectedRun}
                />
              </div>
            ) : null}
          </>
        )}
      </section>

      {/* 第二区：运行记录与步骤轨迹回放 */}
      <section
        data-testid="agents-runs-section"
        className="rounded-surface border border-edge bg-base-100 p-4"
      >
        <h2 className="text-sm font-semibold">
          运行记录与步骤轨迹回放
          <span className="ml-2 text-xs font-normal text-muted">共 {runs.data?.total ?? 0} 条</span>
        </h2>
        {runs.loading ? (
          <SkeletonPanel lines={4} />
        ) : runs.error !== null ? (
          <ErrorPanel error={runs.error} onRetry={runs.retry} />
        ) : runs.data?.available === false || runItems.length === 0 ? (
          <EmptyPanel
            text={runs.data?.message ?? "暂无 AgentRun：rp-agent run 端到端链未产生运行记录。"}
          />
        ) : (
          <ul className="mt-2 space-y-2">
            {runItems.map((run) => (
              <li key={run.run_id}>
                <button
                  type="button"
                  data-testid="agent-run-list-row"
                  onClick={() =>
                    setSelectedRun((current) => (current === run.run_id ? null : run.run_id))
                  }
                  className={`block w-full rounded-surface border p-3 text-left transition-colors hover:border-base-content/30 ${
                    selectedRun === run.run_id ? "border-primary" : "border-edge"
                  } bg-base-100`}
                >
                  <div className="flex flex-wrap items-center gap-2">
                    <StateBadge state={run.state} />
                    {run.steps_count !== undefined ? (
                      <span className="text-[11px] text-muted">{run.steps_count} 步</span>
                    ) : null}
                    {run.created_at ? (
                      <span className="text-[11px] text-muted">
                        发起 {formatDateTime(run.created_at)}
                      </span>
                    ) : null}
                  </div>
                  <p className="mt-1.5 break-all text-sm font-medium">{run.run_id}</p>
                  <p className="mt-1 flex flex-wrap items-center gap-x-3 gap-y-1 text-[11px] text-muted">
                    <span className="break-all">
                      {run.agent_id} @ v{run.agent_version}
                    </span>
                    {run.trace_id ? <TraceIdLink id={run.trace_id} /> : null}
                  </p>
                </button>
              </li>
            ))}
          </ul>
        )}
        {selectedRun ? (
          <div className="mt-3 border-t border-edge-soft pt-3">
            <p className="text-xs text-muted">
              当前 run：<span className="break-all font-medium">{selectedRun}</span>
              <button
                type="button"
                className="ml-2 underline underline-offset-2"
                onClick={() => setSelectedRun(null)}
              >
                收起
              </button>
            </p>
            {runDetail.loading ? (
              <SkeletonPanel lines={5} />
            ) : runDetail.error !== null ? (
              <ErrorPanel error={runDetail.error} onRetry={runDetail.retry} />
            ) : detail ? (
              <div className="mt-2 space-y-3">
                <div className="rounded-control border border-edge-soft p-3">
                  <h3 className="text-xs font-semibold">概要与关联（P7-5 六 ID 面）</h3>
                  <div className="mt-2 space-y-1">
                    <div className="flex gap-2 text-xs">
                      <span className="w-24 shrink-0 text-muted">agent</span>
                      <span className="min-w-0 flex-1 break-all">
                        {detail.run.agent_id} @ v{detail.run.agent_version}
                      </span>
                    </div>
                    <CorrelationRow label="agent_run" ids={[detail.correlation.agent_run_id]} />
                    {detail.correlation.trace_id ? (
                      <CorrelationRow label="trace" ids={[detail.correlation.trace_id]} />
                    ) : null}
                    <CorrelationRow
                      label="task"
                      ids={detail.correlation.task_id ? [detail.correlation.task_id] : []}
                    />
                    <CorrelationRow label="tool_run" ids={detail.correlation.tool_run_ids} />
                    <CorrelationRow label="artifacts" ids={detail.correlation.artifact_ids} />
                    <CorrelationRow label="evidence" ids={detail.correlation.evidence_ids} />
                    {detail.control?.goal_text ? (
                      <div className="flex gap-2 text-xs">
                        <span className="w-24 shrink-0 text-muted">goal</span>
                        <span className="min-w-0 flex-1 break-all">{detail.control.goal_text}</span>
                      </div>
                    ) : null}
                    {detail.control?.retry_of_run_id ? (
                      <div className="flex gap-2 text-xs">
                        <span className="w-24 shrink-0 text-muted">retry_of</span>
                        <TraceIdLink id={detail.control.retry_of_run_id} />
                      </div>
                    ) : null}
                    <div className="flex gap-2 text-xs">
                      <span className="w-24 shrink-0 text-muted">时间</span>
                      <span className="min-w-0 flex-1 break-all">
                        {detail.run.created_at
                          ? `发起 ${formatDateTime(detail.run.created_at)}`
                          : ""}
                        {detail.run.started_at
                          ? ` · 开始 ${formatDateTime(detail.run.started_at)}`
                          : ""}
                        {detail.run.finished_at
                          ? ` · 完成 ${formatDateTime(detail.run.finished_at)}`
                          : ""}
                      </span>
                    </div>
                  </div>
                </div>
                <div className="rounded-control border border-edge-soft p-3">
                  <h3 className="text-xs font-semibold">
                    步骤轨迹回放（agent_run_step，seq 升序）
                    <span className="ml-2 font-normal text-muted">{detail.counts.steps} 步</span>
                  </h3>
                  {detail.steps.length === 0 ? (
                    <p className="mt-1 text-xs text-muted">
                      该 run 无步骤轨迹（014 轨迹面为空：轨迹落库失败降级或历史 run）。
                    </p>
                  ) : (
                    <ol className="mt-2 space-y-2">
                      {detail.steps.map((step) => (
                        <StepRow key={step.seq} step={step} />
                      ))}
                    </ol>
                  )}
                </div>
              </div>
            ) : null}
          </div>
        ) : null}
      </section>

      {/* 第三区：Task/Evidence/Research Memory 视图（013 三记忆表） */}
      <section
        data-testid="agents-memory-section"
        className="rounded-surface border border-edge bg-base-100 p-4"
      >
        <h2 className="text-sm font-semibold">
          Task / Evidence / Research Memory 视图
          <span className="ml-2 text-xs font-normal text-muted">013 三记忆表</span>
        </h2>
        {!selectedRun ? (
          <EmptyPanel text="在上方第二区选择一条运行后，展示该 run 的三类 Memory 视图（run 内工作记忆 / 证据指针台账 / 可追溯研究成果）。" />
        ) : runDetail.loading ? (
          <SkeletonPanel lines={3} />
        ) : runDetail.error !== null ? (
          <ErrorPanel error={runDetail.error} onRetry={runDetail.retry} />
        ) : memories ? (
          <div className="mt-2 space-y-3">
            <MemoryBlock
              title="Task Memory（agent_task_memory，run 内工作记忆）"
              count={memories.task.length}
            >
              {memories.task.length === 0 ? (
                <p className="mt-1 text-xs text-muted">该 run 无 Task Memory 行。</p>
              ) : (
                <ul className="mt-2 space-y-1">
                  {memories.task.map((row) => (
                    <li key={row.memory_id} className="text-xs">
                      <span className="break-all font-medium">{row.memory_id}</span>
                      <span className="ml-2 text-muted">round {row.round}</span>
                      <span className="ml-2 badge badge-sm badge-ghost">{row.lifecycle}</span>
                      {row.summary ? <span className="ml-2 break-all">{row.summary}</span> : null}
                    </li>
                  ))}
                </ul>
              )}
            </MemoryBlock>
            <MemoryBlock
              title="Evidence Memory（agent_evidence_memory，证据指针台账）"
              count={memories.evidence.length}
            >
              {memories.evidence.length === 0 ? (
                <p className="mt-1 text-xs text-muted">该 run 无 Evidence Memory 行。</p>
              ) : (
                <ul className="mt-2 space-y-1">
                  {memories.evidence.map((row) => (
                    <li key={row.memory_id} className="flex flex-wrap items-center gap-2 text-xs">
                      <span className="break-all font-medium">{row.memory_id}</span>
                      <TraceIdLink id={row.evidence_id} />
                      {row.artifact_id ? <TraceIdLink id={row.artifact_id} /> : null}
                      <span className="badge badge-sm badge-ghost">{row.lifecycle}</span>
                      {row.content_hash ? (
                        <span className="break-all text-[11px] text-muted">
                          sha256 {row.content_hash.slice(0, 16)}…
                        </span>
                      ) : null}
                    </li>
                  ))}
                </ul>
              )}
            </MemoryBlock>
            <MemoryBlock
              title="Research Memory（agent_research_memory，可追溯研究成果）"
              count={memories.research.length}
            >
              {memories.research.length === 0 ? (
                <p className="mt-1 text-xs text-muted">该 run 无 Research Memory 行。</p>
              ) : (
                <ul className="mt-2 space-y-1">
                  {memories.research.map((row) => (
                    <li key={row.memory_id} className="text-xs">
                      <span className="break-all font-medium">{row.memory_id}</span>
                      <span className="ml-2">{row.title}</span>
                      <span className="ml-2 badge badge-sm badge-ghost">{row.lifecycle}</span>
                      <span className="ml-2 text-muted">
                        {Array.isArray(row.refs) ? `${row.refs.length} 条回链` : "0 条回链"}
                      </span>
                      {row.summary ? (
                        <p className="mt-0.5 break-all text-muted">{row.summary}</p>
                      ) : null}
                    </li>
                  ))}
                </ul>
              )}
            </MemoryBlock>
          </div>
        ) : null}
      </section>
    </div>
  );
}
