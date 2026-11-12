import { cleanup, render, screen, fireEvent } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { App } from "../src/App";

/** P7-7 冒烟：Agent Studio 实页三区（/agents）：
 *  1. Agent 定义与能力绑定（定义列表 + ?id= 详情能力绑定对账与运行摘要）；
 *  2. 运行记录与步骤轨迹回放（run 列表 + 详情步骤 seq 升序与六 ID 关联）；
 *  3. Task/Evidence/Research Memory 视图（013 三记忆表行字段在场）。
 *  六 ID 跳转断言：trace/task/artifact/evidence 跳 /traces?id=，
 *  tool_run_id 跳 /runs/<id>（RunDetail，消费 P7-5 关联面）。 */

const AGENT_ID = "agent.research.minimal";
const RUN_ID = "arn-p77-e2e";
const TRACE_ID = "tr-p77-e2e";
const TASK_ID = "rtis-20261113-010203-001";
const TOOL_RUN_ID = "rn-p77-e2e";
const ARTIFACT_ID = "art-p77-e2e";
const EVIDENCE_ID = "ev-p77-e2e";

const AGENTS_LIST_STUB = {
  available: true,
  total: 1,
  agents: 1,
  returned: 1,
  source: "aux",
  items: [
    {
      agent_id: AGENT_ID,
      version: "0.1.0",
      name: "研究最小 Agent",
      capability_refs: [{ tool_id: "rip.kb.keyword_search", version: "0.1.0" }],
      created_at: "2026-11-13T00:00:00Z",
      updated_at: "2026-11-13T00:00:00Z",
    },
  ],
  runs_total: 1,
  runs_by_state: { SUCCEEDED: 1 },
};

const AGENTS_DETAIL_STUB = {
  available: true,
  agent_id: AGENT_ID,
  definitions: AGENTS_LIST_STUB.items,
  tool_bindings: [
    {
      tool_id: "rip.kb.keyword_search",
      version: "0.1.0",
      registered: true,
      lifecycle: "ENABLED",
      governance: "ACTIVE",
    },
  ],
  runs: [
    {
      run_id: RUN_ID,
      agent_id: AGENT_ID,
      agent_version: "0.1.0",
      state: "SUCCEEDED",
      trace_id: TRACE_ID,
      steps_count: 1,
      created_at: "2026-11-13T01:00:00Z",
    },
  ],
  runs_total: 1,
  runs_by_state: { SUCCEEDED: 1 },
};

const RUNS_LIST_STUB = {
  available: true,
  source: "aux",
  total: 1,
  returned: 1,
  items: [
    {
      run_id: RUN_ID,
      agent_id: AGENT_ID,
      agent_version: "0.1.0",
      state: "SUCCEEDED",
      trace_id: TRACE_ID,
      steps_count: 1,
      created_at: "2026-11-13T01:00:00Z",
    },
  ],
};

const RUN_DETAIL_STUB = {
  available: true,
  source: "aux",
  run: {
    run_id: RUN_ID,
    agent_id: AGENT_ID,
    agent_version: "0.1.0",
    state: "SUCCEEDED",
    trace_id: TRACE_ID,
    memory_refs: [`mem://memory/mem-r-abc`],
    created_at: "2026-11-13T01:00:00Z",
    started_at: "2026-11-13T01:00:01Z",
    finished_at: "2026-11-13T01:00:02Z",
  },
  steps: [
    {
      seq: 0,
      round: 0,
      tool_id: "rip.kb.keyword_search",
      tool_version: "0.1.0",
      tool_run_id: TOOL_RUN_ID,
      status: "SUCCEEDED",
      artifact_id: ARTIFACT_ID,
      evidence_id: EVIDENCE_ID,
      content_hash: "aa11bb22cc33dd44ee55ff6677889900aabbccddeeff00112233445566778899",
      summary: "检索 nmap 关键词命中 3 条知识",
      elapsed_ms: 210,
      input: { keyword: "nmap" },
    },
  ],
  memories: {
    task: [
      {
        memory_id: "mem-t-1",
        run_id: RUN_ID,
        round: 0,
        summary: "",
        lifecycle: "ACTIVE",
      },
    ],
    evidence: [
      {
        memory_id: "mem-e-1",
        run_id: RUN_ID,
        evidence_id: EVIDENCE_ID,
        artifact_id: ARTIFACT_ID,
        content_hash: "aa11bb22cc33dd44ee55ff6677889900aabbccddeeff00112233445566778899",
        lifecycle: "ACTIVE",
      },
    ],
    research: [
      {
        memory_id: "mem-r-1",
        run_id: RUN_ID,
        title: "nmap 端口扫描知识摘要",
        summary: "命中 3 条，含 SYN 扫描时序建议",
        refs: [
          { ref_type: "artifact", ref_id: ARTIFACT_ID },
          { ref_type: "evidence", ref_id: EVIDENCE_ID },
        ],
        lifecycle: "ACTIVE",
      },
    ],
  },
  control: { goal_text: "检索 nmap 端口扫描相关知识", retry_of_run_id: null },
  correlation: {
    trace_id: TRACE_ID,
    task_id: TASK_ID,
    agent_run_id: RUN_ID,
    tool_run_ids: [TOOL_RUN_ID],
    artifact_ids: [ARTIFACT_ID],
    evidence_ids: [EVIDENCE_ID],
  },
  counts: { steps: 1, task_memory: 1, evidence_memory: 1, research_memory: 1 },
};

