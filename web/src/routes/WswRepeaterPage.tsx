import { useCallback, useEffect, useState } from "react";
import { useSearchParams } from "react-router";
import { useQueryClient } from "@tanstack/react-query";
import { apiGet, apiSend, ApiError } from "../lib/api";
import { ErrorPanel, PageHeader, StatusBadge } from "../components/ui";
import { MessageViews, type HeaderPair } from "../components/MessageViews";

/** Repeater 页（WSW-3-ADR W3-D8.3；PLAN §5.4 :406-415 布局规格）：顶部 Tab
 * 栏（新建/重命名/关闭/版本历史下拉回退）→ 左 Request 编辑器（Raw/结构化
 * 双模）右 Response 视图 → Send/Cancel（发送中态）。版本历史 append-only
 * （PUT tab request = 新 version 行；UPDATE/DELETE 端点不存在，W3-D1）。
 * Send = rp-wsw send 子进程（Rust-First 发送面；W3-D4）。 */

interface TabRow {
  tab_id: string;
  session_id: string;
  title: string;
  created_at: string;
  updated_at: string;
  latest_version_no?: number | null;
  latest_origin?: string | null;
  version_count?: number;
}

interface TabVersion {
  version_no: number;
  origin: string;
  source_exchange_id?: string | null;
  created_at: string;
  request_json_len?: number;
}

interface TabDetail {
  tab: TabRow;
  versions: TabVersion[];
  latest_request_json: string | null;
  latest_version_no: number | null;
}

interface OutboundDraft {
  method: string;
  target: string;
  version: string;
  headers: HeaderPair[];
  body: string;
  trailers: HeaderPair[];
  framing: string;
}

interface SendRow {
  send_id: string;
  tab_id: string;
  version_no: number;
  exchange_id?: string | null;
  state: string;
  error_code?: string | null;
  response_status?: number | null;
  response_body_len?: number | null;
  response_mime?: string | null;
  duration_ms?: number | null;
  ttfb_ms?: number | null;
  redirect_chain?: Array<{ status: number; location: string; url: string; remote_ip?: string | null }>;
}

const SEND_STATE_TONES: Record<string, string> = {
  pending: "badge-info",
  sending: "badge-info",
  succeeded: "badge-success",
  failed: "badge-error",
  cancelled: "badge-ghost",
  timed_out: "badge-warning",
};

/** OutboundRequest 草稿 → Raw HTTP/1.1 文本预览（展示面；发送面的唯一
 * 事实是 wsw_message::serialize——Display 不发送，PLAN :481-482 四表示）。 */
function draftToRawText(draft: OutboundDraft): string {
  const lines = [`${draft.method} ${draft.target} ${draft.version}`];
  for (const h of draft.headers) lines.push(`${h.name}: ${h.value}`);
  return lines.join("\r\n") + "\r\n\r\n" + decodeDraftBody(draft.body);
}

function decodeDraftBody(b64: string): string {
  if (!b64) return "";
  try {
    const bin = atob(b64);
    const bytes = new Uint8Array(bin.length);
    for (let i = 0; i < bin.length; i += 1) bytes[i] = bin.charCodeAt(i);
    return new TextDecoder("utf-8", { fatal: false }).decode(bytes);
  } catch {
    return "";
  }
}

function encodeDraftBody(text: string): string {
  const bytes = new TextEncoder().encode(text);
  let bin = "";
  for (const b of bytes) bin += String.fromCharCode(b);
  return btoa(bin);
}

/** Raw 请求行版本 token → wsw_message HttpVersion 词表形式（types.rs
 *  from_str_vocabulary 小写词表；raw 文本习惯写法 HTTP/1.1 需归一，否则
 *  OutboundRequest serde 反序列化拒收 → send 卡 pending）。 */
function normalizeVersionToken(token: string): string {
  const t = token.trim().toUpperCase();
  if (t === "HTTP/0.9") return "http/0.9";
  if (t === "HTTP/1.0") return "http/1.0";
  if (t === "HTTP/1.1" || t === "" || t === "HTTP") return "http/1.1";
  if (t === "HTTP/2" || t === "HTTP/2.0") return "h2";
  if (t === "HTTP/3" || t === "HTTP/3.0") return "h3";
  return "http/1.1";
}

