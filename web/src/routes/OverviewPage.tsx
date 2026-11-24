import { BookOpen, ClipboardList, Play, ShieldCheck, ArrowUpRight } from "lucide-react";
import type { ReactNode } from "react";
import { Link } from "react-router";
import { useApi } from "../lib/hooks";
import { EmptyPanel, ErrorPanel, SkeletonPanel } from "../components/ui";
import { formatDateTime } from "../lib/format";
import { useUiText } from "../i18n";

interface OverviewData {
  knowledge?: {
    total?: number;
    by_lifecycle?: Record<string, number>;
    by_verified?: Record<string, number>;
  };
  tasks?: { total?: number; by_state?: Record<string, number> };
  evidence?: { total?: number };
  runs?: {
    available?: boolean;
    total?: number;
    by_status?: Record<string, number>;
    reason?: string;
  };
}

interface StatsData {
  reviews?: {
    total?: number;
    by_queue?: Record<string, { total?: number; by_decision?: Record<string, number> }>;
  };
}

interface RuntimeNode {
  node_id: string;
  kind?: string;
  endpoint?: string | null;
  health?: string;
  version?: string | null;
  last_heartbeat_at?: string | null;
  heartbeat_age_seconds?: number | null;
}

interface RuntimeData {
  available?: boolean;
  total?: number;
  message?: string;
  reason?: string;
  items?: RuntimeNode[];
}

interface RecentRun {
  run_id: string;
  tool_id?: string;
  status?: string;
  error_category?: string | null;
  duration_ms?: number | null;
  created_at?: string | null;
  finished_at?: string | null;
  trace_id?: string | null;
}

interface RecentRunsData {
  available?: boolean;
  total?: number;
  message?: string;
  reason?: string;
  items?: RecentRun[];
}

interface KpiSpec {
  label: string;
  value: string;
  note?: string;
  to: string;
  icon: typeof BookOpen;
}

const QUEUE_LABELS: Record<string, string> = {
  d5: "D5 duplicate pairs",
  license: "License review",
  mapping: "Mapping review",
};

const RUN_STATUS_TONES: Record<string, string> = {
  SUCCEEDED: "badge-success",
  FAILED: "badge-error",
  TIMED_OUT: "badge-error",
  CANCELLED: "badge-warning",
  RUNNING: "badge-info",
  PREPARING: "badge-info",
  WAITING_FOR_RUNNER: "badge-ghost",
  QUEUED: "badge-ghost",
};

const RUNTIME_KIND_LABELS: Record<string, string> = {
  rust_native: "Rust Native Runtime",
  python_worker: "Python Worker",
  podman_host: "Podman Host",
  podman_vm: "Podman VM",
};

function greeting(ui: (source: string) => string): string {
  const hour = new Date().getHours();
  if (hour < 12) return ui("Good morning");
  if (hour < 18) return ui("Good afternoon");
  return ui("Good evening");
}

function verifiedNote(data: OverviewData["knowledge"], ui: (source: string) => string) {
  const verified = data?.by_verified?.verified ?? 0;
  if (verified <= 0) return undefined;
  return String(verified) + " " + ui("verified");
}

function taskNote(data: OverviewData["tasks"], ui: (source: string) => string) {
  const entries = Object.entries(data?.by_state ?? {});
  if (entries.length === 0) return undefined;
  const labels: Record<string, string> = {
    ACTIVE: "active",
    NEEDS_REVIEW: "needs review",
    COMPLETED: "completed",
    FAILED: "failed",
    PAUSED: "paused",
  };
  return entries
    .sort((a, b) => b[1] - a[1])
    .slice(0, 3)
    .map(([state, count]) => String(count) + " " + ui(labels[state] ?? state.toLowerCase()))
    .join(" · ");
}

function successfulRunNote(data: OverviewData["runs"], ui: (source: string) => string) {
  if (!data?.available || !data.total || data.total <= 0) return undefined;
  const succeeded = data.by_status?.SUCCEEDED ?? 0;
  const percent = Math.round((succeeded / data.total) * 100);
  return String(percent) + "% " + ui("succeeded");
}

function runtimeHealthLabel(health: string | undefined, ui: (source: string) => string) {
  switch (health) {
    case "ok":
      return ui("Healthy");
    case "degraded":
      return ui("Degraded");
    case "broken":
      return ui("Offline");
    default:
      return ui("Unknown");
  }
}

