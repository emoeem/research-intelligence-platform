import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { App } from "../src/App";

/** P5-7 Tool Center 冒烟：目录列表（总数/badge）、lifecycle/governance 过滤
 *  （服务端参数）、过滤无命中空态、详情页（Schema 原始 JSON、governance/
 *  health、tool_version 历史、试运行禁用占位）、Schema 未登记诚实文案、
 *  404 Error、Loading 骨架。 */

const LIST_STUB = {
  available: true,
  source: "main",
  total: 20,
  returned: 2,
  items: [
    {
      tool_id: "nmap",
      name: "nmap",
      kind: "binary",
      lifecycle: "REGISTERED",
      governance: "ACTIVE",
      health_state: null,
      permission_level: "T0",
      updated_at: "2026-11-09T10:00:00Z",
    },
    {
      tool_id: "rip.kb.rtis",
      name: "rip.kb.rtis",
      kind: "cli",
      lifecycle: "ENABLED",
      governance: "ACTIVE",
      health_state: "ok",
      permission_level: "T1",
      updated_at: "2026-11-09T11:00:00Z",
    },
  ],
};

const INPUT_SCHEMA = {
  type: "object",
  properties: { query: { type: "string" }, limit: { type: "number" } },
  required: ["query"],
};

const DETAIL_STUB = {
  available: true,
  source: "main",
  tool: {
    tool_id: "rip.kb.rtis",
    name: "rip.kb.rtis",
    kind: "cli",
    lifecycle: "ENABLED",
    governance: "ACTIVE",
    health_state: "ok",
    install_state: null,
    permission_level: "T1",
    input_schema: JSON.stringify(INPUT_SCHEMA),
    output_schema: null,
    timeout_s: 60,
    binary_ref: "rtis.py",
    source_system: "tool-registry",
    created_at: "2026-11-09T10:00:00Z",
    updated_at: "2026-11-09T11:00:00Z",
    schema_version: 1,
  },
  input_schema: INPUT_SCHEMA,
  output_schema: null,
  versions: [
    {
      tv_id: 2,
      tool_id: "rip.kb.rtis",
      version: "1.1.0",
      status: "current",
      provenance: { source_url: "https://example.org/rtis", note: "契约登记" },
      content_hash: "a".repeat(64),
      validated_at: "2026-11-09T10:30:00Z",
      created_at: "2026-11-09T10:00:00Z",
    },
    {
      tv_id: 1,
      tool_id: "rip.kb.rtis",
      version: "1.0.0",
      status: "superseded",
      provenance: null,
      content_hash: null,
      validated_at: null,
      created_at: "2026-11-08T09:00:00Z",
    },
  ],
};

function stubFetch(): void {
  vi.stubGlobal(
    "fetch",
    vi.fn(async (input: RequestInfo | URL) => {
      const url = String(input);
      let body: unknown = {};
      if (url.includes("tool_id=")) body = DETAIL_STUB;
      else if (url.includes("/api/tools")) body = LIST_STUB;
      return new Response(JSON.stringify(body), {
        status: 200,
        headers: { "content-type": "application/json; charset=utf-8" },
      });
    }),
  );
}

beforeEach(() => {
  stubFetch();
  window.history.pushState({}, "", "/tools");
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
  document.body.innerHTML = "";
});

describe("Tool Center 列表（P5-7）", () => {
  it("渲染目录：总数与条目（badge 含 lifecycle/governance）", async () => {
    render(<App />);
    expect(await screen.findByText("共 20 个工具（显示 2，数据源：主库注册面）")).toBeTruthy();
    expect(screen.getByText("nmap")).toBeTruthy();
    expect(screen.getByText("rip.kb.rtis")).toBeTruthy();
    expect(screen.getAllByText("REGISTERED").length).toBeGreaterThan(0);
    expect(screen.getAllByText("ACTIVE").length).toBeGreaterThan(0);
    // 行点击进详情页
    expect(screen.getByRole("link", { name: /rip\.kb\.rtis/ }).getAttribute("href")).toBe(
      "/tools/rip.kb.rtis",
    );
  });

  it("lifecycle 与 governance 过滤走服务端参数", async () => {
    const fetchMock = vi.fn(async (input: RequestInfo | URL) => {
      const url = String(input);
      const body = url.includes("tool_id=")
        ? DETAIL_STUB
        : url.includes("/api/tools")
          ? LIST_STUB
          : {};
      return new Response(JSON.stringify(body), { status: 200 });
    });
    vi.stubGlobal("fetch", fetchMock);
    render(<App />);
    await screen.findByText("共 20 个工具（显示 2，数据源：主库注册面）");
    fireEvent.change(screen.getByLabelText("按生命周期过滤"), { target: { value: "REGISTERED" } });
    await waitFor(() => {
      const called = fetchMock.mock.calls.some((call) =>
        String(call[0]).includes("lifecycle=REGISTERED"),
      );
      if (!called) throw new Error("lifecycle 过滤请求未发出");
    });
    fireEvent.change(screen.getByLabelText("按治理状态过滤"), { target: { value: "QUARANTINED" } });
    await waitFor(() => {
      const called = fetchMock.mock.calls.some((call) =>
        String(call[0]).includes("governance=QUARANTINED"),
      );
      if (!called) throw new Error("governance 过滤请求未发出");
    });
  });

  it("过滤无命中渲染真实空态", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(async (input: RequestInfo | URL) => {
        const url = String(input);
        const body = url.includes("governance=REJECTED")
          ? { available: true, total: 0, returned: 0, items: [] }
          : url.includes("tool_id=")
            ? DETAIL_STUB
            : LIST_STUB;
        return new Response(JSON.stringify(body), { status: 200 });
      }),
    );
    render(<App />);
    await screen.findByText("共 20 个工具（显示 2，数据源：主库注册面）");
    fireEvent.change(screen.getByLabelText("按治理状态过滤"), { target: { value: "REJECTED" } });
    expect(
      await screen.findByText("当前过滤条件下没有工具，调整生命周期或治理状态后重试。"),
    ).toBeTruthy();
  });

  it("Loading 态渲染骨架", () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(() => new Promise<Response>(() => {})),
    );
    render(<App />);
    expect(document.querySelector(".skeleton")).toBeTruthy();
  });
});

