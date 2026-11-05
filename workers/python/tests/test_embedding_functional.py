# -*- coding: utf-8 -*-
"""embedding-worker 功能测试：确定性、L2 归一化、批量、参数校验、替换点注册表。"""

import math
import unittest

import _harness
from _harness import WorkerProc, assert_execute_result, assert_protocol_response


class EmbeddingFunctionalTest(unittest.TestCase):
    def setUp(self):
        self.w = WorkerProc("embedding", allow_debug=False)
        self.w.send({"jsonrpc": "2.0", "id": "init", "method": "initialize",
                     "params": {"protocol_version": "1"}})
        self.w.recv_for("init")

    def tearDown(self):
        self.w.close()

    def embed(self, task: dict, request_id="x") -> dict:
        self.w.send({"jsonrpc": "2.0", "id": request_id, "method": "execute",
                     "params": {"task": task}})
        msg = self.w.recv_for(request_id)
        assert_protocol_response(msg, request_id)
        return assert_execute_result(msg["result"])

    def test_deterministic_same_text_same_vector(self):
        task = {"text": "determinism check 安全研究"}
        a = self.embed(dict(task), "a")["data"]["vectors"][0]["vector"]
        b = self.embed(dict(task), "b")["data"]["vectors"][0]["vector"]
        assert a == b

    def test_different_text_different_vector(self):
        a = self.embed({"text": "alpha beta gamma"}, "a")["data"]["vectors"][0]["vector"]
        b = self.embed({"text": "completely different words"}, "b")["data"]["vectors"][0]["vector"]
        assert a != b

    def test_l2_normalized(self):
        vec = self.embed({"text": "normalize me please with several tokens"},
                         "n")["data"]["vectors"][0]
        norm = math.sqrt(sum(v * v for v in vec["vector"]))
        assert abs(norm - 1.0) < 1e-5, f"l2 norm {norm}"
        assert vec["dim"] == 256 and len(vec["vector"]) == 256
        assert vec["nnz"] > 0
        assert abs(vec["l2_norm"] - 1.0) < 1e-5

    def test_zero_vector_for_empty_text(self):
        vec = self.embed({"text": ""}, "z")["data"]["vectors"][0]
        assert vec["nnz"] == 0 and vec["l2_norm"] == 0.0
        assert all(v == 0.0 for v in vec["vector"])

    def test_batch_order_preserved(self):
        data = self.embed({"texts": ["first doc", "second doc", "third doc"]},
                          "b")["data"]
        assert data["model"] == "hashing-ngram-v1"
        assert data["deterministic"] is True
        vectors = data["vectors"]
        assert [v["index"] for v in vectors] == [0, 1, 2]
        assert vectors[0]["vector"] != vectors[1]["vector"]
        single = self.embed({"text": "second doc"}, "s")["data"]["vectors"][0]["vector"]
        assert vectors[1]["vector"] == single

    def test_dim_128_rejected(self):
        result = self.embed({"text": "x", "dim": 128})
        assert result["status"] == "error"
        assert result["error"]["code"] == -32602

    def test_unknown_model_rejected(self):
        result = self.embed({"text": "x", "model": "bge-large-onnx"})
        assert result["status"] == "error"
        assert result["error"]["code"] == -32602
        assert "hashing-ngram-v1" in result["error"]["message"]

    def test_invalid_ngram_range_rejected(self):
        result = self.embed({"text": "x", "ngram_range": [0, 2]})
        assert result["status"] == "error"
        assert result["error"]["code"] == -32602

    def test_char_bigram_ngram_range(self):
        data = self.embed({"text": "token overlap check", "ngram_range": [2, 2]},
                          "n2")["data"]
        assert data["ngram_range"] == [2, 2]
        assert data["vectors"][0]["nnz"] > 0

    def test_batch_too_large_rejected(self):
        result = self.embed({"texts": ["x"] * 65})
        assert result["status"] == "error"
        assert result["error"]["code"] == -32602


if __name__ == "__main__":
    unittest.main()
