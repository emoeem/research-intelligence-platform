# RELEASE：Phase 7 P7-5 trace 统一关联与跨栈查询（2026-11-13）

> 本批把六类关联 ID（trace_id/task_id/agent_run_id/tool_run_id/artifact_id/
> evidence_id）从「散落在各表」收口成统一体系：类型集中定义在
> domain::ids，写面五表（agent_run/tool_run/artifact/evidence/audit_event）
> 统一注入 trace_id（015 迁移五列五索引），读面提供两个只读入口
> （rp-agent trace 子命令 + /api/traces 端点，另有 v_trace_chain SQL 视图），
> 按任一 ID 关联出任务、意图、上下文、工具、证据全链。用户可感知的变化：
> Runs 页 trace 徽标与 Run 详情六 ID 关联面可点击跳转 /traces 链路页；
> Python /api 响应带 correlation 对象与 trace 字段；worker 协议透传
> params.correlation 六 ID。本批为跨机续作首批：接手停运行 WIP 半成品
> （40ced58 已入库）续作收口，未推倒重来。

| 项 | 值 |
| --- | --- |
| 状态 | 生效中（编排批次 P7-5 第 1/3 批，SESSION-HANDOFF §十追加项 1） |
| 日期 | 2026-11-13 |
| 基线 | WIP 半成品 40ced58（停运行遗留按交接 §二.3 纪律入库，71 文件 3993 插入）；本批续作增量见「一、改动清单」，工作树改动共 10 文件（Rust 2 处 clippy 修复 + selftest 夹具与断言 + web 7 文件 prettier 格式化） |
| 数据库迁移 | 新增 015_trace_correlation.sql（audit_event/tool_run/artifact/evidence/agent_run 五表追加可空 trace_id 列 + 五索引 + v_trace_chain 只读视图；014 已被 P7-4 实际占用，编号顺延）；在测试内存库（database/agent-engine/tool-engine/context-broker 各迁移测试）与 /tmp 开发副本两处执行，手工验证副本 user_version=15；migrations/platform*.db 零写入（红线，selftest 前后 user_version=1 不变实测两次） |
| 自动化测试 | `cargo test --offline --workspace`：494 passed 0 failed（90 个测试目标；P7-4 基线 481 + 13 新增：domain ids 4 用例 + agent-engine trace_correlation_p7_5 集成 + tool-engine trace_correlation_p7_5 5 用例 + trace.rs 单元 2 用例等）；`python3 -m unittest discover -s workers/python/tests`：211 OK（跳过 11，与 SK 收口基线一致） |
| Web 门禁 | `npm run build`（tsc --noEmit + vite build）通过；`npm run test`（vitest）：95 passed（14 文件），含 trace-smoke 3 用例（六 ID 入参五类链、空态、引导文案）与 runs-smoke 9 用例（六 ID 关联面断言在场且可点击）；prettier 全过（补格式化 7 文件，见「四、坏消息与遗留」） |
| 静态检查 | `cargo clippy --offline --workspace --all-targets -- -D warnings` 通过（0 error；本批修复 2 处：crates/database/src/migrate.rs 未使用 import、crates/tool-engine/tests/trace_correlation_p7_5.rs doc 缩进） |
| 鉴权回归 | `bash apps/server/selftest_auth.sh`：11/11 PASS |
| 批次自验 | `bash apps/server/selftest_phase7_trace.sh`：33/33 PASS（见「三、端到端自验」）；另有保留副本手工复验双向链逐段一致 |
| 相关 | docs/SESSION-HANDOFF.md §十追加项 1、docs/phase0/PHASE-ROADMAP.md §8.3/§8.4、docs/DOC-STANDARD.md、docs/HANDOVER-RIP2-CONTINUATION.md（跨机续作记录） |

## 一、改动清单（本批增量，基线 40ced58 之上）

