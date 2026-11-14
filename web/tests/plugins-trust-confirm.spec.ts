import { test, expect, type Page } from "@playwright/test";

/** P10-2 PluginsPage 分级确认提示真实浏览器实测（离线静态服务器 +
 *  route-mock，P6-8 插件面同模式；真实浏览器逐档实测证据，截图落
 *  docs/phase10/browser-e2e/，是本批验收判据的载体）：
 *  - official：常规确认（一次点击）；
 *  - community：确认对话框一次同意；
 *  - local：来源核对勾选后确认；
 *  - remote：输入插件 id 二次确认（不一致阻断，D10 手动检查更新提示）；
 *  - 写请求体随档位携带 confirm/confirm_id（服务端同一策略表强制的
 *    前端对偶面）。 */

const POLICY_STUB = {
  schema_version: "1",
  source: "plugin-registry::trust_policy (PLUGIN-CONTRACT §6 + D10)",
  tiers: [
    {
      trust_level: "official",
      install_confirmation: "single",
      confirmation_label: "官方来源：常规确认",
      extra_steps: 0,
      requires_provenance: false,
      requires_typed_id: false,
      required_confirm_fields: [],
      auto_update_prompt: true,
    },
    {
      trust_level: "community",
      install_confirmation: "confirm",
      confirmation_label: "社区来源：确认安装",
      extra_steps: 1,
      requires_provenance: false,
      requires_typed_id: false,
      required_confirm_fields: ["confirm"],
      auto_update_prompt: false,
    },
    {
      trust_level: "local",
      install_confirmation: "confirm_with_provenance",
      confirmation_label: "本地来源：核对来源后确认",
      extra_steps: 1,
      requires_provenance: true,
      requires_typed_id: false,
      required_confirm_fields: ["confirm"],
      auto_update_prompt: false,
    },
    {
      trust_level: "remote",
      install_confirmation: "double_confirm_typed",
      confirmation_label: "远程来源：输入插件 id 二次确认",
      extra_steps: 1,
      requires_provenance: false,
      requires_typed_id: true,
      required_confirm_fields: ["confirm", "confirm_id"],
      auto_update_prompt: false,
    },
  ],
};

const INSPECT_STUB = {
  ok: true,
  result: {
    id: "knowledge.rtis-redteam",
    version: "0.1.0",
    kind: "knowledge",
    license: "UNLICENSED",
    source: { type: "git", host: "github", url: "https://github.com/x/y" },
    source_default_trust: "community",
    install_confirmation: "confirm",
    confirmation_label: "社区来源：确认安装",
    auto_update_prompt: false,
  },
};

// 截图相对 web/ cwd 落 docs/phase10/browser-e2e/（playwright 自动建目录，
// node:* 导入不在 tsconfig types 面，P10-2 口径）。

const installWrites: { confirm?: unknown; confirm_id?: unknown; trust?: unknown }[] = [];

async function mockApis(page: Page): Promise<void> {
  // 路由匹配后注册者优先：catch-all 先注册，具体端点后注册覆盖。
  await page.route("**/api/**", (route) =>
    route.fulfill({ contentType: "application/json", body: "{}" }),
  );
  await page.route("**/api/plugins/trust-policy", (route) =>
    route.fulfill({ contentType: "application/json", body: JSON.stringify(POLICY_STUB) }),
  );
  await page.route("**/api/plugins/manifest-inspect", (route) =>
    route.fulfill({ contentType: "application/json", body: JSON.stringify(INSPECT_STUB) }),
  );
  await page.route("**/api/plugins/install", (route) => {
    const body = route.request().postDataJSON() as Record<string, unknown>;
    installWrites.push({
      confirm: body.confirm,
      confirm_id: body.confirm_id,
      trust: body.trust,
    });
    route.fulfill({
      contentType: "application/json",
      body: JSON.stringify({
        ok: true,
        action: "install",
        plugin_id: "x",
        result: { command: "install" },
      }),
    });
  });
  await page.route("**/api/plugins", (route) =>
    route.fulfill({
      contentType: "application/json",
      body: JSON.stringify({ available: true, total: 0, items: [] }),
    }),
  );
}

