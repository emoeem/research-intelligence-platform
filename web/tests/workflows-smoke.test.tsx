import { cleanup, render, screen, fireEvent, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { App } from "../src/App";
import {
  flowToGraph,
  graphToFlow,
  layeredPositions,
  precheckIssues,
  buildDefinition,
} from "../src/lib/workflow-graph";
import type { WorkflowGraph } from "../src/lib/api";

/** P8-4 冒烟：Workflow Studio 实页五区（/workflows，计划 §10.4 G）：
 *  1. 左节点分类面板（DSL 八类词表）；2. 中央 @xyflow/react 画布（真实
 *  FlowCanvas + ResizeObserver stub，graph 装载）；3. 右节点参数 Schema 表单
 *  与运行配置；4. 底部执行日志节点状态产物（017 节点行 + 018 控制行 + 六 ID）；
 *  5. 版本列表与历史回看（只读降级）。写面断言：保存（P8-4 put payload）、
 *  运行/暂停（P8-3 API 路径与请求体）、移动端只读（matchMedia stub）。
 *  纯函数（互转/布局/预检）不依赖 DOM 直接覆盖。 */

const WF_ID = "wf-p84-demo";
const RUN_ID = "wfr-p84-smoke";
const TRACE_ID = "tr-p84-smoke";

const GRAPH_V1: WorkflowGraph = {
  nodes: [
    { id: "start", type: "start" },
    { id: "fetch", type: "tool", params: { tool_id: "rip.kb.keyword_search" } },
    { id: "done", type: "end" },
  ],
  edges: [
    { from: "start", to: "fetch" },
    { from: "fetch", to: "done" },
  ],
};

const GRAPH_V2: WorkflowGraph = {
  nodes: [
    { id: "start", type: "start" },
    { id: "retrieve", type: "knowledge", params: { query: "$/target" } },
    { id: "review", type: "human", params: { prompt: "是否批准?", key: "approval" } },
    { id: "done", type: "end" },
  ],
  edges: [
    { from: "start", to: "retrieve" },
    { from: "retrieve", to: "review" },
    { from: "review", to: "done" },
  ],
};

const DEFINITION_ROW = (version: number, graph: WorkflowGraph) => ({
  workflow_id: WF_ID,
  version,
  name: "P8-4 画布演示流",
  description: "start -> tool -> end",
  compat: "graph-v1",
  node_count: graph.nodes.length,
  edge_count: graph.edges.length,
  input_schema: { type: "object", properties: { target: { type: "string" } } },
  source: "rp-workflow-put",
  created_at: `2026-11-14T0${version}:00:00Z`,
  graph,
});

const WORKFLOWS_LIST_STUB = {
  available: true,
  source: "main+aux",
  total_workflows: 1,
  total_versions: 2,
  items: [DEFINITION_ROW(1, GRAPH_V1), DEFINITION_ROW(2, GRAPH_V2)],
};

const WORKFLOWS_DETAIL_STUB = {
  ...WORKFLOWS_LIST_STUB,
  items: [DEFINITION_ROW(1, GRAPH_V1), DEFINITION_ROW(2, GRAPH_V2)],
};

const RUNS_LIST_STUB = {
  available: true,
  source: "main+aux",
  total: 1,
  returned: 1,
  items: [
    {
      run_id: RUN_ID,
      workflow_id: WF_ID,
      workflow_version: 2,
      state: "paused",
      trace_id: TRACE_ID,
      task_id: "rtis-20261114-000000-001",
      created_at: "2026-11-14T01:00:00Z",
    },
  ],
};

const RUN_DETAIL_STUB = {
  available: true,
  source: "main+aux",
  query_id: RUN_ID,
  run: {
    run_id: RUN_ID,
    workflow_id: WF_ID,
    workflow_version: 2,
    state: "paused",
    trace_id: TRACE_ID,
    task_id: "rtis-20261114-000000-001",
    input: { target: "example.com" },
    output: null,
    error: null,
    created_at: "2026-11-14T01:00:00Z",
    started_at: "2026-11-14T01:00:01Z",
    finished_at: null,
  },
  nodes: [
    {
      node_id: "start",
      node_kind: "start",
      state: "succeeded",
      attempt: 1,
      output: null,
      error: null,
      tool_run_id: null,
      agent_run_id: null,
    },
    {
      node_id: "review",
      node_kind: "human",
      state: "succeeded",
      attempt: 2,
      output: { approval: { ok: true } },
      error: null,
      tool_run_id: null,
      agent_run_id: null,
    },
    {
      node_id: "probe",
      node_kind: "tool",
      state: "failed",
      attempt: 1,
      output: null,
      error: { code: "tool_failed" },
      tool_run_id: "rn-p84-failed",
      agent_run_id: null,
    },
  ],
  checkpoint_seq: 3,
  control: {
    approval_node_id: "review",
    approval_request: { prompt: "是否批准?", key: "approval" },
    approval: { ok: true },
    approval_updated_at: "2026-11-14T01:10:00Z",
    awaiting_approval: true,
    pause_requested_at: "2026-11-14T01:05:00Z",
    cancel_requested_at: null,
  },
  correlation: {
    trace_id: TRACE_ID,
    task_id: "rtis-20261114-000000-001",
    agent_run_id: null,
    tool_run_id: "rn-p84-failed",
    artifact_id: "art-p84-smoke",
    evidence_id: "ev-p84-smoke",
  },
  trace_query: `/api/traces?id=${TRACE_ID}`,
};

const EMPTY_LISTS_STUB = {
  available: true,
  source: "main",
  total_workflows: 0,
  total_versions: 0,
  items: [],
  message: "暂无 workflow 定义：rp-workflow put 端到端链未产生定义记录",
};

/** fetch stub：按 method + pathname + search 分流，并记录调用（写面断言用）。 */
let calls: { method: string; pathname: string; body: unknown }[] = [];

function stubFetch(
  handler: (pathname: string, search: string, method: string, body: unknown) => unknown,
): void {
  vi.stubGlobal(
    "fetch",
    vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const url = typeof input === "string" ? input : input instanceof URL ? input.href : input.url;
      const parsed = new URL(url, "http://localhost");
      let body: unknown = null;
      if (init?.body) {
        try {
          body = JSON.parse(String(init.body));
        } catch {
          body = String(init.body);
        }
      }
      calls.push({ method: init?.method ?? "GET", pathname: parsed.pathname, body });
      return new Response(
        JSON.stringify(handler(parsed.pathname, parsed.search, init?.method ?? "GET", body) ?? {}),
        {
          status: 200,
          headers: { "content-type": "application/json; charset=utf-8" },
        },
      );
    }),
  );
}

