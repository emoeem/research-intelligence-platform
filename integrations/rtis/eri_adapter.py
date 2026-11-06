#!/usr/bin/env python3
"""ERI 统一 Resolve 网关（M5 adapter）。

契约与 eri_p2.py 完全一致（bridge-gate --task-file <json>，stdout=JSON），
供 rtis.py _eri_external_review() 在 RTIS_RESOLVER_MODE=rp 时前置调用。

解析顺序（--mode auto，生产默认）：
  (a) rp-resolve CLI（新平台 resource-engine，字段级兼容 eri resolve 输出）
  (b) 回落 eri_p2.py bridge-gate（legacy 现行为）
  (c) 再失败返回 status=error 的 external_resource_review（绝不抛异常）

--mode rp     仅走 (a)；--mode legacy 仅走 (b)；缺省 auto 走 a->b->c。
输出：{"external_resource_review": {...9 字段与 legacy 逐字段兼容...},
       "adapter": {"resolver_used": ..., "attempts": [...]}}

配置（环境变量）：
  RTIS_RP_RESOLVE           rp-resolve 二进制路径（默认依次尝试 target/debug、target/release）
  RTIS_RP_DB                平台库路径（默认 migrations/platform-dryrun.db）
  RTIS_ERI_RESOLVER         legacy eri_p2.py 路径（与 rtis.py 同一默认）
  RTIS_ERI_ADAPTER_TIMEOUT  单次子进程超时秒数（默认 5）
  RTIS_ERI_ADAPTER_AUDIT    审计 JSONL 路径，"off" 关闭（默认本目录 adapter_audit.jsonl）

stdlib-only；子进程超时 5s，失败单次重试（每路最多 2 次尝试）。
"""

import json
import os
import subprocess
import sys
import tempfile
import time
from datetime import datetime, timezone

RT_PLATFORM = os.path.join(os.path.expanduser("~"), "code", "pensting", "research-platform")
# M8 切换（2026-11-06）：生产库 platform.db（verify 100% 快照批），dryrun 库保留为对照。
RP_DB_DEFAULT = os.path.join(RT_PLATFORM, "migrations", "platform.db")
ERI_P2_DEFAULT = os.path.join(os.path.expanduser("~"), "code", "pensting",
                              "security-research-upgrade", "eri", "eri_p2.py")
AUDIT_DEFAULT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "adapter_audit.jsonl")

# external_resource_review 的 9 个规范字段（与 eri_p2.bridge_gate 输出一致）
REVIEW_FIELDS = ("status", "query_id", "candidate_count", "recommended_resources",
                 "rejected_resources", "already_covered_by_internal", "capability_gaps",
                 "detailed_research_required", "reason")


def now_iso() -> str:
    return datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


def _timeout() -> float:
    try:
        return max(1.0, float(os.environ.get("RTIS_ERI_ADAPTER_TIMEOUT", "5")))
    except ValueError:
        return 5.0


def _find_rp_binary() -> str | None:
    cand = os.environ.get("RTIS_RP_RESOLVE")
    if cand:
        return cand if os.path.exists(cand) else None
    for rel in ("target/debug/rp-resolve", "target/release/rp-resolve"):
        p = os.path.join(RT_PLATFORM, rel)
        if os.path.exists(p):
            return p
    return None


CJK_BRIDGE = {
    "落地页": ["landing", "redirect"], "跳转": ["redirect"], "资产": ["asset", "discovery"],
    "加密": ["crypto", "encryption"], "密文": ["crypto", "encryption"],
    "小程序": ["miniapp", "wechat"], "边缘": ["cdn", "edge"], "代理": ["proxy"],
    "移动": ["mobile", "android"], "安卓": ["android", "apk"], "接口": ["api"],
    "渗透": ["pentest"], "溯源": ["osint"], "口令": ["password"], "密码": ["password"],
    "端口": ["scan", "recon"], "跳板": ["tunnel", "pivot"], "隧道": ["tunnel"],
}


def _split_phrase(text: str) -> list:
    """整句任务目标 -> 确定性检索词：ASCII 词元 + CJK 桥表（与 eri_p2 同表同步）。"""
    import re as _re
    out = [t for t in _re.findall(r"[a-zA-Z0-9+#.]{2,}", text or "") if len(t) >= 2]
    joined = text or ""
    for cjk, en in CJK_BRIDGE.items():
        if cjk in joined:
            out.extend(en)
    return sorted(set(w.lower() for w in out))


