# Phase 7 独立验收浏览器实测证据（2026-11-13）

| 项   | 值 |
| ---- | ---- |
| 性质 | Phase 7 独立验收员（PhaseAcceptance）的浏览器实测截图，独立于 P7-7 批次自验（02/），不复用批次数据 |
| 工具 | browser-use 真实浏览器（ZCode In-app Browser，1440x900）+ vite dev（4179 端口，RP_API_TARGET 代理）+ rp_server dev 模式（7802 端口，RP_AUX_DB=/tmp/p7_verify_dev.db 开发副本，RP_AUTH_TOKEN_FILE 指向空临时文件，与 selftest_auth dev 模式同口径；用户 .rp-workbench-token 未动） |
| 数据 | 真实链路：rp-tool register + rp-agent def-add + 两次 rp-agent run（真实 python_worker rip.kb.keyword_search；run_id arn-86d16148c64febaa53327520 / arn-a6beddee7053213a24cf9f88，trace_id tr-p7-accept / tr-p7-accept-task，第二条带 --task-id task-p7-accept 验证任务面）+ 013 三记忆夹具行（引用真实 evidence/artifact，refs_json 为 selftest 同款 ref_type/ref_id 数组形状）；页面数据全部来自 /api/agents 与 /api/agent-runs 真实响应，无 route mock |
| 声明 | 浏览器内点击、跳转均为真实 UI 操作；工具侧备注沿用 P7-1/P7-7 口径（IAB 对部分 Playwright locator click 可操作性管线超时，改用 dom_cua 节点路径真实点击，同一交互在 vitest/playwright 视觉门禁两路均过，非应用缺陷） |

## 截图清单

| 文件 | 内容 |
| ---- | ---- |
| 01-agents-five-zones-overview.png | /agents 五区全貌：图标轨 + 顶栏（工作区切换器 7·Agents / 全局搜索 / 运行任务）+ 编号标签条（11 固定 tab，7.1 选中）+ 主区三段（定义卡 agent.trace.selftest @ v0.1.0 + 2 条 SUCCEEDED run + Memory 引导空态）+ 右侧 Inspector（当前对象/关联知识工具/运行配置） |
| 02-agent-detail-tool-bindings.png | 第一区详情：能力绑定对账（tool 注册面：rip.kb.keyword_search 已注册 · VALIDATED）+ 运行摘要（2 条 run 行带 trace 链接） |
| 03-run-detail-correlation-steps.png | 第二区 run 维度展开入口：当前 run 面板出现（概要与关联 P7-5 六 ID 面起始可见） |
| 04-run-sixid-steps-replay.png | 六 ID 关联面全量（agent / agent_run / trace / tool_run / artifacts / evidence 全部可点击链接 + goal + 时间）+ 步骤轨迹回放（agent_run_step seq 升序：步骤 0 SUCCEEDED · 85 ms · tool_run/artifact/evidence 链接 + sha256 + 计划入参快照折叠块）+ 三 Memory 视图 + 底部 Activity 面板五页签同框 |
| 05-memory-views.png | 三 Memory 视图（修正夹具 refs_json 为 selftest 规范形状后）：Task Memory 1 行 / Evidence Memory 1 行（evidence+artifact 链接 + sha256）/ Research Memory 1 行（2 条回链） |
| 06-tool-run-jump-rundetail.png | 六 ID 跳转 1：点 tool_run id 跳 /runs/rn-bd95dd3fb3b0174b012add2b（RunDetail），壳层切换到 9·Runs，submit_key=agent:arn-86d16...:0:rip.kb.keyword_search 还原 agent 关联，Artifact 列表可预览/下载 |
| 07-trace-jump-chain.png | 六 ID 跳转 2：点 trace id 跳 /traces?id=tr-p7-accept（P7-5 链路页），命中面 trace_id，counts：tasks 0 · agent_runs 1 · manifests 1 · context_items 8 · tool_runs 1 · artifacts 1 · evidence 1 · audit_events 4（任务段 0 行为真实空态并如实标注） |
| 08-session-tabs-multitab.png | /traces 页壳层全貌（会话多标签证据见 domSnapshot：标签条出现 rn-bd95dd3fb3b0174b012add2b 与 /traces 两个会话 tab，各带关闭按钮；截图当时横滚条未滚到会话段） |

## 实测路径与结论

1. 五区壳层真实可用：图标轨 / 顶栏 + 多标签条 / 主区 / 右 Inspector / 底 Activity 全部渲染，会话 tab 随详情路由真实新增（rn-…、/traces 两个）。
2. Agent Studio 三区为实页非占位：定义与能力绑定对账到 tool 注册面 lifecycle（VALIDATED），运行行与步骤轨迹（015 correlation + 014 steps）真实渲染，三类 Memory 视图按 run_id 归并展示。
3. 六 ID 跳转两条路径真实通过：tool_run → /runs/<id>（RunDetail），trace → /traces?id=（链路页，counts 与 rp-agent trace 子命令一致）。
4. /traces 深链下壳层切换器/Inspector 显示「总览」为 P7-5 引入链路页的预置表现（非十一工作区路径），沿批次 caveat 留阶段级收口处置，实测确认现象仍在。
5. 本目录与 01/（P7-1 五区壳层）、02/（P7-7 批次自验）互补，构成 Phase 7 阶段级浏览器实测证据链。
