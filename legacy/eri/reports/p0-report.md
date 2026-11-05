# ERI P0 验收报告（Awesome-Hacking 资源接入器第一版）

日期：2026-11-05　执行：主会话（授权依据：ERI 实施计划 §18"可以自主执行"清单）
状态口径：全部数字来自本机实测命令，非推算。

## 一、交付物

| # | 交付 | 路径 | 状态 |
|---|---|---|---|
| 1 | 阶段一架构审计（12 问逐条回答 + 3 项既有问题记录） | `audit/current-architecture.md` | 已落盘 |
| 2 | 资源接入器（解析/规范化/去重/入库/同步/检索，单文件 stdlib） | `eri.py`（约 700 行） | 已落盘并实测 |
| 3 | sidecar 资源目录（8 张表，含 P1/P2 占位） | `db/catalog.db`（不进 git） | 已建并入库 |
| 4 | 离线测试套件（8 组，夹具驱动） | `tests/run_tests.py` + `fixtures/` | ALL PASS |
| 5 | 上游快照（README/LICENSE/meta/哈希，可追溯） | `data/upstream/awesome-hacking/440486fd08b0/` | 已落盘 |
| 6 | 资源清单 + 去重基线 | `reports/resource-inventory.md`、`reports/dedup-baseline.json` | 已生成 |

## 二、首次入库实测

| 指标 | 值 |
|---|---|
| 上游 commit | `440486fd08b0a0d9ad9a041a0bb234dcdf65dee2`（master，README.md） |
| 快照内容哈希 | `c1a2b188351480e9af92cfcc03e8eb8ad8fffce3f92fb076c06e5d0a2bcc82a2` |
| 解析记录 | 81 条（主列表 47 + 其他有用仓库 34），跳过 0 |
| 入库资源 | 81（github_repo 79 + github_page 2：CyberChef、GTFOBins） |
| 跨章节重复 | 0（该版本 README 两个章节无重叠仓库） |
| 来源行 | 81（每资源 1 条，含章节与行号） |
| 上游许可 | CC0-1.0（记录于 sync_state，仅指"列表"本身） |

## 三、成本控制实测（计划 §9.1/9.2 的 P0 部分）

1. **幂等**：同一 commit 重复 ingest，返回 `noop`（commit+hash 双校验），零解析、零写入、零 LLM。
2. **增量**：`sync` 实测只发 2 次 API 调用（repo meta + README commit），commit 未变即返回，不下载 tarball、不重解析。
3. **检索预算**：`search` 默认 5 条、上限 20（`SEARCH_MAX_LIMIT`），只读目录元数据，不触上游原文。
4. 全流程 LLM 调用数：**0**（解析、去重、入库、同步、检索全部确定性代码）。

## 四、测试结果（`python3 -I tests/run_tests.py`，离线）

| 组 | 覆盖 | 结果 |
|---|---|---|
| 1 规范化 | `.git`/大小写/子路径收敛/默认端口/追踪参数/锚点/协议外跳过（13 用例） | PASS |
| 2 解析 | 表格行/列表行/章节归属/行号/类型判定/排除规则 | PASS |
| 3 批内去重 | 跨章节合并、`.git` 变体、同名别名、名称差异记录 | PASS |
| 4 首次入库 | added/parsed 计数 | PASS |
| 5 幂等 | 二次入库 noop、无重复行 | PASS |
| 6 增量 | +1 增 / 1 改 / 2 软删（UNLISTED），来源历史保留，回滚重列恢复 INDEXED | PASS |
| 7 检索 | 命中排序、limit、空结果、get 含来源与分类 | PASS |
| 8 报告 | 清单/去重基线生成 | PASS |

开发中修复的真 bug（均已回归）：`resource_id_for` 双重 hexdigest；根路径尾斜杠未收敛；重新收录资源未从 UNLISTED 恢复。

## 五、对照 P0 批次（计划 §17）

| P0 项 | 状态 |
|---|---|
| 审查当前 RTIS 和数据库 | 完成（audit/current-architecture.md） |
| 设计 Resource Catalog | 完成（8 表 schema，SQL 见 eri.py SCHEMA） |
| 实现 Awesome-Hacking Adapter | 完成（sync/ingest；Adapter 抽象推迟到 P4 首个第二源时定型） |
| Markdown 解析 / URL 规范化 / 资源唯一 ID / 基本去重 / 来源追踪 | 完成（D1-D4） |
| 只导入主仓库资源目录，不深入子项目 | 遵守（子仓库零请求） |

## 六、P1 待办（按计划顺序）

1. ResourceQualityEngine：子项目 License/维护状态核查（需逐仓库 API，注意限流与分层频率）。
2. D5 相似去重（先普通算法，疑似才进人工/AI）。
3. 仓库重命名/转移探测（GitHub redirect 跟随）。
4. `resource_versions` 细化为逐资源版本。
