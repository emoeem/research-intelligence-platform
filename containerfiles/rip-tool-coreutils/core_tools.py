#!/usr/bin/env python3
"""rip.core.* 五工具共用容器内入口（P3-1 交付，P3-4 改 op 分派并对齐五 Manifest）。

帧协议（TOOL-CONTRACT D-TC-03）：stdin 一行 JSON 请求，stdout 一行 JSON 结果。
请求形状：{"run_id": "...", "input": {...}, "budget": {...}}；
结果形状：{"status": "result", "data": {...}}，错误帧
{"status": "error", "error": {"code": ..., "message": "..."}}；
错误码：-32700 请求解析失败；-32602 参数非法；-32006 执行错误。

op 分派（P3-4）：五 Manifest 共用本镜像与入口，op 取容器 argv[1]（各 Manifest
runtime.entrypoint 第三位：file_meta/hash/compress/convert/validate），input 即
该 op 的参数对象；兼容旧形态 input.tool + input.args（无 argv 时）。
- file_meta  路径元数据 + magic 字节嗅探类型（hashlib/tarfile 标准库）。
- hash       sha256/sha1/sha512/md5 文件摘要（1 MiB 分块）。
- compress   gzip/gunzip/tar/untar（gzip + tarfile；untar 用 filter="data"
             防绝对路径与 .. 穿越，Python 3.12 原生支持）。
- convert    json 与 csv 互转的受限矩阵（json + csv 标准库；json 侧要求
             扁平对象数组；csv 侧值全部按字符串保留并如实标注）。
- validate   json/csv 结构化解析与统计；YAML 显式不支持（stdlib-only 镜像，
             PHASE3-DESIGN §5.2 降级声明：不自研解析器、不装第三方包）。

安全边界：读限 /task/in 之下（os.path.realpath 归一化后前缀校验，防 ../ 逃逸）；
写限 /task/out 之下且目标名仅取 basename（防穿越）；产出与 result.json 均落
/task/out（Artifact 收集载体，keep-id 下属主为宿主用户）。全部动作纯本地变换，
network.mode=none（T1 平台内部读写面，无外部目标副作用）。
"""

import csv
import gzip
import hashlib
import io
import json
import os
import signal
import sys
import tarfile
import time

IN_ROOT = "/task/in"
OUT_ROOT = "/task/out"
HASH_ALGOS = ("sha256", "sha1", "sha512", "md5")
CHUNK = 1024 * 1024

MAGIC = (
    (b"\x1f\x8b", "gzip", "application/gzip"),
    (b"PK\x03\x04", "zip", "application/zip"),
    (b"%PDF-", "pdf", "application/pdf"),
    (b"\x89PNG", "png", "image/png"),
    (b"\xff\xd8\xff", "jpeg", "image/jpeg"),
    (b"\x7fELF", "elf", "application/x-executable"),
)
TEXT_SUFFIXES = (
    ".txt", ".md", ".json", ".csv", ".yaml", ".yml", ".py", ".rs", ".toml",
    ".html", ".xml", ".log", ".ini", ".cfg",
)


def _on_term(signum, _frame):
    """SIGTERM 处理器（D-TC-04 协作式取消的容器内侧，与 smoke_tool 同款）。"""
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
    """归一化并校验输入路径：必须在 /task/in 之下且存在。"""
    if not isinstance(raw, str) or not raw:
        raise ValueError("path is required")
    real = os.path.realpath(raw)
    if real != IN_ROOT and not real.startswith(IN_ROOT + os.sep):
        raise ValueError(f"path escapes /task/in: {raw!r}")
    if not os.path.exists(real):
        raise FileNotFoundError(f"input not found: {raw!r}")
    return real


def _out_name(raw, fallback):
    """输出文件名：仅取 basename，落 /task/out 之下。"""
    name = os.path.basename(str(raw)) if raw else fallback
    if not name or name in (".", ".."):
        raise ValueError(f"output name invalid: {raw!r}")
    return os.path.join(OUT_ROOT, name)


def sha256_of(path):
    digest = hashlib.sha256()
    with open(path, "rb") as fh:
        while True:
            block = fh.read(CHUNK)
            if not block:
                break
            digest.update(block)
    return digest.hexdigest()


def sniff(path):
    """magic 字节嗅探：返回 (kind, mime_guess)。"""
    with open(path, "rb") as fh:
        head = fh.read(8)
    for prefix, kind, mime in MAGIC:
        if head.startswith(prefix):
            return kind, mime
    if tarfile.is_tarfile(path):
        return "tar", "application/x-tar"
    try:
        with open(path, "rb") as fh:
            fh.read(1024 * 1024).decode("utf-8")
        return "text", "text/plain; charset=utf-8"
    except (UnicodeDecodeError, OSError):
        return "binary", "application/octet-stream"


