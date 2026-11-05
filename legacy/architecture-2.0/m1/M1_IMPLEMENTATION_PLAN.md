# M1 Implementation Plan（M1 实施计划）

> **状态：草案，待项目负责人批准后实施。**
> 在获得明确书面批准前，仅进行只读评审与计划完善，不执行本计划中的任何写库、建文件（除本文档组）或脚本动作（[计划 §15:1222-1224]）。

| 项 | 值 |
|---|---|
| 日期 | 2026-11-06 |
| 对齐 | [计划 §M1:588-673]（M1-A..M1-E、M1 明确不做、退出门槛）；[计划 §6.2/§6.3]（28 库专项与 ETL 原则）；[计划 §12.2]（批次固定流程） |
| 前置 | `M1_READINESS_REVIEW.md` 准入判断 ready_with_conditions 及条件 C0..C7 |
| 批准请求 | 见 §8 |

---

## 1. 目标与范围

- **目标**（[计划 §M1:594]）：证明统一 Domain Model 能够准确映射七个领域的现有业务与数据，并在完全隔离环境中完成 ETL Dry-run。
- **范围红线**（[计划 §M1 明确不做:654-661]，逐条采纳）：不切生产流量；不替换旧数据库；不改写 ERI、RTIS、KB、Skills 生产数据；不启动全量 Rust Core 开发；不删除原始或重复记录；不自动批准人工审核队列。
- **领域口径**：以 7 个功能引擎（resource/knowledge/search/quality/tool/workflow/agent）为工作口径（M1_READINESS_REVIEW K-03，待负责人确认；确认前不写入 schema 命名）。
- **实体口径**：15 核心实体 + 差异实体（AuthorizationRecord 第 16，DEC-006；NegativeKnowledge 独立治理对象；其余差异实体由 M1-B 逐项定案，见 M1_READINESS_REVIEW K-04/K-10）。

## 2. 任务分解（M1-A..M1-E）

### M1-A：Readiness Review 与架构决策冻结（[计划 §596-607]）

| 项 | 内容 |
|---|---|
| 任务 | 已完成主体：13 份报告校验、DEC-001..005 评审、实体清单核对、数量抽样核验、28 库预盘点、bridge 方案（见 M1_READINESS_REVIEW）。剩余：DEC 签署归档、K-01/K-02 勘误执行（获批后）、差异决策清单冻结 |
| 输入 | 本文档组 + `M1_DECISION_LOG.md` + 负责人批示 |
| 产出 | 已签署的 `M1_DECISION_LOG.md`（Status 更新）；M0 报告勘误（「13」笔误两处 + 断引一处）的修订：**属批次外单独授权变更，不在 §3 文件清单与约束内**，获批后作为独立变更执行，未授权前 M0 报告保持只读 |
| 完成判据 | C1/C3/C7 满足；DEC-006 定稿或驳回留痕 |

### M1-B：Schema V1 与物理模型设计（[计划 §608-618]）

| 项 | 内容 |
|---|---|
| 任务 | ① 定义实体表/字段/主键/外键/唯一约束/索引（15 实体 + DEC-006 授权子域 + NegativeKnowledge 独立表 + ExternalIdentifier）；② 差异决策落地：Review 1:N（K-07，实测依据 review_log 165 > 81）、治理轴与生命周期链分离（K-08）、Tool:ToolVersion 1:N、ToolExecution/UsageEvent/ResourceUsage/ResourceSource/SkillMapping 逐项定实体/派生表/视图（K-10）、Resource 生命周期补验证维度（FETCHED/VERIFIED，对齐 [计划 §5.3]）、SkillVersion 或显式接受 Review-only 差异、Evidence 桥表 EVD 映射结构（K-11）、ResearchTask 状态机补暂停/恢复语义或显式差异决策；③ JSON 字段使用边界：重要关系不退化为不可查询 JSON（[计划 §613]）；④ 事务边界、并发策略、备份恢复要求；⑤ 初始化与升级迁移脚本（先只在隔离库验证）；⑥ 外键强制开启、schema_version 表 + user_version（DEC-002 条件、R-01 教训） |
| 输入 | UNIFIED_DOMAIN_MODEL.md、CURRENT_DATABASE_MODEL.md、M1_DECISION_LOG.md、DATA_BASELINE_SNAPSHOT.md |
| 产出 | `M1_SCHEMA_DESIGN.md`（含 ER 图与数据字典）、`migrations/001_schema_v1_init.sql`、schema 版本策略 |
| 完成判据 | 隔离库执行 001 脚本成功；外键 pragma 全开无 violation；数据字典覆盖全部实体表且逐表标注数据级别（保真/可重建/冷存，[计划 §6.4]） |

