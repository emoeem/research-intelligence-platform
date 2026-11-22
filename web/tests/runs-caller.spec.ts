import { type Page, test, expect } from "@playwright/test";

/** MA-4 Runs/Agents 页 caller 维度截图与渲染断言（web/tests/runs-caller.spec.ts，
 *  专用配置 web/playwright.runs.config.ts；计划 :48「截图/实测入库」+ :52
 *  「caller 维度列与过滤」双授权面）：
 *
 *  模式 A（默认，离线静态服务器 + route-mock，P5-10/P6-8 同模式）：
 *    - /runs 首屏（caller 徽标列在屏）/ caller 过滤态 / /runs/:id 详情
 *      caller 行 / /agents 运行记录 caller 列；
 *    - 截图落 web/qa/runs/*.png（≥4 帧入库交付）；
 *    - 断言：徽标/下拉/详情行在屏、过滤请求带 ?caller=、零 console.error。
 *
 *  模式 B（RP_P6_E2E=1，真实后端，selftest_ma4.sh §6 调用）：baseURL 仍由
 *  visual-server 提供 SPA，/api/** 代理到 RP_P6_BASE_URL 真实 rp_server
 *  （RP_P6_TOKEN 注入鉴权，plugins.spec.ts 模式 B 同款），真实库水位行
 *  caller 徽标实测截图（027 库旧库不伪造——列表行按库水位条件渲染）。
 *
 *  运行：npx playwright test tests/runs-caller.spec.ts -c playwright.runs.config.ts */

const RUNS_STUB = {
  available: true,
  source: "aux",
  total: 3,
  returned: 3,
  filters: { status: null, tool_id: null, caller: null },
  items: [
    {
      run_id: "rn-ma4-a1",
      tool_id: "rip.kb.keyword_search",
      tool_version: "0.1.0",
      workspace_id: "ws-ma4",
      caller_id: "caller-ma4-a",
      status: "SUCCEEDED",
      duration_ms: 1200,
      created_at: "2026-11-22T08:00:00Z",
      finished_at: "2026-11-22T08:00:01Z",
    },
    {
      run_id: "rn-ma4-a2",
      tool_id: "rip.kb.keyword_search",
      workspace_id: "ws-ma4",
      caller_id: "caller-ma4-a",
      status: "RUNNING",
      created_at: "2026-11-22T08:01:00Z",
    },
    {
      run_id: "rn-ma4-b1",
      tool_id: "rip.kb.rtis",
      workspace_id: "ws-ma4",
      caller_id: "caller-ma4-b",
      status: "SUCCEEDED",
      duration_ms: 2400,
      created_at: "2026-11-22T08:02:00Z",
      finished_at: "2026-11-22T08:02:02Z",
    },
  ],
};

const RUN_DETAIL_STUB = {
  available: true,
  source: "aux",
  run: {
    run_id: "rn-ma4-a1",
    tool_id: "rip.kb.keyword_search",
    tool_version: "0.1.0",
    workspace_id: "ws-ma4",
    caller_id: "caller-ma4-a",
    status: "SUCCEEDED",
    duration_ms: 1200,
    input_hash: "ab".repeat(32),
    created_at: "2026-11-22T08:00:00Z",
    finished_at: "2026-11-22T08:00:01Z",
  },
  artifacts: [],
  artifacts_note: "artifact 表不存在（对应迁移未应用），返回空列表",
  correlation: {
    trace_id: "wfr-ma4-a1",
    task_id: null,
    agent_run_id: null,
    tool_run_id: "rn-ma4-a1",
    artifact_ids: [],
    evidence_ids: [],
  },
};

const AGENTS_STUB = {
  available: true,
  total: 1,
  agents: 1,
  items: [
    {
      agent_id: "agent.ma4.selftest",
      version: "0.1.0",
      name: "MA-4 验收 Agent",
      context_policy: {},
      capability_refs: ["rip.kb.keyword_search"],
      created_at: "2026-11-22T07:00:00Z",
      updated_at: "2026-11-22T07:00:00Z",
    },
  ],
};

const AGENT_RUNS_STUB = {
  available: true,
  source: "main",
  total: 2,
  returned: 2,
  filters: { state: null, agent_id: null, caller: null },
  items: [
    {
      run_id: "arn-ma4-1",
      agent_id: "agent.ma4.selftest",
      agent_version: "0.1.0",
      state: "SUCCEEDED",
      trace_id: "wfr-ma4-a1",
      caller_id: "caller-ma4-a",
      steps_count: 2,
      created_at: "2026-11-22T08:00:00Z",
    },
    {
      run_id: "arn-ma4-2",
      agent_id: "agent.ma4.selftest",
      agent_version: "0.1.0",
      state: "RUNNING",
      caller_id: "caller-ma4-b",
      steps_count: 1,
      created_at: "2026-11-22T08:01:00Z",
    },
  ],
};

const RUNTIME_STUB = {
  available: false,
  reason: "aux_db_not_configured",
  message: "未配置 RP_AUX_DB：runtime 面真实空态（stub 不影响 Runs/Agents 断言）",
  total: 0,
  items: [],
};

const e2e =
  (globalThis as { process?: { env?: Record<string, string | undefined> } }).process?.env
    ?.RP_P6_E2E === "1";

