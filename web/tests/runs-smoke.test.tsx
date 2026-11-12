import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { App } from "../src/App";

/** P5-8 冒烟：Run 列表（状态过滤）、aux 不可用/缺表真实空态、Run 详情
 *  （duration/input_hash/usage/environment JSON 解析 + artifact 预览入口与
 *  缺表说明）、Runtime 节点渲染与零行/不可用空态。 */

const RUNS_STUB = {
  available: true,
  source: "aux",
  total: 3,
  returned: 3,
  items: [
    {
      run_id: "rn-b5-rt-py",
      tool_id: "rip.kb.rtis",
      tool_version: "0.1.0",
      workspace_id: "ws-dev",
      status: "SUCCEEDED",
      duration_ms: 4210,
      created_at: "2026-11-10T04:57:00Z",
      finished_at: "2026-11-10T04:57:04Z",
    },
    {
      run_id: "rn-b5-kb-mjs",
      tool_id: "rip.kb.rtis",
      status: "FAILED",
      error_category: "EXECUTION",
      created_at: "2026-11-10T04:57:10Z",
    },
    {
      run_id: "rn-b5-eri",
      tool_id: "rip.kb.eri",
      status: "RUNNING",
      created_at: "2026-11-10T04:58:00Z",
    },
  ],
};

const RUN_DETAIL_STUB = {
  available: true,
  source: "aux",
  correlation: {
    trace_id: "tr-p75-smoke",
    task_id: null,
    agent_run_id: "arn-p75-agent",
    tool_run_id: "rn-b5-rt-py",
    artifact_ids: ["art-p75-1"],
    evidence_ids: ["ev-p75-1"],
  },
  run: {
    run_id: "rn-b5-rt-py",
    trace_id: "tr-p75-smoke",
    tool_id: "rip.kb.rtis",
    tool_version: "0.1.0",
    workspace_id: "ws-dev",
    input_hash: "ab".repeat(32),
    status: "SUCCEEDED",
    duration_ms: 4210,
    usage_json: { elapsed_ms: 4210, output_bytes: 2048, truncated: false },
    environment_json: { runner: "python_worker", fingerprint: "py3.12" },
    replay_json: { cached: false },
    created_at: "2026-11-10T04:57:00Z",
    finished_at: "2026-11-10T04:57:04Z",
  },
  artifacts: [],
  artifacts_note: "artifact 表不存在（对应迁移未应用），返回空列表",
};

const RUNTIME_NODES_STUB = {
  available: true,
  source: "aux",
  total: 2,
  items: [
    {
      node_id: "node-py-1",
      kind: "python_worker",
      endpoint: "local://runner",
      health: "ok",
      version: "runner 0.3",
      last_heartbeat_at: "2026-11-10T04:57:00Z",
      heartbeat_age_seconds: 42.5,
    },
    {
      node_id: "node-vm-1",
      kind: "podman_vm",
      endpoint: null,
      health: "degraded",
      version: null,
      last_heartbeat_at: null,
      heartbeat_age_seconds: null,
    },
  ],
};

function stubFetch(runsBody: unknown = RUNS_STUB, runtimeBody: unknown = RUNTIME_NODES_STUB): void {
  vi.stubGlobal(
    "fetch",
    vi.fn(async (input: RequestInfo | URL) => {
      const url = String(input);
      let body: unknown = {};
      if (url.includes("/api/runs?") && url.includes("run_id=")) body = RUN_DETAIL_STUB;
      else if (url.includes("/api/runs")) body = runsBody;
      else if (url.includes("/api/runtime")) body = runtimeBody;
      return new Response(JSON.stringify(body), {
        status: 200,
        headers: { "content-type": "application/json; charset=utf-8" },
      });
    }),
  );
}

beforeEach(() => {
  stubFetch();
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
  document.body.innerHTML = "";
});