### M1-C：源数据盘点与映射（[计划 §620-629]）

| 项 | 内容 |
|---|---|
| 任务 | ① Source Mapping Matrix：ERI/RTIS/KB/Skills/Tools/Agent/UI 七源字段逐一标注来源/语义/目标字段/转换规则/空值策略/可信度；② 28 库正式盘点（[计划 §6.2:409-419] 全字段：SHA-256、大小、时间、schema 指纹、数据量、空值比例、重复、异常引用、脱敏样本、映射可行性、兼容级别、人工判断事项）；③ D5/License/SkillMapping 队列重计数并快照（以 DATA_BASELINE_SNAPSHOT 为基线对账）；④ Resource/Knowledge/Evidence 区分，模板索引归属 Resource（[计划 §2.3:136]）；⑤ 无法确认的数据定义隔离原因与人工决策入口 |
| 输入 | catalog.db / knowledge.db / 28 库（只读）、28库预盘点快照 |
| 产出 | `M1_SOURCE_MAPPING.md`、`M1_ASSETS_DB_COMPATIBILITY.md`、字段映射与异常清单、`m1/tools/inventory_28db.py`（只读盘点脚本，输出 JSON） |
| 完成判据 | C6 满足（正式盘点完成前不固化任何映射规则）；openredir-landing 9 张差异表进入隔离队列；无 Unknown 被猜测转换 |

### M1-D：ETL Dry-run（[计划 §631-640]）

| 项 | 内容 |
|---|---|
| 任务 | ① 只读抽取生产数据（`file:...?mode=ro` 连接，抽取前记录源库 SHA-256）；② 加载至隔离 platform.db（§4）；③ 保留源 ID、来源系统、源快照哈希、转换器版本（ExternalIdentifier 全四要素：来源系统/命名空间/有效时间/冲突检查，[计划 §5.4:385]）；④ 验证转换、关系重建、幂等性、失败重试；⑤ 验证重复数据、孤儿关系、无效状态、权限差异；⑥ Unknown/Unsupported 数据隔离，不猜测转换 |
| 输入 | M1-B Schema V1、M1-C 映射矩阵、`m1/tools/etl_dryrun.py`（新建，含版本号） |
| 产出 | `M1_ETL_DESIGN.md`、执行日志、Dry-run 报告、异常隔离清单 |
| 完成判据 | 同一源快照连续执行 2 次结果可重复（[计划 §6.3:436] Replay）；隔离库中不存在无 ExternalIdentifier 的迁移行 |

### M1-E：一致性、安全与回滚验证（[计划 §642-652]）

| 项 | 内容 |
|---|---|
| 任务 | ① 逐类对账：源记录数 = 目标记录数 + 隔离记录数（逐实体逐表，不允许「总体数量相同」代替 [计划 §650]）；② ID 覆盖率、Alias 冲突、关系完整性对账；③ 原始内容哈希一致性抽样；④ 审核状态/权限/授权/审计记录对比；⑤ Dry-run 重复执行可重复性验证；⑥ 隔离目标删除/重建，证明不影响源（源库 SHA-256 前后一致） |
| 输入 | M1-D 产物、`m1/tools/verify.py`（新建，校验维度不少于计划 M1-E 四类：行数对账 + ID 覆盖率 + Alias 冲突 + 关系完整性，另有字段哈希抽样） |
| 产出 | `M1_RECONCILIATION_REPORT.md`、`M1_SECURITY_VALIDATION.md`、`M1_ACCEPTANCE_REPORT.md`（按 [计划 附录 C] 模板） |
| 完成判据 | §6 验收门槛全绿；M2 独立授权请求随验收报告提交 |

## 3. 修改文件清单（仅新建，获批后创建）

**原则**：本批次只新建以下两个目录树内的文件，不修改、不移动、不删除任何既有文件（[计划 §12.2:1096] 修改文件清单要求）。

**例外声明**：M1-A 的 M0 报告勘误（K-01 两处「13」笔误 + K-02 断引，见 M1_READINESS_REVIEW §2）属计划外变更，需项目负责人单独授权后执行，**不在本清单与上述约束覆盖范围内**；未获授权前 13 份 M0 报告保持只读。

