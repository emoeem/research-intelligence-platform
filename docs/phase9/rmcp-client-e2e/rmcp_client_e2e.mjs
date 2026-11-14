#!/usr/bin/env node
// rmcp 迁移整改批次真实客户端端到端验收（审计 §5 任务 9）：
// 独立 Node 客户端（仅 node: 内建模块，零 npm 依赖）spawn 真实 rp-mcp 二进制，
// 走标准 MCP 2024-11-05 客户端全链：
//   initialize（含版本协商）→ notifications/initialized → tools/list 多页
//   （含 inputSchema 校验）→ tools/call 成功与失败 → 未知工具 → 通知与超时
//   处理 → stdout/stderr 分离 → 进程退出。
// 运行：仓库根执行 `node docs/phase9/rmcp-client-e2e/rmcp_client_e2e.mjs`
// （先 cargo build --offline -p mcp-server --bin rp-mcp）。
// 证据：逐行会话日志 + 结果摘要写入本目录（rmcp-client-e2e/）。

import { spawn } from "node:child_process";
import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { createInterface } from "node:readline";

const REPO = process.cwd();
const MCP_BIN = join(REPO, "target/debug/rp-mcp");
const TOKEN = `rmcp-e2e-${Date.now().toString(36)}`;
const REQUEST_TIMEOUT_MS = 15000;

const transcript = [];
const log = (kind, obj) => {
  const line = JSON.stringify({ ts: new Date().toISOString(), kind, ...obj });
  transcript.push(line);
  process.stdout.write(line + "\n");
};

class McpE2eError extends Error {}
const assert = (cond, name, detail) => {
  if (!cond) throw new McpE2eError(`${name} 失败${detail ? `：${detail}` : ""}`);
  log("assert", { name, ok: true });
};

const runDb = join(mkdtempSync(join(tmpdir(), "rmcp-e2e-")), "run.db");
const child = spawn(MCP_BIN, [], {
  cwd: REPO,
  env: {
    ...process.env,
    RP_MCP_TOKEN: TOKEN,
    RP_DB_PATH: join(REPO, "migrations/platform.db"),
    RP_AUX_DB: join(REPO, "migrations/dev-b5.db"),
    RP_RUN_DB: runDb,
    RP_PLUGIN_DB: "none",
  },
  stdio: ["pipe", "pipe", "pipe"],
});

const stderrChunks = [];
child.stderr.on("data", (c) => stderrChunks.push(c));
child.stdout.on("data", (c) => {
  // stdout 纪律实时校验：每一行都必须是合法 JSON（协议消息独占 stdout）。
  for (const line of c.toString().split("\n")) {
    if (!line.trim()) continue;
    try {
      JSON.parse(line);
      stdoutJsonLines += 1;
    } catch {
      stdoutBadLines.push(line);
    }
  }
});
let stdoutJsonLines = 0;
const stdoutBadLines = [];

const stdin = child.stdin;
let nextId = 1;
const pending = new Map();

const reader = createInterface({ input: child.stdout });
reader.on("line", (line) => {
  if (!line.trim()) return;
  let msg;
  try {
    msg = JSON.parse(line);
  } catch {
    return; // 已由 stdout 纪律计为坏行
  }
  const id = msg.id !== undefined && msg.id !== null ? String(msg.id) : null;
  const entry = id ? pending.get(id) : null;
  if (entry) {
    pending.delete(id);
    if (msg.error) entry.reject(Object.assign(new Error(JSON.stringify(msg.error)), { reply: msg }));
    else entry.resolve(msg);
  }
});

function sendRaw(line) {
  log("client-send", { line: JSON.parse(line) });
  stdin.write(line + "\n");
}

/** 发请求并等待响应（客户端侧超时：超时不复用 token，直接判失败）。 */
function request(method, params, { timeoutMs = REQUEST_TIMEOUT_MS } = {}) {
  const id = nextId++;
  const promise = new Promise((resolve, reject) => {
    pending.set(String(id), { resolve, reject });
    const timer = setTimeout(() => {
      if (pending.has(String(id))) {
        pending.delete(String(id));
        reject(new McpE2eError(`request ${method} 超时（${timeoutMs}ms）`));
      }
    }, timeoutMs);
    if (typeof timer.unref === "function") timer.unref();
  });
  sendRaw(JSON.stringify({ jsonrpc: "2.0", id, method, params }));
  return promise;
}

