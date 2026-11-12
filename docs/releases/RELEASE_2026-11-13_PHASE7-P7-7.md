# RELEASE：Phase 7 P7-7 Agent Studio 页与 AgentRun/step 轨迹读取面（2026-11-13）

> 本批是 Phase 7 编排第 3/3 批：把 AgentRun/step 轨迹读取面从「数据面已落
> 但无出口」收口成 CLI + HTTP + Web 三面可读（014 agent_run_step 已在
> state.rs 写入，本批前 rp-agent 与 rp_server 均未露出）；同时把 AgentsPage
> 从 P7-1 占位换成实页三区（SESSION-HANDOFF §11.4 接入点 4，用户硬性验收
> b）。用户可感知的变化：/agents 工作区现在有 Agent 定义与能力绑定对账、
> 运行记录与步骤轨迹回放（P7-5 六 ID 关联面可点击跳 /traces 与 RunDetail）、
> Task/Evidence/Research Memory 三视图；rp-agent 增 run-steps 子命令；
> rp_server 增 /api/agents 与 /api/agent-runs 两个只读端点。零迁移
> （013/014/015 数据面已足，本批只加读面）。

| 项 | 值 |
| --- | --- |
| 状态 | 生效中（编排批次 P7-7 第 3/3 批，SESSION-HANDOFF §11.4 接入点 4） |
| 日期 | 2026-11-13 |
| 基线 | P7-6 收口 2d3fda8（工作树干净）；本批增量见「一、改动清单」 |
| 数据库迁移 | 零新增迁移：006（agent_definition/agent_run）+ 013（三记忆表）+ 014（agent_run_control/agent_run_step）+ 015（agent_run.trace_id）数据面已足；本批全部为只读 SELECT 读面 |
| 后端门禁 | `cargo test --offline --workspace`：510 passed 0 failed（与 P7-6 基线一致，本批无新增 Rust 测试目标，CLI 行为由 selftest 覆盖）；`cargo clippy --offline --workspace --all-targets -- -D warnings` 0 error |
| Web 门禁 | `npm --prefix web run typecheck` 过；`npm --prefix web run build`（tsc + vite）过；`npm --prefix web run test`（vitest）100 passed 0 failed（15 文件；P7-6 基线 95 + agents-smoke 5 新增，含 shell-smoke 占位断言改实页断言的 1 例）；prettier --check 过（新改 7 文件全部归位） |
| 视觉门禁 | `npx playwright test`（qa/visual/pages.spec.ts）：55 passed（11 页 × 2 主题 × 2 视口 = 44 + 11 页 axe AA）；PW_CHROMIUM=/home/emo/.cache/ms-playwright/chromium-1234/chrome-linux64/chrome（Chrome for Testing 151，本机用户缓存，零系统级安装）；/agents 视觉用例 marker 换实页段标题 + /api/agents 与 /api/agent-runs 真实形状 stub |
| 批次自验 | `bash apps/server/selftest_phase7_agents.sh`：50/50 PASS exit 0（见「三、端到端自验」）；P7-5 selftest_phase7_trace.sh 回归 33/33、selftest_auth.sh 回归 11/11 |
| 浏览器实测 | browser-use 真实浏览器逐区实测 6 截图 docs/phase7/browser-e2e/02/（README 附实测路径与结论）：三区列表、定义详情、run 详情与步骤回放、Memory 三视图、tool_run 跳 RunDetail、trace 跳 /traces 关联链，页面数据来自真实 rp_server + 真实端到端 run（无 route mock） |
| 相关 | docs/SESSION-HANDOFF.md §9（浏览器实测硬性要求）、§11.4、docs/phase0/PHASE-ROADMAP.md §8.3/§8.4、docs/DOC-STANDARD.md |

## 一、改动清单（本批增量，基线 2d3fda8 之上）

