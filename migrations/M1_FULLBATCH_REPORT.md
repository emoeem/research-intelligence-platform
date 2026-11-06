# M1 全量批报告（Evidence 全量迁移 + ResearchTask 大 JSON 段 + verify 扩展 + 生产库就位）

| 项 | 值 |
|---|---|
| 日期 | 2026-11-07 |
| 批次 | M1 全量批（接续第二批 commit 后的 M6/M8 切换态，隔离环境重建，最后落生产库） |
| 工作目录 | `/home/emo/code/pensting/research-platform/migrations/` |
| 全量批目标库 | `migrations/platform-fullbatch.db`（自 `platform-dryrun.db` 快照复制起点，增量扩展；验证 100% 后复制为生产库） |
| 生产库 | `migrations/platform.db`（本批升级为全量生产批；gitignore，重建流程 = 本报告 §7） |
| 依据 | `M8_READINESS.md` 第三节已知限制 1 与 2（本批关闭项）；第一批/第二批报告口径；`MIGRATION_STRATEGY` §10.1/§6.2/§6.3；计划 §6.3 ETL 原则 |
| 证据标签 | **[verified]** = 本批实际执行并有输出记录 |

---

## 0. 结论摘要

1. **Evidence 全量迁移 700/700**：26 个兼容库全部迁移（第二批仅抽样 4 库），每源表无行数上限（第二批每表前 20 行）。http_evidence 632 行 + shot 68 行 = 700 行全部入库，坏行隔离 0 行，700/700 有 entity_alias。隔离库保持隔离（1 异构 + 1 无库），未硬迁。**[verified]**
2. **ResearchTask 大 JSON 段 126/126**：42 个 task JSON（快照口径）顶层 authorization / environment / knowledge 三段全部保真序列化入库（规范 json.dumps 逐字节 + 可 json.loads 复原为等价对象），共 153,990 字节；坏行隔离机制落地（本批 0 坏行），解析失败与序列化失败不再静默丢弃。**[verified]**
3. **verify 扩展后全量对账**：11 个检查段、203 项 PASS、9 项 INFO、0 FAIL、一致率 100%、exit 0；对 `platform-fullbatch.db` 与 `platform.db`（生产库）各复跑一次，结果一致。**[verified]**
4. **幂等**：etl_tasks（v2.0.0）、etl_evidence（v2.0.0）、etl_dryrun（v1.1.1）各连续执行两次，第二次均为 no-op（变化表为空、全表内容摘要与行数逐一相等）；运行日志对比见 §5。**[verified]**
5. **源库零写入**：31 个源文件（catalog 活源+快照、knowledge 活源+快照、27 个 assets.db）sha256 与 mtime 前后锚点对比全部一致，0 变化。**[verified]**（见 §6，锚点含 mtime，严于前两批的 SHA 锚点）
6. **生产库就位**：`platform-fullbatch.db` 验证 100% 后复制为 `platform.db` 并复跑 verify（203 PASS / exit 0）；quick_check ok；schema_version 1/2/3/4 四行齐备。**[verified]**
7. 失败清单：对账、幂等、源库写入检查均无失败项。**[verified]**

## 1. 任务 1：ResearchTask 大 JSON 段映射（etl_tasks.py v1.0.0 → v2.0.0）

### 1.1 映射规则（版本化，v2.0.0） **[verified]**

| 源（task JSON 顶层段） | 目标列 | 规则 |
|---|---|---|
| `authorization` | research_task.authorization_json（追加列） | 键存在且值非 null → `json.dumps(value, ensure_ascii=False, sort_keys=True)` 保真序列化；键缺失或 null → 列 NULL（忠实于源，不造空对象、不猜默认值） |
| `environment` | research_task.environment_json（追加列） | 同上 |
| `knowledge` | research_task.knowledge_json（追加列） | 同上 |
| 序列化失败 | 隔离清单 | 整行写入 `quarantine/research_task_isolation.jsonl`（确定性覆写），不迁移、不静默丢弃 |
| 源 JSON 解析失败 | 隔离清单 | 同上（第二批行为是计入 parse_errors 后静默跳过，本批改为必须留隔离痕迹） |

schema：三列为 `ALTER TABLE ADD COLUMN TEXT` 追加列（SQLite 无条件 DDL，由 etl_tasks.py `ensure_task_json_columns()` 守卫执行），权威记录 `004_fullbatch.sql` + `schema_version` 登记 version=4。第二批的 version=2 与 M6 的 version=3 行不受影响。**[verified]**

