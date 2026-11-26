// WSW-3 真实浏览器端到端（自包含版）：脚本自行起停本地靶场、rp-wsw listener、
// rp_server、静态服务，退出时清理全部子进程；所有断言失败统一 exit 1。
// 骨架沿 rip3_wsw2_browser_e2e.mjs（chromium 探针 + spawnLogged + cleanup），
// 截图 ≥4 张入库 docs/rip3/batches/browser-e2e/wsw-3/。
//
// 断言面（WSW-3-TASK §四.3 全集）：
// 1. 种子：经代理 socket 流量 ≥1000（变体路径/方法/状态）→ /api/wsw/history
//    total ≥1000 断言；末位凭据面种子（Cookie/Authorization，脱敏实测数据面）；
// 2. 虚拟滚动仅视口渲染（DOM 行节点 ≤80）+ 滚动后行更换；
// 3. 过滤防抖 300ms（5 次快速键入 → /api/wsw/history 请求数 ≤2）；
// 4. Body 按需（列表全程零 /body；详情打开后恰一次）+ 详情视图切换
//    （Raw/Pretty/Headers 互斥渲染）+ 交互时序 <1s；
// 5. Send to Repeater 联动（跳转 + 编辑器含 method/path + 版本行
//    origin='from_history' API 核验）；
// 6. Repeater 发送往返（marker 响应 + source=repeater 行 + send 行
//    succeeded，全部 API 核验）；
// 9. Repeater 多 Tab（新建后 tabbar +1）+ 编辑请求改 method/path/body
//    发送往返（POST+体 marker + latest_request_json API 核验）；
// 10. Tab 修改历史保留 + 版本回退（restore → 新 origin='restore' 行，
//     append-only 不覆盖，API 核验）；
// 11. 脱敏展示投影（红线 10）：凭据面 exchange 详情 Headers 视图
//     Authorization/Cookie 头值 = [REDACTED]、非命中头值原样、API 投影
//     redacted_header_names 在场（wsw_message::redact 挂钩，rp-wsw
//     redact-headers 子进程消费）；
// 7. 性能硬门：首屏 <3s / 交互 <1s / 新 API p95 <300ms（20 采样 × ≥3 端点）
//    / 代理增量 <50ms（直连 vs 经代理 curl time_total 中位数差，repeat≥3）。
//
// 用法：node scripts/rip3_wsw3_browser_e2e.mjs [截图目录]（web/ 目录运行；
// PW_CHROMIUM 可覆盖浏览器路径）。
import { chromium } from "@playwright/test";
import { spawn, spawnSync } from "node:child_process";
import { connect as netConnect } from "node:net";
import {
  closeSync,
  existsSync,
  mkdirSync,
  openSync,
  readdirSync,
  readFileSync,
  rmSync,
} from "node:fs";
import { createServer as httpServer } from "node:http";
import { request as httpRequest } from "node:http";
import { homedir } from "node:os";
import { join, resolve, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const REPO = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const WEB_DIST = join(REPO, "web", "dist");
const RANGE_PORT = 18443;
const PROXY_PORT = 13128;
const API_PORT = 17893;
const STATIC_PORT = 15175;
const MARKER = "RIP3-P59C-LOCAL-RANGE-MARKER";
const TOKEN = "wsw3-e2e-token";
const STATE_DIR = join(REPO, "rip-data", "wsw-state");
const CA_DIR = join(REPO, "rip-data", "wsw-ca");
const E2E_DB = join(REPO, "rip-data", "wsw-e2e-3.db");
const STATUS_FILE = join(STATE_DIR, "listener-status.json");
const PID_FILE = join(STATE_DIR, "listener.pid");
const RULES = { include: ["127.0.0.1"], allow_private_targets: true };
const SEED_COUNT = 1100;
const AUTH = { authorization: `Bearer ${TOKEN}` };

const outDir = process.argv[2] || resolve(REPO, "docs/rip3/batches/browser-e2e/wsw-3");
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
    try {
      s.close();
    } catch {}
  }
  for (const { child } of children) {
    try {
      process.kill(-child.pid, "SIGTERM");
    } catch {
      try {
        child.kill("SIGTERM");
      } catch {}
    }
  }
  const deadline = Date.now() + 4000;
  for (const { child } of children) {
    while (child.exitCode === null && child.signalCode === null && Date.now() < deadline) {
      Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, 100);
    }
    if (child.exitCode === null && child.signalCode === null) {
      try {
        process.kill(-child.pid, "SIGKILL");
      } catch {
        try {
          child.kill("SIGKILL");
        } catch {}
      }
    }
  }
}
process.on("exit", () => {
  if (!cleaning) cleanup();
});