function parseRawToDraft(raw: string, fallback: OutboundDraft): OutboundDraft {
  const normalized = raw.replace(/\r\n/g, "\n");
  const splitAt = normalized.indexOf("\n\n");
  const head = splitAt >= 0 ? normalized.slice(0, splitAt) : normalized;
  const bodyText = splitAt >= 0 ? normalized.slice(splitAt + 2) : "";
  const headLines = head.split("\n").filter((l) => l.length > 0);
  const requestLine = headLines[0] ?? "";
  const [method = fallback.method, target = fallback.target, version = fallback.version] =
    requestLine.split(" ");
  const headers: HeaderPair[] = headLines.slice(1).map((line) => {
    const idx = line.indexOf(":");
    if (idx < 0) return { name: line.trim(), value: "" };
    return { name: line.slice(0, idx).trim(), value: line.slice(idx + 1).trim() };
  });
  return {
    method: method.trim().toUpperCase() || "GET",
    target: target.trim() || "/",
    version: normalizeVersionToken(version),
    headers,
    body: encodeDraftBody(bodyText),
    trailers: [],
    framing: "content_length",
  };
}

export function WswRepeaterPage() {
  const [searchParams, setSearchParams] = useSearchParams();
  const queryClient = useQueryClient();
  const [tabs, setTabs] = useState<TabRow[]>([]);
  const [tabsError, setTabsError] = useState<ApiError | null>(null);
  const [activeTab, setActiveTab] = useState<string | null>(null);
  const [detail, setDetail] = useState<TabDetail | null>(null);
  const [draft, setDraft] = useState<OutboundDraft | null>(null);
  const [editorMode, setEditorMode] = useState<"raw" | "structured">("raw");
  const [rawText, setRawText] = useState("");
  const [dirty, setDirty] = useState(false);
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const [send, setSend] = useState<SendRow | null>(null);
  const [sendPolling, setSendPolling] = useState(false);
  const [responseBody, setResponseBody] = useState<string>("");

  const loadTabs = useCallback(async () => {
    try {
      const data = await apiGet<{ tabs: TabRow[] }>("/api/wsw/repeater/tabs");
      setTabs(data.tabs);
      setTabsError(null);
      return data.tabs;
    } catch (error) {
      setTabsError(error instanceof ApiError ? error : new ApiError(0, String(error)));
      return [];
    }
  }, []);

  const loadTab = useCallback(async (tabId: string) => {
    const data = await apiGet<TabDetail>(`/api/wsw/repeater/tabs/${tabId}`);
    setDetail(data);
    setActiveTab(tabId);
    if (data.latest_request_json) {
      const parsed = JSON.parse(data.latest_request_json) as OutboundDraft;
      setDraft(parsed);
      setRawText(draftToRawText(parsed));
    }
    setDirty(false);
  }, []);

  // 初次加载 + ?tab= 直跳（Send to Repeater 联动落点，W3-D6.1）。
  useEffect(() => {
    void (async () => {
      const list = await loadTabs();
      const wanted = searchParams.get("tab");
      if (wanted && list.some((t) => t.tab_id === wanted)) {
        await loadTab(wanted).catch(() => undefined);
      } else if (list.length > 0 && list[0]) {
        await loadTab(list[0].tab_id).catch(() => undefined);
      }
    })();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // 发送轮询（sending/pending → 终态；W3-D4.5 六态词表）。
  useEffect(() => {
    if (!sendPolling || !send) return;
    const timer = window.setInterval(() => {
      void (async () => {
        try {
          const fresh = await apiGet<SendRow>(
            `/api/wsw/repeater/send-status?send_id=${send.send_id}`,
          );
          setSend(fresh);
          if (!["pending", "sending"].includes(fresh.state)) {
            setSendPolling(false);
            if (fresh.exchange_id) {
              // 响应体按需读回（/body 端点；W3-D3 纪律对 repeater 行同样生效）。
              const body = await apiGet<{ body: string }>(
                `/api/wsw/history/${fresh.exchange_id}/body?direction=response`,
              ).catch(() => null);
              if (body) {
                const bin = atob(body.body);
                const bytes = new Uint8Array(bin.length);
                for (let i = 0; i < bin.length; i += 1) bytes[i] = bin.charCodeAt(i);
                setResponseBody(new TextDecoder("utf-8", { fatal: false }).decode(bytes));
              }
            }
            void queryClient.invalidateQueries();
          }
        } catch {
          // 单次轮询失败静默（下一 tick 重试；终态由子命令落行）。
        }
      })();
    }, 500);
    return () => window.clearInterval(timer);
  }, [sendPolling, send, queryClient]);

  async function createTab() {
    setBusy(true);
    setMessage(null);
    try {
      const result = await apiSend<{ tab_id: string }>("POST", "/api/wsw/repeater/tabs", {
        title: `Tab ${new Date().toLocaleTimeString()}`,
      });
      await loadTabs();
      await loadTab(result.tab_id);
      setSearchParams({ tab: result.tab_id });
    } catch (error) {
      setMessage(actionErrorMessage("新建 tab", error));
    } finally {
      setBusy(false);
    }
  }

  async function renameTab() {
    if (!activeTab) return;
    const title = window.prompt("重命名 tab", detail?.tab.title ?? "");
    if (!title) return;
    setBusy(true);
    try {
      // tab 控制行直写面（title 属 tab 行；D7 写权面）。
      await apiSend("PATCH", `/api/wsw/repeater/tabs/${activeTab}`, { title });
      await loadTabs();
      setMessage("已重命名");
    } catch (error) {
      setMessage(actionErrorMessage("重命名", error));
    } finally {
      setBusy(false);
    }
  }

  async function closeTab() {
    if (!activeTab) return;
    if (!window.confirm(`关闭 tab「${detail?.tab.title ?? activeTab}」？（版本历史保留）`)) return;
    setBusy(true);
    try {
      await apiSend("POST", `/api/wsw/repeater/tabs/${activeTab}/close`, {});
      setDetail(null);
      setActiveTab(null);
      setSend(null);
      const list = await loadTabs();
      if (list.length > 0 && list[0]) await loadTab(list[0].tab_id).catch(() => undefined);
    } catch (error) {
      setMessage(actionErrorMessage("关闭 tab", error));
    } finally {
      setBusy(false);
    }
  }

  /** 保存编辑 = append-only 新版本行（W3-D1/D8.3：PUT 落新行；无 UPDATE）。 */
  async function saveDraft() {
    if (!activeTab || !draft) return;
    const outgoing = editorMode === "raw" ? parseRawToDraft(rawText, draft) : draft;
    setBusy(true);
    setMessage(null);
    try {
      const result = await apiSend<{ tab_id: string; version_no: number; append_only: boolean }>(
        "PUT",
        `/api/wsw/repeater/tabs/${activeTab}/request`,
        { request_json: outgoing, origin: "manual" },
      );
      await loadTab(activeTab);
      setMessage(`已保存为新版本 v${result.version_no}（append-only，无 UPDATE/DELETE 路径）`);
    } catch (error) {
      setMessage(actionErrorMessage("保存版本", error));
    } finally {
      setBusy(false);
    }
  }

  async function restoreVersion(versionNo: number) {
    if (!activeTab) return;
    setBusy(true);
    try {
      const result = await apiSend<{ restored_from: number; new_version_no: number }>(
        "POST",
        `/api/wsw/repeater/tabs/${activeTab}/restore`,
        { version_no: versionNo },
      );
      await loadTab(activeTab);
      setMessage(
        `已回退到 v${result.restored_from} → 新版本 v${result.new_version_no}（origin=restore，append-only）`,
      );
    } catch (error) {
      setMessage(actionErrorMessage("版本回退", error));
    } finally {
      setBusy(false);
    }
  }

  async function sendRequest() {
    if (!activeTab) return;
    setBusy(true);
    setMessage(null);
    setResponseBody("");
    try {
      if (dirty || editorMode === "raw") {
        const outgoing = editorMode === "raw" ? parseRawToDraft(rawText, draft ?? emptyDraft()) : draft ?? emptyDraft();
        const saved = await apiSend<{ version_no: number; append_only: boolean }>(
          "PUT",
          `/api/wsw/repeater/tabs/${activeTab}/request`,
          { request_json: outgoing, origin: "manual" },
        );
        setMessage(`编辑已快照 v${saved.version_no}（send_snapshot 前置保存）`);
        await loadTab(activeTab);
      }
      const spawned = await apiSend<{ send_id: string; version_no: number }>(
        "POST",
        "/api/wsw/repeater/send",
        { tab_id: activeTab },
      );
      setSend({
        send_id: spawned.send_id,
        tab_id: activeTab,
        version_no: spawned.version_no,
        state: "sending",
      });
      setSendPolling(true);
    } catch (error) {
      setMessage(actionErrorMessage("发送", error));
    } finally {
      setBusy(false);
    }
  }

  async function cancelSend() {
    if (!send) return;
    setBusy(true);
    try {
      await apiSend("POST", "/api/wsw/repeater/cancel", { send_id: send.send_id });
      setMessage("已发 SIGTERM（graceful → cancelled 终态）");
    } catch (error) {
      setMessage(actionErrorMessage("取消", error));
    } finally {
      setBusy(false);
    }
  }

  return (
    <main className="page" data-testid="wsw-repeater-page">
      <PageHeader
        title="Repeater"
        actions={
          <>
            <button
              type="button"
              className="btn btn-primary"
              disabled={busy}
              data-testid="wsw-repeater-new-tab"
              onClick={() => void createTab()}
            >
              新建 Tab
            </button>
            <button
              type="button"
              className="btn"
              disabled={busy || !activeTab}
              data-testid="wsw-repeater-rename-tab"
              onClick={() => void renameTab()}
            >
              重命名
            </button>
            <button
              type="button"
              className="btn"
              disabled={busy || !activeTab}
              data-testid="wsw-repeater-close-tab"
              onClick={() => void closeTab()}
            >
              关闭
            </button>
          </>
        }
      />
      <p className="muted">
        Tab 修改历史 append-only（每次编辑 = 新版本行；回退 = origin=restore 新行；
        UPDATE/DELETE 端点不存在）。Send 经 rp-wsw 子进程（Outbound 唯一事实，
        编辑头序保持）。
      </p>
      {message && (
        <p className="badge-info" data-testid="wsw-repeater-message">
          {message}
        </p>
      )}
      {tabsError && <ErrorPanel error={tabsError} onRetry={() => void loadTabs()} />}

      {/* Tab 栏 + 版本历史 */}
      <div className="flex gap-2 flex-wrap items-center" data-testid="wsw-repeater-tabbar">
        {tabs.map((tab) => (
          <button
            key={tab.tab_id}
            type="button"
            className={`btn btn-sm ${tab.tab_id === activeTab ? "btn-primary" : ""}`}
            data-testid="wsw-repeater-tab"
            onClick={() => void loadTab(tab.tab_id).catch(() => undefined)}
          >
            {tab.title}{" "}
            <span className="muted text-xs">
              v{tab.latest_version_no ?? "-"}/{tab.version_count ?? 0}
            </span>
          </button>
        ))}
        {tabs.length === 0 && <span className="muted">（无打开 tab——右上角新建）</span>}
        {detail && (
          <select
            className="select select-sm"
            data-testid="wsw-repeater-version-select"
            value=""
            onChange={(e) => {
              const v = Number(e.target.value);
              if (v >= 1) void restoreVersion(v);
            }}
          >
            <option value="">版本历史回退…</option>
            {detail.versions.map((v) => (
              <option key={v.version_no} value={v.version_no}>
                v{v.version_no} · {v.origin}
                {v.source_exchange_id ? ` · ${v.source_exchange_id.slice(0, 20)}…` : ""}
              </option>
            ))}
          </select>
        )}
      </div>

      {detail && draft && (
        <div className="grid grid-cols-2 gap-4 mt-2">
          {/* 左：Request 编辑器（Raw/结构化双模） */}
          <section className="panel" data-testid="wsw-repeater-request">
            <div className="flex items-center gap-2 mb-2">
              <h2 className="text-sm">Request（v{detail.latest_version_no ?? "-"}）</h2>
              <button
                type="button"
                className={`btn btn-xs ${editorMode === "raw" ? "btn-primary" : ""}`}
                data-testid="wsw-repeater-mode-raw"
                onClick={() => {
                  if (editorMode !== "raw") setRawText(draftToRawText(draft));
                  setEditorMode("raw");
                }}
              >
                Raw
              </button>
              <button
                type="button"
                className={`btn btn-xs ${editorMode === "structured" ? "btn-primary" : ""}`}
                data-testid="wsw-repeater-mode-structured"
                onClick={() => {
                  if (editorMode !== "structured" && rawText) {
                    setDraft(parseRawToDraft(rawText, draft));
                  }
                  setEditorMode("structured");
                }}
              >
                结构化
              </button>
              <span className="flex-1" />
              <button
                type="button"
                className="btn btn-sm"
                disabled={busy}
                data-testid="wsw-repeater-save"
                onClick={() => void saveDraft()}
              >
                保存版本
              </button>
            </div>
            {editorMode === "raw" ? (
              <textarea
                className="textarea font-mono text-xs"
                data-testid="wsw-repeater-raw-editor"
                style={{ minHeight: "18rem", whiteSpace: "pre" }}
                value={rawText}
                onChange={(e) => {
                  setRawText(e.target.value);
                  setDirty(true);
                }}
              />
            ) : (
              <div className="flex flex-col gap-2">
                <div className="flex gap-2">
                  <input
                    className="input input-sm font-mono"
                    style={{ width: "7rem" }}
                    value={draft.method}
                    aria-label="method"
                    onChange={(e) => {
                      setDraft({ ...draft, method: e.target.value.toUpperCase() });
                      setDirty(true);
                    }}
                  />
                  <input
                    className="input input-sm font-mono flex-1"
                    value={draft.target}
                    aria-label="target"
                    onChange={(e) => {
                      setDraft({ ...draft, target: e.target.value });
                      setDirty(true);
                    }}
                  />
                </div>
                <table data-testid="wsw-repeater-headers-edit">
                  <thead>
                    <tr>
                      <th>头名</th>
                      <th>值</th>
                    </tr>
                  </thead>
                  <tbody>
                    {draft.headers.map((h, i) => (
                      <tr key={`${h.name}-${i}`}>
                        <td>
                          <input
                            className="input input-sm"
                            value={h.name}
                            onChange={(e) => {
                              const headers = [...draft.headers];
                              headers[i] = { ...headers[i]!, name: e.target.value };
                              setDraft({ ...draft, headers });
                              setDirty(true);
                            }}
                          />
                        </td>
                        <td>
                          <input
                            className="input input-sm"
                            value={h.value}
                            onChange={(e) => {
                              const headers = [...draft.headers];
                              headers[i] = { ...headers[i]!, value: e.target.value };
                              setDraft({ ...draft, headers });
                              setDirty(true);
                            }}
                          />
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
                <textarea
                  className="textarea font-mono text-xs"
                  style={{ minHeight: "6rem" }}
                  placeholder="体（Raw 文本；保存时 base64 内联）"
                  value={decodeDraftBody(draft.body)}
                  onChange={(e) => {
                    setDraft({ ...draft, body: encodeDraftBody(e.target.value) });
                    setDirty(true);
                  }}
                />
              </div>
            )}
          </section>

          {/* 右：Response 视图 + Send/Cancel */}
          <section className="panel" data-testid="wsw-repeater-response">
            <div className="flex items-center gap-2 mb-2">
              <h2 className="text-sm">Response</h2>
              {send && (
                <>
                  <StatusBadge status={send.state} tones={SEND_STATE_TONES} />
                  <span className="muted text-xs">
                    {send.response_status != null ? `HTTP ${send.response_status}` : ""}
                    {send.duration_ms != null ? ` · ${send.duration_ms}ms` : ""}
                  </span>
                </>
              )}
              <span className="flex-1" />
              <button
                type="button"
                className="btn btn-primary btn-sm"
                disabled={busy || sendPolling}
                data-testid="wsw-repeater-send"
                onClick={() => void sendRequest()}
              >
                {sendPolling ? "发送中…" : "Send"}
              </button>
              <button
                type="button"
                className="btn btn-sm"
                disabled={!sendPolling}
                data-testid="wsw-repeater-cancel"
                onClick={() => void cancelSend()}
              >
                Cancel
              </button>
            </div>
            {send?.error_code && (
              <p className="badge-error text-xs" data-testid="wsw-repeater-send-error">
                error_code = {send.error_code}
                {send.state === "failed" && send.error_code === "scope_blocked"
                  ? "（scope 闸拒发：越界目标/未配置规则——PLAN :527 主动面不自动扩大范围）"
                  : ""}
              </p>
            )}
            {send?.redirect_chain && send.redirect_chain.length > 0 && (
              <div data-testid="wsw-repeater-redirect-chain">
                <p className="text-xs muted">重定向链（逐跳 scope 闸 + IP 审计）：</p>
                <ul className="text-xs">
                  {send.redirect_chain.map((hop, i) => (
                    <li key={`${hop.url}-${i}`}>
                      {hop.status} → {hop.url}（remote_ip {hop.remote_ip ?? "-"}）
                    </li>
                  ))}
                </ul>
              </div>
            )}
            {responseBody ? (
              <MessageViews
                testIdPrefix="wsw-repeater-res"
                headers={[]}
                rawBody={responseBody}
              />
            ) : (
              <pre className="muted text-xs">
                {sendPolling
                  ? "（发送中——子进程执行中；终态后响应体按需读回）"
                  : "（无响应——Send 后按需读回 /body）"}
              </pre>
            )}
          </section>
        </div>
      )}
    </main>
  );
}

function emptyDraft(): OutboundDraft {
  return {
    method: "GET",
    target: "/",
    version: "http/1.1",
    headers: [{ name: "Host", value: "127.0.0.1" }],
    body: "",
    trailers: [],
    framing: "content_length",
  };
}

function actionErrorMessage(action: string, error: unknown): string {
  if (error instanceof ApiError) {
    return `${action} 失败（HTTP ${error.status}）：${error.message}`;
  }
  return `${action} 失败：${String(error)}`;
}
