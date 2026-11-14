#!/usr/bin/env python3
"""rip.src.* 三工具共用容器内入口（收尾武器库批交付，source-analysis pack
首三能力：依赖清单提取 / 静态检查 / 代码搜索；pack_id 与
manifests/packs/planned/source-analysis.json 冻结存根同源）。

帧协议（TOOL-CONTRACT D-TC-03）：stdin 一行 JSON 请求，stdout 一行 JSON 结果。
请求形状：{"run_id": "...", "input": {...}, "budget": {...}}；
结果形状：{"status": "result", "data": {...}}，错误帧
{"status": "error", "error": {"code": ..., "message": "..."}}；
错误码：-32700 请求解析失败；-32602 参数非法；-32006 执行错误。

op 分派（core_tools.py P3-4 同款）：op 取容器 argv[1]，input 即该 op 的参数对象；
兼容旧形态 input.tool + input.args（无 argv 时）。
- deps          依赖清单提取：package.json / requirements*.txt / pyproject.toml
                / Cargo.toml 归一化清单（name/constraint/kind/ecosystem），只读
                解析不联网不装包。
- static-check  静态检查（安全与代码卫生规则集）：秘钥形态（AWS/GitHub/PEM）、
                高危调用（eval/exec/os.system/shell=True/pickle.loads）、
                TODO/FIXME 密度；逐条 rule_id/severity/line/excerpt。
- search        代码搜索：目录限定 regex 检索（include/exclude glob、
                max_matches 截断、二进制跳过）。

安全边界：读限 /task/in 之下（os.path.realpath 归一化后前缀校验，防 ../ 逃逸）；
写限 /task/out 之下且目标名仅取 basename（防穿越）；产出与 result.json 均落
/task/out（Artifact 收集载体）。全部动作纯本地只读分析，network.mode=none
（T1 平台内部读写面，无外部目标副作用）。
"""

import glob
import json
import os
import re
import signal
import sys
import time

IN_ROOT = "/task/in"
OUT_ROOT = "/task/out"

MAX_DEPS = 2000
MAX_FINDINGS = 500
MAX_SEARCH_FILES = 5000
MAX_SEARCH_MATCHES = 200
MAX_FILE_BYTES = 1024 * 1024
EXCERPT_MAX = 120

# 依赖清单格式识别（deps op）：文件名/扩展名到生态的映射。
DEPS_FORMATS = {
    "package.json": "npm",
    "requirements.txt": "pip",
    "pyproject.toml": "pip",
    "Cargo.toml": "cargo",
}

# 静态检查规则集（static-check op）：编译期定长 tuple，无动态规则加载。
RULES = (
    ("SEC001", "high", re.compile(r"AKIA[0-9A-Z]{16}"), "AWS access key id 形态"),
    ("SEC002", "high", re.compile(r"\bgh[pousr]_[A-Za-z0-9]{30,}\b"), "GitHub token 形态"),
    ("SEC003", "high", re.compile(r"-----BEGIN [A-Z ]*PRIVATE KEY-----"), "私钥块头部"),
    ("SEC004", "medium", re.compile(r"(?i)\b(pass(word)?|secret|token|api[_-]?key)\b\s*[=:]\s*[\"'][^\"']{6,}[\"']"), "硬编码凭据赋值形态"),
    ("CODE001", "medium", re.compile(r"(?<![.\w])(eval|exec)\s*\("), "eval/exec 调用"),
    ("CODE002", "high", re.compile(r"os\.system\s*\(|subprocess\.\w+\([^)]*shell\s*=\s*True"), "shell 注入面（os.system / shell=True）"),
    ("CODE003", "medium", re.compile(r"pickle\.loads?\s*\(|yaml\.load\s*\((?![^)]*Loader)"), "不安全反序列化（pickle / 无 Loader 的 yaml.load）"),
)
TODO_RULE = re.compile(r"\b(TODO|FIXME|XXX|HACK)\b")


def _on_term(signum, _frame):
    """SIGTERM 处理器（D-TC-04 协作式取消的容器内侧，与 core_tools 同款）。"""
    sys.exit(128 + signum)


def _emit_error(code, message, exit_code):
    print(json.dumps({
        "status": "error",
        "error": {"code": code, "message": message},
    }, ensure_ascii=False))
    # stderr 镜像（worker 惯例）：Adapter 非零退出路径的信封消息取自
    # stderr，结构化错误帧必须在两路都可见。
    print(json.dumps({
        "status": "error",
        "error": {"code": code, "message": message},
    }, ensure_ascii=False), file=sys.stderr)
    return exit_code


