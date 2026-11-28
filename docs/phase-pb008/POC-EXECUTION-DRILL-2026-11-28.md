# PB-008 POC/Nuclei 独立执行演练证据（2026-11-28）

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
| nuclei 正例 | pb008-vuln（/.git 暴露） | 命中 1 项：[{"template_id": "git-directory-exposure-drill", "host": "127.0.0.1", "matched_at": "http://127.0.0.1:29857/.git/HEAD", "severity": "low"}] |
| nuclei 负例 | pb008-clean（无暴露对照） | 命中 0 项（预期 0） |
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
