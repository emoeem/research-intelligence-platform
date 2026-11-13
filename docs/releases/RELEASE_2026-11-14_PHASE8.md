# 更新说明 · 2026-11-14（Phase 8 阶段总账：Workflow Studio 与工作流执行引擎）

> Phase 8 全段（P8-1 到 P8-4）交付完成后的总账。用户可感知的变化：平台第一次有了可视化工作流画布（Workflow Studio 五区实页：拖节点、连线、保存版本化、发起运行、逐节点看状态与产物、暂停恢复与人工介入批准注入都在页面上完成）；工作流从静态 DSL 长成了能跑的引擎（Tool/Knowledge/Condition/Parallel/Agent/Human 六类执行器，checkpoint 每步落库、kill 后 resume 从断点续跑、失败节点 retry 只重跑自身、节点超时显式终态）；六 ID 关联（trace/task/agent_run/tool_run/artifact/evidence）把 workflow run 与 Phase 7 的 trace 面与 Agent Studio 打通。本文件是阶段总账，分批实施细节以各批 RELEASE（P8-1 到 P8-4 四份）为准，不在此重写。

| 项         | 值 |
| ---        | --- |
| 状态       | 生效中（Phase 8 阶段总账，阶段收口入库批次） |
| 日期       | 2026-11-14 |
| 适用版本   | git `7707d13`（WIP 备份，P8-1 到 P8-3 代码面）及之后，本收口提交补 P8-4 代码面与阶段总账 |
| 阶段交付   | 本收口提交（内容：P8-4 未提交工作树（web 改 8 文件 + 新增 6 组：画布组件/模型/测试/spec）+ P8-4 RELEASE + browser-e2e 两目录 11 截图 + 本总账 + docs 登记；git 短哈希按仓库回填惯例由后续 docs 批次补记，先例 `05ed5ee`/`f02d9b3`） |
| 数据库迁移 | 阶段累计 016_workflow_definition.sql（定义版本化）+ 017_workflow_run.sql（run/node/checkpoint 三表，CHECK 域含批内补入 human 第 8 词值）+ 018_workflow_control_correlation.sql（控制表 + run 六 ID 关联列，015 ALTER TABLE 先例同款加列）；各迁移在临时库与 migrations/dev-p8.db 开发库实测（migration_replay.rs 重放两次幂等口径）；生产 platform.db 与 platform-dryrun.db 全程未应用（入库当日复测 selftest 前后指纹一致，user_version 与逐表行数零变化） |
| 自动化测试 | `cargo test --offline --workspace`：582 passed 0 failed 0 ignored（入库当日复测 84 目标，exit 0；阶段演进 510（P7-7 基线）→ 536（P8-1）→ 568（P8-2）→ 582（P8-3）→ 582（P8-4 零 Rust 改动））；`python3 -m unittest discover -s workers/python/tests`：211 OK 跳过 11（入库当日复测，Phase 8 四批无 workers/python 改动）；web vitest 16 文件 111 passed（P7-7 基线 100 + workflows-smoke 11） |
| 静态检查   | `cargo clippy --offline --workspace --all-targets -- -D warnings` 0 error（入库当日复测 exit 0）；web typecheck/build/prettier --check 全过（入库当日复测）；视觉门 59 passed 38.2s（PW_CHROMIUM 指向用户缓存 Chrome for Testing 151.0.7922.34，axe color-contrast findings 0） |
| 相关       | docs/phase0/PHASE-ROADMAP.md §9、docs/RIP-2.0-PLATFORM-AND-TOOLING-PLAN.md §10.4、docs/SESSION-HANDOFF.md §三/§十一、分批 RELEASE 四份（见「一、」表）、设计稿三份（[phase8/WORKFLOW-DSL.md](../phase8/WORKFLOW-DSL.md)、[phase8/RUN-STATE-MACHINE.md](../phase8/RUN-STATE-MACHINE.md)、[phase8/WORKFLOW-CONTROL-PLANE.md](../phase8/WORKFLOW-CONTROL-PLANE.md)） |

## 一、批次 1 到 4 汇总

编排批次 P8-1 到 P8-4 四批，对应 ROADMAP §9.3 Phase 8 车道全部内容行。P8-1 到 P8-3 由活体工作流完成，编排层 2026-11-14 00:07 按配额停跑纪律落 WIP 备份提交 `7707d13`（吸收当时工作树：workflow-engine 全部代码、016 到 018 三迁移、schemas/workflow 两份契约、P8-1 到 P8-3 三份 RELEASE、设计稿三份、selftest 脚本与 P8-4 早期 web 改动），本提交为阶段收口入库批次（P8-4 终态工作树 + P8-4 RELEASE + 截图 + 总账 + 登记）。

