import { existsSync, readdirSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";
import { defineConfig } from "@playwright/test";

/** P9-4 Tool Center MCP 目录同步状态卡专用 Playwright 配置
 *  （web/tests/mcp-status.spec.ts）：
 *  - 模式 A（默认，离线静态服务器 + route-mock，P5-10 同模式）：
 *    baseURL = node scripts/visual-server.mjs 4179（/api/** 在测试内
 *    route-mock），截图落 docs/phase9/browser-e2e/mock/；
 *  - 模式 B（RP_P9_E2E=1，真实后端，P6-8 同款）：SPA 仍由 visual-server
 *    提供，/api/** 由测试内 route 转发到 RP_P9_BASE_URL（真实 rp_server，
 *    Bearer 经 RP_P9_TOKEN 注入），逐区查看状态卡并截图入库。
 *  浏览器二进制机器无关口径（P8-4 同款三级探测）：PW_CHROMIUM 显式注入
 *  优先，用户缓存 ms-playwright 下 chromium 版本目录内的 chrome 可执行探测
 *  （写用户缓存目录，零系统级安装），回退系统 chromium。
 *  运行：npx playwright test tests/mcp-status.spec.ts -c playwright.mcp.config.ts
 *  （模式 B 加 RP_P9_E2E=1 RP_P9_BASE_URL=... RP_P9_TOKEN=...） */

function probeChromium(): string {
  if (process.env.PW_CHROMIUM) return process.env.PW_CHROMIUM;
  const cacheRoot = join(homedir(), ".cache", "ms-playwright");
  try {
    if (existsSync(cacheRoot)) {
      for (const entry of readdirSync(cacheRoot).filter((n) => n.startsWith("chromium-"))) {
        for (const inner of ["chrome-linux64", "chrome-linux"]) {
          const candidate = join(cacheRoot, entry, inner, "chrome");
          if (existsSync(candidate)) return candidate;
        }
      }
    }
  } catch {
    /* 探测失败走回退 */
  }
  return "/usr/sbin/chromium";
}

export default defineConfig({
  testDir: "tests",
  testMatch: "mcp-status.spec.ts",
  timeout: 60000,
  workers: 1,
  retries: 0,
  fullyParallel: false,
  reporter: [["list"]],
  outputDir: "qa/mcp/.run",
  use: {
    baseURL: "http://127.0.0.1:4179",
    headless: true,
    launchOptions: {
      executablePath: probeChromium(),
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
