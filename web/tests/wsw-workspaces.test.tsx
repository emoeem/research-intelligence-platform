import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { App } from "../src/App";
import i18n from "../src/i18n";

// The History empty-state test exercises page/API behavior, not the panel library's
// ResizeObserver/geometry behavior. The real splitter remains covered in Playwright.
vi.mock("../src/components/ResizableSplitView", () => ({
  ResizableSplitView: ({ first, second }: { first: React.ReactNode; second: React.ReactNode }) => (
    <div data-testid="mock-resizable-split">
      <section>{first}</section>
      <section>{second}</section>
    </div>
  ),
}));

const requests: Array<{ path: string; method: string; body?: string }> = [];
const apiPayloads: Record<string, unknown> = {
  "/api/wsw/passive/rules": {
    rules: [
      {
        rule_id: "missing_hsts",
        rule_version: 1,
        severity: "medium",
        class: "missing_security_header",
        description: "Missing Strict-Transport-Security",
      },
    ],
    classes: ["missing_security_header"],
    severities: ["info", "low", "medium", "high"],
    count: 1,
    note: "read-only projection",
  },
  "/api/wsw/passive/hits?limit=100": {
    rows: [],
    next_cursor: "",
    has_more: false,
    total: 0,
    limit: 100,
    fingerprint: "test",
  },
  "/api/wsw/ws/frames?limit=100": {
    rows: [],
    next_cursor: "",
    has_more: false,
    total: 0,
    limit: 100,
    fingerprint: "test",
  },
  "/api/wsw/fuzz/jobs?limit=100": { jobs: [], limit: 100 },
};

function stubFetch() {
  requests.length = 0;
  vi.stubGlobal(
    "fetch",
    vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const url = new URL(String(input), window.location.origin);
      const path = url.pathname + url.search;
      const method = (init?.method ?? "GET").toUpperCase();
      const body = typeof init?.body === "string" ? init.body : undefined;
      requests.push({ path, method, body });
      const payload =
        method === "POST" && url.pathname === "/api/wsw/decode"
          ? { operation: "b64_decode", result: "hello", success: true }
          : method === "POST" && url.pathname === "/api/wsw/compare"
            ? { mode: "text", equal: false, hunks: [] }
            : method === "POST" && url.pathname === "/api/wsw/fuzz/jobs"
              ? { job_id: "wsw-fzj-created-1", status_url: "/api/wsw/fuzz/jobs/wsw-fzj-created-1" }
              : method === "POST" && url.pathname.endsWith("/cancel")
                ? { cancelled: true, note: "cancelled in test" }
                : method === "GET" && url.pathname === "/api/wsw/history"
                  ? {
                      rows: [],
                      next_cursor: "",
                      has_more: false,
                      total: 0,
                      limit: 200,
                      fingerprint: "empty",
                    }
                  : method === "GET" && /^\/api\/wsw\/fuzz\/jobs\/[^/]+$/.test(url.pathname)
                    ? {
                        job_id: "wsw-fzj-active-1",
                        session_id: "wsw-ses-cli-0001",
                        state: "RUNNING",
                        estimated_requests: 2,
                        max_requests: 10,
                        max_concurrency: 1,
                        duration_ms: null,
                        created_at: "2026-11-24T00:00:00Z",
                        cases: [],
                      }
                    : (apiPayloads[path] ?? {});
      return new Response(JSON.stringify(payload), {
        status: 200,
        headers: { "content-type": "application/json; charset=utf-8" },
      });
    }),
  );
}

function goTo(path: string) {
  window.history.pushState({}, "", path);
  window.dispatchEvent(new PopStateEvent("popstate"));
}

beforeEach(async () => {
  apiPayloads["/api/wsw/fuzz/jobs?limit=100"] = { jobs: [], limit: 100 };
  stubFetch();
  localStorage.clear();
  await i18n.changeLanguage("zh-CN");
  window.history.pushState({}, "", "/");
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
  localStorage.clear();
});