// 浏览器交互等待必须让 Node 事件环继续转（Atomics.wait 阻塞主线程会吞
// page.on('request') 的 CDP 事件派发——防抖计数实测踩坑：阻塞期间事件入队、
// page.off 先执行 → 计数恒 0）。改 async sleep。
function waitMs(ms) {
  return new Promise((ok) => setTimeout(ok, ms));
}

// ---- chromium 探针（W2 e2e 同款）-------------------------------------------
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
async function fetchJson(url, opts = {}) {
  const res = await fetch(url, {
    ...opts,
    headers: { ...AUTH, ...(opts.headers ?? {}) },
    signal: AbortSignal.timeout(10000),
  });
  const body = await res.text();
  let json = null;
  try {
    json = JSON.parse(body);
  } catch {}
  return { status: res.status, body, json };
}
function waitFor(fn, timeoutMs, everyMs = 250, label = "condition") {
  const deadline = Date.now() + timeoutMs;
  let lastSample = "(no sample)";
  return new Promise((resolveWait, rejectWait) => {
    const tick = async () => {
      let value = null;
      try {
        value = await fn();
        lastSample = String(value).slice(0, 140);
      } catch (e) {
        lastSample = String(e).slice(0, 140);
      }
      if (value) return resolveWait(value);
      if (Date.now() > deadline)
        return rejectWait(new Error(`waitFor 超时：${label}；末态=${lastSample}`));
      setTimeout(tick, everyMs);
    };
    tick();
  });
}
function p95(samples) {
  const sorted = [...samples].sort((a, b) => a - b);
  return sorted[Math.min(sorted.length, Math.ceil(sorted.length * 0.95)) - 1];
}
function median(samples) {
  const sorted = [...samples].sort((a, b) => a - b);
  const mid = Math.floor(sorted.length / 2);
  return sorted.length % 2 ? sorted[mid] : (sorted[mid - 1] + sorted[mid]) / 2;
}

// ---- 种子流量（每请求一 socket 连接，curl -x 同型；absolute-form 代理语义）----
// 实测依据（2026-11-20 e2e 调试）：local_range.py 为 BaseHTTPRequestHandler
// 默认 protocol_version="HTTP/1.0"（scripts/local_range.py:34），上游每响应后
// 关连接；代理转发面响应为 HTTP/1.0 + 无 CL + 无 chunked（EOF 定界）→
// 客户端连接逐请求关闭。故单连接 keep-alive 种子模式不成立，改为连接-请求-
// 关闭循环；响应定界 CL/chunked 快路径 + socket EOF 兜底（本靶场实态）。
async function seedRequest(writeFn, tag) {
  const socket = netConnect({ host: "127.0.0.1", port: PROXY_PORT });
  await new Promise((ok, err) => {
    socket.once("connect", ok);
    socket.once("error", err);
  });
  let buffer = Buffer.alloc(0);
  let eof = false;
  const onEof = () => {
    eof = true;
  };
  socket.on("data", (chunk) => {
    buffer = Buffer.concat([buffer, chunk]);
  });
  socket.on("close", onEof);
  socket.on("end", onEof);
  const wait = (ms) => new Promise((ok) => setTimeout(ok, ms));
  try {
    socket.write(writeFn());
    const deadline = Date.now() + 8000;
    for (;;) {
      const headEnd = buffer.indexOf("\r\n\r\n");
      if (headEnd >= 0) {
        const head = buffer.subarray(0, headEnd).toString("latin1");
        // MITM tee 面：响应 CL 被移除由 hyper 重定界 → 靶场经代理实态为
        // HTTP/1.0 EOF 定界；明文 CL/chunked 形态并存（快路径都解析）。
        const chunked = /transfer-encoding:\s*chunked/i.test(head);
        const clMatch = head.match(/content-length:\s*(\d+)/i);
        let total = -1;
        if (chunked) {
          const term = buffer.indexOf("0\r\n\r\n", headEnd);
          if (term >= 0) total = term + 5;
        } else if (clMatch) {
          total = headEnd + 4 + Number(clMatch[1]);
        }
        if (total >= 0 && buffer.length >= total) {
          return buffer.subarray(0, total);
        }
      }
      if (eof) return buffer; // EOF 定界（HTTP/1.0 上游面）
      if (Date.now() > deadline) throw new Error(`seed response timeout (${tag})`);
      await wait(2);
    }
  } finally {
    socket.destroy();
  }
}
async function seedTraffic(count) {
  for (let i = 0; i < count; i += 1) {
    const variant = i % 6;
    let writeFn;
    if (variant === 3) {
      // POST 变体（带体；靶场 501/404 均为可捕获状态面）。
      const body = `seed-payload-${i}`;
      writeFn = () =>
        `POST http://127.0.0.1:${RANGE_PORT}/seed/${i} HTTP/1.1\r\n` +
        `Host: 127.0.0.1:${RANGE_PORT}\r\nContent-Type: text/plain\r\n` +
        `Content-Length: ${body.length}\r\nConnection: close\r\n\r\n${body}`;
    } else {
      const path = variant === 0 ? "/p59c-marker" : variant === 1 ? "/" : `/seed/${i}`;
      writeFn = () =>
        `GET http://127.0.0.1:${RANGE_PORT}${path} HTTP/1.1\r\n` +
        `Host: 127.0.0.1:${RANGE_PORT}\r\nConnection: close\r\n\r\n`;
    }
    await seedRequest(writeFn, `#${i}`);
  }
  // 凭据面种子（脱敏实测：Cookie/Authorization 经代理捕获；详情展示投影
  // 期望掩码——wsw_message::redact 挂钩，红线 10；X-Custom-Keep 期望原样）。
  await seedRequest(
    () =>
      `GET http://127.0.0.1:${RANGE_PORT}/cred-e2e-marker HTTP/1.1\r\n` +
      `Host: 127.0.0.1:${RANGE_PORT}\r\n` +
      `Authorization: Bearer wsw3-cred-secret-value\r\n` +
      `Cookie: session=wsw3-cred-session-value; theme=dark\r\n` +
      `X-Custom-Keep: keep-me\r\n` +
      `Connection: close\r\n\r\n`,
    "#cred",
  );
}

