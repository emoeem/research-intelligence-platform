#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""Extension Audit Worker：Skill/Agent/MCP/Tool Manifest/仓库静态审查
（SK-3，P6-11 第 2 步，rip.security.extension.audit）。

T0 纪律（安全知识源计划 §6.2B/§7 第 3 条）：纯静态，不执行任何被审查物、
不访问网络、不开子进程；README/Markdown/SKILL.md/脚本/配置全部按不可信
文本扫描；只给静态结论，不声称证明代码安全（disclaimer 固定输出）。

零执行能力自证：本模块只 import json/re/sys/pathlib/worker_runtime，
无 subprocess/socket/urllib/requests/eval/exec（tests/test_security_
extension_audit.py 有 AST 级断言锁定 import 面与危险调用点）。

检查面（findings[]，每项 id/severity/file/line/evidence/rule）：
  许可与版权声明、版本/Commit pin、依赖锁文件、安装脚本、命令执行入口、
  网络与文件系统能力、Secrets 读取、容器边界、权限升级、MCP tools 输入输出
  Schema、Prompt Injection/越权指令迹象、维护状态。

输入 execute.task（三选一）：
  {"path": "/abs/target"}                          目录或单文件
  {"files": {"name": "content", ...}}              内联多文件
  {"text": "...", "name": "SKILL.md"}              内联单文件
  可选 kind: "repo"|"skill"|"mcp"|"tool_manifest"（缺省 repo）
  可选 license: "MIT" 等显式元数据（无 LICENSE 文件时的声明来源）
  可选 maintenance: {"archived": bool, "last_commit_days": int}
输出 data：
  target/kind/files_scanned/bytes_scanned/skipped_files/findings[]/
  counts_by_severity/risk_level/review_status/required_permissions[]/
  license_status/supply_chain_notes[]/disclaimer
