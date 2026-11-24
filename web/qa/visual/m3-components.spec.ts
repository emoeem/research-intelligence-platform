import { expect, test } from "@playwright/test";
import { AxeBuilder } from "@axe-core/playwright";

test("M3 VirtualTable windows a real 10,000-row Trace response and keeps keyboard scrolling", async ({
  page,
}) => {
  const tasks = Array.from({ length: 10_000 }, (_, index) => ({
    task_id: "task-" + String(index).padStart(4, "0"),
    state: index % 2 ? "RUNNING" : "SUCCEEDED",
    task_type: "research",
    target: "https://target-" + index + ".example.test",
  }));
  await page.route("**/api/traces?id=trace-m3-virtual", (route) =>
    route.fulfill({
      json: {
        available: true,
        query_id: "trace-m3-virtual",
        trace_ids: ["trace-m3-virtual"],
        matched_via: "trace_id",
        counts: { tasks: tasks.length },
        tasks,
        agent_runs: [],
        manifests: [],
        context_items: [],
        tool_runs: [],
        artifacts: [],
        evidence: [],
        audit_events: [],
      },
    }),
  );
  await page.goto("/traces?id=trace-m3-virtual");
  const grid = page.getByRole("grid", { name: "任务（research_task）" });
  await expect(grid).toBeVisible();
  const rows = grid.getByTestId("virtual-table-row");
  await expect(rows.first()).toBeVisible();
  expect(await rows.count()).toBeLessThan(40);
  expect(await grid.getAttribute("aria-rowcount")).toBe("10001");
  await grid.focus();
  await page.keyboard.press("End");
  await expect
    .poll(() => page.getByTestId("virtual-table-scroll").evaluate((element) => element.scrollTop))
    .toBeGreaterThan(1000);
});

test("M3 DataTable column resizing supports keyboard and pointer and persists", async ({
  page,
}) => {
  await page.route("**/api/tasks?limit=200", (route) =>
    route.fulfill({
      json: {
        total: 1,
        items: [
          {
            task_id: "task-one",
            title: "Task one",
            state: "RUNNING",
            task_type: "research",
            target: "example.test",
            objective: "audit",
          },
        ],
        by_state: { RUNNING: 1 },
        by_type: { research: 1 },
      },
    }),
  );
  await page.goto("/research/tasks");
  const handle = page.getByRole("separator", { name: /调整 .* 列宽/ }).first();
  await expect(handle).toBeVisible();
  const before = Number(await handle.getAttribute("aria-valuenow"));
  await handle.focus();
  await page.keyboard.press("ArrowRight");
  await expect(handle).not.toHaveAttribute("aria-valuenow", String(before));
  const dragBox = await handle.boundingBox();
  if (!dragBox) throw new Error("DataTable resize handle has no hit target");
  const beforeDrag = Number(await handle.getAttribute("aria-valuenow"));
  await page.mouse.move(dragBox.x + dragBox.width / 2, dragBox.y + dragBox.height / 2);
  await page.mouse.down();
  await page.mouse.move(dragBox.x + dragBox.width / 2 + 40, dragBox.y + dragBox.height / 2, {
    steps: 4,
  });
  await page.mouse.up();
  await expect(handle).not.toHaveAttribute("aria-valuenow", String(beforeDrag));
  expect(
    await page.evaluate(() =>
      Object.keys(localStorage).some((key) => key.startsWith("rip_data_table_v1:")),
    ),
  ).toBe(true);
});

