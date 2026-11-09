#!/usr/bin/env python3
"""anon handler 存在性 oracle 枚举器

原理：目标用了统一响应体 {"code":..,"msg":..,"path":..,"ok":..}，HTTP 恒 200，
业务码才是判据。据此可把「handler 是否存在」变成三值判定，无需字典命中也能
确认路径存在：

  code=400 "请求方式不支持"        → handler 存在（只是 method 不对）
  code=500 "System error!"         → handler 不存在（全局异常兜底）
  code=200 + data                  → handler 存在且返回数据（命中！）
  code=401/403                     → 被网关/租户闸门拦（不在白名单）

⚠️ 关键坑：不要用 HTTP status 判定。该 API 所有响应（含 500 System error）
   的 HTTP 码都是 200，用 %{http_code} 判会把全部「不存在」误判为命中
   （本次 167 条字典全部落入此坑，一度显示 166 hits，实为 0）。

用法: ./anon_oracle_enum.py <DOMAIN> <PLATFORM_ID> <svc> <dict.txt> [prefix]
  prefix 默认 anon；app 侧接口用 anon/app
"""
import sys, json, time, urllib.request, urllib.error
# ---- 参数守卫（无参数必须给出人话，不抛 traceback、不朝真实目标发包）----
import sys as _sys, os as _os
if len(_sys.argv) < 2:
    _u = '用法: ./anon_oracle_enum.py <DOMAIN> <PLATFORM_ID> <svc> <dict.txt> [prefix]'
    print("用法: " + _os.path.basename(__file__) + " " + _u, file=_sys.stderr)
    raise SystemExit(1)
# ---- 守卫结束 ----


DOMAIN, PID, SVC, DICT = sys.argv[1], sys.argv[2], sys.argv[3], sys.argv[4]
PREFIX = sys.argv[5] if len(sys.argv) > 5 else "anon"
BASE = f"https://{DOMAIN}/api/{SVC}/{PREFIX}"
HDRS = {
    "Host": DOMAIN, "X-Login-Type": PID, "x-req-domain": DOMAIN,
    "X-Platform-Id": PID, "X-Currency": "CNY", "Language": "zh",
    "X-Device-Code": "oracle-probe", "User-Agent": "curl/8.5.0", "Accept": "*/*",
}
HITS, EXIST, MISS = [], [], 0

def probe(name):
    req = urllib.request.Request(f"{BASE}/{name}", headers=HDRS)
    try:
        with urllib.request.urlopen(req, timeout=10) as r:
            raw = r.read()
    except urllib.error.HTTPError as e:
        raw = e.read()
    except Exception as e:
        return "ERR", str(e)[:80]
    try:
        j = json.loads(raw)
    except Exception:
        return "NONJSON", raw[:100].decode("utf-8", "ignore")
    code = j.get("code"); msg = (j.get("msg") or "")
    if code == 200 and j.get("data") is not None:
        return "DATA", json.dumps(j.get("data"), ensure_ascii=False)[:200]
    if code == 400:
        return "EXISTS", msg[:100]
    if code in (401, 403):
        return "DENIED", msg[:80]
    if code == 500 and "System error" in msg:
        return "MISS", msg
    return "OTHER", f"code={code} msg={msg[:100]}"

for line in open(DICT, encoding="utf-8", errors="ignore"):
    e = line.strip()
    if not e or e.startswith("#"):
        continue
    kind, detail = probe(e)
    if kind == "DATA":
        HITS.append((e, detail)); print(f"[DATA]   {SVC}/{PREFIX}/{e} :: {detail}")
    elif kind == "EXISTS":
        EXIST.append((e, detail)); print(f"[EXISTS] {SVC}/{PREFIX}/{e} :: {detail}")
    elif kind == "OTHER":
        print(f"[OTHER]  {SVC}/{PREFIX}/{e} :: {detail}")
    elif kind == "MISS":
        MISS += 1
    time.sleep(0.5)   # 2 req/s 硬约束

print(f"\n# {SVC}/{PREFIX}: DATA={len(HITS)} EXISTS(400)={len(EXIST)} MISS={MISS}", file=sys.stderr)

if __name__ == "__main__":
    # ---- 参数守卫（无参数必须给出人话，不抛 traceback、不朝真实目标发包）----
    if len(sys.argv) < 4:
        import os as _os
        print("用法: " + _os.path.basename(__file__) + " 用法: ./anon_oracle_enum.py <DOMAIN> <PLATFORM_ID> <svc> <dict.txt> [prefix]\n详见文件头 docstring。", file=sys.stderr)
        raise SystemExit(1)
