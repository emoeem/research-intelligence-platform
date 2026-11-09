#!/usr/bin/env bash
# ============================================================================
# 泛微 e-cology 三层判据探测器
# 用法: ./poc.sh https://TARGET
#
# 覆盖三个本项目反复遇到的现象：
#   ① 「系统维护中」503 其实是 request_uri 路径封堵，不是真维护
#   ② 404 / 534b 错误页二分：securityIntercept 安全拦截 vs 真 404
#   ③ 封堵名单漏项：503 挡外、500/200 进应用、302 到 noright.jsp(已完成鉴权)
#
# 【判据】504/534b 这类「错误页」为什么能当 oracle
#   泛微的自定义错误页在 body 里带固定标记（如 "534b"、"安全拦截"、"系统维护中"），
#   而真正的 Tomcat 404 是标准 Whitelabel 页。
#   两者 body 完全不同 → 可以据此判断「路径被拦」还是「路径不存在」。
#
#   关键区别：
#     被 WAF 拦    → 拦截页（可能是 200）
#     被应用拦     → noright.jsp / 302 到登录页
#     路径被删     → 真 404
#     路径存在     → 业务响应（JSON/HTML 正常页）
#
# 【常见误报】
#   1. 把「站点级 404」当 WAF。对照: 请求一个肯定不存在的路径 /zzz_xxx，
#      如果它也返回同样的 503/534b，那说明是**全站兜底**，不是针对你的路径。
#   2. 302 到 login.jsp → 是「需要登录」，不是「漏洞」。要看 302 的 Location。
#   3. noright.jsp 出现 = 已通过身份校验但无权限，这是「登录会话有效」的证据，
#      不是漏洞本身。要配合弱口令或未授权接口才成链。
# ============================================================================
set -uo pipefail
source "$(dirname "$0")/../_lib/common.sh"

BASE="${1:-}"
[ -z "$BASE" ] && usage_die "$0 <https://TARGET>"
BASE="${BASE%/}"
preflight || exit 1

# ---- 基线：随机路径 -------------------------------------------------------
log "基线: 随机路径（判定全站兜底页）"
rB=$(reqc GET "$BASE$(rand_path)")
cB="${rB%%$'\t'*}"; bB="${rB#*$'\t'}"
fpB=$(printf '%s' "$bB" | fp)
dim "随机路径 → $cB  len=${#bB}  fp=${fpB:0:70}"
printf '%s' "$bB" | head -c 300 | tr -d '\n'; echo
echo

# 全站兜底特征提取（用于后续逐条排除）
FALLBACK_504=0
printf '%s' "$bB" | grep -qE '系统维护|534b|安全拦截' && FALLBACK_504=1
[ "$FALLBACK_504" = 1 ] && warn "随机路径也返回维护/拦截页 → 这是全站兜底，不能拿它当 per-path 信号"

# ---- 泛微路径表 -----------------------------------------------------------
# path|说明|期望特征
PATS=(
  '/weaver/|核心 servlet 前缀|任意非 404'
  '/wui/index.html|前端入口|HTML'
  '/spa/portal/static/|静态资源|200'
  '/mobile/plugin/1/|移动端插件（历史 SSRF/RCE 入口）|非 404'
  '/weaver/bsh.servlet.BshServlet|BeanShell（CVE-2019-…经典 RCE 入口）|200/500 且非兜底'
  '/weaver/org.apache.axis2/|Axis2 管理面|HTML'
  '/services/|Axis2 services 列表（SOAP）|XML'
  '/weaver/ln.FileDownload|任意文件下载|非 404'
  '/download/|文件下载面|非 404'
  '/weaver/weaver.file.FileDownloadForMobile|移动端下载|非 404'
  '/workflow/request/ViewRequestForwardSPA.jsp|流程详情|302/200'
  '/api/ec/dev/app/getCheckUserStatus|未授权接口探测|JSON'
  '/api/ec/dev/locale/getSystemLanguage|未授权接口探测|JSON'
  '/api/ec/dev/table/createTable|建表（高危，需 POST）|不自动调'
  '/oauth2/authorize|OAuth2 授权端点|302/200'
  '/noright.jsp|无权限页（说明已完成鉴权）|200/302'
  '/login/Login.jsp|登录页|200'
  '/weaver/login.jsp|登录页（旧）|200'
  '/js/hrm/getdata.jsp|hrm 数据接口|非 404'
  '/plugins/|插件目录|目录列表或 403'
)

