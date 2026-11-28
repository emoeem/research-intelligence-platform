#!/usr/bin/env bash
# Read-only DeepSeek/OpenAI-compatible Provider gate. Never prints secret values.
set -euo pipefail

fail=0

model="${RIP_LLM_MODEL:-}"
api_base="${RIP_LLM_API_BASE:-}"
key_env="${RIP_LLM_API_KEY_ENV:-OPENAI_API_KEY}"

if [[ -z "$model" ]]; then
  echo "[BLOCKED] RIP_LLM_MODEL is unset"
  fail=1
else
  echo "[PASS] RIP_LLM_MODEL is set"
fi

if [[ -z "$api_base" ]]; then
  echo "[BLOCKED] RIP_LLM_API_BASE is unset"
  fail=1
elif [[ "$api_base" =~ ^https:// ]] || [[ "$api_base" =~ ^http://(127\.0\.0\.1|localhost)(:|/) ]]; then
  echo "[PASS] RIP_LLM_API_BASE uses HTTPS or loopback HTTP"
else
  echo "[BLOCKED] RIP_LLM_API_BASE must use HTTPS or loopback HTTP"
  fail=1
fi

if [[ -z "${!key_env:-}" ]]; then
  echo "[BLOCKED] $key_env is unset or empty"
  fail=1
else
  echo "[PASS] $key_env is present (value not displayed)"
fi

if [[ "$model" == deepseek-* && "$api_base" != *api.deepseek.com* ]]; then
  echo "[BLOCKED] DeepSeek model name is set but API base is not api.deepseek.com"
  fail=1
fi

if [[ "$fail" -ne 0 ]]; then
  echo "LLM_READINESS=BLOCKED"
  exit 1
fi

echo "LLM_READINESS=READY"
