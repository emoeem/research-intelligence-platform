#!/usr/bin/env python3
"""
Flutter APK 密钥/签名/内网地址 全自动提取器
用法: python3 flutter_apk_secrets.py <app.apk> <outdir>

设计要点（踩过的坑）:
 1. blutter 的 indir 参数必须是 **APK 本身** 或 **直接含 libapp.so 的目录**；
    给解包根目录会报 "Cannot find libapp file"。
 2. 不要去 grep 单个文件找密钥——先把全部自有 package 的字符串常量抽成一张表，
    再做二次筛选。逐文件 grep 一定会漏项。
 3. 判断一个 16 字符串是不是密钥，必须看它的上下文：
    base64 大块（证书/公钥/X.509）内部的 16 字符 alnum 串是巧合，不是密钥。
    真正的密钥通常**独立成串**、且出现在加密函数所在文件的字符串池里。
 4. pointycastle 的 AESMode 枚举顺序：cbc=0, cfb64=1, ctr=2, ecb=3,
    ofb64Gctr=4, ofb64=5, sic=6, gcm=7。要确认模式就去看 `Instance_AESMode`
    后面那行的 `Obj!AESMode@0x...`，再去 objs.txt 里对枚举值，别凭函数名猜。
 5. 非 lib/ 目录的资源必须先看：assets/ 里常躺着 *.conf (VPN)、*.json、
    *.html (本地 WebView 壳) —— 这些是 strings libapp.so 拿不到的。
"""
import os, re, subprocess, sys, collections

def sh(cmd, **kw):
    return subprocess.run(cmd, shell=isinstance(cmd, str), capture_output=True, text=True, **kw)

def unpack(apk, out):
    os.makedirs(out, exist_ok=True)
    sh(["unzip", "-q", "-o", apk, "-d", out])
    return out

def low_hanging(out, resdir):
    """assets/ 与 res/xml/ 的高价值静态文件"""
    print("\n=== [1] assets/ 与 res/xml/ 直接读（不经 libapp.so）===")
    hits = []
    for root, _, files in os.walk(out):
        for f in files:
            if re.search(r'\.(conf|cfg|ini|env|pem|key|p12|jks|json|xml|yaml|yml)$', f, re.I):
                p = os.path.join(root, f)
                if os.path.getsize(p) > 512 * 1024:      # 跳过巨大资源
                    continue
                try:
                    t = open(p, encoding="utf-8", errors="ignore").read()
                except Exception:
                    continue
                if re.search(r'(PrivateKey|BEGIN [A-Z ]*PRIVATE KEY|PresharedKey|'
                             r'api_?key|app_?secret|password\s*=|Endpoint\s*=)', t, re.I):
                    print(f"  [!] {p}  ({len(t)}B)")
                    for line in t.splitlines():
                        if re.search(r'(Private|Public|Preshared|Endpoint|Address|api_?key|secret)', line, re.I):
                            print("        ", line.strip()[:120])
                    hits.append(p)
    if not hits:
        print("  (无命中)")
    return hits

def dart_strings(asm_dir, own_pkgs=None):
    """把自有 package 的 dart 字符串池统一抽成一张表"""
    print("\n=== [2] 自有 package 字符串常量汇总 ===")
    pat = re.compile(r'\[pp\+0x[0-9a-f]+\]\s*("(?:[^"\\]|\\.)*")')
    pat2 = re.compile(r'r\d+ = ("(?:[^"\\]|\\.)*")')
    if own_pkgs is None:
        # 启发式：asm 下非知名 pub 包即视为业务包
        known = set("""app_installer app_links archive asn1lib async audio_session auto_size_text
        bloc bot_toast cached_network_image card_swiper characters clock collection
        connectivity_plus cross_file crypto csslib date_format decimal device_info_plus
        dio dotted_border equatable fading_edge_scrollview ffi file file_picker fixnum
        fl_chart flutter flutter_avif flutter_bloc flutter_cache_manager flutter_svg
        go_router html http image image_picker intl logger logging lottie
        material_color_utilities media_kit mime nested path path_provider petitparser
        platform plugin_platform_interface pointycastle protobuf provider pull_to_refresh
        qr rxdart shared_preferences shimmer source_span sqflite synchronized timezone
        tuple url_launcher uuid vector_graphics vector_math web_socket webview_flutter xml
        charcode convert js meta typed_data vector_graphics_codec web wakelock_plus""".split())
        own_pkgs = [d for d in os.listdir(asm_dir)
                    if os.path.isdir(os.path.join(asm_dir, d)) and d not in known]
    total = collections.Counter()
    for pkg in own_pkgs:
        c = collections.Counter()
        for root, _, files in os.walk(os.path.join(asm_dir, pkg)):
            for f in files:
                if not f.endswith(".dart"):
                    continue
                t = open(os.path.join(root, f), encoding="utf-8", errors="replace").read()
                for m in pat.finditer(t):  c[m.group(1)] += 1
                for m in pat2.finditer(t): c[m.group(1)] += 1
        if c:
            print(f"  {pkg}: {len(c)} distinct")
            total.update(c)
    print(f"  ---- 合计 {len(total)} 条，业务包 = {own_pkgs}")
    return total, own_pkgs

