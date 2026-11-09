#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
TUN 伪 ACK 环境下的协议级端口判活

问题：开着 sing-box / clash 的 TUN 模式时，任何 TCP connect 都会立即返回成功
      —— 因为 TUN 栈在本地就把 SYN 应答了，真正的握手失败要等几百毫秒后
      才会以「无数据」的形式暴露。于是 nmap -sT 会把所有端口报成 open。

判别法（三层，必须全部通过）：
  L1 TCP connect 成功（这一步在 TUN 环境里没有意义，但要知道它说了什么）
  L2 协议层有响应：针对端口发对应的最小请求，收到合法响应头才算出活
     · 80/8080   → "GET / HTTP/1.0\r\nHost: x\r\n\r\n"，等 "HTTP/"
     · 443/8443  → TLS ClientHello，等 ServerHello（0x16 0x03）
     · 22        → 等 SSH banner "SSH-"
     · 21        → 等 "220"
     · 25/587    → 等 "220"
     · 3306      → 等 MySQL 握手包（首字节 = payload 长度）
     · 6379      → "PING\r\n"，等 "+PONG"
     · 9200      → GET /，等 "You Know, for Search"
  L3 阴性对照：连一个必然关闭的端口，确认它不是「也成功」
     —— 如果关闭端口也 connect 成功，说明 TUN 在骗你，L2 是唯一可信判据

另一个常见坑：nginx 占位端口。
  运维常把未启用的 server 块留成 default_server，任意 Host 都返回
  一个固定欢迎页（"Welcome to nginx" / "It works"）。这类端口「活着但没业务」，
  必须单独标出来，否则会把一堆占位端口当成资产。

用法：
  ./tun-port-probe.py <host> <port[,port,...]|range> [--timeout 5] [--json out.json]
示例：
  ./tun-port-probe.py 10.0.0.5 22,80,443,3306,6379,8080-8090
