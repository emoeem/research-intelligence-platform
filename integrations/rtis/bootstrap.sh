#!/usr/bin/env bash
# RTIS kit 引导：仓库克隆后重建运行时数据（knowledge.db）。
# 幂等：DB 已存在则只做体检，不覆盖。
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DB="$HERE/knowledge.db"
DSH_DB="$HOME/.dsh/redteam/knowledge.db"

ok()   { printf '\033[32m[ok]\033[0m %s\n' "$*"; }
warn() { printf '\033[33m[!!]\033[0m %s\n' "$*"; }

if [[ -f "$DB" ]]; then
  ok "knowledge.db 已存在: $(du -h "$DB" | cut -f1)"
else
  if [[ -f "$DSH_DB" ]]; then
    cp "$DSH_DB" "$DB"
    ok "从 ~/.dsh/redteam/knowledge.db 复制完成"
  else
    # 从备份 tar.zst 里抽（备份含 .dsh/redteam/knowledge.db）
    TARBALL="$(ls -1t "$HOME"/backups/dsh-redteam-*.tar.zst 2>/dev/null | head -1 || true)"
    if [[ -n "${TARBALL:-}" ]]; then
      tar -I zstd -xf "$TARBALL" -C "$HERE" --strip-components=3 \
        .dsh/redteam/knowledge.db --transform 's|.dsh/redteam/||' 2>/dev/null \
        || tar -I zstd -xf "$TARBALL" -C /tmp --strip-components=2 .dsh/redteam/knowledge.db && mv /tmp/knowledge.db "$DB"
      [[ -f "$DB" ]] && ok "从备份 $TARBALL 重建 knowledge.db"
    else
      warn "找不到 knowledge.db 来源（既无 $DSH_DB 也无 ~/backups/dsh-redteam-*.tar.zst）"
      warn "检索/写入将不可用；skills 与脚本仍可用。"
      exit 1
    fi
  fi
fi

# 体检：能查库就算通过
if command -v node >/dev/null 2>&1; then
  R="$(echo '{"cmd":"search","q":"kingdee","limit":1}' | node "$HERE/rtis_kb.mjs" 2>/dev/null | head -c 200 || true)"
  case "$R" in
    *'"ok":true'*) ok "引擎自检通过（rtis-bridge + knowledge.db）" ;;
    *) warn "引擎自检未通过，回显: ${R:-<空>}" ;;
  esac
else
  warn "无 node，跳过引擎自检（rtis_kb.mjs / rtis_mcp.mjs 需要 node ≥ 22，内置 node:sqlite）"
fi

command -v python3 >/dev/null 2>&1 && python3 -c "import yaml" 2>/dev/null \
  && ok "rtis.py 依赖 pyyaml 可用" \
  || warn "缺 pyyaml（rtis.py 需要）：pip install pyyaml"

ok "完成。检索入口: node $HERE/rtis_kb.mjs   CLI: python3 $HERE/rt.py poc search --q <词>"
