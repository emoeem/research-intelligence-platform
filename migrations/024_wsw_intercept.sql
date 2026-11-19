-- ============================================================================
-- Research Intelligence Platform : RIP3 WSW-2 Proxy / MITM Runtime
--   增量迁移（WSW-2-TASK.md §二/§三 S5；WSW-2-ADR W2-D4/D6；DESIGN §3.2）：
--
--   纯新增三表，零 ALTER、零既有表改动（023 文件头 :30 纪律同源；批内若发现
--   确需 ALTER 既有表，另立 025+，禁改历史迁移）。
--
--   wsw_listener : MITM 监听器实例登记（六态状态机列，DESIGN §2 状态机词表）。
--   wsw_session  : WSW 会话/工作区（scope 规则源 + out-of-scope 命中策略）。
--   wsw_intercept: 拦截队列行式状态机（W2-D4「行即事实」——五态迁移落在行的
--                  state 列 + resolved_at；不单建 wsw_intercept_event 事件流水
--                  表，事件流水属审计增强留 WSW-3 History 落库批）。
--
--   列语义（词表 CHECK 逐列）：
--   - ID 形状：listener_id = wsw-lis- 前缀（wsw-message ids.rs:130-135）、
--     session_id = wsw-ses-（ids.rs:122-128）、intercept_id = wsw-itc-
--     （本批定稿，沿 ids.rs 范式生成侧承载）、exchange_id = wsw- 前缀
--     （元数据表 wsw_exchange 留 WSW-3，W2-D9 只落引用列）。
--   - event_id：幂等键 UNIQUE（DESIGN §3.2 wsw_intercept_event 幂等语义并入行）。
--   - state：五态词表 queued/forwarded/dropped/modified_forwarded/expired
--     （与 crates/wsw-runtime InterceptState::transition 词表一致）；
--     终态唯一性由应用面 UPDATE ... WHERE state='queued' SQL 强制 + 内存
--     PendingQueue 双面（W2-D6）。
--   - queued_at / expires_at / resolved_at：ISO8601 UTC 带毫秒固定宽
--     （字典序=时间序；TTL=ListenerConfig.queue_ttl_seconds 1..3600）。
--   - reason：拒绝/过期/丢弃原因（错误五态 class 可入，W2-D7 词表）。
--
--   幂等：CREATE TABLE IF NOT EXISTS + CREATE INDEX IF NOT EXISTS +
--   INSERT OR IGNORE（002/018/020/021/022/023 先例）。
--   只在开发库副本与测试内存库执行；migrations/platform*.db 生产文件不写。
-- ============================================================================

CREATE TABLE IF NOT EXISTS wsw_listener (
  listener_id  TEXT PRIMARY KEY
                 CHECK (listener_id LIKE 'wsw-lis-%'),
  name         TEXT NOT NULL CHECK (length(trim(name)) > 0),
  bind         TEXT NOT NULL CHECK (bind LIKE '127.0.0.1:%' OR bind LIKE '[::1]:%'), -- loopback 双保险（应用面 ConfigError::NonLoopback 前置）
  state        TEXT NOT NULL
                 CHECK (state IN ('stopped','starting','running','degraded','stopping','failed')),
  config_json  TEXT NOT NULL CHECK (length(config_json) > 0), -- ListenerConfig JSON + scope 规则引用
  created_at   TEXT NOT NULL CHECK (length(trim(created_at)) > 0),
  updated_at   TEXT NOT NULL CHECK (length(trim(updated_at)) > 0)
);

CREATE TABLE IF NOT EXISTS wsw_session (
  session_id          TEXT PRIMARY KEY
                       CHECK (session_id LIKE 'wsw-ses-%'),
  title               TEXT NOT NULL CHECK (length(trim(title)) > 0),
  scope_rules_json    TEXT NOT NULL CHECK (length(scope_rules_json) > 0), -- include/exclude/allow_private/policy（W2-D5 规则源）
  out_of_scope_policy TEXT NOT NULL
                       CHECK (out_of_scope_policy IN ('blocked','passive_forward')),
  created_at          TEXT NOT NULL CHECK (length(trim(created_at)) > 0),
  updated_at          TEXT NOT NULL CHECK (length(trim(updated_at)) > 0)
);

