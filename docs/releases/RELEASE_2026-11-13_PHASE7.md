# 更新说明 · 2026-11-13（Phase 7 阶段总账：Agent Engine 与工作台壳层升级）

> Phase 7 全段（P7-1 到 P7-7）交付完成后的总账。用户可感知的变化：Web Workbench 从管理面板升级为 VS Code 式五区工作平台（十一工作区、中央多标签、Inspector、Activity）；平台第一次有了能跑、能对账、能回放轨迹的 Agent（一条 `rp-agent run` 完成「目标 → Broker 装配 → 计划 → 工具执行 → 证据落账 → 报告」全流程）；trace_id/task_id/agent_run_id/tool_run_id/artifact_id/evidence_id 六 ID 贯穿 Rust、Python worker 与 Web 三栈可查；七个研究能力以 14 份 Schema 契约交付（不是 prompt）；Agent Studio 三区实页与 /runs、/traces 关联面互相跳转。本文件是阶段总账，分批实施细节以各批 RELEASE（P7-2 到 P7-7 六份）为准，不在此重写。

| 项         | 值 |
| ---        | --- |
| 状态       | 生效中（Phase 7 阶段总账，阶段收口入库批次） |
| 日期       | 2026-11-13 |
| 适用版本   | git `4f79e2b`（P7-7）及之后，收口提交为本批次 |
| 阶段交付   | 本收口提交（内容：本总账 + docs/README.md 登记 + SESSION-HANDOFF 完成/落地注记 + browser-e2e/03 独立验收截图；git 短哈希按仓库回填惯例由后续 docs 批次补记，先例 `05ed5ee`/`f02d9b3`） |
| 数据库迁移 | 阶段累计 006（agent_definition/agent_run，P7-1 通道提前）+ `013_agent_memory.sql`（三记忆表）+ `014_agent_run_control.sql`（控制/步骤两表）+ `015_trace_correlation.sql`（trace_id 列与视图）；各迁移在开发副本幂等验收（连续两次第二次 no-op，migration_idempotent.rs）；生产 platform.db 与 platform-dryrun.db 全程未触及（本次入库复测 selftest 前后指纹核对一致） |
| 自动化测试 | `cargo test --offline --workspace`：94 个测试目标 510 passed 0 failed 0 ignored（入库当日复测，与 P7-6/P7-7 批次口径一致）；`python3 -m unittest discover -s workers/python/tests`：211 OK 跳过 11（入库当日复测） |
| 静态检查   | `cargo clippy --offline --workspace --all-targets -- -D warnings` 0 error（入库当日复测）；web prettier --check 全过（入库当日复测） |
| 相关       | docs/phase0/PHASE-ROADMAP.md §8.3/§8.4、docs/RIP-2.0-PLATFORM-AND-TOOLING-PLAN.md §9/§10.2、docs/SESSION-HANDOFF.md §三/§十/§十一、分批 RELEASE 六份（见「一、」表） |

## 一、批次 1 到 7 汇总

编排批次 P7-1 到 P7-7 七批，对应 ROADMAP §8.3 Phase 7 车道全部内容行。批次 1 到 4 由活体工作流完成（编排运行 dwfrun-954913b0-9e59-48cd-a2af-1202026edf95，半成品入库 WIP 备份 `7effba9`），批次 5 到 7 跨机续作收口（`2bd35a8`/`2d3fda8`/`4f79e2b`），本提交为阶段收口入库批次（总账 + 登记 + 独立验收截图）。

