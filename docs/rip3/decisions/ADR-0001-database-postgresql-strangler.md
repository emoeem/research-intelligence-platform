# ADR-0001：情报核数据库采纳 PostgreSQL（绞杀者路线）

- 状态：已裁决（用户 2026-11-14）；Phase 1 起生效
- 决策者：用户；取证与比较：RIP3 Phase 0（00-ARCHITECTURE-AUDIT.md）
- 关联：R2 §6.3/§10.1（本 ADR 按用户裁决修订其「第一阶段继续 SQLite」口径，
  修订记录见 RIP3-ARCHITECTURE-UPGRADE-PLAN.md 尾部）；12-STORAGE-AND-MIGRATION.md
  （实施细节）；03-DOMAIN-MODEL.md（实体落库面）

## 一、背景

RIP3 情报核（文档/分块/向量/知识卡片/关系/治理/报告）是全新绿地面。目标规模
1k 到 100k 文档与百万级 Chunk（R2 §15.4）。既有栈为 SQLite（rusqlite，19 迁移，
单写者，FTS5，CJK 靠 2-gram 手搓桥表）；向量面零接线（00-审计 1.1 实证）。

## 二、问题

情报核需要：关系 + CJK 全文 + 向量（1M×512）+ 结构化过滤 + 行级权限 + 多写者并发。
SQLite 单库无法同时满足（FTS5 CJK 弱、无原生向量、无行级安全、单写者）；
引入独立向量库（LanceDB/Qdrant）则产生双存储一致性负担。

## 三、候选比较

| 候选 | 能力 | 成本/风险 |
|---|---|---|
| SQLite + sqlite-vec | 零新组件 | 硬伤：vec0 纯暴力 KNN 无 ANN，1M×512 每查询全扫秒级；CJK 分词仍靠手搓 |
| PostgreSQL 17 + pgvector 0.8 + pg_jieba + pg_search | 一个引擎同时满足全部五项：HNSW/量化向量、真 BM25、CJK 分词、JSONB 过滤、RLS；MVCC 多写者 | 新增容器服务运维；驱动/迁移面重建（仅绿地面代码） |
| SQLite（运营核）+ LanceDB（向量） | 保留 SQLite 习惯 | 双存储双写一致性负担；FTS CJK 缺口仍在 |
| 全量迁移到 PG（含运营核） | 终态最干净 | 大爆炸式跨 16 crate 重写，先于任何 RIP3 价值交付，风险最高 |

## 四、最终选择（用户裁决）

**绞杀者路线**：情报核 = PostgreSQL 17 + pgvector 0.8 + pg_jieba + pg_search，
Phase 1 起新增式落地（refinery 迁移链 PG-0001+），新增域永不落 SQLite；
运营核暂留 SQLite（020+ 编号），Phase 8 基准后裁决是否移植。
跨库以 trace 六 ID 关联（无跨库 FK），/api/traces 双库归并。

选择原因：情报核代码是绿地，选 PG 的实现成本与选 SQLite 相同（都要新写），
但免除后端规模墙与向量库一致性负担；运营核留在零运维 SQLite 避免大爆炸。
「新增域永不落 SQLite」规则掐死后期漂移（用户：现在决定好，避免后面出问题）。

## 五、成本

PG 容器运维（digest 钉死镜像 + pg_jieba/pg_search 扩展构建入镜像）；驱动面新增
（postgres 0.19 + r2d2_postgres，同步口径）；备份双轨（pg_dump + rp-admin）；
CI 起 postgres service 容器。

## 六、风险与缓解

1. 双库过渡期一致性：ID 引用纪律 + 对账 selftest（六 ID 双库对账进每阶段门禁）。
2. pg_search AGPL：按用户裁决能力优先（许可仅记录，2026-11-14）；组件在服务侧使用。
3. pgvector 换后端自由：VectorBackend trait 保留（LanceDB/Qdrant 可换，基准不达标启用）。
4. 运营核 SQLite 的 WAL 缺席：P1-1 批补齐 + PRAGMA 自检（12-STORAGE 第五节）。

## 七、放弃的方案

sqlite-vec（规模墙）；全量大爆炸迁移（风险前置）；LanceDB 主选（一致性负担，
降为可换候选）；维持纯 SQLite（不满足目标规模五项能力）。
