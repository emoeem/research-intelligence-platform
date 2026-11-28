# RIP 1.0 数据治理

> 状态：治理层现状和边界。日期：2026-11-28。

## 一、权威数据

PostgreSQL 是知识实体、关系、来源、证据、审核和审计治理的正式存储方案。SQLite 是兼容运行数据和派生运行数据。

源数据库和 RTIS production knowledge.db 不应被日常 Agent 或 UI 写入。变更先进入开发副本、迁移副本或审核批次。

## 二、资产生命周期

资源、Knowledge、POC、Nuclei 和工具必须区分：

- lifecycle 状态
- governance 状态
- 来源状态
- evidence 状态
- review 状态
- runtime 验证状态

来源存在、APPROVED 或 VERIFIED 历史位不能跨生命轴自动提升为当前验证结论。

## 三、审核闭环

候选数据进入审核批次；审核决策保存 reviewer、reason 和 reviewed_at；APPROVED 或 REJECTED 不得只凭 AI 建议写入。

POC 证据要求来源、时间、原始 Artifact、SHA-256、采集方式和有效状态。当前 curated POC 仍保留未评估项，不得批量变为已验证。

## 四、检索与去重

搜索索引是派生投影，使用 typed identity 和 canonical key。重复标题、共享主页 URL 或共享 corpus URL 不构成重复依据。

M4 当前只保守审计重复候选，没有自动 merge 或 delete；用户采样接受了 140 债对，但零人工 grade 和 null relevance metrics 不得转换为 AI 推荐分数。

## 五、持续更新

数据源、索引、来源变更、失效引用和恢复状态需要定期核对。每次摄取保留来源快照、commit/path 或版本、sha256 和审计记录。
