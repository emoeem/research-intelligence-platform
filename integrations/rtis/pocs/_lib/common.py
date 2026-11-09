#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
redteam POC 公共库 v1（Python 版，对应 common.sh）

设计原则（为什么这样写，不是为了好看）：
  1. 判定一律基于「基线对照」，不基于单次响应。单次 200/404 在 CDN、
     软 404、WAF 兜底场景下没有判别力。
  2. 归一化先抹掉每次必变的字段（uuid / 时间戳 / 耗时 / traceId / nonce），
     否则每个响应都「不同」，差分全部失效。
  3. 默认只读。任何写操作必须由调用方显式开 allow_write=True，且脚本里
     必须写明怎么回滚。
  4. 失败不静默。网络错误必须返回可见的错误标记，否则「无输出」会被
     误读成「无漏洞」——这是这类工具最常见的误判来源。

依赖：仅标准库（urllib）。不装 requests，避免在有代理的环境里多一层
变量（urllib 走 https_proxy 环境变量，行为可预期）。
"""

from __future__ import annotations

import http.client
import json
import os
import random
import re
import socket
import ssl
import string
import sys
import time
import urllib.error
import urllib.parse
import urllib.request

# ---- 终端 ----------------------------------------------------------------
_TTY = sys.stderr.isatty()


def _c(code: str) -> str:
    return code if _TTY else ""


RED, GRN, YEL, CYA, DIM, BLD, RST = (
    _c("\033[31m"), _c("\033[32m"), _c("\033[33m"),
    _c("\033[36m"), _c("\033[2m"), _c("\033[1m"), _c("\033[0m"),
)


def log(*a):
    print(f"{CYA}[*]{RST} {' '.join(str(x) for x in a)}", flush=True)


def ok(*a):
    print(f"{GRN}[+]{RST} {' '.join(str(x) for x in a)}", flush=True)


def warn(*a):
    print(f"{YEL}[!]{RST} {' '.join(str(x) for x in a)}", flush=True)


def err(*a):
    print(f"{RED}[-]{RST} {' '.join(str(x) for x in a)}", file=sys.stderr, flush=True)


def dim(*a):
    print(f"{DIM}    {' '.join(str(x) for x in a)}{RST}", flush=True)


def hdr(title: str):
    print(f"\n{BLD}== {title} =={RST}", flush=True)


def rule(n: int = 60):
    print(f"{DIM}{'-' * n}{RST}", flush=True)


def usage_die(usage: str, code: int = 1):
    """无参数 / 参数不足时的唯一出口。

    不用 argparse 的原因：argparse 缺参数会抛 SystemExit(2) 打 usage 到
    stderr，格式和我们其它脚本不一致；这里统一成一行，便于批量回归。
    """
    print(usage, file=sys.stderr)
    sys.exit(code)


# ---- HTTP ----------------------------------------------------------------
UA = (
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 "
    "(KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36"
)
TIMEOUT = float(os.environ.get("RT_TIMEOUT", "12"))

_CTX = ssl.create_default_context()
_CTX.check_hostname = False
_CTX.verify_mode = ssl.CERT_NONE  # 靶标常用自签，验证只会让探测失败


class Resp:
    """一次响应。status == 0 表示根本没拿到 HTTP 层响应。"""

    __slots__ = ("status", "body", "headers", "url", "elapsed", "error")

    def __init__(self, status=0, body="", headers=None, url="", elapsed=0.0, error=""):
        self.status = status
        self.body = body or ""
        self.headers = headers or {}
        self.url = url
        self.elapsed = elapsed
        self.error = error

    @property
    def ok(self) -> bool:
        return self.status > 0

    def header(self, name: str) -> str:
        for k, v in self.headers.items():
            if k.lower() == name.lower():
                return v
        return ""

    def json(self):
        try:
            return json.loads(self.body)
        except Exception:
            return None

    def __repr__(self):
        return f"<Resp {self.status} {len(self.body)}B {self.elapsed:.2f}s{(' ERR:' + self.error) if self.error else ''}>"


def req(url, method="GET", data=None, headers=None, timeout=None,
        proxy=None, allow_redirects=False, raw=False) -> Resp:
    """发一个请求。永远返回 Resp，从不抛异常。

    为什么吞异常：探测脚本在中途因为一个超时崩掉，会让人以为「扫完了」。
    返回 status=0 + error 文本，调用方必须显式判断。
    """
    h = {"User-Agent": UA, "Accept": "*/*", "Connection": "close"}
    if headers:
        h.update(headers)

    body = None
    if data is not None:
        if isinstance(data, (dict, list)):
            body = json.dumps(data).encode()
            h.setdefault("Content-Type", "application/json")
        elif isinstance(data, str):
            body = data.encode()
        else:
            body = data

    r = urllib.request.Request(url, data=body, headers=h, method=method.upper())

    handlers = []
    if proxy:
        handlers.append(urllib.request.ProxyHandler({"http": proxy, "https": proxy}))
    else:
        # 显式禁用环境代理：目标在内网时，走 TUN/系统代理会得到假连通
        handlers.append(urllib.request.ProxyHandler({}))
    if not allow_redirects:
        class _NoRedirect(urllib.request.HTTPRedirectHandler):
            def redirect_request(self, *a, **kw):
                return None
        handlers.append(_NoRedirect())
    opener = urllib.request.build_opener(*handlers, urllib.request.HTTPSHandler(context=_CTX))

    t0 = time.time()
    try:
        with opener.open(r, timeout=timeout or TIMEOUT) as resp:
            raw_body = resp.read()
            return Resp(
                status=resp.status,
                body=raw_body if raw else raw_body.decode("utf-8", "replace"),
                headers=dict(resp.headers.items()),
                url=resp.geturl(),
                elapsed=time.time() - t0,
            )
    except urllib.error.HTTPError as e:
        raw_body = e.read()
        return Resp(
            status=e.code,
            body=raw_body if raw else raw_body.decode("utf-8", "replace"),
            headers=dict(e.headers.items()) if e.headers else {},
            url=url,
            elapsed=time.time() - t0,
        )
    except urllib.error.URLError as e:
        return Resp(url=url, elapsed=time.time() - t0, error=f"URLError: {e.reason}")
    except (socket.timeout, TimeoutError):
        return Resp(url=url, elapsed=time.time() - t0, error="timeout")
    except (http.client.HTTPException, ConnectionError, OSError) as e:
        return Resp(url=url, elapsed=time.time() - t0, error=f"{type(e).__name__}: {e}")
    except Exception as e:  # 兜底，绝不让探测脚本崩
        return Resp(url=url, elapsed=time.time() - t0, error=f"{type(e).__name__}: {e}")


# ---- 归一化 --------------------------------------------------------------
_NORMS = [
    (re.compile(r"[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}", re.I), "<UUID>"),
    (re.compile(r"\b[0-9a-f]{32}\b", re.I), "<HEX32>"),
    (re.compile(r"\b[0-9a-f]{40}\b", re.I), "<HEX40>"),
    (re.compile(r"\b[0-9a-f]{64}\b", re.I), "<HEX64>"),
    (re.compile(r"\b\d{4}-\d{2}-\d{2}[T ]\d{2}:\d{2}:\d{2}(\.\d+)?(Z|[+-]\d{2}:?\d{2})?"), "<TS>"),
    (re.compile(r"\b1[3-9]\d{9}\b"), "<PHONE>"),          # 手机号，避免指纹被真实数据带偏
    (re.compile(r"\b\d{13,19}\b"), "<NUMID>"),           # 长数字 ID / 时间戳毫秒
    (re.compile(r'"(timestamp|serverTime|now|ts|nonce|traceId|requestId|request_id)"\s*:\s*[^,}\]]+', re.I), r'"\1":<V>'),
    (re.compile(r"(?i)\b(took|elapsed|duration|cost|time_?cost)\s*[:=]\s*[\d.]+"), r"\1=<V>"),
    (re.compile(r"\s+"), " "),                            # 折叠空白：大小/换行差异不算差异
]


def norm(s: str, limit: int = 2000) -> str:
    """归一化：抹掉每次请求必然变化的字段，只留结构指纹。

    limit 是必要的：两个 5MB 的响应做全文 diff 没有意义，反而会因为
    响应尾部的一条随机广告位判成「不同」。
    """
    if not s:
        return ""
    out = s
    for pat, rep in _NORMS:
        out = pat.sub(rep, out)
    return out[:limit].strip()


def rand_path(prefix: str = "/zzz") -> str:
    tail = "".join(random.choices(string.ascii_lowercase + string.digits, k=10))
    return f"{prefix}_{int(time.time() * 1000) % 10**7}_{tail}"


# ---- 基线 ----------------------------------------------------------------
class Baseline:
    """针对一个 host 的基线响应集合。

    用法：
        b = Baseline("https://t.example.com")
        b.add("root", "/")
        if b.same("root", "/admin"):  # /admin 和 / 长得一样 → 软 404
    """

    def __init__(self, base: str, headers=None):
        self.base = base.rstrip("/")
        self.headers = headers or {}
        self._m: dict[str, Resp] = {}

    def add(self, name: str, path: str) -> Resp:
        r = req(self.base + path, headers=self.headers)
        self._m[name] = r
        return r

    def get(self, name: str) -> Resp | None:
        return self._m.get(name)

    def _fps(self):
        return {k: (v.status, norm(v.body)) for k, v in self._m.items()}

    def same(self, name: str, path: str, headers=None) -> bool:
        """目标路径是否与某条基线「完全相同」（状态码 + 归一化 body）。"""
        if name not in self._m:
            return False
        cur = req(self.base + path, headers=headers or self.headers)
        ref = self._m[name]
        return cur.status == ref.status and norm(cur.body) == norm(ref.body)

    def matches_any(self, path: str, headers=None) -> str | None:
        """返回命中的基线名。命中任意一条 = 这个路径没有独立响应（软 404 / 兜底）。"""
        cur = req(self.base + path, headers=headers or self.headers)
        n = norm(cur.body)
        for k, ref in self._m.items():
            if cur.status == ref.status and n == norm(ref.body):
                return k
        return None

    def __repr__(self):
        return "<Baseline " + " ".join(f"{k}={v.status}" for k, v in self._m.items()) + ">"


def soft404_guard(base: str, path: str, rand: str | None = None) -> tuple[Resp, Resp]:
    """返回 (随机路径响应, 目标路径响应)。随机路径必须是 404 或与之不同才算有判别力。"""
    rand = rand or rand_path()
    rnd = req(base + rand)
    tgt = req(base + path)
    return rnd, tgt


def classify(resp: Resp, rnd: Resp) -> str:
    """三态分类：BLOCK / SAME(无判别力) / OTHER。

    只做最保守的判断。具体业务命中要靠调用方自己的特征串，
    因为「什么算命中」只有该脚本知道。
    """
    if not resp.ok:
        return "DEAD"
    if resp.status in (401, 403, 407):
        return "BLOCK"
    if not rnd.ok:
        return "OTHER"
    if resp.status == rnd.status and norm(resp.body) == norm(rnd.body):
        return "SAME"
    return "OTHER"


# ---- 内网 / 输入校验 ------------------------------------------------------
_PRIVATE = [
    re.compile(r"^10\."),
    re.compile(r"^127\."),
    re.compile(r"^169\.254\."),
    re.compile(r"^172\.(1[6-9]|2\d|3[01])\."),
    re.compile(r"^192\.168\."),
    re.compile(r"^0\."),
]


def is_private_ip(ip: str) -> bool:
    return any(p.search(ip) for p in _PRIVATE)


def die_if_public_scan(target: str):
    """写操作前的护栏：拒绝把批量写操作打到公网。

    只在 allow_write 的脚本里调用。理由是这些脚本会改数据，
    公网目标上的一次误触就是一次事故。
    """
    host = urllib.parse.urlsplit(target).hostname or ""
    if not host:
        return
    try:
        ip = socket.gethostbyname(host)
    except Exception:
        return
    if not is_private_ip(ip):
        warn(f"目标是公网地址 ({host} → {ip})，写操作已禁用。确认授权后用 --i-have-authorization 重跑")
        sys.exit(3)


# ---- 输出 ----------------------------------------------------------------
def dump_json(obj, path: str | None = None):
    s = json.dumps(obj, ensure_ascii=False, indent=2)
    if path:
        with open(path, "w", encoding="utf-8") as f:
            f.write(s)
        dim(f"已写入 {path}")
    else:
        print(s)


def verdict(name: str, value: str, note: str = ""):
    """统一结论行。所有脚本最后一律用这个，方便批量 grep 汇总。"""
    tail = f"  {DIM}{note}{RST}" if note else ""
    print(f"\n{BLD}判定{RST}: {name} = {value}{tail}", flush=True)


def main_guard(fn):
    """包装 main：KeyboardInterrupt 干净退出，其它异常打印但不吞栈。"""
    def wrapper():
        try:
            return fn()
        except KeyboardInterrupt:
            print("", file=sys.stderr)
            return 130
        except BrokenPipeError:
            return 0
    return wrapper
