# M1 Readiness Review（M1 就绪评审）

| 项 | 值 |
|---|---|
| 日期 | 2026-11-06 |
| 执笔 | M1 文档撰写员（动态工作流子代理） |
| 依据计划书 | `/home/emo/Downloads/RESEARCH_INTELLIGENCE_PLATFORM_MASTER_PLAN.md`（§2.4 / §13 / §M1 / §11 / 附录 B） |
| 评审对象 | Architecture 2.0 M0 交付（architecture-2.0/ 下 13 份报告）、DEC-001..005、ERI/RTIS 现网数据基线、28 库、rtis-bridge 缺失影响 |
| 准入判断 | **ready_with_conditions**（条件见 §4） |
| 配套文档 | `M1_DECISION_LOG.md`、`DATA_BASELINE_SNAPSHOT.md`、`M1_IMPLEMENTATION_PLAN.md`（同目录） |

**证据标签约定**（写作纪律：区分已核实事实与建议）：

- `[计划 §x]`：计划书原文，行号以本文件引用的 1324 行版本为准。
- `[实测]`：本评审在本会话实际执行的命令与输出，命令全文见 `DATA_BASELINE_SNAPSHOT.md` 可重放命令节。
- `[侦察:…]`：本工作流侦察员交付的只读快照（world.run sqlite3 直查 / 文件系统扫描），本评审仅复验其中关键锚点，未逐项复放；未复放项均显式标注"侦察证据，未本机复放"。
- `[建议]`：评审建议，批准权在项目负责人，不构成决定。

---

## 0. 结论摘要

1. 十步执行清单（[计划 §13:1129-1142]）全部完成侦察与评审，其中步 2/6 的关键数字已由本评审实测复放（复现率 100%，见 `DATA_BASELINE_SNAPSHOT.md`）。
2. 无阻塞项：rtis-bridge.js 缺失不阻塞 M1（隔离 Adapter 兜底可用）；28 库无未知结构（26 同构 + 1 异构已知差异 + 1 无库）。
3. 未解决冲突/缺口台账共 17 项（§2 K-01..K-17），全部有明确归属：已裁定 1 项（K-01，含两处「13」笔误）、待批勘误 1 项（K-02）、待人工确认 2 项（K-03/K-12）、移交 M1-B 差异决策 9 项（K-04/K-05/K-07/K-08/K-09/K-10/K-11/K-16/K-17）、不阻塞记录 3 项（K-06/K-13/K-14）、已补齐 1 项（K-15）。
4. 风险登记册 15 项中 8 项被本次证据更新（§3）：R-06 为关闭候选，R-01/R-02/R-03/R-04/R-05/R-07/R-08 获得新证据或实证细化；其余 7 项（R-09..R-15）维持原状。
5. M1 准入判断为 **ready_with_conditions**：前置条件是 C0 M0 架构（13 份报告）批准（[计划 §15:1222] 第一顺位）、DEC-001..005/006 签署、M1 实施计划批准稿、M0 勘误（两处「13」笔误 + 一处断引）获批修订、冻结条款生效确认。

---

## 1. 十步执行清单逐步结论（对应计划 §13）

### 步 1：冻结 F0 边界（状态条款）

**结论：以下作为项目状态条款声明，不构成本评审的可验证事实，生效以项目负责人确认为准。**

- 自本文档提交之日起，ERI P2 处于功能冻结状态：只允许 [计划 §2.2:103-108] 列出的维护活动（数据安全/完整性/可用性缺陷修复、可运行可导出可审计保障、迁移准备所必需修复、测试文档快照补充）；§2.2:110-114 所列扩张类活动默认停止；例外须由项目负责人批准。
- 佐证现状：文件 mtime（[实测] `ls -la`）显示 knowledge.db 为 2026-11-05 04:25、catalog.db 为 2026-11-05 23:28。注意 mtime 无法区分业务写入与 checkpoint/维护性写入，仅作粗信号；[实测] 两库当前均无未合并的 -wal/-shm 伴生文件（目录中仅历史备份文件带 -wal/-shm）。
- 每周冻结检查的可判定判据：以 `DATA_BASELINE_SNAPSHOT.md` §1 库 SHA-256 与 §2 关键计数为锚复查是否变化；有变化时逐项核对是否属 [计划 §2.2:103-108] 允许的维护活动，由项目负责人裁定；mtime 与 -wal/-shm 状态仅作辅助线索。
- 待办：项目负责人确认冻结条款生效，计入 M1 准入条件 C7。