def derive_keywords(task: dict) -> list:
    """与 eri_p2.bridge_gate 一致的关键词推导（keywords + objective + 画像维度值），
    另加 M8 修正：整句 objective/维度值经确定性拆词，避免整句 LIKE 预筛 no_match。"""
    profile = task.get("profile") or {}
    dims = profile.get("dims") or profile.get("dimensions") or {}
    dim_words = []
    for v in dims.values():
        val = v.get("value") if isinstance(v, dict) else v
        if isinstance(val, str) and val:
            dim_words.append(val)
        elif isinstance(val, list):
            dim_words.extend(str(x) for x in val if x)
    objective = str(task.get("objective") or "")
    base = list(task.get("keywords") or []) + dim_words
    base = [k for k in base if k and len(k) <= 24]        # 短词直用；长句拆词
    for phrase in [objective] + [w for w in dim_words if len(w) > 24]:
        base.extend(_split_phrase(phrase))
    out = []
    for k in base:
        k = str(k).strip()
        if k and k not in out:
            out.append(k)
    return out[:16]


def run_with_retry(cmd: list, timeout: float, retries: int = 1) -> dict:
    """跑子进程并解析 stdout JSON。失败（超时/非零退出/坏 JSON）单次重试。

    返回 {"ok": True, "data": ...} 或 {"ok": False, "error": ...}，绝不抛异常。
    """
    last_err = ""
    for attempt in range(retries + 1):
        try:
            out = subprocess.run(cmd, capture_output=True, text=True, timeout=timeout)
            if out.returncode != 0:
                last_err = f"rc={out.returncode} stderr={out.stderr.strip()[:120]}"
            else:
                data = json.loads(out.stdout or "{}")
                return {"ok": True, "data": data}
        except subprocess.TimeoutExpired:
            last_err = f"timeout after {timeout}s"
        except Exception as e:  # noqa: BLE001
            last_err = repr(e)[:200]
    return {"ok": False, "error": last_err}


def resolve_via_rp(task: dict, timeout: float) -> dict:
    """(a) rp-resolve 路径：把 eri resolve 形状转成 external_resource_review。"""
    binp = _find_rp_binary()
    if not binp:
        return {"ok": False, "error": "rp-resolve binary not found"}
    db = os.environ.get("RTIS_RP_DB") or RP_DB_DEFAULT
    if db != ":memory:" and not os.path.exists(db):
        return {"ok": False, "error": f"rp db missing: {db}"}
    kws = derive_keywords(task)
    cmd = [binp, "--db", db, "--keywords", ",".join(kws), "--limit", "5",
           "--task-id", str(task.get("task_id") or "adhoc")]
    r = run_with_retry(cmd, timeout)
    if not r["ok"]:
        return r
    return {"ok": True, "data": review_from_resolve(r["data"])}


def review_from_resolve(data: dict) -> dict:
    """eri resolve 结果 -> external_resource_review（与 bridge_gate 相同的 9 字段与二分规则）。"""
    matched = data.get("matched_resources") or []
    recommended, rejected = [], []
    for r in matched:
        if r.get("grade") in ("A", "B") and r.get("adoptable_as_dependency"):
            recommended.append(r["resource_id"])
        else:
            rejected.append({"resource_id": r["resource_id"],
                             "reason": r.get("license_note") or f"grade={r.get('grade')}"})
    gaps = data.get("capability_gaps") or []
    return {
        "status": data.get("search_status") or "no_match",
        "query_id": data.get("query_id"),
        "candidate_count": (data.get("retrieval_stats") or {}).get("returned", 0),
        "recommended_resources": recommended,
        "rejected_resources": rejected,
        "already_covered_by_internal": data.get("already_covered_by_internal") or [],
        "capability_gaps": gaps,
        "detailed_research_required": bool(gaps),
        "reason": "存在未映射的任务相关资源，需按需深读" if gaps
                  else "内部能力已覆盖或无显著相关资源",
    }


