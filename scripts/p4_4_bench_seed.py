#!/usr/bin/env python3
"""P4-4 Benchmark 语料播种（R2 6.8：RTIS 真实语料 + 固定合成集）。

数据来源与红线：
- RTIS 侧只读：`rt.py poc search --verified --json`（元数据面：title/code/
  category/kind/severity；search 结果不含 POC 正文）；
- POC 正文/利用细节零复制：卡片 content 只用元数据句式（类别/类型/严重级/
  验证状态 + RTIS 引用码），R2 红线「POC 正文不进 RIP3 库」；
- 写入走公开接口：rp-kc create 子进程（不直写 PG）；隔离 workspace
  ws-bench，播种前清场（可重跑幂等）；
- qrel 显式声明（脚本内 match 规则 → 相关卡 ID），不用检索结果自判。

用法（仓库根）：
  RP_PG_DSN=... python3 scripts/p4_4_bench_seed.py [--rt-bin PATH] [--kc-bin PATH]
输出：
  rip-data/bench/qrel.json（查询集 + 相关卡 ID；每次播种重新生成）
"""

from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
RT_BIN = REPO / "integrations/rtis/rt.py"
KC_BIN = REPO / "target/debug/rp-kc"
QREL_OUT = REPO / "rip-data/bench/qrel.json"
WORKSPACE = "ws-bench"

# 固定合成集（R2 6.8 辅助面；通用示例，无真实目标；覆盖版本差异与章节形态）。
SYNTHETIC = [
    {"title": "BENCH-VER 路径遍历组件 4.2.0 修复差异分析", "type": "vuln_principle",
     "content": "版本差异：组件 4.2.0 前的路径遍历校验缺陷在 4.2.0 修复，旧版本特征与升级判断要点。"},
    {"title": "BENCH-VER 反序列化组件 2.8.11 白名单差异", "type": "vuln_principle",
     "content": "版本差异：反序列化组件 2.8.11 引入类型白名单，此前版本还原对象时缺少类型校验。"},
    {"title": "BENCH-VER 模板引擎 3.0.2 沙箱逃逸差异", "type": "vuln_principle",
     "content": "版本差异：模板引擎 3.0.2 收紧沙箱表达式，旧版本模板注入面更宽。"},
    {"title": "BENCH-CH 第 3 章 输入校验与编码", "type": "security_technique",
     "content": "章节：输入校验与编码。白名单校验、上下文相关输出编码与参数化查询。"},
    {"title": "BENCH-CH 第 5 章 会话与认证控制", "type": "security_technique",
     "content": "章节：会话与认证控制。会话固定防御、认证失败限速与多因素验证。"},
    {"title": "BENCH-CH 第 7 章 文件处理与上传安全", "type": "security_technique",
     "content": "章节：文件处理与上传安全。类型校验、存储隔离与执行权限剥离。"},
    {"title": "BENCH-MIX file upload 校验方法（中英混合）", "type": "detection_method",
     "content": "file upload 检测：MIME 类型伪造、扩展名黑名单绕过与内容嗅探（magic bytes）校验点。"},
    {"title": "BENCH-MIX 服务端请求伪造 SSRF 排查面", "type": "detection_method",
     "content": "SSRF 检测：出网请求点枚举、内网地址探测与协议白名单缺失（gopher/dict/file）。"},
]

# 查询集（R2 6.8 六形态；match 规则生成 qrel，不含真实目标名）。
QUERIES = [
    {"query": "info-leak", "match": {"category": "info-leak"}},
    {"query": "auth-bypass", "match": {"category": "auth-bypass"}},
    {"query": "unauthorized", "match": {"category": "unauthorized"}},
    {"query": "ssrf", "match": {"category": "ssrf"}},
    {"query": "敏感信息泄露排查", "match": {"category": "info-leak"}},
    {"query": "绕过登录验证", "match": {"category": "auth-bypass"}},
    {"query": "未授权访问检测", "match": {"category": "unauthorized"}},
    {"query": "file upload 上传校验", "match": {"category": "file-upload"}},
    {"query": "4.2.0 版本路径遍历差异", "match": {"title_prefix": "BENCH-VER 路径遍历"}},
    {"query": "输入校验与编码 章节", "match": {"title_prefix": "BENCH-CH 第 3 章"}},
]


def run(cmd: list[str], ok_codes=(0,)) -> str:
    p = subprocess.run(cmd, capture_output=True, text=True)
    if p.returncode not in ok_codes:
        raise RuntimeError(f"cmd failed ({p.returncode}): {' '.join(cmd[:4])}...\n{p.stderr[-500:]}")
    return p.stdout


