import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { App } from "../src/App";
import { ErrorBoundary } from "../src/components/ErrorBoundary";

/** P7-1 壳层冒烟（五区版）：顶部栏五要素（工作区切换 / 全局搜索 / 运行任务 /
 *  Runner 状态 / 通知）、图标轨十一工作区、中央多标签工作区（固定 tab 稳定编号 /
 *  深链接会话 tab / 关闭与回退）、Inspector 与 Activity 折叠、移动抽屉（滚动锁 +
 *  Esc + 路由变化收起）、⌘K 面板分组命中（真实端点桩）、ErrorBoundary 按路由
 *  re-key。fetch 打桩返回与 rp_server 同构的真实形状数据（jsdom 无服务）。 */

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
  window.dispatchEvent(new PopStateEvent("popstate"));
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

describe("顶部栏五要素（P7-1）", () => {
  it("工作区切换器列出十一工作区，切换后路由跳到该区着陆页", () => {
    render(<App />);
    const select = screen.getByRole("combobox", { name: "切换工作区" }) as HTMLSelectElement;
    expect(select.options).toHaveLength(12); // 十一工作区 + 隐藏占位 option
    expect(select.value).toBe("dashboard");
    fireEvent.change(select, { target: { value: "tools" } });
    expect(window.location.pathname).toBe("/tools");
  });

  it("搜索 / 运行任务 / Runner / 通知四要素在场，通知气泡可开合", () => {
    render(<App />);
    // 搜索按钮的可访问名含快捷键徽标（「搜索 ⌘K」），用正则匹配。
    expect(screen.getByRole("button", { name: /搜索/ })).toBeTruthy();
    expect(screen.getByRole("link", { name: "运行任务" }).getAttribute("href")).toBe("/runs");
    // Runner 链接的可访问名来自内容（title 仅是 hover 提示）。
    expect(screen.getByRole("link", { name: "Runner" }).getAttribute("href")).toBe("/runtime");
    const notifBtn = screen.getByRole("button", { name: "通知" });
    expect(screen.queryByText("暂无通知")).toBeNull();
    fireEvent.click(notifBtn);
    expect(screen.getByText("暂无通知")).toBeTruthy();
    expect(notifBtn.getAttribute("aria-expanded")).toBe("true");
    fireEvent.keyDown(window, { key: "Escape" });
    expect(screen.queryByText("暂无通知")).toBeNull();
  });

  it("Inspector 与 Activity 的顶部栏开关翻转 aria-expanded 并显隐面板", () => {
    render(<App />);
    const inspectorBtn = screen.getByRole("button", { name: "Inspector" });
    expect(inspectorBtn.getAttribute("aria-expanded")).toBe("true");
    expect(screen.getByRole("complementary", { name: "Inspector" })).toBeTruthy();
    fireEvent.click(inspectorBtn);
    expect(screen.queryByRole("complementary", { name: "Inspector" })).toBeNull();
    fireEvent.click(inspectorBtn);
    expect(screen.getByRole("complementary", { name: "Inspector" })).toBeTruthy();

    const activityBtn = screen.getByRole("button", { name: "Activity 面板" });
    expect(screen.getByRole("region", { name: "Activity 面板" })).toBeTruthy();
    fireEvent.click(activityBtn);
    expect(screen.queryByRole("region", { name: "Activity 面板" })).toBeNull();
    fireEvent.click(activityBtn);
    expect(screen.getByRole("region", { name: "Activity 面板" })).toBeTruthy();
  });
});