// ---- 静态服务（web/dist + /api 反代；W2 e2e 同款）--------------------------
async function startStaticServer() {
  const MIME = {
    ".html": "text/html",
    ".js": "text/javascript",
    ".css": "text/css",
    ".png": "image/png",
    ".svg": "image/svg+xml",
    ".json": "application/json",
    ".woff2": "font/woff2",
  };
  const server = httpServer((req, res) => {
    if (req.url.startsWith("/api/")) {
      const proxy = httpRequest(
        {
          host: "127.0.0.1",
          port: API_PORT,
          path: req.url,
          method: req.method,
          headers: { ...req.headers, host: `127.0.0.1:${API_PORT}` },
        },
        (up) => {
          res.writeHead(up.statusCode, up.headers);
          up.pipe(res);
        },
      );
      proxy.on("error", () => {
        res.writeHead(502);
        res.end("static proxy error");
      });
      req.pipe(proxy);
      return;
    }
    const rel = req.url === "/" ? "/index.html" : req.url.split("?")[0];
    let file = join(WEB_DIST, rel);
    if (!existsSync(file) || !file.startsWith(WEB_DIST)) file = join(WEB_DIST, "index.html");
    try {
      const body = readFileSync(file);
      res.writeHead(200, {
        "content-type": MIME[join(file).slice(file.lastIndexOf("."))] ?? "application/octet-stream",
      });
      res.end(body);
    } catch {
      res.writeHead(404);
      res.end("not found");
    }
  });
  await new Promise((ok) => server.listen(STATIC_PORT, "127.0.0.1", ok));
  servers.push(server);
}

// ---- 主流程 ----------------------------------------------------------------
const RUST_BIN = process.env.RP_WSW_BIN || join(REPO, "target", "debug", "rp-wsw");

