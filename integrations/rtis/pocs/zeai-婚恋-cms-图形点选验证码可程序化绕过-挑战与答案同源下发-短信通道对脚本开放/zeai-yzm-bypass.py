#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""zeai CMS 点选验证码自动通过（仅验证码环节，默认不发短信）
用法: python3 zeai-yzm-bypass.py https://520.wanghongniang.com /m/reg_alone.php
"""
import json, sys, urllib.parse, urllib.request, http.cookiejar

MAP = {'零':'0','〇':'0','一':'1','壹':'1','二':'2','贰':'2','两':'2','三':'3','叁':'3',
       '四':'4','肆':'4','五':'5','伍':'5','六':'6','陆':'6','七':'7','柒':'7',
       '八':'8','捌':'8','九':'9','玖':'9','十':'10','拾':'10'}

def main(base, path):
    cj = http.cookiejar.CookieJar()
    op = urllib.request.build_opener(urllib.request.HTTPCookieProcessor(cj))
    op.addheaders = [('User-Agent','Mozilla/5.0 (Linux; Android 16) Chrome/130 Mobile')]

    r = op.open(base + path + '?submitok=ajax_click_yzm', timeout=15).read().decode()
    ch = json.loads(r)
    key, ans = ch['key'], MAP.get(ch['key'], '')
    print('[challenge]', ch)
    print('[solve] key=%s -> v=%s' % (key, ans))

    data = urllib.parse.urlencode({'key': key, 'v': ans}).encode()
    r2 = op.open(base + path + '?submitok=ajax_click_yzm_chk', data=data, timeout=15).read().decode()
    print('[check]', r2)
    return json.loads(r2).get('flag') == 1

if __name__ == '__main__':
    base = sys.argv[1] if len(sys.argv) > 1 else 'https://520.wanghongniang.com'
    path = sys.argv[2] if len(sys.argv) > 2 else '/m/reg_alone.php'
    sys.exit(0 if main(base, path) else 1)
