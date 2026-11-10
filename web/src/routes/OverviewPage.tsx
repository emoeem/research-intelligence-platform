import { Link } from "react-router";
import { useApi } from "../lib/hooks";
import { EChart } from "../components/EChart";
import { EmptyPanel, ErrorPanel, PageHeader, SkeletonPanel } from "../components/ui";
import { buildFeatureMap } from "../workspaces";

/** 总览页（P5-5）：KPI 卡数据全部来自 /api/overview 与 /api/stats（只读真实
 *  端点），有对应列表页的卡可点击跳转（知识→/knowledge、任务→/projects、
 *  工具→/tools、运行→/runs）；证据与评审队列暂无专属页面（Phase 5 六页范围
 *  外），保持静态卡。统一三态：loading 走 SkeletonPanel、error 走
 *  ErrorPanel（可重试）、数据面全零走 EmptyPanel。 */

interface OverviewData {
  knowledge?: {
    total?: number;
    by_lifecycle?: Record<string, number>;
    by_verified?: Record<string, number>;
  };
  tasks?: { total?: number; by_state?: Record<string, number> };
  resources?: { total?: number };
  evidence?: { total?: number };
  reviews?: { total?: number };
  tools?: { available?: boolean; total?: number; reason?: string };
  runs?: {
    available?: boolean;
    total?: number;
    by_status?: Record<string, number>;
    reason?: string;
  };
  runtime?: { available?: boolean; total?: number; reason?: string };
}

interface StatsData {
  reviews?: {
    total?: number;
    by_queue?: Record<string, { total?: number; by_decision?: Record<string, number> }>;
  };
}

const QUEUE_LABELS: Record<string, string> = { d5: "D5", license: "License", mapping: "Mapping" };

interface KpiSpec {
  label: string;
  value: string;
  note?: string;
  /** 对应列表页路由；缺省为静态卡（暂无专属页面的数据面）。 */
  to?: string;
}

function KpiCard({ spec }: { spec: KpiSpec }) {
  const body = (
    <>
      <p className="text-xs text-base-content/60">{spec.label}</p>
      <p className="mt-1 text-2xl font-semibold tabular-nums">{spec.value}</p>
      {spec.note ? (
        <p className="mt-1 truncate text-[11px] text-base-content/50" title={spec.note}>
          {spec.note}
        </p>
      ) : null}
    </>
  );
  const cls = "rounded-surface border border-edge bg-base-100 p-4";
  if (spec.to) {
    return (
      <Link to={spec.to} className={`${cls} block transition-colors hover:border-primary`}>
        {body}
      </Link>
    );
  }
  return <div className={cls}>{body}</div>;
}

function phase14Face(face: { available?: boolean; total?: number; reason?: string } | undefined): {
  value: string;
  note?: string;
} {
  if (!face) return { value: "-" };
  if (!face.available) return { value: "未接入", note: face.reason };
  return { value: String(face.total ?? 0) };
}

function runStatusNote(face: OverviewData["runs"]): string | undefined {
  if (!face?.available) return face?.reason;
  const byStatus = face.by_status ?? {};
  const parts = Object.entries(byStatus).map(([status, count]) => `${status} ${count}`);
  return parts.length > 0 ? parts.join(" · ") : "暂无 Run";
}

