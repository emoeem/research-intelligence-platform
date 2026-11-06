# -*- coding: utf-8 -*-
"""embedding-worker 真实模型后端测试（M8）。

环境守卫：
- 依赖缺失（无 onnxruntime/tokenizers）或模型文件缺失时，真实模型用例
  skipUnless 跳过，CI（无模型环境）保持全绿。
- 回退路径用例不依赖模型可用性，任何环境都执行（CI 走 ImportError 分支，
  本地走 FileNotFoundError 分支，殊途同归 backend="hash-fallback"）。
"""

from __future__ import annotations

import math
import os
import sys
import unittest
from contextlib import contextmanager
from pathlib import Path

import _harness
from _harness import WorkerProc, assert_execute_result, assert_protocol_response

WORKERS_PY = _harness.WORKERS_PY
REPO_ROOT = WORKERS_PY.parents[1]
MODEL_DIR = REPO_ROOT / "models" / "bge-small-zh-v1.5"

sys.path.insert(0, str(WORKERS_PY))

_MISSING = object()


def _probe_model_env() -> tuple[bool, str]:
    """(可用, 原因)：以 worker 子进程的真实视角探测，而不是测试进程的
    可导入性。测试进程能 import 不代表 -I 隔离子进程能 import（例如依赖
    装在 user site 时，-I 的 -s 会把 user site 排除掉，实测踩过：
    onnxruntime 在系统 site 可见、tokenizers 在 user site 不可见，子进程
    回退 hash 而用例误跑导致断言失败）。因此守卫 = 模型文件齐全 + 真实
    起一次 worker 拿到 onnx 后端。"""
    for fname in ("model.onnx", "tokenizer.json"):
        if not (MODEL_DIR / fname).is_file():
            return False, f"model file missing: {MODEL_DIR / fname}"
    try:
        w = WorkerProc("embedding", cache_dir=None)
    except Exception as exc:  # noqa: BLE001
        return False, f"worker spawn failed: {exc}"
    try:
        w.send({"jsonrpc": "2.0", "id": "init", "method": "initialize",
                "params": {"protocol_version": "1"}})
        w.recv_for("init")
        w.send({"jsonrpc": "2.0", "id": "probe", "method": "execute",
                "params": {"task": {"text": "probe", "model": "bge-small-zh-v1.5"}}})
        resp = w.recv_for("probe")
    except Exception as exc:  # noqa: BLE001
        return False, f"worker probe failed: {exc}"
    finally:
        w.close()
    backend = ((resp.get("result") or {}).get("data") or {}).get("backend", "")
    if not backend.startswith("onnx:"):
        for line in w.drain_stderr():
            if "embedding_backend_fallback" in line:
                import json as _json
                try:
                    reason = _json.loads(line).get("reason", "")
                except Exception:  # noqa: BLE001
                    reason = line[:200]
                return False, f"worker fell back to {backend or 'unknown'}: {reason}"
        return False, f"worker backend is {backend or 'unknown'}, not onnx"
    return True, "ready"


MODEL_ENV_READY, _MODEL_ENV_REASON = _probe_model_env()
requires_model = unittest.skipUnless(MODEL_ENV_READY,
                                     f"onnx model env unavailable: {_MODEL_ENV_REASON}")


@contextmanager
def _worker_env(**values):
    """临时改 os.environ（None 表示删除），WorkerProc spawn 时继承。"""
    saved = {k: os.environ.get(k, _MISSING) for k in values}
    for k, v in values.items():
        if v is None:
            os.environ.pop(k, None)
        else:
            os.environ[k] = v
    try:
        yield
    finally:
        for k, old in saved.items():
            if old is _MISSING:
                os.environ.pop(k, None)
            else:
                os.environ[k] = old


def _cosine(a: list[float], b: list[float]) -> float:
    dot = sum(x * y for x, y in zip(a, b))
    na = math.sqrt(sum(x * x for x in a))
    nb = math.sqrt(sum(x * x for x in b))
    if na == 0 or nb == 0:
        return 0.0
    return dot / (na * nb)