| 批次 | 内容 | 关键交付 | 记录 |
| --- | --- | --- | --- |
| P7-1 | WebUI 壳层升级 | 五区布局（顶栏/图标轨/中央多标签/Inspector/Activity）+ 十一工作区 + TabArea/InspectorPanel/ActivityPanel 数据无关通用组件，既有 10 页路由不变迁移；browser-use 真实浏览器 9 截图 | 随 WIP `7effba9` 入库，正式门禁经后续批次全仓复测覆盖；无独立 RELEASE，以本总账与 [browser-e2e/01/](../phase7/browser-e2e/01/README.md) 截图 README 为准 |
| P7-2 | Agent Engine 最小执行闭环 | AgentStore + 状态机 CAS + capability_refs Registry 引用校验接线 + 计划 §9 八步闭环 + rp-agent CLI 定稿 | [RELEASE_2026-11-12_PHASE7-P7-2.md](RELEASE_2026-11-12_PHASE7-P7-2.md) |
| P7-3 | 三类 Memory 与 Context Memory 雏形 | 013 三表 + 生命周期跃迁闸门（domain/迁移 CHECK/FK 反向锚定三层）+ Evidence 快照三方对账 | [RELEASE_2026-11-13_PHASE7-P7-3.md](RELEASE_2026-11-13_PHASE7-P7-3.md)、评审 [phase7/AGENT-MEMORY-REVIEW.md](../phase7/AGENT-MEMORY-REVIEW.md) |
| P7-4 | 运行控制九类护栏与 AgentRun 持久化轨迹 | 014 agent_run_control/agent_run_step 两表 + 九类控制契约 10 用例 | [RELEASE_2026-11-13_PHASE7-P7-4.md](RELEASE_2026-11-13_PHASE7-P7-4.md) |
| P7-5 | trace 统一关联与跨栈查询 | 015 + 六 ID 类型集中（domain::ids）+ 写面单锚传播 + Rust/Python 读面同拓扑双入口 + /traces 链路页 + worker correlation 透传 | [RELEASE_2026-11-13_PHASE7-P7-5.md](RELEASE_2026-11-13_PHASE7-P7-5.md)（commit `2bd35a8` 已推送） |
| P7-6 | 研究能力契约化 | crates/research-engine 七能力确定性 mock 纯函数 + 14 份 request-v1/result-v1 Schema + rp-research CLI 四道门 | [RELEASE_2026-11-13_PHASE7-P7-6.md](RELEASE_2026-11-13_PHASE7-P7-6.md)（commit `2d3fda8`）、设计稿 [phase7/RESEARCH-CAPABILITY-CONTRACTS.md](../phase7/RESEARCH-CAPABILITY-CONTRACTS.md) |
| P7-7 | Agent Studio 页与轨迹读取面 | /api/agents 与 /api/agent-runs 只读端点 + rp-agent run-steps 子命令 + AgentsPage 实页三区（定义绑定对账/运行与步骤回放/三 Memory 视图）+ 六 ID 点击跳转 | [RELEASE_2026-11-13_PHASE7-P7-7.md](RELEASE_2026-11-13_PHASE7-P7-7.md)（commit `4f79e2b`） |

## 二、五道门禁数字（批次演进与入库当日复测）

门禁口径沿用 Phase 5/6 收口惯例五组：cargo test、clippy、worker unittest、web 门禁（typecheck/build/vitest/视觉门/prettier）、selftest。全仓 cargo test 批次基线演进：446（P7-2）→ 463（P7-3）→ 481（P7-4）→ 494（P7-5）→ 510（P7-6）→ 510（P7-7，无新增 Rust 测试目标，行为覆盖转 selftest）。web 门禁自 P7-5 起纳入批次口径（vitest 95 用例），P7-7 达 100 用例 + 视觉门 55 用例；P7-2 到 P7-4 批次口径未含 web 与视觉门（其 RELEASE 如实记录）。

| 门禁 | 批次时点数字（来源：各批 RELEASE 元信息表） | 入库当日复测（2026-11-13 本机实测） |
| --- | --- | --- |
| 1. cargo test --offline --workspace | 446/463/481/494/510/510（P7-2 至 P7-7 逐批） | 94 目标 510 passed 0 failed 0 ignored，exit 0 |
| 2. cargo clippy -D warnings | 各批 0 error（P7-2 至 P7-7 逐批） | exit 0，0 error |
| 3. worker unittest | 211 OK 跳过 11（P7-5 起口径，延续 SK 收口基线） | Ran 211，OK (skipped=11)，exit 0 |
| 4. web 门禁（typecheck/build/vitest/视觉门/prettier） | typecheck+build 各批过；vitest 95（P7-5）→ 100（P7-7，15 文件）；视觉门 55 passed（P7-7，11 页 × 2 主题 × 2 视口 + 11 页 axe AA）；prettier 全过（P7-5/P7-7） | typecheck exit 0；build exit 0；vitest 15 文件 100 passed exit 0；视觉门 55 passed exit 0（PW_CHROMIUM 指向用户缓存 Chrome for Testing 151，零系统级安装）；prettier --check 全过 exit 0 |
| 5. selftest | auth 11/11（P7-5 回归）；phase7_trace 33/33（P7-5）；phase7_agents 50/50（P7-7） | auth PASS=11 FAIL=0 exit 0；trace PASS=33 FAIL=0 exit 0（含 worker correlation 透传与生产库零写）；agents PASS=50 FAIL=0 exit 0（含生产库前后指纹一致） |

