# MA-3 批次报告：协作协议与知识供体面（2026-11-22）

- 状态：收口（本报告 = §5 全部命令实测回显；三硬验收数字全部来自真实进程/真实线程/真实 HTTP/真实 MCP stdio/真实 PG）
- 依据：[MA-3-TASK.md](MA-3-TASK.md)（HEAD `d664ceef` 时点锚点 A1-A22）；实施按 §3 S1-S9 串行
- 工作树未 commit（禁 git commit 纪律）；主会话审查收口

---

## 一、改动文件清单

### 新建（10 文件，4128 行）

| 文件 | 行数 | 内容 |
|---|---|---|
| `migrations/029_collab_protocol.sql` | 125 | D6：task_claim 五态 CHECK（claimable/claimed/completed/failed/transferred）+ kind CHECK（agent_subtask/review）+ attempt 计数 + claimed_by/created_by 真 FK→caller + depends_on_claim_id 自引用 FK + ma_message（mam- / from-to_caller 真 FK / delivered_at+read_at 分记）+ 五索引 + schema_version(29)/user_version=29 双写；纯新增零 ALTER |
| `migrations_pg/V0007__knowledge_donor.sql` | 30 | D5：knowledge_cards 纯追加 donor_caller_id TEXT + donor_channel CHECK('mcp','cli') + idx_cards_donor |
| `crates/workflow-engine/src/collab.rs` | 1362 | D2/D3：ClaimStore（create 递归 CTE 环检测/ready_list 依赖门读面/claim 单语句 CAS+RETURNING/complete+fail 带 claimed_by 守 CAS/transfer with_tx 两语句 attempt+1/show）+ MessageStore（send/fetch 拉模型投递分记/read read_at IS NULL CAS/sweep 未投递清单计数）+ 单测 8 枚 |
| `crates/workflow-engine/src/bin/rp-collab.rs` | 443 | D7：协作 CLI 十一子命令（claim-create/list/claim/complete/fail/transfer/show + msg-send/fetch/read/sweep；exit 0/1/2 口径；--db platform*.db 守卫；--caller-id 必填署名面） |
| `crates/workflow-engine/tests/node_caller_binding.rs` | 675 | S4：判据 1/2 Rust 面 e2e 六枚（双 caller agent_run 行/预算帽 BudgetDenied 词形/GUARDRAIL 分账/params_invalid 负例/legacy NULL/零自动建 claim） |
| `crates/workflow-engine/tests/collab_claim.rs` | 389 | S5：判据 4/5/6 集成六枚（真线程 N=8 CAS 恰一胜/依赖门/transfer attempt 链/五态词表 029 CHECK 拒/邮箱全序/ready_list，文件库+WAL 真多连接竞争） |
| `crates/mcp-server/tests/subtask_dispatch.rs` | 386 | S7：判据 3 e2e 两枚（真实 stdio JSON-RPC → rip.run_subtask 真实 rp-agent python_worker 闭环 → agent_run.caller_id 落列 → get_subtask_status 一致回读；非法 token POLICY_DENIED 结构化拒绝 + 目录面零影响） |
| `crates/intel-db/tests/donor_governance.rs` | 276 | S6：判据 7/8 两枚（N=6 真线程并发 donor 同 content 全 CANDIDATE + SAME_CONTENT N-1 边幂等 + 双闸负例 + approve 正例 + review_log 留痕 + 检索面谓词；V-9 SKIP 守卫） |
| `apps/server/selftest_ma3.sh` | 442 | S8：收口脚本（三硬验收 + claim 真进程 N=6 竞争 + 邮箱全序 + 隔离 PG 供体治理 + 生产库指纹；57 ck 全过 exit 0） |
| `docs/rip3/MA-3-TASK.md` | （任务书本身，前置已落） |

### 修改（41 文件，+1143/-74 行；A18 十六硬钉 28→29 全清单）

