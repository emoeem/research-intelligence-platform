# RELEASE：Phase 8 P8-1 Workflow DSL 与定义版本化（2026-11-13）

> 本批把 Phase 8 的静态半边落成可验收的代码面：definition-v1 图 DSL
> （nodes/edges JSON + draft 2020-12 Schema 门）+ 图静态校验纯函数（环 /
> 孤立节点 / 必填参数 / 非法边端点，一次收集全部问题）+ workflow_definition
> 版本化存储（016 迁移，(workflow_id, version) 复合主键，put 每版追加历史
> 不可变）+ rp-workflow CLI（validate/put/get/list/import-legacy）+
> domain::workflow 域模型与 run/node 状态词表跃迁表。用户可感知的变化：
> 一条命令即可校验/登记/回读带版本历史的 workflow 定义；存量 13 行实查
> 落地（003 表是 RTIS 方法层定义非可执行图，以 legacy-method-reference
> 只读参考导入，DSL 不扭曲）。执行半边（Tool/Knowledge/Condition/Parallel
> 节点执行器、Agent 节点、暂停恢复重试）留 P8-2/P8-3，画布留 P8-4。

| 项 | 值 |
| --- | --- |
| 状态 | 生效中（Phase 8 首批 P8-1，第 1/4 批，ROADMAP §9.3 P8-1） |
| 日期 | 2026-11-13 |
| 数据库迁移 | 016_workflow_definition.sql（新表 workflow_definition + workflow_id 索引 + schema_version 登记；在临时库与 migrations/dev-p8.db 开发库实测，migrations/platform*.db 生产库零触及：存量实查走 mode=ro URI，import-legacy 源库走只读池 SQLITE_OPEN_READ_ONLY，可写 --db 有 platform 前缀守卫） |
| 自动化测试 | `cargo test --offline --workspace`：536 passed 0 failed（P7-7 基线 510 + 本批新增 26：domain::workflow 单测 7、workflow-engine lib 单测 16、workflow-engine 集成测试 3） |
| 静态检查 | `cargo clippy --offline --workspace --all-targets -- -D warnings` 通过（0 error；过程告警 too_many_arguments 加注 allow、identity map/redundant closure/doc 缩进逐条修复后归零） |
| 批次自验 | 五条 ask 判据逐条实测（见「三、五条自验逐条实测」） |
| 相关 | docs/phase8/WORKFLOW-DSL.md（设计稿）、docs/phase0/PHASE-ROADMAP.md §9.3/§9.4、schemas/workflow/definition-v1.schema.json、docs/DOC-STANDARD.md |

## 一、改动清单

| 文件 | 内容 |
| --- | --- |
| Cargo.toml / Cargo.lock | workspace members 增 crates/workflow-engine；lock 仅新增成员自身条目（依赖 database/domain/jsonschema/serde/serde_json 全为既有 package，零第三方） |
| crates/domain/src/workflow.rs（新） | WorkflowDefinition/WorkflowRun/WorkflowNode/WorkflowEdge 域类型 + WorkflowId 新类型 + 节点七值/边两值/compat 两值/run 六态/节点运行五态词表 + run 跃迁表 + 7 单测（纯 std 零依赖，error.rs 增 10 变体） |
| crates/workflow-engine/（新 crate） | src/lib.rs 门面、src/dsl.rs（definition-v1 serde 文档模型 + Schema include_str! 内嵌门 + canonical_document 规范形状 + domain 互转）、src/validate.rs（静态校验纯函数 + kind 词表 + 10 单测）、src/store.rs（put 追加版本/get/list/import-legacy）、src/error.rs、src/bin/rp-workflow.rs（五子命令 CLI）、tests/migration_replay.rs（3 集成测试） |
| migrations/016_workflow_definition.sql（新） | workflow_definition 表（复合主键 + compat CHECK 域两值 + legacy_json 快照列）、workflow_id 索引、schema_version 登记 version=16、PRAGMA user_version = 16；零 ALTER，不动 003 workflow 表 |
| schemas/workflow/definition-v1.schema.json（新） | draft 2020-12，additionalProperties false，词表 enum，条件约束（空图仅 legacy-method-reference 放行），节点 256/边 1024 上限 |
| docs/phase8/samples/four-node.json、cycle.json（新） | CLI 验收样例（四节点 happy path / 环图反例） |
| docs/phase8/WORKFLOW-DSL.md（新） | 设计稿（实查结论、DSL 形状、校验分工、版本管理语义、domain 词表、边界、已知限制） |
| docs/README.md | Phase 8 行登记（本批次行） |
| 链尾断言顺延（7 个既有测试文件，编号冻结规则常规演进） | crates/agent-engine/tests/migration_dev_copy_p7_3.rs（13..15 改 13..016 + user_version 16）、crates/agent-engine/tests/migration_idempotent.rs（001..016 + uv 16）、crates/database/tests/migrations_agent_data_plane.rs / migrations_context_broker.rs / migrations_runtime_node.rs / migrations_tool_contract.rs（discover 全链 001..016、链尾 016_workflow_definition、uv 16）、crates/plugin-runtime/tests/lifecycle_full_path.rs（009 自钉回 9 后重放 010..016 七枚） |

