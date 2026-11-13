# RELEASE 2026-11-13：Phase 8 P8-3（Agent 节点 + 暂停恢复人工介入 + 六 ID 贯穿 + API 写面）

| 项 | 值 |
| --- | --- |
| 批次 | P8-3（Phase 8 第 3/4 批，ROADMAP §9.3 P8-3） |
| 日期 | 2026-11-13 |
| 基线 | HEAD 102cab5（P8-1/P8-2 改动同在未提交工作树） |
| 设计稿 | docs/phase8/WORKFLOW-CONTROL-PLANE.md（本批新增）+ docs/phase8/RUN-STATE-MACHINE.md（P8-2 既有） |
| 门禁 | cargo test --offline --workspace 582 passed 0 failed；cargo clippy --offline --workspace --all-targets -D warnings 0 error；selftest_phase8_workflow.sh 76/76 PASS exit 0；生产 platform*.db 指纹前后一致（零写） |

## 一、交付清单

1. **迁移 018_workflow_control_correlation.sql**（新增编号文件，测试副本验证）：控制表 workflow_run_control（1:1 对 workflow_run：pause_requested_at / cancel_requested_at / approval_node_id / approval_request_json / approval_json / approval_updated_at，014 agent_run_control 同款 consume-on-observe 语义）+ workflow_run 追加六 ID 关联列（trace_id / task_id / agent_run_id / tool_run_id / artifact_id / evidence_id，全部可空不回填，015 ALTER TABLE 先例 + 执行器 ADD COLUMN 守卫）+ workflow_run_node 追加 tool_run_id / agent_run_id 节点级实体链接列 + 三索引 + schema_version 登记 version=18 + user_version=18。
2. **Agent 节点执行器（exec/node_agent.rs）**：AgentNodeDriver 端口（AgentNodeRequest / AgentNodeOutcome）+ AgentNodeResult 映射；agent_run_id 预生成（AgentStore::new_run_id 同源）并在派发前落节点/run 链接列（跨进程控制传播据此发现运行中实体）；agent-engine RunRequest 增可选 run_id 钉定（additive，None = 既有行为零改动）。
3. **CLI 真实闭环（bin/rp-workflow AgentLoopDriver）**：AgentLoop + BrokerLink（rp-agent build_broker_link 同款装配）+ ToolLink（PythonWorkerDriver）+ EvidenceRegistrar + AuditEngine + KeywordSearchPlanner + FirstFamilyConfirmer；fresh run / PAUSED 续接按 agent_run 行现态分派（AgentLoop::run / AgentLoop::resume），trace_id = workflow run_id 落 015 链路。
4. **Human 人工介入节点（exec/node_human.rs）**：执行即写 018 介入记录并返回 awaiting 信号；run 转 paused 停在 awaiting_approval；resume --input 注入批准参数（浅并入恢复态 + approval_json 留痕）后节点重派发即成功；node kind 词表批内扩展（WorkflowNodeKind::Human 第 8 值，017 CHECK 域与 definition-v1 Schema enum 随批补入，词表仍单源）。
5. **控制面（control.rs WorkflowControl）**：pause 请求位写/消费、cancel 留痕、awaiting 记录与批准注入、propagate_pause_to_agents（agent-engine RunControl 写 014 请求位）与 propagate_cancel_to_entities（agent + tool 双面留痕）；WorkflowExecutor 增 pause() / resume_with_injection() / awaiting_approval() / start_with_id() / retry_with_id()，cancel() 升级为跃迁 + 记状态 + 传播。
6. **运行循环暂停消费（exec/mod.rs）**：波间 consume_pause_request（终局判定之后，run 实际完成时不吃暂停）；「等完成再 checkpoint」定稿（当前节点完成并落 checkpoint 后下一波转 paused）；agent 节点协作暂停例外路径（节点回 pending attempt 保留）；取消记状态定稿（tool_run.cancel_requested_at + 014 请求位 + 018 留痕）。
7. **六 ID 关联贯穿**：create_run 即写 trace_id（= run_id）与 task_id（input 顶层）；tool_run_id 经 submit_key（wf:run:node:aN）精确回查落节点/run 行；artifact_id / evidence_id 经 015 trace 面刷新最近值；handle_traces 增 workflow_run 探测（run_id/trace_id 双向）与 workflow_runs 链面臂。
8. **rp_server（apps/server/rp_server.py）**：只读 GET /api/workflows（定义列表 + ?id= 全版本）与 GET /api/workflow-runs（列表 + ?id= 详情：run 头行 + 节点行 + 018 控制行 awaiting 视图 + checkpoint seq + correlation 六 ID + trace_query 入口），双 schema 归并（platform*.db 停在 V1 守卫降级空态说明）；do_POST 白名单新增 run（集合路径）与 pause/resume/retry（动态路径 + body.run_id 两形状），一律经 rp-workflow 子进程（本进程零直连 SQL 写，P6-8 纪律），run/resume/retry 异步承载（run_id 预生成钉定，日志落 /tmp/rp-workflow-run-logs），pause 同步；鉴权四件套保持；RP_WORKFLOW_DB / RP_WORKFLOW_BIN 环境变量 + platform*.db 写面拒绝守卫。
9. **CLI 增量**：pause 子命令；run/retry --run-id 钉定；resume --input 注入；status 增 correlation 与 awaiting_approval 视图；nodes 增实体链接列；exit 口径更新（Paused 停等 = 0 非错误）。
10. **契约与样例**：schemas/workflow/run-request-v1.schema.json（写面四动作契约，字段形状与 rp_server 手工核对一致）；docs/phase8/samples/p8-agent-node.json 与 p8-human-node.json（validate 门 PASSED 实测）。
11. **文档**：docs/phase8/WORKFLOW-CONTROL-PLANE.md（本批设计稿：控制面语义 / ToolRun 取舍定稿 / 六 ID 关联 / API 写面 / 已知取舍七条）+ 本 RELEASE + docs/README.md Phase 8 行更新 + docs/phase8/WORKFLOW-DSL.md P8-3 词表注记。

