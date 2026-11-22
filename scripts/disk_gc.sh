#!/usr/bin/env bash
# scripts/disk_gc.sh — RT-5/6 磁盘治理与容器预算脚本（零 sudo；dry-run 默认；--apply 显式执行）
# 依据：docs/phase0/RT-5-6-TASK.md（D2 三层动作清单 L0/L1/L2 + D4 dangling-only；红线 4/5/8）
#       NATIVE-FIRST :194-196（镜像/容器/卷分别计量；先报告归属再清理；禁无差别全局面 prune）
# 用法：
#   bash scripts/disk_gc.sh report                        # L0 报表（默认 dry-run：双 target 四层 du + df + podman df + dangling 归属）
#   bash scripts/disk_gc.sh gc-incremental                # L1 dry-run：仅打印待清 incremental 与预期回收，零删除
#   bash scripts/disk_gc.sh gc-incremental --apply        # L1 执行：清空 <target>/debug/incremental（再生缓存，零语义损失）
#   bash scripts/disk_gc.sh report-deps                   # L2 报告级：deps 旧 fingerprint 残留报表（零删除——判定面留报告）
#   bash scripts/disk_gc.sh podman-prune-dangling         # D4 dry-run：dangling 镜像归属双证，拒绝执行删除
#   bash scripts/disk_gc.sh podman-prune-dangling --apply # D4 执行：podman image prune -f（dangling-only，禁全局面）
# 红线：本脚本永不执行 system 级全局清理（独立 -a 旗标 / --volumes 词形一律硬编码拒绝）；
#       零 sudo；零卷操作；ubuntu/kali Distrobox 容器、已停 rip-pg17 容器、带标签镜像零触碰。
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
REPO_TARGET="$REPO_ROOT/target"
RT3_TARGET="${RT3_TARGET:-/home/emo/rt3-target}"

usage() {
  grep '^#   bash scripts' "${BASH_SOURCE[0]}" | sed 's/^#   //'
}

die() { echo "REJECT: $*" >&2; exit 2; }

# 全局 prune 词形守卫（红线 5 / NATIVE-FIRST :195 原文词形）：独立 -a 旗标或 --volumes 一律拒绝
guard_no_global_prune() {
  local joined="$*"
  if printf '%s' "$joined" | grep -q -- '--volumes'; then
    die "命中全局面词形（--volumes）——硬编码拒绝：清理只允许 dangling-only 且零卷触碰"
  fi
  if printf '%s' "$joined" | grep -Eq -- '(^|[[:space:]])-a([[:space:]]|$)'; then
    die "命中全局面词形（独立 -a 旗标）——硬编码拒绝：清理只允许 dangling-only"
  fi
}

du_layer() {
  if [ -d "$1" ]; then
    du -sh "$1" 2>/dev/null | awk '{print $1"\t"$2}'
  else
    echo "缺失\t$1"
  fi
}

df_face() {
  echo "-- df（双挂载点）--"
  df -h /media/emo /home/emo | awk 'NR==1 || NR>1'
}

report_mode() {
  echo "===== L0 report（dry-run 默认，零删除）===== $(date '+%F %T')"
  df_face
  echo "-- 双 target 顶层占用 --"
  du_layer "$REPO_TARGET"
  du_layer "$RT3_TARGET"
  echo "-- repo target 四层 --"
  du_layer "$REPO_TARGET/debug/deps"
  du_layer "$REPO_TARGET/debug/incremental"
  du_layer "$REPO_TARGET/debug/build"
  du_layer "$REPO_TARGET/release"
  echo "-- rt3-target 四层 --"
  du_layer "$RT3_TARGET/debug/deps"
  du_layer "$RT3_TARGET/debug/incremental"
  du_layer "$RT3_TARGET/debug/build"
  du_layer "$RT3_TARGET/release"
  echo "-- podman system df（镜像/容器/卷分别计量）--"
  podman system df
  echo "-- podman dangling 归属（<none>:<none>，容器引用核查随行）--"
  local dangling_ids used
  dangling_ids="$(podman images -f dangling=true --format '{{.ID}} {{.Size}} {{.Created}}' || true)"
  used="$(collect_used_image_ids || true)"
  if [ -z "$dangling_ids" ]; then
    echo "（无 dangling 镜像）"
  else
    while IFS= read -r row; do
      local id="${row%% *}"
      if printf '%s' "$used" | grep -q "$id"; then
        echo "$row  [引用命中：有容器引用，禁删]"
      else
        echo "$row  [引用零命中：可 prune 候选]"
      fi
    done <<< "$dangling_ids"
  fi
  echo "-- podman 容器（Distrobox/停容器只报不删）--"
  podman ps -a --format '{{.Names}}\t{{.Status}}\t{{.Image}}'
  echo "-- podman 卷（零删除面）--"
  podman volume ls --format '{{.Name}}' | sed 's/^/  /'
  echo "-- 卷计数 --"
  podman volume ls --format '{{.Name}}' | wc -l
  echo "===== L0 report 结束（exit 0 幂等）====="
}