def resolve_via_legacy(task_file: str, timeout: float) -> dict:
    """(b) legacy 路径：eri_p2.py bridge-gate（与 rtis.py 原行为同一子进程调用）。"""
    resolver = os.environ.get("RTIS_ERI_RESOLVER") or ERI_P2_DEFAULT
    if not os.path.exists(resolver):
        return {"ok": False, "error": f"legacy resolver missing: {resolver}"}
    cmd = [sys.executable, "-I", resolver, "bridge-gate", "--task-file", task_file]
    r = run_with_retry(cmd, timeout)
    if not r["ok"]:
        return r
    review = (r["data"] or {}).get("external_resource_review")
    if not isinstance(review, dict):
        return {"ok": False, "error": "legacy output missing external_resource_review"}
    return {"ok": True, "data": review}


def error_review(reason: str) -> dict:
    return {
        "status": "error", "query_id": None, "candidate_count": 0,
        "recommended_resources": [], "rejected_resources": [],
        "already_covered_by_internal": [], "capability_gaps": [],
        "detailed_research_required": False,
        "reason": f"rp 与 legacy resolver 均不可用: {reason}",
    }


def _audit(record: dict) -> None:
    path = os.environ.get("RTIS_ERI_ADAPTER_AUDIT", AUDIT_DEFAULT)
    if path == "off":
        return
    try:
        with open(path, "a", encoding="utf-8") as f:
            f.write(json.dumps(record, ensure_ascii=False) + "\n")
    except Exception:  # noqa: BLE001
        pass


def bridge_gate(task: dict, mode: str = "auto") -> dict:
    """统一入口：a(rap)->b(legacy)->c(error) / 指定单路。输出 adapter 顶层信封。"""
    t0 = time.perf_counter()
    timeout = _timeout()
    attempts, review, used = [], None, None
    order = {"auto": ["rp", "legacy"], "rp": ["rp"], "legacy": ["legacy"]}.get(mode, ["rp", "legacy"])

    tmp = None
    try:
        if "legacy" in order:
            fd, tmp = tempfile.mkstemp(suffix=".json")
            with os.fdopen(fd, "w", encoding="utf-8") as f:
                json.dump(task, f, ensure_ascii=False)

        for path_kind in order:
            if path_kind == "rp":
                r = resolve_via_rp(task, timeout)
            else:
                r = resolve_via_legacy(tmp, timeout)
            attempts.append({"path": path_kind, "ok": r["ok"], "error": r.get("error")})
            if r["ok"]:
                review, used = r["data"], path_kind
                break
        if review is None:
            err = "; ".join(a["error"] or "?" for a in attempts)
            review, used = error_review(err), "error"
    except Exception as e:  # noqa: BLE001
        review, used = error_review(repr(e)[:200]), "error"
    finally:
        if tmp and os.path.exists(tmp):
            try:
                os.unlink(tmp)
            except OSError:
                pass

    elapsed_ms = round((time.perf_counter() - t0) * 1000, 1)
    _audit({"at": now_iso(), "mode": mode, "resolver_used": used,
            "status": review.get("status"), "elapsed_ms": elapsed_ms,
            "attempts": attempts})
    return {"external_resource_review": review,
            "adapter": {"resolver_used": used, "mode": mode,
                        "elapsed_ms": elapsed_ms, "attempts": attempts}}


def main() -> None:
    import argparse
    p = argparse.ArgumentParser(prog="eri_adapter.py", description="ERI 统一 Resolve 网关（M5）")
    sub = p.add_subparsers(dest="cmd", required=True)
    bg = sub.add_parser("bridge-gate", help="与 eri_p2 bridge-gate 同契约")
    bg.add_argument("--task-file", required=True)
    bg.add_argument("--mode", default="auto", choices=["auto", "rp", "legacy"])
    args = p.parse_args()
    if args.cmd == "bridge-gate":
        try:
            with open(args.task_file, encoding="utf-8") as f:
                task = json.load(f)
        except Exception as e:  # noqa: BLE001
            task = {}
            env = {"external_resource_review": error_review(f"bad task-file: {repr(e)[:120]}"),
                   "adapter": {"resolver_used": "error", "mode": args.mode,
                               "elapsed_ms": 0.0, "attempts": []}}
            print(json.dumps(env, ensure_ascii=False))
            return
        print(json.dumps(bridge_gate(task, mode=args.mode), ensure_ascii=False))


if __name__ == "__main__":
    main()
