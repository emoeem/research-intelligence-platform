# 7. UNIFIED_DOMAIN_MODEL（逻辑层统一，非单库物理合并）

日期：2026-11-05　原则：每个实体有全局 ID、来源、版本策略、状态机、权限级、软删除规则；实体可分布在不同物理库，靠 ID 与审计流关联。每节标注现状来源（已实现/部分/缺失）。

## 7.0 全局约定

- **全局 ID 格式**：`<实体前缀>-<base32 时间戳>-<随机 6>`（如 RES-01J4F3-8KQ2Z1）；资源类实体沿用既有稳定 ID（`gh:owner/repo`、`url:hash16`、`poc.code`）作为别名（alias 列），不推翻现网引用。
- **公共字段**（所有实体）：`id, created_at, created_by, updated_at, schema_version, source_ref, audit_ref`。
- **公共状态轴**：生命周期状态机 + 治理状态（ACTIVE/QUARANTINED/REJECTED/ARCHIVED）分离，双轴正交。
- **权限级**：沿用 RTIS T0（只读）/T1（可观测）/T2（高影响），实体上声明 `min_permission`。
- **删除规则**：一律软删除（ARCHIVED/REJECTED + archived_at），物理删除仅限可重建索引（FTS/向量/模板缓存）且需审计。

## 7.1 Resource（外部资源；现状：ERI resources，已实现）

- ID：`gh:owner/repo` / `url:hash16`（别名 RES-*）
- 核心字段：canonical_url, resource_type, name, description, owner/repo, status
- 生命周期：DISCOVERED→PARSED→NORMALIZED→DEDUPLICATED→CLASSIFIED→EVALUATED→INDEXED→(ADOPTED)；侧态 UNLISTED/UNAVAILABLE
- 来源：Source Adapter（Awesome-Hacking Adapter 已实现；GitHubRepoAdapter 设计中）
- 版本：content_hash + sync commit（resource_versions）
- 关联：ResourceVersion(1:N)、ResourceSource(1:N)、Review(1:1)、SkillMapping(N:M→Skill)、UsageEvent(1:N)
- 权限：读取 T0；adopted 转工具链 T2（需审批）
- 归档：上游移除→UNLISTED；仓库消失→UNAVAILABLE；均保留历史

## 7.2 ResourceVersion（现状：resource_versions，部分实现：仅同步级）

- ID：RV-…；核心：resource_id, commit_sha, content_hash, metadata_hash, etag, fetched_at, changed_files
- 状态机：无（不可变行）；冲突→新行
- 版本策略：每上游变更一行；同 commit 幂等
- 关联：→Resource；→Evidence（diff 证据，缺失待建）
- 权限：T0；删除：随 Resource 归档，不单独删

## 7.3 Knowledge（自有知识；现状：poc 自有部分 + k_meta，部分实现）

- ID：`poc.code`（别名 KNW-*）
- 核心字段：title, kind(Rule/Method/Case/Playbook/Principle), category, component, versions, severity, content, usage
- 生命周期（目标 10 态）：DISCOVERED→FETCHED→PARSED→CANDIDATE→VERIFIED→INDEXED→STALE→(CONFLICTED/QUARANTINED/REJECTED)；现状仅 3 态在用，其余为迁移目标态
- 来源：AAR 提炼（k_proposal）、人工录入、研究产出
- 版本：KnowledgeVersion 链；**VERIFIED 后 AI 只能提 proposal 不可直改**（现纪律成文化）
- 关联：反知识(1:1 可选)、Evidence(N:M)、Skill(N:M)、ResearchTask(N:M，via 案例)
- 权限：读 T0；写 CANDIDATE T1；改 VERIFIED T2+人审
- 归档：STALE 不删；REJECTED 保留理由

## 7.4 KnowledgeVersion（现状：k_version，部分）

