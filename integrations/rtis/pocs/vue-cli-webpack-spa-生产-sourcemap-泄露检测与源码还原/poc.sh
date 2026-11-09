#!/usr/bin/env bash
# ============================================================================
# 生产 sourcemap 泄露检测与源码还原
# 用法: ./poc.sh https://TARGET
#       ./poc.sh https://TARGET /static/js/app.js   (已知入口)
#
# 【判据】为什么 .map 返回 200 还不够
#   SPA 常在 404 时返回 index.html（200），所以直接请求 app.js.map 拿到
#   200 不代表真的存在。必须校验:
#     ① Content-Type 是 application/json，或 body 以 {"version":3 开头
#     ② body 含 "sources" 且 "mappings" 两个必需字段
#     ③ 可选但关键: "sourcesContent" 存在 → 源码直接可读（不用还原）
#
# 【常见误报】
#   1. CDN 对 *.map 返回 200 的 HTML 错误页 → 上面 ①② 排除。
#   2. sourceMappingURL 指向内网地址（如 http://10.x/app.js.map）→ 那是开发
#      配置泄露，说明构建时没清理，但 map 本身取不到。要单独记录这条信号。
#   3. inline sourcemap（base64 data URI）→ 直接从 bundle 里 decode，不用请求。
# ============================================================================
set -uo pipefail
source "$(dirname "$0")/../_lib/common.sh"

BASE="${1:-}"; KNOWN="${2:-}"
[ -z "$BASE" ] && usage_die "$0 <https://TARGET> [js-path]"
BASE="${BASE%/}"
OUT="${OUTDIR:-./_sm_$(date +%s)}"; mkdir -p "$OUT"
preflight || exit 1

log "输出目录: $OUT"

# ---- 1) 找 JS 入口 --------------------------------------------------------
JS_LIST="$OUT/js.txt"; : > "$JS_LIST"
if [ -n "$KNOWN" ]; then
  echo "$BASE$KNOWN" >> "$JS_LIST"
