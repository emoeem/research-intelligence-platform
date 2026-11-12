# RELEASE 2026-11-12 Phase 7 P7-2：Agent Engine 最小执行闭环

> 本批后 RIP 有了第一个能跑的 Agent：一条 `rp-agent run` 命令完成「目标 → Broker 装配（含意图确认回路与 L3 expand 记账）→ 计划 → 经 Tool Engine 调用登记工具 → Artifact/Evidence 落账 → 结束报告」全流程，且每一步可对账（agent_run、tool_run、artifact、evidence、context_manifest、context_ref_log、audit_event 七张表互证）。修掉的隐患：AgentRun 此前只有状态词表没有迁移表（P7-1 保守核心面的已知空档），capability_refs 此前只有纯函数没有 Registry 接线（幽灵工具可进库的口子被关上）。

## 元信息

| 项         | 值 |
| ---        | --- |
| 状态       | 生效中 |
| 日期       | 2026-11-12 |
| 适用版本   | 工作区未提交（基线 git 2bf8e9c 之后，提交由编排层收口） |
| 相关       | docs/phase0/PHASE-ROADMAP.md §8.3 P7-2、docs/RIP-2.0-PLATFORM-AND-TOOLING-PLAN.md §9:454-477、docs/phase7/browser-e2e/01/（P7-1 壳层批次） |
| 数据库迁移 | 零新迁移（006 agent_definition/agent_run 两表由 P7-1 提前通道建好，本批只补读写面）；幂等验收：migrations 目录副本对临时库连续 migrate 两次，第二次 no-op 且 sqlite_master 快照零差异（crates/agent-engine/tests/migration_idempotent.rs）；生产 platform.db 与 platform-dryrun.db 未触及（实查 mtime 2026-11-07，且 kb_search 脚本走 file:...?mode=ro 只读通道） |
| 自动化测试 | `cargo test --offline -p agent-engine -p domain -p context-broker`：21 个测试二进制 128 通过 0 失败；全仓 `cargo test --offline --workspace`：446 通过 0 失败（上批全仓口径未见更大数字，本批为增量） |
| 静态检查   | `cargo clippy --offline --workspace --all-targets -- -D warnings` 通过（0 error）；rustfmt 对 agent-engine 与 domain 已执行 |

## 一、交付内容（按批次判据逐条）

### 1. AgentDefinition/AgentRun 持久化读写面与状态机

- `crates/agent-engine/src/store.rs`：AgentStore（pool + tool_registry::Registry 双依赖），定义登记/读取/最新版本/全量列表，run 建行/读取/CAS 状态推进/记忆引用追加。capability_refs 以 006 注释原文形状（`[{"tool_id":...,"version":...}]`）序列化回读，域对象重建走 `CapabilityRef::try_new` 再校验。
- 状态机补充在 `crates/domain/src/agent.rs`（不在 engine 重复建模）：`AgentRunState::LEGAL_TRANSITIONS` 8 行合法对（QUEUED→RUNNING/CANCELLED、RUNNING→PAUSED/SUCCEEDED/FAILED/CANCELLED、PAUSED→RUNNING/CANCELLED），`can_transition` 纯函数 + `AgentRun::transition_to`；重试（FAILED→QUEUED）有意不入表，P7-5 按「新 run 重放同定义版本」设计（与 tool_run 契约同向）。新增 `DomainError::InvalidAgentTransition`。
- 存储层 `transition_run`：域校验 + `UPDATE ... WHERE run_id=? AND state=?` CAS（0 行更新按并发漂移拒绝），RUNNING 进入记 started_at，终态进入记 finished_at。

### 2. capability_refs 绑定引用校验接线（P7-1 判据收口）

- 谓词接线到 Registry 查询（不直读 tool 表，MODULE_BOUNDARIES 口径）：指向 Registry 不存在的工具即拒绝；版本钉定（`tool@x.y.z`）与 Registry 已登记版本比对，未登记版本同样拒绝。拒绝即整行不入库，agent_definition 零残留（单测断言 COUNT(*)=0）。
- 两条实测路径：单测 `capability_refs_unknown_tool_rejected_and_no_row_written`；CLI 实测 `rp-agent def-add --caps "rip.ghost.tool"` 回显 `capability ref targets unknown tool "rip.ghost.tool"` 且 exit 1。

### 3. 执行闭环（计划 §9:454-477 八步）

- `broker_link.rs`：Broker 每轮上下文唯一入口，`assemble_confirmed` 承载 ambiguous 意图确认回路（确认词回填重装配留痕，弃权/达轮次上限按 ambiguous 继续 + warnings 留痕，不伪造 ok）；`expand` 透传（EXPANDED 记账进 context_ref_log，l3_reserved 扣账在 Broker）。
- `agent_loop.rs`：Planner 端口 + 内置 `KeywordSearchPlanner`（确定性最小规划：capability_refs ∩ Broker L2 tool_briefs 选工具，入参取显式 input 或目标文本首个 ASCII 词元）；护栏：步数上限、绑定外工具拒绝（run 落 FAILED 留痕）、失败即收尾（重试/换面属 P7-5）。
- `tool_link.rs`：D-TC-05 受理 + 状态机 + 信封全走 tool-engine 公开接口；RunDriver 端口双实现：MockDriver（契约测试注入口径）与 PythonWorkerDriver（真实 python_worker Adapter，argv 固定清单不经 shell，计划 §6:370 禁令同等生效）。
- `evidence.rs`：工具结构化输出登记 artifact（role=primary，sha256 实算；artifacts_dir 给出时落文件，缺省 ripartifact:// 引用形态如实标注）+ evidence（type=log，与 artifact 同哈希对账，source_pk 指回 tool_run）。
- Agent 零库表直读（知识/工具候选全从 AssembledContext 取），P7-3 判据「每轮上下文来自 Broker」本批已按此口径实现。

