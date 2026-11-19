// WSW-2 真实浏览器端到端（自包含版）：脚本自行起停本地代理、靶场、rp_server
// 与进程内 HTTPS 夹具/静态服务，退出时清理全部子进程；所有断言失败统一
// exit 1（含 UI 时序断言）。骨架沿 rip3_p5_9c_browser_e2e.mjs 的 chromium
// 探针模式，截图 ≥4 张入库 docs/rip3/batches/browser-e2e/wsw-2/。
//
// 断言面（WSW-2 验收 §实拍 + UI 时序硬门）：
// - 代理捕获：靶场首页/marker 经代理渲染 + status 计数增量 + 越界 403 文案；
// - CONNECT MITM：HTTPS 夹具端点经代理渲染（ignoreHTTPSErrors 仅渲染路径；
//   信任链证据 = curl --cacert + openssl s_client，脚本不装 CA）；
// - UI 时序：首屏 <3s、页内交互响应 <1s、新增 API 端点 p95 <300ms；
// - 任一断言失败 → exit 1。
//
// 用法：node scripts/rip3_wsw2_browser_e2e.mjs [截图目录]（web/ 目录运行；
// PW_CHROMIUM 可覆盖浏览器路径，缺省探针 ms-playwright 缓存 → /usr/sbin/chromium）。
import { chromium } from "@playwright/test";
import { spawn, spawnSync } from "node:child_process";
import {
  closeSync,
  existsSync,
  mkdirSync,
  openSync,
  readdirSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { createServer as httpServer } from "node:http";
import { request as httpRequest } from "node:http";
import { homedir } from "node:os";
import { join, resolve, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const REPO = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const WEB_DIST = join(REPO, "web", "dist");
const RANGE_PORT = 8443;
const PROXY_PORT = 3128;
const TLS_PORT = 9443;
const API_PORT = 7893;
const STATIC_PORT = 5175;
const MARKER = "RIP3-P59C-LOCAL-RANGE-MARKER";
const TOKEN = "wsw-e2e-token";
const STATE_DIR = join(REPO, "rip-data", "wsw-state");
const CA_DIR = join(REPO, "rip-data", "wsw-ca");
const FIXTURE_DIR = join(REPO, "rip-data", "wsw-fixture");
const E2E_DB = join(REPO, "rip-data", "wsw-e2e.db");
const STATUS_FILE = join(STATE_DIR, "listener-status.json");
const PID_FILE = join(STATE_DIR, "listener.pid");
const RULES = { include: ["127.0.0.1"], allow_private_targets: true };

const outDir =
  process.argv[2] || resolve(REPO, "docs/rip3/batches/browser-e2e/wsw-2");
mkdirSync(outDir, { recursive: true });
mkdirSync(STATE_DIR, { recursive: true });

const results = [];
function pass(name, ok, detail = "") {
  results.push({ name, ok, detail });
  console.log(`${ok ? "[PASS]" : "[FAIL]"} ${name}${detail ? " :: " + detail : ""}`);
}
function failFast(name, detail) {
  pass(name, false, detail);
  console.error("[E2E] 存在失败断言，exit 1");
  cleanup();
  process.exit(1);
}

// ---- 子进程登记与清理 ------------------------------------------------------
const children = [];
const servers = [];
let cleaning = false;
function spawnLogged(cmd, args, opts = {}) {
  const logPath = opts.logPath ?? join(STATE_DIR, `${opts.name ?? "child"}.log`);
  const out = openSync(logPath, "a");
  const child = spawn(cmd, args, { cwd: REPO, stdio: ["ignore", out, out], ...opts });
  children.push({ child, name: opts.name ?? cmd });
  return child;
}
function cleanup() {
  if (cleaning) return;
  cleaning = true;
  for (const s of servers) {
    try { s.close(); } catch {}
  }
  for (const { child } of children) {
    try { process.kill(-child.pid, "SIGTERM"); } catch { try { child.kill("SIGTERM"); } catch {} }
  }
  const deadline = Date.now() + 4000;
  for (const { child } of children) {
    while (child.exitCode === null && child.signalCode === null && Date.now() < deadline) {
      Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, 100);
    }
    if (child.exitCode === null && child.signalCode === null) {
      try { process.kill(-child.pid, "SIGKILL"); } catch { try { child.kill("SIGKILL"); } catch {} }
    }
  }
}
process.on("exit", () => { if (!cleaning) cleanup(); });

function waitMs(ms) {
  Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, ms);
}