function stubDefaultData(): void {
  stubFetch((pathname, search, method) => {
    if (pathname === "/api/workflows") {
      if (method === "POST") {
        return { result: "put", workflow_id: WF_ID, version: 3, nodes: 3, edges: 2 };
      }
      return search.includes("id=") ? WORKFLOWS_DETAIL_STUB : WORKFLOWS_LIST_STUB;
    }
    if (pathname === "/api/workflow-runs") {
      if (method === "POST") {
        return {
          result: "accepted",
          action: "run",
          run_id: RUN_ID,
          status_query: `/api/workflow-runs?id=${RUN_ID}`,
        };
      }
      if (/\/api\/workflow-runs\/[^/]+\/(pause|resume|retry)$/.test(pathname)) {
        return { result: "pause-requested", action: pathname.split("/").pop(), run_id: RUN_ID };
      }
      return search.includes("id=") ? RUN_DETAIL_STUB : RUNS_LIST_STUB;
    }
    return {};
  });
}

beforeEach(() => {
  calls = [];
  // jsdom 无 ResizeObserver：@xyflow/system 观察容器尺寸需要 stub（不伪造
  // 断言，只为让真实画布在 jsdom 挂载）。
  class ResizeObserverStub {
    observe(): void {}
    unobserve(): void {}
    disconnect(): void {}
  }
  vi.stubGlobal("ResizeObserver", ResizeObserverStub);
  stubDefaultData();
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
  document.body.innerHTML = "";
});

