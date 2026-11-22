-- ============================================================================
-- Research Intelligence Platform : RIP3 MA-3 协作协议与知识供体面（SQLite 侧）
--   增量迁移（MA-3-TASK.md §0 D2/D3/D6；MULTI-AGENT-ACCESS-PLAN.md §四 MA-3 行 :47）：
--
--   纯新增两表 + 五索引，零 ALTER、零既有表改动（028 文件头纪律同源；批内
--   若发现确需 ALTER 既有表，另立 030+，禁改历史迁移）。
--
--   task_claim : TaskClaim 子任务认领队列（D2）：
--     - state 五态词表逐字 CHECK（claimable/claimed/completed/failed/
--       transferred）；状态跃迁全部走 collab.rs 单语句 CAS（UPDATE …
--       WHERE state=from [AND claimed_by=?] RETURNING …），非法迁移 0 行
--       受影响即拒——SQLite WAL 单写者串行化使单语句 CAS 原子性与
--       BEGIN IMMEDIATE 等价（D2 裁决原文）。
--     - kind 词表 agent_subtask/review（D7：R6「任务类型词表加 review，
--       复用既有语义不自造复审体系」的最小落法 = kind 词值；MA-4 扩词值
--       须升 030+ 不回改）。
--     - attempt 计数列：首建 = 1；transfer 时旧行转 transferred 终态 +
--       同事务插入新 claimable 行 attempt+1（R3「claim 竞争失败方接新
--       attempt，旧 attempt 结果作废位标记」；transferred 行禁写 result）。
--     - claimed_by/created_by 真 FK→caller（026:52 api_key 先例；claim 行
--       归属必须是在用 caller——caller 只 revoke 置位不删除，FK 恒成立）。
--     - depends_on_claim_id 自引用 FK（dsh R1 依赖门：依赖未完成的任务
--       不可领取——CAS WHERE 子句内联 EXISTS(dep.state='completed')）；
--       create 时递归 CTE 环检测 fail-closed（collab.rs 承载，本表列面
--       不承载行为）。
--     - run_id/node_id/agent_id 引用列（可空；编排方显式登记，D7 本批
--       零引擎自动建 claim）：run_id 走 wfr- 词形守卫；引用列不建硬 FK
--       （027:18-20 归属列口径同源，存在性由创建面把关）。
--   ma_message : 邮箱消息（D3，R2「投递/读取分别记录；未投递消息持久
--       保存后重投」）：delivered_at 与 read_at 两列分记（投递 = 收件方
--       fetch 时单语句 UPDATE delivered_at（首拉落记不覆盖）；读取 =
--       read 标记单语句 CAS read_at IS NULL 守卫）；重投 = 拉模型天然
--       持久——未读行每次 fetch 均返回，read 确认后不再返回；sweep =
--       只读未投递行清单/计数（MA-4 SSE 推送面通知源输入，推送通道
--       本批零实现）。from_caller/to_caller 真 FK→caller（消息路由主语
--       强一致：收发双方必须是在用 caller，fail-closed；027 归属列不建
--       FK 口径不适用——本表 caller 是路由主语非归属标注）。
--
--   双库分工（D6③）：SQLite = 平台运行时（tasks/workflow/agent/tool/
--   caller/wsw/claim/message 协作运行时数据），PG = 情报核（document/
--   knowledge/search，V0007 供体署名落 PG knowledge_cards）；既有域分工
--   不是租户分库——claim/ma_message 与 workflow_run/agent_run/caller 同库
--   同 schema 行级 caller 字段（禁止事项 1 禁 per-caller 分库分 schema）。
--
--   写权分治：两表写入唯一入口 = workflow-engine collab.rs
--   （ClaimStore/MessageStore）经 rp-collab CLI / MCP rip.claim_task 等
--   七工具；rp_server.py 零直连 SQL 写红线维持（REST 派发走 rp-agent
--   subprocess 委派）。
--
--   幂等：CREATE TABLE IF NOT EXISTS + CREATE INDEX IF NOT EXISTS +
--   INSERT OR IGNORE（002/018/020/021/022/023/024/025/026/027/028 先例）。
--   红线（ROADMAP §13）：只在开发库副本与测试内存库执行；
--   migrations/platform*.db 与 platform-dryrun.db 一律不写（018:55-56 同款）。
-- ============================================================================

