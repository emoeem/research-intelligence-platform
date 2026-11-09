#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""RTIS 知识栈只读 Adapter worker（B5 批次，ROADMAP P1-7d）。

红线（TOOL-INVENTORY:161）：不修改生产 RTIS 行为；本 worker 只做进程编排，
两个入口均硬编码在本文件内（不从 task 取，固定入口）：
- op=cli    → [python3, -I, integrations/rtis/rt.py, *args]
              （rt.py FTS5 CLI 面；登记面只开放 poc search 检索路径，
              shape_task 侧只产 search 参数，mode=ro，rt.py:557）
- op=engine → [node, integrations/rtis/rtis_kb.mjs]，stdin 喂
              {"cmd":"search",...} JSON（JS 检索引擎调用面）

task 形状：{"surface": "cli"|"engine", "args": [string,...]（cli 面）,
"stdin_payload": string（engine 面，JSON 文本，上限 64 KiB）}。
（surface 而非 op：op 键保留给 worker runtime 的调试面。）
目标输出原样返回 {exit_code, stdout, stderr_tail}，由平台侧结构化解析
（Phase 4 Context Broker 摄取面）。
"""
from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "workers" / "python" / "runtime"))

from worker_runtime import (InvalidParams, WorkerContext,  # noqa: E402
                            WorkerRuntime)

ROOT = Path(__file__).resolve().parents[1]
RTIS = ROOT / "integrations" / "rtis"
MAX_ARGS = 16
MAX_ARG_CHARS = 200
MAX_PAYLOAD_CHARS = 65536
SCRIPT_TIMEOUT_S = 30
STDERR_TAIL_CHARS = 2000


class RtisKbWorker(WorkerRuntime):
    NAME = "rtis-kb-worker"
    VERSION = "0.1.0"
    CAPABILITIES = ["rtis.kb.readonly"]

    def execute(self, ctx: WorkerContext, task: dict) -> dict:
        if "op" in task:  # op 前缀保留给运行时（debug sleep/boom 等）
            return super().execute(ctx, task)
        if not isinstance(task, dict):
            raise InvalidParams("task must be an object")
        surface = task.get("surface")
        if surface not in ("cli", "engine"):
            raise InvalidParams('surface must be "cli" or "engine"')

        args = task.get("args", [])
        if not isinstance(args, list) or len(args) > MAX_ARGS:
            raise InvalidParams(f"args must be a list of at most {MAX_ARGS} strings")
        clean: list[str] = []
        for a in args:
            if not isinstance(a, str) or not a.strip() or len(a) > MAX_ARG_CHARS:
                raise InvalidParams(
                    f"each arg must be a non-empty string of at most {MAX_ARG_CHARS} chars"
                )
            clean.append(a)

        stdin_payload: str | None = None
        if surface == "cli":
            argv = ["python3", "-I", str(RTIS / "rt.py"), *clean]
        else:
            if clean:
                raise InvalidParams("engine surface takes stdin_payload only, no args")
            payload = task.get("stdin_payload")
            if (
                not isinstance(payload, str)
                or not payload
                or len(payload) > MAX_PAYLOAD_CHARS
            ):
                raise InvalidParams(
                    f"engine surface requires stdin_payload (JSON string, at most "
                    f"{MAX_PAYLOAD_CHARS} chars)"
                )
            try:
                json.loads(payload)
            except ValueError as e:
                raise InvalidParams(f"stdin_payload must be valid JSON: {e}") from None
            argv = ["node", str(RTIS / "rtis_kb.mjs")]
            stdin_payload = payload

        ctx.check_cancelled()
        proc = subprocess.run(
            argv,
            capture_output=True,
            text=True,
            cwd=str(ROOT),
            timeout=SCRIPT_TIMEOUT_S,
            check=False,
            input=stdin_payload,
        )
        ctx.check_cancelled()
        return {
            "exit_code": proc.returncode,
            "stdout": proc.stdout,
            "stderr_tail": proc.stderr[-STDERR_TAIL_CHARS:],
        }


if __name__ == "__main__":
    RtisKbWorker().serve()