const notify = (method, params) =>
  sendRaw(JSON.stringify({ jsonrpc: "2.0", method, ...(params ? { params } : {}) }));

const payloadOf = (reply) => JSON.parse(reply.result.content[0].text);

const steps = [];
const step = async (name, fn) => {
  log("step-start", { name });
  const result = await fn();
  steps.push({ name, ok: true });
  log("step-ok", { name, result: result ?? null });
  return result;
};

try {
  // 1. initialize（含版本协商）。
  await step("initialize 2024-11-05 协商回显", async () => {
    const reply = await request("initialize", {
      protocolVersion: "2024-11-05",
      capabilities: {},
      clientInfo: { name: "rmcp-e2e-node", version: "0" },
    });
    const r = reply.result;
    assert(r.protocolVersion === "2024-11-05", "协商回显客户端请求版本");
    assert(r.serverInfo?.name === "rp-mcp", "serverInfo.name");
    assert(r.capabilities?.tools?.listChanged === false, "capabilities.tools.listChanged=false");
    assert(typeof r.instructions === "string" && r.instructions.length < 200, "一行导航 instructions");
    assert(!JSON.stringify(r).includes('"inputSchema"'), "initialize 响应零工具 Schema");
    return r.protocolVersion;
  });

  // 2. notifications/initialized（通知静默：无响应行）。
  await step("notifications/initialized 静默", async () => {
    const before = stdoutJsonLines;
    notify("notifications/initialized");
    await new Promise((r) => setTimeout(r, 300));
    assert(stdoutJsonLines === before, "通知不产生任何 stdout 响应行", `after=${stdoutJsonLines}`);
    return true;
  });

  // 3. 版本协商两路：支持列表内回显 + 列表外回落服务端版本。
  await step("版本协商（2025-11-25 回显 / 1999-01-01 回落 2025-11-25）", async () => {
    const replyNew = await request("initialize", {
      protocolVersion: "2025-11-25",
      capabilities: {},
      clientInfo: { name: "rmcp-e2e-node", version: "0" },
    });
    assert(replyNew.result.protocolVersion === "2025-11-25", "列表内版本协商回显");
    const replyOld = await request("initialize", {
      protocolVersion: "1999-01-01",
      capabilities: {},
      clientInfo: { name: "rmcp-e2e-node", version: "0" },
    });
    assert(replyOld.result.protocolVersion === "2025-11-25", "列表外版本回落服务端声明版本");
    return { negotiatedLatest: "2025-11-25" };
  });

  // 4. tools/list 多页 + inputSchema 校验（P0）。
  let toolsAll = [];
  await step("tools/list 多页遍历与 inputSchema 校验", async () => {
    let cursor;
    let pages = 0;
    do {
      const reply = await request("tools/list", cursor ? { cursor } : {});
      const { tools, nextCursor } = reply.result;
      pages += 1;
      for (const tool of tools) {
        assert(typeof tool.name === "string" && tool.name, "Tool.name 必需非空");
        assert(
          tool.inputSchema && typeof tool.inputSchema === "object" && typeof tool.inputSchema.type === "string",
          `Tool.inputSchema 必需（${tool.name}）`,
        );
      }
      toolsAll = toolsAll.concat(tools);
      cursor = nextCursor;
      assert(pages < 50, "分页游标收敛");
    } while (cursor);
    assert(pages > 1, "真分页（多页）");
    const caps = toolsAll.filter((t) =>
      ["rip.list_tools", "rip.get_tool_schema", "rip.run_tool", "rip.search_knowledge", "rip.get_run_status", "rip.cancel_run"].includes(t.name),
    );
    assert(caps.length === 6, "能力面六条在场");
    return { pages, tools: toolsAll.length };
  });

  // 5. tools/call 成功（rip.get_tool_schema）。
  await step("tools/call 成功（rip.get_tool_schema aux 行 Schema 同源）", async () => {
    const reply = await request("tools/call", {
      name: "rip.get_tool_schema",
      arguments: { tool_id: "rip.kb.rtis" },
    });
    assert(reply.result.isError === false, "isError=false");
    const payload = payloadOf(reply);
    assert(payload.source === "aux", "aux 读面");
    assert(payload.input_schema?.properties?.q?.type === "string", "input_schema 与 tool 表列同源");
    return payload.tool_id;
  });

  // 6. tools/call 失败（rip.run_tool 未登记工具：结构化 isError，不伪造成功）。
  await step("tools/call 结构化失败（run_tool 未登记工具 D-TC-05）", async () => {
    const reply = await request("tools/call", {
      name: "rip.run_tool",
      arguments: { tool_id: "rip.kb.rtis", input: {} },
    });
    assert(reply.result.isError === true, "isError=true（结构化失败）");
    const payload = payloadOf(reply);
    assert(payload.ok === false && /unknown tool/.test(payload.error?.message ?? ""), "拒绝原因 unknown tool");
    return payload.error?.category ?? null;
  });

  // 7. 未知工具（协议错误 -32602）。
  await step("未知工具 -32602", async () => {
    let caught = null;
    try {
      await request("tools/call", { name: "rip.nothere", arguments: {} });
    } catch (e) {
      caught = e.reply ?? { error: { code: null } };
    }
    assert(caught && caught.error?.code === -32602, "JSON-RPC error code -32602");
    return caught.error?.message ?? "";
  });

  // 8. 通知与超时处理：未知通知静默 + 超时口径下后续请求照常应答。
  await step("未知通知静默 + 客户端超时口径下服务恢复", async () => {
    const before = stdoutJsonLines;
    notify("notifications/unknown-notify");
    notify("notifications/progress", { progressToken: "t", progress: 1 });
    await new Promise((r) => setTimeout(r, 300));
    assert(stdoutJsonLines === before, "未知通知不产生 stdout 响应行");
    const reply = await request("ping", {}, { timeoutMs: 3000 });
    assert(reply.result && Object.keys(reply.result).length === 0, "通知风暴后 ping 正常应答");
    return true;
  });

  // 9. stdout/stderr 分离。
  await step("stdout/stderr 分离（stdout 纯协议消息，stderr 日志）", async () => {
    assert(stdoutBadLines.length === 0, "stdout 零坏行", JSON.stringify(stdoutBadLines));
    assert(stdoutJsonLines >= 9, "stdout 协议消息计数（3 initialize + 2 tools/list + 2 tools/call + 1 错误 + 1 ping）", String(stdoutJsonLines));
    const stderrText = Buffer.concat(stderrChunks).toString();
    assert(stderrText.includes("[rp-mcp] ready"), "stderr 含 ready 日志");
    assert(!stderrText.includes("{"), "stderr 零协议 JSON 泄漏");
    return { stdoutJsonLines };
  });

  // 10. 进程退出（stdin EOF → 退出 0）。
  await step("stdin EOF 进程退出 0", async () => {
    const code = await new Promise((resolve) => {
      stdin.end();
      const timer = setTimeout(() => resolve(-1), REQUEST_TIMEOUT_MS);
      child.on("exit", (c) => {
        clearTimeout(timer);
        resolve(c);
      });
    });
    assert(code === 0, "退出码 0（EOF 收尾）", String(code));
    return code;
  });

  log("summary", { ok: true, steps: steps.length });
  process.stdout.write("\nrmcp-e2e: ALL PASS\n");
  process.exitCode = 0;
} catch (e) {
  log("summary", { ok: false, error: String(e && e.message ? e.message : e) });
  process.stdout.write(`\nrmcp-e2e: FAILED\n${e?.message ?? e}\n`);
  process.exitCode = 1;
} finally {
  try {
    child.kill();
  } catch {}
  const { writeFileSync } = await import("node:fs");
  const url = new URL(import.meta.url);
  const dir = join(url.pathname, "..");
  writeFileSync(join(dir, "e2e-session-latest.jsonl"), transcript.join("\n") + "\n");
}