class EmbeddingModelUnitTest(unittest.TestCase):
    """进程内直接构造 BgeOnnxModel：维度、确定性、L2、中文语义排序。"""

    @requires_model
    def setUp(self):
        import embedding_worker as ew
        self.ew = ew
        self.model = ew.BgeOnnxModel(MODEL_DIR)

    def test_dim_is_512(self):
        self.assertEqual(self.model.dim, 512)
        vec = self.model.embed(["一句话"])[0]
        self.assertEqual(len(vec), 512)

    def test_deterministic_same_text_same_vector(self):
        a = self.model.embed(["确定性检查 安全研究"])[0]
        b = self.model.embed(["确定性检查 安全研究"])[0]
        self.assertEqual(a, b)

    def test_l2_normalized(self):
        vec = self.model.embed(["normalize me please with several tokens"])[0]
        norm = math.sqrt(sum(v * v for v in vec))
        self.assertAlmostEqual(norm, 1.0, delta=1e-5)

    def test_empty_text_no_crash_and_normalized(self):
        vec = self.model.embed([""])[0]
        self.assertEqual(len(vec), 512)
        norm = math.sqrt(sum(v * v for v in vec))
        self.assertAlmostEqual(norm, 1.0, delta=1e-5)

    def test_chinese_similar_above_dissimilar(self):
        sim_texts = ["安全研究中的漏洞扫描方法", "渗透测试的漏洞扫描技术"]
        dissimilar = "今天天气很好，适合出去野餐"
        a, b, c = self.model.embed([sim_texts[0], sim_texts[1], dissimilar])
        sim_ab = _cosine(a, b)
        sim_ac = _cosine(a, c)
        self.assertGreater(sim_ab, sim_ac + 0.1,
                           f"sim_ab={sim_ab:.4f} sim_ac={sim_ac:.4f}")

    def test_long_text_truncated_at_512_tokens(self):
        vec = self.model.embed(["测试 " * 20000])[0]  # 远超 512 token
        self.assertEqual(len(vec), 512)
        norm = math.sqrt(sum(v * v for v in vec))
        self.assertAlmostEqual(norm, 1.0, delta=1e-5)