CREATE TABLE IF NOT EXISTS task_claim (
  claim_id            TEXT PRIMARY KEY
                        CHECK (claim_id LIKE 'tcl-%'),
  kind                TEXT NOT NULL
                        CHECK (kind IN ('agent_subtask','review')),
  title               TEXT NOT NULL CHECK (length(trim(title)) > 0 AND length(title) <= 512),
  payload_json        TEXT NOT NULL CHECK (length(payload_json) <= 1024*1024),
  run_id              TEXT CHECK (run_id IS NULL OR run_id LIKE 'wfr-%'),
  node_id             TEXT CHECK (node_id IS NULL OR (length(node_id) > 0 AND length(node_id) <= 128)),
  agent_id            TEXT CHECK (agent_id IS NULL OR (length(agent_id) > 0 AND length(agent_id) <= 128)),
  depends_on_claim_id TEXT REFERENCES task_claim(claim_id),
  state               TEXT NOT NULL CHECK (state IN
                        ('claimable','claimed','completed','failed','transferred')),
  claimed_by          TEXT REFERENCES caller(caller_id),   -- NULL = 未领取
  attempt             INTEGER NOT NULL CHECK (attempt >= 1),
  result_json         TEXT,   -- completed 终态载荷（transferred/failed 不写 result）
  error_json          TEXT,   -- failed 终态载荷
  created_by          TEXT NOT NULL REFERENCES caller(caller_id),
  created_at          TEXT NOT NULL CHECK (created_at GLOB
                        '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9].[0-9][0-9][0-9]Z'),
  claimed_at          TEXT CHECK (claimed_at IS NULL OR claimed_at GLOB
                        '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9].[0-9][0-9][0-9]Z'),
  finished_at         TEXT CHECK (finished_at IS NULL OR finished_at GLOB
                        '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9].[0-9][0-9][0-9]Z'),
  updated_at          TEXT NOT NULL CHECK (updated_at GLOB
                        '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9].[0-9][0-9][0-9]Z')
);

CREATE TABLE IF NOT EXISTS ma_message (
  message_id   TEXT PRIMARY KEY
                 CHECK (message_id LIKE 'mam-%'),
  from_caller  TEXT NOT NULL REFERENCES caller(caller_id),
  to_caller    TEXT NOT NULL REFERENCES caller(caller_id),
  claim_id     TEXT CHECK (claim_id IS NULL OR claim_id LIKE 'tcl-%'),  -- 可空引用列（不建硬 FK）
  run_id       TEXT CHECK (run_id IS NULL OR run_id LIKE 'wfr-%'),      -- 可空引用列（不建硬 FK）
  payload_json TEXT NOT NULL CHECK (length(payload_json) <= 1024*1024),
  delivered_at TEXT CHECK (delivered_at IS NULL OR delivered_at GLOB
                 '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9].[0-9][0-9][0-9]Z'), -- NULL=未投递
  read_at      TEXT CHECK (read_at IS NULL OR read_at GLOB
                 '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9].[0-9][0-9][0-9]Z'), -- NULL=未读
  created_at   TEXT NOT NULL CHECK (created_at GLOB
                 '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9].[0-9][0-9][0-9]Z'),
  updated_at   TEXT NOT NULL CHECK (updated_at GLOB
                 '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9].[0-9][0-9][0-9]Z')
);

-- 五索引（029 任务书 D6：state / claimed_by / run_id / to_caller+read_at / claim_id）。
CREATE INDEX IF NOT EXISTS ix_task_claim_state
  ON task_claim(state, created_at);