### 1.2 迁移结果 **[verified]**

- 源 42 JSON（快照 `sources-snapshot/tasks/`，M8 §6.3 冻结口径）= 目标 42 行，ID 集合逐一相等，解析错误 0，隔离 0。
- 三段覆盖 126/126（42 文件 × 3 段全部非空）：authorization 7,278 字节、environment 14,032 字节、knowledge 132,680 字节，合计 153,990 字节规范序列化入库。
- 大 JSON 段逐行保真复建比对（verify §8）：42 行 × 3 段规范序列化逐字节相等，且每段可 `json.loads` 复原为与源等价的对象，坏段 0。
- 核心字段（task_id/state/status_history 等）与第二批口径一致：状态分布与源一致原值保留（CREATED 28 / AUTHORIZATION_REVIEW 7 / WORKFLOW_PLANNING 3 / COMPLETED 2 / AUTHORIZED_EXECUTION 1 / LOCAL_KNOWLEDGE_RETRIEVAL 1，42 个，快照口径较第二批的 35 个快照增长部分随快照版本演进）。
- 42/42 行有 entity_alias；FK 违例 0。

## 2. 任务 2：Evidence 全量迁移（etl_evidence.py v1.0.0 → v2.0.0）

### 2.1 范围与选库规则 **[verified]**

- 兼容判定不变：assets.db 表集指纹与多数派一致 = 兼容。本批盘点 28 个 engagement：**26 兼容 + 1 异构（openredir-landing）+ 1 无库（王红娘-520-wanghongniang-com）**，与第二批一致。
- v2.0.0 选库规则：**全部兼容库（26 个）全量选择**，角色标记 `compatible-full`；废止第二批"最大/最小/中等 + 补抽样"规则与每表前 20 行上限（`SAMPLE_LIMIT` 常量移除，运行日志 `mode=full`、`sample_limit_per_table=null`）。
- 映射域边界（如实说明）：兼容库基线共 28 张表，本批 evidence 映射域仍为第二批定的 `http_evidence` 与 `shot` 两张证据表，映射规则 v1 不变（evidence-mapping-v1-full）；其余表（asset/vuln/scan_run/score_hit 等）属其他实体域，其建模归属 M2+ 批次，不在"evidence 全量迁移"范围内，无行数丢失口径问题。
- 隔离不变：异构库与无库 engagement 只登记 `quarantine/assets_isolation.jsonl`（28 行盘点 + 2 条隔离），不抽取、不猜测转换。**[verified]**

### 2.2 逐库迁移结果（26 库 × 2 表，源总数全量） **[verified]**

| engagement | http_evidence | shot |
|---|---:|---:|
| bet-cash-cc-多货币博彩平台集群-bet-play01-08-bet-sports01-02-admin-bet-cash-cc | 80 | 3 |
| flash-photo-cat-bot-telegram-bot | 0 | 0 |
| https-hudasmvp-com | 4 | 0 |
| hudasmlab-com | 66 | 1 |
| jyb40-z0m5a6-top | 4 | 2 |
| jyb40-z0m5a6-top-问卷投放站 | 10 | 0 |
| qt0008 | 0 | 0 |
| src-fofa-hunt-2026-11 | 1 | 0 |
| src-pc后台管理系统-高危漏洞 | 0 | 0 |
| 丁香园-dxysrc | 18 | 0 |
| 上海富勒信息科技有限公司 | 23 | 0 |
| 佳缘-cpi | 0 | 0 |
| 公网高易打面资产-fofa-暴露面测绘 | 0 | 0 |
| 六后台管理系统-src-2026-11 | 88 | 21 |
| 博彩站群-34域名-13节点-证书san集群 | 12 | 0 |
| 厦门傲众网络科技有限公司 | 28 | 3 |
| 婚恋交友小程序安全评估-珍爱-佳缘-牵手-我主良缘-一伴-伊对-一线姻缘 | 92 | 0 |
| 婚恋交友小站候选池 | 0 | 0 |
| 婚恋交友小站候选池-fofa测绘 | 0 | 0 |
| 婚恋交友站点测绘-2026-11 | 1 | 0 |
| 婚恋平台-app | 39 | 23 |
| 开放重定向批次-dy11-22域名 | 15 | 6 |
| 彩票-博彩站点群-pg-cm-153-vip-7811-com-等-30-域名与-ip-端口 | 0 | 0 |
| 王红娘 | 19 | 0 |
| 白牌包网盘口系统集群-实体a-12端点-10品牌-saas-实体b-彩票导航站群 | 126 | 9 |
| 联想-lenovo-src | 6 | 0 |
| **合计（26 库）** | **632** | **68** |

