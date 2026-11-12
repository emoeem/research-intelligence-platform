import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { App } from "../src/App";

/** P7-5 冒烟：Trace 关联链页（/traces?id=）：六 ID 任一入参汇出五类链
 *  （任务/意图/上下文/工具/证据 + 审计旁路面）、命中面徽标与计数摘要、
 *  空链真实空态说明、未带 id 引导文案。 */

const TRACE_CHAIN_STUB = {
  available: true,
  query_id: "tr-p75-e2e",
  trace_ids: ["tr-p75-e2e"],
  matched_via: "trace_id",
  counts: {
    tasks: 1,
    agent_runs: 1,
    manifests: 1,
    context_items: 3,
    tool_runs: 1,
    artifacts: 1,
    evidence: 1,
    audit_events: 4,
  },
  tasks: [
    {
      task_id: "rtis-20261113-010203-001",
      title: "短信发送能力核查",
      state: "COMPLETED",
      task_type: "recon",
    },
  ],
  agent_runs: [
    {
      run_id: "arn-p75-e2e",
      state: "SUCCEEDED",
      agent_id: "agent.research.minimal",
      agent_version: "0.1.0",
    },
  ],
  manifests: [
    { manifest_id: "ctxm-1", intent_status: "ok", task_id: "rtis-20261113-010203-001" },
  ],
  context_items: [
    { manifest_id: "ctxm-1", item_id: "ki-1", layer: "L1", kind: "knowledge", included: "full" },
    { manifest_id: "ctxm-1", item_id: "tb-1", layer: "L2", kind: "tool", included: "summary" },
  ],
  tool_runs: [
    { run_id: "rn-p75-e2e", tool_id: "rip.kb.keyword_search", status: "SUCCEEDED" },
  ],
  artifacts: [
    { artifact_id: "art-p75-e2e", role: "primary", mime_type: "application/json" },
  ],
  evidence: [
    { evidence_id: "ev-p75-e2e", evidence_type: "log", task_ref: "rtis-20261113-010203-001" },
  ],
  audit_events: [
    { event_id: 1, action: "agent.run.accepted", actor: "agent-engine:agent.research.minimal" },
    { event_id: 2, action: "agent.step.completed", actor: "agent-engine:agent.research.minimal" },
    { event_id: 3, action: "agent.run.finished", actor: "agent-engine:agent.research.minimal" },
  ],
};

function stubFetch(body: unknown): void {
  vi.stubGlobal(
    "fetch",
    vi.fn(async () => {
      return new Response(JSON.stringify(body), {
        status: 200,
        headers: { "content-type": "application/json; charset=utf-8" },
      });
    }),
  );
}

beforeEach(() => {
  stubFetch(TRACE_CHAIN_STUB);
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
  document.body.innerHTML = "";
});

describe("Trace 关联链页（P7-5）", () => {
  it("六 ID 入参汇出五类链全段：任务/意图/上下文/工具/证据 + 审计与命中面", async () => {
    window.history.pushState({}, "", "/traces?id=tr-p75-e2e");
    render(<App />);
    expect(await screen.findByText("tr-p75-e2e")).toBeTruthy();
    expect(screen.getByText("trace_id")).toBeTruthy();
    // 五类链段标题与行数
    expect(screen.getByText(/任务（research_task）/)).toBeTruthy();
    expect(screen.getByText(/Agent 运行（agent_run）/)).toBeTruthy();
    expect(screen.getByText(/意图清单（context_manifest/)).toBeTruthy();
    expect(screen.getByText(/上下文条目（context_manifest_item）/)).toBeTruthy();
    expect(screen.getByText(/工具运行（tool_run）/)).toBeTruthy();
    expect(screen.getByText(/产物（artifact）/)).toBeTruthy();
    expect(screen.getByText(/证据（evidence）/)).toBeTruthy();
    expect(screen.getByText(/审计事件（audit_event/)).toBeTruthy();
    // 链内 id 均可点击跳转 /traces?id=
    for (const id of [
      "arn-p75-e2e",
      "rn-p75-e2e",
      "art-p75-e2e",
      "ev-p75-e2e",
      "ctxm-1",
      "ki-1",
    ]) {
      expect(
        screen.getByRole("link", { name: id }).getAttribute("href"),
      ).toBe(`/traces?id=${id}`);
    }
    // 摘要计数
    expect(screen.getByText(/tool_runs 1/)).toBeTruthy();
    expect(screen.getByText(/evidence 1/)).toBeTruthy();
  });

  it("空链渲染真实空态说明（不伪造）", async () => {
    stubFetch({
      available: true,
      query_id: "no-such",
      trace_ids: [],
      message: "no-such 未解析到任何 trace_id（六 ID 方向均未命中）",
      counts: {},
    });
    window.history.pushState({}, "", "/traces?id=no-such");
    render(<App />);
    expect(await screen.findByText(/no-such 未解析到任何 trace_id/)).toBeTruthy();
  });

  it("未带 id 渲染引导文案", async () => {
    window.history.pushState({}, "", "/traces");
    render(<App />);
    expect(await screen.findByText(/需 \?id=<关联 ID>/)).toBeTruthy();
  });
});
