-- ============================================================================
-- Research Intelligence Platform : RIP3 WSW-3 HTTP History + Repeater
--   增量迁移（WSW-3-TASK.md §二/§三 S2；WSW-3-ADR W3-D1/D3/D4/D7；DESIGN §3.2）：
--
--   纯新增四表，零 ALTER、零既有表改动（024 文件头 :5-6 纪律同源；批内若发现
--   确需 ALTER 既有表，另立 026+，禁改历史迁移）。
--
--   wsw_exchange          : 捕获/发送交换行（单行纪律：一 exchange 一行，请求+
--                           响应同行；响应未到 = 响应列 NULL + parse_status=
--                           'partial'。Body 大内容不入行——行内只落 CAS BodyRef
--                           派生列 sha256/uri/len/mime/fidelity，字节读回走
--                           023 wsw_body_blob + cas.rs read_body 按需端点）。
--   wsw_repeater_tab      : Repeater 多 Tab 登记行（closed_at NULL=打开）。
--   wsw_repeater_tab_version : Tab 请求编辑版本（append-only：禁 UPDATE 禁
--                           DELETE——每编辑一次 = 新 version_no 行；重放面
--                           restore N = 新增 origin='restore' 行；除 retention
--                           批 GC 执行位外无任何 UPDATE/DELETE 路径）。
--   wsw_repeater_send     : 发送历史行式状态机（PLAN §6.4 :501 六态词表；
--                           request_outbound_sha256 = OutboundBytes CAS 引用，
--                           逐字节重发唯一依据，W3-D4/D6）。
--
--   列语义（词表 CHECK 逐列，DESIGN §1.2 Exchange 字段冻结表逐行映射）：
--   - ID 形状：exchange_id = wsw- 前缀（listener/handler.rs new_exchange_id
--     生成面）、tab_id = wsw-rpt-、send_id = wsw-snd-（沿 ids.rs 范式生成侧
--     承载）、session_id = wsw-ses-、listener_id = wsw-lis-。
--   - direction/source/protocol/scope_decision/capture_fidelity/parse_status：
--     词表 CHECK（exchange.rs 词表消费；scope_decision NULL=Unknown 第三态）。
--   - started_at/completed_at：ISO8601 UTC 带毫秒固定宽 GLOB（024 口径同源，
--     字典序=时间序 → 游标排序键 (started_at DESC, exchange_id DESC) 稳定）。
--   - sequence_no：listener 内单调序号，UNIQUE(listener_id, sequence_no)
--     对账面；游标默认排序键不用它（W3-D3）。
--   - search_text：D7 有界搜索投影（authority+path+头名/头值+体前 4KiB，
--     总帽 8KiB，insert 侧生成；凭据头值排除——DESIGN §4.5）。
--   - request_json（tab_version）：OutboundRequest serde JSON（体 base64
--     内联，8MiB 帽超帽拒——与 parse/serialize max_body_bytes 同源）。
--
--   写权分治（W3-D7）：wsw_exchange 行（capture 通道）= Rust listener worker
--   批量事务；wsw_exchange 行（send 通道）+ wsw_repeater_send 行 = rp-wsw
--   send 子命令；tags/notes PATCH 与 tab/tab_version 小控制行 = rp_server
--   Python 直写（busy_timeout + WAL）。读面全部 mode=ro。
--
--   幂等：CREATE TABLE IF NOT EXISTS + CREATE INDEX IF NOT EXISTS +
--   INSERT OR IGNORE（002/018/020/021/022/023/024 先例）。
--   只在开发库副本与测试内存库执行；migrations/platform*.db 生产文件不写。
-- ============================================================================