test("M3 DataTable and VirtualTable remain usable at all five target widths", async ({ page }) => {
  const taskResponse = {
    total: 1,
    items: [
      {
        task_id: "task-one",
        title: "Task one",
        state: "RUNNING",
        task_type: "research",
        target: "example.test",
        objective: "audit",
      },
    ],
    by_state: { RUNNING: 1 },
    by_type: { research: 1 },
  };
  const traceRows = Array.from({ length: 1000 }, (_, index) => ({
    task_id: "task-" + index,
    state: "RUNNING",
    task_type: "research",
    target: "target-" + index + ".example.test",
  }));
  await page.route("**/api/tasks?limit=200", (route) => route.fulfill({ json: taskResponse }));
  await page.route("**/api/traces?id=trace-m3-widths", (route) =>
    route.fulfill({
      json: {
        available: true,
        query_id: "trace-m3-widths",
        trace_ids: ["trace-m3-widths"],
        matched_via: "trace_id",
        counts: { tasks: 1000 },
        tasks: traceRows,
        agent_runs: [],
        manifests: [],
        context_items: [],
        tool_runs: [],
        artifacts: [],
        evidence: [],
        audit_events: [],
      },
    }),
  );
  for (const viewport of [
    { width: 360, height: 800 },
    { width: 768, height: 1024 },
    { width: 1024, height: 768 },
    { width: 1280, height: 900 },
    { width: 1600, height: 1000 },
  ]) {
    await page.setViewportSize(viewport);
    await page.goto("/research/tasks");
    await expect(page.getByRole("table", { name: /研究任务/ })).toBeVisible();
    let dimensions = await page.evaluate(() => ({
      width: document.documentElement.clientWidth,
      scroll: document.documentElement.scrollWidth,
    }));
    expect(dimensions.scroll).toBeLessThanOrEqual(dimensions.width + 1);
    await page.goto("/traces?id=trace-m3-widths");
    const grid = page.getByRole("grid", { name: "任务（research_task）" });
    await expect(grid).toBeVisible();
    expect(await grid.getByTestId("virtual-table-row").count()).toBeLessThan(40);
    dimensions = await page.evaluate(() => ({
      width: document.documentElement.clientWidth,
      scroll: document.documentElement.scrollWidth,
    }));
    expect(dimensions.scroll).toBeLessThanOrEqual(dimensions.width + 1);
  }
});

test("M3 component labels and contrast survive all supported locales and themes", async ({
  page,
}) => {
  await page.goto("/");
  await page.route("**/api/tasks?limit=200", (route) =>
    route.fulfill({
      json: {
        total: 1,
        items: [
          {
            task_id: "task-one",
            title: "Task one",
            state: "RUNNING",
            task_type: "research",
            target: "example.test",
            objective: "audit",
          },
        ],
        by_state: { RUNNING: 1 },
        by_type: { research: 1 },
      },
    }),
  );
  for (const locale of ["zh-CN", "en-US", "ja-JP"]) {
    for (const theme of ["rip-dark", "rip-light"]) {
      await page.evaluate(
        ({ locale, theme }) => {
          localStorage.setItem("rip_language", locale);
          localStorage.setItem("rip_theme", theme);
        },
        { locale, theme },
      );
      await page.reload();
      await page.goto("/research/tasks");
      await expect(page.locator("html")).toHaveAttribute("data-theme", theme);
      const expected =
        locale === "en-US"
          ? "Advanced filters"
          : locale === "ja-JP"
            ? "詳細フィルター"
            : "高级筛选";
      await expect(page.getByText(expected, { exact: true })).toBeVisible();
      const axe = await new AxeBuilder({ page })
        .withTags(["wcag2a", "wcag2aa", "wcag21a", "wcag21aa"])
        .analyze();
      expect(
        axe.violations.filter(
          (violation) => violation.impact === "serious" || violation.impact === "critical",
        ),
        locale + " / " + theme,
      ).toEqual([]);
    }
  }
});

