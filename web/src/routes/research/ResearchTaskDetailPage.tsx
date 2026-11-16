/** Research Task 详情（P5-1，Phase 5 §4.5.C 首版）：/api/tasks 单条 + 关联
 *  运行（/api/runs 客户端按 task_id 过滤）+ status_history 时间线（RTIS 源
 *  JSON 数组保真展示）。
 *
 *  步骤六分类（已完成/执行中/未执行/跳过/失败/不确定）属验证编排域（P5-2/P5-7），
 *  本版展示的是 RTIS 任务镜像的状态流转事实，不伪造步骤状态。 */

import { Link, useParams } from "react-router";
import { useApi } from "../../lib/hooks";
import { EmptyPanel, ErrorPanel, PageHeader, SkeletonPanel } from "../../components/ui";
import { formatDateTime } from "../../lib/format";
import {
  parseStatusHistory,
  RUN_STATUS_TONES,
  TASK_STATE_TONES,
  type ResearchRunList,
  type ResearchTaskRow,
} from "./shared";

/** /api/tasks?task_id= 单条响应形状（服务端 handle_tasks 单条守卫口径，
 *  P5-1 起支持；镜像表只读）。 */
interface TaskDetailPayload {
  available?: boolean;
  item?: ResearchTaskRow;
  task_id?: string;
}

export function ResearchTaskDetailPage() {
  const { taskId = "" } = useParams();
  const detail = useApi<TaskDetailPayload>(
    taskId ? `/api/tasks?task_id=${encodeURIComponent(taskId)}` : null,
  );
  const runs = useApi<ResearchRunList>("/api/runs?limit=200");

  if (detail.loading) {
    return (
      <div>
        <PageHeader title="任务详情" />
        <SkeletonPanel lines={6} />
      </div>
    );
  }
  if (detail.error !== null) {
    return (
      <div>
        <PageHeader title="任务详情" />
        <ErrorPanel error={detail.error} onRetry={detail.retry} />
      </div>
    );
  }

  const task = detail.data?.item;
  if (!task) {
    return (
      <div>
        <PageHeader title="任务详情" />
        <EmptyPanel
          text={`任务不存在：${taskId || "(空 ID)"}`}
          actionLabel="返回任务列表"
          onAction={() => {
            window.location.assign("/research/tasks");
          }}
        />
      </div>
    );
  }

  const history = parseStatusHistory(task.status_history);
  const relatedRuns =
    runs.data?.available === false
      ? []
      : (runs.data?.items ?? []).filter((run) => run.task_id === task.task_id);

  return (
    <div>
      <PageHeader
        title={task.title ?? "任务详情"}
        actions={
          <>
            <span
              className={`badge badge-sm ${TASK_STATE_TONES[task.state ?? ""] ?? "badge-ghost"}`}
            >
              {task.state ?? "-"}
            </span>
            {task.task_type ? (
              <span className="badge badge-sm badge-outline">{task.task_type}</span>
            ) : null}
            <Link to="/research/tasks" className="btn btn-ghost btn-sm">
              返回列表
            </Link>
          </>
        }
      />

      <section className="rounded-surface border border-edge bg-base-100 p-4" aria-label="任务属性">
        <dl className="grid grid-cols-1 gap-x-6 gap-y-2 text-sm md:grid-cols-2">
          <div>
            <dt className="text-xs text-muted">任务 ID</dt>
            <dd className="font-mono text-xs break-all">{task.task_id}</dd>
          </div>
          <div>
            <dt className="text-xs text-muted">目标</dt>
            <dd className="break-all">{task.target ?? "-"}</dd>
          </div>
          <div>
            <dt className="text-xs text-muted">来源系统</dt>
            <dd>
              {task.source_system ?? "-"}
              {task.source_file ? `（${task.source_file}）` : ""}
            </dd>
          </div>
          <div>
            <dt className="text-xs text-muted">创建 / 更新</dt>
            <dd>
              {task.created_at ? formatDateTime(task.created_at) : "-"} /{" "}
              {task.updated_at ? formatDateTime(task.updated_at) : "-"}
            </dd>
          </div>
          <div className="md:col-span-2">
            <dt className="text-xs text-muted">目标（objective）</dt>
            <dd className="whitespace-pre-wrap break-words">{task.objective ?? "-"}</dd>
          </div>
        </dl>
      </section>

      <section className="mt-5" aria-label="状态时间线">
        <h2 className="mb-2 text-base font-semibold">状态时间线（status_history 保真）</h2>
        {history.length === 0 ? (
          <EmptyPanel text="无状态历史（源数据 status_history 为空或不可解析）。" />
        ) : (
          <ol className="space-y-1.5">
            {history.map((entry, index) => (
              <li
                key={`${entry.at ?? ""}-${entry.state}-${index}`}
                className="flex flex-wrap items-baseline gap-2 rounded-surface border border-edge bg-base-100 px-3 py-2"
              >
                <span className="font-mono text-[11px] text-muted">{entry.at ?? "-"}</span>
                <span
                  className={`badge badge-sm ${TASK_STATE_TONES[entry.state] ?? "badge-ghost"}`}
                >
                  {entry.state}
                </span>
                {entry.reason ? <span className="text-xs text-muted">{entry.reason}</span> : null}
              </li>
            ))}
          </ol>
        )}
      </section>

      <section className="mt-5" aria-label="关联运行">
        <h2 className="mb-2 text-base font-semibold">
          关联运行（{relatedRuns.length}）
          <span className="ml-2 text-xs font-normal text-muted">
            task_id 精确匹配 /api/runs（载入 200 条口径）
          </span>
        </h2>
        {runs.error !== null ? (
          <p className="text-xs text-error">运行面不可读：{runs.error.message}</p>
        ) : relatedRuns.length === 0 ? (
          <EmptyPanel text="该任务暂无关联 ToolRun（运行域记录随 P5-6 执行调度批次出现）。" />
        ) : (
          <ul className="space-y-2">
            {relatedRuns.map((run) => (
              <li
                key={run.run_id}
                className="flex flex-wrap items-center gap-2 rounded-surface border border-edge bg-base-100 px-3 py-2"
              >
                <span
                  className={`badge badge-sm ${RUN_STATUS_TONES[run.status ?? ""] ?? "badge-ghost"}`}
                >
                  {run.status ?? "-"}
                </span>
                <Link
                  to={`/runs/${encodeURIComponent(run.run_id)}`}
                  className="font-mono text-xs text-primary underline-offset-2 hover:underline"
                >
                  {run.run_id}
                </Link>
                <span className="text-xs text-muted">{run.tool_id}</span>
                {run.created_at ? (
                  <span className="text-[11px] text-muted">{formatDateTime(run.created_at)}</span>
                ) : null}
              </li>
            ))}
          </ul>
        )}
      </section>
    </div>
  );
}
