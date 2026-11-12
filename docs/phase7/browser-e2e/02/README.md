# P7-7 浏览器实测证据（Agent Studio 实页三区，2026-11-13）

| 项   | 值                                                                                                          |
| ---- | ----------------------------------------------------------------------------------------------------------- |
| 性质 | P7-7 批次（Agent Studio 页落新壳 + AgentRun/step 轨迹读取面）的浏览器实测截图：三区列表、详情、步骤轨迹回放、三类 Memory 视图、六 ID 跳转 |
| 工具 | browser-use 真实浏览器（ZCode In-app Browser，1440x900）+ vite dev（4178 端口，RP_API_TARGET 代理）+ rp_server 本地开发模式（7801 端口，RP_AUX_DB=/tmp/rp_p77_manual.db 开发副本，真实端到端 run 数据） |
| 数据 | 真实链路：rp-agent def-add + run（真实 python_worker rip.kb.keyword_search，run_id arn-255f1d12edb3035990b9caeb，trace_id tr-4149ef6e19441bb119f1fef3）+ 013 三记忆夹具行（引用真实 evidence/artifact）；页面数据全部来自 /api/agents 与 /api/agent-runs 真实响应，无 route mock |
| 声明 | 浏览器内点击、跳转均为真实 UI 操作；工具侧备注沿用 P7-1 口径（IAB 对部分 Playwright locator click 的可操作性管线会超时，改用 dom_cua 节点路径点击真实完成，同一交互非应用缺陷） |

## 截图清单

| 文件                                    | 内容                                                                                       |
| --------------------------------------- | ------------------------------------------------------------------------------------------ |
| 01-agents-three-sections-overview.png   | /agents 三区全貌：第一区「Agent 定义与能力绑定」定义卡（agent.manual.p77 @ v0.1.0 + 能力徽标 rip.kb.keyword_search + 运行合计）、第二区「运行记录与步骤轨迹回放」run 行（SUCCEEDED · 1 步 · trace 链接）、第三区 Memory 引导空态 |
| 02-agent-detail-tool-bindings.png       | 点击定义卡展开第一区详情：能力绑定对账（rip.kb.keyword_search 已注册 · VALIDATED）+ 运行摘要（共 1 条 · SUCCEEDED 1，run 行可点击） |
| 03-run-detail-steps-replay.png          | 点击运行行展开第二区详情：概要与关联（P7-5 六 ID 面：agent_run/trace/tool_run/artifacts/evidence 全部可点击）+ goal 与时间线 + 步骤轨迹回放（步骤 0 SUCCEEDED · 96 ms · tool_run 链接） |
| 04-memory-views.png                     | 第三区三类 Memory 视图真实行：Task Memory（mem-t-manual-p77 ACTIVE）、Evidence Memory（mem-e-manual-p77，evidence/artifact 链接 + sha256 快照）、Research Memory（mem-r-manual-p77「nuclei 关键词检索成果」2 条回链）；步骤行计划入参快照折叠块同框 |
| 05-tool-run-jump-rundetail.png          | 六 ID 跳转验证 1：点击 tool_run id 跳 /runs/rn-300a9eca37dfc81ed2f89a7f（RunDetail），submit_key 还原 agent run 关联（agent:arn-...:0:rip.kb.keyword_search） |
| 06-trace-jump-chain.png                 | 六 ID 跳转验证 2：点击 trace id 跳 /traces?id=tr-4149ef6e19441bb119f1fef3（P7-5 关联链页），命中面 trace_id，counts：agent_runs 1 · manifests 1 · context_items 8 · tool_runs 1 · artifacts 1 · evidence 1 · audit_events 4（任务段 0 行为真实空态：手工副本未播种 research_task） |

## 实测路径与结论

1. 三区列表落位：定义卡、run 行、Memory 引导文案逐区可见，真实 API 数据渲染无「加载失败」。
2. 第一区详情：agent 维度展开（能力绑定对账对账到 tool 注册面 lifecycle）+ 运行摘要（run 行与第二区联动选中）。
3. 第二区详情：run 维度展开（六 ID 关联面 + 步骤轨迹 seq 升序回放 + 计划入参快照折叠）。
4. 第三区视图：013 三记忆表按 run_id 归并展示（指针台账带 evidence/artifact 回链与 sha256 快照）。
5. 六 ID 跳转两条路径实测：tool_run_id 到 RunDetail（/runs/<id>）、trace_id 到 /traces 关联链页，跨工作区消费 P7-5 关联面打通。
