import { useState } from "react";
import { Link } from "react-router";
import { useApi } from "../lib/hooks";
import { EmptyPanel, ErrorPanel, PageHeader, SkeletonPanel } from "../components/ui";
import { formatDateTime } from "../lib/format";

/** 运行记录页（P5-8）：ToolRun 列表（/api/runs，状态过滤走服务端参数）。
 *  数据面 aux 优先（RP_AUX_DB）；未配置/缺表时按服务端守卫显真实空态说明。
 *  三态统一。P7-5：trace_id 在场（015 迁移后开发副本）时行内渲染 trace
 *  徽标链接（/traces?id=），列按库水位条件出现、旧库不伪造。 */

interface RunRow {
  run_id: string;
  tool_id: string;
  tool_version?: string | null;
  task_id?: string | null;
  trace_id?: string | null;
  workspace_id?: string | null;
  status?: string;
  error_category?: string | null;
  duration_ms?: number | null;
  created_at?: string | null;
  started_at?: string | null;
  finished_at?: string | null;
  updated_at?: string | null;
}

interface RunList {
  available?: boolean;
  source?: string;
  total?: number;
  returned?: number;
  reason?: string;
  message?: string;
  items?: RunRow[];
}

const RUN_STATUS = [
  "QUEUED",
  "WAITING_FOR_RUNNER",
  "PREPARING",
  "RUNNING",
  "SUCCEEDED",
  "FAILED",
  "TIMED_OUT",
  "CANCELLED",
] as const;

const STATUS_BADGE: Record<string, string> = {
  SUCCEEDED: "badge-success",
  FAILED: "badge-error",
  TIMED_OUT: "badge-error",
  CANCELLED: "badge-warning",
  RUNNING: "badge-info",
  PREPARING: "badge-info",
  WAITING_FOR_RUNNER: "badge-ghost",
  QUEUED: "badge-ghost",
};

export function RunsPage() {
  const [status, setStatus] = useState("");
  const params = new URLSearchParams({ limit: "50" });
  if (status) params.set("status", status);
  const list = useApi<RunList>(`/api/runs?${params.toString()}`);

  const items = list.data?.items ?? [];

  return (
    <div>
      <PageHeader
        title="运行记录"
        actions={
          <select
            aria-label="按状态过滤"
            className="select select-sm rounded-control border border-edge bg-base-100"
            value={status}
            onChange={(event) => setStatus(event.target.value)}
          >
            <option value="">全部状态</option>
            {RUN_STATUS.map((option) => (
              <option key={option} value={option}>
                {option}
              </option>
            ))}
          </select>
        }
      />
      {list.loading ? (
        <SkeletonPanel lines={5} />
      ) : list.error !== null ? (
        <ErrorPanel error={list.error} onRetry={list.retry} />
      ) : list.data?.available === false ? (
        <EmptyPanel
          text={
            list.data.message ?? `数据面不可读（${list.data.reason ?? "unknown"}）：无 Run 数据。`
          }
        />
      ) : items.length === 0 ? (
        <EmptyPanel text="暂无 Run：工具派发链（Phase 1/3）未产生运行记录。" />
      ) : (
        <>
          <p className="mb-2 text-xs text-muted">
            {status ? `过滤 ${status}：` : ""}共 {list.data?.total ?? 0} 条（显示 {items.length}
            ，数据源：
            {list.data?.source === "aux" ? "RP_AUX_DB 开发库副本" : list.data?.source}）
          </p>
          <ul className="space-y-2">
            {items.map((run) => (
              <li key={run.run_id}>
                <Link
                  to={`/runs/${encodeURIComponent(run.run_id)}`}
                  className="block rounded-surface border border-edge bg-base-100 p-3 transition-colors hover:border-base-content/30"
                >
                  <div className="flex flex-wrap items-center gap-1">
                    {run.status ? (
                      <span
                        className={`badge badge-sm ${STATUS_BADGE[run.status] ?? "badge-ghost"}`}
                      >
                        {run.status}
                      </span>
                    ) : null}
                    {run.error_category ? (
                      <span className="badge badge-sm badge-error">{run.error_category}</span>
                    ) : null}
                    {run.duration_ms !== null && run.duration_ms !== undefined ? (
                      <span className="text-[11px] text-muted">{run.duration_ms} ms</span>
                    ) : null}
                  </div>
                  <p className="mt-1.5 break-all text-sm font-medium">{run.run_id}</p>
                  {run.trace_id ? (
                    <p className="mt-1 text-[11px]">
                      <span className="text-muted">trace </span>
                      <Link
                        to={`/traces?id=${encodeURIComponent(run.trace_id)}`}
                        className="text-primary underline underline-offset-2"
                      >
                        {run.trace_id}
                      </Link>
                    </p>
                  ) : null}
                  <p className="mt-1 text-[11px] text-muted">
                    {run.tool_id}
                    {run.tool_version ? ` @ v${run.tool_version}` : ""}
                    {run.created_at ? ` · 发起 ${formatDateTime(run.created_at)}` : ""}
                    {run.finished_at ? ` · 完成 ${formatDateTime(run.finished_at)}` : ""}
                  </p>
                </Link>
              </li>
            ))}
          </ul>
        </>
      )}
    </div>
  );
}