CREATE TABLE IF NOT EXISTS wsw_exchange (
  exchange_id  TEXT PRIMARY KEY
                 CHECK (exchange_id LIKE 'wsw-%'),
  session_id   TEXT NOT NULL CHECK (session_id LIKE 'wsw-ses-%'),
  listener_id  TEXT NOT NULL CHECK (listener_id LIKE 'wsw-lis-%'),
  sequence_no  INTEGER NOT NULL CHECK (sequence_no >= 0),
  direction    TEXT NOT NULL CHECK (direction IN ('client_to_server','server_to_client')),
  source       TEXT NOT NULL CHECK (source IN ('proxy_capture','repeater','fuzzer','import')),
  protocol     TEXT NOT NULL CHECK (protocol IN ('http','ws')), -- exchange.rs:90 词表
  http_version TEXT,
  tls_version  TEXT,
  alpn         TEXT,
  method       TEXT,
  scheme       TEXT,
  authority    TEXT,
  path         TEXT,
  response_status INTEGER,
  request_headers_json  TEXT, -- Parsed 头序保持投影（detail 端点专用，列表不选）
  response_headers_json TEXT,
  -- Body 大内容不入行：只落 CAS BodyRef 派生列（cas.rs:34-46 sha256/uri/size/fidelity）
  request_body_sha256 TEXT CHECK (request_body_sha256 IS NULL OR
                        (length(request_body_sha256) = 64 AND request_body_sha256 GLOB '[0-9a-f]*')),
  request_body_uri    TEXT,
  request_body_len    INTEGER CHECK (request_body_len IS NULL OR request_body_len >= 0),
  request_mime        TEXT,
  request_body_fidelity TEXT CHECK (request_body_fidelity IS NULL OR
                          request_body_fidelity IN ('full','truncated')),
  response_body_sha256 TEXT CHECK (response_body_sha256 IS NULL OR
                         (length(response_body_sha256) = 64 AND response_body_sha256 GLOB '[0-9a-f]*')),
  response_body_uri    TEXT,
  response_body_len    INTEGER CHECK (response_body_len IS NULL OR response_body_len >= 0),
  response_mime        TEXT,
  response_body_fidelity TEXT CHECK (response_body_fidelity IS NULL OR
                           response_body_fidelity IN ('full','truncated')),
  started_at   TEXT CHECK (started_at IS NULL OR started_at GLOB
                 '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9].[0-9][0-9][0-9]Z'),
  completed_at TEXT CHECK (completed_at IS NULL OR completed_at GLOB
                 '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9].[0-9][0-9][0-9]Z'),
  duration_ms  INTEGER CHECK (duration_ms IS NULL OR duration_ms >= 0),
  ttfb_ms      INTEGER CHECK (ttfb_ms IS NULL OR ttfb_ms >= 0),
  remote_ip    TEXT,
  scope_decision TEXT CHECK (scope_decision IS NULL OR scope_decision IN
                 ('in_scope','out_of_scope_passive','out_of_scope_blocked')), -- NULL=Unknown 第三态
  edited       INTEGER NOT NULL DEFAULT 0 CHECK (edited IN (0,1)),
  tags_json    TEXT NOT NULL DEFAULT '[]',
  notes        TEXT,
  trace_id     TEXT, task_id TEXT, tool_run_id TEXT, artifact_id TEXT, evidence_id TEXT, -- 六 ID 引用列
  capture_fidelity TEXT NOT NULL CHECK (capture_fidelity IN ('full','truncated','degraded_no_wire','import')),
  parse_status TEXT NOT NULL CHECK (parse_status IN ('ok','partial','failed')),
  error_code   TEXT,
  search_text  TEXT
);

CREATE UNIQUE INDEX IF NOT EXISTS ux_wsw_exchange_listener_seq
  ON wsw_exchange(listener_id, sequence_no);
CREATE INDEX IF NOT EXISTS ix_wsw_exchange_cursor
  ON wsw_exchange(session_id, started_at DESC, exchange_id DESC); -- 游标主序（W3-D3）
CREATE INDEX IF NOT EXISTS ix_wsw_exchange_authority
  ON wsw_exchange(authority, started_at DESC);                    -- 域名过滤
CREATE INDEX IF NOT EXISTS ix_wsw_exchange_method
  ON wsw_exchange(method);
CREATE INDEX IF NOT EXISTS ix_wsw_exchange_status
  ON wsw_exchange(response_status);
CREATE INDEX IF NOT EXISTS ix_wsw_exchange_source
  ON wsw_exchange(source, started_at DESC);                       -- 来源过滤
CREATE INDEX IF NOT EXISTS ix_wsw_exchange_started
  ON wsw_exchange(started_at DESC, exchange_id DESC);             -- 无 session 全局面游标

CREATE TABLE IF NOT EXISTS wsw_repeater_tab (
  tab_id     TEXT PRIMARY KEY CHECK (tab_id LIKE 'wsw-rpt-%'),
  session_id TEXT NOT NULL CHECK (session_id LIKE 'wsw-ses-%'),
  title      TEXT NOT NULL CHECK (length(trim(title)) > 0),
  created_at TEXT NOT NULL CHECK (created_at GLOB
               '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9].[0-9][0-9][0-9]Z'),
  updated_at TEXT NOT NULL CHECK (updated_at GLOB
               '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9].[0-9][0-9][0-9]Z'),
  closed_at  TEXT CHECK (closed_at IS NULL OR closed_at GLOB
               '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9].[0-9][0-9][0-9]Z') -- NULL=打开
);

