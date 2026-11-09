#!/usr/bin/env python3
"""rip.http.inspect 容器内工具本体（P3-1 交付，P3-3 按 TOOL-CONTRACT §9.1 对齐）。

帧协议（TOOL-CONTRACT D-TC-03）：stdin 一行 JSON 请求，stdout 一行 JSON 结果。
请求形状：{"run_id": "...", "input": {...}, "budget": {...}}；
结果形状：{"status": "result", "data": {...}}，错误帧
{"status": "error", "error": {"code": ..., "message": "..."}}。

input 字段（与 TOOL-CONTRACT §9.1 input_schema 一致，URL 除外均可选）：
- url              必填，仅 http:// 与 https://（T1 只读面，其余 scheme 拒绝）。
- method           可选，枚举 ["GET", "HEAD"]（缺省 GET）。§9.1 草案枚举含
                   POST 属演示枚举（§9.2：POST 场景应改 T2 + authorization_ref），
                   本登记按 T1 只读面收敛为 GET/HEAD，POST 一律拒绝。
- headers          可选 dict，键限 [A-Za-z0-9-]，拒绝逐跳头（host/connection/
                   content-length/transfer-encoding 由 urllib 自管）。
- timeout_seconds  可选，缺省 15，区间 [1, 60]（§9.1 域；ADR §7 执行时长由
                   Adapter 看门狗另按 budget 强制，两者取交集生效）。
- save_body        可选，缺省 true。true 且 method=GET 时响应体落
                   /task/out/body.bin（Artifact 收集载体）并在结果内带
                   body_artifact_id（值 = 容器内产物名 body.bin；引擎侧
                   artifact 行按 file://...body.bin 与 sha256 对账，role=primary）；
                   false 或 HEAD 不落响应体，无 body_artifact_id。

行为：urllib.request（纯标准库，TLS 由 ssl 提供）发起请求，跟随重定向并计数；
完整结果另落 /task/out/result.json。结果 data 字段（§9.1 output_schema 对齐）：
status_code/final_url/headers/body_bytes/body_artifact_id，另附
request_url/reason/redirects/elapsed_ms/body_sha256/body_cap_bytes/
content_type/header_count/out_files/uid。

错误帧码值：-32700 请求解析失败；-32602 参数非法（scheme/method/headers/超限）；
-32006 执行错误（连接、TLS、代理拒绝；HTTP 4xx/5xx 是真实探测结果，照实返回
不视为错误帧）。

T1 边界声明：脚本层只强制 method 枚举与 scheme 白名单；出站 allowlist 由
Adapter 与派发面执行（P3-2 egress_proxy，代理层策略；PHASE3-DESIGN §5.3），
本脚本不做网络层阻断，不虚报。
"""

import json
import os
import re
import signal
import sys
import time
import urllib.error
import urllib.request
from urllib.parse import urlparse

DEFAULT_TIMEOUT_S = 15
MAX_TIMEOUT_S = 60
DEFAULT_MAX_BODY = 16 * 1024 * 1024
MAX_MAX_BODY = 64 * 1024 * 1024
ALLOWED_METHODS = ("GET", "HEAD")
HEADER_KEY_RE = re.compile(r"^[A-Za-z0-9-]{1,128}$")
DENYLIST_HEADERS = {"host", "connection", "content-length", "transfer-encoding"}
SELECTED_HEADERS = (
    "Content-Type",
    "Content-Length",
    "Content-Encoding",
    "Last-Modified",
    "ETag",
    "Server",
    "Date",
)
SELECTED_HEADERS_LOWER = {h.lower() for h in SELECTED_HEADERS}


class _RedirectCounter(urllib.request.HTTPRedirectHandler):
    """HTTPRedirectHandler 计数器：记录重定向次数（跟随行为保持默认）。"""

    count = 0

    def redirect_request(self, req, fp, code, msg, headers, newurl):
        self.count += 1
        return super().redirect_request(req, fp, code, msg, headers, newurl)


def _on_term(signum, _frame):
    """SIGTERM 处理器（D-TC-04 协作式取消的容器内侧，与 smoke_tool 同款）。"""
    sys.exit(128 + signum)


def _emit_error(code, message, exit_code):
    print(json.dumps({
        "status": "error",
        "error": {"code": code, "message": message},
    }, ensure_ascii=False))
    # stderr 镜像（worker 惯例）：Adapter 非零退出路径的信封消息取自
    # stderr，结构化错误帧必须在两路都可见。
    print(json.dumps({
        "status": "error",
        "error": {"code": code, "message": message},
    }, ensure_ascii=False), file=sys.stderr)
    return exit_code


