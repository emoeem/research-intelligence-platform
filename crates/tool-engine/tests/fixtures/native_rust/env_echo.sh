#!/bin/bash
# RT-1 native_rust e2e fixture（受控探针；RT-1-TASK S6「fixture 与 sha256 一并入库，测试自持」）。
#
# 用法: env_echo.sh <mode> <out_dir> <input_file>
#   argv 形态 = [fixture, subcommand, {{out_dir}}, {{input_path}}]（RT-1 ADR D3 形态 B）。
# stdin 一行 JSON 请求（D-TC-03）：fixture 不解析内容，仅读排空。
# stdout 一行 JSON 结果帧 {"status":"result","data":{...}}。
# PATH = 钉值二进制所在目录最小集（RT-1 ADR D4），所有外部命令一律绝对路径。
set -u

mode="${1:-env}"
out_dir="${2:-.}"
input_file="${3:-}"

emit() { printf '{"status":"result","data":%s}\n' "$1"; }

# 排空 stdin 请求行（内置 read，不依赖外部命令）
_line=""
while IFS= read -r _line; do :; done
_line=""

case "$mode" in
  env)
    # 环境回显（判据 4：断言子进程环境 = 显式白名单集合）
    json="["
    sep=""
    while IFS= read -r kv; do
      json="$json$sep\"$kv\""
      sep=","
    done < <(/usr/bin/env | /usr/bin/sort)
    json="$json]"
    nbytes="0"
    head32=""
    if [ -f "$input_file" ]; then
      nbytes=$(/usr/bin/wc -c < "$input_file")
      read -r head32 < "$input_file" || true
      head32="${head32:0:32}"
    fi
    emit "{\"env\":$json,\"input_file_bytes\":$nbytes,\"input_file_head\":\"$head32\"}"
    ;;
  echo)
    # 内容回显（判据 7a：结果侧 envelope::truncate 触发）
    content="${RIP_RT1_ECHO_CONTENT:-}"
    emit "{\"echo\":\"$content\"}"
    ;;
  spawn)
    # 孙进程 + 忽略 TERM（判据 5：进程组 TERM→KILL 与零残留）
    trap '' TERM
    /bin/sleep 60 &
    printf '%s\n' "$!" > "$out_dir/child.pid"
    wait "$!"
    ;;
  slow)
    # 阻塞 30s（判据 5：超时 TERM 路径；判据 6：取消）
    /bin/sleep 30
    ;;
  partial)
    # 先产出完整结果帧再阻塞（判据 6：取消期部分输出不伪装成功）
    emit '{"echo":"partial-not-delivered"}'
    /bin/sleep 30
    ;;
  big)
    # 200KB 帧（判据 7b：读侧流式帽触顶 → 进程组击杀，不整读 OOM）
    blob=$(/usr/bin/head -c 200000 /dev/zero | /usr/bin/tr '\0' 'x')
    emit "{\"blob\":\"$blob\"}"
    ;;
  *)
    printf '{"status":"error","error":{"code":-32602,"message":"unknown mode"}}\n' >&2
    exit 2
    ;;
esac
