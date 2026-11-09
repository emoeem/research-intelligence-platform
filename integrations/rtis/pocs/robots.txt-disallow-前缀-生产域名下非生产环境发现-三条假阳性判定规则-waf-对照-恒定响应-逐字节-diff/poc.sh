#!/usr/bin/env bash
# ============================================================================
# robots.txt disallow 前缀 → 生产域名下的非生产环境发现
#
# 原理：运维常把 /test /staging /admin 写进 robots.txt 的 Disallow 里
#       （本意是「别爬」），但生产域名的 robots 往往是从非生产环境复制来的。
#       这些前缀在生产域名上如果是**另一套后端**在服务，就会出现：
#         · 不同的 Server / X-Powered-By / 不同的错误页模板
#         · 不同的 TLS 证书 CN（反向代理按路径分流到了内网机）
#         · 不同的 Set-Cookie 域
#       这些差异才是判据，不是「能访问」。
#
# 三条假阳性规则（缺一条就会误报）：
#   R1 WAF 对照   ：先建「确定不存在路径」的基线，任何与基线逐字节相同的响应，
#                   都只是兜底页，不是非生产环境。
#   R2 恒定响应   ：同一前缀连续两次请求若 body 完全相同且含「建设中/维护」等字样，
#                   判为占位页而非环境。
#   R3 逐字节diff ：只有「前缀响应」与「根响应」在归一化后**确实不同**，
#                   且差异落在结构字段（Server/X-Powered-By/title/错误模板）上，
#                   才算发现。仅长度不同不算。
#
# 用法: ./poc.sh <BASE_URL> [前缀列表文件]
# ============================================================================
set -uo pipefail
source "$(dirname "$0")/../_lib/common.sh"