export function OverviewPage() {
  const overview = useApi<OverviewData>("/api/overview");
  const stats = useApi<StatsData>("/api/stats");
  const { data, error, loading, retry } = overview;

  const lifecycleEntries = Object.entries(data?.knowledge?.by_lifecycle ?? {});
  const featureMap = buildFeatureMap({
    knowledge: data?.knowledge?.total ?? 0,
    tools: data?.tools?.total ?? 0,
    runs: data?.runs?.total ?? 0,
  });

  // 评审队列：/api/stats 的 by_queue（d5/license/mapping 各队列计数）。
  const byQueue = stats.data?.reviews?.by_queue ?? {};
  const queueParts = Object.entries(byQueue).map(
    ([queue, info]) => `${QUEUE_LABELS[queue] ?? queue} ${info.total ?? 0}`,
  );
  const reviewsTotal = stats.data?.reviews?.total ?? data?.reviews?.total;
  const reviewsNote =
    stats.error !== null
      ? "/api/stats 不可用"
      : queueParts.length > 0
        ? queueParts.join(" · ")
        : "暂无评审队列数据";

  const kpis: KpiSpec[] = [
    { label: "知识条目", value: String(data?.knowledge?.total ?? 0), to: "/knowledge" },
    { label: "研究任务", value: String(data?.tasks?.total ?? 0), to: "/projects" },
    { label: "证据", value: String(data?.evidence?.total ?? 0) },
    {
      label: "评审队列",
      value: reviewsTotal === undefined ? "-" : String(reviewsTotal),
      note: reviewsNote,
    },
    { label: "工具", ...phase14Face(data?.tools), to: "/tools" },
    { label: "运行", ...phase14Face(data?.runs), note: runStatusNote(data?.runs), to: "/runs" },
  ];

  const allZero =
    !loading &&
    error === null &&
    (data?.knowledge?.total ?? 0) === 0 &&
    (data?.tasks?.total ?? 0) === 0 &&
    (data?.evidence?.total ?? 0) === 0 &&
    (data?.resources?.total ?? 0) === 0;

  return (
    <div>
      <PageHeader
        title="总览"
        actions={
          <span className="badge badge-ghost">数据源：/api/overview + /api/stats（只读）</span>
        }
      />
      {loading ? (
        <SkeletonPanel lines={5} />
      ) : error ? (
        <ErrorPanel error={error} onRetry={retry} />
      ) : allZero ? (
        <EmptyPanel text="平台数据面暂为空：等待数据接入后这里会显示 KPI 与分布图。" />
      ) : (
        <div className="space-y-4">
          <div className="grid grid-cols-2 gap-3 md:grid-cols-3 xl:grid-cols-6">
            {kpis.map((spec) => (
              <KpiCard key={spec.label} spec={spec} />
            ))}
          </div>

          <div className="rounded-surface border border-edge bg-base-100 p-4">
            <h2 className="text-sm font-semibold">知识生命周期分布</h2>
            {lifecycleEntries.length === 0 ? (
              <p className="mt-3 text-sm text-base-content/60">暂无生命周期数据。</p>
            ) : (
              <EChart
                style={{ height: 240 }}
                option={{
                  grid: { left: 8, right: 8, top: 16, bottom: 0, containLabel: true },
                  xAxis: { type: "category", data: lifecycleEntries.map(([key]) => key) },
                  yAxis: { type: "value", minInterval: 1 },
                  series: [
                    {
                      type: "bar",
                      name: "知识条目",
                      data: lifecycleEntries.map(([, count]) => count),
                      barMaxWidth: 36,
                    },
                  ],
                }}
              />
            )}
          </div>
        </div>
      )}

      <div className="mt-4 rounded-surface border border-edge bg-base-100 p-4">
        <h2 className="text-sm font-semibold">功能地图</h2>
        <div className="mt-3 grid gap-3 md:grid-cols-2">
          {featureMap.map((workspace) => (
            <div key={workspace.id} className="rounded-control border border-edge-soft p-3">
              <p className="text-sm font-medium">
                <span className="mr-2 tabular-nums text-base-content/50">{workspace.number}</span>
                {workspace.label}
                <span className="ml-2 text-xs text-base-content/50">{workspace.hint}</span>
              </p>
              {workspace.badges.length > 0 ? (
                <p className="mt-1 flex gap-2">
                  {workspace.badges.map((badge) => (
                    <span key={badge.label} className="badge badge-ghost badge-sm">
                      {badge.label} {badge.value}
                    </span>
                  ))}
                </p>
              ) : null}
              <ul className="mt-2 space-y-1">
                {workspace.tabs.map((tab) => (
                  <li key={tab.to} className="text-xs text-base-content/70" title={tab.hint}>
                    <span className="mr-2 tabular-nums opacity-60">{tab.number}</span>
                    {tab.label}
                  </li>
                ))}
              </ul>
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}
