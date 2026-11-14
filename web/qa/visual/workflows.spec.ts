import { type Page, test, expect } from "@playwright/test";
import { AxeBuilder } from "@axe-core/playwright";

/** /workflows 专项视觉与交互门禁（P8-4；P10-4 基线与阻断化）：在 pages.spec.ts
 *  的 11 页矩阵之外对 Workflow Studio 画布页加专项断言：
 *  1. 五区落位（面板/画布/Inspector/底部 Run/版本区）与画布节点 DOM 计数；
 *  2. 画布主题覆盖（深空/浅色）零 console.error、零 pageerror、无横向溢出，
 *     并落 viewport 级视觉快照基线（qa/visual/baseline/，统一字体环境同
 *     pages.spec 口径）；
 *  3. 移动端只读降级徽标（<768px matchMedia）；
 *  4. axe AA 桌面（深空 + 晨白）扫描（serious/critical = 0，color-contrast
 *     已清零并与 pages.spec 同口径阻断）。
 *  全部 /api/** 请求在 page.route 层 mock（离线静态服务器模式）。 */

const WORKFLOW_ID = "wf-p84-visual";

const WORKFLOWS_STUB = {
  available: true,
  source: "main+aux",
  total_workflows: 1,
  total_versions: 2,
  items: [
    {
      workflow_id: WORKFLOW_ID,
      version: 1,
      name: "P8-4 视觉矩阵演示流 v1",
      compat: "graph-v1",
      node_count: 3,
      edge_count: 2,
      input_schema: { type: "object", properties: { target: { type: "string" } } },
      source: "rp-workflow-put",
      created_at: "2026-11-14T00:00:00Z",
      graph: {
        nodes: [
          { id: "start", type: "start" },
          { id: "fetch", type: "tool", params: { tool_id: "rip.kb.keyword_search" } },
          { id: "done", type: "end" },
        ],
        edges: [
          { from: "start", to: "fetch" },
          { from: "fetch", to: "done" },
        ],
      },
    },
    {
      workflow_id: WORKFLOW_ID,
      version: 2,
      name: "P8-4 视觉矩阵演示流 v2",
      compat: "graph-v1",
      node_count: 4,
      edge_count: 3,
      input_schema: { type: "object", properties: { target: { type: "string" } } },
      source: "rp-workflow-put",
      created_at: "2026-11-14T01:00:00Z",
      graph: {
        nodes: [
          { id: "start", type: "start" },
          { id: "retrieve", type: "knowledge", params: { query: "$/target" } },
          { id: "review", type: "human", params: { prompt: "是否批准外部访问?", key: "approval" } },
          { id: "done", type: "end" },
        ],
        edges: [
          { from: "start", to: "retrieve" },
          { from: "retrieve", to: "review" },
          { from: "review", to: "done" },
        ],
      },
    },
  ],
};

const RUNS_STUB = {
  available: true,
  source: "main+aux",
  total: 1,
  returned: 1,
  limit: 50,
  items: [
    {
      run_id: "wfr-p84-visual",
      workflow_id: WORKFLOW_ID,
      workflow_version: 2,
      state: "paused",
      trace_id: "tr-p84-visual",
      task_id: "rtis-20261114-010000-001",
      created_at: "2026-11-14T01:00:00Z",
    },
  ],
};

async function openPage(page: Page, theme: string): Promise<void> {
  await page.addInitScript((stored) => {
    window.localStorage.setItem("rip_theme", stored);
  }, theme);
  await page.route("**/api/**", (route) => {
    const path = new URL(route.request().url()).pathname;
    const body =
      path === "/api/workflows" ? WORKFLOWS_STUB : path === "/api/workflow-runs" ? RUNS_STUB : {};
    route.fulfill({ contentType: "application/json; charset=utf-8", body: JSON.stringify(body) });
  });
  await page.goto("http://127.0.0.1:4177/workflows");
  await page.waitForLoadState("domcontentloaded");
  // 统一字体环境（P10-4）：与 pages.spec.ts 同一口径，快照钉死单一 CJK 字族
  await page.addStyleTag({
    content:
      ':root { --font-sans: "Noto Sans CJK SC", sans-serif !important; --font-mono: "Noto Sans Mono CJK SC", monospace !important; }',
  });
  // 列表落位后选中 workflow（与真实用户路径一致），画布 graph 随详情装载
  await expect(page.getByTestId("workflow-id-select")).toBeVisible({ timeout: 5000 });
  await page.getByTestId("workflow-id-select").fill(WORKFLOW_ID);
  await expect(page.getByTestId("version-chip-2")).toBeVisible({ timeout: 5000 });
}

async function assertClean(page: Page, label: string): Promise<void> {
  const overflow = await page.evaluate(() => ({
    scrollWidth: document.documentElement.scrollWidth,
    innerWidth: window.innerWidth,
  }));
  expect(
    overflow.scrollWidth,
    `横向溢出 on ${label}: scrollWidth ${overflow.scrollWidth} > innerWidth ${overflow.innerWidth}`,
  ).toBeLessThanOrEqual(overflow.innerWidth + 1);
}