| 文件 | 改动 |
| --- | --- |
| crates/database/src/migrate.rs | clippy 修复：删除未使用 import（`crate::rusqlite::OptionalExtension`；WIP 引入 guard_alter_add_columns 后该 trait 方法不再直接调用） |
| crates/tool-engine/tests/trace_correlation_p7_5.rs | clippy 修复：doc 注释续行缩进（doc list item indentation） |
| apps/server/selftest_phase7_trace.sh | 夹具补 research_task 行（任务面五类链判据：agent-engine 只读关联不代写任务行，生产口径该行来自 M1 ETL，开发副本手工播种）；新增断言「方向1 tasks=1」 |
| web/src/App.tsx、web/src/routes/KnowledgePage.tsx、web/src/routes/PluginDetailPage.tsx、web/src/routes/PluginsPage.tsx、web/src/routes/RunDetailPage.tsx、web/tests/runs-smoke.test.tsx、web/tests/trace-smoke.test.tsx | prettier --write 纯格式化（换行归位，零逻辑改动；KnowledgePage/Plugin 两页为 SK-2 批次遗留未格式化面，P7-5 四文件为 WIP 未格式化面） |

基线 40ced58 已含（本批核对非重写）：crates/domain/src/ids.rs（六 ID 集中类型：TraceId/TaskId/EvidenceId 新类型 + AgentRunId/ToolRunId 类型别名收口 + ArtifactId 既有收编）、crates/agent-engine/src/trace.rs（733 行，统一只读关联查询 + 六 ID 方向探测）、migrations/015_trace_correlation.sql、audit-engine trace_by_trace_id、tool-engine submit_correlated/trace_id_of、agent-engine agent_loop/evidence/store/tool_link trace 注入与 retry 链继承、rp-agent trace 子命令、rp_server.py /api/runs correlation 与 /api/traces、worker_runtime.py correlation 透传、web TracePage/RunDetailPage/RunsPage 六 ID 面、两侧 trace_correlation_p7_5 集成测试、selftest 脚本。

## 二、六 ID 贯穿口径（设计要点）

1. **类型集中**（crates/domain/src/ids.rs）：TraceId/TaskId/EvidenceId 新类型（非空 + 128 字符上限校验，与冻结契约 run_id maxLength 一致）；AgentRunId/ToolRunId 为 RunId 类型别名（两族 run 表共享同一冻结契约形状，语义区分靠别名与查询层表探测，不为别名重复实现校验）；ArtifactId 沿用既有非空校验。全仓不再散落字面量校验。
2. **写面单锚传播**：agent_run 受理时生成或继承 trace_id（domain::TraceId 校验，RunControl::retry 链继承源 run，resume 续接沿用），随 tool_run 提交（tool-engine submit_correlated，None 不伪造关联列、非法 ID 拒落且受理校验面 D-TC-05 语义不变）、artifact/evidence 登记（EvidenceRegistrar 同链传播）、audit 事件（correlation_id 体系扩展：correlation_id = run_id 既有口径不动，trace_id 为跨实体族关联锚）统一注入。context_manifest/agent_run_step/agent_run_control 不加 trace_id 列（反重复建模：manifest 经 task_id 关联，step/control 经 run_id FK）。
3. **读面双入口同拓扑**：Rust rp-agent trace 子命令（agent-engine trace::trace_query）与 Python /api/traces（rp_server.py handle_traces）按同一探测顺序解析（run 族 id → artifact/evidence id（列直查 + run 行 join 兜底）→ trace_id/correlation_id 直查 → task_id（多 run 展开）→ context manifest 反查），汇出任务/意图/上下文/工具/证据/审计六面行。只读 SELECT，不写行；015 之前旧行 trace_id 为 NULL 按空段如实呈现，不回填。
4. **跨栈透传**：worker 协议 params.correlation 六 ID 原样回传 result.correlation（worker 不校验业务语义）；rp_server /api/runs 列表行带 trace_id（按库水位条件列，缺列库降级不炸），详情带 correlation 对象（trace_id/task_id/agent_run_id/tool_run_id/artifact_ids/evidence_ids）。
5. **Web 面**：Runs 页行内 trace 徽标链接（库水位条件出现，旧库不伪造）；Run 详情概要区六 ID 关联面（trace/task/agent_run/tool_run/artifacts/evidence）全部可点击跳 /traces?id=；/traces 链路页按任一 ID 汇出五类链全段与命中面。

## 三、端到端自验（ask 判据逐条实测）