def action_file_meta(args):
    path = _in_path(args.get("path"))
    is_dir = os.path.isdir(path)
    data = {
        "name": os.path.basename(path),
        "path": path,
        "is_dir": is_dir,
        "is_symlink": os.path.islink(args.get("path", "")),
        "size_bytes": 0 if is_dir else os.path.getsize(path),
        "mtime": int(os.path.getmtime(path)),
        "sha256": None,
        "type": "directory",
        "mime_guess": "inode/directory",
    }
    if not is_dir:
        kind, mime = sniff(path)
        data["sha256"] = sha256_of(path)
        data["type"] = kind
        data["mime_guess"] = mime
    return data


def action_hash(args):
    path = _in_path(args.get("path"))
    algo = args.get("algorithm", "sha256")
    if algo not in HASH_ALGOS:
        raise ValueError(f"algorithm not allowed: {algo!r} (one of {HASH_ALGOS})")
    digest = hashlib.new(algo)
    with open(path, "rb") as fh:
        while True:
            block = fh.read(CHUNK)
            if not block:
                break
            digest.update(block)
    return {
        "name": os.path.basename(path),
        "algorithm": algo,
        "digest": digest.hexdigest(),
        "size_bytes": os.path.getsize(path),
    }


def _copy_stream(src_fh, dst_fh):
    total = 0
    while True:
        block = src_fh.read(CHUNK)
        if not block:
            break
        dst_fh.write(block)
        total += len(block)
    return total


def action_compress(args):
    mode = args.get("mode")
    src = _in_path(args.get("src"))
    base = os.path.basename(src)
    os.makedirs(OUT_ROOT, exist_ok=True)
    if mode == "gzip":
        dest = _out_name(args.get("dest_name"), base + ".gz")
        with open(src, "rb") as fin, gzip.open(dest, "wb") as fout:
            total = _copy_stream(fin, fout)
    elif mode == "gunzip":
        dest = _out_name(
            args.get("dest_name"),
            base[:-3] if base.endswith(".gz") else base + ".out",
        )
        with gzip.open(src, "rb") as fin, open(dest, "wb") as fout:
            total = _copy_stream(fin, fout)
    elif mode == "tar":
        dest = _out_name(args.get("dest_name"), base + ".tar")
        with tarfile.open(dest, "w") as tf:
            tf.add(src, arcname=base, recursive=True)
        total = os.path.getsize(dest)
    elif mode == "untar":
        dest_dir_name = os.path.basename(str(args.get("dest_name") or "extracted"))
        dest_dir = os.path.join(OUT_ROOT, dest_dir_name)
        os.makedirs(dest_dir, exist_ok=True)
        with tarfile.open(src) as tf:
            tf.extractall(dest_dir, filter="data")
        total = sum(
            os.path.getsize(os.path.join(root, name))
            for root, _dirs, files in os.walk(dest_dir)
            for name in files
        )
        return {
            "mode": mode,
            "src": src,
            "dest": dest_dir,
            "size_bytes": total,
            "sha256": None,
            "is_dir": True,
        }
    else:
        raise ValueError(f"mode not allowed: {mode!r} (gzip|gunzip|tar|untar)")
    return {
        "mode": mode,
        "src": src,
        "dest": dest,
        "size_bytes": total,
        "sha256": sha256_of(dest),
    }


def _load_convert_input(args):
    """convert/validate 共用：优先 text 内联，其次 /task/in 文件。返回 (text, src_name)。"""
    text = args.get("text")
    if isinstance(text, str):
        return text, "inline"
    src = _in_path(args.get("src"))
    with open(src, "r", encoding="utf-8") as fh:
        return fh.read(), os.path.basename(src)


