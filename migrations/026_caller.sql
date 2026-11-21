-- ============================================================================
-- Research Intelligence Platform : RIP3 MA-1 Caller 身份与多密钥鉴权
--   增量迁移（MA-1-TASK.md §0 D1/D2；MULTI-AGENT-ACCESS-PLAN.md §四 MA-1 行）：
--
--   纯新增两表，零 ALTER、零既有表改动（025 文件头 :5-6 纪律同源；批内若发现
--   确需 ALTER 既有表，另立 027+，禁改历史迁移）。
--
--   caller : 调用者身份登记（caller ≠ 用户账号，计划 §三：不建第二套事实源；
--            权限位挂 caller 级——key 是凭据不是权限主体）。caller 停用 =
--            revoked_at 非空，其名下 key 全部失效（校验时 join 判定，D1）。
--   api_key: API 密钥登记。key_hash = sha256(明文全文) hex 小写 64 位（D2：
--            明文熵 256bit，零 salt 免查全表，主键查询 O(1)）；明文形如
--            rp1_<43 位 base64url>（32 字节 /dev/urandom），只在 rp-admin
--            key-gen 输出一次，库中零明文。
--
--   列语义：
--   - caller_id：caller- 前缀（wsw-lis-/wsw-ses- ids.rs 范式同源，生成侧
--     rp-admin caller-add 承载）。
--   - permission_level：T0/T1/T2 词表 CHECK（001:289 Tool Policy 词表同源；
--     D8：本批是登记属性，写审计行对齐词表，不接授权判断——enforcement
--     留 MA-2/MA-3）。
--   - label：key 人读标签（可选）。
--   - last_used_at：使用时刻登记列，本批无写入方（server 校验路径对
--     api_key/caller 零写，D4 红线 3；消费面留 MA-2 预算/限流批）。
--   - created_at / revoked_at / last_used_at：ISO8601 UTC 带毫秒固定宽
--     GLOB（024/025 口径同源，字典序=时间序）；revoked_at NULL = 在用。
--
--   写权分治（D4/D6）：两表写入一律走 rp-admin CLI（caller-add/key-gen/
--   key-revoke）；rp_server.py 校验路径 mode=ro 直查零写（吊销即时生效，
--   无缓存每请求直查）；外部_api_audit 表（019）零改动。
--
--   幂等：CREATE TABLE IF NOT EXISTS + CREATE INDEX IF NOT EXISTS +
--   INSERT OR IGNORE（002/018/020/021/022/023/024/025 先例）。
--   只在开发库副本与测试内存库执行；migrations/platform*.db 生产文件不写。
-- ============================================================================

CREATE TABLE IF NOT EXISTS caller (
  caller_id        TEXT PRIMARY KEY
                     CHECK (caller_id LIKE 'caller-%'),
  display_name     TEXT NOT NULL CHECK (length(trim(display_name)) > 0),
  permission_level TEXT NOT NULL
                     CHECK (permission_level IN ('T0','T1','T2')),
  created_at       TEXT NOT NULL CHECK (created_at GLOB
                     '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9].[0-9][0-9][0-9]Z'),
  revoked_at       TEXT CHECK (revoked_at IS NULL OR revoked_at GLOB
                     '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9].[0-9][0-9][0-9]Z') -- NULL=在用
);

CREATE TABLE IF NOT EXISTS api_key (
  key_hash   TEXT PRIMARY KEY
               CHECK (length(key_hash) = 64 AND key_hash GLOB '[0-9a-f]*'), -- sha256(明文全文) hex 小写（D2）
  caller_id  TEXT NOT NULL REFERENCES caller(caller_id),
  label      TEXT,
  created_at TEXT NOT NULL CHECK (created_at GLOB
               '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9].[0-9][0-9][0-9]Z'),
  last_used_at TEXT CHECK (last_used_at IS NULL OR last_used_at GLOB
               '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9].[0-9][0-9][0-9]Z'), -- 登记列，本批无写入方（MA-2）
  revoked_at TEXT CHECK (revoked_at IS NULL OR revoked_at GLOB
               '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9].[0-9][0-9][0-9]Z') -- NULL=在用
);

CREATE INDEX IF NOT EXISTS ix_api_key_caller ON api_key(caller_id, created_at DESC);

-- ---------------------------------------------------------------------------
-- schema_version：MA-1 Caller 身份与多密钥鉴权持久化登记（version=26；链头
-- 025_wsw_exchange.sql 推进 025→026）
-- ---------------------------------------------------------------------------
INSERT OR IGNORE INTO schema_version (version, name, description, applied_at, applied_by) VALUES
  (26, 'V1 additive MA-1 caller identity and api_key tables (caller, api_key)',
   'MA-1（docs/rip3/MA-1-TASK.md §0 D1/D2/D4/D6 + MULTI-AGENT-ACCESS-PLAN.md §四 MA-1 行）：纯新增两表零 ALTER。caller 调用者身份登记（caller- 前缀 ID；display_name 非空；permission_level T0/T1/T2 词表 CHECK 登记属性——本批不接授权判断，enforcement 留 MA-2/MA-3；revoked_at NULL=在用，停用即名下 key 全失效，校验时 join 判定）；api_key 密钥登记（key_hash=sha256(明文全文) hex 64 位主键零 salt——明文 rp1_<43 位 base64url> 由 /dev/urandom 32 字节生成、仅 rp-admin key-gen 输出一次、库中零明文；caller_id FK→caller；last_used_at 登记列本批无写入方；label 可选）。写权分治（D4/D6）：写入一律走 rp-admin CLI，rp_server.py 校验路径 mode=ro 直查零写、吊销即时生效无缓存。legacy 单令牌（RP_AUTH_TOKEN/RP_AUTH_TOKEN_FILE）语义零变化，映射隐式 caller default（D3，库中无行不可吊销）。external_api_audit（019）零改动；per-caller 限流/并发/预算帽属 MA-2（D7）零实现。只在开发库副本与测试内存库执行，platform*.db 生产文件不写',
   strftime('%Y-%m-%dT%H:%M:%SZ', 'now'), '026_caller.sql');

-- user_version 双写（DEC-002 条件，001/018/022/023/024/025 先例）
PRAGMA user_version = 26;
