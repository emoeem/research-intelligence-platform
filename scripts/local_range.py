#!/usr/bin/env python3
"""P5-9c 本地靶场（受控易受测面，仅 127.0.0.1）。

用途：Phase 5 最终验收（计划 §14 场景 C/F）的真实命中面。低影响原则：
只绑定 127.0.0.1、零安装（纯标准库 http.server）、零外部依赖、响应体
只有一个可检测的固定 marker（信息面特征，无任何真实漏洞语义）。

端面：
- GET /p59c-marker        200，body = RIP3-P59C-LOCAL-RANGE-MARKER（模板
                          scripts/local_range_template.yaml word match 锚）
- GET /                   200，靶场首页（说明文字）
- 其余                    404

用法：python3 scripts/local_range.py [--port 9443]
退出：Ctrl-C；日志 stdout 单行一条（时间戳 路径 状态）。
纪律：本脚本只服务 loopback（硬编码 127.0.0.1，不接受 --host）；
生产/外联面零涉及；用完即停（靶场不是常驻服务）。
"""

import argparse
import http.server
import socketserver

MARKER = "RIP3-P59C-LOCAL-RANGE-MARKER"

PAGE_INDEX = (
    "<!doctype html><html><head><title>RIP3 P5-9c local range</title></head>"
    "<body><h1>RIP3 P5-9c 本地靶场</h1>"
    "<p>受控易受测面（loopback only）。检测锚：/p59c-marker</p>"
    "</body></html>"
).encode()


class RangeHandler(http.server.BaseHTTPRequestHandler):
    server_version = "rip-local-range/0.1"

    def do_GET(self):  # noqa: N802
        if self.path == "/p59c-marker":
            body = MARKER.encode()
            self.send_response(200)
            self.send_header("Content-Type", "text/plain; charset=utf-8")
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)
        elif self.path == "/":
            self.send_response(200)
            self.send_header("Content-Type", "text/html; charset=utf-8")
            self.send_header("Content-Length", str(len(PAGE_INDEX)))
            self.end_headers()
            self.wfile.write(PAGE_INDEX)
        else:
            body = b"not found"
            self.send_response(404)
            self.send_header("Content-Type", "text/plain; charset=utf-8")
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)

    def log_message(self, fmt, *args):  # noqa: A002
        # 单行 stdout 日志（实拍留证消费面），不写文件。
        import datetime

        stamp = datetime.datetime.now().strftime("%H:%M:%S")
        print(f"[{stamp}] {self.address_string()} {fmt % args}", flush=True)


class ReusableThreadingTCPServer(socketserver.ThreadingTCPServer):
    # WSW-3 e2e 实测面：HTTP/1.0 响应后服务端先关连接，子连接 TIME_WAIT
    # 挂 18443 达 2×MSL，本类默认 allow_reuse_address=False → 60s 内重绑
    # EADDRINUSE（e2e/复现脚本快速重跑即踩）。开 REUSEADDR 仅消除
    # TIME_WAIT 重绑阻塞，不改变监听语义。
    allow_reuse_address = True


def main() -> int:
    parser = argparse.ArgumentParser(description="RIP3 P5-9c 本地靶场（loopback only）")
    parser.add_argument("--port", type=int, default=9443)
    args = parser.parse_args()
    # 纪律硬编码：只绑定 127.0.0.1，不提供 --host 覆盖（防误开外联面）。
    host = "127.0.0.1"
    with ReusableThreadingTCPServer((host, args.port), RangeHandler) as httpd:
        print(f"[local-range] listening http://{host}:{args.port}/ （Ctrl-C 停）", flush=True)
        try:
            httpd.serve_forever()
        except KeyboardInterrupt:
            print("[local-range] stopped", flush=True)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
