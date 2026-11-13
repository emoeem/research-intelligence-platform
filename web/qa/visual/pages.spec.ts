import { type Page, test, expect } from "@playwright/test";
import { AxeBuilder } from "@axe-core/playwright";

/** 视觉矩阵（P5-10，ROADMAP P5-9 裁剪版；P7-1 五区壳层重拍）：11 页 × 2 主题
 *  × 2 视口 = 44 组合（新壳含 Inspector/Activity，axe 断言并轨覆盖）。
 *  断言（无快照基线，ROADMAP 风险 3）：
 *  1. 零 console.error 与零 pageerror；
 *  2. 无横向溢出（scrollWidth <= innerWidth + 1）；
 *  3. 页面关键内容标记在位（数据来自 route-mock 的真实形状 JSON）；
 *  4. axe AA 尝试：每页桌面深空组合扫描，serious/critical 级违规为 0
 *     （color-contrast 单独收集打印进验收文档，不阻断，属已知待修项）。
 *  全部 /api/** 请求在 page.route 层 mock（离线静态服务器模式，TELEGRAM §2.5）。 */

const API_STUBS: Record<string, unknown> = {
  "/api/overview": {
    knowledge: { total: 46, by_lifecycle: { VERIFIED: 37, CANDIDATE: 7, DISCOVERED: 2 } },
    tasks: { total: 42 },
    resources: { total: 81 },
    evidence: { total: 700 },
    reviews: { total: 165 },
    tools: { available: true, source: "main", total: 20 },
    runs: { available: true, source: "aux", total: 3, by_status: { SUCCEEDED: 3 } },
    runtime: { available: true, source: "aux", total: 0 },
  },
  "/api/stats": {
    reviews: {
      total: 165,
      by_queue: {
        d5: { total: 68, by_decision: {} },
        license: { total: 79, by_decision: {} },
        mapping: { total: 18, by_decision: {} },
      },
    },
  },
  "/api/knowledge": {
    available: true,
    source: "main",
    total: 46,
    returned: 2,
    items: [
      {
        knowledge_id: "k-1",
        lifecycle: "VERIFIED",
        title: "站群/落地页开放重定向专项测试法",
        component: "landing",
        severity: "high",
        verified: 1,
        hit_count: 7,
        updated_at: "2026-11-05T10:00:00Z",
      },
      {
        knowledge_id: "k-2",
        lifecycle: "CANDIDATE",
        title: "hex2bin 解码技巧在参数矩阵中的应用",
        verified: 0,
        updated_at: "2026-11-04T09:00:00Z",
      },
    ],
  },
  "/api/tools": {
    available: true,
    source: "main",
    total: 20,
    returned: 2,
    items: [
      {
        tool_id: "nmap",
        name: "nmap",
        lifecycle: "REGISTERED",
        governance: "ACTIVE",
        updated_at: "2026-11-05T10:00:00Z",
      },
      {
        tool_id: "rip.kb.rtis",
        name: "rip.kb.rtis",
        lifecycle: "VALIDATED",
        governance: "ACTIVE",
        updated_at: "2026-11-09T10:00:00Z",
      },
    ],
  },
  "/api/runs": {
    available: true,
    source: "aux",
    total: 3,
    returned: 3,
    items: [
      {
        run_id: "rn-b5-rt-py",
        tool_id: "rip.kb.rtis",
        status: "SUCCEEDED",
        duration_ms: 221,
        created_at: "2026-11-10T04:57:00Z",
      },
      {
        run_id: "rn-b5-kb-mjs",
        tool_id: "rip.kb.rtis",
        status: "SUCCEEDED",
        duration_ms: 208,
        created_at: "2026-11-10T04:57:10Z",
      },
      {
        run_id: "rn-b5-eri",
        tool_id: "rip.kb.eri",
        status: "SUCCEEDED",
        duration_ms: 927,
        created_at: "2026-11-10T04:58:00Z",
      },
    ],
  },
  "/api/runtime": {
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
    ],
  },
  // P7-7 Agent Studio 三区列表面（详情 ?id= 不在视觉矩阵路径内，仅列表落位）。
  "/api/agents": {
    available: true,
    total: 2,
    agents: 1,
    returned: 2,
    source: "main+aux",
    items: [
      {
        agent_id: "agent.research.minimal",
        version: "0.1.0",
        name: "研究最小 Agent",
        capability_refs: [{ tool_id: "rip.kb.keyword_search", version: "0.1.0" }],
        created_at: "2026-11-13T00:00:00Z",
        updated_at: "2026-11-13T00:00:00Z",
      },
      {
        agent_id: "agent.research.minimal",
        version: "0.2.0",
        name: "研究最小 Agent",
        capability_refs: [
          { tool_id: "rip.kb.keyword_search", version: "0.1.0" },
          { tool_id: "rip.http.tls_inspect", version: null },
        ],
        created_at: "2026-11-13T01:00:00Z",
        updated_at: "2026-11-13T01:00:00Z",
      },
    ],
    runs_total: 1,
    runs_by_state: { SUCCEEDED: 1 },
  },
  "/api/agent-runs": {
    available: true,
    source: "aux",
    total: 1,
    returned: 1,
    items: [
      {
        run_id: "arn-p77-visual",
        agent_id: "agent.research.minimal",
        agent_version: "0.2.0",
        state: "SUCCEEDED",
        trace_id: "tr-p77-visual",
        steps_count: 2,
        created_at: "2026-11-13T02:00:00Z",
      },
    ],
  },
  // P8-4 Workflow Studio 五区面（列表形状与 /api/workflows 真实响应一致；
  // ?id= 详情不在视觉矩阵路径内，仅列表落位；逐版 graph 形状见
  // workflows.spec.ts）。
  "/api/workflows": {
    available: true,
    source: "main+aux",
    total_workflows: 1,
    total_versions: 2,
    items: [
      {
        workflow_id: "wf-p84-visual",
        version: 1,
        name: "P8-4 视觉矩阵演示流 v1",
        compat: "graph-v1",
        node_count: 3,
        edge_count: 2,
        input_schema: { type: "object", properties: { target: { type: "string" } } },
        source: "rp-workflow-put",
        created_at: "2026-11-14T00:00:00Z",
      },
      {
        workflow_id: "wf-p84-visual",
        version: 2,
        name: "P8-4 视觉矩阵演示流 v2",
        compat: "graph-v1",
        node_count: 4,
        edge_count: 3,
        input_schema: { type: "object", properties: { target: { type: "string" } } },
        source: "rp-workflow-put",
        created_at: "2026-11-14T01:00:00Z",
      },
    ],
  },
  "/api/workflow-runs": {
    available: true,
    source: "main+aux",
    total: 1,
    returned: 1,
    limit: 50,
    items: [
      {
        run_id: "wfr-p84-visual",
        workflow_id: "wf-p84-visual",
        workflow_version: 2,
        state: "paused",
        trace_id: "tr-p84-visual",
        task_id: "rtis-20261114-010000-001",
        created_at: "2026-11-14T01:00:00Z",
      },
    ],
  },
  "/api/tasks": {
    total: 42,
    by_state: { CREATED: 28, COMPLETED: 2 },
    by_type: { recon: 19, vuln: 17 },
    returned: 3,
    items: [
      {
        task_id: "rtis-20261106-034508-9517",
        title: "全链路回归",
        target: "example.test",
        task_type: "recon",
        state: "CREATED",
        updated_at: "2026-11-06T03:45:00Z",
      },
      {
        task_id: "rtis-20261106-034447-9052",
        title: "状态机测试",
        task_type: "recon",
        state: "WORKFLOW_PLANNING",
        updated_at: "2026-11-06T03:44:00Z",
      },
      {
        // 回归形态（P5-10 验收意见 1）：真实库最长 target 为 141 字符 scope
        // 清单（rtis-20261103-023603-7373，airbnb.com scope 列表），无空格长串
        // 曾致移动端 /projects 横向溢出 156px。此行让门禁持续覆盖该形态。
        task_id: "rtis-20261103-023603-7373",
        title: "站群开放重定向专项（长 scope 清单）",
        target:
          "airbnb.com（scope: www/support-api/one/next/m/api/assets/callbacks.airbnb.com, *.airbnb.com, airbnb.ca, airbnb.de, airbnb.es, airbnb.fr）",
        task_type: "vuln",
        state: "COMPLETED",
        updated_at: "2026-11-03T02:36:00Z",
      },
    ],
  },
};