test("画布桌面深空：五区落位 + 画布节点/柄计数 + 零 console 错误", async ({ page }) => {
  const consoleErrors: string[] = [];
  const pageErrors: string[] = [];
  page.on("console", (msg) => {
    if (msg.type() === "error") consoleErrors.push(msg.text());
  });
  page.on("pageerror", (error) => pageErrors.push(error.message));
  await page.setViewportSize({ width: 1440, height: 900 });
  await openPage(page, "rip-dark");
  for (const testid of [
    "workflows-palette-section",
    "workflows-canvas-section",
    "workflows-inspector-section",
    "workflows-run-section",
    "workflows-versions-section",
  ]) {
    await expect(page.getByTestId(testid)).toBeVisible();
  }
  // 画布装载 v2 graph（4 节点 6 柄），运行列表 1 条
  await expect(page.locator(".react-flow__node")).toHaveCount(4, { timeout: 5000 });
  await expect(page.locator(".react-flow__handle")).toHaveCount(6);
  await expect(page.locator(".react-flow__edge")).toHaveCount(3);
  await expect(page.getByTestId("workflow-run-row")).toHaveCount(1);
  expect(consoleErrors, `console.error: ${consoleErrors.join(" | ")}`).toEqual([]);
  expect(pageErrors, `pageerror: ${pageErrors.join(" | ")}`).toEqual([]);
  await assertClean(page, "/workflows desktop dark");
  // 画布布局动画收尾后快照（react-flow 节点定位有入场过渡）
  await page.waitForTimeout(500);
  await expect(page).toHaveScreenshot("_workflows-rip-dark-desktop.png");
});

test("画布浅色主题：节点词徽标渲染 + 零 console 错误", async ({ page }) => {
  const consoleErrors: string[] = [];
  const pageErrors: string[] = [];
  page.on("console", (msg) => {
    if (msg.type() === "error") consoleErrors.push(msg.text());
  });
  page.on("pageerror", (error) => pageErrors.push(error.message));
  await page.setViewportSize({ width: 1440, height: 900 });
  await openPage(page, "rip-light");
  await expect(page.locator(".react-flow__node")).toHaveCount(4, { timeout: 5000 });
  await expect(page.locator('.react-flow__node[data-id="review"]')).toBeVisible();
  await expect(
    page.locator('.react-flow__node[data-id="review"]').getByText("human"),
  ).toBeVisible();
  expect(consoleErrors, `console.error: ${consoleErrors.join(" | ")}`).toEqual([]);
  expect(pageErrors, `pageerror: ${pageErrors.join(" | ")}`).toEqual([]);
  await assertClean(page, "/workflows desktop light");
  await page.waitForTimeout(500);
  await expect(page).toHaveScreenshot("_workflows-rip-light-desktop.png");
});

test("移动端只读降级：窄视口徽标 + 画布只读 + 无横向溢出", async ({ page }) => {
  const consoleErrors: string[] = [];
  page.on("console", (msg) => {
    if (msg.type() === "error") consoleErrors.push(msg.text());
  });
  await page.setViewportSize({ width: 390, height: 844 });
  await openPage(page, "rip-dark");
  await expect(page.getByTestId("readonly-banner")).toBeVisible({ timeout: 5000 });
  await expect(page.getByTestId("readonly-banner")).toHaveText("移动端只读查看：编辑请切换宽屏");
  await expect(page.locator(".react-flow__node")).toHaveCount(4, { timeout: 5000 });
  expect(consoleErrors, `console.error: ${consoleErrors.join(" | ")}`).toEqual([]);
  await assertClean(page, "/workflows mobile");
  await page.waitForTimeout(500);
  await expect(page).toHaveScreenshot("_workflows-rip-dark-mobile.png");
});

for (const theme of ["rip-dark", "rip-light"] as const) {
  test(`axe AA /workflows（桌面 ${theme === "rip-dark" ? "深空" : "晨白"}）`, async ({ page }) => {
    await page.setViewportSize({ width: 1440, height: 900 });
    await openPage(page, theme);
    await expect(page.locator(".react-flow__node").first()).toBeVisible({ timeout: 5000 });
    const results = await new AxeBuilder({ page })
      .withTags(["wcag2a", "wcag2aa", "wcag21aa"])
      .analyze();
    const serious = results.violations.filter(
      (v) => v.impact === "serious" || v.impact === "critical",
    );
    // P10-4 阻断化：color-contrast 已清零，不再豁免（与 pages.spec 同口径）
    expect(
      serious,
      `axe serious/critical on /workflows ${theme}: ${serious
        .map((v) => `${v.id}(${v.impact})x${v.nodes.length}`)
        .join(", ")}`,
    ).toEqual([]);
  });
}
