import { useApi } from "../lib/hooks";
import { EChart } from "../components/EChart";
import { ErrorPanel, PageHeader, SkeletonPanel } from "../components/ui";
import { buildFeatureMap } from "../workspaces";

/** 总览页（P5-4 最小版）：/api/overview 真实数据 + 知识生命周期分布图。
 *  EChart 在此首次进入构建图（按需动态加载，vendor-echarts 分组随之出现）；
 *  完整 Overview 页（功能地图、更多图表与快捷入口）由 P5-5 批次扩展。 */

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
  runs?: { available?: boolean; total?: number; reason?: string };
  runtime?: { available?: boolean; total?: number; reason?: string };
}

function KpiCard({ label, value, note }: { label: string; value: string; note?: string }) {
  return (
    <div className="rounded-surface border border-edge bg-base-100 p-4">
      <p className="text-xs text-base-content/60">{label}</p>
      <p className="mt-1 text-2xl font-semibold tabular-nums">{value}</p>
      {note ? <p className="mt-1 text-[11px] text-base-content/50">{note}</p> : null}
    </div>
  );
}

function phase14Note(face: { available?: boolean; total?: number; reason?: string } | undefined): {
  value: string;
  note?: string;
} {
  if (!face) return { value: "-" };
  if (!face.available) return { value: "未接入", note: face.reason };
  return { value: String(face.total ?? 0) };
}

export function OverviewPage() {
  const { data, error, loading, retry } = useApi<OverviewData>("/api/overview");
  const lifecycleEntries = Object.entries(data?.knowledge?.by_lifecycle ?? {});
  const featureMap = buildFeatureMap({
    knowledge: data?.knowledge?.total ?? 0,
    tools: data?.tools?.total ?? 0,
    runs: data?.runs?.total ?? 0,
  });

  return (
    <div>
      <PageHeader
        title="总览"
        actions={<span className="badge badge-ghost">数据源：/api/overview（只读）</span>}
      />
      {loading ? (
        <SkeletonPanel lines={5} />
      ) : error ? (
        <ErrorPanel error={error} onRetry={retry} />
      ) : (
        <div className="space-y-4">
          <div className="grid grid-cols-2 gap-3 md:grid-cols-3 xl:grid-cols-6">
            <KpiCard label="知识条目" value={String(data?.knowledge?.total ?? 0)} />
            <KpiCard label="研究任务" value={String(data?.tasks?.total ?? 0)} />
            <KpiCard label="资源" value={String(data?.resources?.total ?? 0)} />
            <KpiCard label="证据" value={String(data?.evidence?.total ?? 0)} />
            <KpiCard label="工具" {...phase14Note(data?.tools)} />
            <KpiCard label="运行" {...phase14Note(data?.runs)} />
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

          <div className="rounded-surface border border-edge bg-base-100 p-4">
            <h2 className="text-sm font-semibold">功能地图</h2>
            <div className="mt-3 grid gap-3 md:grid-cols-2">
              {featureMap.map((workspace) => (
                <div key={workspace.id} className="rounded-control border border-edge-soft p-3">
                  <p className="text-sm font-medium">
                    <span className="mr-2 tabular-nums text-base-content/50">
                      {workspace.number}
                    </span>
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
      )}
    </div>
  );
}
