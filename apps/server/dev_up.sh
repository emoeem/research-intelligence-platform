#!/usr/bin/env bash
# RIP 本地开发栈一键启动（幂等）：PG 容器 → REST/静态后端(7800) → rip-web 前端(5173)。
# 全部按需、零 systemd、零 sudo；幂等：已监听则跳过。
# 用法：bash apps/server/dev_up.sh        # 启动并自检
#       bash apps/server/dev_down.sh      # 停止（默认保留 PG；--with-pg 连 PG 一起停）
set -uo pipefail
REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
RUN_DIR="${REPO}/rip-data/run"
mkdir -p "${RUN_DIR}"
BACK_PORT="${RP_PORT:-7800}"
WEB_PORT="${RP_WEB_PORT:-5173}"
PG_NAME="${RP_PG_NAME:-rip-pg17}"
PG_IMAGE="${RP_PG_IMAGE:-localhost/rip-pg17:latest@sha256:229189e4c66a2eb8ddd1fcf4bc08ee4726cacd8aee7445ec3c6f1499a357ef45}"
PG_DATA="${RP_PG_DATA:-/home/emo/rip-pg17-data}"

port_up() { ss -tln 2>/dev/null | grep -q ":$1 "; }
wait_port() { for _ in $(seq 1 40); do port_up "$1" && return 0; sleep 1; done; return 1; }

echo "== 1. PG 知识面（${PG_NAME}）"
if ! command -v podman >/dev/null 2>&1; then
  echo "  [skip] podman 缺位（知识卡/donor-submit 面不可用）"
elif podman container exists "${PG_NAME}" 2>/dev/null; then
  if [ "$(podman inspect -f '{{.State.Running}}' "${PG_NAME}")" = "true" ]; then
    echo "  [ok] 容器已在运行"
  else
    podman start "${PG_NAME}" >/dev/null && echo "  [ok] 已启动既有容器"
  fi
else
  mkdir -p "${PG_DATA}"
  podman run -d --name "${PG_NAME}" -e POSTGRES_PASSWORD=pgselftest \
    -p "127.0.0.1:5432:5432" -v "${PG_DATA}:/var/lib/postgresql/data" \
    "${PG_IMAGE}" >/dev/null 2>&1 \
    && echo "  [ok] 新建容器（数据 ${PG_DATA}）" \
    || echo "  [warn] 容器创建失败（镜像缺席？podman images | grep rip-pg17）"
fi

echo "== 2. 后端 rp_server（127.0.0.1:${BACK_PORT}）"
# M10 批次审核面：正式库（rip_platform @ rip-m4-personal-pg:55432）DSN 从
# 600 权限 env 文件注入（不入库、不打印）；缺文件时批次审核端点 fail-closed。
if [ -f "${REPO}/rip-data/dev/rip-platform-dsn.env" ]; then
  # shellcheck disable=SC1091
  . "${REPO}/rip-data/dev/rip-platform-dsn.env"
  export RP_PLATFORM_PG_DSN
  echo "  [ok] RP_PLATFORM_PG_DSN 已注入（rip-platform-dsn.env）"
fi
if port_up "${BACK_PORT}"; then
  echo "  [ok] 端口已在监听（复用既有进程）"
else
  [ -s "${REPO}/.rp-workbench-token" ] || { echo "  [fail] 缺 .rp-workbench-token（先 rp_service.sh gen-token）"; exit 1; }
  setsid nohup env RP_PORT="${BACK_PORT}" \
    RP_DB_PATH="${REPO}/migrations/platform.db" \
    RP_AUTH_TOKEN_FILE="${REPO}/.rp-workbench-token" \
    RP_PG_DSN="${RP_PG_DSN:-postgres://postgres:pgselftest@127.0.0.1:5432/postgres}" \
    python3 "${REPO}/apps/server/rp_server.py" \
    > "${RUN_DIR}/rp_server.log" 2>&1 < /dev/null &
  echo $! > "${RUN_DIR}/rp_server.pid"
  wait_port "${BACK_PORT}" && echo "  [ok] 已启动 pid=$(cat "${RUN_DIR}/rp_server.pid")" \
    || { echo "  [fail] 未监听，见 ${RUN_DIR}/rp_server.log"; tail -5 "${RUN_DIR}/rp_server.log"; exit 1; }
fi

echo "== 3. 前端 rip-web（127.0.0.1:${WEB_PORT}）"
if port_up "${WEB_PORT}"; then
  echo "  [ok] 端口已在监听（复用既有进程）"
else
  [ -d "${REPO}/web/node_modules" ] || { echo "  [fail] web/node_modules 缺失（cd web && npm ci）"; exit 1; }
  setsid nohup bash -c "cd '${REPO}/web' && exec npm run dev -- --host 127.0.0.1 --port ${WEB_PORT}" \
    > "${RUN_DIR}/rip_web.log" 2>&1 < /dev/null &
  echo $! > "${RUN_DIR}/rip_web.pid"
  wait_port "${WEB_PORT}" && echo "  [ok] 已启动 pid=$(cat "${RUN_DIR}/rip_web.pid")" \
    || { echo "  [fail] 未监听，见 ${RUN_DIR}/rip_web.log"; tail -5 "${RUN_DIR}/rip_web.log"; exit 1; }
fi

echo "== 4. 自检"
T="$(tr -d '\n' < "${REPO}/.rp-workbench-token" 2>/dev/null || true)"
code() { curl -s -o /dev/null -w '%{http_code}' "$@"; }
echo "  web  /            -> $(code "http://127.0.0.1:${WEB_PORT}/")"
echo "  api  /api/overview 无令牌 -> $(code "http://127.0.0.1:${BACK_PORT}/api/overview")"
echo "  api  /api/overview 带令牌 -> $(code -H "Authorization: Bearer ${T}" "http://127.0.0.1:${BACK_PORT}/api/overview")"
echo "  api  /api/plugins  带令牌 -> $(code -H "Authorization: Bearer ${T}" "http://127.0.0.1:${BACK_PORT}/api/plugins")"
echo "  proxy 5173/api/overview 带令牌 -> $(code -H "Authorization: Bearer ${T}" "http://127.0.0.1:${WEB_PORT}/api/overview")"

cat <<EOF

入口
  rip-web 主控制台 : http://127.0.0.1:${WEB_PORT}/      （令牌键 localStorage["rip_auth_token"]）
  M7 只读工作台    : http://127.0.0.1:${BACK_PORT}/     （/login.html 粘贴令牌）
  令牌             : tr -d '\\n' < ${REPO}/.rp-workbench-token
  日志             : ${RUN_DIR}/rp_server.log , ${RUN_DIR}/rip_web.log
停止 : bash apps/server/dev_down.sh [--with-pg]
EOF
