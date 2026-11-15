#!/usr/bin/env python3
"""rip.embed.* 嵌入 worker 容器入口（RIP3 P3-3，D4 语义候选面）。

帧协议（TOOL-CONTRACT D-TC-03）：stdin 一行 JSON 请求；stdout 一行 JSON 结果
（stdout 守卫同 parser_tools/gov_tools：帧 = stdout 第一行）。op 取 argv[1]。

op 分派：
- embed  bge-small-zh-v1.5 ONNX 批量语义向量（复用 workers/python/embedding_
         worker.py 的 BgeOnnxModel 真实后端类——同一模型代码零复制导入）：
         input {"texts": [...]}（帽 64 条 / 单条 64KiB）→ {"vectors": [[f32]],
         "dim": 512}（L2 归一化）。

安全边界：纯本地推理，network.mode=none（manifest 强制）；模型构建期烘焙
/opt/models/bge-small-zh-v1.5（EMBEDDING_MODEL_DIR）；无回退面（真实后端不可
用即 -32006 显式报错，不静默降 hash——治理语义向量必须真实）。
"""

from __future__ import annotations

import json
import signal
import sys
from pathlib import Path

MAX_TEXTS = 64
MAX_TEXT_BYTES = 64 * 1024

sys.path.insert(0, "/app/workers/python")


def _on_term(signum, _frame):
    sys.exit(128 + signum)


def _emit(payload):
    print(json.dumps(payload, ensure_ascii=False))
    sys.stdout.flush()


def _emit_error(code, message, exit_code=1):
    frame = {"status": "error", "error": {"code": code, "message": message}}
    print(json.dumps(frame, ensure_ascii=False))
    print(json.dumps(frame, ensure_ascii=False), file=sys.stderr)
    sys.stderr.flush()
    return exit_code


def op_embed(req_input):
    texts = req_input.get("texts")
    if not isinstance(texts, list) or not texts:
        raise ValueError("texts must be a non-empty array")
    if len(texts) > MAX_TEXTS:
        raise ValueError(f"texts exceeds cap {MAX_TEXTS}")
    cleaned = []
    for t in texts:
        if not isinstance(t, str):
            raise ValueError("each text must be a string")
        if len(t.encode("utf-8")) > MAX_TEXT_BYTES:
            raise ValueError("text exceeds 64KiB cap")
        cleaned.append(t)

    from embedding_worker import _get_or_load_onnx_model, default_model_dir
    from importlib.metadata import version

    model = _get_or_load_onnx_model(default_model_dir())
    vectors = model.embed(cleaned)
    return {
        "vectors": vectors,
        "dim": model.dim,
        "model": model.name,
        "backend": f"onnx:{model.name}",
        "onnxruntime": version("onnxruntime"),
    }


OPS = {
    "embed": op_embed,
}


def main(argv):
    signal.signal(signal.SIGTERM, _on_term)
    if len(argv) < 2 or argv[1] not in OPS:
        return _emit_error(-32602, "usage: embed_tools.py <embed>")
    op = argv[1]
    line = sys.stdin.readline()
    if not line.strip():
        return _emit_error(-32700, "empty request line")
    try:
        request = json.loads(line)
    except json.JSONDecodeError as e:
        return _emit_error(-32700, f"request parse failed: {e}")
    req_input = request.get("input") or {}
    real_stdout = sys.stdout
    sys.stdout = sys.stderr
    try:
        try:
            data = OPS[op](req_input)
        except FileNotFoundError as e:
            return _emit_error(-32006, f"model missing: {e}")
        except ValueError as e:
            return _emit_error(-32602, str(e))
        except ImportError as e:
            return _emit_error(-32006, f"engine import failed: {e}")
        except Exception as e:  # noqa: BLE001 容器侧顶层兜底
            return _emit_error(-32006, f"execution error: {type(e).__name__}: {e}")
    finally:
        sys.stdout = real_stdout
    _emit({"status": "result", "data": data})
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
