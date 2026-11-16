// RIP3 P5-1 真实浏览器端到端：Research Workspace 骨架（真 rp_server 7801 +
// dev 库副本 + vite proxy），chromium 用户缓存。截图落
// docs/rip3/batches/browser-e2e/p5-1/（出参目录 argv[2] 可覆盖）。
// 用法：node scripts/rip3_p5_browser_e2e.mjs（web/ 目录运行，栈先起）。
import { chromium } from "@playwright/test";
import { readdirSync, mkdirSync } from "node:fs";
import { homedir } from "node:os";
import { join, resolve, dirname } from "node:path";
import { fileURLToPath } from "node:url";

function probeChromium() {
  if (process.env.PW_CHROMIUM) return process.env.PW_CHROMIUM;
  const cacheRoot = join(homedir(), ".cache", "ms-playwright");
  try {
    for (const entry of readdirSync(cacheRoot).filter((n) => n.startsWith("chromium-"))) {
      for (const inner of ["chrome-linux64", "chrome-linux", "chrome-win"]) {
        const candidate = join(cacheRoot, entry, inner, "chrome");
        try {
          if (readdirSync(join(cacheRoot, entry, inner)).includes("chrome")) return candidate;
        } catch {}
      }
    }
  } catch {}
  return undefined;
}

const root = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const token = process.env.RP_E2E_TOKEN || "p5browser";
const outDir = process.argv[2] || resolve(root, "docs/rip3/batches/browser-e2e/p5-1");
mkdirSync(outDir, { recursive: true });

const browser = await chromium.launch({
  executablePath: probeChromium(),
  headless: true,
});

const results = [];
function pass(name, ok, detail = "") {
  results.push({ name, ok, detail });
  console.log(`${ok ? "[PASS]" : "[FAIL]"} ${name}${detail ? " :: " + detail : ""}`);
}

async function newPage(viewport = { width: 1600, height: 1000 }) {
  const ctx = await browser.newContext({ viewport });
  const page = await ctx.newPage();
  const pageErrors = [];
  page.on("pageerror", (e) => pageErrors.push(String(e)));
  await page.addInitScript(([t]) => localStorage.setItem("rip_auth_token", t), [token]);
  return { ctx, page, pageErrors };
}

// 1) Research 总览（真实 API：/api/tasks + /api/runs + /api/overview）
{
  const { ctx, page, pageErrors } = await newPage();
  await page.goto("http://localhost:5173/research", { waitUntil: "networkidle", timeout: 30000 });
  await page.waitForTimeout(600);
  const kpi = await page.locator('[aria-label="研究概况卡片"], main').first().textContent();
  pass(
    "研究总览渲染（KPI/最近任务）",
    (kpi ?? "").includes("研究任务"),
    `len=${(kpi ?? "").length}`,
  );
  await page.screenshot({ path: join(outDir, "01-research-overview-1600.png") });
  pass("总览无未处理 JS 异常", pageErrors.length === 0, pageErrors.join("; ").slice(0, 200));
  await ctx.close();
}

