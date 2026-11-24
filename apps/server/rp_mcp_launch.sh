#!/usr/bin/env bash
# rp-mcp stdio 启动器（ZCode / DSH MCP 注册单点；env 全部在此，客户端配置零 env）。
# 用法：客户端以本脚本为 command（stdio MCP）。
# 令牌：.rp-mcp-token（gitignored，600）= rp-admin key-gen 签发的 rp1_ 钥明文；
# 轮换 = 重发钥 → 覆写该文件 → 客户端重连。凭据纪律：钥不进仓库。
set -euo pipefail
REPO="/media/emo/code/pensting/research-platform"

# 授权边界（MA-1 D2：非空即授权；sha256 → caller JOIN 在 rp-mcp 启动时解析
# ——吊销 = 七协作工具 fail-closed，目录 rip.* 零影响）。
export RP_MCP_TOKEN="$(cat "${REPO}/.rp-mcp-token")"

# 数据面（caller/api_key 注册库 = rp-admin 自动迁移至 026 的专用库；
# 生产 platform.db 零写——rp-mcp 两库只读打开）。
export RP_DB_PATH="${REPO}/rip-data/caller-registry.db"
export RP_RUN_DB="${REPO}/rip-data/tool-runs.db"
export RP_MIGRATIONS_DIR="${REPO}/migrations"
export RP_MANIFEST_DIRS="${REPO}/manifests/tools,${REPO}/containerfiles/rip-tool-webresearch,${REPO}/containerfiles/rip-tool-coreutils"
export RP_ARTIFACTS_DIR="${REPO}/rip-data/artifacts"

# 知识面（rp-kc donor-submit 经此 DSN；refinery V0001-V0010 自动入链）。
export RP_PG_DSN="postgres://postgres:pgselftest@127.0.0.1:5432/postgres"

# 派发子进程（rp-mcp 缺省 target/debug/ 相对 CWD——显式绝对路径防 CWD 漂移）。
export RP_AGENT_BIN="${REPO}/target/debug/rp-agent"
export RP_KC_BIN="${REPO}/target/debug/rp-kc"

exec "${REPO}/target/debug/rp-mcp"
