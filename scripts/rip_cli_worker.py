#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""RIP 只读 CLI 脚本与内部 kit 的 Worker Protocol v1 薄包装（B2/B4/B5 批次）。

固定入口（均从本文件自身 argv 取，不从 task 取，杜绝 task 指定任意目标）：
1. 文件模式：`rip_cli_worker.py [--env K=V ...] <script-relpath>`
   子进程固定 argv [python3, -I, <script>, *args]，脚本须位于平台仓库根下。
2. 模块模式：`rip_cli_worker.py [--env K=V ...] --module <name> --pythonpath <relpath>`
   pythonpath 相对平台仓库根解析（可指向工作区根下的内部 kit），解析后必须
   落在工作区根内；子进程固定 argv
   [python3, -I, -c, <sys.path 引导代码>, *args]（-I 隔离忽略 PYTHONPATH
   环境变量，故用 -c 显式引导；不经 shell）。
--env 只接受大写键名（平台侧固定注入，如 RTIS_ERI_ADAPTER_AUDIT=off），
与 os.environ 合并后传给子进程。

task 形状：{"args": [string, ...]}，另含两个可选通用能力：
- "stdin_payload"：字符串（UTF-8，上限 64 KiB），原样写入子进程 stdin 后关闭；
- "task_file"：JSON 字符串（上限 64 KiB，写入前 json.loads 校验），物化为
  临时文件，args 中每个 "{TASK_FILE}" 占位符替换为该路径，进程结束即删
  （适配 eri_adapter.py bridge-gate --task-file 的文件契约）。

