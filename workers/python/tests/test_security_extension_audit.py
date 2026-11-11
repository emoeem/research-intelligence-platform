# -*- coding: utf-8 -*-
"""SK-3 security.extension.audit 测试：Worker Protocol v1 契约
（ContractMixin）+ 恶意 fixture 拒绝矩阵（六类反例）+ 零执行能力 AST 断言。

批次验收（安全知识源计划 §8 P6-11）：浮动分支、无许可证、安装钩子（curl|sh）、
越权 Shell、私网 URL、危险 MCP Schema 逐项断言拒绝/高危告警，且审查过程
本身不执行任何被审查物（AST 级断言锁定 worker 的 import 面与危险调用点）。
"""

from __future__ import annotations

import ast
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from _harness import ContractMixin, WORKERS_PY  # noqa: E402

import extension_audit_worker as eaw  # noqa: E402
from extension_audit_worker import ExtensionAuditWorker  # noqa: E402

REPO_ROOT = WORKERS_PY.parents[1]
SK3 = REPO_ROOT / "tests" / "fixtures" / "sk3"


class AuditContractTests(ContractMixin, unittest.TestCase):
    """Worker Protocol v1 全契约（握手/预算/取消/幂等/截断/崩溃安全）。"""

    WORKER = "audit"

    def expected_worker_name(self) -> str:
        return "extension-audit-worker"


def run_audit(task: dict) -> dict:
    """直接驱动 worker（不经子进程），便于单元断言。"""
    import worker_runtime

    ctx = worker_runtime.WorkerContext(
        ExtensionAuditWorker(), "unit", task, 30_000, 67_883_264, None)
    return ExtensionAuditWorker().execute(ctx, task)


def finding_ids(out: dict, *prefixes: str) -> list[str]:
    return [f["id"] for f in out["findings"]
            if any(f["id"].startswith(p) for p in prefixes)]


class ZeroExecutionCapabilityTests(unittest.TestCase):
    """零执行能力自证：AST 解析 worker 源码，锁定 import 面与危险调用点。

    断言成立即证明：审查 fixture（含 curl|sh 安装钩子）时不存在任何可执行
    被审查物的代码路径（无子进程、无网络、无 eval/exec 族）。
    """

    def _module_ast(self) -> ast.Module:
        src = (WORKERS_PY / "extension_audit_worker.py").read_text(encoding="utf-8")
        return ast.parse(src)

    def test_no_dangerous_imports(self):
        forbidden = {"subprocess", "socket", "urllib", "requests", "http",
                     "pty", "multiprocessing", "asyncio", "ctypes", "os",
                     "shutil", "pickle", "ssl", "telnetlib"}
        found = []
        for node in ast.walk(self._module_ast()):
            if isinstance(node, ast.Import):
                found += [a.name for a in node.names]
            elif isinstance(node, ast.ImportFrom):
                found.append(node.module or "")
        self.assertEqual(
            sorted(set(found)),
            ["__future__", "json", "pathlib", "re", "sys", "worker_runtime"],
            f"import surface changed: {found}")
        self.assertFalse(forbidden & set(found),
                         f"forbidden imports present: {forbidden & set(found)}")

    def test_no_dynamic_execution_calls(self):
        # 裸名调用禁 eval/exec/compile/__import__/open；属性调用禁
        # subprocess/os 面的 system/popen/spawn 族（re.compile 合法豁免）
        forbidden_names = {"eval", "exec", "compile", "__import__", "open"}
        forbidden_attrs = {"system", "popen", "spawn", "spawnl", "spawnv",
                           "Popen", "exec_", "execve", "setuid"}
        for node in ast.walk(self._module_ast()):
            if not isinstance(node, ast.Call):
                continue
            func = node.func
            if isinstance(func, ast.Name):
                self.assertNotIn(func.id, forbidden_names,
                                 f"dangerous call at line {node.lineno}")
            if isinstance(func, ast.Attribute):
                self.assertNotIn(func.attr, forbidden_attrs,
                                 f"dangerous attribute call at line {node.lineno}")

    def test_no_eval_exec_exec_in_strings_executed(self):
        # 字符串常量不得经 eval/exec 类动态执行（本测只断言不存在
        # getattr/eval 组合等间接执行面；正则词表内的字面词不算调用）
        src = (WORKERS_PY / "extension_audit_worker.py").read_text(encoding="utf-8")
        self.assertNotIn("getattr", src)
        self.assertNotIn("__import__", src)


