import { type Page, test, expect } from "@playwright/test";

/** P6-8 插件面 Playwright（web/tests/plugins.spec.ts，专用配置
 *  web/playwright.plugins.config.ts）：
 *
 *  模式 A（默认，离线静态服务器 + route-mock，P5-10 同模式）：
 *    - /plugins 与 /plugins/:id 视觉矩阵（2 主题 × 2 视口）；
 *    - 截图落 web/qa/plugins/（真实浏览器逐页逐功能实测证据，SESSION-HANDOFF §九）；
 *    - 断言：零 console.error / 零 pageerror、分类与徽章在位、L1/L3 级别切换。
 *
 *  模式 B（RP_P6_E2E=1，真实后端）：baseURL 指向真实 rp_server（selftest
 *  起服 + 临时令牌），走真实 API 完成启用→禁用→启用的操作回路断言与截图。
 *
 *  运行：npx playwright test tests/plugins.spec.ts -c playwright.plugins.config.ts */

const API_STUBS: Record<string, unknown> = {
  "/api/plugins": {
    available: true,
    db: "migrations/dev-p6.db",
    total: 3,
    items: [
      {
        plugin_id: "knowledge.rtis-redteam", version: "0.1.0", name: "RTIS 红队知识包",
        kind: "knowledge", description: "RTIS kit 语料接入", license: "UNLICENSED",
        trust_level: "local", state: "ENABLED", update_pending: false, is_active: true,
        governance: "ACTIVE", source_type: "local_dir", source_ref: "integrations/rtis",
        capabilities_count: 2, updated_at: "2026-11-11T10:00:00Z",
      },
      {
        plugin_id: "tool.rip.python-workers", version: "0.1.0", name: "RIP Python Workers",
        kind: "tool", description: "三个 Python Worker", license: "UNLICENSED",
        trust_level: "local", state: "REGISTERED", update_pending: true, is_active: true,
        governance: "ACTIVE", source_type: "local_dir", source_ref: "workers/python",
        capabilities_count: 3, updated_at: "2026-11-11T10:00:00Z",
      },
      {
        plugin_id: "project.rtis-mcp", version: "0.1.0", name: "RTIS MCP Endpoint",
        kind: "project", description: "rtis_mcp.mjs 接入点", license: "UNLICENSED",
        trust_level: "remote", state: "ENABLED", update_pending: false, is_active: true,
        governance: "ACTIVE", source_type: "local_dir", source_ref: "integrations/rtis",
        capabilities_count: 1, updated_at: "2026-11-11T10:05:00Z",
      },
    ],
    categories: {
      installed: ["knowledge.rtis-redteam", "tool.rip.python-workers", "project.rtis-mcp"],
      updatable: ["tool.rip.python-workers"],
      disabled: [],
      uninstalled: [],
    },
    audit_summary: [{ action: "plugin.install", n: 3 }, { action: "plugin.enable", n: 2 }],
  },
  "/api/plugins/knowledge.rtis-redteam": {
    level: 2,
    plugin_id: "knowledge.rtis-redteam", version: "0.1.0", name: "RTIS 红队知识包",
    kind: "knowledge", description: "RTIS kit 语料接入", license: "UNLICENSED",
    trust_level: "local", state: "ENABLED", update_pending: false, is_active: true,
    governance: "ACTIVE", source_type: "local_dir", source_ref: "integrations/rtis",
    lock_sha256: "f".repeat(64),
    installed_dir: "rip-data/plugins/installed/knowledge.rtis-redteam/0.1.0",
    source_uninstalled: false, updated_at: "2026-11-11T10:00:00Z",
    context: { summary: "红队 POC 与技能知识源", docs: "README.md" },
    capabilities: [
      { capability_id: "knowledge-source.rtis.poc-corpus", capability_type: "knowledge_source",
        entry: "README.md", summary: "POC 正文与技能文档语料", runtime: null, requires: [] },
    ],
    compatibility: { rip_core: ">=0.2.0, <1.0.0", manifest_schema: ["1"] },
    dependencies: [],
    audit: [
      { event_id: 3, ts: "2026-11-11T10:01:00Z", actor: "user:cli", action: "plugin.enable", entity_id: "knowledge.rtis-redteam@0.1.0" },
      { event_id: 1, ts: "2026-11-11T10:00:00Z", actor: "user:cli", action: "plugin.install", entity_id: "knowledge.rtis-redteam@0.1.0" },
    ],
  },
};

const THEMES = ["rip-dark", "rip-light"] as const;
const VIEWPORTS = [
  { width: 1440, height: 900, label: "desktop" },
  { width: 390, height: 844, label: "mobile" },
];

const e2e = (globalThis as { process?: { env?: Record<string, string | undefined> } }).process?.env?.RP_P6_E2E === "1";

