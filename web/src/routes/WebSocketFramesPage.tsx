import { useEffect, useMemo, useState } from "react";
import { apiGet } from "../lib/api";
import { useApi } from "../lib/hooks";
import { EmptyPanel, ErrorPanel, PageHeader, SkeletonPanel, StatusBadge } from "../components/ui";
import { useUiText } from "../i18n";
import { Pagination } from "../components/data/Pagination";

interface FrameRow {
  frame_id: string;
  connection_exchange_id: string;
  direction: string;
  sequence_no: number;
  opcode: string;
  payload_len: number;
  payload_body_sha256?: string | null;
  payload_body_len?: number;
  payload_body_fidelity?: string;
  masked?: number;
  captured_at: string;
  scope_decision: string;
}
interface FrameList {
  rows: FrameRow[];
  next_cursor: string;
  has_more: boolean;
  total: number;
  limit: number;
}
interface FrameDetail extends FrameRow {
  payload_encoding?: string;
  payload?: string;
}
const directionLabel: Record<string, string> = {
  client_to_server: "Client → Server",
  server_to_client: "Server → Client",
};

export function WebSocketFramesPage() {
  const ui = useUiText();
  const [opcode, setOpcode] = useState("");
  const [direction, setDirection] = useState("");
  const [selectedId, setSelectedId] = useState("");
  const [detail, setDetail] = useState<FrameDetail | null>(null);
  const [detailError, setDetailError] = useState<string | null>(null);
  const [payloadBusy, setPayloadBusy] = useState(false);
  const [payloadError, setPayloadError] = useState<string | null>(null);
  const [cursor, setCursor] = useState("");
  const [extraRows, setExtraRows] = useState<FrameRow[]>([]);
  const [pageBusy, setPageBusy] = useState(false);
  const [pageError, setPageError] = useState<string | null>(null);
  const params = new URLSearchParams({ limit: "100" });
  if (opcode) params.set("opcode", opcode);
  if (direction) params.set("direction", direction);
  const list = useApi<FrameList>(`/api/wsw/ws/frames?${params.toString()}`);
  const rows = useMemo(() => [...(list.data?.rows ?? []), ...extraRows], [list.data, extraRows]);
  useEffect(() => {
    setExtraRows([]);
    setCursor(list.data?.next_cursor ?? "");
    setPageError(null);
  }, [list.data]);

  async function selectFrame(row: FrameRow) {
    setSelectedId(row.frame_id);
    setDetail(null);
    setPayloadError(null);
    setDetailError(null);
    try {
      setDetail(
        await apiGet<FrameDetail>(`/api/wsw/ws/frames/${encodeURIComponent(row.frame_id)}`),
      );
    } catch (error) {
      setDetailError(String(error));
    }
  }
  async function loadPayload() {
    if (!selectedId) return;
    setPayloadBusy(true);
    setPayloadError(null);
    try {
      setDetail(
        await apiGet<FrameDetail>(`/api/wsw/ws/frames/${encodeURIComponent(selectedId)}?payload=1`),
      );
    } catch (error) {
      setPayloadError(String(error));
    } finally {
      setPayloadBusy(false);
    }
  }
  async function loadMore() {
    if (!cursor || pageBusy) return;
    setPageBusy(true);
    setPageError(null);
    try {
      const page = await apiGet<FrameList>(
        `/api/wsw/ws/frames?${params.toString()}&cursor=${encodeURIComponent(cursor)}`,
      );
      setExtraRows((previous) => [...previous, ...page.rows]);
      setCursor(page.next_cursor);
    } catch (error) {
      setPageError(String(error));
    } finally {
      setPageBusy(false);
    }
  }

  return (
    <div className="wsw-workspace-page" data-testid="wsw-websocket-page">
      <PageHeader
        title={ui("WebSocket Frames")}
        actions={<span className="badge badge-outline">{ui("只读")}</span>}
      />
      <p className="text-sm text-muted">
        {ui(
          "帧列表默认不读取 Payload；点击一帧后才会请求详情，Payload 仍需单独加载。Replay 会产生真实网络通信，本页面不提供自动重放。",
        )}
      </p>
      <section className="rounded-surface border border-edge bg-base-100 p-4">
        <div className="mb-3 flex flex-wrap items-end gap-3">
          <label className="wsw-field">
            <span>{ui("Opcode")}</span>
            <select
              className="select select-bordered select-sm"
              value={opcode}
              onChange={(e) => {
                setOpcode(e.target.value);
                setCursor("");
                setSelectedId("");
                setDetail(null);
              }}
              aria-label={ui("Opcode")}
            >
              <option value="">{ui("全部")}</option>
              {["text", "binary", "ping", "pong", "close"].map((v) => (
                <option key={v}>{v}</option>
              ))}
            </select>
          </label>
          <label className="wsw-field">
            <span>{ui("方向")}</span>
            <select
              className="select select-bordered select-sm"
              value={direction}
              onChange={(e) => {
                setDirection(e.target.value);
                setCursor("");
                setSelectedId("");
                setDetail(null);
              }}
              aria-label={ui("方向")}
            >
              <option value="">{ui("全部")}</option>
              <option value="client_to_server">Client → Server</option>
              <option value="server_to_client">Server → Client</option>
            </select>
          </label>
          <span className="ml-auto text-xs text-muted">
            {list.data?.total ?? 0} {ui("条记录")}
          </span>
        </div>
        {list.loading ? (
          <SkeletonPanel lines={4} />
        ) : list.error ? (
          <ErrorPanel error={list.error} onRetry={list.retry} />
        ) : (list.data?.rows.length ?? 0) === 0 ? (
          <EmptyPanel
            text={ui("尚无 WebSocket 帧记录。启动已配置的捕获会话后，帧将显示在这里。")}
          />
        ) : (
          <div className="overflow-x-auto">
            <table className="table table-sm">
              <thead>
                <tr>
                  <th>Seq</th>
                  <th>{ui("Opcode")}</th>
                  <th>{ui("方向")}</th>
                  <th>{ui("Payload 长度")}</th>
                  <th>{ui("Scope")}</th>
                  <th>{ui("时间")}</th>
                </tr>
              </thead>
              <tbody>
                {rows.map((row) => (
                  <tr
                    key={row.frame_id}
                    className={selectedId === row.frame_id ? "is-selected" : ""}
                  >
                    <td>
                      <button
                        type="button"
                        className="font-mono text-xs underline"
                        onClick={() => void selectFrame(row)}
                      >
                        {row.sequence_no}
                      </button>
                    </td>
                    <td>{row.opcode}</td>
                    <td className="text-xs">{directionLabel[row.direction] ?? row.direction}</td>
                    <td>{row.payload_len}</td>
                    <td>
                      <StatusBadge status={row.scope_decision} />
                    </td>
                    <td className="text-xs">{row.captured_at}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
        <Pagination
          loadedCount={rows.length}
          totalCount={list.data?.total}
          hasNext={Boolean(cursor)}
          loading={pageBusy}
          onNext={() => void loadMore()}
        />
        {pageError ? (
          <p role="alert" className="mt-2 text-xs text-error">
            {pageError}
          </p>
        ) : null}
      </section>
      {detail ? (
        <section className="rounded-surface border border-edge bg-base-100 p-4">
          <div className="mb-3 flex flex-wrap items-center justify-between gap-2">
            <h2 className="text-sm font-semibold">{ui("帧详情")}</h2>
            <span className="font-mono text-xs">{detail.frame_id}</span>
          </div>
          <dl className="grid gap-2 text-xs sm:grid-cols-2">
            <div>
              <dt className="text-muted">Connection</dt>
              <dd className="break-all">{detail.connection_exchange_id}</dd>
            </div>
            <div>
              <dt className="text-muted">Direction</dt>
              <dd>{directionLabel[detail.direction] ?? detail.direction}</dd>
            </div>
            <div>
              <dt className="text-muted">SHA-256</dt>
              <dd className="break-all">{detail.payload_body_sha256 ?? "—"}</dd>
            </div>
            <div>
              <dt className="text-muted">Fidelity</dt>
              <dd>{detail.payload_body_fidelity ?? "—"}</dd>
            </div>
          </dl>
          <div className="mt-3 border-t border-edge pt-3">
            <h3 className="text-xs font-semibold">{ui("Payload")}</h3>
            {detail.payload ? (
              <pre className="mt-2 max-h-80 overflow-auto whitespace-pre-wrap break-all rounded-control bg-base-200 p-3 text-xs">
                {detail.payload}
              </pre>
            ) : (
              <p className="my-2 text-xs text-muted">
                {ui("Payload 尚未加载。加载操作会从本地 CAS 读取该帧内容。")}
              </p>
            )}
            <button
              className="btn btn-sm mt-2"
              type="button"
              disabled={payloadBusy || Boolean(detail.payload)}
              onClick={() => void loadPayload()}
            >
              {payloadBusy ? ui("加载中…") : ui("按需加载 Payload（Base64）")}
            </button>
            {payloadError ? (
              <p role="alert" className="mt-2 text-xs text-error">
                {payloadError}
              </p>
            ) : null}
          </div>
        </section>
      ) : selectedId ? (
        <p role="alert" className="text-xs text-error">
          {detailError}
        </p>
      ) : null}
    </div>
  );
}