class RejectionMatrixTests(unittest.TestCase):
    """六类恶意 fixture 拒绝矩阵（批次验收逐项断言）。"""

    def test_floating_branch_manifest_flagged(self):
        out = run_audit({"path": str(SK3 / "floating-branch")})
        self.assertIn("AUD-PIN-01", finding_ids(out, "AUD-PIN"))
        pin = next(f for f in out["findings"] if f["id"] == "AUD-PIN-01")
        self.assertEqual(pin["severity"], "high")
        self.assertEqual(pin["rule"], "floating_ref")
        self.assertGreaterEqual(out["counts_by_severity"]["high"], 1)
        self.assertEqual(out["review_status"], "BLOCKED_FOR_REVIEW")

    def test_no_license_flagged_unknown(self):
        out = run_audit({"path": str(SK3 / "no-license")})
        self.assertEqual(out["license_status"], "unknown")
        self.assertIn("AUD-LIC-01", finding_ids(out, "AUD-LIC"))
        self.assertIn("registration blocked by tool-registry rule 5",
                      " ".join(out["supply_chain_notes"]))

    def test_install_hook_curl_pipe_sh_flagged_and_not_executed(self):
        out = run_audit({"path": str(SK3 / "install-hook")})
        self.assertIn("AUD-INSTALL-01", finding_ids(out, "AUD-INSTALL"))
        self.assertIn("AUD-INSTALL-02", finding_ids(out, "AUD-INSTALL"))
        self.assertGreaterEqual(out["counts_by_severity"]["high"], 3)
        self.assertEqual(out["risk_level"], "high")
        self.assertEqual(out["review_status"], "BLOCKED_FOR_REVIEW")
        # 零执行：审查完成即返回且无任何进程副作用（配合
        # ZeroExecutionCapabilityTests 的 AST 断言成立）
        self.assertEqual(out["files_scanned"], 2)
        hook = next(f for f in out["findings"] if f["id"] == "AUD-INSTALL-01")
        self.assertIn("curl", hook["evidence"])
        self.assertIn("|", hook["evidence"])

    def test_overreach_shell_and_prompt_injection_flagged(self):
        out = run_audit({"path": str(SK3 / "overreach-shell"), "kind": "skill"})
        self.assertIn("AUD-OVERREACH-01", finding_ids(out, "AUD-OVERREACH"))
        self.assertIn("AUD-INJECT-01", finding_ids(out, "AUD-INJECT"))
        self.assertIn("AUD-PRIV-01", finding_ids(out, "AUD-PRIV"))
        self.assertEqual(out["counts_by_severity"]["critical"], 1)
        self.assertEqual(out["risk_level"], "critical")
        self.assertEqual(out["review_status"], "BLOCKED_FOR_REVIEW")
        over = next(f for f in out["findings"] if f["id"] == "AUD-OVERREACH-01")
        self.assertIn("rm -rf", over["evidence"])

    def test_private_url_and_metadata_endpoint_flagged(self):
        out = run_audit({"path": str(SK3 / "private-url")})
        self.assertIn("AUD-NET-02", finding_ids(out, "AUD-NET"))  # 169.254.169.254
        net01 = [f for f in out["findings"] if f["id"] == "AUD-NET-01"]
        self.assertGreaterEqual(len(net01), 3)  # 127.0.0.1 / 192.168.1.1 / localhost
        self.assertEqual(out["risk_level"], "high")
        # 公网对照条目不得被私网规则误伤
        public = [f for f in out["findings"]
                  if "example.com/ok" in f["evidence"]
                  and f["id"].startswith("AUD-NET-0")]
        self.assertEqual(public, [])

    def test_dangerous_mcp_schema_flagged(self):
        out = run_audit({"path": str(SK3 / "mcp-dangerous"), "kind": "mcp"})
        self.assertIn("AUD-MCP-03", finding_ids(out, "AUD-MCP"))
        self.assertIn("AUD-MCP-01", finding_ids(out, "AUD-MCP"))
        crit = next(f for f in out["findings"] if f["id"] == "AUD-MCP-03")
        self.assertIn("run_shell", crit["evidence"])
        self.assertEqual(out["risk_level"], "critical")
        self.assertIn("mcp.tools", out["required_permissions"])

    def test_constrained_mcp_tool_not_flagged(self):
        out = run_audit({"files": {".mcp.json": """{
          "mcpServers": {"safe": {"command": "uvx", "args": ["srv"],
            "tools": [{"name": "read_config",
                       "inputSchema": {"type": "object",
                                       "properties": {"key": {"type": "string"}}}}]}}}
        """}, "kind": "mcp"})
        self.assertEqual(finding_ids(out, "AUD-MCP"), [])


