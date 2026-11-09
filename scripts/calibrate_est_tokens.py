#!/usr/bin/env python3
"""est_tokens 常数对 bge tokenizer 离线校准（P4-6，ROADMAP §5.5 风险 1 的收口动作）。

只读：models/bge-small-zh-v1.5/tokenizer.json（WordPiece，词表 21128）；
std-lib only（python3 -I 运行，零安装红线）。对比对象：crates/context-broker
src/estimate.rs 的公式 est_tokens = ceil(ascii/4 + cjk*1.0 + other/2)。

口径：
- bge 侧按 BERT 基本分词近似（小写、空白切分、CJK 单字、标点独立），
  再对每段做 WordPiece 贪心最长匹配（## 续接），OOV 记 [UNK] 计 1；
- 抽样覆盖 Broker 真实负载形态：规则包正文（CJK 为主）、工具 brief
  （混合）、意图查询（短句）、ascii 技术串（id/路径/URL）；
- 判定：各样本 est/bge 比值的中位数落在 [0.8, 1.25] 视为常数可保持
  （预算分项只有相对意义，设计稿 §13.1）；越界则输出修正常数建议。

留痕：结果 JSON 写 docs/phase4/est-tokens-calibration.json（同批提交）。
"""

import json
import math
import sys
from pathlib import Path

TOKENIZER_PATH = Path("models/bge-small-zh-v1.5/tokenizer.json")
TRACE_PATH = Path("docs/phase4/est-tokens-calibration.json")

CJK_RANGES = (
    (0x3000, 0x303F),
    (0x3040, 0x30FF),
    (0x3400, 0x4DBF),
    (0x4E00, 0x9FFF),
    (0xF900, 0xFAFF),
    (0xFF00, 0xFFEF),
)


def is_cjk(ch: str) -> bool:
    cp = ord(ch)
    return any(lo <= cp <= hi for lo, hi in CJK_RANGES)


def est_tokens(text: str) -> int:
    """与 crates/context-broker/src/estimate.rs 同公式（当前常数）。"""
    ascii_n = sum(1 for c in text if ord(c) < 128)
    cjk_n = sum(1 for c in text if not ord(c) < 128 and is_cjk(c))
    other_n = len(text) - ascii_n - cjk_n
    return math.ceil(ascii_n / 4 + cjk_n * 1.0 + other_n / 2)


def basic_tokenize(text: str) -> list:
    """BERT basic 近似：空白切分 -> 小写 -> 段内按 CJK 单字与标点再切。"""
    out = []
    for chunk in text.split():
        buf = ""
        for ch in chunk.lower():
            if is_cjk(ch):
                if buf:
                    out.append(buf)
                    buf = ""
                out.append(ch)
            elif ch.isascii() and (ch.isalnum() or ch in "_+#./-"):
                buf += ch
            else:
                if buf:
                    out.append(buf)
                    buf = ""
                out.append(ch)
        if buf:
            out.append(buf)
    return out


def wordpiece(tokens: list, vocab: dict, max_chars: int = 100) -> int:
    """贪心最长匹配 WordPiece（## 续接）；OOV 记 1（[UNK] 等价）。"""
    total = 0
    for token in tokens:
        if len(token) > max_chars:
            total += 1
            continue
        if token in vocab:
            total += 1
            continue
        start, count = 0, 0
        unk = False
        while start < len(token):
            end = len(token)
            match = None
            while start < end:
                candidate = token[start:end]
                if start > 0:
                    candidate = "##" + candidate
                if candidate in vocab:
                    match = candidate
                    break
                end -= 1
            if match is None:
                unk = True
                break
            count += 1
            start = end if start == 0 else end
        total += count + (1 if unk and count == 0 else 0)
    return total


SAMPLES = [
    ("rule-body-cjk", "任何一层内容进入上下文前先过预算闸，不把整个知识库或所有工具描述塞入每次上下文。"),
    ("rule-body-mixed", "AI 不允许随意污染宿主机：不在宿主机安装任何依赖或工具，运行面走容器与 vendored 依赖。"),
    ("brief-mixed", "rip.kb.keyword_search v0.1.0：对 platform.db 的五面做关键词 LIKE 检索（只读）｜输入必填: keyword｜输出: keyword, groups｜限制: 网络 none，权限 T0"),
    ("query-short", "kingdee HTTP 检查"),
    ("query-cjk", "扫描 example.com 的端口并核验来源"),
    ("ascii-id", "ctxm-14c4b1316d5ebe159bb33da5 rtis://poc/900008 file:///tmp/art-acc-3.bin"),
    ("ascii-path", "crates/context-broker/src/sources/registry.rs REF_ONLY_COST"),
    ("doc-title", "K3Cloud ServiceFacade 错误码判读：N001 不是服务存在性判据"),
]


def main() -> int:
    if not TOKENIZER_PATH.exists():
        print(f"[skip] {TOKENIZER_PATH} 不存在（CI 口径）；本机口径应存在（P4-6 实测）")
        return 0
    doc = json.loads(TOKENIZER_PATH.read_text(encoding="utf-8"))
    model = doc.get("model", {})
    vocab = model.get("vocab", {})
    if not vocab:
        print("[fail] tokenizer.json 无 vocab")
        return 1

    rows = []
    ratios = []
    for name, text in SAMPLES:
        bge = wordpiece(basic_tokenize(text), vocab)
        est = est_tokens(text)
        ratio = round(est / bge, 4) if bge else 0.0
        ratios.append(ratio)
        rows.append({"sample": name, "chars": len(text), "bge_tokens": bge, "est_tokens": est, "est_over_bge": ratio})

    ratios.sort()
    median = ratios[len(ratios) // 2]
    verdict = "keep" if 0.8 <= median <= 1.25 else "recalibrate"
    trace = {
        "date": "2026-11-10",
        "tokenizer": str(TOKENIZER_PATH),
        "model_type": model.get("type"),
        "vocab_size": len(vocab),
        "formula": "est = ceil(ascii/4 + cjk*1.0 + other/2)",
        "tolerance": [0.8, 1.25],
        "median_est_over_bge": median,
        "verdict": verdict,
        "samples": rows,
    }
    print(f"{'sample':<16} {'chars':>6} {'bge':>5} {'est':>5} {'est/bge':>8}")
    for row in rows:
        print(f"{row['sample']:<16} {row['chars']:>6} {row['bge_tokens']:>5} {row['est_tokens']:>5} {row['est_over_bge']:>8}")
    print(f"median est/bge = {median} -> verdict: {verdict}")

    TRACE_PATH.parent.mkdir(parents=True, exist_ok=True)
    TRACE_PATH.write_text(json.dumps(trace, ensure_ascii=False, indent=1), encoding="utf-8")
    print(f"trace written: {TRACE_PATH}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
