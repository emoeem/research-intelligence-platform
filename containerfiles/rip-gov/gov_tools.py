#!/usr/bin/env python3
"""rip.gov.* 治理 worker 容器共用入口（RIP3 P3-2，D3 近似重复面）。

帧协议（TOOL-CONTRACT D-TC-03）：stdin 一行 JSON 请求 {"run_id","input","budget"}；
stdout 一行 JSON 结果 {"status":"result","data":{...}}（stdout 守卫同 parser_tools：
库噪声改道 stderr，帧 = stdout 第一行）。op 取 argv[1]。

op 分派：
- minhash  datasketch MinHash+MinHashLSH 批量近似重复候选发现（R2 5.2 D3）：
           input.items = [{"id","text"}]，char-bigram ∪ 词元 shingling（中英混
           排覆盖），LSH 候选对 + jaccard 估计值排序输出；只产候选不判定合并
           （合并走审核，R2 5.2 硬性）。

安全边界：纯本地计算，network.mode=none（manifest 强制）；条目数帽 5000、单条
文本帽 64KB（超帽拒绝，-32602）；SIGTERM 协作取消（D-TC-04）。
"""

from __future__ import annotations

import json
import signal
import sys

MAX_ITEMS = 5000
MAX_ITEM_BYTES = 64 * 1024


def _on_term(signum, _frame):
    sys.exit(128 + signum)


def _emit(payload):
    print(json.dumps(payload, ensure_ascii=False))
    sys.stdout.flush()


def _emit_error(code, message, exit_code=1):
    frame = {"status": "error", "error": {"code": code, "message": message}}
    print(json.dumps(frame, ensure_ascii=False))
    print(json.dumps(frame, ensure_ascii=False), file=sys.stderr)
    sys.stderr.flush()
    return exit_code


def _shingles(text):
    """词元 ∪ char-bigram 混合 shingling（中英混排覆盖；去重集合）。"""
    words = set(text.split())
    compact = "".join(text.split())
    bigrams = {compact[i : i + 2] for i in range(len(compact) - 1)}
    return words | bigrams


def op_minhash(req_input):
    items = req_input.get("items")
    if not isinstance(items, list) or not items:
        raise ValueError("items must be a non-empty array of {id, text}")
    if len(items) > MAX_ITEMS:
        raise ValueError(f"items exceeds cap {MAX_ITEMS}")
    num_perm = int(req_input.get("num_perm", 128))
    threshold = float(req_input.get("threshold", 0.5))
    if not (16 <= num_perm <= 512):
        raise ValueError("num_perm must be within [16,512]")
    if not (0.1 <= threshold <= 0.95):
        raise ValueError("threshold must be within [0.1,0.95]")

    from datasketch import MinHash, MinHashLSH

    from importlib.metadata import version

    sig = {}
    lsh = MinHashLSH(threshold=threshold, num_perm=num_perm)
    for it in items:
        if not isinstance(it, dict) or "id" not in it or "text" not in it:
            raise ValueError("each item must be an object with id and text")
        text = str(it["text"])
        if len(text.encode("utf-8")) > MAX_ITEM_BYTES:
            raise ValueError(f"item {it['id']!r} text exceeds 64KiB cap")
        m = MinHash(num_perm=num_perm)
        for sh in _shingles(text):
            m.update(sh.encode("utf-8"))
        sig[str(it["id"])] = m
        lsh.insert(str(it["id"]), m)

    pairs = []
    seen = set()
    for key, m in sig.items():
        for other in lsh.query(m):
            lo, hi = sorted([key, other])
            if lo == hi:
                continue
            pair = (lo, hi)
            if pair in seen:
                continue
            seen.add(pair)
            j = m.jaccard(sig[other])
            pairs.append({"a": lo, "b": hi, "jaccard_est": round(j, 4)})
    pairs.sort(key=lambda p: -p["jaccard_est"])
    return {
        "pairs": pairs,
        "items": len(sig),
        "num_perm": num_perm,
        "threshold": threshold,
        "engine": f"datasketch {version('datasketch')}",
    }


OPS = {
    "minhash": op_minhash,
}


def main(argv):
    signal.signal(signal.SIGTERM, _on_term)
    if len(argv) < 2 or argv[1] not in OPS:
        return _emit_error(-32602, "usage: gov_tools.py <minhash>")
    op = argv[1]
    line = sys.stdin.readline()
    if not line.strip():
        return _emit_error(-32700, "empty request line")
    try:
        request = json.loads(line)
    except json.JSONDecodeError as e:
        return _emit_error(-32700, f"request parse failed: {e}")
    req_input = request.get("input") or {}
    real_stdout = sys.stdout
    sys.stdout = sys.stderr
    try:
        try:
            data = OPS[op](req_input)
        except ValueError as e:
            return _emit_error(-32602, str(e))
        except ImportError as e:
            return _emit_error(-32006, f"engine import failed: {e}")
        except Exception as e:  # noqa: BLE001 容器侧顶层兜底
            return _emit_error(-32006, f"execution error: {type(e).__name__}: {e}")
    finally:
        sys.stdout = real_stdout
    _emit({"status": "result", "data": data})
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