## 二、判据实测（ask 自验五条）

1. **含 Agent 节点 workflow 暂停恢复**（selftest §11）：真实 agent run（rp-agent def-add agent.wf.demo + kb 工具 + python_worker 真实派发）进入 RUNNING 后 `rp-workflow pause` 受理 → workflow run 转 paused 且 agent run 转 PAUSED（014 协作暂停传播）→ agent_run_control 有对应控制行（goal_text 落库）→ workflow_run 六 ID 关联列（trace_id = run_id / task_id / agent_run_id）→ resume exit 0 续跑至 SUCCEEDED，已完成节点 start attempt 保持 1 不变（agent 节点重派发 attempt 1 → 2）。Rust 层 p8_3_control.rs 四测同源覆盖（Mock 驱动确定性）。
2. **人工介入节点**（selftest §12）：run 停在 paused + 018 控制行 awaiting（approval_node_id = review + 请求载荷 + approval_json 空）→ `resume --input '{"approval":{"ok":true}}'` → 终态 succeeded + approval_json 落值留痕。
3. **六 ID 对账**（selftest §13，python3 -I 只读）：workflow_run 六列与 GET /api/workflow-runs?id= 响应（correlation + run + 节点 agent_run_id）逐字段一致；/api/traces?id=<run_id> 正向可查（agent_run / workflow_run / artifact / evidence 入链）；/api/traces?id=<agent_run_id> 反向可查（回 workflow_run 行）；GET /api/workflows 列表含样例定义。
4. **selftest 全 PASS exit 0**：76/76（P8-2 46 断言保留全过 + P8-3 新增 30），含 POST 白名单守卫（白名单外 POST /api/stats 仍 405、白名单内缺参 400、无令牌 401）与生产 platform.db / platform-dryrun.db 指纹前后一致（mode=ro 只读核对）。
5. **cargo 门禁**：cargo test --offline --workspace 582 passed 0 failed（P8-2 基线 568 + 14）；cargo clippy --offline --workspace --all-targets -- -D warnings 0 error（exit 0）。

## 三、已知取舍与如实登记

1. **human 节点词表扩展走 017 批内补入**（非新迁移号）：SQLite 无法以新迁移改既有 CHECK 域；017 未入库提交（基线 HEAD 102cab5）、dev-p8.db 可删除重建、platform*.db 从未应用 016/017（生产零触及）；若 017 已入库提交则需表重建迁移。P8-1 冻结注「扩展须新增变体并升迁移」的落点，词表仍单源（domain），CHECK 域与 Schema enum 同批同值。
2. **API run/resume/retry 异步承载**：受理即回，受理拒绝（未知 workflow_id 等）只能从日志文件与「run 行从未出现」观测；运维面以 GET 轮询 + /tmp/rp-workflow-run-logs 日志为准。
3. **无批准参数 resume 重写介入请求再次停等**（幂等语义）：节点 attempt 随重派发递增；ask 判据只约束已完成节点不变（实测满足）。
4. **六 ID 单列承载最近值**：1:N 全集经 trace_id 查 v_trace_chain / /api/traces（015 trace 面主通道），单列不承载 N 方向。
5. **测试基线滚动**：P7-1/P7-5/P2-4a/P4-1 五处迁移链测试的链尾断言（user_version 17 → 18、reapply 计数 8 → 9）随 018 号段演进更新（断言按链尾距离演进的既有设计注记）。
6. **上一批遗留 caveat 沿用**：P8-1/P8-2 改动仍在未提交工作树；release 元信息阶段交付短哈希按仓库回填惯例补记；docs/handoff/workflow-script-RIP2-2026-11-12-RESUME.dwf.ts 改动在本批开工前已存在。
7. **零新增第三方 package**：Cargo.lock diff 仅 workflow-engine 增 agent-engine（workspace 内部依赖，vendor 无新条目）；浏览器面本批无视觉门禁触点（web/ 未改动）。