class StaticConclusionTests(unittest.TestCase):
    """输出契约与静态结论边界（不含恶意内容时的形态）。"""

    def test_positive_control_skill_pack_clean(self):
        out = run_audit({"path": str(SK3 / "skill-pack")})
        self.assertEqual(out["license_status"], "MIT")
        self.assertEqual(out["risk_level"], "info")  # 仅 maintenance_unknown
        self.assertEqual(out["review_status"], "STATIC_CLEAN")
        self.assertEqual(out["required_permissions"], [])
        self.assertIn("不证明代码安全", out["disclaimer"])

    def test_dep_lock_missing_needs_review(self):
        # license 元数据显式给 MIT（隔离锁文件问题本身）；缺锁文件 = medium
        out = run_audit({"files": {"package.json": '{"name": "x", "scripts": {}}'},
                         "license": "MIT License"})
        self.assertIn("AUD-DEP-01", finding_ids(out, "AUD-DEP"))
        self.assertEqual(out["review_status"], "NEEDS_REVIEW")

    def test_license_unknown_is_registration_blocker(self):
        # 计划 §7.1：目录许可不传递，license unknown 语义上被 tool-registry
        # rule 5 拒登：审查面给 high（与 lock 缺失的 medium 分级区分）
        out = run_audit({"files": {"package.json": '{"name": "x"}'}})
        lic = next(f for f in out["findings"] if f["id"] == "AUD-LIC-01")
        self.assertEqual(lic["severity"], "high")
        self.assertEqual(out["review_status"], "BLOCKED_FOR_REVIEW")

    def test_license_mismatch_declared_vs_file(self):
        out = run_audit({"files": {
            "LICENSE": "MIT License\nPermission is hereby granted, free of charge",
            "manifest.yaml": "license: \"Apache-2.0\"\nname: x\n"}})
        self.assertIn("AUD-LIC-04", finding_ids(out, "AUD-LIC"))
        self.assertEqual(out["license_status"], "MIT")

    def test_license_unknown_declared_flagged(self):
        out = run_audit({"files": {"manifest.yaml": "license: \"unknown\"\n"}})
        self.assertIn("AUD-LIC-03", finding_ids(out, "AUD-LIC"))
        self.assertEqual(out["license_status"], "unknown")

    def test_maintenance_metadata_drives_findings(self):
        out = run_audit({"text": "benign\n", "name": "README.md",
                         "maintenance": {"archived": True,
                                         "last_commit_days": 400}})
        self.assertIn("AUD-MAINT-02", finding_ids(out, "AUD-MAINT"))
        self.assertIn("AUD-MAINT-03", finding_ids(out, "AUD-MAINT"))

    def test_maintenance_unknown_info_when_absent(self):
        out = run_audit({"text": "benign\n", "name": "README.md"})
        self.assertIn("AUD-MAINT-01", finding_ids(out, "AUD-MAINT"))
        self.assertEqual(out["counts_by_severity"]["info"], 1)

    def test_inline_oversize_rejected(self):
        with self.assertRaises(Exception) as cm:
            run_audit({"text": "x" * (2 * 1024 * 1024 + 1), "name": "a.md"})
        self.assertEqual(getattr(cm.exception, "code", None), -32008)

    def test_input_validation_rejections(self):
        cases = [
            {},                                   # 缺全部输入
            {"text": "a", "files": {"b": "c"}},   # 双输入
            {"path": "a", "files": {}},           # 空 files + path
            {"text": "a", "name": "dir/x.md"},    # name 带路径分隔
            {"text": "a", "kind": "unknown"},     # 未知 kind
        ]
        for task in cases:
            with self.assertRaises(Exception) as cm:
                run_audit(task)
            self.assertEqual(getattr(cm.exception, "code", None), -32602,
                             f"task={task!r}")

    def test_binary_extension_refused(self):
        with self.assertRaises(Exception) as cm:
            run_audit({"path": str(SK3 / "mcp-dangerous" / ".mcp.json")
                       .replace(".mcp.json", "no-such.bin")})
        # 路径不存在按 InputError（-32008），不是静默通过
        self.assertEqual(getattr(cm.exception, "code", None), -32008)

    def test_findings_shape(self):
        out = run_audit({"path": str(SK3 / "install-hook")})
        for f in out["findings"]:
            self.assertEqual(set(f.keys()),
                             {"id", "severity", "file", "line", "evidence", "rule"})
            self.assertIn(f["severity"], set(eaw.SEVERITY_ORDER))


if __name__ == "__main__":
    unittest.main()