function runtimeHealthTone(health: string | undefined) {
  switch (health) {
    case "ok":
      return "badge-success";
    case "degraded":
      return "badge-warning";
    case "broken":
      return "badge-error";
    default:
      return "badge-ghost";
  }
}

function heartbeatLabel(node: RuntimeNode, ui: (source: string) => string) {
  if (!node.last_heartbeat_at) return ui("No heartbeat yet");
  const age =
    node.heartbeat_age_seconds ??
    Math.max(0, (Date.now() - Date.parse(node.last_heartbeat_at)) / 1000);
  if (!Number.isFinite(age)) return formatDateTime(node.last_heartbeat_at);
  if (age < 60) return String(Math.max(1, Math.round(age))) + " " + ui("seconds ago");
  if (age < 3600) return String(Math.round(age / 60)) + " " + ui("minutes ago");
  if (age < 86400) return String(Math.round(age / 3600)) + " " + ui("hours ago");
  return formatDateTime(node.last_heartbeat_at);
}

function KpiCard({ spec }: { spec: KpiSpec }) {
  const Icon = spec.icon;
  return (
    <Link
      to={spec.to}
      className="group block min-w-0 rounded-surface border border-edge bg-base-100 p-4 transition-colors hover:border-primary"
    >
      <div className="flex items-center justify-between gap-3">
        <p className="truncate text-xs font-medium text-muted">{spec.label}</p>
        <Icon
          className="h-4 w-4 shrink-0 text-muted transition-colors group-hover:text-primary"
          aria-hidden="true"
        />
      </div>
      <p className="mt-2 text-2xl font-semibold tabular-nums tracking-tight">{spec.value}</p>
      {spec.note ? (
        <p className="mt-1 truncate text-[11px] text-muted" title={spec.note}>
          {spec.note}
        </p>
      ) : null}
    </Link>
  );
}

function PanelHeader({
  title,
  description,
  trailing,
}: {
  title: string;
  description: string;
  trailing?: ReactNode;
}) {
  return (
    <div className="flex min-h-12 items-center justify-between gap-3 border-b border-edge px-4 py-3">
      <div className="min-w-0">
        <h2 className="text-sm font-semibold">{title}</h2>
        <p className="mt-0.5 text-[11px] text-muted">{description}</p>
      </div>
      {trailing}
    </div>
  );
}