## 二、设计要点（为什么这样做）

1. **先实查再定导入策略**（ROADMAP §9.5 风险 2）：016 动工前对 003 workflow 表只读实查（platform.db user_version=1，workflow 13 行 WF-00..WF-12，九 JSON 字段全齐，registry_meta 单行 default_workflow=WF-00），结论：方法层定义非可执行图。导入策略 = 只读参考（compat='legacy-method-reference'，空图 + 原文快照落 legacy_json 列），不自动转换不扭曲 DSL；可执行化必须人工重写为 graph-v1 后走 put。
2. **参考 LangGraph 建模但执行引擎自研**：借 START/END 边界与分支边词形（恰好一个 start、至少一个 end、seq/branch 两词形），不引执行框架；Phase 8 节点五类直接进类型词表，P8-2/P8-3 执行器按词表落实现，DSL 不返工。
3. **三层校验分工**（serde 形状 → JSON Schema 门 → 图静态纯函数）：形状漂移、词表漂移、拓扑缺陷分别拦截；一次收集全部问题（画布编辑一次改完），错误文本含 kind 词（cycle/orphan node/required param/illegal edge endpoint）machine 可 grep（自验判据 3 直接依赖）。
4. **版本管理 = 追加不可变**：存储侧分配版本（max+1），落库文本为 canonical 形状（version 字段回填存储行版本，同输入恒同字节），同版本任意两次 get 逐字节一致；历史行无 UPDATE 路径。003 etl json.dumps(sort_keys=True) 入库规范化先例同款。
5. **复用优先**：持久化复用 database Pool（rusqlite bundled 随链），Schema 校验复用 vendor 既有 jsonschema 0.56（research-engine/tool-registry 同版本钉面），域校验复用 domain try_new 惯例；零新增第三方 package（Cargo.lock diff 仅 workflow-engine 自身条目）。
6. **生产库零触及 mechanically 保证**：--src 只读池（SQLITE_OPEN_READ_ONLY 物理不可写）、--db platform 前缀守卫、实查 mode=ro URI；mtimes（11-06/11-07，早于本会话）与只读机制双重证据。

## 三、五条自验逐条实测（ask 判据）

1. **cargo test --offline -p workflow-engine -p domain 全过，四类用例齐**：`cargo test --offline -p workflow-engine -p domain` 实测 domain 53 passed 0 failed（含 workflow 模块 7）、workflow-engine lib 16 passed 0 failed（dsl 5 + validate 10 + 1 doc 冒烟）、集成 3 passed 0 failed；环（cycle_detection_reports_closed_path_with_cycle_word、self_loop_is_reported_as_cycle）/ 孤立（orphan_nodes_unreachable_from_start_are_reported）/ 必填参数（missing_required_params_per_node_type_wordtable，三类型三键）/ 非法边端点（illegal_edge_endpoints_reference_missing_nodes）各至少 1 用例全绿。全仓门禁复测 `cargo test --offline --workspace`：536 passed 0 failed（P7-7 基线 510，零回归）。
2. **put version=1 / 重复 put version=2 / get --version 1 逐字节一致**：`cargo run --offline -p workflow-engine --bin rp-workflow -- put docs/phase8/samples/four-node.json` 首跑输出 `{"result":"PUT","version":1,"workflow_id":"wf-four-node","nodes":4,"edges":3}`（迁移自动应用 16 枚），重跑输出 `version=2`；`get wf-four-node --version 1` 两次输出经 `cmp` 逐字节一致（BYTE-IDENTICAL）。
3. **validate 环图 exit 非 0 且含 cycle**：`validate docs/phase8/samples/cycle.json` 实测 exit=1，输出 `{"kind":"cycle","message":"cycle detected: a -> b -> a","node":"a"}` 与 `{"result":"REJECTED","issues":1}`；对照组 `validate four-node.json` exit=0 PASSED。
4. **016 迁移临时库重放两次幂等**：集成测试 migration_016_replay_twice_is_idempotent 实测（temp 文件库）：第一次 applied 含 16，第二次 applied 为空、user_version 两次均为 16、表清单两次一致；raw file replay 口径另测 016 SQL 原文直跑两次通过（016 全 IF NOT EXISTS / INSERT OR IGNORE，零 ALTER）。
5. **migrations/ 只新增 016，生产库零改动**：`git status --short migrations/` 实测仅 `?? migrations/016_workflow_definition.sql`；`ls -l --time-style=full-iso migrations/platform*.db` mtimes 为 2026-11-06/07（早于本会话，只读访问不动 mtime），git status 对 platform*.db（gitignore）无任何条目。

