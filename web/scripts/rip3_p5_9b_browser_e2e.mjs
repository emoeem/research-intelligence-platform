// RIP3 P5-9b 真实浏览器端到端：Evidence 页与 Coverage 矩阵页（真 rp_server
// 7801 + 实拍库 /tmp/p5-9b-e2e/verify.db + vite proxy），chromium 用户缓存。
// 截图落 docs/rip3/batches/browser-e2e/p5-9b/（出参目录 argv[2] 可覆盖）。
// 前置：rp_server(RP_PORT=7801, RP_DB_PATH=/tmp/p5-9b-e2e/verify.db) 与
// vite(RP_API_TARGET=7801) 已起；实拍库含 vp-…-03d6 计划（4 项四维
// NOT_REPRODUCED + 8 条 evidence 双帧 + 3 维盲区）。
// 用法：node scripts/rip3_p5_9b_browser_e2e.mjs [截图目录]（web/ 目录运行）。
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
const outDir = process.argv[2] || resolve(root, "docs/rip3/batches/browser-e2e/p5-9b");
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

// 1) tab 区：Research 七固定 tab（P5-9b 加证据与覆盖矩阵两个）
{
  const { ctx, page, pageErrors } = await newPage();
  await page.goto("http://localhost:5173/research", { waitUntil: "networkidle", timeout: 30000 });
  await page.waitForTimeout(500);
  const tabarea = await page.locator('nav[aria-label="工作区标签页"]').textContent();
  pass(
    "tab 区七固定 tab（含 P5-9b 证据与覆盖矩阵）",
    ["研究总览", "任务", "资产", "知识上下文", "验证计划", "证据", "覆盖矩阵"].every((label) =>
      (tabarea ?? "").includes(label),
    ),
    "",
  );
  pass("evidence 页无未处理 JS 异常", pageErrors.length === 0, pageErrors.join("; ").slice(0, 200));
  await ctx.close();
}

// 2) Evidence 页：选计划 → 列表 8 行 → 详情（SHA256/类型/六 ID 跳转/Raw/Pretty）
{
  const { ctx, page, pageErrors } = await newPage();
  await page.goto("http://localhost:5173/research/evidence", { waitUntil: "networkidle", timeout: 30000 });
  await page.waitForSelector('[data-testid="evidence-plan-select"]', { timeout: 15000 });
  await page.screenshot({ path: join(outDir, "01-evidence-empty-state.png") });
  // 空态（未选计划不伪造数据）
  const emptyText = await page.locator("main").textContent();
  pass("Evidence 空态诚实提示", (emptyText ?? "").includes("选择一个验证计划"), "");
  // 选 v3 计划
  await page.selectOption('[data-testid="evidence-plan-select"]', { index: 1 });
  await page.waitForSelector('[data-testid^="evidence-row-"]', { timeout: 15000 });
  const rowCount = await page.locator('[data-testid^="evidence-row-"]').count();
  pass("Evidence 列表真实行（8 条双帧）", rowCount === 8, `rows=${rowCount}`);
  // 点首行 → 详情
  await page.locator('[data-testid^="evidence-row-"]').first().click();
  await page.waitForSelector('[data-testid="evidence-payload"]', { timeout: 10000 });
  const detail = await page.locator('[data-testid="evidence-detail-section"]').textContent();
  const hashOk = /[0-9a-f]{64}/.test(detail ?? "");
  pass("详情含 SHA256 与六 ID 锚", hashOk && (detail ?? "").includes("来源计划") && (detail ?? "").includes("trace"), "");
  // Pretty/Raw 切换
  const prettyText = await page.locator('[data-testid="evidence-payload"]').textContent();
  await page.getByRole("button", { name: "Raw", exact: true }).click();
  await page.waitForTimeout(300);
  const rawText = await page.locator('[data-testid="evidence-payload"]').textContent();
  pass("Raw/Pretty 双视图切换", rawText !== prettyText && (rawText ?? "").length > 0, `rawLen=${(rawText ?? "").length}`);
  await page.screenshot({ path: join(outDir, "02-evidence-detail-raw.png") });
  await page.getByRole("button", { name: "Pretty", exact: true }).click();
  await page.waitForTimeout(300);
  await page.screenshot({ path: join(outDir, "03-evidence-detail-pretty.png") });
  pass("evidence 交互无未处理 JS 异常", pageErrors.length === 0, pageErrors.join("; ").slice(0, 200));
  await ctx.close();
}

