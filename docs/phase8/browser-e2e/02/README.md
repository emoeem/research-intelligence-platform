# P8-4 浏览器实测证据（验收员独立复测，2026-11-14）

| 项   | 值                                                                                                                  |
| ---- | ------------------------------------------------------------------------------------------------------------------- |
| 性质 | Phase 8 验收员（独立于实施批次的复核）对 Workflow Studio 画布页的浏览器实测截图：拖拽、连线、保存、运行、底栏逐节点终态、版本回看 |
| 工具 | browser-use 真实浏览器（ZCode In-app Browser，1440x900）+ vite dev（4178 端口，RP_API_TARGET 代理）+ rp_server（7802 端口，RP_AUTH_TOKEN 鉴权开启，RP_AUX_DB=RP_WORKFLOW_DB=/tmp/p8acc_e2e.db（migrations/dev-p8.db 副本开发库），真实 rp-workflow 子进程链） |
| 数据 | 真实链路：保存走 POST /api/workflows put（服务端对账 source=rp-workflow-put，workflow_definition 落 wf-p8acc-e2e v1，4 节点 3 边与画布构建一致）；运行走 POST /api/workflow-runs（rp-workflow 子进程真实执行 run wfr-5ba2fdc0033e47bb93d5dbf8c8390c1b，succeeded，checkpoint seq 24）；tool 节点经 tool-engine + python_worker 真实派发（tool_run rn-1f87a878f0aa2fdb00000000，keyword_search 真实输出在底栏节点产物）；knowledge 节点经 context-broker 真实装配（context_id ctx-903ca9ead53afa9484d54f1c 在底栏节点产物）。无 route mock |
| 声明 | 页面内点击、输入、点击追加节点、保存、运行均为真实浏览器事件；两处管线适配如实登记：1) 面板拖入画布的 HTML5 dragstart 在 IAB 指针管线不触发（与 01/README 同一 Chromium 原生限制），以真实 DragEvent 事件链（DataTransfer 由真实 onDragStart handler 写入 + dragover + drop 落点坐标）完成同一拖拽路径，tool 节点真实落画布；2) 连线柄拖拽在 IAB cua 管线持续 30s 超时（@xyflow/system 拖拽路径多次重试均挂起），按 @xyflow/system 真实事件契约（源柄 mousedown 起手，document 级 mousemove 多步越过 dragThreshold=1，目标柄圆心 mouseup，getClosestHandle 按 connectionRadius 自行命中）以真实事件链完成三条连线，aria-label 逐一核对（Edge from start to tool-1 / from tool-1 to knowledge-1 / from knowledge-1 to end） |

## 截图清单

| 文件                            | 内容                                                                                                        |
| ------------------------------- | ----------------------------------------------------------------------------------------------------------- |
| 01-shell-five-zones.png         | 五区壳层初始态：版本面板（空 workflow 尚无版本）+ 节点分类面板（DSL 八类词表）+ 空画布（React Flow Control Panel/MiniMap）+ 运行配置 + 底栏执行日志（dev-p8.db 副本 7 条真实 run） |
| 02-nodes-edges.png              | 四节点三边画布：start/end/knowledge-1（点击追加，真实指针点击）+ tool-1（DragEvent 链拖入落点），三条连线可视化（xyflow 真实连接逻辑消费事件链产生） |
| 03-save-definition.png          | 保存回显：「已保存 wf-p8acc-e2e @ v1（4 节点 3 边）」+ v1 版本 chip + 「编辑最新（v1 基础上）」；运行配置 workflow_id 自动带入；服务端对账 workflow_definition source=rp-workflow-put，graph 与画布逐字段一致 |
| 04-run-node-states.png          | 运行终态：底栏 run 详情 wfr-5ba2fdc0033e47bb93d5dbf8c8390c1b（succeeded，checkpoint seq 24，暂停/恢复按钮在位），逐节点终态 end/knowledge-1/start/tool-1 全部 succeeded attempt 1，tool-1 带 tool_run rn-1f87a878f0aa2fdb00000000 链接，knowledge-1 与 tool-1 节点产物折叠块可展开（真实 context-broker 与 keyword_search 输出），run 产物 output 折叠块，trace 链接指向 /api/traces?id=<run_id> |
| 05-version-history-review.png   | 版本回看：v1 chip 点击后「历史回看只读」徽标（工作区徽标与版本面板「历史回看 v1（只读）」两处）+ 画布只读渲染该版 graph（4 节点 3 条 edge path 实测在位），Inspector 运行配置只读展示 |

## 实测路径与结论

1. 判据「多步骤 Workflow 可在 Web 中编辑、运行并查看每个节点结果」（计划 §14:832）逐动作达成：拖入 tool 节点、点击追加 start/end/knowledge、三条连线、填参数（tool_id=rip.kb.keyword_search + input，query=$/target）、保存（版本递增落库）、运行（真实执行到 succeeded）。
2. 底栏逐节点终态可见且为真实执行产物：节点行 state/attempt/tool_run 链接/节点产物 JSON 全部来自 017 workflow_run_node 与真实 ToolRun/装配输出，只读库对账一致（select node_id,state,attempt 四行全 succeeded attempt 1；workflow_checkpoint 4 行 max seq 24）。
3. 版本管理 Web 消费面：保存即出现版本 chip 与「编辑最新」入口，v1 chip 点击进入只读回看（两处只读徽标 + 该版 graph 完整渲染），与 016 workflow_definition 存储侧版本递增口径一致。
4. 六 ID trace 关联在画布 run 详情消费：trace 链接 /api/traces?id=wfr-5ba2fdc0033e47bb93d5dbf8c8390c1b 正向可查（matched_via=workflow_run_id，tool_runs 命中），tool_run_id 反向（matched_via=tool_run_id）与 agent_run_id 反向（matched_via=agent_run_id，回 wfr-5960b035...）均实测可达。
5. 鉴权：RP_AUTH_TOKEN 开启实例，本页全部取数与写端点经 Bearer 令牌 200（保存/运行），无令牌 /api/stats 401 实测。
6. 本次复核与前批 01/ 六步（含 human 停等与暂停恢复）互补：01/ 覆盖 human/agent 节点停等恢复与暂停请求位语义，02/ 以全新 workflow（wf-p8acc-e2e）从空白画布完整走一遍「建图、保存、运行、回看」闭环，两批截图互为独立证据。
