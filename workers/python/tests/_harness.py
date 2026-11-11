# -*- coding: utf-8 -*-
"""契约测试公共骨架：子进程方式驱动 worker 的 stdio 协议客户端 + 响应校验。"""

from __future__ import annotations

import json
import os
import queue
import subprocess
import sys
import tempfile
import threading
import time
import unittest
from pathlib import Path

WORKERS_PY = Path(__file__).resolve().parents[1]

WORKER_SCRIPTS = {
    "parsing": WORKERS_PY / "parsing_worker.py",
    "embedding": WORKERS_PY / "embedding_worker.py",
    "evaluation": WORKERS_PY / "evaluation_worker.py",
    "catalog": WORKERS_PY / "catalog_worker.py",
    "audit": WORKERS_PY / "extension_audit_worker.py",
}

# 每个 worker 的"常规任务"与"大输出任务"（用于截断用例）
HAPPY_TASKS = {
    "parsing": {"task": {"text": "# Title\nbody one\n\n## Sub\nbody two",
                         "format": "md"}},
    "embedding": {"task": {"text": "hello world 安全研究"}},
    "evaluation": {"task": {"entries": [
        {"id": "e1", "title": "t", "content": "c", "source": "https://a"}]}},
    "catalog": {"task": {"source_id": "sk3-contract",
                         "text": "# A\n\n- [x](https://a.example/x): d\n",
                         "format": "md"}},
    "audit": {"task": {"text": "# benign\nnothing to flag here\n",
                       "name": "README.md"}},
}

BIG_TASKS = {
    "parsing": {"task": {"text": "# Big Doc\n" + ("lorem ipsum dolor line\n" * 3000),
                         "format": "md"}},
    "embedding": {"task": {"text": "alpha beta gamma delta epsilon " * 8}},
    "evaluation": {"task": {"entries": [
        {"id": f"x{i}", "title": f"title {i}",
         "content": "body text " * 30, "source": f"https://src/{i}"}
        for i in range(200)]}},
    "catalog": {"task": {"source_id": "sk3-big",
                         "text": "# Big\n"
                                 + "".join(f"- [n{i}](https://h.example/{i}): d\n"
                                           for i in range(600)),
                         "format": "md"}},
    "audit": {"task": {"text": "sudo rm -rf / --no-preserve-root\n" * 400,
                       "name": "probe.md"}},
}


def canonical(value) -> str:
    return json.dumps(value, sort_keys=True, ensure_ascii=False,
                      separators=(",", ":"))


