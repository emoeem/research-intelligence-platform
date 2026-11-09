#!/usr/bin/env python3
# 站群/SPA 落地页「开放重定向」普查方法论 —— 唯一标记法 + 无点载荷 + 只认 Location/JS 数据流
# 适用：入口站 → 中枢 → 落地页 三层导流站群、hex2bin 混淆落地页、有 A/B 模板轮换的目标
import re, time, binascii, requests, urllib3
# ---- 参数守卫（无参数必须给出人话，不抛 traceback、不朝真实目标发包）----
import sys as _sys, os as _os
if len(_sys.argv) < 2:
    _u = '<参数...>'
    print("用法: " + _os.path.basename(__file__) + " " + _u, file=_sys.stderr)
    raise SystemExit(1)
# ---- 守卫结束 ----

urllib3.disable_warnings()
PROXY = {"http": "http://127.0.0.1:7899", "https": "http://127.0.0.1:7899"}  # 必须能出境外

PARAMS = ["url","redirect","redirect_uri","redirect_url","return","returnUrl","returl","next","goto","go",
 "out","outurl","jump","to","r","u","l","v","f","p","callback","backurl","continue","forward","href",
 "domain","site","host","src","source","file","path","open","openUrl","share","shareUrl","tg","checkin",
 "_c","_did","channel","code","id","appid","sign","token","target","link","dest","destination","redirurl",
 "jumpurl","jump_url","ret","from","ref","referer","ref_url","callback_url","cb","dl","download","apk",
 "ios","android","pf","platform","os","cid","page_id","landing","lc","ch","chn","campaign","ad","adid",
 "utm_source","hostname"]

HEXRE = re.compile(r"hex2bin\(\s*['\"]([0-9a-fA-F\s]+)['\"]\s*\)")
def decode(txt):
    out = []
    for m in HEXRE.finditer(txt or ""):
        try: out.append(binascii.unhexlify(re.sub(r"\s+", "", m.group(1))).decode("utf-8", "replace"))
        except Exception: pass
    return "\n".join(out)

def probe(base, s=None):
    s = s or requests.Session(); s.proxies = PROXY
    s.headers["User-Agent"] = "Mozilla/5.0 (iPhone; CPU iPhone OS 17_0 like Mac OS X) Mobile/15E148"
    sep = "&" if "?" in base else "?"

    # ---- 步骤 0：基线 ----
    r = s.get(base, allow_redirects=False, timeout=25, verify=False)
    if r.headers.get("Location"): return {"baseline_location": r.headers["Location"]}

    # ---- 步骤 1：唯一标记法（一发定位哪些参数被读）----
    mk = {k: f"zq{i:02d}" for i, k in enumerate(PARAMS, 1)}
    hit = {}
    for tag, order in (("fwd", PARAMS), ("rev", list(reversed(PARAMS)))):
        q = sep + "&".join(f"{k}={mk[k]}" for k in order)
        rr = s.get(base + q, allow_redirects=False, timeout=25, verify=False)
        if rr.headers.get("Location"): return {"marker_location": rr.headers["Location"]}
        body = rr.text + "\n" + decode(rr.text)
        for z in set(re.findall(r"zq[0-9]{2}", body)):
            hit.setdefault([k for k, v in mk.items() if v == z][0], []).append(tag)
        time.sleep(0.45)
    # 注意：某标记在**基线页**里本就存在 → 假命中，必须用单参数反证
    for k in list(hit):
        r2 = s.get(f"{base}{sep}{k}=qmarker_{k}", allow_redirects=False, timeout=25, verify=False)
        if f"qmarker_{k}" not in (r2.text + decode(r2.text)):
            hit.pop(k)          # 反证：不反射 → 不是它读的
        time.sleep(0.45)

    # ---- 步骤 2：路径位/平台位逃逸 ----
    path_hits = {}
    for p in ["//example.com", "https://example.com", "%2f%2fexample.com",
              "..%2f..%2fexample.com", "%09//example.com", "@example.com"]:
        stem = base.rsplit("/", 1)[0]
        u = f"{stem}/{requests.utils.quote(p, safe='')}"
        rr = s.get(u, allow_redirects=False, timeout=25, verify=False)
        loc = rr.headers.get("Location")
        if loc:
            path_hits[p] = {"loc": loc, "external": "example.com" in loc,
                            "relative": loc.startswith("/")}
        time.sleep(0.4)

    # ---- 步骤 3：静态数据流 ----
    js = decode(r.text)
    prim = {p: len(re.findall(p, js)) for p in
            [r"location\.href", r"window\.open", r"location\.replace", r"openUrl", r"openLink", r"atob\("]}
    return {"marker_hits": hit, "path_hits": path_hits, "js_primitives": prim}

# 判定口径（唯一合法判据，别用长度/md5——这批站有 A/B 轮换 + 随机 trace_id）：
#   HIT  : Location 的 host 是外部域，且换第二个外部域仍可控
#   NO   : Location 缺失；或 Location 是相对路径（/ 开头，结构上离不开本 host）
#   注意 : 判定「是否反射」的载荷必须不带 "." —— 站群常把 code/channel 截断于首个点，
#          带点载荷（https://example.com）会被截成 https://example 造成假阴性。
    import sys, json
    print(json.dumps(probe(sys.argv[1]), ensure_ascii=False, indent=1))
