#!/usr/bin/env bash
# PB-006 LLM Provider 就绪探针三态自验（只读探针 scripts/rip_llm_readiness.sh，
# 不发任何网络请求、不打印密钥明文）：
#   1. 全缺省 → BLOCKED exit 1；
#   2. 合法形状（loopback base + 假 key）→ READY exit 0；
#   3. key env 未设 → BLOCKED exit 1；
#   4. deepseek-* 模型 + 非 deepseek base → BLOCKED exit 1；
#   5. 输出永不回显密钥明文（假 key 值不得出现在 stdout/stderr）。
# live Provider E2E 仍为条件授权（需真实 RIP_LLM_* 凭据，PB-006 P2/Optional）。
# 全过 exit 0；任何 FAIL exit 1。
set -uo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${REPO}"
PROBE="scripts/rip_llm_readiness.sh"

PASS=0; FAIL=0
ck() { if [[ "$2" == "$3" ]]; then PASS=$((PASS+1)); echo "[PASS] $1"; else FAIL=$((FAIL+1)); echo "[FAIL] $1 (期望=$2 实际=$3)"; fi; }

WORK="$(mktemp -d /tmp/rp_llm_probe.XXXX)"
trap 'rm -rf "${WORK}"' EXIT

echo "== 1. 全缺省 → BLOCKED"
env -i PATH="$PATH" bash "$PROBE" > "${WORK}/m1.out" 2>&1
ck "全缺省 exit 1" "1" "$?"
grep -q "LLM_READINESS=BLOCKED" "${WORK}/m1.out" && ck "BLOCKED 文案" "0" "0" || ck "BLOCKED 文案" "0" "1"

echo "== 2. 合法形状（假 key、loopback base）→ READY"
export RIP_LLM_MODEL="test-model"
export RIP_LLM_API_BASE="http://127.0.0.1:18080/v1"
export RIP_LLM_API_KEY_ENV="RP_SELFTEST_FAKE_KEY"
export RP_SELFTEST_FAKE_KEY="rp1_fakekey_DO_NOT_PRINT_0123456789abcdef"
bash "$PROBE" > "${WORK}/m2.out" 2>&1
ck "合法形状 exit 0" "0" "$?"
grep -q "LLM_READINESS=READY" "${WORK}/m2.out" && ck "READY 文案" "0" "0" || ck "READY 文案" "0" "1"
if grep -q "rp1_fakekey_DO_NOT_PRINT" "${WORK}/m2.out"; then ck "密钥明文零回显" "leaked" "clean"; else ck "密钥明文零回显" "clean" "clean"; fi

echo "== 3. key env 未设 → BLOCKED"
unset RP_SELFTEST_FAKE_KEY
bash "$PROBE" > "${WORK}/m3.out" 2>&1
ck "key 缺失 exit 1" "1" "$?"
grep -q "RP_SELFTEST_FAKE_KEY is unset" "${WORK}/m3.out" && ck "点名缺失 key env" "0" "0" || ck "点名缺失 key env" "0" "1"

echo "== 4. deepseek 模型 + 非 deepseek base → BLOCKED"
export RIP_LLM_MODEL="deepseek-chat"
export RIP_LLM_API_BASE="https://example.invalid/v1"
export RP_SELFTEST_FAKE_KEY="rp1_fakekey_DO_NOT_PRINT_0123456789abcdef"
bash "$PROBE" > "${WORK}/m4.out" 2>&1
ck "deepseek 错配 exit 1" "1" "$?"
grep -q "api.deepseek.com" "${WORK}/m4.out" && ck "错配文案点名 deepseek" "0" "0" || ck "错配文案点名 deepseek" "0" "1"

echo "== 5. 非 HTTPS 非 loopback base → BLOCKED"
export RIP_LLM_MODEL="test-model"
export RIP_LLM_API_BASE="http://10.0.0.1:18080/v1"
bash "$PROBE" > "${WORK}/m5.out" 2>&1
ck "非 HTTPS/loopback base exit 1" "1" "$?"

unset RIP_LLM_MODEL RIP_LLM_API_BASE RIP_LLM_API_KEY_ENV RP_SELFTEST_FAKE_KEY
echo "== 自验汇总: PASS=$PASS FAIL=$FAIL"
[[ "$FAIL" -eq 0 ]] || exit 1
