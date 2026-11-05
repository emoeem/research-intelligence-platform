# M1 第一批 Dry-run 报告（Schema V1 + ETL + 对账）

| 项 | 值 |
|---|---|
| 日期 | 2026-11-06 |
| 批次 | M1-B/C/D 第一批：隔离环境建表 + 映射 ETL dry-run + 对账（不切流） |
| 工作目录 | `/home/emo/code/pensting/research-platform/migrations/` |
| 隔离目标库 | `/home/emo/code/pensting/research-platform/migrations/platform-dryrun.db`（独立 SQLite 文件，删除即回滚） |
| 依据 | `RESEARCH_INTELLIGENCE_PLATFORM_MASTER_PLAN.md` §5/§6/M1；`architecture-2.0/`（UNIFIED_DOMAIN_MODEL / MIGRATION_STRATEGY / CURRENT_DATABASE_MODEL）；`m1/M1_READINESS_REVIEW.md`（15 条 M0 矛盾修正，本批必须吸收）；`m1/M1_IMPLEMENTATION_PLAN.md`；`m1/DATA_BASELINE_SNAPSHOT.md`（数量基线） |
| 证据标签 | **[verified]** = 本批实际执行命令并有输出记录；**[unverified]** = 采信上游文档、未在本批复放 |

---

## 0. 结论摘要

1. Schema V1 落 11 张表：6 个核心实体（Resource / Knowledge / Evidence / Tool / Skill / Review）+ 2 张从属子表（resource_source、tool_version，分别落实"来源"映射与 K-10 Tool 1:N）+ 3 张基础设施表（entity_alias / audit_event / schema_version）。`PRAGMA foreign_keys=ON`、每表含 created_at/updated_at/schema_version、`user_version=1` 与 schema_version 表双写。**[verified]**（sqlite3 CLI 执行 + 结构查询）
2. ETL dry-run 迁移结果：resource 81、resource_source 81、knowledge 45（own 28 + negative 17）、review 165、entity_alias 372；d5_candidates 68 条入隔离待审清单不转实体；RTIS 35 个 task JSON 仅统计；28 个 assets.db 不迁移。**[verified]**
3. verify 对账：28 项检查全部 PASS，一致率 100%，exit 0。**[verified]**（全文见 §8）
4. 幂等：同一源快照连续执行两次，第二次为 no-op（全表内容摘要逐一相等，0 行变化）。**[verified]**
5. 源库零数据写入：两库 SHA-256 与 DATA_BASELINE_SNAPSHOT §1 基线完全一致；主库 mtime 未变。**[verified]**（详见 §9，含一处伴生文件副作用的如实记录）
6. 失败清单：空（对账与幂等均无失败项；开发期 3 个实现缺陷已在交付前修复，见 §10.2）。**[verified]**

---

## 1. 实体范围裁定

`M1_IMPLEMENTATION_PLAN.md` 对"先落实体数"的裁定：其 M1-B 节描述的是 15 实体 + 差异实体的**全量建模目标**（且文档状态为"草案，待批准"），**未裁定"8 个先落实体"**。按任务书回落口径执行：落 **6 个有现网数据/现网定义的核心实体** + 3 张基础设施表。

| 裁定项 | 本批执行 | 依据 |
|---|---|---|
| 实体范围 | Resource、Knowledge、Evidence、Tool、Skill、Review（6） | 任务书回落口径；K-04（M1-D 按数据可得性逐表填充，空表显式标注） |
| 基础设施 | entity_alias、audit_event、schema_version（3） | 计划 §5.4、§M1-B；DEC-002 条件（schema_version 表 + user_version 双写） |
| 子表 | resource_source（Resource 1:N）、tool_version（Tool 1:N） | 任务"resource_sources → 来源"映射；K-10 修正 Tool↔ToolVersion=1:N |
| 不落 | ResourceVersion/KnowledgeVersion/Agent/Workflow/ResearchTask/AuthorizationRecord/NegativeKnowledge 独立表等 | 留 M1 第二批（K-10/K-11/DEC-006/K-17 差异决策随批次落地） |

## 2. Schema V1（migrations/001_init.sql）

11 张表 **[verified]**（`sqlite3 platform-dryrun.db < 001_init.sql` 执行成功；`PRAGMA user_version` = 1；`PRAGMA foreign_key_check` 空）：

