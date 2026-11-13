# P8-4 浏览器实测证据（Workflow Studio 画布页五区壳层，2026-11-14）

| 项   | 值                                                                                                         |
| ---- | ---------------------------------------------------------------------------------------------------------- |
| 性质 | P8-4 批次（Workflow Studio 画布页 @xyflow/react 落五区壳层 + 保存/运行/暂停/恢复接 P8-3 API）的浏览器实测截图 |
| 工具 | browser-use 真实浏览器（ZCode In-app Browser，1440x900）+ vite dev（4178 端口，RP_API_TARGET 代理）+ rp_server（7801 端口，RP_AUTH_TOKEN 鉴权开启，RP_AUX_DB=RP_WORKFLOW_DB=migrations/dev-p8.db 开发库，真实 rp-workflow 子进程链） |
| 数据 | 真实链路：画布图保存走 POST /api/workflows put（rp-workflow put 四道门落库，版本存储侧递增）；run 走 POST /api/workflow-runs（rp-workflow 子进程真实执行）；tool 节点经 tool-engine + python_worker 真实派发（tool_run rn-8ab3224540dc1bb600000000 等三个）；agent 节点经 agent-engine 真实闭环（agent_run arn-f44530d00067e7301a54ee1f + artifact/evidence 关联）；018 控制行暂停请求位与 consume-on-serve 语义真实生效。无 route mock |
| 声明 | 页面内点击、输入、拖拽均为真实浏览器事件；两处登记：1) 面板拖入画布的 HTML5 dragstart 在 IAB 指针管线不触发（Chromium 原生限制），以真实 DragEvent 事件链（DataTransfer 携带节点类型词 + dragover + drop 落点坐标）完成同一拖拽路径；2) 连线柄拖拽在多点多路径下 IAB cua 管线不稳定（单次 30s 超时一次），改用紧凑五点路径（柄上驻留两点）的真实指针拖拽完成，全部四条边均为真实指针事件连线 |

## 截图清单

| 文件                          | 内容                                                                                                  |
| ----------------------------- | ----------------------------------------------------------------------------------------------------- |
| 01-drag-node.png              | 步骤 1 拖拽节点：面板「人工介入 human」拖入画布落点生成 human-1 节点（随后点击追加 start/end 边界节点，三节点在位） |
| 02-connect-edges.png          | 步骤 2 连线：start→agent-1→human-1→end 三条边（v2 图，agent 节点 agent_id=agent.wf.demo 已在 Inspector 填参，三边 aria-label 逐一核对） |
| 03-save-definition.png        | 步骤 3 保存：「已保存 wf-p84-e2e @ v1（3 节点 2 边）」落库回显 + v1 版本 chip 出现（服务端 GET /api/workflows?id= 对账 graph 逐字节还原 start→human-1→end，source=rp-workflow-put） |
| 04-run-awaiting.png           | 步骤 4 运行：v1 图 run wfr-3970760ac75e45ecae6f7fc1457b38fd 发起后 human 节点停在 awaiting_approval @ human-1（018 控制行 approval_request {"node_key":"approval"} 在 RunPanel 呈现，批准参数 inject 框预填，checkpoint seq 4） |
| 05-pause-requested.png        | 步骤 5 暂停：wf-p84-pause-demo v1 run wfr-626789cbdada4bd397fb55f6fca0bd6a 在 tool-1 执行中点击暂停，018 请求位写入后于波间边界消费（consume-on-observe），run 停在 tool-1 成功/tool-2 待跑边界（checkpoint seq 12）；徽标文本「PAUSE-REQUESTED 2026-11-14 01:09」在消费前瞬态捕获于操作日志 |
| 06-resume-succeeded.png       | 步骤 6 恢复：恢复受理后 run 终态 succeeded（checkpoint seq 20），节点行 start/tool-1/tool-2/tool-3/end 全部 succeeded attempt 1，tool_run 链接三条（rn-8ab3224540dc1bb600000000 / rn-2b29adbb02c6ecf800000000 / rn-ffac3289463e98b200000000）+ run 产物 output 折叠块 |

## 实测路径与结论

1. 五区壳层落位：节点分类面板（DSL 八类词表）、@xyflow/react 画布（自定义 wf 节点 + Background/Controls/MiniMap）、节点参数 Schema 表单与运行配置、底部执行日志节点状态产物、版本列表与历史回看，逐区截图在位。
2. 保存走版本递增：wf-p84-e2e v1（3 节点 2 边）、v2（4 节点 3 边）、wf-p84-pause-demo v1（5 节点 4 边）三连保存，服务端 workflow_definition 版本清单与页面回显一致（DB 对账实测）。
3. 运行停等语义：human 节点 run 停在 paused + awaiting_approval，018 控制行审批请求与 inject 框同屏；恢复注入 {"approval":{"ok":true}} 后（另一次 run）节点重派发成功。
4. 暂停吃 RUNNING 窗口：暂停请求位仅对 queued/running 生效（control.rs request_pause 对 paused 是 no-op，P8-3 冻结语义），本次在 tool-1 运行中点击暂停成功，波间消费后停在 checkpoint seq 12，恢复后续跑至 succeeded（seq 20），节点 attempt 保持 1（已完成节点不重跑）。
5. 六 ID 关联在画布 run 详情消费：agent_run / artifact / evidence / tool_run 链接逐条可点（wfr-5960b035... run 详情实测 arn-f44530d00067e7301a54ee1f + art-7b3c672f640ae078ca8fa5dc + ev-a1f63969a6d49159c5f67e76）。
6. 鉴权接线：P8-4 为 main.tsx 补 Bearer 令牌来源（localStorage rip_auth_token，P5-4 会话面落地前的最小桥），E2E 实例 RP_AUTH_TOKEN 开启，写端点（保存/运行/暂停/恢复）全部经鉴权 200，无令牌请求 401。
7. 过程中发现并修复两处页面缺陷（均已回归单测）：run 受理异步落库晚于受理响应（详情首拉空行）改有界补拉；resume/retry 后单次刷新会错过 running 窗口致详情停旧态，改动作后 8s 脉冲轮询。
