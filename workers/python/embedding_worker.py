#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""Embedding Worker：hash 占位后端 + bge-small-zh-v1.5 ONNX 真实模型后端（M3/M8）。

输入 execute.task：
  {"text": "..."}                                  单条
  {"texts": ["...", "..."]}                        批量（<=64 条）
  {"model": "hashing-ngram-v1"}                    可选，默认 hash 占位后端
  {"model": "bge-small-zh-v1.5"}                   真实模型后端（ONNX，512 维）
  {"dim": N}                                       可选；必须与所选模型输出维度一致
  {"ngram_range": [1, 2]}                          可选；仅 hashing-ngram-v1 支持
输出 data：
  {model, backend, dim, deterministic, vectors: [{index, dim, nnz, l2_norm, vector}]}
  hashing-ngram-v1 额外含 ngram_range。

=== 后端 ===
- "hashing-ngram-v1"（默认）：sha256 signed hashing trick -> 256 维 + L2 归一化，
  纯确定性、零依赖，语义能力为零，用于管线联调与回归基线。
- "bge-small-zh-v1.5"：ONNX Runtime CPU 推理（BAAI bge-small-zh-v1.5，BERT，
  hidden=512），输出 512 维 L2 归一化语义向量。模型文件与 tokenizer.json 从
  models/bge-small-zh-v1.5/（或环境变量 EMBEDDING_MODEL_DIR 指向的目录）加载，
  不存在或依赖缺失时自动回退 hash 后端，响应 backend="hash-fallback" 标注。

backend 字段取值：
  "hash"            按请求（含默认）使用 hash 后端
  "onnx:bge-small-zh-v1.5"        ONNX 真实模型后端
  "hash-fallback"   请求了真实模型但不可用，回退 hash

维度协商：不静默截断。task.dim 缺省 = 接受所选后端的输出维度；显式给出时必须
等于该后端输出维度（hash=256，bge=512），否则 -32602。真实模型不可用且显式
dim 与回退维度 256 冲突时显式报错，不静默错维。

环境变量：
  EMBEDDING_MODEL_DIR   模型目录（需含 model.onnx 与 tokenizer.json），
                        缺省为仓库根 models/bge-small-zh-v1.5/
  EMBEDDING_BACKEND=hash  强制回退 hash 后端（CI/排障用）

模型升级时递增 VERSION 即可使旧幂等缓存失效（指纹含 worker_version）。
"""

from __future__ import annotations

import hashlib
import os
import re
import sys
import threading
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent / "runtime"))

from worker_runtime import (WorkerRuntime, WorkerContext, InvalidParams,
                            InputError)

DEFAULT_DIM = 256
MAX_BATCH = 64
MAX_TEXT_CHARS = 1_000_000

ONNX_MODEL_NAME = "bge-small-zh-v1.5"
ONNX_MODEL_DIM = 512
ONNX_MAX_SEQ_TOKENS = 512
ONNX_INTRA_OP_THREADS = 4  # 固定线程数，保证同机确定性（同输入同输出）

BACKEND_HASH = "hash"
BACKEND_ONNX = f"onnx:{ONNX_MODEL_NAME}"
BACKEND_HASH_FALLBACK = "hash-fallback"

MODEL_DIR_ENV = "EMBEDDING_MODEL_DIR"
FORCE_HASH_ENV = "EMBEDDING_BACKEND"

_WORD_RE = re.compile(r"[A-Za-z0-9_']+")
_CJK_RE = re.compile(r"[\u3400-\u4dbf\u4e00-\u9fff\uf900-\ufaff\u3040-\u30ff]")

_REPO_ROOT = Path(__file__).resolve().parents[2]


def tokenize(text: str) -> list[str]:
    """与 parsing worker 同口径：ASCII 词（小写）+ 逐个 CJK/假名字符。"""
    return _WORD_RE.findall(text.lower()) + _CJK_RE.findall(text)


class VectorModel:
    """向量模型接口：实现 embed() 并在 MODEL_REGISTRY 注册即可。

    约定：
    - embed(texts) 返回等长的 list[list[float]]，维度固定为类属性 dim，
      已 L2 归一化。
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


