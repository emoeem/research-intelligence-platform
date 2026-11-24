import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { App } from "../src/App";
import i18n from "../src/i18n";
import { ErrorBoundary } from "../src/components/ErrorBoundary";

const API_STUBS: Record<string, unknown> = {
  "/api/overview": {
    knowledge: { total: 46, by_lifecycle: { VERIFIED: 37, CANDIDATE: 7, DISCOVERED: 2 } },
    tasks: { total: 42 },
    resources: { total: 81 },
    evidence: { total: 700 },
    reviews: { total: 165 },
    tools: { available: true, source: "main", total: 20 },
    runs: { available: true, source: "aux", total: 3 },
    runtime: { available: true, source: "aux", total: 0 },
  },
  "/api/tools": {
    available: true,
    source: "main",
    total: 2,
    items: [
      { tool_id: "nmap", name: "nmap", lifecycle: "REGISTERED" },
      { tool_id: "nuclei", name: "nuclei", lifecycle: "REGISTERED" },
    ],
  },
  "/api/knowledge": {
    available: true,
    source: "main",
    total: 1,
    items: [{ knowledge_id: "k1", title: "站群开放重定向测试法", component: "landing" }],
  },
  "/api/tasks": {
    total: 1,
    items: [{ task_id: "t1", title: "aozoneyun 管理面测试", state: "COMPLETED" }],
  },
  "/api/runs?run_id=run-live-1": {
    run: {
      run_id: "run-live-1",
      tool_id: "nmap",
      status: "FAILED",
      error_category: "timeout",
      error_json: { message: "timed out" },
      created_at: "2026-11-24T10:00:00Z",
    },
    artifacts: [
      {
        artifact_id: "artifact-live-1",
        role: "report",
        uri: "https://example.invalid/report.json",
        sha256: "a".repeat(64),
        mime_type: "application/json",
        size_bytes: 128,
      },
    ],
  },
  "/api/plugins": {
    available: true,
    total: 1,
    items: [
      {
        plugin_id: "plugin-a",
        name: "Plugin Alpha",
        version: "1.2.0",
        kind: "knowledge_source",
        state: "ACTIVE",
        trust_level: "VERIFIED",
        license: "Apache-2.0",
      },
    ],
  },
  "/api/knowledge?knowledge_id=k1": {
    item: {
      knowledge_id: "k1",
      title: "Knowledge Alpha",
      lifecycle: "VERIFIED",
      governance: "APPROVED",
      component: "web",
      severity: "high",
      source: "test source",
      source_url: "https://example.test/source",
    },
  },
  "/api/tools?tool_id=nmap": {
    tool: {
      tool_id: "nmap",
      name: "Nmap scanner",
      kind: "scanner",
      lifecycle: "ENABLED",
      governance: "ACTIVE",
      health_state: "HEALTHY",
      permission_level: "T1",
      timeout_s: 120,
    },
  },
  "/api/tasks?task_id=t1": {
    item: {
      task_id: "t1",
      title: "Test research task",
      state: "EXECUTING",
      target: "example.test",
      task_type: "web",
    },
  },
  "/api/runs": {
    available: true,
    total: 1,
    items: [
      {
        run_id: "run-live-1",
        tool_id: "nmap",
        status: "RUNNING",
        created_at: "2026-11-24T10:00:00Z",
      },
    ],
  },
  "/api/runtime": {
    available: true,
    total: 1,
    items: [
      { node_id: "worker-1", kind: "python_worker", health: "ok", heartbeat_age_seconds: 12 },
    ],
  },
};

function stubFetch(): void {
  vi.stubGlobal(
    "fetch",
    vi.fn(async (input: RequestInfo | URL) => {
      const parsed = new URL(String(input), window.location.origin);
      const rawPath = parsed.pathname + parsed.search;
      const path = parsed.pathname;
      return new Response(JSON.stringify(API_STUBS[rawPath] ?? API_STUBS[path] ?? {}), {
        status: 200,
        headers: { "content-type": "application/json; charset=utf-8" },
      });
    }),
  );
}

function goTo(path: string): void {
  window.history.pushState({}, "", path);
  window.dispatchEvent(new PopStateEvent("popstate"));
}

beforeEach(async () => {
  stubFetch();
  localStorage.clear();
  await i18n.changeLanguage("zh-CN");
  window.history.pushState({}, "", "/");
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
  document.body.innerHTML = "";
  localStorage.clear();
});

