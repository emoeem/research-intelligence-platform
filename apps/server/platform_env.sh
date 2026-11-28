#!/usr/bin/env bash
# Shared RIP server environment loader.
# The DSN file must contain only local environment assignments and mode 600.

rip_load_platform_pg_env() {
  local repo="$1"
  local dsn_file="${RP_PLATFORM_PG_DSN_FILE:-${repo}/rip-data/dev/rip-platform-dsn.env}"
  local line

  if [[ -n "${RP_PLATFORM_PG_DSN:-}" ]]; then
    return 0
  fi
  if [[ ! -f "${dsn_file}" ]]; then
    return 1
  fi
  if [[ $(stat -c %a "${dsn_file}") != "600" ]]; then
    echo "PostgreSQL DSN 文件权限不是 600，拒绝加载：${dsn_file}" >&2
    return 2
  fi
  RP_PLATFORM_PG_DSN=""
  while IFS= read -r line || [[ -n "$line" ]]; do
    [[ -z "$line" || "$line" == \#* ]] && continue
    if [[ "$line" != RP_PLATFORM_PG_DSN=* ]]; then
      echo "PostgreSQL DSN 文件含不受支持的配置行：${dsn_file}" >&2
      return 4
    fi
    RP_PLATFORM_PG_DSN="${line#RP_PLATFORM_PG_DSN=}"
  done < "${dsn_file}"
  export RP_PLATFORM_PG_DSN
  if [[ -z "${RP_PLATFORM_PG_DSN:-}" ]]; then
    echo "PostgreSQL DSN 文件未定义 RP_PLATFORM_PG_DSN：${dsn_file}" >&2
    return 3
  fi
  return 0
}