class WorkerProc:
    """子进程 worker 客户端：stdin 写请求行，stdout/stderr 各由独立线程泵入队列。"""

    def __init__(self, name: str, allow_debug: bool = True,
                 cache_dir: str | None = None, timeout_kill: float = 10.0):
        script = WORKER_SCRIPTS[name]
        env = os.environ.copy()
        env["WORKER_CACHE_DIR"] = cache_dir or tempfile.mkdtemp(prefix="m3cache-")
        if allow_debug:
            env["WORKER_ALLOW_DEBUG"] = "1"
        else:
            env.pop("WORKER_ALLOW_DEBUG", None)
        self.cache_dir = env["WORKER_CACHE_DIR"]
        self.allow_debug = allow_debug
        self.proc = subprocess.Popen(
            [sys.executable, "-I", str(script)],
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
            env=env, text=True, encoding="utf-8", bufsize=1)
        self._out: queue.Queue = queue.Queue()
        self._err: queue.Queue = queue.Queue()
        self._pump_out = threading.Thread(target=self._pump,
                                          args=(self.proc.stdout, self._out), daemon=True)
        self._pump_err = threading.Thread(target=self._pump,
                                          args=(self.proc.stderr, self._err), daemon=True)
        self._pump_out.start()
        self._pump_err.start()

    @staticmethod
    def _pump(stream, sink: queue.Queue):
        try:
            for line in stream:
                sink.put(line.rstrip("\n"))
        finally:
            sink.put(None)

    @property
    def alive(self) -> bool:
        return self.proc.poll() is None

    def send(self, obj: dict) -> None:
        assert self.alive, "worker exited unexpectedly"
        self.proc.stdin.write(json.dumps(obj, ensure_ascii=False) + "\n")
        self.proc.stdin.flush()

    def send_raw(self, line: str) -> None:
        self.proc.stdin.write(line + "\n")
        self.proc.stdin.flush()

    def recv(self, timeout: float = 15.0):
        """任一响应行（dict），EOF 返回 None，超时抛 AssertionError。"""
        try:
            item = self._out.get(timeout=timeout)
        except queue.Empty:
            raise AssertionError(f"no response within {timeout}s; "
                                 f"stderr tail: {self.stderr_tail()}")
        if item is None:
            return None
        return json.loads(item)

    def recv_for(self, request_id, timeout: float = 20.0) -> dict:
        """取 id 匹配的响应，忽略乱序的其它帧。"""
        deadline = time.monotonic() + timeout
        other = []
        while time.monotonic() < deadline:
            remaining = max(0.1, deadline - time.monotonic())
            item = self._out.get(timeout=remaining)
            if item is None:
                raise AssertionError("worker EOF while waiting for response; "
                                     f"stderr tail: {self.stderr_tail()}")
            msg = json.loads(item)
            if "id" in msg and canonical(msg["id"]) == canonical(request_id):
                return msg
            other.append(msg)
        raise AssertionError(f"no response for id={request_id!r} within {timeout}s; "
                             f"saw: {other[:5]}; stderr tail: {self.stderr_tail()}")

    def stderr_tail(self, limit: int = 8) -> str:
        lines = []
        while True:
            try:
                item = self._err.get_nowait()
            except queue.Empty:
                break
            if item is not None:
                lines.append(item)
        return " | ".join(lines[-limit:])

    def drain_stderr(self) -> list[str]:
        out = []
        while True:
            try:
                item = self._err.get_nowait()
            except queue.Empty:
                return out
            if item is not None:
                out.append(item)

    def close(self, force: bool = False):
        try:
            if self.proc.poll() is None:
                try:
                    self.proc.stdin.close()
                except OSError:
                    pass
                try:
                    self.proc.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    self.proc.terminate()
                    try:
                        self.proc.wait(timeout=3)
                    except subprocess.TimeoutExpired:
                        self.proc.kill()
                        self.proc.wait(timeout=3)
        finally:
            # 进程退出后管道可能仍有尾部数据在 pump 线程途中：必须等两个
            # 读取线程结束，drain_stdout/drain_stderr 才能看到完整流
            #（此前不等导致 CI 偶发读到空 stderr，test_stderr_structured_logs_stdout_clean 报 ERROR）
            for t in (self._pump_out, self._pump_err):
                t.join(timeout=5)


# ---------------------------------------------------------------- 契约断言

def assert_protocol_response(msg: dict, request_id=None) -> dict:
    """校验 JSON-RPC 信封层。返回 msg。"""
    assert isinstance(msg, dict), f"response not an object: {msg!r}"
    assert msg.get("jsonrpc") == "2.0", f"jsonrpc field wrong: {msg!r}"
    if request_id is not None:
        assert canonical(msg.get("id")) == canonical(request_id), \
            f"response id mismatch: {msg!r}"
    has_result, has_error = "result" in msg, "error" in msg
    assert has_result != has_error, f"need exactly one of result|error: {msg!r}"
    return msg