describe("Unified AppShell", () => {
  it("renders a single page-level heading and keeps the topbar breadcrumb-only", async () => {
    render(<App />);
    const heading = await screen.findByRole("heading", { level: 1 });
    expect(["早上好", "下午好", "晚上好"]).toContain(heading.textContent);
    expect(screen.getAllByRole("heading", { level: 1 })).toHaveLength(1);
    expect(document.querySelector(".topbar-title")).toBeNull();
    const breadcrumb = screen.getByRole("navigation", { name: "当前位置" });
    expect(breadcrumb.textContent).toContain("工作区");
    expect(breadcrumb.textContent).toContain("总览");
  });

  it("renders all twelve workspaces grouped in the global sidebar", () => {
    render(<App />);
    expect(screen.getByRole("complementary", { name: "全局导航" })).toBeTruthy();
    const nav = screen.getByRole("navigation", { name: "全局导航" });
    for (const route of [
      "/",
      "/projects",
      "/research",
      "/knowledge",
      "/tools",
      "/plugins",
      "/agents",
      "/workflows",
      "/runs",
      "/runtime",
      "/settings",
      "/websec/history",
    ]) {
      expect(nav.querySelector('a[href="' + route + '"]')).toBeTruthy();
    }
    for (const group of ["工作区", "能力", "运行与安全", "系统"]) {
      expect(nav.textContent).toContain(group);
    }
    const workspaceNav = screen.getByRole("navigation", { name: "工作区导航" });
    expect(workspaceNav.textContent).toContain("总览");
    expect(
      within(workspaceNav).getByRole("link", { name: "总览" }).getAttribute("aria-current"),
    ).toBe("page");
    expect(screen.queryByRole("navigation", { name: "已打开的会话" })).toBeNull();
  });

  it("only shows navigation for the current workspace and retains all child routes", async () => {
    render(<App />);
    fireEvent.click(screen.getByRole("link", { name: "Research" }));
    const nav = await screen.findByRole("navigation", { name: "工作区导航" });
    for (const label of [
      "研究总览",
      "任务",
      "资产",
      "知识上下文",
      "验证计划",
      "证据",
      "覆盖矩阵",
    ]) {
      expect(nav.textContent).toContain(label);
    }
    fireEvent.click(screen.getByRole("link", { name: "覆盖矩阵" }));
    await waitFor(() => expect(window.location.pathname).toBe("/research/coverage"));
    expect(screen.getByRole("link", { name: "覆盖矩阵" }).getAttribute("aria-current")).toBe(
      "page",
    );
  });

  it("keeps existing deep links as real session tabs, not pinned workspace tabs", async () => {
    goTo("/tools/nmap");
    render(<App />);
    const tabarea = await screen.findByRole("navigation", { name: "已打开的会话" });
    expect(tabarea.textContent).toContain("nmap");
    expect(tabarea.textContent).not.toContain("Agent Studio");
    expect(screen.getByRole("button", { name: "关闭 nmap" })).toBeTruthy();
  });

  it("closes the active session and falls back to its workspace landing route", async () => {
    goTo("/tools/nmap");
    render(<App />);
    await screen.findByRole("button", { name: "关闭 nmap" });
    fireEvent.click(screen.getByRole("button", { name: "关闭 nmap" }));
    await waitFor(() => expect(window.location.pathname).toBe("/tools"));
    expect(screen.queryByRole("navigation", { name: "已打开的会话" })).toBeNull();
  });

  it("keeps multiple session tabs ordered by recency and falls back to the previous session", async () => {
    goTo("/tools/nmap");
    render(<App />);
    await screen.findByRole("button", { name: "nmap" });
    goTo("/runs/rn-1");
    await screen.findByRole("button", { name: "rn-1" });
    const tabarea = screen.getByRole("navigation", { name: "已打开的会话" });
    const labels = Array.from(tabarea.querySelectorAll(".tabarea-label")).map(
      (node) => node.textContent,
    );
    expect(labels.indexOf("nmap")).toBeLessThan(labels.indexOf("rn-1"));
    fireEvent.click(screen.getByRole("button", { name: "关闭 rn-1" }));
    await waitFor(() => expect(window.location.pathname).toBe("/tools/nmap"));
    expect(screen.getByRole("button", { name: "nmap" }).getAttribute("aria-current")).toBe("page");
  });
});

