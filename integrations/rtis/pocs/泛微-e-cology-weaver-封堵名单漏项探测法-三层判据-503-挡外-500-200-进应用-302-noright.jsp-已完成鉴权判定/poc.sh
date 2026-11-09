#!/usr/bin/env bash
# 泛微 e-cology /weaver/ 封堵名单漏项探测（只 GET，只读）
# 用法: bash weaver-blocklist-gap.sh https://target.example.com
# ---- 参数检查 ----
if [ $# -lt 1 ]; then printf '用法: ./poc.sh <https://target.example.com>\n' >&2; exit 1; fi
HOST="${1:?usage: $0 https://target.example.com}"
UA='Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36'
SLEEP=1.6

# e-cology 9 高危 servlet / 入口清单（越全越好；漏项往往在冷门 servlet 上）
PATHS=(
  /weaver/bsh.servlet.BshServlet
  /weaver/ln.CtrlAction
  /weaver/weaver.file.FileDownload
  /weaver/weaver.file.FileDownloadForOutDoc
  /weaver/weaver.file.FileUpload
  /weaver/weaver.file.FileUploadForOutDoc
  /weaver/weaver.file.MakeDownloadFile
  /weaver/weaver.formmode.upload.Upload
  /weaver/weaver.docs.file.FileUpload
  /weaver/weaver.docs.file.DocOperate
  /weaver/docs/file/FileDownload
  /weaver/weaver.hrm.HrmUserInfo
  /weaver/weaver.hrm.HrmUserHrmInfo
  /weaver/weaver.sync.SyncUserInfo
  /weaver/weaver.extra.SyncUserInfo
  /weaver/SyncUserInfo
  /weaver/weaver.security.bean.SecurityRuleGetter
  /weaver/weaver.jmx.JMXBean
  /weaver/weaver.cache.CacheManager
  /weaver/weaver.cpt.cpt.CptGoto
  /weaver/weaver.general.GetHtmlRemindInfo
  /weaver/DBUtil
  /weaver/weaver.conn.DBUtil
  /weaver/rdeploy/deploy.jsp
  /weaver/WFSSO/wfso.jsp
  /weaver/webservices/WorkflowService
  /weaver/mobile/plugin/browser.jsp
  /weaver/mobile/plugin/jgetfile.jsp
  /weaver/mobile/plugin/VerifyQuickLogin.jsp
  /weaver/mobile/plugin/ecology.MobilePluginOperation
  /weaver/portal/
  /weaver/workflow/
  /weaver/ecology/
  /weaver/develop/
)

BODY=$(mktemp); HDR=$(mktemp)
printf '%-52s %-5s %-7s %s\n' PATH CODE SIZE JUDGE
printf '%.0s-' {1..110}; echo

for p in "${PATHS[@]}"; do
  curl -sk --max-time 20 -A "$UA" -D "$HDR" -o "$BODY" "${HOST}${p}"
  code=$(awk '/^HTTP\//{c=$2} END{print c+0}' "$HDR")
  size=$(wc -c < "$BODY")
  judge="?"
  if   grep -q '系统维护中' "$BODY";                     then judge="503-BLOCKED(list 内)"
  elif grep -q '/images/error/404.png' "$BODY";          then judge="404-不存在"
  elif grep -q '/images/error/500.png' "$BODY";          then judge="500-已进应用(抛异常) ★"
  elif grep -qi 'notice/noright.jsp' "$BODY";            then judge="302-noright(已完成鉴权判定) ★★"
  elif grep -qi 'X-XSS-Protection' "$HDR";               then judge="200-已进应用(泛微头) ★★"
  elif [ "$code" = "503" ];                              then judge="503-其他"
  elif [ "$code" = "200" ];                              then judge="200-其他(看 body)"
  fi
  printf '%-52s %-5s %-7s %s\n' "$p" "$code" "$size" "$judge"
  sleep "$SLEEP"
done

echo
echo "=== 判定规则 ==="
echo "只有 503 与 404 才是被挡在门外；500 / 200 / 302→noright.jsp 都说明请求已到 Java 应用。"
echo "把非 503/404 的行挑出来 —— 那些就是封堵名单的漏项，是后续所有工作的落点。"
echo
echo "=== 不要做的事 ==="
echo "不要对 /weaver/ 做路径变形（// 、%XX 、大小写、./ 、..;/）—— 该前缀的封堵做了 URI 规范化，变形 100% 无效。"
echo "唯一有效的方向是换 servlet 名。"
rm -f "$BODY" "$HDR"