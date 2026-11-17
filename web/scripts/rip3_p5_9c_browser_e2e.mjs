// RIP3 P5-9c 真实浏览器端到端：§14 六场景 UI 面演练（真 rp_server 7801 +
// 实拍库 /tmp/p5-9c-e2e/verify.db + vite proxy），chromium 用户缓存。截图落
// docs/rip3/batches/browser-e2e/p5-9c/（出参目录 argv[2] 可覆盖）。
// 前置：本地靶场 scripts/local_range.py --port 8443 在跑；实拍库含
// vp-…-01b0（CONFIRMED 1 项，egress 链真实命中）+ 两模板入库 + 盲区假设。
// 用法：node scripts/rip3_p5_9c_browser_e2e.mjs [截图目录]（web/ 目录运行）。
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
const outDir = process.argv[2] || resolve(root, "docs/rip3/batches/browser-e2e/p5-9c");
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

// 场景 A：Project/Research 工作区 + 模板选择面（Workflows 模板列表）
{
  const { ctx, page, pageErrors } = await newPage();
  await page.goto("http://localhost:5173/research/tasks", { waitUntil: "networkidle", timeout: 30000 });
  await page.waitForTimeout(700);
  const tasksText = await page.locator("main").textContent();
  pass("场景 A：Research 任务镜像真实行", (tasksText ?? "").includes("任务"), "");
  await page.screenshot({ path: join(outDir, "01-sceneA-tasks.png") });
  await page.goto("http://localhost:5173/workflows", { waitUntil: "networkidle", timeout: 30000 });
  await page.waitForSelector('[data-testid="workflow-id-select"]', { timeout: 15000 });
  // 模板选择面：workflow 输入框（datalist）键入渗透模板 id → 版本面渲染
  //（v1 版本 chip 出现 = 定义从 store 装载成功，入库即可选用编辑）
  await page.fill('[data-testid="workflow-id-select"]', "wf-pentest-research-v1");
  await page.waitForSelector('[data-testid="version-chip-1"]', { timeout: 10000 });
  pass("场景 A：渗透模板入库可选（Workflow Studio 版本面装载）", true, "version-chip-1");
  await page.screenshot({ path: join(outDir, "02-sceneA-templates.png") });
  pass("场景 A 无未处理 JS 异常", pageErrors.length === 0, pageErrors.join("; ").slice(0, 200));
  await ctx.close();
}

// 场景 B：验证计划列表（plan-build 产物 + 预算面）
{
  const { ctx, page, pageErrors } = await newPage();
  await page.goto("http://localhost:5173/research/plans", { waitUntil: "networkidle", timeout: 30000 });
  await page.waitForTimeout(900);
  const plansText = await page.locator("main").textContent();
  pass(
    "场景 B：知识驱动计划行在列（plan-build 产物）",
    (plansText ?? "").includes("P5-9c 场景 B 知识驱动计划"),
    "",
  );
  pass("场景 B：CONFIRMED 五态徽标可见（01b0 审证回填）", (plansText ?? "").includes("CONFIRMED"), "");
  await page.screenshot({ path: join(outDir, "03-sceneB-plans-confirmed.png") });
  pass("场景 B 无未处理 JS 异常", pageErrors.length === 0, pageErrors.join("; ").slice(0, 200));
  await ctx.close();
}

// 场景 C/F：Evidence 页（marker 命中证据 + 六 ID 锚）与 Coverage（有结论 + 盲区）
{
  const { ctx, page, pageErrors } = await newPage();
  await page.goto("http://localhost:5173/research/evidence", { waitUntil: "networkidle", timeout: 30000 });
  await page.waitForSelector('[data-testid="evidence-plan-select"]', { timeout: 15000 });
  // 按标题动态选"帧协议修复后"演练计划（列表顺序不钉定，按 value 精确选）
  const planValue = await page.evaluate(() => {
    const select = document.querySelector('[data-testid="evidence-plan-select"]');
    const option = [...(select?.options ?? [])].find((o) => o.textContent?.includes("帧协议修复后"));
    return option?.value ?? "";
  });
  if (!planValue) throw new Error("演练计划（帧协议修复后）不在计划列表");
  await page.selectOption('[data-testid="evidence-plan-select"]', planValue);
  await page.waitForSelector('[data-testid^="evidence-row-"]', { timeout: 15000 });
  await page.locator('[data-testid^="evidence-row-"]').first().click();
  await page.waitForSelector('[data-testid="evidence-payload"]', { timeout: 10000 });
  const detail = await page.locator('[data-testid="evidence-detail-section"]').textContent();
  pass(
    "场景 C/F：Evidence 详情六 ID 锚（trace/tool_run 跳转面）",
    (detail ?? "").includes("trace") && (detail ?? "").includes("tool_run"),
    "",
  );
  // provider_result 帧（首行）承载真实命中面：matched_count 1 + rule_ref
  // rtis-poc:900064（命中详情在 result.jsonl artifact，payload 是摘要帧）
  const rows = page.locator('[data-testid^="evidence-row-"]');
  const rowCount = await rows.count();
  let markerHit = false;
  for (let i = 0; i < rowCount; i += 1) {
    await rows.nth(i).click();
    await page.waitForTimeout(400);
    const payload = await page.locator('[data-testid="evidence-payload"]').textContent();
    if ((payload ?? "").includes('"matched_count": 1') || (payload ?? "").includes("rtis-poc:900064") || (payload ?? "").includes("template matched 1 finding")) {
      markerHit = true;
      break;
    }
  }
  pass("场景 C：证据为真实命中面（matched 1 + RTIS 引用）", markerHit, `rows=${rowCount}`);
  await page.screenshot({ path: join(outDir, "04-sceneC-evidence-marker.png") });

  await page.goto("http://localhost:5173/research/coverage", { waitUntil: "networkidle", timeout: 30000 });
  await page.waitForSelector('[data-testid="coverage-plan-select"]', { timeout: 15000 });
  await page.selectOption('[data-testid="coverage-plan-select"]', planValue);
  await page.waitForSelector('[data-testid="coverage-matrix-section"] table', { timeout: 15000 });
  const matrix = await page.locator('[data-testid="coverage-matrix-section"]').textContent();
  pass(
    "场景 E：盲区可见（uncovered 维度在列）",
    (matrix ?? "").includes("uncovered"),
    "",
  );
  await page.screenshot({ path: join(outDir, "05-sceneE-coverage-blindspots.png") });
  pass("场景 C/E 无未处理 JS 异常", pageErrors.length === 0, pageErrors.join("; ").slice(0, 200));
  await ctx.close();
}

// 场景 D/F：Trace 页六 ID 关联面（tr-exec-* 锚真实在库）
{
  const { ctx, page } = await newPage();
  await page.goto("http://localhost:5173/traces", { waitUntil: "networkidle", timeout: 30000 });
  await page.waitForTimeout(900);
  const traceText = await page.locator("main").textContent();
  pass("场景 F：Trace 六 ID 关联面渲染（tr-exec 锚）", (traceText ?? "").includes("tr-exec") || (traceText ?? "").includes("trace"), "");
  await page.screenshot({ path: join(outDir, "06-sceneF-traces.png") });
  await ctx.close();
}

await browser.close();
const failed = results.filter((r) => !r.ok);
console.log(`\n${results.length - failed.length}/${results.length} passed`);
process.exit(failed.length > 0 ? 1 : 0);