test("M3 ResizableSplitView persists keyboard and pointer adjustments and adapts to narrow screens", async ({
  page,
}) => {
  await page.route("**/api/wsw/history?*", (route) =>
    route.fulfill({
      json: {
        rows: [
          {
            exchange_id: "exchange-m3",
            session_id: "session-m3",
            listener_id: "listener-m3",
            sequence_no: 1,
            direction: "outbound",
            source: "proxy",
            protocol: "http",
            http_version: "HTTP/1.1",
            method: "GET",
            scheme: "https",
            authority: "example.test",
            path: "/",
            response_status: 200,
            request_body_len: 0,
            response_body_len: 0,
            started_at: "2026-11-24T10:00:00Z",
            completed_at: "2026-11-24T10:00:01Z",
            duration_ms: 1000,
            scope_decision: "in_scope",
          },
        ],
        next_cursor: "",
        has_more: false,
        total: 1,
        limit: 100,
      },
    }),
  );
  await page.setViewportSize({ width: 1440, height: 900 });
  await page.goto("/websec/history");
  await expect(page.getByTestId("wsw-history-list-panel")).toBeVisible();
  const separator = page.getByRole("separator", {
    name: /HTTP 历史列表.*请求与响应详情 panel divider/,
  });
  await expect(separator).toBeVisible();
  await expect(separator).toHaveAttribute("aria-orientation", "vertical");
  const initial = await separator.getAttribute("aria-valuenow");
  await separator.focus();
  await page.keyboard.press("ArrowRight");
  await expect(separator).not.toHaveAttribute("aria-valuenow", initial ?? "");
  const box = await separator.boundingBox();
  if (!box) throw new Error("ResizableSplitView separator has no hit target");
  const beforeDrag = await separator.getAttribute("aria-valuenow");
  await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
  await page.mouse.down();
  await page.mouse.move(box.x + box.width / 2 + 50, box.y + box.height / 2, { steps: 5 });
  await page.mouse.up();
  await expect(separator).not.toHaveAttribute("aria-valuenow", beforeDrag ?? "");
  expect(
    await page.evaluate(() =>
      localStorage.getItem("rip_split_v1:wsw-history-list-detail:horizontal"),
    ),
  ).not.toBeNull();

  const groupBox = await separator.evaluate((element) =>
    element.parentElement!.getBoundingClientRect().toJSON(),
  );
  const currentBox = await separator.boundingBox();
  if (!currentBox) throw new Error("ResizableSplitView separator has no collapse hit target");
  await page.mouse.move(currentBox.x + currentBox.width / 2, currentBox.y + currentBox.height / 2);
  await page.mouse.down();
  await page.mouse.move(groupBox.x + groupBox.width - 2, currentBox.y + currentBox.height / 2, {
    steps: 8,
  });
  await page.mouse.up();
  await expect
    .poll(async () => Number(await separator.getAttribute("aria-valuenow")))
    .toBeGreaterThan(95);
  const collapsedLayout = await page.evaluate(() =>
    localStorage.getItem("rip_split_v1:wsw-history-list-detail:horizontal"),
  );
  expect(collapsedLayout).not.toBeNull();

  await page.setViewportSize({ width: 360, height: 800 });
  await page.reload();
  await expect(page.getByRole("separator")).toHaveCount(0);
  const mobileTabs = page.getByRole("tablist", { name: "分栏面板" });
  await expect(mobileTabs).toBeVisible();
  const listTab = page.getByRole("tab", { name: "HTTP 历史列表" });
  const detailTab = page.getByRole("tab", { name: "请求与响应详情" });
  await expect(listTab).toHaveAttribute("aria-selected", "true");
  await listTab.focus();
  await page.keyboard.press("ArrowRight");
  await expect(detailTab).toHaveAttribute("aria-selected", "true");
  await expect(page.getByTestId("wsw-history-list-panel")).toBeHidden();
  await detailTab.click();
  await expect(page.getByRole("tabpanel", { name: "请求与响应详情" })).toBeVisible();
  const mobileAxe = await new AxeBuilder({ page })
    .withTags(["wcag2a", "wcag2aa", "wcag21a", "wcag21aa"])
    .analyze();
  expect(
    mobileAxe.violations.filter(
      (violation) => violation.impact === "serious" || violation.impact === "critical",
    ),
  ).toEqual([]);
  await listTab.click();
  await expect(page.getByTestId("wsw-history-list-panel")).toBeVisible();
});