def _in_path(raw):
    """归一化并校验输入路径：必须在 /task/in 之下且存在（core_tools 同款）。"""
    if not isinstance(raw, str) or not raw:
        raise ValueError("path is required")
    real = os.path.realpath(raw)
    if real != IN_ROOT and not real.startswith(IN_ROOT + os.sep):
        raise ValueError(f"path escapes /task/in: {raw!r}")
    if not os.path.exists(real):
        raise FileNotFoundError(f"input not found: {raw!r}")
    return real


def _out_name(raw, fallback):
    """输出文件名：仅取 basename，落 /task/out 之下（core_tools 同款）。"""
    name = os.path.basename(str(raw)) if raw else fallback
    if not name or name in (".", ".."):
        raise ValueError(f"output name invalid: {raw!r}")
    return os.path.join(OUT_ROOT, name)


def _read_text(real):
    """读文本文件（UTF-8，1 MiB 上限；超限与二进制按错误拒绝）。"""
    size = os.path.getsize(real)
    if size > MAX_FILE_BYTES:
        raise ValueError(f"file too large: {size} bytes (max {MAX_FILE_BYTES})")
    with open(real, "rb") as fh:
        raw = fh.read()
    try:
        return raw.decode("utf-8")
    except UnicodeDecodeError as exc:
        raise ValueError(f"not utf-8 text: {exc}") from exc


def _write_out(name, payload):
    """结果快照落 /task/out（Artifact 收集载体，core_tools 同款）。"""
    out = _out_name(name, name)
    with open(out, "w", encoding="utf-8") as fh:
        json.dump(payload, fh, ensure_ascii=False)
    return os.path.basename(out)


def _constraint_str(spec):
    """依赖约束统一为字符串（dict 形态的取 version 字段，pip 行原样保留）。"""
    if isinstance(spec, str):
        return spec.strip()
    if isinstance(spec, dict):
        ver = spec.get("version") or spec.get("git") or spec.get("path") or "*"
        return str(ver)
    return "*"


def parse_deps(text, fmt):
    """按格式解析依赖清单文本 → 归一化条目列表（纯函数，宿主可测）。

    package.json 依赖 JSON 解析；requirements*.txt 逐行剥离注释与选项行；
    pyproject.toml 用 3.11+ 标准库 tomllib；Cargo.toml 用 tomllib 取
    [dependencies]/[dev-dependencies] 段。越界与形态异常行计入 notes 不中断。
    """
    deps, notes = [], []
    if fmt == "npm":
        doc = json.loads(text)
        for kind in ("dependencies", "devDependencies", "peerDependencies"):
            for name, spec in (doc.get(kind) or {}).items():
                deps.append({
                    "name": name,
                    "constraint": _constraint_str(spec),
                    "kind": "dev" if kind == "devDependencies" else "runtime",
                    "source_line": None,
                })
    elif fmt == "pip-requirements":
        for lineno, line in enumerate(text.splitlines(), start=1):
            stripped = line.strip()
            if not stripped or stripped.startswith(("#", "-")):
                continue
            name = re.split(r"[<>=!~\[; ]", stripped, 1)[0].strip()
            if not name:
                notes.append(f"line {lineno}: unparsable requirement {stripped[:60]!r}")
                continue
            deps.append({
                "name": name,
                "constraint": stripped,
                "kind": "runtime",
                "source_line": lineno,
            })
    elif fmt == "pip-pyproject":
        import tomllib
        doc = tomllib.loads(text)
        proj = doc.get("project") or {}
        for name_spec in list(proj.get("dependencies") or []):
            name = re.split(r"[<>=!~\[; ]", str(name_spec), 1)[0].strip()
            deps.append({"name": name, "constraint": str(name_spec), "kind": "runtime", "source_line": None})
        for name_spec in list((proj.get("optional-dependencies") or {}).get("dev") or []):
            name = re.split(r"[<>=!~\[; ]", str(name_spec), 1)[0].strip()
            deps.append({"name": name, "constraint": str(name_spec), "kind": "dev", "source_line": None})
    elif fmt == "cargo":
        import tomllib
        doc = tomllib.loads(text)
        for section, kind in (("dependencies", "runtime"), ("dev-dependencies", "dev")):
            for name, spec in (doc.get(section) or {}).items():
                deps.append({
                    "name": name,
                    "constraint": _constraint_str(spec),
                    "kind": kind,
                    "source_line": None,
                })
    else:
        raise ValueError(f"unsupported deps format: {fmt!r}")
    if len(deps) > MAX_DEPS:
        notes.append(f"truncated at {MAX_DEPS} entries")
        deps = deps[:MAX_DEPS]
    return deps, notes


