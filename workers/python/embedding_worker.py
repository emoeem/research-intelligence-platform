#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""Embedding Worker：确定性 hashing vectorizer（M3，stdlib-only 占位实现）。

输入 execute.task：
  {"text": "..."}                                  单条
  {"texts": ["...", "..."]}                        批量（<=64 条）
  {"dim": 256}（可选，v1 只支持 256）
  {"ngram_range": [1, 2]}（可选，1..3）
  {"model": "hashing-ngram-v1"}（可选）
输出 data：
  {model, dim, ngram_range, deterministic, vectors: [{index, dim, nnz, l2_norm, vector}]}

=== 真实模型替换点 ===
本 worker 的向量来自 signed hashing trick（sha256 词元哈希 -> 256 维 + L2 归一化），
纯确定性、零依赖，用于索引/管线联调，不是语义向量。接入真实模型（如 bge/gte ONNX、
sentence-transformers、外部推理服务）时：

1. 实现 `VectorModel` 接口（见下方类注释）：embed(texts: list[str]) -> list[list[float]]。
2. 在 `MODEL_REGISTRY` 注册：MODEL_REGISTRY["bge-small-zh-onnx"] = MyOnnxModel(...)。
3. host 调用时传 task.model 切换；未知 model 名返回 -32602。
4. 换模型不影响协议层：budget/幂等/截断逻辑全部保留。幂等指纹含 worker_version，
   模型升级时递增 VERSION 即可使旧缓存失效。

