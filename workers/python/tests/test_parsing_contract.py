# -*- coding: utf-8 -*-
"""parsing-worker 协议契约测试：完整走 initialize -> execute -> cancel -> timeout -> 幂等。"""

import unittest

import _harness
from _harness import ContractMixin


class ParsingWorkerContract(ContractMixin, unittest.TestCase):
    WORKER = "parsing"

    def expected_worker_name(self) -> str:
        return "parsing-worker"


if __name__ == "__main__":
    unittest.main()
