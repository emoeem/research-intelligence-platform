# -*- coding: utf-8 -*-
"""embedding-worker 协议契约测试。"""

import unittest

import _harness
from _harness import ContractMixin


class EmbeddingWorkerContract(ContractMixin, unittest.TestCase):
    WORKER = "embedding"

    def expected_worker_name(self) -> str:
        return "embedding-worker"


if __name__ == "__main__":
    unittest.main()