describe("图标轨与中央多标签工作区（P7-1）", () => {
  it("图标轨渲染品牌 + 十一个工作区入口", () => {
    render(<App />);
    expect(screen.getByRole("complementary", { name: "工作区图标轨" })).toBeTruthy();
    expect(screen.getByRole("link", { name: "回到总览" })).toBeTruthy();
    for (const label of [
      "Dashboard",
      "Projects",
      "Research",
      "Knowledge",
      "Tools",
      "Plugins",
      "Agents",
      "Workflows",
      "Runs",
      "Runtime",
      "Settings",
    ]) {
      expect(screen.getByRole("link", { name: label })).toBeTruthy();
    }
  });

  it("标签区渲染十一个固定 tab 且带稳定编号，总览为活跃 tab", () => {
    render(<App />);
    const tabarea = screen.getByRole("navigation", { name: "工作区标签页" });
    expect(tabarea.textContent).toContain("1.1");
    expect(tabarea.textContent).toContain("5.1");
    expect(tabarea.textContent).toContain("11.1");
    expect(tabarea.textContent).toContain("总览");
    expect(tabarea.textContent).toContain("Agent Studio");
    expect(screen.getByRole("button", { name: /总览/ }).getAttribute("aria-current")).toBe("page");
    // 固定 tab 无关闭钮
    expect(screen.queryByRole("button", { name: "关闭标签 总览" })).toBeNull();
  });

  it("深链接直达对象并登记会话 tab：/tools/nmap 打开「nmap」标签且 Inspector 标注对象", async () => {
    goTo("/tools/nmap");
    render(<App />);
    const tabarea = await screen.findByRole("navigation", { name: "工作区标签页" });
    await waitFor(() => {
      if (!screen.getByRole("button", { name: "nmap" })) throw new Error("会话 tab 未登记");
    });
    expect(screen.getByRole("button", { name: "nmap" }).getAttribute("aria-current")).toBe("page");
    const inspector = screen.getByRole("complementary", { name: "Inspector" });
    expect(inspector.textContent).toContain("工具 nmap");
    expect(inspector.textContent).toContain("/tools/nmap");
    expect(tabarea.textContent).toContain("nmap");
  });

  it("关闭活跃会话 tab 回退到所属工作区着陆页", async () => {
    goTo("/tools/nmap");
    render(<App />);
    await screen.findByRole("button", { name: "关闭标签 nmap" });
    fireEvent.click(screen.getByRole("button", { name: "关闭标签 nmap" }));
    await waitFor(() => {
      if (window.location.pathname !== "/tools")
        throw new Error(`未回退: ${window.location.pathname}`);
    });
    expect(screen.queryByRole("button", { name: "关闭标签 nmap" })).toBeNull();
    // 固定 tab 的可访问名含编号前缀（「5.1 工具中心」），用正则匹配。
    expect(screen.getByRole("button", { name: /工具中心/ }).getAttribute("aria-current")).toBe(
      "page",
    );
  });

  it("多个会话 tab 按最近访问排序，关闭当前 tab 回退到上一个会话 tab", async () => {
    goTo("/tools/nmap");
    render(<App />);
    await screen.findByRole("button", { name: "nmap" });
    goTo("/runs/rn-1");
    await waitFor(() => {
      if (!screen.getByRole("button", { name: "rn-1" })) throw new Error("第二个会话 tab 未登记");
    });
    const tabarea = screen.getByRole("navigation", { name: "工作区标签页" });
    const labels = Array.from(tabarea.querySelectorAll(".tabarea-label")).map(
      (node) => node.textContent,
    );
    expect(labels.indexOf("nmap")).toBeLessThan(labels.indexOf("rn-1"));
    // 关闭活跃的 rn-1：回退到仍开着的 nmap 会话 tab。
    fireEvent.click(screen.getByRole("button", { name: "关闭标签 rn-1" }));
    await waitFor(() => {
      if (window.location.pathname !== "/tools/nmap") {
        throw new Error(`未回退到 nmap: ${window.location.pathname}`);
      }
    });
    expect(screen.getByRole("button", { name: "nmap" }).getAttribute("aria-current")).toBe("page");
  });

  it("Inspector 分区三段在场（当前对象 / 关联 / 运行配置），Activity 五页签可切换", () => {
    render(<App />);
    const inspector = screen.getByRole("complementary", { name: "Inspector" });
    expect(inspector.textContent).toContain("当前对象");
    expect(inspector.textContent).toContain("关联知识 / 工具");
    expect(inspector.textContent).toContain("运行配置");
    const activity = screen.getByRole("region", { name: "Activity 面板" });
    for (const tabLabel of ["任务队列", "Tool Runs", "日志", "输出", "状态"]) {
      expect(activity.textContent).toContain(tabLabel);
    }
    fireEvent.click(screen.getByRole("tab", { name: "Tool Runs" }));
    expect(screen.getByRole("tabpanel", { name: "Tool Runs" })).toBeTruthy();
  });

  it("占位工作区（Agents）深链接直达占位页", async () => {
    goTo("/agents");
    render(<App />);
    // 顶栏 h1 在懒加载 chunk 到位前就渲染 Agent Studio，等占位页正文出现再断言。
    expect((await screen.findByTestId("fills-in")).textContent).toBe("P7-7");
    expect(screen.getByRole("button", { name: /Agent Studio/ }).getAttribute("aria-current")).toBe(
      "page",
    );
  });
});

describe("移动抽屉与搜索面板（P5-4 既有行为保持）", () => {
  it("移动抽屉：打开锁滚动，Esc 关闭并解锁", () => {
    render(<App />);
    expect(document.body.style.overflow).toBe("");
    fireEvent.click(screen.getByRole("button", { name: "打开菜单" }));
    expect(document.body.style.overflow).toBe("hidden");
    fireEvent.keyDown(window, { key: "Escape" });
    expect(document.body.style.overflow).toBe("");
  });

  it("路由变化自动收起抽屉", () => {
    render(<App />);
    fireEvent.click(screen.getByRole("button", { name: "打开菜单" }));
    expect(document.body.style.overflow).toBe("hidden");
    // 点击 rail 里的工作区链接（同工作区着陆页）触发路由变化。
    fireEvent.click(screen.getByRole("link", { name: "Tools" }));
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