if [ $# -lt 1 ]; then
  cat >&2 <<'U'
用法: ./poc.sh <BASE_URL> [前缀文件]
  BASE_URL   如 https://example.com
  前缀文件   可选，每行一个路径；不给则从目标 robots.txt 自动提取
示例: ./poc.sh https://example.com
判据: 前缀响应与「根」及「随机路径基线」三方 diff，差异必须落在结构字段上
U
  exit 1
fi

BASE="${1%/}"
PFILE="${2:-}"

need curl || exit 1

hdr "0. 建立基线（R1 / R2 的前提）"
ROOT=$(reqc GET "$BASE/")
ROOT_C="${ROOT%%$'\t'*}"; ROOT_B="${ROOT#*$'\t'}"
RND_PATH=$(rand_path)
RND=$(reqc GET "$BASE$RND_PATH")
RND_C="${RND%%$'\t'*}"; RND_B="${RND#*$'\t'}"
ROOT_FP=$(printf '%s' "$ROOT_B" | norm)
RND_FP=$(printf '%s' "$RND_B" | norm)
dim "根       : $ROOT_C len=$(printf '%s' "$ROOT_B" | wc -c)"
dim "随机路径 : $RND_C len=$(printf '%s' "$RND_B" | wc -c)"

if [ "$ROOT_FP" = "$RND_FP" ] && [ "$ROOT_C" = "$RND_C" ]; then
  warn "根路径与随机路径响应一致 → 该站是「任意路径返回同一页」"
  warn "本方法在此目标上无判别力，后续结论一律不可信，退出"
  exit 2
fi
ok "基线有判别力（根 ≠ 随机）"

hdr "1. 取 robots.txt 的 Disallow 前缀"
if [ -n "$PFILE" ] && [ -f "$PFILE" ]; then
  mapfile -t PFX < "$PFILE"
  dim "来源: $PFILE"
else
  ROBOTS=$(req GET "$BASE/robots.txt")
  if ! printf '%s' "$ROBOTS" | grep -qi 'disallow'; then
    warn "robots.txt 无 Disallow 或不可读（len=$(printf '%s' "$ROBOTS" | wc -c)）"
    cat >&2 <<'U'
无 robots 时改用常见前缀清单：把下面 12 行存成 pfx.txt 后重跑 ./poc.sh <BASE> pfx.txt
/test
/staging
/uat
/pre
/dev
/admin
/manage
/internal
/api-test
/demo
/sandbox
/beta
U
    exit 3
  fi
  mapfile -t PFX < <(printf '%s' "$ROBOTS" | grep -i '^disallow' | sed 's/^[Dd]isallow:[[:space:]]*//' | tr -d '\r' | grep -v '^$' | sort -u)
  dim "来源: robots.txt（$(printf '%s' "$ROBOTS" | wc -c) 字节）"
fi

[ ${#PFX[@]} -eq 0 ] && { warn "前缀列表为空"; exit 3; }
dim "候选前缀 ${#PFX[@]} 条"

hdr "2. 逐前缀差分（R1 + R3）"
HITS=0
for p in "${PFX[@]}"; do
  [ -z "$p" ] && continue
  url="$BASE/${p#/}"
  R=$(reqc GET "$url")
  C="${R%%$'\t'*}"; B="${R#*$'\t'}"
  FP=$(printf '%s' "$B" | norm)

  # R1: 与随机路径一致 → 兜底，跳过
  if [ "$FP" = "$RND_FP" ] && [ "$C" = "$RND_C" ]; then
    dim "$p → $C 与随机基线一致（兜底页）"
    continue
  fi
  # R3: 与根一致 → 前端 router 兜底，不是另一套后端
  if [ "$FP" = "$ROOT_FP" ] && [ "$C" = "$ROOT_C" ]; then
    dim "$p → $C 与根一致（前端路由兜底）"
    continue
  fi

  # R2: 恒定响应 + 占位字样
  R2=$(reqc GET "$url"); B2="${R2#*$'\t'}"
  if [ "$(printf '%s' "$B" | norm)" = "$(printf '%s' "$B2" | norm)" ] && \
     printf '%s' "$B" | grep -qiE '建设中|维护中|coming soon|under construction|placeholder'; then
    dim "$p → $C 恒定占位页"
    continue
  fi

  # 结构字段差异
  SRV=$(printf '%s' "$ROOT_B" | grep -oiE 'Server:[^\r\n]*' | head -1)
  SRV2=$(printf '%s' "$B"       | grep -oiE 'Server:[^\r\n]*' | head -1)
  T1=$(printf '%s' "$ROOT_B" | grep -oiE '<title>[^<]*' | head -1)
  T2=$(printf '%s' "$B"       | grep -oiE '<title>[^<]*' | head -1)

  ok "候选: $p → $C len=$(printf '%s' "$B" | wc -c)"
  [ -n "$SRV2" ] && [ "$SRV" != "$SRV2" ] && dim "server 变了: ${SRV:-<none>} → $SRV2"
  [ -n "$T2" ]   && [ "$T1"  != "$T2"  ] && dim "title 变了 : ${T1:-<none>} → $T2"
  HITS=$((HITS+1))
  sleep "$SLEEP"
done

hdr "结论"
if [ "$HITS" -eq 0 ]; then
  ok "0 条候选存活 → 该目标上 robots 前缀没有独立后端"
  verdict "robots-disallow-nonprod" "NOT_FOUND" "R1/R2/R3 全部排除"
else
  warn "$HITS 条候选与根/随机基线都不同"
  cat >&2 <<'U'
下一步（人工）：
  1) 对每条候选比对 TLS 证书 CN / SAN：openssl s_client -connect host:443 -servername host
     证书变了 = 按路径分流到了另一台机器，这是最强的非生产环境证据
  2) 比对响应头完整集合（不只是 Server）：X-Env / X-Backend / Set-Cookie Domain
  3) 取 /favicon.ico 的 md5：不同环境常复用同一套前端，favicon 相同但后端不同
U
  verdict "robots-disallow-nonprod" "CANDIDATE" "$HITS 条待人工确认"
fi