describe("WSW read-only and local tool workspaces", () => {
  it("shows a useful HTTP History empty state without a blank list pane", async () => {
    goTo("/websec/history");
    render(<App />);
    expect(
      await screen.findByText(
        "暂无 HTTP 流量。启动 Proxy/MITM 捕获请求，或从 Repeater 发送请求后会显示在这里。",
      ),
    ).toBeTruthy();
    expect(requests.some((r) => r.path.startsWith("/api/wsw/history?") && r.method === "GET")).toBe(
      true,
    );
    expect(screen.getByTestId("wsw-history-empty-state")).toBeTruthy();
    expect(screen.queryByText("选择一条请求以查看完整详情。")).toBeNull();
    expect(screen.queryByTestId("mock-resizable-split")).toBeNull();
  });

  it("shows the passive rule registry and read-only findings state", async () => {
    goTo("/websec/passive");
    render(<App />);
    expect((await screen.findAllByText("missing_hsts")).length).toBeGreaterThan(0);
    expect(await screen.findByText(/当前筛选下没有审计命中/)).toBeTruthy();
    expect(
      requests.some((r) => r.path.startsWith("/api/wsw/passive/hits") && r.method === "GET"),
    ).toBe(true);
    expect(requests.some((r) => r.method === "POST")).toBe(false);
  });

  it("loads WebSocket frame metadata without fetching Payload eagerly", async () => {
    goTo("/websec/websocket");
    render(<App />);
    expect(await screen.findByText(/尚无 WebSocket 帧记录/)).toBeTruthy();
    expect(requests.some((r) => r.path === "/api/wsw/ws/frames?limit=100")).toBe(true);
    expect(requests.some((r) => r.path.includes("payload=1"))).toBe(false);
  });

  it("requires scope confirmation before creating a Fuzzer job and sends the backend FuzzSpec contract", async () => {
    goTo("/websec/fuzzer");
    render(<App />);
    expect(await screen.findByText(/尚无 Fuzzer Job/)).toBeTruthy();
    expect(requests.some((r) => r.path === "/api/wsw/fuzz/jobs?limit=100")).toBe(true);
    const submit = screen.getByRole("button", { name: "确认并创建 Fuzzer Job" });
    expect((submit as HTMLButtonElement).disabled).toBe(true);
    expect(requests.some((r) => r.method === "POST")).toBe(false);

    fireEvent.change(screen.getByLabelText("目标请求路径或绝对 URL"), {
      target: { value: "https://authorized.example.test/search?q=§value§" },
    });
    fireEvent.change(screen.getByLabelText("Payload 列表（每行一个值，所有标记位置共用）"), {
      target: { value: "alpha\nbeta" },
    });
    fireEvent.click(screen.getByLabelText(/我确认目标属于我有权测试的 Scope/));
    expect((submit as HTMLButtonElement).disabled).toBe(false);
    fireEvent.click(submit);

    expect(await screen.findByText(/已提交 Fuzzer Job.*wsw-fzj-created-1/)).toBeTruthy();
    const post = requests.find((r) => r.path === "/api/wsw/fuzz/jobs" && r.method === "POST");
    expect(post).toBeTruthy();
    const spec = JSON.parse(post!.body ?? "{}").spec;
    expect(spec).toMatchObject({
      request: {
        method: "GET",
        target: "https://authorized.example.test/search?q=§value§",
        version: "http/1.1",
        framing: "content_length",
        headers: [
          { name: "Host", value: "authorized.example.test" },
          { name: "User-Agent", value: "RIP Fuzzer" },
        ],
      },
      positions: [],
      payloads: [{ type: "list", values: ["alpha", "beta"] }],
      mode: "single",
      budget: {
        max_requests: 25,
        max_concurrency: 1,
        rate_window_ms: 1000,
        rate_max_requests: 1,
        min_request_interval_ms: 1000,
        max_duration_ms: 60000,
      },
    });
    expect(requests.filter((r) => r.method === "POST")).toHaveLength(1);
  });

  it("cancels an active Fuzzer job only after an explicit confirmation", async () => {
    apiPayloads["/api/wsw/fuzz/jobs?limit=100"] = {
      jobs: [
        {
          job_id: "wsw-fzj-active-1",
          session_id: "wsw-ses-cli-0001",
          state: "RUNNING",
          estimated_requests: 2,
          max_requests: 10,
          max_concurrency: 1,
          created_at: "2026-11-24T00:00:00Z",
        },
      ],
      limit: 100,
    };
    vi.spyOn(window, "confirm").mockReturnValue(true);
    goTo("/websec/fuzzer");
    render(<App />);
    expect(await screen.findByText("wsw-fzj-active-1")).toBeTruthy();
    fireEvent.click(screen.getByText("wsw-fzj-active-1"));
    expect(await screen.findByText("RUNNING")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "取消任务" }));
    expect(await screen.findByText("cancelled in test")).toBeTruthy();
    expect(
      requests.some(
        (r) => r.path === "/api/wsw/fuzz/jobs/wsw-fzj-active-1/cancel" && r.method === "POST",
      ),
    ).toBe(true);
  });

  it("posts a local decoder transformation with the selected operation", async () => {
    goTo("/websec/decoder");
    render(<App />);
    const operation = await screen.findByRole("combobox", { name: "转换操作" });
    fireEvent.change(operation, {
      target: { value: "b64_decode" },
    });
    fireEvent.change(await screen.findByPlaceholderText("粘贴需要转换的内容…"), {
      target: { value: "aGVsbG8=" },
    });
    fireEvent.click(screen.getByRole("button", { name: "执行转换" }));
    expect(await screen.findByText(/hello/)).toBeTruthy();
    const post = requests.find((r) => r.path === "/api/wsw/decode" && r.method === "POST");
    expect(post).toBeTruthy();
    expect(JSON.parse(post!.body ?? "{}")).toMatchObject({
      input: "aGVsbG8=",
      ops: ["b64_decode"],
    });
  });

  it("compares user-provided text locally and does not fetch History content", async () => {
    goTo("/websec/comparer");
    render(<App />);
    const leftInput = await screen.findByLabelText("左侧内容");
    const rightInput = await screen.findByLabelText("右侧内容");
    fireEvent.change(leftInput, { target: { value: "alpha" } });
    fireEvent.change(rightInput, { target: { value: "beta" } });
    fireEvent.click(screen.getByRole("button", { name: "比较" }));
    expect(await screen.findByText(/equal/)).toBeTruthy();
    const post = requests.find((r) => r.path === "/api/wsw/compare" && r.method === "POST");
    expect(post).toBeTruthy();
    expect(JSON.parse(post!.body ?? "{}")).toMatchObject({
      mode: "text",
      left: { text: "alpha" },
      right: { text: "beta" },
    });
    expect(requests.some((r) => r.path.startsWith("/api/wsw/history"))).toBe(false);
  });
});
