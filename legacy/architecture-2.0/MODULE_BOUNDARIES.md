# 8. MODULE_BOUNDARIES

日期：2026-11-05　原则：模块间只经稳定接口（Schema 化的 API + 事件），禁止直接读他模块库表；ERI 从中心降位为普通域模块。

## 8.1 模块划分（对应目标 crates/ 分层，逻辑先行）

| 模块 | 职责 | 拥有数据 | 对外接口 |
|---|---|---|---|
| domain | 15 实体定义、ID、状态机、权限级枚举 | 无（纯类型/Schema） | 类型库（被所有模块依赖，自身零依赖） |
| database | 连接、迁移、事务、备份恢复 | migrations/ | `with_tx(fn)`、`migrate()`、`backup()/restore()` |
| resource-engine | 接入/规范化/去重/同步/元数据/质量/D5 | resources + versions + sources | `Resolve(query)→Candidates`、`Enrich(job)`、事件 `resource.changed` |
| knowledge-engine | 知识生命周期/反知识/版本/提议 | knowledge + versions + negatives | `Search(q)→Dossier`、`Propose(change)`、事件 `knowledge.changed` |
| search-engine | FTS/向量/混合检索、分词、排序 | 索引（全部可重建） | `Index(doc)`、`Query(q, filters)→Hits`；索引损坏可由 domain 库全量重建 |
| quality-engine | 评分/隔离/License/D5 审核 | scores + reviews | `Score(id)`、`ReviewQueue()` |
| tool-engine | 工具注册/Schema/权限闸/执行日志 | tools + versions + exec_log | `Execute(tool, input, ctx)→Result`（权限检查在此，不在调用方） |
| workflow-engine | 工作流注册/触发路由/阶段推进 | workflows + executions | `Route(profile)→Plan`、`Advance(exec, step)` |
| agent-engine | 角色定义/执行/交接/预算 | agents + executions | `Spawn(role, task, budget)→Exec`、`Handoff(exec)→Record` |
| audit-engine | 统一事件流、关联 ID、冷存归档 | events | `Emit(event)`（各模块唯一出口）、`Trace(correlation_id)` |
| integrations/* | GitHub/MCP/eri/rtis 适配 | 无（无状态适配器） | 把外部世界翻译成 domain 事件/命令 |

## 8.2 依赖规则

1. domain 被所有人依赖，不依赖任何人。
2. database 只被各 engine 依赖；engine 之间不互读库，只走接口或事件。
3. search-engine 可重建：删掉整个索引库，从 domain 库重灌即恢复（现网 nuclei 模板索引同策略）。
4. audit-engine 是旁路：任何模块故障不得因审计写入失败而失败（本地缓冲+重试）。
5. integrations 不得反向调用 engine 内部函数，只消费公开接口。

## 8.3 现有系统 → 模块映射

| 现状 | 去向 | 边界修正 |
|---|---|---|
| eri.py/eri_p1/eri_p2 | resource-engine + quality-engine + 部分 search | 三个单文件的 import 链拆开；DB_PATH 全局变量改注入 |
| rt.py（知识读写） | knowledge-engine | 唯一写者纪律升级为 engine 内部写队列 |
| rtis.py task/profile/gate | workflow-engine + audit-engine | task JSON 迁 DB；gate 的 ERI subprocess 钩子改进程内接口 |
| rtis.py route + workflows.yaml | workflow-engine | YAML 注册表保留为声明格式 |
| rtis_mcp.mjs | integrations/mcp | 工具面 = domain 接口的 MCP 投影 |
| assets.db ×28 | evidence 冷存分区 + audit 桥 | 不合并；Evidence 全局 ID 回填 |
| skills/*.md | agent-engine（Skill 实体） | Markdown 降级为渲染产物，front matter 结构化 |

## 8.4 稳定接口首批清单（M1 交付物）

```
Resolve(ctx: TaskContext, limit) -> CandidateSet          # resource-engine（P2 已有 JSON 形状）
KnowledgeSearch(q: str, filters) -> Dossier               # knowledge-engine（gate 已有形状）
Route(profile: SiteProfile) -> ResearchPlan               # workflow-engine（rtis 已有形状）
Execute(tool_id, input, permission_ctx) -> ToolResult     # tool-engine（新增）
Emit(AuditEvent) / Trace(correlation_id)                  # audit-engine（新增）
Propose(Review) / Decide(review_id, decision)             # quality-engine（新增）
```

兼容承诺：前三个接口的 JSON 形状与现有 rtis/eri 输出保持字段级兼容，保证迁移期双轨。