// 2) 任务列表：真实行数 > 0 + 过滤交互 + 表格
{
  const { ctx, page } = await newPage();
  await page.goto("http://localhost:5173/research/tasks", {
    waitUntil: "networkidle",
    timeout: 30000,
  });
  await page.waitForSelector("table tbody tr", { timeout: 15000 });
  const rowCount = await page.locator("table tbody tr").count();
  pass("任务列表真实行（>0）", rowCount > 0, `rows=${rowCount}`);
  // 状态过滤（选择 COMPLETED 或首个非 all 项后行数变化或保持有效）
  const select = page.locator('select[aria-label="状态"]');
  const options = await select.locator("option").allTextContents();
  pass(
    "状态过滤选项含词表",
    options.some((o) => o.includes("CREATED")),
    options.slice(0, 4).join("/"),
  );
  await select.selectOption({ index: 1 });
  await page.waitForTimeout(300);
  const filteredRows = await page.locator("table tbody tr").count();
  pass("状态过滤生效（行数变化或不超全量）", filteredRows <= rowCount, `filtered=${filteredRows}`);
  await page.screenshot({ path: join(outDir, "02-research-tasks-filtered.png") });
  // 点击首行 → 任务详情深链接
  await select.selectOption({ index: 0 });
  await page.waitForTimeout(300);
  await page.locator("table tbody tr").first().click();
  await page.waitForURL(/\/research\/tasks\/.+/, { timeout: 10000 });
  pass("任务详情深链接", true, page.url());
  await page.waitForTimeout(600);
  await page.screenshot({ path: join(outDir, "03-research-task-detail.png") });
  const detailText = await page.locator("main").textContent();
  pass("详情含状态时间线", (detailText ?? "").includes("状态时间线"), "");
  await ctx.close();
}

// 3) 资产页诚实空态 + 知识上下文检索
{
  const { ctx, page } = await newPage();
  await page.goto("http://localhost:5173/research/assets", {
    waitUntil: "networkidle",
    timeout: 30000,
  });
  await page.waitForTimeout(600);
  const text = await page.locator("main").textContent();
  pass("资产页诚实空态", (text ?? "").includes("资产域尚未落库"), "");
  await page.screenshot({ path: join(outDir, "04-research-assets-empty.png") });

  await page.goto("http://localhost:5173/research/knowledge", {
    waitUntil: "networkidle",
    timeout: 30000,
  });
  await page.fill('input[aria-label="检索"]', "EZVIZ");
  await page.click('button:has-text("检索知识")');
  await page.waitForTimeout(1500);
  await page.waitForSelector("table, ul.space-y-2", { timeout: 10000 });
  const kText = await page.locator("main").textContent();
  const hitCount = (kText ?? "").match(/命中 (\d+) 条/);
  pass(
    "知识上下文检索（真实 /api/knowledge）",
    Boolean(hitCount) && Number(hitCount?.[1]) > 0,
    hitCount ? `hits=${hitCount[1]}` : (kText ?? "").slice(0, 60),
  );
  await page.screenshot({ path: join(outDir, "05-research-knowledge-query.png") });
  await ctx.close();
}

// 4) 1440 与 390 窄屏 + tab 区四 tab
{
  const { ctx, page } = await newPage({ width: 1440, height: 900 });
  await page.goto("http://localhost:5173/research", { waitUntil: "networkidle", timeout: 30000 });
  await page.waitForTimeout(500);
  await page.screenshot({ path: join(outDir, "06-research-overview-1440.png") });
  await ctx.close();
  const { ctx: ctx2, page: page2 } = await newPage({ width: 390, height: 844 });
  await page2.goto("http://localhost:5173/research", { waitUntil: "networkidle", timeout: 30000 });
  await page2.waitForTimeout(500);
  const overflow = await page2.evaluate(
    () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
  );
  pass("390 窄屏无横向溢出", overflow <= 2, `overflowPx=${overflow}`);
  await page2.screenshot({ path: join(outDir, "07-research-overview-390.png") });
  await ctx2.close();
}

// 5) tab 区渲染 research 四固定 tab
{
  const { ctx, page } = await newPage();
  await page.goto("http://localhost:5173/research", { waitUntil: "networkidle", timeout: 30000 });
  await page.waitForTimeout(400);
  const tabarea = await page.locator('nav[aria-label="工作区标签页"]').textContent();
  pass(
    "tab 区四固定 tab",
    ["研究总览", "任务", "资产", "知识上下文"].every((label) => (tabarea ?? "").includes(label)),
    "",
  );
  await ctx.close();
}

await browser.close();
const failed = results.filter((r) => !r.ok);
console.log(`\n${results.length - failed.length}/${results.length} passed`);
process.exit(failed.length > 0 ? 1 : 0);
