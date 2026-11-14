import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { App } from "../src/App";

/** P9-4 Tool Center MCP 目录同步状态卡冒烟：状态卡（目录条数/能力面数/
 *  最后对账时间/对账正常 badge）、对账漂移如实展示、数据面不可读诚实
 *  空态、读取失败降级说明。目录列表本体归 tools-smoke.test.tsx。 */

const MCP_OK = {
  available: true,
  source: "main",
  catalog_count: 21,
  capability_count: 6,
  capability_tools: [
    "rip.list_tools",
    "rip.get_tool_schema",
    "rip.run_tool",
    "rip.search_knowledge",
    "rip.get_run_status",
    "rip.cancel_run",
  ],
  reconciliation: {
    ok: true,
    reconciled_at: "2026-11-14T08:00:00Z",
    drift: [],
    scope: "读面对账（注册面完整性 + 能力面词表）；跨通道 wire 对账归 rp-mcp-parity CI 门禁",
  },
};

const TOOLS_STUB = {
  available: true,
  source: "main",
  total: 21,
  returned: 1,
  items: [
    {
      tool_id: "rip.kb.keyword_search",
      name: "KB Keyword Search",
      kind: "tool",
      lifecycle: "ENABLED",
      governance: "ACTIVE",
      health_state: "ok",
      permission_level: "T0",
      updated_at: "2026-11-14T07:00:00Z",
    },
  ],
};

type StubMap = Record<string, unknown>;

function stubFetch(map: StubMap, fallback: unknown = {}): void {
  vi.stubGlobal(
    "fetch",
    vi.fn(async (input: RequestInfo | URL) => {
      const url = String(input);
      const path = url.replace(/^https?:\/\/[^/]+/, "").split("?")[0] ?? "";
      const body = map[path] ?? fallback;
      return new Response(JSON.stringify(body), {
        status: 200,
        headers: { "content-type": "application/json; charset=utf-8" },
      });
    }),
  );
}

beforeEach(() => {
  window.history.pushState({}, "", "/tools");
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
  document.body.innerHTML = "";
});

describe("Tool Center MCP 目录同步状态卡（P9-4）", () => {
  it("对账正常：目录条数 + 能力面数 + 最后对账时间在卡内", async () => {
    stubFetch({ "/api/tools": TOOLS_STUB, "/api/mcp-catalog": MCP_OK });
    render(<App />);
    expect(await screen.findByLabelText("MCP 目录同步状态")).toBeTruthy();
    expect(await screen.findByText(/目录条数 21（能力面 6 条固定）/)).toBeTruthy();
    expect(screen.getByText("对账正常")).toBeTruthy();
    expect(screen.getByText(/最后对账/)).toBeTruthy();
    expect(screen.getByText(/跨通道 wire 对账归 rp-mcp-parity CI 门禁/)).toBeTruthy();
    // 目录列表本体不受状态卡影响
    expect(await screen.findByText("共 21 个工具（显示 1，数据源：主库注册面）")).toBeTruthy();
  });

  it("对账漂移：ok=false 与漂移条目如实展示", async () => {
    stubFetch({
      "/api/tools": TOOLS_STUB,
      "/api/mcp-catalog": {
        ...MCP_OK,
        reconciliation: {
          ok: false,
          reconciled_at: "2026-11-14T08:00:00Z",
          drift: [
            { check: "lifecycle_wordlist", tool_id: "nmap", detail: "lifecycle=WEIRD 不在词表" },
          ],
          scope: MCP_OK.reconciliation.scope,
        },
      },
    });
    render(<App />);
    expect(await screen.findByText("对账漂移 1 条")).toBeTruthy();
    expect(screen.getByText("lifecycle_wordlist")).toBeTruthy();
    expect(screen.getByText(/nmap：lifecycle=WEIRD 不在词表/)).toBeTruthy();
    expect(screen.queryByText("对账正常")).toBeNull();
  });

  it("数据面不可读：available=false 诚实空态说明", async () => {
    stubFetch({
      "/api/tools": TOOLS_STUB,
      "/api/mcp-catalog": {
        available: false,
        reason: "table_missing",
        message: "主库缺 tool 表（迁移水位不足），目录暂不可读",
      },
    });
    render(<App />);
    expect(await screen.findByText("主库缺 tool 表（迁移水位不足），目录暂不可读")).toBeTruthy();
    expect(screen.queryByText("对账正常")).toBeNull();
  });

  it("状态接口失败：降级说明不冒充状态（目录列表照常）", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(async (input: RequestInfo | URL) => {
        const url = String(input);
        if (url.includes("/api/mcp-catalog")) {
          return new Response("boom", { status: 500 });
        }
        if (url.includes("/api/tools")) {
          return new Response(JSON.stringify(TOOLS_STUB), {
            status: 200,
            headers: { "content-type": "application/json; charset=utf-8" },
          });
        }
        return new Response("{}", { status: 200 });
      }),
    );
    render(<App />);
    expect(await screen.findByText(/MCP 目录状态读取失败/)).toBeTruthy();
    expect(screen.getByText(/仅状态卡不可用/)).toBeTruthy();
    expect(await screen.findByText("共 21 个工具（显示 1，数据源：主库注册面）")).toBeTruthy();
  });
});