- evidence 表 23 行（第二批抽样）→ **700 行**（全量），净增 677；700/700 有 entity_alias（namespace `assets.<表>.id`，alias_value = source_pk 全路径串），entity_alias 总数 596 → 1,273。
- 坏行隔离 0 行：全量预检与实际迁移均无时间戳断言失败行（第二批"无时间戳直接 FATAL"的断言在本批改为逐行隔离，机制在场、本批未触发）。
- 如实记录：4 行 http_evidence 的 captured_by 源值为 NULL，列可空，忠实承载不猜测。
- shot 二进制本体仍未迁移（与第二批口径一致）：storage_ref 为源库内 file 值，文件在 engagement 目录原位保留，经 source_pk 可回溯。

## 3. 任务 3：verify.py 扩展（对账语义保持既有红线）

新增/重写的对账项（红线不变：一致率 100%，快照对快照，孤儿全域校验，幂等分组判定）：**[verified]**

- **§8 ResearchTask（第二批核心字段 + 全量批大 JSON 段）**：坏行隔离登记数 = 源解析失败文件数（不静默丢弃）；行数对账 源 = 目标 + 隔离；ID 集合一致；状态分布原值一致；status_history 保真；**三列在场（004 守卫 DDL）+ schema_version version=4 登记；authorization/environment/knowledge 逐行保真复建（规范序列化逐字节相等 + json.loads 复原等价）**；别名覆盖；etl_tasks 同源快照分组幂等。
- **§9 Evidence（全量批）**：运行日志 mode=full 且无抽样上限；异构/无库隔离登记恰 2 条；28 库盘点口径（26+1+1）；**坏行隔离清单 = 运行日志隔离计数且条目齐备；逐库逐表全量对账（源总数 = 迁移数 + 隔离数 = 目标行数 + 隔离数）；全字段 payload_json + content_hash 逐行复建比对，覆盖全部 700 行（无 LIMIT），隔离坏行除外且坏行不得落库**；别名覆盖；etl_evidence 最近两次运行 no-op。
- 其余各段（§1 快照完整性、§2 行数对账、§3 ID 覆盖、§4 Alias 冲突、§5 关系完整性、§6 字段抽样哈希、§7 幂等、§10 ID 审计、§11 M6 域）语义未动；§11 复核确认 M6 域（tool/skill/workflow 与库内 etl_run_log）在复制出的 fullbatch 库上继续全部 PASS。
- 目标库可配置：`RP_TARGET_DB` 环境变量（相对路径按 migrations/ 解析），默认仍为 `platform-dryrun.db`（既有 dryrun 复现流程不受影响）。etl_tasks / etl_evidence / etl_dryrun / id_audit / verify 五脚本一致支持。

**最终结果 [verified]**：对 `platform-fullbatch.db`：11 段、203 PASS、9 INFO、0 FAIL、一致率 100%、exit 0；对生产库 `platform.db`（复制后）：同一结果。既有 72 项检查口径全部继续 PASS。

## 4. 任务 4：幂等（重跑两次，第二次 no-op） **[verified]**

三个职责域 ETL 在 `platform-fullbatch.db` 上各连续执行两次，运行日志（追加型 JSONL）最近两次运行对比：

| ETL | 日志 | 第 1 次变化表 | 第 2 次变化表 | 摘要一致 | 行数一致 |
|---|---|---|---|---|---|
| etl_tasks v2.0.0 | etl_tasks_run_log.jsonl（共 6 次） | research_task | （无） | 是 | 是 |
| etl_evidence v2.0.0 | etl_evidence_run_log.jsonl（共 4 次） | evidence | （无） | 是 | 是 |
| etl_dryrun v1.1.1 | etl_run_log.jsonl（共 9 次） | （无） | （无） | 是 | 是 |