复测命令原样（仓库根执行，可复制）：

```bash
cargo test --offline --workspace
cargo clippy --offline --workspace --all-targets -- -D warnings
python3 -m unittest discover -s workers/python/tests
npm --prefix web run typecheck && npm --prefix web run build && npm --prefix web run test && npm --prefix web run format:check
cd web && PW_CHROMIUM=/home/emo/.cache/ms-playwright/chromium-1234/chrome-linux64/chrome npx playwright test qa/visual/pages.spec.ts && cd ..
bash apps/server/selftest_auth.sh && bash apps/server/selftest_phase7_trace.sh && bash apps/server/selftest_phase7_agents.sh
```

浏览器二进制机器无关口径：视觉门禁不依赖系统级浏览器安装，用 PW_CHROMIUM 环境变量注入用户级缓存浏览器（本机为 /home/emo/.cache/ms-playwright/chromium-1234/chrome-linux64/chrome，Chrome for Testing 151）；换机时先探测本机可用浏览器再注入等价变量，缺浏览器用用户级缓存方案补齐（如 `npx playwright install chromium`，只写用户缓存目录），绝不装系统级依赖。

## 三、trace 统一关联说明（P7-5 收口的体系）

Phase 7 把「可对账」从单 run 内推进到跨实体跨栈，设计要点（细节见 P7-5 RELEASE「二、六 ID 贯穿口径」）：

1. **类型集中**：TraceId/TaskId/EvidenceId 新类型落在 crates/domain/src/ids.rs（非空 + 128 字符上限），AgentRunId/ToolRunId 为 RunId 类型别名（两族 run 表共享冻结契约形状），全仓不再散落字面量校验。
2. **写面单锚传播**：agent_run 受理时生成或继承 trace_id，随 tool_run 提交（submit_correlated）、artifact/evidence 登记（EvidenceRegistrar 同链）、audit 事件（correlation_id = run_id 既有口径 + trace_id 跨实体族关联锚）统一注入；context_manifest/agent_run_step/agent_run_control 不加 trace_id 列（经 task_id/run_id FK 关联，反重复建模）。
3. **读面双入口同拓扑**：Rust rp-agent trace 子命令与 Python /api/traces 按同一探测顺序解析（run 族 id → artifact/evidence id → trace_id/correlation_id → task_id → manifest 反查），汇出任务/意图/上下文/工具/证据/审计六面行；015 之前旧行 trace_id 为 NULL 如实呈现空段，不回填。
4. **跨栈透传**：worker 协议 params.correlation 六 ID 原样回传 result.correlation（worker 不校验业务语义）；本次入库复测 selftest_phase7_trace 第 7 步再次实测通过。
5. **Web 消费**：/runs 行内 trace 徽标（库水位条件出现）、Run 详情六 ID 关联面全部可点、/traces 链路页按任一 ID 汇出五类链全段与命中面；Agent Studio（P7-7）把 tool_run 跳 /runs/<id>（RunDetail），其余六方向跳 /traces?id=，与 P7-5 关联面打通。

## 四、新壳与 Agent Studio 截图路径引用（浏览器实测证据链）

三个截图目录构成阶段级浏览器实测证据链（均为真实 UI 操作，各 README 内附实测路径、数据来源与结论）：

