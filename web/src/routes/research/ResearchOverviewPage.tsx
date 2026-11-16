/** Research Workspace 总览（P5-1，Phase 5 §4.5.A 首版）：当前研究概况
 *  全部来自既有真实 API（/api/tasks 状态计数 + /api/runs 运行面 + /api/overview
 *  知识计数），无假数据；子页深链接入口。
 *
 *  KPI 语义（宁缺毋滥，不做没有数据源的卡）：
 *  - 研究任务（total + by_state）：/api/tasks；
 *  - 活跃运行（RUNNING/PREPARING 计数）：/api/runs?limit=200 客户端聚合
 *    （服务端列表无状态聚合参数，limit 200 与 RunsPage 口径一致，分母如实）；
 *  - 知识条目：/api/overview 的既有 knowledge 计数字段；
 *  - 验证结果/覆盖矩阵卡：Phase 5 后续批次（P5-7/P5-8）交付，本版不渲染
 *    编造数字。 */

import { Link } from "react-router";
import { useApi } from "../../lib/hooks";
import { EmptyPanel, ErrorPanel, PageHeader, SkeletonPanel } from "../../components/ui";
import { CountChips } from "../../components/data";
import { formatDateTime } from "../../lib/format";
import {
  RUN_STATUS_TONES,
  TASK_STATE_TONES,
  type ResearchRunList,
  type ResearchTaskList,
} from "./shared";

/** /api/overview 既有响应中本页消费的字段（局部窄类型，与 OverviewPage 各自
 *  独立声明——总览页的 OverviewData 未导出，窄化消费避免耦合其完整形状）。 */
interface OverviewKnowledge {
  knowledge?: { total?: number };
}

/** 活跃运行态（Run 状态机中非终态且在执行面）。 */
const ACTIVE_RUN_STATES = new Set(["QUEUED", "WAITING_FOR_RUNNER", "PREPARING", "RUNNING"]);

function KpiCard({
  label,
  value,
  hint,
  to,
}: {
  label: string;
  value: string | number;
  hint: string;
  to?: string;
}) {
  const body = (
    <div className="rounded-surface border border-edge bg-base-100 p-4">
      <p className="text-xs text-muted">{label}</p>
      <p className="mt-1 text-2xl font-bold tabular-nums">{value}</p>
      <p className="mt-1 text-[11px] text-muted">{hint}</p>
    </div>
  );
  return to ? (
    <Link
      to={to}
      className="transition-colors hover:border-primary"
      aria-label={`${label}：${value}`}
    >
      {body}
    </Link>
  ) : (
    body
  );
}

export function ResearchOverviewPage() {
  const tasks = useApi<ResearchTaskList>("/api/tasks?limit=200");
  const runs = useApi<ResearchRunList>("/api/runs?limit=200");
  const overview = useApi<OverviewKnowledge>("/api/overview");

  if (tasks.loading || runs.loading || overview.loading) {
    return (
      <div>
        <PageHeader title="研究总览" />
        <SkeletonPanel lines={6} />
      </div>
    );
  }
  if (tasks.error !== null) {
    return (
      <div>
        <PageHeader title="研究总览" />
        <ErrorPanel error={tasks.error} onRetry={tasks.retry} />
      </div>
    );
  }

  const taskItems = tasks.data?.items ?? [];
  const byState = Object.entries(tasks.data?.by_state ?? {});
  const runItems = runs.data?.available === false ? [] : (runs.data?.items ?? []);
  const activeRuns = runItems.filter((run) => ACTIVE_RUN_STATES.has(run.status ?? ""));
  const failedRuns = runItems.filter((run) => ["FAILED", "TIMED_OUT"].includes(run.status ?? ""));
  const knowledgeCount = overview.data?.knowledge?.total;
  const recentTasks = taskItems.slice(0, 8);
  const hasAnySource =
    (tasks.data?.total ?? 0) > 0 || runItems.length > 0 || (knowledgeCount ?? 0) > 0;

  return (
    <div>
      <PageHeader
        title="研究总览"
        actions={
          <span className="badge badge-ghost">数据源：/api/tasks + /api/runs + /api/overview</span>
        }
      />

      <div className="grid grid-cols-2 gap-3 lg:grid-cols-4">
        <KpiCard
          label="研究任务"
          value={tasks.data?.total ?? 0}
          hint={`显示 ${taskItems.length}（limit 200 口径）`}
          to="/research/tasks"
        />
        <KpiCard
          label="活跃运行"
          value={activeRuns.length}
          hint={`失败/超时 ${failedRuns.length} · 共 ${runs.data?.total ?? runItems.length} 条`}
          to="/research/tasks"
        />
        <KpiCard
          label="知识条目"
          value={knowledgeCount ?? "不可用"}
          hint="来源 /api/overview 既有计数"
          to="/research/knowledge"
        />
        <KpiCard label="验证结果" value="待交付" hint="五态结果域随 P5-7 交付" />
      </div>

      <div className="mt-5 flex flex-wrap gap-3">
        <CountChips entries={byState} tones={TASK_STATE_TONES} label="任务状态计数" />
      </div>

      <section className="mt-5" aria-label="最近研究任务">
        <header className="mb-2 flex items-center justify-between">
          <h2 className="text-base font-semibold">最近研究任务</h2>
          <Link to="/research/tasks" className="link link-sm text-primary">
            全部任务
          </Link>
        </header>
        {!hasAnySource ? (
          <EmptyPanel text="暂无研究数据：/api/tasks 与 /api/runs 均为空。" />
        ) : recentTasks.length === 0 ? (
          <EmptyPanel text="暂无研究任务。" />
        ) : (
          <ul className="space-y-2">
            {recentTasks.map((task) => (
              <li key={task.task_id}>
                <Link
                  to={`/research/tasks/${encodeURIComponent(task.task_id)}`}
                  className="block rounded-surface border border-edge bg-base-100 p-3 transition-colors hover:border-primary"
                >
                  <div className="flex flex-wrap items-center gap-1.5">
                    <span
                      className={`badge badge-sm ${TASK_STATE_TONES[task.state ?? ""] ?? "badge-ghost"}`}
                    >
                      {task.state ?? "-"}
                    </span>
                    {task.task_type ? (
                      <span className="badge badge-sm badge-outline">{task.task_type}</span>
                    ) : null}
                    {runItems.some((run) => run.task_id === task.task_id) ? (
                      <span className={`badge badge-sm ${RUN_STATUS_TONES.RUNNING} badge-outline`}>
                        有关联运行
                      </span>
                    ) : null}
                    <span className="text-[11px] text-muted">
                      {task.updated_at ? formatDateTime(task.updated_at) : ""}
                    </span>
                  </div>
                  <p className="mt-1.5 text-sm font-medium leading-5">{task.title}</p>
                  {task.target ? (
                    <p className="mt-1 truncate text-[11px] text-muted">{task.target}</p>
                  ) : null}
                </Link>
              </li>
            ))}
          </ul>
        )}
      </section>

      <p className="mt-6 text-xs text-muted">
        资产与知识上下文首版见 Tabs；Scan Center、Coverage、Findings、Evidence、 Trace 子页随 P5-5
        至 P5-8 批次交付（骨架不渲染占位假数据）。
      </p>
    </div>
  );
}