// ---- chromium 探针（P5-9c 模式 + 系统 chromium 兜底）-----------------------
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
  if (existsSync("/usr/sbin/chromium")) return "/usr/sbin/chromium";
  return undefined;
}

// ---- HTTP 工具 -------------------------------------------------------------
function fetchText(url, opts = {}) {
  return fetch(url, { ...opts, signal: AbortSignal.timeout(8000) });
}
async function fetchJsonWithTiming(url, opts = {}) {
  const t0 = performance.now();
  const res = await fetch(url, { ...opts, signal: AbortSignal.timeout(8000) });
  const body = await res.text();
  return { ms: performance.now() - t0, status: res.status, body };
}
function waitFor(fn, timeoutMs, everyMs = 250, label = "condition") {
  const deadline = Date.now() + timeoutMs;
  let lastSample = "(no sample)";
  return new Promise((resolveWait, rejectWait) => {
    const tick = async () => {
      let value = null;
      try {
        value = await fn();
        lastSample = String(value).slice(0, 120);
      } catch (e) {
        lastSample = String(e).slice(0, 120);
      }
      if (value) return resolveWait(value);
      if (Date.now() > deadline) return rejectWait(new Error(`waitFor 超时：${label}；末态=${lastSample}`));
      setTimeout(tick, everyMs);
    };
    tick();
  });
}
function p95(samples) {
  const sorted = [...samples].sort((a, b) => a - b);
  return sorted[Math.min(sorted.length, Math.ceil(sorted.length * 0.95)) - 1];
}

// ---- 进程内服务 ------------------------------------------------------------
function generateFixtureCert() {
  // 夹具证书：openssl CLI 现生成（CA:FALSE + serverAuth + IP SAN；rip-data/ 不入库）。
  const cert = join(FIXTURE_DIR, "e2e-fixture.crt");
  const key = join(FIXTURE_DIR, "e2e-fixture.key");
  mkdirSync(FIXTURE_DIR, { recursive: true });
  const gen = spawnSync("openssl", [
    "req", "-x509", "-newkey", "rsa:2048", "-keyout", key, "-out", cert,
    "-days", "2", "-nodes", "-subj", "/CN=127.0.0.1",
    "-addext", "subjectAltName=IP:127.0.0.1,DNS:localhost",
    "-addext", "extendedKeyUsage=serverAuth", "-addext", "basicConstraints=CA:FALSE",
  ], { cwd: REPO });
  if (gen.status !== 0) failFast("openssl 夹具证书生成", String(gen.stderr));
  return cert;
}

async function startHttpsFixture(cert) {
  const { default: https } = await import("node:https");
  const server = https.createServer(
    { cert: readFileSync(cert), key: readFileSync(cert.replace(".crt", ".key")) },
    (_req, res) => {
      res.writeHead(200, { "content-type": "text/plain; charset=utf-8" });
      res.end(MARKER);
    },
  );
  await new Promise((ok) => server.listen(TLS_PORT, "127.0.0.1", ok));
  servers.push(server);
}

