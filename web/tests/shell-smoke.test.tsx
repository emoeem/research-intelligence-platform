import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { App } from "../src/App";
import { ErrorBoundary } from "../src/components/ErrorBoundary";

/** P5-4 壳层冒烟：图标轨与页头、工作区 tab 条、移动抽屉（滚动锁 + Esc +
 *  路由变化收起）、⌘K 面板分组命中（真实端点桩）、ErrorBoundary 按路由 re-key。
 *  fetch 打桩返回与 rp_server 同构的真实形状数据（jsdom 无服务）。 */

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
};

function stubFetch(): void {
  vi.stubGlobal(
    "fetch",
    vi.fn(async (input: RequestInfo | URL) => {
      const path = String(input).split("?")[0] ?? "";
      const body = API_STUBS[path] ?? {};
      return new Response(JSON.stringify(body), {
        status: 200,
        headers: { "content-type": "application/json; charset=utf-8" },
      });
    }),
  );
}

function goTo(path: string): void {
  window.history.pushState({}, "", path);
}

beforeEach(() => {
  stubFetch();
  // jsdom 的 window.history 在用例间残留，统一回到总览再开始。
  window.history.pushState({}, "", "/");
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
  document.body.innerHTML = "";
});

describe("壳层冒烟（P5-4）", () => {
  it("渲染图标轨（品牌 + 两个工作区）与页头", async () => {
    render(<App />);
    expect(screen.getByRole("complementary", { name: "工作区图标轨" })).toBeTruthy();
    expect(screen.getByRole("link", { name: "回到总览" })).toBeTruthy();
    expect(screen.getByRole("link", { name: "研究" })).toBeTruthy();
    expect(screen.getByRole("link", { name: "执行" })).toBeTruthy();
    expect(screen.getByRole("heading", { name: "总览" })).toBeTruthy();
    // 总览页接 /api/overview 真实数据（桩对账）：KPI 数字出现。
    await waitFor(() => {
      if (!screen.getByText("46")) throw new Error("KPI 未渲染");
    });
  });

  it("进入工作区后出现 tab 条，tab 带稳定编号", async () => {
    goTo("/tools");
    render(<App />);
    const tabs = await screen.findByRole("navigation", { name: "执行工作区" });
    expect(tabs.textContent).toContain("工具中心");
    expect(tabs.textContent).toContain("运行记录");
    expect(tabs.textContent).toContain("运行时");
    expect(tabs.textContent).toContain("2.1");
    expect(tabs.textContent).toContain("2.3");
    // 总览不属于任何工作区：页头仍是总览，无执行 tab 条。
    expect(screen.getByRole("heading", { name: "工具中心" })).toBeTruthy();
  });

  it("总览路由不显示任何工作区 tab 条", () => {
    render(<App />);
    expect(screen.queryByRole("navigation", { name: "研究工作区" })).toBeNull();
    expect(screen.queryByRole("navigation", { name: "执行工作区" })).toBeNull();
  });

  it("移动抽屉：打开锁滚动，Esc 关闭并解锁", () => {
    render(<App />);
    expect(document.body.style.overflow).toBe("");
    fireEvent.click(screen.getByRole("button", { name: "打开菜单" }));
    expect(document.body.style.overflow).toBe("hidden");
    fireEvent.keyDown(window, { key: "Escape" });
    expect(document.body.style.overflow).toBe("");
  });

  it("路由变化自动收起抽屉", () => {
    goTo("/tools");
    render(<App />);
    fireEvent.click(screen.getByRole("button", { name: "打开菜单" }));
    expect(document.body.style.overflow).toBe("hidden");
    // 点击 rail 里的工作区链接（同工作区着陆页）触发路由变化。
    fireEvent.click(screen.getByRole("link", { name: "研究" }));
    expect(document.body.style.overflow).toBe("");
  });

  it("⌘K 打开面板：空态热门入口，输入后命中真实端点桩的工具分组", async () => {
    render(<App />);
    fireEvent.keyDown(window, { key: "k", ctrlKey: true });
    const dialog = await screen.findByRole("dialog", { name: "全局搜索" });
    expect(screen.getAllByText("热门入口").length).toBeGreaterThan(0);
    fireEvent.change(screen.getByRole("textbox", { name: "搜索关键词" }), {
      target: { value: "nmap" },
    });
    await waitFor(() => {
      const groupLabels = Array.from(document.querySelectorAll(".search-group-label")).map(
        (node) => node.textContent,
      );
      if (!groupLabels.includes("工具")) throw new Error("工具分组未出现");
    });
    expect(dialog.textContent).toContain("nmap");
  });

  it("Esc 关闭搜索面板", async () => {
    render(<App />);
    fireEvent.keyDown(window, { key: "k", ctrlKey: true });
    await screen.findByRole("dialog", { name: "全局搜索" });
    fireEvent.keyDown(window, { key: "Escape" });
    await waitFor(() => {
      if (screen.queryByRole("dialog", { name: "全局搜索" }) !== null) {
        throw new Error("面板未关闭");
      }
    });
  });
});

describe("ErrorBoundary 按路由 re-key（P5-4）", () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  function Bomb(): never {
    throw new Error("boom");
  }

  it("key 变化复位错误态（路由切换后页面恢复渲染）", () => {
    vi.spyOn(console, "error").mockImplementation(() => {});
    const { rerender } = render(
      <ErrorBoundary key="/tools" scope="route">
        <Bomb />
      </ErrorBoundary>,
    );
    expect(screen.getByText("页面出错了")).toBeTruthy();
    // 同 key 换好子树：错误保持（不自动吞错）。
    rerender(
      <ErrorBoundary key="/tools" scope="route">
        <p>好的页面</p>
      </ErrorBoundary>,
    );
    expect(screen.getByText("页面出错了")).toBeTruthy();
    // 换 key（模拟路由切换）：错误复位，新子树正常渲染。
    rerender(
      <ErrorBoundary key="/runs" scope="route">
        <p>好的页面</p>
      </ErrorBoundary>,
    );
    expect(screen.getByText("好的页面")).toBeTruthy();
    expect(screen.queryByText("页面出错了")).toBeNull();
  });
});
