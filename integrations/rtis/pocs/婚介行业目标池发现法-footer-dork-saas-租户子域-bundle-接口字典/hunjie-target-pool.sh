#!/usr/bin/env bash
# hunjie-target-pool.sh —— 婚介/婚恋行业目标池发现法（无需 FOFA）
# 思路：不从"站点"找站点，从"卖系统的厂商"倒推"买系统的客户"。
#   ① footer 特征 dork  → 找到运营中的 OElove 独立站
#   ② vendor 子域枚举   → 拿到 SaaS 厂商的全部租户站（一套代码 × N 客户）
#   ③ 单份前端 bundle   → 提取全平台接口字典
set -u
UA='Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 Chrome/120'
# ---- 参数检查 / 执行确认 ----
# 本脚本会真实访问外部搜索引擎与 crt.sh，并对真实厂商域名做子域枚举。
# 不加 --run 时只打印计划，不发请求。
if [ "${1:-}" != "--run" ]; then
  cat >&2 <<'USAGE'
用法: ./hunjie-target-pool.sh --run [--vendor-dir-only]

本脚本会执行：
  ① 6 组 dork × 3 页 → so.com（约 18 次请求，每次间隔 2s）
  ② 5 个厂商域名的 subfinder 枚举 + crt.sh 证书透明日志查询
  ③ 租户站前端 bundle 提取接口字典

不加 --run 时只显示本说明，不发起任何外部请求。
USAGE
  exit 1
fi
shift
OUT="${OUT:-/tmp/hunjie-out}"; mkdir -p "$OUT"

## ① 中文搜索引擎 footer 特征反查（360 结果页直接回显真实域名，不用跟跳转；bing/baidu 的操作符在本机网络下失效）
dork () {
  local q="$1"; local enc
  enc=$(python3 -c "import urllib.parse,sys;print(urllib.parse.quote(sys.argv[1]))" "$q")
  for pn in 1 2 3; do
    curl -s -A "$UA" "https://www.so.com/s?q=$enc&pn=$pn"
    sleep 2
  done | grep -oE 'https?://[a-zA-Z0-9._-]+\.(com|cn|net|top|cc|vip|org)' \
       | sed -E 's#https?://##' | tr 'A-Z' 'a-z' \
       | grep -viE 'so\.com|360|baidu|bing|qhimg|360tres|mediav|oelove\.com|phpcoo\.com|oephp\.com'
}
{
  dork '"Powered By OElove"'
  dork '"Powered by OElove" 婚介 相亲 会员列表'
  dork 'inurl:index.php?a=login&c=passport'
  dork '"会员列表" "Powered By OElove"'
  dork '"163K" 婚恋 相亲 系统'
  dork '婚介管理系统 红娘 会员 登录'
} | sort -u > $OUT/candidates.txt

## ② SaaS 厂商子域枚举 —— 租户站 = 客户缩写.厂商域名
# 已知厂商（可继续扩：赛信 saixin.net、奥壹 oelove.com、奇思 sikesoft.net、99婚介 99hunjie.cn、飞英 flyingdigital.net）
VENDORS="163k.com 163kyun.cn mnhongsidai.com hafnkj.com ruraltec.cn"
: > $OUT/tenants.txt
for d in $VENDORS; do
  subfinder -d "$d" -silent 2>/dev/null                         >> $OUT/tenants.txt
  curl -s "https://crt.sh/?q=%25.$d&output=json" \
    | jq -r '.[].name_value' 2>/dev/null | tr ',' '\n' \
    | sed 's/^\*\.//' | tr 'A-Z' 'a-z'                          >> $OUT/tenants.txt
done
sort -u $OUT/tenants.txt | grep -E '\.' > $OUT/tenants_uniq.txt

## ③ 统一指纹：只要 80/443，不做端口全扫
httpx -l $OUT/tenants_uniq.txt -ports 80,443 -title -tech-detect -status-code -silent -no-color \
  | tee $OUT/tenants_httpx.txt

## ④ 单份 SPA bundle → 全平台接口字典（Vite 入口在 HTML 里的 /assets/index.*.js）
# 例：curl -s https://ailem.163kyun.cn/ | grep -oE '/assets/index\.[a-f0-9]+\.js'
#     curl -s https://ailem.163kyun.cn/assets/index.b59611bf.js -o idx.js
#     grep -oE '"/[a-zA-Z][a-zA-Z0-9_/-]{4,60}"' idx.js | tr -d '"' | sort -u > api.txt

## ⑤ OElove 专项：后台版本回显 + 控制器存在性探测（不爆破、不登录）
# for h in $(cat $OUT/candidates.txt); do
#   curl -s "http://$h/index.php?c=login&m=admin" | grep -oE 'OElove v[0-9.]+'   # 版本直接印在标题
#   curl -s "http://$h/index.php?c=NOT_EXIST"    | grep -q 'Index controller is forbiden' && echo "$h 是 OElove"
# done
echo "[done] 候选 $(wc -l < $OUT/candidates.txt) / 租户 $(wc -l < $OUT/tenants_uniq.txt)"
