import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useNavigate } from "react-router";
import { useQueryClient } from "@tanstack/react-query";
import { apiGet, apiSend, ApiError } from "../lib/api";
import { ErrorPanel, PageHeader } from "../components/ui";
import { VirtualList } from "../components/VirtualList";
import { MessageViews, parseQueryParams, type HeaderPair } from "../components/MessageViews";

/** HTTP History 页（WSW-3-ADR W3-D8.3；PLAN §5.3 :396-404 布局规格）：
 * 顶部过滤构建器（防抖 300ms）→ 虚拟滚动列表（默认时间倒序，自研窗口化
 * 仅视口渲染）→ 下方详情区（Raw/Pretty/Headers/Params/Body/Hex 六视图 +
 * Send to Repeater 行操作）。数据 = /api/wsw/history 游标分页 + 按需
 * /body 端点；不用静态假数据（PLAN :436）。 */

interface HistoryRow {
  exchange_id: string;
  session_id: string;
  listener_id: string;
  sequence_no: number;
  direction: string;
  source: string;
  protocol: string;
  http_version?: string | null;
  method?: string | null;
  scheme?: string | null;
  authority?: string | null;
  path?: string | null;
  response_status?: number | null;
  request_body_len?: number | null;
  request_mime?: string | null;
  response_body_len?: number | null;
  response_mime?: string | null;
  started_at?: string | null;
  completed_at?: string | null;
  duration_ms?: number | null;
  ttfb_ms?: number | null;
  remote_ip?: string | null;
  scope_decision?: string | null;
  edited?: number;
  tags_json?: string;
  capture_fidelity?: string;
  parse_status?: string;
  error_code?: string | null;
}

interface HistoryListData {
  rows: HistoryRow[];
  next_cursor: string;
  has_more: boolean;
  total: number;
  limit: number;
}

interface HistoryDetail {
  exchange_id: string;
  method?: string | null;
  scheme?: string | null;
  authority?: string | null;
  path?: string | null;
  response_status?: number | null;
  request_headers: HeaderPair[];
  response_headers: HeaderPair[];
  request_body_sha256?: string | null;
  request_body_uri?: string | null;
  request_body_len?: number | null;
  request_mime?: string | null;
  request_body_fidelity?: string | null;
  response_body_sha256?: string | null;
  response_body_uri?: string | null;
  response_body_len?: number | null;
  response_mime?: string | null;
  response_body_fidelity?: string | null;
  started_at?: string | null;
  completed_at?: string | null;
  duration_ms?: number | null;
  ttfb_ms?: number | null;
  scope_decision?: string | null;
  tags: string[];
  notes?: string | null;
  capture_fidelity?: string;
  parse_status?: string;
  error_code?: string | null;
}

interface BodyPayload {
  exchange_id: string;
  direction: string;
  mime: string;
  len: number;
  fidelity?: string;
  sha256: string;
  encoding: "base64";
  body: string;
}

const ROW_HEIGHT = 28;
const DEBOUNCE_MS = 300;

const SCOPE_TONES: Record<string, string> = {
  in_scope: "badge-success",
  out_of_scope_passive: "badge-warning",
  out_of_scope_blocked: "badge-error",
};

const STATUS_TONES: Record<string, string> = {
  "2": "badge-success",
  "3": "badge-info",
  "4": "badge-warning",
  "5": "badge-error",
};

function statusTone(status: number | null | undefined): string {
  if (!status) return "badge-ghost";
  const digit = String(status).charAt(0);
  return (digit && STATUS_TONES[digit]) || "badge-ghost";
}

/** 过滤构建器状态 → query 串（游标协议：过滤参与指纹，变更即新游标族）。 */
function filtersToQuery(f: FilterState, cursor: string): string {
  const params = new URLSearchParams();
  if (f.authority) params.set("authority", f.authority);
  if (f.method) params.set("method", f.method);
  if (f.path) params.set("path", f.path);
  if (f.status) params.set("status", f.status);
  if (f.mime) params.set("mime", f.mime);
  if (f.source) params.set("source", f.source);
  if (f.q) params.set("q", f.q);
  if (f.tags) params.set("tags", f.tags);
  if (f.from) params.set("from", f.from);
  if (f.to) params.set("to", f.to);
  params.set("limit", "200");
  if (cursor) params.set("cursor", cursor);
  return params.toString();
}