def assert_execute_result(result: dict) -> dict:
    """校验 execute 响应的 result 层（v1 契约形状）。"""
    for field in ("worker_name", "worker_version", "protocol_version",
                  "status", "cached", "usage"):
        assert field in result, f"execute result missing {field}: {result!r}"
    assert result["protocol_version"] == "1"
    assert result["status"] in ("result", "error", "timeout"), \
        f"bad status: {result['status']!r}"
    assert isinstance(result["cached"], bool)
    usage = result["usage"]
    for field in ("tokens_est", "elapsed_ms", "output_bytes", "truncated"):
        assert field in usage, f"usage missing {field}: {usage!r}"
    assert isinstance(usage["tokens_est"], int) and usage["tokens_est"] >= 0
    assert isinstance(usage["elapsed_ms"], int) and usage["elapsed_ms"] >= 0
    assert isinstance(usage["output_bytes"], int) and usage["output_bytes"] > 0
    assert isinstance(usage["truncated"], bool)
    if result["status"] == "result":
        assert isinstance(result.get("data"), dict), \
            f"status=result requires dict data: {result!r}"
    else:
        err = result.get("error")
        assert isinstance(err, dict), f"status={result['status']} requires error: {result!r}"
        assert isinstance(err.get("code"), int) and isinstance(err.get("message"), str)
    return result


def protocol_error_code(msg: dict) -> int:
    assert_protocol_response(msg)
    assert "error" in msg, f"expected top-level error: {msg!r}"
    assert isinstance(msg["error"].get("code"), int)
    return msg["error"]["code"]


def task_error_code(result: dict) -> int:
    assert_execute_result(result)
    assert result["status"] == "error", f"expected status=error: {result!r}"
    return result["error"]["code"]


# ---------------------------------------------------------------- 契约用例 Mixin