"""

from __future__ import annotations

import json
import re
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent / "runtime"))

from worker_runtime import WorkerRuntime, WorkerContext, InputError, InvalidParams  # noqa: E402

MAX_FILES = 5000
MAX_TOTAL_BYTES = 8 * 1024 * 1024
MAX_FILE_BYTES = 2 * 1024 * 1024
SCAN_EXTENSIONS = {
    ".md", ".markdown", ".txt", ".json", ".yaml", ".yml", ".toml",
    ".cfg", ".ini", ".py", ".js", ".ts", ".mjs", ".cjs",
    ".sh", ".bash", ".ps1", ".psm1", ".rb", ".pl",
}
LICENSE_NAMES = {"license", "licence", "copying", "copyright", "notice"}
SEVERITY_ORDER = ["none", "info", "low", "medium", "high", "critical"]
FINDINGS_CAP = 2000
DISCLAIMER = ("静态结论：只覆盖可见文本的迹象检查，不证明代码安全，"
              "不替代人工审查（计划 §6.2B）")

# ---------------------------------------------------------------- 规则词表

EXEC_SIGNS = [
    (r"\bchild_process\s*\.\s*(exec|spawn|execSync|spawnSync)\b", "child_process"),
    (r"\beval\s*\(", "eval_call"),
    (r"new\s+Function\s*\(", "js_dynamic_function"),
    (r"\bexec\s*\(\s*f?[\"']", "python_exec"),
    (r"\bos\s*\.\s*(system|popen|exec\w*)\s*\(", "os_process"),
    (r"\bsubprocess\s*\.\s*\w+\s*\(", "subprocess_py"),
    (r"\b(Invoke-Expression|iex\s+\()", "powershell_iex"),
    (r"\bosascript\b", "osascript"),
    (r"\bStart-Process\b", "powershell_exec"),
    (r"\|\s*/?(usr/bin/)?\{?\s*(ba|z|da|k)?sh\b", "pipe_to_shell"),
]
INSTALL_HOOK_NETWORK = re.compile(
    r"\b(curl|wget|npm\s+(install|i)|pip\s+install|git\s+clone|"
    r"npx|pnpm\s+(add|install)|axel)\b", re.I)
REMOTE_PIPE_SHELL = re.compile(
    r"\b(curl|wget)\b[^#\n]*\|\s*(sudo\s+)?(ba|z|da|k)?sh\b", re.I)
PRIVATE_URL = re.compile(
    r"https?://("
    r"localhost|127\.\d+\.\d+\.\d+|0\.0\.0\.0|\[?::1\]?|"
    r"10\.\d+\.\d+\.\d+|172\.(1[6-9]|2\d|3[01])\.\d+\.\d+|192\.168\.\d+\.\d+|"
    r"169\.254\.\d+\.\d+|100\.(6[4-9]|[7-9]\d|1[01]\d|12[0-7])\.\d+\.\d+"
    r")([:/?]|$)", re.I)
METADATA_ENDPOINT = re.compile(r"https?://169\.254\.169\.254", re.I)
SECRET_SIGNS = [
    (r"process\.env\s*\.\s*[A-Za-z_]*(KEY|TOKEN|SECRET|PASSWORD|CREDENTIAL)", "env_secret"),
    (r"os\.environ(\.get)?\s*\(\s*[\"'][^\"']*(KEY|TOKEN|SECRET|PASSWORD|CREDENTIAL)", "env_secret"),
    (r"\.ssh\b|id_rsa|id_ed25519", "ssh_material"),
    (r"AWS_ACCESS_KEY|AWS_SECRET|GITHUB_TOKEN|GH_TOKEN|SLACK_TOKEN|OPENAI_API_KEY|ANTHROPIC_API_KEY", "named_secret"),
    (r"\bkeytar\b", "secret_store"),
    (r"credentials(\.json|\.db)?\b", "credentials_file"),
]
CONTAINER_SIGNS = [
    (r"--privileged\b", "privileged_container"),
    (r"/var/run/docker\.sock|/run/docker\.sock", "docker_socket"),
    (r"docker\s+run\b|podman\s+run\b", "container_runtime"),
    (r"nsenter|unshare\s+--mount|--pid=host|--net=host", "host_namespace"),
]
PRIVILEGE_SIGNS = [
    (r"\bsudo\b", "sudo"),
    (r"chmod\s+(-R\s+)?777\b", "chmod_world_writable"),
    (r"setuid\s*\(|seteuid\s*\(|os\.setuid", "setuid"),
    (r"\bchown\s+root\b|\buseradd\b", "account_change"),
    (r"/etc/sudoers|/etc/shadow", "system_auth_files"),
]
OVERREACH_SIGNS = [
    (r"rm\s+-rf\s+/(?:\s|$)", "rm_rf_root"),
    (r"sudo\s+rm\s+-rf", "sudo_rm_rf"),
    (r"\bdd\s+if=.*of=/dev/(sd|hd|nvme|disk)", "disk_write"),
    (r"mkfs\.\w+", "mkfs"),
    (r">\s*/etc/(passwd|sudoers|crontab)", "overwrite_system_file"),
]
INJECTION_SIGNS = [
    (r"ignore\s+(all\s+|any\s+)?(previous|prior|above)\s+instructions", "ignore_instructions"),
    (r"disregard\s+(all\s+)?(previous|prior|above)", "ignore_instructions"),
    (r"you\s+must\s+now\s+(obey|follow|execute)", "instruction_override"),
    (r"exfiltrat\w+", "exfiltration"),
    (r"send\s+(me\s+)?(your|the)\s+(api\s+)?key|post\s+.*api[_-]?key\s+to", "key_harvest"),
    (r"disable\s+(all\s+)?(security|guardrails|filters|safety)", "security_bypass"),
    (r"reveal\s+your\s+(system\s+)?(prompt|instructions)", "prompt_extraction"),
]
MCP_SCHEMA_UNCONSTRAINED_NAME = re.compile(
    r"(shell|exec|command|cmd|run_cmd|terminal|eval|code_exec|interpreter)", re.I)
LOCKFILES = {
    "node": ["package-lock.json", "pnpm-lock.yaml", "yarn.lock", "bun.lockb"],
}
MANIFEST_NAMES = {"plugin.lock", "manifest.yaml", "manifest.json", "plugin.json",
                  "tool.json", "tool.yaml", ".mcp.json", "mcp.json"}
SPDX_PATTERNS = [
    (r"Apache License\s+Version 2", "Apache-2.0"),
    (r"MIT License|Permission is hereby granted, free of charge", "MIT"),
    (r"GNU GENERAL PUBLIC LICENSE", "GPL"),
    (r"GNU LESSER GENERAL PUBLIC LICENSE", "LGPL"),
    (r"BSD \d-Clause|Redistribution and use in source and binary forms", "BSD"),
    (r"Creative Commons (Legal Code )?CC0|CC0 1\.0 Universal|public domain", "CC0-1.0"),
    (r"Mozilla Public License", "MPL"),
    (r"ISC License", "ISC"),
]


def _severity(a: str, b: str) -> str:
    return a if SEVERITY_ORDER.index(a) >= SEVERITY_ORDER.index(b) else b


def _add(findings: list, fid: str, severity: str, file: str, line: int,
         evidence: str, rule: str) -> None:
    if len(findings) >= FINDINGS_CAP:
        return  # findings 硬上限（防御性），风险等级由已有发现反映
    findings.append({
        "id": fid,
        "severity": severity,
        "file": file,
        "line": line,
        "evidence": evidence[:200],
        "rule": rule,
    })


def _detect_license(license_text: str) -> str:
    for pattern, spdx in SPDX_PATTERNS:
        if re.search(pattern, license_text, re.I):
            return spdx
    return "present_unknown"


def _licenses_agree(declared: str, detected: str) -> bool:
    d, l = declared.lower(), detected.lower()
    return l in d or d in l


def _scan_text(rel: str, text: str, findings: list, state: dict,
               seen_info: set) -> None:
    lines = text.splitlines()
    for i, line in enumerate(lines, start=1):
        # 1) 远程管道到 shell（安装钩子类）
        if REMOTE_PIPE_SHELL.search(line):
            _add(findings, "AUD-INSTALL-01", "high", rel, i,
                 line.strip(), "remote_pipe_to_shell")
        if METADATA_ENDPOINT.search(line):
            _add(findings, "AUD-NET-02", "high", rel, i,
                 line.strip(), "cloud_metadata_endpoint")
        elif PRIVATE_URL.search(line):
            _add(findings, "AUD-NET-01", "high", rel, i,
                 line.strip(), "private_network_url")
        # 2) 命令执行入口
        for pattern, tag in EXEC_SIGNS:
            if re.search(pattern, line, re.I):
                in_install = "install.sh" in rel.lower() or "bootstrap" in rel.lower()
                sev = "high" if in_install else "medium"
                _add(findings, "AUD-EXEC-01", sev, rel, i,
                     line.strip(), f"code_exec:{tag}")
                state["required_permissions"].add("process.exec")
                break
        # 3) Secrets 读取
        for pattern, tag in SECRET_SIGNS:
            if re.search(pattern, line, re.I):
                _add(findings, "AUD-SECRET-01", "high", rel, i,
                     line.strip(), f"secret_access:{tag}")
                state["required_permissions"].add("secrets.read")
                break
        # 4) 容器边界
        for pattern, tag in CONTAINER_SIGNS:
            if re.search(pattern, line, re.I):
                sev = "high" if tag in ("privileged_container", "docker_socket",
                                        "host_namespace") else "medium"
                _add(findings, "AUD-CONTAINER-01", sev, rel, i,
                     line.strip(), f"container_boundary:{tag}")
                state["required_permissions"].add("container.host")
                break
        # 5) 权限升级
        for pattern, tag in PRIVILEGE_SIGNS:
            if re.search(pattern, line, re.I):
                _add(findings, "AUD-PRIV-01", "high", rel, i,
                     line.strip(), f"privilege_escalation:{tag}")
                state["required_permissions"].add("privilege.escalation")
                break
        # 6) 越权破坏性命令
        for pattern, tag in OVERREACH_SIGNS:
            if re.search(pattern, line, re.I):
                _add(findings, "AUD-OVERREACH-01", "critical", rel, i,
                     line.strip(), f"overreach_command:{tag}")
                break
        # 7) Prompt Injection / 越权指令迹象
        for pattern, tag in INJECTION_SIGNS:
            if re.search(pattern, line, re.I):
                _add(findings, "AUD-INJECT-01", "high", rel, i,
                     line.strip(), f"prompt_injection:{tag}")
                break
        # 8) 网络能力（info 级，进 required_permissions；每文件只记一条）
        if re.search(r"\b(requests\.\w+|urllib|urlopen|socket\.\w+|httpx|"
                     r"fetch\(|axios|XMLHttpRequest|curl\s+-|wget\s+)\b", line, re.I):
            state["required_permissions"].add("network.egress")
            if ("net", rel) not in seen_info:
                seen_info.add(("net", rel))
                _add(findings, "AUD-NET-03", "info", rel, i,
                     line.strip(), "network_capability")
        # 9) 文件系统能力（info 级）
        if re.search(r"\b(writeFile|writeFileSync|os\.remove|shutil\.rmtree|"
                     r"rm\s+-rf\b|unlink\s*\()", line, re.I):
            state["required_permissions"].add("filesystem.write")
            if ("fs", rel) not in seen_info:
                seen_info.add(("fs", rel))
                _add(findings, "AUD-FS-01", "info", rel, i,
                     line.strip(), "filesystem_write_capability")

    # 10) MCP tools Schema（.mcp.json / mcp.json / mcpServers 块）
    if rel.lower().endswith(".json") and \
            ("mcp" in rel.lower() or '"mcpServers"' in text):
        _audit_mcp_schema(rel, text, findings, state)

    # 11) package.json 安装钩子
    if rel.lower().endswith("package.json"):
        try:
            pkg = json.loads(text)
        except ValueError:
            _add(findings, "AUD-META-01", "low", rel, 1,
                 "package.json unparseable", "manifest_unparseable")
        else:
            scripts = pkg.get("scripts") or {}
            if isinstance(scripts, dict):
                for hook in ("preinstall", "install", "postinstall", "prepare"):
                    cmd = scripts.get(hook)
                    if cmd and INSTALL_HOOK_NETWORK.search(str(cmd)):
                        _add(findings, "AUD-INSTALL-02", "high", rel, 1,
                             f"{hook}: {str(cmd).strip()}", "install_hook_network")
                        state["required_permissions"].add("process.exec")

    # 12) Manifest pin 形态（manifest/lock 类）
    base = rel.rsplit("/", 1)[-1].lower()
    if base in MANIFEST_NAMES and ("pinned" in text):
        if re.search(r"kind[\"']?\s*:\s*[\"'](branch|head|latest|main|master)[\"']",
                     text, re.I) or '"kind": "branch"' in text:
            _add(findings, "AUD-PIN-01", "high", rel, 1,
                 "pinned.kind is a floating branch/head reference", "floating_ref")
        else:
            m = re.search(r"value[\"']?\s*:\s*[\"']([0-9a-f]{8,64})[\"']", text, re.I)
            if m and len(m.group(1)) != 40:
                _add(findings, "AUD-PIN-02", "medium", rel, 1,
                     f"pinned.value length {len(m.group(1))} != 40 (not a full commit)",
                     "short_pin")


def _audit_mcp_schema(rel: str, text: str, findings: list, state: dict) -> None:
    """MCP tools 输入输出 Schema 审查：缺 inputSchema，或执行类 tool 名
    （shell/exec/command/eval）配无约束 Schema = 危险 Schema。"""
    try:
        doc = json.loads(text)
    except ValueError:
        _add(findings, "AUD-MCP-02", "low", rel, 1,
             "MCP config unparseable", "mcp_config_unparseable")
        return

    def walk(node):
        if isinstance(node, dict):
            tools = node.get("tools")
            if isinstance(tools, list):
                state["required_permissions"].add("mcp.tools")
                for tool in tools:
                    if not isinstance(tool, dict):
                        continue
                    name = str(tool.get("name", ""))
                    schema = tool.get("inputSchema") or tool.get("input_schema")
                    if schema is None:
                        _add(findings, "AUD-MCP-01", "high", rel, 1,
                             f"tool {name!r}: inputSchema missing",
                             "mcp_schema_missing")
                        continue
                    constrained = isinstance(schema, dict) and bool(
                        schema.get("properties") or schema.get("enum")
                        or schema.get("pattern")
                        or schema.get("additionalProperties") is False)
                    if MCP_SCHEMA_UNCONSTRAINED_NAME.search(name) and not constrained:
                        _add(findings, "AUD-MCP-03", "critical", rel, 1,
                             f"tool {name!r}: execution-shaped name with "
                             "unconstrained inputSchema", "dangerous_mcp_schema")
            for v in node.values():
                walk(v)
        elif isinstance(node, list):
            for v in node:
                walk(v)
    walk(doc)


class ExtensionAuditWorker(WorkerRuntime):
    NAME = "extension-audit-worker"
    VERSION = "0.1.0"
    CAPABILITIES = ["security.extension.audit.static"]

    def execute(self, ctx: WorkerContext, task: dict) -> dict:
        if "op" in task:  # op 前缀保留给运行时（debug sleep/boom 等）
            return super().execute(ctx, task)
        kind = task.get("kind", "repo")
        if kind not in ("repo", "skill", "mcp", "tool_manifest"):
            raise InvalidParams("task.kind must be repo|skill|mcp|tool_manifest")
        maintenance = task.get("maintenance")
        if maintenance is not None and not isinstance(maintenance, dict):
            raise InvalidParams("task.maintenance must be an object")
        inline_license = task.get("license")
        if inline_license is not None and not isinstance(inline_license, str):
            raise InvalidParams("task.license must be a string")

        findings: list[dict] = []
        state: dict = {"required_permissions": set(), "supply_chain_notes": []}
        seen_info: set = set()
        files: list[tuple[str, str]] = []  # (rel_name, content)
        target, files_scanned, bytes_scanned, skipped_files = \
            self._collect(task, files)
        ctx.check_cancelled()

        # 许可与版权声明
        file_names = {name for name, _ in files}
        license_status = "unknown"
        license_file = self._license_file(files)
        if license_file is not None:
            license_status = _detect_license(license_file[1])
        elif inline_license:
            license_status = _detect_license(inline_license)
        manifest_doc = self._manifest_text(files)
        if manifest_doc is not None:
            _scan_manifest_license(manifest_doc[0], manifest_doc[1], findings,
                                   license_status)
        if license_status == "unknown":
            _add(findings, "AUD-LIC-01", "high", "(bundle)", 1,
                 "no LICENSE/COPYING file and no license metadata",
                 "license_missing")
            state["supply_chain_notes"].append(
                "license unknown: registration blocked by tool-registry rule 5 semantics")
        elif license_status in ("present_unknown", "GPL"):
            _add(findings, "AUD-LIC-02", "medium", "(bundle)", 1,
                 f"license detected as {license_status}", "license_unclear")

        # 依赖锁文件
        if "package.json" in file_names \
                and not (file_names & set(LOCKFILES["node"])):
            _add(findings, "AUD-DEP-01", "medium", "(bundle)", 1,
                 "package.json without package-lock.json/pnpm-lock.yaml/yarn.lock",
                 "dep_lock_missing")
            state["supply_chain_notes"].append("node dependency lockfile missing")

        for rel, text in files:
            _scan_text(rel, text, findings, state, seen_info)
            ctx.check_cancelled()

        # 维护状态（只来自调用方显式传入的元数据，不联网核对）
        supply_notes = state["supply_chain_notes"]
        if maintenance is None:
            _add(findings, "AUD-MAINT-01", "info", "(bundle)", 1,
                 "maintenance metadata not provided (static audit cannot verify)",
                 "maintenance_unknown")
        else:
            if maintenance.get("archived"):
                _add(findings, "AUD-MAINT-02", "medium", "(bundle)", 1,
                     "upstream marked archived", "maintenance_archived")
                supply_notes.append("upstream archived")
            days = maintenance.get("last_commit_days")
            if isinstance(days, int) and not isinstance(days, bool) and days > 365:
                _add(findings, "AUD-MAINT-03", "medium", "(bundle)", 1,
                     f"last commit {days} days ago", "maintenance_stale")
                supply_notes.append(f"last commit {days} days ago")
        if state["required_permissions"]:
            supply_notes.append(
                "declared-by-evidence permissions: "
                + ",".join(sorted(state["required_permissions"])))

        counts = {sev: 0 for sev in SEVERITY_ORDER}
        for f in findings:
            counts[f["severity"]] += 1
        risk_level = "none"
        for sev in SEVERITY_ORDER:
            if counts[sev]:
                risk_level = sev
        if risk_level in ("high", "critical"):
            review_status = "BLOCKED_FOR_REVIEW"
        elif risk_level in ("medium", "low"):
            review_status = "NEEDS_REVIEW"
        else:
            review_status = "STATIC_CLEAN"

        return {
            "target": target,
            "kind": kind,
            "files_scanned": files_scanned,
            "bytes_scanned": bytes_scanned,
            "skipped_files": skipped_files,
            "findings": findings,
            "counts_by_severity": counts,
            "risk_level": risk_level,
            "review_status": review_status,
            "required_permissions": sorted(state["required_permissions"]),
            "license_status": license_status,
            "supply_chain_notes": supply_notes,
            "disclaimer": DISCLAIMER,
        }

    # ------------------------------------------------------------------ 输入收集

    def _collect(self, task: dict, files: list) -> tuple[str, int, int, int]:
        """三选一输入收集；返回 (target, files_scanned, bytes_scanned, skipped)。"""
        text = task.get("text")
        files_map = task.get("files")
        path = task.get("path")
        provided = sum(x is not None for x in (text, files_map, path))
        if provided != 1:
            raise InvalidParams("task requires exactly one of text|files|path")
        if text is not None:
            if not isinstance(text, str):
                raise InvalidParams("task.text must be a string")
            name = task.get("name") or "input.txt"
            if not isinstance(name, str) or "/" in name or name.startswith("."):
                raise InvalidParams("task.name must be a bare file name")
            raw_bytes = len(text.encode("utf-8"))
            if raw_bytes > MAX_FILE_BYTES:
                raise InputError(f"inline text exceeds {MAX_FILE_BYTES} bytes")
            files.append((name, text))
            return name, 1, raw_bytes, 0
        if files_map is not None:
            if not isinstance(files_map, dict) or not files_map:
                raise InvalidParams("task.files must be a non-empty object")
            if len(files_map) > MAX_FILES:
                raise InputError(f"files map exceeds {MAX_FILES} entries")
            total = 0
            for name, content in files_map.items():
                if not isinstance(name, str) or not isinstance(content, str):
                    raise InvalidParams("task.files entries must be str->str")
                b = len(content.encode("utf-8"))
                total += b
                if b > MAX_FILE_BYTES:
                    raise InputError(f"file {name!r} exceeds {MAX_FILE_BYTES} bytes")
                files.append((name.lstrip("/"), content))
            if total > MAX_TOTAL_BYTES:
                raise InputError(f"inline bundle exceeds {MAX_TOTAL_BYTES} bytes")
            return "(inline files)", len(files_map), total, 0

        # path：目录或单文件（只读文本扩展名；不跟随符号链接；.git 跳过）
        p = Path(str(path))
        try:
            if p.is_file():
                return self._collect_file(p, files)
            if p.is_dir():
                return self._collect_dir(p, files)
            raise InputError(f"path is not a readable file or directory: {str(p)!r}")
        except InputError:
            raise
        except OSError as exc:
            raise InputError(f"cannot read path: {exc}")

    @staticmethod
    def _collect_file(p: Path, files: list) -> tuple[str, int, int, int]:
        size = p.stat().st_size
        if size > MAX_FILE_BYTES:
            raise InputError(f"file exceeds {MAX_FILE_BYTES} bytes: {size}")
        if p.suffix.lower() not in SCAN_EXTENSIONS:
            raise InputError(
                f"extension {p.suffix.lower()!r} not in scan set (refusing binaries)")
        raw = p.read_bytes()
        files.append((p.name, raw.decode("utf-8", errors="replace")))
        return str(p), 1, len(raw), 0

    @staticmethod
    def _collect_dir(root: Path, files: list) -> tuple[str, int, int, int]:
        scanned = 0
        total_bytes = 0
        skipped = 0
        for entry in sorted(root.rglob("*")):
            if scanned >= MAX_FILES:
                skipped += 1
                continue
            if entry.is_symlink() or not entry.is_file():
                continue  # 不跟随符号链接（文件与目录一体拒绝）
            rel_parts = entry.relative_to(root).parts
            if any(part == ".git" for part in rel_parts[:-1]):
                continue
            if entry.suffix.lower() not in SCAN_EXTENSIONS \
                    and entry.name.lower() not in LICENSE_NAMES:
                # 扩展名不在扫描集（拒绝二进制）；无扩展名的 LICENSE/COPYING/
                # NOTICE 类声明文件仍扫描（许可核验需要它们）
                skipped += 1
                continue
            size = entry.stat().st_size
            if size > MAX_FILE_BYTES or total_bytes + size > MAX_TOTAL_BYTES:
                skipped += 1
                continue
            try:
                raw = entry.read_bytes()
            except OSError:
                skipped += 1
                continue
            files.append(("/".join(rel_parts),
                          raw.decode("utf-8", errors="replace")))
            scanned += 1
            total_bytes += size
        if scanned == 0:
            raise InputError(f"no scannable text files under {str(root)!r}")
        return str(root), scanned, total_bytes, skipped

    # ------------------------------------------------------------------ 版权/声明

    @staticmethod
    def _license_file(files: list):
        for rel, text in files:
            base = rel.rsplit("/", 1)[-1].lower()
            stem = base.rsplit(".", 1)[0] if "." in base else base
            if base in LICENSE_NAMES or stem in LICENSE_NAMES:
                return rel, text
        return None

    @staticmethod
    def _manifest_text(files: list):
        for rel, text in files:
            base = rel.rsplit("/", 1)[-1].lower()
            if base in MANIFEST_NAMES and base != "package.json":
                return rel, text
        return None


def _scan_manifest_license(rel: str, text: str, findings: list,
                           license_status: str) -> None:
    """Manifest license 字段与实际 LICENSE 文件交叉核验（静态一致性）。"""
    m = re.search(r"[\"']?license[\"']?\s*:\s*[\"']([^\"'\n]+)[\"']", text)
    if not m:
        return
    declared = m.group(1).strip()
    if declared.lower() == "unknown":
        _add(findings, "AUD-LIC-03", "high", rel, 1,
             "manifest license=unknown (registration blocked semantics)",
             "license_unknown_declared")
    elif license_status not in ("unknown", "present_unknown") \
            and not _licenses_agree(declared, license_status):
        _add(findings, "AUD-LIC-04", "medium", rel, 1,
             f"manifest declares {declared!r}, LICENSE file detects {license_status!r}",
             "license_mismatch")


if __name__ == "__main__":
    ExtensionAuditWorker().serve()