### 步 2：校验 M0 交付

**结论：13 份报告全部在位且未损坏；存在 1 处断引与若干笔误/口径问题，不构成返工条件。**

- 完整性：present=13、missing=[]、共 728 行 [侦察:M0交付校验]。本机复核目录清点 `ls architecture-2.0/*.md` = 13 份 [实测]。
- 断引 1 项：`SYSTEM_INVENTORY.md:55` 引用「TECHNOLOGY_DECISION.md §3」，该文件仅含 §9.1-§9.6，所指 Rust/vendoring 网络前提内容实际位于 §9.1（grep 标题核实）[侦察:M0交付校验]。
- 数字矛盾（已裁定，见步 6）：SYSTEM_INVENTORY.md:61「poc 表 99.12% 未验证（13/14717 verified）」中的 `13` 为笔误，实测 verified=1 共 130 条 [实测]，与同句 99.12% 及 KNOWLEDGE_BASE_REVIEW.md:11「仅 130 (0.88%)」自洽。
- 其余 5 项问题的去向：「七域」无定义（§2 K-03）、UsageEvent/ResourceUsage 关联命名不一（由 §2 K-10 的实体定义差异承载）；其余三项（KNOWLEDGE_BASE_REVIEW.md:11 单元格多余字符「1」、DEC 编号仅靠 §9.6 声明对应、离库引用不可核实）为低风险笔误或外部文档引用限制，不单列台账行，随 K-01/K-02 勘误批次一并处置或留档备查。

### 步 3：审阅总架构

**结论：FINAL_ARCHITECTURE 与 UNIFIED_DOMAIN_MODEL 的方向与计划一致，但存在多项证据性缺口。统一台账为 §2 表 K-01..K-17，其中 9 项移交 M1-B 差异决策（K-04/K-05/K-07/K-08/K-09/K-10/K-11/K-16/K-17），K-06/K-13/K-14 不阻塞 M1，K-01/K-02 为 M0 勘误，K-03/K-12 待人工确认，K-15 已补齐。**

决定性缺口（逐项标注 §2 台账编号）：

1. M1 建表范围自相矛盾：风险表写「M1 先落 8 个有现网数据的实体」（FINAL_ARCHITECTURE.md §13.5），路线图交付物写「platform.db schema（15 实体+alias+schema_version）」（IMPLEMENTATION_ROADMAP.md M1 行）[侦察:架构评审]（§2 K-04）。
2. DEC-005 未做任何 PoC 即否决 GPUI 并定本地 Web UI，违反 [计划 §M7]「不在没有验证前预设最终答案」（§2 K-06，不阻塞 M1，M7 前闭环）。
3. 自设「治理状态与生命周期状态双轴正交」原则被 Knowledge 10 态打破（QUARANTINED/REJECTED/CONFLICTED 嵌入生命周期链）（UNIFIED_DOMAIN_MODEL.md §7.0 vs §7.3）（§2 K-08）。
4. 别名机制仅单个「alias 列」，缺 [计划 §5.4:385] 要求的来源系统/来源命名空间/有效时间/冲突检查四要素；§7.13「RTIS-…」与实测 task_id 格式（小写 `rtis-20261102-220853-7827`）不符（§2 K-09）。
5. Resource 生命周期链无验证维度（缺 FETCHED/VERIFIED），不满足 [计划 §5.2:335]「区分资源发现与资源验证」与 §5.3 状态机约束（§2 K-16）。
6. M0 路线止于 M7，无计划 §M8 受控切换阶段；rtis-bridge.js 被定为 P0 却无任何阶段的修复安排（§2 K-13/K-14，均不阻塞 M1，后者由步 8 补齐）。

**模块依赖核对（[计划 §M1-A:602] 要求项，本评审补充执行，文档级 [实测]）**：

- 检查对象：MODULE_BOUNDARIES.md 全文（52 行）逐行读取；检查范围声明：静态文档级核对，未做代码级 import 图分析（代码级验证列入 M2-A workspace 拆分验收项）。
- 依赖规则为无环分层设计：domain 零依赖、被所有模块依赖（:9/:23）；database 只被各 engine 依赖（:24）；engine 之间不互读库、只走接口或事件（:24）；audit-engine 为旁路（:26）；integrations 单向消费公开接口（:27）。规则声明层面未发现循环依赖。
- 跨库/跨模块耦合点 6 处，均有边界修正条目或归属阶段：eri.py/eri_p1/eri_p2 单文件 import 链与 DB_PATH 全局变量（:33，M2 拆分对象）；rt.py 作为 knowledge 唯一写者（:34）；gate 的 ERI subprocess 钩子（:35）；assets.db×28 不合并、经 Evidence 全局 ID 回填桥接（:38，对应 §2 K-11）；rtis-bridge.js 断链（步 8 已覆盖）；d5_candidates/skill_mappings 计算归属 resource-engine（:11）而审核归属 quality-engine（:14），两表所有权未在 §8.1 明确划分，列为 M1-B 数据字典必答项。