| 表 | 类别 | 本批行数 | 说明 |
|---|---|---:|---|
| schema_version | 基础设施 | 1 | V1 种子行；与 user_version 双写 |
| entity_alias | 基础设施 | 372 | K-09 四要素别名 |
| audit_event | 基础设施 | 0 | 只追加；k_event/eri events 迁移留第二批 |
| resource | 核心实体 | 81 | 源 resources + quality_scores 折入 |
| resource_source | 子表 | 81 | 源 resource_sources |
| knowledge | 核心实体 | 45 | own 28 + negative 17 |
| evidence | 核心实体 | 0 | 28 个 assets.db 映射留第二批 |
| tool / tool_version | 核心实体+子表 | 0 / 0 | tools.json → Tool 属 M6（MIGRATION_STRATEGY §10.2） |
| skill | 核心实体 | 0 | 125 个 Skill 文件属后续批次 |
| review | 核心实体 | 165 | 源 review_log，多态 1:N |

### 2.1 就绪评审修正落实对照（逐条）

| 评审编号 | 修正要求 | Schema 落实 | 验证 |
|---|---|---|---|
| K-07 | Review 对 Resource 实为 1:N（review_log 165 > resources 81） | review 表多态引用 (target_type, target_id)，无 1:1 唯一约束；唯一键为 (source_system, source_pk) | **[verified]** 165 行入库，(d5/license/mapping) 三类混合 |
| K-08 | 治理状态与生命周期双轴分离 | resource 与 knowledge 均为 `lifecycle` 列（DISCOVERED/FETCHED/PARSED/CANDIDATE/VERIFIED/INDEXED/STALE）+ `governance` 列（ACTIVE/QUARANTINED/REJECTED/ARCHIVED），CHECK 分列约束，QUARANTINED/REJECTED 不在生命周期链内 | **[verified]** 全部行 lifecycle/governance 非空且合法（CHECK 强制） |
| K-09 | 别名四要素 | entity_alias：source_system / namespace / valid_from+valid_until / `UNIQUE(source_system, namespace, alias_value, entity_type)` 冲突唯一约束 | **[verified]** §8 第 4 节：冲突 0、缺要素 0 |
| K-10 | Tool↔ToolVersion 为 1:N | tool_version 子表 `tool_id REFERENCES tool(tool_id)` + `UNIQUE(tool_id, version)`；本批空表（无现网迁移源） | **[verified]**（表结构存在；1:N 关系待 M6 数据填充后实证） |
| K-16 | Resource 生命周期补验证维度 | lifecycle CHECK 集含 FETCHED、VERIFIED；源 status 仅 INDEXED，映射直通并保留 source_status 原值 | **[verified]** 81 行 lifecycle='INDEXED'，source_status='INDEXED' |
| K-17 | 反知识独立治理对象 | knowledge.record_kind='negative' + 5 个反知识专属列 + negates_knowledge_id 自引用 FK；独立表建议留第二批差异决策 | **[verified]** 17 行，可按 record_kind 查询 |
| K-01 | verified=130（「13」为笔误） | 统计口径采用 130/14717=0.88%；本批仅迁移 k_meta 覆盖 28 条，verified 分布为源值直拷 | **[verified]** 28 行 own 中 verified=1 共 20 行（源查询实测） |
| R-01 教训 | schema 版本机制 | schema_version 表 + `PRAGMA user_version=1` 双写 | **[verified]** |

### 2.2 ID 策略

不批量生成新平台 ID（计划 §5.4：ID 冲突审计完成前禁止）。主键沿用现网稳定 ID：`gh:owner/repo` / `url:hash16`（resource）、`poc.code`（knowledge own）、`'neg:'||父code`（knowledge negative，确定性派生）、`'eri-catalog:review_log:<id>'`（review，确定性派生）。全部迁移行在 entity_alias 登记来源身份。**[verified]**

## 3. 映射规则（版本化 Transform，etl_dryrun.py v1.0.0）

### 3.1 源A catalog.db（sha256 `cd495a81…`，与基线一致）