const EMPTY_LISTS_STUB = {
  available: true,
  total: 0,
  agents: 0,
  returned: 0,
  source: "main",
  items: [],
  runs_total: 0,
  runs_by_state: {},
};

/** 按 pathname + query 分流 stub：?id= 详情与列表共用路径，必须以查询串区分。 */
function stubFetchByPath(respond: (pathname: string, search: string) => unknown): void {
  vi.stubGlobal(
    "fetch",
    vi.fn(async (input: RequestInfo | URL) => {
      const url = typeof input === "string" ? input : input instanceof URL ? input.href : input.url;
      const parsed = new URL(url, "http://localhost");
      return new Response(JSON.stringify(respond(parsed.pathname, parsed.search)), {
        status: 200,
        headers: { "content-type": "application/json; charset=utf-8" },
      });
    }),
  );
}

beforeEach(() => {
  stubFetchByPath((pathname, search) => {
    if (pathname === "/api/agents") {
      return search.includes("id=") ? AGENTS_DETAIL_STUB : AGENTS_LIST_STUB;
    }
    if (pathname === "/api/agent-runs") {
      return search.includes("id=") ? RUN_DETAIL_STUB : RUNS_LIST_STUB;
    }
    return {};
  });
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
  document.body.innerHTML = "";
});