"""
import os
import socket
import ssl
import sys

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "_lib"))
import common as C  # noqa: E402

# 每个端口的「最小可判别请求」与「期望响应特征」
# 返回 (payload_bytes, expect_regex, proto_name)
HTTP_REQ = b"GET / HTTP/1.0\r\nHost: probe.invalid\r\nUser-Agent: rt-probe/1\r\n\r\n"

PROBES = {
    80:    (HTTP_REQ, rb"^HTTP/1\.[01] \d{3}", "http"),
    8000:  (HTTP_REQ, rb"^HTTP/1\.[01] \d{3}", "http"),
    8080:  (HTTP_REQ, rb"^HTTP/1\.[01] \d{3}", "http"),
    8888:  (HTTP_REQ, rb"^HTTP/1\.[01] \d{3}", "http"),
    3000:  (HTTP_REQ, rb"^HTTP/1\.[01] \d{3}", "http"),
    5000:  (HTTP_REQ, rb"^HTTP/1\.[01] \d{3}", "http"),
    9000:  (HTTP_REQ, rb"^HTTP/1\.[01] \d{3}", "http"),
    22:    (b"", rb"^SSH-\d", "ssh"),
    21:    (b"", rb"^220", "ftp"),
    25:    (b"", rb"^220", "smtp"),
    110:   (b"", rb"^\+OK", "pop3"),
    143:   (b"", rb"^\* OK", "imap"),
    3306:  (b"", rb"^.", "mysql"),          # 首字节是 payload 长度，任何非空字节都算
    6379:  (b"PING\r\n", rb"^\+PONG", "redis"),
    9200:  (HTTP_REQ, rb"You Know, for Search|\"tagline\"|number", "elasticsearch"),
    11211: (b"version\r\n", rb"^VERSION", "memcached"),
    27017: (b"", rb"^.", "mongodb"),
    5432:  (b"", rb"^.", "postgres"),
    2375:  (HTTP_REQ, rb"^HTTP/1\.[01] \d{3}", "docker"),
}

TLS_PORTS = {443, 8443, 9443, 10443, 465, 993, 995, 6443, 2376}

# nginx 占位页特征：命中说明端口活着但没有业务
PLACEHOLDER_SIGS = [
    rb"Welcome to nginx",
    rb"It works!",
    rb"Test Page for the Nginx HTTP Server",
    rb"Apache2? (?:Ubuntu|Debian|Default) Page",
    rb"default_server",
    rb"IIS Windows Server",
    rb"Site not found|No site configured|domain (?:is )?not (?:bound|configured)",
]


def parse_ports(spec: str):
    out = []
    for part in spec.split(","):
        part = part.strip()
        if not part:
            continue
        if "-" in part:
            a, b = part.split("-", 1)
            out.extend(range(int(a), int(b) + 1))
        else:
            out.append(int(part))
    return sorted(set(out))


def tcp_connect(host, port, timeout):
    s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    s.settimeout(timeout)
    try:
        s.connect((host, port))
        return True, ""
    except socket.timeout:
        return False, "timeout"
    except ConnectionRefusedError:
        return False, "refused"
    except OSError as e:
        return False, f"{type(e).__name__}: {e}"
    finally:
        s.close()


def probe_tls(host, port, timeout):
    """TLS 不做完整握手校验，只看有没有 ServerHello 回来。"""
    try:
        ctx = ssl.create_default_context()
        ctx.check_hostname = False
        ctx.verify_mode = ssl.CERT_NONE
        with socket.create_connection((host, port), timeout=timeout) as raw:
            with ctx.wrap_socket(raw, server_hostname=host) as ss:
                cert = ss.getpeercert()
                ver = ss.version()
                cn = ""
                if cert:
                    for rdn in cert.get("subject", ()):
                        for k, v in rdn:
                            if k == "commonName":
                                cn = v
                return True, f"TLS {ver} CN={cn or '<none>'}"
    except ssl.SSLError as e:
        return False, f"TLSError: {e.reason or e}"
    except (socket.timeout, TimeoutError):
        return False, "timeout"
    except ConnectionResetError:
        return False, "reset"
    except OSError as e:
        return False, f"{type(e).__name__}: {e}"


def probe_raw(host, port, payload, expect, timeout):
    try:
        with socket.create_connection((host, port), timeout=timeout) as s:
            s.settimeout(timeout)
            if payload:
                s.sendall(payload)
            data = s.recv(2048)
            if not data:
                return False, "connected but no data", b""
            import re as _re
            m = _re.search(expect, data)
            if m:
                return True, f"hit: {m.group(0)[:40]!r}", data
            return False, f"no match, got {len(data)}B", data
    except (socket.timeout, TimeoutError):
        return False, "timeout (connected, silent)", b""
    except ConnectionResetError:
        return False, "reset", b""
    except OSError as e:
        return False, f"{type(e).__name__}: {e}", b""


def is_placeholder(data: bytes) -> str:
    for sig in PLACEHOLDER_SIGS:
        if sig in data:
            return sig.decode("utf-8", "replace")
    return ""


def main():
    if len(sys.argv) < 3:
        C.usage_die(__doc__.strip().split("用法：")[0] + "用法：\n  ./tun-port-probe.py <host> <ports> [--timeout 5] [--json out.json]\n示例：\n  ./tun-port-probe.py 10.0.0.5 22,80,443,6379,8080-8090")

    host = sys.argv[1]
    ports = parse_ports(sys.argv[2])
    timeout = 5.0
    if "--timeout" in sys.argv:
        timeout = float(sys.argv[sys.argv.index("--timeout") + 1])
    out_json = sys.argv[sys.argv.index("--json") + 1] if "--json" in sys.argv else None

    C.hdr("0. TUN 伪 ACK 检测")
    # 选一个几乎必然关闭的端口作阴性对照
    dead_port = 1
    ok_c, why = tcp_connect(host, dead_port, timeout)
    if ok_c:
        C.warn(f"端口 {dead_port} 也 connect 成功 → 处于 TUN/代理环境，TCP 判活失效")
        C.warn("后续只信 L2 协议层响应")
        tun_mode = True
    else:
        C.ok(f"端口 {dead_port} connect 失败（{why}）→ 裸 TCP 判活有效")
        tun_mode = False

    C.hdr("1. 逐端口探测")
    results = []
    for p in ports:
        conn_ok, why = tcp_connect(host, p, timeout)
        rec = {"port": p, "tcp": conn_ok, "tcp_note": why, "alive": False,
               "proto": "", "note": "", "placeholder": ""}
        if not conn_ok:
            C.dim(f"{p:>5}  closed ({why})")
            results.append(rec)
            continue

        if p in TLS_PORTS:
            alive, note = probe_tls(host, p, timeout)
            rec["proto"] = "tls"
        elif p in PROBES:
            payload, expect, proto = PROBES[p]
            alive, note, data = probe_raw(host, p, payload, expect, timeout)
            rec["proto"] = proto
            if alive and data:
                ph = is_placeholder(data)
                if ph:
                    rec["placeholder"] = ph
        else:
            # 未知端口：先试 HTTP（绝大多数非常规端口其实是 http 服务），
            # 失败再试 TLS。都不通才判「连通但无协议响应」。
            alive, note, data = probe_raw(host, p, HTTP_REQ, rb"^HTTP/1\.[01] \d{3}", timeout)
            rec["proto"] = "http?"
            if not alive:
                t_alive, t_note = probe_tls(host, p, timeout)
                if t_alive:
                    alive, note = t_alive, t_note
                    rec["proto"] = "tls?"
            elif data:
                ph = is_placeholder(data)
                if ph:
                    rec["placeholder"] = ph

        rec["alive"] = alive
        rec["note"] = note
        tag = "ALIVE" if alive else "FAKE "
        line = f"{p:>5}  {tag}  {rec['proto']:<12} {note}"
        if rec["placeholder"]:
            line += f"  [占位页: {rec['placeholder'][:30]}]"
        (C.ok if alive else C.dim)(line)
        results.append(rec)

    C.hdr("结论")
    alive = [r for r in results if r["alive"]]
    fake = [r for r in results if r["tcp"] and not r["alive"]]
    ph = [r for r in results if r["placeholder"]]
    C.dim(f"探测 {len(ports)} 端口: 真活 {len(alive)} / 伪连通 {len(fake)} / 占位页 {len(ph)}")

    if fake:
        C.warn(f"{len(fake)} 个端口 TCP 通但协议层无响应 —— 这是 TUN 伪 ACK 或中间设备静默丢包")
        C.dim("  端口: " + ",".join(str(r["port"]) for r in fake[:20]))
    if ph:
        C.warn(f"{len(ph)} 个端口是 nginx/apache 占位页（活着但无业务）")
        C.dim("  不要当资产报，先说清「哪条规则没配 upstream」")

    if out_json:
        C.dump_json({"host": host, "tun_mode": tun_mode, "results": results}, out_json)

    if alive:
        C.verdict("tun-port-probe", "ALIVE", f"{len(alive)}/{len(ports)} 协议层确认")
    else:
        C.verdict("tun-port-probe", "NONE", f"{len(ports)} 端口全无协议响应")


if __name__ == "__main__":
    sys.exit(C.main_guard(main)())
