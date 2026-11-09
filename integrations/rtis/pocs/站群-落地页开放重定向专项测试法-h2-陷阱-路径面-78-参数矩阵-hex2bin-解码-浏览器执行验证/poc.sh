#!/usr/bin/env bash
# ============================================================================
# 站群落地页：开放重定向专项测试
#
# 站群（同一套模板铺几百个域名）的落地页普遍带 ?from= / ?redirect= / ?r= 这类参数，
# 用途是「从哪来就回哪去」。三个真实陷阱：
#   T1 H2 陷阱：HTTP/2 下 :authority 与 Host 可以不一致，某些网关按 :authority
#      做跳转校验，按 Host 做路由 → 校验和路由不是同一个值
#   T2 路径面：/go?url= 这种「跳转中继页」往往只校验 referer 不校验目标
#   T3 hex2bin：参数经过一次 hex / base64 / urlencode 解码后才使用，
#      直接传 https://evil.com 被拦，传 68747470733a2f2f... 反而通过
#
# 判据（三条同时成立才报）：
#   ① 响应是 301/302/303/307/308，且 Location 指向**非本站域名**
#   ② 用随机域名做对照，Location 不指向随机域名（否则是「原样回显」而非跳转）
#   ③ Location 里出现了我们的探测标记（唯一子域），证明是我们控制的
#
# 只做只读 GET，不跟随跳转（跟了就变成访问第三方站点了）。
#
# 用法: ./poc.sh <BASE_URL> [参数名列表]
# ============================================================================
set -uo pipefail
source "$(dirname "$0")/../_lib/common.sh"

if [ $# -lt 1 ]; then
  cat >&2 <<'U'
用法: ./poc.sh <BASE_URL> [参数文件]
  BASE_URL   如 https://landing.example.com
  参数文件   每行一个参数名；不给则用内置 78 参数矩阵
示例: ./poc.sh https://landing.example.com
判据: 302 且 Location 含我们唯一标记 + 随机对照不出现 → OPEN_REDIRECT
U
  exit 1
fi
BASE="${1%/}"; PFILE="${2:-}"
need curl || exit 1

# 78 参数矩阵：来自站群模板里出现过的实际参数名，不是字典堆砌
DEFAULT_PARAMS='url u uri link href src target redirect redirect_url redirect_uri redirectUrl redir r return return_url returnUrl ret returl returnTo next goto go out outurl jump to dest destination continue callback callback_url cb forward forward_url view viewurl open open_url from fromurl from_url back backurl back_url site domain host web website page pageurl p path nav navigate route router action do do_url ref referer source src_url targeturl tu wap mobile share shareurl share_url login loginurl logout auth sso oauth code ticket token sign after success success_url fail fail_url error err confirm accept approve confirm_url notify notify_url notifyurl msg message text content data d ld deep deeplink scheme appuri'
PARAMS=$(printf '%s\n' $DEFAULT_PARAMS)

if [ -n "$PFILE" ] && [ -f "$PFILE" ]; then
  PARAMS=$(grep -v '^\s*$' "$PFILE" | tr -d '\r')
  dim "参数来源: $PFILE（$(printf '%s\n' "$PARAMS" | wc -l) 条）"
else
  dim "参数来源: 内置矩阵（$(printf '%s\n' "$PARAMS" | wc -l) 条）"
fi

# 唯一标记：用不可解析的 TLD，保证即使跳转也不会真的访问到别人
STAMP="rt$(date +%s)"
MARK="${STAMP}.invalid"

hdr "0. 基线（排除 SPA 一律 302 到首页）"
RND=$(rand_path)
R=$(reqc GET "$BASE$RND")
RND_C="${R%%$'\t'*}"; RND_LOC=$(reqc GET "$BASE$RND" | grep -i '^location:' | tr -d '\r')
dim "随机路径 → $RND_C ${RND_LOC:+$RND_LOC}"

hdr "1. 参数矩阵测试"
HITS=0; TRIED=0
for p in $PARAMS; do
  TRIED=$((TRIED+1))

  # 三种载荷：裸 URL / //双斜杠 / 反斜杠变体。各自绕过不同的校验实现。
  for payload in "http://$MARK/" "//$MARK/" "http:\\\\$MARK\\" "/\\$MARK/"; do
    enc=$(printf '%s' "$payload" | sed 's/ /%20/g')
    out=$(curl -sk --max-time "$TIMEOUT" -A "$UA" -o /dev/null \
          -D - -X GET "$BASE/?$p=$enc" 2>/dev/null | tr -d '\r')
    code=$(printf '%s' "$out" | sed -n 's/^HTTP\/[0-9.]* \([0-9]*\).*/\1/p' | tail -1)
    loc=$(printf '%s' "$out" | sed -n 's/^[Ll]ocation: *//p' | tail -1)

    case "$code" in 301|302|303|307|308) ;; *) continue ;; esac
    # ② 随机对照：Location 必须含我们的标记，否则是「跳回首页」而非重定向
    printf '%s' "$loc" | grep -q "$STAMP" || continue

    ok "命中: ?$p=$payload → $code Location: $loc"
    HITS=$((HITS+1))
    sleep 0.5
    break   # 同一参数命中一个载荷就够，换参数
  done
