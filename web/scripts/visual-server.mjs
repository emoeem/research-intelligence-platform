#!/usr/bin/env node
/** 离线静态服务器（视觉矩阵专用）：服务 web/dist + SPA fallback，
 *  不提供任何 API（视觉矩阵在测试内 route-mock 全部 /api/** 请求，
 *  模式沿自参照实现 scripts/visual-qa-server.mjs，TELEGRAM §2.5）。
 *  用法：node scripts/visual-server.mjs [port]（默认 4177）。 */

import { createServer } from "node:http";
import { readFile } from "node:fs/promises";
import { extname, join, normalize } from "node:path";
import { fileURLToPath } from "node:url";

const PORT = Number(process.argv[2] ?? "4177");
const ROOT = join(fileURLToPath(new URL(".", import.meta.url)), "..", "dist");
const HOST = "127.0.0.1";

const MIME = {
  ".html": "text/html; charset=utf-8",
  ".js": "application/javascript; charset=utf-8",
  ".css": "text/css; charset=utf-8",
  ".svg": "image/svg+xml",
  ".json": "application/json; charset=utf-8",
  ".png": "image/png",
  ".ico": "image/x-icon",
  ".woff2": "font/woff2",
};

const server = createServer(async (req, res) => {
  try {
    const url = new URL(req.url ?? "/", `http://${HOST}:${PORT}`);
    let rel = normalize(decodeURIComponent(url.pathname)).replace(/^([/\\])+/, "");
    if (rel === "" || rel === ".") rel = "index.html";
    let file = join(ROOT, rel);
    if (!file.startsWith(ROOT)) {
      res.writeHead(403).end("forbidden");
      return;
    }
    try {
      const body = await readFile(file);
      res.writeHead(200, { "Content-Type": MIME[extname(file)] ?? "application/octet-stream" });
      res.end(body);
      return;
    } catch {
      // SPA fallback：资源路径（assets/ 前缀或已知静态扩展名）404，其余一律
      // 回 index.html。P6-8 修订：原 !extname(rel) 判定会把含点号的 SPA 路由
      // （/plugins/knowledge.rtis-redteam）误判为静态文件而 404。
      if (rel.startsWith("assets/") || MIME[extname(rel)]) {
        res.writeHead(404).end("not found");
        return;
      }
      const body = await readFile(join(ROOT, "index.html"));
      res.writeHead(200, { "Content-Type": MIME[".html"] });
      res.end(body);
    }
  } catch (error) {
    res.writeHead(500).end(`server error: ${error}`);
  }
});

server.listen(PORT, HOST, () => {
  console.log(`[visual-server] serving ${ROOT} at http://${HOST}:${PORT}`);
});