async function startStaticServer() {
  // web/dist 静态服务（SPA fallback）+ /api 反代到 rp_server。
  const MIME = { ".html": "text/html", ".js": "text/javascript", ".css": "text/css", ".png": "image/png", ".svg": "image/svg+xml", ".json": "application/json", ".woff2": "font/woff2" };
  const server = httpServer((req, res) => {
    if (req.url.startsWith("/api/")) {
      const proxy = httpRequest(
        { host: "127.0.0.1", port: API_PORT, path: req.url, method: req.method, headers: { ...req.headers, host: `127.0.0.1:${API_PORT}` } },
        (up) => { res.writeHead(up.statusCode, up.headers); up.pipe(res); },
      );
      proxy.on("error", () => { res.writeHead(502); res.end("static proxy error"); });
      req.pipe(proxy);
      return;
    }
    const rel = req.url === "/" ? "/index.html" : req.url.split("?")[0];
    let file = join(WEB_DIST, rel);
    if (!existsSync(file) || !file.startsWith(WEB_DIST)) file = join(WEB_DIST, "index.html");
    try {
      const body = readFileSync(file);
      res.writeHead(200, { "content-type": MIME[join(file).slice(file.lastIndexOf("."))] ?? "application/octet-stream" });
      res.end(body);
    } catch {
      res.writeHead(404); res.end("not found");
    }
  });
  await new Promise((ok) => server.listen(STATIC_PORT, "127.0.0.1", ok));
  servers.push(server);
}

// ---- 主流程 ----------------------------------------------------------------
const RUST_BIN = process.env.RP_WSW_BIN || join(REPO, "target", "debug", "rp-wsw");
let browserCertPath = null;