| 文件 | 行数 | 内容 |
|---|---|---|
| `crates/workflow-engine/src/state.rs` | +26/-2 | create_run_with_caller（additive；create_run 委托 None——既有测试零改动前提下的扩参落法，D1① workflow_run.caller_id 写面） |
| `crates/workflow-engine/src/exec/mod.rs` | +63/-8 | start_with_caller（start_with_id 委托 None）+ retry 继承原 run caller（MA-2 control.rs:332 口径）+ run_caller_of 读形 + dispatch_node AgentNodeRequest caller_id:None 字段 |
| `crates/workflow-engine/src/exec/node_agent.rs` | +29/-2 | AgentNodeRequest.caller_id 字段 + params 开放面新键解析（非字符串 params_invalid 拒——判据 2 负例词形） |
| `crates/workflow-engine/src/bin/rp-workflow.rs` | +87/-6 | RunFlags --caller-id + cmd_run start_with_caller + AgentLoopDriver 解析序（节点 params > workflow_run.caller_id 行 > 装配兜底 > None）+ caller_budget load_caller_budgets + with_control 装配（rp-agent.rs:746-782 同型）+ usage |
| `crates/workflow-engine/src/lib.rs` | +5 | collab 模块注册 + 导出 |
| `crates/workflow-engine/Cargo.toml` | +4 | [[bin]] rp-collab |
| `crates/mcp-server/src/dispatch.rs` | +458 | RunChannel 四字段（collab_caller/agent_bin/kc_bin/subtask_timeout_s）+ 七工具方法（claim_task 双语义/complete_claim/send_message/fetch_messages/run_subtask spawn rp-agent wait+超时信封/subtask_status agent_run 读行/contribute_knowledge spawn rp-kc 收据透传） |
| `crates/mcp-server/src/lib.rs` | +85 | CAPABILITY_TOOLS 6→13（七枚 additive；既有五枚 wired 词形零 diff）+ description + inputSchema |
| `crates/mcp-server/src/service.rs` | +27 | call_tool 七枚新分派臂（additive） |
| `crates/mcp-server/src/bin/rp-mcp.rs` | +71/-6 | resolve_caller_key（RP_MCP_TOKEN sha256 → api_key JOIN caller 吊销判定，只读直查；rp_server.py _resolve_caller_key 同款 SQL）+ RP_AGENT_BIN/RP_KC_BIN/RP_SUBTASK_TIMEOUT_S 装配 |
| `crates/mcp-server/Cargo.toml` | +9 | sha2 转正一行（D9）+ workflow-engine path 依赖（claim/message store 复用面，workspace 既有成员零新外部包） |
| `crates/mcp-server/tests/native_rust_dispatch.rs` | +6 | RunChannel 新字段机械适配（collab_caller:None 缺省） |
| `crates/mcp-server/tests/mcp_protocol.rs` | +9/-4 | 能力面计数钉 6→13 / 11→18 / 36→43（七枚 additive 的目录计数随更，两处断言本就动态构造仅钉值与注释更新） |
| `crates/rp-mcp-parity/tests/parity.rs` | +4/-2 | 注释随更（断言本就取 CAPABILITY_TOOLS 动态长度，零钉值改动） |
| `crates/intel-db/src/card_repo.rs` | +36/-2 | CardInput donor_caller_id/donor_channel 两字段 + create_card INSERT 扩两列（None→NULL 零回归）+ donor_info 读取面 + donor_channel 词表 fail-closed |
| `crates/intel-db/src/bin/rp-kc.rs` | +98 | donor-submit 子命令（create→decide CANDIDATE 两步 + source_trust=agent_derived/ai_derived=TRUE 定死 + review_log reviewer=donor_caller + 收据 JSON stdout `{knowledge_id, review_status, normalized_hash}`；--donor-caller 必填 fail-closed）+ usage |
| `crates/intel-db/tests/{gov_semantic,gov_dedup,knowledge_governance,search_hybrid}.rs` | +2×4 | CardInput 新字段机械适配（None） |
| `apps/server/rp_server.py` | +90/-6 | POST /api/subtasks（require_auth + caller 未署名 403 fail-closed + spawn rp-agent --caller-id --db RP_WORKFLOW_DB + run 回执；AGENT_BIN 常量）+ workflow_write run 分支 --caller-id（鉴权面恒定；body caller 键 400 拒）+ handle_workflow_runs caller_id 投影（_wf_col 动态）+ agent_run_list_sql caller_id 列（判据 3 读面） |
| `apps/server/selftest_phase9_mcp.sh` | +13/-3 | 能力面计数钉 6→13 + description 白名单七枚（七枚 additive 目录随更；判据 3 交付的必然联动，见坏消息 2） |
| A18 十六硬钉余下 12 文件 | +50/-50 | 28→29 四类钉值形态逐 file:line（migration_idempotent/migration_dev_copy_p7_3/migrations_agent_data_plane/migrations_tool_contract/migrations_runtime_node/migrations_context_broker/backup_restore/caller_auth/lifecycle_full_path/migration_replay/wsw-message store.rs+cas_e2e/wsw-runtime persist.rs+exchange_persist+intercept_queue+fuzz_job） |
| `Cargo.lock` | +2 | 仅 mcp-server 依赖清单两行（sha2 0.10.9 转正 + workflow-engine 复用面）；`[[package]]` 399 恒定（S1/S9 双点复核）；reqwest = 0 保持 |