class ContractMixin:
    """Worker Protocol v1 全流程契约用例，由三个 worker 的 TestCase 共用。

    覆盖：握手 -> 能力 -> 执行 -> 取消 -> 预算超时 -> 幂等命中 -> 截断 -> 崩溃安全。
    每个用例独立起子进程（setUp），互不干扰。
    """

    WORKER: str = ""

    def make_worker(self, **kwargs) -> WorkerProc:
        return WorkerProc(self.WORKER, **kwargs)

    def setUp(self):
        self.w = self.make_worker()

    def tearDown(self):
        self.w.close()

    def initialize(self, worker=None, **params):
        w = worker or self.w
        payload = {"protocol_version": "1"}
        payload.update(params)
        w.send({"jsonrpc": "2.0", "id": "init", "method": "initialize",
                "params": payload})
        return w.recv_for("init")

    # -- 握手 ------------------------------------------------------------

    def test_initialize_ok(self):
        msg = self.initialize()
        assert_protocol_response(msg, "init")
        r = msg["result"]
        assert r["protocol_version"] == "1"
        assert isinstance(r["worker_name"], str) and r["worker_name"]
        assert isinstance(r["worker_version"], str) and r["worker_version"]
        assert isinstance(r["capabilities"], list)

    def test_initialize_wrong_version_rejected(self):
        msg = self.initialize(protocol_version="99")
        code = protocol_error_code(msg)
        assert code == -32000, f"expect -32000, got {code}"
        data = msg["error"].get("data") or {}
        assert data.get("expected") == "1"

    def test_initialize_missing_version_invalid_params(self):
        self.w.send({"jsonrpc": "2.0", "id": 1, "method": "initialize",
                     "params": {}})
        assert protocol_error_code(self.w.recv_for(1)) == -32602

    def test_execute_before_initialize_rejected(self):
        self.w.send({"jsonrpc": "2.0", "id": 1, "method": "execute",
                     "params": HAPPY_TASKS[self.WORKER]})
        assert protocol_error_code(self.w.recv_for(1)) == -32001

    # -- 能力与健康 ------------------------------------------------------

    def test_get_capabilities(self):
        msg = self.initialize()
        self.w.send({"jsonrpc": "2.0", "id": 2, "method": "get_capabilities"})
        msg = self.w.recv_for(2)
        assert_protocol_response(msg, 2)
        r = msg["result"]
        assert set(r["methods"]) == {"initialize", "get_capabilities",
                                     "execute", "cancel", "ping"}
        assert r["initialized"] is True
        assert r["capabilities"] == msg["result"]["capabilities"] or \
            isinstance(r["capabilities"], list)

    def test_ping_pong(self):
        self.w.send({"jsonrpc": "2.0", "id": "p", "method": "ping"})
        msg = self.w.recv_for("p")
        assert_protocol_response(msg, "p")
        assert msg["result"]["pong"] is True
        assert isinstance(msg["result"]["uptime_ms"], int)

    def test_unknown_method(self):
        self.w.send({"jsonrpc": "2.0", "id": 3, "method": "shutdown_now"})
        assert protocol_error_code(self.w.recv_for(3)) == -32601

    # -- 传输层违规 ------------------------------------------------------

    def test_parse_error_line(self):
        self.w.send_raw("this is not json at all")
        msg = self.w.recv()
        assert protocol_error_code(msg) == -32700
        assert msg["id"] is None

    def test_invalid_request_missing_jsonrpc(self):
        self.w.send({"id": 4, "method": "ping"})
        assert protocol_error_code(self.w.recv_for(4)) == -32600

    def test_invalid_request_missing_id(self):
        self.w.send({"jsonrpc": "2.0", "method": "ping"})
        msg = self.w.recv()
        assert protocol_error_code(msg) == -32600
        assert msg["id"] is None

    # -- execute 主流程 --------------------------------------------------

    def test_execute_happy_path(self):
        self.initialize()
        self.w.send({"jsonrpc": "2.0", "id": "h", "method": "execute",
                     "params": HAPPY_TASKS[self.WORKER]})
        msg = self.w.recv_for("h")
        assert_protocol_response(msg, "h")
        result = assert_execute_result(msg["result"])
        assert result["status"] == "result" and result["cached"] is False
        assert result["worker_name"] == self.expected_worker_name()
        assert result["worker_version"]

    def test_execute_output_bytes_exact(self):
        self.initialize()
        self.w.send({"jsonrpc": "2.0", "id": "ob", "method": "execute",
                     "params": HAPPY_TASKS[self.WORKER]})
        # 直接读原始行核字节数
        raw = None
        deadline = time.monotonic() + 15
        while time.monotonic() < deadline and raw is None:
            item = self.w._out.get(timeout=5)
            if item is not None:
                msg = json.loads(item)
                if msg.get("id") == "ob":
                    raw = item
        assert raw is not None
        result = assert_execute_result(msg["result"])
        assert result["status"] == "result"
        assert result["usage"]["output_bytes"] == len(raw.encode("utf-8")), \
            f"usage.output_bytes={result['usage']['output_bytes']} " \
            f"but line is {len(raw.encode('utf-8'))} bytes"

    def test_budget_invalid_zero_timeout(self):
        self.initialize()
        params = dict(HAPPY_TASKS[self.WORKER])
        params["budget"] = {"timeout_ms": 0}
        self.w.send({"jsonrpc": "2.0", "id": 5, "method": "execute",
                     "params": params})
        assert protocol_error_code(self.w.recv_for(5)) == -32003

    def test_budget_invalid_tiny_output(self):
        self.initialize()
        params = dict(HAPPY_TASKS[self.WORKER])
        params["budget"] = {"max_output_bytes": 16}
        self.w.send({"jsonrpc": "2.0", "id": 6, "method": "execute",
                     "params": params})
        assert protocol_error_code(self.w.recv_for(6)) == -32003

    # -- 预算超时 ---------------------------------------------------------

    def test_budget_timeout_and_worker_survives(self):
        self.initialize()
        params = {"task": {"op": "sleep", "ms": 8000},
                  "budget": {"timeout_ms": 300}}
        self.w.send({"jsonrpc": "2.0", "id": "t", "method": "execute",
                     "params": params})
        msg = self.w.recv_for("t")
        result = assert_execute_result(msg["result"])
        assert result["status"] == "timeout", f"expect timeout, got {result!r}"
        assert result["error"]["code"] == -32002
        # worker 存活且可继续服务
        self.w.send({"jsonrpc": "2.0", "id": "after", "method": "ping"})
        after = self.w.recv_for("after")
        assert after["result"]["pong"] is True

    # -- 取消 -------------------------------------------------------------

    def test_cancel_running_task(self):
        self.initialize()
        self.w.send({"jsonrpc": "2.0", "id": "job", "method": "execute",
                     "params": {"task": {"op": "sleep", "ms": 60000}}})
        time.sleep(0.3)  # 让任务线程起来
        self.w.send({"jsonrpc": "2.0", "id": "c", "method": "cancel",
                     "params": {"target_request_id": "job"}})
        cmsg = self.w.recv_for("c")
        assert_protocol_response(cmsg, "c")
        assert cmsg["result"]["cancelled"] is True
        assert cmsg["result"]["state"] == "running"
        emsg = self.w.recv_for("job")
        result = assert_execute_result(emsg["result"])
        assert result["status"] == "error"
        assert result["error"]["code"] == -32004
        self.w.send({"jsonrpc": "2.0", "id": "alive", "method": "ping"})
        assert self.w.recv_for("alive")["result"]["pong"] is True

    def test_cancel_unknown_target(self):
        self.initialize()
        self.w.send({"jsonrpc": "2.0", "id": "cx", "method": "cancel",
                     "params": {"target_request_id": "no-such"}})
        assert protocol_error_code(self.w.recv_for("cx")) == -32007

    # -- 幂等 -------------------------------------------------------------

    def test_idempotency_cache_hit(self):
        self.initialize()
        key = f"idem-{self.WORKER}"
        params = dict(HAPPY_TASKS[self.WORKER])
        params["idempotency_key"] = key
        self.w.send({"jsonrpc": "2.0", "id": "first", "method": "execute",
                     "params": params})
        first = assert_execute_result(self.w.recv_for("first")["result"])
        assert first["status"] == "result" and first["cached"] is False
        self.w.send({"jsonrpc": "2.0", "id": "second", "method": "execute",
                     "params": params})
        second = assert_execute_result(self.w.recv_for("second")["result"])
        assert second["status"] == "result" and second["cached"] is True
        assert second["data"] == first["data"]
        assert second["usage"]["elapsed_ms"] <= first["usage"]["elapsed_ms"]
        # 缓存文件落盘
        cache_files = list(Path(self.w.cache_dir).glob("*.json"))
        assert len(cache_files) == 1, f"expect 1 cache file, got {cache_files}"

    def test_idempotency_conflict(self):
        self.initialize()
        key = f"conflict-{self.WORKER}"
        params = dict(HAPPY_TASKS[self.WORKER])
        params["idempotency_key"] = key
        self.w.send({"jsonrpc": "2.0", "id": "a", "method": "execute",
                     "params": params})
        assert_execute_result(self.w.recv_for("a")["result"])
        changed = dict(params)
        changed["task"] = dict(params["task"])
        changed["task"]["__mutated__"] = True
        self.w.send({"jsonrpc": "2.0", "id": "b", "method": "execute",
                     "params": changed})
        msg = self.w.recv_for("b")
        assert protocol_error_code(msg) == -32005, f"got {msg!r}"

    def test_idempotency_key_must_be_string(self):
        self.initialize()
        params = dict(HAPPY_TASKS[self.WORKER])
        params["idempotency_key"] = 12345
        self.w.send({"jsonrpc": "2.0", "id": 7, "method": "execute",
                     "params": params})
        assert protocol_error_code(self.w.recv_for(7)) == -32602

    # -- 截断 -------------------------------------------------------------

    def test_output_truncated_within_budget(self):
        self.initialize()
        params = dict(BIG_TASKS[self.WORKER])
        params["budget"] = {"timeout_ms": 20000, "max_output_bytes": 1400}
        self.w.send({"jsonrpc": "2.0", "id": "big", "method": "execute",
                     "params": params})
        raw = None
        deadline = time.monotonic() + 25
        while time.monotonic() < deadline and raw is None:
            item = self.w._out.get(timeout=10)
            if item is not None:
                msg = json.loads(item)
                if msg.get("id") == "big":
                    raw = item
        assert raw is not None, f"no response; stderr: {self.w.stderr_tail()}"
        result = assert_execute_result(msg["result"])
        raw_bytes = len(raw.encode("utf-8"))
        assert raw_bytes <= 1400, f"delivered {raw_bytes} bytes > budget 1400"
        assert result["usage"]["truncated"] is True
        assert result["usage"]["output_bytes"] == raw_bytes
        data = result["data"]
        assert data.get("truncated") is True
        head = data.get("data_head")
        assert isinstance(head, str)
        # data_head 必须是原始 data JSON 的非空前缀，original_bytes 记录其原大小
        assert len(head) > 0
        assert data["original_bytes"] >= len(head.encode("utf-8"))
        # 原始 data 确实大到加信封后超预算（信封开销约数百字节）
        assert data["original_bytes"] > 1400 - 600
        # worker 截断后仍健康
        self.w.send({"jsonrpc": "2.0", "id": "ok", "method": "ping"})
        assert self.w.recv_for("ok")["result"]["pong"] is True

    # -- 崩溃安全与任务级错误 ---------------------------------------------

    def test_handler_crash_returns_error_not_exit(self):
        self.initialize()
        self.w.send({"jsonrpc": "2.0", "id": "boom", "method": "execute",
                     "params": {"task": {"op": "boom"}}})
        result = assert_execute_result(self.w.recv_for("boom")["result"])
        assert result["status"] == "error"
        assert result["error"]["code"] == -32006
        assert self.w.alive
        self.w.send({"jsonrpc": "2.0", "id": "still", "method": "ping"})
        assert self.w.recv_for("still")["result"]["pong"] is True

    def test_unknown_op_task_level_error(self):
        self.initialize()
        self.w.send({"jsonrpc": "2.0", "id": "u", "method": "execute",
                     "params": {"task": {"op": "definitely-not-an-op"}}})
        result = assert_execute_result(self.w.recv_for("u")["result"])
        assert result["status"] == "error"
        assert result["error"]["code"] == -32602

    def test_debug_ops_gated_by_env(self):
        """生产模式（无 WORKER_ALLOW_DEBUG）下 debug op 必须按未知 op 拒绝。"""
        self.initialize()
        w2 = self.make_worker(allow_debug=False)
        try:
            w2.send({"jsonrpc": "2.0", "id": "i2", "method": "initialize",
                     "params": {"protocol_version": "1"}})
            w2.recv_for("i2")
            w2.send({"jsonrpc": "2.0", "id": "s2", "method": "execute",
                     "params": {"task": {"op": "sleep", "ms": 500}}})
            result = assert_execute_result(w2.recv_for("s2")["result"])
            assert result["status"] == "error"
            assert result["error"]["code"] == -32602
        finally:
            w2.close()

    # -- 日志通道 ---------------------------------------------------------

    def test_stderr_structured_logs_stdout_clean(self):
        self.initialize()
        self.w.send({"jsonrpc": "2.0", "id": "log", "method": "execute",
                     "params": HAPPY_TASKS[self.WORKER]})
        self.w.recv_for("log")
        self.w.close()
        lines = self.w.drain_stderr()
        assert lines, "expected structured logs on stderr"
        events = set()
        for line in lines:
            rec = json.loads(line)  # stderr 每行必须是合法 JSON
            events.add(rec.get("event"))
            assert rec.get("worker") == self.expected_worker_name()
        assert "startup" in events and "response_sent" in events

    def expected_worker_name(self) -> str:
        raise NotImplementedError