CREATE TABLE IF NOT EXISTS wsw_intercept (
  intercept_id TEXT PRIMARY KEY
                 CHECK (intercept_id LIKE 'wsw-itc-%'),
  event_id     TEXT NOT NULL UNIQUE CHECK (length(trim(event_id)) > 0), -- 幂等键（DESIGN §3.2）
  listener_id  TEXT NOT NULL CHECK (listener_id LIKE 'wsw-lis-%'),
  session_id   TEXT NOT NULL CHECK (session_id LIKE 'wsw-ses-%'),
  exchange_id  TEXT
                 CHECK (exchange_id IS NULL OR exchange_id LIKE 'wsw-%'), -- 元数据表留 WSW-3（W2-D9）
  direction    TEXT NOT NULL CHECK (direction IN ('request','response')),
  state        TEXT NOT NULL
                 CHECK (state IN ('queued','forwarded','dropped','modified_forwarded','expired')),
  host         TEXT,
  method       TEXT,
  path         TEXT,
  queued_at    TEXT NOT NULL
                 CHECK (queued_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9].[0-9][0-9][0-9]Z'),
  expires_at   TEXT NOT NULL
                 CHECK (expires_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9].[0-9][0-9][0-9]Z'),
  resolved_at  TEXT
                 CHECK (resolved_at IS NULL OR resolved_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9].[0-9][0-9][0-9]Z'),
  reason       TEXT
);

CREATE INDEX IF NOT EXISTS ix_wsw_intercept_state ON wsw_intercept(state, queued_at);
CREATE INDEX IF NOT EXISTS ix_wsw_intercept_session ON wsw_intercept(session_id, queued_at);
CREATE INDEX IF NOT EXISTS ix_wsw_intercept_listener ON wsw_intercept(listener_id, queued_at);

-- ---------------------------------------------------------------------------
-- schema_version：WSW-2 Proxy/MITM Runtime 持久化登记（version=24；链头
-- 023_wsw_body_blob.sql 推进 023→024）
-- ---------------------------------------------------------------------------
INSERT OR IGNORE INTO schema_version (version, name, description, applied_at, applied_by) VALUES
  (24, 'V1 additive WSW-2 proxy runtime listener/session/intercept tables (wsw_listener, wsw_session, wsw_intercept)',
   'WSW-2（docs/rip3/wsw/WSW-2-TASK.md §二/§三 S5 + WSW-2-ADR W2-D4/D6）：纯新增三表零 ALTER。wsw_listener 监听器实例六态登记（bind loopback 双保险 CHECK）；wsw_session 会话 scope 规则源（include/exclude/allow_private + out_of_scope_policy 词表）；wsw_intercept 拦截队列行式状态机（W2-D4「行即事实」：event_id 幂等键 UNIQUE + direction request/response + 五态 state 词表 + queued_at/expires_at TTL + resolved_at 终态时刻 + reason 错误五态可入；不单建事件流水表，wsw_exchange 元数据表留 WSW-3）。终态唯一性 = 应用面 UPDATE WHERE state=queued + 内存 PendingQueue 双面强制；重启对账（QUEUED 未过期回载/过期 EXPIRED）与停机全 EXPIRED（PLAN §7.5 :553）。只在开发库副本与测试内存库执行，platform*.db 生产文件不写',
   strftime('%Y-%m-%dT%H:%M:%SZ', 'now'), '024_wsw_intercept.sql');

-- user_version 双写（DEC-002 条件，001/018/022/023 先例）
PRAGMA user_version = 24;
