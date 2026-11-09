#!/usr/bin/env bash
# 网易易盾加固 APK 静态判别 + 明文符号提取
# 用法: ./poc.sh <app.apk> [关键词1,关键词2]
#   ./poc.sh target.apk signRequestParam,appSignKey,getHmacSha1
set -u
APK="${1:?用法: $0 <app.apk> [关键词,逗号分隔]}"
KW="${2:-sign,key,hmac,token,encrypt}"
TMP=$(mktemp -d); trap 'rm -rf "$TMP"' EXIT

[ -f "$APK" ] || { echo "[-] 文件不存在: $APK"; exit 1; }

echo "=== 1. 壳特征判别 ==="
CLASSES=$(unzip -l "$APK" 2>/dev/null | grep -cE 'classes[0-9]*\.dex')
LIBS=$(unzip -l "$APK" 2>/dev/null | grep -oE 'lib/[^/]+/[^ ]*\.so' | sed 's|.*/||' | sort -u)
echo "  dex 数量: $CLASSES"
echo "  so 库:"
echo "$LIBS" | grep -qi 'nesec\|nshelper' \
  && echo "    [+] 命中易盾特征 so (libnesec.so / libnshelper.so)" \
  || echo "    [-] 未见易盾特征 so，可能不是易盾加固"
unzip -l "$APK" 2>/dev/null | grep -q 'assets/nsecmetadata.a' \
  && echo "    [+] 命中 assets/nsecmetadata.a" \
  || echo "    [-] 无 assets/nsecmetadata.a"

echo ""
echo "=== 2. dex 头部与抽取判定 ==="
for i in $(seq 1 "$CLASSES"); do
  n=$([ "$i" -eq 1 ] && echo classes.dex || echo "classes$i.dex")
  unzip -p "$APK" "$n" > "$TMP/$n" 2>/dev/null || continue
  python3 - "$TMP/$n" "$n" <<'PY'
import sys, struct
p, name = sys.argv[1], sys.argv[2]
d = open(p,'rb').read()
if len(d) < 0x70: print(f"  {name}: 太小"); sys.exit()
magic = d[:8]
f_size  = struct.unpack_from('<I', d, 0x20)[0]
cdefs   = struct.unpack_from('<I', d, 0x58)[0]
cdef_off= struct.unpack_from('<I', d, 0x5C)[0]
sids_sz = struct.unpack_from('<I', d, 0x38)[0]
data_off= struct.unpack_from('<I', d, 0x6C)[0]
data_sz = struct.unpack_from('<I', d, 0x70)[0]
verdict = "疑似抽取加固" if (cdefs and cdefs < 200 and f_size == len(d)) else "结构正常"
print(f"  {name}: {len(d)}B  class_defs={cdefs}  string_ids={sids_sz}")
print(f"       data_off={data_off} data_size={data_sz}  → {verdict}")
PY
done

echo ""
echo "=== 3. 明文符号提取（易盾常把 string_data 段留在尾部）==="
for i in $(seq 1 "$CLASSES"); do
  n=$([ "$i" -eq 1 ] && echo classes.dex || echo "classes$i.dex")
  [ -f "$TMP/$n" ] || continue
  python3 - "$TMP/$n" <<'PY' > "$TMP/str.txt"
import re,sys
d=open(sys.argv[1],'rb').read()
seen=set()
for m in re.finditer(rb'[\x20-\x7e]{5,}', d):
    s=m.group().decode('latin1')
    if s not in seen:
        seen.add(s); print(s)
PY
  cnt=$(wc -l < "$TMP/str.txt")
  echo "  $n: 提取 $cnt 条可打印字符串"
done

echo ""
echo "=== 4. 关键词命中（签名链路线索）==="
if [ -f "$TMP/str.txt" ] && [ -s "$TMP/str.txt" ]; then
  echo "$KW" | tr ',' '\n' | while read -r k; do
    [ -z "$k" ] && continue
    hits=$(grep -ic -- "$k" "$TMP/str.txt" 2>/dev/null || echo 0)
    if [ "$hits" -gt 0 ]; then
      echo "  [+] '$k' 命中 $hits 条:"
      grep -i -- "$k" "$TMP/str.txt" | head -5 | sed 's/^/       /'
    else
      echo "  [-] '$k' 无命中"
    fi
  done
else
  echo "  [-] 未提取到字符串"
fi

echo ""
echo "边界: 拿到的是符号与常量，不等于拿到密钥。密钥可能运行时解码或多段拼接。"
