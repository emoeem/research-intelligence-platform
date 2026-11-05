# M1 第二批 Dry-run 报告（ID 冲突审计 + ResearchTask + Evidence 抽样 + 反知识回填）

| 项 | 值 |
|---|---|
| 日期 | 2026-11-06 |
| 批次 | M1 第二批（接续第一批 commit d78163e，隔离环境 dry-run，不切流） |
| 工作目录 | `/home/emo/code/pensting/research-platform/migrations/` |
| 隔离目标库 | `migrations/platform-dryrun.db`（同一隔离库增量扩展，删除即回滚） |
| 依据 | 计划书 §5.4/§6.2/§M1-C/§M1-E；`m1/M1_READINESS_REVIEW.md`（K-09/K-11/R-01）；第一批 `M1_DRYRUN_REPORT.md` |
| 证据标签 | **[verified]** = 本批实际执行并有输出记录；**[unverified]** = 采信上游文档、未在本批复放 |

---

## 0. 结论摘要

1. **ID 冲突审计（计划 §5.4 硬门槛）通过**：三源全量审计（catalog resource_id 81、knowledge poc.code 14,717、RTIS task id 35 + 36 目录名）+ 目标库派生 ID（neg:* 17、review:* 165、EVD:* 23、research_task 35）。唯一性违例 0、跨源碰撞 0、格式违规 0、alias 与实体主键双向一致 0 缺失。结论：**"现网 ID 可直接沿用，无需批量生成新 ID"**。**[verified]**（详见 `M1_ID_AUDIT_REPORT.md` 与 `id_audit_result.json`）
2. **ResearchTask 映射 35/35**：全部 task JSON（解析错误 0）迁入 research_task 表，task_id 沿用现网 ID；研究状态原值保留（CREATED 24 / AUTHORIZATION_REVIEW 7 / COMPLETED 2 / LOCAL_KNOWLEDGE_RETRIEVAL 1 / AUTHORIZED_EXECUTION 1），status_history JSON 保真。幂等两次执行 no-op。**[verified]**
3. **Evidence 抽样映射 23 行**：26 兼容库中按确定性规则抽 3 个代表库（最大/最小/中等）+ 1 个结构覆盖补抽样库，http_evidence 与 shot 各取前 20 行（实际 21 + 2），evidence_id 确定性派生（`EVD-<库slug>-<源表>-<源行id>`），source_ref 经 source_pk 保留（`库绝对路径#表#行id`），payload_json 全字段保真。2 个非兼容对象（openredir-landing 异构、王红娘-520-wanghongniang-com 无库）只登记隔离不抽取。**[verified]**
4. **反知识父链接回填 1/1**：第一批遗留的 `neg:sso-gateway-401-host-injection-redirect-uri` 悬空父引用已回填，17/17 反知识 negates 链接全部可解析且等于源父 poc.code。**[verified]**
5. **verify 扩展后全量对账**：10 个检查段、72 项 PASS、0 FAIL、一致率 100%，exit 0。新增 task 行数对账、evidence 抽样对账（含全字段逐行复建）、ID 审计结论检查三段。**[verified]**（全文见 §6）
6. 失败清单：对账/幂等无失败项；开发期 3 个实现缺陷已在交付前修复并留痕（§8）。**[verified]**

## 1. 任务 1：ID 冲突审计（id_audit.py v1.0.0）

完整报告独立成文：`M1_ID_AUDIT_REPORT.md`（人读）+ `id_audit_result.json`（机器可读，verify §10 对账输入）。要点：**[verified]**

| 检查 | 结果 |
|---|---|
| A. catalog resource_id（81，gh 79 + url 2） | 唯一、格式合规（gh:owner/repo、url:16hex） |
| B. knowledge poc.code（14,717 全量，含 14,111 nuclei 模板） | 唯一（UNIQUE(code) 约束实测成立）、格式合规（非空/无空白/无控制字符；ascii-only 14,592 + 含 CJK 125，CJK 为源设计内 slug 组成） |
| C. RTIS task id（35 JSON + 36 目录名） | 唯一、格式合规（rtis-YYYYMMDD-HHMMSS-ms） |
| D. 目标库派生 ID（17 + 165 + 23 + 35） | 唯一、前缀格式合规 |
| 跨源碰撞 | {A,B,C,D} 两两交集 = 0；rtis_task_id ∩ research_task 主键 = 35 为有意沿用（intentional reuse，非冲突） |
| alias↔主键一致性 | 6 个非空实体类型双向 0 缺失（resource 81 / resource_source 81 / knowledge 46 / review 165 / research_task 35 / evidence 23） |

