import { useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { apiSend, ApiError } from "../lib/api";
import { ErrorPanel, PageHeader, StatusBadge } from "../components/ui";
import { useApi } from "../lib/hooks";

/** WSW-2 代理状态页（W2-D8 最小 UI 面）：Listener 六态徽标、bind 地址端口、
 *  Start/Stop（显式动作 + confirm）、Intercept 开关、连接/捕获/排队计数、
 *  错误五态计数与拦截队列摘要（只读）。数据源 = rp_server WSW 端点
 *  （Bearer 鉴权同既有面）；History/拦截编辑器/Repeater UI 留 W3+。 */

interface WswProxyStatus {
  state?: string;
  listener_id?: string | null;
  name?: string;
  bind?: string;
  intercept_enabled?: boolean;
  requests_total?: number;
  responses_total?: number;
  scope_blocked_total?: number;
  out_of_scope_passive_total?: number;
  queue_pending?: number;
  captures_total?: number;
  error_counts?: Record<string, number>;
  started_at?: string;
  note?: string;
}

interface WswQueueData {
  db?: string;
  exists?: boolean;
  rows?: Array<{
    intercept_id: string;
    direction: string;
    state: string;
    host?: string | null;
    method?: string | null;
    path?: string | null;
    queued_at: string;
    reason?: string | null;
  }>;
  counts?: Record<string, number>;
  note?: string;
}

const STATE_TONES: Record<string, string> = {
  running: "badge-success",
  degraded: "badge-warning",
  starting: "badge-info",
  stopping: "badge-info",
  stopped: "badge-ghost",
  failed: "badge-error",
};

function actionError(action: string, error: unknown): string {
  if (error instanceof ApiError) {
    return `失败（HTTP ${error.status}）：${error.message}`;
  }
  return `${action} 失败：${String(error)}`;
}

export function WswProxyPage() {
  const status = useApi<WswProxyStatus>("/api/wsw/proxy-status", {
    refetchIntervalMs: 3000,
  });
  const queue = useApi<WswQueueData>("/api/wsw/intercept-queue", {
    refetchIntervalMs: 5000,
  });
  const queryClient = useQueryClient();
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState<string | null>(null);

  const data = status.data ?? {};
  const state = data.state ?? "unknown";

  async function runAction(action: "listener/start" | "listener/stop", confirmText: string) {
    if (!window.confirm(confirmText)) return;
    setBusy(true);
    setMessage(null);
    try {
      const result = await apiSend<{ started?: boolean; stopped?: boolean; pid?: number; note?: string }>(
        "POST",
        `/api/wsw/${action}`,
        { confirm: true },
      );
      setMessage(
        action === "listener/start"
          ? `listener 已受理启动（pid ${result.pid ?? "?"}）；状态见上方六态徽标`
          : `listener 已停止${result.note ? `（${result.note}）` : ""}`,
      );
      void queryClient.invalidateQueries();
    } catch (error) {
      setMessage(actionError(action === "listener/start" ? "启动" : "停止", error));
    } finally {
      setBusy(false);
    }
  }

  return (
    <main className="page" data-testid="wsw-proxy-page">
      <PageHeader
        title="WSW 代理状态"
        actions={
          <>
            <button
              type="button"
              className="btn btn-primary"
              disabled={busy || state === "running"}
              data-testid="wsw-listener-start"
              onClick={() =>
                runAction(
                  "listener/start",
                  "启动 WSW 代理（只听 127.0.0.1，scope 闸后端强制）？",
                )
              }
            >
              启动代理
            </button>
            <button
              type="button"
              className="btn"
              disabled={busy || state !== "running"}
              data-testid="wsw-listener-stop"
              onClick={() =>
                runAction("listener/stop", "停止代理并收口拦截队列（全部 EXPIRED）？")
              }
            >
              停止代理
            </button>
          </>
        }
      />
      <p className="muted">
        MITM 代理只绑定 loopback（127.0.0.1/::1）；Scope 闸只在 Rust 后端强制，
        超范围请求 403 且拒绝记录可见。拦截队列三动作 Forward/Drop/Modified-Forward，
        TTL/队满/停机全部 EXPIRED 终态。
      </p>
      {message && <p className="badge-info" data-testid="wsw-action-message">{message}</p>}
      {status.error && (
        <ErrorPanel error={status.error} onRetry={status.retry} />
      )}
      {status.loading && !status.data && <p className="muted">加载中…</p>}
      {status.data && (
        <section className="panel" data-testid="wsw-proxy-summary">
          <h2>
            六态 <StatusBadge status={state} tones={STATE_TONES} />
          </h2>
          <dl className="kv">
            <dt>listener</dt>
            <dd data-testid="wsw-listener-id">{data.listener_id ?? "-"}</dd>
            <dt>bind</dt>
            <dd>{data.bind ?? "-"}</dd>
            <dt>intercept</dt>
            <dd>{data.intercept_enabled ? "开启（命中请求入队）" : "关闭（默认）"}</dd>
            <dt>累计请求 / 响应</dt>
            <dd>
              {data.requests_total ?? 0} / {data.responses_total ?? 0}
            </dd>
            <dt>scope 拒绝（可见记录）</dt>
            <dd data-testid="wsw-scope-blocked">{data.scope_blocked_total ?? 0}</dd>
            <dt>out-of-scope 放行（passive_forward）</dt>
            <dd>{data.out_of_scope_passive_total ?? 0}</dd>
            <dt>CAS 捕获登记</dt>
            <dd>{data.captures_total ?? 0}</dd>
            <dt>队列 pending</dt>
            <dd>{data.queue_pending ?? 0}</dd>
            <dt>启动时刻</dt>
            <dd>{data.started_at ?? "-"}</dd>
            {data.note && (
              <>
                <dt>说明</dt>
                <dd>{data.note}</dd>
              </>
            )}
          </dl>
        </section>
      )}
      {data.error_counts && Object.keys(data.error_counts).length > 0 && (
        <section className="panel">
          <h2>错误五态计数</h2>
          <ul>
            {Object.entries(data.error_counts).map(([cls, count]) => (
              <li key={cls}>
                {cls}：{count}
              </li>
            ))}
          </ul>
        </section>
      )}
      <section className="panel" data-testid="wsw-queue-summary">
        <h2>拦截队列摘要（只读，最近 50 行）</h2>
        {queue.error && <ErrorPanel error={queue.error} onRetry={queue.retry} />}
        {queue.data && (queue.data.rows?.length ?? 0) === 0 && (
          <p className="muted">{queue.data.note ?? "队列为空"}</p>
        )}
        {(queue.data?.rows?.length ?? 0) > 0 && (
          <table>
            <thead>
              <tr>
                <th>intercept_id</th>
                <th>方向</th>
                <th>五态</th>
                <th>host</th>
                <th>method</th>
                <th>path</th>
                <th>入队</th>
                <th>reason</th>
              </tr>
            </thead>
            <tbody>
              {queue.data?.rows?.map((row) => (
                <tr key={row.intercept_id}>
                  <td>{row.intercept_id}</td>
                  <td>{row.direction}</td>
                  <td>{row.state}</td>
                  <td>{row.host ?? "-"}</td>
                  <td>{row.method ?? "-"}</td>
                  <td>{row.path ?? "-"}</td>
                  <td>{row.queued_at}</td>
                  <td>{row.reason ?? "-"}</td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
        {queue.data?.counts && Object.keys(queue.data.counts).length > 0 && (
          <p className="muted">
            计数：
            {Object.entries(queue.data.counts)
              .map(([k, v]) => `${k}=${v}`)
              .join("，")}
          </p>
        )}
      </section>
    </main>
  );
}