| 目录 | 性质 | 内容 |
| --- | --- | --- |
| [phase7/browser-e2e/01/](../phase7/browser-e2e/01/README.md) | P7-1 批次（壳层） | 9 截图：五区全貌（11 固定 tab）、深链接会话 tab、关闭回退、Inspector/Activity 折叠、Activity 页签切换、切换器跳转、通知气泡、Agents 占位页；静态服务器口径（无后端，数据面「加载失败」属预期） |
| [phase7/browser-e2e/02/](../phase7/browser-e2e/02/README.md) | P7-7 批次自验 | 6 截图：Agent Studio 三区、定义详情能力绑定对账（VALIDATED）、run 详情与步骤轨迹回放、三 Memory 视图、tool_run 跳 RunDetail、trace 跳 /traces 关联链；真实 rp_server + vite proxy + 真实端到端 run（无 route mock） |
| [phase7/browser-e2e/03/](../phase7/browser-e2e/03/README.md) | 阶段级独立验收（本批入库，PhaseAcceptance） | 8 截图：五区全貌、能力绑定对账、run 关联面起始、六 ID 全量与步骤回放、三 Memory 视图、tool_run 跳转、trace 跳转链（counts 与 rp-agent trace 一致）、会话多标签；独立于批次自验不复用批次数据，真实 run 两条（arn-86d16148c64febaa53327520 / arn-a6beddee7053213a24cf9f88）+ 013 夹具 |

## 五、已知取舍

1. **/traces 深链下壳层切换器与 Inspector 显示「总览」**：P7-5 引入链路页的预置表现（非十一工作区路径），03/ 独立验收实测确认现象仍在（其 README 第 4 条）；沿批次 caveat 留阶段级收口处置，归收尾审阅批次。
2. **研究能力真实接线留 Phase 8**：P7-6 交付的是契约 Schema + 确定性 mock 纯函数（反 prompt 断言零命中），agent_loop 调用七能力的真实接线归 Phase 8 尾洞；rp-agent run 未挂 Memory（with_memory 可选接线同归 Phase 8）。
3. **跨栈读面为同拓扑双实现**：apps/server/rp_server.py handle_traces 与 crates/agent-engine/src/trace.rs 探测顺序一致但代码两份（独立 python 进程无法共享实现），两侧测试锚定；探测规则变化需双侧同步，风险登记，Phase 9 对外 correlation 接口时再评估收敛。
4. **research-engine 无单元测试用例**：行为覆盖在 16 个集成用例（Schema 全量门/正反例/往返/确定性/逐能力行为），P7-6 有意取舍（纯函数面小，集成测试更贴契约）。
5. **P7-1 批次无独立 RELEASE**：半成品随 WIP `7effba9` 入库，正式门禁由后续批次全仓复测覆盖；壳层批次细节以 01/ 截图 README 与 web/src/workspaces.ts 注释为准。
6. **迁移编号与 ROADMAP §8.3 表行错位**：编排批次 P7-3 内容对应 ROADMAP P7-4 行（三类 Memory），P7-2 已提前覆盖 ROADMAP P7-3 行（Broker 深度集成）；迁移号按实际合入顺序取 013（P7-3 RELEASE 批次序号说明原文）。
7. **视觉门禁依赖构建产物**：playwright 走 dist 静态服务器（test:visual:gate 先 build），跨机复测需先 `npm run build`；build 的 chunk 500 kB 告警为 echarts vendor 既有提示，非本阶段引入。
8. **编排运行已结束**：dwfrun-954913b0-9e59-48cd-a2af-1202026edf95（2026-11-12 起跑，11-13 用户手动停）全部批次已收口，无需再 Resume；后续 Phase 8 起新开编排。

## 六、回滚方式

1. **本收口提交**（总账 + 登记 + 03/ 截图）：纯 docs 变更零代码零迁移，`git revert <本提交哈希>` 即完全回滚。
2. **代码按批次倒序 revert**：P7-7 `4f79e2b`（读面 + web 实页）→ P7-6 `2d3fda8`（整批为新增文件加 Cargo.toml 一行 members 注册，可整目录移除，回 494 基线）→ P7-5 `2bd35a8`；revert 后 web/ 需 `npm run build` 重建 dist（构建产物不入库）。
3. **迁移回退**：006/013/014/015 均为追加式迁移，SQLite 单向，不做物理删迁移；如需回滚库结构，以恢复库副本（开发副本可重建）或后续补偿迁移实现；生产 platform.db 未触及，无生产回滚面。
4. **WIP 备份 `7effba9`** 含批次 1 到 4 全部代码，revert 影响面大，不建议单独操作；若需回退壳层，按 web/ 前端文件单独还原（壳层为纯前端增量，后端仅 006 迁移与 rp-agent 少量面）。
5. **生产库零写红线**：本次入库复测三个 selftest 均含生产库零写核对（auth/trace user_version 收尾不变，agents 前后指纹一致），Phase 7 各批同一口径。
