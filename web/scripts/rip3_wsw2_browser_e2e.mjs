// WSW-2 真实浏览器端到端：chromium 经本地 MITM 代理访问本地靶场
//（scripts/local_range.py --port 8443，loopback only，禁外联）。骨架复制
// rip3_p5_9c_browser_e2e.mjs 模式（playwright chromium 用户缓存探针原样复用）。
// 截图 ≥4 张入库 docs/rip3/batches/browser-e2e/wsw-2/（出参目录 argv[2] 可覆盖）。
//
// 断言面（WSW-2-TASK §四.3.1）：
// - 代理捕获证据：经代理请求命中靶场 marker + 代理状态计数增量
//  （status-file 面由 rp_server /api/wsw/proxy-status 承载，本脚本直读
//   rip-data/wsw-state/listener-status.json 做增量断言，避免依赖 UI 服务）；
// - HTTPS CONNECT MITM：浏览器 ignoreHTTPSErrors 面走通（浏览器信任安装
//   属用户显式动作，本脚本不装 CA；证书信任链证据由 curl --cacert +
//   openssl s_client 承载，见批次报告 E 节）。
//
// 前置：local_range.py --port 8443 与 rp-wsw listener（127.0.0.1:3128）在跑。
// 用法：node scripts/rip3_wsw2_browser_e2e.mjs [截图目录]（web/ 目录运行）。
import { chromium } from "@playwright/test";
import { readFileSync, readdirSync, mkdirSync } from "node:fs";
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
const proxyPort = process.env.WSW_PROXY_PORT || "3128";
const rangePort = process.env.WSW_RANGE_PORT || "8443";
const tlsPort = process.env.WSW_TLS_PORT || "9443";
const outDir =
  process.argv[2] || resolve(root, "docs/rip3/batches/browser-e2e/wsw-2");
mkdirSync(outDir, { recursive: true });

const PROXY = `http://127.0.0.1:${proxyPort}`;
const RANGE = `http://127.0.0.1:${rangePort}`;

function readStatusFile() {
  try {
    return JSON.parse(readFileSync(join(root, "rip-data/wsw-state/listener-status.json"), "utf8"));
  } catch {
    return null;
  }
}

const results = [];
function pass(name, ok, detail = "") {
  results.push({ name, ok, detail });
  console.log(`${ok ? "[PASS]" : "[FAIL]"} ${name}${detail ? " :: " + detail : ""}`);
}

const before = readStatusFile();
const browser = await chromium.launch({
  executablePath: probeChromium(),
  headless: true,
  proxy: { server: PROXY },
});

// 场景 A：明文面——浏览器经代理访问靶场首页与 marker。
{
  const ctx = await browser.newContext({ viewport: { width: 1600, height: 1000 } });
  const page = await ctx.newPage();
  const pageErrors = [];
  page.on("pageerror", (e) => pageErrors.push(String(e)));
  await page.goto(`${RANGE}/`, { waitUntil: "domcontentloaded", timeout: 30000 });
  await page.waitForTimeout(500);
  const indexText = await page.locator("body").textContent();
  pass("场景 A：靶场首页经代理渲染", (indexText ?? "").includes("本地靶场"), "index via proxy");
  await page.screenshot({ path: join(outDir, "01-range-index-via-proxy.png") });

  await page.goto(`${RANGE}/p59c-marker`, { waitUntil: "domcontentloaded", timeout: 30000 });
  await page.waitForTimeout(300);
  const markerText = (await page.locator("body").textContent())?.trim();
  pass(
    "场景 A：marker 经代理命中（RIP3-P59C-LOCAL-RANGE-MARKER）",
    markerText === "RIP3-P59C-LOCAL-RANGE-MARKER",
    markerText?.slice(0, 60) ?? "",
  );
  await page.screenshot({ path: join(outDir, "02-marker-via-proxy.png") });

  // 场景 B：越界目标经代理 403（scope_blocked 可见记录面）。
  await page.goto("http://example.com:80/", { waitUntil: "domcontentloaded", timeout: 30000 }).catch(() => {});
  await page.waitForTimeout(500);
  const blockedText = await page.locator("body").textContent().catch(() => "");
  pass(
    "场景 B：越界请求被代理 403（out_of_scope_blocked）",
    (blockedText ?? "").includes("out_of_scope_blocked"),
    (blockedText ?? "").slice(0, 80),
  );
  await page.screenshot({ path: join(outDir, "03-out-of-scope-403.png") });

  // 场景 C：CONNECT MITM——HTTPS 靶场端点经代理渲染。
  //（浏览器未导入 WSW CA：ignoreHTTPSErrors 仅用于本截图；信任链证据走
  // curl --cacert + openssl，红线禁止脚本静默装 CA。）
  const ctxTls = await browser.newContext({
    viewport: { width: 1600, height: 1000 },
    ignoreHTTPSErrors: true,
  });
  const pageTls = await ctxTls.newPage();
  await pageTls.goto(`https://127.0.0.1:${tlsPort}/p59c-marker`, {
    waitUntil: "domcontentloaded",
    timeout: 30000,
  });
  await pageTls.waitForTimeout(500);
  const tlsText = (await pageTls.locator("body").textContent())?.trim();
  pass(
    "场景 C：CONNECT MITM HTTPS marker 渲染",
    tlsText === "RIP3-P59C-LOCAL-RANGE-MARKER",
    tlsText?.slice(0, 60) ?? "",
  );
  await pageTls.screenshot({ path: join(outDir, "04-https-connect-mitm.png") });

  // 场景 D：UI 代理状态页（web build 后由静态服务提供；此处直接断言状态文件
  // 计数增量作为代理捕获证据，UI 截图由 05/06 两个 UI 页面场景承载）。
  const after = readStatusFile();
  const beforeReq = before?.requests_total ?? 0;
  const afterReq = after?.requests_total ?? 0;
  pass(
    "场景 D：代理捕获计数增量（status-file 心跳面证据）",
    afterReq > beforeReq,
    `requests_total ${beforeReq} → ${afterReq}`,
  );
  pass("场景 A 无未处理 JS 异常", pageErrors.length === 0, pageErrors.join("; ").slice(0, 200));
  await ctx.close();
  await ctxTls.close();
}

await browser.close();

const failed = results.filter((r) => !r.ok);
console.log(`\n${results.length - failed.length}/${results.length} PASS`);
if (failed.length > 0) {
  process.exitCode = 1;
}
