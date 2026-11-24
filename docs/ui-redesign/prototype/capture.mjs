import { chromium } from "../../../web/node_modules/@playwright/test/index.mjs";
import { pathToFileURL } from "node:url";
import { mkdir } from "node:fs/promises";

const root = "/media/emo/code/pensting/research-platform/docs/ui-redesign";
const prototypePath = root + "/prototype/index.html";
const screenshotDir = root + "/screenshots";
await mkdir(screenshotDir, { recursive: true });

const browser = await chromium.launch({
  headless: true,
  executablePath: "/usr/sbin/chromium",
  args: ["--no-sandbox"],
});
const page = await browser.newPage({
  viewport: { width: 1440, height: 900 },
  deviceScaleFactor: 1,
});
const pageErrors = [];
page.on("pageerror", (error) => pageErrors.push(error.message));

async function capture(name) {
  await page.waitForFunction(
    () => !document.querySelector("#toast")?.classList.contains("show"),
    null,
    { timeout: 3000 },
  ).catch(() => {});
  await page.waitForTimeout(100);
  await page.screenshot({
    path: screenshotDir + "/" + name,
    fullPage: false,
    animations: "disabled",
  });
}
async function openView(name) {
  await page.evaluate((view) => {
    const item = document.querySelector(`.nav-item[data-view="${view}"]`);
    if (!item) throw new Error(`Missing navigation item: ${view}`);
    item.dispatchEvent(new MouseEvent("click", { bubbles: true, cancelable: true }));
  }, name);
  await page.locator(`#view-${name}`).waitFor({ state: "visible" });
}

await page.goto(pathToFileURL(prototypePath).href, { waitUntil: "load" });
await page.waitForTimeout(200);
await capture("m1-dashboard-dark-1440x900.png");

await openView("history");
await capture("m1-history-dark-1440x900.png");
const rowCount = await page.locator("#historyTable tbody tr").count();
if (rowCount < 1) throw new Error("HTTP History sample rows are missing");
await page.locator("#historyTable tbody tr").first().click();
await page.locator("#requestCode").waitFor();
await capture("m1-history-selected-dark-1440x900.png");

await page.locator("#dockToggle").click();
await page.locator("#activityDrawer.open").waitFor({ state: "visible" });
await capture("m1-activity-dock-dark-1440x900.png");
await page.locator("#dockToggle").click();
await page.locator("#activityDrawer.open").waitFor({ state: "hidden" });

await page.locator("#historySearch").fill("api.example.test");
await page.waitForTimeout(100);
const filteredRows = await page.locator("#historyTable tbody tr:visible").count();
await page.locator("#historySearch").fill("");
await page.waitForTimeout(100);

await page.locator('[data-view="repeater"]').first().click();
await page.locator("#view-repeater").waitFor({ state: "visible" });
await page.locator("#sendRequest").click();
await page.waitForTimeout(150);
await capture("m1-repeater-response-dark-1440x900.png");

await openView("agents");
await capture("m1-agent-studio-dark-1440x900.png");

await page.locator("#themeToggle").click();
await capture("m1-agent-studio-light-1440x900.png");
await openView("history");
await capture("m1-history-light-1440x900.png");

await page.setViewportSize({ width: 1280, height: 800 });
await capture("m1-history-light-1280x800.png");

await page.setViewportSize({ width: 390, height: 844 });
await capture("m1-history-light-390x844.png");

await page.locator("#mobileNavToggle").click();
await page.locator("body.mobile-nav-open .sidebar").waitFor({ state: "visible" });
await capture("m1-mobile-navigation-light-390x844.png");
await page.locator("#mobileNavBackdrop").click({ position: { x: 320, y: 180 } });
await page.locator("body.mobile-nav-open").waitFor({ state: "detached" });

await page.locator("#historyTable tbody tr").first().click();
await page.locator("body.inspector-open .inspector:not(.hidden)").waitFor({ state: "visible" });
await capture("m1-inspector-mobile-light-390x844.png");
const mobileMetrics = await page.evaluate(() => ({
  viewport: window.innerWidth,
  document: document.documentElement.scrollWidth,
  body: document.body.scrollWidth,
  activeView: document.querySelector(".view:not(.hide)")?.id ?? null,
  inspectorOpen: document.body.classList.contains("inspector-open"),
}));
await page.locator("#inspectorToggle").click();
await page.locator("body.inspector-open").waitFor({ state: "detached" });
await page.locator("#themeToggle").click();
await openView("dashboard");
await capture("m1-dashboard-dark-390x844.png");

const result = {
  pageErrors,
  rowCount,
  filteredRows,
  mobileMetrics,
  screenshots: [
    "m1-dashboard-dark-1440x900.png",
    "m1-history-dark-1440x900.png",
    "m1-history-selected-dark-1440x900.png",
    "m1-activity-dock-dark-1440x900.png",
    "m1-repeater-response-dark-1440x900.png",
    "m1-agent-studio-dark-1440x900.png",
    "m1-agent-studio-light-1440x900.png",
    "m1-history-light-1440x900.png",
    "m1-history-light-1280x800.png",
    "m1-history-light-390x844.png",
    "m1-mobile-navigation-light-390x844.png",
    "m1-inspector-mobile-light-390x844.png",
    "m1-dashboard-dark-390x844.png",
  ],
};
console.log(JSON.stringify(result, null, 2));
await browser.close();
if (pageErrors.length > 0) process.exitCode = 1;
if (mobileMetrics.document > mobileMetrics.viewport) process.exitCode = 1;