**1. selftest 脚本（apps/server/selftest_phase7_trace.sh，33/33 PASS）**：真实 rp-agent CLI 端到端 run（真实 python_worker：rip_cli_worker + rip_kb_search，显式 --trace-id tr-p75-selftest）→ 五面同链落库核对（临时副本 mode=ro 只读：agent_run/tool_run/artifact/evidence 四行 trace_id 同值 + audit_event >= 3）→ rp-agent trace 三方向（trace_id 命中面 ['trace_id'] 且 tasks=1/manifests>=1/tool_runs=1/evidence=1/artifacts=1/audit>=3；evidence_id 命中面 ['evidence_id'] 同链同宽；task_id 命中面 ['task_id']）→ 真实 rp_server /api 响应 trace 字段断言（/api/runs 列表含 "trace_id": "tr-p75-selftest"，详情 correlation.trace_id/tool_run_id/artifact_ids/evidence_ids，/api/traces available=true 双方向同链）→ worker correlation 透传（六 ID 原样回传）→ 生产库零写核对（前后 user_version=1）。

**2. 保留副本手工复验**（独立于脚本，/tmp/rp_p75_manual.db，run_id arn-9921b916ade2fae20eb5f98b）：

- 方向1 trace_id=tr-manual-p75：matched_via=['trace_id']，counts = {tasks:1, manifests:1, context_items:8, tool_runs:1, artifacts:1, evidence:1, agent_runs:1, audit_events:4}（五类链全段：任务/意图/上下文/工具/证据 + 审计）。
- 方向2 evidence_id=ev-53ebb5af04d5e85985a5f30c：matched_via=['evidence_id']，trace_ids 同为 ['tr-manual-p75']，八段行逐段一致。
- audit_event 只读直查（mode=ro）按 trace_id 回放 4 事件：agent.run.accepted / context.intent.classify / agent.step.completed / agent.run.finished（correlation_id = run_id，trace_id 关联列同值）。
- v_trace_chain 视图按 trace_id 汇出 8 行（agent_run 1 / tool_run 1 / artifact 1 / evidence 1 / audit_event 4），副本 user_version=15。

## 四、坏消息与遗留

1. **TracePage 真实浏览器实测未在本批执行**：本批 web 面验收为 vitest mock smoke（trace-smoke 3 用例 + runs-smoke 六 ID 断言）；§九浏览器实测硬性要求归 Phase 7 阶段门禁/独立验收三站（交接文档第三节「Phase 7 门禁/独立验收/入库三站」），届时按 PW_CHROMIUM 本机口径执行。
2. **/api/traces 与 Rust 读面为同拓扑双实现**（Python 侧 20 行探测 SQL 与 Rust trace.rs 探测顺序一致）：跨栈只读查询无法共享实现（rp_server 为独立 python 进程），探测顺序以两侧测试锚定；后续若探测规则变化需双侧同步（风险登记，Phase 9 对外 correlation 接口时再评估收敛）。
3. **任务面段落依赖 research_task 行在场**：agent-engine 只读关联不代写任务行；无任务行的 task_id（如临时实验）链中任务段为空（如实呈现，不伪造）。生产与自验口径均有任务行（M1 ETL / 夹具播种）。
4. **prettier 修复越出 P7-5 文件面**：KnowledgePage/PluginDetailPage/PluginsPage 三文件为 SK-2 批次遗留未格式化（SK 批次门禁未含 web 五门禁），本批顺手归位，diff 纯格式化零逻辑。
5. **跨机环境事实**（本机续作首批实测）：cargo 需 RUSTUP_HOME=/home/emo/.rustup 前缀（ZCode shell HOME 指向 distrobox 家目录，rustup 在 HOME 下找不到工具链，宿主 /home/emo/.rustup 有 stable 1.99.1 等 4 个）；web npm 用用户级 /home/emo/.local/node22/bin（node v22.19.0 + npm 10.9.3，PATH 里 /usr/sbin/node 无 npm）。两处均为会话级 PATH/环境变量，未装任何系统级依赖（红线遵守）。

## 五、批次对位说明

本批为编排批次 P7-5 第 1/3 批（trace 统一关联与跨栈查询），对应 ROADMAP §8.3 Phase 7 车道内「trace 关联」项与 SESSION-HANDOFF §十追加项 1。第 2/3 批（P7-6 研究能力契约化、P7-7 Agent Studio 页）未开工；Phase 7 阶段级五门禁复测 + 独立验收 + 入库三站待 P7-7 后统一执行。