### 零 diff 实证面（§2.2）

- `crates/agent-engine/**` 零 diff（git status 实证）——RunRequest.caller_id/BudgetDenied/load_caller_budgets 全复用（A12/D1）；rp-agent.rs 零改动。
- `crates/workflow-engine/src/exec/node_tool.rs / node_knowledge.rs / node_condition.rs / node_parallel.rs / node_human.rs`、`dsl.rs / validate.rs` 零 diff（D1 只动 node_agent 参数面）。
- `crates/tool-engine/**`、`crates/tool-registry/**`、`policy.rs`、`crates/context-broker/**`、`crates/intel-db/src/gov_worker.rs / search.rs / embed_worker.rs / document_repo.rs` 零 diff。
- `crates/mcp-server/src/catalog.rs / prompts.rs / resources.rs` 零 diff（七枚新工具纯 additive）。
- `migrations/001-028` 零 diff、`migrations_pg/V0001-V0006` 零 diff（git diff --stat 实测 0 文件）；`schemas/` 零 diff；`integrations/rtis/`、`rip-data/**` 零触碰；`web/**` 零 diff（phase6 Playwright 截图副作用 git checkout 还原，收尾复核 clean）。
- workspace 根 `Cargo.toml` 零 diff（sha2 已在 workspace.dependencies:91）。

---

## 二、坏消息（单列）

1. **Cargo.lock 超出「仅 sha2 转正行」预期 +2 行**：mcp-server 复用 ClaimStore/MessageStore（D9 明文「claim/message store 落 workflow-engine 复用 database::Pool」）必须依赖 workflow-engine——workspace.dependencies 无该条目（workflow-engine 是叶子成员），故用 path 依赖（`{ path = "../workflow-engine" }`），Cargo.lock 落 +2 行（mcp-server 依赖清单内 sha2 + workflow-engine 两行）。`[[package]]` 399 恒定、vendor 零变更、reqwest=0——零新依赖红线（D9 红线 5）实质满足（全 workspace 既有成员），但与任务书 §2.2「Cargo.lock 仅 mcp-server 依赖清单行（sha2 转正）」的字面单行预期多出一行（同属 mcp-server 依赖清单行范畴）。
2. **`apps/server/selftest_phase9_mcp.sh` 钉值随更（3 处）**：能力面计数 6→13、description 白名单七枚——七枚新工具 additive 入目录是 D4 判据 3 的交付本体，目录计数断言必然随更（mcp_protocol.rs 两处同款：11→18、36→43）。既有五枚 rip.* 词形零 diff 实证不变（tools/list 条目逐一对账 PASS）；这是「既有 selftest 零改动」在 additive 目录面下的唯一不可免例外，与 A18 硬钉随更同性质。
3. **`RunStore::create_run` 未改签名**：任务书 S3 写「create_run 扩参 caller_id」；实施取 additive 形（`create_run_with_caller`，create_run 委托 None）——in-crate 既有测试（control.rs/state.rs 五处调用点）零改动是判据 2「既有测试零改动」的硬约束，二者冲突时取后者；D1 写面语义等价（start_with_caller 全链落列）。
4. **`RunChannel` 扩四字段（非「claim/message store 挂 Pool」字面）**：store 不常驻（每次 tools/call 现开 Pool——P9-1 无状态模型 §九延续，dispatch.rs 头注「每次 tools/call 独立打开运行库」既有纪律），`collab_caller` 以字段随 RunChannel 装配；native_rust_dispatch.rs 测试构造机械补四字段（collab_caller None = fail-closed 缺省）。
5. **phase6 remote install 一次 PARSE_FAIL（网络抖动，复跑消失）**：首轮回归 `install project.rtis-mcp（remote 档）` 网络面失败引发 3 连锁 FAIL；复跑 PASS=48 FAIL=1（与 MA-2 报告同水位）。非本批引入。

