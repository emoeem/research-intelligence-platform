import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { App } from "../src/App";

/** P5-9 Projects 最小版冒烟：列表渲染（/api/tasks 桩：total/by_state/by_type/
 *  行字段）、?task_id= 定位高亮、Loading/Error/Empty 三态。 */

const TASKS_STUB = {
  total: 42,
  by_state: { CREATED: 28, AUTHORIZATION_REVIEW: 7, COMPLETED: 2 },
  by_type: { recon: 19, vuln: 17, full: 6 },
  returned: 2,
  items: [
    {
      task_id: "t-001",
      title: "aozoneyun 管理面变体绕过测试",
      objective: "验证变体边界与维护者口径",
      target: "aozoneyun.example",
      task_type: "vuln",
      state: "COMPLETED",
      updated_at: "2026-11-05T10:00:00Z",
    },
    {
      task_id: "t-002",
      title: "站群开放重定向普查",
      target: "landing.example",
      task_type: "recon",
      state: "CREATED",
      updated_at: "2026-11-04T09:00:00Z",
    },
  ],
};

const EMPTY_TASKS_STUB = {
  total: 0,
  by_state: {},
  by_type: {},
  returned: 0,
  message: "暂无任务：research_task 为空，待 M1 全量数据接入后显示",
  items: [],
};

function stubTasks(body: unknown = TASKS_STUB): void {
  vi.stubGlobal(
    "fetch",
    vi.fn(async (input: RequestInfo | URL) => {
      const path = String(input).split("?")[0] ?? "";
      const respBody = path === "/api/tasks" ? body : {};
      return new Response(JSON.stringify(respBody), {
        status: 200,
        headers: { "content-type": "application/json; charset=utf-8" },
      });
    }),
  );
}

beforeEach(() => {
  stubTasks();
  window.history.pushState({}, "", "/projects");
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
  document.body.innerHTML = "";
});

describe("Projects 页最小版（P5-9）", () => {
  it("渲染列表：total、by_state/by_type 计数 chips 与行字段", async () => {
    render(<App />);
    expect(await screen.findByText("共 42 个任务（显示 2）")).toBeTruthy();
    expect(screen.getByText("CREATED 28")).toBeTruthy();
    expect(screen.getByText("COMPLETED 2")).toBeTruthy();
    expect(screen.getByText("recon 19")).toBeTruthy();
    expect(screen.getByText("aozoneyun 管理面变体绕过测试")).toBeTruthy();
    expect(screen.getByText("COMPLETED")).toBeTruthy();
    expect(screen.getByText("vuln")).toBeTruthy();
    expect(screen.getByText("aozoneyun.example")).toBeTruthy();
  });

  it("?task_id= 定位高亮对应行", async () => {
    window.history.pushState({}, "", "/projects?task_id=t-002");
    render(<App />);
    await screen.findByText("站群开放重定向普查");
    const highlighted = document.querySelector('li[aria-current="true"]');
    expect(highlighted).toBeTruthy();
    expect(highlighted?.textContent).toContain("站群开放重定向普查");
    // 无 query 的行不高亮
    expect(document.querySelectorAll('li[aria-current="true"]').length).toBe(1);
  });

  it("Loading 态渲染骨架", () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(() => new Promise<Response>(() => {})),
    );
    render(<App />);
    expect(document.querySelector(".skeleton")).toBeTruthy();
  });

  it("Error 态渲染 ErrorPanel 且可重试", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => new Response(JSON.stringify({ message: "内部错误" }), { status: 500 })),
    );
    render(<App />);
    expect(await screen.findByText("加载失败")).toBeTruthy();
    expect(screen.getByRole("button", { name: /重试/ })).toBeTruthy();
  });

  it("Empty 态显服务端真实文案", async () => {
    stubTasks(EMPTY_TASKS_STUB);
    render(<App />);
    expect(
      await screen.findByText("暂无任务：research_task 为空，待 M1 全量数据接入后显示"),
    ).toBeTruthy();
  });
});