def wipe_workspace() -> None:
    dsn = os.environ.get("RP_PG_DSN", "postgres://postgres:postgres@127.0.0.1:5432/postgres")
    for sql in [
        "DELETE FROM knowledge_review_log WHERE knowledge_id IN (SELECT knowledge_id FROM knowledge_cards WHERE workspace_id = 'ws-bench')",
        "DELETE FROM knowledge_relations WHERE src_id IN (SELECT knowledge_id FROM knowledge_cards WHERE workspace_id = 'ws-bench') OR dst_id IN (SELECT knowledge_id FROM knowledge_cards WHERE workspace_id = 'ws-bench')",
        "DELETE FROM knowledge_card_vectors WHERE card_id IN (SELECT knowledge_id FROM knowledge_cards WHERE workspace_id = 'ws-bench')",
        "DELETE FROM knowledge_cards WHERE workspace_id = 'ws-bench'",
    ]:
        run(["psql", dsn, "-tAc", sql])


def create_card(title: str, ctype: str, content: str) -> str:
    out = run([str(KC_BIN), "create", "--type", ctype, "--title", title,
               "--summary", "P4-4 benchmark 语料",
               "--content", content, "--workspace", WORKSPACE])
    return json.loads(out)["knowledge_id"]


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--limit", type=int, default=200, help="RTIS verified 上限")
    args = ap.parse_args()

    print(f"== 1. 清场 {WORKSPACE}")
    wipe_workspace()

    print("== 2. RTIS verified 元数据（只读）")
    raw = run([sys.executable, "-I", str(RT_BIN), "poc", "search", "--verified",
               "--limit", str(args.limit), "--json"])
    items = json.loads(raw)["results"]
    print(f"   RTIS verified: {len(items)}")

    print("== 3. 播种（RTIS 元数据面 + 合成集）")
    cards: list[dict] = []  # {id, title, category, source}
    for it in items:
        title = str(it.get("title") or it.get("code") or f"RTIS-{it.get('id')}")
        category = str(it.get("category") or "other")
        kind = str(it.get("kind") or "poc")
        severity = str(it.get("severity") or "unknown")
        content = (f"RTIS 引用：{it.get('code')}。类别 {category}，类型 {kind}，"
                   f"严重级 {severity}，验证状态已验证。检测面与前置条件以 RTIS 库为准"
                   "（本卡片仅元数据面，不含利用细节）。")
        cid = create_card(title, "vuln_principle", content)
        cards.append({"id": cid, "title": title, "category": category, "source": "rtis"})
    for s in SYNTHETIC:
        cid = create_card(s["title"], s["type"], s["content"])
        cards.append({"id": cid, "title": s["title"], "category": "synthetic", "source": "synthetic"})
    print(f"   卡片创建：{len(cards)}")

    print("== 4. 计分 + 批准（审核双闸：approve 前必须先 score，否则地板闸全跳）")
    for c in cards:
        run([str(KC_BIN), "score", "--card", c["id"]])
    out = run([str(KC_BIN), "batch-review", "--decision", "approve",
               "--status", "DISCOVERED", "--workspace", WORKSPACE,
               "--reason", "P4-4 benchmark 播种", "--reviewer", "p4-4-bench"])
    approved = json.loads(out).get("applied", [])
    print(f"   批准：{len(approved)}/{len(cards)}")
    if len(approved) < len(cards):
        raise RuntimeError(f"approve 覆盖不全：{len(approved)}/{len(cards)}（硬标记/地板闸跳过面须清零）")

    print("== 5. 嵌入补向量（embed-cards）")
    out = run([str(KC_BIN), "embed-cards", "--workspace", WORKSPACE])
    emb = json.loads(out)
    print(f"   embedded={emb.get('embedded')} skipped={emb.get('skipped')}")

    print("== 6. qrel 生成")
    qrel = []
    for q in QUERIES:
        m = q["match"]
        rel = [c["id"] for c in cards
               if (m.get("category") and c["category"] == m["category"] and c["source"] == "rtis")
               or (m.get("title_prefix") and c["title"].startswith(m["title_prefix"]))]
        qrel.append({"query": q["query"], "relevant": rel})
        print(f"   {q['query']!r}: {len(rel)} 相关卡")
    QREL_OUT.parent.mkdir(parents=True, exist_ok=True)
    QREL_OUT.write_text(json.dumps(qrel, ensure_ascii=False, indent=2))
    print(f"== qrel 落盘：{QREL_OUT}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
