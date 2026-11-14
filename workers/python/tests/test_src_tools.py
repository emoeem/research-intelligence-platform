# -*- coding: utf-8 -*-
"""src_tools.py（Source Analysis Pack 容器 worker）宿主纯函数单测。

容器内帧协议端到端由 crates/tool-engine/tests/src_analysis_e2e.rs 承载
（V-9 digest 守卫）；本文件只测宿主可导入的纯函数面：四格式依赖解析、
规则扫描与截断、目录检索护栏、路径穿越拒绝。容器内根路径 /task/in 与
/task/out 在宿主侧 monkeypatch 到临时目录。
"""

from __future__ import annotations

import importlib.util
import json
import os
import sys
import tempfile
import unittest
from pathlib import Path

REPO = Path(__file__).resolve().parents[3]
SRC = REPO / "containerfiles" / "rip-tool-source" / "src_tools.py"


def load_module():
    spec = importlib.util.spec_from_file_location("src_tools", SRC)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


class DepsParsingTests(unittest.TestCase):
    """parse_deps：npm/pip/pyproject/cargo 四格式归一化。"""

    def test_npm_kinds_and_constraints(self):
        mod = load_module()
        text = json.dumps({
            "dependencies": {"react": "^18.0.0", "axios": "1.6.0"},
            "devDependencies": {"vitest": "^1.2.0"},
        })
        deps, notes = mod.parse_deps(text, "npm")
        self.assertEqual(len(deps), 3)
        self.assertEqual(deps[0]["name"], "react")
        self.assertEqual(deps[0]["kind"], "runtime")
        self.assertEqual(deps[2]["kind"], "dev")
        self.assertEqual(notes, [])

    def test_pip_requirements_lines(self):
        mod = load_module()
        text = "requests==2.31.0\n# comment\nflask>=3.0\n--index-url https://x\n\n"
        deps, notes = mod.parse_deps(text, "pip-requirements")
        self.assertEqual([d["name"] for d in deps], ["requests", "flask"])
        self.assertEqual(deps[0]["constraint"], "requests==2.31.0")
        self.assertEqual(deps[0]["source_line"], 1)
        self.assertEqual(notes, [])

    def test_pip_pyproject_tomllib(self):
        mod = load_module()
        text = (
            '[project]\nname="demo"\ndependencies=["requests>=2.31","numpy"]\n'
            '[project.optional-dependencies]\ndev=["pytest"]\n'
        )
        deps, notes = mod.parse_deps(text, "pip-pyproject")
        self.assertEqual(len(deps), 3)
        self.assertEqual([d["kind"] for d in deps], ["runtime", "runtime", "dev"])

    def test_cargo_sections(self):
        mod = load_module()
        text = '[dependencies]\nserde={ version = "1.0", features = ["derive"] }\n[dev-dependencies]\ncriterion="0.5"\n'
        deps, notes = mod.parse_deps(text, "cargo")
        self.assertEqual(len(deps), 2)
        self.assertEqual(deps[0]["constraint"], "1.0")
        self.assertEqual(deps[0]["kind"], "runtime")
        self.assertEqual(deps[1]["kind"], "dev")

    def test_unsupported_format_rejected(self):
        mod = load_module()
        with self.assertRaises(ValueError):
            mod.parse_deps("{}", "gemfile")

    def test_truncation_guard(self):
        mod = load_module()
        text = json.dumps({"dependencies": {f"p{i}": "1.0" for i in range(2100)}})
        deps, notes = mod.parse_deps(text, "npm")
        self.assertEqual(len(deps), mod.MAX_DEPS)
        self.assertTrue(any("truncated" in n for n in notes))


class StaticCheckTests(unittest.TestCase):
    """scan_text：规则命中、severity 汇总与截断。"""

    def test_secret_and_code_rules(self):
        mod = load_module()
        text = (
            'KEY = "AKIAIOSFODNN7EXAMPLE"\n'
            "token = 'ghp_A1b2C3d4E5f6G7h8I9j0K1l2M3n4O5p6Q7r8'\n"
            "-----BEGIN RSA PRIVATE KEY-----\n"
            "password = \"hunter2secret\"\n"
            "result = eval(x)\n"
            "subprocess.run(cmd, shell=True)\n"
            "pickle.loads(blob)\n"
            "# TODO refactor\n"
        )
        findings, todo_lines, truncated = mod.scan_text(text)
        rules = {f["rule_id"] for f in findings}
        self.assertIn("SEC001", rules)
        self.assertIn("SEC002", rules)
        self.assertIn("SEC003", rules)
        self.assertIn("SEC004", rules)
        self.assertIn("CODE001", rules)
        self.assertIn("CODE002", rules)
        self.assertIn("CODE003", rules)
        self.assertEqual(todo_lines, 1)
        self.assertFalse(truncated)
        # severity 汇总口径：SEC001/CODE002/SEC003 为 high
        highs = [f for f in findings if f["severity"] == "high"]
        self.assertGreaterEqual(len(highs), 3)

    def test_excerpt_truncation_and_line_numbers(self):
        mod = load_module()
        text = "\n\nAWS = 'AKIAIOSFODNN7EXAMPLE'\n"
        findings, _, _ = mod.scan_text(text)
        self.assertEqual(findings[0]["line"], 3)

    def test_benign_text_no_findings(self):
        mod = load_module()
        findings, todo_lines, truncated = mod.scan_text("def add(a, b):\n    return a + b\n")
        self.assertEqual(findings, [])
        self.assertEqual(todo_lines, 0)
        self.assertFalse(truncated)