| 批次 | 内容 | 关键交付 | 记录 |
| --- | --- | --- | --- |
| P8-1 | Workflow DSL 与定义版本化 | definition-v1 图 DSL（nodes/edges JSON）+ draft 2020-12 Schema 门 + 图静态校验纯函数（环/孤立节点/必填参数/非法边端点，一次收集全部问题，kind 词 machine 可 grep）+ 016 workflow_definition 版本化存储（(workflow_id, version) 复合主键，put 存储侧分配版本追加不可变，同版本 get 逐字节一致）+ rp-workflow CLI 五子命令（validate/put/get/list/import-legacy）+ domain::workflow 词表与跃迁表；存量 003 workflow 表 13 行只读实查为 RTIS 方法层定义，以 compat=legacy-method-reference 只读参考导入不扭曲 DSL | [RELEASE_2026-11-13_PHASE8-P8-1.md](RELEASE_2026-11-13_PHASE8-P8-1.md)（随 WIP `7707d13` 入库） |
| P8-2 | 执行引擎与非智能节点 + 运行状态持久化 | 017 三表（workflow_run 六态 + workflow_run_node 五态 attempt + workflow_checkpoint 每步快照只追加）+ Tool（tool-engine 派发链真实 python_worker 派发）/ Knowledge（context-broker Broker::assemble 装配）/ Condition（jsonschema 纯函数谓词）/ Parallel（fork-join 并发 join 收敛）四类执行器 + CLI run/status/nodes/resume/retry/cancel 六子命令 + selftest_phase8_workflow.sh 十段 46 断言 | [RELEASE_2026-11-13_PHASE8-P8-2.md](RELEASE_2026-11-13_PHASE8-P8-2.md)（随 WIP `7707d13` 入库） |
| P8-3 | 控制面与智能节点 + 六 ID 贯穿 + API 写面 | 018（workflow_run_control 1:1 控制表 + workflow_run 六 ID 关联列 + workflow_run_node 节点级 tool_run_id/agent_run_id 链接列）+ Agent 节点执行器（agent-engine AgentLoop 为执行体，014 协作暂停复用不重复实现控制面）+ human 人工介入节点（词表第 8 值，awaiting_approval 停等 + resume --input 批准注入）+ workflow 级协作暂停/取消传播（等完成再 checkpoint，取消三面留痕）+ CLI pause 子命令与 run/retry --run-id 钉定 + rp_server GET /api/workflows 与 /api/workflow-runs 只读面 + POST 白名单 run/pause/resume/retry 写面（一律 rp-workflow 子进程，本进程零直连 SQL 写）+ /api/traces workflow_run 双向 + run-request-v1 契约 | [RELEASE_2026-11-13_PHASE8-P8-3.md](RELEASE_2026-11-13_PHASE8-P8-3.md)（随 WIP `7707d13` 入库） |
| P8-4 | Workflow Studio 画布页五区壳层 | @xyflow/react 12.12.1（npm 实查 latest，exact 锁定，只进 web/，vite 独立 vendor-xyflow 分组，vendor-react 共享组零变化）+ WorkflowsPage 占位换实页五区（NodePalette 八类词表单源/FlowCanvas 自定义节点与 branch 边标签/NodeInspector 参数 Schema 表单与运行配置/RunPanel 执行日志与产物/VersionList 版本与历史回看）+ 画布模型纯函数 web/src/lib/workflow-graph.ts（互转/确定性分层布局/保存装配/预检）+ 画布保存 POST /api/workflows put（definition 落临时文件 rp-workflow put 同步子进程四道门，exit 1=422 exit 2=400）+ 运行/暂停/恢复/重试接 P8-3 API（活动 run 2s 轮询 + 动作后 8s 脉冲轮询 + 无行有界补拉，实测发现两处轮询缺陷已修并回归单测）+ main.tsx Bearer 令牌 localStorage 最小桥 + playwright.config 机器无关浏览器三级探测 + workflows-smoke 11 用例 + 视觉门 /workflows 专项 4 用例 | [RELEASE_2026-11-13_PHASE8-P8-4.md](RELEASE_2026-11-13_PHASE8-P8-4.md)（本收口提交入库） |

## 二、五道门禁数字（批次演进与入库当日复测）

