#!/usr/bin/env bash
# ============================================================================
# FluxWMS 识别 + 未授权面探测
#
# 为什么单独写一个 WMS（仓储管理系统）：这类系统有三个共同特征
#   1. 前端是固定模板（/static/js/app.*.js 里有 flux / wms 字样）
#   2. 后端多为 Spring Boot，actuator 常年不关
#   3. 登录接口常把 RSA 公钥直接下发（/login/getPublicKey），可离线构造密文
#
# 探测面（全部只读）：
#   A 指纹    /、/login、/static/、/favicon.ico 的 title/ico hash
#   B actuator /actuator、/actuator/env、/actuator/health、/actuator/mappings
#   C SAML     /saml/metadata、/saml2/metadata、/sso/saml/metadata（部分版本可枚举 IdP）
#   D 文件读取 /static/../application.yml、/actuator/env 里的 file 引用
#   E 登录 RSA /login/getPublicKey 或 /api/auth/publicKey，返回 PEM/base64 即命中
#
# 判据：每条都要与随机路径做基线差分。200 本身没有判别力。
#
# 用法: ./poc.sh <BASE_URL>
# ============================================================================
set -uo pipefail
source "$(dirname "$0")/../_lib/common.sh"

if [ $# -lt 1 ]; then
  cat >&2 <<'U'
用法: ./poc.sh <BASE_URL>
  BASE_URL 如 https://wms.example.com 或 http://10.0.0.8:8080
判据: 指纹命中 flux/wms 后，再逐面探测；每面都与随机路径差分
U
  exit 1
fi
BASE="${1%/}"
need curl || exit 1

hdr "0. 基线"
RND=$(rand_path)
R=$(reqc GET "$BASE$RND")
RND_C="${R%%$'\t'*}"; RND_FP=$(printf '%s' "$R" | norm)
dim "随机路径 → $RND_C len=$(printf '%s' "${R#*$'\t'}" | wc -c)"

hdr "A. 指纹"
FP_HIT=0
for p in "/" "/login" "/index" "/#/login"; do
  r=$(reqc GET "$BASE$p"); c="${r%%$'\t'*}"; b="${r#*$'\t'}"
  fp=$(printf '%s' "$b" | norm)
  [ "$c" = "$RND_C" ] && [ "$fp" = "$RND_FP" ] && { dim "$p → 兜底"; continue; }
  t=$(printf '%s' "$b" | grep -oiE '<title>[^<]*' | head -1 | sed 's/<title>//i')
  js=$(printf '%s' "$b" | grep -oE 'src="[^"]+\.js"' | head -3 | tr '\n' ' ')
  [ -n "$t$js" ] && dim "$p → $c title=${t:-<none>} js=$(printf '%s' "$js" | head -c 120)"
  if printf '%s' "$b" | grep -qiE 'flux|wms|warehouse|仓储'; then
    ok "指纹命中: $p 含 flux/wms/warehouse 关键字"; FP_HIT=1
  fi
done

# favicon hash 是跨站比对最稳的指纹（同一套代码换域名，ico 不变）
ICO=$(reqc GET "$BASE/favicon.ico")
ICO_C="${ICO%%$'\t'*}"
if [ "$ICO_C" = "200" ]; then
  H=$(printf '%s' "${ICO#*$'\t'}" | md5sum | cut -d' ' -f1)
  ok "favicon.ico md5 = $H"
  dim "把该 hash 拿去 FOFA/Shodan 搜同族：icon_hash=\"$H\""
else
  dim "favicon.ico → $ICO_C"
fi

[ "$FP_HIT" -eq 0 ] && warn "未命中 flux/wms 关键字，继续探面（可能只是被换皮）"

hdr "B. actuator（Spring Boot 未授权）"
# 关键：actuator 的命中判据是 JSON 结构，不是 200
for p in "/actuator" "/actuator/health" "/actuator/env" "/actuator/mappings" "/actuator/beans" "/actuator/configprops"; do
  r=$(reqc GET "$BASE$p"); c="${r%%$'\t'*}"; b="${r#*$'\t'}"
  fp=$(printf '%s' "$b" | norm)
  [ "$c" = "$RND_C" ] && [ "$fp" = "$RND_FP" ] && { dim "$p → 兜底"; continue; }
  if printf '%s' "$b" | grep -qE '"_links"|"status"\s*:\s*"UP"|"propertySources"|"contexts"|"beans"'; then
    ok "$p → $c 结构命中"
    printf '%s' "$b" | head -c 200; echo
  else
    dim "$p → $c（无 actuator 结构）"
  fi
  sleep 0.4
done

hdr "C. SAML metadata"
for p in "/saml/metadata" "/saml2/metadata" "/sso/saml/metadata" "/simplesaml/module.php/saml/sp/metadata.php/default-sp"; do
  r=$(reqc GET "$BASE$p"); c="${r%%$'\t'*}"; b="${r#*$'\t'}"
  if [ "$c" = "200" ] && printf '%s' "$b" | grep -qiE 'EntityDescriptor|AssertionConsumerService|X509Certificate'; then
    warn "$p → $c SAML metadata 可读"
    printf '%s' "$b" | grep -oiE 'entityID="[^"]+"|Location="[^"]+"' | head -5
  else
    dim "$p → $c"
  fi
done

hdr "D. 配置文件读取（只读，只看前 200 字节判断是否 yml）"
for p in "/application.yml" "/application.properties" "/static/../application.yml" "/actuator/env/spring.datasource.url" "/config/application.yml"; do
  r=$(reqc GET "$BASE$p"); c="${r%%$'\t'*}"; b="${r#*$'\t'}"
  if [ "$c" = "200" ] && printf '%s' "$b" | grep -qiE '^\s*(spring|server|datasource|logging)\s*[:.]'; then
    warn "$p → $c 疑似配置文件"
    printf '%s' "$b" | head -5
  else
    dim "$p → $c"
  fi
  sleep 0.4
done

hdr "E. 登录 RSA 公钥下发"
for p in "/login/getPublicKey" "/api/auth/publicKey" "/auth/publicKey" "/login/publicKey" "/sys/login/publicKey"; do
  r=$(reqc GET "$BASE$p"); c="${r%%$'\t'*}"; b="${r#*$'\t'}"
  if [ "$c" = "200" ] && printf '%s' "$b" | grep -qiE 'BEGIN PUBLIC KEY|MII[A-Za-z0-9+/]{40,}|publicKey'; then
    warn "$p → $c 下发了公钥"
    printf '%s' "$b" | head -c 300; echo
    cat >&2 <<'U'
    这条本身不是漏洞，但它意味着登录密码是 RSA 加密后提交：
      · 若公钥固定不变 → 攻击者可离线构造任意密码密文，撞库不受"每次随机"保护
      · 若私钥在配置里泄露（D 面命中 application.yml）→ 可直接解密抓到的登录包
      · 常见实现错误：只加密不校验签名，重放同一密文可重复登录
U
  else
    dim "$p → $c"
  fi
done

hdr "结论"
verdict "flux-wms-surface" "DONE" "逐面结果见上，命中项需人工确认同一性"
cat >&2 <<'U'
同一性确认（避免把别的 Spring 站当 FluxWMS）：
  1) /actuator/mappings 里搜 flux/wms 的 Controller 包名
  2) 登录页 JS 里搜版本号：grep -oE 'version["'\'']?\s*[:=]\s*["'\''][0-9.]+' app.*.js
  3) favicon hash 拿去 FOFA：icon_hash="<上面的 md5>"，同 hash 即同套代码
U
