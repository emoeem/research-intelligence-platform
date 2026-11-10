import { useLocation } from "react-router";
import { useApi } from "../lib/hooks";
import { EmptyPanel, ErrorPanel, PageHeader, SkeletonPanel } from "../components/ui";
import { formatDateTime } from "../lib/format";

/** 项目页（P5-9 最小版，ROADMAP P5-8 裁剪）：/api/tasks 真实列表 + by_state/
 *  by_type 计数 chips；任务详情与研究时间线不做（后续批次）。支持
 *  ?task_id= 定位高亮（Run 详情页与全局搜索的落点契约）。三态统一。 */

interface TaskRow {
  task_id: string;
  title: string;
  objective?: string | null;
  target?: string | null;
  task_type?: string | null;
  state?: string | null;
  source_file?: string | null;
  source_system?: string | null;
  created_at?: string | null;
  updated_at?: string | null;
}

interface TaskList {
  total?: number;
  by_state?: Record<string, number>;
  by_type?: Record<string, number>;
  returned?: number;
  message?: string;
  items?: TaskRow[];
}

const STATE_BADGE: Record<string, string> = {
  COMPLETED: "badge-success",
  AUTHORIZED_EXECUTION: "badge-info",
  CREATED: "badge-ghost",
  AUTHORIZATION_REVIEW: "badge-warning",
  WORKFLOW_PLANNING: "badge-info",
  LOCAL_KNOWLEDGE_RETRIEVAL: "badge-info",
};

export function ProjectsPage() {
  const location = useLocation();
  const highlightId = new URLSearchParams(location.search).get("task_id");
  const list = useApi<TaskList>("/api/tasks?limit=200");

  const items = list.data?.items ?? [];
  const byState = Object.entries(list.data?.by_state ?? {});
  const byType = Object.entries(list.data?.by_type ?? {});

  return (
    <div>
      <PageHeader
        title="项目"
        actions={<span className="badge badge-ghost">数据源：/api/tasks（只读）</span>}
      />
      {list.loading ? (
        <SkeletonPanel lines={6} />
      ) : list.error !== null ? (
        <ErrorPanel error={list.error} onRetry={list.retry} />
      ) : items.length === 0 ? (
        <EmptyPanel text={list.data?.message ?? "暂无任务。"} />
      ) : (
        <>
          <p className="mb-2 text-xs text-base-content/50">
            共 {list.data?.total ?? 0} 个任务（显示 {items.length}）
          </p>
          <div className="mb-4 flex flex-wrap gap-1" aria-label="状态计数">
            {byState.map(([state, count]) => (
              <span key={state} className={`badge badge-sm ${STATE_BADGE[state] ?? "badge-ghost"}`}>
                {state} {count}
              </span>
            ))}
          </div>
          <div className="mb-4 flex flex-wrap gap-1" aria-label="类型计数">
            {byType.map(([type, count]) => (
              <span key={type} className="badge badge-sm badge-outline">
                {type} {count}
              </span>
            ))}
          </div>
          <ul className="space-y-2">
            {items.map((task) => {
              const highlighted = task.task_id === highlightId;
              return (
                <li
                  key={task.task_id}
                  aria-current={highlighted ? "true" : undefined}
                  className={`rounded-surface border bg-base-100 p-3 ${
                    highlighted ? "border-primary bg-primary/5" : "border-edge"
                  }`}
                >
                  <div className="flex flex-wrap items-center gap-1">
                    {task.state ? (
                      <span
                        className={`badge badge-sm ${STATE_BADGE[task.state] ?? "badge-ghost"}`}
                      >
                        {task.state}
                      </span>
                    ) : null}
                    {task.task_type ? (
                      <span className="badge badge-sm badge-outline">{task.task_type}</span>
                    ) : null}
                    {task.target ? (
                      <span className="text-[11px] text-base-content/50">{task.target}</span>
                    ) : null}
                  </div>
                  <p className="mt-1.5 text-sm font-medium leading-5">{task.title}</p>
                  {task.objective ? (
                    <p className="mt-1 line-clamp-2 text-xs text-base-content/60">
                      {task.objective}
                    </p>
                  ) : null}
                  <p className="mt-1 text-[11px] text-base-content/40">
                    {task.task_id}
                    {task.updated_at ? ` · ${formatDateTime(task.updated_at)}` : ""}
                  </p>
                </li>
              );
            })}
          </ul>
        </>
      )}
    </div>
  );
}