class BgeOnnxModel(VectorModel):
    """bge-small-zh-v1.5 ONNX 后端：onnxruntime CPU + tokenizers，512 维。

    - tokenizer.json 为 BERT WordPiece fast tokenizer，截断至 512 token。
    - 导出图含 sentence_embedding 输出（CLS pooling + L2 归一化已固化进图）；
      若只有 last_hidden_state 则取 [CLS] 位并自行归一化。
    - 逐条推理（batch=1，无 padding 进图），不依赖批内其他文本长度，
      严格满足"同输入同输出"。
    - onnxruntime Session.run 线程安全，实例可并发复用（见 _MODEL_CACHE）。
    """

    name = ONNX_MODEL_NAME
    dim = ONNX_MODEL_DIM

    def __init__(self, model_dir: Path):
        import numpy as np
        import onnxruntime as ort
        from tokenizers import Tokenizer

        self._np = np
        model_path = model_dir / "model.onnx"
        tokenizer_path = model_dir / "tokenizer.json"
        if not model_path.is_file():
            raise FileNotFoundError(f"model file missing: {model_path}")
        if not tokenizer_path.is_file():
            raise FileNotFoundError(f"tokenizer file missing: {tokenizer_path}")

        self._tokenizer = Tokenizer.from_file(str(tokenizer_path))
        self._tokenizer.no_padding()
        self._tokenizer.enable_truncation(max_length=ONNX_MAX_SEQ_TOKENS)

        opts = ort.SessionOptions()
        opts.intra_op_num_threads = ONNX_INTRA_OP_THREADS
        opts.inter_op_num_threads = 1
        self._session = ort.InferenceSession(
            str(model_path), sess_options=opts,
            providers=["CPUExecutionProvider"])
        self._output_name = self._resolve_output()

    def _resolve_output(self) -> str:
        outputs = {o.name: o.shape for o in self._session.get_outputs()}
        if "sentence_embedding" in outputs:
            return "sentence_embedding"
        if "last_hidden_state" in outputs:
            return "last_hidden_state"
        raise RuntimeError(f"unsupported onnx outputs: {sorted(outputs)}")

    def _embed_one(self, text: str) -> list[float]:
        np = self._np
        enc = self._tokenizer.encode(text)
        ids = np.asarray([enc.ids], dtype=np.int64)
        feeds = {
            "input_ids": ids,
            "attention_mask": np.asarray([enc.attention_mask], dtype=np.int64),
            "token_type_ids": np.asarray([enc.type_ids], dtype=np.int64),
        }
        # 只喂模型声明过的输入，兼容不同导出图的输入集合
        declared = {i.name for i in self._session.get_inputs()}
        feeds = {k: v for k, v in feeds.items() if k in declared}
        out = self._session.run([self._output_name], feeds)[0]
        if self._output_name == "last_hidden_state":
            out = out[:, 0, :]  # CLS pooling（bge 约定）
        row = out[0]
        norm = float(np.linalg.norm(row))
        if norm > 0:
            row = row / norm
        return [round(float(v), 6) for v in row]

    def embed(self, texts: list[str]) -> list[list[float]]:
        return [self._embed_one(t) for t in texts]


def default_model_dir() -> Path:
    env = os.environ.get(MODEL_DIR_ENV)
    if env:
        return Path(env)
    return _REPO_ROOT / "models" / ONNX_MODEL_NAME


# 已加载会话缓存（key=模型目录），避免每个请求重复 mmap + 建图
_MODEL_CACHE: dict[str, BgeOnnxModel] = {}
_MODEL_CACHE_LOCK = threading.Lock()


def _get_or_load_onnx_model(model_dir: Path) -> BgeOnnxModel:
    key = str(model_dir)
    with _MODEL_CACHE_LOCK:
        cached = _MODEL_CACHE.get(key)
    if cached is not None:
        return cached
    model = BgeOnnxModel(model_dir)  # 加载在锁外，慢加载不阻塞并发请求
    with _MODEL_CACHE_LOCK:
        _MODEL_CACHE[key] = model
    return model


MODEL_REGISTRY: dict[str, type] = {
    HashingNgramVectorizer.name: HashingNgramVectorizer,
    BgeOnnxModel.name: BgeOnnxModel,
}