async function mockApis(page: Page): Promise<void> {
  await page.route("**/api/**", (route) => {
    const url = new URL(route.request().url());
    const path = url.pathname;
    let body: unknown = {};
    if (path === "/api/runs" && url.searchParams.has("run_id")) body = RUN_DETAIL_STUB;
    else if (path === "/api/runs") {
      // ?caller= 服务端过滤语义同型（等值子集 + filters 回显）。
      const callerParam = url.searchParams.get("caller") ?? "";
      const items = callerParam
        ? RUNS_STUB.items.filter((r) => r.caller_id === callerParam)
        : RUNS_STUB.items;
      body = {
        ...RUNS_STUB,
        total: items.length,
        returned: items.length,
        filters: { status: null, tool_id: null, caller: callerParam || null },
        items,
      };
    } else if (path === "/api/agents") body = AGENTS_STUB;
    else if (path === "/api/agent-runs") body = AGENT_RUNS_STUB;
    else if (path === "/api/runtime") body = RUNTIME_STUB;
    route.fulfill({
      contentType: "application/json; charset=utf-8",
      body: JSON.stringify(body),
    });
  });
}

async function proxiedApis(page: Page): Promise<void> {
  const proc = (globalThis as { process?: { env?: Record<string, string | undefined> } }).process;
  const backend = proc?.env?.RP_P6_BASE_URL ?? "";
  const token = proc?.env?.RP_P6_TOKEN ?? "";
  await page.route("**/api/**", (route) => {
    const url = new URL(route.request().url());
    const apiPath = url.pathname + url.search;
    const headers = { ...route.request().headers(), Authorization: `Bearer ${token}` };
    route.continue({ url: `${backend}${apiPath}`, headers });
  });
}

test.describe("MA-4 Runs/Agents caller 维度（离线 mock）", () => {
  test.skip(e2e, "真实后端模式下跳过 mock 矩阵");

  test("Runs 首屏 caller 徽标列 + 过滤态 + 详情 caller 行 + Agents caller 列（截图 ≥4 帧）", async ({
    page,
  }) => {
    await page.setViewportSize({ width: 1440, height: 900 });
    const consoleErrors: string[] = [];
    const pageErrors: string[] = [];
    page.on("console", (msg) => {
      if (msg.type() === "error") consoleErrors.push(msg.text());
    });
    page.on("pageerror", (err) => pageErrors.push(String(err)));
    await mockApis(page);

    // 1) Runs 首屏：caller 徽标列在屏（2 caller × 行水位渲染）。
    await page.goto("/runs");
    await page.waitForLoadState("domcontentloaded");
    await expect(page.getByTestId("run-caller-badge")).toHaveCount(3);
    await expect(page.getByTestId("caller-filter")).toBeVisible();
    await page.screenshot({ path: "qa/runs/runs-page.png", fullPage: true });

    // 2) caller 过滤态：下拉选 caller-ma4-b → 服务端 ?caller= 参数 + 行收缩。
    await page.getByTestId("caller-filter").selectOption("caller-ma4-b");
    await expect(page.getByTestId("run-caller-badge")).toHaveCount(1);
    await expect(page.getByTestId("run-caller-badge")).toHaveText("caller-ma4-b");
    await page.screenshot({ path: "qa/runs/runs-filtered.png", fullPage: true });

    // 3) Run 详情：概要区 caller 行在屏。
    await page.goto("/runs/rn-ma4-a1");
    await page.waitForLoadState("domcontentloaded");
    await expect(page.getByText("caller", { exact: true })).toBeVisible();
    await expect(page.getByText("caller-ma4-a").first()).toBeVisible();
    await page.screenshot({ path: "qa/runs/run-detail.png", fullPage: true });

    // 4) Agents 页运行记录区：caller 徽标列在屏。
    await page.goto("/agents");
    await page.waitForLoadState("domcontentloaded");
    await expect(page.getByTestId("agents-runs-section")).toBeVisible();
    await expect(page.getByTestId("agent-run-caller-badge")).toHaveCount(2);
    await page.screenshot({ path: "qa/runs/agents-page.png", fullPage: true });

    expect(consoleErrors, `console.error: ${consoleErrors}`).toEqual([]);
    expect(pageErrors, `pageerror: ${pageErrors}`).toEqual([]);
  });
});

test.describe("MA-4 Runs/Agents caller 维度（RP_P6_E2E=1 真实后端）", () => {
  test.skip(!e2e, "默认离线模式跳过（selftest_ma4.sh §6 以 RP_P6_E2E=1 调用）");

  test("真实库水位 caller 徽标实测 + 截图入库", async ({ page }) => {
    await page.setViewportSize({ width: 1440, height: 900 });
    await proxiedApis(page);

    await page.goto("/runs");
    await page.waitForLoadState("domcontentloaded");
    await expect(page.getByTestId("caller-filter")).toBeVisible();
    // 真实库水位：027 库行含 caller_id（selftest_ma4 夹具预置），旧库行不伪造。
    await page.screenshot({ path: "qa/runs/e2e-runs-page.png", fullPage: true });
    const badgeCount = await page.getByTestId("run-caller-badge").count();
    if (badgeCount > 0) {
      await page.getByTestId("caller-filter").selectOption({ index: 1 });
      await page.waitForTimeout(500);
    }
    await page.screenshot({ path: "qa/runs/e2e-runs-filtered.png", fullPage: true });
  });
});
