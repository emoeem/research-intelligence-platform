#!/usr/bin/env python3
"""P6-6 project_endpoint fixture：stdio JSON-RPC 2.0 MCP 应答器（2 个工具）。"""
import json
import sys

for line in sys.stdin:
    line = line.strip()
    if not line:
        continue
    try:
        req = json.loads(line)
    except json.JSONDecodeError:
        continue
    method = req.get("method", "")
    rid = req.get("id")
    if method == "initialize" and rid is not None:
        print(json.dumps({"jsonrpc": "2.0", "id": rid, "result": {
            "protocolVersion": "2024-11-05",
            "capabilities": {"tools": {}},
            "serverInfo": {"name": "fixture-endpoint", "version": "0.2.0"},
        }}), flush=True)
    elif method.startswith("notifications/"):
        continue
    elif method == "tools/list" and rid is not None:
        print(json.dumps({"jsonrpc": "2.0", "id": rid, "result": {"tools": [
            {"name": "fixture_echo", "description": "echo fixture",
             "inputSchema": {"type": "object", "required": ["text"],
                             "properties": {"text": {"type": "string"}}}},
            {"name": "fixture_stats", "description": "stats fixture",
             "inputSchema": {"type": "object"}},
        ]}}), flush=True)
        break