目标输出原样返回 {exit_code, stdout, stderr_tail}，由平台侧结构化 Adapter
解析。脚本与 kit 本体零修改。
"""
from __future__ import annotations

import json
import os
import subprocess
import sys
import tempfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "workers" / "python" / "runtime"))

from worker_runtime import (InvalidParams, WorkerContext,  # noqa: E402
                            WorkerRuntime)

ROOT = Path(__file__).resolve().parents[1]
WORKSPACE = ROOT.parent
MAX_ARGS = 32
MAX_ARG_CHARS = 200
MAX_PAYLOAD_CHARS = 65536
SCRIPT_TIMEOUT_S = 60
STDERR_TAIL_CHARS = 2000
TASK_FILE_TOKEN = "{TASK_FILE}"


def _contained(path: Path, root: Path) -> bool:
    return root in path.parents or path == root


def _parse_argv(argv: list[str]):
    """拆分 [--env K=V ...] 前缀与后续模式段，返回 (env_pairs, rest)。"""
    env_pairs: list[tuple[str, str]] = []
    i = 1
    while i < len(argv) and argv[i] == "--env":
        if i + 1 >= len(argv):
            raise SystemExit("--env requires KEY=VALUE")
        pair = argv[i + 1]
        key, sep, value = pair.partition("=")
        if not sep or not key or not key[0].isascii() or not key[0].isupper():
            raise SystemExit(f"bad --env pair: {pair!r} (uppercase key required)")
        if not all(c.isascii() and (c.isupper() or c.isdigit() or c == "_") for c in key):
            raise SystemExit(f"bad --env key: {key!r}")
        env_pairs.append((key, value))
        i += 2
    return env_pairs, argv[i:]


def _resolve_file(rest: list[str]) -> Path:
    if not rest:
        raise SystemExit("usage: rip_cli_worker.py [--env K=V ...] <script-relpath>")
    script = (ROOT / rest[0]).resolve()
    if not _contained(script, ROOT):
        raise SystemExit(f"script must live under repo root: {script}")
    if not script.is_file():
        raise SystemExit(f"script not found: {script}")
    return script


def _resolve_module(rest: list[str]) -> tuple[str, str]:
    if len(rest) < 4 or rest[0] != "--module" or rest[2] != "--pythonpath":
        raise SystemExit(
            "usage: rip_cli_worker.py [--env K=V ...] --module <name> --pythonpath <relpath>"
        )
    module, rel = rest[1], rest[3]
    src = (ROOT / rel).resolve()
    if not _contained(src, WORKSPACE):
        raise SystemExit(f"pythonpath must live under workspace root: {src}")
    if not src.is_dir():
        raise SystemExit(f"pythonpath not a directory: {src}")
    if not module.replace("_", "").isalnum():
        raise SystemExit(f"bad module name: {module}")
    return module, str(src)


ENV_PAIRS, REST = _parse_argv(sys.argv)
if REST and REST[0] == "--module":
    MODULE, SRC = _resolve_module(REST)
    BOOTSTRAP = (
        f"import sys; sys.path.insert(0, {SRC!r}); "
        f"from {MODULE}.cli import main; raise SystemExit(main())"
    )
    MODE = "module"
    CAP = f"cli.module.{MODULE}"
else:
    SCRIPT = _resolve_file(REST)
    MODE = "file"
    CAP = f"cli.readonly.{SCRIPT.stem}"


class RipCliWorker(WorkerRuntime):
    NAME = "rip-cli-worker"
    VERSION = "0.3.0"
    CAPABILITIES = [CAP]

    def execute(self, ctx: WorkerContext, task: dict) -> dict:
        if "op" in task:  # op 前缀保留给运行时（debug sleep/boom 等）
            return super().execute(ctx, task)
        if not isinstance(task, dict):
            raise InvalidParams("task must be an object")
        args = task.get("args")
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
        if "stdin_payload" in task:
            p = task["stdin_payload"]
            if not isinstance(p, str) or len(p) > MAX_PAYLOAD_CHARS:
                raise InvalidParams(
                    f"stdin_payload must be a string of at most {MAX_PAYLOAD_CHARS} chars"
                )
            stdin_payload = p

        task_file_value: str | None = None
        if "task_file" in task:
            p = task["task_file"]
            if not isinstance(p, str) or len(p) > MAX_PAYLOAD_CHARS:
                raise InvalidParams(
                    f"task_file must be a JSON string of at most {MAX_PAYLOAD_CHARS} chars"
                )
            try:
                json.loads(p)
            except ValueError as e:
                raise InvalidParams(f"task_file must be valid JSON: {e}") from None
            task_file_value = p
            if TASK_FILE_TOKEN not in clean:
                raise InvalidParams(
                    f"task_file requires at least one {TASK_FILE_TOKEN} placeholder in args"
                )
            clean = [c.replace(TASK_FILE_TOKEN, "{TEMP}") for c in clean]

        if MODE == "file":
            argv = ["python3", "-I", str(SCRIPT), *clean]
        else:
            argv = ["python3", "-I", "-c", BOOTSTRAP, *clean]
        env = None
        if ENV_PAIRS:
            env = dict(os.environ)
            env.update(dict(ENV_PAIRS))

        ctx.check_cancelled()
        tmp_path: str | None = None
        try:
            if task_file_value is not None:
                with tempfile.NamedTemporaryFile(
                    "w", suffix=".json", encoding="utf-8", delete=False
                ) as tmp:
                    tmp.write(task_file_value)
                    tmp_path = tmp.name
                argv = [a.replace("{TEMP}", tmp_path) if isinstance(a, str) else a for a in argv]
            proc = subprocess.run(
                argv,
                capture_output=True,
                text=True,
                cwd=str(ROOT),
                timeout=SCRIPT_TIMEOUT_S,
                check=False,
                input=stdin_payload,
                env=env,
            )
        finally:
            if tmp_path is not None:
                try:
                    os.unlink(tmp_path)
                except OSError:
                    pass
        ctx.check_cancelled()
        return {
            "exit_code": proc.returncode,
            "stdout": proc.stdout,
            "stderr_tail": proc.stderr[-STDERR_TAIL_CHARS:],
        }


if __name__ == "__main__":
    RipCliWorker().serve()
