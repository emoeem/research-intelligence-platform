-- ============================================================================
-- Research Intelligence Platform : RIP3 WSW-5/6 WebSocket Workbench 三工具
--   增量迁移（WSW-5-6-TASK.md §0 D2 + §三 S2；PLAN §4.5 :274-283 帧面 /
--   §4.8 :305-311 被动面）：
--
--   纯新增两表 + 索引，零 ALTER、零既有表改动（028 文件头纪律同源；批内若
--   发现确需 ALTER 既有表，另立 031+，禁改历史迁移）。
--
--   wsw_ws_frame   : WS 帧元数据行（PLAN :283「不把 WebSocket 帧与普通 HTTP
--                    Request 混成同一种实体」——帧行不入 wsw_exchange，独立
--                    行式存储；payload 大内容不入行，行内只落 CAS BodyRef
--                    派生列 = 025 Body 列同型，D2）：
--                    frame_id UNIQUE 幂等键（wsw-fr- 前缀）+
--                    connection_exchange_id 引用 wsw_exchange 握手行
--                    （protocol='ws'，source='proxy_capture' 025 词形直用）+
--                    direction CHECK 两词表 + sequence_no（连接内单调，
--                    UNIQUE(connection_exchange_id, sequence_no) 幂等锚）+
--                    opcode CHECK 词表（PLAN :276/:678 text/binary/ping/pong/
--                    close 五词形）+ payload_len + payload CAS 四列 +
--                    masked（RFC6455 client→server 帧恒掩码、server→client
--                    恒不掩码——tungstenite 解码后掩码位不透出，由方向词形
--                    承载线缆层事实）+ captured_at + scope_decision 三态
--                    （沿用 wsw_exchange 同列语义，判据 8）。
--   wsw_passive_hit: 被动命中候选行（PLAN :307-309/:504/:565 候选语义）：
--                    零 evidence_id 列、零 Finding 提升面（判据 6 负例承载）；
--                    exchange_id 引用 wsw_exchange 行 + listener_id +
--                    rule_id CHECK 词表（passive.rs 注册表五类 14 规则逐字
--                    同源；WSW-7 Provider 接线禁第二注册表——判据 12）+
--                    rule_version + severity CHECK 四词表 + match_detail_json
--                    （8MiB 帽同 025 request_json 口径）+ created_at；
--                    UNIQUE(exchange_id, rule_id) 幂等锚（同 exchange 同规则
--                    单 hit——逐行分析重跑不重复落候选）。
--
--   写权分治（W3-D7 延续）：帧行/命中行 = rp-wsw listener / rp-wsw ws 子进程
--   Rust-only；rp_server 只做 spawn + 只读读面（D4 七端点）。
--
--   幂等：CREATE TABLE IF NOT EXISTS + CREATE INDEX IF NOT EXISTS +
--   INSERT OR IGNORE（002/018/020/021/022/023/024/025/028/029 先例）。
--   只在开发库副本与测试内存库执行；migrations/platform*.db 生产文件不写。
-- ============================================================================

CREATE TABLE IF NOT EXISTS wsw_ws_frame (
  frame_id     TEXT PRIMARY KEY
                 CHECK (frame_id LIKE 'wsw-fr-%'),
  connection_exchange_id TEXT NOT NULL REFERENCES wsw_exchange(exchange_id)
                 CHECK (connection_exchange_id LIKE 'wsw-%'),
  direction    TEXT NOT NULL CHECK (direction IN
               ('client_to_server','server_to_client')),
  sequence_no  INTEGER NOT NULL CHECK (sequence_no >= 0),
  opcode       TEXT NOT NULL CHECK (opcode IN
               ('text','binary','ping','pong','close')),
  payload_len  INTEGER NOT NULL CHECK (payload_len >= 0),
  -- CAS BodyRef 四派生列（025 Body 列同型；空帧/超帽/截断 = NULL + fidelity 标注）。
  payload_body_sha256 TEXT
                 CHECK (payload_body_sha256 IS NULL OR
                        (length(payload_body_sha256) = 64 AND payload_body_sha256 GLOB '[0-9a-f]*')),
  payload_body_uri TEXT
                 CHECK (payload_body_uri IS NULL OR payload_body_uri LIKE 'wsw/%'),
  payload_body_len INTEGER CHECK (payload_body_len IS NULL OR payload_body_len >= 0),
  payload_body_fidelity TEXT
                 CHECK (payload_body_fidelity IS NULL OR payload_body_fidelity IN ('full','truncated')),
  -- 线缆掩码面（方向词形承载：client_to_server=1 / server_to_client=0）。
  masked       INTEGER NOT NULL CHECK (masked IN (0,1)),
  captured_at  TEXT NOT NULL CHECK (captured_at GLOB
               '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9].[0-9][0-9][0-9]Z'),
  scope_decision TEXT
                 CHECK (scope_decision IS NULL OR scope_decision IN
                        ('in_scope','out_of_scope_passive','out_of_scope_blocked')),
  UNIQUE(connection_exchange_id, sequence_no)
);