# L1：incremental 老化（再生缓存零语义损失；首次增量构建为唯一代价）
gc_incremental() {
  local apply="no"
  [ "${1:-}" = "--apply" ] && apply="yes"
  local dirs=()
  [ -d "$RT3_TARGET/debug/incremental" ] && dirs+=("$RT3_TARGET/debug/incremental")
  [ -d "$REPO_TARGET/debug/incremental" ] && dirs+=("$REPO_TARGET/debug/incremental")
  if [ "${#dirs[@]}" -eq 0 ]; then
    echo "gc-incremental：两 target 均无 incremental 目录，零动作"
    return 0
  fi
  echo "== L1 gc-incremental（apply=$apply）== $(date '+%F %T')"
  local before total_before=0 d b a reclaimed
  for d in "${dirs[@]}"; do
    b="$(du -sm "$d" 2>/dev/null | awk '{print $1}')"
    echo "待清 ${b}MiB  $d"
    total_before=$((total_before + b))
  done
  echo "预期回收合计 ${total_before}MiB"
  if [ "$apply" != "yes" ]; then
    echo "DRY-RUN：未执行删除（需 --apply 显式执行）"
    return 0
  fi
  if pgrep -x cargo >/dev/null 2>&1 || pgrep -x rustc >/dev/null 2>&1; then
    die "检测到 cargo/rustc 进程在运行——--apply 拒绝（避免破坏在途构建状态）"
  fi
  for d in "${dirs[@]}"; do
    b="$(du -sm "$d" 2>/dev/null | awk '{print $1}')"
    rm -rf "$d"
    mkdir -p "$d"
    a="$(du -sm "$d" 2>/dev/null | awk '{print $1}')"
    reclaimed=$((b - a))
    echo "已清 $d：before=${b}MiB after=${a}MiB 回收=${reclaimed}MiB"
  done
  echo "gc-incremental --apply 完成（再生缓存：下次构建自动重建）"
}

# L2：deps 旧 fingerprint 残留报表（报告级零删除——判定面留报告附录，RT-0 :12-14 词形）
report_deps() {
  echo "===== L2 report-deps（零删除）===== $(date '+%F %T')"
  local t deps
  for t in "$RT3_TARGET" "$REPO_TARGET"; do
    deps="$t/debug/deps"
    echo "-- $deps --"
    if [ ! -d "$deps" ]; then echo "  （目录缺失）"; continue; fi
    du -sh "$deps" | awk '{print "  层占用: "$1}'
    echo "  文件数: $(ls "$deps" | wc -l)"
    echo "  同名前缀多 hash 变体（旧 fingerprint 残留视图，top 20）："
    ls "$deps" 2>/dev/null \
      | sed -E 's/-[0-9a-f]{8,16}\.(rlib|rmeta|d|so)$//' \
      | sort | uniq -c | awk '$1 > 1' | sort -rn | awk 'NR <= 20 {print "    "$1" 变体  "$2}'
    echo "  层内最大文件 top 10："
    ls -S "$deps" 2>/dev/null | awk 'NR <= 10' | while IFS= read -r f; do
      du -h "$deps/$f" | awk '{print "    "$1"  "$2}'
    done
  done
  echo "L2 结束：本轮零删除（deps 层判定面留报告附录，RT-0 :14 词形延续）"
}

# D4：dangling-only 镜像清理（归属双证 → prune；禁全局面；禁卷操作）
# 引用核查口径（2026-11-23 实测教训：仅 ps -a + inspect 口径漏 external 引用——
# podman inspect 对 buildah working container 报 no such object；prune 首跑 loud 拦截 /
# 复跑静默跳过。收编两证：①常规容器 inspect .Image；②external working container
# 命名词形 "<image-id>-working-container" 内嵌基础镜像 ID（保守误报向安全侧）。
# 最终裁判 = podman image prune 自身（引擎侧零引用判定，本脚本不绕过）。
collect_used_image_ids() {
  for c in $(podman ps -a --format '{{.Names}}' 2>/dev/null); do
    podman inspect -f '{{.Image}}' "$c" 2>/dev/null
  done
  podman ps -a --external --format '{{.Names}}' 2>/dev/null \
    | grep -E -- '-working-container$' | sed -E 's/-working-container$//'
  true
}

podman_prune_dangling() {
  local apply="no"
  [ "${1:-}" = "--apply" ] && apply="yes"
  echo "== D4 podman-prune-dangling（apply=$apply）== $(date '+%F %T')"
  local dangling used
  dangling="$(podman images -f dangling=true --format '{{.ID}} {{.Size}} {{.Created}}')"
  if [ -z "$dangling" ]; then
    echo "无 dangling 镜像，零动作"
    return 0
  fi
  echo "-- dangling 清单 --"
  echo "$dangling"
  # 归属双证 1：容器引用零命中（含停止容器与 external working container）
  used="$(collect_used_image_ids)"
  while IFS= read -r row; do
    local id="${row%% *}"
    if printf '%s' "$used" | grep -q "$id"; then
      die "dangling 镜像 $id 被容器引用——红线 5 拒绝执行（归属未确认面零触碰）"
    fi
  done <<< "$dangling"
  echo "归属双证 1 通过：全部 dangling 镜像容器引用零命中"
  if [ "$apply" != "yes" ]; then
    echo "DRY-RUN：归属双证通过，拒绝执行删除（需 --apply 显式执行）"
    return 0
  fi
  echo "-- before: podman system df --"
  podman system df
  podman image prune -f
  echo "-- after: podman system df --"
  podman system df
  echo "D4 完成：dangling-only 回收（全局面词形零使用；卷零触碰）"
}

main() {
  local mode="${1:-report}"
  guard_no_global_prune "$@"
  shift || true
  case "$mode" in
    report) report_mode "$@" ;;
    gc-incremental) gc_incremental "$@" ;;
    report-deps) report_deps "$@" ;;
    podman-prune-dangling) podman_prune_dangling "$@" ;;
    -h|--help|help) usage ;;
    *) usage >&2; die "未知模式：$mode" ;;
  esac
}

main "$@"