备查发现（不构成冲突）：任务目录与 JSON 配对差异（6 目录无 JSON、5 JSON 无目录，属工件布局事实）；poc.code 含 CJK 125 条（源设计，若 M2+ 收窄 ID 域须走别名桥，不得原地改写）。**[verified]**

## 2. 任务 2：ResearchTask 映射（etl_tasks.py v1.0.0 + 002_tasks.sql）

### 2.1 建表（002_tasks.sql）

`research_task`（task_id 主键沿用现网 ID、title、objective、target、task_type、state、status_history JSON、source_file、source_system、created_at/updated_at、schema_version），两索引（state、task_type）。`schema_version` 表登记 version=2 行。**[verified]**

版本策略说明（如实记录）：PRAGMA user_version 在 M1 dry-run 阶段锚定 V1（001_init.sql 每次幂等执行重申 user_version=1），第二批追加变更以 schema_version 表 version=2 行登记；正式 user_version 递增机制随 M2 migration 框架统一收口。**[verified]**（设计裁定，非上游规定）

同文件记录两处由脚本守卫执行的增量 DDL（SQLite 无条件 DDL）：evidence.payload_json 追加列（etl_evidence.py）、entity_alias CHECK 域扩展加入 'research_task'（etl_tasks.py，表重建方式，431 行全量拷贝无丢失，由 id_audit §2.4 双向一致佐证）。**[verified]**

### 2.2 映射规则

| 源（task JSON） | 目标 | 规则 |
|---|---|---|
| task.id | task_id | 现网 ID 直通（rtis-YYYYMMDD-HHMMSS-ms） |
| task.status（或 state） | state | **原值直通，不归一化**（CREATED/AUTHORIZATION_REVIEW/LOCAL_KNOWLEDGE_RETRIEVAL/AUTHORIZED_EXECUTION/COMPLETED 语义原样保留） |
| task.status_history | status_history | JSON 数组原样序列化（ensure_ascii=False, sort_keys=True），迁移无语义损失 |
| task.created_at / updated_at | created_at / updated_at | 源内时间直通（确定性） |
| task.title / objective / target / task_type | 同名列 | 直通 |
| 源 JSON 文件名 | source_file | 追溯线索 |
| authorization / environment / knowledge 等大 JSON 段 | 不迁移 | 留 M3+ ResearchTask 全量建模（任务书字段范围之外） |

### 2.3 迁移结果 **[verified]**

- 源 35 JSON = 目标 35 行，ID 集合逐一相等，解析错误 0。
- 状态分布与源完全一致：CREATED 24、AUTHORIZATION_REVIEW 7、COMPLETED 2、LOCAL_KNOWLEDGE_RETRIEVAL 1、AUTHORIZED_EXECUTION 1（与第一批 `rtis_tasks_stats.json` 基线一致）。
- 35/35 行有 entity_alias（K-09 四要素：source_system='rtis-tasks'、namespace='rtis.data.tasks.task.id'）。
- 幂等：连续两次执行，第二次 no-op（research_task 内容摘要一致、alias 全部 skipped-existing）。

## 3. 任务 3：Evidence 抽样映射（etl_evidence.py v1.0.0）

### 3.1 逐库盘点（第一批 [unverified] 项在本批升级为 [verified]） **[verified]**

27 个 assets.db 实存 + 1 个 engagement 无库，共 28 engagement：**26 库表集指纹同构（兼容）**、openredir-landing 异构（38 个表名含 sqlite_sequence，即 37 张业务表，偏离 26 库基线）、王红娘-520-wanghongniang-com 无 assets.db。与 M1_READINESS_REVIEW 步 7 的侦察结论一致（26/1/1）。

隔离登记：`quarantine/assets_isolation.jsonl` 覆盖全部 28 个 engagement（名称 + 类别 + 原因 + 处置），2 条隔离条目：**[verified]**

