#!/usr/bin/env bash
# ============================================================================
# 对象存储桶零凭据枚举（阿里 OSS / 腾讯 COS / AWS S3 / 七牛 / 又拍）
# 用法: ./poc.sh https://TARGET
#       ./poc.sh --bucket my-bucket-1250000000 cos ap-shanghai
#
# 两条路径：
#   A. 从目标站点自动提取桶名（图片/JS/下载链接里的 bucket 域）
#   B. 给定桶名，判定四态 + ACL + 列桶 + 写权限（写只判定不落文件）
#
# 【判据】四态错误码 —— 这是唯一可靠的区分方式
#   200 + ListBucketResult        → 公开可读，可列全部对象
#   403 + AccessDenied            → 桶存在，但 List 被拒
#   403 + SignatureDoesNotMatch   → 桶存在，需要签名（说明有凭据校验）
#   404 + NoSuchBucket            → 桶不存在
#   404 + 其它（NoSuchKey）        → 桶存在，只是这个 key 不存在
#
#   ★ 关键: 不能只看 403。OSS 对「不存在的桶」也可能返回 403（防枚举），
#     所以要同时看 body 里的 Code 字段，或者用 region 交叉验证。
#
# 【常见误报】
#   1. CDN 边缘对任意路径返回 200 的 index.html → 必须校验 body 是 XML 且含 ListBucketResult。
#   2. 桶开了「静态网站托管」→ 根路径返回 index.html(200)，但 list 是 403。
#      这不代表可以列桶。要单独请求 ?list-type=2。
#   3. 桶名规则: OSS 必须 3-63 字符小写字母数字连字符；COS 必带 appid 后缀。
# ============================================================================
set -uo pipefail
source "$(dirname "$0")/../_lib/common.sh"

MODE=auto; BUCKET=""; VENDOR="oss"; REGION=""
case "${1:-}" in
  --bucket) MODE=direct; BUCKET="$2"; VENDOR="${3:-oss}"; REGION="${4:-}" ;;
  ""|--help) usage_die "$0 <https://TARGET>  |  $0 --bucket NAME [oss|cos|s3|qiniu] [region]" ;;
  *) MODE=auto; BASE="${1%/}" ;;
esac

# ---- 端点构造 -------------------------------------------------------------
endpoint_for() {  # endpoint_for <bucket> <vendor> <region>
  local b="$1" v="$2" r="$3"
  case "$v" in
    oss)   echo "${b}.${r:-oss-cn-hangzhou}.aliyuncs.com" ;;
    cos)   echo "${b}.cos.${r:-ap-guangzhou}.myqcloud.com" ;;
    s3)    echo "${b}.s3.${r:-us-east-1}.amazonaws.com" ;;
    qiniu) echo "${b}.${r:-clouddn}.qiniudn.com" ;;
  esac
}

# ---- 四态判定 -------------------------------------------------------------
probe_bucket() {  # probe_bucket <endpoint>
  local ep="$1"
  local r; r=$(reqc GET "https://$ep/?list-type=2&max-keys=3")
  local c="${r%%$'\t'*}"; local b="${r#*$'\t'}"

  # 必须是 XML 且是 ListBucketResult / Error
  local isxml=0
  printf '%s' "$b" | grep -q '^<?xml' && isxml=1

  local code_field=""
  code_field=$(printf '%s' "$b" | grep -oE '<Code>[^<]*' | head -1 | sed 's/<Code>//')

  if [ "$c" = "200" ] && printf '%s' "$b" | grep -q 'ListBucketResult'; then
    ok "★ 公开可列  https://$ep"
    dim "    对象数（本次取样）: $(printf '%s' "$b" | grep -c '<Key>')"
    printf '%s' "$b" | grep -oE '<Key>[^<]*' | sed 's/<Key>//' | head -10 | while read -r k; do
      dim "      $k"
    done
    return 0
  elif [ "$c" = "403" ] && [ "$code_field" = "AccessDenied" ]; then
    warn "存在但 List 被拒  https://$ep  (Code=AccessDenied)"
  elif [ "$c" = "403" ] && [ "$code_field" = "SignatureDoesNotMatch" ]; then
    warn "存在但需签名     https://$ep  (Code=SignatureDoesNotMatch)"
  elif [ "$c" = "404" ] && [ "$code_field" = "NoSuchBucket" ]; then
    dim "不存在           https://$ep  (Code=NoSuchBucket)"
  elif [ "$c" = "404" ]; then
    dim "key 不存在但桶在  https://$ep  (Code=${code_field:-?})"
  elif [ "$c" = "301" ]; then
    local loc; loc=$(req GET "https://$ep/" -o /dev/null -w '%{redirect_url}')
    warn "桶在其它区域     → $loc（用该 region 重试）"
  elif [ "$isxml" = 0 ]; then
    dim "非对象存储响应    https://$ep  → $c (可能是 CDN 兜底)"
  else
    dim "未知            https://$ep  → $c  Code=${code_field:-?}"
  fi
  return 1
}

# ---- ACL 与写权限（非侵入）------------------------------------------------
probe_acl() {  # probe_acl <endpoint>
  local ep="$1"
  local b; b=$(req GET "https://$ep/?acl")
  printf '%s' "$b" | grep -q 'AccessControlPolicy' || { dim "ACL 不可读"; return; }
  echo "  ACL:"
  printf '%s' "$b" | grep -oE '<(ID|DisplayName|Permission|Grantee)[^>]*>[^<]*' \
    | sed 's/<[^>]*>/    /' | head -12
}