CREATE INDEX IF NOT EXISTS ix_wsw_ws_frame_conn
  ON wsw_ws_frame(connection_exchange_id, sequence_no);
CREATE INDEX IF NOT EXISTS ix_wsw_ws_frame_conn_dir
  ON wsw_ws_frame(connection_exchange_id, direction, sequence_no);
CREATE INDEX IF NOT EXISTS ix_wsw_ws_frame_opcode
  ON wsw_ws_frame(opcode, captured_at DESC);

CREATE TABLE IF NOT EXISTS wsw_passive_hit (
  hit_id      TEXT PRIMARY KEY
                 CHECK (hit_id LIKE 'wsw-pah-%'),
  exchange_id TEXT NOT NULL REFERENCES wsw_exchange(exchange_id)
                 CHECK (exchange_id LIKE 'wsw-%'),
  listener_id TEXT NOT NULL CHECK (listener_id LIKE 'wsw-lis-%'),
  -- 规则词表（crates/wsw-runtime/src/passive.rs 注册表五类 14 规则逐字同源；
  -- 候选态零 evidence_id 零 Finding 提升——PLAN :309/:504）。
  rule_id     TEXT NOT NULL CHECK (rule_id IN
               ('missing_header_strict_transport_security',
                'missing_header_content_security_policy',
                'missing_header_x_content_type_options',
                'missing_header_x_frame_options',
                'cookie_missing_secure',
                'cookie_missing_httponly',
                'cookie_missing_samesite',
                'info_leak_server_version',
                'info_leak_x_powered_by',
                'info_leak_via_header',
                'redirect_cross_origin',
                'redirect_scheme_downgrade',
                'config_permissive_cors',
                'config_set_cookie_cacheable')),
  rule_version INTEGER NOT NULL CHECK (rule_version >= 0),
  severity    TEXT NOT NULL CHECK (severity IN ('info','low','medium','high')),
  match_detail_json TEXT
                 CHECK (match_detail_json IS NULL OR length(match_detail_json) <= 8*1024*1024),
  created_at  TEXT NOT NULL CHECK (created_at GLOB
               '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9].[0-9][0-9][0-9]Z'),
  UNIQUE(exchange_id, rule_id)
);

CREATE INDEX IF NOT EXISTS ix_wsw_passive_hit_exchange
  ON wsw_passive_hit(exchange_id, rule_id);
CREATE INDEX IF NOT EXISTS ix_wsw_passive_hit_severity
  ON wsw_passive_hit(severity, created_at DESC);
CREATE INDEX IF NOT EXISTS ix_wsw_passive_hit_rule
  ON wsw_passive_hit(rule_id, created_at DESC);

-- ---------------------------------------------------------------------------
-- schema_version：WSW-5/6 持久化登记（version=30；链头 029→030 单文件
-- 单版本推进，D2；编号冻结规则顺延——migration_dev_copy_p7_3.rs:76 注释原文）
-- ---------------------------------------------------------------------------
INSERT OR IGNORE INTO schema_version (version, name, description, applied_at, applied_by) VALUES
  (30, 'V1 additive WSW-5/6 ws frame + passive hit tables (wsw_ws_frame, wsw_passive_hit)',
   'WSW-5/6（docs/rip3/wsw/WSW-5-6-TASK.md §0 D2 + §三 S2）：纯新增两表零 ALTER。wsw_ws_frame 帧元数据行（PLAN :283 不混实体——帧行不入 wsw_exchange）：frame_id UNIQUE 幂等键（wsw-fr- 前缀）+ connection_exchange_id 引用 wsw_exchange 握手行（protocol=ws，source=proxy_capture 025 词形直用）+ direction 两词表 CHECK + sequence_no 连接内单调（UNIQUE(connection_exchange_id,sequence_no) 幂等锚）+ opcode 五词表 CHECK（PLAN :276/:678 text/binary/ping/pong/close）+ payload_len + payload CAS 四派生列（025 Body 列同型）+ masked（RFC6455 方向词形承载）+ captured_at + scope_decision 三态沿用 wsw_exchange 同列语义。wsw_passive_hit 被动命中候选行（PLAN :307-309/:504/:565 候选语义，零 evidence_id 列零 Finding 提升）：exchange_id 引用 + listener_id + rule_id CHECK 词表（passive.rs 注册表五类 14 规则逐字同源，WSW-7 Provider 禁第二注册表）+ rule_version + severity 四词表 CHECK + match_detail_json 8MiB 帽 + created_at + UNIQUE(exchange_id,rule_id) 幂等锚。写权 = rp-wsw listener/ws 子进程 Rust-only（W3-D7 分治延续）。只在开发库副本与测试内存库执行，platform*.db 生产文件不写',
   strftime('%Y-%m-%dT%H:%M:%SZ', 'now'), '030_wsw_ws_frame.sql');

-- user_version 双写（DEC-002 条件，001/018/022/023/024/025/028 先例）
PRAGMA user_version = 30;