describe("Agent Studio 实页三区（P7-7）", () => {
  it("三区段标题与定义列表字段在场：agent id、版本、能力绑定工具徽标", async () => {
    window.history.pushState({}, "", "/agents");
    render(<App />);
    // 固定 tab 与页头同名（Agent Studio），三区以 section testid 断言。
    expect(await screen.findByTestId("agents-defs-section")).toBeTruthy();
    expect(screen.getByTestId("agents-runs-section")).toBeTruthy();
    expect(screen.getByTestId("agents-memory-section")).toBeTruthy();
    expect(screen.getByText("Agent 定义与能力绑定")).toBeTruthy();
    expect(screen.getByText("运行记录与步骤轨迹回放")).toBeTruthy();
    expect(screen.getByText("Task / Evidence / Research Memory 视图")).toBeTruthy();
    // 定义列表字段
    expect(await screen.findByText(AGENT_ID)).toBeTruthy();
    expect(screen.getByText("研究最小 Agent")).toBeTruthy();
    expect(screen.getByText("rip.kb.keyword_search @ v0.1.0")).toBeTruthy();
    // 运行列表行（步骤计数与 trace 链接）
    expect(screen.getByText(RUN_ID)).toBeTruthy();
    expect(screen.getByText("1 步")).toBeTruthy();
    const traceLink = screen.getByRole("link", { name: TRACE_ID });
    expect(traceLink.getAttribute("href")).toBe(`/traces?id=${TRACE_ID}`);
    // 未选 run 时第三区显示引导文案（不伪造数据）
    expect(screen.getByText(/在上方第二区选择一条运行后/)).toBeTruthy();
  });

  it("选中 run 后步骤轨迹回放与六 ID 关联跳转在场（tool_run 跳 RunDetail，其余跳 /traces）", async () => {
    stubFetchByPath((pathname, search) => {
      if (pathname === "/api/agents") {
        return search.includes("id=") ? AGENTS_DETAIL_STUB : AGENTS_LIST_STUB;
      }
      if (pathname === "/api/agent-runs") {
        return search.includes("id=") ? RUN_DETAIL_STUB : RUNS_LIST_STUB;
      }
      return {};
    });
    window.history.pushState({}, "", "/agents");
    render(<App />);
    const runRow = (await screen.findAllByTestId("agent-run-list-row"))[0]!;
    fireEvent.click(runRow);
    // 概要区六 ID 关联面
    expect(await screen.findByText(/检索 nmap 端口扫描相关知识/)).toBeTruthy();
    expect(screen.getByText("agent_run")).toBeTruthy();
    expect(screen.getByText("tool_run")).toBeTruthy();
    const traceLinks = screen.getAllByRole("link", { name: TRACE_ID });
    expect(traceLinks.length).toBeGreaterThan(0);
    expect(traceLinks.every((el) => el.getAttribute("href") === `/traces?id=${TRACE_ID}`)).toBe(
      true,
    );
    const toolRunLink = screen.getByRole("link", { name: `tool_run ${TOOL_RUN_ID}` });
    expect(toolRunLink.getAttribute("href")).toBe(`/runs/${TOOL_RUN_ID}`);
    const artifactLinks = screen.getAllByRole("link", { name: ARTIFACT_ID });
    expect(artifactLinks.length).toBeGreaterThan(0);
    expect(
      artifactLinks.every((el) => el.getAttribute("href") === `/traces?id=${ARTIFACT_ID}`),
    ).toBe(true);
    const evidenceLink = screen.getAllByRole("link", { name: EVIDENCE_ID })[0]!;
    expect(evidenceLink.getAttribute("href")).toBe(`/traces?id=${EVIDENCE_ID}`);
    // 步骤轨迹回放（seq 升序、状态、入参快照摘要）
    expect(screen.getByText("步骤 0")).toBeTruthy();
    expect(screen.getAllByText(/rip.kb.keyword_search @ v0.1.0/).length).toBeGreaterThan(0);
    expect(screen.getAllByText("SUCCEEDED").length).toBeGreaterThan(0);
    expect(screen.getByText(/检索 nmap 关键词命中 3 条知识/)).toBeTruthy();
    expect(screen.getByText("210 ms")).toBeTruthy();
    expect(screen.getByText("计划入参快照")).toBeTruthy();
  });

  it("选中 run 后三类 Memory 视图行字段在场（Task/Evidence/Research，013）", async () => {
    stubFetchByPath((pathname, search) => {
      if (pathname === "/api/agents") {
        return search.includes("id=") ? AGENTS_DETAIL_STUB : AGENTS_LIST_STUB;
      }
      if (pathname === "/api/agent-runs") {
        return search.includes("id=") ? RUN_DETAIL_STUB : RUNS_LIST_STUB;
      }
      return {};
    });
    window.history.pushState({}, "", "/agents");
    render(<App />);
    fireEvent.click((await screen.findAllByTestId("agent-run-list-row"))[0]!);
    expect(await screen.findByText("mem-t-1")).toBeTruthy();
    expect(screen.getByText("mem-e-1")).toBeTruthy();
    expect(screen.getByText("mem-r-1")).toBeTruthy();
    expect(screen.getByText(/Task Memory（agent_task_memory/)).toBeTruthy();
    expect(screen.getByText(/Evidence Memory（agent_evidence_memory/)).toBeTruthy();
    expect(screen.getByText(/Research Memory（agent_research_memory/)).toBeTruthy();
    expect(screen.getByText("nmap 端口扫描知识摘要")).toBeTruthy();
    expect(screen.getByText("2 条回链")).toBeTruthy();
  });

  it("选中 agent 后详情面板：能力绑定对账与运行摘要在场", async () => {
    stubFetchByPath((pathname, search) => {
      if (pathname === "/api/agents") {
        return search.includes("id=") ? AGENTS_DETAIL_STUB : AGENTS_LIST_STUB;
      }
      if (pathname === "/api/agent-runs") return RUNS_LIST_STUB;
      return {};
    });
    window.history.pushState({}, "", "/agents");
    render(<App />);
    fireEvent.click(await screen.findByTestId("agent-definition-row"));
    expect(await screen.findByText("能力绑定对账（tool 注册面）")).toBeTruthy();
    expect(screen.getByText("已注册 · ENABLED")).toBeTruthy();
    expect(screen.getByText("运行摘要")).toBeTruthy();
    expect(screen.getAllByText(/SUCCEEDED 1/).length).toBeGreaterThan(0);
  });

  it("空定义与空运行列表渲染真实空态说明（不伪造）", async () => {
    stubFetchByPath((pathname, search) => {
      if (pathname === "/api/agents" && !search.includes("id=")) return EMPTY_LISTS_STUB;
      if (pathname === "/api/agent-runs" && !search.includes("id=")) {
        return { ...EMPTY_LISTS_STUB, message: "暂无 AgentRun" };
      }
      return {};
    });
    window.history.pushState({}, "", "/agents");
    render(<App />);
    expect(await screen.findByText(/暂无 Agent 定义/)).toBeTruthy();
    expect(screen.getByText(/暂无 AgentRun/)).toBeTruthy();
  });
});