### 步 4：审阅 DEC-001..005

**结论：评审建议为 1 项 accepted、4 项 accepted_with_conditions；全部以 Status: Proposed 记录，批准权在项目负责人。**

逐项结论与条件见 `M1_DECISION_LOG.md`（按计划附录 B 模板）。摘要：

| ID | 主题 | 评审建议 | 关键条件 |
|---|---|---|---|
| DEC-001 | Rust 分阶段 + vendored 回退 | accepted_with_conditions | M2 开工前完成一次可执行 vendored 离线 clean build |
| DEC-002 | SQLite 持续 + PG 触发条件 | accepted_with_conditions | M1-B 定案单库 platform.db；启用外键/事务/备份检查 |
| DEC-003 | FTS5+2-gram、向量暂缓 | accepted | 无（M4 试产评估命中率） |
| DEC-004 | 内部 JSON-RPC / 对外 MCP+HTTP | accepted_with_conditions | 补版本化契约 Schema 与演进策略 |
| DEC-005 | 本地 Web UI | accepted_with_conditions | M7 前补最小 PoC（含移动端/可访问性） |

### 步 5：核对域模型清单

**结论：15 实体清单可确认；AuthorizationRecord 建议实体化为第 16 实体（DEC-006 草案）；其余 9 项实体差异经 §2 台账（K-09/K-10/K-11/K-16/K-17）全部移交 M1-B。**

- 10 项实体缺口 [侦察:架构评审 entityGaps]：AuthorizationRecord 缺实体（现网 task JSON 已内嵌 authorization 块，M0 字段草案互不覆盖，走 DEC-006）；NegativeKnowledge 无独立治理对象（违反 [计划 §10.2]，§2 K-17）；ToolExecution 引用悬空、UsageEvent/ResourceUsage/ResourceSource/SkillMapping 无定义、SkillVersion 缺位、Workflow/Agent 保留策略字段缺落点、ResearchTask 状态机暂停/恢复语义未确认（均归 §2 K-10）；Resource 生命周期缺验证维度（§2 K-16）；ID 别名三处待修（§2 K-09）；Evidence 桥表 schema 未定义（§2 K-11）。
- 三类现网 ID 别名落点已核实：poc.code→Knowledge、gh:/url:→Resource、task_id→ResearchTask（catalog.db 实测 `resource_id=gh:owner/repo`、task JSON 实测 `rtis-20261102-220853-7827`）[侦察:架构评审]。
- 正式决策草案：`M1_DECISION_LOG.md` DEC-006；其余差异决策清单与建议口径随 M1-B 产出，清单见 §2 冲突表 K-09/K-10/K-11/K-16/K-17「处置」列。

### 步 6：复核数据基线

**结论：全部可复放计数实测复现，与计划 §2.4 要求一致；产出签名级基线 `DATA_BASELINE_SNAPSHOT.md`（含快照时间、库哈希、逐条来源命令）。**

要点：

- catalog.db 16 项计数（resources INDEXED=81、d5 63+5、review_log 68/79/18、mapping 16/1/1、quality A4/B28/C44/D5、license 56/16/7）[侦察:确定性事实] → [实测] 全部逐条复现，零偏差。
- knowledge.db 四项（poc 14717 / k_meta 28 / k_negative 17 / k_event 633）[侦察:确定性事实] → [实测] 全部复现。
- **13 vs 130 矛盾裁定**：`select verified,count(*) from poc group by verified` → `1|130`、`0|14587` [实测]。130/14717=0.883%≈0.88%，未验证 14587/14717=99.12%。verified 基准数为 130：KNOWLEDGE_BASE_REVIEW.md:11 正确；「13」笔误共两处（SYSTEM_INVENTORY.md:61、KNOWLEDGE_BASE_REVIEW.md:36，后者与同文件 :11 相抵），归 §2 K-01。
- D5 口径解释（[计划 §2.4:167-172] 要求项 3）：候选总量 68 = 63 条 NEEDS_REVIEW（待人工）+ 5 条 RELATED_RESOURCE（判定相关资源、保留双方）；review_log 中 d5 队列 68 行 = 63 条 NEEDS_MORE_EVIDENCE + 5 条 RELATED_KEEP_BOTH（规则审核已留痕，reviewer 默认 `eri-p2-rules`）[实测]。早期摘要「63+5」指候选总量，后期说明「63 条待人工」指仅 NEEDS_REVIEW，两口径均正确。
- License「8 条待人工」（[计划 §2.4:167]）：本次在 catalog.db 内**未能导出**该口径（license 队列 79 条均已规则审核；license_status 分布 56/16/7）。**待人工确认**，不阻塞 M1。
- engagements=28：`ls ~/.dsh/redteam/engagements/ | wc -l` = 28 [实测]，与 [侦察:28库盘点] 行数一致。

