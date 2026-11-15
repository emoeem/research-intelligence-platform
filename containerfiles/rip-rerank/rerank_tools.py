#!/usr/bin/env python3
"""rip.rerank.* 重排序 worker 容器入口（RIP3 P4-3，R2 6.2 Reranking 面）。

帧协议（TOOL-CONTRACT D-TC-03）：stdin 一行 JSON 请求；stdout 一行 JSON 结果
（stdout 守卫同 embed_tools：帧 = stdout 第一行）。op 取 argv[1]。

op 分派：
- rerank  bge-reranker（Xenova ONNX 导出，XLMRobertaForSequenceClassification）
          pairwise 重排序：input {"query", "texts":[{"id","text"}]}（帽 64 条 /
          单条 8KiB）→ scores 按 query-text 对逐条推理（logit 经 sigmoid = 相关
          性分 0-1），输出按分降序 [{"id","score"}]。调用面据此对混合检索结果
          重排序（排序面；不改检索集——R2 6.2 流程位置：RRF 之后 Dedup 之前）。

安全边界：纯本地推理，network.mode=none（manifest 强制）；模型构建期烘焙
/opt/models/bge-reranker-base-onnx；无回退面（模型缺失即 -32006 显式报错）。
输入构造纪律：XLM-RoBERTa 无 token_type_ids，只喂模型声明过的输入（与
embedding_worker._embed_one 的 declared 过滤同款）。
"""

from __future__ import annotations

import json
import math
import signal
import sys
from pathlib import Path

MAX_TEXTS = 64
MAX_TEXT_BYTES = 8 * 1024

sys.path.insert(0, "/app/workers/python")

MODEL_DIR = Path("/opt/models/bge-reranker-base-onnx")


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


def _load_session():
    import numpy as np
    import onnxruntime as ort
    from tokenizers import Tokenizer

    model_path = MODEL_DIR / "model.onnx"
    tokenizer_path = MODEL_DIR / "tokenizer.json"
    if not model_path.is_file():
        raise FileNotFoundError(f"model file missing: {model_path}")
    if not tokenizer_path.is_file():
        raise FileNotFoundError(f"tokenizer file missing: {tokenizer_path}")
    tokenizer = Tokenizer.from_file(str(tokenizer_path))
    tokenizer.no_padding()
    tokenizer.enable_truncation(max_length=512)
    opts = ort.SessionOptions()
    opts.intra_op_num_threads = 2
    opts.inter_op_num_threads = 1
    session = ort.InferenceSession(str(model_path), sess_options=opts,
                                   providers=["CPUExecutionProvider"])
    declared = {i.name for i in session.get_inputs()}
    output_name = session.get_outputs()[0].name
    return np, tokenizer, session, declared, output_name


def _sigmoid(x):
    return 1.0 / (1.0 + math.exp(-x))


def op_rerank(req_input):
    query = req_input.get("query")
    texts = req_input.get("texts")
    if not isinstance(query, str) or not query.strip():
        raise ValueError("query must be a non-empty string")
    if not isinstance(texts, list) or not texts:
        raise ValueError("texts must be a non-empty array of {id, text}")
    if len(texts) > MAX_TEXTS:
        raise ValueError(f"texts exceeds cap {MAX_TEXTS}")

    np, tokenizer, session, declared, output_name = _load_session()
    results = []
    for it in texts:
        if not isinstance(it, dict) or "id" not in it or "text" not in it:
            raise ValueError("each text must be an object with id and text")
        text = str(it["text"])
        if len(text.encode("utf-8")) > MAX_TEXT_BYTES:
            raise ValueError(f"text {it['id']!r} exceeds 8KiB cap")
        pair = tokenizer.encode(query, text)
        ids = np.asarray([pair.ids], dtype=np.int64)
        feeds = {
            "input_ids": ids,
            "attention_mask": np.asarray([pair.attention_mask], dtype=np.int64),
            "token_type_ids": np.asarray([pair.type_ids], dtype=np.int64),
        }
        feeds = {k: v for k, v in feeds.items() if k in declared}
        out = session.run([output_name], feeds)[0]
        logit = float(np.asarray(out).reshape(-1)[0])
        results.append({"id": str(it["id"]), "score": round(_sigmoid(logit), 6)})
    results.sort(key=lambda r: -r["score"])
    return {
        "scores": results,
        "model": "bge-reranker-base (Xenova ONNX)",
        "count": len(results),
    }


OPS = {
    "rerank": op_rerank,
}


def main(argv):
    signal.signal(signal.SIGTERM, _on_term)
    if len(argv) < 2 or argv[1] not in OPS:
        return _emit_error(-32602, "usage: rerank_tools.py <rerank>")
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
