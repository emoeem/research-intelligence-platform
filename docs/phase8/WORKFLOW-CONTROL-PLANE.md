# Workflow 控制面与智能节点（P8-3 设计稿）

> 本文件是 Phase 8 P8-3 批次（Agent 节点 + workflow 级暂停恢复 + 人工介入 + 六 ID 关联 + API 写面）的设计稿与口径登记。上游：docs/phase8/RUN-STATE-MACHINE.md（P8-2 运行状态机）、docs/phase0/PHASE-ROADMAP.md §9.3 P8-3、计划 §14:830（运行、暂停、恢复、失败重试和版本管理）与 §9:474（暂停恢复能力清单）。代码面：crates/workflow-engine/src/control.rs、src/exec/node_agent.rs、src/exec/node_human.rs、migrations/018_workflow_control_correlation.sql、apps/server/rp_server.py（/api/workflows、/api/workflow-runs、POST 写面）。

| 项 | 值 |
| --- | --- |
| 状态 | 生效中（批次 P8-3，第 3/4 批） |
| 日期 | 2026-11-13 |
| 相关 | docs/phase8/RUN-STATE-MACHINE.md、docs/phase8/WORKFLOW-DSL.md、apps/server/selftest_phase8_workflow.sh、docs/releases/RELEASE_2026-11-13_PHASE8-P8-3.md |

## 一、结论先行

1. Agent 节点执行体 = Phase 7 agent-engine：运行面经 `AgentNodeDriver` 端口注入（CLI 装 `AgentLoopDriver` 真实闭环，测试装 Mock），fresh run 与 PAUSED 续接（AgentLoop::run / AgentLoop::resume）由端口实现按 agent_run 行现态分派；控制面复用 agent-engine `RunControl` pause/cancel/retry 与 014 agent_run_control 请求位（D-TC-04 consume-on-observe），引擎 crate 零 agent 状态机知识、不重复实现控制面。
2. workflow 级协作暂停落 018 `workflow_run_control` 请求位（014 同款 consume-on-observe）：`rp-workflow pause` 写请求位并经 agent-engine RunControl 向运行中 agent_run 传播（014 控制行）；run 循环波间消费，「等完成再 checkpoint」定稿：当前节点完成并落 checkpoint 后下一波转 paused，运行中节点不抢占。取消记状态定稿：cancel 直接跃迁之外向运行中实体传播留痕（tool_run.cancel_requested_at、014 cancel 请求位、018 控制行 cancel_requested_at）。
3. 人工介入节点（node kind `human`，词表批内扩展）执行即写 018 介入记录（approval_node_id + approval_request_json，approval_json 置空），run 转 paused 停在 awaiting_approval；批准经 `resume --input '<json>'`（或 API POST inject）注入：参数浅并入恢复态 + approval_json 落值留痕，节点重派发后即刻成功，下游续跑。
4. workflow_run 六 ID 关联列（018 追加，015 trace 面同款列名）：trace_id（= run_id，创建即写，P8-2 惯例的成列化）/ task_id / agent_run_id / tool_run_id / artifact_id / evidence_id；workflow_run_node 追加 tool_run_id / agent_run_id 节点级 1:1 实体链接列。1:N 全集经 trace_id 查 v_trace_chain 与 /api/traces，单列承载最近值做快速对账锚。
5. rp_server 读面 /api/workflows 与 /api/workflow-runs（只读 SELECT 零 SQL 写，agent 读面同款双 schema 归并）；写面 do_POST 白名单新增 run（集合路径）与 pause/resume/retry（`/api/workflow-runs/<run_id>/<action>` 或 `<action>` + body.run_id），一律经 rp-workflow 子进程落库，本进程零直连 SQL 写（P6-8 plugin_write 同款纪律）；鉴权四件套保持（require_auth、constant-time 比较、失败节流、写面强制鉴权）。

## 二、词表扩展（human 节点，批内登记）

P8-1 冻结注预留「扩展须新增变体并升迁移」落点：本批新增 `WorkflowNodeKind::Human`（词形 `human`，第 8 值）。017 迁移尚未入库提交（基线 HEAD 102cab5，workflow_run_node 的 node_kind CHECK 与 schemas/workflow/definition-v1.schema.json enum 均随批补入），dev 副本（migrations/dev-p8.db）删除重建即可，platform*.db 从未应用 016/017（生产零触及）。词表仍只有 domain 一处权威源，CHECK 域与 Schema enum 同词值，不新增第三处。human 节点无必填参数（required_params = []），可选 params：`prompt`（审批人说明）、`request`（请求附加载荷，`$/` 引用先解析）、`key`（批准参数在 run state 的落键，缺省 `approval`，形状 `[A-Za-z0-9][A-Za-z0-9_.-]*`）。受理门约束：human 出边必须全 seq（branch 从 human 出被既有 branch 形状规则拒绝），fan 子节点限定 tool/knowledge/condition（agent/human 禁入 fan）。