describe("Workflow Studio 五区壳层（P8-4）", () => {
  it("五区 section 在场：面板八类词表 / 画布 graph 装载 / 版本 chips / run 列表", async () => {
    window.history.pushState({}, "", "/workflows");
    render(<App />);
    expect(
      await screen.findByTestId("workflows-palette-section", {}, { timeout: 5000 }),
    ).toBeTruthy();
    expect(screen.getByTestId("workflows-canvas-section")).toBeTruthy();
    expect(screen.getByTestId("workflows-inspector-section")).toBeTruthy();
    expect(screen.getByTestId("workflows-run-section")).toBeTruthy();
    expect(screen.getByTestId("workflows-versions-section")).toBeTruthy();
    // 面板：DSL 八类（start/end/tool/knowledge/agent/human/condition/parallel）
    for (const type of [
      "start",
      "end",
      "tool",
      "knowledge",
      "agent",
      "human",
      "condition",
      "parallel",
    ]) {
      expect(screen.getByTestId(`palette-${type}`)).toBeTruthy();
    }
    // workflow 输入框 + 版本 chips（列表数据落位后 v1/v2 在场）
    await waitFor(() => expect(screen.getByTestId("workflow-id-select")).toBeTruthy());
    fireEvent.change(screen.getByTestId("workflow-id-select"), { target: { value: WF_ID } });
    expect(await screen.findByTestId("version-chip-1")).toBeTruthy();
    expect(screen.getByTestId("version-chip-2")).toBeTruthy();
    // 画布装载最新版 graph（真实 FlowCanvas）：节点 DOM 落位 + 连接柄齐备
    // （边 DOM 依赖 RF 节点测量，jsdom 下 ResizeObserver stub 不回报尺寸，
    // 以节点/柄计数替代边 DOM 计数，边渲染属浏览器 E2E 证据）。
    await waitFor(() => expect(document.querySelector(".react-flow__node")!).toBeTruthy());
    const nodeEls = document.querySelectorAll(".react-flow__node");
    expect(nodeEls.length).toBe(GRAPH_V2.nodes.length);
    // start(1 源柄) + retrieve/review(各 2 柄) + done(1 目标柄) = 6
    expect(document.querySelectorAll(".react-flow__handle").length).toBe(6);
  });

  it("历史回看：选中 v1 进入只读（画布渲染 v1 graph），返回编辑最新", async () => {
    window.history.pushState({}, "", "/workflows");
    render(<App />);
    fireEvent.change(await screen.findByTestId("workflow-id-select", {}, { timeout: 5000 }), {
      target: { value: WF_ID },
    });
    fireEvent.click(await screen.findByTestId("version-chip-1"));
    expect(await screen.findByTestId("readonly-history-badge")).toBeTruthy();
    expect(screen.getByTestId("readonly-banner")).toBeTruthy();
    expect(screen.getByTestId("readonly-banner").textContent).toContain("历史回看只读");
    // 保存按钮在只读态禁用
    expect((screen.getByTestId("save-definition-btn") as HTMLButtonElement).disabled).toBe(true);
    // 回到编辑态
    fireEvent.click(screen.getByText(/编辑最新/));
    await waitFor(() => expect(screen.queryByTestId("readonly-history-badge")).toBeNull());
    expect((screen.getByTestId("save-definition-btn") as HTMLButtonElement).disabled).toBe(false);
  });

  it("节点参数 Schema 表单：面板点击追加 tool 节点后选中编辑 tool_id，必填提示在场", async () => {
    window.history.pushState({}, "", "/workflows");
    render(<App />);
    fireEvent.change(await screen.findByTestId("workflow-id-select", {}, { timeout: 5000 }), {
      target: { value: WF_ID },
    });
    await screen.findByTestId("version-chip-1");
    // 面板点击追加（编辑态）：追加一个 tool 节点
    fireEvent.click(screen.getByTestId("palette-tool"));
    await waitFor(() => {
      const ids = [...document.querySelectorAll(".react-flow__node")].map((el) =>
        el.getAttribute("data-id"),
      );
      expect(ids).toContain("tool-1");
    });
    // 点击画布节点 → Inspector 出 Schema 表单（tool_id 必填）
    const nodeEl = document.querySelector('.react-flow__node[data-id="tool-1"]')!;
    fireEvent.click(nodeEl);
    const toolIdInput = await screen.findByLabelText(/tool_id/);
    expect(toolIdInput).toBeTruthy();
    expect(screen.getAllByText("必填").length).toBeGreaterThan(0);
    // 填参数 → data 更新（保存 payload 断言在下一用例覆盖）
    fireEvent.change(toolIdInput, { target: { value: "rip.kb.eri" } });
    await waitFor(() => expect((toolIdInput as HTMLInputElement).value).toBe("rip.kb.eri"));
    expect(screen.getByTestId("inspector-remove-node")).toBeTruthy();
    // 运行配置区（第三区下段）字段在场
    expect(screen.getByTestId("run-start-btn")).toBeTruthy();
    expect(screen.getByTestId("run-input-json")).toBeTruthy();
  });

  it("保存链路：画布图转 definition JSON 走 POST /api/workflows，payload 含 graph 与声明 version", async () => {
    window.history.pushState({}, "", "/workflows");
    render(<App />);
    fireEvent.change(await screen.findByTestId("workflow-id-select", {}, { timeout: 5000 }), {
      target: { value: WF_ID },
    });
    await screen.findByTestId("version-chip-1");
    fireEvent.click(screen.getByTestId("save-definition-btn"));
    expect(await screen.findByTestId("save-message")).toBeTruthy();
    expect(screen.getByTestId("save-message").textContent).toContain("v3");
    const putCall = calls.find(
      (call) => call.method === "POST" && call.pathname === "/api/workflows",
    );
    expect(putCall).toBeTruthy();
    const definition = (putCall!.body as { definition: Record<string, unknown> }).definition;
    expect(definition.schema).toBe("workflow/definition-v1");
    expect(definition.workflow_id).toBe(WF_ID);
    expect(definition.compat).toBe("graph-v1");
    // 画布当前装载 v2 graph：flowToGraph 还原节点/边向量
    expect((definition.graph as { nodes: unknown[] }).nodes.length).toBe(GRAPH_V2.nodes.length);
    expect((definition.graph as { edges: unknown[] }).edges.length).toBe(GRAPH_V2.edges.length);
    expect(definition.version).toBe(3); // 存储侧递增的声明随行
  });

  it("空图保存预检：start 缺失列入 save-issues，不发起 POST", async () => {
    window.history.pushState({}, "", "/workflows");
    render(<App />);
    fireEvent.change(await screen.findByTestId("workflow-id-select", {}, { timeout: 5000 }), {
      target: { value: WF_ID },
    });
    await screen.findByTestId("version-chip-1");
    // 删掉画布全部节点（逐个经 Inspector 删除按钮，或直接断言预检逻辑在
    // 页面层生效：这里用 API 层面等价验证 precheckIssues）
    expect(precheckIssues({ nodes: [], edges: [] }).length).toBeGreaterThan(0);
    expect(precheckIssues(GRAPH_V2)).toEqual([]);
    // 页面上清空节点后保存：issues 呈现且零 POST（逐节点选中后删除；先等
    // 画布节点 DOM 落位，RF 节点渲染走内部 store 异步批处理）
    await waitFor(() => expect(document.querySelector(".react-flow__node")!).toBeTruthy());
    const before = calls.filter(
      (call) => call.pathname === "/api/workflows" && call.method === "POST",
    ).length;
    let guard = 0;
    while (document.querySelector(".react-flow__node") && guard < 10) {
      guard += 1;
      fireEvent.click(document.querySelector(".react-flow__node")!);
      const remove = await screen.findByTestId("inspector-remove-node", {}, { timeout: 1000 });
      fireEvent.click(remove);
      await waitFor(() => expect(screen.queryByTestId("inspector-remove-node")).toBeNull());
    }
    expect(document.querySelectorAll(".react-flow__node").length).toBe(0);
    fireEvent.click(screen.getByTestId("save-definition-btn"));
    await waitFor(() => expect(screen.getByTestId("save-issues")).toBeTruthy());
    expect(screen.getByTestId("save-issues").textContent).toContain("图为空");
    const after = calls.filter(
      (call) => call.pathname === "/api/workflows" && call.method === "POST",
    ).length;
    expect(after).toBe(before);
  });

  it("运行受理：POST /api/workflow-runs 带 workflow_id/input，run 详情六 ID 与控制行在场", async () => {
    window.history.pushState({}, "", "/workflows");
    render(<App />);
    fireEvent.change(await screen.findByTestId("workflow-id-select", {}, { timeout: 5000 }), {
      target: { value: WF_ID },
    });
    await screen.findByTestId("version-chip-1");
    // 发起运行（默认 input {}）
    fireEvent.click(screen.getByTestId("run-start-btn"));
    await waitFor(() => {
      const runCall = calls.find(
        (call) => call.method === "POST" && call.pathname === "/api/workflow-runs",
      );
      expect(runCall).toBeTruthy();
    });
    const runCall = calls.find(
      (call) => call.method === "POST" && call.pathname === "/api/workflow-runs",
    )!;
    expect((runCall.body as { workflow_id: string }).workflow_id).toBe(WF_ID);
    // 受理后自动选中新 run（stub 的 ?id= 恒回详情）→ 详情区块落位
    expect(await screen.findByTestId("awaiting-approval-badge")).toBeTruthy();
    expect(screen.getByTestId("awaiting-approval-badge").textContent).toContain("review");
    expect(screen.getByTestId("pause-requested-badge")).toBeTruthy();
    // 节点行（017）：attempt + 失败节点重试按钮
    expect(screen.getAllByTestId("workflow-node-row").length).toBe(RUN_DETAIL_STUB.nodes.length);
    expect(screen.getByText(/attempt 2/)).toBeTruthy();
    expect(screen.getByText("重试该节点")).toBeTruthy();
    // 六 ID 关联（文本节点被 JSX 拆分，用子串匹配；节点行与关联行各一处）
    expect(screen.getAllByText(/rn-p84-failed/).length).toBeGreaterThan(0);
    expect(screen.getAllByText(/art-p84-smoke/).length).toBeGreaterThan(0);
    expect(screen.getAllByText(/ev-p84-smoke/).length).toBeGreaterThan(0);
    const traceLink = screen.getAllByRole("link", { name: TRACE_ID })[0]!;
    expect(traceLink.getAttribute("href")).toBe(`/traces?id=${TRACE_ID}`);
    // 018 介入请求视图 + 批准参数注入框
    expect(screen.getByTestId("approval-request-view")).toBeTruthy();
    expect((screen.getByTestId("resume-inject-input") as HTMLTextAreaElement).value).toContain(
      "approval",
    );
  });

  it("暂停/恢复/重试接 P8-3 REST 动态路径", async () => {
    window.history.pushState({}, "", "/workflows");
    render(<App />);
    fireEvent.change(await screen.findByTestId("workflow-id-select", {}, { timeout: 5000 }), {
      target: { value: WF_ID },
    });
    await screen.findByTestId("version-chip-1");
    fireEvent.click(screen.getByTestId("run-start-btn"));
    await screen.findByTestId("awaiting-approval-badge");
    fireEvent.click(screen.getByTestId("run-pause-btn"));
    await waitFor(() => {
      expect(
        calls.some(
          (call) =>
            call.method === "POST" && call.pathname === `/api/workflow-runs/${RUN_ID}/pause`,
        ),
      ).toBe(true);
    });
    fireEvent.click(screen.getByTestId("run-resume-btn"));
    await waitFor(() => {
      const resume = calls.find(
        (call) => call.method === "POST" && call.pathname === `/api/workflow-runs/${RUN_ID}/resume`,
      );
      expect(resume).toBeTruthy();
      expect((resume!.body as { inject: unknown }).inject).toEqual({ approval: { ok: true } });
    });
    fireEvent.click(screen.getByText("重试该节点"));
    await waitFor(() => {
      const retry = calls.find(
        (call) => call.method === "POST" && call.pathname === `/api/workflow-runs/${RUN_ID}/retry`,
      );
      expect(retry).toBeTruthy();
      expect((retry!.body as { node_id: string }).node_id).toBe("probe");
    });
  });

  it("空定义与空运行列表渲染服务端空态说明（不伪造）", async () => {
    stubFetch((pathname, search) => {
      if (pathname === "/api/workflows" && !search.includes("id=")) return EMPTY_LISTS_STUB;
      if (pathname === "/api/workflow-runs" && !search.includes("id=")) {
        return {
          ...EMPTY_LISTS_STUB,
          message: "暂无 WorkflowRun：rp-workflow run 端到端链未产生运行记录",
        };
      }
      return {};
    });
    window.history.pushState({}, "", "/workflows");
    render(<App />);
    expect(await screen.findByTestId("defs-empty-note", {}, { timeout: 5000 })).toBeTruthy();
    expect(screen.getByTestId("defs-empty-note").textContent).toContain("暂无 workflow 定义");
    expect(screen.getByText(/暂无 WorkflowRun/)).toBeTruthy();
  });

  it("移动端只读降级：matchMedia 命中窄视口 → 只读徽标 + 追加节点被拒", async () => {
    vi.stubGlobal(
      "matchMedia",
      vi.fn().mockImplementation((query: string) => ({
        matches: query === "(max-width: 767px)",
        media: query,
        addEventListener: vi.fn(),
        removeEventListener: vi.fn(),
      })),
    );
    window.history.pushState({}, "", "/workflows");
    render(<App />);
    fireEvent.change(await screen.findByTestId("workflow-id-select", {}, { timeout: 5000 }), {
      target: { value: WF_ID },
    });
    await screen.findByTestId("version-chip-1");
    expect(await screen.findByTestId("readonly-banner")).toBeTruthy();
    expect(screen.getByTestId("readonly-banner").textContent).toContain("移动端只读查看");
    // 只读态点击面板不追加节点（画布节点数保持 v2 graph 原样）
    const before = document.querySelectorAll(".react-flow__node").length;
    fireEvent.click(screen.getByTestId("palette-tool"));
    expect(document.querySelectorAll(".react-flow__node").length).toBe(before);
  });
});

