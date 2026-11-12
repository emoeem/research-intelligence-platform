#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""Worker Runtime 基类：JSON-RPC 2.0 over stdio（Worker Protocol v1）。

契约文档：research-platform/schemas/worker-protocol/v1.json 与 README.md。

要点：
- stdio 主循环，一行一个 JSON 请求；stdout 只承载协议行，日志全部走 stderr。
- execute 在独立 daemon 线程运行，主循环不阻塞，cancel 可随时到达。
- budget.timeout_ms 用 threading.Timer 强制执行：到点先交付 timeout 响应，再置取消令牌。
- budget.max_output_bytes 约束最终响应行字节数，超限走截断信封。
- idempotency_key 结果缓存到 workers/.cache/<sha256(key)>.json，原子写。
- 崩溃安全：handler 未捕获异常一律回 error 响应，进程不退出。
- 取消是协作式：handler 通过 ctx.check_cancelled() 在安全点退出。

子类只需实现 execute(ctx, task) -> dict，并在收到未知 op 时
fallthrough 到 super().execute() 以获得 debug op 与统一错误。
"""

from __future__ import annotations

import hashlib
import json
import os
import signal
import sys
import threading
import time
import traceback
from pathlib import Path

PROTOCOL_VERSION = "1"

DEFAULT_TIMEOUT_MS = 30_000
MAX_TIMEOUT_MS = 600_000
DEFAULT_MAX_OUTPUT_BYTES = 1_048_576
MIN_MAX_OUTPUT_BYTES = 256
MAX_MAX_OUTPUT_BYTES = 67_883_264  # 64 MiB

WORKERS_DIR = Path(__file__).resolve().parents[2]  # workers/python/runtime -> workers
DEFAULT_CACHE_DIR = WORKERS_DIR / ".cache"
CACHE_FILENAME_BYTES = 32  # sha256 hex 字符数，杜绝 key 到路径的注入

DEBUG_ENV = "WORKER_ALLOW_DEBUG"


class ErrorCodes:
    """协议错误码，与 schemas/worker-protocol/v1.json / README 对齐。"""

    PARSE_ERROR = -32700
    INVALID_REQUEST = -32600
    METHOD_NOT_FOUND = -32601
    INVALID_PARAMS = -32602
    INTERNAL_ERROR = -32603
    PROTOCOL_VERSION_MISMATCH = -32000
    NOT_INITIALIZED = -32001
    BUDGET_TIMEOUT = -32002
    BUDGET_INVALID = -32003
    CANCELLED = -32004
    IDEMPOTENCY_CONFLICT = -32005
    EXECUTION_ERROR = -32006
    UNKNOWN_REQUEST = -32007
    INPUT_ERROR = -32008
    UNSUPPORTED_FORMAT = -32009


class WorkerError(Exception):
    """任务级错误基类：会被包装为 status=error 的 execute 响应，进程不退出。"""

    code = ErrorCodes.EXECUTION_ERROR

    def __init__(self, message: str, data: dict | None = None):
        super().__init__(message)
        self.data = data


class InvalidParams(WorkerError):
    code = ErrorCodes.INVALID_PARAMS


class BudgetInvalid(WorkerError):
    code = ErrorCodes.BUDGET_INVALID


class InputError(WorkerError):
    code = ErrorCodes.INPUT_ERROR


class UnsupportedFormatError(InputError):
    code = ErrorCodes.UNSUPPORTED_FORMAT


class TaskCancelled(WorkerError):
    code = ErrorCodes.CANCELLED


class WorkerContext:
    """传给 handler 的单请求上下文：取消令牌 + 预算 + 日志。"""

    def __init__(self, runtime: "WorkerRuntime", request_id, task: dict,
                 budget_timeout_ms: int, budget_max_output_bytes: int,
                 idempotency_key: str | None):
        self.runtime = runtime
        self.request_id = request_id
        self.task = task
        self.budget = {
            "timeout_ms": budget_timeout_ms,
            "max_output_bytes": budget_max_output_bytes,
        }
        self.idempotency_key = idempotency_key
        self.cancel_event = threading.Event()

    def cancelled(self) -> bool:
        return self.cancel_event.is_set()

    def check_cancelled(self) -> None:
        """取消安全点：已取消则抛 TaskCancelled。handler 应周期性调用。"""
        if self.cancelled():
            raise TaskCancelled("task cancelled by cancel request")

    def log(self, event: str, **fields) -> None:
        self.runtime.log("info", event, request_id=self.request_id, **fields)


class _TaskRecord:
    __slots__ = ("key", "request_id", "task", "ctx", "thread", "timer",
                 "state", "lock", "started", "timeout_ms", "max_output_bytes",
                 "idempotency_key", "correlation", "done_event")

    def __init__(self, key, request_id, task, ctx, timeout_ms, max_output_bytes,
                 idempotency_key, correlation=None):
        self.key = key
        self.request_id = request_id
        self.task = task
        self.ctx = ctx
        self.thread = None
        self.timer = None
        self.state = "running"  # running | done | timeout_delivered
        self.lock = threading.Lock()
        self.started = time.monotonic()
        self.timeout_ms = timeout_ms
        self.max_output_bytes = max_output_bytes
        self.idempotency_key = idempotency_key
        # P7-5 六 ID 关联透传（SESSION-HANDOFF §十追加项 1）：平台下发
        # params.correlation 原样回传 result.correlation（worker 不校验业务
        # 字段、不落盘，只做响应透传；缺省 None 不改既有响应形状）。
        self.correlation = correlation
        self.done_event = threading.Event()


class WorkerRuntime:
    """协议 v1 运行时基类。子类实现 execute() 并设置 NAME/VERSION/CAPABILITIES。"""

    PROTOCOL_VERSION = PROTOCOL_VERSION
    NAME = "abstract-worker"
    VERSION = "0.0.0"
    CAPABILITIES: list[str] = []
    METHODS = ["initialize", "get_capabilities", "execute", "cancel", "ping"]

    def __init__(self, cache_dir: str | Path | None = None):
        env_dir = os.environ.get("WORKER_CACHE_DIR")
        if cache_dir is not None:
            chosen = Path(cache_dir)
        elif env_dir:
            chosen = Path(env_dir)
        else:
            chosen = DEFAULT_CACHE_DIR
        self.cache_dir = chosen
        self._initialized = False
        self._tasks: dict[str, _TaskRecord] = {}
        self._idem_inflight: dict[str, _TaskRecord] = {}
        self._tasks_lock = threading.Lock()
        self._write_lock = threading.Lock()
        self._started = time.monotonic()
        self._stdin = sys.stdin
        self._stdout = sys.stdout

    # ------------------------------------------------------------------ 服务循环

    def serve(self) -> None:
        """stdio 主循环。EOF 或 SIGTERM/SIGINT 时优雅退出。"""
        try:
            self._stdout.reconfigure(encoding="utf-8", newline="\n")
            self._stdin.reconfigure(encoding="utf-8")
        except Exception:
            pass
        try:
            sys.stderr.reconfigure(encoding="utf-8", errors="backslashreplace")
        except Exception:
            pass
        signal.signal(signal.SIGTERM, self._on_signal)
        signal.signal(signal.SIGINT, self._on_signal)

        self.log("info", "startup", worker_version=self.VERSION,
                 pid=os.getpid(), cache_dir=str(self.cache_dir))
        try:
            for raw_line in self._stdin:
                line = raw_line.strip()
                if not line:
                    continue
                self._handle_line(line)
        except SystemExit:
            raise
        except Exception as exc:  # 主循环自身异常：尽力报错并继续
            self.log("error", "main_loop_exception", error=str(exc),
                     traceback="".join(traceback.format_exc()))
        finally:
            self._shutdown()

    def _on_signal(self, signum, _frame):
        self.log("info", "signal", signum=signum)
        raise SystemExit(0)

    def _shutdown(self) -> None:
        self.log("info", "shutdown", active_tasks=len(self._tasks))
        deadline = time.monotonic() + 1.0
        for rec in list(self._tasks.values()):
            remain = deadline - time.monotonic()
            if remain <= 0:
                break
            if rec.thread is not None:
                rec.thread.join(timeout=remain)

    # ------------------------------------------------------------------ 日志

    def log(self, level: str, event: str, **fields) -> None:
        """结构化日志到 stderr，绝不写 stdout。"""
        rec = {"ts": time.strftime("%Y-%m-%dT%H:%M:%S%z"),
               "level": level, "worker": self.NAME, "event": event}
        rec.update(fields)
        try:
            sys.stderr.write(json.dumps(rec, ensure_ascii=False,
                                        separators=(",", ":")) + "\n")
            sys.stderr.flush()
        except Exception:
            pass

    # ------------------------------------------------------------------ 消息处理

    def _handle_line(self, line: str) -> None:
        try:
            msg = json.loads(line)
        except ValueError as exc:
            self._send_error(None, ErrorCodes.PARSE_ERROR,
                             f"request line is not valid JSON: {exc}")
            return

        if not isinstance(msg, dict):
            self._send_error(None, ErrorCodes.INVALID_REQUEST,
                             "request must be a JSON object")
            return
        if msg.get("jsonrpc") != "2.0":
            self._send_error(msg.get("id"), ErrorCodes.INVALID_REQUEST,
                             'jsonrpc field must be "2.0"')
            return
        if "id" not in msg:
            self._send_error(None, ErrorCodes.INVALID_REQUEST,
                             "every request requires an id (request_id)")
            return
        request_id = msg["id"]
        if not isinstance(request_id, (str, int)) and request_id is not None:
            self._send_error(None, ErrorCodes.INVALID_REQUEST,
                             "id must be string, integer or null")
            return
        method = msg.get("method")
        if not isinstance(method, str):
            self._send_error(request_id, ErrorCodes.INVALID_REQUEST,
                             "method must be a string")
            return

        handler = {
            "initialize": self._method_initialize,
            "get_capabilities": self._method_get_capabilities,
            "execute": self._method_execute_entry,
            "cancel": self._method_cancel,
            "ping": self._method_ping,
        }.get(method)
        if handler is None:
            self._send_error(request_id, ErrorCodes.METHOD_NOT_FOUND,
                             f"unknown method: {method}",
                             data={"supported": self.METHODS})
            return
        self.log("info", "request_received", request_id=request_id, method=method)
        try:
            handler(msg)
        except WorkerError as exc:
            self._send_error(request_id, exc.code, str(exc), data=exc.data)
        except Exception as exc:
            self.log("error", "dispatch_exception", request_id=request_id,
                     method=method, error=str(exc),
                     traceback="".join(traceback.format_exc()))
            self._send_error(request_id, ErrorCodes.INTERNAL_ERROR,
                             f"internal error: {exc}")

    # ------------------------------------------------------------------ 方法实现

    def _method_initialize(self, msg: dict) -> None:
        params = msg.get("params")
        if params is None:
            params = {}
        if not isinstance(params, dict):
            raise InvalidParams("params must be an object")
        requested = params.get("protocol_version")
        if not isinstance(requested, str):
            raise InvalidParams("params.protocol_version (string) is required")
        if requested != PROTOCOL_VERSION:
            self._send_error(msg["id"], ErrorCodes.PROTOCOL_VERSION_MISMATCH,
                             f"protocol version mismatch: worker speaks "
                             f"{PROTOCOL_VERSION!r}, client requested {requested!r}",
                             data={"expected": PROTOCOL_VERSION,
                                   "received": requested,
                                   "supported": [PROTOCOL_VERSION]})
            return
        self._initialized = True
        self._send_result(msg["id"], {
            "protocol_version": PROTOCOL_VERSION,
            "worker_name": self.NAME,
            "worker_version": self.VERSION,
            "capabilities": list(self.CAPABILITIES),
        })

    def _method_get_capabilities(self, msg: dict) -> None:
        self._send_result(msg["id"], {
            "protocol_version": PROTOCOL_VERSION,
            "worker_name": self.NAME,
            "worker_version": self.VERSION,
            "methods": list(self.METHODS),
            "capabilities": list(self.CAPABILITIES),
            "initialized": self._initialized,
        })

    def _method_ping(self, msg: dict) -> None:
        self._send_result(msg["id"], {
            "pong": True,
            "worker_version": self.VERSION,
            "uptime_ms": int((time.monotonic() - self._started) * 1000),
            "active_tasks": len(self._tasks),
        })

    def _method_execute_entry(self, msg: dict) -> None:
        if not self._initialized:
            self._send_error(msg["id"], ErrorCodes.NOT_INITIALIZED,
                             "execute requires initialize first")
            return
        params = msg.get("params")
        if params is None:
            params = {}
        if not isinstance(params, dict):
            raise InvalidParams("params must be an object")
        task = params.get("task")
        if not isinstance(task, dict):
            raise InvalidParams("params.task (object) is required")

        budget = params.get("budget") or {}
        if not isinstance(budget, dict):
            raise InvalidParams("params.budget must be an object")
        timeout_ms = budget.get("timeout_ms", DEFAULT_TIMEOUT_MS)
        max_output_bytes = budget.get("max_output_bytes", DEFAULT_MAX_OUTPUT_BYTES)
        if not isinstance(timeout_ms, int) or isinstance(timeout_ms, bool) \
                or not (1 <= timeout_ms <= MAX_TIMEOUT_MS):
            raise BudgetInvalid(
                f"budget.timeout_ms must be integer in [1, {MAX_TIMEOUT_MS}]",
                data={"received": timeout_ms})
        if not isinstance(max_output_bytes, int) or isinstance(max_output_bytes, bool) \
                or not (MIN_MAX_OUTPUT_BYTES <= max_output_bytes <= MAX_MAX_OUTPUT_BYTES):
            raise BudgetInvalid(
                f"budget.max_output_bytes must be integer in "
                f"[{MIN_MAX_OUTPUT_BYTES}, {MAX_MAX_OUTPUT_BYTES}]",
                data={"received": max_output_bytes})

        idem = params.get("idempotency_key")
        if idem is not None:
            if not isinstance(idem, str) or not idem or len(idem) > 512:
                raise InvalidParams(
                    "idempotency_key must be a non-empty string <=512 chars")

        correlation = params.get("correlation")
        if correlation is not None and not isinstance(correlation, dict):
            raise InvalidParams("params.correlation must be an object when present")

        if idem is not None:
            if self._cache_lookup(idem, task, msg["id"], max_output_bytes,
                                  timeout_ms, correlation):
                return  # True=命中或冲突均已回包；False=未命中，继续执行

        rec = _TaskRecord(
            key=_canonical(msg["id"]), request_id=msg["id"], task=task,
            ctx=WorkerContext(self, msg["id"], task, timeout_ms,
                              max_output_bytes, idem),
            timeout_ms=timeout_ms, max_output_bytes=max_output_bytes,
            idempotency_key=idem, correlation=correlation)
        with self._tasks_lock:
            if rec.key in self._tasks:
                raise WorkerError(
                    f"duplicate in-flight request id {msg['id']!r}; "
                    "request ids must be unique among running requests",
                    code=ErrorCodes.INVALID_REQUEST)
        rec.thread = threading.Thread(
            target=self._run_task, args=(rec,),
            name=f"task-{rec.key}", daemon=True)
        with self._tasks_lock:
            self._tasks[rec.key] = rec
            if idem is not None:
                self._idem_inflight[idem] = rec
        rec.timer = threading.Timer(timeout_ms / 1000.0,
                                    self._on_task_timeout, args=(rec,))
        rec.timer.daemon = True
        rec.timer.start()
        rec.thread.start()

    def _method_cancel(self, msg: dict) -> None:
        params = msg.get("params")
        if params is None:
            params = {}
        if not isinstance(params, dict):
            raise InvalidParams("params must be an object")
        target = params.get("target_request_id")
        if "target_request_id" not in params:
            raise InvalidParams("params.target_request_id is required")
        key = _canonical(target)
        with self._tasks_lock:
            rec = self._tasks.get(key)
        if rec is None:
            self._send_error(msg["id"], ErrorCodes.UNKNOWN_REQUEST,
                             f"no running request with id {target!r}")
            return
        rec.ctx.cancel_event.set()
        with rec.lock:
            state = rec.state
        running = state == "running"
        self.log("info", "cancel_requested", request_id=msg["id"],
                 target_request_id=target, was_running=running)
        self._send_result(msg["id"], {
            "cancelled": running,
            "state": "running" if running else "finished",
            "target_request_id": target,
        })

    # ------------------------------------------------------------------ 任务执行

    def execute(self, ctx: WorkerContext, task: dict) -> dict:
        """子类覆盖。返回 dict 作为 data（status=result）。"""
        op = task.get("op")
        if os.environ.get(DEBUG_ENV) == "1":
            if op == "sleep":
                ms = task.get("ms", 1000)
                if not isinstance(ms, int) or ms < 0 or ms > 600_000:
                    raise InvalidParams("task.ms must be integer in [0, 600000]")
                deadline = time.monotonic() + ms / 1000.0
                while time.monotonic() < deadline:
                    ctx.check_cancelled()
                    time.sleep(min(0.02, max(0.0, deadline - time.monotonic())))
                return {"slept_ms": ms, "op": "sleep"}
            if op == "boom":
                raise RuntimeError("debug boom: intentional handler crash")
        raise InvalidParams(f"unknown task op: {op!r}")

    def _run_task(self, rec: _TaskRecord) -> None:
        ctx = rec.ctx
        status, data, err = "result", None, None
        try:
            data = self.execute(ctx, rec.task)
            if not isinstance(data, dict):
                raise WorkerError("execute() must return a dict")
        except TaskCancelled as exc:
            status = "error"
            err = {"code": exc.code, "message": str(exc)}
            self.log("info", "task_cancelled", request_id=rec.request_id)
        except WorkerError as exc:
            status = "error"
            err = {"code": exc.code, "message": str(exc), "data": exc.data}
        except Exception as exc:
            status = "error"
            err = {"code": ErrorCodes.EXECUTION_ERROR,
                   "message": f"uncaught exception: {type(exc).__name__}: {exc}",
                   "data": {"exception_type": type(exc).__name__}}
            self.log("error", "handler_exception", request_id=rec.request_id,
                     error=str(exc), traceback="".join(traceback.format_exc()))

        with rec.lock:
            if rec.state != "running":
                return  # 超时已交付，丢弃迟到结果
            rec.state = "done"
        if rec.timer is not None:
            rec.timer.cancel()

        elapsed_ms = int((time.monotonic() - rec.started) * 1000)
        if status == "result" and rec.idempotency_key:
            self._cache_write(rec.idempotency_key, rec.task, data)
        if rec.idempotency_key:
            with self._tasks_lock:
                if self._idem_inflight.get(rec.idempotency_key) is rec:
                    self._idem_inflight.pop(rec.idempotency_key, None)
        rec.done_event.set()

        usage = {
            "tokens_est": _tokens_est(data if data is not None else {}),
            "elapsed_ms": elapsed_ms,
            "output_bytes": 0,
            "truncated": False,
        }
        result = self._build_execute_result(status, data, err, cached=False,
                                            usage=usage,
                                            correlation=rec.correlation)
        self._remove_task(rec)
        self._send_fitted(rec.request_id, result, rec.max_output_bytes)

    def _on_task_timeout(self, rec: _TaskRecord) -> None:
        with rec.lock:
            if rec.state != "running":
                return
            rec.state = "timeout_delivered"
        rec.ctx.cancel_event.set()
        elapsed_ms = int((time.monotonic() - rec.started) * 1000)
        self.log("warn", "task_timeout", request_id=rec.request_id,
                 timeout_ms=rec.timeout_ms, elapsed_ms=elapsed_ms)
        err = {"code": ErrorCodes.BUDGET_TIMEOUT,
               "message": f"budget timeout after {rec.timeout_ms} ms",
               "data": {"timeout_ms": rec.timeout_ms}}
        usage = {"tokens_est": 0, "elapsed_ms": elapsed_ms,
                 "output_bytes": 0, "truncated": False}
        result = self._build_execute_result("timeout", None, err,
                                            cached=False, usage=usage,
                                            correlation=rec.correlation)
        if rec.idempotency_key:
            with self._tasks_lock:
                if self._idem_inflight.get(rec.idempotency_key) is rec:
                    self._idem_inflight.pop(rec.idempotency_key, None)
        rec.done_event.set()
        self._remove_task(rec)
        self._send_fitted(rec.request_id, result, rec.max_output_bytes)

    def _remove_task(self, rec: _TaskRecord) -> None:
        with self._tasks_lock:
            self._tasks.pop(rec.key, None)

    # ------------------------------------------------------------------ 幂等缓存

    def _cache_path(self, key: str) -> Path:
        digest = hashlib.sha256(key.encode("utf-8")).hexdigest()[:CACHE_FILENAME_BYTES * 2]
        return self.cache_dir / f"{digest}.json"

    def _cache_fingerprint(self, task: dict) -> str:
        payload = json.dumps({"worker_version": self.VERSION, "task": task},
                             sort_keys=True, ensure_ascii=False,
                             separators=(",", ":"))
        return hashlib.sha256(payload.encode("utf-8")).hexdigest()

    def _cache_lookup(self, key: str, task: dict, request_id,
                      max_output_bytes: int, timeout_ms: int,
                      correlation: dict | None = None) -> bool:
        """True=已处理（命中或冲突已回包）；False=未命中，调用方继续执行。

        同 key 的前一任务若仍在飞，先等待其完成（至多 timeout_ms）再查缓存，
        保证流水线并发下同 key 请求串行化，不会重复执行。
        """
        with self._tasks_lock:
            inflight = self._idem_inflight.get(key)
        if inflight is not None:
            inflight.done_event.wait(timeout=timeout_ms / 1000.0)
        path = self._cache_path(key)
        if not path.is_file():
            return False
        try:
            entry = json.loads(path.read_text(encoding="utf-8"))
            fp = entry["params_fingerprint"]
            data = entry["data"]
        except (OSError, ValueError, KeyError, TypeError) as exc:
            self.log("warn", "cache_corrupt", key=key, error=str(exc))
            return False
        if fp != self._cache_fingerprint(task):
            self._send_error(request_id, ErrorCodes.IDEMPOTENCY_CONFLICT,
                             "idempotency_key reused with different task or "
                             "worker_version",
                             data={"idempotency_key": key})
            return True
        usage = {"tokens_est": _tokens_est(data), "elapsed_ms": 0,
                 "output_bytes": 0, "truncated": False}
        result = self._build_execute_result("result", data, None, cached=True,
                                            usage=usage, correlation=correlation)
        self.log("info", "cache_hit", request_id=request_id, key=key)
        self._send_fitted(request_id, result, max_output_bytes)
        return True

    def _cache_write(self, key: str, task: dict, data: dict) -> None:
        path = self._cache_path(key)
        entry = {
            "idempotency_key": key,
            "params_fingerprint": self._cache_fingerprint(task),
            "worker_name": self.NAME,
            "worker_version": self.VERSION,
            "status": "result",
            "data": data,
            "created_at": time.strftime("%Y-%m-%dT%H:%M:%S%z"),
        }
        try:
            self.cache_dir.mkdir(parents=True, exist_ok=True)
            tmp = path.with_suffix(".tmp")
            tmp.write_text(json.dumps(entry, ensure_ascii=False,
                                      separators=(",", ":")),
                           encoding="utf-8")
            os.replace(tmp, path)
            self.log("info", "cache_write", key=key, path=str(path))
        except OSError as exc:
            self.log("warn", "cache_write_failed", key=key, error=str(exc))

    # ------------------------------------------------------------------ 响应发送

    def _build_execute_result(self, status: str, data: dict | None,
                              err: dict | None, cached: bool,
                              usage: dict,
                              correlation: dict | None = None) -> dict:
        result = {
            "worker_name": self.NAME,
            "worker_version": self.VERSION,
            "protocol_version": PROTOCOL_VERSION,
            "status": status,
            "cached": cached,
            "usage": usage,
        }
        if data is not None:
            result["data"] = data
        if err is not None:
            result["error"] = err
        if correlation is not None:
            # P7-5：六 ID 关联原样回传（trace_id/task_id/agent_run_id/
            # tool_run_id/artifact_id/evidence_id 由平台侧组装，worker 只透传）。
            result["correlation"] = correlation
        return result

    def _send_result(self, request_id, result: dict) -> None:
        self._write_line({"jsonrpc": "2.0", "id": request_id, "result": result})
        self.log("info", "response_sent", request_id=request_id, kind="result")

    def _send_error(self, request_id, code: int, message: str,
                    data: dict | None = None) -> None:
        err = {"code": code, "message": message}
        if data is not None:
            err["data"] = data
        self._write_line({"jsonrpc": "2.0", "id": request_id, "error": err})
        self.log("info", "response_sent", request_id=request_id, kind="error",
                 code=code)

    def _write_line(self, obj: dict) -> None:
        line = json.dumps(obj, ensure_ascii=False, separators=(",", ":"))
        with self._write_lock:
            self._stdout.write(line + "\n")
            self._stdout.flush()

    def _send_fitted(self, request_id, result: dict, max_output_bytes: int) -> None:
        """按 budget.max_output_bytes 裁剪并交付 execute 响应。

        超限策略：result.data 换成截断信封；仍超则收缩 data_head；
        最终兜底为 id=null 的最小错误行。
        长度检查用 10 位 output_bytes 占位（保守上界），最终交付前用不动点法
        算出精确的 output_bytes（值与所在行字节数严格一致）。
        """
        usage = result.get("usage")
        if usage is not None:
            usage["output_bytes"] = 9_999_999_999  # 10 位占位，长度上界
        envelope = {"jsonrpc": "2.0", "id": request_id, "result": result}
        raw = _json_bytes(envelope)
        if len(raw) <= max_output_bytes:
            self._finish_execute(request_id, envelope)
            return

        status = result.get("status")
        if status == "result" and isinstance(result.get("data"), dict):
            original = raw
            data = result.pop("data")
            original_bytes = len(_json_bytes(data))
            head_budget = max_output_bytes - (len(_json_bytes(envelope)) + 256)
            while True:
                head = _head_string(_json_bytes(data), head_budget)
                result["data"] = {"truncated": True,
                                  "original_bytes": original_bytes,
                                  "data_head": head}
                envelope = {"jsonrpc": "2.0", "id": request_id, "result": result}
                raw = _json_bytes(envelope)
                if len(raw) <= max_output_bytes or head_budget <= 64:
                    break
                head_budget //= 2
            if len(raw) > max_output_bytes:
                result.pop("data", None)
                envelope = {"jsonrpc": "2.0", "id": request_id, "result": result}
                raw = _json_bytes(envelope)
            if usage is not None:
                usage["truncated"] = True
            self.log("info", "response_truncated", request_id=request_id,
                     original_bytes=len(original), delivered_bytes=len(raw))
            self._finish_execute(request_id, envelope, original_bytes=len(original))
            return

        # error/timeout 超预算（罕见）：收缩 message 后仍超则最小兜底行
        err = result.get("error")
        if isinstance(err, dict):
            err["message"] = err.get("message", "")[:120]
            envelope = {"jsonrpc": "2.0", "id": request_id, "result": result}
            raw = _json_bytes(envelope)
            if len(raw) <= max_output_bytes:
                self._finish_execute(request_id, envelope)
                return
        self._send_minimal_error(request_id, status)

    def _finish_execute(self, request_id, envelope: dict,
                        original_bytes: int = 0) -> None:
        """交付 execute 响应，并让 usage.output_bytes 精确等于本行字节数。

        output_bytes 是行内字段，序列化自引用：以 C = 行长-数字位数 的不动点
        v = C + len(str(v)) 迭代收敛（常数次）。
        """
        usage = envelope.get("result", {}).get("usage")
        if usage is not None:
            placeholder = len(str(usage["output_bytes"]))
            c = len(_json_bytes(envelope)) - placeholder
            v = c + 1
            for _ in range(6):
                nv = c + len(str(v))
                if nv == v:
                    break
                v = nv
            usage["output_bytes"] = v
            if len(_json_bytes(envelope)) != v:  # 极端位数跨界，再收敛一轮
                c = len(_json_bytes(envelope)) - len(str(v))
                v = c + len(str(c + len(str(v))))
                usage["output_bytes"] = v
        raw = _json_bytes(envelope)
        line = raw.decode("utf-8")
        with self._write_lock:
            self._stdout.write(line + "\n")
            self._stdout.flush()
        self.log("info", "response_sent", request_id=request_id, kind="execute",
                 status=envelope.get("result", {}).get("status"),
                 output_bytes=len(raw), original_bytes=original_bytes)

    def _send_minimal_error(self, request_id, status: str | None) -> None:
        code = ErrorCodes.BUDGET_TIMEOUT if status == "timeout" \
            else ErrorCodes.INTERNAL_ERROR
        err = {"code": code, "message": "response could not fit budget"}
        line = json.dumps({"jsonrpc": "2.0", "id": None, "error": err},
                          separators=(",", ":"))
        with self._write_lock:
            self._stdout.write(line + "\n")
            self._stdout.flush()
        self.log("warn", "response_budget_fallback", request_id=request_id)


def _json_bytes(obj) -> bytes:
    return json.dumps(obj, ensure_ascii=False,
                      separators=(",", ":")).encode("utf-8")


def _head_string(raw: bytes, max_bytes: int) -> str:
    """UTF-8 字符边界安全地取 raw 的前 max_bytes 字节并解码。"""
    if max_bytes <= 0:
        return ""
    cut = raw[:max_bytes]
    while cut:
        try:
            return cut.decode("utf-8")
        except UnicodeDecodeError:
            cut = cut[:-1]
    return ""


def _tokens_est(obj) -> int:
    chars = len(json.dumps(obj, ensure_ascii=False, separators=(",", ":")))
    return max(1, (chars + 3) // 4)


def _canonical(value) -> str:
    return json.dumps(value, sort_keys=True, ensure_ascii=False,
                      separators=(",", ":"))