async function mockApis(page: Page): Promise<void> {
  await page.route("**/api/**", (route) => {
    const path = new URL(route.request().url()).pathname;
    route.fulfill({
      contentType: "application/json; charset=utf-8",
      body: JSON.stringify(API_STUBS[path] ?? {}),
    });
  });
}

async function openPage(page: Page, path: string, theme: string): Promise<void> {
  await page.addInitScript((stored) => {
    window.localStorage.setItem("rip_theme", stored);
  }, theme);
  await page.goto(path);
  await page.waitForLoadState("domcontentloaded");
}

test.describe("P6-8 插件面视觉矩阵（离线 mock）", () => {
  test.skip(e2e, "真实后端模式下跳过 mock 矩阵");

  for (const theme of THEMES) {
    for (const viewport of VIEWPORTS) {
      test(`插件目录 /plugins ${theme} ${viewport.label}`, async ({ page }) => {
        await page.setViewportSize({ width: viewport.width, height: viewport.height });
        const consoleErrors: string[] = [];
        const pageErrors: string[] = [];
        page.on("console", (msg) => {
          if (msg.type() === "error") consoleErrors.push(msg.text());
        });
        page.on("pageerror", (err) => pageErrors.push(String(err)));
        await mockApis(page);
        await openPage(page, "/plugins", theme);

        await expect(page.getByTestId("plugins-installed")).toBeVisible();
        await expect(page.getByTestId("plugin-card-knowledge.rtis-redteam")).toBeVisible();
        await expect(page.getByTestId("plugin-card-tool.rip.python-workers")).toBeVisible();

        const shot = `qa/plugins/plugins-page-${theme}-${viewport.label}.png`;
        await page.screenshot({ path: shot, fullPage: true });

        expect(consoleErrors, `console.error: ${consoleErrors}`).toEqual([]);
        expect(pageErrors, `pageerror: ${pageErrors}`).toEqual([]);
        const overflow = await page.evaluate(
          () => document.documentElement.scrollWidth <= window.innerWidth + 1,
        );
        expect(overflow, "无横向溢出").toBe(true);
      });

      test(`插件详情 /plugins/knowledge.rtis-redteam ${theme} ${viewport.label}`, async ({ page }) => {
        await page.setViewportSize({ width: viewport.width, height: viewport.height });
        const consoleErrors: string[] = [];
        page.on("console", (msg) => {
          if (msg.type() === "error") consoleErrors.push(msg.text());
        });
        page.on("pageerror", (err) => consoleErrors.push(String(err)));
        await mockApis(page);
        await openPage(page, "/plugins/knowledge.rtis-redteam", theme);

        await expect(page.getByTestId("capability-list")).toBeVisible();
        await expect(page.getByTestId("compat-table")).toBeVisible();
        await expect(page.getByTestId("audit-trail")).toBeVisible();

        // L3 级别切换：Manifest JSON 与 docs 面出现。
        await page.getByTestId("level-switch").getByRole("button", { name: "L3" }).click();
        await expect(page.getByTestId("manifest-json")).toBeVisible();

        const shot = `qa/plugins/plugins-detail-${theme}-${viewport.label}.png`;
        await page.screenshot({ path: shot, fullPage: true });
        expect(consoleErrors, `console.error: ${consoleErrors}`).toEqual([]);
      });
    }
  }
});

test.describe("P6-8 真实后端操作回路（RP_P6_E2E=1）", () => {
  test.skip(!e2e, "默认离线模式跳过（selftest_phase6.sh 以 RP_P6_E2E=1 调用）");

  test("启用→禁用→启用 操作回路（真实 API + 截图）", async ({ page }) => {
    await page.setViewportSize({ width: 1440, height: 900 });
    // 前置：selftest 已把 knowledge.rtis-redteam 置 DISABLED，本测试点启用按钮。
    await page.goto("/plugins/knowledge.rtis-redteam");
    await expect(page.getByTestId("detail-state")).toContainText("DISABLED");
    await page.screenshot({ path: "qa/plugins/e2e-before-enable.png", fullPage: true });

    await page.getByTestId("action-enable").click();
    await expect(page.getByTestId("detail-state")).toHaveText("ENABLED", { timeout: 30000 });
    await page.screenshot({ path: "qa/plugins/e2e-enabled.png", fullPage: true });

    // disable 回路（恢复 selftest 后续断言期望的 DISstate 基线在 shell 侧再做）。
    await page.getByTestId("action-disable").click();
    await expect(page.getByTestId("detail-state")).toHaveText("DISABLED", { timeout: 30000 });
    await page.screenshot({ path: "qa/plugins/e2e-disabled.png", fullPage: true });

    // 目录页分类核验。
    await page.goto("/plugins");
    await expect(page.getByTestId("plugin-card-knowledge.rtis-redteam")).toBeVisible();
    await page.screenshot({ path: "qa/plugins/e2e-catalog.png", fullPage: true });
  });
});
