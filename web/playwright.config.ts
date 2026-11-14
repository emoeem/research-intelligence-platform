import { existsSync, readdirSync } from "node:fs";
import { join } from "node:path";
import { homedir } from "node:os";
import { defineConfig } from "@playwright/test";

/** 视觉与交互门禁配置（P5-10，ROADMAP P5-9 裁剪版；P8-4 浏览器二进制
 *  机器无关探测；P10-4 快照基线落盘）：
 *  1. PW_CHROMIUM 环境变量显式注入（机器无关口径，CI/新机器优先）；
 *  2. 用户级缓存探测（~/.cache/ms-playwright 下 chromium 版本目录内的
 *     chrome-linux64/chrome，npx playwright install chromium 只写用户缓存
 *     目录，零系统级安装）；
 *  3. 回退系统 chromium（/usr/sbin/chromium，部分发行版环境已存在）。
 *  快照基线（P10-4）：viewport 级 toHaveScreenshot，基线统一落
 *  qa/visual/baseline/{spec 文件}/；统一字体环境由两个 spec 在页面内注入
 *  （Noto Sans CJK SC 钉死），animations 禁用。基线重生成：先探测/安装用户
 *  缓存 chromium（npx playwright install chromium，零系统级写入），再
 *  `npm run build && npx playwright test --update-snapshots`（跨机器或字体
 *  环境变更时必须整目录重落，不做单张补丁，参照 a1796e8 教训）。 */

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
  snapshotPathTemplate: "{testDir}/baseline/{testFileName}/{arg}{ext}",
  use: {
    headless: true,
    launchOptions: {
      executablePath,
      args: ["--no-sandbox", "--disable-dev-shm-usage"],
    },
  },
  expect: {
    toHaveScreenshot: {
      animations: "disabled",
      caret: "hide",
      maxDiffPixelRatio: 0.02,
    },
  },
  webServer: {
    command: "node scripts/visual-server.mjs 4177",
    port: 4177,
    reuseExistingServer: false,
    timeout: 15000,
  },
});
