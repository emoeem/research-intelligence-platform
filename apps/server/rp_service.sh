#!/usr/bin/env bash
# RP 工作台 systemd user 服务管理（无 sudo，用户级单元）。
# 用法：rp_service.sh {gen-token|install|uninstall|start|stop|restart|status|logs}
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
SERVER="${REPO_ROOT}/apps/server/rp_server.py"
TOKEN_FILE="${RP_AUTH_TOKEN_FILE:-${REPO_ROOT}/.rp-workbench-token}"
UNIT_DIR="${HOME}/.config/systemd/user"
UNIT_NAME="rp-workbench.service"
PORT="${RP_PORT:-7800}"
PYTHON_BIN="$(command -v python3)"

gen_token() {
  if [[ -e "${TOKEN_FILE}" ]]; then
    echo "令牌文件已存在，不覆盖：${TOKEN_FILE}（如需轮换请先删除）" >&2
    exit 1
  fi
  "${PYTHON_BIN}" -c 'import secrets; print(secrets.token_hex(32))' > "${TOKEN_FILE}"
  chmod 600 "${TOKEN_FILE}"
  echo "已生成令牌文件（600）：${TOKEN_FILE}"
}

install_unit() {
  [[ -f "${TOKEN_FILE}" ]] || { echo "缺少令牌文件：${TOKEN_FILE}（先执行 gen-token）" >&2; exit 1; }
  [[ $(stat -c %a "${TOKEN_FILE}") == "600" ]] || { echo "令牌文件权限不是 600，拒绝安装" >&2; exit 1; }
  mkdir -p "${UNIT_DIR}"
  cat > "${UNIT_DIR}/${UNIT_NAME}" <<UNIT
[Unit]
Description=RIP Workbench (M7 只读工作台 + Token 鉴权)
After=network.target

[Service]
ExecStart=${PYTHON_BIN} ${SERVER}
Environment=RP_PORT=${PORT}
Environment=RP_DB_PATH=${REPO_ROOT}/migrations/platform.db
Environment=RP_AUTH_TOKEN_FILE=${TOKEN_FILE}
WorkingDirectory=${REPO_ROOT}
Restart=on-failure
RestartSec=3

[Install]
WantedBy=default.target
UNIT
  systemctl --user daemon-reload
  systemctl --user enable --now "${UNIT_NAME}"
  echo "已安装并启动：systemctl --user status ${UNIT_NAME}"
}

case "${1:-}" in
  gen-token) gen_token ;;
  install) install_unit ;;
  uninstall) systemctl --user disable --now "${UNIT_NAME}" 2>/dev/null || true; rm -f "${UNIT_DIR}/${UNIT_NAME}"; systemctl --user daemon-reload; echo "已卸载" ;;
  start) systemctl --user start "${UNIT_NAME}" ;;
  stop) systemctl --user stop "${UNIT_NAME}" ;;
  restart) systemctl --user restart "${UNIT_NAME}" ;;
  status) systemctl --user status "${UNIT_NAME}" ;;
  logs) journalctl --user -u "${UNIT_NAME}" -f ;;
  *) echo "用法：$0 {gen-token|install|uninstall|start|stop|restart|status|logs}" >&2; exit 1 ;;
esac
