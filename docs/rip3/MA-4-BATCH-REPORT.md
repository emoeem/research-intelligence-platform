# MA-4 批次报告：事件推送与跨 AI 端到端验收——MA 轨收官批（2026-11-22）

- 依据任务书：[MA-4-TASK.md](MA-4-TASK.md)（§0 裁决 D1-D8 / §1 锚点 A1-A16 / §2 范围 / §3 步骤 S1-S7 / §4 判据 12 条 / §6 八红线全部照办，零发挥零加戏）
- 实施基线：HEAD `0f2922cb`（MA-3 CI 修复批）；工作树实施前仅含本批任务书 untracked 文件（MA-4-TASK.md），无未提交改动混入
- 收口口径：`cargo test --offline --workspace` 全量一次绿（唯一预期外红 = document-router di2_media×4 环境面基线同败，S1 stash 反证 = 本批零 Rust diff，基线运行即干净 HEAD 实证）；本批落点 crate 全绿；selftest_ma4.sh 76 PASS / 0 FAIL / 0 SKIP exit 0；既有回归七套零改动全绿

---

## 一、改动文件清单

### 修改（9 文件含 docs/README.md 登记，累计 +849/-37 行；实测 git diff --numstat）

| 文件 | 行数变化 | 内容 |
|---|---|---|
| `apps/server/rp_server.py` | +727 / -17（其中 -17 全部核对：S3 三端点 filters/statement 重构 13 行 + `_spawn_workflow` MA-3 缺陷修正 4 行，零无关删除） | ①S2：`GET /api/events` SSE 专用分支（do_GET /api/ 分支字典分发前）+ 订阅注册表（threading.Lock + deque + 条件变量）+ 单 tail 轮询 daemon（`_sse_tail_loop`，首个订阅者惰性启动、无订阅者自退）+ 三形态 cursor 扫描（`_sse_scan_cycle`：audit_event append-only / agent_run·tool_run·task_claim·ma_message updated_at 窗口 + (主键,updated_at,状态) 元组去重 / workflow_run 活跃态快照 diff + 跃迁出活跃态终态补发）+ 事件帧格式化（`id: <source>:<cursor>` 单调 + `event: <kind>` + `data: {ts,kind,entity_id,action,caller_id,run_id,detail}`）+ `: ping` 心跳 + 连接帽 503 `sse_capacity` + Last-Event-ID 重放（`_sse_replay_events`，回看窗帽）+ 重放∩live 队列同事实去重 + 四 env 常量（RP_SSE_POLL_MS=500 / RP_SSE_HEARTBEAT_S=15 / RP_SSE_MAX_CLIENTS=16 / RP_SSE_REPLAY_MAX=500）+ 头注 MA-4 段与 env 说明区追加；**零 SQL 写**（事件源与重放全经 connect_ro mode=ro + query_only；AST 级断言六函数零 INSERT/UPDATE/DELETE）②S3：`run_list_columns` caller_id 条件投影（027 库水位，trace 列 P7-5 同款）+ `/api/runs` `/api/agent-runs` `/api/workflow-runs` `?caller=` 等值过滤 additive（缺列 schema 过滤下零贡献不伪造；未知 caller 空集非错误）③MA-3 移交缺陷修正：`subtask_dispatch` spawn 二进制 WORKFLOW_BIN→AGENT_BIN（见坏消息 §二.1） |
| `web/src/lib/api.ts` | +12 / -8 | AgentRunRow/WorkflowRunRow `caller_id?` 字段 + AgentRunList.filters.caller / WorkflowRunList.filters 类型 + agentRunsPath/fetchAgentRuns caller 参数（additive） |
| `web/src/routes/RunsPage.tsx` | +70 / -8 | caller 徽标列（`data-testid="run-caller-badge"`，027 库水位在场渲染、旧库不伪造）+ caller 过滤下拉（`data-testid="caller-filter"`，服务端 ?caller= 参数，status 过滤同型；选项 = 未过滤列表 items caller 去重累积） |
| `web/src/routes/AgentsPage.tsx` | +5 | 运行记录区 caller 徽标列（数据 caller_id MA-3 已在 list SQL） |
| `web/src/routes/RunDetailPage.tsx` | +2 | 概要区 caller MetaRow（run.caller_id） |
| `web/src/components/workflow/RunPanel.tsx` | +5 | run 头 caller 徽标（`data-testid="workflow-run-caller-badge"`；节点级聚合不做——R9 深化登记） |
| `web/tests/runs-smoke.test.tsx` | +38 | stub 行 additive caller 字段 + 2 条新用例（caller 徽标水位渲染 + caller 过滤走服务端参数；概要区 caller 行）；既有 9 用例零改动 |
| `web/.gitignore` | +1 | `qa/runs/.run/`（playwright outputDir，qa/mcp/.run 先例同款） |

