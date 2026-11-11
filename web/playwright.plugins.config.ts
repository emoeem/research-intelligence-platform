import { defineConfig } from "@playwright/test";

/** P6-8 插件面专用 Playwright 配置（web/tests/plugins.spec.ts）：
 *  - 视觉矩阵与功能点走离线静态服务器 + route-mock（P5-10 同模式）；
 *  - RP_P6_E2E=1 时切换真实后端 E2E（baseURL/token 由环境注入，
 *    apps/server selftest_phase6.sh 起真实 rp_server 后调用）。 */

const e2e = process.env.RP_P6_E2E === "1";

export default defineConfig({
  testDir: "tests",
  testMatch: "plugins.spec.ts",
  timeout: 60000,
  workers: 1,
  retries: 0,
  fullyParallel: false,
  reporter: [["list"]],
  outputDir: "qa/plugins/.run",
  use: {
    baseURL: e2e ? (process.env.RP_P6_BASE_URL ?? "http://127.0.0.1:7893") : "http://127.0.0.1:4178",
    headless: true,
    launchOptions: {
      executablePath: process.env.PW_CHROMIUM ?? "/usr/sbin/chromium",
      args: ["--no-sandbox", "--disable-dev-shm-usage"],
    },
    ...(e2e
      ? {
          extraHTTPHeaders: {
            Authorization: `Bearer ${process.env.RP_P6_TOKEN ?? ""}`,
          },
        }
      : {}),
  },
  ...(e2e
    ? {}
    : {
        webServer: {
          command: "node scripts/visual-server.mjs 4178",
          port: 4178,
          reuseExistingServer: false,
          timeout: 15000,
        },
      }),
});
