import { Link } from "react-router";
import { useApi } from "../lib/hooks";
import { useTranslation } from "react-i18next";
import { formatDateTime } from "../lib/format";

interface RuntimeNode {
  node_id: string;
  kind?: string;
  health?: string;
  version?: string | null;
  last_heartbeat_at?: string | null;
  heartbeat_age_seconds?: number | null;
}
interface RuntimeList {
  available?: boolean;
  total?: number;
  message?: string;
  reason?: string;
  items?: RuntimeNode[];
}

function ageLabel(node: RuntimeNode, neverLabel: string): string {
  if (!node.last_heartbeat_at) return neverLabel;
  const age = node.heartbeat_age_seconds;
  if (age === null || age === undefined) return formatDateTime(node.last_heartbeat_at);
  if (age < 90) return `${Math.round(age)}s`;
  if (age < 5400) return `${Math.round(age / 60)}m`;
  return `${Math.round(age / 3600)}h`;
}

export function ActivityRuntimeLive() {
  const { t } = useTranslation();
  const list = useApi<RuntimeList>("/api/runtime?limit=20", { refetchIntervalMs: 10000 });
  const items = list.data?.items ?? [];
  if (list.loading && !list.data)
    return <p className="activity-note">{t("activity.runtimeLoading")}</p>;
  if (list.error)
    return (
      <p className="activity-note">
        {t("activity.runtimeError")}（{list.error.message}）。{" "}
        <Link to="/runtime">{t("activity.runtimeOpen")}</Link>
      </p>
    );
  if (list.data?.available === false)
    return (
      <p className="activity-note">
        {list.data.message ?? list.data.reason ?? t("activity.runtimeUnavailable")}。{" "}
        <Link to="/runtime">{t("activity.liveDetails")}</Link>
      </p>
    );
  if (items.length === 0)
    return (
      <p className="activity-note">
        {t("activity.runtimeEmpty")} <Link to="/runtime">{t("activity.runtimeOpen")}</Link>
      </p>
    );
  return (
    <div className="activity-live-list">
      <ul className="space-y-2" aria-label={t("activity.runtimeNodes")}>
        {items.map((node) => (
          <li key={node.node_id} className="flex min-w-0 items-center gap-2 text-xs">
            <span
              className={`badge badge-sm ${node.health === "ok" ? "badge-success" : node.health === "degraded" ? "badge-warning" : node.health === "broken" ? "badge-error" : "badge-ghost"}`}
            >
              {node.health ?? t("activity.unknown")}
            </span>
            <span className="min-w-0 flex-1 truncate">{node.node_id}</span>
            <span className="shrink-0 text-muted">
              {ageLabel(node, t("activity.neverHeartbeat"))}
            </span>
          </li>
        ))}
      </ul>
      <p className="mt-2 text-right text-xs">
        <Link to="/runtime" className="text-primary hover:underline">
          {t("activity.runtimeAll")}（{list.data?.total ?? items.length}）
        </Link>
      </p>
    </div>
  );
}
