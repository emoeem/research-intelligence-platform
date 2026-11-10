import { defineConfig } from "@playwright/test";

/** 视觉与交互门禁配置（P5-10，ROADMAP P5-9 裁剪版）。
 *  浏览器二进制口径（ROADMAP §6.2）：优先用户缓存目录（~/.cache/ms-playwright，
 *  npx playwright install chromium）；本机该下载不可得（CDN 中断，实测两次），
 *  回退驱动系统 chromium（/usr/sbin/chromium，环境已存在、零安装），可用
 *  PW_CHROMIUM 环境变量覆盖。快照基线不本地生成（ROADMAP 风险 3：基线只能由
 *  CI 字体环境生成），本门禁只跑零 console.error/pageerror、横向溢出与
 *  axe AA 扫描断言。 */

const executablePath = process.env.PW_CHROMIUM ?? "/usr/sbin/chromium";

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