def action_deps(args):
    """deps op：识别格式（显式 format 或文件名映射）→ 归一化清单。"""
    real = _in_path(args.get("path"))
    fmt = args.get("format") or "auto"
    basename = os.path.basename(real).lower()
    if fmt == "auto":
        if basename == "package.json":
            fmt = "npm"
        elif basename == "pyproject.toml":
            fmt = "pip-pyproject"
        elif basename == "cargo.toml":
            fmt = "cargo"
        elif basename.startswith("requirements") and basename.endswith(".txt"):
            fmt = "pip-requirements"
        else:
            raise ValueError(f"cannot infer deps format from {basename!r}; pass format explicitly")
    if fmt not in ("npm", "pip-requirements", "pip-pyproject", "cargo"):
        raise ValueError(f"unsupported deps format: {fmt!r}")
    deps, notes = parse_deps(_read_text(real), fmt)
    data = {
        "file": os.path.basename(real),
        "format": fmt,
        "ecosystem": {"npm": "npm", "pip-requirements": "pip", "pip-pyproject": "pip", "cargo": "cargo"}[fmt],
        "count": len(deps),
        "deps": deps,
        "notes": notes,
    }
    data["output_file"] = _write_out("deps.json", data)
    return data


def scan_text(text, rules=RULES):
    """静态检查规则扫描（纯函数，宿主可测）：逐行跑规则集，超限截断。"""
    findings, todo_lines = [], 0
    for lineno, line in enumerate(text.splitlines(), start=1):
        if TODO_RULE.search(line):
            todo_lines += 1
        for rule_id, severity, pattern, message in rules:
            match = pattern.search(line)
            if match:
                if len(findings) >= MAX_FINDINGS:
                    return findings, todo_lines, True
                findings.append({
                    "rule_id": rule_id,
                    "severity": severity,
                    "line": lineno,
                    "excerpt": line.strip()[:EXCERPT_MAX],
                })
    return findings, todo_lines, False


def action_static_check(args):
    """static-check op：单文件或目录内文本文件扫描 → findings 与摘要。"""
    real = _in_path(args.get("path"))
    target = real
    files_scanned = 0
    if os.path.isdir(real):
        texts = sorted(
            p for p in glob.glob(os.path.join(real, "**", "*"), recursive=True)
            if os.path.isfile(p) and (
                os.path.splitext(p)[1] in (
                    ".py", ".rs", ".js", ".ts", ".go", ".java", ".sh", ".toml",
                    ".yaml", ".yml", ".json", ".md", ".txt",
                )
                # 前缀点文件（.env 类）splitext 后缀为空，按 basename 白名单收。
                or os.path.basename(p) in (".env", ".env.local", ".npmrc")
            )
        )[:MAX_SEARCH_FILES]
    else:
        texts = [real]
    findings, truncated = [], False
    todo_total = 0
    for path in texts:
        try:
            text = _read_text(path)
        except ValueError:
            continue  # 二进制/超限文件跳过并计数，不中断整批扫描
        files_scanned += 1
        part, todo_lines, trunc = scan_text(text)
        todo_total += todo_lines
        truncated = truncated or trunc
        findings.extend({**f, "file": os.path.relpath(path, target)} for f in part)
        if len(findings) >= MAX_FINDINGS:
            findings = findings[:MAX_FINDINGS]
            truncated = True
            break
    summary = {}
    for f in findings:
        summary[f["severity"]] = summary.get(f["severity"], 0) + 1
    data = {
        "target": os.path.basename(real.rstrip("/")) or real,
        "files_scanned": files_scanned,
        "lines_todo": todo_total,
        "count": len(findings),
        "findings": findings,
        "summary": summary,
        "truncated": truncated,
    }
    data["output_file"] = _write_out("static-check.json", data)
    return data


def _glob_match(rel, patterns, default):
    """glob 匹配（fnmatch 语义）；patterns 为空返回 default。"""
    if not patterns:
        return default
    return any(glob.fnmatch.fnmatch(rel, p) for p in patterns)