describe("Runs 页（P5-8）", () => {
  it("渲染 3 条真实 Run（状态/时长/工具）与 aux 数据源标注", async () => {
    window.history.pushState({}, "", "/runs");
    render(<App />);
    expect(await screen.findByText("rn-b5-rt-py")).toBeTruthy();
    expect(screen.getByText("rn-b5-kb-mjs")).toBeTruthy();
    expect(screen.getByText("rn-b5-eri")).toBeTruthy();
    expect(screen.getByText(/共 3 条（显示 3，数据源：RP_AUX_DB 开发库副本）/)).toBeTruthy();
    expect(screen.getByText("4210 ms")).toBeTruthy();
    expect(screen.getByText("EXECUTION")).toBeTruthy();
    expect(screen.getAllByText("SUCCEEDED").length).toBeGreaterThan(0);
    // 行点击进详情
    expect(screen.getByRole("link", { name: /rn-b5-rt-py/ }).getAttribute("href")).toBe(
      "/runs/rn-b5-rt-py",
    );
  });

  it("状态过滤走服务端参数", async () => {
    const fetchMock = vi.fn(async (input: RequestInfo | URL) => {
      const url = String(input);
      const body = url.includes("run_id=")
        ? RUN_DETAIL_STUB
        : url.includes("/api/runs")
          ? RUNS_STUB
          : {};
      return new Response(JSON.stringify(body), { status: 200 });
    });
    vi.stubGlobal("fetch", fetchMock);
    window.history.pushState({}, "", "/runs");
    render(<App />);
    await screen.findByText("rn-b5-rt-py");
    fireEvent.change(screen.getByLabelText("按状态过滤"), { target: { value: "SUCCEEDED" } });
    await waitFor(() => {
      const called = fetchMock.mock.calls.some((call) =>
        String(call[0]).includes("status=SUCCEEDED"),
      );
      if (!called) throw new Error("状态过滤请求未发出");
    });
  });

  it("aux 未配置时显服务端真实空态说明", async () => {
    stubFetch({
      available: false,
      reason: "aux_db_not_configured",
      message:
        "未配置 RP_AUX_DB，且主库无 tool_run 表：Phase 1-4 数据面暂不可读（真实空态说明，非空数据）",
      total: 0,
      items: [],
    });
    window.history.pushState({}, "", "/runs");
    render(<App />);
    expect(
      await screen.findByText(
        "未配置 RP_AUX_DB，且主库无 tool_run 表：Phase 1-4 数据面暂不可读（真实空态说明，非空数据）",
      ),
    ).toBeTruthy();
  });

  it("Loading 态渲染骨架", () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(() => new Promise<Response>(() => {})),
    );
    window.history.pushState({}, "", "/runs");
    render(<App />);
    expect(document.querySelector(".skeleton")).toBeTruthy();
  });
});

describe("Run 详情页（P5-8）", () => {
  function renderDetail(runId: string): void {
    window.history.pushState({}, "", `/runs/${encodeURIComponent(runId)}`);
    render(<App />);
  }

  it("渲染概要字段、JSON 块与 artifact 缺表说明", async () => {
    renderDetail("rn-b5-rt-py");
    // P7-1 壳层：会话 tab 先于详情数据出现 run id，页面就绪标记改用状态徽标。
    expect(await screen.findByText("SUCCEEDED")).toBeTruthy();
    expect(screen.getAllByText("rn-b5-rt-py").length).toBeGreaterThan(0);
    expect(screen.getByText("4210 ms")).toBeTruthy();
    expect(screen.getByText(`input_hash`)).toBeTruthy();
    expect(screen.getByText("ab".repeat(32))).toBeTruthy();
    // JSON 块：usage 与 environment 原样解析渲染
    expect(screen.getByText("usage")).toBeTruthy();
    expect(screen.getByText(/"elapsed_ms": 4210/)).toBeTruthy();
    // P7-5 六 ID 关联面在场（correlation）且可点击跳转 /traces?id=
    expect(screen.getByText("trace")).toBeTruthy();
    const traceLink = screen.getByRole("link", { name: "tr-p75-smoke" });
    expect(traceLink.getAttribute("href")).toBe("/traces?id=tr-p75-smoke");
    expect(screen.getByText("agent_run")).toBeTruthy();
    expect(
      screen.getByRole("link", { name: "arn-p75-agent" }).getAttribute("href"),
    ).toBe("/traces?id=arn-p75-agent");
    expect(screen.getByText("tool_run")).toBeTruthy();
    expect(
      screen.getByRole("link", { name: "rn-b5-rt-py" }).getAttribute("href"),
    ).toBe("/traces?id=rn-b5-rt-py");
    expect(screen.getByText("artifacts")).toBeTruthy();
    expect(screen.getByRole("link", { name: "art-p75-1" }).getAttribute("href")).toBe(
      "/traces?id=art-p75-1",
    );
    expect(screen.getByText("evidence")).toBeTruthy();
    expect(screen.getByRole("link", { name: "ev-p75-1" }).getAttribute("href")).toBe(
      "/traces?id=ev-p75-1",
    );
    expect(screen.getByText("environment")).toBeTruthy();
    expect(screen.getByText(/"fingerprint": "py3.12"/)).toBeTruthy();
    // artifact 缺表：服务端 note + 空列表说明
    expect(screen.getByText("artifact 表不存在（对应迁移未应用），返回空列表")).toBeTruthy();
    expect(screen.getByText("本 Run 无已登记 artifact。")).toBeTruthy();
  });

  it("artifact 存在时渲染预览入口链接", async () => {
    stubFetch(RUN_DETAIL_STUB, RUNTIME_NODES_STUB);
    vi.stubGlobal(
      "fetch",
      vi.fn(async (input: RequestInfo | URL) => {
        const url = String(input);
        let body: unknown = {};
        if (url.includes("run_id=")) {
          body = {
            ...RUN_DETAIL_STUB,
            artifacts_note: null,
            artifacts: [
              {
                artifact_id: "art-1",
                run_id: "rn-b5-rt-py",
                role: "stdout",
                uri: "artifacts://rn-b5-rt-py/stdout.log",
                sha256: "c".repeat(64),
                size_bytes: 2048,
                mime_type: "text/plain",
                created_at: "2026-11-10T04:57:05Z",
              },
            ],
          };
        } else if (url.includes("/api/runs")) body = RUNS_STUB;
        else if (url.includes("/api/runtime")) body = RUNTIME_NODES_STUB;
        return new Response(JSON.stringify(body), { status: 200 });
      }),
    );
    renderDetail("rn-b5-rt-py");
    const preview = await screen.findByRole("link", { name: "预览 / 下载" });
    expect(preview.getAttribute("href")).toBe("artifacts://rn-b5-rt-py/stdout.log");
    expect(screen.getByText("2.0 KB")).toBeTruthy();
    expect(screen.getByText("stdout")).toBeTruthy();
  });

  it("404 渲染 ErrorPanel", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(
        async () =>
          new Response(
            JSON.stringify({ error: { code: "no_run", message: "Run 不存在：ghost" } }),
            { status: 404 },
          ),
      ),
    );
    renderDetail("ghost");
    expect(await screen.findByText("数据不存在")).toBeTruthy();
  });
});