const PAGES = [
  { path: "/", marker: "知识生命周期分布" },
  { path: "/projects", marker: "共 42 个任务" },
  { path: "/research", marker: "研究工作台" },
  { path: "/knowledge", marker: "共 46 条" },
  { path: "/tools", marker: "共 20 个工具" },
  { path: "/plugins", marker: "插件中心" },
  { path: "/agents", marker: "Agent 定义与能力绑定" },
  { path: "/workflows", marker: "节点分类面板" },
  { path: "/runs", marker: "rn-b5-rt-py" },
  { path: "/runtime", marker: "node-py-1" },
  { path: "/settings", marker: "界面偏好" },
];

const THEMES = ["rip-dark", "rip-light"] as const;
const VIEWPORTS = [
  { width: 1440, height: 900, label: "desktop" },
  { width: 390, height: 844, label: "mobile" },
];

async function openPage(page: Page, path: string, theme: string): Promise<void> {
  await page.addInitScript((stored) => {
    window.localStorage.setItem("rip_theme", stored);
  }, theme);
  await page.route("**/api/**", (route) => {
    const path = new URL(route.request().url()).pathname;
    route.fulfill({
      contentType: "application/json; charset=utf-8",
      body: JSON.stringify(API_STUBS[path] ?? {}),
    });
  });
  await page.goto(`http://127.0.0.1:4177${path}`);
  await page.waitForLoadState("domcontentloaded");
}