describe("Panels and global actions", () => {
  it("keeps Inspector and Activity collapsed by default and opens them on demand", () => {
    render(<App />);
    const inspectorBtn = screen.getByRole("button", { name: "Inspector" });
    expect(inspectorBtn.getAttribute("aria-expanded")).toBe("false");
    expect(screen.queryByRole("complementary", { name: "Inspector" })).toBeNull();
    fireEvent.click(inspectorBtn);
    expect(screen.getByRole("complementary", { name: "Inspector" })).toBeTruthy();
    fireEvent.click(inspectorBtn);
    expect(screen.queryByRole("complementary", { name: "Inspector" })).toBeNull();

    const activityBtn = screen.getByRole("button", { name: /Activity 面板/ });
    expect(activityBtn.getAttribute("aria-expanded")).toBe("false");
    expect(screen.queryByRole("region", { name: "Activity 面板" })).toBeNull();
    fireEvent.click(activityBtn);
    expect(screen.getByRole("region", { name: "Activity 面板" })).toBeTruthy();
    for (const label of ["任务队列", "Tool Runs", "日志", "输出", "状态"]) {
      expect(screen.getByRole("region", { name: "Activity 面板" }).textContent).toContain(label);
    }
    fireEvent.click(screen.getByRole("tab", { name: "Tool Runs" }));
    expect(screen.getByRole("tabpanel", { name: "Tool Runs" })).toBeTruthy();
  });

  it("opens Inspector context for a deep-linked entity", async () => {
    goTo("/tools/nmap");
    render(<App />);
    await screen.findByRole("button", { name: "nmap" });
    fireEvent.click(screen.getByRole("button", { name: "Inspector" }));
    const inspector = screen.getByRole("complementary", { name: "Inspector" });
    expect(inspector.textContent).toContain("工具 nmap");
    expect(inspector.textContent).toContain("/tools/nmap");
  });

  it("shows real local-service state and keeps search, language and workspace navigation available", async () => {
    render(<App />);
    expect(screen.getByRole("button", { name: /搜索/ })).toBeTruthy();
    expect(
      screen.getAllByRole("link").filter((link) => link.getAttribute("href") === "/runs").length,
    ).toBeGreaterThanOrEqual(2);
    expect(
      screen.getAllByRole("link").filter((link) => link.getAttribute("href") === "/runtime").length,
    ).toBeGreaterThanOrEqual(2);
    expect(screen.getByRole("link", { name: "Runs" }).getAttribute("href")).toBe("/runs");
    expect(screen.getByRole("link", { name: "Runtime" }).getAttribute("href")).toBe("/runtime");
    expect(screen.getByRole("combobox", { name: "界面语言" })).toBeTruthy();
    await waitFor(() =>
      expect(screen.getByRole("status").textContent).toContain("API 与 Runtime 可用"),
    );
    const notifications = screen.getByRole("button", { name: "通知" });
    expect((notifications as HTMLButtonElement).disabled).toBe(true);
    expect(notifications.getAttribute("title")).toContain("尚未接入");
    expect(screen.getByRole("button", { name: "切换主题" })).toBeTruthy();
  });

  it("keeps the mobile drawer scroll lock, Escape close and route-close behavior", () => {
    render(<App />);
    expect(document.body.style.overflow).toBe("");
    fireEvent.click(screen.getByRole("button", { name: "打开导航菜单" }));
    expect(document.body.style.overflow).toBe("hidden");
    fireEvent.keyDown(window, { key: "Escape" });
    expect(document.body.style.overflow).toBe("");
    fireEvent.click(screen.getByRole("button", { name: "打开导航菜单" }));
    fireEvent.click(screen.getByRole("link", { name: "Tools" }));
    expect(document.body.style.overflow).toBe("");
  });

  it("supports Ctrl+K global search and Escape dismissal", async () => {
    render(<App />);
    fireEvent.keyDown(window, { key: "k", ctrlKey: true });
    const dialog = await screen.findByRole("dialog", { name: "全局搜索" });
    expect(dialog.textContent).toContain("热门入口");
    fireEvent.keyDown(window, { key: "Escape" });
    await waitFor(() => expect(screen.queryByRole("dialog", { name: "全局搜索" })).toBeNull());
  });

  it("switches between Chinese, English and Japanese and persists the choice", async () => {
    render(<App />);
    const language = screen.getByRole("combobox", { name: "界面语言" }) as HTMLSelectElement;
    expect(language.value).toBe("zh-CN");
    fireEvent.change(language, { target: { value: "en-US" } });
    await waitFor(() => expect(document.documentElement.lang).toBe("en-US"));
    expect(screen.getByRole("navigation", { name: "Global navigation" })).toBeTruthy();
    const englishWorkspaceNav = screen.getByRole("navigation", { name: "Workspace navigation" });
    expect(englishWorkspaceNav.textContent).toContain("Overview");
    expect(
      within(englishWorkspaceNav)
        .getByRole("link", { name: "Overview" })
        .getAttribute("aria-current"),
    ).toBe("page");
    expect(localStorage.getItem("rip_language")).toBe("en-US");
    expect(document.title).toBe("Research Intelligence Platform");
    fireEvent.change(screen.getByRole("combobox", { name: "Language" }), {
      target: { value: "ja-JP" },
    });
    await waitFor(() => expect(document.documentElement.lang).toBe("ja-JP"));
    expect(screen.getByRole("navigation", { name: "グローバルナビゲーション" })).toBeTruthy();
    expect(localStorage.getItem("rip_language")).toBe("ja-JP");
    fireEvent.change(screen.getByRole("combobox", { name: "表示言語" }), {
      target: { value: "zh-CN" },
    });
    await waitFor(() => expect(document.documentElement.lang).toBe("zh-CN"));
    expect(document.title).toBe("RIP 研究情报平台");
  });

  it("shows live run and runtime data in the Activity panel", async () => {
    render(<App />);
    fireEvent.click(screen.getByRole("button", { name: "展开 Activity 面板" }));
    fireEvent.click(await screen.findByRole("tab", { name: "Tool Runs" }));
    expect(await screen.findByRole("link", { name: /nmap · run-live-1/ })).toBeTruthy();
    fireEvent.click(screen.getByRole("tab", { name: "状态" }));
    expect(await screen.findByText("worker-1")).toBeTruthy();
    const separator = screen.getByRole("separator", { name: "调整 Activity 高度" });
    const before = Number(separator.getAttribute("aria-valuenow"));
    fireEvent.keyDown(separator, { key: "ArrowUp" });
    expect(Number(separator.getAttribute("aria-valuenow"))).toBe(before - 2);
  });

  it("shows real Run error events and registered artifacts in Activity", async () => {
    goTo("/runs/run-live-1");
    render(<App />);
    fireEvent.click(screen.getByRole("button", { name: "展开 Activity 面板" }));
    fireEvent.click(await screen.findByRole("tab", { name: "日志" }));
    expect(await screen.findByText("timeout")).toBeTruthy();
    expect((await screen.findAllByText(/timed out/)).length).toBeGreaterThan(0);
    fireEvent.click(screen.getByRole("tab", { name: "输出" }));
    expect(await screen.findByText("artifact-live-1")).toBeTruthy();
  });

  it("loads selected Tool and Task data into Inspector context", async () => {
    goTo("/tools/nmap");
    render(<App />);
    fireEvent.click(screen.getByRole("button", { name: "Inspector" }));
    expect((await screen.findAllByText("Nmap scanner")).length).toBeGreaterThan(0);
    expect((await screen.findAllByText("120 s")).length).toBeGreaterThan(0);
    cleanup();
    goTo("/research/tasks/t1");
    render(<App />);
    fireEvent.click(screen.getByRole("button", { name: "Inspector" }));
    expect((await screen.findAllByText("Test research task")).length).toBeGreaterThan(0);
    expect((await screen.findAllByText("example.test")).length).toBeGreaterThan(0);
    cleanup();
    goTo("/plugins/plugin-a");
    render(<App />);
    fireEvent.click(screen.getByRole("button", { name: "Inspector" }));
    expect((await screen.findAllByText("Plugin Alpha")).length).toBeGreaterThan(0);
    expect(screen.getByText("Apache-2.0")).toBeTruthy();
    cleanup();
    goTo("/knowledge?knowledge_id=k1");
    render(<App />);
    fireEvent.click(screen.getByRole("button", { name: "Inspector" }));
    expect((await screen.findAllByText("Knowledge Alpha")).length).toBeGreaterThan(0);
    expect(screen.getByText("APPROVED")).toBeTruthy();
  });

  it("persists collapsed sidebar preference and synchronizes the theme control", () => {
    render(<App />);
    const collapse = screen.getByRole("button", { name: "收起侧边栏" });
    fireEvent.click(collapse);
    expect(localStorage.getItem("rip_sidebar_collapsed")).toBe("true");
    fireEvent.click(screen.getByRole("button", { name: "切换主题" }));
    expect(document.documentElement.getAttribute("data-theme")).toBe("rip-light");
  });
});

describe("ErrorBoundary routing recovery", () => {
  afterEach(() => vi.restoreAllMocks());
  function Bomb(): never {
    throw new Error("boom");
  }

  it("resets only when the route key changes", () => {
    vi.spyOn(console, "error").mockImplementation(() => {});
    const { rerender } = render(
      <ErrorBoundary key="/tools" scope="route">
        <Bomb />
      </ErrorBoundary>,
    );
    expect(screen.getByText("页面出错了")).toBeTruthy();
    rerender(
      <ErrorBoundary key="/tools" scope="route">
        <p>好的页面</p>
      </ErrorBoundary>,
    );
    expect(screen.getByText("页面出错了")).toBeTruthy();
    rerender(
      <ErrorBoundary key="/runs" scope="route">
        <p>好的页面</p>
      </ErrorBoundary>,
    );
    expect(screen.getByText("好的页面")).toBeTruthy();
  });
});