def classify(total, resdir):
    os.makedirs(resdir, exist_ok=True)
    blob = "\n".join(total.keys())
    rules = {
        "hosts_urls":  r'"(https?://[^"\\]+)"',
        "ipv4":        r'"(\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}(?::\d+)?)"',
        "private_ip":  r'"((?:10|192\.168|172\.(?:1[6-9]|2\d|3[01]))\.[\d.]+(?:/\d+)?)"',
        "hex32":       r'"([0-9a-fA-F]{32})"',
        "hex64":       r'"([0-9a-fA-F]{64})"',
        "b64keys":     r'"(-----BEGIN [^"\\]*-----)"',
        "crypto_keys": r'"([A-Za-z0-9+/=]{16})"',
        "debug_hints": r'"([^"]*(?:debug|test|uat|staging|dev|internal)[^"]*)"',
    }
    print("\n=== [3] 分类筛选 ===")
    for name, rx in rules.items():
        found = sorted(set(re.findall(rx, blob)))
        # 过滤 pointycastle 曲线参数等噪音
        if name in ("hex32", "hex64"):
            found = [f for f in found
                     if not re.match(r'^(ffffffff|80000000|9b9f605f|a9fb57db|5ac635d8)', f, re.I)]
        p = os.path.join(resdir, f"{name}.txt")
        open(p, "w").write("\n".join(found) + "\n")
        print(f"  {name:12s} {len(found):4d} -> {p}")
        for f in found[:8]:
            print(f"      {f[:100]}")

def run_blutter(apk, outdir):
    print("\n=== [4] blutter（Dart AOT 反汇编）===")
    bt = "/tmp/blutter"
    if not os.path.isdir(bt):
        sh(f"git clone --depth 1 https://github.com/worawit/blutter {bt}")
    r = sh(["python3", os.path.join(bt, "blutter.py"), apk, outdir])
    tail = (r.stdout or "")[-800:] + (r.stderr or "")[-400:]
    print(tail)
    return os.path.join(outdir, "asm")

if __name__ == "__main__":
    if len(sys.argv) < 3:
        print(__doc__); sys.exit(1)
    apk, base = sys.argv[1], sys.argv[2]
    x   = unpack(apk, os.path.join(base, "apk_x"))
    res = os.path.join(base, "apk_strings")
    os.makedirs(res, exist_ok=True)
    low_hanging(x, res)
    asm = run_blutter(apk, os.path.join(base, "blutter_out"))
    if os.path.isdir(asm):
        total, pkgs = dart_strings(asm)
        classify(total, res)
        print("\n=== [5] 下一步人工确认 ===")
        print("  · 密钥候选要看上下文：用 `grep -abo <key> libapp.so` 取偏移，")
        print("    再 dd 取前后 900 字节 strings —— 落在 base64 大块里的不要当密钥。")
        print("  · AES 模式要读 Instance_AESMode 后一行的 Obj!AESMode 偏移，")
        print("    再到 objs.txt 对枚举名，不要凭函数名猜。")
        print("  · 找到 PEM 后用 openssl 校验：openssl rsa -in k.pem -noout -text")