# 写权限判定：用 PUT + 0 字节 + 一个明显无效的 key，看是否返回 403。
# 不真正写入（0 字节且立刻 DELETE 尝试清理；若 DELETE 也成功，说明可写）。
probe_write() {  # probe_write <endpoint>
  local ep="$1"
  local key="_probe_$(date +%s)_$RANDOM.txt"
  local c; c=$(curl -sk -o /dev/null -w '%{http_code}' --max-time "$TIMEOUT" \
      -X PUT -H 'Content-Length: 0' --data '' "https://$ep/$key" 2>/dev/null)
  case "$c" in
    200|204) warn "★ 匿名 PUT 成功 → 可写入 https://$ep/$key（已尝试删除清理）"
             curl -sk -X DELETE --max-time "$TIMEOUT" "https://$ep/$key" >/dev/null 2>&1
             dim "    DELETE 已发出，请人工确认对象是否残留" ;;
    403)     dim "PUT 被拒 (403)" ;;
    *)       dim "PUT → $c" ;;
  esac
}

# ---- 模式 A: 从站点自动提取 -----------------------------------------------
if [ "$MODE" = "auto" ]; then
  preflight || exit 1
  log "从 $BASE 提取桶名候选"
  html=$(req GET "$BASE/")
  : > /tmp/_buckets.txt

  # 站内资源里的对象存储域名
  printf '%s' "$html" \
    | grep -oE '(://|//)[a-z0-9][a-z0-9.-]*\.(aliyuncs\.com|myqcloud\.com|amazonaws\.com|qiniucdn\.com|qiniudn\.com|b0\.upaiyun\.com)' \
    | sed -E 's#^(://|//)##' | sort -u >> /tmp/_buckets.txt

  # 页面里直接出现的 css/js/img src
  printf '%s' "$html" | grep -oE '(src|href)="[^"]*"' | sed -E 's/^(src|href)="//; s/"$//' \
    | while read -r u; do
        case "$u" in
          *aliyuncs.com*|*myqcloud.com*|*amazonaws.com*|*qiniucdn.com*)
            printf '%s\n' "$u" ;;
        esac
      done >> /tmp/_buckets.txt

  # CNAME 反查: 解析站点的 cdn 域名看它 CNAME 到哪个桶
  log "CNAME 反查"
  for h in $(printf '%s' "$html" | grep -oE '//[a-z0-9][a-z0-9.-]*\.(com|cn|net)' | sed 's#^//##' | sort -u | head -20); do
    cn=$(dig +short CNAME "$h" 2>/dev/null | head -1)
    case "$cn" in
      *.aliyuncs.com|*.myqcloud.com|*.amazonaws.com)
        ok "CNAME: $h → $cn" | tee -a /tmp/_buckets.txt ;;
    esac
  done

  EP=$(sed -E 's#^https?://##; s#/.*$##' /tmp/_buckets.txt 2>/dev/null | sort -u)
  n=$(printf '%s\n' "$EP" | grep -c . 2>/dev/null; true)
  log "候选端点 $n 个"
  [ "$n" = 0 ] && { warn "未从页面提取到桶，改用 --bucket 直接指定"; exit 0; }

  echo
  while read -r ep; do
    [ -z "$ep" ] && continue
    probe_bucket "$ep"
    # 只对「存在」的继续
    r=$(reqc GET "https://$ep/?list-type=2")
    printf '%s' "${r#*$'\t'}" | grep -qE 'AccessDenied|SignatureDoesNotMatch|ListBucketResult' && {
      probe_acl "$ep"
      probe_write "$ep"
    }
    echo
    sleep "$SLEEP"
  done <<< "$EP"
fi

# ---- 模式 B: 直接指定 -----------------------------------------------------
if [ "$MODE" = "direct" ]; then
  ep=$(endpoint_for "$BUCKET" "$VENDOR" "$REGION")
  log "探测 $VENDOR 桶: $ep"
  probe_bucket "$ep"
  probe_acl "$ep"
  probe_write "$ep"
  echo
  # 其它 region 交叉验证（OSS 桶可能不在你猜的 region）
  case "$VENDOR" in
    oss)
      log "跨 region 交叉验证（OSS 返回 301 时用）"
      for rg in oss-cn-hangzhou oss-cn-shanghai oss-cn-beijing oss-cn-shenzhen oss-cn-beijing oss-ap-southeast-1; do
        e="${BUCKET}.${rg}.aliyuncs.com"
        c=$(code_of GET "https://$e/?list-type=2")
        [ "$c" = "200" ] && ok "$rg → 200"
      done ;;
    cos)
      log "COS 必须带 appid 后缀；若上面 404，尝试补 @appid"
      ;;
  esac
fi

cat <<'EOF'

════ 判读速查 ════
  200 + ListBucketResult   → 完全公开（可列、可下载）
  403 + AccessDenied       → 存在，List 禁；但**单个 key 仍可能可读**
  403 + SignatureDoesNotMatch → 存在，需签名
  404 + NoSuchBucket       → 不存在
  301                      → 桶在别的 region，跟随 Location 重试

════ 存在但不可列时（403 AccessDenied）════
  仍有三种可能（逐条验证，不要直接判「安全」）：
  ① 已知 key 可直读:  https://BUCKET.oss-xxx.aliyuncs.com/ <已知文件名>
  ② 桶策略只禁 List 但允许 GetObject: 用从站点抓到的真实对象 URL 直接下载
  ③ 静态网站托管:      https://BUCKET.oss-xxx.aliyuncs.com/index.html

════ 注意 ════
  probe_write 会真的发一次 PUT（0 字节），成功后立即 DELETE。
  如果目标桶在生产使用中，这一步可能触发审计告警。
  确定要做再跑，或者只跑 probe_bucket 不要 probe_write。
EOF