CREATE TABLE IF NOT EXISTS wsw_repeater_tab_version (
  tab_id             TEXT NOT NULL REFERENCES wsw_repeater_tab(tab_id),
  version_no         INTEGER NOT NULL CHECK (version_no >= 1),
  request_json       TEXT NOT NULL CHECK (length(request_json) <= 8388608), -- OutboundRequest serde JSON
  source_exchange_id TEXT CHECK (source_exchange_id IS NULL OR source_exchange_id LIKE 'wsw-%'),
  origin             TEXT NOT NULL CHECK (origin IN ('manual','from_history','restore','send_snapshot')),
  created_at         TEXT NOT NULL CHECK (created_at GLOB
                       '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9].[0-9][0-9][0-9]Z'),
  UNIQUE(tab_id, version_no) -- append-only 锚：同 (tab_id, version_no) 不可二写
);

CREATE INDEX IF NOT EXISTS ix_wsw_repeater_version
  ON wsw_repeater_tab_version(tab_id, version_no DESC);

CREATE TABLE IF NOT EXISTS wsw_repeater_send (
  send_id      TEXT PRIMARY KEY CHECK (send_id LIKE 'wsw-snd-%'),
  tab_id       TEXT NOT NULL,
  version_no   INTEGER NOT NULL,
  session_id   TEXT NOT NULL CHECK (session_id LIKE 'wsw-ses-%'),
  exchange_id  TEXT CHECK (exchange_id IS NULL OR exchange_id LIKE 'wsw-%'), -- 发送产出 exchange（source=repeater）
  state        TEXT NOT NULL CHECK (state IN
               ('pending','sending','succeeded','failed','cancelled','timed_out')), -- PLAN §6.4 :501 词表
  request_outbound_sha256 TEXT CHECK (request_outbound_sha256 IS NULL OR
               (length(request_outbound_sha256) = 64 AND request_outbound_sha256 GLOB '[0-9a-f]*')),
  redirect_chain_json TEXT,
  response_status       INTEGER,
  response_body_sha256  TEXT,
  response_body_len     INTEGER,
  response_mime         TEXT,
  remote_ip    TEXT,
  started_at   TEXT,
  completed_at TEXT,
  duration_ms  INTEGER CHECK (duration_ms IS NULL OR duration_ms >= 0),
  ttfb_ms      INTEGER CHECK (ttfb_ms IS NULL OR ttfb_ms >= 0),
  error_code   TEXT,
  created_at   TEXT NOT NULL CHECK (created_at GLOB
               '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9].[0-9][0-9][0-9]Z')
);

CREATE INDEX IF NOT EXISTS ix_wsw_repeater_send_tab
  ON wsw_repeater_send(tab_id, created_at DESC);
CREATE INDEX IF NOT EXISTS ix_wsw_repeater_send_session
  ON wsw_repeater_send(session_id, created_at DESC);

-- ---------------------------------------------------------------------------
-- schema_version：WSW-3 History+Repeater 持久化登记（version=25；链头
-- 024_wsw_intercept.sql 推进 024→025）
-- ---------------------------------------------------------------------------
INSERT OR IGNORE INTO schema_version (version, name, description, applied_at, applied_by) VALUES
  (25, 'V1 additive WSW-3 exchange/repeater tables (wsw_exchange, wsw_repeater_tab, wsw_repeater_tab_version, wsw_repeater_send)',
   'WSW-3（docs/rip3/wsw/WSW-3-TASK.md §二/§三 S2 + WSW-3-ADR W3-D1/D3/D4/D7）：纯新增四表零 ALTER。wsw_exchange 单行纪律（一 exchange 一行，请求+响应同行，响应未到=响应列 NULL+parse_status=partial；Body 大内容不入行只落 CAS BodyRef 派生列 sha256/uri/len/mime/fidelity；capture_fidelity 四词表含 degraded_no_wire 如实落库；scope_decision NULL=Unknown 第三态；search_text 有界搜索投影 8KiB 帽凭据头值排除）；wsw_repeater_tab 多 Tab 登记（closed_at NULL=打开）；wsw_repeater_tab_version append-only 版本（UNIQUE(tab_id,version_no)，origin 四词表 manual/from_history/restore/send_snapshot，request_json 8MiB 帽）；wsw_repeater_send 行式六态状态机（PLAN §6.4 :501 词表，request_outbound_sha256=OutboundBytes CAS 逐字节重发依据）。游标分页排序键 (started_at DESC, exchange_id DESC) 复合索引；写权分治 = exchange/send 行 Rust-only（listener worker 批量 + rp-wsw send），tags/notes/tab/tab_version Python 直写（W3-D7）。只在开发库副本与测试内存库执行，platform*.db 生产文件不写',
   strftime('%Y-%m-%dT%H:%M:%SZ', 'now'), '025_wsw_exchange.sql');

-- user_version 双写（DEC-002 条件，001/018/022/023/024 先例）
PRAGMA user_version = 25;