---

## 三、§5 命令实测（exit code + 关键输出行）

| # | 命令 | exit | 关键输出 |
|---|---|---|---|
| 1 | `CARGO_TARGET_DIR=/home/emo/rt3-target cargo test --offline --workspace --no-fail-fast` | 101（唯一红 = 环境面） | 177 套件；`test result: FAILED. 22 passed; 4 failed`（document-router three_channel_e2e di2_media×4，S1 基线同败 = ffprobe 环境漂移，登记环境面）；其余 175 套件全 ok（含本批 collab_claim 6/node_caller_binding 6/subtask_dispatch 2/collab 8/mcp_protocol 11/donor_governance 2[PG 在场实跑]） |
| 2 | `cargo test --offline -p workflow-engine -- --test-threads=1` | 0 | collab 8/8 + collab_claim 6/6 + node_caller_binding 6/6 + 既有全套绿 |
| 3 | `cargo test --offline -p mcp-server -- --test-threads=1` | 0 | subtask_dispatch 2/2 + mcp_protocol 11/11 + run_tool_e2e/native_rust_dispatch/mcp_protocol 全绿 |
| 4 | `cargo test --offline -p intel-db -- --test-threads=1`（PG 在场） | 0 | donor_governance 2/2（真实 PG）+ knowledge_governance 5/5 + search_hybrid 3/3 + 全 crate 绿 |
| 5 | `cargo clippy --offline -p workflow-engine -p mcp-server -p intel-db --all-targets -- -D warnings` | 0 | `Finished \`dev\` profile`（rp-mcp-parity clippy 同过） |
| 6 | `cargo deny check advisories && cargo deny check licenses` | 0 | `advisories ok` / `licenses ok` |
| 7 | `grep -cE '^\[\[package\]\]' Cargo.lock` | 0 | `399`（HEAD d664ceef 基准恒定） |
| 8 | `grep -c 'name = "reqwest"' Cargo.lock` | 0 | `0` |
| 9 | 迁移水位反证（python3 ro 探针） | 0 | dev-p13/dev-p4/dev-p71 = **29**；platform.db=1 / platform-dryrun.db=1 / platform-fullbatch.db=1 / platform-governance.db=0 / platform-search.db=0（反证基线不变）；dev-b* 副本自迁面见 D 节观察 |
| 10 | `bash apps/server/selftest_auth.sh` | 0 | 全过 |
| 11 | `bash apps/server/selftest_ma2.sh` | 0 | 全过（MA-2 三硬验收零改动回归） |
| 12 | `bash apps/server/selftest_phase7_agents.sh` | 0 | 全过（405 钉值不动） |
| 13 | `bash apps/server/selftest_phase8_workflow.sh` | 0 | 全过 |
| 14 | `bash apps/server/selftest_phase9_mcp.sh` | 0 | PASS=26 FAIL=0（能力面计数钉 6→13 随更后全过） |
| 15 | `bash apps/server/selftest_phase6.sh` | 1 | PASS=48 FAIL=1——唯一 FAIL = 「rp_server.py 全文件无 SQL 写语句」，MA-1/MA-2 报告登记的**既有红**（`git show HEAD:apps/server/rp_server.py` grep 三处 wsw_repeater INSERT 命中实证 = HEAD 同败），与本批无关 |
| 16 | `bash apps/server/selftest_ma3.sh` | 0 | **PASS=57 FAIL=0 ALL-PASS**（三硬验收 + 竞争 + 邮箱 + 供体 + 指纹） |

