#!/bin/sh
# rip-nuclei 运行面 wrapper（P5-5）：keep-id 非 root 写面收敛 + D-TC-03 帧协议。
# 1) 配置目录与 ignore 文件运行期就位（nuclei v3.11.1 对缺失 .nuclei-ignore
#    的 ENOENT 会把全部模板过滤出局，空对象 {} 即合法空忽略表）；
# 2) 调用面模板（/task/in/template.yaml，Planner 的 template_ref 锚落位）
#    装入默认模板目录 /task/out/nuclei-templates（不联网装模板不跑全量）；
# 3) nuclei 结果落 /task/out/result.jsonl（Adapter 收集 out/ 为 artifact），
#    stdout 第一行输出 D-TC-03 result 帧（{"status":"result","data":{...}}，
#    matched_count + nuclei_version；大输出摘录由 rp_server 读 artifact 承载）；
# 4) 命中行数不是 Finding 判定——五态归一化在 P5-7 evaluator。
set -e
CFG="${XDG_CONFIG_HOME:-/task/out/.config}/nuclei"
mkdir -p "$CFG" /task/out/nuclei-templates
[ -f "$CFG/.nuclei-ignore" ] || echo '{}' > "$CFG/.nuclei-ignore"
[ -f /task/in/template.yaml ] && cp /task/in/template.yaml /task/out/nuclei-templates/template.yaml
# P5-9c：egress 白名单代理链（podman_host 注入 http(s)_proxy env 指向 run
# 生命周期白名单代理）。nuclei 不读 env proxy（实测死端口代理仍"成功"零
# 命中且零请求落目标面），须显式 -http-proxy 旗标承载白名单约束；无 env
# （network none 本机面）保持零旗标零行为变化。
if [ -n "$http_proxy" ]; then
  set -- -proxy "$http_proxy" "$@"
fi
rm -f /task/out/result.jsonl
# P5-9c 实录：nuclei 命中行打 stdout（-o 只落 JSONL 文件），会把 D-TC-03
# 结果帧挤出"stdout 第一行"不变量（信封解析 frame status unexpected）。
# stdout 静默（诊断走 stderr 照旧），命中详情以 result.jsonl artifact 承载。
/usr/local/bin/nuclei -silent -no-color -j -irr=false -o /task/out/result.jsonl "$@" >/dev/null || RC=$?
RC=${RC:-0}
COUNT=0
[ -f /task/out/result.jsonl ] && COUNT=$(grep -c . /task/out/result.jsonl || true)
COUNT=$(echo "$COUNT" | head -1)
VERSION=$(/usr/local/bin/nuclei -version 2>&1 | grep -oE "v[0-9]+\.[0-9]+\.[0-9]+" | head -1)
if [ "$RC" = "0" ]; then
  printf '{"status":"result","data":{"matched_count":%s,"nuclei_version":"%s"}}\n' "$COUNT" "$VERSION"
fi
exit "$RC"
