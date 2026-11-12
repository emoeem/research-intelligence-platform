# Agent Memory 013 迁移评审记录（P7-3，防重复建模专项）

| 项     | 值 |
| ---    | --- |
| 状态   | 生效中 |
| 日期   | 2026-11-13 |
| 对象   | migrations/013_agent_memory.sql 三表 + crates/domain/src/agent.rs Memory 域模型 |
| 依据   | PHASE-ROADMAP.md §8.5 风险 4（防与 Evidence 实体重复建模）、§8.3 P7-4 判据、SESSION-HANDOFF.md §十 Context Memory 雏形追加项 |
| 结论   | 三表与既有 evidence 表（001_init.sql:250）无实体字段重复建模，Memory 条目只承载指针、登记时刻快照与摘要 |

## 一、评审问题与结论

ROADMAP §8.5 风险 4 原文：Memory 设计缺既有实现参照，P7-4 的数据面设计要在迁移评审时单独过，防止与 Evidence 实体（计划 §12:704）重复建模。本记录逐字段核对 013 三表与 evidence 表（001）的列集，结论如下，核对过程见第二节。

1. **无重复建模。** agent_evidence_memory 不设 evidence_type、captured_at、captured_by、permission_level、sensitivity、governance、source_system、source_pk 等 evidence 实体字段；原始证据的唯一登记面仍是 001 evidence 表。
2. **两个同名列（content_hash、storage_ref）是登记时刻快照，不是实体重建模。** 用途是压缩/驱逐后做三方哈希对账（快照 = evidence.content_hash = artifact.sha256，落盘产物逐字节复算），先例是 008 context_manifest_item.content_hash（装配清单的内容 sha256 快照列）。写入面（EvidenceMemoryStore::record）强制快照等于既有表权威值，不一致整行拒绝，快照因此永远可由权威面复算，不构成第二份事实。
3. **lifecycle 同名不同义。** agent_evidence_memory.lifecycle 是 Memory 生命周期（ACTIVE/COMPRESSED，无 EVICTED 词值），与 evidence.lifecycle（RAW/VERIFIED/REFERENCED/ARCHIVED，M1 双轴状态机的处理阶段链）正交；不改 evidence 行的任何治理列。
4. **task_id 是查询透传指针。** agent_task_memory/agent_research_memory 的 task_id 与 agent_run.task_id（006）同口径，指向研究任务，不承载 evidence.task_ref 语义之外的内容。
5. **与聊天/会话存储分离。** Research Memory 落 agent_research_memory 独立表独立生命周期，不复用 context_manifest/context_manifest_item/context_ref_log（008 会话面）；这些表只作回链目标。

## 二、逐字段核对表

evidence 表（001_init.sql，实测列）对照 agent_evidence_memory（013，实测列）：

| evidence（001）列 | agent_evidence_memory（013）对应 | 判定 |
| --- | --- | --- |
| evidence_id | evidence_id（指针，FK REFERENCES evidence） | 指针，非重建模 |
| evidence_type | 不设列 | 不重复 |
| storage_ref | storage_ref（登记时刻快照，写入面强制等于权威值） | 快照钉面，见第一节第 2 条 |
| content_hash | content_hash（登记时刻快照，同上） | 快照钉面，见第一节第 2 条 |
| captured_at / captured_by | 不设列 | 不重复 |
| task_ref | 不设列（run_id 指向 agent_run，归属语义在 run 面不在证据面） | 不重复 |
| permission_level / sensitivity | 不设列 | 不重复 |
| lifecycle | lifecycle（Memory 生命周期，词值域不同） | 正交，见第一节第 3 条 |
| governance | 不设列 | 不重复 |
| source_system / source_pk | 不设列 | 不重复 |
| created_at / updated_at / schema_version | 同名行管列（表级通用约定，与全库口径一致） | 行管通用列，非实体语义 |

agent_evidence_memory 独有列：memory_id（台账主键）、run_id（FK agent_run）、artifact_id（FK artifact，同哈希对账面）、UNIQUE(run_id, evidence_id)（同 run 防重）。

agent_task_memory / agent_research_memory 不含任何 evidence/artifact 实体列：research 记忆的回链以 refs_json 数组（manifest item/artifact/evidence 三型）承载，回链行存在性由写入面当场校验（不可解析即整行拒绝），不复制回链目标的内容。

## 三、执行面证据（本批实测）

- 域模型层：domain::agent::EvidenceMemoryEntry::try_new 拒绝 EVICTED/ARCHIVED 词值（单测 evidence_memory_entry_rejects_eviction_and_bad_hash）；生命周期跃迁表 MemoryLifecycle::EVIDENCE_TRANSITIONS 仅 ACTIVE 到 COMPRESSED 一行（单测 memory_lifecycle_tables_refuse_evidence_and_research_eviction）。
- 存储层：EvidenceMemoryStore::record 强制快照一致（单测 record_snapshots_must_match_authoritative_rows，漂移即拒绝且零行残留）；台账指针在场时权威证据行不可被删（FK 兜底，单测 resolve_detects_authoritative_drift_and_missing_rows 断言 DELETE 报约束失败）。
- 契约层：压缩/驱逐后全部 Evidence 与 Artifact 引用解析成功且 content_hash 校验通过（集成测试 crates/agent-engine/tests/memory_contract_p7_3.rs，落盘产物逐字节复算 Some(true)）。

## 四、遗留（如实记录）

- refs_json 的回链存在性校验在写入面与解析面执行，DB 层不对 JSON 数组做外键（SQLite 不支持 JSON 列外键），断链风险由 resolve_refs 全量重放兜底（单测 resolve_refs_reports_broken_backlinks_after_target_removal 验证漂移如实报告）。
- agent_research_memory 无按 task_id 之外的二级索引（当前查询面按 run_id/list_by_run 与 memory_id/get 即够，task 索引已建 ix_agent_research_memory_task）。
