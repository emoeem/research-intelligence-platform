#!/usr/bin/env python3
"""PB-007 本地 HTTPS 切换演练夹具：stdlib TLS 终结反代（仅演练用，非生产组件）。

生产配置产物见 containerfiles/https-cutover/（Caddyfile 与 nginx 样例）。
本夹具只证明「rp_server HTTP 入口 + TLS 终结反代 + rip_production_probe」
全链在原生 TLS 下可探活：listen TLS → 转发明文 HTTP。
"""
from __future__ import annotations

import argparse
import http.client
import ssl
import socketserver
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer


class ProxyHandler(BaseHTTPRequestHandler):
    upstream_host: str = "127.0.0.1"
    upstream_port: int = 0

    def _forward(self) -> None:
        length = int(self.headers.get("Content-Length") or 0)
        payload = self.rfile.read(length) if length else None
        conn = http.client.HTTPConnection(self.upstream_host, self.upstream_port, timeout=15)
        headers = {k: v for k, v in self.headers.items() if k.lower() not in ("host",)}
        try:
            conn.request(self.command, self.path, body=payload, headers=headers)
            resp = conn.getresponse()
            data = resp.read()
            self.send_response(resp.status)
            for k, v in resp.getheaders():
                if k.lower() in ("transfer-encoding", "connection"):
                    continue
                self.send_header(k, v)
            self.send_header("Content-Length", str(len(data)))
            self.end_headers()
            if data:
                self.wfile.write(data)
        finally:
            conn.close()

    do_GET = _forward
    do_POST = _forward
    do_HEAD = _forward

    def log_message(self, fmt, *args) -> None:  # 静默演练日志
        pass


class TLSServer(ThreadingHTTPServer):
    def __init__(self, addr, handler, certfile, keyfile):
        super().__init__(addr, handler)
        ctx = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
        ctx.load_cert_chain(certfile, keyfile)
        self.socket = ctx.wrap_socket(self.socket, server_side=True)


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--listen-port", type=int, required=True)
    ap.add_argument("--cert", required=True)
    ap.add_argument("--key", required=True)
    ap.add_argument("--upstream-port", type=int, required=True)
    args = ap.parse_args()

    ProxyHandler.upstream_port = args.upstream_port
    srv = TLSServer(("127.0.0.1", args.listen_port), ProxyHandler, args.cert, args.key)
    try:
        srv.serve_forever()
    except KeyboardInterrupt:
        pass
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