test.describe("P10-2 插件信任级分级确认（真实浏览器）", () => {
  test.beforeEach(() => {
    installWrites.length = 0;
  });

  test("official：常规确认一次提交（无额外字段）+ 截图", async ({ page }) => {
    const consoleErrors: string[] = [];
    page.on("console", (msg) => {
      if (msg.type() === "error") consoleErrors.push(msg.text());
    });
    page.on("pageerror", (err) => consoleErrors.push(String(err)));
    await mockApis(page);
    await page.goto("/plugins");
    await expect(page.getByTestId("install-panel")).toBeVisible();
    await page
      .getByTestId("install-manifest-path")
      .fill("schemas/plugin-manifest/examples/knowledge.rtis-redteam.json");
    await page.getByTestId("install-trust-select").selectOption("official");
    await expect(page.getByTestId("install-tier-hint")).toContainText("D10 允许自动更新提示");
    await page.getByTestId("install-inspect").click();
    await expect(page.getByTestId("install-preview")).toBeVisible();
    await page.getByTestId("install-open-confirm").click();
    const dialog = page.getByTestId("trust-confirm-dialog");
    await expect(dialog).toContainText("官方来源：常规确认");
    await page.screenshot({
      path: "../docs/phase10/browser-e2e/trust-confirm-official.png",
      fullPage: true,
    });
    const proceed = page.getByTestId("confirm-proceed");
    await expect(proceed).toBeEnabled();
    await proceed.click();
    await expect(page.getByTestId("install-ok")).toBeVisible();
    expect(installWrites).toHaveLength(1);
    expect(installWrites[0]!).toMatchObject({ trust: "official" });
    expect(installWrites[0]!.confirm).toBeUndefined();
    expect(consoleErrors, `console.error: ${consoleErrors}`).toEqual([]);
  });

  test("community：确认对话框一次同意 + 截图", async ({ page }) => {
    const consoleErrors: string[] = [];
    page.on("console", (msg) => {
      if (msg.type() === "error") consoleErrors.push(msg.text());
    });
    page.on("pageerror", (err) => consoleErrors.push(String(err)));
    await mockApis(page);
    await page.goto("/plugins");
    await page
      .getByTestId("install-manifest-path")
      .fill("schemas/plugin-manifest/examples/knowledge.rtis-redteam.json");
    await page.getByTestId("install-trust-select").selectOption("community");
    await page.getByTestId("install-inspect").click();
    await expect(page.getByTestId("install-preview")).toBeVisible();
    await page.getByTestId("install-open-confirm").click();
    const dialog = page.getByTestId("trust-confirm-dialog");
    await expect(dialog).toContainText("社区来源：确认安装");
    const proceed = page.getByTestId("confirm-proceed");
    await expect(proceed).toBeEnabled();
    await page.screenshot({
      path: "../docs/phase10/browser-e2e/trust-confirm-community.png",
      fullPage: true,
    });
    await proceed.click();
    await expect(page.getByTestId("install-ok")).toBeVisible();
    expect(installWrites[0]!).toMatchObject({ trust: "community", confirm: true });
    expect(consoleErrors, `console.error: ${consoleErrors}`).toEqual([]);
  });

  test("local：来源核对勾选门 + 截图", async ({ page }) => {
    const consoleErrors: string[] = [];
    page.on("console", (msg) => {
      if (msg.type() === "error") consoleErrors.push(msg.text());
    });
    page.on("pageerror", (err) => consoleErrors.push(String(err)));
    await mockApis(page);
    await page.goto("/plugins");
    await page
      .getByTestId("install-manifest-path")
      .fill("schemas/plugin-manifest/examples/knowledge.rtis-redteam.json");
    await page.getByTestId("install-trust-select").selectOption("local");
    await page.getByTestId("install-inspect").click();
    await expect(page.getByTestId("install-preview")).toBeVisible();
    await page.getByTestId("install-open-confirm").click();
    const dialog = page.getByTestId("trust-confirm-dialog");
    await expect(dialog).toContainText("本地来源：核对来源后确认");
    await expect(page.getByTestId("confirm-provenance")).toBeVisible();
    const proceed = page.getByTestId("confirm-proceed");
    await expect(proceed).toBeDisabled();
    await page.getByTestId("confirm-provenance").locator("input").check();
    await expect(proceed).toBeEnabled();
    await page.screenshot({
      path: "../docs/phase10/browser-e2e/trust-confirm-local.png",
      fullPage: true,
    });
    await proceed.click();
    await expect(page.getByTestId("install-ok")).toBeVisible();
    expect(installWrites[0]!).toMatchObject({ trust: "local", confirm: true });
    expect(installWrites[0]!.confirm_id).toBeUndefined();
    expect(consoleErrors, `console.error: ${consoleErrors}`).toEqual([]);
  });

  test("remote：输入插件 id 二次确认（不一致阻断）+ 截图", async ({ page }) => {
    const consoleErrors: string[] = [];
    page.on("console", (msg) => {
      if (msg.type() === "error") consoleErrors.push(msg.text());
    });
    page.on("pageerror", (err) => consoleErrors.push(String(err)));
    await mockApis(page);
    await page.goto("/plugins");
    await page
      .getByTestId("install-manifest-path")
      .fill("schemas/plugin-manifest/examples/knowledge.rtis-redteam.json");
    await page.getByTestId("install-trust-select").selectOption("remote");
    await expect(page.getByTestId("install-tier-hint")).toContainText("D10 需手动检查更新");
    await page.getByTestId("install-inspect").click();
    await expect(page.getByTestId("install-preview")).toBeVisible();
    await page.getByTestId("install-open-confirm").click();
    const dialog = page.getByTestId("trust-confirm-dialog");
    await expect(dialog).toContainText("远程来源：输入插件 id 二次确认");
    const proceed = page.getByTestId("confirm-proceed");
    await expect(proceed).toBeDisabled();
    const typed = page.getByTestId("confirm-typed-id");
    await typed.fill("knowledge.wrong-id");
    await expect(page.getByTestId("confirm-id-mismatch")).toBeVisible();
    await page.screenshot({
      path: "../docs/phase10/browser-e2e/trust-confirm-remote-mismatch.png",
      fullPage: true,
    });
    await typed.fill("knowledge.rtis-redteam");
    await expect(proceed).toBeEnabled();
    await page.screenshot({
      path: "../docs/phase10/browser-e2e/trust-confirm-remote.png",
      fullPage: true,
    });
    await proceed.click();
    await expect(page.getByTestId("install-ok")).toBeVisible();
    expect(installWrites[0]!).toMatchObject({
      trust: "remote",
      confirm: true,
      confirm_id: "knowledge.rtis-redteam",
    });
    expect(consoleErrors, `console.error: ${consoleErrors}`).toEqual([]);
  });
});
