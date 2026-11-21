#!/usr/bin/env python3
"""RT-2 venv 协议一致性对拍 fixture worker（venv 解释器 vs 系统解释器）。

协议行为 100% 由 workers/python/runtime/worker_runtime.py 基座承载（握手/
预算超时/协作取消/framing 全部不在此文件），子类只做两类动作：
- 捕获不改写：`_send_result` 按到达序缓存前两条 result 响应（adapter 握手序
  恒为 initialize → get_capabilities，python_worker.rs:417/443），用于判据 6
  的原始握手响应逐字段对拍——捕获面在写出之后无关动作，协议行为零分叉；
- execute 任务面覆盖：
  - parity.echo：回显解释器身份面（sys.executable / argv / isolated 标志 /
    numpy 可达性）+ 握手原始响应回放（handshake_replay）——对拍断言的声明
    差异面与 -I 保留断言面；
  - parity.sleep：确定性睡眠（取消安全点循环，与 debug_worker sleep 同款）。

stdlib-only；worker 缓存经 WORKER_CACHE_DIR 重定向（无 idempotency_key
不落缓存）；测试自持（RT-1 fixture 先例，不触 workers/ 目录）。
"""
import importlib.util
import sys
import time
from pathlib import Path

sys.path.insert(
    0, str(Path(__file__).resolve().parents[4] / "workers" / "python" / "runtime")
)

from worker_runtime import InvalidParams, WorkerRuntime  # noqa: E402


class VenvParityWorker(WorkerRuntime):
    NAME = "venv-parity-worker"
    VERSION = "1.0.0"
    CAPABILITIES = ["parity.echo", "parity.sleep"]

    def __init__(self):
        super().__init__()
        # 判据 6 字面达标面：前两条 result = 原始 initialize / get_capabilities
        # 响应（捕获不改写，基座行为零分叉）。
        self._handshake_results = []

    def _send_result(self, request_id, result):
        if len(self._handshake_results) < 2:
            self._handshake_results.append(result)
        super()._send_result(request_id, result)

    def execute(self, ctx, task):
        op = task.get("op")
        if op == "echo":
            replay = {}
            if len(self._handshake_results) >= 1:
                replay["initialize"] = self._handshake_results[0]
            if len(self._handshake_results) >= 2:
                replay["get_capabilities"] = self._handshake_results[1]
            return {
                "op": "echo",
                "protocol_version": self.PROTOCOL_VERSION,
                "worker_name": self.NAME,
                "worker_version": self.VERSION,
                "capabilities": list(self.CAPABILITIES),
                # 原始握手响应捕获回放（判据 6 逐字段对拍面）
                "handshake_replay": replay,
                # 声明差异面（venv 解释器 vs 系统解释器唯一允许的字段差）
                "executable": sys.executable,
                "argv": sys.argv,
                # -I 隔离保留断言面（两解释器必须同为 True）
                "isolated": bool(sys.flags.isolated),
                # 依赖可达性事实面（venv 装了 numpy / 系统面 pacman numpy；
                # 裸 fixture venv 无 numpy——对拍侧单独断言，不进逐字段一致集）
                "numpy_available": importlib.util.find_spec("numpy") is not None,
            }
        if op == "sleep":
            ms = task.get("ms", 1000)
            if not isinstance(ms, int) or ms < 0 or ms > 600_000:
                raise InvalidParams("task.ms must be integer in [0, 600000]")
            deadline = time.monotonic() + ms / 1000.0
            while time.monotonic() < deadline:
                ctx.check_cancelled()
                time.sleep(min(0.02, max(0.0, deadline - time.monotonic())))
            return {"slept_ms": ms, "op": "sleep"}
        raise InvalidParams(f"unknown task op: {op!r}")


if __name__ == "__main__":
    VenvParityWorker().serve()
