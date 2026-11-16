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
rm -f /task/out/result.jsonl
/usr/local/bin/nuclei -silent -no-color -j -irr=false -o /task/out/result.jsonl "$@" || RC=$?
RC=${RC:-0}
COUNT=0
[ -f /task/out/result.jsonl ] && COUNT=$(grep -c . /task/out/result.jsonl || true)
COUNT=$(echo "$COUNT" | head -1)
VERSION=$(/usr/local/bin/nuclei -version 2>&1 | grep -oE "v[0-9]+\.[0-9]+\.[0-9]+" | head -1)
if [ "$RC" = "0" ]; then
  printf '{"status":"result","data":{"matched_count":%s,"nuclei_version":"%s"}}\n' "$COUNT" "$VERSION"
fi
exit "$RC"