注意：真实模型引入第三方依赖与模型文件，违反 M3 stdlib-only 约束，故仅留接口。
"""

from __future__ import annotations

import hashlib
import re
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent / "runtime"))

from worker_runtime import (WorkerRuntime, WorkerContext, InvalidParams,
                            InputError)

DEFAULT_DIM = 256
MAX_BATCH = 64
MAX_TEXT_CHARS = 1_000_000

_WORD_RE = re.compile(r"[A-Za-z0-9_']+")
_CJK_RE = re.compile(r"[\u3400-\u4dbf\u4e00-\u9fff\uf900-\ufaff\u3040-\u30ff]")


def tokenize(text: str) -> list[str]:
    """与 parsing worker 同口径：ASCII 词（小写）+ 逐个 CJK/假名字符。"""
    return _WORD_RE.findall(text.lower()) + _CJK_RE.findall(text)


class VectorModel:
    """真实模型替换点：实现本接口并在 MODEL_REGISTRY 注册即可。

    约定：
    - embed(texts) 返回等长的 list[list[float]]，维度固定 dim=256，已 L2 归一化。
    - 结果必须确定（同输入同输出），协议层幂等缓存依赖这一点。
    - 实现内部可以做 IO/推理，但不得触碰协议通道（stdout/stderr）。
    """

    name: str = "abstract"
    dim: int = DEFAULT_DIM

    def embed(self, texts: list[str]) -> list[list[float]]:
        raise NotImplementedError


class HashingNgramVectorizer(VectorModel):
    """sha256 signed hashing trick：词元 n-gram -> 256 维稀疏向量 + L2 归一化。

    bucket = int(sha256(f"n{n}|{gram}") 前 8 字节) % dim
    sign   = sha256 第 9 字节奇偶（+1/-1），降低碰撞偏置
    权重   = 词元出现次数；最终 L2 归一化（零向量保持零）。
    """

    name = "hashing-ngram-v1"
    dim = DEFAULT_DIM

    def __init__(self, ngram_range: tuple[int, int] = (1, 2)):
        lo, hi = ngram_range
        if not (1 <= lo <= hi <= 3):
            raise ValueError(f"invalid ngram_range: {ngram_range}")
        self.ngram_range = (lo, hi)

    def _grams(self, toks: list[str]) -> list[str]:
        lo, hi = self.ngram_range
        out: list[str] = []
        for n in range(lo, hi + 1):
            if n == 1:
                out.extend(toks)
            else:
                out.extend(" ".join(toks[i:i + n])
                           for i in range(len(toks) - n + 1))
        return out

    def embed(self, texts: list[str]) -> list[list[float]]:
        vectors: list[list[float]] = []
        for text in texts:
            vec = [0.0] * self.dim
            toks = tokenize(text)
            for gram in self._grams(toks):
                digest = hashlib.sha256(f"n|{gram}".encode("utf-8")).digest()
                idx = int.from_bytes(digest[:8], "big") % self.dim
                sign = 1.0 if digest[8] & 1 else -1.0
                vec[idx] += sign
            norm = sum(v * v for v in vec) ** 0.5
            if norm > 0:
                vec = [v / norm for v in vec]
            vectors.append([round(v, 6) for v in vec])
        return vectors


MODEL_REGISTRY: dict[str, VectorModel] = {
    HashingNgramVectorizer.name: HashingNgramVectorizer,
}


class EmbeddingWorker(WorkerRuntime):
    NAME = "embedding-worker"
    VERSION = "0.1.0"
    CAPABILITIES = ["embed.hashing-ngram-v1", "embed.dim-256", "embed.batch"]

    def execute(self, ctx: WorkerContext, task: dict) -> dict:
        if "op" in task:  # op 前缀保留给运行时（debug sleep/boom 等）
            return super().execute(ctx, task)
        model_name = task.get("model", HashingNgramVectorizer.name)
        if not isinstance(model_name, str) or model_name not in MODEL_REGISTRY:
            raise InvalidParams(
                f"unknown model {model_name!r}, available: {sorted(MODEL_REGISTRY)}")
        dim = task.get("dim", DEFAULT_DIM)
        if dim != DEFAULT_DIM:
            raise InvalidParams(
                f"only dim={DEFAULT_DIM} is supported in protocol v1 "
                f"(real models may register other dims later)",
                data={"requested": dim, "supported": [DEFAULT_DIM]})

        ngram_range = task.get("ngram_range", [1, 2])
        if (not isinstance(ngram_range, list) or len(ngram_range) != 2
                or not all(isinstance(v, int) and not isinstance(v, bool)
                           for v in ngram_range)):
            raise InvalidParams("task.ngram_range must be [int, int] in 1..3")
        model_cls = MODEL_REGISTRY[model_name]
        try:
            model = model_cls(tuple(ngram_range))
        except ValueError as exc:
            raise InvalidParams(str(exc))

        texts = self._collect_texts(task)
        ctx.check_cancelled()
        vectors = model.embed(texts)
        ctx.check_cancelled()

        out = []
        for i, vec in enumerate(vectors):
            norm = sum(v * v for v in vec) ** 0.5
            out.append({
                "index": i,
                "dim": DEFAULT_DIM,
                "nnz": sum(1 for v in vec if v != 0.0),
                "l2_norm": round(norm, 6),
                "vector": vec,
            })
        return {
            "model": model_name,
            "dim": DEFAULT_DIM,
            "ngram_range": list(ngram_range),
            "deterministic": True,
            "vectors": out,
        }

    def _collect_texts(self, task: dict) -> list[str]:
        texts: list[str] | None = None
        if "texts" in task:
            texts = task.get("texts")
            if not isinstance(texts, list) or not all(isinstance(t, str) for t in texts):
                raise InvalidParams("task.texts must be a list of strings")
        elif "text" in task:
            single = task.get("text")
            if not isinstance(single, str):
                raise InvalidParams("task.text must be a string")
            texts = [single]
        else:
            raise InvalidParams("task requires text or texts")
        if not texts or len(texts) > MAX_BATCH:
            raise InvalidParams(f"task.texts length must be in [1, {MAX_BATCH}]")
        for t in texts:
            if len(t) > MAX_TEXT_CHARS:
                raise InputError(f"text exceeds {MAX_TEXT_CHARS} chars")
        return texts


if __name__ == "__main__":
    EmbeddingWorker().serve()