## 三、控制面（control.rs + 018 workflow_run_control）

| 列 | 语义 |
| --- | --- |
| pause_requested_at | 协作暂停请求时刻（consume-on-observe，消费即清空） |
| cancel_requested_at | 取消请求留痕（不消费，历史面） |
| approval_node_id | 人工介入节点锚（awaiting 与 approved 共用） |
| approval_request_json | 介入请求载荷（prompt / request / node_key） |
| approval_json | 批准参数（resume 注入后落值；NULL = 未批准） |
| approval_updated_at | 介入记录最近更新时刻 |

「停在 awaiting_approval」判定 = approval_node_id 非空 且 approval_json 为空 且 run 现态 paused（CLI status 与 /api/workflow-runs 详情均透出该视图）。

暂停传播链（判据 1）：`rp-workflow pause` → WorkflowExecutor::pause 写 018 请求位 + WorkflowControl::propagate_pause_to_agents（agent_run WHERE trace_id = workflow run id AND state = 'RUNNING' → agent-engine RunControl::pause 写 014 agent_run_control.pause_requested_at）→ agent 循环步间消费（agent run 转 PAUSED）→ 节点执行器返回 Paused → 节点回 pending（attempt 保留）→ run 转 paused。resume 时节点重派发（attempt +1），端口按 agent_run 现态走 AgentLoop::resume（已完成步骤不重跑）。

取消传播链（记状态定稿）：WorkflowExecutor::cancel → run 行直接跃迁 cancelled（P8-2 既有）→ 018 控制行 cancel_requested_at 留痕 + propagate_cancel_to_entities（agent_run：RUNNING/QUEUED/PAUSED 经 RunControl::cancel；tool_run：RUNNING/WAITING_FOR_RUNNER 经 tool-engine request_cancel 写 cancel_requested_at）。运行中节点不抢占（协作语义与 ToolRun D-TC-04 同源），取消后节点若自然完成，其终态写入照常落库但 run 保持 cancelled 终态。

## 四、ToolRun 衔接取舍（ask 定稿项）

- **pause：等完成再 checkpoint（选中）。** 消费点在 run 循环波间（上一节点终态 + checkpoint 已落、下一节点未派发），运行中 ToolRun 自然完成后 run 才转 paused。理由：checkpoint 完整性（每个终态节点必有快照，恢复语义零特例）、零抢占复杂度（python_worker 不需要中断通道）、确定性（同图同输入恒同轨迹）。取消并记状态（另一选项）不作为 pause 的语义，作为 cancel 的传播留痕语义落面（见上节），两种取舍均有测试覆盖（p8_3_control.rs 的 pause_waits_for_running_tool_node_then_checkpoints 与 cancel_records_state_on_in_flight_tool_run）。
- **agent 节点例外路径：** agent 循环自带步间协作暂停（014 请求位），所以 agent 节点不等整个 agent run 完成才停：传播后 agent run 在步间转 PAUSED，节点回 pending，run 立即转 paused。这是「等完成」语义的协作式例外，与 014 语义一致。

## 五、六 ID 关联（对账 015 trace 面）

| 列 | 写入时机 | 对账方式 |
| --- | --- | --- |
| trace_id | create_run 即写（= run_id，本 run 即链锚） | /api/traces?id=<run_id> 全链汇出 |
| task_id | create_run 即写（input 顶层 task_id 字符串） | research_task join |
| agent_run_id | agent 节点派发前预生成落列（节点行 + run 行双写） | agent_run.trace_id = run_id 反查 |
| tool_run_id | tool 节点完成后经 submit_key（wf:run:node:aN）精确回查落列 | tool_run.trace_id = run_id + submit_key |
| artifact_id / evidence_id | tool/agent 节点终态后 refresh_trace_correlations（trace_id = run_id 最新行） | artifact/evidence.trace_id = run_id |

