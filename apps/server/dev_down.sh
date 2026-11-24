#!/usr/bin/env bash
# RIP 本地开发栈停止（dev_up.sh 的对偶）。默认保留 PG 容器（数据面），--with-pg 连它一起停。
# 用法：bash apps/server/dev_down.sh [--with-pg]
set -uo pipefail
REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
RUN_DIR="${REPO}/rip-data/run"
PG_NAME="${RP_PG_NAME:-rip-pg17}"
WITH_PG=0
[ "${1:-}" = "--with-pg" ] && WITH_PG=1

stop_pidfile() { # stop_pidfile <name> <pidfile> <port>
  local name="$1" pidfile="$2" port="$3" pid=""
  if [ -s "$pidfile" ]; then
    pid="$(cat "$pidfile")"
    # setsid 起的进程自成进程组：杀整组，连带 npm→vite 子进程
    kill -TERM -- "-${pid}" 2>/dev/null || kill -TERM "${pid}" 2>/dev/null
    rm -f "$pidfile"
    sleep 1
  fi
  # 兜底：按监听端口解析（覆盖非本脚本启动的实例）
  local p
  p="$(ss -tlnp 2>/dev/null | awk -v port=":${port}" '$4 ~ port {print $6}' \
       | grep -oE 'pid=[0-9]+' | head -1 | cut -d= -f2)"
  if [ -n "${p:-}" ]; then kill -TERM "$p" 2>/dev/null; sleep 1; fi
  if ss -tln 2>/dev/null | grep -q ":${port} "; then
    echo "  [warn] ${name} 端口 ${port} 仍占用（pid ${p:-?}），可 kill -9"
  else
    echo "  [ok] ${name} 已停止"
  fi
}

echo "== 停止前端 rip-web"
stop_pidfile "rip-web" "${RUN_DIR}/rip_web.pid" "${RP_WEB_PORT:-5173}"
echo "== 停止后端 rp_server"
stop_pidfile "rp_server" "${RUN_DIR}/rp_server.pid" "${RP_PORT:-7800}"

if [ "$WITH_PG" = "1" ]; then
  echo "== 停止 PG（${PG_NAME}，数据卷保留）"
  podman stop "${PG_NAME}" >/dev/null 2>&1 && echo "  [ok] 已停止" || echo "  [skip] 容器不在或已停"
else
  echo "== PG 保留（${PG_NAME}）；要一起停：bash $0 --with-pg"
fi