| engagement | 类别 | 原因 |
|---|---|---|
| openredir-landing | heterogeneous | 表集指纹偏离多数派（26 库同构基线），37 张业务表含 9 张差异表（evidence_index/plan_revision/postmortem/profile_delta/research_plan/site_feature/site_profile/state_transition/workflow_run），未知结构不猜测转换（计划 §6.2） |
| 王红娘-520-wanghongniang-com | no_db | 目录无 assets.db（仅 engagement.yaml/SCOPE.md 等，无证据库） |

### 3.2 选库规则（确定性，规则写入运行日志） **[verified]**

- 最大 / 最小 / 中等：兼容库按（文件大小，行数多，路径字典序）排序取两端；中等 = 文件大小最接近中位数（481,280 字节）的兼容库，平局依次按（行数多、路径）决出。
- 结构覆盖补抽样：任务指定三库对 shot 表抽到 0 行（shot 数据集中在少数库），为达成"验证 schema 可容纳真实证据结构"的目的，追加 1 个含 shot 数据的最小兼容库，角色标记 `supplemental-shot`。这是任务书"最大/最小/中等"之外的一次明示扩展（非静默替代）。

### 3.3 抽样与迁移结果 **[verified]**

| 库 | 角色 | http_evidence（源总数/抽取） | shot（源总数/抽取） |
|---|---|---|---|
| src-fofa-hunt-2026-11 | min（360,448 B） | 1 / 1 | 0 / 0 |
| jyb40-z0m5a6-top-问卷投放站 | median（487,424 B） | 10 / 10 | 0 / 0 |
| 联想-lenovo-src | max（4,382,720 B） | 6 / 6 | 0 / 0 |
| jyb40-z0m5a6-top | supplemental-shot（442,368 B） | 4 / 4 | 2 / 2 |
| 合计 | | 21 | 2 |

evidence 表共 23 行，23/23 有 entity_alias（alias_value = source_ref 全路径串，规避跨库自增 id 碰撞）。幂等两次执行 no-op。**[verified]**

### 3.4 映射规则与 schema 容纳性结论

| 源 | 目标 | 规则 |
|---|---|---|
| evidence_id | `EVD-<库slug>-<源表>-<源行id>` | 库slug = ascii 片段（小写归并，截 32）+ md5(目录名) 前 8 位；纯中文名退化为纯 md5 前缀；28 个 slug 经断言全局唯一 |
| source_ref | source_pk 列 | `<库绝对路径>#<源表>#<源行id>`（任务书要求的 库路径#表#行id） |
| http_evidence → evidence_type='http_pair' | content_hash | sha256(method|url|status|request|response 规范拼接) |
| shot → evidence_type='screenshot' | storage_ref=源 file、content_hash=源 sha256（真实内容哈希直通）、captured_by=agent（回退 found_by_agent） | |
| 全部源列 | payload_json（002 批次追加列） | 源行全字段 JSON（ensure_ascii=False, sort_keys=True），逐字节可复原 |
| captured_at / captured_at 或 created_at | captured_at | 时间戳断言：无时间戳行直接 FATAL（实际 0 行触发） |
| lifecycle/governance | 'RAW' / 'ACTIVE' | 证据默认生命周期链首；sensitivity/permission_level 置空（现网源无该维度，不得猜测） |

**容纳性结论 [verified]**：V1 evidence 核心列（id/type/storage_ref/content_hash/captured_at/captured_by/task_ref/lifecycle/governance/source_system/source_pk）可承载真实证据行的核心结构；业务关联列（http_evidence 的 vuln_id/asset_id/label/method/url/status/note，shot 的 point_id/hit_id/width/height 等）V1 无对应列，经 payload_json 全字段保真 + source_pk 回溯承载，无数据丢失。正式的 Evidence 字段映射矩阵（哪些源列该升为平台列）属 M1-C 收尾产物，本批不越权裁定。

## 4. 任务 4：反知识父链接回填（etl_dryrun.py v1.1.0）