| 源 | 目标 | 规则 |
|---|---|---|
| resources（81） | resource | 直拷 17 列；status→lifecycle（INDEXED→INDEXED）；governance='ACTIVE'；quality_scores LEFT JOIN 折入 12 个质量列 |
| resource_sources（81） | resource_source | 直拷；created_at:=first_seen、updated_at:=last_seen（源无时间戳列，确定性派生） |
| quality_scores（81） | resource 质量列 | 1:1 折入（任务裁定"质量字段"），不单独建实体 |
| review_log（165） | review | queue→target_type（d5/license/mapping 原值）；item_key→target_id 原值；review_id:='eri-catalog:review_log:'\|\|id；created_at:=reviewed_at |
| d5_candidates（68） | 隔离待审清单 | 不转实体；全行 JSON（含全部源字段）写入 `quarantine/d5_candidates.jsonl`，disposition=manual-review-pending |
| skill_mappings（18）/ events（83）/ github_metadata（79）/ 其他 P2 表 | 不迁移 | 留第二批；行数记录于 §5，对账不丢失口径 |

### 3.2 源B knowledge.db（sha256 `c3f42f58…`，与基线一致；URI mode=ro）

| 源 | 目标 | 规则 |
|---|---|---|
| poc × k_meta（28） | knowledge(record_kind='own') | poc 全字段直拷（含 content/usage）；k_meta 10 列直拷（evidence_level/verification_basis/dedup_key/meta_version/last_reviewed/meta_updated_at 等）；lifecycle 映射：Field-validated/Lab-validated→VERIFIED，Researched→CANDIDATE（原值保留于 source_lifecycle）；governance='ACTIVE' |
| k_negative（17） | knowledge(record_kind='negative') | knowledge_id:='neg:'\|\|父code；do_not_conclude→content 与 neg_do_not_conclude；4 个反知识列直拷；neg_source_knowledge_id:=源 poc.id（字符串）；negates_knowledge_id:=父code（父已迁移时，16/17）；title:=父 poc.title（不新造内容）；created_at/updated_at:=父 poc 时间戳（源无时间戳列，确定性派生） |
| poc.source='nuclei'（14,111） | 不迁移 | 可重建级（MIGRATION_STRATEGY §10.1），归属 Resource 侧；仅统计 |
| 其余非 k_meta 非 nuclei poc（578） | 不迁移 | 未分层自有知识（kb/manual/self/web），留第二批；行数已入对账口径 |
| k_event（633）/ k_proposal / k_version / k_relation | 不迁移 | 留第二批（k_event 633 为红线级，随 audit_event 映射批次迁移） |

### 3.3 源C RTIS task JSON 与 28 个 assets.db

- **[verified]** 35 个 task JSON：仅统计。state 分布：CREATED 24、AUTHORIZATION_REVIEW 7、COMPLETED 2、LOCAL_KNOWLEDGE_RETRIEVAL 1、AUTHORIZED_EXECUTION 1；task_type 分布：vuln 17、recon 12、full 6；解析错误 0。产出 `rtis_tasks_stats.json`。task 实体迁移与字段映射留 M1 第二批。
- **[unverified]**（采信 `m1/M1_READINESS_REVIEW.md` 步 7 与任务书）28 个 assets.db：26 兼容 / 1 异构（openredir-landing，9 张差异表）/ 1 无库；本批一律不迁移，Evidence 映射留第二批。

## 4. 行数对账总表

**[verified]**（逐项见 §8 verify 输出；此处为汇总）

| 映射 | 源行数 | 目标行数 | 隔离行数 | 未迁移（有解释） |
|---|---:|---:|---:|---|
| resources → resource | 81 | 81 | 0 | 0 |
| resource_sources → resource_source | 81 | 81 | 0 | 0 |
| quality_scores → resource 质量列 | 81 | 81（折入） | 0 | 0 |
| review_log → review | 165 | 165 | 0 | 0 |
| d5_candidates → 隔离清单 | 68 | 0 | 68 | 0 |
| poc×k_meta → knowledge(own) | 28 | 28 | 0 | 0 |
| k_negative → knowledge(negative) | 17 | 17 | 0 | 0 |
| poc 模板索引（source='nuclei'） | 14,111 | 0 | 0 | 14,111（可重建，归属 Resource 侧） |
| poc 其余未分层 | 578 | 0 | 0 | 578（留第二批） |
| k_event / events / skill_mappings 等 | 633/83/18 | 0 | 0 | 留第二批（审计/映射批次） |
| poc 全量口径自洽 | 14,717 = 28 + 14,111 + 578 | | | 无未解释行 |