echo "════ 路径探测 ════"
printf '%-46s %-6s %-8s %s\n' "PATH" "CODE" "LEN" "判定"
printf '%s\n' "──────────────────────────────────────────────────────────────────"
hits=0
for row in "${PATS[@]}"; do
  IFS='|' read -r p desc expect <<<"$row"
  r=$(reqc GET "$BASE$p")
  c="${r%%$'\t'*}"; b="${r#*$'\t'}"
  f=$(printf '%s' "$b" | fp)
  len=${#b}

  verdict=""
  if [ "$f" = "$fpB" ]; then
    verdict="兜底页(与随机一致)"
  else
    case "$c" in
      200) verdict="★ 200 有内容" ;;
      301|302)
        loc=$(req GET "$BASE$p" -o /dev/null -w '%{redirect_url}')
        case "$loc" in
          *noright*) verdict="302→noright.jsp ★已鉴权" ;;
          *login*|*Login*) verdict="302→登录页(需认证)" ;;
          *) verdict="302→${loc:0:40}" ;;
        esac ;;
      401) verdict="401 需认证" ;;
      403) verdict="403 拒绝" ;;
      500) verdict="500 处理异常(路径存在)" ;;
      503) [ "$FALLBACK_504" = 1 ] && verdict="503(兜底)" || verdict="503 路径级封堵" ;;
      404) verdict="404 不存在" ;;
      534) verdict="534 定制错误页" ;;
      *)   verdict="code=$c" ;;
    esac
  fi

  case "$verdict" in
    *★*) ok "$(printf '%-46s %-6s %-8s %s' "$p" "$c" "$len" "$verdict")"; hits=$((hits+1)) ;;
    *)   dim "$(printf '%-46s %-6s %-8s %s' "$p" "$c" "$len" "$verdict")" ;;
  esac
  dim "      $desc"
  sleep "$SLEEP"
done

# ---- 封堵绕过候选 ---------------------------------------------------------
echo
log "封堵绕过候选（仅当 /weaver/ 被 403/503 单独拦截时有效）"
cb=$(code_of GET "$BASE/weaver/")
if [ "$cb" = "403" ] || [ "$cb" = "503" ]; then
  BYPASS=(
    '/Weaver/'
    '/weaver//'
    '/weaver/./'
    '/weaver/%2e/'
    '/weaver/..;/'
    '/.;/weaver/'
    '/%2fweaver/'
    '/weaver%2f'
    '/weaver/;/weaver/'
    '/weaver/../weaver/'
    '//weaver/'
    '/weaver%00/'
  )
  for bp in "${BYPASS[@]}"; do
    cc=$(code_of GET "$BASE$bp")
    if [ "$cc" != "$cb" ] && [ "$cc" != "000" ]; then
      ok "绕过候选: $bp → $cc（原 $cb）"
    else
      dim "无效: $bp → $cc"
    fi
    sleep 0.4
  done
else
  dim "/weaver/ 返回 $cb，未触发路径级封堵，无需绕过"
fi

echo
[ "$hits" -gt 0 ] && ok "共 $hits 条高价值信号" || warn "无高价值信号"
cat <<'EOF'

════ 判读口径 ════
  「系统维护中」503 ≠ 站点维护。
  判据是你请求的**那个路径**返回 503，而随机路径返回 404/正常页。
  若随机路径也是 503 → 全站兜底，这个信号无效。

  noright.jsp 的含义是「已通过身份校验但无权限」——
  它证明当前会话有效。如果你拿到它，说明你已经过了认证层，
  下一步该测越权（换 uid/换部门）而不是继续找入口。

════ 下一步（按发现走，不自动执行）════
  BshServlet 200  → 该类漏洞为命令执行，需先确认授权范围
  /api/ec/dev/*   → 这些是历史未授权接口，POST 参数见各自的公开分析
  /services/      → Axis2 SOAP，可 listServices 看部署了哪些
  noright.jsp     → 有会话，测水平/垂直越权
EOF
