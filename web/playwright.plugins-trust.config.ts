import { existsSync, readdirSync } from "node:fs";
import { join } from "node:path";
import { homedir } from "node:os";
import { defineConfig } from "@playwright/test";

/** P10-2 插件信任级分级确认 Playwright 配置（web/tests/plugins-trust-confirm.spec.ts）：
 *  机器无关浏览器探测三级口径（P8-4 同款，P10-2 复用）：
 *  1. PW_CHROMIUM 环境变量显式注入；
 *  2. 用户级缓存探测（~/.cache/ms-playwright 下 chromium 版本目录内的
 *     chrome-linux64/chrome；npx playwright install chromium 只写用户缓存
 *     目录，零系统级安装）；
 *  3. 回退系统 chromium（/usr/sbin/chromium）。 */

function probeChromium(): string | undefined {
  const cacheRoot = join(homedir(), ".cache", "ms-playwright");
  try {
    if (existsSync(cacheRoot)) {
      for (const entry of readdirSync(cacheRoot).filter((name) => name.startsWith("chromium-"))) {
        for (const inner of ["chrome-linux64", "chrome-linux", "chrome-win"]) {
          const candidate = join(
            cacheRoot,
            entry,
            inner,
            process.platform === "win32" ? "chrome.exe" : "chrome",
          );
          if (existsSync(candidate)) return candidate;
        }
      }
    }
  } catch {
    /* 探测失败走回退 */
  }
  return undefined;
}

const executablePath = process.env.PW_CHROMIUM ?? probeChromium() ?? "/usr/sbin/chromium";

export default defineConfig({
  testDir: "tests",
  testMatch: "plugins-trust-confirm.spec.ts",
  timeout: 60000,
  workers: 1,
  retries: 0,
  fullyParallel: false,
  reporter: [["list"]],
  outputDir: "qa/plugins-trust/.run",
  use: {
    baseURL: "http://127.0.0.1:4188",
    headless: true,
    launchOptions: {
      executablePath,
      args: ["--no-sandbox", "--disable-dev-shm-usage"],
    },
  },
  webServer: {
    command: "node scripts/visual-server.mjs 4188",
    port: 4188,
    reuseExistingServer: false,
    timeout: 15000,
  },
});