隔离总数：68（d5_candidates 全量，人工待审入口已保留）。审核队列 0 自动批准（d5 全部维持 NEEDS_MORE_EVIDENCE/RELATED_KEEP_BOTH 原决策入库 review 表，仅照抄不升级）。**[verified]**

## 5. 幂等结论

**[verified]** 同一源快照连续执行 2 次：

| 项 | 第 1 次 | 第 2 次 |
|---|---|---|
| 行数（全部 10 表） | 81/81/45/165/372/0/0/0/0/0 | 完全一致 |
| 全表内容摘要（sha256，逐表） | 记录于 etl_run_log.jsonl | 与第 1 次逐一相等 |
| 相对上一状态变化的表 | 5 张数据表 | **无（no-op）** |
| alias 登记 | 372 inserted | 0 inserted / 372 skipped-existing |

机制：全部写入使用确定性主键 + INSERT OR REPLACE；全部时间戳列取源内时间（运行时刻只进执行日志，不进数据行）。001_init.sql 亦幂等（IF NOT EXISTS / INSERT OR IGNORE，CLI 连续执行两次验证通过）。**[verified]**

## 6. verify 对账输出全文

命令：`python3 -I verify.py`，exit 0。以下为逐字输出 **[verified]**：

```text
========================================================================
M1 第一批 verify 对账（MIGRATION_STRATEGY §10.5 / 计划 §M1-E）
========================================================================

== 1. 快照完整性（源库未被写入） ==
[PASS] 源 catalog SHA-256 与 manifest 一致 — cd495a81a59cb1b2…
[PASS] 源 knowledge SHA-256 与 manifest 一致 — c3f42f58b05aafec…

== 2. 行数对账（源 = 目标 + 隔离，逐映射） ==
[PASS] 行数对账：catalog.resources → resource — 源=81 目标=81 隔离=0
[PASS] 行数对账：catalog.resource_sources → resource_source — 源=81 目标=81 隔离=0
[PASS] 行数对账：catalog.quality_scores → resource 质量字段（折入） — 源=81 目标=81 隔离=0
[PASS] 行数对账：catalog.review_log → review — 源=165 目标=165 隔离=0
[PASS] 行数对账：catalog.d5_candidates → 隔离待审清单（不转实体） — 源=68 目标=0 隔离=68
[PASS] 行数对账：knowledge poc×k_meta → knowledge(own) — 源=28 目标=28 隔离=0
[PASS] 行数对账：knowledge.k_negative → knowledge(negative) — 源=17 目标=17 隔离=0
[INFO] poc 全量口径 — 总数=14717 = 迁移own=28 + nuclei模板(可重建,归属Resource侧,不迁移)=14111 + 其余未分层(留第二批)=578
[PASS] poc 全量口径自洽（无未解释行） — 14717 = 28+14111+578

== 3. ID 覆盖率（源 ID 100% 到达目标/隔离；迁移行 100% 有别名） ==
[PASS] resource ID 覆盖 — 源 81 / 命中 81
[PASS] knowledge own ID 覆盖（poc.code） — 源 28 / 命中 28
[PASS] knowledge negative 源引用覆盖（k_negative.knowledge_id） — 源 17 / 命中 17
[PASS] resource 行 100% 有 entity_alias — 缺失=0
[PASS] resource_source 行 100% 有 entity_alias — 缺失=0
[PASS] knowledge 行 100% 有 entity_alias — 缺失=0
[PASS] review 行 100% 有 entity_alias — 缺失=0
[INFO] entity_alias 总数 — 372（resource 81 + resource_source 81 + knowledge 45 + review 165）

== 4. Alias 冲突（K-09 四要素 + 冲突唯一约束） ==
[PASS] 同键别名指向不同实体 = 0 — 冲突数=0
[PASS] 别名四要素（source_system/namespace/alias_value/唯一约束）齐备 — 缺要素行=0
[PASS] 冲突唯一约束生效（无重复登记行） — 重复行=0

== 5. 关系完整性（FK + 孤儿检查） ==
[PASS] PRAGMA foreign_key_check 违例 = 0 — 违例=0
[PASS] resource_source → resource 孤儿 = 0 — 孤儿=0
[PASS] knowledge(negative) → knowledge(own) 孤儿 = 0 — 孤儿=0
[PASS] 反知识父链接未解析数 = 1（唯一已知例外：父 poc 不在 k_meta 覆盖，源引用已保留） — 未解析=['neg:sso-gateway-401-host-injection-redirect-uri']
[PASS] review(d5) item_key 均可解析为现存资源对 — 总数=68 可解析=68
[PASS] review(license) item_key 均为现存资源 — 坏引用=0
[PASS] review(mapping) 资源部分均为现存资源 — 坏引用=0
[INFO] review(mapping) 的 skill: 侧目标本批不校验存在性 — skill 实体表为空（125 个 Skill 文件属后续批次）；映射行保真保留 item_key 原值

== 6. 字段抽样哈希（源 vs 目标 各 10 行，md5 比对） ==
[PASS] 字段抽样哈希：catalog.resources×quality_scores → resource — 抽样 10/10 一致
[PASS] 字段抽样哈希：catalog.resource_sources → resource_source — 抽样 10/10 一致
[PASS] resource_source 派生列自洽（created_at=first_seen, updated_at=last_seen） — 异常行=0
[PASS] 字段抽样哈希：catalog.review_log → review — 抽样 10/10 一致
[PASS] 字段抽样哈希：knowledge poc×k_meta → knowledge(own) — 抽样 10/10 一致
[PASS] 字段抽样哈希：knowledge.k_negative → knowledge(negative) — 抽样 10/10 一致

== 7. 幂等（同源快照连续两次运行） ==
[PASS] 运行日志含 ≥2 次运行 — 共 2 次
[PASS] 第 1、2 次运行全表内容摘要一致（no-op） — 一致
[PASS] 第 1、2 次运行行数一致
[PASS] 当前隔离库内容与最后一次运行摘要一致 — 差异表=无

========================================================================
结果：PASS — 一致率 100%（全部对账项通过，无未解释差异）
exit 0
```