def search_tree(root, pattern, include=None, exclude=None,
                case_insensitive=False, max_matches=MAX_SEARCH_MATCHES):
    """目录限定 regex 检索（纯函数，宿主可测）：跳过二进制，双截断护栏。"""
    flags = re.IGNORECASE if case_insensitive else 0
    try:
        rx = re.compile(pattern, flags)
    except re.error as exc:
        raise ValueError(f"invalid regex: {exc}") from exc
    matches, notes = [], []
    files_scanned = 0
    truncated = False
    for path in sorted(glob.glob(os.path.join(root, "**", "*"), recursive=True)):
        if not os.path.isfile(path) or len(matches) >= max_matches:
            continue
        if files_scanned >= MAX_SEARCH_FILES:
            notes.append(f"file budget exhausted at {MAX_SEARCH_FILES}")
            truncated = True
            break
        rel = os.path.relpath(path, root)
        if not _glob_match(rel, include, True) or _glob_match(rel, exclude, False):
            continue
        try:
            text = _read_text(path)
        except ValueError:
            continue
        files_scanned += 1
        for lineno, line in enumerate(text.splitlines(), start=1):
            if rx.search(line):
                matches.append({
                    "file": rel,
                    "line": lineno,
                    "excerpt": line.strip()[:EXCERPT_MAX],
                })
                if len(matches) >= max_matches:
                    truncated = True
                    break
    return {
        "matches": matches,
        "files_scanned": files_scanned,
        "count": len(matches),
        "truncated": truncated,
        "notes": notes,
    }


def action_search(args):
    """search op：/task/in 之下的 regex 检索。"""
    real = _in_path(args.get("path"))
    pattern = args.get("pattern")
    if not isinstance(pattern, str) or not pattern:
        raise ValueError("pattern is required")
    if os.path.isdir(real):
        result = search_tree(
            real, pattern,
            include=args.get("include"),
            exclude=args.get("exclude"),
            case_insensitive=bool(args.get("case_insensitive")),
            max_matches=min(int(args.get("max_matches") or MAX_SEARCH_MATCHES), MAX_SEARCH_MATCHES),
        )
    else:
        text = _read_text(real)
        try:
            rule = re.compile(pattern, re.IGNORECASE if args.get("case_insensitive") else 0)
        except re.error as exc:
            raise ValueError(f"invalid regex: {exc}") from exc
        part, _, trunc = scan_text(text, rules=(
            ("MATCH", "info", rule, "match"),
        ))
        result = {
            "matches": [{"file": os.path.basename(real), "line": f["line"], "excerpt": f["excerpt"]} for f in part],
            "files_scanned": 1,
            "count": len(part),
            "truncated": trunc,
            "notes": [],
        }
    result["target"] = os.path.basename(real.rstrip("/")) or real
    result["output_file"] = _write_out("search.json", result)
    return result


ACTIONS = {
    "deps": action_deps,
    "static-check": action_static_check,
    "search": action_search,
}


def main() -> int:
    signal.signal(signal.SIGTERM, _on_term)
    started = time.monotonic()
    raw = sys.stdin.readline()
    try:
        request = json.loads(raw)
    except json.JSONDecodeError as exc:
        return _emit_error(-32700, f"request parse failed: {exc}", 2)

    inp = request.get("input") or {}
    if len(sys.argv) > 1:
        # op 分派：op 取自 Manifest runtime.entrypoint 第三位，input 即参数。
        op = sys.argv[1]
        args = inp if isinstance(inp, dict) else {}
    else:
        # 兼容旧形态：input.tool + input.args。
        op = inp.get("tool")
        args = inp.get("args") or {}
    if not isinstance(op, str) or op not in ACTIONS:
        return _emit_error(
            -32602,
            f"op must be one of {sorted(ACTIONS)} (argv[1] or input.tool)",
            2,
        )
    if not isinstance(args, dict):
        return _emit_error(-32602, "input must be an args object", 2)

    try:
        data = ACTIONS[op](args)
    except (ValueError, FileNotFoundError, KeyError, OSError, json.JSONDecodeError) as exc:
        code = -32602 if isinstance(exc, ValueError) else -32006
        return _emit_error(code, f"{type(exc).__name__}: {exc}", 2 if code == -32602 else 1)

    data["tool"] = op
    data["elapsed_ms"] = int((time.monotonic() - started) * 1000)
    data["uid"] = os.getuid()
    print(json.dumps({"status": "result", "data": data}, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    sys.exit(main())
