import { defineConfig } from "@playwright/test";

/** MA-4 Runs/Agents 页 caller 维度截图与渲染断言专用 Playwright 配置
 *  （web/tests/runs-caller.spec.ts，playwright.plugins.config.ts 同款双模式）：
 *  - 模式 A（默认）：离线静态服务器 + route-mock（P5-10/P6-8 同模式），
 *    截图落 web/qa/runs/（计划 :48「截图/实测入库」交付面）；
 *  - 模式 B（RP_P6_E2E=1）：真实后端（RP_P6_BASE_URL/RP_P6_TOKEN 环境注入，
 *    selftest_ma4.sh §6 起真实 rp_server 后调用；SPA 由 visual-server 提供，
 *    /api/** 经 page.route 代理到真实 rp_server——plugins.spec.ts 模式 B 同款）。
 *
 *  运行：npx playwright test tests/runs-caller.spec.ts -c playwright.runs.config.ts */

const e2e = process.env.RP_P6_E2E === "1";

export default defineConfig({
  testDir: "tests",
  testMatch: "runs-caller.spec.ts",
  timeout: 60000,
  workers: 1,
  retries: 0,
  fullyParallel: false,
  reporter: [["list"]],
  outputDir: "qa/runs/.run",
  use: {
    baseURL: "http://127.0.0.1:4179",
    headless: true,
    launchOptions: {
      executablePath: process.env.PW_CHROMIUM ?? "/usr/sbin/chromium",
      args: ["--no-sandbox", "--disable-dev-shm-usage"],
    },
  },
  webServer: {
    command: "node scripts/visual-server.mjs 4179",
    port: 4179,
    reuseExistingServer: false,
    timeout: 15000,
  },
});
