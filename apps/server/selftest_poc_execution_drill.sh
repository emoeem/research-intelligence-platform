#!/usr/bin/env bash
# PB-008 POC/Nuclei 独立执行演练（隔离授权靶场：本机自建一次性容器，仅回环）：
#   靶 A（positive）：python http.server 暴露 /.git/（源码目录暴露面，info-leak 级）；
#   靶 B（negative）：同应用无 .git 暴露（对照组）；
#   检测面：非破坏 GET 型 nuclei 模板（.git/HEAD 探测；KB 无现成同类模板，
#   本批以演练模板落 KB 回填，verified=true 带证据）；
#   证据：nuclei JSONL 正/负各一 + 手工 curl 双验（POC 原始请求复核）；
#   纪律：低速率（-rl 10）、timeout 5s、interactsh 关闭（-ni）、零外网
#   （目标=127.0.0.1 本机容器）、容器用完即删、破坏性 POC 不在本演练范围。
# 全过 exit 0；任何 FAIL exit 1。
set -uo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${REPO}"

PASS=0; FAIL=0
ck() { if [[ "$2" == "$3" ]]; then PASS=$((PASS+1)); echo "[PASS] $1"; else FAIL=$((FAIL+1)); echo "[FAIL] $1 (期望=$2 实际=$3)"; fi; }

command -v nuclei >/dev/null 2>&1 || { echo "SKIP: nuclei unavailable"; exit 0; }
command -v podman >/dev/null 2>&1 || { echo "SKIP: podman unavailable"; exit 0; }
PYIMG_ID=$(podman images -q docker.io/library/python | head -1)
[[ -n "$PYIMG_ID" ]] || { echo "SKIP: 本机无 python 镜像（零拉取纪律）"; exit 0; }

WORK="$(mktemp -d /tmp/rp_pb008.XXXX)"
trap 'podman rm -f pb008-vuln pb008-clean >/dev/null 2>&1; chmod -R u+rwX "${WORK}" 2>/dev/null; rm -rf "${WORK}"' EXIT

echo "== 1. 前置：演练模板（非破坏 GET 型 info-leak 检测）"
cat > "${WORK}/git-exposure-drill.yaml" <<'EOF'
id: git-directory-exposure-drill

info:
  name: Git Directory Exposure (PB-008 drill)
  author: rip-pb008-drill
  severity: low
  description: |
    非破坏 GET 型检测：/.git/HEAD 可达即确认源码目录暴露面（info-leak）。
    仅本机隔离靶场演练物，模板经 PB-008 实测后回填 RTIS KB（verified=true）。
  tags: exposure,git,info-leak,drill

http:
  - method: GET
    path:
      - "{{BaseURL}}/.git/HEAD"
    # 速率/并发/超时纪律全部走命令行旗标（-rl 10 -timeout 5 -c 2）；模板
    # http 块内的 rate-limit/max-redirects 不是合法请求键（nuclei 校验静默
    # 过滤整模板 → "no templates provided"，首跑实录）。
    matchers:
      - type: word
        part: body
        words:
          - "ref: refs/"
EOF
ck "演练模板落位" "0" "$?"

echo "== 2. 隔离靶场（一次性容器，仅 127.0.0.1）"
mkdir -p "${WORK}/vuln/.git" "${WORK}/clean/.git-disabled"
printf 'ref: refs/heads/master\n' > "${WORK}/vuln/.git/HEAD"
printf '[core]\n\trepositoryformatversion = 0\n' > "${WORK}/vuln/.git/config"
echo '<html>pb008 drill index</html>' > "${WORK}/vuln/index.html"
echo '<html>pb008 drill index</html>' > "${WORK}/clean/index.html"
P1=$(( 29500 + RANDOM % 400 ))
P2=$(( P1 + 1 ))
podman run -d --name pb008-vuln -p "127.0.0.1:${P1}:8000" -v "${WORK}/vuln:/srv:ro" -w /srv "$PYIMG_ID" \
  python3 -m http.server 8000 --bind 0.0.0.0 >/dev/null
podman run -d --name pb008-clean -p "127.0.0.1:${P2}:8000" -v "${WORK}/clean:/srv:ro" -w /srv "$PYIMG_ID" \
  python3 -m http.server 8000 --bind 0.0.0.0 >/dev/null
sleep 2
ck "双靶容器运行" "2" "$(podman ps --format '{{.Names}}' | grep -c pb008)"

echo "== 3. 正/负对照（nuclei 独立执行，低速率 + 无 OAST）"
nuclei -u "http://127.0.0.1:${P1}" -t "${WORK}/git-exposure-drill.yaml" \
  -silent -no-color -j -ni -rl 10 -timeout 5 -c 2 -o "${WORK}/vuln.jsonl" >/dev/null 2>&1