no-op 细节（etl_evidence 第 2 次）：evidence_inserted=0、evidence_skipped_existing=700、alias_inserted=0、alias_skipped_existing=700，content_digests 与第 1 次逐表相等。etl_tasks 第 2 次全部 skipped-existing，stats 与第 1 次无差异键。第 2 次 etl_dryrun 与第 1 次逐键相等。

运行顺序（与摘要域归属一致：etl_dryrun 的内容摘要覆盖 evidence/research_task，故最后跑两次刷新）：tasks ×2 → evidence ×2 → dryrun ×2 → id_audit → verify。

## 5. 生产库就位流程 **[verified]**

1. 起点：`cp platform-dryrun.db platform-fullbatch.db`（81 资源已验证批作为基线，quick_check ok）。
2. 全量批 ETL + 幂等（§4）+ id_audit 重跑 + verify 100%（§3）。
3. `cp platform-fullbatch.db platform.db`，复跑 verify（RP_TARGET_DB=platform.db）：203 PASS、exit 0；`PRAGMA quick_check` ok；user_version=1；schema_version 1/2/3/4 齐备。
4. 最终表行数（生产库，与运行日志一致）：

| 表 | 行数 | 变化（相对全量批前） |
|---|---:|---|
| resource / resource_source | 81 / 81 | 不变 |
| knowledge | 46 | 不变 |
| review | 165 | 不变 |
| evidence | 700 | 23 → 700（+677，26 兼容库全量） |
| research_task | 42 | 行数不变，新增三大 JSON 段三列 126 段填充 |
| entity_alias | 1,273 | +677（全部 evidence） |
| tool / tool_version / skill / workflow / workflow_registry_meta | 20 / 20 / 125 / 13 / 1 | 不变（M6 域，随基线复制） |
| audit_event / tool_execution | 0 / 0 | 不变 |

5. ID 审计（id_audit.py v1.1.1，对 fullbatch 库）：唯一性/跨源碰撞/格式/alias 一致性全过，evidence 派生 ID 700 全部合规唯一，结论"现网 ID 可直接沿用，无需批量生成新 ID"，exit 0；verify §10 复核审计结果与当前库同步（alias 1,273 = 1,273）。

## 6. 源库零写入证明 **[verified]**

本批在全量 ETL 前后对 31 个源文件建立双锚点（sha256 + mtime_ns），前后对比：

| 检查项 | 结果 |
|---|---|
| catalog.db（活源 + 快照） | sha256 与 mtime 均未变 |
| knowledge.db（活源 + 快照） | sha256 与 mtime 均未变 |
| 27 个 assets.db（engagements 下全部实存库） | sha256 与 mtime 均未变 |
| 合计 | 31/31 一致，0 变化（锚点文件存于运行时临时目录，不进仓库） |

连接方式：assets.db 与 catalog/knowledge 均以 `sqlite3 URI mode=ro` 只读打开；task JSON `open()` 只读。本批未产生任何源库伴生文件变化（mtime 锚点覆盖）。

## 7. 交付物与复现命令

```text
migrations/ 新增：
├── 004_fullbatch.sql                          # 全量批权威记录：research_task 三追加列 + schema_version v4（幂等）
├── platform-fullbatch.db                      # 全量批目标库（验证 100% 后已复制为 platform.db）
├── M1_FULLBATCH_REPORT.md                     # 本报告
└── quarantine/
    ├── research_task_isolation.jsonl          # task 坏行隔离（确定性覆写；本批 0 行，空文件）
    └── evidence_isolation.jsonl               # evidence 坏行隔离（确定性覆写；本批 0 行，空文件）
migrations/ 修改：
├── etl_tasks.py          # v1.0.0 → v2.0.0（大 JSON 段三列 + 坏行隔离 + RP_TARGET_DB）
├── etl_evidence.py       # v1.0.0 → v2.0.0（26 库全量无上限 + 逐行坏行隔离 + RP_TARGET_DB）
├── etl_dryrun.py         # v1.1.0 → v1.1.1（RP_TARGET_DB，映射规则不变）
├── id_audit.py           # v1.1.0 → v1.1.1（RP_TARGET_DB，审计规则不变）
├── verify.py             # §8/§9 全量批扩展（72 项口径 → 203 项 PASS）
├── etl_tasks_last_run.json / etl_tasks_run_log.jsonl
├── etl_evidence_last_run.json / etl_evidence_run_log.jsonl
├── etl_last_run.json / etl_run_log.jsonl
├── id_audit_result.json / source_manifest.json / rtis_tasks_stats.json
└── platform.db           # 生产库（gitignore，不入库；= platform-fullbatch.db 逐字节副本）
```

