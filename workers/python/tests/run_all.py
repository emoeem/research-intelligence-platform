#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""M3 契约测试统一入口：python3 tests/run_all.py 或
python3 -m unittest discover -s workers/python/tests（需在该目录可导入 _harness）。"""

import sys
import unittest
from pathlib import Path

TESTS_DIR = Path(__file__).resolve().parent


def main() -> int:
    sys.path.insert(0, str(TESTS_DIR))
    loader = unittest.TestLoader()
    suite = loader.discover(start_dir=str(TESTS_DIR), top_level_dir=str(TESTS_DIR))
    runner = unittest.TextTestRunner(verbosity=2)
    result = runner.run(suite)
    total = result.testsRun
    failed = len(result.failures) + len(result.errors)
    print(f"\nSUMMARY tests={total} failed={failed} "
          f"errors={len(result.errors)} failures={len(result.failures)} "
          f"skipped={len(result.skipped)} ok={result.wasSuccessful()}")
    return 0 if result.wasSuccessful() else 1


if __name__ == "__main__":
    sys.exit(main())