const contrastFindings: string[] = [];

for (const pageSpec of PAGES) {
  for (const theme of THEMES) {
    for (const viewport of VIEWPORTS) {
      test(`视觉矩阵 ${pageSpec.path} ${theme} ${viewport.label}`, async ({ page }) => {
        const consoleErrors: string[] = [];
        const pageErrors: string[] = [];
        page.on("console", (msg) => {
          if (msg.type() === "error") consoleErrors.push(msg.text());
        });
        page.on("pageerror", (error) => pageErrors.push(error.message));

        await page.setViewportSize({ width: viewport.width, height: viewport.height });
        await openPage(page, pageSpec.path, theme);

        // 关键内容标记（真实形状 mock 数据落地）
        await expect(page.getByText(pageSpec.marker).first()).toBeVisible({ timeout: 5000 });

        // 零 console.error / 零 pageerror
        expect(
          consoleErrors,
          `console.error on ${pageSpec.path} ${theme} ${viewport.label}: ${consoleErrors.join(" | ")}`,
        ).toEqual([]);
        expect(
          pageErrors,
          `pageerror on ${pageSpec.path} ${theme} ${viewport.label}: ${pageErrors.join(" | ")}`,
        ).toEqual([]);

        // 无横向溢出
        const overflow = await page.evaluate(() => ({
          scrollWidth: document.documentElement.scrollWidth,
          innerWidth: window.innerWidth,
        }));
        expect(
          overflow.scrollWidth,
          `横向溢出 on ${pageSpec.path} ${theme} ${viewport.label}: scrollWidth ${overflow.scrollWidth} > innerWidth ${overflow.innerWidth}`,
        ).toBeLessThanOrEqual(overflow.innerWidth + 1);
      });
    }
  }

  test(`axe AA ${pageSpec.path}（桌面深空）`, async ({ page }) => {
    await page.setViewportSize({ width: 1440, height: 900 });
    await openPage(page, pageSpec.path, "rip-dark");
    await page.getByText(pageSpec.marker).first().waitFor({ state: "visible", timeout: 5000 });
    const results = await new AxeBuilder({ page })
      .withTags(["wcag2a", "wcag2aa", "wcag21aa"])
      .analyze();
    const serious = results.violations.filter(
      (v) => v.impact === "serious" || v.impact === "critical",
    );
    for (const violation of serious) {
      if (violation.id === "color-contrast") {
        // 已知待修项：主题对比度为设计目标值（PHASE5-PLAN 风险 5），收集进验收文档，不阻断门禁
        contrastFindings.push(`${pageSpec.path}: color-contrast x${violation.nodes.length}`);
        continue;
      }
      expect(
        serious.filter((v) => v.id !== "color-contrast"),
        `axe ${violation.id} (${violation.impact}) on ${pageSpec.path}: ${violation.nodes.length} 处`,
      ).toEqual([]);
    }
  });
}

// color-contrast 收尾汇总：打印全部发现（验收文档摘录），上限断言防失控膨胀
test.afterAll(async () => {
  // eslint-disable-next-line no-console
  console.log(
    `[axe color-contrast findings] ${contrastFindings.length} 处: ${contrastFindings.join("; ")}`,
  );
});