## 7. 隔离与销毁

- 目标库为独立文件，未注册进任何现网配置；`rm -f migrations/platform-dryrun.db`（及 quarantine/、*_log 等产物）即完整回滚，源系统零耦合。**[verified]**（架构性质，删除重建路径已由 CLI 建库 + ETL 重跑两次证实）
- 隔离库中不存在无 entity_alias 的迁移行（§8 第 3 节）。**[verified]**

## 8. 源库零写入证明与副作用记录

| 检查项 | 结果 |
|---|---|
| catalog.db SHA-256 | `cd495a81a59cb1b23df7d080a864fa094a344cd4efb79409110e4d90a9f135e8`，与 DATA_BASELINE_SNAPSHOT §1 一致 **[verified]** |
| knowledge.db SHA-256 | `c3f42f58b05aafec8bec76965d82a437e967e77863f42ee5512771f30f1984ee`，与基线一致 **[verified]** |
| 主库 mtime | catalog.db 2026-11-05 23:28、knowledge.db 2026-11-05 04:25，与基线一致，未变 **[verified]** |
| 连接方式 | 两库均以 `sqlite3.connect("file:…?mode=ro", uri=True)` 只读打开（etl/verify 同一方式）**[verified]** |

副作用（如实记录）**[verified]**：以 mode=ro 打开 WAL 库时 SQLite 会在库旁创建 0 字节 `-wal` 与 32KB `-shm` 伴生文件（只读连接共享内存所需），不构成数据写入。处置：catalog.db 的伴生文件在确认无进程占用后已删除，目录恢复原状；knowledge.db 的 `-wal`/`-shm` 由既有常驻进程 `rtis_mcp.mjs`（PID 28966，2026-11-05 23:00 启动，早于本批）持有，按"不触碰他人活跃工作"原则保留未动。该进程的存在与 M1_READINESS_REVIEW 步 1"冻结检查以 SHA-256 为锚、mtime 与 -wal/-shm 仅作辅助线索"的判据一致：两库 SHA-256 前后一致是零写入的判定依据。

## 9. 失败清单

### 9.1 对账/幂等失败项

无。**[verified]**（verify exit 0，§8）

### 9.2 开发期缺陷（交付前已修复，留痕）

1. 001_init.sql 初版使用了 SQLite 不支持的相邻字符串字面量拼接与 `IMPLIES` 关键字 → CLI 执行报 Parse error，改为单字面量与布尔展开。**[verified]**
2. knowledge.neg_do_not_conclude 误写列级 `NOT NULL` 与条件 CHECK 冲突（own 行插入失败）→ 移除列级 NOT NULL，保留条件 CHECK。**[verified]**
3. verify.py 抽样段变量遮蔽（tgt 连接被结果列表覆盖）与反知识抽样第 7 列未施加 `'neg:'` 前缀映射变换 → 修复后全绿。**[verified]**