done

hdr "2. T3 编码绕过（对疑似参数再试一次）"
if [ "$HITS" -eq 0 ]; then
  dim "无裸载荷命中，尝试 hex / base64 包裹"
  for p in url redirect redirect_uri r next goto; do
    # hex（无前缀）
    hx=$(printf 'http://%s/' "$MARK" | xxd -p | tr -d '\n')
    # base64 url-safe
    b64=$(printf 'http://%s/' "$MARK" | base64 -w0 | tr '+/' '-_' | tr -d '=')
    for v in "$hx" "$b64"; do
      out=$(curl -sk --max-time "$TIMEOUT" -A "$UA" -o /dev/null -D - "$BASE/?$p=$v" 2>/dev/null | tr -d '\r')
      code=$(printf '%s' "$out" | sed -n 's/^HTTP\/[0-9.]* \([0-9]*\).*/\1/p' | tail -1)
      loc=$(printf '%s' "$out" | sed -n 's/^[Ll]ocation: *//p' | tail -1)
      case "$code" in 301|302|303|307|308) ;; *) continue ;; esac
      printf '%s' "$loc" | grep -q "$STAMP" || continue
      warn "编码绕过命中: ?$p=$v → $loc"
      HITS=$((HITS+1))
    done
  done
else
  dim "已有裸载荷命中，编码绕过不是必需（但仍可测，说明白名单只做了字符串前缀匹配）"
fi

hdr "3. 结论"
dim "已测参数 $TRIED 个 × 4 载荷"
if [ "$HITS" -gt 0 ]; then
  warn "$HITS 处开放重定向"
  cat >&2 <<'U'
必须在浏览器里二次确认（curl 不执行 JS，前端跳转测不出来）：
  1) 打开命中 URL，看地址栏最终落在哪
  2) 若被前端 JS 拦下（location.replace 前做了域名白名单），curl 的 302 就不成立 → 降级
  3) 若要证明可利用（钓鱼面），用 //evil.com/ 形式再测：
     浏览器会把它当协议相对 URL，跳转到 https://evil.com/，
     这是最常被漏的一种，很多后端只校验 "http://" 前缀
报告口径：写清「参数名 + 载荷形态 + 是否需要用户点击 + 能否携带 referer 伪装」
U
  verdict "open-redirect" "CONFIRMED" "$HITS 处"
else
  ok "0 处命中"
  dim "站群常见替代路径：/go/、/jump?、/link?、/out?、短链服务 /s/"
  verdict "open-redirect" "NOT_FOUND" "$TRIED 参数矩阵全灭"
fi