### 步 7：专项盘点 28 库（只读，不运行迁移）

**结论：预盘点完成，无未知结构；正式逐库盘点（含 SHA-256、样本、空值比例）仍属 M1-C 交付。**

- 28 个 engagement：26 个 compatible（schema_md5 `55c9ad012317b709acd77b0ba2f00f34`，表集一致，28 表）、1 个 unsupported（`openredir-landing`，37 表、差异 9 项：evidence_index/plan_revision/postmortem/profile_delta/research_plan/site_feature/site_profile/state_transition/workflow_run）、1 个不适用（`王红娘-520-wanghongniang-com`，无 assets.db，目录仅含 engagement.yaml/SCOPE.md 与空 reports/runs）[侦察:28库盘点]（world.run sqlite3 只读直查；本机复验 engagements 目录数=28，逐库指纹未本机复放）。
- 全部可读库 user_version=0（无库记 -1），佐证 [计划 §2.3:157]「28 个库缺少统一 Schema Version」，即 R-01 证据。
- 最大库：联想-lenovo-src，25,483 行、4,382,720 字节；asset_rows 最大 2,335 [侦察:28库盘点]。
- 依据 [计划 §6.2:405-428]：本次预盘点仅覆盖「Schema 指纹 + 兼容级别」，M1-C 必须补齐文件 SHA-256、时间、空值比例、重复记录、样本（脱敏）、映射可行性；`openredir-landing` 的 9 张差异表进入隔离队列，不做自动转换。

### 步 8：明确 RTIS Bridge 方案

**结论：降级范围已实测界定，不阻塞 M1；推荐方案 A（正式修复），执行需另行批准，不混入 M1 批次。**

- 影响范围 [侦察:Bridge评估]：`redteam_poc_search` 与 `redteam_kb_stats` 实测返回 `{ok:false, error:"引擎不可用：未找到 rtis-bridge.js"}`；降级限于桥依赖的知识工具（poc_search/poc_get/rtis_knowledge 全 action/kb_stats）；`redteam_skill_list`（125 skills）与 `rtis_task`（35 tasks，走 rtis.py 编排+gate）实测正常。
- 备份定位 [实测]：全盘唯一备份 `/home/emo/.dsh/dsh-purge-community/backups/2026-11-04T21-24-20-570Z-62366585/plugins/dsh-purge-community/lib/redteam/rtis-bridge.js` 存在（24,698 字节，mtime 2026-11-05 05:24，权限 600）；目标路径 `security-research-upgrade/rtis/tools/rtis-bridge.js` 确认不存在 [实测 `ls`]。
- 三方案对比与推荐 [侦察:Bridge评估]：A 正式修复（复制备份到 findBridge 已扫描路径，约 10 分钟，免重启，`redteam_kb_stats` 冒烟验证；风险：备份可能落后 DSH 侧最新桥，但已是全盘唯一）；B 改接 rt.py mcp（约 30 分钟，退回 legacy 检索，丢失 2-gram/K0-K4/反知识/站型知识层，违背单源原则）；C 隔离 Adapter（M1 用 rtis_task gate + skills + rt.py CLI，零代码，不阻塞）。
- 决定：**推荐先 A**，冒烟失败则 M1 用 C 兜底，B 列后续排期；恢复动作属于对 `rtis/tools/` 的写入，不在 M1 允许修改文件清单内，**列为独立变更请求待批**（对应 [计划 §13:1140]「正式修复路线或阶段性隔离接口」两支均已明确）。满足 M1 退出门槛「rtis-bridge.js 依赖已明确」[计划 §M1:672]。

