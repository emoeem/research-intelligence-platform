# Workflow DSL 与定义版本化（Phase 8 P8-1 设计稿）

| 项 | 值 |
| --- | --- |
| 状态 | 生效中（Phase 8 首批 P8-1，第 1/4 批） |
| 日期 | 2026-11-13 |
| 依据 | docs/RIP-2.0-PLATFORM-AND-TOOLING-PLAN.md §12:701-702 / §14:825-831，docs/phase0/PHASE-ROADMAP.md §9.3 P8-1 |
| 批次记录 | docs/releases/RELEASE_2026-11-13_PHASE8-P8-1.md |

## 一、本批冻结了什么

P8-1 交付「Workflow DSL 与定义版本化」的静态半边（可写面），执行半边留 P8-2/P8-3：

1. **definition-v1 图 DSL**（`crates/workflow-engine/src/dsl.rs`）：nodes/edges 两向量 JSON 文档，参考 LangGraph 状态机建模（START/END 边界 + seq/branch 边词形），执行引擎沿用 Rust 自研（不引入 LangGraph 或任何执行框架）。
2. **JSON Schema 门**（`schemas/workflow/definition-v1.schema.json`）：draft 2020-12，additionalProperties false + 词表 enum + 条件约束（空图仅 `legacy-method-reference` 放行）；编译期 include_str! 内嵌，运行期零路径依赖。
3. **图静态校验纯函数**（`crates/workflow-engine/src/validate.rs`）：环（三色 DFS，跨连通分量）、孤立节点（start 可达集补集）、必填参数（类型词表）、非法边端点，外加 start/end 边界、空图、重复邻接去重；一次收集全部问题，错误文本含 kind 词（`cycle` / `orphan node` / `required param` / `illegal edge endpoint`），machine 可 grep。
4. **版本化定义存储**（`crates/workflow-engine/src/store.rs` + `migrations/016_workflow_definition.sql`）：workflow_definition 表，(workflow_id, version) 复合主键，put 每版追加、历史行不可变、get 回读同版本逐字节一致。
5. **rp-workflow CLI**：validate / put / get / list / import-legacy 五子命令（run 子命令属 P8-2 执行器批次）。
6. **domain 域模型**（`crates/domain/src/workflow.rs`）：WorkflowDefinition / WorkflowRun 纯类型 + run/node 状态词表与跃迁表（词值即未来 17 号段迁移 CHECK 域）。

## 二、存量 13 行实查结论（P8-1 先实查再定导入策略，ROADMAP §9.5 风险 2）

实测命令与输出（2026-11-13，migrations/platform.db 只读 URI `file:...?mode=ro`）：

```text
user_version=1  workflow_rows=13
WF-00..WF-12 共 13 行，全部 trigger/requires/related_skills/knowledge_queries/
phases/stop_conditions/excluded_activities 九字段齐备（各 JSON 列 is not null）
workflow_registry_meta 单行：default_workflow=WF-00
```

结论（落进 store::import_legacy 的 report.conclusion，随 CLI 输出透出）：

- 003 workflow 表是 **RTIS 方法层定义**（「什么证据触发、分几阶段、用哪些技能、何时停」），**不是可执行节点图**（无 nodes/edges 结构）。
- 导入策略：只读参考导入（`compat='legacy-method-reference'`，空图 + 九字段 JSON 原文快照逐字段落 `legacy_json` 列），**不自动转换、不为兼容旧形态扭曲 DSL**；这批行永不进入执行器。
- 如确需可执行化：人工重写为 graph-v1 文档后走正常 put（版本追加语义自动为该 workflow_id 开新版本）。

实测（CLI）：

```text
import-legacy 首跑: {"imported":13,"skipped":0,"source_rows":13}
import-legacy 重跑: {"imported":0,"skipped":13,"source_rows":13}   # 幂等
list: total_workflows=14（13 legacy 参考 + 1 graph-v1 样例）
```

## 三、DSL 形状（definition-v1）

```json
{
  "schema": "workflow/definition-v1",
  "workflow_id": "wf-four-node",
  "version": 1,
  "name": "四节点样例",
  "description": "可选",
  "compat": "graph-v1",
  "input_schema": { "type": "object" },
  "graph": {
    "nodes": [
      { "id": "start", "type": "start" },
      { "id": "fetch", "type": "tool", "params": { "tool_id": "webresearch.fetch" } },
      { "id": "summarize", "type": "knowledge", "params": { "query": "..." } },
      { "id": "done", "type": "end" }
    ],
    "edges": [
      { "from": "start", "to": "fetch" },
      { "from": "fetch", "to": "summarize" },
      { "from": "summarize", "to": "done" }
    ]
  }
}
```

词表与约束（三层校验分工）：

| 层 | 承担者 | 规则 |
| --- | --- | --- |
| serde 形状 | dsl::parse_definition | deny_unknown_fields，类型/缺字段即拒 |
| Schema 门 | definition-v1.schema.json（内嵌） | 词表 enum、节点上限 256/边上限 1024、node id pattern、空图条件约束 |
| 图静态 | validate::validate_definition | 环/孤立/必填参数/非法边端点/start=end 边界 |

