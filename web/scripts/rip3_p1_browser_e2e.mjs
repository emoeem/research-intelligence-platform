// RIP3 P1-5 真实浏览器端到端：/documents 页全链（真 rp_server 7801 + 真 PG
// 容器 + vite proxy），chromium 用户缓存（与视觉门同口径）。
// 用法：node /tmp/rip3_p1_browser_e2e.mjs（web/ 目录运行，截图出参目录 argv[2]）。
import { chromium } from "@playwright/test";
import { readdirSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";
import { mkdirSync, writeFileSync } from "node:fs";

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

const outDir = process.argv[2] || "/tmp/rip3-e2e";
mkdirSync(outDir, { recursive: true });

const browser = await chromium.launch({
  executablePath: probeChromium(),
  headless: true,
});
const context = await browser.newContext({ viewport: { width: 1440, height: 900 } });
const page = await context.newPage();
const pageErrors = [];
page.on("pageerror", (e) => pageErrors.push(String(e)));
const consoleErrors = [];
page.on("console", (m) => {
  if (m.type() === "error") consoleErrors.push(m.text());
});

const results = [];
function pass(name, ok, detail = "") {
  results.push({ name, ok, detail });
  console.log(`${ok ? "[PASS]" : "[FAIL]"} ${name}${detail ? " :: " + detail : ""}`);
}

// 1) 首访：注入 Bearer 令牌（P8-4 localStorage 最小桥）后重载
await page.goto("http://localhost:5173/documents", { waitUntil: "domcontentloaded" });
await page.evaluate(() => localStorage.setItem("rip_auth_token", "p1browser"));
await page.reload({ waitUntil: "domcontentloaded" });

// 2) 上传表单与列表就绪（真 PG 数据：此前 CLI/REST smoke 的文档在列）
await page.waitForSelector('[data-testid="doc-upload-form"]', { timeout: 15000 });
const beforeCount = await page.locator('[data-testid^="doc-card-"]').count();
pass("列表就绪（真 PG 数据经 vite proxy）", beforeCount >= 1, `rows=${beforeCount}`);
await page.waitForTimeout(400);
await page.screenshot({ path: join(outDir, "01-documents-list.png"), fullPage: true });

// 3) 真实文件上传流（setInputFiles → onChange → base64 → POST → rp-doc → PG）
await page.setInputFiles('[data-testid="doc-file"]', "/tmp/browser-upload.md");
await page.fill('[data-testid="doc-title"]', "P1-5 浏览器上传样本");
await page.click('[data-testid="doc-upload-submit"]');
await page.waitForSelector('[data-testid="doc-upload-result"]', { timeout: 15000 });
const resultText = await page.textContent('[data-testid="doc-upload-result"]');
pass("上传流完成（真 rp-doc 子进程 + PG 落库）", /已入库/.test(resultText), resultText.trim());
await page.waitForTimeout(400);
await page.screenshot({ path: join(outDir, "02-upload-result.png"), fullPage: true });

// 4) 列表行数 +1（新文档经 refetch 在列）
await page.waitForTimeout(600);
const afterCount = await page.locator('[data-testid^="doc-card-"]').count();
pass(
  "新文档经 refetch 在列",
  afterCount === beforeCount + 1,
  `before=${beforeCount} after=${afterCount}`,
);

// 5) 点击行 → 版本历史（SUPERSEDED/CURRENT 真数据）
await page.locator('[data-testid^="doc-card-"]').first().click();
await page.waitForSelector('[data-testid="doc-revisions"]', { timeout: 15000 });
const revText = await page.textContent('[data-testid="doc-revisions"]');
pass("版本历史面板（含状态词）", /CURRENT/.test(revText), "");
await page.waitForTimeout(400);
await page.screenshot({ path: join(outDir, "03-revisions.png"), fullPage: true });

// 6) 无文件提交被前端拦截
await page.click('[data-testid="doc-upload-submit"]');
await page.waitForSelector('[data-testid="doc-upload-error"]', { timeout: 5000 });
const errText = await page.textContent('[data-testid="doc-upload-error"]');
pass("无文件提交前端拦截", /请选择文件/.test(errText), errText.trim());

pass("零 pageerror", pageErrors.length === 0, pageErrors.join("; ").slice(0, 200));
pass(
  "零 console.error（鉴权/资源类）",
  consoleErrors.filter((t) => !/401|Failed to load/.test(t)).length === 0,
  consoleErrors.join("; ").slice(0, 200),
);

writeFileSync(
  join(outDir, "summary.json"),
  JSON.stringify({ results, pageErrors, consoleErrors }, null, 2),
  "utf-8",
);
const failed = results.filter((r) => !r.ok).length;
console.log(`SUMMARY pass=${results.length - failed} fail=${failed} out=${outDir}`);
await browser.close();
process.exit(failed === 0 ? 0 : 1);