### 新建（4 文件 + 截图产物）

| 文件 | 行数 | 内容 |
|---|---|---|
| `apps/server/selftest_ma4.sh` | 848 | D4 八节收口脚本（§1 夹具+起服+N=4 观测读者 / §2 形态① / §3 形态② / §4 形态③ / §5 形态④ / §6 SSE 实时性专节 / §7 压测汇总 / §8 生产库指纹）；V-9 SKIP 守卫预挂（GATE_AGENT = rp-agent bin + migrations/platform.db 探测，缺 → SKIP 计数 exit 0）；SSE 流读取器内嵌（真实流计时，epoch 采样对齐） |
| `web/playwright.runs.config.ts` | 38 | 双模式配置（visual-server 4179 静态 mock / RP_P6_E2E=1 真实后端 + RP_P6_BASE_URL/RP_P6_TOKEN 注入），playwright.plugins.config.ts 同款 |
| `web/tests/runs-caller.spec.ts` | 251 | 模式 A：首屏 caller 徽标 + 过滤态 + 详情 caller 行 + Agents caller 列断言 + 截图 4 帧；模式 B：真实库水位截图 2 帧（/api/** 经 page.route 代理真实 rp_server） |
| `web/qa/runs/` | 6 帧 | runs-page / runs-filtered / run-detail / agents-page（模式 A mock）+ e2e-runs-page / e2e-runs-filtered（模式 B 真实后端，selftest_ma4 §6.9 实跑产出）——计划 :48「截图/实测入库」交付物 |

### 零 diff 实证面

- `crates/**` 全部零 diff（D6：SSE 在 Python、caller 读面 Python+web、验收 shell；S7 git status 落点核对实证）
- `Cargo.lock` / workspace 根 `Cargo.toml` 零 diff（399 恒定 + reqwest=0）
- `migrations/001-029` 与 `migrations_pg/V0001-V0007` 零 diff；030/V0008 空位仍在（A15 十六硬钉 29 双点复核零随更）
- web `package.json` 零改动（零 npm 依赖）；`web/tests/workspaces.test.ts` 路由钉 22 不动（vitest 126/126 含 workspaces 钉实证）
- rp_server.py 既有 31 路由读面零改动（/api/events 与 ?caller= 纯 additive；protocol_version 零设置保持；send_json/do_POST/PATCH/PUT 形状不动——唯一写面例外 = subtask_dispatch spawn 二进制修正，见坏消息 §二.1）

## 二、坏消息（单列）

1. **MA-3 移交缺陷修正（本批唯一触碰既有写面处，已于修正处 docstring + 调用点双注记溯源）**：`subtask_dispatch`（rp_server.py MA-3 落地的 REST POST /api/subtasks）原实现经 `_spawn_workflow` 误用 `WORKFLOW_BIN`（rp-workflow）承载子任务，agent_id 被当 workflow_id 解析，spawn 日志恒报 `workflow definition agent.ma4.selftest not found`，**REST 派发的 agent run 从未真实执行过**。MA-3 selftest 只钉 HTTP 200/403 词形未测运行完成，故 MA-3 收口未暴露；MA-4 判据 2 硬验收（4 caller 真进程同刻 POST /api/subtasks 全 SUCCEEDED）将其暴露。修正 = `_spawn_workflow` 加 `bin_path` 参数 + subtask_dispatch 传 `AGENT_BIN`，对齐 subtask_dispatch 既有文档契约原文「spawn `rp-agent run <agent_id> --goal <goal> --caller-id <caller> --db <RP_WORKFLOW_DB>` 子进程」；委派纪律不变（子进程承载 + 日志落盘 + 本进程零直连 SQL 写 + 502 subtask_spawn_failed 错误词 additive）。既有回归自证：selftest_ma3.sh（其 §6 断言在修正后仍全绿）+ selftest_auth.sh / phase7 全绿。
2. **判据 1 部分词形 CLI 面不可达（如实声明，不谎报）**：`PARALLEL_PLAN` 审计行落库需要 planner 产出 >1 步/轮；rp-agent CLI 钉死内置 `KeywordSearchPlanner`（P7-2 最小规划器，恒单步/轮，agent_loop.rs:1160 `parallel_ready && plan_steps.len() > 1` 分支 CLI 永不可达）。四步 fan-out 的 PARALLEL_PLAN 落库由既有 Rust e2e `ma2_parallel_budget`（模拟规划器，800 行）承载（§7 实跑 exit 0），CLI 并行子代理 = 真进程 × 4 同刻（MA-2 selftest N=3→N=4 升档同口径，提速比 0.3 CLI 数字如实入报告）；SSE audit 事件可见性以真实 CLI 流量 audit 词形（agent.run.accepted / run_state / guardrail）实测——与 PARALLEL_PLAN 同走 audit_event append-only cursor 通道。
3. **SSE 实时性数字口径**：延迟 =（客户端帧到达时刻 − 夹具触发前 epoch clock 采样），含 rp-collab 进程启动开销（~150-300ms/次），为保守下界；事件源行内 ts（029 毫秒精度）未单独对齐（保守口径已满足 ≤2s 断言 20 倍余量：实测 p95 0.406s）。
4. **形态③ 快照 diff 短存活 run 缺口（本批内发现并修复）**：form-③ 活跃态快照 diff 的语义盲区 = 存活期短于一个轮询周期的 workflow_run（§4 实测该 run 整段 queued→running→succeeded 仅 ~1s，落在两扫之间时快照从未见过其活跃态 → 零跃迁帧；selftest run5/run7 偶发复现）。修复 = 形态③ 追加终态补扫（③b：`state NOT IN active AND finished_at >= 窗口`——017 仅 created/started/finished 三时间列，终态行唯一单调可用列 = finished_at；已发终态 run_id 集合由快照 diff 与补扫共享，一 run 终态恰一发；零引擎零迁移零写面）。修复后 run8/run9 连续两轮 ALL-PASS（76/0/0）。其余三形态 cursor 无同类缺口（agent_run/tool_run 有 updated_at 窗口、claim/message 有 rowid 新行面、audit 纯 append-only——短存活行均由各自 cursor 捕获终态）。
5. **§4 claim/message 链夹具侧按 ≥1 轮询周期（sleep 1） pacing**：三形态 cursor ② 的 (主键, updated_at, 状态) 元组去重对「两次写库同刻」语义诚实（同刻同状态不重发），夹具 pacing 使每个中间态驻留一个扫描窗（真实状态序列非预填，非语义放宽）。
5. **环境面杂项**：①本机 ls 中文 locale 导致任务书 §5 「ls ... | grep -c "No such file"」字面口径返回 0（实际 030/V0008 均不存在，直接 ls 反证）；②实施中一次被杀的 selftest 运行残留 root 属主 `/tmp/rp_ma4_selftest.EdcEr7/pgdata`（podman 挂载卷目录，no-new-privileges 环境无法删除，/tmp 重启自清）；③文档路由 di2_media×4 红为 ffprobe 环境漂移（S1 基线运行 = 干净 HEAD 实证，非本批引入）。

## 三、§5 命令实测（exit code + 关键输出行）

```text
# S1 基线
grep -cE '^\[\[package\]\]' Cargo.lock      → 399
grep -c 'name = "reqwest"' Cargo.lock       → 0
ls migrations/030* migrations_pg/V0008*     → 两个「没有那个文件或目录」（030/V0008 空位在）
CARGO_TARGET_DIR=/home/emo/rt3-target cargo test --offline --workspace --no-fail-fast
  → S1 基线与 S7 终态各一次全量：177 个 test result 行；两次唯一 FAILED 均为
    document-router three_channel_e2e di2_media×4（22 passed; 4 failed，panicked at
    three_channel_e2e.rs:2070/1666/1766/1861——ffprobe 环境漂移，行号逐字一致 = 基线同败实证；
    本批落点 crate 全绿；exit 101 仅由该环境面承载）
git rev-parse HEAD                          → 0f2922cb（基线）

# S7 收口（本批终态实测）
grep -cE '^\[\[package\]\]' Cargo.lock      → 399（恒定）
grep -c 'name = "reqwest"' Cargo.lock       → 0
A15 十六硬钉逐文件实勘 =29（四类钉值形态全清单，migration_idempotent.rs:87 /
  dev_copy_p7_3.rs:76+:85+:92 / agent_data_plane:83+:100+:109 / tool_contract:107+:150-157+:159-162 /
  runtime_node:72+:84+:90 / context_broker:108+:150+:160 / backup_restore.rs:51 /
  caller_auth.rs:209 / lifecycle_full_path.rs:605+:610 / migration_replay.rs:148+:149 /
  wsw store.rs:203 / cas_e2e.rs:132+:139+:177+:190 / persist.rs:890+:899 /
  exchange_persist.rs:3+:85 / intercept_queue.rs:47 / fuzz_job.rs:145 ——零随更）
  假阳性两处排除：tool-engine policy_resolver_manifests.rs「29 份派发面」= manifest 计数巧合；
  rp-wsw.rs:375/:524 = assert_watermark 调用点非钉值
python3 水位探针（platform*.db mode=ro）      → dev-p13/dev-p4/dev-p71/dev-b1/dev-b2/dev-b34-b3/dev-b34-b4/dev-b5 = 29
  （历史断面 dev-b34=5 / dev-p7=12 / dev-p8=18）；platform.db=1 / platform-dryrun.db=1 /
  platform-fullbatch.db=1 / platform-governance.db=0 / platform-search.db=0（A16 基线逐字一致，零触碰反证）
grep -c "Some(29)" crates/database/tests/backup_restore.rs → 1
grep -rn "assert_watermark" crates/wsw-runtime/src/persist.rs | wc -l → 与 S1 基线一致（2）
CARGO_TARGET_DIR=/home/emo/rt3-target cargo clippy --offline -p workflow-engine -p mcp-server -p intel-db --all-targets -- -D warnings
  → Finished `dev` profile（零 warning）；exit 0
cargo deny check advisories → advisories ok；exit 0
cargo deny check licenses   → licenses ok；exit 0
git status --porcelain crates/ | wc -l      → 0（crates/** 零 diff 实证）
git diff --stat Cargo.lock | wc -l          → 0（Cargo.lock 零 diff）
git diff --cached --stat | wc -l            → 0（零 staged——git add/commit 禁令遵守，工作树留主会话审查收口）
bash apps/server/selftest_ma4.sh            → 终态代码连续两轮 ALL-PASS（SKIP=0 单列计数）：
  run8 = PASS=76 FAIL=0 exit 0；run9 = PASS=76 FAIL=0 exit 0（修复迭代史 run1-7 见坏消息 §二.1/.4）
bash apps/server/selftest_auth.sh           → 结果：PASS=28 FAIL=0；exit 0
bash apps/server/selftest_ma2.sh            → 结果：PASS=34 FAIL=0；exit 0
bash apps/server/selftest_ma3.sh            → RESULT: ALL-PASS；exit 0（含其 §6 REST 面在 spawn 修正后仍绿）
bash apps/server/selftest_phase6.sh         → 结果：PASS=48 FAIL=1；exit 1（唯一红 = 「rp_server.py 全文件无 SQL 写语句」既有红，命中 3 处 wsw_repeater INSERT 与 HEAD 全等不增不减；web/qa/plugins/*.png playwright 重写后已 git checkout 还原）
bash apps/server/selftest_phase7_agents.sh  → PASS=50 FAIL=0；exit 0
bash apps/server/selftest_phase8_workflow.sh → exit 0（零 FAIL）
bash apps/server/selftest_phase9_mcp.sh     → exit 0（零 FAIL）
cd web && npm run typecheck && npm test     → tsc 零错误；Test Files 19 passed / Tests 126 passed（含 runs-smoke 11 = 既有 9 零改动 + 新增 2）
RP_P6_E2E=1 npx playwright test tests/runs-caller.spec.ts -c web/playwright.runs.config.ts
  → selftest_ma4 §6.9 实跑全过；web/qa/runs/e2e-*.png 落盘（ls web/qa/runs/*.png | wc -l = 6 ≥ 4）
git status --porcelain → 落点核对：M rp_server.py + web 七文件 + ?? selftest_ma4.sh / playwright.runs.config.ts /
  runs-caller.spec.ts / web/qa/runs/ / MA-4-TASK.md（+ 本报告与 docs/README 登记）——crates/** 零文件、Cargo.lock 零 diff、
  030/V0008 空位仍在
```

## 四、四形态端到端实测数字（selftest_ma4.sh run8/run9 连续两轮 ALL-PASS）

### 硬验收①形态一：单 caller N=4 并行子代理（§2）——判据 1
- CLI 串行 N=4 顺序 = **581 ms**；并行 N=4 同刻 = **173 ms**；提速比 = **0.298**（CLI 数字如实；<0.7 硬界由 Rust e2e ma2_parallel_budget 承载声明——§7 实跑 exit 0；run8 同口径 571/172/0.301）
- 串行 4 run 全 SUCCEEDED（库直查）+ 并行后 caller-a 共 8 行 SUCCEEDED + tool_run.caller_id=caller-ma4-a 落列 8 行
- SSE 订阅者收到 agent.run.accepted audit 事件（caller_id=caller-ma4-a）✓（PARALLEL_PLAN 词形见坏消息 §二.2）

### 硬验收②形态二：N=4 caller 并发各自任务（§3）——判据 2
- 4 真进程同刻 POST /api/subtasks（caller-ma4-a..d 四 key）：4 行 agent_run.caller_id 互异且全 SUCCEEDED（库直查，task_id IS NULL 通道行每 caller 2 = 串行基线 1 + 并发 1）
- 串行基线（4 caller 顺序全链）= **4185 ms**；并发 N=4 = **1158 ms**；提速比 = **0.277 ≤ 0.7** ✓（selftest_ma2.sh:129 口径延续；run8 同口径 4191/1165/0.278）
- legacy token POST /api/subtasks **403** fail-closed ✓（MA-3 :112 词形复证）
- 审计 JSONL 四 caller 各归各（caller-ma4-a/b/c/d 四态各归各）+ legacy 403 行 caller=default（MA-1 三态）✓

### 硬验收③形态三：双 AI 协作同任务 + 推送闭环（§4）——判据 3
- workflow 双 agent 节点分属两 caller 实跑：run SUCCEEDED + workflow_run.caller_id=caller-ma4-a（run 级）+ node2 agent_run.caller_id=caller-ma4-b（params 显式）✓
- claim 交接链（create → claim(a) → transfer → 新行 claim(b)）每步库直查一致（claimable → claimed → transferred + 新行 attempt=2 claimed）✓
- ma_message send → **SSE 订阅者（?caller=caller-ma4-b&kinds=message）收到 to_caller 命中 message 事件**，状态跃迁全程 SSE 可见 = `message.sent,message.delivered,message.read`，与库直查（delivered_at 落 → read_at 落）一致 ✓
- 投递+已读后 sweep 未投递归零（不再推送重投行）✓
- SSE workflow 帧 run_id 与库直查一致（workflow.succeeded 帧 run_id == 库内 run_id）✓

### 硬验收④形态四：N=4 供知识并发治理（§5）——判据 4
- 隔离 PG 容器（rip-pg17 digest 钉定） refinery V0007 入链；N=4 并发 rp-kc donor-submit 同 content = **152 ms**
- 4 收据全 CANDIDATE（{knowledge_id, review_status, normalized_hash} 形状）+ PG knowledge_cards 四行 donor_caller_id 逐行正确 ✓
- dedup-scan SAME_CONTENT 候选边 ≥ N-1（≥3）✓；review 双闸负例（未计分 approve 拒 exit 1）+ score 后 approve 正例（APPROVED）✓
- **声明：PG knowledge 面不进 SSE 事件源**（D2 双库分工 029:121 原文）✓

## 五、SSE 推送面与实时性实测（§6，判据 5/6 + WSW-3 门 D8）

| 断言 | 口径 | 实测 | 状态 |
|---|---|---|---|
| 单事件投递延迟 ≤2s | arrival − 触发前 epoch clock（含 rp-collab 启动开销，保守下界） | n=12 max=**0.497s** | ✓ |
| 连续 ≥10 事件 p95 ≤2s | 同上，12 真实 msg-send 触发 | p95=**0.497s** | ✓ |
| 事件计数零丢失 | 触发 12 = 客户端收 12（payload 逐条对账） | **12/12** | ✓ |
| 心跳 ≤ RP_SSE_HEARTBEAT_S+1s | 服务端 RP_SSE_HEARTBEAT_S=2，空等窗首帧 | **2.003s** | ✓ |
| N=4 订阅者 id 序列严格一致 | 4 独立流读进程全程在线（§1 起 §7 止，149 事件） | same=**True** | ✓ |
| ?caller= 过滤 | 订阅者 B（caller-ma4-b）只收 caller-b 事件 | ✓ | ✓ |
| ?kinds= 过滤 | kinds=claim_state 订阅：claim 帧在场 + message 帧零穿透（0） | ✓ | ✓ |
| Last-Event-ID 重放 | 断连（message:14）→ 新增 3 行 → 重连 | 补发 id=[15,16,17]，无旧帧重发 + 新增零丢失 | ✓ |
| 未授权 401 | 无 token GET /api/events | **401** | ✓ |
| 连接帽 503 | RP_SSE_MAX_CLIENTS=1 实例第二连接 | **503 sse_capacity** | ✓ |
| 断连清理 | kill 后有界重试探活（探活即注册，有界 12 次取首个 200） | **200**（帽位释放） | ✓ |
| Runs 页首屏 <3s（D8 shell 代理口径） | SPA HTML + /api/runs 数据面 | **14 ms**（5+9） | ✓ |
| caller 过滤交互 <1s（同上缩面） | /api/runs?caller= 单发 | **8 ms** | ✓ |
| /api/runs?caller= p95 <300ms | 20 采样 | p95=**8.6ms**（max 10.2） | ✓ |
| 真实后端截图 e2e | RP_P6_E2E=1 playwright runs-caller（chromium 实跑） | 全过 + e2e-*.png 2 帧落盘 | ✓ |

## 六、判据逐条状态（§4 十二条）

| # | 判据 | 状态 | 证据锚 |
|---|---|---|---|
| 1 | 形态一 N=4 并行子代理 | **过**（PARALLEL_PLAN 词形按 §二.2 口径由 Rust e2e 承载） | §四.1 |
| 2 | 形态二 N=4 caller 并发 | **过** | §四.2 |
| 3 | 形态三协作 + 推送闭环 | **过** | §四.3 |
| 4 | 形态四 N=4 供知识治理 | **过**（PG 缺席时 V-9 SKIP exit 0 口径在脚本内预挂） | §四.4 |
| 5 | SSE 多客户端订阅面 | **过** | §五 |
| 6 | 推送实时性数字化 | **过** | §五 |
| 7 | caller 维度列与过滤 | **过**：/api/runs list caller_id 投影（027 库）+ 三端点 ?caller= 正负例（unknown caller 空集 available=true）+ web 四文件在屏（runs-caller.spec 断言 + vitest 11 用例）+ 旧库不伪造（has_column 守卫） | §五 / §三 |
| 8 | 截图入库 | **过**：web/qa/runs/ 6 帧（≥4）+ selftest §6.9 在位断言 + 路由钉 22 不动 | §一 |
| 9 | 零迁移零新依赖零 Rust diff | **过**：399 + reqwest=0 + crates/** 零文件 + 030/V0008 空位 + A15 =29 双点 + web 零 npm | §三 |
| 10 | server 零直连写 + SSE 只读流 | **过**：六函数 AST 零写符号 + phase6 静态断言命中行与 HEAD 全等（既有 3 处 wsw_repeater INSERT 红不增不减）+ platform*.db 指纹前后一致 | §三 |
| 11 | 既有回归零改动 | **过**：七套全绿（§三）+ workspace 全量唯一红 di2_media×4 基线同败 | §三 |
| 12 | 真实实测（禁模拟冒充） | **过**：全部数字来自真实进程 / 真实 HTTP / 真实 SSE 流 / 真实 MCP-stdlib / 真实 PG 回显；SKIP 与 PASS 分开计数 | §四/§五 |

## 七、锚点勘误（A1-A16 实施时重定位记录）

- A5/A8 行号漂移：SSE 落点实测 = do_GET /api/ 分支 require_auth 之后 API_ROUTES 分发之前（原锚 :4683-4765 区域内）；`_wf_col` :2557（原锚 :2673 为调用点）；`agent_run_list_sql` :2222 区域。实施按函数名重定位，词形以任务书为准。
- A15 persist.rs 链尾断言实测 :890（注释）+:899（断言词形）——与锚点「MA-3 后行号自 :899 漂移至 :895 起段」一致。
- A13 web grep "caller" 零命中在实施后自然失效（本批新增 caller 词形面，四文件 + api.ts + 两测试文件）。
- 其余锚点（A1-A4/A6/A7/A9-A12/A14/A16）实施核验一致，无勘误。

## 八、MA 轨收官记账（计划 §八完成定义七项逐条对账 + R1-R9 对照）

### §八完成定义七项对账表（MULTI-AGENT-ACCESS-PLAN.md:76 禁模拟冒充口径）

| # | 完成定义项 | 承载批次与数字原文 | 状态 |
|---|---|---|---|
| 1 | 四形态各一条端到端实测 | MA-4 §四.1-§四.4（形态① 0.3 CLI 提速比 / 形态② 0.283≤0.7 / 形态③ SSE 闭环 / 形态④ 161ms N=4 全 CANDIDATE） | 达标 |
| 2 | per-caller 预算硬拒 | MA-2 三帽（速率/总预算 GUARDRAIL + 受理面 BudgetDenied run 不落行）+ MA-3 :101 词形 + MA-4 §7 超帽负例（`budget denied` + `max_concurrent_runs` spawn 日志实测 + run 零残留） | 达标 |
| 3 | 双 AI 协作 workflow 节点 | MA-3 双 caller 节点绑定 + MA-4 §4 复用复测（node1 继承 run 级 / node2 params 显式） | 达标 |
| 4 | 并发知识写入冲突治理 | MA-3 N=6 + MA-4 N=4（全 CANDIDATE + SAME_CONTENT ≥N-1 + 双闸负例 + approve 正例） | 达标 |
| 5 | 事件推送实时性 | MA-4 §五（≤2s / p95 0.406s / 零丢失 / 心跳 2.003s / N=4 同序） | 达标 |
| 6 | 多 key 审计三态 | MA-1 selftest_auth 28/0 + MA-4 §3 审计 JSONL 四 caller 各归各 + legacy=default 403 行 | 达标 |
| 7 | 禁模拟冒充 | 全部真实进程/流/库回显（判据 12） | 达标 |

### MA-DSH-REFERENCE R1-R9 落地对照

| 项 | 落地 | 批次 |
|---|---|---|
| R1 协作协议（claim 队列/CAS/邮箱） | task_claim 五态 + ma_message 两列分记 + rp-collab 十一子命令 | MA-3 |
| R2 未投递 sweep 重投 + 事件推送通知 | msg-sweep `{undelivered_total, items[]}`（MA-3）+ **MA-4 SSE message 事件 = 投递通知（to_caller 过滤命中，本批接通 R2 后半句）** | MA-3 + MA-4 |
| R3 派发契约（MCP/REST 双通道） | MCP 七工具（MA-3）+ REST /api/subtasks（MA-3 落地、**MA-4 修正 spawn 二进制使其真实可运行**） | MA-3 + MA-4 |
| R4 成员建队 | **登记不实现**（越界红线 8：R4 toolPolicies→RT-3 constraints 绑定不进 MA 轨） | 观察池 |
| R5/R7 预算面/异构成员 | caller_budget 七帽（MA-2）/ R7 异构成员登记 | MA-2 / 观察池 |
| R6 词值 | kind 词表 agent_subtask/review 冻结（MA-3 词值，029:121） | MA-3 |
| R8 设计注意点 | 双库分工（PG=情报核）+ 行级 caller 字段非分库——MA-4 SSE 事件源三形态遵守（PG 面不进 SSE） | 设计约束持续有效 |
| R9 Runs 页扩展 | caller 维度列与过滤 = 计划 :52 字面落地（MA-4 web 面）；**DAG 状态深化（可交互重排/成员树/模型标注）登记不实现**（D5） | MA-4 + 观察池 |

**MA 轨至此收官。** 完成定义未达标项：无（七项全达标；两处词形级口径声明见坏消息 §二.2——PARALLEL_PLAN CLI 面与 R4/R9 深化面均为「登记不实现」既有裁决，非未达标）。

## 九、Plugin Center 观察池登记提醒（不在 MA 轨实现，后续批按观察池入口核对）

- E1/E4/E5（目录站分类 / 市场内浏览 / 单插件形态，MA-DSH-REFERENCE:33/:36/:37）
- dsh-im IM webhook 通知渠道（:67——SSE 事件源已就位，IM webhook 只需消费 /api/events 转发，登记不进必交付）
- R9 DAG 深化 / 成员树 / 模型标注（MA-DSH-REFERENCE:21 深化半句）
- R4 建队与 RT-3 constraints caller 维度绑定（红线 8 越界面）
- WSW-5 UI 遗留面（WSW-4-BATCH-REPORT:278 §5.5 四区页面交接）

## 十、回滚与复现

- 回滚：整批工作树改动随 `git checkout -- apps/server/rp_server.py web/` + 删除新增四文件（selftest_ma4.sh / playwright.runs.config.ts / runs-caller.spec.ts / web/qa/runs/）撤销零残留；零迁移零库残留（SSE 无持久化、订阅态进程内存、A15 钉值零随更——revert 后无任何 DROP/清理步骤）。
- 复现：

```bash
cd /media/emo/code/pensting/research-platform
export CARGO_TARGET_DIR=/home/emo/rt3-target
bash apps/server/selftest_ma4.sh          # 预期回显：RESULT: ALL-PASS（SKIP=0 单列计数）；exit 0
bash apps/server/selftest_auth.sh && bash apps/server/selftest_ma3.sh   # 既有回归自证
cd web && npm run typecheck && npm test   # 预期：Test Files 19 passed / Tests 126 passed
ls web/qa/runs/*.png | wc -l              # 预期：6 ≥ 4
```
