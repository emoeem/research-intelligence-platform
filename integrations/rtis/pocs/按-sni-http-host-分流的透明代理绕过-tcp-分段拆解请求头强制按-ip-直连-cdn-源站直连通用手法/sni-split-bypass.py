#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
按 SNI / Host 分流的绕过：CDN 源站直连通用手法

场景：目标在 CDN 后面，直接解析到源站 IP 访问会被拒（SNI 不匹配 / Host 不在
      白名单）。三种常见的「分流」实现，各有一种绕过：

  M1 SNI 分流（TLS 层）：网关按 ClientHello 里的 SNI 决定回源到哪个站点。
     绕过：对 IP 直连但把 SNI 设成目标域名 —— 这是标准做法，多数 CDN 会放行。
     反制：源站要求 SNI 必须等于某个内部名字（如 *.internal），那就得猜。
  M2 Host 分流（HTTP 层）：网关按 Host 头路由。
     绕过：TCP 连到目标 IP，发 Host: 目标域名。
  M3 表头拆包检测：WAF 会检查「ClientHello 与 Host 是否匹配」，不匹配就拦。
     绕过：TCP 分段发送，先发 SNI=A 的 ClientHello，握手完成后在加密通道里
           发 Host: B。WAF 看不到明文 Host（但源站能看到，因为它解 TLS）。

本脚本实现 M1 / M2 / M3 三种探测，并给出「哪种通」的结论。

关键判据：三种方式的响应必须**互不相同**才有意义。
  如果三种都返回同一个 403 页面，说明拦在更前面（IP 黑名单 / 全站 WAF），
  这时候不是绕过失败，是这条路走不通，别再试变体。

用法：
  ./sni-split-bypass.py <IP> <域名> [--port 443] [--host-header 另一个域名]
示例：
  ./sni-split-bypass.py 1.2.3.4 origin.example.com
  ./sni-split-bypass.py 1.2.3.4 a.example.com --host-header b.example.com
