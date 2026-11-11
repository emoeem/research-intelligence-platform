#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""rip_new_tool.py：新增工具脚手架（ROADMAP P6-9，SESSION-HANDOFF §八）。

一条线：rp_new_tool.py --id <tool_id> --name <name> --family <family> \
  --description <desc> [--worker-dir workers/python] [--db <path>]

步骤：
  1. D-TC-02 正则校验 tool_id（小写点分 namespace.name）；
  2. 生成 Tool Manifest 骨架（YAML，schemas/tool-manifest/v1.json 词表）；
  3. 生成 python worker stub（Worker Protocol v1 Runtime 框架）；
  4. 调 rp-tool validate（七条静态校验）；
  5. 调 rp-tool register --db <db>（tool + tool_version 落库）；
  6. 调 rp-tool-health ping（Worker Protocol v1 握手）；
  7. 查 DB 确认 tool 行在位（/api/tools 露出的数据面证据）。

stdlib-only：仅 import json/re/argparse/subprocess/sqlite3/pathlib。
"""

from __future__ import annotations

import argparse
import json
import re
import sqlite3
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]

# D-TC-02（TOOL-CONTRACT §2.1，schemas/tool-manifest/v1.json id pattern）：
# 小写点分 namespace.name，段内 [a-z0-9_-]
TOOL_ID_RE = re.compile(r"^[a-z][a-z0-9_-]*(\.[a-z][a-z0-9_-]*)+$")

FAMILIES = [
    "web_http", "dns_domain", "network", "web_analysis", "source_code",
    "mobile_apk", "binary_reverse", "document", "data_processing",
    "knowledge", "evidence", "utility", "browser_automation",
]

MANIFEST_TEMPLATE = """\
# {name}（rip_new_tool.py 脚手架生成，ROADMAP P6-9）
schema_version: "1"
id: "{tool_id}"
name: "{name}"
version: "0.1.0"
kind: "tool"
family: "{family}"
description: "{description}"
runtime:
  type: "python_worker"
  protocol_version: "1"
  worker_name: "{worker_name}"
  command: ["python3", "-I", "{worker_rel}"]
  working_dir: "repo://research-platform"
  restart: "pooled"
input_schema:
  type: "object"
  required: ["text"]
  properties:
    text:
      type: "string"
      description: "输入文本（脚手架生成，请按工具语义修改）"
output_schema:
  type: "object"
  properties:
    result:
      type: "string"
      description: "处理结果（脚手架生成，请按工具语义修改）"
resources:
  cpu: "1"
  memory: "256Mi"
  timeout_seconds: 60
  max_output_bytes: 1048576
network:
  mode: "none"
secrets: []
permission_level: "T0"
idempotent: true
health:
  probe:
    type: "worker_ping"
    timeout_ms: 2000
    interval_seconds: 300
    degraded_after: 1
    broken_after: 3
  on_run_recover: true
provenance:
  origin: "rip-native"
  source_path: "{worker_rel}"
  license: "repo-internal"
  dependencies: []
  tested: "rip_new_tool.py 脚手架冒烟（Worker Protocol v1 initialize + execute 回环）"
context:
  summary: "{description}"
  docs: "{worker_rel} 模块头注"
"""

WORKER_TEMPLATE = """\
#!/usr/bin/env python3
# -*- coding: utf-8 -*-
\"\"\"{name} worker（rip_new_tool.py 脚手架生成）。

Worker Protocol v1（JSON-RPC 2.0 over stdio）由 worker_runtime.py 基类承载。
子类只需实现 execute(ctx, task) -> dict。
\"\"\"

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent / "runtime"))

from worker_runtime import WorkerRuntime, WorkerContext


class {class_name}(WorkerRuntime):
    NAME = "{worker_name}"
    VERSION = "0.1.0"
    CAPABILITIES = []

    def execute(self, ctx: WorkerContext, task: dict) -> dict:
        text = task.get("text", "")
        # TODO: 在此实现工具语义（当前为脚手架回显）
        return {{"result": text}}


if __name__ == "__main__":
    {class_name}().serve()