interface FilterState {
  authority: string;
  method: string;
  path: string;
  status: string;
  mime: string;
  source: string;
  q: string;
  tags: string;
  from: string;
  to: string;
}

const EMPTY_FILTERS: FilterState = {
  authority: "",
  method: "",
  path: "",
  status: "",
  mime: "",
  source: "",
  q: "",
  tags: "",
  from: "",
  to: "",
};

export function WswHistoryPage() {
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  // 防抖过滤态（300ms；e2e §四.3.3 断言 5 次快速键入窗口内 ≤2 次请求）。
  const [draftFilters, setDraftFilters] = useState<FilterState>(EMPTY_FILTERS);
  const [filters, setFilters] = useState<FilterState>(EMPTY_FILTERS);
  const [rows, setRows] = useState<HistoryRow[]>([]);
  const [cursor, setCursor] = useState("");
  const [hasMore, setHasMore] = useState(false);
  const [total, setTotal] = useState(0);
  const [listError, setListError] = useState<ApiError | null>(null);
  const [loadingList, setLoadingList] = useState(false);
  const [selected, setSelected] = useState<HistoryRow | null>(null);
  const [detail, setDetail] = useState<HistoryDetail | null>(null);
  const [detailError, setDetailError] = useState<string | null>(null);
  const [requestBody, setRequestBody] = useState<BodyPayload | null>(null);
  const [responseBody, setResponseBody] = useState<BodyPayload | null>(null);
  const [bodyNote, setBodyNote] = useState<string | null>(null);
  const [actionMessage, setActionMessage] = useState<string | null>(null);
  const listLoadedRef = useRef(false);

  // 防抖 300ms（PLAN :404 过滤构建器 + e2e 请求计数断言锚）。
  useEffect(() => {
    const timer = window.setTimeout(() => setFilters(draftFilters), DEBOUNCE_MS);
    return () => window.clearTimeout(timer);
  }, [draftFilters]);

  const query = useMemo(() => filtersToQuery(filters, ""), [filters]);

  // 首页加载（过滤族重置游标）。
  useEffect(() => {
    let cancelled = false;
    setLoadingList(true);
    setListError(null);
    apiGet<HistoryListData>(`/api/wsw/history?${query}`)
      .then((data) => {
        if (cancelled) return;
        setRows(data.rows);
        setCursor(data.next_cursor);
        setHasMore(data.has_more);
        setTotal(data.total);
        listLoadedRef.current = true;
      })
      .catch((error: ApiError) => {
        if (!cancelled) setListError(error);
      })
      .finally(() => {
        if (!cancelled) setLoadingList(false);
      });
    return () => {
      cancelled = true;
    };
  }, [query]);

  const loadMore = useCallback(async () => {
    if (!hasMore || loadingList || !cursor) return;
    setLoadingList(true);
    try {
      const data = await apiGet<HistoryListData>(
        `/api/wsw/history?${filtersToQuery(filters, cursor)}`,
      );
      setRows((prev) => {
        const seen = new Set(prev.map((r) => r.exchange_id));
        return [...prev, ...data.rows.filter((r) => !seen.has(r.exchange_id))];
      });
      setCursor(data.next_cursor);
      setHasMore(data.has_more);
    } catch (error) {
      setListError(error instanceof ApiError ? error : new ApiError(0, String(error)));
    } finally {
      setLoadingList(false);
    }
  }, [hasMore, loadingList, cursor, filters]);

  const selectRow = useCallback(async (row: HistoryRow) => {
    setSelected(row);
    setDetail(null);
    setRequestBody(null);
    setResponseBody(null);
    setBodyNote(null);
    setDetailError(null);
    try {
      const data = await apiGet<HistoryDetail>(`/api/wsw/history/${row.exchange_id}`);
      setDetail(data);
    } catch (error) {
      setDetailError(error instanceof ApiError ? error.message : String(error));
    }
  }, []);

  // Body 按需加载（W3-D3：列表/详情零 body 字节；打开 Body 视图才请求
  // ——e2e §四.3.4 断言「列表全程零 /body、详情恰一次」）。
  const loadBody = useCallback(async (direction: "request" | "response") => {
    if (!selected) return;
    const setter = direction === "request" ? setRequestBody : setResponseBody;
    setter(null);
    setBodyNote(null);
    try {
      const data = await apiGet<BodyPayload>(
        `/api/wsw/history/${selected.exchange_id}/body?direction=${direction}`,
      );
      setter(data);
    } catch (error) {
      setBodyNote(
        error instanceof ApiError
          ? `${direction} 体按需加载：${error.message}`
          : String(error),
      );
    }
  }, [selected]);

  async function sendToRepeater(row: HistoryRow) {
    setActionMessage(null);
    try {
      const result = await apiSend<{ tab_id: string; version_no: number; rebuild_fidelity: string }>(
        "POST",
        `/api/wsw/history/${row.exchange_id}/send-to-repeater`,
        {},
      );
      setActionMessage(
        `已送入 Repeater（tab ${result.tab_id} v${result.version_no}；保真 ${result.rebuild_fidelity}——头序/绝对形式不保证逐字节）`,
      );
      void queryClient.invalidateQueries();
      navigate(`/websec/repeater?tab=${encodeURIComponent(result.tab_id)}`);
    } catch (error) {
      setActionMessage(
        error instanceof ApiError ? `Send to Repeater 失败：${error.message}` : String(error),
      );
    }
  }

  async function saveTagsNotes() {
    if (!detail) return;
    try {
      const tags = (detail.tags ?? []).join(",");
      const input = window.prompt("编辑 tags（逗号分隔）", tags);
      if (input === null) return;
      await apiSend("PATCH", `/api/wsw/history/${detail.exchange_id}`, {
        tags: input.split(",").map((t) => t.trim()).filter(Boolean),
        notes: detail.notes ?? "",
      });
      const fresh = await apiGet<HistoryDetail>(
        `/api/wsw/history/${detail.exchange_id}`,
      );
      setDetail(fresh);
      void queryClient.invalidateQueries();
    } catch (error) {
      setActionMessage(
        error instanceof ApiError ? `保存失败：${error.message}` : String(error),
      );
    }
  }

  const totalText = `${total} 行${hasMore ? "（已载入 " + rows.length + "）" : ""}`;

  return (
    <main className="page" data-testid="wsw-history-page">
      <PageHeader title="HTTP History" />
      <p className="muted">
        捕获/发送交换行（025 单行纪律）；列表零 body 字节、Body 按需端点单响应帽
        8MiB；游标分页 (started_at, exchange_id) 倒序 + 过滤指纹守卫。
      </p>

      {/* 过滤构建器（防抖 300ms） */}
      <section className="panel" data-testid="wsw-history-filters">
        <div className="flex flex-wrap gap-2 items-center">
          <input
            className="input input-sm"
            placeholder="关键词搜索（authority/路径/头名/体前 4KiB）"
            data-testid="wsw-history-filter-q"
            value={draftFilters.q}
            onChange={(e) => setDraftFilters({ ...draftFilters, q: e.target.value })}
            style={{ minWidth: "16rem" }}
          />
          <select
            className="select select-sm"
            data-testid="wsw-history-filter-method"
            value={draftFilters.method}
            onChange={(e) => setDraftFilters({ ...draftFilters, method: e.target.value })}
          >
            <option value="">方法</option>
            {["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"].map((m) => (
              <option key={m} value={m}>
                {m}
              </option>
            ))}
          </select>
          <input
            className="input input-sm"
            placeholder="authority"
            data-testid="wsw-history-filter-authority"
            value={draftFilters.authority}
            onChange={(e) => setDraftFilters({ ...draftFilters, authority: e.target.value })}
            style={{ width: "10rem" }}
          />
          <input
            className="input input-sm"
            placeholder="路径前缀"
            data-testid="wsw-history-filter-path"
            value={draftFilters.path}
            onChange={(e) => setDraftFilters({ ...draftFilters, path: e.target.value })}
            style={{ width: "10rem" }}
          />
          <input
            className="input input-sm"
            placeholder="状态码"
            data-testid="wsw-history-filter-status"
            value={draftFilters.status}
            onChange={(e) => setDraftFilters({ ...draftFilters, status: e.target.value })}
            style={{ width: "5rem" }}
          />
          <select
            className="select select-sm"
            data-testid="wsw-history-filter-source"
            value={draftFilters.source}
            onChange={(e) => setDraftFilters({ ...draftFilters, source: e.target.value })}
          >
            <option value="">来源</option>
            <option value="proxy_capture">proxy_capture</option>
            <option value="repeater">repeater</option>
          </select>
          <button
            type="button"
            className="btn btn-sm"
            data-testid="wsw-history-filter-reset"
            onClick={() => setDraftFilters(EMPTY_FILTERS)}
          >
            重置
          </button>
          <span className="muted text-xs" data-testid="wsw-history-total">
            {loadingList ? "加载中…" : totalText}
          </span>
        </div>
      </section>

      {listError && <ErrorPanel error={listError} onRetry={() => setFilters({ ...filters })} />}

      {/* 虚拟滚动列表（仅视口渲染）+ 详情区 */}
      <section className="panel" style={{ height: "20rem" }} data-testid="wsw-history-list-panel">
        <VirtualList
          itemCount={rows.length}
          rowHeight={ROW_HEIGHT}
          testId="wsw-history-list"
          onScrollOffset={(offset) => {
            // 触底加载下一页（游标协议，禁 OFFSET）。
            if (hasMore && offset + 20 * ROW_HEIGHT >= rows.length * ROW_HEIGHT) {
              void loadMore();
            }
          }}
          renderItem={(index) => {
            const row = rows[index];
            if (!row) return null;
            const isSelected = selected?.exchange_id === row.exchange_id;
            return (
              <div
                role="row"
                data-testid="wsw-history-row"
                data-exchange-id={row.exchange_id}
                className={`flex items-center gap-2 px-2 text-xs w-full ${isSelected ? "bg-base-200" : ""}`}
                style={{ height: ROW_HEIGHT, cursor: "pointer" }}
                onClick={() => void selectRow(row)}
              >
                <span className={`badge badge-xs ${statusTone(row.response_status)}`}>
                  {row.response_status ?? "-"}
                </span>
                <span className="font-mono" style={{ width: "3.5rem" }}>
                  {row.method ?? "-"}
                </span>
                <span className="font-mono truncate flex-1">
                  {row.authority ?? "-"}
                  {row.path ?? "/"}
                </span>
                <span className="badge badge-ghost badge-xs">{row.source}</span>
                {row.scope_decision && (
                  <span className={`badge badge-xs ${SCOPE_TONES[row.scope_decision] ?? "badge-ghost"}`}>
                    {row.scope_decision}
                  </span>
                )}
                <span className="muted" style={{ width: "5rem", textAlign: "right" }}>
                  {row.duration_ms != null ? `${row.duration_ms}ms` : "-"}
                </span>
                <span className="muted" style={{ width: "9rem" }}>
                  {row.started_at ?? "-"}
                </span>
                <button
                  type="button"
                  className="btn btn-xs"
                  data-testid="wsw-history-send-to-repeater"
                  onClick={(e) => {
                    e.stopPropagation();
                    void sendToRepeater(row);
                  }}
                >
                  Send to Repeater
                </button>
              </div>
            );
          }}
        />
      </section>

      {actionMessage && (
        <p className="badge-info" data-testid="wsw-history-action-message">
          {actionMessage}
        </p>
      )}

      {selected && (
        <section className="panel" data-testid="wsw-history-detail">
          <div className="flex items-center justify-between">
            <h2>
              详情 <span className="font-mono text-sm">{selected.exchange_id}</span>
            </h2>
            <div className="flex gap-2">
              <button type="button" className="btn btn-sm" onClick={() => void saveTagsNotes()}>
                编辑 tags/notes
              </button>
              <button
                type="button"
                className="btn btn-sm"
                onClick={() => void sendToRepeater(selected)}
              >
                Send to Repeater
              </button>
            </div>
          </div>
          {detailError && <p className="badge-error">{detailError}</p>}
          {detail && (
            <>
              <dl className="kv">
                <dt>请求</dt>
                <dd className="font-mono">
                  {detail.method} {detail.scheme}://{detail.authority}
                  {detail.path}
                </dd>
                <dt>状态 / 耗时 / TTFB</dt>
                <dd>
                  {detail.response_status ?? "-"} / {detail.duration_ms ?? "-"}ms /{" "}
                  {detail.ttfb_ms ?? "-"}ms
                </dd>
                <dt>捕获保真 / 解析</dt>
                <dd>
                  {detail.capture_fidelity} / {detail.parse_status}
                  {detail.error_code ? `（${detail.error_code}）` : ""}
                </dd>
                <dt>体引用（请求 / 响应）</dt>
                <dd className="font-mono text-xs">
                  req len={detail.request_body_len ?? "-"} fidelity={detail.request_body_fidelity ?? "-"}<br />
                  res len={detail.response_body_len ?? "-"} fidelity={detail.response_body_fidelity ?? "-"}
                </dd>
                <dt>tags</dt>
                <dd>{detail.tags?.join("，") || "（无）"}</dd>
                <dt>notes</dt>
                <dd>{detail.notes || "（无）"}</dd>
              </dl>
              <h3 className="text-sm mt-2">请求报文</h3>
              <MessageViews
                testIdPrefix="wsw-history-req"
                headers={detail.request_headers ?? []}
                params={parseQueryParams(detail.path)}
                rawBody={requestBodyText(requestBody)}
                bodyIsBinary={requestBodyIsBinary(requestBody, detail.request_mime)}
                bodyTruncated={detail.request_body_fidelity === 'truncated'}
              />
              <div className="flex gap-2 mt-2">
                <button
                  type="button"
                  className="btn btn-sm"
                  data-testid="wsw-history-load-request-body"
                  onClick={() => void loadBody("request")}
                >
                  按需加载请求体
                </button>
                <button
                  type="button"
                  className="btn btn-sm"
                  data-testid="wsw-history-load-response-body"
                  onClick={() => void loadBody("response")}
                >
                  按需加载响应体
                </button>
              </div>
              {bodyNote && (
                <p className="muted text-xs" data-testid="wsw-history-body-note">
                  {bodyNote}
                </p>
              )}
              <h3 className="text-sm mt-2">响应报文</h3>
              <MessageViews
                testIdPrefix="wsw-history-res"
                headers={detail.response_headers ?? []}
                rawBody={responseBodyText(responseBody)}
                bodyIsBinary={responseBodyIsBinary(responseBody, detail.response_mime)}
              />
            </>
          )}
        </section>
      )}
    </main>
  );
}

function requestBodyText(body: BodyPayload | null): string {
  if (!body) return "";
  try {
    return new TextDecoder("utf-8", { fatal: false }).decode(base64ToBytes(body.body));
  } catch {
    return "";
  }
}

function responseBodyText(body: BodyPayload | null): string {
  return requestBodyText(body);
}

function requestBodyIsBinary(body: BodyPayload | null, mime: string | null | undefined): boolean {
  if (!body) return false;
  if (mime && mime.startsWith("text/")) return false;
  if (mime && mime.includes("json")) return false;
  return !isProbablyText(base64ToBytes(body.body));
}

function responseBodyIsBinary(body: BodyPayload | null, mime: string | null | undefined): boolean {
  return requestBodyIsBinary(body, mime);
}

function isProbablyText(bytes: Uint8Array): boolean {
  const probe = bytes.subarray(0, Math.min(bytes.length, 1024));
  let suspicious = 0;
  for (const b of probe) {
    if (b === 0 || (b < 0x09 && b !== 0)) suspicious += 1;
  }
  return suspicious === 0;
}

function base64ToBytes(b64: string): Uint8Array {
  const bin = atob(b64);
  const bytes = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i += 1) bytes[i] = bin.charCodeAt(i);
  return bytes;
}