### 步 9：完成 M1 Readiness Review

**结论：即本文档。** 前置条件、未解决冲突、修改文件清单（指向 `M1_IMPLEMENTATION_PLAN.md` §3）、风险更新（§3）齐备。

### 步 10：提交 M1 实施计划供批准

**结论：`M1_IMPLEMENTATION_PLAN.md` 已产出，头部标注「状态：草案，待项目负责人批准后实施」。** 在获明确授权前仅进行只读评审与计划完善（[计划 §15:1222-1224]）。M1-A..E 任务分解对齐 [计划 §M1:596-652]。

---

## 2. 未解决冲突清单

状态定义：`已裁定`（证据已定，待批勘误）/ `移交 M1-B`（差异决策，M1 范围内闭环）/ `待人工确认`（口径或事实只有项目负责人可定）/ `不阻塞`（记录在案，不阻碍 M1 准入）。

| # | 冲突/缺口 | 来源 | 状态 | 处置与归属 |
|---|---|---|---|---|
| K-01 | 「13」笔误共两处：SYSTEM_INVENTORY.md:61「13/14717 verified」、KNOWLEDGE_BASE_REVIEW.md:36「13 条已验证知识」（后者与同文件 :11「仅 130 (0.88%)」相抵；两处均经 [实测] grep 核实） | [侦察:M0交付校验] + [实测] | **已裁定** [实测] | verified=130（0.88%，130/14717=0.883%），「13」为笔误；两处一并勘误修订，M0 报告修改需批准（不在 M1 修改清单），裁定依据见 DATA_BASELINE_SNAPSHOT §2.2 |
| K-02 | SYSTEM_INVENTORY.md:55 断引「TECHNOLOGY_DECISION.md §3」（实际内容在 §9.1） | [侦察:M0交付校验] | 待批勘误 | 随 K-01 一并修订 |
| K-03 | 「七域」在 13 份报告及全工作区无定义（grep 零命中）；最接近枚举为 7 个功能引擎（resource/knowledge/search/quality/tool/workflow/agent） | [侦察:M0交付校验] | 待人工确认 | [建议] M1-B 文档显式定义域清单（采纳 7 功能引擎口径）；负责人确认前不写入 schema 命名 |
| K-04 | M1 建表范围矛盾：8 个有现网数据实体 vs 15 实体+alias+schema_version | FINAL_ARCHITECTURE §13.5 vs IMPLEMENTATION_ROADMAP M1 [侦察:架构评审] | 移交 M1-B | [建议] 按 15 实体+差异实体全量建模，M1-D 按数据可得性逐表填充，空表在对账报告显式标注 |
| K-05 | 「每域一库或单库多 schema」两可 vs 路线图写死单库 platform.db | TECHNOLOGY_DECISION §9.2 vs IMPLEMENTATION_ROADMAP [侦察:架构评审] | 移交 M1-B | DEC-002 条件：M1-B 定案；[建议] 单库 platform.db（与 ROADMAP 一致、写路径唯一） |
| K-06 | DEC-005 未 PoC 即定本地 Web UI | TECHNOLOGY_DECISION §9.5 vs 计划 §7.1/§M7 [侦察:架构评审] | 不阻塞（M7 前） | 条件进 DEC-005：M7 前补最小 PoC 记录 |
| K-07 | Review(1:1) 声明 vs review_log 165 行 > resources 81 条（实为 1:N） | UNIFIED_DOMAIN_MODEL §7.1 vs §7.14 [侦察:架构评审] | 移交 M1-B | [建议] 定案 1:N（以实测为据） |
| K-08 | 双轴正交原则被 Knowledge 10 态打破（QUARANTINED/REJECTED/CONFLICTED 嵌入生命周期链） | UNIFIED_DOMAIN_MODEL §7.0 vs §7.3 [侦察:架构评审] | 移交 M1-B | Schema V1 重构：治理轴独立列/表，生命周期链纯化（对齐计划 §5.3） |
| K-09 | 别名仅单 alias 列（缺四要素）；§7.0 把 poc.code 误归资源类别名；§7.13「RTIS-…」大小写与实测不符 | UNIFIED_DOMAIN_MODEL vs 计划 §5.4 [侦察:架构评审] | 移交 M1-B | ExternalIdentifier 结构（来源系统/命名空间/有效时间/冲突检查）；[建议] task_id 沿用现网小写格式作主 ID + 别名表记录 |
| K-10 | Tool↔ToolVersion 被画成 1:1；ToolExecution/UsageEvent/ResourceUsage/ResourceSource/SkillMapping 无实体定义；SkillVersion 缺位；Workflow/Agent 保留策略字段缺落点；ResearchTask 暂停/恢复语义未确认 | UNIFIED_DOMAIN_MODEL 关系图等 [侦察:架构评审] | 移交 M1-B | 数据字典逐项定案（实体/派生表/视图三选一），产出差异决策记录 |
| K-11 | Evidence 全局 ID 桥表 schema 未定义（assets.db 自增 ID 对 EVD-*） | FINAL_ARCHITECTURE §13.4、MIGRATION_STRATEGY §10.1 [侦察:架构评审] | 移交 M1-B | M1-B 定义映射结构并纳入 M1-C 对账 |
| K-12 | License「8 条待人工」口径在 catalog.db 不可导出 | [计划 §2.4:167] vs [实测] | **待人工确认** | 不阻塞 M1-D 抽取；确认结果补入对账报告 license 队列解释 |
| K-13 | M8 阶段缺位；「不做数据反向迁移」与计划 M8 回放/反向同步要求未衔接 | IMPLEMENTATION_ROADMAP vs 计划 §M8、MIGRATION_STRATEGY §10.3.3 [侦察:架构评审] | 不阻塞 | 列入路线图补全待办（待批变更，M1 之外） |
| K-14 | rtis-bridge.js 修复在 M0-M7 路线中无安排 | SYSTEM_INVENTORY §1.5.1 [侦察:架构评审] | 不阻塞 | 步 8 已给出 A/C 方案与归属，独立变更待批 |
| K-15 | M0 报告缺 D5/License/SkillMapping 队列计数基线 | [侦察:架构评审]（对照计划 §2.4/R-06） | **已补齐** | `DATA_BASELINE_SNAPSHOT.md` 交付，含 63+5 口径解释 |
| K-16 | Resource 生命周期链无验证维度（缺 FETCHED/VERIFIED），EVALUATED 属质量评估非验证，不满足计划「区分资源发现与资源验证」 | UNIFIED_DOMAIN_MODEL §7.1 vs 计划 §5.2:335/§5.3 [侦察:架构评审] | 移交 M1-B | Schema V1 生命周期加态或显式声明验证治理轴，对齐 [计划 §5.3] 状态机约束 |
| K-17 | NegativeKnowledge 仅作 Knowledge 伴随实体保留（k_negative 现状），无字段/状态机定义，违反计划 §10.2 独立治理对象要求 | UNIFIED_DOMAIN_MODEL §7.16 vs 计划 §10.2 [侦察:架构评审] | 移交 M1-B | 独立表 + 最小实体定义（现网 17 条 k_negative 迁移，[实测] 计数见 DATA_BASELINE §2.2） |