| 文件 | 改动 |
| --- | --- |
| apps/server/rp_server.py | 新增 handle_agents 与 handle_agent_runs 两个只读 handler + API_ROUTES 注册（/api/agents、/api/agent-runs）；主库加 aux 双 schema 归并同款（schemas_with_table/select_merged/sort_by 三个模块级助手，与 handle_traces 的 schemas_with 同拓扑提为共享面）；capability_refs/context_policy/memory_refs_json/input_json/refs_json 全部经 parse_json_or_none 解析；do_POST 白名单不动（新端点 POST 仍 405 readonly）；模块 docstring 补 P7-7 端点行 |
| crates/agent-engine/src/bin/rp-agent.rs | 新增 run-steps 子命令：复用 agent_engine::state::RunStateStore::steps（014 agent_run_step 持久化轨迹权威面）输出全量步骤 JSON（seq 升序，含 tool/tool_run/status/artifact/evidence/content_hash/summary/error/elapsed_ms/input 快照），run 不存在 exit 1；usage 文本同步 |
| apps/server/selftest_phase7_agents.sh | 新增（50 断言）：端到端 run + run-steps 读取面 + 013 夹具 + /api 双端点响应断言 + 写面静态守卫 + POST 405 + 生产库指纹零写核对 |
| web/src/lib/api.ts | 新增 Agent Studio 读面类型（AgentDefinitionRow/AgentRunRow/AgentRunStepRow/三记忆行/AgentList/AgentDetail/AgentRunList/AgentRunDetail）与 fetchAgents（重载：列表/id 详情）+ fetchAgentRuns + fetchAgentRunDetail + agentsPath/agentRunsPath 路径组装 |
| web/src/routes/AgentsPage.tsx | 占位页换实页三区：Agent 定义与能力绑定（定义卡 + ?id= 详情能力绑定对账/运行摘要）、运行记录与步骤轨迹回放（run 列表 + 详情概要六 ID 面与 seq 升序步骤回放）、Task/Evidence/Research Memory 视图；六 ID 点击跳转：tool_run_id 到 /runs/<id>（RunDetail），其余到 /traces?id=（消费 P7-5 关联面）；useApi fetcher 注入消费 api.ts 三个新函数 |
| web/src/workspaces.ts、web/src/App.tsx | 注释与 hint 去「占位/P7-7 填充」标注（工作区 7 hint 改实页描述） |
| web/src/routes/RouteFallback.tsx | axe 修正：roleless div 不允许 aria-label（aria-prohibited-attr serious），改挂 role=status（隐式 aria-live=polite）；本机视觉门禁逐页扫描赶上 Suspense 回退帧时暴露的预置缺陷，11 页 axe 全绿 |
| web/tests/agents-smoke.test.tsx | 新增 5 用例：三区段标题与定义列表字段、选中 run 后步骤轨迹回放与六 ID 跳转 href 断言（tool_run 跳 RunDetail、其余跳 /traces）、三类 Memory 行字段、选中 agent 后能力绑定对账、空列表真实空态 |
| web/tests/shell-smoke.test.tsx | /agents 深链接用例从占位断言（fills-in=P7-7）改实页断言（三区 section testid + 固定 tab 激活） |
| web/qa/visual/pages.spec.ts | /agents marker 从「P7-7」换「Agent 定义与能力绑定」；API_STUBS 增 /api/agents 与 /api/agent-runs 真实形状列表 stub |
| docs/phase7/browser-e2e/02/ | 新增实测截图 6 张 + README（沿用 01/ 目录惯例） |
| docs/releases/RELEASE_2026-11-13_PHASE7-P7-7.md、docs/README.md | 本文件与 Phase 7 行登记 |

## 二、读取面口径（设计要点）

1. **同一数据面三个出口**：rp-agent run-steps（RunStateStore::steps）、/api/agent-runs?id=（handle_agent_runs）、AgentsPage 步骤回放，三面同读 014 agent_run_step（seq 升序权威面）；不再有「轨迹已落库但读不出来」的缺口（本批开工时实查：state.rs:273 steps 已在、CLI 与 HTTP 均未露出）。
2. **主库加 aux 双 schema 归并**（handle_traces 同款）：agent_definition/agent_run/agent_run_step/agent_run_control/三记忆表/tool 逐表按 schema 探测（schemas_with_table），单库缺表零贡献不炸；归并行 _src 标注来源库；去重键（agent_id,version）/run_id/seq。主库停 V1 水位时 agent 面全部落在 aux 开发副本，与 Phase 1-4 端点守卫口径一致。
3. **只读红线**：两 handler 纯 SELECT（selftest 内 ast 静态断言锁死 INSERT/UPDATE/DELETE/CREATE/DROP/ATTACH 关键字）；do_POST 白名单不动，POST /api/agents 与 /api/agent-runs 实测 405 readonly；生产 platform*.db 以 URI mode=ro 前后指纹（user_version + 逐表行数）核对一致。
4. **能力绑定对账**：/api/agents?id= 把 capability_refs 逐条对 tool 注册面（主库优先、aux 回退，handle_tools 详情同款），registered/lifecycle 直接暴露给页面徽标；「绑定引用的工具未注册」在页面上可见，不静默。
5. **六 ID 关联消费**：/api/agent-runs?id= 的 correlation（trace_id/task_id/agent_run_id/tool_run_ids/artifact_ids/evidence_ids）由步骤轨迹与 evidence memory 台账聚合（读面聚合不写库）；tool_run_id 跳 RunDetail（/runs/<id>），其余六方向跳 /traces?id=，Agent Studio 与 P7-5 关联面打通。
6. **013 三记忆视图**：详情按 run_id 归并三表（task/evidence/research），content_json/refs_json 解析；CLI run 未挂 Memory（with_memory 可选接线留 Phase 8），自验用夹具行验证读面（同 P7-5 research_task 播种口径，不伪造生产数据）。

## 三、端到端自验（ask 判据逐条实测）

