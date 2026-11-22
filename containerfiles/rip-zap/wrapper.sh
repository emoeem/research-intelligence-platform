#!/bin/sh
# rip-zap 运行面 wrapper（WSW-7 D1②）：keep-id 非 root 写面收敛 + D-TC-03 帧协议。
# 1) ZAP home 收敛到 /task/out/zap-home（-dir 旗标；配置/session/report 写面
#    全收敛任务目录——根文件系统只读由 Adapter --read-only 注入）；
# 2) zap.sh 内联 baseline 扫描（-cmd -quickurl/-quickout：spider + active
#    scan 快扫面；scan_mode 词表单值 baseline——深扫扩展归观察池）；
# 3) egress 白名单代理链（podman_host 注入 http(s)_proxy env 指向 run 生命周期
#    白名单代理）。zap.sh 不读 env proxy（nuclei wrapper :16-22 同问题域），
#    须显式 -config connection.proxy.chain.* 旗标承载白名单约束（镜像内
#    zap.sh -help 实拍核实项 2——S3 实拍词形为准）；无 env（network none
#    本机面）保持零旗标零行为变化；
# 4) 结果落 /task/out/zap-report.json（Adapter 收集 out/ 为 artifact），
#    stdout 第一行输出 D-TC-03 result 帧（{"status":"result","data":{...}}，
#    matched_count + alerts_excerpt + zap_version；命中数不是 Finding 判定
#    ——五态归一化在 evaluator）。zap 进程 stdout/stderr 静默落
#    /task/out/zap.log（nuclei wrapper :24-26 实录同问题域：诊断流会把
#    result 帧挤出"stdout 第一行"不变量）。
# 参数面（execute_zap entrypoint 同型）：-u <url> -t <timeout_seconds> -b <alert_budget>
# 组装纪律：set -- 数组拼装（零 sh -c 重引号面——URL 词形不受宿主注入面）。
set -eu

ZAP_BIN="${ZAP_BIN:-/zap/zap.sh}"
[ -x "$ZAP_BIN" ] || ZAP_BIN=$(command -v zap.sh)

URL=""
TIMEOUT_S=300
ALERT_BUDGET=100
while [ $# -gt 0 ]; do
  case "$1" in
    -u) URL="${2:-}"; shift 2 ;;
    -t) TIMEOUT_S="${2:-300}"; shift 2 ;;
    -b) ALERT_BUDGET="${2:-100}"; shift 2 ;;
    *) shift ;;
  esac
done
[ -n "$URL" ] || { echo "rip-zap-wrapper: missing -u <url>" >&2; exit 2; }

ZAP_HOME=/task/out/zap-home
mkdir -p "$ZAP_HOME" /task/out
# P5-9c 同款：egress 白名单代理链（env proxy → 显式旗标承载）。
set -- "$ZAP_BIN" -cmd -dir "$ZAP_HOME" -config start.checkforupdates=false
if [ -n "${http_proxy:-}" ]; then
  PROXY_PORT=$(printf '%s' "$http_proxy" | sed -E 's|.*:([0-9]+)$|\1|')
  set -- "$@" \
    -config connection.proxy.chain.enabled=true \
    -config connection.proxy.chain.serverName=127.0.0.1 \
    -config connection.proxy.chain.port="$PROXY_PORT"
fi
set -- "$@" -quickout /task/out/zap-report.json -quickurl "$URL"
# zap 进程输出静默（诊断落 artifact zap.log；stdout 第一行留给 result 帧）。
"$@" >/task/out/zap.log 2>&1 || RC=$?
RC=${RC:-0}
if [ -f /task/out/zap-report.json ]; then
  FRAME=$(ALERT_BUDGET="$ALERT_BUDGET" python3 - <<'PYEOF'
import json, os
cap = int(os.environ.get("ALERT_BUDGET", "100"))
try:
    with open("/task/out/zap-report.json", encoding="utf-8", errors="replace") as fh:
        report = json.load(fh)
except Exception:
    report = {}
sites = report.get("site") or []
if isinstance(sites, dict):
    sites = [sites]
alerts = []
for site in sites:
    site_alerts = site.get("alerts") or []
    if isinstance(site_alerts, dict):
        site_alerts = [site_alerts]
    alerts.extend(site_alerts)
excerpt_items = []
for a in alerts[:cap]:
    name = a.get("alert") or a.get("name") or "?"
    risk = a.get("riskcode") or a.get("risk") or ""
    excerpt_items.append(f"{name}({risk})")
version = report.get("version") or report.get("zapVersion") or ""
frame = {"status": "result", "data": {
    "matched_count": len(alerts),
    "alerts_excerpt": "; ".join(excerpt_items),
    "zap_version": str(version),
}}
print(json.dumps(frame, ensure_ascii=False))
PYEOF
  )
  [ -n "$FRAME" ] && printf '%s\n' "$FRAME"
fi
exit "$RC"