---

## 3. 风险表更新（计划 §11 R-01..R-15 中被本次证据更新者）

更新级别：`更新`（本次证据改变应对策略的输入或剩余动作）、`维持`（无新证据）、`关闭候选`（关闭条件已满足，待项目负责人确认，关闭须有证据 [计划 §11:1077]）。

| ID | 风险（简） | 原级别 | 本次更新 | 新状态 | 剩余动作与归属 |
|---|---|---|---|---|---|
| R-01 | 28 个 assets.db 缺统一 Schema Version | 极高 | 28 库指纹已采集：26 库同构（同一 md5）、1 库 37 表异构已知差异、1 库无文件 [侦察:28库盘点]；user_version 全 0；无未知结构 | 更新（缓解证据落地） | M1-C 正式逐库盘点（SHA-256/样本/空值比例）；schema_version 机制进 Schema V1（M1-B）；未知结构隔离 |
| R-02 | rtis-bridge.js 缺失（P0） | 高 | 全盘唯一备份已定位 [实测]；降级范围实测界定；A/C 两方案与归属明确；不阻塞 M1 | 更新（影响面收窄） | 批准后执行方案 A 复制 + kb_stats 冒烟；失败即 C 兜底；Stub 不得冒充生产修复 |
| R-03 | crates.io API 超时 / sparse index TLS EOF | 高 | 本轮证据确认 M0 止于网络探测、vendored clean build 从未实际执行过一次（计划 §13:1153 提问回答为「否」）[侦察:架构评审 gaps] | 更新（回退未验证获实证） | M2-A 前完成可执行 vendored 离线 clean build（DEC-001 条件）；M1 不受影响 |
| R-04 | KB 绝大部分为 Nuclei 模板索引 | 高 | verified=130 (0.88%) / 14717 实测确认 [实测]；模板占比 95.9% 为 M0 摘要口径，本次未重统 | 更新（验证率锚定） | Resource/Knowledge/Evidence 分离进 Schema V1（M1-B）；验证状态保真 |
| R-05 | D5/License 等审核队列待人工项 | 高 | 队列计数与决策分布快照固化：d5 68（63 待人工 + 5 相关）、license 79、mapping 18，规则审核 165 行留痕 [实测] | 更新（数量锚定） | 人工队列由项目负责人排期；M1 不自动批准任何队列 [计划 §M1:661] |
| R-06 | D5 数量口径 63 vs 63+5 差异 | 中 | 口径已解释且两表互证：候选总量 68 = 63 NEEDS_REVIEW + 5 RELATED_RESOURCE；review_log d5=68 与 d5_candidates=68 对账一致 [实测] | **关闭候选** | 项目负责人在 M1-A 评审确认关闭；关闭证据即 DATA_BASELINE_SNAPSHOT |
| R-07 | Skills 元数据缺字段、标签不足 | 中 | 6 个文件点名（av-evasion / confluence-atlassian / electron-client / iot-snmp-pivot / opsec-anti-trace / phishing-delivery，均缺 role/whenToUse/enabled）；hunt 42、tags 6 与 M0 摘要一致 [侦察:Skills/Tools复核] | 更新（定位到文件） | 兼容层与质量规则 M1-B/M4 设计；不直接批量重写源文件 [计划 §2.3:145] |
| R-08 | tools.json 探测逻辑与 installed 标记不可靠 | 高 | 5 项失真实测点名：fscan version=报错首行、gogo installed=false 失真（二进制存在仅不在 PATH）、frpc 0.71.1→0.72.0 漂移、suo5 version=usage 头、rtis.py version 硬编码「1」；另 pd-httpx 误判（实际在 ~/.local/bin/httpx）[侦察:Skills/Tools复核] | 更新（失真清单化） | M1-B Tool 实体设计：定义/安装/健康/授权四态分离 [计划 R-08 应对]；注册表重建属 M6 |
| R-09 | 旧系统数据库耦合 | 高 | 无本次证据更新 | 维持 | Adapter/API 契约；禁止跨模块直读私有表 |
| R-10 | 过早微服务化 | 中 | 无本次证据更新 | 维持 | 模块化单体优先 |
| R-11 | Rust 重写行为语义漂移 | 高 | 无本次证据更新 | 维持 | M2-D ERI 基线对照；差异审批 |
| R-12 | AI Worker 越权或覆盖已验证数据 | 极高 | 设计输入新增：DEC-006 将 AuthorizationRecord 实体化并设强制闸（评审建议，未批准） | 维持（附设计输入） | Worker 契约、最小权限、候选状态、审计与人工审核不变 |
| R-13 | 双写导致数据分叉 | 极高 | 无本次证据更新 | 维持 | 单一权威写入方；M1 只写隔离库天然规避 |
| R-14 | 搜索索引被误当事实数据 | 高 | 无本次证据更新 | 维持 | 派生索引可重建；权威数据分层（计划 §6.1） |
| R-15 | UI 与后端权限实现不一致 | 高 | 无本次证据更新 | 维持 | 后端强制鉴权；M7 才涉及 |