export function OverviewPage() {
  const ui = useUiText();
  const overview = useApi<OverviewData>("/api/overview");
  const stats = useApi<StatsData>("/api/stats");
  const runtime = useApi<RuntimeData>("/api/runtime?limit=5");
  const recentRuns = useApi<RecentRunsData>("/api/runs?limit=5");
  const data = overview.data;

  const pendingQueues = Object.entries(stats.data?.reviews?.by_queue ?? {})
    .map(([key, queue]) => ({
      key,
      label: ui(QUEUE_LABELS[key] ?? key),
      total: queue.total ?? 0,
      pending: queue.by_decision?.PENDING ?? queue.by_decision?.pending ?? 0,
    }))
    .filter((queue) => queue.pending > 0)
    .sort((a, b) => b.pending - a.pending);
  const pendingTotal = pendingQueues.reduce((sum, queue) => sum + queue.pending, 0);

  const kpis: KpiSpec[] = [
    {
      label: ui("Knowledge items"),
      value: String(data?.knowledge?.total ?? 0),
      note: verifiedNote(data?.knowledge, ui),
      to: "/knowledge",
      icon: BookOpen,
    },
    {
      label: ui("Research tasks"),
      value: String(data?.tasks?.total ?? 0),
      note: taskNote(data?.tasks, ui),
      to: "/research/tasks",
      icon: ClipboardList,
    },
    {
      label: ui("Tool runs"),
      value: String(data?.runs?.total ?? 0),
      note: successfulRunNote(data?.runs, ui),
      to: "/runs",
      icon: Play,
    },
    {
      label: ui("Evidence records"),
      value: String(data?.evidence?.total ?? 0),
      note: ui("Read-only records"),
      to: "/research/evidence",
      icon: ShieldCheck,
    },
  ];

  const runtimeNodes = runtime.data?.items ?? [];
  const allRuntimeHealthy =
    runtimeNodes.length > 0 && runtimeNodes.every((node) => node.health === "ok");
  const runItems = recentRuns.data?.items ?? [];

  if (overview.loading) return <SkeletonPanel lines={8} />;
  if (overview.error !== null) {
    return (
      <div className="mx-auto w-full max-w-5xl">
        <ErrorPanel error={overview.error} onRetry={overview.retry} />
      </div>
    );
  }

  return (
    <div className="space-y-4">
      <header className="flex flex-wrap items-end justify-between gap-4">
        <div className="min-w-0">
          <h1 className="text-xl font-bold tracking-tight">{greeting(ui)}</h1>
          <p className="mt-1 text-xs text-muted">
            {ui("Research workspace overview · Activity and governance at a glance")}
          </p>
        </div>
        <Link
          to="/research"
          className="inline-flex min-h-9 items-center gap-2 rounded-control bg-primary px-3 text-xs font-semibold text-primary-content transition-opacity hover:opacity-90"
        >
          {ui("Open research workspace")}
          <ArrowUpRight className="h-3.5 w-3.5" aria-hidden="true" />
        </Link>
      </header>

      <section
        aria-label={ui("Key metrics")}
        className="grid grid-cols-1 gap-3 sm:grid-cols-2 2xl:grid-cols-4"
      >
        {kpis.map((spec) => (
          <KpiCard key={spec.label} spec={spec} />
        ))}
      </section>

      <div className="grid min-w-0 gap-4 xl:grid-cols-[minmax(0,1.7fr)_minmax(300px,1fr)]">
        <section className="min-w-0 overflow-hidden rounded-surface border border-edge bg-base-100">
          <PanelHeader
            title={ui("Needs attention")}
            description={ui("Items waiting for your decision")}
            trailing={
              <span className="badge badge-warning badge-sm tabular-nums">
                {pendingTotal} {ui("pending")}
              </span>
            }
          />
          {stats.loading ? (
            <div className="p-4">
              <SkeletonPanel lines={3} />
            </div>
          ) : stats.error !== null ? (
            <div className="p-4">
              <ErrorPanel error={stats.error} onRetry={stats.retry} />
            </div>
          ) : pendingQueues.length === 0 ? (
            <div className="p-4">
              <EmptyPanel text={ui("No pending review items.")} />
            </div>
          ) : (
            <ul className="divide-y divide-edge">
              {pendingQueues.map((queue) => (
                <li key={queue.key} className="flex min-w-0 items-center gap-3 px-4 py-3">
                  <span className="grid h-8 w-8 shrink-0 place-items-center rounded-control bg-base-200 text-primary">
                    <ShieldCheck className="h-4 w-4" aria-hidden="true" />
                  </span>
                  <div className="min-w-0 flex-1">
                    <p className="truncate text-xs font-semibold">{queue.label}</p>
                    <p className="mt-0.5 truncate text-[11px] text-muted">
                      {queue.pending} {ui("items pending")}
                      {queue.total > queue.pending
                        ? " · " + ui("total decisions") + " " + queue.total
                        : ""}
                    </p>
                  </div>
                  <span className="badge badge-warning badge-sm">{ui("Review")}</span>
                </li>
              ))}
            </ul>
          )}
        </section>

        <section className="min-w-0 overflow-hidden rounded-surface border border-edge bg-base-100">
          <PanelHeader
            title={ui("Runtime health")}
            description={ui("Registered execution environments")}
            trailing={
              runtime.loading ? (
                <span className="badge badge-ghost badge-sm">{ui("Checking")}</span>
              ) : runtime.error !== null || runtime.data?.available === false ? (
                <span className="badge badge-warning badge-sm">{ui("Unavailable")}</span>
              ) : allRuntimeHealthy ? (
                <span className="badge badge-success badge-sm">
                  {ui("All systems operational")}
                </span>
              ) : (
                <span className="badge badge-warning badge-sm">{ui("Check status")}</span>
              )
            }
          />
          {runtime.loading ? (
            <div className="p-4">
              <SkeletonPanel lines={3} />
            </div>
          ) : runtime.error !== null ? (
            <div className="p-4">
              <ErrorPanel error={runtime.error} onRetry={runtime.retry} />
            </div>
          ) : runtime.data?.available === false ? (
            <div className="p-4">
              <EmptyPanel text={runtime.data.message ?? ui("Runtime data unavailable.")} />
            </div>
          ) : runtimeNodes.length === 0 ? (
            <div className="p-4">
              <EmptyPanel text={ui("No registered runtime nodes.")} />
            </div>
          ) : (
            <ul className="divide-y divide-edge">
              {runtimeNodes.map((node) => (
                <li key={node.node_id} className="flex min-w-0 items-center gap-3 px-4 py-3">
                  <span
                    className={
                      "h-2 w-2 shrink-0 rounded-full " +
                      (node.health === "ok"
                        ? "bg-success"
                        : node.health === "broken"
                          ? "bg-error"
                          : "bg-warning")
                    }
                    aria-hidden="true"
                  />
                  <div className="min-w-0 flex-1">
                    <p className="truncate text-xs font-semibold">
                      {ui(RUNTIME_KIND_LABELS[node.kind ?? ""] ?? node.kind ?? "Runtime node")}
                    </p>
                    <p className="mt-0.5 truncate text-[11px] text-muted">
                      {node.node_id} · {heartbeatLabel(node, ui)}
                    </p>
                  </div>
                  <span className={"badge badge-sm " + runtimeHealthTone(node.health)}>
                    {runtimeHealthLabel(node.health, ui)}
                  </span>
                </li>
              ))}
            </ul>
          )}
        </section>
      </div>

      <section className="overflow-hidden rounded-surface border border-edge bg-base-100">
        <PanelHeader
          title={ui("Recent activity")}
          description={ui("Recent recorded runs and execution outcomes")}
          trailing={
            <Link
              to="/runs"
              className="rounded-control border border-edge px-3 py-1.5 text-[11px] font-medium hover:border-primary"
            >
              {ui("Open activity")}
              <ArrowUpRight className="ml-1 inline h-3 w-3" aria-hidden="true" />
            </Link>
          }
        />
        {recentRuns.loading ? (
          <div className="p-4">
            <SkeletonPanel lines={3} />
          </div>
        ) : recentRuns.error !== null ? (
          <div className="p-4">
            <ErrorPanel error={recentRuns.error} onRetry={recentRuns.retry} />
          </div>
        ) : recentRuns.data?.available === false ? (
          <div className="p-4">
            <EmptyPanel text={recentRuns.data.message ?? ui("Run data unavailable.")} />
          </div>
        ) : runItems.length === 0 ? (
          <div className="p-4">
            <EmptyPanel text={ui("No recent activity yet.")} />
          </div>
        ) : (
          <ol className="divide-y divide-edge">
            {runItems.slice(0, 5).map((run) => (
              <li key={run.run_id} className="flex min-w-0 items-start gap-3 px-4 py-3">
                <time className="w-16 shrink-0 pt-0.5 font-mono text-[10px] tabular-nums text-muted">
                  {run.created_at ? formatDateTime(run.created_at) : "—"}
                </time>
                <span
                  className={
                    "mt-1.5 h-2 w-2 shrink-0 rounded-full " +
                    (run.status === "SUCCEEDED"
                      ? "bg-success"
                      : run.status === "FAILED" || run.status === "TIMED_OUT"
                        ? "bg-error"
                        : "bg-info")
                  }
                  aria-hidden="true"
                />
                <div className="min-w-0 flex-1">
                  <Link
                    to={"/runs/" + encodeURIComponent(run.run_id)}
                    className="break-all text-xs font-semibold hover:text-primary"
                  >
                    {run.tool_id ?? run.run_id}
                  </Link>
                  <p className="mt-0.5 truncate text-[11px] text-muted">
                    {run.status ?? "UNKNOWN"}
                    {run.error_category ? " · " + run.error_category : ""}
                    {run.duration_ms !== undefined && run.duration_ms !== null
                      ? " · " + run.duration_ms + " ms"
                      : ""}
                  </p>
                </div>
                <span
                  className={
                    "badge badge-sm " + (RUN_STATUS_TONES[run.status ?? ""] ?? "badge-ghost")
                  }
                >
                  {run.status ?? "UNKNOWN"}
                </span>
              </li>
            ))}
          </ol>
        )}
      </section>
    </div>
  );
}