## 四、三硬验收实测数字

### 硬验收①：双 AI 协作同一 workflow（节点分属两 caller）——判据 1/2

- 载具：`wf-ma3-selftest`（node1 无绑定 / node2 params caller_id=caller-ma3-b），真实 rp-workflow 进程 `run --caller-id caller-ma3-a`。
- `workflow_run.caller_id = caller-ma3-a`（库直查）；`agent_run` 两行 caller_id 分别 = **caller-ma3-a（node1 继承 run 级）/ caller-ma3-b（node2 params 显式）**；两行 trace_id = run_id 同链。
- 预算分账：caller-a 总预算帽 1 步 → GUARDRAIL `budget_caller_total` 审计行（Rust 面 node_caller_binding 实测，detail 含 caller_id）；caller-b 无帽零预算行。
- 帽拒词形：caller-b 并发帽 1 + RUNNING 记账行占位 → node2 failed `code=agent_failed`、message 含 **`budget denied`** 与 `max_concurrent_runs`（selftest 实测回显）；BudgetDenied run 不落行（agent_run 计数 0）→ run fail-fast 收敛。
- 解析序三层正例 + 负例：params 显式 / run 级继承 / None legacy（agent_run.caller_id NULL、workflow_run.caller_id NULL）；params caller_id=42 → `params_invalid`（message 含 "caller_id must be a string"）。
- retry 继承：retry_with_id 新 run 继承原 run caller（Rust 面实测于既有 retry 用例 + 代码路径 control.rs:332 口径同源）。
- D7：executor 全 run 零 task_claim 行自动创建（`SELECT COUNT(*) FROM task_claim` = 0，node_caller_binding.rs 实测）。

### 硬验收②：外部 AI MCP 派发往返——判据 3

- caller-ma3-ext key（rp-admin key-gen，rp1_+43 位）→ RP_MCP_TOKEN 注入真实 rp-mcp（stdio JSON-RPC）。
- `rip.run_subtask` 往返：**226 ms**（selftest）/ **402 ms**（Rust e2e），真实 rp-agent 子进程 + 真实 python_worker（rip.kb.keyword_search）闭环 → report.state = **SUCCEEDED**、caller = caller-ma3-ext。
- `agent_run.caller_id = caller-ma3-ext` 落列（库直查）；`rip.get_subtask_status` 一致回读（state/caller_id/agent_id 三列一致）。
- REST 同契约：`POST /api/subtasks`（caller-ma3-a key）→ 200 `{"result":"accepted","action":"subtask","caller":"caller-ma3-a",...}` → `/api/agent-runs` 读面 caller_id 可见；`POST /api/workflow-runs` run action → `workflow_run.caller_id = caller-ma3-a`（鉴权面传递）；body 传 caller_id → 400。
- legacy token 两通道均 fail-closed：MCP `rip.run_subtask` → isError `POLICY_DENIED`（message 含 fail-closed）；REST POST /api/subtasks → **403**；非法 token tools/list 正常（目录面零影响）。

### 硬验收③：并发知识写入冲突治理——判据 4/7

- Rust 面（真实 PG，donor_governance.rs）：**N=6 真线程**并发 donor-submit 同 content → 6 行卡片全 **CANDIDATE** + donor_caller_id/donor_channel 逐行正确 + dedup-scan SAME_CONTENT 候选边 = **5（= N-1）** + 二跑幂等 0 新建 + 未计分 approve 拒（"quality score not computed yet"）+ 硬标记 approve 拒（"approve blocked: hard risk flag present"）+ 单卡 approve 正例（DISCOVERED→CANDIDATE→APPROVED 日志三跃迁留痕）。
- 真进程面（selftest_ma3.sh，隔离 PG 容器 rip-pg17 digest 钉死）：**N=6 并发 rp-kc donor-submit 进程**，耗时 **188 ms**，收据 6/6 全 CANDIDATE（`{knowledge_id, review_status:"CANDIDATE", normalized_hash:64hex}`），PG knowledge_cards 六行 donor 署名逐行正确（donor_channel='mcp'），SAME_CONTENT 候选边 = 5，knowledge_review_log 供体转移留痕 6 行（reviewer=caller-donor-*），双闸负例未计分 approve 拒 exit 1。