复现（全部命令本批实际执行过，顺序敏感）：

```bash
cd /home/emo/code/pensting/research-platform/migrations
export RP_TARGET_DB=platform-fullbatch.db
cp platform-dryrun.db platform-fullbatch.db
python3 -I etl_tasks.py && python3 -I etl_tasks.py        # 第 2 次 no-op
python3 -I etl_evidence.py && python3 -I etl_evidence.py  # 第 2 次 no-op（700 行全量）
python3 -I etl_dryrun.py && python3 -I etl_dryrun.py      # 摘要刷新 + 第 2 次 no-op
python3 -I id_audit.py                                    # 期望 exit 0 + 结论"现网 ID 可直接沿用"
python3 -I verify.py                                      # 期望 203 PASS / 一致率 100% / exit 0
unset RP_TARGET_DB
cp platform-fullbatch.db platform.db                      # 生产库就位（既定重建流程的全量批版）
RP_TARGET_DB=platform.db python3 -I verify.py             # 生产复核，期望同结果
```

## 8. 失败清单

### 8.1 对账/幂等/源库写入失败项

无（verify 203/203 PASS，exit 0；三次幂等对比全部 no-op；31/31 源文件锚点一致）。**[verified]**

### 8.2 开发期实现缺陷（交付前已修复，留痕）

无交付期失败项。实现说明：evidence 单行映射失败由第二批的 SystemExit FATAL 改为逐行隔离（ValueError + 调用方捕获写隔离清单），是任务口径要求的机制变更而非缺陷修复。

## 9. 已知限制与未验证项

1. **evidence 映射域边界** **[verified]**（边界本身）/ **[unverified]**（其余表建模）：兼容库 28 张基线表中仅 http_evidence 与 shot 属 evidence 映射域；asset/vuln/scan_run/score_hit 等其余表的实体建模属 M2+，本批不迁移也不隔离（有行数与指纹登记，无"丢失"口径）。
2. **id_audit 审计口径为活源** **[verified]**（现状留痕）：id_audit.py 的 C 段对活源 tasks 目录审计（当前 46 JSON / 50 目录），而 research_task 目标行 42 行来自 M8 冻结快照（verify/etl_tasks 全部对快照）；C 与 D 的 intentional reuse 记 42。该口径差异为 M8 快照锁定机制的既有事实（第二批审计时活源与目标同为 35，本批起两者出现版本差），活源与快照的任务数演进属合法 drift，不影响对账红线；如需对齐可随下次快照刷新收敛。
3. **异构库 openredir-landing 仍隔离** **[verified]**（维持第二批裁定）：37 张业务表结构偏离 26 库基线，未知结构不猜测转换，登记于 quarantine/assets_isolation.jsonl；其 9 张差异表的结构理解与迁移留后续批次评估。
4. **shot 二进制本体未迁移** **[verified]**：同第二批限制 3，storage_ref + source_pk 可回溯，文件在 engagement 目录原位。
5. **大 JSON 段的"键缺失/null → NULL"语义** **[verified]**（执行一致）：42 文件三段全非空，该分支本批未实际触发，语义正确性以规则文本与 verify 复建逻辑双重表达为准。
6. **rp 侧三轴缺口等 M8 限制不在本批范围** **[verified]**（未动）：topics 轴/CJK 桥/license 轴、worker embedding 占位、M7 只读骨架、rtis.py 自测基线 4 败等 M8 第三节其余限制维持原状；本批仅关闭其第三节限制 1（evidence 抽样）与限制 2（task 大 JSON 段）。
7. **本批不构成 M1 收尾验收**：隔离重建证明、七域映射全覆盖等 M1 收尾门槛口径仍以 M1_IMPLEMENTATION_PLAN §6 为准；本报告覆盖任务书全量批范围。

---

> 口径衔接：第一批 `M1_DRYRUN_REPORT.md`、第二批 `M1_BATCH2_REPORT.md` 的既有结论不变；本批在其上完成"抽样批 → 全量生产批"升级，`platform.db` 由 81 资源抽样批升级为全量生产批（evidence 700 / research_task 42 含大 JSON 段 / alias 1,273）。
