import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { App } from "../src/App";

/** P5-5 Overview 页冒烟：KPI 渲染（/api/overview + /api/stats 真实形状桩）、
 *  卡片可点击跳列表页、Loading（Skeleton）/Error（ErrorPanel + 重试）/
 *  Empty（EmptyPanel）三态。 */

const OVERVIEW_STUB = {
  knowledge: { total: 46, by_lifecycle: { VERIFIED: 37, CANDIDATE: 7, DISCOVERED: 2 } },
  tasks: { total: 42 },
  resources: { total: 81 },
  evidence: { total: 700 },
  reviews: { total: 165 },
  tools: { available: true, source: "main", total: 20 },
  runs: { available: true, source: "aux", total: 3, by_status: { SUCCEEDED: 3 } },
  runtime: { available: true, source: "aux", total: 0 },
};

const STATS_STUB = {
  reviews: {
    total: 165,
    by_queue: {
      d5: { total: 68, by_decision: {} },
      license: { total: 79, by_decision: {} },
      mapping: { total: 18, by_decision: {} },
    },
  },
};

type FetchMode = "ok" | "pending" | "error";

let fetchMode: FetchMode = "ok";

function stubFetch(): void {
  vi.stubGlobal(
    "fetch",
    vi.fn((input: RequestInfo | URL) => {
      if (fetchMode === "pending") return new Promise(() => {});
      if (fetchMode === "error") {
        return Promise.resolve(
          new Response(JSON.stringify({ error: { code: "internal", message: "内部错误" } }), {
            status: 500,
          }),
        );
      }
      const path = String(input).split("?")[0] ?? "";
      const body =
        path === "/api/stats" ? STATS_STUB : path === "/api/overview" ? OVERVIEW_STUB : {};
      return Promise.resolve(
        new Response(JSON.stringify(body), {
          status: 200,
          headers: { "content-type": "application/json; charset=utf-8" },
        }),
      );
    }),
  );
}

beforeEach(() => {
  fetchMode = "ok";
  stubFetch();
  window.history.pushState({}, "", "/");
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
  document.body.innerHTML = "";
});

describe("Overview 页（P5-5）", () => {
  it("渲染 KPI 卡：数值来自 /api/overview 与 /api/stats 桩", async () => {
    render(<App />);
    expect(await screen.findByText("46")).toBeTruthy(); // 知识条目
    expect(screen.getByText("42")).toBeTruthy(); // 研究任务
    expect(screen.getByText("700")).toBeTruthy(); // 证据
    expect(screen.getByText("165")).toBeTruthy(); // 评审队列（stats.reviews.total）
    expect(screen.getByText("20")).toBeTruthy(); // 工具
    // P7-1 壳层里功能地图工作区卡编号（3=Research）与 KPI 数值同屏：
    // 运行 KPI 断言收窄到运行卡本身（可访问名含数值）。
    expect(screen.getByRole("link", { name: /^运行 3/ })).toBeTruthy(); // 运行
    // 评审队列 note 显示三个队列计数
    expect(screen.getByTitle("D5 68 · License 79 · Mapping 18")).toBeTruthy();
    // Run 状态 note 显示 by_status
    expect(screen.getByTitle("SUCCEEDED 3")).toBeTruthy();
  });

  it("有列表页的卡可点击跳转：知识条目卡 → /knowledge", async () => {
    render(<App />);
    await screen.findByText("46");
    const card = screen.getByRole("link", { name: /知识条目/ });
    fireEvent.click(card);
    await waitFor(() => {
      if (window.location.pathname !== "/knowledge") throw new Error("未跳转");
    });
    expect(screen.getByRole("heading", { name: "知识库" })).toBeTruthy();
  });

  it("工具卡跳 /tools、运行卡跳 /runs", async () => {
    render(<App />);
    await screen.findByText("46");
    // P7-1 壳层新增 Inspector 关联链接与顶栏运行任务链接，链接名正则收窄到
    // KPI 卡的可访问名（含数值），避免误命中壳层元素。
    expect(screen.getByRole("link", { name: "工具 20" }).getAttribute("href")).toBe("/tools");
    expect(screen.getByRole("link", { name: /^运行 3/ }).getAttribute("href")).toBe("/runs");
    // 证据与评审队列无专属页面：静态卡
    expect(screen.queryByRole("link", { name: /证据/ })).toBeNull();
    expect(screen.queryByRole("link", { name: /评审队列/ })).toBeNull();
  });

  it("Loading 态：fetch 挂起时渲染 SkeletonPanel", () => {
    fetchMode = "pending";
    render(<App />);
    expect(document.querySelector(".skeleton")).toBeTruthy();
    expect(screen.queryByText("知识条目")).toBeNull();
  });

  it("Error 态：500 时渲染 ErrorPanel 且可重试", async () => {
    fetchMode = "error";
    render(<App />);
    expect(await screen.findByText("加载失败")).toBeTruthy();
    const retry = screen.getByRole("button", { name: /重试/ });
    expect(retry).toBeTruthy();
    // 重试切回正常桩：数据落地
    fetchMode = "ok";
    fireEvent.click(retry);
    expect(await screen.findByText("46")).toBeTruthy();
  });

  it("Empty 态：数据面全零渲染 EmptyPanel", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(async (input: RequestInfo | URL) => {
        const path = String(input).split("?")[0] ?? "";
        const body =
          path === "/api/stats"
            ? { reviews: { total: 0 } }
            : {
                knowledge: { total: 0 },
                tasks: { total: 0 },
                resources: { total: 0 },
                evidence: { total: 0 },
                reviews: { total: 0 },
                tools: { available: false, reason: "aux_db_not_configured" },
                runs: { available: false, reason: "aux_db_not_configured" },
              };
        return new Response(JSON.stringify(body), { status: 200 });
      }),
    );
    render(<App />);
    expect(
      await screen.findByText("平台数据面暂为空：等待数据接入后这里会显示 KPI 与分布图。"),
    ).toBeTruthy();
    // 功能地图仍可见（静态导航辅助）
    expect(screen.getByText("功能地图")).toBeTruthy();
  });
});