class EmbeddingModelProtocolTest(unittest.TestCase):
    """协议层：真实后端经 stdio JSON-RPC 全流程 + 强制回退。"""

    def setUp(self):
        self._env_stack = []

    def tearDown(self):
        for env in reversed(self._env_stack):
            env.__exit__(None, None, None)

    def _spawn(self, **env_values) -> WorkerProc:
        ctx = _worker_env(**env_values)
        ctx.__enter__()
        self._env_stack.append(ctx)
        w = WorkerProc("embedding", allow_debug=False)
        w.send({"jsonrpc": "2.0", "id": "init", "method": "initialize",
                "params": {"protocol_version": "1"}})
        self._init_msg = w.recv_for("init")
        return w

    def _execute(self, w: WorkerProc, task: dict, request_id="x") -> dict:
        w.send({"jsonrpc": "2.0", "id": request_id, "method": "execute",
                "params": {"task": task}})
        msg = w.recv_for(request_id)
        assert_protocol_response(msg, request_id)
        return assert_execute_result(msg["result"])

    def test_hash_default_response_has_backend_field(self):
        """旧请求（不带 model）响应新增 backend 字段，向后兼容。"""
        w = self._spawn(EMBEDDING_MODEL_DIR=None, EMBEDDING_BACKEND=None)
        try:
            r = self._execute(w, {"text": "hello world 安全研究"})
            d = r["data"]
            self.assertEqual(d["model"], "hashing-ngram-v1")
            self.assertEqual(d["backend"], "hash")
            self.assertEqual(d["dim"], 256)
            self.assertTrue(d["deterministic"])
        finally:
            w.close()

    @requires_model
    def test_onnx_backend_end_to_end(self):
        w = self._spawn(EMBEDDING_MODEL_DIR=str(MODEL_DIR),
                        EMBEDDING_BACKEND=None)
        try:
            task = {"text": "安全研究中的漏洞扫描方法",
                    "model": "bge-small-zh-v1.5"}
            r1 = self._execute(w, task, "a")
            r2 = self._execute(w, task, "b")
            d1, d2 = r1["data"], r2["data"]
            self.assertEqual(d1["backend"], "onnx:bge-small-zh-v1.5")
            self.assertEqual(d1["model"], "bge-small-zh-v1.5")
            self.assertEqual(d1["dim"], 512)
            self.assertTrue(d1["deterministic"])
            self.assertEqual(d1["vectors"], d2["vectors"])  # 同输入同输出
            vec = d1["vectors"][0]
            self.assertEqual(len(vec["vector"]), 512)
            self.assertAlmostEqual(vec["l2_norm"], 1.0, delta=1e-5)
        finally:
            w.close()

    @requires_model
    def test_onnx_backend_chinese_similarity_protocol_level(self):
        w = self._spawn(EMBEDDING_MODEL_DIR=str(MODEL_DIR),
                        EMBEDDING_BACKEND=None)
        try:
            d = self._execute(w, {"texts": [
                "安全研究中的漏洞扫描方法", "渗透测试的漏洞扫描技术",
                "今天天气很好，适合出去野餐"],
                "model": "bge-small-zh-v1.5"}, "sim")["data"]
            self.assertEqual([v["index"] for v in d["vectors"]], [0, 1, 2])
            v = [x["vector"] for x in d["vectors"]]
            sim_ab = _cosine(v[0], v[1])
            sim_ac = _cosine(v[0], v[2])
            self.assertGreater(sim_ab, sim_ac + 0.1,
                               f"sim_ab={sim_ab:.4f} sim_ac={sim_ac:.4f}")
        finally:
            w.close()

    @requires_model
    def test_batch_result_equals_single_result(self):
        """逐条推理保证：批内向量与单条结果严格一致（批组合无关）。"""
        w = self._spawn(EMBEDDING_MODEL_DIR=str(MODEL_DIR),
                        EMBEDDING_BACKEND=None)
        try:
            batch = self._execute(w, {"texts": ["first doc 安全", "second doc 研究"],
                                      "model": "bge-small-zh-v1.5"}, "b")["data"]
            single = self._execute(w, {"text": "second doc 研究",
                                       "model": "bge-small-zh-v1.5"}, "s")["data"]
            self.assertEqual(batch["vectors"][1]["vector"],
                             single["vectors"][0]["vector"])
        finally:
            w.close()

    @requires_model
    def test_dim_512_accepted_dim_256_rejected(self):
        w = self._spawn(EMBEDDING_MODEL_DIR=str(MODEL_DIR),
                        EMBEDDING_BACKEND=None)
        try:
            r = self._execute(w, {"text": "x", "model": "bge-small-zh-v1.5",
                                  "dim": 512})
            self.assertEqual(r["data"]["dim"], 512)
            bad = self._execute(w, {"text": "x", "model": "bge-small-zh-v1.5",
                                    "dim": 256}, "bad")
            self.assertEqual(bad["status"], "error")
            self.assertEqual(bad["error"]["code"], -32602)
        finally:
            w.close()

    @requires_model
    def test_ngram_range_rejected_for_onnx_model(self):
        w = self._spawn(EMBEDDING_MODEL_DIR=str(MODEL_DIR),
                        EMBEDDING_BACKEND=None)
        try:
            r = self._execute(w, {"text": "x", "model": "bge-small-zh-v1.5",
                                  "ngram_range": [2, 2]})
            self.assertEqual(r["status"], "error")
            self.assertEqual(r["error"]["code"], -32602)
        finally:
            w.close()

    def test_forced_fallback_wrong_model_dir(self):
        """EMBEDDING_MODEL_DIR 指错路径 -> 自动回退 hash，backend=hash-fallback。"""
        w = self._spawn(EMBEDDING_MODEL_DIR="/nonexistent/model-dir-xyz",
                        EMBEDDING_BACKEND=None)
        try:
            r = self._execute(w, {"text": "回退路径测试", "model": "bge-small-zh-v1.5"})
            d = r["data"]
            self.assertEqual(d["backend"], "hash-fallback")
            self.assertEqual(d["model"], "bge-small-zh-v1.5")
            self.assertEqual(d["dim"], 256)
            self.assertTrue(d["deterministic"])
            r2 = self._execute(w, {"text": "回退路径测试",
                                   "model": "bge-small-zh-v1.5"}, "again")
            self.assertEqual(r2["data"]["vectors"], d["vectors"])  # 回退仍确定性
        finally:
            w.close()

    def test_forced_fallback_env_flag(self):
        """EMBEDDING_BACKEND=hash 强制回退（CI/排障开关）。"""
        w = self._spawn(EMBEDDING_MODEL_DIR=str(MODEL_DIR),
                        EMBEDDING_BACKEND="hash")
        try:
            r = self._execute(w, {"text": "x", "model": "bge-small-zh-v1.5"})
            self.assertEqual(r["data"]["backend"], "hash-fallback")
            self.assertEqual(r["data"]["dim"], 256)
        finally:
            w.close()

    def test_fallback_dim_conflict_rejected(self):
        """真实模型不可用且显式 dim=512 与回退维度 256 冲突 -> 显式 -32602。"""
        w = self._spawn(EMBEDDING_MODEL_DIR="/nonexistent/model-dir-xyz",
                        EMBEDDING_BACKEND=None)
        try:
            r = self._execute(w, {"text": "x", "model": "bge-small-zh-v1.5",
                                  "dim": 512})
            self.assertEqual(r["status"], "error")
            self.assertEqual(r["error"]["code"], -32602)
            self.assertIn("conflicts with fallback", r["error"]["message"])
        finally:
            w.close()

    def test_worker_version_bumped(self):
        """模型接入后 worker_version 递增，旧幂等缓存随指纹失效。"""
        w = self._spawn(EMBEDDING_MODEL_DIR=None, EMBEDDING_BACKEND=None)
        try:
            self.assertEqual(self._init_msg["result"]["worker_version"], "0.2.0")
        finally:
            w.close()


if __name__ == "__main__":
    unittest.main()