**1. selftest（apps/server/selftest_phase7_agents.sh，50/50 PASS exit 0）**：cargo --offline 构建 rp-tool/rp-agent → 生产库指纹基线（platform.db user_version=1 + platform-dryrun.db 逐表行数）→ 临时开发副本登记工具 + def-add + research_task 夹具 → 真实 python_worker 端到端 run（SUCCEEDED，显式 --trace-id tr-p77-selftest）→ rp-agent run-steps 断言（exit 0、steps_count>=1、steps[0].seq=0、tool_run_id 与报告步骤一一致、input 快照在场；未知 run exit 1）→ 013 三记忆夹具行播种（引用真实 evidence/artifact，content_hash 从 evidence 表取权威值）→ 写面静态守卫（ast 断言两 handler 零 SQL 写）→ 真实 rp_server 断言：/api/agents 列表与 ?id= 详情（definitions/tool_bindings registered=true/runs 含 steps_count 与 trace_id）、/api/agent-runs 列表与 ?id= 详情（steps seq=0、correlation 六 ID 与报告一致、counts 三记忆>=1、control.goal_text 在场）、未知 run 404 no_agent_run、未知 agent 404 no_agent → POST 双端点 405 → 生产库指纹收尾一致。

**2. rp-agent run-steps 手工复验**（独立于脚本，/tmp/rp_p77_manual.db，run_id arn-255f1d12edb3035990b9caeb）：输出单行 JSON，steps[0] 含 seq=0、round=0、tool_id=rip.kb.keyword_search、tool_run_id=rn-300a9eca37dfc81ed2f89a7f、artifact_id/evidence_id、content_hash=sha256 64hex、summary、elapsed_ms=96、input={"keyword":"nuclei"}，exit 0。

**3. 视觉与浏览器双路实测**：playwright 视觉门禁 55/55（含 /agents 四组合 + axe）；browser-use 真实浏览器（IAB 1440x900）逐区实测 6 截图见 docs/phase7/browser-e2e/02/README.md：真实 rp_server（7801，RP_AUX_DB 指向手工副本）+ vite dev proxy（4178）+ 真实端到端 run 数据，三区、两详情、Memory 三视图、两条六 ID 跳转逐点走查。

## 四、坏消息与遗留

1. **RouteFallback 修的是预置缺陷且越出 P7-7 文件面**：roleless div 挂 aria-label 违反 axe aria-prohibited-attr（serious），旧机器门禁未暴露（Suspense 回退帧时序差异），本机 11 页 axe 中 5 页被该预置缺陷扫中；本批改组件根因（role=status），diff 1 行属性级，全仓 55 用例复扫全绿。
2. **CLI run 不写 013 三记忆表**（with_memory 可选接线默认关闭，Phase 8 尾洞）：/api/agent-runs 详情的三记忆视图对 CLI 产生的 run 如实显空态；自验读面用夹具行（同 P7-5 research_task 播种口径）。agent_loop 直连测试（P7-3 memory_contract）覆盖真实写入面。
3. **/api/agents 与 /api/agent-runs 为 Python 读面单实现**：Rust 侧 rp-agent run-steps 只覆盖步骤轨迹单表，列表/详情聚合（绑定对账、六 ID correlation、三记忆归并）仅在 rp_server；两侧口径由 selftest 对 014 表行的同一断言锚定（steps[0].seq/tool_run_id 双面一致）。Phase 9 对外 correlation 接口时再评估是否收敛（P7-5 §四.2 同款风险登记）。
4. **/traces 深链接下壳层切换器显示空白（工作区 1 显示「总览」占位）**：/traces 不属于十一工作区路径（P7-5 引入的链路页），Inspector 当前对象显示为总览；预置表现非本批引入，Phase 7 阶段级收口审阅时统一处置。
5. **IAB 对 Playwright locator click 的可操作性管线超时**（P7-1 已登记的工具侧备注）本机复现：定义卡与 trace 链接的 locator click 超时，改用 dom_cua 节点路径真实点击完成；同一交互在 vitest fireEvent 与 playwright 视觉门禁两路均过，非应用缺陷。
6. **本机环境口径**（沿用交接）：cargo 需 RUSTUP_HOME=/home/emo/.rustup 前缀；web npm 用用户级 /home/emo/.local/node22/bin；视觉门禁 PW_CHROMIUM 指向本机用户缓存 Chrome for Testing 151（~/.cache/ms-playwright/chromium-1234，机器无关口径实测：command -v 全缺、用户缓存在场，零系统级安装）。

## 五、批次对位说明

本批为编排批次 P7-7 第 3/3 批（Agent Studio 页 + AgentRun/step 轨迹读取面），对应 ROADMAP §8.3 Phase 7 车道内「Agent Studio」项与 SESSION-HANDOFF §11.4 接入点 4。至此 Phase 7 七批（P7-1..P7-7）全部完成；阶段级三站（浏览器全工作区复验/独立验收/入库推送）待统一执行（沿用 P7-5/P7-6 批次口径，推送归阶段级收口站）。