try {
  if (!existsSync(RUST_BIN)) failFast("rp-wsw 二进制", `${RUST_BIN} 不存在（cargo build -p wsw-runtime --bin rp-wsw）`);
  if (!existsSync(WEB_DIST)) failFast("web/dist", "不存在（先 npm --prefix web run build）");

  // 0) 环境复位：专用 e2e 库 + pid/status 文件清盘（确定性捕获计数）。
  rmSync(E2E_DB, { force: true });
  rmSync(STATUS_FILE, { force: true });
  rmSync(PID_FILE, { force: true });

  // 1) 靶场（loopback only）。
  spawnLogged("python3", ["scripts/local_range.py", "--port", String(RANGE_PORT)], { name: "range" });
  await waitFor(async () => (await fetchText(`http://127.0.0.1:${RANGE_PORT}/p59c-marker`)).status === 200, 15000, 250, "靶场就绪");
  pass("靶场就绪（local_range.py --port 8443，loopback）", true);

  // 2) rp_server（WSW 端点宿主；API p95 被测面）。
  spawnLogged("python3", ["apps/server/rp_server.py"], {
    name: "rp-server",
    env: {
      ...process.env, RP_AUTH_TOKEN: TOKEN, RP_PORT: String(API_PORT),
      RP_DB_PATH: join(REPO, "migrations", "dev-b1.db"),
      RP_WSW_STATE_DIR: STATE_DIR, RP_WSW_CA_DIR: CA_DIR,
      RP_WSW_DB: E2E_DB, RP_WSW_BIND: `127.0.0.1:${PROXY_PORT}`,
    },
  });
  await waitFor(async () => {
      const r = await fetchJsonWithTiming(`http://127.0.0.1:${API_PORT}/api/wsw/ca-status`, { headers: { authorization: `Bearer ${TOKEN}` } });
      return r.status === 200 ? true : `status=${r.status} body=${r.body.slice(0, 60)}`;
    }, 15000, 250, "rp_server 就绪");
  pass("rp_server 就绪（/api/wsw/* 宿主，Bearer 鉴权）", true);

  // 3) 本地代理（只听 loopback；scope include=127.0.0.1 + allow_private；
  //    --upstream-ca 信任进程内 HTTPS 夹具证书 = 上游追加信任面）。
  const fixtureCert = generateFixtureCert();
  spawnLogged(RUST_BIN, [
    "listener", "--bind", `127.0.0.1:${PROXY_PORT}`, "--db", E2E_DB,
    "--ca-dir", CA_DIR, "--rules-json", JSON.stringify(RULES),
    "--status-file", STATUS_FILE, "--pid-file", PID_FILE,
    "--upstream-ca", fixtureCert,
  ], { name: "listener" });
  await waitFor(async () => {
    try {
      const raw = readFileSync(STATUS_FILE, "utf8");
      return JSON.parse(raw).state === "running";
    } catch { return false; }
  }, 20000, 250, "listener running 心跳");
  pass("代理就绪（rp-wsw listener，127.0.0.1:3128，状态心跳 running）", true);

  // 4) 进程内 HTTPS 夹具（marker）+ 静态服务（web/dist + /api 反代）。
  await startHttpsFixture(fixtureCert);
  browserCertPath = fixtureCert;
  await startStaticServer();

  // 5) CA 导出（显式 confirm 端点；顺带验证 confirm 语义负例）。
  const neg = await fetchJsonWithTiming(`http://127.0.0.1:${API_PORT}/api/wsw/ca/export`, {
    method: "POST", headers: { "content-type": "application/json", authorization: `Bearer ${TOKEN}` },
    body: JSON.stringify({}),
  });
  pass("CA 导出无 confirm 被拒（400 confirm_required）", neg.status === 400 && neg.body.includes("confirm_required"), `status=${neg.status}`);
  const exported = await fetchJsonWithTiming(`http://127.0.0.1:${API_PORT}/api/wsw/ca/export`, {
    method: "POST", headers: { "content-type": "application/json", authorization: `Bearer ${TOKEN}` },
    body: JSON.stringify({ confirm: true }),
  });
  pass("CA 导出（confirm=true）", exported.status === 200 && exported.body.includes("BEGIN CERTIFICATE"), `status=${exported.status}`);

  // 6) 浏览器断言。
  const executablePath = probeChromium();
  if (!executablePath) failFast("chromium 探针", "未找到浏览器（设 PW_CHROMIUM）");
  const requestsBefore = JSON.parse(readFileSync(STATUS_FILE, "utf8")).requests_total ?? 0;
  const browser = await chromium.launch({ executablePath, headless: true });

  // 场景 A/B/C：经代理（明文面 + 越界拒）。
  const ctxA = await browser.newContext({ viewport: { width: 1600, height: 1000 }, proxy: { server: `http://127.0.0.1:${PROXY_PORT}` } });
  const pageA = await ctxA.newPage();
  const firstShot0 = performance.now();
  await pageA.goto(`http://127.0.0.1:${RANGE_PORT}/`, { waitUntil: "domcontentloaded", timeout: 15000 });
  const firstShotMs = performance.now() - firstShot0;
  pass("场景 A：靶场首页经代理渲染", (await pageA.locator("body").textContent()).includes("本地靶场"));
  pass("时序：经代理首屏加载 <3s", firstShotMs < 3000, `${firstShotMs.toFixed(0)}ms`);
  await pageA.screenshot({ path: join(outDir, "01-range-index-via-proxy.png") });

  await pageA.goto(`http://127.0.0.1:${RANGE_PORT}/p59c-marker`, { waitUntil: "domcontentloaded", timeout: 15000 });
  pass("场景 A：marker 经代理命中", (await pageA.locator("body").textContent()).trim() === MARKER);
  await pageA.screenshot({ path: join(outDir, "02-marker-via-proxy.png") });

  await pageA.goto("http://example.com:80/", { waitUntil: "domcontentloaded", timeout: 15000 }).catch(() => {});
  await waitMs(400);
  pass("场景 C：越界请求代理 403（out_of_scope_blocked 文案）", (await pageA.locator("body").textContent().catch(() => "")).includes("out_of_scope_blocked"));
  await pageA.screenshot({ path: join(outDir, "03-out-of-scope-403.png") });

  // 场景 D：CONNECT MITM（HTTPS 夹具）。
  const ctxB = await browser.newContext({ viewport: { width: 1600, height: 1000 }, ignoreHTTPSErrors: true, proxy: { server: `http://127.0.0.1:${PROXY_PORT}` } });
  const pageB = await ctxB.newPage();
  await pageB.goto(`https://127.0.0.1:${TLS_PORT}/p59c-marker`, { waitUntil: "domcontentloaded", timeout: 15000 });
  pass("场景 D：CONNECT MITM HTTPS marker 渲染", (await pageB.locator("body").textContent()).trim() === MARKER);
  await pageB.screenshot({ path: join(outDir, "04-https-connect-mitm.png") });
  await ctxA.close();
  await ctxB.close();

  // 场景 E：UI 时序（静态 dist + API 反代；首屏 <3s / 交互 <1s）。
  const ctxC = await browser.newContext({ viewport: { width: 1600, height: 1000 } });
  const pageC = await ctxC.newPage();
  await pageC.addInitScript(([t]) => localStorage.setItem("rip_auth_token", t), [TOKEN]);
  const uiFirst0 = performance.now();
  await pageC.goto(`http://127.0.0.1:${STATIC_PORT}/settings/wsw-proxy`, { waitUntil: "domcontentloaded", timeout: 15000 });
  await pageC.waitForSelector('[data-testid="wsw-proxy-summary"]', { timeout: 15000 });
  const uiFirstMs = performance.now() - uiFirst0;
  pass("时序：UI 首屏（代理状态页可交互）<3s", uiFirstMs < 3000, `${uiFirstMs.toFixed(0)}ms`);
  await pageC.screenshot({ path: join(outDir, "05-ui-wsw-proxy-page.png") });

  // 交互面 = 证书状态页「导出 PEM」显式动作按钮（confirm 端点 + 响应消息）。
  await pageC.goto(`http://127.0.0.1:${STATIC_PORT}/settings/wsw-cert`, { waitUntil: "domcontentloaded", timeout: 15000 });
  await pageC.waitForSelector('[data-testid="wsw-ca-summary"]', { timeout: 15000 });
  const interact0 = performance.now();
  await pageC.locator('[data-testid="wsw-ca-export"]').click();
  await pageC.waitForSelector('[data-testid="wsw-cert-message"]', { timeout: 10000 });
  const interactMs = performance.now() - interact0;
  pass("时序：页内交互响应（导出 PEM 按钮出确认消息）<1s", interactMs < 1000, `${interactMs.toFixed(0)}ms`);
  await pageC.screenshot({ path: join(outDir, "06-ui-wsw-cert-page.png") });
  await ctxC.close();

  // 场景 F：新增 API 端点 p95 <300ms（20 采样 × 3 端点）。
  const apiEndpoints = ["/api/wsw/proxy-status", "/api/wsw/ca-status", "/api/wsw/intercept-queue"];
  const authHeaders = { authorization: `Bearer ${TOKEN}` };
  for (const endpoint of apiEndpoints) {
    const samples = [];
    for (let i = 0; i < 20; i++) {
      const { ms, status } = await fetchJsonWithTiming(`http://127.0.0.1:${API_PORT}${endpoint}`, { headers: authHeaders });
      if (status !== 200) failFast(`API ${endpoint}`, `采样期间出现非 200（${status}）`);
      samples.push(ms);
    }
    const med = [...samples].sort((a, b) => a - b)[10];
    const tail = p95(samples);
    pass(`时序：API ${endpoint} p95 <300ms`, tail < 300, `p95=${tail.toFixed(1)}ms median=${med.toFixed(1)}ms n=20`);
  }

  // 场景 G：捕获计数增量（status 心跳面证据）。
  const after = JSON.parse(readFileSync(STATUS_FILE, "utf8"));
  pass("捕获计数增量（代理捕获证据）", (after.requests_total ?? 0) > requestsBefore, `requests_total ${requestsBefore} → ${after.requests_total}`);

  await browser.close();
} catch (error) {
  failFast("e2e 未捕获异常", String(error));
} finally {
  cleanup();
}

const failed = results.filter((r) => !r.ok);
console.log(`\n${results.length - failed.length}/${results.length} PASS`);
if (failed.length > 0 || results.length === 0) {
  process.exitCode = 1;
}
closeSync(0);
