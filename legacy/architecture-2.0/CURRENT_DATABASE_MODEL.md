# 6. CURRENT_DATABASE_MODEL

日期：2026-11-05　方法：逐库 `.schema` + 计数实查。

## 6.1 库清单与物理拓扑

```
~/.dsh/redteam/
├── knowledge.db          # 内部知识（11 表 + FTS5，WAL）
├── rtis/data/tasks/*.json # 编排状态（文件，非 DB）
└── engagements/<slug>/assets.db × 28   # 靶标事实（15+ 表 each，同构不同步）
eri/db/catalog.db        # 外部资源（15 表，WAL，gitignore）
```

## 6.2 knowledge.db 表清单

| 表 | 用途 | 键/约束 |
|---|---|---|
| poc | 知识主表（含模板索引） | UNIQUE(code)；idx: cve/component/kind+verified/category/engagement |
| poc_fts(+_data/_idx/_content/_docsize/_config) | FTS5 | title/cve/component/versions/tags/description/content |
| k_meta | 自有知识分层 | PK=knowledge_id；CHECK evidence_level IN K0-K4 |
| k_negative | 反知识 | FK knowledge_id |
| k_relation | 关系边 | UNIQUE(src,dst,type)；CHECK 8 型 |
| k_event | 审计事件 | 追加型 |
| k_proposal / k_version | 候选 / 版本 | |

无外键强制（k_meta.knowledge_id 无 FK 约束）、无 schema_version、无迁移日志表。

## 6.3 assets.db（engagement 级，28 实例）

表集（样本"王红娘"）：asset, asset_name, service, port, fingerprint, observation, http_evidence, shot, vuln, attack_step, attack_file, credential, access_session, tunnel, webshell, tag, segment, edge, stage, scan_run, score_hit, score_point, asset_fts(+5), v_asset_service, v_asset_summary。

要点：结构同源（同一生成器）但**无 schema 版本**；28 实例间无联邦查询；evidence（http_evidence/shot）与知识库（knowledge.db）物理分离且无引用。

## 6.4 catalog.db（ERI）

15 表：resources / resource_sources / resource_categories / resource_versions / resource_relationships(空，P2) / resource_reviews / sync_state / events / github_metadata / enrichment_state / d5_candidates / quality_scores / skill_mappings / ai_usage_log / api_call_log + P2 四表（resolver_queries/resource_usage/reader_cache/review_log）。
外键：sources/categories/versions/reviews→resources；github_metadata/quality_scores→resources。有 PRAGMA foreign_keys=ON。同样无 schema 版本表。

## 6.5 跨库问题清单（统一 Domain Model 的动机）

1. **三套 ID 体系**：poc.code、resources.resource_id、assets 内部自增，无全局 ID。
2. **证据三处放**：http_evidence(assets.db) / verified_note(knowledge.db) / evidence JSON(eri events)，无统一 Evidence 实体。
3. **事件三处流**：k_event / eri events / task JSON steps，无关联 ID，无法回答一次任务的全链路。
4. **28 库孤岛**：跨靶标统计（"此类站点历史上中过什么"）只能靠知识库二手沉淀，原始明细查不了。
5. **schema 演进无护栏**：三处 CREATE IF NOT EXISTS 叠加，无版本号、无迁移框架、无回滚。

## 6.6 UI 需求调研（统一工作台实体清单，源自审计）

- 实体视图：Resource（含元数据/质量/许可）、Knowledge（含 K 级/反知识）、Skill、Tool（含执行日志）、Task（状态机泳道）、Workflow、Evidence（截图/HTTP 对）、Review 队列（D5/License/映射/知识候选四合一）。
- 操作：触发 sync/enrich/scan；审核通过/拒绝；查看漏斗与成本；任务时间线回放。
- 关系图：任务↔证据↔知识↔资源↔工具的链路视图（当前唯一没有可视化表达的部分）。
- 状态总览：三库健康、API 配额、断点任务、隔离项。

## 6.7 结论

物理层"多库"现状有合理内核（靶标隔离、知识/资源分离），统一平台的正确动作是：**逻辑域模型统一 + 全局 ID + 统一审计流 + schema 版本化**，而不是把 28 个 assets.db 硬塞进一个大库。