```text
/home/emo/code/pensting/security-research-upgrade/architecture-2.0/m1/
├── M1_READINESS_REVIEW.md          # 已交付（本轮评审产出）
├── M1_DECISION_LOG.md              # 已交付；M1-A 签署后仅更新 Status 字段
├── DATA_BASELINE_SNAPSHOT.md       # 已交付
├── M1_IMPLEMENTATION_PLAN.md       # 本文件；批准后为执行基线
├── M1_SCHEMA_DESIGN.md             # M1-B 新建
├── M1_SOURCE_MAPPING.md            # M1-C 新建
├── M1_ASSETS_DB_COMPATIBILITY.md   # M1-C 新建
├── M1_ETL_DESIGN.md                # M1-D 新建
├── M1_RECONCILIATION_REPORT.md     # M1-E 新建
├── M1_SECURITY_VALIDATION.md       # M1-E 新建
├── M1_ACCEPTANCE_REPORT.md         # M1-E 新建
├── tools/                          # M1-C/M1-D/M1-E 新建（脚本均带版本号，只读源）
│   ├── inventory_28db.py
│   ├── etl_dryrun.py
│   └── verify.py
└── sandbox/                        # M1-D 隔离工作区（可随时整体删除，见 §4/§7）
    ├── platform.db                 # 隔离目标库（独立文件）
    ├── source_manifest.json        # 源库清单：路径/SHA-256/抽取时间/转换器版本
    └── quarantine/                 # Unknown/Unsupported 隔离记录

/home/emo/code/pensting/security-research-upgrade/architecture-2.0/migrations/
├── 001_schema_v1_init.sql          # M1-B 新建（Schema V1 + schema_version 种子）
└── 002_indexes_and_views.sql       # M1-B 新建（索引与只读视图，视图仅派生数据）
```

说明：`migrations/` 落位于 architecture-2.0/ 目录树内（与 M0 报告同根，避免触及 [计划 附录 A:1266] 目标态的 research-platform/ 目录，后者属 M2 范围）；如负责人指定其他位置，以批示为准。`migrations/` 目录当前不存在，属新建。

## 4. 隔离环境方案

1. **隔离目标库**：`m1/sandbox/platform.db`，独立 SQLite 文件；`PRAGMA foreign_keys=ON`；`PRAGMA user_version` 承载 schema 版本（与 schema_version 表双写）；journal_mode=WAL。
2. **源访问**：所有源库以 `sqlite3 "file:<path>?mode=ro"` 只读 URI 打开；抽取前将源库 SHA-256 写入 `source_manifest.json`；盘点/抽取脚本无任何写入语句（M1-C/M1-D 评审脚本 diff 作为验收材料）。
3. **源快照固定**：以库文件 SHA-256 + 行级锚点（各表 count）固定源快照；不做生产库复制（除隔离库中经 ETL 的映射副本）。
4. **不注册**：platform.db 不写入任何现网配置（eri/rtis 的连接配置、MCP 配置均不引用它）；仅 m1/tools 脚本显式传参访问。
5. **销毁**：`rm -rf m1/sandbox/` 即销毁全部隔离状态；销毁后可由 migrations + etl_dryrun.py + 同一源快照重建（[计划 §6.3:437] Rollback 语义）。

## 5. 禁止触碰的生产路径清单

以下路径本批次一律只读或完全不接触：

```text
/home/emo/.dsh/redteam/knowledge.db                      # RTIS 知识库（只读）
/home/emo/.dsh/redteam/engagements/**                    # 28 个 engagement 全部内容（只读；
                                                         #   含 assets.db、engagement.yaml、SCOPE.md）
/home/emo/code/pensting/security-research-upgrade/eri/** # ERI 代码、db/catalog.db、data/、audit/（只读）
/home/emo/.dsh/redteam/toolkit/**                        # 工具库（只读）
skills 目录与 tools.json（rtis 侧）                       # R-07/R-08 治理属后续阶段，本批不动
rtis.py、rtis_mcp.mjs、rtis/tools/**                     # 含 rtis-bridge.js 恢复动作（独立变更，不在本批）
/home/emo/.dsh/dsh-purge-community/backups/**            # 备份区（只读）
runs/、reports/（工作区生产报告与证据）                    # 不修改
Git 操作：不 commit/push/reset/checkout/clean/stash/rebase
```