describe("画布模型纯函数（P8-4）", () => {
  it("graphToFlow → flowToGraph 往返保持节点/边/词形；布局同输入恒同", () => {
    const { nodes } = graphToFlow(GRAPH_V2);
    expect(nodes.map((node) => node.id)).toEqual(["start", "retrieve", "review", "done"]);
    expect((nodes[1]!.data as { nodeType: string }).nodeType).toBe("knowledge");
    const branchGraph = {
      nodes: [
        { id: "start", type: "start" },
        { id: "cond", type: "condition", params: { expression: "ok" } },
        { id: "a", type: "end" },
        { id: "b", type: "end" },
      ],
      edges: [
        { from: "start", to: "cond" },
        { from: "cond", to: "a", kind: "branch", when: { eq: 1 } },
        { from: "cond", to: "b", kind: "branch" },
      ],
    };
    const flow = graphToFlow(branchGraph);
    expect(flow.edges.filter((edge) => edge.label === "branch").length).toBe(2);
    const roundTrip = flowToGraph(flow.nodes, flow.edges);
    expect(roundTrip.nodes.map((node) => node.type)).toEqual(["start", "condition", "end", "end"]);
    expect(roundTrip.edges.filter((edge) => edge.kind === "branch").length).toBe(2);
    expect(roundTrip.edges.find((edge) => edge.to === "a")?.when).toEqual({ eq: 1 });
    // 确定性布局：两次计算逐点相等
    const p1 = layeredPositions(branchGraph.nodes, branchGraph.edges);
    const p2 = layeredPositions(branchGraph.nodes, branchGraph.edges);
    expect([...p1.entries()]).toEqual([...p2.entries()]);
    expect(p1.get("cond")!.x).toBeGreaterThan(p1.get("start")!.x);
  });

  it("buildDefinition 装配 definition-v1 文档形状", () => {
    const doc = buildDefinition({
      workflowId: WF_ID,
      name: "demo",
      inputSchema: { type: "object" },
      graph: GRAPH_V1,
      version: 2,
    });
    expect(doc.schema).toBe("workflow/definition-v1");
    expect(doc.compat).toBe("graph-v1");
    expect(doc.version).toBe(2);
    expect((doc.graph as { nodes: unknown[] }).nodes.length).toBe(3);
  });
});