### 判据 4/5/6（claim CAS / 转派记账 / 邮箱分记）

- 真进程 **N=6 并发 CAS 恰一胜**（selftest：6 进程同刻 claim-claim，1 胜 5 败 exit 1，胜方 attempt=1）；真线程 N=8 同刻 CAS（collab_claim.rs：1 胜 7 败 NotClaimable，零 attempt 副产物行）。
- 依赖门：depends_on 未 completed → claim 拒（DependencyNotCompleted）+ ready_list 过滤；依赖 completed 后可领；环检测（递归 CTE 深度帽 64）拒 CycleDetected。
- transfer：仅 claimed_by 本人（他人 NotClaimer 0 行零副作用）→ transferred 终态禁写 result → 新行 claimable attempt=2 → 败方领取成功；complete/fail 仅本人可写；failed 终态 error_json 落行；五态词表逐字（非法词 INSERT 被 029 CHECK 拒断言）。
- 邮箱全序：send → sweep 未投递计数 1 → fetch（delivered_at 落 / read_at NULL）→ 重复 fetch 可见（持久重投）→ read → fetch 不再返回 → sweep 归零；未知/吊销 caller 收发双拒。
- dsh 弱点对照：R1 原子领取 = 数据库单语句 CAS（WAL 单写者原子等价 BEGIN IMMEDIATE，D2 裁决）优于 dsh 单进程串行状态文件；弱点 3 引擎侧记账 = complete 带 `AND claimed_by=?` 成员不可伪造他人认领行（实测 NotClaimer 拒）。

## 五、判据逐条状态（§4 十二条）

| # | 判据 | 状态 | 实证 |
|---|---|---|---|
| 1 | 双 AI 协作节点分属两 caller | **PASS** | selftest §2 节 + node_caller_binding.rs |
| 2 | 节点绑定解析序 | **PASS** | 三层正例 + params_invalid 负例 + legacy NULL（node_caller_binding.rs 3 枚） |
| 3 | 外部 AI MCP/REST 派发往返 | **PASS** | selftest §5/§6 节 + subtask_dispatch.rs（226/402ms 真闭环） |
| 4 | TaskClaim 事务化原子领取 | **PASS** | 真进程 N=6 恰一胜 + 真线程 N=8 + 依赖门 + 环检测（dsh 弱点 1 对照声明见四节） |
| 5 | 转派/接管与引擎侧记账 | **PASS** | transfer attempt 链 + NotClaimer 拒 + 五态 CHECK 拒（collab_claim.rs） |
| 6 | ma_message 邮箱分记 | **PASS** | send→fetch→read→sweep 全序 + 吊销 caller 拒（collab_claim.rs + selftest §4） |
| 7 | 并发知识写入冲突治理 | **PASS** | 真线程 N=6（PG）+ 真进程 N=6（容器 PG）全 CANDIDATE + N-1 边 + 双闸（PG 缺席 V-9 SKIP 路径实测 0.00s SKIP exit 0） |
| 8 | 供体治理面不越权 | **PASS** | approve 前不进检索面（缺省谓词仅 APPROVED 断言）+ 供体恒 CANDIDATE + RTIS 零触碰（integrations/rtis 零 diff） |
| 9 | 029/V0007 纯增量幂等 | **PASS** | user_version=29（临时库/dev-p13/p4/p71）+ refinery 二跑零变化（migrations_apply_idempotent 断言 version>=4 实测 7）+ 029 纯新增零 ALTER + V0007 两列一索引零 CHECK 域改动 |
| 10 | 三面同源与零回归 | **PASS** | MCP/REST/CLI 同 rp-agent CLI 路径同库同 schema（selftest 同库三通道断言）；既有 selftest 五绿 + phase6 既有红口径不变（48/1，HEAD 同败）；agent-engine/tool-engine/tool-registry/policy.rs 零 diff 实证 |
| 11 | 门禁与迁移纪律 | **PASS** | workspace 全量一次（唯一红 = 环境面 di2_media×4，S1 干净 HEAD 基线同败）+ 定向 --test-threads=1 + clippy -D warnings + deny 双检 + 399 恒定 + reqwest=0 + 001-028/V0001-0006 零 diff + 029/V0007 单文件 + A18 十六硬钉全绿 + git status 只含 §2.1 落点（web 截图副作用已还原） |
| 12 | 真实实测禁模拟冒充 | **PASS** | 判据 1/3/4/7 数字全部来自真实进程/线程/HTTP/MCP stdio/PG 回显（226ms/402ms 派发、188ms 并发供体、N=6 进程 CAS 竞争） |