门禁口径沿用 Phase 5 到 7 收口惯例五组：cargo test、clippy、worker unittest、web 门禁（typecheck/build/vitest/视觉门/prettier）、selftest。全仓 cargo test 批次基线演进：510（P7-7）→ 536（P8-1）→ 568（P8-2）→ 582（P8-3）→ 582（P8-4，本批零 Rust 改动沿用基线）。worker unittest 在 Phase 8 四批口径未列（零 workers/python 改动），入库当日复测补跑归零风险。web 门禁 P8-1 到 P8-3 无触点（P8-3 RELEASE 已知取舍第 7 条明示「web/ 未改动」），P8-4 起纳入批次口径。

| 门禁 | 批次时点数字（来源：各批 RELEASE 元信息表） | 入库当日复测（2026-11-14 本机实测） |
| --- | --- | --- |
| 1. cargo test --offline --workspace | 536/568/582/582（P8-1 至 P8-4 逐批） | 582 passed 0 failed 0 ignored（84 目标），exit 0 |
| 2. cargo clippy -D warnings | 各批 0 error（P8-1 至 P8-4 逐批） | exit 0，0 error |
| 3. worker unittest | Phase 8 四批未列口径（零 workers/python 改动，P7 收口基线 211） | Ran 211，OK (skipped=11)，exit 0 |
| 4. web 门禁（typecheck/build/vitest/视觉门/prettier） | P8-4：typecheck+build 过；vitest 111 passed（基线 100 + workflows-smoke 11，两连跑）；视觉门 59 passed（基线 55 + /workflows 专项 4）；prettier 全过；vendor-xyflow 183.40 kB（gzip 59.39）独立 chunk，vendor-react 256.51 kB（gzip 81.42）零变化 | typecheck exit 0；build exit 0（vendor-xyflow 183.40 kB gzip 59.39 / vendor-react 256.51 kB gzip 81.42，与 P8-4 登记逐数一致）；vitest 16 文件 111 passed exit 0；prettier --check 全过 exit 0；视觉门 59 passed（38.2s），axe color-contrast findings 0 处 |
| 5. selftest_phase8_workflow.sh | 46/46（P8-2）→ 76/76（P8-3）；P8-4 零 Rust 触点未复跑（其 RELEASE 如实登记） | PASS=76 FAIL=0 exit 0（含生产 platform.db 与 platform-dryrun.db 指纹前后一致零写复核） |

复测命令原样（仓库根执行，可复制）：

```bash
cargo test --offline --workspace
cargo clippy --offline --workspace --all-targets -- -D warnings
python3 -m unittest discover -s workers/python/tests
npm --prefix web run typecheck && npm --prefix web run build && npm --prefix web run test && npm --prefix web run format:check
cd web && PW_CHROMIUM=$HOME/.cache/ms-playwright/chromium-1234/chrome-linux64/chrome npm run test:visual:gate && cd ..
bash apps/server/selftest_phase8_workflow.sh
```

浏览器二进制机器无关口径：本机实测 `command -v chromium chromium-browser google-chrome-stable` 无命中、/usr/sbin/chromium 不存在、用户缓存 ~/.cache/ms-playwright/chromium-1234/chrome-linux64/chrome 在场（--version 输出 Google Chrome for Testing 151.0.7922.34）；复测以 PW_CHROMIUM 环境变量显式注入。playwright.config.ts（P8-4 改）已内置三级探测（PW_CHROMIUM 环境变量优先 → 用户缓存 chromium-* 目录探测 → /usr/sbin/chromium 回退），换机时先探测本机可用浏览器再注入等价变量，缺浏览器用用户级缓存方案补齐（如 `npx playwright install chromium`，只写用户缓存目录），绝不装系统级依赖。

## 三、画布与 checkpoint/恢复说明（Phase 8 的执行语义收口）

