#!/usr/bin/env python3
"""P6-4 mcp_server fixture：最小 stdio JSON-RPC 2.0 应答器（initialize + tools/list）。"""
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
            "serverInfo": {"name": "fixture-mcp", "version": "0.1.0"},
        }}), flush=True)
    elif method == "notifications/initialized":
        continue
    elif method == "tools/list" and rid is not None:
        print(json.dumps({"jsonrpc": "2.0", "id": rid, "result": {"tools": [
            {"name": "fixture.tool", "description": "fixture tool", "inputSchema": {"type": "object"}},
        ]}}), flush=True)
        break