class EmbeddingWorker(WorkerRuntime):
    NAME = "embedding-worker"
    VERSION = "0.2.0"
    CAPABILITIES = ["embed.hashing-ngram-v1", "embed.dim-256",
                    "embed.bge-small-zh-v1.5", "embed.dim-512",
                    "embed.batch"]

    def execute(self, ctx: WorkerContext, task: dict) -> dict:
        if "op" in task:  # op 前缀保留给运行时（debug sleep/boom 等）
            return super().execute(ctx, task)
        model_name = task.get("model", HashingNgramVectorizer.name)
        if not isinstance(model_name, str) or model_name not in MODEL_REGISTRY:
            raise InvalidParams(
                f"unknown model {model_name!r}, available: {sorted(MODEL_REGISTRY)}")

        ngram_range = task.get("ngram_range", [1, 2])
        if (not isinstance(ngram_range, list) or len(ngram_range) != 2
                or not all(isinstance(v, int) and not isinstance(v, bool)
                           for v in ngram_range)):
            raise InvalidParams("task.ngram_range must be [int, int] in 1..3")
        if model_name != HashingNgramVectorizer.name and "ngram_range" in task:
            raise InvalidParams(
                "task.ngram_range only applies to model "
                f"{HashingNgramVectorizer.name!r}")

        dim = task.get("dim")
        if dim is not None and (isinstance(dim, bool) or not isinstance(dim, int)):
            raise InvalidParams("task.dim must be an int")

        model, backend = self._resolve_model(ctx, model_name,
                                             tuple(ngram_range), dim)
        if dim is not None and dim != model.dim:
            raise InvalidParams(
                f"task.dim={dim} does not match model {model_name!r} "
                f"output dim {model.dim}",
                data={"requested": dim, "supported": [model.dim]})

        texts = self._collect_texts(task)
        ctx.check_cancelled()
        vectors = model.embed(texts)
        ctx.check_cancelled()

        out = []
        for i, vec in enumerate(vectors):
            norm = sum(v * v for v in vec) ** 0.5
            out.append({
                "index": i,
                "dim": model.dim,
                "nnz": sum(1 for v in vec if v != 0.0),
                "l2_norm": round(norm, 6),
                "vector": vec,
            })
        resp = {
            "model": model_name,
            "backend": backend,
            "dim": model.dim,
            "deterministic": True,
            "vectors": out,
        }
        if backend in (BACKEND_HASH, BACKEND_HASH_FALLBACK):
            # hash 路径（含回退）保留 ngram_range 字段，向后兼容旧消费者
            resp["ngram_range"] = list(ngram_range)
        return resp

    def _resolve_model(self, ctx: WorkerContext, model_name: str,
                       ngram_range: tuple[int, int],
                       requested_dim) -> tuple[VectorModel, str]:
        """构造向量模型，返回 (model, backend)。

        hashing-ngram-v1 直接构造；真实模型不可用（依赖缺失/模型文件缺失/
        加载失败）时回退 hash 后端。若调用方显式给了 dim 且与回退维度冲突，
        显式报错而不是静默错维回退。
        """
        if model_name == HashingNgramVectorizer.name:
            try:
                return HashingNgramVectorizer(ngram_range), BACKEND_HASH
            except ValueError as exc:
                raise InvalidParams(str(exc))

        if os.environ.get(FORCE_HASH_ENV, "").strip().lower() == "hash":
            reason = f"forced by {FORCE_HASH_ENV}=hash"
        else:
            try:
                return _get_or_load_onnx_model(default_model_dir()), BACKEND_ONNX
            except Exception as exc:  # noqa: BLE001 依赖缺失/文件缺失/加载失败均回退
                reason = f"{type(exc).__name__}: {exc}"

        if requested_dim is not None and requested_dim != DEFAULT_DIM:
            raise InvalidParams(
                f"model {model_name!r} unavailable and task.dim="
                f"{requested_dim} conflicts with fallback dim {DEFAULT_DIM}; "
                "refusing silent-dim fallback",
                data={"requested_dim": requested_dim,
                      "fallback_dim": DEFAULT_DIM, "reason": reason})
        ctx.runtime.log("warn", "embedding_backend_fallback",
                        request_id=ctx.request_id,
                        requested_model=model_name, reason=reason)
        model = HashingNgramVectorizer(ngram_range)
        return model, BACKEND_HASH_FALLBACK

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