describe("Runtime 页（P5-8）", () => {
  it("渲染节点（kind/health/version/endpoint/心跳）", async () => {
    window.history.pushState({}, "", "/runtime");
    render(<App />);
    expect(await screen.findByText("node-py-1")).toBeTruthy();
    expect(screen.getByText("python_worker")).toBeTruthy();
    expect(screen.getByText("ok")).toBeTruthy();
    expect(screen.getByText("runner 0.3")).toBeTruthy();
    expect(screen.getByText("endpoint: local://runner")).toBeTruthy();
    expect(screen.getByText(/43 秒前/)).toBeTruthy();
    // 从未心跳的节点诚实显示（整行文本为「心跳: 从未收到心跳」，取正则）
    expect(screen.getByText("node-vm-1")).toBeTruthy();
    expect(screen.getByText("degraded")).toBeTruthy();
    expect(screen.getByText(/从未收到心跳/)).toBeTruthy();
    // 待验证说明在页
    expect(screen.getByText(/实时心跳依赖 Gateway 常驻编排/)).toBeTruthy();
  });

  it("零行与不可用均为真实空态", async () => {
    // 第一步：runtime 零行（available=true, items=[]）→ 页内固定空态文案
    stubFetch(RUNS_STUB, {
      available: true,
      source: "aux",
      total: 0,
      items: [],
    });
    window.history.pushState({}, "", "/runtime");
    render(<App />);
    expect(
      await screen.findByText(
        "暂无运行时节点：runtime_node 为空（Gateway 心跳尚未接入或节点未登记）。",
      ),
    ).toBeTruthy();
    cleanup();
    // 第二步：表缺失守卫 → 显服务端 message
    vi.stubGlobal(
      "fetch",
      vi.fn(
        async () =>
          new Response(
            JSON.stringify({
              available: false,
              reason: "table_missing",
              message: "aux 库（dev-b5.db）中不存在 runtime_node 表：对应 Phase 迁移未应用",
              total: 0,
              items: [],
            }),
            { status: 200 },
          ),
      ),
    );
    window.history.pushState({}, "", "/runtime");
    render(<App />);
    expect(
      await screen.findByText("aux 库（dev-b5.db）中不存在 runtime_node 表：对应 Phase 迁移未应用"),
    ).toBeTruthy();
    cleanup();
    window.history.pushState({}, "", "/runtime");
    render(<App />);
    expect(
      await screen.findByText(/aux 库（dev-b5.db）中不存在 runtime_node 表：对应 Phase 迁移未应用/),
    ).toBeTruthy();
  });
});
