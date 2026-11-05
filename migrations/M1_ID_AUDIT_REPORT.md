# M1 ID 冲突审计报告（计划 §5.4 硬门槛）

| 项 | 值 |
|---|---|
| 日期 | 2026-11-06 |
| 批次 | M1 第二批，任务 1（最优先） |
| 执行脚本 | `migrations/id_audit.py` v1.0.0（stdlib-only，三源与目标库全量只读） |
| 机器可读结果 | `migrations/id_audit_result.json`（verify §10 以此为对账输入） |
| 证据标签 | **[verified]** = 本批实际执行并有输出记录；**[unverified]** = 采信上游、未复放 |

## 0. 结论（硬门槛判定）

**现网 ID 可直接沿用，无需批量生成新 ID。** **[verified]**

依据：四类检查全绿：唯一性违例 0、跨源碰撞 0、格式违规 0、alias 与实体主键双向一致 0 缺失。计划 §5.4「不在 M1 未完成 ID 冲突审计前批量生成新 ID」的前提条件已满足，第一批"主键沿用现网稳定 ID"的策略（`M1_DRYRUN_REPORT.md` §2.2）经全量审计证实可行，M2 起无需引入新 ID 生成器来替换现网 ID。

## 1. 审计范围与口径

三源现有稳定 ID 全量审计（非抽样）+ 隔离目标库派生 ID：

| ID 集 | 源 | 行数 | 说明 |
|---|---|---:|---|
| A. catalog resource_id | `eri/db/catalog.db` resources | 81 | gh:/url: 两型 |
| B. knowledge poc.code | `~/.dsh/redteam/knowledge.db` poc | 14,717 | 全量口径，含 14,111 条 nuclei 模板索引 code |
| C. RTIS task id | `~/.dsh/redteam/rtis/data/tasks/` | 35 JSON + 36 目录 | 格式 rtis-YYYYMMDD-HHMMSS-ms |
| D1. knowledge neg:* | platform-dryrun.db | 17 | 第一批确定性派生（'neg:'+父code） |
| D2. review eri-catalog:review_log:* | platform-dryrun.db | 165 | 第一批确定性派生 |
| D3. evidence EVD-* | platform-dryrun.db | 23 | 第二批确定性派生（EVD-库slug-源表-行id） |
| D4. research_task task_id | platform-dryrun.db | 35 | 第二批沿用 C 的现网 ID 作主键 |

## 2. 检查结果 **[verified]**

### 2.1 全局唯一性：全部通过（违例 0）

| ID 集 | 行数 | 去重后 | 重复 |
|---|---:|---:|---:|
| A resource_id | 81 | 81 | 0 |
| B poc.code | 14,717 | 14,717 | 0 |
| C task id | 35 | 35 | 0 |
| D1 neg:* | 17 | 17 | 0 |
| D2 review:* | 165 | 165 | 0 |
| D3 EVD:* | 23 | 23 | 0 |
| D4 research_task | 35 | 35 | 0 |

B 的全量口径说明：14,717 行含 nuclei 模板索引（source='nuclei'，14,111 条），其 code 亦全量唯一；源 poc 表存在 `UNIQUE(code)` 约束，审计以数据实测复核约束实际成立。

### 2.2 跨源碰撞：0

{A, B, C, D1, D2, D3} 六个集合两两精确交集均为空。

单列说明：C（rtis_task_id）与 D4（research_task.task_id）交集 = 35，为**有意沿用**（同一现网 ID 直接作为平台主键，002_tasks.sql 的映射设计），登记为 intentional reuse，不计冲突。同理 A/B 与各自别名 namespace 的登记也是同源沿用。

### 2.3 格式合规：违规 0

| ID 集 | 规则 | 违规 |
|---|---|---:|
| A resource_id | `gh:owner/repo` 或 `url:` + 16 位小写 hex | 0（gh 79、url 2） |
| B poc.code | 非空、无空白/控制字符、长度≤256 | 0（ascii-only 14,592、含 CJK/宽字符 125；CJK 为自有中文知识的合法 slug 组成） |
| C task id | `rtis-\d{8}-\d{6}-\d{2,}` | 0（JSON 35 个、目录名 36 个均合规） |
| D1/D2/D3/D4 | 前缀派生规则各自 regex | 0 |

### 2.4 alias 与实体主键一致性：双向 0 缺失 **[verified]**

对 platform-dryrun.db 全部 6 个非空实体类型：

| 实体类型 | 主键行 | 别名行 | 别名指向不存在的主键 | 有主键无别名 |
|---|---:|---:|---:|---:|
| resource | 81 | 81 | 0 | 0 |
| resource_source | 81 | 81 | 0 | 0 |
| knowledge | 46 | 46 | 0 | 0 |
| review | 165 | 165 | 0 | 0 |
| research_task | 35 | 35 | 0 | 0 |
| evidence | 23 | 23 | 0 | 0 |

（knowledge 46 = own 28 + 父回填 1 + negative 17，见 M1_BATCH2_REPORT.md；evidence/research_task 为第二批新增迁移行，同样 100% 有 K-09 四要素别名登记。）

## 3. 冲突清单

空。无任何同 ID 指向不同实体、跨源碰撞或格式违规条目。

## 4. 备查发现（不构成冲突，登记备查）**[verified]**

1. **任务目录与 JSON 配对差异**：36 个任务目录名 vs 35 个 task JSON id；6 个目录无同名 JSON（rtis-20261102-220844-7484 等，为无状态文件的工件目录），5 个 JSON 无同名目录。属工件布局事实，不影响 ID 唯一性与格式合规；task 实体迁移以 JSON 的 task.id 为准（35 个）。
2. **poc.code 含 CJK 字符（125 条）**：中文 slug 是 knowledge.db 的既有设计（列注释"稳定标识（slug）"），不视为格式违规；若 M2+ 平台 ID 域决定收窄为 ASCII，须经差异决策并走 entity_alias 别名桥，不得原地改写现网 ID。
3. **url: 型仅 2 条**（79 gh: + 2 url:）：样本量小但格式规则明确（url:16hex），审计仍按全规则校验通过。

## 5. 复现

```bash
cd /home/emo/code/pensting/research-platform/migrations
python3 -I id_audit.py        # 期望 exit 0，结论"现网 ID 可直接沿用，无需批量生成新 ID"
```