"""


def check_tool_id(tool_id: str) -> None:
    if not TOOL_ID_RE.match(tool_id):
        print(f"[FAIL] tool_id {tool_id!r} 不合 D-TC-02 正则 "
              f"(须为小写点分 namespace.name，段内 [a-z0-9_-])")
        sys.exit(1)
    print(f"[PASS] tool_id {tool_id!r} 合 D-TC-02 正则")


def check_family(family: str) -> None:
    if family not in FAMILIES:
        print(f"[FAIL] family {family!r} 不在十三族词表 {FAMILIES}")
        sys.exit(1)
    print(f"[PASS] family {family!r} 合十三族词表")


def gen_manifest(args: argparse.Namespace, worker_rel: str, worker_name: str) -> Path:
    out = REPO / "manifests" / "tools" / f"{args.id}.yaml"
    out.parent.mkdir(parents=True, exist_ok=True)
    class_name = "".join(w.capitalize() for w in worker_name.replace("-", "_").split("_"))
    content = MANIFEST_TEMPLATE.format(
        tool_id=args.id, name=args.name, family=args.family,
        description=args.description, worker_name=worker_name,
        worker_rel=worker_rel, class_name=class_name,
    )
    out.write_text(content, encoding="utf-8")
    print(f"[PASS] Tool Manifest 写入 {out}")
    return out


def gen_worker(args: argparse.Namespace, worker_rel: str, worker_name: str) -> Path:
    out = REPO / worker_rel
    out.parent.mkdir(parents=True, exist_ok=True)
    class_name = "".join(w.capitalize() for w in worker_name.replace("-", "_").split("_"))
    content = WORKER_TEMPLATE.format(name=args.name, worker_name=worker_name, class_name=class_name)
    out.write_text(content, encoding="utf-8")
    print(f"[PASS] worker stub 写入 {out}")
    return out


def run_cmd(cmd: list[str], label: str) -> str:
    try:
        proc = subprocess.run(cmd, capture_output=True, text=True, timeout=120, cwd=str(REPO))
    except FileNotFoundError as e:
        print(f"[FAIL] {label}: {e}")
        sys.exit(1)
    if proc.returncode != 0:
        print(f"[FAIL] {label}: exit {proc.returncode}")
        print(f"  stderr: {proc.stderr.strip()[:300]}")
        sys.exit(1)
    stdout = proc.stdout.strip()
    print(f"[PASS] {label}")
    return stdout


def check_visibility(db: str, tool_id: str) -> None:
    conn = sqlite3.connect(f"file:{db}?mode=ro", uri=True)
    try:
        row = conn.execute(
            "SELECT tool_id, name, lifecycle, governance FROM tool WHERE tool_id = ?", (tool_id,)
        ).fetchone()
        if row is None:
            print(f"[FAIL] tool 表未查到 {tool_id!r}（/api/tools 不露出）")
            sys.exit(1)
        print(f"[PASS] tool 表行在位：tool_id={row[0]} lifecycle={row[2]} governance={row[3]}")
        ver = conn.execute(
            "SELECT version, status, content_hash FROM tool_version WHERE tool_id = ?",
            (tool_id,),
        ).fetchone()
        if ver is None:
            print(f"[FAIL] tool_version 表未查到 {tool_id!r}")
            sys.exit(1)
        print(f"[PASS] tool_version 行在位：version={ver[0]} status={ver[1]} hash_len={len(ver[2] or '')}")
    finally:
        conn.close()


def main() -> None:
    parser = argparse.ArgumentParser(description="RIP 新增工具脚手架（stdlib-only）")
    parser.add_argument("--id", required=True, help="tool_id（小写点分 namespace.name）")
    parser.add_argument("--name", required=True, help="工具名称")
    parser.add_argument("--family", required=True, help="工具族（十三族之一）")
    parser.add_argument("--description", required=True, help="一句话描述")
    parser.add_argument("--worker-dir", default="workers/python", help="worker stub 输出目录（相对仓库根）")
    parser.add_argument("--db", default=str(REPO / "migrations" / "dev-p6-tool.db"),
                        help="注册目标库（开发副本，默认 migrations/dev-p6-tool.db）")
    parser.add_argument("--skip-health", action="store_true", help="跳过健康检查（非 python_worker 工具）")
    args = parser.parse_args()

    check_tool_id(args.id)
    check_family(args.family)

    # worker 文件与命名
    last_seg = args.id.rsplit(".", 1)[-1]
    worker_filename = last_seg.replace("-", "_") + "_worker.py"
    worker_rel = f"{args.worker_dir}/{worker_filename}"
    worker_name = last_seg.replace("-", "_") + "-worker"

    # 步骤 2-3：生成骨架
    manifest_path = gen_manifest(args, worker_rel, worker_name)
    worker_path = gen_worker(args, worker_rel, worker_name)

    # 步骤 4：validate
    bin_tool = str(REPO / "target" / "debug" / "rp-tool")
    run_cmd([bin_tool, "validate", "--manifest", str(manifest_path)],
            "rp-tool validate（七条静态校验）")

    # 步骤 5：register
    run_cmd([bin_tool, "register", "--manifest", str(manifest_path), "--db", args.db],
            "rp-tool register（tool + tool_version 落库）")

    # 步骤 6：health check
    if not args.skip_health:
        bin_health = str(REPO / "target" / "debug" / "rp-tool-health")
        run_cmd([bin_health, "ping", "--manifest", str(manifest_path)],
                "rp-tool-health ping（Worker Protocol v1 握手）")

    # 步骤 7：露出核验
    check_visibility(args.db, args.id)

    print("----")
    print(f"完成：{args.id} 已注册到 {args.db} 并露出于 /api/tools（Tool Center 数据面）。")
    print(f"Manifest: {manifest_path}")
    print(f"Worker:   {worker_path}")


if __name__ == "__main__":
    main()