## 六、观察项与遗留登记（MA-4 前置输出见任务书 §8）

1. **dev-b1/dev-b2 水位自迁**：S1 探针 dev-b1=27 → 收口 29（dev-b2=28）——写入方为 tool-registry `first_batch_b1.rs`（P2 批既有测试，直接对 migrations/dev-b1.db 迁移，非本批代码路径）；dev-b* 均为 gitignored 开发副本，platform*.db 反证不变。登记观察，无需处理。
2. **phase6 SQL 写静态断言既有红延续**：rp_server.py 三处 wsw_repeater INSERT（WSW-2/3 批落，HEAD d664ceef 同败，MA-1/MA-2 报告同登记）——本批未新增任何 SQL 写（`INSERT INTO|UPDATE plugin|DELETE FROM` grep 命中行与 HEAD 全等）。
3. **MA-4 移交清单（任务书 §8 五项全部在位）**：①ma_message 行形状与 sweep 语义（delivered/read 两列分记即事件去重键）；②claim 队列词形与 CAS 原语签名（kind 词表 agent_subtask/review 冻结，扩词值须升 030+）；③节点 caller 绑定双层解析序（params 开放面挂点零迁移）；④caller 预算面实测语义备忘（速率窗口秒级粒度/总预算闸整轮硬停/BudgetDenied 拒绝词形 `budget denied: caller ... at cap ...`）；⑤供体收据词形 `{knowledge_id, review_status, normalized_hash}`。
4. **越界零项**：SSE/WS 推送、成员建队、R4 toolPolicies、R7 异构、嵌套预算、自动建 claim、Runs 页 UI、api_key.last_used_at 写入、research_task.caller_id 写面、RT-3 constraints caller 维度——全部未实现（红线 8 合规；rp_server.py research_task.caller_id 维持零写面）。
5. **回滚口径**（任务书 §7）：单 commit 交付后 `git revert` 整批撤销；029 revert 后开发副本残留 task_claim/ma_message 空表（DROP TABLE + PRAGMA user_version=28 可选清理）；V0007 残留两可空 NULL 列 + 一索引（PG 侧 `ALTER TABLE knowledge_cards DROP COLUMN` 参考语句仅限开发库手工执行）。

## 七、复现

```bash
cd /media/emo/code/pensting/research-platform
export CARGO_TARGET_DIR=/home/emo/rt3-target
cargo test --offline --workspace --no-fail-fast   # 175/177 套件绿；唯一红 = document-router di2_media×4（ffprobe 环境漂移，S1 干净 HEAD 同败）
bash apps/server/selftest_ma3.sh                  # 预期回显: MA-3 selftest: PASS=57 FAIL=0 / RESULT: ALL-PASS
grep -cE '^\[\[package\]\]' Cargo.lock            # 预期回显: 399
grep -c 'name = "reqwest"' Cargo.lock             # 预期回显: 0
python3 -c "import sqlite3; print(sqlite3.connect('file:migrations/platform.db?mode=ro',uri=True).execute('PRAGMA user_version').fetchone()[0])"  # 预期回显: 1（生产零触碰反证）
```