class SearchTreeTests(unittest.TestCase):
    """search_tree：include/exclude、截断与非法 regex。"""

    def setUp(self):
        self.tmp = tempfile.mkdtemp(prefix="src-tools-search-")
        (Path(self.tmp) / "a.py").write_text("alpha one\nbeta two\n", encoding="utf-8")
        (Path(self.tmp) / "b.py").write_text("alpha three\n", encoding="utf-8")
        (Path(self.tmp) / "c.md").write_text("alpha four\n", encoding="utf-8")

    def test_include_glob(self):
        mod = load_module()
        result = mod.search_tree(self.tmp, "alpha", include=["*.py"])
        self.assertEqual(result["count"], 2)
        self.assertEqual({m["file"] for m in result["matches"]}, {"a.py", "b.py"})

    def test_exclude_glob(self):
        mod = load_module()
        result = mod.search_tree(self.tmp, "alpha", exclude=["b.py"])
        self.assertEqual(result["count"], 2)

    def test_max_matches_truncates(self):
        mod = load_module()
        result = mod.search_tree(self.tmp, "alpha", max_matches=2)
        self.assertTrue(result["truncated"])
        self.assertEqual(result["count"], 2)

    def test_case_insensitive(self):
        mod = load_module()
        (Path(self.tmp) / "d.py").write_text("ALPHA upper\n", encoding="utf-8")
        plain = mod.search_tree(self.tmp, "alpha")
        lowered = mod.search_tree(self.tmp, "alpha", case_insensitive=True)
        self.assertEqual(len(lowered["matches"]), len(plain["matches"]) + 1)

    def test_invalid_regex_value_error(self):
        mod = load_module()
        with self.assertRaises(ValueError):
            mod.search_tree(self.tmp, "([unclosed")


class PathGuardTests(unittest.TestCase):
    """_in_path/_out_name：穿越拒绝与 basename 收敛（monkeypatch 根路径）。"""

    def setUp(self):
        self.mod = load_module()
        self.tmp = tempfile.mkdtemp(prefix="src-tools-root-")
        self.orig_in, self.orig_out = self.mod.IN_ROOT, self.mod.OUT_ROOT
        self.mod.IN_ROOT = self.tmp
        self.mod.OUT_ROOT = self.tmp
        (Path(self.tmp) / "ok.txt").write_text("x", encoding="utf-8")

    def tearDown(self):
        self.mod.IN_ROOT, self.mod.OUT_ROOT = self.orig_in, self.orig_out

    def test_inside_root_accepted(self):
        self.assertEqual(os.path.basename(self.mod._in_path(str(Path(self.tmp) / "ok.txt"))), "ok.txt")

    def test_traversal_rejected(self):
        with self.assertRaises(ValueError):
            self.mod._in_path(os.path.join(self.tmp, "..", "etc", "passwd"))

    def test_missing_file_rejected(self):
        with self.assertRaises(FileNotFoundError):
            self.mod._in_path(str(Path(self.tmp) / "absent.txt"))

    def test_out_name_takes_basename(self):
        out = self.mod._out_name("../evil/deps.json", "deps.json")
        self.assertEqual(os.path.dirname(out), self.mod.OUT_ROOT)
        self.assertEqual(os.path.basename(out), "deps.json")

    def test_dotdot_name_rejected(self):
        with self.assertRaises(ValueError):
            self.mod._out_name("..", "fallback.json")


class ReadGuardTests(unittest.TestCase):
    """_read_text：超限与二进制按 ValueError 拒绝（扫描批量跳过口径）。"""

    def setUp(self):
        self.mod = load_module()
        self.tmp = tempfile.mkdtemp(prefix="src-tools-read-")

    def test_oversized_rejected(self):
        big = Path(self.tmp) / "big.txt"
        big.write_bytes(b"x" * (self.mod.MAX_FILE_BYTES + 1))
        with self.assertRaises(ValueError):
            self.mod._read_text(str(big))

    def test_binary_rejected(self):
        blob = Path(self.tmp) / "blob.bin"
        blob.write_bytes(b"\x00\x01\x02\xff\xfe")
        with self.assertRaises(ValueError):
            self.mod._read_text(str(blob))


if __name__ == "__main__":
    unittest.main()