else
  idx=$(req GET "$BASE/")
  printf '%s' "$idx" | grep -oE '(src|href)="[^"]+\.js[^"]*"' \
    | sed -E 's/^(src|href)="//; s/"$//' | while read -r u; do
        case "$u" in
          http://*|https://*) echo "$u" ;;
          //*) echo "https:$u" ;;
          /*) echo "$BASE$u" ;;
          *) echo "$BASE/$u" ;;
        esac
      done | sort -u >> "$JS_LIST"
  # webpack runtime 常见命名也补上
  for g in '/static/js/main.js' '/js/app.js' '/assets/index.js' '/static/index.js'; do
    c=$(code_of GET "$BASE$g")
    [ "$c" = "200" ] && echo "$BASE$g" >> "$JS_LIST"
  done
fi
sort -u "$JS_LIST" -o "$JS_LIST" 2>/dev/null || true
n=$(wc -l < "$JS_LIST")
log "发现 $n 个 JS 入口"
[ "$n" = 0 ] && { warn "未找到 JS 入口，用第二个参数直接指定"; exit 0; }

# ---- 2) 逐个查 sourceMappingURL ------------------------------------------
found=0; inline=0
while read -r ju; do
  [ -z "$ju" ] && continue
  body=$(req GET "$ju")
  [ -z "$body" ] && { dim "空响应: $ju"; continue; }

  # inline sourcemap
  if printf '%s' "$body" | grep -q 'sourceMappingURL=data:application/json'; then
    warn "inline sourcemap: $ju"
    printf '%s' "$body" | grep -oE 'sourceMappingURL=data:application/json[^ ]+' \
      | head -1 | sed 's#.*base64,##' | base64 -d 2>/dev/null > "$OUT/$(basename "$ju").inline.map"
    inline=$((inline+1))
    continue
  fi

  mu=$(printf '%s' "$body" | grep -oE 'sourceMappingURL=[^ *]+' | tail -1 | sed 's/^sourceMappingURL=//')
  [ -z "$mu" ] && { dim "无 sourceMappingURL: $(basename "$ju")"; continue; }

  # 内网地址泄露（即使取不到 map 也是非常强的信号）
  if printf '%s' "$mu" | grep -qE '^https?://(10\.|172\.(1[6-9]|2[0-9]|3[01])\.|192\.168\.)'; then
    warn "内网 map 地址泄露: $ju → $mu"
  fi

  case "$mu" in
    http://*|https://*) mapurl="$mu" ;;
    /*) mapurl="$BASE$mu" ;;
    *) mapurl="$(dirname "$ju")/$mu" ;;
  esac

  r=$(reqc GET "$mapurl")
  c="${r%%$'\t'*}"; b="${r#*$'\t'}"
  if [ "$c" = "200" ] \
     && printf '%s' "$b" | head -c 40 | grep -q '"version"' \
     && printf '%s' "$b" | grep -q '"mappings"'; then
    ok "SOURCE MAP 可取: $mapurl"
    fn="$OUT/$(echo "$mapurl" | md5sum | cut -c1-8).map"
    printf '%s' "$b" > "$fn"
    srcs=$(printf '%s' "$b" | jq -r '.sources|length' 2>/dev/null || echo '?')
    hasc=$(printf '%s' "$b" | jq -r 'if .sourcesContent then (.sourcesContent|length) else 0 end' 2>/dev/null || echo 0)
    dim "    sources=$srcs  sourcesContent=$hasc  → $fn"
    [ "$hasc" != "0" ] && ok "    ★ sourcesContent 含原文，无需还原即可读源码"
    found=$((found+1))
  else
    dim "map 不可取: $mapurl → $c"
  fi
  sleep "$SLEEP"
done < "$JS_LIST"

# ---- 3) webpack chunk 全量枚举 -------------------------------------------
log "检查 webpack chunk map"
rt=$(req GET "${BASE}/static/js/runtime~main.js")
[ -z "$rt" ] && rt=$(req GET "${BASE}/static/js/runtime-main.js")
if printf '%s' "$rt" | grep -qE '\{[0-9]+:"[0-9a-f]{8}"'; then
  ok "发现 chunk 映射表"
  chunks=$(printf '%s' "$rt" | grep -oE '[0-9]+:"[0-9a-f]{8}"' | tr -d '"' | tr ':' ' ')
  cnt=0
  while read -r cid h; do
    [ -z "$cid" ] && continue
    for pat in "/static/js/${cid}.${h}.chunk.js" "/static/js/${cid}.chunk.js" \
               "/static/js/chunk-${cid}.${h}.js"; do
      cc=$(code_of GET "$BASE$pat")
      if [ "$cc" = "200" ]; then
        echo "$BASE$pat" >> "$OUT/chunks.txt"
        cnt=$((cnt+1)); break
      fi
    done
  done <<< "$chunks"
  [ "$cnt" -gt 0 ] && ok "抓到 $cnt 个 chunk → $OUT/chunks.txt（可继续按第 2 步查各自的 map）"
else
  dim "未发现 runtime chunk 映射（可能是 v5 的 .u() 形式或非 webpack）"
fi

# ---- 4) 还原 -------------------------------------------------------------
echo
if [ "$found" -gt 0 ] || [ "$inline" -gt 0 ]; then
  cat <<EOF
════ 还原方法 ════
安装: npm i -g source-map-cli   (或 npx source-map-cli)

单份还原:
  npx source-map-cli  -o "$OUT/src"  <本地 .map 文件>

批量（webpack 产出的 sources 路径含 webpack:// 前缀，会被还原成目录）:
  find "$OUT" -name '*.map' -exec npx source-map-cli -o "$OUT/src" {} \;

如果 sourcesContent 有内容，最快的方式是直接用 jq 落盘:
  jq -r '.sources as \$s | .sourcesContent as \$c
         | range(0; \$s|length) as \$i
         | "=====\\( \$s[\$i] )=====\\n\\( \$c[\$i] )"'  xxx.map > all.txt

════ 还原后优先找 ════
  1. API 路径全集      grep -oE '"/[a-zA-Z0-9_/-]{3,}"' all.txt | sort -u
  2. 硬编码密钥        grep -rniE '(api[_-]?key|secret|token|aes|iv)\s*[:=]\s*["'"'"'][^"'"'"']{8,}'
  3. 内网地址          grep -oE 'https?://(10|172\.1[6-9]|192\.168)\.[0-9.]+' all.txt
  4. 注释掉的调试口    grep -rn 'TODO\|FIXME\|debugger\|console.log' all.txt | head
  5. 已下线的旧路由（本项目 POC 里有多条正是靠这一步找到的）
EOF
else
  warn "未发现可取的 source map"
  dim "① 生产常见做法是删除 .map，只留 sourceMappingURL 注释 —— 检查有没有内网地址泄露"
  dim "② 试 .map 的变体: app.js.map / app.js.LICENSE.txt / app-beautify.js"
  dim "③ 试从其他前置（测试环境域名、旧版 CDN、Wayback）取同版本产物"
fi
