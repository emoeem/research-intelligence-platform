#!/usr/bin/env bash
# Struts2 四 oracle 判别 —— 不用任何 OGNL 关键字，WAF 全拦也能定案
# 用法: ./poc.sh <BASE_URL> [已知活的 action 路径，如 /ent/index.do]
set -u
. "$(dirname "$0")/../_lib/common.sh"
preflight || exit 1

BASE="${1:-}"; [ -n "$BASE" ] || usage_die "$0 <BASE_URL> [已知action路径]"
BASE="${BASE%/}"
ACTION="${2:-}"

TIMEOUT="${TIMEOUT:-10}"
hdr "Struts2 四 oracle 判别"
dim "目标: $BASE"
dim "已知action: ${ACTION:-（无，oracle 2/3 将跳过）}"
echo

# ---------- 基线 ----------
read -r BL_CODE BL_FP < <(baseline GET "$BASE")
log "基线: 随机路径 → $BL_CODE"
echo

V1=""; V2=""; V3=""; V4=""

# ---------- Oracle 1 · 内置静态资源 ----------
log "Oracle 1 · /struts/ 内置静态资源"
for p in /struts/utils.js /struts/domTT.css; do
  r=$(reqc GET "$BASE$p")
  c="${r%%$'\t'*}"; b="${r#*$'\t'}"
  sz=${#b}
  cls=$(classify3 "$c" "$b" "$BL_FP")
  if [ "$c" = "200" ] && [ "$sz" -gt 200 ]; then
    if printf '%s' "$b" | grep -qiE 'struts|dojo|domTT'; then
      ok "$p → 200 (${sz}B) 含 Struts 特征"
      V1="yes"
    else
      warn "$p → 200 (${sz}B) 但内容无 Struts 特征（可能是 SPA 兜底页）"
    fi
  else
    dim "$p → $c [$cls] ${sz}B"
  fi
done
[ -z "$V1" ] && warn "静态资源未命中（filter 只挂 *.do 时属正常，不能据此否定）"
echo

# ---------- Oracle 2 · .action 后缀 ----------
if [ -n "$ACTION" ]; then
  log "Oracle 2 · .action 后缀一致性"
  baseR=$(reqc GET "$BASE$ACTION"); bc="${baseR%%$'\t'*}"; bb="${baseR#*$'\t'}"
  alt="${ACTION%.do}.action"
  r=$(reqc GET "$BASE$alt"); ac="${r%%$'\t'*}"; ab="${r#*$'\t'}"
  dim "$ACTION → $bc (${#bb}B)"
  dim "$alt → $ac (${#ab}B)"
  if [ "$bc" = "200" ] && [ "$ac" = "200" ] && [ "$(printf '%s' "$bb"|fp)" = "$(printf '%s' "$ab"|fp)" ]; then
    ok ".action 与 .do 响应结构一致 → 支持 Struts2"
    V2="yes"
  elif [ "$bc" = "200" ] && [ "$ac" = "404" ]; then
    err ".action 404 而 .do 正常 → 不是 Struts2（或 extension 已改成 do）"
    V2="no"
  else
    dim "行为不一致，无法定案"
  fi
  echo
fi

# ---------- Oracle 3 · DMI method: 前缀 ----------
if [ -n "$ACTION" ]; then
  log "Oracle 3 · DMI method: 前缀（决定性，无关键字）"
  f1=$(reqc GET "$BASE$ACTION" | cut -f2 | fp)
  f2=$(reqc GET "$BASE$ACTION?method:__nope__" | cut -f2 | fp)
  f3=$(reqc GET "$BASE$ACTION?method:execute" | cut -f2 | fp)
  if [ "$f1" = "$f2" ] && [ "$f1" = "$f3" ]; then
    err "三次响应逐字节相同 → 无 DMI 语义 → 不是 Struts2"
    V3="no"
  elif [ "$f1" != "$f2" ]; then
    ok "method:__nope__ 产生响应差异 → DMI 生效 → 是 Struts2"
    V3="yes"
  else
    dim "部分差异，需人工看 500/参数注入痕迹"
    V3="?"
  fi
  echo
fi

# ---------- Oracle 4 · 错误信封 ----------
log "Oracle 4 · 404 错误信封指纹"
dim "$(printf '%s' "$BL_FP" | head -c 160)"
if printf '%s' "$BL_FP" | grep -qE '"errorCode"|"success"|No handler|Whitelabel'; then
  ok "404 返回结构化 JSON/框架错误页 → 后端是 Spring/Struts 系"
  V4="yes"
elif [ "$BL_CODE" = "404" ]; then
  dim "裸 404，无信封（nginx 兜底或框架未暴露错误细节）"
fi
echo

# ---------- 结论 ----------
hdr "结论"
score=0
[ "$V1" = "yes" ] && score=$((score+1))
[ "$V2" = "yes" ] && score=$((score+1))
[ "$V3" = "yes" ] && score=$((score+1))
[ "$V4" = "yes" ] && score=$((score+1))
neg=0
[ "$V2" = "no" ] && neg=$((neg+1))
[ "$V3" = "no" ] && neg=$((neg+1))

printf '  正证据 %d / 负证据 %d\n' "$score" "$neg"
if [ "$neg" -gt 0 ]; then
  err "判定: 不是 Struts2（负证据优先级高于正证据）"
  exit 2
elif [ "$score" -ge 2 ]; then
  ok "判定: 是 Struts2（${score} 条独立正证据）"
  exit 0
elif [ "$score" -eq 1 ]; then
  warn "判定: 疑似 Struts2，证据不足 —— 需补一个已知 action 重跑 oracle 2/3"
  exit 3
else
  warn "判定: 证据不足（WAF 可能全拦，或目标非 Struts2）"
  exit 3
fi