---

## 4. M1 准入判断

**判断：ready_with_conditions。**

理由（依据 §1/§2/§3）：

- 就绪面：十步清单全部完成；关键数据基线实测可复放（复现率 100%）；28 库无未知结构；bridge 缺失有兜底且不阻塞；M1-A/B/C 交付物骨架（本文档组 + 实施计划草案）已备。
- 非就绪面：M0 架构本身尚未获批（[计划 §15:1222] 第一顺位）；DEC-001..005/006 均处 Proposed（[计划 §12.1:1085]「M0 已完成，不代表 M1 自动获批」，批准权在项目负责人）；M1 写入边界（文件清单/隔离库/禁改路径）尚待批准 [计划 §15:1222-1224]；M0 勘误（两处「13」笔误 + 一处断引）与 2 项口径待人工确认。

**准入条件（全部满足方可进入 M1-B 之后的写库类工作；M1-A 文档类工作不受 C3/C4/C5 阻塞）：**

| # | 条件 | 阻塞范围 |
|---|---|---|
| C0 | M0 架构（13 份报告）与总体设计经项目负责人审阅批准（[计划 §15:1222] 第一顺位：先批准 M0 架构与 DEC-001..005，再批准 M1 Readiness Review 与隔离环境实施范围） | M1-A 决策冻结及后续全部 |
| C1 | DEC-001..005 按 `M1_DECISION_LOG.md` 建议签署（Accepted / Accepted with Conditions），DEC-006 定稿 | M1-B 及后续全部 |
| C2 | `M1_IMPLEMENTATION_PLAN.md` 获批准稿（文件清单、隔离库、只读源访问、禁改路径按计划 §15 要求明确） | M1-B 及后续全部 |
| C3 | K-01/K-02 勘误修订与 K-03「七域」定义获批准（修订 M0 报告属计划外变更） | 仅文档一致性；不阻塞隔离 Schema 设计 |
| C4 | rtis-bridge.js 方案 A 作为独立变更批准执行并冒烟通过；否则 M1 知识检索走 C 兜底（rtis_task gate + rt.py CLI） | 不阻塞 M1（退出门槛已满足「依赖已明确」） |
| C5 | K-12 License「8 条待人工」口径确认 | 仅对账报告解释；不阻塞抽取 |
| C6 | M1-C 完成 28 库正式盘点（含 SHA-256/样本/空值比例）后方可固化任何映射规则 | M1-C 到 M1-D 之间的推进 |
| C7 | 步 1 冻结条款生效确认 | 项目状态条款，非技术门槛 |