describe("Tool 详情页（P5-7）", () => {
  function renderDetail(toolId: string): void {
    window.history.pushState({}, "", `/tools/${encodeURIComponent(toolId)}`);
    render(<App />);
  }

  it("渲染 Schema 原始 JSON、governance/health 与试运行禁用占位", async () => {
    renderDetail("rip.kb.rtis");
    // 名称出现在页头 h1 与元数据行两处
    expect((await screen.findAllByText("rip.kb.rtis")).length).toBeGreaterThan(0);
    // Schema 原始 JSON：后端返回的结构按 JSON.stringify 原样渲染
    const pre = await screen.findByLabelText("Input Schema 原始 JSON");
    expect(pre.textContent).toContain('"type": "object"');
    expect(pre.textContent).toContain('"properties"');
    expect(pre.textContent).toContain('"query"');
    // output_schema 为 null：诚实说明而非编造
    expect(screen.getByText(/该工具未登记 Schema/)).toBeTruthy();
    // governance/health
    expect(screen.getAllByText("ACTIVE").length).toBeGreaterThan(0);
    expect(screen.getAllByText("ENABLED").length).toBeGreaterThan(0);
    expect(screen.getByText("ok")).toBeTruthy();
    // tool_version 历史
    expect(screen.getByText("v1.1.0")).toBeTruthy();
    expect(screen.getByText("v1.0.0")).toBeTruthy();
    expect(screen.getAllByText("current").length).toBeGreaterThan(0);
    // provenance 原始 JSON 与 Schema 未登记说明都含「契约登记」字样，取「存在即可」
    expect(screen.getAllByText(/契约登记/).length).toBeGreaterThan(0);
    // 试运行禁用占位 + 依赖说明
    const trial = screen.getByRole("button", { name: "试运行" });
    expect((trial as HTMLButtonElement).disabled).toBe(true);
    expect(screen.getByText(/依赖 Phase 1\/3 派发链（本阶段只读 API 无执行面）$/)).toBeTruthy();
  });

  it("未登记 Schema 的盘点工具显示诚实文案", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(async (input: RequestInfo | URL) => {
        const url = String(input);
        const detail = {
          ...DETAIL_STUB,
          input_schema: null,
          output_schema: null,
          tool: {
            ...DETAIL_STUB.tool,
            tool_id: "nmap",
            name: "nmap",
            lifecycle: "REGISTERED",
            health_state: null,
          },
        };
        const body = url.includes("tool_id=") ? detail : LIST_STUB;
        return new Response(JSON.stringify(body), { status: 200 });
      }),
    );
    renderDetail("nmap");
    expect((await screen.findAllByText("nmap")).length).toBeGreaterThan(0);
    const notes = await screen.findAllByText(/该工具未登记 Schema/);
    expect(notes.length).toBe(2); // input 与 output 两处
    // health 为 null 不渲染（不造假显示）
    expect(screen.queryByText(/health:/)).toBeNull();
  });

  it("404 渲染 ErrorPanel（数据不存在）", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(
        async () =>
          new Response(
            JSON.stringify({ error: { code: "no_tool", message: "工具不存在：ghost" } }),
            {
              status: 404,
            },
          ),
      ),
    );
    renderDetail("ghost");
    expect(await screen.findByText("数据不存在")).toBeTruthy();
    expect(screen.getByRole("button", { name: /重试/ })).toBeTruthy();
  });

  it("Loading 态渲染骨架", () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(() => new Promise<Response>(() => {})),
    );
    renderDetail("nmap");
    expect(document.querySelector(".skeleton")).toBeTruthy();
  });
});