- 第一批遗留 1/17 悬空：`neg:sso-gateway-401-host-injection-redirect-uri` 的父 poc 存在于源库（k_negative.knowledge_id=900015 → poc 900015，code 与负知识同名），但不在 k_meta 28 条分层覆盖内。**[verified]**
- **勘误 [verified]**：第一批报告 §10.1 写"源 k_negative.knowledge_id=900021"，实为 **900015**（本批复查源库确认），后续以本批为准。
- 回填实现（v1.1.0，B2 段）：父 poc 不在 k_meta 覆盖时，若父行存在于源则迁移为 knowledge(record_kind='own', lifecycle='DISCOVERED', k_meta 分层字段全部 NULL)，登记 poc.code 别名，并回填 negates_knowledge_id=父code。选择在 etl_dryrun B2 内实现而非独立脚本，原因是 B2 以 INSERT OR REPLACE 全行覆写负知识行，独立脚本先回填会被后续 etl_dryrun 重跑覆写回 NULL；实现在同一 Transform 内才能保证幂等性不被破坏。**[verified]**
- 结果：knowledge 表 46 行 = own 29（k_meta 28 + 父回填 1）+ negative 17；17/17 反知识 negates 链接全部可解析且等于源父 poc.code（verify §5 全量校验）；FK 违例 0；未分层 poc 净未迁移数由 578 修正为 577（1 行经回填迁出）。**[verified]**
- 生命周期语义裁定（留负责人确认，同第一批限制 4 的口径）：回填父行无 k_meta 分层，lifecycle 取链首 DISCOVERED（成熟度未知的最小主张），负知识行继承 DISCOVERED（与第一批对该行的处理一致）。

## 5. 任务 5：verify.py 扩展与全量对账

新增三段（任务书指定三项）：**[verified]**

- **§8 ResearchTask 对账**：源 35 JSON = 目标 35 行、ID 集合一致、状态分布与源一致（原值保留）、status_history JSON 保真可解析、35/35 有别名、schema_version version=2 已登记、etl_tasks 重复执行 no-op（其运行日志最近两次摘要一致）。
- **§9 Evidence 抽样对账**：隔离登记恰 2 条且原因齐备、28 库盘点口径（26+1+1）、逐库逐表"源总数/应抽/目标行数"对账、**全字段保真逐行复建比对**（verify 侧独立重建 payload_json 与 content_hash，与目标库逐行比对，4 库 23 行全部一致）、23/23 有别名、etl_evidence 重复执行 no-op。
- **§10 ID 审计结论检查**：id_audit_result.json 存在且唯一性/跨源碰撞/格式/alias 四项全过、结论为"现网 ID 可直接沿用，无需批量生成新 ID"、审计结果与当前库同步（alias 总数 431 = 审计时点，防陈旧）。

另有四处口径更新：knowledge(own) 对账拆分为 k_meta 覆盖行（28）+ 父回填行（1，且回填集合必须恰等于源中"不在 k_meta 覆盖的反知识父集合"）；poc 全量口径更新为 14,717 = 29 + 14,111 + 577；反知识父链接未解析数检查 1 → 0 并新增 17/17 negates=源父code 全量校验；entity_alias 总数 INFO 行改为动态统计（431 = evidence 23 + knowledge 46 + research_task 35 + resource 81 + resource_source 81 + review 165）。

**最终结果 [verified]**：10 段、72 PASS、0 FAIL、4 INFO，一致率 100%，exit 0（全文输出存档于本报告工作过程，节选见下）。第一批 28 项检查全部继续 PASS，红线（一致率 100%）维持。

