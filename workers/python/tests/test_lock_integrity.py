# -*- coding: utf-8 -*-
"""RT-2 lock 完整性静态校验（纯 stdlib，零第三方依赖，CI python job 同款零安装面）。

校验对象：workers/python/requirements.lock.txt（uv pip compile --generate-hashes 产物，
RT-2-ADR-VENV-GOVERNANCE.md D2）。三项判据（任务书 §5 判据 1）：
  1. 每个非注释包行（name==version）至少携带 1 个 --hash=sha256: 行；
  2. requirements.in 三直接依赖与 lock 中版本钉定一致；
  3. 无 `..` 路径穿越面。
本测试不消费 lock 安装（网络面零依赖），只做静态解析。
"""

import unittest
from pathlib import Path

WORKERS_PY = Path(__file__).resolve().parents[1]
LOCK = WORKERS_PY / "requirements.lock.txt"
IN = WORKERS_PY / "requirements.in"

DIRECT_DEPS_IN = ("numpy", "onnxruntime", "tokenizers")


def parse_lock_packages(text: str) -> dict:
    """解析 pip --require-hashes 格式 lock：包行 name==version 起始，续行含 hash。

    返回 {name: {"version": str, "hashes": int}}。
    """
    packages: dict = {}
    current = None
    for raw in text.splitlines():
        line = raw.rstrip()
        stripped = line.strip()
        if not line or line.lstrip().startswith("#"):
            continue
        if line.startswith(" ") or line.startswith("\t"):
            # 续行：--hash=... 或 "# via" 注释
            if stripped.startswith("--hash=sha256:") and current is not None:
                packages[current]["hashes"] += 1
            continue
        if "==" in stripped and not stripped.startswith("--"):
            name, _, version = stripped.partition("==")
            name = name.strip()
            # 包行可能以 " \\" 续行结尾
            version = version.strip().rstrip("\\").strip()
            current = name
            packages[name] = {"version": version, "hashes": 0}
        else:
            current = None
    return packages


def parse_in_pins(text: str) -> dict:
    """解析 requirements.in 直接依赖钉定：{name: version}，跳过注释与空行。"""
    pins: dict = {}
    for raw in text.splitlines():
        line = raw.strip()
        if not line or line.startswith("#"):
            continue
        if "==" in line:
            name, _, version = line.partition("==")
            pins[name.strip()] = version.strip()
    return pins


class TestLockIntegrity(unittest.TestCase):
    """判据 1：lock 完整性（存在性 / 哈希覆盖 / 钉定一致 / 无穿越）。"""

    def test_lock_file_exists(self):
        self.assertTrue(LOCK.is_file(), f"lock 文件缺失: {LOCK}")

    def test_in_file_exists(self):
        self.assertTrue(IN.is_file(), f"直接依赖声明缺失: {IN}")

    def test_every_package_has_at_least_one_sha256_hash(self):
        packages = parse_lock_packages(LOCK.read_text(encoding="utf-8"))
        self.assertGreater(len(packages), 0, "lock 未解析出任何包行")
        bare = [n for n, meta in packages.items() if meta["hashes"] < 1]
        self.assertEqual(
            bare, [],
            f"以下包行零 --hash=sha256: 覆盖（--require-hashes 不可消费）: {sorted(bare)}",
        )

    def test_direct_deps_pinned_versions_match_in(self):
        packages = parse_lock_packages(LOCK.read_text(encoding="utf-8"))
        pins = parse_in_pins(IN.read_text(encoding="utf-8"))
        self.assertEqual(
            sorted(pins.keys()), sorted(DIRECT_DEPS_IN),
            "requirements.in 直接依赖集漂移（须为 numpy/onnxruntime/tokenizers 三件）",
        )
        for name, version in sorted(pins.items()):
            self.assertIn(
                name, packages,
                f"直接依赖 {name} 不在 lock 传递闭包内",
            )
            self.assertEqual(
                packages[name]["version"], version,
                f"直接依赖 {name} 版本漂移: .in 钉 {version} vs lock {packages[name]['version']}",
            )

    def test_no_parent_directory_traversal(self):
        text = LOCK.read_text(encoding="utf-8")
        self.assertNotIn("..", text, "lock 内出现 .. 路径面")


if __name__ == "__main__":
    unittest.main()
