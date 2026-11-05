# ERI：External Resource Intelligence（P0 已交付）

RTIS 的外部资源子系统。当前状态：**Awesome-Hacking 资源目录已入库（81 条），增量同步、去重、来源追踪、软删除、元数据层检索全部可用。**

## 定位

- 独立 sidecar（`db/catalog.db`），与内部知识库（`~/.dsh/redteam/knowledge.db`）完全分离，只加不改。
- 确定性工作（解析/规范化/去重/哈希/入库/增量）全部普通代码，零 LLM 参与。
- 每条资源必有来源（source_id / section / commit / 行号），无来源不入库。
- 资源从上游消失只标 `UNLISTED`，永不物理删除。
- 上游"列表"许可证（CC0-1.0）不代表子项目许可证；子项目许可核查属 P1（ResourceQualityEngine）。

## 用法

```bash
cd security-research-upgrade/eri

python3 -I eri.py sync                  # 增量同步上游（commit 未变则 2 次 API 调用即返回）
python3 -I eri.py search fuzzing        # 元数据层检索，默认 5 条（--limit 可扩，上限 20）
python3 -I eri.py get gh:secfigo/awesome-fuzzing
python3 -I eri.py stats                 # 目录统计
python3 -I eri.py report > reports/resource-inventory.md
python3 -I eri.py dedup-report          # 目录内疑似重复候选（D5 前置筛查）
python3 -I eri.py parse <readme.md>     # 干跑：只解析不落库
python3 -I tests/run_tests.py           # 离线测试套件（解析/去重/幂等/增量/软删除/检索）
```

环境变量：`ERI_DB` 覆盖数据库路径（测试用）。

## 资源身份与去重（D1-D4，已实现）

| 层 | 规则 | 实现 |
|---|---|---|
| D1 | URL 精确去重 | `resources.canonical_url UNIQUE` |
| D2 | URL 规范化 | 主机小写、去默认端口、去追踪参数（保守清单）、去锚点、去 `.git`、去尾斜杠、GitHub 收敛到 owner/repo |
| D3 | 仓库身份 | GitHub 资源 ID = `gh:owner/repo`（大小写不敏感）；重命名/转移/镜像的探测属 P1 |
| D4 | 内容哈希 | 快照 SHA-256 入 `sync_state`；commit+hash 均未变则整次同步为 no-op |
| D5 | 相似内容 | P1（先普通算法筛疑似，再人工/AI 复核；不确定一律保留） |

规范 URL 与原始 URL 分开保存：`resources.canonical_url`（身份）+ `resource_sources.source_url`（原始，含锚点/参数）。

## 数据表

`resources`（81 行）、`resource_sources`（来源多对多）、`resource_categories`（来自 README 真实章节结构，不依赖手写清单）、`resource_versions`（同步级版本）、`sync_state`（上游 commit/哈希/许可）、`resource_relationships`（P2，与 Skills/Workflow/知识关联）、`resource_reviews`（P1）、`events`（审计）。

## 检索预算（计划 §9.2）

默认返回 5 条，`--limit` 最多 20；只读元数据，不读上游原文。P2 起 Resource Resolver 挂在内部知识检索之后（内部优先，外部补充）。

## 已知边界（P0 刻意不做）

- 不深入子项目（不拉子仓库 README/meta，计划 P0 明令"只导入主仓库资源目录"）。
- D5 相似去重、License 核查、质量评分、Quarantine 属 P1。
- 与 Skills/Workflow 的自动关联属 P2。
- 第二个上游源（OWASP 等）属 P4；Adapter 接口届时从 `parse_readme`/`normalize_url`/`dedup_batch` 三个纯函数中抽取。

## 主机网络适配

本机对 `github.com` 主站 TLS 不稳，`api.github.com`/`codeload.github.com` 稳定。sync 只用 API 取 meta 与 commit（2 次调用），内容变更时走 codeload tarball，带重试与限流保护。

## P2（2026-11-05 已交付）

`eri_p2.py`：ResourceResolver（六维加权检索，零 LLM）+ RTIS gate 集成（`rtis.py` 尾部钩子，`RTIS_ERI=off` 可关）+ ResourceReader（ETag/内容哈希/章节缓存 + codeload tarball 回落）+ UsageTracker（七态漏斗）+ 三审核队列（D5/映射/License）+ 100k 性能器。

```bash
python3 -I eri_p2.py resolve --task-id T --keywords "landing,redirect" --json
python3 -I eri_p2.py read <resource_id> [--section 章节名]
python3 -I eri_p2.py track SELECTED --task-id T --resource-id R
python3 -I eri_p2.py funnel [--task-id T]
python3 -I eri_p2.py review-d5 | review-mappings | review-licenses
python3 -I tests/run_p2_tests.py
```

报告见 reports/p2-*.md（八份）。集成回滚备份：db/backups/rtis.py.pre-p2-*.bak。
