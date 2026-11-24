import { Link, useLocation } from "react-router";
import { useApi } from "../lib/hooks";
import { useTranslation } from "react-i18next";
import { formatDateTime } from "../lib/format";
import { LogViewer, type LogEntry } from "./feedback/LogViewer";
import { JsonViewer } from "./editor/JsonViewer";
import {
  ErrorState,
  LoadingState,
  OfflineState,
  PartialDataState,
  UnauthorizedState,
} from "./feedback/StatePanels";

interface RunSummary {
  run_id: string;
  tool_id: string;
  status?: string;
  error_category?: string | null;
  created_at?: string | null;
}
interface RunList {
  available?: boolean;
  total?: number;
  message?: string;
  reason?: string;
  items?: RunSummary[];
}
interface RunArtifact {
  artifact_id: string;
  role?: string | null;
  uri: string;
  mime_type: string;
  size_bytes: number;
  created_at?: string | null;
}
interface RunDetail {
  run?: {
    run_id: string;
    status?: string;
    error_category?: string | null;
    error_json?: unknown;
    created_at?: string | null;
    started_at?: string | null;
    finished_at?: string | null;
    updated_at?: string | null;
  };
  artifacts?: RunArtifact[];
  artifacts_note?: string | null;
}

export function ActivityRunDetails({ mode }: { mode: "logs" | "output" }) {
  const { t } = useTranslation();
  const location = useLocation();
  const runId = location.pathname.match(/^\/runs\/([^/]+)$/)?.[1] ?? null;
  const list = useApi<RunList>("/api/runs?limit=20", { refetchIntervalMs: 5000 });
  const detail = useApi<RunDetail>(runId ? `/api/runs?run_id=${encodeURIComponent(runId)}` : null, {
    refetchIntervalMs: 5000,
  });

  if (mode === "logs") {
    if (detail.loading && runId && !detail.data)
      return <LoadingState label={t("activity.detailLoading")} />;
    if (detail.error) {
      if (detail.error.status === 401 || detail.error.status === 403)
        return (
          <UnauthorizedState label={t("activity.detailError")} detail={detail.error.message} />
        );
      if (detail.error.status && detail.error.status >= 400 && detail.error.status < 500)
        return <ErrorState label={t("activity.detailError")} detail={detail.error.message} />;
      return (
        <OfflineState
          label={t("activity.detailError")}
          detail={detail.error.message}
          onRetry={detail.retry}
        />
      );
    }
    if (runId && detail.data?.run) {
      const run = detail.data.run;
      const events: LogEntry[] = [
        ["created", run.created_at],
        ["started", run.started_at],
        ["finished", run.finished_at],
        ["updated", run.updated_at],
      ]
        .filter((entry): entry is [string, string] => Boolean(entry[1]))
        .map(([name, timestamp]) => ({
          id: `${run.run_id}-${name}`,
          timestamp: formatDateTime(timestamp),
          level:
            name === "finished" && ["FAILED", "TIMED_OUT"].includes(run.status ?? "")
              ? "error"
              : "info",
          source: "run.lifecycle",
          message: name,
        }));
      if (run.error_category)
        events.push({
          id: `${run.run_id}-error-category`,
          level: "error",
          source: "run.error",
          message: run.error_category,
        });
      return (
        <div className="activity-live-list">
          <p className="mb-2 text-xs">
            {run.run_id} · {run.status ?? t("activity.unknown")}
          </p>
          <p className="mb-2 text-xs text-muted">
            Run 生命周期事件与后端提供的错误载荷；不是 stdout/stderr 实时流。
          </p>
          {events.length ? (
            <LogViewer
              entries={events}
              label="Run 事件"
              height={240}
              emptyText={t("activity.noErrorPayload")}
            />
          ) : (
            <p className="activity-note">{t("activity.noErrorPayload")}</p>
          )}
          {run.error_json !== null && run.error_json !== undefined ? (
            <div className="mt-3">
              <JsonViewer value={run.error_json} label="Run 错误载荷" />
            </div>
          ) : null}
        </div>
      );
    }
    const failures = (list.data?.items ?? []).filter((run) =>
      ["FAILED", "TIMED_OUT"].includes(run.status ?? ""),
    );
    if (list.loading && !list.data) return <LoadingState label={t("activity.detailLoading")} />;
    if (list.error) {
      if (list.error.status === 401 || list.error.status === 403)
        return (
          <UnauthorizedState label={t("activity.logsUnavailable")} detail={list.error.message} />
        );
      if (list.error.status && list.error.status >= 400 && list.error.status < 500)
        return <ErrorState label={t("activity.logsUnavailable")} detail={list.error.message} />;
      return (
        <OfflineState
          label={t("activity.logsUnavailable")}
          detail={list.error.message}
          onRetry={list.retry}
        />
      );
    }
    return (
      <div className="activity-live-list">
        <p className="activity-note">{t("activity.failureEvents")}</p>
        {failures.length ? (
          <ul className="space-y-2">
            {failures.map((run) => (
              <li key={run.run_id} className="flex items-center gap-2 text-xs">
                <span className="badge badge-error">{run.status}</span>
                <Link
                  className="min-w-0 flex-1 truncate hover:underline"
                  to={`/runs/${encodeURIComponent(run.run_id)}`}
                >
                  {run.tool_id} · {run.run_id}
                </Link>
                <span className="text-muted">{run.error_category ?? ""}</span>
                <span className="text-muted">
                  {run.created_at ? formatDateTime(run.created_at) : ""}
                </span>
              </li>
            ))}
          </ul>
        ) : (
          <p className="activity-note">{t("activity.noFailures")}</p>
        )}
      </div>
    );
  }

  if (!runId)
    return (
      <p className="activity-note">
        {t("activity.selectRunForOutput")} <Link to="/runs">{t("inspector.runs")}</Link>
      </p>
    );
  if (detail.loading && !detail.data) return <LoadingState label={t("activity.detailLoading")} />;
  if (detail.error) {
    if (detail.error.status === 401 || detail.error.status === 403)
      return <UnauthorizedState label={t("activity.detailError")} detail={detail.error.message} />;
    if (detail.error.status && detail.error.status >= 400 && detail.error.status < 500)
      return <ErrorState label={t("activity.detailError")} detail={detail.error.message} />;
    return (
      <OfflineState
        label={t("activity.detailError")}
        detail={detail.error.message}
        onRetry={detail.retry}
      />
    );
  }
  const artifacts = detail.data?.artifacts ?? [];
  return (
    <div className="activity-live-list">
      <p className="text-xs">{runId}</p>
      {detail.data?.artifacts_note && artifacts.length > 0 ? (
        <PartialDataState detail={detail.data.artifacts_note} />
      ) : detail.data?.artifacts_note ? (
        <p className="activity-note">{detail.data.artifacts_note}</p>
      ) : null}
      {artifacts.length ? (
        <ul className="space-y-2">
          {artifacts.map((artifact) => (
            <li key={artifact.artifact_id} className="flex min-w-0 items-center gap-2 text-xs">
              <span className="badge badge-outline">{artifact.role ?? "artifact"}</span>
              <span className="min-w-0 flex-1 truncate">{artifact.artifact_id}</span>
              <span className="text-muted">
                {artifact.mime_type} · {artifact.size_bytes} B
              </span>
              <a
                href={artifact.uri}
                target="_blank"
                rel="noreferrer"
                className="text-primary underline"
              >
                {t("activity.openArtifact")}
              </a>
            </li>
          ))}
        </ul>
      ) : (
        <p className="activity-note">{t("activity.noArtifacts")}</p>
      )}
    </div>
  );
}