1. **画布模型（定义与视图分离）**：definition-v1 不存坐标，画布布局为确定性推导（最长路分层 + 层内 id 字典序，web/src/lib/workflow-graph.ts layeredPositions），同图恒同布局，历史回看不跳动；graphToFlow/flowToGraph 往返 branch 词形与 when 载荷保真，指向缺失节点的边丢弃；保存装配 buildDefinition（schema/workflow_id/version 声明/compat=graph-v1/input_schema/graph）+ precheckIssues 客户端预检（非空图/恰一 start/至少一 end，与 workflow-engine validate 口径一致，最终以 rp-workflow put 四道门为准）。画布引擎 @xyflow/react 独立 vendor-xyflow 分组仅 /workflows 懒加载消费，vendor-react 共享组体积零变化（开工前探针与终局两次实测，见 P8-4 RELEASE §一.6）。
2. **checkpoint 语义（LangGraph/Temporal 思路的持久化执行）**：节点到达终态时向 workflow_checkpoint 写全量投影快照（seq 自增只追加，非增量事件），恢复 = MAX(seq) 一行整体回放（restore_from_snapshot），恢复路径不重放事件序列、不需要幂等去重；只追加表无 UPDATE 路径，对账形状与快照恒一致（crates/workflow-engine/src/state.rs）。
3. **恢复面四条路径**：resume 从 checkpoint 续跑，已完成节点 attempt 不变不重跑（kill 断点恢复 selftest §7 与集成测试 SplitDriver 确定性 kill 点双覆盖）；retry 以新 run 承载只重跑失败节点，attempt 在重跑进入 running 时递增（1 → 2 实测，冻结跃迁表 failed 终态不改动）；节点超时是显式 failed 终态（code=node_timeout，看门狗 recv_timeout，残留线程写入被 WHERE state='running' 乐观守卫静默丢弃）；协作暂停在波间边界消费（等当前节点完成并落 checkpoint 后下一波转 paused，run 实际完成时不吃暂停请求），agent 节点暂停经 014 控制行传播到 agent-engine，human 节点停在 awaiting_approval 由 resume --input 注入批准参数（approval_json 留痕）。
4. **Web 消费面**：RunPanel 呈现 017 节点行五态与 attempt、018 控制行 awaiting_approval/PAUSE-REQUESTED 视图、六 ID 关联链接（agent_run/tool_run/artifact/evidence 可点跳转）、checkpoint seq 与节点产物折叠块；活动 run 2s 轮询 + 动作受理后 8s 脉冲轮询 + run 行落库晚于受理响应的无行有界补拉（800ms 最多 10 次），后两项为浏览器实测发现的两处轮询缺陷修复（均回归单测，见 P8-4 RELEASE §三.7）。

## 四、浏览器实测截图路径引用（真实 UI 操作证据链）

两个截图目录构成阶段级浏览器实测证据链（均为真实 UI 操作、真实后端链路，各 README 内附逐截图对账与管线适配声明）：

| 目录 | 性质 | 内容 |
| --- | --- | --- |
| [phase8/browser-e2e/01/](../phase8/browser-e2e/01/README.md) | P8-4 批次自验（browser-use 六步 E2E） | 6 截图：拖拽节点（DragEvent 事件链落点）/ 连线（真实指针柄拖拽四条边）/ 保存（wf-p84-e2e v1/v2/v3 三连保存版本递增落库对账）/ 运行（v1 图 human 节点停等 awaiting_approval，018 控制行 + inject 框同屏）/ 暂停（tool 链 RUNNING 窗口点击，波间消费停在 checkpoint seq 12，PAUSE-REQUESTED 徽标瞬态捕获）/ 恢复（续跑至 succeeded seq 20，节点 attempt 保持 1，三条 tool_run 链接）；真实 rp_server（7801，鉴权开启）+ vite proxy + dev-p8.db，tool 节点经 tool-engine + python_worker 真实派发，agent 节点经 agent-engine 真实闭环（arn-f44530d00067e7301a54ee1f + artifact/evidence 六 ID） |
| [phase8/browser-e2e/02/](../phase8/browser-e2e/02/README.md) | 验收员独立复测（独立于实施批次） | 5 截图：五区壳层初始态 / 四节点三边画布 / 保存回显（wf-p8acc-e2e v1，4 节点 3 边，source=rp-workflow-put 对账）/ 运行终态（run wfr-5ba2fdc0033e47bb93d5dbf8c8390c1b succeeded，checkpoint seq 24，逐节点终态 + 节点产物）/ 版本历史只读回看（v1 chip + 两处只读徽标 + 该版 graph 完整渲染）；全新 workflow 从空白画布完整走「建图、保存、运行、回看」闭环，trace 三向可达（matched_via workflow_run_id/tool_run_id/agent_run_id），与 01/ 互补互为独立证据 |

两处管线适配声明（Chromium 原生限制，非伪造操作，README 声明节原样登记）：面板拖入画布的 HTML5 dragstart 在 In-app Browser 指针管线不触发，以真实 DragEvent 事件链（DataTransfer 真实写入 + dragover + drop 落点坐标）完成同一拖拽路径；连线柄拖拽在多点多路径下不稳定或持续超时，01/ 以紧凑五点路径真实指针拖拽完成四条边，02/ 按 @xyflow/system 真实事件契约（mousedown/mousemove/mouseup）以真实事件链完成三条连线，aria-label 逐一核对。

## 五、已知取舍