```text
== 7. 幂等 ==
[PASS] 运行日志含 ≥2 次运行 — 共 6 次
[PASS] 第 1、2 次运行全表内容摘要一致（no-op） — 一致
[PASS] 当前隔离库内容与最后一次运行摘要一致 — 差异表=无

== 8. ResearchTask 对账（第二批映射，源只读） ==
[PASS] 行数对账：RTIS task JSON → research_task — 源=35 目标=35
[PASS] task ID 集合一致（现网 ID 直接沿用为平台主键） — 差异=[]
[PASS] 研究状态分布与源一致（state 原值保留，不归一化） — 源={AUTHORIZATION_REVIEW:7, AUTHORIZED_EXECUTION:1, COMPLETED:2, CREATED:24, LOCAL_KNOWLEDGE_RETRIEVAL:1} 目标=同
[PASS] status_history JSON 保真可解析（数组） — 坏行=[]
[PASS] research_task 行 100% 有 entity_alias — 缺失=0
[PASS] schema_version 已登记第二批增量（version=2） — 行数=1
[PASS] etl_tasks 重复执行 no-op（最近两次运行摘要一致） — 共 2 次运行

== 9. Evidence 抽样对账（第二批映射，源只读） ==
[PASS] 异构/无库隔离登记 = 2 且名称与原因齐备
[PASS] 28 库盘点口径（26 兼容 + 1 异构 + 1 无库） — 兼容=26 总数=28
[PASS] 抽样对账×8（4 库 × 2 表：源总数/应抽/目标行逐一相等）
[PASS] 全字段保真×8（payload_json + content_hash 逐行复建，不一致行=0）
[PASS] evidence 行 100% 有 entity_alias — 缺失=0
[PASS] etl_evidence 重复执行 no-op（最近两次运行摘要一致） — 共 2 次运行

== 10. ID 冲突审计结论（计划 §5.4，id_audit.py 输出对账） ==
[PASS] ID 全局唯一性（三源全量 + 派生） — 违例=0
[PASS] 跨源碰撞 = 0（intentional reuse 单列不计冲突） — 碰撞=0
[PASS] ID 格式合规（resource_id/poc.code/task id/派生前缀） — 违规=0
[PASS] alias↔实体主键双向一致 — 缺失=0
[PASS] 审计结论（硬门槛） — 现网 ID 可直接沿用，无需批量生成新 ID
[PASS] 审计结果与当前隔离库同步（无陈旧） — 审计时 alias=431 当前=431

结果：PASS — 一致率 100%（全部对账项通过，无未解释差异）
exit 0
```

## 6. 源库零写入证明

| 检查项 | 结果 |
|---|---|
| catalog.db / knowledge.db SHA-256 | 与 source_manifest.json 一致（verify §1 PASS），manifest 哈希与第一批基线相同（cd495a81… / c3f42f58…）**[verified]** |
| 打开方式 | 三源全部 `sqlite3 URI mode=ro`（catalog/knowledge/assets.db 同一方式）；task JSON `open()` 只读 **[verified]** |
| assets.db 主文件 | 27 个主文件 mtime 均早于本批（2026-10-24 / 11-02，本批运行于 11-06 02:1x），未变 **[verified]** |
| assets.db 伴生文件（如实记录）**[verified]** | 27 库均为 WAL 模式；`-wal` 全部 0 字节且 mtime 为 2026-11-04 19:09（早于本批，非本批产物，未触碰）；`-shm` mtime 更新为本批时刻，系只读连接的共享内存伴生现象（与第一批报告 §8 对 knowledge.db 的记录同因），不构成数据写入。判定依据同 M1_READINESS_REVIEW 步 1：主库文件 mtime 与 0 字节 wal 为锚。伴生文件一律保留未删（不触碰非本批创建的文件） |
| 生产代码 | security-research-upgrade / engagements / skills / knowledge.db 零改动 **[verified]**（本批新文件全部位于 research-platform/migrations/ 与 docs/ 之外未涉及） |

## 7. 目标库最终状态 **[verified]**

| 表 | 行数 | 变化（相对第一批） |
|---|---:|---|
| resource / resource_source | 81 / 81 | 不变 |
| knowledge | 46 | +1（父回填行；own 28→29，negative 17 不变） |
| review | 165 | 不变 |
| entity_alias | 431 | +59（research_task 35 + evidence 23 + 父回填 1） |
| evidence | 23 | 0→23（第二批抽样） |
| research_task | 35 | 新表 |
| tool / tool_version / skill / audit_event | 0 / 0 / 0 / 0 | 不变（M6/后续批次） |

隔离产物：quarantine/d5_candidates.jsonl 68 行（第一批）+ quarantine/assets_isolation.jsonl 28 行（第二批盘点登记）。

## 8. 失败清单

### 8.1 对账/幂等失败项

无（最终 verify 72/72 PASS，exit 0）。**[verified]**

### 8.2 开发期缺陷（交付前已修复，留痕）

1. etl_evidence.py 初版缺少 open_target() 定义 → NameError；补上后重跑通过。**[verified]**
2. verify.py §9 抽样对账的 source_pk LIKE 模式漏尾部通配符 `%`（退化为精确匹配）→ 5 项误报 FAIL；修复后 8/8 PASS。**[verified]**
3. verify.py §2 父回填集合查询绑定参数漏元组包装 → sqlite3.ProgrammingError；修复后通过。**[verified]**
4. etl_dryrun.py 统计键 kb_other_not_migrated 语义滞后（578 未扣除回填 1 行）→ 拆分为 kb_other_unstratified_total(578) 与 kb_other_not_migrated(577)，重跑两次确认 no-op。**[verified]**

