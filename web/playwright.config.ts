import { existsSync, readdirSync } from "node:fs";
import { join } from "node:path";
import { homedir } from "node:os";
import { defineConfig } from "@playwright/test";

/** 视觉与交互门禁配置（P5-10，ROADMAP P5-9 裁剪版；P8-4 浏览器二进制
 *  机器无关探测）：
 *  1. PW_CHROMIUM 环境变量显式注入（机器无关口径，CI/新机器优先）；
 *  2. 用户级缓存探测（~/.cache/ms-playwright 下 chromium 版本目录内的
 *     chrome-linux64/chrome，npx playwright install chromium 只写用户缓存
 *     目录，零系统级安装）；
 *  3. 回退系统 chromium（/usr/sbin/chromium，部分发行版环境已存在）。
 *  快照基线不本地生成（ROADMAP 风险 3：基线只能由 CI 字体环境生成），本门禁
 *  只跑零 console.error/pageerror、横向溢出与 axe AA 扫描断言。 */

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
  testDir: "qa/visual",
  timeout: 30000,
  workers: 1,
  retries: 0,
  fullyParallel: false,
  reporter: [["list"]],
  use: {
    headless: true,
    launchOptions: {
      executablePath,
      args: ["--no-sandbox", "--disable-dev-shm-usage"],
    },
  },
  webServer: {
    command: "node scripts/visual-server.mjs 4177",
    port: 4177,
    reuseExistingServer: false,
    timeout: 15000,
  },
});