CREATE INDEX IF NOT EXISTS ix_task_claim_claimed_by
  ON task_claim(claimed_by);
CREATE INDEX IF NOT EXISTS ix_task_claim_run
  ON task_claim(run_id);
CREATE INDEX IF NOT EXISTS ix_ma_message_inbox
  ON ma_message(to_caller, read_at);
CREATE INDEX IF NOT EXISTS ix_ma_message_claim
  ON ma_message(claim_id);

-- ---------------------------------------------------------------------------
-- schema_version：MA-3 协作协议持久化登记（version=29；链头 028_wsw_fuzz.sql
-- 推进 28→29；编号冻结规则顺延——migration_dev_copy_p7_3.rs:76 注释原文链尾
-- 演进至 029，MA-3-TASK.md §0 D6）
-- ---------------------------------------------------------------------------
INSERT OR IGNORE INTO schema_version (version, name, description, applied_at, applied_by) VALUES
  (29, 'V1 additive MA-3 collab protocol tables (task_claim, ma_message)',
   'MA-3（docs/rip3/MA-3-TASK.md §0 D2/D3/D6/D7 + MULTI-AGENT-ACCESS-PLAN.md §四 MA-3 行）：纯新增两表五索引零 ALTER。task_claim 子任务认领队列（claim_id tcl- 前缀；kind 词表 agent_subtask/review——R6 任务类型词表加 review 的最小落法，MA-4 扩词值须升 030+；state 五态 claimable/claimed/completed/failed/transferred 逐字 CHECK，状态跃迁全部单语句 CAS（UPDATE…WHERE state=from [AND claimed_by=?] RETURNING…，SQLite WAL 单写者原子等价 BEGIN IMMEDIATE，D2 裁决）；attempt 计数列首建 1、transfer 旧行转 transferred 终态 + 同事务插入新 claimable 行 attempt+1（R3 竞争失败方接新 attempt，旧 attempt 结果作废位）；claimed_by/created_by 真 FK→caller（026:52 先例，caller 只 revoke 不删除 FK 恒成立）；depends_on_claim_id 自引用 FK + 依赖门 CAS 内联 EXISTS(dep.state=completed)（dsh R1）+ create 时递归 CTE 环检测 fail-closed（collab.rs 承载）；run_id/node_id/agent_id 可空引用列不建硬 FK（027:18-20 口径，存在性由创建面把关）；payload 1MiB 帽）。ma_message 邮箱消息（message_id mam- 前缀；from/to_caller 真 FK→caller 路由主语强一致——收发双方必须是在用 caller fail-closed，027 归属列口径不适用本表；claim_id/run_id 可空引用列不建硬 FK；delivered_at 与 read_at 两列分记：投递 = 收件方 fetch 单语句 UPDATE 首拉落记不覆盖，读取 = read_at IS NULL 守卫 CAS；重投 = 拉模型天然持久——未读行每次 fetch 均返回、read 确认后不再返回（R2 未投递消息持久保存后重投的 RIP 落法）；sweep 只读未投递行清单/计数 = MA-4 SSE 推送面通知源输入，推送通道本批零实现；payload 1MiB 帽）。双库分工（D6③）：SQLite = 平台运行时（claim/message 与 workflow_run/agent_run/caller 同库同 schema 行级 caller 字段），PG = 情报核（V0007 供体署名落 knowledge_cards），不是租户分库。写权分治：两表写入唯一入口 workflow-engine collab.rs 经 rp-collab CLI / MCP 七工具；rp_server.py 零直连 SQL 写维持。零引擎自动建 claim（D7：claim 队列由编排方显式登记，workflow 执行器 dispatch_node 零改动）。只在开发库副本与测试内存库执行，platform*.db 生产文件不写',
   strftime('%Y-%m-%dT%H:%M:%SZ', 'now'), '029_collab_protocol.sql');

-- user_version 双写（DEC-002 条件，001/018/022/023/024/025/026/027/028 先例）
PRAGMA user_version = 29;
