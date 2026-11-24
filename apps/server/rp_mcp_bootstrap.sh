#!/usr/bin/env bash
# rp-mcp 数据面 bootstrap（ZCode / DSH 首次接入或 rip-data 重建后跑一次）。
# 幂等：caller 已存在 → caller-add 跳过（rp-admin 行为），key 已存在 → 复用
# .rp-mcp-token，register 同版本重复 → VersionComparable 拒绝（非零可忽略）。
# 用法：bash apps/server/rp_mcp_bootstrap.sh
set -euo pipefail
REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
ADMIN="${REPO}/target/debug/rp-admin"
TOOL="${REPO}/target/debug/rp-tool"
MAIN_DB="${REPO}/rip-data/caller-registry.db"
TOKEN_FILE="${REPO}/.rp-mcp-token"

mkdir -p "${REPO}/rip-data"
for bin in "$ADMIN" "$TOOL"; do
  [ -x "$bin" ] || { echo "缺少 $bin（先 cargo build --offline --workspace --bins）" >&2; exit 1; }
done

# 1. caller 注册库（rp-admin 自动迁移 migrations/ 全量 → caller/api_key 表；
#    同库兼作 rp-mcp catalog 读面 = tool 表所在主库 RP_DB_PATH）。
"$ADMIN" caller-list --db "$MAIN_DB" >/dev/null 2>&1 || true
if ! "$ADMIN" caller-list --db "$MAIN_DB" 2>/dev/null | grep -q "caller-dsh-main"; then
  "$ADMIN" caller-add --db "$MAIN_DB" --name dsh-main --level T2 --caller-id caller-dsh-main >/dev/null
  echo "caller 已建：caller-dsh-main（T2）"
fi

# 2. MCP 令牌（明文仅落 gitignored 600 文件；库内只存 sha256）。
if [ ! -s "$TOKEN_FILE" ]; then
  "$ADMIN" key-gen --db "$MAIN_DB" --caller-id caller-dsh-main --label zcode-mcp \
    | python3 -I -c 'import json,sys; print(json.load(sys.stdin)["key"])' > "$TOKEN_FILE"
  chmod 600 "$TOKEN_FILE"
  echo "令牌已签发：$TOKEN_FILE（600）"
fi

# 3. 工具目录注册（main 库 tool 表 = rip.list_tools 读面）。
ok=0
for f in "${REPO}"/manifests/tools/*.json; do
  "$TOOL" register --manifest "$f" --db "$MAIN_DB" >/dev/null 2>&1 && ok=$((ok+1))
done
total=$("$TOOL" list --db "$MAIN_DB" 2>/dev/null | python3 -I -c 'import json,sys; d=json.load(sys.stdin); print(len(d.get("items",d) if isinstance(d,dict) else d))')
echo "目录注册：本轮新增/更新 ${ok} 条，库内 total=${total}"

# 4. 知识面 PG（donor-submit / 知识卡；容器按需起）。
if command -v podman >/dev/null 2>&1 && ! podman container exists rip-pg17 2>/dev/null; then
  echo "提示：知识面需 PG——podman run -d --name rip-pg17 -e POSTGRES_PASSWORD=pgselftest -p 127.0.0.1:5432:5432 -v /home/emo/rip-pg17-data:/var/lib/postgresql/data localhost/rip-pg17:latest"
fi
echo "bootstrap 完成。启动器：apps/server/rp_mcp_launch.sh"
