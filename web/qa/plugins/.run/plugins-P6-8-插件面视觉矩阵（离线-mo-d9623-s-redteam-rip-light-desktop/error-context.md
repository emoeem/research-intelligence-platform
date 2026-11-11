# Instructions

- Following Playwright test failed.
- Explain why, be concise, respect Playwright best practices.
- Provide a snippet of code with the fix, if possible.

# Test info

- Name: plugins.spec.ts >> P6-8 插件面视觉矩阵（离线 mock） >> 插件详情 /plugins/knowledge.rtis-redteam rip-light desktop
- Location: tests/plugins.spec.ts:132:7

# Error details

```
Error: expect(locator).toBeVisible() failed

Locator: getByTestId('capability-list')
Expected: visible
Timeout: 5000ms
Error: element(s) not found

Call log:
  - Expect "toBeVisible" getByTestId('capability-list') with timeout 5000ms
  - waiting for getByTestId('capability-list')

```

```yaml
- text: not found
```

# Test source

```ts
  42  |       },
  43  |     ],
  44  |     categories: {
  45  |       installed: ["knowledge.rtis-redteam", "tool.rip.python-workers", "project.rtis-mcp"],
  46  |       updatable: ["tool.rip.python-workers"],
  47  |       disabled: [],
  48  |       uninstalled: [],
  49  |     },
  50  |     audit_summary: [{ action: "plugin.install", n: 3 }, { action: "plugin.enable", n: 2 }],
  51  |   },
  52  |   "/api/plugins/knowledge.rtis-redteam": {
  53  |     level: 2,
  54  |     plugin_id: "knowledge.rtis-redteam", version: "0.1.0", name: "RTIS 红队知识包",
  55  |     kind: "knowledge", description: "RTIS kit 语料接入", license: "UNLICENSED",
  56  |     trust_level: "local", state: "ENABLED", update_pending: false, is_active: true,
  57  |     governance: "ACTIVE", source_type: "local_dir", source_ref: "integrations/rtis",
  58  |     lock_sha256: "f".repeat(64),
  59  |     installed_dir: "rip-data/plugins/installed/knowledge.rtis-redteam/0.1.0",
  60  |     source_uninstalled: false, updated_at: "2026-11-11T10:00:00Z",
  61  |     context: { summary: "红队 POC 与技能知识源", docs: "README.md" },
  62  |     capabilities: [
  63  |       { capability_id: "knowledge-source.rtis.poc-corpus", capability_type: "knowledge_source",
  64  |         entry: "README.md", summary: "POC 正文与技能文档语料", runtime: null, requires: [] },
  65  |     ],
  66  |     compatibility: { rip_core: ">=0.2.0, <1.0.0", manifest_schema: ["1"] },
  67  |     dependencies: [],
  68  |     audit: [
  69  |       { event_id: 3, ts: "2026-11-11T10:01:00Z", actor: "user:cli", action: "plugin.enable", entity_id: "knowledge.rtis-redteam@0.1.0" },
  70  |       { event_id: 1, ts: "2026-11-11T10:00:00Z", actor: "user:cli", action: "plugin.install", entity_id: "knowledge.rtis-redteam@0.1.0" },
  71  |     ],
  72  |   },
  73  | };
  74  | 
  75  | const THEMES = ["rip-dark", "rip-light"] as const;
  76  | const VIEWPORTS = [
  77  |   { width: 1440, height: 900, label: "desktop" },
  78  |   { width: 390, height: 844, label: "mobile" },
  79  | ];
  80  | 
  81  | const e2e = (globalThis as { process?: { env?: Record<string, string | undefined> } }).process?.env?.RP_P6_E2E === "1";
  82  | 
  83  | async function mockApis(page: Page): Promise<void> {
  84  |   await page.route("**/api/**", (route) => {
  85  |     const path = new URL(route.request().url()).pathname;
  86  |     route.fulfill({
  87  |       contentType: "application/json; charset=utf-8",
  88  |       body: JSON.stringify(API_STUBS[path] ?? {}),
  89  |     });
  90  |   });
  91  | }
  92  | 
  93  | async function openPage(page: Page, path: string, theme: string): Promise<void> {
  94  |   await page.addInitScript((stored) => {
  95  |     window.localStorage.setItem("rip_theme", stored);
  96  |   }, theme);
  97  |   await page.goto(path);
  98  |   await page.waitForLoadState("domcontentloaded");
  99  | }
  100 | 
  101 | test.describe("P6-8 插件面视觉矩阵（离线 mock）", () => {
  102 |   test.skip(e2e, "真实后端模式下跳过 mock 矩阵");
  103 | 
  104 |   for (const theme of THEMES) {
  105 |     for (const viewport of VIEWPORTS) {
  106 |       test(`插件目录 /plugins ${theme} ${viewport.label}`, async ({ page }) => {
  107 |         await page.setViewportSize({ width: viewport.width, height: viewport.height });
  108 |         const consoleErrors: string[] = [];
  109 |         const pageErrors: string[] = [];
  110 |         page.on("console", (msg) => {
  111 |           if (msg.type() === "error") consoleErrors.push(msg.text());
  112 |         });
  113 |         page.on("pageerror", (err) => pageErrors.push(String(err)));
  114 |         await mockApis(page);
  115 |         await openPage(page, "/plugins", theme);
  116 | 
  117 |         await expect(page.getByTestId("plugins-installed")).toBeVisible();
  118 |         await expect(page.getByTestId("plugin-card-knowledge.rtis-redteam")).toBeVisible();
  119 |         await expect(page.getByTestId("plugin-card-tool.rip.python-workers")).toBeVisible();
  120 | 
  121 |         const shot = `qa/plugins/plugins-page-${theme}-${viewport.label}.png`;
  122 |         await page.screenshot({ path: shot, fullPage: true });
  123 | 
  124 |         expect(consoleErrors, `console.error: ${consoleErrors}`).toEqual([]);
  125 |         expect(pageErrors, `pageerror: ${pageErrors}`).toEqual([]);
  126 |         const overflow = await page.evaluate(
  127 |           () => document.documentElement.scrollWidth <= window.innerWidth + 1,
  128 |         );
  129 |         expect(overflow, "无横向溢出").toBe(true);
  130 |       });
  131 | 
  132 |       test(`插件详情 /plugins/knowledge.rtis-redteam ${theme} ${viewport.label}`, async ({ page }) => {
  133 |         await page.setViewportSize({ width: viewport.width, height: viewport.height });
  134 |         const consoleErrors: string[] = [];
  135 |         page.on("console", (msg) => {
  136 |           if (msg.type() === "error") consoleErrors.push(msg.text());
  137 |         });
  138 |         page.on("pageerror", (err) => consoleErrors.push(String(err)));
  139 |         await mockApis(page);
  140 |         await openPage(page, "/plugins/knowledge.rtis-redteam", theme);
  141 | 
> 142 |         await expect(page.getByTestId("capability-list")).toBeVisible();
      |                                                           ^ Error: expect(locator).toBeVisible() failed
  143 |         await expect(page.getByTestId("compat-table")).toBeVisible();
  144 |         await expect(page.getByTestId("audit-trail")).toBeVisible();
  145 | 
  146 |         // L3 级别切换：Manifest JSON 与 docs 面出现。
  147 |         await page.getByTestId("level-switch").getByRole("button", { name: "L3" }).click();
  148 |         await expect(page.getByTestId("manifest-json")).toBeVisible();
  149 | 
  150 |         const shot = `qa/plugins/plugins-detail-${theme}-${viewport.label}.png`;
  151 |         await page.screenshot({ path: shot, fullPage: true });
  152 |         expect(consoleErrors, `console.error: ${consoleErrors}`).toEqual([]);
  153 |       });
  154 |     }
  155 |   }
  156 | });
  157 | 
  158 | test.describe("P6-8 真实后端操作回路（RP_P6_E2E=1）", () => {
  159 |   test.skip(!e2e, "默认离线模式跳过（selftest_phase6.sh 以 RP_P6_E2E=1 调用）");
  160 | 
  161 |   test("启用→禁用→启用 操作回路（真实 API + 截图）", async ({ page }) => {
  162 |     await page.setViewportSize({ width: 1440, height: 900 });
  163 |     // 前置：selftest 已把 knowledge.rtis-redteam 置 DISABLED，本测试点启用按钮。
  164 |     await page.goto("/plugins/knowledge.rtis-redteam");
  165 |     await expect(page.getByTestId("detail-state")).toContainText("DISABLED");
  166 |     await page.screenshot({ path: "qa/plugins/e2e-before-enable.png", fullPage: true });
  167 | 
  168 |     await page.getByTestId("action-enable").click();
  169 |     await expect(page.getByTestId("detail-state")).toHaveText("ENABLED", { timeout: 30000 });
  170 |     await page.screenshot({ path: "qa/plugins/e2e-enabled.png", fullPage: true });
  171 | 
  172 |     // disable 回路（恢复 selftest 后续断言期望的 DISstate 基线在 shell 侧再做）。
  173 |     await page.getByTestId("action-disable").click();
  174 |     await expect(page.getByTestId("detail-state")).toHaveText("DISABLED", { timeout: 30000 });
  175 |     await page.screenshot({ path: "qa/plugins/e2e-disabled.png", fullPage: true });
  176 | 
  177 |     // 目录页分类核验。
  178 |     await page.goto("/plugins");
  179 |     await expect(page.getByTestId("plugin-card-knowledge.rtis-redteam")).toBeVisible();
  180 |     await page.screenshot({ path: "qa/plugins/e2e-catalog.png", fullPage: true });
  181 |   });
  182 | });
  183 | 
```