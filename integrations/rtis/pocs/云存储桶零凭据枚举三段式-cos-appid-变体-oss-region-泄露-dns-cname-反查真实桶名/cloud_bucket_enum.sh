#!/bin/bash
# 云存储桶零凭据枚举三段式 —— 全部只读（HEAD/GET），不做目录深挖、不猜对象 key
# 依赖: curl dig
set -u

probe() { # $1=host
  local h="$1" code body
  for u in "/" "/?prefix=&max-keys=10" "/?list-type=2"; do
    code=$(curl -s -o /tmp/_b -w '%{http_code}' --max-time 10 -A 'Mozilla/5.0' "https://$h$u")
    body=$(head -c 200 /tmp/_b | tr -d '\n\r\t')
    case "$code" in
      200) case "$body" in
             *ListBucketResult*) echo -e "VULN\t200-listable\t$h$u";;
             *)                  echo -e "open\t200-other\t$h$u";;
           esac;;
      403) echo -e "private\t403\t$h$u";;
      404) echo -e "absent\t404\t$h$u";;
      *)   echo -e "other\t$code\t$h$u";;
    esac
  done
  rm -f /tmp/_b
}

cos_enum() { # $1=appid $2=region  (names.txt 每行一个名字)
  while read -r n; do
    echo "$n-$1.cos.$2.myqcloud.com"
  done < names.txt | xargs -P 15 -I{} bash -c 'c=$(curl -s -o /dev/null -w "%{http_code}" --max-time 10 "https://{}/?max-keys=1"); [ "$c" != "404" ] && echo -e "$c\t{}"'
}

oss_enum() { # $1=region ; 403 时提取 <Endpoint> 切区重测
  while read -r n; do
    echo "$n.oss-$1.aliyuncs.com"
  done < names.txt | xargs -P 15 -I{} bash -c '
    h="{}"; c=$(curl -s -o /tmp/_o -w "%{http_code}" --max-time 10 "https://$h/?max-keys=1")
    if [ "$c" != "404" ]; then
      ep=$(grep -o "<Endpoint>[^<]*" /tmp/_o | sed "s/<Endpoint>//")
      if [ -n "$ep" ]; then
        # 关键：换到泄露出来的真实 region 再测一次，否则公开桶会被误判成 403
        b="${h%%.oss-$1.aliyuncs.com}"
        c2=$(curl -s -o /dev/null -w "%{http_code}" --max-time 10 "https://$b.$ep/?max-keys=1")
        echo -e "$c\t->\t$c2\t$b @ $ep"
      else
        echo -e "$c\t$h"
      fi
    fi; rm -f /tmp/_o'
}

cname_scan() { # $1=hosts.txt ; 反查真实存储域名
  while read -r h; do
    c=$(dig +short CNAME "$h" 2>/dev/null | head -1)
    case "$c" in
      *qiniudns.com*|*aliyuncs.com*|*myqcloud.com*|*bdydns.com*) echo -e "$h\t$c";;
    esac
  done < "$1" | sort -u
}

case "${1:-}" in
  probe) shift; for h in "$@"; do probe "$h"; done ;;
  cos)   cos_enum "${2:?appid}" "${3:-ap-guangzhou}" ;;
  oss)   oss_enum "${2:-cn-shanghai}" ;;
  cname) cname_scan "${2:?hosts.txt}" ;;
  *) echo "usage: $0 {probe <host...>|cos <appid> [region]|oss [region]|cname <hosts.txt>}"; exit 1;;
esac
