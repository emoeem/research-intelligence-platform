#!/usr/bin/env python3
"""R14 — 婚恋小程序（wx788f5daad54eb854）请求签名链离线复现工具

签名算法逐字符还原自真机取出的 wxapkg（v922）解包产物
`app-service.js :: api/core/apiSecret.js`：

    exports.sign = function(method, "/"+path, header, params, type, secret){
        var u = (type === "json") ? JSONStringify(params) : qsStringifyWithSort(params);
        ["Platform","AppVersion","Nonce","VersionCode","Channel","TDID"].forEach(function(k){
            u += "&" + k + "=" + (header[k] || "");       // 缺失的 key 退化为空串
        });
        var a = [method, "/"+path];
        if (u) a.push(u);
        var o = a.join("&");
        var c = HmacSHA1(o, secret);                       // crypto-js, key 按 UTF-8 处理
        return { sign: Base64.stringify(c), signStr: o };
    };

    qsStringifyWithSort = keys.sort().map(k => `${k}=${params[k]}`).join("&")   // 不做 URL 编码

secret ��源（同文件）：
    APISecret.getSecretInfo() → expired ? wx.login() → GET config/get_sign?code=<code>
                              : wx.getBackgroundFetchData({fetchType:"pre"})
    响应体：{"code":0,"data":{"sign":"<32 hex>","sign_time":<秒>}}
    →  data.sign 就是 secret（字段名不叫 secret，这是最容易漏掉的一点）

请求侧还要带：Sign=<base64>、SignType=2(Temp)/1(Prefetch)、SignTime=<sign_time>、
              Nonce=<毫秒时间戳，参与签名>、Platform、AppVersion

用法
----
1) 离线自校验：拿已捕获的真实请求批量重算，与请求头里的 Sign 逐字节比对
   python3 sign-recompute.py verify <capture_dir> <secret>

2) 现场伪造签名（拿到有效 secret 之后）：
   python3 sign-recompute.py sign GET /config/theme <secret> platform=1 keyword=xx
   python3 sign-recompute.py sign POST /user/get_user_info <secret> platform=1

3) 直接把伪造好的头部打出去（只读用法）：
   python3 sign-recompute.py curl POST /user/get_user_info <secret> <jwt> platform=1
"""
import json, base64, hmac, hashlib, sys, os, glob, time, subprocess
from urllib.parse import urlparse, parse_qsl

SIGN_HEADER_KEYS = ["Platform", "AppVersion", "Nonce", "VersionCode", "Channel", "TDID"]
APP_VERSION = "5.62.1"
PLATFORM = "1"


def qs_sorted(params):
    """qsStringifyWithSort：键排序、k=v、不编码、& 连接"""
    return "&".join(f"{k}={params[k]}" for k in sorted(params.keys()))


def signstr(method, path, header, params, type_=""):
    u = (json.dumps(params, separators=(",", ":"), ensure_ascii=False)
         if type_ == "json" else qs_sorted(params))
    for k in SIGN_HEADER_KEYS:
        u += "&" + k + "=" + (header.get(k) or "")
    a = [method.upper(), "/" + path.lstrip("/")]
    if u:
        a.append(u)
    return "&".join(a)


def calc(method, path, header, params, secret, type_=""):
    s = signstr(method, path, header, params, type_)
    mac = hmac.new(secret.encode(), s.encode(), hashlib.sha1).digest()
    return base64.b64encode(mac).decode(), s


def mk_header(nonce=None, platform=PLATFORM, appversion=APP_VERSION, extra=None):
    h = {"Platform": platform, "AppVersion": appversion,
         "Nonce": str(nonce if nonce is not None else int(time.time() * 1000))}
    if extra:
        h.update(extra)
    return h