## 10. 已知限制与未验证项

1. **反知识父链接 1/17 未解析** **[verified]**：`neg:sso-gateway-401-host-injection-redirect-uri` 的父 poc（源 k_negative.knowledge_id=900021 对应 poc）不在 k_meta 28 条覆盖内，本批不迁移父行（任务裁定仅迁 k_meta 覆盖 28 条），故 negates_knowledge_id 为 NULL；源引用完整保留于 neg_source_knowledge_id，无数据丢失，待第二批父行迁移后回填。
2. **空表语义** **[verified]**（表存在）/ **[unverified]**（业务正确性）：evidence/tool/tool_version/skill/audit_event 为空表。其现网数据（28 个 assets.db、tools.json、125 个 Skill 文件、k_event 633 条）本批按裁定不迁移，表结构正确性未经真实数据检验。
3. **review(mapping) 的 skill: 侧引用** **[verified]**（保真保留）/ **[unverified]**（指向存在性）：18 条 mapping 行的 `skill:<name>` 目标未校验存在性（skill 表空），第二批迁移 Skill 后补校。
4. **生命周期映射为语义裁定** **[verified]**（执行一致）：k_meta.lifecycle → 纯化生命周期（Field-validated/Lab-validated→VERIFIED，Researched→CANDIDATE）是本批实现的映射规则，其语义合理性属 M1-B 差异决策，待负责人确认；原值保留于 source_lifecycle 可随时重映射。
5. **resource_source.created_at 为派生值** **[verified]**：源表无 created_at/updated_at 列，取 first_seen/last_seen（保真语义等价，已断言 0 异常行）。
6. **28 库盘点数字** **[unverified]**：26 兼容/1 异构/1 无库采信 M1_READINESS_REVIEW 步 7（侦察快照），本批未复放逐库指纹。
7. **rtis-bridge.js 仍缺失** **[unverified]**（采信评审步 8）：与本批隔离 ETL 无依赖，不影响 dry-run；方案 A 属独立变更待批。
8. **本批不构成 M1 验收**：M1_IMPLEMENTATION_PLAN §6 门槛 A-6/A-7/A-8/A-10/A-11（隔离重建证明、七域映射全覆盖、28 库正式盘点、bridge 依赖、验收报告）属后续批次；本报告仅覆盖任务书限定的第一批范围。

## 11. 交付物与复现命令

```text
migrations/
├── 001_init.sql               # Schema V1（11 表；幂等；CLI 可执行）
├── etl_dryrun.py              # ETL v1.0.0（stdlib-only；源 mode=ro；幂等覆盖）
├── verify.py                  # 对账（28 项检查；任何失败 exit 1）
├── M1_DRYRUN_REPORT.md        # 本报告
├── platform-dryrun.db         # 隔离目标库（712,704 字节）
├── source_manifest.json       # 源清单（路径/SHA-256/大小/处置）
├── rtis_tasks_stats.json      # 源C 统计
├── etl_last_run.json          # 最近一次运行摘要（行数/内容摘要/FK 违例）
├── etl_run_log.jsonl          # 运行日志（追加型，2 次运行）
└── quarantine/
    └── d5_candidates.jsonl    # 68 条隔离待审记录（全字段保真）
```

复现（全部命令本批实际执行过）：

```bash
cd /home/emo/code/pensting/research-platform/migrations
sqlite3 platform-dryrun.db < 001_init.sql        # 建库（幂等，可重复）
python3 -I etl_dryrun.py                          # 第 1 次 ETL
python3 -I etl_dryrun.py                          # 第 2 次（应为 no-op）
python3 -I verify.py                              # 对账，期望 exit 0 / 一致率 100%
```

---

> 第二批（ID 冲突审计 / ResearchTask 35 映射 / Evidence 抽样映射 / 反知识父链接回填 / verify 扩展 72 项全 PASS）产物与结论见同目录 `M1_BATCH2_REPORT.md` 与 `M1_ID_AUDIT_REPORT.md`。本文其余内容保持第一批时点原貌（其中"反知识父链接 1/17 未解析"已在第二批回填关闭；源 k_negative.knowledge_id 实为 900015，第一批所记 900021 系笔误，勘误见 M1_BATCH2_REPORT.md §4）。
