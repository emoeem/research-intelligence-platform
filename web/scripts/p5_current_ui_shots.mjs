// P5-0 前置素材：当前 UI 现状截图（Phase 5 计划 §9.1 A 组 + §9.4 D 组补充）。
// A1 = Overview 正常态（功能地图/工作区入口/Inspector/Activity 全壳层）；
// A2 = Overview 无 token 错误态（登录状态过期表现）；顺带 projects/research/
// workflows/documents 四页现状（P5-0 审计底账）。落 docs/rip3/ui-reference/current/。
import { chromium } from "@playwright/test";
import { readFileSync, mkdirSync } from "node:fs";
import { resolve, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const token = readFileSync(resolve(root, ".rp-workbench-token"), "utf8").trim();
const outDir = resolve(root, "docs/rip3/ui-reference/current");
mkdirSync(outDir, { recursive: true });

const shots = [
  { url: "http://localhost:5173/", name: "current-dashboard-expired-token.png", auth: false },
  { url: "http://localhost:5173/", name: "current-feature-map.png", auth: true },
  { url: "http://localhost:5173/projects", name: "current-projects.png", auth: true },
  { url: "http://localhost:5173/research", name: "current-research-placeholder.png", auth: true },
  { url: "http://localhost:5173/workflows", name: "current-workflows.png", auth: true },
  { url: "http://localhost:5173/documents", name: "current-documents.png", auth: true },
];

const browser = await chromium.launch();
for (const s of shots) {
  const ctx = await browser.newContext({ viewport: { width: 1600, height: 1000 } });
  const page = await ctx.newPage();
  if (s.auth) {
    await page.addInitScript(([t]) => localStorage.setItem("rip_auth_token", t), [token]);
  }
  await page.goto(s.url, { waitUntil: "networkidle", timeout: 30000 });
  await page.waitForTimeout(1200);
  await page.screenshot({ path: resolve(outDir, s.name), fullPage: false });
  console.log("shot:", s.name);
  await ctx.close();
}
await browser.close();
console.log("done ->", outDir);