def action_convert(args):
    src_kind = args.get("from")
    dst_kind = args.get("to")
    if src_kind not in ("json", "csv") or dst_kind not in ("json", "csv"):
        raise ValueError(
            "from/to must be json|csv (supported matrix: json->csv, csv->json)"
        )
    delimiter = str(args.get("delimiter") or ",")
    if len(delimiter) != 1:
        raise ValueError("delimiter must be a single character")
    text, src_name = _load_convert_input(args)
    os.makedirs(OUT_ROOT, exist_ok=True)

    if src_kind == "json" and dst_kind == "csv":
        parsed = json.loads(text)
        if not isinstance(parsed, list) or not all(isinstance(r, dict) for r in parsed):
            raise ValueError("json->csv requires an array of flat objects")
        columns = []
        for row in parsed:
            for key in row:
                if key not in columns:
                    columns.append(key)
        for row in parsed:
            for key, value in row.items():
                if isinstance(value, (dict, list)):
                    raise ValueError(
                        f"json->csv requires flat values (nested at key {key!r})"
                    )
        dest = _out_name(
            args.get("dest_name"), os.path.splitext(src_name)[0] + ".csv"
        )
        with open(dest, "w", encoding="utf-8", newline="") as fh:
            writer = csv.DictWriter(fh, fieldnames=columns, delimiter=delimiter)
            writer.writeheader()
            for row in parsed:
                writer.writerow(
                    {k: "" if row.get(k) is None else row.get(k) for k in columns}
                )
        return {
            "conversion": "json->csv",
            "src": src_name,
            "dest": dest,
            "rows": len(parsed),
            "columns": columns,
            "sha256": sha256_of(dest),
        }
    if src_kind == "csv" and dst_kind == "json":
        rows = list(csv.reader(io.StringIO(text), delimiter=delimiter))
        if not rows:
            raise ValueError("csv input is empty")
        header = rows[0]
        records = [dict(zip(header, row)) for row in rows[1:]]
        dest = _out_name(
            args.get("dest_name"), os.path.splitext(src_name)[0] + ".json"
        )
        payload = {
            "header": header,
            "rows": records,
            "note": "all values are strings (csv round-trip keeps text form)",
        }
        with open(dest, "w", encoding="utf-8") as fh:
            json.dump(payload, fh, ensure_ascii=False, indent=2, sort_keys=True)
        return {
            "conversion": "csv->json",
            "src": src_name,
            "dest": dest,
            "rows": len(records),
            "columns": header,
            "sha256": sha256_of(dest),
        }
    raise ValueError(f"conversion not supported: {src_kind}->{dst_kind}")


def _depth(value):
    if isinstance(value, dict):
        return 1 + max((_depth(v) for v in value.values()), default=0)
    if isinstance(value, list):
        return 1 + max((_depth(v) for v in value), default=0)
    return 0


def action_validate(args):
    fmt = args.get("format")
    text, src_name = _load_convert_input(args)
    if fmt == "yaml":
        raise ValueError(
            "yaml_unsupported: stdlib-only image has no YAML parser "
            "(PHASE3-DESIGN section 5.2 downgrade, no third-party package)"
        )
    if fmt == "json":
        parsed = json.loads(text)
        return {
            "format": "json",
            "src": src_name,
            "valid": True,
            "top_type": type(parsed).__name__,
            "depth": _depth(parsed),
            "top_keys": len(parsed) if isinstance(parsed, dict) else None,
            "items": len(parsed) if isinstance(parsed, list) else None,
            "size_bytes": len(text.encode("utf-8")),
        }
    if fmt == "csv":
        rows = list(
            csv.reader(io.StringIO(text), delimiter=str(args.get("delimiter") or ","))
        )
        if not rows:
            raise ValueError("csv input is empty")
        return {
            "format": "csv",
            "src": src_name,
            "valid": True,
            "header": rows[0],
            "rows": len(rows) - 1,
            "cols_max": max(len(row) for row in rows),
            "size_bytes": len(text.encode("utf-8")),
        }
    raise ValueError(f"format not allowed: {fmt!r} (json|csv; yaml unsupported)")


ACTIONS = {
    "file_meta": action_file_meta,
    "hash": action_hash,
    "compress": action_compress,
    "convert": action_convert,
    "validate": action_validate,
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
        # P3-4 op 分派：op 取自 Manifest runtime.entrypoint 第三位，input 即参数。
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
    except (
        ValueError,
        FileNotFoundError,
        KeyError,
        OSError,
        tarfile.TarError,
        csv.Error,
        json.JSONDecodeError,
    ) as exc:
        code = -32602 if isinstance(exc, ValueError) else -32006
        return _emit_error(code, f"{type(exc).__name__}: {exc}", 2 if code == -32602 else 1)

    data["tool"] = op
    data["elapsed_ms"] = int((time.monotonic() - started) * 1000)
    data["uid"] = os.getuid()
    # 不适用字段不输出（如列表 json 的 top_keys、目录的 sha256、untar 的
    # sha256）：output_schema 按值类型声明，null 会判格式违规。
    data = {k: v for k, v in data.items() if v is not None}
    try:
        os.makedirs(OUT_ROOT, exist_ok=True)
        with open(os.path.join(OUT_ROOT, "result.json"), "w", encoding="utf-8") as fh:
            json.dump(data, fh, ensure_ascii=False, sort_keys=True)
    except OSError as exc:
        return _emit_error(-32006, f"/task/out write failed: {exc}", 1)
    print(json.dumps({"status": "result", "data": data}, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    sys.exit(main())