# ---------------- verify ----------------
def cmd_verify(capture_dir, secret):
    rows, hits = [], 0
    for f in sorted(glob.glob(os.path.join(capture_dir, "rec_*.json"))):
        try:
            d = json.load(open(f))
        except Exception:
            continue
        hs = {h["name"].lower(): h["value"] for h in d.get("request", {}).get("headers", [])}
        if "sign" not in hs:
            continue
        u = urlparse(d["url"])
        path, method = u.path, d["request"]["method"]
        hdr = {"Platform": hs.get("platform", ""), "AppVersion": hs.get("appversion", ""),
               "Nonce": hs.get("nonce", "")}
        qd = dict(parse_qsl(u.query, keep_blank_values=True))
        body = d["request"].get("body")
        bt = body.get("text") if isinstance(body, dict) else (body if isinstance(body, str) else "")
        cands = [("empty", {"platform": hdr["Platform"]})]
        if qd and "crypto_data" not in qd:                    # 明文 GET 参数可见
            p = dict(qd); p.setdefault("platform", hdr["Platform"]); cands.append(("query", p))
        if bt and "crypto_data" not in bt:                    # 明文 POST 表单
            p = dict(parse_qsl(bt, keep_blank_values=True)); p.setdefault("platform", hdr["Platform"])
            cands.append(("body", p))
        for tag, params in cands:
            s, sstr = calc(method, path, hdr, params, secret)
            if s == hs["sign"]:
                hits += 1
                rows.append({"file": os.path.basename(f), "tag": tag, "method": method,
                             "path": path, "nonce": hdr["Nonce"], "signtime": hs.get("signtime"),
                             "signtype": hs.get("signtype"), "signStr": sstr, "sign": s})
                break
    print(json.dumps({"tested": sum(1 for _ in glob.glob(os.path.join(capture_dir, "rec_*.json"))),
                      "verified_hits": hits}, ensure_ascii=False))
    for r in rows:
        print(json.dumps(r, ensure_ascii=False))
    return rows


# ---------------- sign ----------------
def cmd_sign(method, path, secret, kv):
    params = dict(x.split("=", 1) for x in kv)
    hdr = mk_header()
    s, sstr = calc(method, path, hdr, params, secret)
    out = {"Sign": s, "SignType": "2", "Nonce": hdr["Nonce"],
           "Platform": hdr["Platform"], "AppVersion": hdr["AppVersion"],
           "signStr": sstr, "params": params}
    print(json.dumps(out, ensure_ascii=False, indent=1))
    return out


def cmd_curl(method, path, secret, jwt, kv):
    params = dict(x.split("=", 1) for x in kv)
    hdr = mk_header()
    s, sstr = calc(method, path, hdr, params, secret)
    body = "&".join(f"{k}={v}" for k, v in params.items())
    url = f"https://vauqtic.afunapp.com{path}"
    if method.upper() == "GET":
        url += "?" + body
        data = []
    else:
        data = ["--data-raw", body]
    args = ["curl", "-sk", "-4", "--compressed", "--max-time", "25", url,
            "-H", "Host: vauqtic.afunapp.com", "-H", f"sign: {s}",
            "-H", f"signtype: 2", "-H", f"nonce: {hdr['Nonce']}",
            "-H", f"platform: {PLATFORM}", "-H", f"appversion: {APP_VERSION}",
            "-H", "charset: utf-8", "-H", "content-type: application/x-www-form-urlencoded",
            "-H", f"authorization: {jwt}",
            "-H", "referer: https://servicewechat.com/wx788f5daad54eb854/922/page-frame.html"]
    if data:
        args += data
    print("# " + " ".join(a if a != jwt else "<JWT>" for a in args), file=sys.stderr)
    subprocess.run(args)


if __name__ == "__main__":
    if len(sys.argv) < 2 or sys.argv[1] in ("-h", "--help"):
        print(__doc__)
        sys.exit(0)
    cmd = sys.argv[1]
    if cmd == "verify":
        cmd_verify(sys.argv[2], sys.argv[3])
    elif cmd == "sign":
        cmd_sign(sys.argv[2], sys.argv[3], sys.argv[4], sys.argv[5:])
    elif cmd == "curl":
        cmd_curl(sys.argv[2], sys.argv[3], sys.argv[4], sys.argv[5], sys.argv[6:])
    else:
        print(__doc__)
