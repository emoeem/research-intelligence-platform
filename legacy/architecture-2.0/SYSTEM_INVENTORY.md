# 1. SYSTEM_INVENTORY：全系统清点（Architecture 2.0 / M0）

日期：2026-11-05 23:38-00:30 +0800　方法：只读实查（代码行数、DB schema/计数、网络探测），全部数字来自本机命令输出。

## 1.1 代码资产

| 模块 | 位置 | 规模 | 语言 | 状态 |
|---|---|---|---|---|
| RTIS 编排引擎 | `~/.dsh/redteam/rtis/rtis.py` | 1633 行 | Python stdlib | 生产运行中 |
| 知识读写唯一入口 | `~/.dsh/redteam/scripts/rt.py` | 1171 行 | Python stdlib | 生产运行中 |
| RTIS MCP 桥 | `~/.dsh/redteam/scripts/rtis_mcp.mjs` | 506 行 | Node | 生产（检索面降级，见 §1.4） |
| KB 检索引擎 | `~/.dsh/redteam/rtis/rtis_kb.mjs` | 104 行 | Node | 生产 |
| ERI P0 | `security-research-upgrade/eri/eri.py` | 749 行 | Python stdlib | 生产 |
| ERI P1 | `eri/eri_p1.py` | 1063 行 | Python stdlib | 生产 |
| ERI P2 | `eri/eri_p2.py` | 889 行 | Python stdlib | 生产 |
| ERI 测试 | `eri/tests/*.py` | 919 行 | Python stdlib | 三套件全绿 |
| 升级工程文档 | `security-research-upgrade/{rtis,knowledge,workflows,reports}` | 30+ 文档 | Markdown | 已交付 |

## 1.2 数据资产（三库并立 + 28 库孤岛）

| 库 | 表数 | 关键计数 | 角色 |
|---|---|---|---|
| `~/.dsh/redteam/knowledge.db` | 11 | poc 14,717（nuclei 占 95.9%）；k_meta 28；k_event 633；k_relation 34；k_negative 17 | 内部知识事实源（单写者 rt.py） |
| `eri/db/catalog.db` | 15 | resources 81；github_metadata 79；review_log 165；quality_scores 81；skill_mappings 18 | 外部资源事实源（单写者 eri*.py） |
| `engagements/*/assets.db` × 28 | 15+ each | asset/observation/http_evidence/vuln/attack_step/credential/tunnel/webshell… | 每靶标独立事实源，互不可查 |
| `rtis/data/tasks/*.json` | 文件 | 任务状态机 + 事件流 | 编排层状态 |
| `rtis/data/registry/*` | 4 文件 | tools 20、workflows WF-*、signals、skills-index | 方法层注册表 |

## 1.3 能力成熟度总表（已实现 / 部分 / 仅设计 / 缺失）

| 能力 | 状态 | 证据 |
|---|---|---|
| 资源接入/去重/增量同步 | 已实现 | ERI P0，81 条，幂等+304 实测 |
| 元数据补全/质量评分 | 已实现 | P1，79/79，A4/B28/C44/D5 |
| 资源检索（六维加权） | 已实现 | P2，resolve 1.2ms@81 / 455ms@100k |
| RTIS 知识门禁 | 已实现 | rtis.py gate()，633 事件 |
| ERI↔RTIS 集成 | 已实现 | gate 尾部钩子，端到端实测 |
| 资源按需阅读+缓存 | 已实现 | P2 Reader，tarball 回落 |
| 使用漏斗追踪 | 部分实现 | 七态表就位；SELECTED/APPLIED 零真实数据 |
| 知识检索（2-gram+K 级） | 已实现 | rtis-bridge.js 算法；但 zcode 侧桥缺失（§1.4） |
| 证据分级 K0-K4 | 部分实现 | k_meta 28 条有级；poc 14,717 条无级 |
| 站型画像/路由 | 部分实现 | signal-catalog v2 + workflows.yaml 存在；route() 有实现；画像写入 gate 链路已验证 |
| 工具注册表 | 部分实现 | 20 工具有 readonly 标记；fscan 版本探测坏、gogo 未装标记 installed=false；无执行日志 |
| 任务后复盘（AAR） | 已实现 | rtis.py aar() + k_proposal 流程 |
| 跨库统一查询 | 缺失 | 三库并立，无联邦层 |
| 统一审计 | 部分实现 | 三处独立事件流（k_event/eri events/rtis tasks），无关联 ID |
| 向量检索 | 缺失 | 全系统无 embedding |
| UI | 缺失 | 纯 CLI/MCP |
| Agent 运行时 | 部分实现 | zcode harness + MCP + 五角色稿；无独立进程运行时 |

## 1.4 环境事实（影响选型）

- 本机网络：github.com 主站、crates.io index（git/sparse）、rustup 均间歇 TLS EOF；**static.crates.io 稳定 200**；api.github.com 2026-11-05 晚间起不稳；codeload 稳定。
- Rust 工具链已装（cargo/rustc 于 /usr/sbin）；Python 3.14 系统级可用。
- 结论：Rust 构建可行但依赖 vendoring/镜像策略是硬前提（详见 TECHNOLOGY_DECISION.md §3）。

## 1.5 已知缺陷清单（审计中发现，M1+ 待修）

1. rtis-bridge.js 在 zcode 侧全部候选路径缺失 → MCP 知识检索降级（副本仅存备份目录）。
2. tools.json 中 fscan 版本探测命令错误（"flag needs an argument"）、gogo installed=false 但 toolkit 目录存在二进制。
3. poc 表 99.12% 未验证（13/14717 verified），无证据等级。
4. 6/125 skills 缺 whenToUse/role/enabled 字段；tags 覆盖仅 6 个。
5. 28 个 engagement 库结构同源但无 schema 版本号，无法安全批量升级。
6. 三套事件流（k_event/eri events/task events）无统一关联 ID。
