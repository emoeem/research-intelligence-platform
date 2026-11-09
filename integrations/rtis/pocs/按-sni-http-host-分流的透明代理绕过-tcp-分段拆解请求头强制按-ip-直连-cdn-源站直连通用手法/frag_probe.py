#!/usr/bin/env python3
"""Host-header TCP-fragment prober.

Defeats SNI/HTTP-Host based transparent proxies (sing-box, FlClash, Passwall,
Clash TUN etc.) that re-route by sniffed hostname and ignore the destination IP.
Splitting the request header across TCP segments makes the sniffer fail to parse
the Host header, so it falls back to a raw IP connection. TCP is a byte stream,
so the origin reassembles the request normally.

Usage:  python3 frag_probe.py <ip> <port> <host> <path> [delay]
Judging: ORIGIN-REACHED  -> no via / cf-ray / x-amz-cf-pop
         CDN/SNIFFER-WON -> sniffer won, raise delay
"""
import socket, sys, time

DEFAULT_HEADERS = {
    "X-Login-Type": "3",
    "X-Platform-Id": "3",
    "X-Currency": "CNY",
    "Language": "zh",
}

def frag_request(ip, port, host, path="/", delay=0.8, headers=None, timeout=15):
    headers = {**DEFAULT_HEADERS, **(headers or {})}
    headers.setdefault("x-req-domain", host)
    req  = f"GET {path} HTTP/1.1\r\n"
    hdr  = f"Host: {host}\r\n"
    rest = "".join(f"{k}: {v}\r\n" for k, v in headers.items())
    rest += "User-Agent: curl/8.5\r\nAccept: */*\r\nConnection: close\r\n\r\n"
    s = socket.create_connection((ip, port), timeout=timeout)
    try:
        s.sendall(req.encode());  time.sleep(delay)   # segment 1
        s.sendall(hdr.encode());  time.sleep(delay)   # segment 2 <- the trick
        s.sendall(rest.encode())                      # segment 3
        s.settimeout(timeout)
        buf = b""
        while len(buf) < 8000:
            d = s.recv(4096)
            if not d:
                break
            buf += d
        return buf.decode("utf-8", "replace")
    finally:
        s.close()


def is_origin(resp: str) -> bool:
    low = resp.lower()
    return ("via:" not in low) and ("cf-ray" not in low) and ("x-amz-cf-pop" not in low) \
           and resp.startswith("HTTP/")


if __name__ == "__main__":
    if len(sys.argv) < 5:
        print(__doc__); sys.exit(1)
    ip, port, host, path = sys.argv[1], int(sys.argv[2]), sys.argv[3], sys.argv[4]
    delay = float(sys.argv[5]) if len(sys.argv) > 5 else 0.8
    r = frag_request(ip, port, host, path, delay)
    lines = r.split("\r\n")
    print(lines[0] if lines else "(empty)")
    for l in lines:
        if l.lower().startswith(("server:", "cf-ray:", "via:", "x-amz-cf-pop:")):
            print(" ", l)
    print("ORIGIN-REACHED" if is_origin(r) else "CDN/SNIFFER-WON")
    print(r.split("\r\n\r\n", 1)[1][:400] if "\r\n\r\n" in r else "")
