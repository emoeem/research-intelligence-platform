# ERI 阶段一审计：现有 RTIS 架构（current-architecture）

日期：2026-11-05　审计人：主会话直接取证　方法：读文档 + 实查 DB schema/计数 + 读代码结构
范围：`~/.dsh/redteam/`（运行时）与 `~/code/pensting/security-research-upgrade/`（升级工程）
结论先行：**ERI 以独立 sidecar SQLite 落在升级工程内，不触碰 `knowledge.db`；复用其单写者纪律与事件日志模式，不复用其表结构。**

## 一、系统实况（实测，非文档转述）

| 项 | 实测值 | 取证方式 |
|---|---|---|
| Skills | 125 个（`~/.dsh/redteam/skills/`） | `ls \| wc -l` |
| 知识库 | `~/.dsh/redteam/knowledge.db`（SQLite，WAL） | `PRAGMA journal_mode` → `wal` |
| poc 表 | 14,717 行 | `SELECT count(*)` |
| k_meta（知识分层） | 28 行 | 同上 |
| k_event（事件） | 633 行 | 同上 |
| k_relation / k_negative / k_proposal / k_version | 34 / 17 / 1 / 15 | 同上 |
| 引擎 | `rtis/rtis.py`（任务/画像/门禁/路由）+ `scripts/rt.py`（知识读写唯一入口） | 代码结构 grep |
| 注册表 | `rtis/data/registry/{tools.json, workflows.yaml, skills-index.json, signal-catalog.yaml}` | 目录实查 |
| 任务目录 | `rtis/data/tasks/rtis-*`（JSON 状态） | 目录实查 |
| MCP 入口 | `scripts/rtis_mcp.mjs`（stdio JSON-RPC，动态 import 桥文件） | 读文件头 |

## 二、实施计划第三部分 12 问逐条回答

1. **知识库如何查询？** 三条路：`rt.py poc-search` CLI（FTS5 `poc_fts` + LIKE 兜底）；MCP 工具 `redteam_poc_search`（经 `rtis_mcp.mjs` 动态加载 rtis-bridge.js，2-gram + 证据等级加权）；`rtis.py kb_search`（门禁内部封装）。
2. **是否可 API 查询？** 是。`rt.py mcp` 提供 stdio JSON-RPC（MCP 2024-11-05），工具面与 zcode MCP 一致。
3. **是否有统一 Resource/Tool Registry？** 有雏形：`rtis/data/registry/tools.json`（M2 产物，工具级）；**没有资源目录**，这正是 ERI 补位。
4. **任务事件记录？** 有：`k_event` 表（633 条）+ `rtis.py log_event`（写 `rtis/data/tasks/*/events`）+ `tasks/*.json` 状态机文件。
5. **数据库？** SQLite 单文件（WAL）。
6. **事务？** 支持（SQLite 原生）。
7. **并发读？** 支持（WAL 模式读写不互斥）。
8. **安全并发写？** 无多写者协议；靠**单写者纪律**：`rt.py` 是唯一写入实现，MCP 写路径全部转调它。ERI 采用同一纪律（eri.py 唯一写入口）+ 独立库文件，从根本上避免写竞争。
9. **全文检索？** 有，FTS5（`poc_fts`，字段 title/cve/component/versions/tags/description/content）。
10. **向量检索？** 无。架构文档（`RTIS_System_Architecture.md` §九）明确不以向量库为唯一检索，本地优先、可解释。
11. **Skills 与知识库如何关联？** `k_meta.archetypes/technology_tags/method_tags` + `k_relation`（supports/contradicts/requires 等 8 型）+ skills front matter（`rt.py list_skills` 解析）。
12. **外部资源可否经现有系统调用？** 目前只有知识条目通道（POC/EXP），**没有外部资源层**。ERI 的 resource catalog 与 P2 的 Resource Resolver 即为此缺口设计。

## 三、与 ERI 直接相关的复用决定

| 计划要求 | 复用/新建 | 依据 |
|---|---|---|
| 不重建知识库 | 新建 sidecar `eri/db/catalog.db` | 计划 §5"不得直接改造现有知识库表结构" |
| 任务事件机制 | 复用模式（events 表 + JSON detail），不共用表 | k_event 已 633 条，混写会污染审计口径 |
| Tool Registry | 暂不合并；P2 通过 `resource_relationships` 桥接 | registry/tools.json 是工具运行面，ERI 是资源目录，语义不同 |
| 站型/知识检索 | P2 Resource Resolver 挂在 kb_search 之后（内部优先） | 计划 §11.1 查询顺序 |

## 四、审计中发现的既有问题（记录，不在本任务内处理）

1. **MCP 检索已降级**：`rtis-bridge.js` 在全部候选路径缺失（`~/.dsh/profiles/*/node_modules/dsh-purge-community/...` 无此目录；pensting `rtis/tools/` 也无），仅 `~/.dsh/dsh-purge-community/backups/` 有历史副本。实测本会话 `redteam_kb_stats` 返回"引擎不可用：未找到 rtis-bridge.js"。修复属 dsh 插件运行时，超出本任务授权（§18"修改无关项目"），仅记录。
2. **`catalog/repos-raw.tsv`（24 项目 API 元数据，11-02 产物）** 是现成第二批种子，P4 接入 GithubRepoAdapter 时可直接复用。
3. 本机对 github.com 主站 TLS 不稳（多次 `unexpected eof`），但 `api.github.com` 与 `codeload.github.com` 稳定：ERI 同步设计已按此选路（API 仅取 meta/commit，内容走 codeload tarball），与主机网络特性一致。

## 五、ERI 落位

```text
security-research-upgrade/eri/          # 升级工程内新增模块，不建平行系统
├── eri.py                              # 全部逻辑（stdlib-only，单写入口）
├── audit/current-architecture.md       # 本文
├── data/upstream/awesome-hacking/<sha> # 上游快照（README/LICENSE/meta/哈希）
├── db/catalog.db                       # sidecar 资源目录（不进 git）
├── tests/{run_tests.py, fixtures/}     # 离线测试套件
└── reports/                            # 清单与验收报告
```
