# -*- coding: utf-8 -*-
"""evaluation-worker 协议契约测试。"""

import unittest

import _harness
from _harness import ContractMixin


class EvaluationWorkerContract(ContractMixin, unittest.TestCase):
    WORKER = "evaluation"

    def expected_worker_name(self) -> str:
        return "evaluation-worker"


if __name__ == "__main__":
    unittest.main()