依据：[计划 §15:1208-1218] 不自动授予的权限清单；[计划 §12.3:1103-1112] Git 与工作区安全；项目 AGENTS.md（engagement.yaml/SCOPE.md 为授权事实源）。

## 6. 验收门槛（对照 [计划 §M1 退出门槛:663-673] 逐条）

| # | 门槛（量化判据） | 来源 |
|---|---|---|
| A-1 | **verify 一致率 100%**：逐实体逐表满足 源记录数 = 目标记录数 + 隔离记录数，且每条差异均有解释或隔离记录；不允许总量对抵 | [计划 §650] + 本计划量化 |
| A-2 | **源 ID 覆盖率 100%**：每类源 ID 全部映射进 ExternalIdentifier 或进入隔离清单，无未解释缺失 | [计划 §M1-E:645] |
| A-3 | **Alias 冲突 0 未解决**：冲突项全部隔离并留人工决策入口 | [计划 §M1:667] |
| A-4 | **关系完整性对账通过**：外键 pragma 全开 + 孤儿关系检查 0 未解释 | [计划 §M1-E:645] |
| A-5 | **可重复性**：同一源快照连续 ≥2 次 Dry-run，verify 全绿且差异字段清单为空或与首次声明一致 | [计划 §M1-E:648] |
| A-6 | **隔离性证明**：sandbox/ 整体删除重建后，全部源库 SHA-256 与抽取前一致；生产路径无任何写入（§5 清单 mtimes 抽查） | [计划 §M1:671] |
| A-7 | **模型覆盖完整**：七域映射全覆盖，未映射项逐条有解释；差异决策（K-04/K-05/K-07..K-11）全部落定 | [计划 §M1:665] |
| A-8 | **28 库盘点完成**：27 库正式盘点 + 1 库不适用判定留痕；未知结构隔离 | [计划 §M1:666] |
| A-9 | **权限与审核不越界**：权限无扩大（DEC-006 投影与源 yaml 语义一致）；人工审核队列 0 自动批准 | [计划 §M1:669] |
| A-10 | **bridge 依赖明确**：方案 A 已执行并冒烟通过，或 M1 全程采用 C 隔离 Adapter 并在验收报告记录 | [计划 §M1:672] |
| A-11 | **验收报告获批**：M1_ACCEPTANCE_REPORT.md 通过（附录 C 模板，结论 Pass / Pass with Conditions），M2 独立授权另行申请 | [计划 §M1:673] |

## 7. 回滚方式

1. **M1-B..M1-E 任意时点**：删除 `m1/sandbox/` 与 `migrations/` 下新建文件即回滚（隔离产物与源系统零耦合，无数据回滚需求）；`m1/` 下文档文件按批次归档保留作为决策痕迹。
2. **源系统**：全程零写入（§4.2/§5），无需也无法回滚；若发现任何源库 SHA-256 变化，立即停止批次并上报负责人（视为违规写入事故）。
3. **文档**：本批次不执行任何 Git 操作；新文件未提交，删除即彻底回滚。
4. **M1 后**：M1 验收通过也不自动授权 M2；切换与退役按 [计划 §M8] 单独受控推进。

## 8. 批准请求（需项目负责人逐项明确）

1. **批准 M0 架构（13 份报告）**：[计划 §15:1222] 第一顺位为「先审阅并批准 M0 架构与 DEC-001..005，再批准 M1 Readiness Review 和隔离环境实施范围」；M0 架构批准是本计划获批的前置，本套文档不替代该批准。
2. 批准本计划（M1-A..E 范围、§3 文件清单、§4 隔离方案、§6 验收门槛）。
3. 批准只读访问生产路径清单（§5）中的源库与 engagement 目录（读操作）。
4. 批准 DEC-001..005 签署建议与 DEC-006 定稿方向（`M1_DECISION_LOG.md`）。
5. 批准 M0 报告勘误修订（K-01 两处「13」笔误 + K-02 断引；计划外变更，单独授权，见 §3 例外声明）。
6. 批准 rtis-bridge.js 方案 A 作为独立变更（复制备份至 `rtis/tools/rtis-bridge.js` + kb_stats 冒烟）；或不批准则确认 M1 采用 C 隔离 Adapter。
7. 确认 K-12（License「8 条待人工」口径）与 K-03（「七域」定义）。
8. 确认步 1 冻结条款生效（M1_READINESS_REVIEW §1）。
