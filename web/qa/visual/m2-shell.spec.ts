import { expect, test } from "@playwright/test";

const VIEWPORTS = [
  { name: "compact-mobile", width: 360, height: 800 },
  { name: "tablet-portrait", width: 768, height: 1024 },
  { name: "tablet-landscape", width: 1024, height: 768 },
  { name: "desktop", width: 1280, height: 900 },
  { name: "wide-desktop", width: 1600, height: 1000 },
] as const;

test("M2 shell panels remain accessible and overflow-free at all five target widths", async ({
  page,
}) => {
  for (const viewport of VIEWPORTS) {
    await page.setViewportSize({ width: viewport.width, height: viewport.height });
    await page.goto("/");
    await page.getByRole("button", { name: "Inspector" }).click();
    await expect(page.getByRole("complementary", { name: "Inspector" })).toBeVisible();
    await page.getByRole("button", { name: "展开 Activity 面板" }).click();
    await expect(page.getByRole("region", { name: "Activity 面板" })).toBeVisible();
    const activitySeparator = page.getByRole("separator", { name: "调整 Activity 高度" });
    await expect(activitySeparator).toBeVisible();
    const activitySize = Number(await activitySeparator.getAttribute("aria-valuenow"));
    await activitySeparator.focus();
    await page.keyboard.press("ArrowUp");
    await expect(activitySeparator).not.toHaveAttribute("aria-valuenow", String(activitySize));
    expect(
      await page.evaluate(() => localStorage.getItem("rip_shell_activity_height_v1")),
    ).not.toBeNull();
    if (viewport.width >= 1280) {
      const inspectorSeparator = page.getByRole("separator", { name: "调整 Inspector 宽度" });
      await expect(inspectorSeparator).toBeVisible();
      const inspectorSize = Number(await inspectorSeparator.getAttribute("aria-valuenow"));
      await inspectorSeparator.focus();
      await page.keyboard.press("ArrowRight");
      const inspectorHandle = await inspectorSeparator.boundingBox();
      if (!inspectorHandle) throw new Error("Inspector splitter has no hit target");
      const beforeDrag = Number(await inspectorSeparator.getAttribute("aria-valuenow"));
      await page.mouse.move(
        inspectorHandle.x + inspectorHandle.width / 2,
        inspectorHandle.y + inspectorHandle.height / 2,
      );
      await page.mouse.down();
      await page.mouse.move(
        inspectorHandle.x + inspectorHandle.width / 2 - 48,
        inspectorHandle.y + inspectorHandle.height / 2,
        { steps: 5 },
      );
      await page.mouse.up();
      await expect(inspectorSeparator).not.toHaveAttribute("aria-valuenow", String(beforeDrag));
      await expect(inspectorSeparator).not.toHaveAttribute("aria-valuenow", String(inspectorSize));
      expect(
        await page.evaluate(() => localStorage.getItem("rip_shell_inspector_width_v1")),
      ).not.toBeNull();
      const persistedInspector = await page.evaluate(() =>
        localStorage.getItem("rip_shell_inspector_width_v1"),
      );
      const persistedActivity = await page.evaluate(() =>
        localStorage.getItem("rip_shell_activity_height_v1"),
      );
      await page.reload();
      await page.getByRole("button", { name: "Inspector" }).click();
      await page.getByRole("button", { name: "展开 Activity 面板" }).click();
      await expect(page.getByRole("separator", { name: "调整 Inspector 宽度" })).toHaveAttribute(
        "aria-valuenow",
        persistedInspector ?? "72",
      );
      await expect(page.getByRole("separator", { name: "调整 Activity 高度" })).toHaveAttribute(
        "aria-valuenow",
        persistedActivity ?? "76",
      );
    } else {
      await expect(page.getByRole("separator", { name: "调整 Inspector 宽度" })).toHaveCount(0);
    }
    const dimensions = await page.evaluate(() => ({
      viewport: document.documentElement.clientWidth,
      content: document.documentElement.scrollWidth,
    }));
    expect(dimensions.content, viewport.name).toBeLessThanOrEqual(dimensions.viewport + 1);
  }
});

test("M2 Activity handles eight real-data states", async ({ page }) => {
  const openTab = async (tab: string) => {
    await page.goto("/");
    await page.getByRole("button", { name: "展开 Activity 面板" }).click();
    await page.getByRole("tab", { name: tab }).click();
  };

  await page.route("**/api/runs*", async (route) => {
    await new Promise((resolve) => setTimeout(resolve, 700));
    await route.fulfill({ json: { available: true, total: 0, items: [] } });
  });
  await openTab("Tool Runs");
  await expect(page.getByText("正在读取最近运行记录…")).toBeVisible();
  await expect(page.getByText("当前没有运行记录。")).toBeVisible();

  await page.unroute("**/api/runs*");
  await page.route("**/api/runs*", (route) =>
    route.fulfill({ json: { available: true, total: 0, items: [] } }),
  );
  await openTab("Tool Runs");
  await expect(page.getByText("当前没有运行记录。")).toBeVisible();

  await page.unroute("**/api/runs*");
  await page.route("**/api/runs*", (route) =>
    route.fulfill({ json: { available: false, message: "Run source offline", items: [] } }),
  );
  await openTab("Tool Runs");
  await expect(page.getByText("Run source offline")).toBeVisible();

  await page.unroute("**/api/runs*");
  await page.route("**/api/runs*", (route) => route.abort());
  await openTab("Tool Runs");
  await expect(page.getByText(/运行记录暂不可用/)).toBeVisible();

  await page.unroute("**/api/runs*");
  await page.route("**/api/runs*", (route) =>
    route.fulfill({
      json: {
        available: true,
        total: 1,
        items: [{ run_id: "run-active", tool_id: "nmap", status: "RUNNING" }],
      },
    }),
  );
  await openTab("Tool Runs");
  await expect(page.getByRole("link", { name: /run-active/ })).toBeVisible();

  await page.unroute("**/api/runs*");
  await page.route("**/api/runs*", (route) =>
    route.fulfill({
      json: {
        available: true,
        total: 1,
        items: [
          { run_id: "run-failed", tool_id: "nuclei", status: "FAILED", error_category: "timeout" },
        ],
      },
    }),
  );
  await openTab("日志");
  await expect(page.getByText("最近失败或超时的 Run 事件")).toBeVisible();
  await expect(page.getByRole("link", { name: /run-failed/ })).toBeVisible();

  await page.route("**/api/runtime?limit=20", (route) =>
    route.fulfill({
      json: {
        available: true,
        total: 1,
        items: [{ node_id: "worker-healthy", health: "ok", heartbeat_age_seconds: 5 }],
      },
    }),
  );
  await openTab("状态");
  await expect(page.getByText("worker-healthy")).toBeVisible();

  await page.unroute("**/api/runtime?limit=20");
  await page.route("**/api/runtime?limit=20", (route) =>
    route.fulfill({
      json: {
        available: true,
        total: 1,
        items: [{ node_id: "worker-degraded", health: "degraded", heartbeat_age_seconds: 90 }],
      },
    }),
  );
  await openTab("状态");
  await expect(page.getByText("worker-degraded")).toBeVisible();
});