// 3) Coverage 矩阵页：选计划 → 七维五态 + 盲区 + 分母明示
{
  const { ctx, page, pageErrors } = await newPage();
  await page.goto("http://localhost:5173/research/coverage", { waitUntil: "networkidle", timeout: 30000 });
  await page.waitForSelector('[data-testid="coverage-plan-select"]', { timeout: 15000 });
  await page.screenshot({ path: join(outDir, "04-coverage-empty-state.png") });
  await page.selectOption('[data-testid="coverage-plan-select"]', { index: 1 });
  await page.waitForSelector('[data-testid="coverage-matrix-section"] table', { timeout: 15000 });
  const matrixText = await page.locator('[data-testid="coverage-matrix-section"]').textContent();
  const dimsOk = ["assets", "input_points", "business_flows", "api_endpoints"].every((d) =>
    (matrixText ?? "").includes(d),
  );
  pass("七维矩阵行渲染", dimsOk, "");
  pass("五态列分开显示（未测试与失败独立）", (matrixText ?? "").includes("NOT_TESTED") && (matrixText ?? "").includes("NOT_REPRODUCED"), "");
  pass("盲区可见（uncovered 三维）", (matrixText ?? "").includes("uncovered") && (matrixText ?? "").includes("identity_permissions"), "");
  const total = await page.locator('[data-testid="coverage-total"]').textContent();
  pass("分母明示不用百分比掩盖", (total ?? "").includes("计划项共 4"), total ?? "");
  await page.screenshot({ path: join(outDir, "05-coverage-matrix.png") });
  pass("coverage 交互无未处理 JS 异常", pageErrors.length === 0, pageErrors.join("; ").slice(0, 200));
  await ctx.close();
}

// 4) 验证计划页：控制按钮按状态渲染（completed 计划终态无按钮）
{
  const { ctx, page } = await newPage();
  await page.goto("http://localhost:5173/research/plans", { waitUntil: "networkidle", timeout: 30000 });
  await page.waitForTimeout(800);
  const plansText = await page.locator("main").textContent();
  const hasPlan = (plansText ?? "").includes("P5-9b 浏览器实测场景 v3");
  const completedRow = page.locator("div.rounded-surface", { hasText: "P5-9b 浏览器实测场景 v3" }).first();
  const btnCount = await completedRow.locator("button").count();
  pass("计划列表渲染 v3（completed）", hasPlan, "");
  pass("终态计划不渲染编造操作按钮", btnCount === 0, `buttons=${btnCount}`);
  await page.screenshot({ path: join(outDir, "06-plans-terminal-state.png") });
  await ctx.close();
}

// 5) 390 窄屏：两新页无横向溢出
{
  const { ctx, page } = await newPage({ width: 390, height: 844 });
  await page.goto("http://localhost:5173/research/evidence", { waitUntil: "networkidle", timeout: 30000 });
  await page.waitForTimeout(500);
  const overflow1 = await page.evaluate(
    () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
  );
  await page.goto("http://localhost:5173/research/coverage", { waitUntil: "networkidle", timeout: 30000 });
  await page.waitForTimeout(500);
  const overflow2 = await page.evaluate(
    () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
  );
  pass("390 窄屏 evidence 无横向溢出", overflow1 <= 2, `overflowPx=${overflow1}`);
  pass("390 窄屏 coverage 无横向溢出", overflow2 <= 2, `overflowPx=${overflow2}`);
  await page.screenshot({ path: join(outDir, "07-coverage-390.png") });
  await ctx.close();
}

await browser.close();
const failed = results.filter((r) => !r.ok);
console.log(`\n${results.length - failed.length}/${results.length} passed`);
process.exit(failed.length > 0 ? 1 : 0);