单列承载「最近一个实体」：1:N 全集（多 tool 节点、多 artifact）不复制进 run 行，经 trace_id 查 v_trace_chain / /api/traces 汇出（015 trace 面主通道）。节点级 1:1 链接（workflow_run_node.tool_run_id / agent_run_id）补足精确对账与跨进程控制传播（pause 传播按 agent_run.trace_id 反查，链接列供对账面展示）。

## 六、API 写面（rp_server.py）

- 读面：GET /api/workflows（定义列表 + ?id= 全版本）、GET /api/workflow-runs（run 列表 + ?id= 详情：run 头行 + 节点行 + 控制行 awaiting 视图 + checkpoint seq + correlation 六 ID + trace_query 反查入口）；主库与 aux 双 schema 归并（platform*.db 停在 V1 无 016/017/018，守卫降级真实空态说明）。
- 写面：POST /api/workflow-runs（action=run，workflow_id 必填，run_id 可选钉定缺省预生成）、POST /api/workflow-runs/<run_id>/{pause,resume,retry}（或 `<action>` + body.run_id）；契约 = schemas/workflow/run-request-v1.schema.json（服务内 stdlib 手工核对同形状）。
- 承载方式：run/resume/retry 异步（Popen 不等待，stdout/stderr 落 /tmp/rp-workflow-run-logs/<action>-<run_id>.log，状态经 GET 轮询）；pause 同步（请求位写入即返回）。异步承载要求 run_id 预生成，故 rp-workflow run/retry 增 --run-id 钉定入口（start_with_id / retry_with_id）。
- 守卫：RP_WORKFLOW_DB 未配置 503、指向 platform*.db 403（镜像 CLI production_db_refused）、rp-workflow 二进制缺失 502、CLI 非零退出 502（stderr 尾部透传）。
- handle_traces 增 workflow_run 面：探测序前插 workflow_run_id/trace_id 反查（wfr- run id 即链锚），链面加 workflow_runs 臂（trace_id 或 run_id 双向命中），counts 加 workflow_runs。

## 七、CLI 增量（rp-workflow）

| 命令 | 作用 | exit |
| --- | --- | --- |
| `pause <run_id>` | 写 018 请求位 + 014 传播，返回 state_at_request | ok = 0 |
| `run <workflow_id> --run-id <id>` | run id 钉定（API 异步承载用） | 同既有（SUCCEEDED/paused = 0） |
| `resume <run_id> --input '<json>'` | 批准/注入参数（浅并入恢复态 + approval_json 留痕） | 同 run |
| `retry <run_id> --node <n> --run-id <new>` | 新 run id 钉定 | 同 run |

exit 口径更新：Paused（协作暂停 / awaiting_approval 停等）= 0（停等非错误）。status 输出增 `correlation`（六 ID）与 `awaiting_approval` 视图；nodes 输出增 tool_run_id / agent_run_id 实体链接列。

## 八、已知取舍

1. human 节点词表扩展走 017 批内补入而非新迁移号：SQLite 无法用新迁移改既有 CHECK 域，017 未入库提交且 dev 副本可重建，platform*.db 从未应用；若 017 已入库提交则需表重建迁移（未走，如实登记）。
2. 无批准参数的 resume 会重写介入请求再次停等（幂等语义）：节点 attempt 随重派发递增（1 → 2 → 3），ask 判据只约束已完成节点不变。
3. 暂停与节点级停等同时在场时，节点级暂停路径即消费用户请求位（防 stale 请求位让 resume 一波即回 paused，实测修复）；人工介入停等无用户请求时消费返回 false 无副作用。
4. API run/resume/retry 异步承载返回受理即回，受理拒绝（未知 workflow_id 等）只能从日志文件与「run 行从未出现」观测（不阻塞 HTTP），运维面以 GET 轮询 + 日志为准。
5. 六 ID 单列承载最近值：多实体场景的全集对账必须走 trace_id 链面（v_trace_chain / /api/traces），单列不承载 N 方向。
6. agent 节点 goal 的 `$/` 引用沿用 P8-2 整值引用口径：混合句内的 `$/` 片段保持字面（resolve_refs 只解析整体此前缀的字符串叶值）；缺省 goal = run state 紧凑 JSON。
7. propagate_pause_to_agents 按 trace_id 反查 RUNNING agent run：同 run 多 agent 节点并发（理论可达：parallel fan 禁入 agent，但多 agent 顺序节点只有前序完成后才派发后继）实际同时 RUNNING 至多一个，反查多行时全部受理（幂等安全）。