- 节点类型八值：start / end / tool / knowledge / agent / human / condition / parallel（计划 Phase 8 五类执行体 + 图边界两值 + P8-3 人工介入一值）；P8-2 落 tool/knowledge/condition/parallel 执行器，P8-3 落 agent 执行器与 human 人工介入节点（human 无必填参数：可选 prompt/request/key，停在 awaiting_approval，批准经 resume --input 注入；词表扩展按 P8-1 冻结注「新增变体并升迁移」落点批内补入 017 CHECK 域与本 Schema，单源不变）。
- 边类型两值：seq（缺省）/ branch（condition 与 parallel 出边词形，运行期判定走向）。
- 必填参数词表（P8-1 冻结基线，P8-2 执行器按此受理）：tool 必填 `tool_id`，knowledge 必填 `query`，agent 必填 `agent_id`，condition 必填 `expression`；parallel/start/end 无必填键。
- branch 边在静态阶段按可达计（走向运行期才知道，静态不猜）；被条件跳过的节点由 P8-2 落 `skipped` 终态（domain::WorkflowNodeRunState 词表已冻结）。
- 必须恰好一个 start、至少一个 end：LangGraph 的 START/END 语义对应物，给 P8-2/P8-3 的运行轨迹一个确定的入口与出口锚。

## 四、版本管理语义（为什么 put 是追加而不是覆盖）

- 版本号由存储侧分配（现有最大版本 + 1），文档内 version 字段不作分配依据：同文件重复 put 得 version=2（P8-1 自验判据 2 的口径），历史行无 UPDATE 路径。
- 落库文本为存储侧规范化形状（`dsl::canonical_document`，单行紧凑 JSON，键序由 serde_json::Map（BTreeMap）决定，同输入恒同字节），version 字段回填为存储行版本：同版本任意两次 get 逐字节一致，版本间 diff 恰为语义差异。这与 003 etl 的 `json.dumps(ensure_ascii=False, sort_keys=True)` 入库规范化先例同款。取舍：不保存用户原始文件字节（缩进/键序会漂移），换取「回读 = 权威形状」的单一事实源。
- 装载四道门（put 路径）：文件读入 → parse_definition（serde + Schema 门）→ validate_definition（图静态）→ 存储侧版本分配入库；拒绝即整行不入库（环图样例 put 后 list 仍空，集成测试断言）。

## 五、domain 词表（P8-1 冻结，17 号段迁移的 CHECK 域预锚）

- WorkflowRunState 六值：queued / running / paused / succeeded / failed / cancelled；跃迁表（can_transition）：queued→running|cancelled，running→paused|succeeded|failed|cancelled，paused→running|cancelled；三终态不迁出，失败重试以新 run 承载并继承链（先例 agent_engine RunControl::retry）。
- WorkflowNodeRunState 五值：pending / running / succeeded / failed / skipped。
- WorkflowCompat 两值：graph-v1 / legacy-method-reference（016 CHECK 域同词值）。
- WorkflowRun 增设 workflow_version 钉定字段：run 钉死定义版本，P8-3 版本管理按 (workflow_id, workflow_version) 回放，不漂移到最新版。
- WorkflowRun 持久化面（run 表 + 节点状态表）不在本批：属 P8-2/P8-3（迁移 17 号段待占），词表先行冻结避免届时返工。

## 六、与既有面的边界

- 003 workflow 表零改动（本迁移只有 CREATE TABLE/INDEX IF NOT EXISTS + schema_version 登记 + PRAGMA user_version = 16，零 ALTER）。
- 生产库 platform*.db 零触及：存量实查走 `mode=ro` URI，import-legacy 源库走只读池（SQLITE_OPEN_READ_ONLY 物理不可写），CLI 可写 --db 有 platform 前缀守卫（rp-agent 同款），mtimes 证据见 RELEASE。
- 不依赖 agent-engine / tool-engine / research-engine（执行器接线属 P8-2/P8-3，单向弱耦合先例 research-engine 同款）。
- web 面零改动（画布属 P8-4）。
- Cargo.lock 零新增第三方 package（jsonschema 0.56、serde、serde_json、database、domain 全为既有）。

## 七、已知限制（如实）

1. put 不保存用户原始字节（见 §四取舍）；若外部系统按原始文件字节对账，需自行先规范化。
2. input_schema 只做形状门（type: object），对运行输入实例的符合性校验属 P8-2 执行器受理面。
3. `when` 条件载荷形状 P8-1 不解释（声明式占位），条件执行器冻结口径后升级 Schema（升版 definition-v2，不回改 v1 词值）。
4. legacy 参考行的 name 沿用 003 原文（如「Generic Assessment（通用基线）」），无独立清洗。
5. 版本分配与插入两步非原子（同一 workflow 并发 put 的竞争面），主键兜底重复插入必失败；CLI 单写者场景足够，P8-2 执行器若出现多写者需改单事务取版本（store.rs 头注已声明）。
