#!/usr/bin/env python3
"""调试面 worker（P1-6 e2e 专用 fixture，不在 workers/ 目录）。

直接实例化真实 WorkerRuntime 基类：在 WORKER_ALLOW_DEBUG=1 下继承其
sleep（协作取消检查）/boom（EXECUTION_ERROR）调试 op，用于确定性驱动
超时（status=timeout -32002）与取消（TaskCancelled -32004）路径。
"""
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[4] / "workers" / "python" / "runtime"))

from worker_runtime import WorkerRuntime  # noqa: E402


class DebugWorker(WorkerRuntime):
    NAME = "debug-worker"
    VERSION = "0.1.0"
    CAPABILITIES = ["debug.sleep", "debug.boom"]


if __name__ == "__main__":
    DebugWorker().serve()