1. **面板「数据转换 / 证据保存」分类未落**：计划 §10.4 G 七类节点中该两类暂无 DSL 词值（workflow-engine 冻结词表八类不含），面板不伪造词表外节点项（NODE_CATALOG 注记见 web/src/lib/workflow-graph.ts:4 到 5，DSL 扩词后追加），词表单源纪律优先于页面分类完整性。
2. **画布坐标不入 definition**：布局确定性推导，保存/回看往返图结构逐字节保真而坐标不保真（坐标仅画布态）；需要固定布局时按 id 命名序稳定推导。
3. **human 词表扩展走 017 批内补入**（非新迁移号）：SQLite 无法以新迁移改既有 CHECK 域；该取舍成立于 017 未入库提交时点（随 WIP `7707d13` 一次性入库），生产 platform*.db 从未应用 016/017/018，无已发布库漂移面；词表仍单源（domain），CHECK 域与 Schema enum 同批同值。
4. **retry 以新 run 承载 + 暂停请求位 consume-on-observe**：冻结跃迁表 failed 是终态不改动，retry 对账按 CLI 输出的新 run_id；pause_requested_at 波间消费即清空，暂停后最终态不保留请求位痕迹（E2E 徽标文本瞬态捕获于操作日志，截图以暂停后的 run/节点边界状态为准）。
5. **P8-1 到 P8-3 无独立批次提交**：代码随编排层 WIP 备份 `7707d13` 一次性入库（2026-11-14 00:07 配额停跑纪律收口点），批次 RELEASE 元信息「阶段交付」短哈希按仓库回填惯例以本收口后的独立 docs 批次补记（先例 `05ed5ee`/`f02d9b3`）。
6. **API run/resume/retry 异步承载**：受理即回，受理拒绝（未知 workflow_id 等）只能从日志文件与「run 行从未出现」观测；运维面以 GET 轮询 + /tmp/rp-workflow-run-logs 日志为准（P8-3 RELEASE §三.2 原样沿用）。
7. **六 ID 单列承载最近值**：1:N 全集经 trace_id 查 v_trace_chain 与 /api/traces（015 trace 面主通道），workflow_run 单列不承载 N 方向。
8. **随收口提交一并入库的非 Phase 8 面改动**：.gitignore 增 integrations/rtis/pocs/（rt.py POCS 运行时落盘不入库，说明注释在 .gitignore:29 到 32，活体工作流运行期间遗留改动，非 Phase 8 批次产物）。
9. **收尾审阅的全工作区 browser-use 复验仍待执行**：SESSION-HANDOFF §十一既有待办（Phase 8 范围外，归收尾审阅批次）；Phase 8 页面自身证据为 01/ 与 02/ 两目录截图 + 视觉门 59 用例。

## 六、回滚方式

1. **本收口提交**（P8-4 web 面 + P8-4 RELEASE + 截图 + 总账 + 登记）：`git revert <本提交哈希>` 即回 WIP `7707d13` 态；revert 后 web/ 需 `npm run build` 重建 dist（构建产物不入库），@xyflow/react 依赖随 package.json/package-lock 回退后需 `npm install`。
2. **P8-1 到 P8-3 代码按批次倒序回退**（与 `7707d13` 绑定，单独 revert 该 WIP 提交影响面大，不建议；需回退时按文件粒度）：P8-3 面（crates/workflow-engine/src/control.rs、exec/node_agent.rs、exec/node_human.rs、migrations/018、rp_server API 写面、tests/p8_3_control.rs）→ P8-2 面（exec/mod.rs 与 node_tool/node_knowledge/node_condition/node_parallel、state.rs、migrations/017、tests/run_execution.rs、selftest 脚本）→ P8-1 面（crates/workflow-engine 其余、crates/domain/src/workflow.rs、migrations/016、schemas/workflow/、链尾断言顺延的既有测试文件）；全量回退（7707d13 + 本收口提交）即回 P7-7 基线 102cab5 态。
3. **迁移回退**：016/017 为纯新增结构（017 CHECK 域含 human 第 8 词值），018 为加列迁移（015 ALTER TABLE 先例同款）；开发库副本（dev-p8.db 与 /tmp 临时库）可删重建，物理回退 = 删迁移文件 + DROP 对应表与列 + 清 schema_version 行 + user_version 回退；生产 platform.db 与 platform-dryrun.db 从未应用 016/017/018（selftest 指纹复核 user_version 与逐表行数前后一致），无生产回滚面。
4. **生产库零写红线**：入库当日复测 selftest 76 断言含生产双库指纹前后一致核对（user_version + 逐表行数），Phase 8 各批同一口径；workflow_run 三表不承载生产数据，无数据迁移负担。
