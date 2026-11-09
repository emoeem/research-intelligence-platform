#!/usr/bin/env python3
"""坏握手 worker（P1-6 e2e 专用 fixture）：对 initialize 一律回 -32001。

用于验证适配器握手失败映射 INFRA（PREPARING → FAILED，TOOL-CONTRACT §3.2 第 7 行）。
"""
import json
import sys


def main() -> None:
    for raw in sys.stdin:
        line = raw.strip()
        if not line:
            continue
        try:
            msg = json.loads(line)
        except ValueError:
            continue
        reply = {
            "jsonrpc": "2.0",
            "id": msg.get("id"),
            "error": {"code": -32001, "message": "fixture worker refuses initialize"},
        }
        sys.stdout.write(json.dumps(reply) + "\n")
        sys.stdout.flush()


if __name__ == "__main__":
    main()
