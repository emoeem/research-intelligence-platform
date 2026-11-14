import { type Page, test, expect } from "@playwright/test";

/** P9-4 Tool Center MCP 目录同步状态卡 Playwright（web/tests/mcp-status.spec.ts，
 *  专用配置 web/playwright.mcp.config.ts）：
 *
 *  模式 A（默认，离线静态服务器 + route-mock，P5-10 同模式）：状态卡三态
 *  （对账正常 / 对账漂移 / 数据面不可读）+ 截图落 docs/phase9/browser-e2e/mock/。
 *
 *  模式 B（RP_P9_E2E=1，真实后端）：baseURL/token 由环境注入（起服脚本见
 *  docs/phase9/browser-e2e/README），真实 /api/mcp-catalog 与 /api/tools，
 *  逐区查看状态卡并截图入库（SESSION-HANDOFF §九 真实浏览器硬性要求）。
 *
 *  运行：npx playwright test tests/mcp-status.spec.ts -c playwright.mcp.config.ts
 *  （模式 B 加 RP_P9_E2E=1 RP_P9_BASE_URL=... RP_P9_TOKEN=...） */

const MCP_OK = {
  available: true,
  source: "main",
  catalog_count: 20,
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
  note: "MCP tools/list 注册面与 /api/tools 共用同一 tool 表读面",
};

const TOOLS_STUB = {
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

/** 环境读取（spec 文件不含 node types；config 侧 webServer 形态恒定）：
 *  RP_P9_E2E=1 真实后端、RP_P9_BASE_URL/RP_P9_TOKEN 由起服脚本注入。 */
const env =
  (globalThis as { process?: { env?: Record<string, string | undefined> } }).process?.env ?? {};
const isE2E = env.RP_P9_E2E === "1";
const SHOT_DIR = isE2E ? "../docs/phase9/browser-e2e" : "../docs/phase9/browser-e2e/mock";

async function openToolsPage(page: Page, apiStubs?: Record<string, unknown>): Promise<void> {
  await page.addInitScript(() => {
    window.localStorage.setItem("rip_theme", "rip-dark");
  });
  if (apiStubs) {
    await page.route("**/api/**", (route) => {
      const path = new URL(route.request().url()).pathname;
      route.fulfill({
        contentType: "application/json; charset=utf-8",
        body: JSON.stringify(apiStubs[path] ?? {}),
      });
    });
  } else if (isE2E) {
    // 模式 B（P6-8 同款）：/api/** 转发真实 rp_server，Bearer 由 RP_P9_TOKEN 注入。
    const backend = env.RP_P9_BASE_URL ?? "";
    const token = env.RP_P9_TOKEN ?? "";
    await page.route("**/api/**", (route) => {
      const url = new URL(route.request().url());
      const apiPath = url.pathname + url.search;
      const headers = { ...route.request().headers(), Authorization: `Bearer ${token}` };
      route.continue({ url: `${backend}${apiPath}`, headers });
    });
  }
  await page.goto("/tools");
  await page.waitForLoadState("domcontentloaded");
}

if (!isE2E) {
  // 模式 A：route-mock 三态（视觉矩阵主门禁已含 /tools 页，这里补状态卡三态）。
  test("状态卡 对账正常（route-mock）", async ({ page }) => {
    await openToolsPage(page, { "/api/tools": TOOLS_STUB, "/api/mcp-catalog": MCP_OK });
    const card = page.getByLabel("MCP 目录同步状态");
    await expect(card).toBeVisible({ timeout: 5000 });
    await expect(card.getByText(/目录条数 20（能力面 6 条固定）/)).toBeVisible();
    await expect(card.getByText("对账正常")).toBeVisible();
    await expect(page.getByText("共 20 个工具").first()).toBeVisible();
    await page.screenshot({
      path: `${SHOT_DIR}/mcp-status-ok.png`,
      fullPage: true,
    });
  });

  test("状态卡 对账漂移（route-mock）", async ({ page }) => {
    await openToolsPage(page, {
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
    const card = page.getByLabel("MCP 目录同步状态");
    await expect(card.getByText("对账漂移 1 条")).toBeVisible({ timeout: 5000 });
    await expect(card.getByText("lifecycle_wordlist")).toBeVisible();
    await page.screenshot({ path: `${SHOT_DIR}/mcp-status-drift.png`, fullPage: true });
  });

  test("状态卡 数据面不可读（route-mock）", async ({ page }) => {
    await openToolsPage(page, {
      "/api/tools": TOOLS_STUB,
      "/api/mcp-catalog": {
        available: false,
        reason: "table_missing",
        message: "主库缺 tool 表（迁移水位不足），目录暂不可读",
      },
    });
    const card = page.getByLabel("MCP 目录同步状态");
    await expect(card.getByText("主库缺 tool 表（迁移水位不足），目录暂不可读")).toBeVisible({
      timeout: 5000,
    });
    await page.screenshot({ path: `${SHOT_DIR}/mcp-status-unavailable.png`, fullPage: true });
  });
} else {
  // 模式 B：真实后端逐区实测（SESSION-HANDOFF §九）。数据面由起服脚本准备
  // （fixture 库：rp-tool register + rp-workflow put + 两通道真实运行），
  // 状态卡与目录列表逐区断言 + 截图入库。
  test("真实后端：MCP 状态卡逐区查看与入库", async ({ page }) => {
    await openToolsPage(page);
    const card = page.getByLabel("MCP 目录同步状态");
    await expect(card).toBeVisible({ timeout: 10000 });
    await expect(card.getByText(/目录条数/)).toBeVisible();
    // 对账结果标记（真实数据面 ok=true 时为「对账正常」，漂移如实展示）。
    const okBadge = card.getByText("对账正常");
    const driftBadge = card.getByText(/对账漂移/);
    await expect(okBadge.or(driftBadge).first()).toBeVisible();
    // 状态卡区域特写截图（逐区查看证据）。
    await card.screenshot({ path: `${SHOT_DIR}/01-mcp-status-card-real.png` });
    // 目录列表区（同页逐区）。
    const list = page.getByRole("list").first();
    await expect(list).toBeVisible();
    await list.screenshot({ path: `${SHOT_DIR}/02-tool-list-real.png` });
    // 整页（桌面视口）。
    await page.setViewportSize({ width: 1440, height: 900 });
    await page.screenshot({
      path: `${SHOT_DIR}/03-tool-center-fullpage-real.png`,
      fullPage: true,
    });
    // 移动视口。
    await page.setViewportSize({ width: 390, height: 844 });
    await page.screenshot({
      path: `${SHOT_DIR}/04-tool-center-mobile-real.png`,
      fullPage: true,
    });
  });
}