---

## 5. M1 开工前必须回答的问题（计划 §13:1144-1153 对照）

| 计划问题 | 本评审回答 | 依据 |
|---|---|---|
| 15 实体最终清单？是否含独立 AuthorizationRecord？ | 15 清单可确认；AuthorizationRecord 建议实体化为第 16 实体（DEC-006 草案，待批） | [侦察:架构评审 entityGaps] + [计划 §5.2:351] |
| 统一 ID 生成/冲突/Alias/跨来源映射规则？ | 差异决策移交 M1-B；红线：ID 冲突审计完成前不批量生成新 ID [计划 §5.4:388] | K-09/K-10/K-11 |
| 三类现有数据库权威关系与读取优先级？ | 未在 M0 定案，M1-B 必答；现网 task JSON/catalog.db/knowledge.db 读取优先级按域声明 | K-04 关联缺口 |
| 28 库兼容/条件转换/隔离？ | 26 compatible / 1 unsupported（openredir-landing 隔离）/ 1 不适用（无库）；正式盘点在 M1-C | [侦察:28库盘点] |
| 保真级/可重建级/冷存级划分？ | 按 [计划 §6.4] 执行，逐表标注进数据字典（M1-B），迁移不得只按表名判断 | [计划 §6.4:447] |
| Dry-run 源快照如何固定、目标如何隔离销毁？ | 实施计划 §4：源库 SHA-256 + 只读连接；platform.db 独立文件；销毁即删目录 | `M1_IMPLEMENTATION_PLAN.md` |
| RTIS Bridge 缺失是否阻塞 M1？ | 不阻塞；方案 A 推荐 / C 兜底，已满足退出门槛表述要求 | 步 8 |
| DEC-001 vendored 构建回退是否已通过可执行验证？ | **否**，从未执行过一次，止于网络探测；列 M2-A 硬门槛，不阻塞 M1 | [侦察:架构评审 gaps] + R-03 |

---

## 6. 本评审未做与限制（如实报告）

- 28 库逐库指纹未在本机复放，采信 [侦察:28库盘点]（world.run 直查快照，声明可重放）；本机仅复核 engagements 目录数=28 [实测]。
- Skills/tools 复核（125 skills、6 文件缺字段、5 项工具失真）采信 [侦察:Skills/Tools复核]，未逐文件复放。
- MCP 侧 bridge 降级表现（poc_search/kb_stats 返回 ok:false）采信 [侦察:Bridge评估]；本评审未通过 MCP 复调（避免在只读评审会话内依赖降级通道），但备份与目标路径存在性已实测 [实测]。
- M0 报告正文（13 份、728 行）未逐行重读，采信 [侦察:M0交付校验] 的行级定位；关键矛盾（13/130、断引、七域）已单独裁定或标注。
- 本文档不授予任何写入权限：所有「建议」以项目负责人批示为准（[计划 §15:1208-1218] 不自动授予的权限清单继续有效）。