- ID：KV-…；核心：knowledge_id, version_no, content_hash, diff_summary, changed_by, approved_by
- 状态机：draft→approved→superseded
- 策略：内容哈希变化即出版本；回滚=指向旧版本
- 权限：approved 动作 T2；归档：随实体

## 7.5 Evidence（现状：assets.db http_evidence/shot + eri events，割裂，缺失统一）

- ID：EVD-…；核心：evidence_type(http_pair/screenshot/file/log/hash), storage_ref(对象路径), content_hash, captured_at, captured_by, task_ref, permission_level
- 生命周期：RAW→VERIFIED(哈希复核)→REFERENCED→ARCHIVED
- 来源：执行器采集；Reader 快照
- 版本：不可变；更正=新 Evidence + contradicts 关系
- 关联：→ResearchTask/AgentExecution/ToolExecution/Knowledge/ResourceVersion
- 权限：T0 读；**PII 证据单独 sensitivity 标记**（现 .gitignore 策略结构化）
- 归档：按 engagement 保留策略；原始文件冷存储，索引行永久

## 7.6 Tool（现状：registry tools.json，部分）

- ID：`tool:<name>`（别名 TL-*）；核心：name, kind, version, installed, binary_ref, input_schema, output_schema, permission_level(T0/T1/T2), timeout, error_taxonomy
- 生命周期：REGISTERED→VALIDATED→ENABLED→(DISABLED/DEPRECATED)
- 版本：语义版本 + provenance（BIN-PROVENANCE.txt 现状结构化）
- 关联：Skill(N:M required_tools)、ToolExecution(1:N)
- 权限：执行按 T 级；注册/启停 T2
- 归档：DEPRECATED 保留记录

## 7.7 ToolVersion（缺失，新设）

- ID：TV-…；核心：tool_id, version, provenance(source_url, build_cmd, hash), validated_at, validation_output_ref
- 状态机：current/superseded；策略：二进制替换必须先建版本（解决 gogo/registry 失同步问题）
- 权限：写 T2

## 7.8 Skill（现状：125 个 Markdown + 单维 role，部分）

- ID：`skill:<name>`（别名 SKL-*）；核心：name, version, roles[], description, when_to_use, input_schema, output_schema, required_tools[], knowledge_refs[], body_ref(MD 路径), enabled, generator
- 生命周期：DRAFT→ACTIVE→(DEPRECATED/SPLIT→指向替代)
- 版本：front matter + body 哈希；变更走 Review
- 关联：Tool(N:M)、Knowledge(N:M)、Workflow(N:M)、AgentExecution 使用记录
- 权限：调用按关联工具的最高 T 级；编辑 T2
- 归档：DEPRECATED 不删，hunt-* 子家族用 tag 层级表达

## 7.9 Agent（角色定义；现状：五角色稿，部分）

- ID：AGT-…；核心：role, system_prompt_ref, allowed_tools[], allowed_permission_level, handoff_schema, escalation_rules
- 生命周期：DEFINED→ACTIVE→RETIRED
- 版本：角色稿哈希；变更 Review
- 权限：自身定义的 allowed_permission_level 是执行闸（现状仅在提示词，需结构化强制）
- 归档：RETIRED 保留历史执行引用

## 7.10 AgentExecution（现状：handoff 13 字段 + attack_step，割裂）

- ID：AEX-…；核心：task_id, agent_id, status(running/completed/partial/blocked/failed), started_at, ended_at, handoff(findings/evidence/limits/next), token_usage, tool_calls[]
- 生命周期：RUNNING→(COMPLETED/PARTIAL/BLOCKED/FAILED)
- 版本：不可变，重跑新行
- 关联：→ResearchTask/Agent/ToolExecution(1:N)/Evidence(1:N)
- 权限：读 T0；写限自身
- 归档：随 Task

## 7.11 Workflow（现状：workflows.yaml，已实现）

