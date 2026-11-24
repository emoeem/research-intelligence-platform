import { Link } from "react-router";
import { useApi } from "../lib/hooks";
import { useTranslation } from "react-i18next";
import { formatDateTime } from "../lib/format";

interface RunRow {
  run_id: string;
  tool_id: string;
  status?: string;
  error_category?: string | null;
  duration_ms?: number | null;
  created_at?: string | null;
  finished_at?: string | null;
}
interface RunList {
  available?: boolean;
  total?: number;
  source?: string;
  message?: string;
  reason?: string;
  items?: RunRow[];
}

const ACTIVE = new Set(["QUEUED", "WAITING_FOR_RUNNER", "PREPARING", "RUNNING"]);

export function ActivityRunsLive() {
  const { t } = useTranslation();
  const list = useApi<RunList>("/api/runs?limit=12", { refetchIntervalMs: 5000 });
  const items = list.data?.items ?? [];
  if (list.loading && !list.data) {
    return <p className="activity-note">{t("activity.liveLoading")}</p>;
  }
  if (list.error) {
    return (
      <p className="activity-note">
        {t("activity.liveError")}（{list.error.message}）。{" "}
        <Link to="/runs">{t("activity.liveOpen")}</Link>
      </p>
    );
  }
  if (list.data?.available === false) {
    return (
      <p className="activity-note">
        {list.data.message ?? list.data.reason ?? t("activity.liveUnavailable")}。{" "}
        <Link to="/runs">{t("activity.liveDetails")}</Link>
      </p>
    );
  }
  if (items.length === 0) {
    return (
      <p className="activity-note">
        {t("activity.liveEmpty")} <Link to="/runs">{t("activity.liveOpen")}</Link>
      </p>
    );
  }
  return (
    <div className="activity-live-list">
      <ul className="space-y-2" aria-label={t("activity.liveRecent")}>
        {items.map((run) => (
          <li key={run.run_id} className="flex min-w-0 items-center gap-2 text-xs">
            <span
              className={`badge badge-sm ${ACTIVE.has(run.status ?? "") ? "badge-info" : run.status === "SUCCEEDED" ? "badge-success" : run.status === "FAILED" || run.status === "TIMED_OUT" ? "badge-error" : "badge-ghost"}`}
            >
              {run.status ?? t("activity.liveUnknown")}
            </span>
            <Link
              to={`/runs/${encodeURIComponent(run.run_id)}`}
              className="min-w-0 flex-1 truncate hover:underline"
            >
              {run.tool_id} · {run.run_id}
            </Link>
            <span className="shrink-0 text-muted">
              {run.created_at ? formatDateTime(run.created_at) : ""}
            </span>
          </li>
        ))}
      </ul>
      <p className="mt-2 text-right text-xs">
        <Link to="/runs" className="text-primary hover:underline">
          {t("activity.liveAll")}（{list.data?.total ?? items.length}）
        </Link>
      </p>
    </div>
  );
}