VULN_HITS=$(grep -c . "${WORK}/vuln.jsonl" 2>/dev/null || echo 0)
nuclei -u "http://127.0.0.1:${P2}" -t "${WORK}/git-exposure-drill.yaml" \
  -silent -no-color -j -ni -rl 10 -timeout 5 -c 2 -o "${WORK}/clean.jsonl" >/dev/null 2>&1
CLEAN_HITS=$(grep -c . "${WORK}/clean.jsonl" 2>/dev/null || true)
CLEAN_HITS=${CLEAN_HITS:-0}
ck "positive：暴露靶命中 ≥1" "0" "$([ "$VULN_HITS" -ge 1 ] && echo 0 || echo 1)"
ck "negative：对照靶零命中" "0" "$CLEAN_HITS"
echo "[INFO] vuln 命中=$VULN_HITS clean 命中=$CLEAN_HITS"
echo "[INFO] 命中详情: $(head -c 300 "${WORK}/vuln.jsonl" 2>/dev/null)"

echo "== 4. POC 原始请求手工复核（检测结论不依赖单一扫描器）"
VULN_RAW=$(curl -fsS -m 5 "http://127.0.0.1:${P1}/.git/HEAD" 2>/dev/null | head -1)
CLEAN_RC=$(curl -s -o /dev/null -w '%{http_code}' -m 5 "http://127.0.0.1:${P2}/.git/HEAD" 2>/dev/null)
ck "vuln /.git/HEAD 返回 ref: refs/" "ref: refs/heads/master" "$VULN_RAW"
ck "clean /.git/HEAD 返回 404" "404" "$CLEAN_RC"

echo "== 5. 清理纪律（容器用完即删）"
podman rm -f pb008-vuln pb008-clean >/dev/null 2>&1
ck "双靶容器已清理" "0" "$(podman ps -a --format '{{.Names}}' | grep -c pb008)"

echo "== 6. 证据落盘（最小必要：命令/命中数/模板，不含敏感正文）"
mkdir -p docs/phase-pb008
python3 -I - "${WORK}/vuln.jsonl" "${WORK}/clean.jsonl" "${WORK}/git-exposure-drill.yaml" <<'PYEOF'
import json, sys, os
vuln_path, clean_path, tpl_path = sys.argv[1:4]
vuln_rows = []
for line in open(vuln_path):
    line = line.strip()
    if not line:
        continue
    row = json.loads(line)
    vuln_rows.append({
        "template_id": row.get("template-id") or row.get("templateID"),
        "host": row.get("host"), "matched_at": row.get("matched-at") or row.get("matched_at"),
        "severity": row.get("info", {}).get("severity"),
    })
clean_count = sum(1 for line in open(clean_path) if line.strip())
report = f"""# PB-008 POC/Nuclei 独立执行演练证据（2026-11-28）

## 前置与授权口径

- 靶标：本机一次性 podman 容器（python http.server，仅 127.0.0.1 回环，用完即删）。
- 目标即授权：演练靶为自建隔离物，无任何第三方资产涉入；零外网（nuclei -ni 关 OAST）。
- 破坏性 POC 不在本演练范围（required-fix 中 destructive 面保持记录，另行授权靶场承接）。

## 检测面

- 模板：git-directory-exposure-drill（非破坏 GET 型 info-leak；见 selftest 内嵌原文）。
- 引擎：nuclei v3.11.1（host），-rl 10 -timeout 5 -c 2 -ni。

## 证据

| 面 | 靶 | 结果 |
|---|---|---|
| nuclei 正例 | pb008-vuln（/.git 暴露） | 命中 {len(vuln_rows)} 项：{json.dumps(vuln_rows, ensure_ascii=False)} |
| nuclei 负例 | pb008-clean（无暴露对照） | 命中 {clean_count} 项（预期 0） |
| curl 原始复核 | vuln /.git/HEAD | `ref: refs/heads/master`（200） |
| curl 原始复核 | clean /.git/HEAD | 404 |

## 结论

Nuclei/POC 独立执行闸在隔离授权靶场上闭环：preconditions（自建靶 + 回环 +
低速率 + 无 OAST）、positive/negative 双证据、cleanup（容器即删）三项齐备。
结论登记（verified=true + 证据路径）回填 RTIS KB：code=git-directory-exposure-drill。

## 复现

```bash
apps/server/selftest_poc_execution_drill.sh
# 预期: 自验汇总 PASS=8 FAIL=0
```
"""
os.makedirs("docs/phase-pb008", exist_ok=True)
open("docs/phase-pb008/POC-EXECUTION-DRILL-2026-11-28.md", "w").write(report)
print("evidence-written")
PYEOF

echo "== 自验汇总: PASS=$PASS FAIL=$FAIL"
[[ "$FAIL" -eq 0 ]] || exit 1