## 9. 已知限制与未验证项

1. **Evidence 仅抽样** **[verified]**（抽样）/ **[unverified]**（其余库内容）：按任务口径仅 4/26 兼容库、每表前 20 行；其余 22 个兼容库未迁移（行数与指纹已登记），全量 Evidence 迁移属 M1 收尾后批次。
2. **Evidence 字段映射矩阵未定** **[verified]**（保真承载）/ **[unverified]**（升列裁定）：payload_json 保证无丢失，但哪些源列应升为平台列属 M1-C 收尾差异决策，本批不裁定。
3. **shot 二进制本体未迁移** **[verified]**：storage_ref 为源库内 file 值（如 20261022100256-shot-01.png），截图文件在 engagement 目录中原位保留，经 source_pk 可回溯；二进制迁移不在本批范围。
4. **http_evidence 请求/响应体不占平台列** **[verified]**：经 payload_json 逐字节保留（可复原），dedup/索引策略留 M2+。
5. **research_task 仅映射任务书指定字段** **[verified]**：authorization/environment/knowledge 等大 JSON 段不迁移，留 M3+ ResearchTask 全量建模；status_history 已保真。
6. **回填父行生命周期为语义裁定** **[verified]**（执行一致）：DISCOVERED 无 k_meta 分层，语义合理性待负责人确认（同第一批限制 4 口径）。
7. **user_version 保持 1** **[verified]**（设计裁定）：第二批以 schema_version version=2 行登记，正式递增机制随 M2 migration 框架收口。
8. **任务目录/JSON 配对差异** **[verified]**（登记备查）：6 目录无 JSON、5 JSON 无目录，task 迁移以 JSON 的 task.id 为准（35 个）。
9. **本批不构成 M1 验收**：M1 收尾验收（隔离重建证明、七域映射全覆盖、28 库正式盘点报告、bridge 依赖、验收报告）仍属后续批次；本报告覆盖任务书第二批范围。

## 10. 交付物与复现命令

```text
migrations/ 新增：
├── 002_tasks.sql                # 第二批增量（research_task 建表 + schema_version v2；幂等）
├── etl_tasks.py                 # ResearchTask ETL v1.0.0（35 JSON 映射；幂等）
├── etl_evidence.py              # Evidence 抽样 ETL v1.0.0（4 库 23 行；幂等）
├── id_audit.py                  # ID 冲突审计 v1.0.0（全量只读；exit 0）
├── M1_ID_AUDIT_REPORT.md        # ID 审计报告（结论：现网 ID 可直接沿用）
├── id_audit_result.json         # 审计机器可读结果（verify §10 输入）
├── M1_BATCH2_REPORT.md          # 本报告
├── etl_tasks_last_run.json / etl_tasks_run_log.jsonl
├── etl_evidence_last_run.json / etl_evidence_run_log.jsonl
└── quarantine/assets_isolation.jsonl   # 28 engagement 盘点与隔离登记
migrations/ 修改：
├── etl_dryrun.py                # v1.0.0 → v1.1.0（父回填 + research_task 入摘要 + 统计键修正）
├── verify.py                    # +3 对账段与口径更新（28 项 → 72 项检查）
└── platform-dryrun.db           # 隔离库增量（knowledge 46 / evidence 23 / research_task 35 / alias 431）
```

复现（全部命令本批实际执行过，顺序敏感：tasks → evidence → dryrun×2 → audit → verify）：

```bash
cd /home/emo/code/pensting/research-platform/migrations
python3 -I etl_tasks.py && python3 -I etl_tasks.py      # 第 2 次 no-op
python3 -I etl_evidence.py && python3 -I etl_evidence.py # 第 2 次 no-op
python3 -I etl_dryrun.py && python3 -I etl_dryrun.py     # 第 2 次 no-op（v1.1.0 含父回填）
python3 -I id_audit.py                                   # 期望 exit 0 + 结论"现网 ID 可直接沿用"
python3 -I verify.py                                     # 期望 72 PASS / 一致率 100% / exit 0
```