"""
import os
import socket
import ssl
import sys

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "_lib"))
import common as C  # noqa: E402


def tls_get(ip, port, sni, path="/", host_header=None, timeout=8, split=False):
    """对 IP 建 TLS 连接，SNI 设为 sni，可选覆盖 Host 头。

    split=True 时模拟「分段发送」：先建连接，等 200ms 再发请求，
    用于观察某些网关是否在连接建立后就不再检查 Host。
    """
    ctx = ssl.create_default_context()
    ctx.check_hostname = False
    ctx.verify_mode = ssl.CERT_NONE
    try:
        raw = socket.create_connection((ip, port), timeout=timeout)
    except OSError as e:
        return None, f"connect failed: {type(e).__name__}: {e}"

    try:
        with ctx.wrap_socket(raw, server_hostname=sni) as ss:
            cert = ss.getpeercert()
            cn = ""
            if cert:
                for rdn in cert.get("subject", ()):
                    for k, v in rdn:
                        if k == "commonName":
                            cn = v
            if split:
                import time
                time.sleep(0.2)
            hh = host_header or sni
            req = (
                f"GET {path} HTTP/1.1\r\n"
                f"Host: {hh}\r\n"
                f"User-Agent: {C.UA}\r\n"
                f"Accept: */*\r\n"
                f"Connection: close\r\n\r\n"
            ).encode()
            ss.sendall(req)
            chunks = []
            while True:
                try:
                    b = ss.recv(4096)
                except (socket.timeout, TimeoutError):
                    break
                if not b:
                    break
                chunks.append(b)
                if sum(len(x) for x in chunks) > 65536:
                    break
            data = b"".join(chunks)
            head = data.split(b"\r\n\r\n", 1)[0].decode("latin-1", "replace")
            status = head.split("\r\n")[0] if head else "<no response>"
            return {"status": status, "cn": cn, "head": head, "body": data}, ""
    except ssl.SSLError as e:
        return None, f"TLS: {e.reason or e}"
    except (socket.timeout, TimeoutError):
        return None, "timeout"
    except OSError as e:
        return None, f"{type(e).__name__}: {e}"
    finally:
        try:
            raw.close()
        except Exception:
            pass


def http_get(ip, port, host_header, path="/", timeout=8):
    try:
        with socket.create_connection((ip, port), timeout=timeout) as s:
            s.settimeout(timeout)
            req = (f"GET {path} HTTP/1.0\r\nHost: {host_header}\r\n"
                   f"User-Agent: {C.UA}\r\nConnection: close\r\n\r\n").encode()
            s.sendall(req)
            data = b""
            while True:
                try:
                    b = s.recv(4096)
                except (socket.timeout, TimeoutError):
                    break
                if not b:
                    break
                data += b
                if len(data) > 65536:
                    break
            head = data.split(b"\r\n\r\n", 1)[0].decode("latin-1", "replace")
            return {"status": head.split("\r\n")[0] if head else "<none>", "head": head,
                    "body": data}, ""
    except OSError as e:
        return None, f"{type(e).__name__}: {e}"


def fingerprint(res):
    """把响应压成一个短指纹，用于比较「三种方式是否得到不同结果」。"""
    if not res:
        return "DEAD"
    st = res["status"]
    body = res["body"]
    import hashlib
    return f"{st.split(' ')[1] if ' ' in st else st}:{hashlib.md5(body[:4096]).hexdigest()[:8]}:{len(body)}"


def main():
    if len(sys.argv) < 3:
        C.usage_die(__doc__.strip().split("用法：")[0] + "用法：\n  ./sni-split-bypass.py <IP> <域名> [--port 443] [--host-header 域名]\n示例：\n  ./sni-split-bypass.py 1.2.3.4 origin.example.com")

    ip, domain = sys.argv[1], sys.argv[2]
    port = 443
    host_hdr = None
    if "--port" in sys.argv:
        port = int(sys.argv[sys.argv.index("--port") + 1])
    if "--host-header" in sys.argv:
        host_hdr = sys.argv[sys.argv.index("--host-header") + 1]

    C.hdr("0. 直连可达性")
    ok_c, why = C.req(f"https://{domain}/").ok, ""
    C.dim(f"走域名访问: {'OK' if ok_c else '失败'}")

    C.hdr("1. M1 · IP + SNI=域名（标准做法）")
    r1, e1 = tls_get(ip, port, domain, path="/")
    if r1:
        C.ok(f"{r1['status']}  CN={r1['cn'] or '<none>'}  {len(r1['body'])}B")
    else:
        C.warn(f"失败: {e1}")

    C.hdr("2. M2 · IP + Host=域名（明文 HTTP 或 TLS 后发）")
    r2, e2 = http_get(ip, port, domain)
    if r2:
        C.ok(f"HTTP 层: {r2['status']}  {len(r2['body'])}B")
    else:
        C.dim(f"明文 HTTP 失败（{e2}）——该端口是 TLS，改用 M1")

    C.hdr("3. M3 · 分段发送（连接后延迟 200ms 再发请求）")
    r3, e3 = tls_get(ip, port, domain, path="/", split=True)
    if r3:
        C.ok(f"{r3['status']}  {len(r3['body'])}B")
    else:
        C.warn(f"失败: {e3}")

    C.hdr("4. M4 · SNI 与 Host 不一致")
    r4 = None
    if host_hdr:
        r4, e4 = tls_get(ip, port, domain, path="/", host_header=host_hdr)
        if r4:
            C.ok(f"SNI={domain} Host={host_hdr} → {r4['status']}")
        else:
            C.dim(f"失败: {e4}")
    else:
        C.dim("未提供 --host-header，跳过。用法：--host-header 想探的另一个域名")
        C.dim("典型场景：SNI=公网域名 拿到证书，Host=内网名 试探源站还挂了哪些站")

    C.hdr("结论")
    fps = {
        "M1(SNI)": fingerprint(r1),
        "M2(Host)": fingerprint(r2),
        "M3(split)": fingerprint(r3),
    }
    if r4:
        fps["M4(mismatch)"] = fingerprint(r4)
    for k, v in fps.items():
        C.dim(f"  {k:<14} {v}")

    uniq = set(fps.values())
    C.err("")
    if len(uniq) == 1:
        C.warn("四种方式响应完全相同 → 拦截在更前面（IP 黑名单 / 全站 WAF / 统一错误页）")
        print("""判读：
  · 不是「绕过失败」，是这条路径走不通。别再试 payload 变体，换思路：
    - 找 CDN 的其他回源域名（CNAME 链、证书 SAN、DNS 历史记录）
    - 找同源站的其他 IP（C 段扫描只扫 80/443 + 只比 favicon，不扫全端口）
    - 从被动 DNS / 证书透明日志（crt.sh）找历史解析
""")
        C.verdict("sni-split-bypass", "BLOCKED_UPSTREAM", "四种方式结果一致")
    else:
        C.warn("不同方式得到不同响应 → 分流存在，且部分方式可绕")
        print("""判读（按可信度）：
  · M1 通而 M2 不通 → 网关按 SNI 路由，源站接受直连。这是最常见也最有用的一条
  · M3 与 M1 不同 → 存在「连接建立后不再校验 Host」的行为，可尝试 Host 注入
  · M4 拿到不同站点 → 源站上挂了多个站（vhost），可横向枚举同源站的其他业务
下一步：M1 通的话，直接对 IP 做 Host 枚举（用 vhost 矩阵脚本），
        并检查源站是否开放了 CDN 上没有的端口（管理端常只在内网段可达）""")
        C.verdict("sni-split-bypass", "BYPASSABLE", f"{len(uniq)} 种不同响应")
    return 0


if __name__ == "__main__":
    sys.exit(C.main_guard(main)())