def _validate_input(inp):
    """校验并归一化 input。

    返回 (url, method, timeout_s, max_body, extra_headers, save_body)。
    """
    if not isinstance(inp, dict):
        raise ValueError("input must be an object")
    url = inp.get("url")
    if not isinstance(url, str) or not url:
        raise ValueError("input.url is required")
    parsed = urlparse(url)
    if parsed.scheme not in ("http", "https"):
        raise ValueError(f"scheme not allowed: {parsed.scheme!r} (http/https only)")
    if not parsed.netloc:
        raise ValueError("input.url netloc is empty")

    method = inp.get("method", "GET")
    if method not in ALLOWED_METHODS:
        raise ValueError(
            f"method not allowed: {method!r} (one of {ALLOWED_METHODS}, T1 read-only face)"
        )

    timeout_seconds = inp.get("timeout_seconds", DEFAULT_TIMEOUT_S)
    try:
        timeout_seconds = int(timeout_seconds)
    except (TypeError, ValueError):
        raise ValueError("input.timeout_seconds must be an integer")
    timeout_seconds = max(1, min(timeout_seconds, MAX_TIMEOUT_S))

    max_body = inp.get("max_body_bytes", DEFAULT_MAX_BODY)
    try:
        max_body = int(max_body)
    except (TypeError, ValueError):
        raise ValueError("input.max_body_bytes must be an integer")
    max_body = max(1, min(max_body, MAX_MAX_BODY))

    extra_headers = inp.get("headers") or {}
    if not isinstance(extra_headers, dict):
        raise ValueError("input.headers must be an object")
    clean_headers = {}
    for key, value in extra_headers.items():
        if not isinstance(key, str) or not HEADER_KEY_RE.match(key):
            raise ValueError(f"header key not allowed: {key!r}")
        if key.lower() in DENYLIST_HEADERS:
            raise ValueError(f"header is adapter-managed: {key!r}")
        if not isinstance(value, str) or len(value) > 1024:
            raise ValueError(f"header value invalid for: {key!r}")
        clean_headers[key] = value

    save_body = inp.get("save_body", True)
    if not isinstance(save_body, bool):
        raise ValueError("input.save_body must be a boolean")
    return url, method, timeout_seconds, max_body, clean_headers, save_body


def inspect(url, method, timeout_s, max_body, extra_headers, save_body):
    """执行请求并返回 (data dict, exit_code)；执行错误返回 (None, (code, msg, exit))。"""
    counter = _RedirectCounter()
    opener = urllib.request.build_opener(counter)
    request = urllib.request.Request(url, method=method, headers=dict(extra_headers))
    started = time.monotonic()
    try:
        response = opener.open(request, timeout=timeout_s)
    except urllib.error.HTTPError as exc:
        # HTTP 状态异常（4xx/5xx）是真实探测结果，照实返回而非错误帧。
        elapsed_ms = int((time.monotonic() - started) * 1000)
        body = exc.read(max_body + 1) if hasattr(exc, "read") else b""
        return _finish(
            url, method, exc.geturl(), exc.code, exc.reason, counter.count,
            elapsed_ms, body, max_body,
            dict(exc.headers.items()) if exc.headers else {}, save_body=False,
        )
    except (urllib.error.URLError, OSError, ValueError) as exc:
        return None, (-32006, f"request failed: {exc}", 1)
    elapsed_ms = int((time.monotonic() - started) * 1000)
    body = b"" if method == "HEAD" else response.read(max_body + 1)
    if len(body) > max_body:
        return None, (-32602, f"response body exceeds cap: {max_body}", 2)
    return _finish(
        url, method, response.geturl(), response.status, response.reason,
        counter.count, elapsed_ms, body, max_body, dict(response.headers.items()),
        save_body,
    )


def _finish(request_url, method, final_url, status_code, reason, redirects,
            elapsed_ms, body, max_body, all_headers, save_body):
    """落盘 body.bin（save_body 且 GET 时）与 result.json，组装结果帧。"""
    import hashlib

    effective_save = bool(save_body) and method == "GET"
    os.makedirs("/task/out", exist_ok=True)
    body_artifact_id = None
    out_files = ["result.json"]
    if effective_save:
        body_path = "/task/out/body.bin"
        with open(body_path, "wb") as fh:
            fh.write(body)
        body_artifact_id = "body.bin"
        out_files = ["body.bin", "result.json"]
    body_sha = hashlib.sha256(body).hexdigest()

    selected = {}
    for key, value in all_headers.items():
        if key.lower() in SELECTED_HEADERS_LOWER:
            selected[key] = value

    data = {
        "request_url": request_url,
        "final_url": final_url,
        "status_code": int(status_code),
        "reason": reason,
        "redirects": redirects,
        "elapsed_ms": elapsed_ms,
        "body_bytes": len(body),
        "body_sha256": body_sha,
        "body_cap_bytes": max_body,
        "content_type": all_headers.get("Content-Type")
        or all_headers.get("content-type"),
        "headers": selected,
        "header_count": len(all_headers),
        "out_files": out_files,
        "uid": os.getuid(),
    }
    if body_artifact_id is not None:
        data["body_artifact_id"] = body_artifact_id
    with open("/task/out/result.json", "w", encoding="utf-8") as fh:
        json.dump(data, fh, ensure_ascii=False, sort_keys=True)
    return data, 0


def main() -> int:
    signal.signal(signal.SIGTERM, _on_term)
    raw = sys.stdin.readline()
    try:
        request = json.loads(raw)
    except json.JSONDecodeError as exc:
        return _emit_error(-32700, f"request parse failed: {exc}", 2)

    try:
        url, method, timeout_s, max_body, extra_headers, save_body = _validate_input(
            request.get("input") or {}
        )
    except ValueError as exc:
        return _emit_error(-32602, str(exc), 2)

    data, result = inspect(url, method, timeout_s, max_body, extra_headers, save_body)
    if data is None:
        code, message, exit_code = result
        return _emit_error(code, message, exit_code)
    print(json.dumps({"status": "result", "data": data}, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    sys.exit(main())