try {
  if (!existsSync(RUST_BIN))
    failFast("rp-wsw 二进制", `${RUST_BIN} 不存在（cargo build -p wsw-runtime --bin rp-wsw）`);
  if (!existsSync(WEB_DIST)) failFast("web/dist", "不存在（先 npm --prefix web run build）");

  // 0) 环境复位。
  rmSync(E2E_DB, { force: true });
  rmSync(STATUS_FILE, { force: true });
  rmSync(PID_FILE, { force: true });

  // 1) 靶场（loopback only）。
  spawnLogged("python3", ["scripts/local_range.py", "--port", String(RANGE_PORT)], {
    name: "range-3",
  });
  await waitFor(
    async () =>
      (
        await fetch(`http://127.0.0.1:${RANGE_PORT}/p59c-marker`, {
          signal: AbortSignal.timeout(3000),
        })
      ).status === 200,
    15000,
    250,
    "靶场就绪",
  );
  pass("靶场就绪（local_range.py --port 18443，loopback）", true);

  // 2) rp_server（WSW 端点宿主；RP_ARTIFACTS_DIR 走默认 rip-data/artifacts，
  //    Rust listener CAS 同根 → /body 按需端点读回一致）。
  spawnLogged("python3", ["apps/server/rp_server.py"], {
    name: "rp-server-3",
    env: {
      ...process.env,
      RP_AUTH_TOKEN: TOKEN,
      RP_PORT: String(API_PORT),
      RP_DB_PATH: join(REPO, "migrations", "dev-b1.db"),
      RP_WSW_STATE_DIR: STATE_DIR,
      RP_WSW_CA_DIR: CA_DIR,
      RP_WSW_DB: E2E_DB,
      RP_WSW_BIND: `127.0.0.1:${PROXY_PORT}`,
    },
  });
  await waitFor(
    async () => {
      const r = await fetchJson(`http://127.0.0.1:${API_PORT}/api/wsw/status`);
      return r.status === 503 || r.status === 200 ? true : `status=${r.status}`;
    },
    15000,
    250,
    "rp_server 就绪",
  );
  pass("rp_server 就绪（/api/wsw/* 宿主，Bearer 鉴权）", true);

  // 3) listener（025 迁移即随启动落地；scope include=127.0.0.1 + allow_private）。
  spawnLogged(
    RUST_BIN,
    [
      "listener",
      "--bind",
      `127.0.0.1:${PROXY_PORT}`,
      "--db",
      E2E_DB,
      "--ca-dir",
      CA_DIR,
      "--rules-json",
      JSON.stringify(RULES),
      "--status-file",
      STATUS_FILE,
      "--pid-file",
      PID_FILE,
    ],
    { name: "listener-3" },
  );
  await waitFor(
    async () => {
      try {
        return JSON.parse(readFileSync(STATUS_FILE, "utf8")).state === "running";
      } catch {
        return false;
      }
    },
    20000,
    250,
    "listener running 心跳",
  );
  pass("代理就绪（rp-wsw listener，127.0.0.1:13128，状态心跳 running；025 迁移已落地）", true);

  // 4) 静态服务。
  await startStaticServer();

  // 5) 种子流量 ≥1000（经代理；变体路径/方法/状态）→ total 断言。
  const tSeed0 = Date.now();
  await seedTraffic(SEED_COUNT);
  const seedMs = Date.now() - tSeed0;
  await waitFor(
    async () => {
      const r = await fetchJson(`http://127.0.0.1:${API_PORT}/api/wsw/history?limit=1`);
      // 未达标必须返回 falsy（真值字符串会被 waitFor 当满足条件首轮即解析）。
      return r.json?.total >= SEED_COUNT;
    },
    30000,
    400,
    "025 落库 total≥1000",
  );
  const seedList = await fetchJson(`http://127.0.0.1:${API_PORT}/api/wsw/history?limit=1`);
  pass(
    "种子流量与落库（≥1000 行 025 wsw_exchange）",
    seedList.json.total >= SEED_COUNT,
    `total=${seedList.json.total} seed=${seedMs}ms`,
  );

  // 6) 浏览器断言。
  const executablePath = probeChromium();
  if (!executablePath) failFast("chromium 探针", "未找到浏览器（设 PW_CHROMIUM）");
  const browser = await chromium.launch({ executablePath, headless: true });
  const ctx = await browser.newContext({ viewport: { width: 1600, height: 1000 } });
  const page = await ctx.newPage();
  await page.addInitScript(([t]) => localStorage.setItem("rip_auth_token", t), [TOKEN]);

  // —— 断言 2：History 页首屏 <3s + 虚拟滚动仅视口渲染 ——
  const uiFirst0 = performance.now();
  await page.goto(`http://127.0.0.1:${STATIC_PORT}/websec/history`, {
    waitUntil: "domcontentloaded",
    timeout: 20000,
  });
  await page.waitForSelector('[data-testid="wsw-history-list"]', { timeout: 20000 });
  await waitFor(
    async () => {
      const n = await page.locator('[data-testid="wsw-history-row"]').count();
      return n > 0 ? true : `rows=${n}`;
    },
    20000,
    300,
    "列表首屏行",
  );
  const uiFirstMs = performance.now() - uiFirst0;
  pass("时序：History 首屏（列表行在屏）<3s", uiFirstMs < 3000, `${uiFirstMs.toFixed(0)}ms`);
  const nodeCount1 = await page.locator('[data-testid="wsw-history-row"]').count();
  pass(
    "虚拟滚动：仅视口渲染（DOM 行节点 ≤80）",
    nodeCount1 > 0 && nodeCount1 <= 80,
    `行节点=${nodeCount1}（种子 ${SEED_COUNT}+ 行；窗口+overscan 上界）`,
  );
  await page.screenshot({ path: join(outDir, "01-history-list.png") });

  const firstVisibleBefore = await page
    .locator('[data-testid="wsw-history-row"]')
    .first()
    .getAttribute("data-exchange-id");
  await page.locator('[data-testid="wsw-history-list"]').evaluate((el) => {
    el.scrollTop = el.scrollHeight;
  });
  await waitMs(400);
  const firstVisibleAfter = await page
    .locator('[data-testid="wsw-history-row"]')
    .first()
    .getAttribute("data-exchange-id");
  const nodeCount2 = await page.locator('[data-testid="wsw-history-row"]').count();
  pass(
    "虚拟滚动：滚动后行节点更换（复用窗口）",
    firstVisibleBefore !== firstVisibleAfter && nodeCount2 <= 80,
    `before=${firstVisibleBefore} after=${firstVisibleAfter} 行节点=${nodeCount2}`,
  );
  // 滚回顶部（后续断言回到首页行）。
  await page.locator('[data-testid="wsw-history-list"]').evaluate((el) => {
    el.scrollTop = 0;
  });
  await waitMs(300);

  // —— 断言 3：过滤防抖（5 次快速键入 → /api/wsw/history 请求 ≤2）——
  let historyRequests = 0;
  const onHistoryRequest = (req) => {
    if (req.url().includes("/api/wsw/history")) historyRequests += 1;
  };
  page.on("request", onHistoryRequest);
  historyRequests = 0;
  const qInput = page.locator('[data-testid="wsw-history-filter-q"]');
  await qInput.click();
  for (const ch of "seed1") {
    await qInput.pressSequentially(ch, { delay: 40 });
  }
  await waitMs(900); // 防抖窗口 300ms + 请求完成余量
  page.off("request", onHistoryRequest);
  pass(
    "过滤防抖：5 次快速键入 → 请求 ≤2",
    historyRequests > 0 && historyRequests <= 2,
    `请求数=${historyRequests}（防抖 300ms）`,
  );
  await page.locator('[data-testid="wsw-history-filter-reset"]').click();
  await waitMs(700);

  // —— 断言 4 + 5：Body 按需 + 详情视图切换 + 交互时序 + Send to Repeater ——
  let bodyRequests = 0;
  const onBodyRequest = (req) => {
    if (req.url().includes("/api/wsw/history/") && req.url().includes("/body")) bodyRequests += 1;
  };
  page.on("request", onBodyRequest);
  bodyRequests = 0;
  const firstRow = page.locator('[data-testid="wsw-history-row"]').first();
  const tInteract0 = performance.now();
  await firstRow.click();
  await page.waitForSelector('[data-testid="wsw-history-detail"]', { timeout: 10000 });
  const interactMs = performance.now() - tInteract0;
  pass(
    "时序：详情交互响应（行点击 → 详情面板在屏）<1s",
    interactMs < 1000,
    `${interactMs.toFixed(0)}ms`,
  );
  const bodyAfterDetail = bodyRequests;
  await page.locator('[data-testid="wsw-history-load-request-body"]').click();
  await waitFor(
    async () => (bodyRequests > bodyAfterDetail ? true : "no body request"),
    8000,
    200,
    "body 按需请求",
  );
  const bodyAfterLoad = bodyRequests;
  await waitMs(500);
  pass(
    "Body 按需：详情打开零 /body + 按钮恰一次",
    bodyAfterDetail === 0 && bodyRequests - bodyAfterDetail === 1,
    `详情打开时=${bodyAfterDetail} 加载后=${bodyAfterLoad}`,
  );
  // 详情六视图切换（Raw/Pretty/Headers 互斥渲染；MessageViews testIdPrefix
  // = wsw-history-req 请求侧）。
  await page.locator('[data-testid="wsw-history-req-tab-headers"]').click();
  const headersVisible = await page.locator('[data-testid="wsw-history-req-headers"]').isVisible();
  pass(
    "详情视图切换：Headers 表格渲染（头名+值列在场）",
    headersVisible &&
      (await page.locator('[data-testid="wsw-history-req-headers"]').textContent()).includes(
        "host",
      ),
    `headers 可见=${headersVisible}`,
  );
  await page.locator('[data-testid="wsw-history-req-tab-pretty"]').click();
  pass(
    "详情视图切换：Pretty 视图渲染（互斥面，Raw 退出）",
    (await page.locator('[data-testid="wsw-history-req-pretty"]').isVisible()) &&
      !(await page
        .locator('[data-testid="wsw-history-req-raw"]')
        .isVisible()
        .catch(() => false)),
    "pretty 在屏",
  );
  await page.locator('[data-testid="wsw-history-req-tab-raw"]').click();
  pass(
    "详情视图切换：回 Raw 视图",
    await page.locator('[data-testid="wsw-history-req-raw"]').isVisible(),
    "raw 在屏",
  );
  await page.screenshot({ path: join(outDir, "02-history-detail-body.png") });

  const detailExchange = await firstRow.getAttribute("data-exchange-id");
  await page.locator('[data-testid="wsw-history-send-to-repeater"]').first().click();
  await page.waitForSelector('[data-testid="wsw-repeater-request"]', { timeout: 15000 });
  pass(
    "Send to Repeater：跳转 /websec/repeater",
    page.url().includes("/websec/repeater?tab="),
    page.url(),
  );
  const editorText = await page.locator('[data-testid="wsw-repeater-raw-editor"]').inputValue();
  const linkedTab = page.url().split("tab=")[1] ?? "";
  const tabDetail = await fetchJson(
    `http://127.0.0.1:${API_PORT}/api/wsw/repeater/tabs/${linkedTab}`,
  );
  const topVersion = tabDetail.json?.versions?.[0];
  pass(
    "Send to Repeater：编辑器含 method/path",
    editorText.includes("GET") && editorText.includes("/"),
    `editor 首行=${editorText.split("\n")[0]}`,
  );
  pass(
    "Send to Repeater：版本行 origin=from_history + source_exchange_id（API 核验）",
    topVersion?.origin === "from_history" && topVersion?.source_exchange_id === detailExchange,
    `origin=${topVersion?.origin} source=${topVersion?.source_exchange_id}`,
  );
  await page.screenshot({ path: join(outDir, "03-repeater-from-history.png") });

  // —— 断言 6：Repeater 发送往返 ——
  const rawEditor = page.locator('[data-testid="wsw-repeater-raw-editor"]');
  await rawEditor.fill(`GET /p59c-marker HTTP/1.1\r\nHost: 127.0.0.1:${RANGE_PORT}\r\n\r\n`);
  await page.locator('[data-testid="wsw-repeater-send"]').click();
  await waitFor(
    async () => {
      const text =
        (await page
          .locator('[data-testid="wsw-repeater-response"]')
          .textContent()
          .catch(() => "")) ?? "";
      return text.includes(MARKER) ? true : text.slice(0, 60);
    },
    20000,
    400,
    "Repeater 响应 marker",
  );
  pass("Repeater 发送往返：响应视图含 marker", true);
  await page.screenshot({ path: join(outDir, "04-repeater-send-roundtrip.png") });
  const sendsOfTab = await fetchJson(
    `http://127.0.0.1:${API_PORT}/api/wsw/repeater/sends?tab_id=${linkedTab}`,
  );
  const sendRow = sendsOfTab.json?.sends?.[0];
  pass(
    "Repeater 发送往返：send 行终态 succeeded（API 核验）",
    sendRow?.state === "succeeded",
    `state=${sendRow?.state}`,
  );
  const repeaterRows = await fetchJson(
    `http://127.0.0.1:${API_PORT}/api/wsw/history?source=repeater&limit=5`,
  );
  pass(
    "Repeater 发送往返：History source=repeater 行（API 核验）",
    (repeaterRows.json?.rows?.length ?? 0) >= 1,
    `rows=${repeaterRows.json?.rows?.length} path=${repeaterRows.json?.rows?.[0]?.path}`,
  );
  const repeaterExchange = repeaterRows.json?.rows?.[0]?.exchange_id;
  const sendStatus = await fetchJson(
    `http://127.0.0.1:${API_PORT}/api/wsw/repeater/send-status?send_id=${sendRow.send_id}`,
  );
  pass(
    "send-status 端点：六态 succeeded + exchange 引用一致",
    sendStatus.json?.state === "succeeded" && sendStatus.json?.exchange_id === repeaterExchange,
    `state=${sendStatus.json?.state} exchange=${sendStatus.json?.exchange_id}`,
  );

  // —— 断言 9：Repeater 多 Tab + 编辑请求（改 method/path/body）发送往返 ——
  const tabCountBefore = await page.locator('[data-testid="wsw-repeater-tab"]').count();
  const prevTabId = page.url().split("tab=")[1] ?? "";
  await page.locator('[data-testid="wsw-repeater-new-tab"]').click();
  // 新 Tab 就绪门 = URL 切到新 tab_id（createTab: loadTabs→loadTab→
  // setSearchParams 末位导航；先等导航再编辑，防 fill 落在 loadTab 前被
  // 新 tab 初始 draft 覆盖——首轮实测 latest_version_no=2 但内容为旧草稿）。
  await waitFor(
    async () => {
      const cur = page.url().split("tab=")[1] ?? "";
      return cur && cur !== prevTabId ? true : `url=${page.url()}`;
    },
    10000,
    200,
    "新 Tab 导航",
  );
  await waitMs(400);
  await waitFor(
    async () =>
      (await page.locator('[data-testid="wsw-repeater-tab"]').count()) === tabCountBefore + 1
        ? true
        : `tabs=${await page.locator('[data-testid="wsw-repeater-tab"]').count()}`,
    10000,
    200,
    "新建 Tab tabbar",
  );
  pass(
    "Repeater 多 Tab：新建 Tab 后 tabbar +1",
    true,
    `tabs=${tabCountBefore}→${await page.locator('[data-testid="wsw-repeater-tab"]').count()}`,
  );
  // 新 Tab 内编辑请求：改 method/path/body（POST + /p59c-marker + 文本体）。
  const editedRequest =
    `POST /p59c-marker HTTP/1.1\r\nHost: 127.0.0.1:${RANGE_PORT}\r\n` +
    `Content-Type: text/plain\r\nX-Wsw3-Edit: e2e\r\n\r\nRIP3-EDIT-BODY-E2E`;
  const editor2 = page.locator('[data-testid="wsw-repeater-raw-editor"]');
  await editor2.fill(editedRequest);
  await page.locator('[data-testid="wsw-repeater-send"]').click();
  await waitFor(
    async () => {
      const text =
        (await page
          .locator('[data-testid="wsw-repeater-response"]')
          .textContent()
          .catch(() => "")) ?? "";
      return text.includes(MARKER) ? true : text.slice(0, 60);
    },
    20000,
    400,
    "编辑请求发送响应 marker",
  );
  pass(
    "Repeater 编辑发送：改 method/path/body 后响应视图含 marker",
    true,
    `编辑请求=${editedRequest.split("\r\n")[0]}`,
  );
  await page.screenshot({ path: join(outDir, "05-repeater-multitab-send.png") });
  const activeTabId = page.url().split("tab=")[1] ?? "";
  const editedTabDetail = await fetchJson(
    `http://127.0.0.1:${API_PORT}/api/wsw/repeater/tabs/${activeTabId}`,
  );
  const editedVersions = editedTabDetail.json?.versions ?? [];
  const latestReq = String(editedTabDetail.json?.latest_request_json ?? "");
  // 版本行 body = OutboundRequest base64 内联（体走 base64 JSON 往返），解码比对。
  const latestParsed = (() => {
    try {
      return JSON.parse(latestReq);
    } catch {
      return {};
    }
  })();
  const latestBody = Buffer.from(String(latestParsed.body ?? ""), "base64").toString("utf8");
  pass(
    "Repeater 编辑发送：版本行保留编辑后的 method/target/body（API 核验）",
    latestParsed.method === "POST" &&
      latestParsed.target === "/p59c-marker" &&
      latestBody.includes("RIP3-EDIT-BODY-E2E") &&
      (editedTabDetail.json?.latest_version_no ?? 0) >= 2,
    `latest_version_no=${editedTabDetail.json?.latest_version_no} method=${latestParsed.method} target=${latestParsed.target} body=${latestBody.slice(0, 30)}`,
  );

  // —— 断言 10：Tab 修改历史保留 + 版本回退（append-only + origin=restore）——
  const versionsBefore = editedVersions.length;
  await page.locator('[data-testid="wsw-repeater-version-select"]').selectOption("1");
  await waitFor(
    async () => {
      const d = await fetchJson(
        `http://127.0.0.1:${API_PORT}/api/wsw/repeater/tabs/${activeTabId}`,
      );
      const vs = d.json?.versions ?? [];
      return vs.length === versionsBefore + 1 && vs[0]?.origin === "restore"
        ? true
        : `versions=${vs.length} top=${vs[0]?.origin}`;
    },
    10000,
    300,
    "回退新版本行",
  );
  pass(
    "Tab 修改历史保留：回退 v1 → 新 origin=restore 行（append-only 不覆盖）",
    editedVersions.every((v) => v.version_no >= 1) && versionsBefore >= 2,
    `回退前 ${versionsBefore} 行 → 回退后 ${versionsBefore + 1} 行`,
  );
  await page.screenshot({ path: join(outDir, "06-repeater-version-restore.png") });

  // —— 断言 11：脱敏展示投影（红线 10；wsw_message::redact 挂钩）——
  // 凭据面 exchange（Cookie/Authorization 种子）经 UI 详情 Headers 视图：
  // 命中头值 = 掩码、非命中头值原样、凭据明文零出现。
  await page.goto(`http://127.0.0.1:${STATIC_PORT}/websec/history`, {
    waitUntil: "domcontentloaded",
    timeout: 20000,
  });
  await page.waitForSelector('[data-testid="wsw-history-list"]', { timeout: 20000 });
  const pathFilter = page.locator('[data-testid="wsw-history-filter-path"]');
  await pathFilter.fill("/cred-e2e-marker");
  // 过滤生效门 = 首行文本即凭据行（rows>=1 会被未过滤旧列表满足——首轮
  // 实测点了非凭据行导致脱敏断言假阴性）。
  await waitFor(
    async () => {
      const rows = page.locator('[data-testid="wsw-history-row"]');
      const n = await rows.count();
      if (n === 0) return "rows=0";
      const t = await rows.first().textContent();
      // 未匹配必须返回 falsy（真值字符串会被 waitFor 首轮即解析——G10 同坑）。
      return t.includes("/cred-e2e-marker");
    },
    10000,
    300,
    "路径过滤出凭据行",
  );
  const credRowId = await page
    .locator('[data-testid="wsw-history-row"]')
    .first()
    .getAttribute("data-exchange-id");
  await page.locator('[data-testid="wsw-history-row"]').first().click();
  await page.waitForSelector('[data-testid="wsw-history-detail"]', { timeout: 10000 });
  await waitMs(600);
  const detailShown = await page.locator('[data-testid="wsw-history-detail"]').textContent();
  const totalShown = await page
    .locator('[data-testid="wsw-history-total"]')
    .textContent()
    .catch(() => "?");
  console.log(
    `[diag-cred] credRowId=${credRowId} detailShown=${(detailShown.match(/详情 (wsw-[\w-]+)/) ?? ["", "?"])[1]} total=${totalShown.slice(0, 24)}`,
  );
  await page.locator('[data-testid="wsw-history-req-tab-headers"]').click();
  await waitFor(
    async () =>
      page.locator('[data-testid="wsw-history-req-headers"]').isVisible() ? true : "headers 不可见",
    8000,
    200,
    "Headers 视图",
  );
  const headersText = await page.locator('[data-testid="wsw-history-req-headers"]').textContent();
  pass(
    "脱敏展示：Authorization/Cookie 头值 = 掩码（UI 详情侧）",
    headersText.includes("[REDACTED]") &&
      !headersText.includes("wsw3-cred-secret-value") &&
      !headersText.includes("wsw3-cred-session-value"),
    `掩码=${headersText.includes("[REDACTED]")} 凭据明文零出现=${!headersText.includes("wsw3-cred-secret-value")}`,
  );
  // 头名大小写：MITM Parsed 视图 hyper 归一小写（API 实态 x-custom-keep），
  // 值原样保留——比对大小写不敏感。
  pass(
    "脱敏展示：非命中头值原样保留（X-Custom-Keep: keep-me）",
    headersText.toLowerCase().includes("x-custom-keep") && headersText.includes("keep-me"),
    `keep-me=${headersText.includes("keep-me")} 头名小写在场=${headersText.toLowerCase().includes("x-custom-keep")}`,
  );
  const detailApi = await fetchJson(`http://127.0.0.1:${API_PORT}/api/wsw/history/${credRowId}`);
  const apiAuth = (detailApi.json?.request_headers ?? []).find(
    (h) => h.name?.toLowerCase() === "authorization",
  );
  pass(
    "脱敏展示：API 详情投影掩码 + redacted_header_names 在场（API 核验）",
    apiAuth?.value === "[REDACTED]" &&
      (detailApi.json?.redacted_header_names ?? []).some(
        (n) => n.toLowerCase() === "authorization",
      ),
    `API authorization=${apiAuth?.value} names=${JSON.stringify(detailApi.json?.redacted_header_names)}`,
  );
  await page.screenshot({ path: join(outDir, "07-history-redacted-detail.png") });

  // —— 断言 7：API p95 <300ms（20 采样 × 4 新端点；首屏在 1000+ 行上）——
  const apiEndpoints = [
    `/api/wsw/history?limit=200`,
    `/api/wsw/history/${repeaterExchange}/body?direction=response`,
    `/api/wsw/repeater/send-status?send_id=${sendRow.send_id}`,
    `/api/wsw/repeater/tabs/${linkedTab}`,
  ];
  for (const endpoint of apiEndpoints) {
    const samples = [];
    for (let i = 0; i < 20; i += 1) {
      const t0 = performance.now();
      const r = await fetchJson(`http://127.0.0.1:${API_PORT}${endpoint}`);
      if (r.status !== 200)
        failFast(`API ${endpoint}`, `采样非 200（${r.status}）：${r.body.slice(0, 80)}`);
      samples.push(performance.now() - t0);
    }
    const tail = p95(samples);
    pass(
      `时序：API ${endpoint.split("?")[0].replace("/api/wsw/", "")} p95 <300ms`,
      tail < 300,
      `p95=${tail.toFixed(1)}ms median=${median(samples).toFixed(1)}ms n=20`,
    );
  }
  await ctx.close();

  // —— 断言 8：代理增量 <50ms（直连 vs 经代理 curl time_total 中位数差）——
  function curlTimes(count, viaProxy) {
    const times = [];
    for (let i = 0; i < count; i += 1) {
      const args = [
        "-s",
        "-o",
        "/dev/null",
        "-w",
        "%{time_total}",
        `http://127.0.0.1:${RANGE_PORT}/p59c-marker`,
      ];
      if (viaProxy) args.unshift("-x", `127.0.0.1:${PROXY_PORT}`);
      const r = spawnSync("curl", args, { timeout: 15000 });
      if (r.status === 0) times.push(Number(r.stdout.toString().trim()) * 1000);
    }
    return times;
  }
  const directMs = median(curlTimes(21, false));
  const proxyMs = median(curlTimes(21, true));
  const increment = proxyMs - directMs;
  pass(
    "时序：代理增量 <50ms（直连 vs 经代理中位数差；落库链路在环）",
    increment < 50,
    `direct=${directMs.toFixed(2)}ms proxy=${proxyMs.toFixed(2)}ms 增量=${increment.toFixed(2)}ms（n=21）`,
  );

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