补充实测（超出 ask 判据的验收面）：

- `list`：total_workflows=14（13 legacy 参考 + 1 graph-v1，每行 latest_version/version_count/compat/nodes/edges）。
- `import-legacy --src migrations/platform.db`：首跑 `{"imported":13,"skipped":0,"source_rows":13,"conclusion":"003 workflow table is RTIS method-layer definitions ..., DSL not distorted"}`，重跑幂等 `{"imported":0,"skipped":13}`；`get WF-00` 回读 legacy 参考行（compat=legacy-method-reference，空图，legacy_json 快照逐字段保真）。
- `cargo clippy --offline --workspace --all-targets -- -D warnings` 通过。

## 四、已知取舍与遗留

1. **release 文件路径口径**：本批次 ask 关键路径写作 `releases/RELEASE_...`，按 docs/DOC-STANDARD.md「文档只存在于 docs/、更新记录放 docs/releases/」的仓库硬规则落为 `docs/releases/RELEASE_2026-11-13_PHASE8-P8-1.md`（先例 P7-2 到 P7-7 同路径）。
2. **put 不保存用户原始字节**：落库为存储侧 canonical 形状（version 字段与存储行一致），字节一致性判据 = 同版本任意两次 get 一致；外部系统若按原始文件字节对账需自行规范化（设计稿 §四）。
3. **run 半边未落**：CLI 无 run 子命令，WorkflowRun 无持久化面（17 号段待占）；domain 词表与跃迁表已冻结，P8-2（非智能节点执行器）/P8-3（Agent 节点 + 长任务）按词表实现即可。
4. **legacy 参考行不可执行**：13 行 compat=legacy-method-reference 永不进入执行器；需要可执行化的 workflow 必须人工重写 graph-v1（不提供自动转换，避免把方法层猜测升格为执行语义）。
5. **input_schema 形状门**：仅校验 type: object，输入实例符合性属 P8-2 执行器受理面。
6. **版本分配非原子**：分配与插入两步，同 workflow 并发 put 主键兜底必失败（CLI 单写者足够；多写者需改单事务，store.rs 头注声明）。
7. **链尾断言顺延涉及 7 个既有测试文件**：编号冻结规则的常规演进（链尾从 015 到 016），语义即「新增编号文件」，未推翻任何 Phase 0-7 已验收能力（510 基线用例全数保留并通过）。
8. **jsonschema if/then/else 依赖**：空图条件约束用 draft 2020-12 条件关键字，jsonschema 0.56（default-features=false）实测支持（meta-schema 门 + 正反例测试）；升级依赖时需回归该门。

## 五、回滚方式

1. 代码面：还原 Cargo.toml（workspace members 移除 crates/workflow-engine）、删除 crates/workflow-engine/、crates/domain/src/workflow.rs、schemas/workflow/、docs/phase8/，还原 domain/src/{lib.rs,error.rs} 与 7 个链尾测试文件（git checkout 即可，全为可重建面）。
2. 迁移面：016 为纯新增结构（新表 + 索引），回滚 = 删 016 文件 + 在开发库 `DROP TABLE workflow_definition; DELETE FROM schema_version WHERE version=16; PRAGMA user_version=15;`；生产库从未写入，无需处理。
3. 数据面：dev-p8.db 为 gitignore 副本（13 legacy 参考行 + 样例 2 版），直接删除即可。