### 4. SK 插件知识源可用性（L2 召回 + L3 expand）

- 集成测试 `closed_loop_p7_2.rs` 种子插件 + APPROVED knowledge_source_entry（SK-1 形态），断言 `PluginSource` 召回条目 `kse:sk.test.recon:catalog_entry:nmap-basics` 进装配且 `l3_expansions[0].granted_tokens > 0`，context_ref_log 存 EXPANDED 事件。CLI 面源接线同 rp-context（rulepack + registry + artifact + PluginSource::mount_all + SK-1 引用证据固化写面）。

### 5. rp-agent CLI 定稿（ROADMAP §8.4 验收命令基线）

```bash
cargo run --offline -p agent-engine --bin rp-agent -- run <agent_id> --goal "<目标文本>" [--input <json>] [--db <path>]
# 附带面：def-add / def-show / run-show
```

默认库 `migrations/dev-p7.db`（开发副本），生产 platform*.db 名字命中即拒接（同 rp-knowledge-fetch guard）。

## 二、端到端实测（真实 python_worker 路径）

命令与产物（dev-p7.db）：

```bash
cargo run --offline -q -p tool-registry --bin rp-tool -- register --manifest manifests/tools/rip.kb.keyword_search.json --db migrations/dev-p7.db
# {"command":"register","tool_id":"rip.kb.keyword_search","version":"0.1.0","ok":true,...}
cargo run --offline -q -p agent-engine --bin rp-agent -- run agent.research.minimal \
  --goal "检索 nmap 端口扫描相关知识并调用登记工具" --task-id task-p72-cli-accept \
  --db migrations/dev-p7.db --artifacts-dir /tmp/rp-agent-p72-artifacts --out /tmp/rp-agent-p72-report.json
# exit 0，报告 state=SUCCEEDED，intent_confidence=1.0，steps=1，elapsed_ms=179
```

七表对账（只读查询，python3 -I sqlite3）：

| 表 | 实测值 |
| --- | --- |
| agent_run | arn-14fa2ac2020f7c28c6ff26f4，SUCCEEDED，memory_refs 记 manifest/artifact/evidence 三引用 |
| tool_run | rn-1c163ec8efc0aab80a716202，SUCCEEDED，179 ms |
| artifact | art-e45ebbcde46e0ca53778b115，role=primary，sha256=12c488ea…396628（407 字节，正文落 /tmp 产物目录） |
| evidence | ev-879d7184e9a0ee3e2777495d，type=log，content_hash 与 artifact 同哈希，source_pk 指回 tool_run |
| audit_event | 事件链 agent.run.accepted → context.intent.classify → agent.step.completed → agent.run.finished（correlation_id = run_id） |
| 工具真实执行 | worker rip-cli-worker 实跑 rip_kb_search.py，keyword=nmap 命中工具/技能两组（工具面 nmap 行、技能面 active-scan 行） |

## 三、已知取舍与待验证

1. **Artifact 正文持久化是登记面不是归档面。** artifacts_dir 给出时写文件；缺省 uri 为 ripartifact:// 引用形态，正文归档属 Artifact Manager 后续批次，报告与文档如实标注，不做伪装。
2. **CLI 驱动面只承载 python_worker。** P7-2 CLI 基线按绑定首工具 Manifest 的 runtime 块组装 Adapter（runtime.type 非 python_worker 即显式报错）；podman_vm/http_api 等 Runtime 的 Agent 调用面留 Phase 2 适配器扩展。MockDriver 保留为测试注入口径，不进 CLI。
3. **L3 展开策略是最小面。** 每轮至多展开一条 L2 Knowledge/Plugin Summary 条目（装配序首个），展开失败降级 warning 不炸 run；按规划器需求按需展开属 P7-3 深度集成。
4. **确认回路不提分类置信度。** 确认词回填后重装配若仍低于 0.5 阈值就如实保持 ambiguous（触发词重复不增族命中数），P7-3 起接模型在环/人工回路再升级语义。
5. **内置规划器是启发式不是模型。** 关键词推导（首个 ASCII 词元，缺省整段截断 100 字符）只保闭环确定性，真实研究规划属 P7-3/P7-5（ROADMAP §8.5 风险 3 的确定性/模型在环分离纪律）。
6. **dev-p7.db 含本次验收数据**（1 定义、1 run、1 tool_run、artifact/evidence 各 1 行），保留作复查样本；重置可直接删文件重跑（迁移幂等自动重建）。
7. 待验证：workflow 与 Agent 的节点级集成（Phase 8）、Memory 三类面（P7-4）、运行控制护栏补全（P7-5）、trace 回放覆盖五类事件（P7-6）。

## 四、回滚方式

本批零迁移，回滚即 git revert 代码提交（domain::agent 状态机与 agent-engine 新 crate 均为追加面，不触碰既有表结构）；dev-p7.db 删除即可，无生产面数据。