- ID：WF-*；核心：trigger(dimensions/signals/business), requires, related_skills, knowledge_queries, phases, stop_conditions, excluded_activities
- 生命周期：DRAFT→ACTIVE→DEPRECATED
- 版本：YAML 版本号 + last_reviewed（现状已有）
- 关联：Skill(N:M)、Knowledge 查询集、WorkflowExecution(1:N)
- 权限：编辑 T2（L7 只提案不改的纪律保留）
- 归档：DEPRECATED 保留

## 7.12 WorkflowExecution（现状：task.planning + attack_step，部分）

- ID：WFX-…；核心：workflow_id, task_id, selected_at, router_snapshot(输入画像+分数+excluded 理由), phase_progress[], stopped_reason
- 生命周期：PLANNED→RUNNING→(COMPLETED/STOPPED/ABORTED)
- 关联：→Workflow/ResearchTask/AgentExecution(1:N)
- 权限：T0 读
- 归档：随 Task

## 7.13 ResearchTask（现状：rtis task JSON + engagement，已实现割裂）

- ID：RTIS-…（现网 task_id）；核心：objective, target, authorization_ref, state(S0-S9), profile_snapshot, gate_results[], external_resource_review, deliverables[]
- 生命周期：现状状态机原样收编（含跳段审计）
- 版本：状态转移历史（status_history 现状已有）
- 关联：Evidence/AgentExecution/WorkflowExecution/ResourceUsage
- 权限：创建 T1；执行按授权记录
- 归档：engagement 结束→ARCHIVED，assets 部分可冷存

## 7.14 Review（现状：k_proposal + resource_reviews + review_log，三处割裂）

- ID：REV-…；核心：target_type(resource/knowledge/mapping/d5/license/tool/skill), target_id, decision, decision_reason, reviewer, evidence_refs[], proposed_change
- 生命周期：PENDING→(APPROVED/REJECTED/NEEDS_MORE_EVIDENCE)→APPLIED/ DISMISSED
- 来源：AI 提议（标 generated_by）或人工
- 版本：不可变；重审新行
- 关联：→任意实体（多态引用 target_type+target_id）
- 权限：AI 提交 T1；批准 T2 人审
- 归档：永久（治理证据）

## 7.15 AuditEvent（现状：k_event 633 + eri events + task events，割裂）

- ID：AUD-…；核心：ts, actor(agent_id/user/system), action, entity_ref(target_type+target_id), task_id, query_id, correlation_id, detail(json), permission_level_at
- 生命周期：只追加，不可改
- 策略：三套现存事件流归并映射（k_event.action → 标准动词表），correlation_id 打通"任务-查询-工具-证据"链
- 关联：全实体（多态）
- 权限：追加 T0；读取含 sensitivity 过滤
- 归档：滚动冷存，索引行永久

## 7.16 实体关系总览

```
ResearchTask 1─N AgentExecution N─1 Agent
      │              │ 1─N ToolExecution N─1 ToolVersion 1─1 Tool
      │              └─N Evidence
      ├─1 WorkflowExecution N─1 Workflow N─M Skill
      ├─N ResourceUsage ──→ Resource 1─N ResourceVersion / ResourceSource
      ├─N Evidence
      └─N AuditEvent（全实体多态指向）
Knowledge N─M Skill；Knowledge 1─N KnowledgeVersion；Knowledge 1─0..1 NegativeKnowledge*
Review 多态 → 全实体；Evidence sensitivity 分级。
* NegativeKnowledge 本阶段作为 Knowledge 的伴随实体保留（k_negative 现状），不单列独立章节。
```

## 7.17 明确不做

1. 不把 15 实体塞进一张物理大表（要求原文如此）。
2. 不给 28 个 assets.db 强行合并：Evidence/AuditEvent 用全局 ID 桥接，物理上 engagement 库保留为"证据冷存分区"。
3. 不推翻现网稳定 ID（poc.code / gh:… / task_id 全部保留为别名）。
